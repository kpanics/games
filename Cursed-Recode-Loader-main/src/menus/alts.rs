//! Alt Manager's card in the menus (the main menu's and the pause's): pressed, the nickname at the top right opens it
//! under itself on glass — every account (its face, the game draws it; its name; a lamp under the one in use; the
//! licence's mark by a Microsoft one; a cross to take it out, pressed twice), a field for a new nickname (Enter adds it),
//! and the ways to add: Microsoft's sign-in, a random nickname (into the field, Enter takes it), the free Cursed's
//! accounts (while it has some not here); under them what a sign-in is doing. The list itself is crate::alts's.

use super::controls::Field;
use super::lang::{self, tr, trf};
use super::ui::{clamp01, ease, id, tw, BONE};
use super::{out, Event, Menus};
use crate::gui::draw::{grey, Canvas};
use crate::gui::text::Face;

/// The card's elements (their kind: no page's own).
const K: u32 = 0xA17;
const OUT: u32 = 1;
const BG: u32 = 2;
const FIELD: u32 = 3;
const MS: u32 = 4;
const RANDOM: u32 = 5;
const CURSED: u32 = 6;
const ROW: u32 = 100;
const DEL: u32 = 200;
/// A face of the list for the game to draw (Java's Extras: kind 3, the account's place in the list), and the list's
/// part they are cut to as it scrolls (kind 4: its bottom in the number, then its left, top, right).
pub const FACE: u32 = 3;
pub const CLIP: u32 = 4;
/// How many rows show at once (more scroll).
const ROWS: usize = 7;
/// The nickname at the top right (the page's): it opens the card.
pub const WHO: u64 = id(K, 999);

pub struct AltCard {
    open: bool,
    field: Option<Field>,
    /// What is wrong with the nickname typed, since when.
    wrong: Option<(&'static str, f32)>,
    /// A cross pressed once: whose, when (the second press within 3 s takes it out).
    armed: Option<(String, f32)>,
    scroll: f32,
    box_: [f32; 4],
    /// The free Cursed's accounts not here yet (looked at as it opens).
    cursed: usize,
}

impl AltCard {
    pub fn new() -> AltCard {
        AltCard { open: false, field: None, wrong: None, armed: None, scroll: 0.0, box_: [0.0; 4], cursed: 0 }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn toggle(&mut self, m: &Menus) {
        self.open = !self.open;
        self.field = None;
        self.armed = None;
        if self.open {
            self.cursed = crate::alts::cursed_new();
        }
        m.tick_sound(1.0);
    }

    /// The card under `who` (the nickname's box: its right edge is the card's), seen as much as it is open. `live`: the
    /// page takes input; `in_world`: a new nickname is the next join's.
    pub fn draw(&mut self, m: &mut Menus, c: &mut Canvas, who: [f32; 4], live: bool, in_world: bool) {
        let p = m.an(id(K, 900), if self.open { 1.0 } else { 0.0 }, 9.0);
        m.out[out::TYPING] = if self.open && self.field.is_some() { 1.0 } else { 0.0 };
        if p < 0.01 {
            return;
        }
        let e = ease(p);
        let (ui, vh) = (m.uipx(), m.vh());
        let size = m.cl(9.0, 18.0, 1.15);
        let row = 2.6 * size;
        let pad = 1.1 * size;
        let alts = crate::alts::list();
        let active = crate::alts::active();
        let st = crate::alts::status();
        let licence = tr("alts.licence").to_uppercase();
        let lic_w = tw(Face::Caps, &licence, 0.72 * size, 0.12);
        // as wide as the widest row (its face, its name, its mark, its cross) and the buttons
        let names_w = alts.iter().map(|a| tw(Face::Caps, &a.name, size, 0.1) + if a.ms.is_some() { lic_w + 1.0 * size } else { 0.0 }).fold(0.0f32, f32::max);
        let buttons = self.buttons();
        let buttons_w: f32 = buttons.iter().map(|(_, t)| tw(Face::Caps, t, 0.85 * size, 0.12) + 1.6 * size).sum();
        let w = (names_w + 6.0 * size).max(buttons_w + pad).max(22.0 * size).min(m.w - 4.0 * vh);
        let shown = alts.len().min(ROWS);
        let status = self.status_words(m, &st);
        let hint = in_world.then(|| tr("alts.next").to_uppercase());
        let foot = if status.is_some() { 1.6 * size } else { 0.0 } + if hint.is_some() { 1.5 * size } else { 0.0 };
        let hgt = pad + shown as f32 * row + 0.9 * size + row + 0.4 * size + 2.2 * size + foot + 0.6 * pad;
        let x1 = who[2];
        let x0 = (x1 - w).max(2.0 * vh);
        let y0 = who[3] + 0.9 * vh - (1.0 - e) * 1.0 * vh;
        let r = [x0, y0, x0 + w, y0 + hgt];
        self.box_ = r;
        let open = self.open && live;
        if open {
            // a press past it closes it (and is nothing else's)
            m.hit([0.0, 0.0, m.w, m.h], id(K, OUT));
            m.hit(r, id(K, BG));
        }
        let a0 = c.alpha;
        c.alpha = a0 * e;
        c.glass(r, 0.9 * vh, [0.02, 0.02, 0.025, 0.78], 0.75, 0.6, 0.0);
        c.alpha = a0;

        // the accounts
        let list_top = r[1] + pad - 0.3 * size;
        let list = [r[0], list_top, r[2], list_top + shown as f32 * row];
        let max_scroll = (alts.len() as f32 - ROWS as f32).max(0.0) * row;
        self.scroll = self.scroll.clamp(0.0, max_scroll);
        let scroll = m.scroll_ease(id(K, 901), self.scroll);
        m.extra(CLIP, list[3], [list[0], list[1], list[2]], 1.0);
        c.push_clip(list);
        for (i, a) in alts.iter().enumerate() {
            let y = list_top + i as f32 * row - scroll;
            if y + row < list[1] - 1.0 || y > list[3] + 1.0 {
                continue;
            }
            let ir = [r[0] + 0.5 * pad, y, r[2] - 0.5 * pad, y + row];
            let rid = id(K, ROW + i as u32);
            let did = id(K, DEL + i as u32);
            let over = open && m.over(ir) && m.over(list);
            let hot = m.an(rid ^ 0x40, if over { 1.0 } else { 0.0 }, 10.0);
            let sel = a.name == active;
            let q = clamp01((p - i as f32 * 0.03) * 1.25);
            c.rect(ir, 0.5 * size, grey(1.0, (0.07 * hot + if sel { 0.05 } else { 0.0 }) * e));
            let mid = (ir[1] + ir[3]) / 2.0;
            // its face (the game draws it)
            let fs = 1.5 * size;
            m.extra(FACE, i as f32, [ir[0] + 0.7 * size, mid - fs / 2.0, fs], e * q);
            let tx = ir[0] + 0.7 * size + fs + 0.9 * size;
            let bright = if sel { 1.0 } else { 0.56 + 0.34 * hot };
            let end = m.rising_text(c, Face::Caps, &a.name, tx, mid - 0.65 * size, size, 1.3, 0.1, grey(bright, 1.0), 0.0, q);
            if sel {
                c.light(tx, ir[3] - 0.42 * size, end - 0.1 * size, 0.85 * e, 0.0, 1.0, 0.8, ui);
            }
            if a.ms.is_some() {
                m.caps(c, &licence, end + 0.9 * size, mid - 0.5 * size, 0.72 * size, 0.12, 0.42, q);
            }
            if open {
                m.hit(ir, rid);
            }
            // its cross (not the one in use): pressed once it asks, again it takes the account out
            if !sel && (hot > 0.01 || self.armed.as_ref().is_some_and(|(n, _)| *n == a.name)) {
                let armed = self.armed.as_ref().is_some_and(|(n, t)| *n == a.name && m.now - t < 3.0);
                let dr = if armed {
                    let t = tr("alts.delete").to_uppercase();
                    let tw_ = tw(Face::Caps, &t, 0.72 * size, 0.12);
                    let dr = [ir[2] - tw_ - 1.4 * size, mid - 0.8 * size, ir[2] - 0.4 * size, mid + 0.8 * size];
                    m.caps(c, &t, dr[0] + 0.5 * size, mid - 0.5 * size, 0.72 * size, 0.12, BONE, 1.0);
                    dr
                } else {
                    let cc = [ir[2] - 1.3 * size, mid];
                    let dhot = m.an(did ^ 0x40, if open && m.over([cc[0] - 0.8 * size, mid - 0.8 * size, cc[0] + 0.8 * size, mid + 0.8 * size]) { 1.0 } else { 0.0 }, 12.0);
                    let s = 0.28 * size;
                    let col = grey(0.42 + 0.5 * dhot, hot.max(0.3) * e);
                    c.line([cc[0] - s, mid - s], [cc[0] + s, mid + s], 1.3, col, 0.0);
                    c.line([cc[0] + s, mid - s], [cc[0] - s, mid + s], 1.3, col, 0.0);
                    [cc[0] - 0.8 * size, mid - 0.8 * size, cc[0] + 0.8 * size, mid + 0.8 * size]
                };
                if open {
                    m.hit(dr, did);
                }
            }
        }
        c.pop_clip();

        // a hair, then the field for a new nickname
        let fy = list[3] + 0.45 * size;
        c.line([r[0] + pad, fy], [r[2] - pad, fy], 1.0, grey(1.0, 0.08 * e), 0.0);
        let fr = [r[0] + 0.5 * pad, fy + 0.45 * size, r[2] - 0.5 * pad, fy + 0.45 * size + row];
        let fid = id(K, FIELD);
        let focus = self.field.is_some();
        let fhot = m.an(fid ^ 0x40, if open && m.over(fr) || focus { 1.0 } else { 0.0 }, 10.0);
        c.rect(fr, 0.5 * size, grey(1.0, 0.05 * fhot * e));
        let fmid = (fr[1] + fr[3]) / 2.0;
        let qf = clamp01((p - 0.15) * 1.25);
        let px = fr[0] + 0.7 * size;
        let plus_c = [px + 0.75 * size, fmid];
        let pl = 0.4 * size;
        c.line([plus_c[0] - pl, plus_c[1]], [plus_c[0] + pl, plus_c[1]], 1.3, grey(0.56 + 0.4 * fhot, e), 0.0);
        c.line([plus_c[0], plus_c[1] - pl], [plus_c[0], plus_c[1] + pl], 1.3, grey(0.56 + 0.4 * fhot, e), 0.0);
        let tx = px + 1.5 * size + 0.9 * size;
        c.push_clip([tx - 0.2 * size, fr[1], fr[2] - 0.5 * size, fr[3]]);
        match &self.field {
            Some(f) => {
                let end = m.rising_text(c, Face::Caps, &f.text, tx, fmid - 0.65 * size, size, 1.3, 0.1, grey(1.0, 1.0), 0.0, qf);
                let caret_x = tx + tw(Face::Caps, &f.text.chars().take(f.caret).collect::<String>(), size, 0.1) + if f.caret > 0 { 0.1 * size } else { 0.0 };
                if ((m.now - f.moved) * 1.1).fract() < 0.6 {
                    c.line([caret_x, fmid - 0.55 * size], [caret_x, fmid + 0.55 * size], 1.0, grey(1.0, 0.9 * e), 0.0);
                }
                let _ = end;
            }
            None => {
                m.caps(c, &tr("alts.new").to_uppercase(), tx, fmid - 0.65 * size, size, 0.1, 0.36, qf);
            }
        }
        c.pop_clip();
        // the line under it lit while it has the keys
        let lit = m.an(fid ^ 0x41, if focus { 1.0 } else { 0.0 }, 6.0);
        if lit > 0.01 {
            c.light(tx, fr[3] - 0.35 * size, fr[2] - 0.8 * size, lit * e, 0.0, ease(lit), 0.8, ui);
        }
        if open {
            m.hit(fr, fid);
        }

        // the ways to add
        let by = fr[3] + 0.4 * size;
        let mut bx = r[0] + pad;
        for (k, (b, text)) in buttons.iter().enumerate() {
            let bid = id(K, *b);
            let bw = tw(Face::Caps, text, 0.85 * size, 0.12);
            let br = [bx - 0.5 * size, by, bx + bw + 0.5 * size, by + 2.0 * size];
            let bhot = m.an(bid ^ 0x40, if open && m.over(br) { 1.0 } else { 0.0 }, 10.0);
            let q = clamp01((p - 0.2 - k as f32 * 0.05) * 1.25);
            let end = m.caps(c, text, bx, by + 0.45 * size, 0.85 * size, 0.12, 0.6 + 0.4 * bhot, q);
            if bhot > 0.01 {
                c.light(bx, br[3] - 0.25 * size, end, bhot * e, 0.0, bhot, 0.6, ui);
            }
            if open {
                m.hit(br, bid);
            }
            bx += bw + 1.6 * size;
        }
        // what goes on: a sign-in, a word on the nickname typed; and in a world, when a new one counts
        let mut sy = by + 2.4 * size;
        if let Some((words, bright)) = status {
            m.caps(c, &words, r[0] + pad, sy, 0.78 * size, 0.1, bright, e);
            sy += 1.6 * size;
        }
        if let Some(h) = hint {
            m.caps(c, &h, r[0] + pad, sy, 0.72 * size, 0.12, 0.34, e);
        }
    }

    /// The buttons there are now: Microsoft's sign-in, a random nickname, the free Cursed's (when it has some).
    fn buttons(&self) -> Vec<(u32, String)> {
        let mut b = vec![(MS, "MICROSOFT".to_string()), (RANDOM, tr("alts.random").to_uppercase())];
        if self.cursed > 0 {
            b.push((CURSED, trf("alts.cursed", &[&self.cursed.to_string()]).to_uppercase()));
        }
        b
    }

    /// The line under the card: what is wrong with the nickname typed (a few seconds), else what a sign-in does (while
    /// it works, and a while after it is done or went wrong); its words and how bright.
    fn status_words(&self, m: &Menus, st: &crate::alts::Status) -> Option<(String, f32)> {
        if let Some((w, at)) = self.wrong {
            if m.now - at < 4.0 {
                return Some((word(w).to_uppercase(), BONE));
            }
        }
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
            // a working one breathes its dots
            s.push_str(&".".repeat(1 + (m.now * 2.5) as usize % 3));
        }
        Some((s.to_uppercase(), if st.kind == 2 { BONE } else { 0.6 }))
    }

    /// The nickname's box opens it (the page puts it among its own).
    pub fn who(&self, m: &mut Menus, r: [f32; 4], live: bool) {
        if live {
            m.hit(r, WHO);
        }
    }

    /// An event: true when it was the card's (nothing else hears it then) — the nickname pressed, anything while open.
    pub fn event(&mut self, m: &mut Menus, e: &Event) -> bool {
        if let Event::Press(hit, b) = e {
            if *hit == WHO && b & 15 == 0 {
                self.toggle(m);
                return true;
            }
        }
        if !self.open {
            return false;
        }
        match e {
            Event::Press(hit, b) => {
                if *hit >> 32 != K as u64 {
                    // a press outside the card's (as its catcher took it, this is the page's own): it closes
                    self.close(m);
                    return false;
                }
                let n = (*hit & 0xffff_ffff) as u32;
                let alts = crate::alts::list();
                if b & 15 != 0 {
                    return true;
                }
                match n {
                    OUT => self.close(m),
                    FIELD => {
                        if self.field.is_none() {
                            self.field = Some(Field::new(id(K, FIELD), "", 16, m.now));
                            m.tick_sound(0.7);
                        }
                    }
                    MS => {
                        crate::alts::ask_login();
                        m.tick_sound(1.0);
                    }
                    RANDOM => {
                        let name = crate::alts::random();
                        self.field = Some(Field::new(id(K, FIELD), &name, 16, m.now));
                        m.tick_sound(0.8);
                    }
                    CURSED => {
                        let got = crate::alts::take_cursed();
                        crate::alts::set_status(1, "Taken from Cursed", &got.to_string());
                        self.cursed = crate::alts::cursed_new();
                        m.tick_sound(1.0);
                    }
                    _ if (ROW..ROW + 100).contains(&n) => {
                        if let Some(a) = alts.get((n - ROW) as usize) {
                            crate::alts::use_alt(&a.name);
                            self.armed = None;
                            m.tick_sound(1.0);
                        }
                    }
                    _ if (DEL..DEL + 100).contains(&n) => {
                        if let Some(a) = alts.get((n - DEL) as usize) {
                            let again = self.armed.as_ref().is_some_and(|(name, t)| *name == a.name && m.now - t < 3.0);
                            if again {
                                crate::alts::remove(&a.name);
                                self.armed = None;
                            } else {
                                self.armed = Some((a.name.clone(), m.now));
                            }
                            m.tick_sound(0.8);
                        }
                    }
                    _ => {}
                }
                true
            }
            Event::Key(k, mods) => {
                if let Some(f) = self.field.as_mut() {
                    match k.as_str() {
                        "Escape" => self.field = None,
                        "Return" | "Enter" | "Keypad Enter" => {
                            let name = f.text.trim().to_string();
                            match crate::alts::add(&name) {
                                Ok(()) => {
                                    self.field = None;
                                    self.wrong = None;
                                    m.tick_sound(1.0);
                                }
                                Err(w) => self.wrong = Some((w, m.now)),
                            }
                        }
                        _ => {
                            f.key(k, *mods, m.now);
                        }
                    }
                    return true;
                }
                if k == "Escape" {
                    self.close(m);
                    return true;
                }
                false
            }
            Event::Char(ch) => {
                if let Some(f) = self.field.as_mut() {
                    f.insert(*ch, m.now);
                    return true;
                }
                false
            }
            Event::Scroll(dy) if m.over(self.box_) => {
                self.scroll -= dy * 2.6 * m.cl(9.0, 18.0, 1.15);
                true
            }
            _ => false,
        }
    }

    fn close(&mut self, m: &Menus) {
        if self.open {
            self.open = false;
            self.field = None;
            self.armed = None;
            m.tick_sound(0.7);
        }
    }
}

/// A word of the card's in the game's language: the menus' own by its key (the status's English words are their own
/// keys), else as it is.
fn word(key: &str) -> &str {
    if lang::known(key) {
        tr(key)
    } else {
        key
    }
}
