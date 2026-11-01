//! What the chat holds: every message as it came from Java (its runs), read into rows the way the client reads a
//! server (parse.rs), the open chat's field and hints as Java has them, which channel and conversation is shown, the
//! players hidden (kept on the disk), the events pinned. Drawing is view.rs's; what a row shows is body.rs's.

use super::body::Laid;
use super::parse::rich::{self, Span};
use super::parse::{self, Kind as Said, Server};
use std::collections::HashMap;
use std::sync::Mutex;

pub const F_BOLD: u32 = 1;
pub const F_ITALIC: u32 = 2;
pub const F_UNDER: u32 = 4;
pub const F_STRIKE: u32 = 8;
pub const F_OBF: u32 = 16;
/// a pack's glyphs (or a font of the server's): Java draws them, `gw` wide in the game's units
pub const F_GLYPH: u32 = 32;
pub const F_CLICK: u32 = 64;
pub const F_ITEM: u32 = 128;
/// no colour of its own (the chat's)
pub const F_PLAIN: u32 = 256;

#[derive(Clone, Default)]
pub struct Run {
    pub text: String,
    pub col: u32,
    pub fl: u32,
    pub gw: f32,
    pub hover: String,
}

/// A message as it came, waiting with those that came with it (a server sends an event's lines one message each, in
/// the same moment): Java's guess of whose it is (0 a server's, 1 a player's, 2 private to you, 3 yours to someone).
#[derive(Clone, Default)]
struct In {
    id: i32,
    clock: i32,
    guess: u8,
    sender: String,
    runs: Vec<Run>,
}

/// The channels of the open chat (its tabs).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Tab {
    #[default]
    All,
    Players,
    Private,
    Server,
    Mentions,
    /// the player's own (0..3): what holds its words
    Own(u8),
}

impl Tab {
    pub const ALL: [Tab; 8] = [Tab::All, Tab::Players, Tab::Private, Tab::Server, Tab::Mentions, Tab::Own(0), Tab::Own(1), Tab::Own(2)];
    pub fn name(self) -> &'static str {
        match self {
            Tab::All => "All",
            Tab::Players => "Players",
            Tab::Private => "Private",
            Tab::Server => "Server",
            Tab::Mentions => "Mentions",
            Tab::Own(_) => "",
        }
    }
    /// Its icon (gui.comp chatIcon; the player's own: the search's glass).
    pub fn icon(self) -> u32 {
        match self {
            Tab::All => 38,
            Tab::Players => 35,
            Tab::Private => 36,
            Tab::Server => 37,
            Tab::Mentions => 31,
            Tab::Own(_) => 13,
        }
    }
    /// Its word in the file of their order.
    fn key(self) -> String {
        match self {
            Tab::Own(i) => format!("own{}", i + 1),
            t => t.name().to_lowercase(),
        }
    }
}

/// A row of the chat: what the server's lines were read as.
pub struct Row {
    pub id: i32,
    pub clock: i32,
    pub kind: Said,
    pub lines: Vec<(i32, Vec<Span>)>,
    /// 1 a player's, 2 private, 3 the server's (events too)
    pub cat: u8,
    pub mention: bool,
    /// the player it is of (a face, a card, hiding); `mine`: you said it
    pub who: String,
    pub mine: bool,
    pub plain: String,
    pub key: Option<String>,
    pub count: u32,
    pub born: f32,
    pub bump: f32,
    /// a request's, a teleport's, a restart's time: when it runs out, how long it was
    pub until: f32,
    pub secs: f32,
    /// 0 waiting, 1 accepted, 2 declined, 3 done (a teleport made, a restart called off)
    pub state: u8,
    /// a private message seen in its conversation
    pub read: bool,
    /// what changed in it since it was laid out (a repeat counted, an answer given, its time out)
    pub version: u32,
    /// its layout, made for a width and a look (its key): made again only when one of them changes
    pub laid: Option<(u64, Laid)>,
}

/// An event pinned over the open chat while it lasts.
#[derive(Clone)]
pub struct Pin {
    pub key: String,
    pub name: String,
    pub xyz: String,
    pub until: f32,
    pub born: f32,
}

/// What a press means (view.rs puts them where it draws).
#[derive(Clone, PartialEq, Debug)]
pub enum Hit {
    Tab(Tab),
    Peer(String),
    /// 0 the search, 1 the settings
    Tool(u8),
    /// one of the hidden brought back
    Unhide(String),
    Tap(Tap),
    Nick(String),
    /// a row's action: 0 reply, 1 mention, 2 copy, 3 hide, 4 its writer's name copied, 5 the chat's settings; the
    /// row's id
    Act(u8, i32),
    /// a card's button: its row's id, 0 accept, 1 decline
    Btn(i32, u8),
    Sugg(usize),
    Pill,
    /// the player's card: 0 write, 1 mention, 2 teleport, 3 hide or show
    Card(u8),
    Chan,
    Undo(String),
    /// the window's corner, to stretch it by; the field (where the caret goes)
    Grip,
    Field,
    /// a row (a right click opens its menu)
    Row(i32),
    Panel,
    /// the header's title with no left column: the channels' list; a channel picked from it
    Chans,
    Pick(Tab),
}

/// What a click on a chip does: puts words in the field, copies them, or the server's own click (a message's run).
#[derive(Clone, PartialEq, Debug)]
pub enum Tap {
    Field(String),
    Copy(String, &'static str),
    Game(i32, usize),
}

#[derive(Default)]
pub struct Store {
    pub rows: Vec<Row>,
    batch: Vec<In>,
    batch_at: f32,
    last_in: f32,
    pub runs: HashMap<i32, Vec<Run>>,
    // what Java says each frame
    pub open: bool,
    pub field: String,
    pub cursor: usize,
    pub anchor: usize,
    pub ghost: String,
    pub sugg: Vec<String>,
    pub sugg_sel: i32,
    pub sugg_off: usize,
    pub usage: Vec<String>,
    pub me: String,
    pub server: String,
    pub mouse: [f32; 2],
    // what is shown
    pub tab: Tab,
    pub dm: Option<String>,
    pub scroll: f32,
    pub scroll_t: f32,
    pub unread: u32,
    /// how many private messages and mentions were there when their tab was last seen
    pub seen: [usize; 2],
    pub find: bool,
    pub pins: Vec<Pin>,
    /// the player's card: whose, where (by the name clicked), since when
    pub card: Option<(String, [f32; 2], f32)>,
    /// a message's menu: its row's id, where it was pressed (the chat's px), when
    pub menu: Option<(i32, [f32; 2], f32)>,
    /// the hidden players' list open (2), the channels' (3: the header's, with no left column)
    pub pop: u8,
    /// where the channels' list opens (under the header's title)
    pub chans_at: [f32; 2],
    /// the channels shown besides «All» (players, private, server, mentions); the player's own: their names and words
    pub shown: [bool; 4],
    pub own: Vec<(String, Vec<String>)>,
    /// the channels' order as the player dragged them (kept in cursed-recode/chat-channels.txt)
    pub order: Vec<Tab>,
    order_file: Option<std::path::PathBuf>,
    /// the channels' rows as last drawn: their left, the first's top, their right, a row's height (the chat's px)
    pub tabs_at: [f32; 4],
    /// a channel held: which, where it was pressed (y), whether it moved (being dragged)
    pub tab_drag: Option<(Tab, f32, bool)>,
    /// the command hints open under the field (it is at the window's top)
    pub hints_down: bool,
    /// the world behind the open window: 0 as it is, 1 darkened, 2 blurred; how much (as far as it is open); when the
    /// chat said it
    pub behind: (usize, f32, f32),
    /// a divider over a message after so long a quiet (s): the player's lines part there too
    pub pause: Option<f32>,
    /// a word for a moment (copied, hidden…) and since when; a hidden player to bring back
    pub toast: (String, f32),
    pub undo: Option<String>,
    /// the players whose messages are not shown (kept in cursed-recode/chat-hidden.txt), loaded once
    pub hidden: Vec<String>,
    hidden_file: Option<std::path::PathBuf>,
    /// the faces Java draws this frame (icon 23 by its place here); who is in the game (lowercase: only they have one)
    pub faces: Vec<String>,
    /// the faces' shape: 0 square, 1 their corners rounded, 2 round
    pub face_shape: usize,
    pub online: std::collections::HashSet<String>,
    /// the frame's hits (in the element's own px), its whole, the shift from its corner to the window
    pub hits: Vec<(Hit, [f32; 4])>,
    pub panel: [f32; 4],
    pub shift: [f32; 2],
    pub now: f32,
    pub max: usize,
    pub fold: bool,
    /// the window being stretched: the pointer then, its size then (the chat's own px)
    pub grip: Option<([f32; 2], [f32; 2])>,
    /// the scroll bar seen (it shows while the list moves, or the pointer is near)
    pub bar_at: f32,
    /// a conversation's words just sent: to whom, when (what the server says next is its answer)
    pub sent: Option<(String, f32)>,
    /// the server's answer to words that did not reach them (no echo of them came first): to whom, its words, when
    pub note: Option<(String, String, f32)>,
    /// what was typed and not sent, by where it was typed: a conversation (its name, lowercase) or "" (the chat)
    pub drafts: HashMap<String, String>,
    /// the field as it was when the search took it (Java keeps it aside; it comes back when the search closes)
    pub stash: String,
    /// the field's words scrolled out to the left (px), as last drawn
    pub field_off: f32,
    /// the private messages to you and the mentions that ever came (the rows go once there are too many; these do not)
    pub priv_total: usize,
    pub ment_total: usize,
}

/// The chat's store (a panic while it was held does not leave the chat broken: what is there is used).
pub fn store() -> std::sync::MutexGuard<'static, Store> {
    STORE.lock().unwrap_or_else(|e| e.into_inner())
}

static STORE: std::sync::LazyLock<Mutex<Store>> =
    std::sync::LazyLock::new(|| Mutex::new(Store { sugg_sel: -1, max: 500, fold: true, toast: (String::new(), -9.0), ..Default::default() }));

/// "chatadd": id, clock, kind, mention, sender, then runs (text, colour, flags, glyphs' width, hover) — fields by
/// \u{1}, runs by \u{2}, a run's fields by \u{3}. Kept with those that come with it, read a moment later.
pub fn add(payload: &str, stack: bool) {
    let mut f = payload.split('\u{1}');
    let id = f.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let clock = f.next().and_then(|v| v.parse().ok()).unwrap_or(-1);
    let guess = f.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let _mention = f.next();
    let sender = f.next().unwrap_or("").to_string();
    let mut runs = Vec::new();
    for r in f.next().unwrap_or("").split('\u{2}') {
        if r.is_empty() {
            continue;
        }
        let mut p = r.split('\u{3}');
        let text = p.next().unwrap_or("").to_string();
        let col = u32::from_str_radix(p.next().unwrap_or("FFFFFF"), 16).unwrap_or(0xFFFFFF);
        let fl = p.next().and_then(|v| v.parse().ok()).unwrap_or(0);
        let gw = p.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
        let hover = p.next().unwrap_or("").to_string();
        runs.push(Run { text, col, fl, gw, hover });
    }
    let mut s = store();
    s.fold = stack;
    let now = s.now;
    if s.batch.is_empty() {
        s.batch_at = now;
    }
    s.last_in = now;
    s.runs.insert(id, runs.clone());
    s.batch.push(In { id, clock, guess, sender, runs });
}

pub fn clear() {
    let mut s = store();
    s.rows.clear();
    s.batch.clear();
    s.runs.clear();
    s.pins.clear();
    s.scroll = 0.0;
    s.scroll_t = 0.0;
    s.unread = 0;
    s.seen = [0, 0];
}

/// "chatserver": the server's address and your name (whose formats the chat reads, your name lit).
pub fn server(payload: &str) {
    let mut f = payload.split('\t');
    let mut s = store();
    s.server = f.next().unwrap_or("").to_string();
    if let Some(me) = f.next().filter(|m| !m.is_empty()) {
        s.me = me.to_string();
    }
}

/// "chatfind": the search on (1) or off; the field is its words while it is on (Java keeps what was typed aside).
pub fn find(on: bool) {
    let mut s = store();
    if on && !s.find {
        s.stash = s.field.clone();
    }
    s.find = on;
    s.scroll_t = 0.0;
}

/// "chatonline": the players in the game, by tabs (only they have a face to draw).
pub fn online(payload: &str) {
    let mut s = store();
    s.online = payload.split('\t').filter(|n| !n.is_empty()).map(|n| n.to_lowercase()).collect();
}

/// "chatfaces": the faces Java draws this frame, by their place (icon 23).
pub fn faces() -> String {
    store().faces.join("\n")
}

/// "chatto": whom the open conversation is with ("" none): Enter's words go to them (Java reads its field as it is
/// now, the command not being typed).
pub fn talking_to() -> String {
    let s = store();
    if s.open && !s.find && s.tab == Tab::Private { s.dm.clone().unwrap_or_default() } else { String::new() }
}

/// "chatsent": words sent to them in their conversation (what the server says next is its answer).
pub fn sent(to: &str) {
    let mut s = store();
    let now = s.now;
    s.sent = Some((to.to_string(), now));
    s.note = None;
}

/// Esc while the chat is open: what is open over it closes first (a card, the settings, the search).
/// The list of the hidden players over the open window (the chat's settings' button).
pub fn show_hidden() {
    let mut s = store();
    if s.open {
        s.pop = 2;
    }
}

pub fn escape() -> bool {
    let mut s = store();
    if s.open && (s.card.is_some() || s.pop != 0 || s.menu.is_some()) {
        s.card = None;
        s.pop = 0;
        s.menu = None;
        return true;
    }
    false
}

/// "chatin": open, the field's words, its caret and the selection's other end (chars), the hint's ghost, you, the
/// command hints (\u{2}), the chosen one, the first shown, the command's usage lines (\u{2}), the pointer.
pub fn input(payload: &str) {
    let mut f = payload.split('\u{1}');
    let mut s = store();
    let was = s.open;
    s.open = f.next() == Some("1");
    if s.open {
        s.field = f.next().unwrap_or("").to_string();
        s.cursor = f.next().and_then(|v| v.parse().ok()).unwrap_or(0);
        s.anchor = f.next().and_then(|v| v.parse().ok()).unwrap_or(0);
        s.ghost = f.next().unwrap_or("").to_string();
        if let Some(me) = f.next().filter(|m| !m.is_empty()) {
            s.me = me.to_string();
        }
        s.sugg = f.next().unwrap_or("").split('\u{2}').filter(|v| !v.is_empty()).map(|v| v.to_string()).collect();
        s.sugg_sel = f.next().and_then(|v| v.parse().ok()).unwrap_or(-1);
        s.sugg_off = f.next().and_then(|v| v.parse().ok()).unwrap_or(0);
        s.usage = f.next().unwrap_or("").split('\u{2}').filter(|v| !v.is_empty()).map(|v| v.to_string()).collect();
        let mx = f.next().and_then(|v| v.parse().ok()).unwrap_or(-1.0);
        let my = f.next().and_then(|v| v.parse().ok()).unwrap_or(-1.0);
        s.mouse = [mx, my];
    }
    if was && !s.open {
        s.scroll_t = 0.0;
        s.unread = 0;
        s.card = None;
        s.menu = None;
        s.pop = 0;
        s.find = false;
        s.grip = None;
        s.tab_drag = None;
    }
}

// ------------------------------------------------------------------------------------------------ the hidden

impl Store {
    /// The hidden players from the disk, once (the folder is the menu's).
    pub fn load_hidden(&mut self, dir: Option<&std::path::Path>) {
        if self.hidden_file.is_some() {
            return;
        }
        let Some(d) = dir else { return };
        let f = d.join("cursed-recode").join("chat-hidden.txt");
        if let Ok(text) = std::fs::read_to_string(&f) {
            self.hidden = text.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect();
        }
        self.hidden_file = Some(f);
    }

    /// The channels shown and the player's own, as set; the order dragged, read once. When the channel shown is no
    /// more, «All» is: where the field wrote before (its draft goes back there), or None.
    pub fn set_channels(&mut self, shown: [bool; 4], own: Vec<(String, Vec<String>)>) -> Option<String> {
        self.shown = shown;
        self.own = own;
        if self.order_file.is_none() {
            if let Some(f) = self.hidden_file.as_ref().and_then(|h| h.parent()).map(|d| d.join("chat-channels.txt")) {
                if let Ok(text) = std::fs::read_to_string(&f) {
                    self.order = text.lines().filter_map(|l| Tab::ALL.iter().copied().find(|t| t.key() == l.trim())).collect();
                }
                self.order_file = Some(f);
            }
        }
        // (a conversation opened is shown though the private channel is not listed)
        let in_dm = self.tab == Tab::Private && self.dm.is_some();
        if !in_dm && !self.tabs().contains(&self.tab) {
            let was = match (&self.tab, &self.dm) {
                (Tab::Private, Some(d)) => d.to_lowercase(),
                _ => String::new(),
            };
            self.tab = Tab::All;
            self.dm = None;
            return Some(was);
        }
        None
    }

    /// The channels shown, in their order: «All» always, the others as set, the player's own that have a name.
    pub fn tabs(&self) -> Vec<Tab> {
        let mut v: Vec<Tab> = self.order.clone();
        for t in Tab::ALL {
            if !v.contains(&t) {
                v.push(t);
            }
        }
        v.retain(|t| match t {
            Tab::All => true,
            Tab::Players => self.shown[0],
            Tab::Private => self.shown[1],
            Tab::Server => self.shown[2],
            Tab::Mentions => self.shown[3],
            Tab::Own(i) => self.own.get(*i as usize).is_some_and(|o| !o.0.is_empty()),
        });
        v
    }

    /// A channel's name as shown (the player's own: as they named it).
    pub fn tab_name(&self, t: Tab) -> String {
        match t {
            Tab::Own(i) => self.own.get(i as usize).map(|o| o.0.clone()).unwrap_or_default(),
            t => crate::gui::i18n::tr(t.name()).to_string(),
        }
    }

    /// The channel `t` dragged to the place of the shown one `to` (its index among the shown); kept.
    pub fn move_tab(&mut self, t: Tab, to: usize) {
        let mut shown = self.tabs();
        let Some(from) = shown.iter().position(|q| *q == t) else { return };
        let to = to.min(shown.len() - 1);
        if from == to {
            return;
        }
        shown.remove(from);
        shown.insert(to, t);
        // the hidden ones keep their places after the shown
        let mut all = shown;
        for q in self.order.iter().chain(Tab::ALL.iter()) {
            if !all.contains(q) {
                all.push(*q);
            }
        }
        self.order = all;
    }

    pub fn save_order(&self) {
        if let Some(f) = &self.order_file {
            let _ = std::fs::create_dir_all(f.parent().unwrap());
            let _ = std::fs::write(f, self.order.iter().map(|t| t.key()).collect::<Vec<_>>().join("\n") + "\n");
        }
    }

    fn save_hidden(&self) {
        if let Some(f) = &self.hidden_file {
            let _ = std::fs::create_dir_all(f.parent().unwrap());
            let _ = std::fs::write(f, self.hidden.join("\n") + "\n");
        }
    }

    /// The conversation the field writes to: one open, not searching, and not a command being typed.
    pub fn talking_to(&self) -> Option<String> {
        if !self.open || self.find || self.tab != Tab::Private || self.field.starts_with('/') {
            return None;
        }
        self.dm.clone()
    }

    /// Whether they are in the game (a face to draw).
    pub fn has_face(&self, who: &str) -> bool {
        !who.is_empty() && self.online.contains(&who.to_lowercase())
    }

    pub fn is_hidden(&self, who: &str) -> bool {
        !who.is_empty() && self.hidden.iter().any(|h| h.eq_ignore_ascii_case(who))
    }

    /// Their messages hidden (never yours).
    pub fn hide(&mut self, who: &str) -> bool {
        if who.is_empty() || who.eq_ignore_ascii_case(&self.me) || self.is_hidden(who) {
            return false;
        }
        self.hidden.push(who.to_string());
        self.save_hidden();
        true
    }

    pub fn unhide(&mut self, who: &str) {
        self.hidden.retain(|h| !h.eq_ignore_ascii_case(who));
        self.save_hidden();
    }
}

// ------------------------------------------------------------------------------------------------ reading

fn has_word(hay: &str, word: &str) -> bool {
    if word.is_empty() {
        return false;
    }
    let mut from = 0;
    while let Some(i) = hay[from..].find(word) {
        let a = from + i;
        let b = a + word.len();
        let ok_a = hay[..a].chars().next_back().is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
        let ok_b = hay[b..].chars().next().is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
        if ok_a && ok_b {
            return true;
        }
        from = b;
    }
    false
}

impl Store {
    /// What came together read into rows, once nothing more came for two frames (or it waited long enough). Returns
    /// whether a new row mentions you (for its sound).
    pub fn pump(&mut self) -> bool {
        if self.batch.is_empty() || !(self.now - self.last_in > 0.034 || self.now - self.batch_at > 0.2 || self.now < self.last_in) {
            return false;
        }
        let batch = std::mem::take(&mut self.batch);
        let server = Server::of(&self.server);
        let me = self.me.clone();
        let mut lines = Vec::new();
        for m in &batch {
            let spans: Vec<Span> = m
                .runs
                .iter()
                .enumerate()
                .map(|(i, r)| Span {
                    text: r.text.clone(),
                    colour: if r.fl & F_PLAIN != 0 { None } else { Some(r.col) },
                    flags: r.fl & (F_BOLD | F_ITALIC | F_UNDER | F_STRIKE | F_OBF | F_GLYPH),
                    click: if r.fl & F_CLICK != 0 { i as i32 } else { -1 },
                    run: i as i32,
                })
                .collect();
            for l in rich::lines(&spans) {
                lines.push(parse::Line::new(m.id, l));
            }
        }
        let mut mentioned = false;
        for p in parse::read(&lines, server, &me) {
            mentioned |= self.push(p.kind, p.lines, &batch);
        }
        mentioned
    }

    /// A row more, or one more on the row it repeats; the server's answers mark what they answer. True when it is a
    /// new mention of you.
    fn push(&mut self, kind: Said, lines: Vec<(i32, Vec<Span>)>, batch: &[In]) -> bool {
        let now = self.now;
        let ids: Vec<i32> = lines.iter().map(|l| l.0).collect();
        let src: Vec<&In> = batch.iter().filter(|m| ids.contains(&m.id)).collect();
        let clock = src.last().map_or(-1, |m| m.clock);
        match &kind {
            Said::Skip | Said::Cleared { .. } | Said::CheckSent => return false,
            Said::TpDone => {
                if let Some(r) = self.rows.iter_mut().rev().find(|r| matches!(r.kind, Said::Tp { .. }) && r.state == 0) {
                    r.state = 3;
                    r.bump = now;
                    r.version += 1;
                }
                return false;
            }
            Said::RestartStop => {
                self.pins.retain(|p| p.key != "restart");
                if let Some(r) = self.rows.iter_mut().rev().find(|r| matches!(r.kind, Said::Restart { .. }) && r.state == 0) {
                    r.state = 3;
                    r.bump = now;
                    r.version += 1;
                }
                return false;
            }
            Said::Restart { secs } => {
                if let Some(p) = self.pins.iter_mut().find(|p| p.key == "restart") {
                    p.until = now + *secs as f32;
                } else {
                    self.pins.insert(0, Pin { key: "restart".into(), name: super::super::super::i18n::tr("Server restart").to_string(), xyz: String::new(), until: now + *secs as f32, born: now });
                }
                if let Some(i) = self.rows.iter().rposition(|r| matches!(r.kind, Said::Restart { .. }) && r.state == 0) {
                    // the count said again: its row comes down to the foot with the new time
                    let mut r = self.rows.remove(i);
                    r.until = now + *secs as f32;
                    r.secs = *secs as f32;
                    r.born = now;
                    r.version += 1;
                    self.rows.push(r);
                    return false;
                }
            }
            _ => {}
        }
        // what no rule read but Java knew to be a private message (the game's own words for it, any language):
        // a conversation's, its words after the first ": "
        let kind = match kind {
            Said::Raw if src.first().is_some_and(|m| (m.guess == 2 || m.guess == 3) && !m.sender.is_empty()) => {
                let m = src[0];
                let first = lines.first().map(|l| l.1.clone()).unwrap_or_default();
                let p = rich::plain(&first);
                match p.find(": ") {
                    Some(at) => Said::Whisper { from: m.sender.clone(), text: rich::from(&first, p[..at + 2].chars().count()), mine: m.guess == 3 },
                    None => Said::Raw,
                }
            }
            k => k,
        };
        let plain = lines.iter().map(|(_, l)| rich::plain(l)).collect::<Vec<_>>().join("\n");
        let (cat, who) = match &kind {
            Said::Chat(c) => (1, c.player.clone()),
            Said::Whisper { from, .. } => (2, from.clone()),
            Said::Request { from, .. } => (3, from.clone()),
            Said::Raw => {
                let g = src.first().map_or(0, |m| m.guess);
                (
                    match g {
                        1 => 1,
                        2 | 3 => 2,
                        _ => 3,
                    },
                    src.first().map_or(String::new(), |m| m.sender.clone()),
                )
            }
            _ => (3, String::new()),
        };
        let mine = match &kind {
            Said::Whisper { mine, .. } => *mine,
            _ => !self.me.is_empty() && who.eq_ignore_ascii_case(&self.me),
        };
        // a mention: your name in what another player said (not the server's words that name you — "Lunuca was
        // teleported" — nor your own)
        let me = self.me.to_lowercase();
        let mention = !mine
            && match &kind {
                Said::Chat(c) => has_word(&rich::plain(&c.text).to_lowercase(), &me),
                Said::Whisper { text, .. } => has_word(&rich::plain(text).to_lowercase(), &me),
                Said::Raw if cat == 1 => has_word(&plain.to_lowercase(), &me),
                _ => false,
            };
        let key = match &kind {
            Said::Chat(c) => Some(format!("c|{}|{}", c.player, rich::plain(&c.text))),
            Said::Raw | Said::Status { .. } | Said::Remind { .. } | Said::Section { .. } | Said::Promo { .. } | Said::Kill { .. } => Some(format!("r|{plain}")),
            _ => None,
        };
        // words sent in a conversation: their echo (they went), or the server's word on them first (they did not)
        if let Some((to, at)) = self.sent.clone() {
            if now - at > 2.5 || now < at {
                self.sent = None;
            } else if matches!(&kind, Said::Whisper { from, mine: true, .. } if from.eq_ignore_ascii_case(&to)) {
                self.sent = None;
            } else if cat == 3 && matches!(kind, Said::Raw) && !plain.trim().is_empty() {
                self.note = Some((to, plain.lines().next().unwrap_or("").trim().to_string(), now));
                self.sent = None;
            }
        }
        if let Said::Whisper { from, mine: true, .. } = &kind {
            if self.note.as_ref().is_some_and(|n| n.0.eq_ignore_ascii_case(from)) {
                self.note = None;
            }
        }
        if self.fold {
            if let (Some(k), Some(last)) = (&key, self.rows.last_mut()) {
                if last.key.as_ref() == Some(k) {
                    last.count += 1;
                    last.bump = now;
                    last.born = last.born.max(now - 0.3);
                    last.version += 1;
                    return false;
                }
            }
        }
        let (until, secs) = match &kind {
            Said::Request { secs, .. } | Said::Tp { secs } | Said::Restart { secs } => (now + *secs as f32, *secs as f32),
            Said::Check { secs, .. } => (now + *secs as f32, *secs as f32),
            _ => (0.0, 0.0),
        };
        // events pinned over the open chat while they last
        let xyz = |x: i32, y: Option<i32>, z: i32| match y {
            Some(y) => format!("{x} {y} {z}"),
            None => format!("{x} {z}"),
        };
        match &kind {
            Said::Event { ev, state, pos, title, .. } if !matches!(state, parse::EvState::Opened | parse::EvState::Wave | parse::EvState::Clear) => {
                let name = if title.is_empty() { ev.name().to_string() } else { title.clone() };
                let at = pos.map(|(x, y, z)| xyz(x, y, z)).unwrap_or_default();
                self.pin(format!("{}{}", ev.name(), at), name, at, now);
            }
            Said::Airdrop { title, x, z, .. } => {
                let at = xyz(*x, None, *z);
                let name = if title.is_empty() { super::super::super::i18n::tr("Airdrop").to_string() } else { title.clone() };
                self.pin(format!("drop{at}"), name, at, now);
            }
            _ => {}
        }
        let id = ids.first().copied().unwrap_or(0);
        self.rows.push(Row { id, clock, kind, lines, cat, mention, who, mine, plain, key, count: 1, born: now, bump: -9.0, until, secs, state: 0, read: false, version: 0, laid: None });
        if cat == 2 && !mine {
            self.priv_total += 1;
        }
        if mention {
            self.ment_total += 1;
        }
        // «new messages» counts what comes into what is shown, while it is scrolled up
        if self.scroll_t > 2.0 && self.open && self.rows.last().is_some_and(|r| self.in_view(r)) {
            self.unread += 1;
        }
        let max = self.max.max(100);
        if self.rows.len() > max {
            let n = self.rows.len() - max;
            let gone: Vec<i32> = self.rows.drain(..n).flat_map(|r| r.lines.into_iter().map(|l| l.0)).collect();
            let keep: std::collections::HashSet<i32> = self.rows.iter().flat_map(|r| r.lines.iter().map(|l| l.0)).collect();
            for g in gone {
                if !keep.contains(&g) {
                    self.runs.remove(&g);
                }
            }
        }
        mention
    }

    fn pin(&mut self, key: String, name: String, xyz: String, now: f32) {
        self.pins.retain(|p| p.key != key);
        self.pins.insert(0, Pin { key, name, xyz, until: now + 180.0, born: now });
        self.pins.truncate(2);
    }

    /// Whether a row is in what the open window shows now: the conversation open, or the channel.
    pub fn in_view(&self, r: &Row) -> bool {
        match (&self.tab, &self.dm) {
            (Tab::Private, Some(d)) => matches!(&r.kind, Said::Whisper { from, .. } if from.eq_ignore_ascii_case(d)),
            (tab, _) => self.shows(r, *tab, true),
        }
    }

    /// Whether a row is shown on a channel (and not hidden; the ads only when asked).
    pub fn shows(&self, r: &Row, tab: Tab, ads: bool) -> bool {
        if matches!(r.kind, Said::Promo { .. }) && !ads {
            return false;
        }
        if !r.mine && self.is_hidden(&r.who) && matches!(r.kind, Said::Chat(_) | Said::Raw | Said::Whisper { .. }) {
            return false;
        }
        match tab {
            Tab::All => true,
            Tab::Players => r.cat == 1,
            Tab::Private => r.cat == 2,
            Tab::Server => r.cat == 3,
            Tab::Mentions => r.mention,
            Tab::Own(i) => self.own.get(i as usize).is_some_and(|(_, words)| {
                let low = r.plain.to_lowercase();
                words.iter().any(|w| low.contains(w.as_str()))
            }),
        }
    }

    /// The private conversations, the latest first: the other player, the rows (their places).
    pub fn conversations(&self) -> Vec<(String, Vec<usize>)> {
        let mut peers: Vec<(String, Vec<usize>)> = Vec::new();
        for (i, r) in self.rows.iter().enumerate() {
            if let Said::Whisper { from, .. } = &r.kind {
                match peers.iter_mut().find(|p| p.0.eq_ignore_ascii_case(from)) {
                    Some(p) => p.1.push(i),
                    None => peers.push((from.clone(), vec![i])),
                }
            }
        }
        peers.sort_by_key(|p| std::cmp::Reverse(*p.1.last().unwrap_or(&0)));
        peers
    }

    pub fn row_by_id(&mut self, id: i32) -> Option<&mut Row> {
        self.rows.iter_mut().rev().find(|r| r.id == id)
    }
}
