//! The Cosmetics page (crate::cosmetics keeps the catalog, the loadout, the look at an item): its tabs — Counter-Strike
//! 2, Minecraft and Dota 2 (the last two still to come) — then CS2's things in cards as Neverlose shows them, each with
//! CS2's own inventory icon and its rarity's strip (her word: a strip only, never the finish touched); a knife's or a
//! glove's card opens its finishes; a finish's opens the look at it — the item itself, turned by the mouse (never on
//! its own; a fling carries on and slows), nearer by the wheel — its wear and its pattern to set, and on with it. What is
//! on is kept at once. Everything comes in: a view crosses over to the next, the cards one after another, a picture once
//! it is there (a soft pulse in its place till then), the item once all of it is.

use super::draw::{grey, hex, mix, Canvas};
use super::i18n::{ru, tr};
use super::text::{fonts, Face};
use super::theme::{fade, pal};
use super::cosmcharm::{CHARM_PICK, CHARM_ROW};
use super::cosmsticker::{DRAG_MOVE, DRAG_TURN, DRAG_WEAR, STICKER_OPPOSITE, STICKER_ROW, STICKER_TURN};
use super::cosmlook::{DRAG_STRENGTH, LOOK_ROW, LOOK_STRENGTH};
use super::cosmcode::{CODE_COPY, CODE_PASTE};
use super::cosmhand::HAND;
use super::cosmkeys::{ALL_ON, HAND_SIDE, INSPECT_KEY};
use super::cosmstage::DRAG_LIGHT;
use super::cosmtag::TAG_ROW;
use super::{ak, Gui, Hit, A_HOV};
use crate::cosmetics::catalog::{self, Kind};
use crate::cosmetics::loadout::{self, Equip, Slot};

/// The page's parts (Hit::Cosm).
/// The item looked at as the loadout keeps it (its StatTrak count the one on's, when it is the same item and finish).
fn look_equip(l: &Look, kind: Kind, on: Option<&Equip>) -> Equip {
    let kept = on.filter(|e| e.item == l.item && e.paint == l.paint).and_then(|e| e.stattrak);
    let stattrak = l.stattrak.then(|| kept.unwrap_or(0));
    let charm = l.charm.clone().filter(|_| kind == Kind::Rifle);
    let charm_offset = (charm.is_some() && l.charm_offset != [0.0; 3]).then_some(l.charm_offset);
    let (stickers, nametag, overlay) = (l.stickers.clone(), l.nametag.clone().filter(|n| !n.is_empty()), l.overlay);
    Equip { item: l.item.clone(), paint: l.paint.clone(), seed: l.seed, wear: l.wear, stattrak, charm, charm_offset, stickers, nametag, overlay }
}

/// The chip (CHIPS) an item is under.
fn chip_of(kind: Kind, item: &str) -> usize {
    let want = match kind {
        Kind::Knife => "Knives",
        Kind::Gloves => "Gloves",
        Kind::Rifle if item.contains("awp") => "AWP",
        Kind::Rifle => "SSG 08",
    };
    CHIPS.iter().position(|c| *c == want).unwrap_or(0)
}

const TAB: u8 = 1;
const CHIP: u8 = 2;
const ITEM: u8 = 3;
const FINISH: u8 = 4;
const BACK: u8 = 5;
const VIEW: u8 = 6;
const WEAR: u8 = 7;
const SEED: u8 = 8;
const SEED_DOWN: u8 = 9;
const SEED_UP: u8 = 10;
const EQUIP: u8 = 11;
const SIDE: u8 = 12;
const STATTRAK: u8 = 20;

/// The look's turn: radians a pixel of the pointer turns it; how far it tilts (past it, as an elastic, never past the
/// most); the spring the shown turn follows the hand's on (stiffness, damping: a touch behind it, settling with the
/// least overshoot).
const TURN: f32 = 0.0085;
const TILT: f32 = 0.55;
const TILT_MAX: f32 = 0.8;
const SPRING: (f32, f32) = (100.0, 15.0);

/// The look at a finish.
#[derive(Clone, Debug)]
pub struct Look {
    pub item: String,
    pub paint: String,
    pub seed: i32,
    pub wear: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub zoom: f32,
    pub at: f32,
    /// Where the wheel takes its nearness (it eases there). Its turn as the hand asks it (yaw, pitch: `yaw` and `pitch`
    /// follow it on a spring), how fast that runs on after a fling (radians a second), as the last frame had it; how
    /// fast the shown turn turns.
    pub zoom_to: f32,
    pub aim: [f32; 2],
    pub spin: [f32; 2],
    pub was: [f32; 2],
    pub vel: [f32; 2],
    /// The view's turn in the hands (yaw, pitch: radians), as the pointer drags it.
    pub hand_turn: [f32; 2],
    /// StatTrak™ (its module on the item, its kills counted).
    pub stattrak: bool,
    /// The charm on it (a rifle's): its name, where it hangs from (off its own place).
    pub charm: Option<String>,
    pub charm_offset: [f32; 3],
    /// The stickers on it, its name tag, our material over its finish.
    pub stickers: Vec<crate::cosmetics::sticker::Placed>,
    pub nametag: Option<String>,
    pub overlay: Option<crate::cosmetics::overlay::Overlay>,
}

#[derive(Default)]
pub struct Page {
    /// 0 Counter-Strike 2, 1 Minecraft, 2 Dota 2.
    tab: u8,
    /// 0 all, 1 knives, 2 gloves, 3 the AWP, 4 the SSG 08.
    pub(super) cat: u8,
    /// An item opened: its finishes.
    pub(super) item: Option<String>,
    pub(super) look: Option<Look>,
    /// The look's charms shown in its panel (cosmcharm.rs), scrolled; where they are.
    pub(super) picking_charm: bool,
    pub(super) charm_scroll: f32,
    pub(super) charm_scroll_to: f32,
    pub(super) charm_content: f32,
    pub(super) charm_area: [f32; 4],
    /// The look's stickers in its panel (cosmsticker.rs): shown, the place chosen, the search (typed into while
    /// `sticker_typing`) and what it found (for its words), scrolled; where they are; the chosen one's sliders' tracks.
    pub(super) picking_sticker: bool,
    pub(super) sticker_sel: usize,
    pub(super) sticker_q: String,
    pub(super) sticker_typing: bool,
    pub(super) sticker_found: (String, Vec<usize>),
    pub(super) sticker_scroll: f32,
    pub(super) sticker_scroll_to: f32,
    pub(super) sticker_content: f32,
    pub(super) sticker_area: [f32; 4],
    pub(super) sticker_tracks: [[f32; 2]; 2],
    /// The look's name tag typed into; its materials shown (cosmlook.rs), their strength's slider's track.
    pub(super) tag_typing: bool,
    pub(super) picking_look: bool,
    /// When the look's quality was last changed (its chip's value comes up into place).
    pub(super) quality_at: f32,
    /// the material's colour row (the picker opens by it); its colour changed while the picker's hand is down (the item
    /// on takes it as it lets go)
    pub(super) look_colour_at: [f32; 4],
    pub(super) look_dirty: bool,
    pub(super) look_track: [f32; 2],
    /// The look as the last frame showed it (where the pointer meets the item).
    pub(super) last_shown: Option<crate::cosmetics::preview::Preview>,
    scroll: f32,
    scroll_to: f32,
    content: f32,
    /// What is held: 1 the model (the pointer as it was), 2 the wear's slider, 3 the seed's, 10 + n the crosshair's
    /// slider n.
    pub(super) drag: Option<(u8, [f32; 2])>,
    /// CS2's cases (cosmcase.rs): the one looked into, its rare special items shown in its contents, the tab at its
    /// right (0 its contents, 1 the history), the opening going on; their cards scrolled, where; their sounds asked for.
    pub(super) case: Option<String>,
    pub(super) case_rares: bool,
    pub(super) case_tab: u8,
    pub(super) opening: Option<super::cosmcase::Spin>,
    pub(super) case_scroll: f32,
    pub(super) case_scroll_to: f32,
    pub(super) case_content: f32,
    pub(super) case_area: [f32; 4],
    pub(super) case_sounds: bool,
    /// The loadouts kept (cosmsets.rs): the one whose name is typed, what is typed; their cards scrolled, where.
    pub(super) set_typing: Option<usize>,
    pub(super) set_buf: String,
    pub(super) sets_scroll: f32,
    pub(super) sets_scroll_to: f32,
    pub(super) sets_content: f32,
    pub(super) sets_area: [f32; 4],
    /// A sticker or a charm carried from the panel onto the item (cosmsticker.rs, cosmcharm.rs); how far from the
    /// pointer a sticker on the item was taken.
    pub(super) carry: Option<super::cosmsticker::Carry>,
    pub(super) carry_charm: Option<super::cosmcharm::CharmCarry>,
    pub(super) grab_off: [f32; 2],
    /// The crosshair's sliders' tracks as drawn, its colour's row (the picker opens by it), its colour changed while
    /// the picker's hand is down (written as it lets go); the clipboard asked for its code.
    pub(super) x_tracks: [[f32; 2]; 4],
    pub(super) x_colour_at: [f32; 4],
    pub(super) x_dirty: bool,
    pub(super) paste: bool,
    /// The clipboard asked for an item's code (cosmcode.rs).
    pub(super) paste_item: bool,
    /// The sliders' tracks as drawn (x0, x1).
    wear_track: [f32; 2],
    seed_track: [f32; 2],
    /// The look at an item this frame shows (the menu's frame takes it).
    shown: Option<crate::cosmetics::preview::Preview>,
    /// The icons asked for (by key, when): one the atlas let go is asked for again.
    asked: std::collections::HashMap<String, f32>,
    /// When each picture was first there (it comes in from then).
    seen: std::collections::HashMap<String, f32>,
    /// The look's panel: what it shows (0 its rows, 1 the charms, 2 the stickers, 3 the materials), since when.
    panel: (u8, f32),
    /// The look just left: the item going away (as it was last shown, since when).
    leaving: Option<(crate::cosmetics::preview::Preview, f32)>,
    /// Where (in the page's shapes as last drawn) the tabs end, and the chips: what stands through a change is left
    /// out of what goes away.
    marks: (usize, usize),
    /// The look in CS's hands (cosmhand.rs), kept from one item to the next.
    pub(super) in_hand: bool,
    /// Where a press on the item's place began, whether the pointer has left it since (a click, or a drag).
    press: Option<([f32; 2], bool)>,
    /// Words that change in place (cosm_words, by their key): what they were, what they are, since when.
    words: std::collections::HashMap<usize, (String, String, f32)>,
    /// The inspect key's chip waits for its new key (cosmkeys.rs); the notes of the game's controls it moved, to be
    /// shown once the HUD is seen.
    pub(super) key_listen: bool,
    pub(super) notes_due: Vec<(String, String)>,
}

/// The icons that are not among the files (never asked for again).
static MISSING: std::sync::Mutex<Option<std::collections::HashSet<String>>> = std::sync::Mutex::new(None);

const CHIPS: [&str; 8] = ["All", "Knives", "Gloves", "AWP", "SSG 08", "Crosshair", "Cases", "Loadouts"];
/// The chips of CS's crosshair (cosmxhair.rs), of CS2's cases (cosmcase.rs), of the loadouts kept (cosmsets.rs).
const CROSSHAIR: u8 = 5;
pub(super) const CASES: u8 = 6;
pub(super) const SETS: u8 = 7;
const TABS: [&str; 3] = ["Counter-Strike 2", "Minecraft", "Dota 2"];

/// CS's names of wear.
pub(super) fn wear_name(w: f32) -> &'static str {
    match w {
        x if x < 0.07 => "Factory New",
        x if x < 0.15 => "Minimal Wear",
        x if x < 0.38 => "Field-Tested",
        x if x < 0.45 => "Well-Worn",
        _ => "Battle-Scarred",
    }
}

/// "{n} skins" in the game's language (its plural).
pub(super) fn skins(n: usize) -> String {
    let form = |one: &str, few: &str, many: &str| {
        let w = match (n % 10, n % 100) {
            (1, m) if m != 11 => one,
            (2..=4, m) if !(12..=14).contains(&m) => few,
            _ => many,
        };
        format!("{n} {w}")
    };
    match super::i18n::lang() {
        super::i18n::Lang::Ru => form("скин", "скина", "скинов"),
        super::i18n::Lang::Uk => form("скін", "скіни", "скінів"),
        super::i18n::Lang::Kk => format!("{n} скин"),
        _ => format!("{n} {}", if n == 1 { "skin" } else { "skins" }),
    }
}

/// The items a chip shows, in the page's order: the knives, the gloves, the rifles, each by its name as shown.
fn shown_items(cat: &catalog::Catalog, chip: u8) -> Vec<&catalog::Item> {
    let rank = |k: Kind| match k {
        Kind::Knife => 0,
        Kind::Gloves => 1,
        Kind::Rifle => 2,
    };
    let mut v: Vec<&catalog::Item> = cat
        .items
        .iter()
        .filter(|it| match chip {
            1 => it.kind == Kind::Knife,
            2 => it.kind == Kind::Gloves,
            3 => it.name == "weapon_awp",
            4 => it.name == "weapon_ssg08",
            _ => true,
        })
        .collect();
    v.sort_by(|a, b| rank(a.kind).cmp(&rank(b.kind)).then_with(|| name_of(a).cmp(name_of(b))));
    v
}

/// Whether an item has an unpainted card (gloves have none: CS2 has no plain pair).
fn has_vanilla(kind: Kind) -> bool {
    kind != Kind::Gloves
}

fn name_of(item: &catalog::Item) -> &str {
    if ru() {
        &item.ru
    } else {
        &item.en
    }
}

/// An icon's key among the menu's pictures, and its file.
fn icon_key(item: &str, paint: &str) -> (String, String) {
    let p = if paint.is_empty() { "default" } else { paint };
    (format!("cosm:{item}:{p}"), format!("icons/{item}/{p}.rtx"))
}

impl Gui {
    /// An icon (its place in the atlas and size), asked for when it is not there yet (read on a thread of its own).
    /// A picture by its file (an .rtx under the cosmetics' folder) at its first mip at most `most` wide, asked for when
    /// it is not there yet (read on a thread of its own).
    pub(super) fn cosm_pic(&mut self, file: &str, most: u32) -> Option<([f32; 4], [u32; 2], f32)> {
        let key = format!("pic:{file}:{most}");
        if let Some((uv, size)) = super::pics::get(&key) {
            return Some((uv, size, self.cosm_in(&key, true)));
        }
        self.cosm_in(&key, false);
        let now = self.now;
        let due = self.cosm.asked.get(&key).is_none_or(|at| now - at > 2.0);
        if due && !MISSING.lock().unwrap().as_ref().is_some_and(|m| m.contains(&key)) {
            self.cosm.asked.insert(key.clone(), now);
            let file = file.to_string();
            std::thread::spawn(move || {
                let img = crate::cosmetics::store::read_wait(&file).and_then(|d| crate::cosmetics::texture::Image::parse(&d).ok());
                let mip = img.as_ref().filter(|i| !i.wide).and_then(|i| {
                    let (mut w, mut h, mut k) = (i.width, i.height, 0);
                    while w > most && k + 1 < i.mips.len() {
                        w = (w / 2).max(1);
                        h = (h / 2).max(1);
                        k += 1;
                    }
                    i.mips.get(k).map(|px| (w, h, px.clone()))
                });
                match mip {
                    Some((w, h, px)) => super::pics::put(&key, w, h, px),
                    None => {
                        MISSING.lock().unwrap().get_or_insert_with(Default::default).insert(key);
                    }
                }
            });
        }
        None
    }

    /// An icon (as cosm_pic: its place, its size, how far it has come in).
    pub(super) fn cosm_icon(&mut self, item: &str, paint: &str) -> Option<([f32; 4], [u32; 2], f32)> {
        let (key, file) = icon_key(item, paint);
        if let Some((uv, size)) = super::pics::get(&key) {
            return Some((uv, size, self.cosm_in(&key, true)));
        }
        self.cosm_in(&key, false);
        let now = self.now;
        let due = self.cosm.asked.get(&key).is_none_or(|at| now - at > 2.0);
        if due && !MISSING.lock().unwrap().as_ref().is_some_and(|m| m.contains(&key)) {
            self.cosm.asked.insert(key.clone(), now);
            std::thread::spawn(move || {
                let img = crate::cosmetics::store::read_wait(&file).and_then(|d| crate::cosmetics::texture::Image::parse(&d).ok());
                match img.as_ref().and_then(|i| i.mips.first().map(|px| (i, px))) {
                    Some((i, px)) if !i.wide => super::pics::put(&key, i.width, i.height, px.clone()),
                    _ => {
                        MISSING.lock().unwrap().get_or_insert_with(Default::default).insert(key);
                    }
                }
            });
        }
        None
    }

    /// How far a picture (by its key) has come in since it was first there (`there`: it is now; else it is forgotten,
    /// to come in again).
    fn cosm_in(&mut self, key: &str, there: bool) -> f32 {
        if !there {
            self.cosm.seen.remove(key);
            return 0.0;
        }
        if !pal().anim {
            return 1.0;
        }
        let now = self.now;
        let at = *self.cosm.seen.entry(key.to_string()).or_insert(now);
        super::smooth(0.0, 0.3, now - at)
    }

    /// A picture's place while it is not there yet (`k` how far it has come in): a soft light pulsing in it, going as
    /// the picture comes.
    pub(super) fn cosm_waiting(&self, c: &mut Canvas, b: [f32; 4], k: f32, n: usize) {
        if k >= 1.0 || !pal().anim {
            return;
        }
        let pulse = 0.5 + 0.5 * (self.now * 2.6 - n as f32 * 0.45).sin();
        c.rect(b, 0.5 * self.r, grey(1.0, (0.018 + 0.03 * pulse) * (1.0 - k)));
    }

    /// Words that change in place crossing over (`key` whose): the old going up and away, the new coming up into it
    /// (`centre`: centred on `x`, else from it); how wide they are, eased between the two.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn cosm_words(&mut self, c: &mut Canvas, key: usize, x: f32, base: f32, size: f32, face: Face, s: &str, col: super::draw::Rgba, centre: bool) -> f32 {
        let now = self.now;
        let e = self.cosm.words.entry(key).or_insert_with(|| (String::new(), s.to_string(), -10.0));
        if e.1 != s {
            *e = (std::mem::take(&mut e.1), s.to_string(), now);
        }
        let k = if pal().anim { super::ease_out((now - e.2) / 0.24) } else { 1.0 };
        let old = e.0.clone();
        let rise = 0.45 * size;
        let mut put = |text: &str, a: f32, dy: f32| {
            let col = [col[0], col[1], col[2], col[3] * a];
            if centre {
                c.text_c(x, base + dy, size, face, text, col);
            } else {
                c.text(x, base + dy, size, face, text, col);
            }
        };
        if k < 1.0 && !old.is_empty() {
            put(&old, (1.0 - k) * (1.0 - k), -rise * k);
        }
        put(s, k, rise * (1.0 - k));
        let (wo, wn) = (fonts().width(face, size, &old), fonts().width(face, size, s));
        if k < 1.0 && !old.is_empty() {
            wo + (wn - wo) * k
        } else {
            wn
        }
    }

    /// Three dots breathing (something coming), `a` as seen.
    pub(super) fn cosm_dots(&self, c: &mut Canvas, m: [f32; 2], a: f32) {
        let r = self.r;
        for i in 0..3 {
            let p = 0.5 + 0.5 * (self.now * 4.2 - i as f32 * 0.9).sin();
            let d = (0.16 + 0.05 * p) * r;
            let x = m[0] + (i as f32 - 1.0) * 0.75 * r;
            c.rect([x - d, m[1] - d, x + d, m[1] + d], d, grey(1.0, (0.18 + 0.45 * p) * a));
        }
    }

    /// The light under the chosen one of a row of chips (`rects`), slid to it from where it was (`anim` its two
    /// keys).
    pub(super) fn cosm_pill(&mut self, c: &mut Canvas, rects: &[[f32; 4]], on: usize, anim: usize, radius: f32, col: super::draw::Rgba) {
        let Some(b) = rects.get(on) else { return };
        let x = self.an(ak(A_HOV, anim), b[0], 16.0, b[0]);
        let w = self.an(ak(A_HOV, anim + 1), b[2] - b[0], 16.0, b[2] - b[0]);
        c.rect([x, b[1], x + w, b[3]], radius, col);
    }

    /// What the page shows is to change: what it showed goes away (all of it under the tabs; `keep_chips`: under the
    /// chips too, which stay) as the next comes in.
    pub(super) fn cosm_swap(&mut self, keep_chips: bool) {
        self.swap(false);
        let m = if keep_chips { self.cosm.marks.1 } else { self.cosm.marks.0 } * super::draw::FLOATS;
        if m <= self.swap_from.len() {
            self.swap_from.drain(..m);
        }
    }

    pub(super) fn cosm_page(&mut self, c: &mut Canvas, area: [f32; 4]) {
        let r = self.r;
        // the tabs and the chips stand through a change of what is under them (only the window's own rising); the rest
        // crosses over
        let (aw, dyw) = self.rise(0.22);
        let (a, dyv) = self.rise_c(0.22, 0.0, false);
        let lift = dyv - dyw;
        c.alpha = aw;
        let top = area[1] + dyw;
        let start = c.p.len() / super::draw::FLOATS;
        // the tabs (the chosen one's light slides to it)
        let mut tx = area[0];
        let mut rects = Vec::with_capacity(TABS.len());
        for t in TABS.iter() {
            let tw = fonts().width(Face::Semi, 0.86 * r, t) + 1.8 * r;
            rects.push([tx, top, tx + tw, top + 2.2 * r]);
            tx += tw + 0.5 * r;
        }
        for (k, tr_) in rects.iter().enumerate() {
            let hov = self.an(ak(A_HOV, 36_100 + k), if self.hover == Hit::Cosm(TAB, k as u16) { 1.0 } else { 0.0 }, 14.0, 0.0);
            c.rect(*tr_, 0.7 * r, grey(1.0, 0.03 + 0.03 * hov));
        }
        self.cosm_pill(c, &rects, self.cosm.tab as usize, 36_000, 0.7 * r, grey(1.0, 0.07));
        for (k, (t, tr_)) in TABS.iter().zip(rects).enumerate() {
            let hit = Hit::Cosm(TAB, k as u16);
            let on = self.cosm.tab == k as u8;
            let hv = self.an(ak(A_HOV, 31_000 + k), if on { 1.0 } else if self.hover == hit { 0.5 } else { 0.0 }, 14.0, 0.0);
            c.text(tr_[0] + 0.9 * r, top + 1.45 * r, 0.86 * r, Face::Semi, t, mix(pal().t3, pal().t1, hv));
            self.hits.push((tr_, hit));
        }
        let tabs = c.p.len() / super::draw::FLOATS - start;
        self.cosm.marks = (tabs, tabs);
        c.alpha = a;
        let body = [area[0], top + 3.0 * r, area[2], area[3]];
        if self.cosm.tab != 0 {
            let cx = (body[0] + body[2]) / 2.0;
            let cy = body[1] + 8.0 * r + lift;
            c.text_c(cx, cy, 1.6 * r, Face::Bold, TABS[self.cosm.tab as usize], grey(1.0, 0.9));
            c.text_c(cx, cy + 1.8 * r, 0.8 * r, Face::Regular, tr("In a later build"), pal().t3);
            c.alpha = 1.0;
            return;
        }
        if self.cosm.look.is_some() {
            self.cosm_look(c, [body[0], body[1] + lift, body[2], body[3]]);
        } else {
            // the item just left goes away where it stood
            if let Some((p, at)) = self.cosm.leaving.clone() {
                let k = 1.0 - ((self.now - at) / 0.2).clamp(0.0, 1.0);
                if k > 0.0 && pal().anim {
                    self.cosm.shown = Some(crate::cosmetics::preview::Preview { alpha: k * k, ..p });
                } else {
                    self.cosm.leaving = None;
                }
            }
            self.cosm_grid(c, body, start);
        }
        c.alpha = 1.0;
    }

    /// The cards: the chips over them, then the items (a category's), or an item's finishes (`start`: the page's first
    /// shape).
    fn cosm_grid(&mut self, c: &mut Canvas, body: [f32; 4], start: usize) {
        let r = self.r;
        let cat = catalog::get();
        let (lo, _) = loadout::current();
        let (aw, dyw) = self.rise(0.22);
        let (a, dyv) = self.rise_c(0.22, 0.0, false);
        let lift = dyv - dyw;
        let mut y = body[1];
        // the chips (standing), or the way back with the item's name (crossing over)
        if let Some(item) = self.cosm.item.clone() {
            let y = y + lift;
            let hit = Hit::Cosm(BACK, 0);
            let hv = self.an(ak(A_HOV, 31_100), if self.hover == hit { 1.0 } else { 0.0 }, 14.0, 0.0);
            let b = [body[0], y, body[0] + 2.2 * r, y + 2.2 * r];
            c.rect(b, 0.7 * r, grey(1.0, 0.04 + 0.06 * hv));
            let m = [(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0];
            c.polyline(&[[m[0] + 0.25 * r, m[1] - 0.45 * r], [m[0] - 0.25 * r, m[1]], [m[0] + 0.25 * r, m[1] + 0.45 * r]], 1.6, mix(pal().t3, pal().t1, hv));
            self.hits.push((b, hit));
            let name = cat.item(&item).map_or(item.clone(), |i| name_of(i).to_string());
            c.text(b[2] + 0.8 * r, y + 1.5 * r, 1.0 * r, Face::Bold, &name, pal().t1);
        } else {
            c.alpha = aw;
            let mut x = body[0];
            let mut rects = Vec::with_capacity(CHIPS.len());
            for label in CHIPS.iter() {
                let w = fonts().width(Face::Medium, 0.74 * r, tr(label)) + 1.4 * r;
                rects.push([x, y, x + w, y + 1.9 * r]);
                x += w + 0.45 * r;
            }
            for (k, b) in rects.iter().enumerate() {
                let hov = self.an(ak(A_HOV, 36_200 + k), if self.hover == Hit::Cosm(CHIP, k as u16) { 1.0 } else { 0.0 }, 14.0, 0.0);
                c.rect(*b, 0.95 * r, mix(pal().f1, pal().f2, hov));
            }
            self.cosm_pill(c, &rects, self.cosm.cat as usize, 36_010, 0.95 * r, pal().f3);
            for (k, (label, b)) in CHIPS.iter().zip(rects).enumerate() {
                let hit = Hit::Cosm(CHIP, k as u16);
                let on = self.cosm.cat == k as u8;
                let hv = self.an(ak(A_HOV, 31_200 + k), if on { 1.0 } else if self.hover == hit { 0.45 } else { 0.0 }, 14.0, 0.0);
                c.text(b[0] + 0.7 * r, y + 1.28 * r, 0.74 * r, Face::Medium, tr(label), mix(pal().t3, pal().t1, hv));
                self.hits.push((b, hit));
            }
            // the hands' side, at the right (its words crossing over as it changes)
            let label = format!("{} {}", tr("Hands"), lo.arms.to_uppercase());
            let w = fonts().width(Face::Medium, 0.74 * r, &label) + 1.4 * r;
            let w = self.an(ak(A_HOV, 36_600), w, 14.0, w);
            // (all of it on or off, at the far right)
            let right = self.keys_switch(c, body[2], y);
            let b = [right - w, y, right, y + 1.9 * r];
            let hit = Hit::Cosm(SIDE, 0);
            let hv = self.an(ak(A_HOV, 31_300), if self.hover == hit { 1.0 } else { 0.0 }, 14.0, 0.0);
            c.rect(b, 0.95 * r, mix(pal().f1, pal().f3, hv));
            c.push_clip(b);
            self.cosm_words(c, 36_600, b[0] + 0.7 * r, y + 1.28 * r, 0.74 * r, Face::Medium, &label, mix(pal().t2, pal().t1, hv), false);
            c.pop_clip();
            self.hits.push((b, hit));
            // the inspect key, the hand, before it
            self.keys_chips(c, b[0] - 0.45 * r, y);
            self.cosm.marks.1 = c.p.len() / super::draw::FLOATS - start;
            c.alpha = a;
        }
        y += 3.0 * r;
        if self.cosm.item.is_none() && self.cosm.cat == CROSSHAIR {
            self.cosm_crosshair(c, [body[0], y + lift, body[2], body[3]]);
            return;
        }
        if self.cosm.item.is_none() && self.cosm.cat == CASES {
            self.cosm_cases(c, [body[0], y + lift, body[2], body[3]]);
            return;
        }
        if self.cosm.item.is_none() && self.cosm.cat == SETS {
            self.cosm_sets(c, [body[0], y + lift, body[2], body[3]]);
            return;
        }
        // (the picker on the crosshair's colour goes with its editor)
        if self.pick.as_ref().is_some_and(|p| p.owner == super::Owner::Cosm && p.set == 1) {
            self.pick = None;
        }
        // the cards: (item, paint, name, its second line, rarity's colour, on)
        let mut cards: Vec<(String, String, String, String, u32, bool)> = Vec::new();
        match &self.cosm.item {
            Some(item) => {
                let it = cat.item(item).cloned();
                let kind = it.as_ref().map_or(Kind::Knife, |i| i.kind);
                let slot = Slot::of(item, kind);
                let on = lo.get(slot).cloned();
                let base = it.as_ref().map_or(item.clone(), |i| name_of(i).to_string());
                // the item unpainted (gloves have none in CS2: theirs is the side's own pair, gloves off)
                if has_vanilla(kind) {
                    let vanilla = on.as_ref().is_some_and(|e| e.item == *item && e.paint.is_empty());
                    cards.push((item.clone(), String::new(), base, tr("Vanilla").to_string(), catalog::rarity_colour(kind, ""), vanilla));
                }
                for f in cat.finishes_of(item) {
                    let rarity = it.as_ref().map_or("", |i| catalog::rarity_on(i, f));
                    let on_this = on.as_ref().is_some_and(|e| e.item == *item && e.paint == f.paint);
                    let second = tr(catalog::rarity_name(kind, rarity)).to_string();
                    cards.push((item.clone(), f.paint.clone(), catalog::finish_name(f, ru()), second, catalog::rarity_colour(kind, rarity), on_this));
                }
            }
            None => {
                for it in shown_items(&cat, self.cosm.cat) {
                    let slot = Slot::of(&it.name, it.kind);
                    let on = lo.get(slot).is_some_and(|e| e.item == it.name);
                    let n = cat.finishes_of(&it.name).count();
                    let second = skins(n);
                    // its picture: its plain one, or (gloves: CS2 has no plain pair) its first finish's
                    let pic = if has_vanilla(it.kind) { String::new() } else { cat.finishes_of(&it.name).next().map_or(String::new(), |f| f.paint.clone()) };
                    cards.push((it.name.clone(), pic, name_of(it).to_string(), second, catalog::rarity_colour(it.kind, ""), on));
                }
            }
        }
        // none yet: the catalog still coming from the server (three dots breathing), or the server not answering
        if cards.is_empty() && self.cosm.item.is_none() && cat.items.is_empty() {
            let m = [(body[0] + body[2]) * 0.5, y + (body[3] - y) * 0.4];
            let coming = crate::cosmetics::store::coming("catalog.txt") || crate::cosmetics::store::busy();
            if coming {
                self.cosm_dots(c, m, 1.0);
            }
            let line = if coming { tr("Fetching CS2's things…") } else { tr("No answer from the server: CS2's things come when it answers") };
            c.text_c(m[0], m[1] + 2.2 * r, 0.74 * r, Face::Medium, line, pal().t3);
            return;
        }
        // laid in columns, scrolled
        let gap = 0.7 * r;
        let cw_min = 9.5 * r;
        let cols = (((body[2] - body[0]) + gap) / (cw_min + gap)).floor().max(1.0) as usize;
        let cw = ((body[2] - body[0]) - gap * (cols as f32 - 1.0)) / cols as f32;
        // the icon (4:3, as CS2 makes them), the name and the second line under it, the strip at the bottom
        let ih = (cw - r) * 0.75;
        let ch = 0.4 * r + ih + 2.9 * r;
        let rows = cards.len().div_ceil(cols);
        let view = [body[0], y, body[2], body[3]];
        self.cosm.content = (rows as f32 * (ch + gap) - gap - (view[3] - view[1])).max(0.0);
        self.cosm.scroll_to = self.cosm.scroll_to.clamp(0.0, self.cosm.content);
        self.cosm.scroll += (self.cosm.scroll_to - self.cosm.scroll) * (1.0 - (-self.dt * 14.0).exp());
        c.push_clip(view);
        let part = if self.cosm.item.is_some() { FINISH } else { ITEM };
        // (the page's own rising: a card comes in a beat after the one before it)
        let (a0, dy0) = (a, dyw);
        let mut shown = 0usize;
        for (n, (item, paint, name, second, col, on)) in cards.iter().enumerate() {
            let (cx, cy) = (n % cols, n / cols);
            let x0 = view[0] + cx as f32 * (cw + gap);
            let y0 = view[1] + cy as f32 * (ch + gap) - self.cosm.scroll;
            if y0 > view[3] || y0 + ch < view[1] {
                continue;
            }
            let (ca, cdy) = self.rise_c(0.22, 0.022 * shown.min(20) as f32, false);
            shown += 1;
            c.alpha = ca;
            let y0 = y0 + cdy - dy0;
            let rr = [x0, y0, x0 + cw, y0 + ch];
            let hit = Hit::Cosm(part, n as u16);
            let hv = self.an(ak(A_HOV, 32_000 + n), if self.hover == hit { 1.0 } else { 0.0 }, 14.0, 0.0);
            c.rect_lit(rr, 0.8 * r, mix(pal().f1, pal().f2, hv), grey(1.0, 0.04 * hv), 2.0);
            // the icon (CS2's own)
            let ir = [rr[0] + 0.5 * r, rr[1] + 0.4 * r, rr[2] - 0.5 * r, rr[1] + 0.4 * r + ih];
            let icon = self.cosm_icon(item, paint);
            let seen = icon.map_or(0.0, |i| i.2);
            self.cosm_waiting(c, [ir[0] + 1.2 * r, ir[1] + 0.6 * r, ir[2] - 1.2 * r, ir[3] - 0.6 * r], seen, n);
            if let Some((uv, [iw, ih], seen)) = icon {
                // its own shape kept, in the middle of its place, a little bigger under the pointer (and coming up
                // into it as it comes in)
                let k = ((ir[2] - ir[0]) / iw as f32).min((ir[3] - ir[1]) / ih as f32) * (1.0 + 0.04 * hv) * (0.96 + 0.04 * seen);
                let (w, h) = (iw as f32 * k, ih as f32 * k);
                let m = [(ir[0] + ir[2]) / 2.0, (ir[1] + ir[3]) / 2.0];
                c.pic([m[0] - w / 2.0, m[1] - h / 2.0, m[0] + w / 2.0, m[1] + h / 2.0], uv, seen, false, 1.0, 1.0, 0.0);
            }
            let nm = fonts().fit(Face::Semi, 0.78 * r, name, cw - 1.6 * r);
            c.text(rr[0] + 0.8 * r, ir[3] + 1.05 * r, 0.78 * r, Face::Semi, &nm, mix(pal().t2, pal().t1, hv));
            let sc = fonts().fit(Face::Regular, 0.62 * r, second, cw - 1.6 * r);
            c.text(rr[0] + 0.8 * r, ir[3] + 1.9 * r, 0.62 * r, Face::Regular, &sc, pal().t4);
            // the rarity's strip
            c.rect([rr[0] + 0.8 * r, rr[3] - 0.42 * r, rr[2] - 0.8 * r, rr[3] - 0.26 * r], 0.08 * r, hex(*col, 0.9));
            if *on {
                // what is on: a tick in the corner
                let m = [rr[2] - 1.1 * r, rr[1] + 1.1 * r];
                c.rect([m[0] - 0.6 * r, m[1] - 0.6 * r, m[0] + 0.6 * r, m[1] + 0.6 * r], 0.6 * r, grey(1.0, 0.9));
                c.polyline(&[[m[0] - 0.3 * r, m[1]], [m[0] - 0.08 * r, m[1] + 0.24 * r], [m[0] + 0.32 * r, m[1] - 0.24 * r]], 1.6, grey(0.0, 0.9));
            }
            self.hits.push(([rr[0], rr[1].max(view[1]), rr[2], rr[3].min(view[3])], hit));
        }
        c.alpha = a0;
        c.pop_clip();
    }

    /// The look at a finish: the item (the engine draws it in the part kept for it), its name, its wear, its pattern,
    /// on with it.
    fn cosm_look(&mut self, c: &mut Canvas, body: [f32; 4]) {
        let r = self.r;
        self.look_motion();
        let Some(look) = self.cosm.look.clone() else { return };
        let cat = catalog::get();
        let it = cat.item(&look.item).cloned();
        let f = cat.finish(&look.item, &look.paint).cloned();
        let (lo, _) = loadout::current();
        let k = ((self.now - look.at) / 0.35).clamp(0.0, 1.0);
        let k = 1.0 - (1.0 - k).powi(3);
        // the way back
        let hit = Hit::Cosm(BACK, 1);
        let hv = self.an(ak(A_HOV, 31_101), if self.hover == hit { 1.0 } else { 0.0 }, 14.0, 0.0);
        let b = [body[0], body[1], body[0] + 2.2 * r, body[1] + 2.2 * r];
        c.rect(b, 0.7 * r, grey(1.0, 0.04 + 0.06 * hv));
        let m = [(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0];
        c.polyline(&[[m[0] + 0.25 * r, m[1] - 0.45 * r], [m[0] - 0.25 * r, m[1]], [m[0] + 0.25 * r, m[1] + 0.45 * r]], 1.6, mix(pal().t3, pal().t1, hv));
        self.hits.push((b, hit));
        // where one is (each step pressed: back to it): the kind, the item, the finish, the panel open
        let kind_of = it.as_ref().map_or(Kind::Knife, |i| i.kind);
        let panel = if self.cosm.picking_charm {
            Some("Charm")
        } else if self.cosm.picking_sticker {
            Some("Stickers")
        } else if self.cosm.picking_look {
            Some("Material")
        } else {
            None
        };
        let crumbs: Vec<(String, Option<u16>)> = [
            Some((tr(CHIPS[chip_of(kind_of, &look.item)]).to_string(), Some(2))),
            Some((it.as_ref().map_or(look.item.clone(), |i| name_of(i).to_string()), Some(1))),
            Some((f.as_ref().map_or(tr("Vanilla").to_string(), |f| catalog::finish_name(f, ru())), panel.map(|_| 3))),
            panel.map(|p| (tr(p).to_string(), None)),
        ]
        .into_iter()
        .flatten()
        .collect();
        let mut cx = b[2] + 0.8 * r;
        for (k, (label, to)) in crumbs.iter().enumerate() {
            if k > 0 {
                c.text(cx, body[1] + 1.45 * r, 0.78 * r, Face::Regular, "›", pal().t4);
                cx += 0.9 * r;
            }
            let last = k + 1 == crumbs.len();
            let w = fonts().width(Face::Medium, 0.78 * r, label);
            let hb = [cx - 0.3 * r, body[1] + 0.3 * r, cx + w + 0.3 * r, body[1] + 1.9 * r];
            let ch = to.map(|n| Hit::Cosm(BACK, n));
            let hv = self.an(ak(A_HOV, 31_120 + k), if ch.is_some_and(|h| self.hover == h) { 1.0 } else { 0.0 }, 14.0, 0.0);
            let col = if last { pal().t1 } else { mix(pal().t3, pal().t1, hv) };
            c.text(cx, body[1] + 1.45 * r, 0.78 * r, if last { Face::Semi } else { Face::Medium }, label, col);
            if let Some(h) = ch {
                c.rect([cx, body[1] + 1.72 * r, cx + w, body[1] + 1.72 * r + 1.0], 0.5, fade(pal().t1, 0.4 * hv));
                self.hits.push((hb, h));
            }
            cx += w + 0.5 * r;
        }
        // the picker on the material's colour goes with its panel
        if !self.cosm.picking_look && self.pick.as_ref().is_some_and(|p| p.owner == super::Owner::Cosm && p.set == 0) {
            self.pick = None;
        }
        // its changes on the one worn at once (the item and its finish on: wear, pattern and the rest are it, tuned)
        if self.cosm.drag.is_none() {
            let slot = Slot::of(&look.item, kind_of);
            if let Some(e) = lo.get(slot).filter(|e| e.item == look.item && e.paint == look.paint) {
                let now = look_equip(&look, kind_of, Some(e));
                if now != *e {
                    loadout::change(|lo| lo.set(slot, Some(now)));
                }
            }
        }
        // the item: the left of the page, the panel at its right
        let panel_w = (16.0 * r).min((body[2] - body[0]) * 0.42);
        let view = [body[0], body[1] + 2.9 * r, body[2] - panel_w - 0.8 * r, body[3]];
        // where the engine draws it (over the floor, its reflection under it in the whole place): the window's own scaling
        // and lift taken in (as the faces of Alt Manager's card)
        let [mx, my, sc, drop, _] = self.xf;
        let at = |x: f32, y: f32| [mx + (x - mx) * sc, my + (y - my) * sc + drop];
        let stand = self.stage_place(view);
        let p0 = at(stand[0], stand[1]);
        let p1 = at(stand[2], stand[3]);
        let (f0, f1) = (at(view[0], view[1]), at(view[2], view[3]));
        let shown = (k > 0.2).then(|| crate::cosmetics::preview::Preview {
            rect: [p0[0], p0[1], p1[0], p1[1]],
            frame: [f0[0], f0[1], f1[0], f1[1]],
            item: look.item.clone(),
            paint: look.paint.clone(),
            seed: look.seed,
            wear: look.wear,
            yaw: if self.cosm.in_hand { look.hand_turn[0] } else { look.yaw },
            pitch: if self.cosm.in_hand { look.hand_turn[1] } else { look.pitch },
            zoom: look.zoom,
            stattrak: look.stattrak.then(|| {
                let on = lo.get(Slot::of(&look.item, it.as_ref().map_or(Kind::Knife, |i| i.kind)));
                on.filter(|e| e.item == look.item && e.paint == look.paint).and_then(|e| e.stattrak).unwrap_or(0)
            }),
            charm: look.charm.clone().filter(|_| it.as_ref().is_some_and(|i| i.kind == Kind::Rifle)),
            charm_offset: look.charm_offset,
            stickers: look.stickers.clone(),
            nametag: look.nametag.clone().filter(|_| it.as_ref().is_some_and(|i| i.kind != Kind::Gloves)),
            overlay: look.overlay,
            light: [lo.light.0, lo.light.1],
            alpha: 1.0,
            in_hand: self.cosm.in_hand,
            ss: lo.quality,
        });
        self.showcase(c, view, k, shown.as_ref());
        self.hits.push((view, Hit::Cosm(VIEW, 0)));
        self.hand_chips(c, view);
        self.quality_chip(c, view);
        c.push_clip(view);
        self.sticker_marks(c);
        c.pop_clip();
        // while its model, its textures, its finish are still coming: three dots breathing where it will be
        let wait = (1.0 - 1.8 * crate::cosmetics::preview::seen()).max(0.0) * super::smooth(0.25, 0.45, self.now - look.at);
        if wait > 0.01 && pal().anim {
            let m = [(stand[0] + stand[2]) / 2.0, (stand[1] + stand[3]) / 2.0];
            self.cosm_dots(c, m, wait);
            // what has come of it from the server
            if let Some((got, all)) = crate::cosmetics::remote::progress().filter(|p| p.1 > 0) {
                let mb = |b: u64| format!("{:.1}", b as f64 / 1_048_576.0);
                let line = super::i18n::trn("{got} of {all} MB", &[("got", mb(got.min(all))), ("all", mb(all))]);
                c.text_c(m[0], m[1] + 1.6 * r, 0.62 * r, Face::Mono, &line, [pal().t3[0], pal().t3[1], pal().t3[2], pal().t3[3] * wait]);
            }
        }
        if let Some(p) = shown {
            self.cosm.last_shown = Some(p.clone());
            self.cosm.shown = Some(p);
        }
        // (its stickers shown: one is dragged on it)
        let charm_on = self.cosm.picking_charm && look.charm.is_some();
        let hint = if self.cosm.in_hand {
            self.hand_hint()
        } else if self.cosm.picking_sticker {
            tr("Drag stickers onto the item and along it; the wheel over one turns it, the right button takes it off").to_string()
        } else if charm_on || self.cosm.picking_charm {
            tr("Drag a charm onto the item and along it; past it to turn the item").to_string()
        } else {
            tr("Drag to turn it, the wheel to come closer, the right button to turn its light").to_string()
        };
        let hint = hint.as_str();
        // on a dark strip of its own (the world's blur may be bright), crossing over as it changes
        let hw = fonts().width(Face::Regular, 0.62 * r, hint);
        let hw = self.an(ak(A_HOV, 36_610), hw, 14.0, hw);
        let strip = [view[0] + 0.6 * r, view[3] - 1.75 * r, view[0] + 1.4 * r + hw, view[3] - 0.45 * r];
        c.rect(strip, 0.6 * r, grey(0.0, 0.35));
        c.push_clip(strip);
        self.cosm_words(c, 36_610, view[0] + 1.0 * r, view[3] - 0.9 * r, 0.62 * r, Face::Regular, hint, pal().t3, false);
        c.pop_clip();
        // the panel
        let px = view[2] + 0.8 * r;
        let pr = [px, view[1], body[2], view[3]];
        c.rect(pr, r, grey(1.0, 0.04));
        let x0 = pr[0] + 1.1 * r;
        let x1 = pr[2] - 1.1 * r;
        let mut y = pr[1] + 2.0 * r;
        let item_name = it.as_ref().map_or(look.item.clone(), |i| name_of(i).to_string());
        let fname = f.as_ref().map_or(tr("Vanilla").to_string(), |f| catalog::finish_name(f, ru()));
        // its code to copy, one to paste: at the panel's top right, the name short of them
        let codes = self.code_buttons(c, [x1, pr[1] + 0.75 * r]);
        c.text(x0, y, 1.15 * r, Face::Bold, &fonts().fit(Face::Bold, 1.15 * r, &fname, x1 - x0 - codes - 0.3 * r), pal().t1);
        y += 1.2 * r;
        // the item, and its rarity by its strip
        let kind = it.as_ref().map_or(Kind::Knife, |i| i.kind);
        let rarity = match (&it, &f) {
            (Some(i), Some(f)) => catalog::rarity_on(i, f),
            _ => "",
        };
        let w = c.text(x0, y, 0.78 * r, Face::Medium, &fonts().fit(Face::Medium, 0.78 * r, &item_name, x1 - x0), pal().t3);
        if f.is_some() {
            // the rarity after the item when it fits there, else under it
            let name = tr(catalog::rarity_name(kind, rarity));
            if x0 + w + 0.6 * r + fonts().width(Face::Regular, 0.7 * r, name) <= x1 {
                c.text(x0 + w + 0.6 * r, y, 0.7 * r, Face::Regular, name, pal().t4);
            } else {
                y += 1.0 * r;
                c.text(x0, y, 0.7 * r, Face::Regular, &fonts().fit(Face::Regular, 0.7 * r, name, x1 - x0), pal().t4);
            }
        }
        y += 0.7 * r;
        c.rect([x0, y, x0 + 3.0 * r, y + 0.2 * r], 0.1 * r, hex(catalog::rarity_colour(kind, rarity), 0.9));
        y += 1.6 * r;
        // a rifle's charm, a rifle's or a knife's stickers: their rows, and the charms or the stickers in the panel when
        // one is pressed (in place of the rest); what the panel shows crossing over as it changes
        let rifle = kind == Kind::Rifle;
        let stickers = kind != Kind::Gloves;
        let mode = if rifle && self.cosm.picking_charm {
            1
        } else if stickers && self.cosm.picking_sticker {
            2
        } else if self.cosm.picking_look {
            3
        } else {
            0
        };
        if mode != self.cosm.panel.0 {
            self.cosm.panel = (mode, self.now);
        }
        let start = c.p.len() / super::draw::FLOATS;
        let outer = c.clip_now();
        self.look_panel_body(c, &look, mode, [x0, y, x1], pr, kind, &f, &lo);
        let pk = if pal().anim { super::ease_out((self.now - self.cosm.panel.1) / 0.22) } else { 1.0 };
        if pk < 1.0 {
            let d = [0.0, (1.0 - pk) * 0.5 * r];
            c.fade_from(start, pk);
            c.shift(start, d);
            c.shift_own_clips(start, outer, d);
        }
        // a sticker or a charm carried from the panel, under the pointer
        self.carry_ghost(c);
    }

    /// The item's turn: the shown one following the hand's on a spring (a touch behind it, as a thing with weight), the
    /// hand's running on after a fling and slowing (a hand on it again stops it) — tilted past its bound, back to it as
    /// an elastic would; its nearness easing to the wheel's.
    fn look_motion(&mut self) {
        let dt = self.dt.clamp(1e-3, 0.05);
        let anim = pal().anim;
        let drag = self.cosm.drag.map(|(w, _)| w);
        let in_hand = self.cosm.in_hand;
        let Some(l) = self.cosm.look.as_mut() else { return };
        match drag {
            _ if in_hand => l.spin = [0.0; 2],
            Some(1) => {
                let v = [(l.aim[0] - l.was[0]) / dt, (l.aim[1] - l.was[1]) / dt];
                l.spin = [l.spin[0] + (v[0] - l.spin[0]) * 0.35, l.spin[1] + (v[1] - l.spin[1]) * 0.35];
            }
            Some(_) => l.spin = [0.0; 2],
            None if anim => {
                l.aim[0] += l.spin[0] * dt;
                l.aim[1] = (l.aim[1] + l.spin[1] * dt).clamp(-TILT_MAX, TILT_MAX);
                let k = (-dt * 2.4).exp();
                l.spin = [l.spin[0] * k, l.spin[1] * k];
                // past its bound: none of the fling that way, back to the bound
                if l.aim[1].abs() > TILT {
                    l.spin[1] = 0.0;
                    l.aim[1] += (TILT.copysign(l.aim[1]) - l.aim[1]) * (1.0 - (-dt * 8.0).exp());
                }
                if l.spin[0].abs() + l.spin[1].abs() < 0.01 {
                    l.spin = [0.0; 2];
                }
            }
            None => {
                l.spin = [0.0; 2];
                l.aim[1] = l.aim[1].clamp(-TILT, TILT);
            }
        }
        l.was = l.aim;
        if anim {
            // (in small steps: the spring as steady whatever a frame's length)
            let steps = (dt * 240.0).ceil().max(1.0);
            let h = dt / steps;
            let mut shown = [l.yaw, l.pitch];
            for _ in 0..steps as usize {
                for i in 0..2 {
                    let a = SPRING.0 * (l.aim[i] - shown[i]) - SPRING.1 * l.vel[i];
                    l.vel[i] += a * h;
                    shown[i] += l.vel[i] * h;
                }
            }
            [l.yaw, l.pitch] = shown;
        } else {
            [l.yaw, l.pitch] = l.aim;
            l.vel = [0.0; 2];
        }
        l.zoom = if anim { l.zoom + (l.zoom_to - l.zoom) * (1.0 - (-dt * 9.0).exp()) } else { l.zoom_to };
    }

    /// The look's panel below its name: the charms, the stickers or our materials when one is open (`mode`), else its
    /// rows, its wear and its pattern, StatTrak's switch and the button.
    #[allow(clippy::too_many_arguments)]
    fn look_panel_body(&mut self, c: &mut Canvas, look: &Look, mode: u8, [x0, y, x1]: [f32; 3], pr: [f32; 4], kind: Kind, f: &Option<catalog::Finish>, lo: &loadout::Loadout) {
        let r = self.r;
        let mut y = y;
        let rifle = kind == Kind::Rifle;
        let stickers = kind != Kind::Gloves;
        if mode == 1 {
            let rb = [x0, y - 0.9 * r, x1, y + 1.0 * r];
            self.charm_row(c, rb, look.charm.as_deref());
            self.charm_picker(c, [pr[0], rb[3] + 0.5 * r, pr[2], pr[3]], look.charm.as_deref());
            return;
        }
        if mode == 2 {
            let rb = [x0, y - 0.9 * r, x1, y + 1.0 * r];
            self.sticker_row(c, rb, look.stickers.len());
            self.sticker_panel(c, [pr[0], rb[3] + 0.6 * r, pr[2], pr[3]], &look.stickers);
            return;
        }
        if mode == 3 {
            let rb = [x0, y - 0.9 * r, x1, y + 1.0 * r];
            self.look_row(c, rb, look.overlay);
            self.look_panel(c, [pr[0], rb[3] + 0.6 * r, pr[2], pr[3]], look.overlay);
            return;
        }
        // the rows over the button, one height each from the bottom: StatTrak's switch, our material, the name tag, the
        // stickers, the charm (a rifle's)
        let row = |k: f32| [x0, pr[3] - 3.9 * r - k * 1.8 * r - 1.55 * r, x1, pr[3] - 3.9 * r - k * 1.8 * r];
        let mut k = if kind != Kind::Gloves { 1.0 } else { 0.0 };
        self.look_row(c, row(k), look.overlay);
        k += 1.0;
        if stickers {
            self.tag_row(c, row(k), look.nametag.as_deref());
            self.sticker_row(c, row(k + 1.0), look.stickers.len());
            if rifle {
                self.charm_row(c, row(k + 2.0), look.charm.as_deref());
            }
        }
        if let Some(f) = &f {
            // the wear: within the finish's range
            c.text(x0, y, 0.72 * r, Face::Medium, tr("Wear"), pal().t3);
            c.text_r(x1, y, 0.72 * r, Face::Mono, &format!("{:.4}", look.wear), pal().t2);
            y += 0.7 * r;
            let t = (look.wear - f.wear.0) / (f.wear.1 - f.wear.0).max(1e-6);
            self.cosm.wear_track = [x0, x1];
            self.cosm_slider(c, [x0, y, x1, y + 1.6 * r], t, Hit::Cosm(WEAR, 0), 31_400);
            // CS's name for it under the track
            c.text(x0, y + 2.25 * r, 0.66 * r, Face::Regular, tr(wear_name(look.wear)), pal().t4);
            y += 3.6 * r;
            // the pattern: its seed (0..1000)
            c.text(x0, y, 0.72 * r, Face::Medium, tr("Pattern"), pal().t3);
            c.text_r(x1, y, 0.72 * r, Face::Mono, &format!("#{}", look.seed), pal().t2);
            y += 0.7 * r;
            let bw = 1.6 * r;
            self.cosm.seed_track = [x0 + bw + 0.5 * r, x1 - bw - 0.5 * r];
            for (part, bx, s) in [(SEED_DOWN, x0, "−"), (SEED_UP, x1 - bw, "+")] {
                let hit = Hit::Cosm(part, 0);
                let hv = self.an(ak(A_HOV, 31_500 + part as usize), if self.hover == hit { 1.0 } else { 0.0 }, 14.0, 0.0);
                let b = [bx, y, bx + bw, y + 1.6 * r];
                c.rect(b, 0.5 * r, mix(pal().f1, pal().f3, hv));
                c.text_c((b[0] + b[2]) / 2.0, y + 1.12 * r, 0.9 * r, Face::Medium, s, mix(pal().t2, pal().t1, hv));
                self.hits.push((b, hit));
            }
            let st = self.cosm.seed_track;
            self.cosm_slider(c, [st[0], y, st[1], y + 1.6 * r], look.seed as f32 / 1000.0, Hit::Cosm(SEED, 0), 31_600);
        }
        let slot = Slot::of(&look.item, kind);
        let on = lo.get(slot).is_some_and(|e| e.item == look.item && e.paint == look.paint);
        // StatTrak™ (not on gloves): its switch, the kills it has counted when it is on (the lowest row)
        if kind != Kind::Gloves {
            let b = [x0, pr[3] - 3.9 * r - 1.55 * r, x1, pr[3] - 3.9 * r];
            let y = (b[1] + b[3]) * 0.5 - 0.3 * r;
            let hit = Hit::Cosm(STATTRAK, 0);
            let hv = self.an(ak(A_HOV, 31_800), if self.hover == hit { 1.0 } else { 0.0 }, 14.0, 0.0);
            let kv = self.an(ak(A_HOV, 31_801), if look.stattrak { 1.0 } else { 0.0 }, 14.0, look.stattrak as i32 as f32);
            c.text(x0, y + 0.55 * r, 0.76 * r, Face::Semi, "StatTrak™", mix(pal().t2, pal().t1, hv.max(kv)));
            let kills = lo.get(slot).filter(|e| on && e.stattrak.is_some()).and_then(|e| e.stattrak);
            if let Some(n) = kills {
                let w = fonts().width(Face::Semi, 0.76 * r, "StatTrak™");
                c.text(x0 + w + 0.6 * r, y + 0.55 * r, 0.66 * r, Face::Mono, &format!("{n}"), pal().t3);
            }
            let s = [x1 - 2.2 * r, y - 0.2 * r, x1, y + 0.9 * r];
            c.rect(s, 0.55 * r, mix(pal().f2, grey(1.0, 0.85), kv));
            let kx = s[0] + 0.55 * r + kv * (s[2] - s[0] - 1.1 * r);
            c.rect([kx - 0.42 * r, s[1] + 0.13 * r, kx + 0.42 * r, s[3] - 0.13 * r], 0.42 * r, mix(pal().t2, grey(0.08, 1.0), kv));
            self.hits.push((b, hit));
        }
        // on with it (or what is on)
        let hit = Hit::Cosm(EQUIP, 0);
        let hv = self.an(ak(A_HOV, 31_700), if self.hover == hit { 1.0 } else { 0.0 }, 14.0, 0.0);
        // what is on comes off by the same button (the hands' own then, or the side's own gloves)
        let label = if on && hv > 0.5 { tr("Take off") } else if on { tr("Equipped") } else { tr("Put on") };
        let b = [x0, pr[3] - 3.4 * r, x1, pr[3] - 1.1 * r];
        // its light going as it is put on (and back), its words crossing over
        let ov = self.an(ak(A_HOV, 31_702), if on { 1.0 } else { 0.0 }, 12.0, on as i32 as f32);
        c.rect(b, 0.8 * r, mix(mix(grey(0.92, 1.0), grey(1.0, 1.0), hv), grey(1.0, 0.12 + 0.05 * hv), ov));
        c.push_clip(b);
        self.cosm_words(c, 31_702, (b[0] + b[2]) / 2.0, b[1] + 1.48 * r, 0.86 * r, Face::Semi, label, mix(grey(0.05, 1.0), pal().t1, ov), true);
        c.pop_clip();
        self.hits.push((b, hit));
    }

    /// A slider: its track, its fill to `t` (0..1), its knob.
    pub(super) fn cosm_slider(&mut self, c: &mut Canvas, b: [f32; 4], t: f32, hit: Hit, anim: usize) {
        let r = self.r;
        let t = t.clamp(0.0, 1.0);
        let hv = self.an(ak(A_HOV, anim), if self.hover == hit || self.cosm.drag.is_some_and(|(p, _)| Hit::Cosm(p, 0) == hit) { 1.0 } else { 0.0 }, 14.0, 0.0);
        let mid = (b[1] + b[3]) / 2.0;
        c.rect([b[0], mid - 0.14 * r, b[2], mid + 0.14 * r], 0.14 * r, pal().f2);
        let x = b[0] + (b[2] - b[0]) * t;
        c.rect([b[0], mid - 0.14 * r, x, mid + 0.14 * r], 0.14 * r, mix(pal().t3, pal().t1, hv));
        let kr = (0.42 + 0.1 * hv) * r;
        c.rect([x - kr, mid - kr, x + kr, mid + kr], kr, pal().t1);
        self.hits.push((b, hit));
    }

    /// Development (dev.rs "cosm"): an item's finishes opened, or the look at one of them.
    pub(super) fn cosm_dev(&mut self, item: &str, paint: Option<&str>) {
        let cat = catalog::get();
        if cat.item(item).is_none() {
            eprintln!("[gui dev] no item {item} among {}", cat.items.len());
            return;
        }
        self.cosm_swap(false);
        self.cosm.tab = 0;
        self.cosm.item = Some(item.to_string());
        self.cosm.look = None;
        self.cosm.scroll = 0.0;
        self.cosm.scroll_to = 0.0;
        if let Some(p) = paint {
            let first = cat.item(item).is_some_and(|i| has_vanilla(i.kind)) as usize;
            let n = if p == "-" { Some(0) } else { cat.finishes_of(item).position(|f| f.paint == p).map(|i| i + first) };
            match n {
                Some(n) => self.cosm_press(FINISH, n as u16, 0),
                None => eprintln!("[gui dev] no finish {p} on {item}"),
            }
        }
    }

    /// The look opened at an item as the loadout keeps one (a code pasted): its finishes behind it.
    pub(super) fn cosm_open(&mut self, e: &Equip) {
        self.cosm_swap(false);
        self.cosm.tab = 0;
        self.cosm.item = Some(e.item.clone());
        self.cosm.scroll = 0.0;
        self.cosm.scroll_to = 0.0;
        self.cosm.picking_charm = false;
        self.cosm.picking_sticker = false;
        self.cosm.picking_look = false;
        self.cosm.sticker_typing = false;
        self.cosm.tag_typing = false;
        self.cosm.sticker_sel = 0;
        self.cosm.look = Some(Look {
            item: e.item.clone(),
            paint: e.paint.clone(),
            seed: e.seed,
            wear: e.wear,
            yaw: 0.0,
            pitch: 0.0,
            zoom: 1.0,
            at: self.now,
            zoom_to: 1.0,
            aim: [0.0; 2],
            spin: [0.0; 2],
            was: [0.0; 2],
            vel: [0.0; 2],
            hand_turn: [0.0; 2],
            stattrak: e.stattrak.is_some(),
            charm: e.charm.clone(),
            charm_offset: e.charm_offset.unwrap_or([0.0; 3]),
            stickers: e.stickers.clone(),
            nametag: e.nametag.clone(),
            overlay: e.overlay,
        });
    }

    /// Esc on the page: a level up — the field being typed in, the panel open in the look, the look, the item; false
    /// at the top (the menu closes).
    pub(super) fn cosm_escape(&mut self) -> bool {
        if self.cosm.sticker_typing || self.cosm.tag_typing {
            self.cosm.sticker_typing = false;
            self.cosm.tag_typing = false;
            return true;
        }
        if self.cosm.look.is_some() && (self.cosm.picking_charm || self.cosm.picking_sticker || self.cosm.picking_look) {
            self.cosm_press(BACK, 3, 0);
            return true;
        }
        if self.cosm.look.is_some() {
            self.cosm_press(BACK, 1, 0);
            return true;
        }
        if self.cosm.item.is_some() {
            self.cosm_press(BACK, 0, 0);
            return true;
        }
        false
    }

    /// The look at an item the frame just laid out shows (taken: the next frame says again).
    pub(super) fn cosm_shown(&mut self) -> Option<crate::cosmetics::preview::Preview> {
        self.cosm.shown.take()
    }

    /// A press on the page.
    pub(super) fn cosm_press(&mut self, part: u8, n: u16, button: i32) {
        // the right button on the item turns its light (in the hands, let go where it began: their heavy swing)
        if part == VIEW {
            self.cosm.press = Some((self.mouse, false));
        }
        if button == 1 && part == VIEW {
            // (on a sticker while they are shown: it comes off)
            if !self.sticker_take_off_at() {
                self.cosm.drag = Some((DRAG_LIGHT, self.mouse));
            }
            return;
        }
        if button != 0 {
            return;
        }
        let cat = catalog::get();
        match part {
            TAB => {
                if self.cosm.tab != n as u8 {
                    self.cosm_swap(false);
                }
                self.cosm.tab = n as u8;
                self.sound_tick(0.6);
            }
            CHIP => {
                if self.cosm.cat != n as u8 {
                    self.cosm_swap(true);
                }
                self.cosm.cat = n as u8;
                self.cosm.scroll_to = 0.0;
                self.sound_tick(0.5);
            }
            ITEM => {
                if let Some(it) = shown_items(&cat, self.cosm.cat).get(n as usize) {
                    self.cosm_swap(false);
                    self.cosm.item = Some(it.name.clone());
                    self.cosm.scroll_to = 0.0;
                    self.cosm.scroll = 0.0;
                    self.sound_tick(0.7);
                }
            }
            FINISH => {
                let Some(item) = self.cosm.item.clone() else { return };
                let kind = cat.item(&item).map_or(Kind::Knife, |i| i.kind);
                let first = has_vanilla(kind) as usize;
                let paint = if (n as usize) < first { String::new() } else { cat.finishes_of(&item).nth(n as usize - first).map(|f| f.paint.clone()).unwrap_or_default() };
                let f = cat.finish(&item, &paint);
                // as it is on, when it is; else the finish's least wear and its default seed
                let (lo, _) = loadout::current();
                let on = lo.get(Slot::of(&item, kind)).filter(|e| e.item == item && e.paint == paint).cloned();
                let (seed, wear) = on.map(|e| (e.seed, e.wear)).unwrap_or((f.map_or(0, |f| f.seed), f.map_or(0.0, |f| f.wear.0 + 0.01_f32.min((f.wear.1 - f.wear.0) * 0.5))));
                let stattrak = lo.get(Slot::of(&item, kind)).is_some_and(|e| e.item == item && e.paint == paint && e.stattrak.is_some());
                // the charm and the stickers the item on has (whatever its finish)
                let on_item = lo.get(Slot::of(&item, kind)).filter(|e| e.item == item);
                let charm = on_item.and_then(|e| e.charm.clone());
                let charm_offset = on_item.and_then(|e| e.charm_offset).unwrap_or([0.0; 3]);
                let stickers = on_item.map_or(Vec::new(), |e| e.stickers.clone());
                let nametag = on_item.and_then(|e| e.nametag.clone());
                let overlay = on_item.and_then(|e| e.overlay);
                self.cosm_swap(false);
                self.cosm.look = Some(Look { item, paint, seed, wear, yaw: 0.0, pitch: 0.0, zoom: 1.0, at: self.now, zoom_to: 1.0, aim: [0.0; 2], spin: [0.0; 2], was: [0.0; 2], vel: [0.0; 2], hand_turn: [0.0; 2], stattrak, charm, charm_offset, stickers, nametag, overlay });
                self.cosm.picking_look = false;
                self.cosm.picking_charm = false;
                self.cosm.picking_sticker = false;
                self.cosm.sticker_typing = false;
                self.cosm.tag_typing = false;
                self.cosm.sticker_sel = 0;
                self.sound_tick(0.8);
            }
            BACK => {
                // (the crumbs: 2 the grid of the category, 3 the look without its panel open)
                if n == 3 || (n == 1 && (self.cosm.picking_charm || self.cosm.picking_sticker || self.cosm.picking_look)) {
                    self.cosm.picking_charm = false;
                    self.cosm.picking_sticker = false;
                    self.cosm.picking_look = false;
                    self.cosm.sticker_typing = false;
                    self.cosm.tag_typing = false;
                    self.sound_tick(0.5);
                    return;
                }
                self.cosm_swap(false);
                if n == 2 {
                    // (the category the item is under)
                    if let Some(l) = self.cosm.look.as_ref() {
                        let kind = cat.item(&l.item).map_or(Kind::Knife, |i| i.kind);
                        self.cosm.cat = chip_of(kind, &l.item) as u8;
                    }
                    self.cosm.leaving = self.cosm.last_shown.take().map(|p| (p, self.now));
                    self.cosm.look = None;
                    self.cosm.item = None;
                    self.cosm.scroll_to = 0.0;
                    self.sound_tick(0.5);
                    return;
                }
                if n == 1 {
                    // the item goes away where it stood
                    self.cosm.leaving = self.cosm.last_shown.take().map(|p| (p, self.now));
                    self.cosm.look = None;
                    self.cosm.picking_charm = false;
                    self.cosm.picking_sticker = false;
                    self.cosm.sticker_typing = false;
                    self.cosm.tag_typing = false;
                    self.cosm.picking_look = false;
                } else {
                    self.cosm.item = None;
                    self.cosm.scroll_to = 0.0;
                }
                self.sound_tick(0.5);
            }
            VIEW => {
                // the chosen sticker (or the charm) taken where the pointer meets the item, else the item turned (in
                // the hands: the view)
                if self.cosm.in_hand || (!self.sticker_grab() && !self.charm_grab()) {
                    self.cosm.drag = Some((1, self.mouse));
                }
            }
            WEAR => {
                self.cosm.drag = Some((2, self.mouse));
                self.cosm_move(self.mouse[0], self.mouse[1]);
            }
            SEED => {
                self.cosm.drag = Some((3, self.mouse));
                self.cosm_move(self.mouse[0], self.mouse[1]);
            }
            SEED_DOWN | SEED_UP => {
                if let Some(l) = self.cosm.look.as_mut() {
                    l.seed = (l.seed + if part == SEED_UP { 1 } else { -1 }).clamp(0, 1000);
                }
                self.sound_tick(0.4);
            }
            EQUIP => {
                if let Some(l) = self.cosm.look.clone() {
                    let kind = cat.item(&l.item).map_or(Kind::Knife, |i| i.kind);
                    let slot = Slot::of(&l.item, kind);
                    let (lo, _) = loadout::current();
                    let on = lo.get(slot).is_some_and(|e| e.item == l.item && e.paint == l.paint);
                    let e = (!on).then(|| look_equip(&l, kind, lo.get(slot)));
                    // put on: CS's things on with it (off at first)
                    let wear = e.is_some();
                    loadout::change(|lo| {
                        lo.set(slot, e);
                        lo.on |= wear;
                    });
                    self.switch_sound(!on, 0.6);
                }
            }
            SIDE => {
                loadout::change(|lo| lo.arms = if lo.arms == "t" { "ct".into() } else { "t".into() });
                self.sound_tick(0.6);
            }
            13..=19 | super::cosmxhair::XPICK | super::cosmxhair::XBG | super::cosmxhair::XZOOM => self.xhair_press(part, n),
            super::cosmcase::CASE_PICK..=super::cosmcase::CASE_SKIP => self.case_press(part, n),
            super::cosmsets::SET_SAVE..=super::cosmsets::SET_NAME => self.sets_press(part, n),
            CHARM_ROW | CHARM_PICK => self.charm_press(part, n),
            STICKER_ROW..=STICKER_TURN | STICKER_OPPOSITE => self.sticker_press(part, n),
            LOOK_ROW..=LOOK_STRENGTH => self.look_press(part, n),
            CODE_COPY | CODE_PASTE => self.code_press(part),
            HAND => self.hand_press(n),
            super::cosmstage::QUALITY => self.quality_press(),
            INSPECT_KEY | HAND_SIDE | ALL_ON => self.keys_press(part),
            TAG_ROW => self.tag_press(),
            STATTRAK => {
                if let Some(l) = self.cosm.look.as_mut() {
                    l.stattrak = !l.stattrak;
                    let (item, paint, on) = (l.item.clone(), l.paint.clone(), l.stattrak);
                    // the one on keeps it at once (its count from 0 when it begins)
                    let kind = cat.item(&item).map_or(Kind::Knife, |i| i.kind);
                    let slot = Slot::of(&item, kind);
                    loadout::change(|lo| {
                        if let Some(e) = lo.get_mut(slot).filter(|e| e.item == item && e.paint == paint) {
                            e.stattrak = if on { e.stattrak.or(Some(0)) } else { None };
                        }
                    });
                    self.switch_sound(on, 0.5);
                }
            }
            _ => {}
        }
    }

    /// The pointer moved: what is held follows it.
    pub(super) fn cosm_move(&mut self, x: f32, y: f32) {
        if let Some((at, moved)) = self.cosm.press.as_mut() {
            *moved |= (x - at[0]).abs() + (y - at[1]).abs() > 5.0;
        }
        let Some((what, last)) = self.cosm.drag else { return };
        if what == super::cosmsticker::DRAG_CARRY {
            self.sticker_carry_move(x, y);
            return;
        }
        if what == super::cosmcharm::DRAG_CARRY_CHARM {
            self.charm_carry_move(x, y);
            return;
        }
        if what >= 10 {
            self.xhair_move(what - 10, x);
            return;
        }
        if matches!(what, DRAG_WEAR | DRAG_TURN | DRAG_MOVE) {
            self.sticker_move(x, y);
            return;
        }
        if what == super::cosmcharm::DRAG_CHARM {
            self.charm_place_at([x, y]);
            return;
        }
        if what == DRAG_STRENGTH {
            self.look_strength(x);
            return;
        }
        if what == DRAG_LIGHT {
            self.light_move(x, y, last);
            return;
        }
        let cat = catalog::get();
        let Some(l) = self.cosm.look.as_mut() else { return };
        match what {
            1 if self.cosm.in_hand => {
                if self.cosm.press.is_none_or(|(_, moved)| moved) {
                    l.hand_turn[0] += (x - last[0]) * 0.006;
                    l.hand_turn[1] = (l.hand_turn[1] + (y - last[1]) * 0.006).clamp(-1.4, 1.4);
                }
                self.cosm.drag = Some((1, [x, y]));
            }
            1 => {
                l.aim[0] += (x - last[0]) * TURN;
                // tilted past its bound: the harder the further (an elastic), never past the most
                let d = (y - last[1]) * TURN;
                let over = (l.aim[1].abs() - TILT).max(0.0);
                let d = if d * l.aim[1] > 0.0 { d / (1.0 + over * 10.0) } else { d };
                l.aim[1] = (l.aim[1] + d).clamp(-TILT_MAX, TILT_MAX);
                self.cosm.drag = Some((1, [x, y]));
            }
            2 => {
                if let Some(f) = cat.finish(&l.item, &l.paint) {
                    let [a, b] = self.cosm.wear_track;
                    let t = ((x - a) / (b - a).max(1.0)).clamp(0.0, 1.0);
                    l.wear = ((f.wear.0 + (f.wear.1 - f.wear.0) * t) * 10_000.0).round() / 10_000.0;
                }
            }
            3 => {
                let [a, b] = self.cosm.seed_track;
                let t = ((x - a) / (b - a).max(1.0)).clamp(0.0, 1.0);
                l.seed = (t * 1000.0).round() as i32;
            }
            _ => {}
        }
    }

    pub(super) fn cosm_release(&mut self) {
        let held = self.cosm.drag.take();
        match held {
            Some((super::cosmsticker::DRAG_CARRY, _)) => self.sticker_carry_drop(),
            Some((super::cosmcharm::DRAG_CARRY_CHARM, _)) => self.charm_carry_drop(),
            _ => {}
        }
        // in the hands, a press let go where it began: the left button's swing (or shot), the right one's heavy swing
        if let (Some((_, false)), true) = (self.cosm.press.take(), self.cosm.in_hand && self.cosm.look.is_some()) {
            match held {
                Some((1, _)) => self.hand_click(0),
                Some((DRAG_LIGHT, _)) => self.hand_click(1),
                _ => {}
            }
        }
    }

    /// The wheel on the page: nearer to the item in the look, else the cards scroll. True when it was the page's.
    pub(super) fn cosm_wheel(&mut self, dy: f32) -> bool {
        if self.sticker_wheel(dy) || self.sticker_spin(dy) || self.case_wheel(dy) || self.sets_wheel(dy) {
            return true;
        }
        // the charms scroll under the pointer
        let [x, y] = self.mouse;
        let a = self.cosm.charm_area;
        if self.cosm.look.is_some() && self.cosm.picking_charm && x >= a[0] && x <= a[2] && y >= a[1] && y <= a[3] {
            self.cosm.charm_scroll_to = (self.cosm.charm_scroll_to - dy * 3.2 * self.r).clamp(0.0, self.cosm.charm_content);
            return true;
        }
        if let Some(l) = self.cosm.look.as_mut() {
            // (in the hands the view is the game's: no nearer)
            if !self.cosm.in_hand {
                l.zoom_to = (l.zoom_to * (1.0 + dy * 0.12)).clamp(0.6, 2.6);
            }
            return true;
        }
        self.cosm.scroll_to = (self.cosm.scroll_to - dy * 3.2 * self.r).clamp(0.0, self.cosm.content);
        true
    }
}
