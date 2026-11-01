//! The staff's panel (the site's #/admin): what was done from the command line on the VDS, done from the site, each
//! door for those whose roles allow it.
//!
//! - The summary: accounts and registration, who is on the site and in the game now and on which builds, open
//!   tickets and reports, crashes of the day.
//! - Crashes grouped by their cause (a native frame, a Java exception, an exit code), with the builds and video
//!   cards they happened on; each crash's report readable.
//! - Members: found by login, UID or e-mail; one opened whole — roles, badges, the computer, bans, warnings, mutes,
//!   tickets, crashes, the staff's notes about them, what the staff did to them; banned, freed, given a new UID.
//! - Her switches: registration, the partners pinned in every player's server list, the builds and a rollback.
//! - Roles: what each may do, and who holds them.
//! - The journal: who of the staff did what.
//! - Announcements: written once, they go to the site, the forum, every player in the client (a notification), the
//!   three Discord servers (the bot takes them from here, with @everyone if asked) and her Telegram (the bot sends
//!   the post to her, to forward to the channel).

use std::collections::{BTreeSet, HashMap};

use axum::extract::{DefaultBodyLimit, Path as Key, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::market::{busy, clean_line, member, run, said};
use crate::people::{can, AUDIT, ANNOUNCE, STAFF_PANEL, USERS_BAN, USERS_DEVICE, USERS_VIEW};
use crate::{db, fail, now, Shared};

pub fn schema(c: &Connection) {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS user_notes (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             uid INTEGER NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
             by INTEGER NOT NULL,
             text TEXT NOT NULL,
             at INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS announcements (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             title TEXT NOT NULL,
             body TEXT NOT NULL,
             by INTEGER NOT NULL,
             at INTEGER NOT NULL,
             targets TEXT NOT NULL,
             thread INTEGER,
             discord TEXT,
             discord_at INTEGER,
             telegram_at INTEGER,
             error TEXT);",
    )
    .expect("make the panel's tables");
    for (col, def) in [("sig", "TEXT"), ("gpu", "TEXT")] {
        let has: bool = c.prepare("SELECT 1 FROM pragma_table_info('crashes') WHERE name = ?1").and_then(|mut q| q.exists(params![col])).unwrap_or(false);
        if !has {
            c.execute(&format!("ALTER TABLE crashes ADD COLUMN {col} {def}"), []).expect("a crash's new column");
        }
    }
}

fn may(uid: i64, perm: &str) -> Result<(), Response> {
    if can(uid, perm) {
        Ok(())
    } else {
        Err(fail(StatusCode::FORBIDDEN, "staff"))
    }
}

fn hers(uid: i64) -> Result<(), Response> {
    if crate::is_owner(uid) {
        Ok(())
    } else {
        Err(fail(StatusCode::FORBIDDEN, "owner"))
    }
}

// ------------------------------------------------------------------------------------------------ crashes

/// What a crash comes down to, read from its report: the native frame it died in, the Java exception, or only the
/// way it exited; and the video card when the report names it.
pub fn crash_cause(text: &str) -> (String, Option<String>) {
    let lines: Vec<&str> = text.lines().collect();
    let gpu = lines.iter().find_map(|l| {
        let l = l.trim_start_matches(|c: char| c == '\t' || c == ' ' || c == '#');
        l.split_once("Graphics card #0 name: ")
            .map(|(_, g)| g.to_string())
            .or_else(|| l.split_once("Renderer: ").map(|(_, g)| g.to_string()))
            .or_else(|| l.split_once("OpenGL: ").map(|(_, g)| g.split(',').next().unwrap_or("").to_string()))
            .map(|g| g.trim().trim_end_matches("/PCIe/SSE2").chars().take(80).collect::<String>())
            .filter(|g| g.len() > 3 && !g.starts_with("1282") && !g.contains("ERROR"))
    });
    // A native crash: the module of its problematic frame.
    if let Some(i) = lines.iter().position(|l| l.contains("Problematic frame:")) {
        if let Some(frame) = lines.get(i + 1) {
            let f = frame.trim_start_matches('#').trim();
            let module = f.split('[').nth(1).and_then(|r| r.split(['+', ']']).next()).unwrap_or(f);
            let kind = lines.iter().find_map(|l| l.split_once("#  ").and_then(|(_, r)| r.split_whitespace().next().filter(|w| w.starts_with("EXCEPTION_")))).unwrap_or("native crash");
            return (format!("{kind} in {}", module.trim()), gpu);
        }
    }
    // A crash report of the game: its description and the exception under it.
    if let Some(i) = lines.iter().position(|l| l.starts_with("Description: ")) {
        let description = lines[i].trim_start_matches("Description: ").trim();
        let exception = lines[i + 1..].iter().map(|l| l.trim()).find(|l| !l.is_empty()).unwrap_or("");
        let exception: String = exception.chars().take(140).collect();
        return (format!("{description} · {exception}"), gpu);
    }
    // A panic of the native side.
    if let Some(l) = lines.iter().find(|l| l.contains("panicked at")) {
        return (format!("native panic {}", l.split("panicked at").nth(1).unwrap_or("").trim().chars().take(120).collect::<String>()), gpu);
    }
    let exit = lines.iter().find_map(|l| l.strip_prefix("exit: ")).unwrap_or("?").trim();
    (format!("exit {exit} (no report)"), gpu)
}

/// Crashes not yet read for their cause: read now (at most a few hundred at a time).
fn crash_causes(c: &Connection, dir: &std::path::Path) {
    let rows: Vec<(i64, String)> = c.prepare("SELECT id, file FROM crashes WHERE sig IS NULL ORDER BY id DESC LIMIT 400").and_then(|mut q| q.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).map(|m| m.flatten().collect())).unwrap_or_default();
    for (id, file) in rows {
        let text = std::fs::read(dir.join(&file)).map(|b| String::from_utf8_lossy(&b).to_string()).unwrap_or_default();
        let (sig, gpu) = crash_cause(&text);
        let _ = c.execute("UPDATE crashes SET sig = ?2, gpu = ?3 WHERE id = ?1", params![id, sig, gpu]);
    }
}

#[derive(Deserialize)]
struct Days {
    days: Option<i64>,
}

async fn crashes(State(app): State<Shared>, h: HeaderMap, Query(q): Query<Days>) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        may(w.uid, USERS_VIEW)?;
        let since = now() - q.days.unwrap_or(7).clamp(1, 365) * 86_400;
        let dir = crate::beta::crashes_dir(&app.db_path);
        let v = db(&app, move |c| -> rusqlite::Result<Value> {
            crash_causes(c, &dir);
            let rows: Vec<(i64, i64, i64, Option<i64>, String, Option<String>)> = c
                .prepare("SELECT id, uid, at, build, COALESCE(sig, '?'), gpu FROM crashes WHERE at > ?1 ORDER BY id DESC")?
                .query_map(params![since], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))?
                .flatten()
                .collect();
            let mut groups: HashMap<String, (i64, BTreeSet<i64>, BTreeSet<i64>, BTreeSet<String>, i64, Vec<i64>)> = HashMap::new();
            for (id, uid, at, build, sig, gpu) in &rows {
                let g = groups.entry(sig.clone()).or_insert_with(|| (0, BTreeSet::new(), BTreeSet::new(), BTreeSet::new(), 0, Vec::new()));
                g.0 += 1;
                g.1.insert(*uid);
                if let Some(b) = build {
                    g.2.insert(*b);
                }
                if let Some(gp) = gpu {
                    g.3.insert(gp.clone());
                }
                g.4 = g.4.max(*at);
                if g.5.len() < 20 {
                    g.5.push(*id);
                }
            }
            let mut out: Vec<Value> = groups
                .into_iter()
                .map(|(sig, (count, users, builds, gpus, last, ids))| json!({ "cause": sig, "count": count, "people": users.len(), "builds": builds, "gpus": gpus, "last": last, "crashes": ids }))
                .collect();
            out.sort_by(|a, b| b["count"].as_i64().cmp(&a["count"].as_i64()).then(b["last"].as_i64().cmp(&a["last"].as_i64())));
            Ok(json!({ "groups": out, "total": rows.len() }))
        })
        .await
        .map_err(|_| busy())?;
        Ok(Json(v).into_response())
    })
    .await
}

/// One crash's report as it was sent, with whose it was.
async fn crash(State(app): State<Shared>, h: HeaderMap, Key(id): Key<i64>) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        may(w.uid, USERS_VIEW)?;
        let found = db(&app, move |c| c.query_row("SELECT c.file, u.login, c.at FROM crashes c LEFT JOIN users u ON u.uid = c.uid WHERE c.id = ?1", params![id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, i64>(2)?))).optional())
            .await
            .map_err(|_| busy())?
            .ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
        let path = crate::beta::crashes_dir(&app.db_path).join(&found.0);
        let bytes = tokio::task::spawn_blocking(move || std::fs::read(path)).await.map_err(|_| busy())?.map_err(|_| fail(StatusCode::NOT_FOUND, "missing"))?;
        let name = format!("crash-{}-{}.log", found.1.unwrap_or_default(), found.2);
        Ok(([(header::CONTENT_TYPE, "text/plain; charset=utf-8".to_string()), (header::CONTENT_DISPOSITION, format!("inline; filename=\"{name}\""))], bytes).into_response())
    })
    .await
}

// ------------------------------------------------------------------------------------------------ the summary

async fn summary(State(app): State<Shared>, h: HeaderMap) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        may(w.uid, STAFF_PANEL)?;
        let (site, game, builds) = crate::live::census();
        let current = db(&app, |c| crate::dist::current(c, "beta").map(|x| x.0)).await;
        let v = db(&app, move |c| {
            let n = |sql: &str, p: &[&dyn rusqlite::ToSql]| c.query_row(sql, p, |r| r.get::<_, i64>(0)).unwrap_or(0);
            let at = now();
            json!({
                "accounts": n("SELECT COUNT(*) FROM users", &[]),
                "new_day": n("SELECT COUNT(*) FROM users WHERE created > ?1", &[&(at - 86_400)]),
                "new_week": n("SELECT COUNT(*) FROM users WHERE created > ?1", &[&(at - 7 * 86_400)]),
                "registration": c.query_row("SELECT value FROM settings WHERE name = 'registration'", [], |r| r.get::<_, String>(0)).unwrap_or_default(),
                "limit": crate::beta::limit(c), "taken": crate::beta::taken(c),
                "online": { "site": site, "game": game },
                "builds": builds.iter().map(|(b, n)| json!({ "build": b, "people": n, "current": b.is_some() && *b == current })).collect::<Vec<_>>(),
                "current_build": current,
                "tickets": { "open": n("SELECT COUNT(*) FROM support_tickets WHERE state = 'open'", &[]), "working": n("SELECT COUNT(*) FROM support_tickets WHERE state = 'working'", &[]),
                    "waiting": n("SELECT COUNT(*) FROM support_tickets WHERE state = 'waiting'", &[]), "unassigned": n("SELECT COUNT(*) FROM support_tickets WHERE assignee IS NULL AND state != 'closed'", &[]) },
                "reports": n("SELECT COUNT(*) FROM reports WHERE closed IS NULL", &[]),
                "chat_reports": n("SELECT COUNT(*) FROM chat_reports WHERE state = 'open'", &[]),
                "crashes_day": n("SELECT COUNT(*) FROM crashes WHERE at > ?1", &[&(at - 86_400)]),
                "crashes_week": n("SELECT COUNT(*) FROM crashes WHERE at > ?1", &[&(at - 7 * 86_400)]),
                "bans": n("SELECT COUNT(*) FROM bans WHERE lifted IS NULL AND (until IS NULL OR until > ?1)", &[&at]),
            })
        })
        .await;
        Ok(Json(v).into_response())
    })
    .await
}

// ------------------------------------------------------------------------------------------------ members

#[derive(Deserialize)]
struct Search {
    q: Option<String>,
}

async fn users(State(app): State<Shared>, h: HeaderMap, Query(s): Query<Search>) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        may(w.uid, USERS_VIEW)?;
        let q = s.q.map(|q| clean_line(&q, 60).to_lowercase()).unwrap_or_default();
        let v = db(&app, move |c| -> rusqlite::Result<Value> {
            let uids: Vec<i64> = if q.is_empty() {
                c.prepare("SELECT uid FROM users ORDER BY created DESC LIMIT 50")?.query_map([], |r| r.get(0))?.flatten().collect()
            } else {
                let like = format!("%{}%", q.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"));
                let exact: i64 = q.trim_start_matches('#').parse().unwrap_or(-2);
                let text = q.trim_start_matches('#').to_string();
                c.prepare("SELECT uid FROM users WHERE uid = ?1 OR lower(uid_text) = ?3 OR login_key LIKE ?2 ESCAPE '\\' OR lower(email) LIKE ?2 ESCAPE '\\' ORDER BY (uid = ?1 OR lower(uid_text) = ?3) DESC, length(login), login LIMIT 50")?
                    .query_map(params![exact, like, text], |r| r.get(0))?
                    .flatten()
                    .collect()
            };
            let rows: Vec<Value> = uids
                .into_iter()
                .filter_map(|u| {
                    let mut t = crate::beta::tester_json(c, u, false)?;
                    t["card"] = crate::people::card_of(c, u);
                    t["online"] = json!(crate::live::online(u));
                    Some(t)
                })
                .collect();
            Ok(json!({ "items": rows }))
        })
        .await
        .map_err(|_| busy())?;
        Ok(Json(v).into_response())
    })
    .await
}

/// One member, whole, as the staff see them.
async fn user(State(app): State<Shared>, h: HeaderMap, Key(login): Key<String>) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        may(w.uid, USERS_VIEW)?;
        let viewer = w.uid;
        let v = db(&app, move |c| -> Result<Value, Response> {
            let uid = crate::people::uid_of(c, &login).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            let mut out = crate::beta::tester_json(c, uid, true).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            if !crate::is_owner(viewer) && !can(viewer, crate::people::USERS_BAN) {
                out["email"] = Value::Null;
            }
            out["card"] = crate::people::card_of(c, uid);
            out["roles"] = json!(crate::people::roles_of(uid));
            out["online"] = json!(crate::live::online(uid));
            let list = |sql: &str, f: &dyn Fn(&rusqlite::Row) -> rusqlite::Result<Value>| -> Vec<Value> {
                c.prepare(sql).and_then(|mut q| q.query_map(params![uid], |r| f(r)).map(|m| m.flatten().collect())).unwrap_or_default()
            };
            out["warnings"] = json!(list("SELECT by, why, at FROM warnings WHERE uid = ?1 ORDER BY id DESC LIMIT 30", &|r| Ok(json!({ "by": crate::people::card_of(c, r.get(0)?), "why": r.get::<_, String>(1)?, "at": r.get::<_, i64>(2)? }))));
            out["mute"] = crate::forum2::muted(c, uid).map_or(Value::Null, |(until, why)| json!({ "until": until, "why": why }));
            out["notes"] = json!(list("SELECT id, by, text, at FROM user_notes WHERE uid = ?1 ORDER BY id DESC LIMIT 50", &|r| Ok(json!({ "id": r.get::<_, i64>(0)?, "by": crate::people::card_of(c, r.get(1)?), "text": r.get::<_, String>(2)?, "at": r.get::<_, i64>(3)? }))));
            out["tickets"] = json!(list("SELECT id, title, state, updated FROM support_tickets WHERE uid = ?1 ORDER BY id DESC LIMIT 20", &|r| Ok(json!({ "id": r.get::<_, i64>(0)?, "title": r.get::<_, String>(1)?, "state": r.get::<_, String>(2)?, "updated": r.get::<_, i64>(3)? }))));
            out["crash_list"] = json!(list("SELECT id, at, build, COALESCE(sig, '?') FROM crashes WHERE uid = ?1 ORDER BY id DESC LIMIT 20", &|r| Ok(json!({ "id": r.get::<_, i64>(0)?, "at": r.get::<_, i64>(1)?, "build": r.get::<_, Option<i64>>(2)?, "cause": r.get::<_, String>(3)? }))));
            out["history"] = json!(list("SELECT actor, action, details, at FROM audit WHERE target = CAST(?1 AS TEXT) AND (action LIKE 'user.%' OR action LIKE 'device.%' OR action IN ('forum.warn', 'forum.mute', 'forum.unmute')) ORDER BY id DESC LIMIT 50", &|r| Ok(json!({ "by": crate::people::card_of(c, r.get(0)?), "action": r.get::<_, String>(1)?, "details": serde_json::from_str::<Value>(&r.get::<_, String>(2)?).unwrap_or(Value::Null), "at": r.get::<_, i64>(3)? }))));
            out["sessions"] = json!(c.query_row("SELECT COUNT(*) FROM sessions WHERE uid = ?1", params![uid], |r| r.get::<_, i64>(0)).unwrap_or(0));
            out["posts"] = json!(crate::people::stat(c, uid, "posts"));
            out["game_hours"] = json!(crate::people::stat(c, uid, "game_min") / 60);
            // Recode: hers to see and to give (her word, 19.10.2026), and the role «Ключи»'s, with what they gave (23.10)
            if crate::is_owner(viewer) {
                out["recode"] = crate::recode::card(c, uid, None);
            } else if crate::people::can(viewer, crate::people::RECODE_KEYS) || crate::people::can(viewer, crate::people::RECODE_REVOKE) {
                out["recode"] = crate::recode::card(c, uid, Some(viewer));
            }
            if out.get("recode").is_some() {
                out["recode"]["can_revoke_all"] = json!(crate::people::can(viewer, crate::people::RECODE_REVOKE) && !crate::is_owner(uid));
            }
            Ok(out)
        })
        .await?;
        Ok(Json(v).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct Note {
    text: String,
}

async fn add_note(State(app): State<Shared>, h: HeaderMap, Key(login): Key<String>, Json(n): Json<Note>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        may(w.uid, USERS_VIEW)?;
        let by = w.uid;
        let text: String = n.text.replace("\r\n", "\n").chars().filter(|c| *c == '\n' || !c.is_control()).take(2000).collect::<String>().trim().to_string();
        if text.is_empty() {
            return Err(said(StatusCode::BAD_REQUEST, "text", "a few words"));
        }
        db(&app, move |c| -> Result<(), Response> {
            let uid = crate::people::uid_of(c, &login).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            c.execute("INSERT INTO user_notes (uid, by, text, at) VALUES (?1, ?2, ?3, ?4)", params![uid, by, text, now()]).map_err(|_| busy())?;
            Ok(())
        })
        .await?;
        Ok(Json(json!({})).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct BanIt {
    reason: String,
    /// "7d", "12h", "30m"; none: for good.
    #[serde(default)]
    length: Option<String>,
}

async fn ban(State(app): State<Shared>, h: HeaderMap, Key(login): Key<String>, Json(b): Json<BanIt>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        may(w.uid, USERS_BAN)?;
        let by_uid = w.uid;
        let by = format!("uid:{}", w.uid);
        let reason = clean_line(&b.reason, 200);
        if reason.chars().count() < 3 {
            return Err(said(StatusCode::BAD_REQUEST, "reason", "a reason: the player sees it"));
        }
        let until = match b.length.as_deref().filter(|l| !l.is_empty()) {
            Some(l) => Some(now() + crate::beta::parse_length(l).ok_or_else(|| said(StatusCode::BAD_REQUEST, "length", "a length like 7d, 12h or 30m"))?),
            None => None,
        };
        db(&app, move |c| -> Result<(), Response> {
            let uid = crate::people::uid_of(c, &login).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            if crate::is_owner(uid) || (crate::people::is_staff(uid) && !crate::is_owner(by_uid)) {
                return Err(said(StatusCode::BAD_REQUEST, "staff", "staff are not banned from here"));
            }
            crate::beta::ban(c, uid, &reason, until, &by).map_err(|e| said(StatusCode::BAD_REQUEST, "ban", &e))?;
            crate::people::audit(c, by_uid, "user.ban", &uid.to_string(), json!({ "reason": reason, "until": until }));
            Ok(())
        })
        .await?;
        Ok(Json(json!({})).into_response())
    })
    .await
}

async fn unban(State(app): State<Shared>, h: HeaderMap, Key(login): Key<String>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        may(w.uid, USERS_BAN)?;
        let by_uid = w.uid;
        let n = db(&app, move |c| -> Result<usize, Response> {
            let uid = crate::people::uid_of(c, &login).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            let n = crate::beta::unban(c, uid, &format!("uid:{by_uid}"));
            crate::people::audit(c, by_uid, "user.unban", &uid.to_string(), json!({ "lifted": n }));
            Ok(n)
        })
        .await?;
        Ok(Json(json!({ "lifted": n })).into_response())
    })
    .await
}

async fn free_device(State(app): State<Shared>, h: HeaderMap, Key(login): Key<String>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        may(w.uid, USERS_DEVICE)?;
        let by_uid = w.uid;
        db(&app, move |c| -> Result<(), Response> {
            let uid = crate::people::uid_of(c, &login).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            let _ = crate::beta::free_device(c, uid, false);
            crate::people::audit(c, by_uid, "device.reset", &uid.to_string(), json!({}));
            Ok(())
        })
        .await?;
        Ok(Json(json!({})).into_response())
    })
    .await
}

/// A link to set a new password (hers, 11.10.2026): for a member who wrote to the support without access to their
/// e-mail. A day long, once; the staff give it to them themselves. Another staff member's account only she may.
async fn reset_link(State(app): State<Shared>, h: HeaderMap, Key(login): Key<String>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        may(w.uid, USERS_BAN)?;
        let by_uid = w.uid;
        let origin = app.origin.clone();
        let link = db(&app, move |c| -> Result<String, Response> {
            let uid = crate::people::uid_of(c, &login).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            if uid != by_uid && (crate::is_owner(uid) || crate::people::is_staff(uid)) && !crate::is_owner(by_uid) {
                return Err(fail(StatusCode::FORBIDDEN, "owner"));
            }
            let t = crate::mail::staff_token(c, uid).map_err(|_| fail(StatusCode::SERVICE_UNAVAILABLE, "busy"))?;
            crate::people::audit(c, by_uid, "pass.link", &uid.to_string(), json!({}));
            Ok(format!("{origin}/#/reset/{t}"))
        })
        .await?;
        Ok(Json(json!({ "link": link, "hours": 24 })).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct NewUid {
    to: Option<i64>,
    /// A text UID instead (empty: the number again).
    text: Option<String>,
}

/// A member given another UID (hers only): everything of theirs moves with it; the members kept in memory are made
/// again from the database.
async fn renumber(State(app): State<Shared>, h: HeaderMap, Key(login): Key<String>, Json(b): Json<NewUid>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        hers(w.uid)?;
        if let Some(text) = b.text {
            let by = w.uid;
            let text = text.trim().to_string();
            let shown = text.clone();
            let uid = db(&app, move |c| -> Result<i64, Response> {
                let uid = crate::people::uid_of(c, &login).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
                crate::people::set_text_uid(c, uid, &text).map_err(crate::people::text_uid_refused)?;
                crate::people::audit(c, by, "user.uid_text", &uid.to_string(), json!({ "text": text }));
                Ok(uid)
            })
            .await?;
            return Ok(Json(json!({ "from": uid, "to": uid, "uid_text": (!shown.is_empty()).then_some(shown) })).into_response());
        }
        let Some(to) = b.to else {
            return Err(said(StatusCode::BAD_REQUEST, "uid", "a UID from 0"));
        };
        if !(0..10_000_000).contains(&to) {
            return Err(said(StatusCode::BAD_REQUEST, "uid", "a UID from 0"));
        }
        let by = w.uid;
        let from = db(&app, move |c| -> Result<i64, Response> {
            let from = crate::people::uid_of(c, &login).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            if crate::is_owner(from) || crate::is_owner(to) {
                return Err(said(StatusCode::BAD_REQUEST, "uid", "Owner account UIDs cannot be moved or reused"));
            }
            crate::renumber(c, from, to).map_err(|e| said(StatusCode::CONFLICT, "uid", &e))?;
            crate::people::sweep(c);
            crate::people::refresh(c, from);
            crate::people::refresh(c, to);
            crate::people::audit(c, by, "user.uid", &to.to_string(), json!({ "from": from, "to": to }));
            Ok(from)
        })
        .await?;
        Ok(Json(json!({ "from": from, "to": to })).into_response())
    })
    .await
}

async fn delete_user(State(app): State<Shared>, h: HeaderMap, Key(login): Key<String>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        hers(w.uid)?;
        let by = w.uid;
        db(&app, move |c| -> Result<(), Response> {
            let uid = crate::people::uid_of(c, &login).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            crate::beta::delete(c, uid).map_err(|e| said(StatusCode::CONFLICT, "delete", &e))?;
            crate::people::refresh(c, uid);
            crate::people::audit(c, by, "user.delete", &uid.to_string(), json!({ "login": login }));
            Ok(())
        })
        .await?;
        Ok(Json(json!({})).into_response())
    })
    .await
}

// ------------------------------------------------------------------------------------------------ her switches

#[derive(Deserialize)]
struct Reg {
    state: Option<String>,
    limit: Option<i64>,
}

async fn registration(State(app): State<Shared>, h: HeaderMap, body: Option<Json<Reg>>) -> Response {
    run(async {
        let w = member(&app, &h, body.is_some()).await?;
        if body.is_some() {
            hers(w.uid)?;
        } else {
            may(w.uid, STAFF_PANEL)?;
        }
        let by = w.uid;
        let r = body.map(|Json(r)| r).unwrap_or(Reg { state: None, limit: None });
        let changing = r.state.is_some() || r.limit.is_some();
        let v = db(&app, move |c| -> Result<Value, Response> {
            let v = crate::beta::set_registration(c, r.state.as_deref(), r.limit).map_err(|e| said(StatusCode::BAD_REQUEST, "registration", &e))?;
            if changing {
                crate::people::audit(c, by, "registration", "", json!({ "state": r.state, "limit": r.limit }));
            }
            Ok(v)
        })
        .await?;
        Ok(Json(v).into_response())
    })
    .await
}

async fn pinned(State(app): State<Shared>, h: HeaderMap) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        may(w.uid, STAFF_PANEL)?;
        let v = db(&app, |c| crate::blocklist::pinned(c, true)).await;
        Ok(Json(json!({ "items": v })).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct Pin {
    address: String,
    #[serde(default)]
    name: String,
}

async fn pin(State(app): State<Shared>, h: HeaderMap, Json(p): Json<Pin>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        hers(w.uid)?;
        let by = w.uid;
        db(&app, move |c| -> Result<(), Response> {
            crate::blocklist::pin(c, &p.address, &p.name).map_err(|e| said(StatusCode::BAD_REQUEST, "pin", &e))?;
            crate::people::audit(c, by, "server.pin", &p.address, json!({ "name": p.name }));
            Ok(())
        })
        .await?;
        Ok(Json(json!({})).into_response())
    })
    .await
}

async fn unpin(State(app): State<Shared>, h: HeaderMap, Key(address): Key<String>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        hers(w.uid)?;
        let by = w.uid;
        db(&app, move |c| -> Result<(), Response> {
            crate::blocklist::unpin(c, &address).map_err(|e| said(StatusCode::BAD_REQUEST, "pin", &e))?;
            crate::people::audit(c, by, "server.unpin", &address, json!({}));
            Ok(())
        })
        .await?;
        Ok(Json(json!({})).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct Channel {
    channel: Option<String>,
}

async fn builds(State(app): State<Shared>, h: HeaderMap, Query(q): Query<Channel>) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        may(w.uid, STAFF_PANEL)?;
        let channel = q.channel.unwrap_or_else(|| "beta".into());
        let v = db(&app, move |c| crate::dist::list(c, &channel)).await;
        Ok(Json(json!({ "items": v })).into_response())
    })
    .await
}

async fn rollback(State(app): State<Shared>, h: HeaderMap, Json(q): Json<Channel>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        hers(w.uid)?;
        let by = w.uid;
        let channel = q.channel.unwrap_or_else(|| "beta".into());
        let build = db(&app, move |c| -> Result<i64, Response> {
            let b = crate::dist::rollback(c, &channel).map_err(|e| said(StatusCode::CONFLICT, "rollback", &e))?;
            crate::people::audit(c, by, "release.rollback", &channel, json!({ "build": b }));
            Ok(b)
        })
        .await?;
        Ok(Json(json!({ "build": build })).into_response())
    })
    .await
}

// ------------------------------------------------------------------------------------------------ roles, journal

async fn roles(State(app): State<Shared>, h: HeaderMap) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        may(w.uid, STAFF_PANEL)?;
        let v = db(&app, |c| {
            let holders: Vec<(String, i64)> = c.prepare("SELECT role, uid FROM roles ORDER BY since").and_then(|mut q| q.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).map(|m| m.flatten().collect())).unwrap_or_default();
            let roles: Vec<Value> = crate::people::ROLES
                .iter()
                .map(|r| {
                    let people: Vec<Value> = if r.id == "creator" {
                        crate::OWNERS.iter().filter(|uid| c.query_row("SELECT 1 FROM users WHERE uid=?1", params![uid], |_| Ok(())).is_ok()).map(|uid| crate::people::card_of(c, *uid)).collect()
                    } else if r.id == "beta" {
                        Vec::new()
                    } else {
                        holders.iter().filter(|(id, _)| id == r.id).map(|(_, u)| crate::people::card_of(c, *u)).collect()
                    };
                    let count = if r.id == "creator" { people.len() } else { holders.iter().filter(|(id, _)| id == r.id).count() };
                    json!({ "id": r.id, "en": r.en, "ru": r.ru, "prefix": r.prefix, "colour": r.colour, "staff": r.staff, "perms": r.perms, "people": people, "count": count })
                })
                .collect();
            json!({ "roles": roles, "perms": crate::people::PERMS })
        })
        .await;
        Ok(Json(v).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct JournalQ {
    before: Option<i64>,
    action: Option<String>,
    actor: Option<String>,
}

async fn journal(State(app): State<Shared>, h: HeaderMap, Query(q): Query<JournalQ>) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        may(w.uid, AUDIT)?;
        let v = db(&app, move |c| -> rusqlite::Result<Value> {
            let actor = match q.actor.as_deref().filter(|a| !a.is_empty()) {
                Some(l) => crate::people::uid_of(c, l).unwrap_or(-99),
                None => -1,
            };
            let action = q.action.map(|a| format!("{}%", clean_line(&a, 40))).unwrap_or_else(|| "%".into());
            let rows: Vec<Value> = c
                .prepare("SELECT id, at, actor, action, target, details FROM audit WHERE id < ?1 AND action LIKE ?2 AND (?3 = -1 OR actor = ?3) ORDER BY id DESC LIMIT 60")?
                .query_map(params![q.before.unwrap_or(i64::MAX), action, actor], |r| {
                    let target: String = r.get(4)?;
                    Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, r.get::<_, String>(3)?, target, r.get::<_, String>(5)?))
                })?
                .flatten()
                .map(|(id, at, actor, action, target, details)| {
                    // A target that is a member's UID is shown as them.
                    let person = target.parse::<i64>().ok().filter(|_| action.starts_with("user.") || action.starts_with("device.") || action.starts_with("forum.warn") || action.starts_with("forum.mute") || action.starts_with("forum.unmute") || action.starts_with("role")).map(|u| crate::people::card_of(c, u));
                    json!({ "id": id, "at": at, "by": crate::people::card_of(c, actor), "action": action, "target": target, "person": person, "details": serde_json::from_str::<Value>(&details).unwrap_or(Value::Null) })
                })
                .collect();
            Ok(json!({ "items": rows }))
        })
        .await
        .map_err(|_| busy())?;
        Ok(Json(v).into_response())
    })
    .await
}

// ------------------------------------------------------------------------------------------------ announcements

#[derive(Deserialize)]
struct Announce {
    title: String,
    body: String,
    #[serde(default)]
    site: bool,
    #[serde(default)]
    forum: bool,
    #[serde(default)]
    game: bool,
    #[serde(default)]
    discord: bool,
    #[serde(default)]
    everyone: bool,
    #[serde(default)]
    telegram: bool,
}

/// An announcement written once and sent where it was asked: the site's news and the forum's announcements at once,
/// every member a notification (the game shows it), and for the bot to take — the Discord servers and her Telegram.
async fn announce(State(app): State<Shared>, h: HeaderMap, Json(a): Json<Announce>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        may(w.uid, ANNOUNCE)?;
        let by = w.uid;
        let title = clean_line(&a.title, 120);
        let body: String = a.body.replace("\r\n", "\n").chars().filter(|c| *c == '\n' || !c.is_control()).take(4000).collect::<String>().trim().to_string();
        if title.chars().count() < 3 || body.is_empty() {
            return Err(said(StatusCode::BAD_REQUEST, "text", "a title and the words"));
        }
        if !(a.site || a.forum || a.game || a.discord || a.telegram) {
            return Err(said(StatusCode::BAD_REQUEST, "targets", "somewhere to send it"));
        }
        let targets = json!({ "site": a.site, "forum": a.forum, "game": a.game, "discord": a.discord, "everyone": a.everyone && a.discord, "telegram": a.telegram });
        let v = db(&app, move |c| -> Result<Value, Response> {
            let at = now();
            c.execute("INSERT INTO announcements (title, body, by, at, targets) VALUES (?1, ?2, ?3, ?4, ?5)", params![title, body, by, at, targets.to_string()]).map_err(|_| busy())?;
            let id = c.last_insert_rowid();
            if a.forum {
                let tx = c.unchecked_transaction().map_err(|_| busy())?;
                tx.execute("INSERT INTO threads (category, author, title, search, created, last_at, last_by) VALUES ('announcements', ?1, ?2, ?3, ?4, ?4, ?1)", params![by, title, title.to_lowercase(), at]).map_err(|_| busy())?;
                let thread = tx.last_insert_rowid();
                tx.execute("INSERT INTO posts (thread, author, body, created) VALUES (?1, ?2, ?3, ?4)", params![thread, by, body, at]).map_err(|_| busy())?;
                let post = tx.last_insert_rowid();
                tx.execute("UPDATE threads SET last_post = ?2 WHERE id = ?1", params![thread, post]).map_err(|_| busy())?;
                tx.commit().map_err(|_| busy())?;
                crate::forum2::follow(c, thread, by);
                c.execute("UPDATE announcements SET thread = ?2 WHERE id = ?1", params![id, thread]).map_err(|_| busy())?;
            }
            if a.game {
                let thread: Option<i64> = c.query_row("SELECT thread FROM announcements WHERE id = ?1", params![id], |r| r.get(0)).ok().flatten();
                let uids: Vec<i64> = c.prepare("SELECT uid FROM users").and_then(|mut q| q.query_map([], |r| r.get(0)).map(|m| m.flatten().collect())).unwrap_or_default();
                let excerpt = crate::notify::excerpt(&body);
                for u in uids {
                    crate::notify::add(c, u, "system", None, &format!("announcement:{id}"), json!({ "title": title, "text": excerpt, "announcement": id, "thread": thread, "href": "#/news" }));
                }
            }
            crate::people::audit(c, by, "announce", &id.to_string(), json!({ "title": title, "targets": targets }));
            Ok(json!({ "id": id }))
        })
        .await?;
        Ok((StatusCode::CREATED, Json(v)).into_response())
    })
    .await
}

fn announcement_json(c: &Connection, r: &rusqlite::Row) -> rusqlite::Result<Value> {
    let targets: Value = serde_json::from_str(&r.get::<_, String>(5)?).unwrap_or(Value::Null);
    Ok(json!({
        "id": r.get::<_, i64>(0)?, "title": r.get::<_, String>(1)?, "body": r.get::<_, String>(2)?,
        "by": crate::people::card_of(c, r.get(3)?), "at": r.get::<_, i64>(4)?, "targets": targets,
        "thread": r.get::<_, Option<i64>>(6)?,
        "discord": r.get::<_, Option<String>>(7)?.and_then(|d| serde_json::from_str::<Value>(&d).ok()),
        "discord_at": r.get::<_, Option<i64>>(8)?, "telegram_at": r.get::<_, Option<i64>>(9)?, "error": r.get::<_, Option<String>>(10)?,
    }))
}

const ANNOUNCEMENT: &str = "SELECT id, title, body, by, at, targets, thread, discord, discord_at, telegram_at, error FROM announcements";

async fn announcements(State(app): State<Shared>, h: HeaderMap) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        may(w.uid, ANNOUNCE)?;
        let v = db(&app, |c| -> rusqlite::Result<Value> {
            let rows: Vec<Value> = c.prepare(&format!("{ANNOUNCEMENT} ORDER BY id DESC LIMIT 30"))?.query_map([], |r| announcement_json(c, r))?.flatten().collect();
            Ok(json!({ "items": rows }))
        })
        .await
        .map_err(|_| busy())?;
        Ok(Json(v).into_response())
    })
    .await
}

/// The site's news: what was announced for the site (members only, as the rest of it).
async fn news(State(app): State<Shared>, h: HeaderMap) -> Response {
    run(async {
        let _ = member(&app, &h, false).await?;
        let v = db(&app, |c| -> rusqlite::Result<Value> {
            let rows: Vec<Value> = c
                .prepare(&format!("{ANNOUNCEMENT} WHERE json_extract(targets, '$.site') = 1 ORDER BY id DESC LIMIT 20"))?
                .query_map([], |r| announcement_json(c, r))?
                .flatten()
                .map(|mut v| {
                    v["targets"] = Value::Null;
                    v["discord"] = Value::Null;
                    v["error"] = Value::Null;
                    v
                })
                .collect();
            Ok(json!({ "items": rows }))
        })
        .await
        .map_err(|_| busy())?;
        Ok(Json(v).into_response())
    })
    .await
}

/// The bot's door: is it the bot (its key).
async fn the_bot(app: &Shared, h: &HeaderMap) -> Result<(), Response> {
    let key = h.get("x-cursed-key").and_then(|v| v.to_str().ok()).ok_or_else(|| fail(StatusCode::UNAUTHORIZED, "key"))?;
    let hash: String = crate::digest(key.trim()).iter().map(|b| format!("{b:02x}")).collect();
    let known = db(app, |c| c.query_row("SELECT value FROM settings WHERE name = 'service_key'", [], |r| r.get::<_, String>(0)).optional().ok().flatten()).await;
    if known.as_deref() == Some(hash.as_str()) {
        Ok(())
    } else {
        Err(fail(StatusCode::UNAUTHORIZED, "key"))
    }
}

/// What the bot has still to post: to the Discord servers, to her Telegram.
async fn pending(State(app): State<Shared>, h: HeaderMap) -> Response {
    run(async {
        the_bot(&app, &h).await?;
        let v = db(&app, |c| -> rusqlite::Result<Value> {
            let rows: Vec<Value> = c
                .prepare(&format!(
                    "{ANNOUNCEMENT} WHERE (json_extract(targets, '$.discord') = 1 AND discord_at IS NULL) OR (json_extract(targets, '$.telegram') = 1 AND telegram_at IS NULL) ORDER BY id"
                ))?
                .query_map([], |r| announcement_json(c, r))?
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
struct Done {
    /// The messages posted: [{guild, channel, message}].
    discord: Option<Value>,
    #[serde(default)]
    telegram: bool,
    error: Option<String>,
}

async fn done(State(app): State<Shared>, h: HeaderMap, Key(id): Key<i64>, Json(d): Json<Done>) -> Response {
    run(async {
        the_bot(&app, &h).await?;
        db(&app, move |c| -> Result<(), Response> {
            let at = now();
            if let Some(posted) = d.discord.filter(|v| v.is_array()) {
                c.execute("UPDATE announcements SET discord = ?2, discord_at = ?3 WHERE id = ?1", params![id, posted.to_string(), at]).map_err(|_| busy())?;
            }
            if d.telegram {
                c.execute("UPDATE announcements SET telegram_at = ?2 WHERE id = ?1", params![id, at]).map_err(|_| busy())?;
            }
            if let Some(e) = d.error {
                c.execute("UPDATE announcements SET error = ?2 WHERE id = ?1", params![id, clean_line(&e, 300)]).map_err(|_| busy())?;
            }
            Ok(())
        })
        .await?;
        Ok(Json(json!({})).into_response())
    })
    .await
}

pub fn routes() -> Router<Shared> {
    let json = || DefaultBodyLimit::max(16 * 1024);
    Router::new()
        .route("/api/admin/summary", get(summary))
        .route("/api/admin/crashes", get(crashes))
        .route("/api/admin/crashes/{id}", get(crash))
        .route("/api/admin/users", get(users))
        .route("/api/admin/users/{login}", get(user).delete(delete_user))
        .route("/api/admin/users/{login}/notes", post(add_note).layer(json()))
        .route("/api/admin/users/{login}/ban", post(ban).delete(unban).layer(json()))
        .route("/api/admin/users/{login}/device", post(free_device))
        .route("/api/admin/users/{login}/reset-link", post(reset_link))
        .route("/api/admin/users/{login}/uid", post(renumber).layer(json()))
        .route("/api/admin/switch/registration", get(registration).post(registration).layer(json()))
        .route("/api/admin/pinned", get(pinned).post(pin).layer(json()))
        .route("/api/admin/pinned/{address}", delete(unpin))
        .route("/api/admin/builds", get(builds))
        .route("/api/admin/builds/rollback", post(rollback).layer(json()))
        .route("/api/admin/roles", get(roles))
        .route("/api/admin/journal", get(journal))
        .route("/api/admin/announcements", get(announcements).post(announce).layer(json()))
        .route("/api/news", get(news))
        .route("/api/service/announcements", get(pending))
        .route("/api/service/announcements/{id}", post(done).layer(json()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `CRASH_DIR=<folder> cargo test --release real_crashes -- --ignored --nocapture`: the causes of real reports.
    #[test]
    #[ignore]
    fn real_crashes() {
        let dir = std::env::var("CRASH_DIR").expect("CRASH_DIR");
        let mut seen: HashMap<String, (usize, BTreeSet<String>)> = HashMap::new();
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let text = String::from_utf8_lossy(&std::fs::read(e.path()).unwrap()).to_string();
            let (sig, gpu) = crash_cause(&text);
            let s = seen.entry(sig).or_default();
            s.0 += 1;
            s.1.extend(gpu);
        }
        let mut list: Vec<_> = seen.into_iter().collect();
        list.sort_by(|a, b| b.1 .0.cmp(&a.1 .0));
        for (sig, (n, gpus)) in list {
            println!("{n:>3}  {sig}  {gpus:?}");
        }
    }

    #[test]
    fn a_crash_comes_down_to_its_cause() {
        let native = "Cursed 0.1.0-beta (build 3)\nexit: 0\n\n===== java runtime =====\n#  EXCEPTION_ACCESS_VIOLATION (0xc0000005) at pc=0x1\n# Problematic frame:\n# C  [ig12icd64.dll+0x34a72b]\n";
        assert_eq!(crash_cause(native).0, "EXCEPTION_ACCESS_VIOLATION in ig12icd64.dll");
        let java = "Cursed\nexit: 0\n===== crash report =====\nDescription: Client shutdown\njava.lang.Error: Watchdog\n\tat x\n\tGraphics card #0 name: Intel(R) UHD Graphics 610\n";
        let (sig, gpu) = crash_cause(java);
        assert_eq!(sig, "Client shutdown · java.lang.Error: Watchdog");
        assert_eq!(gpu.as_deref(), Some("Intel(R) UHD Graphics 610"));
        let bare = "Cursed 0.1.0-beta (build 5)\nexit: -1073741819 (0xC0000005)\n\n[x] Renderer: NVIDIA GeForce RTX 4060/PCIe/SSE2\n";
        let (sig, gpu) = crash_cause(bare);
        assert_eq!(sig, "exit -1073741819 (0xC0000005) (no report)");
        assert_eq!(gpu.as_deref(), Some("NVIDIA GeForce RTX 4060"));
    }
}
