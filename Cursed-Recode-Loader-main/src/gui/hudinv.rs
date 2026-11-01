//! Custom Inventory (Interface → Custom Inventory): the survival inventory as a character screen, all of it the
//! client's. The player big in the middle in a shaft of light (Java draws the model where and how this says: the
//! camera flies to what a slot is about, armour and weapons are tried on as they are pointed at); the equipment on
//! the left with names, enchantments and wear, the experience, the effects as rings; the item pointed at turned in
//! the inspection on the right (Java draws it), its words and numbers beside it; the backpack under it. Java's
//! HudInv sends what the game knows (InvF) and draws the items, the model and the carried stack where the frame's
//! `out` says; the game's own menu does the clicks (its slots are where this frame put them).
//!
//! Laid out on a 1920 x 1080 picture fitted into the window (`u` window px to its px).

use super::*;
use std::sync::Mutex;

/// A slot of the menu as Java has it.
#[derive(Clone, Default)]
pub struct InvSlot {
    pub count: u32,
    pub dmg: f32,
    pub max: f32,
    /// its name's colour (rarity, or the server's own), its picture's colour
    pub rar: u32,
    pub col: u32,
    /// 0 none, 1..4 armour (head, chest, legs, feet) that can be tried on, 5 held (a weapon, a tool…), 6 other
    pub kind: u8,
    pub glint: bool,
    /// in the drag that spreads a stack
    pub drag: bool,
    /// which item (the same number: the same item)
    pub item: i64,
}

/// The inspected item's words and numbers.
#[derive(Clone, Default)]
pub struct Insp {
    pub name: String,
    pub rar: u32,
    pub kind: String,
    pub count: u32,
    pub dmg: f32,
    pub max: f32,
    pub lines: Vec<(u32, String)>,
    /// damage, attack speed, armour, toughness, knockback resistance, food, saturation (below 0: none)
    pub stats: [f32; 7],
    /// a box's contents: 27 counts (0 empty)
    pub boxed: Vec<u32>,
}

/// What Java sends each frame the inventory is open.
#[derive(Clone, Default)]
pub struct InvF {
    pub gs: f32,
    pub since: f32,
    pub mouse: [f32; 2],
    pub lmb: bool,
    pub sel: usize,
    pub carried: u32,
    pub hp: f32,
    pub hp_max: f32,
    pub absorb: f32,
    pub armor: f32,
    pub tough: f32,
    pub level: u32,
    pub xp: f32,
    pub xp_need: u32,
    pub closing: f32,
    pub slots: Vec<InvSlot>,
    /// (name, ticks left, level, ticks it had when first seen)
    pub effects: Vec<(String, f32, u32, f32)>,
    pub name: String,
    /// the equipment's rows (head, chest, legs, feet, the other hand): name, the line under it
    pub eq: [(String, String); 5],
    pub insp: Option<Insp>,
}

impl InvF {
    /// nums: [gs, since, mouse x, y, lmb, sel, carried, hp, hp max, absorb, armour, toughness, level, xp, xp needed,
    /// closing, slots N, then 10 a slot, effects M, then 3 an effect, inspected (0/1) and its 12 numbers + 27 counts];
    /// text: lines "key\tvalue…" (name, eq0..eq4, fx, in, il).
    pub fn parse(nums: &[f64], text: &str) -> InvF {
        let g = |i: usize| nums.get(i).copied().unwrap_or(0.0);
        let mut f = InvF {
            gs: (g(0) as f32).max(1.0),
            since: g(1) as f32,
            mouse: [g(2) as f32, g(3) as f32],
            lmb: g(4) > 0.5,
            sel: (g(5).max(0.0) as usize).min(8),
            carried: g(6).max(0.0) as u32,
            hp: g(7) as f32,
            hp_max: (g(8) as f32).max(1.0),
            absorb: g(9) as f32,
            armor: g(10) as f32,
            tough: g(11) as f32,
            level: g(12).max(0.0) as u32,
            xp: (g(13) as f32).clamp(0.0, 1.0),
            xp_need: g(14).max(0.0) as u32,
            closing: g(15) as f32,
            ..Default::default()
        };
        let n = (g(16).max(0.0) as usize).min(128);
        let mut i = 17;
        for _ in 0..n {
            f.slots.push(read_slot(nums, i));
            i += 10;
        }
        let m = (g(i).max(0.0) as usize).min(32);
        i += 1;
        let mut fx: Vec<(f32, u32, f32)> = Vec::new();
        for _ in 0..m {
            fx.push((g(i) as f32, g(i + 1).max(0.0) as u32, g(i + 2) as f32));
            i += 3;
        }
        let (has, mut insp, _) = read_insp(nums, i);
        let mut names: Vec<String> = Vec::new();
        for line in text.lines() {
            let mut p = line.split('\t');
            let key = p.next().unwrap_or("");
            let a = p.next().unwrap_or("").to_string();
            let b = p.next().unwrap_or("").to_string();
            match key {
                "name" => f.name = a,
                "eq" => {
                    let k = a.parse::<usize>().unwrap_or(9);
                    if k < 5 {
                        f.eq[k] = (b, p.next().unwrap_or("").to_string());
                    }
                }
                "fx" => names.push(a),
                "in" | "il" => insp_line(&mut insp, key, a, b),
                _ => {}
            }
        }
        for (k, (t, lv, all)) in fx.into_iter().enumerate() {
            f.effects.push((names.get(k).cloned().unwrap_or_default(), t, lv, all));
        }
        if has {
            f.insp = Some(insp);
        }
        f
    }
}

/// A slot's 10 numbers from `i` on: count, wear, its most, name's colour, picture's colour, kind, glint, in the drag,
/// which item.
pub(super) fn read_slot(nums: &[f64], i: usize) -> InvSlot {
    let g = |i: usize| nums.get(i).copied().unwrap_or(0.0);
    InvSlot {
        count: g(i).max(0.0) as u32,
        dmg: g(i + 1) as f32,
        max: g(i + 2) as f32,
        rar: g(i + 3) as u32,
        col: g(i + 4) as u32,
        kind: g(i + 5) as u8,
        glint: g(i + 6) > 0.5,
        drag: g(i + 7) > 0.5,
        item: g(i + 8) as i64,
    }
}

/// The inspected item's numbers from `i` on (shown 0/1, then its rarity's colour, count, wear, its most, 7 stats, a
/// box's 27 counts after a 1): whether there is one, its numbers, where the next number is.
pub(super) fn read_insp(nums: &[f64], mut i: usize) -> (bool, Insp, usize) {
    let g = |i: usize| nums.get(i).copied().unwrap_or(0.0);
    let has = g(i) > 0.5;
    i += 1;
    let mut insp = Insp::default();
    if has {
        insp.rar = g(i) as u32;
        insp.count = g(i + 1).max(0.0) as u32;
        insp.dmg = g(i + 2) as f32;
        insp.max = g(i + 3) as f32;
        for k in 0..7 {
            insp.stats[k] = g(i + 4 + k) as f32;
        }
        let boxed = g(i + 11) > 0.5;
        i += 12;
        if boxed {
            for k in 0..27 {
                insp.boxed.push(g(i + k).max(0.0) as u32);
            }
            i += 27;
        }
    }
    (has, insp, i)
}

/// A line of the inspected item's words: "in" its name and kind, "il" a line under it (its colour, its words).
pub(super) fn insp_line(insp: &mut Insp, key: &str, a: String, b: String) {
    if key == "in" {
        insp.name = a;
        insp.kind = b;
    } else {
        insp.lines.push((u32::from_str_radix(&a, 16).unwrap_or(0xA3A3A3), b));
    }
}

/// The model's turning (dragged, its inertia) between frames.
struct Turn {
    yaw: f32,
    v: f32,
    drag: Option<f32>,
    dragged_at: f32,
    was_down: bool,
    began_on_stage: bool,
    t: f32,
}

static TURN: Mutex<Turn> = Mutex::new(Turn { yaw: 0.0, v: 0.0, drag: None, dragged_at: -9.0, was_down: false, began_on_stage: false, t: 0.0 });

/// Custom Inventory's animations.
pub(super) const A_INV: u64 = 47;

/// The picture's measures (1920 x 1080 px).
const LX: f32 = 110.0;
const ROW_Y: f32 = 350.0;
const ROW_P: f32 = 84.0;
const BX: f32 = 1266.0;
const BY: f32 = 576.0;
const BP: f32 = 60.0;
const HY: f32 = 778.0;
const INS: f32 = 1262.0;
const STAGE: [f32; 4] = [520.0, 0.0, 1240.0, 1080.0];
const FLOOR: f32 = 868.0;

/// Where the camera looks for what a slot is about: px a block (on the picture), the height it centres (blocks), the
/// model's turn (degrees).
const FOCUS: [(f32, f32, f32); 7] = [(355.0, 0.92, -24.0), (650.0, 1.6, -20.0), (560.0, 1.22, -17.0), (520.0, 0.62, -23.0), (520.0, 0.2, -30.0), (520.0, 0.95, -58.0), (520.0, 0.95, 36.0)];

pub(super) fn rgb(c: u32, a: f32) -> Rgba {
    crate::gui::draw::hex(c & 0xFFFFFF, a)
}

/// A slot's place on the picture (x, y, side) by its index in the survival menu.
pub fn slot_place(idx: usize) -> Option<(f32, f32, f32, u8)> {
    Some(match idx {
        0 => (LX + 170.0, 864.0, 64.0, 0),
        1..=4 => (LX + ((idx - 1) % 2) as f32 * 58.0, 840.0 + ((idx - 1) / 2) as f32 * 58.0, 52.0, 0),
        5..=8 => (LX, ROW_Y + (idx - 5) as f32 * ROW_P, 72.0, 0),
        9..=35 => (BX + ((idx - 9) % 9) as f32 * BP, BY + ((idx - 9) / 9) as f32 * BP, 56.0, 1),
        36..=44 => (BX + (idx - 36) as f32 * BP, HY, 56.0, 1),
        45 => (LX, ROW_Y + 4.0 * ROW_P, 72.0, 0),
        _ => return None,
    })
}

impl Gui {
    /// The inventory's two pictures (under the items, over them) and what Java needs: [hovered slot, slots R then 10
    /// each (index, its item's x, y, side, how much bigger, seen, the slot's box x0 y0 x1 y1), the model (stage box x0 y0 x1 y1, px a block, centred height, turn,
    /// tilt, head turn, head tilt, try on: 0 no, 1 armour, 2 in the hand), the inspected item (shown, x, y, side, turn,
    /// tilt, seen), effects E then 5 each (index, x, y, side, seen), the box's items K then 4 each (index, x, y, side),
    /// the carried stack's side].
    pub fn inv_frame(&mut self, w: f32, h: f32, f: &InvF) -> (Canvas, Canvas, Vec<f32>) {
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
        let r = self.inv_draw(w, h, f, t);
        self.dt = menu_dt;
        r
    }

    fn inv_draw(&mut self, w: f32, h: f32, f: &InvF, t: f32) -> (Canvas, Canvas, Vec<f32>) {
        let u = (w / 1920.0).min(h / 1080.0);
        let (ox, oy) = ((w - 1920.0 * u) / 2.0, (h - 1080.0 * u) / 2.0);
        // the columns lean a little away from the pointer, the stage the other way
        let mx = ((f.mouse[0] - w / 2.0) / (w / 2.0)).clamp(-1.0, 1.0);
        let my = ((f.mouse[1] - h / 2.0) / (h / 2.0)).clamp(-1.0, 1.0);
        let lx = self.an(ak(A_INV, 0), -mx * 10.0, 6.0, 0.0) * u;
        let ly = self.an(ak(A_INV, 1), -my * 6.0, 6.0, 0.0) * u;
        let cxo = self.an(ak(A_INV, 2), mx * 5.0, 6.0, 0.0) * u;
        let cyo = self.an(ak(A_INV, 3), my * 3.0, 6.0, 0.0) * u;
        let px = |x: f32| ox + x * u;
        let py = |y: f32| oy + y * u;
        let open = ease_out((f.since / 0.34).clamp(0.0, 1.0)) * (1.0 - ease_out((f.closing / 0.24).clamp(0.0, 1.0)));
        let col_in = |d: f32| ease_out(((f.since - d) / 0.55).clamp(0.0, 1.0));
        let mut c = Canvas::new(w, h);
        let mut top = Canvas::new(w, h);
        let mut out: Vec<f32> = Vec::with_capacity(512);

        // ---- the world behind: blurred, dimmed, darker at the edges
        c.alpha = open;
        c.glass([0.0, 0.0, w, h], 0.0, [0.016, 0.018, 0.026, 0.42], 1.0, 0.0, 0.0);
        c.rect([0.0, 0.0, w, h], 0.0, grey(0.0, 0.32));
        // ---- the stage: a halo, a shaft of light from above, the floor's glow and line, dust in the light
        let scx = px(880.0) + cxo;
        let floor = py(FLOOR) + cyo;
        c.glow([scx, py(560.0) + cyo], 380.0 * u, [0.55, 0.63, 1.0, 0.11]);
        for (k, (dx, r, a)) in [(-110.0, 150.0, 0.05), (0.0, 190.0, 0.075), (120.0, 140.0, 0.045)].iter().enumerate() {
            let x = scx + dx * u;
            for j in 0..5 {
                let y = py(40.0 + j as f32 * 120.0) + cyo;
                c.glow([x + (j as f32 - 2.0) * (k as f32 - 1.0) * 6.0 * u, y], r * u * (1.0 - j as f32 * 0.08), grey(1.0, a * (1.0 - j as f32 * 0.17)));
            }
        }
        let fl = ease_out((f.since / 0.36).clamp(0.0, 1.0));
        c.glow([scx, floor], 330.0 * u, grey(1.0, 0.07 * col_in(0.18)));
        c.rect([scx - 300.0 * u, floor - 20.0 * u, scx + 300.0 * u, floor + 20.0 * u], 20.0 * u, grey(1.0, 0.035 * col_in(0.2)));
        let ring_a = col_in(0.3) * (0.7 + 0.3 * (t * 1.6).sin());
        c.rect([scx - 150.0 * u, floor - 1.0 * u, scx + 150.0 * u, floor + 1.0 * u], 1.0 * u, grey(1.0, 0.28 * ring_a));
        // the line of light across the floor as it opens
        let line_a = 1.0 - ease_out(((f.since - 0.42) / 0.5).clamp(0.0, 1.0));
        if line_a > 0.01 {
            let lw = 320.0 * u * fl;
            c.glow([scx, floor], lw * 0.6, [1.0, 0.75, 0.55, 0.18 * line_a]);
            c.rect([scx - lw, floor - 1.0 * u, scx + lw, floor + 1.0 * u], 1.0 * u, grey(1.0, 0.95 * line_a));
        }
        for k in 0..46 {
            let hsh = |n: u32| ((k as u32).wrapping_mul(2654435761).wrapping_add(n.wrapping_mul(40503)) % 1000) as f32 / 1000.0;
            let sp = 10.0 + hsh(1) * 16.0;
            let y = 860.0 - ((t * sp + hsh(2) * 860.0) % 830.0);
            let x = 880.0 + (hsh(3) - 0.5) * 430.0 + (t * 0.7 + hsh(4) * 6.0).sin() * 8.0;
            let fy = ((860.0 - y) / 120.0).clamp(0.0, 1.0) * (y / 260.0).clamp(0.0, 1.0);
            let fx = 1.0 - ((x - 880.0).abs() / 260.0).clamp(0.0, 1.0);
            let a = fy * fx * (0.25 + 0.4 * (t * 2.0 + hsh(5) * 18.0).sin().powi(2)) * col_in(0.2);
            c.glow([px(x) + cxo, py(y) + cyo], (1.2 + hsh(6) * 2.2) * u, grey(1.0, a * 0.8));
        }

        // ---- the slots: where each is, which is under the pointer
        let mut rects: Vec<(usize, [f32; 4], u8)> = Vec::new();
        for (idx, _) in f.slots.iter().enumerate() {
            if let Some((x, y, s, g)) = slot_place(idx) {
                let (dx, dy) = (lx, ly);
                let r = [px(x) + dx, py(y) + dy, px(x + s) + dx, py(y + s) + dy];
                rects.push((idx, r, g));
            }
        }
        let hovered = rects.iter().find(|(_, r, _)| inside(*r, f.mouse)).map(|(i, _, _)| *i);
        out.push(hovered.map_or(-1.0, |i| i as f32));
        let hs = hovered.and_then(|i| f.slots.get(i)).cloned().unwrap_or_default();

        // ---- left: who, experience, effects
        let la = col_in(0.18);
        c.alpha = open * la;
        let lxo = lx - (1.0 - la) * 46.0 * u;
        let name = if f.name.is_empty() { "Player" } else { &f.name };
        c.text(px(LX) + lxo, py(132.0) + ly, 46.0 * u, Face::Semi, name, grey(0.93, 1.0));
        let stats = format!("♥ {}/{}    ⛨ {} {}    ✦ {} {}", fmt1(f.hp + f.absorb), fmt1(f.hp_max), fmt1(f.armor), tr("armour"), fmt1(f.tough), tr("toughness"));
        c.text(px(LX) + lxo, py(170.0) + ly, 13.0 * u, Face::Mono, &stats, grey(0.64, 1.0));
        let lv = format!("{}", f.level);
        let lvw = c.text(px(LX) + lxo, py(212.0) + ly, 15.0 * u, Face::Semi, &lv, grey(0.93, 1.0));
        let bx0 = px(LX) + lxo + lvw + 12.0 * u;
        let bar = [bx0, py(206.0) + ly, bx0 + 300.0 * u, py(209.0) + ly];
        c.rect(bar, 1.5 * u, grey(1.0, 0.1));
        let xpk = self.an(ak(A_INV, 4), f.xp, 4.0, 0.0);
        c.rect([bar[0], bar[1], bar[0] + (bar[2] - bar[0]) * xpk, bar[3]], 1.5 * u, [0.62, 0.88, 0.48, 1.0]);
        c.glow([bar[0] + (bar[2] - bar[0]) * xpk, (bar[1] + bar[3]) / 2.0], 8.0 * u, [0.7, 1.0, 0.55, 0.5]);
        if f.xp_need > 0 {
            let s = format!("{} / {}", (f.xp * f.xp_need as f32).round() as u32, f.xp_need);
            c.text(bar[2] + 12.0 * u, py(212.0) + ly, 12.0 * u, Face::Mono, &s, grey(0.42, 1.0));
        }
        let mut effects_out: Vec<[f32; 5]> = Vec::new();
        for (k, (nm, left, lvl, all)) in f.effects.iter().enumerate().take(6) {
            let cx = px(LX + 25.0 + k as f32 * 64.0) + lxo;
            let cy = py(262.0) + ly;
            let part = if *all > 0.0 { (left / all).clamp(0.0, 1.0) } else { 1.0 };
            c.ring([cx, cy], 22.0 * u, 2.0 * u, grey(1.0, 0.1));
            c.arc([cx, cy], 22.0 * u, 2.0 * u, part, grey(1.0, 0.85));
            let secs = (left / 20.0).ceil().max(0.0) as u32;
            let tm = if *left < 0.0 { "∞".to_string() } else { format!("{}:{:02}", secs / 60, secs % 60) };
            c.text_c(cx, cy + 40.0 * u, 11.0 * u, Face::Mono, &tm, grey(0.64, 1.0));
            if *lvl > 0 {
                let roman = ["", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X"].get(*lvl as usize).copied().unwrap_or("X+");
                let tw = fonts().width(Face::Mono, 10.0 * u, roman);
                let r = [cx + 10.0 * u, cy - 26.0 * u, cx + 18.0 * u + tw, cy - 12.0 * u];
                c.rect(r, 6.0 * u, grey(0.93, 1.0));
                c.text(r[0] + 4.0 * u, r[3] - 3.5 * u, 10.0 * u, Face::Mono, roman, grey(0.04, 1.0));
            }
            let _ = nm;
            effects_out.push([k as f32, cx - 13.0 * u, cy - 13.0 * u, 26.0 * u, open * la]);
        }

        // ---- left: the equipment's rows
        caps(&mut c, px(LX) + lxo, py(ROW_Y - 22.0) + ly, 12.0 * u, &tr("Equipment"));
        for (k, idx) in [5usize, 6, 7, 8, 45].iter().enumerate() {
            let Some(s) = f.slots.get(*idx) else { continue };
            let y = ROW_Y + k as f32 * ROW_P;
            let (nm, sub) = &f.eq[k];
            let tx = px(LX + 90.0) + lxo;
            let empty = s.count == 0;
            let nmc = if empty { grey(0.42, 1.0) } else { rgb(s.rar, 1.0) };
            let shown = if empty { tr(["Helmet", "Chestplate", "Leggings", "Boots", "Off hand"][k]).to_string() } else { nm.clone() };
            let shown = fonts().fit(Face::Semi, 16.5 * u, &shown, 290.0 * u);
            c.text(tx, py(y + 24.0) + ly, 16.5 * u, Face::Semi, &shown, nmc);
            let sub = if empty { tr("empty").to_string() } else { sub.clone() };
            let sub = fonts().fit(Face::Mono, 11.5 * u, &sub, 290.0 * u);
            c.text(tx, py(y + 44.0) + ly, 11.5 * u, Face::Mono, &sub, grey(0.42, 1.0));
            if !empty && s.max > 0.0 && s.dmg > 0.0 {
                let fr = (1.0 - s.dmg / s.max).clamp(0.0, 1.0);
                let r = [tx, py(y + 58.0) + ly, tx + 220.0 * u, py(y + 60.0) + ly];
                c.rect(r, 1.0 * u, grey(1.0, 0.1));
                c.rect([r[0], r[1], r[0] + (r[2] - r[0]) * fr, r[3]], 1.0 * u, wear(fr));
                let s2 = format!("{} / {}", (s.max - s.dmg).round() as u32, s.max as u32);
                c.text(r[2] + 10.0 * u, r[3] + 3.5 * u, 11.0 * u, Face::Mono, &s2, grey(0.42, 1.0));
            }
        }
        caps(&mut c, px(LX) + lxo, py(820.0) + ly, 12.0 * u, &tr("Crafting"));
        let res = f.slots.first().is_some_and(|s| s.count > 0);
        let ar = [px(LX + 122.0) + lxo, py(894.0) + ly, px(LX + 156.0) + lxo, py(896.0) + ly];
        c.rect(ar, 1.0 * u, grey(1.0, 0.14));
        let ark = self.an(ak(A_INV, 5), if res { 1.0 } else { 0.0 }, 8.0, 0.0);
        if ark > 0.01 {
            c.rect([ar[0], ar[1], ar[0] + (ar[2] - ar[0]) * ark, ar[3]], 1.0 * u, grey(1.0, 1.0));
            c.glow([ar[0] + (ar[2] - ar[0]) * ark, (ar[1] + ar[3]) / 2.0], 8.0 * u, grey(1.0, 0.6 * ark));
        }

        // ---- right: the inspection
        let ra = col_in(0.24);
        c.alpha = open * ra;
        let rxo = lx + (1.0 - ra) * 46.0 * u;
        let insp_out: [f32; 7];
        let ix = px(INS + 130.0) + rxo;
        let iy = py(224.0) + ly;
        match &f.insp {
            Some(ins) => {
                let k = self.an(ak(A_INV, 6), 1.0, 5.0, 0.0);
                c.glow([ix, iy], 150.0 * u, rgb(ins.rar, 0.0));
                c.glow([ix, iy], 140.0 * u, mix_rgb(ins.rar, 0x8090ff, 0.4, 0.22 * k));
                c.glow([ix, py(368.0) + ly], 70.0 * u, grey(1.0, 0.1));
                c.rect([ix - 80.0 * u, py(366.0) + ly, ix + 80.0 * u, py(370.0) + ly], 2.0 * u, grey(1.0, 0.06));
                let x = px(INS + 280.0) + rxo;
                self.insp_text(&mut c, ins, x, py(116.0) + ly, 270.0 * u, u);
                insp_out = [1.0, ix - 100.0 * u, iy - 100.0 * u, 200.0 * u, (t * 0.7).sin() * 32.0 + (1.0 - k) * -140.0, -12.0, open * ra * k];
            }
            None => {
                c.text(px(INS + 280.0) + rxo, py(150.0) + ly, 22.0 * u, Face::Semi, &tr("Point at an item"), grey(0.42, 1.0));
                insp_out = [0.0; 7];
            }
        }
        // a box's contents: its wells (Java draws the items)
        let mut boxed_out: Vec<[f32; 4]> = Vec::new();
        if let Some(ins) = f.insp.as_ref().filter(|i| i.boxed.len() == 27) {
            let x0 = px(INS + 280.0) + rxo;
            let y0 = py(430.0) + ly;
            for (k, n) in ins.boxed.iter().enumerate() {
                let r = [x0 + (k % 9) as f32 * 31.0 * u, y0 + (k / 9) as f32 * 31.0 * u];
                c.rect([r[0], r[1], r[0] + 28.0 * u, r[1] + 28.0 * u], 6.0 * u, grey(1.0, 0.05));
                if *n > 0 {
                    boxed_out.push([k as f32, r[0] + 3.0 * u, r[1] + 3.0 * u, 22.0 * u]);
                    if *n > 1 {
                        top.text_r(r[0] + 28.0 * u, r[1] + 27.0 * u, 9.5 * u, Face::Mono, &n.to_string(), grey(1.0, 1.0));
                    }
                }
            }
        }

        // ---- right: the backpack
        caps(&mut c, px(BX) + rxo, py(BY - 26.0) + ly, 12.0 * u, &tr("Backpack"));
        for col in 0..9usize {
            let on = col == f.sel;
            let x = px(BX + col as f32 * BP + 28.0) + rxo;
            c.text_c(x, py(HY + 80.0) + ly, 11.0 * u, Face::Mono, &(col + 1).to_string(), if on { grey(0.64, 1.0) } else { grey(0.25, 1.0) });
        }
        let selx = self.an(ak(A_INV, 8), BX + f.sel as f32 * BP + 15.0, 14.0, BX + f.sel as f32 * BP + 15.0);
        c.rect([px(selx) + rxo, py(HY + 62.0) + ly, px(selx + 26.0) + rxo, py(HY + 64.0) + ly], 1.0 * u, grey(1.0, 1.0));
        c.glow([px(selx + 13.0) + rxo, py(HY + 63.0) + ly], 12.0 * u, grey(1.0, 0.5));

        // ---- the wells, the light on the hovered one; the counts and the wear over the items
        let hov_item = hovered.and_then(|i| f.slots.get(i)).map(|s| s.item).filter(|&v| v != 0);
        text::set_style(0, false, 0.75);
        for (idx, r0, g) in &rects {
            let s = &f.slots[*idx];
            let a = if *g == 0 { la } else { ra };
            let dx = if *g == 0 { -(1.0 - la) * 46.0 * u } else { (1.0 - ra) * 46.0 * u };
            let r = [r0[0] + dx, r0[1], r0[2] + dx, r0[3]];
            let side = r[2] - r[0];
            let rad = side * 0.215;
            c.alpha = open * a;
            c.rect(r, rad, grey(1.0, 0.045));
            if s.count > 0 {
                c.glow([(r[0] + r[2]) / 2.0, r[1] + side * 0.58], side * 0.55, rgb(s.col, if *g == 0 { 0.22 } else { 0.17 }));
            }
            if hov_item.is_some() && Some(s.item) == hov_item && Some(*idx) != hovered && s.count > 0 {
                c.rect(r, rad, grey(1.0, 0.04));
            }
            if s.drag {
                c.rect(r, rad, grey(1.0, 0.16));
            }
            // where Java draws its item, a little up and bigger under the pointer
            let lift = self.an(ak(A_INV, 100 + idx), if Some(*idx) == hovered { 1.0 } else { 0.0 }, 14.0, 0.0);
            let icon = side * if side > 66.0 * u { 0.72 } else if side > 60.0 * u { 0.75 } else if side < 54.0 * u { 0.77 } else { 0.785 };
            let ic = [(r[0] + r[2]) / 2.0 - icon / 2.0, (r[1] + r[3]) / 2.0 - icon / 2.0 - lift * 2.0 * u];
            rects_out(&mut out, *idx, [ic[0], ic[1], icon, 1.0 + lift * 0.09, open * a], r);
            top.alpha = open * a;
            if s.count > 1 {
                let fs = (side * 0.25).max(9.0 * u);
                top.text_r(r[2] - 4.0 * u, r[3] - 3.0 * u, fs, Face::Mono, &s.count.to_string(), grey(1.0, 1.0));
            }
            if s.count > 0 && s.max > 0.0 && s.dmg > 0.0 {
                let fr = (1.0 - s.dmg / s.max).clamp(0.0, 1.0);
                let b = [r[0] + side * 0.17, r[3] - side * 0.11, r[2] - side * 0.17, r[3] - side * 0.11 + 2.5 * u];
                top.rect(b, 1.25 * u, grey(1.0, 0.13));
                top.rect([b[0], b[1], b[0] + (b[2] - b[0]) * fr, b[3]], 1.25 * u, wear(fr));
            }
        }
        text::set_style(0, false, 0.0);
        let n_rects = (out.len() - 1) / 10;
        out.insert(1, n_rects as f32);
        // the light slides to the hovered slot and takes its size
        let lens_a = self.an(ak(A_INV, 9), if hovered.is_some() { 1.0 } else { 0.0 }, 12.0, 0.0);
        if let Some((_, r, g)) = hovered.and_then(|h| rects.iter().find(|(i, _, _)| *i == h)) {
            let dx = if *g == 0 { -(1.0 - la) * 46.0 * u } else { (1.0 - ra) * 46.0 * u };
            let fresh = lens_a < 0.02;
            let set = |g: &mut Gui, k: usize, v: f32| if fresh { g.an_set(ak(A_INV, k), v) } else { g.an(ak(A_INV, k), v, 16.0, v) };
            set(self, 10, r[0] + dx);
            set(self, 11, r[1]);
            set(self, 12, r[2] - r[0]);
        }
        if lens_a > 0.01 {
            let x = *self.anim.get(&ak(A_INV, 10)).unwrap_or(&0.0);
            let y = *self.anim.get(&ak(A_INV, 11)).unwrap_or(&0.0);
            let s = *self.anim.get(&ak(A_INV, 12)).unwrap_or(&56.0);
            c.alpha = open * lens_a;
            c.glow([x + s / 2.0, y + s / 2.0], s * 0.85, grey(1.0, 0.08));
            c.rect([x, y, x + s, y + s], s * 0.215, grey(1.0, 0.095));
        }

        // ---- the model: where the camera looks, its turn, what is tried on
        let focus = match hovered {
            Some(i @ 5..=8) => i - 4,
            Some(45) => 6,
            Some(_) if f.carried == 0 => match hs.kind {
                k @ 1..=4 => k as usize,
                5 => 5,
                _ => 0,
            },
            _ => 0,
        };
        let try_on = match hovered {
            Some(i) if i >= 9 && f.carried == 0 && hs.count > 0 => match hs.kind {
                1..=4 => 1.0,
                5 if i != 36 + f.sel => 2.0,
                _ => 0.0,
            },
            _ => 0.0,
        };
        // (moving across slots does not jerk the camera: the whole of her a moment after nothing about her)
        let since_focus = self.an(ak(A_INV, 13), if focus > 0 { 0.0 } else { 1.0 }, 4.0, 1.0);
        let fi = if focus > 0 || since_focus > 0.6 { focus } else { self.anim.get(&ak(A_INV, 14)).map_or(0, |v| *v as usize) };
        self.an_set(ak(A_INV, 14), fi as f32);
        let (sc, fy, turn) = FOCUS[fi.min(6)];
        let scale = self.an(ak(A_INV, 15), sc, 3.2, FOCUS[0].0);
        let cy = self.an(ak(A_INV, 16), fy, 3.2, FOCUS[0].1);
        let turn_to = self.an(ak(A_INV, 17), turn, 3.0, FOCUS[0].2);
        let intro = ease_out(((f.since - 0.05) / 1.05).clamp(0.0, 1.0));
        let stage = [px(STAGE[0]) + cxo, py(STAGE[1]) + cyo, px(STAGE[2]) + cxo, py(STAGE[3]) + cyo];
        let yaw = {
            let mut tn = TURN.lock().unwrap();
            let dt = (t - tn.t).clamp(0.0, 0.1);
            tn.t = t;
            let on_stage = inside(stage, f.mouse) && hovered.is_none();
            if f.lmb && !tn.was_down {
                tn.began_on_stage = on_stage && f.carried == 0;
                tn.drag = None;
            }
            if f.lmb && tn.began_on_stage {
                if let Some(last) = tn.drag {
                    let d = (f.mouse[0] - last) / u * 0.55;
                    tn.yaw += d;
                    tn.v = tn.v + (d / dt.max(0.001) - tn.v) * 0.5;
                    tn.dragged_at = t;
                }
                tn.drag = Some(f.mouse[0]);
            } else {
                tn.drag = None;
                tn.yaw += tn.v * dt;
                tn.v *= 0.04f32.powf(dt);
                if t - tn.dragged_at > 1.6 || focus > 0 {
                    let k = ((tn.yaw - turn_to) / 360.0).round();
                    let rest = turn_to + k * 360.0;
                    tn.yaw += (rest - tn.yaw) * (1.0 - (-dt * if focus > 0 { 5.0 } else { 2.2 }).exp());
                }
            }
            tn.was_down = f.lmb;
            tn.yaw
        };
        let head_x = ((f.mouse[0] - (stage[0] + stage[2]) / 2.0) / (300.0 * u)).atan().to_degrees().clamp(-45.0, 45.0) * 0.8;
        let head_y = ((f.mouse[1] - py(330.0)) / (300.0 * u)).atan().to_degrees().clamp(-30.0, 30.0) * 0.7;
        // (the game's head turn is the other way round: negative toward the right)
        let hx = self.an(ak(A_INV, 18), -head_x, 6.0, 0.0);
        let hy = self.an(ak(A_INV, 19), head_y, 6.0, 0.0);
        out.extend_from_slice(&[stage[0], stage[1], stage[2], stage[3], scale * u * (0.6 + 0.4 * intro), cy, yaw - (1.0 - intro) * 60.0, 0.0, hx, hy, try_on]);
        out.extend_from_slice(&insp_out);
        out.push(effects_out.len() as f32);
        for e in &effects_out {
            out.extend_from_slice(e);
        }
        out.push(boxed_out.len() as f32);
        for b in &boxed_out {
            out.extend_from_slice(b);
        }
        out.push(46.0 * u);
        // the mark on the body a slot is about (orthographic: its height straight under the camera's)
        if focus > 0 && focus < 5 {
            let part = [0.0, 1.62, 1.22, 0.62, 0.12][focus];
            let mk_y = (stage[1] + stage[3]) / 2.0 + (cy - part) * scale * u;
            let mk = [(stage[0] + stage[2]) / 2.0, mk_y];
            top.alpha = open * (1.0 - since_focus);
            top.ring(mk, 7.0 * u, 1.5 * u, grey(1.0, 0.9));
            top.glow(mk, 18.0 * u, grey(1.0, 0.35));
            let pulse = (t * 0.62).fract();
            top.ring(mk, (7.0 + pulse * 10.0) * u, 1.0 * u, grey(1.0, 0.35 * (1.0 - pulse)));
            if let Some((_, r, _)) = hovered.filter(|i| (5..=8).contains(i)).and_then(|h| rects.iter().find(|(i, _, _)| *i == h)) {
                let sy = (r[1] + r[3]) / 2.0;
                let sx = r[2] + 330.0 * u;
                let ex = mk[0] - 9.0 * u;
                let bx = sx.min(ex - 30.0 * u);
                top.line([r[2] + 300.0 * u, sy], [bx, sy], 1.0 * u, grey(1.0, 0.45), 0.0);
                top.line([bx, sy], [ex, mk[1]], 1.0 * u, grey(1.0, 0.45), 0.0);
            }
        }
        c.alpha = 1.0;
        top.alpha = 1.0;
        (c, top, out)
    }
}

impl Gui {
    /// The inspected item's words from (x, y) on, `wmax` wide: its rarity and kind, its name, the game's lines under
    /// it (a server's colours softened), its numbers as bars. Where they end.
    pub(super) fn insp_text(&mut self, c: &mut Canvas, ins: &Insp, x: f32, y0: f32, wmax: f32, u: f32) -> f32 {
        let mut y = y0;
        let rar_name = tr(match ins.rar & 0xFFFFFF {
            0xFF55FF => "Epic",
            0x55FFFF => "Rare",
            0xFFFF55 => "Uncommon",
            _ => "Common",
        });
        let head = if ins.kind.is_empty() { rar_name.to_uppercase() } else { format!("{} · {}", rar_name, ins.kind).to_uppercase() };
        c.text_ls(x, y, 11.0 * u, Face::Mono, &head, rgb(ins.rar, 1.0), 0.14, 0.0);
        y += 14.0 * u;
        for line in wrap(&ins.name, Face::Semi, 27.0 * u, wmax).iter().take(3) {
            y += 30.0 * u;
            c.text(x, y, 27.0 * u, Face::Semi, line, rgb(ins.rar, 1.0));
        }
        if ins.count > 1 {
            y += 20.0 * u;
            c.text(x, y, 11.5 * u, Face::Mono, &format!("{} {}", ins.count, tr("pcs")), grey(0.42, 1.0));
        }
        y += 10.0 * u;
        let mut shown = 0;
        for (colr, line) in &ins.lines {
            if line.trim().is_empty() {
                continue;
            }
            for l in wrap(line, Face::Regular, 14.0 * u, wmax).iter().take(2) {
                if shown >= 9 {
                    break;
                }
                y += 20.0 * u;
                let lc = if *colr & 0xFFFFFF == 0xFFFFFF || *colr & 0xFFFFFF == 0xAAAAAA { grey(0.64, 1.0) } else { soften(*colr) };
                c.text(x, y, 14.0 * u, Face::Regular, l, lc);
                shown += 1;
            }
        }
        // the numbers as bars
        let mut rows: Vec<(String, f32, String, Option<Rgba>)> = Vec::new();
        let names = ["Damage", "Speed", "Armour", "Toughness", "Knockback", "Food", "Saturation"];
        let maxs = [12.0, 4.0, 8.0, 3.0, 0.4, 20.0, 20.0];
        for (n, v) in ins.stats.iter().enumerate() {
            if *v >= 0.0 {
                rows.push((tr(names[n]).to_string(), v / maxs[n], fmt1(*v), None));
            }
        }
        if ins.max > 0.0 {
            let fr = (1.0 - ins.dmg / ins.max).clamp(0.0, 1.0);
            rows.push((tr("Durability").to_string(), fr, format!("{}", (ins.max - ins.dmg).round() as u32), Some(wear(fr))));
        }
        if !rows.is_empty() {
            y += 14.0 * u;
        }
        let grow = self.an(ak(A_INV, 7), 1.0, 5.0, 0.0);
        for (n, fr, v, wc) in rows.iter().take(6) {
            y += 19.0 * u;
            c.text(x, y, 12.0 * u, Face::Mono, n, grey(0.42, 1.0));
            let b = [x + 96.0 * u, y - 5.0 * u, x + wmax - 52.0 * u, y - 2.0 * u];
            c.rect(b, 1.5 * u, grey(1.0, 0.09));
            c.rect([b[0], b[1], b[0] + (b[2] - b[0]) * fr.clamp(0.0, 1.0) * grow, b[3]], 1.5 * u, wc.unwrap_or(grey(1.0, 0.8)));
            c.text_r(x + wmax, y, 12.0 * u, Face::Mono, v, grey(0.93, 1.0));
        }
        y
    }
}

/// What is chosen (the hotbar's held slot, an option, a tab): a light from the middle of its foot up into it, no
/// outline (her word: an outline always looks cheap).
pub(super) fn lamp(c: &mut Canvas, r: [f32; 4], rad: f32, col: Rgba) {
    c.rect_lit(r, rad, fade(col, 0.05), fade(col, 0.32), 6.0);
}

/// A slot for Java: its index, where its item goes (x, y, side, how much bigger, seen), the slot's own box.
pub(super) fn rects_out(out: &mut Vec<f32>, idx: usize, icon: [f32; 5], r: [f32; 4]) {
    out.push(idx as f32);
    out.extend_from_slice(&icon);
    out.extend_from_slice(&r);
}

/// Small spaced capitals (the section labels).
pub(super) fn caps(c: &mut Canvas, x: f32, base: f32, size: f32, s: &str) {
    c.text_ls(x, base, size, Face::Mono, &s.to_uppercase(), grey(0.42, 1.0), 0.16, 0.0);
}

pub(super) fn fmt1(v: f32) -> String {
    if (v - v.round()).abs() < 0.05 {
        format!("{}", v.round() as i32)
    } else {
        format!("{v:.1}")
    }
}

/// The wear's colour: light while there is plenty, amber, then red.
pub(super) fn wear(f: f32) -> Rgba {
    if f > 0.5 {
        grey(1.0, 0.75)
    } else if f > 0.22 {
        [1.0, 0.76, 0.42, 1.0]
    } else {
        [1.0, 0.42, 0.37, 1.0]
    }
}

/// A server's or the game's colour of a tooltip line, a little softer on the glass.
fn soften(c: u32) -> Rgba {
    let r = rgb(c, 1.0);
    [r[0] * 0.85 + 0.12, r[1] * 0.85 + 0.12, r[2] * 0.85 + 0.12, 1.0]
}

pub(super) fn mix_rgb(a: u32, b: u32, k: f32, alpha: f32) -> Rgba {
    let (x, y) = (rgb(a, 1.0), rgb(b, 1.0));
    [x[0] + (y[0] - x[0]) * k, x[1] + (y[1] - x[1]) * k, x[2] + (y[2] - x[2]) * k, alpha]
}

/// Words wrapped to `max` px.
fn wrap(s: &str, face: Face, size: f32, max: f32) -> Vec<String> {
    let f = fonts();
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in s.split(' ') {
        let try_ = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
        if f.width(face, size, &try_) <= max || cur.is_empty() {
            cur = try_;
        } else {
            lines.push(std::mem::take(&mut cur));
            cur = word.to_string();
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
        .into_iter()
        .map(|l| if f.width(face, size, &l) > max { f.fit(face, size, &l, max) } else { l })
        .collect()
}
