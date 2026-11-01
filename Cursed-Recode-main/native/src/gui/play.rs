//! The modules that act in the game (her word, 25.10: every one of them working): Auto Sprint, Auto Respawn, Death
//! Coords, and those after them. Java tells each tick's facts of the game and its moments (cursed.recode.play.Play);
//! they decide here; what they say back Java does, a line each:
//!   sprint                   the player runs
//!   respawn                  back from the death screen
//!   local <text>\x1f<copy>   a line in the chat only the player sees (pressed: <copy> copied)
//!   say <text>               a line said in the chat
//!   cmd <command>            a command sent
//!   friend?                  who is looked at, asked (its name comes back: "friend <name>")

use super::i18n::{tr, trn};
use super::{theme, Gui};
use std::collections::HashMap;
use std::time::Instant;

/// A death kept (Death Coords): when (s since 1970), the server, the world, where, of what.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Death {
    pub at: u64,
    pub server: String,
    pub dim: String,
    pub pos: [f64; 3],
    pub cause: String,
}

impl Death {
    fn line(&self) -> String {
        format!("{}\t{}\t{}\t{:.1} {:.1} {:.1}\t{}", self.at, clean(&self.server), clean(&self.dim), self.pos[0], self.pos[1], self.pos[2], clean(&self.cause))
    }

    fn parse(l: &str) -> Option<Death> {
        let mut f = l.split('\t');
        let at = f.next()?.parse().ok()?;
        let server = f.next()?.to_string();
        let dim = f.next()?.to_string();
        let mut p = f.next()?.split(' ').filter_map(|v| v.parse::<f64>().ok());
        let pos = [p.next()?, p.next()?, p.next()?];
        Some(Death { at, server, dim, pos, cause: f.next().unwrap_or("").to_string() })
    }

    /// Its block, as the game's F3 says it ("x y z").
    pub fn block(&self) -> String {
        format!("{} {} {}", self.pos[0].floor() as i64, self.pos[1].floor() as i64, self.pos[2].floor() as i64)
    }
}

fn clean(s: &str) -> String {
    s.replace(['\t', '\n', '\r', '\u{1f}'], " ")
}

// Keep one bridge action and stay within Minecraft's 256 UTF-16 chat units.
fn chat_line(s: &str) -> String {
    let mut out = String::new();
    let mut units = 0;
    for ch in s.chars() {
        if ch == '§' { continue; }
        let ch = if ch.is_control() { ' ' } else { ch };
        units += ch.len_utf16();
        if units > 256 { break; }
        out.push(ch);
    }
    out
}

/// A world's name as the player knows it (the game's three; another as its id says).
pub(super) fn world_name(dim: &str) -> String {
    match dim {
        "minecraft:overworld" => tr("Overworld").to_string(),
        "minecraft:the_nether" => tr("The Nether").to_string(),
        "minecraft:the_end" => tr("The End").to_string(),
        d => d.rsplit(':').next().unwrap_or(d).replace('_', " "),
    }
}

#[derive(Default)]
pub(super) struct Play {
    /// dead at the last tick, since when; the respawn asked last
    dead: bool,
    dead_at: Option<Instant>,
    asked: Option<Instant>,
    /// Death Coords' list (newest last), read once
    pub deaths: Vec<Death>,
    loaded: bool,
    /// No Friend Damage's friends (their nicks, as added), read once; its key pressed: who is looked at is asked
    pub friends: Vec<String>,
    friends_loaded: bool,
    friend_ask: bool,
    /// what is said or sent later (a human moment after: Auto Accept, Kill Message), in the order due
    queue: Vec<(Instant, String)>,
    accepted_at: [Option<Instant>; 4],
    server: Option<String>,
    /// Kill Message: the kills since the last death, who killed last, when a line was last said, each list's turn and
    /// the lines not yet said of a bag (At random, no repeats)
    streak: u32,
    killer: String,
    said_at: Option<Instant>,
    turn: HashMap<String, usize>,
    bag: HashMap<String, Vec<usize>>,
    seed: u64,
}

/// What a server's button accepts (its command), by kind: 0 a teleport, 1 a duel, 2 an invitation (a party, a clan, a
/// team), 3 a trade; and the nick in it when there is one.
pub(super) fn accepts(cmd: &str) -> Option<(usize, Option<String>)> {
    let c = cmd.trim().trim_start_matches('/').to_lowercase();
    let w: Vec<&str> = c.split_whitespace().collect();
    let first = *w.first()?;
    let has = |x: &str| w.iter().any(|v| *v == x);
    let kind = if ["tpaccept", "tpyes", "tpaaccept", "tpok", "tpacc"].contains(&first) || (["tpa", "tpahere", "teleport", "tp"].contains(&first) && has("accept")) {
        0
    } else if ["duel", "duels", "1v1", "fight", "pvp"].contains(&first) && (has("accept") || has("yes")) {
        1
    } else if ["party", "p", "clan", "c", "team", "f", "faction", "guild", "g", "is", "island"].contains(&first) && (has("accept") || has("join")) {
        2
    } else if ["trade", "tr", "exchange"].contains(&first) && (has("accept") || has("yes")) {
        3
    } else {
        return None;
    };
    // the nick: the word after the one that accepts, as a nick is written (the command's own case kept)
    let orig: Vec<&str> = cmd.trim().trim_start_matches('/').split_whitespace().collect();
    let at = orig.iter().position(|v| ["accept", "yes", "join"].contains(&v.to_lowercase().as_str())).map_or(1, |i| i + 1);
    let name = orig.get(at).filter(|n| nick_ok(n)).map(|n| n.to_string());
    Some((kind, name))
}

/// The kinds' switches in Auto Accept, by the kind's number.
const ACCEPT_KINDS: [&str; 4] = ["Teleport requests", "Duels", "Invitations", "Trades"];
pub(super) const ACCEPT_FIELDS: [[&str; 4]; 4] = [
    ["Teleport settings", "Teleport from", "Teleport list", "Teleport delay"],
    ["Duel settings", "Duel from", "Duel list", "Duel delay"],
    ["Invitation settings", "Invitation from", "Invitation list", "Invitation delay"],
    ["Trade settings", "Trade from", "Trade list", "Trade delay"],
];

fn accept_fields(set: &[super::data::Setting], kind: usize) -> [&'static str; 3] {
    if theme::on(set, "Separate request settings") {
        let f = ACCEPT_FIELDS[kind];
        [f[1], f[2], f[3]]
    } else { ["From", "The list", "Delay"] }
}

/// Kill Message's lists, by the kill's kind: the settings that hold them (lines by \x1e).
pub(super) const KILL_LISTS: [(&str, &str); 5] = [("Kill", "Lines"), ("Streak", "Streak lines"), ("Revenge", "Revenge lines"), ("Last hearts", "Last hearts lines"), ("Long shot", "Long shot lines")];

/// A nick as the game allows one (3 to 16 of letters, figures and _).
pub(super) fn nick_ok(n: &str) -> bool {
    (3..=16).contains(&n.chars().count()) && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// A tick's facts: "key=value" by \x1f.
fn facts(s: &str) -> HashMap<&str, &str> {
    s.split('\u{1f}').filter_map(|p| p.split_once('=')).collect()
}

impl Gui {
    /// Java's words to the acting modules: "tick <facts>" (what to do now, lines), "deaths" (the list's file is read).
    pub(super) fn play_call(&mut self, verb: &str, a: &str) -> String {
        match verb {
            "tick" => self.play_tick(a),
            // a line come to the chat ("source\x1fplain text\x1fcommands of its buttons by \x1e"): Auto Accept
            "chat" => {
                self.play_chat(a);
                String::new()
            }
            // a kill (the HUD saw the one fought die after the player's hit): Kill Message
            "kill" => {
                self.play_kill(a);
                String::new()
            }
            // the player looked at as the friend key went: a friend now, or no longer
            "friend" => {
                self.friend_toggle(a.trim());
                String::new()
            }
            // a hit about to go at a player ("name\tally"): "1" when it must not
            "hit" => {
                let (name, ally) = a.split_once('\t').unwrap_or((a, "0"));
                (self.friend_spared(name, ally == "1") as i32).to_string()
            }
            // the camera this frame (Server Adaptation's zones and timers are drawn by it)
            "view" => {
                self.adapt_view(a);
                String::new()
            }
            // a line about to be sent: "1" held back (Server Adaptation's chat filter)
            "say?" => (self.adapt_chat(a) as i32).to_string(),
            "item-name" => (a.split_once('\u{1f}').is_some_and(|(expected,actual)|super::adapt::item_name_matches(expected,actual)) as i32).to_string(),
            "adapt-ready" => (self.adapt_ready(a) as i32).to_string(),
            _ => String::new(),
        }
    }

    /// A key down in the world (cards.rs world_key): No Friend Damage's key asks who is looked at.
    pub(super) fn play_key(&mut self, name: &str) {
        // Server Adaptation's keys of the servers' things
        self.adapt_key(name);
        let key = self.play_on("no-friend-damage").and_then(|s| theme::find(s, "Add a friend")).and_then(|k| match k {
            super::data::Kind::Key(v) => Some(v.clone()),
            _ => None,
        });
        if key.is_some_and(|k| !k.is_empty() && k == name) && !self.hidden_here("no-friend-damage") {
            self.play.friend_ask = true;
        }
    }

    fn play_on(&self, id: &str) -> Option<&[super::data::Setting]> {
        self.mods.iter().find(|m| m.id == id && m.on).map(|m| m.set.as_slice())
    }

    fn play_tick(&mut self, s: &str) -> String {
        let f = facts(s);
        let flag = |k: &str| f.get(k) == Some(&"1");
        let mut out: Vec<String> = Vec::new();
        if !flag("world") {
            self.adapt=Default::default();
            self.fl=Default::default();
            crate::client_marks::context("", &self.player);
            self.play.dead = false;
            self.play.dead_at = None;
            self.play.queue.clear();
            self.play.accepted_at = [None; 4];
            self.play.server = None;
            self.kill_messages_reset();
            return String::new();
        }
        let server = f.get("server").copied().unwrap_or("");
        let nick = f.get("nick").copied().unwrap_or(&self.player);
        crate::client_marks::context(server, nick);
        if self.play.server.as_deref().is_some_and(|old| old != server) {
            self.adapt=Default::default();
            self.fl=Default::default();
            self.play.queue.clear();
            self.play.accepted_at = [None; 4];
            self.kill_messages_reset();
        }
        self.play.server = Some(server.to_string());
        // ---- a death: kept and said (Death Coords); back by itself when its time is up (Auto Respawn)
        let dead = flag("dead");
        if dead && !self.play.dead {
            self.play.dead_at = Some(Instant::now());
            self.play.asked = None;
            // (Kill Message: the streak ends; who killed is kept for a revenge)
            self.play.streak = 0;
            self.play.killer = f.get("killer").copied().unwrap_or("").to_string();
            self.died(&f, &mut out);
        }
        if !dead {
            self.play.dead_at = None;
        }
        self.play.dead = dead;
        if let (true, Some(set), Some(at)) = (dead, self.play_on("auto-respawn"), self.play.dead_at) {
            let delay = theme::num(set, "Delay") / 1000.0;
            // (asked again now and then while the server has not let it be: a lag, a death screen held back)
            let due = self.play.asked.is_none_or(|t| t.elapsed().as_secs_f32() > 2.0);
            if !flag("hardcore") && at.elapsed().as_secs_f32() >= delay && due {
                self.play.asked = Some(Instant::now());
                out.push("respawn".into());
            }
        }
        if self.play_on("auto-accept").is_none() || self.hidden_here("auto-accept") {
            self.play.queue.retain(|(_, action)| !action.starts_with("accept "));
        }
        if self.play_on("kill-message").is_none() || self.hidden_here("kill-message") {
            self.kill_messages_reset();
        }
        // ---- what was put off until now (a human moment after)
        let now = Instant::now();
        while self.play.queue.first().is_some_and(|q| q.0 <= now) {
            let action = self.play.queue.remove(0).1;
            if let Some(request) = action.strip_prefix("accept ") {
                self.accept_due(request, &mut out);
            } else if let Some(message) = action.strip_prefix("kill-message ") {
                out.push(message.to_string());
            } else {
                out.push(action);
            }
        }
        // ---- No Friend Damage's key: who is looked at
        if std::mem::take(&mut self.play.friend_ask) {
            out.push("friend?".into());
        }
        // ---- Server Adaptation: the things' timers, the keys' uses, /fix all
        self.adapt_tick(&f, &mut out);
        // ---- Always ignores physical restrictions; Smart keeps the previous behaviour.
        if let Some(set) = self.play_on("auto-sprint") {
            let fed = f.get("food").and_then(|v| v.parse::<f32>().ok()).unwrap_or(20.0) > 6.0 || flag("fly");
            let water_ok = !flag("water") || theme::on(set, "Swim in water");
            let run = match theme::pick(set, "Mode") {
                0 => true,
                1 => flag("fwd"),
                _ => flag("fwd") && !flag("sneak") && !flag("using") && fed && !flag("blind") && !flag("wall") && water_ok,
            };
            if !dead && !flag("screen") && run {
                out.push("sprint".into());
            }
        }
        out.join("\n")
    }

    // ------------------------------------------------------------------ No Friend Damage's friends
    fn friends_file(&self) -> Option<std::path::PathBuf> {
        self.dir.as_ref().map(|d| d.join("cursed-recode").join("friends.txt"))
    }

    pub(super) fn friends_load(&mut self) {
        if self.play.friends_loaded {
            return;
        }
        self.play.friends_loaded = true;
        if let Some(text) = self.friends_file().and_then(|f| std::fs::read_to_string(f).ok()) {
            self.play.friends = text.lines().map(str::trim).filter(|l| nick_ok(l)).map(String::from).collect();
        }
    }

    fn friends_save(&self) {
        let Some(f) = self.friends_file() else { return };
        let text: String = self.play.friends.iter().map(|n| format!("{n}\n")).collect();
        let _ = std::fs::create_dir_all(f.parent().unwrap());
        let tmp = f.with_extension("tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, &f);
        }
    }

    pub(super) fn is_friend(&mut self, name: &str) -> bool {
        self.friends_load();
        self.play.friends.iter().any(|f| f.eq_ignore_ascii_case(name))
    }

    /// A friend added (its card's field, the key); false when it is no nick or one already.
    pub(super) fn friend_add(&mut self, name: &str) -> bool {
        if !nick_ok(name) || self.is_friend(name) {
            return false;
        }
        self.play.friends.push(name.to_string());
        self.friends_save();
        true
    }

    pub(super) fn friend_remove(&mut self, name: &str) {
        self.friends_load();
        self.play.friends.retain(|f| !f.eq_ignore_ascii_case(name));
        self.friends_save();
    }

    /// The key on a player: a friend now, or no longer — said in a note.
    fn friend_toggle(&mut self, name: &str) {
        if !nick_ok(name) {
            return;
        }
        let now = if self.is_friend(name) {
            self.friend_remove(name);
            false
        } else {
            self.friend_add(name)
        };
        let notify = self.play_on("no-friend-damage").is_some_and(|s| theme::on(s, "Notify"));
        if notify {
            let line = if now { trn("{name} is your friend now", &[("name", name.to_string())]) } else { trn("{name} is no longer your friend", &[("name", name.to_string())]) };
            let t = self.now;
            self.hud.note(tr("No Friend Damage").to_string(), line, t);
        }
        self.switch_sound(now, 0.5);
    }

    /// Whether a hit at this player is held back (a friend, or a teammate if those are friends too).
    fn friend_spared(&mut self, name: &str, ally: bool) -> bool {
        let Some(team) = self.play_on("no-friend-damage").map(|s| theme::on(s, "Teammates are friends")) else { return false };
        if self.hidden_here("no-friend-damage") {
            return false;
        }
        (team && ally) || self.is_friend(name)
    }

    /// What is waiting to be said or done (the tests).
    #[cfg(test)]
    pub(super) fn play_queue(&self) -> Vec<String> {
        self.play.queue.iter().map(|q| q.1.clone()).collect()
    }

    pub(super) fn later(&mut self, ms: f32, what: String) {
        let due = Instant::now() + std::time::Duration::from_millis(ms.max(0.0) as u64);
        let at = self.play.queue.iter().position(|q| q.0 > due).unwrap_or(self.play.queue.len());
        self.play.queue.insert(at, (due, what));
    }

    fn rand(&mut self) -> u64 {
        if self.play.seed == 0 {
            self.play.seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(7, |d| d.as_nanos() as u64) | 1;
        }
        let mut x = self.play.seed;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.play.seed = x;
        x
    }

    // ------------------------------------------------------------------ Auto Accept
    /// A line of the chat: a server's button accepting a request, from whom the settings trust, pressed after a moment.
    fn play_chat(&mut self, a: &str) {
        let mut f = a.split('\u{1f}');
        let (src, text, cmds) = (f.next().unwrap_or(""), f.next().unwrap_or(""), f.next().unwrap_or(""));
        // (only the server's own words: a player's cannot carry a button that accepts)
        if src != "SYSTEM_SERVER" || self.hidden_here("auto-accept") {
            return;
        }
        let Some(set) = self.play_on("auto-accept").map(|s| s.to_vec()) else { return };
        for cmd in cmds.split('\u{1e}').filter(|c| !c.trim().is_empty()) {
            let Some((kind, name)) = accepts(cmd) else { continue };
            if !theme::on(&set, ACCEPT_KINDS[kind]) {
                continue;
            }
            let fields = accept_fields(&set, kind);
            let trusted = self.accept_trusted(&set, kind, name.as_deref(), text);
            if !trusted {
                continue;
            }
            let prefix = format!("accept {kind}\u{1f}");
            let gap = theme::num(&set, "Accept interval").max(0.0);
            if self.play.queue.iter().any(|(_, action)| action.starts_with(&prefix))
                || self.play.accepted_at[kind].is_some_and(|at| at.elapsed().as_secs_f32() < gap) {
                continue;
            }
            let delay = theme::num(&set, fields[2]) * (0.85 + 0.3 * (self.rand() % 1000) as f32 / 1000.0);
            self.later(delay, format!("{prefix}{}\u{1f}{text}", cmd.trim().trim_start_matches('/')));
            // Only one accepting button from a server line is needed.
            break;
        }
    }

    fn accept_trusted(&mut self, set: &[super::data::Setting], kind: usize, name: Option<&str>, text: &str) -> bool {
        let fields = accept_fields(set, kind);
        match theme::pick(set, fields[0]) {
                0 => true,
                from => {
                    let list: Vec<String> = theme::text(&set, fields[1]).split([',', ' ']).map(|n| n.trim().to_lowercase()).filter(|n| !n.is_empty()).collect();
                    // the nick of the button, or else one of the friends' named in the words
                    let lower = text.to_lowercase();
                    let named = |n: &str| lower.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).any(|w| w == n.to_lowercase());
                    self.friends_load();
                    let friends = self.play.friends.clone();
                    let ok = |n: &str| (from != 3 && friends.iter().any(|f| f.eq_ignore_ascii_case(n))) || (from >= 2 && list.iter().any(|l| l.eq_ignore_ascii_case(n)));
                    match name {
                        Some(n) => ok(n),
                        None => (from != 3 && friends.iter().any(|f| named(f))) || (from >= 2 && list.iter().any(|l| named(l))),
                    }
                }
            }
    }

    fn accept_due(&mut self, request: &str, out: &mut Vec<String>) {
        let mut fields = request.splitn(3, '\u{1f}');
        let Some(kind) = fields.next().and_then(|n| n.parse::<usize>().ok()).filter(|n| *n < 4) else { return };
        let (cmd, text) = (fields.next().unwrap_or(""), fields.next().unwrap_or(""));
        if self.hidden_here("auto-accept") { return; }
        let Some(set) = self.play_on("auto-accept").map(|s| s.to_vec()) else { return };
        if !theme::on(&set, ACCEPT_KINDS[kind]) { return; }
        let Some((got, name)) = accepts(cmd) else { return };
        if got != kind { return; }
        if !self.accept_trusted(&set, kind, name.as_deref(), text) { return; }
        let last = self.play.accepted_at[kind];
        let gap = theme::num(&set, "Accept interval").max(0.0);
        if last.is_some_and(|at| at.elapsed().as_secs_f32() < gap) { return; }
        out.push(format!("cmd {cmd}"));
        self.play.accepted_at[kind] = Some(Instant::now());
        if theme::on(&set, "Sound when accepted") { self.switch_sound(true, 0.5); }
        if theme::on(&set, "Notify") {
                let what = [tr("a teleport"), tr("a duel"), tr("an invitation"), tr("a trade")][kind];
                let line = match &name {
                    Some(n) => trn("Accepted {what} from {name}", &[("what", what.to_string()), ("name", n.clone())]),
                    None => trn("Accepted {what}", &[("what", what.to_string())]),
                };
                let t = self.now;
                self.hud.note("Auto Accept".to_string(), line, t);
            }
    }

    /// A request preview, driven by the module's real delay and filter settings; no action is
    /// queued or sent by it. The client palette and type are shared with every other menu card.
    pub(super) fn accept_preview(&mut self, c: &mut super::draw::Canvas, area: [f32; 4], module: usize, reveal: f32) {
        use super::draw::{grey, mix};
        use super::text::{fonts, Face};
        use super::theme::{fade, pal};
        let set = self.mods[module].set.clone();
        let enabled: Vec<usize> = ACCEPT_KINDS.iter().enumerate().filter_map(|(i, key)| theme::on(&set, key).then_some(i)).collect();
        let kind = enabled.get((self.now / 8.0) as usize % enabled.len().max(1)).copied().unwrap_or(0);
        let fields = accept_fields(&set, kind);
        let delay = theme::num(&set, fields[2]) / 1000.0;
        let phase = self.now % (delay + 4.5);
        self.friends_load();
        let from_list = theme::text(&set, fields[1]).split([',', ' ']).find(|n| nick_ok(n)).map(String::from);
        let name = match theme::pick(&set, fields[0]) {
            1 => self.play.friends.first().cloned(),
            2 => self.play.friends.first().cloned().or(from_list),
            3 => from_list,
            _ => None,
        }.unwrap_or_else(|| "Alex".into());
        let trusted = self.accept_trusted(&set, kind, Some(&name), &name);
        let accepted = phase >= delay + 1.0 && !enabled.is_empty() && trusted;
        let progress = if delay <= 0.0 { 1.0 } else { ((phase - 1.0) / delay).clamp(0.0, 1.0) };
        let r = self.r;
        let w = (area[2] - area[0] - 3.0 * r).min(26.0 * r);
        let x = (area[0] + area[2] - w) * 0.5;
        let clip = c.clip_now();
        let y = (area[1].max(clip[1]) + area[3].min(clip[3])) * 0.5 - 5.7 * r;
        let card = [x, y, x + w, y + 11.4 * r];
        let alpha = c.alpha;
        c.alpha *= super::ease_out(reveal);
        c.rect(area, 0.75 * r, pal().f1);
        c.shadow(card, 1.1 * r, 1.5 * r, grey(0.0, 0.25));
        c.rect_lit(card, 1.1 * r, pal().f2, grey(1.0, 0.06), 1.0);
        let avatar = [x + 1.3 * r, y + 1.3 * r, x + 4.3 * r, y + 4.3 * r];
        c.rect(avatar, 0.85 * r, pal().f4);
        let initial = name.chars().next().unwrap_or('A').to_uppercase().to_string();
        c.text_c(x + 2.8 * r, y + 3.35 * r, 1.25 * r, Face::Semi, &initial, pal().t1);
        c.text(x + 5.2 * r, y + 2.65 * r, 1.0 * r, Face::Semi, &fonts().fit(Face::Semi, 1.0 * r, &name, w - 6.5 * r), pal().t1);
        let from = match theme::pick(&set, fields[0]) { 0 => tr("Everyone"), 1 => tr("Friends"), 2 => tr("Friends and the list"), _ => tr("Only the list") };
        let line = fonts().fit(Face::Regular, 0.7 * r, from, w - 6.5 * r);
        c.text(x + 5.2 * r, y + 3.95 * r, 0.7 * r, Face::Regular, &line, pal().t3);
        let title = fonts().fit(Face::Semi, 0.88 * r, tr(ACCEPT_KINDS[kind]), w - 2.6 * r);
        c.text(x + 1.3 * r, y + 6.0 * r, 0.88 * r, Face::Semi, &title, pal().t1);
        let status = if enabled.is_empty() { tr("Choose a request type") } else if !trusted { tr("Outside the chosen filter") } else if accepted { tr("Accepted") } else { tr("Waiting for acceptance") };
        let text = fonts().fit(Face::Regular, 0.76 * r, status, w - 2.6 * r);
        c.text(x + 1.3 * r, y + 7.7 * r, 0.76 * r, Face::Regular, &text, if accepted { pal().t1 } else { pal().t3 });
        let track = [x + 1.3 * r, y + 8.75 * r, x + w - 1.3 * r, y + 9.05 * r];
        c.rect(track, 0.15 * r, pal().f4);
        if !enabled.is_empty() && trusted && progress > 0.0 {
            c.rect([track[0], track[1], track[0] + (track[2] - track[0]) * progress, track[3]], 0.15 * r, mix(fade(pal().accent, 0.6), pal().accent, accepted as u8 as f32));
        }
        c.text_c(x + w * 0.5, y + 10.45 * r, 0.65 * r, Face::Mono, &format!("{:.1} {}", delay, tr("s")), pal().t4);
        c.alpha = alpha;
    }

    // ------------------------------------------------------------------ Kill Message
    /// A kill ("name\x1fplayer 1|0\x1fdistance\x1fyour health\x1fweapon\x1fyour nick\x1fserver"): a line of its kind's
    /// list said after a moment, as the order says.
    fn play_kill(&mut self, a: &str) {
        let f: Vec<&str> = a.split('\u{1f}').collect();
        let get = |i: usize| f.get(i).copied().unwrap_or("");
        let (name, player) = (get(0), get(1) == "1");
        // Kill Effects: where the one killed stood ("x y z", how tall)
        let feet: Vec<f64> = get(7).split(' ').filter_map(|v| v.parse().ok()).collect();
        let tall = get(8).parse().unwrap_or(1.8);
        self.killfx_kill(player, (feet.len() == 3).then(|| [feet[0], feet[1], feet[2]]), tall);
        if self.hidden_here("kill-message") {
            return;
        }
        let Some(set) = self.play_on("kill-message").map(|s| s.to_vec()) else { return };
        if theme::on(&set, "Players only") && !player {
            return;
        }
        self.play.streak = self.play.streak.saturating_add(1);
        let gap = theme::num(&set, "Not more often than");
        if self.play.said_at.is_some_and(|t| t.elapsed().as_secs_f32() < gap) {
            return;
        }
        let dist: f32 = get(2).parse().unwrap_or(0.0);
        let hp: f32 = get(3).parse().unwrap_or(20.0);
        let profile = super::messages::Book::read(&set).profile(&set, get(6));
        let lines = |kind: usize| profile.lines(kind, Some(self.play.streak));
        // its kind: a revenge, on the last hearts, from far, in a streak, or a kill
        let revenge = !self.play.killer.is_empty() && self.play.killer.eq_ignore_ascii_case(name);
        let order: [(usize, bool); 4] = [
            (2, theme::on(&set, "Revenge") && revenge),
            (3, theme::on(&set, "Last hearts") && hp <= 6.0),
            (4, theme::on(&set, "Long shots") && dist >= theme::num(&set, "Long shot from")),
            (1, theme::on(&set, "Streaks") && self.play.streak >= 2),
        ];
        let kind = order.iter().find(|(k, on)| *on && (!lines(*k).is_empty() || (*k == 1 && profile.streaks.contains_key(&self.play.streak)))).map_or(0, |o| o.0);
        let list = lines(kind).to_vec();
        if list.is_empty() {
            return;
        }
        if revenge {
            self.play.killer.clear();
        }
        use std::hash::{Hash, Hasher};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        list.hash(&mut hash);
        let key = format!("{}:{}:{}:{}", crate::client_marks::server_key(get(6)).unwrap_or_default(), kind, if kind == 1 && profile.streaks.contains_key(&self.play.streak) { self.play.streak } else { 0 }, hash.finish());
        if self.play.turn.len() + self.play.bag.len() > 1024 { self.kill_messages_order_reset(); }
        let i = match theme::pick(&set, "Order") {
            // in turn
            1 => {
                let t = self.play.turn.entry(key.clone()).or_insert(0);
                let i = *t % list.len();
                *t = t.wrapping_add(1);
                i
            }
            // at random, none again before all were said
            2 => {
                let fresh = self.play.bag.get(&key).is_none_or(|b| b.is_empty() || b.iter().any(|&x| x >= list.len()));
                if fresh {
                    self.play.bag.insert(key.clone(), (0..list.len()).collect());
                }
                let n = self.play.bag[&key].len();
                let at = (self.rand() % n as u64) as usize;
                self.play.bag.get_mut(&key).unwrap().swap_remove(at)
            }
            _ => (self.rand() % list.len() as u64) as usize,
        };
        let line = super::messages::render(&list[i], [
            name, &format!("{}", (hp / 2.0 * 10.0).round() / 10.0), get(4), &format!("{:.0}", dist),
            &self.play.streak.to_string(), get(5), get(6),
        ]);
        self.play.said_at = Some(Instant::now());
        let delay = theme::num(&set, "Delay");
        let what = if theme::pick(&set, "Send to") == 1 { format!("local {}", chat_line(&line)) } else { format!("say {}", chat_line(&format!("{}{line}", theme::text(&set, "Before each")))) };
        self.later(delay, format!("kill-message {what}"));
    }

    pub(super) fn kill_messages_order_reset(&mut self) {
        self.play.turn.clear();
        self.play.bag.clear();
    }

    fn kill_messages_reset(&mut self) {
        self.play.streak = 0;
        self.play.killer.clear();
        self.play.said_at = None;
        self.play.turn.clear();
        self.play.bag.clear();
        self.play.queue.retain(|(_, action)| !action.starts_with("kill-message "));
    }

    // ------------------------------------------------------------------ Death Coords
    fn deaths_file(&self) -> Option<std::path::PathBuf> {
        self.dir.as_ref().map(|d| d.join("cursed-recode").join("deaths.txt"))
    }

    /// The list from its file (once).
    pub(super) fn deaths_load(&mut self) {
        if self.play.loaded {
            return;
        }
        self.play.loaded = true;
        if let Some(text) = self.deaths_file().and_then(|f| std::fs::read_to_string(f).ok()) {
            self.play.deaths = text.lines().filter_map(Death::parse).collect();
        }
    }

    pub(super) fn deaths_save(&self) {
        let Some(f) = self.deaths_file() else { return };
        let text: String = self.play.deaths.iter().map(|d| d.line() + "\n").collect();
        let _ = std::fs::create_dir_all(f.parent().unwrap());
        let tmp = f.with_extension("tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, &f);
        }
    }

    /// The list emptied (its card's button).
    pub(super) fn deaths_clear(&mut self) {
        self.deaths_load();
        self.play.deaths.clear();
        self.deaths_save();
    }

    fn died(&mut self, f: &HashMap<&str, &str>, out: &mut Vec<String>) {
        let Some(set) = self.play_on("death-coords").map(|s| s.to_vec()) else { return };
        let mut p = f.get("pos").copied().unwrap_or("").split(' ').filter_map(|v| v.parse::<f64>().ok());
        let (Some(x), Some(y), Some(z)) = (p.next(), p.next(), p.next()) else { return };
        let d = Death {
            at: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()),
            server: f.get("server").copied().unwrap_or("").to_string(),
            dim: f.get("dim").copied().unwrap_or("").to_string(),
            pos: [x, y, z],
            cause: f.get("cause").copied().unwrap_or("").to_string(),
        };
        self.deaths_load();
        self.play.deaths.push(d.clone());
        let keep = theme::num(&set, "Keep the last").max(1.0) as usize;
        let n = self.play.deaths.len();
        if n > keep {
            self.play.deaths.drain(..n - keep);
        }
        self.deaths_save();
        if theme::on(&set, "Line in the chat") {
            // (Streamer Mode hiding the coordinates: dots said, nothing to copy)
            let hide = self.streamer_hides("Coordinates");
            let pos = if hide { "•••".to_string() } else { d.block() };
            let line = trn("You died at {pos} · {world}", &[("pos", pos), ("world", world_name(&d.dim))]);
            out.push(format!("local {line}\u{1f}{}", if hide { String::new() } else { d.block() }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_death_is_kept_as_a_line_and_read_back() {
        let d = Death { at: 1792890000, server: "mc.funtime.su".into(), dim: "minecraft:the_nether".into(), pos: [-12.6, 64.0, 1022.3], cause: "Lunuca\twas slain".into() };
        let back = Death::parse(&d.line()).unwrap();
        assert_eq!(back.block(), "-13 64 1022");
        assert_eq!(back.server, "mc.funtime.su");
        assert_eq!(back.cause, "Lunuca was slain");
        assert!(Death::parse("garbage").is_none());
    }

    #[test]
    fn a_servers_button_is_read() {
        assert_eq!(accepts("/tpaccept Lunuca"), Some((0, Some("Lunuca".into()))));
        assert_eq!(accepts("/tpyes"), Some((0, None)));
        assert_eq!(accepts("/duel accept Steve"), Some((1, Some("Steve".into()))));
        assert_eq!(accepts("/clan accept"), Some((2, None)));
        assert_eq!(accepts("/party join Alex"), Some((2, Some("Alex".into()))));
        assert_eq!(accepts("/trade accept Bob"), Some((3, Some("Bob".into()))));
        assert_eq!(accepts("/spawn"), None);
        assert_eq!(accepts("/tpa Lunuca"), None);
    }

    #[test]
    fn a_request_is_accepted_from_whom_it_trusts() {
        let mut g = Gui::new();
        g.play.friends_loaded = true;
        let i = g.mod_index("auto-accept");
        g.mods[i].on = true;
        for s in g.mods[i].set.iter_mut() {
            if s.label == "Delay" {
                s.kind = super::super::data::Kind::Slider { min: 0.0, max: 5000.0, step: 50.0, v: 0.0, unit: " ms" };
            }
        }
        // friends only (the default): a stranger's button is left alone, a friend's pressed
        g.play_chat("SYSTEM_SERVER\u{1f}Steve wants to teleport to you\u{1f}/tpaccept Steve");
        assert_eq!(g.play_tick("world=1"), "");
        g.friend_add("Steve");
        g.play_chat("SYSTEM_SERVER\u{1f}Steve wants to teleport to you\u{1f}/tpaccept Steve");
        assert_eq!(g.play_tick("world=1"), "cmd tpaccept Steve");
        // a player's line never
        g.play_chat("PLAYER\u{1f}Steve: lol\u{1f}/tpaccept Steve");
        assert_eq!(g.play_tick("world=1"), "");
    }

    #[test]
    fn a_kill_says_a_line_of_its_kind() {
        let mut g = Gui::new();
        let i = g.mod_index("kill-message");
        g.mods[i].on = true;
        for s in g.mods[i].set.iter_mut() {
            match s.label {
                "Delay" | "Not more often than" => s.kind = super::super::data::Kind::Slider { min: 0.0, max: 5000.0, step: 50.0, v: 0.0, unit: "" },
                "Lines" => s.kind = super::super::data::Kind::Text("gg {name}".into()),
                "Revenge lines" => s.kind = super::super::data::Kind::Text("got you back, {name}".into()),
                "Order" => s.kind = super::super::data::Kind::Choice { v: 1, opts: vec!["At random", "In turn", "At random, no repeats"] },
                _ => {}
            }
        }
        g.play_kill("Steve\u{1f}1\u{1f}3\u{1f}20\u{1f}Sword\u{1f}Lunuca\u{1f}");
        assert_eq!(g.play_tick("world=1"), "say gg Steve");
        // killed by Alex, then Alex killed: the revenge's line
        g.play_tick("world=1\u{1f}dead=1\u{1f}killer=Alex");
        g.play_tick("world=1");
        g.play_kill("Alex\u{1f}1\u{1f}3\u{1f}20\u{1f}Sword\u{1f}Lunuca\u{1f}");
        assert_eq!(g.play_tick("world=1"), "say got you back, Alex");
        // a mob is no one's to speak of
        g.play_kill("Zombie\u{1f}0\u{1f}3\u{1f}20\u{1f}Sword\u{1f}Lunuca\u{1f}");
        assert_eq!(g.play_tick("world=1"), "");
    }

    #[test]
    fn kill_messages_count_players_and_cancel_on_disable_or_server_change() {
        let mut g = Gui::new();
        let i = g.mod_index("kill-message");
        g.mods[i].on = true;
        for s in &mut g.mods[i].set {
            match s.label {
                "Lines" => s.kind = super::super::data::Kind::Text("gg {streak}".into()),
                "Not more often than" => s.kind = super::super::data::Kind::Slider { min: 0.0, max: 10.0, step: 1.0, v: 0.0, unit: "" },
                _ => {}
            }
        }
        g.play_tick("world=1\u{1f}server=one.test");
        g.play_kill("Zombie\u{1f}0");
        assert_eq!(g.play.streak, 0);
        g.play_kill("Steve\u{1f}1");
        assert_eq!(g.play.streak, 1);
        assert!(g.play.queue.iter().any(|(_, q)| q.starts_with("kill-message ")));
        g.mods[i].on = false;
        g.play_tick("world=1\u{1f}server=one.test");
        assert!(g.play.queue.is_empty());
        assert_eq!(g.play.streak, 0);
        g.mods[i].on = true;
        g.play_kill("Alex\u{1f}1");
        g.play_tick("world=1\u{1f}server=two.test");
        assert!(g.play.queue.is_empty());
        assert_eq!(g.play.streak, 0);
        g.play_kill("Alex\u{1f}1");
        g.play_tick("world=0");
        assert!(g.play.queue.is_empty());
        assert_eq!(g.play.streak, 0);
    }

    #[test]
    fn exact_streaks_and_server_messages_choose_the_right_list() {
        let mut g = Gui::new();
        let i = g.mod_index("kill-message");
        g.mods[i].on = true;
        for s in &mut g.mods[i].set {
            match s.label {
                "Lines" => s.kind = super::super::data::Kind::Text("default kill".into()),
                "Streak lines" => s.kind = super::super::data::Kind::Text("general {streak}".into()),
                "Delay" | "Not more often than" => s.kind = super::super::data::Kind::Slider { min: 0.0, max: 10.0, step: 1.0, v: 0.0, unit: "" },
                _ => {}
            }
        }
        let mut book = super::super::messages::Book::default();
        book.streaks.insert(3, vec!["default three".into()]);
        let mut profile = book.defaults(&g.mods[i].set);
        profile.lists[0] = vec!["server kill".into()];
        profile.streaks.insert(3, vec!["{server}: {streak}".into()]);
        profile.streaks.insert(4, Vec::new());
        book.servers.insert("one.test".into(), profile);
        g.mods[i].set.iter_mut().find(|s| s.label == super::super::messages::SETTING).unwrap().kind = super::super::data::Kind::Text(serde_json::to_string(&book).unwrap());
        g.play_tick("world=1\u{1f}server=one.test");
        for expected in ["say server kill", "say general 2", "say one.test: 3", ""] {
            g.play_kill("Steve\u{1f}1\u{1f}3\u{1f}20\u{1f}Sword\u{1f}Lunuca\u{1f}one.test");
            assert_eq!(g.play_tick("world=1\u{1f}server=one.test"), expected);
        }
        g.play_tick("world=1\u{1f}server=two.test");
        for expected in ["say default kill", "say general 2", "say default three"] {
            g.play_kill("Steve\u{1f}1\u{1f}3\u{1f}20\u{1f}Sword\u{1f}Lunuca\u{1f}two.test");
            assert_eq!(g.play_tick("world=1\u{1f}server=two.test"), expected);
        }
    }

    #[test]
    fn kill_chat_text_cannot_split_bridge_actions_or_exceed_game_limit() {
        assert_eq!(chat_line("gg\ncmd stop\r\u{1f}§a"), "gg cmd stop  a");
        let text = chat_line(&"😀".repeat(200));
        assert_eq!(text.chars().count(), 128);
        assert_eq!(text.encode_utf16().count(), 256);
    }

    #[test]
    fn friends_are_spared_and_kept() {
        let mut g = Gui::new();
        g.play.friends_loaded = true;
        let i = g.mod_index("no-friend-damage");
        g.mods[i].on = true;
        assert!(g.friend_add("Lunuca"));
        assert!(!g.friend_add("lunuca"));
        assert!(!g.friend_add("no spaces"));
        assert_eq!(g.play_call("hit", "LUNUCA\t0"), "1");
        assert_eq!(g.play_call("hit", "Steve\t0"), "0");
        // a teammate is spared too, as its switch says
        assert_eq!(g.play_call("hit", "Steve\t1"), "1");
        g.mods[i].on = false;
        assert_eq!(g.play_call("hit", "Lunuca\t0"), "0");
        g.friend_remove("Lunuca");
        assert!(!g.is_friend("Lunuca"));
    }


    fn accept_set(g: &mut Gui, label: &str, value: super::super::data::Kind) {
        let i = g.mod_index("auto-accept");
        g.mods[i].on = true;
        g.mods[i].set.iter_mut().find(|s| s.label == label).unwrap().kind = value;
    }

    fn accept_num(v: f32) -> super::super::data::Kind {
        super::super::data::Kind::Slider { min: 0.0, max: 5000.0, step: 1.0, v, unit: "" }
    }

    #[test]
    fn request_types_keep_their_own_filters_delays_and_configs() {
        use super::super::data::Kind;
        let mut g = Gui::new();
        g.play.friends_loaded = true;
        accept_set(&mut g, "Separate request settings", Kind::Toggle(true));
        accept_set(&mut g, "Duels", Kind::Toggle(true));
        accept_set(&mut g, "Teleport from", Kind::Choice { v: 3, opts: vec!["Everyone", "Friends", "Friends and the list", "Only the list"] });
        accept_set(&mut g, "Teleport list", Kind::Text("Alex".into()));
        accept_set(&mut g, "Teleport delay", accept_num(0.0));
        accept_set(&mut g, "Duel delay", accept_num(4000.0));
        g.friend_add("Steve");
        g.play_chat("SYSTEM_SERVER\u{1f}Steve\u{1f}/tpaccept Steve");
        assert!(g.play.queue.is_empty(), "a friend is not necessarily on the teleport list");
        g.play_chat("SYSTEM_SERVER\u{1f}Alex\u{1f}/tpaccept Alex");
        g.play_chat("SYSTEM_SERVER\u{1f}Steve\u{1f}/duel accept Steve");
        assert_eq!(g.play_tick("world=1"), "cmd tpaccept Alex");
        assert_eq!(g.play.queue.len(), 1, "the duel waits its own delay");
        let (vals, _, parts) = super::super::configs::parse(&g.snapshot("test", super::super::configs::P_ALL));
        let mut restored = Gui::new();
        restored.apply_vals(&vals, parts);
        let i = restored.mod_index("auto-accept");
        let settings = &restored.mods[i].set;
        assert!(theme::on(settings, "Separate request settings"));
        assert_eq!(theme::text(settings, "Teleport list"), "Alex");
        assert_eq!(theme::num(settings, "Duel delay"), 4000.0);
        assert_eq!(theme::pick(settings, "Teleport from"), 3);
    }

    #[test]
    fn pending_requests_cancel_on_disable_disconnect_and_server_change() {
        use super::super::data::Kind;
        let mut g = Gui::new();
        g.play.friends_loaded = true;
        accept_set(&mut g, "From", Kind::Choice { v: 0, opts: vec!["Everyone"] });
        accept_set(&mut g, "Delay", accept_num(0.0));
        g.play_tick("world=1\u{1f}server=one");
        let request = "SYSTEM_SERVER\u{1f}Steve\u{1f}/tpaccept Steve";
        g.play_chat(request);
        let i = g.mod_index("auto-accept");
        g.mods[i].on = false;
        assert_eq!(g.play_tick("world=1\u{1f}server=one"), "");
        g.mods[i].on = true;
        assert_eq!(g.play_tick("world=1\u{1f}server=one"), "");
        g.play_chat(request);
        assert_eq!(g.play_tick("world=1\u{1f}server=two"), "");
        g.play_chat(request);
        g.play_tick("world=0");
        assert_eq!(g.play_tick("world=1\u{1f}server=two"), "");
    }

    #[test]
    fn request_bursts_are_limited_and_trust_is_rechecked_before_accepting() {
        use super::super::data::Kind;
        let mut g = Gui::new();
        g.play.friends_loaded = true;
        accept_set(&mut g, "Delay", accept_num(0.0));
        accept_set(&mut g, "Accept interval", accept_num(60.0));
        g.friend_add("Steve");
        let request = "SYSTEM_SERVER\u{1f}Steve\u{1f}/tpaccept Steve";
        g.play_chat(request); g.play_chat(request);
        assert_eq!(g.play.queue.len(), 1);
        assert_eq!(g.play_tick("world=1"), "cmd tpaccept Steve");
        g.play_chat(request);
        assert!(g.play.queue.is_empty());
        g.play.accepted_at = [None; 4];
        g.play_chat(request);
        g.friend_remove("Steve");
        assert_eq!(g.play_tick("world=1"), "");
        g.friend_add("Steve");
        g.play_chat(request);
        accept_set(&mut g, "Teleport requests", Kind::Toggle(false));
        assert_eq!(g.play_tick("world=1"), "");
    }

    #[test]
    fn sprint_and_respawn_as_the_facts_say() {
        let mut g = Gui::new();
        let i = g.mod_index("auto-sprint");
        g.mods[i].on = true;
        let walk = "world=1\u{1f}fwd=1\u{1f}food=20";
        assert_eq!(g.play_tick(walk), "sprint");
        assert_eq!(g.play_tick("world=1\u{1f}fwd=1\u{1f}food=4"), "");
        assert_eq!(g.play_tick("world=1\u{1f}fwd=1\u{1f}food=20\u{1f}sneak=1"), "");
        assert_eq!(g.play_tick("world=1\u{1f}fwd=1\u{1f}food=20\u{1f}water=1"), "sprint");
        g.mods[i].on = false;
        assert_eq!(g.play_tick(walk), "");
        // dead: back at once with no delay, never in hardcore
        let j = g.mod_index("auto-respawn");
        g.mods[j].on = true;
        if let Some(s) = g.mods[j].set.iter_mut().find(|s| s.label == "Delay") {
            s.kind = super::super::data::Kind::Slider { min: 0.0, max: 3000.0, step: 100.0, v: 0.0, unit: " ms" };
        }
        assert_eq!(g.play_tick("world=1\u{1f}dead=1"), "respawn");
        g.play_tick("world=1");
        assert_eq!(g.play_tick("world=1\u{1f}dead=1\u{1f}hardcore=1"), "");
    }

    #[test]
    fn sprint_modes_distinguish_always_forward_and_smart() {
        use super::super::data::Kind;
        let mut g = Gui::new();
        let i = g.mod_index("auto-sprint");
        g.mods[i].on = true;
        let blocked = "world=1\u{1f}fwd=1\u{1f}food=0\u{1f}sneak=1\u{1f}using=1\u{1f}blind=1\u{1f}wall=1\u{1f}water=1";
        assert_eq!(g.play_tick(blocked), "");
        let set_mode = |g: &mut Gui, n| {
            if let Kind::Choice { v, .. } = &mut g.mods[i].set.iter_mut().find(|s| s.label == "Mode").unwrap().kind {
                *v = n;
            }
        };
        set_mode(&mut g, 0);
        assert_eq!(g.play_tick(blocked), "sprint");
        assert_eq!(g.play_tick("world=1"), "sprint");
        assert_eq!(g.play_tick("world=1\u{1f}screen=1"), "");
        assert_eq!(g.play_tick("world=1\u{1f}dead=1"), "");
        assert_eq!(g.play_tick("world=0"), "");
        set_mode(&mut g, 1);
        assert_eq!(g.play_tick("world=1"), "");
        assert_eq!(g.play_tick(blocked), "sprint");
        set_mode(&mut g, 2);
        assert_eq!(g.play_tick(blocked), "");
        assert_eq!(g.play_tick("world=1\u{1f}fwd=1\u{1f}food=20"), "sprint");
        if let Kind::Toggle(v) = &mut g.mods[i].set.iter_mut().find(|s| s.label == "Swim in water").unwrap().kind {
            *v = false;
        }
        assert_eq!(g.play_tick("world=1\u{1f}fwd=1\u{1f}food=20\u{1f}water=1"), "");
        set_mode(&mut g, 0);
        let saved = g.snapshot("always", super::super::configs::P_MODULES);
        let (values, _, _) = super::super::configs::parse(&saved);
        let mut restored = Gui::new();
        restored.apply_vals(&values, super::super::configs::P_MODULES);
        assert_eq!(theme::pick(&restored.mods[i].set, "Mode"), 0);
        let (old, _, _) = super::super::configs::parse("auto-sprint.#on=1\nauto-sprint.Swim in water=0\n");
        let mut legacy = Gui::new();
        legacy.apply_vals(&old, super::super::configs::P_MODULES);
        assert_eq!(theme::pick(&legacy.mods[i].set, "Mode"), 2);
    }
}
