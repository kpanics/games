//! Item Physic (her word, 23.10: "outdo the free Cursed's a thousand times — every little thing"): a dropped item is a
//! body. Each copy of a stack (the game draws one to five) is a rigid body of the item's own shape — a box as its
//! model is, a disc for round things — with its weight and its turning inertia; it is thrown turning as it was thrown,
//! and on the ground it meets the blocks' real tops (a slab, a carpet, a step: Java hands over a grid of heights round
//! the item) at its corners or its rim: it bounces as its stuff does (metal rings, wool does not), slides as the block
//! lets it (on ice it skates and spins), tips over an edge, rocks on a corner and settles on a face, a coin wobbles
//! down; a stack falls apart into a little heap, one copy on another; on water it floats as deep as it is light,
//! rocking with the waves, in lava higher and tossed. Where the item is stays the game's (the server's): the bodies keep
//! near it on a soft leash, so a slide the game makes drags them along (they turn and roll as they go), and they drop
//! when it drops. A walker going through kicks them. What happened (a landing, a splash) goes back to Java for the dust,
//! the drops and the sound. The game's clock drives it (paused, all stands still).

use std::collections::HashMap;
use std::f32::consts::{PI, TAU};
use std::sync::Mutex;

// ------------------------------------------------------------------------------------------------ what Java says

/// The numbers Java hands over for an item each frame (its place is a double array of its own: x, y, z).
pub const IN: usize = 56;
pub const I_ID: usize = 0;
pub const I_FLAGS: usize = 1;
pub const I_SHAPE: usize = 2;
pub const I_STUFF: usize = 3;
pub const I_COUNT: usize = 4;
/// Its movement (blocks a tick: x, y, z).
pub const I_VEL: usize = 5;
/// Its age in ticks with the part of the tick: the clock.
pub const I_AGE: usize = 8;
/// Its model's box (blocks: min x, y, z, max x, y, z).
pub const I_BOX: usize = 9;
/// The water's (or lava's) top over the item's place (blocks; with IN_FLUID).
pub const I_FLUID: usize = 15;
/// How slippery the block it lies on is (the game's friction: 0.6 most, 0.98 ice).
pub const I_SLIP: usize = 16;
/// The ground's tops round the item, over its place's height (blocks): 5 × 5 at GRID apart, x then z.
pub const I_GROUND: usize = 17;
/// A walker going through: where (x, z from the item), its movement (blocks a second: x, z), whether there is one.
pub const I_KICK: usize = 42;
/// The settings: bounce (×), spin (×), piles (0/1).
pub const I_BOUNCE: usize = 47;
pub const I_SPIN: usize = 48;
pub const I_PILES: usize = 49;
/// The item's id's high bits (I_ID its low 16: a float holds whole numbers only to 16.7 million).
pub const I_ID_HI: usize = 50;

pub const ON_GROUND: u32 = 1;
pub const IN_FLUID: u32 = 2;
pub const IN_LAVA: u32 = 4;

/// What it is like (Java tells them apart): a flat thing (a sprite), a solid (a block, anything with depth), a round
/// flat thing (rolls on its rim: a clock, a disc, a pearl), a long thing (a tool, a stick, a bottle).
pub const FLAT: u32 = 0;
pub const SOLID: u32 = 1;
pub const ROUND: u32 = 2;
pub const LONG: u32 = 3;

/// What it is made of, for its bounce and its sound: soft, wood, stone, metal, glass.
pub const SOFT: u32 = 0;
pub const WOOD: u32 = 1;
pub const STONE: u32 = 2;
pub const METAL: u32 = 3;
pub const GLASS: u32 = 4;

pub const GRID: f32 = 0.25;
const GRID_N: usize = 5;

/// What goes back: how many copies, each copy's middle (from the item's place) and turn (a quaternion: x, y, z, w);
/// then the frame's landing (how hard, where from the item's place), its splash (how hard), how deep it is in.
pub const MAX_COPIES: usize = 5;
pub const OUT: usize = 1 + MAX_COPIES * 7 + 6;
pub const O_IMPACT: usize = 1 + MAX_COPIES * 7;
pub const O_SPLASH: usize = O_IMPACT + 4;
pub const O_SUNK: usize = O_IMPACT + 5;

// ------------------------------------------------------------------------------------------------ the maths

type V3 = [f32; 3];

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn mul(a: V3, k: f32) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn len(a: V3) -> f32 {
    dot(a, a).sqrt()
}
fn unit(a: V3) -> V3 {
    let l = len(a);
    if l < 1e-9 { [0.0, 1.0, 0.0] } else { mul(a, 1.0 / l) }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Quat {
    x: f32,
    y: f32,
    z: f32,
    w: f32,
}

impl Quat {
    const ID: Quat = Quat { x: 0.0, y: 0.0, z: 0.0, w: 1.0 };

    fn around(axis: V3, angle: f32) -> Quat {
        let a = unit(axis);
        let (s, c) = (angle * 0.5).sin_cos();
        Quat { x: a[0] * s, y: a[1] * s, z: a[2] * s, w: c }
    }

    fn then(self, o: Quat) -> Quat {
        // self after o
        Quat {
            w: self.w * o.w - self.x * o.x - self.y * o.y - self.z * o.z,
            x: self.w * o.x + self.x * o.w + self.y * o.z - self.z * o.y,
            y: self.w * o.y - self.x * o.z + self.y * o.w + self.z * o.x,
            z: self.w * o.z + self.x * o.y - self.y * o.x + self.z * o.w,
        }
    }

    fn unit(self) -> Quat {
        let l = (self.x * self.x + self.y * self.y + self.z * self.z + self.w * self.w).sqrt().max(1e-9);
        Quat { x: self.x / l, y: self.y / l, z: self.z / l, w: self.w / l }
    }

    fn conj(self) -> Quat {
        Quat { x: -self.x, y: -self.y, z: -self.z, w: self.w }
    }

    fn rotate(self, v: V3) -> V3 {
        let u = [self.x, self.y, self.z];
        let t = mul(cross(u, v), 2.0);
        add(add(v, mul(t, self.w)), cross(u, t))
    }

    /// Turned on by `w` (radians a second, world) for `h` seconds.
    fn spun(self, w: V3, h: f32) -> Quat {
        let a = len(w) * h;
        if a < 1e-7 {
            return self;
        }
        Quat::around(w, a).then(self).unit()
    }

    /// The shortest turn taking the direction `a` to `b`.
    fn arc(a: V3, b: V3) -> Quat {
        let (a, b) = (unit(a), unit(b));
        let d = dot(a, b);
        if d > 0.999_999 {
            return Quat::ID;
        }
        if d < -0.999_999 {
            let side = if a[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 0.0, 1.0] };
            return Quat::around(cross(a, side), PI);
        }
        let c = cross(a, b);
        Quat { x: c[0], y: c[1], z: c[2], w: 1.0 + d }.unit()
    }
}

fn mix(seed: u32) -> u32 {
    let mut x = seed;
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

/// The `k`th number of a seed's, 0..1.
fn chance(seed: u32, k: u32) -> f32 {
    (mix(seed ^ mix(k.wrapping_add(0x9e37_79b9))) >> 8) as f32 / (1u32 << 24) as f32
}

// ------------------------------------------------------------------------------------------------ the stuff

/// How it bounces (its restitution) and how its own weight goes (relative: metal heavy, wool light).
fn stuff(of: u32) -> (f32, f32) {
    match of {
        SOFT => (0.05, 0.6),
        WOOD => (0.22, 0.8),
        STONE => (0.18, 1.3),
        METAL => (0.34, 1.6),
        GLASS => (0.28, 1.1),
        _ => (0.15, 1.0),
    }
}

/// The game's falling: 0.04 blocks a tick each tick (blocks a second²).
const GRAVITY: f32 = 16.0;
/// The step the bodies are worked out in (seconds).
const STEP: f32 = 1.0 / 240.0;
/// How far a copy may lie from its spot in the heap before the leash pulls it (blocks).
const LEASH_SLACK: f32 = 0.16;

// ------------------------------------------------------------------------------------------------ a body

#[derive(Clone, Copy, Debug)]
struct Body {
    /// its middle (blocks, from the item's anchor), its pace, its turn, its turning (radians a second, world)
    p: V3,
    v: V3,
    q: Quat,
    w: V3,
    /// where it keeps to by the item's place (its own spot in a heap)
    home: V3,
    /// still this long (it sleeps after a while still), asleep
    still: f32,
    asleep: bool,
    kicked: f32,
}

/// An item's bodies as they go.
struct Item {
    seen: f64,
    age: f32,
    /// where the item was when first seen (the bodies' frame), where it is now in it
    anchor: [f64; 3],
    at: V3,
    /// on the leash of the item's place, or carried with it in the air as one
    free: bool,
    bodies: Vec<Body>,
    seed: u32,
    vel: V3,
    wet: bool,
    landed_at: f32,
    sound_at: f32,
    /** what it is like, its copies' half size (the others meet it by them) */
    kind: u32,
    half: V3,
}

/// Another item's copy lying by one (in its frame): it meets it as a box standing still.
struct Other {
    p: V3,
    q: Quat,
    half: V3,
    feel: Vec<V3>,
}

/// The model's box: its middle and half its size.
fn shape_of(input: &[f32; IN]) -> (V3, V3) {
    let b = &input[I_BOX..I_BOX + 6];
    let mid = [(b[0] + b[3]) * 0.5, (b[1] + b[4]) * 0.5, (b[2] + b[5]) * 0.5];
    let half = [((b[3] - b[0]) * 0.5).max(0.02), ((b[4] - b[1]) * 0.5).max(0.02), ((b[5] - b[2]) * 0.5).max(0.012)];
    (mid, half)
}

/// The points a body touches with (its own frame): a box's corners; a disc's rim, rounded (its middle sticks out past
/// its faces' edges: on its rim it cannot stand, it rolls or falls over, as a coin).
fn points(kind: u32, half: V3) -> Vec<V3> {
    if kind == ROUND {
        let r = half[0].min(half[1]) * 0.92;
        let e = half[2] * 1.5;
        let mut v = Vec::with_capacity(48);
        for k in 0..16 {
            let a = k as f32 / 16.0 * TAU;
            v.push([a.cos() * r, a.sin() * r, 0.0]);
            for s in [-1.0, 1.0] {
                v.push([a.cos() * (r - e), a.sin() * (r - e), s * half[2]]);
            }
        }
        v
    } else {
        let mut v = Vec::with_capacity(8);
        for c in 0..8 {
            v.push([if c & 1 == 0 { -half[0] } else { half[0] }, if c & 2 == 0 { -half[1] } else { half[1] }, if c & 4 == 0 { -half[2] } else { half[2] }]);
        }
        v
    }
}

/// The points a copy feels another with: its corners (or rim), and a box's edges along (two crossing edges have no
/// corner in each other); a flat thing's faces too (two leaves crossing touch nowhere near their edges).
fn feelers(kind: u32, half: V3) -> Vec<V3> {
    let mut v = points(kind, half);
    if kind == FLAT {
        for s in [-1.0f32, 1.0] {
            for i in 0..4 {
                for k in 0..4 {
                    v.push([(i as f32 / 3.0 * 2.0 - 1.0) * half[0] * 0.75, (k as f32 / 3.0 * 2.0 - 1.0) * half[1] * 0.75, s * half[2]]);
                }
            }
        }
    }
    if kind != ROUND {
        for axis in 0..3 {
            for c in 0..4 {
                for t in [-0.5f32, 0.0, 0.5] {
                    let mut p = [0.0; 3];
                    let (u, w) = ((axis + 1) % 3, (axis + 2) % 3);
                    p[axis] = t * half[axis];
                    p[u] = if c & 1 == 0 { -half[u] } else { half[u] };
                    p[w] = if c & 2 == 0 { -half[w] } else { half[w] };
                    v.push(p);
                }
            }
        }
    }
    v
}

/// Its inertia (its own frame, for a weight of 1): a box's, a disc's.
fn inertia(kind: u32, half: V3) -> V3 {
    let s = mul(half, 2.0);
    if kind == ROUND {
        let r = half[0].min(half[1]);
        let i = r * r / 4.0 + s[2] * s[2] / 12.0;
        [i, i, r * r / 2.0]
    } else {
        [(s[1] * s[1] + s[2] * s[2]) / 12.0, (s[0] * s[0] + s[2] * s[2]) / 12.0, (s[0] * s[0] + s[1] * s[1]) / 12.0]
    }
}

/// The world's inverse inertia times `t` for a body turned by `q`.
fn inv_inertia(q: Quat, inv: V3, t: V3) -> V3 {
    let local = q.conj().rotate(t);
    q.rotate([local[0] * inv[0], local[1] * inv[1], local[2] * inv[2]])
}

/// The ground's top at (x, z) from the item's place, over its height: the grid's, between its points.
fn ground_at(grid: &[f32], x: f32, z: f32) -> f32 {
    let c = (GRID_N as f32 - 1.0) * 0.5;
    let fx = (x / GRID + c).clamp(0.0, GRID_N as f32 - 1.001);
    let fz = (z / GRID + c).clamp(0.0, GRID_N as f32 - 1.001);
    let (ix, iz) = (fx.floor() as usize, fz.floor() as usize);
    let (tx, tz) = (fx - ix as f32, fz - iz as f32);
    let g = |i: usize, k: usize| grid[k * GRID_N + i];
    let a = g(ix, iz) + (g(ix + 1, iz) - g(ix, iz)) * tx;
    let b = g(ix, iz + 1) + (g(ix + 1, iz + 1) - g(ix, iz + 1)) * tx;
    a + (b - a) * tz
}

/// A touch to work out: which bodies (b none: the ground), where on each (from its middle), the way it pushes a out,
/// how deep, the friction, the bounce, the impulses gathered (the pace's, the push-out's).
struct Touch {
    a: usize,
    b: Option<usize>,
    ra: V3,
    rb: V3,
    n: V3,
    depth: f32,
    mu: f32,
    bounce: f32,
    jn: f32,
    jt: [f32; 2],
    t1: V3,
    t2: V3,
    target: f32,
    out: f32,
    jp: f32,
}

static ITEMS: Mutex<Option<HashMap<i32, Item>>> = Mutex::new(None);

/// The item's id, whole, from its two halves.
fn item_id(input: &[f32; IN]) -> i32 {
    (input[I_ID].max(0.0) as i64 + ((input[I_ID_HI].max(0.0) as i64) << 16)) as i32
}

fn clock() -> f64 {
    use std::sync::OnceLock;
    use std::time::Instant;
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f64()
}

/// The copies' spots in a heap (from the item's place, flat on the ground; the bodies keep to them on the leash): blocks
/// side by side in a triangle, the next ones over the hollows between them (they come to lie on them); flat things a
/// little off one another (they lie one on another, as a stack of papers does).
fn heap(n: usize, seed: u32, half: V3, kind: u32) -> Vec<V3> {
    const SPOTS: [[f32; 2]; 5] = [[0.0, 0.0], [1.04, 0.0], [0.52, 0.92], [0.52, 0.31], [0.78, 0.46]];
    let size = if kind == SOLID { 2.0 * half[0].max(half[2]) } else { half[0].max(half[1]) * 0.55 };
    let turn = chance(seed, 39) * TAU;
    let (s, c) = turn.sin_cos();
    let used = &SPOTS[..n.min(5)];
    let (mx, mz) = if kind == SOLID && n > 1 {
        let k = n.min(3) as f32;
        (used.iter().take(3).map(|p| p[0]).sum::<f32>() / k, used.iter().take(3).map(|p| p[1]).sum::<f32>() / k)
    } else {
        (0.0, 0.0)
    };
    used.iter()
        .enumerate()
        .map(|(k, p)| {
            let jitter = if kind == SOLID { 0.04 } else { 0.35 };
            let x = (p[0] - mx + (chance(seed, 40 + k as u32) - 0.5) * jitter) * size;
            let z = (p[1] - mz + (chance(seed, 50 + k as u32) - 0.5) * jitter) * size;
            [x * c - z * s, 0.0, x * s + z * c]
        })
        .collect()
}

/// A turn's axes: its own x, y, z in the world.
fn axes(q: Quat) -> [V3; 3] {
    [q.rotate([1.0, 0.0, 0.0]), q.rotate([0.0, 1.0, 0.0]), q.rotate([0.0, 0.0, 1.0])]
}

/// A body lying on its broadest face, turned `yaw` about the up.
fn lying(kind: u32, half: V3, yaw: f32) -> Quat {
    // the broadest face down: the axis with the least half-size is the up
    let thin = if kind == ROUND || kind == FLAT || kind == LONG {
        2
    } else if half[0] <= half[1] && half[0] <= half[2] {
        0
    } else if half[1] <= half[2] {
        1
    } else {
        2
    };
    let mut axis = [0.0; 3];
    axis[thin] = 1.0;
    Quat::around([0.0, 1.0, 0.0], yaw).then(Quat::arc(axis, [0.0, 1.0, 0.0]))
}

/// How far a body turned `q` reaches below its middle.
fn reach_down(kind: u32, half: V3, q: Quat) -> f32 {
    points(kind, half).iter().map(|p| -q.rotate(*p)[1]).fold(0.0, f32::max)
}

impl Item {
    fn new(input: &[f32; IN], at: [f64; 3], now: f64) -> Item {
        let id = item_id(input);
        let seed = mix(id as u32 ^ 0x5bd1_e995);
        let kind = input[I_SHAPE] as u32;
        let (_, half) = shape_of(input);
        let flags = input[I_FLAGS] as u32;
        let ground = flags & ON_GROUND != 0;
        let vel = mul([input[I_VEL], input[I_VEL + 1], input[I_VEL + 2]], 20.0);
        let n = (input[I_COUNT] as usize).clamp(1, MAX_COPIES);
        let homes = heap(n, seed, half, kind);
        let settled = ground && input[I_AGE] > 20.0 && len(vel) < 0.3;
        let grid = &input[I_GROUND..I_GROUND + GRID_N * GRID_N];
        let mut bodies = Vec::with_capacity(n);
        for (k, home) in homes.iter().enumerate() {
            let yaw = chance(seed, 60 + k as u32) * TAU;
            let (q, p, asleep) = if settled {
                // lying there already when it came into sight: as it fell, no show of falling
                let q = lying(kind, half, yaw);
                let floor = ground_at(grid, home[0], home[2]);
                let stack = if input[I_PILES] > 0.5 { k as f32 * 0.0 } else { 0.0 };
                (q, [home[0], floor + reach_down(kind, half, q) + stack, home[2]], true)
            } else {
                // as the game had it: upright, turned by its own spin
                let q = Quat::around([0.0, 1.0, 0.0], yaw * 0.2 + input[I_AGE] / 20.0);
                (q, [home[0] * 0.3, reach_down(kind, half, q) + 0.0625, home[2] * 0.3], false)
            };
            bodies.push(Body { p, v: vel, q, w: [0.0; 3], home: *home, still: 0.0, asleep, kicked: 0.0 });
        }
        let mut item = Item {
            seen: now,
            age: input[I_AGE],
            anchor: at,
            at: [0.0; 3],
            free: settled,
            bodies,
            seed,
            vel,
            wet: flags & IN_FLUID != 0,
            landed_at: -100.0,
            sound_at: -100.0,
            kind,
            half,
        };
        if settled && input[I_PILES] > 0.5 && n > 1 {
            // a heap lying there: let it settle a moment unseen, so the copies lie on each other
            for b in &mut item.bodies {
                b.asleep = false;
            }
            let mut out = [0.0; OUT];
            for _ in 0..160 {
                item.step(input, STEP, [0.0; 3], &mut out, &[]);
            }
        }
        if !settled && !ground {
            let spin = input[I_SPIN].max(0.0);
            for (k, b) in item.bodies.iter_mut().enumerate() {
                b.w = launch(vel, seed ^ k as u32, spin);
            }
        }
        item
    }

    /// One step of `h` seconds: forces, touches, impulses, moves; `slide`: how far the item's place went in it (the bodies
    /// on the ground go with it, as on a moving floor, and turn as things dragged do).
    fn step(&mut self, input: &[f32; IN], h: f32, slide: V3, out: &mut [f32; OUT], others: &[Other]) {
        let kind = input[I_SHAPE] as u32;
        let (_, half) = shape_of(input);
        let flags = input[I_FLAGS] as u32;
        let grid = &input[I_GROUND..I_GROUND + GRID_N * GRID_N];
        let (bounce, weight) = stuff(input[I_STUFF] as u32);
        let bounce = bounce * input[I_BOUNCE].max(0.0);
        let inv_mass = 1.0 / weight;
        let inv_i = {
            let i = inertia(kind, half);
            [1.0 / (i[0] * weight), 1.0 / (i[1] * weight), 1.0 / (i[2] * weight)]
        };
        // the block's friction: most 0.6 (grippy), ice 0.98 (skating)
        let slip = input[I_SLIP].clamp(0.4, 0.999);
        let mu = ((1.0 - slip) * 1.45).clamp(0.02, 0.75);
        let wet = flags & IN_FLUID != 0;
        let lava = flags & IN_LAVA != 0;
        let surface = input[I_FLUID];
        let time = self.age / 20.0;
        let pts = points(kind, half);
        let feel = feelers(kind, half);
        let at = self.at;
        let ground = |x: f32, z: f32| ground_at(grid, x - at[0], z - at[2]) + at[1];

        // forces: falling, the leash to the item's place, the water holding it up
        for b in self.bodies.iter_mut() {
            if b.asleep {
                continue;
            }
            b.v[1] -= GRAVITY * h;
            // the leash: slack near its spot (a copy lies where it fell, by another or on it, and does not creep),
            // pulling past it
            let target = add(at, b.home);
            let d = [target[0] - b.p[0], 0.0, target[2] - b.p[2]];
            let far = len(d);
            if far > LEASH_SLACK {
                let k: f32 = 38.0;
                let c = 2.0 * k.sqrt() * 0.9 * 0.35;
                let pull = mul(d, k * (far - LEASH_SLACK) / far);
                let ease = ((far - LEASH_SLACK) / 0.1).min(1.0);
                b.v[0] += (pull[0] - b.v[0] * c * ease) * h;
                b.v[2] += (pull[2] - b.v[2] * c * ease) * h;
            }
            if wet {
                // afloat: each point under the top lifts, so it rocks with the waves; lava holds it higher
                let dens = if lava { 3.4 } else { 2.2 };
                let mut lift = [0.0f32; 3];
                let mut turn = [0.0f32; 3];
                let mut under = 0.0;
                for p in &pts {
                    let r = b.q.rotate(*p);
                    let w = add(b.p, r);
                    let wave = 0.028 * (time * 1.7 + w[0] * 2.9 + w[2] * 0.7).sin() + 0.02 * (time * 2.3 + w[2] * 2.6 - w[0] * 1.1).sin();
                    let top = at[1] + surface + wave;
                    let depth = (top - w[1]).clamp(0.0, half[1].max(half[0]) * 2.0);
                    if depth > 0.0 {
                        let f = GRAVITY * dens * depth / (pts.len() as f32 * half[2].max(0.03) * 2.0).max(0.05);
                        let f = f.min(GRAVITY * 3.0 / pts.len() as f32 * 2.0);
                        lift[1] += f;
                        turn = add(turn, cross(r, [0.0, f, 0.0]));
                        under += 1.0;
                    }
                }
                b.v = add(b.v, mul(lift, h));
                b.w = add(b.w, mul(inv_inertia(b.q, inv_i, turn), h * 0.6 * weight));
                let drag = if under > 0.0 { if lava { 4.0 } else { 2.6 } } else { 0.0 };
                b.v = mul(b.v, (-drag * h).exp());
                b.w = mul(b.w, (-drag * 0.8 * h).exp());
                if lava {
                    // lava tosses it about as it burns
                    let k = (time * 7.0) as u32;
                    b.w = add(b.w, mul([chance(self.seed, k) - 0.5, 0.0, chance(self.seed, k + 7) - 0.5], 30.0 * h));
                }
            }
        }

        // the touches: each body's points with the ground under them, with the other bodies
        let mut touches: Vec<Touch> = Vec::new();
        for (i, b) in self.bodies.iter().enumerate() {
            if b.asleep {
                continue;
            }
            for p in &pts {
                let r = b.q.rotate(*p);
                let w = add(b.p, r);
                let g = ground(w[0], w[2]);
                let depth = g - w[1];
                if depth > -0.004 {
                    touches.push(touch(i, None, r, [0.0; 3], [0.0, 1.0, 0.0], depth, mu, bounce));
                }
            }
        }
        if self.bodies.len() > 1 {
            for i in 0..self.bodies.len() {
                for j in i + 1..self.bodies.len() {
                    let (a, b) = (&self.bodies[i], &self.bodies[j]);
                    if a.asleep && b.asleep {
                        continue;
                    }
                    let d = sub(a.p, b.p);
                    if len(d) > len(half) * 2.2 {
                        continue;
                    }
                    // the way the two boxes overlap least (of their faces' six), a out along it: so two copies side by
                    // side at one height, or one right in the other, part sideways, and one lying on another stays on it
                    let (ea, eb) = (axes(a.q), axes(b.q));
                    let mut over = f32::MAX;
                    let mut n = ea[0];
                    for l in ea.iter().chain(eb.iter()) {
                        let ra: f32 = (0..3).map(|k| (half[k] * dot(ea[k], *l)).abs()).sum();
                        let rb: f32 = (0..3).map(|k| (half[k] * dot(eb[k], *l)).abs()).sum();
                        let o = ra + rb - dot(d, *l).abs();
                        if o < over {
                            over = o;
                            n = if dot(d, *l) >= 0.0 { *l } else { mul(*l, -1.0) };
                        }
                    }
                    if over <= -0.003 {
                        continue;
                    }
                    // each one's points in (or on) the other's box touch it, as deep as they are past its side that way
                    for (x, y, s) in [(i, j, 1.0), (j, i, -1.0)] {
                        let (bx, by) = (&self.bodies[x], &self.bodies[y]);
                        let out = mul(n, s);
                        let ey = axes(by.q);
                        let side: f32 = (0..3).map(|k| (half[k] * dot(ey[k], out)).abs()).sum();
                        for p in &feel {
                            let r = bx.q.rotate(*p);
                            let w = add(bx.p, r);
                            let local = by.q.conj().rotate(sub(w, by.p));
                            if (0..3).all(|k| local[k].abs() <= half[k] + 0.003) {
                                let depth = (side - dot(sub(w, by.p), out)).clamp(0.0, over.max(0.0));
                                touches.push(touch(x, Some(y), r, sub(w, by.p), out, depth, 0.45, bounce * 0.6));
                            }
                        }
                    }
                }
            }
        }

        // the other items' copies by it, as boxes standing still (each meets the other when it is worked out)
        for (i, b) in self.bodies.iter().enumerate() {
            if b.asleep {
                continue;
            }
            for o in others {
                let d = sub(b.p, o.p);
                if len(d) > len(half) + len(o.half) + 0.01 {
                    continue;
                }
                let (ea, eb) = (axes(b.q), axes(o.q));
                let mut over = f32::MAX;
                let mut n = ea[0];
                for l in ea.iter().chain(eb.iter()) {
                    let ra: f32 = (0..3).map(|k| (half[k] * dot(ea[k], *l)).abs()).sum();
                    let rb: f32 = (0..3).map(|k| (o.half[k] * dot(eb[k], *l)).abs()).sum();
                    let ov = ra + rb - dot(d, *l).abs();
                    if ov < over {
                        over = ov;
                        n = if dot(d, *l) >= 0.0 { *l } else { mul(*l, -1.0) };
                    }
                }
                if over <= -0.003 {
                    continue;
                }
                // its side toward this one, this one's toward it
                let side_o: f32 = (0..3).map(|k| (o.half[k] * dot(eb[k], n)).abs()).sum();
                let side_b: f32 = (0..3).map(|k| (half[k] * dot(ea[k], n)).abs()).sum();
                for p in &feel {
                    let r = b.q.rotate(*p);
                    let w = add(b.p, r);
                    let local = o.q.conj().rotate(sub(w, o.p));
                    if (0..3).all(|k| local[k].abs() <= o.half[k] + 0.003) {
                        let depth = (side_o - dot(sub(w, o.p), n)).clamp(0.0, over.max(0.0));
                        touches.push(touch(i, None, r, [0.0; 3], n, depth, 0.45, bounce * 0.6));
                    }
                }
                for p in &o.feel {
                    let w = add(o.p, o.q.rotate(*p));
                    let local = b.q.conj().rotate(sub(w, b.p));
                    if (0..3).all(|k| local[k].abs() <= half[k] + 0.003) {
                        let depth = (side_b + dot(sub(w, b.p), n)).clamp(0.0, over.max(0.0));
                        touches.push(touch(i, None, sub(w, b.p), [0.0; 3], n, depth, 0.45, bounce * 0.6));
                    }
                }
            }
        }

        // the impulses, again and again: no point goes in, the friction holds, a bounce as hard as the stuff gives
        let mut hardest = 0.0f32;
        let mut hard_at = [0.0f32; 3];
        for t in touches.iter_mut() {
            let a = &self.bodies[t.a];
            let va = add(a.v, cross(a.w, t.ra));
            let vb = match t.b {
                Some(j) => add(self.bodies[j].v, cross(self.bodies[j].w, t.rb)),
                None => [0.0; 3],
            };
            let vn = dot(sub(va, vb), t.n);
            if t.b.is_none() && -vn > hardest {
                hardest = -vn;
                hard_at = add(a.p, t.ra);
            }
            // a bounce only when it comes in hard (resting contacts do not jitter)
            t.target = if -vn > 1.1 { -vn * t.bounce } else { 0.0 };
            // what went in comes out apart from the pace (it moves the body this step only: a thing at rest gathers no
            // speed from it, so it does not jitter), gently (copies one in another part at a walk, not flung)
            let most = if t.b.is_some() { 0.9 } else { 2.5 };
            t.out = ((t.depth - 0.002).max(0.0) * 0.4 / h).min(most);
        }
        for _ in 0..10 {
            for t in touches.iter_mut() {
                let (a, b) = (t.a, t.b);
                let va = add(self.bodies[a].v, cross(self.bodies[a].w, t.ra));
                let vb = match b {
                    Some(j) => add(self.bodies[j].v, cross(self.bodies[j].w, t.rb)),
                    None => [0.0; 3],
                };
                let rel = sub(va, vb);
                // along the push
                let k_a = give(&self.bodies[a], t.ra, t.n, inv_mass, inv_i);
                let k_b = b.map_or(0.0, |j| give(&self.bodies[j], t.rb, t.n, inv_mass, inv_i));
                if k_a + k_b <= 0.0 {
                    continue;
                }
                let vn = dot(rel, t.n);
                let mut dj = (t.target - vn) / (k_a + k_b);
                let old = t.jn;
                t.jn = (old + dj).max(0.0);
                dj = t.jn - old;
                apply(&mut self.bodies, a, b, mul(t.n, dj), t.ra, t.rb, inv_mass, inv_i);
                // across: the friction, as far as the push allows
                for (k, dir) in [t.t1, t.t2].into_iter().enumerate() {
                    let va = add(self.bodies[a].v, cross(self.bodies[a].w, t.ra));
                    let vb = match b {
                        Some(j) => add(self.bodies[j].v, cross(self.bodies[j].w, t.rb)),
                        None => [0.0; 3],
                    };
                    let vt = dot(sub(va, vb), dir);
                    let k_a = give(&self.bodies[a], t.ra, dir, inv_mass, inv_i);
                    let k_b = b.map_or(0.0, |j| give(&self.bodies[j], t.rb, dir, inv_mass, inv_i));
                    let mut dj = -vt / (k_a + k_b);
                    let limit = t.mu * t.jn;
                    let old = t.jt[k];
                    t.jt[k] = (old + dj).clamp(-limit, limit);
                    dj = t.jt[k] - old;
                    apply(&mut self.bodies, a, b, mul(dir, dj), t.ra, t.rb, inv_mass, inv_i);
                }
            }
        }

        // the push-out: the same way, on an extra pace each body has for this step only
        let mut extra = vec![([0.0f32; 3], [0.0f32; 3]); self.bodies.len()];
        for _ in 0..6 {
            for t in touches.iter_mut() {
                if t.out <= 0.0 && t.jp <= 0.0 {
                    continue;
                }
                let (a, b) = (t.a, t.b);
                let va = add(extra[a].0, cross(extra[a].1, t.ra));
                let vb = match b {
                    Some(j) => add(extra[j].0, cross(extra[j].1, t.rb)),
                    None => [0.0; 3],
                };
                let k_a = give(&self.bodies[a], t.ra, t.n, inv_mass, inv_i);
                let k_b = b.map_or(0.0, |j| give(&self.bodies[j], t.rb, t.n, inv_mass, inv_i));
                if k_a + k_b <= 0.0 {
                    continue;
                }
                let mut dj = (t.out - dot(sub(va, vb), t.n)) / (k_a + k_b);
                let old = t.jp;
                t.jp = (old + dj).max(0.0);
                dj = t.jp - old;
                let j = mul(t.n, dj);
                if !self.bodies[a].asleep {
                    extra[a].0 = add(extra[a].0, mul(j, inv_mass));
                    extra[a].1 = add(extra[a].1, inv_inertia(self.bodies[a].q, inv_i, cross(t.ra, j)));
                }
                if let Some(k) = b.filter(|k| !self.bodies[*k].asleep) {
                    extra[k].0 = sub(extra[k].0, mul(j, inv_mass));
                    extra[k].1 = sub(extra[k].1, inv_inertia(self.bodies[k].q, inv_i, cross(t.rb, j)));
                }
            }
        }

        // the moves; a body still long enough sleeps
        let touching: Vec<bool> = (0..self.bodies.len()).map(|i| touches.iter().any(|t| t.a == i)).collect();
        let dragged = len([slide[0], 0.0, slide[2]]) / h.max(1e-6);
        let turn = if chance(self.seed, 5) < 0.5 { -1.0 } else { 1.0 };
        for (i, b) in self.bodies.iter_mut().enumerate() {
            if touching[i] && dragged > 0.05 {
                // carried with the item's slide; a round thing rolls the way it goes, the rest spin as they skid
                b.p = add(b.p, [slide[0], 0.0, slide[2]]);
                if kind == ROUND {
                    let d = unit([slide[0], 0.0, slide[2]]);
                    let r = half[0].min(half[1]).max(0.05);
                    let roll = mul(cross([0.0, 1.0, 0.0], d), dragged / r);
                    b.w = add(b.w, mul(sub(roll, b.w), 1.0 - (-6.0 * h).exp()));
                } else {
                    let per_block = match kind {
                        LONG => 1.7,
                        SOLID => 0.4,
                        _ => 0.8,
                    };
                    let to = turn * dragged * per_block * input[I_SPIN].max(0.0);
                    b.w[1] += (to - b.w[1]) * (1.0 - (-3.0 * h).exp());
                }
                b.asleep = false;
            }
            if b.asleep {
                continue;
            }
            if touching[i] {
                // rolling dies down on the ground (a round thing rolls longer)
                let k = if kind == ROUND { 0.25 } else { 1.4 };
                b.w = mul(b.w, (-k * h).exp());
            } else {
                b.w = mul(b.w, (-0.12 * h).exp());
            }
            if touching[i] && (kind == FLAT || kind == LONG) {
                // a thin thing cannot balance on its edge: standing on it, it falls over to the nearer side
                let n = b.q.rotate([0.0, 0.0, 1.0]);
                if n[1].abs() < 0.7 && len(b.w) < 3.0 {
                    let s = if n[1] >= 0.0 { 1.0 } else { -1.0 };
                    let axis = cross(mul(n, s), [0.0, 1.0, 0.0]);
                    if len(axis) > 1e-4 {
                        b.w = add(b.w, mul(unit(axis), 9.0 * h * (1.0 - n[1].abs())));
                    }
                }
            }
            b.p = add(b.p, mul(add(b.v, extra[i].0), h));
            b.q = b.q.spun(add(b.w, extra[i].1), h);
            if touching[i] && len(b.v) < 0.04 && len(b.w) < 0.15 && dragged < 0.05 {
                b.still += h;
                if b.still > 0.35 && !wet {
                    b.asleep = true;
                    b.v = [0.0; 3];
                    b.w = [0.0; 3];
                }
            } else {
                b.still = 0.0;
            }
        }
        if hardest > out[O_IMPACT] {
            out[O_IMPACT] = hardest;
            out[O_IMPACT + 1] = hard_at[0] - self.at[0];
            out[O_IMPACT + 2] = hard_at[1] - self.at[1];
            out[O_IMPACT + 3] = hard_at[2] - self.at[2];
        }
    }
}

fn touch(a: usize, b: Option<usize>, ra: V3, rb: V3, n: V3, depth: f32, mu: f32, bounce: f32) -> Touch {
    let side = if n[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 0.0, 1.0] };
    let t1 = unit(cross(n, side));
    let t2 = cross(n, t1);
    Touch { a, b, ra, rb, n, depth, mu, bounce, jn: 0.0, jt: [0.0; 2], t1, t2, target: 0.0, out: 0.0, jp: 0.0 }
}

#[allow(clippy::too_many_arguments)]
fn apply(bodies: &mut [Body], a: usize, b: Option<usize>, j: V3, ra: V3, rb: V3, inv_mass: f32, inv_i: V3) {
    push(&mut bodies[a], j, ra, inv_mass, inv_i);
    if let Some(k) = b {
        push(&mut bodies[k], mul(j, -1.0), rb, inv_mass, inv_i);
    }
}

/// An impulse on a body; one asleep stands as the ground does, unless it is hit hard: then it wakes.
fn push(b: &mut Body, j: V3, r: V3, inv_mass: f32, inv_i: V3) {
    if b.asleep {
        if len(j) * inv_mass < 0.35 {
            return;
        }
        b.asleep = false;
        b.still = 0.0;
    }
    b.v = add(b.v, mul(j, inv_mass));
    b.w = add(b.w, inv_inertia(b.q, inv_i, cross(r, j)));
}

/// How hard a body is to move at a point along a way (none for one asleep: it stands as the ground).
fn give(b: &Body, r: V3, n: V3, inv_mass: f32, inv_i: V3) -> f32 {
    if b.asleep { 0.0 } else { inv_mass + dot(n, cross(inv_inertia(b.q, inv_i, cross(r, n)), r)) }
}

/// The turn a throw gives: over and forward the way it goes, faster the harder, never twice the same.
fn launch(v: V3, seed: u32, spin: f32) -> V3 {
    let flat = [v[0], 0.0, v[2]];
    let speed = len(v);
    let axis = if len(flat) > 0.4 {
        unit(cross([0.0, 1.0, 0.0], flat))
    } else {
        let a = chance(seed, 2) * TAU;
        [a.cos(), 0.0, a.sin()]
    };
    let rate = (3.0 + 1.1 * speed).min(18.0) * (0.7 + 0.6 * chance(seed, 3)) * spin;
    let yaw = (chance(seed, 4) - 0.5) * 4.0 * spin;
    add(mul(axis, rate), [0.0, yaw, 0.0])
}

/// An item's copies this frame, from what Java says of it (`at`: its place).
pub fn pose(input: &[f32; IN], at: [f64; 3]) -> [f32; OUT] {
    let now = clock();
    let mut out = [0.0f32; OUT];
    let mut all = ITEMS.lock().unwrap_or_else(|p| p.into_inner());
    let map = all.get_or_insert_with(HashMap::new);
    if map.len() > 64 && (now * 4.0) as i64 % 8 == 0 {
        map.retain(|_, it| now - it.seen < 6.0);
    }
    let id = item_id(input);
    let n = (input[I_COUNT] as usize).clamp(1, MAX_COPIES);
    let fresh = match map.get(&id) {
        None => true,
        Some(it) => {
            let far = (0..3).map(|k| (at[k] - it.anchor[k]) as f32).map(|d| d * d).sum::<f32>().sqrt();
            let moved = (0..3).map(|k| (at[k] - it.anchor[k]) as f32 - it.at[k]).map(|d| d * d).sum::<f32>().sqrt();
            // somewhere else at once (a teleport), the game's clock gone back, or out of sight a long while and moved
            // since (one lying still out of sight keeps its heap: worked out anew, a pile turned back to would cost all
            // its settling at once)
            far > 24.0 || input[I_AGE] + 1.0 < it.age || input[I_AGE] - it.age > 40.0 && moved > 0.3
        }
    };
    if fresh {
        map.insert(id, Item::new(input, at, now));
    }
    // the other items' copies close by, in this one's frame (a heap of different things lies one on another)
    let others: Vec<Other> = {
        let me = &map[&id];
        let mine = [me.anchor[0] + me.at[0] as f64, me.anchor[1] + me.at[1] as f64, me.anchor[2] + me.at[2] as f64];
        let mut v = Vec::new();
        for (oid, o) in map.iter() {
            if *oid == id || now - o.seen > 0.5 || !o.free {
                continue;
            }
            let d = [o.anchor[0] + o.at[0] as f64 - mine[0], o.anchor[1] + o.at[1] as f64 - mine[1], o.anchor[2] + o.at[2] as f64 - mine[2]];
            if d[0] * d[0] + d[2] * d[2] > 1.6 * 1.6 || d[1].abs() > 1.5 {
                continue;
            }
            let shift = [(o.anchor[0] - me.anchor[0]) as f32, (o.anchor[1] - me.anchor[1]) as f32, (o.anchor[2] - me.anchor[2]) as f32];
            let feel = feelers(o.kind, o.half);
            for b in &o.bodies {
                v.push(Other { p: add(shift, b.p), q: b.q, half: o.half, feel: feel.clone() });
            }
        }
        v
    };
    let it = map.get_mut(&id).unwrap();
    it.seen = now;
    let rel = [(at[0] - it.anchor[0]) as f32, (at[1] - it.anchor[1]) as f32, (at[2] - it.anchor[2]) as f32];
    let dt = ((input[I_AGE] - it.age) / 20.0).clamp(0.0, 0.1);
    it.age = input[I_AGE];
    let flags = input[I_FLAGS] as u32;
    let kind = input[I_SHAPE] as u32;
    let (_, half) = shape_of(input);
    it.kind = kind;
    it.half = half;
    let vel = mul([input[I_VEL], input[I_VEL + 1], input[I_VEL + 2]], 20.0);
    let wet = flags & IN_FLUID != 0;
    let on_ground = flags & ON_GROUND != 0;

    // copies come and go as the stack grows or shrinks (a merge, a part picked up)
    if it.bodies.len() != n {
        let homes = heap(n, it.seed, half, kind);
        while it.bodies.len() > n {
            it.bodies.pop();
        }
        while it.bodies.len() < n {
            let k = it.bodies.len();
            let q = Quat::around([0.0, 1.0, 0.0], chance(it.seed, 70 + k as u32) * TAU);
            let p = add(rel, [homes[k][0] * 0.5, reach_down(kind, half, q) + 0.25, homes[k][2] * 0.5]);
            it.bodies.push(Body { p, v: [0.0, -1.0, 0.0], q, w: [0.0; 3], home: homes[k], still: 0.0, asleep: false, kicked: 0.0 });
        }
        for (k, b) in it.bodies.iter_mut().enumerate() {
            b.home = homes[k];
            b.asleep = false;
        }
    }

    // the item's place moved: the sleepers wake (a slide, a push, the ground gone)
    let moved = len(sub(rel, it.at));
    if moved > 0.004 {
        for b in &mut it.bodies {
            b.asleep = false;
            b.still = 0.0;
        }
    }
    // a walker going through kicks them
    if input[I_KICK + 4] > 0.5 {
        let kx = input[I_KICK] + rel[0];
        let kz = input[I_KICK + 1] + rel[2];
        let kv = [input[I_KICK + 2], 0.0, input[I_KICK + 3]];
        let speed = len(kv);
        for (k, b) in it.bodies.iter_mut().enumerate() {
            let d = [b.p[0] - kx, 0.0, b.p[2] - kz];
            if len(d) < 0.42 && speed > 0.8 && it.age - b.kicked > 6.0 {
                b.kicked = it.age;
                b.asleep = false;
                let push = mul(unit(add(unit(kv), mul(unit(d), 0.6))), (speed * 0.55).min(3.5));
                b.v = add(b.v, [push[0], (0.9 + 0.4 * chance(it.seed, 90 + k as u32)) * speed.min(4.0) * 0.35, push[2]]);
                b.w = add(b.w, mul(unit(cross([0.0, 1.0, 0.0], push)), speed * 3.0 + 2.0));
            }
        }
    }

    if !it.free {
        // in the air, as one: carried at the item's place, turning as it was thrown
        it.at = rel;
        let main_q = it.bodies[0].q;
        let w = it.bodies[0].w;
        let q = main_q.spun(w, dt);
        let lowest = reach_down(kind, half, q);
        // the copies round one another as the game has them (turning slowly about the up), one a little over another:
        // down, they come apart side by side and the upper ones onto them, not one standing on another
        let round = Quat::around([0.0, 1.0, 0.0], it.age * 0.05 + chance(it.seed, 79) * TAU);
        for (k, b) in it.bodies.iter_mut().enumerate() {
            let off = add(round.rotate(mul(b.home, 0.5)), [0.0, k as f32 * 0.03, 0.0]);
            b.q = if k == 0 { q } else { q.then(Quat::around([0.0, 1.0, 0.0], chance(it.seed, 80 + k as u32) * 2.0)) };
            b.w = w;
            b.p = add(add(rel, [0.0, lowest + 0.0625, 0.0]), off);
            b.v = vel;
        }
        if on_ground || wet {
            // down: each copy on its own from here, as it was going
            it.free = true;
            let last = it.vel;
            for (k, b) in it.bodies.iter_mut().enumerate() {
                b.v = add(last, [(chance(it.seed, 100 + k as u32) - 0.5) * 0.6, 0.0, (chance(it.seed, 110 + k as u32) - 0.5) * 0.6]);
            }
            if wet && !it.wet {
                out[O_SPLASH] = (-last[1]).max(0.0);
            }
        }
    }
    if it.free {
        // thrown up again (a blast, a piston): back to the air as one
        if !on_ground && !wet && vel[1] > 2.0 {
            it.free = false;
            let spin = input[I_SPIN].max(0.0);
            let seed = it.seed;
            for b in &mut it.bodies {
                b.w = launch(vel, seed, spin);
                b.asleep = false;
            }
        }
    }
    if it.free {
        let steps = (dt / STEP).ceil() as usize;
        let from = it.at;
        for s in 0..steps.max(if dt > 0.0 { 1 } else { 0 }) {
            let t = (s + 1) as f32 / steps.max(1) as f32;
            let was = it.at;
            it.at = add(from, mul(sub(rel, from), t));
            let h = dt / steps.max(1) as f32;
            it.step(input, h, sub(it.at, was), &mut out, &others);
        }
        it.at = rel;
        // gone far from the item's place (the leash could not keep it): back to it
        for b in &mut it.bodies {
            let off = sub(b.p, add(rel, b.home));
            if len([off[0], 0.0, off[2]]) > 0.9 || off[1].abs() > 1.6 {
                b.p = add(rel, add(b.home, [0.0, reach_down(kind, half, b.q) + 0.02, 0.0]));
                b.v = [0.0; 3];
                b.w = [0.0; 3];
            }
        }
        if wet && !it.wet {
            out[O_SPLASH] = out[O_SPLASH].max((-it.vel[1]).max(0.0));
        }
    }
    it.wet = wet;
    it.vel = vel;

    // the frame's landing: once in a while (not each rattle)
    if out[O_IMPACT] > 1.2 && it.age - it.sound_at > 3.0 {
        it.sound_at = it.age;
        it.landed_at = it.age;
    } else {
        out[O_IMPACT] = 0.0;
    }
    out[0] = it.bodies.len() as f32;
    for (k, b) in it.bodies.iter().enumerate() {
        let o = 1 + k * 7;
        out[o] = b.p[0] - rel[0];
        out[o + 1] = b.p[1] - rel[1];
        out[o + 2] = b.p[2] - rel[2];
        out[o + 3] = b.q.x;
        out[o + 4] = b.q.y;
        out[o + 5] = b.q.z;
        out[o + 6] = b.q.w;
    }
    if wet {
        let top = input[I_FLUID];
        let mid = it.bodies.iter().map(|b| b.p[1] - rel[1]).sum::<f32>() / it.bodies.len() as f32;
        out[O_SUNK] = (top - mid).clamp(0.0, 1.0);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(id: f32, kind: u32, stuff: u32, count: f32, half: V3, flags: u32) -> [f32; IN] {
        let mut v = [0.0; IN];
        v[I_ID] = id;
        v[I_FLAGS] = flags as f32;
        v[I_SHAPE] = kind as f32;
        v[I_STUFF] = stuff as f32;
        v[I_COUNT] = count;
        v[I_BOX..I_BOX + 6].copy_from_slice(&[-half[0], -half[1], -half[2], half[0], half[1], half[2]]);
        v[I_SLIP] = 0.6;
        v[I_BOUNCE] = 1.0;
        v[I_SPIN] = 1.0;
        v[I_PILES] = 1.0;
        v
    }

    /// The bodies' lowest points under the ground (most) and how fast they still go, after `secs` of the clock.
    fn run(v: &mut [f32; IN], at: [f64; 3], secs: f32, fps: f32) -> ([f32; OUT], f32, f32) {
        let mut out = [0.0; OUT];
        let mut deepest = 0.0f32;
        let frames = (secs * fps) as usize;
        for _ in 0..frames {
            v[I_AGE] += 20.0 / fps;
            out = pose(v, at);
            let all = ITEMS.lock().unwrap();
            let it = all.as_ref().unwrap().get(&(v[I_ID] as i32)).unwrap();
            let (_, half) = shape_of(v);
            for b in &it.bodies {
                for p in points(v[I_SHAPE] as u32, half) {
                    let w = add(b.p, b.q.rotate(p));
                    deepest = deepest.max(-(w[1] - it.at[1]));
                }
            }
        }
        let all = ITEMS.lock().unwrap();
        let it = all.as_ref().unwrap().get(&(v[I_ID] as i32)).unwrap();
        let fastest = it.bodies.iter().map(|b| len(b.v) + len(b.w) * 0.1).fold(0.0, f32::max);
        (out, deepest, fastest)
    }

    #[test]
    fn a_block_dropped_lands_on_a_face_and_rests() {
        let half = [0.125, 0.125, 0.125];
        let mut v = input(1.0, SOLID, STONE, 1.0, half, 0);
        v[I_VEL] = 0.08;
        v[I_VEL + 1] = 0.1;
        let _ = pose(&v, [0.0, 0.0, 0.0]);
        // it lands at once (the game says so), then works itself out
        v[I_FLAGS] = ON_GROUND as f32;
        v[I_VEL] = 0.0;
        v[I_VEL + 1] = 0.0;
        let (out, deepest, fastest) = run(&mut v, [0.0, 0.0, 0.0], 3.0, 60.0);
        assert!(deepest < 0.012, "into the ground {deepest}");
        assert!(fastest < 0.05, "still going {fastest}");
        // a face down: one of its axes stands straight up
        let q = Quat { x: out[4], y: out[5], z: out[6], w: out[7] };
        let up = [q.rotate([1.0, 0.0, 0.0])[1].abs(), q.rotate([0.0, 1.0, 0.0])[1].abs(), q.rotate([0.0, 0.0, 1.0])[1].abs()];
        assert!(up.iter().any(|u| *u > 0.99), "not on a face: {up:?}");
        // on the ground: its middle half a block's eighth over it
        assert!((out[2] - 0.125).abs() < 0.015, "its middle at {}", out[2]);
    }

    #[test]
    fn a_flat_thing_lies_flat() {
        let half = [0.25, 0.25, 0.03];
        let mut v = input(2.0, FLAT, SOFT, 1.0, half, 0);
        v[I_VEL + 1] = -0.3;
        let _ = pose(&v, [5.0, 0.0, 5.0]);
        v[I_FLAGS] = ON_GROUND as f32;
        let (out, deepest, fastest) = run(&mut v, [5.0, 0.0, 5.0], 3.0, 60.0);
        let q = Quat { x: out[4], y: out[5], z: out[6], w: out[7] };
        assert!(q.rotate([0.0, 0.0, 1.0])[1].abs() > 0.98, "not lying flat");
        // (a hard landing may sink it for a frame by a step's fall: no more)
        assert!(deepest < 0.03 && fastest < 0.05, "{deepest} {fastest}");
    }

    #[test]
    fn a_heap_lies_one_on_another() {
        let half = [0.125, 0.125, 0.125];
        let mut v = input(3.0, SOLID, WOOD, 5.0, half, ON_GROUND);
        v[I_AGE] = 100.0;
        let _ = pose(&v, [10.0, 0.0, 10.0]);
        let (_, deepest, fastest) = run(&mut v, [10.0, 0.0, 10.0], 3.0, 60.0);
        assert!(deepest < 0.015, "into the ground {deepest}");
        assert!(fastest < 0.08, "still going {fastest}");
        apart(3, half);
    }

    /// No two copies of the item one in the other (by their boxes' least overlap).
    fn apart(id: i32, half: V3) {
        let all = ITEMS.lock().unwrap();
        let it = all.as_ref().unwrap().get(&id).unwrap();
        let axes = |q: Quat| [q.rotate([1.0, 0.0, 0.0]), q.rotate([0.0, 1.0, 0.0]), q.rotate([0.0, 0.0, 1.0])];
        for i in 0..it.bodies.len() {
            for j in i + 1..it.bodies.len() {
                let (a, b) = (&it.bodies[i], &it.bodies[j]);
                let (ea, eb) = (axes(a.q), axes(b.q));
                let d = sub(a.p, b.p);
                let over = ea
                    .iter()
                    .chain(eb.iter())
                    .map(|l| {
                        let ra: f32 = (0..3).map(|k| (half[k] * dot(ea[k], *l)).abs()).sum();
                        let rb: f32 = (0..3).map(|k| (half[k] * dot(eb[k], *l)).abs()).sum();
                        ra + rb - dot(d, *l).abs()
                    })
                    .fold(f32::MAX, f32::min);
                assert!(over < 0.02, "copy {i} in copy {j} by {over}: {:?} {:?}", a.p, b.p);
            }
        }
    }

    #[test]
    fn two_things_dropped_on_one_spot_lie_by_each_other() {
        // one lying there, another dropped where it lies (the game has them a tenth of a block apart)
        let half = [0.125, 0.125, 0.125];
        let mut a = input(9.0, SOLID, STONE, 1.0, half, ON_GROUND);
        a[I_AGE] = 100.0;
        let _ = pose(&a, [70.0, 0.0, 70.0]);
        let mut b = input(10.0, SOLID, WOOD, 1.0, half, 0);
        b[I_VEL + 1] = -0.2;
        let _ = pose(&b, [70.1, 0.0, 70.0]);
        for k in 0..240 {
            if k == 12 {
                b[I_FLAGS] = ON_GROUND as f32;
                b[I_VEL + 1] = 0.0;
            }
            a[I_AGE] += 20.0 / 60.0;
            b[I_AGE] += 20.0 / 60.0;
            let _ = pose(&a, [70.0, 0.0, 70.0]);
            let _ = pose(&b, [70.1, 0.0, 70.0]);
        }
        let all = ITEMS.lock().unwrap();
        let (ia, ib) = (&all.as_ref().unwrap()[&9], &all.as_ref().unwrap()[&10]);
        let (pa, qa) = (add(ia.bodies[0].p, [0.0; 3]), ia.bodies[0].q);
        let (pb, qb) = (add(ib.bodies[0].p, [0.1, 0.0, 0.0]), ib.bodies[0].q);
        let (ea, eb) = (axes(qa), axes(qb));
        let d = sub(pa, pb);
        let over = ea
            .iter()
            .chain(eb.iter())
            .map(|l| {
                let ra: f32 = (0..3).map(|k| (half[k] * dot(ea[k], *l)).abs()).sum();
                let rb: f32 = (0..3).map(|k| (half[k] * dot(eb[k], *l)).abs()).sum();
                ra + rb - dot(d, *l).abs()
            })
            .fold(f32::MAX, f32::min);
        assert!(over < 0.02, "one in the other by {over}: {pa:?} {pb:?}");
    }

    #[test]
    fn a_stack_dropped_falls_apart_into_a_heap() {
        // as the game drops a stack: in the air, its copies close together, then down
        let half = [0.125, 0.125, 0.125];
        let mut v = input(7.0, SOLID, STONE, 5.0, half, 0);
        v[I_VEL + 1] = -0.2;
        let _ = pose(&v, [40.0, 0.0, 40.0]);
        let _ = run(&mut v, [40.0, 0.0, 40.0], 0.2, 60.0);
        v[I_FLAGS] = ON_GROUND as f32;
        v[I_VEL + 1] = 0.0;
        let (_, deepest, fastest) = run(&mut v, [40.0, 0.0, 40.0], 4.0, 60.0);
        assert!(deepest < 0.015, "into the ground {deepest}");
        assert!(fastest < 0.08, "still going {fastest}");
        apart(7, half);
        // and flat things land one on another, not one in another
        let half = [0.2, 0.2, 0.03];
        let mut v = input(8.0, FLAT, METAL, 5.0, half, 0);
        v[I_VEL + 1] = -0.2;
        let _ = pose(&v, [50.0, 0.0, 50.0]);
        let _ = run(&mut v, [50.0, 0.0, 50.0], 0.2, 60.0);
        v[I_FLAGS] = ON_GROUND as f32;
        v[I_VEL + 1] = 0.0;
        let (_, deepest, fastest) = run(&mut v, [50.0, 0.0, 50.0], 4.0, 60.0);
        assert!(deepest < 0.015 && fastest < 0.08, "{deepest} {fastest}");
        apart(8, half);
    }

    #[test]
    fn it_follows_the_items_slide() {
        let half = [0.25, 0.25, 0.03];
        let mut v = input(4.0, FLAT, SOFT, 1.0, half, ON_GROUND);
        v[I_AGE] = 100.0;
        let _ = pose(&v, [60.0, 0.0, 60.0]);
        for k in 0..60 {
            v[I_AGE] += 20.0 / 60.0;
            let x = 60.0 + (k as f64) / 60.0;
            v[I_VEL] = 1.0 / 20.0;
            let _ = pose(&v, [x, 0.0, 60.0]);
        }
        v[I_VEL] = 0.0;
        let (out, _, _) = run(&mut v, [61.0, 0.0, 60.0], 2.0, 60.0);
        assert!(out[1].abs() < 0.15 && out[3].abs() < 0.15, "left behind: {} {}", out[1], out[3]);
    }

    #[test]
    fn a_coin_on_its_rim_falls_over() {
        let half = [0.2, 0.2, 0.03];
        let mut v = input(5.0, ROUND, METAL, 1.0, half, ON_GROUND);
        let _ = pose(&v, [20.0, 0.0, 20.0]);
        {
            // stood on its rim, a little off straight
            let mut all = ITEMS.lock().unwrap();
            let it = all.as_mut().unwrap().get_mut(&5).unwrap();
            it.free = true;
            let q = Quat::around([1.0, 0.0, 0.0], 0.15);
            it.bodies[0].q = q;
            it.bodies[0].p = [0.0, reach_down(ROUND, half, q) + 0.001, 0.0];
            it.bodies[0].asleep = false;
        }
        let (out, deepest, _) = run(&mut v, [20.0, 0.0, 20.0], 4.0, 60.0);
        let q = Quat { x: out[4], y: out[5], z: out[6], w: out[7] };
        assert!(q.rotate([0.0, 0.0, 1.0])[1].abs() > 0.95, "still on its rim");
        assert!(deepest < 0.015);
    }

    #[test]
    fn afloat_it_stays_at_the_top() {
        let half = [0.25, 0.25, 0.03];
        let mut v = input(6.0, FLAT, WOOD, 1.0, half, IN_FLUID);
        v[I_FLUID] = 0.4;
        for k in 0..GRID_N * GRID_N {
            v[I_GROUND + k] = -0.6;
        }
        let _ = pose(&v, [30.0, 0.0, 30.0]);
        let (out, _, _) = run(&mut v, [30.0, 0.0, 30.0], 5.0, 60.0);
        assert!((out[2] - 0.4).abs() < 0.12, "not at the water's top: {}", out[2]);
    }
}
