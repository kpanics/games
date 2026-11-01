//! Letters (hers, 11.10.2026): a password forgotten, an e-mail confirmed, the old address told when it is changed.
//!
//! Sent through Resend (api.resend.com) with the key from /etc/cursed/mail.env (RESEND_KEY; MAIL_FROM may name the
//! sender), read by the service's EnvironmentFile. Without the key nothing is sent and the doors say so.
//! A letter's link carries a one-time token in the address's fragment (#/reset/…, #/confirm/…): the fragment never
//! reaches a server, so no log keeps it; here only its hash is kept. A reset lives half an hour (a day when the staff
//! made it), a confirmation three days. An account gets at most three letters of a kind an hour, and all of them
//! together at most `PER_DAY` a day (the service's free share), so no one can spend the letters by asking for others.

use std::env;
use std::time::Duration;

use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::json;

use crate::{blocking, cookie, db, digest, fail, hash_password, new_session, now, password_ok, presented, random_hex, same_site, who, Shared, WEB_DAYS};

const RESET_LIFE: i64 = 30 * 60;
const STAFF_RESET_LIFE: i64 = 24 * 3600;
const CONFIRM_LIFE: i64 = 3 * 86_400;
const PER_HOUR: i64 = 3;
const PER_DAY: i64 = 90;

pub fn schema(c: &Connection) {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS mail_tokens (
             hash BLOB PRIMARY KEY,
             uid INTEGER NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
             kind TEXT NOT NULL,
             email TEXT NOT NULL DEFAULT '',
             made INTEGER NOT NULL,
             until INTEGER NOT NULL,
             used INTEGER);
         CREATE INDEX IF NOT EXISTS mail_tokens_uid ON mail_tokens(uid, kind, made);
         CREATE INDEX IF NOT EXISTS mail_tokens_made ON mail_tokens(made);",
    )
    .expect("make the letters' table");
    let has: bool = c.prepare("SELECT 1 FROM pragma_table_info('users') WHERE name = 'email_confirmed'").and_then(|mut q| q.exists([])).unwrap_or(false);
    if !has {
        c.execute("ALTER TABLE users ADD COLUMN email_confirmed INTEGER", []).expect("the users' new column");
    }
}

/// Whether letters can be sent at all.
pub fn configured() -> bool {
    env::var("RESEND_KEY").is_ok_and(|k| !k.trim().is_empty())
}

/// Whether the account's e-mail was confirmed (by a confirmation, or by a reset that came to it).
pub fn confirmed(c: &Connection, uid: i64) -> bool {
    c.query_row("SELECT email_confirmed IS NOT NULL FROM users WHERE uid = ?1", params![uid], |r| r.get(0)).unwrap_or(false)
}

fn well_formed(t: &str) -> bool {
    t.len() == 64 && t.bytes().all(|b| b.is_ascii_hexdigit())
}

/// A new token of this kind for the account; the old ones long dead are swept out on the way.
fn make(c: &Connection, uid: i64, kind: &str, email: &str, life: i64) -> rusqlite::Result<String> {
    let t = random_hex(32);
    let at = now();
    c.execute(
        "INSERT INTO mail_tokens (hash, uid, kind, email, made, until) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![digest(&t), uid, kind, email, at, at + life],
    )?;
    let _ = c.execute("DELETE FROM mail_tokens WHERE until < ?1", params![at - 7 * 86_400]);
    Ok(t)
}

/// May one more letter of this kind go to this account now (and one more letter at all today)?
fn may_send(c: &Connection, uid: i64, kind: &str) -> bool {
    let at = now();
    let hour: i64 = c
        .query_row("SELECT COUNT(*) FROM mail_tokens WHERE uid = ?1 AND kind = ?2 AND made > ?3", params![uid, kind, at - 3600], |r| r.get(0))
        .unwrap_or(PER_HOUR);
    let day: i64 = c.query_row("SELECT COUNT(*) FROM mail_tokens WHERE kind != 'staff-reset' AND made > ?1", params![at - 86_400], |r| r.get(0)).unwrap_or(PER_DAY);
    hour < PER_HOUR && day < PER_DAY
}

/// A staff member's reset link for an account (admin.rs): a day long, no letter.
pub fn staff_token(c: &Connection, uid: i64) -> rusqlite::Result<String> {
    make(c, uid, "staff-reset", "", STAFF_RESET_LIFE)
}

// ------------------------------------------------------------------------------------------------ the letters

pub struct Letter {
    to: String,
    subject: String,
    html: String,
    text: String,
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// The address shown without its middle: da••••@gmail.com.
pub fn mask(e: &str) -> String {
    match e.split_once('@') {
        Some((l, d)) => format!("{}••••@{d}", l.chars().take(2).collect::<String>()),
        None => "••••".into(),
    }
}

/// One letter in the site's look: black, a white button, the words plain. `lines` are already escaped HTML.
fn letter(to: &str, subject: &str, title: &str, lines: &[String], button: Option<(&str, &str)>, after: &str, text: String) -> Letter {
    let p = |s: &str| format!("<tr><td style=\"color:#c4c4c8;font-size:15px;line-height:1.6;padding:0 0 12px\">{s}</td></tr>");
    let btn = button.map_or(String::new(), |(href, label)| {
        format!(
            "<tr><td style=\"padding:10px 0 22px\"><a href=\"{}\" style=\"display:inline-block;background:#ffffff;color:#0b0b0c;text-decoration:none;font-weight:600;font-size:15px;padding:13px 24px;border-radius:10px\">{}</a></td></tr>",
            esc(href),
            esc(label)
        )
    });
    let html = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><meta name=\"color-scheme\" content=\"dark\"></head><body style=\"margin:0;padding:0;background:#0b0b0c\">\
<table role=\"presentation\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\" style=\"background:#0b0b0c;padding:40px 16px;font-family:-apple-system,'Segoe UI',Roboto,Helvetica,Arial,sans-serif\"><tr><td align=\"center\">\
<table role=\"presentation\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\" style=\"max-width:480px;background:#141416;border-radius:16px\"><tr><td style=\"padding:32px 32px 26px\">\
<table role=\"presentation\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\">\
<tr><td style=\"color:#ffffff;font-size:12px;letter-spacing:5px;font-weight:700;padding:0 0 26px\">CURSED</td></tr>\
<tr><td style=\"color:#ffffff;font-size:22px;line-height:1.3;font-weight:600;padding:0 0 14px\">{}</td></tr>{}{}\
<tr><td style=\"color:#6a6a70;font-size:12.5px;line-height:1.55\">{}</td></tr></table></td></tr></table>\
<p style=\"color:#4a4a50;font-size:11.5px;margin:18px 0 0\">cursedclient.xyz</p></td></tr></table></body></html>",
        esc(title),
        lines.iter().map(|l| p(l)).collect::<String>(),
        btn,
        after
    );
    Letter { to: to.to_string(), subject: subject.to_string(), html, text }
}

fn en(lang: &str) -> bool {
    lang == "en"
}

fn reset_letter(lang: &str, login: &str, email: &str, link: &str) -> Letter {
    if en(lang) {
        letter(
            email,
            "Reset your Cursed password",
            "A new password",
            &[format!("Hi, <b style=\"color:#fff\">{}</b>. Someone (we hope you) asked to reset the password of your Cursed account.", esc(login)), "The link works once, for half an hour.".into()],
            Some((link, "Set a new password")),
            "If it wasn't you, do nothing: the password stays as it is.",
            format!("Hi, {login}.\n\nSomeone (we hope you) asked to reset the password of your Cursed account. Set a new one here (once, for half an hour):\n{link}\n\nIf it wasn't you, do nothing: the password stays as it is.\n\ncursedclient.xyz"),
        )
    } else {
        letter(
            email,
            "Сброс пароля Cursed",
            "Новый пароль",
            &[format!("Привет, <b style=\"color:#fff\">{}</b>. Кто-то (надеемся, вы) попросил сбросить пароль аккаунта Cursed.", esc(login)), "Ссылка сработает один раз и действует полчаса.".into()],
            Some((link, "Задать новый пароль")),
            "Если это были не вы — ничего не делайте, пароль останется прежним.",
            format!("Привет, {login}.\n\nКто-то (надеемся, вы) попросил сбросить пароль аккаунта Cursed. Задать новый можно здесь (один раз, в течение получаса):\n{link}\n\nЕсли это были не вы — ничего не делайте, пароль останется прежним.\n\ncursedclient.xyz"),
        )
    }
}

fn confirm_letter(lang: &str, login: &str, email: &str, link: &str) -> Letter {
    if en(lang) {
        letter(
            email,
            "Confirm your e-mail for Cursed",
            "Is this your e-mail?",
            &[format!("Hi, <b style=\"color:#fff\">{}</b>. Confirm that this address is yours: if you ever forget the password, it comes back through it.", esc(login))],
            Some((link, "Confirm the e-mail")),
            "The link works for three days. If you have no Cursed account, ignore this letter.",
            format!("Hi, {login}.\n\nConfirm that this address is yours (if you ever forget the password, it comes back through it):\n{link}\n\nThe link works for three days. If you have no Cursed account, ignore this letter.\n\ncursedclient.xyz"),
        )
    } else {
        letter(
            email,
            "Подтвердите почту для Cursed",
            "Это ваша почта?",
            &[format!("Привет, <b style=\"color:#fff\">{}</b>. Подтвердите, что это ваш адрес: если забудете пароль, он вернётся через него.", esc(login))],
            Some((link, "Подтвердить почту")),
            "Ссылка действует три дня. Если у вас нет аккаунта Cursed, просто не обращайте внимания на это письмо.",
            format!("Привет, {login}.\n\nПодтвердите, что это ваш адрес (если забудете пароль, он вернётся через него):\n{link}\n\nСсылка действует три дня. Если у вас нет аккаунта Cursed, просто не обращайте внимания на это письмо.\n\ncursedclient.xyz"),
        )
    }
}

fn changed_letter(lang: &str, login: &str, old: &str, new: &str) -> Letter {
    let new = mask(new);
    if en(lang) {
        letter(
            old,
            "Your Cursed e-mail was changed",
            "The e-mail was changed",
            &[format!("Hi, <b style=\"color:#fff\">{}</b>. The e-mail of your Cursed account was just changed to <b style=\"color:#fff\">{}</b>.", esc(login), esc(&new))],
            None,
            "If it was you, all is well. If not, write to Cursed's support on Discord at once.",
            format!("Hi, {login}.\n\nThe e-mail of your Cursed account was just changed to {new}.\n\nIf it was you, all is well. If not, write to Cursed's support on Discord at once.\n\ncursedclient.xyz"),
        )
    } else {
        letter(
            old,
            "Почта аккаунта Cursed изменена",
            "Почта изменена",
            &[format!("Привет, <b style=\"color:#fff\">{}</b>. Почту вашего аккаунта Cursed только что сменили на <b style=\"color:#fff\">{}</b>.", esc(login), esc(&new))],
            None,
            "Если это вы — всё в порядке. Если нет — сразу напишите в поддержку Cursed в Discord.",
            format!("Привет, {login}.\n\nПочту вашего аккаунта Cursed только что сменили на {new}.\n\nЕсли это вы — всё в порядке. Если нет — сразу напишите в поддержку Cursed в Discord.\n\ncursedclient.xyz"),
        )
    }
}

fn send(l: &Letter) -> Result<(), String> {
    let key = env::var("RESEND_KEY").map_err(|_| "no key".to_string())?;
    let from = env::var("MAIL_FROM").unwrap_or_else(|_| "Cursed <no-reply@cursedclient.xyz>".into());
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(15)).build();
    match agent
        .post("https://api.resend.com/emails")
        .set("Authorization", &format!("Bearer {}", key.trim()))
        .send_json(json!({ "from": from, "to": [l.to], "subject": l.subject, "html": l.html, "text": l.text }))
    {
        Ok(_) => Ok(()),
        Err(ureq::Error::Status(code, r)) => Err(format!("{code} {}", r.into_string().unwrap_or_default().chars().take(300).collect::<String>())),
        Err(e) => Err(e.to_string()),
    }
}

/// Sent aside: the answer does not wait for the letter (nor tells, by how long it took, whether one went).
fn send_later(l: Letter) {
    tokio::task::spawn_blocking(move || {
        if let Err(e) = send(&l) {
            eprintln!("mail: to {} failed: {e}", mask(&l.to));
        }
    });
}

// ------------------------------------------------------------------------------------------------ the doors

#[derive(Deserialize)]
pub struct Ask {
    login: String,
    #[serde(default)]
    lang: String,
}

/// «Forgot the password»: a login or an e-mail. The answer is the same whether such an account is or not.
pub async fn reset_ask(State(app): State<Shared>, h: HeaderMap, Json(a): Json<Ask>) -> Response {
    if !same_site(&app, &h) {
        return fail(StatusCode::FORBIDDEN, "origin");
    }
    if !configured() {
        return fail(StatusCode::SERVICE_UNAVAILABLE, "mail");
    }
    let id = a.login.trim().to_lowercase();
    if id.is_empty() || id.len() > 254 {
        return fail(StatusCode::BAD_REQUEST, "login");
    }
    let origin = app.origin.clone();
    let lang = a.lang;
    let letter = db(&app, move |c| -> Option<Letter> {
        let (uid, login, email): (i64, String, String) = c
            .query_row("SELECT uid, login, email FROM users WHERE login_key = ?1 OR email_key = ?1", params![id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .optional()
            .ok()??;
        if !may_send(c, uid, "reset") {
            return None;
        }
        let t = make(c, uid, "reset", "", RESET_LIFE).ok()?;
        Some(reset_letter(&lang, &login, &email, &format!("{origin}/#/reset/{t}")))
    })
    .await;
    if let Some(l) = letter {
        send_later(l);
    }
    Json(json!({})).into_response()
}

/// Whether a reset link is still good (the page asks before showing the form): the account's login, nothing more.
#[derive(Deserialize)]
pub struct Token {
    token: String,
}

pub async fn reset_check(State(app): State<Shared>, h: HeaderMap, Json(t): Json<Token>) -> Response {
    if !same_site(&app, &h) {
        return fail(StatusCode::FORBIDDEN, "origin");
    }
    if !well_formed(&t.token) {
        return fail(StatusCode::GONE, "token");
    }
    let hash = digest(&t.token);
    let login = db(&app, move |c| {
        c.query_row(
            "SELECT u.login FROM mail_tokens t JOIN users u ON u.uid = t.uid WHERE t.hash = ?1 AND t.kind IN ('reset', 'staff-reset') AND t.used IS NULL AND t.until > ?2",
            params![hash, now()],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .ok()
        .flatten()
    })
    .await;
    match login {
        Some(login) => Json(json!({ "login": login })).into_response(),
        None => fail(StatusCode::GONE, "token"),
    }
}

#[derive(Deserialize)]
pub struct SetPass {
    token: String,
    password: String,
}

/// The new password by the link: every session of the account ends, this browser is signed in anew.
pub async fn reset_set(State(app): State<Shared>, h: HeaderMap, Json(s): Json<SetPass>) -> Response {
    if !same_site(&app, &h) {
        return fail(StatusCode::FORBIDDEN, "origin");
    }
    if !well_formed(&s.token) {
        return fail(StatusCode::GONE, "token");
    }
    if !password_ok(&s.password) {
        return fail(StatusCode::BAD_REQUEST, "password");
    }
    let hash = digest(&s.token);
    let h2 = hash.clone();
    let found = db(&app, move |c| {
        c.query_row(
            "SELECT t.uid, t.kind, u.login, u.email FROM mail_tokens t JOIN users u ON u.uid = t.uid
             WHERE t.hash = ?1 AND t.kind IN ('reset', 'staff-reset') AND t.used IS NULL AND t.until > ?2",
            params![h2, now()],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?)),
        )
        .optional()
        .ok()
        .flatten()
    })
    .await;
    let Some((uid, kind, login, email)) = found else {
        return fail(StatusCode::GONE, "token");
    };
    let password = s.password;
    let pass = blocking(move || hash_password(&password)).await;
    let by_mail = kind == "reset";
    let done = db(&app, move |c| -> rusqlite::Result<bool> {
        let tx = c.transaction()?;
        let at = now();
        // The link is spent in the same breath as the password is set: two at once, only the first.
        if tx.execute("UPDATE mail_tokens SET used = ?2 WHERE hash = ?1 AND used IS NULL", params![hash, at])? == 0 {
            return Ok(false);
        }
        tx.execute("UPDATE mail_tokens SET used = ?2 WHERE uid = ?1 AND kind IN ('reset', 'staff-reset') AND used IS NULL", params![uid, at])?;
        tx.execute("UPDATE users SET pass = ?2 WHERE uid = ?1", params![uid, pass])?;
        if by_mail {
            // The letter came to the address: it is theirs.
            tx.execute("UPDATE users SET email_confirmed = ?2 WHERE uid = ?1 AND email_confirmed IS NULL", params![uid, at])?;
        }
        tx.execute("DELETE FROM sessions WHERE uid = ?1", params![uid])?;
        crate::people::audit(&tx, uid, "pass.reset", &format!("user:{uid}"), json!({ "by": if by_mail { "mail" } else { "staff" } }));
        tx.commit()?;
        Ok(true)
    })
    .await;
    match done {
        Ok(true) => {}
        Ok(false) => return fail(StatusCode::GONE, "token"),
        Err(_) => return fail(StatusCode::SERVICE_UNAVAILABLE, "busy"),
    }
    {
        let mut tries = app.tries.lock().unwrap_or_else(|e| e.into_inner());
        tries.remove(&login.to_lowercase());
        tries.remove(&email.to_lowercase());
    }
    // A banned account gets its new password, but signs in nowhere: told why, as at the door.
    if let Some(b) = db(&app, move |c| crate::beta::ban_of(c, uid)).await {
        return (StatusCode::FORBIDDEN, Json(b.json())).into_response();
    }
    let label = crate::people::agent_label(&h);
    let token = db(&app, move |c| {
        let t = new_session(c, uid, "web")?;
        c.execute("UPDATE sessions SET label = ?2 WHERE hash = ?1", params![digest(&t), label])?;
        Ok::<_, rusqlite::Error>(t)
    })
    .await;
    match token {
        Ok(t) => {
            let mut res = Json(json!({ "uid": uid, "login": login })).into_response();
            res.headers_mut().insert(header::SET_COOKIE, cookie(&app, &t, WEB_DAYS * 86_400));
            res
        }
        Err(_) => fail(StatusCode::SERVICE_UNAVAILABLE, "busy"),
    }
}

#[derive(Deserialize, Default)]
pub struct Lang {
    #[serde(default)]
    lang: String,
}

/// A confirmation letter to the account's own e-mail (asked from the cabinet).
pub async fn confirm_send(State(app): State<Shared>, h: HeaderMap, Json(l): Json<Lang>) -> Response {
    if !same_site(&app, &h) {
        return fail(StatusCode::FORBIDDEN, "origin");
    }
    let Some((token, _)) = presented(&h) else {
        return fail(StatusCode::UNAUTHORIZED, "signed-out");
    };
    let Some(w) = db(&app, move |c| who(c, &token)).await else {
        return fail(StatusCode::UNAUTHORIZED, "signed-out");
    };
    if !configured() {
        return fail(StatusCode::SERVICE_UNAVAILABLE, "mail");
    }
    let origin = app.origin.clone();
    let r = db(&app, move |c| -> Result<Option<Letter>, &'static str> {
        if confirmed(c, w.uid) {
            return Ok(None);
        }
        if !may_send(c, w.uid, "confirm") {
            return Err("slow");
        }
        let t = make(c, w.uid, "confirm", &w.email.to_lowercase(), CONFIRM_LIFE).map_err(|_| "busy")?;
        Ok(Some(confirm_letter(&l.lang, &w.login, &w.email, &format!("{origin}/#/confirm/{t}"))))
    })
    .await;
    match r {
        Ok(Some(letter)) => {
            send_later(letter);
            Json(json!({ "sent": true })).into_response()
        }
        Ok(None) => Json(json!({ "confirmed": true })).into_response(),
        Err("slow") => fail(StatusCode::TOO_MANY_REQUESTS, "slow"),
        Err(_) => fail(StatusCode::SERVICE_UNAVAILABLE, "busy"),
    }
}

/// The confirmation link opened: the address is the account's, if it still is the account's address.
pub async fn confirm(State(app): State<Shared>, h: HeaderMap, Json(t): Json<Token>) -> Response {
    if !same_site(&app, &h) {
        return fail(StatusCode::FORBIDDEN, "origin");
    }
    if !well_formed(&t.token) {
        return fail(StatusCode::GONE, "token");
    }
    let hash = digest(&t.token);
    let r = db(&app, move |c| -> Result<String, &'static str> {
        let (uid, email): (i64, String) = c
            .query_row(
                "SELECT uid, email FROM mail_tokens WHERE hash = ?1 AND kind = 'confirm' AND used IS NULL AND until > ?2",
                params![hash, now()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(|_| "busy")?
            .ok_or("token")?;
        let (key, shown): (String, String) = c.query_row("SELECT email_key, email FROM users WHERE uid = ?1", params![uid], |r| Ok((r.get(0)?, r.get(1)?))).map_err(|_| "token")?;
        c.execute("UPDATE mail_tokens SET used = ?2 WHERE hash = ?1", params![hash, now()]).map_err(|_| "busy")?;
        if key != email {
            return Err("stale");
        }
        c.execute("UPDATE users SET email_confirmed = ?2 WHERE uid = ?1", params![uid, now()]).map_err(|_| "busy")?;
        Ok(mask(&shown))
    })
    .await;
    match r {
        Ok(email) => Json(json!({ "email": email })).into_response(),
        Err("token") => fail(StatusCode::GONE, "token"),
        Err("stale") => fail(StatusCode::CONFLICT, "stale"),
        Err(_) => fail(StatusCode::SERVICE_UNAVAILABLE, "busy"),
    }
}

/// Right after a registration: the confirmation letter.
pub async fn welcome(app: &Shared, uid: i64, login: String, email: String, lang: String) {
    if !configured() {
        return;
    }
    let origin = app.origin.clone();
    let key = email.to_lowercase();
    let t = db(app, move |c| if may_send(c, uid, "confirm") { make(c, uid, "confirm", &key, CONFIRM_LIFE).ok() } else { None }).await;
    if let Some(t) = t {
        send_later(confirm_letter(&lang, &login, &email, &format!("{origin}/#/confirm/{t}")));
    }
}

/// After the e-mail is changed: the old address is told, the new one asked to confirm.
pub async fn changed(app: &Shared, uid: i64, login: String, old: String, new: String, lang: String) {
    if !configured() {
        return;
    }
    send_later(changed_letter(&lang, &login, &old, &new));
    welcome(app, uid, login, new, lang).await;
}
