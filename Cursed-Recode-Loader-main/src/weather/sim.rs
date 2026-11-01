//! The weather's particle systems running as Source 2 runs them (a port of ValveResourceFormat's particle renderer, MIT,
//! for the functions the weather uses): each step the control points are set, the emitters let their particles out
//! (each made by the initializers), the operators move and fade them (the forces inside the movement), the dead go;
//! then the children step, reading their parent's particles. In the game's units (Source's: z up, a block 40 of them),
//! from a point near the camera (the positions stay small; mod.rs moves it along).

use super::def::*;
use std::f32::consts::PI;

pub const CPS: usize = 64;

/// A control point: where, and which way it faces (none: the world's way).
#[derive(Clone, Copy, Debug, Default)]
pub struct Cp {
    pub pos: V3,
    pub fwd: Option<V3>,
}

/// A particle.
#[derive(Clone, Copy, Debug)]
pub struct P {
    pub pos: V3,
    pub prev: V3,
    pub vel: V3,
    pub force: V3,
    pub age: f32,
    pub life: f32,
    pub created: f32,
    pub alpha: f32,
    pub alpha2: f32,
    pub color: V3,
    pub radius: f32,
    pub trail: f32,
    pub roll: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub seq: i32,
    /// Its random identity (the draws that stay its own all its life come from it).
    pub id: u32,
    pub scratch: f32,
    pub scratch1: f32,
    pub dead: bool,
}

impl P {
    fn new(d: &Def) -> P {
        P {
            pos: [0.0; 3],
            prev: [0.0; 3],
            vel: [0.0; 3],
            force: [0.0; 3],
            age: 0.0,
            life: d.life,
            created: 0.0,
            alpha: d.alpha,
            alpha2: 1.0,
            color: d.colour,
            radius: d.radius,
            trail: 0.1,
            roll: 0.0,
            yaw: 0.0,
            pitch: 0.0,
            seq: 0,
            id: 0,
            scratch: 0.0,
            scratch1: 0.0,
            dead: false,
        }
    }

    pub fn get(&self, field: u8) -> f32 {
        match field {
            LIFE => self.life,
            RADIUS => self.radius,
            ROLL => self.roll,
            ALPHA => self.alpha,
            TRAIL => self.trail,
            YAW => self.yaw,
            ALPHA2 => self.alpha2,
            SCRATCH => self.scratch,
            PITCH => self.pitch,
            SCRATCH1 => self.scratch1,
            _ => 0.0,
        }
    }

    pub fn set(&mut self, field: u8, v: f32) {
        match field {
            LIFE => self.life = v,
            RADIUS => self.radius = v,
            ROLL => self.roll = v,
            ALPHA => self.alpha = v,
            TRAIL => self.trail = v,
            YAW => self.yaw = v,
            ALPHA2 => self.alpha2 = v,
            SCRATCH => self.scratch = v,
            PITCH => self.pitch = v,
            SCRATCH1 => self.scratch1 = v,
            _ => {}
        }
    }

    fn get_v(&self, field: u8) -> V3 {
        match field {
            POS => self.pos,
            PREV => self.prev,
            COLOR => self.color,
            _ => [0.0; 3],
        }
    }

    fn set_v(&mut self, field: u8, v: V3) {
        match field {
            POS => self.pos = v,
            PREV => self.prev = v,
            COLOR => self.color = v,
            _ => {}
        }
    }

    pub fn normalized_age(&self) -> f32 {
        self.age / self.life.max(1e-4)
    }
}

// ---- vectors

pub fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub fn mul(a: V3, k: f32) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}

pub fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

pub fn len(a: V3) -> f32 {
    dot(a, a).sqrt()
}

pub fn norm(a: V3) -> V3 {
    let l = len(a);
    if l > 1e-9 {
        mul(a, 1.0 / l)
    } else {
        [0.0; 3]
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn lerp3(a: V3, b: V3, t: f32) -> V3 {
    [lerp(a[0], b[0], t), lerp(a[1], b[1], t), lerp(a[2], b[2], t)]
}

/// A point's frame: its forward, left and up (the world's when it faces nowhere).
pub fn basis(cp: &Cp) -> Option<(V3, V3, V3)> {
    let f = norm(cp.fwd?);
    let l = norm(cross([0.0, 0.0, 1.0], f));
    if len(l) < 0.5 {
        return None;
    }
    Some((f, l, cross(f, l)))
}

/// A direction in a point's frame, in the world.
pub fn turn(cp: &Cp, v: V3) -> V3 {
    match basis(cp) {
        Some((f, l, u)) => add(add(mul(f, v[0]), mul(l, v[1])), mul(u, v[2])),
        None => v,
    }
}

// ---- randomness

/// A small fast generator (xorshift*).
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed | 1)
    }

    pub fn next(&mut self) -> f32 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        ((self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 40) as f32) / (1u64 << 24) as f32
    }

    fn between(&mut self, a: f32, b: f32) -> f32 {
        lerp(a, b, self.next())
    }

    fn between3(&mut self, a: V3, b: V3) -> V3 {
        [self.between(a[0], b[0]), self.between(a[1], b[1]), self.between(a[2], b[2])]
    }
}

/// A particle's own draw, the same each time it is asked (its identity and which draw).
pub fn own(id: u32, which: u32) -> f32 {
    let mut x = (id as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (which as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    x ^= x >> 31;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 29;
    ((x >> 40) as f32) / (1u64 << 24) as f32
}

fn hash3(x: i32, y: i32, z: i32) -> f32 {
    own((x as u32).wrapping_add((y as u32) << 10).wrapping_add((z as u32) << 20), 0x5EED)
}

/// Value noise in three dimensions, -1..1.
pub fn noise3(x: f32, y: f32, z: f32) -> f32 {
    let (ix, iy, iz) = (x.floor() as i32, y.floor() as i32, z.floor() as i32);
    let (fx, fy, fz) = (x - ix as f32, y - iy as f32, z - iz as f32);
    let c00 = lerp(hash3(ix, iy, iz), hash3(ix + 1, iy, iz), fx);
    let c10 = lerp(hash3(ix, iy + 1, iz), hash3(ix + 1, iy + 1, iz), fx);
    let c01 = lerp(hash3(ix, iy, iz + 1), hash3(ix + 1, iy, iz + 1), fx);
    let c11 = lerp(hash3(ix, iy + 1, iz + 1), hash3(ix + 1, iy + 1, iz + 1), fx);
    (lerp(lerp(c00, c10, fy), lerp(c01, c11, fy), fz) - 0.5) * 2.0
}

fn bias(x: f32, b: f32) -> f32 {
    if b <= 0.0 {
        return 0.0;
    }
    if b >= 1.0 {
        return 1.0;
    }
    x / ((1.0 - x) * (1.0 / b - 2.0) + 1.0)
}

/// Where the ground is under a point (the game's units, from the simulation's origin), within so far down: its
/// height; none when there is none so near.
pub type Ground<'a> = dyn FnMut(V3, f32) -> Option<f32> + 'a;

fn eval(n: &Num, rng: &mut Rng, cps: &[Cp; CPS]) -> f32 {
    match *n {
        Num::Lit(x) => x,
        Num::Rand { lo, hi, flip } => {
            let v = rng.between(lo, hi);
            if flip && rng.next() < 0.5 {
                -v
            } else {
                v
            }
        }
        Num::Cp { cp, comp, mult } => cps[cp].pos[comp] * mult,
    }
}

/// A system and its children, running.
pub struct Sys {
    pub def: usize,
    pub parts: Vec<P>,
    /// Each particle as it was born (what the fades and the radius scale from).
    born: Vec<P>,
    pub age: f32,
    emitted: u32,
    seed: u32,
    rng: Rng,
    /// Each emitter's charge: particles owed so far, particles let out.
    charge: Vec<(f64, i64)>,
    /// The random control points': whether placed, when last, where.
    placed: Vec<(bool, f32, V3)>,
    set_once: Vec<bool>,
    /// The walk over the parent's particles (CreateFromParentParticles, InheritFromParentParticles), each its own.
    walk: Vec<f32>,
    pub children: Vec<Sys>,
    pub dt: f32,
    prev_dt: f32,
}

impl Sys {
    pub fn new(def: usize, defs: &[Def], find: &dyn Fn(&str) -> Option<usize>, seed: u32) -> Sys {
        let d = &defs[def];
        let children = d
            .children
            .iter()
            .enumerate()
            .filter_map(|(k, c)| find(&c.name).map(|i| Sys::new(i, defs, find, seed.wrapping_mul(31).wrapping_add(k as u32 + 1))))
            .collect();
        Sys {
            def,
            parts: Vec::new(),
            born: Vec::new(),
            age: 0.0,
            emitted: 0,
            seed: seed.wrapping_mul(0x9E37_79B9),
            rng: Rng::new(seed as u64 * 0x2545_F491 + 7),
            charge: vec![(0.0, 0); d.emitters.len()],
            placed: vec![(false, 0.0, [0.0; 3]); d.pre.len()],
            set_once: vec![false; d.pre.len()],
            walk: vec![0.0; d.inits.len()],
            children,
            dt: 0.0,
            prev_dt: 0.0,
        }
    }

    /// Everything moved by `by` (the origin moved along).
    pub fn shift(&mut self, by: V3) {
        for p in self.parts.iter_mut().chain(self.born.iter_mut()) {
            p.pos = sub(p.pos, by);
            p.prev = sub(p.prev, by);
        }
        for p in self.placed.iter_mut() {
            p.2 = sub(p.2, by);
        }
        for c in self.children.iter_mut() {
            c.shift(by);
        }
    }

    pub fn count(&self) -> usize {
        self.parts.len() + self.children.iter().map(Sys::count).sum::<usize>()
    }

    /// A frame of `dt` seconds: the system in steps no longer than its own, then its children.
    pub fn step(&mut self, dt: f32, defs: &[Def], cps: &mut [Cp; CPS], ground: &mut Ground<'_>, parent: Option<(&[P], f32)>) {
        let d = &defs[self.def];
        let mut left = dt.min(d.step_max * 10.0);
        while left > 0.0 {
            let s = left.min(d.step_max);
            left -= s;
            self.simulate(s, d, cps, ground, parent);
        }
        let mut children = std::mem::take(&mut self.children);
        for (k, c) in children.iter_mut().enumerate() {
            if self.age >= d.children.get(k).map_or(0.0, |c| c.delay) {
                c.step(dt, defs, cps, ground, Some((&self.parts, self.dt)));
            }
        }
        self.children = children;
    }

    fn simulate(&mut self, dt: f32, d: &Def, cps: &mut [Cp; CPS], ground: &mut Ground<'_>, parent: Option<(&[P], f32)>) {
        self.prev_dt = self.dt;
        self.dt = dt;
        let start = self.age;
        self.age += dt;
        for p in self.parts.iter_mut() {
            p.age = self.age - p.created;
        }
        // the control points
        for (k, f) in d.pre.iter().enumerate() {
            if f.w.strength(self.age) <= 0.0 {
                continue;
            }
            match f.f {
                Pre::SetCp { cp, pos, once, world, head } => {
                    if once && self.set_once[k] {
                        continue;
                    }
                    self.set_once[k] = true;
                    cps[cp].pos = if world { pos } else { add(cps[head].pos, turn(&cps[head], pos)) };
                }
                Pre::RandomCp { cp, lo, hi, world, head, rate, interp } => {
                    let (ref mut done, ref mut last, ref mut at) = self.placed[k];
                    let mut lerp_k = interp;
                    if !*done {
                        *done = true;
                        *last = f32::MIN;
                        *at = self.rng.between3(lo, hi);
                        lerp_k = 1.0;
                    }
                    if self.age >= *last + rate {
                        *at = self.rng.between3(lo, hi);
                        *last = if rate < 0.0 { f32::MAX } else { self.age };
                    }
                    let target = if world { *at } else { add(cps[head].pos, turn(&cps[head], *at)) };
                    cps[cp].pos = lerp3(cps[cp].pos, target, lerp_k);
                }
            }
        }
        // the emitters
        let parents = parent.map_or(0, |p| p.0.len());
        for (k, e) in d.emitters.iter().enumerate() {
            let strength = e.w.strength(self.age);
            if strength <= 0.0 {
                continue;
            }
            let (from, to) = (start, self.age);
            if e.f.start > to {
                continue;
            }
            let (ws, we) = if e.f.duration != 0.0 { (from.max(e.f.start), to.min(e.f.start + e.f.duration)) } else { (from, to) };
            if we <= ws {
                continue;
            }
            let mut rate = eval(&e.f.rate, &mut self.rng, cps) * strength;
            if e.f.per_parent > 0.0 {
                rate *= parents as f32 * e.f.per_parent;
            }
            if rate <= 0.0 {
                continue;
            }
            let (ref mut owed, ref mut out) = self.charge[k];
            *owed += rate as f64 * (we - ws) as f64;
            let flush = (*owed + 0.001).floor() as i64;
            let n = flush - *out;
            if n <= 0 {
                continue;
            }
            *out = flush;
            let each = (we - ws) / n as f32;
            for i in 0..n {
                let created = (ws + (i + 1) as f32 * each).min(we);
                self.emit(to - created, d, cps, ground, parent);
            }
        }
        // the operators
        for (k, f) in d.ops.iter().enumerate() {
            let strength = f.w.strength(self.age);
            if strength <= 0.0 {
                continue;
            }
            self.operate(k as u32, &f.f, strength, dt, d, cps, ground);
        }
        // the dead go
        if self.parts.iter().any(|p| p.dead) {
            let mut k = 0;
            while k < self.parts.len() {
                if self.parts[k].dead {
                    self.parts.swap_remove(k);
                    self.born.swap_remove(k);
                } else {
                    k += 1;
                }
            }
        }
    }

    /// A particle let out `age` seconds ago (within this step), made by the initializers.
    fn emit(&mut self, age: f32, d: &Def, cps: &[Cp; CPS], ground: &mut Ground<'_>, parent: Option<(&[P], f32)>) {
        if self.parts.len() >= d.max {
            return;
        }
        let mut p = P::new(d);
        p.id = self.emitted.wrapping_add(self.seed);
        self.emitted = self.emitted.wrapping_add(1);
        p.pos = cps[0].pos;
        p.created = self.age - age;
        p.age = age;
        for (k, init) in d.inits.iter().enumerate() {
            self.initialize(k, init, &mut p, cps, ground, parent);
        }
        p.prev = sub(p.pos, mul(p.vel, self.dt));
        self.parts.push(p);
        self.born.push(p);
    }

    fn initialize(&mut self, k: usize, init: &Init, p: &mut P, cps: &[Cp; CPS], ground: &mut Ground<'_>, parent: Option<(&[P], f32)>) {
        let rng = &mut self.rng;
        match *init {
            Init::Float { field, ref value, ref strength } => {
                let mut v = eval(value, rng, cps) * eval(strength, rng, cps);
                if angle(field) {
                    v = v.to_radians();
                }
                p.set(field, v);
            }
            Init::Sphere { cp, ref r_lo, ref r_hi, ref s_lo, ref s_hi, s_exp, local_lo, local_hi, bias, bias_abs, local } => {
                let c = &cps[cp];
                let cos_polar = rng.between(-1.0, 1.0);
                let azimuth = rng.between(0.0, 2.0 * PI);
                let sin_polar = (1.0 - cos_polar * cos_polar).max(0.0).sqrt();
                let mut dir = [sin_polar * azimuth.cos(), sin_polar * azimuth.sin(), cos_polar];
                for i in 0..3 {
                    if bias_abs[i] != 0.0 {
                        dir[i] = dir[i].abs();
                    }
                    dir[i] *= bias[i];
                }
                let dir = if len(dir) > 1e-6 { norm(dir) } else { norm([bias[0].max(1e-3), bias[1], 0.0]) };
                let (lo, hi) = (eval(r_lo, rng, cps), eval(r_hi, rng, cps));
                let distance = lerp(lo, hi, rng.next().powf(1.0 / 3.0));
                let (slo, shi) = (eval(s_lo, rng, cps), eval(s_hi, rng, cps));
                let speed = lerp(slo, shi, rng.next().powf(s_exp));
                let local_speed = rng.between3(local_lo, local_hi);
                let offset = mul(dir, distance);
                p.pos = if local { add(turn(c, offset), c.pos) } else { add(c.pos, offset) };
                let vdir = if local { turn(c, dir) } else { dir };
                p.vel = add(mul(vdir, speed), turn(c, local_speed));
            }
            Init::Sequence { lo, hi } => {
                let r = rng.next();
                p.seq = if hi > lo { (lo + (r * (hi - lo + 1) as f32) as i32).min(hi) } else { lo };
            }
            Init::VelocityNoise { lo, hi, scale, scale_loc, offset, offset_loc, cp } => {
                let at = mul(add(p.pos, offset_loc), scale_loc);
                let c = at[0] * 0.7 + at[1] * 1.3 + at[2] * 2.1 + (self.age + offset) * scale;
                let r = [noise3(c, c, c), noise3(c + 101_723.0, c + 101_723.0, c + 101_723.0), noise3(c + 555_557.0, c + 555_557.0, c + 555_557.0)];
                let v = [lerp(lo[0], hi[0], r[0] * 0.5 + 0.5), lerp(lo[1], hi[1], r[1] * 0.5 + 0.5), lerp(lo[2], hi[2], r[2] * 0.5 + 0.5)];
                p.vel = add(p.vel, turn(&cps[cp], v));
            }
            Init::Offset { lo, hi, local, proportional } => {
                let mut o = rng.between3(lo, hi);
                if proportional {
                    o = mul(o, p.radius);
                }
                if local {
                    o = turn(&cps[0], o);
                }
                p.pos = add(p.pos, o);
            }
            Init::Color { lo, hi, tint_lo, tint_hi, tint_perc, tint_cp, add: additive } => {
                let mut c = lerp3(lo, hi, rng.next());
                if tint_perc > 0.0 {
                    let t = cps[tint_cp].pos;
                    let tint = [t[0].clamp(tint_lo[0], tint_hi[0]), t[1].clamp(tint_lo[1], tint_hi[1]), t[2].clamp(tint_lo[2], tint_hi[2])];
                    let tinted = if additive { add(c, tint) } else { [c[0] * tint[0], c[1] * tint[1], c[2] * tint[2]] };
                    c = lerp3(c, tinted, tint_perc.clamp(0.0, 1.0));
                }
                p.color = c;
            }
            Init::YawFlip { percent } => {
                if rng.next() < percent {
                    p.yaw += PI;
                }
            }
            Init::FromParent { velocity_scale, increment, random, subframe } => {
                let Some((parents, parent_dt)) = parent.filter(|p| !p.0.is_empty()) else {
                    p.life = 0.0;
                    return;
                };
                let last = parents.len() - 1;
                let fractional = increment != increment.floor();
                let walk = &mut self.walk[k];
                if random {
                    let r = rng.next();
                    *walk = if fractional { r * last as f32 } else { (r * (last + 1) as f32).floor() };
                } else if *walk > last as f32 {
                    *walk = 0.0;
                }
                let at = *walk;
                *walk += increment;
                let i = (at.floor() as usize).min(last);
                let q = &parents[i];
                let (mut pos, mut prev) = (q.pos, q.prev);
                if fractional {
                    let n = &parents[(at.ceil() as usize).min(last)];
                    let t = at - at.floor();
                    pos = lerp3(pos, n.pos, t);
                    prev = lerp3(prev, n.prev, t);
                }
                let birth = if subframe && self.dt > 0.0 { ((p.created - (self.age - self.dt)) / self.dt).clamp(0.0, 1.0) } else { 1.0 };
                p.pos = lerp3(prev, pos, birth);
                let v = if parent_dt > 0.0 { mul(sub(pos, prev), 1.0 / parent_dt) } else { q.vel };
                p.vel = mul(v, birth * velocity_scale);
            }
            Init::InheritParent { scale, field, increment, random } => {
                let Some((parents, _)) = parent.filter(|p| !p.0.is_empty()) else { return };
                let n = parents.len();
                let walk = &mut self.walk[k];
                let i = if random {
                    let i = ((n as f32 * rng.next()) as usize).min(n - 1);
                    *walk = i as f32;
                    i
                } else if *walk as usize > n - 1 {
                    *walk = 0.0;
                    0
                } else {
                    *walk as usize
                };
                *walk += increment.max(1) as f32;
                let q = &parents[i];
                if matches!(field, POS | PREV | COLOR) {
                    let mut v = mul(q.get_v(field), scale);
                    if field == COLOR {
                        v = [v[0].clamp(0.0, 1.0), v[1].clamp(0.0, 1.0), v[2].clamp(0.0, 1.0)];
                    }
                    p.set_v(field, v);
                } else {
                    let mut v = q.get(field) * scale;
                    if matches!(field, ALPHA | ALPHA2) {
                        v = v.clamp(0.0, 1.0);
                    }
                    p.set(field, v);
                }
            }
            Init::OnGround { offset, max } => {
                if let Some(g) = ground(p.pos, max) {
                    p.pos[2] = g + offset;
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn operate(&mut self, k: u32, op: &Op, strength: f32, dt: f32, d: &Def, cps: &[Cp; CPS], ground: &mut Ground<'_>) {
        // each operator's own draws per particle (as the engine strides them)
        let o = k * 37;
        match *op {
            Op::Decay => {
                for p in self.parts.iter_mut() {
                    if p.age > p.life {
                        p.dead = true;
                    }
                }
            }
            Op::Movement { gravity, drag } => {
                // the forces add to each particle first
                for f in &d.forces {
                    let s = f.w.strength(self.age);
                    if s <= 0.0 {
                        continue;
                    }
                    match f.f {
                        Force::Twist { amount, axis, local, cp } => {
                            let a = if local { turn(&cps[cp], axis) } else { axis };
                            let a = if len(a) < 1e-9 { [0.0, 0.0, 1.0] } else { norm(a) };
                            let c = cps[cp].pos;
                            for p in self.parts.iter_mut() {
                                let delta = sub(p.pos, c);
                                if dot(delta, delta) <= 1e-12 {
                                    continue;
                                }
                                let dir = norm(delta);
                                let align = 1.0 - dot(dir, a);
                                if align * align <= 1e-12 {
                                    continue;
                                }
                                p.force = add(p.force, mul(cross(dir, a), amount * s));
                            }
                        }
                        Force::Random { lo, hi } => {
                            for p in self.parts.iter_mut() {
                                let r = self.rng.between3(lo, hi);
                                p.force = add(p.force, mul(r, s));
                            }
                        }
                    }
                }
                let dt2 = dt * dt;
                let g = mul(gravity, dt2);
                let drag = drag.clamp(0.0, 0.999_999_9);
                let mut keep = ((1.0 - drag).ln() / (1.0 / 30.0) * dt).exp();
                if self.prev_dt > 0.0 {
                    keep *= dt / self.prev_dt;
                }
                for p in self.parts.iter_mut() {
                    let step = add(add(g, mul(p.force, dt2)), mul(sub(p.pos, p.prev), keep));
                    p.vel = mul(step, 1.0 / dt);
                    p.force = [0.0; 3];
                    p.prev = p.pos;
                    p.pos = add(p.pos, step);
                }
            }
            Op::FadeIn { lo, hi, exp, proportional } => {
                for (p, b) in self.parts.iter_mut().zip(&self.born) {
                    let fade = if lo == hi { lo } else { lerp(lo, hi, own(p.id, o).powf(exp)) };
                    let t = if proportional { p.normalized_age() } else { p.age };
                    if t <= fade && fade > 0.0 {
                        p.alpha = t / fade * b.alpha;
                    }
                }
            }
            Op::FadeOut { lo, hi, exp, bias: fb, proportional } => {
                for (p, b) in self.parts.iter_mut().zip(&self.born) {
                    let fade = if lo == hi { lo } else { lerp(lo, hi, own(p.id, o).powf(exp)) };
                    let left = if proportional { 1.0 - p.normalized_age() } else { p.life - p.age };
                    if left <= fade && fade > 0.0 {
                        let mut e = (1.0 - left / fade).clamp(0.0, 1.0);
                        if fb != 0.5 {
                            e = bias(e, fb);
                        }
                        p.alpha = (1.0 - e) * b.alpha;
                    }
                }
            }
            Op::FadeInSimple { time, field } => {
                for (p, b) in self.parts.iter_mut().zip(&self.born) {
                    let t = p.normalized_age();
                    if t <= time && time > 0.0 {
                        p.set(field, t / time * b.alpha);
                    }
                }
            }
            Op::FadeOutSimple { time, field } => {
                for (p, b) in self.parts.iter_mut().zip(&self.born) {
                    let left = 1.0 - p.normalized_age();
                    if left <= time && time > 0.0 {
                        p.set(field, left / time * b.alpha);
                    }
                }
            }
            Op::Oscillate { field, rate, freq, start, end, mult, add: shift, proportional, window_proportional } => {
                for p in self.parts.iter_mut() {
                    let w = if window_proportional { p.normalized_age() } else { p.age };
                    let (s0, s1) = (lerp(start.0, start.1, own(p.id, o + 11)), lerp(end.0, end.1, own(p.id, o + 12)));
                    if w < s0 || w >= s1 {
                        continue;
                    }
                    let fq = lerp(freq.0, freq.1, own(p.id, o));
                    let r = lerp(rate.0, rate.1, own(p.id, o + 1));
                    let t = if proportional { p.normalized_age() } else { p.age };
                    let delta = (PI * (t * fq * mult + shift)).sin();
                    let v = p.get(field) + delta * r * dt * strength;
                    p.set(field, v);
                }
            }
            Op::OscillateVec { field, rate, freq, mult, add: shift, proportional } => {
                for p in self.parts.iter_mut() {
                    let r = [lerp(rate.0[0], rate.1[0], own(p.id, o + 3)), lerp(rate.0[1], rate.1[1], own(p.id, o + 7)), lerp(rate.0[2], rate.1[2], own(p.id, o + 9))];
                    let fq = [lerp(freq.0[0], freq.1[0], own(p.id, o + 8)), lerp(freq.0[1], freq.1[1], own(p.id, o + 12)), lerp(freq.0[2], freq.1[2], own(p.id, o + 15))];
                    let t = if proportional { p.normalized_age() } else { p.age };
                    let delta: V3 = std::array::from_fn(|i| (PI * (t * fq[i] * mult + shift)).sin() * r[i] * dt * strength);
                    let v = add(p.get_v(field), delta);
                    p.set_v(field, v);
                }
            }
            Op::Radius { start, end, ref from, ref to, ease, bias: ref bb } => {
                for (p, b) in self.parts.iter_mut().zip(&self.born) {
                    let t = p.normalized_age();
                    if t < start || t > end {
                        continue;
                    }
                    let (a, z) = (eval(from, &mut self.rng, cps), eval(to, &mut self.rng, cps));
                    let mut s = if end > start { (t - start) / (end - start) } else { 1.0 };
                    if ease {
                        s = s * s * (3.0 - 2.0 * s);
                    }
                    s = s.powf(1.0 - eval(bb, &mut self.rng, cps));
                    p.radius = b.radius * lerp(a, z, s);
                }
            }
            Op::Cull { cp, ref distance, offset, inside } => {
                let r = eval(distance, &mut self.rng, cps);
                let c = add(cps[cp].pos, offset);
                for p in self.parts.iter_mut() {
                    let edge = len(sub(p.pos, c)) - r;
                    if (inside && edge < 0.0) || (!inside && edge > 0.0) {
                        p.dead = true;
                    }
                }
            }
            Op::LoopInSphere { cp, distance, scale } => {
                let c = cps[cp].pos;
                for p in self.parts.iter_mut() {
                    let d3 = sub(p.pos, c);
                    let scaled = [d3[0] * scale[0], d3[1] * scale[1], d3[2] * scale[2]];
                    let l = len(scaled);
                    if l > distance && l > 1e-6 {
                        // out of it: in again from the other side, going on as it went
                        let by = mul(scaled, 2.0 * distance / l);
                        p.pos = sub(p.pos, by);
                        p.prev = sub(p.prev, by);
                    }
                }
            }
            Op::VecNoise { field, lo, hi, scale, time_scale, additive } => {
                let half = mul(sub(hi, lo), 0.5);
                let base = add(half, lo);
                let t = time_scale * self.age;
                for p in self.parts.iter_mut() {
                    let c = add(mul(p.pos, scale), [t, 0.0, 0.0]);
                    let c2 = add(c, [100_000.5, 300_000.25, 9_000_001.0]);
                    let c3 = add(c2, [110_000.25, 310_000.75, 9_100_000.0]);
                    let n = [noise3(c[0], c[1], c[2]), noise3(c2[0], c2[1], c2[2]), noise3(c3[0], c3[1], c3[2])];
                    let mut v = [n[0] * half[0] + base[0], n[1] * half[1] + base[1], n[2] * half[2] + base[2]];
                    if !additive {
                        p.set_v(field, v);
                        continue;
                    }
                    v = mul(v, dt * strength);
                    let now = add(p.get_v(field), v);
                    p.set_v(field, now);
                    if field == POS {
                        p.prev = add(p.prev, v);
                    }
                }
            }
            Op::Clamp { field, ref lo, ref hi } => {
                let (mut a, mut z) = (eval(lo, &mut self.rng, cps), eval(hi, &mut self.rng, cps));
                if a > z {
                    std::mem::swap(&mut a, &mut z);
                }
                for p in self.parts.iter_mut() {
                    let v = p.get(field);
                    p.set(field, lerp(v, v.clamp(a, z), strength));
                }
            }
            Op::ColorFade { to, start, end, ease } => {
                for (p, b) in self.parts.iter_mut().zip(&self.born) {
                    let t = p.normalized_age();
                    if t < start || t > end {
                        continue;
                    }
                    let mut s = if end > start { (t - start) / (end - start) } else { 1.0 };
                    if ease {
                        s = s * s * (3.0 - 2.0 * s);
                    }
                    let faded = lerp3(b.color, to, s);
                    p.color = lerp3(b.color, faded, strength);
                }
            }
            Op::RampSpline { field, rate, start, end, ease_out } => {
                for p in self.parts.iter_mut() {
                    let t = p.normalized_age();
                    if t <= start || t >= end {
                        continue;
                    }
                    let r = lerp(rate.0, rate.1, own(p.id, o));
                    // easing out: fast at first, slowing to nothing (the same way in all)
                    let k = if ease_out { 2.0 * (1.0 - (t - start) / (end - start)) } else { 1.0 };
                    let v = p.get(field) + r * dt * k * strength;
                    p.set(field, v);
                }
            }
            Op::PlaceOnGround { offset, max, trace_offset } => {
                for p in self.parts.iter_mut() {
                    let from = add(p.pos, [0.0, 0.0, trace_offset]);
                    if let Some(g) = ground(from, max + trace_offset) {
                        let z = g + offset;
                        p.prev[2] += z - p.pos[2];
                        p.pos[2] = z;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_open_and_close_over_their_period() {
        let d = Def::read("t", &serde_json::json!({"m_ForceGenerators": [ {"_class": "C_OP_RandomForce", "m_flOpStartFadeOutTime": 0.1, "m_flOpEndFadeOutTime": 0.1, "m_flOpFadeOscillatePeriod": 10.0} ]}));
        let win = d.forces[0].w;
        // the first tenth of each ten seconds, then nothing till the next
        assert_eq!(win.strength(0.5), 1.0);
        assert_eq!(win.strength(5.0), 0.0);
        assert_eq!(win.strength(10.5), 1.0);
    }

    #[test]
    fn noise_stays_in_its_range() {
        for i in 0..1000 {
            let v = noise3(i as f32 * 0.37, i as f32 * 1.1, -(i as f32) * 0.71);
            assert!((-1.0..=1.0).contains(&v));
        }
    }
}
