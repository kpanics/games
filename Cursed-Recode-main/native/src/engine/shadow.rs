//! Chams' shadow in the air (the menu's Look "Shadow", shaders/engine/shadow.comp): the bodies in it are thick black
//! smoke (their polygons, chams.glsl ch_shadow, with the creature's own eyes glowing); round them the smoke frays —
//! a ragged edge beyond the body, wisps rising off it — and puffs of it trail behind them as they go. A body is its
//! model's boxes (cursed.recode.chams.ChamsLiquid: each cube where its pose is this frame); the puffs live here, in the
//! world, drifting up and fading.

use super::gpu::{image_in, invert, retire, Compute};
use super::ink::{bytes, dispatch, Bind, Look};
use super::liquid::LiquidBox;
use crate::vk::device::Gpu;
use crate::vk::encoder::Encoder;
use crate::vk::resources::{Sampler, Texture, View};
use ash::vk;
use std::collections::HashMap;
use std::time::Instant;

/// The most boxes (engine/liquid.rs MOST_BOXES) and puffs (shadow.comp's); the nearest bodies whose smoke frays round
/// them (the others only trail their puffs: a crowd of them would cost as much as they are many).
const MOST_BOXES: usize = 128;
const FRAYING: usize = 4;
const MOST_PUFFS: usize = 96;
const UBO: u64 = 224 + MOST_BOXES as u64 * 64 + MOST_PUFFS as u64 * 32;

/// The shadow's settings: its wisps (on, how much), its trail; its eyes' colour (linear) and whether they glow (the
/// eyes are drawn by the bodies' own shader). 1 each as made.
#[derive(Clone, Copy)]
pub struct Settings {
    pub wisps: bool,
    pub amount: f32,
    pub trail: bool,
    pub eyes: bool,
    pub eyes_colour: [f32; 3],
}

impl Default for Settings {
    fn default() -> Settings {
        Settings { wisps: true, amount: 1.0, trail: true, eyes: true, eyes_colour: [1.0; 3] }
    }
}

impl Settings {
    /// One of the menu's settings by its label (the others are not the shadow's).
    pub fn read(&mut self, k: &str, v: &str) {
        let n = v.parse::<f32>().ok();
        match k {
            "Wisps" => self.wisps = v == "1",
            "Smoke amount" => self.amount = n.unwrap_or(self.amount).clamp(0.0, 3.0),
            "Smoke trail" => self.trail = v == "1",
            "Glowing eyes" => self.eyes = v == "1",
            "Eyes colour" => self.eyes_colour = u32::from_str_radix(v, 16).map(super::ink::srgb_of).unwrap_or(self.eyes_colour),
            _ => {}
        }
    }
}

/// A puff of smoke let go: where (the world), how fast, how old, how long it lasts, how large at last.
struct Puff {
    at: [f64; 3],
    vel: [f32; 3],
    age: f32,
    life: f32,
    size: f32,
}

fn chance(seed: u32) -> f32 {
    let mut n = seed.wrapping_mul(0x9e37_79b9) ^ 0x85eb_ca6b;
    n ^= n >> 16;
    n = n.wrapping_mul(0x7feb_352d);
    n ^= n >> 15;
    n = n.wrapping_mul(0x846c_a68b);
    n ^= n >> 16;
    (n & 0xff_ffff) as f32 / 16_777_216.0
}

fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

pub struct Shadow {
    pipe: Option<Compute>,
    canvas: Option<(Box<Texture>, Box<View>)>,
    /// Each body's middle last frame (the world), to see how fast it goes; the puffs; the puffs owed each body.
    was: HashMap<i32, [f64; 3]>,
    puffs: Vec<Puff>,
    owed: HashMap<i32, f32>,
    seed: u32,
    started: Instant,
    last: Option<Instant>,
}

impl Shadow {
    pub fn new() -> Shadow {
        Shadow { pipe: None, canvas: None, was: HashMap::new(), puffs: Vec::new(), owed: HashMap::new(), seed: 1, started: Instant::now(), last: None }
    }

    pub fn set_shader(&mut self, gpu: &Gpu, spirv: &[u32]) {
        use vk::DescriptorType as T;
        let kinds = [T::STORAGE_IMAGE, T::COMBINED_IMAGE_SAMPLER, T::UNIFORM_BUFFER];
        if let Some(old) = self.pipe.replace(Compute::new(gpu, "chams_shadow", spirv, &kinds, 4)) {
            old.destroy(gpu);
        }
    }

    /// The puffs stepped: each body lets go of its own (more as it goes), they drift up, spread and fade.
    fn step(&mut self, boxes: &[LiquidBox], camera: [f64; 3], dt: f32, s: &Settings) {
        // each body's boxes and middle
        let mut bodies: HashMap<i32, (Vec<usize>, [f64; 3])> = HashMap::new();
        for (i, b) in boxes.iter().enumerate() {
            let e = bodies.entry(b[3] as i32).or_insert((Vec::new(), [0.0; 3]));
            e.0.push(i);
            for k in 0..3 {
                e.1[k] += camera[k] + b[k] as f64;
            }
        }
        let mut was = HashMap::new();
        for (id, (list, sum)) in &bodies {
            let mid = sum.map(|v| v / list.len() as f64);
            let speed = self.was.get(id).map_or(0.0, |w| {
                let (dx, dz) = (mid[0] - w[0], mid[2] - w[2]);
                ((dx * dx + dz * dz).sqrt() / dt.max(1e-3) as f64) as f32
            });
            was.insert(*id, mid);
            if !s.trail || s.amount <= 0.0 {
                continue;
            }
            let owed = self.owed.entry(*id).or_insert(0.0);
            *owed += dt * (4.0 + 7.0 * speed.min(7.0)) * s.amount;
            while *owed >= 1.0 {
                *owed -= 1.0;
                self.seed = self.seed.wrapping_add(0x9e37);
                let r = |k: u32| chance(self.seed ^ k.wrapping_mul(0x51ed_270b));
                let b = boxes[list[((r(1) * list.len() as f32) as usize).min(list.len() - 1)]];
                let (u, v, w) = (r(2) * 2.0 - 1.0, r(3) * 2.0 - 1.0, r(4) * 2.0 - 1.0);
                let at: [f64; 3] = std::array::from_fn(|k| camera[k] + (b[k] + b[4 + k] * u + b[8 + k] * v + b[12 + k] * w) as f64);
                if self.puffs.len() >= MOST_PUFFS {
                    self.puffs.remove(0);
                }
                self.puffs.push(Puff {
                    at,
                    vel: [(r(5) - 0.5) * 0.25, 0.2 + r(6) * 0.3, (r(7) - 0.5) * 0.25],
                    age: 0.0,
                    life: 0.9 + r(8) * 0.9,
                    size: 0.14 + r(9) * 0.14,
                });
            }
        }
        self.owed.retain(|id, _| bodies.contains_key(id));
        self.was = was;
        for p in &mut self.puffs {
            p.age += dt;
            for k in 0..3 {
                p.at[k] += (p.vel[k] * dt) as f64;
            }
            p.vel[0] *= 1.0 - dt * 0.8;
            p.vel[2] *= 1.0 - dt * 0.8;
        }
        self.puffs.retain(|p| p.age < p.life);
    }

    fn let_go(&mut self, enc: &mut Encoder) {
        self.puffs.clear();
        self.was.clear();
        self.owed.clear();
        if let Some((t, v)) = self.canvas.take() {
            retire(enc, t, v);
        }
    }

    /// One frame over `color` (`depth` the level's): the bodies' boxes, the look.
    #[allow(clippy::too_many_arguments)]
    pub fn frame(
        &mut self,
        gpu: &Gpu,
        enc: &mut Encoder,
        color: &Texture,
        depth: &Texture,
        boxes: &[LiquidBox],
        look: &Look,
        camera: [f64; 3],
        view_proj: [f32; 16],
        sun: [f32; 4],
        sky: [f32; 4],
        nearest: &Sampler,
    ) {
        let now_i = Instant::now();
        let dt = self.last.map_or(0.0, |l| now_i.duration_since(l).as_secs_f32()).min(0.1);
        self.last = Some(now_i);
        let s = look.shadow;
        if !look.on || look.kind != super::ink::KIND_SHADOW {
            if !self.puffs.is_empty() || self.canvas.is_some() {
                self.let_go(enc);
            }
            return;
        }
        self.step(boxes, camera, dt, &s);
        let Some(pipe) = self.pipe.as_ref() else { return };
        let (fw, fh) = (color.width, color.height);
        if fw == 0 || fh == 0 || (boxes.is_empty() && self.puffs.is_empty()) || (!s.wisps && self.puffs.is_empty()) {
            return;
        }
        let rel = |p: [f64; 3]| [(p[0] - camera[0]) as f32, (p[1] - camera[1]) as f32, (p[2] - camera[2]) as f32];
        let puffs: Vec<([f32; 4], f32)> = self
            .puffs
            .iter()
            .map(|p| {
                let f = p.age / p.life;
                let at = rel(p.at);
                ([at[0], at[1], at[2], p.size * (0.45 + 0.75 * f.sqrt())], (1.0 - f) * (1.0 - f) * smooth(p.age / 0.2) * 0.85)
            })
            .collect();
        // the nearest bodies' boxes (the smoke frays round them; they hide the puffs behind them)
        let mut near: Vec<(i32, f32)> = Vec::new();
        for b in boxes {
            let d = b[0] * b[0] + b[1] * b[1] + b[2] * b[2];
            match near.iter_mut().find(|n| n.0 == b[3] as i32) {
                Some(n) => n.1 = n.1.min(d),
                None => near.push((b[3] as i32, d)),
            }
        }
        near.sort_by(|a, b| a.1.total_cmp(&b.1));
        near.truncate(FRAYING);
        let kept: Vec<LiquidBox> = boxes.iter().filter(|b| near.iter().any(|n| n.0 == b[3] as i32)).take(MOST_BOXES).copied().collect();
        let n_boxes = kept.len();
        let Some(rect) = reach(&view_proj, &kept, &puffs, fw, fh) else { return };
        let cmd = enc.cmd(gpu);
        if self.canvas.as_ref().is_some_and(|c| c.0.width != fw || c.0.height != fh) {
            let (t, v) = self.canvas.take().unwrap();
            retire(enc, t, v);
        }
        if self.canvas.is_none() {
            self.canvas = Some(image_in(gpu, cmd, "Chams shadow over the frame", vk::Format::R8G8B8A8_UNORM, fw, fh, 1, false));
        }
        let piece = enc.transient.mapped(gpu, UBO, gpu.info.min_uniform_align.max(16) as u64, UBO, 1);
        let host = piece.host;
        let sl = (sun[0] * sun[0] + sun[1] * sun[1] + sun[2] * sun[2]).sqrt().max(1e-4);
        bytes(host, 0, &view_proj);
        bytes(host, 64, &invert(&view_proj));
        bytes(host, 128, &[(now_i.duration_since(self.started).as_secs_f64() % 3600.0) as f32, n_boxes as f32, fw as f32, fh as f32]);
        bytes(host, 144, &rect);
        bytes(host, 160, &[s.wisps as i32 as f32 * s.amount, 0.0, puffs.len() as f32, s.amount]);
        bytes(host, 176, &[sky[0], sky[1], sky[2], sun[3].max(0.0)]);
        bytes(host, 192, &[sun[0] / sl, sun[1] / sl, sun[2] / sl, 0.0]);
        // (208..224 spare)
        super::tesseract::write_body(host, 224, &kept, [0.0; 3], 0.0);
        let po = 224 + MOST_BOXES * 64;
        for (i, (p, a)) in puffs.iter().enumerate() {
            bytes(host, po + i * 16, p);
            bytes(host, po + MOST_PUFFS * 16 + i * 16, &[*a, 0.0, 0.0, 0.0]);
        }
        let ubo = Bind::Uniform(unsafe { (*piece.buffer).raw }, piece.offset, UBO);
        let depth_view = super::upscale::temp_view(gpu, enc, depth);
        let canvas = self.canvas.as_ref().unwrap();
        let set = [Bind::Storage(canvas.1.raw), Bind::Sampled(depth_view, nearest.raw), ubo];
        enc.segment(gpu, cmd, "Engine", "chams shadow (compute)");
        super::tesseract::over_rect(gpu, cmd, color, &canvas.0, rect, |cmd| {
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

/// The frame's rectangle that holds every box (and the smoke round it) and every puff; the whole frame when any is near
/// the camera or behind it.
fn reach(m: &[f32; 16], boxes: &[LiquidBox], puffs: &[([f32; 4], f32)], fw: u32, fh: u32) -> Option<[i32; 4]> {
    let (w, h) = (fw as f32, fh as f32);
    let ppb = (m[1] * m[1] + m[5] * m[5] + m[9] * m[9]).sqrt() * h * 0.5;
    let mut lo = [f32::MAX; 2];
    let mut hi = [f32::MIN; 2];
    let mut whole = false;
    let mut grow = |c: [f32; 3], r: f32| {
        let cl = |k: usize| m[k] * c[0] + m[4 + k] * c[1] + m[8 + k] * c[2] + m[12 + k];
        let d = cl(3);
        if d < r + 0.05 {
            whole = true;
            return;
        }
        let s = [(cl(0) / d * 0.5 + 0.5) * w, (cl(1) / d * 0.5 + 0.5) * h];
        let margin = r * ppb / d + 3.0;
        lo = [lo[0].min(s[0] - margin), lo[1].min(s[1] - margin)];
        hi = [hi[0].max(s[0] + margin), hi[1].max(s[1] + margin)];
    };
    for b in boxes {
        let r = [4, 8, 12].iter().map(|&k| (b[k] * b[k] + b[k + 1] * b[k + 1] + b[k + 2] * b[k + 2]).sqrt()).sum::<f32>() + 0.25;
        grow([b[0], b[1], b[2]], r);
    }
    for (p, _) in puffs {
        grow([p[0], p[1], p[2]], p[3] * 1.3);
    }
    if whole {
        return Some([0, 0, fw as i32, fh as i32]);
    }
    let r = [lo[0].floor().max(0.0) as i32, lo[1].floor().max(0.0) as i32, hi[0].ceil().min(w) as i32, hi[1].ceil().min(h) as i32];
    (r[2] > r[0] && r[3] > r[1]).then_some(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trunk(at: [f32; 3], body: f32) -> LiquidBox {
        [at[0], at[1], at[2], body, 0.25, 0.0, 0.0, 0.0, 0.0, 0.375, 0.0, 0.0, 0.0, 0.0, 0.125, 0.0]
    }

    #[test]
    fn a_body_going_trails_more_smoke_than_one_standing() {
        let s = Settings::default();
        let mut still = Shadow::new();
        let mut going = Shadow::new();
        for k in 0..120 {
            still.step(&[trunk([0.0, 1.0, 4.0], 1.0)], [0.0; 3], 1.0 / 60.0, &s);
            going.step(&[trunk([k as f32 * 0.08, 1.0, 4.0], 1.0)], [0.0; 3], 1.0 / 60.0, &s);
        }
        assert!(!still.puffs.is_empty());
        assert!(going.puffs.len() > still.puffs.len() * 2, "{} {}", going.puffs.len(), still.puffs.len());
        assert!(going.puffs.len() <= MOST_PUFFS);
        // they rise (above the top of the box they came off)
        assert!(still.puffs.iter().any(|p| p.at[1] > 1.4));
    }

    #[test]
    fn settings_from_the_menu() {
        let mut s = Settings::default();
        for (k, v) in [("Wisps", "0"), ("Smoke amount", "1.5"), ("Smoke trail", "0"), ("Glowing eyes", "0"), ("Eyes colour", "ff0000")] {
            s.read(k, v);
        }
        assert!(!s.wisps && !s.trail && !s.eyes && s.amount == 1.5 && s.eyes_colour[1] < 1e-4);
    }

    #[test]
    fn its_numbers_fit_the_shader() {
        assert!(UBO <= 16384);
    }
}
