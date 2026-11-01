//! The best (23.10.2026): the members first by a count — hours in Cursed, hours in Recode, badges, the forum's posts, the
//! likes on their look — for the site's «Best» page. Hours only of those who show them on their page (privacy "hours");
//! the banned and the forum's AI (role "ai") are not in it. Members only, as the profiles are.

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use rusqlite::params;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::market::member;
use crate::Shared;

#[derive(Deserialize)]
struct Asked {
    by: String,
}

/// Each list's count, as SQL giving (uid, value) for every member who has one.
fn source(by: &str) -> Option<&'static str> {
    Some(match by {
        "hours" => "SELECT s.uid, s.value / 60 FROM stats s LEFT JOIN profiles p ON p.uid = s.uid
                     WHERE s.key = 'game_min' AND s.value >= 60 AND COALESCE(json_extract(p.privacy, '$.hours'), 1) != 0",
        "recode" => "SELECT s.uid, s.value / 60 FROM stats s LEFT JOIN profiles p ON p.uid = s.uid
                      WHERE s.key = 'recode_min' AND s.value >= 60 AND COALESCE(json_extract(p.privacy, '$.hours'), 1) != 0",
        "badges" => "SELECT uid, COUNT(*) FROM user_badges GROUP BY uid",
        "posts" => "SELECT uid, value FROM stats WHERE key = 'posts' AND value > 0",
        "likes" => "SELECT owner, COUNT(*) FROM look_likes GROUP BY owner",
        _ => return None,
    })
}

async fn top(State(app): State<Shared>, h: HeaderMap, Query(q): Query<Asked>) -> Response {
    let who = match member(&app, &h, false).await {
        Ok(w) => w,
        Err(no) => return no,
    };
    let Some(src) = source(&q.by) else { return crate::fail(StatusCode::BAD_REQUEST, "by") };
    let me = who.uid;
    let now = crate::now();
    let out = crate::db(&app, move |c| -> Value {
        let sql = format!(
            "WITH t(uid, value) AS ({src}),
                  ok AS (SELECT t.uid, t.value FROM t JOIN users u ON u.uid = t.uid
                          WHERE t.uid NOT IN (SELECT uid FROM bans WHERE lifted IS NULL AND (until IS NULL OR until > ?1))
                            AND t.uid NOT IN (SELECT uid FROM roles WHERE role = 'ai'))
             SELECT uid, value, RANK() OVER (ORDER BY value DESC) FROM ok ORDER BY value DESC, uid LIMIT 50"
        );
        let rows: Vec<(i64, i64, i64)> = c
            .prepare(&sql)
            .and_then(|mut st| st.query_map(params![now], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).map(|m| m.flatten().collect()))
            .unwrap_or_default();
        let items: Vec<Value> = rows.iter().map(|(uid, value, rank)| json!({ "rank": rank, "value": value, "card": crate::people::card_of(c, *uid) })).collect();
        // where the one asking stands, when they are not among the fifty
        let mine = if rows.iter().any(|r| r.0 == me) {
            Value::Null
        } else {
            let sql = format!(
                "WITH t(uid, value) AS ({src}),
                      ok AS (SELECT t.uid, t.value FROM t JOIN users u ON u.uid = t.uid
                              WHERE t.uid NOT IN (SELECT uid FROM bans WHERE lifted IS NULL AND (until IS NULL OR until > ?1))
                                AND t.uid NOT IN (SELECT uid FROM roles WHERE role = 'ai'))
                 SELECT (SELECT value FROM ok WHERE uid = ?2), (SELECT COUNT(*) + 1 FROM ok WHERE value > (SELECT value FROM ok WHERE uid = ?2))"
            );
            c.query_row(&sql, params![now, me], |r| Ok((r.get::<_, Option<i64>>(0)?, r.get::<_, i64>(1)?)))
                .ok()
                .and_then(|(v, rank)| v.map(|v| json!({ "rank": rank, "value": v })))
                .unwrap_or(Value::Null)
        };
        json!({ "items": items, "me": mine })
    })
    .await;
    Json(out).into_response()
}

pub fn routes() -> Router<Shared> {
    Router::new().route("/api/top", get(top))
}
