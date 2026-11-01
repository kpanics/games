//! A charm swinging on its weapon (CS2's keychain: a chain on its "keychain" attachment, a pendant at its end): one
//! pendulum from the chain's top, kept in the view's own space — the world's down turned into it each frame, the
//! view's turn carried back into it (a charm hangs in the world, so turning the head swings it), the weapon's own
//! movement and the player's shaking it — the chain turned about its top as the pendulum is; the weapon's side a wall
//! it never swings into (the gun's body would hide it), leaning out as far as the body under the ring bulges.

use super::model::{mul, M4};

/// The fall (m/s²) and how fast a swing dies down (each second, its speed's share kept).
const GRAVITY: f32 = 9.8;
const DAMPING: f32 = 2.2;
/// How far over its ring a charm may swing (the cosine of its angle from straight down): a little past level, never up
/// into the weapon over the ring.
const HIGHEST: f32 = -0.34;

#[derive(Clone, Debug, Default)]
pub struct Swing {
    /// The pendant's place from the top (view space), now and a frame ago.
    tip: [f32; 3],
    prev: [f32; 3],
    /// The top's place (view space) the two frames before, and the view's basis the frame before.
    tops: [[f32; 3]; 2],
    basis: Option<[[f32; 3]; 3]>,
    frames: u32,
    /// The player's acceleration, eased (the game's velocity moves a tick at a time).
    accel: [f32; 3],
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(a: [f32; 3], k: f32) -> [f32; 3] {
    a.map(|x| x * k)
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn len(a: [f32; 3]) -> f32 {
    dot(a, a).sqrt()
}

/// `a`, no longer than `most`.
fn limit(a: [f32; 3], most: f32) -> [f32; 3] {
    let l = len(a);
    if l > most {
        scale(a, most / l)
    } else {
        a
    }
}

/// A point through a matrix (column-major, as M4).
pub fn point(m: &M4, p: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|k| m[k] * p[0] + m[4 + k] * p[1] + m[8 + k] * p[2] + m[12 + k])
}

fn unit(v: [f32; 3]) -> [f32; 3] {
    scale(v, 1.0 / len(v).max(1e-9))
}

/// The weapon's side a charm held by `held` hangs from, as a wall (out of it, unit): its own x the way out (its
/// chain's first link goes so: CS2 makes them so), tilted out by `lean` (radians) at its foot.
pub fn wall(held: &M4, lean: f32) -> [f32; 3] {
    let (out, up) = (unit([held[0], held[1], held[2]]), unit([held[4], held[5], held[6]]));
    add(scale(out, lean.cos()), scale(up, lean.sin()))
}

/// How far out of its down a charm hanging from `held` (on the weapon as it stands) leans to clear the weapon's body
/// under its ring (`body`: the weapon's vertices, the same space), its pendant `length` long and `width` across
/// (radians; the ring's own few millimetres left to it).
pub fn lean(held: &M4, body: &[[f32; 3]], length: f32, width: f32) -> f32 {
    let (out, up, along) = (unit([held[0], held[1], held[2]]), unit([held[4], held[5], held[6]]), unit([held[8], held[9], held[10]]));
    let top = [held[12], held[13], held[14]];
    let mut need = 0f32;
    for p in body {
        let d = sub(*p, top);
        let (o, h, a) = (dot(d, out), -dot(d, up), dot(d, along));
        if (0.01..=length).contains(&h) && a.abs() <= width * 0.5 + 0.003 {
            need = need.max((o + 0.001).atan2(h));
        }
    }
    need.min(1.0)
}

/// The places a charm may hang on a body (its markup's quads: the model's own space as Source has it — x ahead, y
/// left, z up, inches — whatever bone they name), the model as bound (glTF's metres: x left, y up, z ahead), as
/// triangles.
pub fn places(mk: &super::sticker::Markup, legacy: bool) -> Vec<[[f32; 3]; 3]> {
    let mut out = Vec::new();
    for q in mk.charms.iter().filter(|q| q.legacy == legacy) {
        let c = q.corners.map(|p| [p[1] * 0.0254, p[2] * 0.0254, p[0] * 0.0254]);
        out.push([c[0], c[1], c[2]]);
        out.push([c[1], c[3], c[2]]);
    }
    out
}

/// The point of a triangle nearest `p` (Ericson's).
fn nearest_on(p: [f32; 3], [a, b, c]: [[f32; 3]; 3]) -> [f32; 3] {
    let (ab, ac, ap) = (sub(b, a), sub(c, a), sub(p, a));
    let (d1, d2) = (dot(ab, ap), dot(ac, ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = sub(p, b);
    let (d3, d4) = (dot(ab, bp), dot(ac, bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return add(a, scale(ab, d1 / (d1 - d3)));
    }
    let cp = sub(p, c);
    let (d5, d6) = (dot(ab, cp), dot(ac, cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return add(a, scale(ac, d2 / (d2 - d6)));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return add(b, scale(sub(c, b), (d4 - d3) / ((d4 - d3) + (d5 - d6))));
    }
    let denom = 1.0 / (va + vb + vc);
    add(a, add(scale(ab, vb * denom), scale(ac, vc * denom)))
}

/// The point of the places nearest `p`.
pub fn nearest(places: &[[[f32; 3]; 3]], p: [f32; 3]) -> Option<[f32; 3]> {
    places.iter().map(|t| nearest_on(p, *t)).min_by(|a, b| len(sub(*a, p)).total_cmp(&len(sub(*b, p))))
}

/// A world vector in the view's space (the basis' rows: right, up, back).
fn into_view(b: &[[f32; 3]; 3], w: [f32; 3]) -> [f32; 3] {
    [dot(b[0], w), dot(b[1], w), dot(b[2], w)]
}

/// A view vector into the world.
fn out_of_view(b: &[[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
    add(add(scale(b[0], v[0]), scale(b[1], v[1])), scale(b[2], v[2]))
}

/// The turn taking `a` onto `b` (unit vectors), as a matrix (column-major, as M4).
fn turn_between(a: [f32; 3], b: [f32; 3]) -> M4 {
    let c = dot(a, b).clamp(-1.0, 1.0);
    let axis = cross(a, b);
    let s = len(axis);
    if s < 1e-6 {
        return super::model::IDENTITY;
    }
    let k = scale(axis, 1.0 / s);
    let t = 1.0 - c;
    let (x, y, z) = (k[0], k[1], k[2]);
    [
        t * x * x + c,
        t * x * y + s * z,
        t * x * z - s * y,
        0.0,
        t * x * y - s * z,
        t * y * y + c,
        t * y * z + s * x,
        0.0,
        t * x * z + s * y,
        t * y * z - s * x,
        t * z * z + c,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
    ]
}

impl Swing {
    /// One frame: `top` and `rest` the chain's top and its hanging direction as held (view space; `rest`'s length
    /// the chain's), `side` the weapon's side there (out of it, a unit vector; none: zero), `basis` the view's (right,
    /// up, back in the world), `accel` the player's own (m/s², the world's). What turns the charm about its top (view
    /// space).
    #[allow(clippy::too_many_arguments)]
    pub fn step(&mut self, top: [f32; 3], rest: [f32; 3], side: [f32; 3], basis: [[f32; 3]; 3], accel: [f32; 3], dt: f32) -> M4 {
        let length = len(rest).max(1e-4);
        if self.frames == 0 {
            self.tip = rest;
            self.prev = rest;
            self.tops = [top, top];
        }
        let dt = dt.clamp(1e-4, 0.05);
        // the view turned since: what hangs in the world turns the other way in it
        if let Some(b0) = self.basis {
            let carry = |v: [f32; 3]| into_view(&basis, out_of_view(&b0, v));
            self.tip = carry(self.tip);
            self.prev = carry(self.prev);
        }
        self.basis = Some(basis);
        // the forces: the fall, the weapon's own shaking (its top's), the player's — eased (its ticks no kicks), a fall
        // no more than the charm's own (the game's falls three times as hard as the world's: falling, a charm floats,
        // it is never thrown up), the rest within what a body does
        let fall = into_view(&basis, [0.0, -GRAVITY, 0.0]);
        let shake = if self.frames >= 2 { limit(scale(add(sub(top, scale(self.tops[0], 2.0)), self.tops[1]), 1.0 / (dt * dt)), 4.0 * GRAVITY) } else { [0.0; 3] };
        self.accel = add(self.accel, scale(sub(accel, self.accel), 1.0 - (-dt * 10.0).exp()));
        let mut own = self.accel;
        own[1] = own[1].clamp(-GRAVITY, 2.0 * GRAVITY);
        let flat = limit([own[0], 0.0, own[2]], 2.0 * GRAVITY);
        own = [flat[0], own[1], flat[2]];
        let force = sub(sub(fall, shake), into_view(&basis, own));
        let keep = (-DAMPING * dt).exp();
        let mut next = add(add(self.tip, scale(sub(self.tip, self.prev), keep)), scale(force, dt * dt));
        next = scale(next, length / len(next).max(1e-6));
        // the weapon's side: along it, never into it
        let into = dot(next, side);
        if into < 0.0 {
            next = sub(next, scale(side, into));
            next = scale(next, length / len(next).max(1e-6));
        }
        // nor up over its ring into the weapon (stopped there, not bounced)
        let down = unit(fall);
        for _ in 0..3 {
            let d = dot(next, down) / length;
            if d >= HIGHEST {
                break;
            }
            next = add(next, scale(down, (HIGHEST - d) * length));
            next = scale(next, length / len(next).max(1e-6));
            self.tip = next;
        }
        self.prev = self.tip;
        self.tip = next;
        self.tops = [top, self.tops[0]];
        self.frames = self.frames.saturating_add(1);
        let r = turn_between(scale(rest, 1.0 / length), scale(self.tip, 1.0 / length));
        // about the top
        let to: M4 = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, top[0], top[1], top[2], 1.0];
        let from: M4 = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, -top[0], -top[1], -top[2], 1.0];
        mul(&to, &mul(&r, &from))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_charm_hangs_down_and_settles() {
        let mut s = Swing::default();
        // looking ahead (right x, up y, back z); the chain held out sideways
        let basis = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        for _ in 0..600 {
            s.step([0.0; 3], [0.03, 0.0, 0.0], [0.0; 3], basis, [0.0; 3], 1.0 / 120.0);
        }
        let d = scale(s.tip, 1.0 / len(s.tip));
        assert!(d[1] < -0.99, "{d:?}");
        // the view turned a quarter about up: what hung down still hangs down
        let turned = [[0.0, 0.0, -1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]];
        s.step([0.0; 3], [0.03, 0.0, 0.0], [0.0; 3], turned, [0.0; 3], 1.0 / 120.0);
        assert!(scale(s.tip, 1.0 / len(s.tip))[1] < -0.99);
    }

    #[test]
    fn a_charm_leans_out_over_the_body_under_its_ring() {
        // held upright at the origin (out x, up y, along z); a bulge 1 cm out, 3 cm down
        let held = crate::cosmetics::model::IDENTITY;
        let body = [[0.01, -0.03, 0.0], [0.05, -0.03, 0.2], [-0.02, -0.02, 0.0], [0.02, -0.002, 0.0]];
        let l = lean(&held, &body, 0.05, 0.03);
        assert!((l - (0.011f32).atan2(0.03)).abs() < 1e-5, "{l}");
        let w = wall(&held, l);
        assert!(w[0] > 0.9 && w[1] > 0.3, "{w:?}");
    }

    #[test]
    fn the_nearest_point_of_a_place() {
        let close = |a: [f32; 3], b: [f32; 3]| (0..3).all(|k| (a[k] - b[k]).abs() < 1e-5);
        let tri = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        assert!(close(nearest_on([0.2, 0.2, 5.0], tri), [0.2, 0.2, 0.0]));
        assert!(close(nearest_on([-1.0, -1.0, 0.0], tri), [0.0, 0.0, 0.0]));
        assert!(close(nearest_on([1.0, 1.0, 0.0], tri), [0.5, 0.5, 0.0]));
        let far = [[[5.0, 5.0, 5.0], [6.0, 5.0, 5.0], [5.0, 6.0, 5.0]], tri];
        assert!(nearest(&far, [0.1, 0.1, 0.1]).is_some_and(|p| close(p, [0.1, 0.1, 0.0])));
    }

    #[test]
    fn a_charm_floats_in_a_jump_and_never_swings_up_into_the_weapon() {
        let mut s = Swing::default();
        let basis = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let rest = [0.0, -0.03, 0.0];
        for _ in 0..240 {
            s.step([0.0; 3], rest, [1.0, 0.0, 0.0], basis, [0.0; 3], 1.0 / 120.0);
        }
        // a jump: the game's push up a tick, then its fall (32 m/s², three times the world's) for half a second, the
        // velocity moving a tick at a time (a kick each sixth frame)
        for f in 0..120 {
            let a = if f < 6 { [0.0, 60.0, 0.0] } else if f % 6 == 0 { [0.0, -32.0 * 6.0, 0.0] } else { [0.0; 3] };
            s.step([0.0; 3], rest, [1.0, 0.0, 0.0], basis, a, 1.0 / 120.0);
            let d = scale(s.tip, 1.0 / len(s.tip));
            assert!(d[1] <= -HIGHEST + 1e-4, "frame {f}: over its ring {d:?}");
        }
    }

    #[test]
    fn a_charm_rests_against_the_side_it_would_fall_into() {
        let mut s = Swing::default();
        let basis = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        // the weapon's side faces up a little: the fall would take the charm into it
        let side = [0.0, 0.6, 0.8];
        for _ in 0..600 {
            s.step([0.0; 3], [0.0, 0.0, 0.03], side, basis, [0.0; 3], 1.0 / 120.0);
            assert!(dot(s.tip, side) > -1e-5, "{:?}", s.tip);
        }
        // lying along the side, as low as it lets it
        let d = scale(s.tip, 1.0 / len(s.tip));
        assert!(dot(d, side).abs() < 1e-3 && d[1] < -0.79, "{d:?}");
    }
}
