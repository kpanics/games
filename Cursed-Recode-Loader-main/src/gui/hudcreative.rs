//! Custom Inventory in creative (Interface → Custom Inventory): the creative inventory, all of it the client's. A rail
//! on the left (the search on top, favourites and the recently taken, the game's tabs with their counts, the saved
//! hotbars and the survival inventory last); the chosen one's items in a wide grid that scrolls smoothly; the hotbar
//! under it with a bin and the saving of the set; the item pointed at turning on the right with its words. It opens
//! from a line across the middle. Java's HudCreative sends what the game knows (CrF) and draws the items, the model
//! and the carried stack where `out` says; the game's own screen does the clicks (a grid item is the picker's first
//! slot with that item put in it).
//!
//! Laid out on a 1920 x 1080 picture fitted into the window (`u` window px to its px).

use super::hudinv::{fmt1, insp_line, lamp, read_insp, read_slot, rects_out, rgb, wear, Insp, InvSlot};
use super::hudinvui::header;
use super::*;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;

/// An item of the grid as Java has it (only those near what is seen are sent).
#[derive(Clone, Default)]
pub struct CrCell {
    pub col: u32,
    pub rar: u32,
    pub fav: bool,
    pub glint: bool,
    /// a saved hotbar's empty place (the game's paper that cannot be taken)
    pub locked: bool,
    pub count: u32,
}

/// A set of the whole inventory as Java has it: its name (none: "Set N"), how many each place of the player's menu
/// holds (its slots 5…45: the armour, the backpack, the hotbar, the other hand).
#[derive(Clone)]
pub struct CrKit {
    pub name: String,
    pub counts: [u32; 41],
}

impl CrKit {
    fn at(&self, slot: usize) -> u32 {
        if (5..=45).contains(&slot) { self.counts[slot - 5] } else { 0 }
    }
}

/// A row of the rail: 0 a tab of the game (its picture is its item: Java draws it), 1 favourites, 2 the recently taken,
/// 3 the sets.
#[derive(Clone, Default)]
pub struct RailRow {
    pub kind: u8,
    pub count: i32,
    pub sep: bool,
    pub on: bool,
    pub name: String,
}

/// What Java sends each frame the creative inventory is open.
#[derive(Clone, Default)]
pub struct CrF {
    pub since: f32,
    pub mouse: [f32; 2],
    pub sel: usize,
    pub carried: u32,
    /// 0 a category, 1 the search, 2 the saved hotbars, 3 the survival inventory, 4 favourites, 5 the recently taken,
    /// 6 the sets
    pub tab: u8,
    /// another number: another tab (its grid comes in from the top again)
    pub key: i64,
    /// the wheel turned since the last frame (notches, up positive)
    pub wheel: f32,
    pub n: usize,
    pub i0: usize,
    pub cells: Vec<CrCell>,
    /// the game's menu's slots by their index
    pub slots: Vec<InvSlot>,
    pub rail: Vec<RailRow>,
    pub insp: Option<Insp>,
    /// the saved hotbar the button saves into (-1 all taken); the last saved (seconds since, which)
    pub save_next: i32,
    pub saved_since: f32,
    pub saved_slot: i32,
    /// seconds since something was thrown into the bin
    pub bin_since: f32,
    pub title: String,
    pub q: String,
    pub cursor: usize,
    pub focused: bool,
    /// the game's keys that save and load a hotbar (C, X)
    pub keys: (String, String),
    /// seconds since the inventory was emptied by the bin (its undo is there for a while; below 0 none), Shift held
    pub cleared: f32,
    pub shift: bool,
    /// the mouse's buttons (held 1 the left, 2 the right; pressed since the last frame 4, 8), what is asked (2: Esc)
    pub buttons: u32,
    pub ask: u8,
    /// the sets; a button of one armed for its second click (which, 1 write over 2 delete); the name being typed (which,
    /// the words); what the undo puts back (0 the emptied inventory, 1 the one a set took the place of); a stack from a
    /// set carried (the bin takes it out of the set)
    pub kits: Vec<CrKit>,
    pub kit_arm: Option<(usize, u8)>,
    pub kit_name: Option<(usize, String)>,
    pub cleared_kind: u8,
    pub from_kit: bool,
}

impl CrF {
    /// nums: [gs, since, mouse x, y, sel, carried, tab, key, wheel, save next, saved since, saved slot, bin since,
    /// items N, first sent I0, sent K then 4 each (colour, name's colour, flags: 1 favourite 2 glint 4 locked, count),
    /// slots S then 10 each (hudinv read_slot), rail R then 4 each (kind, count, after a line, on), the inspected item
    /// (hudinv read_insp)]; text: lines "title", "q\twords\tcursor\tfocused", "keys\tsave\tload", "rail\tname" (each
    /// row in turn), "in", "il".
    pub fn parse(nums: &[f64], text: &str) -> CrF {
        let g = |i: usize| nums.get(i).copied().unwrap_or(0.0);
        let mut f = CrF {
            since: g(1) as f32,
            mouse: [g(2) as f32, g(3) as f32],
            sel: (g(4).max(0.0) as usize).min(8),
            carried: g(5).max(0.0) as u32,
            tab: g(6).clamp(0.0, 6.0) as u8,
            key: g(7) as i64,
            wheel: g(8) as f32,
            save_next: g(9) as i32,
            saved_since: g(10) as f32,
            saved_slot: g(11) as i32,
            bin_since: g(12) as f32,
            cleared: -1.0,
            n: g(13).max(0.0) as usize,
            i0: g(14).max(0.0) as usize,
            ..Default::default()
        };
        let k = (g(15).max(0.0) as usize).min(4096);
        let mut i = 16;
        for _ in 0..k {
            let fl = g(i + 2) as u32;
            f.cells.push(CrCell { col: g(i) as u32, rar: g(i + 1) as u32, fav: fl & 1 != 0, glint: fl & 2 != 0, locked: fl & 4 != 0, count: g(i + 3).max(0.0) as u32 });
            i += 4;
        }
        let s = (g(i).max(0.0) as usize).min(128);
        i += 1;
        for _ in 0..s {
            f.slots.push(read_slot(nums, i));
            i += 10;
        }
        let r = (g(i).max(0.0) as usize).min(32);
        i += 1;
        for _ in 0..r {
            f.rail.push(RailRow { kind: g(i) as u8, count: g(i + 1) as i32, sep: g(i + 2) > 0.5, on: g(i + 3) > 0.5, name: String::new() });
            i += 4;
        }
        let (has, mut insp, _) = read_insp(nums, i);
        let mut names = 0;
        for line in text.lines() {
            let mut p = line.split('\t');
            let key = p.next().unwrap_or("");
            let a = p.next().unwrap_or("").to_string();
            let b = p.next().unwrap_or("").to_string();
            match key {
                "title" => f.title = a,
                "q" => {
                    f.q = a;
                    f.cursor = b.parse().unwrap_or(usize::MAX);
                    f.focused = p.next() == Some("1");
                }
                "keys" => f.keys = (a, b),
                "cleared" => {
                    f.cleared = a.parse().unwrap_or(-1.0);
                    f.cleared_kind = b.parse().unwrap_or(0);
                }
                "kit" => {
                    let mut counts = [0u32; 41];
                    for (k, v) in b.split(',').enumerate().take(41) {
                        counts[k] = v.parse().unwrap_or(0);
                    }
                    f.kits.push(CrKit { name: a, counts });
                }
                "kitarm" => f.kit_arm = Some((a.parse().unwrap_or(usize::MAX), b.parse().unwrap_or(0))),
                "kitname" => f.kit_name = Some((a.parse().unwrap_or(usize::MAX), b)),
                "fromkit" => f.from_kit = true,
                "shift" => f.shift = a == "1",
                "mouse" => {
                    f.buttons = a.parse().unwrap_or(0);
                    f.ask = b.parse().unwrap_or(0);
                }
                "rail" => {
                    if let Some(row) = f.rail.get_mut(names) {
                        row.name = a;
                    }
                    names += 1;
                }
                "in" | "il" => insp_line(&mut insp, key, a, b),
                _ => {}
            }
        }
        if has {
            f.insp = Some(insp);
        }
        f
    }
}

/// The grid's scroll and what came in when, between frames.
struct State {
    /// the tab shown, since when (its grid coming in), the words searched (a new search: back to the top)
    key: i64,
    switched: f32,
    q: u64,
    /// the scroll: where it goes, where it is (px of the picture)
    target: f32,
    pos: f32,
    now: f32,
    since: f32,
}

static STATE: Mutex<State> = Mutex::new(State { key: i64::MIN, switched: -9.0, q: 0, target: 0.0, pos: 0.0, now: 0.0, since: f32::MAX });

/// Custom Inventory in creative's animations.
const A_CR: u64 = 49;

/// What `out` names things by: a slot of the game's menu by its index; the bin; a grid item by GRID + its place in
/// the list; a row of the rail by RAIL + its row; the search; the saving of the hotbar.
const BIN: usize = 9000;
const GRID: usize = 10000;
const RAIL: usize = 20000;
const SEARCH: usize = 30000;
const SAVE: usize = 30001;
const UNDO: usize = 30002;
const TOGGLE: usize = 30003;
/// A set's item (KIT + its place × 64 + the slot), its buttons (+ its place), a new one, its name.
const KIT: usize = 40000;
const KIT_TAKE: usize = 200000;
const KIT_OVER: usize = 300000;
const KIT_DEL: usize = 400000;
const KIT_NEW: usize = 500000;
const KIT_NAME: usize = 600000;
/// How long the bin's undo stays (s; Java's UNDO_NS).
const UNDO_FOR: f32 = 5.0;

/// The creative screen's blocks, each taken by its header and put where one likes (their places: the HUD's).
pub const CR_BLOCKS: [&str; 3] = ["cr-rail", "cr-grid", "cr-hot"];

/// The grid: nine across (as the hotbar); as many rows seen as the rail is tall (at least seven: the survival
/// inventory's tab).
const COLS: usize = 9;

/// The blocks' measures this frame (window px).
#[derive(Clone, Copy)]
struct CrL {
    u: f32,
    cell: f32,
    pitch: f32,
    pad: f32,
    /// the grid's rows seen
    rows: usize,
    /// the hotbar's block
    hotb: [f32; 4],
    /// the grid's own area (its cells, under the block's title)
    ga: [f32; 4],
}

impl CrL {
    /// A slot's cell by its index in the game's menu: the hotbar in its block; on the survival inventory's tab the
    /// armour down the left, the other hand down the right (the player between), the backpack at the grid's foot.
    fn place(&self, tab: u8, idx: usize) -> Option<[f32; 4]> {
        let at = |x: f32, y: f32| Some([x, y, x + self.cell, y + self.cell]);
        let hot = |k: usize| at(self.hotb[0] + self.pad + k as f32 * self.pitch, self.hotb[1] + self.pad);
        let g = |col: f32, row: f32| at(self.ga[0] + col * self.pitch, self.ga[1] + row * self.pitch);
        if tab == 3 {
            let back = self.rows as f32 - 3.0;
            let eq = ((back - 0.5 - 4.0) / 2.0).max(0.0);
            return match idx {
                5..=8 => g(0.0, eq + (idx - 5) as f32),
                45 => g(8.0, eq + 3.0),
                9..=35 => g(((idx - 9) % 9) as f32, back + ((idx - 9) / 9) as f32),
                36..=44 => hot(idx - 36),
                _ => None,
            };
        }
        match idx {
            45..=53 => hot(idx - 45),
            _ => None,
        }
    }

    fn bin(&self) -> [f32; 4] {
        let x = self.hotb[0] + self.pad + 9.0 * self.pitch + 8.0 * self.u;
        [x, self.hotb[1] + self.pad, x + self.cell, self.hotb[1] + self.pad + self.cell]
    }
}

impl Gui {
    /// The creative inventory's two pictures (under the items, over them) and what Java needs: [hovered, items R then
    /// 10 each (what, its item's x, y, side, how much bigger, seen, its box x0 y0 x1 y1), boxes B then 5 each (what,
    /// x0 y0 x1 y1), the rail's pictures P then 5 each (row, x, y, side, seen), the inspected item (shown, x, y, side,
    /// turn, tilt, seen: none now, its words are the tooltip's), the model (hudinv's 11; none off the survival
    /// inventory's tab), a box's items K then 4 each (index, x, y, side), the carried stack's side, the items to send
    /// next frame (from, to), the grid's box (x0 y0 x1 y1), the band seen (y0 y1: all of it)].
    pub fn creative_frame(&mut self, w: f32, h: f32, f: &CrF) -> (Canvas, Canvas, Vec<f32>) {
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
        let r = self.cr_draw(w, h, f, t);
        self.dt = menu_dt;
        r
    }

    /// The creative inventory as blocks of the client's glass (her choice, 15.10): the rail on the left (the search on
    /// top, favourites, the recently taken, the game's tabs with their counts, the saved hotbars, the survival
    /// inventory; the saving of the set at its foot), the chosen one's items in the grid's block (nine across, scrolling
    /// smoothly), the hotbar's block under it with the bin; the item pointed at as a tooltip card.
    fn cr_draw(&mut self, w: f32, h: f32, f: &CrF, t: f32) -> (Canvas, Canvas, Vec<f32>) {
        let mi = self.mod_index("custom-inventory");
        let set = self.mods[mi].set.clone();
        let num = |l: &str| theme::num(&set, l);
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
        let (t1, t2) = (look.text, look.text2);
        let m = f.mouse;
        let mut c = Canvas::new(w, h);
        let mut top = Canvas::new(w, h);
        c.drain = self.hud.veil * (1.0 - theme::pal().around);
        top.drain = c.drain;

        // ---- the blocks
        let head = 50.0 * u;
        let gutter = 22.0 * u;
        let rh = 32.0 * u;
        let search_h = 36.0 * u;
        let save_h = 40.0 * u;
        let grid_w = COLS as f32 * pitch - gap + gutter + 2.0 * pad;
        let hot_w = 10.0 * pitch - gap + 8.0 * u + 2.0 * pad;
        let bw = grid_w.max(hot_w);
        let hot_h = cell + 2.0 * pad;
        let mut rail_rows_h = 0.0;
        for row in &f.rail {
            rail_rows_h += rh + 2.0 * u + if row.sep { 10.0 * u } else { 0.0 };
        }
        // the rail whole or its icons alone (Left panel; its button by the search turns it): as narrow as a square then,
        // the search a square under the button; its height the same either way
        let icons_on = theme::pick(&set, "Left panel") == 1;
        let ik = self.an(ak(A_CR, 40), if icons_on { 1.0 } else { 0.0 }, 10.0, if icons_on { 1.0 } else { 0.0 });
        let rail_w = 248.0 * u + (2.0 * pad + search_h - 248.0 * u) * ik;
        let rail_min = pad + 2.0 * search_h + 16.0 * u + rail_rows_h + 8.0 * u + save_h + pad;
        let vis_rows = ((rail_min - bgap - hot_h - 2.0 * pad - head + gap) / pitch).ceil().clamp(7.0, 14.0) as usize;
        let grid_h = head + vis_rows as f32 * pitch - gap + 2.0 * pad;
        let right_h = grid_h + bgap + hot_h;
        let rail_h = rail_min.max(right_h);
        let total_w = rail_w + bgap + bw;
        let total_h = rail_h.max(right_h);
        let x0 = ((w - total_w) / 2.0).round();
        let y0 = ((h - total_h) / 2.0 - 10.0 * u).round();
        let ry = y0 + (total_h - right_h) / 2.0;
        let rail0 = [x0, y0, x0 + rail_w, y0 + rail_h];
        let gridb0 = [x0 + rail_w + bgap, ry, x0 + rail_w + bgap + bw, ry + grid_h];
        let hotb0 = [gridb0[0], gridb0[3] + bgap, gridb0[2], gridb0[3] + bgap + hot_h];
        // ---- where the player put them (each taken by its header), kept on the screen; the settings' panel takes its
        // part of the buttons first, the headers theirs (the block held laid where it is this very frame)
        let keep = |r: [f32; 4], o: [f32; 2]| -> [f32; 4] {
            let mut d = [o[0] * u, o[1] * u];
            d[0] = d[0].clamp(4.0 - r[0], (w - 4.0 - r[2]).max(4.0 - r[0]));
            d[1] = d[1].clamp(4.0 - r[1], (h - 4.0 - r[3]).max(4.0 - r[1]));
            [r[0] + d[0], r[1] + d[1], r[2] + d[0], r[3] + d[1]]
        };
        let off_of = |g: &Gui, id: &str| g.hud.places.get(id).map_or([0.0, 0.0], |p| p.1);
        let (edges, took) = self.iu_begin(f.buttons, f.since, f.ask, m);
        let mut rail = keep(rail0, off_of(self, "cr-rail"));
        let mut gridb = keep(gridb0, off_of(self, "cr-grid"));
        let mut hotb = keep(hotb0, off_of(self, "cr-hot"));
        let heads = |rail: [f32; 4], gridb: [f32; 4], hotb: [f32; 4]| -> Vec<(&'static str, [f32; 4], [f32; 4], [f32; 4])> {
            vec![
                ("cr-rail", rail0, rail, header(rail, 0.0, pad, u)),
                ("cr-grid", gridb0, gridb, header(gridb, pad + head, pad, u)),
                ("cr-hot", hotb0, hotb, header(hotb, 0.0, pad, u)),
            ]
        };
        if let Some((id, o)) = self.iu_blocks(edges, took, m, u, w, h, bgap, &heads(rail, gridb, hotb)) {
            match id {
                "cr-rail" => rail = keep(rail0, o),
                "cr-grid" => gridb = keep(gridb0, o),
                _ => hotb = keep(hotb0, o),
            }
        }
        let ga = [gridb[0] + pad, gridb[1] + pad + head, gridb[0] + pad + COLS as f32 * pitch - gap, gridb[1] + pad + head + vis_rows as f32 * pitch - gap];
        let l = CrL { u, cell, pitch, pad, rows: vis_rows, hotb, ga };
        // coming in: each block a little after the one before, rising into place
        let speed = look.speed;
        let seen = |i: usize| -> f32 { if !look.anim { 1.0 } else { ease_out(((f.since * speed - i as f32 * 0.035) / 0.22).clamp(0.0, 1.0)) } };
        let lift = |i: usize| (1.0 - seen(i)) * 10.0 * u;
        for (bi, r) in [rail, gridb, hotb].iter().enumerate() {
            c.alpha = seen(bi);
            c.glass([r[0], r[1] + lift(bi), r[2], r[3] + lift(bi)], brad, glass, look.blur, if look.shadow { 0.55 } else { 0.0 }, 0.0);
        }
        c.alpha = 1.0;

        // ---- the grid's scroll (smooth), the tab's coming in
        let grid_on = f.tab != 3 && f.tab != 6;
        let rows = f.n.div_ceil(COLS);
        // the sets' cards: a new one's, then one each (its name and buttons, its hotbar, its armour and the rest)
        let kit_new_h = 44.0 * u;
        let kit_cell = ((ga_w(pitch, gap) - 24.0 * u - 8.0 * gap) / 9.0).min(cell);
        let kit_h = 10.0 * u + 22.0 * u + 8.0 * u + kit_cell + 8.0 * u + 24.0 * u + 10.0 * u;
        let content = if f.tab == 6 { kit_new_h + f.kits.len() as f32 * (kit_h + 8.0 * u) + gap } else { rows as f32 * pitch };
        let view = vis_rows as f32 * pitch - gap;
        let max = (content - gap - view).max(0.0);
        let qh = {
            let mut hs = std::collections::hash_map::DefaultHasher::new();
            f.q.hash(&mut hs);
            hs.finish()
        };
        let (scroll, switched) = {
            let mut st = STATE.lock().unwrap();
            let dt = (t - st.now).clamp(0.0, 0.1);
            st.now = t;
            // opened again: from the top, with nothing coming in (the screen itself does)
            if f.since < st.since {
                st.key = f.key;
                st.switched = t - 9.0;
                st.target = 0.0;
                st.pos = 0.0;
                st.q = qh;
            }
            st.since = f.since;
            if st.key != f.key {
                st.key = f.key;
                st.switched = t;
                st.target = 0.0;
                st.pos = 0.0;
            }
            if st.q != qh {
                st.q = qh;
                st.target = 0.0;
            }
            st.target = (st.target - f.wheel * pitch * 1.5).clamp(0.0, max);
            st.pos += (st.target - st.pos) * (1.0 - 0.0004f32.powf(dt));
            if (st.pos - st.target).abs() < 0.3 {
                st.pos = st.target;
            }
            st.pos = st.pos.clamp(0.0, max);
            (st.pos, st.switched)
        };

        // ---- the rail's places
        let top_y = rail[1] + pad + lift(0);
        let toggle_r = [rail[2] - pad - search_h, top_y, rail[2] - pad, top_y + search_h];
        let sy = top_y + ik * (search_h + 6.0 * u);
        let sx1 = rail[2] - pad - search_h - 6.0 * u;
        let search_r = [rail[0] + pad, sy, sx1 + (rail[0] + pad + search_h - sx1) * ik, sy + search_h];
        // (the words go first as it narrows, come last as it widens)
        let words_k = (1.0 - ik * 2.0).clamp(0.0, 1.0);
        let mut rail_r: Vec<[f32; 4]> = Vec::with_capacity(f.rail.len());
        {
            let mut y = search_r[3] + 10.0 * u;
            for row in &f.rail {
                if row.sep {
                    y += 10.0 * u;
                }
                rail_r.push([rail[0] + 6.0 * u, y, rail[2] - 6.0 * u, y + rh]);
                y += rh + 2.0 * u;
            }
        }
        let save_r = [rail[0] + pad, rail[3] - pad - save_h + lift(0), rail[2] - pad, rail[3] - pad + lift(0)];
        let bin_r = l.bin();
        let bin_r = [bin_r[0], bin_r[1] + lift(2), bin_r[2], bin_r[3] + lift(2)];
        let clip = [ga[0] - 3.0 * u, ga[1] - 3.0 * u, ga[2] + 3.0 * u, ga[3] + 3.0 * u];

        // ---- what is under the pointer
        let mut hovered: Option<usize> = None;
        if inside(search_r, m) {
            hovered = Some(SEARCH);
        }
        if inside(toggle_r, m) {
            hovered = Some(TOGGLE);
        }
        for (r, rr) in rail_r.iter().enumerate() {
            if inside(*rr, m) {
                hovered = Some(RAIL + r);
            }
        }
        if inside(bin_r, m) {
            hovered = Some(BIN);
        }
        if f.save_next >= 0 && inside(save_r, m) {
            hovered = Some(SAVE);
        }
        for idx in 0..f.slots.len() {
            if let Some(r) = l.place(f.tab, idx) {
                if inside([r[0] - gap / 2.0, r[1] - gap / 2.0, r[2] + gap / 2.0, r[3] + gap / 2.0], m) {
                    hovered = Some(idx);
                }
            }
        }
        // (a cell's box takes half the gap round it: no place between two where nothing is pointed at)
        if grid_on && inside(clip, m) {
            let lx = m[0] - ga[0] + gap / 2.0;
            let ly = m[1] - ga[1] + scroll + gap / 2.0;
            if lx >= 0.0 && ly >= 0.0 {
                let (col, row) = ((lx / pitch) as usize, (ly / pitch) as usize);
                if col < COLS && row * COLS + col < f.n {
                    hovered = Some(GRID + row * COLS + col);
                }
            }
        }

        // (a block held: nothing under the pointer is pointed at)
        if self.hud.inv.drag.is_some() {
            hovered = None;
        }

        // ---- the rail: the search, the rows, the saving of the set
        let mut boxes: Vec<(usize, [f32; 4])> = vec![(SEARCH, search_r)];
        // (the rail as icons: the name of the one pointed at, and where)
        let mut names_tip: Option<(String, f32)> = None;
        let mut icons: Vec<[f32; 5]> = Vec::new();
        c.alpha = seen(0);
        c.rect(search_r, 9.0 * u, cell_col);
        // (its keys taken: a breath lighter)
        let fa = self.an(ak(A_CR, 1), if f.focused { 1.0 } else { 0.0 }, 9.0, 0.0);
        if fa > 0.01 {
            c.rect(search_r, 9.0 * u, grey(1.0, 0.05 * fa));
        }
        let smid = (search_r[1] + search_r[3]) / 2.0;
        let gx = search_r[0] + 10.0 * u + ((search_r[0] + search_r[2]) / 2.0 - 7.0 * u - search_r[0] - 10.0 * u) * ik;
        c.icon([gx, smid - 7.0 * u, gx + 14.0 * u, smid + 7.0 * u], 13, if f.focused || f.tab == 1 { t1 } else { t2 });
        let base_a = c.alpha;
        c.alpha = base_a * words_k;
        let tx = search_r[0] + 32.0 * u;
        let tmax = (search_r[2] - tx - 34.0 * u).max(0.0);
        let fs = 13.5 * u;
        let base = smid + fs * 0.36;
        let typed: String = f.q.chars().take(f.cursor).collect();
        let cw = fonts().width(Face::Regular, fs, &typed);
        let shift = (cw - tmax + 4.0 * u).max(0.0);
        if f.q.is_empty() {
            let ph = fonts().fit(Face::Regular, fs, tr("Search items"), (search_r[2] - tx - 10.0 * u).max(0.0));
            c.text(tx, base, fs, Face::Regular, &ph, fade(t2, 0.8));
        } else {
            c.push_clip([tx, search_r[1], tx + tmax, search_r[3]]);
            c.text(tx - shift, base, fs, Face::Regular, &f.q, t1);
            c.pop_clip();
            if f.tab == 1 {
                c.text_r(search_r[2] - 10.0 * u, base, 11.0 * u, Face::Mono, &f.n.to_string(), t2);
            }
        }
        if f.focused && (t * 1.9).fract() < 0.6 {
            let x = tx + cw - shift + 1.0 * u;
            c.rect([x, smid - 8.0 * u, x + 1.4 * u, smid + 8.0 * u], 0.7 * u, t1);
        }
        c.alpha = base_a;
        // the rail's button: « folds it to its icons, » opens it again
        let th = self.an(ak(A_CR, 41), if hovered == Some(TOGGLE) { 1.0 } else { 0.0 }, 14.0, 0.0);
        c.rect(toggle_r, 9.0 * u, if th > 0.01 { grey(1.0, 0.04 + 0.06 * th) } else { grey(1.0, 0.04) });
        {
            let tc = [(toggle_r[0] + toggle_r[2]) / 2.0, (toggle_r[1] + toggle_r[3]) / 2.0];
            // the chevrons turn over as it folds: their points left (fold), then right (open)
            let d = 1.0 - 2.0 * ik;
            let col = mix(fade(t2, 0.9), t1, th);
            for k in [-1.0f32, 1.0] {
                let x = tc[0] + k * 3.2 * u;
                c.polyline(&[[x + 2.6 * u * d, tc[1] - 4.6 * u], [x - 2.6 * u * d, tc[1]], [x + 2.6 * u * d, tc[1] + 4.6 * u]], 1.6 * u, col);
            }
        }
        boxes.push((TOGGLE, toggle_r));
        // the chosen row's light slides to it
        let on_row = f.rail.iter().position(|r| r.on);
        if let Some(k) = on_row {
            let y = self.an(ak(A_CR, 2), rail_r[k][1], 16.0, rail_r[k][1]);
            let rr = [rail_r[k][0], y, rail_r[k][2], y + rh];
            c.rect_lit(rr, 8.0 * u, grey(1.0, 0.09), grey(1.0, 0.025), 1.0);
            c.line([rr[0] + 4.0 * u, rr[1] + 9.0 * u], [rr[0] + 4.0 * u, rr[3] - 9.0 * u], 2.0 * u, look.accent, 5.0);
        }
        for (r, row) in f.rail.iter().enumerate() {
            let rr = rail_r[r];
            if row.sep {
                c.rect([rr[0] + 8.0 * u, rr[1] - 6.0 * u, rr[2] - 8.0 * u, rr[1] - 5.0 * u], 0.0, grey(1.0, 0.07));
            }
            let hov = hovered == Some(RAIL + r);
            let hv = self.an(ak(A_CR, 100 + r), if hov && !row.on { 1.0 } else { 0.0 }, 14.0, 0.0);
            if hv > 0.01 {
                c.rect(rr, 8.0 * u, grey(1.0, 0.045 * hv));
            }
            let col = if row.on { t1 } else { mix(fade(t2, 0.9), t1, hv * 0.7) };
            let mid = (rr[1] + rr[3]) / 2.0;
            let is = 20.0 * u;
            let ix = rr[0] + 12.0 * u + ((rr[0] + rr[2]) / 2.0 - is / 2.0 - rr[0] - 12.0 * u) * ik;
            let ic = [ix, mid - is / 2.0, ix + is, mid + is / 2.0];
            let icx = (ic[0] + ic[2]) / 2.0;
            let name = match row.kind {
                1 => {
                    c.text_c(icx, mid + 6.0 * u, 17.0 * u, Face::Regular, "★", col);
                    tr("Favourites").to_string()
                }
                2 => {
                    clock(&mut c, [icx, mid], 7.5 * u, col, u);
                    tr("Recent").to_string()
                }
                3 => {
                    c.icon([icx - 8.0 * u, mid - 8.0 * u, icx + 8.0 * u, mid + 8.0 * u], 18, col);
                    tr("Sets").to_string()
                }
                _ => {
                    // the tab's own item, in whole steps of its 16 px
                    let isz = ((is * 1.1) / 16.0).round().max(1.0) * 16.0;
                    icons.push([r as f32, icx - isz / 2.0, mid - isz / 2.0, isz, seen(0)]);
                    row.name.clone()
                }
            };
            if words_k > 0.01 {
                // (laid out for the whole rail; cut by the narrowing one)
                let full_r = rail[0] + 248.0 * u - 6.0 * u;
                let nx = rr[0] + 42.0 * u;
                let cnt = if row.count >= 0 { row.count.to_string() } else { String::new() };
                let cnw = if cnt.is_empty() { 0.0 } else { fonts().width(Face::Mono, 11.0 * u, &cnt) + 10.0 * u };
                let nm = fonts().fit(Face::Regular, 13.5 * u, &name, full_r - 10.0 * u - cnw - nx);
                let a0 = c.alpha;
                c.alpha = a0 * words_k;
                c.push_clip([rail[0], rail[1], rail[2] - 6.0 * u, rail[3]]);
                c.text(nx, mid + 4.8 * u, 13.5 * u, Face::Regular, &nm, col);
                if !cnt.is_empty() {
                    c.text_r(full_r - 10.0 * u, mid + 4.0 * u, 11.0 * u, Face::Mono, &cnt, fade(t2, 0.75));
                }
                c.pop_clip();
                c.alpha = a0;
            }
            // icons only: its name beside the rail while it is pointed at
            if ik > 0.5 && hov {
                names_tip = Some((name.clone(), mid));
            }
            boxes.push((RAIL + r, rr));
        }
        if ik > 0.5 && hovered == Some(SEARCH) {
            names_tip = Some((tr("Search items").to_string(), (search_r[1] + search_r[3]) / 2.0));
        }
        // the saving of the hotbar as a set (its key and place said once it is saved)
        let saved = f.saved_since >= 0.0 && f.saved_since < 1.8;
        let (label, live) = if saved {
            (tr("Set saved").to_string(), true)
        } else if f.save_next < 0 {
            (tr("All sets are taken").to_string(), false)
        } else {
            (tr("Save set").to_string(), true)
        };
        let sh = self.an(ak(A_CR, 25), if hovered == Some(SAVE) { 1.0 } else { 0.0 }, 12.0, 0.0);
        c.rect(save_r, 9.0 * u, if live { grey(1.0, 0.06 + 0.06 * sh) } else { grey(1.0, 0.03) });
        if saved {
            c.rect(save_r, 9.0 * u, grey(1.0, 0.1 * (1.0 - (f.saved_since / 1.8).clamp(0.0, 1.0))));
        }
        let scol = if live { mix(t2, t1, sh) } else { fade(t2, 0.5) };
        let smid = (save_r[1] + save_r[3]) / 2.0;
        let lw = fonts().width(Face::Medium, 12.5 * u, &label) + 22.0 * u;
        let sx = (save_r[0] + save_r[2]) / 2.0 - lw / 2.0;
        let ix = sx + ((save_r[0] + save_r[2]) / 2.0 - 7.0 * u - sx) * ik;
        c.icon([ix, smid - 7.0 * u, ix + 14.0 * u, smid + 7.0 * u], 11, scol);
        let a0 = c.alpha;
        c.alpha = a0 * words_k;
        c.text(sx + 22.0 * u, smid + 4.5 * u, 12.5 * u, Face::Medium, &label, scol);
        c.alpha = a0;
        if ik > 0.5 && hovered == Some(SAVE) {
            names_tip = Some((label.clone(), smid));
        }
        if f.save_next >= 0 {
            boxes.push((SAVE, save_r));
        }
        c.alpha = 1.0;

        // the name beside the folded rail
        let nk = self.an(ak(A_CR, 42), if names_tip.is_some() { 1.0 } else { 0.0 }, 18.0, 0.0);
        if let Some((name, y)) = names_tip.as_ref() {
            let fs = 12.5 * u;
            let tw = fonts().width(Face::Medium, fs, name) + 22.0 * u;
            let r = [rail[2] + 8.0 * u + (1.0 - nk) * 4.0 * u, y - 15.0 * u, rail[2] + 8.0 * u + tw + (1.0 - nk) * 4.0 * u, y + 15.0 * u];
            let gl = look.glass;
            top.alpha = nk;
            top.shadow(r, 15.0 * u, 16.0 * u, grey(0.0, 0.45));
            top.rect(r, 15.0 * u, [gl[0].max(0.05), gl[1].max(0.05), gl[2].max(0.06), 1.0]);
            top.text(r[0] + 11.0 * u, y + fs * 0.36, fs, Face::Medium, name, t1);
            top.alpha = 1.0;
        }

        // ---- the grid's block: the tab's name and a word under it (rising with the tab)
        let tk = ease_out(((t - switched) / 0.3).clamp(0.0, 1.0));
        c.alpha = seen(1) * tk;
        let ty = (1.0 - tk) * 6.0 * u + lift(1);
        let title = match f.tab {
            1 if !f.q.is_empty() => format!("«{}»", f.q),
            1 => tr("Search items").to_string(),
            4 => tr("Favourites").to_string(),
            5 => tr("Recent").to_string(),
            6 => tr("Sets").to_string(),
            _ => f.title.clone(),
        };
        let title = fonts().fit(Face::Semi, 16.0 * u, &title, ga[2] - ga[0] - 10.0 * u);
        c.text(ga[0], gridb[1] + pad + 18.0 * u + ty, 16.0 * u, Face::Semi, &title, t1);
        let sub = match f.tab {
            1 if !f.q.is_empty() && f.n == 0 => tr("Nothing found — try in English").to_string(),
            2 => format!("{}+1…9 — {}   ·   {}+1…9 — {}", f.keys.0, tr("save the hotbar"), f.keys.1, tr("put it back")),
            3 => tr("Armour, backpack and hotbar").to_string(),
            4 => format!("{}  ·  {}", f.n, tr("Alt+click on an item adds and removes")),
            5 => tr("What you took recently").to_string(),
            6 => tr("The whole inventory: armour, backpack, hotbar").to_string(),
            _ => items_word(f.n),
        };
        let sub = fonts().fit(Face::Mono, 11.0 * u, &sub, gridb[2] - pad - ga[0]);
        c.text(ga[0], gridb[1] + pad + 36.0 * u + ty, 11.0 * u, Face::Mono, &sub, fade(t2, 0.8));
        c.alpha = 1.0;

        // ---- the grid: the cells in sight; they come in row by row with a tab
        let mut items: Vec<(usize, [f32; 5], [f32; 4])> = Vec::new();
        let mut want = [0usize, 0usize];
        let isz_of = |side: f32| {
            let s = ((side * 0.86) / 16.0).round().max(1.0) * 16.0;
            if s > side { (s - 16.0).max(16.0) } else { s }
        };
        let mut empty_rows: Vec<(usize, f32)> = Vec::new();
        if grid_on {
            c.push_clip(clip);
            top.push_clip(clip);
            let r0 = (scroll / pitch).floor().max(0.0) as usize;
            let r1 = ((scroll + view) / pitch).ceil() as usize;
            want = [r0.saturating_sub(2) * COLS, ((r1 + 3) * COLS).min(f.n)];
            for i in (r0 * COLS)..((r1 + 1) * COLS).min(f.n) {
                let (row, col) = (i / COLS, i % COLS);
                let y = ga[1] + row as f32 * pitch - scroll + lift(1);
                let kin = ease_out(((t - switched - (row as f32 * 0.026 + col as f32 * 0.006)) / 0.34).clamp(0.0, 1.0));
                let cl = if i >= f.i0 { f.cells.get(i - f.i0).cloned().unwrap_or_default() } else { CrCell::default() };
                let r = [ga[0] + col as f32 * pitch, y + (1.0 - kin) * 8.0 * u, ga[0] + col as f32 * pitch + cell, y + (1.0 - kin) * 8.0 * u + cell];
                // softly gone at the grid's edges
                let yc = (r[1] + r[3]) / 2.0;
                let edge = ((yc - clip[1]) / (cell * 0.5)).clamp(0.0, 1.0).min(((clip[3] - yc) / (cell * 0.5)).clamp(0.0, 1.0));
                let a = seen(1) * edge * kin;
                c.alpha = a;
                // a saved hotbar not saved yet: one long empty place for its row, its words in it
                let row_empty = f.tab == 2 && (row * COLS..row * COLS + COLS).any(|k| k >= f.i0 && f.cells.get(k - f.i0).is_some_and(|x| x.locked));
                if row_empty {
                    if col == 0 {
                        let rr = [ga[0], r[1], ga[0] + COLS as f32 * pitch - gap, r[3]];
                        c.rect(rr, crad, fade(cell_col, 0.45));
                        empty_rows.push((row, a));
                    }
                    continue;
                }
                c.rect(r, crad, cell_col);
                let hv = self.an(ak(A_CR, 1000 + i % 512), if hovered == Some(GRID + i) { 1.0 } else { 0.0 }, 16.0, 0.0);
                if hv > 0.01 {
                    c.rect(r, crad, fade(hover_col, hv));
                }
                if cl.fav && f.tab != 4 {
                    c.text(r[0] + 4.0 * u, r[1] + 12.0 * u, 10.0 * u, Face::Regular, "★", fade(t1, 0.8));
                }
                if cl.locked {
                    continue;
                }
                let side = isz_of(cell);
                let ic = [(r[0] + r[2]) / 2.0 - side / 2.0, (r[1] + r[3]) / 2.0 - side / 2.0];
                let hit = [r[0] - gap / 2.0, (r[1] - gap / 2.0).max(clip[1]), r[2] + gap / 2.0, (r[3] + gap / 2.0).min(clip[3])];
                items.push((GRID + i, [ic[0], ic[1], side, (1.0 + hv * 0.08) * (0.7 + 0.3 * kin), a], hit));
                if cl.count > 1 {
                    top.alpha = a;
                    let fs = 12.0 * u;
                    top.text_r(r[2] - 3.0 * u + 1.0 * u, r[3] - 4.0 * u + 1.0 * u, fs, Face::Semi, &cl.count.to_string(), grey(0.0, 0.75));
                    top.text_r(r[2] - 3.0 * u, r[3] - 4.0 * u, fs, Face::Semi, &cl.count.to_string(), grey(1.0, 0.96));
                }
            }
            for (row, a) in &empty_rows {
                let y = ga[1] + *row as f32 * pitch - scroll + lift(1) + cell / 2.0 + 4.0 * u;
                c.alpha = *a;
                let s = format!("{}  ·  {}+{} {}", tr("empty"), f.keys.0, row + 1, tr("saves here"));
                c.text_c((ga[0] + ga[2]) / 2.0, y, 11.0 * u, Face::Mono, &s, fade(t2, 0.5));
            }
            c.pop_clip();
            top.pop_clip();
            c.alpha = 1.0;
            top.alpha = 1.0;
            // the saved hotbars' numbers beside their rows
            if f.tab == 2 {
                for row in 0..rows {
                    let y = ga[1] + row as f32 * pitch - scroll + cell / 2.0 + 4.0 * u;
                    if y > ga[1] && y < ga[3] {
                        c.text(ga[2] + 8.0 * u, y, 11.0 * u, Face::Mono, &(row + 1).to_string(), fade(t2, 0.7));
                    }
                }
            }
            // the scroll's bar (in the gutter)
            if max > 0.0 {
                let bx = gridb[2] - pad - 4.0 * u;
                c.rect([bx, ga[1], bx + 3.0 * u, ga[3]], 1.5 * u, grey(1.0, 0.06));
                let vis = (view / (content - gap)).min(1.0);
                let y0 = ga[1] + scroll / max * (1.0 - vis) * view;
                c.rect([bx, y0, bx + 3.0 * u, y0 + vis * view], 1.5 * u, grey(1.0, 0.4));
            }
        }

        // ---- the sets: a card to save the inventory as one, then each set's card — its name (typed in place), its buttons
        // (put on; write over, delete: a second click), its hotbar, its armour and the other hand, how much more it holds
        if f.tab == 6 {
            c.push_clip(clip);
            top.push_clip(clip);
            let kin = ease_out(((t - switched) / 0.3).clamp(0.0, 1.0));
            let mut y = ga[1] - scroll + lift(1) + (1.0 - kin) * 8.0 * u;
            let a = seen(1) * kin;
            let cut = |r: [f32; 4]| [r[0], r[1].max(clip[1]), r[2], r[3].min(clip[3])];
            let seen_r = |r: [f32; 4]| r[3] > clip[1] && r[1] < clip[3];
            // the new one
            let nr = [ga[0], y, ga[2], y + kit_new_h];
            let nh = self.an(ak(A_CR, 50), if inside(cut(nr), m) && inside(clip, m) { 1.0 } else { 0.0 }, 14.0, 0.0);
            c.alpha = a;
            c.rect(nr, crad * 1.3, grey(1.0, 0.045 + 0.05 * nh));
            let word = format!("+   {}", tr("Save the inventory as a set"));
            c.text_c((nr[0] + nr[2]) / 2.0, (nr[1] + nr[3]) / 2.0 + 4.6 * u, 13.0 * u, Face::Medium, &word, mix(t2, t1, nh));
            if seen_r(nr) {
                boxes.push((KIT_NEW, cut(nr)));
                if inside(cut(nr), m) {
                    hovered = Some(KIT_NEW);
                }
            }
            y += kit_new_h + 8.0 * u;
            if f.kits.is_empty() {
                // (in as many lines as it takes)
                let hint = tr("A set keeps the whole inventory: armour, backpack, hotbar and the other hand");
                let room = (ga[2] - ga[0]) * 0.86;
                let mut lines: Vec<String> = vec![String::new()];
                for word in hint.split(' ') {
                    let last = lines.last_mut().unwrap();
                    let next = if last.is_empty() { word.to_string() } else { format!("{last} {word}") };
                    if fonts().width(Face::Regular, 12.0 * u, &next) > room && !last.is_empty() {
                        lines.push(word.to_string());
                    } else {
                        *last = next;
                    }
                }
                for (i, l) in lines.iter().enumerate() {
                    c.text_c((ga[0] + ga[2]) / 2.0, y + 22.0 * u + i as f32 * 18.0 * u, 12.0 * u, Face::Regular, l, fade(t2, 0.7));
                }
            }
            for (k, kit) in f.kits.iter().enumerate() {
                let cr = [ga[0], y, ga[2], y + kit_h];
                y += kit_h + 8.0 * u;
                if !seen_r(cr) {
                    continue;
                }
                let hov = inside(cut(cr), m) && inside(clip, m);
                let hk = self.an(ak(A_CR, 600 + k % 256), if hov { 1.0 } else { 0.0 }, 14.0, 0.0);
                // (just saved or written over: a light that fades)
                let fresh = if f.saved_since >= 0.0 && f.saved_since < 1.8 && f.saved_slot == k as i32 { 1.0 - f.saved_since / 1.8 } else { 0.0 };
                c.alpha = a;
                c.rect(cr, crad * 1.3, grey(1.0, 0.035 + 0.03 * hk + 0.08 * fresh));
                // the name, or the one being typed (its caret)
                let typing = f.kit_name.as_ref().filter(|n| n.0 == k).map(|n| n.1.clone());
                let name = match &typing {
                    Some(n) => n.clone(),
                    None if kit.name.is_empty() => format!("{} {}", tr("Set"), k + 1),
                    None => kit.name.clone(),
                };
                let fs = 14.0 * u;
                let nx = cr[0] + 12.0 * u;
                let ny = cr[1] + 10.0 * u + 16.0 * u;
                // the buttons by the name's row: put on, write over, delete
                let bh = 26.0 * u;
                let by = cr[1] + 8.0 * u;
                let armed = |what: u8| f.kit_arm.is_some_and(|q| q.0 == k && q.1 == what);
                let labels = [
                    (KIT_TAKE + k, tr("Put on").to_string(), false),
                    (KIT_OVER + k, if armed(1) { tr("Sure?").to_string() } else { tr("Write over").to_string() }, armed(1)),
                    (KIT_DEL + k, if armed(2) { tr("Sure?").to_string() } else { String::new() }, armed(2)),
                ];
                let mut bx = cr[2] - 10.0 * u;
                let mut name_room = cr[2] - nx;
                for (bi, (code, label, hot)) in labels.iter().enumerate().rev() {
                    let bw = if label.is_empty() { bh } else { fonts().width(Face::Medium, 12.0 * u, label) + 20.0 * u };
                    let br = [bx - bw, by, bx, by + bh];
                    bx -= bw + 6.0 * u;
                    name_room = br[0] - 10.0 * u - nx;
                    let bhv = self.an(ak(A_CR, 900 + (k % 256) * 3 + bi), if inside(br, m) && inside(clip, m) { 1.0 } else { 0.0 }, 16.0, 0.0);
                    let red = *code >= KIT_DEL;
                    let base = if bi == 0 { 0.1 } else { 0.05 };
                    let fill = if red && (*hot || bhv > 0.01) { [1.0, 0.37, 0.34, 0.1 + 0.12 * bhv.max(*hot as i32 as f32)] } else { grey(1.0, base + 0.08 * bhv) };
                    c.alpha = a * (0.55 + 0.45 * hk.max(bhv).max(*hot as i32 as f32));
                    c.rect(br, bh / 2.0, fill);
                    let tc = if red { [1.0, 0.62, 0.58, 1.0] } else if *hot { t1 } else { mix(t2, t1, bhv.max(if bi == 0 { 0.6 } else { 0.0 })) };
                    if label.is_empty() {
                        bin(&mut c, [(br[0] + br[2]) / 2.0, (br[1] + br[3]) / 2.0], 6.5 * u, tc);
                    } else {
                        c.text_c((br[0] + br[2]) / 2.0, (br[1] + br[3]) / 2.0 + 4.4 * u, 12.0 * u, Face::Medium, label, tc);
                    }
                    if seen_r(br) {
                        boxes.push((*code, cut(br)));
                        if inside(cut(br), m) && inside(clip, m) {
                            hovered = Some(*code);
                        }
                    }
                }
                c.alpha = a;
                let shown = fonts().fit(Face::Semi, fs, &name, name_room.max(20.0 * u));
                let nw = c.text(nx, ny, fs, Face::Semi, &shown, if typing.is_some() { t1 } else { mix(fade(t1, 0.92), t1, hk) });
                if typing.is_some() && (t * 1.9).fract() < 0.6 {
                    let cx = nx + fonts().width(Face::Semi, fs, &shown) + 2.0 * u;
                    c.rect([cx, ny - fs * 0.8, cx + 1.4 * u, ny + fs * 0.15], 0.7 * u, t1);
                }
                let _ = nw;
                let name_r = [nx - 4.0 * u, cr[1] + 6.0 * u, (nx + fonts().width(Face::Semi, fs, &shown) + 12.0 * u).min(cr[0] + name_room + 12.0 * u), cr[1] + 34.0 * u];
                if seen_r(name_r) {
                    boxes.push((KIT_NAME + k, cut(name_r)));
                }
                // its hotbar (the player's menu's 36…44)
                let hy = cr[1] + 10.0 * u + 22.0 * u + 8.0 * u;
                let side = ((kit_cell * 0.86) / 16.0).round().max(1.0) * 16.0;
                let side = if side > kit_cell { (side - 16.0).max(16.0) } else { side };
                for j in 0..9usize {
                    let x = cr[0] + 12.0 * u + j as f32 * (kit_cell + gap);
                    let r = [x, hy, x + kit_cell, hy + kit_cell];
                    c.rect(r, crad, cell_col);
                    let slot = 36 + j;
                    let n = kit.at(slot);
                    if n > 0 && seen_r(r) {
                        let code = KIT + k * 64 + slot;
                        let ih = self.an(ak(A_CR, 2000 + (k % 64) * 46 + slot), if hovered == Some(code) { 1.0 } else { 0.0 }, 16.0, 0.0);
                        if ih > 0.01 {
                            c.rect(r, crad, fade(hover_col, ih));
                        }
                        let hit = cut([r[0] - gap / 2.0, r[1] - gap / 2.0, r[2] + gap / 2.0, r[3] + gap / 2.0]);
                        items.push((code, [(r[0] + r[2]) / 2.0 - side / 2.0, (r[1] + r[3]) / 2.0 - side / 2.0, side, 1.0 + 0.08 * ih, a], hit));
                        if inside(hit, m) && inside(clip, m) {
                            hovered = Some(code);
                        }
                        if n > 1 {
                            top.alpha = a;
                            let fs2 = 11.5 * u;
                            top.text_r(r[2] - 2.0 * u, r[3] - 3.0 * u, fs2, Face::Semi, &n.to_string(), grey(0.0, 0.75));
                            top.text_r(r[2] - 3.0 * u, r[3] - 4.0 * u, fs2, Face::Semi, &n.to_string(), grey(1.0, 0.96));
                        }
                    }
                }
                // its armour and the other hand, small; how much more it holds
                let iy = hy + kit_cell + 8.0 * u;
                let ms = 24.0 * u;
                let mside = ((ms * 0.86) / 16.0).floor().max(1.0) * 16.0;
                let mut x = cr[0] + 12.0 * u;
                for (q, slot) in [5usize, 6, 7, 8, 45].into_iter().enumerate() {
                    let r = [x, iy, x + ms, iy + ms];
                    x += ms + 4.0 * u + if q == 3 { 8.0 * u } else { 0.0 };
                    c.rect(r, crad * 0.7, fade(cell_col, 0.8));
                    if kit.at(slot) > 0 && seen_r(r) {
                        let code = KIT + k * 64 + slot;
                        let hit = cut(r);
                        items.push((code, [(r[0] + r[2]) / 2.0 - mside / 2.0, (r[1] + r[3]) / 2.0 - mside / 2.0, mside, 1.0, a], hit));
                        if inside(hit, m) && inside(clip, m) {
                            hovered = Some(code);
                        }
                    } else {
                        let kind = if slot == 45 { 54 } else { 50 + (slot - 5) as u32 };
                        let q2 = ms * 0.24;
                        c.icon([r[0] + q2, r[1] + q2, r[2] - q2, r[3] - q2], kind, grey(1.0, 0.14));
                    }
                }
                let more: usize = (9..=35).filter(|s| kit.at(*s) > 0).count();
                if more > 0 {
                    let w2 = format!("+ {} {}", more, tr("in the backpack"));
                    c.text(x + 8.0 * u, iy + ms / 2.0 + 4.0 * u, 11.0 * u, Face::Mono, &w2, fade(t2, 0.75));
                }
            }
            c.alpha = 1.0;
            top.alpha = 1.0;
            c.pop_clip();
            top.pop_clip();
            // the scroll's bar (in the gutter)
            if max > 0.0 {
                let bx = gridb[2] - pad - 4.0 * u;
                c.rect([bx, ga[1], bx + 3.0 * u, ga[3]], 1.5 * u, grey(1.0, 0.06));
                let vis = (view / content.max(1.0)).min(1.0);
                let y0 = ga[1] + scroll / max * (1.0 - vis) * view;
                c.rect([bx, y0, bx + 3.0 * u, y0 + vis * view], 1.5 * u, grey(1.0, 0.4));
            }
        }

        // ---- the survival inventory's tab: the player beside the armour
        let mut model = [0.0f32; 11];
        if f.tab == 3 {
            let st = [ga[0] + 1.3 * pitch, ga[1] + lift(1), ga[2] - 1.3 * pitch, ga[1] + (vis_rows as f32 - 3.5) * pitch + lift(1)];
            let scx = (st[0] + st[2]) / 2.0;
            c.glow([scx, st[3] - 6.0 * u], (st[2] - st[0]) * 0.3, grey(1.0, 0.05));
            let head_x = ((m[0] - scx) / (300.0 * u)).atan().to_degrees().clamp(-45.0, 45.0) * 0.8;
            let head_y = ((m[1] - st[1] - 30.0 * u) / (300.0 * u)).atan().to_degrees().clamp(-30.0, 30.0) * 0.7;
            let hx = self.an(ak(A_CR, 20), -head_x, 6.0, 0.0);
            let hy = self.an(ak(A_CR, 21), head_y, 6.0, 0.0);
            model = [st[0], st[1], st[2], st[3], (st[3] - st[1]) * 0.36, 0.95, -24.0, 0.0, hx, hy, 0.0];
        }

        // ---- the slots: the hotbar (and the survival inventory's), the same item elsewhere a breath lighter
        let hov_item = hovered.and_then(|hv| f.slots.get(hv)).map(|s| s.item).filter(|v| *v != 0);
        let hot_base = if f.tab == 3 { 36 } else { 45 };
        for (idx, sl) in f.slots.iter().enumerate() {
            let Some(r) = l.place(f.tab, idx) else { continue };
            let bi = if (hot_base..hot_base + 9).contains(&idx) { 2 } else { 1 };
            let r = [r[0], r[1] + lift(bi), r[2], r[3] + lift(bi)];
            c.alpha = seen(bi);
            c.rect(r, crad, cell_col);
            if idx >= hot_base && idx < hot_base + 9 && idx - hot_base == f.sel {
                lamp(&mut c, r, crad, ring_col);
            }
            if hov_item.is_some() && Some(sl.item) == hov_item && Some(idx) != hovered && sl.count > 0 {
                c.rect(r, crad, grey(1.0, 0.05));
            }
            if sl.drag {
                c.rect(r, crad, grey(1.0, 0.12));
            }
            let hv = self.an(ak(A_CR, 300 + idx), if hovered == Some(idx) { 1.0 } else { 0.0 }, 16.0, 0.0);
            if hv > 0.01 {
                c.rect(r, crad, fade(hover_col, hv));
            }
            // what goes in an empty one: armour, the other hand
            if sl.count == 0 && f.tab == 3 {
                let kind = match idx {
                    5..=8 => Some(50 + (idx - 5) as u32),
                    45 => Some(54),
                    _ => None,
                };
                if let Some(k) = kind {
                    let q = cell * 0.27;
                    c.icon([r[0] + q, r[1] + q, r[2] - q, r[3] - q], k, grey(1.0, 0.16));
                }
            }
            let side = isz_of(cell);
            let ic = [(r[0] + r[2]) / 2.0 - side / 2.0, (r[1] + r[3]) / 2.0 - side / 2.0];
            items.push((idx, [ic[0], ic[1], side, 1.0 + hv * 0.08, seen(bi)], [r[0] - gap / 2.0, r[1] - gap / 2.0, r[2] + gap / 2.0, r[3] + gap / 2.0]));
            top.alpha = seen(bi);
            if sl.count > 1 {
                let fs = 12.0 * u;
                top.text_r(r[2] - 2.0 * u, r[3] - 3.0 * u, fs, Face::Semi, &sl.count.to_string(), grey(0.0, 0.75));
                top.text_r(r[2] - 3.0 * u, r[3] - 4.0 * u, fs, Face::Semi, &sl.count.to_string(), grey(1.0, 0.96));
            }
            if sl.count > 0 && sl.max > 0.0 && sl.dmg > 0.0 {
                let fr = (1.0 - sl.dmg / sl.max).clamp(0.0, 1.0);
                let b = [r[0] + 5.0 * u, r[3] - 5.0 * u, r[2] - 5.0 * u, r[3] - 5.0 * u + 2.4 * u];
                top.rect(b, 1.2 * u, grey(0.0, 0.7));
                top.rect([b[0], b[1], b[0] + (b[2] - b[0]) * fr, b[3]], 1.2 * u, wear(fr));
            }
        }
        c.alpha = 1.0;
        top.alpha = 1.0;
        // the hotbar's numbers under its cells: which key takes an item there
        c.alpha = seen(2);
        for col in 0..9usize {
            let r = l.place(f.tab, hot_base + col).unwrap_or([0.0; 4]);
            c.text_c((r[0] + r[2]) / 2.0, hotb[3] + lift(2) + 13.0 * u, 10.0 * u, Face::Mono, &(col + 1).to_string(), if col == f.sel { fade(t1, 0.8) } else { fade(t2, 0.45) });
        }

        // ---- the bin (it swells when something goes in)
        let bh = self.an(ak(A_CR, 24), if hovered == Some(BIN) { 1.0 } else { 0.0 }, 12.0, 0.0);
        let pulse = if f.bin_since >= 0.0 && f.bin_since < 0.26 { 1.0 + 0.15 * (1.0 - ease_out(f.bin_since / 0.26)) } else { 1.0 };
        let bc = [(bin_r[0] + bin_r[2]) / 2.0, (bin_r[1] + bin_r[3]) / 2.0];
        let bs = (bin_r[2] - bin_r[0]) / 2.0 * pulse;
        // (Shift over it: it empties everything — redder)
        let all = self.an(ak(A_CR, 26), if hovered == Some(BIN) && f.shift { 1.0 } else { 0.0 }, 14.0, 0.0);
        c.rect([bc[0] - bs, bc[1] - bs, bc[0] + bs, bc[1] + bs], crad * pulse, [1.0, 0.37, 0.34, 0.06 + 0.1 * bh + 0.12 * all]);
        bin(&mut c, bc, cell * 0.2 * pulse, [1.0, 0.55, 0.51, 0.7 + 0.3 * bh]);
        boxes.push((BIN, bin_r));
        // what it does, over it while pointed at
        if bh > 0.01 {
            let word = if f.shift { tr("Empty the inventory") } else if f.from_kit && f.carried > 0 { tr("Take it out of the set") } else { tr("Shift — empty everything") };
            let fs = 11.5 * u;
            let tw = fonts().width(Face::Medium, fs, word) + 20.0 * u;
            let tx = (bc[0] - tw / 2.0).min(w - 8.0 * u - tw);
            let tr_ = [tx, bin_r[1] - 10.0 * u - 26.0 * u + (1.0 - bh) * 4.0 * u, tx + tw, bin_r[1] - 10.0 * u + (1.0 - bh) * 4.0 * u];
            top.alpha = bh;
            let gl = look.glass;
            top.shadow(tr_, 13.0 * u, 16.0 * u, grey(0.0, 0.45));
            top.rect(tr_, 13.0 * u, [gl[0].max(0.05), gl[1].max(0.05), gl[2].max(0.06), 1.0]);
            top.text_c((tr_[0] + tr_[2]) / 2.0, (tr_[1] + tr_[3]) / 2.0 + fs * 0.36, fs, Face::Medium, word, if f.shift { [1.0, 0.62, 0.58, 1.0] } else { t1 });
            top.alpha = 1.0;
        }
        c.alpha = 1.0;

        // ---- the inventory emptied: its undo under the hotbar for a while, the time left a line along its foot
        let left = if f.cleared >= 0.0 && f.cleared < UNDO_FOR { 1.0 - f.cleared / UNDO_FOR } else { 0.0 };
        let uk = self.an(ak(A_CR, 27), if left > 0.0 { 1.0 } else { 0.0 }, 12.0, 0.0);
        if uk > 0.01 {
            let fs = 12.5 * u;
            let word = if f.cleared_kind == 1 { tr("Set put on") } else { tr("Inventory emptied") };
            let back = tr("Put it back");
            let ww = fonts().width(Face::Regular, fs, word);
            let bw2 = fonts().width(Face::Medium, fs, back) + 24.0 * u;
            let th = 38.0 * u;
            let tw = 18.0 * u + ww + 14.0 * u + bw2 + 6.0 * u;
            let cx = (hotb[0] + hotb[2]) / 2.0;
            let y = hotb[3] + lift(2) + 26.0 * u + (1.0 - uk) * 8.0 * u;
            let tr_ = [cx - tw / 2.0, y, cx + tw / 2.0, y + th];
            let br = [tr_[2] - 6.0 * u - bw2, tr_[1] + 6.0 * u, tr_[2] - 6.0 * u, tr_[3] - 6.0 * u];
            let bhv = self.an(ak(A_CR, 28), if inside(br, m) { 1.0 } else { 0.0 }, 16.0, 0.0);
            top.alpha = uk;
            let gl = look.glass;
            top.shadow(tr_, th / 2.0, 18.0 * u, grey(0.0, 0.5));
            top.rect(tr_, th / 2.0, [gl[0].max(0.05), gl[1].max(0.05), gl[2].max(0.06), 1.0]);
            top.text(tr_[0] + 18.0 * u, (tr_[1] + tr_[3]) / 2.0 + fs * 0.36, fs, Face::Regular, word, t2);
            top.rect(br, (br[3] - br[1]) / 2.0, grey(1.0, 0.1 + 0.1 * bhv));
            top.text_c((br[0] + br[2]) / 2.0, (br[1] + br[3]) / 2.0 + fs * 0.36, fs, Face::Medium, back, t1);
            // the time left
            let lw = (tw - th) * left;
            top.rect([cx - lw / 2.0, tr_[3] - 2.5 * u, cx + lw / 2.0, tr_[3] - 1.0 * u], 0.75 * u, grey(1.0, 0.35));
            top.alpha = 1.0;
            if left > 0.0 {
                boxes.push((UNDO, br));
                if inside(br, m) {
                    hovered = Some(UNDO);
                }
            }
        }

        // ---- the tooltip: the item pointed at, as a card of the HUD's (its name in its rarity's colour, its lines);
        // under them what the game keeps to itself: how worn it is, how much it feeds, what a box holds
        let mut boxed: Vec<[f32; 4]> = Vec::new();
        let pointed = hovered.is_some_and(|hv| (GRID..RAIL).contains(&hv) || (KIT..KIT_TAKE).contains(&hv) || f.slots.get(hv).is_some_and(|s| s.count > 0));
        if let (true, Some(ins), 0) = (pointed, &f.insp, f.carried) {
            let fs1 = 14.0 * u;
            let fsl = 12.5 * u;
            let fsm = 11.0 * u;
            let lh = fsl * 1.5;
            let pd = 12.0 * u;
            let lines: Vec<&(u32, String)> = ins.lines.iter().take(14).collect();
            let wear_row = ins.max > 0.0;
            let food = ins.stats[5] >= 0.0;
            let food_s = if food { format!("{} {}   ·   {} {}", tr("Food"), fmt1(ins.stats[5]), tr("Saturation"), fmt1(ins.stats[6].max(0.0))) } else { String::new() };
            let has_box = ins.boxed.len() == 27;
            let bc = 22.0 * u;
            let mut tw = fonts().width(Face::Semi, fs1, &ins.name);
            for (_, l2) in &lines {
                tw = tw.max(fonts().width(Face::Regular, fsl, l2));
            }
            if food {
                tw = tw.max(fonts().width(Face::Mono, fsm, &food_s));
            }
            if wear_row {
                tw = tw.max(170.0 * u);
            }
            if has_box {
                tw = tw.max(9.0 * (bc + 2.0 * u) - 2.0 * u);
            }
            let tw = tw.min(440.0 * u);
            let b0 = pd + fs1 * 0.8;
            let mut last = b0 + if lines.is_empty() { 0.0 } else { 6.0 * u + lines.len() as f32 * lh };
            let extra_at = last;
            if wear_row {
                last += 18.0 * u;
            }
            if food {
                last += 18.0 * u;
            }
            let box_at = last + 10.0 * u;
            if has_box {
                last += 10.0 * u + 3.0 * (bc + 2.0 * u);
            }
            let (cw2, ch2) = (tw + 2.0 * pd, last + fsl * 0.32 + pd);
            let mut x = m[0] + 16.0 * u;
            if x + cw2 > w - 8.0 * u {
                x = m[0] - 16.0 * u - cw2;
            }
            let y = (m[1] - 10.0 * u).clamp(8.0 * u, (h - ch2 - 8.0 * u).max(8.0 * u));
            let kt = self.an(ak(A_CR, 31), 1.0, 20.0, 0.0);
            top.alpha = kt;
            let r = [x, y, x + cw2, y + ch2];
            top.shadow(r, 10.0 * u, 22.0 * u, grey(0.0, 0.5));
            let gl = look.glass;
            top.rect(r, (brad * 0.8).max(6.0 * u), [gl[0].max(0.05), gl[1].max(0.05), gl[2].max(0.06), 1.0]);
            top.push_clip([r[0] + pd, r[1], r[2] - pd + 1.0, r[3]]);
            top.text(x + pd, y + b0, fs1, Face::Semi, &ins.name, rgb(if ins.rar == 0 { 0xffffff } else { ins.rar }, 1.0));
            for (k, (col, l2)) in lines.iter().enumerate() {
                top.text(x + pd, y + b0 + 6.0 * u + (k + 1) as f32 * lh, fsl, Face::Regular, l2, super::hudtext::readable(Some(*col), t2));
            }
            let mut ey = y + extra_at;
            if wear_row {
                ey += 18.0 * u;
                let fr = (1.0 - ins.dmg / ins.max).clamp(0.0, 1.0);
                let nums = format!("{} / {}", (ins.max - ins.dmg).round() as i32, ins.max.round() as i32);
                let nw = fonts().width(Face::Mono, fsm, &nums);
                let b = [x + pd, ey - 5.0 * u, x + cw2 - pd - nw - 10.0 * u, ey - 2.0 * u];
                top.rect(b, 1.5 * u, grey(1.0, 0.1));
                top.rect([b[0], b[1], b[0] + (b[2] - b[0]) * fr, b[3]], 1.5 * u, wear(fr));
                top.text_r(x + cw2 - pd, ey, fsm, Face::Mono, &nums, fade(t2, 0.9));
            }
            if food {
                ey += 18.0 * u;
                top.text(x + pd, ey, fsm, Face::Mono, &food_s, [0.95, 0.78, 0.5, 0.9]);
            }
            if has_box {
                for (k, n) in ins.boxed.iter().enumerate() {
                    let q = [x + pd + (k % 9) as f32 * (bc + 2.0 * u), y + box_at + (k / 9) as f32 * (bc + 2.0 * u)];
                    top.rect([q[0], q[1], q[0] + bc, q[1] + bc], 4.0 * u, cell_col);
                    if *n > 0 {
                        boxed.push([k as f32, q[0] + 3.0 * u, q[1] + 3.0 * u, bc - 6.0 * u]);
                    }
                }
            }
            top.pop_clip();
            top.alpha = 1.0;
        } else {
            self.anim.insert(ak(A_CR, 31), 0.0);
        }

        let mut out: Vec<f32> = Vec::with_capacity(4 + items.len() * 10 + boxes.len() * 5 + 64);
        out.push(hovered.map_or(-1.0, |hv| hv as f32));
        out.push(items.len() as f32);
        for (code, ic, r) in &items {
            rects_out(&mut out, *code, *ic, *r);
        }
        out.push(boxes.len() as f32);
        for (code, r) in &boxes {
            out.push(*code as f32);
            out.extend_from_slice(r);
        }
        out.push(icons.len() as f32);
        for i in &icons {
            out.extend_from_slice(i);
        }
        out.extend_from_slice(&[0.0f32; 7]);
        out.extend_from_slice(&model);
        out.push(boxed.len() as f32);
        for b in &boxed {
            out.extend_from_slice(b);
        }
        out.push(cell * 0.9);
        out.push(want[0] as f32);
        out.push(want[1] as f32);
        out.extend_from_slice(&clip);
        out.extend_from_slice(&[0.0, h]);
        // the blocks (a stack let go off them is thrown)
        for r in [rail, gridb, [hotb[0], hotb[1], hotb[2], hotb[3] + 20.0 * u]] {
            out.extend_from_slice(&r);
        }
        // the carried stack's count
        if f.carried > 1 {
            let fs = 12.0 * u;
            let (x, y) = (m[0] + cell * 0.45 - 1.0 * u, m[1] + cell * 0.45);
            top.text_r(x + 1.0 * u, y + 1.0 * u, fs, Face::Semi, &f.carried.to_string(), grey(0.0, 0.75));
            top.text_r(x, y, fs, Face::Semi, &f.carried.to_string(), grey(1.0, 0.96));
        }
        // ---- the headers' handles; the right button on a block's empty place opens the settings by it; the settings'
        // panel over all; whose the clicks are, for Java
        let empty = if hovered.is_none() { [rail, gridb, hotb].into_iter().find(|r| inside(*r, m)) } else { None };
        let eats = self.iu_after(&mut top, m, u, w, h, look.accent, &heads(rail, gridb, hotb), empty, hovered.is_some());
        self.iu_panel(&mut top, w, h);
        out.extend_from_slice(&eats.flat());
        (c, top, out)
    }
}

/// The grid's width (its nine cells).
fn ga_w(pitch: f32, gap: f32) -> f32 {
    COLS as f32 * pitch - gap
}

/// "N items" in the game's language.
fn items_word(n: usize) -> String {
    use crate::gui::i18n::{lang, Lang};
    let (m10, m100) = (n % 10, n % 100);
    let one = m10 == 1 && m100 != 11;
    let few = (2..=4).contains(&m10) && !(12..=14).contains(&m100);
    match lang() {
        Lang::Ru => format!("{n} {}", if one { "предмет" } else if few { "предмета" } else { "предметов" }),
        Lang::Uk => format!("{n} {}", if one { "предмет" } else if few { "предмети" } else { "предметів" }),
        Lang::Kk => format!("{n} зат"),
        Lang::En => format!("{n} {}", if n == 1 { "item" } else { "items" }),
    }
}

/// A clock's face (the recently taken).
fn clock(c: &mut Canvas, at: [f32; 2], r: f32, col: Rgba, u: f32) {
    c.ring(at, r, 1.6 * u, col);
    c.line(at, [at[0], at[1] - r * 0.55], 1.6 * u, col, 0.0);
    c.line(at, [at[0] + r * 0.45, at[1] + r * 0.2], 1.6 * u, col, 0.0);
}

/// A bin: its lid with the handle, its body narrowing down, two lines on it.
fn bin(c: &mut Canvas, at: [f32; 2], s: f32, col: Rgba) {
    let k = s / 11.0;
    let p = |x: f32, y: f32| [at[0] + x * k, at[1] + y * k];
    let lw = 1.6 * k;
    c.line(p(-9.0, -6.0), p(9.0, -6.0), lw, col, 0.0);
    c.polyline(&[p(-3.0, -6.0), p(-3.0, -9.0), p(3.0, -9.0), p(3.0, -6.0)], lw, col);
    c.polyline(&[p(-7.0, -6.0), p(-6.0, 9.0), p(6.0, 9.0), p(7.0, -6.0)], lw, col);
    c.line(p(-2.5, -2.0), p(-2.5, 5.5), lw, col, 0.0);
    c.line(p(2.5, -2.0), p(2.5, 5.5), lw, col, 0.0);
}
