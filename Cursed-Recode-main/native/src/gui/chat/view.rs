//! The chat drawn. Closed: one glass panel behind its newest lines over the world — they rise in, fade after a while,
//! the panel shrinks and fades with them. Open: a window of its own grown out of it — the channels and the private
//! conversations on the left, the messages on the right (a conversation as bubbles), the field at its foot with the
//! game's command hints over it, a search, its settings, a player's card by the name. In the client's style: the
//! Theme's glass and words, Geist, monochrome (a server's own colours are its words', lifted to be read), no fire.
//!
//! Everything is drawn from the element's corner (hud.rs M) and put in its place after; what can be pressed is kept in
//! the element's own px (store.rs Store::hits), the pointer shifted back by where the element went.

use super::super::*;
use super::body::{self, gutter, Ak, Atom, Body, Icon, Laid, Look, Mark, Placed};
use super::parse::{Kind as Said, Server, Tone};
use super::store::{store, Hit, Store, Tab, Tap, F_OBF, F_STRIKE, F_UNDER};
use std::cell::RefCell;

const A_CH: u64 = 48;

thread_local! {
    /// The server's hover words under the pointer this frame, where.
    static TIP: RefCell<Option<(String, [f32; 2])>> = const { RefCell::new(None) };
    /// What is open over the window this frame (the hints, the settings, the card, a tip, a toast), the chat's own px:
    /// the icons Java draws under it are cut around it (Gui::chat_cut_icons).
    static OVER: RefCell<Vec<[f32; 4]>> = const { RefCell::new(Vec::new()) };
    /// The HUD's icons this frame: where the chat's begin, where those over its window begin, where the chat's end.
    static ICONS_AT: std::cell::Cell<[usize; 3]> = const { std::cell::Cell::new([usize::MAX; 3]) };
}

/// Something open over the window, as opaque as it is now (fading in or out: under half, what is under shows).
fn over(r: [f32; 4], a: f32) {
    if a > 0.5 {
        OVER.with(|o| o.borrow_mut().push(r));
    }
}

/// `r` less the rectangles `covers`: None when none of them touches it, else the parts left (none: all covered).
fn cut_rect(r: [f32; 4], covers: &[[f32; 4]]) -> Option<Vec<[f32; 4]>> {
    let touches = |p: &[f32; 4], c: &[f32; 4]| p[0] < c[2] && p[2] > c[0] && p[1] < c[3] && p[3] > c[1];
    if !covers.iter().any(|c| touches(&r, c)) {
        return None;
    }
    let mut parts = vec![r];
    for c in covers {
        let mut next = Vec::new();
        for p in parts {
            if !touches(&p, c) {
                next.push(p);
                continue;
            }
            // above it, below it, then beside it (between those)
            let (y0, y1) = (p[1].max(c[1]), p[3].min(c[3]));
            for q in [[p[0], p[1], p[2], c[1]], [p[0], c[3], p[2], p[3]], [p[0], y0, c[0], y1], [c[2], y0, p[2], y1]] {
                if q[2] - q[0] > 0.5 && q[3] - q[1] > 0.5 {
                    next.push(q);
                }
            }
        }
        parts = next;
    }
    Some(parts)
}

/// Its icons (HudLayer): 21 a pack's glyphs, 22 an item a message shows, 23 a face by a name (the chat's faces this
/// frame; 26 its corners rounded, 27 round), 24 an item of a card.
const IC_GLYPH: f32 = 21.0;
const IC_ITEM: f32 = 22.0;
const IC_FACE: f32 = 23.0;
const IC_CARD_ITEM: f32 = 24.0;

/// The open window's measures (the chat's own px, ×k).
struct Geo {
    k: f32,
    /// its whole, and the left column's width
    w: f32,
    h: f32,
    left: f32,
    head: f32,
    field: f32,
    /// the left column: 0 whole, 1 its icons only, 2 none (the header's title lists the channels)
    col: usize,
    /// the field under the header (the hints open under it)
    field_top: bool,
}

fn mixr(a: [f32; 4], b: [f32; 4], k: f32) -> [f32; 4] {
    [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k, a[3] + (b[3] - a[3]) * k]
}

fn clock_text(secs: i32, seconds: bool, h24: bool) -> String {
    let s = secs.rem_euclid(86400);
    let (h, m, sc) = (s / 3600, s / 60 % 60, s % 60);
    let h = if h24 { h } else if h % 12 == 0 { 12 } else { h % 12 };
    if seconds {
        format!("{h:02}:{m:02}:{sc:02}")
    } else {
        format!("{h:02}:{m:02}")
    }
}

/// How a row is drawn this time: closed (over the world: no hover, its words shadowed when there is no plate) or
/// open (hover, actions, time on hover).
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Closed,
    Open,
}

impl Gui {
    /// A face (Java's, by the name; only a player in the game has one — nothing is drawn for the others).
    fn chat_face(&mut self, st: &mut Store, name: &str, r: [f32; 4], a: f32) {
        let i = match st.faces.iter().position(|f| f.eq_ignore_ascii_case(name)) {
            Some(i) => i,
            None => {
                st.faces.push(name.to_string());
                st.faces.len() - 1
            }
        };
        // (square, its corners rounded, or round: as set)
        let kind = [IC_FACE, 26.0, 27.0][st.face_shape.min(2)];
        self.hud.icons.push([kind, i as f32, r[0], r[1], r[2] - r[0], r[3] - r[1], a]);
    }

    /// A row's atoms drawn from (x, y) (its first line's top), their hits.
    #[allow(clippy::too_many_arguments)]
    fn chat_atoms(&mut self, c: &mut Canvas, atoms: &[Atom], placed: &[Placed], x: f32, y: f32, lh: f32, look: &Look, a: f32, mouse: [f32; 2], hits: &mut Vec<(Hit, [f32; 4])>, t: f32, live: bool) {
        let k = look.k;
        for p in placed {
            let at = &atoms[p.ai];
            let base = y + p.line as f32 * lh + lh * 0.72;
            let px = x + p.x;
            let box_ = [px, base - lh * 0.66, px + p.w, base + lh * 0.2];
            let hov = live && inside(box_, mouse);
            if hov && a > 0.5 {
                if let Some(hv) = &at.hover {
                    TIP.with(|tip| *tip.borrow_mut() = Some((hv.clone(), [px, box_[1]])));
                }
            }
            match at.ak {
                Ak::Glyph(key, w) => {
                    self.hud.icons.push([IC_GLYPH, key, px, base - look.size * 0.78, w, look.size * 0.9, a]);
                }
                Ak::Words => {
                    if at.found {
                        c.rect([px - 1.0 * k, base - at.size * 0.82, px + p.w + 1.0 * k, base + at.size * 0.22], 3.0 * k, fade(look.base, 0.22));
                    }
                    let txt: std::borrow::Cow<str> = if at.fl & F_OBF != 0 {
                        let n = (t * 20.0) as u32;
                        std::borrow::Cow::Owned(p.text.chars().enumerate().map(|(i, ch)| if ch == ' ' { ' ' } else { ['#', '%', '&', '?', '@', '$'][(i as u32 + n) as usize % 6] }).collect())
                    } else {
                        std::borrow::Cow::Borrowed(p.text.as_str())
                    };
                    let w = c.text(px, base, at.size, at.face, &txt, at.col);
                    if at.fl & F_UNDER != 0 || (at.tap.is_some() && hov) {
                        c.rect([px, base + 2.0 * k, px + w, base + 3.0 * k], 0.5, fade(at.col, 0.7));
                    }
                    if at.fl & F_STRIKE != 0 {
                        c.rect([px, base - at.size * 0.3, px + w, base - at.size * 0.3 + 1.2 * k], 0.5, fade(at.col, 0.8));
                    }
                    if let (Some(tap), true) = (&at.tap, live) {
                        hits.push((Hit::Tap(tap.clone()), box_));
                    }
                }
                Ak::Nick => {
                    let w = c.text(px, base, at.size, at.face, &p.text, at.col);
                    if hov {
                        c.rect([px, base + 2.5 * k, px + w, base + 3.5 * k], 0.5, fade(at.col, 0.4));
                    }
                    if live {
                        hits.push((Hit::Nick(at.text.clone()), box_));
                    }
                }
                Ak::Me => {
                    let r = [px, base - at.size * 0.86, px + p.w, base + at.size * 0.28];
                    c.rect(r, 5.0 * k, fade(look.ment, 0.2));
                    c.text(px + 4.0 * k, base, at.size, Face::Semi, &p.text, mix(look.base, look.ment, 0.35));
                }
                Ak::Chip(_) | Ak::Tag | Ak::Item(_) => {
                    let (pl, _) = body::chip_pad(at, k);
                    let r = [px, base - at.size * 1.12, px + p.w, base + at.size * 0.42];
                    let tapable = at.tap.is_some() && live;
                    c.rect(r, 6.0 * k, grey(1.0, if tapable && hov { 0.14 } else { 0.075 }));
                    let mid = (r[1] + r[3]) / 2.0;
                    let ic = |kind: u32, c: &mut Canvas| {
                        let s = 13.0 * k;
                        c.icon([px + 4.0 * k, mid - s / 2.0, px + 4.0 * k + s, mid + s / 2.0], kind, fade(at.col, 0.85));
                    };
                    match at.ak {
                        Ak::Chip(1) => ic(40, c),
                        Ak::Chip(2) => ic(41, c),
                        Ak::Item(key) => {
                            let s = 17.0 * k;
                            self.hud.icons.push([IC_ITEM, key, px + 4.0 * k, mid - s / 2.0, s, s, a]);
                        }
                        _ => {}
                    }
                    c.text(px + pl, base - at.size * 0.05, at.size, at.face, &p.text, if at.ak == Ak::Tag { look.dim } else { at.col });
                    if let (Some(tap), true) = (&at.tap, live) {
                        hits.push((Hit::Tap(tap.clone()), r));
                    }
                }
            }
        }
    }

    /// A row drawn in `row` (its whole box): its face or mark, its words, ×N, the time, a card's parts. Returns
    /// whether the pointer is over it.
    #[allow(clippy::too_many_arguments)]
    fn chat_row(&mut self, c: &mut Canvas, st: &mut Store, i: usize, l: &Laid, row: [f32; 4], look: &Look, a: f32, mouse: [f32; 2], hits: &mut Vec<(Hit, [f32; 4])>, mode: Mode, set: &RowSet) -> bool {
        let k = look.k;
        let t = st.now;
        let live = mode == Mode::Open;
        let ytop = row[1];
        let hov = live && inside(row, mouse);
        let r_id = st.rows[i].id;
        let hk = self.an(ak(A_CH, 1000 + (r_id.rem_euclid(4096)) as usize), if hov { 1.0 } else { 0.0 }, 16.0, 0.0);
        let base_a = c.alpha;
        c.alpha = base_a * a;
        let (mention, count, bump, clock, secs, until, who) = {
            let r = &st.rows[i];
            (r.mention, r.count, r.bump, r.clock, r.secs, r.until, r.who.clone())
        };
        let card = matches!(l.body, Body::Card { .. });
        // the row's own hit first: what is in it (a name, a chip, its actions) is over it
        if live {
            hits.push((Hit::Row(r_id), row));
        }
        // (lit under the pointer, or while its menu is open)
        let menu_k = if st.menu.as_ref().is_some_and(|m| m.0 == r_id) { 1.0 } else { 0.0 };
        if hk.max(menu_k) > 0.01 && !card {
            c.rect(row, 8.0 * k, grey(1.0, 0.045 * hk.max(menu_k)));
        }
        if mention && set.mentions && !card {
            c.rect(row, 8.0 * k, fade(look.ment, 0.08));
            c.rect([row[0], row[1] + 4.0 * k, row[0] + 2.0 * k, row[3] - 4.0 * k], 1.0 * k, fade(look.ment, 0.85));
        }
        if bump > 0.0 && t - bump < 0.6 {
            let kb = 1.0 - (t - bump) / 0.6;
            c.rect(row, 8.0 * k, grey(1.0, 0.1 * kb * kb));
        }
        let tx0 = row[0] + 8.0 * k;
        let gut = gutter(look);
        match &l.body {
            Body::Line { face, atoms } => {
                let y = ytop + if l.cont { 1.0 * k } else { 4.0 * k };
                if let (true, Some(name)) = (look.heads, face.as_ref().filter(|n| st.has_face(n))) {
                    let fs = look.head;
                    self.chat_face(st, name, [tx0, y + (look.lh - fs) / 2.0, tx0 + fs, y + (look.lh + fs) / 2.0], c.alpha);
                }
                self.chat_atoms(c, atoms, &l.a, tx0 + gut, y, look.lh, look, c.alpha, mouse, hits, t, live);
            }
            Body::Sys { mark, atoms } => {
                let mid = ytop + 4.0 * k + look.lh * 0.48;
                let mx = tx0 + if look.heads { 9.0 * k } else { 5.0 * k };
                match mark {
                    Mark::Dot(tone) => {
                        let (col, ring) = match tone {
                            Tone::Info => (fade(look.dim, 0.55), false),
                            Tone::Ok => (look.base, false),
                            Tone::Bad => (look.base, true),
                        };
                        if ring {
                            c.ring([mx, mid], 3.0 * k, 1.2 * k, col);
                        } else {
                            c.rect([mx - 2.6 * k, mid - 2.6 * k, mx + 2.6 * k, mid + 2.6 * k], 2.6 * k, col);
                        }
                    }
                    Mark::Icon(kind) => {
                        let s = 12.0 * k;
                        c.icon([mx - s / 2.0, mid - s / 2.0, mx + s / 2.0, mid + s / 2.0], *kind, fade(look.dim, 0.9));
                    }
                }
                self.chat_atoms(c, atoms, &l.a, tx0 + gut, ytop + 4.0 * k, look.lh * 0.96, look, c.alpha, mouse, hits, t, live);
            }
            Body::Card { icon, title, sub, btns, fuse, state, done } => {
                let cr = [row[0], ytop + 3.0 * k, row[2], row[3] - 3.0 * k];
                c.rect(cr, 12.0 * k, grey(1.0, 0.055 + 0.03 * hk));
                let ca = if *done { 0.45 } else { 1.0 };
                c.alpha = base_a * a * ca;
                let body_h = cr[3] - cr[1] - if *fuse { 10.0 * k } else { 0.0 };
                let mid = cr[1] + body_h / 2.0;
                let ic = [cr[0] + 12.0 * k, mid - 19.0 * k, cr[0] + 50.0 * k, mid + 19.0 * k];
                match icon {
                    Icon::Face(name) if st.has_face(name) => self.chat_face(st, name, ic, c.alpha),
                    Icon::Face(_) => {
                        c.rect(ic, 9.0 * k, grey(1.0, 0.05));
                        let s = 20.0 * k;
                        let m = [(ic[0] + ic[2]) / 2.0, (ic[1] + ic[3]) / 2.0];
                        c.icon([m[0] - s / 2.0, m[1] - s / 2.0, m[0] + s / 2.0, m[1] + s / 2.0], 34, look.dim);
                    }
                    Icon::Item(n) => {
                        c.rect(ic, 9.0 * k, grey(1.0, 0.05));
                        let s = 28.0 * k;
                        let cx = (ic[0] + ic[2]) / 2.0;
                        self.hud.icons.push([IC_CARD_ITEM, *n as f32, cx - s / 2.0, mid - s / 2.0, s, s, c.alpha]);
                    }
                }
                let text_h = l.a_lines as f32 * look.lh + l.b_lines as f32 * look.lh * 0.88;
                let ty = mid - text_h / 2.0;
                let tx = ic[2] + 12.0 * k;
                self.chat_atoms(c, title, &l.a, tx, ty, look.lh, look, c.alpha, mouse, hits, t, live);
                if !l.b.is_empty() {
                    self.chat_atoms(c, sub, &l.b, tx, ty + l.a_lines as f32 * look.lh, look.lh * 0.88, look, c.alpha, mouse, hits, t, live);
                }
                // the buttons (right), or what became of it
                let mut bx = cr[2] - 12.0 * k;
                if let Some(sv) = state {
                    c.text_r(bx, mid + look.size * 0.3, look.size * 0.76, Face::Mono, sv, fade(look.dim, 0.9));
                }
                for b in btns.iter().rev() {
                    let w = body::btn_w(b, look);
                    let r2 = [bx - w, mid - 14.0 * k, bx, mid + 14.0 * k];
                    let bh = live && inside(r2, mouse);
                    let bv = self.an(ak(A_CH, 9000 + (r_id.rem_euclid(512) as usize) * 2 + b.which as usize), if bh { 1.0 } else { 0.0 }, 16.0, 0.0);
                    let (bg, fg) = match b.style {
                        1 => (mix(fade(look.base, 0.9), look.base, bv), grey(0.04, 1.0)),
                        _ => (grey(1.0, 0.07 + 0.08 * bv), mix(look.dim, look.base, bv)),
                    };
                    c.rect(r2, 8.0 * k, bg);
                    c.text_c((r2[0] + r2[2]) / 2.0, mid + look.size * 0.3, look.size * 0.86, if b.style == 1 { Face::Semi } else { Face::Medium }, &b.label, fg);
                    if live {
                        hits.push((Hit::Btn(r_id, b.which), r2));
                    }
                    bx -= w + 6.0 * k;
                }
                // the time it has: a quiet white line going down to nothing
                if *fuse && secs > 0.0 {
                    let left = ((until - t) / secs).clamp(0.0, 1.0);
                    let fy = cr[3] - 8.0 * k;
                    let fr = [cr[0] + 12.0 * k, fy, cr[2] - 12.0 * k, fy + 2.0 * k];
                    c.rect(fr, 1.0 * k, grey(1.0, 0.08));
                    c.rect([fr[0], fr[1], fr[0] + (fr[2] - fr[0]) * left, fr[3]], 1.0 * k, fade(look.base, 0.75));
                }
                c.alpha = base_a * a;
            }
        }
        // ×N after the words
        if count > 1 && !card {
            let lab = format!("×{count}");
            let fs = look.size * 0.72;
            let w = fonts().width(Face::Mono, fs, &lab) + 10.0 * k;
            let ex = row[0] + 8.0 * k + l.end[0];
            let bx = (ex + 6.0 * k).min(row[2] - w - 4.0 * k);
            let base = ytop + l.end[1] + look.lh * 0.72;
            let kb = if bump > 0.0 { 1.0 - ((t - bump) / 0.4).clamp(0.0, 1.0) } else { 0.0 };
            let sc = 1.0 + 0.3 * kb * kb;
            let (cx, cy) = (bx + w / 2.0, base - fs * 0.35);
            let (hw, hh) = (w / 2.0 * sc, fs * 0.7 * sc);
            c.rect([cx - hw, cy - hh, cx + hw, cy + hh], hh, grey(1.0, 0.08 + 0.2 * kb));
            c.text_c(cx, base - fs * 0.02, fs * sc, Face::Mono, &lab, look.dim);
        }
        // the time, always: at the row's right (its words were laid short of it)
        if set.time == 0 && !card && clock >= 0 && !(live && hk > 0.5) {
            let fs = look.size * 0.7;
            c.text_r(row[2] - 8.0 * k, ytop + 4.0 * k + look.lh * 0.7, fs, Face::Mono, &clock_text(clock, set.seconds, set.h24), fade(look.dim, 0.7 * (1.0 - hk)));
        }
        // under the pointer (open): a small plate over the row's right end — the time, then reply, mention, copy,
        // hide (not to yourself)
        if hk > 0.02 && live && !card {
            let mine = st.rows[i].mine;
            let acts: Vec<(u8, u32)> = if who.is_empty() { vec![(2, 32)] } else if mine { vec![(1, 31), (2, 32)] } else { vec![(0, 30), (1, 31), (2, 32), (3, 33)] };
            let bs = 24.0 * k;
            let time = (set.time != 2 && clock >= 0).then(|| clock_text(clock, set.seconds, set.h24));
            let tw = time.as_ref().map_or(0.0, |s| fonts().width(Face::Mono, look.size * 0.68, s) + 14.0 * k);
            let pw = tw + acts.len() as f32 * (bs + 2.0 * k) + 4.0 * k;
            let px1 = row[2] - 4.0 * k + (1.0 - hk) * 6.0 * k;
            let pr = [px1 - pw, ytop + 1.0 * k, px1, ytop + 1.0 * k + bs + 4.0 * k];
            c.alpha = base_a * a * hk;
            let gl = crate::gui::theme::pal().glass;
            c.shadow(pr, 9.0 * k, 10.0 * k, grey(0.0, 0.35));
            c.rect(pr, 9.0 * k, [gl[0].max(0.075), gl[1].max(0.075), gl[2].max(0.085), 0.97]);
            if let Some(ts) = &time {
                c.text(pr[0] + 9.0 * k, (pr[1] + pr[3]) / 2.0 + 3.8 * k, look.size * 0.68, Face::Mono, ts, fade(look.dim, 0.9));
            }
            let mut bx = pr[2] - 2.0 * k;
            for (ki, icon) in acts.iter().rev() {
                let r2 = [bx - bs, pr[1] + 2.0 * k, bx, pr[1] + 2.0 * k + bs];
                let on = inside(r2, mouse);
                if on {
                    c.rect(r2, 7.0 * k, grey(1.0, 0.12));
                }
                let s = 14.0 * k;
                let cx = (r2[0] + r2[2]) / 2.0;
                let cy = (r2[1] + r2[3]) / 2.0;
                c.icon([cx - s / 2.0, cy - s / 2.0, cx + s / 2.0, cy + s / 2.0], *icon, fade(look.base, if on { 1.0 } else { 0.7 }));
                hits.push((Hit::Act(*ki, r_id), r2));
                bx -= bs + 2.0 * k;
            }
        }
        c.alpha = base_a;
        hov
    }
}

/// The element's own settings, read once a frame.
struct RowSet {
    time: usize,
    seconds: bool,
    h24: bool,
    mentions: bool,
    /// the room kept at a row's right for its time (shown always)
    room: f32,
    /// a divider over a message that came after so long a quiet (s; none: no dividers)
    pause: Option<f32>,
    /// the conversation's bubbles: all on the left, their corners, yours and theirs
    bub_left: bool,
    bub_r: f32,
    bub_mine: Rgba,
    bub_theirs: Rgba,
}

/// The box of the command hints (or a command's usage) over the field `r`, when there are any.
fn hints_box(st: &Store, k: f32, r: [f32; 4]) -> Option<[f32; 4]> {
    let n = if !st.sugg.is_empty() { st.sugg.len() } else { st.usage.len() };
    if n == 0 || st.find {
        return None;
    }
    let hh = n.min(8) as f32 * 32.0 * k + 10.0 * k;
    if st.hints_down {
        return Some([r[0], r[3] + 8.0 * k, r[0] + (r[2] - r[0]).min(520.0 * k), r[3] + 8.0 * k + hh]);
    }
    Some([r[0], r[1] - 8.0 * k - hh, r[0] + (r[2] - r[0]).min(520.0 * k), r[1] - 8.0 * k])
}

/// Whether row `j` goes on from row `i` (the same player's words a moment later): no face, no name again.
fn goes_on(st: &Store, i: usize, j: usize) -> bool {
    let (a, b) = (&st.rows[i], &st.rows[j]);
    matches!(a.kind, Said::Chat(_))
        && matches!(b.kind, Said::Chat(_))
        && !a.who.is_empty()
        && a.who.eq_ignore_ascii_case(&b.who)
        && (b.clock - a.clock).abs() <= 120
        && a.count == 1
        // (a divider between them: the name again under it)
        && st.pause.is_none_or(|p| b.born - a.born <= p)
}

/// Where the field writes: a conversation (its name, lowercase) or "" (the chat).
fn field_key(st: &Store) -> String {
    match (&st.tab, &st.dm) {
        (Tab::Private, Some(d)) => d.to_lowercase(),
        _ => String::new(),
    }
}

/// The channel or the conversation changed (`was` where the field wrote before): what was typed stays where it was
/// typed (a draft each, never sent anywhere else), the field holds the draft of where it writes now. While searching,
/// the search is closed first (pushed before this): what it kept aside is the draft.
fn swap_draft(st: &mut Store, was: String, out: &mut Vec<String>) {
    let now = field_key(st);
    if now == was {
        return;
    }
    let typed = if st.find { st.stash.clone() } else { st.field.clone() };
    if typed.trim().is_empty() {
        st.drafts.remove(&was);
    } else {
        st.drafts.insert(was, typed);
    }
    let next = st.drafts.remove(&now).unwrap_or_default();
    out.push(format!("chat\tset\t{next}"));
    // (as Java will say: a second press this frame reads it right)
    st.field = next;
}

impl Gui {
    /// The chat drawn at the element's corner; its size.
    pub(in super::super) fn el_chat(&mut self, c: &mut Canvas, x: &Ctx) -> Option<[f32; 2]> {
        let k = x.s;
        let t = x.t;
        let editing = self.hud.editing;
        // the window's corner or a channel held: they follow the pointer (as Java last said where it is)
        let (held, m_now) = {
            let s = store();
            (s.grip.is_some() || s.tab_drag.is_some(), s.mouse)
        };
        if held {
            self.chat_drag(m_now[0], m_now[1]);
        }
        OVER.with(|o| o.borrow_mut().clear());
        ICONS_AT.with(|a| a.set([self.hud.icons.len(), usize::MAX, usize::MAX]));
        let mut st = store();
        st.now = t;
        st.max = x.num("History").max(100.0) as usize;
        st.fold = x.on("Stack repeats");
        st.load_hidden(self.dir.as_deref());
        // the channels shown, the player's own (their name, their words)
        let own: Vec<(String, Vec<String>)> = (1..=3)
            .map(|i| {
                let name = theme::text(&x.set, &format!("Own channel {i}")).trim().to_string();
                let words = theme::text(&x.set, &format!("Channel {i} words")).split(',').map(|w| w.trim().to_lowercase()).filter(|w| !w.is_empty()).collect();
                (name, words)
            })
            .collect();
        if let Some(was) = st.set_channels([x.on("Players channel"), x.on("Private channel"), x.on("Server channel"), x.on("Mentions channel")], own) {
            swap_draft(&mut st, was, &mut self.outbox);
        }
        if st.pump() && x.on("Mention sound") {
            crate::launch::sound::play(crate::launch::sound::Sfx::Shimmer, 0.25 * self.volume(), 0.0);
        }
        st.faces.clear();
        st.face_shape = x.pick("Heads shape");
        // (the letters' size as set: the lines as tall as they are)
        let tk = text::scale();
        let spacing = (x.num("Line spacing") / 100.0).clamp(0.6, 2.0);
        let look = Look {
            k,
            size: 15.0 * k,
            lh: 21.0 * k * tk * spacing,
            gu: 15.0 * k * 0.1 * tk,
            base: x.el.text,
            dim: x.el.text2,
            accent: x.el.accent,
            ment: x.col("Mentions colour", x.el.accent),
            me: st.me.clone(),
            q: if st.find { st.field.trim().to_lowercase() } else { String::new() },
            heads: x.on("Player heads"),
            head: x.num("Heads size").clamp(12.0, 30.0) * k,
            indent: x.num("Wrapped lines indent") * k,
            tones: x.pick("Server colours") as u8,
            names: x.pick("Player names") as u8,
            ranks: x.on("Rank prefixes"),
        };
        let look_key = look.key();
        let (time, seconds) = (x.pick("Time"), x.on("Seconds"));
        let room = if time == 0 { fonts().width(Face::Mono, look.size * 0.7, if seconds { "00:00:00" } else { "00:00" }) + 14.0 * k } else { 0.0 };
        st.pause = x.on("Pause dividers").then(|| x.num("Pause").max(1.0) * 60.0);
        let set = RowSet {
            time,
            seconds,
            h24: x.on("24-hour clock"),
            mentions: x.on("Highlight mentions"),
            room,
            pause: st.pause,
            bub_left: x.pick("Bubbles") == 1,
            bub_r: 20.0 * k * x.num("Bubble corners") / 100.0,
            bub_mine: x.col("Your bubbles", x.el.text),
            bub_theirs: x.col("Their bubbles", grey(1.0, 0.075)),
        };
        let ads = x.on("Server ads");
        let cards_on = x.on("Event cards");
        let open = st.open && !editing;
        let speed = (x.num("Opening speed") / 100.0).clamp(0.25, 3.0);
        let ok = self.an(ak(A_CH, 0), if open { 1.0 } else { 0.0 }, 11.0 * speed, 0.0);
        let e = ease_out(ok);
        // how it opens: grown out of the closed panel, or the window itself sliding in, fading in, out of a point
        let opens = x.pick("Opens");
        let grow = opens == 0;
        // the world behind it (drawn before the chat by the HUD: Gui::chat_behind)
        st.behind = (x.pick("Behind it"), x.num("Behind strength") / 100.0 * e, t);
        let (sw, sh) = (self.hud.w, self.hud.h);
        // the window's size: as set, kept on the screen
        let g = Geo {
            k,
            w: (x.num("Window width") * k).min(sw - 24.0).max(420.0 * k.min(1.0)),
            h: (x.num("Window height") * k).min(sh - 24.0).max(260.0 * k.min(1.0)),
            left: match x.pick("Left column") {
                1 => 58.0 * k,
                2 => 0.0,
                _ => x.num("Left column width").clamp(160.0, 360.0) * k,
            },
            head: 58.0 * k,
            field: 62.0 * k,
            col: x.pick("Left column"),
            field_top: x.pick("Field at") == 1,
        };
        let cw = (x.num("Width") * k).max(200.0 * k);
        let pad = 9.0 * k * x.look.pad * x.el.pad;
        let gap_px = x.num("Message gap") * k;
        let rise_kind = x.pick("New lines");
        let a0 = c.alpha;
        let mouse = [st.mouse[0] - st.shift[0], st.mouse[1] - st.shift[1]];
        let mut hits: Vec<(Hit, [f32; 4])> = Vec::new();
        TIP.with(|tip| *tip.borrow_mut() = None);

        // ---- closed: the newest rows as long as they are fresh, at most so many lines. A row comes in where it stays:
        // its room opens by the others as they move aside (the panel's edge with them) and it shows in it rising a
        // little; past the most lines the oldest gives its room up as the newest opens its own. No line is cut by the
        // panel's edge: one going or not yet in is seen over it, faint
        let fade_after = x.num("Fade after").max(2.0);
        let fades = x.on("Fade out");
        let together = x.pick("Fading") == 1;
        let newest_top = x.pick("Newest") == 1;
        let fit = x.pick("Panel size") == 1;
        let panel_op = (x.num("Panel opacity") / 100.0).clamp(0.0, 1.0);
        let max_lines = x.num("Lines shown").max(2.0);
        let feed_w_closed = cw - pad * 2.0;
        // (the row, how much it is seen, how much of its room it has, how far it is from where it stays)
        let mut closed: Vec<(usize, f32, f32, f32)> = Vec::new();
        let mut closed_h = 0.0f32;
        // the widest of the rows shown (the panel the size of its lines)
        let mut widest = 0.0f32;
        if ok < 0.999 && !st.rows.is_empty() {
            let n = st.rows.len();
            // the newer rows' lines (all in), and the room they have yet to open: the rows past the most keep it till
            // then
            let mut full = 0.0f32;
            let mut owed = 0.0f32;
            // (all fading together: as the newest does)
            let mut lead: Option<f32> = None;
            let mut mi = n;
            while mi > 0 {
                mi -= 1;
                if !st.shows(&st.rows[mi], Tab::All, ads) || (!cards_on && matches!(st.rows[mi].kind, Said::Event { .. } | Said::Airdrop { .. })) {
                    continue;
                }
                let r = &st.rows[mi];
                let own_age = t - r.born;
                let age = if together { *lead.get_or_insert(own_age) } else { own_age };
                let waiting = r.until > t && r.state == 0;
                let alive = editing || !fades || age < fade_after + 0.6 || waiting;
                if !alive {
                    break;
                }
                // each its own here, its face and name with it: the rows go one by one, a line going on from one
                // gone would stand by an empty place
                let l = st.laid(mi, feed_w_closed - set.room, &look, look_key, false).clone();
                let a = if editing || !fades || waiting { 1.0 } else { 1.0 - smooth(fade_after, fade_after + 0.6, age) };
                // coming in: its room opening (easing out), the row showing in it a moment later, rising a little or
                // only fading in; at once: there
                let (open, seen) = if rise_kind == 2 || editing {
                    (1.0, 1.0)
                } else {
                    (1.0 - (1.0 - (own_age / 0.3).clamp(0.0, 1.0)).powi(3), smooth(0.08, 0.34, own_age))
                };
                let lift = if rise_kind == 0 { (1.0 - seen) * 6.0 * k } else { 0.0 };
                // past the most lines (its lines of words: a card's two at least): the room the newer rows have not
                // opened yet is still its
                let nl = match &l.body {
                    Body::Card { .. } => (l.a_lines + l.b_lines).max(2),
                    _ => l.a_lines.max(1),
                } as f32;
                let fit = if closed.is_empty() || full + nl <= max_lines + 0.01 {
                    full += nl;
                    owed += nl * (1.0 - open);
                    1.0
                } else {
                    let f = (owed / nl).clamp(0.0, 1.0);
                    owed -= nl * f;
                    f
                };
                if fit <= 0.001 {
                    break;
                }
                widest = widest.max(match &l.body {
                    Body::Card { .. } => feed_w_closed,
                    _ => gutter(&look) + l.a.iter().map(|p| p.x + p.w).fold(0.0, f32::max) + 16.0 * k,
                });
                let room = open * fit * a.max(0.0).powf(0.35);
                closed_h += (l.h + gap_px) * room;
                closed.push((mi, a * seen * fit * fit * fit, room, lift));
            }
        }
        let panel_a = closed.iter().map(|q| q.1).fold(0.0f32, f32::max);
        // the panel as tall as its rows' rooms: its edge moves with them
        let ch = if closed.is_empty() { 0.0 } else { closed_h + pad * 2.0 };
        let panel_on = x.on("Panel");
        // the closed panel's width: as set, or the size of its lines (easing as they come and go)
        let cw_to = if fit && !closed.is_empty() && !editing { (widest + set.room + pad * 2.0).clamp(120.0 * k, cw) } else { cw };
        let cw = self.an(ak(A_CH, 9), cw_to, 12.0, cw_to);

        // ---- the element's size: from the panel to the window (grown), or the window's while it shows
        let (w, h) = if grow {
            (cw + (g.w - cw) * e, ch + (g.h - ch) * e)
        } else if ok > 0.0005 {
            (g.w.max(cw), g.h.max(ch))
        } else {
            (cw, ch)
        };
        let frame = [M, M, M + w, M + h];
        // the closed panel's foot: the element's, or (held by its top, the window coming on its own) its top's
        let anchor = self.hud.places.get("chat").map_or(6, |p| p.0);
        let cb = if !grow && anchor / 3 == 0 { M + ch } else { M + h };
        // the glass: the panel's (as seen as its newest row, as opaque as set), the window's
        let ga = if editing { 1.0 } else { panel_a.max(e) };
        if grow && (panel_on || e > 0.01) && ga > 0.004 && h > 1.0 {
            c.alpha = a0 * if panel_on { ga * (panel_op + (1.0 - panel_op) * e) } else { e };
            x.part(c, frame);
            c.alpha = a0;
        } else if !grow && panel_on && panel_a > 0.004 && ch > 1.0 && ok < 0.999 {
            c.alpha = a0 * panel_a * panel_op * (1.0 - ok);
            x.part(c, [M, cb - ch, M + cw, cb]);
            c.alpha = a0;
        }

        // ---- the closed rows, fading out as the window comes: the newest at the foot, or at the top. Each stands by the
        // side of its room toward the newer rows (what of it is not its room yet, or no more, is over the others' or
        // past the edge, faint); cut only at the sides
        if ok < 0.999 && !closed.is_empty() {
            let ca = 1.0 - smooth(0.0, 0.5, ok);
            text::set_style(x.el.family, x.el.bolder, if panel_on { x.el.tshadow } else { x.el.tshadow.max(0.85) });
            c.push_clip([M, -1.0e4, M + w, 1.0e4]);
            let mut yb = cb - pad;
            let mut yt = cb - ch + pad;
            for &(mi, a, room, lift) in &closed {
                let l = st.laid(mi, feed_w_closed - set.room, &look, look_key, false).clone();
                let step = (l.h + gap_px) * room;
                let ytop = if newest_top {
                    let y = yt - lift;
                    yt += step;
                    y
                } else {
                    let y = yb - l.h + lift;
                    yb -= step;
                    y
                };
                if newest_top && ytop > cb + look.lh || !newest_top && ytop + l.h < M - look.lh {
                    break;
                }
                let row = [M + pad - 4.0 * k, ytop, M + cw - pad + 4.0 * k, ytop + l.h];
                self.chat_row(c, &mut st, mi, &l, row, &look, a * ca, mouse, &mut hits, Mode::Closed, &set);
            }
            c.pop_clip();
            text::set_style(x.el.family, x.el.bolder, x.el.tshadow);
        }

        // ---- the window (grown: in the panel's glass; else in its own, sliding in, fading in or out of a point)
        if e > 0.01 {
            let from = c.p.len() / crate::gui::draw::FLOATS;
            c.alpha = a0 * if grow { smooth(0.25, 1.0, ok) } else { e };
            if !grow {
                x.part(c, [M, M + h - g.h, M + g.w, M + h]);
            }
            self.chat_window(c, &mut st, x, &g, &look, look_key, &set, ads, mouse, &mut hits, w, h);
            c.alpha = a0;
            match opens {
                1 => c.scale_about(from, [0.0, 0.0], 1.0, [0.0, (1.0 - e) * 56.0 * k * if anchor / 3 == 0 { -1.0 } else { 1.0 }]),
                3 => {
                    // the point: the closed chat's corner it is held by
                    let px = match anchor % 3 {
                        0 => M,
                        1 => M + g.w / 2.0,
                        _ => M + g.w,
                    };
                    let py = if anchor / 3 == 0 { M + h - g.h } else { M + h };
                    c.scale_about(from, [px, py], 0.55 + 0.45 * e, [0.0, 0.0]);
                }
                _ => {}
            }
        }

        // ---- the example (the editor, the menu's preview): when there is nothing to show
        if (st.rows.is_empty() || closed.is_empty()) && editing || st.rows.is_empty() && !x.icons {
            let ex = [("Alex", tr("Welcome back! Your base is safe.")), ("Steve", tr("who's up for a duel at spawn?")), (st.me.as_str(), tr("anyone seen the sunset today?"))];
            let me = st.me.clone();
            let eh = 3.0 * (look.lh + 8.0 * k) + pad * 2.0;
            let r = [M, M + h.max(eh) - eh, M + cw, M + h.max(eh)];
            if ch < 1.0 {
                x.part(c, r);
            }
            for (i, (who, said)) in ex.iter().enumerate() {
                let who = if who.is_empty() { me.as_str() } else { who };
                let top = r[1] + pad + i as f32 * (look.lh + 8.0 * k) + 4.0 * k;
                let base = top + look.lh * 0.72;
                let tx = r[0] + pad + 4.0 * k + gutter(&look);
                if look.heads {
                    c.rect([r[0] + pad + 4.0 * k, top + 1.5 * k, r[0] + pad + 22.0 * k, top + 19.5 * k], 4.0 * k, grey(1.0, 0.18));
                }
                let nw = c.text(tx, base, look.size, Face::Semi, who, look.base);
                c.text(tx + nw + 8.0 * k, base, look.size, Face::Regular, said, look.dim);
            }
            st.hits = hits;
            st.panel = [M, M, M + w, M + h.max(eh)];
            return Some([w.max(cw), h.max(eh)]);
        }

        // ---- the server's hover words over what the pointer is on (open)
        if let Some((tip, at)) = TIP.with(|tip| tip.borrow_mut().take()) {
            if ok > 0.5 && st.card.is_none() {
                self.chat_tip(c, &look, &tip, at, w);
            }
        }
        let toast_at = [M + g.left + (w - g.left) / 2.0, if g.field_top { M + h - 14.0 * k } else { M + h - g.field - 8.0 * k }];
        self.chat_toast(c, &mut st, &look, &mut hits, mouse, toast_at);
        st.hits = hits;
        st.panel = frame;
        Some([w, h.max(if editing { 40.0 * k } else { 0.0 })])
    }

    /// The open window's insides (its glass is drawn): the left column (title, channels, conversations), the right
    /// (header, pins, the messages, the field and its hints), what is open over it.
    #[allow(clippy::too_many_arguments)]
    fn chat_window(&mut self, c: &mut Canvas, st: &mut Store, x: &Ctx, g: &Geo, look: &Look, look_key: u64, set: &RowSet, ads: bool, mouse: [f32; 2], hits: &mut Vec<(Hit, [f32; 4])>, w: f32, h: f32) {
        let k = g.k;
        let t = st.now;
        let a0 = c.alpha;
        let (x0, y0) = (M, M);
        // the window grows from the panel: what is in it is laid at its full size and cut to what is open
        c.push_clip([x0, y0, x0 + w, y0 + h]);
        let wy = y0 + h - g.h;
        let lx1 = x0 + g.left;
        // what is open over the window takes the pointer: the rows under it are not lit
        let field_r = if g.field_top {
            [lx1 + 1.0 + 12.0 * k, wy + g.head + 8.0 * k, x0 + g.w - 12.0 * k, wy + g.head + g.field - 14.0 * k]
        } else {
            [lx1 + 1.0 + 12.0 * k, wy + g.h - g.field + 10.0 * k, x0 + g.w - 12.0 * k, wy + g.h - 12.0 * k]
        };
        st.hints_down = g.field_top;
        // what is under the messages and over them: the field's band (at the top or the foot)
        let (feed_top, feed_bottom) = if g.field_top { (wy + g.field + g.head - 6.0 * k, wy + g.h - 8.0 * k) } else { (wy + g.head, wy + g.h - g.field) };
        let popup = st.pop != 0 || st.card.is_some() || st.menu.is_some() || hints_box(st, k, field_r).is_some_and(|b| inside(b, mouse));
        // ---- left: the title, the channels, the conversations; their icons in a narrow column; or none (the header's
        // title lists them)
        let tabs = st.tabs();
        let n_priv = st.priv_total;
        let n_ment = st.ment_total;
        // what comes while its channel is shown is seen
        if st.tab == Tab::Private && !st.find {
            st.seen[0] = n_priv;
        }
        if st.tab == Tab::Mentions && !st.find {
            st.seen[1] = n_ment;
        }
        let new_in = |st: &Store, tab: Tab, on: bool| match tab {
            Tab::Private if !on => n_priv.saturating_sub(st.seen[0]),
            Tab::Mentions if !on => n_ment.saturating_sub(st.seen[1]),
            _ => 0,
        };
        let icons = g.col == 1;
        if g.col != 2 {
            c.rect([lx1, wy + 14.0 * k, lx1 + 1.0, wy + g.h - 14.0 * k], 0.0, grey(1.0, 0.07));
            if !icons {
                let srv = st.server.split(':').next().unwrap_or("").to_string();
                c.text(x0 + 18.0 * k, wy + 31.0 * k, 17.0 * k, Face::Semi, tr("Chat"), look.base);
                if !srv.is_empty() {
                    let s = fonts().fit(Face::Mono, 10.5 * k, &srv, g.left - 36.0 * k);
                    c.text(x0 + 18.0 * k, wy + 47.0 * k, 10.5 * k, Face::Mono, &s, fade(look.dim, 0.75));
                }
            }
            let ty0 = wy + if icons { 14.0 * k } else { 66.0 * k };
            let th = 34.0 * k;
            st.tabs_at = [x0 + 8.0 * k, ty0, lx1 - 8.0 * k, th];
            // (the channel shown lit; a conversation opened with the private channel not listed: none)
            let cur = tabs.iter().position(|&q| q == st.tab);
            let cy = ty0 + cur.unwrap_or(0) as f32 * th;
            let ind = self.an(ak(A_CH, 2), cy, 18.0, cy);
            if !st.find && cur.is_some() {
                let rr = [x0 + 8.0 * k, ind, lx1 - 8.0 * k, ind + th - 2.0 * k];
                c.rect_lit(rr, 9.0 * k, grey(1.0, 0.1), grey(1.0, 0.03), 1.0);
                if !icons {
                    c.line([rr[0] + 4.0 * k, rr[1] + 10.0 * k], [rr[0] + 4.0 * k, rr[3] - 10.0 * k], 2.0 * k, look.accent, 5.0);
                }
            }
            for (ti, tab) in tabs.iter().enumerate() {
                let rr = [x0 + 8.0 * k, ty0 + ti as f32 * th, lx1 - 8.0 * k, ty0 + (ti + 1) as f32 * th - 2.0 * k];
                let on = st.tab == *tab && !st.find;
                let held = st.tab_drag.as_ref().is_some_and(|d| d.0 == *tab && d.2);
                let hov = inside(rr, mouse) || held;
                let hv = self.an(ak(A_CH, 10 + ti), if hov { 1.0 } else { 0.0 }, 14.0, 0.0);
                if held {
                    c.rect(rr, 9.0 * k, grey(1.0, 0.06));
                }
                let col = if on { look.base } else { mix(fade(look.dim, 0.85), look.base, hv * 0.7) };
                let mid = (rr[1] + rr[3]) / 2.0;
                let s = 15.0 * k;
                let ix = if icons { (rr[0] + rr[2]) / 2.0 - s / 2.0 } else { rr[0] + 12.0 * k };
                c.icon([ix, mid - s / 2.0, ix + s, mid + s / 2.0], tab.icon(), col);
                let new = new_in(st, *tab, on);
                if icons {
                    if new > 0 {
                        c.rect([ix + s - 1.0 * k, mid - s / 2.0 - 3.0 * k, ix + s + 5.0 * k, mid - s / 2.0 + 3.0 * k], 3.0 * k, look.base);
                    }
                } else {
                    let name = st.tab_name(*tab);
                    c.text(rr[0] + 36.0 * k, mid + 5.0 * k, 13.5 * k, Face::Medium, &fonts().fit(Face::Medium, 13.5 * k, &name, rr[2] - rr[0] - 70.0 * k), col);
                    if new > 0 {
                        let lab = new.min(99).to_string();
                        let bw = fonts().width(Face::Mono, 10.0 * k, &lab) + 11.0 * k;
                        let br = [rr[2] - 8.0 * k - bw, mid - 8.0 * k, rr[2] - 8.0 * k, mid + 8.0 * k];
                        c.rect(br, 8.0 * k, look.base);
                        c.text_c((br[0] + br[2]) / 2.0, mid + 3.6 * k, 10.0 * k, Face::Mono, &lab, grey(0.04, 1.0));
                    }
                }
                hits.push((Hit::Tab(*tab), rr));
            }
            // the conversations
            let peers = st.conversations();
            let mut py = ty0 + tabs.len() as f32 * th + if icons { 10.0 * k } else { 16.0 * k };
            if !icons {
                c.text(x0 + 18.0 * k, py + 10.0 * k, 11.5 * k, Face::Semi, tr("Conversations"), fade(look.dim, 0.65));
                py += 20.0 * k;
                if peers.is_empty() {
                    for line in fonts().wrap(Face::Regular, 11.5 * k, tr("Private messages will be here"), g.left - 36.0 * k).iter().take(2) {
                        c.text(x0 + 18.0 * k, py + 14.0 * k, 11.5 * k, Face::Regular, line, fade(look.dim, 0.5));
                        py += 15.0 * k;
                    }
                }
            } else if !peers.is_empty() {
                c.rect([x0 + 16.0 * k, py - 5.0 * k, lx1 - 16.0 * k, py - 4.0 * k], 0.0, grey(1.0, 0.07));
            }
            let ph = if icons { 40.0 * k } else { 44.0 * k };
            for (name, ids) in peers.iter() {
                if py + ph > wy + g.h - 12.0 * k {
                    break;
                }
                let rr = [x0 + 8.0 * k, py, lx1 - 8.0 * k, py + ph];
                let on = st.tab == Tab::Private && st.dm.as_deref().is_some_and(|d| d.eq_ignore_ascii_case(name));
                let hov = inside(rr, mouse);
                c.rect(rr, 9.0 * k, grey(1.0, if on { 0.09 } else if hov { 0.045 } else { 0.0 }));
                let face = st.has_face(name);
                let unread = ids.iter().filter(|i| st.rows.get(**i).is_some_and(|r| !r.read && !r.mine)).count();
                if icons {
                    // their face, or the name's first letter in a circle
                    let fs = 24.0 * k;
                    let fr = [(rr[0] + rr[2] - fs) / 2.0, (rr[1] + rr[3] - fs) / 2.0, (rr[0] + rr[2] + fs) / 2.0, (rr[1] + rr[3] + fs) / 2.0];
                    if face {
                        self.chat_face(st, name, fr, c.alpha);
                    } else {
                        c.rect(fr, fs / 2.0, grey(1.0, 0.08));
                        let first: String = name.chars().next().map(|ch| ch.to_uppercase().collect()).unwrap_or_default();
                        c.text_c((fr[0] + fr[2]) / 2.0, fr[3] - 7.5 * k, 12.0 * k, Face::Semi, &first, look.base);
                    }
                    if unread > 0 && !on {
                        c.rect([fr[2] - 4.0 * k, fr[1] - 2.0 * k, fr[2] + 3.0 * k, fr[1] + 5.0 * k], 3.5 * k, look.base);
                    }
                } else {
                    // their face while they are in the game; else the words take its place
                    let fs = 26.0 * k;
                    if face {
                        self.chat_face(st, name, [rr[0] + 8.0 * k, rr[1] + 9.0 * k, rr[0] + 8.0 * k + fs, rr[1] + 9.0 * k + fs], c.alpha);
                    }
                    let right = if unread > 0 && !on { rr[2] - 30.0 * k } else { rr[2] - 8.0 * k };
                    let nx = rr[0] + if face { 42.0 * k } else { 10.0 * k };
                    let nm = fonts().fit(Face::Semi, 13.0 * k, name, right - nx);
                    c.text(nx, rr[1] + 19.0 * k, 13.0 * k, Face::Semi, &nm, look.base);
                    if let Some(last) = ids.last().and_then(|i| st.rows.get(*i)) {
                        let (txt, mine) = match &last.kind {
                            Said::Whisper { text, mine, .. } => (super::parse::rich::plain(text), *mine),
                            _ => (String::new(), false),
                        };
                        let line = format!("{}{}", if mine { format!("{}: ", tr("you")) } else { String::new() }, txt);
                        let fit = fonts().fit(Face::Regular, 11.5 * k, &line, right - nx);
                        c.text(nx, rr[1] + 35.0 * k, 11.5 * k, Face::Regular, &fit, fade(look.dim, 0.75));
                    }
                    if unread > 0 && !on {
                        let lab = unread.min(99).to_string();
                        let bw = fonts().width(Face::Mono, 10.0 * k, &lab) + 11.0 * k;
                        let br = [rr[2] - 8.0 * k - bw, rr[1] + 14.0 * k, rr[2] - 8.0 * k, rr[1] + 30.0 * k];
                        c.rect(br, 8.0 * k, look.base);
                        c.text_c((br[0] + br[2]) / 2.0, br[3] - 4.4 * k, 10.0 * k, Face::Mono, &lab, grey(0.04, 1.0));
                    }
                }
                hits.push((Hit::Peer(name.clone()), rr));
                py += ph + 2.0 * k;
            }
        }

        // ---- right: the header (or the search), the tools
        let rx0 = lx1 + 1.0;
        let rx1 = x0 + g.w;
        let tools_x = rx1 - 14.0 * k;
        // (the gear lit while the chat's settings are open by it)
        let settings_open = self.hud.pop == Some(self.mod_index("chat"));
        let bs = 30.0 * k;
        let hy = wy + (g.head - bs) / 2.0;
        for (i, tool) in [(0u8, 13u32), (1, 39)].iter().enumerate() {
            let bx = tools_x - (2 - i) as f32 * (bs + 4.0 * k) + 4.0 * k;
            let r = [bx, hy, bx + bs, hy + bs];
            let on = (tool.0 == 1 && settings_open) || (tool.0 == 0 && st.find);
            let hov = inside(r, mouse);
            let lit = self.an(ak(A_CH, 20 + i), if on || hov { 1.0 } else { 0.0 }, 14.0, 0.0);
            c.rect(r, 8.0 * k, grey(1.0, 0.03 + 0.08 * lit));
            let s = 15.0 * k;
            let mid = [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0];
            c.icon([mid[0] - s / 2.0, mid[1] - s / 2.0, mid[0] + s / 2.0, mid[1] + s / 2.0], tool.1, mix(fade(look.dim, 0.9), look.base, lit));
            hits.push((Hit::Tool(tool.0), r));
        }
        let title_right = tools_x - 2.0 * (bs + 4.0 * k) - 8.0 * k;
        let hx = rx0 + 18.0 * k;
        let feed_tab = st.tab;
        let dm = if feed_tab == Tab::Private { st.dm.clone() } else { None };
        if st.find {
            let q = st.field.trim().to_string();
            let s = 14.0 * k;
            c.icon([hx, wy + 31.0 * k - s * 0.8, hx + s, wy + 31.0 * k + s * 0.2], 13, fade(look.dim, 0.9));
            let n = if q.is_empty() { 0 } else { st.rows.iter().filter(|r| r.plain.to_lowercase().contains(&q.to_lowercase())).count() };
            let label = if q.is_empty() { tr("Search the chat").to_string() } else { q.clone() };
            c.text(hx + 22.0 * k, wy + 31.0 * k, 15.0 * k, Face::Regular, &fonts().fit(Face::Regular, 15.0 * k, &label, title_right - hx - 80.0 * k), if q.is_empty() { fade(look.dim, 0.7) } else { look.base });
            let right = if q.is_empty() { String::new() } else if n == 0 { tr("nothing").to_string() } else { super::super::super::i18n::trn("{n} found", &[("n", n.to_string())]) };
            c.text_r(title_right, wy + 31.0 * k, 11.5 * k, Face::Mono, &right, fade(look.dim, 0.8));
            let ul = self.an(ak(A_CH, 5), 1.0, 8.0, 0.0);
            c.rect([hx, wy + g.head - 10.0 * k, hx + (title_right - hx) * ul, wy + g.head - 9.0 * k], 0.5, fade(look.base, 0.7));
        } else {
            let (title, sub) = match &dm {
                Some(peer) => (peer.clone(), tr("Private messages").to_string()),
                None => {
                    let n = st.rows.iter().filter(|r| st.shows(r, feed_tab, ads)).count();
                    (st.tab_name(feed_tab), super::super::super::i18n::trn("{n} messages", &[("n", n.to_string())]))
                }
            };
            let mut tx = hx;
            if let Some(peer) = dm.as_ref().filter(|p| st.has_face(p)) {
                let fs = 24.0 * k;
                self.chat_face(st, peer, [tx, wy + 17.0 * k, tx + fs, wy + 17.0 * k + fs], c.alpha);
                tx += fs + 10.0 * k;
            }
            let tw = c.text(tx, wy + 29.0 * k, 16.0 * k, Face::Semi, &fonts().fit(Face::Semi, 16.0 * k, &title, title_right - tx - 24.0 * k), look.base);
            c.text(tx, wy + 45.0 * k, 11.5 * k, Face::Regular, &sub, fade(look.dim, 0.75));
            if g.col == 2 {
                let hr = [hx - 8.0 * k, wy + 9.0 * k, tx + tw + 30.0 * k, wy + g.head - 7.0 * k];
                let hv = self.an(ak(A_CH, 24), if inside(hr, mouse) || st.pop == 3 { 1.0 } else { 0.0 }, 14.0, 0.0);
                c.rect(hr, 9.0 * k, grey(1.0, 0.05 * hv));
                let s = 11.0 * k;
                c.icon([tx + tw + 9.0 * k, wy + 24.0 * k - s / 2.0, tx + tw + 9.0 * k + s, wy + 24.0 * k + s / 2.0], 11, mix(look.dim, look.base, hv));
                hits.push((Hit::Chans, hr));
                st.chans_at = [hr[0], hr[3] + 4.0 * k];
            }
        }
        c.rect([rx0 + 14.0 * k, wy + g.head - 1.0, rx1 - 14.0 * k, wy + g.head], 0.0, grey(1.0, 0.06));

        // ---- the pinned events (not over a conversation)
        let pins: Vec<super::store::Pin> = if x.on("Pinned events") && dm.is_none() && !st.find { st.pins.iter().filter(|p| p.until > t).cloned().collect() } else { Vec::new() };
        let pins_k = self.an(ak(A_CH, 4), pins.len() as f32, 10.0, 0.0);
        let pins_h = pins_k * 36.0 * k + if pins_k > 0.01 { 6.0 * k } else { 0.0 };
        let mut py = feed_top + 6.0 * k;
        for (pi, p) in pins.iter().enumerate() {
            let ka = (pins_k - pi as f32).clamp(0.0, 1.0) * ease_out(((t - p.born) / 0.35).clamp(0.0, 1.0));
            if ka <= 0.01 {
                continue;
            }
            c.alpha = a0 * ka;
            let r = [rx0 + 12.0 * k, py, rx1 - 12.0 * k, py + 30.0 * k];
            c.rect(r, 9.0 * k, grey(1.0, 0.055));
            let mid = (r[1] + r[3]) / 2.0;
            let s = 13.0 * k;
            c.icon([r[0] + 10.0 * k, mid - s / 2.0, r[0] + 10.0 * k + s, mid + s / 2.0], 40, look.base);
            let nw = c.text(r[0] + 30.0 * k, mid + 4.8 * k, 13.5 * k, Face::Semi, &p.name, look.base);
            if !p.xyz.is_empty() {
                let at = body::place_chip(&p.xyz, look);
                let cw = fonts().width(at.face, at.size, &at.text) + 27.0 * k;
                let pl = vec![Placed { ai: 0, line: 0, x: 0.0, text: at.text.clone(), w: cw }];
                self.chat_atoms(c, &[at], &pl, r[0] + 30.0 * k + nw + 10.0 * k, mid - look.lh * 0.5, look.lh, look, c.alpha, mouse, hits, t, true);
            }
            let left = (p.until - t).max(0.0) as u32;
            c.text_r(r[2] - 10.0 * k, mid + 4.0 * k, 11.5 * k, Face::Mono, &format!("{}:{:02}", left / 60, left % 60), fade(look.dim, 0.85));
            py += 36.0 * k;
        }
        c.alpha = a0;

        // ---- the messages (or the conversation's bubbles), newest at the foot, scrolled
        let feed = [rx0 + 10.0 * k, feed_top + pins_h + 2.0 * k, rx1 - 10.0 * k, feed_bottom];
        let fw = feed[2] - feed[0];
        let list_h = feed[3] - feed[1];
        let dt = self.dt.max(0.0);
        let smooth_scroll = x.on("Smooth scrolling");
        c.push_clip([feed[0] - 4.0 * k, feed[1], feed[2] + 4.0 * k, feed[3]]);
        let feed_hits = hits.len();
        let total = if let Some(peer) = &dm {
            // the conversation scrolls as the feed does (its height known after it is laid: a frame late)
            if smooth_scroll {
                st.scroll += (st.scroll_t - st.scroll) * (1.0 - (-dt * 16.0).exp());
            } else {
                st.scroll = st.scroll_t;
            }
            let total = self.chat_bubbles(c, st, look, feed, peer, mouse, hits, set);
            st.scroll_t = st.scroll_t.clamp(0.0, (total - list_h).max(0.0));
            if st.scroll_t < 2.0 {
                st.unread = 0;
            }
            total
        } else {
            let n = st.rows.len();
            let mut shown: Vec<usize> = Vec::new();
            let mut total = 0.0f32;
            // a divider over a message after a quiet (by when they came), its height
            let mut parted: Vec<usize> = Vec::new();
            let div_h = 30.0 * k;
            let mut mi = n;
            // the rows from the newest up, until well past what the scroll shows
            while mi > 0 {
                mi -= 1;
                if !st.shows(&st.rows[mi], feed_tab, ads) {
                    continue;
                }
                if !look.q.is_empty() && !st.rows[mi].plain.to_lowercase().contains(&look.q) {
                    continue;
                }
                let cont = shown.last().is_some_and(|_| false) || (mi > 0 && st.shows(&st.rows[mi - 1], feed_tab, ads) && goes_on(st, mi - 1, mi));
                let lh = st.laid(mi, fw - set.room, look, look_key, cont).h;
                if let (Some(p), Some(&newer)) = (set.pause, shown.last()) {
                    if st.rows[newer].born - st.rows[mi].born > p {
                        parted.push(newer);
                        total += div_h;
                    }
                }
                total += lh;
                shown.push(mi);
                if total > list_h + st.scroll_t + 400.0 * k {
                    break;
                }
            }
            let max_scroll = (total - list_h).max(0.0);
            st.scroll_t = st.scroll_t.clamp(0.0, max_scroll);
            if smooth_scroll {
                st.scroll += (st.scroll_t - st.scroll) * (1.0 - (-dt * 16.0).exp());
            } else {
                st.scroll = st.scroll_t;
            }
            if st.scroll_t < 2.0 {
                st.unread = 0;
            }
            let mut yb = feed[3] - 4.0 * k + st.scroll;
            for &mi in &shown {
                let cont = mi > 0 && st.shows(&st.rows[mi - 1], feed_tab, ads) && goes_on(st, mi - 1, mi);
                let l = st.laid(mi, fw - set.room, look, look_key, cont).clone();
                let ytop = yb - l.h;
                yb -= l.h;
                // the quiet before it: its room over the row (the scroll's height counts it, the row seen or not)
                let parted_at = parted.contains(&mi).then(|| {
                    let y = yb;
                    yb -= div_h;
                    y
                });
                if ytop > feed[3] + 4.0 {
                    continue;
                }
                if ytop + l.h < feed[1] - 4.0 {
                    break;
                }
                let age = t - st.rows[mi].born;
                let rise = 1.0 - ease_out((age / 0.35).clamp(0.0, 1.0));
                // the top edge: rows fade as they go under the header
                let edge = (((ytop + l.h) - feed[1]) / (look.lh * 1.4)).clamp(0.0, 1.0);
                let row = [feed[0], ytop + rise * look.lh * 0.5, feed[2], ytop + l.h + rise * look.lh * 0.5];
                self.chat_row(c, st, mi, &l, row, look, (1.0 - rise * 0.9) * edge, if popup { [-1.0e5, -1.0e5] } else { mouse }, hits, Mode::Open, set);
                // the quiet before it: a thin line with the time it ended
                if let Some(y0) = parted_at {
                    let mid = y0 - div_h / 2.0;
                    let lab = clock_text(st.rows[mi].clock, false, set.h24);
                    let fs = 10.5 * k;
                    let lw = fonts().width(Face::Mono, fs, &lab) + 18.0 * k;
                    let cx = (feed[0] + feed[2]) / 2.0;
                    let col = grey(1.0, 0.08 * edge);
                    c.rect([feed[0] + 12.0 * k, mid, cx - lw / 2.0, mid + 1.0], 0.0, col);
                    c.rect([cx + lw / 2.0, mid, feed[2] - 12.0 * k, mid + 1.0], 0.0, col);
                    c.text_c(cx, mid + fs * 0.36, fs, Face::Mono, &lab, fade(look.dim, 0.7 * edge));
                }
            }
            total
        };
        c.pop_clip();
        // what is scrolled out of sight takes no presses (a row under the header took the gear's)
        let clip = [feed[0] - 4.0 * k, feed[1], feed[2] + 4.0 * k, feed[3]];
        let kept: Vec<(Hit, [f32; 4])> = hits
            .drain(feed_hits..)
            .filter_map(|(h, q)| {
                let cq = [q[0].max(clip[0]), q[1].max(clip[1]), q[2].min(clip[2]), q[3].min(clip[3])];
                (cq[2] > cq[0] && cq[3] > cq[1]).then_some((h, cq))
            })
            .collect();
        hits.extend(kept);
        // the scroll bar: while it moves, or the pointer is near
        if total > list_h + 1.0 {
            let near = inside([feed[2] - 30.0 * k, feed[1], feed[2] + 10.0 * k, feed[3]], mouse);
            if (st.scroll - st.scroll_t).abs() > 0.5 || near {
                st.bar_at = t;
            }
            let ba = self.an(ak(A_CH, 6), if t - st.bar_at < 1.2 { 1.0 } else { 0.0 }, 8.0, 0.0);
            if ba > 0.01 {
                let max_scroll = (total - list_h).max(1.0);
                let len = (list_h * list_h / total).max(28.0 * k);
                let y1 = feed[3] - (feed[3] - feed[1] - len) * (st.scroll / max_scroll).clamp(0.0, 1.0);
                c.rect([feed[2] + 3.0 * k, y1 - len, feed[2] + 6.0 * k, y1], 1.5 * k, fade(look.base, 0.28 * ba));
            }
        }
        // «↓ new messages» while scrolled up
        let nb = self.an(ak(A_CH, 3), if st.unread > 0 { 1.0 } else { 0.0 }, 10.0, 0.0);
        if nb > 0.01 {
            let label = format!("{} · {}", tr("New messages"), st.unread);
            let fs = 12.5 * k;
            let tw = fonts().width(Face::Semi, fs, &label) + 44.0 * k;
            let cx = (feed[0] + feed[2]) / 2.0;
            let r = [cx - tw / 2.0, feed[3] - 38.0 * k + (1.0 - nb) * 8.0 * k, cx + tw / 2.0, feed[3] - 10.0 * k + (1.0 - nb) * 8.0 * k];
            c.alpha = a0 * nb;
            c.shadow(r, (r[3] - r[1]) / 2.0, 14.0 * k, grey(0.0, 0.4));
            c.rect(r, (r[3] - r[1]) / 2.0, look.base);
            c.text(r[0] + 14.0 * k, (r[1] + r[3]) / 2.0 + fs * 0.36, fs, Face::Semi, &label, grey(0.04, 1.0));
            let s = 12.0 * k;
            c.icon([r[2] - 24.0 * k, (r[1] + r[3]) / 2.0 - s / 2.0, r[2] - 24.0 * k + s, (r[1] + r[3]) / 2.0 + s / 2.0], 11, grey(0.04, 1.0));
            hits.push((Hit::Pill, r));
            c.alpha = a0;
        }

        // ---- the field
        // (what Java draws from here on is over the window's rows)
        let n = self.hud.icons.len();
        ICONS_AT.with(|a| a.set([a.get()[0], n, usize::MAX]));
        self.chat_field(c, st, look, [field_r[0], field_r[1], rx1 - 12.0 * k, field_r[3]], mouse, hits);

        // ---- the corner to stretch it by (the one away from where it is held)
        let anchor = self.hud.places.get("chat").map_or(6, |p| p.0);
        let (gx, gy) = (if anchor % 3 == 2 { x0 } else { x0 + g.w }, if anchor / 3 == 0 { wy + g.h } else { wy });
        let gr = [gx - 14.0 * k, gy - 14.0 * k, gx + 14.0 * k, gy + 14.0 * k];
        let gv = self.an(ak(A_CH, 7), if inside(gr, mouse) || st.grip.is_some() { 1.0 } else { 0.0 }, 12.0, 0.0);
        if gv > 0.01 {
            let sx = if anchor % 3 == 2 { 1.0 } else { -1.0 };
            let sy = if anchor / 3 == 0 { -1.0 } else { 1.0 };
            let col = fade(look.base, 0.6 * gv);
            for d in [5.0, 9.0] {
                c.line([gx + sx * 3.0 * k, gy + sy * (3.0 + d) * k], [gx + sx * (3.0 + d) * k, gy + sy * 3.0 * k], 1.4 * k, col, 0.0);
            }
        }
        hits.push((Hit::Grip, gr));
        c.pop_clip();
        // ---- over it: the settings, the hidden, the player's card, the hints
        self.chat_popups(c, st, look, mouse, hits, [x0, wy, x0 + g.w, wy + g.h], tools_x);
        hits.insert(0, (Hit::Panel, [x0, wy, x0 + g.w, wy + g.h]));
    }

    /// The private conversation with `peer` as bubbles (yours on the right, light; theirs on the left); its height.
    #[allow(clippy::too_many_arguments)]
    fn chat_bubbles(&mut self, c: &mut Canvas, st: &mut Store, look: &Look, feed: [f32; 4], peer: &str, mouse: [f32; 2], hits: &mut Vec<(Hit, [f32; 4])>, set: &RowSet) -> f32 {
        let k = look.k;
        let ids: Vec<usize> = st.rows.iter().enumerate().filter(|(_, r)| matches!(&r.kind, Said::Whisper { from, .. } if from.eq_ignore_ascii_case(peer))).map(|(i, _)| i).collect();
        if ids.is_empty() {
            c.text_c((feed[0] + feed[2]) / 2.0, (feed[1] + feed[3]) / 2.0, 13.0 * k, Face::Regular, tr("No messages yet"), fade(look.dim, 0.6));
            return 0.0;
        }
        let maxw = (feed[2] - feed[0]) * 0.72;
        let mut by = feed[3] - 6.0 * k + st.scroll;
        let mut total = 0.0;
        // the server's answer to what did not reach them, under the last words (a while)
        if let Some((_, said, at)) = st.note.clone().filter(|n| n.0.eq_ignore_ascii_case(peer)) {
            let age = st.now - at;
            let a = ease_out((age / 0.3).clamp(0.0, 1.0)) * (1.0 - ((age - 10.0) / 0.6).clamp(0.0, 1.0));
            if a > 0.01 {
                let line = format!("{} · {said}", tr("Not delivered"));
                let fs = 11.5 * k;
                let line = fonts().fit(Face::Regular, fs, &line, feed[2] - feed[0] - 40.0 * k);
                let w = fonts().width(Face::Regular, fs, &line);
                c.text(feed[2] - w - 4.0 * k, by - 5.5 * k, fs, Face::Regular, &line, fade(look.dim, 0.9 * a));
                by -= 26.0 * k * a;
                total += 26.0 * k * a;
            }
        }
        for &i in ids.iter().rev() {
            let (mine, id, clock) = match &st.rows[i].kind {
                Said::Whisper { mine, .. } => (*mine, st.rows[i].id, st.rows[i].clock),
                _ => continue,
            };
            let text = match &st.rows[i].kind {
                Said::Whisper { text, .. } => text.clone(),
                _ => continue,
            };
            // the bubble's colour as set; on a light one the words are dark, whatever the server's colours
            let bub = if mine { set.bub_mine } else { set.bub_theirs };
            let light = (0.3 * bub[0] + 0.59 * bub[1] + 0.11 * bub[2]) * bub[3] + 0.05 * (1.0 - bub[3]) > 0.45;
            let col = if light { grey(0.05, 1.0) } else { look.base };
            let lk = Look { base: col, me: String::new(), q: String::new(), indent: 0.0, ..look.clone() };
            let words: Vec<super::parse::rich::Span> = if light { text.iter().map(|s| super::parse::rich::Span { colour: None, ..s.clone() }).collect() } else { text };
            let mut atoms = Vec::new();
            let row = super::store::Row {
                id,
                clock,
                kind: Said::Raw,
                lines: vec![(id, words)],
                cat: 2,
                mention: false,
                who: String::new(),
                mine,
                plain: String::new(),
                key: None,
                count: 1,
                born: 0.0,
                bump: 0.0,
                until: 0.0,
                secs: 0.0,
                state: 0,
                read: true,
                version: 0,
                laid: None,
            };
            if let Body::Sys { atoms: a, .. } | Body::Line { atoms: a, .. } = st.body(&row, &lk, true) {
                atoms = a;
            }
            let (pl, n) = body::lay(&atoms, maxw - 24.0 * k, k, 0.0);
            let bw = pl.iter().map(|p| p.x + p.w).fold(0.0, f32::max).max(54.0 * k) + 24.0 * k;
            let bh = n as f32 * look.lh + 24.0 * k;
            let bx = if mine && !set.bub_left { feed[2] - bw } else { feed[0] };
            let br = [bx, by - bh, bx + bw, by];
            c.rect(br, set.bub_r.min(bh / 2.0), bub);
            self.chat_atoms(c, &atoms, &pl, bx + 12.0 * k, br[1] + 6.0 * k, look.lh, &lk, c.alpha, mouse, hits, st.now, true);
            if clock >= 0 {
                c.text_r(br[2] - 9.0 * k, br[3] - 5.0 * k, 9.5 * k, Face::Mono, &clock_text(clock, false, set.h24), if light { grey(0.05, 0.5) } else { fade(look.dim, 0.6) });
            }
            by -= bh + 6.0 * k;
            total += bh + 6.0 * k;
            st.rows[i].read = true;
            if by < feed[1] - 400.0 * k {
                break;
            }
        }
        total
    }

    /// The field: who it goes to, the game's words, the caret, the selection, the hint's ghost, how much is left; the
    /// command hints (the game's) or a command's usage over it.
    fn chat_field(&mut self, c: &mut Canvas, st: &mut Store, look: &Look, r: [f32; 4], mouse: [f32; 2], hits: &mut Vec<(Hit, [f32; 4])>) {
        let k = look.k;
        let t = st.now;
        let fhv = self.an(ak(A_CH, 8), if inside(r, mouse) { 1.0 } else { 0.0 }, 14.0, 0.0);
        c.rect(r, 11.0 * k, grey(1.0, 0.05 + 0.025 * fhv));
        let fs = 15.0 * k;
        let mut tx = r[0] + 12.0 * k;
        let mid = (r[1] + r[3]) / 2.0;
        let base = mid + fs * 0.36;
        let field = st.field.clone();
        // who it goes to: near (as typed), everyone ("!" on the servers that read it), a private message, a command
        let server = Server::of(&st.server);
        let lower = field.to_lowercase();
        // a private message: to the name typed (once it is whole: a space after it), or "/r" to who wrote last
        let pm = if lower.starts_with("/r ") {
            st.rows.iter().rev().find_map(|r| match &r.kind {
                Said::Whisper { from, mine: false, .. } => Some(from.clone()),
                _ => None,
            })
        } else {
            ["/msg ", "/m ", "/w ", "/tell "].iter().find(|p| lower.starts_with(*p)).and_then(|p| field.get(p.len()..)?.split_once(' ').map(|(n, _)| n.to_string()))
        };
        let talk = st.talking_to();
        let (chan, icon): (String, u32) = if st.find {
            (tr("Search").to_string(), 13)
        } else if let Some(to) = talk.clone() {
            (to, 36)
        } else if let Some(to) = pm.clone().filter(|v| !v.is_empty()) {
            (to, 36)
        } else if field.starts_with('/') {
            (tr("Command").to_string(), 42)
        } else if server != Server::Other && field.starts_with('!') {
            (tr("Everyone").to_string(), 37)
        } else if server != Server::Other {
            (tr("near").to_string(), 35)
        } else {
            (String::new(), 0)
        };
        if !chan.is_empty() {
            let cw = fonts().width(Face::Medium, 12.5 * k, &chan) + 34.0 * k;
            let cr = [r[0] + 6.0 * k, mid - 14.0 * k, r[0] + 6.0 * k + cw, mid + 14.0 * k];
            let hov = inside(cr, mouse);
            c.rect(cr, 8.0 * k, grey(1.0, if hov { 0.12 } else { 0.065 }));
            let s = 13.0 * k;
            c.icon([cr[0] + 9.0 * k, mid - s / 2.0, cr[0] + 9.0 * k + s, mid + s / 2.0], icon, look.base);
            c.text(cr[0] + 27.0 * k, mid + 4.5 * k, 12.5 * k, Face::Medium, &chan, look.base);
            if server != Server::Other && !st.find && pm.is_none() && talk.is_none() && !field.starts_with('/') {
                hits.push((Hit::Chan, cr));
            }
            tx = cr[2] + 10.0 * k;
        }
        let chars: Vec<char> = field.chars().collect();
        let upto = |n: usize| -> String { chars.iter().take(n.min(chars.len())).collect() };
        let room = r[2] - tx - 46.0 * k;
        let full_w = fonts().width(Face::Regular, fs, &field);
        let cur_w = fonts().width(Face::Regular, fs, &upto(st.cursor));
        let off = (cur_w - room * 0.85).max(0.0).min((full_w - room).max(0.0));
        st.field_off = off;
        c.push_clip([tx - 2.0, r[1], r[2] - 42.0 * k, r[3]]);
        if field.is_empty() {
            let ph = match &talk {
                _ if st.find => tr("Search the chat").to_string(),
                Some(to) => super::super::super::i18n::trn("Message to {name}", &[("name", to.clone())]),
                None => tr("Write a message or / for a command").to_string(),
            };
            c.text(tx, base, fs, Face::Regular, &ph, fade(look.dim, 0.6));
        } else {
            if st.anchor != st.cursor {
                let (a, b) = (st.anchor.min(st.cursor), st.anchor.max(st.cursor));
                let xa = tx - off + fonts().width(Face::Regular, fs, &upto(a));
                let xb = tx - off + fonts().width(Face::Regular, fs, &upto(b));
                c.rect([xa, r[1] + 10.0 * k, xb, r[3] - 10.0 * k], 2.0, fade(look.base, 0.22));
            }
            let w = c.text(tx - off, base, fs, Face::Regular, &field, look.base);
            if !st.ghost.is_empty() {
                c.text(tx - off + w, base, fs, Face::Regular, &st.ghost, fade(look.dim, 0.55));
            }
        }
        let blink = ((t * 1.9).fract() < 0.55) as i32 as f32;
        let cx = tx - off + cur_w;
        c.rect([cx, r[1] + 11.0 * k, cx + 1.5 * k, r[3] - 11.0 * k], 0.5, fade(look.base, blink));
        c.pop_clip();
        hits.push((Hit::Field, [tx - 4.0 * k, r[1], r[2] - 42.0 * k, r[3]]));
        // how much of the 256 a line may be is left (in a conversation the command to them is counted too)
        let n = chars.len() as f32 + talk.as_ref().map_or(0.0, |to| format!("/msg {to} ").chars().count() as f32);
        if !chars.is_empty() && !st.find {
            let kk = (n / 256.0).min(1.0);
            let col = if n > 230.0 { look.base } else { fade(look.base, 0.3) };
            c.rect([tx, r[3] - 7.0 * k, tx + (r[2] - 46.0 * k - tx) * kk, r[3] - 5.5 * k], 0.75 * k, col);
            c.text_r(r[2] - 12.0 * k, base, 11.0 * k, Face::Mono, &format!("{}", 256 - n as i32), fade(look.dim, if n > 200.0 { 0.95 } else { 0.5 }));
        }
        // the game's command hints or the command's usage, over the field
        let list: Vec<String> = if !st.sugg.is_empty() { st.sugg.clone() } else { st.usage.clone() };
        // what the hints are: the command's name (the first word after "/"), or what goes after it
        let cmd_name = field.starts_with('/') && !upto(st.cursor).contains(' ');
        if let Some(pr) = hints_box(st, k, r) {
            over(pr, 1.0);
            let rows = list.len().min(8);
            let rh = 32.0 * k;
            c.shadow(pr, 12.0 * k, 26.0 * k, grey(0.0, 0.5));
            let gl = crate::gui::theme::pal().glass;
            c.rect(pr, 12.0 * k, [gl[0].max(0.06), gl[1].max(0.06), gl[2].max(0.07), 1.0]);
            let typed = field.rsplit(' ').next().unwrap_or("").to_lowercase();
            for (i, item) in list.iter().take(rows).enumerate() {
                let rr = [pr[0] + 5.0 * k, pr[1] + 5.0 * k + i as f32 * rh, pr[2] - 5.0 * k, pr[1] + 5.0 * k + (i + 1) as f32 * rh];
                let idx = st.sugg_off + i;
                let on = !st.sugg.is_empty() && idx as i32 == st.sugg_sel;
                let hov = inside(rr, mouse);
                if on || hov {
                    c.rect(rr, 8.0 * k, grey(1.0, if on { 0.09 } else { 0.05 }));
                }
                let b = (rr[1] + rr[3]) / 2.0 + 13.5 * k * 0.36;
                let fs2 = 13.5 * k;
                let ix = rr[0] + 10.0 * k;
                if st.sugg.is_empty() {
                    c.text(ix, b, fs2, Face::Mono, item, look.dim);
                } else {
                    // a command: a slash in a well; a player in the game: their face; anything else as it is
                    let wr = [ix, (rr[1] + rr[3]) / 2.0 - 10.0 * k, ix + 20.0 * k, (rr[1] + rr[3]) / 2.0 + 10.0 * k];
                    if cmd_name || item.starts_with('/') {
                        c.rect(wr, 5.0 * k, grey(1.0, 0.06));
                        c.text_c((wr[0] + wr[2]) / 2.0, wr[3] - 5.5 * k, 12.0 * k, Face::Mono, "/", look.dim);
                    } else if st.has_face(item) {
                        self.chat_face(st, item, wr, c.alpha);
                    } else {
                        c.rect([wr[0] + 8.0 * k, wr[1] + 8.0 * k, wr[2] - 8.0 * k, wr[3] - 8.0 * k], 2.0 * k, fade(look.dim, 0.5));
                    }
                    let lo = item.to_lowercase();
                    let tq = typed.trim_start_matches('/').to_string();
                    let n = if !tq.is_empty() && lo.trim_start_matches('/').starts_with(&tq) { tq.chars().count() + item.starts_with('/') as usize } else { 0 };
                    let head: String = item.chars().take(n).collect();
                    let rest: String = item.chars().skip(n).collect();
                    let hx = ix + 30.0 * k;
                    let w = c.text(hx, b, fs2, Face::Mono, &head, look.base);
                    c.text(hx + w, b, fs2, Face::Mono, &rest, fade(look.base, 0.62));
                    hits.push((Hit::Sugg(idx), rr));
                }
            }
        }
    }

    /// What is open over the window: the settings' list, the hidden players, the player's card.
    #[allow(clippy::too_many_arguments)]
    fn chat_popups(&mut self, c: &mut Canvas, st: &mut Store, look: &Look, mouse: [f32; 2], hits: &mut Vec<(Hit, [f32; 4])>, win: [f32; 4], tools_x: f32) {
        let k = look.k;
        let t = st.now;
        let a0 = c.alpha;
        let gl = crate::gui::theme::pal().glass;
        let pop_bg = [gl[0].max(0.055), gl[1].max(0.055), gl[2].max(0.065), 1.0];
        // a message's menu (a right click on it): it copied, its writer's name copied, an answer, a mention, its
        // writer hidden; the chat's settings
        let mo = self.an(ak(A_CH, 30), if st.menu.is_some() { 1.0 } else { 0.0 }, 18.0, 0.0);
        if let Some((id, at, _)) = st.menu.clone().filter(|_| mo > 0.01) {
            if let Some((who, mine)) = st.row_by_id(id).map(|r| (r.who.clone(), r.mine)) {
                let mut items: Vec<(u8, u32, &str)> = vec![(2, 32, "Copy")];
                if !who.is_empty() {
                    items.push((4, 34, "Copy the name"));
                    if !mine {
                        items.extend([(0, 30, "Reply"), (1, 31, "Mention"), (3, 33, "Hide the player")]);
                    }
                }
                items.push((5, 39, "Chat settings"));
                let w = items.iter().map(|q| fonts().width(Face::Regular, 13.0 * k, tr(q.2))).fold(0.0, f32::max) + 58.0 * k;
                let rh = 32.0 * k;
                let hgt = 12.0 * k + items.len() as f32 * rh + 9.0 * k;
                let mx = at[0].clamp(win[0] + 6.0 * k, (win[2] - w - 6.0 * k).max(win[0]));
                let my = if at[1] + hgt > win[3] - 6.0 * k { (at[1] - hgt).max(win[1] + 6.0 * k) } else { at[1] };
                let r = [mx, my + (1.0 - mo) * 6.0 * k, mx + w, my + hgt + (1.0 - mo) * 6.0 * k];
                over(r, mo);
                c.alpha = a0 * mo;
                c.shadow(r, 12.0 * k, 26.0 * k, grey(0.0, 0.5));
                c.rect(r, 12.0 * k, pop_bg);
                let mut y = r[1] + 6.0 * k;
                for (n, (which, icon, label)) in items.iter().enumerate() {
                    // the chat's settings apart from what is done to the message
                    if n + 1 == items.len() {
                        c.rect([r[0] + 12.0 * k, y + 4.0 * k, r[2] - 12.0 * k, y + 4.0 * k + 1.0], 0.0, grey(1.0, 0.07));
                        y += 9.0 * k;
                    }
                    let rr = [r[0] + 5.0 * k, y, r[2] - 5.0 * k, y + rh];
                    let hov = inside(rr, mouse);
                    if hov {
                        c.rect(rr, 8.0 * k, grey(1.0, 0.07));
                    }
                    let s = 14.0 * k;
                    let mid = (rr[1] + rr[3]) / 2.0;
                    c.icon([rr[0] + 10.0 * k, mid - s / 2.0, rr[0] + 10.0 * k + s, mid + s / 2.0], *icon, fade(look.base, if hov { 1.0 } else { 0.75 }));
                    c.text(rr[0] + 34.0 * k, mid + 4.6 * k, 13.0 * k, Face::Regular, tr(label), if hov { look.base } else { mix(look.dim, look.base, 0.6) });
                    hits.push((Hit::Act(*which, id), rr));
                    y += rh;
                }
                c.alpha = a0;
            }
        }
        // the channels and the conversations, under the header's title (no left column)
        let lo = self.an(ak(A_CH, 32), if st.pop == 3 { 1.0 } else { 0.0 }, 16.0, 0.0);
        if lo > 0.01 {
            let tabs = st.tabs();
            let peers: Vec<String> = st.conversations().into_iter().map(|p| p.0).take(6).collect();
            let rh = 34.0 * k;
            let w = 240.0 * k;
            let hgt = 10.0 * k + tabs.len() as f32 * rh + if peers.is_empty() { 0.0 } else { 26.0 * k + peers.len() as f32 * rh } + 6.0 * k;
            let at = st.chans_at;
            let r = [at[0], at[1] - (1.0 - lo) * 6.0 * k, at[0] + w, at[1] + hgt - (1.0 - lo) * 6.0 * k];
            over(r, lo);
            c.alpha = a0 * lo;
            c.shadow(r, 13.0 * k, 28.0 * k, grey(0.0, 0.55));
            c.rect(r, 13.0 * k, pop_bg);
            let mut y = r[1] + 5.0 * k;
            for tab in tabs {
                let rr = [r[0] + 5.0 * k, y, r[2] - 5.0 * k, y + rh];
                let on = st.tab == tab && !st.find && st.dm.is_none();
                let hov = inside(rr, mouse);
                if on || hov {
                    c.rect(rr, 8.0 * k, grey(1.0, if on { 0.09 } else { 0.05 }));
                }
                let mid = (rr[1] + rr[3]) / 2.0;
                let s = 15.0 * k;
                c.icon([rr[0] + 10.0 * k, mid - s / 2.0, rr[0] + 10.0 * k + s, mid + s / 2.0], tab.icon(), if on { look.base } else { fade(look.base, 0.75) });
                c.text(rr[0] + 34.0 * k, mid + 4.8 * k, 13.5 * k, Face::Medium, &st.tab_name(tab), if on || hov { look.base } else { mix(look.dim, look.base, 0.6) });
                hits.push((Hit::Pick(tab), rr));
                y += rh;
            }
            if !peers.is_empty() {
                c.text(r[0] + 15.0 * k, y + 18.0 * k, 11.5 * k, Face::Semi, tr("Conversations"), fade(look.dim, 0.65));
                y += 26.0 * k;
                for name in peers {
                    let rr = [r[0] + 5.0 * k, y, r[2] - 5.0 * k, y + rh];
                    let on = st.dm.as_deref().is_some_and(|d| d.eq_ignore_ascii_case(&name));
                    let hov = inside(rr, mouse);
                    if on || hov {
                        c.rect(rr, 8.0 * k, grey(1.0, if on { 0.09 } else { 0.05 }));
                    }
                    let mid = (rr[1] + rr[3]) / 2.0;
                    let fs = 20.0 * k;
                    let face = st.has_face(&name);
                    if face {
                        self.chat_face(st, &name, [rr[0] + 8.0 * k, mid - fs / 2.0, rr[0] + 8.0 * k + fs, mid + fs / 2.0], c.alpha);
                    }
                    c.text(rr[0] + if face { 36.0 * k } else { 12.0 * k }, mid + 4.8 * k, 13.5 * k, Face::Semi, &name, look.base);
                    hits.push((Hit::Peer(name), rr));
                    y += rh;
                }
            }
            c.alpha = a0;
        }
        // the hidden players, each to bring back
        let ho = self.an(ak(A_CH, 31), if st.pop == 2 { 1.0 } else { 0.0 }, 16.0, 0.0);
        if ho > 0.01 {
            let w = 300.0 * k;
            let rh = 38.0 * k;
            let n = st.hidden.len().max(1) as f32;
            let hgt = 50.0 * k + n.min(9.0) * rh + 10.0 * k;
            let r = [tools_x - w, win[1] + 50.0 * k - (1.0 - ho) * 6.0 * k, tools_x, win[1] + 50.0 * k + hgt - (1.0 - ho) * 6.0 * k];
            over(r, ho);
            c.alpha = a0 * ho;
            c.shadow(r, 13.0 * k, 28.0 * k, grey(0.0, 0.55));
            c.rect(r, 13.0 * k, pop_bg);
            c.text(r[0] + 16.0 * k, r[1] + 30.0 * k, 14.0 * k, Face::Semi, tr("Hidden players"), look.base);
            if st.hidden.is_empty() {
                c.text(r[0] + 16.0 * k, r[1] + 70.0 * k, 12.5 * k, Face::Regular, tr("Nobody is hidden"), fade(look.dim, 0.7));
            }
            let names = st.hidden.clone();
            for (i, name) in names.iter().take(9).enumerate() {
                let rr = [r[0] + 8.0 * k, r[1] + 46.0 * k + i as f32 * rh, r[2] - 8.0 * k, r[1] + 46.0 * k + (i + 1) as f32 * rh];
                let fs = 22.0 * k;
                let mid = (rr[1] + rr[3]) / 2.0;
                let face = st.has_face(name);
                if face {
                    self.chat_face(st, name, [rr[0] + 8.0 * k, mid - fs / 2.0, rr[0] + 8.0 * k + fs, mid + fs / 2.0], c.alpha);
                }
                c.text(rr[0] + if face { 38.0 * k } else { 10.0 * k }, mid + 4.6 * k, 13.0 * k, Face::Medium, name, look.base);
                let lab = tr("Bring back");
                let bw = fonts().width(Face::Medium, 11.5 * k, lab) + 20.0 * k;
                let br = [rr[2] - bw, mid - 12.0 * k, rr[2], mid + 12.0 * k];
                let bh = inside(br, mouse);
                c.rect(br, 7.0 * k, grey(1.0, if bh { 0.16 } else { 0.08 }));
                c.text_c((br[0] + br[2]) / 2.0, mid + 4.0 * k, 11.5 * k, Face::Medium, lab, look.base);
                hits.push((Hit::Unhide(name.clone()), br));
            }
            c.alpha = a0;
        }
        // the player's card (a name clicked)
        if let Some((name, at, since)) = st.card.clone() {
            let ka = ease_out(((t - since) / 0.2).clamp(0.0, 1.0));
            let w = 300.0 * k;
            let hgt = 196.0 * k;
            let cx = at[0].clamp(win[0] + 8.0 * k, (win[2] - w - 8.0 * k).max(win[0]));
            let mut cy = at[1] - hgt - 8.0 * k;
            if cy < win[1] + 8.0 * k {
                cy = at[1] + look.lh + 4.0 * k;
            }
            let r = [cx, cy + (1.0 - ka) * 6.0 * k, cx + w, cy + hgt + (1.0 - ka) * 6.0 * k];
            over(r, ka);
            c.alpha = a0 * ka;
            c.shadow(r, 15.0 * k, 34.0 * k, grey(0.0, 0.55));
            c.rect(r, 15.0 * k, pop_bg);
            // their face while they are in the game; else the name stands where it would be
            let face = st.has_face(&name);
            let fr = if face { [r[0] + 16.0 * k, r[1] + 16.0 * k, r[0] + 72.0 * k, r[1] + 72.0 * k] } else { [r[0] + 16.0 * k, r[1] + 16.0 * k, r[0] + 2.0 * k, r[1] + 72.0 * k] };
            if face {
                self.chat_face(st, &name, fr, c.alpha);
            }
            c.text(fr[2] + 14.0 * k, fr[1] + 26.0 * k, 18.0 * k, Face::Semi, &fonts().fit(Face::Semi, 18.0 * k, &name, r[2] - fr[2] - 30.0 * k), look.base);
            // what the chat knows of them: their rank and clan as last said, how much they said, since when
            let theirs: Vec<usize> = st.rows.iter().enumerate().filter(|(_, r)| r.who.eq_ignore_ascii_case(&name) && matches!(r.kind, Said::Chat(_))).map(|(i, _)| i).collect();
            let mut meta = Vec::new();
            if let Some(Said::Chat(cl)) = theirs.last().map(|&i| &st.rows[i].kind) {
                if let Some((w2, _, _)) = &cl.rank_word {
                    meta.push(w2.clone());
                }
                if let Some((clan, _)) = &cl.clan {
                    meta.push(clan.clone());
                }
                if let Some((ttl, _)) = &cl.title {
                    meta.push(ttl.clone());
                }
            }
            let you = name.eq_ignore_ascii_case(&st.me);
            if you {
                meta.insert(0, tr("you").to_string());
            }
            c.text(fr[2] + 14.0 * k, fr[1] + 46.0 * k, 12.0 * k, Face::Regular, &fonts().fit(Face::Regular, 12.0 * k, &meta.join(" · "), r[2] - fr[2] - 30.0 * k), fade(look.dim, 0.85));
            let sy = fr[3] + 14.0 * k;
            c.rect([r[0] + 16.0 * k, sy, r[2] - 16.0 * k, sy + 1.0], 0.0, grey(1.0, 0.07));
            let said: usize = theirs.iter().map(|&i| st.rows[i].count as usize).sum();
            let first = theirs.first().map(|&i| clock_text(st.rows[i].clock, false, true)).unwrap_or_else(|| "—".into());
            for (i, (v, l2)) in [(said.to_string(), tr("messages")), (first, tr("first seen"))].iter().enumerate() {
                let vx = r[0] + 16.0 * k + i as f32 * 130.0 * k;
                c.text(vx, sy + 24.0 * k, 15.5 * k, Face::Semi, v, look.base);
                c.text(vx, sy + 39.0 * k, 10.0 * k, Face::Mono, &l2.to_uppercase(), fade(look.dim, 0.7));
            }
            let hidden = st.is_hidden(&name);
            let labels: Vec<(u8, &str)> = if you { vec![(1, tr("Mention"))] } else { vec![(0, tr("Write")), (1, tr("Mention")), (2, tr("Teleport")), (3, if hidden { tr("Show") } else { tr("Hide") })] };
            let cols = labels.len().min(2);
            let bw = (w - 32.0 * k - 6.0 * k * (cols as f32 - 1.0)) / cols as f32;
            for (n, (which, lab)) in labels.iter().enumerate() {
                let bx = r[0] + 16.0 * k + (n % 2) as f32 * (bw + 6.0 * k);
                let by = sy + 52.0 * k + (n / 2) as f32 * 34.0 * k;
                let br = [bx, by, bx + bw, by + 28.0 * k];
                let bh = inside(br, mouse);
                let go = *which == 0;
                c.rect(br, 8.0 * k, if go { mix(fade(look.base, 0.9), look.base, bh as i32 as f32) } else { grey(1.0, if bh { 0.15 } else { 0.075 }) });
                c.text_c((br[0] + br[2]) / 2.0, by + 18.5 * k, 12.5 * k, if go { Face::Semi } else { Face::Medium }, lab, if go { grey(0.04, 1.0) } else { look.base });
                hits.push((Hit::Card(*which), br));
            }
            c.alpha = a0;
        }
    }

    /// The server's words for what is under the pointer.
    fn chat_tip(&mut self, c: &mut Canvas, look: &Look, tip: &str, at: [f32; 2], w: f32) {
        let k = look.k;
        let fs = 12.5 * k;
        let lines: Vec<&str> = tip.lines().filter(|l| !l.trim().is_empty()).take(8).collect();
        if lines.is_empty() {
            return;
        }
        let tw = lines.iter().map(|l| fonts().width(Face::Regular, fs, l)).fold(0.0, f32::max).min(420.0 * k) + 22.0 * k;
        let hgt = lines.len() as f32 * fs * 1.4 + 14.0 * k;
        let tx = at[0].min(M + w - tw).max(M);
        let r = [tx, at[1] - hgt - 6.0 * k, tx + tw, at[1] - 6.0 * k];
        over(r, 1.0);
        let gl = crate::gui::theme::pal().glass;
        c.shadow(r, 10.0 * k, 20.0 * k, grey(0.0, 0.45));
        c.rect(r, 10.0 * k, [gl[0].max(0.05), gl[1].max(0.05), gl[2].max(0.06), 1.0]);
        for (i, l) in lines.iter().enumerate() {
            let l = fonts().fit(Face::Regular, fs, l, tw - 22.0 * k);
            c.text(r[0] + 11.0 * k, r[1] + 7.0 * k + (i as f32 + 0.78) * fs * 1.4, fs, Face::Regular, &l, if i == 0 { look.base } else { look.dim });
        }
    }

    /// A word for a moment (copied, hidden — with a way back): in the open window over the field, in its middle
    /// (`at`: the middle, the bottom).
    fn chat_toast(&mut self, c: &mut Canvas, st: &mut Store, look: &Look, hits: &mut Vec<(Hit, [f32; 4])>, mouse: [f32; 2], at: [f32; 2]) {
        let k = look.k;
        let since = st.now - st.toast.1;
        if !(0.0..2.6).contains(&since) {
            return;
        }
        let kt = smooth(0.0, 0.16, since) * (1.0 - smooth(2.3, 2.6, since));
        let label = st.toast.0.clone();
        let fs = 13.0 * k;
        let undo = st.undo.clone();
        let back = tr("Bring back");
        let uw = if undo.is_some() { fonts().width(Face::Medium, 12.0 * k, back) + 22.0 * k } else { 0.0 };
        let tw = fonts().width(Face::Regular, fs, &label) + 30.0 * k + uw + if undo.is_some() { 10.0 * k } else { 0.0 };
        let bottom = at[1] + (1.0 - kt) * 6.0 * k;
        let r = [at[0] - tw / 2.0, bottom - 36.0 * k, at[0] + tw / 2.0, bottom];
        over(r, kt);
        let a0 = c.alpha;
        c.alpha = a0 * kt;
        let gl = crate::gui::theme::pal().glass;
        c.shadow(r, 11.0 * k, 22.0 * k, grey(0.0, 0.45));
        c.rect(r, 11.0 * k, [gl[0].max(0.05), gl[1].max(0.05), gl[2].max(0.06), 1.0]);
        let w = c.text(r[0] + 15.0 * k, (r[1] + r[3]) / 2.0 + fs * 0.36, fs, Face::Regular, &label, look.base);
        if let Some(p) = undo {
            let ur = [r[0] + 15.0 * k + w + 10.0 * k, r[1] + 7.0 * k, r[0] + 15.0 * k + w + 10.0 * k + uw, r[3] - 7.0 * k];
            c.rect(ur, 7.0 * k, grey(1.0, if inside(ur, mouse) { 0.16 } else { 0.08 }));
            c.text_c((ur[0] + ur[2]) / 2.0, (ur[1] + ur[3]) / 2.0 + 4.2 * k, 12.0 * k, Face::Medium, back, look.base);
            hits.push((Hit::Undo(p), ur));
        }
        c.alpha = a0;
    }

    /// After the chat is put in its place: the shift from its corner (its hits are in window px from it).
    pub(in super::super) fn chat_placed(&mut self, corner: [f32; 2]) {
        let mut st = store();
        st.shift = [corner[0] - M, corner[1] - M];
        let n = self.hud.icons.len();
        ICONS_AT.with(|a| {
            let v = a.get();
            a.set([v[0], v[1], n]);
        });
    }

    /// The end of the HUD's frame: with the chat open, what Java draws (faces, items, models: over the HUD's picture)
    /// is cut around what covers it here — the other elements' around the window, the window's rows around what is
    /// open over them. A cut icon goes as its parts: a clip (kind 99: x, y, w, h) before each copy of it.
    pub(in super::super) fn chat_cut_icons(&mut self) {
        let [from, pops, to] = ICONS_AT.with(|a| a.replace([usize::MAX; 3]));
        let (open, panel, shift) = {
            let s = store();
            (s.open, s.panel, s.shift)
        };
        if !open || from == usize::MAX || to == usize::MAX || panel[2] <= panel[0] {
            return;
        }
        let at = |r: [f32; 4]| [r[0] + shift[0], r[1] + shift[1], r[2] + shift[0], r[3] + shift[1]];
        let over: Vec<[f32; 4]> = OVER.with(|o| o.borrow().iter().map(|r| at(*r)).collect());
        let mut around_window = vec![at(panel)];
        around_window.extend_from_slice(&over);
        let icons = std::mem::take(&mut self.hud.icons);
        let mut out = Vec::with_capacity(icons.len() + 8);
        for (i, ic) in icons.into_iter().enumerate() {
            let covers: &[[f32; 4]] = if i < from || i >= to {
                &around_window
            } else if i < pops {
                &over
            } else {
                &[]
            };
            match cut_rect([ic[2], ic[3], ic[2] + ic[4], ic[3] + ic[5]], covers) {
                None => out.push(ic),
                Some(parts) => {
                    for p in parts {
                        out.push([99.0, 0.0, p[0], p[1], p[2] - p[0], p[3] - p[1], 1.0]);
                        out.push(ic);
                    }
                }
            }
        }
        self.hud.icons = out;
    }

    /// The pointer pressed while the chat is open: true when it was the chat's.
    pub(in super::super) fn chat_press(&mut self, x: f32, y: f32, button: i32) -> bool {
        // a press on the element's settings (or a list or the colour wheel open from them) is theirs
        if self.pick.is_some() || self.pop.is_some() || self.hud.pop.is_some() && inside(self.hud.pop_rect, [x, y]) {
            return false;
        }
        // (one in the chat: what is typed is the chat's again, not the settings' search or a text of theirs)
        self.hud.pop_find = false;
        self.text_focus = None;
        let mut st = store();
        if !st.open {
            return false;
        }
        let p = [x - st.shift[0], y - st.shift[1]];
        let hit = st.hits.iter().rev().find(|(_, r)| inside(*r, p)).map(|(h, _)| h.clone());
        let on_panel = inside(st.panel, p);
        let now = st.now;
        // a press anywhere else closes a card or a list
        let keep_card = matches!(hit, Some(Hit::Card(_)) | Some(Hit::Nick(_)));
        let keep_pop = matches!(hit, Some(Hit::Unhide(_)) | Some(Hit::Chans));
        let column_tab = matches!(hit, Some(Hit::Tab(_)));
        let was_pop = st.card.is_some() || st.pop != 0 || st.menu.is_some();
        // a message's menu goes with any press (what it held is done below)
        st.menu = None;
        if !keep_card {
            st.card = None;
        }
        if !keep_pop {
            st.pop = 0;
        }
        let Some(hit) = hit else { return on_panel || was_pop };
        let toast = |st: &mut Store, w: &str| {
            st.toast = (w.to_string(), now);
            st.undo = None;
        };
        let mut sound = 0.0f32;
        // the right button: on a message (or what it holds) its menu by the pointer; anywhere else in the window the
        // chat's settings by it
        if button == 1 {
            let row = match &hit {
                Hit::Row(id) | Hit::Act(_, id) => Some(*id),
                Hit::Tap(_) | Hit::Nick(_) => st.hits.iter().rev().find(|(h, r)| matches!(h, Hit::Row(_)) && inside(*r, p)).and_then(|(h, _)| if let Hit::Row(id) = h { Some(*id) } else { None }),
                _ => None,
            };
            if let Some(id) = row {
                st.menu = Some((id, p, now));
                st.card = None;
                drop(st);
                self.sound_tick(0.4);
                return true;
            }
            if on_panel {
                drop(st);
                self.chat_settings();
                return true;
            }
            return false;
        }
        match hit {
            Hit::Panel | Hit::Row(_) => {}
            Hit::Tab(tab) | Hit::Pick(tab) => {
                // (a channel of the column held: dragged, it moves)
                if column_tab && st.tabs_at[3] > 1.0 {
                    st.tab_drag = Some((tab, p[1], false));
                }
                if st.tab != tab || st.dm.is_some() {
                    sound = 0.5;
                }
                let was = field_key(&st);
                st.tab = tab;
                st.dm = None;
                st.scroll_t = 0.0;
                st.unread = 0;
                if st.find {
                    self.outbox.push("chat\tfind".to_string());
                }
                if tab == Tab::Private {
                    st.seen[0] = st.priv_total;
                    st.dm = st.conversations().first().map(|p| p.0.clone());
                }
                if tab == Tab::Mentions {
                    st.seen[1] = st.ment_total;
                }
                swap_draft(&mut st, was, &mut self.outbox);
            }
            Hit::Peer(name) => {
                let was = field_key(&st);
                st.tab = Tab::Private;
                st.dm = Some(name.clone());
                st.scroll_t = 0.0;
                st.unread = 0;
                st.seen[0] = st.priv_total;
                if st.find {
                    self.outbox.push("chat\tfind".to_string());
                }
                swap_draft(&mut st, was, &mut self.outbox);
                sound = 0.5;
            }
            Hit::Tool(0) => {
                self.outbox.push("chat\tfind".to_string());
                sound = 0.5;
            }
            Hit::Tool(_) => {
                drop(st);
                self.chat_settings();
                return true;
            }
            Hit::Chans => {
                st.pop = if st.pop == 3 { 0 } else { 3 };
                sound = 0.5;
            }
            Hit::Unhide(name) => {
                st.unhide(&name);
                toast(&mut st, &format!("{} {name} {}", tr("Messages of"), tr("are shown again")));
                sound = 0.4;
            }
            Hit::Tap(Tap::Game(id, run)) => self.outbox.push(format!("chat\tclick\t{id}\t{run}\t{button}")),
            Hit::Tap(Tap::Field(w)) => {
                if !w.is_empty() {
                    self.outbox.push(format!("chat\tset\t{w}"));
                }
            }
            Hit::Tap(Tap::Copy(w, said)) => {
                self.outbox.push(format!("copy\t{w}"));
                toast(&mut st, tr(said));
                sound = 0.4;
            }
            Hit::Nick(name) => {
                let at = st.hits.iter().rev().find(|(h, r)| matches!(h, Hit::Nick(n) if *n == name) && inside(*r, p)).map_or(p, |(_, r)| [r[0], r[1]]);
                st.card = if st.card.as_ref().is_some_and(|c| c.0 == name) { None } else { Some((name, at, now)) };
                sound = 0.4;
            }
            Hit::Card(i) => {
                if let Some((name, _, _)) = st.card.take() {
                    match i {
                        0 => {
                            let was = field_key(&st);
                            st.tab = Tab::Private;
                            st.dm = Some(name.clone());
                            st.scroll_t = 0.0;
                            st.unread = 0;
                            if st.find {
                                self.outbox.push("chat\tfind".to_string());
                            }
                            swap_draft(&mut st, was, &mut self.outbox);
                        }
                        1 => self.outbox.push(format!("chat\tinsert\t{name}, ")),
                        2 => self.outbox.push(format!("chat\tsend\t/tpa {name}")),
                        _ => {
                            if st.is_hidden(&name) {
                                st.unhide(&name);
                                toast(&mut st, &format!("{} {name} {}", tr("Messages of"), tr("are shown again")));
                            } else if st.hide(&name) {
                                st.toast = (format!("{} {name} {}", tr("Messages of"), tr("hidden")), now);
                                st.undo = Some(name);
                            }
                        }
                    }
                    sound = 0.45;
                }
            }
            Hit::Act(i, id) => {
                let r = st.row_by_id(id).map(|r| (r.who.clone(), r.plain.clone()));
                if let Some((who, plain)) = r {
                    match i {
                        0 => self.outbox.push(format!("chat\tset\t/msg {who} ")),
                        1 => self.outbox.push(format!("chat\tinsert\t{who}, ")),
                        2 => {
                            self.outbox.push(format!("copy\t{}", plain.replace('\n', " ")));
                            toast(&mut st, tr("Message copied"));
                        }
                        4 => {
                            self.outbox.push(format!("copy\t{who}"));
                            toast(&mut st, tr("Name copied"));
                        }
                        5 => {
                            drop(st);
                            self.chat_settings();
                            return true;
                        }
                        _ => {
                            if st.hide(&who) {
                                st.toast = (format!("{} {who} {}", tr("Messages of"), tr("hidden")), now);
                                st.undo = Some(who);
                            }
                        }
                    }
                    sound = 0.4;
                }
            }
            Hit::Undo(name) => {
                st.unhide(&name);
                st.toast.1 = -9.0;
                st.undo = None;
                sound = 0.4;
            }
            Hit::Btn(id, which) => {
                if let Some(r) = st.row_by_id(id) {
                    if r.state == 0 {
                        if let Said::Request { yes, no, .. } = &r.kind {
                            let cmd = if which == 0 { Some(yes.clone()) } else { no.clone() };
                            match cmd {
                                Some(super::parse::Cmd::Click(mid, run)) => self.outbox.push(format!("chat\tclick\t{mid}\t{run}\t0")),
                                Some(super::parse::Cmd::Send(cm)) => self.outbox.push(format!("chat\tsend\t{cm}")),
                                None => {}
                            }
                            r.state = if which == 0 { 1 } else { 2 };
                            r.bump = now;
                            r.version += 1;
                        }
                    }
                }
                sound = 0.5;
            }
            Hit::Sugg(i) => self.outbox.push(format!("chat\tsugg\t{i}")),
            Hit::Pill => {
                st.scroll_t = 0.0;
                st.unread = 0;
            }
            Hit::Chan => {
                let f = st.field.clone();
                let next = match f.strip_prefix('!') {
                    Some(rest) => rest.to_string(),
                    None => format!("!{f}"),
                };
                self.outbox.push(format!("chat\tset\t{next}"));
                sound = 0.4;
            }
            Hit::Grip => {
                let mi = self.mod_index("chat");
                let set = &self.mods[mi].set;
                st.grip = Some(([x, y], [theme::num(set, "Window width"), theme::num(set, "Window height")]));
            }
            Hit::Field => {
                // the caret where the field was pressed
                let k = self.hud.u * (theme::num(&self.mods[self.mod_index("chat")].set, "Scale") / 100.0).max(0.1);
                let fs = 15.0 * k;
                let r = st.hits.iter().rev().find(|(h, _)| *h == Hit::Field).map(|q| q.1).unwrap_or([0.0; 4]);
                let chars: Vec<char> = st.field.chars().collect();
                let rel = p[0] - r[0] - 4.0 * k + st.field_off;
                let mut best = chars.len();
                for n in 0..=chars.len() {
                    let s: String = chars.iter().take(n).collect();
                    if fonts().width(Face::Regular, fs, &s) >= rel {
                        let prev: String = chars.iter().take(n.saturating_sub(1)).collect();
                        best = if n > 0 && rel - fonts().width(Face::Regular, fs, &prev) < fonts().width(Face::Regular, fs, &s) - rel { n - 1 } else { n };
                        break;
                    }
                }
                self.outbox.push(format!("chat\tcursor\t{best}"));
            }
        }
        drop(st);
        if sound > 0.0 {
            self.sound_tick(sound);
        }
        true
    }

    /// The world behind the open window, as set: darkened, or blurred under a little shade (under the whole HUD: its
    /// elements stay as they are over it).
    pub(in super::super) fn chat_behind(&mut self, c: &mut Canvas, w: f32, h: f32) {
        let (kind, k, at) = store().behind;
        // (said by the chat drawn a moment ago: an element switched off or gone says nothing more)
        if k <= 0.003 || (self.now - at).abs() > 0.25 {
            return;
        }
        match kind {
            1 => c.rect([0.0, 0.0, w, h], 0.0, grey(0.0, 0.75 * k)),
            2 => c.glass([0.0, 0.0, w, h], 0.0, [0.0, 0.0, 0.0, 0.3 * k], k, 0.0, 0.0),
            _ => {}
        }
    }

    /// The chat's settings by its window (the element's own, as the editor opens them; closed again when open).
    fn chat_settings(&mut self) {
        let i = self.mod_index("chat");
        self.open_pop(i);
    }

    /// The pointer let go (the window's corner).
    pub(in super::super) fn chat_release(&mut self) {
        let mut st = store();
        if st.grip.take().is_some() {
            self.hud.dirty = true;
        }
        // a channel let go: where it was dragged to is kept
        if let Some((_, _, true)) = st.tab_drag.take() {
            st.save_order();
        }
    }

    /// The pointer moving while the window's corner is held: its size follows (the chat's own px, on its steps).
    pub(in super::super) fn chat_drag(&mut self, x: f32, y: f32) {
        let mut st = store();
        // a channel held: once moved a little, it goes where the pointer is among the others
        if let Some((tab, y0, moved)) = st.tab_drag {
            let py = y - st.shift[1];
            let [_, top, _, th] = st.tabs_at;
            if moved || (py - y0).abs() > th * 0.3 {
                st.tab_drag = Some((tab, y0, true));
                let to = ((py - top) / th.max(1.0)).floor().max(0.0) as usize;
                st.move_tab(tab, to);
            }
            return;
        }
        let Some((m0, wh0)) = st.grip else { return };
        drop(st);
        let mi = self.mod_index("chat");
        let k = self.hud.u * (theme::num(&self.mods[mi].set, "Scale") / 100.0).max(0.1);
        let anchor = self.hud.places.get("chat").map_or(6, |p| p.0);
        let sx = if anchor % 3 == 2 { -1.0 } else { 1.0 };
        let sy = if anchor / 3 == 0 { 1.0 } else { -1.0 };
        let (nw, nh) = (wh0[0] + sx * (x - m0[0]) / k, wh0[1] + sy * (y - m0[1]) / k);
        for s in self.mods[mi].set.iter_mut() {
            let want = match s.label {
                "Window width" => nw,
                "Window height" => nh,
                _ => continue,
            };
            if let Kind::Slider { min, max, step, v, .. } = &mut s.kind {
                *v = ((want / *step).round() * *step).clamp(*min, *max);
            }
        }
    }

    /// The wheel over the open chat: its rows scroll.
    pub(in super::super) fn chat_scroll(&mut self, dy: f32) -> bool {
        let mut st = store();
        if !st.open {
            return false;
        }
        let p = [st.mouse[0] - st.shift[0], st.mouse[1] - st.shift[1]];
        if !inside(st.panel, p) {
            return false;
        }
        let k = self.hud.u;
        st.scroll_t = (st.scroll_t + dy * 66.0 * k).max(0.0);
        st.bar_at = st.now;
        true
    }
}

/// Where the chat's own px go: a box `r` laid `a` of the way from `from`.
#[allow(dead_code)]
fn lerp_box(from: [f32; 4], r: [f32; 4], a: f32) -> [f32; 4] {
    mixr(from, r, a)
}
