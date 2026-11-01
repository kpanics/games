//! The Cosmetics page's loadouts (cosmetics/loadout.rs Set): what is in the hands now kept under a name in one press —
//! the knife, the gloves, the AWP, the SSG 08, the side's arms and the crosshair —, and each kept one as a card: its
//! name (pressed, typed anew), its four things by their icons, the button that puts it all on (on: said so), the way
//! to throw it away.

use super::draw::{grey, mix, Canvas};
use super::i18n::{tr, trn};
use super::text::{fonts, Face};
use super::theme::pal;
use super::{ak, Gui, Hit, A_HOV};
use crate::cosmetics::loadout::{self, Set, Slot};

/// The parts (Hit::Cosm): the button that keeps what is on, a card's button that puts it on, its way away, its name.
pub(super) const SET_SAVE: u8 = 56;
pub(super) const SET_PUT: u8 = 57;
pub(super) const SET_DEL: u8 = 58;
pub(super) const SET_NAME: u8 = 59;

const SLOTS: [(Slot, &str); 4] = [(Slot::Knife, "Knife"), (Slot::Gloves, "Gloves"), (Slot::Awp, "AWP"), (Slot::Ssg, "SSG 08")];

impl Gui {
    /// The loadouts in `body` (under the chips): the button that keeps what is on, the kept ones as cards.
    pub(super) fn cosm_sets(&mut self, c: &mut Canvas, body: [f32; 4]) {
        let r = self.r;
        let all = loadout::sets();
        let (lo, _) = loadout::current();
        // the button that keeps what is on now, what a loadout is beside it
        let y = body[1];
        let label = tr("Keep what is on now");
        let bw = fonts().width(Face::Semi, 0.78 * r, label) + 3.4 * r;
        let b = [body[0], y, body[0] + bw, y + 2.3 * r];
        let hit = Hit::Cosm(SET_SAVE, 0);
        let hv = self.an(ak(A_HOV, 38_000), if self.hover == hit { 1.0 } else { 0.0 }, 14.0, 0.0);
        c.glow([(b[0] + b[2]) * 0.5, (b[1] + b[3]) * 0.5], bw * 0.6, grey(1.0, 0.03 + 0.05 * hv));
        c.rect(b, 1.15 * r, grey(0.86 + 0.14 * hv, 1.0));
        let pm = [b[0] + 1.25 * r, (b[1] + b[3]) * 0.5];
        let ink = grey(0.06, 1.0);
        c.rect([pm[0] - 0.36 * r, pm[1] - 0.07 * r, pm[0] + 0.36 * r, pm[1] + 0.07 * r], 0.07 * r, ink);
        c.rect([pm[0] - 0.07 * r, pm[1] - 0.36 * r, pm[0] + 0.07 * r, pm[1] + 0.36 * r], 0.07 * r, ink);
        c.text(b[0] + 2.1 * r, b[3] - 0.78 * r, 0.78 * r, Face::Semi, label, ink);
        self.hits.push((b, hit));
        let hint = tr("A loadout: the knife, the gloves, the AWP, the SSG 08, the arms and the crosshair — put on in one press");
        c.text(b[2] + 1.0 * r, b[3] - 0.8 * r, 0.64 * r, Face::Regular, &fonts().fit(Face::Regular, 0.64 * r, hint, body[2] - b[2] - 1.0 * r), pal().t4);
        let area = [body[0], b[3] + 1.2 * r, body[2], body[3]];
        if all.is_empty() {
            c.text_c((area[0] + area[2]) * 0.5, area[1] + 4.0 * r, 0.74 * r, Face::Medium, tr("No loadouts yet: put on what you like and keep it"), pal().t3);
            return;
        }
        // the cards, laid in columns, scrolled
        let gap = 0.7 * r;
        let cols = (((area[2] - area[0]) + gap) / (16.0 * r + gap)).floor().max(1.0) as usize;
        let cw = ((area[2] - area[0]) - gap * (cols as f32 - 1.0)) / cols as f32;
        let sw = (cw - 1.6 * r - 3.0 * 0.4 * r) / 4.0;
        let sh = sw * 0.75;
        let ch = 1.2 * r + 1.6 * r + sh + 1.4 * r + 2.6 * r;
        let rows = all.len().div_ceil(cols);
        self.cosm.sets_area = area;
        self.cosm.sets_content = (rows as f32 * (ch + gap) - gap - (area[3] - area[1])).max(0.0);
        self.cosm.sets_scroll_to = self.cosm.sets_scroll_to.clamp(0.0, self.cosm.sets_content);
        self.cosm.sets_scroll += (self.cosm.sets_scroll_to - self.cosm.sets_scroll) * (1.0 - (-self.dt * 14.0).exp());
        c.push_clip(area);
        let a0 = c.alpha;
        for (n, s) in all.iter().enumerate() {
            let (cx, cy) = (n % cols, n / cols);
            let x0 = area[0] + cx as f32 * (cw + gap);
            let y0 = area[1] + cy as f32 * (ch + gap) - self.cosm.sets_scroll;
            if y0 > area[3] || y0 + ch < area[1] {
                continue;
            }
            let (ca, cdy) = self.rise_c(0.22, 0.03 * n.min(16) as f32, false);
            c.alpha = a0 * ca;
            let rr = [x0, y0 + cdy, x0 + cw, y0 + cdy + ch];
            let on = s.is(&lo);
            let kon = self.an(ak(A_HOV, 38_100 + n), if on { 1.0 } else { 0.0 }, 12.0, on as i32 as f32);
            c.rect(rr, 0.8 * r, mix(pal().f1, pal().f2, 0.3 * kon));
            if kon > 0.01 {
                c.rect_line(rr, 0.8 * r, 1.2, grey(1.0, 0.25 * kon));
            }
            // its name: pressed, typed anew (its caret blinking)
            let typing = self.cosm.set_typing == Some(n);
            let nb = [rr[0] + 0.5 * r, rr[1] + 0.5 * r, rr[2] - 3.0 * r, rr[1] + 2.2 * r];
            let nhit = Hit::Cosm(SET_NAME, n as u16);
            let nh = self.an(ak(A_HOV, 38_200 + n), if typing { 1.0 } else if self.hover == nhit { 0.5 } else { 0.0 }, 14.0, 0.0);
            c.rect(nb, 0.5 * r, grey(1.0, 0.05 * nh));
            let shown = if typing { self.cosm.set_buf.clone() } else { s.name.clone() };
            let w = c.text(nb[0] + 0.4 * r, nb[3] - 0.5 * r, 0.82 * r, Face::Semi, &fonts().fit(Face::Semi, 0.82 * r, &shown, nb[2] - nb[0] - 0.8 * r), pal().t1);
            if typing && (self.now * 2.0).fract() < 0.5 {
                c.rect([nb[0] + 0.45 * r + w, nb[1] + 0.3 * r, nb[0] + 0.52 * r + w, nb[3] - 0.3 * r], 0.0, pal().t2);
            }
            self.hits.push((nb, nhit));
            // its way away: a cross at the top right
            let db = [rr[2] - 2.2 * r, rr[1] + 0.5 * r, rr[2] - 0.5 * r, rr[1] + 2.2 * r];
            let dhit = Hit::Cosm(SET_DEL, n as u16);
            let dh = self.an(ak(A_HOV, 38_300 + n), if self.hover == dhit { 1.0 } else { 0.0 }, 14.0, 0.0);
            c.rect(db, 0.5 * r, grey(1.0, 0.06 * dh));
            let m = [(db[0] + db[2]) * 0.5, (db[1] + db[3]) * 0.5];
            let k = 0.3 * r;
            let col = mix(pal().t4, pal().t1, dh);
            c.line([m[0] - k, m[1] - k], [m[0] + k, m[1] + k], 1.4, col, 0.0);
            c.line([m[0] - k, m[1] + k], [m[0] + k, m[1] - k], 1.4, col, 0.0);
            self.hits.push((db, dhit));
            // its four things by their icons, each slot's name under it
            let iy = rr[1] + 2.8 * r;
            for (k, (slot, label)) in SLOTS.iter().enumerate() {
                let sb = [rr[0] + 0.8 * r + k as f32 * (sw + 0.4 * r), iy, rr[0] + 0.8 * r + k as f32 * (sw + 0.4 * r) + sw, iy + sh];
                c.rect(sb, 0.4 * r, grey(1.0, 0.03));
                match s.get(*slot) {
                    Some(e) => {
                        let paint = if e.paint.is_empty() && *slot == Slot::Gloves { String::new() } else { e.paint.clone() };
                        if let Some((uv, [iw, ih], seen)) = self.cosm_icon(&e.item, &paint) {
                            let kk = ((sb[2] - sb[0]) / iw as f32).min((sb[3] - sb[1]) / ih as f32);
                            let (pw, ph) = (iw as f32 * kk, ih as f32 * kk);
                            let mm = [(sb[0] + sb[2]) * 0.5, (sb[1] + sb[3]) * 0.5];
                            c.pic([mm[0] - pw / 2.0, mm[1] - ph / 2.0, mm[0] + pw / 2.0, mm[1] + ph / 2.0], uv, seen, false, 1.0, 1.0, 0.0);
                        }
                    }
                    None => {
                        let mm = [(sb[0] + sb[2]) * 0.5, (sb[1] + sb[3]) * 0.5];
                        c.rect([mm[0] - 0.4 * r, mm[1] - 0.05 * r, mm[0] + 0.4 * r, mm[1] + 0.05 * r], 0.05 * r, pal().t4);
                    }
                }
                c.text_c((sb[0] + sb[2]) * 0.5, sb[3] + 0.85 * r, 0.56 * r, Face::Regular, &fonts().fit(Face::Regular, 0.56 * r, tr(label), sw), pal().t4);
            }
            // the button that puts it all on (on: said so), its arms and crosshair beside
            let by = rr[3] - 2.6 * r;
            let label = if on { tr("Equipped") } else { tr("Put on") };
            let pw = fonts().width(Face::Semi, 0.72 * r, label) + 2.2 * r;
            let pb = [rr[0] + 0.8 * r, by, rr[0] + 0.8 * r + pw, by + 1.9 * r];
            let phit = Hit::Cosm(SET_PUT, n as u16);
            let ph = self.an(ak(A_HOV, 38_400 + n), if self.hover == phit && !on { 1.0 } else { 0.0 }, 14.0, 0.0);
            if on {
                c.rect(pb, 0.95 * r, grey(1.0, 0.08));
                c.text_c((pb[0] + pb[2]) * 0.5, pb[3] - 0.62 * r, 0.72 * r, Face::Semi, label, pal().t2);
            } else {
                c.rect(pb, 0.95 * r, grey(0.86 + 0.14 * ph, 1.0));
                c.text_c((pb[0] + pb[2]) * 0.5, pb[3] - 0.62 * r, 0.72 * r, Face::Semi, label, grey(0.06, 1.0));
                self.hits.push((pb, phit));
            }
            let side = format!("{} {}{}", tr("Hands"), s.arms.to_uppercase(), if s.crosshair.is_empty() { String::new() } else { format!(" · {}", tr("own crosshair")) });
            c.text(pb[2] + 0.7 * r, pb[3] - 0.62 * r, 0.6 * r, Face::Regular, &fonts().fit(Face::Regular, 0.6 * r, &side, rr[2] - pb[2] - 1.4 * r), pal().t4);
        }
        c.alpha = a0;
        c.pop_clip();
    }

    /// The name typed is kept (an empty one: the old name stays).
    fn sets_name_done(&mut self) {
        let Some(i) = self.cosm.set_typing.take() else { return };
        let name = self.cosm.set_buf.trim().to_string();
        let mut all = loadout::sets();
        if let (Some(s), false) = (all.get_mut(i), name.is_empty()) {
            s.name = name;
            loadout::save_sets(&all);
        }
    }

    /// A press on the loadouts' parts.
    pub(super) fn sets_press(&mut self, part: u8, n: u16) {
        let mut all = loadout::sets();
        let i = n as usize;
        match part {
            SET_SAVE => {
                self.sets_name_done();
                let (lo, _) = loadout::current();
                let mut all = loadout::sets();
                let name = trn("Loadout {n}", &[("n", (all.len() + 1).to_string())]);
                all.push(Set::of(&name, &lo));
                loadout::save_sets(&all);
                self.cosm.set_typing = Some(all.len() - 1);
                self.cosm.set_buf = name;
                self.cosm.sets_scroll_to = self.cosm.sets_content + 100.0 * self.r;
                self.switch_sound(true, 0.6);
            }
            SET_PUT => {
                self.sets_name_done();
                if let Some(s) = all.get(i).cloned() {
                    loadout::change(|l| s.put_on(l));
                    self.switch_sound(true, 0.6);
                    self.toast(trn("{name} is on", &[("name", s.name.clone())]));
                }
            }
            SET_DEL => {
                if i < all.len() {
                    all.remove(i);
                    loadout::save_sets(&all);
                    self.cosm.set_typing = None;
                    self.switch_sound(false, 0.5);
                }
            }
            SET_NAME => {
                if self.cosm.set_typing != Some(i) {
                    self.sets_name_done();
                    if let Some(s) = all.get(i) {
                        self.cosm.set_typing = Some(i);
                        self.cosm.set_buf = s.name.clone();
                        self.sound_tick(0.4);
                    }
                }
            }
            _ => {}
        }
    }

    /// A key while a loadout's name is typed: true when it was the name's.
    pub(super) fn sets_key(&mut self, escape: bool, enter: bool, backspace: bool) -> bool {
        if self.cosm.set_typing.is_none() {
            return false;
        }
        if backspace {
            self.cosm.set_buf.pop();
        }
        if enter {
            self.sets_name_done();
            self.sound_tick(0.4);
        } else if escape {
            self.cosm.set_typing = None;
        }
        true
    }

    /// A letter typed while a loadout's name is: true when it was the name's.
    pub(super) fn sets_char(&mut self, ch: char) -> bool {
        if self.cosm.set_typing.is_none() || ch.is_control() {
            return self.cosm.set_typing.is_some();
        }
        if self.cosm.set_buf.chars().count() < 40 {
            self.cosm.set_buf.push(ch);
        }
        true
    }

    /// The wheel over the loadouts: true when it scrolled them.
    pub(super) fn sets_wheel(&mut self, dy: f32) -> bool {
        let [x, y] = self.mouse;
        let a = self.cosm.sets_area;
        let on = self.cosm.look.is_none() && self.cosm.item.is_none() && self.cosm.cat == super::cosmui::SETS;
        if !(on && x >= a[0] && x <= a[2] && y >= a[1] && y <= a[3]) {
            return false;
        }
        self.cosm.sets_scroll_to = (self.cosm.sets_scroll_to - dy * 3.2 * self.r).clamp(0.0, self.cosm.sets_content);
        true
    }
}
