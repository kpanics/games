//! Parties: players of Cursed together for a while — joined by a code, or invited by their nickname in the game while
//! they play on the same server — who see each other's marks: pings (they go in under a minute) and marks kept until
//! taken away. The client asks every couple of seconds (it says who it is in the game, where it plays, which world):
//! the party, its members (where they play: not where they stand), its marks, the invitations waiting.
//!
//! Nothing here is kept for long: a party with nobody in it goes, marks of a party that went go with it.

use axum::extract::{DefaultBodyLimit, Path as Key, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::market::{busy, clean_line, member, run, said};
use crate::{db, fail, now, Shared};

/// At most so many in a party, marks in it, and how long a ping and a mark live.
const MOST_MEMBERS: i64 = 10;
const MOST_MARKS: i64 = 24;
const PING_LIFE: i64 = 45;
const MARK_LIFE: i64 = 4 * 3600;
/// Seen within this long: playing now.
const ONLINE: i64 = 20;

pub fn schema(c: &Connection) {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS parties (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             code TEXT NOT NULL UNIQUE,
             leader INTEGER NOT NULL,
             created INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS party_members (
             uid INTEGER PRIMARY KEY REFERENCES users(uid) ON DELETE CASCADE,
             party INTEGER NOT NULL REFERENCES parties(id) ON DELETE CASCADE,
             joined INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS party_seen (
             uid INTEGER PRIMARY KEY REFERENCES users(uid) ON DELETE CASCADE,
             mc TEXT NOT NULL,
             server TEXT NOT NULL,
             world TEXT NOT NULL,
             at INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS party_invites (
             party INTEGER NOT NULL REFERENCES parties(id) ON DELETE CASCADE,
             uid INTEGER NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
             by INTEGER NOT NULL,
             at INTEGER NOT NULL,
             PRIMARY KEY (party, uid));
         CREATE TABLE IF NOT EXISTS party_marks (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             party INTEGER NOT NULL REFERENCES parties(id) ON DELETE CASCADE,
             uid INTEGER NOT NULL,
             world TEXT NOT NULL,
             x REAL NOT NULL, y REAL, z REAL NOT NULL,
             name TEXT NOT NULL,
             icon TEXT NOT NULL,
             colour INTEGER NOT NULL,
             ping INTEGER NOT NULL,
             created INTEGER NOT NULL,
             until INTEGER NOT NULL);
         CREATE INDEX IF NOT EXISTS party_marks_party ON party_marks(party);",
    )
    .expect("make the parties' tables");
}

fn party_of(c: &Connection, uid: i64) -> Option<i64> {
    c.query_row("SELECT party FROM party_members WHERE uid = ?1", params![uid], |r| r.get(0)).optional().ok().flatten()
}

fn new_code(c: &Connection) -> String {
    // Letters that cannot be mistaken one for another.
    const ABC: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    loop {
        let raw = crate::random_hex(6);
        let code: String = raw.as_bytes().chunks(2).take(6).map(|p| ABC[(u8::from_str_radix(std::str::from_utf8(p).unwrap_or("00"), 16).unwrap_or(0) as usize) % ABC.len()] as char).collect();
        let taken = c.query_row("SELECT 1 FROM parties WHERE code = ?1", params![code], |_| Ok(())).optional().ok().flatten().is_some();
        if !taken {
            return code;
        }
    }
}

/// Out of a party (their marks there go with them): a leader leaving hands it to the one who joined after; nobody
/// left, it goes.
pub fn leave(c: &Connection, uid: i64) {
    let Some(party) = party_of(c, uid) else { return };
    let _ = c.execute("DELETE FROM party_members WHERE uid = ?1", params![uid]);
    let _ = c.execute("DELETE FROM party_marks WHERE party = ?1 AND uid = ?2", params![party, uid]);
    let next: Option<i64> = c.query_row("SELECT uid FROM party_members WHERE party = ?1 ORDER BY joined LIMIT 1", params![party], |r| r.get(0)).optional().ok().flatten();
    match next {
        None => {
            let _ = c.execute("DELETE FROM party_marks WHERE party = ?1", params![party]);
            let _ = c.execute("DELETE FROM parties WHERE id = ?1", params![party]);
        }
        Some(n) => {
            let _ = c.execute("UPDATE parties SET leader = ?2 WHERE id = ?1 AND leader = ?3", params![party, n, uid]);
        }
    }
}

fn join(c: &Connection, uid: i64, party: i64) -> Result<(), Response> {
    if party_of(c, uid) == Some(party) {
        return Ok(());
    }
    let n: i64 = c.query_row("SELECT COUNT(*) FROM party_members WHERE party = ?1", params![party], |r| r.get(0)).map_err(|_| busy())?;
    if n >= MOST_MEMBERS {
        return Err(said(StatusCode::CONFLICT, "full", "ten in a party at most"));
    }
    leave(c, uid);
    c.execute("INSERT INTO party_members (uid, party, joined) VALUES (?1, ?2, ?3)", params![uid, party, now()]).map_err(|_| busy())?;
    let _ = c.execute("DELETE FROM party_invites WHERE uid = ?1", params![uid]);
    Ok(())
}

#[derive(Deserialize)]
struct Here {
    #[serde(default)]
    mc: String,
    #[serde(default)]
    server: String,
    #[serde(default)]
    world: String,
}

fn person(c: &Connection, uid: i64) -> (String, Option<i64>) {
    let login = c.query_row("SELECT login FROM users WHERE uid = ?1", params![uid], |r| r.get::<_, String>(0)).unwrap_or_default();
    (login, None)
}

/// The client's look at its party (and where it is told to the party).
async fn state(State(app): State<Shared>, h: HeaderMap, Query(q): Query<Here>) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        let uid = w.uid;
        let mc: String = q.mc.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').take(16).collect();
        let server = clean_line(&q.server, 80).to_lowercase();
        let world = clean_line(&q.world, 120);
        let v = db(&app, move |c| -> rusqlite::Result<Value> {
            let at = now();
            if !mc.is_empty() {
                c.execute(
                    "INSERT INTO party_seen (uid, mc, server, world, at) VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT(uid) DO UPDATE SET mc = ?2, server = ?3, world = ?4, at = ?5",
                    params![uid, mc, server, world, at],
                )?;
            }
            let _ = c.execute("DELETE FROM party_marks WHERE until < ?1", params![at]);
            let _ = c.execute("DELETE FROM party_invites WHERE at < ?1", params![at - 600]);
            let invites: Vec<Value> = c
                .prepare("SELECT i.party, u.login FROM party_invites i JOIN users u ON u.uid = i.by WHERE i.uid = ?1 ORDER BY i.at DESC")?
                .query_map(params![uid], |r| Ok(json!({ "party": r.get::<_, i64>(0)?, "from": r.get::<_, String>(1)? })))?
                .flatten()
                .collect();
            let Some(party) = party_of(c, uid) else {
                return Ok(json!({ "party": null, "invites": invites }));
            };
            let (code, leader): (String, i64) = c.query_row("SELECT code, leader FROM parties WHERE id = ?1", params![party], |r| Ok((r.get(0)?, r.get(1)?)))?;
            let members: Vec<Value> = c
                .prepare("SELECT m.uid, u.login, s.mc, s.server, s.world, s.at FROM party_members m JOIN users u ON u.uid = m.uid LEFT JOIN party_seen s ON s.uid = m.uid WHERE m.party = ?1 ORDER BY m.joined")?
                .query_map(params![party], |r| {
                    let id: i64 = r.get(0)?;
                    let seen: Option<i64> = r.get(5)?;
                    Ok(json!({
                        "login": r.get::<_, String>(1)?, "mc": r.get::<_, Option<String>>(2)?, "server": r.get::<_, Option<String>>(3)?,
                        "world": r.get::<_, Option<String>>(4)?, "online": seen.is_some_and(|s| at - s < ONLINE), "leader": id == leader, "me": id == uid,
                    }))
                })?
                .flatten()
                .collect();
            let marks: Vec<Value> = c
                .prepare("SELECT m.id, m.uid, u.login, m.world, m.x, m.y, m.z, m.name, m.icon, m.colour, m.ping, m.created, m.until FROM party_marks m JOIN users u ON u.uid = m.uid WHERE m.party = ?1 ORDER BY m.id")?
                .query_map(params![party], |r| {
                    Ok(json!({
                        "id": r.get::<_, i64>(0)?, "mine": r.get::<_, i64>(1)? == uid, "by": r.get::<_, String>(2)?, "world": r.get::<_, String>(3)?,
                        "x": r.get::<_, f64>(4)?, "y": r.get::<_, Option<f64>>(5)?, "z": r.get::<_, f64>(6)?, "name": r.get::<_, String>(7)?, "icon": r.get::<_, String>(8)?,
                        "colour": r.get::<_, i64>(9)?, "ping": r.get::<_, i64>(10)? == 1, "created": r.get::<_, i64>(11)?, "until": r.get::<_, i64>(12)?,
                    }))
                })?
                .flatten()
                .collect();
            let _ = person;
            Ok(json!({ "party": { "id": party, "code": code, "leader": leader == uid, "members": members, "marks": marks }, "invites": invites, "now": at }))
        })
        .await
        .map_err(|_| busy())?;
        Ok(Json(v).into_response())
    })
    .await
}

async fn create(State(app): State<Shared>, h: HeaderMap) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        let code = db(&app, move |c| -> Result<String, Response> {
            leave(c, uid);
            let code = new_code(c);
            c.execute("INSERT INTO parties (code, leader, created) VALUES (?1, ?2, ?3)", params![code, uid, now()]).map_err(|_| busy())?;
            let id = c.last_insert_rowid();
            join(c, uid, id)?;
            Ok(code)
        })
        .await?;
        Ok((StatusCode::CREATED, Json(json!({ "code": code }))).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct Join {
    code: Option<String>,
    party: Option<i64>,
}

async fn join_door(State(app): State<Shared>, h: HeaderMap, Json(j): Json<Join>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        db(&app, move |c| -> Result<(), Response> {
            let party = match (j.code, j.party) {
                (Some(code), _) => {
                    let code: String = code.to_uppercase().chars().filter(|c| c.is_ascii_alphanumeric()).collect();
                    c.query_row("SELECT id FROM parties WHERE code = ?1", params![code], |r| r.get::<_, i64>(0)).optional().map_err(|_| busy())?.ok_or_else(|| said(StatusCode::NOT_FOUND, "code", "no such party"))?
                }
                (None, Some(p)) => {
                    let invited = c.query_row("SELECT 1 FROM party_invites WHERE party = ?1 AND uid = ?2", params![p, uid], |_| Ok(())).optional().map_err(|_| busy())?.is_some();
                    if !invited {
                        return Err(said(StatusCode::NOT_FOUND, "invite", "the invitation is gone"));
                    }
                    p
                }
                _ => return Err(fail(StatusCode::BAD_REQUEST, "code")),
            };
            join(c, uid, party)
        })
        .await?;
        Ok(Json(json!({})).into_response())
    })
    .await
}

async fn leave_door(State(app): State<Shared>, h: HeaderMap) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        db(&app, move |c| leave(c, uid)).await;
        Ok(Json(json!({})).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct Login {
    login: String,
}

async fn kick(State(app): State<Shared>, h: HeaderMap, Json(k): Json<Login>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        db(&app, move |c| -> Result<(), Response> {
            let party = party_of(c, uid).ok_or_else(|| fail(StatusCode::NOT_FOUND, "party"))?;
            let leader: i64 = c.query_row("SELECT leader FROM parties WHERE id = ?1", params![party], |r| r.get(0)).map_err(|_| busy())?;
            if leader != uid {
                return Err(fail(StatusCode::FORBIDDEN, "leader"));
            }
            let target: i64 = c.query_row("SELECT uid FROM users WHERE login_key = ?1", params![k.login.to_lowercase()], |r| r.get(0)).optional().map_err(|_| busy())?.ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            if party_of(c, target) == Some(party) && target != uid {
                leave(c, target);
            }
            Ok(())
        })
        .await?;
        Ok(Json(json!({})).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct Invite {
    mc: String,
}

/// An invitation by a nickname in the game: to whoever of Cursed plays under it on the same server now. The one who
/// invites hears the same either way (who plays Cursed is not told).
async fn invite(State(app): State<Shared>, h: HeaderMap, Json(i): Json<Invite>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        let mc: String = i.mc.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').take(16).collect::<String>().to_lowercase();
        if mc.len() < 3 {
            return Err(said(StatusCode::BAD_REQUEST, "mc", "a nickname"));
        }
        db(&app, move |c| -> Result<(), Response> {
            let party = party_of(c, uid).ok_or_else(|| fail(StatusCode::NOT_FOUND, "party"))?;
            let mine: Option<String> = c.query_row("SELECT server FROM party_seen WHERE uid = ?1", params![uid], |r| r.get(0)).optional().map_err(|_| busy())?;
            let at = now();
            let found: Vec<i64> = c
                .prepare("SELECT uid FROM party_seen WHERE lower(mc) = ?1 AND at > ?2 AND (?3 IS NULL OR server = ?3) AND uid != ?4")
                .and_then(|mut q| q.query_map(params![mc, at - ONLINE * 3, mine, uid], |r| r.get(0)).map(|m| m.flatten().collect()))
                .unwrap_or_default();
            for t in found {
                if party_of(c, t) != Some(party) {
                    let _ = c.execute("INSERT OR REPLACE INTO party_invites (party, uid, by, at) VALUES (?1, ?2, ?3, ?4)", params![party, t, uid, at]);
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
struct Decline {
    party: i64,
}

async fn decline(State(app): State<Shared>, h: HeaderMap, Json(d): Json<Decline>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        db(&app, move |c| c.execute("DELETE FROM party_invites WHERE party = ?1 AND uid = ?2", params![d.party, uid])).await.map_err(|_| busy())?;
        Ok(Json(json!({})).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct NewMark {
    world: String,
    x: f64,
    y: Option<f64>,
    z: f64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    icon: String,
    #[serde(default)]
    colour: i64,
    #[serde(default)]
    ping: bool,
}

async fn mark(State(app): State<Shared>, h: HeaderMap, Json(m): Json<NewMark>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        if !(m.x.is_finite() && m.z.is_finite() && m.x.abs() < 3.0e7 && m.z.abs() < 3.0e7 && m.y.is_none_or(|y| y.is_finite() && y.abs() < 5000.0)) {
            return Err(fail(StatusCode::BAD_REQUEST, "place"));
        }
        let name = clean_line(&m.name, 24);
        let icon: String = m.icon.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').take(24).collect();
        let world = clean_line(&m.world, 120);
        let v = db(&app, move |c| -> Result<Value, Response> {
            let party = party_of(c, uid).ok_or_else(|| fail(StatusCode::NOT_FOUND, "party"))?;
            let at = now();
            // A new ping of one's own takes the place of the last one.
            if m.ping {
                let _ = c.execute("DELETE FROM party_marks WHERE party = ?1 AND uid = ?2 AND ping = 1", params![party, uid]);
            }
            let n: i64 = c.query_row("SELECT COUNT(*) FROM party_marks WHERE party = ?1", params![party], |r| r.get(0)).map_err(|_| busy())?;
            if n >= MOST_MARKS {
                let _ = c.execute("DELETE FROM party_marks WHERE id = (SELECT id FROM party_marks WHERE party = ?1 ORDER BY ping DESC, id LIMIT 1)", params![party]);
            }
            let until = at + if m.ping { PING_LIFE } else { MARK_LIFE };
            c.execute(
                "INSERT INTO party_marks (party, uid, world, x, y, z, name, icon, colour, ping, created, until) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![party, uid, world, m.x, m.y, m.z, if name.is_empty() { "Метка".to_string() } else { name }, if icon.is_empty() { "mark-pin".to_string() } else { icon }, m.colour & 0xFF_FFFF, m.ping as i64, at, until],
            )
            .map_err(|_| busy())?;
            Ok(json!({ "id": c.last_insert_rowid() }))
        })
        .await?;
        Ok((StatusCode::CREATED, Json(v)).into_response())
    })
    .await
}

async fn unmark(State(app): State<Shared>, h: HeaderMap, Key(id): Key<i64>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        db(&app, move |c| -> Result<(), Response> {
            let party = party_of(c, uid).ok_or_else(|| fail(StatusCode::NOT_FOUND, "party"))?;
            let leader: i64 = c.query_row("SELECT leader FROM parties WHERE id = ?1", params![party], |r| r.get(0)).map_err(|_| busy())?;
            c.execute("DELETE FROM party_marks WHERE id = ?1 AND party = ?2 AND (uid = ?3 OR ?4)", params![id, party, uid, leader == uid]).map_err(|_| busy())?;
            Ok(())
        })
        .await?;
        Ok(Json(json!({})).into_response())
    })
    .await
}

pub fn routes() -> Router<Shared> {
    let json = || DefaultBodyLimit::max(4 * 1024);
    Router::new()
        .route("/api/party", get(state).post(create))
        .route("/api/party/join", post(join_door).layer(json()))
        .route("/api/party/leave", post(leave_door))
        .route("/api/party/kick", post(kick).layer(json()))
        .route("/api/party/invite", post(invite).layer(json()))
        .route("/api/party/decline", post(decline).layer(json()))
        .route("/api/party/marks", post(mark).layer(json()))
        .route("/api/party/marks/{id}", delete(unmark))
}
