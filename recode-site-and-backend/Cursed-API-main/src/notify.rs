//! Notifications: whatever happens to a member while they look elsewhere — a reply in their thread, their name
//! mentioned, a post of theirs moderated, a badge earned, a role given. Each is kept (the list on the site and in the
//! game's tab) and sent at once down the live line; what a member wants of each kind is theirs to say: on the site,
//! in the game (a note in its corner), as a Windows notification while the game is not in front.

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::market::{busy, member, run, said};
use crate::{db, now, Shared};

/// Kept this long, read or not.
const KEEP_DAYS: i64 = 90;
const PAGE: i64 = 30;

/// The kinds, as the settings list them: (kind, English, Russian, site, game, Windows) — the defaults.
pub const KINDS: &[(&str, &str, &str, bool, bool, bool)] = &[
    ("reply", "Replies in threads you follow", "Ответы в темах, на которые вы подписаны", true, true, true),
    ("forum", "Your bugs, ideas and questions", "Ваши баги, идеи и вопросы", true, true, true),
    ("mention", "Mentions of you", "Упоминания вас", true, true, true),
    ("moderation", "Moderation of your posts", "Модерация ваших постов", true, true, true),
    ("report", "Answers to your reports", "Ответы на ваши жалобы", true, true, false),
    ("badge", "New badges", "Новые значки", true, true, true),
    ("role", "Roles given to you", "Выданные вам роли", true, true, true),
    ("friend", "Friend requests", "Заявки в друзья", true, true, true),
    ("group", "Invitations to groups", "Приглашения в группы", true, true, true),
    ("ticket", "Answers in your tickets", "Ответы в ваших тикетах", true, true, true),
    ("look", "Likes on your look", "Оценки вашего образа", true, false, false),
    // Messages are not kept here (the conversations count their own unread); these say where new ones are shown.
    ("message", "Messages", "Сообщения", true, true, true),
    ("system", "News from Cursed", "Новости Cursed", true, true, true),
];

pub fn schema(c: &Connection) {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS notifications (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             uid INTEGER NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
             kind TEXT NOT NULL,
             actor INTEGER,
             subject TEXT NOT NULL,                   -- what it is about: thread:12, post:55, badge:first100…
             data TEXT NOT NULL,                      -- what the line says (JSON)
             created INTEGER NOT NULL,
             read INTEGER NOT NULL DEFAULT 0);
         CREATE INDEX IF NOT EXISTS notifications_uid ON notifications(uid, id);
         CREATE TABLE IF NOT EXISTS notify_prefs (
             uid INTEGER NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
             kind TEXT NOT NULL,
             site INTEGER NOT NULL,
             game INTEGER NOT NULL,
             push INTEGER NOT NULL,
             PRIMARY KEY (uid, kind));",
    )
    .expect("make the notifications' tables");
}

pub fn unread(c: &Connection, uid: i64) -> i64 {
    c.query_row("SELECT COUNT(*) FROM notifications WHERE uid = ?1 AND read = 0", params![uid], |r| r.get(0)).unwrap_or(0)
}

/// What a member wants of a kind: (site, game, Windows).
pub fn wants(c: &Connection, uid: i64, kind: &str) -> (bool, bool, bool) {
    let default = KINDS.iter().find(|k| k.0 == kind).map_or((true, true, false), |k| (k.3, k.4, k.5));
    c.query_row("SELECT site, game, push FROM notify_prefs WHERE uid = ?1 AND kind = ?2", params![uid, kind], |r| Ok((r.get::<_, bool>(0)?, r.get::<_, bool>(1)?, r.get::<_, bool>(2)?)))
        .optional()
        .ok()
        .flatten()
        .unwrap_or(default)
}

fn row(c: &Connection, id: i64, kind: String, actor: Option<i64>, subject: String, data: String, created: i64, read: bool) -> Value {
    json!({
        "id": id, "kind": kind, "subject": subject, "created": created, "read": read,
        "actor": actor.map(|a| crate::people::card_of(c, a)),
        "data": serde_json::from_str::<Value>(&data).unwrap_or(Value::Null),
    })
}

/// A notification for `uid`: kept, and sent down their live line with what they want done with it. Nothing when the
/// actor is the member themselves, or someone they have blocked.
pub fn add(c: &Connection, uid: i64, kind: &str, actor: Option<i64>, subject: &str, data: Value) -> Option<i64> {
    if actor == Some(uid) || actor.is_some_and(|a| crate::people::blocked(c, uid, a)) {
        return None;
    }
    let at = now();
    c.execute(
        "INSERT INTO notifications (uid, kind, actor, subject, data, created) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![uid, kind, actor, subject, data.to_string(), at],
    )
    .ok()?;
    let id = c.last_insert_rowid();
    let (site, game, push) = wants(c, uid, kind);
    let n = row(c, id, kind.to_string(), actor, subject.to_string(), data.to_string(), at, false);
    crate::live::publish(uid, &json!({ "t": "note", "n": n, "unread": unread(c, uid), "site": site, "game": game, "push": push }));
    Some(id)
}

/// A notification of the same subject replaced rather than piled up (ten replies in a thread: one line, the newest).
pub fn add_or_refresh(c: &Connection, uid: i64, kind: &str, actor: Option<i64>, subject: &str, data: Value) -> Option<i64> {
    let _ = c.execute("DELETE FROM notifications WHERE uid = ?1 AND kind = ?2 AND subject = ?3 AND read = 0", params![uid, kind, subject]);
    add(c, uid, kind, actor, subject, data)
}

pub fn routes() -> Router<Shared> {
    Router::new()
        .route("/api/notifications", get(list))
        .route("/api/notifications/read", post(mark))
        .route("/api/me/notify", get(prefs).put(set_prefs))
}

#[derive(Deserialize)]
struct Before {
    before: Option<i64>,
}

async fn list(State(app): State<Shared>, h: HeaderMap, Query(q): Query<Before>) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        let uid = w.uid;
        let before = q.before.unwrap_or(i64::MAX);
        let v = db(&app, move |c| -> rusqlite::Result<Value> {
            let _ = c.execute("DELETE FROM notifications WHERE uid = ?1 AND created < ?2", params![uid, now() - KEEP_DAYS * 86_400]);
            let mut q = c.prepare("SELECT id, kind, actor, subject, data, created, read FROM notifications WHERE uid = ?1 AND id < ?2 ORDER BY id DESC LIMIT ?3")?;
            let rows: Vec<(i64, String, Option<i64>, String, String, i64, bool)> =
                q.query_map(params![uid, before, PAGE], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)))?.flatten().collect();
            let more = rows.len() as i64 == PAGE;
            let items: Vec<Value> = rows.into_iter().map(|(id, kind, actor, subject, data, created, read)| row(c, id, kind, actor, subject, data, created, read)).collect();
            Ok(json!({ "items": items, "unread": unread(c, uid), "more": more }))
        })
        .await
        .map_err(|_| busy())?;
        Ok(Json(v).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct Mark {
    ids: Option<Vec<i64>>,
    all: Option<bool>,
    subject: Option<String>,
}

/// Read: some by their ids, all of them, or all of one subject (the thread just opened).
async fn mark(State(app): State<Shared>, h: HeaderMap, Json(m): Json<Mark>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        let left = db(&app, move |c| -> rusqlite::Result<i64> {
            if m.all == Some(true) {
                c.execute("UPDATE notifications SET read = 1 WHERE uid = ?1 AND read = 0", params![uid])?;
            }
            if let Some(subject) = m.subject {
                c.execute("UPDATE notifications SET read = 1 WHERE uid = ?1 AND subject = ?2 AND read = 0", params![uid, subject])?;
            }
            for id in m.ids.unwrap_or_default().into_iter().take(200) {
                c.execute("UPDATE notifications SET read = 1 WHERE uid = ?1 AND id = ?2", params![uid, id])?;
            }
            let left = unread(c, uid);
            crate::live::publish(uid, &json!({ "t": "unread", "unread": left }));
            Ok(left)
        })
        .await
        .map_err(|_| busy())?;
        Ok(Json(json!({ "unread": left })).into_response())
    })
    .await
}

async fn prefs(State(app): State<Shared>, h: HeaderMap) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        let uid = w.uid;
        let v = db(&app, move |c| {
            let kinds: Vec<Value> = KINDS
                .iter()
                .map(|k| {
                    let (site, game, push) = wants(c, uid, k.0);
                    json!({ "kind": k.0, "en": k.1, "ru": k.2, "site": site, "game": game, "push": push })
                })
                .collect();
            json!({ "kinds": kinds })
        })
        .await;
        Ok(Json(v).into_response())
    })
    .await
}

/// `{"reply": {"site": true, "game": false, "push": false}, …}`: the kinds said, the others as they were.
async fn set_prefs(State(app): State<Shared>, h: HeaderMap, Json(b): Json<Map<String, Value>>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        for (kind, v) in &b {
            if !KINDS.iter().any(|k| k.0 == kind) || !v.is_object() {
                return Err(said(StatusCode::BAD_REQUEST, "kind", "no such kind of notification"));
            }
        }
        let uid = w.uid;
        db(&app, move |c| -> rusqlite::Result<()> {
            for (kind, v) in b {
                let (site, game, push) = wants(c, uid, &kind);
                c.execute(
                    "INSERT INTO notify_prefs (uid, kind, site, game, push) VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(uid, kind) DO UPDATE SET site = ?3, game = ?4, push = ?5",
                    params![uid, kind, v["site"].as_bool().unwrap_or(site), v["game"].as_bool().unwrap_or(game), v["push"].as_bool().unwrap_or(push)],
                )?;
            }
            Ok(())
        })
        .await
        .map_err(|_| busy())?;
        Ok(Json(json!({})).into_response())
    })
    .await
}

/// The words of a post for a notification's line: its start, on one line.
pub fn excerpt(body: &str) -> String {
    let one: String = body.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.chars().count() <= 140 {
        one
    } else {
        one.chars().take(139).collect::<String>() + "…"
    }
}

/// The logins a post mentions (@login), at most ten, each once, without the author.
pub fn mentions(body: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let b = body.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'@' && (i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_')) {
            let start = i + 1;
            let mut end = start;
            while end < b.len() && (b[end].is_ascii_alphanumeric() || b[end] == b'_') {
                end += 1;
            }
            let login = &body[start..end];
            if (3..=16).contains(&login.len()) && !out.iter().any(|l| l.eq_ignore_ascii_case(login)) {
                out.push(login.to_string());
                if out.len() == 10 {
                    break;
                }
            }
            i = end;
        } else {
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mentions_are_found_once_each_and_not_in_addresses() {
        assert_eq!(mentions("hi @Lunuca and @sequwi, @lunuca again; mail me a@b.com @x"), vec!["Lunuca", "sequwi"]);
        assert!(mentions("no one here").is_empty());
    }

    #[test]
    fn an_excerpt_is_one_short_line() {
        assert_eq!(excerpt("a\n\nb   c"), "a b c");
        assert_eq!(excerpt(&"x".repeat(300)).chars().count(), 140);
    }
}
