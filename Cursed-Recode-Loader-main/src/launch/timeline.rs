//! The launch's numbers for one frame (v12, «one line»): a spark on the desktop draws a line to the window's edges, the
//! window opens along it; the spark is the wings' tip, the feathers are drawn stroke by stroke; under them CURSED rises
//! out of its line; a burning point runs along it WITH THE GAME'S LOADING and rewrites the letters into RECODE; the
//! wings open into rays and lie down as the horizon; the world loads from it in chunks; RECODE glides into the corner
//! as a caption; the menu rises from its lines. The ways out and into a world are scenes of their own.
//! All of it as the approved preview computes it (Desktop\Cursed Recode Launch\v12: js/core.js LT RV XM XW GN,
//! main.js frameParams, gl.js camera); the shaders take it as the Launch block (shaders/launch).

/// The launch, seconds from the start (the desktop's spark): the line, the window opening, the wings' strokes, the
/// rule and the word, the rewrite (at least `REWRITE_MIN` long; it waits for the game: the point never runs past what
/// has loaded).
pub mod lt {
    use super::super::ink::it;
    // («ink»: the window is the ink's from the drop; the loading's pace is the lines' light; launch/ink.rs)
    pub const OPEN_FROM: f32 = 0.0;
    pub const OPEN_TO: f32 = 0.05;
    pub const COOL_TO: f32 = it::WHOLE;
    pub const CHROME: f32 = it::CHROME;
    pub const REWRITE_FROM: f32 = it::LOAD_FROM;
    pub const REWRITE_MIN: f32 = it::LOAD_MIN;
}
/// The opening, seconds from R0 (the letters rewritten): the wings spread, the title glides, the horizon's line, the
/// world's glow, the wave of chunks, the menu.
pub mod rv {
    use super::super::ink::ir;
    pub const MENU_FROM: f32 = ir::MENU_FROM;
    pub const SIDE: f32 = ir::SIDE;
    pub const LIVE: f32 = ir::LIVE;
    pub const SETTLE: f32 = ir::SETTLE;
}
/// A way out's own seconds: the world goes, the window closes onto the line, the line into the spark, a wisp.
#[derive(Clone, Copy)]
pub struct Exit {
    pub close_from: f32,
    pub close_to: f32,
    pub shrink_to: f32,
    pub spark_out: f32,
    pub wisp_to: f32,
    pub line_from: f32,
    pub line_to: f32,
}
/// From the main menu (Quit held, the close button) and from a world («ink»: launch/ink.rs ix): the picture back into its
/// outlines, the ink running down (the window's bar and shadow go with it), gone.
pub const XM: Exit = Exit {
    close_from: super::ink::ix::DRAIN_FROM,
    close_to: super::ink::ix::DRAIN_TO,
    shrink_to: super::ink::ix::DRAIN_TO,
    spark_out: super::ink::ix::DONE - 0.1,
    wisp_to: super::ink::ix::DONE,
    line_from: 99.0,
    line_to: 99.0,
};
pub const XW: Exit = XM;
/// Into a world: the menu goes, the world unloads into the line, the world's name over it, the loading along it, then
/// the world's frame put together in blocks from the line.
pub mod gn {
    pub const UNLOAD_AT: f32 = 0.08;
    pub const LINE_FROM: f32 = 0.75;
    pub const LINE_TO: f32 = 1.25;
    pub const NARROW_FROM: f32 = 1.0;
    pub const NARROW_TO: f32 = 1.6;
    pub const NAME_FROM: f32 = 0.7;
    pub const NAME_TO: f32 = 1.3;
    pub const LOAD_FROM: f32 = 1.95;
    /// The loading's least time along the line (it waits for the world past it).
    pub const LOAD_MIN: f32 = 2.6;
    pub const TILES_DUR: f32 = 1.35;
}

/// The card's seconds the rest of the client keeps to (the menus, LaunchScreen): where the story waits for the game,
/// R0, when the menu rises, its side, its build line, when it takes the pointer, when it has settled.
pub mod tl {
    use super::{lt, rv};
    pub const HOLD_AT: f32 = lt::REWRITE_FROM + lt::REWRITE_MIN;
    pub const R0: f32 = HOLD_AT + 0.45;
    pub const MENU_FROM: f32 = R0 + rv::MENU_FROM;
    pub const SIDE: f32 = R0 + rv::SIDE;
    pub const BUILD: f32 = R0 + rv::SIDE + 0.3;
    pub const LIVE: f32 = R0 + rv::LIVE;
    pub const SETTLE: f32 = R0 + rv::SETTLE;
    /// The game's window may take over from the window before the game from here (the pane is whole and dark behind,
    /// its bar is on).
    pub const HAND_FROM: f32 = lt::COOL_TO + 0.05;
    /// Without the window before the game (a full screen) the card starts here: the pane is there already.
    pub const START_IN_GAME: f32 = lt::COOL_TO;
}

/// The world's glow at rest (the menus' valley as approved: the logo set behind the ridges).
pub const GLOW_REST: f32 = 0.72;
/// The light's place: straight ahead behind the ridges (yaw 0: the spark, the wings' tip and the glow in the middle).
const LIGHT_YAW: f32 = 0.0;
const LIGHT_DIST: f32 = 600.0;
const LIGHT_ELEV: f32 = -0.34;
/// The title's cells (atlas px): their pitch, a letter's inset from its cell's left.
const PITCH: f32 = 172.0;
const INSET: f32 = (172.0 - 92.0) * 0.5;

pub fn clamp01(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}
pub fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let k = clamp01((x - a) / (b - a));
    k * k * (3.0 - 2.0 * k)
}
pub fn smoother(a: f32, b: f32, x: f32) -> f32 {
    let k = clamp01((x - a) / (b - a));
    k * k * k * (k * (k * 6.0 - 15.0) + 10.0)
}
pub fn ease_out(a: f32, b: f32, x: f32) -> f32 {
    let k = clamp01((x - a) / (b - a));
    1.0 - (1.0 - k).powi(3)
}
pub fn ease_out5(a: f32, b: f32, x: f32) -> f32 {
    let k = clamp01((x - a) / (b - a));
    1.0 - (1.0 - k).powi(5)
}
pub fn ease_in(a: f32, b: f32, x: f32) -> f32 {
    let k = clamp01((x - a) / (b - a));
    k * k * k
}
fn mix(a: f32, b: f32, k: f32) -> f32 {
    a + (b - a) * k
}

type V3 = [f32; 3];
fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn mul(a: V3, s: f32) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn norm(a: V3) -> V3 {
    let l = dot(a, a).sqrt().max(1e-9);
    mul(a, 1.0 / l)
}
fn dir_yp(yaw: f32, pitch: f32) -> V3 {
    let cp = pitch.cos();
    [yaw.sin() * cp, pitch.sin(), yaw.cos() * cp]
}
fn river_x(z: f32) -> f32 {
    7.0 * (z * 0.012).sin() + 3.0 * (z * 0.031 + 1.3).sin()
}

pub struct Camera {
    pub pos: V3,
    pub fwd: V3,
    pub right: V3,
    pub up: V3,
    pub tan_h: f32,
    pub aspect: f32,
}

/// The menu's camera: still while everything is put together (`m` 0), then it breathes and leans a little toward the
/// cursor (`m` 1). `life`: the clock that never waits. `pose` (once the menu is up): the menus' screen's place — along
/// the river, height, pitch, yaw — and the field of view (the main menu's: 0, 22, -0.085, 0, 70).
pub fn camera(m: f32, life: f32, aspect: f32, look: [f32; 2], pose: [f32; 5]) -> Camera {
    let z = (12.0 * (life * 0.0045).sin() + pose[0]) * m;
    let pos = [river_x(z) * 0.85 - 1.0, 22.0 + (pose[1] - 22.0 + 0.35 * (life * 0.23).sin()) * m, z];
    let yaw = (0.018 * (life * 0.05).sin() + look[0] + pose[3]) * m;
    let pitch = -0.085 + (0.008 * (life * 0.071).sin() + look[1] + pose[2] + 0.085) * m;
    let fwd = dir_yp(yaw, pitch);
    let right = norm([-fwd[2], 0.0, fwd[0]]);
    let up = cross(right, fwd);
    let fov = (pose[4].clamp(30.0, 110.0) * 0.5).to_radians().tan() / 35.0f32.to_radians().tan();
    let tan_h = (27.0f32.to_radians().tan()).max(0.42 / aspect) * (1.0 + (fov - 1.0) * m);
    Camera { pos, fwd, right, up, tan_h, aspect }
}

/// Where the light burns behind the ridges (its glow, the world lit from it).
pub fn light_pos(cam: &Camera) -> V3 {
    add(cam.pos, mul(dir_yp(LIGHT_YAW, LIGHT_ELEV), LIGHT_DIST))
}

/// A world point on the screen: x, y in 0..1 (y up), the depth.
pub fn to_screen(p: V3, cam: &Camera) -> Option<[f32; 3]> {
    let d = sub(p, cam.pos);
    let zc = dot(d, cam.fwd);
    if zc < 0.5 {
        return None;
    }
    Some([dot(d, cam.right) / (zc * cam.tan_h * cam.aspect) * 0.5 + 0.5, dot(d, cam.up) / (zc * cam.tan_h) * 0.5 + 0.5, zc])
}

/// The cursor is a small fire in the world: a point on the ray under it, ~26 blocks out. `cursor` x, y in 0..1 of
/// the picture (y down), on; `k` how much of it the scene lets in.
pub fn cursor_light(cam: &Camera, cursor: [f32; 3], k: f32) -> [f32; 4] {
    let nx = cursor[0] * 2.0 - 1.0;
    let ny = 1.0 - cursor[1] * 2.0;
    let d = norm(add(cam.fwd, add(mul(cam.right, nx * cam.tan_h * cam.aspect), mul(cam.up, ny * cam.tan_h))));
    let p = add(cam.pos, mul(d, 26.0));
    [p[0], p[1].max(11.0), p[2], cursor[2] * k * 0.9]
}

/// Where the menu puts the caption (LaunchScreen.frame's numbers, so the window before the game and the game agree):
/// picture px per ui px, the word's left edge and the build line's top, picture px from the top, for a picture `w` × `h`.
pub fn menu_anchor(w: f32, h: f32) -> (f32, f32, f32) {
    let ui = (h / 1300.0).max(1.0);
    let narrow = w < 760.0 * ui;
    let vh = h / 100.0;
    let left = if narrow { 20.0 * ui } else { w * 0.062 };
    let small = (h * 1.05 / 100.0).clamp(8.0 * ui, 12.0 * ui);
    let foot_top = h - 5.0 * vh - small * 1.3 - 0.4 * vh;
    (ui, left, foot_top)
}

/// The scene's layout for a picture (ui px, y from the top as the preview's LAY): the tip (the spark, the wings' point,
/// the horizon's line, where the glow is), the logo's size, the title under the tip and as the caption.
#[derive(Clone, Copy)]
pub struct Layout {
    pub view: [f32; 2],
    pub tip: [f32; 2],
    pub logo: f32,
    pub vh: f32,
    /// the title at the start and as the caption: scale (ui px an atlas px), left edge, its line (from the bottom)
    pub t0: [f32; 3],
    pub t1: [f32; 3],
}

/// `w`, `h`: the picture in ui px; `left`, `foot_top` where the menu's caption goes (ui px, from the left and the top);
/// `cap`: the atlas's capital height (atlas px).
pub fn layout(w: f32, h: f32, left: f32, foot_top: f32, cap: f32) -> Layout {
    let cam = camera(0.0, 0.0, w / h, [0.0; 2], [0.0, 22.0, -0.085, 0.0, 70.0]);
    let s = to_screen(add(cam.pos, mul(dir_yp(LIGHT_YAW, 0.004), 1000.0)), &cam).unwrap_or([0.5, 0.5, 1.0]);
    let tip = [(s[0] * w).round(), ((1.0 - s[1]) * h).round()];
    let vh = h.min(w * 0.62);
    let cap0 = 0.074 * vh;
    let s0 = cap0 / cap;
    let t0 = [s0, tip[0] - 3.0 * PITCH * s0, h - (tip[1] + 0.215 * vh)];
    let cap1 = (vh * 0.024).clamp(9.0, 20.0);
    let s1 = cap1 / cap;
    let t1 = [s1, left - INSET * s1, (h - foot_top) + (vh * 0.016).max(10.0)];
    Layout { view: [w, h], tip, logo: 0.42 * vh, vh, t0, t1 }
}

/// The title between its place under the tip (0) and the caption (1): scale, left, line, the cells' pitch, the capital.
#[derive(Clone, Copy, Default)]
pub struct Title {
    pub s: f32,
    pub left: f32,
    pub clip: f32,
    pub pitch: f32,
    pub cap: f32,
}
pub fn title_at(l: &Layout, k: f32, cap: f32) -> Title {
    let (a, b) = (l.t0, l.t1);
    let s = a[0] * (b[0] / a[0]).powf(k);
    Title { s, left: mix(a[1], b[1], k), clip: mix(a[2], b[2], k), pitch: PITCH * s, cap: cap * s }
}

/// Which scene plays.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Phase {
    /// The launch and the menu (the story's second `t`).
    #[default]
    Run,
    /// The way out of the main menu (its seconds).
    ExitMenu,
    /// Into a world (its seconds; `load` how far the world has loaded, `loaded_at` when it did, or -1).
    Enter,
    /// In the world (its frame).
    World,
    /// The way out of a world.
    ExitWorld,
}

/// One frame's numbers (the preview's F): the scene's, the light's over it, the title's, the world frame's.
#[derive(Clone, Copy, Default)]
pub struct Frame {
    pub scene_on: bool,
    pub glow: f32,
    pub lit: f32,
    pub rise_t: f32,
    pub rise_dir: f32,
    pub ridge: [f32; 3],
    /// the camera's breathing and the cursor's light: how much of them
    pub cam_m: f32,
    pub cur_k: f32,
    pub logo_i: f32,
    pub stroke: f32,
    pub spread: f32,
    pub ember: f32,
    /// the horizon's line: left, right of the tip (ui px), strength, «as the glow» 0..1
    pub hl: [f32; 4],
    /// the line under the word (its light; crisp in the picture): x0, x1, y, drawn
    pub rule: [f32; 4],
    pub rule_c: [f32; 4],
    /// the burning point: x, strength, trail (px), the line's heat behind it
    pub point: [f32; 4],
    /// its sparks: x, y, strength (speed comes from the renderer)
    pub sparks: [f32; 3],
    pub title: Title,
    pub title_a: f32,
    /// a letter: the old gone 0..1, the new risen 0..1, the new's heat, the old's heat
    pub letters: [[f32; 4]; 6],
    /// the world's frame: shown, seconds of its blocks, which way (1 put together, -1 crumbling), grey
    pub world: [f32; 4],
    /// the menu's shade
    pub menu_k: f32,
    /// The ink over the picture (launch/ink.rs)
    pub pane: super::ink::Pane,
}

/// How far the point is along the line (0..1) at the story's second `d`, never faster than looks right. The story's
/// clock itself keeps to the game's loading while the point runs (`story_pace`): the point slows as it comes up to
/// what has loaded and waits there — it never runs past it, and never leaps.
pub fn point_p(d: f32) -> f32 {
    clamp01((d - lt::REWRITE_FROM) / lt::REWRITE_MIN)
}

/// The desk's second the game's resources began to load, as the launches on this computer took it till now (the
/// first time a guess, a long one: a game sooner only hurries the point to its own pace).
pub const RES_AT: f32 = 16.0;
/// The part of the line the game gets before its resources load (the JVM, the mods, its window): by the time; past
/// when they began the last times it goes on more and more slowly toward `BY_TAIL`.
const BY_TIME: f32 = 0.8;
const BY_TAIL: f32 = 0.95;

/// How far the game has loaded as the point may show it (0..1) at the desk's second `d`: until its resources say
/// (`said` negative) by the time, against when they began the last times (`res_at`) — `BY_TIME` of the line by then,
/// slower and slower; then the rest by what they say (`said` 0..1). 1 once the game is ready.
pub fn game_load(d: f32, said: f32, ready: bool, res_at: f32) -> f32 {
    if ready {
        return 1.0;
    }
    // (from a little before the point sets off: it goes at once, not from a standstill)
    let from = lt::REWRITE_FROM - 0.8;
    let x = ((d - from) / (res_at - from).max(1.0)).max(0.0);
    let k = -(1.0 - BY_TIME / BY_TAIL).ln();
    let by_time = BY_TAIL * (1.0 - (-k * x).exp());
    if said < 0.0 {
        by_time
    } else {
        by_time.max(BY_TIME + (0.97 - BY_TIME) * said.clamp(0.0, 1.0))
    }
}

/// How fast the story's clock runs at its second `t` (1: as the desk's). As is until the point sets off; then it keeps
/// the point up with `goal` (game_load): the gap closed over ~0.6 s, never faster than the card's own pace, slower and
/// slower as the point comes up to it. Once the game is ready, faster (up to 1.8×) while the story is `behind` the
/// desk (s), easing back to 1 by the rewrite's end. Before the game is ready it never passes the rewrite's end.
pub fn story_pace(t: f32, goal: f32, ready: bool, behind: f32) -> f32 {
    if t < lt::REWRITE_FROM {
        return 1.0;
    }
    let p = point_p(t);
    if ready {
        return 1.0 + 0.8 * smooth(0.0, 0.25, 1.0 - p) * smooth(0.0, 0.4, behind);
    }
    if t >= tl::HOLD_AT {
        return 0.0;
    }
    ((goal - p) * lt::REWRITE_MIN / 0.6).clamp(0.0, 1.0)
}


/// The scene's numbers for a frame. `t`: the story's second (Run) or the scene's own; `load` the game's (Run) or the
/// world's (Enter) loading, 0..1; `loaded_at`: when the world had loaded (Enter's seconds; negative: not yet); `cap` the
/// atlas's capital.
/// `quit`: seconds since the window was closed while the launch played (-1: it was not).
pub fn frame(phase: Phase, t: f32, load: f32, loaded_at: f32, l: &Layout, cap: f32, quit: f32, fx: f32) -> Frame {
    let mut f = Frame { ridge: [1.0; 3], ..Frame::default() };
    let all_new = |rise: f32| [[1.0, rise, 0.0, 0.0]; 6];
    match phase {
        Phase::Run => {
            let d = t;
            let r = d - tl::R0;
            f.pane = super::ink::launch(d, r, point_p(d), fx, quit);
            // the valley under the ink: all of it there, its glow at rest, from the start (the outlines are its)
            f.scene_on = true;
            f.glow = GLOW_REST;
            f.lit = 1.0;
            f.menu_k = smooth(rv::MENU_FROM - 0.6, rv::LIVE, r);
            f.cam_m = smooth(rv::LIVE - 0.4, rv::SETTLE + 0.6, r);
            f.cur_k = smooth(rv::LIVE, rv::SETTLE + 0.6, r);
            // the caption in the menu's corner comes with the menu
            let tt = title_at(l, 1.0, cap);
            let shown = smooth(rv::MENU_FROM, rv::SIDE + 0.3, r);
            f.title = tt;
            f.title_a = shown;
            f.letters = all_new(shown);
        }
        Phase::ExitMenu => {
            let x = t;
            f.pane = super::ink::way_out(x, fx);
            f.scene_on = true;
            f.cam_m = 1.0;
            f.glow = GLOW_REST;
            f.lit = 1.0;
            f.menu_k = 1.0 - smooth(0.0, 0.8, x);
            let tt = title_at(l, 1.0, cap);
            f.title = tt;
            f.title_a = 1.0;
            f.letters = all_new(1.0 - smooth(0.05, 0.55, x));
        }
        Phase::Enter => {
            // («ink»: launch/ink.rs ie) the valley into its living outlines, kept while the world loads, the world's
            // outlines coming over them once it is there, the world filling in from them
            let x = t;
            f.scene_on = true;
            f.cam_m = 1.0;
            f.glow = GLOW_REST;
            f.lit = 1.0;
            f.menu_k = 1.0 - smooth(0.0, 0.8, x);
            let tt = title_at(l, 1.0, cap);
            f.title = tt;
            f.title_a = 1.0;
            f.letters = all_new(1.0 - smooth(0.05, 0.55, x));
            let (pane, world) = super::ink::enter(x, load, loaded_at, fx);
            f.pane = pane;
            if world > 0.0 {
                f.world = [world, 99.0, 1.0, 0.0];
            }
        }
        Phase::World => {
            f.world = [1.0, 99.0, 1.0, 0.0];
        }
        Phase::ExitWorld => {
            // the world (the game's own frame, live) into its outlines, which go over into the valley's — the window
            // before the game draws the valley, not the world: it takes the way out from there, on the same picture —
            // then the ink runs down as from the menu
            let w = t;
            f.pane = super::ink::way_out(w, fx);
            f.scene_on = true;
            f.cam_m = 1.0;
            f.glow = GLOW_REST;
            f.lit = 1.0;
            let k = 1.0 - smoother(0.7, 1.35, w);
            if k > 0.0 {
                f.world = [k, 99.0, 1.0, 0.0];
            }
        }
    }
    if phase == Phase::ExitMenu && t > XM.close_to + 0.1 || phase == Phase::ExitWorld && t > XW.close_to + 0.1 {
        // the window has closed onto the line: inside it nothing more
        f.hl[2] = 0.0;
    }
    f
}

/// When the scene at `t` is over (the way out's wisp gone), and when the world's frame is whole (Enter).
pub fn enter_done(t: f32, loaded_at: f32) -> bool {
    loaded_at >= 0.0 && t >= loaded_at.max(super::ink::ie::EARLIEST) + super::ink::ie::DONE
}

/// The desktop's part of the window before the game (the preview's deskState): the line's length either side of the
/// spark, its strength and its heads', the window's opening up and down (px), its edges' heat, whether it closes, the
/// spark, the wisp (0..1, -1 none), the window's shadow. Lengths in the window's px (`tip_up`, `tip_down`: from the
/// spark to the window's top and bottom edges; `line_l`, `line_r` to its sides).
#[derive(Clone, Copy, Default)]
pub struct Desk {
    pub left: f32,
    pub right: f32,
    pub line: f32,
    pub head: f32,
    pub up: f32,
    pub down: f32,
    pub edge: f32,
    pub closing: f32,
    pub spark: f32,
    pub wisp: f32,
    pub shadow: f32,
}

pub fn desk_launch(d: f32, tip_up: f32, tip_down: f32, line_l: f32, line_r: f32) -> Desk {
    let _ = (line_l, line_r);
    // (the window is the ink's from the drop — ink.comp draws it, see-through till it has spread)
    // its shadow once it is a window: the ink spread
    let (a, b) = (super::ink::it::SPREAD_TO - 0.3, super::ink::it::SPREAD_TO + 0.4);
    Desk { up: tip_up, down: tip_down, wisp: -1.0, shadow: smooth(a, b, d), ..Desk::default() }
}

/// The ways out's common end: the window closes onto the line, the line into the spark, the spark goes out in a wisp.
pub fn desk_closing(t: f32, x: &Exit, tip_up: f32, tip_down: f32, line_l: f32, line_r: f32) -> Desk {
    let _ = (line_l, line_r);
    // (the ink runs down in ink.comp; here only its shadow goes)
    Desk { up: tip_up, down: tip_down, closing: 1.0, wisp: -1.0, shadow: 1.0 - smooth(x.close_from, x.close_to, t), ..Desk::default() }
}
