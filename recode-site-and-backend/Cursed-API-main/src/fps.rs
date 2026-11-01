//! The frame rate of every player (her word 19.10: "FPS of all players, in the site's admin page"). The client tells
//! it every four minutes of play (native/src/fpsreport.rs): the average and the 1% low, the hitches, and what it ran
//! on — the video card, the processor, the memory, the game's own memory, the window, the render distance, a
//! shaderpack, Optimization, the light settings, the server. The admin page shows it to her alone: by build (whether a
//! release helped), by kind of video card, the light settings against none, the slowest players. Kept 30 days.
//!
//! A player counts once in every figure: their own median first, then the median of those.

use std::collections::HashMap;

use axum::extract::{DefaultBodyLimit, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use rusqlite::{params, Connection};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::market::{clean_line, member, run};
use crate::{db, fail, now, Shared};

/// How long the reports are kept, seconds.
const KEPT: i64 = 30 * 86_400;
/// A player's reports closer than this to the one before are dropped (the client sends one in four minutes).
const LEAST_APART: i64 = 120;

pub fn schema(c: &Connection) {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS fps (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             uid INTEGER NOT NULL,
             at INTEGER NOT NULL,
             build INTEGER NOT NULL,
             avg REAL NOT NULL,                       -- frames a second over the stretch
             low REAL NOT NULL,                       -- the 1% low
             hitches INTEGER NOT NULL,                -- frames of 100 ms and more
             seconds INTEGER NOT NULL,                -- the stretch played
             gpu TEXT NOT NULL,
             cpu TEXT NOT NULL,
             threads INTEGER NOT NULL,
             ram REAL NOT NULL,                       -- the computer's, GB
             xmx INTEGER NOT NULL,                    -- the game's, MB
             rd INTEGER NOT NULL,
             shaders INTEGER NOT NULL,
             w INTEGER NOT NULL,
             h INTEGER NOT NULL,
             optimization INTEGER NOT NULL,
             light INTEGER NOT NULL,                  -- 0 none, 1 the light settings on, 2 given back
             weak INTEGER NOT NULL,
             server TEXT NOT NULL);
         CREATE INDEX IF NOT EXISTS fps_at ON fps(at);
         CREATE INDEX IF NOT EXISTS fps_uid ON fps(uid, at);",
    )
    .expect("the fps table");
}

#[derive(Deserialize)]
struct Report {
    #[serde(default)]
    build: i64,
    avg: f64,
    low: f64,
    #[serde(default)]
    hitches: i64,
    seconds: f64,
    #[serde(default)]
    gpu: String,
    #[serde(default)]
    cpu: String,
    #[serde(default)]
    threads: i64,
    #[serde(default)]
    ram: f64,
    #[serde(default)]
    xmx: i64,
    #[serde(default)]
    rd: i64,
    #[serde(default)]
    shaders: bool,
    #[serde(default)]
    w: i64,
    #[serde(default)]
    h: i64,
    #[serde(default)]
    optimization: bool,
    #[serde(default)]
    light: i64,
    #[serde(default)]
    weak: bool,
    #[serde(default)]
    server: String,
}

/// POST /api/me/fps from the game.
async fn report(State(app): State<Shared>, h: HeaderMap, Json(r): Json<Report>) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        let sane = |x: f64, most: f64| x.is_finite() && x >= 0.0 && x <= most;
        if !sane(r.avg, 10_000.0) || !sane(r.low, 10_000.0) || !sane(r.seconds, 3_600.0) || r.seconds < 30.0 {
            return Err(fail(StatusCode::BAD_REQUEST, "report"));
        }
        let uid = w.uid;
        db(&app, move |c| {
            let at = now();
            let last: i64 = c.query_row("SELECT COALESCE(MAX(at), 0) FROM fps WHERE uid = ?1", params![uid], |x| x.get(0)).unwrap_or(0);
            if at - last < LEAST_APART {
                return;
            }
            let _ = c.execute(
                "INSERT INTO fps (uid, at, build, avg, low, hitches, seconds, gpu, cpu, threads, ram, xmx, rd, shaders, w, h, optimization, light, weak, server)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
                params![
                    uid,
                    at,
                    r.build.clamp(0, 1_000_000),
                    r.avg,
                    r.low,
                    r.hitches.clamp(0, 1_000_000),
                    r.seconds.round() as i64,
                    clean_line(&r.gpu, 120),
                    clean_line(&r.cpu, 120),
                    r.threads.clamp(0, 1024),
                    if r.ram.is_finite() { r.ram.clamp(0.0, 4096.0) } else { 0.0 },
                    r.xmx.clamp(0, 1 << 20),
                    r.rd.clamp(0, 128),
                    r.shaders,
                    r.w.clamp(0, 100_000),
                    r.h.clamp(0, 100_000),
                    r.optimization,
                    r.light.clamp(0, 2),
                    r.weak,
                    clean_line(&r.server.to_ascii_lowercase(), 80),
                ],
            );
            if at % 50 == 0 {
                let _ = c.execute("DELETE FROM fps WHERE at < ?1", params![at - KEPT]);
            }
        })
        .await;
        Ok(Json(json!({})).into_response())
    })
    .await
}

/// The kind of a video card by its name, as weakpc.rs on the client tells weak ones and the crash logs showed them.
pub fn tier(gpu: &str) -> &'static str {
    let g = gpu.to_ascii_lowercase();
    let arc = g.contains("arc(tm) a") || g.contains("arc(tm) b");
    let radeon_m = g.contains("radeon") && ["610m", "660m", "680m", "740m", "760m", "780m", "880m", "890m"].iter().any(|k| g.contains(k));
    if (["intel", "radeon(tm) graphics", "radeon (tm) graphics", "vega", "radeon hd 8", "radeon r2", "radeon r3", "radeon r4", "radeon r5", "radeon r6", "radeon r7 graphics", "svga3d", "llvmpipe", "microsoft basic"].iter().any(|k| g.contains(k)) && !arc && !g.contains("rx vega"))
        || radeon_m
    {
        return "integrated";
    }
    if ["rtx", "rx 6", "rx 7", "rx 9", "rx 5700", "arc(tm) a", "arc(tm) b"].iter().any(|k| g.contains(k)) && !["rx 6400", "rx 6500", "rx 640", "rx 6300"].iter().any(|k| g.contains(k)) {
        return "modern";
    }
    if ["gtx 1060", "gtx 1070", "gtx 1080", "gtx 1650", "gtx 1660", "gtx 970", "gtx 980", "rx 470", "rx 480", "rx 570", "rx 580", "rx590", "rx 590", "2048sp", "rx 5500", "rx 5600", "rx 6400", "rx 6500", "rx vega"].iter().any(|k| g.contains(k)) {
        return "middle";
    }
    if g.is_empty() {
        return "unknown";
    }
    "old"
}

fn median(mut v: Vec<f64>) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

/// One player's stretch of days: their medians and what they played on last.
struct Player {
    uid: i64,
    avg: f64,
    low: f64,
    hitches: f64,
    reports: usize,
    last: Row,
}

#[derive(Clone)]
struct Row {
    at: i64,
    build: i64,
    avg: f64,
    low: f64,
    hitches: i64,
    seconds: i64,
    gpu: String,
    cpu: String,
    threads: i64,
    ram: f64,
    xmx: i64,
    rd: i64,
    shaders: bool,
    w: i64,
    h: i64,
    optimization: bool,
    light: i64,
    weak: bool,
    server: String,
}

/// A group's figures: its players, their median frame rate and 1% low, how many are under 60 and under 30.
fn group(players: &[&Player]) -> Value {
    let avgs: Vec<f64> = players.iter().map(|p| p.avg).collect();
    json!({
        "players": players.len(),
        "avg": (median(avgs.clone()) * 10.0).round() / 10.0,
        "low": (median(players.iter().map(|p| p.low).collect()) * 10.0).round() / 10.0,
        "under60": avgs.iter().filter(|&&a| a < 60.0).count(),
        "under30": avgs.iter().filter(|&&a| a < 30.0).count(),
    })
}

#[derive(Deserialize)]
struct Days {
    days: Option<i64>,
}

/// GET /api/admin/fps?days=N — hers alone.
async fn admin(State(app): State<Shared>, h: HeaderMap, Query(q): Query<Days>) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        if !crate::is_owner(w.uid) {
            return Err(fail(StatusCode::FORBIDDEN, "not-yours"));
        }
        let since = now() - q.days.unwrap_or(7).clamp(1, 30) * 86_400;
        let v = db(&app, move |c| -> rusqlite::Result<Value> {
            let mut st = c.prepare(
                "SELECT f.uid, f.at, f.build, f.avg, f.low, f.hitches, f.seconds, f.gpu, f.cpu, f.threads, f.ram, f.xmx, f.rd, f.shaders, f.w, f.h,
                        f.optimization, f.light, f.weak, f.server
                 FROM fps f WHERE f.at > ?1 ORDER BY f.at",
            )?;
            let rows: Vec<(i64, Row)> = st
                .query_map(params![since], |r| {
                    Ok((
                        r.get(0)?,
                        Row {
                            at: r.get(1)?,
                            build: r.get(2)?,
                            avg: r.get(3)?,
                            low: r.get(4)?,
                            hitches: r.get(5)?,
                            seconds: r.get(6)?,
                            gpu: r.get(7)?,
                            cpu: r.get(8)?,
                            threads: r.get(9)?,
                            ram: r.get(10)?,
                            xmx: r.get(11)?,
                            rd: r.get(12)?,
                            shaders: r.get(13)?,
                            w: r.get(14)?,
                            h: r.get(15)?,
                            optimization: r.get(16)?,
                            light: r.get(17)?,
                            weak: r.get(18)?,
                            server: r.get(19)?,
                        },
                    ))
                })?
                .flatten()
                .collect();
            let reports = rows.len();
            let hours = rows.iter().map(|(_, r)| r.seconds).sum::<i64>() as f64 / 3600.0;
            // Each player once (and once per build: a player who moved to a new build counts in both).
            let mut by_player: HashMap<i64, Vec<Row>> = HashMap::new();
            let mut by_player_build: HashMap<(i64, i64), Vec<Row>> = HashMap::new();
            for (uid, r) in &rows {
                by_player.entry(*uid).or_default().push(r.clone());
                by_player_build.entry((*uid, r.build)).or_default().push(r.clone());
            }
            let fold = |uid: i64, rs: &[Row]| Player {
                uid,
                avg: median(rs.iter().map(|r| r.avg).collect()),
                low: median(rs.iter().map(|r| r.low).collect()),
                hitches: rs.iter().map(|r| r.hitches as f64).sum::<f64>() / (rs.iter().map(|r| r.seconds).sum::<i64>().max(1) as f64 / 3600.0),
                reports: rs.len(),
                last: rs.last().cloned().expect("a player has a report"),
            };
            let players: Vec<Player> = by_player.iter().map(|(uid, rs)| fold(*uid, rs)).collect();
            let all: Vec<&Player> = players.iter().collect();

            let mut builds: HashMap<i64, Vec<Player>> = HashMap::new();
            for ((uid, build), rs) in &by_player_build {
                builds.entry(*build).or_default().push(fold(*uid, rs));
            }
            let mut builds: Vec<(i64, Vec<Player>)> = builds.into_iter().collect();
            builds.sort_by(|a, b| b.0.cmp(&a.0));

            let tiers = ["integrated", "old", "middle", "modern", "unknown"].iter().map(|t| {
                let ps: Vec<&Player> = players.iter().filter(|p| tier(&p.last.gpu) == *t).collect();
                json!({ "tier": t, "figures": group(&ps) })
            });
            let light = [0i64, 1, 2].iter().map(|l| {
                let ps: Vec<&Player> = players.iter().filter(|p| p.last.light == *l && p.last.weak).collect();
                json!({ "light": l, "figures": group(&ps) })
            });
            let mut gpus: HashMap<String, Vec<&Player>> = HashMap::new();
            for p in &players {
                gpus.entry(p.last.gpu.replace("/PCIe/SSE2", "")).or_default().push(p);
            }
            let mut gpus: Vec<(String, Vec<&Player>)> = gpus.into_iter().collect();
            gpus.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(&b.0)));
            let ram = [(0.0, 6.5, "≤6"), (6.5, 11.0, "8"), (11.0, 20.0, "12–16"), (20.0, 1e9, "32+")].iter().map(|(lo, hi, label)| {
                let ps: Vec<&Player> = players.iter().filter(|p| p.last.ram >= *lo && p.last.ram < *hi).collect();
                json!({ "ram": label, "figures": group(&ps) })
            });
            let mut slow: Vec<&Player> = players.iter().collect();
            slow.sort_by(|a, b| a.avg.partial_cmp(&b.avg).unwrap_or(std::cmp::Ordering::Equal));
            let login = |uid: i64| c.query_row("SELECT login FROM users WHERE uid = ?1", params![uid], |r| r.get::<_, String>(0)).unwrap_or_default();
            let slowest: Vec<Value> = slow
                .iter()
                .take(30)
                .map(|p| {
                    let r = &p.last;
                    json!({
                        "card": crate::people::card(p.uid, login(p.uid)),
                        "avg": (p.avg * 10.0).round() / 10.0, "low": (p.low * 10.0).round() / 10.0, "hitches_hour": p.hitches.round(), "reports": p.reports,
                        "gpu": r.gpu, "cpu": r.cpu, "threads": r.threads, "ram": r.ram, "xmx": r.xmx, "rd": r.rd, "shaders": r.shaders,
                        "screen": format!("{}×{}", r.w, r.h), "optimization": r.optimization, "light": r.light, "weak": r.weak,
                        "server": r.server, "build": r.build, "at": r.at, "last_avg": r.avg, "last_low": r.low,
                    })
                })
                .collect();
            Ok(json!({
                "reports": reports,
                "hours": (hours * 10.0).round() / 10.0,
                "all": group(&all),
                "builds": builds.iter().take(8).map(|(b, ps)| json!({ "build": b, "figures": group(&ps.iter().collect::<Vec<_>>()) })).collect::<Vec<_>>(),
                "tiers": tiers.collect::<Vec<_>>(),
                "light": light.collect::<Vec<_>>(),
                "ram": ram.collect::<Vec<_>>(),
                "gpus": gpus.iter().take(25).map(|(g, ps)| json!({ "gpu": g, "tier": tier(g), "figures": group(ps) })).collect::<Vec<_>>(),
                "slowest": slowest,
            }))
        })
        .await
        .map_err(|_| fail(StatusCode::INTERNAL_SERVER_ERROR, "db"))?;
        Ok(Json(v).into_response())
    })
    .await
}

pub fn routes() -> Router<Shared> {
    Router::new().route("/api/me/fps", post(report).layer(DefaultBodyLimit::max(4096))).route("/api/admin/fps", get(admin))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cards_are_told_apart() {
        for (gpu, t) in [
            ("Intel(R) UHD Graphics", "integrated"),
            ("AMD Radeon(TM) Graphics", "integrated"),
            ("AMD Radeon (TM) Graphics", "integrated"),
            ("NVIDIA GeForce GTX 1050 Ti/PCIe/SSE2", "old"),
            ("GeForce GT 710/PCIe/SSE2", "old"),
            ("NVIDIA GeForce GTX 1650/PCIe/SSE2", "middle"),
            ("AMD Radeon RX 580 2048SP", "middle"),
            ("AMD Radeon RX 5600 XT", "middle"),
            ("NVIDIA GeForce RTX 4060/PCIe/SSE2", "modern"),
            ("AMD Radeon RX 6600", "modern"),
            ("Intel(R) Arc(TM) A770 Graphics", "modern"),
            ("Radeon RX 560 Series", "old"),
            ("Intel(R) Arc(TM) B580 Graphics", "modern"),
            ("Radeon RX 640", "old"),
            ("Radeon RX Vega 56", "middle"),
            ("AMD Radeon(TM) 680M", "integrated"),
            ("AMD Radeon 780M Graphics", "integrated"),
            ("NVIDIA GeForce GTX 660M/PCIe/SSE2", "old"),
            ("AMD Radeon RX 7600", "modern"),
            ("AMD Radeon HD 8400 / R3 Series", "integrated"),
            ("", "unknown"),
        ] {
            assert_eq!(tier(gpu), t, "{gpu}");
        }
    }

    #[test]
    fn a_median_is_the_middle() {
        assert_eq!(median(vec![3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(vec![4.0, 1.0, 2.0, 3.0]), 2.5);
        assert_eq!(median(vec![]), 0.0);
    }
}
