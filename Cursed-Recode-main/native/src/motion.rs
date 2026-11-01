//! Living Motion (her word, 23.10: the game's walk is "a wooden doll's"): the gait of the players' model, worked out
//! from how each one moves. Java hands over a player's movement, turning and state each frame
//! (cursed.recode.motion.LivingMotion) and puts the pose this gives on the model and under it.
//!
//! What it keeps to:
//! - a step goes by the ground covered, not by time: the planted foot goes back exactly as fast as the body goes on (it
//!   never slides); the faster, the longer and quicker the steps, and running has a moment both feet are off;
//! - the legs bend at the knee (stage 2): each sole has its place — the planted one on the ground going back with it,
//!   the swinging one through the air on a curve over the ground (running, the heel kicked up behind first) — and the
//!   thigh and the knee are worked out to reach it (two equal halves: the knee bends by what the leg is short of its
//!   length); the hips sit as the planted knee gives (a little on landing, more running), so the bob is the knee's and
//!   the stride's, not a wave made up; a foot that cannot reach the ground any more lifts (the heel off at the end);
//! - the arms bend at the elbow, more as they swing forward, a runner's held at near a right angle;
//! - the arms answer the legs on springs: they lag and follow through, they do not tick like a pendulum;
//! - the body leans with speed and with speeding up, rolls into a turn, shifts over the planted foot, the shoulders twist
//!   against the stride; the head stays level and looking where it looked;
//! - in the air the legs tuck, landing it sinks and comes back; standing, it breathes and shifts its weight;
//! - swimming, elytra, riding, sleeping stay the game's own.
//! Styles are sets of these numbers (natural, light, heavy); the menu's sliders scale them.

use std::collections::HashMap;
use std::f32::consts::{PI, TAU};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// What Java hands over, in order: the player's key's low 16 bits (its id, twice, +1 for a screen's picture of it), its
/// movement this tick (blocks: x, z, y), its body's yaw (degrees), its flags, the style, the sliders (bounce, lean, arms,
/// stride as fractions; breathing 0/1), the key's high bits (a float holds whole numbers only to 16.7 million).
pub const IN: usize = 13;
const I_KEY: usize = 0;
const I_VX: usize = 1;
const I_VZ: usize = 2;
const I_VY: usize = 3;
const I_BODY: usize = 4;
const I_FLAGS: usize = 5;
const I_STYLE: usize = 6;
const I_BOUNCE: usize = 7;
const I_LEAN: usize = 8;
const I_ARMS: usize = 9;
const I_STRIDE: usize = 10;
const I_BREATH: usize = 11;
const I_KEY_HI: usize = 12;

pub const ON_GROUND: u32 = 1;
pub const CROUCHING: u32 = 2;
pub const SPRINTING: u32 = 4;
/// Swimming, elytra, riding, sleeping, dead: the game's own pose.
pub const VANILLA: u32 = 8;

/// What it gives back (radians, blocks; the model's own signs: a negative xRot swings forward, a positive zRot takes the
/// right limb out and the left one in): on (1, else the game's own), the legs' xRot and zRot (right, left), the arms'
/// (right, left), the shoulders' twist (positive: the left one forward), the chest's tilt, what the head turns by to
/// stay as it was (x, y, z), and under the whole: down (negative), the lean forward, the roll to the right, the shift to
/// the right; then the knees' bend (right, left: the shin back) and the elbows' (the forearm forward); then each sole on
/// the ground or not (right, left) and where it is from under the hips (forward, to the right: blocks) — footprints go
/// where it is set down.
pub const OUT: usize = 28;
pub const O_ON: usize = 0;
pub const O_RLEG_X: usize = 1;
pub const O_RLEG_Z: usize = 2;
pub const O_LLEG_X: usize = 3;
pub const O_LLEG_Z: usize = 4;
pub const O_RARM_X: usize = 5;
pub const O_RARM_Z: usize = 6;
pub const O_LARM_X: usize = 7;
pub const O_LARM_Z: usize = 8;
pub const O_TWIST: usize = 9;
pub const O_CHEST: usize = 10;
pub const O_HEAD_X: usize = 11;
pub const O_HEAD_Y: usize = 12;
pub const O_HEAD_Z: usize = 13;
pub const O_DOWN: usize = 14;
pub const O_LEAN: usize = 15;
pub const O_ROLL: usize = 16;
pub const O_SHIFT: usize = 17;
pub const O_RKNEE: usize = 18;
pub const O_LKNEE: usize = 19;
pub const O_RELBOW: usize = 20;
pub const O_LELBOW: usize = 21;
pub const O_RPLANT: usize = 22;
pub const O_LPLANT: usize = 23;
pub const O_RSOLE_F: usize = 24;
pub const O_RSOLE_R: usize = 25;
pub const O_LSOLE_F: usize = 26;
pub const O_LSOLE_R: usize = 27;

/// A leg, hip to sole (blocks: 12 pixels, the player's model at 0.9375).
const LEG: f32 = 12.0 / 16.0 * 0.9375;
/// The game's walking speed (blocks a second).
const WALK: f32 = 4.317;

/// A style: how it walks.
#[derive(Clone, Copy)]
struct Style {
    /// how quick its steps are (1: a person's pace, about 2.5 strides a second at the game's walking speed)
    cadence: f32,
    /// the part of a stride a foot is planted, walking and running
    duty_walk: f32,
    duty_run: f32,
    /// a spring at each step over the compass's (blocks)
    hop: f32,
    /// over the planted foot (blocks), the roll with it
    sway: f32,
    rock: f32,
    /// the shoulders against the stride
    twist: f32,
    /// the arms' swing against the legs', their springs
    arms: f32,
    arm_hz: f32,
    arm_damp: f32,
    /// the lean: per block a second, per block a second each second
    lean_speed: f32,
    lean_push: f32,
    /// the roll into a turn per (radian a second × block a second)
    turn: f32,
    /// the legs apart; the swinging one out to the side
    width: f32,
    swing_out: f32,
    /// the planted knee's bend at its most (walking, running: radians), the swinging foot's height over the ground at its
    /// highest (blocks), the elbows' bend (walking, running)
    knee_walk: f32,
    knee_run: f32,
    clear_walk: f32,
    clear_run: f32,
    elbow_walk: f32,
    elbow_run: f32,
}

const STYLES: [Style; 3] = [
    // natural: weight and spring
    Style {
        cadence: 1.0,
        duty_walk: 0.52,
        duty_run: 0.42,
        hop: 0.0,
        sway: 0.022,
        rock: 0.035,
        twist: 0.10,
        arms: 0.80,
        arm_hz: 3.4,
        arm_damp: 0.55,
        lean_speed: 0.012,
        lean_push: 0.018,
        turn: 0.010,
        width: 0.03,
        swing_out: 0.05,
        knee_walk: 0.24,
        knee_run: 0.62,
        clear_walk: 0.075,
        clear_run: 0.30,
        elbow_walk: 0.30,
        elbow_run: 1.35,
    },
    // light: quick, springy
    Style {
        cadence: 1.12,
        duty_walk: 0.48,
        duty_run: 0.38,
        hop: 0.035,
        sway: 0.012,
        rock: 0.02,
        twist: 0.08,
        arms: 0.95,
        arm_hz: 4.2,
        arm_damp: 0.45,
        lean_speed: 0.010,
        lean_push: 0.014,
        turn: 0.012,
        width: 0.02,
        swing_out: 0.04,
        knee_walk: 0.30,
        knee_run: 0.70,
        clear_walk: 0.09,
        clear_run: 0.34,
        elbow_walk: 0.36,
        elbow_run: 1.45,
    },
    // heavy: long sure strides, rocking
    Style {
        cadence: 0.88,
        duty_walk: 0.56,
        duty_run: 0.46,
        hop: 0.0,
        sway: 0.040,
        rock: 0.075,
        twist: 0.15,
        arms: 0.60,
        arm_hz: 2.6,
        arm_damp: 0.70,
        lean_speed: 0.015,
        lean_push: 0.022,
        turn: 0.008,
        width: 0.06,
        swing_out: 0.07,
        knee_walk: 0.18,
        knee_run: 0.52,
        clear_walk: 0.06,
        clear_run: 0.24,
        elbow_walk: 0.22,
        elbow_run: 1.22,
    },
];

/// A damped spring toward where it is told.
#[derive(Clone, Copy, Default)]
struct Spring {
    x: f32,
    v: f32,
}

impl Spring {
    /// Toward `to` at `hz` (its own frequency), `damp` (1 critical); in steps short enough to stay steady.
    fn go(&mut self, to: f32, hz: f32, damp: f32, dt: f32) -> f32 {
        let w = TAU * hz;
        let n = ((dt * w / 0.5).ceil() as usize).clamp(1, 8);
        let h = dt / n as f32;
        for _ in 0..n {
            let a = w * w * (to - self.x) - 2.0 * damp * w * self.v;
            self.v += a * h;
            self.x += self.v * h;
        }
        self.x
    }
}

/// One player's walk as it goes.
#[derive(Clone, Copy, Default)]
struct Gait {
    seen: f64,
    fresh: bool,
    /// where in its stride (0..1: the right foot planted from 0)
    phase: f32,
    /// its movement in the body's frame (blocks a second), smoothed: forward, to the right
    fwd: f32,
    side: f32,
    /// how much it walks (0 standing), runs, crouches
    amp: f32,
    /// how far the legs sweep now (it shrinks over a last step when stopping)
    reach: f32,
    run: f32,
    crouch: f32,
    yaw: f32,
    yaw_rate: f32,
    speed_was: f32,
    push: f32,
    lean: Spring,
    turn: Spring,
    arm_r: Spring,
    arm_l: Spring,
    land: Spring,
    elbow_r: Spring,
    elbow_l: Spring,
    air: f32,
    vy: f32,
    grounded: bool,
    idle: f32,
    /// which way the legs swing (radians from forward, to the right)
    heading: f32,
}

static GAITS: Mutex<Option<HashMap<i64, Gait>>> = Mutex::new(None);

fn clock() -> f64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f64()
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Toward `to` at `rate` a second.
fn follow(x: &mut f32, to: f32, rate: f32, dt: f32) {
    *x += (to - *x) * (1.0 - (-rate * dt).exp());
}

fn smootherstep(u: f32) -> f32 {
    let u = u.clamp(0.0, 1.0);
    u * u * u * (u * (u * 6.0 - 15.0) + 10.0)
}

/// A sole at `p` of its stride (0..1, planted first for `duty` of it), its planted sweep `a` either way of the hip
/// (blocks): how far ahead of the hip it is, how high over the ground, how far through its planting (or -1) and its
/// swing (or -1). Planted, it goes back evenly with the ground covered; swinging, it comes forward on an eased curve
/// over the ground — walking, low and even; running (`run`), the heel kicked up behind first, then the knee driven
/// through, the shin reaching out before it lands.
fn sole(p: f32, duty: f32, a: f32, run: f32, clear: f32) -> (f32, f32, f32, f32) {
    if p < duty {
        let u = p / duty;
        (a * (1.0 - 2.0 * u), 0.0, u, -1.0)
    } else {
        let u = (p - duty) / (1.0 - duty);
        let walk_x = smootherstep(u);
        let run_x = smootherstep((u - 0.12) / 0.8);
        let x = -a + 2.0 * a * lerp(walk_x, run_x, run);
        let walk_up = (PI * u.powf(0.85)).sin().max(0.0);
        let run_up = (PI * u.powf(0.6)).sin().max(0.0).powf(1.4);
        (x, clear * lerp(walk_up, run_up, run), -1.0, u)
    }
}

/// A leg's thigh angle (forward positive) and knee bend (radians) to put its sole `x` ahead of the hip and `y` below it:
/// two equal halves of `LEG`, the knee bending by what the leg is short of its length; out of reach, straight toward it.
fn reach_to(x: f32, y: f32) -> (f32, f32) {
    let d = x.hypot(y).clamp(LEG * 0.2, LEG * 0.9995);
    let k = 2.0 * (d / LEG).acos();
    (x.atan2(y) + k * 0.5, k)
}

/// How high the hips sit over a planted sole `x` ahead of them with its knee bent `k`.
fn hip_over(x: f32, k: f32) -> f32 {
    let r = LEG * (k * 0.5).cos();
    (r * r - x * x).max((LEG * 0.45).powi(2)).sqrt()
}

/// A player's pose now, from what Java says of it.
pub fn pose(input: &[f32; IN]) -> [f32; OUT] {
    let now = clock();
    let mut out = [0.0f32; OUT];
    let mut all = GAITS.lock().unwrap_or_else(|p| p.into_inner());
    let map = all.get_or_insert_with(HashMap::new);
    if map.len() > 256 {
        map.retain(|_, g| now - g.seen < 5.0);
    }
    let key = input[I_KEY] as i64 + ((input[I_KEY_HI].max(0.0) as i64) << 16);
    let g = map.entry(key).or_insert(Gait { fresh: true, ..Gait::default() });
    let flags = input[I_FLAGS].max(0.0) as u32;
    let body = input[I_BODY];
    if flags & VANILLA != 0 {
        g.fresh = true;
        g.seen = now;
        return out;
    }
    let mut dt = (now - g.seen) as f32;
    if g.fresh || dt > 0.5 {
        *g = Gait { seen: now, yaw: body, grounded: true, ..Gait::default() };
        dt = 0.0;
    }
    g.seen = now;
    let dt = dt.clamp(0.0, 0.1);
    let st = STYLES[(input[I_STYLE].max(0.0) as usize).min(STYLES.len() - 1)];
    let (bounce, lean_k, arms_k, stride_k) = (input[I_BOUNCE].max(0.0), input[I_LEAN].max(0.0), input[I_ARMS].max(0.0), input[I_STRIDE].clamp(0.3, 2.0));
    let breathe = input[I_BREATH] > 0.5;
    let on_ground = flags & ON_GROUND != 0;

    // its movement in the body's frame: yaw 0 faces +z, the right hand toward -x
    let yaw = body.to_radians();
    let (vx, vz) = (input[I_VX] * 20.0, input[I_VZ] * 20.0);
    let fwd = -vx * yaw.sin() + vz * yaw.cos();
    let side = -vx * yaw.cos() - vz * yaw.sin();
    follow(&mut g.fwd, fwd, 10.0, dt);
    follow(&mut g.side, side, 10.0, dt);
    let speed = g.fwd.hypot(g.side);
    if dt > 0.0 {
        let turned = ((body - g.yaw + 540.0).rem_euclid(360.0) - 180.0).to_radians();
        follow(&mut g.yaw_rate, turned / dt, 8.0, dt);
        follow(&mut g.push, (speed - g.speed_was) / dt, 6.0, dt);
    }
    g.yaw = body;
    g.speed_was = speed;
    if speed > 0.3 {
        let to = g.side.atan2(g.fwd.abs());
        follow(&mut g.heading, to, 10.0, dt);
    }

    // in the air: the time it has been, the landing
    if !on_ground {
        g.air += dt;
        g.vy = input[I_VY];
    } else {
        if !g.grounded && g.air > 0.12 {
            let fall = (-g.vy * 20.0).clamp(0.0, 25.0);
            g.land.v -= fall * 0.09 * bounce.min(2.0);
        }
        g.air = 0.0;
    }
    g.grounded = on_ground;
    let air = smoothstep(0.08, 0.25, g.air);

    // running: sprinting, or going as fast as a sprint (another's sprint is not always known)
    let running = if flags & SPRINTING != 0 { smoothstep(3.8, 5.0, speed) } else { smoothstep(4.9, 5.5, speed) };
    follow(&mut g.run, running, 6.0, dt);
    follow(&mut g.crouch, if flags & CROUCHING != 0 { 1.0 } else { 0.0 }, 10.0, dt);
    let turning = smoothstep(0.6, 2.5, g.yaw_rate.abs()) * (1.0 - smoothstep(0.3, 1.0, speed));
    follow(&mut g.amp, smoothstep(0.08, 0.9, speed).max(0.45 * turning), 6.0, dt);

    // the stride: its pace grows with the speed as a person's does (about 2.5 strides a second at the game's walking
    // speed, 2.75 sprinting), the ground one stride covers is what the speed asks at that pace, and a leg sweeps as far
    // as a planted foot needs to cover it — so it never slides, starting, stopping or stepping round on the spot
    let duty = lerp(st.duty_walk, st.duty_run, g.run);
    let stepping = speed + turning * g.yaw_rate.abs() * 0.22;
    let pace = (2.55 + 0.33 * (stepping - WALK)).max(0.9) * st.cadence / stride_k;
    let reach_full = ((stepping / pace) * duty / (2.0 * LEG)).min(0.8).asin() * lerp(1.0, 0.85, g.crouch);
    let stride = (2.0 * LEG * reach_full.sin() / duty).max(0.05);
    let back = g.fwd < -0.05 && g.fwd.abs() > 0.5 * g.side.abs();
    let covered = stepping * dt * if back { -1.0 } else { 1.0 };
    if on_ground {
        g.phase = (g.phase + covered / stride).rem_euclid(1.0);
    }
    // stopping, the legs come together through a last step (on to the next foot under the body), not at once
    let shrink = if reach_full < g.reach { 5.0 } else { 14.0 };
    follow(&mut g.reach, reach_full, shrink, dt);
    if on_ground && stepping < 0.4 && g.reach > 0.03 {
        let mids = [duty * 0.5, (0.5 + duty * 0.5).rem_euclid(1.0)];
        let ahead = |m: f32| (m - g.phase).rem_euclid(1.0);
        let left = ahead(mids[0]).min(ahead(mids[1]));
        if left > 0.01 {
            g.phase = (g.phase + (1.6 * dt).min(left)).rem_euclid(1.0);
        }
    }
    let reach = g.reach;

    // the soles: where each is (planted, going back with the ground; swinging, through the air over it)
    let a = LEG * reach.sin();
    let clear = lerp(st.clear_walk, st.clear_run, g.run) * smoothstep(0.02, 0.3, reach) * lerp(1.0, 0.7, g.crouch);
    let (rx, ry, rs, ru) = sole(g.phase, duty, a, g.run, clear);
    let (lx, ly, ls, lu) = sole((g.phase + 0.5).rem_euclid(1.0), duty, a, g.run, clear);
    // the planted knee's bend: a little as it takes the weight, more running; crouched, low; standing, soft
    let knee_most = lerp(st.knee_walk, st.knee_run, g.run) * smoothstep(0.0, 0.25, reach);
    let soft = 0.10 + 0.5 * g.crouch;
    let knee_at = |u: f32| {
        let walk = (PI * (u / 0.6).min(1.0)).sin();
        let run = (PI * u).sin();
        soft + knee_most * lerp(walk, run, g.run)
    };
    // the hips: as the knee that most lately came down holds them; between two plantings (running) from one to the
    // other, a little up
    let mut hip = if rs >= 0.0 && (ls < 0.0 || rs <= ls) {
        hip_over(rx, knee_at(rs))
    } else if ls >= 0.0 {
        hip_over(lx, knee_at(ls))
    } else {
        let p = g.phase.rem_euclid(0.5);
        let f = ((p - duty) / (0.5 - duty).max(1e-3)).clamp(0.0, 1.0);
        let off = hip_over(-a, knee_at(1.0));
        let on = hip_over(a, knee_at(0.0));
        lerp(off, on, f) + 0.045 * g.run * bounce * (PI * f).sin()
    };
    // walking, toward the next foot's coming down the hips go down to where they will be then (the planted knee bending
    // as it does before it pushes off), so the change of foot is no jump
    if duty >= 0.5 {
        let step = (g.phase.rem_euclid(0.5) / 0.5).clamp(0.0, 1.0);
        let w = smoothstep(0.7, 1.0, step);
        if w > 0.0 {
            hip = lerp(hip, hip.min(hip_over(a, knee_at(0.0))), w);
        }
    }
    // a spring over it at each step (the style's)
    let stance_u = if rs >= 0.0 { rs } else if ls >= 0.0 { ls } else { -1.0 };
    if stance_u >= 0.0 {
        hip += st.hop * bounce * g.amp * (PI * stance_u).sin();
    }
    let mut down = hip - LEG;
    // each leg to its sole
    let (mut r, mut rk) = reach_to(rx, hip - ry);
    let (mut l, mut lk) = reach_to(lx, hip - ly);
    // tucked in the air, the knees up
    r = lerp(r, 0.55, air);
    rk = lerp(rk, 1.05, air);
    l = lerp(l, 0.05, air);
    lk = lerp(lk, 0.55, air);
    down *= 1.0 - air;
    let (hc, hs) = (g.heading.cos(), g.heading.sin());
    // the line from hip to sole goes the way it steps; the knee bends forward over it (the thigh forward half the bend,
    // the shin back the whole)
    let (rl, ll) = (r - rk * 0.5, l - lk * 0.5);
    out[O_RLEG_X] = -rl * hc - rk * 0.5;
    out[O_LLEG_X] = -ll * hc - lk * 0.5;
    let out_r = if ru >= 0.0 { st.swing_out * (PI * ru).sin() * g.amp } else { 0.0 };
    let out_l = if lu >= 0.0 { st.swing_out * (PI * lu).sin() * g.amp } else { 0.0 };
    out[O_RLEG_Z] = rl * hs + st.width + out_r;
    out[O_LLEG_Z] = ll * hs - st.width - out_l;
    out[O_RKNEE] = rk;
    out[O_LKNEE] = lk;
    let planted = on_ground && air < 0.5 && g.amp > 0.2;
    out[O_RPLANT] = if planted && rs >= 0.0 { 1.0 } else { 0.0 };
    out[O_LPLANT] = if planted && ls >= 0.0 { 1.0 } else { 0.0 };
    out[O_RSOLE_F] = rx * hc;
    out[O_RSOLE_R] = rx * hs;
    out[O_LSOLE_F] = lx * hc;
    out[O_LSOLE_R] = lx * hs;

    // over the planted foot, the shoulders against the stride
    let c = (TAU * (g.phase - duty * 0.5)).cos();
    let mut shift = st.sway * g.amp * c * (1.0 - 0.5 * g.run);
    let mut roll = st.rock * g.amp * c * (1.0 - 0.5 * g.run);
    let twist = st.twist * g.amp * lerp(1.0, 1.2, g.run) * ((rl - ll) / (2.0 * reach_full.max(1e-3))).clamp(-1.2, 1.2) * (1.0 - air);

    // the arms on their springs
    let cadence = speed / stride;
    let hz = st.arm_hz.max(cadence * 1.6);
    let swing = st.arms * arms_k * lerp(1.0, 1.25, g.run) * hc.abs();
    let bias = 0.25 * g.run + 0.25 * air;
    let ar = g.arm_r.go(-swing * rl + bias, hz, st.arm_damp, dt);
    let al = g.arm_l.go(-swing * ll + bias, hz, st.arm_damp, dt);
    out[O_RARM_X] = -ar;
    out[O_LARM_X] = -al;
    // the elbows: bent a little walking and more as the arm comes forward, a runner's near a right angle; in the air
    // and crouched a little more; on springs as the arms are
    let elbow_base = lerp(0.12, lerp(st.elbow_walk, st.elbow_run, g.run), g.amp.max(g.run)) + 0.3 * air + 0.2 * g.crouch;
    let forward = |arm: f32| (arm / 0.6).clamp(-1.0, 1.0);
    let more = lerp(0.45, 0.22, g.run);
    let er = g.elbow_r.go(elbow_base * (1.0 + more * forward(ar)), hz, 0.7, dt);
    let el = g.elbow_l.go(elbow_base * (1.0 + more * forward(al)), hz, 0.7, dt);
    out[O_RELBOW] = er.max(0.0);
    out[O_LELBOW] = el.max(0.0);
    let spread = (0.04 + 0.10 * g.run) * g.amp + 0.25 * air;
    out[O_RARM_Z] = spread;
    out[O_LARM_Z] = -spread;

    // the lean with speed and speeding up, the roll into a turn
    let lean_to = (st.lean_speed * g.fwd + st.lean_push * g.push.clamp(-8.0, 8.0) + 0.05 * g.run) * lean_k * (1.0 - 0.6 * g.crouch) * (1.0 - 0.5 * air);
    let lean = g.lean.go(lean_to, 2.2, 0.85, dt);
    let turn_to = (st.turn * g.yaw_rate * speed).clamp(-0.22, 0.22) * lean_k;
    roll += g.turn.go(turn_to, 2.5, 0.8, dt);
    down += g.land.go(0.0, 3.5, 0.45, dt).min(0.0);

    // standing: breath, the weight from foot to foot
    let still = (1.0 - g.amp) * if on_ground { 1.0 } else { 0.0 };
    let mut chest = 0.0;
    if still > 0.01 {
        g.idle += dt;
        let w = (TAU * 0.061 * g.idle + 1.3).sin() * 0.6 + (TAU * 0.113 * g.idle).sin() * 0.4;
        shift += 0.018 * w * still;
        roll += 0.02 * w * still;
        if breathe {
            let b = (TAU * 0.23 * g.idle).sin();
            chest = -0.02 * b * still;
            out[O_RARM_Z] += 0.012 * b * still;
            out[O_LARM_Z] -= 0.012 * b * still;
        }
    } else {
        g.idle = 0.0;
    }

    out[O_ON] = 1.0;
    out[O_TWIST] = twist;
    out[O_CHEST] = chest;
    out[O_HEAD_X] = -lean - chest;
    out[O_HEAD_Y] = -twist;
    out[O_HEAD_Z] = -roll;
    out[O_DOWN] = down;
    out[O_LEAN] = lean;
    out[O_ROLL] = roll;
    out[O_SHIFT] = shift;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(key: f32, vx: f32, vz: f32, flags: u32) -> [f32; IN] {
        let mut v = [0.0; IN];
        v[I_KEY] = key;
        v[I_VX] = vx;
        v[I_VZ] = vz;
        v[I_FLAGS] = flags as f32;
        v[I_BOUNCE] = 1.0;
        v[I_LEAN] = 1.0;
        v[I_ARMS] = 1.0;
        v[I_STRIDE] = 1.0;
        v
    }

    /// The soles' heights over the ground and how far ahead of the hips they are (blocks), from a pose (walking straight
    /// on: the thigh's angle is its xRot's, the shin's that less the knee's bend).
    fn soles(o: &[f32; OUT]) -> [(f32, f32); 2] {
        let hip = LEG + o[O_DOWN];
        let half = LEG * 0.5;
        let foot = |x: f32, k: f32| {
            let t = -x;
            let down = half * t.cos() + half * (t - k).cos();
            (hip - down, half * t.sin() + half * (t - k).sin())
        };
        [foot(o[O_RLEG_X], o[O_RKNEE]), foot(o[O_LLEG_X], o[O_LKNEE])]
    }

    /// Walking straight on at the game's speed (south, +z), frame by frame: the gait's frames after it got going.
    fn walk(key: f32, frames: usize, dt: f64) -> Vec<[f32; OUT]> {
        let mut out = Vec::new();
        let v = input(key, 0.0, WALK / 20.0, ON_GROUND);
        {
            let mut all = GAITS.lock().unwrap();
            all.get_or_insert_with(HashMap::new).remove(&(key as i64));
        }
        for i in 0..frames {
            // the clock moved by hand: the gait's own time stepped as frames are
            {
                let mut all = GAITS.lock().unwrap();
                if let Some(g) = all.get_or_insert_with(HashMap::new).get_mut(&(key as i64)) {
                    g.seen -= dt;
                }
            }
            let o = pose(&v);
            if i > frames / 2 {
                out.push(o);
            }
        }
        out
    }

    #[test]
    fn a_planted_foot_neither_sinks_nor_slides() {
        let dt = 1.0 / 60.0;
        let frames = walk(1.0, 600, dt);
        let mut slid = 0.0f32;
        let mut covered = 0.0f32;
        for w in frames.windows(2) {
            let (a, b) = (soles(&w[0]), soles(&w[1]));
            for k in 0..2 {
                // no sole under the ground
                assert!(b[k].0 > -2e-3, "a sole {} under the ground", b[k].0);
                // the sole on the ground stays where it is on the ground (its distance ahead of the hips falls by
                // the ground the body covered)
                if a[k].0.abs() < 1e-3 && b[k].0.abs() < 1e-3 {
                    slid += ((b[k].1 - a[k].1) + WALK * dt as f32).abs();
                    covered += WALK * dt as f32;
                }
            }
        }
        assert!(covered > 1.0, "never planted");
        assert!(slid / covered < 0.05, "slid {slid} over {covered}");
    }

    /// The ranges of a steady walk and run, printed (cargo test motion::tests::ranges -- --nocapture).
    #[test]
    fn ranges() {
        for (name, speed, flags) in [("walk", WALK, ON_GROUND), ("run", 5.612, ON_GROUND | SPRINTING)] {
            let key = if name == "walk" { 10.0 } else { 11.0 };
            let mut v = input(key, 0.0, speed / 20.0, flags);
            v[I_STYLE] = 0.0;
            let mut lo = [f32::MAX; OUT];
            let mut hi = [f32::MIN; OUT];
            for i in 0..900 {
                {
                    let mut all = GAITS.lock().unwrap();
                    if let Some(g) = all.get_or_insert_with(HashMap::new).get_mut(&(key as i64)) {
                        g.seen -= 1.0 / 60.0;
                    }
                }
                let o = pose(&v);
                if i > 600 {
                    for k in 0..OUT {
                        lo[k] = lo[k].min(o[k]);
                        hi[k] = hi[k].max(o[k]);
                    }
                }
            }
            let names = ["on", "rleg x", "rleg z", "lleg x", "lleg z", "rarm x", "rarm z", "larm x", "larm z", "twist", "chest", "head x", "head y", "head z", "down", "lean", "roll", "shift",
                "rknee", "lknee", "relbow", "lelbow", "rplant", "lplant", "rsole f", "rsole r", "lsole f", "lsole r"];
            for k in 1..OUT {
                println!("{name} {:7}: {:7.3} .. {:7.3} ({:5.1} deg span)", names[k], lo[k], hi[k], (hi[k] - lo[k]).to_degrees());
            }
        }
    }

    #[test]
    fn standing_it_stays_upright_and_breathes() {
        let key = 2.0;
        let mut v = input(key, 0.0, 0.0, ON_GROUND);
        v[I_BREATH] = 1.0;
        let mut last = [0.0; OUT];
        for _ in 0..240 {
            {
                let mut all = GAITS.lock().unwrap();
                if let Some(g) = all.get_or_insert_with(HashMap::new).get_mut(&(key as i64)) {
                    g.seen -= 1.0 / 60.0;
                }
            }
            last = pose(&v);
        }
        assert_eq!(last[O_ON], 1.0);
        // the soles under the hips, on the ground (the knees soft)
        for (h, x) in soles(&last) {
            assert!(h.abs() < 0.005 && x.abs() < 0.02, "a sole at {h} {x}");
        }
        assert!(last[O_DOWN].abs() < 0.01 && last[O_LEAN].abs() < 0.01);
    }

    #[test]
    fn swimming_and_the_rest_are_the_games() {
        let o = pose(&input(3.0, 0.1, 0.1, VANILLA));
        assert_eq!(o[O_ON], 0.0);
    }
}
