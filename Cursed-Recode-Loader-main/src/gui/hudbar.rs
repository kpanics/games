//! The Custom Hotbar's parts (Interface → Custom Hotbar): the hotbar, health, food, armour, air, experience and the
//! mount, drawn here as elements of the HUD — each dragged in the editor, each its own look — while the game's own are
//! left out (Java's HudBody). Their facts come with the HUD's (Body). Everything eases: the held slot's light slides to
//! the next, the bars fill and drain smoothly, lost health leaves a trail that follows a moment later, a new level
//! flashes.

use super::*;
use crate::gui::draw::FLOATS;

/// The body as the game says it (Java's HudBody), each frame.
#[derive(Clone, Default)]
pub struct Body {
    pub sel: usize,
    pub off_left: bool,
    pub hp: f32,
    pub hp_max: f32,
    pub absorb: f32,
    /// 0 as it is, 1 poisoned, 2 withered, 3 frozen
    pub heart: u8,
    pub hardcore: bool,
    pub regen: bool,
    pub food: f32,
    pub sat: f32,
    pub hunger: bool,
    pub armor: f32,
    pub tough: f32,
    pub air: f32,
    pub air_max: f32,
    pub in_water: bool,
    pub level: i32,
    pub xp: f32,
    pub mount_hp: f32,
    pub mount_max: f32,
    /// the jump being charged (0..1; negative: nothing to jump on)
    pub jump: f32,
    pub attack: f32,
    /// 0 survival, 1 creative, 2 adventure, 3 spectator
    pub mode: u8,
    /// seconds since the held item changed, since the last hurt
    pub name_since: f32,
    pub hurt_since: f32,
    pub name: String,
    /// the nine slots and the off hand: count (0 empty), how much is left of it (negative: not worn), its cooldown
    pub slots: [[f32; 3]; 10],
}

impl Body {
    /// Its numbers from Java (HudBody.SIZE of them) and the held item's name.
    pub fn parse(g: &dyn Fn(usize) -> f64, at: usize, name: String) -> Body {
        let f = |i: usize| g(at + i) as f32;
        let mut b = Body {
            sel: (f(0).max(0.0) as usize).min(8),
            off_left: f(1) > 0.5,
            hp: f(2),
            hp_max: f(3).max(1.0),
            absorb: f(4),
            heart: f(5) as u8,
            hardcore: f(6) > 0.5,
            regen: f(7) > 0.5,
            food: f(8),
            sat: f(9),
            hunger: f(10) > 0.5,
            armor: f(11),
            tough: f(12),
            air: f(13),
            air_max: f(14).max(1.0),
            in_water: f(15) > 0.5,
            level: f(16) as i32,
            xp: f(17).clamp(0.0, 1.0),
            mount_hp: f(18),
            mount_max: f(19),
            jump: f(20),
            attack: f(21).clamp(0.0, 1.0),
            mode: f(22) as u8,
            name_since: f(23),
            hurt_since: f(24),
            name,
            slots: [[0.0; 3]; 10],
        };
        for i in 0..10 {
            b.slots[i] = [f(25 + i * 3), f(26 + i * 3), f(27 + i * 3)];
        }
        b
    }

    /// What the editor and the menu show when there is no game to say it.
    pub fn sample() -> Body {
        let mut b = Body {
            sel: 2,
            hp: 15.0,
            hp_max: 20.0,
            absorb: 4.0,
            food: 17.0,
            sat: 5.0,
            armor: 14.0,
            tough: 2.0,
            air: 180.0,
            air_max: 300.0,
            in_water: true,
            level: 23,
            xp: 0.45,
            mount_hp: 18.0,
            mount_max: 26.0,
            jump: 0.6,
            attack: 1.0,
            name_since: 99.0,
            hurt_since: 99.0,
            name: tr("Diamond Sword").to_string(),
            ..Body::default()
        };
        for (i, s) in [[1.0, 0.82, 0.0], [1.0, -1.0, 0.0], [1.0, 0.4, 0.0], [16.0, -1.0, 0.0], [64.0, -1.0, 0.0], [1.0, -1.0, 0.35], [0.0, -1.0, 0.0], [12.0, -1.0, 0.0], [1.0, 0.15, 0.0], [1.0, -1.0, 0.0]]
            .into_iter()
            .enumerate()
        {
            b.slots[i] = s;
        }
        b
    }

    /// Whether the game hurts the player (survival, adventure): only then the bars of the body.
    fn mortal(&self) -> bool {
        self.mode == 0 || self.mode == 2
    }
}

/// A slot's corner by its shape and size.
fn slot_radius(shape: usize, size: f32) -> f32 {
    match shape {
        1 => size * 0.08,
        2 => size * 0.5,
        _ => size * 0.24,
    }
}

/// How worn: green, then amber, then red, as little is left.
fn wear(left: f32) -> Rgba {
    let g = [0.48, 0.86, 0.44, 1.0];
    let a = [0.98, 0.78, 0.32, 1.0];
    let r = [0.98, 0.36, 0.32, 1.0];
    if left > 0.5 {
        mix(a, g, (left - 0.5) * 2.0)
    } else {
        mix(r, a, left * 2.0)
    }
}

/// A colour toward another (an effect's tint).
fn tint(c: Rgba, to: u32, k: f32) -> Rgba {
    let t = [((to >> 16) & 255) as f32 / 255.0, ((to >> 8) & 255) as f32 / 255.0, (to & 255) as f32 / 255.0, c[3]];
    mix(c, t, k)
}

/// The icons the bars use (shaders/gui/gui.comp icon()): full and its left half.
const HEART: (u32, u32) = (14, 15);
const FOOD: (u32, u32) = (16, 17);
const ARMOR: (u32, u32) = (18, 19);
const BUBBLE: (u32, u32) = (20, 20);

/// The animations' keys of the Custom Hotbar.
const A_HB: u64 = 40;

/// The parts the hotbar holds while it is a shelf: none of them drawn by itself then, nor among the cards.
const ON_SHELF: [&str; 6] = ["hb-health", "hb-food", "hb-armor", "hb-air", "hb-xp", "hb-mount"];
/// The shelf's row of the body over the slots, the room between them, and the room more under the slots for the
/// experience's line (design px).
const SHELF_ROW: f32 = 18.0;
const SHELF_GAP: f32 = 8.0;
const SHELF_FOOT: f32 = 4.0;

impl Gui {
    /// Whether the part `id` is on the hotbar's shelf now (the hotbar on, its bars on the shelf, its slots not a
    /// column: the shelf's row needs the slots' width).
    pub(crate) fn shelf_holds(&self, id: &str) -> bool {
        ON_SHELF.contains(&id)
            && self.mods.iter().find(|m| m.id == "hb-hotbar").is_some_and(|m| m.on && theme::pick(&m.set, "Bars") == 0 && theme::pick(&m.set, "Layout") != 1)
    }

    /// Whether a part of the Custom Hotbar is switched on (the shelf shows only those: one off is the game's again).
    pub(crate) fn part_on(&self, id: &str) -> bool {
        self.mods.iter().any(|m| m.id == id && m.on)
    }

    /// What a bar's trail of the lost shows (`key` its own): it waits a moment after a loss, then follows.
    fn bar_trail(&mut self, key: usize, v: f32, on: bool, t: f32) -> f32 {
        let mut trail = self.hud.bar_trail[key];
        if !on || v >= trail {
            trail = v;
            self.hud.bar_trail_at[key] = t;
        } else {
            // a new loss: it waits again before it follows (not what comes back: it would wait for ever as health
            // regenerates)
            if v < self.hud.bar_trail_seen[key] - 1e-3 {
                self.hud.bar_trail_at[key] = t;
            }
            if t - self.hud.bar_trail_at[key] > 0.45 {
                trail += (v - trail) * (1.0 - (-self.dt * 5.0).exp());
            }
        }
        self.hud.bar_trail[key] = trail;
        self.hud.bar_trail_seen[key] = v;
        trail
    }

    /// How bright a new level's flash is now (0 none; it fades in 0.9 s).
    fn level_flash(&mut self, level: i32, t: f32) -> f32 {
        if level != self.hud.level_was {
            if level > self.hud.level_was && self.hud.level_was >= 0 {
                self.hud.level_at = t;
            }
            self.hud.level_was = level;
        }
        (1.0 - (t - self.hud.level_at) / 0.9).clamp(0.0, 1.0)
    }
    /// The body as the game says, or an example (in the editor, the chat, the menu's preview).
    fn body_now(&self, x: &Ctx) -> Option<Body> {
        match self.hud.facts.body.clone() {
            Some(b) => Some(b),
            None if x.sample => Some(Body::sample()),
            None => None,
        }
    }

    pub(super) fn el_hotbar(&mut self, c: &mut Canvas, x: &Ctx) -> Option<[f32; 2]> {
        let b = self.body_now(x)?;
        if b.mode == 3 && !x.sample {
            return None;
        }
        let s = x.s;
        let p = x.pad();
        let size = x.num("Slot size").max(8.0) * s;
        let gap = x.num("Gap") * s;
        let shape = x.pick("Slot shape");
        let rad = slot_radius(shape, size);
        let layout = x.pick("Layout");
        let st = size + gap;
        let pos = |i: usize| -> [f32; 2] {
            match layout {
                1 => [0.0, i as f32 * st],
                2 if i >= 5 => [(i - 5) as f32 * st + st * 0.5, st],
                _ => [i as f32 * st, 0.0],
            }
        };
        let (cols, rows) = match layout {
            1 => (1.0, 9.0),
            2 => (5.0, 2.0),
            _ => (9.0, 1.0),
        };
        // a shelf: the body's row over the slots on the same glass (while the game hurts: not in creative), the
        // experience along its foot
        let shelf = x.pick("Bars") == 0 && layout != 1;
        // (the row while any part it holds is on: none, and it folds away)
        let any = ON_SHELF.iter().any(|id| self.part_on(id));
        let row_k = if shelf { self.an(ak(A_HB, 200), if (b.mortal() || x.sample) && any { 1.0 } else { 0.0 }, 10.0, 1.0) } else { 0.0 };
        let row_h = SHELF_ROW * s;
        let top = (row_h + SHELF_GAP * s) * row_k;
        let foot_k = self.an(ak(A_HB, 240), if x.on("Experience edge") && self.part_on("hb-xp") { 1.0 } else { 0.0 }, 10.0, 1.0);
        let foot = SHELF_FOOT * s * row_k * foot_k;
        let w = 2.0 * p + cols * size + (cols - 1.0) * gap;
        let h = 2.0 * p + rows * size + (rows - 1.0) * gap + top + foot;
        x.plate(c, [M, M, M + w, M + h]);
        let t = x.t;
        if row_k > 0.01 {
            self.shelf_row(c, x, &b, [M + p, M + p, M + w - p, M + p + row_h], row_k);
            // (under the slots, as wide as they are, midway to the glass's foot)
            self.shelf_edge(c, x, &b, [M + p, M + w - p], M + h - (p + foot) / 2.0, row_k);
        }
        let sel = b.sel.min(8);
        // the held slot's light: a drop between two edges on springs (critically damped), the one ahead quick (it is on
        // the slot as the wheel turns, not a slot behind) and the one behind following, so a fast wheel stretches it
        // along the bar and it gathers again where the wheel stops. On a line of slots the wheel past an end goes
        // through it, the short way (out at one end, in at the other), however many slots it turned in a frame; a key
        // of a slot far off goes straight there.
        let line = layout != 2;
        let omega = (x.num("Slide speed") / 100.0).max(0.25) * 13.0;
        let stretch = x.num("Stretch") / 100.0;
        let mut f = self.anim.get(&ak(A_HB, 2)).copied().unwrap_or(sel as f32);
        let mut v = self.anim.get(&ak(A_HB, 3)).copied().unwrap_or(0.0);
        let mut g = self.anim.get(&ak(A_HB, 6)).copied().unwrap_or(f);
        let mut gv = self.anim.get(&ak(A_HB, 7)).copied().unwrap_or(0.0);
        let mut target = self.anim.get(&ak(A_HB, 4)).copied().unwrap_or(sel as f32);
        let last = self.anim.get(&ak(A_HB, 5)).copied().unwrap_or(sel as f32) as usize;
        if sel != last {
            let d = (sel as i32 - last as i32).rem_euclid(9);
            let d = if d > 4 { d - 9 } else { d };
            if line && d.abs() <= 2 {
                target += d as f32;
            } else {
                // straight there: the light (both edges) brought back over the bar first
                let shift = (f / 9.0).floor() * 9.0;
                f -= shift;
                g -= shift;
                target = sel as f32;
            }
        }
        self.anim.insert(ak(A_HB, 5), sel as f32);
        let look = super::theme::pal();
        let dt = self.dt * look.speed;
        if !look.anim || dt > 0.25 {
            f = target;
            g = target;
            v = 0.0;
            gv = 0.0;
        } else {
            // the edge ahead twice as quick as the one behind; with no stretch both alike
            let lead = omega * (1.0 + 1.1 * stretch);
            let steps = (dt / 0.004).ceil().clamp(1.0, 64.0) as i32;
            let step = dt / steps as f32;
            for _ in 0..steps {
                let a = lead * lead * (target - f) - 2.0 * lead * v;
                v += a * step;
                f += v * step;
                let a = omega * omega * (target - g) - 2.0 * omega * gv;
                gv += a * step;
                g += gv * step;
            }
            if (target - f).abs() < 0.001 && v.abs() < 0.01 && (target - g).abs() < 0.001 && gv.abs() < 0.01 {
                f = target;
                g = target;
                v = 0.0;
                gv = 0.0;
            }
        }
        // there: through an end it is the slot at the other
        if f == target && g == target {
            f = f.rem_euclid(9.0);
            g = f;
            target = f;
        }
        self.anim.insert(ak(A_HB, 2), f);
        self.anim.insert(ak(A_HB, 3), v);
        self.anim.insert(ak(A_HB, 6), g);
        self.anim.insert(ak(A_HB, 7), gv);
        self.anim.insert(ak(A_HB, 4), target);
        // where it is: along the line, or between the two slots of the two rows it goes from and to
        let at = |f: f32| -> [f32; 2] {
            match layout {
                1 => [0.0, f * st],
                2 => {
                    let i0 = (f.floor() as usize).min(8);
                    let (a, b2) = (pos(i0), pos((i0 + 1).min(8)));
                    let k = f - i0 as f32;
                    [a[0] + (b2[0] - a[0]) * k, a[1] + (b2[1] - a[1]) * k]
                }
                _ => [f * st, 0.0],
            }
        };
        let held = x.col("Held colour", x.el.accent);
        let style = x.pick("Held slot");
        let base = [M + p, M + p + top];
        let counts = x.on("Counts");
        let dur_on = x.on("Durability");
        let dur_as = x.pick("Durability as");
        let wear_col = x.on("Wear colours");
        let empty = x.on("Empty slots");
        for i in 0..9 {
            // each slot as held as the light is over it: the item rises and lights with it, not on its own clock
            let d = (f.rem_euclid(9.0) - i as f32).abs();
            let d = if line { d.min(9.0 - d) } else { d };
            let k = (1.0 - d).clamp(0.0, 1.0);
            let k = k * k * (3.0 - 2.0 * k);
            let lift = if style == 0 { 2.5 * s * k } else { 0.0 };
            let q = pos(i);
            let r = [base[0] + q[0], base[1] + q[1] - lift, base[0] + q[0] + size, base[1] + q[1] + size - lift];
            self.slot(c, x, &b, i, r, rad, k, empty, counts, dur_on, dur_as, wear_col);
            if x.on("Slot numbers") {
                c.text(r[0] + 3.0 * s, r[1] + 9.0 * s, 8.0 * s, Face::Mono, &(i + 1).to_string(), fade(x.el.text2, 0.55 + 0.45 * k));
            }
        }
        // the light over the slots' glass (one piece: under the glass the gaps between the slots showed it brighter),
        // from its edge behind to its edge ahead; past the last slot it is also coming in before the first, each cut
        // to the slots' span. Stretched it spreads its light: dimmer the longer it is.
        let (lo, hi) = if line { (f.min(g), f.max(g)) } else { (f, f) };
        let long = hi - lo;
        let dim = 1.0 / (1.0 + 0.55 * long);
        let mut pieces = vec![(lo, hi)];
        if line && hi > 8.0 {
            pieces.push((lo - 9.0, hi - 9.0));
        }
        if line && lo < 0.0 {
            pieces.push((lo + 9.0, hi + 9.0));
        }
        let span = [base[0] - 1.0, base[1] - 4.0 * s, base[0] + cols * size + (cols - 1.0) * gap + 1.0, base[1] + rows * size + (rows - 1.0) * gap + 4.0 * s];
        c.push_clip(if layout == 1 { [M, span[1], M + w, span[3]] } else { [span[0], M, span[2], M + h] });
        let lift_s = if style == 0 { 2.5 * s } else { 0.0 };
        for (a, b2) in pieces {
            let (p0, p1) = (at(a), at(b2));
            let hr = [base[0] + p0[0], base[1] + p0[1] - lift_s, base[0] + p1[0] + size, base[1] + p1[1] + size - lift_s];
            let k = dim;
            match style {
                1 => {
                    let cx = (hr[0] + hr[2]) / 2.0;
                    let cy = (hr[1] + hr[3]) / 2.0;
                    c.glow([cx, cy], size * 0.9 + 0.5 * (hr[2] - hr[0] + hr[3] - hr[1] - 2.0 * size), fade(held, 0.22 * k));
                    c.rect(hr, rad, fade(held, 0.12 * k));
                }
                2 => c.rect(hr, rad, fade(held, 0.26 * k)),
                3 => c.rect([hr[0] + size * 0.22, hr[3] + lift_s + 2.0 * s, hr[2] - size * 0.22, hr[3] + lift_s + 3.6 * s], 1.0 * s, fade(held, 0.4 + 0.6 * k)),
                // lifted a little, a light from its foot up to the item (no outline: her word, it always looks cheap)
                _ => c.rect_lit(hr, rad, fade(held, 0.06 * k), fade(held, 0.34 * k), 6.0),
            }
        }
        c.pop_clip();
        // the off hand: beside the bar (out of the element's box: the hotbar keeps its place when it fills)
        let side = match x.pick("Off hand") {
            1 => Some(true),
            2 => Some(false),
            3 => None,
            _ => Some(b.off_left),
        };
        let has_off = b.slots[9][0] > 0.0;
        let ok = self.an(ak(A_HB, 30), if has_off && side.is_some() { 1.0 } else { 0.0 }, 10.0, 0.0);
        if ok > 0.01 {
            let left = side.unwrap_or(true);
            let ow = size + 2.0 * p;
            let ox = if left { M - 10.0 * s - ow } else { M + w + 10.0 * s };
            let oy = M + h - ow;
            let a0 = c.alpha;
            c.alpha *= ok;
            x.part(c, [ox, oy, ox + ow, oy + ow]);
            let r = [ox + p, oy + p, ox + p + size, oy + p + size];
            self.slot(c, x, &b, 9, r, rad, 0.0, true, counts, dur_on, dur_as, wear_col);
            c.alpha = a0;
        }
        // the attack's strength under the held slot while it comes back
        if x.on("Attack strength") {
            let ak_ = self.an(ak(A_HB, 31), if b.attack < 0.999 { 1.0 } else { 0.0 }, 9.0, 0.0);
            if ak_ > 0.01 {
                let y = M + h + 4.0 * s;
                let q = pos(sel);
                let (x0, x1) = (base[0] + q[0] + size * 0.12, base[0] + q[0] + size * 0.88);
                c.rect([x0, y, x1, y + 2.0 * s], 1.0 * s, grey(1.0, 0.12 * ak_));
                c.rect([x0, y, x0 + (x1 - x0) * b.attack, y + 2.0 * s], 1.0 * s, fade(x.el.accent, 0.85 * ak_));
            }
        }
        // the held item's name over the bar: once the wheel rests on a slot (a fast wheel does not flash a name at each
        // one it passes), rising in; another while it shows crosses over in the same plate, which glides to its width
        if x.on("Item name") {
            let time = x.num("Item name time").max(0.5);
            let rest = x.num("Name after") / 1000.0;
            let since = b.name_since;
            let now = self.now;
            let shown = self.anim.get(&ak(A_HB, 40)).copied().unwrap_or(0.0);
            if !b.name.is_empty() && b.name != self.hud.hb_name && (since >= rest || shown > 0.05) {
                self.hud.hb_name_prev = std::mem::take(&mut self.hud.hb_name);
                self.hud.hb_name = b.name.clone();
                self.hud.hb_name_at = now;
            }
            let want = !b.name.is_empty() && b.name == self.hud.hb_name && since >= rest.min(time - 0.3) && since < time;
            let a = self.an(ak(A_HB, 40), if want { 1.0 } else { 0.0 }, if want { 14.0 } else { 7.0 }, 0.0);
            if a > 0.01 && !self.hud.hb_name.is_empty() {
                let size_t = 13.0 * s;
                let tw_to = fonts().width(Face::Semi, size_t, &self.hud.hb_name);
                let tw = self.an(ak(A_HB, 41), tw_to, 16.0, tw_to);
                let cx = M + w / 2.0;
                let rise = (1.0 - a) * 6.0 * s;
                let y = M - 12.0 * s + rise;
                c.rect([cx - tw / 2.0 - 10.0 * s, y - size_t - 4.0 * s, cx + tw / 2.0 + 10.0 * s, y + 6.0 * s], 9.0 * s, grey(0.0, 0.38 * a));
                let k = ((now - self.hud.hb_name_at) / 0.16).clamp(0.0, 1.0);
                let k = k * k * (3.0 - 2.0 * k);
                if k < 1.0 && !self.hud.hb_name_prev.is_empty() {
                    c.text_c(cx, y - 4.0 * s * k, size_t, Face::Semi, &self.hud.hb_name_prev, fade(x.el.text, a * (1.0 - k)));
                }
                c.text_c(cx, y + 4.0 * s * (1.0 - k), size_t, Face::Semi, &self.hud.hb_name, fade(x.el.text, a * k.max(if self.hud.hb_name_prev.is_empty() { 1.0 } else { 0.0 })));
            }
        }
        let _ = t;
        Some([w, h])
    }

    /// One slot: its plate, its item (Java draws it where room is left: icon kind 9), count, wear, cooldown.
    #[allow(clippy::too_many_arguments)]
    fn slot(&mut self, c: &mut Canvas, x: &Ctx, b: &Body, i: usize, r: [f32; 4], rad: f32, lit: f32, empty: bool, counts: bool, dur_on: bool, dur_as: usize, wear_col: bool) {
        let s = x.s;
        let size = r[2] - r[0];
        let [count, left, cool] = b.slots[i];
        let has = count > 0.0;
        if has || empty {
            c.rect(r, rad, grey(1.0, 0.05 + 0.04 * lit));
        }
        if !has {
            return;
        }
        let isz = size * (0.62 + 0.06 * lit);
        let (cx, cy) = ((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0);
        // Item Cooldowns' seconds on its top edge (the item's icon, drawn over the HUD, keeps the middle); the item a
        // little faded while it cools
        let secs = if cool > 0.0 && self.cool_at("Hotbar") { self.hud.facts.cools.iter().find(|q| (q.frac - cool).abs() < 0.003).map(|q| q.secs) } else { None };
        if x.icons {
            let fade_k = if secs.is_some() { 0.75 } else { 1.0 };
            self.hud.icons.push([9.0, i as f32, cx - isz / 2.0, cy - isz / 2.0, isz, isz, c.alpha * fade_k]);
        } else {
            // (the menu's preview: a mark where the item goes)
            c.rect([cx - isz * 0.32, cy - isz * 0.32, cx + isz * 0.32, cy + isz * 0.32], isz * 0.12, grey(1.0, 0.18));
        }
        // its cooldown: a shade turning away (as its switch says); Item Cooldowns' seconds
        if cool > 0.0 && x.on("Cooldowns") {
            c.arc([cx, cy], size * 0.24, size * 0.48, cool.clamp(0.0, 1.0), grey(0.0, 0.42));
            c.arc([cx, cy], size * 0.43, 1.4 * s, cool.clamp(0.0, 1.0), grey(1.0, 0.55));
        }
        if let Some(secs) = secs {
            let tx = self.cool_secs(secs);
            let fs = (size * 0.22).max(7.5 * s);
            let tw = fonts().width(Face::Semi, fs, &tx);
            let ph = fs * 1.25;
            c.rect([cx - tw / 2.0 - 3.0 * s, r[1] - ph / 2.0, cx + tw / 2.0 + 3.0 * s, r[1] + ph / 2.0], ph / 2.0, grey(0.0, 0.72));
            c.text_c(cx, r[1] + fs * 0.36, fs, Face::Semi, &tx, x.el.text);
        }
        if dur_on && left >= 0.0 && left < 0.999 {
            let col = if wear_col { wear(left) } else { x.el.accent };
            match dur_as {
                1 => {
                    c.ring([cx, cy], size * 0.44, 1.5 * s, grey(1.0, 0.08));
                    c.arc([cx, cy], size * 0.44, 1.5 * s, left, col);
                }
                2 => {
                    let tx = format!("{}%", (left * 100.0).round() as i32);
                    c.text(r[0] + 3.0 * s, r[1] + 9.0 * s, 8.0 * s, Face::Mono, &tx, col);
                }
                _ => {
                    let (x0, x1) = (r[0] + size * 0.18, r[2] - size * 0.18);
                    let y = r[3] - size * 0.13;
                    c.rect([x0, y, x1, y + 2.2 * s], 1.1 * s, grey(0.0, 0.55));
                    c.rect([x0, y, x0 + (x1 - x0) * left, y + 2.2 * s], 1.1 * s, col);
                }
            }
        }
        if counts && count > 1.0 {
            let tx = (count as i32).to_string();
            let fs = (size * 0.27).max(8.0 * s);
            c.text_r(r[2] - 3.0 * s + 0.8 * s, r[3] - 3.0 * s + 0.8 * s, fs, Face::Semi, &tx, grey(0.0, 0.6));
            c.text_r(r[2] - 3.0 * s, r[3] - 3.0 * s, fs, Face::Semi, &tx, x.el.text);
        }
    }

    /// The shelf's row in `r` (`k`: how much of it is there): armour while there is some, the heart, the health and its
    /// bar to the middle; the level in the middle on a capsule of its own; from it the food's bar (the mount's health
    /// while riding, as the game's) to its number and icon. Both bars drain toward their ends. Absorption is a thin
    /// line over the health's bar, the air one over the food's while it runs out.
    fn shelf_row(&mut self, c: &mut Canvas, x: &Ctx, b: &Body, r: [f32; 4], k: f32) {
        let s = x.s;
        let t = x.t;
        let a0 = c.alpha;
        c.alpha = a0 * k;
        let cy = (r[1] + r[3]) / 2.0;
        let thick = x.num("Bar thickness").max(1.0) * s;
        let isz = 13.0 * s;
        let ns = 11.5 * s;
        let nums = x.on("Numbers");
        // the numbers keep room for two figures: the bars do not move as they change
        let room = if nums { fonts().width(Face::Mono, ns, "00") } else { 0.0 };
        let base_y = cy + ns * 0.36;
        let acc = x.el.accent;
        let text = x.el.text;
        let trail_on = x.on("Lost health trail");
        let mid = (r[0] + r[2]) / 2.0;
        // ---- the level in the middle; a new one lights its capsule
        let flash = self.level_flash(b.level, t);
        let lv = b.level.max(0).to_string();
        // the level, the health and the right side (the food, or the mount's health while riding) each as its part is
        // switched: one going off fades and gives its room back
        let lv_k = self.an(ak(A_HB, 241), if x.on("Level") && self.part_on("hb-xp") { 1.0 } else { 0.0 }, 10.0, 1.0);
        let hp_k = self.an(ak(A_HB, 242), if self.part_on("hb-health") { 1.0 } else { 0.0 }, 10.0, 1.0);
        let hh = (r[3] - r[1]) / 2.0;
        let bw = (fonts().width(Face::Mono, ns, &lv) + 12.0 * s).max(2.0 * hh) * lv_k;
        if lv_k > 0.01 {
            c.alpha = a0 * k * lv_k;
            c.rect([mid - bw / 2.0, cy - hh, mid + bw / 2.0, cy + hh], hh, grey(1.0, 0.07 + 0.22 * flash));
            c.text_c(mid, base_y, ns, Face::Mono, &lv, fade(text, if b.level > 0 { 1.0 } else { 0.45 }));
            c.alpha = a0 * k;
        }
        let inner = bw / 2.0 + (4.0 + 3.0 * lv_k) * s;
        // ---- the left: armour (it comes in, the rest makes room), the heart, the number, the bar
        let mut lx = r[0];
        let arm_k = self.an(ak(A_HB, 201), if x.on("Armour") && self.part_on("hb-armor") && b.armor > 0.0 { 1.0 } else { 0.0 }, 10.0, 0.0);
        if arm_k > 0.01 {
            let an = (b.armor as i32).to_string();
            let asz = isz * 0.9;
            c.alpha = a0 * k * arm_k;
            c.icon([lx, cy - asz / 2.0, lx + asz, cy + asz / 2.0], ARMOR.0, x.el.text2);
            c.text(lx + asz + 4.0 * s, base_y, ns, Face::Mono, &an, x.el.text2);
            c.alpha = a0 * k;
            lx += (asz + 4.0 * s + fonts().width(Face::Mono, ns, &an) + 10.0 * s) * arm_k;
        }
        let mut hcol = acc;
        if x.on("Effect tints") {
            hcol = match b.heart {
                1 => tint(hcol, 0x8db84a, 0.75),
                2 => tint(hcol, 0x3c3c3c, 0.8),
                3 => tint(hcol, 0x9fd8ff, 0.7),
                _ => hcol,
            };
        }
        // little health left: it beats
        if x.on("Low health pulse") && b.hp / b.hp_max.max(1.0) <= 0.3 {
            hcol = fade(hcol, 0.55 + 0.45 * (t * 7.0).sin().abs());
        }
        c.alpha = a0 * k * hp_k;
        c.icon([lx, cy - isz / 2.0, lx + isz, cy + isz / 2.0], HEART.0, hcol);
        lx += isz + 5.0 * s;
        if nums {
            let hn = ((b.hp + b.absorb).ceil() as i32).to_string();
            c.text(lx, base_y, ns, Face::Mono, &hn, text);
            // (a server's hundred: the bar gives way)
            lx += room.max(fonts().width(Face::Mono, ns, &hn)) + 8.0 * s;
        } else {
            lx += 3.0 * s;
        }
        let hb = [lx, cy - thick / 2.0, mid - inner, cy + thick / 2.0];
        self.shelf_bar(c, hb, 0, b.hp, b.hp_max, hcol, false, trail_on, t);
        let ab_k = self.an(ak(A_HB, 202), if b.absorb > 0.0 { 1.0 } else { 0.0 }, 10.0, 0.0);
        if ab_k > 0.01 {
            let ab = self.an(ak(A_HB, 212), b.absorb, 9.0, b.absorb);
            let len = (hb[2] - hb[0]) * (ab / b.hp_max.max(1.0)).clamp(0.0, 1.0);
            c.rect([hb[0], hb[1] - 5.0 * s, hb[0] + len, hb[1] - 3.0 * s], 1.0 * s, fade(acc, 0.9 * ab_k));
        }
        c.alpha = a0 * k;
        // ---- the right: the food or the mount's health (between the two the side fades out and back), its number,
        // its icon
        let mk = self.an(ak(A_HB, 203), if b.mount_max > 0.0 && !x.sample { 1.0 } else { 0.0 }, 8.0, 0.0);
        let riding = mk > 0.5;
        let right_k = self.an(ak(A_HB, 243), if self.part_on(if b.mount_max > 0.0 && !x.sample { "hb-mount" } else { "hb-food" }) { 1.0 } else { 0.0 }, 10.0, 1.0);
        c.alpha = a0 * k * right_k * (1.0 - 4.0 * mk * (1.0 - mk)).max(0.0);
        let (v, max, icon, key, num) = if riding {
            (b.mount_hp, b.mount_max.max(1.0), HEART.0, 4, (b.mount_hp.ceil() as i32).to_string())
        } else {
            (b.food, 20.0, FOOD.0, 1, (b.food as i32).to_string())
        };
        let mut fcol = acc;
        if !riding && x.on("Effect tints") && b.hunger {
            fcol = tint(fcol, 0x8db84a, 0.75);
        }
        let mut rx = r[2];
        c.icon([rx - isz, cy - isz / 2.0, rx, cy + isz / 2.0], icon, fcol);
        rx -= isz + 5.0 * s;
        if nums {
            c.text_r(rx, base_y, ns, Face::Mono, &num, text);
            rx -= room.max(fonts().width(Face::Mono, ns, &num)) + 8.0 * s;
        } else {
            rx -= 3.0 * s;
        }
        let fb = [mid + inner, cy - thick / 2.0, rx, cy + thick / 2.0];
        self.shelf_bar(c, fb, key, v, max, fcol, true, trail_on && riding, t);
        c.alpha = a0 * k;
        let air_k = self.an(ak(A_HB, 204), if x.on("Air") && self.part_on("hb-air") && (b.in_water || b.air < b.air_max) { 1.0 } else { 0.0 }, 8.0, 0.0);
        if air_k > 0.01 {
            let f = self.an(ak(A_HB, 213), b.air / b.air_max, 9.0, b.air / b.air_max).clamp(0.0, 1.0);
            let ac = if b.air < b.air_max * 0.2 { fade(acc, 0.55 + 0.45 * (t * 7.0).sin().abs()) } else { acc };
            let line = [fb[0], fb[1] - 5.0 * s, fb[2], fb[1] - 3.0 * s];
            c.rect(line, 1.0 * s, grey(1.0, 0.08 * air_k));
            c.rect([line[2] - (line[2] - line[0]) * f, line[1], line[2], line[3]], 1.0 * s, fade(ac, 0.9 * air_k));
        }
        c.alpha = a0;
    }

    /// A bar of the shelf in `r`: its groove, the trail of what was lost (`key` its own), what is — filled from the
    /// left, or from the right.
    #[allow(clippy::too_many_arguments)]
    fn shelf_bar(&mut self, c: &mut Canvas, r: [f32; 4], key: usize, v: f32, max: f32, col: Rgba, right: bool, trail_on: bool, t: f32) {
        let shown = self.an(ak(A_HB, 100 + key), v, 9.0, v);
        let trail = self.bar_trail(key, v, trail_on, t);
        let len = r[2] - r[0];
        if len < 2.0 {
            return;
        }
        let rad = (r[3] - r[1]) / 2.0;
        let max = max.max(0.001);
        let part = |f: f32| -> [f32; 4] {
            let f = f.clamp(0.0, 1.0);
            if right { [r[2] - len * f, r[1], r[2], r[3]] } else { [r[0], r[1], r[0] + len * f, r[3]] }
        };
        c.rect(r, rad, grey(1.0, 0.10));
        if trail > shown + 0.01 {
            c.rect(part(trail / max), rad, grey(1.0, 0.3));
        }
        if shown > 0.001 {
            c.rect(part(shown / max), rad, col);
        }
    }

    /// The experience along the shelf's foot: a line from `span`'s left to its right, its middle at `y` (the jump being
    /// charged instead while riding something that jumps, as the game's); a new level lights it all a moment.
    fn shelf_edge(&mut self, c: &mut Canvas, x: &Ctx, b: &Body, span: [f32; 2], y: f32, k: f32) {
        if !x.on("Experience edge") || !self.part_on("hb-xp") {
            return;
        }
        let s = x.s;
        let a0 = c.alpha;
        let jk = self.an(ak(A_HB, 205), if b.jump >= 0.0 && !x.sample && self.part_on("hb-mount") { 1.0 } else { 0.0 }, 10.0, 0.0);
        let jumping = jk > 0.5;
        c.alpha = a0 * k * (1.0 - 4.0 * jk * (1.0 - jk)).max(0.0);
        let v = if jumping { b.jump.max(0.0) } else { b.xp };
        let f = self.an(ak(A_HB, 206), v, 8.0, v).clamp(0.0, 1.0);
        let th = 2.0 * s;
        let r = [span[0], y - th / 2.0, span[1], y + th / 2.0];
        let acc = x.el.accent;
        c.rect(r, th / 2.0, grey(1.0, 0.08));
        if f > 0.001 {
            c.rect([r[0], r[1], r[0] + (r[2] - r[0]) * f, r[3]], th / 2.0, fade(acc, 0.85));
        }
        let flash = if jumping { 0.0 } else { (1.0 - (x.t - self.hud.level_at) / 0.9).clamp(0.0, 1.0) };
        if flash > 0.0 {
            c.rect([r[0], r[1] - th, r[2], r[3] + th * 0.5], th, fade(acc, 0.3 * flash));
        }
        c.alpha = a0;
    }

    /// A bar of the body as its kind says: a bar, ten segments, icons (full, half, empty as the game's), or a number;
    /// `v` of `max` filled, `over` past it (absorption) or inside it (saturation, `inside`), the trail of what was lost
    /// (`key` its own), its icon at the start, its number at the end; «Fills from» the right turns it all around (the
    /// food's, as the game's). Returns its size.
    #[allow(clippy::too_many_arguments)]
    fn body_bar(&mut self, c: &mut Canvas, x: &Ctx, key: usize, v: f32, max: f32, over: f32, inside: bool, col: Rgba, over_col: Rgba, icon: (u32, u32), number: &str, pulse: f32) -> [f32; 2] {
        let s = x.s;
        let p = if x.el.bg { x.pad() } else { 0.0 };
        let kind = x.pick("Kind");
        let thick = x.num("Thickness").max(1.0) * s;
        // the whole of it as wide as this: the icon and the number take theirs from the bar
        let width = x.num("Width").max(20.0) * s;
        let show_icon = x.on("Icon");
        let show_num = x.on("Number") && !number.is_empty();
        let flip = x.pick("Fills from") == 1;
        let t = x.t;
        // shown values ease; the trail waits a moment, then follows
        let shown = self.an(ak(A_HB, 100 + key), v, 9.0, v);
        let total = if inside { max } else { (max + over).max(max) };
        // (the trail is health's: any bar with the setting)
        let trail = self.bar_trail(key, v, x.on("Lost health trail"), t);
        let pulse_k = if pulse > 0.0 { 0.72 + 0.28 * (t * 7.0).sin().abs() * pulse + (1.0 - pulse) * 0.28 } else { 1.0 };
        let col = [col[0] * pulse_k, col[1] * pulse_k, col[2] * pulse_k, col[3]];
        let isz = (thick * 2.2).max(12.0 * s);
        let num_size = (thick * 1.6).max(11.0 * s);
        // the number keeps room for two figures at least: the bar does not move as it changes
        let num_w = |size: f32| fonts().width(Face::Semi, size, number).max(fonts().width(Face::Semi, size, if number.contains('·') { "00 · 0" } else { "00" }));
        let start = c.p.len() / FLOATS;
        // the words go on after the shapes are turned (left to right always): [left, baseline, size]
        let mut words: Vec<[f32; 3]> = Vec::new();
        let cx = M + p;
        let mid_h;
        let w;
        match kind {
            2 => {
                // icons as the game's: ten a row, full, half, empty; what is past the most (absorption) after them
                let per = 10usize;
                let n_full = (total / 2.0).ceil().max(1.0) as usize;
                let rows = n_full.div_ceil(per);
                let step = isz * 0.86;
                let row_h = isz * 0.95;
                mid_h = rows as f32 * row_h;
                let mut ww = per.min(n_full) as f32 * step + isz * 0.14;
                let regen = key == 0 && self.hud.facts.body.as_ref().is_some_and(|b| b.regen);
                for k in 0..n_full {
                    let (rx, ry) = ((k % per) as f32, (rows - 1 - k / per) as f32);
                    // the regeneration's wave: one icon after another lifts
                    let bob = if regen { (t * 9.0 - k as f32 * 0.6).sin().max(0.0) * 1.6 * s } else { 0.0 };
                    let r = [cx + rx * step, M + p + ry * row_h - bob, cx + rx * step + isz, M + p + ry * row_h + isz - bob];
                    c.icon(r, icon.0, grey(0.0, 0.32));
                    c.icon(r, icon.0, grey(1.0, 0.10));
                    let here = k as f32 * 2.0;
                    let lost_full = trail.min(max) - here;
                    if lost_full > 0.0 && shown - here < 2.0 {
                        c.icon(r, if lost_full >= 2.0 { icon.0 } else { icon.1 }, grey(1.0, 0.45));
                    }
                    let fill = shown.min(max) - here;
                    if fill >= 2.0 {
                        c.icon(r, icon.0, col);
                    } else if fill > 0.0 {
                        c.icon(r, icon.1, col);
                    }
                    if !inside && here >= max {
                        let ov = over - (here - max);
                        if ov >= 2.0 {
                            c.icon(r, icon.0, over_col);
                        } else if ov > 0.0 {
                            c.icon(r, icon.1, over_col);
                        }
                    }
                    if inside && over > here {
                        let ins = [r[0] + isz * 0.3, r[1] + isz * 0.3, r[2] - isz * 0.3, r[3] - isz * 0.3];
                        c.rect(ins, isz * 0.2, fade(over_col, if over - here >= 2.0 { 0.9 } else { 0.5 }));
                    }
                }
                if show_num {
                    words.push([cx + ww + 6.0 * s, M + p + mid_h - row_h * 0.5 + num_size * 0.36, num_size]);
                    ww += 6.0 * s + num_w(num_size);
                }
                w = ww;
            }
            3 => {
                // a number: the icon and how much
                let big = (thick * 2.6).max(18.0 * s);
                mid_h = big * 1.1;
                let r = [cx, M + p + (mid_h - isz) / 2.0, cx + isz, M + p + (mid_h + isz) / 2.0];
                c.icon(r, icon.0, col);
                let xx = cx + isz + 6.0 * s;
                words.push([xx, M + p + mid_h * 0.5 + big * 0.36, big]);
                w = xx - cx + num_w(big);
            }
            _ => {
                mid_h = isz.max(thick);
                let cy = M + p + mid_h / 2.0;
                let mut bx0 = cx;
                if show_icon {
                    c.icon([cx, cy - isz / 2.0, cx + isz, cy + isz / 2.0], icon.0, col);
                    bx0 += isz + 6.0 * s;
                }
                let width = (width - (bx0 - cx) - if show_num { 6.0 * s + num_w(num_size) } else { 0.0 }).max(16.0 * s);
                let bx = [bx0, cy - thick / 2.0, bx0 + width, cy + thick / 2.0];
                let rr = thick / 2.0;
                let total = total.max(0.001);
                let segs = kind == 1;
                let n = if segs { 10 } else { 1 };
                let sg = if segs { 2.0 * s } else { 0.0 };
                let seg_w = (width - sg * (n as f32 - 1.0)) / n as f32;
                for k in 0..n {
                    let (a0, a1) = (k as f32 / n as f32, (k + 1) as f32 / n as f32);
                    let r = [bx[0] + k as f32 * (seg_w + sg), bx[1], bx[0] + k as f32 * (seg_w + sg) + seg_w, bx[3]];
                    let seg = |f: f32| ((f - a0) / (a1 - a0)).clamp(0.0, 1.0);
                    let rad = if segs { rr.min(2.0 * s) } else { rr };
                    // its empty part: dark under a little light, seen over a bright world and a dark one
                    c.rect(r, rad, grey(0.0, 0.42));
                    c.rect(r, rad, grey(1.0, 0.07));
                    // the trail of what was lost, then what is, then what is past it (or inside it)
                    let ft = seg(trail.min(total) / total);
                    if ft > 0.0 {
                        c.rect([r[0], r[1], r[0] + (r[2] - r[0]) * ft, r[3]], rad, grey(1.0, 0.42));
                    }
                    let fv = seg(shown.min(total) / total);
                    if fv > 0.0 {
                        c.rect([r[0], r[1], r[0] + (r[2] - r[0]) * fv, r[3]], rad, col);
                    }
                    if over > 0.0 {
                        if inside {
                            let fo = seg(over.min(total) / total);
                            if fo > 0.0 {
                                let ih = (r[3] - r[1]) * 0.36;
                                let my = (r[1] + r[3]) / 2.0;
                                c.rect([r[0], my - ih / 2.0, r[0] + (r[2] - r[0]) * fo, my + ih / 2.0], ih / 2.0, fade(over_col, 0.95));
                            }
                        } else {
                            let (o0, o1) = (seg(shown.min(max) / total), seg((shown.min(max) + over) / total));
                            if o1 > o0 {
                                c.rect([r[0] + (r[2] - r[0]) * o0, r[1], r[0] + (r[2] - r[0]) * o1, r[3]], rad, over_col);
                            }
                        }
                    }
                }
                // a gleam running along it now and then
                let gk = ((t * 0.31 + key as f32 * 0.17).fract() * 2.0 - 0.5).clamp(0.0, 1.0);
                if gk > 0.0 && gk < 1.0 && !segs {
                    let gx = bx[0] + (bx[2] - bx[0]) * gk * (shown.min(total) / total);
                    c.glow([gx, (bx[1] + bx[3]) / 2.0], thick * 1.6, grey(1.0, 0.12 * (std::f32::consts::PI * gk).sin()));
                }
                let mut ww = bx[2] - cx;
                if show_num {
                    words.push([bx[2] + 6.0 * s, cy + num_size * 0.36, num_size]);
                    ww += 6.0 * s + num_w(num_size);
                }
                w = ww;
            }
        }
        // from the right: the shapes turned about the middle, the number put where its room went
        let sum = 2.0 * cx + w;
        if flip {
            c.mirror_x(start, sum);
        }
        for [lx, base, size] in words {
            let room = num_w(size);
            let tw = fonts().width(Face::Semi, size, number);
            let at = if flip { sum - lx - room + (room - tw) } else { lx };
            c.text(at, base, size, Face::Semi, number, x.el.text);
        }
        [w + 2.0 * p, mid_h + 2.0 * p]
    }

    /// A bar of the body with its plate (when it has one) behind it.
    #[allow(clippy::too_many_arguments)]
    fn body_bar_plated(&mut self, c: &mut Canvas, x: &Ctx, key: usize, v: f32, max: f32, over: f32, inside: bool, col: Rgba, over_col: Rgba, icon: (u32, u32), number: &str, pulse: f32) -> Option<[f32; 2]> {
        let start = c.p.len();
        let gr = c.glass_rects.len();
        let size = self.body_bar(c, x, key, v, max, over, inside, col, over_col, icon, number, pulse);
        if x.el.bg {
            // the plate goes under what was drawn: drawn after, moved before it
            let body: Vec<f32> = c.p.split_off(start);
            let glass: Vec<[f32; 4]> = c.glass_rects.split_off(gr);
            x.plate(c, [M, M, M + size[0], M + size[1]]);
            c.p.extend_from_slice(&body);
            c.glass_rects.extend_from_slice(&glass);
        }
        Some(size)
    }

    pub(super) fn el_health(&mut self, c: &mut Canvas, x: &Ctx) -> Option<[f32; 2]> {
        if self.shelf_holds("hb-health") {
            return None;
        }
        let b = self.body_now(x)?;
        if !b.mortal() && !x.sample {
            return None;
        }
        let mut col = x.col("Health colour", x.el.accent);
        if x.on("Effect tints") {
            col = match b.heart {
                1 => tint(col, 0x8db84a, 0.75),
                2 => tint(col, 0x3c3c3c, 0.8),
                3 => tint(col, 0x9fd8ff, 0.7),
                _ => col,
            };
        }
        let low = x.num("Low health") / 100.0;
        let frac = b.hp / b.hp_max.max(1.0);
        let pulse = if x.on("Low health pulse") && frac <= low { 1.0 } else { 0.0 };
        let num = format!("{}", b.hp.ceil() as i32 + b.absorb.ceil() as i32);
        let ab = x.col("Absorption colour", [0.95, 0.8, 0.42, 1.0]);
        self.body_bar_plated(c, x, 0, b.hp, b.hp_max, b.absorb, false, col, ab, HEART, &num, pulse)
    }

    pub(super) fn el_food(&mut self, c: &mut Canvas, x: &Ctx) -> Option<[f32; 2]> {
        if self.shelf_holds("hb-food") {
            return None;
        }
        let b = self.body_now(x)?;
        if !b.mortal() && !x.sample {
            return None;
        }
        let mut col = x.col("Food colour", x.el.accent);
        if x.on("Effect tints") && b.hunger {
            col = tint(col, 0x8db84a, 0.75);
        }
        let pulse = if x.on("Low food pulse") && b.food <= 6.0 { 1.0 } else { 0.0 };
        let sat = if x.on("Saturation") { b.sat } else { 0.0 };
        let sc = x.col("Saturation colour", [0.95, 0.8, 0.42, 1.0]);
        let num = format!("{}", b.food as i32);
        self.body_bar_plated(c, x, 1, b.food, 20.0, sat, true, col, sc, FOOD, &num, pulse)
    }

    pub(super) fn el_armor(&mut self, c: &mut Canvas, x: &Ctx) -> Option<[f32; 2]> {
        if self.shelf_holds("hb-armor") {
            return None;
        }
        let b = self.body_now(x)?;
        if (!b.mortal() || b.armor <= 0.0 && x.on("Hide with none")) && !x.sample {
            return None;
        }
        let col = x.col("Armour colour", x.el.accent);
        let num = if x.on("Toughness") && b.tough > 0.0 { format!("{} · {}", b.armor as i32, b.tough.round() as i32) } else { format!("{}", b.armor as i32) };
        self.body_bar_plated(c, x, 2, b.armor, 20.0, 0.0, false, col, col, ARMOR, &num, 0.0)
    }

    pub(super) fn el_air(&mut self, c: &mut Canvas, x: &Ctx) -> Option<[f32; 2]> {
        if self.shelf_holds("hb-air") {
            return None;
        }
        let b = self.body_now(x)?;
        if (!b.mortal() || !b.in_water && b.air >= b.air_max) && !x.sample {
            return None;
        }
        let col = x.col("Air colour", [0.62, 0.85, 1.0, 1.0]);
        let v = b.air / b.air_max * 20.0;
        let num = format!("{}", (b.air / 20.0).ceil().max(0.0) as i32);
        self.body_bar_plated(c, x, 3, v, 20.0, 0.0, false, col, col, BUBBLE, &num, if b.air < b.air_max * 0.2 { 1.0 } else { 0.0 })
    }

    pub(super) fn el_mount(&mut self, c: &mut Canvas, x: &Ctx) -> Option<[f32; 2]> {
        if self.shelf_holds("hb-mount") {
            return None;
        }
        let b = self.body_now(x)?;
        if b.mount_max <= 0.0 && b.jump < 0.0 && !x.sample {
            return None;
        }
        let s = x.s;
        let col = x.col("Mount colour", x.el.accent);
        let num = format!("{}", b.mount_hp.ceil() as i32);
        let start = c.p.len();
        let gr = c.glass_rects.len();
        let mut size = if b.mount_max > 0.0 { self.body_bar(c, x, 4, b.mount_hp, b.mount_max, 0.0, false, col, col, HEART, &num, 0.0) } else { [0.0, 0.0] };
        // the jump being charged: a thin line under it
        if x.on("Jump") && b.jump >= 0.0 {
            let jc = x.col("Jump colour", [0.95, 0.8, 0.42, 1.0]);
            let p = if x.el.bg { x.pad() } else { 0.0 };
            let w = (x.num("Width") * s).max(40.0 * s);
            let y = M + size[1].max(2.0 * p) - p + 5.0 * s;
            let jk = self.an(ak(A_HB, 120), b.jump, 14.0, b.jump);
            c.rect([M + p, y, M + p + w, y + 3.0 * s], 1.5 * s, grey(0.0, 0.32));
            c.rect([M + p, y, M + p + w, y + 3.0 * s], 1.5 * s, grey(1.0, 0.08));
            c.rect([M + p, y, M + p + w * jk, y + 3.0 * s], 1.5 * s, jc);
            size = [size[0].max(w + 2.0 * p), size[1].max(2.0 * p) + 8.0 * s];
        }
        if x.el.bg {
            let body: Vec<f32> = c.p.split_off(start);
            let glass: Vec<[f32; 4]> = c.glass_rects.split_off(gr);
            x.plate(c, [M, M, M + size[0], M + size[1]]);
            c.p.extend_from_slice(&body);
            c.glass_rects.extend_from_slice(&glass);
        }
        Some(size)
    }

    pub(super) fn el_xp(&mut self, c: &mut Canvas, x: &Ctx) -> Option<[f32; 2]> {
        if self.shelf_holds("hb-xp") {
            return None;
        }
        let b = self.body_now(x)?;
        if !b.mortal() && !x.sample {
            return None;
        }
        let s = x.s;
        let t = x.t;
        let p = if x.el.bg { x.pad() } else { 0.0 };
        let col = x.col("Experience colour", x.el.accent);
        let kind = x.pick("Kind");
        let width = x.num("Width").max(20.0) * s;
        let thick = x.num("Thickness").max(1.0) * s * if kind == 1 { 2.0 } else { 1.0 };
        // a new level: a flash
        let flash = self.level_flash(b.level, t);
        let flash = if x.on("Level up flash") { flash } else { 0.0 };
        let xp = self.an(ak(A_HB, 130), b.xp, 8.0, b.xp);
        let lv = b.level.to_string();
        let ls = if kind == 2 { 22.0 * s } else { 12.0 * s };
        let lw = fonts().width(Face::Semi, ls, &lv);
        let start = c.p.len();
        let gr = c.glass_rects.len();
        let size;
        if kind == 2 {
            // the level only
            let sub = tr("Level").to_uppercase();
            let ss = 8.0 * s;
            let sw = fonts().width(Face::Caps, ss, &sub);
            let w = lw.max(sw) + 2.0 * p;
            let h = ls * 1.1 + ss * 1.6 + 2.0 * p;
            c.text_c(M + w / 2.0, M + p + ls, ls, Face::Semi, &lv, x.el.text);
            c.text_c(M + w / 2.0, M + p + ls * 1.1 + ss * 1.3, ss, Face::Caps, &sub, x.el.text2);
            if flash > 0.0 {
                c.glow([M + w / 2.0, M + p + ls * 0.6], ls * 1.4, fade(col, 0.5 * flash));
            }
            size = [w, h];
        } else {
            let at = x.pick("Level at");
            let show_lv = x.on("Level") && b.level > 0 || x.sample && x.on("Level");
            let (mut bx0, top) = (M + p, M + p);
            let line_y;
            let mut w = width;
            let h;
            // in the middle: the line parts around the level
            let mut gap: Option<[f32; 2]> = None;
            if show_lv && at == 0 {
                line_y = top + ls * 0.5 - thick / 2.0;
                h = ls.max(thick);
                let mx = bx0 + width / 2.0;
                c.text_c(mx, top + ls * 0.86, ls, Face::Semi, &lv, mix(x.el.text, col, flash));
                gap = Some([mx - lw / 2.0 - 7.0 * s, mx + lw / 2.0 + 7.0 * s]);
            } else if show_lv && at == 1 {
                line_y = top + ls * 1.25;
                h = ls * 1.25 + thick;
                c.text_c(bx0 + width / 2.0, top + ls, ls, Face::Semi, &lv, mix(x.el.text, col, flash));
            } else if show_lv && at == 2 {
                line_y = top + ls * 0.5 - thick / 2.0;
                h = ls.max(thick);
                c.text(bx0, top + ls * 0.86, ls, Face::Semi, &lv, mix(x.el.text, col, flash));
                bx0 += lw + 6.0 * s;
                w += lw + 6.0 * s;
            } else if show_lv && at == 3 {
                line_y = top + ls * 0.5 - thick / 2.0;
                h = ls.max(thick);
                c.text(bx0 + width + 6.0 * s, top + ls * 0.86, ls, Face::Semi, &lv, mix(x.el.text, col, flash));
                w += lw + 6.0 * s;
            } else {
                line_y = top;
                h = thick;
            }
            let r = [bx0, line_y, bx0 + width, line_y + thick];
            let fx = r[0] + width * xp;
            // its pieces: the whole line, or the two halves around the level
            let pieces: Vec<[f32; 2]> = match gap {
                Some(g) => vec![[r[0], g[0]], [g[1], r[2]]],
                None => vec![[r[0], r[2]]],
            };
            for [a, z] in pieces {
                if z - a < 1.0 {
                    continue;
                }
                c.rect([a, r[1], z, r[3]], thick / 2.0, grey(0.0, 0.42));
                c.rect([a, r[1], z, r[3]], thick / 2.0, grey(1.0, 0.07));
                let e = fx.clamp(a, z);
                if e > a + 0.5 {
                    c.rect([a, r[1], e, r[3]], thick / 2.0, col);
                }
                if flash > 0.0 {
                    c.rect([a, r[1] - thick, z, r[3] + thick], thick * 1.5, fade(col, 0.25 * flash));
                }
            }
            size = [w + 2.0 * p, h + 2.0 * p];
        }
        if x.el.bg {
            let body: Vec<f32> = c.p.split_off(start);
            let glass: Vec<[f32; 4]> = c.glass_rects.split_off(gr);
            x.plate(c, [M, M, M + size[0], M + size[1]]);
            c.p.extend_from_slice(&body);
            c.glass_rects.extend_from_slice(&glass);
        }
        Some(size)
    }
}
