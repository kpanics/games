//! Streamer Mode (her word, 25.10: "максимально кастомизированный, учитывать даже демку в Discord (Go Live)"): while a
//! stream or a screen share is on, what tells who and where the player is — their nick, the others' nicks, the
//! server's address, the coordinates, their skin — is not shown on their screen. On by its switch, or by itself only
//! while OBS (or Streamlabs, XSplit, Twitch Studio) runs, or while Discord streams the game (its Go Live hook in the
//! game; its overlay, which is the same hook, counts too).
//!
//! Java tells the facts each second (the player's nick, the server's address, the nicks on the server: "streamer
//! facts"); the list of what is put for what is made here, kept for the client's own words (mask.rs) and answered to
//! Java for the game's.

use super::data::Kind;
use super::i18n::{tr, trn};
use super::mask::{self, Mask};
use super::theme;
use super::Gui;
use std::collections::HashMap;
use std::time::Instant;

pub(super) const MODULE: &str = "streamer-mode";
pub(super) const HIDE: [&str; 5] = ["My nick", "Others' nicks", "Server address", "Coordinates", "My skin"];
pub(super) const WHEN: [&str; 4] = ["By its switch", "While OBS runs", "While Discord streams the game", "While either does"];

#[derive(Default)]
pub(super) struct Streamer {
    /// what was seen of the machine last, and when it was looked at
    obs: bool,
    discord: bool,
    looked: Option<Instant>,
    /// on now; on at the last facts
    pub active: bool,
    /// the others' nicks and their numbers (kept: the same player is the same number all along)
    numbers: HashMap<String, usize>,
}

/// A streaming program running (the processes' names).
fn obs_running() -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};
    const NAMES: [&str; 7] = ["obs64.exe", "obs32.exe", "obs.exe", "streamlabs obs.exe", "streamlabs desktop.exe", "xsplit.core.exe", "twitchstudio.exe"];
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == -1isize as _ {
            return false;
        }
        let mut e: PROCESSENTRY32W = std::mem::zeroed();
        e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut ok = Process32FirstW(snap, &mut e);
        let mut found = false;
        while ok != 0 && !found {
            let n = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
            let name = String::from_utf16_lossy(&e.szExeFile[..n]).to_lowercase();
            found = NAMES.contains(&name.as_str());
            ok = Process32NextW(snap, &mut e);
        }
        CloseHandle(snap);
        found
    }
}

/// Discord's hook in the game (it is put in to stream a game in Go Live, and for its overlay).
fn discord_hooked() -> bool {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    ["DiscordHook64.dll", "DiscordHook.dll"].iter().any(|n| {
        let w: Vec<u16> = n.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe { !GetModuleHandleW(w.as_ptr()).is_null() }
    })
}

/// The list of what is put for what: the player's nick for theirs, the others' for "Player N", the server's host for
/// dots.
pub(super) fn masks(hide: &[bool; 5], fake: &str, own: &str, server: &str, others: &[(String, usize)]) -> Vec<Mask> {
    let mut v = Vec::new();
    if hide[0] && !own.is_empty() {
        v.push(Mask::Word(own.to_string(), if fake.trim().is_empty() { "Streamer".into() } else { fake.trim().to_string() }));
    }
    if hide[1] {
        // (the longer first: a nick inside another's is not cut out of it)
        let mut o: Vec<&(String, usize)> = others.iter().filter(|(n, _)| n != own && n.len() >= 3).collect();
        o.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
        for (n, k) in o.into_iter().take(200) {
            v.push(Mask::Word(n.clone(), format!("{} {}", tr("Player"), k)));
        }
    }
    if hide[2] {
        // the host's last two parts (the server's own name, whatever comes before it)
        let host = server.split(':').next().unwrap_or("").trim().to_lowercase();
        let parts: Vec<&str> = host.split('.').filter(|p| !p.is_empty()).collect();
        let numeric = parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit()));
        if parts.len() >= 2 {
            let end = if numeric { host.clone() } else { parts[parts.len() - 2..].join(".") };
            v.push(Mask::Host(end, "•••".into()));
        }
    }
    v
}

impl Gui {
    fn streamer_set(&self) -> Option<&[super::data::Setting]> {
        self.mods.iter().find(|m| m.id == MODULE && m.on).map(|m| m.set.as_slice())
    }

    fn multi_on(set: &[super::data::Setting], label: &str, opt: &str) -> bool {
        match theme::find(set, label) {
            Some(Kind::Multi { on, opts }) => opts.iter().zip(on).any(|(o, on)| *on && *o == opt),
            _ => false,
        }
    }

    /// Streamer Mode on now and hiding `what` (HIDE's word).
    pub(super) fn streamer_hides(&self, what: &str) -> bool {
        self.streamer.active && self.streamer_set().is_some_and(|s| Gui::multi_on(s, "Hide", what))
    }

    /// Java's facts each second ("nick\x1fserver\x1fnicks by ,"): whether it is on, the list made; answered as "skin
    /// 0|1" and the list's lines ("w\x1ffrom\x1fto" a word, "h\x1fend\x1fto" a host).
    pub(super) fn streamer_call(&mut self, facts: &str) -> String {
        let mut f = facts.split('\u{1f}');
        let (own, server, names) = (f.next().unwrap_or(""), f.next().unwrap_or(""), f.next().unwrap_or(""));
        let Some(set) = self.streamer_set().map(|s| s.to_vec()) else {
            self.streamer.active = false;
            mask::set(Vec::new());
            return String::new();
        };
        // the machine looked at now and then (a snapshot of its processes is not for every second)
        let when = theme::pick(&set, "Turns on");
        let always = when == 0;
        let (by_obs, by_discord) = (when == 1 || when == 3, when >= 2);
        if !always && self.streamer.looked.is_none_or(|t| t.elapsed().as_secs_f32() > 4.0) {
            self.streamer.looked = Some(Instant::now());
            self.streamer.obs = by_obs && obs_running();
            self.streamer.discord = by_discord && discord_hooked();
        }
        let active = always || (by_obs && self.streamer.obs) || (by_discord && self.streamer.discord);
        if active && !self.streamer.active && !always && theme::on(&set, "Say when it turns on") {
            let why = if self.streamer.obs { tr("OBS is running") } else { tr("Discord streams the game") };
            let t = self.clock.elapsed().as_secs_f32();
            self.hud.note(tr("Streamer Mode").to_string(), trn("On: {why}", &[("why", why.to_string())]), t);
        }
        self.streamer.active = active;
        if !active {
            mask::set(Vec::new());
            return String::new();
        }
        let hide: [bool; 5] = std::array::from_fn(|i| Gui::multi_on(&set, "Hide", HIDE[i]));
        // the others' numbers: each new nick the next one
        let mut others = Vec::new();
        for n in names.split(',').filter(|n| !n.is_empty() && *n != own) {
            let next = self.streamer.numbers.len() + 1;
            let k = *self.streamer.numbers.entry(n.to_string()).or_insert(next);
            others.push((n.to_string(), k));
        }
        let list = masks(&hide, theme::text(&set, "Shown nick"), own, server, &others);
        mask::set(list.clone());
        let mut out = vec![format!("skin {}", hide[4] as i32)];
        for m in list {
            out.push(match m {
                Mask::Word(a, b) => format!("w\u{1f}{a}\u{1f}{b}"),
                Mask::Host(a, b) => format!("h\u{1f}{a}\u{1f}{b}"),
            });
        }
        out.join("\n")
    }

    /// Its mark at the top of the screen while it is on (when asked for): a red dot and its name.
    pub(super) fn streamer_draw(&mut self, c: &mut super::draw::Canvas, w: f32) {
        let on = self.streamer.active && self.streamer_set().is_some_and(|s| theme::on(s, "Mark on the screen"));
        let k = self.an(super::ak(super::A_HOV, 43_000), if on { 1.0 } else { 0.0 }, 9.0, 0.0);
        if k < 0.01 {
            return;
        }
        use super::text::{fonts, Face};
        let r = self.r;
        let label = tr("Streamer Mode");
        let tw = fonts().width(Face::Medium, 0.7 * r, label);
        let pw = tw + 2.6 * r;
        let y = 0.6 * r - (1.0 - k) * 0.8 * r;
        let b = [(w - pw) / 2.0, y, (w + pw) / 2.0, y + 1.7 * r];
        c.alpha = k;
        c.rect(b, 0.85 * r, super::draw::grey(0.0, 0.5));
        // (the dot breathes, as a recording's)
        let pulse = 0.65 + 0.35 * (self.now * 2.4).sin().abs();
        let d = 0.26 * r;
        let cx = b[0] + 0.95 * r;
        let cy = (b[1] + b[3]) / 2.0;
        c.rect([cx - d, cy - d, cx + d, cy + d], d, [0.95, 0.3, 0.32, pulse]);
        c.text(cx + 0.55 * r, cy + 0.26 * r, 0.7 * r, Face::Medium, label, super::draw::grey(1.0, 0.92));
        c.alpha = 1.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_hides_what_is_asked() {
        let all = [true; 5];
        let l = masks(&all, "", "Lunuca", "mc.funtime.su:25565", &[("Steve".into(), 1), ("Lunuca".into(), 9), ("Al".into(), 2)]);
        assert_eq!(l[0], Mask::Word("Lunuca".into(), "Streamer".into()));
        // the others numbered (not the player, not one too short to be told from a word)
        assert!(l.iter().any(|m| matches!(m, Mask::Word(a, b) if a == "Steve" && b.ends_with(" 1"))));
        assert!(!l.iter().any(|m| matches!(m, Mask::Word(a, _) if a == "Al")));
        assert!(l.contains(&Mask::Host("funtime.su".into(), "•••".into())));
        // an address of numbers is hidden whole; nothing asked, nothing hidden
        let ip = masks(&[false, false, true, false, false], "", "x", "45.12.1.9", &[]);
        assert_eq!(ip, vec![Mask::Host("45.12.1.9".into(), "•••".into())]);
        assert!(masks(&[false; 5], "Me", "Lunuca", "a.b", &[]).is_empty());
    }

    #[test]
    fn it_is_on_by_its_switch_and_its_numbers_stay() {
        let _one = super::super::mask::TESTS.lock().unwrap_or_else(|e| e.into_inner());
        let mut g = Gui::new();
        assert_eq!(g.streamer_call("Lunuca\u{1f}mc.funtime.su\u{1f}Steve,Alex"), "");
        let i = g.mods.iter().position(|m| m.id == MODULE).unwrap();
        g.mods[i].on = true;
        // (the defaults: always on, the nick and the address hidden)
        let a = g.streamer_call("Lunuca\u{1f}mc.funtime.su\u{1f}Steve,Alex");
        assert!(a.starts_with("skin 0") && a.contains("w\u{1f}Lunuca\u{1f}") && a.contains("h\u{1f}funtime.su"), "{a}");
        assert!(g.streamer_hides("My nick") && !g.streamer_hides("My skin"));
        g.mods[i].on = false;
        assert_eq!(g.streamer_call("Lunuca\u{1f}\u{1f}"), "");
        assert!(!g.streamer.active);
    }
}
