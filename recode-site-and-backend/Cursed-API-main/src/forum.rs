//! The forum: categories, threads, posts; reading marks; who is online; reports (of posts, threads and packs) and
//! moderation (UID 0): pin, lock, move, hide with a reason the author sees.
//!
//! Like the market, it is for members only. Posts are plain text as their authors typed it; the site draws links,
//! quotes, @mentions and `code` itself, from escaped text: nothing typed here ever becomes HTML.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::extract::{DefaultBodyLimit, Path as Key, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::market::{busy, clean_line, member, run, said};
use crate::{db, fail, now, Shared, Who, OWNER};

const PER_PAGE: i64 = 30; // posts on a page of a thread
const THREADS_PAGE: i64 = 30;
const ONLINE_FOR: i64 = 5 * 60; // someone who did anything in the last five minutes is online

/// Categories: fixed, in this order. Announcements take threads from her only.
const CATEGORIES: &[(&str, &str, &str, &str, bool)] = &[
    ("announcements", "Announcements", "Updates.", "#F2F1CB", true),
    ("bugs", "Bugs", "Something broken: you hear when it is fixed.", "#BFC0E7", false),
    ("ideas", "Ideas", "What Cursed could have: vote for it.", "#6669F5", false),
    ("help", "Help", "Questions.", "#797992", false),
    ("market", "Market: packs and shaders", "Releases, requests, feedback.", "#6669F5", false),
    ("showcase", "Showcase", "Screenshots.", "#BFC0E7", false),
    ("making", "Making packs", "Tools, formats, CTM, emissive.", "#797992", false),
    // Forum #52 (her word 11.10: "add it"): looking for people to play with, on the servers each names.
    ("teams", "Teammates", "Looking for people to play with: the server, the mode, what you are good at.", "#8E90F7", false),
    ("offtopic", "Off topic", "Everything else.", "#6D6E90", false),
];

/// How often one member may post: the doors against a flood (her posts go unmetered).
pub struct Pace {
    posts: Mutex<HashMap<i64, Vec<Instant>>>,
    threads: Mutex<HashMap<i64, Vec<Instant>>>,
    reports: Mutex<HashMap<i64, Vec<Instant>>>,
    pub online: Mutex<HashMap<i64, (String, i64)>>,
}

impl Pace {
    pub fn new() -> Self {
        Pace { posts: Mutex::new(HashMap::new()), threads: Mutex::new(HashMap::new()), reports: Mutex::new(HashMap::new()), online: Mutex::new(HashMap::new()) }
    }
}

/// Whether one more of something is allowed: at least `gap` since the last, at most `most` within `window`.
fn allowed(book: &Mutex<HashMap<i64, Vec<Instant>>>, uid: i64, gap: Duration, most: usize, window: Duration) -> bool {
    if crate::is_owner(uid) {
        return true;
    }
    let mut b = book.lock().unwrap_or_else(|e| e.into_inner());
    let list = b.entry(uid).or_default();
    list.retain(|t| t.elapsed() < window);
    if list.last().is_some_and(|t| t.elapsed() < gap) || list.len() >= most {
        return false;
    }
    list.push(Instant::now());
    true
}

/// Someone did something: online for the next five minutes.
pub fn seen(app: &Shared, w: &Who) {
    app.pace.online.lock().unwrap_or_else(|e| e.into_inner()).insert(w.uid, (w.login.clone(), now()));
}

pub fn schema(c: &Connection) {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS forum_categories (
             id TEXT PRIMARY KEY, name TEXT NOT NULL, about TEXT NOT NULL, colour TEXT NOT NULL, place INTEGER NOT NULL, closed INTEGER NOT NULL DEFAULT 0);
         CREATE TABLE IF NOT EXISTS threads (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             category TEXT NOT NULL REFERENCES forum_categories(id),
             author INTEGER NOT NULL REFERENCES users(uid),
             title TEXT NOT NULL,
             search TEXT NOT NULL,
             created INTEGER NOT NULL,
             last_at INTEGER NOT NULL,
             last_by INTEGER,
             last_post INTEGER NOT NULL DEFAULT 0,
             replies INTEGER NOT NULL DEFAULT 0,
             pinned INTEGER NOT NULL DEFAULT 0,
             locked INTEGER NOT NULL DEFAULT 0,
             state TEXT NOT NULL DEFAULT 'open',        -- open | hidden | removed
             hidden_why TEXT);
         CREATE INDEX IF NOT EXISTS threads_list ON threads(category, state, pinned, last_at);
         CREATE TABLE IF NOT EXISTS posts (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             thread INTEGER NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
             author INTEGER NOT NULL REFERENCES users(uid),
             body TEXT NOT NULL,
             created INTEGER NOT NULL,
             edited INTEGER,
             state TEXT NOT NULL DEFAULT 'shown',       -- shown | hidden | deleted
             hidden_why TEXT);
         CREATE INDEX IF NOT EXISTS posts_thread ON posts(thread, id);
         CREATE TABLE IF NOT EXISTS reads (uid INTEGER NOT NULL, thread INTEGER NOT NULL, post INTEGER NOT NULL, PRIMARY KEY (uid, thread));
         CREATE TABLE IF NOT EXISTS reports (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             kind TEXT NOT NULL,                        -- post | thread | pack
             target INTEGER NOT NULL,
             by INTEGER NOT NULL REFERENCES users(uid),
             why TEXT NOT NULL,
             created INTEGER NOT NULL,
             closed INTEGER,
             outcome TEXT);",
    )
    .expect("make the forum's tables");
    for (place, (id, name, about, colour, closed)) in CATEGORIES.iter().enumerate() {
        c.execute(
            "INSERT INTO forum_categories (id, name, about, colour, place, closed) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET name = ?2, about = ?3, colour = ?4, place = ?5, closed = ?6",
            params![id, name, about, colour, place as i64, *closed as i64],
        )
        .expect("the categories");
    }
}

pub fn routes() -> Router<Shared> {
    let json = || DefaultBodyLimit::max(64 * 1024);
    Router::new()
        .route("/api/forum", get(index))
        .route("/api/forum/threads", get(threads).post(start).layer(json()))
        .route("/api/forum/threads/{id}", get(thread).patch(retitle).layer(json()))
        .route("/api/forum/threads/{id}/posts", post(reply).layer(json()))
        .route("/api/forum/threads/{id}/hide", post(hide_thread).layer(json()))
        .route("/api/forum/threads/{id}/unhide", post(unhide_thread))
        .route("/api/forum/posts/{id}", patch(edit_post).delete(delete_post).layer(json()))
        .route("/api/forum/posts/{id}/hide", post(hide_post).layer(json()))
        .route("/api/forum/posts/{id}/unhide", post(unhide_post))
        .route("/api/reports", get(reports).post(report).layer(json()))
        .route("/api/reports/{id}/close", post(close_report).layer(json()))
}

// ------------------------------------------------------------------------------------------------ helpers

fn body_ok(s: &str) -> Option<String> {
    let t: String = s.replace("\r\n", "\n").chars().filter(|c| *c == '\n' || *c == '\t' || !c.is_control()).collect();
    let t = t.trim().to_string();
    (1..=10_000).contains(&t.chars().count()).then_some(t)
}

/// Posting refused while a mute of the forum lasts.
async fn not_muted(app: &Shared, uid: i64) -> Result<(), Response> {
    match db(app, move |c| crate::forum2::muted(c, uid)).await {
        Some((until, why)) => Err(crate::forum2::muted_answer(until, &why)),
        None => Ok(()),
    }
}

/// Ten pictures in a post at most.
fn pictures_ok(body: &str) -> Result<(), Response> {
    if crate::forum2::picture_ids(body).len() > 10 {
        return Err(said(StatusCode::BAD_REQUEST, "pictures", "ten pictures in a post at most"));
    }
    Ok(())
}

fn person(uid: i64, login: String) -> Value {
    crate::people::card(uid, login)
}

const THREAD: &str = "SELECT t.id, t.title, t.category, t.author, a.login, t.created, t.last_at, t.last_by, l.login, t.replies, t.pinned, t.locked, t.state, t.hidden_why, t.last_post,
    t.status, t.votes, t.fixed_in, t.solution
    FROM threads t JOIN users a ON a.uid = t.author LEFT JOIN users l ON l.uid = t.last_by";

/// A thread as lists show it; `unread` when it has posts newer than the reader's last visit.
fn thread_row(r: &Row, viewer: i64, read: Option<i64>) -> rusqlite::Result<Value> {
    thread_row_at(r, 0, viewer, read)
}

/// Threads matching `filter` (SQL over t.*), as a reader sees them: hidden ones only to their author and to her.
fn list_threads(c: &Connection, viewer: i64, filter: &str, args: &[&dyn rusqlite::ToSql], order: &str, limit: i64, offset: i64) -> rusqlite::Result<Vec<Value>> {
    let seen = if crate::people::can(viewer, crate::people::FORUM_MODERATE) { "t.state != 'removed'".to_string() } else { format!("(t.state = 'open' OR (t.state = 'hidden' AND t.author = {viewer}))") };
    let sql = format!("{THREAD} LEFT JOIN reads r ON r.thread = t.id AND r.uid = {viewer} WHERE {seen} AND {filter} ORDER BY {order} LIMIT {limit} OFFSET {offset}");
    let sql = sql.replacen("SELECT t.id", "SELECT r.post AS read_post, t.id", 1);
    let mut q = c.prepare(&sql)?;
    let rows = q.query_map(args, |r| {
        let read: Option<i64> = r.get(0)?;
        // The columns of THREAD start one further along.
        thread_row_at(r, 1, viewer, read)
    })?;
    Ok(rows.filter_map(Result::ok).collect())
}

fn thread_row_at(r: &Row, base: usize, viewer: i64, read: Option<i64>) -> rusqlite::Result<Value> {
    let author: i64 = r.get(base + 3)?;
    let last_post: i64 = r.get(base + 14)?;
    let why: Option<String> = r.get(base + 13)?;
    Ok(json!({
        "id": r.get::<_, i64>(base)?,
        "title": r.get::<_, String>(base + 1)?,
        "category": r.get::<_, String>(base + 2)?,
        "author": person(author, r.get(base + 4)?),
        "created": r.get::<_, i64>(base + 5)?,
        "last": { "at": r.get::<_, i64>(base + 6)?, "by": r.get::<_, Option<i64>>(base + 7)?.map(|u| person(u, r.get::<_, Option<String>>(base + 8).ok().flatten().unwrap_or_default())) },
        "replies": r.get::<_, i64>(base + 9)?,
        "pinned": r.get::<_, i64>(base + 10)? == 1,
        "locked": r.get::<_, i64>(base + 11)? == 1,
        "state": r.get::<_, String>(base + 12)?,
        "hidden_why": if viewer == author || crate::people::can(viewer, crate::people::FORUM_MODERATE) { why } else { None },
        "unread": read.map_or(true, |p| last_post > p),
        "status": r.get::<_, String>(base + 15)?,
        "votes": r.get::<_, i64>(base + 16)?,
        "fixed_in": r.get::<_, Option<i64>>(base + 17)?,
        "solution": r.get::<_, Option<i64>>(base + 18)?,
    }))
}

/// A thread by its number, if the viewer may read it: (category, author, state, locked).
fn thread_of(c: &Connection, id: i64, viewer: i64) -> Option<(String, i64, String, bool)> {
    let (cat, author, state, locked): (String, i64, String, i64) =
        c.query_row("SELECT category, author, state, locked FROM threads WHERE id = ?1", params![id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).optional().ok()??;
    let may = state == "open" || (state == "hidden" && (viewer == author || crate::people::can(viewer, crate::people::FORUM_MODERATE)));
    may.then_some((cat, author, state, locked == 1))
}

/// A post's readers told: the thread's author (a reply), and whoever the post mentions by @login.
/// A new post told: to everyone following the thread (its author and those who replied, unless they stopped), and to
/// those it mentions.
fn told(c: &Connection, by: i64, thread: i64, post: i64, page: i64, body: &str) {
    let Some(title) = c.query_row("SELECT title FROM threads WHERE id = ?1", params![thread], |r| r.get::<_, String>(0)).optional().ok().flatten() else { return };
    let data = json!({ "thread": thread, "post": post, "page": page, "title": title, "excerpt": crate::notify::excerpt(body) });
    let followers = crate::forum2::followers(c, thread);
    for &uid in &followers {
        if uid != by && thread_of(c, thread, uid).is_some() {
            crate::notify::add_or_refresh(c, uid, "reply", Some(by), &format!("thread:{thread}"), data.clone());
        }
    }
    for login in crate::notify::mentions(body) {
        if let Some(uid) = crate::people::uid_of(c, &login) {
            if !followers.contains(&uid) && thread_of(c, thread, uid).is_some() {
                crate::notify::add(c, uid, "mention", Some(by), &format!("post:{post}"), data.clone());
            }
        }
    }
}

/// A post or a thread hidden by a moderator: its author told why.
fn hidden_told(c: &Connection, kind: &str, id: i64, why: &str) {
    let found: Option<(i64, i64, String)> = if kind == "post" {
        c.query_row("SELECT p.author, p.thread, t.title FROM posts p JOIN threads t ON t.id = p.thread WHERE p.id = ?1", params![id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).optional().ok().flatten()
    } else {
        c.query_row("SELECT author, id, title FROM threads WHERE id = ?1", params![id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).optional().ok().flatten()
    };
    if let Some((author, thread, title)) = found {
        crate::notify::add(c, author, "moderation", None, &format!("{kind}:{id}"), json!({ "what": kind, "thread": thread, "title": title, "why": why }));
    }
}

/// Recounts a thread after its posts changed: replies, the last post, when and by whom.
fn recount(c: &Connection, thread: i64) -> rusqlite::Result<()> {
    c.execute(
        "UPDATE threads SET
            replies = (SELECT COUNT(*) FROM posts WHERE thread = ?1 AND state = 'shown') - 1,
            last_post = COALESCE((SELECT MAX(id) FROM posts WHERE thread = ?1 AND state = 'shown'), 0),
            last_at = COALESCE((SELECT created FROM posts WHERE id = (SELECT MAX(id) FROM posts WHERE thread = ?1 AND state = 'shown')), created),
            last_by = (SELECT author FROM posts WHERE id = (SELECT MAX(id) FROM posts WHERE thread = ?1 AND state = 'shown'))
         WHERE id = ?1",
        params![thread],
    )?;
    Ok(())
}

// ------------------------------------------------------------------------------------------------ reading

/// The forum's front: categories with their numbers and last thread, the latest threads, who is online.
async fn index(State(app): State<Shared>, h: HeaderMap) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        let viewer = w.uid;
        let online: Vec<Value> = {
            let now = now();
            let mut o = app.pace.online.lock().unwrap_or_else(|e| e.into_inner());
            o.retain(|_, (_, at)| now - *at < ONLINE_FOR);
            let mut list: Vec<(i64, String)> = o.iter().map(|(u, (l, _))| (*u, l.clone())).collect();
            list.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));
            list.into_iter().map(|(u, l)| person(u, l)).collect()
        };
        let found = db(&app, move |c| -> rusqlite::Result<Value> {
            let mut q = c.prepare(
                "SELECT c.id, c.name, c.about, c.colour, c.closed,
                    (SELECT COUNT(*) FROM threads t WHERE t.category = c.id AND t.state = 'open'),
                    (SELECT COALESCE(SUM(t.replies + 1), 0) FROM threads t WHERE t.category = c.id AND t.state = 'open'),
                    (SELECT MAX(t.last_at) FROM threads t WHERE t.category = c.id AND t.state = 'open')
                 FROM forum_categories c ORDER BY c.place",
            )?;
            let cats: Vec<Value> = q
                .query_map([], |r| {
                    Ok(json!({ "id": r.get::<_, String>(0)?, "name": r.get::<_, String>(1)?, "about": r.get::<_, String>(2)?, "colour": r.get::<_, String>(3)?,
                        "closed": r.get::<_, i64>(4)? == 1, "threads": r.get::<_, i64>(5)?, "posts": r.get::<_, i64>(6)?, "last_at": r.get::<_, Option<i64>>(7)? }))
                })?
                .filter_map(Result::ok)
                .collect();
            let latest = list_threads(c, viewer, "1", &[], "t.last_at DESC", 12, 0)?;
            let unread: i64 = c.query_row(
                "SELECT COUNT(*) FROM threads t LEFT JOIN reads r ON r.thread = t.id AND r.uid = ?1 WHERE t.state = 'open' AND t.last_post > COALESCE(r.post, 0)",
                params![viewer],
                |r| r.get(0),
            )?;
            Ok(json!({ "categories": cats, "latest": latest, "unread": unread }))
        })
        .await
        .map_err(|_| busy())?;
        let mut v = found;
        v["online"] = json!(online);
        Ok(Json(v).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct ThreadFilter {
    cat: Option<String>,
    q: Option<String>,
    page: Option<i64>,
    /// A status of bugs or ideas ("solved" / "open" for questions).
    status: Option<String>,
    /// "top": by votes; "new": by when started; otherwise by the last post.
    sort: Option<String>,
}

/// Threads of a category (pinned first, then by their last post), or matching words across the forum.
async fn threads(State(app): State<Shared>, h: HeaderMap, Query(f): Query<ThreadFilter>) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        let viewer = w.uid;
        let page = f.page.unwrap_or(1).clamp(1, 10_000);
        let cat = f.cat.filter(|c| CATEGORIES.iter().any(|x| x.0 == c));
        let words = f.q.map(|q| clean_line(&q, 60).to_lowercase()).filter(|q| !q.is_empty());
        let status = f.status.filter(|s| crate::forum2::BUG_STATES.contains(&s.as_str()) || crate::forum2::IDEA_STATES.contains(&s.as_str()) || s == "solved" || s == "unsolved");
        let sort = f.sort.unwrap_or_default();
        let found = db(&app, move |c| -> rusqlite::Result<(Vec<Value>, i64)> {
            let mut filter = "1".to_string();
            let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
            if let Some(k) = &cat {
                args.push(Box::new(k.clone()));
                filter += &format!(" AND t.category = ?{}", args.len());
            }
            if let Some(q) = &words {
                for word in q.split(' ').take(6) {
                    let escaped = word.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
                    args.push(Box::new(format!("%{escaped}%")));
                    filter += &format!(" AND t.search LIKE ?{} ESCAPE '\\'", args.len());
                }
            }
            match status.as_deref() {
                Some("unsolved") => filter += " AND t.status != 'solved'",
                Some(s) => {
                    args.push(Box::new(s.to_string()));
                    filter += &format!(" AND t.status = ?{}", args.len());
                }
                None => {}
            }
            let refs: Vec<&dyn rusqlite::ToSql> = args.iter().map(|b| b.as_ref()).collect();
            let seen = if crate::people::can(viewer, crate::people::FORUM_MODERATE) { "t.state != 'removed'".to_string() } else { format!("(t.state = 'open' OR (t.state = 'hidden' AND t.author = {viewer}))") };
            let total: i64 = c.query_row(&format!("SELECT COUNT(*) FROM threads t WHERE {seen} AND {filter}"), refs.as_slice(), |r| r.get(0))?;
            let pins = cat.is_some() && words.is_none() && status.is_none();
            let order = match (sort.as_str(), pins) {
                ("top", true) => "t.pinned DESC, t.votes DESC, t.last_at DESC",
                ("top", false) => "t.votes DESC, t.last_at DESC",
                ("new", true) => "t.pinned DESC, t.created DESC",
                ("new", false) => "t.created DESC",
                (_, true) => "t.pinned DESC, t.last_at DESC",
                _ => "t.last_at DESC",
            };
            let items = list_threads(c, viewer, &filter, refs.as_slice(), order, THREADS_PAGE, (page - 1) * THREADS_PAGE)?;
            Ok((items, total))
        })
        .await
        .map_err(|_| busy())?;
        Ok(Json(json!({ "items": found.0, "total": found.1, "page": page, "per_page": THREADS_PAGE })).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct PageOf {
    page: Option<i64>,
}

/// A thread and a page of its posts. Reading it marks it read up to the last post shown.
async fn thread(State(app): State<Shared>, h: HeaderMap, Key(id): Key<i64>, Query(p): Query<PageOf>) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        let viewer = w.uid;
        let found = db(&app, move |c| -> Option<Value> {
            thread_of(c, id, viewer)?;
            let head = c.query_row(&format!("{THREAD} WHERE t.id = ?1"), params![id], |r| thread_row(r, viewer, None)).ok()?;
            let total: i64 = c.query_row("SELECT COUNT(*) FROM posts WHERE thread = ?1", params![id], |r| r.get(0)).ok()?;
            let pages = ((total + PER_PAGE - 1) / PER_PAGE).max(1);
            // No page asked: the first one with something unread (else the last).
            let page = match p.page {
                Some(n) => n.clamp(1, pages),
                None => {
                    let read: i64 = c.query_row("SELECT post FROM reads WHERE uid = ?1 AND thread = ?2", params![viewer, id], |r| r.get(0)).optional().ok().flatten().unwrap_or(0);
                    let before: i64 = c.query_row("SELECT COUNT(*) FROM posts WHERE thread = ?1 AND id <= ?2", params![id, read], |r| r.get(0)).ok()?;
                    if read == 0 { 1 } else { (before / PER_PAGE + 1).min(pages) }
                }
            };
            let read_before: i64 = c.query_row("SELECT post FROM reads WHERE uid = ?1 AND thread = ?2", params![viewer, id], |r| r.get(0)).optional().ok().flatten().unwrap_or(0);
            let mut q = c
                .prepare("SELECT p.id, p.author, u.login, p.body, p.created, p.edited, p.state, p.hidden_why FROM posts p JOIN users u ON u.uid = p.author WHERE p.thread = ?1 ORDER BY p.id LIMIT ?2 OFFSET ?3")
                .ok()?;
            let posts: Vec<Value> = q
                .query_map(params![id, PER_PAGE, (page - 1) * PER_PAGE], |r| {
                    let (pid, author, state): (i64, i64, String) = (r.get(0)?, r.get(1)?, r.get(6)?);
                    let mine = author == viewer;
                    let visible = state == "shown" || (state == "hidden" && (mine || crate::people::can(viewer, crate::people::FORUM_MODERATE)));
                    let body: String = if visible { r.get(3)? } else { String::new() };
                    let extra = crate::forum2::post_extra(c, pid, &body);
                    Ok(json!({
                        "id": pid,
                        "author": person(author, r.get(2)?),
                        "body": body,
                        "created": r.get::<_, i64>(4)?,
                        "edited": r.get::<_, Option<i64>>(5)?,
                        "state": state,
                        "hidden_why": if mine || crate::people::can(viewer, crate::people::FORUM_MODERATE) { r.get::<_, Option<String>>(7)? } else { None },
                        "mine": mine,
                        "new": read_before > 0 && pid > read_before, // since the last visit (a first visit marks nothing)
                        "reactions": extra["reactions"],
                        "pictures": extra["pictures"],
                        "cards": extra["cards"],
                        "edits": extra["edits"],
                    }))
                })
                .ok()?
                .filter_map(Result::ok)
                .collect();
            let last_shown = posts.last().and_then(|p| p["id"].as_i64()).unwrap_or(0);
            let _ = c.execute(
                "INSERT INTO reads (uid, thread, post) VALUES (?1, ?2, ?3) ON CONFLICT(uid, thread) DO UPDATE SET post = MAX(post, ?3)",
                params![viewer, id, last_shown],
            );
            let mut v = head;
            v["posts"] = json!(posts);
            v["page"] = json!(page);
            v["pages"] = json!(pages);
            v["moderator"] = json!(crate::people::can(viewer, crate::people::FORUM_MODERATE));
            let extra = crate::forum2::thread_extra(c, id, viewer);
            v["voted"] = extra["voted"].clone();
            v["following"] = extra["following"].clone();
            v["fix_pending"] = extra["fix_pending"].clone();
            Some(v)
        })
        .await;
        found.map(|v| Json(v).into_response()).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))
    })
    .await
}

// ------------------------------------------------------------------------------------------------ writing

#[derive(Deserialize)]
struct NewThread {
    category: String,
    title: String,
    body: String,
}

async fn start(State(app): State<Shared>, h: HeaderMap, Json(n): Json<NewThread>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let Some(cat) = CATEGORIES.iter().find(|c| c.0 == n.category) else {
            return Err(said(StatusCode::BAD_REQUEST, "category", "no such category"));
        };
        if cat.4 && !crate::people::can(w.uid, crate::people::ANNOUNCE) {
            return Err(said(StatusCode::FORBIDDEN, "category", "only Cursed posts announcements"));
        }
        let title = clean_line(&n.title, 120);
        if title.chars().count() < 4 {
            return Err(said(StatusCode::BAD_REQUEST, "title", "a title of 4 to 120 characters"));
        }
        let Some(body) = body_ok(&n.body) else {
            return Err(said(StatusCode::BAD_REQUEST, "body", "a post of 1 to 10 000 characters"));
        };
        not_muted(&app, w.uid).await?;
        pictures_ok(&body)?;
        if !allowed(&app.pace.threads, w.uid, Duration::from_secs(30), 6, Duration::from_secs(3600)) {
            return Err(said(StatusCode::TOO_MANY_REQUESTS, "slow", "a new thread every 30 seconds, and 6 an hour at most"));
        }
        let (uid, cat_id) = (w.uid, cat.0.to_string());
        let status = match cat.0 {
            "bugs" => "new",
            "ideas" => "open",
            _ => "",
        };
        crate::forum2::cards_for(&app, &body);
        let id = db(&app, move |c| -> rusqlite::Result<i64> {
            let at = now();
            let tx = c.unchecked_transaction()?;
            tx.execute(
                "INSERT INTO threads (category, author, title, search, created, last_at, last_by, status) VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?2, ?6)",
                params![cat_id, uid, title, title.to_lowercase(), at, status],
            )?;
            let thread = tx.last_insert_rowid();
            tx.execute("INSERT INTO posts (thread, author, body, created) VALUES (?1, ?2, ?3, ?4)", params![thread, uid, body, at])?;
            let post = tx.last_insert_rowid();
            tx.execute("UPDATE threads SET last_post = ?2 WHERE id = ?1", params![thread, post])?;
            tx.execute("INSERT INTO reads (uid, thread, post) VALUES (?1, ?2, ?3)", params![uid, thread, post])?;
            tx.commit()?;
            crate::forum2::follow(c, thread, uid);
            crate::people::bump(c, uid, "posts", 1);
            told(c, uid, thread, post, 1, &body);
            Ok(thread)
        })
        .await
        .map_err(|_| busy())?;
        Ok((StatusCode::CREATED, Json(json!({ "id": id }))).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct Body {
    body: String,
}

async fn reply(State(app): State<Shared>, h: HeaderMap, Key(id): Key<i64>, Json(b): Json<Body>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let Some(body) = body_ok(&b.body) else {
            return Err(said(StatusCode::BAD_REQUEST, "body", "a post of 1 to 10 000 characters"));
        };
        let uid = w.uid;
        not_muted(&app, uid).await?;
        pictures_ok(&body)?;
        let open = db(&app, move |c| thread_of(c, id, uid)).await;
        let Some((_, _, state, locked)) = open else {
            return Err(fail(StatusCode::NOT_FOUND, "missing"));
        };
        if (locked || state != "open") && !crate::people::can(uid, crate::people::FORUM_MODERATE) {
            return Err(said(StatusCode::FORBIDDEN, "locked", "this thread is closed"));
        }
        if !allowed(&app.pace.posts, uid, Duration::from_secs(8), 40, Duration::from_secs(3600)) {
            return Err(said(StatusCode::TOO_MANY_REQUESTS, "slow", "a post every 8 seconds, and 40 an hour at most"));
        }
        crate::forum2::cards_for(&app, &body);
        let post = db(&app, move |c| -> rusqlite::Result<(i64, i64)> {
            let at = now();
            c.execute("INSERT INTO posts (thread, author, body, created) VALUES (?1, ?2, ?3, ?4)", params![id, uid, body, at])?;
            let post = c.last_insert_rowid();
            recount(c, id)?;
            c.execute("INSERT INTO reads (uid, thread, post) VALUES (?1, ?2, ?3) ON CONFLICT(uid, thread) DO UPDATE SET post = MAX(post, ?3)", params![uid, id, post])?;
            let n: i64 = c.query_row("SELECT COUNT(*) FROM posts WHERE thread = ?1 AND id <= ?2", params![id, post], |r| r.get(0))?;
            let page = (n + PER_PAGE - 1) / PER_PAGE;
            crate::people::bump(c, uid, "posts", 1);
            crate::forum2::follow(c, id, uid);
            told(c, uid, id, post, page, &body);
            Ok((post, page))
        })
        .await
        .map_err(|_| busy())?;
        Ok((StatusCode::CREATED, Json(json!({ "id": post.0, "page": post.1 }))).into_response())
    })
    .await
}

async fn edit_post(State(app): State<Shared>, h: HeaderMap, Key(id): Key<i64>, Json(b): Json<Body>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let Some(body) = body_ok(&b.body) else {
            return Err(said(StatusCode::BAD_REQUEST, "body", "a post of 1 to 10 000 characters"));
        };
        let uid = w.uid;
        not_muted(&app, uid).await?;
        pictures_ok(&body)?;
        crate::forum2::cards_for(&app, &body);
        let done = db(&app, move |c| {
            let same = c.query_row("SELECT body = ?2 FROM posts WHERE id = ?1 AND author = ?3 AND state = 'shown'", params![id, body, uid], |r| r.get::<_, bool>(0)).optional().ok().flatten();
            match same {
                None => 0,
                Some(true) => 1,
                Some(false) => {
                    crate::forum2::keep_edit(c, id);
                    c.execute("UPDATE posts SET body = ?3, edited = ?4 WHERE id = ?1 AND author = ?2 AND state = 'shown'", params![id, uid, body, now()]).unwrap_or(0)
                }
            }
        })
        .await;
        if done == 0 {
            return Err(fail(StatusCode::NOT_FOUND, "missing"));
        }
        Ok(Json(json!({})).into_response())
    })
    .await
}

/// A post taken back by its author (or her): its place stays, its words go. The opening post of a thread with replies
/// stays; a thread with nothing but its opening post goes with it.
async fn delete_post(State(app): State<Shared>, h: HeaderMap, Key(id): Key<i64>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        let done = db(&app, move |c| -> Result<(), &'static str> {
            let (thread, author): (i64, i64) = c.query_row("SELECT thread, author FROM posts WHERE id = ?1", params![id], |r| Ok((r.get(0)?, r.get(1)?))).map_err(|_| "missing")?;
            if author != uid && !crate::people::can(uid, crate::people::FORUM_MODERATE) {
                return Err("missing");
            }
            let first: i64 = c.query_row("SELECT MIN(id) FROM posts WHERE thread = ?1", params![thread], |r| r.get(0)).map_err(|_| "busy")?;
            let replies: i64 = c.query_row("SELECT COUNT(*) FROM posts WHERE thread = ?1 AND state = 'shown' AND id != ?2", params![thread, first], |r| r.get(0)).map_err(|_| "busy")?;
            if id == first {
                if replies > 0 && !crate::people::can(uid, crate::people::FORUM_MODERATE) {
                    return Err("first");
                }
                c.execute("UPDATE threads SET state = 'removed' WHERE id = ?1", params![thread]).map_err(|_| "busy")?;
                return Ok(());
            }
            c.execute("UPDATE posts SET state = 'deleted', body = '' WHERE id = ?1", params![id]).map_err(|_| "busy")?;
            recount(c, thread).map_err(|_| "busy")?;
            Ok(())
        })
        .await;
        match done {
            Ok(()) => Ok(Json(json!({})).into_response()),
            Err("first") => Err(said(StatusCode::CONFLICT, "first", "the opening post of a thread with replies stays: edit it instead")),
            Err("missing") => Err(fail(StatusCode::NOT_FOUND, "missing")),
            Err(_) => Err(busy()),
        }
    })
    .await
}

#[derive(Deserialize)]
struct Retitle {
    title: Option<String>,
    category: Option<String>,
    pinned: Option<bool>,
    locked: Option<bool>,
}

/// Its author renames a thread; she also pins, locks and moves it.
async fn retitle(State(app): State<Shared>, h: HeaderMap, Key(id): Key<i64>, Json(r): Json<Retitle>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        let moderator = crate::people::can(uid, crate::people::FORUM_MODERATE);
        if (r.category.is_some() || r.pinned.is_some() || r.locked.is_some()) && !moderator {
            return Err(fail(StatusCode::FORBIDDEN, "moderation"));
        }
        if let Some(cat) = &r.category {
            if !CATEGORIES.iter().any(|c| c.0 == cat) {
                return Err(said(StatusCode::BAD_REQUEST, "category", "no such category"));
            }
        }
        let title = match &r.title {
            Some(t) => {
                let t = clean_line(t, 120);
                if t.chars().count() < 4 {
                    return Err(said(StatusCode::BAD_REQUEST, "title", "a title of 4 to 120 characters"));
                }
                Some(t)
            }
            None => None,
        };
        let done = db(&app, move |c| -> Option<()> {
            let (_, author, _, _) = thread_of(c, id, uid)?;
            if author != uid && !moderator {
                return None;
            }
            if let Some(t) = &title {
                c.execute("UPDATE threads SET title = ?2, search = ?3 WHERE id = ?1", params![id, t, t.to_lowercase()]).ok()?;
            }
            if let Some(cat) = &r.category {
                let status = match cat.as_str() {
                    "bugs" => "new",
                    "ideas" => "open",
                    _ => "",
                };
                c.execute("UPDATE threads SET category = ?2, status = ?3, fix_pending = 0, solution = NULL WHERE id = ?1 AND category != ?2", params![id, cat, status]).ok()?;
                crate::people::audit(c, uid, "forum.move", &id.to_string(), json!({ "to": cat }));
            }
            if let Some(p) = r.pinned {
                c.execute("UPDATE threads SET pinned = ?2 WHERE id = ?1", params![id, p as i64]).ok()?;
            }
            if let Some(l) = r.locked {
                c.execute("UPDATE threads SET locked = ?2 WHERE id = ?1", params![id, l as i64]).ok()?;
            }
            Some(())
        })
        .await;
        done.map(|_| Json(json!({})).into_response()).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))
    })
    .await
}

// ------------------------------------------------------------------------------------------------ moderation

#[derive(Deserialize)]
struct Why {
    #[serde(default)]
    why: String,
}

async fn moderate(app: Shared, h: HeaderMap, sql: &'static str, id: i64, why: Option<String>, thread_of_post: bool) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        if !crate::people::can(w.uid, crate::people::FORUM_MODERATE) {
            return Err(fail(StatusCode::FORBIDDEN, "moderation"));
        }
        let by = w.uid;
        let done = db(&app, move |c| -> rusqlite::Result<usize> {
            let n = c.execute(sql, params![id, why.clone()])?;
            if n > 0 {
                let action = match (thread_of_post, why.is_some()) {
                    (true, true) => "forum.hide_post",
                    (true, false) => "forum.unhide_post",
                    (false, true) => "forum.hide_thread",
                    (false, false) => "forum.unhide_thread",
                };
                crate::people::audit(c, by, action, &id.to_string(), json!({ "why": why }));
                if let Some(why) = &why {
                    hidden_told(c, if thread_of_post { "post" } else { "thread" }, id, why);
                }
            }
            if thread_of_post {
                if let Some(t) = c.query_row("SELECT thread FROM posts WHERE id = ?1", params![id], |r| r.get::<_, i64>(0)).optional()? {
                    recount(c, t)?;
                }
            }
            Ok(n)
        })
        .await
        .unwrap_or(0);
        if done == 0 {
            return Err(fail(StatusCode::NOT_FOUND, "missing"));
        }
        Ok(Json(json!({})).into_response())
    })
    .await
}

async fn hide_thread(State(app): State<Shared>, h: HeaderMap, Key(id): Key<i64>, Json(y): Json<Why>) -> Response {
    moderate(app, h, "UPDATE threads SET state = 'hidden', hidden_why = ?2 WHERE id = ?1 AND state = 'open'", id, Some(clean_line(&y.why, 200)), false).await
}
async fn unhide_thread(State(app): State<Shared>, h: HeaderMap, Key(id): Key<i64>) -> Response {
    moderate(app, h, "UPDATE threads SET state = 'open', hidden_why = ?2 WHERE id = ?1 AND state = 'hidden'", id, None, false).await
}
async fn hide_post(State(app): State<Shared>, h: HeaderMap, Key(id): Key<i64>, Json(y): Json<Why>) -> Response {
    moderate(app, h, "UPDATE posts SET state = 'hidden', hidden_why = ?2 WHERE id = ?1 AND state = 'shown'", id, Some(clean_line(&y.why, 200)), true).await
}
async fn unhide_post(State(app): State<Shared>, h: HeaderMap, Key(id): Key<i64>) -> Response {
    moderate(app, h, "UPDATE posts SET state = 'shown', hidden_why = ?2 WHERE id = ?1 AND state = 'hidden'", id, None, true).await
}

// ------------------------------------------------------------------------------------------------ reports

#[derive(Deserialize)]
struct Report {
    kind: String,
    id: i64,
    #[serde(default)]
    why: String,
}

/// A member reports a post, a thread or a pack to her. Ten a day; the same thing once.
async fn report(State(app): State<Shared>, h: HeaderMap, Json(r): Json<Report>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        if !matches!(r.kind.as_str(), "post" | "thread" | "pack") {
            return Err(said(StatusCode::BAD_REQUEST, "kind", "a post, a thread or a pack"));
        }
        let why = clean_line(&r.why, 300);
        if why.chars().count() < 3 {
            return Err(said(StatusCode::BAD_REQUEST, "why", "say what is wrong with it"));
        }
        // What it is about must exist and not be reported by them already; only then does it count against their day.
        let (uid, kind, id) = (w.uid, r.kind.clone(), r.id);
        let checked = db(&app, move |c| -> Result<(), &'static str> {
            let exists = match kind.as_str() {
                "post" => c.query_row("SELECT 1 FROM posts WHERE id = ?1 AND state = 'shown'", params![id], |_| Ok(())).optional(),
                "thread" => c.query_row("SELECT 1 FROM threads WHERE id = ?1 AND state = 'open'", params![id], |_| Ok(())).optional(),
                _ => c.query_row("SELECT 1 FROM packs WHERE id = ?1 AND state = 'published'", params![id], |_| Ok(())).optional(),
            }
            .map_err(|_| "busy")?;
            if exists.is_none() {
                return Err("missing");
            }
            let again = c.query_row("SELECT 1 FROM reports WHERE kind = ?1 AND target = ?2 AND by = ?3 AND closed IS NULL", params![kind, id, uid], |_| Ok(())).optional().map_err(|_| "busy")?;
            if again.is_some() {
                return Err("again");
            }
            Ok(())
        })
        .await;
        let done = match checked {
            Ok(()) if !allowed(&app.pace.reports, w.uid, Duration::from_secs(5), 10, Duration::from_secs(86_400)) => {
                return Err(said(StatusCode::TOO_MANY_REQUESTS, "slow", "ten reports a day at most"));
            }
            Ok(()) => {
                let kind = r.kind.clone();
                db(&app, move |c| c.execute("INSERT INTO reports (kind, target, by, why, created) VALUES (?1, ?2, ?3, ?4, ?5)", params![kind, id, uid, why, now()]).map(|_| ()).map_err(|_| "busy")).await
            }
            Err(e) => Err(e),
        };
        match done {
            Ok(()) => Ok((StatusCode::CREATED, Json(json!({}))).into_response()),
            Err("missing") => Err(fail(StatusCode::NOT_FOUND, "missing")),
            Err("again") => Err(said(StatusCode::CONFLICT, "again", "you have reported this already")),
            Err(_) => Err(busy()),
        }
    })
    .await
}

/// Her list of open reports, each with what it is about.
async fn reports(State(app): State<Shared>, h: HeaderMap) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        if !crate::people::can(w.uid, crate::people::REPORTS) {
            return Err(fail(StatusCode::FORBIDDEN, "moderation"));
        }
        let items = db(&app, |c| -> rusqlite::Result<Vec<Value>> {
            let mut q = c.prepare("SELECT r.id, r.kind, r.target, r.why, r.created, u.uid, u.login FROM reports r JOIN users u ON u.uid = r.by WHERE r.closed IS NULL ORDER BY r.id DESC LIMIT 200")?;
            let rows: Vec<(i64, String, i64, String, i64, i64, String)> =
                q.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)))?.filter_map(Result::ok).collect();
            let mut out = Vec::new();
            for (id, kind, target, why, created, by, login) in rows {
                let about: Value = match kind.as_str() {
                    "post" => c
                        .query_row(
                            "SELECT p.body, p.thread, t.title, a.uid, a.login, p.state FROM posts p JOIN threads t ON t.id = p.thread JOIN users a ON a.uid = p.author WHERE p.id = ?1",
                            params![target],
                            |r| Ok(json!({ "text": r.get::<_, String>(0)?.chars().take(400).collect::<String>(), "thread": r.get::<_, i64>(1)?, "title": r.get::<_, String>(2)?, "author": person(r.get(3)?, r.get(4)?), "state": r.get::<_, String>(5)? })),
                        )
                        .optional()?
                        .unwrap_or(Value::Null),
                    "thread" => c
                        .query_row("SELECT t.title, a.uid, a.login, t.state FROM threads t JOIN users a ON a.uid = t.author WHERE t.id = ?1", params![target], |r| {
                            Ok(json!({ "title": r.get::<_, String>(0)?, "thread": target, "author": person(r.get(1)?, r.get(2)?), "state": r.get::<_, String>(3)? }))
                        })
                        .optional()?
                        .unwrap_or(Value::Null),
                    _ => c
                        .query_row("SELECT p.title, p.slug, a.uid, a.login, p.state FROM packs p JOIN users a ON a.uid = p.owner WHERE p.id = ?1", params![target], |r| {
                            Ok(json!({ "title": r.get::<_, String>(0)?, "slug": r.get::<_, String>(1)?, "author": person(r.get(2)?, r.get(3)?), "state": r.get::<_, String>(4)? }))
                        })
                        .optional()?
                        .unwrap_or(Value::Null),
                };
                out.push(json!({ "id": id, "kind": kind, "target": target, "why": why, "created": created, "by": person(by, login), "about": about }));
            }
            Ok(out)
        })
        .await
        .map_err(|_| busy())?;
        Ok(Json(json!({ "items": items })).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct Close {
    action: String, // dismiss | hide
    #[serde(default)]
    why: String,
}

/// Closing a report: dismissed, or what it is about hidden (with a reason its author sees). Other open reports of the
/// same thing close with it.
async fn close_report(State(app): State<Shared>, h: HeaderMap, Key(id): Key<i64>, Json(x): Json<Close>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        if !crate::people::can(w.uid, crate::people::REPORTS) {
            return Err(fail(StatusCode::FORBIDDEN, "moderation"));
        }
        if x.action != "dismiss" && x.action != "hide" {
            return Err(fail(StatusCode::BAD_REQUEST, "action"));
        }
        let why = clean_line(&x.why, 200);
        let done = db(&app, move |c| -> rusqlite::Result<bool> {
            let Some((kind, target)): Option<(String, i64)> = c.query_row("SELECT kind, target FROM reports WHERE id = ?1 AND closed IS NULL", params![id], |r| Ok((r.get(0)?, r.get(1)?))).optional()? else {
                return Ok(false);
            };
            if x.action == "hide" {
                match kind.as_str() {
                    "post" => {
                        c.execute("UPDATE posts SET state = 'hidden', hidden_why = ?2 WHERE id = ?1 AND state = 'shown'", params![target, why])?;
                        if let Some(t) = c.query_row("SELECT thread FROM posts WHERE id = ?1", params![target], |r| r.get::<_, i64>(0)).optional()? {
                            recount(c, t)?;
                        }
                    }
                    "thread" => {
                        c.execute("UPDATE threads SET state = 'hidden', hidden_why = ?2 WHERE id = ?1 AND state = 'open'", params![target, why])?;
                    }
                    _ => {
                        c.execute("UPDATE packs SET state = 'hidden', hidden_why = ?2 WHERE id = ?1 AND state != 'removed'", params![target, why])?;
                    }
                }
            }
            if x.action == "hide" && kind != "pack" {
                hidden_told(c, &kind, target, &why);
            }
            let reporters: Vec<i64> = c.prepare("SELECT DISTINCT by FROM reports WHERE kind = ?1 AND target = ?2 AND closed IS NULL")?.query_map(params![kind, target], |r| r.get(0))?.flatten().collect();
            c.execute("UPDATE reports SET closed = ?3, outcome = ?4 WHERE kind = ?1 AND target = ?2 AND closed IS NULL", params![kind, target, now(), x.action])?;
            for by in reporters {
                crate::notify::add(c, by, "report", None, &format!("report:{kind}:{target}"), json!({ "kind": kind, "target": target, "outcome": x.action }));
            }
            Ok(true)
        })
        .await
        .map_err(|_| busy())?;
        if !done {
            return Err(fail(StatusCode::NOT_FOUND, "missing"));
        }
        Ok(Json(json!({})).into_response())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bodies() {
        assert_eq!(body_ok("  hi\r\nthere\u{7}  ").as_deref(), Some("hi\nthere"));
        assert!(body_ok("   ").is_none() && body_ok(&"x".repeat(10_001)).is_none());
    }

    #[test]
    fn pacing() {
        let book = Mutex::new(HashMap::new());
        assert!(allowed(&book, 5, Duration::from_millis(0), 2, Duration::from_secs(60)));
        assert!(allowed(&book, 5, Duration::from_millis(0), 2, Duration::from_secs(60)));
        assert!(!allowed(&book, 5, Duration::from_millis(0), 2, Duration::from_secs(60)), "a third within the window");
        assert!(allowed(&book, 6, Duration::from_millis(0), 2, Duration::from_secs(60)), "someone else");
        assert!(allowed(&book, 7, Duration::from_secs(60), 5, Duration::from_secs(60)));
        assert!(!allowed(&book, 7, Duration::from_secs(60), 5, Duration::from_secs(60)), "too soon after the last");
        for _ in 0..5 {
            assert!(allowed(&book, OWNER, Duration::from_secs(60), 1, Duration::from_secs(60)), "hers go unmetered");
        }
    }
}
