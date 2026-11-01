//! Other Custom Things (Interface → Other Custom Things): the boss bars, the title, the action bar and the name tags
//! over things. Their words are written in the HUD's letters (Java's HudOther and HudText: a server's glyphs and
//! colours stay). A boss bar is a glass card of its own, its name in it and its fill a thin line along the card's
//! bottom edge (the server's colour, easing, a trail of what was lost, its notches), sliding in and out; the title is a
//! glass card in the upper half of the screen, coming in as its animation says; the action bar on its pill. The name
//! tags' look is a preview (their plate, shadow, size and fading are Java's, in the world).

use super::hudtext::RText;
use super::*;
use std::collections::HashMap;

/// A boss bar as Java has it: how full, its colour and notches (the game's order), how seen (it slides in and out),
/// its own number.
#[derive(Clone, Copy, Default)]
pub struct BossBar {
    pub progress: f32,
    pub color: u8,
    pub overlay: u8,
    pub alpha: f32,
    pub id: u32,
}

/// The title: how seen, how long it is in (s), whether it has a subtitle.
#[derive(Clone, Copy, Default)]
pub struct TitleF {
    pub alpha: f32,
    pub phase: f32,
    pub sub: bool,
}

/// The action bar: how seen, since it changed, a record's colours going round.
#[derive(Clone, Copy, Default)]
pub struct ActionF {
    pub alpha: f32,
    pub since: f32,
    pub rainbow: bool,
}

#[derive(Clone, Default)]
pub struct Other {
    pub gs: f32,
    pub bars: Vec<BossBar>,
    pub names: Vec<RText>,
    pub title: Option<TitleF>,
    pub title_text: RText,
    pub sub_text: RText,
    pub action: Option<ActionF>,
    pub action_text: RText,
}

impl Other {
    /// From Java's numbers at `at` (HudOther.append): the bars (2, then 6 each), the title (6), the action bar (5); and
    /// its text: each bar's name, the title and subtitle (when there is a title), the action bar's (when there is one).
    pub fn parse(g: &dyn Fn(usize) -> f64, at: usize, line: &mut dyn FnMut() -> String) -> (Other, usize) {
        let n = (g(at).max(0.0) as usize).min(8);
        let mut o = Other { gs: (g(at + 1) as f32).max(1.0), ..Default::default() };
        let mut i = at + 2;
        for k in 0..n {
            o.bars.push(BossBar {
                progress: (g(i) as f32).clamp(0.0, 1.0),
                color: g(i + 1) as u8,
                overlay: g(i + 2) as u8,
                alpha: (g(i + 4) as f32).clamp(0.0, 1.0),
                id: g(i + 5) as u32,
            });
            o.names.push(RText::parse(70 + k as u32, &line()));
            i += 6;
        }
        if g(i) > 0.5 {
            o.title = Some(TitleF { alpha: g(i + 1) as f32, phase: g(i + 2) as f32, sub: g(i + 5) > 0.5 });
            o.title_text = RText::parse(80, &line());
            o.sub_text = RText::parse(81, &line());
        }
        i += 6;
        if g(i) > 0.5 {
            o.action = Some(ActionF { alpha: g(i + 1) as f32, since: g(i + 3) as f32, rainbow: g(i + 4) > 0.5 });
            o.action_text = RText::parse(82, &line());
        }
        i += 5;
        (o, i - at)
    }
}

/// Where an element's words go (said back to Java): relative to its corner as drawn, then on the screen; how seen.
#[derive(Clone, Copy, Default)]
pub struct Placed {
    pub rel: [f32; 2],
    pub abs: [f32; 2],
    pub alpha: f32,
    /// a boss bar row's height (px)
    pub step: f32,
}

/// The boss bars' colours (the game's order: pink, blue, red, green, yellow, purple, white).
const BAR_COLOURS: [u32; 7] = [0xff6ac1, 0x56a8ff, 0xff5757, 0x5fe08a, 0xffd65a, 0xb478ff, 0xf2f2f2];
/// How many notches each overlay has (none, 6, 10, 12, 20).
const NOTCHES: [usize; 5] = [1, 6, 10, 12, 20];

fn ease(x: f32) -> f32 {
    let t = x.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

/// A colour of the hue `h` (turns), as the game's record line goes round.
fn hue(h: f32) -> Rgba {
    let h = h.rem_euclid(1.0) * 6.0;
    let (s, v) = (0.55, 0.95);
    let f = h.fract();
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    let (r, g, b) = match h as u32 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    [r, g, b, 1.0]
}

impl Gui {
    fn other_now(&self) -> Option<Other> {
        self.hud.facts.other.clone()
    }

    pub(super) fn el_bossbars(&mut self, c: &mut Canvas, x: &Ctx) -> Option<[f32; 2]> {
        let s = x.s;
        let p = x.pad();
        let real = self.other_now();
        let example = x.sample && (!x.icons || real.as_ref().is_none_or(|o| o.bars.is_empty()));
        let (bars, names) = if example {
            (
                vec![BossBar { progress: 0.64, color: 5, overlay: 2, alpha: 1.0, id: 1 }, BossBar { progress: 0.3, color: 4, overlay: 0, alpha: 1.0, id: 2 }],
                vec![RText::words(tr("Ender Dragon")), RText::words(tr("PvP mode: 18 s"))],
            )
        } else {
            let o = real?;
            if o.bars.is_empty() {
                return None;
            }
            (o.bars, o.names)
        };
        let icons = x.icons && !example;
        let tk = super::text::scale();
        let fs = 12.5 * s * (x.num("Names size") / 100.0).max(0.3);
        let percent = x.on("Percent");
        let width = x.num("Width").max(60.0) * s;
        let thick = x.num("Thickness").max(1.0) * s;
        let gap = x.num("Gap") * s;
        let show_names = x.on("Names");
        // each bar its own glass card (the element's Background), its line along the card's bottom edge
        let card = x.el.bg;
        let py = if card { (p * 0.8).max(7.0 * s) } else { 0.0 };
        let name_h = if show_names || percent { fs * tk * 1.35 } else { 0.0 };
        let edge = if card { (3.5 * s).max(thick * 0.6) } else { 0.0 };
        let ch = py + name_h + if show_names || percent { 5.0 * s } else { py.min(6.0 * s) } + thick + edge;
        let name_w = names.iter().map(|n| n.width(fs, Face::Semi)).fold(0.0, f32::max);
        let w = width.max(name_w + 2.0 * p);
        let step = ch + gap;
        let h = bars.len() as f32 * step - gap;
        if x.icons {
            let pl = self.hud.placed.entry("bossbars").or_default();
            pl.rel = [w / 2.0, py];
            pl.step = step;
        }
        let t = x.t;
        let dt = self.dt;
        let trail_on = x.on("Lost trail");
        let accent = x.pick("Colours") == 1;
        let notches_on = x.pick("Notches") == 0;
        let rad_card = x.radius();
        let a0 = c.alpha;
        for (i, b) in bars.iter().enumerate() {
            // sliding in from above as it comes, out as it goes
            let y = M + i as f32 * step - (1.0 - b.alpha) * 6.0 * s;
            c.alpha = a0 * b.alpha;
            let r = [M, y, M + w, y + ch];
            if card {
                x.part(c, r);
            }
            if show_names {
                if let Some(name) = names.get(i) {
                    let nw = name.width(fs, Face::Semi);
                    self.rtext(c, name, M + (w - nw) / 2.0, y + py + fs * tk * 1.0, fs, Face::Semi, x.el.text, icons);
                }
            }
            // how much is left, in the corner
            if percent {
                let label = format!("{}%", (b.progress * 100.0).round() as i32);
                c.text_r(M + w - p.max(8.0 * s), y + py + fs * tk * 1.0, fs * 0.9, Face::Mono, &label, x.el.text2);
            }
            // the line: along the bottom edge, clear of the card's round corners
            let inset = if card { rad_card.max(8.0 * s) } else { 0.0 };
            let bx = [r[0] + inset, r[3] - edge - thick, r[2] - inset, r[3] - edge];
            let lw = bx[2] - bx[0];
            // the fill eases, the lost part waits a moment then follows
            let st = self.hud.boss.entry(b.id).or_insert([b.progress, b.progress, t]);
            st[0] += (b.progress - st[0]) * (1.0 - (-dt * 9.0).exp());
            if b.progress >= st[1] || !trail_on {
                st[1] = b.progress;
                st[2] = t;
            } else if t - st[2] > 0.4 {
                st[1] += (b.progress - st[1]) * (1.0 - (-dt * 4.0).exp());
            }
            let (shown, trail) = (st[0], st[1]);
            let col = if accent { x.el.accent } else { crate::gui::draw::hex(BAR_COLOURS[(b.color as usize).min(6)], 1.0) };
            let n = if notches_on { NOTCHES[(b.overlay as usize).min(4)] } else { 1 };
            let sg = if n > 1 { (1.5 * s).min(lw / n as f32 * 0.3) } else { 0.0 };
            let seg_w = (lw - sg * (n as f32 - 1.0)) / n as f32;
            let rad = thick / 2.0;
            for j in 0..n {
                let (a, z) = (j as f32 / n as f32, (j + 1) as f32 / n as f32);
                let sr = [bx[0] + j as f32 * (seg_w + sg), bx[1], bx[0] + j as f32 * (seg_w + sg) + seg_w, bx[3]];
                let part = |f: f32| ((f - a) / (z - a)).clamp(0.0, 1.0);
                c.rect(sr, rad, grey(1.0, if card { 0.09 } else { 0.16 }));
                let ft = part(trail);
                if ft > 0.0 {
                    c.rect([sr[0], sr[1], sr[0] + (sr[2] - sr[0]) * ft, sr[3]], rad, grey(1.0, 0.38));
                }
                let fv = part(shown);
                if fv > 0.0 {
                    c.rect([sr[0], sr[1], sr[0] + (sr[2] - sr[0]) * fv, sr[3]], rad, col);
                }
            }
            // a soft light at the fill's end
            if x.on("Colour glow") && shown > 0.01 {
                let gx = bx[0] + lw * shown;
                c.glow([gx, (bx[1] + bx[3]) / 2.0], thick * 2.6, fade(col, 0.35));
            }
        }
        c.alpha = a0;
        if !example {
            // (the bars gone: their state with them)
            self.hud.boss.retain(|id, _| bars.iter().any(|b| b.id == *id));
        }
        Some([w, h])
    }

    pub(super) fn el_titles(&mut self, c: &mut Canvas, x: &Ctx) -> Option<[f32; 2]> {
        let s = x.s;
        let p = x.pad();
        let real = self.other_now().and_then(|o| o.title.map(|t| (t, o.title_text, o.sub_text)));
        let example = x.sample && (!x.icons || real.is_none());
        let (tf, title, sub) = if example {
            (TitleF { alpha: 1.0, phase: 2.0, sub: true }, RText::words(tr("Victory!")), RText::words(tr("You won the duel")))
        } else {
            real?
        };
        let icons = x.icons && !example;
        let tk = super::text::scale();
        let (tfs, sfs) = (34.0 * s * (x.num("Title size") / 100.0).max(0.3), 16.0 * s * (x.num("Subtitle size") / 100.0).max(0.3));
        let has_sub = tf.sub && !sub.is_empty() && x.on("Subtitle");
        let tw = title.width(tfs, Face::Semi);
        let sw = if has_sub { sub.width(sfs, Face::Regular) } else { 0.0 };
        let (px, py) = ((p * 2.2).max(22.0 * s), (p * 1.4).max(14.0 * s));
        let w = tw.max(sw) + 2.0 * px;
        let h = 2.0 * py + tfs * tk * 1.12 + if has_sub { sfs * tk * 1.6 } else { 0.0 };
        // coming in: a little pop, rising, or only its light; going: a touch smaller as it fades
        let ph = tf.phase * (x.num("Animation speed") / 100.0).max(0.25);
        let (mut k, mut dy) = (1.0, 0.0);
        match x.pick("Animation") {
            0 => {
                let e = (ph / 0.45).clamp(0.0, 1.0);
                let back = 1.0 + 2.2 * (e - 1.0).powi(3) + 1.2 * (e - 1.0).powi(2);
                k = 0.86 + 0.14 * back;
            }
            1 => dy = (1.0 - ease(ph / 0.5)) * 14.0 * s,
            _ => {}
        }
        k *= 0.96 + 0.04 * (tf.alpha * 1.5).clamp(0.0, 1.0);
        let a0 = c.alpha;
        c.alpha = a0 * tf.alpha;
        let (cx, cy) = (M + w / 2.0, M + h / 2.0 + dy);
        let r = [cx - w / 2.0 * k, cy - h / 2.0 * k, cx + w / 2.0 * k, cy + h / 2.0 * k];
        if x.el.bg {
            x.part(c, r);
        } else {
            // no card: a soft shade, the words read over any sky
            c.veil([r[0] - 20.0 * s, r[1], r[2] + 20.0 * s, r[3]], grey(0.0, 0.42), 26.0 * s, 0.0);
        }
        let tb = r[1] + py * k + tfs * tk * k * 0.92;
        self.rtext(c, &title, cx - tw * k / 2.0, tb, tfs * k, Face::Semi, x.el.text, icons);
        if has_sub {
            self.rtext(c, &sub, cx - sw * k / 2.0, tb + sfs * tk * k * 1.55, sfs * k, Face::Regular, x.el.text2, icons);
        }
        c.alpha = a0;
        if x.icons {
            let pl = self.hud.placed.entry("titles").or_default();
            pl.rel = [w / 2.0, h / 2.0];
        }
        Some([w, h])
    }

    pub(super) fn el_actionbar(&mut self, c: &mut Canvas, x: &Ctx) -> Option<[f32; 2]> {
        let s = x.s;
        let real = self.other_now().and_then(|o| o.action.map(|a| (a, o.action_text)));
        let example = x.sample && (!x.icons || real.is_none());
        let (af, words) = if example {
            (ActionF { alpha: 1.0, since: 9.0, rainbow: false }, RText::words(tr("Mana: 120 / 200")))
        } else {
            real?
        };
        let icons = x.icons && !example;
        let fs = 12.5 * s;
        let tk = super::text::scale();
        let (px, py) = (12.0 * s * x.look.pad * x.el.pad, 6.0 * s * x.look.pad * x.el.pad);
        let words = if x.pick("Server colours") == 1 { RText::words(&words.plain()) } else { words };
        let tw = words.width(fs, Face::Medium);
        let w = tw + 2.0 * px;
        let h = fs * tk * 1.25 + 2.0 * py;
        let a0 = c.alpha;
        // it comes in growing a little
        let grow = 1.0 - ease_out((af.since / 0.3).min(1.0));
        c.alpha = a0 * af.alpha;
        let r = [M + grow * 6.0 * s, M, M + w - grow * 6.0 * s, M + h];
        match x.pick("Backdrop") {
            0 => x.part(c, r),
            1 => c.veil([M - 10.0 * s, M - 4.0 * s, M + w + 10.0 * s, M + h + 4.0 * s], grey(0.0, 0.42), 12.0 * s, 0.0),
            _ => {}
        }
        let col = if af.rainbow { hue(x.t * 0.4) } else { x.el.text };
        self.rtext(c, &words, M + px, M + py + fs * tk * 0.98, fs, Face::Medium, col, icons);
        c.alpha = a0;
        if x.icons {
            let pl = self.hud.placed.entry("actionbar").or_default();
            pl.rel = [w / 2.0, h / 2.0];
        }
        Some([w, h])
    }

    /// The name tags' look, as a preview (in the world Java draws them; nothing of this in the game's HUD).
    pub(super) fn el_holograms(&mut self, c: &mut Canvas, x: &Ctx) -> Option<[f32; 2]> {
        if x.icons {
            return None;
        }
        let s = x.s * (x.num("Tag size") / 100.0).max(0.3);
        let fs = 9.0 * s;
        let lines = [("[Admin] ", "Lunuca"), ("", tr("Click to open the shop"))];
        let shadow = x.on("Words shadow");
        let plate = x.pick("Plate");
        let op = match plate {
            1 => 0.0,
            2 => x.num("Plate opacity") / 100.0,
            _ => 0.25,
        };
        let pc = if plate == 2 { x.col("Plate colour", [0.0, 0.0, 0.0, 1.0]) } else { [0.0, 0.0, 0.0, 1.0] };
        let mut w = 0.0f32;
        for (a, b) in lines {
            w = w.max(fonts().width(Face::Mono, fs, &format!("{a}{b}")));
        }
        let lh = fs * 1.25;
        let (pw, ph) = (w + 4.0 * s, lh * 2.0 + 4.0 * s);
        // two tags: near, and far (fading with distance when it fades)
        let far = if x.on("Fade with distance") { 0.45 } else { 1.0 };
        for (n, a) in [(0, 1.0f32), (1, far)] {
            let ox = M + n as f32 * (pw + 14.0 * s);
            let sc = if n == 1 { 0.7 } else { 1.0 };
            let (ww, hh) = (pw * sc, ph * sc);
            let oy = M + (ph - hh);
            if op > 0.0 {
                c.rect([ox, oy, ox + ww, oy + hh], 0.0, [pc[0], pc[1], pc[2], op * a]);
            }
            for (i, (pre, name)) in lines.iter().enumerate() {
                let y = oy + 2.0 * s * sc + (i as f32 + 0.8) * lh * sc;
                let full = format!("{pre}{name}");
                let tw = fonts().width(Face::Mono, fs * sc, &full);
                let tx = ox + (ww - tw) / 2.0;
                if shadow {
                    c.text(tx + 1.0 * s * sc, y + 1.0 * s * sc, fs * sc, Face::Mono, &full, grey(0.0, 0.6 * a));
                }
                let pw0 = c.text(tx, y, fs * sc, Face::Mono, pre, [1.0, 0.33, 0.33, a]);
                c.text(tx + pw0, y, fs * sc, Face::Mono, name, grey(1.0, a));
            }
        }
        Some([pw * 1.7 + 14.0 * s, ph])
    }
}

/// The boss bars' state between frames: each one's shown fill, the trail of what it lost, since when it waits.
pub type BossState = HashMap<u32, [f32; 3]>;
