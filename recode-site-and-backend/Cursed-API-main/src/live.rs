//! The live line: one stream for each signed-in page and each running game (Server-Sent Events, `GET /api/live`),
//! carrying what happens to its member as it happens — notifications, messages, someone typing, a friend coming
//! online. The site opens it with its cookie, the game with its token (`?kind=game`).
//!
//! It also knows who is here: on the site, in the game (Cursed 1.21.11 or Recode), what they listen to in the client
//! and, if they let it be shown, which server they play on (`POST /api/me/presence`) — and so who of ours plays on a
//! server (`GET /api/game/ours?server=`: the clients mark them). Time in the game is counted from the game's own
//! stream (for the badges of hours), a minute at a time.
//!
//! Everything here lives in memory: a restart of the server is a reconnect for every stream (the site and the game
//! come back by themselves within seconds).

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::market::{clean_line, member, said};
use crate::{db, now, Shared};

mod games;

/// Streams a member may hold at once (pages and games together).
const MOST_STREAMS: usize = 8;
/// Events waiting for a slow stream before it is dropped (it reconnects and reads what it missed from the lists).
const QUEUE: usize = 64;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Site,
    Game,
}

struct Sub {
    id: u64,
    kind: Kind,
    tx: mpsc::Sender<String>,
}

/// What is known of someone who is here.
#[derive(Clone, Default, Debug)]
pub struct Presence {
    pub site: usize,
    pub game: usize,
    /// When the game's time was last counted (seconds).
    pub game_counted: i64,
    pub listening: Option<Value>,
    pub server: Option<String>,
    /// The nick the game plays under (shown to friends as the server is).
    pub mc: Option<String>,
    pub since: i64,
    /// The client's build (the loader's number), for the staff's summary.
    pub build: Option<i64>,
    /// Which client plays: "cursed" (Cursed 1.21.11, what was before) or "recode".
    pub client: Option<String>,
}

struct Hub {
    next: AtomicU64,
    subs: Mutex<HashMap<i64, Vec<Sub>>>,
    here: Mutex<HashMap<i64, Presence>>,
    games: Mutex<games::Games>,
}

fn hub() -> &'static Hub {
    static HUB: OnceLock<Hub> = OnceLock::new();
    HUB.get_or_init(|| Hub { next: AtomicU64::new(1), subs: Mutex::new(HashMap::new()), here: Mutex::new(HashMap::new()), games: Mutex::new(games::Games::default()) })
}

/// An event to every stream of a member (pages and games); a stream too far behind is let go.
pub fn publish(uid: i64, event: &Value) {
    publish_to(uid, None, event);
}

/// An event to the streams of one kind only.
pub fn publish_to(uid: i64, kind: Option<Kind>, event: &Value) {
    let text = event.to_string();
    let mut subs = hub().subs.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(list) = subs.get_mut(&uid) {
        list.retain(|s| {
            if kind.is_some_and(|k| k != s.kind) {
                return true;
            }
            !matches!(s.tx.try_send(text.clone()), Err(mpsc::error::TrySendError::Closed(_)) | Err(mpsc::error::TrySendError::Full(_)))
        });
    }
}

/// Everyone with a stream open now: (site, game) counts, and the game's builds with how many run each.
pub fn census() -> (usize, usize, Vec<(Option<i64>, usize)>) {
    let here = hub().here.lock().unwrap_or_else(|e| e.into_inner());
    let (mut site, mut game) = (0, 0);
    let mut builds: HashMap<Option<i64>, usize> = HashMap::new();
    for p in here.values() {
        if p.site > 0 {
            site += 1;
        }
        if p.game > 0 {
            game += 1;
            *builds.entry(p.build).or_default() += 1;
        }
    }
    let mut builds: Vec<(Option<i64>, usize)> = builds.into_iter().collect();
    builds.sort_by(|a, b| b.1.cmp(&a.1));
    (site, game, builds)
}

/// Whether a member has any stream open now.
pub fn online(uid: i64) -> bool {
    hub().here.lock().unwrap_or_else(|e| e.into_inner()).get(&uid).is_some_and(|p| p.site + p.game > 0)
}

pub fn presence(uid: i64) -> Option<Presence> {
    hub().here.lock().unwrap_or_else(|e| e.into_inner()).get(&uid).filter(|p| p.site + p.game > 0).cloned()
}

/// Who is here now, and where.
pub fn everyone_here() -> Vec<(i64, Presence)> {
    hub().here.lock().unwrap_or_else(|e| e.into_inner()).iter().filter(|(_, p)| p.site + p.game > 0).map(|(u, p)| (*u, p.clone())).collect()
}

/// The stream's end, whenever and however it ends (the page closed, the game quit, the connection lost).
struct Leave {
    uid: i64,
    id: u64,
    kind: Kind,
    session: Vec<u8>,
}

impl Drop for Leave {
    fn drop(&mut self) {
        {
            let mut subs = hub().subs.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(list) = subs.get_mut(&self.uid) {
                list.retain(|s| s.id != self.id);
                if list.is_empty() {
                    subs.remove(&self.uid);
                }
            }
        }
        let gone = {
            // Lock order everywhere: game sessions, then the aggregate account presence.
            let mut games = hub().games.lock().unwrap_or_else(|e| e.into_inner());
            if self.kind == Kind::Game {
                games.close(self.uid, &self.session);
            }
            let mut here = hub().here.lock().unwrap_or_else(|e| e.into_inner());
            let p = here.entry(self.uid).or_default();
            match self.kind {
                Kind::Site => p.site = p.site.saturating_sub(1),
                Kind::Game => p.game = p.game.saturating_sub(1),
            }
            let game_over = self.kind == Kind::Game && p.game == 0;
            let changed = self.kind == Kind::Game && games::reflect(p, games.latest(self.uid));
            let all_gone = p.site + p.game == 0;
            (game_over || changed, all_gone)
        };
        if gone.0 || gone.1 {
            crate::people::presence_changed(self.uid);
        }
    }
}

#[derive(Deserialize)]
struct Open {
    kind: Option<String>,
}

async fn open(State(app): State<Shared>, h: HeaderMap, Query(q): Query<Open>) -> Response {
    let w = match member(&app, &h, false).await {
        Ok(w) => w,
        Err(r) => return r,
    };
    let kind = if q.kind.as_deref() == Some("game") { Kind::Game } else { Kind::Site };
    let uid = w.uid;
    let (tx, rx) = mpsc::channel::<String>(QUEUE);
    let id = hub().next.fetch_add(1, Ordering::Relaxed);
    {
        let mut subs = hub().subs.lock().unwrap_or_else(|e| e.into_inner());
        let list = subs.entry(uid).or_default();
        if list.len() >= MOST_STREAMS {
            // The oldest makes room (a page left open somewhere and forgotten).
            list.remove(0);
        }
        list.push(Sub { id, kind, tx: tx.clone() });
    }
    let came = {
        let mut games = hub().games.lock().unwrap_or_else(|e| e.into_inner());
        if kind == Kind::Game {
            games.open(uid, &w.hash, now());
        }
        let mut here = hub().here.lock().unwrap_or_else(|e| e.into_inner());
        let p = here.entry(uid).or_default();
        let was_here = p.site + p.game > 0;
        let game_was = p.game > 0;
        match kind {
            Kind::Site => p.site += 1,
            Kind::Game => {
                p.game += 1;
                if !game_was {
                    p.game_counted = now();
                }
            }
        }
        if !was_here {
            p.since = now();
        }
        let changed = kind == Kind::Game && games::reflect(p, games.latest(uid));
        !was_here || (kind == Kind::Game && !game_was) || changed
    };
    if came {
        crate::people::presence_changed(uid);
    }
    // Own the lifetime before awaiting the database: a cancelled handshake must release presence too.
    let leave = Leave { uid, id, kind, session: w.hash };
    // First: who the stream belongs to, and how many notifications wait unread.
    let hello = db(&app, move |c| json!({ "t": "hello", "me": crate::people::card_of(c, uid), "unread": crate::notify::unread(c, uid) })).await;
    let _ = tx.try_send(hello.to_string());
    let stream = futures_util::stream::unfold((rx, leave, app), |(mut rx, leave, app)| async move {
        loop {
            // Check before each event, and close idle revoked streams within 15 seconds too.
            let received = tokio::time::timeout(Duration::from_secs(15), rx.recv()).await;
            let hash = leave.session.clone();
            if !db(&app, move |c| crate::session_alive(c, &hash)).await { return None; }
            match received {
                Ok(Some(text)) => return Some((Ok::<Event, Infallible>(Event::default().data(text)), (rx, leave, app))),
                Ok(None) => return None,
                Err(_) => {}
            }
        }
    });
    let mut res = Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(20)).text("·")).into_response();
    // nginx gives every event at once (no buffering of this answer).
    res.headers_mut().insert("X-Accel-Buffering", axum::http::HeaderValue::from_static("no"));
    res
}

#[derive(Deserialize)]
struct Here {
    /// What the client plays now: {"title", "artist", "source"} or null.
    listening: Option<Value>,
    /// The server the game is on, its address as typed (null in the menu); shown only if its member lets it be.
    server: Option<String>,
    /// The client's build.
    build: Option<i64>,
    /// The nick the game plays under.
    #[serde(default)]
    mc: Option<String>,
    /// Which client it is ("recode"; none: Cursed 1.21.11).
    #[serde(default)]
    client: Option<String>,
}

/// The game says what it is doing (when it changes): what it listens to, where it plays.
async fn here(State(app): State<Shared>, h: HeaderMap, Json(b): Json<Here>) -> Response {
    let w = match member(&app, &h, false).await {
        Ok(w) => w,
        Err(r) => return r,
    };
    let listening = b.listening.filter(|v| v.is_object()).map(|v| {
        json!({
            "title": clean_line(v["title"].as_str().unwrap_or(""), 120),
            "artist": clean_line(v["artist"].as_str().unwrap_or(""), 120),
            "source": clean_line(v["source"].as_str().unwrap_or(""), 24),
        })
    });
    let listening = listening.filter(|v| !v["title"].as_str().unwrap_or("").is_empty());
    let server = match b.server.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        None => None,
        Some(s) => match games::server_key(s) {
            Some(s) => Some(s),
            None => return said(StatusCode::BAD_REQUEST, "server", "an address of a server"),
        },
    };
    let changed = {
        let mut games = hub().games.lock().unwrap_or_else(|e| e.into_inner());
        games.update(w.uid, &w.hash, now(), Presence {
            listening,
            server,
            mc: b.mc.as_deref().and_then(crate::friends::game_nick),
            client: Some(if b.client.as_deref() == Some("recode") { "recode" } else { "cursed" }.to_string()),
            build: b.build.filter(|n| (1..1_000_000).contains(n)),
            ..Presence::default()
        });
        let mut here = hub().here.lock().unwrap_or_else(|e| e.into_inner());
        here.get_mut(&w.uid).is_some_and(|p| games::reflect(p, games.latest(w.uid)))
    };
    if changed {
        crate::people::presence_changed(w.uid);
    }
    Json(json!({})).into_response()
}

#[derive(Deserialize)]
struct OursOn {
    server: String,
}

/// Who of ours plays on a server (a member asks, from the game).
async fn ours(State(app): State<Shared>, h: HeaderMap, Query(q): Query<OursOn>) -> Response {
    if let Err(r) = member(&app, &h, false).await {
        return r;
    }
    let Some(server) = games::server_key(&q.server) else {
        return said(StatusCode::BAD_REQUEST, "server", "an address of a server");
    };
    let players = hub().games.lock().unwrap_or_else(|e| e.into_inner()).ours(&server);
    Json(json!({ "players": players })).into_response()
}

pub fn routes() -> Router<Shared> {
    Router::new().route("/api/live", get(open)).route("/api/me/presence", post(here)).route("/api/game/ours", get(ours))
}

/// Every minute: the game's time of everyone playing counted into their stats (and so their badges); every five, the
/// cards of everyone made again (roles given from the command line reach the lists).
pub async fn counter(app: Shared) {
    let mut minute = 0u64;
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;
        minute += 1;
        if minute % 5 == 0 {
            // (Recode's roles first: one that ran out goes within five minutes, and the cards made after it show so)
            db(&app, |c| {
                crate::recode::sweep_marks(c);
                crate::people::refresh_all(c)
            })
            .await;
        }
        let at = now();
        hub().games.lock().unwrap_or_else(|e| e.into_inner()).prune(at);
        let playing: Vec<(i64, i64)> = {
            let mut here = hub().here.lock().unwrap_or_else(|e| e.into_inner());
            here.retain(|_, p| p.site + p.game > 0);
            here.iter_mut()
                .filter(|(_, p)| p.game > 0)
                .map(|(uid, p)| {
                    let minutes = (at - p.game_counted) / 60;
                    p.game_counted += minutes * 60;
                    (*uid, minutes)
                })
                .filter(|(_, m)| *m > 0)
                .collect()
        };
        if playing.is_empty() {
            continue;
        }
        db(&app, move |c| {
            for (uid, minutes) in playing {
                crate::people::played(c, uid, minutes, at);
            }
        })
        .await;
    }
}
