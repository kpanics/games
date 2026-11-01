//! Alt Manager's card in the client's menu (crate::alts keeps the accounts): the account's row at the bottom left opens
//! it over itself — every account (its face, Java draws it where "altfaces" says; its name; a tick by the one in use,
//! the licence's mark by a Microsoft one, a cross to take one out, pressed twice), a field for a new nickname typed
//! straight in (Enter adds it), and the ways to add: Microsoft's sign-in, a random nickname (into the field), the free
//! Cursed's accounts (while it has some not here); under them what a sign-in is doing. In a world a new one is the next
//! join's. Its words are the menus' card's (menus::alts), the ones it has of its own the menu's.

use super::draw::{grey, mix, Canvas};
use super::i18n::tr;
use super::text::{fonts, Face};
use super::theme::pal;
use super::{ak, Card, Gui, Hit, A_HOV, A_POP};
use crate::menus::lang;

/// The card's parts (Hit::Card): a row, its cross, the field, Microsoft, a random nickname, the free Cursed's.
const ROW: u8 = 80;
const DEL: u8 = 81;
const FIELD: u8 = 82;
const MS: u8 = 83;
const RANDOM: u8 = 84;
const CURSED: u8 = 85;
/// How many rows show at once (more scroll).
const ROWS: usize = 6;

/// A face for Java to draw: the account's place in the list, where (x, y, side), how seen, the part it is cut to.
pub struct AltFace {
    pub i: usize,
    pub at: [f32; 3],
    pub alpha: f32,
    pub clip: [f32; 4],
}

impl Gui {
    pub(super) fn open_alts(&mut self, at: [f32; 4]) {
        if matches!(self.card_shut.take(), Some(Card::Alts { .. })) {
            return;
        }
        self.card = Some(Card::Alts { at });
        self.card_at = self.now;
        self.card_field.clear();
        self.card_scroll = 0.0;
        self.card_scroll_to = 0.0;
        self.alt_armed = None;
        self.alt_wrong = None;
        self.alt_cursed = crate::alts::cursed_new();
        self.anim.remove(&ak(A_POP, 2));
        self.sound_tick(0.6);
    }

    pub(super) fn alts_card(&mut self, c: &mut Canvas, at: [f32; 4]) {
        let r = self.r;
        let k = self.an(ak(A_POP, 2), 1.0, 14.0, 0.0);
        let alts = crate::alts::list();
        let active = crate::alts::active();
        let w = 24.0 * r;
        let rows_h = 2.5 * r;
        let list_h = alts.len().clamp(1, ROWS) as f32 * rows_h;
        let (x0w, x1w) = (1.2 * r, w - 1.2 * r);
        let status = self.alt_status();
        let status_lines = status.as_ref().map_or(Vec::new(), |(s, _)| fonts().wrap(Face::Regular, 0.66 * r, s, x1w - x0w));
        // a line kept for it always (the card would move under the pointer as one comes and goes)
        let status_h = 0.5 * r + status_lines.len().clamp(1, 2) as f32 * 0.95 * r;
        let h = 4.1 * r + list_h + 0.5 * r + 2.2 * r + 0.6 * r + 2.0 * r + status_h + 1.1 * r;
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
        let (x0, x1) = (rr[0] + x0w, rr[0] + x1w);
        c.text(x0, rr[1] + 2.0 * r, 1.05 * r, Face::Bold, tr("Accounts"), pal().t1);
        c.text(x0, rr[1] + 3.2 * r, 0.72 * r, Face::Regular, lang::tr("alts.next"), pal().t3);

        // the accounts, scrolled within their part
        self.card_scroll += (self.card_scroll_to - self.card_scroll) * (1.0 - (-self.dt * 14.0).exp());
        let top = rr[1] + 4.1 * r;
        let clip = [rr[0], top, rr[2], top + list_h];
        let max = (alts.len() as f32 * rows_h - list_h).max(0.0);
        self.card_scroll_to = self.card_scroll_to.clamp(0.0, max);
        c.push_clip(clip);
        let armed = self.alt_armed.clone().filter(|(_, t)| self.now - t < 3.0);
        for (n, a) in alts.iter().enumerate() {
            let yy = top + n as f32 * rows_h - self.card_scroll;
            if yy + rows_h < top - 1.0 || yy > top + list_h + 1.0 {
                continue;
            }
            let row = [x0 - 0.5 * r, yy + 0.1 * r, x1 + 0.5 * r, yy + rows_h - 0.1 * r];
            let cur = a.name == active;
            let hit = Hit::Card(ROW, n as u16);
            let del = Hit::Card(DEL, n as u16);
            let over = self.hover == hit || self.hover == del;
            let hv = self.an(ak(A_HOV, 19_000 + n), if over { 1.0 } else if cur { 0.7 } else { 0.0 }, 14.0, 0.0);
            c.rect(row, 0.55 * r, grey(1.0, 0.07 * hv));
            let mid = (row[1] + row[3]) / 2.0;
            let fs = 1.45 * r;
            self.alt_faces.push(AltFace { i: n, at: [x0, mid - fs / 2.0, fs], alpha: k, clip });
            let tx = x0 + fs + 0.75 * r;
            let nw = c.text(tx, mid + 0.3 * r, 0.84 * r, Face::Medium, &a.name, if cur { pal().t1 } else { mix(pal().t2, pal().t1, hv) });
            if a.ms.is_some() {
                let label = lang::tr("alts.licence");
                let lw = fonts().width(Face::Medium, 0.6 * r, label) + 0.8 * r;
                let lx = tx + nw + 0.6 * r;
                c.rect([lx, mid - 0.55 * r, lx + lw, mid + 0.55 * r], 0.3 * r, pal().f2);
                c.text(lx + 0.4 * r, mid + 0.22 * r, 0.6 * r, Face::Medium, label, pal().t2);
            }
            self.hits.push((clipped(row, clip), hit));
            if cur {
                let cc = [x1 - 0.4 * r, mid];
                c.polyline(&[[cc[0] - 0.36 * r, cc[1]], [cc[0] - 0.1 * r, cc[1] + 0.26 * r], [cc[0] + 0.36 * r, cc[1] - 0.28 * r]], 1.6, pal().t1);
                continue;
            }
            // its cross (not the one in use): pressed once it asks, again it takes the account out
            let asks = armed.as_ref().is_some_and(|(name, _)| *name == a.name);
            if !asks && hv < 0.01 {
                continue;
            }
            let dr = if asks {
                let label = lang::tr("alts.delete");
                let lw = fonts().width(Face::Medium, 0.66 * r, label) + 1.0 * r;
                let dr = [x1 + 0.2 * r - lw, mid - 0.7 * r, x1 + 0.2 * r, mid + 0.7 * r];
                let dv = self.an(ak(A_HOV, 19_200 + n), if self.hover == del { 1.0 } else { 0.0 }, 14.0, 0.0);
                c.rect(dr, 0.35 * r, mix(pal().f2, pal().f3, dv));
                c.text(dr[0] + 0.5 * r, mid + 0.24 * r, 0.66 * r, Face::Medium, label, pal().t1);
                dr
            } else {
                let cc = [x1 - 0.4 * r, mid];
                let dv = self.an(ak(A_HOV, 19_200 + n), if self.hover == del { 1.0 } else { 0.0 }, 14.0, 0.0);
                let s = 0.26 * r;
                let col = mix(pal().t4, pal().t1, dv);
                let col = [col[0], col[1], col[2], col[3] * hv];
                c.line([cc[0] - s, mid - s], [cc[0] + s, mid + s], 1.4, col, 0.0);
                c.line([cc[0] + s, mid - s], [cc[0] - s, mid + s], 1.4, col, 0.0);
                [cc[0] - 0.8 * r, mid - 0.8 * r, cc[0] + 0.8 * r, mid + 0.8 * r]
            };
            self.hits.push((clipped(dr, clip), del));
        }
        c.pop_clip();

        // the field for a new nickname: typed straight in
        let fr = [x0, top + list_h + 0.5 * r, x1, top + list_h + 2.7 * r];
        c.rect(fr, 0.65 * r, pal().f2);
        let pc = [fr[0] + 1.1 * r, (fr[1] + fr[3]) / 2.0];
        let pl = 0.36 * r;
        c.line([pc[0] - pl, pc[1]], [pc[0] + pl, pc[1]], 1.4, pal().t3, 0.0);
        c.line([pc[0], pc[1] - pl], [pc[0], pc[1] + pl], 1.4, pal().t3, 0.0);
        let base_y = fr[1] + 1.42 * r;
        if self.card_field.is_empty() {
            c.text(fr[0] + 1.9 * r, base_y, 0.8 * r, Face::Regular, lang::tr("alts.new"), pal().t4);
        }
        let tw = c.text(fr[0] + 1.9 * r, base_y, 0.8 * r, Face::Regular, &self.card_field, pal().t1);
        if (self.now * 2.0).fract() < 0.55 {
            c.rect([fr[0] + 1.95 * r + tw, fr[1] + 0.55 * r, fr[0] + 1.95 * r + tw + 1.5, fr[3] - 0.55 * r], 0.0, pal().accent);
        }
        self.hits.push((fr, Hit::Card(FIELD, 0)));

        // the ways to add
        let by = fr[3] + 0.6 * r;
        let mut bx = x0;
        let mut buttons = vec![(MS, "Microsoft".to_string()), (RANDOM, lang::tr("alts.random").to_string())];
        if self.alt_cursed > 0 {
            buttons.push((CURSED, lang::trf("alts.cursed", &[&self.alt_cursed.to_string()])));
        }
        for (part, label) in buttons {
            let hit = Hit::Card(part, 0);
            let bw = fonts().width(Face::Medium, 0.72 * r, &label) + 1.6 * r;
            let b = [bx, by, bx + bw, by + 2.0 * r];
            let bv = self.an(ak(A_HOV, 19_400 + part as usize), if self.hover == hit { 1.0 } else { 0.0 }, 14.0, 0.0);
            c.rect(b, 0.55 * r, mix(pal().f1, pal().f3, bv));
            c.text(b[0] + 0.8 * r, by + 1.26 * r, 0.72 * r, Face::Medium, &label, mix(pal().t2, pal().t1, bv));
            self.hits.push((b, hit));
            bx += bw + 0.5 * r;
        }
        // what goes on: a word on the nickname typed, else what a sign-in does
        if let Some((_, bright)) = status {
            let col = if bright { pal().t1 } else { pal().t3 };
            for (n, l) in status_lines.iter().take(2).enumerate() {
                c.text(x0, by + 2.0 * r + 1.15 * r + n as f32 * 0.95 * r, 0.66 * r, Face::Regular, l, col);
            }
        }
        c.alpha = base;
    }

    /// The line under the card: what is wrong with the nickname typed (a few seconds), else what a sign-in does (while
    /// it works, and a while after it is done or went wrong); its words and whether it is a wrong one.
    fn alt_status(&self) -> Option<(String, bool)> {
        if let Some((w, at)) = self.alt_wrong {
            if self.now - at < 4.0 {
                return Some((word(w).to_string(), true));
            }
        }
        let st = crate::alts::status();
        if st.text.is_empty() {
            return None;
        }
        let age = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64()) - st.at;
        if st.kind != 0 && age > 8.0 {
            return None;
        }
        let mut s = word(&st.text).to_string();
        if !st.detail.is_empty() {
            s.push(' ');
            s.push_str(&st.detail);
        }
        if st.kind == 0 {
            s.push_str(&".".repeat(1 + (self.now * 2.5) as usize % 3));
        }
        Some((s, st.kind == 2))
    }

    /// A press while the card is open: true when it was the card's.
    pub(super) fn alts_press(&mut self, h: Hit, button: i32) -> bool {
        let Hit::Card(part, n) = h else {
            self.card_shut = self.card.take();
            return false;
        };
        if button != 0 {
            return true;
        }
        let alts = crate::alts::list();
        match part {
            ROW => {
                if let Some(a) = alts.get(n as usize) {
                    if a.name != crate::alts::active() {
                        crate::alts::use_alt(&a.name);
                        self.switch_sound(true, 0.5);
                    }
                    self.alt_armed = None;
                }
            }
            DEL => {
                if let Some(a) = alts.get(n as usize) {
                    let again = self.alt_armed.as_ref().is_some_and(|(name, t)| *name == a.name && self.now - t < 3.0);
                    if again {
                        crate::alts::remove(&a.name);
                        self.alt_armed = None;
                        self.alt_cursed = crate::alts::cursed_new();
                    } else {
                        self.alt_armed = Some((a.name.clone(), self.now));
                    }
                    self.sound_tick(0.8);
                }
            }
            MS => {
                crate::alts::ask_login();
                self.sound_tick(1.0);
            }
            RANDOM => {
                self.card_field = crate::alts::random();
                self.alt_wrong = None;
                self.sound_tick(0.8);
            }
            CURSED => {
                let got = crate::alts::take_cursed();
                crate::alts::set_status(1, "Taken from Cursed", &got.to_string());
                self.alt_cursed = crate::alts::cursed_new();
                self.sound_tick(1.0);
            }
            _ => {}
        }
        true
    }

    /// Enter in the field: the nickname typed added (and the field emptied), or what is wrong with it.
    pub(super) fn alts_enter(&mut self) {
        let name = self.card_field.trim().to_string();
        if name.is_empty() {
            return;
        }
        match crate::alts::add(&name) {
            Ok(()) => {
                self.card_field.clear();
                self.alt_wrong = None;
                self.switch_sound(true, 0.5);
            }
            Err(w) => self.alt_wrong = Some((w, self.now)),
        }
    }

    /// The faces Java draws this frame, where the window is drawn (the glass's opening scales and lifts it): "i x y side
    /// alpha x0 y0 x1 y1" a line each.
    pub(super) fn alt_faces_text(&self) -> String {
        let [mx, my, sc, drop, wa] = self.xf;
        let at = |x: f32, y: f32| (mx + (x - mx) * sc, my + (y - my) * sc + drop);
        let mut out = String::new();
        for f in &self.alt_faces {
            let (x, y) = at(f.at[0], f.at[1]);
            let (c0x, c0y) = at(f.clip[0], f.clip[1]);
            let (c1x, c1y) = at(f.clip[2], f.clip[3]);
            out.push_str(&format!("{} {x:.1} {y:.1} {:.1} {:.3} {c0x:.1} {c0y:.1} {c1x:.1} {c1y:.1}\n", f.i, f.at[2] * sc, f.alpha * wa));
        }
        out
    }
}

/// `r` cut to `clip` (a row half out of its list is pressed only where it shows).
fn clipped(r: [f32; 4], clip: [f32; 4]) -> [f32; 4] {
    [r[0].max(clip[0]), r[1].max(clip[1]), r[2].min(clip[2]), r[3].min(clip[3])]
}

/// A word of the card's in the game's language: the menus' own by its key (the status's English words are their own
/// keys), else as it is.
fn word(key: &str) -> &str {
    if lang::known(key) {
        lang::tr(key)
    } else {
        key
    }
}
