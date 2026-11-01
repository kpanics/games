//! Presence has its own worker: unsupported cloud formats, local file recovery and
//! held binds must not stop the client's online badges.
use super::{auth, call, same_auth, stream, Auth};
use crate::client_marks::{self, Client, Scope};
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

pub(super) static OPENED: AtomicU64 = AtomicU64::new(0);

#[derive(Default)]
pub(super) struct Presence {
    last: Option<(Scope, u64, Instant)>,
}
impl Presence {
    pub(super) fn tick(&mut self, a: &Auth) {
        let scope = client_marks::scope();
        let opened = OPENED.load(Ordering::Relaxed);
        if self.last.as_ref().is_some_and(|(s, o, at)| {
            *s == scope && *o == opened && at.elapsed() < Duration::from_secs(10)
        }) {
            return;
        }
        self.last = Some((scope.clone(), opened, Instant::now()));
        let build = option_env!("CURSED_BUILD").and_then(|v| v.parse::<i64>().ok());
        let _ = call(
            a,
            "POST",
            "/api/me/presence",
            Some(
                &json!({"server":scope.server,"mc":scope.nick,"client":"recode","build":build,"listening":null}),
            ),
        );
        if let Some(host) = &scope.server {
            if let Ok(v) = call(a, "GET", &format!("/api/game/ours?server={host}"), None) {
                let players: Vec<_> = v["players"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .take(10000)
                    .filter_map(|p| {
                        Some((
                            p["mc"].as_str()?.to_string(),
                            Client::parse(p["client"].as_str()?)?,
                        ))
                    })
                    .collect();
                if same_auth(a) {
                    client_marks::replace(&scope, &players);
                }
            }
        }
    }
}

pub(super) fn run() {
    let mut previous: Option<Auth> = None;
    let mut presence = Presence::default();
    loop {
        std::thread::sleep(Duration::from_secs(1));
        let Some(a) = auth() else {
            if previous.take().is_some() {
                client_marks::clear();
            }
            continue;
        };
        if previous
            .as_ref()
            .is_none_or(|p| p.api != a.api || p.token != a.token)
        {
            client_marks::clear();
            previous = Some(a.clone());
            presence = Presence::default();
            stream(a.clone());
        }
        presence.tick(&a);
    }
}
