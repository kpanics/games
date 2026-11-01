//! The launch, «ink» (her choice, 15.10): a drop of ink lands on the desktop and spreads as in water into the window;
//! a pen draws the valley's outlines in white out of the black; while the game loads the drawing lives — the valley
//! under it runs on, the lines sway a little as a hand's drawing — and its lines fill with light from the left as the
//! game gets on; when it is ready the picture fills in from its lines (nearest them first, the light before the dark)
//! and the menu rises. The way out (from the menu, from a world) goes on from the very frame that was shown: the
//! picture goes back into its living outlines, and the ink runs down off the desktop.
//!
//! Here the story's seconds and the numbers for a frame; shaders/launch/ink.comp draws them over the picture (its block:
//! `Pane::block`), see-through over the desktop until the ink has spread.

use super::timeline::{clamp01, smooth, smoother};

/// The launch, seconds from the start (the drop): it lands, the ink spreads over the window, the outlines are drawn,
/// the window's bar comes, the window is whole (the game's window may take over from then); the loading's pace (the
/// story waits for the game from `LOAD_FROM + LOAD_MIN` on: the lines' light keeps to the game's loading till then).
pub mod it {
    pub const DROP: f32 = 0.15;
    pub const SPREAD_TO: f32 = 2.35;
    pub const DRAW_FROM: f32 = SPREAD_TO - 0.7;
    pub const DRAW_TO: f32 = SPREAD_TO + 2.2;
    pub const CHROME: f32 = 2.1;
    pub const WHOLE: f32 = 2.6;
    pub const LIGHT_FROM: f32 = 1.4;
    pub const LOAD_FROM: f32 = 4.7;
    pub const LOAD_MIN: f32 = 3.8;
}
/// After the loading (R0): the picture fills in from the lines, the menu rises, takes the pointer, has settled.
pub mod ir {
    pub const ALIVE_TO: f32 = 2.0;
    pub const MENU_FROM: f32 = 2.9;
    pub const SIDE: f32 = 3.3;
    pub const LIVE: f32 = 3.7;
    pub const SETTLE: f32 = 4.5;
    /// Past this the pane has nothing left to draw.
    pub const OFF: f32 = 2.2;
}
/// A way out, its own seconds: the picture back into its outlines, the ink running down, gone.
pub mod ix {
    pub const LINES_FROM: f32 = 0.1;
    pub const LINES_TO: f32 = 1.25;
    pub const DRAIN_FROM: f32 = 1.35;
    pub const DRAIN_TO: f32 = 3.7;
    pub const DONE: f32 = 3.95;
}

/// Into a world, its own seconds: the valley into its living outlines (the menu going), the world's name (the loading
/// page's); the outlines kept while the world loads, their light its loading; once it is there (not before `EARLIEST`),
/// the outlines go over from the valley's to the world's (`MORPH`), and the world fills in from them (`FILL_FROM` ..
/// `FILL_TO` after it came); then it is the game's own.
pub mod ie {
    pub const LINES_FROM: f32 = 0.05;
    pub const LINES_TO: f32 = 0.95;
    pub const EARLIEST: f32 = 1.6;
    pub const MORPH: f32 = 0.6;
    pub const FILL_FROM: f32 = 0.5;
    pub const FILL_TO: f32 = 2.0;
    pub const DONE: f32 = 2.2;
}

/// The valley's second at the drop (its clock then runs with the one that never waits: the drawing never hangs).
const LIFE0: f32 = 155.0;

/// How far the valley's clock is ahead of the drop's (s): the loader's valley lived before the game was started; the
/// story it starts goes on from its second (twin link's `life_shift`, the same in the game).
static LIFE_SHIFT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub fn life_shift() -> f32 {
    f32::from_bits(LIFE_SHIFT.load(std::sync::atomic::Ordering::Relaxed))
}

pub fn set_life_shift(s: f32) {
    LIFE_SHIFT.store(s.to_bits(), std::sync::atomic::Ordering::Relaxed);
}

/// The lines' light the loading starts from (0..1; 0 the launch's own): the loader lit them as the build came, half
/// of it; the game's loading lights the rest.
static LIGHT_BASE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub fn set_light_base(b: f32) {
    LIGHT_BASE.store(b.clamp(0.0, 1.0).to_bits(), std::sync::atomic::Ordering::Relaxed);
}

fn light_base() -> f32 {
    f32::from_bits(LIGHT_BASE.load(std::sync::atomic::Ordering::Relaxed))
}

/// The pane's numbers for a frame (its block, shaders/launch/ink.comp), and the valley's clock under it.
#[derive(Clone, Copy)]
pub struct Pane {
    pub on: bool,
    pub head: [[f32; 4]; 3],
    /// The valley's second to draw (-1: the clock the caller gives).
    pub life: f32,
    /// The crows kept on the tree (the launch while it is a drawing).
    pub hold: f32,
}

/// The valley's second the crows were startled off the tree as the picture came alive (-1 not yet): the same for every
/// scene after it in this session (the menu, the ways in and out), so none of them leaps.
static STARTLED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0xBF80_0000);

pub fn startled() -> f32 {
    f32::from_bits(STARTLED.load(std::sync::atomic::Ordering::Relaxed))
}

/// As another process saw it (the way out, in the window before the game: the game's second).
pub fn set_startled(s: f32) {
    STARTLED.store(s.to_bits(), std::sync::atomic::Ordering::Relaxed);
}

impl Default for Pane {
    fn default() -> Pane {
        Pane::off()
    }
}

/// The block's size in vec4s: the head, the clock and the picture's shape.
pub const BLOCK_VEC4: usize = 4;

impl Pane {
    pub fn off() -> Pane {
        Pane { on: false, head: [[0.0; 4]; 3], life: -1.0, hold: 0.0 }
    }

    /// The block for the shader: the head, then the clock (`t`) and the picture's width over its height.
    pub fn block(&self, t: f32, aspect: f32) -> [f32; BLOCK_VEC4 * 4] {
        let mut u = [0.0f32; BLOCK_VEC4 * 4];
        for (k, h) in self.head.iter().enumerate() {
            u[k * 4..k * 4 + 4].copy_from_slice(h);
        }
        u[12] = t;
        u[13] = aspect;
        u
    }
}

/// The valley's clock under the ink: its own from the drop on, with the clock that never waits (`fx`).
pub fn life(fx: f32) -> f32 {
    LIFE0 + fx + life_shift()
}

/// How far the game has loaded, as the lines' light shows it (0..1): a part by the clock, the rest with the loading's
/// pace (`prog`: the story's, 0..1 over LOAD_FROM .. LOAD_FROM + LOAD_MIN, waiting for the game).
pub fn loaded(d: f32, prog: f32) -> f32 {
    let base = light_base();
    if base > 0.0 {
        return base + (1.0 - base) * prog;
    }
    0.3 * smooth(it::LIGHT_FROM, it::LOAD_FROM, d) + 0.7 * prog
}

/// The launch at the story's second `d` (`r` from R0), `prog` the loading's pace, `fx` the clock that never waits;
/// `quit`: seconds since the window was closed while it played (-1: it was not).
pub fn launch(d: f32, r: f32, prog: f32, fx: f32, quit: f32) -> Pane {
    let mut g = Pane::off();
    g.life = life(fx);
    // the crows: on the tree while it is a drawing; startled as it comes alive (the valley's second kept)
    if r >= 0.3 {
        if startled() < 0.0 {
            STARTLED.store((g.life - (r - 0.3)).to_bits(), std::sync::atomic::Ordering::Relaxed);
        }
    } else if quit < 0.0 && startled() < 0.0 {
        // (the loader's: they flew there already — they fly on)
        g.hold = 1.0;
    }
    if r > ir::OFF && quit < 0.0 {
        return g;
    }
    g.on = true;
    let drop = clamp01((d - it::DROP) / 0.35);
    let spread = smoother(it::DROP + 0.25, it::SPREAD_TO, d).powf(0.85);
    let draw = smooth(it::DRAW_FROM, it::DRAW_TO, d);
    let alive = smoother(0.0, ir::ALIVE_TO, r);
    g.head[0] = [spread, draw, alive, drop];
    g.head[2] = [if quit >= 0.0 { smooth(0.0, 0.85, quit) } else { 0.0 }, loaded(d, prog), 0.0, 0.0];
    g
}

/// Into a world at its second `x`: the pane, and how much of the world's frame is under it (0 the valley's still);
/// `load` how far the world has loaded, `loaded_at` when it was there (-1 not yet).
pub fn enter(x: f32, load: f32, loaded_at: f32, fx: f32) -> (Pane, f32) {
    let mut g = Pane::off();
    g.on = true;
    g.life = life(fx);
    let lines = smoother(ie::LINES_FROM, ie::LINES_TO, x);
    let (world, fill) = if loaded_at >= 0.0 {
        let le = loaded_at.max(ie::EARLIEST);
        (smoother(le, le + ie::MORPH, x), smoother(le + ie::FILL_FROM, le + ie::FILL_TO, x))
    } else {
        (0.0, 0.0)
    };
    g.head[0] = [1.0, 1.0, fill, 1.0];
    g.head[1] = [lines, 0.0, 0.0, 2.0];
    // the lines' light: the world's loading (all of it once it is there)
    g.head[2] = [0.0, if loaded_at >= 0.0 { 1.0 } else { load.clamp(0.0, 1.0) * 0.95 }, 0.0, 0.0];
    (g, world)
}

/// A way out at its second `x` (`fx` the clock that never waits: the valley goes on under its outlines, as it was).
pub fn way_out(x: f32, fx: f32) -> Pane {
    let mut g = Pane::off();
    g.on = true;
    g.life = life(fx);
    g.head[0] = [1.0, 1.0, 1.0, 1.0];
    g.head[1] = [smoother(ix::LINES_FROM, ix::LINES_TO, x), smooth(ix::DRAIN_FROM, ix::DRAIN_TO, x), 0.0, 1.0];
    g.head[2] = [0.0, 1.0, 0.0, 0.0];
    g
}
