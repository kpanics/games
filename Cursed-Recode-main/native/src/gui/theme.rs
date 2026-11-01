//! The client's look, from one list of settings (Theme) and each element's own colours over it, resolved once a frame
//! into plain numbers the drawing reads: the menu's (Palette) and the HUD's (HudLook, the same theme as it suits what
//! is over the world). A colour may turn its hue by itself (chroma) or follow the theme (auto).

use super::data::{Col, Kind, Setting};
use super::draw::{mix, Canvas, Rgba};
use std::cell::Cell;

/// A setting's value by its label (a section's header may have the same name: it is not one).
pub fn find<'a>(set: &'a [Setting], label: &str) -> Option<&'a Kind> {
    set.iter().find(|s| s.label == label && !matches!(s.kind, Kind::Header | Kind::Action | Kind::Cost)).map(|s| &s.kind)
}
pub fn on(set: &[Setting], label: &str) -> bool {
    matches!(find(set, label), Some(Kind::Toggle(true)))
}
pub fn num(set: &[Setting], label: &str) -> f32 {
    match find(set, label) {
        Some(Kind::Slider { v, .. }) => *v,
        _ => 0.0,
    }
}
pub fn pick(set: &[Setting], label: &str) -> usize {
    match find(set, label) {
        Some(Kind::Choice { v, .. }) => *v,
        _ => 0,
    }
}
pub fn text<'a>(set: &'a [Setting], label: &str) -> &'a str {
    match find(set, label) {
        Some(Kind::Text(v)) => v,
        _ => "",
    }
}
pub fn col(set: &[Setting], label: &str) -> Col {
    match find(set, label) {
        Some(Kind::Colour(c)) => *c,
        _ => Col::new(0xffffff),
    }
}

// ------------------------------------------------------------------------------------------------ colours
pub fn rgb_of(c: u32) -> [f32; 3] {
    [((c >> 16) & 255) as f32 / 255.0, ((c >> 8) & 255) as f32 / 255.0, (c & 255) as f32 / 255.0]
}
pub fn pack(r: [f32; 3]) -> u32 {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    (b(r[0]) << 16) | (b(r[1]) << 8) | b(r[2])
}
/// rgb → hue (0..1), saturation, value
pub fn hsv(c: u32) -> [f32; 3] {
    let [r, g, b] = rgb_of(c);
    let mx = r.max(g).max(b);
    let mn = r.min(g).min(b);
    let d = mx - mn;
    let h = if d <= 1e-6 {
        0.0
    } else if mx == r {
        ((g - b) / d).rem_euclid(6.0) / 6.0
    } else if mx == g {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    [h, if mx <= 1e-6 { 0.0 } else { d / mx }, mx]
}
pub fn from_hsv(h: f32, s: f32, v: f32) -> u32 {
    let h6 = h.rem_euclid(1.0) * 6.0;
    let c = v * s;
    let x = c * (1.0 - ((h6 % 2.0) - 1.0).abs());
    let (r, g, b) = match h6 as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    pack([r + m, g + m, b + m])
}
/// A colour now: its chroma turns its hue with the time (keeping how saturated and bright it is; a grey turns as a
/// full colour of its brightness).
pub fn now(c: Col, t: f32) -> Rgba {
    let mut rgb = c.rgb;
    if c.chroma > 0.0 {
        let [h, s, v] = hsv(c.rgb);
        rgb = from_hsv(h + t * c.chroma * 0.25, s.max(0.65), v.max(0.6));
    }
    let [r, g, b] = rgb_of(rgb);
    [r, g, b, c.a]
}
/// An element's colour: its own, or the theme's while it follows it.
pub fn or(c: Col, theme: Rgba, t: f32) -> Rgba {
    if c.auto {
        theme
    } else {
        now(c, t)
    }
}
fn scale(c: Rgba, k: f32, a: f32) -> Rgba {
    [c[0] * k, c[1] * k, c[2] * k, a]
}

// ------------------------------------------------------------------------------------------------ the menu
#[derive(Clone, Copy)]
pub struct Palette {
    pub t1: Rgba,
    pub t2: Rgba,
    pub t3: Rgba,
    pub t4: Rgba,
    pub t5: Rgba,
    pub f1: Rgba,
    pub f2: Rgba,
    pub f3: Rgba,
    pub f4: Rgba,
    pub accent: Rgba,
    pub switch: Rgba,
    /// the glass's tint and how much of it covers the world
    pub glass: Rgba,
    pub veil: Rgba,
    pub blur: f32,
    pub world: f32,
    /// how much of the world's colour stays round the window (0: grey, as the main menu's valley)
    pub around: f32,
    pub rail: f32,
    pub radius: f32,
    pub cursor: bool,
    pub sheen: bool,
    pub shadow: bool,
    pub size: f32,
    pub ww: f32,
    pub wh: f32,
    pub text: f32,
    pub cols: usize,
    pub desc: bool,
    pub binds: bool,
    pub keep: bool,
    pub anim: bool,
    pub speed: f32,
    /// 0 the glass, 2 a fade, 3 at once (1 was the burning line: no fire is left in the client)
    pub opening: usize,
}

impl Palette {
    pub fn of(set: &[Setting], t: f32) -> Palette {
        let text = now(col(set, "Text"), t);
        let sec = now(col(set, "Secondary text"), t);
        let surf = now(col(set, "Surfaces"), t);
        let surface = |a: f32| [surf[0], surf[1], surf[2], a * surf[3]];
        Palette {
            t1: text,
            t2: sec,
            t3: scale(sec, 0.705, sec[3]),
            t4: scale(sec, 0.46, sec[3]),
            t5: scale(sec, 0.317, sec[3]),
            f1: surface(0.04),
            f2: surface(0.065),
            f3: surface(0.095),
            f4: surface(0.15),
            accent: now(col(set, "Accent"), t),
            switch: now(col(set, "Switches"), t),
            glass: now(col(set, "Glass"), t),
            veil: {
                let c = now(col(set, "Dim colour"), t);
                [c[0], c[1], c[2], num(set, "Dim the world") / 100.0]
            },
            blur: num(set, "Blur") / 100.0,
            world: num(set, "World colour through") / 100.0,
            around: num(set, "World colour around") / 100.0,
            rail: num(set, "Sidebar shade") / 100.0,
            radius: num(set, "Corner radius") / 100.0,
            cursor: on(set, "Cursor light"),
            sheen: on(set, "Sheen"),
            shadow: on(set, "Shadow"),
            size: num(set, "Menu size") / 100.0,
            ww: num(set, "Window width") / 100.0,
            wh: num(set, "Window height") / 100.0,
            text: num(set, "Text size") / 100.0,
            cols: pick(set, "Columns") + 1,
            desc: on(set, "Descriptions"),
            binds: on(set, "Binds on tiles"),
            keep: on(set, "Keep where it was moved"),
            anim: on(set, "Animations"),
            speed: (num(set, "Speed") / 100.0).max(0.05),
            opening: [0, 2, 3][pick(set, "Opening animation").min(2)],
        }
    }
}

thread_local! {
    static PAL: Cell<Option<Palette>> = const { Cell::new(None) };
}
/// The menu's palette this frame (the theme's defaults before the first).
pub fn pal() -> Palette {
    PAL.with(|p| p.get()).unwrap_or_else(|| Palette::of(&super::data::theme_settings(), 0.0))
}
pub fn set_pal(p: Palette) {
    PAL.with(|c| c.set(Some(p)));
}

// ------------------------------------------------------------------------------------------------ the HUD
/// The HUD's look as a whole: the theme's, its plates' glass a little lighter than the menu's (over the world, not
/// over a darkened one), its own size and editing.
#[derive(Clone, Copy)]
pub struct HudLook {
    pub show: bool,
    pub scale: f32,
    pub text_k: f32,
    pub text: Rgba,
    pub text2: Rgba,
    pub accent: Rgba,
    pub glass: Rgba,
    pub blur: f32,
    pub radius: f32,
    pub shadow: bool,
    /// padding: .8 compact, 1 normal, 1.25 roomy
    pub pad: f32,
    pub anim: bool,
    pub speed: f32,
    pub hide_f3: bool,
    pub hide_effects: bool,
    pub snap_edges: bool,
    pub snap_elems: bool,
    pub guides: bool,
    pub grid: f32,
}

impl HudLook {
    /// From the Theme's settings.
    pub fn of(set: &[Setting], t: f32) -> HudLook {
        let g = now(col(set, "Glass"), t);
        HudLook {
            show: true,
            scale: num(set, "Interface scale") / 100.0,
            text_k: num(set, "Interface text") / 100.0,
            text: now(col(set, "Text"), t),
            text2: now(col(set, "Secondary text"), t),
            accent: now(col(set, "Accent"), t),
            glass: [g[0], g[1], g[2], g[3] * 0.82],
            blur: num(set, "Blur") / 100.0 * 0.875,
            radius: 12.0 * num(set, "Corner radius") / 100.0,
            shadow: on(set, "Shadow"),
            pad: [0.8, 1.0, 1.25][pick(set, "Padding").min(2)],
            anim: on(set, "Animations"),
            speed: (num(set, "Speed") / 100.0).max(0.05),
            hide_f3: on(set, "Hide with F3"),
            hide_effects: on(set, "Hide the game's effect icons"),
            snap_edges: on(set, "Snap to the edges"),
            snap_elems: on(set, "Snap to elements"),
            guides: on(set, "Guides"),
            grid: num(set, "Grid"),
        }
    }
}

/// One element's look: the HUD's, where it follows it.
#[derive(Clone, Copy)]
pub struct ElemLook {
    pub scale: f32,
    pub alpha: f32,
    pub bg: bool,
    pub glass: Rgba,
    pub text: Rgba,
    pub text2: Rgba,
    pub accent: Rgba,
    /// its plate: 0 glass, 1 blur only, 2 interference, 3 its colour flat
    pub style: u8,
    /// its corners and the room inside it, times the HUD's
    pub corners: f32,
    pub pad: f32,
    pub shadow: bool,
    pub light: bool,
    /// its letters: their size times the HUD's, family (text.rs set_style), a step bolder, their shadow
    pub text_k: f32,
    pub family: u8,
    pub bolder: bool,
    pub tshadow: f32,
}

impl ElemLook {
    /// Shared material for HUD plates and previews anchored in the world.
    pub(super) fn draw_plate(&self, c: &mut Canvas, r: [f32; 4], look: &HudLook, scale: f32, time: f32) {
        if !self.bg {
            return;
        }
        let g = self.glass;
        let rad = look.radius * self.corners * scale;
        let sh = if self.shadow { 0.55 } else { 0.0 };
        match self.style {
            // the world behind blurred, hardly tinted
            1 => c.glass(r, rad, [g[0], g[1], g[2], g[3] * 0.25], look.blur.max(0.6), sh, 0.0),
            // the television's interference over a dark glass (its prim right after the glass: it eases with it)
            2 => {
                c.glass(r, rad, [g[0], g[1], g[2], g[3].max(0.55)], look.blur, sh, 0.0);
                c.interference(r, rad, time, 3.0, 0.55);
            }
            // its colour flat, nothing through it
            3 => c.glass(r, rad, [g[0], g[1], g[2], g[3].max(0.92)], 0.0, sh, 0.0),
            _ => c.glass(r, rad, g, look.blur, sh, 0.0),
        }
    }

    pub fn of(set: &[Setting], hud: &HudLook, t: f32) -> ElemLook {
        // what an element has not (an older one's file, a card's or a picture's own): as it always looked
        let n = |l: &str, d: f32| if find(set, l).is_some() { num(set, l) } else { d };
        let o = |l: &str, d: bool| if find(set, l).is_some() { on(set, l) } else { d };
        ElemLook {
            scale: num(set, "Scale") / 100.0,
            alpha: num(set, "Opacity") / 100.0,
            bg: on(set, "Background"),
            glass: or(col(set, "Glass"), hud.glass, t),
            text: or(col(set, "Text"), hud.text, t),
            text2: or(col(set, "Secondary text"), hud.text2, t),
            accent: or(col(set, "Accent"), hud.accent, t),
            style: pick(set, "Style") as u8,
            corners: n("Corners", 100.0) / 100.0,
            pad: n("Inner space", 100.0) / 100.0,
            shadow: o("Shadow", true) && hud.shadow,
            light: o("Line of light", false) && find(set, "Fill").is_none(),
            text_k: n("Text size", 100.0) / 100.0,
            family: pick(set, "Font") as u8,
            bolder: o("Bold", false),
            tshadow: n("Text shadow", 0.0) / 100.0,
        }
    }
}

/// A colour with its alpha times `a`.
pub fn fade(c: Rgba, a: f32) -> Rgba {
    [c[0], c[1], c[2], c[3] * a]
}
/// Between two colours (the draw module's mix, for symmetry here).
pub fn blend(a: Rgba, b: Rgba, k: f32) -> Rgba {
    mix(a, b, k)
}
