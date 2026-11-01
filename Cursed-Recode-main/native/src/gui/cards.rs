//! Binds in the menu and in the world: the card of a module's or a setting's binds (any number, each a combination,
//! pressed or held), every bind on one page, recording a combination, and what the keys do in the world.

use super::binds::{self, Bind};
use super::data::Kind;
use super::draw::{grey, mix, Canvas};
#[allow(unused_imports)]
use super::text;
use super::i18n::{self, tr};
use super::text::{fonts, Face};
use super::theme::{self, pal};
use super::{ak, BindTo, Card, Gui, Hit, Listen, Owner, A_HOV, A_POP};
use super::hudx::PLACEHOLDERS;

/// What a held bind put back when its keys go: a module's switch or a setting as it was.
#[derive(Clone)]
pub enum Prior {
    Module(bool),
    Setting(Kind),
}

/// The binds of a list as a tile shows them: the first, and how many more.
pub fn binds_label(list: &[Bind]) -> String {
    match list.len() {
        0 => String::new(),
        1 => list[0].label(),
        n => format!("{}  +{}", list[0].label(), n - 1),
    }
}

/// The binds' icon: a keyboard `s` wide round `cc` (shaders/gui/gui.comp keysIcon); `typing` seconds since the pointer
/// came onto it (-1 not): its keys light one after another.
pub fn key_icon(c: &mut Canvas, cc: [f32; 2], s: f32, typing: f32, col: super::draw::Rgba) {
    let h = 0.6 * s;
    c.icon_t([cc[0] - h, cc[1] - h, cc[0] + h, cc[1] + h], 45, typing, col);
}

impl Gui {
    // ------------------------------------------------------------------ what binds are on
    pub(super) fn binds_of(&self, to: BindTo) -> &Vec<Bind> {
        match to {
            BindTo::Module(i) => &self.mods[i].binds,
            BindTo::Setting(o, si) => &self.set_ref(o)[si].binds,
        }
    }

    pub(super) fn binds_mut(&mut self, to: BindTo) -> &mut Vec<Bind> {
        match to {
            BindTo::Module(i) => &mut self.mods[i].binds,
            BindTo::Setting(o, si) => &mut self.set_list(o)[si].binds,
        }
    }

    /// What it is called: the module, or the module (or the page) and the setting.
    pub(super) fn bind_name(&self, to: BindTo) -> (String, String) {
        match to {
            BindTo::Module(i) => {
                let m = &self.mods[i];
                (if m.plain { tr(m.name).to_string() } else { i18n::nm(m.name).to_string() }, String::new())
            }
            BindTo::Setting(o, si) => {
                let owner = match o {
                    Owner::Module(i) => {
                        let m = &self.mods[i];
                        if m.plain { tr(m.name).to_string() } else { i18n::nm(m.name).to_string() }
                    }
                    Owner::Client => tr("Settings").to_string(),
                    Owner::Theme => tr("Theme").to_string(),
                    Owner::Cosm => tr("Cosmetics").to_string(),
                };
                (owner, tr(self.set_ref(o)[si].label).to_string())
            }
        }
    }

    /// Whether a setting can be bound, and what a bind does to it.
    pub(super) fn bind_does(&self, to: BindTo) -> Option<&'static str> {
        match to {
            BindTo::Module(i) if self.mods[i].id == super::swap::PEARL => Some("Each press throws a pearl. The switch on its tile turns it on."),
            BindTo::Module(_) => Some("Each press switches it on or off; held, it is on while the keys are down."),
            BindTo::Setting(o, si) => match self.set_ref(o).get(si).map(|s| &s.kind) {
                Some(Kind::Toggle(_)) => Some("Each press flips it; held, it is flipped while the keys are down."),
                Some(Kind::Choice { .. }) => Some("Each press picks the next option; held, the next one while the keys are down."),
                Some(Kind::Slider { .. }) => Some("Each press moves it a quarter on, round to the start; held, to its end while the keys are down."),
                Some(Kind::Multi { .. }) => Some("Each press turns on the next option alone; held, all of them while the keys are down."),
                Some(Kind::Colour(_)) => Some("Each press turns its chroma on or off."),
                Some(Kind::Action) => Some("Each press does it."),
                _ => None,
            },
        }
    }

    /// The combination being recorded, as it stands.
    pub(super) fn rec_label(&self) -> String {
        let mut keys: Vec<String> = self.gheld.iter().filter(|k| binds::modifier(k)).cloned().collect();
        for k in &self.rec {
            if !keys.contains(k) {
                keys.push(k.clone());
            }
        }
        if keys.is_empty() {
            tr("Press keys").to_string()
        } else {
            format!("{} + …", Bind { keys: binds::canonical(keys), hold: false, value: None }.label())
        }
    }

    pub(super) fn start_bind(&mut self, to: BindTo, slot: Option<usize>) {
        if self.bind_does(to).is_none() {
            return;
        }
        self.listen = Some(Listen::Bind(to, slot));
        self.rec.clear();
        self.sound_tick(0.6);
    }

    /// The combination is in: it replaces the bind it was for, or joins the list (the same keys are never twice).
    pub(super) fn finish_bind(&mut self, to: BindTo, slot: Option<usize>, keys: Vec<String>) {
        let keys = binds::canonical(keys);
        self.listen = None;
        self.rec.clear();
        if keys.is_empty() {
            return;
        }
        let list = self.binds_mut(to);
        let hold = slot.and_then(|k| list.get(k)).is_some_and(|b| b.hold);
        let value = slot.and_then(|k| list.get(k)).and_then(|b| b.value.clone());
        let b = Bind { keys, hold, value };
        match slot {
            Some(k) if k < list.len() => {
                if list.iter().enumerate().any(|(j, x)| j != k && x.keys == b.keys) {
                    list.remove(k);
                } else {
                    list[k] = b;
                }
            }
            _ => {
                if !list.iter().any(|x| x.keys == b.keys) {
                    list.push(b);
                }
            }
        }
        self.sound_tick(0.8);
        self.save();
    }

    pub(super) fn open_binds(&mut self, to: BindTo, at: [f32; 4]) {
        if self.bind_does(to).is_none() {
            return;
        }
        // its own card just closed by this press: it stays closed
        if matches!(self.card_shut.take(), Some(Card::Binds { to: t, .. }) if t == to) {
            return;
        }
        self.card = Some(Card::Binds { to, at });
        self.card_at = self.now;
        self.anim.remove(&ak(A_POP, 2));
        self.pop = None;
        self.pick = None;
        self.sound_tick(0.6);
    }

    /// The other things bound to the same keys (shown by a bind, to be seen before it surprises).
    fn clashes(&self, to: BindTo, b: &Bind) -> Vec<String> {
        let mut out = Vec::new();
        for (t, list) in self.all_binds() {
            if t == to {
                continue;
            }
            if list.iter().any(|x| x.keys == b.keys) {
                let (a, s) = self.bind_name(t);
                out.push(if s.is_empty() { a } else { format!("{a} · {s}") });
            }
        }
        out
    }

    /// Every bound thing with its binds: the modules, then the settings of each owner.
    pub(super) fn all_binds(&self) -> Vec<(BindTo, Vec<Bind>)> {
        let mut v = Vec::new();
        for (i, m) in self.mods.iter().enumerate() {
            if !m.binds.is_empty() {
                v.push((BindTo::Module(i), m.binds.clone()));
            }
            for (si, s) in m.set.iter().enumerate() {
                if !s.binds.is_empty() {
                    v.push((BindTo::Setting(Owner::Module(i), si), s.binds.clone()));
                }
            }
        }
        for (si, s) in self.client.iter().enumerate() {
            if !s.binds.is_empty() {
                v.push((BindTo::Setting(Owner::Client, si), s.binds.clone()));
            }
        }
        for (si, s) in self.theme.iter().enumerate() {
            if !s.binds.is_empty() {
                v.push((BindTo::Setting(Owner::Theme, si), s.binds.clone()));
            }
        }
        v
    }

    // ------------------------------------------------------------------ the card
    pub(super) fn card(&mut self, c: &mut Canvas) {
        let (to, at) = match self.card.clone() {
            Some(Card::Binds { to, at }) => (to, at),
            Some(Card::Lang { at }) => return self.lang_card(c, at),
            Some(Card::Alts { at }) => return self.alts_card(c, at),
            Some(Card::Placeholders { i, at }) => return self.ph_card(c, i, at),
            Some(Card::Deaths { at }) => return self.deaths_card(c, at),
            Some(Card::Friends { at }) => return self.friends_card(c, at),
            Some(Card::Lines { i, tab, at }) => return self.lines_card(c, i, tab, at),
            Some(Card::Cloud { at, tab, page }) => return self.cloud_card(c, at, tab, page),
            Some(_) => return self.cfg_card(c),
            None => {
                self.anim.remove(&ak(A_POP, 2));
                return;
            }
        };
        let r = self.r;
        let k = self.an(ak(A_POP, 2), 1.0, 14.0, 0.0);
        let list = self.binds_of(to).clone();
        let recording_new = self.listen == Some(Listen::Bind(to, None));
        let w = 25.0 * r;
        let (who, what) = self.bind_name(to);
        // a setting with values to choose: each bind can set one (a second line under its keys)
        let kind = match to {
            BindTo::Setting(o, si) => Some(self.set_ref(o)[si].kind.clone()),
            BindTo::Module(_) => None,
        };
        let values = kind.as_ref().map_or(Vec::new(), bind_values);
        let does = tr(self.bind_does(to).unwrap_or("")).to_string();
        let does = if values.is_empty() { does } else { format!("{does} {}", tr("Or a value of its own, chosen under the keys: pressed again, it goes back to what was before.")) };
        let does_lines = fonts().wrap(Face::Regular, 0.72 * r, &does, w - 2.4 * r);
        let row_h = if values.is_empty() { 2.7 * r } else { 4.55 * r };
        let extra = if recording_new || list.is_empty() { 2.7 * r } else { 0.0 };
        let h = 4.4 * r + list.len() as f32 * row_h + extra + 3.2 * r + does_lines.len() as f32 * 1.05 * r + 1.0 * r;
        let mut x = at[2] - w;
        let mut y = at[3] + 0.5 * r;
        if y + h > self.h - 8.0 {
            y = (at[1] - h - 0.5 * r).max(8.0);
        }
        x = x.clamp(8.0, (self.w - w - 8.0).max(8.0));
        let rr = [x, y - (1.0 - k) * 0.5 * r, x + w, y + h - (1.0 - k) * 0.5 * r];
        let base = c.alpha;
        c.alpha = k;
        c.shadow(rr, r, 3.0 * r, grey(0.0, 0.62));
        let g = pal().glass;
        c.rect(rr, r, [g[0].max(0.05), g[1].max(0.05), g[2].max(0.055), 0.97]);
        self.hits.push((rr, Hit::Card(0, 0)));
        let (x0, x1) = (rr[0] + 1.2 * r, rr[2] - 1.2 * r);
        // what: the module, the setting under it; close
        let xb = [x1 - 1.8 * r, rr[1] + 0.95 * r, x1, rr[1] + 2.75 * r];
        let xv = self.an(ak(A_HOV, 9840), if self.hover == Hit::Card(6, 0) { 1.0 } else { 0.0 }, 12.0, 0.0);
        c.rect(xb, 0.55 * r, mix(pal().f1, pal().f3, xv));
        let xc = [(xb[0] + xb[2]) / 2.0, (xb[1] + xb[3]) / 2.0];
        let xs = 0.27 * r;
        let xcol = mix(pal().t3, pal().t1, xv);
        c.line([xc[0] - xs, xc[1] - xs], [xc[0] + xs, xc[1] + xs], 1.5, xcol, 0.0);
        c.line([xc[0] + xs, xc[1] - xs], [xc[0] - xs, xc[1] + xs], 1.5, xcol, 0.0);
        self.hits.push((xb, Hit::Card(6, 0)));
        let title = if what.is_empty() { who.clone() } else { what.clone() };
        let tt = fonts().fit(Face::Bold, 1.05 * r, &title, xb[0] - x0 - 0.5 * r);
        c.text(x0, rr[1] + 2.0 * r, 1.05 * r, Face::Bold, &tt, pal().t1);
        let sub = if what.is_empty() { tr("Binds").to_string() } else { format!("{} · {}", who, tr("Binds")) };
        c.text(x0, rr[1] + 3.2 * r, 0.72 * r, Face::Regular, &fonts().fit(Face::Regular, 0.72 * r, &sub, x1 - x0), pal().t3);
        let mut yy = rr[1] + 4.4 * r;
        if list.is_empty() && !recording_new {
            c.text(x0, yy + 1.55 * r, 0.8 * r, Face::Regular, tr("No binds yet"), pal().t4);
            yy += 2.7 * r;
        }
        for (kk, b) in list.iter().enumerate() {
            let row = [x0 - 0.4 * r, yy, x1 + 0.4 * r, yy + 2.45 * r];
            let mid = (row[1] + row[3]) / 2.0;
            let waiting = self.listen == Some(Listen::Bind(to, Some(kk)));
            // the keys: a chip each, pressed again to record anew
            let label = if waiting { self.rec_label() } else { b.label() };
            let cw = (fonts().width(Face::Mono, 0.72 * r, &label) + 1.1 * r).min(x1 - x0 - 10.5 * r);
            let chip = [x0, mid - 0.8 * r, x0 + cw, mid + 0.8 * r];
            let hit = Hit::Card(1, kk as u16);
            let cv = self.an(ak(A_HOV, 9850 + kk), if self.hover == hit { 1.0 } else { 0.0 }, 12.0, 0.0);
            let pulse = if waiting { 0.5 + 0.5 * (self.now * 7.0).sin() } else { 0.0 };
            c.rect(chip, 0.45 * r, mix(mix(pal().f2, pal().f4, cv), grey(1.0, 0.24), pulse));
            let shown = fonts().fit(Face::Mono, 0.72 * r, &label, cw - 1.0 * r);
            c.text(chip[0] + 0.55 * r, mid + 0.27 * r, 0.72 * r, Face::Mono, &shown, pal().t1);
            self.hits.push((chip, hit));
            // delete
            let db = [x1 - 1.6 * r, mid - 0.8 * r, x1, mid + 0.8 * r];
            let dv = self.an(ak(A_HOV, 9870 + kk), if self.hover == Hit::Card(4, kk as u16) { 1.0 } else { 0.0 }, 12.0, 0.0);
            c.rect(db, 0.45 * r, mix(pal().f1, pal().f3, dv));
            let dc = [(db[0] + db[2]) / 2.0, mid];
            let ds = 0.22 * r;
            c.line([dc[0] - ds, dc[1] - ds], [dc[0] + ds, dc[1] + ds], 1.4, mix(pal().t3, pal().t1, dv), 0.0);
            c.line([dc[0] + ds, dc[1] - ds], [dc[0] - ds, dc[1] + ds], 1.4, mix(pal().t3, pal().t1, dv), 0.0);
            self.hits.push((db, Hit::Card(4, kk as u16)));
            // pressed or held
            let opts = [tr("Press"), tr("Hold")];
            let size = 0.7 * r;
            let ws: Vec<f32> = opts.iter().map(|o| fonts().width(Face::Medium, size, o) + 1.1 * r).collect();
            let total: f32 = ws.iter().sum::<f32>() + 0.3 * r;
            let sx = db[0] - 0.45 * r - total;
            let sh = 1.6 * r;
            let sy = mid - sh / 2.0;
            c.rect([sx, sy, sx + total, sy + sh], 0.5 * r, pal().f1);
            let cur = b.hold as usize;
            let cx = sx + 0.15 * r + if cur == 1 { ws[0] } else { 0.0 };
            let tx = self.an(ak(super::A_SEG, 7000 + kk), cx, 16.0, cx);
            let tw = self.an(ak(super::A_SEGW, 7000 + kk), ws[cur], 16.0, ws[cur]);
            c.rect([tx, sy + 0.15 * r, tx + tw, sy + sh - 0.15 * r], 0.38 * r, pal().f4);
            let mut ox = sx + 0.15 * r;
            for (oi, o) in opts.iter().enumerate() {
                let hit = Hit::Card(2 + oi as u8, kk as u16);
                let ov = self.an(ak(A_HOV, 9890 + kk * 2 + oi), if self.hover == hit { 1.0 } else { 0.0 }, 12.0, 0.0);
                let col = if oi == cur { pal().t1 } else { mix(pal().t3, pal().t2, ov) };
                c.text_c(ox + ws[oi] / 2.0, mid + 0.25 * r, size, Face::Medium, o, col);
                self.hits.push(([ox, sy, ox + ws[oi], sy + sh], hit));
                ox += ws[oi];
            }
            // bound to the same keys elsewhere: said under it, quietly
            let clash = self.clashes(to, b);
            if !clash.is_empty() && !waiting {
                let line = format!("{} {}", tr("Also on"), clash.join(", "));
                let lw = fonts().fit(Face::Regular, 0.6 * r, &line, sx - x0 - cw - 0.8 * r);
                c.text(chip[2] + 0.5 * r, mid + 0.22 * r, 0.6 * r, Face::Regular, &lw, theme::fade(pal().t3, 0.9));
            }
            // what it sets: its own way ("the next one") or a value of the setting's, gone through with ‹ ›
            if let Some(kind) = kind.as_ref().filter(|_| !values.is_empty()) {
                let vm = mid + 1.95 * r;
                let fs = 0.7 * r;
                let sets = tr("Sets it to");
                c.text(x0, vm + 0.25 * r, fs, Face::Medium, sets, pal().t3);
                let lx = x0 + fonts().width(Face::Medium, fs, sets) + 0.6 * r;
                let cur = b.value.as_ref().and_then(|v| values.iter().position(|o| o == v));
                let set = b.value.as_deref().and_then(|v| value_kind(kind, v));
                let text = match &set {
                    Some(k) => value_text(k),
                    None => tr(match kind {
                        Kind::Toggle(_) => "Flips it",
                        Kind::Choice { .. } => "The next one",
                        _ if b.hold => "To the most",
                        _ => "A quarter on",
                    })
                    .to_string(),
                };
                // as wide as its longest value: the arrows stay where they are while it is gone through
                let widest = values
                    .iter()
                    .filter_map(|v| value_kind(kind, v))
                    .map(|k| value_text(&k))
                    .chain(std::iter::once(text.clone()))
                    .map(|t| fonts().width(Face::Medium, fs, &t))
                    .fold(0.0f32, f32::max);
                let cw = (widest + 3.4 * r).min(x1 - lx);
                let chip = [lx, vm - 0.78 * r, lx + cw, vm + 0.78 * r];
                let back = Hit::Card(7, (kk * 2) as u16);
                let on = Hit::Card(7, (kk * 2 + 1) as u16);
                let bv = self.an(ak(A_HOV, 9700 + kk * 2), if self.hover == back { 1.0 } else { 0.0 }, 12.0, 0.0);
                let fv = self.an(ak(A_HOV, 9701 + kk * 2), if self.hover == on { 1.0 } else { 0.0 }, 12.0, 0.0);
                c.rect(chip, 0.45 * r, mix(pal().f1, pal().f3, bv.max(fv)));
                // the value comes in from the side it was gone to
                let at = cur.map_or(0.0, |p| p as f32 + 1.0);
                let lag = self.an(ak(super::A_BVAL, kk * 4 + 1), at, 14.0, at);
                let off = (lag - at).clamp(-1.0, 1.0);
                let fade = 1.0 - off.abs();
                let tc = if set.is_some() { pal().t1 } else { pal().t2 };
                c.text_c((chip[0] + chip[2]) / 2.0 - off * 0.6 * r, vm + 0.25 * r, fs, Face::Medium, &text, theme::fade(tc, fade.max(0.0)));
                let s = 0.2 * r;
                let lc = [chip[0] + 0.85 * r, vm];
                let rc = [chip[2] - 0.85 * r, vm];
                let bc = mix(pal().t4, pal().t1, bv);
                let fc = mix(pal().t4, pal().t1, fv);
                c.line([lc[0] + s * 0.6, lc[1] - s], [lc[0] - s * 0.6, lc[1]], 1.4, bc, 0.0);
                c.line([lc[0] - s * 0.6, lc[1]], [lc[0] + s * 0.6, lc[1] + s], 1.4, bc, 0.0);
                c.line([rc[0] - s * 0.6, rc[1] - s], [rc[0] + s * 0.6, rc[1]], 1.4, fc, 0.0);
                c.line([rc[0] + s * 0.6, rc[1]], [rc[0] - s * 0.6, rc[1] + s], 1.4, fc, 0.0);
                let half = (chip[0] + chip[2]) / 2.0;
                self.hits.push(([chip[0], chip[1], half, chip[3]], back));
                self.hits.push(([half, chip[1], chip[2], chip[3]], on));
            }
            yy += row_h;
        }
        if recording_new {
            let mid = yy + 1.22 * r;
            let label = self.rec_label();
            let cw = fonts().width(Face::Mono, 0.72 * r, &label) + 1.1 * r;
            let pulse = 0.5 + 0.5 * (self.now * 7.0).sin();
            c.rect([x0, mid - 0.8 * r, x0 + cw, mid + 0.8 * r], 0.45 * r, mix(pal().f2, grey(1.0, 0.24), pulse));
            c.text(x0 + 0.55 * r, mid + 0.27 * r, 0.72 * r, Face::Mono, &label, pal().t1);
            c.text_r(x1, mid + 0.25 * r, 0.66 * r, Face::Regular, tr("Esc cancels"), pal().t4);
            yy += 2.7 * r;
        }
        // add one more
        let ab = [x0, yy + 0.3 * r, x1, yy + 2.3 * r];
        let av = self.an(ak(A_HOV, 9880), if self.hover == Hit::Card(5, 0) { 1.0 } else { 0.0 }, 12.0, 0.0);
        c.rect(ab, 0.6 * r, mix(pal().f2, pal().f4, av));
        let label = tr("Add a bind");
        let lw = fonts().width(Face::Medium, 0.78 * r, label);
        let mx = (ab[0] + ab[2]) / 2.0;
        let pc = [mx - lw / 2.0 - 0.75 * r, (ab[1] + ab[3]) / 2.0];
        let pcol = mix(pal().t2, pal().t1, av);
        c.line([pc[0] - 0.3 * r, pc[1]], [pc[0] + 0.3 * r, pc[1]], 1.5, pcol, 0.0);
        c.line([pc[0], pc[1] - 0.3 * r], [pc[0], pc[1] + 0.3 * r], 1.5, pcol, 0.0);
        c.text(mx - lw / 2.0 + 0.1 * r, ab[1] + 1.33 * r, 0.78 * r, Face::Medium, label, pcol);
        self.hits.push((ab, Hit::Card(5, 0)));
        yy += 3.2 * r;
        for l in &does_lines {
            c.text(x0, yy + 0.7 * r, 0.72 * r, Face::Regular, l, pal().t4);
            yy += 1.05 * r;
        }
        c.alpha = base;
    }

    /// A press on the card: true when it was its (a press past it closes it, and is the menu's).
    pub(super) fn card_press(&mut self, h: Hit, button: i32) -> bool {
        if matches!(self.card, Some(Card::Cloud { .. })) {
            return self.cloud_press(h, button);
        }
        if let Some(Card::Placeholders { i, .. }) = self.card {
            return match h {
                Hit::Card(70, li) => {
                    if button == 0 {
                        self.ph_insert(i, li as usize);
                    }
                    true
                }
                Hit::Card(0, _) => true,
                _ => {
                    self.card_shut = self.card.take();
                    false
                }
            };
        }
        if let Some(Card::Alts { .. }) = self.card {
            return self.alts_press(h, button);
        }
        if let Some(Card::Deaths { .. }) = self.card {
            return self.deaths_press(h, button);
        }
        if let Some(Card::Friends { .. }) = self.card {
            return self.friends_press(h, button);
        }
        if let Some(Card::Lines { .. }) = self.card {
            return self.lines_press(h, button);
        }
        if let Some(Card::Lang { .. }) = self.card {
            return match h {
                Hit::Card(20, li) => {
                    if button == 0 {
                        self.lang_choose(li as usize);
                    }
                    true
                }
                Hit::Card(0, _) => true,
                _ => {
                    self.card_shut = self.card.take();
                    false
                }
            };
        }
        if matches!(self.card, Some(Card::NewCfg { .. } | Card::Cfg { .. } | Card::Import { .. } | Card::ModuleImport { .. } | Card::Reset { .. })) {
            if let Hit::Card(..) = h {
                if button == 0 {
                    self.cfg_card_press(h);
                }
                return true;
            }
            self.card_shut = self.card.take();
            return false;
        }
        let Some(Card::Binds { to, .. }) = self.card.clone() else { return false };
        let Hit::Card(part, idx) = h else {
            if !matches!(self.listen, Some(Listen::Bind(..))) {
                self.card_shut = self.card.take();
            }
            return false;
        };
        if part >= 8 {
            // a row of the binds' page under it: the card goes, the press is the page's
            self.card = None;
            return false;
        }
        if part == 7 {
            // ‹ › on what a bind sets (the right button goes back too)
            let dir = if idx % 2 == 0 || button == 1 { -1 } else { 1 };
            if button == 0 || button == 1 {
                self.bind_value_step(to, idx as usize / 2, dir);
            }
            return true;
        }
        if button != 0 {
            return true;
        }
        let k = idx as usize;
        match part {
            1 => self.start_bind(to, Some(k)),
            2 | 3 => {
                let hold = part == 3;
                if let Some(b) = self.binds_mut(to).get_mut(k) {
                    if b.hold != hold {
                        b.hold = hold;
                        self.sound_tick(0.5);
                        self.save();
                    }
                }
            }
            4 => {
                if k < self.binds_of(to).len() {
                    self.binds_mut(to).remove(k);
                    self.bind_back.retain(|b| b.0 != to);
                    self.listen = None;
                    self.sound_tick(0.5);
                    self.save();
                }
            }
            5 => self.start_bind(to, None),
            6 => {
                self.card = None;
                self.listen = None;
            }
            _ => {}
        }
        true
    }

    /// What the bind sets, one on or back: its own way, then each value of its setting's.
    fn bind_value_step(&mut self, to: BindTo, k: usize, dir: i32) {
        let BindTo::Setting(o, si) = to else { return };
        let values = bind_values(&self.set_ref(o)[si].kind);
        if values.is_empty() {
            return;
        }
        let Some(b) = self.binds_mut(to).get_mut(k) else { return };
        let n = values.len() as i32 + 1;
        let cur = b.value.as_ref().and_then(|v| values.iter().position(|o| o == v)).map_or(0, |p| p as i32 + 1);
        let next = (cur + dir).rem_euclid(n);
        b.value = if next == 0 { None } else { Some(values[next as usize - 1].clone()) };
        self.bind_back.retain(|q| q.0 != to);
        self.sound_tick(0.5);
        self.save();
    }

    // ------------------------------------------------------------------ every bind on a page
    pub(super) fn binds_page(&mut self, c: &mut Canvas, area: [f32; 4]) {
        let r = self.r;
        let all = self.all_binds();
        let (a, dyv) = self.rise_c(0.22, 0.0, false);
        c.alpha = a;
        let mut y = area[1] - self.scroll + dyv;
        if all.is_empty() {
            let rr = [area[0], y, area[2], y + 6.2 * r];
            c.rect(rr, r, pal().f1);
            c.text(rr[0] + 1.15 * r, rr[1] + 2.0 * r, 0.93 * r, Face::Semi, tr("No binds yet"), pal().t2);
            let line = tr("The middle button on a module binds it; the right one on a setting opens its binds.");
            for (n, l) in fonts().wrap(Face::Regular, 0.76 * r, line, rr[2] - rr[0] - 2.3 * r).iter().enumerate() {
                c.text(rr[0] + 1.15 * r, rr[1] + 3.4 * r + n as f32 * 1.1 * r, 0.76 * r, Face::Regular, l, pal().t4);
            }
            self.scroll_clamp(area, rr[3]);
            c.alpha = 1.0;
            return;
        }
        for (n, (to, list)) in all.iter().enumerate() {
            let rows = list.len();
            let h = 2.9 * r + rows as f32 * 2.3 * r + 0.6 * r;
            let rr = [area[0], y, area[2], y + h];
            let (k, ddy) = self.rise_c(0.22 + (n as f32 * 0.025).min(0.3), (n as f32 * 0.01).min(0.1), false);
            c.alpha = k;
            let hit = Hit::Card(9, n as u16);
            let hv = self.an(ak(A_HOV, 9900 + n), if self.hover == hit { 1.0 } else { 0.0 }, 12.0, 0.0);
            let rr2 = [rr[0], rr[1] + ddy - dyv, rr[2], rr[3] + ddy - dyv];
            c.rect(rr2, r, mix(pal().f1, pal().f2, hv));
            let (who, what) = self.bind_name(*to);
            let title = if what.is_empty() { who.clone() } else { what.clone() };
            let tw = c.text(rr2[0] + 1.15 * r, rr2[1] + 1.85 * r, 0.9 * r, Face::Semi, &title, pal().t1);
            if !what.is_empty() {
                c.text(rr2[0] + 1.15 * r + tw + 0.55 * r, rr2[1] + 1.85 * r, 0.7 * r, Face::Medium, &who, pal().t4);
            } else if let BindTo::Module(i) = to {
                let on = self.mods[*i].on;
                c.text(rr2[0] + 1.15 * r + tw + 0.55 * r, rr2[1] + 1.85 * r, 0.7 * r, Face::Medium, tr(if on { "On" } else { "Off" }), pal().t4);
            }
            let mut by = rr2[1] + 2.75 * r;
            for b in list {
                let label = b.label();
                let cw = fonts().width(Face::Mono, 0.7 * r, &label) + 1.0 * r;
                c.rect([rr2[0] + 1.15 * r, by, rr2[0] + 1.15 * r + cw, by + 1.6 * r], 0.42 * r, pal().f2);
                c.text(rr2[0] + 1.65 * r, by + 1.08 * r, 0.7 * r, Face::Mono, &label, pal().t1);
                let mode = tr(if b.hold { "Hold" } else { "Press" }).to_string();
                let mode = match (to, b.value.as_deref()) {
                    (BindTo::Setting(o, si), Some(v)) => {
                        value_kind(&self.set_ref(*o)[*si].kind, v).map_or(mode.clone(), |k| format!("{mode} · {}", value_text(&k)))
                    }
                    _ => mode,
                };
                let mode = mode.as_str();
                c.text(rr2[0] + 1.15 * r + cw + 0.6 * r, by + 1.08 * r, 0.7 * r, Face::Medium, mode, pal().t3);
                let clash = self.clashes(*to, b);
                if !clash.is_empty() {
                    let line = format!("{} {}", tr("Also on"), clash.join(", "));
                    let mw = fonts().width(Face::Medium, 0.7 * r, mode);
                    let x0 = rr2[0] + 1.15 * r + cw + 0.6 * r + mw + 0.8 * r;
                    let lw = fonts().fit(Face::Regular, 0.66 * r, &line, rr2[2] - 1.15 * r - x0);
                    c.text(x0, by + 1.06 * r, 0.66 * r, Face::Regular, &lw, theme::fade(pal().t3, 0.9));
                }
                by += 2.3 * r;
            }
            self.hits.push((rr2, hit));
            y += h + 0.62 * r;
        }
        self.scroll_clamp(area, y + self.scroll - self.scroll + 0.0);
        c.alpha = 1.0;
    }

    /// A press on the page's rows: the card of the binds pressed.
    pub(super) fn binds_page_press(&mut self, h: Hit) -> bool {
        let Hit::Card(9, n) = h else { return false };
        let all = self.all_binds();
        let Some((to, _)) = all.get(n as usize) else { return true };
        let at = self.hits.iter().find(|q| q.1 == h).map_or([self.mouse[0], self.mouse[1], self.mouse[0], self.mouse[1]], |q| q.0);
        let at = [at[2] - 1.0, at[1], at[2] - 1.0, at[1] + 2.4 * self.r];
        self.open_binds(*to, at);
        true
    }

    // ------------------------------------------------------------------ keys in the menu while a bind is recorded
    /// A key down in the menu; true when it was the recording's (a bind's, or an Auto Swap's key).
    pub(super) fn bind_key(&mut self, escape: bool, backspace: bool, name: &str) -> bool {
        let Some(l @ (Listen::Bind(..) | Listen::Swap(_))) = self.listen else { return false };
        // the letter the game sends after the key is the recording's too: not typed in the search under the card
        self.eat = Some(name.to_string());
        if escape {
            self.listen = None;
            self.rec.clear();
            return true;
        }
        if backspace || name == "Delete" {
            match l {
                Listen::Bind(to, Some(k)) if k < self.binds_of(to).len() => {
                    self.binds_mut(to).remove(k);
                    self.save();
                }
                Listen::Swap(i) => {
                    self.swap_key_set(i, None);
                    return true;
                }
                _ => {}
            }
            self.listen = None;
            self.rec.clear();
            self.sound_tick(0.5);
            return true;
        }
        if binds::modifier(name) {
            if !self.rec.iter().any(|k| k == name) {
                self.rec.push(name.to_string());
            }
            return true;
        }
        let mut keys: Vec<String> = self.gheld.iter().filter(|k| binds::modifier(k)).cloned().collect();
        keys.push(name.to_string());
        self.listen_done(keys);
        true
    }

    /// The combination waited for is in: the bind's, or the swap's key.
    fn listen_done(&mut self, keys: Vec<String>) {
        match self.listen {
            Some(Listen::Bind(to, slot)) => self.finish_bind(to, slot, keys),
            Some(Listen::Swap(i)) => self.swap_key_set(i, Some(keys)),
            _ => {}
        }
    }

    /// A key down in the menu (or the layout editor): held from now on; pressed anew (not its repeat), what it types
    /// is the menu's again.
    pub(super) fn bind_key_down(&mut self, name: &str) {
        if !name.is_empty() && !self.gheld.iter().any(|k| k == name) {
            self.gheld.push(name.to_string());
            self.eat = None;
        }
    }

    /// A key let go in the menu: a combination of modifiers only is done when one of them goes.
    pub(super) fn bind_key_up(&mut self, name: &str) {
        self.gheld.retain(|k| k != name);
        if self.eat.as_deref() == Some(name) {
            self.eat = None;
        }
        if !matches!(self.listen, Some(Listen::Bind(..) | Listen::Swap(_))) {
            return;
        }
        if binds::modifier(name) && !self.rec.is_empty() && self.rec.iter().all(|k| binds::modifier(k)) {
            let keys = self.rec.clone();
            self.listen_done(keys);
        }
    }

    /// A mouse button other than the left and the right while a bind is recorded: it is the trigger.
    pub(super) fn bind_mouse(&mut self, name: &str) -> bool {
        if !matches!(self.listen, Some(Listen::Bind(..) | Listen::Swap(_))) {
            return false;
        }
        let mut keys: Vec<String> = self.gheld.iter().filter(|k| binds::modifier(k)).cloned().collect();
        keys.push(name.to_string());
        self.listen_done(keys);
        true
    }

    // ------------------------------------------------------------------ the keys in the world
    /// A key (or a mouse button) down or up in the world with no screen open (ups come always: a key held while a
    /// screen opened is let go then). True if a bind went off.
    pub(super) fn world_key(&mut self, name: &str, down: bool) -> bool {
        self.ensure_loaded();
        if !down {
            self.wheld.retain(|k| k != name);
            // held binds whose keys are no longer all down go back
            let held = self.wheld.clone();
            self.swap_keys_up(&held);
            self.fl_keys_up(&held);
            let mut gone = Vec::new();
            self.holds.retain(|(to, keys, prior)| {
                if keys.iter().all(|k| held.contains(k)) {
                    true
                } else {
                    gone.push((*to, prior.clone()));
                    false
                }
            });
            for (to, prior) in gone {
                self.bind_restore(to, prior);
            }
            return false;
        }
        if self.wheld.iter().any(|k| k == name) {
            // the key's repeat: nothing more
            return false;
        }
        self.wheld.push(name.to_string());
        let held = self.wheld.clone();
        // the acting modules' keys (play.rs: No Friend Damage's)
        self.play_key(name);
        // Auto Swap's keys (swap.rs) go to Java besides; CS's inspect (cosmetics/) while the hands are CS's
        let swapped = self.swap_keys_down(name, &held) | crate::cosmetics::world_key(name);
        // the longest combinations win: Ctrl + G does not also set off G
        let mut found: Vec<(BindTo, Bind)> = Vec::new();
        for (to, list) in self.all_binds() {
            for b in list {
                if b.fires(name, &held) {
                    found.push((to, b));
                }
            }
        }
        if found.is_empty() {
            return swapped;
        }
        let most = found.iter().map(|f| f.1.keys.len()).max().unwrap_or(1);
        found.retain(|f| f.1.keys.len() == most);
        for (to, b) in found {
            if b.hold {
                if self.holds.iter().any(|h| h.0 == to) {
                    continue;
                }
                let prior = self.bind_fire(to, true, b.value.as_deref());
                if let Some(p) = prior {
                    self.holds.push((to, b.keys.clone(), p));
                }
            } else {
                self.bind_fire(to, false, b.value.as_deref());
            }
        }
        self.save();
        true
    }

    /// What a bind does to its thing; `hold`: it is held (the way it was is returned, to be put back); `value`: the
    /// value it sets its setting to (pressed again it puts back what was before it).
    fn bind_fire(&mut self, to: BindTo, hold: bool, value: Option<&str>) -> Option<Prior> {
        let t = self.clock.elapsed().as_secs_f32();
        let notes = self.mods[self.mod_index("notes")].set.iter().any(|s| s.label == "Modules" && s.kind == Kind::Toggle(true));
        match to {
            BindTo::Module(i) => {
                if self.mods[i].plain {
                    return None;
                }
                // Free Look's keys look round, they never switch it
                if self.mods[i].id == "free-look" {
                    self.fl_press();
                    return None;
                }
                // Click Pearl's keys throw (Java takes the press), they never switch it
                if self.mods[i].id == super::swap::PEARL {
                    if self.mods[i].on && !self.hidden_here(super::swap::PEARL) {
                        self.pearl_ev += 1;
                    }
                    return None;
                }
                let was = self.mods[i].on;
                if hold && was {
                    // held, it is on while held: it already is
                    return Some(Prior::Module(was));
                }
                self.toggle(i);
                if notes {
                    let line = tr(if self.mods[i].on { "On" } else { "Off" });
                    self.hud.note(self.mods[i].name.to_string(), line.into(), t);
                }
                Some(Prior::Module(was))
            }
            BindTo::Setting(o, si) => {
                let label = self.set_ref(o)[si].label;
                let was = self.set_ref(o)[si].kind.clone();
                if let Some(target) = value.and_then(|v| value_kind(&was, v)) {
                    if !hold && was == target {
                        // pressed again on its value: back to what it was before (when that is known)
                        if let Some(k) = self.bind_back.iter().position(|b| b.0 == to) {
                            let back = self.bind_back.remove(k).1;
                            self.set_list(o)[si].kind = back;
                        }
                    } else {
                        if !hold {
                            self.bind_back.retain(|b| b.0 != to);
                            self.bind_back.push((to, was.clone()));
                        }
                        self.set_list(o)[si].kind = target;
                    }
                    if self.set_ref(o)[si].kind != was {
                        self.graphics_follow(o, si);
                    }
                    if notes {
                        let (who, what) = self.bind_name(to);
                        let line = value_text(&self.set_ref(o)[si].kind);
                        self.hud.note(format!("{who} · {what}"), line, t);
                    }
                    return Some(Prior::Setting(was));
                }
                match &mut self.set_list(o)[si].kind {
                    Kind::Toggle(v) => *v = !*v,
                    Kind::Choice { v, opts } => {
                        // a preset's "Custom" / "Own" is what a change makes, not a choice: passed (it would fall back to
                        // the preset it was made from, and the key would stay there)
                        let n = opts.len().max(1);
                        let mut k = (*v + 1) % n;
                        if matches!(opts.get(k), Some(&("Custom" | "Own"))) {
                            k = (k + 1) % n;
                        }
                        *v = k;
                    }
                    Kind::Slider { min, max, v, step, .. } => {
                        if hold {
                            *v = *max;
                        } else {
                            let q = (*max - *min) / 4.0;
                            let next = *v + q;
                            *v = if next > *max + 1e-4 { *min } else { ((next.min(*max) - *min) / *step).round() * *step + *min };
                        }
                    }
                    Kind::Multi { on, .. } => {
                        if hold {
                            on.iter_mut().for_each(|x| *x = true);
                        } else {
                            let cur = on.iter().position(|x| *x).map_or(0, |p| p + 1) % on.len().max(1);
                            on.iter_mut().enumerate().for_each(|(j, x)| *x = j == cur);
                        }
                    }
                    Kind::Colour(col) => col.chroma = if col.chroma > 0.0 { 0.0 } else { 1.0 },
                    Kind::Action => {
                        self.action(o, label);
                        return None;
                    }
                    _ => return None,
                }
                self.graphics_follow(o, si);
                if notes {
                    let (who, what) = self.bind_name(to);
                    let line = value_text(&self.set_ref(o)[si].kind);
                    self.hud.note(format!("{who} · {what}"), line, t);
                }
                Some(Prior::Setting(was))
            }
        }
    }

    fn bind_restore(&mut self, to: BindTo, prior: Prior) {
        match (to, prior) {
            (BindTo::Module(i), Prior::Module(was)) => {
                if self.mods[i].on != was {
                    self.toggle(i);
                }
            }
            (BindTo::Setting(o, si), Prior::Setting(k)) => {
                let mut changed = false;
                if let Some(s) = self.set_list(o).get_mut(si) {
                    changed = s.kind != k;
                    s.kind = k;
                }
                if changed {
                    self.graphics_follow(o, si);
                }
            }
            _ => {}
        }
        self.save();
    }

    /// A setting a bind changed: when it is Graphics, its preset as the menu sets it.
    fn graphics_follow(&mut self, o: Owner, si: usize) {
        if let Some(s) = self.set_ref(o).get(si) {
            if s.label == "Graphics" {
                if let Kind::Choice { v, .. } = s.kind {
                    self.apply_preset(v);
                }
            }
        }
    }
}

/// A bind's value as its setting would be with it (None: not a value of this setting's).
pub(super) fn value_kind(kind: &Kind, v: &str) -> Option<Kind> {
    let mut k = kind.clone();
    match &mut k {
        Kind::Toggle(on) => *on = v == "1",
        Kind::Choice { v: cur, opts } => *cur = opts.iter().position(|o| *o == v)?,
        Kind::Slider { min, max, v: cur, .. } => *cur = v.parse::<f32>().ok()?.clamp(*min, *max),
        _ => return None,
    }
    Some(k)
}

/// A setting's value as the menu says it ("On", "Medium", "2.5 x").
pub(super) fn value_text(kind: &Kind) -> String {
    match kind {
        Kind::Toggle(v) => tr(if *v { "On" } else { "Off" }).to_string(),
        Kind::Choice { v, opts } => tr(opts[*v]).to_string(),
        Kind::Slider { v, unit, step, .. } => {
            format!("{:.*}{}", if *step >= 1.0 { 0 } else if *step >= 0.1 { 1 } else { 2 }, v, i18n::unit(unit))
        }
        _ => String::new(),
    }
}

/// The values a bind of this setting can set, in order (none for a setting that has none to choose).
pub(super) fn bind_values(kind: &Kind) -> Vec<String> {
    match kind {
        Kind::Toggle(_) => vec!["1".into(), "0".into()],
        Kind::Choice { opts, .. } => opts.iter().map(|o| o.to_string()).collect(),
        Kind::Slider { min, max, step, .. } => (0..=4)
            .map(|q| {
                let x = min + (max - min) * q as f32 / 4.0;
                let x = ((x - min) / step).round() * step + min;
                format!("{}", (x * 1000.0).round() / 1000.0)
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// The press on a module's tile with the middle button: its first bind recorded anew (or its first made).
pub(super) fn first_slot(list: &[Bind]) -> Option<usize> {
    if list.is_empty() {
        None
    } else {
        Some(0)
    }
}

impl Gui {
    /// The clipboard, asked for (a code being pasted).
    pub(super) fn paste(&mut self, text: &str) {
        if let Some(id) = self.paste_module.take() {
            self.module_code_paste(text, Some(&id));
            return;
        }
        if matches!(self.card, Some(Card::Lines { .. })) {
            for ch in text.chars() { self.message_char(ch); }
            return;
        }
        if matches!(self.card,Some(Card::Cloud {tab:3,..})) {
            self.cloud_paste(text);
            return;
        }
        // the Cosmetics page's crosshair asked for it, or a config's code
        if std::mem::take(&mut self.swing_ed.paste) {
            self.swing_paste(text);
        } else if std::mem::take(&mut self.cosm.paste) {
            self.cosm_paste(text);
        } else if std::mem::take(&mut self.cosm.paste_item) {
            self.item_paste(text);
        } else {
            self.paste_code(text);
        }
    }
}

// ---------------------------------------------------------------------- the language card
impl Gui {
    /// The game's languages as Java tells them (code, name, region): the client's own four first, then by name.
    pub(super) fn set_langs(&mut self, sel: &str, list: &str) {
        let mut v: Vec<(String, String, String)> = list
            .split('\u{1e}')
            .filter_map(|l| {
                let mut f = l.split('\u{1f}');
                Some((f.next()?.to_string(), f.next().unwrap_or("").to_string(), f.next().unwrap_or("").to_string()))
            })
            .filter(|l| !l.0.is_empty())
            .collect();
        const OURS: [&str; 4] = ["ru_ru", "uk_ua", "kk_kz", "en_us"];
        v.sort_by(|a, b| {
            let ka = OURS.iter().position(|c| *c == a.0).unwrap_or(9);
            let kb = OURS.iter().position(|c| *c == b.0).unwrap_or(9);
            ka.cmp(&kb).then_with(|| a.1.to_lowercase().cmp(&b.1.to_lowercase()))
        });
        self.langs = v;
        if !sel.is_empty() {
            self.lang_sel = sel.to_string();
        }
    }

    pub(super) fn open_lang(&mut self, at: [f32; 4]) {
        if matches!(self.card_shut.take(), Some(Card::Lang { .. })) {
            return;
        }
        self.card = Some(Card::Lang { at });
        self.card_at = self.now;
        self.card_field.clear();
        self.card_scroll = 0.0;
        self.card_scroll_to = 0.0;
        self.anim.remove(&ak(A_POP, 2));
        self.outbox.push("langs".into());
        self.sound_tick(0.6);
    }

    /// The languages the search answers (name, region, code; the client's own words for the four).
    pub(super) fn langs_found(&self) -> Vec<usize> {
        let q = self.card_field.to_lowercase();
        (0..self.langs.len())
            .filter(|&i| {
                if q.is_empty() {
                    return true;
                }
                let (c, n, r) = &self.langs[i];
                format!("{n} {r} {c}").to_lowercase().contains(&q)
            })
            .collect()
    }

    pub(super) fn lang_card(&mut self, c: &mut Canvas, at: [f32; 4]) {
        let r = self.r;
        let k = self.an(ak(A_POP, 2), 1.0, 14.0, 0.0);
        let w = 24.0 * r;
        let found = self.langs_found();
        let rows_h = 2.35 * r;
        let list_h = (found.len().max(1) as f32 * rows_h).min(11.0 * rows_h);
        let h = 7.6 * r + list_h + 2.4 * r;
        let mut x = at[0];
        let mut y = at[1] - h - 0.5 * r;
        if y < 8.0 {
            y = at[3] + 0.5 * r;
        }
        x = x.clamp(8.0, (self.w - w - 8.0).max(8.0));
        y = y.clamp(8.0, (self.h - h - 8.0).max(8.0));
        let rr = [x, y + (1.0 - k) * 0.5 * r, x + w, y + h + (1.0 - k) * 0.5 * r];
        self.card_rect = rr;
        let base = c.alpha;
        c.alpha = k;
        c.shadow(rr, r, 3.0 * r, grey(0.0, 0.62));
        let g = pal().glass;
        c.rect(rr, r, [g[0].max(0.05), g[1].max(0.05), g[2].max(0.055), 0.97]);
        self.hits.push((rr, Hit::Card(0, 0)));
        let (x0, x1) = (rr[0] + 1.2 * r, rr[2] - 1.2 * r);
        c.text(x0, rr[1] + 2.0 * r, 1.05 * r, Face::Bold, tr("Language"), pal().t1);
        c.text(x0, rr[1] + 3.2 * r, 0.72 * r, Face::Regular, tr("The client and the whole game"), pal().t3);
        // the search: typed straight in
        let sr = [x0, rr[1] + 4.1 * r, x1, rr[1] + 6.3 * r];
        c.rect(sr, 0.65 * r, pal().f2);
        let ic = [sr[0] + 1.1 * r, (sr[1] + sr[3]) / 2.0 - 0.05 * r];
        c.ring([ic[0] - 0.1 * r, ic[1] - 0.1 * r], 0.34 * r, 1.4, pal().t3);
        c.line([ic[0] + 0.16 * r, ic[1] + 0.16 * r], [ic[0] + 0.42 * r, ic[1] + 0.42 * r], 1.4, pal().t3, 0.0);
        let base_y = sr[1] + 1.42 * r;
        if self.card_field.is_empty() {
            c.text(sr[0] + 1.9 * r, base_y, 0.8 * r, Face::Regular, tr("Find a language"), pal().t4);
        }
        let tw = c.text(sr[0] + 1.9 * r, base_y, 0.8 * r, Face::Regular, &self.card_field, pal().t1);
        if (self.now * 2.0).fract() < 0.55 {
            c.rect([sr[0] + 1.95 * r + tw, sr[1] + 0.55 * r, sr[0] + 1.95 * r + tw + 1.5, sr[3] - 0.55 * r], 0.0, pal().accent);
        }
        // the list, scrolled within it
        self.card_scroll += (self.card_scroll_to - self.card_scroll) * (1.0 - (-self.dt * 14.0).exp());
        let top = sr[3] + 0.6 * r;
        let clip = [rr[0], top, rr[2], top + list_h];
        let max = (found.len() as f32 * rows_h - list_h).max(0.0);
        self.card_scroll_to = self.card_scroll_to.clamp(0.0, max);
        c.push_clip(clip);
        if found.is_empty() {
            c.text(x0, top + 1.5 * r, 0.8 * r, Face::Regular, tr("No such language"), pal().t4);
        }
        for (n, &li) in found.iter().enumerate() {
            let yy = top + n as f32 * rows_h - self.card_scroll;
            if yy + rows_h < top - 1.0 || yy > top + list_h + 1.0 {
                continue;
            }
            let row = [x0 - 0.5 * r, yy + 0.1 * r, x1 + 0.5 * r, yy + rows_h - 0.1 * r];
            let (code, name, region) = self.langs[li].clone();
            let cur = code == self.lang_sel;
            let hit = Hit::Card(20, li as u16);
            let hv = self.an(ak(A_HOV, 13_000 + li), if self.hover == hit { 1.0 } else if cur { 0.7 } else { 0.0 }, 14.0, 0.0);
            c.rect(row, 0.55 * r, grey(1.0, 0.07 * hv));
            let mid = (row[1] + row[3]) / 2.0;
            let nw = c.text(x0, mid + 0.3 * r, 0.84 * r, Face::Medium, &name, if cur { pal().t1 } else { mix(pal().t2, pal().t1, hv) });
            c.text(x0 + nw + 0.5 * r, mid + 0.3 * r, 0.7 * r, Face::Regular, &region, pal().t4);
            let ours = ["ru_ru", "uk_ua", "kk_kz", "en_us"].contains(&code.as_str());
            let mut rx = x1;
            if cur {
                let cc = [rx - 0.4 * r, mid];
                c.polyline(&[[cc[0] - 0.36 * r, cc[1]], [cc[0] - 0.1 * r, cc[1] + 0.26 * r], [cc[0] + 0.36 * r, cc[1] - 0.28 * r]], 1.6, pal().t1);
                rx -= 1.3 * r;
            }
            if ours {
                let label = tr("Recode in it");
                let lw = fonts().width(Face::Medium, 0.6 * r, label) + 0.8 * r;
                c.rect([rx - lw, mid - 0.55 * r, rx, mid + 0.55 * r], 0.3 * r, pal().f2);
                c.text(rx - lw + 0.4 * r, mid + 0.22 * r, 0.6 * r, Face::Medium, label, pal().t2);
            }
            self.hits.push((row, hit));
        }
        c.pop_clip();
        // what a language changes
        let line = tr("The game's words change too; another language reads the client in English.");
        let ly = top + list_h + 0.4 * r;
        for (n, l) in fonts().wrap(Face::Regular, 0.66 * r, line, x1 - x0).iter().take(2).enumerate() {
            c.text(x0, ly + 0.8 * r + n as f32 * 0.95 * r, 0.66 * r, Face::Regular, l, pal().t4);
        }
        c.alpha = base;
    }

    /// A language pressed (or the first found, Enter): the game and the client change to it.
    pub(super) fn lang_choose(&mut self, li: usize) {
        let Some((code, ..)) = self.langs.get(li).cloned() else { return };
        if code != self.lang_sel {
            self.outbox.push(format!("lang\t{code}"));
            self.lang_sel = code;
            self.switch_sound(true, 0.5);
        }
        self.card = None;
    }
}

// ---------------------------------------------------------------------- the placeholders' card
impl Gui {
    pub(super) fn ph_card(&mut self, c: &mut Canvas, i: usize, at: [f32; 4]) {
        let r = self.r;
        let k = self.an(ak(A_POP, 2), 1.0, 14.0, 0.0);
        let w = 27.0 * r;
        let q = self.card_field.to_lowercase();
        let all = self.placeholders_now();
        // (as shown and in English, typed in either layout)
        let qs = [q.clone(), super::search::other_layout(&q)];
        let found: Vec<_> = all
            .into_iter()
            .filter(|(n, d, _)| q.is_empty() || qs.iter().any(|q| n.contains(q.as_str()) || tr(d).to_lowercase().contains(q.as_str()) || d.to_lowercase().contains(q.as_str())))
            .collect();
        let rows_h = 2.25 * r;
        let list_h = (found.len().max(1) as f32 * rows_h).min(12.0 * rows_h);
        let h = 7.4 * r + list_h + 1.6 * r;
        let mut x = at[0] - w - 1.0 * r;
        if x < 8.0 {
            x = at[0] + 1.0 * r;
        }
        let y = (at[1] - h / 2.0).clamp(8.0, (self.h - h - 8.0).max(8.0));
        x = x.clamp(8.0, (self.w - w - 8.0).max(8.0));
        let rr = [x, y + (1.0 - k) * 0.5 * r, x + w, y + h + (1.0 - k) * 0.5 * r];
        self.card_rect = rr;
        let base = c.alpha;
        c.alpha = k;
        c.shadow(rr, r, 3.0 * r, grey(0.0, 0.62));
        let g = pal().glass;
        c.rect(rr, r, [g[0].max(0.05), g[1].max(0.05), g[2].max(0.055), 0.97]);
        self.hits.push((rr, Hit::Card(0, 0)));
        let (x0, x1) = (rr[0] + 1.2 * r, rr[2] - 1.2 * r);
        c.text(x0, rr[1] + 2.0 * r, 1.05 * r, Face::Bold, tr("Placeholders"), pal().t1);
        c.text(x0, rr[1] + 3.2 * r, 0.72 * r, Face::Regular, tr("A press puts it in the text; {name:1} gives one decimal"), pal().t3);
        let sr = [x0, rr[1] + 4.1 * r, x1, rr[1] + 6.3 * r];
        c.rect(sr, 0.65 * r, pal().f2);
        if self.card_field.is_empty() {
            c.text(sr[0] + 0.8 * r, sr[1] + 1.42 * r, 0.8 * r, Face::Regular, tr("Find a placeholder"), pal().t4);
        }
        let tw = c.text(sr[0] + 0.8 * r, sr[1] + 1.42 * r, 0.8 * r, Face::Regular, &self.card_field.clone(), pal().t1);
        if (self.now * 2.0).fract() < 0.55 {
            c.rect([sr[0] + 0.85 * r + tw, sr[1] + 0.55 * r, sr[0] + 0.85 * r + tw + 1.5, sr[3] - 0.55 * r], 0.0, pal().accent);
        }
        self.card_scroll += (self.card_scroll_to - self.card_scroll) * (1.0 - (-self.dt * 14.0).exp());
        let top = sr[3] + 0.6 * r;
        let clip = [rr[0], top, rr[2], top + list_h];
        let max = (found.len() as f32 * rows_h - list_h).max(0.0);
        self.card_scroll_to = self.card_scroll_to.clamp(0.0, max);
        c.push_clip(clip);
        for (n, (name, desc, val)) in found.iter().enumerate() {
            let yy = top + n as f32 * rows_h - self.card_scroll;
            if yy + rows_h < top - 1.0 || yy > top + list_h + 1.0 {
                continue;
            }
            let row = [x0 - 0.5 * r, yy + 0.1 * r, x1 + 0.5 * r, yy + rows_h - 0.1 * r];
            let li = PLACEHOLDERS.iter().position(|p| p.0 == *name).unwrap_or(0);
            let hit = Hit::Card(70, li as u16);
            let hv = self.an(ak(A_HOV, 18_000 + li), if self.hover == hit { 1.0 } else { 0.0 }, 14.0, 0.0);
            c.rect(row, 0.55 * r, grey(1.0, 0.07 * hv));
            let mid = (row[1] + row[3]) / 2.0;
            let chip = format!("{{{name}}}");
            let cw = fonts().width(Face::Mono, 0.68 * r, &chip) + 0.9 * r;
            c.rect([x0, mid - 0.68 * r, x0 + cw, mid + 0.68 * r], 0.35 * r, pal().f2);
            c.text(x0 + 0.45 * r, mid + 0.24 * r, 0.68 * r, Face::Mono, &chip, pal().t1);
            let vw = fonts().width(Face::Medium, 0.72 * r, val).min(7.5 * r);
            let d = fonts().fit(Face::Regular, 0.72 * r, tr(desc), x1 - x0 - cw - vw - 1.6 * r);
            c.text(x0 + cw + 0.6 * r, mid + 0.26 * r, 0.72 * r, Face::Regular, &d, pal().t3);
            let v = fonts().fit(Face::Medium, 0.72 * r, val, 7.5 * r);
            c.text_r(x1, mid + 0.26 * r, 0.72 * r, Face::Medium, &v, pal().t2);
            self.hits.push((row, hit));
        }
        c.pop_clip();
        let _ = i;
        c.alpha = base;
    }

    /// A placeholder pressed: into the element's text last typed in (or its first).
    pub(super) fn ph_insert(&mut self, i: usize, li: usize) {
        let Some((name, _)) = PLACEHOLDERS.get(li) else { return };
        let token = format!("{{{name}}}");
        let si = match self.last_text {
            Some((Owner::Module(j), si)) if j == i => Some(si),
            _ => self.mods[i].set.iter().position(|s| matches!(s.kind, Kind::Text(_)) && s.label != "File"),
        };
        let Some(si) = si else { return };
        if let Kind::Text(v) = &mut self.mods[i].set[si].kind {
            if !v.is_empty() && !v.ends_with(' ') {
                v.push(' ');
            }
            v.push_str(&token);
        }
        self.last_text = Some((Owner::Module(i), si));
        self.save();
        self.sound_tick(0.6);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bind_values_set() {
        let look = Kind::Choice { v: 0, opts: vec!["Ink", "Polymorph", "Cursed"] };
        assert_eq!(bind_values(&look), vec!["Ink", "Polymorph", "Cursed"]);
        assert!(matches!(value_kind(&look, "Cursed"), Some(Kind::Choice { v: 2, .. })));
        assert!(value_kind(&look, "Gone").is_none());
        let speed = Kind::Slider { min: 0.5, max: 2.5, step: 0.05, v: 1.0, unit: "x" };
        assert_eq!(bind_values(&speed), vec!["0.5", "1", "1.5", "2", "2.5"]);
        assert!(matches!(value_kind(&speed, "9"), Some(Kind::Slider { v, .. }) if v == 2.5));
        assert!(matches!(value_kind(&Kind::Toggle(true), "0"), Some(Kind::Toggle(false))));
        assert!(bind_values(&Kind::Action).is_empty());
    }

    /// Graphics held on a value: its preset while held, the one before back when let go; pressed twice, back too.
    #[test]
    fn recorded_key_types_nothing() {
        let mut g = Gui::new();
        let qi = g.mod_index("quality");
        let to = BindTo::Module(qi);
        // a bind recorded with K: the 'k' after it, its repeats, are not the search's
        g.listen = Some(Listen::Bind(to, None));
        assert_eq!(g.key(false, false, false, "K".into(), false), 1);
        g.char_typed('k');
        assert_eq!(g.key(false, false, false, "K".into(), false), 0);
        g.char_typed('k');
        assert!(g.q.is_empty() && g.listen.is_none(), "typed {:?}", g.q);
        assert_eq!(g.binds_of(to).last().map(|b| b.keys.clone()), Some(vec!["K".to_string()]));
        // let go and pressed again: the search's
        g.bind_key_up("K");
        g.card = None;
        g.key(false, false, false, "K".into(), false);
        g.char_typed('k');
        assert_eq!(g.q, "k");
        // another key pressed while it is held: that one types
        g.q.clear();
        g.listen = Some(Listen::Bind(to, None));
        g.key(false, false, false, "J".into(), false);
        g.char_typed('j');
        g.key(false, false, false, "L".into(), false);
        g.char_typed('l');
        assert_eq!(g.q, "l");
        g.q.clear();
        // a key setting the same
        let si = g.client.iter().position(|s| s.label == "Open the menu").expect("the menu's key");
        g.bind_key_up("J");
        g.bind_key_up("L");
        g.listen = Some(Listen::Key(Owner::Client, si));
        g.key(false, false, false, "G".into(), false);
        g.char_typed('g');
        assert!(g.q.is_empty(), "typed {:?}", g.q);
        // a card over the menu: neither letters nor Backspace reach the search under it
        g.bind_key_up("G");
        g.q = "ab".into();
        g.card = Some(Card::Binds { to, at: [0.0; 4] });
        g.key(false, false, false, "C".into(), false);
        g.char_typed('c');
        g.key(false, false, true, "Backspace".into(), false);
        assert_eq!(g.q, "ab");
    }

    #[test]
    fn liquid_presets_follow() {
        let mut g = Gui::new();
        let ci = g.mod_index("chams");
        let at = |g: &Gui, l: &str| g.mods[ci].set.iter().position(|s| s.label == l).unwrap();
        let choice = |g: &Gui, l: &str| match g.mods[ci].set[at(g, l)].kind {
            Kind::Choice { v, .. } => v,
            _ => 99,
        };
        let slider = |g: &Gui, l: &str| match g.mods[ci].set[at(g, l)].kind {
            Kind::Slider { v, .. } => v,
            _ => -1.0,
        };
        let set_slider = |g: &mut Gui, l: &str, x: f32| {
            let i = at(g, l);
            if let Kind::Slider { v, .. } = &mut g.mods[ci].set[i].kind {
                *v = x;
            }
        };
        g.liquid_follow();
        assert_eq!((choice(&g, "Liquid"), slider(&g, "Transparency")), (0, 92.0));
        // mercury chosen: all its substance set, none of it shown as changed
        let li = at(&g, "Liquid");
        if let Kind::Choice { v, .. } = &mut g.mods[ci].set[li].kind {
            *v = 1;
        }
        g.liquid_follow();
        assert_eq!((slider(&g, "Metalness"), slider(&g, "Transparency"), choice(&g, "Liquid")), (100.0, 0.0, 1));
        assert!(!g.mods[ci].set[at(&g, "Metalness")].changed());
        // one of it changed: Own, from mercury; put back: mercury again
        set_slider(&mut g, "Roughness", 40.0);
        g.liquid_follow();
        assert_eq!((choice(&g, "Liquid"), choice(&g, "Liquid base")), (3, 1));
        assert!(g.mods[ci].set[at(&g, "Roughness")].changed());
        let ri = at(&g, "Roughness");
        g.mods[ci].set[ri].reset();
        g.liquid_follow();
        assert_eq!(choice(&g, "Liquid"), 1);
        // dragged back to its value (a slider's step rounds a hair off it): mercury still, no round arrow
        set_slider(&mut g, "Merging", (1.3f32 / 0.05).round() * 0.05);
        g.liquid_follow();
        assert_eq!(choice(&g, "Liquid"), 1);
        assert!(!g.mods[ci].set[at(&g, "Merging")].changed());
        // read back: an Own one stays as it was (its arrows to its base); one saved before the bases, its preset whole
        let vals: std::collections::HashMap<String, String> =
            [("chams.Liquid", "Own"), ("chams.Liquid base", "Oil"), ("chams.Roughness", "50")].iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        g.apply_vals(&vals, super::super::configs::P_MODULES);
        assert_eq!((choice(&g, "Liquid"), choice(&g, "Liquid base"), slider(&g, "Roughness")), (3, 2, 50.0));
        assert!(!g.mods[ci].set[at(&g, "Iridescence")].changed() || slider(&g, "Iridescence") != 55.0);
        let old: std::collections::HashMap<String, String> = [("chams.Liquid", "Water")].iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        g.apply_vals(&old, super::super::configs::P_MODULES);
        assert_eq!((choice(&g, "Liquid"), slider(&g, "Roughness"), slider(&g, "Metalness")), (0, 3.0, 0.0));
    }

    #[test]
    fn graphics_bind_follows_preset() {
        let mut g = Gui::new();
        let qi = g.mod_index("quality");
        let si = g.mods[qi].set.iter().position(|s| s.label == "Graphics").unwrap();
        let to = BindTo::Setting(Owner::Module(qi), si);
        let res = |g: &Gui| match g.mods[qi].set.iter().find(|s| s.label == "World resolution").map(|s| &s.kind) {
            Some(Kind::Slider { v, .. }) => *v,
            _ => -1.0,
        };
        let pick = |g: &Gui| match g.mods[qi].set[si].kind {
            Kind::Choice { v, .. } => v,
            _ => 99,
        };
        let presets = super::super::data::PRESETS;
        let low = presets.iter().position(|p| p.0 == "Low").unwrap();
        let high = presets.iter().position(|p| p.0 == "High").unwrap();
        assert_ne!(presets[low].1, presets[high].1, "the test needs Low and High to differ in resolution");
        if let Kind::Choice { v, .. } = &mut g.mods[qi].set[si].kind {
            *v = high;
        }
        g.apply_preset(high);
        let prior = g.bind_fire(to, true, Some("Low")).unwrap();
        assert_eq!((pick(&g), res(&g)), (low, presets[low].1));
        g.bind_restore(to, prior);
        assert_eq!((pick(&g), res(&g)), (high, presets[high].1));
        g.bind_fire(to, false, Some("Low"));
        assert_eq!((pick(&g), res(&g)), (low, presets[low].1));
        g.bind_fire(to, false, Some("Low"));
        assert_eq!((pick(&g), res(&g)), (high, presets[high].1));
    }

    #[test]
    fn a_key_on_a_preset_goes_round_past_custom() {
        let mut g = Gui::new();
        let mi = g.mod_index(super::super::swap::MODULE);
        let si = g.mods[mi].set.iter().position(|s| s.label == super::super::swaptime::L_PRESET).unwrap();
        let to = BindTo::Setting(Owner::Module(mi), si);
        let pick = |g: &Gui| match g.mods[mi].set[si].kind {
            Kind::Choice { v, .. } => v,
            _ => 99,
        };
        g.timing_follow();
        let mut seen = Vec::new();
        for _ in 0..5 {
            g.bind_fire(to, false, None);
            g.timing_follow();
            seen.push(pick(&g));
        }
        // Standard → Like autoswap 3.0 → Fast → Careful → Standard (Custom passed) → …
        assert_eq!(seen, vec![1, 2, 3, 0, 1]);
    }
}
