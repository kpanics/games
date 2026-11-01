//! What the player has on (the Cosmetics page sets it; cursed-recode/cosmetics.txt keeps it): the knife, the gloves,
//! the AWP and the SSG 08 they chose — each its finish, seed, wear — the hands' side (CT, T), and which of the game's
//! weapons shows which (her defaults: a sword or an axe the knife, a bow the AWP, a crossbow the SSG 08; any of CS's
//! on any of the game's).

use std::sync::{Mutex, OnceLock};

/// One thing on: CS2's item, its finish ("" none), its seed, its wear, its StatTrak's kills (none: not StatTrak).
#[derive(Clone, Debug, PartialEq)]
pub struct Equip {
    pub item: String,
    pub paint: String,
    pub seed: i32,
    pub wear: f32,
    pub stattrak: Option<u32>,
    /// A charm on it (a rifle's): its name, where it hangs from (off its own place, in its frame: metres).
    pub charm: Option<String>,
    pub charm_offset: Option<[f32; 3]>,
    /// Stickers on it (cosmetics/sticker.rs).
    pub stickers: Vec<super::sticker::Placed>,
    /// Its name tag (cosmetics/nametag.rs), our material over its finish (cosmetics/overlay.rs).
    pub nametag: Option<String>,
    pub overlay: Option<super::overlay::Overlay>,
}

impl Equip {
    fn plain(item: &str) -> Equip {
        Equip { item: item.into(), paint: String::new(), seed: 0, wear: 0.0, stattrak: None, charm: None, charm_offset: None, stickers: Vec::new(), nametag: None, overlay: None }
    }

    /// As the loadout keeps it ("item|paint|seed|wear|…").
    pub fn dump(&self) -> String {
        let st = self.stattrak.map_or(String::new(), |n| n.to_string());
        let mut s = format!("{}|{}|{}|{}", self.item, self.paint, self.seed, self.wear);
        // the fields after the four: each written while one after it is
        let tag = self.nametag.as_deref().filter(|n| !n.is_empty());
        let more = self.overlay.is_some();
        if self.stattrak.is_some() || self.charm.is_some() || !self.stickers.is_empty() || tag.is_some() || more {
            s.push_str(&format!("|{st}"));
        }
        if self.charm.is_some() || !self.stickers.is_empty() || tag.is_some() || more {
            let at = self.charm_offset.filter(|_| self.charm.is_some()).map_or(String::new(), |o| format!("@{:.5},{:.5},{:.5}", o[0], o[1], o[2]));
            s.push_str(&format!("|{}{at}", self.charm.as_deref().unwrap_or("")));
        }
        if !self.stickers.is_empty() || tag.is_some() || more {
            let v: Vec<String> = self.stickers.iter().map(|p| p.dump()).collect();
            s.push_str(&format!("|{}", v.join(";")));
        }
        if tag.is_some() || more {
            s.push_str(&format!("|{}", tag.unwrap_or("")));
        }
        if let Some(o) = &self.overlay {
            s.push_str(&format!("|{}", o.dump()));
        }
        s
    }

    pub fn parse(s: &str) -> Option<Equip> {
        let f: Vec<&str> = s.split('|').collect();
        Some(Equip {
            item: f.first()?.to_string(),
            paint: f.get(1).unwrap_or(&"").to_string(),
            seed: f.get(2).and_then(|v| v.parse().ok()).unwrap_or(0),
            wear: f.get(3).and_then(|v| v.parse().ok()).unwrap_or(0.0),
            stattrak: f.get(4).and_then(|v| v.parse().ok()),
            charm: f.get(5).map(|v| v.split_once('@').map_or(*v, |(n, _)| n)).filter(|v| !v.is_empty()).map(|v| v.to_string()),
            charm_offset: f.get(5).and_then(|v| v.split_once('@')).and_then(|(_, o)| {
                let v: Vec<f32> = o.split(',').filter_map(|x| x.parse().ok()).collect();
                (v.len() == 3).then(|| [v[0], v[1], v[2]])
            }),
            stickers: f.get(6).map_or(Vec::new(), |v| v.split(';').filter_map(super::sticker::Placed::parse).collect()),
            nametag: f.get(7).filter(|v| !v.is_empty()).map(|v| v.to_string()),
            overlay: f.get(8).and_then(|v| super::overlay::Overlay::parse(v)),
        })
    }
}

/// A slot of the loadout.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Slot {
    Knife,
    Gloves,
    Awp,
    Ssg,
}

impl Slot {
    pub const ALL: [Slot; 4] = [Slot::Knife, Slot::Gloves, Slot::Awp, Slot::Ssg];

    pub fn key(self) -> &'static str {
        match self {
            Slot::Knife => "knife",
            Slot::Gloves => "gloves",
            Slot::Awp => "awp",
            Slot::Ssg => "ssg",
        }
    }

    fn from_key(k: &str) -> Option<Slot> {
        Slot::ALL.into_iter().find(|s| s.key() == k)
    }

    /// The slot an item of the catalog goes in.
    pub fn of(item: &str, kind: super::catalog::Kind) -> Slot {
        match kind {
            super::catalog::Kind::Knife => Slot::Knife,
            super::catalog::Kind::Gloves => Slot::Gloves,
            super::catalog::Kind::Rifle if item == "weapon_ssg08" => Slot::Ssg,
            super::catalog::Kind::Rifle => Slot::Awp,
        }
    }
}

/// The game's weapons the mapping knows (what an item id ends with or is).
pub const WEAPONS: [&str; 6] = ["sword", "axe", "bow", "crossbow", "trident", "mace"];

#[derive(Clone, Debug, PartialEq)]
pub struct Loadout {
    pub knife: Option<Equip>,
    pub gloves: Option<Equip>,
    pub awp: Option<Equip>,
    pub ssg: Option<Equip>,
    /// "ct" or "t".
    pub arms: String,
    /// Each of the game's weapons (WEAPONS): the slot it shows (none: the game's own).
    pub map: Vec<(String, Option<Slot>)>,
    /// CS's crosshair with a CS item in the hands: its code (none: CS2's own); how fast its colour's hue runs round
    /// (the client's Chroma: 0 it stands); the page's stage for it (gui/cosmxhair.rs): its background, its size.
    pub crosshair: String,
    pub crosshair_chroma: f32,
    pub xh_stage: (u8, u8),
    /// The page's look's light (gui/cosmstage.rs): its turn about up and its height (radians). How much larger the look
    /// is drawn, then brought down (1 Standard, 2 High, 3 Ultra: cosmetics/render.rs frame_large).
    pub light: (f32, f32),
    pub quality: u8,
    /// The key that inspects what the hands hold (Recode's name; none: no inspect), the hand they hold it in (the game's
    /// own main hand, as its settings have it; else the left as CS's cl_righthand 0, or the right), the game's controls
    /// moved off the inspect key (their name, the key they were on, the key they were put on: gui/cosmkeys.rs).
    pub inspect: String,
    pub hand_game: bool,
    pub left: bool,
    pub moved: Vec<(String, String, String)>,
    /// CS's things in the hands at all (off: the game's own hands, crosshair and keys) — off at first (her word, 25.10),
    /// on by the page's switch or once something is put on.
    pub on: bool,
}

/// The look's light as it first comes (from the upper left, in front).
pub const LIGHT: (f32, f32) = (-0.644, 0.714);

impl Default for Loadout {
    fn default() -> Loadout {
        Loadout {
            knife: Some(Equip::plain("weapon_knife_karambit")),
            gloves: None,
            awp: Some(Equip::plain("weapon_awp")),
            ssg: Some(Equip::plain("weapon_ssg08")),
            arms: "ct".into(),
            map: vec![
                ("sword".into(), Some(Slot::Knife)),
                ("axe".into(), Some(Slot::Knife)),
                ("bow".into(), Some(Slot::Awp)),
                ("crossbow".into(), Some(Slot::Ssg)),
                ("trident".into(), None),
                ("mace".into(), None),
            ],
            crosshair: String::new(),
            crosshair_chroma: 0.0,
            xh_stage: (0, 2),
            light: LIGHT,
            quality: 3,
            inspect: "F".into(),
            hand_game: true,
            left: false,
            moved: Vec::new(),
            on: false,
        }
    }
}

impl Loadout {
    pub fn get(&self, s: Slot) -> Option<&Equip> {
        match s {
            Slot::Knife => self.knife.as_ref(),
            Slot::Gloves => self.gloves.as_ref(),
            Slot::Awp => self.awp.as_ref(),
            Slot::Ssg => self.ssg.as_ref(),
        }
    }

    pub fn get_mut(&mut self, s: Slot) -> Option<&mut Equip> {
        match s {
            Slot::Knife => self.knife.as_mut(),
            Slot::Gloves => self.gloves.as_mut(),
            Slot::Awp => self.awp.as_mut(),
            Slot::Ssg => self.ssg.as_mut(),
        }
    }

    pub fn set(&mut self, s: Slot, e: Option<Equip>) {
        match s {
            Slot::Knife => self.knife = e,
            Slot::Gloves => self.gloves = e,
            Slot::Awp => self.awp = e,
            Slot::Ssg => self.ssg = e,
        }
    }

    /// CS's things in the hands now: on, and a knife or a rifle put on that one of the game's weapons shows (only then
    /// the inspect key takes its key from the game's controls).
    pub fn worn(&self) -> bool {
        self.on && [Slot::Knife, Slot::Awp, Slot::Ssg].into_iter().any(|s| self.get(s).is_some() && self.map.iter().any(|m| m.1 == Some(s)))
    }

    /// The slot one of the game's items shows (its id: "minecraft:diamond_sword").
    pub fn slot_for(&self, item: &str) -> Option<Slot> {
        let id = item.rsplit_once(':').map_or(item, |(_, p)| p);
        // the longest of the names it ends with ("crossbow" before "bow")
        let mut best: Option<(&str, Option<Slot>)> = None;
        for (w, s) in &self.map {
            if (id == w || id.ends_with(&format!("_{w}"))) && best.is_none_or(|(b, _)| w.len() > b.len()) {
                best = Some((w.as_str(), *s));
            }
        }
        best.and_then(|(_, s)| s)
    }

    pub fn dump(&self) -> String {
        let mut out = String::new();
        for s in Slot::ALL {
            out.push_str(&format!("{}={}\n", s.key(), self.get(s).map_or(String::new(), Equip::dump)));
        }
        out.push_str(&format!("arms={}\n", self.arms));
        out.push_str(&format!("crosshair={}\n", self.crosshair));
        out.push_str(&format!("crosshair_chroma={:.2}\n", self.crosshair_chroma));
        out.push_str(&format!("crosshair_stage={},{}\n", self.xh_stage.0, self.xh_stage.1));
        out.push_str(&format!("light={:.3},{:.3}\n", self.light.0, self.light.1));
        out.push_str(&format!("quality={}\n", self.quality));
        out.push_str(&format!("enabled={}\n", self.on as u8));
        out.push_str(&format!("inspect={}\n", self.inspect));
        out.push_str(&format!("hand={}\n", if self.hand_game { "game" } else if self.left { "left" } else { "right" }));
        let moved: Vec<String> = self.moved.iter().map(|(n, from, to)| format!("{n}:{from}>{to}")).collect();
        out.push_str(&format!("moved={}\n", moved.join(";")));
        for (w, s) in &self.map {
            out.push_str(&format!("map.{w}={}\n", s.map_or("", Slot::key)));
        }
        out
    }

    pub fn parse(text: &str) -> Loadout {
        let mut l = Loadout::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            if let Some(s) = Slot::from_key(k) {
                l.set(s, if v.is_empty() { None } else { Equip::parse(v) });
            } else if k == "arms" && (v == "ct" || v == "t") {
                l.arms = v.into();
            } else if k == "crosshair" {
                l.crosshair = v.trim().into();
            } else if k == "crosshair_chroma" {
                l.crosshair_chroma = v.trim().parse::<f32>().unwrap_or(0.0).clamp(0.0, 3.0);
            } else if k == "crosshair_stage" {
                let f: Vec<u8> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                if let [bg, size] = f[..] {
                    l.xh_stage = (bg.min(3), size.min(2));
                }
            } else if k == "light" || k == "stage" {
                // (a stage's line, from before the showcase: its first number the stage it was)
                let f: Vec<f32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                let f = if k == "stage" { f.get(1..).unwrap_or_default() } else { &f[..] };
                if let [yaw, height] = f {
                    l.light = (*yaw, height.clamp(-1.3, 1.4));
                }
            } else if k == "quality" {
                if let Ok(q) = v.trim().parse::<u8>() {
                    l.quality = q.clamp(1, 3);
                }
            } else if k == "enabled" {
                l.on = v.trim() != "0";
            } else if k == "inspect" {
                l.inspect = v.trim().into();
            } else if k == "hand" {
                l.hand_game = v.trim() == "game";
                l.left = v.trim() == "left";
            } else if k == "moved" {
                l.moved = v
                    .split(';')
                    .filter_map(|m| {
                        let (n, rest) = m.split_once(':')?;
                        let (from, to) = rest.split_once('>')?;
                        Some((n.to_string(), from.to_string(), to.to_string()))
                    })
                    .collect();
            } else if let Some(w) = k.strip_prefix("map.") {
                if let Some(e) = l.map.iter_mut().find(|(n, _)| n == w) {
                    e.1 = Slot::from_key(v);
                }
            }
        }
        l
    }
}

/// A loadout kept under a name (the page's loadouts: gui/cosmsets.rs): what is in the hands — the knife, the gloves,
/// the AWP, the SSG 08 —, the side's arms and the crosshair.
#[derive(Clone, Debug, PartialEq)]
pub struct Set {
    pub name: String,
    pub knife: Option<Equip>,
    pub gloves: Option<Equip>,
    pub awp: Option<Equip>,
    pub ssg: Option<Equip>,
    pub arms: String,
    pub crosshair: String,
}

impl Set {
    pub fn of(name: &str, l: &Loadout) -> Set {
        Set { name: name.to_string(), knife: l.knife.clone(), gloves: l.gloves.clone(), awp: l.awp.clone(), ssg: l.ssg.clone(), arms: l.arms.clone(), crosshair: l.crosshair.clone() }
    }

    pub fn get(&self, s: Slot) -> Option<&Equip> {
        match s {
            Slot::Knife => self.knife.as_ref(),
            Slot::Gloves => self.gloves.as_ref(),
            Slot::Awp => self.awp.as_ref(),
            Slot::Ssg => self.ssg.as_ref(),
        }
    }

    /// Whether the loadout is this.
    pub fn is(&self, l: &Loadout) -> bool {
        Slot::ALL.into_iter().all(|s| self.get(s) == l.get(s)) && self.arms == l.arms && self.crosshair == l.crosshair
    }

    /// On: its things in the hands (CS's things on with them), its arms, its crosshair.
    pub fn put_on(&self, l: &mut Loadout) {
        for s in Slot::ALL {
            l.set(s, self.get(s).cloned());
        }
        l.arms = self.arms.clone();
        l.crosshair = self.crosshair.clone();
        l.on = true;
    }

    /// A line: its name, its four things, its arms, its crosshair (unit separators between).
    pub fn dump(&self) -> String {
        let e = |x: &Option<Equip>| x.as_ref().map_or(String::new(), Equip::dump);
        let name: String = self.name.chars().filter(|c| !c.is_control()).collect();
        [name, e(&self.knife), e(&self.gloves), e(&self.awp), e(&self.ssg), self.arms.clone(), self.crosshair.clone()].join("\u{1f}")
    }

    pub fn parse(line: &str) -> Option<Set> {
        let f: Vec<&str> = line.split('\u{1f}').collect();
        let [name, knife, gloves, awp, ssg, arms, crosshair] = f.as_slice() else { return None };
        let e = |v: &str| if v.is_empty() { None } else { Equip::parse(v) };
        Some(Set { name: name.to_string(), knife: e(knife), gloves: e(gloves), awp: e(awp), ssg: e(ssg), arms: if *arms == "t" { "t".into() } else { "ct".into() }, crosshair: crosshair.to_string() })
    }
}

/// The loadouts kept (cursed-recode/loadout_sets.txt beside the loadout's file).
fn sets_path() -> Option<std::path::PathBuf> {
    kept().lock().unwrap().path.as_ref().and_then(|p| p.parent()).map(|d| d.join("loadout_sets.txt"))
}

pub fn sets() -> Vec<Set> {
    let Some(p) = sets_path() else { return Vec::new() };
    std::fs::read_to_string(p).map(|t| t.lines().filter_map(Set::parse).collect()).unwrap_or_default()
}

pub fn save_sets(all: &[Set]) {
    let Some(p) = sets_path() else { return };
    let text: String = all.iter().map(|s| s.dump() + "\n").collect();
    let _ = std::fs::write(p, text);
}

struct Kept {
    l: Loadout,
    path: Option<std::path::PathBuf>,
    /// Bumped on each change (the hands and the page see it).
    version: u64,
}

fn kept() -> &'static Mutex<Kept> {
    static K: OnceLock<Mutex<Kept>> = OnceLock::new();
    K.get_or_init(|| Mutex::new(Kept { l: Loadout::default(), path: None, version: 1 }))
}

/// The file it lives in (cursed-recode/cosmetics.txt), read now.
pub fn set_path(path: std::path::PathBuf) {
    let mut k = kept().lock().unwrap();
    if k.path.as_ref() == Some(&path) {
        return;
    }
    if let Ok(t) = std::fs::read_to_string(&path) {
        k.l = Loadout::parse(&t);
        k.version += 1;
    }
    k.path = Some(path);
}

pub fn current() -> (Loadout, u64) {
    let k = kept().lock().unwrap();
    (k.l.clone(), k.version)
}

/// A change seen at once, its file written later (a picker's hand still down): the next `change` writes it.
pub fn change_quiet(f: impl FnOnce(&mut Loadout)) {
    let mut k = kept().lock().unwrap();
    f(&mut k.l);
    k.version += 1;
}

/// A change, kept at once.
pub fn change(f: impl FnOnce(&mut Loadout)) {
    let mut k = kept().lock().unwrap();
    f(&mut k.l);
    k.version += 1;
    if let Some(p) = &k.path {
        let _ = std::fs::write(p, k.l.dump());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_loadout_is_kept_and_read_back() {
        let mut l = Loadout::default();
        l.knife = Some(Equip { item: "weapon_knife_butterfly".into(), paint: "aa_fade".into(), seed: 412, wear: 0.012, stattrak: Some(37), charm: None, charm_offset: None, stickers: Vec::new(), nametag: Some("Lunuca's own".into()), overlay: Some(super::super::overlay::Overlay::of(8)) });
        let sticker = super::super::sticker::Placed { name: "dh_gologo1".into(), at: [0.01, 0.2, 0.11], normal: [1.0, 0.0, 0.0], up: [0.0, 1.0, 0.0], size: 0.04, wear: 0.0 };
        l.awp = Some(Equip { item: "weapon_awp".into(), paint: String::new(), seed: 0, wear: 0.0, stattrak: None, charm: Some("kc_missinglink_cat".into()), charm_offset: Some([0.0, 0.25, -0.5]), stickers: Vec::new(), nametag: None, overlay: None });
        l.ssg = Some(Equip { item: "weapon_ssg08".into(), paint: String::new(), seed: 0, wear: 0.0, stattrak: None, charm: None, charm_offset: None, stickers: vec![sticker.clone(), sticker], nametag: None, overlay: Some(super::super::overlay::Overlay::of(1)) });
        l.arms = "t".into();
        l.map[4].1 = Some(Slot::Knife);
        l.light = (1.25, -0.5);
        l.quality = 2;
        l.crosshair_chroma = 1.5;
        l.xh_stage = (3, 1);
        l.inspect = "G".into();
        l.hand_game = false;
        l.left = true;
        l.moved = vec![("key.swapOffhand".into(), "F".into(), "R".into())];
        l.on = true;
        let back = Loadout::parse(&l.dump());
        assert_eq!(back, l);
        // a stage's line from before the showcase keeps its light
        assert_eq!(Loadout::parse("stage=2,1.250,-0.500").light, (1.25, -0.5));
    }

    /// A loadout kept under a name reads back, and puts on what it kept.
    #[test]
    fn a_set_keeps_and_puts_on() {
        let mut l = Loadout::default();
        l.knife = Some(Equip { item: "weapon_knife_butterfly".into(), paint: "aa_fade".into(), seed: 412, wear: 0.012, stattrak: Some(37), charm: None, charm_offset: None, stickers: Vec::new(), nametag: None, overlay: None });
        l.gloves = None;
        l.arms = "t".into();
        l.crosshair = "CSabc".into();
        let s = Set::of("Fade «1»", &l);
        let back = Set::parse(&s.dump()).unwrap();
        assert_eq!(back, s);
        let mut other = Loadout::default();
        assert!(!s.is(&other));
        s.put_on(&mut other);
        assert!(s.is(&other) && other.on);
    }

    /// Off at first (a player's keys left alone); on with a knife or a rifle put on, worn; all of it taken off, not.
    #[test]
    fn worn_only_when_on_and_put_on() {
        let mut l = Loadout::default();
        assert!(!l.on && !l.worn());
        assert!(!Loadout::parse("").on);
        l.on = true;
        assert!(l.worn());
        l.knife = None;
        l.awp = None;
        assert!(l.worn());
        l.ssg = None;
        l.gloves = Some(Equip::plain("sporty_gloves"));
        assert!(!l.worn());
        l.awp = Some(Equip::plain("weapon_awp"));
        l.map.iter_mut().for_each(|m| m.1 = None);
        assert!(!l.worn());
    }

    #[test]
    fn the_games_weapons_find_their_slots() {
        let l = Loadout::default();
        assert_eq!(l.slot_for("minecraft:diamond_sword"), Some(Slot::Knife));
        assert_eq!(l.slot_for("minecraft:netherite_axe"), Some(Slot::Knife));
        assert_eq!(l.slot_for("minecraft:bow"), Some(Slot::Awp));
        assert_eq!(l.slot_for("minecraft:crossbow"), Some(Slot::Ssg));
        assert_eq!(l.slot_for("minecraft:trident"), None);
        assert_eq!(l.slot_for("minecraft:stick"), None);
    }
}
