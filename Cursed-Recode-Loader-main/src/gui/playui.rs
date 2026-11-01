//! The acting modules' cards (play.rs): Death Coords' list — the newest death first, each its place (pressed, copied),
//! its world and server, how long ago and of what; emptied by a button pressed twice, its rows going out together.

use super::draw::{grey, mix, Canvas};
use super::i18n::{tr, trn};
use super::play::{world_name, Death, KILL_LISTS};
use super::text::{fonts, Face};
use super::theme::pal;
use super::{ak, Card, Gui, Hit, A_HOV, A_POP};

/// The card's parts (Hit::Card): a row (copied), the button that empties the list.
const ROW: u8 = 80;
const CLEAR: u8 = 81;
const CLOSE: u8 = 6;

/// How long ago, said short.
fn ago(secs: u64) -> String {
    match secs {
        0..=59 => tr("just now").to_string(),
        60..=3599 => trn("{n} min ago", &[("n", (secs / 60).to_string())]),
        3600..=86_399 => trn("{n} h ago", &[("n", (secs / 3600).to_string())]),
        _ => trn("{n} days ago", &[("n", (secs / 86_400).to_string())]),
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

impl Gui {
    /// Its button in the module's settings: the card opens by the pointer (pressed again, it shuts).
    pub(super) fn deaths_open(&mut self) {
        if matches!(self.card_shut.take(), Some(Card::Deaths { .. })) {
            return;
        }
        self.deaths_load();
        let m = self.mouse;
        self.card = Some(Card::Deaths { at: [m[0], m[1], m[0], m[1]] });
        self.card_scroll = 0.0;
        self.card_scroll_to = 0.0;
        self.card_at = self.now;
        self.play_ui.clear_armed = None;
        self.anim.remove(&ak(A_POP, 2));
    }

    pub(super) fn deaths_card(&mut self, c: &mut Canvas, at: [f32; 4]) {
        let r = self.r;
        let k = self.an(ak(A_POP, 2), 1.0, 14.0, 0.0);
        let w = 26.0 * r;
        // the rows: the list, or the one going out (emptied a moment ago)
        let fading = self.play_ui.gone.as_ref().map(|(l, t)| (l.clone(), (1.0 - (self.now - t) / 0.35).clamp(0.0, 1.0)));
        if fading.as_ref().is_some_and(|f| f.1 <= 0.0) {
            self.play_ui.gone = None;
        }
        let (rows, rows_a): (Vec<Death>, f32) = match fading.filter(|f| f.1 > 0.0) {
            Some((l, a)) => (l, a),
            None => (self.play.deaths.iter().rev().cloned().collect(), 1.0),
        };
        let row_h = 3.3 * r;
        let list_h = (rows.len().max(1) as f32 * row_h).min(8.0 * row_h);
        let h = 5.4 * r + list_h + 3.8 * r;
        let mut x = at[0] - w / 2.0;
        let mut y = at[1] - h - 0.6 * r;
        if y < 8.0 {
            y = at[3] + 0.6 * r;
        }
        x = x.clamp(8.0, (self.w - w - 8.0).max(8.0));
        y = y.clamp(8.0, (self.h - h - 8.0).max(8.0));
        let rr = [x, y + (1.0 - k) * 0.5 * r, x + w, y + h + (1.0 - k) * 0.5 * r];
        self.card_rect = rr;
        let base = c.alpha;
        c.alpha = base * k;
        c.shadow(rr, r, 3.0 * r, grey(0.0, 0.62));
        let g = pal().glass;
        c.rect(rr, r, [g[0].max(0.05), g[1].max(0.05), g[2].max(0.055), 0.97]);
        self.hits.push((rr, Hit::Card(0, 0)));
        let (x0, x1) = (rr[0] + 1.2 * r, rr[2] - 1.2 * r);
        // the title, how many, close
        c.text(x0, rr[1] + 2.0 * r, 1.05 * r, Face::Bold, tr("Deaths"), pal().t1);
        let sub = if self.play.deaths.is_empty() { tr("None yet").to_string() } else { tr("The newest first; pressed, its place is copied").to_string() };
        c.text(x0, rr[1] + 3.2 * r, 0.72 * r, Face::Regular, &sub, pal().t3);
        let xb = [x1 - 1.8 * r, rr[1] + 0.95 * r, x1, rr[1] + 2.75 * r];
        let xv = self.an(ak(A_HOV, 9940), if self.hover == Hit::Card(CLOSE, 0) { 1.0 } else { 0.0 }, 12.0, 0.0);
        c.rect(xb, 0.55 * r, mix(pal().f1, pal().f3, xv));
        let xc = [(xb[0] + xb[2]) / 2.0, (xb[1] + xb[3]) / 2.0];
        let xs = 0.27 * r;
        let xcol = mix(pal().t3, pal().t1, xv);
        c.line([xc[0] - xs, xc[1] - xs], [xc[0] + xs, xc[1] + xs], 1.5, xcol, 0.0);
        c.line([xc[0] + xs, xc[1] - xs], [xc[0] - xs, xc[1] + xs], 1.5, xcol, 0.0);
        self.hits.push((xb, Hit::Card(CLOSE, 0)));
        // the list, scrolled within it; each row comes in after the one above it
        let top = rr[1] + 4.3 * r;
        let clip = [rr[0], top, rr[2], top + list_h];
        let max = (rows.len() as f32 * row_h - list_h).max(0.0);
        self.card_scroll_to = self.card_scroll_to.clamp(0.0, max);
        self.card_scroll += (self.card_scroll_to - self.card_scroll) * (1.0 - (-self.dt * 14.0).exp());
        c.push_clip(clip);
        if rows.is_empty() {
            for (n, l) in fonts().wrap(Face::Regular, 0.78 * r, tr("Your deaths will be here, newest first."), x1 - x0).iter().take(2).enumerate() {
                c.text(x0, top + 1.6 * r + n as f32 * 1.05 * r, 0.78 * r, Face::Regular, l, pal().t4);
            }
        }
        let now = now_secs();
        let since = self.now - self.card_at;
        for (n, d) in rows.iter().enumerate() {
            let yy = top + n as f32 * row_h - self.card_scroll;
            if yy + row_h < top - 1.0 || yy > top + list_h + 1.0 {
                continue;
            }
            let wave = super::smooth(0.0, 1.0, ((since - 0.04 * n as f32) / 0.3).clamp(0.0, 1.0));
            let a = rows_a * wave;
            c.alpha = base * k * a;
            let dy = (1.0 - wave) * 0.6 * r + (1.0 - rows_a) * -0.4 * r;
            let row = [x0 - 0.5 * r, yy + 0.15 * r + dy, x1 + 0.5 * r, yy + row_h - 0.15 * r + dy];
            let hit = Hit::Card(ROW, n as u16);
            let hv = self.an(ak(A_HOV, 31_000 + n), if self.hover == hit { 1.0 } else { 0.0 }, 14.0, 0.0);
            c.rect(row, 0.6 * r, grey(1.0, 0.035 + 0.06 * hv));
            // its place large, its world and server under it; how long ago and of what on the right
            let place = if self.streamer_hides("Coordinates") { "•••".to_string() } else { d.block() };
            c.text(x0, row[1] + 1.35 * r, 0.95 * r, Face::Mono, &place, mix(pal().t2, pal().t1, hv));
            let mut where_ = world_name(&d.dim);
            if !d.server.is_empty() {
                where_ = format!("{where_} · {}", d.server);
            }
            let ww = fonts().fit(Face::Regular, 0.68 * r, &where_, (x1 - x0) * 0.55);
            c.text(x0, row[1] + 2.5 * r, 0.68 * r, Face::Regular, &ww, pal().t3);
            let when = ago(now.saturating_sub(d.at));
            c.text_r(x1, row[1] + 1.35 * r, 0.7 * r, Face::Medium, &when, pal().t3);
            if !d.cause.is_empty() {
                let cw = fonts().fit(Face::Regular, 0.64 * r, &d.cause, (x1 - x0) * 0.42);
                c.text_r(x1, row[1] + 2.5 * r, 0.64 * r, Face::Regular, &cw, pal().t4);
            }
            self.hits.push((row, hit));
        }
        c.pop_clip();
        c.alpha = base * k;
        // empty the list: a second press does it (the button says so meanwhile)
        let armed = self.play_ui.clear_armed.is_some_and(|t| self.now - t < 3.0);
        if !armed {
            self.play_ui.clear_armed = None;
        }
        let bb = [x0, top + list_h + 1.1 * r, x1, top + list_h + 3.1 * r];
        let has = !self.play.deaths.is_empty();
        let bv = self.an(ak(A_HOV, 9941), if self.hover == Hit::Card(CLEAR, 0) && has { 1.0 } else { 0.0 }, 12.0, 0.0);
        let av = self.an(ak(A_HOV, 9942), if armed { 1.0 } else { 0.0 }, 12.0, 0.0);
        c.alpha = base * k * if has { 1.0 } else { 0.4 };
        c.rect(bb, 0.65 * r, mix(mix(pal().f1, pal().f3, bv), [0.62, 0.16, 0.16, 0.85], av));
        let label = if armed { tr("Press again to clear") } else { tr("Clear the list") };
        self.cosm_words(c, 9943, (bb[0] + bb[2]) / 2.0, bb[1] + 1.33 * r, 0.78 * r, Face::Medium, label, mix(pal().t2, pal().t1, bv.max(av)), true);
        if has {
            self.hits.push((bb, Hit::Card(CLEAR, 0)));
        }
        c.alpha = base;
    }

    /// A press on the card: true when it was its.
    pub(super) fn deaths_press(&mut self, h: Hit, button: i32) -> bool {
        match h {
            Hit::Card(ROW, n) => {
                if button == 0 {
                    if let Some(d) = self.play.deaths.iter().rev().nth(n as usize).cloned() {
                        self.copy(&d.block());
                        self.toast(trn("Copied: {pos}", &[("pos", d.block())]));
                        self.sound_tick(0.5);
                    }
                }
                true
            }
            Hit::Card(CLEAR, _) => {
                if button == 0 {
                    if self.play_ui.clear_armed.is_some_and(|t| self.now - t < 3.0) {
                        self.play_ui.gone = Some((self.play.deaths.iter().rev().cloned().collect(), self.now));
                        self.play_ui.clear_armed = None;
                        self.deaths_clear();
                        self.switch_sound(false, 0.6);
                    } else {
                        self.play_ui.clear_armed = Some(self.now);
                        self.sound_tick(0.6);
                    }
                }
                true
            }
            Hit::Card(CLOSE, _) => {
                self.card_shut = self.card.take();
                true
            }
            Hit::Card(..) => true,
            _ => {
                self.card_shut = self.card.take();
                false
            }
        }
    }
}

/// What the cards keep between frames: since when the list's clearing waits for its second press; the rows going out;
/// the friend whose removal waits for its second press; since when the field shakes (a wrong nick).
#[derive(Default)]
pub(super) struct PlayUi {
    pub clear_armed: Option<f32>,
    pub gone: Option<(Vec<Death>, f32)>,
    pub remove_armed: Option<(String, f32)>,
    pub wrong_at: Option<f32>,
    /// Kill Message's card: the line being changed, the list last shown and since when
    pub editing: Option<usize>,
    pub tab_seen: Option<usize>,
    pub tab_at: f32,
    pub message: super::messageui::State,
}

// ---------------------------------------------------------------------- No Friend Damage's friends
/// Its parts: a row's cross (removed on a second press), the field (a nick typed, Enter adds it).
const F_REMOVE: u8 = 82;
const F_FIELD: u8 = 83;

impl Gui {
    pub(super) fn friends_open(&mut self) {
        if matches!(self.card_shut.take(), Some(Card::Friends { .. })) {
            return;
        }
        self.friends_load();
        let m = self.mouse;
        self.card = Some(Card::Friends { at: [m[0], m[1], m[0], m[1]] });
        self.card_field.clear();
        self.card_scroll = 0.0;
        self.card_scroll_to = 0.0;
        self.card_at = self.now;
        self.play_ui.remove_armed = None;
        self.play_ui.wrong_at = None;
        self.anim.remove(&ak(A_POP, 2));
    }

    pub(super) fn friends_card(&mut self, c: &mut Canvas, at: [f32; 4]) {
        let r = self.r;
        let k = self.an(ak(A_POP, 2), 1.0, 14.0, 0.0);
        let w = 22.0 * r;
        let names = self.play.friends.clone();
        let row_h = 2.4 * r;
        let list_h = (names.len().max(1) as f32 * row_h).min(9.0 * row_h);
        let h = 5.4 * r + list_h + 3.6 * r;
        let mut x = at[0] - w / 2.0;
        let mut y = at[1] - h - 0.6 * r;
        if y < 8.0 {
            y = at[3] + 0.6 * r;
        }
        x = x.clamp(8.0, (self.w - w - 8.0).max(8.0));
        y = y.clamp(8.0, (self.h - h - 8.0).max(8.0));
        let rr = [x, y + (1.0 - k) * 0.5 * r, x + w, y + h + (1.0 - k) * 0.5 * r];
        self.card_rect = rr;
        let base = c.alpha;
        c.alpha = base * k;
        c.shadow(rr, r, 3.0 * r, grey(0.0, 0.62));
        let g = pal().glass;
        c.rect(rr, r, [g[0].max(0.05), g[1].max(0.05), g[2].max(0.055), 0.97]);
        self.hits.push((rr, Hit::Card(0, 0)));
        let (x0, x1) = (rr[0] + 1.2 * r, rr[2] - 1.2 * r);
        c.text(x0, rr[1] + 2.0 * r, 1.05 * r, Face::Bold, tr("Friends"), pal().t1);
        let sub = if names.is_empty() { tr("None yet").to_string() } else { trn("{n} on the list", &[("n", names.len().to_string())]) };
        c.text(x0, rr[1] + 3.2 * r, 0.72 * r, Face::Regular, &sub, pal().t3);
        let xb = [x1 - 1.8 * r, rr[1] + 0.95 * r, x1, rr[1] + 2.75 * r];
        let xv = self.an(ak(A_HOV, 9950), if self.hover == Hit::Card(CLOSE, 0) { 1.0 } else { 0.0 }, 12.0, 0.0);
        c.rect(xb, 0.55 * r, mix(pal().f1, pal().f3, xv));
        let xc = [(xb[0] + xb[2]) / 2.0, (xb[1] + xb[3]) / 2.0];
        let xs = 0.27 * r;
        let xcol = mix(pal().t3, pal().t1, xv);
        c.line([xc[0] - xs, xc[1] - xs], [xc[0] + xs, xc[1] + xs], 1.5, xcol, 0.0);
        c.line([xc[0] + xs, xc[1] - xs], [xc[0] - xs, xc[1] + xs], 1.5, xcol, 0.0);
        self.hits.push((xb, Hit::Card(CLOSE, 0)));
        let top = rr[1] + 4.3 * r;
        let clip = [rr[0], top, rr[2], top + list_h];
        let max = (names.len() as f32 * row_h - list_h).max(0.0);
        self.card_scroll_to = self.card_scroll_to.clamp(0.0, max);
        self.card_scroll += (self.card_scroll_to - self.card_scroll) * (1.0 - (-self.dt * 14.0).exp());
        c.push_clip(clip);
        if names.is_empty() {
            for (n, l) in fonts().wrap(Face::Regular, 0.74 * r, tr("Look at a player and press the key, or type a nick below."), x1 - x0).iter().take(2).enumerate() {
                c.text(x0, top + 1.2 * r + n as f32 * 1.0 * r, 0.74 * r, Face::Regular, l, pal().t4);
            }
        }
        let since = self.now - self.card_at;
        let armed = self.play_ui.remove_armed.clone().filter(|(_, t)| self.now - t < 3.0);
        if armed.is_none() {
            self.play_ui.remove_armed = None;
        }
        for (n, name) in names.iter().enumerate() {
            let yy = top + n as f32 * row_h - self.card_scroll;
            if yy + row_h < top - 1.0 || yy > top + list_h + 1.0 {
                continue;
            }
            let wave = super::smooth(0.0, 1.0, ((since - 0.04 * n as f32) / 0.3).clamp(0.0, 1.0));
            c.alpha = base * k * wave;
            let dy = (1.0 - wave) * 0.6 * r;
            let row = [x0 - 0.5 * r, yy + 0.12 * r + dy, x1 + 0.5 * r, yy + row_h - 0.12 * r + dy];
            let mid = (row[1] + row[3]) / 2.0;
            let del = Hit::Card(F_REMOVE, n as u16);
            let hv = self.an(ak(A_HOV, 32_000 + n), if self.hover == del { 1.0 } else { 0.0 }, 14.0, 0.0);
            let on = armed.as_ref().is_some_and(|(a, _)| a.eq_ignore_ascii_case(name));
            let av = self.an(ak(A_HOV, 33_000 + n), if on { 1.0 } else { 0.0 }, 12.0, 0.0);
            c.rect(row, 0.55 * r, mix(grey(1.0, 0.035), [0.62, 0.16, 0.16, 0.55], av));
            c.text(x0, mid + 0.3 * r, 0.84 * r, Face::Medium, name, pal().t1);
            if av > 0.01 {
                c.alpha = base * k * wave * av;
                c.text_r(x1 - 1.6 * r, mid + 0.26 * r, 0.66 * r, Face::Regular, tr("Press again to remove"), pal().t2);
                c.alpha = base * k * wave;
            }
            let cc = [x1 - 0.6 * r, mid];
            let s = 0.26 * r;
            let col = mix(pal().t4, pal().t1, hv.max(av));
            c.line([cc[0] - s, mid - s], [cc[0] + s, mid + s], 1.4, col, 0.0);
            c.line([cc[0] + s, mid - s], [cc[0] - s, mid + s], 1.4, col, 0.0);
            self.hits.push(([cc[0] - 0.9 * r, row[1], cc[0] + 0.9 * r, row[3]], del));
        }
        c.pop_clip();
        c.alpha = base * k;
        // the field: a nick typed straight in, Enter adds it (a wrong one shakes)
        let wrong = self.play_ui.wrong_at.map_or(0.0, |t| (1.0 - (self.now - t) / 0.4).clamp(0.0, 1.0));
        let shake = (self.now * 60.0).sin() * 0.25 * r * wrong;
        let fr = [x0 + shake, top + list_h + 0.7 * r, x1 + shake, top + list_h + 2.9 * r];
        c.rect(fr, 0.65 * r, mix(pal().f2, [0.55, 0.14, 0.14, 0.6], wrong));
        let pc = [fr[0] + 1.1 * r, (fr[1] + fr[3]) / 2.0];
        let pl = 0.36 * r;
        c.line([pc[0] - pl, pc[1]], [pc[0] + pl, pc[1]], 1.4, pal().t3, 0.0);
        c.line([pc[0], pc[1] - pl], [pc[0], pc[1] + pl], 1.4, pal().t3, 0.0);
        let base_y = fr[1] + 1.42 * r;
        if self.card_field.is_empty() {
            c.text(fr[0] + 1.9 * r, base_y, 0.8 * r, Face::Regular, tr("A nick to add"), pal().t4);
        }
        let tw = c.text(fr[0] + 1.9 * r, base_y, 0.8 * r, Face::Regular, &self.card_field, pal().t1);
        if (self.now * 2.0).fract() < 0.55 {
            c.rect([fr[0] + 1.95 * r + tw, fr[1] + 0.55 * r, fr[0] + 1.95 * r + tw + 1.5, fr[3] - 0.55 * r], 0.0, pal().accent);
        }
        self.hits.push((fr, Hit::Card(F_FIELD, 0)));
        c.alpha = base;
    }

    pub(super) fn friends_press(&mut self, h: Hit, button: i32) -> bool {
        match h {
            Hit::Card(F_REMOVE, n) => {
                if button == 0 {
                    if let Some(name) = self.play.friends.get(n as usize).cloned() {
                        let again = self.play_ui.remove_armed.as_ref().is_some_and(|(a, t)| a.eq_ignore_ascii_case(&name) && self.now - t < 3.0);
                        if again {
                            self.friend_remove(&name);
                            self.play_ui.remove_armed = None;
                            self.switch_sound(false, 0.5);
                        } else {
                            self.play_ui.remove_armed = Some((name, self.now));
                            self.sound_tick(0.6);
                        }
                    }
                }
                true
            }
            Hit::Card(CLOSE, _) => {
                self.card_shut = self.card.take();
                true
            }
            Hit::Card(..) => true,
            _ => {
                self.card_shut = self.card.take();
                false
            }
        }
    }

    /// Enter in the field: the nick typed added (a wrong one shakes the field).
    pub(super) fn friends_enter(&mut self) {
        let name = self.card_field.trim().to_string();
        if self.friend_add(&name) {
            self.card_field.clear();
            self.switch_sound(true, 0.5);
            // (the new one seen: the list scrolled to its end)
            self.card_scroll_to = f32::MAX;
        } else {
            self.play_ui.wrong_at = Some(self.now);
            self.sound_tick(0.4);
        }
    }
}

// ---------------------------------------------------------------------- Kill Message's lines
/// Its parts: a list's tab, a line (pressed: taken into the field to be changed), a line's cross, the field, a
/// placeholder's chip (put into the field).
const L_TAB: u8 = 84;
const L_ROW: u8 = 85;
const L_REMOVE: u8 = 86;
const L_FIELD: u8 = 87;
const L_PH: u8 = 88;
/// What a line may hold.
const KILL_PH: [&str; 7] = ["{name}", "{hp}", "{weapon}", "{distance}", "{streak}", "{me}", "{server}"];

fn tab_label(name: &str) -> &'static str {
    tr(match name {
        "Kill" => "A kill",
        "Streak" => "A streak",
        "Last hearts" => "Last hearts",
        "Long shot" => "A long shot",
        _ => "Revenge",
    })
}

impl Gui {
    pub(super) fn lines_open(&mut self, owner: usize) {
        if matches!(self.card_shut.take(), Some(Card::Lines { .. })) {
            return;
        }
        let m = self.mouse;
        self.card = Some(Card::Lines { i: owner, tab: 0, at: [m[0], m[1], m[0], m[1]] });
        self.message_open(owner);
        self.card_field.clear();
        self.card_scroll = 0.0;
        self.card_scroll_to = 0.0;
        self.card_at = self.now;
        self.play_ui.editing = None;
        self.play_ui.wrong_at = None;
        self.anim.remove(&ak(A_POP, 2));
    }

    fn lines_of(&self, i: usize, tab: usize) -> Vec<String> {
        self.message_lines(i, tab)
    }

    fn lines_put(&mut self, i: usize, tab: usize, lines: &[String]) {
        self.message_put(i, tab, lines);
    }

    pub(super) fn lines_card(&mut self, c: &mut Canvas, i: usize, tab: usize, at: [f32; 4]) {
        self.message_sync(i);
        let r = self.r;
        let k = self.an(ak(A_POP, 2), 1.0, 14.0, 0.0);
        // (as wide as its tabs need, at least 30)
        let tabs_w: f32 = KILL_LISTS.iter().map(|(n, _)| fonts().width(Face::Medium, 0.72 * r, tab_label(n)) + 1.75 * r).sum();
        let w = (30.0 * r).max(tabs_w + 2.4 * r);
        let lines = self.lines_of(i, tab);
        let row_h = 2.4 * r;
        let list_h = (lines.len().max(1) as f32 * row_h).min(8.0 * row_h).min((self.h - 16.0 - 19.6*r).max(row_h));
        let h = 13.2 * r + list_h + 6.4 * r;
        let mut x = at[0] - w / 2.0;
        let mut y = at[1] - h - 0.6 * r;
        if y < 8.0 {
            y = at[3] + 0.6 * r;
        }
        x = x.clamp(8.0, (self.w - w - 8.0).max(8.0));
        y = y.clamp(8.0, (self.h - h - 8.0).max(8.0));
        let rr = [x, y + (1.0 - k) * 0.5 * r, x + w, y + h + (1.0 - k) * 0.5 * r];
        self.card_rect = rr;
        let base = c.alpha;
        c.alpha = base * k;
        c.shadow(rr, r, 3.0 * r, grey(0.0, 0.62));
        let g = pal().glass;
        c.rect(rr, r, [g[0].max(0.05), g[1].max(0.05), g[2].max(0.055), 0.97]);
        self.hits.push((rr, Hit::Card(0, 0)));
        let (x0, x1) = (rr[0] + 1.2 * r, rr[2] - 1.2 * r);
        c.text(x0, rr[1] + 2.0 * r, 1.05 * r, Face::Bold, tr("Messages"), pal().t1);
        c.text(x0, rr[1] + 3.2 * r, 0.72 * r, Face::Regular, tr(match self.play_ui.message.field { 1 => "Enter a server address", 2 => "Number of kills in a row", _ => "Pressed, a line is yours to change; Enter keeps it" }), pal().t3);
        let xb = [x1 - 1.8 * r, rr[1] + 0.95 * r, x1, rr[1] + 2.75 * r];
        let xv = self.an(ak(A_HOV, 9960), if self.hover == Hit::Card(CLOSE, 0) { 1.0 } else { 0.0 }, 12.0, 0.0);
        c.rect(xb, 0.55 * r, mix(pal().f1, pal().f3, xv));
        let xc = [(xb[0] + xb[2]) / 2.0, (xb[1] + xb[3]) / 2.0];
        let xs = 0.27 * r;
        let xcol = mix(pal().t3, pal().t1, xv);
        c.line([xc[0] - xs, xc[1] - xs], [xc[0] + xs, xc[1] + xs], 1.5, xcol, 0.0);
        c.line([xc[0] + xs, xc[1] - xs], [xc[0] - xs, xc[1] + xs], 1.5, xcol, 0.0);
        self.hits.push((xb, Hit::Card(CLOSE, 0)));
        // the lists' tabs, a light under the one open sliding to it
        self.message_controls(c, x0, x1, rr[1] + 4.1*r, tab);
        let ty = rr[1] + 9.4 * r;
        let mut tx = x0;
        let mut lit = [0.0f32; 2];
        for (n, (name, _)) in KILL_LISTS.iter().enumerate() {
            let label = tab_label(name);
            let tw = fonts().width(Face::Medium, 0.72 * r, label) + 1.4 * r;
            let tr_ = [tx, ty, tx + tw, ty + 1.9 * r];
            let hit = Hit::Card(L_TAB, n as u16);
            let hv = self.an(ak(A_HOV, 9961 + n), if self.hover == hit { 1.0 } else { 0.0 }, 14.0, 0.0);
            if n == tab {
                lit = [tr_[0], tr_[2]];
            }
            c.rect(tr_, 0.95 * r, grey(1.0, 0.04 * hv));
            c.text(tx + 0.7 * r, ty + 1.28 * r, 0.72 * r, Face::Medium, label, if n == tab { pal().t1 } else { mix(pal().t3, pal().t2, hv) });
            self.hits.push((tr_, hit));
            tx += tw + 0.35 * r;
        }
        let lx0 = self.an(ak(A_HOV, 9970), lit[0], 16.0, lit[0]);
        let lx1 = self.an(ak(A_HOV, 9971), lit[1], 16.0, lit[1]);
        c.rect([lx0, ty, lx1, ty + 1.9 * r], 0.95 * r, grey(1.0, 0.09));
        // the lines (a new list: they come in again)
        if self.play_ui.tab_seen != Some(tab) {
            self.play_ui.tab_seen = Some(tab);
            self.play_ui.tab_at = self.now;
        }
        let top = ty + 2.6 * r;
        let clip = [rr[0], top, rr[2], top + list_h];
        let max = (lines.len() as f32 * row_h - list_h).max(0.0);
        self.card_scroll_to = self.card_scroll_to.clamp(0.0, max);
        self.card_scroll += (self.card_scroll_to - self.card_scroll) * (1.0 - (-self.dt * 14.0).exp());
        c.push_clip(clip);
        if lines.is_empty() {
            for (n, l) in fonts().wrap(Face::Regular, 0.74 * r, tr("No lines: nothing is said for this one."), x1 - x0).iter().take(2).enumerate() {
                c.text(x0, top + 1.2 * r + n as f32 * 1.0 * r, 0.74 * r, Face::Regular, l, pal().t4);
            }
        }
        let since = self.now - self.play_ui.tab_at;
        for (n, l) in lines.iter().enumerate() {
            let yy = top + n as f32 * row_h - self.card_scroll;
            if yy + row_h < top - 1.0 || yy > top + list_h + 1.0 {
                continue;
            }
            let wave = super::smooth(0.0, 1.0, ((since - 0.035 * n as f32) / 0.28).clamp(0.0, 1.0));
            c.alpha = base * k * wave;
            let dy = (1.0 - wave) * 0.5 * r;
            let row = [x0 - 0.5 * r, yy + 0.12 * r + dy, x1 + 0.5 * r, yy + row_h - 0.12 * r + dy];
            let mid = (row[1] + row[3]) / 2.0;
            let hit = Hit::Card(L_ROW, n as u16);
            let del = Hit::Card(L_REMOVE, n as u16);
            let hv = self.an(ak(A_HOV, 34_000 + n), if self.hover == hit || self.hover == del { 1.0 } else { 0.0 }, 14.0, 0.0);
            let ed = self.an(ak(A_HOV, 35_000 + n), if self.play_ui.editing == Some(n) { 1.0 } else { 0.0 }, 12.0, 0.0);
            c.rect(row, 0.55 * r, mix(grey(1.0, 0.035 + 0.04 * hv), grey(1.0, 0.12), ed));
            let shown = fonts().fit(Face::Regular, 0.8 * r, l, x1 - x0 - 2.0 * r);
            c.text(x0, mid + 0.28 * r, 0.8 * r, Face::Regular, &shown, mix(pal().t2, pal().t1, hv.max(ed)));
            self.hits.push(([row[0], row[1], x1 - 1.4 * r, row[3]], hit));
            let cc = [x1 - 0.6 * r, mid];
            let s = 0.26 * r;
            let dv = self.an(ak(A_HOV, 36_000 + n), if self.hover == del { 1.0 } else { 0.0 }, 14.0, 0.0);
            let col = mix(pal().t4, pal().t1, dv);
            c.line([cc[0] - s, mid - s], [cc[0] + s, mid + s], 1.4, col, 0.0);
            c.line([cc[0] + s, mid - s], [cc[0] - s, mid + s], 1.4, col, 0.0);
            self.hits.push(([cc[0] - 0.9 * r, row[1], cc[0] + 0.9 * r, row[3]], del));
        }
        c.pop_clip();
        c.alpha = base * k;
        // the field: a new line, or the one being changed (Enter keeps it; Escape lets it be)
        let wrong = self.play_ui.wrong_at.map_or(0.0, |t| (1.0 - (self.now - t) / 0.4).clamp(0.0, 1.0));
        let shake = (self.now * 60.0).sin() * 0.25 * r * wrong;
        let fr = [x0 + shake, top + list_h + 0.7 * r, x1 + shake, top + list_h + 2.9 * r];
        let ed = self.play_ui.editing.is_some();
        c.rect(fr, 0.65 * r, mix(mix(pal().f2, pal().f3, ed as i32 as f32 * 0.6), [0.55, 0.14, 0.14, 0.6], wrong));
        let pc = [fr[0] + 1.1 * r, (fr[1] + fr[3]) / 2.0];
        let pl = 0.36 * r;
        if ed {
            // a pen: the line is being changed
            c.line([pc[0] - pl, pc[1] + pl], [pc[0] + pl, pc[1] - pl], 1.6, pal().t2, 0.0);
        } else {
            c.line([pc[0] - pl, pc[1]], [pc[0] + pl, pc[1]], 1.4, pal().t3, 0.0);
            c.line([pc[0], pc[1] - pl], [pc[0], pc[1] + pl], 1.4, pal().t3, 0.0);
        }
        let base_y = fr[1] + 1.42 * r;
        if self.card_field.is_empty() {
            c.text(fr[0] + 1.9 * r, base_y, 0.8 * r, Face::Regular, tr(match self.play_ui.message.field { 1 => "Server address", 2 => "Exact streak", _ => "A new line" }), pal().t4);
        }
        let fw = x1 - x0 - 2.6 * r;
        let shown = if fonts().width(Face::Regular, 0.8 * r, &self.card_field) > fw {
            // (the end of a long line in view)
            let mut s: String = self.card_field.clone();
            while fonts().width(Face::Regular, 0.8 * r, &s) > fw && !s.is_empty() {
                s.remove(0);
            }
            s
        } else {
            self.card_field.clone()
        };
        let tw = c.text(fr[0] + 1.9 * r, base_y, 0.8 * r, Face::Regular, &shown, pal().t1);
        if (self.now * 2.0).fract() < 0.55 {
            c.rect([fr[0] + 1.95 * r + tw, fr[1] + 0.55 * r, fr[0] + 1.95 * r + tw + 1.5, fr[3] - 0.55 * r], 0.0, pal().accent);
        }
        self.hits.push((fr, Hit::Card(L_FIELD, 0)));
        // what a line may hold: pressed, put into the field
        let py = fr[3] + 0.6 * r;
        let mut px = x0;
        let show_placeholders = self.play_ui.message.field == 0;
        let words_w: f32 = KILL_PH.iter().map(|ph| fonts().width(Face::Mono, 0.66*r, ph)).sum();
        let chip_pad = ((x1-x0-words_w-0.3*r*(KILL_PH.len()-1) as f32)/KILL_PH.len() as f32).clamp(0.4*r, r);
        for (n, ph) in KILL_PH.iter().enumerate().filter(|_| show_placeholders) {
            let pw = fonts().width(Face::Mono, 0.66 * r, ph) + chip_pad;
            if px + pw > x1 {
                break;
            }
            let b = [px, py, px + pw, py + 1.6 * r];
            let hit = Hit::Card(L_PH, n as u16);
            let hv = self.an(ak(A_HOV, 9980 + n), if self.hover == hit { 1.0 } else { 0.0 }, 14.0, 0.0);
            c.rect(b, 0.8 * r, mix(pal().f1, pal().f3, hv));
            c.text(px + chip_pad*0.5, py + 1.1 * r, 0.66 * r, Face::Mono, ph, mix(pal().t3, pal().t1, hv));
            self.hits.push((b, hit));
            px += pw + 0.3 * r;
        }
        c.alpha = base;
    }

    pub(super) fn lines_press(&mut self, h: Hit, button: i32) -> bool {
        let Some(Card::Lines { i, tab, at }) = self.card.clone() else { return false };
        match h {
            Hit::Card(super::messageui::CONTROL, n) => {
                if button == 0 { self.message_control(n, i, tab); }
                true
            }
            Hit::Card(L_FIELD, _) => true,
            Hit::Card(L_TAB, n) => {
                if button == 0 && n as usize != tab {
                    self.card = Some(Card::Lines { i, tab: n as usize, at });
                    self.message_field_reset();
                    self.sound_tick(0.5);
                }
                true
            }
            Hit::Card(L_ROW, n) => {
                if button == 0 {
                    if let Some(l) = self.lines_of(i, tab).get(n as usize) {
                        self.card_field = l.clone();
                        self.play_ui.message.field = 0;
                        self.play_ui.editing = Some(n as usize);
                        self.sound_tick(0.4);
                    }
                }
                true
            }
            Hit::Card(L_REMOVE, n) => {
                if button == 0 {
                    let mut lines = self.lines_of(i, tab);
                    if (n as usize) < lines.len() {
                        lines.remove(n as usize);
                        self.lines_put(i, tab, &lines);
                        if self.play_ui.editing == Some(n as usize) {
                            self.play_ui.editing = None;
                            self.card_field.clear();
                        } else if let Some(edit) = self.play_ui.editing.as_mut() {
                            if *edit > n as usize { *edit -= 1; }
                        }
                        self.switch_sound(false, 0.4);
                    }
                }
                true
            }
            Hit::Card(L_PH, n) => {
                if button == 0 {
                    if let Some(ph) = KILL_PH.get(n as usize) {
                        if self.card_field.chars().count() + ph.len() <= 240 {
                            self.card_field.push_str(ph);
                            self.sound_tick(0.4);
                        }
                    }
                }
                true
            }
            Hit::Card(CLOSE, _) => {
                self.card_shut = self.card.take();
                true
            }
            Hit::Card(..) => true,
            _ => {
                self.card_shut = self.card.take();
                false
            }
        }
    }

    /// Enter: the line typed kept (a new one at the end, or the one being changed in its place).
    pub(super) fn lines_enter(&mut self) {
        let Some(Card::Lines { i, tab, .. }) = self.card.clone() else { return };
        if self.message_enter(i) { return; }
        let line = self.card_field.trim().replace(['\u{1e}', '\u{1f}', '\n', '\r', '\t'], " ");
        if line.is_empty() {
            self.play_ui.wrong_at = Some(self.now);
            return;
        }
        let mut lines = self.lines_of(i, tab);
        match self.play_ui.editing.take() {
            Some(n) if n < lines.len() => lines[n] = line,
            _ => {
                lines.push(line);
                self.card_scroll_to = f32::MAX;
            }
        }
        self.lines_put(i, tab, &lines);
        self.card_field.clear();
        self.switch_sound(true, 0.45);
    }

    /// Escape in the field while a line is being changed: it is let be (the card stays).
    pub(super) fn lines_escape(&mut self) -> bool {
        if self.play_ui.message.field != 0 {
            self.message_field_reset();
            return true;
        }
        if self.play_ui.editing.take().is_some() {
            self.card_field.clear();
            return true;
        }
        false
    }
}
