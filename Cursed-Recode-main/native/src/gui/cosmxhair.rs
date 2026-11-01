//! The Cosmetics page's crosshair (CS's, over the picture with a CS knife in the hands: cosmetics/crosshair.rs): at the
//! left a stage with it in its middle — over a dark room, a light wall, the sky or a field of grass (how it reads over
//! each), as large as in the game or two or four times (each of its pixels a square), a dynamic one breathing as a run
//! opens it; at the right its look (the styles as cards, each with its own small crosshair), its sizes, its dot, T and
//! outline as switches, its colour (the client's own picker: hue, brightness, transparency, hex, Chroma, the ready ones —
//! CS's own lamps under it), its code to copy or paste (CS2's, or CS:GO's old one), CS2's own back. Each change kept at
//! once: the loadout's code, its Chroma, the stage's background and size.

use super::data::{Col, Kind};
use super::draw::{grey, mix, Canvas, Rgba};
use super::i18n::tr;
use super::text::{fonts, Face};
use super::theme::pal;
use super::{ak, Gui, Hit, Owner, A_HOV};
use crate::cosmetics::crosshair::{self, Crosshair};
use crate::cosmetics::loadout;

/// The editor's parts (Hit::Cosm).
pub(super) const XCOPY: u8 = 13;
pub(super) const XPASTE: u8 = 14;
pub(super) const XSTYLE: u8 = 15;
pub(super) const XSLIDE: u8 = 16;
pub(super) const XTOGGLE: u8 = 17;
pub(super) const XCOLOR: u8 = 18;
pub(super) const XRESET: u8 = 19;
/// The colour's row (the picker opens by it), the stage's background's chips, its size's.
pub(super) const XPICK: u8 = 45;
pub(super) const XBG: u8 = 46;
pub(super) const XZOOM: u8 = 47;

/// The styles offered (CS2's numbers).
const STYLES: [(&str, u8); 4] = [("Static", 4), ("Dynamic", 0), ("Classic", 2), ("Dot", 6)];
/// CS's colours (cl_crosshaircolor's green, yellow, cyan, red, blue) and white.
const COLOURS: [[u8; 3]; 6] = [[50, 250, 50], [250, 250, 50], [50, 250, 250], [250, 50, 50], [50, 50, 250], [255, 255, 255]];
/// The sliders: what, from, to (pixels at 1080; the opacity 0..255).
const SLIDERS: [(&str, f32, f32); 4] = [("Length", 0.0, 40.0), ("Thickness", 1.0, 12.0), ("Gap", -6.0, 20.0), ("Opacity", 0.0, 255.0)];
const TOGGLES: [&str; 3] = ["Dot", "T-shape", "Outline"];
/// The stage's backgrounds, its sizes.
const BACKGROUNDS: [&str; 4] = ["Dark", "Wall", "Sky", "Grass"];
const ZOOMS: [(&str, f32); 3] = [("1×", 1.0), ("2×", 2.0), ("4×", 4.0)];

/// The crosshair as set (in pixels: the old code's made so).
fn current() -> Crosshair {
    crosshair::decode(&loadout::current().0.crosshair).unwrap_or_default().to_pixels()
}

fn keep(x: &Crosshair) {
    let code = crosshair::encode(x);
    loadout::change(|l| l.crosshair = code);
}

fn slider_value(x: &Crosshair, n: usize) -> f32 {
    match n {
        0 => x.length,
        1 => x.thickness,
        2 => x.gap,
        _ => x.color[3] as f32,
    }
}

fn rgba(c: [u8; 4]) -> Rgba {
    c.map(|v| v as f32 / 255.0)
}

fn rgb_u32(c: [u8; 4]) -> u32 {
    (c[0] as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32
}

/// A box filled from its top down, `col` at each height's share (0 its top, 1 its foot): its rounded corners each
/// half of a rounded box, bands between them on whole pixels (none over another: it can cross over by its alpha).
fn gradient(c: &mut Canvas, b: [f32; 4], radius: f32, col: impl Fn(f32) -> Rgba) {
    let h = (b[3] - b[1]).max(1.0);
    let rr = radius.min(h * 0.5);
    c.push_clip([b[0], b[1], b[2], b[1] + rr]);
    c.rect([b[0], b[1], b[2], b[1] + 2.0 * rr], rr, col(rr * 0.5 / h));
    c.pop_clip();
    c.push_clip([b[0], b[3] - rr, b[2], b[3]]);
    c.rect([b[0], b[3] - 2.0 * rr, b[2], b[3]], rr, col(1.0 - rr * 0.5 / h));
    c.pop_clip();
    let (y0, y1) = (b[1] + rr, b[3] - rr);
    let n = ((y1 - y0) / 6.0).ceil().clamp(1.0, 96.0) as usize;
    for i in 0..n {
        let ya = (y0 + (y1 - y0) * i as f32 / n as f32).round();
        let yb = (y0 + (y1 - y0) * (i + 1) as f32 / n as f32).round();
        c.rect([b[0], ya, b[2], yb], 0.0, col(((ya + yb) * 0.5 - b[1]) / h));
    }
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

impl Gui {
    /// The editor in `body` (under the page's chips): the stage at the left, the panel at the right.
    pub(super) fn cosm_crosshair(&mut self, c: &mut Canvas, body: [f32; 4]) {
        let r = self.r;
        self.xhair_colour_follow();
        let x = current();
        let panel_w = (18.5 * r).min((body[2] - body[0]) * 0.42);
        let view = [body[0], body[1], body[2] - panel_w - 0.8 * r, body[3]];
        self.xhair_stage(c, view, &x);
        self.xhair_panel(c, [view[2] + 0.8 * r, view[1], body[2], view[3]], &x);
    }

    /// One of the stage's backgrounds (`k`) over all of it, `a` of it: the dark room (a lamp's light on the wall), a
    /// light wall, the day's sky over the ground, a field of grass under the sky.
    fn xhair_backdrop(&mut self, c: &mut Canvas, view: [f32; 4], k: usize, a: f32) {
        let r = self.r;
        let (w, h) = (view[2] - view[0], view[3] - view[1]);
        let with = |v: [f32; 3]| [v[0], v[1], v[2], a];
        match k {
            0 => {
                c.rect(view, r, grey(0.012, 0.93 * a));
                c.glow([(view[0] + view[2]) * 0.5, view[1] + 0.36 * h], 0.62 * w, grey(1.0, 0.07 * a));
                c.glow([(view[0] + view[2]) * 0.5, view[1] + 0.4 * h], 0.3 * w, grey(1.0, 0.045 * a));
            }
            1 => gradient(c, view, r, |t| with(lerp3([0.74, 0.74, 0.73], [0.55, 0.55, 0.54], t))),
            2 => gradient(c, view, r, |t| {
                // the sky deeper overhead, pale at the horizon (two thirds down), the ground under it
                if t < 0.66 {
                    with(lerp3([0.27, 0.48, 0.86], [0.74, 0.84, 0.95], (t / 0.66).powf(1.4)))
                } else {
                    with(lerp3([0.42, 0.37, 0.3], [0.22, 0.19, 0.15], (t - 0.66) / 0.34))
                }
            }),
            _ => {
                gradient(c, view, r, |t| {
                    if t < 0.48 {
                        with(lerp3([0.46, 0.64, 0.95], [0.72, 0.84, 0.98], t / 0.48))
                    } else {
                        with(lerp3([0.42, 0.62, 0.24], [0.27, 0.43, 0.15], (t - 0.48) / 0.52))
                    }
                });
                // the grass's blocks: faint lines between them, nearer the larger
                c.push_clip([view[0], view[1] + 0.48 * h, view[2], view[3] - r]);
                let mut y = view[1] + 0.48 * h;
                let mut step = 0.025 * h;
                while y < view[3] {
                    c.rect([view[0], y, view[2], y + 1.0], 0.0, [0.1, 0.2, 0.05, 0.25 * a]);
                    y += step;
                    step *= 1.35;
                }
                c.pop_clip();
            }
        }
    }

    /// The stage: its background (each crossing over as it is chosen), the crosshair in its middle as large as chosen
    /// (its colour's light behind it on the dark; a dynamic one breathing), its chips at the top, at its true size in a
    /// corner while it is shown larger.
    fn xhair_stage(&mut self, c: &mut Canvas, view: [f32; 4], x: &Crosshair) {
        let r = self.r;
        let h = view[3] - view[1];
        let lo = loadout::current().0;
        let (bg, zoom) = (lo.xh_stage.0.min(3) as usize, lo.xh_stage.1.min(2) as usize);
        c.push_clip(view);
        for k in 0..BACKGROUNDS.len() {
            let a = self.an(ak(A_HOV, 33_600 + k), if bg == k { 1.0 } else { 0.0 }, 8.0, (bg == k) as i32 as f32);
            if a > 0.004 {
                self.xhair_backdrop(c, view, k, a);
            }
        }
        let dark = self.an(ak(A_HOV, 33_610), if bg == 0 { 1.0 } else { 0.0 }, 8.0, (bg == 0) as i32 as f32);
        // the crosshair as shown (its colour as Chroma has it now)
        let mut x = *x;
        if lo.crosshair_chroma > 0.0 {
            let [cr, cg, cb] = crate::cosmetics::overlay::running([x.color[0], x.color[1], x.color[2]], lo.crosshair_chroma);
            x.color = [cr, cg, cb, x.color[3]];
        }
        let m = [((view[0] + view[2]) * 0.5).floor(), (view[1] + h * 0.52).floor()];
        // a dynamic one opens as a run would and closes again, slowly
        let dynamic = crosshair::dynamic(x.style);
        let spread = if dynamic && pal().anim { 0.5 - 0.5 * (self.now * 1.4).cos() } else { 0.0 };
        // as the game's picture has it, the chosen times larger (each pixel a square): eased as it changes
        let mag = self.an(ak(A_HOV, 33_620), ZOOMS[zoom].1, 10.0, ZOOMS[zoom].1);
        let mag = if (mag - mag.round()).abs() < 0.02 { mag.round() } else { mag };
        let bars = crosshair::bars(&x, [0.0, 0.0], self.h, spread);
        let reach = bars.iter().flat_map(|(b, _)| b.iter().map(|v| v.abs())).fold(4.0f32, f32::max);
        let col = rgba(x.color);
        if dark > 0.01 {
            c.glow(m, (reach * mag * 1.6).max(4.0 * r), [col[0], col[1], col[2], 0.12 * col[3].max(0.3) * dark]);
        }
        for (b, bc) in &bars {
            c.rect([m[0] + b[0] * mag, m[1] + b[1] * mag, m[0] + b[2] * mag, m[1] + b[3] * mag], 0.0, rgba(*bc));
        }
        // its true size in the corner, while it is shown larger
        let inset_k = self.an(ak(A_HOV, 33_630), if zoom > 0 { 1.0 } else { 0.0 }, 10.0, (zoom > 0) as i32 as f32);
        if inset_k > 0.01 {
            let side = (2.0 * reach + 1.6 * r).max(3.4 * r);
            let inset = [view[0] + 0.9 * r, view[3] - 0.9 * r - side, view[0] + 0.9 * r + side, view[3] - 0.9 * r];
            let alpha = c.alpha;
            c.alpha *= inset_k;
            c.rect(inset, 0.6 * r, grey(0.0, 0.32));
            c.rect_line(inset, 0.6 * r, 1.0, grey(1.0, 0.1));
            let im = [((inset[0] + inset[2]) * 0.5).floor(), ((inset[1] + inset[3]) * 0.5).floor()];
            c.push_clip(inset);
            for (b, bc) in crosshair::bars(&x, im, self.h, spread) {
                c.rect(b, 0.0, rgba(bc));
            }
            c.pop_clip();
            c.text(inset[2] + 0.7 * r, im[1] + 0.25 * r, 0.62 * r, Face::Regular, tr("1:1 — as in the game"), mix(grey(0.1, 0.9), pal().t3, dark));
            c.alpha = alpha;
        }
        if dynamic {
            c.text_r(view[2] - 1.0 * r, view[3] - 1.0 * r, 0.62 * r, Face::Regular, tr("Dynamic: opens as you run"), mix(grey(0.1, 0.8), pal().t4, dark));
        }
        // the chips: its background at the top left, its size at the top right (dark under: the stage may be light)
        let chips = |names: Vec<String>, right: bool, at: f32| -> Vec<[f32; 4]> {
            let mut out = Vec::new();
            let mut cx = at;
            for name in &names {
                let cw = fonts().width(Face::Medium, 0.62 * r, name) + 1.1 * r;
                let b = if right { [cx - cw, view[1] + 0.6 * r, cx, view[1] + 2.0 * r] } else { [cx, view[1] + 0.6 * r, cx + cw, view[1] + 2.0 * r] };
                cx = if right { b[0] - 0.3 * r } else { b[2] + 0.3 * r };
                out.push(b);
            }
            if right {
                out.reverse();
            }
            out
        };
        let bg_names: Vec<String> = BACKGROUNDS.iter().map(|n| tr(n).to_string()).collect();
        let zoom_names: Vec<String> = ZOOMS.iter().map(|z| z.0.to_string()).rev().collect();
        let bg_rects = chips(bg_names.clone(), false, view[0] + 0.6 * r);
        let zoom_rects = chips(zoom_names, true, view[2] - 0.6 * r);
        for (part, rects, names, on, anim) in [(XBG, &bg_rects, bg_names, bg, 33_640), (XZOOM, &zoom_rects, ZOOMS.iter().map(|z| z.0.to_string()).collect(), zoom, 33_660)] {
            for (k, b) in rects.iter().enumerate() {
                let hov = self.an(ak(A_HOV, anim + k), if self.hover == Hit::Cosm(part, k as u16) { 1.0 } else { 0.0 }, 14.0, 0.0);
                c.rect(*b, 0.7 * r, mix(grey(0.05, 0.78), grey(0.14, 0.86), hov));
            }
            self.cosm_pill(c, rects, on, anim + 10, 0.7 * r, grey(0.28, 0.95));
            for (k, (name, b)) in names.iter().zip(rects.iter()).enumerate() {
                let hit = Hit::Cosm(part, k as u16);
                let hv = self.an(ak(A_HOV, anim + 20 + k), if on == k { 1.0 } else if self.hover == hit { 0.5 } else { 0.0 }, 14.0, 0.0);
                c.text_c((b[0] + b[2]) * 0.5, b[3] - 0.45 * r, 0.62 * r, Face::Medium, name, mix(grey(0.68, 1.0), grey(1.0, 1.0), hv));
                self.hits.push((*b, hit));
            }
        }
        c.pop_clip();
    }

    /// The panel: its look, its sizes, its dot, T and outline, its colour, its code, CS2's own.
    fn xhair_panel(&mut self, c: &mut Canvas, pr: [f32; 4], x: &Crosshair) {
        let r = self.r;
        c.rect(pr, r, grey(1.0, 0.04));
        let (x0, x1) = (pr[0] + 1.1 * r, pr[2] - 1.1 * r);
        let mut y = pr[1] + 1.9 * r;
        // its look: the styles as cards, each with its own small crosshair (the chosen one's light slides to it)
        c.text(x0, y, 0.8 * r, Face::Semi, tr("Look"), pal().t1);
        y += 0.7 * r;
        let gap = 0.35 * r;
        let cw = (x1 - x0 - 3.0 * gap) / 4.0;
        let rects: Vec<[f32; 4]> = (0..STYLES.len()).map(|k| [x0 + k as f32 * (cw + gap), y, x0 + k as f32 * (cw + gap) + cw, y + 3.8 * r]).collect();
        for (k, b) in rects.iter().enumerate() {
            let hov = self.an(ak(A_HOV, 33_120 + k), if self.hover == Hit::Cosm(XSTYLE, k as u16) { 1.0 } else { 0.0 }, 14.0, 0.0);
            c.rect(*b, 0.6 * r, mix(pal().f1, pal().f2, hov));
        }
        let on_style = STYLES.iter().position(|s| s.1 == x.style).unwrap_or(0);
        self.cosm_pill(c, &rects, on_style, 36_030, 0.6 * r, pal().f3);
        for (k, (label, style)) in STYLES.iter().enumerate() {
            let b = rects[k];
            let hit = Hit::Cosm(XSTYLE, k as u16);
            let hv = self.an(ak(A_HOV, 33_100 + k), if k == on_style { 1.0 } else if self.hover == hit { 0.45 } else { 0.0 }, 14.0, 0.0);
            let ink = mix(pal().t3, pal().t1, hv);
            // its small crosshair
            let sample = Crosshair { style: *style, length: 5.0, thickness: 1.0, gap: 2.0, dot: *style == 6, t_style: false, outline_mode: 0, ..*x };
            let mm = [((b[0] + b[2]) * 0.5).floor(), b[1] + 1.5 * r];
            let k2 = (0.11 * r).max(1.0).round();
            for (sb, _) in crosshair::bars(&sample, [0.0, 0.0], 1080.0, 0.0) {
                c.rect([mm[0] + sb[0] * k2, mm[1] + sb[1] * k2, mm[0] + sb[2] * k2, mm[1] + sb[3] * k2], 0.0, ink);
            }
            let size = (0.56 * r).min(0.56 * r * (cw - 0.4 * r) / fonts().width(Face::Medium, 0.56 * r, tr(label)).max(1.0));
            c.text_c((b[0] + b[2]) * 0.5, b[3] - 0.6 * r, size, Face::Medium, tr(label), ink);
            self.hits.push((b, hit));
        }
        y += 6.0 * r;
        // its sizes, two by two
        c.text(x0, y, 0.8 * r, Face::Semi, tr("Size"), pal().t1);
        y += 1.1 * r;
        let sw = (x1 - x0 - 1.0 * r) / 2.0;
        for (k, (label, lo, hi)) in SLIDERS.iter().enumerate() {
            let (col, row) = ((k % 2) as f32, (k / 2) as f32);
            let (a, b) = (x0 + col * (sw + 1.0 * r), x0 + col * (sw + 1.0 * r) + sw);
            let yy = y + row * 2.5 * r;
            let v = slider_value(x, k);
            c.text(a, yy, 0.66 * r, Face::Medium, tr(label), pal().t3);
            let shown = if k == 3 { format!("{}%", (v / 2.55).round() as i32) } else { format!("{}", v.round() as i32) };
            c.text_r(b, yy, 0.66 * r, Face::Mono, &shown, pal().t2);
            self.cosm.x_tracks[k] = [a, b];
            self.cosm_slider(c, [a, yy + 0.3 * r, b, yy + 1.6 * r], (v - lo) / (hi - lo), Hit::Cosm(XSLIDE, k as u16), 33_200 + k);
        }
        y += 5.4 * r;
        // its dot, T, outline: switches, side by side
        c.text(x0, y, 0.8 * r, Face::Semi, tr("Details"), pal().t1);
        y += 0.7 * r;
        let on = [x.dot, x.t_style, x.outline_mode > 0];
        let tw = (x1 - x0 - 2.0 * gap) / 3.0;
        for (k, label) in TOGGLES.iter().enumerate() {
            let b = [x0 + k as f32 * (tw + gap), y, x0 + k as f32 * (tw + gap) + tw, y + 2.0 * r];
            let hit = Hit::Cosm(XTOGGLE, k as u16);
            let hv = self.an(ak(A_HOV, 33_300 + k), if self.hover == hit { 1.0 } else { 0.0 }, 14.0, 0.0);
            let kv = self.an(ak(A_HOV, 33_310 + k), if on[k] { 1.0 } else { 0.0 }, 14.0, on[k] as i32 as f32);
            c.rect(b, 0.6 * r, mix(pal().f1, pal().f2, hv));
            let sr = [b[0] + 0.45 * r, b[1] + 0.68 * r, b[0] + 0.45 * r + 1.1 * r, b[3] - 0.68 * r];
            super::switch(c, sr, kv, hv, true);
            let tx = sr[2] + 0.35 * r;
            let size = (0.6 * r).min(0.6 * r * (b[2] - tx - 0.3 * r) / fonts().width(Face::Medium, 0.6 * r, tr(label)).max(1.0));
            c.text(tx, (b[1] + b[3]) * 0.5 + 0.22 * r, size, Face::Medium, tr(label), mix(pal().t3, pal().t1, kv.max(hv * 0.5)));
            self.hits.push((b, hit));
        }
        y += 3.6 * r;
        // its colour: a row as the menu's colour rows are (the client's own picker opens by it), CS's lamps under it
        c.text(x0, y, 0.8 * r, Face::Semi, tr("Colour"), pal().t1);
        y += 0.7 * r;
        let b = [x0, y, x1, y + 1.9 * r];
        let hit = Hit::Cosm(XPICK, 0);
        let open = self.pick.as_ref().is_some_and(|p| p.owner == Owner::Cosm && p.set == 1);
        let hv = self.an(ak(A_HOV, 33_700), if open { 1.0 } else if self.hover == hit { 0.5 } else { 0.0 }, 14.0, 0.0);
        c.rect(b, 0.55 * r, mix(pal().f1, pal().f3, hv));
        let mid = (b[1] + b[3]) * 0.5 + 0.25 * r;
        c.text(b[0] + 0.6 * r, mid, 0.68 * r, Face::Medium, tr("Crosshair colour"), mix(pal().t3, pal().t1, hv));
        let chroma = loadout::current().0.crosshair_chroma;
        let now = if chroma > 0.0 { crate::cosmetics::overlay::running([x.color[0], x.color[1], x.color[2]], chroma) } else { [x.color[0], x.color[1], x.color[2]] };
        let hexs = format!("#{:02X}{:02X}{:02X}", now[0], now[1], now[2]);
        let tw = c.text_r(b[2] - 2.2 * r, mid, 0.66 * r, Face::Mono, &hexs, pal().t2);
        let sq = [b[2] - 1.75 * r, b[1] + 0.45 * r, b[2] - 0.55 * r, b[3] - 0.45 * r];
        c.rect_checker(sq, 0.3 * r, grey(0.75, 1.0), grey(0.45, 1.0));
        c.rect(sq, 0.3 * r, [now[0] as f32 / 255.0, now[1] as f32 / 255.0, now[2] as f32 / 255.0, x.color[3] as f32 / 255.0]);
        if chroma > 0.0 {
            c.text_r(b[2] - 2.2 * r - tw - 0.5 * r, mid, 0.6 * r, Face::Medium, tr("Chroma"), pal().t3);
        }
        self.cosm.x_colour_at = b;
        self.hits.push((b, hit));
        y += 2.5 * r;
        // CS's own lamps: one press, its colour
        let d = 1.05 * r;
        let lamp_gap = 0.45 * r;
        for (k, cc) in COLOURS.iter().enumerate() {
            let lb = [x0 + k as f32 * (d + lamp_gap), y, x0 + k as f32 * (d + lamp_gap) + d, y + d];
            let hit = Hit::Cosm(XCOLOR, k as u16);
            let chosen = x.color[..3] == cc[..] && chroma == 0.0;
            let hv = self.an(ak(A_HOV, 33_400 + k), if chosen { 1.0 } else if self.hover == hit { 0.5 } else { 0.0 }, 14.0, 0.0);
            let lit = [cc[0] as f32 / 255.0, cc[1] as f32 / 255.0, cc[2] as f32 / 255.0];
            let g = 0.18 * r * hv;
            c.rect_line([lb[0] - g - 0.12 * r, lb[1] - g - 0.12 * r, lb[2] + g + 0.12 * r, lb[3] + g + 0.12 * r], d * 0.5 + g + 0.12 * r, 1.2, grey(1.0, 0.6 * hv));
            c.rect(lb, d * 0.5, [lit[0], lit[1], lit[2], 0.55 + 0.45 * hv]);
            self.hits.push((lb, hit));
        }
        c.text(x0 + COLOURS.len() as f32 * (d + lamp_gap) + 0.2 * r, y + d * 0.5 + 0.22 * r, 0.6 * r, Face::Regular, tr("CS's own"), pal().t4);
        y += d + 1.2 * r;
        // its code: one line, its two ways at the right — at the panel's foot, over CS2's own (under the colour when
        // the panel is short)
        let foot = pr[3] - 5.2 * r;
        let y = foot.max(y + 0.4 * r);
        let cb = [x0, y, x1, y + 2.0 * r];
        c.rect(cb, 0.6 * r, pal().f1);
        let mut bx = cb[2] - 0.7 * r;
        for (k, (part, label)) in [(XCOPY, "Copy"), (XPASTE, "Paste")].into_iter().enumerate() {
            let tw = fonts().width(Face::Medium, 0.62 * r, tr(label));
            let b = [bx - tw - 0.6 * r, cb[1] + 0.3 * r, bx, cb[3] - 0.3 * r];
            let hit = Hit::Cosm(part, 0);
            let hv = self.an(ak(A_HOV, 33_000 + k), if self.hover == hit { 1.0 } else { 0.0 }, 14.0, 0.0);
            c.rect(b, 0.5 * r, grey(1.0, 0.07 * hv));
            c.text_c((b[0] + b[2]) * 0.5, (b[1] + b[3]) * 0.5 + 0.22 * r, 0.62 * r, Face::Medium, tr(label), mix(pal().t2, pal().t1, hv));
            self.hits.push((b, hit));
            bx = b[0] - 0.2 * r;
        }
        let code = crosshair::encode(x);
        let code = fonts().fit(Face::Mono, 0.58 * r, &code, bx - cb[0] - 1.2 * r);
        c.text(cb[0] + 0.7 * r, (cb[1] + cb[3]) * 0.5 + 0.21 * r, 0.58 * r, Face::Mono, &code, pal().t2);
        // back to CS2's own
        let label = tr("CS2's own");
        let tw = fonts().width(Face::Medium, 0.66 * r, label);
        let m = (pr[0] + pr[2]) * 0.5;
        let b = [m - tw * 0.5 - 0.9 * r, pr[3] - 2.6 * r, m + tw * 0.5 + 0.9 * r, pr[3] - 0.9 * r];
        let hit = Hit::Cosm(XRESET, 0);
        let hv = self.an(ak(A_HOV, 33_500), if self.hover == hit { 1.0 } else { 0.0 }, 14.0, 0.0);
        c.rect(b, 0.8 * r, grey(1.0, 0.06 * hv));
        c.text_c(m, (b[1] + b[3]) * 0.5 + 0.24 * r, 0.66 * r, Face::Medium, label, mix(pal().t3, pal().t1, hv));
        self.hits.push((b, hit));
    }

    /// The picker open on the crosshair's colour: what it says taken into the crosshair (its brightness and hue the
    /// colour, its transparency the crosshair's, its Chroma the hue running round) — kept as the hand lets go.
    fn xhair_colour_follow(&mut self) {
        let Some((owner, set, drag)) = self.pick.as_ref().map(|p| (p.owner, p.set, p.drag)) else { return };
        if owner != Owner::Cosm || set != 1 {
            return;
        }
        let Kind::Colour(col) = self.cosm_cols[1].kind else { return };
        let mut x = current();
        let a = (col.a * 255.0).round().clamp(0.0, 255.0) as u8;
        let want = [(col.rgb >> 16) as u8, (col.rgb >> 8) as u8, col.rgb as u8, a];
        let chroma = (col.chroma * 100.0).round() / 100.0;
        let (lo, _) = loadout::current();
        if want == x.color && chroma == lo.crosshair_chroma {
            if drag == 0 && std::mem::take(&mut self.cosm.x_dirty) {
                loadout::change(|_| {});
            }
            return;
        }
        x.color = want;
        let code = crosshair::encode(&x);
        if drag == 0 {
            loadout::change(|l| {
                l.crosshair = code;
                l.crosshair_chroma = chroma;
            });
        } else {
            // (while the hand is down: shown at once, written as it lets go)
            loadout::change_quiet(|l| {
                l.crosshair = code;
                l.crosshair_chroma = chroma;
            });
            self.cosm.x_dirty = true;
        }
    }

    /// A press on the editor.
    pub(super) fn xhair_press(&mut self, part: u8, n: u16) {
        let mut x = current();
        match part {
            XCOPY => {
                self.copy(&crosshair::encode(&x));
                self.toast(tr("The crosshair's code is copied: CS2 takes it too").to_string());
                self.sound_tick(0.7);
                return;
            }
            XPASTE => {
                self.cosm.paste = true;
                self.outbox.push("paste".into());
                self.sound_tick(0.5);
                return;
            }
            XSTYLE => x.style = STYLES.get(n as usize).map_or(4, |s| s.1),
            XSLIDE => {
                self.cosm.drag = Some((10 + n as u8, self.mouse));
                self.xhair_move(n as u8, self.mouse[0]);
                return;
            }
            XPICK => {
                if self.pick.as_ref().is_some_and(|p| p.owner == Owner::Cosm && p.set == 1) {
                    self.pick = None;
                    self.sound_tick(0.4);
                    return;
                }
                // the client's picker on a setting of the page's own: the crosshair's colour in it, CS2's own its default
                let chroma = loadout::current().0.crosshair_chroma;
                let own = Crosshair::default().color;
                self.cosm_cols[1].kind = Kind::Colour(Col { rgb: rgb_u32(x.color), a: x.color[3] as f32 / 255.0, chroma, auto: false });
                self.cosm_cols[1].def = Kind::Colour(Col::alpha(rgb_u32(own), own[3] as f32 / 255.0));
                self.open_picker(Owner::Cosm, 1, self.cosm.x_colour_at);
                return;
            }
            XBG | XZOOM => {
                let (lo, _) = loadout::current();
                let mut stage = lo.xh_stage;
                if part == XBG {
                    stage.0 = (n as u8).min(BACKGROUNDS.len() as u8 - 1);
                } else {
                    stage.1 = (n as u8).min(ZOOMS.len() as u8 - 1);
                }
                if stage != lo.xh_stage {
                    loadout::change(|l| l.xh_stage = stage);
                    self.sound_tick(0.5);
                }
                return;
            }
            XTOGGLE => match n {
                0 => x.dot = !x.dot,
                1 => x.t_style = !x.t_style,
                _ => x.outline_mode = if x.outline_mode > 0 { 0 } else { 1 },
            },
            XCOLOR => {
                if let Some(col) = COLOURS.get(n as usize) {
                    x.color = [col[0], col[1], col[2], x.color[3]];
                    // (a lamp of CS's stands still)
                    loadout::change(|l| l.crosshair_chroma = 0.0);
                    self.xhair_picker_again(&x);
                }
            }
            XRESET => {
                x = Crosshair::default();
                loadout::change(|l| l.crosshair_chroma = 0.0);
                self.xhair_picker_again(&x);
            }
            _ => return,
        }
        keep(&x);
        self.sound_tick(0.5);
    }

    /// The picker open on the crosshair's colour as it changed elsewhere (a lamp, CS2's own): it shows it.
    fn xhair_picker_again(&mut self, x: &Crosshair) {
        if !self.pick.as_ref().is_some_and(|p| p.owner == Owner::Cosm && p.set == 1) {
            return;
        }
        self.cosm_cols[1].kind = Kind::Colour(Col { rgb: rgb_u32(x.color), a: x.color[3] as f32 / 255.0, chroma: 0.0, auto: false });
        if let Some(p) = self.pick.as_mut() {
            p.hsv = super::theme::hsv(rgb_u32(x.color));
        }
    }

    /// A slider held: its value where the pointer is.
    pub(super) fn xhair_move(&mut self, n: u8, px: f32) {
        let k = n as usize;
        let Some((_, lo, hi)) = SLIDERS.get(k) else { return };
        let [a, b] = self.cosm.x_tracks[k];
        let t = ((px - a) / (b - a).max(1.0)).clamp(0.0, 1.0);
        let mut x = current();
        let v = (lo + (hi - lo) * t).round();
        match k {
            0 => x.length = v,
            1 => x.thickness = v,
            2 => x.gap = v,
            _ => x.color[3] = v as u8,
        }
        if x != current() {
            keep(&x);
            if k == 3 {
                if let (true, Kind::Colour(col)) = (self.pick.as_ref().is_some_and(|p| p.owner == Owner::Cosm && p.set == 1), &mut self.cosm_cols[1].kind) {
                    col.a = v / 255.0;
                }
            }
        }
    }

    /// The clipboard came for the crosshair: a CS code read into it, or a word that it is not one.
    pub(super) fn cosm_paste(&mut self, text: &str) {
        let code = text.trim().to_string();
        match crosshair::decode(&code) {
            Some(_) => {
                loadout::change(|l| {
                    l.crosshair = code;
                    l.crosshair_chroma = 0.0;
                });
                self.toast(tr("The crosshair is CS's code's now").to_string());
            }
            None => self.toast(tr("That is not a CS crosshair's code").to_string()),
        }
    }
}
