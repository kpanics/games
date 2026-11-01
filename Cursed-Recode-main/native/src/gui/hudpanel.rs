//! The Hud module (Interface): all the HUD's elements in one place. Its panel: on the left the chosen element live
//! over the world and every element as a card (the HUD's look first), on the right the chosen one's settings. A module
//! of the Interface made of HUD elements of its own (the Custom Hotbar: its parts) has the same panel for them.

use super::data::Cat;
use super::draw::{grey, mix, Canvas};
use super::i18n::tr;
use super::text::{fonts, Face};
use super::theme::pal;
use super::{ak, ease_out, smooth, switch, Gui, Hit, Owner, A_HOV, A_SW};

impl Gui {
    /// The group of elements of the module open (the Custom Hotbar's parts), or None: the Hud's own.
    pub(super) fn open_group(&self) -> Option<&'static str> {
        self.open.and_then(|o| self.mods.get(o)).and_then(|m| match m.id {
            "custom-hotbar" => Some("Custom Hotbar"),
            "custom-f3" => Some("Custom F3"),
            "custom-chat" => Some("Custom Chat"),
            "custom-scoreboard" => Some("Custom Scoreboard"),
            "other-custom" => Some("Other Custom Things"),
            _ => None,
        })
    }

    /// The elements' cards in order (a module's own group: only its parts; not those on the hotbar's shelf). The look
    /// of them all is the Theme's.
    pub(super) fn hud_cards(&self) -> Vec<usize> {
        let group = self.open_group();
        (0..self.mods.len())
            .filter(|&i| self.mods[i].cat == Cat::HudEl && !self.mods[i].plain && self.mods[i].sub == group && !self.shelf_holds(self.mods[i].id))
            .collect()
    }

    pub(super) fn is_hud_module(&self, i: usize) -> bool {
        self.mods.get(i).is_some_and(|m| matches!(m.id, "hud" | "custom-hotbar" | "custom-f3" | "custom-chat" | "custom-scoreboard" | "other-custom"))
    }

    fn hud_grid(&self, w: f32) -> (usize, f32, f32) {
        let r = self.r;
        let cols = if w > 25.0 * r { 2 } else { 1 };
        let ch = 3.7 * r;
        let gap = 0.5 * r;
        (cols, ch, gap)
    }

    pub(super) fn hud_panel_height(&self, total_w: f32) -> f32 {
        let r = self.r;
        let lw = total_w * 0.6;
        let (cols, ch, gap) = self.hud_grid(lw - 1.0 * r);
        let n = self.hud_cards().len();
        let rows = n.div_ceil(cols);
        let left = 1.0 * r + 12.5 * r + 0.9 * r + rows as f32 * (ch + gap) + 0.4 * r + 2.6 * r + 0.6 * r;
        let sel = self.hud_sel.min(self.mods.len() - 1);
        let pw = total_w * 0.4 - 2.2 * r;
        let desc = fonts().wrap(Face::Regular, 0.8 * r, tr(self.mods[sel].desc), pw).len() as f32;
        let right = 3.6 * r + desc * 1.2 * r + 1.0 * r + self.rows_height(&self.mods[sel].set, pw) + 3.8 * r;
        (left + 2.6 * r).max(right)
    }

    pub(super) fn hud_panel(&mut self, c: &mut Canvas, rr: [f32; 4], hud_i: usize) {
        let r = self.r;
        let opened = self.now - self.open_at;
        let reveal = (opened / 0.6).clamp(0.0, 1.0);
        c.rect(rr, r, grey(1.0, 0.05));
        self.hits.push((rr, Hit::Panel(hud_i)));
        let cards = self.hud_cards();
        if !cards.contains(&self.hud_sel) {
            self.hud_sel = cards[0];
        }
        let sel = self.hud_sel;
        let px = rr[0] + (rr[2] - rr[0]) * 0.6;
        // left: the chosen element over the world, then the cards
        let st = [rr[0] + 0.5 * r, rr[1] + 0.5 * r, px - 0.1 * r, rr[1] + 13.0 * r];
        self.hud_preview(c, st, sel, reveal);
        let (cols, ch, gap) = self.hud_grid(px - rr[0] - 1.0 * r);
        let gx0 = rr[0] + 0.5 * r;
        let gw = px - 0.1 * r - gx0;
        let cw = (gw - gap * (cols - 1) as f32) / cols as f32;
        let gy0 = st[3] + 0.9 * r;
        let base = c.alpha;
        for (n, &j) in cards.iter().enumerate() {
            let (col, row) = (n % cols, n / cols);
            let k = ((opened - 0.1 - n as f32 * 0.03) / 0.35).clamp(0.0, 1.0);
            c.alpha = base * smooth(0.0, 1.0, k);
            let dy = (1.0 - ease_out(k)) * 0.35 * r;
            let q = [gx0 + col as f32 * (cw + gap), gy0 + row as f32 * (ch + gap) + dy, gx0 + col as f32 * (cw + gap) + cw, gy0 + row as f32 * (ch + gap) + ch + dy];
            let chosen = j == sel;
            let hit = Hit::HudPick(j);
            let hv = self.an(ak(A_HOV, 12_000 + j), if self.hover == hit || chosen { 1.0 } else { 0.0 }, 12.0, 0.0);
            let cv = self.an(ak(A_HOV, 12_500 + j), if chosen { 1.0 } else { 0.0 }, 12.0, 0.0);
            c.rect_lit(q, 0.8 * r, mix(pal().f1, pal().f3, hv * 0.6 + cv * 0.4), grey(1.0, 0.07 * cv), 2.0);
            self.hits.push((q, hit));
            let m = &self.mods[j];
            let plain = m.plain;
            let on = m.on;
            let name = if plain { tr(m.name).to_string() } else { super::i18n::nm(m.name).to_string() };
            let right = if plain { q[2] - 1.0 * r } else { q[2] - 3.4 * r };
            let nm = fonts().fit(Face::Semi, 0.84 * r, &name, right - q[0] - 1.1 * r);
            c.text(q[0] + 1.0 * r, q[1] + 1.65 * r, 0.84 * r, Face::Semi, &nm, if chosen || on || plain { pal().t1 } else { pal().t2 });
            let line = if plain {
                tr("Size, colours, glass, motion").to_string()
            } else {
                let keys = self.mods[j].set.iter().filter(|s| matches!(s.kind, super::data::Kind::Toggle(true))).count();
                format!("{} · {}", tr(if on { "On" } else { "Off" }), super::i18n::trn("{n} shown", &[("n", keys.to_string())]))
            };
            let ln = fonts().fit(Face::Regular, 0.66 * r, &line, right - q[0] - 1.1 * r);
            c.text(q[0] + 1.0 * r, q[1] + 2.75 * r, 0.66 * r, Face::Regular, &ln, pal().t4);
            if !plain {
                let onv = self.an(ak(A_SW, j), if on { 1.0 } else { 0.0 }, 10.0, if on { 1.0 } else { 0.0 });
                let sw = [q[2] - 3.0 * r, q[1] + (ch - 1.24 * r) / 2.0, q[2] - 0.9 * r, q[1] + (ch + 1.24 * r) / 2.0];
                let shv = self.an(ak(A_HOV, 12_900 + j), if self.hover == Hit::TileSwitch(j) { 1.0 } else { 0.0 }, 12.0, 0.0);
                switch(c, sw, onv, shv, true);
                self.hits.push(([sw[0] - 0.3 * r, q[1], q[2], q[3]], Hit::TileSwitch(j)));
            }
        }
        // the "+": an element of one's own of each kind (the Hud's own panel)
        let rows = cards.len().div_ceil(cols);
        let ay = gy0 + rows as f32 * (ch + gap) + 0.4 * r;
        let aw = (gw - 2.0 * gap) / 3.0;
        let kinds: &[(&str, &str)] = if self.open_group().is_none() { &super::hudx::KINDS } else { &[] };
        for (k, (_, label)) in kinds.iter().enumerate() {
            let q = [gx0 + k as f32 * (aw + gap), ay, gx0 + k as f32 * (aw + gap) + aw, ay + 2.4 * r];
            let hit = Hit::HudAdd(k);
            let hv = self.an(ak(A_HOV, 12_950 + k), if self.hover == hit { 1.0 } else { 0.0 }, 12.0, 0.0);
            c.rect(q, 0.7 * r, mix(pal().f1, pal().f3, hv));
            let lbl = format!("+  {}", tr(label));
            c.text_c((q[0] + q[2]) / 2.0, q[1] + 1.55 * r, 0.8 * r, Face::Medium, &lbl, mix(pal().t2, pal().t1, hv));
            self.hits.push((q, hit));
        }
        c.alpha = base;
        // right: the chosen one's name, switch, close; its words; its settings
        let x0 = px + 1.1 * r;
        let x1 = rr[2] - 1.0 * r;
        let rise = move |d: f32| -> (f32, f32) {
            let k = ((opened - d) / 0.4).clamp(0.0, 1.0);
            (smooth(0.0, 1.0, k), (1.0 - ease_out(k)) * 0.35 * r)
        };
        let (a, dy) = rise(0.12);
        c.alpha = base * a;
        let y = rr[1] + 1.05 * r + dy;
        let xb = [x1 - 1.95 * r, y, x1, y + 1.95 * r];
        let hxv = self.an(ak(A_HOV, 2000), if self.hover == Hit::PanelClose { 1.0 } else { 0.0 }, 12.0, 0.0);
        c.rect(xb, 0.6 * r, mix(pal().f1, pal().f3, hxv));
        let xc = [xb[0] + 0.975 * r, xb[1] + 0.975 * r];
        let xs = 0.3 * r;
        let xcol = mix(pal().t3, pal().t1, hxv);
        c.line([xc[0] - xs, xc[1] - xs], [xc[0] + xs, xc[1] + xs], 1.5, xcol, 0.0);
        c.line([xc[0] + xs, xc[1] - xs], [xc[0] - xs, xc[1] + xs], 1.5, xcol, 0.0);
        self.hits.push((xb, Hit::PanelClose));
        let mut right = xb[0] - 0.65 * r;
        let m = self.head(sel);
        if !m.plain {
            let onv = self.an(ak(A_SW, sel), if m.on { 1.0 } else { 0.0 }, 10.0, 0.0);
            let sw = [right - 2.3 * r, y + 0.3 * r, right, y + 1.66 * r];
            let hv = self.an(ak(A_HOV, 2001), if self.hover == Hit::PanelSwitch(sel) { 1.0 } else { 0.0 }, 12.0, 0.0);
            switch(c, sw, onv, hv, false);
            self.hits.push((sw, Hit::PanelSwitch(sel)));
            right = sw[0] - 0.65 * r;
        }
        let title = if m.plain { tr(m.name).to_string() } else { super::i18n::nm(m.name).to_string() };
        let name = fonts().fit(Face::Bold, 1.32 * r, &title, right - x0);
        c.text(x0, y + 1.42 * r, 1.32 * r, Face::Bold, &name, pal().t1);
        let mut yy = y + 2.6 * r;
        let (a, dy2) = rise(0.16);
        c.alpha = base * a;
        for line in fonts().wrap(Face::Regular, 0.8 * r, tr(m.desc), x1 - x0) {
            c.text(x0, yy + 0.9 * r + dy2 - dy, 0.8 * r, Face::Regular, &line, pal().t3);
            yy += 1.2 * r;
        }
        yy += 0.85 * r;
        self.module_code_buttons(c, sel, [x0, yy, x1, yy + 1.9 * r]);
        yy += 2.6 * r;
        self.module_code_buttons(c, hud_i, [gx0, ay + 3.0 * r, gx0 + gw, ay + 4.9 * r]);
        c.alpha = base;
        self.rows(c, Owner::Module(sel), x0, x1, yy, rise);
    }

    /// A press on the Hud's panel that is its own: a card chosen.
    pub(super) fn hud_panel_press(&mut self, h: Hit) -> bool {
        if let Hit::HudAdd(k) = h {
            let kind = super::hudx::KINDS[k.min(2)].0;
            let i = self.custom_add(kind);
            self.hud_sel = i;
            self.open_at = self.now - 0.45;
            self.switch_sound(true, 0.7);
            return true;
        }
        let Hit::HudPick(j) = h else { return false };
        if self.hud_sel != j {
            self.hud_sel = j;
            self.open_at = self.now - 0.45;
            self.pick = None;
            self.pop = None;
            self.text_focus = None;
            self.sound_tick(0.55);
        }
        true
    }
}
