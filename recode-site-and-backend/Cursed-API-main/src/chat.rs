//! Conversations: two members' (a direct one) or a group's (up to fifty, by invitation or by link), and the messages
//! in them — text, a picture, a voice message — with replies, reactions, edits, removal, what each member has read,
//! who is typing, one message pinned. Everything new goes out on the live line to every member at once.
//!
//! Staff never read a conversation: a member who reports a message hands over that message and the ten before it,
//! as they were, and that is all staff see.
//!
//! Who may start a direct conversation is the other's choice (people.rs `dm_privacy`: everyone, friends — the
//! default — or nobody); a block either way closes it.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path as Key, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post};
use axum::{Json, Router};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::market::{busy, member, run, said};
use crate::{db, fail, now, random_hex, Shared};

const MOST_BODY: usize = 2000;
const MOST_TITLE: usize = 40;
pub const MOST_GROUP: i64 = 50;
/// Groups a member may be in.
const MOST_GROUPS: i64 = 30;
const PAGE: i64 = 50;
/// How long a message may still be edited.
const EDIT_FOR: i64 = 48 * 3600;
/// Messages a member may send in ten seconds.
const BURST: usize = 12;
/// A group of this many gives its owner the badge "Life of the party".
const SOUL: i64 = 25;

pub fn schema(c: &Connection) {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS convs (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             kind TEXT NOT NULL,                          -- dm | group
             title TEXT NOT NULL DEFAULT '',
             avatar TEXT,                                 -- the group's picture (JSON, as avatars are)
             owner INTEGER REFERENCES users(uid) ON DELETE SET NULL,
             link TEXT UNIQUE,                            -- the invitation's code, while there is one
             pinned INTEGER,                              -- a message's id
             created INTEGER NOT NULL,
             last_at INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS conv_members (
             conv INTEGER NOT NULL REFERENCES convs(id) ON DELETE CASCADE,
             uid INTEGER NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
             role TEXT NOT NULL DEFAULT 'member',         -- owner | admin | member
             joined INTEGER NOT NULL,
             read_upto INTEGER NOT NULL DEFAULT 0,
             muted INTEGER NOT NULL DEFAULT 0,
             hidden INTEGER NOT NULL DEFAULT 0,           -- put away from the list until something new comes
             PRIMARY KEY (conv, uid));
         CREATE INDEX IF NOT EXISTS conv_members_uid ON conv_members(uid);
         CREATE TABLE IF NOT EXISTS dm_pairs (
             a INTEGER NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
             b INTEGER NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
             conv INTEGER NOT NULL REFERENCES convs(id) ON DELETE CASCADE,
             PRIMARY KEY (a, b));
         CREATE TABLE IF NOT EXISTS conv_invites (
             conv INTEGER NOT NULL REFERENCES convs(id) ON DELETE CASCADE,
             uid INTEGER NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
             by INTEGER NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
             at INTEGER NOT NULL,
             PRIMARY KEY (conv, uid));
         CREATE INDEX IF NOT EXISTS conv_invites_uid ON conv_invites(uid);
         CREATE TABLE IF NOT EXISTS messages (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             conv INTEGER NOT NULL REFERENCES convs(id) ON DELETE CASCADE,
             uid INTEGER REFERENCES users(uid) ON DELETE SET NULL,
             kind TEXT NOT NULL DEFAULT 'text',           -- text | picture | voice | system
             body TEXT NOT NULL DEFAULT '',               -- the text; for system, what happened (JSON)
             media TEXT,                                  -- the picture's or the voice's info (JSON)
             reply_to INTEGER,
             created INTEGER NOT NULL,
             edited INTEGER,
             deleted INTEGER,                             -- when; its words and media are gone
             deleted_by INTEGER);
         CREATE INDEX IF NOT EXISTS messages_conv ON messages(conv, id);
         CREATE TABLE IF NOT EXISTS reactions (
             message INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
             uid INTEGER NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
             emoji TEXT NOT NULL,
             at INTEGER NOT NULL,
             PRIMARY KEY (message, uid, emoji));
         CREATE TABLE IF NOT EXISTS chat_reports (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             message INTEGER NOT NULL,
             conv INTEGER NOT NULL,
             reporter INTEGER NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
             accused INTEGER,
             why TEXT NOT NULL,
             snapshot TEXT NOT NULL,                      -- the message and the ten before it, as they were
             created INTEGER NOT NULL,
             state TEXT NOT NULL DEFAULT 'open',          -- open | done
             outcome TEXT,
             handled_by INTEGER,
             handled_at INTEGER);
         CREATE INDEX IF NOT EXISTS chat_reports_state ON chat_reports(state, created);",
    )
    .expect("make the conversations' tables");
}

// ------------------------------------------------------------------------------------------------ helpers

fn clean_text(s: &str, most: usize) -> String {
    let t: String = s.replace("\r\n", "\n").chars().filter(|c| *c == '\n' || !c.is_control()).take(most).collect();
    t.trim().to_string()
}

/// One more message from this member now, if they have not sent a burst already.
fn paced(uid: i64) -> bool {
    static BOOK: OnceLock<Mutex<HashMap<i64, Vec<i64>>>> = OnceLock::new();
    let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
    let mut b = BOOK.get_or_init(|| Mutex::new(HashMap::new())).lock().unwrap_or_else(|e| e.into_inner());
    let list = b.entry(uid).or_default();
    list.retain(|t| at - t < 10_000);
    if list.len() >= BURST {
        return false;
    }
    list.push(at);
    true
}

/// A member's place in a conversation: its kind and their role, if they are in it.
fn place(c: &Connection, conv: i64, uid: i64) -> Option<(String, String)> {
    c.query_row("SELECT v.kind, m.role FROM conv_members m JOIN convs v ON v.id = m.conv WHERE m.conv = ?1 AND m.uid = ?2", params![conv, uid], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()
        .ok()
        .flatten()
}

fn members(c: &Connection, conv: i64) -> Vec<i64> {
    c.prepare("SELECT uid FROM conv_members WHERE conv = ?1").and_then(|mut q| q.query_map(params![conv], |r| r.get(0)).map(|m| m.flatten().collect())).unwrap_or_default()
}

fn count(c: &Connection, conv: i64) -> i64 {
    c.query_row("SELECT COUNT(*) FROM conv_members WHERE conv = ?1", params![conv], |r| r.get(0)).unwrap_or(0)
}

/// The other one of a direct conversation.
fn other_of(c: &Connection, conv: i64, uid: i64) -> Option<i64> {
    c.query_row("SELECT uid FROM conv_members WHERE conv = ?1 AND uid != ?2", params![conv, uid], |r| r.get(0)).optional().ok().flatten()
}

/// Whether `from` may write to `to` directly: no block either way, and the other's privacy lets them (staff always).
fn may_write(c: &Connection, from: i64, to: i64) -> Result<(), Response> {
    if crate::friends::apart(c, from, to) {
        return Err(said(StatusCode::FORBIDDEN, "blocked", "one of you has blocked the other"));
    }
    if crate::people::is_staff(from) {
        return Ok(());
    }
    match crate::people::dm_privacy(c, to).as_str() {
        "all" => Ok(()),
        "none" => Err(said(StatusCode::FORBIDDEN, "closed", "they take no messages")),
        _ if crate::friends::are_friends(from, to) => Ok(()),
        _ => Err(said(StatusCode::FORBIDDEN, "friends", "they take messages from friends only")),
    }
}

fn may_manage(role: &str) -> bool {
    role == "owner" || role == "admin"
}

// ------------------------------------------------------------------------------------------------ what is shown

const MESSAGE: &str = "SELECT id, conv, uid, kind, body, media, reply_to, created, edited, deleted FROM messages";

struct Msg {
    id: i64,
    conv: i64,
    uid: Option<i64>,
    kind: String,
    body: String,
    media: Option<String>,
    reply_to: Option<i64>,
    created: i64,
    edited: Option<i64>,
    deleted: Option<i64>,
}

fn msg_row(r: &Row) -> rusqlite::Result<Msg> {
    Ok(Msg {
        id: r.get(0)?,
        conv: r.get(1)?,
        uid: r.get(2)?,
        kind: r.get(3)?,
        body: r.get(4)?,
        media: r.get(5)?,
        reply_to: r.get(6)?,
        created: r.get(7)?,
        edited: r.get(8)?,
        deleted: r.get(9)?,
    })
}

fn message(c: &Connection, id: i64) -> Option<Msg> {
    c.query_row(&format!("{MESSAGE} WHERE id = ?1"), params![id], msg_row).optional().ok().flatten()
}

/// A message's reactions: each emoji with who put it.
fn reactions(c: &Connection, id: i64) -> Value {
    let rows: Vec<(String, i64)> = c
        .prepare("SELECT emoji, uid FROM reactions WHERE message = ?1 ORDER BY at")
        .and_then(|mut q| q.query_map(params![id], |r| Ok((r.get(0)?, r.get(1)?))).map(|m| m.flatten().collect()))
        .unwrap_or_default();
    let mut order: Vec<String> = Vec::new();
    let mut who: HashMap<String, Vec<i64>> = HashMap::new();
    for (e, u) in rows {
        if !who.contains_key(&e) {
            order.push(e.clone());
        }
        who.entry(e).or_default().push(u);
    }
    json!(order.into_iter().map(|e| json!({ "emoji": e, "uids": who.remove(&e).unwrap_or_default() })).collect::<Vec<_>>())
}

/// A short line of a message (for a reply's quote, a list's last line, a notification).
fn brief(c: &Connection, m: &Msg) -> Value {
    let author = m.uid.map(|u| crate::people::card_of(c, u)["login"].clone()).unwrap_or(Value::Null);
    let text = if m.deleted.is_some() { String::new() } else if m.kind == "text" { crate::notify::excerpt(&m.body) } else { String::new() };
    json!({ "id": m.id, "author": author, "uid": m.uid, "kind": m.kind, "text": text, "deleted": m.deleted.is_some(), "created": m.created,
            "system": if m.kind == "system" { serde_json::from_str::<Value>(&m.body).unwrap_or(Value::Null) } else { Value::Null } })
}

fn message_json(c: &Connection, m: &Msg) -> Value {
    let gone = m.deleted.is_some();
    let reply = m.reply_to.and_then(|r| message(c, r)).filter(|r| r.conv == m.conv).map(|r| brief(c, &r));
    json!({
        "id": m.id, "chat": m.conv, "kind": m.kind,
        "author": m.uid.map(|u| crate::people::card_of(c, u)).unwrap_or(Value::Null),
        "body": if gone { Value::Null } else if m.kind == "system" { serde_json::from_str::<Value>(&m.body).unwrap_or(Value::Null) } else { json!(m.body) },
        "media": if gone { Value::Null } else { m.media.as_deref().and_then(|x| serde_json::from_str::<Value>(x).ok()).unwrap_or(Value::Null) },
        "reply": reply, "created": m.created, "edited": m.edited, "deleted": gone,
        "reactions": if gone { json!([]) } else { reactions(c, m.id) },
    })
}

/// A conversation as one member sees it in their list and at its top.
fn chat_json(c: &Connection, conv: i64, viewer: i64) -> Option<Value> {
    let (kind, title, avatar, owner, link, pinned, created, last_at): (String, String, Option<String>, Option<i64>, Option<String>, Option<i64>, i64, i64) = c
        .query_row("SELECT kind, title, avatar, owner, link, pinned, created, last_at FROM convs WHERE id = ?1", params![conv], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?))
        })
        .optional()
        .ok()
        .flatten()?;
    let (role, read_upto, muted): (String, i64, bool) =
        c.query_row("SELECT role, read_upto, muted FROM conv_members WHERE conv = ?1 AND uid = ?2", params![conv, viewer], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).optional().ok().flatten()?;
    let unread: i64 = c
        .query_row("SELECT COUNT(*) FROM messages WHERE conv = ?1 AND id > ?2 AND (uid IS NULL OR uid != ?3) AND deleted IS NULL AND kind != 'system'", params![conv, read_upto, viewer], |r| r.get(0))
        .unwrap_or(0);
    let last = c.query_row(&format!("{MESSAGE} WHERE conv = ?1 ORDER BY id DESC LIMIT 1"), params![conv], msg_row).optional().ok().flatten().map(|m| brief(c, &m));
    let other = if kind == "dm" { other_of(c, conv, viewer) } else { None };
    // How far the other one has read (a direct conversation's "read").
    let their_read: Option<i64> = other.and_then(|o| c.query_row("SELECT read_upto FROM conv_members WHERE conv = ?1 AND uid = ?2", params![conv, o], |r| r.get(0)).optional().ok().flatten());
    let manage = may_manage(&role);
    Some(json!({
        "id": conv, "kind": kind, "title": title,
        "avatar": avatar.as_deref().and_then(|a| serde_json::from_str::<Value>(a).ok()),
        "other": other.map(|o| {
            let mut card = crate::people::card_of(c, o);
            card["presence"] = crate::people::public_presence(o);
            card["friend"] = json!(crate::friends::are_friends(viewer, o));
            card
        }),
        "owner": owner, "role": role, "muted": muted, "unread": unread, "read_upto": read_upto, "their_read": their_read,
        "members": count(c, conv), "link": if manage { link.map(Value::from).unwrap_or(Value::Null) } else { Value::Null },
        "pinned": pinned.and_then(|p| message(c, p)).filter(|m| m.deleted.is_none()).map(|m| message_json(c, &m)),
        "created": created, "last_at": last_at, "last": last,
    }))
}

/// A new message written, and every member told: its author's pages with it, the others as their settings say (in
/// the game, on Windows) unless they muted the conversation.
fn send(c: &Connection, conv: i64, uid: Option<i64>, kind: &str, body: &str, media: Option<&Value>, reply_to: Option<i64>) -> rusqlite::Result<Value> {
    let at = now();
    c.execute(
        "INSERT INTO messages (conv, uid, kind, body, media, reply_to, created) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![conv, uid, kind, body, media.map(|m| m.to_string()), reply_to, at],
    )?;
    let id = c.last_insert_rowid();
    c.execute("UPDATE convs SET last_at = ?2 WHERE id = ?1", params![conv, at])?;
    // A conversation put away comes back with anything new; its author has read what they wrote.
    c.execute("UPDATE conv_members SET hidden = 0 WHERE conv = ?1", params![conv])?;
    if let Some(u) = uid {
        c.execute("UPDATE conv_members SET read_upto = ?3 WHERE conv = ?1 AND uid = ?2", params![conv, u, id])?;
        if kind != "system" {
            crate::people::recount(c, u, "messages");
        }
    }
    let (kind_of, title): (String, String) = c.query_row("SELECT kind, title FROM convs WHERE id = ?1", params![conv], |r| Ok((r.get(0)?, r.get(1)?))).unwrap_or_default();
    let direct = kind_of == "dm";
    let m = message(c, id).expect("the message just written");
    let shown = message_json(c, &m);
    let muted: HashMap<i64, bool> = c
        .prepare("SELECT uid, muted FROM conv_members WHERE conv = ?1")?
        .query_map(params![conv], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, bool>(1)?)))?
        .flatten()
        .collect();
    for (member, muted) in muted {
        let own = Some(member) == uid;
        let (_, game, push) = crate::notify::wants(c, member, "message");
        let loud = !own && !muted && kind != "system";
        crate::live::publish(member, &json!({ "t": "msg", "chat": conv, "title": if direct { "" } else { title.as_str() }, "message": shown, "game": loud && game, "push": loud && push, "unread": unread_total(c, member) }));
        // Who writes has read all before it: the other one of a direct conversation sees "read" move with it.
        if direct && !own {
            if let Some(u) = uid {
                crate::live::publish(member, &json!({ "t": "read", "chat": conv, "uid": u, "upto": id }));
            }
        }
    }
    Ok(shown)
}

/// What a group's history says happened (created, joined, left, renamed…), with who did it.
fn happened(c: &Connection, conv: i64, what: &str, who: i64, whom: Option<i64>, extra: Value) {
    let mut v = json!({ "what": what, "who": crate::people::card_of(c, who)["login"], "whom": whom.map(|w| crate::people::card_of(c, w)["login"].clone()) });
    if let Value::Object(more) = extra {
        for (k, x) in more {
            v[k] = x;
        }
    }
    let _ = send(c, conv, None, "system", &v.to_string(), None, None);
}

/// Every member's list hears that this conversation changed (its title, its members, its pin).
fn retell(c: &Connection, conv: i64) {
    for m in members(c, conv) {
        if let Some(v) = chat_json(c, conv, m) {
            crate::live::publish(m, &json!({ "t": "chat", "chat": v }));
        }
    }
}

/// All of a member's unread messages (the Friends tab's count).
pub fn unread_total(c: &Connection, uid: i64) -> i64 {
    c.query_row(
        "SELECT COUNT(*) FROM messages g JOIN conv_members m ON m.conv = g.conv AND m.uid = ?1
         WHERE g.id > m.read_upto AND (g.uid IS NULL OR g.uid != ?1) AND g.deleted IS NULL AND g.kind != 'system' AND m.muted = 0",
        params![uid],
        |r| r.get(0),
    )
    .unwrap_or(0)
}

// ------------------------------------------------------------------------------------------------ the list

/// The member's conversations, newest first, and the groups they are invited to.
async fn list(State(app): State<Shared>, h: HeaderMap) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        let uid = w.uid;
        let v = db(&app, move |c| -> rusqlite::Result<Value> {
            let ids: Vec<i64> = c
                .prepare("SELECT m.conv FROM conv_members m JOIN convs v ON v.id = m.conv WHERE m.uid = ?1 AND m.hidden = 0 ORDER BY v.last_at DESC")?
                .query_map(params![uid], |r| r.get(0))?
                .flatten()
                .collect();
            let chats: Vec<Value> = ids.into_iter().filter_map(|id| chat_json(c, id, uid)).collect();
            let invites: Vec<Value> = c
                .prepare("SELECT i.conv, i.by, i.at, v.title, v.avatar FROM conv_invites i JOIN convs v ON v.id = i.conv WHERE i.uid = ?1 ORDER BY i.at DESC")?
                .query_map(params![uid], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, r.get::<_, String>(3)?, r.get::<_, Option<String>>(4)?)))?
                .flatten()
                .map(|(conv, by, at, title, avatar)| {
                    json!({ "chat": conv, "title": title, "avatar": avatar.and_then(|a| serde_json::from_str::<Value>(&a).ok()), "by": crate::people::card_of(c, by), "at": at, "members": count(c, conv) })
                })
                .collect();
            Ok(json!({ "chats": chats, "invites": invites, "unread": unread_total(c, uid) }))
        })
        .await
        .map_err(|_| busy())?;
        Ok(Json(v).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct With {
    login: String,
}

/// The direct conversation with someone: the one there is, or a new one (if they take messages from the member).
async fn open_dm(State(app): State<Shared>, h: HeaderMap, Json(b): Json<With>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        let login = b.login.trim().to_string();
        let v = db(&app, move |c| -> Result<Value, Response> {
            let other = crate::people::uid_of(c, &login).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            if other == uid {
                return Err(said(StatusCode::BAD_REQUEST, "self", "that is you"));
            }
            let (a, b) = (uid.min(other), uid.max(other));
            let there: Option<i64> = c
                .query_row("SELECT conv FROM dm_pairs WHERE (a = ?1 AND b = ?2) OR (a = ?2 AND b = ?1)", params![a, b], |r| r.get(0))
                .optional()
                .map_err(|_| busy())?;
            let conv = match there {
                Some(conv) => {
                    c.execute("UPDATE conv_members SET hidden = 0 WHERE conv = ?1 AND uid = ?2", params![conv, uid]).map_err(|_| busy())?;
                    conv
                }
                None => {
                    may_write(c, uid, other)?;
                    let at = now();
                    c.execute("INSERT INTO convs (kind, created, last_at) VALUES ('dm', ?1, ?1)", params![at]).map_err(|_| busy())?;
                    let conv = c.last_insert_rowid();
                    c.execute("INSERT INTO conv_members (conv, uid, joined) VALUES (?1, ?2, ?4), (?1, ?3, ?4)", params![conv, uid, other, at]).map_err(|_| busy())?;
                    c.execute("INSERT INTO dm_pairs (a, b, conv) VALUES (?1, ?2, ?3)", params![a, b, conv]).map_err(|_| busy())?;
                    conv
                }
            };
            chat_json(c, conv, uid).ok_or_else(busy)
        })
        .await?;
        Ok(Json(json!({ "chat": v })).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct NewGroup {
    title: String,
    #[serde(default)]
    invite: Vec<String>,
}

/// A group made, its maker its owner; those named are invited.
async fn new_group(State(app): State<Shared>, h: HeaderMap, Json(b): Json<NewGroup>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        let title = crate::market::clean_line(&b.title, MOST_TITLE);
        if title.chars().count() < 2 {
            return Err(said(StatusCode::BAD_REQUEST, "title", "a name of 2 to 40 letters"));
        }
        let (v, told) = db(&app, move |c| -> Result<(Value, Vec<i64>), Response> {
            let groups: i64 = c
                .query_row("SELECT COUNT(*) FROM conv_members m JOIN convs v ON v.id = m.conv WHERE m.uid = ?1 AND v.kind = 'group'", params![uid], |r| r.get(0))
                .unwrap_or(0);
            if groups >= MOST_GROUPS {
                return Err(said(StatusCode::BAD_REQUEST, "many", "you are in 30 groups already"));
            }
            let at = now();
            c.execute("INSERT INTO convs (kind, title, owner, created, last_at) VALUES ('group', ?1, ?2, ?3, ?3)", params![title, uid, at]).map_err(|_| busy())?;
            let conv = c.last_insert_rowid();
            c.execute("INSERT INTO conv_members (conv, uid, role, joined) VALUES (?1, ?2, 'owner', ?3)", params![conv, uid, at]).map_err(|_| busy())?;
            happened(c, conv, "created", uid, None, json!({ "title": title }));
            let mut told = Vec::new();
            for login in b.invite.iter().take((MOST_GROUP - 1) as usize) {
                if let Some(other) = crate::people::uid_of(c, login.trim()) {
                    if invite_one(c, conv, uid, other).is_ok() {
                        told.push(other);
                    }
                }
            }
            Ok((chat_json(c, conv, uid).ok_or_else(busy)?, told))
        })
        .await?;
        for u in told {
            crate::live::publish(u, &json!({ "t": "chats" }));
        }
        Ok((StatusCode::CREATED, Json(json!({ "chat": v }))).into_response())
    })
    .await
}

/// Someone invited to a group (not one who blocked the inviter, nor one already in it).
fn invite_one(c: &Connection, conv: i64, by: i64, uid: i64) -> Result<(), Response> {
    if uid == by || place(c, conv, uid).is_some() {
        return Err(said(StatusCode::BAD_REQUEST, "member", "already in the group"));
    }
    if crate::friends::apart(c, by, uid) {
        return Err(said(StatusCode::FORBIDDEN, "blocked", "one of you has blocked the other"));
    }
    if !crate::friends::are_friends(by, uid) && crate::people::dm_privacy(c, uid) != "all" && !crate::people::is_staff(by) {
        return Err(said(StatusCode::FORBIDDEN, "friends", "only friends can invite them"));
    }
    if count(c, conv) >= MOST_GROUP {
        return Err(said(StatusCode::BAD_REQUEST, "full", "a group holds 50"));
    }
    c.execute("INSERT OR IGNORE INTO conv_invites (conv, uid, by, at) VALUES (?1, ?2, ?3, ?4)", params![conv, uid, by, now()]).map_err(|_| busy())?;
    let title: String = c.query_row("SELECT title FROM convs WHERE id = ?1", params![conv], |r| r.get(0)).unwrap_or_default();
    crate::notify::add_or_refresh(c, uid, "group", Some(by), &format!("group:{conv}"), json!({ "chat": conv, "title": title }));
    Ok(())
}

// ------------------------------------------------------------------------------------------------ one conversation

#[derive(Deserialize)]
struct Page {
    before: Option<i64>,
    after: Option<i64>,
}

/// A page of a conversation: fifty messages before `before` (the newest without it), or those after `after`; who is
/// in it, for a group.
async fn page(State(app): State<Shared>, h: HeaderMap, Key(conv): Key<i64>, Query(q): Query<Page>) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        let uid = w.uid;
        let v = db(&app, move |c| -> Result<Value, Response> {
            let (kind, _) = place(c, conv, uid).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            let mut rows: Vec<Msg> = match (q.before, q.after) {
                (_, Some(after)) => c.prepare(&format!("{MESSAGE} WHERE conv = ?1 AND id > ?2 ORDER BY id LIMIT ?3")).and_then(|mut s| s.query_map(params![conv, after, PAGE], msg_row).map(|m| m.flatten().collect())),
                (Some(before), None) => c.prepare(&format!("{MESSAGE} WHERE conv = ?1 AND id < ?2 ORDER BY id DESC LIMIT ?3")).and_then(|mut s| s.query_map(params![conv, before, PAGE], msg_row).map(|m| m.flatten().collect())),
                (None, None) => c.prepare(&format!("{MESSAGE} WHERE conv = ?1 ORDER BY id DESC LIMIT ?2")).and_then(|mut s| s.query_map(params![conv, PAGE], msg_row).map(|m| m.flatten().collect())),
            }
            .map_err(|_| busy())?;
            rows.sort_by_key(|m| m.id);
            let more = rows.len() as i64 == PAGE;
            let messages: Vec<Value> = rows.iter().map(|m| message_json(c, m)).collect();
            let people: Vec<Value> = if kind == "group" {
                c.prepare("SELECT uid, role, read_upto FROM conv_members WHERE conv = ?1 ORDER BY CASE role WHEN 'owner' THEN 0 WHEN 'admin' THEN 1 ELSE 2 END, joined")
                    .and_then(|mut s| s.query_map(params![conv], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?))).map(|m| m.flatten().collect::<Vec<_>>()))
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(u, role, read)| {
                        let mut card = crate::people::card_of(c, u);
                        card["role"] = json!(role);
                        card["read_upto"] = json!(read);
                        card
                    })
                    .collect()
            } else {
                Vec::new()
            };
            Ok(json!({ "chat": chat_json(c, conv, uid), "messages": messages, "more": more, "people": people }))
        })
        .await?;
        Ok(Json(v).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct Write {
    body: String,
    reply_to: Option<i64>,
}

/// Before any message: the member is in the conversation, may write in it now, and is not sending too fast.
fn may_send(c: &Connection, conv: i64, uid: i64) -> Result<String, Response> {
    let (kind, _) = place(c, conv, uid).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
    if kind == "dm" {
        if let Some(other) = other_of(c, conv, uid) {
            may_write(c, uid, other)?;
        }
    }
    if !paced(uid) {
        return Err(fail(StatusCode::TOO_MANY_REQUESTS, "slow"));
    }
    Ok(kind)
}

fn reply_in(c: &Connection, conv: i64, reply_to: Option<i64>) -> Option<i64> {
    reply_to.and_then(|r| message(c, r)).filter(|m| m.conv == conv && m.deleted.is_none()).map(|m| m.id)
}

async fn write(State(app): State<Shared>, h: HeaderMap, Key(conv): Key<i64>, Json(b): Json<Write>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        let body = clean_text(&b.body, MOST_BODY);
        if body.is_empty() {
            return Err(said(StatusCode::BAD_REQUEST, "body", "an empty message"));
        }
        let v = db(&app, move |c| -> Result<Value, Response> {
            may_send(c, conv, uid)?;
            let reply = reply_in(c, conv, b.reply_to);
            send(c, conv, Some(uid), "text", &body, None, reply).map_err(|_| busy())
        })
        .await?;
        Ok((StatusCode::CREATED, Json(json!({ "message": v }))).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct Attach {
    reply_to: Option<i64>,
    /// A voice message's length as recorded, in milliseconds (an Ogg's is read from it).
    ms: Option<i64>,
}

/// A picture sent (a screenshot, a photo; a GIF is taken as its first frame).
async fn picture(State(app): State<Shared>, h: HeaderMap, Key(conv): Key<i64>, Query(q): Query<Attach>, body: Bytes) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        db(&app, move |c| may_send(c, conv, uid)).await?;
        let info = upload(&app, uid, body, "chat").await?;
        let v = db(&app, move |c| -> Result<Value, Response> {
            let reply = reply_in(c, conv, q.reply_to);
            send(c, conv, Some(uid), "picture", "", Some(&info), reply).map_err(|_| busy())
        })
        .await?;
        Ok((StatusCode::CREATED, Json(json!({ "message": v }))).into_response())
    })
    .await
}

/// A voice message sent (Opus in Ogg or in WebM, as the client and the browsers record it; three minutes at most).
async fn voice(State(app): State<Shared>, h: HeaderMap, Key(conv): Key<i64>, Query(q): Query<Attach>, body: Bytes) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        db(&app, move |c| may_send(c, conv, uid)).await?;
        let ms = q.ms.unwrap_or(0);
        let media = app.market.media.clone();
        let made = tokio::task::spawn_blocking(move || crate::media::voice(&body, ms, &media))
            .await
            .map_err(|_| busy())?
            .map_err(|m| said(StatusCode::BAD_REQUEST, "file", &m))?;
        let v = db(&app, move |c| -> Result<Value, Response> {
            if crate::media::quota_left(c, uid) < made.bytes as i64 {
                return Err(said(StatusCode::INSUFFICIENT_STORAGE, "quota", "your uploads take all of your 300 MB: take some away first"));
            }
            let info = crate::media::record(c, uid, "voice", &made).map_err(|_| busy())?;
            let reply = reply_in(c, conv, q.reply_to);
            send(c, conv, Some(uid), "voice", "", Some(&info), reply).map_err(|_| busy())
        })
        .await?;
        Ok((StatusCode::CREATED, Json(json!({ "message": v }))).into_response())
    })
    .await
}

/// A picture made and recorded under its sender (it counts in their 300 MB).
async fn upload(app: &Shared, uid: i64, body: Bytes, kind: &'static str) -> Result<Value, Response> {
    if body.is_empty() || body.len() > crate::media::MOST_PICTURE {
        return Err(said(StatusCode::BAD_REQUEST, "file", "a picture of at most 12 MB"));
    }
    if !crate::media::paced(uid) {
        return Err(fail(StatusCode::TOO_MANY_REQUESTS, "slow"));
    }
    let left = db(app, move |c| crate::media::quota_left(c, uid)).await;
    if left < body.len() as i64 {
        return Err(said(StatusCode::INSUFFICIENT_STORAGE, "quota", "your uploads take all of your 300 MB: take some away first"));
    }
    let media = app.market.media.clone();
    let made = tokio::task::spawn_blocking(move || crate::media::chat_picture(&body, &media))
        .await
        .map_err(|_| busy())?
        .map_err(|m| said(StatusCode::BAD_REQUEST, "file", &m))?;
    db(app, move |c| crate::media::record(c, uid, kind, &made)).await.map_err(|_| busy())
}

#[derive(Deserialize)]
struct Edit {
    body: String,
}

/// One's own text message changed (within two days).
async fn edit(State(app): State<Shared>, h: HeaderMap, Key((conv, id)): Key<(i64, i64)>, Json(b): Json<Edit>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        let body = clean_text(&b.body, MOST_BODY);
        if body.is_empty() {
            return Err(said(StatusCode::BAD_REQUEST, "body", "an empty message"));
        }
        db(&app, move |c| -> Result<(), Response> {
            place(c, conv, uid).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            let m = message(c, id).filter(|m| m.conv == conv && m.deleted.is_none()).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            if m.uid != Some(uid) || m.kind != "text" {
                return Err(fail(StatusCode::FORBIDDEN, "not-yours"));
            }
            if now() - m.created > EDIT_FOR {
                return Err(said(StatusCode::BAD_REQUEST, "old", "a message can be changed for two days"));
            }
            c.execute("UPDATE messages SET body = ?2, edited = ?3 WHERE id = ?1", params![id, body, now()]).map_err(|_| busy())?;
            let shown = message_json(c, &message(c, id).ok_or_else(busy)?);
            for u in members(c, conv) {
                crate::live::publish(u, &json!({ "t": "msg_edit", "chat": conv, "message": shown }));
            }
            Ok(())
        })
        .await?;
        Ok(Json(json!({})).into_response())
    })
    .await
}

/// A message taken away: by its author, or in a group by its owner and admins.
async fn remove(State(app): State<Shared>, h: HeaderMap, Key((conv, id)): Key<(i64, i64)>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        db(&app, move |c| -> Result<(), Response> {
            let (kind, role) = place(c, conv, uid).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            let m = message(c, id).filter(|m| m.conv == conv && m.deleted.is_none() && m.kind != "system").ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            if m.uid != Some(uid) && !(kind == "group" && may_manage(&role)) {
                return Err(fail(StatusCode::FORBIDDEN, "not-yours"));
            }
            take_away(c, &m, uid).map_err(|_| busy())
        })
        .await?;
        Ok(Json(json!({})).into_response())
    })
    .await
}

/// A message's words and media gone (its place in the history stays, "removed"), the pin with it.
fn take_away(c: &Connection, m: &Msg, by: i64) -> rusqlite::Result<()> {
    c.execute("UPDATE messages SET body = '', deleted = ?2, deleted_by = ?3 WHERE id = ?1", params![m.id, now(), by])?;
    if let Some(id) = m.media.as_deref().and_then(|x| serde_json::from_str::<Value>(x).ok()).and_then(|v| v["id"].as_str().map(String::from)) {
        crate::media::remove(c, &id, None)?;
    }
    c.execute("DELETE FROM reactions WHERE message = ?1", params![m.id])?;
    let unpinned = c.execute("UPDATE convs SET pinned = NULL WHERE id = ?1 AND pinned = ?2", params![m.conv, m.id])? > 0;
    for u in members(c, m.conv) {
        crate::live::publish(u, &json!({ "t": "msg_del", "chat": m.conv, "id": m.id }));
    }
    if unpinned {
        retell(c, m.conv);
    }
    Ok(())
}

#[derive(Deserialize)]
struct React {
    emoji: String,
}

/// A reaction put, or taken back if it was there (the pack's emoji only).
async fn react(State(app): State<Shared>, h: HeaderMap, Key((conv, id)): Key<(i64, i64)>, Json(b): Json<React>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        if !crate::people::EMOJI.contains(&b.emoji.as_str()) {
            return Err(said(StatusCode::BAD_REQUEST, "emoji", "one of the pack's emoji"));
        }
        db(&app, move |c| -> Result<(), Response> {
            place(c, conv, uid).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            message(c, id).filter(|m| m.conv == conv && m.deleted.is_none() && m.kind != "system").ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            let gone = c.execute("DELETE FROM reactions WHERE message = ?1 AND uid = ?2 AND emoji = ?3", params![id, uid, b.emoji]).map_err(|_| busy())?;
            if gone == 0 {
                let kinds: i64 = c.query_row("SELECT COUNT(DISTINCT emoji) FROM reactions WHERE message = ?1", params![id], |r| r.get(0)).unwrap_or(0);
                if kinds >= 12 {
                    return Err(said(StatusCode::BAD_REQUEST, "many", "twelve kinds of reaction at most"));
                }
                c.execute("INSERT INTO reactions (message, uid, emoji, at) VALUES (?1, ?2, ?3, ?4)", params![id, uid, b.emoji, now()]).map_err(|_| busy())?;
                crate::people::recount(c, uid, "reactions_given");
            }
            let r = reactions(c, id);
            for u in members(c, conv) {
                crate::live::publish(u, &json!({ "t": "react", "chat": conv, "id": id, "reactions": r }));
            }
            Ok(())
        })
        .await?;
        Ok(Json(json!({})).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct Upto {
    upto: i64,
}

/// What the member has read, up to a message (never back); the others of a direct conversation see "read".
async fn read(State(app): State<Shared>, h: HeaderMap, Key(conv): Key<i64>, Json(b): Json<Upto>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        db(&app, move |c| -> Result<(), Response> {
            place(c, conv, uid).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            let top: i64 = c.query_row("SELECT COALESCE(MAX(id), 0) FROM messages WHERE conv = ?1", params![conv], |r| r.get(0)).unwrap_or(0);
            let upto = b.upto.min(top);
            let moved = c.execute("UPDATE conv_members SET read_upto = ?3 WHERE conv = ?1 AND uid = ?2 AND read_upto < ?3", params![conv, uid, upto]).map_err(|_| busy())?;
            if moved > 0 {
                let total = unread_total(c, uid);
                for u in members(c, conv) {
                    let mut e = json!({ "t": "read", "chat": conv, "uid": uid, "upto": upto });
                    if u == uid {
                        e["unread"] = json!(total);
                    }
                    crate::live::publish(u, &e);
                }
            }
            Ok(())
        })
        .await?;
        Ok(Json(json!({})).into_response())
    })
    .await
}

/// "Typing…": the others hear it (a few seconds' worth; said again while it lasts).
async fn typing(State(app): State<Shared>, h: HeaderMap, Key(conv): Key<i64>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        let login = w.login.clone();
        db(&app, move |c| -> Result<(), Response> {
            place(c, conv, uid).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            for u in members(c, conv).into_iter().filter(|u| *u != uid) {
                crate::live::publish(u, &json!({ "t": "typing", "chat": conv, "uid": uid, "login": login }));
            }
            Ok(())
        })
        .await?;
        Ok(Json(json!({})).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct Pin {
    message: Option<i64>,
}

/// One message pinned at the top (or none): in a direct conversation by either, in a group by its owner and admins.
async fn pin(State(app): State<Shared>, h: HeaderMap, Key(conv): Key<i64>, Json(b): Json<Pin>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        db(&app, move |c| -> Result<(), Response> {
            let (kind, role) = place(c, conv, uid).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            if kind == "group" && !may_manage(&role) {
                return Err(fail(StatusCode::FORBIDDEN, "admins"));
            }
            if let Some(id) = b.message {
                message(c, id).filter(|m| m.conv == conv && m.deleted.is_none() && m.kind != "system").ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            }
            c.execute("UPDATE convs SET pinned = ?2 WHERE id = ?1", params![conv, b.message]).map_err(|_| busy())?;
            if b.message.is_some() && kind == "group" {
                happened(c, conv, "pinned", uid, None, json!({}));
            }
            retell(c, conv);
            Ok(())
        })
        .await?;
        Ok(Json(json!({})).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct Settings {
    title: Option<String>,
    muted: Option<bool>,
    hidden: Option<bool>,
}

/// The member's own settings of a conversation (muted, put away), and a group's name (its owner and admins).
async fn settings(State(app): State<Shared>, h: HeaderMap, Key(conv): Key<i64>, Json(b): Json<Settings>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        db(&app, move |c| -> Result<(), Response> {
            let (kind, role) = place(c, conv, uid).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            if let Some(m) = b.muted {
                c.execute("UPDATE conv_members SET muted = ?3 WHERE conv = ?1 AND uid = ?2", params![conv, uid, m]).map_err(|_| busy())?;
            }
            if let Some(hd) = b.hidden {
                c.execute("UPDATE conv_members SET hidden = ?3 WHERE conv = ?1 AND uid = ?2", params![conv, uid, hd]).map_err(|_| busy())?;
            }
            if let Some(t) = b.title {
                if kind != "group" || !may_manage(&role) {
                    return Err(fail(StatusCode::FORBIDDEN, "admins"));
                }
                let t = crate::market::clean_line(&t, MOST_TITLE);
                if t.chars().count() < 2 {
                    return Err(said(StatusCode::BAD_REQUEST, "title", "a name of 2 to 40 letters"));
                }
                c.execute("UPDATE convs SET title = ?2 WHERE id = ?1", params![conv, t]).map_err(|_| busy())?;
                happened(c, conv, "title", uid, None, json!({ "title": t }));
                retell(c, conv);
            } else if let Some(v) = chat_json(c, conv, uid) {
                crate::live::publish(uid, &json!({ "t": "chat", "chat": v, "unread": unread_total(c, uid) }));
            }
            Ok(())
        })
        .await?;
        Ok(Json(json!({})).into_response())
    })
    .await
}

/// A group's picture (as avatars are made: square, a GIF moves).
async fn group_avatar(State(app): State<Shared>, h: HeaderMap, Key(conv): Key<i64>, body: Bytes) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        db(&app, move |c| -> Result<(), Response> {
            let (kind, role) = place(c, conv, uid).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            if kind != "group" || !may_manage(&role) {
                return Err(fail(StatusCode::FORBIDDEN, "admins"));
            }
            Ok(())
        })
        .await?;
        if body.is_empty() || body.len() > 8 << 20 {
            return Err(said(StatusCode::BAD_REQUEST, "file", "a picture of at most 8 MB"));
        }
        if !crate::media::paced(uid) {
            return Err(fail(StatusCode::TOO_MANY_REQUESTS, "slow"));
        }
        let media = app.market.media.clone();
        let made = tokio::task::spawn_blocking(move || crate::avatar::make(&body, &media))
            .await
            .map_err(|_| busy())?
            .map_err(|m| said(StatusCode::BAD_REQUEST, "file", &m))?;
        db(&app, move |c| -> Result<(), Response> {
            c.execute("UPDATE convs SET avatar = ?2 WHERE id = ?1", params![conv, made.to_string()]).map_err(|_| busy())?;
            happened(c, conv, "avatar", uid, None, json!({}));
            retell(c, conv);
            Ok(())
        })
        .await?;
        Ok(Json(json!({})).into_response())
    })
    .await
}

// ------------------------------------------------------------------------------------------------ who is in a group

/// Someone invited (by the group's owner and admins).
async fn invite(State(app): State<Shared>, h: HeaderMap, Key(conv): Key<i64>, Json(b): Json<With>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        let login = b.login.trim().to_string();
        let other = db(&app, move |c| -> Result<i64, Response> {
            let (kind, role) = place(c, conv, uid).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            if kind != "group" || !may_manage(&role) {
                return Err(fail(StatusCode::FORBIDDEN, "admins"));
            }
            let other = crate::people::uid_of(c, &login).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            invite_one(c, conv, uid, other)?;
            Ok(other)
        })
        .await?;
        crate::live::publish(other, &json!({ "t": "chats" }));
        Ok(Json(json!({})).into_response())
    })
    .await
}

/// An invitation turned down by its member ("me"), or taken back by the group's owner and admins.
async fn uninvite(State(app): State<Shared>, h: HeaderMap, Key((conv, login)): Key<(i64, String)>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        let whom = db(&app, move |c| -> Result<i64, Response> {
            let whom = if login == "me" { uid } else { crate::people::uid_of(c, &login).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))? };
            if whom != uid {
                let (_, role) = place(c, conv, uid).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
                if !may_manage(&role) {
                    return Err(fail(StatusCode::FORBIDDEN, "admins"));
                }
            }
            c.execute("DELETE FROM conv_invites WHERE conv = ?1 AND uid = ?2", params![conv, whom]).map_err(|_| busy())?;
            Ok(whom)
        })
        .await?;
        crate::live::publish(whom, &json!({ "t": "chats" }));
        Ok(Json(json!({})).into_response())
    })
    .await
}

/// In: by the invitation the member has, or by the group's link.
fn join(c: &Connection, conv: i64, uid: i64, by_link: bool) -> Result<Value, Response> {
    if place(c, conv, uid).is_some() {
        return chat_json(c, conv, uid).ok_or_else(busy);
    }
    let invited: Option<i64> = c.query_row("SELECT by FROM conv_invites WHERE conv = ?1 AND uid = ?2", params![conv, uid], |r| r.get(0)).optional().map_err(|_| busy())?;
    if invited.is_none() && !by_link {
        return Err(fail(StatusCode::FORBIDDEN, "invitation"));
    }
    let groups: i64 = c
        .query_row("SELECT COUNT(*) FROM conv_members m JOIN convs v ON v.id = m.conv WHERE m.uid = ?1 AND v.kind = 'group'", params![uid], |r| r.get(0))
        .unwrap_or(0);
    if groups >= MOST_GROUPS {
        return Err(said(StatusCode::BAD_REQUEST, "many", "you are in 30 groups already"));
    }
    let n = count(c, conv);
    if n >= MOST_GROUP {
        return Err(said(StatusCode::BAD_REQUEST, "full", "a group holds 50"));
    }
    // A block between the joiner and the owner keeps them apart.
    let owner: Option<i64> = c.query_row("SELECT owner FROM convs WHERE id = ?1", params![conv], |r| r.get(0)).optional().map_err(|_| busy())?.flatten();
    if owner.is_some_and(|o| crate::friends::apart(c, o, uid)) {
        return Err(said(StatusCode::FORBIDDEN, "blocked", "one of you has blocked the other"));
    }
    // What came before the member joined stays read.
    let top: i64 = c.query_row("SELECT COALESCE(MAX(id), 0) FROM messages WHERE conv = ?1", params![conv], |r| r.get(0)).unwrap_or(0);
    c.execute("INSERT INTO conv_members (conv, uid, joined, read_upto) VALUES (?1, ?2, ?3, ?4)", params![conv, uid, now(), top]).map_err(|_| busy())?;
    c.execute("DELETE FROM conv_invites WHERE conv = ?1 AND uid = ?2", params![conv, uid]).map_err(|_| busy())?;
    happened(c, conv, "joined", uid, None, json!({ "link": by_link }));
    if n + 1 >= SOUL {
        if let Some(o) = owner {
            crate::people::award(c, o, "soul", 1, None);
        }
    }
    retell(c, conv);
    chat_json(c, conv, uid).ok_or_else(busy)
}

async fn accept(State(app): State<Shared>, h: HeaderMap, Key(conv): Key<i64>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        let v = db(&app, move |c| join(c, conv, uid, false)).await?;
        Ok(Json(json!({ "chat": v })).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct LinkOn {
    on: bool,
}

/// The group's link made anew (an old one stops working) or taken away.
async fn link(State(app): State<Shared>, h: HeaderMap, Key(conv): Key<i64>, Json(b): Json<LinkOn>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        let code = db(&app, move |c| -> Result<Option<String>, Response> {
            let (kind, role) = place(c, conv, uid).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            if kind != "group" || !may_manage(&role) {
                return Err(fail(StatusCode::FORBIDDEN, "admins"));
            }
            let code = b.on.then(|| random_hex(5));
            c.execute("UPDATE convs SET link = ?2 WHERE id = ?1", params![conv, code]).map_err(|_| busy())?;
            retell(c, conv);
            Ok(code)
        })
        .await?;
        Ok(Json(json!({ "link": code })).into_response())
    })
    .await
}

/// What a group's link leads to (its name, picture, how many are in it), before joining.
async fn by_link(State(app): State<Shared>, h: HeaderMap, Key(code): Key<String>) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        let uid = w.uid;
        let v = db(&app, move |c| -> Result<Value, Response> {
            let (conv, title, avatar): (i64, String, Option<String>) = c
                .query_row("SELECT id, title, avatar FROM convs WHERE link = ?1 AND kind = 'group'", params![code], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .optional()
                .map_err(|_| busy())?
                .ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            Ok(json!({ "chat": conv, "title": title, "avatar": avatar.and_then(|a| serde_json::from_str::<Value>(&a).ok()), "members": count(c, conv), "in": place(c, conv, uid).is_some() }))
        })
        .await?;
        Ok(Json(v).into_response())
    })
    .await
}

async fn join_by_link(State(app): State<Shared>, h: HeaderMap, Key(code): Key<String>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        let v = db(&app, move |c| -> Result<Value, Response> {
            let conv: i64 = c
                .query_row("SELECT id FROM convs WHERE link = ?1 AND kind = 'group'", params![code], |r| r.get(0))
                .optional()
                .map_err(|_| busy())?
                .ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            join(c, conv, uid, true)
        })
        .await?;
        Ok(Json(json!({ "chat": v })).into_response())
    })
    .await
}

/// Out of a group: the member leaves ("me"), or the owner and admins remove someone (an admin only plain members).
/// An owner who leaves hands the group to its oldest admin, else its oldest member; the last one out ends it.
async fn out(State(app): State<Shared>, h: HeaderMap, Key((conv, login)): Key<(i64, String)>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        let whom = db(&app, move |c| -> Result<i64, Response> {
            let (kind, role) = place(c, conv, uid).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            if kind != "group" {
                return Err(fail(StatusCode::BAD_REQUEST, "direct"));
            }
            let whom = if login == "me" { uid } else { crate::people::uid_of(c, &login).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))? };
            let (_, their) = place(c, conv, whom).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            if whom != uid {
                let allowed = role == "owner" || (role == "admin" && their == "member");
                if !allowed {
                    return Err(fail(StatusCode::FORBIDDEN, "admins"));
                }
            }
            c.execute("DELETE FROM conv_members WHERE conv = ?1 AND uid = ?2", params![conv, whom]).map_err(|_| busy())?;
            if count(c, conv) == 0 {
                c.execute("DELETE FROM convs WHERE id = ?1", params![conv]).map_err(|_| busy())?;
                return Ok(whom);
            }
            if their == "owner" {
                let heir: Option<i64> = c
                    .query_row(
                        "SELECT uid FROM conv_members WHERE conv = ?1 ORDER BY CASE role WHEN 'admin' THEN 0 ELSE 1 END, joined LIMIT 1",
                        params![conv],
                        |r| r.get(0),
                    )
                    .optional()
                    .map_err(|_| busy())?;
                if let Some(h) = heir {
                    c.execute("UPDATE conv_members SET role = 'owner' WHERE conv = ?1 AND uid = ?2", params![conv, h]).map_err(|_| busy())?;
                    c.execute("UPDATE convs SET owner = ?2 WHERE id = ?1", params![conv, h]).map_err(|_| busy())?;
                }
            }
            if whom == uid {
                happened(c, conv, "left", uid, None, json!({}));
            } else {
                happened(c, conv, "removed", uid, Some(whom), json!({}));
            }
            retell(c, conv);
            Ok(whom)
        })
        .await?;
        crate::live::publish(whom, &json!({ "t": "chats" }));
        Ok(Json(json!({})).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct Rank {
    admin: bool,
}

/// A member made an admin or a plain member again (by the owner).
async fn rank(State(app): State<Shared>, h: HeaderMap, Key((conv, login)): Key<(i64, String)>, Json(b): Json<Rank>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        db(&app, move |c| -> Result<(), Response> {
            let (_, role) = place(c, conv, uid).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            if role != "owner" {
                return Err(fail(StatusCode::FORBIDDEN, "owner"));
            }
            let whom = crate::people::uid_of(c, &login).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            let (_, their) = place(c, conv, whom).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            if their == "owner" {
                return Err(fail(StatusCode::BAD_REQUEST, "owner"));
            }
            c.execute("UPDATE conv_members SET role = ?3 WHERE conv = ?1 AND uid = ?2", params![conv, whom, if b.admin { "admin" } else { "member" }]).map_err(|_| busy())?;
            happened(c, conv, if b.admin { "admin" } else { "unadmin" }, uid, Some(whom), json!({}));
            retell(c, conv);
            Ok(())
        })
        .await?;
        Ok(Json(json!({})).into_response())
    })
    .await
}

// ------------------------------------------------------------------------------------------------ reports

#[derive(Deserialize)]
struct Why {
    why: String,
}

/// A message reported: staff get it and the ten before it, as they are now, and nothing else of the conversation.
async fn report(State(app): State<Shared>, h: HeaderMap, Key((conv, id)): Key<(i64, i64)>, Json(b): Json<Why>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        let why = crate::market::clean_line(&b.why, 300);
        if why.chars().count() < 3 {
            return Err(said(StatusCode::BAD_REQUEST, "why", "say what is wrong"));
        }
        db(&app, move |c| -> Result<(), Response> {
            place(c, conv, uid).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            let m = message(c, id).filter(|m| m.conv == conv && m.deleted.is_none() && m.kind != "system").ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            if m.uid == Some(uid) {
                return Err(said(StatusCode::BAD_REQUEST, "self", "that message is yours"));
            }
            let open: i64 = c.query_row("SELECT COUNT(*) FROM chat_reports WHERE reporter = ?1 AND state = 'open'", params![uid], |r| r.get(0)).unwrap_or(0);
            if open >= 20 {
                return Err(fail(StatusCode::TOO_MANY_REQUESTS, "slow"));
            }
            let mut before: Vec<Msg> = c
                .prepare(&format!("{MESSAGE} WHERE conv = ?1 AND id < ?2 ORDER BY id DESC LIMIT 10"))
                .and_then(|mut s| s.query_map(params![conv, id], msg_row).map(|r| r.flatten().collect()))
                .map_err(|_| busy())?;
            before.reverse();
            let snapshot: Vec<Value> = before.iter().chain(std::iter::once(&m)).map(|x| message_json(c, x)).collect();
            c.execute(
                "INSERT INTO chat_reports (message, conv, reporter, accused, why, snapshot, created) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![id, conv, uid, m.uid, why, json!(snapshot).to_string(), now()],
            )
            .map_err(|_| busy())?;
            Ok(())
        })
        .await?;
        Ok((StatusCode::CREATED, Json(json!({}))).into_response())
    })
    .await
}

/// The reports waiting for staff (who may handle messages' reports).
async fn reports(State(app): State<Shared>, h: HeaderMap) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        if !crate::people::can(w.uid, crate::people::MESSAGES_REPORTS) {
            return Err(fail(StatusCode::FORBIDDEN, "staff"));
        }
        let v = db(&app, move |c| -> rusqlite::Result<Value> {
            let rows: Vec<Value> = c
                .prepare("SELECT id, message, reporter, accused, why, snapshot, created, state, outcome FROM chat_reports ORDER BY state = 'open' DESC, created DESC LIMIT 100")?
                .query_map([], |r| {
                    Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, r.get::<_, Option<i64>>(3)?, r.get::<_, String>(4)?, r.get::<_, String>(5)?, r.get::<_, i64>(6)?, r.get::<_, String>(7)?, r.get::<_, Option<String>>(8)?))
                })?
                .flatten()
                .map(|(id, message, reporter, accused, why, snapshot, created, state, outcome)| {
                    json!({ "id": id, "message": message, "reporter": crate::people::card_of(c, reporter), "accused": accused.map(|a| crate::people::card_of(c, a)),
                            "why": why, "snapshot": serde_json::from_str::<Value>(&snapshot).unwrap_or(Value::Null), "created": created, "state": state, "outcome": outcome })
                })
                .collect();
            Ok(json!({ "reports": rows }))
        })
        .await
        .map_err(|_| busy())?;
        Ok(Json(v).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct Outcome {
    /// "hide": the message is removed; "dismiss": nothing is wrong.
    outcome: String,
}

async fn handle(State(app): State<Shared>, h: HeaderMap, Key(id): Key<i64>, Json(b): Json<Outcome>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        if !crate::people::can(uid, crate::people::MESSAGES_REPORTS) {
            return Err(fail(StatusCode::FORBIDDEN, "staff"));
        }
        if !["hide", "dismiss"].contains(&b.outcome.as_str()) {
            return Err(fail(StatusCode::BAD_REQUEST, "outcome"));
        }
        db(&app, move |c| -> Result<(), Response> {
            let (message_id, reporter): (i64, i64) = c
                .query_row("SELECT message, reporter FROM chat_reports WHERE id = ?1 AND state = 'open'", params![id], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()
                .map_err(|_| busy())?
                .ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            if b.outcome == "hide" {
                if let Some(m) = message(c, message_id).filter(|m| m.deleted.is_none()) {
                    take_away(c, &m, uid).map_err(|_| busy())?;
                }
            }
            // Every open report of the same message is answered at once.
            c.execute(
                "UPDATE chat_reports SET state = 'done', outcome = ?2, handled_by = ?3, handled_at = ?4 WHERE message = ?1 AND state = 'open'",
                params![message_id, b.outcome, uid, now()],
            )
            .map_err(|_| busy())?;
            crate::people::audit(c, uid, "chat.report", &message_id.to_string(), json!({ "outcome": b.outcome }));
            crate::notify::add(c, reporter, "report", None, &format!("chat-report:{id}"), json!({ "outcome": b.outcome, "title": "" }));
            Ok(())
        })
        .await?;
        Ok(Json(json!({})).into_response())
    })
    .await
}

pub fn routes() -> Router<Shared> {
    let json = || DefaultBodyLimit::max(16 * 1024);
    let file = || DefaultBodyLimit::max(crate::media::MOST_PICTURE + 1024);
    Router::new()
        .route("/api/chats", get(list))
        .route("/api/chats/dm", post(open_dm).layer(json()))
        .route("/api/chats/groups", post(new_group).layer(json()))
        .route("/api/chats/link/{code}", get(by_link).post(join_by_link))
        .route("/api/chats/{id}", get(page).patch(settings).layer(json()))
        .route("/api/chats/{id}/messages", post(write).layer(json()))
        .route("/api/chats/{id}/messages/{mid}", patch(edit).delete(remove).layer(json()))
        .route("/api/chats/{id}/messages/{mid}/react", post(react).layer(json()))
        .route("/api/chats/{id}/messages/{mid}/report", post(report).layer(json()))
        .route("/api/chats/{id}/pictures", post(picture).layer(file()))
        .route("/api/chats/{id}/voice", post(voice).layer(DefaultBodyLimit::max(crate::media::MOST_VOICE + 1024)))
        .route("/api/chats/{id}/avatar", post(group_avatar).layer(DefaultBodyLimit::max((8 << 20) + 1024)))
        .route("/api/chats/{id}/read", post(read).layer(json()))
        .route("/api/chats/{id}/typing", post(typing))
        .route("/api/chats/{id}/pin", post(pin).layer(json()))
        .route("/api/chats/{id}/invites", post(invite).layer(json()))
        .route("/api/chats/{id}/invites/{login}", delete(uninvite))
        .route("/api/chats/{id}/join", post(accept))
        .route("/api/chats/{id}/link", post(link).layer(json()))
        .route("/api/chats/{id}/members/{login}", delete(out))
        .route("/api/chats/{id}/members/{login}/role", post(rank).layer(json()))
        .route("/api/staff/chat-reports", get(reports))
        .route("/api/staff/chat-reports/{id}", post(handle).layer(json()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_is_cleaned_of_what_is_not_text() {
        assert_eq!(clean_text("  hi\u{0007}\r\nthere  ", 100), "hi\nthere");
        assert_eq!(clean_text("abc", 2), "ab");
    }

    #[test]
    fn a_burst_is_held_back() {
        let uid = 987_654;
        for _ in 0..BURST {
            assert!(paced(uid));
        }
        assert!(!paced(uid));
    }
}
