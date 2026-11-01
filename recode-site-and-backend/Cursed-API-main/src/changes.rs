//! What is new (23.10.2026): the builds' notes as they were published (dist.rs keeps each release's notes), for the
//! site's «What's new»: Recode's and Cursed's, the newest first. Members only, as the page is.

use axum::extract::{Query, State};
use axum::http::HeaderMap;
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
    /// "recode", "beta" (Cursed 1.21.11), or nothing: both.
    #[serde(default)]
    channel: Option<String>,
}

async fn list(State(app): State<Shared>, h: HeaderMap, Query(q): Query<Asked>) -> Response {
    if let Err(no) = member(&app, &h, false).await {
        return no;
    }
    let channel = q.channel.filter(|c| c == "recode" || c == "beta");
    let items = crate::db(&app, move |c| -> Vec<Value> {
        let sql = "SELECT r.channel, r.build, r.version, r.notes, r.at FROM releases r
                     WHERE r.channel IN ('recode', 'beta') AND (?1 IS NULL OR r.channel = ?1)
                       AND r.id <= (SELECT ch.release FROM channels ch WHERE ch.name = r.channel)
                     ORDER BY r.build DESC LIMIT 80";
        let Ok(mut st) = c.prepare(sql) else { return Vec::new() };
        let rows = st.query_map(params![channel], |r| {
            Ok(json!({ "channel": r.get::<_, String>(0)?, "build": r.get::<_, i64>(1)?, "version": r.get::<_, String>(2)?, "notes": r.get::<_, String>(3)?, "at": r.get::<_, i64>(4)? }))
        });
        rows.map(|r| r.flatten().collect()).unwrap_or_default()
    })
    .await;
    Json(json!({ "items": items })).into_response()
}

pub fn routes() -> Router<Shared> {
    Router::new().route("/api/changes", get(list))
}
