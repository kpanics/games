//! People: how a member is shown everywhere (login, UID, avatar, prefix, status line), the roles behind the prefix
//! and what each lets its holder do, the profile they dress up (about, status, accent and backdrop, banner, music,
//! links, privacy, the badges they show, their screenshots), the badges themselves, and the sessions they are signed
//! in with. Staff give roles and badges here, and every such act goes into the journal (`audit`).
//!
//! The card — what lists of people show — is kept in memory for every member (a forum page of forty posts asks for
//! forty cards), and made again whenever its member, their roles or their profile change.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path as Key, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::market::{busy, clean_line, member, run, said};
use crate::{db, fail, now, Shared, OWNER};

// ------------------------------------------------------------------------------------------------ roles

pub const STAFF_PANEL: &str = "staff.panel";
pub const USERS_VIEW: &str = "users.view";
pub const USERS_BAN: &str = "users.ban";
pub const USERS_MUTE: &str = "users.mute";
pub const USERS_DEVICE: &str = "users.device";
pub const FORUM_MODERATE: &str = "forum.moderate";
pub const REPORTS: &str = "reports.handle";
pub const TICKETS: &str = "tickets.handle";
pub const MEDIA_MODERATE: &str = "media.moderate";
pub const MARKET_MODERATE: &str = "market.moderate";
pub const MESSAGES_REPORTS: &str = "messages.reports";
pub const BADGES_GIVE: &str = "badges.give";
pub const ANNOUNCE: &str = "announce";
pub const AUDIT: &str = "audit.view";
/// Referral links (refs.rs): one's own, and everyone's.
pub const REFS_OWN: &str = "refs.own";
pub const REFS_ALL: &str = "refs.all";
/// Recode's keys (recode.rs): one's own batches made, listed and revoked, and Recode given by hand.
pub const RECODE_KEYS: &str = "recode.keys";
pub const RECODE_REVOKE: &str = "recode.revoke_all";
/// Every right a role may give, in the order the staff's panel shows them.
pub const PERMS: &[&str] = &[STAFF_PANEL, USERS_VIEW, USERS_BAN, USERS_MUTE, USERS_DEVICE, FORUM_MODERATE, REPORTS, TICKETS, MEDIA_MODERATE, MARKET_MODERATE, MESSAGES_REPORTS, BADGES_GIVE, ANNOUNCE, AUDIT, REFS_OWN, REFS_ALL, RECODE_KEYS, RECODE_REVOKE];

pub struct Role {
    pub id: &'static str,
    pub en: &'static str,
    pub ru: &'static str,
    pub prefix: &'static str,
    pub colour: &'static str,
    pub icon: &'static str,
    pub rank: u8,
    pub staff: bool,
    /// Shown as the prefix without being chosen (the beta's role is everyone's while the beta lasts: theirs to show).
    pub auto: bool,
    pub perms: &'static [&'static str],
}

/// The roles, highest first. The prefix shown is the highest a member holds, unless they choose another of theirs
/// (or none). "beta" is the role of everyone who may play (beta.rs gives it at sign-up and lets the loader start with
/// it): the testers' of the beta until it ended (10.10, her word: Cursed is out), shown as the users' since.
pub const ROLES: &[Role] = &[
    Role { id: "creator", en: "Creator", ru: "Создатель", prefix: "CREATOR", colour: "#F2F1CB", icon: "c_crown", rank: 100, staff: true, auto: true, perms: &["*"] },
    Role {
        id: "admin",
        en: "Admin",
        ru: "Админ",
        prefix: "ADMIN",
        colour: "#E5484D",
        icon: "c_shield",
        rank: 90,
        staff: true,
        auto: true,
        perms: &[STAFF_PANEL, USERS_VIEW, USERS_BAN, USERS_MUTE, USERS_DEVICE, FORUM_MODERATE, REPORTS, TICKETS, MEDIA_MODERATE, MARKET_MODERATE, MESSAGES_REPORTS, BADGES_GIVE, ANNOUNCE, AUDIT, REFS_OWN, REFS_ALL],
    },
    Role {
        id: "moderator",
        en: "Moderator",
        ru: "Модератор",
        prefix: "MOD",
        colour: "#6669F5",
        icon: "c_gavel",
        rank: 70,
        staff: true,
        auto: true,
        perms: &[STAFF_PANEL, USERS_VIEW, USERS_MUTE, FORUM_MODERATE, REPORTS, MEDIA_MODERATE, MARKET_MODERATE, MESSAGES_REPORTS],
    },
    Role { id: "support", en: "Support", ru: "Саппорт", prefix: "SUPPORT", colour: "#4FC3B5", icon: "c_chat", rank: 60, staff: true, auto: true, perms: &[STAFF_PANEL, USERS_VIEW, USERS_DEVICE, TICKETS] },
    // The client's neural network answering on the forum while she is away (her word, 08.10): it says it is one. It
    // moderates the forum and nothing else, and is not staff (no messages to whoever it likes).
    Role { id: "ai", en: "AI", ru: "Нейросеть", prefix: "AI", colour: "#D6D7F5", icon: "c_portal", rank: 50, staff: false, auto: true, perms: &[FORUM_MODERATE, REPORTS] },
    Role { id: "media", en: "Media", ru: "Медиа", prefix: "MEDIA", colour: "#F0629B", icon: "c_play", rank: 40, staff: false, auto: true, perms: &[REFS_OWN] },
    Role { id: "partner", en: "Partner", ru: "Партнёр", prefix: "PARTNER", colour: "#E8B04B", icon: "c_team", rank: 35, staff: false, auto: true, perms: &[REFS_OWN] },
    // Recode's keys made by someone besides her (her word, 23.10.2026, mentos the first): their own batches, ten keys at
    // a time, and Recode given by hand. She alone gives it (KEYS_ROLE); not shown as a prefix unless chosen.
    Role { id: "keys", en: "Keys", ru: "Ключи", prefix: "KEYS", colour: "#7FD1AE", icon: "c_lock", rank: 32, staff: false, auto: false, perms: &[RECODE_KEYS] },
    // Recode's two (her word, 19.10): the first seventy to buy it in its beta, for good; everyone whose Recode is on,
    // while it is. recode.rs `marks` gives and takes them; by hand only she does (RECODE_ROLES).
    Role { id: "subscription_revoke", en: "Subscription revocation", ru: "Отзыв подписок", prefix: "REVOKE", colour: "#E5484D", icon: "c_shield", rank: 31, staff: false, auto: false, perms: &[RECODE_REVOKE] },
    Role { id: "rc_beta", en: "Beta tester", ru: "Бета-тестер", prefix: "BETA", colour: "#A78BFA", icon: "c_rocket", rank: 30, staff: false, auto: true, perms: &[] },
    Role { id: "recode", en: "Recode", ru: "Recode", prefix: "RECODE", colour: "#F2F2F7", icon: "c_portal", rank: 25, staff: false, auto: true, perms: &[] },
    Role { id: "beta", en: "User", ru: "Пользователь", prefix: "USER", colour: "#BFC0E7", icon: "c_wings", rank: 20, staff: false, auto: false, perms: &[] },
];

/// The roles and badges that follow a Recode bought or given: nobody but her gives them by hand (an admin who could
/// would hand out what is paid for).
pub const RECODE_ROLES: &[&str] = &["rc_beta", "recode"];
pub const RECODE_BADGES: &[&str] = &["rc_beta", "recode", "forever"];
/// The role that makes Recode's keys: she alone gives it (her word, 23.10.2026).
pub const KEYS_ROLE: &str = "keys";

pub fn role(id: &str) -> Option<&'static Role> {
    ROLES.iter().find(|r| r.id == id)
}

// ------------------------------------------------------------------------------------------------ cards

#[derive(Clone, Default)]
struct Card {
    login: String,
    /// The text UID shown instead of the number ("" when none).
    uid_text: String,
    roles: Vec<String>,
    prefix: String,
    status_text: String,
    status_emoji: String,
    accent: String,
    show_online: bool,
    show_listening: bool,
    show_server: bool,
    /// When it was made (a card older than a minute is made again on its member's next request).
    at: i64,
}

static CARDS: OnceLock<Mutex<HashMap<i64, Card>>> = OnceLock::new();

fn cards() -> MutexGuard<'static, HashMap<i64, Card>> {
    CARDS.get_or_init(|| Mutex::new(HashMap::new())).lock().unwrap_or_else(|e| e.into_inner())
}

/// A member's card made again from the database (after any change to them).
pub fn refresh(c: &Connection, uid: i64) {
    let Some((login, uid_text)) = c
        .query_row("SELECT login, uid_text FROM users WHERE uid = ?1", params![uid], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)))
        .optional()
        .ok()
        .flatten()
    else {
        cards().remove(&uid);
        return;
    };
    let mut roles: Vec<String> = c
        .prepare("SELECT role FROM roles WHERE uid = ?1")
        .and_then(|mut q| q.query_map(params![uid], |r| r.get::<_, String>(0)).map(|m| m.flatten().collect()))
        .unwrap_or_default();
    if crate::is_owner(uid) && !roles.iter().any(|r| r == "creator") {
        roles.insert(0, "creator".into());
    }
    roles.retain(|r| role(r).is_some());
    roles.sort_by_key(|r| std::cmp::Reverse(role(r).map_or(0, |x| x.rank)));
    let p = profile_row(c, uid);
    cards().insert(
        uid,
        Card {
            login,
            uid_text: uid_text.unwrap_or_default(),
            roles,
            prefix: p.prefix,
            status_text: p.status_text,
            status_emoji: p.status_emoji,
            accent: p.accent,
            show_online: p.privacy.get("online").and_then(Value::as_bool).unwrap_or(true),
            show_listening: p.privacy.get("listening").and_then(Value::as_bool).unwrap_or(true),
            show_server: p.privacy.get("server").and_then(Value::as_bool).unwrap_or(false),
            at: now(),
        },
    );
}

/// A member's card made again when this process has none, or an old one, or one of another login (the command line
/// gives roles and numbers in another process).
pub fn ensure(c: &Connection, uid: i64, login: &str) {
    let stale = cards().get(&uid).map_or(true, |k| k.login != login || now() - k.at > 60);
    if stale {
        refresh(c, uid);
    }
}

/// Every member's card (at the start, and every few minutes).
pub fn refresh_all(c: &Connection) {
    let uids: Vec<i64> = c.prepare("SELECT uid FROM users").and_then(|mut q| q.query_map([], |r| r.get(0)).map(|m| m.flatten().collect())).unwrap_or_default();
    for uid in uids {
        refresh(c, uid);
    }
}

pub fn roles_of(uid: i64) -> Vec<String> {
    let mut r = cards().get(&uid).map(|c| c.roles.clone()).unwrap_or_default();
    if crate::is_owner(uid) && !r.iter().any(|x| x == "creator") {
        r.insert(0, "creator".into());
    }
    r
}

/// Whether a member may do a thing (she may do all).
pub fn can(uid: i64, perm: &str) -> bool {
    crate::is_owner(uid) || roles_of(uid).iter().filter_map(|r| role(r)).any(|r| r.perms.contains(&"*") || r.perms.contains(&perm))
}

pub fn is_staff(uid: i64) -> bool {
    crate::is_owner(uid) || roles_of(uid).iter().filter_map(|r| role(r)).any(|r| r.staff)
}

fn shown_prefix(card: &Card) -> Option<&'static Role> {
    match card.prefix.as_str() {
        "none" => None,
        "" | "auto" => card.roles.iter().filter_map(|r| role(r)).filter(|r| r.auto).max_by_key(|r| r.rank),
        chosen => card.roles.iter().filter(|r| *r == chosen).find_map(|r| role(r)).or_else(|| card.roles.iter().filter_map(|r| role(r)).filter(|r| r.auto).max_by_key(|r| r.rank)),
    }
}

fn card_json(uid: i64, card: &Card) -> Value {
    let prefix = shown_prefix(card).map(|r| json!({ "role": r.id, "text": r.prefix, "colour": r.colour, "icon": r.icon, "en": r.en, "ru": r.ru }));
    let status = (!card.status_text.is_empty() || !card.status_emoji.is_empty()).then(|| json!({ "text": card.status_text, "emoji": card.status_emoji }));
    let online = if card.show_online {
        crate::live::presence(uid).map(|p| if p.game > 0 { "game" } else { "site" })
    } else {
        None
    };
    json!({
        "uid": uid, "uid_text": (!card.uid_text.is_empty()).then_some(&card.uid_text), "login": card.login, "avatar": crate::avatar::of(uid),
        "prefix": prefix, "status": status,
        "accent": (!card.accent.is_empty()).then_some(&card.accent),
        "staff": card.roles.iter().filter_map(|r| role(r)).any(|r| r.staff),
        // (Clients up to build 7 mark "moderator" as Cursed's own: hers alone, as it was; the right to moderate is "mod".)
        "moderator": crate::is_owner(uid), "mod": can(uid, FORUM_MODERATE), "online": online,
    })
}

/// How a member is shown in any list, by their UID and the login the caller already has.
pub fn card(uid: i64, login: String) -> Value {
    let found = cards().get(&uid).cloned();
    match found {
        Some(c) => card_json(uid, &c),
        None => card_json(uid, &Card { login, show_online: true, show_listening: true, ..Default::default() }),
    }
}

/// The same when the caller has only the UID.
pub fn card_of(c: &Connection, uid: i64) -> Value {
    if !cards().contains_key(&uid) {
        refresh(c, uid);
    }
    let found = cards().get(&uid).cloned();
    found.map_or(Value::Null, |k| card_json(uid, &k))
}

// ------------------------------------------------------------------------------------------------ text UIDs

/// A text UID: 2 to 16 Latin letters, digits, `_`, `.`, `-`, never read as somebody's number: a letter among them, or
/// a mark between two digits ("1000-7"); all digits, "-5" or "5." would read as one.
pub fn text_uid_ok(text: &str) -> bool {
    let ch: Vec<char> = text.chars().collect();
    let joined = ch.windows(3).any(|w| w[0].is_ascii_digit() && matches!(w[1], '_' | '.' | '-') && w[2].is_ascii_digit());
    (2..=16).contains(&ch.len())
        && ch.iter().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
        && (ch.iter().any(|c| c.is_ascii_alphabetic()) || joined)
}

/// Gives a member the text UID `text`, or the number back when it is empty. Err: "format", "taken" or "busy".
pub fn set_text_uid(c: &Connection, uid: i64, text: &str) -> Result<(), &'static str> {
    let text = text.trim();
    if text.is_empty() {
        c.execute("UPDATE users SET uid_text = NULL WHERE uid = ?1", params![uid]).map_err(|_| "busy")?;
    } else {
        if !text_uid_ok(text) {
            return Err("format");
        }
        let taken: bool = c
            .query_row("SELECT EXISTS(SELECT 1 FROM users WHERE lower(uid_text) = lower(?1) AND uid != ?2)", params![text, uid], |r| r.get(0))
            .map_err(|_| "busy")?;
        if taken {
            return Err("taken");
        }
        c.execute("UPDATE users SET uid_text = ?2 WHERE uid = ?1", params![uid, text]).map_err(|_| "busy")?;
    }
    refresh(c, uid);
    Ok(())
}

/// What a refused text UID says.
pub fn text_uid_refused(e: &str) -> Response {
    match e {
        "taken" => said(StatusCode::CONFLICT, "taken", "this UID is taken"),
        "format" => said(StatusCode::BAD_REQUEST, "format", "2 to 16 Latin letters, digits, _ . -, with a letter"),
        _ => busy(),
    }
}

/// Whether a member may give themselves a text UID: the admins, and she.
pub fn may_text_uid(uid: i64) -> bool {
    crate::is_owner(uid) || roles_of(uid).iter().any(|r| r == "admin")
}

#[derive(Deserialize)]
struct TextUid {
    text: String,
}

/// An admin's own text UID (empty: the number again).
async fn set_own_uid(State(app): State<Shared>, h: HeaderMap, Json(b): Json<TextUid>) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        if !may_text_uid(w.uid) {
            return Err(fail(StatusCode::FORBIDDEN, "forbidden"));
        }
        let uid = w.uid;
        let text = b.text.trim().to_string();
        let shown = text.clone();
        db(&app, move |c| -> Result<(), Response> {
            set_text_uid(c, uid, &text).map_err(text_uid_refused)?;
            audit(c, uid, "user.uid_text", &uid.to_string(), json!({ "text": text }));
            Ok(())
        })
        .await?;
        Ok(Json(json!({ "uid_text": (!shown.is_empty()).then_some(shown) })).into_response())
    })
    .await
}

pub fn uid_of(c: &Connection, login: &str) -> Option<i64> {
    c.query_row("SELECT uid FROM users WHERE login_key = ?1", params![login.to_lowercase()], |r| r.get(0)).optional().ok().flatten()
}

// ------------------------------------------------------------------------------------------------ schema

pub fn schema(c: &Connection) {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS roles (uid INTEGER NOT NULL REFERENCES users(uid) ON DELETE CASCADE, role TEXT NOT NULL, since INTEGER NOT NULL, PRIMARY KEY (uid, role));
         CREATE TABLE IF NOT EXISTS profiles (
             uid INTEGER PRIMARY KEY REFERENCES users(uid) ON DELETE CASCADE,
             about TEXT NOT NULL DEFAULT '',
             status_text TEXT NOT NULL DEFAULT '',
             status_emoji TEXT NOT NULL DEFAULT '',
             accent TEXT NOT NULL DEFAULT '',
             theme TEXT NOT NULL DEFAULT 'eclipse',
             banner TEXT,                             -- media's info (JSON)
             music TEXT,                              -- {kind: file|soundcloud, …} (JSON)
             socials TEXT NOT NULL DEFAULT '{}',
             privacy TEXT NOT NULL DEFAULT '{}',
             prefix TEXT NOT NULL DEFAULT 'auto',     -- auto | none | a role they hold
             showcase TEXT NOT NULL DEFAULT '[]',     -- the badges shown first (ids, JSON)
             updated INTEGER NOT NULL DEFAULT 0);
         CREATE TABLE IF NOT EXISTS user_badges (
             uid INTEGER NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
             badge TEXT NOT NULL,
             level INTEGER NOT NULL DEFAULT 1,
             at INTEGER NOT NULL,
             by INTEGER,                              -- who gave it; null: earned
             PRIMARY KEY (uid, badge));
         CREATE INDEX IF NOT EXISTS user_badges_badge ON user_badges(badge);
         CREATE TABLE IF NOT EXISTS stats (
             uid INTEGER NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
             key TEXT NOT NULL,
             value INTEGER NOT NULL,
             PRIMARY KEY (uid, key));
         CREATE TABLE IF NOT EXISTS blocks (
             uid INTEGER NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
             blocked INTEGER NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
             at INTEGER NOT NULL,
             PRIMARY KEY (uid, blocked));
         CREATE TABLE IF NOT EXISTS audit (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             at INTEGER NOT NULL,
             actor INTEGER NOT NULL,
             action TEXT NOT NULL,
             target TEXT NOT NULL,
             details TEXT NOT NULL);",
    )
    .expect("make the people's tables");
    let has_label: bool = c.prepare("SELECT 1 FROM pragma_table_info('sessions') WHERE name = 'label'").and_then(|mut q| q.exists([])).unwrap_or(false);
    if !has_label {
        c.execute("ALTER TABLE sessions ADD COLUMN label TEXT NOT NULL DEFAULT ''", []).expect("the sessions' label");
    }
    // A text UID (03.10.2026): shown instead of the number, which stays what everything of the member's is kept by.
    let has_text_uid: bool = c.prepare("SELECT 1 FROM pragma_table_info('users') WHERE name = 'uid_text'").and_then(|mut q| q.exists([])).unwrap_or(false);
    if !has_text_uid {
        c.execute("ALTER TABLE users ADD COLUMN uid_text TEXT", []).expect("the text UID");
    }
    c.execute("CREATE UNIQUE INDEX IF NOT EXISTS users_uid_text ON users(lower(uid_text)) WHERE uid_text IS NOT NULL", []).expect("the text UIDs' index");
    refresh_all(c);
    sweep(c);
}

/// A staff member's act, kept for the journal.
pub fn audit(c: &Connection, actor: i64, action: &str, target: &str, details: Value) {
    let _ = c.execute("INSERT INTO audit (at, actor, action, target, details) VALUES (?1, ?2, ?3, ?4, ?5)", params![now(), actor, action, target, details.to_string()]);
}

/// Whether `uid` has blocked `other`.
pub fn blocked(c: &Connection, uid: i64, other: i64) -> bool {
    c.query_row("SELECT 1 FROM blocks WHERE uid = ?1 AND blocked = ?2", params![uid, other], |_| Ok(())).optional().ok().flatten().is_some()
}

// ------------------------------------------------------------------------------------------------ profiles

pub const THEMES: &[&str] = &["eclipse", "void", "aurora", "ember", "frost", "bloom"];
/// The pack's own emoji (site /e/<id>.gif, the client's sheets): the status and the reactions use these.
pub const EMOJI: &[&str] = &[
    "c_arrow", "c_bell", "c_book", "c_camera", "c_chat", "c_check", "c_cross", "c_crown", "c_dot", "c_download", "c_gavel", "c_gem", "c_globe", "c_heart", "c_lock", "c_mask", "c_play",
    "c_plane", "c_portal", "c_rocket", "c_shield", "c_team", "c_warn", "c_wings",
];
pub const SOCIALS: &[&str] = &["discord", "telegram", "youtube", "twitch", "tiktok", "vk", "steam", "github", "soundcloud", "site"];

#[derive(Default)]
struct ProfileRow {
    about: String,
    status_text: String,
    status_emoji: String,
    accent: String,
    theme: String,
    banner: Option<Value>,
    music: Option<Value>,
    socials: Map<String, Value>,
    privacy: Map<String, Value>,
    prefix: String,
    showcase: Vec<String>,
}

fn profile_row(c: &Connection, uid: i64) -> ProfileRow {
    c.query_row(
        "SELECT about, status_text, status_emoji, accent, theme, banner, music, socials, privacy, prefix, showcase FROM profiles WHERE uid = ?1",
        params![uid],
        |r| {
            let parse = |s: Option<String>| s.and_then(|t| serde_json::from_str::<Value>(&t).ok());
            Ok(ProfileRow {
                about: r.get(0)?,
                status_text: r.get(1)?,
                status_emoji: r.get(2)?,
                accent: r.get(3)?,
                theme: r.get(4)?,
                banner: parse(r.get(5)?),
                music: parse(r.get(6)?),
                socials: parse(r.get(7)?).and_then(|v| v.as_object().cloned()).unwrap_or_default(),
                privacy: parse(r.get(8)?).and_then(|v| v.as_object().cloned()).unwrap_or_default(),
                prefix: r.get(9)?,
                showcase: parse(r.get(10)?).and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default(),
            })
        },
    )
    .optional()
    .ok()
    .flatten()
    .unwrap_or(ProfileRow { theme: "eclipse".into(), prefix: "auto".into(), ..Default::default() })
}

fn ensure_profile(c: &Connection, uid: i64) -> rusqlite::Result<()> {
    c.execute("INSERT OR IGNORE INTO profiles (uid) VALUES (?1)", params![uid]).map(|_| ())
}

/// A member's privacy, each part with its default: the server they play on is hidden until they show it.
fn privacy_of(p: &ProfileRow) -> Value {
    let get = |k: &str, d: bool| p.privacy.get(k).and_then(Value::as_bool).unwrap_or(d);
    json!({
        "online": get("online", true), "listening": get("listening", true), "server": get("server", false), "hours": get("hours", true),
        "dm": p.privacy.get("dm").and_then(Value::as_str).filter(|d| ["all", "friends", "none"].contains(d)).unwrap_or("friends"),
    })
}

/// A name of `min` to `max` letters and digits, and of `extra`.
fn word(s: &str, min: usize, max: usize, extra: &str) -> bool {
    (min..=max).contains(&s.len()) && s.chars().all(|c| c.is_ascii_alphanumeric() || extra.contains(c))
}

/// A link from what a member typed for a network, or why it can't be one. The empty string takes it away.
pub fn social(network: &str, typed: &str) -> Result<String, String> {
    let t = typed.trim();
    let handle = |prefixes: &[&str], ok: fn(&str) -> bool| -> Result<String, String> {
        let mut h = t.trim_start_matches('@').to_string();
        for p in prefixes {
            if let Some(rest) = h.strip_prefix(p) {
                h = rest.trim_start_matches('@').trim_end_matches('/').to_string();
            }
        }
        if ok(&h) {
            Ok(h)
        } else {
            Err(format!("that is not a {network} name"))
        }
    };
    match network {
        "discord" => handle(&[], |s| word(s, 2, 32, "_.")),
        "telegram" => handle(&["https://t.me/", "http://t.me/", "t.me/"], |s| (5..=32).contains(&s.len()) && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')),
        "twitch" => handle(&["https://www.twitch.tv/", "https://twitch.tv/", "twitch.tv/"], |s| (3..=25).contains(&s.len()) && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')),
        "tiktok" => handle(&["https://www.tiktok.com/", "https://tiktok.com/", "tiktok.com/"], |s| (2..=24).contains(&s.len()) && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')),
        "vk" => handle(&["https://vk.com/", "http://vk.com/", "vk.com/"], |s| (2..=32).contains(&s.len()) && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')),
        "github" => handle(&["https://github.com/", "github.com/"], |s| (1..=39).contains(&s.len()) && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')),
        "soundcloud" => handle(&["https://soundcloud.com/", "soundcloud.com/"], |s| (2..=40).contains(&s.len()) && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')),
        "youtube" => {
            let h = t.trim_start_matches("https://").trim_start_matches("http://").trim_start_matches("www.").trim_start_matches("m.");
            let h = h.strip_prefix("youtube.com/").unwrap_or(h).trim_end_matches('/');
            let ok = (h.starts_with('@') && (4..=31).contains(&h.len()) && h[1..].chars().all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c)))
                || (h.starts_with("channel/") && h.len() == 32 && h[8..].chars().all(|c| c.is_ascii_alphanumeric() || "_-".contains(c)));
            let h = if !ok && !h.contains('/') && word(h, 3, 30, "_.-") { format!("@{h}") } else { h.to_string() };
            if ok || h.starts_with('@') {
                Ok(h)
            } else {
                Err("that is not a YouTube channel".into())
            }
        }
        "steam" => {
            let h = t.trim_start_matches("https://").trim_start_matches("http://").trim_start_matches("steamcommunity.com/").trim_end_matches('/');
            let ok = (h.starts_with("id/") && word(&h[3..], 2, 32, "_-")) || (h.starts_with("profiles/") && h.len() == 26 && h[9..].chars().all(|c| c.is_ascii_digit()));
            if ok {
                Ok(h.to_string())
            } else if word(h, 2, 32, "_-") {
                Ok(format!("id/{h}"))
            } else {
                Err("that is not a Steam profile".into())
            }
        }
        "site" => {
            let ok = (t.starts_with("https://") || t.starts_with("http://")) && t.len() <= 200 && !t.chars().any(|c| c.is_whitespace() || c.is_control() || "<>\"'`".contains(c));
            let host = t.split("://").nth(1).unwrap_or("").split('/').next().unwrap_or("");
            if ok && host.contains('.') {
                Ok(t.to_string())
            } else {
                Err("an address that starts with https://".into())
            }
        }
        _ => Err("no such network".into()),
    }
}

/// An accent colour as #RRGGBB, or the empty string (the site's own).
fn accent_ok(s: &str) -> bool {
    s.is_empty() || (s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit()))
}

fn clean_text(s: &str, most: usize) -> String {
    let t: String = s.replace("\r\n", "\n").chars().filter(|c| *c == '\n' || !c.is_control()).take(most).collect();
    t.trim().to_string()
}

/// What a viewer may see of where a member is.
fn presence_for(uid: i64, p: &ProfileRow, viewer: i64) -> Value {
    let privacy = privacy_of(p);
    let own = viewer == uid;
    let Some(here) = crate::live::presence(uid) else { return json!({ "online": null }) };
    if !own && !privacy["online"].as_bool().unwrap_or(true) {
        return json!({ "online": null });
    }
    json!({
        "online": if here.game > 0 { "game" } else { "site" },
        "listening": if own || privacy["listening"].as_bool().unwrap_or(true) { json!(here.listening) } else { Value::Null },
        "server": if own || privacy["server"].as_bool().unwrap_or(false) { json!(here.server) } else { Value::Null },
    })
}

#[derive(Deserialize)]
struct ProfileEdit {
    about: Option<String>,
    status_text: Option<String>,
    status_emoji: Option<String>,
    accent: Option<String>,
    theme: Option<String>,
    socials: Option<Map<String, Value>>,
    privacy: Option<Map<String, Value>>,
    prefix: Option<String>,
    showcase: Option<Vec<String>>,
}

async fn edit_profile(State(app): State<Shared>, h: HeaderMap, Json(e): Json<ProfileEdit>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        if let Some(a) = &e.accent {
            if !accent_ok(a) {
                return Err(said(StatusCode::BAD_REQUEST, "accent", "a colour as #RRGGBB"));
            }
        }
        if let Some(t) = &e.theme {
            if !THEMES.contains(&t.as_str()) {
                return Err(said(StatusCode::BAD_REQUEST, "theme", "no such backdrop"));
            }
        }
        if let Some(em) = &e.status_emoji {
            if !em.is_empty() && !EMOJI.contains(&em.as_str()) {
                return Err(said(StatusCode::BAD_REQUEST, "emoji", "no such emoji"));
            }
        }
        let mut socials: Option<Map<String, Value>> = None;
        if let Some(s) = &e.socials {
            let mut out = Map::new();
            for (net, v) in s {
                if !SOCIALS.contains(&net.as_str()) {
                    return Err(said(StatusCode::BAD_REQUEST, "social", "no such network"));
                }
                let typed = v.as_str().unwrap_or("");
                if typed.trim().is_empty() {
                    continue;
                }
                match social(net, typed) {
                    Ok(link) => {
                        out.insert(net.clone(), json!(link));
                    }
                    Err(m) => return Err(said(StatusCode::BAD_REQUEST, "social", &format!("{net}: {m}"))),
                }
            }
            socials = Some(out);
        }
        let mut privacy: Option<Map<String, Value>> = None;
        if let Some(p) = &e.privacy {
            let mut out = Map::new();
            for (k, v) in p {
                match k.as_str() {
                    "online" | "listening" | "server" | "hours" => {
                        out.insert(k.clone(), json!(v.as_bool().unwrap_or(false)));
                    }
                    "dm" if ["all", "friends", "none"].contains(&v.as_str().unwrap_or("")) => {
                        out.insert(k.clone(), v.clone());
                    }
                    _ => return Err(said(StatusCode::BAD_REQUEST, "privacy", "no such setting")),
                }
            }
            privacy = Some(out);
        }
        if let Some(pfx) = &e.prefix {
            if !["auto", "none"].contains(&pfx.as_str()) && !roles_of(uid).iter().any(|r| r == pfx) {
                return Err(said(StatusCode::BAD_REQUEST, "prefix", "a prefix of a role you hold"));
            }
        }
        let v = db(&app, move |c| -> rusqlite::Result<Result<(), Response>> {
            ensure_profile(c, uid)?;
            let old = profile_row(c, uid);
            if let Some(show) = &e.showcase {
                let mine: Vec<String> = earned(c, uid).into_iter().map(|(id, _, _, _)| id).collect();
                if show.len() > 6 || show.iter().any(|b| !mine.contains(b)) {
                    return Ok(Err(said(StatusCode::BAD_REQUEST, "showcase", "at most six of your own badges")));
                }
            }
            let mut merged_privacy = old.privacy.clone();
            if let Some(p) = privacy {
                merged_privacy.extend(p);
            }
            c.execute(
                "UPDATE profiles SET about = ?2, status_text = ?3, status_emoji = ?4, accent = ?5, theme = ?6, socials = ?7, privacy = ?8, prefix = ?9, showcase = ?10, updated = ?11 WHERE uid = ?1",
                params![
                    uid,
                    e.about.as_deref().map_or(old.about, |a| clean_text(a, 1000)),
                    e.status_text.as_deref().map_or(old.status_text, |s| clean_line(s, 60)),
                    e.status_emoji.unwrap_or(old.status_emoji),
                    e.accent.unwrap_or(old.accent).to_uppercase(),
                    e.theme.unwrap_or(old.theme),
                    Value::Object(socials.unwrap_or(old.socials)).to_string(),
                    Value::Object(merged_privacy).to_string(),
                    e.prefix.unwrap_or(old.prefix),
                    json!(e.showcase.unwrap_or(old.showcase)).to_string(),
                    now(),
                ],
            )?;
            refresh(c, uid);
            Ok(Ok(()))
        })
        .await
        .map_err(|_| busy())?;
        v?;
        presence_changed(uid);
        Ok(Json(json!({})).into_response())
    })
    .await
}

/// A member's profile, as another member sees it (members only, like the forum).
async fn profile(State(app): State<Shared>, h: HeaderMap, Key(login): Key<String>) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        let viewer = w.uid;
        let v = db(&app, move |c| -> rusqlite::Result<Option<Value>> {
            let Some(uid) = uid_of(c, &login) else { return Ok(None) };
            let created: i64 = c.query_row("SELECT created FROM users WHERE uid = ?1", params![uid], |r| r.get(0))?;
            let p = profile_row(c, uid);
            let own = viewer == uid;
            let privacy = privacy_of(&p);
            let posts: i64 = c.query_row("SELECT COUNT(*) FROM posts WHERE author = ?1 AND state = 'shown'", params![uid], |r| r.get(0)).unwrap_or(0);
            let threads: i64 = c.query_row("SELECT COUNT(*) FROM threads WHERE author = ?1 AND state = 'open'", params![uid], |r| r.get(0)).unwrap_or(0);
            let packs: i64 = c.query_row("SELECT COUNT(*) FROM packs WHERE owner = ?1 AND state = 'published' AND visibility = 'public'", params![uid], |r| r.get(0)).unwrap_or(0);
            let minutes = stat(c, uid, "game_min");
            let hours = (own || privacy["hours"].as_bool().unwrap_or(true)).then_some(minutes / 60);
            let badges: Vec<Value> = earned(c, uid).into_iter().map(|(id, level, at, by)| json!({ "id": id, "level": level, "at": at, "given": by.is_some() })).collect();
            let shots = shots_of(c, uid, i64::MAX, 24)?;
            let mut out = json!({
                "person": card_of(c, uid),
                "since": created,
                "about": p.about,
                "theme": p.theme,
                "banner": p.banner,
                "music": p.music,
                "socials": p.socials,
                "showcase": p.showcase,
                "badges": badges,
                "stats": { "posts": posts, "threads": threads, "packs": packs, "hours": hours },
                "presence": presence_for(uid, &p, viewer),
                "shots": shots,
                "viewer": { "self": own, "blocked": blocked(c, viewer, uid), "staff": is_staff(viewer) },
                "relation": if own { Value::Null } else { crate::friends::relation(c, viewer, uid) },
                "dm": privacy["dm"].clone(),
            });
            if own {
                out["privacy"] = privacy;
                out["prefix_choice"] = json!(p.prefix);
                out["roles"] = json!(roles_of(uid));
                out["status_text"] = json!(p.status_text);
                out["status_emoji"] = json!(p.status_emoji);
                out["accent"] = json!(p.accent);
                out["media_used"] = json!(crate::media::used(c, uid));
            }
            Ok(Some(out))
        })
        .await
        .map_err(|_| busy())?;
        match v {
            Some(v) => Ok(Json(v).into_response()),
            None => Err(fail(StatusCode::NOT_FOUND, "missing")),
        }
    })
    .await
}

#[derive(Deserialize)]
struct Ids {
    ids: String,
}

/// Cards of many members at once (the client's lists): `?ids=1,2,3`, at most a hundred.
async fn many(State(app): State<Shared>, h: HeaderMap, Query(q): Query<Ids>) -> Response {
    run(async {
        member(&app, &h, false).await?;
        let ids: Vec<i64> = q.ids.split(',').filter_map(|s| s.trim().parse().ok()).take(100).collect();
        let v = db(&app, move |c| ids.into_iter().map(|u| card_of(c, u)).filter(|v| !v.is_null()).collect::<Vec<Value>>()).await;
        Ok(Json(json!({ "people": v })).into_response())
    })
    .await
}

// ------------------------------------------------------------------------------------------------ banner, music, shots

async fn upload(app: &Shared, h: &HeaderMap, body: Bytes, most: usize, kind: &'static str) -> Result<(i64, Value), Response> {
    let w = member(app, h, true).await?;
    if body.is_empty() || body.len() > most {
        return Err(said(StatusCode::BAD_REQUEST, "file", &format!("a file of at most {} MB", most >> 20)));
    }
    if !crate::media::paced(w.uid) {
        return Err(fail(StatusCode::TOO_MANY_REQUESTS, "slow"));
    }
    let uid = w.uid;
    let left = db(app, move |c| crate::media::quota_left(c, uid)).await;
    if left < body.len() as i64 {
        return Err(said(StatusCode::INSUFFICIENT_STORAGE, "quota", "your uploads take all of your 300 MB: take some away first"));
    }
    let media = app.market.media.clone();
    let made = tokio::task::spawn_blocking(move || match kind {
        "banner" => crate::media::banner(&body, &media),
        "shot" => crate::media::shot(&body, &media),
        _ => crate::media::music(&body, &media),
    })
    .await
    .map_err(|_| busy())?
    .map_err(|m| said(StatusCode::BAD_REQUEST, "file", &m))?;
    let info = db(app, move |c| crate::media::record(c, uid, kind, &made)).await.map_err(|_| busy())?;
    Ok((uid, info))
}

/// The id of what a profile field shows now (to take it down when it is replaced).
fn media_id(v: &Option<Value>) -> Option<String> {
    v.as_ref().and_then(|v| v["id"].as_str()).map(str::to_string)
}

async fn set_banner(State(app): State<Shared>, h: HeaderMap, body: Bytes) -> Response {
    run(async {
        let (uid, info) = upload(&app, &h, body, crate::media::MOST_PICTURE, "banner").await?;
        let shown = info.clone();
        db(&app, move |c| -> rusqlite::Result<()> {
            ensure_profile(c, uid)?;
            if let Some(old) = media_id(&profile_row(c, uid).banner) {
                crate::media::remove(c, &old, None)?;
            }
            c.execute("UPDATE profiles SET banner = ?2, updated = ?3 WHERE uid = ?1", params![uid, info.to_string(), now()])?;
            Ok(())
        })
        .await
        .map_err(|_| busy())?;
        Ok(Json(json!({ "banner": shown })).into_response())
    })
    .await
}

async fn clear_banner(State(app): State<Shared>, h: HeaderMap) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        db(&app, move |c| -> rusqlite::Result<()> {
            if let Some(old) = media_id(&profile_row(c, uid).banner) {
                crate::media::remove(c, &old, None)?;
            }
            c.execute("UPDATE profiles SET banner = NULL, updated = ?2 WHERE uid = ?1", params![uid, now()])?;
            Ok(())
        })
        .await
        .map_err(|_| busy())?;
        Ok(Json(json!({})).into_response())
    })
    .await
}

/// A song sent as an MP3: its names from its tags, which the member may change after.
async fn set_music_file(State(app): State<Shared>, h: HeaderMap, body: Bytes) -> Response {
    run(async {
        let (uid, info) = upload(&app, &h, body, crate::media::MOST_MUSIC, "music").await?;
        let music = json!({
            "kind": "file", "id": info["id"], "url": info["url"], "ms": info["ms"],
            "title": clean_line(info["tag_title"].as_str().unwrap_or(""), 80), "artist": clean_line(info["tag_artist"].as_str().unwrap_or(""), 80),
        });
        let shown = music.clone();
        db(&app, move |c| -> rusqlite::Result<()> {
            ensure_profile(c, uid)?;
            if let Some(old) = media_id(&profile_row(c, uid).music) {
                crate::media::remove(c, &old, None)?;
            }
            c.execute("UPDATE profiles SET music = ?2, updated = ?3 WHERE uid = ?1", params![uid, music.to_string(), now()])?;
            Ok(())
        })
        .await
        .map_err(|_| busy())?;
        Ok(Json(json!({ "music": shown })).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct MusicEdit {
    soundcloud: Option<String>,
    title: Option<String>,
    artist: Option<String>,
}

/// A SoundCloud track's address (`https://soundcloud.com/<artist>/<track>`), or the names of the song shown.
fn soundcloud_ok(u: &str) -> Option<String> {
    let rest = u.trim().strip_prefix("https://soundcloud.com/").or_else(|| u.trim().strip_prefix("https://m.soundcloud.com/"))?;
    let path = rest.split(['?', '#']).next()?.trim_end_matches('/');
    let parts: Vec<&str> = path.split('/').collect();
    let ok = |s: &str| (1..=100).contains(&s.len()) && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    (parts.len() == 2 && parts.iter().all(|p| ok(p)) && !["sets", "you", "discover", "search"].contains(&parts[0])).then(|| format!("https://soundcloud.com/{path}"))
}

async fn edit_music(State(app): State<Shared>, h: HeaderMap, Json(e): Json<MusicEdit>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        let link = match &e.soundcloud {
            Some(u) => Some(soundcloud_ok(u).ok_or_else(|| said(StatusCode::BAD_REQUEST, "soundcloud", "a SoundCloud track: https://soundcloud.com/artist/track"))?),
            None => None,
        };
        let shown = db(&app, move |c| -> rusqlite::Result<Option<Value>> {
            ensure_profile(c, uid)?;
            let old = profile_row(c, uid).music;
            let mut music = match link {
                Some(url) => {
                    if let Some(id) = media_id(&old) {
                        crate::media::remove(c, &id, None)?;
                    }
                    // Its names from its address until the member types them.
                    let mut parts = url.trim_start_matches("https://soundcloud.com/").split('/');
                    let artist = parts.next().unwrap_or("").replace(['-', '_'], " ");
                    let title = parts.next().unwrap_or("").replace(['-', '_'], " ");
                    json!({ "kind": "soundcloud", "url": url, "title": title, "artist": artist })
                }
                None => match old {
                    Some(m) => m,
                    None => return Ok(None),
                },
            };
            if let Some(t) = e.title {
                music["title"] = json!(clean_line(&t, 80));
            }
            if let Some(a) = e.artist {
                music["artist"] = json!(clean_line(&a, 80));
            }
            c.execute("UPDATE profiles SET music = ?2, updated = ?3 WHERE uid = ?1", params![uid, music.to_string(), now()])?;
            Ok(Some(music))
        })
        .await
        .map_err(|_| busy())?;
        match shown {
            Some(m) => Ok(Json(json!({ "music": m })).into_response()),
            None => Err(said(StatusCode::BAD_REQUEST, "music", "there is no song to name")),
        }
    })
    .await
}

async fn clear_music(State(app): State<Shared>, h: HeaderMap) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        db(&app, move |c| -> rusqlite::Result<()> {
            if let Some(old) = media_id(&profile_row(c, uid).music) {
                crate::media::remove(c, &old, None)?;
            }
            c.execute("UPDATE profiles SET music = NULL, updated = ?2 WHERE uid = ?1", params![uid, now()])?;
            Ok(())
        })
        .await
        .map_err(|_| busy())?;
        Ok(Json(json!({})).into_response())
    })
    .await
}

const MOST_SHOTS: i64 = 200;

fn shots_of(c: &Connection, uid: i64, before: i64, limit: i64) -> rusqlite::Result<Vec<Value>> {
    let mut q = c.prepare("SELECT info, created, rowid FROM media WHERE owner = ?1 AND kind = 'shot' AND state = 'ok' AND rowid < ?2 ORDER BY rowid DESC LIMIT ?3")?;
    let rows = q.query_map(params![uid, before, limit], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?)))?;
    Ok(rows
        .flatten()
        .filter_map(|(info, created, at)| {
            let mut v: Value = serde_json::from_str(&info).ok()?;
            v["created"] = json!(created);
            v["at"] = json!(at);
            Some(v)
        })
        .collect())
}

async fn add_shot(State(app): State<Shared>, h: HeaderMap, body: Bytes) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        let count: i64 = db(&app, move |c| c.query_row("SELECT COUNT(*) FROM media WHERE owner = ?1 AND kind = 'shot' AND state = 'ok'", params![uid], |r| r.get(0)).unwrap_or(0)).await;
        if count >= MOST_SHOTS {
            return Err(said(StatusCode::BAD_REQUEST, "shots", "your gallery holds 200 screenshots: take some away first"));
        }
        let (_, info) = upload(&app, &h, body, crate::media::MOST_PICTURE, "shot").await?;
        Ok((StatusCode::CREATED, Json(json!({ "shot": info }))).into_response())
    })
    .await
}

async fn drop_shot(State(app): State<Shared>, h: HeaderMap, Key(id): Key<String>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        let gone = db(&app, move |c| -> rusqlite::Result<bool> {
            match crate::media::info_of(c, uid, &id) {
                Some((kind, _)) if kind == "shot" => Ok(crate::media::remove(c, &id, None)? == 1),
                _ => Ok(false),
            }
        })
        .await
        .map_err(|_| busy())?;
        if gone {
            Ok(Json(json!({})).into_response())
        } else {
            Err(fail(StatusCode::NOT_FOUND, "missing"))
        }
    })
    .await
}

#[derive(Deserialize)]
struct Before {
    before: Option<i64>,
}

async fn shots(State(app): State<Shared>, h: HeaderMap, Key(login): Key<String>, Query(q): Query<Before>) -> Response {
    run(async {
        member(&app, &h, false).await?;
        let v = db(&app, move |c| -> rusqlite::Result<Option<Vec<Value>>> {
            let Some(uid) = uid_of(c, &login) else { return Ok(None) };
            shots_of(c, uid, q.before.unwrap_or(i64::MAX), 48).map(Some)
        })
        .await
        .map_err(|_| busy())?;
        match v {
            Some(list) => Ok(Json(json!({ "shots": list })).into_response()),
            None => Err(fail(StatusCode::NOT_FOUND, "missing")),
        }
    })
    .await
}

// ------------------------------------------------------------------------------------------------ badges

pub struct Badge {
    pub id: &'static str,
    pub en: &'static str,
    pub ru: &'static str,
    pub about_en: &'static str,
    pub about_ru: &'static str,
    pub icon: &'static str,
    /// 0 common, 1 rare, 2 epic, 3 legendary, 4 mythic (for a badge of levels: of its first level).
    pub rarity: u8,
    /// Shown as "???" until earned.
    pub secret: bool,
    /// The count it follows, and the counts of its levels with their rarity.
    pub stat: Option<&'static str>,
    pub levels: &'static [(i64, u8)],
}

macro_rules! badge {
    ($id:literal, $en:literal, $ru:literal, $aen:literal, $aru:literal, $icon:literal, $rarity:literal) => {
        Badge { id: $id, en: $en, ru: $ru, about_en: $aen, about_ru: $aru, icon: $icon, rarity: $rarity, secret: false, stat: None, levels: &[] }
    };
}

pub const BADGES: &[Badge] = &[
    badge!("founder", "Founder", "Основатель", "Made Cursed.", "Создала Cursed.", "c_crown", 4),
    badge!("first100", "First hundred", "Первые 100", "One of the first hundred to join.", "Среди первых ста участников.", "c_wings", 3),
    badge!("first1000", "First thousand", "Первая тысяча", "One of the first thousand to join.", "Среди первой тысячи участников.", "c_gem", 2),
    // (Cursed's beta, its name since Recode has a beta of its own: her word, 19.10)
    badge!("beta", "Cursed beta", "Бета Cursed", "Joined while Cursed was in beta.", "В Cursed со времён беты.", "c_rocket", 2),
    badge!("launch", "Launch day", "День запуска", "Joined on the day the beta opened, 1 October 2026.", "В Cursed с дня открытия беты, 1 октября 2026.", "c_portal", 1),
    // Recode's (her word, 19.10; recode.rs `marks` and `played_recode`)
    badge!("rc_beta", "Recode beta", "Бета Recode", "One of the first seventy to buy Recode, in its beta.", "Одна из первых 70 подписок Recode — со времён беты.", "c_rocket", 3),
    badge!("recode", "Recode", "Recode", "Has had Recode.", "Подписка Recode.", "c_portal", 1),
    badge!("forever", "Forever", "Навсегда", "Recode for good.", "Recode навсегда.", "c_lock", 2),
    Badge {
        id: "recode_hours",
        en: "Time in Recode",
        ru: "Время в Recode",
        about_en: "Hours played in Recode: 10, 100, 500, 1000.",
        about_ru: "Часы в Recode: 10, 100, 500, 1000.",
        icon: "c_dot",
        rarity: 0,
        secret: false,
        stat: Some("recode_min"),
        levels: &[(600, 0), (6_000, 1), (30_000, 2), (60_000, 3)],
    },
    Badge {
        id: "hours",
        en: "Time in Cursed",
        ru: "Время в Cursed",
        about_en: "Hours played in the client: 10, 100, 500, 1000.",
        about_ru: "Часы в клиенте: 10, 100, 500, 1000.",
        icon: "c_dot",
        rarity: 0,
        secret: false,
        stat: Some("game_min"),
        levels: &[(600, 0), (6_000, 1), (30_000, 2), (60_000, 3)],
    },
    badge!("bug_found", "Bug found", "Баг найден", "A bug they reported was confirmed.", "Присланный баг подтвердили.", "c_warn", 1),
    // The wardrobe's (23.10.2026, looks.rs): a look put together and kept, and the likes it gathers.
    badge!("stylist", "Stylist", "Стилист", "Put a look together in the wardrobe and kept it.", "Собрал(а) и сохранил(а) образ в гардеробе.", "c_mask", 0),
    Badge {
        id: "style_icon",
        en: "Style icon",
        ru: "Икона стиля",
        about_en: "Likes on their look: 5, 25, 100.",
        about_ru: "Лайки образа: 5, 25, 100.",
        icon: "c_heart",
        rarity: 1,
        secret: false,
        stat: Some("look_likes"),
        levels: &[(5, 1), (25, 2), (100, 3)],
    },
    Badge {
        id: "bug_hunter",
        en: "Bug hunter",
        ru: "Охотник за багами",
        about_en: "Bugs they reported were fixed: 1, 5, 20.",
        about_ru: "Исправленные баги из присланных: 1, 5, 20.",
        icon: "c_check",
        rarity: 1,
        secret: false,
        stat: Some("bugs_fixed"),
        levels: &[(1, 1), (5, 2), (20, 3)],
    },
    badge!("bug_critical", "Critical find", "Критическая находка", "Found a bug that mattered most.", "Самый важный найденный баг.", "c_shield", 3),
    Badge {
        id: "first_post",
        en: "First word",
        ru: "Первое слово",
        about_en: "Wrote on the forum.",
        about_ru: "Первый пост на форуме.",
        icon: "c_chat",
        rarity: 0,
        secret: false,
        stat: Some("posts"),
        levels: &[(1, 0)],
    },
    Badge {
        id: "expert",
        en: "Expert",
        ru: "Знаток",
        about_en: "Answers marked best: 1, 10, 50.",
        about_ru: "Ответы, отмеченные лучшими: 1, 10, 50.",
        icon: "c_book",
        rarity: 0,
        secret: false,
        stat: Some("best_answers"),
        levels: &[(1, 0), (10, 2), (50, 3)],
    },
    Badge {
        id: "idea",
        en: "Idea maker",
        ru: "Автор идеи",
        about_en: "An idea of theirs made it into Cursed.",
        about_ru: "Идею воплотили в Cursed.",
        icon: "c_gem",
        rarity: 2,
        secret: false,
        stat: Some("ideas_done"),
        levels: &[(1, 2), (5, 3)],
    },
    Badge {
        id: "liked",
        en: "Liked",
        ru: "Любимчик",
        about_en: "Reactions to their posts: 100, 1000.",
        about_ru: "Реакции на посты: 100, 1000.",
        icon: "c_heart",
        rarity: 1,
        secret: false,
        stat: Some("reactions_got"),
        levels: &[(100, 1), (1000, 3)],
    },
    Badge {
        id: "author",
        en: "Pack author",
        ru: "Автор пака",
        about_en: "Published a pack on the market.",
        about_ru: "Свой пак на маркете.",
        icon: "c_download",
        rarity: 0,
        secret: false,
        stat: Some("packs"),
        levels: &[(1, 0)],
    },
    Badge {
        id: "popular",
        en: "Popular",
        ru: "Популярный",
        about_en: "Their packs were taken 1000 times, then 10 000.",
        about_ru: "Паки скачали 1000 раз, потом 10 000.",
        icon: "c_globe",
        rarity: 2,
        secret: false,
        stat: Some("downloads"),
        levels: &[(1000, 2), (10_000, 3)],
    },
    Badge {
        id: "company",
        en: "Good company",
        ru: "Компания",
        about_en: "Ten friends in Cursed.",
        about_ru: "Десять друзей в Cursed.",
        icon: "c_team",
        rarity: 1,
        secret: false,
        stat: Some("friends"),
        levels: &[(10, 1)],
    },
    badge!("soul", "Life of the party", "Душа компании", "Made a group of 25.", "Группа на 25 человек.", "c_team", 2),
    Badge {
        id: "configurator",
        en: "Configurator",
        ru: "Конфигуратор",
        about_en: "Their configs were taken 50 times.",
        about_ru: "Конфиги активировали 50 раз.",
        icon: "c_lock",
        rarity: 2,
        secret: false,
        stat: Some("config_uses"),
        levels: &[(50, 2)],
    },
    badge!("media", "Creator", "Медиа", "Makes videos about Cursed.", "Снимает видео про Cursed.", "c_play", 2),
    // In the game, by the day (days_marks: Cursed's minutes and Recode's alike); on the site (her word, 19.10)
    Badge {
        id: "marathon",
        en: "Marathon",
        ru: "Марафон",
        about_en: "Six hours in the game in one day.",
        about_ru: "Шесть часов в игре за один день.",
        icon: "c_arrow",
        rarity: 2,
        secret: false,
        stat: Some("day_min"),
        levels: &[(360, 2)],
    },
    Badge {
        id: "streak",
        en: "No days off",
        ru: "Без пропусков",
        about_en: "In the game seven days in a row, then thirty.",
        about_ru: "В игре 7 дней подряд, потом 30.",
        icon: "c_check",
        rarity: 1,
        secret: false,
        stat: Some("streak"),
        levels: &[(7, 1), (30, 3)],
    },
    Badge {
        id: "collector",
        en: "Collector",
        ru: "Коллекционер",
        about_en: "Holds 10 badges, then 20.",
        about_ru: "Собрано 10 достижений, потом 20.",
        icon: "c_gem",
        rarity: 2,
        secret: false,
        stat: Some("badges"),
        levels: &[(10, 2), (20, 3)],
    },
    Badge {
        id: "oldtimer",
        en: "Old-timer",
        ru: "Старожил",
        about_en: "A month on the site, then half a year, then a year.",
        about_ru: "Месяц на сайте, потом полгода, потом год.",
        icon: "c_shield",
        rarity: 0,
        secret: false,
        stat: Some("age_days"),
        levels: &[(30, 0), (182, 2), (365, 3)],
    },
    Badge {
        id: "voice",
        en: "Voice",
        ru: "Голос",
        about_en: "Messages in the chats: 100, 1000, 10 000.",
        about_ru: "Сообщения в чатах: 100, 1000, 10 000.",
        icon: "c_chat",
        rarity: 0,
        secret: false,
        stat: Some("messages"),
        levels: &[(100, 0), (1_000, 2), (10_000, 3)],
    },
    Badge {
        id: "generous",
        en: "Generous",
        ru: "Щедрый",
        about_en: "Reactions given: 50, 500.",
        about_ru: "Поставлено реакций: 50, 500.",
        icon: "c_heart",
        rarity: 1,
        secret: false,
        stat: Some("reactions_given"),
        levels: &[(50, 1), (500, 2)],
    },
    Badge {
        id: "inviter",
        en: "Herald",
        ru: "Глашатай",
        about_en: "Players who came by their link: 10, 100, 1000.",
        about_ru: "Игроки, пришедшие по ссылке: 10, 100, 1000.",
        icon: "c_plane",
        rarity: 1,
        secret: false,
        stat: Some("invited"),
        levels: &[(10, 1), (100, 2), (1000, 3)],
    },
    Badge {
        id: "owl",
        en: "Night owl",
        ru: "Сова",
        about_en: "Played between three and five in the morning on ten nights.",
        about_ru: "Десять ночей в игре с трёх до пяти утра.",
        icon: "c_mask",
        rarity: 2,
        secret: true,
        stat: Some("owl_nights"),
        levels: &[(10, 2)],
    },
    Badge {
        id: "early",
        en: "Early bird",
        ru: "Ранняя пташка",
        about_en: "Played between six and eight in the morning on ten mornings.",
        about_ru: "Десять утр в игре с шести до восьми.",
        icon: "c_bell",
        rarity: 2,
        secret: true,
        stat: Some("early_mornings"),
        levels: &[(10, 2)],
    },
    Badge {
        id: "comeback",
        en: "Comeback",
        ru: "Возвращение",
        about_en: "Back in the game after a month away.",
        about_ru: "Снова в игре после месяца без неё.",
        icon: "c_plane",
        rarity: 1,
        secret: true,
        stat: None,
        levels: &[],
    },
    Badge {
        id: "melomane",
        en: "Melomane",
        ru: "Меломан",
        about_en: "Listened to a day of music in the client.",
        about_ru: "Сутки музыки в клиенте.",
        icon: "c_play",
        rarity: 2,
        secret: true,
        stat: Some("music_min"),
        levels: &[(1440, 2)],
    },
    Badge {
        id: "eclipse",
        en: "Stargazer",
        ru: "Звездочёт",
        about_en: "Found the constellations behind the eclipse.",
        about_ru: "Созвездия за затмением найдены.",
        icon: "c_portal",
        rarity: 1,
        secret: true,
        stat: None,
        levels: &[],
    },
];

pub fn badge_of(id: &str) -> Option<&'static Badge> {
    BADGES.iter().find(|b| b.id == id)
}

/// A badge's rarity at a level.
fn rarity_at(b: &Badge, level: i64) -> u8 {
    if b.levels.is_empty() {
        return b.rarity;
    }
    b.levels.get((level.max(1) - 1) as usize).map_or(b.rarity, |l| l.1)
}

/// The badges a member holds: (id, level, when, who gave it).
pub fn earned(c: &Connection, uid: i64) -> Vec<(String, i64, i64, Option<i64>)> {
    c.prepare("SELECT badge, level, at, by FROM user_badges WHERE uid = ?1 ORDER BY at")
        .and_then(|mut q| q.query_map(params![uid], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).map(|m| m.flatten().collect()))
        .unwrap_or_default()
}

/// A badge (or a higher level of it) for a member; a notification when it is new to them. True when it was.
pub fn award(c: &Connection, uid: i64, id: &str, level: i64, by: Option<i64>) -> bool {
    let Some(b) = badge_of(id) else { return false };
    let had: Option<i64> = c.query_row("SELECT level FROM user_badges WHERE uid = ?1 AND badge = ?2", params![uid, id], |r| r.get(0)).optional().ok().flatten();
    if had.is_some_and(|l| l >= level) {
        return false;
    }
    let done = c
        .execute(
            "INSERT INTO user_badges (uid, badge, level, at, by) VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT(uid, badge) DO UPDATE SET level = ?3, at = ?4, by = ?5",
            params![uid, id, level, now(), by],
        )
        .is_ok();
    if done {
        crate::notify::add(c, uid, "badge", by, &format!("badge:{id}"), json!({ "id": id, "level": level, "en": b.en, "ru": b.ru, "icon": b.icon, "rarity": rarity_at(b, level) }));
        // the collector counts every other badge held
        if id != "collector" {
            let held: i64 = c.query_row("SELECT COUNT(*) FROM user_badges WHERE uid = ?1 AND badge != 'collector'", params![uid], |r| r.get(0)).unwrap_or(0);
            set_stat(c, uid, "badges", held);
        }
    }
    done
}

pub fn stat(c: &Connection, uid: i64, key: &str) -> i64 {
    c.query_row("SELECT value FROM stats WHERE uid = ?1 AND key = ?2", params![uid, key], |r| r.get(0)).optional().ok().flatten().unwrap_or(0)
}

/// A count of a member's set, and the badges that follow it given.
pub fn set_stat(c: &Connection, uid: i64, key: &str, value: i64) {
    let _ = c.execute("INSERT INTO stats (uid, key, value) VALUES (?1, ?2, ?3) ON CONFLICT(uid, key) DO UPDATE SET value = ?3", params![uid, key, value]);
    for b in BADGES.iter().filter(|b| b.stat == Some(key)) {
        let level = b.levels.iter().take_while(|(n, _)| value >= *n).count() as i64;
        if level > 0 {
            award(c, uid, b.id, level, None);
        }
    }
}

/// What is counted from the tables again rather than bumped, for one member (`recount`) and for all (the sweep): a
/// reaction taken back and put again is the same reaction, a message deleted is not one.
const MESSAGES_OF: &str = "SELECT COUNT(*) FROM messages WHERE uid = ?1 AND deleted IS NULL AND kind != 'system'";
const REACTIONS_OF: &str = "SELECT (SELECT COUNT(*) FROM reactions WHERE uid = ?1) + (SELECT COUNT(*) FROM post_reactions WHERE uid = ?1)";

/// A member's messages or reactions given counted again (chat.rs, forum2.rs, after each), and the badges that follow.
pub fn recount(c: &Connection, uid: i64, key: &str) {
    let sql = match key {
        "messages" => MESSAGES_OF,
        "reactions_given" => REACTIONS_OF,
        _ => return,
    };
    let n: i64 = c.query_row(sql, params![uid], |r| r.get(0)).unwrap_or(0);
    if stat(c, uid, key) != n {
        set_stat(c, uid, key, n);
    }
}

/// A count of a member's raised (or lowered) by `delta`: its new value.
pub fn bump(c: &Connection, uid: i64, key: &str, delta: i64) -> i64 {
    let value = stat(c, uid, key) + delta;
    set_stat(c, uid, key, value);
    value
}

/// Minutes in the game counted (live.rs, every minute while it runs): the hours, and what any game counts by the day.
pub fn played(c: &Connection, uid: i64, minutes: i64, at: i64) {
    bump(c, uid, "game_min", minutes);
    days_marks(c, uid, minutes, at);
    if crate::live::presence(uid).is_some_and(|p| p.listening.is_some()) {
        bump(c, uid, "music_min", minutes);
    }
}

/// Minutes in Recode counted (recode.rs, its game's alive every minute): its own hours, and the same by the day.
pub fn played_recode(c: &Connection, uid: i64, minutes: i64, at: i64) {
    bump(c, uid, "recode_min", minutes);
    days_marks(c, uid, minutes, at);
}

/// What any game counts by the day, Moscow time: the night owl's nights (three to five in the morning) and the early
/// bird's mornings (six to eight), each counted once a day; the day's minutes (the marathon); days in a row; and a
/// return after a month away.
fn days_marks(c: &Connection, uid: i64, minutes: i64, at: i64) {
    let msk = at + 3 * 3600;
    let (day, hour) = (msk / 86_400, (msk % 86_400) / 3600);
    for (from, to, seen, count) in [(3, 5, "owl_day", "owl_nights"), (6, 8, "early_day", "early_mornings")] {
        if (from..to).contains(&hour) && stat(c, uid, seen) != day {
            set_stat(c, uid, seen, day);
            bump(c, uid, count, 1);
        }
    }
    if stat(c, uid, "min_day") == day {
        bump(c, uid, "day_min", minutes);
    } else {
        set_stat(c, uid, "min_day", day);
        set_stat(c, uid, "day_min", minutes);
    }
    let last = stat(c, uid, "last_day");
    if last != day {
        if last > 0 && day - last >= 30 {
            award(c, uid, "comeback", 1, None);
        }
        let streak = if last == day - 1 { stat(c, uid, "streak") + 1 } else { 1 };
        set_stat(c, uid, "streak", streak);
        set_stat(c, uid, "last_day", day);
    }
}

/// The badges that follow from what is already known, given to everyone who has earned them (at the start, and once
/// a day): where they came from, what they have done.
pub fn sweep(c: &Connection) {
    let _ = c.execute("INSERT OR IGNORE INTO user_badges (uid, badge, level, at) SELECT 0, 'founder', 1, ?1 WHERE EXISTS(SELECT 1 FROM users WHERE uid = 0)", params![now()]);
    let order: Vec<(i64, i64)> = c
        .prepare("SELECT uid, created FROM users WHERE uid != 0 ORDER BY created, uid")
        .and_then(|mut q| q.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).map(|m| m.flatten().collect()))
        .unwrap_or_default();
    let beta_over = c.query_row("SELECT value FROM settings WHERE name = 'beta_badge'", [], |r| r.get::<_, String>(0)).optional().ok().flatten().is_some_and(|v| v == "over");
    // 1 October 2026, Moscow: the day the beta opened.
    let (launch_from, launch_to) = (1_790_802_000, 1_790_802_000 + 86_400);
    for (i, (uid, created)) in order.iter().enumerate() {
        let quiet = |badge: &str| {
            let _ = c.execute("INSERT OR IGNORE INTO user_badges (uid, badge, level, at) VALUES (?1, ?2, 1, ?3)", params![uid, badge, created]);
        };
        if i < 100 {
            quiet("first100");
        }
        if i < 1000 {
            quiet("first1000");
        }
        if !beta_over {
            quiet("beta");
        }
        if (launch_from..launch_to).contains(created) {
            quiet("launch");
        }
    }
    // What the forum, the market and the chats already hold, and how long each has been here; the badges held last
    // (the collector's count, after all the others above have gone in).
    for (key, sql) in [
        ("posts", "SELECT author, COUNT(*) FROM posts WHERE state = 'shown' GROUP BY author"),
        ("packs", "SELECT owner, COUNT(*) FROM packs WHERE state = 'published' GROUP BY owner"),
        ("downloads", "SELECT owner, SUM(users) FROM packs WHERE state = 'published' GROUP BY owner"),
        ("messages", "SELECT uid, COUNT(*) FROM messages WHERE uid IS NOT NULL AND deleted IS NULL AND kind != 'system' GROUP BY uid"),
        ("reactions_given", "SELECT uid, COUNT(*) FROM (SELECT uid FROM reactions UNION ALL SELECT uid FROM post_reactions) GROUP BY uid"),
        ("age_days", "SELECT uid, (CAST(strftime('%s', 'now') AS INTEGER) - created) / 86400 FROM users"),
        ("badges", "SELECT uid, COUNT(*) FROM user_badges WHERE badge != 'collector' GROUP BY uid"),
    ] {
        let rows: Vec<(i64, i64)> = c.prepare(sql).and_then(|mut q| q.query_map([], |r| Ok((r.get(0)?, r.get::<_, Option<i64>>(1)?.unwrap_or(0)))).map(|m| m.flatten().collect())).unwrap_or_default();
        for (uid, n) in rows {
            if stat(c, uid, key) != n {
                let _ = c.execute("INSERT INTO stats (uid, key, value) VALUES (?1, ?2, ?3) ON CONFLICT(uid, key) DO UPDATE SET value = ?3", params![uid, key, n]);
                for b in BADGES.iter().filter(|b| b.stat == Some(key)) {
                    let level = b.levels.iter().take_while(|(m, _)| n >= *m).count() as i64;
                    if level > 0 {
                        let _ = c.execute(
                            "INSERT INTO user_badges (uid, badge, level, at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(uid, badge) DO UPDATE SET level = MAX(level, ?3)",
                            params![uid, b.id, level, now()],
                        );
                    }
                }
            }
        }
    }
    // The media's role carries its badge.
    let _ = c.execute("INSERT OR IGNORE INTO user_badges (uid, badge, level, at) SELECT uid, 'media', 1, since FROM roles WHERE role = 'media'", []);
    // Recode's roles and badges, as its keys say.
    crate::recode::sweep_marks(c);
}

/// The catalogue, with how many hold each (a secret one says nothing of itself to those without it).
async fn catalogue(State(app): State<Shared>, h: HeaderMap) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        let viewer = w.uid;
        let v = db(&app, move |c| {
            let people: i64 = c.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0)).unwrap_or(1).max(1);
            let mine: Vec<String> = earned(c, viewer).into_iter().map(|(id, _, _, _)| id).collect();
            let list: Vec<Value> = BADGES
                .iter()
                .map(|b| {
                    let holders: i64 = c.query_row("SELECT COUNT(*) FROM user_badges WHERE badge = ?1", params![b.id], |r| r.get(0)).unwrap_or(0);
                    let hidden = b.secret && !mine.iter().any(|m| m == b.id);
                    json!({
                        "id": b.id, "icon": if hidden { "c_lock" } else { b.icon }, "rarity": b.rarity, "secret": b.secret,
                        "en": if hidden { "???" } else { b.en }, "ru": if hidden { "???" } else { b.ru },
                        "about_en": if hidden { "A secret: found by doing." } else { b.about_en },
                        "about_ru": if hidden { "Секрет: находится делом." } else { b.about_ru },
                        "levels": b.levels.iter().map(|(n, r)| json!({ "at": n, "rarity": r })).collect::<Vec<_>>(),
                        "stat": b.stat, "holders": holders, "share": (holders as f64 * 1000.0 / people as f64).round() / 10.0,
                    })
                })
                .collect();
            json!({ "badges": list, "people": people })
        })
        .await;
        Ok(Json(v).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct Found {
    what: String,
}

/// A secret the client found (the constellations behind the eclipse).
async fn found(State(app): State<Shared>, h: HeaderMap, Json(f): Json<Found>) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        if f.what != "eclipse" {
            return Err(fail(StatusCode::NOT_FOUND, "missing"));
        }
        let uid = w.uid;
        db(&app, move |c| award(c, uid, "eclipse", 1, None)).await;
        Ok(Json(json!({})).into_response())
    })
    .await
}

// ------------------------------------------------------------------------------------------------ sessions

/// A short name for where a session was opened: the browser and the system its page said it was.
pub fn agent_label(h: &HeaderMap) -> String {
    let ua = h.get(header::USER_AGENT).and_then(|v| v.to_str().ok()).unwrap_or("");
    let browser = if ua.contains("YaBrowser") {
        "Yandex"
    } else if ua.contains("Edg/") {
        "Edge"
    } else if ua.contains("OPR/") || ua.contains("Opera") {
        "Opera"
    } else if ua.contains("Firefox/") {
        "Firefox"
    } else if ua.contains("Chrome/") {
        "Chrome"
    } else if ua.contains("Safari/") {
        "Safari"
    } else {
        "Browser"
    };
    let system = if ua.contains("Windows") {
        "Windows"
    } else if ua.contains("Android") {
        "Android"
    } else if ua.contains("iPhone") || ua.contains("iPad") {
        "iOS"
    } else if ua.contains("Mac OS") {
        "macOS"
    } else if ua.contains("Linux") {
        "Linux"
    } else {
        ""
    };
    if system.is_empty() {
        browser.to_string()
    } else {
        format!("{browser} · {system}")
    }
}

fn session_id(hash: &[u8]) -> String {
    hash.iter().take(6).map(|b| format!("{b:02x}")).collect()
}

async fn sessions(State(app): State<Shared>, h: HeaderMap) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        let (uid, current) = (w.uid, w.hash.clone());
        let v = db(&app, move |c| -> rusqlite::Result<Vec<Value>> {
            let mut q = c.prepare("SELECT hash, kind, label, created, seen FROM sessions WHERE uid = ?1 ORDER BY seen DESC")?;
            let rows = q.query_map(params![uid], |r| Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?, r.get::<_, i64>(4)?)))?;
            Ok(rows
                .flatten()
                .map(|(hash, kind, label, created, seen)| {
                    let label = if !label.is_empty() {
                        label
                    } else {
                        match kind.as_str() {
                            "loader" => "Cursed loader".into(),
                            "client" => "Cursed".into(),
                            _ => "Browser".into(),
                        }
                    };
                    json!({ "id": session_id(&hash), "kind": kind, "label": label, "created": created, "seen": seen, "current": hash == current })
                })
                .collect())
        })
        .await
        .map_err(|_| busy())?;
        Ok(Json(json!({ "sessions": v })).into_response())
    })
    .await
}

async fn end_session(State(app): State<Shared>, h: HeaderMap, Key(id): Key<String>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let (uid, current) = (w.uid, w.hash.clone());
        let gone = db(&app, move |c| -> rusqlite::Result<usize> {
            let hashes: Vec<Vec<u8>> = c.prepare("SELECT hash FROM sessions WHERE uid = ?1")?.query_map(params![uid], |r| r.get(0))?.flatten().collect();
            let mut n = 0;
            for hash in hashes.into_iter().filter(|h| session_id(h) == id && *h != current) {
                n += c.execute("DELETE FROM sessions WHERE hash = ?1", params![hash])?;
            }
            Ok(n)
        })
        .await
        .map_err(|_| busy())?;
        if gone > 0 {
            Ok(Json(json!({})).into_response())
        } else {
            Err(fail(StatusCode::NOT_FOUND, "missing"))
        }
    })
    .await
}

/// Every other session ended: the pages, the loaders and the games signed in anywhere else.
async fn end_others(State(app): State<Shared>, h: HeaderMap) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let (uid, current) = (w.uid, w.hash.clone());
        let n = db(&app, move |c| c.execute("DELETE FROM sessions WHERE uid = ?1 AND hash != ?2", params![uid, current])).await.map_err(|_| busy())?;
        Ok(Json(json!({ "ended": n })).into_response())
    })
    .await
}

// ------------------------------------------------------------------------------------------------ staff

#[derive(Deserialize)]
struct Give {
    login: String,
    role: Option<String>,
    badge: Option<String>,
    level: Option<i64>,
    on: bool,
}

// Web sessions, including owners, cannot change roles. Keep the old endpoint
// fail-closed for cached clients and direct requests.
async fn give_role(State(app): State<Shared>, h: HeaderMap) -> Response {
    run(async {
        member(&app, &h, true).await?;
        Err(fail(StatusCode::FORBIDDEN, "roles-console-only"))
    }).await
}

/// A role given or taken (by `giver`; -1: from the server's own command line).
pub fn set_role(c: &Connection, giver: i64, login: &str, role_id: &str, on: bool) -> Result<(), String> {
    if giver != -1 { return Err("roles-console-only".into()); }
    let r = role(role_id).ok_or("no such role")?;
    let uid = uid_of(c, login).ok_or_else(|| format!("no member {login}"))?;
    if on {
        c.execute("INSERT OR IGNORE INTO roles (uid, role, since) VALUES (?1, ?2, ?3)", params![uid, r.id, now()]).map_err(|e| e.to_string())?;
        if r.id == "media" {
            award(c, uid, "media", 1, (giver >= 0).then_some(giver));
        }
        crate::notify::add(c, uid, "role", (giver >= 0).then_some(giver), &format!("role:{}", r.id), json!({ "role": r.id, "en": r.en, "ru": r.ru, "colour": r.colour, "prefix": r.prefix }));
    } else {
        c.execute("DELETE FROM roles WHERE uid = ?1 AND role = ?2", params![uid, r.id]).map_err(|e| e.to_string())?;
    }
    audit(c, giver, if on { "role.give" } else { "role.take" }, &format!("user:{uid}"), json!({ "role": r.id, "login": login }));
    refresh(c, uid);
    Ok(())
}

/// A role that follows from what a member holds (recode.rs `marks`), given or taken by nobody: told to them when it
/// comes, like one given by hand. True when it changed.
pub fn follow_role(c: &Connection, uid: i64, role_id: &str, on: bool) -> bool {
    if !RECODE_ROLES.contains(&role_id) { return false; }
    let Some(r) = role(role_id) else { return false };
    let changed = if on {
        c.execute("INSERT OR IGNORE INTO roles (uid, role, since) VALUES (?1, ?2, ?3)", params![uid, r.id, now()]).unwrap_or(0) == 1
    } else {
        c.execute("DELETE FROM roles WHERE uid = ?1 AND role = ?2", params![uid, r.id]).unwrap_or(0) == 1
    };
    if changed {
        if on {
            crate::notify::add(c, uid, "role", None, &format!("role:{}", r.id), json!({ "role": r.id, "en": r.en, "ru": r.ru, "colour": r.colour, "prefix": r.prefix }));
        }
        refresh(c, uid);
    }
    changed
}

async fn give_badge(State(app): State<Shared>, h: HeaderMap, Json(g): Json<Give>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        if !can(w.uid, BADGES_GIVE) {
            return Err(fail(StatusCode::FORBIDDEN, "not-yours"));
        }
        let giver = w.uid;
        let res = db(&app, move |c| set_badge(c, giver, &g.login, g.badge.as_deref().unwrap_or(""), g.level.unwrap_or(1), g.on)).await;
        res.map(|_| Json(json!({})).into_response()).map_err(|m| said(StatusCode::BAD_REQUEST, "badge", &m))
    })
    .await
}

pub fn set_badge(c: &Connection, giver: i64, login: &str, badge_id: &str, level: i64, on: bool) -> Result<(), String> {
    let b = badge_of(badge_id).ok_or("no such badge")?;
    if RECODE_BADGES.contains(&b.id) && giver >= 0 && !crate::is_owner(giver) {
        return Err("Recode's badges follow a key: only she gives them by hand".into());
    }
    let uid = uid_of(c, login).ok_or_else(|| format!("no member {login}"))?;
    if on {
        let most = b.levels.len().max(1) as i64;
        award(c, uid, b.id, level.clamp(1, most), (giver >= 0).then_some(giver));
    } else {
        c.execute("DELETE FROM user_badges WHERE uid = ?1 AND badge = ?2", params![uid, b.id]).map_err(|e| e.to_string())?;
    }
    audit(c, giver, if on { "badge.give" } else { "badge.take" }, &format!("user:{uid}"), json!({ "badge": b.id, "login": login, "level": level }));
    Ok(())
}

/// The roles and the badges, for the site's and the client's own drawing of them.
async fn roles_list() -> Response {
    let roles: Vec<Value> = ROLES.iter().map(|r| json!({ "id": r.id, "en": r.en, "ru": r.ru, "prefix": r.prefix, "colour": r.colour, "icon": r.icon, "staff": r.staff })).collect();
    Json(json!({ "roles": roles, "themes": THEMES, "emoji": EMOJI, "socials": SOCIALS })).into_response()
}

// ------------------------------------------------------------------------------------------------ presence

/// Someone came, went, or changed what they show: their own other pages hear of it, and their friends, as much as
/// the member lets be seen.
pub fn presence_changed(uid: i64) {
    let here = crate::live::presence(uid);
    let v = json!({
        "t": "presence", "uid": uid,
        "online": here.as_ref().map(|p| if p.game > 0 { "game" } else { "site" }),
        "listening": here.as_ref().and_then(|p| p.listening.clone()),
    });
    crate::live::publish(uid, &v);
    let mut shown = public_presence(uid);
    shown["t"] = json!("presence");
    shown["uid"] = json!(uid);
    for friend in crate::friends::of(uid) {
        crate::live::publish(friend, &shown);
    }
}

/// Where a member is, as anyone else may see it: online (in the game or on the site), what plays, the server — each
/// as their privacy lets it be seen.
pub fn public_presence(uid: i64) -> Value {
    let card = cards().get(&uid).cloned().unwrap_or(Card { show_online: true, show_listening: true, ..Default::default() });
    match crate::live::presence(uid).filter(|_| card.show_online) {
        Some(here) => json!({
            "online": if here.game > 0 { "game" } else { "site" },
            "listening": if card.show_listening { json!(here.listening) } else { Value::Null },
            "server": if card.show_server { json!(here.server) } else { Value::Null },
        }),
        None => json!({ "online": null }),
    }
}

/// What a friend sees of the member being there: the public presence, and the game's nick where the server is shown.
pub fn friend_presence(uid: i64) -> Value {
    let mut p = public_presence(uid);
    let card = cards().get(&uid).cloned().unwrap_or(Card { show_online: true, show_listening: true, ..Default::default() });
    if card.show_online && card.show_server {
        if let Some(mc) = crate::live::presence(uid).and_then(|h| h.mc) {
            p["mc"] = json!(mc);
        }
    }
    p
}

/// Who may start a conversation with the member: "all", "friends" (the default) or "none".
pub fn dm_privacy(c: &Connection, uid: i64) -> String {
    privacy_of(&profile_row(c, uid))["dm"].as_str().unwrap_or("friends").to_string()
}

pub fn routes() -> Router<Shared> {
    let json = || DefaultBodyLimit::max(16 * 1024);
    Router::new()
        .route("/api/u/{login}", get(profile))
        .route("/api/u/{login}/shots", get(shots))
        .route("/api/people", get(many))
        .route("/api/people/meta", get(roles_list))
        .route("/api/me/profile", axum::routing::patch(edit_profile).layer(json()))
        .route("/api/me/banner", post(set_banner).delete(clear_banner).layer(DefaultBodyLimit::max(crate::media::MOST_PICTURE + 1024)))
        .route("/api/me/music", post(set_music_file).patch(edit_music).delete(clear_music).layer(DefaultBodyLimit::max(crate::media::MOST_MUSIC + 1024)))
        .route("/api/me/shots", post(add_shot).layer(DefaultBodyLimit::max(crate::media::MOST_PICTURE + 1024)))
        .route("/api/me/shots/{id}", delete(drop_shot))
        .route("/api/me/found", post(found).layer(json()))
        .route("/api/me/uid", post(set_own_uid).layer(json()))
        .route("/api/me/sessions", get(sessions))
        .route("/api/me/sessions/{id}", delete(end_session))
        .route("/api/me/sessions/others", post(end_others))
        .route("/api/badges", get(catalogue))
        .route("/api/staff/roles", post(give_role).layer(json()))
        .route("/api/staff/badges", post(give_badge).layer(json()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_uids_never_read_as_numbers() {
        for ok in ["dayn", "1000-7", "a1", "x_9", "12.5", "mentos"] {
            assert!(text_uid_ok(ok), "{ok}");
        }
        for bad in ["993", "-5", "5.", "7-", "-", "a", "1000--7", "abc def", "\u{44f}\u{44f}", "abcdefghijklmnopq"] {
            assert!(!text_uid_ok(bad), "{bad}");
        }
    }

    #[test]
    fn links_are_made_from_what_is_typed() {
        assert_eq!(social("telegram", "@crsdclient").unwrap(), "crsdclient");
        assert_eq!(social("telegram", "https://t.me/crsdclient").unwrap(), "crsdclient");
        assert!(social("telegram", "abc").is_err());
        assert_eq!(social("youtube", "https://www.youtube.com/@Lunuca").unwrap(), "@Lunuca");
        assert_eq!(social("youtube", "lunuca").unwrap(), "@lunuca");
        assert_eq!(social("steam", "https://steamcommunity.com/id/lunuca/").unwrap(), "id/lunuca");
        assert_eq!(social("github", "github.com/Lunuca").unwrap(), "Lunuca");
        assert!(social("site", "javascript:alert(1)").is_err());
        assert!(social("site", "https://cursedclient.xyz/\"onload").is_err());
        assert_eq!(social("site", "https://cursedclient.xyz").unwrap(), "https://cursedclient.xyz");
    }

    #[test]
    fn soundcloud_tracks_only() {
        assert_eq!(soundcloud_ok("https://soundcloud.com/artist/track-name?si=1").as_deref(), Some("https://soundcloud.com/artist/track-name"));
        assert!(soundcloud_ok("https://soundcloud.com/artist").is_none());
        assert!(soundcloud_ok("https://soundcloud.com/artist/sets/x").is_none());
        assert!(soundcloud_ok("https://evil.com/soundcloud.com/a/b").is_none());
    }

    fn fresh() -> (Connection, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!("cursed-people-{}.db", crate::random_hex(4)));
        let c = crate::open(path.to_str().unwrap());
        (c, path)
    }

    fn gone(c: Connection, path: std::path::PathBuf) {
        drop(c);
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
        }
    }

    fn add(c: &Connection, login: &str) -> i64 {
        c.execute("INSERT INTO users (login, login_key, email, email_key, pass, created) VALUES (?1, lower(?1), ?1 || '@e.st', lower(?1) || '@e.st', 'x', 0)", params![login]).unwrap();
        c.last_insert_rowid()
    }

    fn level(c: &Connection, uid: i64, badge: &str) -> i64 {
        c.query_row("SELECT level FROM user_badges WHERE uid = ?1 AND badge = ?2", params![uid, badge], |r| r.get(0)).optional().unwrap().unwrap_or(0)
    }

    /// Moscow time of a day (counted from 1970) at an hour: what `days_marks` reads.
    fn msk(day: i64, hour: i64) -> i64 {
        day * 86_400 + hour * 3600 - 3 * 3600
    }

    /// By the day, in either game (her word, 19.10): days in a row (a day missed starts again, the badge stays), six
    /// hours in a day, ten mornings, ten nights, and a return after a month away.
    #[test]
    fn the_days_in_the_game_are_counted() {
        let (c, path) = fresh();
        let a = add(&c, "ivy");
        let d = 20_000;
        for i in 0..6 {
            played(&c, a, 1, msk(d + i, 12));
        }
        assert_eq!(stat(&c, a, "streak"), 6);
        assert_eq!(level(&c, a, "streak"), 0);
        played_recode(&c, a, 1, msk(d + 6, 12));
        assert_eq!(level(&c, a, "streak"), 1, "the seventh day in a row, in Recode");
        played(&c, a, 1, msk(d + 6, 20));
        assert_eq!(stat(&c, a, "streak"), 7, "the same day again is not another");
        played(&c, a, 1, msk(d + 8, 12));
        assert_eq!(stat(&c, a, "streak"), 1, "a day missed");
        assert_eq!(level(&c, a, "streak"), 1, "the badge stays");
        // six hours in one day, the two games together; the next day starts from nothing
        for _ in 0..5 {
            played(&c, a, 60, msk(d + 8, 14));
        }
        assert_eq!(level(&c, a, "marathon"), 0);
        played_recode(&c, a, 59, msk(d + 8, 22));
        assert_eq!(level(&c, a, "marathon"), 1);
        let b = add(&c, "rowan");
        played(&c, b, 300, msk(d, 23));
        played(&c, b, 300, msk(d + 1, 1));
        assert_eq!(level(&c, b, "marathon"), 0, "two days are not one");
        // ten mornings from six to eight, ten nights from three to five, once a day each
        for i in 0..10 {
            played(&c, b, 1, msk(d + 10 + i, 7));
            played(&c, b, 1, msk(d + 10 + i, 7));
            played(&c, b, 1, msk(d + 10 + i, 4));
            assert_eq!(level(&c, b, "early"), i64::from(i == 9));
            assert_eq!(level(&c, b, "owl"), i64::from(i == 9));
        }
        played(&c, b, 1, msk(d + 30, 8));
        assert_eq!(stat(&c, b, "early_mornings"), 10, "eight is not the morning's any more");
        // a month away
        assert_eq!(level(&c, b, "comeback"), 0);
        played(&c, b, 1, msk(d + 59, 12));
        assert_eq!(level(&c, b, "comeback"), 0, "29 days away");
        played(&c, b, 1, msk(d + 89, 12));
        assert_eq!(level(&c, b, "comeback"), 1, "30 days away");
        // the hours of Recode apart from Cursed's
        let before = stat(&c, b, "game_min");
        played_recode(&c, b, 600, msk(d + 90, 12));
        assert_eq!(level(&c, b, "recode_hours"), 1);
        assert_eq!(stat(&c, b, "game_min"), before);
        gone(c, path);
    }

    /// Ten badges make a collector, the collector itself not among them.
    #[test]
    fn a_collector_counts_the_others() {
        let (c, path) = fresh();
        let a = add(&c, "ivy");
        let ten = ["first100", "launch", "bug_found", "bug_critical", "soul", "media", "eclipse", "comeback", "recode", "forever"];
        for (i, id) in ten.iter().enumerate() {
            assert!(award(&c, a, id, 1, None));
            assert_eq!(level(&c, a, "collector"), i64::from(i == 9), "{id}");
        }
        assert_eq!(stat(&c, a, "badges"), 10);
        gone(c, path);
    }

    /// Messages and reactions given are counted, not bumped: a reaction taken back and put again is the same one.
    #[test]
    fn messages_and_reactions_are_counted_again() {
        let (c, path) = fresh();
        let a = add(&c, "ivy");
        // (the posts themselves are not this test's: only who reacted is counted)
        c.execute_batch("PRAGMA foreign_keys = OFF").unwrap();
        c.execute("INSERT INTO post_reactions (post, uid, emoji, at) VALUES (1, ?1, 'x', 0)", params![a]).unwrap();
        recount(&c, a, "reactions_given");
        c.execute("DELETE FROM post_reactions WHERE uid = ?1", params![a]).unwrap();
        c.execute("INSERT INTO post_reactions (post, uid, emoji, at) VALUES (1, ?1, 'x', 0)", params![a]).unwrap();
        recount(&c, a, "reactions_given");
        assert_eq!(stat(&c, a, "reactions_given"), 1);
        for i in 0..49 {
            c.execute("INSERT INTO post_reactions (post, uid, emoji, at) VALUES (?2, ?1, 'x', 0)", params![a, i + 2]).unwrap();
        }
        recount(&c, a, "reactions_given");
        assert_eq!(level(&c, a, "generous"), 1);
        gone(c, path);
    }

    /// Recode's roles: nobody but her gives them by hand, an admin least of all; one that follows from a key is told
    /// to its holder and goes when it should.
    #[test]
    fn manual_roles_require_console_and_automatic_roles_are_limited() {
        let (c, path) = fresh();
        let admin = add(&c, "rook");
        c.execute("INSERT INTO roles (uid, role, since) VALUES (?1, 'admin', 0)", params![admin]).unwrap();
        refresh(&c, admin);
        for giver in [OWNER, 19, admin, -2] {
            for r in ROLES {
                for on in [true, false] {
                    assert!(set_role(&c, giver, "rook", r.id, on).is_err());
                }
            }
        }
        for r in ROLES.iter().filter(|r| !RECODE_ROLES.contains(&r.id)) {
            assert!(!follow_role(&c, admin, r.id, true));
            assert!(!follow_role(&c, admin, r.id, false));
        }
        assert!(set_role(&c, -1, "rook", "partner", true).is_ok());
        assert!(roles_of(admin).contains(&"partner".to_string()));
        assert!(set_role(&c, -1, "rook", "partner", false).is_ok());
        assert!(!roles_of(admin).contains(&"partner".to_string()));
        assert!(!can(admin, RECODE_KEYS), "an admin makes no keys without the role");
        c.execute("INSERT INTO roles (uid, role, since) VALUES (?1, 'keys', 0)", params![admin]).unwrap();
        refresh(&c, admin);
        assert!(can(admin, RECODE_KEYS), "with it, does");
        assert!(set_badge(&c, admin, "rook", "rc_beta", 1, true).is_err());
        assert!(set_badge(&c, admin, "rook", "bug_found", 1, true).is_ok());
        let a = add(&c, "ivy");
        assert!(follow_role(&c, a, "recode", true));
        assert!(!follow_role(&c, a, "recode", true), "once");
        assert!(roles_of(a).contains(&"recode".to_string()));
        assert!(follow_role(&c, a, "recode", false));
        assert!(!roles_of(a).contains(&"recode".to_string()));
        gone(c, path);
    }

    #[test]
    fn badges_of_levels_go_up_by_their_counts() {
        let b = badge_of("hours").unwrap();
        assert_eq!(b.levels.iter().take_while(|(n, _)| 6_500 >= *n).count(), 2);
        assert_eq!(rarity_at(b, 2), 1);
        assert!(BADGES.iter().all(|b| EMOJI.contains(&b.icon)));
        let mut ids: Vec<&str> = BADGES.iter().map(|b| b.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), BADGES.len());
    }

    #[test]
    fn the_prefix_is_the_highest_role_unless_chosen() {
        let card = Card { login: "x".into(), roles: vec!["moderator".into(), "media".into(), "beta".into()], prefix: "auto".into(), ..Default::default() };
    let beta = Card { login: "y".into(), roles: vec!["beta".into()], prefix: "auto".into(), ..Default::default() };
    assert!(shown_prefix(&beta).is_none(), "the beta's role shows no prefix by itself");
    assert_eq!(shown_prefix(&Card { prefix: "beta".into(), ..beta }).unwrap().id, "beta");
        assert_eq!(shown_prefix(&card).unwrap().id, "moderator");
        let chosen = Card { prefix: "media".into(), ..card.clone() };
        assert_eq!(shown_prefix(&chosen).unwrap().id, "media");
        let none = Card { prefix: "none".into(), ..card };
        assert!(shown_prefix(&none).is_none());
    }
}
