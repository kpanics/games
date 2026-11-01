//! Configs: one kind of them for everything. A config is a snapshot of chosen parts of the client (the modules and
//! their settings, the binds, the theme with the menu's words, the menu's own settings, the HUD with where its
//! elements are), kept on this computer and shared as a code. A theme's code is a config of the theme and the HUD.
//! Loading or pasting one asks which of its parts to take. Also here: the cards that take a word typed (a config's
//! name, the confirmation before every setting goes back), and the little word that says what just happened.

use super::data::{Cat, Kind, Setting};
use super::draw::{grey, mix, Canvas};
use super::i18n::{self, tr};
use super::text::{fonts, Face};
use super::theme::{self, pal};
use super::{ak, binds, ease_out, smooth, Card, Gui, Hit, A_HOV, A_POP};
use std::collections::HashMap;

pub const P_MODULES: u8 = 1;
pub const P_BINDS: u8 = 2;
pub const P_THEME: u8 = 4;
pub const P_MENU: u8 = 8;
pub const P_HUD: u8 = 16;
pub const P_ALL: u8 = 31;
/// The parts in their order, with their names.
pub const PARTS: [(u8, &str); 5] = [(P_MODULES, "Modules"), (P_BINDS, "Binds"), (P_THEME, "Theme"), (P_MENU, "Menu"), (P_HUD, "HUD")];

/// A config on this computer: its name, its file, what it holds, when it was saved (s since 1970), its text.
#[derive(Clone)]
pub struct Saved {
    pub name: String,
    pub file: std::path::PathBuf,
    pub parts: u8,
    pub when: u64,
    pub text: String,
}

// ---------------------------------------------------------------------------------------------- codes
const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn b64(data: &[u8]) -> String {
    let mut o = String::with_capacity(data.len() * 4 / 3 + 4);
    for ch in data.chunks(3) {
        let n = (ch[0] as u32) << 16 | (*ch.get(1).unwrap_or(&0) as u32) << 8 | *ch.get(2).unwrap_or(&0) as u32;
        let k = ch.len() + 1;
        for i in 0..k {
            o.push(B64[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    o
}

fn unb64(s: &str) -> Option<Vec<u8>> {
    let mut o = Vec::new();
    let mut buf = 0u32;
    let mut bits = 0;
    for ch in s.bytes() {
        let v = B64.iter().position(|&b| b == ch)? as u32;
        buf = buf << 6 | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            o.push((buf >> bits & 255) as u8);
        }
    }
    Some(o)
}

/// A text as a code to paste anywhere: its kind's prefix and the text squeezed.
pub fn squeeze(prefix: &str, text: &str) -> String {
    format!("{prefix}{}", b64(&miniz_oxide::deflate::compress_to_vec(text.as_bytes(), 9)))
}

/// A code of a kind back to its text (None: it is not one).
pub fn unsqueeze(prefix: &str, code: &str) -> Option<String> {
    let clean: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    let body = clean.strip_prefix(prefix)?;
    let raw = miniz_oxide::inflate::decompress_to_vec_with_limit(&unb64(body)?, 4 << 20).ok()?;
    String::from_utf8(raw).ok()
}

/// A config's text as a code to paste anywhere: "RCD1-" and the text squeezed.
pub fn encode(text: &str) -> String {
    squeeze("RCD1-", text)
}

/// A code back to its text (None: it is not one of ours).
pub fn decode(code: &str) -> Option<String> {
    unsqueeze("RCD1-", code)
}

/// A config's text: its lines as keys and values, and what its head says (name, parts).
pub fn parse(text: &str) -> (HashMap<String, String>, String, u8) {
    let mut vals = HashMap::new();
    let mut name = String::new();
    let mut parts = P_ALL;
    for l in text.lines() {
        if let Some(n) = l.strip_prefix("#name=") {
            name = n.to_string();
            continue;
        }
        if let Some(p) = l.strip_prefix("#parts=") {
            parts = p.parse().unwrap_or(P_ALL);
            continue;
        }
        // (the rest of the head: its servers, cfgserver.rs)
        if l.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = l.split_once('=') {
            vals.insert(k.to_string(), v.to_string());
        }
    }
    (vals, name, parts)
}

/// A file's name of a config's name (what Windows allows).
fn file_name(name: &str) -> String {
    let s: String = name.chars().map(|c| if "\\/:*?\"<>|".contains(c) || c.is_control() { '_' } else { c }).collect();
    let s = s.trim().trim_end_matches('.').to_string();
    if s.is_empty() { "config".into() } else { s }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// When, as people say it: just now, minutes, hours, yesterday, a date.
pub(super) fn when_text(t: u64) -> String {
    let d = now_secs().saturating_sub(t);
    if d < 60 {
        return tr("just now").to_string();
    }
    if d < 3600 {
        return i18n::trn("{n} min ago", &[("n", (d / 60).to_string())]);
    }
    if d < 86_400 {
        return i18n::trn("{n} h ago", &[("n", (d / 3600).to_string())]);
    }
    if d < 2 * 86_400 {
        return tr("yesterday").to_string();
    }
    i18n::trn("{n} days ago", &[("n", (d / 86_400).to_string())])
}

/// The words that confirm putting everything back, in the languages people may type them in.
const CONFIRM: &[&str] = &[
    "подтверждаю", "підтверджую", "растаймын", "confirm", "i confirm", "confirmo", "confirme", "je confirme", "bestätige", "ich bestätige", "potwierdzam",
    "onaylıyorum", "onayliyorum", "potvrzuji", "patvirtinu", "apstiprinu", "kinnitan", "megerősítem", "confermo", "bevestig", "确认", "確認", "확인", "تأكيد", "אישור",
    "tasdiqlayman", "тасдиқлайман", "ырастайм", "подтверждаю!", "подтверждаю.",
];

pub fn confirmed(word: &str) -> bool {
    let w = word.trim().to_lowercase();
    !w.is_empty() && CONFIRM.contains(&w.as_str())
}

impl Gui {
    // ------------------------------------------------------------------ what is kept, by parts
    /// Every setting of the client as lines (the settings' file).
    pub(super) fn dump(&self) -> String {
        let mut out = String::new();
        let mut put = |id: &str, label: &str, k: &Kind| {
            let v = match k {
                Kind::Toggle(v) => (*v as u8).to_string(),
                Kind::Slider { v, .. } => v.to_string(),
                Kind::Choice { v, opts } => opts[*v].to_string(),
                Kind::Multi { on, opts } => opts.iter().zip(on).filter(|p| *p.1).map(|p| *p.0).collect::<Vec<_>>().join("|"),
                Kind::Colour(v) => format!("{:06x}/{}/{}/{}", v.rgb, v.a, v.chroma, v.auto as u8),
                Kind::Key(v) | Kind::Text(v) => v.clone(),
                _ => return,
            };
            out.push_str(&format!("{id}.{label}={v}\n"));
        };
        for m in &self.mods {
            if !m.plain {
                put(m.id, "#on", &Kind::Toggle(m.on));
                put(m.id, "#binds", &Kind::Text(binds::save(&m.binds)));
            }
            for s in &m.set {
                put(m.id, s.label, &s.kind);
            }
        }
        for s in &self.client {
            put("menu", s.label, &s.kind);
        }
        for s in &self.theme {
            put("theme", s.label, &s.kind);
        }
        let owners: Vec<(&str, &Vec<Setting>)> = self.mods.iter().map(|m| (m.id, &m.set)).chain([("menu", &self.client), ("theme", &self.theme)]).collect();
        for (id, set) in owners {
            for s in set.iter().filter(|s| !s.binds.is_empty()) {
                out.push_str(&format!("{id}.{}#binds={}\n", s.label, binds::save(&s.binds)));
            }
        }
        out.push_str(&format!("{}.#swaps={}\n", super::swap::MODULE, super::swap::save(&self.swaps)));
        out.push_str(&format!("hudx.list={}\n", self.custom_list()));
        out.push_str(&self.hud.save_places());
        for (k, v) in i18n::words_now() {
            out.push_str(&format!("text.{k}={}\n", v.replace('\n', " ")));
        }
        out
    }

    /// Which part a line of the settings belongs to.
    pub(super) fn part_of(&self, key: &str) -> u8 {
        if key.contains("#binds") || key.ends_with(".#bind") {
            return P_BINDS;
        }
        let owner = key.split('.').next().unwrap_or("");
        match owner {
            "theme" | "text" => P_THEME,
            "menu" => P_MENU,
            "hud" | "hudx" => P_HUD,
            id if id.starts_with("x-") => P_HUD,
            id => match self.mods.iter().find(|m| m.id == id) {
                Some(m) if m.cat == Cat::HudEl => P_HUD,
                Some(_) => P_MODULES,
                None => 0,
            },
        }
    }

    /// The client's chosen parts as a config's text, with its head.
    pub(super) fn snapshot(&self, name: &str, parts: u8) -> String {
        let mut out = format!("#name={name}\n#parts={parts}\n");
        for l in self.dump().lines() {
            let key = l.split_once('=').map_or(l, |p| p.0);
            if self.part_of(key) & parts != 0 {
                out.push_str(l);
                out.push('\n');
            }
        }
        out
    }

    /// Settings from lines, only those of the parts asked (a part's settings missing from them stay as they are; its
    /// binds missing go: a snapshot holds them all).
    pub(super) fn apply_vals(&mut self, vals: &HashMap<String, String>, parts: u8) {
        self.apply_vals_scoped(vals, parts, None);
    }

    pub(super) fn apply_vals_scoped(&mut self, vals: &HashMap<String, String>, parts: u8, owners: Option<&[String]>) {
        let takes = |id: &str| owners.is_none_or(|ids| ids.iter().any(|owner| owner == id));
        let val = |id: &str, s: &mut Setting| {
            let key = format!("{id}.{}", s.label);
            // (saved under the name it had before, when it was renamed)
            let Some(v) = vals.get(&key).or_else(|| super::data::RENAMED.iter().find(|q| q.0 == key).and_then(|q| vals.get(q.1))) else { return };
            match &mut s.kind {
                Kind::Toggle(x) => *x = v == "1",
                Kind::Slider { v: x, min, max, .. } => *x = v.parse::<f32>().ok().filter(|v| v.is_finite()).unwrap_or(*x).clamp(*min, *max),
                Kind::Choice { v: x, opts } => *x = opts.iter().position(|o| o == v).unwrap_or(*x),
                Kind::Multi { on, opts } => {
                    for (i, o) in opts.iter().enumerate() {
                        on[i] = v.split('|').any(|p| p == *o);
                    }
                }
                Kind::Colour(x) => {
                    let mut it = v.split('/');
                    if let Some(rgb) = it.next().and_then(|h| u32::from_str_radix(h, 16).ok()) {
                        x.rgb = rgb & 0xffffff;
                    }
                    if let Some(a) = it.next().and_then(|a| a.parse::<f32>().ok()).filter(|v| v.is_finite()) {
                        x.a = a.clamp(0.0, 1.0);
                    }
                    if let Some(c) = it.next().and_then(|c| c.parse::<f32>().ok()).filter(|v| v.is_finite()) {
                        x.chroma = c.clamp(0.0, 3.0);
                    }
                    if let Some(a) = it.next() {
                        x.auto = a == "1";
                    }
                }
                Kind::Key(x) | Kind::Text(x) => *x = v.clone(),
                _ => {}
            }
        };
        let bnd = parts & P_BINDS != 0;
        // the player's own elements first: made (or gone) as the HUD's part says, before their settings are read
        if parts & P_HUD != 0 && owners.is_none() {
            let list = vals.get("hudx.list").cloned().unwrap_or_default();
            self.custom_ensure(&list);
        }
        for m in self.mods.iter_mut() {
            if !takes(m.id) { continue; }
            let mp = if m.cat == Cat::HudEl || m.id == "hud" { P_HUD } else { P_MODULES };
            let take = parts & mp != 0;
            if take {
                if let Some(v) = vals.get(&format!("{}.#on", m.id)) {
                    m.on = v == "1";
                }
            }
            if bnd {
                if let Some(v) = vals.get(&format!("{}.#binds", m.id)) {
                    m.binds = binds::load(v);
                } else if let Some(v) = vals.get(&format!("{}.#bind", m.id)) {
                    m.binds = if v.is_empty() { Vec::new() } else { vec![binds::Bind::one(v)] };
                }
            }
            for s in m.set.iter_mut() {
                if take {
                    val(m.id, s);
                }
                if bnd {
                    s.binds = vals.get(&format!("{}.{}#binds", m.id, s.label)).map_or(Vec::new(), |v| binds::load(v));
                }
            }
        }
        for (owner, list, p) in [("menu", &mut self.client, P_MENU), ("theme", &mut self.theme, P_THEME)] {
            if !takes(owner) { continue; }
            for s in list.iter_mut() {
                if parts & p != 0 {
                    val(owner, s);
                }
                if bnd {
                    s.binds = vals.get(&format!("{owner}.{}#binds", s.label)).map_or(Vec::new(), |v| binds::load(v));
                }
            }
        }
        if parts & P_HUD != 0 {
            if let Some(ids) = owners {
                let places = vals.iter().filter(|(k, _)| ids.iter().any(|id| **k == format!("hud.{id}.place"))).map(|(k, v)| (k.clone(), v.clone())).collect();
                self.hud.load_places(&places);
            } else {
                self.hud.load_places(vals);
            }
        }
        // Auto Swap's swaps go with its module
        if parts & P_MODULES != 0 && takes(super::swap::MODULE) {
            if let Some(v) = vals.get(&format!("{}.#swaps", super::swap::MODULE)) {
                self.swaps = super::swap::load(v);
                self.swap_held.clear();
            }
        }
        // Chams' liquid: saved with the preset it was set from, it stays as it was; saved before there was one, its
        // preset sets all of the substance (only its colour was there)
        if parts & P_MODULES != 0 && takes("chams") {
            let liquid = self.mods.iter().find(|m| m.id == "chams").and_then(|m| m.set.iter().find(|s| s.label == "Liquid")).map(|s| s.kind.clone());
            self.liquid_seen = match liquid {
                Some(Kind::Choice { v, .. }) if vals.contains_key("chams.Liquid base") => v,
                _ => usize::MAX,
            };
            self.liquid_follow();
        }
        if parts & P_MODULES != 0 && takes(super::swap::MODULE) {
            // Auto Swap's timing: its preset as read (one saved in ticks before the presets, in ms)
            self.timing_read(vals);
        }
        // the player's words are the theme's: taken all, or none
        if parts & P_THEME != 0 && owners.is_none() {
            i18n::clear_words();
            for (k, v) in vals {
                if let Some(key) = k.strip_prefix("text.") {
                    i18n::set_word(key, Some(v));
                }
            }
        }
    }

    // ------------------------------------------------------------------ the configs on this computer
    fn cfg_dir(&self) -> Option<std::path::PathBuf> {
        self.dir.as_ref().map(|d| d.join("cursed-recode").join("configs"))
    }

    /// The configs on the disk, newest first.
    pub(super) fn cfg_scan(&mut self) {
        let selected = if let Some(Card::Cfg { i, .. }) = &self.card {
            self.saved.get(*i).map(|s| s.file.clone())
        } else {
            None
        };
        let Some(dir) = self.cfg_dir() else { return };
        let mut v = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let path = e.path();
                if path.extension().and_then(|x| x.to_str()) != Some("cfg") {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&path) else { continue };
                let (_, name, parts) = parse(&text);
                let when = e
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_secs());
                let name = if name.is_empty() {
                    path.file_stem().and_then(|s| s.to_str()).unwrap_or("config").to_string()
                } else {
                    name
                };
                v.push(Saved {
                    name,
                    file: path,
                    parts,
                    when,
                    text,
                });
            }
        }
        v.sort_by(|a, b| b.when.cmp(&a.when));
        if self.saved.len() != v.len() || self.saved.iter().zip(&v).any(|(a, b)| a.file != b.file || a.text != b.text) {
            super::cloud::changed();
        }
        self.saved = v;
        if let Some(file) = selected {
            if let Some(index) = self.saved.iter().position(|s| s.file == file) {
                if let Some(Card::Cfg { i, .. }) = &mut self.card {
                    *i = index;
                }
            } else {
                self.card = None;
            }
        }
    }

    /// A config written (a new one, or one saved over: its file kept).
    pub(super) fn cfg_write(&mut self, name: &str, parts: u8, over: Option<usize>) -> bool {
        let Some(dir) = self.cfg_dir() else { return false };
        if std::fs::create_dir_all(&dir).is_err() {
            self.toast(tr("Could not save config").into());
            return false;
        }
        let previous = over.and_then(|i| self.saved.get(i)).map(|s| s.file.clone());
        self.profile_keep();
        self.cfg_scan();
        let saved = previous.as_ref().and_then(|p| self.saved.iter().find(|s| &s.file == p)).cloned();
        if over.is_some() && saved.is_none() {
            self.toast(tr("Could not save config").into());
            return false;
        }
        let servers = saved.as_ref().map_or(Vec::new(), |s| super::cfgserver::servers_of(&s.text));
        let modes = saved
            .as_ref()
            .filter(|_| parts & super::cfgserver::INTERFACE_PARTS != 0)
            .map_or(Vec::new(), |s| super::cfgserver::modes_of(&s.text));
        let text = super::cfgserver::with_modes(&super::cfgserver::with_servers(&self.snapshot(name, parts), &servers), &modes);
        let file = match &saved {
            Some(s) if s.name == name => s.file.clone(),
            Some(s) => {
                let file = dir.join(format!("{}.cfg", file_name(name)));
                if file.exists() {
                    if matches!((file.canonicalize(),s.file.canonicalize()),(Ok(a),Ok(b)) if a==b) {
                        s.file.clone()
                    } else {
                        self.toast(tr("Config name already exists").into());
                        return false;
                    }
                } else {
                    file
                }
            }
            None => {
                let mut f = dir.join(format!("{}.cfg", file_name(name)));
                let mut n = 2;
                while f.exists() {
                    f = dir.join(format!("{} {n}.cfg", file_name(name)));
                    n += 1;
                }
                f
            }
        };
        let reload = previous.as_ref().is_some_and(|file| self.profiles_use(file));
        if reload {
            self.profile_leave(true);
        }
        let mut result = super::cfgserver::write_config(&file, &text);
        if result.is_ok() {
            if let Some(old) = previous.as_ref().filter(|old| **old != file) {
                if let Err(error) = std::fs::remove_file(old) {
                    let _ = std::fs::remove_file(&file);
                    result = Err(error);
                }
            }
        }
        if result.is_ok() {
            self.loaded_cfg = Some(name.to_string());
        }
        self.cfg_scan();
        if reload {
            self.profiles_reload();
        }
        if result.is_err() {
            self.toast(tr("Could not save config").into());
        }
        result.is_ok()
    }

    pub(super) fn cfg_delete(&mut self, file: &std::path::Path) -> bool {
        let name = self.saved.iter().find(|s| s.file == file).map(|s| s.name.clone());
        let reload = self.profiles_use(file);
        if reload {
            self.profile_leave(true);
        }
        let result = std::fs::remove_file(file);
        if result.is_ok() && self.loaded_cfg == name {
            self.loaded_cfg = None;
        }
        self.cfg_scan();
        if reload {
            self.profiles_reload();
        }
        if result.is_err() {
            self.toast(tr("Could not delete config").into());
        }
        result.is_ok()
    }

    pub(super) fn cfg_load(&mut self, text: &str, parts: u8, name: &str) {
        if self.profile.is_some() || self.interface_profile.is_some() {
            self.profile_leave(false);
        }
        let (vals, _, have) = parse(text);
        self.apply_vals(&vals, parts & have);
        if parts & P_MODULES != 0 {
            self.loaded_cfg = Some(name.to_string());
        }
        self.save();
        self.switch_sound(true, 0.6);
        self.toast(i18n::trn("{name} loaded", &[("name", name.to_string())]));
    }

    /// A short word over the menu (what just happened), fading.
    pub(super) fn toast(&mut self, s: String) {
        self.toast = Some((s, self.now));
    }

    pub(super) fn copy(&mut self, text: &str) {
        self.outbox.push(format!("copy\t{}", text.replace('\n', " ")));
    }

    // ------------------------------------------------------------------ the page
    pub(super) fn configs_page(&mut self, c: &mut Canvas, area: [f32; 4]) {
        let r = self.r;
        let (a, dyv) = self.rise_c(0.22, 0.0, false);
        c.alpha = a;
        let w = area[2] - area[0];
        let top = area[1] - self.scroll + dyv;
        // now: what the client is, and what can be done with it
        let nh = 10.6 * r;
        let nr = [area[0], top, area[2], top + nh];
        c.rect_lit(nr, r, pal().f2, grey(1.0, 0.04), 2.0);
        c.text(nr[0] + 1.15 * r, nr[1] + 1.95 * r, 0.95 * r, Face::Semi, tr("Your client now"), pal().t1);
        let on = self.mods.iter().filter(|m| !m.plain && m.on && !matches!(m.cat, Cat::HudEl | Cat::Hud)).count();
        let nb: usize = self.all_binds().iter().map(|b| b.1.len()).sum();
        let hud = self.mods.iter().filter(|m| m.cat == Cat::HudEl && !m.plain && m.on).count();
        let line = i18n::trn("{on} modules on · {b} binds · {h} HUD elements", &[("on", on.to_string()), ("b", nb.to_string()), ("h", hud.to_string())]);
        c.text(nr[0] + 1.15 * r, nr[1] + 3.15 * r, 0.76 * r, Face::Regular, &line, pal().t3);
        if let Some(n) = self.loaded_cfg.clone() {
            let lw = fonts().width(Face::Regular, 0.76 * r, &line);
            let from = if self.profile.is_some() && !self.cfg_host.is_empty() {
                i18n::trn("from {name}, the profile of {server}", &[("name", n), ("server", self.cfg_host.clone())])
            } else {
                i18n::trn("from {name}", &[("name", n)])
            };
            c.text(nr[0] + 1.15 * r + lw + 0.6 * r, nr[1] + 3.15 * r, 0.76 * r, Face::Medium, &from, pal().t2);
        }
        // the buttons: save it, its code, a code pasted
        let acts = [(tr("Save as a config"), true, 0u16), (tr("Copy its code"), false, 1), (tr("Paste a code"), false, 2)];
        let mut bx = nr[0] + 1.15 * r;
        let by = nr[1] + 4.35 * r;
        for (label, main, id) in acts {
            let bw = fonts().width(Face::Semi, 0.78 * r, label) + 2.0 * r;
            let b = [bx, by, bx + bw, by + 2.15 * r];
            let hit = Hit::Cfg(0, id);
            let hv = self.an(ak(A_HOV, 14_000 + id as usize), if self.hover == hit { 1.0 } else { 0.0 }, 12.0, 0.0);
            if main {
                c.glow([(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0], bw * 0.55, theme::fade(pal().accent, 0.08 + 0.08 * hv));
                c.rect(b, 0.62 * r, mix(theme::fade(pal().accent, 0.9), pal().accent, hv));
                c.text_c((b[0] + b[2]) / 2.0, b[1] + 1.42 * r, 0.78 * r, Face::Semi, label, grey(0.04, 1.0));
            } else {
                c.rect(b, 0.62 * r, mix(pal().f2, pal().f4, hv));
                c.text_c((b[0] + b[2]) / 2.0, b[1] + 1.42 * r, 0.78 * r, Face::Semi, label, pal().t1);
            }
            self.hits.push((b, hit));
            bx += bw + 0.5 * r;
        }
        let cloud = super::cloud::view();
        let status = fonts().fit(Face::Regular, 0.74 * r, tr(cloud.status), w - 11.5 * r);
        c.text(nr[0] + 1.15 * r, nr[1] + 8.35 * r, 0.74 * r, Face::Regular, &status, pal().t3);
        let button = [nr[2] - 9.0 * r, nr[1] + 7.0 * r, nr[2] - 1.15 * r, nr[1] + 9.3 * r];
        let hover = self.an(ak(A_HOV, 14_003), if self.hover == Hit::Cfg(0, 3) { 1.0 } else { 0.0 }, 12.0, 0.0);
        c.rect(button, 0.62 * r, mix(pal().f2, pal().f4, hover));
        let label = fonts().fit(Face::Semi, 0.78 * r, tr(if cloud.conflict { "Resolve conflict" } else { "Cloud" }), button[2] - button[0] - r);
        c.text_c((button[0] + button[2]) * 0.5, button[1] + 1.5 * r, 0.78 * r, Face::Semi, &label, pal().t1);
        self.hits.push((button, Hit::Cfg(0, 3)));
        // the saved ones
        let ly = nr[3] + 1.9 * r;
        c.text(area[0] + 0.3 * r, ly, 0.7 * r, Face::Semi, &i18n::trn("Saved · {n}", &[("n", self.saved.len().to_string())]), pal().t4);
        let gy = ly + 0.8 * r;
        let cols = if w > 34.0 * r { 2 } else { 1 };
        let gap = 0.62 * r;
        let cw = (w - gap * (cols - 1) as f32) / cols as f32;
        let ch = 5.9 * r;
        let n = self.saved.len();
        if n == 0 {
            let er = [area[0], gy, area[2], gy + 4.4 * r];
            c.rect(er, r, pal().f1);
            c.text(er[0] + 1.15 * r, er[1] + 1.9 * r, 0.86 * r, Face::Semi, tr("No configs yet"), pal().t2);
            c.text(er[0] + 1.15 * r, er[1] + 3.15 * r, 0.74 * r, Face::Regular, tr("Save the client as it is now, or paste a friend's code."), pal().t4);
        }
        for i in 0..n {
            let (col, row) = (i % cols, i / cols);
            let q = [area[0] + col as f32 * (cw + gap), gy + row as f32 * (ch + gap), area[0] + col as f32 * (cw + gap) + cw, gy + row as f32 * (ch + gap) + ch];
            let (k, ddy) = self.rise_c(0.26 + (i as f32 * 0.025).min(0.3), (i as f32 * 0.01).min(0.1), false);
            c.alpha = k;
            let q = [q[0], q[1] + ddy - dyv, q[2], q[3] + ddy - dyv];
            let s = self.saved[i].clone();
            let loaded = self.loaded_cfg.as_deref() == Some(s.name.as_str()) || self.interface_uses(&s.file);
            let hit = Hit::Cfg(1, i as u16);
            let hv = self.an(ak(A_HOV, 14_100 + i), if self.hover == hit { 1.0 } else { 0.0 }, 12.0, 0.0);
            c.rect_lit(q, r, mix(pal().f1, pal().f2, hv), grey(1.0, if loaded { 0.08 } else { 0.0 }), 2.0);
            let nm = fonts().fit(Face::Semi, 0.9 * r, &s.name, cw - 2.3 * r - if loaded { 4.0 * r } else { 0.0 });
            let nw = c.text(q[0] + 1.15 * r, q[1] + 1.85 * r, 0.9 * r, Face::Semi, &nm, pal().t1);
            if loaded {
                c.text(q[0] + 1.15 * r + nw + 0.55 * r, q[1] + 1.85 * r, 0.66 * r, Face::Medium, tr("loaded"), pal().t4);
            }
            let mut servers = super::cfgserver::servers_of(&s.text);
            servers.extend(super::cfgserver::modes_of(&s.text).into_iter().map(|m| super::adapt::MODES[m].to_string()));
            let when = if servers.is_empty() { when_text(s.when) } else { format!("{} · {}", when_text(s.when), servers.join(", ")) };
            c.text(q[0] + 1.15 * r, q[1] + 2.95 * r, 0.7 * r, Face::Regular, &fonts().fit(Face::Regular, 0.7 * r, &when, cw - 2.3 * r), pal().t4);
            // what it holds
            let mut px = q[0] + 1.15 * r;
            for (bit, label) in PARTS {
                if s.parts & bit == 0 {
                    continue;
                }
                let lw = fonts().width(Face::Medium, 0.62 * r, tr(label)) + 0.9 * r;
                c.rect([px, q[1] + 3.75 * r, px + lw, q[1] + 4.95 * r], 0.35 * r, pal().f2);
                c.text(px + 0.45 * r, q[1] + 4.6 * r, 0.62 * r, Face::Medium, tr(label), pal().t2);
                px += lw + 0.3 * r;
            }
            self.hits.push((q, hit));
        }
        let bottom = gy + n.div_ceil(cols).max(1) as f32 * (ch + gap) + self.scroll - dyv;
        self.scroll_clamp(area, bottom - self.scroll);
        c.alpha = 1.0;
    }

    pub(super) fn configs_press(&mut self, h: Hit, at: [f32; 4]) -> bool {
        match h {
            Hit::Cfg(0, 0) => {
                self.card = Some(Card::NewCfg { at });
                self.card_field = i18n::trn("Config {n}", &[("n", (self.saved.len() + 1).to_string())]);
                self.cfg_parts = P_ALL;
                self.card_open();
            }
            Hit::Cfg(0, 3) => self.cloud_open(at),
            Hit::Cfg(0, 1) => {
                let code = encode(&self.snapshot(&self.loaded_cfg.clone().unwrap_or_else(|| self.player.clone()), P_ALL));
                self.copy(&code);
                self.toast(tr("The code is copied: paste it to a friend").to_string());
                self.sound_tick(0.7);
            }
            Hit::Cfg(0, 2) => {
                self.paste_for = P_ALL;
                self.outbox.push("paste".into());
                self.sound_tick(0.5);
            }
            Hit::Cfg(1, i) => {
                let i = i as usize;
                if let Some(s) = self.saved.get(i) {
                    self.cfg_parts = s.parts;
                    self.card_field = s.name.clone();
                    self.card = Some(Card::Cfg { i, at });
                    self.card_open();
                }
            }
            _ => return false,
        }
        true
    }

    fn card_open(&mut self) {
        self.card_at = self.now;
        self.card_arm = false;
        self.anim.remove(&ak(A_POP, 2));
        self.pop = None;
        self.pick = None;
        self.sound_tick(0.6);
    }

    /// The clipboard came (a code pasted): the card of what it holds, or a word that it is not a code.
    pub(super) fn paste_code(&mut self, text: &str) {
        let parts_for = std::mem::take(&mut self.paste_for);
        if parts_for == 0 {
            return;
        }
        if parts_for == P_ALL && text.trim_start().starts_with("RCDM1-") {
            self.module_code_paste(text, None);
            return;
        }
        match decode(text) {
            Some(t) => {
                let (_, name, have) = parse(&t);
                let have = have & parts_for;
                if have == 0 {
                    self.toast(tr("This code holds nothing of that").to_string());
                    return;
                }
                self.card_field = if name.is_empty() { tr("A friend's config").to_string() } else { name };
                self.cfg_parts = have;
                let at = [self.mouse[0], self.mouse[1], self.mouse[0], self.mouse[1]];
                self.card = Some(Card::Import { text: t, have, at });
                self.card_open();
            }
            None => {
                self.toast(tr("That is not a Recode code").to_string());
                self.sound_tick(0.4);
            }
        }
    }

    // ------------------------------------------------------------------ the cards with a word typed
    /// A text field: its words, a placeholder, the caret.
    pub(super) fn field(&mut self, c: &mut Canvas, f: [f32; 4], text: &str, ph: &str) {
        let r = self.r;
        c.rect(f, 0.62 * r, pal().f2);
        let base = (f[1] + f[3]) / 2.0 + 0.28 * r;
        if text.is_empty() {
            c.text(f[0] + 0.8 * r, base, 0.8 * r, Face::Regular, ph, pal().t4);
        }
        let room = f[2] - f[0] - 1.6 * r;
        let tw = fonts().width(Face::Regular, 0.8 * r, text);
        let shift = (tw - room).max(0.0);
        c.push_clip([f[0] + 0.4 * r, f[1], f[2] - 0.4 * r, f[3]]);
        c.text(f[0] + 0.8 * r - shift, base, 0.8 * r, Face::Regular, text, pal().t1);
        if (self.now * 2.0).fract() < 0.55 {
            let cx = f[0] + 0.85 * r + tw - shift;
            c.rect([cx, f[1] + 0.5 * r, cx + 1.5, f[3] - 0.5 * r], 0.0, pal().accent);
        }
        c.pop_clip();
    }

    /// A button of a card: the main one lit; `on`: it can be pressed.
    #[allow(clippy::too_many_arguments)]
    fn card_btn(&mut self, c: &mut Canvas, b: [f32; 4], label: &str, main: bool, hit: Hit, on: bool, key: usize) {
        let r = self.r;
        let hv = self.an(ak(A_HOV, 14_500 + key), if self.hover == hit && on { 1.0 } else { 0.0 }, 12.0, 0.0);
        let base = c.alpha;
        if !on {
            c.alpha = base * 0.4;
        }
        if main {
            c.rect(b, 0.6 * r, mix(theme::fade(pal().accent, 0.9), pal().accent, hv));
            c.text_c((b[0] + b[2]) / 2.0, b[1] + (b[3] - b[1]) / 2.0 + 0.27 * r, 0.76 * r, Face::Semi, label, grey(0.04, 1.0));
        } else {
            c.rect(b, 0.6 * r, mix(pal().f2, pal().f4, hv));
            c.text_c((b[0] + b[2]) / 2.0, b[1] + (b[3] - b[1]) / 2.0 + 0.27 * r, 0.76 * r, Face::Semi, label, pal().t1);
        }
        c.alpha = base;
        if on {
            self.hits.push((b, hit));
        }
    }

    /// The parts as chips with a tick: which are taken (`have`: the ones there are).
    fn part_chips(&mut self, c: &mut Canvas, x0: f32, x1: f32, y: f32, have: u8) -> f32 {
        let r = self.r;
        let mut cx = x0;
        let mut cy = y;
        for (n, (bit, label)) in PARTS.iter().enumerate() {
            let there = have & bit != 0;
            let on = there && self.cfg_parts & bit != 0;
            let w = fonts().width(Face::Medium, 0.74 * r, tr(label)) + 2.6 * r;
            if cx + w > x1 {
                cx = x0;
                cy += 2.0 * r;
            }
            let ch = [cx, cy, cx + w, cy + 1.62 * r];
            let hit = Hit::Card(30, n as u16);
            let cv = self.an(ak(A_HOV, 14_700 + n), if on { 1.0 } else { 0.0 }, 12.0, if on { 1.0 } else { 0.0 });
            let hv = self.an(ak(A_HOV, 14_710 + n), if self.hover == hit { 1.0 } else { 0.0 }, 12.0, 0.0);
            let base = c.alpha;
            if !there {
                c.alpha = base * 0.35;
            }
            c.rect(ch, r, mix(mix(pal().f1, pal().f2, hv), pal().f4, cv));
            // the tick's box
            let bx = [ch[0] + 0.55 * r, ch[1] + 0.42 * r, ch[0] + 1.33 * r, ch[1] + 1.2 * r];
            c.rect(bx, 0.22 * r, mix(grey(1.0, 0.12), pal().accent, cv));
            if cv > 0.05 {
                let k = cv;
                let cc = [(bx[0] + bx[2]) / 2.0, (bx[1] + bx[3]) / 2.0];
                c.polyline(&[[cc[0] - 0.22 * r, cc[1]], [cc[0] - 0.06 * r, cc[1] + 0.17 * r * k], [cc[0] + 0.24 * r * k, cc[1] - 0.18 * r * k]], 1.5, grey(0.05, k));
            }
            c.text(ch[0] + 1.7 * r, ch[1] + 1.1 * r, 0.74 * r, Face::Medium, tr(label), mix(pal().t3, pal().t1, cv.max(hv * 0.6)));
            c.alpha = base;
            if there {
                self.hits.push((ch, hit));
            }
            cx += w + 0.35 * r;
        }
        cy + 1.62 * r
    }

    /// The cards of configs and the confirmation (cards.rs draws the binds' and the languages').
    pub(super) fn cfg_card(&mut self, c: &mut Canvas) {
        let Some(card) = self.card.clone() else { return };
        let r = self.r;
        let k = self.an(ak(A_POP, 2), 1.0, 14.0, 0.0);
        let w = 26.0 * r;
        let (title, sub, at) = match &card {
            Card::NewCfg { at } => (tr("A new config").to_string(), tr("Of what the client is now").to_string(), *at),
            Card::Cfg { i, at } => (tr("Config").to_string(), self.saved.get(*i).map_or(String::new(), |s| when_text(s.when)), *at),
            Card::Import { at, .. } => (tr("A config from a code").to_string(), tr("Take what you want of it").to_string(), *at),
            Card::ModuleImport { module, at, .. } => (tr("A module from a code").into(), self.mods.iter().find(|m| m.id == module).map_or(module.clone(), |m| i18n::nm(m.name).to_string()), *at),
            Card::Reset { at } => (tr("Everything back as it came").to_string(), tr("Every setting, bind, theme and the HUD's places").to_string(), *at),
            _ => return,
        };
        let h = match &card {
            Card::ModuleImport { module, .. } => {
                let lines = fonts().wrap(Face::Regular, 0.76 * r, self.module_code_hint(module), w - 2.4 * r).len();
                (8.6 + lines as f32 * 1.15).max(12.0) * r
            }
            Card::Reset { .. } => 15.6 * r,
            Card::Cfg { .. } => (24.6 * r).max(self.cfg_card_need * r),
            _ => 16.4 * r,
        };
        // a saved config's card by it; a new one, a pasted one and the confirmation over the middle of the window
        let (mut x, mut y) = match &card {
            Card::Cfg { .. } => {
                let mut y = at[3] + 0.5 * r;
                if y + h > self.h - 8.0 {
                    y = (at[1] - h - 0.5 * r).max(8.0);
                }
                (at[0], y)
            }
            _ => (self.win[0] + 15.0 * r + (self.win[2] - 15.0 * r - w) / 2.0, self.win[1] + (self.win[3] - h) / 2.0),
        };
        x = x.clamp(8.0, (self.w - w - 8.0).max(8.0));
        y = y.clamp(8.0, (self.h - h - 8.0).max(8.0));
        let rr = [x, y - (1.0 - k) * 0.5 * r, x + w, y + h - (1.0 - k) * 0.5 * r];
        self.card_rect = rr;
        let base = c.alpha;
        c.alpha = k;
        c.shadow(rr, r, 3.0 * r, grey(0.0, 0.62));
        let g = pal().glass;
        c.rect(rr, r, [g[0].max(0.05), g[1].max(0.05), g[2].max(0.055), 0.97]);
        self.hits.push((rr, Hit::Card(0, 0)));
        let (x0, x1) = (rr[0] + 1.2 * r, rr[2] - 1.2 * r);
        c.text(x0, rr[1] + 2.0 * r, 1.05 * r, Face::Bold, &title, pal().t1);
        c.text(x0, rr[1] + 3.2 * r, 0.72 * r, Face::Regular, &fonts().fit(Face::Regular, 0.72 * r, &sub, x1 - x0), pal().t3);
        let mut yy = rr[1] + 4.2 * r;
        match &card {
            Card::ModuleImport { module, .. } => {
                for line in fonts().wrap(Face::Regular, 0.76 * r, self.module_code_hint(module), x1 - x0) {
                    c.text(x0, yy + 0.85 * r, 0.76 * r, Face::Regular, &line, pal().t2);
                    yy += 1.15 * r;
                }
                yy = rr[3] - 3.4 * r;
                let bw = (x1 - x0 - 0.5 * r) / 2.0;
                self.card_btn(c, [x0, yy, x0 + bw, yy + 2.2 * r], tr("Cancel"), false, Hit::Card(40, 0), true, 0);
                self.card_btn(c, [x1 - bw, yy, x1, yy + 2.2 * r], tr("Apply"), true, Hit::Card(super::modulecode::APPLY, 0), true, 1);
            }
            Card::Reset { .. } => {
                let word = tr("I confirm").to_string();
                let line = i18n::trn("Type “{w}” to put it all back. Your configs stay.", &[("w", word.clone())]);
                for l in fonts().wrap(Face::Regular, 0.76 * r, &line, x1 - x0) {
                    c.text(x0, yy + 0.85 * r, 0.76 * r, Face::Regular, &l, pal().t2);
                    yy += 1.15 * r;
                }
                yy += 0.5 * r;
                let f = [x0, yy, x1, yy + 2.2 * r];
                let text = self.card_field.clone();
                self.field(c, f, &text, &word);
                yy += 3.0 * r;
                let ok = confirmed(&self.card_field);
                let bw = (x1 - x0 - 0.5 * r) / 2.0;
                self.card_btn(c, [x0, yy, x0 + bw, yy + 2.2 * r], tr("Cancel"), false, Hit::Card(40, 0), true, 0);
                self.card_btn(c, [x1 - bw, yy, x1, yy + 2.2 * r], tr("Put everything back"), true, Hit::Card(41, 0), ok, 1);
            }
            Card::NewCfg { .. } | Card::Import { .. } | Card::Cfg { .. } => {
                let f = [x0, yy, x1, yy + 2.2 * r];
                let text = self.card_field.clone();
                self.field(c, f, &text, tr("Its name"));
                yy += 2.9 * r;
                c.text(x0, yy + 0.7 * r, 0.66 * r, Face::Semi, tr(match card {
                    Card::Cfg { .. } => "Load or save these parts",
                    _ => "What it takes",
                }), pal().t4);
                yy += 1.2 * r;
                let have = match &card {
                    Card::Import { have, .. } => *have,
                    Card::Cfg { i, .. } => self.saved.get(*i).map_or(P_ALL, |s| s.parts) | P_ALL,
                    _ => P_ALL,
                };
                yy = self.part_chips(c, x0, x1, yy, have) + 0.9 * r;
                let any = self.cfg_parts & have != 0;
                match &card {
                    Card::NewCfg { .. } => {
                        let named = !self.card_field.trim().is_empty();
                        self.card_btn(c, [x0, yy, x1, yy + 2.3 * r], tr("Save"), true, Hit::Card(42, 0), any && named, 2);
                    }
                    Card::Import { .. } => {
                        let bw = (x1 - x0 - 0.5 * r) / 2.0;
                        let named = !self.card_field.trim().is_empty();
                        self.card_btn(c, [x0, yy, x0 + bw, yy + 2.3 * r], tr("Keep it"), false, Hit::Card(43, 0), any && named, 3);
                        self.card_btn(c, [x1 - bw, yy, x1, yy + 2.3 * r], tr("Take it now"), true, Hit::Card(44, 0), any, 4);
                    }
                    Card::Cfg { i, .. } => {
                        let i = *i;
                        let saved_parts = self.saved.get(i).map_or(0, |s| s.parts);
                        let bw = (x1 - x0 - 0.5 * r) / 2.0;
                        self.card_btn(c, [x0, yy, x0 + bw, yy + 2.3 * r], tr("Load"), true, Hit::Card(45, 0), self.cfg_parts & saved_parts != 0, 5);
                        self.card_btn(c, [x1 - bw, yy, x1, yy + 2.3 * r], tr("Save the client here"), false, Hit::Card(46, 0), any, 6);
                        yy += 2.8 * r;
                        let bw3 = (x1 - x0 - 1.0 * r) / 3.0;
                        self.card_btn(c, [x0, yy, x0 + bw3, yy + 2.1 * r], tr("Copy the code"), false, Hit::Card(47, 0), self.cfg_parts & saved_parts != 0, 7);
                        self.card_btn(c, [x0 + bw3 + 0.5 * r, yy, x0 + 2.0 * bw3 + 0.5 * r, yy + 2.1 * r], tr("Duplicate"), false, Hit::Card(48, 0), true, 8);
                        let del = if self.card_arm { tr("Once more") } else { tr("Delete") };
                        self.card_btn(c, [x1 - bw3, yy, x1, yy + 2.1 * r], del, false, Hit::Card(49, 0), true, 9);
                        // the servers it loads on (a server's profile, cfgserver.rs)
                        yy += 3.0 * r;
                        c.text(x0, yy + 0.7 * r, 0.66 * r, Face::Semi, tr("Loads on a server"), pal().t4);
                        yy += 1.3 * r;
                        let servers = self.cfg_servers(i);
                        let host = self.cfg_host.clone();
                        let mut chips: Vec<(String, Hit, bool)> = servers.iter().enumerate().map(|(n, h)| (format!("{h}  ×"), Hit::Card(50, n as u16), false)).collect();
                        if !host.is_empty() && !servers.contains(&host) {
                            chips.push((i18n::trn("+ {server}", &[("server", host.clone())]), Hit::Card(51, 0), true));
                        }
                        let mut px = x0;
                        for (n, (label, hit, add)) in chips.iter().enumerate() {
                            let cw = (fonts().width(Face::Medium, 0.7 * r, label) + 1.3 * r).min(x1 - px);
                            if cw < 3.0 * r {
                                break;
                            }
                            let b = [px, yy, px + cw, yy + 1.8 * r];
                            let hv = self.an(ak(A_HOV, 14_600 + n), if self.hover == *hit { 1.0 } else { 0.0 }, 12.0, 0.0);
                            c.rect(b, 0.5 * r, if *add { mix(pal().f2, pal().f4, hv) } else { mix(pal().f3, pal().f4, hv) });
                            let t = fonts().fit(Face::Medium, 0.7 * r, label, cw - 1.2 * r);
                            c.text(b[0] + 0.65 * r, b[1] + 1.2 * r, 0.7 * r, Face::Medium, &t, if *add { mix(pal().t2, pal().t1, hv) } else { pal().t1 });
                            self.hits.push((b, *hit));
                            px += cw + 0.4 * r;
                        }
                        if chips.is_empty() {
                            c.text(x0, yy + 1.2 * r, 0.7 * r, Face::Regular, tr("Join a server to give it this config"), pal().t3);
                        }
                        yy += 2.5 * r;
                        for l in fonts().wrap(Face::Regular, 0.66 * r, tr("As you join it, this config loads; as you leave, your own settings come back"), x1 - x0).into_iter().take(2) {
                            c.text(x0, yy + 0.6 * r, 0.66 * r, Face::Regular, &l, pal().t4);
                            yy += 1.0 * r;
                        }
                        yy += 0.8 * r;
                        c.text(x0, yy + 0.8 * r, 0.66 * r, Face::Semi, tr("Interface by Server Adaptation"), pal().t4);
                        yy += 1.4 * r;
                        let modes = self.saved.get(i).map_or(Vec::new(), |s| super::cfgserver::modes_of(&s.text));
                        let enabled = saved_parts & super::cfgserver::INTERFACE_PARTS != 0;
                        let width = (x1 - x0 - 0.8 * r)/ 3.0;
                        for (mode, name) in super::adapt::MODES.iter().enumerate() {
                            let x = x0 + mode as f32 * (width + 0.4 * r);
                            self.card_btn(c, [x, yy, x + width, yy + 2.0 * r], name, modes.contains(&mode), Hit::Card(52, mode as u16), enabled, 30 + mode);
                        }
                        yy += 2.7 * r;
                        let hint = if enabled { "This mode uses the config's theme, menu and HUD" } else { "Save theme, menu or HUD in this config first" };
                        for line in fonts().wrap(Face::Regular, 0.66 * r, tr(hint), x1 - x0).into_iter().take(2) {
                            c.text(x0, yy + 0.6 * r, 0.66 * r, Face::Regular, &line, pal().t4);
                            yy += r;
                        }
                        // (in the menu's units: the next frame's card is as tall as this one's content)
                        self.cfg_card_need = (yy - rr[1] + 0.8 * r) / r;
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        c.alpha = base;
    }

    /// A press on a config's card or the confirmation.
    pub(super) fn cfg_card_press(&mut self, h: Hit) -> bool {
        let Some(card) = self.card.clone() else { return false };
        let Hit::Card(part, idx) = h else { return false };
        match part {
            super::modulecode::APPLY => {
                if let Card::ModuleImport { module, text, .. } = card {
                    if self.module_code_apply(&text, &module) { self.card = None; }
                }
            }
            0 => {}
            30 => {
                let bit = PARTS.get(idx as usize).map_or(0, |p| p.0);
                self.cfg_parts ^= bit;
                self.switch_sound(self.cfg_parts & bit != 0, 0.4);
            }
            40 => self.card = None,
            41 => {
                if confirmed(&self.card_field) {
                    self.reset_everything();
                    self.card = None;
                }
            }
            42 => {
                let name = self.card_field.trim().to_string();
                if !name.is_empty() && self.cfg_parts != 0 {
                    if !self.cfg_write(&name, self.cfg_parts, None) { return true; }
                    self.card = None;
                    self.toast(i18n::trn("{name} saved", &[("name", name)]));
                    self.switch_sound(true, 0.6);
                }
            }
            43 | 44 => {
                let Card::Import { text, have, .. } = card else { return true };
                let name = self.card_field.trim().to_string();
                if part == 44 {
                    self.cfg_load(&text, self.cfg_parts & have, if name.is_empty() { tr("A friend's config") } else { &name });
                } else if !name.is_empty() {
                    // kept as it came, its parts as chosen
                    let (vals, _, _) = parse(&text);
                    let mut out = format!("#name={name}\n#parts={}\n", self.cfg_parts & have);
                    let mut lines: Vec<(&String, &String)> = vals.iter().collect();
                    lines.sort();
                    for (k, v) in lines {
                        if self.part_of(k) & self.cfg_parts & have != 0 {
                            out.push_str(&format!("{k}={v}\n"));
                        }
                    }
                    if let Some(dir) = self.cfg_dir() {
                        let _ = std::fs::create_dir_all(&dir);
                        let mut f = dir.join(format!("{}.cfg", file_name(&name)));
                        let mut n = 2;
                        while f.exists() {
                            f = dir.join(format!("{} {n}.cfg", file_name(&name)));
                            n += 1;
                        }
                        let _ = std::fs::write(f, out);
                    }
                    self.cfg_scan();
                    self.toast(i18n::trn("{name} saved", &[("name", name)]));
                    self.switch_sound(true, 0.6);
                }
                self.card = None;
            }
            52 => {
                let Card::Cfg { i, .. } = card else { return true };
                self.cfg_interface(i, idx as usize);
            }
            50 | 51 => {
                let Card::Cfg { i, .. } = card else { return true };
                if part == 51 {
                    let host = self.cfg_host.clone();
                    self.cfg_server(i, &host, true);
                } else if let Some(h) = self.cfg_servers(i).get(idx as usize).cloned() {
                    self.cfg_server(i, &h, false);
                }
            }
            45..=49 => {
                let Card::Cfg { i, .. } = card else { return true };
                let Some(s) = self.saved.get(i).cloned() else { return true };
                match part {
                    45 => {
                        self.cfg_load(&s.text, self.cfg_parts, &s.name);
                        self.card = None;
                    }
                    46 => {
                        let name = if self.card_field.trim().is_empty() { s.name.clone() } else { self.card_field.trim().to_string() };
                        if !self.cfg_write(&name, self.cfg_parts, Some(i)) { return true; }
                        self.card = None;
                        self.toast(i18n::trn("{name} saved", &[("name", name)]));
                        self.switch_sound(true, 0.6);
                    }
                    47 => {
                        let (vals, _, have) = parse(&s.text);
                        let mut out = format!("#name={}\n#parts={}\n", s.name, self.cfg_parts & have);
                        let mut lines: Vec<(&String, &String)> = vals.iter().collect();
                        lines.sort();
                        for (k, v) in lines {
                            if self.part_of(k) & self.cfg_parts & have != 0 {
                                out.push_str(&format!("{k}={v}\n"));
                            }
                        }
                        let code = encode(&out);
                        self.copy(&code);
                        self.toast(tr("The code is copied: paste it to a friend").to_string());
                        self.sound_tick(0.7);
                    }
                    48 => {
                        if let Some(dir) = self.cfg_dir() {
                            let name = i18n::trn("{name} copy", &[("name", s.name.clone())]);
                            let text = super::cfgserver::with_modes(&super::cfgserver::with_servers(&s.text.replacen(&format!("#name={}", s.name), &format!("#name={name}"), 1), &[]), &[]);
                            let mut f = dir.join(format!("{}.cfg", file_name(&name)));
                            let mut n = 2;
                            while f.exists() {
                                f = dir.join(format!("{} {n}.cfg", file_name(&name)));
                                n += 1;
                            }
                            let _ = std::fs::write(f, text);
                            self.cfg_scan();
                        }
                        self.card = None;
                        self.sound_tick(0.6);
                    }
                    _ => {
                        if self.card_arm {
                            if !self.cfg_delete(&s.file) { return true; }
                            self.card = None;
                            self.switch_sound(false, 0.6);
                        } else {
                            self.card_arm = true;
                            self.sound_tick(0.5);
                        }
                    }
                }
            }
            _ => return false,
        }
        true
    }

    /// Every setting of the client back as it came (binds, theme, the HUD's places too); the configs stay.
    pub(super) fn reset_everything(&mut self) {
        let mut mods = super::data::modules();
        mods.extend(super::data::hud_modules());
        self.mods = mods;
        self.client = super::data::client_settings();
        self.theme = super::data::theme_settings();
        self.hud.reset_all();
        self.loaded_cfg = None;
        self.open = None;
        self.save();
        self.switch_sound(false, 0.8);
        self.toast(tr("Everything is as it came").to_string());
    }

    /// The word over the menu, fading after a while.
    pub(super) fn toast_draw(&mut self, c: &mut Canvas) {
        self.toast_rect = [0.0; 4];
        let Some((s, t0)) = self.toast.clone() else { return };
        let r = self.r;
        let age = self.now - t0;
        if age > 2.6 {
            self.toast = None;
            return;
        }
        let k = smooth(0.0, 0.18, age) * (1.0 - smooth(2.1, 2.6, age));
        let tw = fonts().width(Face::Medium, 0.8 * r, &s) + 2.6 * r;
        let cx = self.win[0] + self.win[2] / 2.0;
        let y = self.win[1] + self.win[3] - 4.2 * r + (1.0 - ease_out(smooth(0.0, 0.3, age))) * 0.8 * r;
        let b = [cx - tw / 2.0, y, cx + tw / 2.0, y + 2.3 * r];
        // (Auto Swap's things are not drawn over it)
        self.toast_rect = b;
        let base = c.alpha;
        c.alpha = k;
        c.shadow(b, 1.15 * r, 2.0 * r, grey(0.0, 0.5));
        let g = pal().glass;
        c.rect(b, 1.15 * r, [g[0].max(0.06), g[1].max(0.06), g[2].max(0.065), 0.97]);
        c.text_c(cx, y + 1.45 * r, 0.8 * r, Face::Medium, &s, pal().t1);
        c.alpha = base;
    }
}
