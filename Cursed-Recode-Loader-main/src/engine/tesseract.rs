//! Target ESP (the menu's module "target-esp", shaders/engine/tesseract.comp): the one aimed at in a tesseract — the
//! shadow of a four-dimensional cube turning round its chest, a cube in a cube whose corners are joined, the inner one
//! forever coming out through the outer. Its edges, corners and their trails are lines of light drawn over the level's
//! picture against its depth (a wall, the target's own body hides what is behind it); its corners are lamps on what is
//! round it (the target, the ground), never through a block.
//!
//! The targets come from Java (cursed.recode.render.TargetEsp) each frame: who, whether aimed at, dying, just hit, where
//! their feet are, how large they are. Each mark lives here: it unfolds when aimed at, folds away when let go, jerks and
//! flashes on a hit, and when its target dies its edges come off one by one, fall, bounce and go out.

use super::gpu::{image_in, invert, retire, Compute};
use super::ink::{bytes, dispatch, srgb_of, Bind};
use crate::vk::device::Gpu;
use crate::vk::encoder::{memory_barrier, Encoder};
use crate::vk::resources::{Sampler, Texture, View};
use ash::vk;
use std::collections::VecDeque;
use std::time::Instant;

/// What Java says of a target (cursed.recode.render.TargetEsp's bits).
pub const AIMED: i32 = 1;
pub const DYING: i32 = 2;
pub const HIT: i32 = 4;
/// The most targets a frame brings, and the most marks drawn at once.
pub const MOST_TARGETS: usize = 4;
const MOST_MARKS: usize = 3;
/// The most segments of light, lamps (tesseract.comp's).
const MOST_SEGS: usize = 360;
const LAMPS: usize = 4;
const UBO: u64 = 304 + LAMPS as u64 * 16 + MOST_SEGS as u64 * 32 + MOST_BOXES as u64 * 64;
/// How far w is seen from (the nearer in w the larger): unturned, the inner cube is (D4 - 1) / (D4 + 1) of the outer;
/// turned, a corner's w is never past the square root of 2 (the turns that change it are in one plane), so it never
/// swells past (D4 - 1) / (D4 - √2) of the outer.
const D4: f32 = 2.6;
/// How far a segment's glow reaches, in its core's radii (tesseract.comp's `reach`).
const GLOW_REACH: f32 = 16.0;
/// The turn round each plane, radians a second as made: xw (the inner cube coming out through the outer), yz with it
/// (the two together a double turn: they never meet), xz (our space's, slow).
const RATES: [f32; 3] = [0.55, 0.31, 0.15];
/// Trail samples a second, how many are kept.
const TRAIL_RATE: f32 = 40.0;
const TRAIL: usize = 7;

/// The most boxes of a target's model (cursed.recode.render.TargetEsp MOST_BOXES).
pub const MOST_BOXES: usize = 48;

/// A target this frame.
#[derive(Clone, Default)]
pub struct Target {
    pub id: i32,
    pub flags: i32,
    /// Its feet, from the camera.
    pub feet: [f32; 3],
    pub width: f32,
    pub height: f32,
    /// Which way its body faces (the game's yaw, degrees).
    pub yaw: f32,
    /// Its model's boxes as drawn this frame (none when it was not drawn): the middle (from the camera; w 1 a box of the
    /// head), the three half sides as vectors.
    pub boxes: Vec<[f32; 16]>,
}

/// The module's settings: on, the colour (linear), how fast it turns, how large, trails, a flash on a hit, its light
/// on the world and how strong; 1 each as made.
#[derive(Clone, Copy)]
pub struct Settings {
    pub on: bool,
    pub colour: [f32; 3],
    pub speed: f32,
    pub size: f32,
    pub trails: bool,
    pub flash: bool,
    pub light: bool,
    pub light_strength: f32,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings { on: true, colour: [1.0; 3], speed: 1.0, size: 1.0, trails: true, flash: true, light: true, light_strength: 1.0 }
    }
}

impl Settings {
    /// From the menu's "set" line: "1\tLabel=value…".
    pub fn parse(text: &str) -> Settings {
        let mut o = Settings::default();
        let mut f = text.split('\t');
        o.on = f.next() == Some("1");
        for kv in f {
            let Some((k, v)) = kv.split_once('=') else { continue };
            let n = v.parse::<f32>().ok();
            match k {
                "Colour" => o.colour = u32::from_str_radix(v, 16).map(srgb_of).unwrap_or(o.colour),
                "Speed" => o.speed = n.unwrap_or(o.speed).clamp(0.0, 5.0),
                "Size" => o.size = n.unwrap_or(o.size).clamp(0.2, 4.0),
                "Trails" => o.trails = v == "1",
                "Flash on a hit" => o.flash = v == "1",
                "Light on the world" => o.light = v == "1",
                "Light strength" => o.light_strength = n.unwrap_or(o.light_strength).clamp(0.0, 4.0),
                _ => {}
            }
        }
        o
    }
}

/// The tesseract's 16 corners: ±1 in each of x, y, z, w (bit k of the index the k-th's sign).
pub fn corners() -> [[f32; 4]; 16] {
    std::array::from_fn(|i| std::array::from_fn(|k| if i >> k & 1 == 1 { 1.0 } else { -1.0 }))
}

/// Its 32 edges: the corners one coordinate apart.
pub fn edges() -> Vec<(usize, usize)> {
    let mut e = Vec::with_capacity(32);
    for i in 0..16usize {
        for j in i + 1..16 {
            if (i ^ j).count_ones() == 1 {
                e.push((i, j));
            }
        }
    }
    e
}

fn turn(p: &mut [f32; 4], a: usize, b: usize, t: f32) {
    let (s, c) = t.sin_cos();
    let (x, y) = (p[a], p[b]);
    p[a] = c * x - s * y;
    p[b] = s * x + c * y;
}

/// The corners turned by `angles` (planes xw, yz, then xz) and seen in our space from w = D4 (the nearer in w the
/// larger): each where it is from the middle, the outer cube's half side `half` unturned, and its w (-1 the inner .. 1
/// the outer).
pub fn shadow(angles: [f32; 3], half: f32) -> [([f32; 3], f32); 16] {
    let s = half * (D4 - 1.0) / D4;
    corners().map(|c| {
        let mut p = c;
        turn(&mut p, 0, 3, angles[0]);
        turn(&mut p, 1, 2, angles[1]);
        turn(&mut p, 0, 2, angles[2]);
        let k = D4 / (D4 - p[3]) * s;
        ([p[0] * k, p[1] * k, p[2] * k], p[3])
    })
}

/// The outer cube's half side round a target as large as this (blocks): at the chest, smaller than the body is tall.
pub fn half_for(width: f32, height: f32, size: f32) -> f32 {
    (0.17 * (width + height)).clamp(0.14, 1.4) * size
}

fn ease_out_back(x: f32) -> f32 {
    let (c1, c3) = (1.4f32, 2.4f32);
    1.0 + c3 * (x - 1.0).powi(3) + c1 * (x - 1.0).powi(2)
}

fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// A number of chance from a seed (0..1).
fn chance(seed: u32) -> f32 {
    let mut n = seed.wrapping_mul(0x9e37_79b9) ^ 0x85eb_ca6b;
    n ^= n >> 16;
    n = n.wrapping_mul(0x7feb_352d);
    n ^= n >> 15;
    n = n.wrapping_mul(0x846c_a68b);
    n ^= n >> 16;
    (n & 0xff_ffff) as f32 / 16_777_216.0
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn mul(a: [f32; 3], s: f32) -> [f32; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn norm(a: [f32; 3]) -> [f32; 3] {
    let l = dot(a, a).sqrt().max(1e-6);
    mul(a, 1.0 / l)
}

/// `v` turned round `axis` (a unit) by `a` radians.
fn spin(v: [f32; 3], axis: [f32; 3], a: f32) -> [f32; 3] {
    let (s, c) = a.sin_cos();
    add(add(mul(v, c), mul(cross(axis, v), s)), mul(axis, dot(axis, v) * (1.0 - c)))
}

/// An edge come off a dying target's tesseract: its middle (the world), half of it, how fast it flies and turns.
struct Piece {
    mid: [f64; 3],
    half: [f32; 3],
    vel: [f32; 3],
    axis: [f32; 3],
    turn: f32,
    after: f32,
    w: f32,
}

/// One target's mark.
struct Mark {
    id: i32,
    /// Its chest (the world), its feet's height, its outer cube's half side.
    mid: [f64; 3],
    floor: f64,
    half: f32,
    angles: [f32; 3],
    /// 0..1: unfolding while aimed at, folding away once let go.
    grow: f32,
    held: bool,
    kick: f32,
    flash: f32,
    /// The waves on the ground from its hits: their ages (s).
    waves: Vec<f32>,
    /// Where its corners were (the world), the newest last; when the next is due.
    trail: VecDeque<[[f64; 3]; 16]>,
    trail_due: f32,
    /// Since its death (s), and its edges coming off.
    death: Option<(f32, Vec<Piece>)>,
    /// Its target's feet (the world) and body's boxes from them (what hides the lines behind it).
    feet: [f64; 3],
    boxes: Vec<[f32; 16]>,
}

impl Mark {
    fn new(id: i32) -> Mark {
        // each mark starts turned its own way round us, a cube in a cube
        Mark {
            id,
            mid: [0.0; 3],
            floor: 0.0,
            half: 0.4,
            angles: [0.1, 0.0, chance(id as u32) * std::f32::consts::TAU],
            grow: 0.0,
            held: true,
            kick: 0.0,
            flash: 0.0,
            waves: Vec::new(),
            trail: VecDeque::new(),
            trail_due: 0.0,
            death: None,
            feet: [0.0; 3],
            boxes: Vec::new(),
        }
    }

    /// How large it is now (0..1 of its size).
    fn scale(&self) -> f32 {
        let g = self.grow.clamp(0.0, 1.0);
        let s = if self.held { ease_out_back(g) } else { smooth(g) };
        s * (1.0 + 0.1 * self.flash)
    }

    /// Its corners now (the world) and their w.
    fn corners(&self) -> [([f64; 3], f32); 16] {
        shadow(self.angles, self.half * self.scale()).map(|(p, w)| ([self.mid[0] + p[0] as f64, self.mid[1] + p[1] as f64, self.mid[2] + p[2] as f64], w))
    }

    fn die(&mut self) {
        let c = self.corners();
        let pieces = edges()
            .iter()
            .enumerate()
            .map(|(n, &(i, j))| {
                let (a, b) = (c[i].0, c[j].0);
                let mid = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5, (a[2] + b[2]) * 0.5];
                let half = [((b[0] - a[0]) * 0.5) as f32, ((b[1] - a[1]) * 0.5) as f32, ((b[2] - a[2]) * 0.5) as f32];
                let out = norm([(mid[0] - self.mid[0]) as f32, (mid[1] - self.mid[1]) as f32, (mid[2] - self.mid[2]) as f32]);
                let seed = (self.id as u32).wrapping_mul(97).wrapping_add(n as u32 * 7);
                let speed = 0.35 + 0.6 * chance(seed);
                Piece {
                    mid,
                    half,
                    vel: add(mul(out, speed), [0.0, 0.5 + 0.8 * chance(seed ^ 11), 0.0]),
                    axis: norm([chance(seed ^ 21) - 0.5, chance(seed ^ 31) - 0.5, chance(seed ^ 41) - 0.5]),
                    turn: (chance(seed ^ 51) - 0.5) * 9.0,
                    // they come off one by one, in an order of chance
                    after: 0.1 + chance(seed ^ 61) * 0.95,
                    w: (c[i].1 + c[j].1) * 0.5,
                }
            })
            .collect();
        self.death = Some((0.0, pieces));
        self.flash = 1.0;
        self.trail.clear();
    }

    /// One step of `dt` seconds; false once there is nothing of it left.
    fn step(&mut self, dt: f32, s: &Settings) -> bool {
        self.kick *= (-dt * 2.6).exp();
        self.flash *= (-dt * 4.5).exp();
        for w in &mut self.waves {
            *w += dt;
        }
        self.waves.retain(|w| *w < 1.0);
        if let Some((t, pieces)) = &mut self.death {
            *t += dt;
            let t = *t;
            for p in pieces.iter_mut().filter(|p| t > p.after) {
                p.vel[1] -= 6.5 * dt;
                for k in 0..3 {
                    p.mid[k] += (p.vel[k] * dt) as f64;
                }
                p.half = spin(p.half, p.axis, p.turn * dt);
                // the ground at its feet: it lands, bounces a little, slides, stops turning
                let low = p.mid[1] - p.half[1].abs() as f64;
                if low < self.floor {
                    p.mid[1] += self.floor - low;
                    if p.vel[1] < 0.0 {
                        p.vel = [p.vel[0] * 0.72, -p.vel[1] * 0.3, p.vel[2] * 0.72];
                        p.turn *= 0.7;
                    }
                }
            }
            return t < 1.05 + 2.8;
        }
        let spd = s.speed * (1.0 + self.kick);
        for k in 0..3 {
            self.angles[k] = (self.angles[k] + dt * RATES[k] * spd) % std::f32::consts::TAU;
        }
        self.grow = if self.held { (self.grow + dt / 0.45).min(1.0) } else { self.grow - dt / 0.28 };
        // the trail: while it is whole and aimed at
        if s.trails && self.held && self.grow >= 1.0 {
            self.trail_due += dt * TRAIL_RATE;
            if self.trail_due >= 1.0 || self.trail.is_empty() {
                self.trail_due = (self.trail_due - 1.0).max(0.0);
                self.trail.push_back(self.corners().map(|c| c.0));
                while self.trail.len() > TRAIL {
                    self.trail.pop_front();
                }
            }
        } else {
            self.trail.clear();
        }
        self.grow > 0.0
    }

    /// How brightly it shines now (0..1).
    fn shine(&self) -> f32 {
        match &self.death {
            Some((t, _)) => (1.0 - t / 1.4).max(0.0),
            None => smooth(self.grow),
        }
    }
}

/// The segments of light of one frame: each two ends from the camera, its core's radius, how bright.
#[derive(Default)]
struct Lines {
    segs: Vec<([f32; 4], [f32; 4])>,
}

impl Lines {
    fn push(&mut self, cam: [f64; 3], a: [f64; 3], b: [f64; 3], radius: f32, bright: f32) {
        if self.segs.len() >= MOST_SEGS || bright <= 0.003 {
            return;
        }
        let r = |p: [f64; 3]| [(p[0] - cam[0]) as f32, (p[1] - cam[1]) as f32, (p[2] - cam[2]) as f32];
        let (a, b) = (r(a), r(b));
        self.segs.push(([a[0], a[1], a[2], radius], [b[0], b[1], b[2], bright]));
    }
}

/// Boxes from the camera made from its feet.
pub(super) fn from_feet(boxes: &[[f32; 16]], feet: [f32; 3]) -> Vec<[f32; 16]> {
    boxes.iter().map(|b| {
        let mut o = *b;
        for k in 0..3 {
            o[k] = b[k] - feet[k];
        }
        o
    }).collect()
}

/// A body's boxes (from its feet; `feet` from the camera) into a shader's uniforms at `at`, four vectors each: the middle
/// from the camera, the three axes as units with the half side along each (`grow` more: a body a little larger).
pub(super) fn write_body(host: *mut u8, at: usize, boxes: &[[f32; 16]], feet: [f32; 3], grow: f32) {
    for (i, b) in boxes.iter().enumerate() {
        let ax = [[b[4], b[5], b[6]], [b[8], b[9], b[10]], [b[12], b[13], b[14]]];
        let len = ax.map(|a| (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt());
        let o = at + i * 64;
        bytes(host, o, &[b[0] + feet[0], b[1] + feet[1], b[2] + feet[2], 0.0]);
        for k in 0..3 {
            let u = if len[k] > 1e-5 {
                [ax[k][0] / len[k], ax[k][1] / len[k], ax[k][2] / len[k]]
            } else {
                // a flat side: across the other two
                let (a, c) = (ax[(k + 1) % 3], ax[(k + 2) % 3]);
                let n = [a[1] * c[2] - a[2] * c[1], a[2] * c[0] - a[0] * c[2], a[0] * c[1] - a[1] * c[0]];
                let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-6);
                [n[0] / l, n[1] / l, n[2] / l]
            };
            bytes(host, o + 16 + k * 16, &[u[0], u[1], u[2], len[k].max(0.01) + grow]);
        }
    }
}

/// `draw` over a copy of the frame's rectangle `rect` (pixels: lowest x, y, past the highest) in `canvas`, laid back
/// after (every style of Target ESP draws so: the frame's own format may not be one a shader writes).
pub(super) fn over_rect(gpu: &Gpu, cmd: vk::CommandBuffer, color: &Texture, canvas: &Texture, rect: [i32; 4], draw: impl FnOnce(vk::CommandBuffer)) {
    let d = &gpu.device;
    let range = vk::ImageSubresourceLayers { aspect_mask: vk::ImageAspectFlags::COLOR, mip_level: 0, base_array_layer: 0, layer_count: 1 };
    let at = vk::Offset3D { x: rect[0], y: rect[1], z: 0 };
    let to = vk::Offset3D { x: rect[2], y: rect[3], z: 1 };
    let extent = vk::Extent3D { width: (rect[2] - rect[0]) as u32, height: (rect[3] - rect[1]) as u32, depth: 1 };
    let same = color.format == vk::Format::R8G8B8A8_UNORM;
    let carry = |src: vk::Image, dst: vk::Image| unsafe {
        if same {
            let copy = vk::ImageCopy { src_subresource: range, src_offset: at, dst_subresource: range, dst_offset: at, extent };
            d.cmd_copy_image(cmd, src, vk::ImageLayout::GENERAL, dst, vk::ImageLayout::GENERAL, &[copy]);
        } else {
            let blit = vk::ImageBlit { src_subresource: range, src_offsets: [at, to], dst_subresource: range, dst_offsets: [at, to] };
            d.cmd_blit_image(cmd, src, vk::ImageLayout::GENERAL, dst, vk::ImageLayout::GENERAL, &[blit], vk::Filter::NEAREST);
        }
    };
    carry(color.image, canvas.image);
    memory_barrier(gpu, cmd);
    draw(cmd);
    carry(canvas.image, color.image);
    memory_barrier(gpu, cmd);
}

/// Where a point from the camera falls on the frame (pixels) and its distance along the view (clip w); None behind.
fn on_screen(m: &[f32; 16], p: [f32; 3], fw: f32, fh: f32) -> Option<([f32; 2], f32)> {
    let c = |r: usize| m[r] * p[0] + m[4 + r] * p[1] + m[8 + r] * p[2] + m[12 + r];
    let w = c(3);
    if w < 0.05 {
        return None;
    }
    Some(([(c(0) / w * 0.5 + 0.5) * fw, (c(1) / w * 0.5 + 0.5) * fh], w))
}

/// The rectangle of the frame (pixels: lowest x, y, past the highest) all of it can reach: each segment's ends with
/// their glow round them, and the light's sphere round each lamp's middle. The whole frame when any lies behind.
fn reach(m: &[f32; 16], lines: &Lines, light: Option<([f32; 3], f32)>, fw: u32, fh: u32) -> Option<[i32; 4]> {
    let (w, h) = (fw as f32, fh as f32);
    // pixels a block is at a block's distance: the projection's y scale (its view's rows are unit vectors)
    let ppb = (m[1] * m[1] + m[5] * m[5] + m[9] * m[9]).sqrt() * h * 0.5;
    let mut lo = [f32::MAX; 2];
    let mut hi = [f32::MIN; 2];
    let mut grow = |p: [f32; 3], r: f32| -> bool {
        match on_screen(m, p, w, h) {
            Some((s, d)) if d > r => {
                let margin = r * ppb / d + 2.0;
                lo = [lo[0].min(s[0] - margin), lo[1].min(s[1] - margin)];
                hi = [hi[0].max(s[0] + margin), hi[1].max(s[1] + margin)];
                true
            }
            _ => false,
        }
    };
    let mut whole = false;
    for (a, b) in &lines.segs {
        let r = a[3] * GLOW_REACH;
        whole |= !grow([a[0], a[1], a[2]], r) | !grow([b[0], b[1], b[2]], r);
    }
    if let Some((c, r)) = light {
        whole |= !grow(c, r);
    }
    if lines.segs.is_empty() && light.is_none() {
        return None;
    }
    if whole {
        return Some([0, 0, fw as i32, fh as i32]);
    }
    let r = [(lo[0].floor().max(0.0)) as i32, (lo[1].floor().max(0.0)) as i32, (hi[0].ceil().min(w)) as i32, (hi[1].ceil().min(h)) as i32];
    (r[2] > r[0] && r[3] > r[1]).then_some(r)
}

pub struct Tesseract {
    pipe: Option<Compute>,
    canvas: Option<(Box<Texture>, Box<View>)>,
    marks: Vec<Mark>,
    set: Settings,
    set_at: Option<Instant>,
    started: Instant,
    last: Option<Instant>,
}

impl Tesseract {
    pub fn new() -> Tesseract {
        Tesseract { pipe: None, canvas: None, marks: Vec::new(), set: Settings::default(), set_at: None, started: Instant::now(), last: None }
    }

    /// The menu's settings, asked four times a second.
    pub fn poll(&mut self) -> Settings {
        let now = Instant::now();
        if self.set_at.is_none_or(|t| now.duration_since(t).as_secs_f32() > 0.25) {
            self.set_at = Some(now);
            self.set = Settings::parse(&crate::gui::call("set\ttarget-esp"));
        }
        self.set
    }

    pub fn set_shader(&mut self, gpu: &Gpu, spirv: &[u32]) {
        use vk::DescriptorType as T;
        let kinds = [T::STORAGE_IMAGE, T::COMBINED_IMAGE_SAMPLER, T::COMBINED_IMAGE_SAMPLER, T::UNIFORM_BUFFER];
        if let Some(old) = self.pipe.replace(Compute::new(gpu, "target_esp", spirv, &kinds, 4)) {
            old.destroy(gpu);
        }
    }

    /// The marks stepped by this frame's targets.
    fn follow(&mut self, targets: &[Target], camera: [f64; 3], dt: f32) {
        let s = self.set;
        for t in targets {
            let i = match self.marks.iter().position(|m| m.id == t.id) {
                Some(i) => i,
                // a new mark only for one aimed at (a dying one let go is not marked again)
                None if t.flags & AIMED != 0 && t.flags & DYING == 0 => {
                    self.marks.push(Mark::new(t.id));
                    self.marks.len() - 1
                }
                None => continue,
            };
            let m = &mut self.marks[i];
            if m.death.is_some() {
                continue;
            }
            let feet = [camera[0] + t.feet[0] as f64, camera[1] + t.feet[1] as f64, camera[2] + t.feet[2] as f64];
            m.mid = [feet[0], feet[1] + (t.height * 0.6) as f64, feet[2]];
            m.floor = feet[1];
            m.feet = feet;
            if !t.boxes.is_empty() {
                m.boxes = from_feet(&t.boxes, t.feet);
            }
            m.half = half_for(t.width, t.height, s.size);
            m.held = t.flags & AIMED != 0;
            if t.flags & HIT != 0 && s.flash && m.grow > 0.0 {
                m.kick = 3.0;
                m.flash = 1.0;
                m.waves.push(0.0);
            }
            if t.flags & DYING != 0 && m.grow > 0.05 {
                m.die();
            }
        }
        // the ones Java no longer brings are let go (a dying one goes on falling)
        for m in &mut self.marks {
            if m.death.is_none() && !targets.iter().any(|t| t.id == m.id) {
                m.held = false;
            }
        }
        self.marks.retain_mut(|m| m.step(dt, &s));
        // the one aimed at first, then the youngest let go
        self.marks.sort_by_key(|m| (!m.held, m.death.is_some()));
        self.marks.truncate(MOST_MARKS * 2);
    }

    /// This frame's segments of light, and the lamps: the brightest mark's corners.
    fn lines(&self, cam: [f64; 3]) -> (Lines, Vec<[f32; 4]>, Option<([f32; 3], f32)>) {
        let s = &self.set;
        let mut out = Lines::default();
        let mut lamps = Vec::new();
        let mut light = None;
        for m in self.marks.iter().take(MOST_MARKS) {
            let shine = m.shine();
            let r = m.half * 0.026;
            if let Some((t, pieces)) = &m.death {
                for p in pieces {
                    let since = t - p.after;
                    let a = (1.0 - (since - 1.6) / 1.1).clamp(0.0, 1.0) * if since > 0.0 { 1.0 + 0.6 * (-since * 3.0).exp() } else { 1.0 };
                    let h = [p.half[0] as f64, p.half[1] as f64, p.half[2] as f64];
                    let (a3, b3) = ([p.mid[0] - h[0], p.mid[1] - h[1], p.mid[2] - h[2]], [p.mid[0] + h[0], p.mid[1] + h[1], p.mid[2] + h[2]]);
                    out.push(cam, a3, b3, r, a * (0.5 + 0.25 * (p.w + 1.0)));
                }
            } else {
                let c = m.corners();
                let lit = shine * (1.0 + 0.9 * m.flash);
                for (i, j) in edges() {
                    out.push(cam, c[i].0, c[j].0, r, lit * (0.42 + 0.29 * (c[i].1 + c[j].1 + 2.0)));
                }
                for (p, w) in &c {
                    out.push(cam, *p, *p, r * 2.4, lit * (0.6 + 0.2 * (w + 1.0)));
                }
                // each corner's trail, as two strokes (the oldest place to the middle one, fainter and thinner, then on
                // to the newest): only while it moves further than its glow (a hit's jerk; turning slowly it has none)
                let n = m.trail.len();
                if n >= 3 {
                    let (old, mid, new) = (&m.trail[0], &m.trail[n / 2], &m.trail[n - 1]);
                    for v in 0..16 {
                        let d = [new[v][0] - old[v][0], new[v][1] - old[v][1], new[v][2] - old[v][2]];
                        if ((d[0] * d[0] + d[1] * d[1] + d[2] * d[2]) as f32) < (r * 3.0) * (r * 3.0) {
                            continue;
                        }
                        out.push(cam, old[v], mid[v], r * 0.5, 0.12 * shine);
                        out.push(cam, mid[v], new[v], r * 0.8, 0.3 * shine);
                    }
                }
            }
            // the waves of its hits along the ground
            for &age in &m.waves {
                let rad = (0.3 + 1.8 * (1.0 - (1.0 - age).powi(3))) * m.half / 0.41;
                let y = m.floor + 0.03;
                let at = |k: usize| {
                    let a = k as f64 / 32.0 * std::f64::consts::TAU;
                    [m.mid[0] + a.cos() * rad as f64, y, m.mid[2] + a.sin() * rad as f64]
                };
                for k in 0..32 {
                    out.push(cam, at(k), at(k + 1), r * 0.7, 0.55 * (1.0 - age) * shine);
                }
            }
            // the lamps: the first mark's corners in four groups of four (alike in x and y: each group's middle moves as
            // the inner cube comes out through the outer)
            if lamps.is_empty() && s.light && s.light_strength > 0.0 && shine > 0.01 {
                let c = m.corners();
                let reach = 2.6 * (m.half / 0.41).max(1.0);
                for k in 0..4 {
                    let group = c.iter().enumerate().filter(|(i, _)| i & 3 == k).map(|(_, p)| p.0);
                    let mut at = [0.0f64; 3];
                    for p in group {
                        at = [at[0] + p[0] * 0.25, at[1] + p[1] * 0.25, at[2] + p[2] * 0.25];
                    }
                    let bright = s.light_strength * shine * (1.0 + 0.8 * m.flash) * 0.6;
                    lamps.push([(at[0] - cam[0]) as f32, (at[1] - cam[1]) as f32, (at[2] - cam[2]) as f32, bright]);
                }
                let mid = [(m.mid[0] - cam[0]) as f32, (m.mid[1] - cam[1]) as f32, (m.mid[2] - cam[2]) as f32];
                light = Some((mid, reach));
            }
        }
        (out, lamps, light)
    }

    /// One frame over `color` (`depth` the level's): the marks stepped by `targets` and drawn; the voxel window's blocks
    /// what stops their light.
    #[allow(clippy::too_many_arguments)]
    pub fn frame(
        &mut self,
        gpu: &Gpu,
        enc: &mut Encoder,
        color: &Texture,
        depth: &Texture,
        targets: &[Target],
        camera: [f64; 3],
        view_proj: [f32; 16],
        voxels: Option<(&View, &Sampler)>,
        window: [i32; 5],
        nearest: &Sampler,
    ) {
        let now = Instant::now();
        let dt = self.last.map_or(0.0, |l| now.duration_since(l).as_secs_f32()).min(0.1);
        self.last = Some(now);
        if !self.set.on {
            self.marks.clear();
        } else {
            self.follow(targets, camera, dt);
        }
        // (the voxel window is made by the world's first frame, before this one's: without it, nothing this frame)
        let (Some(pipe), Some((vox, vox_sampler))) = (self.pipe.as_ref(), voxels) else { return };
        if self.marks.is_empty() {
            if let Some((t, v)) = self.canvas.take() {
                retire(enc, t, v);
            }
            return;
        }
        let (fw, fh) = (color.width, color.height);
        if fw == 0 || fh == 0 {
            return;
        }
        let (lines, lamps, light) = self.lines(camera);
        let Some(rect) = reach(&view_proj, &lines, light, fw, fh) else { return };
        // the lines' own rectangle: the groups past it skip them
        let glow = reach(&view_proj, &lines, None, fw, fh).unwrap_or([0; 4]);
        let cmd = enc.cmd(gpu);
        if self.canvas.as_ref().is_some_and(|c| c.0.width != fw || c.0.height != fh) {
            let (t, v) = self.canvas.take().unwrap();
            retire(enc, t, v);
        }
        if self.canvas.is_none() {
            self.canvas = Some(image_in(gpu, cmd, "Target ESP over the frame", vk::Format::R8G8B8A8_UNORM, fw, fh, 1, false));
        }
        let piece = enc.transient.mapped(gpu, UBO, gpu.info.min_uniform_align.max(16) as u64, UBO, 1);
        let host = piece.host;
        let s = self.set;
        let ppb = (view_proj[1] * view_proj[1] + view_proj[5] * view_proj[5] + view_proj[9] * view_proj[9]).sqrt() * fh as f32 * 0.5;
        let block = [camera[0].floor(), camera[1].floor(), camera[2].floor()];
        let (mid, far) = light.unwrap_or(([0.0; 3], 0.0));
        bytes(host, 0, &view_proj);
        bytes(host, 64, &invert(&view_proj));
        bytes(host, 128, &[(now.duration_since(self.started).as_secs_f64() % 3600.0) as f32, lines.segs.len() as f32, fw as f32, fh as f32]);
        bytes(host, 144, &rect);
        bytes(host, 160, &[ppb, GLOW_REACH, lamps.len() as f32, far]);
        bytes(host, 176, &[s.colour[0], s.colour[1], s.colour[2], (s.light && !lamps.is_empty()) as i32 as f32]);
        bytes(host, 192, &[mid[0], mid[1], mid[2], 0.0]);
        bytes(host, 208, &[block[0] as i32, block[1] as i32, block[2] as i32, 0]);
        bytes(host, 224, &[(camera[0] - block[0]) as f32, (camera[1] - block[1]) as f32, (camera[2] - block[2]) as f32, 0.0]);
        bytes(host, 240, &[window[0], window[1], window[2], window[3]]);
        bytes(host, 256, &[window[4], 1, 0, 0]);
        bytes(host, 272, &glow);
        // the first mark's target's body
        let body: &[[f32; 16]] = self.marks.first().map_or(&[], |m| &m.boxes);
        let feet = self.marks.first().map_or([0.0; 3], |m| [(m.feet[0] - camera[0]) as f32, (m.feet[1] - camera[1]) as f32, (m.feet[2] - camera[2]) as f32]);
        bytes(host, 288, &[body.len().min(MOST_BOXES) as f32, 0.0, 0.0, 0.0]);
        write_body(host, 304 + LAMPS * 16 + MOST_SEGS * 32, &body[..body.len().min(MOST_BOXES)], feet, 0.0);
        for (i, l) in lamps.iter().take(LAMPS).enumerate() {
            bytes(host, 304 + i * 16, l);
        }
        for (i, (a, b)) in lines.segs.iter().enumerate() {
            bytes(host, 304 + LAMPS * 16 + i * 32, a);
            bytes(host, 304 + LAMPS * 16 + i * 32 + 16, b);
        }
        let ubo = Bind::Uniform(unsafe { (*piece.buffer).raw }, piece.offset, UBO);
        let depth_view = super::upscale::temp_view(gpu, enc, depth);
        let canvas = self.canvas.as_ref().unwrap();
        let set = [Bind::Storage(canvas.1.raw), Bind::Sampled(depth_view, nearest.raw), Bind::Sampled(vox.raw, vox_sampler.raw), ubo];
        enc.segment(gpu, cmd, "Engine", "target esp (compute)");
        over_rect(gpu, cmd, color, &canvas.0, rect, |cmd| {
            dispatch(gpu, cmd, pipe, &set, &0i32.to_ne_bytes(), [((rect[2] - rect[0]) as u32).div_ceil(8), ((rect[3] - rect[1]) as u32).div_ceil(8), 1]);
        });
        enc.segment_end(gpu, cmd);
    }

    pub fn destroy(&mut self, gpu: &Gpu) {
        if let Some(p) = self.pipe.take() {
            p.destroy(gpu);
        }
        if let Some((t, v)) = self.canvas.take() {
            super::ink::kill(gpu, t, v);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tesseract_has_its_corners_and_edges() {
        let e = edges();
        assert_eq!(e.len(), 32);
        // every corner on four edges
        for c in 0..16 {
            assert_eq!(e.iter().filter(|(i, j)| *i == c || *j == c).count(), 4);
        }
    }

    #[test]
    fn unturned_it_is_a_cube_in_a_cube() {
        let half = 0.4;
        let s = shadow([0.0; 3], half);
        let size = |p: [f32; 3]| p.iter().fold(0f32, |m, v| m.max(v.abs()));
        for (p, w) in s {
            // the outer cube's corners at its half side, the inner's at (D4 - 1) / (D4 + 1) of it
            let want = if w > 0.0 { half } else { half * (D4 - 1.0) / (D4 + 1.0) };
            assert!((size(p) - want).abs() < 1e-5, "{p:?} {w}");
        }
    }

    #[test]
    fn turned_it_never_swells_far() {
        let half = 0.4;
        let most = 2.0 * half * (D4 - 1.0) / (D4 - 2f32.sqrt());
        for k in 0..400 {
            let a = k as f32 * 0.137;
            for (p, w) in shadow([a, a * 0.7, a * 0.3], half) {
                assert!(w.abs() <= 2f32.sqrt() + 1e-5);
                assert!(dot(p, p).sqrt() <= most + 1e-4, "{p:?}");
            }
        }
        assert!(most < half * 3.0);
    }

    #[test]
    fn its_size_follows_the_body() {
        let player = half_for(0.6, 1.8, 1.0);
        assert!((0.38..0.45).contains(&player), "{player}");
        assert!(half_for(0.4, 0.7, 1.0) < player);
        assert!(half_for(4.0, 4.0, 1.0) > player);
        assert!((half_for(0.6, 1.8, 2.0) - player * 2.0).abs() < 1e-5);
    }

    #[test]
    fn settings_from_the_menu() {
        let s = Settings::parse("1\tStyle=Tesseract\tColour=ff0000\tSpeed=1.5\tSize=0.5\tTrails=0\tFlash on a hit=0\tLight on the world=0\tLight strength=0.25");
        assert!(s.on && !s.trails && !s.flash && !s.light);
        assert_eq!((s.speed, s.size, s.light_strength), (1.5, 0.5, 0.25));
        assert!((s.colour[0] - 1.0).abs() < 1e-4 && s.colour[1] < 1e-4);
        assert!(!Settings::parse("0\tSpeed=1").on);
        assert!(!Settings::parse("").on);
    }

    #[test]
    fn a_mark_unfolds_folds_and_dies() {
        let mut t = Tesseract::new();
        let target = Target { id: 7, flags: AIMED, feet: [0.0, 0.0, 3.0], width: 0.6, height: 1.8, ..Default::default() };
        for _ in 0..40 {
            t.follow(std::slice::from_ref(&target), [0.0; 3], 1.0 / 60.0);
        }
        assert_eq!(t.marks.len(), 1);
        assert!(t.marks[0].grow >= 1.0);
        // its chest, not its feet
        assert!((t.marks[0].mid[1] - 1.08).abs() < 1e-4);
        // a hit: it jerks and flashes, a wave goes along the ground
        t.follow(&[Target { flags: AIMED | HIT, ..target.clone() }], [0.0; 3], 1.0 / 60.0);
        assert!(t.marks[0].kick > 2.0 && t.marks[0].flash > 0.9 && t.marks[0].waves.len() == 1);
        // let go: it folds away and is gone
        for _ in 0..30 {
            t.follow(&[], [0.0; 3], 1.0 / 60.0);
        }
        assert!(t.marks.is_empty());
        // aimed at again, then dying: its edges come off, fall onto its floor and go out
        for _ in 0..40 {
            t.follow(std::slice::from_ref(&target), [0.0; 3], 1.0 / 60.0);
        }
        t.follow(&[Target { flags: AIMED | DYING, ..target.clone() }], [0.0; 3], 1.0 / 60.0);
        assert!(t.marks[0].death.is_some());
        for _ in 0..120 {
            t.follow(&[], [0.0; 3], 1.0 / 60.0);
        }
        let (_, pieces) = t.marks[0].death.as_ref().unwrap();
        assert!(pieces.iter().all(|p| p.mid[1] - p.half[1].abs() as f64 >= -1e-6));
        for _ in 0..240 {
            t.follow(&[], [0.0; 3], 1.0 / 60.0);
        }
        assert!(t.marks.is_empty());
    }

    #[test]
    fn its_lines_fit_the_shader() {
        let mut t = Tesseract::new();
        for k in 0..MOST_TARGETS as i32 {
            for _ in 0..90 {
                let targets: Vec<Target> = (0..=k).map(|i| Target { id: i, flags: if i == k { AIMED | HIT } else { 0 }, feet: [i as f32, 0.0, 4.0], width: 0.6, height: 1.8, ..Default::default() }).collect();
                t.follow(&targets, [0.0; 3], 1.0 / 60.0);
            }
        }
        let (lines, lamps, light) = t.lines([0.0; 3]);
        assert!(lines.segs.len() <= MOST_SEGS && lamps.len() <= LAMPS && light.is_some());
        assert!(UBO <= 16384, "the uniform buffer fits every GPU's least");
    }

    #[test]
    fn its_rectangle_holds_it() {
        // a camera at the origin looking down -z, 70 degrees, 16:9
        let f = 1.0 / (35f32.to_radians().tan());
        let (n, fa) = (0.05f32, 1000.0f32);
        let a = 16.0 / 9.0;
        // (column-major: Vulkan's clip, y down, depth reversed does not matter here)
        let m = [f / a, 0.0, 0.0, 0.0, 0.0, -f, 0.0, 0.0, 0.0, 0.0, n / (fa - n), -1.0, 0.0, 0.0, fa * n / (fa - n), 0.0];
        let mut lines = Lines::default();
        lines.push([0.0; 3], [0.0, 0.0, -5.0], [0.0, 0.0, -5.0], 0.01, 1.0);
        let r = reach(&m, &lines, None, 1920, 1080).unwrap();
        assert!(r[0] < 960 && r[2] > 960 && r[1] < 540 && r[3] > 540);
        assert!(r[2] - r[0] < 400, "{r:?}");
        // something behind the camera: the whole frame
        lines.push([0.0; 3], [0.0, 0.0, 1.0], [0.0, 0.0, -5.0], 0.01, 1.0);
        assert_eq!(reach(&m, &lines, None, 1920, 1080), Some([0, 0, 1920, 1080]));
    }
}
