//! The players who play with a cheat, marked for every player of Cursed on the same server (her word, 21.10: "кто
//! играет с Meteor, Baritone или прочей хуйнёй, того видят все игроки Cursed на сервере").
//!
//! The mods are known by their rules (`mod_rules`): a name, the ids their fabric.mod.json gives, the Modrinth projects
//! they are (slugs). A rule is a cheat (a mark everywhere), or a mod some servers ban (`on`: the servers by the names
//! the client knows them by; the player is only warned there). The rules go to the client in the signed blocklist
//! (blocklist.rs), so they change without a build: `cursed-api mod-rule …`.
//!
//! A client that runs a cheat says so (`POST /api/marks`): the account, the game's player it plays as (UUID), which
//! cheats; another client asks by the UUIDs it sees (`GET /api/marks`). A claim is the client's word for its UUID:
//! how a mark is shown from it (to everyone, or to the staff to check) waits for her word, since anyone may claim
//! another's UUID here. Each account's claim is kept apart, so one cannot take another's away.

use std::collections::HashMap;

use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use rusqlite::{params, Connection};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::market::{clean_line, member};
use crate::{now, Shared};

/// How long a claim holds without being said again (the client says it every five minutes while it plays).
const FRESH_MS: i64 = 15 * 60 * 1000;

pub fn schema(c: &Connection) {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS mod_rules (
             name TEXT PRIMARY KEY,
             ids TEXT NOT NULL,
             slugs TEXT NOT NULL,
             cheat INTEGER NOT NULL,
             servers TEXT NOT NULL,
             added INTEGER NOT NULL,
             by TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS cheat_marks (
             uid INTEGER NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
             mc_uuid TEXT NOT NULL,
             mods TEXT NOT NULL,
             updated INTEGER NOT NULL,
             PRIMARY KEY (uid, mc_uuid));
         CREATE INDEX IF NOT EXISTS cheat_marks_uuid ON cheat_marks(mc_uuid);
         INSERT OR IGNORE INTO settings VALUES ('mod_rules_seeded', '0');",
    )
    .expect("the marks' tables");
    let seeded = c.query_row("SELECT value FROM settings WHERE name = 'mod_rules_seeded'", [], |r| r.get::<_, String>(0)).unwrap_or_default();
    if seeded == "0" {
        for (name, ids, slugs, servers) in FIRST_RULES {
            let _ = put_rule(c, name, ids, slugs, servers, "first");
        }
        let _ = c.execute("UPDATE settings SET value = '1' WHERE name = 'mod_rules_seeded'", []);
        crate::blocklist::bump(c);
    }
}

/// The rules the list starts with: (name, ids, slugs, servers). No servers: a cheat. The servers by the names the
/// client's restrict.rs knows them by.
const FIRST_RULES: &[(&str, &str, &str, &str)] = &[
    // Cheat clients, as mods.
    ("Meteor Client", "meteor-client", "", ""),
    ("Wurst", "wurst", "", ""),
    ("LiquidBounce", "liquidbounce", "", ""),
    ("Aristois", "aristois", "", ""),
    ("Thunderhack", "thunderhack", "", ""),
    ("BleachHack", "bleachhack", "", ""),
    ("Boze", "boze", "", ""),
    ("Baritone", "baritone,baritone-meteor", "", ""),
    // Seeing through the ground.
    ("X-Ray", "xray,xtoxray,xray-orevision,advanced-xray,advancedxray", "x-to-xray,xray-orevision,advanced-xray-fabric", ""),
    // The camera away from the player.
    ("Freecam", "freecam,easy-freecam,legacyfreecam,freecam_by_kapiteon", "freecam,easy-freecam,legacyfreecam,freecammc,freecam_yannis_", ""),
    ("Camera Tweaks", "cameratweaks", "cameratweaks", ""),
    // Clicking by itself.
    (
        "Autoclicker",
        "autoclicker,enthusia_autoclicker,inputforge",
        "autoclicker,autoclicker.1.0,better-autoclicker,enthusia-autoclicker,clever-autoclicker-plus,inputforge",
        "",
    ),
    // What some servers ban (restrict.rs has what of the client's own they ban).
    ("ReplayMod", "replaymod", "replaymod", "FunTime,HolyTime"),
    ("Flashback", "flashback", "flashback", "FunTime,HolyTime"),
    ("Item Scroller", "itemscroller", "item-scroller", "FunTime,AresMine,HolyTime,HolyWorld"),
    ("Xaero's Minimap", "xaerominimap", "xaeros-minimap", "FunTime,AresMine,HolyTime,HolyWorld"),
    ("Xaero's World Map", "xaeroworldmap", "xaeros-world-map", "FunTime,AresMine,HolyTime,HolyWorld"),
    ("JourneyMap", "journeymap", "journeymap", "FunTime,AresMine,HolyTime,HolyWorld"),
    ("VoxelMap", "voxelmap", "voxelmap-updated", "FunTime,AresMine,HolyTime,HolyWorld"),
    ("ItemSwapper", "itemswapper", "itemswapper", "FunTime,HolyWorld"),
    ("Health bars", "neat,healthbars,provihealth,simple-health-bar,mr_entity_healthbars,mobhealthbar", "neat,new-health-bars,provis-health-bars,simple-health-bar,entity-health-bars", "FunTime"),
    ("Chunk Animator", "chunkanimator", "chunk-animator-plus", "FunTime"),
    ("Entity outlines", "entity_outline,more-outlines", "more-outlines", "FunTime"),
    ("Auction helpers", "auchelper,funtime-ah-helper", "", "FunTime,HolyTime"),
];

fn list_of(text: &str) -> Vec<String> {
    let mut out: Vec<String> = text.split(',').map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty() && s.len() <= 64).collect();
    out.dedup();
    out
}

/// A rule set (made, or made anew under the same name), and the list's new version for the clients. No ids and no
/// slugs: an error (it would know no mod).
pub fn set_rule(c: &Connection, name: &str, ids: &str, slugs: &str, servers: &str, by: &str) -> Result<(), String> {
    put_rule(c, name, ids, slugs, servers, by)?;
    crate::blocklist::bump(c);
    Ok(())
}

fn put_rule(c: &Connection, name: &str, ids: &str, slugs: &str, servers: &str, by: &str) -> Result<(), String> {
    let name = clean_line(name, 48);
    let (ids, slugs) = (list_of(ids), list_of(slugs));
    let servers: Vec<String> = servers.split(',').map(|s| clean_line(s, 32)).filter(|s| !s.is_empty()).collect();
    if name.is_empty() {
        return Err("a name is needed".into());
    }
    if ids.is_empty() && slugs.is_empty() {
        return Err("an id or a slug is needed".into());
    }
    c.execute(
        "INSERT INTO mod_rules (name, ids, slugs, cheat, servers, added, by) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(name) DO UPDATE SET ids = ?2, slugs = ?3, cheat = ?4, servers = ?5, added = ?6, by = ?7",
        params![name, json!(ids).to_string(), json!(slugs).to_string(), servers.is_empty() as i64, json!(servers).to_string(), now(), by],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// A rule taken off.
pub fn remove_rule(c: &Connection, name: &str) -> Result<(), String> {
    if c.execute("DELETE FROM mod_rules WHERE name = ?1 COLLATE NOCASE", params![name.trim()]).map_err(|e| e.to_string())? == 0 {
        return Err(format!("no rule named {name}"));
    }
    crate::blocklist::bump(c);
    Ok(())
}

/// Every rule, as the signed list carries it: `{name, ids, slugs, cheat, on}`.
pub fn rules(c: &Connection) -> Vec<Value> {
    let Ok(mut st) = c.prepare("SELECT name, ids, slugs, cheat, servers FROM mod_rules ORDER BY cheat DESC, name") else { return Vec::new() };
    st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?, r.get::<_, String>(4)?)))
        .map(|m| {
            m.flatten()
                .map(|(name, ids, slugs, cheat, servers)| {
                    let parse = |t: &str| serde_json::from_str::<Value>(t).unwrap_or(json!([]));
                    json!({ "name": name, "ids": parse(&ids), "slugs": parse(&slugs), "cheat": cheat != 0, "on": parse(&servers) })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The names of the cheats among `names` (what a client may claim), in the rules' own spelling.
fn cheats_among(c: &Connection, names: &[String]) -> Vec<String> {
    let Ok(mut st) = c.prepare("SELECT name FROM mod_rules WHERE cheat = 1") else { return Vec::new() };
    let known: Vec<String> = st.query_map([], |r| r.get::<_, String>(0)).map(|m| m.flatten().collect()).unwrap_or_default();
    let mut out: Vec<String> = known.into_iter().filter(|k| names.iter().any(|n| n.trim().eq_ignore_ascii_case(k))).collect();
    out.sort();
    out
}

fn millis() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// A UUID as one form: 32 hex digits, small letters.
fn uuid_of(s: &str) -> Option<String> {
    let u: String = s.chars().filter(|c| *c != '-').collect::<String>().to_ascii_lowercase();
    (u.len() == 32 && u.chars().all(|c| c.is_ascii_hexdigit())).then_some(u)
}

/// A member's claim kept (the cheats it names, in the rules' spelling), or taken off when it names none. The cheats
/// kept are the answer.
pub fn claim(c: &Connection, uid: i64, uuid: &str, names: &[String], at: i64) -> Vec<String> {
    let mods = cheats_among(c, names);
    if mods.is_empty() {
        let _ = c.execute("DELETE FROM cheat_marks WHERE uid = ?1 AND mc_uuid = ?2", params![uid, uuid]);
        return mods;
    }
    let _ = c.execute(
        "INSERT INTO cheat_marks (uid, mc_uuid, mods, updated) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(uid, mc_uuid) DO UPDATE SET mods = ?3, updated = ?4",
        params![uid, uuid, json!(mods).to_string(), at],
    );
    mods
}

/// The claims on these UUIDs that still hold: `{uuid: [{mods}]}`, one for each account that claims it.
pub fn claims_of(c: &Connection, uuids: &[String], at: i64) -> HashMap<String, Value> {
    let mut out = HashMap::new();
    let Ok(mut st) = c.prepare("SELECT mods FROM cheat_marks WHERE mc_uuid = ?1 AND updated > ?2") else { return out };
    for u in uuids {
        let rows: Vec<Value> = st
            .query_map(params![u, at - FRESH_MS], |r| r.get::<_, String>(0))
            .map(|m| m.flatten().map(|mods| json!({ "mods": serde_json::from_str::<Value>(&mods).unwrap_or(json!([])) })).collect())
            .unwrap_or_default();
        if !rows.is_empty() {
            out.insert(u.clone(), Value::Array(rows));
        }
    }
    out
}

#[derive(Deserialize)]
struct Claim {
    uuid: String,
    #[serde(default)]
    mods: Vec<String>,
}

async fn post_claim(State(app): State<Shared>, h: HeaderMap, Json(b): Json<Claim>) -> Response {
    let who = match member(&app, &h, false).await {
        Ok(w) => w,
        Err(no) => return no,
    };
    let Some(uuid) = uuid_of(&b.uuid) else { return crate::fail(StatusCode::BAD_REQUEST, "uuid") };
    let names: Vec<String> = b.mods.into_iter().take(32).map(|n| clean_line(&n, 48)).collect();
    let uid = who.uid;
    let mods = crate::db(&app, move |c| claim(c, uid, &uuid, &names, millis())).await;
    Json(json!({ "mods": mods })).into_response()
}

#[derive(Deserialize)]
struct Asked {
    /// The UUIDs, by commas.
    u: String,
}

async fn get_claims(State(app): State<Shared>, h: HeaderMap, Query(q): Query<Asked>) -> Response {
    if let Err(no) = member(&app, &h, false).await {
        return no;
    }
    let uuids: Vec<String> = q.u.split(',').filter_map(uuid_of).take(64).collect();
    let found = crate::db(&app, move |c| claims_of(c, &uuids, millis())).await;
    let mut res = Json(json!({ "marks": found })).into_response();
    res.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    res
}

pub fn routes() -> Router<Shared> {
    Router::new().route("/api/marks", post(post_claim).get(get_claims))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> (Connection, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("cursed-marks-{}", crate::random_hex(4)));
        std::fs::create_dir_all(&dir).unwrap();
        let c = crate::open(dir.join("x.db").to_str().unwrap());
        c.execute(
            "INSERT INTO users (uid, login, login_key, email, email_key, pass, created) VALUES (5, 'a', 'a', 'a@x', 'a@x', 'x', 0), (6, 'b', 'b', 'b@x', 'b@x', 'x', 0)",
            [],
        )
        .unwrap();
        (c, dir)
    }

    #[test]
    fn the_first_rules_are_there_once_and_a_cheat_is_known_by_its_name() {
        let (c, dir) = db();
        let all = rules(&c);
        let meteor = all.iter().find(|r| r["name"] == "Meteor Client").unwrap();
        assert_eq!((meteor["cheat"].clone(), meteor["ids"][0].clone()), (json!(true), json!("meteor-client")));
        let replay = all.iter().find(|r| r["name"] == "ReplayMod").unwrap();
        assert_eq!((replay["cheat"].clone(), replay["on"].clone()), (json!(false), json!(["FunTime", "HolyTime"])));
        // Opened again: not seeded twice, and what she took off stays off.
        remove_rule(&c, "wurst").unwrap();
        schema(&c);
        assert_eq!(rules(&c).len(), all.len() - 1);
        assert!(remove_rule(&c, "wurst").is_err());
        assert!(set_rule(&c, "Nothing", "", "", "", "t").is_err());
        drop(c);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_claim_holds_for_a_while_names_only_cheats_and_is_each_accounts_own() {
        let (c, dir) = db();
        let u = "0123456789abcdef0123456789abcdef".to_string();
        let names = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        // Not a cheat, or nothing at all: no claim.
        assert!(claim(&c, 5, &u, &names(&["ReplayMod", "AppleSkin"]), 1000).is_empty());
        assert_eq!(claim(&c, 5, &u, &names(&["meteor client", "Baritone", "AppleSkin"]), 1000), ["Baritone", "Meteor Client"]);
        assert_eq!(claim(&c, 5, &u, &names(&["Meteor Client"]), 2000), ["Meteor Client"]);
        // Another account claims the same UUID: both are kept, neither takes the other's away.
        claim(&c, 6, &u, &names(&["Wurst"]), 2000);
        let found = claims_of(&c, &[u.clone()], 3000);
        assert_eq!(found[&u].as_array().unwrap().len(), 2);
        assert!(found[&u].as_array().unwrap().iter().any(|m| m["mods"] == json!(["Meteor Client"])));
        assert!(found[&u].as_array().unwrap().iter().any(|m| m["mods"] == json!(["Wurst"])));
        // Not said again for a quarter of an hour: gone.
        assert!(claims_of(&c, &[u.clone()], 2000 + FRESH_MS + 1).is_empty());
        // A claim of no cheat takes the account's own off.
        assert!(claim(&c, 6, &u, &[], 4000).is_empty());
        assert_eq!(claims_of(&c, &[u.clone()], 4000)[&u].as_array().unwrap().len(), 1);
        drop(c);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
