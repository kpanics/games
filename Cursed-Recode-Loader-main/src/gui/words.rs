//! The words page (Theme → "Edit the words"): every word of the menu and the HUD, each changed as the player likes
//! (kept as the theme's, shared with its code), each back to the client's own with its round arrow.

use super::draw::{grey, mix, Canvas};
use super::i18n::{self, tr};
use super::text::{fonts, Face};
use super::theme::pal;
use super::{ak, reset_icon, Gui, Hit, A_HOV};

impl Gui {
    /// Every word there is to change: the menu's and the HUD's, then the modules' names.
    pub(super) fn word_keys(&self) -> Vec<&'static str> {
        let mut v: Vec<&'static str> = i18n::keys();
        for m in &self.mods {
            if !v.contains(&m.name) {
                v.push(m.name);
            }
        }
        v
    }

    /// The words the page's search answers (in the word itself, its English, what it is changed to).
    pub(super) fn words_found(&self) -> Vec<&'static str> {
        let q = self.word_q.to_lowercase();
        self.word_keys()
            .into_iter()
            .filter(|k| {
                if q.is_empty() {
                    return true;
                }
                k.to_lowercase().contains(&q) || i18n::plain(k).to_lowercase().contains(&q) || i18n::word(k).is_some_and(|w| w.to_lowercase().contains(&q))
            })
            .collect()
    }

    pub(super) fn words_page(&mut self, c: &mut Canvas, area: [f32; 4]) {
        let r = self.r;
        let (a, dyv) = self.rise_c(0.22, 0.0, false);
        c.alpha = a;
        let top = area[1] + dyv;
        // the search, and putting every word back
        let sr = [area[0], top, area[2] - 12.5 * r, top + 2.3 * r];
        let sv = self.an(ak(A_HOV, 15_000), if self.word_focus.is_none() { 1.0 } else { 0.0 }, 12.0, 0.0);
        c.rect(sr, 0.75 * r, mix(pal().f1, pal().f2, sv));
        let ic = [sr[0] + 1.25 * r, sr[1] + 1.1 * r];
        c.ring([ic[0] - 0.1 * r, ic[1] - 0.1 * r], 0.36 * r, 1.4, pal().t3);
        c.line([ic[0] + 0.17 * r, ic[1] + 0.17 * r], [ic[0] + 0.45 * r, ic[1] + 0.45 * r], 1.4, pal().t3, 0.0);
        if self.word_q.is_empty() {
            c.text(sr[0] + 2.05 * r, sr[1] + 1.45 * r, 0.8 * r, Face::Regular, tr("Find a word"), pal().t4);
        }
        let tw = c.text(sr[0] + 2.05 * r, sr[1] + 1.45 * r, 0.8 * r, Face::Regular, &self.word_q.clone(), pal().t1);
        if self.word_focus.is_none() && (self.now * 2.0).fract() < 0.55 {
            c.rect([sr[0] + 2.1 * r + tw, sr[1] + 0.6 * r, sr[0] + 2.1 * r + tw + 1.5, sr[3] - 0.6 * r], 0.0, pal().accent);
        }
        self.hits.push((sr, Hit::Card(60, 0)));
        let changed = i18n::words_now().len();
        let rb = [area[2] - 12.0 * r, top, area[2], top + 2.3 * r];
        let rv = self.an(ak(A_HOV, 15_001), if self.hover == Hit::Card(61, 0) { 1.0 } else { 0.0 }, 12.0, 0.0);
        let base = c.alpha;
        if changed == 0 {
            c.alpha = base * 0.4;
        }
        c.rect(rb, 0.75 * r, mix(pal().f2, pal().f4, rv));
        reset_icon(c, [rb[0] + 1.3 * r, (rb[1] + rb[3]) / 2.0], 0.4 * r, mix(pal().t3, pal().t1, rv));
        c.text(rb[0] + 2.3 * r, rb[1] + 1.45 * r, 0.78 * r, Face::Medium, &i18n::trn("All words back · {n}", &[("n", changed.to_string())]), pal().t1);
        c.alpha = base;
        if changed > 0 {
            self.hits.push((rb, Hit::Card(61, 0)));
        }
        // the words: drawn only where seen (there are hundreds)
        let list = self.words_found();
        let rh = 2.55 * r;
        let ly = top + 3.1 * r;
        let clip = [area[0], ly, area[2], area[3]];
        c.push_clip(clip);
        let total = list.len() as f32 * rh;
        self.scroll_clamp([area[0], ly, area[2], area[3]], ly + total - self.scroll);
        let first = ((self.scroll / rh).floor() as usize).min(list.len());
        let shown = ((area[3] - ly) / rh).ceil() as usize + 2;
        for (n, &key) in list.iter().enumerate().skip(first).take(shown) {
            let y = ly + n as f32 * rh - self.scroll;
            let row = [area[0], y + 0.1 * r, area[2], y + rh - 0.1 * r];
            let hit = Hit::Word(n as u32);
            let focus = self.word_focus.as_deref() == Some(key);
            let hv = self.an(ak(A_HOV, 16_000 + n), if self.hover == hit || focus { 1.0 } else { 0.0 }, 14.0, 0.0);
            c.rect(row, 0.6 * r, grey(1.0, 0.025 + 0.04 * hv));
            let mid = (row[1] + row[3]) / 2.0;
            let half = (row[2] - row[0]) * 0.46;
            // the client's own word on the left, what it is now on the right
            let own = fonts().fit(Face::Regular, 0.74 * r, i18n::plain(key), half - 1.4 * r);
            c.text(row[0] + 0.9 * r, mid + 0.26 * r, 0.74 * r, Face::Regular, &own, pal().t3);
            let fx = row[0] + half;
            let over = i18n::word(key);
            let now: String = if focus { self.word_buf.clone() } else { over.map_or_else(|| i18n::plain(key).to_string(), |w| w.to_string()) };
            let field = [fx, row[1] + 0.3 * r, row[2] - 2.4 * r, row[3] - 0.3 * r];
            if focus {
                c.rect(field, 0.45 * r, pal().f3);
            }
            c.push_clip([field[0] + 0.2 * r, field[1], field[2] - 0.2 * r, field[3]]);
            let shownw = if focus { now.clone() } else { fonts().fit(Face::Medium, 0.8 * r, &now, field[2] - field[0] - 1.2 * r) };
            let w = c.text(field[0] + 0.6 * r, mid + 0.28 * r, 0.8 * r, Face::Medium, &shownw, if over.is_some() || focus { pal().t1 } else { pal().t2 });
            if focus && (self.now * 2.0).fract() < 0.55 {
                c.rect([field[0] + 0.65 * r + w, mid - 0.5 * r, field[0] + 0.65 * r + w + 1.5, mid + 0.5 * r], 0.0, pal().accent);
            }
            c.pop_clip();
            self.hits.push((row, hit));
            if over.is_some() {
                let ic = [row[2] - 1.2 * r, mid];
                let rv = self.an(ak(A_HOV, 17_000 + n), if self.hover == Hit::WordBack(n as u32) { 1.0 } else { 0.0 }, 14.0, 0.0);
                c.rect([ic[0] - 0.75 * r, ic[1] - 0.75 * r, ic[0] + 0.75 * r, ic[1] + 0.75 * r], 0.75 * r, grey(1.0, 0.03 + 0.09 * rv));
                reset_icon(c, ic, 0.36 * r, mix(pal().t3, pal().t1, rv));
                self.hits.push(([ic[0] - 0.8 * r, ic[1] - 0.8 * r, ic[0] + 0.8 * r, ic[1] + 0.8 * r], Hit::WordBack(n as u32)));
            }
        }
        if list.is_empty() {
            c.text(area[0] + 0.9 * r, ly + 1.5 * r, 0.8 * r, Face::Regular, tr("No such word"), pal().t4);
        }
        c.pop_clip();
        c.alpha = 1.0;
    }

    /// A press on the words' page: a word to change, one put back, all put back, the search.
    pub(super) fn words_press(&mut self, h: Hit) -> bool {
        match h {
            Hit::Word(n) => {
                let list = self.words_found();
                if let Some(&key) = list.get(n as usize) {
                    self.word_end();
                    self.word_focus = Some(key.to_string());
                    self.word_buf = i18n::word(key).map_or_else(|| i18n::plain(key).to_string(), |w| w.to_string());
                    self.sound_tick(0.5);
                }
            }
            Hit::WordBack(n) => {
                let list = self.words_found();
                if let Some(&key) = list.get(n as usize) {
                    if self.word_focus.as_deref() == Some(key) {
                        self.word_focus = None;
                    }
                    i18n::set_word(key, None);
                    self.save();
                    self.sound_tick(0.5);
                }
            }
            Hit::Card(61, _) => {
                self.word_focus = None;
                i18n::clear_words();
                self.save();
                self.switch_sound(false, 0.6);
                self.toast(tr("Every word is the client's own again").to_string());
            }
            Hit::Card(60, _) => {
                self.word_end();
            }
            _ => return false,
        }
        true
    }

    /// The word being changed is done (left as the client's own if it was not changed).
    pub(super) fn word_end(&mut self) {
        if let Some(key) = self.word_focus.take() {
            let v = self.word_buf.trim().to_string();
            let own = i18n::plain(&key).to_string();
            i18n::set_word(&key, if v.is_empty() || v == own { None } else { Some(&v) });
            self.save();
        }
    }

    /// A letter on the words' page: into the word being changed, or the search.
    pub(super) fn words_char(&mut self, ch: char) {
        if let Some(key) = self.word_focus.clone() {
            if self.word_buf.chars().count() < 80 {
                self.word_buf.push(ch);
                let v = self.word_buf.clone();
                i18n::set_word(&key, Some(&v));
            }
        } else if self.word_q.chars().count() < 30 {
            self.word_q.push(ch);
            self.scroll_to = 0.0;
        }
    }

    /// A key on the words' page; true when it was its.
    pub(super) fn words_key(&mut self, escape: bool, enter: bool, backspace: bool) -> bool {
        if self.word_focus.is_some() {
            if backspace {
                self.word_buf.pop();
                let (k, v) = (self.word_focus.clone().unwrap(), self.word_buf.clone());
                i18n::set_word(&k, if v.is_empty() { None } else { Some(&v) });
            } else if escape || enter {
                self.word_end();
            }
            return true;
        }
        if backspace {
            self.word_q.pop();
            return true;
        }
        if escape && !self.word_q.is_empty() {
            self.word_q.clear();
            return true;
        }
        let _ = enter;
        false
    }
}
