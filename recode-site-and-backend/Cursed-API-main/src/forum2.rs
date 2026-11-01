//! The forum's second floor (forum.rs is the first): what threads and posts do beyond being written.
//!
//! - Bugs: a status (new → confirmed → fixed; or won't fix, or a duplicate). Confirmed, its author gets the badge of a
//!   bug found; fixed, the fix waits for the next build of the client, and when that build is published its author is
//!   told "fixed in build N" by themselves (dist.rs calls `builds_out`).
//! - Ideas: votes, and a status (planned, being made, done, declined); done gives its author the idea's badge.
//! - Help: one reply marked the answer (by the thread's author or staff): the thread is solved, the reply's author
//!   counts a best answer.
//! - Posts: the pack's emoji as reactions; pictures put in by a token `[img:<id>]` on a line of its own (uploaded
//!   first, kept as the member's media); YouTube links shown as cards (their picture kept here — the site's rules let
//!   in no picture of another site); every edit kept, the history readable.
//! - Threads followed: their author and whoever replied hear every new reply (and anyone may follow or stop).
//! - Staff: a warning (kept, the member told), a mute on the forum for a while (posting refused till then).

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path as Key, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::market::{busy, member, run, said};
use crate::{db, fail, now, Shared};

pub const BUG_STATES: &[&str] = &["new", "confirmed", "fixed", "wontfix", "duplicate"];
pub const IDEA_STATES: &[&str] = &["open", "planned", "doing", "done", "declined"];

pub fn schema(c: &Connection) {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS post_reactions (
             post INTEGER NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
             uid INTEGER NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
             emoji TEXT NOT NULL,
             at INTEGER NOT NULL,
             PRIMARY KEY (post, uid, emoji));
         CREATE TABLE IF NOT EXISTS post_edits (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             post INTEGER NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
             body TEXT NOT NULL,
             at INTEGER NOT NULL);
         CREATE INDEX IF NOT EXISTS post_edits_post ON post_edits(post);
         CREATE TABLE IF NOT EXISTS thread_subs (
             thread INTEGER NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
             uid INTEGER NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
             on_ INTEGER NOT NULL DEFAULT 1,
             PRIMARY KEY (thread, uid));
         CREATE TABLE IF NOT EXISTS thread_votes (
             thread INTEGER NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
             uid INTEGER NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
             at INTEGER NOT NULL,
             PRIMARY KEY (thread, uid));
         CREATE TABLE IF NOT EXISTS link_cards (
             video TEXT PRIMARY KEY,
             title TEXT NOT NULL,
             author TEXT NOT NULL,
             thumb TEXT,
             at INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS forum_mutes (
             uid INTEGER PRIMARY KEY REFERENCES users(uid) ON DELETE CASCADE,
             until INTEGER NOT NULL,
             why TEXT NOT NULL,
             by INTEGER NOT NULL,
             at INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS warnings (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             uid INTEGER NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
             by INTEGER NOT NULL,
             why TEXT NOT NULL,
             at INTEGER NOT NULL);",
    )
    .expect("make the forum's second tables");
    for (col, def) in [("status", "TEXT NOT NULL DEFAULT ''"), ("fixed_in", "INTEGER"), ("fix_pending", "INTEGER NOT NULL DEFAULT 0"), ("solution", "INTEGER"), ("votes", "INTEGER NOT NULL DEFAULT 0")] {
        let has: bool = c.prepare("SELECT 1 FROM pragma_table_info('threads') WHERE name = ?1").and_then(|mut q| q.exists(params![col])).unwrap_or(false);
        if !has {
            c.execute(&format!("ALTER TABLE threads ADD COLUMN {col} {def}"), []).expect("a thread's new column");
        }
    }
}

// ------------------------------------------------------------------------------------------------ what a thread and a post show

/// The second floor's part of a thread, as its reader sees it.
pub fn thread_extra(c: &Connection, id: i64, viewer: i64) -> Value {
    let (status, fixed_in, pending, solution, votes): (String, Option<i64>, i64, Option<i64>, i64) = c
        .query_row("SELECT status, fixed_in, fix_pending, solution, votes FROM threads WHERE id = ?1", params![id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
        .unwrap_or_default();
    let voted = c.query_row("SELECT 1 FROM thread_votes WHERE thread = ?1 AND uid = ?2", params![id, viewer], |_| Ok(())).optional().ok().flatten().is_some();
    json!({ "status": status, "fixed_in": fixed_in, "fix_pending": pending == 1, "solution": solution, "votes": votes, "voted": voted, "following": following(c, id, viewer) })
}

/// Whether a member follows a thread (its author and those who replied do, unless they stopped).
pub fn following(c: &Connection, thread: i64, uid: i64) -> bool {
    c.query_row("SELECT on_ FROM thread_subs WHERE thread = ?1 AND uid = ?2", params![thread, uid], |r| r.get::<_, i64>(0)).optional().ok().flatten() == Some(1)
}

/// Followed from now on, unless they stopped by hand before.
pub fn follow(c: &Connection, thread: i64, uid: i64) {
    let _ = c.execute("INSERT OR IGNORE INTO thread_subs (thread, uid, on_) VALUES (?1, ?2, 1)", params![thread, uid]);
}

pub fn followers(c: &Connection, thread: i64) -> Vec<i64> {
    c.prepare("SELECT uid FROM thread_subs WHERE thread = ?1 AND on_ = 1").and_then(|mut q| q.query_map(params![thread], |r| r.get(0)).map(|m| m.flatten().collect())).unwrap_or_default()
}

fn reactions(c: &Connection, post: i64) -> Value {
    let rows: Vec<(String, i64)> =
        c.prepare("SELECT emoji, uid FROM post_reactions WHERE post = ?1 ORDER BY at").and_then(|mut q| q.query_map(params![post], |r| Ok((r.get(0)?, r.get(1)?))).map(|m| m.flatten().collect())).unwrap_or_default();
    let mut order: Vec<String> = Vec::new();
    let mut who: std::collections::HashMap<String, Vec<i64>> = std::collections::HashMap::new();
    for (e, u) in rows {
        if !who.contains_key(&e) {
            order.push(e.clone());
        }
        who.entry(e).or_default().push(u);
    }
    json!(order.into_iter().map(|e| json!({ "emoji": e, "uids": who.remove(&e).unwrap_or_default() })).collect::<Vec<_>>())
}

/// The pictures a body puts in (`[img:<id>]`), as their uploads are.
pub fn picture_ids(body: &str) -> Vec<String> {
    body.lines()
        .filter_map(|l| l.trim().strip_prefix("[img:").and_then(|r| r.strip_suffix(']')))
        .filter(|id| id.len() == 16 && id.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(String::from)
        .collect()
}

/// YouTube videos a body links to (watch?v=, youtu.be/, shorts/, embed/), by their id.
pub fn videos(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    for word in body.split_whitespace() {
        let w = word.trim_matches(|c: char| matches!(c, '(' | ')' | '<' | '>' | '"' | '\'' | ','));
        let id = if let Some(i) = w.find("youtube.com/watch") {
            w[i..].split(['?', '&']).find_map(|p| p.strip_prefix("v=")).map(String::from)
        } else if let Some(i) = w.find("youtu.be/") {
            Some(w[i + 9..].split(['?', '&', '/']).next().unwrap_or("").to_string())
        } else if let Some(i) = w.find("youtube.com/shorts/") {
            Some(w[i + 19..].split(['?', '&', '/']).next().unwrap_or("").to_string())
        } else {
            w.find("youtube.com/embed/").map(|i| w[i + 18..].split(['?', '&', '/']).next().unwrap_or("").to_string())
        };
        if let Some(id) = id.filter(|id| id.len() == 11 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')) {
            if !out.contains(&id) {
                out.push(id);
            }
        }
    }
    out
}

/// The second floor's part of a post: reactions, the pictures it puts in, its cards, how many times it was edited.
pub fn post_extra(c: &Connection, post: i64, body: &str) -> Value {
    let mut pictures = Map::new();
    for id in picture_ids(body) {
        if let Some(info) = c
            .query_row("SELECT info FROM media WHERE id = ?1 AND state = 'ok'", params![id], |r| r.get::<_, String>(0))
            .optional()
            .ok()
            .flatten()
            .and_then(|i| serde_json::from_str::<Value>(&i).ok())
        {
            pictures.insert(id, info);
        }
    }
    let mut cards = Map::new();
    for v in videos(body) {
        if let Some((title, author, thumb)) = c
            .query_row("SELECT title, author, thumb FROM link_cards WHERE video = ?1", params![v], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?)))
            .optional()
            .ok()
            .flatten()
        {
            cards.insert(v, json!({ "title": title, "author": author, "thumb": thumb }));
        }
    }
    let edits: i64 = c.query_row("SELECT COUNT(*) FROM post_edits WHERE post = ?1", params![post], |r| r.get(0)).unwrap_or(0);
    json!({ "reactions": reactions(c, post), "pictures": pictures, "cards": cards, "edits": edits })
}

/// A post about to change: what it said is kept.
pub fn keep_edit(c: &Connection, post: i64) {
    let _ = c.execute("INSERT INTO post_edits (post, body, at) SELECT id, body, COALESCE(edited, created) FROM posts WHERE id = ?1", params![post]);
}

/// Whether a member may not post now: until when, and why.
pub fn muted(c: &Connection, uid: i64) -> Option<(i64, String)> {
    c.query_row("SELECT until, why FROM forum_mutes WHERE uid = ?1 AND until > ?2", params![uid, now()], |r| Ok((r.get(0)?, r.get(1)?))).optional().ok().flatten()
}

pub fn muted_answer(until: i64, why: &str) -> Response {
    (StatusCode::FORBIDDEN, Json(json!({ "error": "muted", "until": until, "message": format!("you can't post on the forum until later: {why}") }))).into_response()
}

// ------------------------------------------------------------------------------------------------ YouTube's cards

/// Cards for the videos a post links to, made in the background: the title and channel from YouTube's oEmbed, the
/// picture kept as an upload of the site's own.
pub fn cards_for(app: &Shared, body: &str) {
    let ids = videos(body);
    if ids.is_empty() {
        return;
    }
    let app = app.clone();
    tokio::spawn(async move {
        for id in ids {
            let known = {
                let id = id.clone();
                db(&app, move |c| c.query_row("SELECT 1 FROM link_cards WHERE video = ?1", params![id], |_| Ok(())).optional().ok().flatten().is_some()).await
            };
            if known {
                continue;
            }
            let media = app.market.media.clone();
            let id2 = id.clone();
            let made = tokio::task::spawn_blocking(move || card(&id2, &media)).await.ok().flatten();
            if let Some((title, author, thumb)) = made {
                let _ = db(&app, move |c| c.execute("INSERT OR REPLACE INTO link_cards (video, title, author, thumb, at) VALUES (?1, ?2, ?3, ?4, ?5)", params![id, title, author, thumb, now()])).await;
            }
        }
    });
}

fn card(id: &str, media: &std::path::Path) -> Option<(String, String, Option<String>)> {
    let agent = ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(8)).build();
    let url = format!("https://www.youtube.com/oembed?format=json&url=https%3A%2F%2Fwww.youtube.com%2Fwatch%3Fv%3D{id}");
    let v: Value = agent.get(&url).call().ok()?.into_json().ok()?;
    let title = crate::market::clean_line(v["title"].as_str()?, 200);
    let author = crate::market::clean_line(v["author_name"].as_str().unwrap_or(""), 100);
    let thumb = (|| {
        let resp = agent.get(&format!("https://i.ytimg.com/vi/{id}/hqdefault.jpg")).call().ok()?;
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut std::io::Read::take(resp.into_reader(), 2 << 20), &mut bytes).ok()?;
        let made = crate::media::shot(&bytes, media).ok()?;
        made.info["thumb"].as_str().map(String::from)
    })();
    Some((title, author, thumb))
}

// ------------------------------------------------------------------------------------------------ the doors

fn moderator(uid: i64) -> bool {
    crate::people::can(uid, crate::people::FORUM_MODERATE)
}

fn thread_head(c: &Connection, id: i64) -> Option<(String, i64, String, String)> {
    c.query_row("SELECT category, author, status, title FROM threads WHERE id = ?1 AND state != 'removed'", params![id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).optional().ok().flatten()
}

#[derive(Deserialize)]
struct Status {
    status: String,
}

/// A bug's or an idea's status, set by staff; what it means for its author follows.
async fn set_status(State(app): State<Shared>, h: HeaderMap, Key(id): Key<i64>, Json(b): Json<Status>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        if !moderator(uid) {
            return Err(fail(StatusCode::FORBIDDEN, "staff"));
        }
        db(&app, move |c| -> Result<(), Response> {
            let (cat, author, was, title) = thread_head(c, id).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            let states = match cat.as_str() {
                "bugs" => BUG_STATES,
                "ideas" => IDEA_STATES,
                _ => return Err(said(StatusCode::BAD_REQUEST, "category", "only bugs and ideas have a status")),
            };
            if !states.contains(&b.status.as_str()) {
                return Err(said(StatusCode::BAD_REQUEST, "status", "no such status"));
            }
            if was == b.status {
                return Ok(());
            }
            c.execute("UPDATE threads SET status = ?2 WHERE id = ?1", params![id, b.status]).map_err(|_| busy())?;
            let data = json!({ "thread": id, "title": title, "status": b.status, "category": cat });
            match (cat.as_str(), b.status.as_str()) {
                ("bugs", "confirmed") => {
                    crate::people::award(c, author, "bug_found", 1, None);
                }
                ("bugs", "fixed") => {
                    // The fix comes out with the next build: its author hears then (builds_out).
                    c.execute("UPDATE threads SET fix_pending = 1, fixed_in = NULL WHERE id = ?1", params![id]).map_err(|_| busy())?;
                }
                ("ideas", "done") => {
                    crate::people::bump(c, author, "ideas_done", 1);
                }
                _ => {}
            }
            if was == "fixed" && b.status != "fixed" {
                c.execute("UPDATE threads SET fix_pending = 0 WHERE id = ?1", params![id]).map_err(|_| busy())?;
            }
            crate::notify::add(c, author, "forum", Some(uid), &format!("status:{id}"), data);
            crate::people::audit(c, uid, "forum.status", &id.to_string(), json!({ "from": was, "to": b.status }));
            Ok(())
        })
        .await?;
        Ok(Json(json!({})).into_response())
    })
    .await
}

/// A build of the client published: the bugs fixed meanwhile are fixed in it, and their authors told (and count one
/// more bug of theirs fixed).
fn builds_out(c: &Connection, build: i64) {
    let rows: Vec<(i64, i64, String)> = c
        .prepare("SELECT id, author, title FROM threads WHERE fix_pending = 1 AND category = 'bugs'")
        .and_then(|mut q| q.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).map(|m| m.flatten().collect()))
        .unwrap_or_default();
    for (id, author, title) in rows {
        let _ = c.execute("UPDATE threads SET fix_pending = 0, fixed_in = ?2 WHERE id = ?1", params![id, build]);
        crate::people::bump(c, author, "bugs_fixed", 1);
        crate::notify::add(c, author, "forum", None, &format!("fixed:{id}"), json!({ "thread": id, "title": title, "status": "released", "build": build, "category": "bugs" }));
    }
}

/// Builds are published by the command line (`cursed dist publish`), not by this process: it looks for a new one
/// every half a minute, so what it tells goes down the live line at once.
pub fn watch(app: Shared) {
    tokio::spawn(async move {
        loop {
            db(&app, |c| {
                let newest: i64 = c.query_row("SELECT COALESCE(MAX(build), 0) FROM releases", [], |r| r.get(0)).unwrap_or(0);
                let known: Option<i64> =
                    c.query_row("SELECT value FROM settings WHERE name = 'forum_build'", [], |r| r.get::<_, String>(0)).optional().ok().flatten().and_then(|v| v.parse().ok());
                if known != Some(newest) {
                    if known.is_some() && newest > 0 {
                        builds_out(c, newest);
                    }
                    let _ = c.execute("INSERT INTO settings (name, value) VALUES ('forum_build', ?1) ON CONFLICT(name) DO UPDATE SET value = ?1", params![newest.to_string()]);
                }
            })
            .await;
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        }
    });
}

/// An idea voted for, or the vote taken back.
async fn vote(State(app): State<Shared>, h: HeaderMap, Key(id): Key<i64>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        let v = db(&app, move |c| -> Result<Value, Response> {
            let (cat, _, _, _) = thread_head(c, id).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            if cat != "ideas" {
                return Err(said(StatusCode::BAD_REQUEST, "category", "only ideas are voted for"));
            }
            let gone = c.execute("DELETE FROM thread_votes WHERE thread = ?1 AND uid = ?2", params![id, uid]).map_err(|_| busy())?;
            if gone == 0 {
                c.execute("INSERT INTO thread_votes (thread, uid, at) VALUES (?1, ?2, ?3)", params![id, uid, now()]).map_err(|_| busy())?;
            }
            c.execute("UPDATE threads SET votes = (SELECT COUNT(*) FROM thread_votes WHERE thread = ?1) WHERE id = ?1", params![id]).map_err(|_| busy())?;
            let votes: i64 = c.query_row("SELECT votes FROM threads WHERE id = ?1", params![id], |r| r.get(0)).unwrap_or(0);
            Ok(json!({ "votes": votes, "voted": gone == 0 }))
        })
        .await?;
        Ok(Json(v).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct Solution {
    post: Option<i64>,
}

/// A reply marked the answer of a question (by the thread's author or staff), or unmarked.
async fn solve(State(app): State<Shared>, h: HeaderMap, Key(id): Key<i64>, Json(b): Json<Solution>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        db(&app, move |c| -> Result<(), Response> {
            let (cat, author, _, title) = thread_head(c, id).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            if cat != "help" {
                return Err(said(StatusCode::BAD_REQUEST, "category", "only questions have an answer"));
            }
            if author != uid && !moderator(uid) {
                return Err(fail(StatusCode::FORBIDDEN, "not-yours"));
            }
            let was: Option<i64> = c.query_row("SELECT solution FROM threads WHERE id = ?1", params![id], |r| r.get(0)).map_err(|_| busy())?;
            match b.post {
                Some(p) => {
                    let (thread, by): (i64, i64) = c
                        .query_row("SELECT thread, author FROM posts WHERE id = ?1 AND state = 'shown'", params![p], |r| Ok((r.get(0)?, r.get(1)?)))
                        .optional()
                        .map_err(|_| busy())?
                        .ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
                    if thread != id {
                        return Err(fail(StatusCode::BAD_REQUEST, "thread"));
                    }
                    c.execute("UPDATE threads SET solution = ?2, status = 'solved' WHERE id = ?1", params![id, p]).map_err(|_| busy())?;
                    if was != Some(p) && by != author {
                        crate::people::bump(c, by, "best_answers", 1);
                        crate::notify::add(c, by, "forum", Some(uid), &format!("answer:{p}"), json!({ "thread": id, "title": title, "status": "answer", "category": "help" }));
                    }
                }
                None => {
                    c.execute("UPDATE threads SET solution = NULL, status = '' WHERE id = ?1", params![id]).map_err(|_| busy())?;
                }
            }
            Ok(())
        })
        .await?;
        Ok(Json(json!({})).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct Follow {
    on: bool,
}

async fn set_follow(State(app): State<Shared>, h: HeaderMap, Key(id): Key<i64>, Json(b): Json<Follow>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        db(&app, move |c| -> Result<(), Response> {
            thread_head(c, id).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            c.execute(
                "INSERT INTO thread_subs (thread, uid, on_) VALUES (?1, ?2, ?3) ON CONFLICT(thread, uid) DO UPDATE SET on_ = ?3",
                params![id, uid, b.on as i64],
            )
            .map_err(|_| busy())?;
            Ok(())
        })
        .await?;
        Ok(Json(json!({ "following": b.on })).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct React {
    emoji: String,
}

async fn react(State(app): State<Shared>, h: HeaderMap, Key(id): Key<i64>, Json(b): Json<React>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        if !crate::people::EMOJI.contains(&b.emoji.as_str()) {
            return Err(said(StatusCode::BAD_REQUEST, "emoji", "one of the pack's emoji"));
        }
        let v = db(&app, move |c| -> Result<Value, Response> {
            let (author, state): (i64, String) =
                c.query_row("SELECT author, state FROM posts WHERE id = ?1", params![id], |r| Ok((r.get(0)?, r.get(1)?))).optional().map_err(|_| busy())?.ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            if state != "shown" {
                return Err(fail(StatusCode::NOT_FOUND, "missing"));
            }
            let gone = c.execute("DELETE FROM post_reactions WHERE post = ?1 AND uid = ?2 AND emoji = ?3", params![id, uid, b.emoji]).map_err(|_| busy())?;
            if gone == 0 {
                let kinds: i64 = c.query_row("SELECT COUNT(DISTINCT emoji) FROM post_reactions WHERE post = ?1", params![id], |r| r.get(0)).unwrap_or(0);
                if kinds >= 12 {
                    return Err(said(StatusCode::BAD_REQUEST, "many", "twelve kinds of reaction at most"));
                }
                c.execute("INSERT INTO post_reactions (post, uid, emoji, at) VALUES (?1, ?2, ?3, ?4)", params![id, uid, b.emoji, now()]).map_err(|_| busy())?;
                crate::people::recount(c, uid, "reactions_given");
                if author != uid {
                    crate::people::bump(c, author, "reactions_got", 1);
                }
            } else if author != uid {
                crate::people::bump(c, author, "reactions_got", -1);
            }
            Ok(json!({ "reactions": reactions(c, id) }))
        })
        .await?;
        Ok(Json(v).into_response())
    })
    .await
}

/// What a post said before each edit (its author and staff).
async fn history(State(app): State<Shared>, h: HeaderMap, Key(id): Key<i64>) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        let uid = w.uid;
        let v = db(&app, move |c| -> Result<Value, Response> {
            let author: i64 = c.query_row("SELECT author FROM posts WHERE id = ?1", params![id], |r| r.get(0)).optional().map_err(|_| busy())?.ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            if author != uid && !moderator(uid) {
                return Err(fail(StatusCode::FORBIDDEN, "not-yours"));
            }
            let rows: Vec<Value> = c
                .prepare("SELECT body, at FROM post_edits WHERE post = ?1 ORDER BY id DESC")
                .and_then(|mut q| q.query_map(params![id], |r| Ok(json!({ "body": r.get::<_, String>(0)?, "at": r.get::<_, i64>(1)? }))).map(|m| m.flatten().collect()))
                .unwrap_or_default();
            Ok(json!({ "versions": rows }))
        })
        .await?;
        Ok(Json(v).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct PictureFor {
    /// "ticket": for a message to the staff (a mute of the forum does not stop it: an appeal goes this way).
    #[serde(rename = "for")]
    target: Option<String>,
}

/// A picture for a post (or a ticket): kept as the member's upload; the text puts it in with `[img:<id>]`.
async fn picture(State(app): State<Shared>, h: HeaderMap, Query(q): Query<PictureFor>, body: Bytes) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        let ticket = q.target.as_deref() == Some("ticket");
        if !ticket {
            if let Some((until, why)) = db(&app, move |c| muted(c, uid)).await {
                return Err(muted_answer(until, &why));
            }
        }
        if body.is_empty() || body.len() > crate::media::MOST_PICTURE {
            return Err(said(StatusCode::BAD_REQUEST, "file", "a picture of at most 12 MB"));
        }
        if !crate::media::paced(uid) {
            return Err(fail(StatusCode::TOO_MANY_REQUESTS, "slow"));
        }
        let left = db(&app, move |c| crate::media::quota_left(c, uid)).await;
        if left < body.len() as i64 {
            return Err(said(StatusCode::INSUFFICIENT_STORAGE, "quota", "your uploads take all of your 300 MB: take some away first"));
        }
        let media = app.market.media.clone();
        let made = tokio::task::spawn_blocking(move || crate::media::shot(&body, &media)).await.map_err(|_| busy())?.map_err(|m| said(StatusCode::BAD_REQUEST, "file", &m))?;
        let info = db(&app, move |c| crate::media::record(c, uid, if ticket { "ticket" } else { "forum" }, &made)).await.map_err(|_| busy())?;
        Ok((StatusCode::CREATED, Json(json!({ "picture": info }))).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct Find {
    q: String,
}

/// Logins starting with what is typed after an @ (eight at most).
async fn find(State(app): State<Shared>, h: HeaderMap, Query(f): Query<Find>) -> Response {
    run(async {
        let _ = member(&app, &h, false).await?;
        let q: String = f.q.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').take(16).collect();
        if q.is_empty() {
            return Ok(Json(json!({ "people": [] })).into_response());
        }
        let v = db(&app, move |c| -> rusqlite::Result<Value> {
            let rows: Vec<Value> = c
                .prepare("SELECT uid FROM users WHERE login LIKE ?1 ESCAPE '\\' ORDER BY length(login), login LIMIT 8")?
                .query_map(params![format!("{}%", q.replace('_', "\\_"))], |r| r.get::<_, i64>(0))?
                .flatten()
                .map(|u| crate::people::card_of(c, u))
                .collect();
            Ok(json!({ "people": rows }))
        })
        .await
        .map_err(|_| busy())?;
        Ok(Json(v).into_response())
    })
    .await
}

// ------------------------------------------------------------------------------------------------ staff

#[derive(Deserialize)]
struct Warn {
    login: String,
    why: String,
}

async fn warn(State(app): State<Shared>, h: HeaderMap, Json(b): Json<Warn>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        if !moderator(uid) {
            return Err(fail(StatusCode::FORBIDDEN, "staff"));
        }
        let why = crate::market::clean_line(&b.why, 300);
        if why.chars().count() < 3 {
            return Err(said(StatusCode::BAD_REQUEST, "why", "say why"));
        }
        let n = db(&app, move |c| -> Result<i64, Response> {
            let target = crate::people::uid_of(c, &b.login).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            c.execute("INSERT INTO warnings (uid, by, why, at) VALUES (?1, ?2, ?3, ?4)", params![target, uid, why, now()]).map_err(|_| busy())?;
            let n: i64 = c.query_row("SELECT COUNT(*) FROM warnings WHERE uid = ?1", params![target], |r| r.get(0)).unwrap_or(0);
            crate::notify::add(c, target, "moderation", Some(uid), &format!("warning:{n}"), json!({ "what": "warning", "why": why, "title": "" }));
            crate::people::audit(c, uid, "forum.warn", &target.to_string(), json!({ "why": why }));
            Ok(n)
        })
        .await?;
        Ok(Json(json!({ "warnings": n })).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct Mute {
    login: String,
    minutes: i64,
    why: String,
}

async fn mute(State(app): State<Shared>, h: HeaderMap, Json(b): Json<Mute>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        if !crate::people::can(uid, crate::people::USERS_MUTE) && !moderator(uid) {
            return Err(fail(StatusCode::FORBIDDEN, "staff"));
        }
        if !(1..=60 * 24 * 365).contains(&b.minutes) {
            return Err(said(StatusCode::BAD_REQUEST, "minutes", "from a minute to a year"));
        }
        let why = crate::market::clean_line(&b.why, 300);
        let until = db(&app, move |c| -> Result<i64, Response> {
            let target = crate::people::uid_of(c, &b.login).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            if crate::people::is_staff(target) {
                return Err(said(StatusCode::BAD_REQUEST, "staff", "staff are not muted"));
            }
            let until = now() + b.minutes * 60;
            c.execute(
                "INSERT INTO forum_mutes (uid, until, why, by, at) VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT(uid) DO UPDATE SET until = ?2, why = ?3, by = ?4, at = ?5",
                params![target, until, why, uid, now()],
            )
            .map_err(|_| busy())?;
            crate::notify::add(c, target, "moderation", Some(uid), &format!("mute:{until}"), json!({ "what": "mute", "why": why, "until": until, "title": "" }));
            crate::people::audit(c, uid, "forum.mute", &target.to_string(), json!({ "minutes": b.minutes, "why": why }));
            Ok(until)
        })
        .await?;
        Ok(Json(json!({ "until": until })).into_response())
    })
    .await
}

async fn unmute(State(app): State<Shared>, h: HeaderMap, Key(login): Key<String>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        if !crate::people::can(uid, crate::people::USERS_MUTE) && !moderator(uid) {
            return Err(fail(StatusCode::FORBIDDEN, "staff"));
        }
        db(&app, move |c| -> Result<(), Response> {
            let target = crate::people::uid_of(c, &login).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            c.execute("DELETE FROM forum_mutes WHERE uid = ?1", params![target]).map_err(|_| busy())?;
            crate::people::audit(c, uid, "forum.unmute", &target.to_string(), json!({}));
            Ok(())
        })
        .await?;
        Ok(Json(json!({})).into_response())
    })
    .await
}

/// A member's record as staff see it: warnings, a mute, reports about them.
async fn record(State(app): State<Shared>, h: HeaderMap, Key(login): Key<String>) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        if !moderator(w.uid) && !crate::people::can(w.uid, crate::people::USERS_VIEW) {
            return Err(fail(StatusCode::FORBIDDEN, "staff"));
        }
        let v = db(&app, move |c| -> Result<Value, Response> {
            let target = crate::people::uid_of(c, &login).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            let warnings: Vec<Value> = c
                .prepare("SELECT by, why, at FROM warnings WHERE uid = ?1 ORDER BY id DESC")
                .and_then(|mut q| q.query_map(params![target], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?))).map(|m| m.flatten().collect::<Vec<_>>()))
                .unwrap_or_default()
                .into_iter()
                .map(|(by, why, at)| json!({ "by": crate::people::card_of(c, by), "why": why, "at": at }))
                .collect();
            let mute = muted(c, target).map(|(until, why)| json!({ "until": until, "why": why }));
            Ok(json!({ "person": crate::people::card_of(c, target), "warnings": warnings, "mute": mute }))
        })
        .await?;
        Ok(Json(v).into_response())
    })
    .await
}

pub fn routes() -> Router<Shared> {
    let json = || DefaultBodyLimit::max(8 * 1024);
    Router::new()
        .route("/api/forum/threads/{id}/status", post(set_status).layer(json()))
        .route("/api/forum/threads/{id}/vote", post(vote))
        .route("/api/forum/threads/{id}/solution", post(solve).layer(json()))
        .route("/api/forum/threads/{id}/follow", post(set_follow).layer(json()))
        .route("/api/forum/posts/{id}/react", post(react).layer(json()))
        .route("/api/forum/posts/{id}/history", get(history))
        .route("/api/forum/pictures", post(picture).layer(DefaultBodyLimit::max(crate::media::MOST_PICTURE + 1024)))
        .route("/api/people/find", get(find))
        .route("/api/staff/warn", post(warn).layer(json()))
        .route("/api/staff/mute", post(mute).layer(json()))
        .route("/api/staff/mute/{login}", delete(unmute))
        .route("/api/staff/record/{login}", get(record))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn videos_are_found_in_every_form_of_their_link() {
        let body = "see https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=10 and youtu.be/abcdefghijk, also (https://youtube.com/shorts/ZZZZZZZZZZZ)\nnot: https://youtube.com/watch?v=short";
        assert_eq!(videos(body), vec!["dQw4w9WgXcQ", "abcdefghijk", "ZZZZZZZZZZZ"]);
    }

    #[test]
    fn pictures_are_lines_of_their_own() {
        let body = "look:\n[img:0123456789abcdef]\nand [img:fedcba9876543210] inline is text\n [img:aaaaaaaaaaaaaaaa] ";
        assert_eq!(picture_ids(body), vec!["0123456789abcdef", "aaaaaaaaaaaaaaaa"]);
    }
}
