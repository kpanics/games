//! Support: tickets between a member and Cursed's staff (those with `tickets.handle`).
//!
//! A ticket has a category (account and the device, a bug, a complaint, partnership, the media programme, something
//! else), a state (open → in work → waiting for its author → closed), someone of the staff it is assigned to, and
//! messages: the author's and the staff's, and the staff's own notes the author never sees. Pictures go in as in the
//! forum (`[img:<id>]`); files (the game's log) are attached and kept here, readable by the author and the staff only.
//! A bug reported from the game carries what the game knows of itself (build, video card, system, memory) and its
//! log. Closed, the author may rate the help. The staff keep ready answers.
//!
//! A member frees the computer their account is bound to themselves (as the loader's button, once a day); the staff
//! do it from a ticket at any time.

use std::io::{Read, Write};
use std::path::PathBuf;

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path as Key, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::market::{busy, clean_line, member, run, said};
use crate::{db, fail, now, Shared};

pub const CATEGORIES: &[&str] = &["account", "bug", "complaint", "partner", "media", "other"];
pub const STATES: &[&str] = &["open", "working", "waiting", "closed"];
/// Tickets a member may have not closed at once.
const MOST_OPEN: i64 = 3;
/// A file attached (a log): this much at most, kept compressed.
pub const MOST_FILE: usize = 4 << 20;
const FILES_PER_TICKET: i64 = 6;

pub fn schema(c: &Connection) {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS support_tickets (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             uid INTEGER NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
             category TEXT NOT NULL,
             title TEXT NOT NULL,
             state TEXT NOT NULL DEFAULT 'open',
             assignee INTEGER,
             meta TEXT,
             created INTEGER NOT NULL,
             updated INTEGER NOT NULL,
             closed INTEGER,
             rating INTEGER,
             rating_note TEXT,
             author_read INTEGER NOT NULL DEFAULT 0,
             staff_read INTEGER NOT NULL DEFAULT 0);
         CREATE INDEX IF NOT EXISTS support_tickets_uid ON support_tickets(uid, updated);
         CREATE INDEX IF NOT EXISTS support_tickets_state ON support_tickets(state, updated);
         CREATE TABLE IF NOT EXISTS ticket_messages (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             ticket INTEGER NOT NULL REFERENCES support_tickets(id) ON DELETE CASCADE,
             uid INTEGER,
             body TEXT NOT NULL,
             internal INTEGER NOT NULL DEFAULT 0,
             kind TEXT NOT NULL DEFAULT 'text',
             created INTEGER NOT NULL);
         CREATE INDEX IF NOT EXISTS ticket_messages_ticket ON ticket_messages(ticket, id);
         CREATE TABLE IF NOT EXISTS ticket_files (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             ticket INTEGER NOT NULL REFERENCES support_tickets(id) ON DELETE CASCADE,
             uid INTEGER NOT NULL,
             name TEXT NOT NULL,
             size INTEGER NOT NULL,
             file TEXT NOT NULL,
             created INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS ticket_replies (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             title TEXT NOT NULL,
             body TEXT NOT NULL,
             by INTEGER NOT NULL,
             created INTEGER NOT NULL);",
    )
    .expect("make the tickets' tables");
}

fn staff(uid: i64) -> bool {
    crate::people::can(uid, crate::people::TICKETS)
}

/// Everyone who handles tickets (told of new ones and of their authors' answers).
fn handlers(c: &Connection) -> Vec<i64> {
    let mut out: Vec<i64> = c.prepare("SELECT DISTINCT uid FROM roles").and_then(|mut q| q.query_map([], |r| r.get(0)).map(|m| m.flatten().collect())).unwrap_or_default();
    out.extend(crate::OWNERS.iter().copied());
    out.sort_unstable();
    out.dedup();
    out.retain(|u| staff(*u));
    out
}

fn files_dir(app: &Shared) -> PathBuf {
    std::path::Path::new(&app.db_path).parent().map_or_else(|| PathBuf::from("ticket-files"), |d| d.join("ticket-files"))
}

fn text_ok(s: &str, most: usize) -> Option<String> {
    let t: String = s.replace("\r\n", "\n").chars().filter(|c| *c == '\n' || *c == '\t' || !c.is_control()).collect();
    let t = t.trim().to_string();
    (1..=most).contains(&t.chars().count()).then_some(t)
}

/// A ticket as the one asking may see it: (its author, its state), or None when not theirs to see.
fn ticket_of(c: &Connection, id: i64, viewer: i64) -> Option<(i64, String)> {
    let (author, state): (i64, String) = c.query_row("SELECT uid, state FROM support_tickets WHERE id = ?1", params![id], |r| Ok((r.get(0)?, r.get(1)?))).optional().ok()??;
    (author == viewer || staff(viewer)).then_some((author, state))
}

fn ticket_json(c: &Connection, r: &rusqlite::Row, viewer: i64) -> rusqlite::Result<Value> {
    let id: i64 = r.get(0)?;
    let author: i64 = r.get(1)?;
    let assignee: Option<i64> = r.get(5)?;
    let last: i64 = c.query_row(
        "SELECT COALESCE(MAX(id), 0) FROM ticket_messages WHERE ticket = ?1 AND (internal = 0 OR ?2)",
        params![id, staff(viewer) && viewer != author],
        |r| r.get(0),
    )?;
    let read: i64 = if viewer == author { r.get(12)? } else { r.get(13)? };
    let meta: Option<String> = r.get(6)?;
    Ok(json!({
        "id": id,
        "author": crate::people::card_of(c, author),
        "category": r.get::<_, String>(2)?,
        "title": r.get::<_, String>(3)?,
        "state": r.get::<_, String>(4)?,
        "assignee": assignee.map(|u| crate::people::card_of(c, u)),
        "meta": meta.and_then(|m| serde_json::from_str::<Value>(&m).ok()),
        "created": r.get::<_, i64>(7)?,
        "updated": r.get::<_, i64>(8)?,
        "closed": r.get::<_, Option<i64>>(9)?,
        "rating": r.get::<_, Option<i64>>(10)?,
        "rating_note": r.get::<_, Option<String>>(11)?,
        "unread": last > read,
        "mine": author == viewer,
    }))
}

const TICKET: &str = "SELECT id, uid, category, title, state, assignee, meta, created, updated, closed, rating, rating_note, author_read, staff_read FROM support_tickets";

/// Something happened in a ticket: its author (unless it was them) and the staff (unless one of them did it, and
/// only for what is not their own note) hear it; both see it live.
fn told(c: &Connection, id: i64, by: i64, what: &str, excerpt: &str, internal: bool) {
    let Some((author, title)) = c.query_row("SELECT uid, title FROM support_tickets WHERE id = ?1", params![id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))).optional().ok().flatten() else { return };
    let data = json!({ "ticket": id, "title": title, "what": what, "excerpt": excerpt });
    if by != author && !internal {
        crate::notify::add_or_refresh(c, author, "ticket", Some(by), &format!("ticket:{id}"), data.clone());
    }
    crate::live::publish(author, &json!({ "t": "ticket", "id": id }));
    for s in handlers(c) {
        if s != by && s != author {
            if by == author && !internal {
                crate::notify::add_or_refresh(c, s, "ticket", Some(by), &format!("ticket:{id}"), data.clone());
            }
            crate::live::publish(s, &json!({ "t": "ticket", "id": id }));
        }
    }
}

fn system(c: &Connection, id: i64, by: i64, body: &str) {
    let _ = c.execute("INSERT INTO ticket_messages (ticket, uid, body, kind, created) VALUES (?1, ?2, ?3, 'system', ?4)", params![id, by, body, now()]);
}

// ------------------------------------------------------------------------------------------------ the doors

#[derive(Deserialize)]
struct ListQ {
    /// Staff: every ticket, not only one's own ("1").
    all: Option<String>,
    state: Option<String>,
    /// Staff: "me" (assigned to me), "none" (to nobody).
    assignee: Option<String>,
    category: Option<String>,
}

async fn list(State(app): State<Shared>, h: HeaderMap, Query(q): Query<ListQ>) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        let uid = w.uid;
        let all = q.all.as_deref().is_some_and(|v| v == "1" || v == "true") && staff(uid);
        let v = db(&app, move |c| -> rusqlite::Result<Value> {
            let mut filter = if all { "1".to_string() } else { format!("uid = {uid}") };
            let mut args: Vec<String> = Vec::new();
            if let Some(s) = q.state.filter(|s| STATES.contains(&s.as_str()) || s == "active") {
                if s == "active" {
                    filter += " AND state != 'closed'";
                } else {
                    args.push(s);
                    filter += &format!(" AND state = ?{}", args.len());
                }
            }
            if let Some(k) = q.category.filter(|k| CATEGORIES.contains(&k.as_str())) {
                args.push(k);
                filter += &format!(" AND category = ?{}", args.len());
            }
            match q.assignee.as_deref() {
                Some("me") if all => filter += &format!(" AND assignee = {uid}"),
                Some("none") if all => filter += " AND assignee IS NULL",
                _ => {}
            }
            let order = if all { "CASE state WHEN 'open' THEN 0 WHEN 'working' THEN 1 WHEN 'waiting' THEN 2 ELSE 3 END, updated DESC" } else { "state = 'closed', updated DESC" };
            let mut st = c.prepare(&format!("{TICKET} WHERE {filter} ORDER BY {order} LIMIT 100"))?;
            let refs: Vec<&dyn rusqlite::ToSql> = args.iter().map(|a| a as &dyn rusqlite::ToSql).collect();
            let rows: Vec<Value> = st.query_map(refs.as_slice(), |r| ticket_json(c, r, uid))?.flatten().collect();
            let counts: Value = if staff(uid) {
                let n = |sql: &str| c.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap_or(0);
                json!({ "open": n("SELECT COUNT(*) FROM support_tickets WHERE state = 'open'"), "working": n("SELECT COUNT(*) FROM support_tickets WHERE state = 'working'"),
                    "waiting": n("SELECT COUNT(*) FROM support_tickets WHERE state = 'waiting'"), "unassigned": n("SELECT COUNT(*) FROM support_tickets WHERE assignee IS NULL AND state != 'closed'") })
            } else {
                Value::Null
            };
            Ok(json!({ "items": rows, "staff": staff(uid), "counts": counts }))
        })
        .await
        .map_err(|_| busy())?;
        Ok(Json(v).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct NewTicket {
    category: String,
    title: String,
    body: String,
    /// A bug reported from the game: what the game knows of itself.
    report: Option<Value>,
}

/// What the game says of itself, kept short and plain: a few known fields.
fn report_of(v: &Value) -> Option<Value> {
    let o = v.as_object()?;
    let mut out = serde_json::Map::new();
    for k in ["build", "version", "gpu", "driver", "os", "cpu", "ram", "java", "screen", "server", "fps", "modules"] {
        if let Some(x) = o.get(k) {
            let s = match x {
                Value::String(s) => clean_line(s, 300),
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => b.to_string(),
                _ => continue,
            };
            if !s.is_empty() {
                out.insert(k.to_string(), json!(s));
            }
        }
    }
    (!out.is_empty()).then(|| Value::Object(out))
}

async fn open(State(app): State<Shared>, h: HeaderMap, Json(n): Json<NewTicket>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        if !CATEGORIES.contains(&n.category.as_str()) {
            return Err(said(StatusCode::BAD_REQUEST, "category", "no such category"));
        }
        let title = clean_line(&n.title, 120);
        if title.chars().count() < 4 {
            return Err(said(StatusCode::BAD_REQUEST, "title", "a title of 4 to 120 characters"));
        }
        let Some(body) = text_ok(&n.body, 10_000) else {
            return Err(said(StatusCode::BAD_REQUEST, "body", "a message of 1 to 10 000 characters"));
        };
        if crate::forum2::picture_ids(&body).len() > 10 {
            return Err(said(StatusCode::BAD_REQUEST, "pictures", "ten pictures in a message at most"));
        }
        let meta = n.report.as_ref().and_then(report_of);
        let id = db(&app, move |c| -> Result<i64, Response> {
            let open: i64 = c.query_row("SELECT COUNT(*) FROM support_tickets WHERE uid = ?1 AND state != 'closed'", params![uid], |r| r.get(0)).map_err(|_| busy())?;
            if open >= MOST_OPEN && !staff(uid) {
                return Err(said(StatusCode::TOO_MANY_REQUESTS, "many", "three tickets open at most: wait for an answer"));
            }
            let last: Option<i64> = c.query_row("SELECT MAX(created) FROM support_tickets WHERE uid = ?1", params![uid], |r| r.get(0)).map_err(|_| busy())?;
            if last.is_some_and(|l| now() - l < 60) && !staff(uid) {
                return Err(said(StatusCode::TOO_MANY_REQUESTS, "slow", "a new ticket once a minute"));
            }
            let at = now();
            c.execute(
                "INSERT INTO support_tickets (uid, category, title, meta, created, updated) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                params![uid, n.category, title, meta.map(|m| m.to_string()), at],
            )
            .map_err(|_| busy())?;
            let id = c.last_insert_rowid();
            c.execute("INSERT INTO ticket_messages (ticket, uid, body, created) VALUES (?1, ?2, ?3, ?4)", params![id, uid, body, at]).map_err(|_| busy())?;
            let mid = c.last_insert_rowid();
            c.execute("UPDATE support_tickets SET author_read = ?2 WHERE id = ?1", params![id, mid]).map_err(|_| busy())?;
            told(c, id, uid, "new", &crate::notify::excerpt(&body), false);
            Ok(id)
        })
        .await?;
        Ok((StatusCode::CREATED, Json(json!({ "id": id }))).into_response())
    })
    .await
}

/// A ticket and its messages (the staff's notes to the staff only), its files; reading it marks it read.
async fn one(State(app): State<Shared>, h: HeaderMap, Key(id): Key<i64>) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        let uid = w.uid;
        let v = db(&app, move |c| -> Result<Value, Response> {
            let (author, _) = ticket_of(c, id, uid).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            let sees_notes = staff(uid) && uid != author;
            let mut head = c.query_row(&format!("{TICKET} WHERE id = ?1"), params![id], |r| ticket_json(c, r, uid)).map_err(|_| busy())?;
            let rows: Vec<(i64, Option<i64>, String, bool, String, i64)> = c
                .prepare("SELECT id, uid, body, internal, kind, created FROM ticket_messages WHERE ticket = ?1 AND (internal = 0 OR ?2) ORDER BY id")
                .and_then(|mut q| q.query_map(params![id, sees_notes], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))).map(|m| m.flatten().collect()))
                .unwrap_or_default();
            let last = rows.last().map_or(0, |r| r.0);
            let messages: Vec<Value> = rows
                .into_iter()
                .map(|(mid, by, body, internal, kind, created)| {
                    let pictures = crate::forum2::post_extra(c, 0, &body)["pictures"].clone();
                    json!({ "id": mid, "author": by.map(|u| crate::people::card_of(c, u)), "staff": by.is_some_and(|u| u != author), "body": body, "internal": internal, "kind": kind, "created": created, "pictures": pictures })
                })
                .collect();
            let files: Vec<Value> = c
                .prepare("SELECT id, name, size, created, uid FROM ticket_files WHERE ticket = ?1 ORDER BY id")
                .and_then(|mut q| q.query_map(params![id], |r| Ok(json!({ "id": r.get::<_, i64>(0)?, "name": r.get::<_, String>(1)?, "size": r.get::<_, i64>(2)?, "created": r.get::<_, i64>(3)?, "mine": r.get::<_, i64>(4)? == uid }))).map(|m| m.flatten().collect()))
                .unwrap_or_default();
            let column = if uid == author { "author_read" } else { "staff_read" };
            let _ = c.execute(&format!("UPDATE support_tickets SET {column} = MAX({column}, ?2) WHERE id = ?1"), params![id, last]);
            head["messages"] = json!(messages);
            head["files"] = json!(files);
            head["staff_view"] = json!(sees_notes);
            if sees_notes {
                head["device"] = c
                    .query_row("SELECT bound, seen FROM devices WHERE uid = ?1", params![author], |r| Ok(json!({ "bound": r.get::<_, i64>(0)?, "seen": r.get::<_, i64>(1)? })))
                    .optional()
                    .ok()
                    .flatten()
                    .unwrap_or(Value::Null);
            }
            Ok(head)
        })
        .await?;
        Ok(Json(v).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct Say {
    body: String,
    /// Staff: a note only the staff see.
    #[serde(default)]
    internal: bool,
}

async fn reply(State(app): State<Shared>, h: HeaderMap, Key(id): Key<i64>, Json(s): Json<Say>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        let Some(body) = text_ok(&s.body, 10_000) else {
            return Err(said(StatusCode::BAD_REQUEST, "body", "a message of 1 to 10 000 characters"));
        };
        if crate::forum2::picture_ids(&body).len() > 10 {
            return Err(said(StatusCode::BAD_REQUEST, "pictures", "ten pictures in a message at most"));
        }
        let v = db(&app, move |c| -> Result<Value, Response> {
            let (author, state) = ticket_of(c, id, uid).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            let by_staff = uid != author;
            let internal = s.internal && by_staff;
            if state == "closed" && !by_staff {
                return Err(said(StatusCode::CONFLICT, "closed", "this ticket is closed: open a new one"));
            }
            let at = now();
            c.execute("INSERT INTO ticket_messages (ticket, uid, body, internal, created) VALUES (?1, ?2, ?3, ?4, ?5)", params![id, uid, body, internal, at]).map_err(|_| busy())?;
            let mid = c.last_insert_rowid();
            // Staff answering: the author's turn (and it is someone's: the one who answered, unless assigned already).
            // The author answering: the staff's turn again.
            let next = if internal {
                state.clone()
            } else if by_staff {
                "waiting".to_string()
            } else if state == "waiting" || state == "closed" {
                let assigned: Option<i64> = c.query_row("SELECT assignee FROM support_tickets WHERE id = ?1", params![id], |r| r.get(0)).map_err(|_| busy())?;
                if assigned.is_some() { "working".into() } else { "open".into() }
            } else {
                state.clone()
            };
            c.execute(
                &format!("UPDATE support_tickets SET state = ?2, updated = ?3, closed = CASE WHEN ?2 = 'closed' THEN closed ELSE NULL END, {} = ?4 WHERE id = ?1", if by_staff { "staff_read" } else { "author_read" }),
                params![id, next, at, mid],
            )
            .map_err(|_| busy())?;
            if by_staff && !internal {
                c.execute("UPDATE support_tickets SET assignee = COALESCE(assignee, ?2) WHERE id = ?1", params![id, uid]).map_err(|_| busy())?;
            }
            told(c, id, uid, if internal { "note" } else { "reply" }, &crate::notify::excerpt(&body), internal);
            Ok(json!({ "id": mid, "state": next }))
        })
        .await?;
        Ok((StatusCode::CREATED, Json(v)).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct StateTo {
    state: String,
}

/// Staff move a ticket between states; its author may close it (and open it again).
async fn set_state(State(app): State<Shared>, h: HeaderMap, Key(id): Key<i64>, Json(b): Json<StateTo>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        if !STATES.contains(&b.state.as_str()) {
            return Err(said(StatusCode::BAD_REQUEST, "state", "no such state"));
        }
        db(&app, move |c| -> Result<(), Response> {
            let (author, was) = ticket_of(c, id, uid).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            let by_staff = uid != author;
            if !by_staff && !(b.state == "closed" || (was == "closed" && b.state == "open")) {
                return Err(fail(StatusCode::FORBIDDEN, "staff"));
            }
            if was == b.state {
                return Ok(());
            }
            let at = now();
            c.execute("UPDATE support_tickets SET state = ?2, updated = ?3, closed = CASE WHEN ?2 = 'closed' THEN ?3 ELSE NULL END WHERE id = ?1", params![id, b.state, at]).map_err(|_| busy())?;
            if b.state == "working" {
                c.execute("UPDATE support_tickets SET assignee = COALESCE(assignee, ?2) WHERE id = ?1", params![id, uid]).map_err(|_| busy())?;
            }
            system(c, id, uid, &format!("state:{}", b.state));
            if by_staff {
                crate::people::audit(c, uid, "ticket.state", &id.to_string(), json!({ "from": was, "to": b.state }));
            }
            told(c, id, uid, &format!("state:{}", b.state), "", false);
            Ok(())
        })
        .await?;
        Ok(Json(json!({})).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct Assign {
    /// Someone of the staff; none: nobody.
    login: Option<String>,
}

async fn assign(State(app): State<Shared>, h: HeaderMap, Key(id): Key<i64>, Json(b): Json<Assign>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        if !staff(uid) {
            return Err(fail(StatusCode::FORBIDDEN, "staff"));
        }
        let v = db(&app, move |c| -> Result<Value, Response> {
            ticket_of(c, id, uid).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            let to = match b.login.as_deref().filter(|l| !l.is_empty()) {
                Some(l) => {
                    let u = crate::people::uid_of(c, l).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
                    if !staff(u) {
                        return Err(said(StatusCode::BAD_REQUEST, "staff", "only someone who handles tickets"));
                    }
                    Some(u)
                }
                None => None,
            };
            c.execute(
                "UPDATE support_tickets SET assignee = ?2, state = CASE WHEN state = 'open' AND ?2 IS NOT NULL THEN 'working' ELSE state END, updated = ?3 WHERE id = ?1",
                params![id, to, now()],
            )
            .map_err(|_| busy())?;
            let to_login: String = to.and_then(|u| c.query_row("SELECT login FROM users WHERE uid = ?1", params![u], |r| r.get(0)).ok()).unwrap_or_default();
            system(c, id, uid, &format!("assigned:{to_login}"));
            crate::people::audit(c, uid, "ticket.assign", &id.to_string(), json!({ "to": to }));
            if let Some(u) = to.filter(|u| *u != uid) {
                let title: String = c.query_row("SELECT title FROM support_tickets WHERE id = ?1", params![id], |r| r.get(0)).unwrap_or_default();
                crate::notify::add(c, u, "ticket", Some(uid), &format!("assigned:{id}"), json!({ "ticket": id, "title": title, "what": "assigned" }));
            }
            told(c, id, uid, "assigned", "", true);
            Ok(json!({ "assignee": to.map(|u| crate::people::card_of(c, u)) }))
        })
        .await?;
        Ok(Json(v).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct Rate {
    rating: i64,
    #[serde(default)]
    note: String,
}

async fn rate(State(app): State<Shared>, h: HeaderMap, Key(id): Key<i64>, Json(b): Json<Rate>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        if !(1..=5).contains(&b.rating) {
            return Err(said(StatusCode::BAD_REQUEST, "rating", "from 1 to 5"));
        }
        let note = clean_line(&b.note, 300);
        db(&app, move |c| -> Result<(), Response> {
            let (author, state) = ticket_of(c, id, uid).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            if author != uid {
                return Err(fail(StatusCode::FORBIDDEN, "not-yours"));
            }
            if state != "closed" {
                return Err(said(StatusCode::CONFLICT, "open", "rated once it is closed"));
            }
            c.execute("UPDATE support_tickets SET rating = ?2, rating_note = ?3 WHERE id = ?1", params![id, b.rating, if note.is_empty() { None } else { Some(note) }]).map_err(|_| busy())?;
            told(c, id, uid, "rated", "", true);
            Ok(())
        })
        .await?;
        Ok(Json(json!({})).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct FileName {
    name: Option<String>,
}

/// A file for a ticket (the game's log): its author's or the staff's, kept compressed out of the site's reach.
async fn attach(State(app): State<Shared>, h: HeaderMap, Key(id): Key<i64>, Query(q): Query<FileName>, body: Bytes) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        if body.is_empty() || body.len() > MOST_FILE {
            return Err(said(StatusCode::BAD_REQUEST, "file", "a file of at most 4 MB"));
        }
        let name: String = clean_line(q.name.as_deref().unwrap_or("file.txt"), 80).chars().filter(|c| c.is_alphanumeric() || "._- ".contains(*c)).collect();
        let name = if name.is_empty() { "file.txt".to_string() } else { name };
        let dir = files_dir(&app);
        let id_ok = db(&app, move |c| -> Result<(), Response> {
            let (_, state) = ticket_of(c, id, uid).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            if state == "closed" && !staff(uid) {
                return Err(said(StatusCode::CONFLICT, "closed", "this ticket is closed"));
            }
            let n: i64 = c.query_row("SELECT COUNT(*) FROM ticket_files WHERE ticket = ?1", params![id], |r| r.get(0)).map_err(|_| busy())?;
            if n >= FILES_PER_TICKET {
                return Err(said(StatusCode::BAD_REQUEST, "many", "six files in a ticket at most"));
            }
            Ok(())
        })
        .await;
        id_ok?;
        let size = body.len() as i64;
        let file = format!("{}-{}.gz", id, crate::random_hex(8));
        let path = dir.join(&file);
        tokio::task::spawn_blocking(move || -> std::io::Result<()> {
            std::fs::create_dir_all(&dir)?;
            let mut gz = flate2::write::GzEncoder::new(std::fs::File::create(&path)?, flate2::Compression::default());
            gz.write_all(&body)?;
            gz.finish()?;
            Ok(())
        })
        .await
        .map_err(|_| busy())?
        .map_err(|_| busy())?;
        let v = db(&app, move |c| -> Result<Value, Response> {
            c.execute("INSERT INTO ticket_files (ticket, uid, name, size, file, created) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", params![id, uid, name, size, file, now()]).map_err(|_| busy())?;
            let fid = c.last_insert_rowid();
            system(c, id, uid, &format!("file:{name}"));
            let _ = c.execute("UPDATE support_tickets SET updated = ?2 WHERE id = ?1", params![id, now()]);
            told(c, id, uid, "file", &name, true);
            Ok(json!({ "file": { "id": fid, "name": name, "size": size } }))
        })
        .await?;
        Ok((StatusCode::CREATED, Json(v)).into_response())
    })
    .await
}

async fn file(State(app): State<Shared>, h: HeaderMap, Key((id, fid)): Key<(i64, i64)>) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        let uid = w.uid;
        let found = db(&app, move |c| -> Result<(String, String), Response> {
            ticket_of(c, id, uid).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            c.query_row("SELECT name, file FROM ticket_files WHERE id = ?1 AND ticket = ?2", params![fid, id], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()
                .map_err(|_| busy())?
                .ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))
        })
        .await?;
        let path = files_dir(&app).join(&found.1);
        let bytes = tokio::task::spawn_blocking(move || -> std::io::Result<Vec<u8>> {
            let mut out = Vec::new();
            flate2::read::GzDecoder::new(std::fs::File::open(path)?).read_to_end(&mut out)?;
            Ok(out)
        })
        .await
        .map_err(|_| busy())?
        .map_err(|_| fail(StatusCode::NOT_FOUND, "missing"))?;
        let disposition = format!("attachment; filename=\"{}\"", found.0.replace('"', ""));
        Ok(([(header::CONTENT_TYPE, "text/plain; charset=utf-8".to_string()), (header::CONTENT_DISPOSITION, disposition)], bytes).into_response())
    })
    .await
}

/// Staff free the computer of a ticket's author (at any time).
async fn device_reset(State(app): State<Shared>, h: HeaderMap, Key(id): Key<i64>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        if !crate::people::can(uid, crate::people::USERS_DEVICE) {
            return Err(fail(StatusCode::FORBIDDEN, "staff"));
        }
        db(&app, move |c| -> Result<(), Response> {
            let (author, _) = ticket_of(c, id, uid).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            let _ = crate::beta::free_device(c, author, false);
            system(c, id, uid, "device-reset");
            crate::people::audit(c, uid, "device.reset", &author.to_string(), json!({ "ticket": id }));
            told(c, id, uid, "device-reset", "", false);
            Ok(())
        })
        .await?;
        Ok(Json(json!({})).into_response())
    })
    .await
}

// ------------------------------------------------------------------------------------------------ ready answers

async fn replies(State(app): State<Shared>, h: HeaderMap) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        if !staff(w.uid) {
            return Err(fail(StatusCode::FORBIDDEN, "staff"));
        }
        let v = db(&app, |c| -> rusqlite::Result<Value> {
            let rows: Vec<Value> = c
                .prepare("SELECT id, title, body FROM ticket_replies ORDER BY title")?
                .query_map([], |r| Ok(json!({ "id": r.get::<_, i64>(0)?, "title": r.get::<_, String>(1)?, "body": r.get::<_, String>(2)? })))?
                .flatten()
                .collect();
            Ok(json!({ "items": rows }))
        })
        .await
        .map_err(|_| busy())?;
        Ok(Json(v).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct Ready {
    title: String,
    body: String,
}

async fn add_reply(State(app): State<Shared>, h: HeaderMap, Json(b): Json<Ready>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        if !staff(uid) {
            return Err(fail(StatusCode::FORBIDDEN, "staff"));
        }
        let title = clean_line(&b.title, 60);
        let Some(body) = text_ok(&b.body, 4000).filter(|_| title.chars().count() >= 2) else {
            return Err(said(StatusCode::BAD_REQUEST, "body", "a name and the words"));
        };
        let id = db(&app, move |c| c.execute("INSERT INTO ticket_replies (title, body, by, created) VALUES (?1, ?2, ?3, ?4)", params![title, body, uid, now()]).map(|_| c.last_insert_rowid()))
            .await
            .map_err(|_| busy())?;
        Ok((StatusCode::CREATED, Json(json!({ "id": id }))).into_response())
    })
    .await
}

async fn drop_reply(State(app): State<Shared>, h: HeaderMap, Key(id): Key<i64>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        if !staff(w.uid) {
            return Err(fail(StatusCode::FORBIDDEN, "staff"));
        }
        db(&app, move |c| c.execute("DELETE FROM ticket_replies WHERE id = ?1", params![id])).await.map_err(|_| busy())?;
        Ok(Json(json!({})).into_response())
    })
    .await
}

// ------------------------------------------------------------------------------------------------ one's own computer

/// The computer one's account is bound to: since when, last seen, when it may be freed by oneself next.
async fn my_device(State(app): State<Shared>, h: HeaderMap) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        let uid = w.uid;
        let v = db(&app, move |c| {
            let bound = c.query_row("SELECT bound, seen FROM devices WHERE uid = ?1", params![uid], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))).optional().ok().flatten();
            let from = c.query_row("SELECT at FROM device_resets WHERE uid = ?1", params![uid], |r| r.get::<_, i64>(0)).optional().ok().flatten().map_or(0, |at| at + 86_400);
            json!({ "bound": bound.map(|b| b.0), "seen": bound.map(|b| b.1), "reset_from": from, "now": now() })
        })
        .await;
        Ok(Json(v).into_response())
    })
    .await
}

/// One's own computer freed (as the loader's button): the next start of the loader binds the computer it runs on.
async fn my_device_reset(State(app): State<Shared>, h: HeaderMap) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        db(&app, move |c| {
            if crate::beta::ban_of(c, uid).is_some() {
                return Err(said(StatusCode::FORBIDDEN, "banned", "a banned account keeps its computer"));
            }
            crate::beta::free_device(c, uid, true).map_err(|from| (StatusCode::TOO_MANY_REQUESTS, Json(json!({ "error": "wait", "until": from }))).into_response())?;
            Ok(())
        })
        .await?;
        Ok(Json(json!({ "reset_from": now() + 86_400 })).into_response())
    })
    .await
}

pub fn routes() -> Router<Shared> {
    let json = || DefaultBodyLimit::max(32 * 1024);
    Router::new()
        .route("/api/tickets", get(list).post(open).layer(json()))
        .route("/api/tickets/{id}", get(one))
        .route("/api/tickets/{id}/messages", post(reply).layer(json()))
        .route("/api/tickets/{id}/state", post(set_state).layer(json()))
        .route("/api/tickets/{id}/assign", post(assign).layer(json()))
        .route("/api/tickets/{id}/rate", post(rate).layer(json()))
        .route("/api/tickets/{id}/files", post(attach).layer(DefaultBodyLimit::max(MOST_FILE + 1024)))
        .route("/api/tickets/{id}/files/{fid}", get(file))
        .route("/api/tickets/{id}/device-reset", post(device_reset))
        .route("/api/staff/replies", get(replies).post(add_reply).layer(json()))
        .route("/api/staff/replies/{id}", delete(drop_reply))
        .route("/api/me/device", get(my_device))
        .route("/api/me/device/reset", post(my_device_reset))
}
