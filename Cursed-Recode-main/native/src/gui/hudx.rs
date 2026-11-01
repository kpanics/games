//! The HUD's own elements the player adds (Hud → "+"): a text of their own words and placeholders, a card stretched
//! anywhere (glass, a blur, the client's interference, a colour) with words anywhere on it, a picture of their own
//! (a GIF plays). And the placeholders: {name} in any text of the HUD is what the game says of it now.

use super::data::{Cat, Col, Kind, Module, Setting};
use super::draw::Rgba;
use super::i18n::tr;
use super::text::Face;
use super::Gui;
use std::collections::HashMap;
use std::sync::Mutex;

/// The pictures' own sizes as Java read them (a picture keeps its shape by them).
pub static PIC_SIZES: Mutex<std::sync::LazyLock<HashMap<String, (f32, f32)>>> = Mutex::new(std::sync::LazyLock::new(HashMap::new));

/// The kinds of the player's elements.
pub const KINDS: [(&str, &str); 3] = [("text", "Text"), ("card", "Card"), ("image", "Picture")];

/// The faces a text may be in: the menu's, the main menu's condensed, its spaced capitals.
pub const FACES: [&str; 9] = ["Regular", "Medium", "Semibold", "Bold", "Mono", "Condensed light", "Condensed", "Condensed heavy", "Capitals"];

pub fn face(i: usize) -> Face {
    match i {
        0 => Face::Regular,
        2 => Face::Semi,
        3 => Face::Bold,
        4 => Face::Mono,
        5 => Face::Light,
        6 => Face::Book,
        7 => Face::Heavy,
        8 => Face::Caps,
        _ => Face::Medium,
    }
}

/// Where on a card a text sits.
pub const AT: [&str; 9] = ["Top left", "Top", "Top right", "Left", "Middle", "Right", "Bottom left", "Bottom", "Bottom right"];

/// Every placeholder: its name, what it is (the placeholders' card lists them with their values now).
pub const PLACEHOLDERS: &[(&str, &str)] = &[
    ("fps", "Frames a second"),
    ("frametime", "A frame, ms"),
    ("ping", "Ping, ms"),
    ("player", "Your nick"),
    ("server", "The server's address"),
    ("servername", "The server's name"),
    ("online", "Players online"),
    ("x", "X"),
    ("y", "Y"),
    ("z", "Z"),
    ("bx", "Block X"),
    ("by", "Block Y"),
    ("bz", "Block Z"),
    ("nx", "X in the other dimension"),
    ("nz", "Z in the other dimension"),
    ("dimension", "The dimension"),
    ("biome", "The biome"),
    ("facing", "Facing"),
    ("yaw", "Yaw"),
    ("pitch", "Pitch"),
    ("speed", "Speed, blocks a second"),
    ("speedv", "Speed up or down"),
    ("health", "Health"),
    ("hearts", "Hearts"),
    ("maxhealth", "Health at most"),
    ("absorption", "Absorption"),
    ("food", "Food"),
    ("saturation", "Saturation"),
    ("armor", "Armour"),
    ("air", "Air, seconds"),
    ("level", "Experience level"),
    ("xp", "Experience to the next level, %"),
    ("slot", "Hotbar slot"),
    ("held", "The item in hand"),
    ("heldcount", "How many in hand"),
    ("heldtotal", "How many of it you have"),
    ("helddurability", "The item's durability"),
    ("offhand", "The item in the off hand"),
    ("helmet", "Helmet's durability"),
    ("chestplate", "Chestplate's durability"),
    ("leggings", "Leggings' durability"),
    ("boots", "Boots' durability"),
    ("totems", "Totems"),
    ("pearls", "Pearls"),
    ("gapples", "Golden apples"),
    ("egapples", "Enchanted golden apples"),
    ("crystals", "End crystals"),
    ("obsidian", "Obsidian"),
    ("arrows", "Arrows"),
    ("xpbottles", "Bottles o' enchanting"),
    ("freeslots", "Free slots"),
    ("cps", "Left clicks a second"),
    ("rcps", "Right clicks a second"),
    ("kills", "Kills this session"),
    ("deaths", "Deaths this session"),
    ("playtime", "Played this session"),
    ("target", "The one you fight"),
    ("targethealth", "Their health"),
    ("targetdistance", "How far they are"),
    ("time", "Time"),
    ("seconds", "Time with seconds"),
    ("time12", "12-hour time"),
    ("date", "Date"),
    ("gametime", "The world's time"),
    ("day", "The world's day"),
    ("weather", "Weather"),
    ("light", "Light here"),
    ("gamemode", "Game mode"),
    ("difficulty", "Difficulty"),
    ("entities", "Entities near"),
    ("memory", "Memory used"),
    ("memorypercent", "Memory used, %"),
    ("music", "What plays"),
    ("artist", "Its artist"),
    ("musictime", "Its time"),
    ("musiclength", "Its length"),
    ("modules", "Modules on"),
    ("binds", "Binds"),
    ("config", "The config loaded"),
];

fn txt(label: &'static str, v: &str) -> Setting {
    Setting::new(label, Kind::Text(v.to_string()))
}
fn sl(label: &'static str, min: f32, max: f32, step: f32, v: f32, unit: &'static str) -> Setting {
    Setting::new(label, Kind::Slider { min, max, step, v, unit })
}
fn ch(label: &'static str, v: usize, opts: &[&'static str]) -> Setting {
    Setting::new(label, Kind::Choice { v, opts: opts.to_vec() })
}
fn tg(label: &'static str, v: bool) -> Setting {
    Setting::new(label, Kind::Toggle(v))
}
fn co(label: &'static str, c: Col) -> Setting {
    Setting::new(label, Kind::Colour(c))
}
fn hd(label: &'static str) -> Setting {
    Setting::new(label, Kind::Header)
}
fn ac(label: &'static str) -> Setting {
    Setting::new(label, Kind::Action)
}

/// What every element has first (its look), as data.rs gives the HUD's own; a text has its plate's style too (a card
/// has its own fill, a picture no plate).
fn look(bg: bool, plate: bool) -> Vec<Setting> {
    let mut v = vec![
        hd("Look"),
        // (on the HUD, over the inventories' and chests' screens — Custom Gui — or both: where it was put)
        ch("Show on", 0, &["HUD", "Inventories", "Both"]),
        sl("Scale", 50.0, 200.0, 5.0, 100.0, "%"),
        sl("Opacity", 10.0, 100.0, 5.0, 100.0, "%"),
        tg("Background", bg),
    ];
    if plate {
        v.push(ch("Style", 0, &["Glass", "Blur", "Interference", "Colour"]));
    }
    v.extend([
        co("Glass", Col::theme(0x040404)),
        co("Text", Col::theme(0xececec)),
        co("Secondary text", Col::theme(0xa3a3a3)),
        co("Accent", Col::theme(0xececec)),
    ]);
    if plate {
        v.extend([sl("Corners", 0.0, 300.0, 5.0, 100.0, "%"), sl("Inner space", 40.0, 250.0, 5.0, 100.0, "%"), tg("Line of light", false)]);
    }
    v.extend([tg("Shadow", true), sl("Text shadow", 0.0, 100.0, 5.0, 0.0, "%")]);
    v
}

/// A new element's settings of its kind.
pub fn settings(kind: &str) -> Vec<Setting> {
    match kind {
        "card" => {
            let mut v = look(true, false);
            v.extend([
                hd("Card"),
                sl("Width", 20.0, 1600.0, 2.0, 280.0, " px"),
                sl("Height", 12.0, 1000.0, 2.0, 130.0, " px"),
                ch("Fill", 0, &["Glass", "Blur", "Interference", "Colour", "Nothing"]),
                co("Fill colour", Col::alpha(0x000000, 0.45)),
                sl("Corner", 0.0, 80.0, 1.0, 14.0, " px"),
                tg("Line of light", false),
            ]);
            for (n, (t, at, size, font, col, xx, yy)) in [
                ("Text 1", "Text 1 at", "Text 1 size", "Text 1 face", "Text 1 colour", "Text 1 x", "Text 1 y"),
                ("Text 2", "Text 2 at", "Text 2 size", "Text 2 face", "Text 2 colour", "Text 2 x", "Text 2 y"),
                ("Text 3", "Text 3 at", "Text 3 size", "Text 3 face", "Text 3 colour", "Text 3 x", "Text 3 y"),
            ]
            .into_iter()
            .enumerate()
            {
                let header: &'static str = ["Text one", "Text two", "Text three"][n];
                v.extend([
                    hd(header),
                    txt(t, if n == 0 { "{player}" } else { "" }),
                    ch(at, [4, 1, 7][n], &AT),
                    sl(size, 6.0, 96.0, 1.0, [16.0, 12.0, 12.0][n], " px"),
                    ch(font, 2, &FACES),
                    co(col, Col::theme(0xececec)),
                    sl(xx, -600.0, 600.0, 1.0, 0.0, " px"),
                    sl(yy, -400.0, 400.0, 1.0, 0.0, " px"),
                ]);
            }
            v.push(ac("Placeholders"));
            v.push(ac("Remove this element"));
            v
        }
        "image" => {
            let mut v = look(false, false);
            v.extend([
                hd("Picture"),
                txt("File", ""),
                ac("Choose a picture"),
                sl("Width", 8.0, 1600.0, 2.0, 160.0, " px"),
                sl("Height", 8.0, 1000.0, 2.0, 160.0, " px"),
                tg("Keep its shape", true),
                sl("Corner", 0.0, 80.0, 1.0, 10.0, " px"),
                sl("GIF speed", 0.1, 4.0, 0.05, 1.0, "x"),
                ac("Remove this element"),
            ]);
            v
        }
        _ => {
            let mut v = look(true, true);
            v.extend([
                hd("Shows"),
                txt("Line 1", "{fps} fps  ·  {ping} ms"),
                txt("Line 2", ""),
                txt("Line 3", ""),
                txt("Line 4", ""),
                ch("Align", 0, &["Left", "Centre", "Right"]),
                ch("Face", 1, &FACES),
                sl("Size", 6.0, 96.0, 1.0, 13.0, " px"),
                sl("Line spacing", 0.8, 2.5, 0.05, 1.35, "x"),
                tg("Other lines dimmer", true),
                ac("Placeholders"),
                ac("Remove this element"),
            ]);
            v
        }
    }
}

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

impl Gui {
    /// The player's elements as one line (the settings' file): id|kind, …
    pub(super) fn custom_list(&self) -> String {
        self.mods.iter().filter(|m| m.id.starts_with("x-")).map(|m| format!("{}|{}", m.id, m.id.split('-').nth(1).unwrap_or("text"))).collect::<Vec<_>>().join(",")
    }

    /// The player's elements made as the line says (those it does not name go).
    pub(super) fn custom_ensure(&mut self, list: &str) {
        let want: Vec<(&str, &str)> = list.split(',').filter_map(|p| p.split_once('|')).collect();
        self.mods.retain(|m| !m.id.starts_with("x-") || want.iter().any(|w| w.0 == m.id));
        for (id, kind) in want {
            if !self.mods.iter().any(|m| m.id == id) {
                self.custom_make(leak(id.to_string()), kind);
            }
        }
    }

    /// A new element of the player's: made, placed in the middle, chosen; its id.
    pub(super) fn custom_add(&mut self, kind: &str) -> usize {
        let n = (1..).find(|n| !self.mods.iter().any(|m| m.id == format!("x-{kind}-{n}"))).unwrap_or(1);
        let id = leak(format!("x-{kind}-{n}"));
        let i = self.custom_make(id, kind);
        self.hud.places.insert(id, (4, [0.0, -60.0]));
        self.save();
        i
    }

    fn custom_make(&mut self, id: &'static str, kind: &str) -> usize {
        let n: String = id.rsplit('-').next().unwrap_or("1").to_string();
        let base = KINDS.iter().find(|k| k.0 == kind).map_or("Text", |k| k.1);
        let name = leak(format!("{} {n}", tr(base)));
        let desc: &'static str = match kind {
            "card" => "A card of your own: stretch it in the editor by its corner, fill it as you like, put words anywhere on it.",
            "image" => "A picture of your own from a file; a GIF plays.",
            _ => "Your own words; {name} puts what the game says, the Placeholders list them.",
        };
        self.mods.push(Module { id, cat: Cat::HudEl, sub: None, name, desc, on: true, binds: Vec::new(), def_binds: Vec::new(), plain: false, set: settings(kind) });
        if !self.hud.places.contains_key(id) {
            self.hud.places.insert(id, (4, [0.0, -60.0]));
        }
        self.hud.defaults.insert(id, (4, [0.0, -60.0]));
        self.mods.len() - 1
    }

    pub(super) fn custom_remove(&mut self, i: usize) {
        if !self.mods.get(i).is_some_and(|m| m.id.starts_with("x-")) {
            return;
        }
        let id = self.mods[i].id;
        self.mods.remove(i);
        self.hud.places.remove(id);
        self.hud.pop = None;
        self.open = self.open.map(|o| if o > i { o - 1 } else { o });
        self.hud_sel = self.hud_cards().first().copied().unwrap_or(0);
        self.save();
    }

    // ------------------------------------------------------------------ the placeholders
    /// A value of the game's by its name (None: no such placeholder).
    pub(super) fn ph(&self, name: &str) -> Option<String> {
        let f = &self.hud.facts;
        let v = match name {
            "speed" => format!("{:.1}", self.hud.speed),
            "facing" => tr(super::hud::facing_of(f.yaw)).to_string(),
            "dimension" => tr(match f.vars.get("dimension").map(String::as_str) {
                Some("nether") => "Nether",
                Some("end") => "The End",
                _ => "Overworld",
            })
            .to_string(),
            "weather" => tr(match f.vars.get("weather").map(String::as_str) {
                Some("rain") => "Rain",
                Some("thunder") => "Thunder",
                _ => "Clear",
            })
            .to_string(),
            "music" => f.music.as_ref().map_or(String::new(), |m| m.title.clone()),
            "artist" => f.music.as_ref().map_or(String::new(), |m| m.artist.clone()),
            "musictime" => f.music.as_ref().map_or(String::new(), |m| super::hud::mmss_of(m.pos)),
            "musiclength" => f.music.as_ref().map_or(String::new(), |m| super::hud::mmss_of(m.len)),
            "modules" => self.mods.iter().filter(|m| !m.plain && m.on && !matches!(m.cat, Cat::Hud | Cat::HudEl)).count().to_string(),
            "binds" => self.all_binds().iter().map(|b| b.1.len()).sum::<usize>().to_string(),
            "config" => self.loaded_cfg.clone().unwrap_or_default(),
            _ => return f.vars.get(name).cloned(),
        };
        Some(v)
    }

    /// A text with its placeholders filled: {name}, or {name:N} for N decimals of a number.
    pub(super) fn fill(&self, t: &str) -> String {
        if !t.contains('{') {
            return t.to_string();
        }
        let mut out = String::with_capacity(t.len() + 16);
        let mut rest = t;
        while let Some(a) = rest.find('{') {
            out.push_str(&rest[..a]);
            let after = &rest[a + 1..];
            let Some(b) = after.find('}') else {
                out.push_str(&rest[a..]);
                rest = "";
                break;
            };
            let inner = &after[..b];
            let (name, dec) = match inner.split_once(':') {
                Some((n, d)) => (n.trim(), d.trim().parse::<usize>().ok()),
                None => (inner.trim(), None),
            };
            match self.ph(&name.to_lowercase()) {
                Some(v) => match dec.and_then(|d| v.parse::<f64>().ok().map(|x| (d, x))) {
                    Some((d, x)) => out.push_str(&format!("{x:.d$}")),
                    None => out.push_str(&v),
                },
                None => {
                    out.push('{');
                    out.push_str(inner);
                    out.push('}');
                }
            }
            rest = &after[b + 1..];
        }
        out.push_str(rest);
        out
    }

    /// The placeholders with their values now (the card that lists them).
    pub(super) fn placeholders_now(&self) -> Vec<(&'static str, &'static str, String)> {
        PLACEHOLDERS.iter().map(|(n, d)| (*n, *d, self.ph(n).unwrap_or_default())).collect()
    }
}

/// The colour of a card's text, or of its fill.
pub fn col_of(set: &[Setting], label: &str, theme: Rgba, t: f32) -> Rgba {
    super::theme::or(super::theme::col(set, label), theme, t)
}

/// A picture chosen in the system's dialog: the element it is for, its file (taken by the menu's next frame).
static PICKED: Mutex<Option<(String, String)>> = Mutex::new(None);

/// The system's own dialog to choose a picture, on a thread of its own (the game goes on meanwhile).
pub fn choose_picture(id: &str) {
    let id = id.to_string();
    std::thread::spawn(move || {
        if let Some(path) = open_dialog() {
            *PICKED.lock().unwrap() = Some((id, path));
        }
    });
}

fn open_dialog() -> Option<String> {
    use windows_sys::Win32::UI::Controls::Dialogs::{GetOpenFileNameW, OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_PATHMUSTEXIST, OPENFILENAMEW};
    let filter: Vec<u16> = "Pictures (PNG, JPG, GIF)\0*.png;*.jpg;*.jpeg;*.gif;*.bmp\0All files\0*.*\0\0".encode_utf16().collect();
    let title: Vec<u16> = "Recode\0".encode_utf16().collect();
    let mut buf = vec![0u16; 4096];
    let mut ofn: OPENFILENAMEW = unsafe { std::mem::zeroed() };
    ofn.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
    ofn.lpstrFilter = filter.as_ptr();
    ofn.lpstrFile = buf.as_mut_ptr();
    ofn.nMaxFile = buf.len() as u32;
    ofn.lpstrTitle = title.as_ptr();
    ofn.Flags = OFN_EXPLORER | OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST;
    if unsafe { GetOpenFileNameW(&mut ofn) } == 0 {
        return None;
    }
    let n = buf.iter().position(|&c| c == 0).unwrap_or(0);
    Some(String::from_utf16_lossy(&buf[..n]))
}

impl Gui {
    /// A picture chosen in the dialog: into its element's File.
    pub(super) fn picked_in(&mut self) {
        let Some((id, path)) = PICKED.lock().unwrap().take() else { return };
        if let Some(m) = self.mods.iter_mut().find(|m| m.id == id) {
            if let Some(s) = m.set.iter_mut().find(|s| s.label == "File") {
                s.kind = Kind::Text(path);
            }
        }
        self.save();
    }
}
