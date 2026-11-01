//! Custom Inventory (Interface → Custom Inventory): the survival inventory and the containers as blocks of the client's
//! glass, in the game's own arrangement and compact in the middle (her choice, 15.10): the player, the armour and the
//! other hand, the crafting, the backpack, the hotbar, the effects beside them; a container's own block over the
//! backpack (its slots as the game lays them, in the client's cells). Each cell a dark rounded square, lighter under
//! the pointer, the hotbar's chosen one ringed; the counts in Geist Mono, the wear a thin line; the item's tooltip a
//! card of the HUD's (the game's lines in their colours).
//!
//! Java's HudPack sends what the game knows (PackF) and draws the items, the player, the effects' icons, the carried
//! stack and a pack's glyphs where the frame's `out` says; the game's menu does the clicks (the slot under the pointer
//! is the one under it here).

use super::hudinv::lamp;
use super::hudinvui::header;
use super::hudtext::RText;
use super::*;
use std::sync::Mutex;

const A_PK: u64 = 50;

/// What a slot is to the layout (Java's HudPack.role).
pub mod role {
    #[allow(dead_code)]
    pub const HIDDEN: u8 = 0;
    pub const BACKPACK: u8 = 1;
    pub const HOTBAR: u8 = 2;
    pub const ARMOUR: u8 = 3;
    pub const OFFHAND: u8 = 4;
    pub const CRAFT: u8 = 5;
    pub const CRAFTED: u8 = 6;
    pub const CONTAINER: u8 = 7;
    pub const OUTPUT: u8 = 8;
}

/// A slot of the menu as Java has it.
#[derive(Clone, Default)]
pub struct PSlot {
    pub role: u8,
    /// backpack, hotbar, crafting: its column and row; armour: 0 head … 3 feet; a container's: the game's x and y
    pub a: f32,
    pub b: f32,
    pub count: u32,
    pub dmg: f32,
    pub max: f32,
    /// what shows when a stack is spread over it by dragging (-1: as it is)
    pub preview: i32,
    pub drag: bool,
    pub item: bool,
    /// the item's cooldown left (0..1)
    pub cool: f32,
    /// 1: switched off (a crafter's slot); 2: a recipe's missing ingredient shown faint (the slot is empty)
    pub flags: u32,
}

/// What a container shows besides its slots, where the game draws it (its px): its fire, its arrows, a field.
pub mod deco {
    /// the fire's light left (v)
    pub const FIRE: u8 = 1;
    /// the arrow to the result (v the progress, below 0 none)
    pub const ARROW: u8 = 2;
    /// a brewing's progress, downwards (v)
    pub const BREW: u8 = 3;
    /// a fuel's bar (v)
    pub const FUEL: u8 = 4;
    /// a brewing's bubbles (v the moment)
    pub const BUBBLES: u8 = 5;
    /// the recipe can't be (over the arrow)
    pub const ERROR: u8 = 6;
    /// a text field (v the caret, w the selection's other end; its words in the text)
    pub const FIELD: u8 = 7;
    /// a line of words at its right edge (v 0 quiet, 1 can, 2 can't; its words in the text)
    pub const LABEL: u8 = 8;
    /// a living thing shown (Java draws it)
    pub const ENTITY: u8 = 9;
    /// powered (v)
    pub const POWER: u8 = 10;
    /// a plus between two inputs
    pub const PLUS: u8 = 11;
    /// a map's well (Java draws the map in it)
    pub const MAP: u8 = 12;
    /// an enchantment to choose (v its levels, w 1 can, z how much lapis; its clue in the text)
    pub const OPTION: u8 = 13;
    /// the enchanting table's book (Java draws it)
    pub const BOOK: u8 = 14;
    /// a grid of choices 4 wide (v the first extra item, w how many, z the chosen one's place or -1)
    pub const GRID: u8 = 15;
    /// a scroll bar (v where, w 1 when it can)
    pub const SCROLL: u8 = 16;
    /// a trader's offer (v its three extra items: cost, second cost, result; w 1 sold out, 2 chosen, 4 cheaper; z the
    /// first cost's count before the discount)
    pub const TRADE: u8 = 17;
    /// a trader's experience (v now, w with the chosen offer)
    pub const XPBAR: u8 = 18;
    /// a loom's patterns 4 by 4 (v the first, w how many, z the chosen one's place or -1)
    pub const PATTERNS: u8 = 19;
    /// a banner as it will come out (Java draws it)
    pub const BANNER: u8 = 20;
    /// one of the screen's own buttons (v its effect's picture or -1; w 1 can, 2 chosen, 4 done, 8 cancel)
    pub const WIDGET: u8 = 21;
    /// an extra item where the game shows one (v which)
    pub const ITEM: u8 = 22;
}

#[derive(Clone, Default)]
pub struct Deco {
    pub kind: u8,
    pub r: [f32; 4],
    pub v: f32,
    pub w: f32,
    pub z: f32,
    pub text: RText,
}

/// What Java sends each frame a container is open.
#[derive(Clone, Default)]
pub struct PackF {
    pub gs: f32,
    pub since: f32,
    pub mouse: [f32; 2],
    pub sel: usize,
    pub carried: u32,
    /// 0 the survival inventory, 1 a container
    pub kind: u8,
    pub slots: Vec<PSlot>,
    /// name, ticks left (-1 for ever), level, ticks when first seen
    pub effects: Vec<(String, f32, u32, f32)>,
    pub title: RText,
    /// the hovered item's tooltip, the game's lines (none: no tooltip of ours)
    pub tip: Vec<RText>,
    pub decos: Vec<Deco>,
    /// the items Java draws besides the slots (a stonecutter's recipes, a trader's goods…): their counts
    pub extras: Vec<u32>,
    /// the mouse's buttons held (1 the left, 2 the right), pressed since the last frame (4 the left, 8 the right)
    pub buttons: u32,
    /// the blocks' editor asked for: 1 on (from the menu), 2 off (Esc)
    pub ask: u8,
    /// the screen's recipe book (the inventory's, a crafting table's, a furnace's), if it has one
    pub rec: Option<Rec>,
}

/// The recipe book as Java has it: open, only what can be made, its tabs (the chosen one), its recipes (can be made
/// now, more than one way), the wheel over it, the words searched and whether its field has the keys.
#[derive(Clone, Default)]
pub struct Rec {
    pub open: bool,
    pub filter: bool,
    pub tabs: Vec<bool>,
    pub items: Vec<(bool, bool)>,
    pub wheel: f32,
    pub q: String,
    pub focused: bool,
}

impl PackF {
    /// nums: [gs, since, mouse x, y, chosen hotbar slot, carried, kind, slots N, then 11 a slot (role, a, b, count,
    /// wear, its most, preview, in the drag, an item, cooldown, 0), effects M then 3 each (ticks, level, first), tip
    /// lines T]; text: the title's runs, the effects' names, the tip's lines' runs.
    pub fn parse(nums: &[f64], text: &str) -> PackF {
        let g = |i: usize| nums.get(i).copied().unwrap_or(0.0);
        let mut lines = text.split('\n');
        let mut f = PackF {
            gs: (g(0) as f32).max(1.0),
            since: g(1) as f32,
            mouse: [g(2) as f32, g(3) as f32],
            sel: (g(4).max(0.0) as usize).min(8),
            carried: g(5).max(0.0) as u32,
            kind: g(6) as u8,
            ..Default::default()
        };
        f.title = RText::parse(90, lines.next().unwrap_or(""));
        let n = (g(7).max(0.0) as usize).min(160);
        let mut i = 8;
        for _ in 0..n {
            f.slots.push(PSlot {
                role: g(i) as u8,
                a: g(i + 1) as f32,
                b: g(i + 2) as f32,
                count: g(i + 3).max(0.0) as u32,
                dmg: g(i + 4) as f32,
                max: g(i + 5) as f32,
                preview: g(i + 6) as i32,
                drag: g(i + 7) > 0.5,
                item: g(i + 8) > 0.5,
                cool: (g(i + 9) as f32).clamp(0.0, 1.0),
                flags: g(i + 10).max(0.0) as u32,
            });
            i += 11;
        }
        let m = (g(i).max(0.0) as usize).min(16);
        i += 1;
        for _ in 0..m {
            let name = lines.next().unwrap_or("").to_string();
            f.effects.push((name, g(i) as f32, g(i + 1).max(0.0) as u32, g(i + 2) as f32));
            i += 3;
        }
        let tn = (g(i).max(0.0) as usize).min(40);
        i += 1;
        for k in 0..tn {
            f.tip.push(RText::parse(100 + k as u32, lines.next().unwrap_or("")));
        }
        let dn = (g(i).max(0.0) as usize).min(16);
        i += 1;
        for k in 0..dn {
            let kind = g(i) as u8;
            let text = if matches!(kind, deco::FIELD | deco::LABEL | deco::OPTION) { RText::parse(140 + k as u32, lines.next().unwrap_or("")) } else { RText::default() };
            f.decos.push(Deco { kind, r: [g(i + 1) as f32, g(i + 2) as f32, g(i + 3) as f32, g(i + 4) as f32], v: g(i + 5) as f32, w: g(i + 6) as f32, z: g(i + 7) as f32, text });
            i += 8;
        }
        let en = (g(i).max(0.0) as usize).min(256);
        i += 1;
        for k in 0..en {
            f.extras.push(g(i + k).max(0.0) as u32);
        }
        i += en;
        f.buttons = g(i).max(0.0) as u32;
        f.ask = g(i + 1).max(0.0) as u8;
        i += 2;
        if g(i) > 0.5 {
            let mut r = Rec { open: g(i + 1) > 0.5, filter: g(i + 2) > 0.5, ..Default::default() };
            i += 3;
            let tn = (g(i).max(0.0) as usize).min(16);
            i += 1;
            for k in 0..tn {
                r.tabs.push(g(i + k) > 0.5);
            }
            i += tn;
            let kn = (g(i).max(0.0) as usize).min(2048);
            i += 1;
            for k in 0..kn {
                r.items.push((g(i + 2 * k) > 0.5, g(i + 2 * k + 1) > 0.5));
            }
            i += 2 * kn;
            r.wheel = g(i) as f32;
            if let Some(l) = lines.next() {
                let mut p = l.split('\t');
                if p.next() == Some("rq") {
                    r.q = p.next().unwrap_or("").to_string();
                    r.focused = p.next() == Some("1");
                }
            }
            f.rec = Some(r);
        }
        f
    }
}

/// A block of the layout: its place, the slots in it, what it holds.
#[derive(Clone, Copy, PartialEq)]
enum Block {
    Player,
    Armour,
    Crafting,
    Container,
    Backpack,
    Hotbar,
    Recipes,
}

impl Block {
    /// Its place's name among the HUD's (the editor's offset from the game's arrangement, saved with the HUD's).
    fn id(self) -> &'static str {
        match self {
            Block::Player => "inv-player",
            Block::Armour => "inv-armour",
            Block::Crafting => "inv-crafting",
            Block::Container => "inv-container",
            Block::Backpack => "inv-backpack",
            Block::Hotbar => "inv-hotbar",
            Block::Recipes => "inv-recipes",
        }
    }
}

/// The blocks the editor moves (their offsets from the game's arrangement, design px, kept with the HUD's places).
pub const INV_BLOCKS: [&str; 8] = ["inv-player", "inv-armour", "inv-crafting", "inv-container", "inv-backpack", "inv-hotbar", "inv-effects", "inv-recipes"];

/// The recipe book's grid scrolled (where it goes, where it is), for which tab and search.
struct RecScroll {
    target: f32,
    pos: f32,
    key: u64,
}

static REC_SCROLL: Mutex<RecScroll> = Mutex::new(RecScroll { target: 0.0, pos: 0.0, key: 0 });

/// What the frame tells Java, in window px.
#[derive(Default)]
struct Out {
    /// each slot drawn: its index, the item's box (x, y, size), seen, the box it is hovered in
    slots: Vec<[f32; 10]>,
    /// the player: its box (none: zeros)
    model: [f32; 5],
    /// each effect's icon: which, x, y, size, seen
    effects: Vec<[f32; 5]>,
    /// the blocks (a click outside all of them lets a stack go)
    blocks: Vec<[f32; 4]>,
    /// a pack's glyphs over the tooltip (HudText's icons)
    glyphs: Vec<[f32; 7]>,
    /// where a decoration that is Java's is (a text field it moves its box to, a living thing it draws): kind, box
    decor: Vec<[f32; 5]>,
    /// the container's block as the game's own px: where the game's (cx0, cy0) is, px a game's px, (cx0, cy0), the
    /// block (a click in it goes to the game in its own px)
    frame: [f32; 9],
    /// an extra item to draw: 0, which, x, y, size
    pics: Vec<[f32; 5]>,
    /// the carried stack's size
    carried: f32,
    /// whose the clicks are (hudinvui Eats: every button's, the left one's, the right one's; the settings open; the keys)
    edit: [f32; 5],
    /// the recipe book: [there is one, its button (4), open, its block (4), the search (4), the filter (4), the grid's
    /// box (4), tabs T then 4 each (x, y, side, chosen), recipes K then 9 each (which, x, y, side, seen, its box), the
    /// one pointed at (-1 none)]
    rec: Vec<f32>,
}

impl Out {
    fn flat(&self) -> Vec<f32> {
        let mut v = vec![2.0, self.slots.len() as f32];
        for s in &self.slots {
            v.extend_from_slice(s);
        }
        v.extend_from_slice(&self.model);
        v.push(self.effects.len() as f32);
        for e in &self.effects {
            v.extend_from_slice(e);
        }
        v.push(self.blocks.len() as f32);
        for b in &self.blocks {
            v.extend_from_slice(b);
        }
        v.push(self.glyphs.len() as f32);
        for g in &self.glyphs {
            v.extend_from_slice(g);
        }
        v.push(self.decor.len() as f32);
        for d in &self.decor {
            v.extend_from_slice(d);
        }
        v.extend_from_slice(&self.frame);
        v.push(self.pics.len() as f32);
        for p in &self.pics {
            v.extend_from_slice(p);
        }
        v.push(self.carried);
        v.extend_from_slice(&self.edit);
        if self.rec.is_empty() {
            v.push(0.0);
        } else {
            v.extend_from_slice(&self.rec);
        }
        v
    }
}

fn inside4(r: [f32; 4], p: [f32; 2]) -> bool {
    p[0] >= r[0] && p[0] < r[2] && p[1] >= r[1] && p[1] < r[3]
}

/// The wear's colour, as the game's (green to red), a breath softer.
fn wear_col(k: f32) -> Rgba {
    let k = k.clamp(0.0, 1.0);
    let h = k / 3.0;
    let x = (1.0 - ((h * 6.0) % 2.0 - 1.0).abs()) * 0.9;
    let (r, g) = if h * 6.0 < 1.0 { (0.9, x) } else { (x, 0.9) };
    [r, g, 0.18, 1.0]
}

impl Gui {
    /// The open container's frame: the picture under the items, the one over them, what Java draws where.
    pub fn pack_frame(&mut self, w: f32, h: f32, f: &PackF) -> (Canvas, Canvas, Vec<f32>) {
        // its own clock (the HUD may be hidden and its frame not made)
        self.ensure_loaded();
        let t = self.clock.elapsed().as_secs_f32();
        let menu_dt = self.dt;
        {
            static LAST: Mutex<f32> = Mutex::new(0.0);
            let mut last = LAST.lock().unwrap();
            self.dt = (t - *last).clamp(0.0, 0.1);
            *last = t;
        }
        self.now = t;
        let r = self.pack_draw(w, h, f);
        self.dt = menu_dt;
        r
    }

    fn pack_draw(&mut self, w: f32, h: f32, f: &PackF) -> (Canvas, Canvas, Vec<f32>) {
        let mi = self.mod_index("custom-inventory");
        let set = self.mods[mi].set.clone();
        let num = |l: &str| theme::num(&set, l);
        let on = |l: &str| theme::on(&set, l);
        let look = theme::HudLook::of(&self.theme, self.now);
        let u = (w / 1920.0).min(h / 1080.0).clamp(0.5, 3.0) * look.scale * (num("Scale") / 100.0).max(0.3);
        let cell = num("Cell size").max(20.0) * u;
        let gap = num("Cell gap") * u;
        let pitch = cell + gap;
        let pad = num("Inner space") * u;
        let bgap = num("Block gap") * u;
        let crad = (cell * 0.5 * num("Cell corners") / 100.0).min(cell / 2.0);
        let brad = look.radius * u * num("Corners") / 100.0;
        let cell_col = theme::or(theme::col(&set, "Cell colour"), grey(0.0, 0.32), self.now);
        let hover_col = theme::or(theme::col(&set, "Hover colour"), grey(1.0, 0.12), self.now);
        let ring_col = theme::or(theme::col(&set, "Chosen ring"), look.accent, self.now);
        let glass = theme::or(theme::col(&set, "Glass"), look.glass, self.now);
        let text = look.text;
        let text2 = look.text2;
        let titles = on("Titles");
        let mut c = Canvas::new(w, h);
        let mut top = Canvas::new(w, h);
        c.drain = self.hud.veil * (1.0 - theme::pal().around);
        top.drain = c.drain;
        let mut out = Out::default();
        let m = f.mouse;

        // ---- the blocks' sizes
        let grid = |cols: f32, rows: f32| [cols * pitch - gap + 2.0 * pad, rows * pitch - gap + 2.0 * pad];
        let backpack = grid(9.0, 3.0);
        let hotbar = grid(9.0, 1.0);
        let survival = f.kind == 0;
        let title_fs = 13.0 * u;
        let title_h = if titles { title_fs * 1.9 } else { 0.0 };
        // a container's slots as the game lays them (18 its px apart), in the client's cells
        let cont: Vec<usize> = f.slots.iter().enumerate().filter(|(_, s)| s.role == role::CONTAINER || s.role == role::OUTPUT).map(|(i, _)| i).collect();
        let (mut cx0, mut cy0) = (f32::MAX, f32::MAX);
        let boxed = |d: &Deco| d.r[2] > d.r[0];
        for &i in &cont {
            cx0 = cx0.min(f.slots[i].a);
            cy0 = cy0.min(f.slots[i].b);
        }
        for d in f.decos.iter().filter(|d| boxed(d)) {
            cx0 = cx0.min(d.r[0]);
            cy0 = cy0.min(d.r[1]);
        }
        // as far as its slots (a cell each) and its decorations reach
        let mut cont_size = [0.0f32, 0.0f32];
        for &i in &cont {
            let s = &f.slots[i];
            let big = if s.role == role::OUTPUT { cell * 1.18 } else { cell };
            cont_size[0] = cont_size[0].max((s.a - cx0) / 18.0 * pitch + (cell + big) / 2.0);
            cont_size[1] = cont_size[1].max((s.b - cy0) / 18.0 * pitch + (cell + big) / 2.0);
        }
        for d in f.decos.iter().filter(|d| boxed(d)) {
            cont_size[0] = cont_size[0].max((d.r[2] - cx0) / 18.0 * pitch);
            cont_size[1] = cont_size[1].max((d.r[3] - cy0) / 18.0 * pitch);
        }
        let armour = [cell + 2.0 * pad, 5.0 * pitch - gap + 0.35 * pitch + 2.0 * pad];
        let crafting = [3.0 * pitch + 0.9 * pitch - gap + 2.0 * pad, armour[1]];
        let width = backpack[0].max(if survival { armour[0] + crafting[0] + 2.0 * pitch + 2.0 * bgap } else { cont_size[0] + 2.0 * pad });
        let player = [width - armour[0] - crafting[0] - 2.0 * bgap, armour[1]];
        let container = [width, cont_size[1] + 2.0 * pad + title_h];
        let top_h = if survival { armour[1] } else { container[1] };
        let total_h = top_h + bgap + backpack[1] + bgap + hotbar[1];
        let rec_open = f.rec.as_ref().is_some_and(|r| r.open);
        let rec_w = 5.0 * pitch - gap + 2.0 * pad;
        let rec_room = if rec_open { rec_w + bgap } else { 0.0 };
        let x0 = ((w - width + rec_room) / 2.0).round();
        let y0 = ((h - total_h) / 2.0 - 10.0 * u).round();
        let mut blocks: Vec<(Block, [f32; 4])> = Vec::new();
        if survival {
            blocks.push((Block::Armour, [x0, y0, x0 + armour[0], y0 + armour[1]]));
            let px = x0 + armour[0] + bgap;
            blocks.push((Block::Player, [px, y0, px + player[0], y0 + player[1]]));
            let cx = x0 + width - crafting[0];
            blocks.push((Block::Crafting, [cx, y0, x0 + width, y0 + crafting[1]]));
        } else {
            blocks.push((Block::Container, [x0, y0, x0 + width, y0 + container[1]]));
        }
        let by = y0 + top_h + bgap;
        blocks.push((Block::Backpack, [x0, by, x0 + width, by + backpack[1]]));
        let hy = by + backpack[1] + bgap;
        blocks.push((Block::Hotbar, [x0, hy, x0 + width, hy + hotbar[1]]));
        if rec_open {
            blocks.push((Block::Recipes, [x0 - rec_room, y0, x0 - bgap, y0 + total_h]));
        }

        // ---- coming in: each block a little after the one before, rising into place
        let speed = look.speed;
        let seen = |i: usize| -> f32 { if !look.anim { 1.0 } else { ease_out(((f.since * speed - i as f32 * 0.035) / 0.22).clamp(0.0, 1.0)) } };
        let lift = |i: usize| (1.0 - seen(i)) * 10.0 * u;

        // ---- the player's places for the blocks (their offsets: each taken by its header, hudinvui), kept on the screen
        let keep = |r: [f32; 4], o: [f32; 2]| -> [f32; 4] {
            let mut d = [o[0] * u, o[1] * u];
            d[0] = d[0].clamp(4.0 - r[0], (w - 4.0 - r[2]).max(4.0 - r[0]));
            d[1] = d[1].clamp(4.0 - r[1], (h - 4.0 - r[3]).max(4.0 - r[1]));
            [r[0] + d[0], r[1] + d[1], r[2] + d[0], r[3] + d[1]]
        };
        let off_of = |g: &Gui, id: &str| g.hud.places.get(id).map_or([0.0, 0.0], |p| p.1);
        let base = blocks.clone();
        for (b, r) in blocks.iter_mut() {
            *r = keep(*r, off_of(self, b.id()));
        }
        let ew = 196.0 * u;
        let eh = 40.0 * u;
        let n_eff = if on("Effects") { f.effects.len() } else { 0 };
        let eff_base = [x0 - rec_room - bgap - ew, y0, x0 - rec_room - bgap, y0 + n_eff.max(1) as f32 * (eh + 6.0 * u) - 6.0 * u];
        let mut eff = keep(eff_base, off_of(self, "inv-effects"));
        // the corner's button (the recipe book's): by the top right corner of it all
        let pen = {
            let mut all = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
            for (_, r) in &blocks {
                all = [all[0].min(r[0]), all[1].min(r[1]), all[2].max(r[2]), all[3].max(r[3])];
            }
            let ps = 30.0 * u;
            let mut pen = [all[2] + 8.0 * u, all[1], all[2] + 8.0 * u + ps, all[1] + ps];
            if pen[2] > w - 4.0 {
                pen = [all[2] - ps, all[1] - 8.0 * u - ps, all[2], all[1] - 8.0 * u];
            }
            pen
        };
        // ---- the screen's own: the settings' panel takes its part of the buttons, then the headers theirs (a block
        // held follows the pointer, laid where it is this very frame)
        let (edges, took) = self.iu_begin(f.buttons, f.since, f.ask, m);
        let head_of = |b: Block, r: [f32; 4]| header(r, if b == Block::Container { title_h } else { 0.0 }, pad, u);
        let mut held_list: Vec<(&'static str, [f32; 4], [f32; 4], [f32; 4])> = blocks.iter().zip(base.iter()).map(|((b, r), (_, r0))| (b.id(), *r0, *r, head_of(*b, *r))).collect();
        if n_eff > 0 {
            held_list.push(("inv-effects", eff_base, eff, [eff[0], eff[1] - 16.0 * u, eff[2], eff[1] + 4.0 * u]));
        }
        if let Some((id, o)) = self.iu_blocks(edges, took, m, u, w, h, bgap, &held_list) {
            if id == "inv-effects" {
                eff = keep(eff_base, o);
            } else if let Some(k) = blocks.iter().position(|q| q.0.id() == id) {
                blocks[k].1 = keep(base[k].1, o);
            }
        }
        // (a block held: the slots under the pointer are not pointed at)
        let editing = self.hud.inv.drag.is_some();

        // ---- the blocks' glass
        for (bi, (_, r)) in blocks.iter().enumerate() {
            let r = [r[0], r[1] + lift(bi), r[2], r[3] + lift(bi)];
            c.alpha = seen(bi);
            c.glass(r, brad, glass, look.blur, if look.shadow { 0.55 } else { 0.0 }, 0.0);
            out.blocks.push(r);
        }
        c.alpha = 1.0;

        // ---- each slot's cell
        let mut hovered: Option<usize> = None;
        let mut cells: Vec<(usize, [f32; 4], usize)> = Vec::new();
        let place = |s: &PSlot| -> Option<(usize, f32, f32, f32)> {
            // its block, its cell's corner and size
            let at = |b: Block| blocks.iter().position(|q| q.0 == b);
            match s.role {
                role::BACKPACK => at(Block::Backpack).map(|bi| {
                    let r = blocks[bi].1;
                    let x = r[0] + (r[2] - r[0] - (9.0 * pitch - gap)) / 2.0;
                    (bi, x + s.a * pitch, r[1] + pad + s.b * pitch, cell)
                }),
                role::HOTBAR => at(Block::Hotbar).map(|bi| {
                    let r = blocks[bi].1;
                    let x = r[0] + (r[2] - r[0] - (9.0 * pitch - gap)) / 2.0;
                    (bi, x + s.a * pitch, r[1] + pad, cell)
                }),
                role::ARMOUR => at(Block::Armour).map(|bi| (bi, blocks[bi].1[0] + pad, blocks[bi].1[1] + pad + s.a * pitch, cell)),
                role::OFFHAND => at(Block::Armour).map(|bi| (bi, blocks[bi].1[0] + pad, blocks[bi].1[1] + pad + 4.35 * pitch, cell)),
                role::CRAFT => at(Block::Crafting).map(|bi| {
                    let r = blocks[bi].1;
                    let top = r[1] + (r[3] - r[1] - (2.0 * pitch - gap)) / 2.0;
                    (bi, r[0] + pad + s.a * pitch, top + s.b * pitch, cell)
                }),
                role::CRAFTED => at(Block::Crafting).map(|bi| {
                    let r = blocks[bi].1;
                    let big = cell * 1.18;
                    (bi, r[2] - pad - big, (r[1] + r[3]) / 2.0 - big / 2.0, big)
                }),
                role::CONTAINER | role::OUTPUT => at(Block::Container).map(|bi| {
                    let r = blocks[bi].1;
                    let left = r[0] + (r[2] - r[0] - cont_size[0]) / 2.0;
                    let big = if s.role == role::OUTPUT { cell * 1.18 } else { cell };
                    let (x, y) = (left + (s.a - cx0) / 18.0 * pitch, r[1] + pad + title_h + (s.b - cy0) / 18.0 * pitch);
                    (bi, x - (big - cell) / 2.0, y - (big - cell) / 2.0, big)
                }),
                _ => None,
            }
        };
        for (i, s) in f.slots.iter().enumerate() {
            let Some((bi, x, y, size)) = place(s) else { continue };
            let y = y + lift(bi);
            let r = [x, y, x + size, y + size];
            let hit = [x - gap / 2.0, y - gap / 2.0, x + size + gap / 2.0, y + size + gap / 2.0];
            if inside4(hit, m) && !editing {
                hovered = Some(i);
            }
            cells.push((i, r, bi));
            // the item in whole steps of its 16 pixels (the game's pictures stay sharp), about nine tenths of the cell
            let isz = ((size * 0.86) / 16.0).round().max(1.0) * 16.0;
            let o = (size - isz) / 2.0;
            out.slots.push([i as f32, x + o, y + o, isz, seen(bi), hit[0], hit[1], hit[2], hit[3], 1.0]);
        }
        for &(i, r, bi) in &cells {
            let s = &f.slots[i];
            c.alpha = seen(bi);
            let hv = self.an(ak(A_PK, 1000 + i), if hovered == Some(i) { 1.0 } else { 0.0 }, 18.0, 0.0);
            let rr = if r[2] - r[0] > cell + 0.5 { crad * 1.18 } else { crad };
            c.rect(r, rr, cell_col);
            if s.flags & 1 != 0 {
                let q = (r[2] - r[0]) * 0.28;
                c.rect(r, rr, grey(0.0, 0.3));
                c.line([r[0] + q, r[1] + q], [r[2] - q, r[3] - q], 1.6 * u, grey(1.0, 0.22), 0.0);
                c.line([r[2] - q, r[1] + q], [r[0] + q, r[3] - q], 1.6 * u, grey(1.0, 0.22), 0.0);
            }
            // the hotbar's chosen slot: lit from its foot
            if s.role == role::HOTBAR && s.a as usize == f.sel {
                lamp(&mut c, r, rr, ring_col);
            }
            if s.drag {
                c.rect(r, rr, grey(1.0, 0.1));
            }
            if hv > 0.01 {
                c.rect(r, rr, fade(hover_col, hv));
            }
            // what goes in an empty one: armour, the other hand
            if !s.item {
                let kind = match s.role {
                    role::ARMOUR => Some(50 + (s.a as u32).min(3)),
                    role::OFFHAND => Some(54),
                    _ => None,
                };
                if let Some(k) = kind {
                    let q = (r[2] - r[0]) * 0.27;
                    c.icon([r[0] + q, r[1] + q, r[2] - q, r[3] - q], k, grey(1.0, 0.16 + 0.1 * hv));
                }
            }
        }
        c.alpha = 1.0;
        // the crafting's arrow
        if let Some(&(_, r)) = blocks.iter().find(|b| b.0 == Block::Crafting) {
            let bi = blocks.iter().position(|b| b.0 == Block::Crafting).unwrap();
            c.alpha = seen(bi);
            let ax = r[0] + pad + 2.0 * pitch - gap + 0.12 * pitch;
            let s = 0.62 * pitch;
            let my = (r[1] + r[3]) / 2.0 + lift(bi);
            c.icon([ax, my - s / 2.0, ax + s, my + s / 2.0], 8, grey(1.0, 0.32));
            c.alpha = 1.0;
        }
        // the container's decorations: its fire, the arrow to its result, a brewing's progress, a field…
        if let Some(bi) = blocks.iter().position(|b| b.0 == Block::Container) {
            let r = blocks[bi].1;
            let l = lift(bi);
            let left = r[0] + (r[2] - r[0] - cont_size[0]) / 2.0;
            let ctop = r[1] + pad + title_h + l;
            let k = pitch / 18.0;
            let mapr = |q: [f32; 4]| [left + (q[0] - cx0) * k, ctop + (q[1] - cy0) * k, left + (q[2] - cx0) * k, ctop + (q[3] - cy0) * k];
            out.frame = [left, ctop, k, cx0, cy0, r[0], r[1] + l, r[2], r[3] + l];
            c.alpha = seen(bi);
            let dim = grey(1.0, 0.16);
            let lit = fade(text, 0.92);
            // the arrow: from the inputs nearest the result's left to it, at the result's middle
            let out_slot = cont.iter().map(|&i| &f.slots[i]).find(|s| s.role == role::OUTPUT);
            let arrow = out_slot.map(|o| {
                let from = cont.iter().map(|&i| &f.slots[i]).filter(|s| s.role == role::CONTAINER && s.a < o.a).map(|s| s.a).fold(f32::MIN, f32::max);
                let x0 = left + (from - cx0) * k + cell + 0.25 * pitch;
                let x1 = left + (o.a - cx0) * k - (cell * 0.09) - 0.25 * pitch;
                let y = ctop + (o.b - cy0) * k + cell / 2.0;
                [x0, y, x1.max(x0 + 0.4 * pitch), y]
            });
            for d in &f.decos {
                match d.kind {
                    deco::FIRE => {
                        let q = mapr(d.r);
                        let s = (q[2] - q[0]).min(q[3] - q[1]);
                        let ir = [(q[0] + q[2]) / 2.0 - s / 2.0, (q[1] + q[3]) / 2.0 - s / 2.0, (q[0] + q[2]) / 2.0 + s / 2.0, (q[1] + q[3]) / 2.0 + s / 2.0];
                        c.icon(ir, 56, dim);
                        if d.v > 0.0 {
                            c.push_clip([ir[0], ir[3] - (ir[3] - ir[1]) * d.v.clamp(0.0, 1.0), ir[2], ir[3]]);
                            c.icon(ir, 56, lit);
                            c.pop_clip();
                        }
                    }
                    deco::ARROW => {
                        if let Some(a) = arrow {
                            let w = (2.6 * u).max(1.5);
                            let head = 0.24 * pitch;
                            c.arrow([a[0], a[1]], [a[2], a[1]], w, head, dim);
                            if d.v >= 0.999 {
                                c.arrow([a[0], a[1]], [a[2], a[1]], w, head, lit);
                            } else if d.v > 0.0 {
                                let x = a[0] + (a[2] - a[0]) * d.v.clamp(0.0, 1.0);
                                c.line([a[0], a[1]], [x.min(a[2] - 1.0 * u), a[1]], w, lit, 0.0);
                            }
                        }
                    }
                    deco::ERROR => {
                        // where the game puts it, or over the arrow
                        let at = if d.r[2] > d.r[0] {
                            let q = mapr(d.r);
                            Some([(q[0] + q[2]) / 2.0, (q[1] + q[3]) / 2.0])
                        } else {
                            arrow.map(|a| [(a[0] + a[2]) / 2.0, a[1]])
                        };
                        if let Some(m) = at {
                            let s = 0.42 * pitch;
                            c.icon([m[0] - s / 2.0, m[1] - s / 2.0, m[0] + s / 2.0, m[1] + s / 2.0], 5, [0.95, 0.42, 0.42, 1.0]);
                        }
                    }
                    deco::BOOK => {
                        let q = mapr(d.r);
                        c.glow([(q[0] + q[2]) / 2.0, q[3] - 2.0 * u], (q[2] - q[0]) * 0.55, grey(1.0, 0.05));
                        out.decor.push([d.kind as f32, q[0], q[1], q[2], q[3]]);
                    }
                    deco::OPTION => {
                        let q = mapr(d.r);
                        let q = [q[0], q[1] + 1.5 * u, q[2], q[3] - 1.5 * u];
                        let can = d.w as i32 & 1 != 0;
                        let cost = d.v.max(0.0) as u32;
                        let hov = can && inside4(q, m);
                        let hv = self.an(ak(A_PK, 2000 + (d.z as usize)), if hov { 1.0 } else { 0.0 }, 18.0, 0.0);
                        c.rect(q, 8.0 * u, cell_col);
                        if hv > 0.01 {
                            c.rect(q, 8.0 * u, fade(hover_col, hv));
                        }
                        if cost > 0 {
                            let mid = (q[1] + q[3]) / 2.0;
                            // how much lapis: a small well with its number
                            let bs = (q[3] - q[1]) * 0.62;
                            let b = [q[0] + 6.0 * u, mid - bs / 2.0, q[0] + 6.0 * u + bs, mid + bs / 2.0];
                            c.rect(b, bs * 0.3, grey(1.0, if can { 0.1 } else { 0.05 }));
                            c.text_c((b[0] + b[2]) / 2.0, mid + 4.0 * u, 11.5 * u, Face::Mono, &format!("{}", d.z as i32 + 1), if can { text } else { text2 });
                            // the clue, then the levels at the right (green when they are there, red when not)
                            let num = format!("{cost}");
                            let nfs = 13.0 * u;
                            let nw = fonts().width(Face::Semi, nfs, &num);
                            c.text_r(q[2] - 8.0 * u, mid + nfs * 0.36, nfs, Face::Semi, &num, if can { [0.55, 0.92, 0.55, 1.0] } else { [0.95, 0.45, 0.45, 0.9] });
                            let tx = b[2] + 9.0 * u;
                            let room = (q[2] - 8.0 * u - nw - 8.0 * u - tx).max(10.0);
                            let fs = 12.0 * u;
                            c.push_clip([tx, q[1], tx + room, q[3]]);
                            let icons = std::mem::take(&mut self.hud.icons);
                            self.rtext(&mut c, &d.text, tx, mid + fs * 0.36, fs, Face::Regular, if can { text } else { fade(text2, 0.7) }, true);
                            out.glyphs.extend(std::mem::replace(&mut self.hud.icons, icons));
                            c.pop_clip();
                        }
                    }
                    deco::GRID => {
                        let n = d.w.max(0.0) as usize;
                        let first = d.v.max(0.0) as usize;
                        let chosen = d.z as i32;
                        for i in 0..n.min(16) {
                            let (col, row) = ((i % 4) as f32, (i / 4) as f32);
                            let vx = d.r[0] + col * 16.0;
                            let vy = d.r[1] + row * 18.0;
                            let q = mapr([vx, vy, vx + 16.0, vy + 18.0]);
                            let q = [q[0] + 1.0 * u, q[1] + 1.0 * u, q[2] - 1.0 * u, q[3] - 1.0 * u];
                            let hov = inside4(q, m);
                            let hv = self.an(ak(A_PK, 3000 + i), if hov { 1.0 } else { 0.0 }, 18.0, 0.0);
                            c.rect(q, crad * 0.8, cell_col);
                            if chosen == i as i32 {
                                c.rect(q, crad * 0.8, grey(1.0, 0.08));
                                lamp(&mut c, q, crad * 0.8, ring_col);
                            }
                            if hv > 0.01 {
                                c.rect(q, crad * 0.8, fade(hover_col, hv));
                            }
                            let side = (q[2] - q[0]).min(q[3] - q[1]);
                            let isz = ((side * 0.86) / 16.0).round().max(1.0) * 16.0;
                            let isz = if isz > side { (isz - 16.0).max(16.0) } else { isz };
                            out.pics.push([0.0, (first + i) as f32, (q[0] + q[2]) / 2.0 - isz / 2.0, (q[1] + q[3]) / 2.0 - isz / 2.0, isz]);
                        }
                    }
                    deco::TRADE => {
                        let q = mapr(d.r);
                        let q = [q[0], q[1] + 1.0 * u, q[2], q[3] - 1.0 * u];
                        let sold = d.w as i32 & 1 != 0;
                        let chosen = d.w as i32 & 2 != 0;
                        let hov = inside4(q, m);
                        let key = 4000 + (d.r[1] as usize);
                        let hv = self.an(ak(A_PK, key), if hov { 1.0 } else { 0.0 }, 18.0, 0.0);
                        c.rect(q, 8.0 * u, cell_col);
                        if chosen {
                            c.rect(q, 8.0 * u, grey(1.0, 0.06));
                            lamp(&mut c, q, 8.0 * u, ring_col);
                        }
                        if hv > 0.01 {
                            c.rect(q, 8.0 * u, fade(hover_col, hv));
                        }
                        let first = d.v.max(0.0) as usize;
                        let item_at = |dx: f32| mapr([d.r[0] + dx, d.r[1] + 2.0, d.r[0] + dx + 16.0, d.r[1] + 18.0]);
                        for (n, dx) in [(0usize, 5.0f32), (1, 35.0), (2, 68.0)] {
                            if f.extras.get(first + n).copied().unwrap_or(0) == 0 {
                                continue;
                            }
                            let r = item_at(dx);
                            let side = (r[2] - r[0]).min(r[3] - r[1]);
                            let isz = ((side * 0.95) / 16.0).round().max(1.0) * 16.0;
                            let isz = if isz > side * 1.05 { (isz - 16.0).max(16.0) } else { isz };
                            out.pics.push([0.0, (first + n) as f32, (r[0] + r[2]) / 2.0 - isz / 2.0, (r[1] + r[3]) / 2.0 - isz / 2.0, isz]);
                        }
                        // the arrow between, or a cross when it is sold out
                        let a = mapr([d.r[0] + 55.0, d.r[1] + 5.0, d.r[0] + 65.0, d.r[1] + 15.0]);
                        c.icon(a, if sold { 5 } else { 8 }, if sold { [0.95, 0.42, 0.42, 0.9] } else { grey(1.0, 0.3) });
                        // cheaper: the old count struck through over the first cost
                        if d.w as i32 & 4 != 0 && d.z > 0.0 {
                            let r = item_at(5.0);
                            let fs = 10.0 * u;
                            let label = format!("{}", d.z as i32);
                            let w = fonts().width(Face::Medium, fs, &label);
                            let x = r[0] - 1.0 * u;
                            let y = r[1] + fs * 0.8;
                            c.text(x, y, fs, Face::Medium, &label, fade(text2, 0.9));
                            c.rect([x - 1.0 * u, y - fs * 0.32, x + w + 1.0 * u, y - fs * 0.32 + 1.2 * u], 0.5, [0.95, 0.42, 0.42, 0.9]);
                        }
                    }
                    deco::XPBAR => {
                        let q = mapr(d.r);
                        let h = (q[3] - q[1]).max(4.0 * u);
                        let bar = [q[0], q[1], q[2], q[1] + h];
                        c.rect(bar, h / 2.0, grey(1.0, 0.1));
                        let wv = d.w.clamp(0.0, 1.0);
                        if wv > d.v {
                            c.rect([bar[0], bar[1], bar[0] + (bar[2] - bar[0]) * wv, bar[3]], h / 2.0, [0.55, 0.92, 0.55, 0.45]);
                        }
                        if d.v > 0.0 {
                            c.rect([bar[0], bar[1], bar[0] + (bar[2] - bar[0]) * d.v.clamp(0.0, 1.0), bar[3]], h / 2.0, [0.55, 0.92, 0.55, 1.0]);
                        }
                    }
                    deco::PATTERNS => {
                        let n = d.w.max(0.0) as usize;
                        let first = d.v.max(0.0) as usize;
                        let chosen = d.z as i32;
                        for i in 0..n.min(16) {
                            let (col, row) = ((i % 4) as f32, (i / 4) as f32);
                            let vx = d.r[0] + col * 14.0;
                            let vy = d.r[1] + row * 14.0;
                            let q = mapr([vx, vy, vx + 14.0, vy + 14.0]);
                            let q = [q[0] + 1.0 * u, q[1] + 1.0 * u, q[2] - 1.0 * u, q[3] - 1.0 * u];
                            let hv = self.an(ak(A_PK, 5000 + i), if inside4(q, m) { 1.0 } else { 0.0 }, 18.0, 0.0);
                            c.rect(q, crad * 0.7, cell_col);
                            if chosen == i as i32 {
                                c.rect(q, crad * 0.7, grey(1.0, 0.08));
                                lamp(&mut c, q, crad * 0.7, ring_col);
                            }
                            if hv > 0.01 {
                                c.rect(q, crad * 0.7, fade(hover_col, hv));
                            }
                            out.pics.push([1.0, (first + i) as f32, q[0], q[1], (q[2] - q[0]).min(q[3] - q[1])]);
                        }
                    }
                    deco::BANNER => {
                        let q = mapr(d.r);
                        c.rect([q[0] - 4.0 * u, q[1] - 4.0 * u, q[2] + 4.0 * u, q[3] + 4.0 * u], 8.0 * u, grey(0.0, 0.22));
                        out.decor.push([d.kind as f32, q[0], q[1], q[2], q[3]]);
                    }
                    deco::WIDGET => {
                        let q = mapr(d.r);
                        let q = [q[0] + 1.0 * u, q[1] + 1.0 * u, q[2] - 1.0 * u, q[3] - 1.0 * u];
                        let flags = d.w as i32;
                        let can = flags & 1 != 0;
                        let hov = can && inside4(q, m);
                        let hv = self.an(ak(A_PK, 6000 + (d.r[0] as usize) * 7 + d.r[1] as usize), if hov { 1.0 } else { 0.0 }, 18.0, 0.0);
                        let rr = crad.min((q[2] - q[0]) / 2.0);
                        c.rect(q, rr, cell_col);
                        if flags & 2 != 0 {
                            c.rect(q, rr, grey(1.0, 0.08));
                            lamp(&mut c, q, rr, ring_col);
                        }
                        if hv > 0.01 {
                            c.rect(q, rr, fade(hover_col, hv));
                        }
                        let s = (q[2] - q[0]).min(q[3] - q[1]);
                        let mid = [(q[0] + q[2]) / 2.0, (q[1] + q[3]) / 2.0];
                        if flags & 4 != 0 || flags & 8 != 0 {
                            let is = s * 0.42;
                            let col = if !can { grey(1.0, 0.2) } else if flags & 4 != 0 { [0.55, 0.92, 0.55, 1.0] } else { [0.95, 0.45, 0.45, 1.0] };
                            c.icon([mid[0] - is / 2.0, mid[1] - is / 2.0, mid[0] + is / 2.0, mid[1] + is / 2.0], if flags & 4 != 0 { 6 } else { 5 }, col);
                        } else if d.v >= 0.0 {
                            let isz = ((s * 0.8) / 18.0).round().max(1.0) * 18.0;
                            let isz = if isz > s { (isz - 18.0).max(18.0) } else { isz };
                            out.pics.push([2.0, d.v, mid[0] - isz / 2.0, mid[1] - isz / 2.0, isz]);
                            if !can {
                                top.rect(q, rr, grey(0.0, 0.55));
                            }
                        }
                    }
                    deco::ITEM => {
                        let q = mapr(d.r);
                        let side = (q[2] - q[0]).min(q[3] - q[1]);
                        let isz = ((side * 0.95) / 16.0).round().max(1.0) * 16.0;
                        let isz = if isz > side * 1.05 { (isz - 16.0).max(16.0) } else { isz };
                        out.pics.push([0.0, d.v, (q[0] + q[2]) / 2.0 - isz / 2.0, (q[1] + q[3]) / 2.0 - isz / 2.0, isz]);
                    }
                    deco::SCROLL => {
                        let q = mapr(d.r);
                        let w = (4.0 * u).min(q[2] - q[0]);
                        let x = (q[0] + q[2]) / 2.0 - w / 2.0;
                        c.rect([x, q[1], x + w, q[3]], w / 2.0, grey(1.0, 0.06));
                        let th = ((q[3] - q[1]) * 0.28).max(12.0 * u);
                        let y = q[1] + (q[3] - q[1] - th) * d.v.clamp(0.0, 1.0);
                        c.rect([x, y, x + w, y + th], w / 2.0, grey(1.0, if d.w > 0.5 { 0.38 } else { 0.14 }));
                    }
                    deco::MAP => {
                        let q = mapr(d.r);
                        c.rect(q, 10.0 * u, grey(0.0, 0.28));
                        out.decor.push([d.kind as f32, q[0], q[1], q[2], q[3]]);
                    }
                    deco::BREW => {
                        let q = mapr(d.r);
                        let x = (q[0] + q[2]) / 2.0;
                        let w = (2.6 * u).max(1.5);
                        c.line([x, q[1]], [x, q[3]], w, dim, 0.0);
                        if d.v > 0.0 {
                            c.line([x, q[1]], [x, q[1] + (q[3] - q[1]) * d.v.clamp(0.0, 1.0)], w, lit, 0.0);
                        }
                    }
                    deco::FUEL => {
                        let q = mapr(d.r);
                        let h = (q[3] - q[1]).max(3.0 * u);
                        let my = (q[1] + q[3]) / 2.0;
                        let bar = [q[0], my - h / 2.0, q[2], my + h / 2.0];
                        c.rect(bar, h / 2.0, grey(1.0, 0.1));
                        if d.v > 0.0 {
                            c.rect([bar[0], bar[1], bar[0] + (bar[2] - bar[0]) * d.v.clamp(0.0, 1.0), bar[3]], h / 2.0, lit);
                        }
                    }
                    deco::BUBBLES => {
                        if d.v > 0.0 {
                            let q = mapr(d.r);
                            for n in 0..3 {
                                let ph = (d.v + n as f32 / 3.0).fract();
                                let x = q[0] + (q[2] - q[0]) * (0.25 + 0.25 * n as f32);
                                let y = q[3] - (q[3] - q[1]) * ph;
                                c.ring([x, y], (1.6 + n as f32 * 0.5) * u, 1.0 * u, grey(1.0, 0.45 * (1.0 - ph)));
                            }
                        }
                    }
                    deco::FIELD => {
                        let q = mapr(d.r);
                        let fr = [q[0], q[1] - 3.0 * u, q[2], q[3] + 3.0 * u];
                        c.rect(fr, 7.0 * u, grey(0.0, 0.34));
                        c.rect_line(fr, 7.0 * u, 1.0 * u, grey(1.0, 0.08));
                        let fs = 12.5 * u;
                        let base = (fr[1] + fr[3]) / 2.0 + fs * 0.36;
                        let words = d.text.plain();
                        let chars: Vec<char> = words.chars().collect();
                        let upto = |n: usize| -> String { chars.iter().take(n.min(chars.len())).collect() };
                        let tx = fr[0] + 8.0 * u;
                        let (cur, other) = (d.v.max(0.0) as usize, d.w.max(0.0) as usize);
                        c.push_clip([fr[0] + 2.0 * u, fr[1], fr[2] - 2.0 * u, fr[3]]);
                        if cur != other {
                            let (a, b) = (cur.min(other), cur.max(other));
                            let xa = tx + fonts().width(Face::Regular, fs, &upto(a));
                            let xb = tx + fonts().width(Face::Regular, fs, &upto(b));
                            c.rect([xa, fr[1] + 4.0 * u, xb, fr[3] - 4.0 * u], 2.0, fade(text, 0.22));
                        }
                        c.text(tx, base, fs, Face::Regular, &words, text);
                        if d.v >= 0.0 && (self.now * 1.9).fract() < 0.55 {
                            let cx = tx + fonts().width(Face::Regular, fs, &upto(cur));
                            c.rect([cx, fr[1] + 5.0 * u, cx + 1.4 * u, fr[3] - 5.0 * u], 0.5, text);
                        }
                        c.pop_clip();
                        out.decor.push([d.kind as f32, fr[0], fr[1], fr[2], fr[3]]);
                    }
                    deco::LABEL => {
                        let q = mapr(d.r);
                        let fs = 12.0 * u;
                        let col = match d.v as i32 {
                            1 => [0.5, 0.9, 0.5, 1.0],
                            2 => [0.95, 0.45, 0.45, 1.0],
                            _ => text2,
                        };
                        let tw = d.text.width(fs, Face::Medium);
                        let x = match d.z as i32 {
                            1 => (q[0] + q[2]) / 2.0 - tw / 2.0,
                            2 => q[0],
                            _ => q[2] - tw,
                        };
                        let icons = std::mem::take(&mut self.hud.icons);
                        self.rtext(&mut c, &d.text, x, (q[1] + q[3]) / 2.0 + fs * 0.36, fs, Face::Medium, col, true);
                        out.glyphs.extend(std::mem::replace(&mut self.hud.icons, icons));
                    }
                    deco::ENTITY => {
                        let q = mapr(d.r);
                        c.rect(q, 8.0 * u, grey(0.0, 0.22));
                        out.decor.push([d.kind as f32, q[0], q[1], q[2], q[3]]);
                    }
                    deco::PLUS => {
                        let q = mapr(d.r);
                        let s = (q[2] - q[0]).min(q[3] - q[1]) * 0.55;
                        let m = [(q[0] + q[2]) / 2.0, (q[1] + q[3]) / 2.0];
                        c.icon([m[0] - s / 2.0, m[1] - s / 2.0, m[0] + s / 2.0, m[1] + s / 2.0], 4, dim);
                    }
                    deco::POWER => {
                        let q = mapr(d.r);
                        let m = [(q[0] + q[2]) / 2.0, (q[1] + q[3]) / 2.0];
                        let rr = 4.0 * u;
                        c.rect([m[0] - rr, m[1] - rr, m[0] + rr, m[1] + rr], rr, if d.v > 0.5 { [0.95, 0.3, 0.3, 1.0] } else { grey(1.0, 0.14) });
                        if d.v > 0.5 {
                            c.glow(m, rr * 3.0, [0.95, 0.3, 0.3, 0.35]);
                        }
                    }
                    _ => {}
                }
            }
            c.alpha = 1.0;
        }
        // the container's title
        if titles {
            if let Some(bi) = blocks.iter().position(|b| b.0 == Block::Container) {
                let r = blocks[bi].1;
                c.alpha = seen(bi);
                let tx = r[0] + pad;
                // a server's picture in the title's glyphs is not drawn: its words only, in the block's width
                let words = f.title.words_only();
                c.push_clip([tx, r[1], r[2] - pad, r[3]]);
                self.rtext(&mut c, &words, tx, r[1] + lift(bi) + pad + title_fs * 0.95, title_fs, Face::Semi, text, false);
                c.pop_clip();
                c.alpha = 1.0;
            }
        }
        // the player
        if let Some(bi) = blocks.iter().position(|b| b.0 == Block::Player) {
            let r = blocks[bi].1;
            let l = lift(bi);
            let inner = [r[0] + pad, r[1] + pad + l, r[2] - pad, r[3] - pad + l];
            // a soft light on the floor under them
            c.alpha = seen(bi);
            c.glow([(inner[0] + inner[2]) / 2.0, inner[3] - 6.0 * u], (inner[2] - inner[0]) * 0.42, grey(1.0, 0.05));
            c.alpha = 1.0;
            out.model = [inner[0], inner[1], inner[2], inner[3], (inner[3] - inner[1]) * 0.42];
        }
        // the effects beside it all: an icon, the name, the time
        if on("Effects") && !f.effects.is_empty() {
            let ex = eff[0];
            for (k, (name, ticks, level, _first)) in f.effects.iter().enumerate() {
                let a = seen(k + blocks.len());
                let y = eff[1] + k as f32 * (eh + 6.0 * u) + (1.0 - a) * 10.0 * u;
                let r = [ex, y, ex + ew, y + eh];
                c.alpha = a;
                c.glass(r, brad.min(eh / 2.0), glass, look.blur, if look.shadow { 0.45 } else { 0.0 }, 0.0);
                let is = 22.0 * u;
                out.effects.push([k as f32, r[0] + 9.0 * u, (r[1] + r[3]) / 2.0 - is / 2.0, is, a]);
                let lv = if *level > 0 { format!(" {}", roman(*level + 1)) } else { String::new() };
                let tx = r[0] + 9.0 * u + is + 9.0 * u;
                let label = fonts().fit(Face::Medium, 12.0 * u, &format!("{name}{lv}"), r[2] - tx - 8.0 * u);
                c.text(tx, r[1] + 17.0 * u, 12.0 * u, Face::Medium, &label, text);
                let time = if *ticks < 0.0 { "∞".to_string() } else { mmss_of(*ticks / 20.0) };
                c.text(tx, r[1] + 31.0 * u, 10.5 * u, Face::Mono, &time, text2);
                out.blocks.push(r);
            }
            c.alpha = 1.0;
        }

        // ---- the recipe book: its quiet button by the top right corner; open, its block on the left (the search, all or what
        // can be made now, the book's tabs, the recipes in cells: those that cannot be made yet dimmed)
        if let Some(rec) = &f.rec {
            let book = pen;
            out.blocks.push(book);
            let br = (brad * 0.6).min((book[3] - book[1]) / 2.0);
            let bh = self.an(ak(A_PK, 11), if inside4(book, m) { 1.0 } else { 0.0 }, 16.0, 0.0);
            let lit = bh.max(if rec.open { 1.0 } else { 0.0 });
            top.alpha = seen(0);
            top.glass(book, br, glass, look.blur, 0.0, 0.0);
            top.rect(book, br, grey(1.0, 0.04 + 0.06 * bh + if rec.open { 0.1 } else { 0.0 }));
            book_icon(&mut top, [(book[0] + book[2]) / 2.0, (book[1] + book[3]) / 2.0], 7.0 * u, fade(mix(text2, text, lit), 0.6 + 0.4 * lit), u);
            top.alpha = 1.0;
            let mut ro: Vec<f32> = vec![1.0];
            ro.extend_from_slice(&book);
            let at = blocks.iter().position(|b| b.0 == Block::Recipes);
            match (rec.open, at) {
                (true, Some(bi)) => {
                    let r = blocks[bi].1;
                    let l = lift(bi);
                    let r = [r[0], r[1] + l, r[2], r[3] + l];
                    c.alpha = seen(bi);
                    top.alpha = seen(bi);
                    let fh = 30.0 * u;
                    // the filter as wide as its word, the search the rest
                    let pill_word = if rec.filter { tr("Can make") } else { tr("All recipes") };
                    let pill_w = (fonts().width(Face::Medium, 11.5 * u, pill_word) + 22.0 * u).min((r[2] - r[0] - 2.0 * pad) * 0.5);
                    let search = [r[0] + pad, r[1] + pad, r[2] - pad - pill_w - 6.0 * u, r[1] + pad + fh];
                    let filt = [r[2] - pad - pill_w, r[1] + pad, r[2] - pad, r[1] + pad + fh];
                    // the search
                    c.rect(search, 8.0 * u, cell_col);
                    let fa = self.an(ak(A_PK, 12), if rec.focused { 1.0 } else { 0.0 }, 9.0, 0.0);
                    // (its keys taken: a breath lighter)
                    if fa > 0.01 {
                        c.rect(search, 8.0 * u, grey(1.0, 0.05 * fa));
                    }
                    let smid = (search[1] + search[3]) / 2.0;
                    c.icon([search[0] + 8.0 * u, smid - 6.0 * u, search[0] + 20.0 * u, smid + 6.0 * u], 13, if rec.focused { text } else { text2 });
                    let tx = search[0] + 27.0 * u;
                    let tmax = search[2] - tx - 6.0 * u;
                    let fs = 12.5 * u;
                    let qw = fonts().width(Face::Regular, fs, &rec.q);
                    let shift = (qw - tmax + 3.0 * u).max(0.0);
                    if rec.q.is_empty() {
                        let ph = fonts().fit(Face::Regular, fs, if fonts().width(Face::Regular, fs, tr("Search recipes")) <= tmax { tr("Search recipes") } else { tr("Search") }, tmax);
                        c.text(tx, smid + fs * 0.36, fs, Face::Regular, &ph, fade(text2, 0.8));
                    } else {
                        c.push_clip([tx, search[1], tx + tmax, search[3]]);
                        c.text(tx - shift, smid + fs * 0.36, fs, Face::Regular, &rec.q, text);
                        c.pop_clip();
                    }
                    if rec.focused && (self.now * 1.9).fract() < 0.6 {
                        let x = tx + qw - shift + 1.0 * u;
                        c.rect([x, smid - 7.0 * u, x + 1.3 * u, smid + 7.0 * u], 0.6 * u, text);
                    }
                    // all, or only what can be made now
                    let fhv = self.an(ak(A_PK, 13), if inside4(filt, m) { 1.0 } else { 0.0 }, 16.0, 0.0);
                    c.rect(filt, fh / 2.0, if rec.filter { grey(1.0, 0.13 + 0.05 * fhv) } else { grey(1.0, 0.045 + 0.05 * fhv) });
                    let word = fonts().fit(Face::Medium, 11.5 * u, pill_word, filt[2] - filt[0] - 10.0 * u);
                    c.text_c((filt[0] + filt[2]) / 2.0, smid + 4.2 * u, 11.5 * u, Face::Medium, &word, if rec.filter { text } else { mix(text2, text, fhv) });
                    // the book's tabs (their pictures are Java's)
                    let ts = 30.0 * u;
                    let ty = search[3] + 8.0 * u;
                    let mut tabs_o: Vec<[f32; 4]> = Vec::new();
                    for (k, chosen) in rec.tabs.iter().enumerate() {
                        let x = r[0] + pad + k as f32 * (ts + 5.0 * u);
                        let q = [x, ty, x + ts, ty + ts];
                        if q[2] > r[2] - pad + 0.5 {
                            break;
                        }
                        let hv = self.an(ak(A_PK, 7000 + k), if inside4(q, m) && !chosen { 1.0 } else { 0.0 }, 16.0, 0.0);
                        c.rect(q, crad.min(ts / 2.0), if *chosen { grey(1.0, 0.08) } else { cell_col });
                        if *chosen {
                            lamp(&mut c, q, crad.min(ts / 2.0), ring_col);
                        }
                        if hv > 0.01 {
                            c.rect(q, crad.min(ts / 2.0), fade(hover_col, hv));
                        }
                        let isz = ((ts * 0.75) / 16.0).round().max(1.0) * 16.0;
                        let isz = if isz > ts { (isz - 16.0).max(16.0) } else { isz };
                        tabs_o.push([x + (ts - isz) / 2.0, ty + (ts - isz) / 2.0, isz, *chosen as i32 as f32]);
                    }
                    // the recipes, five across, scrolling smoothly
                    let ga = [r[0] + pad, ty + ts + 10.0 * u, r[2] - pad, r[3] - pad];
                    let n = rec.items.len();
                    let rows = n.div_ceil(5);
                    let content = (rows as f32 * pitch - gap).max(0.0);
                    let view = ga[3] - ga[1];
                    let max = (content - view).max(0.0);
                    let key = {
                        use std::hash::{Hash, Hasher};
                        let mut hs = std::collections::hash_map::DefaultHasher::new();
                        rec.q.hash(&mut hs);
                        rec.tabs.iter().position(|t| *t).hash(&mut hs);
                        rec.filter.hash(&mut hs);
                        hs.finish()
                    };
                    let scroll = {
                        let mut st = REC_SCROLL.lock().unwrap();
                        if st.key != key {
                            st.key = key;
                            st.target = 0.0;
                            st.pos = 0.0;
                        }
                        st.target = (st.target - rec.wheel * pitch * 1.5).clamp(0.0, max);
                        st.pos += (st.target - st.pos) * (1.0 - 0.0004f32.powf(self.dt));
                        if (st.pos - st.target).abs() < 0.3 {
                            st.pos = st.target;
                        }
                        st.pos = st.pos.clamp(0.0, max);
                        st.pos
                    };
                    let mut items_o: Vec<[f32; 9]> = Vec::new();
                    let mut pointed = -1.0f32;
                    c.push_clip(ga);
                    top.push_clip(ga);
                    let r0 = (scroll / pitch).floor().max(0.0) as usize;
                    let r1 = ((scroll + view) / pitch).ceil() as usize;
                    for i in (r0 * 5)..((r1 + 1) * 5).min(n) {
                        let (row, col) = (i / 5, i % 5);
                        let x = ga[0] + col as f32 * pitch;
                        let y = ga[1] + row as f32 * pitch - scroll;
                        let q = [x, y, x + cell, y + cell];
                        let yc = (q[1] + q[3]) / 2.0;
                        let edge = ((yc - ga[1]) / (cell * 0.5) + 0.5).clamp(0.0, 1.0).min(((ga[3] - yc) / (cell * 0.5) + 0.5).clamp(0.0, 1.0));
                        let hit = [q[0] - gap / 2.0, (q[1] - gap / 2.0).max(ga[1]), q[2] + gap / 2.0, (q[3] + gap / 2.0).min(ga[3])];
                        let hov = inside4(hit, m) && !editing;
                        if hov {
                            pointed = i as f32;
                        }
                        let (can, multi) = rec.items[i];
                        let hv = self.an(ak(A_PK, 8000 + i % 512), if hov { 1.0 } else { 0.0 }, 18.0, 0.0);
                        c.alpha = seen(bi) * edge;
                        c.rect(q, crad, cell_col);
                        if hv > 0.01 {
                            c.rect(q, crad, fade(hover_col, hv));
                        }
                        let isz = ((cell * 0.86) / 16.0).round().max(1.0) * 16.0;
                        let isz = if isz > cell { (isz - 16.0).max(16.0) } else { isz };
                        items_o.push([i as f32, x + (cell - isz) / 2.0, y + (cell - isz) / 2.0, isz, seen(bi) * edge, hit[0], hit[1], hit[2], hit[3]]);
                        top.alpha = seen(bi) * edge;
                        // not yet: a shade over it (lighter under the pointer)
                        if !can {
                            top.rect(q, crad, grey(0.0, 0.55 - 0.2 * hv));
                        }
                        // more than one way: two small dots in the corner
                        if multi {
                            for d in 0..2 {
                                let dx = q[2] - 5.0 * u - d as f32 * 4.5 * u;
                                top.rect([dx - 1.4 * u, q[3] - 6.4 * u, dx + 1.4 * u, q[3] - 3.6 * u], 1.4 * u, grey(1.0, if can { 0.75 } else { 0.35 }));
                            }
                        }
                    }
                    c.pop_clip();
                    top.pop_clip();
                    c.alpha = seen(bi);
                    top.alpha = 1.0;
                    if n == 0 {
                        let words = if rec.q.is_empty() { tr("Nothing to make yet") } else { tr("Nothing found") };
                        c.text_c((ga[0] + ga[2]) / 2.0, ga[1] + 40.0 * u, 12.5 * u, Face::Regular, words, fade(text2, 0.8));
                    }
                    if max > 0.0 {
                        let bx = r[2] - pad / 2.0 - 1.5 * u;
                        let vis = (view / content).min(1.0);
                        let y0b = ga[1] + scroll / max * (1.0 - vis) * view;
                        c.rect([bx, ga[1], bx + 3.0 * u, ga[3]], 1.5 * u, grey(1.0, 0.05));
                        c.rect([bx, y0b, bx + 3.0 * u, y0b + vis * view], 1.5 * u, grey(1.0, 0.35));
                    }
                    c.alpha = 1.0;
                    ro.push(1.0);
                    ro.extend_from_slice(&r);
                    ro.extend_from_slice(&search);
                    ro.extend_from_slice(&filt);
                    ro.extend_from_slice(&ga);
                    ro.push(tabs_o.len() as f32);
                    for t in &tabs_o {
                        ro.extend_from_slice(t);
                    }
                    ro.push(items_o.len() as f32);
                    for it in &items_o {
                        ro.extend_from_slice(it);
                    }
                    ro.push(pointed);
                }
                _ => {
                    ro.push(0.0);
                    ro.extend_from_slice(&[0.0; 16]);
                    ro.push(0.0);
                    ro.push(0.0);
                    ro.push(-1.0);
                }
            }
            out.rec = ro;
        }

        // ---- over the items: the counts, the wear, the cooldowns
        for &(i, r, bi) in &cells {
            let s = &f.slots[i];
            if !s.item && s.preview < 0 {
                continue;
            }
            top.alpha = seen(bi);
            let size = r[2] - r[0];
            // what the recipe still needs: faint, a thin ring round it
            if s.flags & 2 != 0 {
                let rr = if size > cell + 0.5 { crad * 1.18 } else { crad };
                top.rect(r, rr, grey(0.02, 0.58));
                top.rect_line(r, rr, (1.2 * u).max(1.0), [1.0, 0.55, 0.52, 0.45]);
                continue;
            }
            if s.cool > 0.0 {
                top.rect([r[0], r[1] + size * (1.0 - s.cool), r[2], r[3]], crad, grey(1.0, 0.22));
            }
            if s.max > 0.0 && s.dmg > 0.0 {
                let k = 1.0 - s.dmg / s.max;
                let bw = size - 10.0 * u;
                let y = r[3] - 5.0 * u;
                top.rect([r[0] + 5.0 * u, y, r[0] + 5.0 * u + bw, y + 2.4 * u], 1.2 * u, grey(0.0, 0.7));
                top.rect([r[0] + 5.0 * u, y, r[0] + 5.0 * u + bw * k, y + 2.4 * u], 1.2 * u, wear_col(k));
            }
            let n = if s.preview >= 0 { s.preview as u32 } else { s.count };
            if n > 1 || s.preview >= 0 {
                let fs = 12.0 * u;
                let label = n.to_string();
                let x = r[2] - 3.0 * u;
                let y = r[3] - 4.0 * u;
                top.text_r(x + 1.0 * u, y + 1.0 * u, fs, Face::Semi, &label, grey(0.0, 0.75));
                top.text_r(x, y, fs, Face::Semi, &label, if s.preview == 0 { [1.0, 0.85, 0.3, 1.0] } else { grey(1.0, 0.96) });
            }
        }
        top.alpha = 1.0;
        for p in &out.pics {
            let n = f.extras.get(p[1] as usize).copied().unwrap_or(0);
            if n > 1 {
                let fs = 11.5 * u;
                let x = p[2] + p[4] + 2.0 * u;
                let y = p[3] + p[4] + 1.0 * u;
                top.text_r(x + 1.0 * u, y + 1.0 * u, fs, Face::Semi, &n.to_string(), grey(0.0, 0.75));
                top.text_r(x, y, fs, Face::Semi, &n.to_string(), grey(1.0, 0.96));
            }
        }
        out.carried = cell * 0.9;
        // the carried stack's count
        if f.carried > 1 {
            let fs = 12.0 * u;
            let x = m[0] + cell * 0.45 - 1.0 * u;
            let y = m[1] + cell * 0.45;
            top.text_r(x + 1.0 * u, y + 1.0 * u, fs, Face::Semi, &f.carried.to_string(), grey(0.0, 0.75));
            top.text_r(x, y, fs, Face::Semi, &f.carried.to_string(), grey(1.0, 0.96));
        }

        // ---- the tooltip: a card of the HUD's by the pointer, the game's lines in their colours
        if !f.tip.is_empty() && f.carried == 0 && !editing {
            let fs1 = 14.0 * u;
            let fs = 12.5 * u;
            let lh = fs * 1.5;
            let pd = 12.0 * u;
            let lines: Vec<&RText> = f.tip.iter().collect();
            let mut tw = lines[0].width(fs1, Face::Semi);
            for l in &lines[1..] {
                tw = tw.max(l.width(fs, Face::Regular));
            }
            // the name's baseline, then each line's under it (the first a breath further)
            let b0 = pd + fs1 * 0.8;
            let last = b0 + if lines.len() > 1 { 6.0 * u + (lines.len() - 1) as f32 * lh } else { 0.0 };
            let (cw, ch) = (tw + 2.0 * pd, last + fs * 0.32 + pd);
            let mut x = m[0] + 16.0 * u;
            let mut y = m[1] - 10.0 * u;
            if x + cw > w - 8.0 * u {
                x = m[0] - 16.0 * u - cw;
            }
            y = y.clamp(8.0 * u, (h - ch - 8.0 * u).max(8.0 * u));
            let kt = self.an(ak(A_PK, 1), 1.0, 20.0, 0.0);
            top.alpha = kt;
            let r = [x, y, x + cw, y + ch];
            top.shadow(r, 10.0 * u, 22.0 * u, grey(0.0, 0.5));
            let gl = look.glass;
            top.rect(r, (brad * 0.8).max(6.0 * u), [gl[0].max(0.05), gl[1].max(0.05), gl[2].max(0.06), 1.0]);
            let icons = std::mem::take(&mut self.hud.icons);
            self.rtext(&mut top, lines[0], x + pd, y + b0, fs1, Face::Semi, text, true);
            for (k, l) in lines[1..].iter().enumerate() {
                self.rtext(&mut top, l, x + pd, y + b0 + 6.0 * u + (k + 1) as f32 * lh, fs, Face::Regular, text2, true);
            }
            out.glyphs.extend(std::mem::replace(&mut self.hud.icons, icons));
            top.alpha = 1.0;
        } else {
            self.anim.insert(ak(A_PK, 1), 0.0);
        }
        // ---- the headers' handles; the right button on a block's empty place (not the container's: its own buttons and
        // fields are there, nor the recipe book's) opens the settings by it; the settings' panel over all
        let mut held_list: Vec<(&'static str, [f32; 4], [f32; 4], [f32; 4])> = blocks.iter().map(|(b, r)| (b.id(), *r, *r, head_of(*b, *r))).collect();
        if on("Effects") && !f.effects.is_empty() {
            held_list.push(("inv-effects", eff, eff, [eff[0], eff[1] - 16.0 * u, eff[2], eff[1] + 4.0 * u]));
        }
        let empty = if hovered.is_none() {
            blocks.iter().find(|(b, r)| !matches!(b, Block::Container | Block::Recipes) && inside4(*r, m)).map(|(_, r)| *r).or_else(|| (on("Effects") && !f.effects.is_empty() && inside4(eff, m)).then_some(eff))
        } else {
            None
        };
        let eats = self.iu_after(&mut top, m, u, w, h, look.accent, &held_list, empty, hovered.is_some());
        // (the recipe book's button by the corner is the screen's too: the panel keeps clear of it)
        if f.rec.is_some() {
            let a = &mut self.hud.inv.all;
            *a = [a[0].min(pen[0]), a[1].min(pen[1]), a[2].max(pen[2]), a[3].max(pen[3])];
        }
        self.iu_panel(&mut top, w, h);
        out.edit = eats.flat();
        (c, top, out.flat())
    }
}

impl Gui {
    /// Custom Inventory's preview in the menu: the survival inventory's blocks as its settings make them now (cells,
    /// gaps, corners, colours, the room inside, the gap between the blocks), a few cells holding something, the hotbar's
    /// chosen one ringed, a cell under an imagined pointer lit; the effects beside them when they are on.
    pub(crate) fn inventory_preview(&mut self, c: &mut Canvas, st: [f32; 4], reveal: f32, i: usize) {
        let r = self.r;
        let t = self.now;
        let set = self.mods[i].set.clone();
        let num = |l: &str| theme::num(&set, l);
        let open = ease_out(((reveal - 0.12) / 0.7).clamp(0.0, 1.0));
        let cy0 = (st[1] + st[3]) / 2.0;
        let hh = (st[3] - st[1]) / 2.0 * open;
        c.push_clip([st[0], cy0 - hh, st[2], cy0 + hh]);
        c.rect(st, 0.75 * r, grey(0.0, 0.28));
        c.interference(st, 0.75 * r, t, 5.0, 0.3);
        let drain0 = c.drain;
        c.drain = 1.0;
        let look = theme::HudLook::of(&self.theme, t);
        // the layout at 1 px a unit, then fitted into the stage
        let cell = num("Cell size").max(20.0);
        let gap = num("Cell gap");
        let pitch = cell + gap;
        let pad = num("Inner space");
        let bgap = num("Block gap");
        let effects = theme::on(&set, "Effects");
        let armour = [cell + 2.0 * pad, 5.0 * pitch - gap + 0.35 * pitch + 2.0 * pad];
        let crafting = [3.0 * pitch + 0.9 * pitch - gap + 2.0 * pad, armour[1]];
        let back = [9.0 * pitch - gap + 2.0 * pad, 3.0 * pitch - gap + 2.0 * pad];
        let hot = [back[0], cell + 2.0 * pad];
        let width = back[0].max(armour[0] + crafting[0] + 2.0 * pitch + 2.0 * bgap);
        let player_w = width - armour[0] - crafting[0] - 2.0 * bgap;
        let eff_w = if effects { 150.0 + bgap } else { 0.0 };
        let total = [width + eff_w, armour[1] + bgap + back[1] + bgap + hot[1]];
        let room = [st[2] - st[0] - 2.0 * r, (st[3] - st[1]).min(22.0 * r) - 2.0 * r];
        let u = (room[0] / total[0]).min(room[1] / total[1]).min(1.4);
        let ox = (st[0] + st[2]) / 2.0 - total[0] * u / 2.0 + eff_w * u;
        let oy = st[1] + ((st[3] - st[1]).min(22.0 * r) - total[1] * u) / 2.0;
        let rect = |x: f32, y: f32, w: f32, h: f32| [ox + x * u, oy + y * u, ox + (x + w) * u, oy + (y + h) * u];
        let brad = look.radius * u * num("Corners") / 100.0;
        let crad = (cell * u * 0.5 * num("Cell corners") / 100.0).min(cell * u / 2.0);
        let glass = theme::or(theme::col(&set, "Glass"), look.glass, t);
        let cell_col = theme::or(theme::col(&set, "Cell colour"), grey(0.0, 0.32), t);
        let hover_col = theme::or(theme::col(&set, "Hover colour"), grey(1.0, 0.12), t);
        let ring_col = theme::or(theme::col(&set, "Chosen ring"), look.accent, t);
        let sh = if look.shadow { 0.5 } else { 0.0 };
        let y1 = armour[1] + bgap;
        let y2 = y1 + back[1] + bgap;
        let blocks = [
            rect(0.0, 0.0, armour[0], armour[1]),
            rect(armour[0] + bgap, 0.0, player_w, armour[1]),
            rect(width - crafting[0], 0.0, crafting[0], crafting[1]),
            rect(0.0, y1, width, back[1]),
            rect(0.0, y2, width, hot[1]),
        ];
        for b in &blocks {
            c.glass(*b, brad, glass, look.blur, sh, 0.0);
        }
        // a cell: some hold something (a soft light in them), one lit as if under the pointer
        let filled = |k: usize| (k * 7 + 3) % 5 == 0;
        let lit_k = ((t * 0.6) as usize) % 9;
        let cellr = |c: &mut Canvas, x: f32, y: f32, k: usize, ring: bool, lit: bool| {
            let q = [ox + x * u, oy + y * u, ox + (x + cell) * u, oy + (y + cell) * u];
            c.rect(q, crad, cell_col);
            if filled(k) {
                c.rect([q[0] + (q[2] - q[0]) * 0.28, q[1] + (q[3] - q[1]) * 0.28, q[2] - (q[2] - q[0]) * 0.28, q[3] - (q[3] - q[1]) * 0.28], crad * 0.5, grey(1.0, 0.22));
            }
            if lit {
                c.rect(q, crad, hover_col);
            }
            if ring {
                lamp(c, q, crad, ring_col);
            }
        };
        for k in 0..4 {
            cellr(c, pad, pad + k as f32 * pitch, 40 + k, false, false);
        }
        cellr(c, pad, pad + 4.35 * pitch, 50, false, false);
        let cx0 = width - crafting[0] + pad;
        let ctop = (armour[1] - (2.0 * pitch - gap)) / 2.0;
        for k in 0..4 {
            cellr(c, cx0 + (k % 2) as f32 * pitch, ctop + (k / 2) as f32 * pitch, 60 + k, false, false);
        }
        let big = cell * 1.18;
        let rq = rect(width - pad - big, armour[1] / 2.0 - big / 2.0, big, big);
        c.rect(rq, crad * 1.18, cell_col);
        let ax = cx0 + 2.0 * pitch - gap + 0.12 * pitch;
        let s_ = 0.62 * pitch;
        c.icon(rect(ax, armour[1] / 2.0 - s_ / 2.0, s_, s_), 8, grey(1.0, 0.32));
        // the player: a figure's outline in the light
        let pb = blocks[1];
        let pcx = (pb[0] + pb[2]) / 2.0;
        c.glow([pcx, pb[3] - 8.0 * u], (pb[2] - pb[0]) * 0.4, grey(1.0, 0.05));
        let fh = (pb[3] - pb[1]) * 0.62;
        let fw = fh * 0.28;
        let fy = pb[3] - (pb[3] - pb[1]) * 0.12 - fh;
        c.rect([pcx - fw * 0.45, fy, pcx + fw * 0.45, fy + fh * 0.24], 2.0 * u, grey(1.0, 0.12));
        c.rect([pcx - fw / 2.0, fy + fh * 0.26, pcx + fw / 2.0, fy + fh * 0.62], 2.0 * u, grey(1.0, 0.1));
        c.rect([pcx - fw / 2.0, fy + fh * 0.63, pcx - fw * 0.04, fy + fh], 2.0 * u, grey(1.0, 0.08));
        c.rect([pcx + fw * 0.04, fy + fh * 0.63, pcx + fw / 2.0, fy + fh], 2.0 * u, grey(1.0, 0.08));
        let bx0 = (width - (9.0 * pitch - gap)) / 2.0;
        for k in 0..27 {
            cellr(c, bx0 + (k % 9) as f32 * pitch, y1 + pad + (k / 9) as f32 * pitch, k, false, k == 9 + lit_k);
        }
        for k in 0..9 {
            cellr(c, bx0 + k as f32 * pitch, y2 + pad, 30 + k, k == 0, false);
        }
        // the effects beside them
        if effects {
            let ew = 150.0;
            let eh = 32.0;
            for k in 0..2 {
                let e = [ox - eff_w * u, oy + k as f32 * (eh + 5.0) * u, ox - bgap * u, oy + (k as f32 * (eh + 5.0) + eh) * u];
                let _ = ew;
                c.glass(e, brad.min((e[3] - e[1]) / 2.0), glass, look.blur, sh, 0.0);
                let is = 16.0 * u;
                c.rect([e[0] + 7.0 * u, (e[1] + e[3]) / 2.0 - is / 2.0, e[0] + 7.0 * u + is, (e[1] + e[3]) / 2.0 + is / 2.0], 3.0 * u, grey(1.0, 0.18));
                c.rect([e[0] + 30.0 * u, e[1] + 9.0 * u, e[0] + (90.0 - k as f32 * 20.0) * u, e[1] + 14.0 * u], 2.0 * u, fade(look.text, 0.7));
                c.rect([e[0] + 30.0 * u, e[1] + 19.0 * u, e[0] + 60.0 * u, e[1] + 23.0 * u], 2.0 * u, fade(look.text2, 0.6));
            }
        }
        c.drain = drain0;
        c.pop_clip();
    }
}

/// An open book, seen from above (`s` half its width).
fn book_icon(c: &mut Canvas, at: [f32; 2], s: f32, col: Rgba, u: f32) {
    let p = |x: f32, y: f32| [at[0] + x * s / 7.0, at[1] + y * s / 7.0];
    let lw = (1.4 * u).max(1.0);
    c.polyline(&[p(0.0, -3.6), p(-2.6, -5.2), p(-7.0, -5.2), p(-7.0, 4.6), p(-2.6, 4.6), p(0.0, 6.0)], lw, col);
    c.polyline(&[p(0.0, -3.6), p(2.6, -5.2), p(7.0, -5.2), p(7.0, 4.6), p(2.6, 4.6), p(0.0, 6.0)], lw, col);
    c.line(p(0.0, -3.6), p(0.0, 6.0), lw, col, 0.0);
}

/// Roman numerals for an effect's level (up to 10, then the number).
fn roman(n: u32) -> String {
    const R: [&str; 10] = ["I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X"];
    if (1..=10).contains(&n) { R[n as usize - 1].to_string() } else { n.to_string() }
}
