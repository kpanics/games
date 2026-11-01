//! The far terrain drawn now and then into six faces around the camera instead of into every frame.
//!
//! Measured 12.10 on an RTX 4060 at 1440p, 15 chunks: what the GPU spends on terrain goes by the number of its
//! triangles (every triangle made a point still cost 220 µs of 280), and seven tenths of them lie farther than 128
//! blocks, where a block is a dozen pixels. So the opaque terrain farther than `radius` (solid and cutout layers; water
//! and glass stay live) is drawn by the game's own pipelines into the six faces of a cube around the camera — with a
//! projection and a view of the face's own instead of the game's — and each frame one triangle over the window reads
//! them back (shaders/impostor.frag): the colour, and the depth turned into this frame's. Turning the camera needs
//! nothing new (the faces hold every direction); a face is drawn again when the camera has moved from where it was
//! drawn far enough for the nearest it holds to be a pixel off (some 0.07 blocks at 80), every 100 ms (water, light, the time
//! of day) and soon after the far terrain changed — and only while it is in sight (the others when they come into it).
//! A section is live while nearer than `radius` and in the faces from 8 blocks nearer, so none falls between.
//! CURSED_DEV_NOIMPOSTOR draws all terrain live; CURSED_DEV_IMPOSTOR_R=blocks and CURSED_DEV_IMPOSTOR_RES=pixels.

use super::device::Gpu;
use super::encoder::{memory_barrier, Encoder};
use super::memory::Place;
use super::pipeline::Pipeline;
use super::resources::{Doomed, Texture, View};
use ash::vk;
use std::sync::atomic::{AtomicBool, AtomicU32};
use std::time::{Duration, Instant};

pub const FACES: usize = 6;
/// The opaque layers (solid, cutout) the faces hold.
pub const HELD: usize = 2;
/// How far the camera may move from where a face was drawn before it is drawn again, per block of distance to the
/// nearest thing a face holds: the far terrain may be off by this angle (about a pixel at 1440p and 70°).
const MOVE_ANGLE: f64 = 0.0012;
/// How old a face may get (water, light, the sky's colour on the fog).
const AGE: Duration = Duration::from_millis(100);
/// How soon after the far terrain changed (a section meshed or let go of) the faces show it.
const CHANGED: Duration = Duration::from_millis(50);
/// Faster than this (blocks a second) a face is drawn at half its size.
const FAST: f64 = 8.0;
/// Sections from this much nearer than `radius` go into the faces too.
pub const OVERLAP: f64 = 8.0;

type M4 = [f32; 16];

fn mul(a: &M4, b: &M4) -> M4 {
    let mut m = [0f32; 16];
    for c in 0..4 {
        for r in 0..4 {
            m[c * 4 + r] = (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum();
        }
    }
    m
}

fn transform(m: &M4, v: [f32; 4]) -> [f32; 4] {
    std::array::from_fn(|r| (0..4).map(|c| m[c * 4 + r] * v[c]).sum())
}

/// The direction of the view's ray through (x, y) on the window (NDC), from the inverse of its camera-relative
/// ProjMat · ModelViewMat: two of its points (a tenth and four tenths of a block out, depth reversed), not one taken
/// as a direction from the camera — walking, the game bobs the view in its projection (a few hundredths of a block
/// across), and a point that near seen from the camera points tens of degrees off (12.10: the far terrain was read
/// from the wrong part of its faces, mountains gone, while walking).
fn ray(inverse: &M4, x: f32, y: f32) -> [f32; 3] {
    let a = transform(inverse, [x, y, 0.5, 1.0]);
    let b = transform(inverse, [x, y, 0.125, 1.0]);
    let v = [b[0] / b[3] - a[0] / a[3], b[1] / b[3] - a[1] / a[3], b[2] / b[3] - a[2] / a[3]];
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1.0e-12);
    [v[0] / l, v[1] / l, v[2] / l]
}

/// The inverse (MESA's gluInvertMatrix; the layout does not matter, the inverse of the transpose is the transpose).
fn inverse(m: &M4) -> Option<M4> {
    let m: [f64; 16] = m.map(|v| v as f64);
    let mut inv = [0f64; 16];
    inv[0] = m[5] * m[10] * m[15] - m[5] * m[11] * m[14] - m[9] * m[6] * m[15] + m[9] * m[7] * m[14] + m[13] * m[6] * m[11] - m[13] * m[7] * m[10];
    inv[4] = -m[4] * m[10] * m[15] + m[4] * m[11] * m[14] + m[8] * m[6] * m[15] - m[8] * m[7] * m[14] - m[12] * m[6] * m[11] + m[12] * m[7] * m[10];
    inv[8] = m[4] * m[9] * m[15] - m[4] * m[11] * m[13] - m[8] * m[5] * m[15] + m[8] * m[7] * m[13] + m[12] * m[5] * m[11] - m[12] * m[7] * m[9];
    inv[12] = -m[4] * m[9] * m[14] + m[4] * m[10] * m[13] + m[8] * m[5] * m[14] - m[8] * m[6] * m[13] - m[12] * m[5] * m[10] + m[12] * m[6] * m[9];
    inv[1] = -m[1] * m[10] * m[15] + m[1] * m[11] * m[14] + m[9] * m[2] * m[15] - m[9] * m[3] * m[14] - m[13] * m[2] * m[11] + m[13] * m[3] * m[10];
    inv[5] = m[0] * m[10] * m[15] - m[0] * m[11] * m[14] - m[8] * m[2] * m[15] + m[8] * m[3] * m[14] + m[12] * m[2] * m[11] - m[12] * m[3] * m[10];
    inv[9] = -m[0] * m[9] * m[15] + m[0] * m[11] * m[13] + m[8] * m[1] * m[15] - m[8] * m[3] * m[13] - m[12] * m[1] * m[11] + m[12] * m[3] * m[9];
    inv[13] = m[0] * m[9] * m[14] - m[0] * m[10] * m[13] - m[8] * m[1] * m[14] + m[8] * m[2] * m[13] + m[12] * m[1] * m[10] - m[12] * m[2] * m[9];
    inv[2] = m[1] * m[6] * m[15] - m[1] * m[7] * m[14] - m[5] * m[2] * m[15] + m[5] * m[3] * m[14] + m[13] * m[2] * m[7] - m[13] * m[3] * m[6];
    inv[6] = -m[0] * m[6] * m[15] + m[0] * m[7] * m[14] + m[4] * m[2] * m[15] - m[4] * m[3] * m[14] - m[12] * m[2] * m[7] + m[12] * m[3] * m[6];
    inv[10] = m[0] * m[5] * m[15] - m[0] * m[7] * m[13] - m[4] * m[1] * m[15] + m[4] * m[3] * m[13] + m[12] * m[1] * m[7] - m[12] * m[3] * m[5];
    inv[14] = -m[0] * m[5] * m[14] + m[0] * m[6] * m[13] + m[4] * m[1] * m[14] - m[4] * m[2] * m[13] - m[12] * m[1] * m[6] + m[12] * m[2] * m[5];
    inv[3] = -m[1] * m[6] * m[11] + m[1] * m[7] * m[10] + m[5] * m[2] * m[11] - m[5] * m[3] * m[10] - m[9] * m[2] * m[7] + m[9] * m[3] * m[6];
    inv[7] = m[0] * m[6] * m[11] - m[0] * m[7] * m[10] - m[4] * m[2] * m[11] + m[4] * m[3] * m[10] + m[8] * m[2] * m[7] - m[8] * m[3] * m[6];
    inv[11] = -m[0] * m[5] * m[11] + m[0] * m[7] * m[9] + m[4] * m[1] * m[11] - m[4] * m[3] * m[9] - m[8] * m[1] * m[7] + m[8] * m[3] * m[5];
    inv[15] = m[0] * m[5] * m[10] - m[0] * m[6] * m[9] - m[4] * m[1] * m[10] + m[4] * m[2] * m[9] + m[8] * m[1] * m[6] - m[8] * m[2] * m[5];
    let det = m[0] * inv[0] + m[1] * inv[4] + m[2] * inv[8] + m[3] * inv[12];
    if det.abs() < 1e-30 {
        return None;
    }
    Some(inv.map(|v| (v / det) as f32))
}

/// A face's forward and up.
const BASES: [([f32; 3], [f32; 3]); FACES] = [
    ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
    ([-1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
    ([0.0, 1.0, 0.0], [0.0, 0.0, -1.0]),
    ([0.0, -1.0, 0.0], [0.0, 0.0, 1.0]),
    ([0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
    ([0.0, 0.0, -1.0], [0.0, 1.0, 0.0]),
];

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// The face's view: a rotation, the camera looking down -z with y up (as the game's view).
fn face_view(f: usize) -> M4 {
    let (fwd, up) = BASES[f];
    let right = cross(fwd, up);
    let back = [-fwd[0], -fwd[1], -fwd[2]];
    let mut m = [0f32; 16];
    for c in 0..3 {
        m[c * 4] = right[c];
        m[c * 4 + 1] = up[c];
        m[c * 4 + 2] = back[c];
    }
    m[15] = 1.0;
    m
}

/// The face's projection: 90° square, y up as the game's (its pictures come out with the first row at the bottom, and
/// its pipelines take clockwise triangles as facing), depth reversed and endless as the game's (near / distance),
/// `near` the nearest the faces need.
fn face_projection(near: f32, sx: f32, sy: f32) -> M4 {
    let mut m = [0f32; 16];
    m[0] = sx;
    m[5] = sy;
    m[11] = -1.0;
    m[14] = near;
    m
}

/// A layer of the game's opaque terrain as drawn this frame (pass.rs takes it at the terrain draw): its pipeline, its
/// uniforms (pass.rs Value, by slot), and what the faces replace.
pub struct Capture {
    pub pipeline: *const Pipeline,
    pub uniforms: Vec<super::pass::Value>,
    /// The slots of the game's Projection and TerrainUniform.
    pub projection: usize,
    pub terrain: usize,
    /// Where the game's TerrainUniform is (ModelViewMat, then TextureSize): the GPU's memory, copied on the GPU.
    pub terrain_source: (vk::Buffer, u64, u64),
    pub color_format: vk::Format,
}

#[derive(Clone, Copy)]
struct Face {
    origin: [f64; 3],
    at: Option<Instant>,
    /// The far terrain changed after it was drawn.
    changed: bool,
    /// Drawn into this part of its image (1, or 0.5 while the camera moves fast: `plan`).
    scale: f32,
    /// What it holds, as its projection's x and y (min x, max x, min y, max y): the sections drawn into it, boxed
    /// (`bound`). The composite covers only that (`corners`); empty (min > max) when it holds nothing.
    bounds: [f32; 4],
}

/// Bounds holding nothing.
const EMPTY: [f32; 4] = [1.0, -1.0, 1.0, -1.0];

struct Images {
    color: Box<Texture>,
    depth: Box<Texture>,
    color_layers: Vec<vk::ImageView>,
    depth_layers: Vec<vk::ImageView>,
    color_all: vk::ImageView,
    depth_all: vk::ImageView,
    format: vk::Format,
    res: u32,
}

struct Composite {
    pipeline: vk::Pipeline,
    layout: vk::PipelineLayout,
    set_layout: vk::DescriptorSetLayout,
    format: vk::Format,
}

pub struct Impostor {
    pub radius: f64,
    res: u32,
    images: Option<Images>,
    faces: [Face; FACES],
    pub captured: [Option<Capture>; HELD],
    /// This frame (Impostor::plan): the faces drawn again at its end, the terrain kept live, the camera.
    pub refresh: u32,
    pub live_radius: Option<f64>,
    camera: [f64; 3],
    /// The game draws its terrain with its own shaders (the engine's are off): the faces need the game's pipelines.
    pub vanilla: bool,
    shaders: Option<(Vec<u32>, Vec<u32>)>,
    composite: Option<Composite>,
    linear: vk::Sampler,
    nearest: vk::Sampler,
    /// The farthest section's distance last frame: nothing far enough, nothing to do.
    pub farthest: f64,
    hidden_at: Instant,
    /// The part of its image each face drawn this frame goes into.
    next_scale: [f32; FACES],
    /// The height of the target the faces were last laid over (`sharp_enough`).
    height: u32,
    /// The bounds of the faces drawn this frame, as the terrain build routes their sections to them (`bound`).
    next_bounds: [[f32; 4]; FACES],
}

/// The sign of the faces' projection on y (dev CURSED_DEV_IMPOSTOR_Y=1 or -1, to try).
fn face_y() -> f32 {
    static Y: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
    *Y.get_or_init(|| std::env::var("CURSED_DEV_IMPOSTOR_Y").ok().and_then(|v| v.parse().ok()).unwrap_or(1.0))
}

static FACES_DRAWN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn off() -> bool {
    static OFF: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *OFF.get_or_init(|| std::env::var_os("CURSED_DEV_NOIMPOSTOR").is_some())
}

impl Impostor {
    pub fn new() -> Impostor {
        let env = |k: &str| std::env::var(k).ok().and_then(|v| v.parse::<f64>().ok());
        Impostor {
            radius: env("CURSED_DEV_IMPOSTOR_R").unwrap_or(80.0),
            res: env("CURSED_DEV_IMPOSTOR_RES").map_or(2048, |v| v as u32),
            images: None,
            faces: [Face { origin: [0.0; 3], at: None, changed: false, scale: 1.0, bounds: EMPTY }; FACES],
            next_scale: [1.0; FACES],
            captured: [None, None],
            refresh: 0,
            live_radius: None,
            camera: [0.0; 3],
            vanilla: true,
            shaders: None,
            composite: None,
            linear: vk::Sampler::null(),
            nearest: vk::Sampler::null(),
            farthest: 0.0,
            hidden_at: Instant::now(),
            height: 0,
            next_bounds: [EMPTY; FACES],
        }
    }

    pub fn set_shaders(&mut self, vertex: &[u32], fragment: &[u32]) {
        self.shaders = Some((vertex.to_vec(), fragment.to_vec()));
    }

    /// The far terrain changed (a section of an opaque layer meshed or let go of).
    pub fn changed(&mut self) {
        for f in &mut self.faces {
            f.changed = true;
        }
    }

    /// Forget the faces (a new world, the terrain reset).
    pub fn reset(&mut self) {
        for f in &mut self.faces {
            f.at = None;
        }
    }

    fn active(&self) -> bool {
        !off() && self.shaders.is_some() && self.vanilla && self.farthest > self.radius + 16.0
    }

    /// The faces are as sharp as the view: at the window's centre the view's pixels per radian are not many more than
    /// the faces' (half their size: a face's centre is its coarsest). A narrower view (a zoom) draws all terrain live.
    fn sharp_enough(&self, inverse: &M4) -> bool {
        let (a, b) = (ray(inverse, 0.0, 1.0), ray(inverse, 0.0, -1.0));
        let half = ((a[0] * b[0] + a[1] * b[1] + a[2] * b[2]).clamp(-1.0, 1.0).acos() * 0.5).max(1.0e-3);
        let view = self.height as f32 * 0.5 / half.tan();
        view <= self.res as f32 * 0.5 * 1.2
    }

    /// The frame's start (the terrain build): which faces are drawn again at its end, and how near the live terrain is.
    pub fn plan(&mut self, camera: [f64; 3], view_proj: &M4) {
        self.refresh = 0;
        self.live_radius = None;
        self.camera = camera;
        if !self.active() {
            self.reset();
            return;
        }
        let now = Instant::now();
        if self.height > 0 && !inverse(view_proj).is_some_and(|inv| self.sharp_enough(&inv)) {
            return;
        }
        let seen = seen_faces(view_proj);
        let moved = |f: &Face| ((f.origin[0] - camera[0]).powi(2) + (f.origin[1] - camera[1]).powi(2) + (f.origin[2] - camera[2]).powi(2)).sqrt();
        let allowed = self.allowed_move();
        let due = |f: &Face| match f.at {
            None => true,
            Some(t) => moved(f) > allowed || now.duration_since(t) > AGE || (f.changed && now.duration_since(t) > CHANGED),
        };
        // every face in sight drawn and from near enough: the far terrain comes from the faces this frame
        let ready = (0..FACES).filter(|&f| seen & (1 << f) != 0).all(|f| self.faces[f].at.is_some() && moved(&self.faces[f]) < OVERLAP * 0.5);
        if ready {
            self.live_radius = Some(self.radius);
        }
        // one face in sight a frame at most, the longest due first: a face costs a few hundred microseconds (its two
        // thousand pixels square), and at these frame rates the others wait a fraction of a millisecond
        let stalest = (0..FACES)
            .filter(|&f| seen & (1 << f) != 0 && due(&self.faces[f]))
            .max_by(|&a, &b| {
                let key = |f: usize| self.faces[f].at.map_or(f64::MAX, |t| moved(&self.faces[f]) * 10.0 + now.duration_since(t).as_secs_f64());
                key(a).total_cmp(&key(b))
            });
        if let Some(f) = stalest {
            self.refresh |= 1 << f;
        }
        self.next_bounds = [EMPTY; FACES];
        // moving fast (flying, riding), a face is drawn at half its size, a quarter of the pixels: drawn again every
        // few milliseconds and moving across the screen, its far detail is not seen; slower, it is drawn whole again
        for f in 0..FACES {
            let fast = self.faces[f].at.is_some_and(|t| moved(&self.faces[f]) / now.duration_since(t).as_secs_f64().max(1e-4) > FAST);
            self.next_scale[f] = if fast { 0.5 } else { 1.0 };
        }
        // dev (FPS/BENCH stats): how it goes, every 5 s
        if super::stats::on() {
            static S: std::sync::Mutex<(u64, u64, u64, Option<Instant>)> = std::sync::Mutex::new((0, 0, 0, None));
            let mut st = S.lock().unwrap();
            st.0 += 1;
            st.1 += u64::from(ready);
            st.2 = FACES_DRAWN.load(std::sync::atomic::Ordering::Relaxed);
            let since = *st.3.get_or_insert(now);
            if now.duration_since(since).as_secs() >= 5 {
                eprintln!("[impostor] {} frames, faces used in {}, faces drawn {} ({:.1}/s); farthest {:.0}", st.0, st.1, st.2, st.2 as f64 / 5.0, self.farthest);
                FACES_DRAWN.store(0, std::sync::atomic::Ordering::Relaxed);
                *st = (0, 0, 0, Some(now));
            }
        }
        // a face out of sight now and then (every 50 ms, the oldest first), so turning round mostly finds them near
        // enough (from farther than half the overlap the turn draws the terrain live for a frame and draws the face)
        if now.duration_since(self.hidden_at) > Duration::from_millis(50) {
            let oldest = (0..FACES)
                .filter(|&f| seen & (1 << f) == 0 && due(&self.faces[f]))
                .min_by_key(|&f| self.faces[f].at.map_or(Duration::MAX, |t| now.duration_since(t)).as_nanos().wrapping_neg());
            if let Some(f) = oldest {
                self.refresh |= 1 << f;
                self.hidden_at = now;
            }
        }
    }

    /// A section (its corner `o`) drawn into face `f` this frame: its box, projected, widens the face's bounds.
    pub fn bound(&mut self, f: usize, o: [f64; 3]) {
        let m = mul(&face_projection(self.near(), 1.0, face_y()), &face_view(f));
        let b = &mut self.next_bounds[f];
        for k in 0..8 {
            let p = [
                (o[0] - self.camera[0] + if k & 1 != 0 { 16.0 } else { 0.0 }) as f32,
                (o[1] - self.camera[1] + if k & 2 != 0 { 16.0 } else { 0.0 }) as f32,
                (o[2] - self.camera[2] + if k & 4 != 0 { 16.0 } else { 0.0 }) as f32,
            ];
            let c = transform(&m, [p[0], p[1], p[2], 1.0]);
            if c[3] <= 1.0e-3 {
                // a corner beside or behind the face's camera: the box may reach any edge
                *b = [-1.0, 1.0, -1.0, 1.0];
                return;
            }
            let (x, y) = (c[0] / c[3], c[1] / c[3]);
            *b = [b[0].min(x), b[1].max(x), b[2].min(y), b[3].max(y)];
        }
    }

    /// Whether a section (its corner `o`) is in face `f`'s view from this frame's camera.
    pub fn in_face(&self, f: usize, o: [f64; 3]) -> bool {
        let lo = [o[0] - self.camera[0], o[1] - self.camera[1], o[2] - self.camera[2]];
        let hi = [lo[0] + 16.0, lo[1] + 16.0, lo[2] + 16.0];
        let (fwd, _) = BASES[f];
        let k = fwd.iter().position(|&v| v != 0.0).unwrap();
        let s = fwd[k] as f64;
        // the most of s·p[k] over the box, against ±p[j] for the other two axes: the four planes of the face's pyramid
        let ahead = if s > 0.0 { hi[k] } else { -lo[k] };
        if ahead <= 0.0 {
            return false;
        }
        (0..3).filter(|&j| j != k).all(|j| ahead - lo[j] >= 0.0 && ahead + hi[j] >= 0.0)
    }

    /// The faces' images, made for this colour format.
    fn ensure_images(&mut self, gpu: &Gpu, enc: &mut Encoder, format: vk::Format) -> bool {
        if self.images.as_ref().is_some_and(|i| i.format == format && i.res == self.res) {
            return true;
        }
        if let Some(old) = self.images.take() {
            Self::retire(enc, old);
        }
        let d = &gpu.device;
        let make = |format: vk::Format, usage: vk::ImageUsageFlags, aspect: vk::ImageAspectFlags, name: &str| -> Option<(Box<Texture>, Vec<vk::ImageView>, vk::ImageView)> {
            let info = vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .format(format)
                .extent(vk::Extent3D { width: self.res, height: self.res, depth: 1 })
                .mip_levels(1)
                .array_layers(FACES as u32)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(usage | vk::ImageUsageFlags::SAMPLED)
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .initial_layout(vk::ImageLayout::UNDEFINED);
            let image = unsafe { d.create_image(&info, None) }.ok()?;
            let req = unsafe { d.get_image_memory_requirements(image) };
            let alloc = match gpu.memory.allocate(d, req, Place::Device, false) {
                Ok(a) => a,
                Err(_) => {
                    unsafe { d.destroy_image(image, None) };
                    return None;
                }
            };
            unsafe { d.bind_image_memory(image, alloc.memory, alloc.offset) }.ok()?;
            gpu.name(image, name);
            let view = |base: u32, count: u32, kind: vk::ImageViewType| {
                let info = vk::ImageViewCreateInfo::default()
                    .image(image)
                    .view_type(kind)
                    .format(format)
                    .subresource_range(vk::ImageSubresourceRange { aspect_mask: aspect, base_mip_level: 0, level_count: 1, base_array_layer: base, layer_count: count });
                unsafe { d.create_image_view(&info, None) }.expect("impostor view")
            };
            let layers = (0..FACES as u32).map(|l| view(l, 1, vk::ImageViewType::TYPE_2D)).collect();
            let all = view(0, FACES as u32, vk::ImageViewType::TYPE_2D_ARRAY);
            let texture = Box::new(Texture {
                image,
                alloc: Some(alloc),
                format,
                aspect,
                width: self.res,
                height: self.res,
                layers: FACES as u32,
                mips: 1,
                cube: false,
                closed: AtomicBool::new(true),
                views: AtomicU32::new(0),
            });
            Some((texture, layers, all))
        };
        let Some((color, color_layers, color_all)) = make(format, vk::ImageUsageFlags::COLOR_ATTACHMENT, vk::ImageAspectFlags::COLOR, "Cursed far terrain (colour)") else {
            return false;
        };
        let Some((depth, depth_layers, depth_all)) =
            make(vk::Format::D32_SFLOAT, vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT, vk::ImageAspectFlags::DEPTH, "Cursed far terrain (depth)")
        else {
            return false;
        };
        let cmd = enc.cmd(gpu);
        let barrier = |t: &Texture| {
            vk::ImageMemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                .dst_access_mask(vk::AccessFlags2::MEMORY_READ | vk::AccessFlags2::MEMORY_WRITE)
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::GENERAL)
                .image(t.image)
                .subresource_range(vk::ImageSubresourceRange { aspect_mask: t.aspect, base_mip_level: 0, level_count: 1, base_array_layer: 0, layer_count: FACES as u32 })
        };
        let barriers = [barrier(&color), barrier(&depth)];
        unsafe { gpu.sync2_d.cmd_pipeline_barrier2(cmd, &vk::DependencyInfo::default().image_memory_barriers(&barriers)) };
        if self.linear == vk::Sampler::null() {
            let s = |filter: vk::Filter| {
                let info = vk::SamplerCreateInfo::default()
                    .mag_filter(filter)
                    .min_filter(filter)
                    .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
                    .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                    .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                    .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                    .max_lod(0.0);
                unsafe { d.create_sampler(&info, None) }.expect("impostor sampler")
            };
            self.linear = s(vk::Filter::LINEAR);
            self.nearest = s(vk::Filter::NEAREST);
        }
        for f in &mut self.faces {
            f.at = None;
        }
        self.images = Some(Images { color, depth, color_layers, depth_layers, color_all, depth_all, format, res: self.res });
        true
    }

    fn retire(enc: &mut Encoder, i: Images) {
        for v in i.color_layers.into_iter().chain(i.depth_layers).chain([i.color_all, i.depth_all]) {
            enc.defer(Doomed::View(Box::new(View { raw: v, texture: std::ptr::null(), base_mip: 0 })));
        }
        enc.defer(Doomed::Texture(i.color));
        enc.defer(Doomed::Texture(i.depth));
    }

    /// How far the camera may move before a face is drawn again (MOVE_ANGLE at the nearest the faces hold).
    fn allowed_move(&self) -> f64 {
        MOVE_ANGLE * (self.radius - OVERLAP - 14.0).max(8.0)
    }

    /// The near plane of the faces: nothing they hold is nearer along a face's axis.
    fn near(&self) -> f32 {
        (((self.radius - OVERLAP - 16.0) * 0.5).max(1.0)) as f32
    }

    /// Ready to draw the faces this frame: the captures of both layers and the images. Returns the faces to draw.
    pub fn prepare(&mut self, gpu: &Gpu, enc: &mut Encoder) -> u32 {
        if self.refresh == 0 || self.captured.iter().any(Option::is_none) {
            return 0;
        }
        let format = self.captured[0].as_ref().unwrap().color_format;
        if !self.ensure_images(gpu, enc, format) {
            eprintln!("[impostor] the faces' images could not be made ({format:?})");
            return 0;
        }
        self.refresh
    }

    /// Begins drawing face `f` (cleared), outside any pass.
    pub fn begin_face(&self, gpu: &Gpu, cmd: vk::CommandBuffer, f: usize) {
        let i = self.images.as_ref().unwrap();
        let color = [vk::RenderingAttachmentInfo::default()
            .image_view(i.color_layers[f])
            .image_layout(vk::ImageLayout::GENERAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue { color: vk::ClearColorValue { float32: [0.0; 4] } })];
        let depth = vk::RenderingAttachmentInfo::default()
            .image_view(i.depth_layers[f])
            .image_layout(vk::ImageLayout::GENERAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue { depth_stencil: vk::ClearDepthStencilValue { depth: 0.0, stencil: 0 } });
        let size = ((i.res as f32 * self.next_scale[f]) as u32).max(1);
        let area = vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent: vk::Extent2D { width: size, height: size } };
        let info = vk::RenderingInfo::default().render_area(area).layer_count(1).color_attachments(&color).depth_attachment(&depth);
        unsafe {
            gpu.render_d.cmd_begin_rendering(cmd, &info);
            gpu.device.cmd_set_viewport(cmd, 0, &[vk::Viewport { x: 0.0, y: 0.0, width: size as f32, height: size as f32, min_depth: 0.0, max_depth: 1.0 }]);
            gpu.device.cmd_set_scissor(cmd, 0, &[area]);
        }
    }

    pub fn end_face(&mut self, gpu: &Gpu, cmd: vk::CommandBuffer, f: usize) {
        FACES_DRAWN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        unsafe { gpu.render_d.cmd_end_rendering(cmd) };
        self.faces[f] = Face { origin: self.camera, at: Some(Instant::now()), changed: false, scale: self.next_scale[f], bounds: self.next_bounds[f] };
    }

    /// The uniforms of the captured layers for each face drawn this frame: the face's projection and view in place of
    /// the game's (the rest of the game's TerrainUniform, the atlas's size, copied on the GPU). Recorded outside any
    /// pass, before the faces.
    pub fn face_uniforms(&self, gpu: &Gpu, enc: &mut Encoder, cmd: vk::CommandBuffer, faces: u32) -> Vec<[Vec<super::pass::Value>; HELD]> {
        let proj = face_projection(self.near(), 1.0, face_y());
        let mut out = Vec::with_capacity(FACES);
        for f in 0..FACES {
            if faces & (1 << f) == 0 {
                out.push([Vec::new(), Vec::new()]);
                continue;
            }
            let view = face_view(f);
            let per = std::array::from_fn(|l| {
                let c = self.captured[l].as_ref().unwrap();
                let mut values = c.uniforms.clone();
                let p = enc.transient.mapped(gpu, 64, 256, 64, 1);
                unsafe { std::ptr::copy_nonoverlapping(proj.as_ptr() as *const u8, p.host, 64) };
                values[c.projection] = super::pass::Value::Buffer { buffer: p.buffer, offset: p.offset, length: 64 };
                let (src, src_offset, n) = c.terrain_source;
                let t = enc.transient.mapped(gpu, n, 256, n, 1);
                unsafe {
                    std::ptr::copy_nonoverlapping(view.as_ptr() as *const u8, t.host, 64);
                    let rest = vk::BufferCopy { src_offset: src_offset + 64, dst_offset: t.offset + 64, size: n - 64 };
                    if rest.size > 0 {
                        gpu.device.cmd_copy_buffer(cmd, src, (*t.buffer).raw, &[rest]);
                    }
                }
                values[c.terrain] = super::pass::Value::Buffer { buffer: t.buffer, offset: t.offset, length: n };
                values
            });
            out.push(per);
        }
        memory_barrier(gpu, cmd);
        out
    }

    /// The end of the faces' drawing this frame.
    pub fn finish(&mut self, gpu: &Gpu, cmd: vk::CommandBuffer) {
        memory_barrier(gpu, cmd);
        self.refresh = 0;
    }

    /// Lays the faces over the frame, in the open pass after its opaque terrain. `color_format`: the pass's one colour
    /// attachment; `size` the pass's; `view_proj` this frame's camera-relative ProjMat · ModelViewMat. Returns whether
    /// anything was drawn (the caller binds its own pipeline again).
    pub fn composite(&mut self, gpu: &Gpu, enc: &mut Encoder, cmd: vk::CommandBuffer, color_format: vk::Format, size: (u32, u32), view_proj: &M4) -> bool {
        self.height = size.1;
        if self.live_radius.is_none() || self.images.as_ref().is_none_or(|i| i.format != color_format) {
            return false;
        }
        let Some(inverse) = inverse(view_proj) else { return false };
        if self.composite.as_ref().is_none_or(|c| c.format != color_format) {
            if let Some(old) = self.composite.take() {
                Self::destroy_composite(gpu, old);
            }
            match self.make_composite(gpu, color_format) {
                Some(c) => self.composite = Some(c),
                None => return false,
            }
        }
        let comp = self.composite.as_ref().unwrap();
        let images = self.images.as_ref().unwrap();
        // the nearest the faces hold (from 8 blocks inside the live radius, a section's half diagonal nearer still, the
        // camera a tenth of a block off), as depth along this frame's view at the window's corners (cos 60°)
        let nearest = ((self.radius - OVERLAP - 14.0 - 1.0).max(1.0) * 0.5) as f32;
        let quad_depth = {
            // a point straight ahead at that distance: its depth in this frame
            let dir = ray(&inverse, 0.0, 0.0);
            let p = transform(view_proj, [dir[0] * nearest, dir[1] * nearest, dir[2] * nearest, 1.0]);
            (p[2] / p[3]).clamp(0.0, 1.0)
        };
        // dev: CURSED_DEV_IMPTINT=1 the faces' pixels tinted by face (X red, Y green, Z blue), 2 also the quads' empty
        // pixels magenta, 3..8 face n-3's image over the window (over everything)
        static TINT: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
        let tint = *TINT.get_or_init(|| std::env::var("CURSED_DEV_IMPTINT").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0));
        let quad_depth = if tint > 2.5 { 1.0 } else { quad_depth };
        let mut data = Vec::with_capacity(640);
        let push = |data: &mut Vec<u8>, m: &M4| m.iter().for_each(|v| data.extend_from_slice(&v.to_le_bytes()));
        push(&mut data, &inverse);
        push(&mut data, view_proj);
        let proj = face_projection(self.near(), 1.0, face_y());
        for f in 0..FACES {
            push(&mut data, &mul(&proj, &face_view(f)));
        }
        for f in &self.faces {
            for k in 0..3 {
                data.extend_from_slice(&((f.origin[k] - self.camera[k]) as f32).to_le_bytes());
            }
            data.extend_from_slice(&f.scale.to_le_bytes());
        }
        for v in [1.0 / size.0 as f32, 1.0 / size.1 as f32, images.res as f32, 0.0, self.near(), 1.002, quad_depth, tint] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        // Each face's bounds as a quad on a plane 500 blocks out along its axis from this frame's camera: the fragment
        // shader reads a face by the direction of a pixel's ray from this camera alone, so the pixels to shade are the
        // directions inside the bounds, wherever the face's own camera was (up to half the overlap off: from there
        // the quads leave a strip between faces). A hundredth of the face wider all round (the filter's texels around
        // an edge) and past the face's own edges into its neighbours' (two quads meeting edge to edge leave pixels
        // between them; a pixel is shaded only by its own face's quad, the fragment shader leaves the others): only
        // the pixels whose rays meet what a face holds are shaded — most of the sky not at all (the whole window cost
        // some 30 µs at 1440p, 12.10).
        let mut quads = 0;
        for (f, face) in self.faces.iter().enumerate() {
            let b = if tint > 2.5 && face.at.is_some() { [-1.0, 1.0, -1.0, 1.0] } else { face.bounds };
            let held = face.at.is_some() && b[0] <= b[1] && b[2] <= b[3];
            let inv = self::inverse(&mul(&proj, &face_view(f)));
            let z = self.near() / 500.0;
            let (x0, x1, y0, y1) = ((b[0] - 0.01).max(-1.05), (b[1] + 0.01).min(1.05), (b[2] - 0.01).max(-1.05), (b[3] + 0.01).min(1.05));
            for (x, y) in [(x0, y0), (x1, y0), (x1, y1), (x0, y1)] {
                let corner = match (held, &inv) {
                    (true, Some(inv)) => {
                        let p = transform(inv, [x, y, z, 1.0]);
                        [p[0] / p[3], p[1] / p[3], p[2] / p[3], 1.0]
                    }
                    _ => [0.0; 4],
                };
                corner.iter().for_each(|v| data.extend_from_slice(&v.to_le_bytes()));
            }
            quads += usize::from(held);
        }
        if quads == 0 {
            return false;
        }
        let ubo = enc.transient.mapped(gpu, data.len() as u64, 256, data.len() as u64, 1);
        unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), ubo.host, data.len()) };
        let buffer = [vk::DescriptorBufferInfo { buffer: unsafe { (*ubo.buffer).raw }, offset: ubo.offset, range: data.len() as u64 }];
        let color = [vk::DescriptorImageInfo { sampler: self.linear, image_view: images.color_all, image_layout: vk::ImageLayout::GENERAL }];
        let depth = [vk::DescriptorImageInfo { sampler: self.nearest, image_view: images.depth_all, image_layout: vk::ImageLayout::GENERAL }];
        let writes = [
            vk::WriteDescriptorSet::default().dst_binding(0).descriptor_type(vk::DescriptorType::UNIFORM_BUFFER).buffer_info(&buffer),
            vk::WriteDescriptorSet::default().dst_binding(1).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(&color),
            vk::WriteDescriptorSet::default().dst_binding(2).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(&depth),
        ];
        let area = vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent: vk::Extent2D { width: size.0, height: size.1 } };
        unsafe {
            gpu.device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, comp.pipeline);
            gpu.push_d.cmd_push_descriptor_set(cmd, vk::PipelineBindPoint::GRAPHICS, comp.layout, 0, &writes);
            gpu.device.cmd_set_viewport(cmd, 0, &[vk::Viewport { x: 0.0, y: 0.0, width: size.0 as f32, height: size.1 as f32, min_depth: 0.0, max_depth: 1.0 }]);
            gpu.device.cmd_set_scissor(cmd, 0, &[area]);
            gpu.device.cmd_draw(cmd, 36, 1, 0, 0);
        }
        true
    }

    fn make_composite(&self, gpu: &Gpu, format: vk::Format) -> Option<Composite> {
        let (vs, fs) = self.shaders.as_ref()?;
        let d = &gpu.device;
        let module = |code: &[u32]| unsafe { d.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(code), None) }.ok();
        let (vm, fm) = (module(vs)?, module(fs)?);
        let bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default().binding(1).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).descriptor_count(1).stage_flags(vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default().binding(2).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).descriptor_count(1).stage_flags(vk::ShaderStageFlags::FRAGMENT),
        ];
        let set_layout = unsafe {
            d.create_descriptor_set_layout(&vk::DescriptorSetLayoutCreateInfo::default().flags(vk::DescriptorSetLayoutCreateFlags::PUSH_DESCRIPTOR_KHR).bindings(&bindings), None)
        }
        .ok()?;
        let set_layouts = [set_layout];
        let layout = unsafe { d.create_pipeline_layout(&vk::PipelineLayoutCreateInfo::default().set_layouts(&set_layouts), None) }.ok()?;
        let entry = c"main";
        let stages = [
            vk::PipelineShaderStageCreateInfo::default().stage(vk::ShaderStageFlags::VERTEX).module(vm).name(entry),
            vk::PipelineShaderStageCreateInfo::default().stage(vk::ShaderStageFlags::FRAGMENT).module(fm).name(entry),
        ];
        let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();
        let assembly = vk::PipelineInputAssemblyStateCreateInfo::default().topology(vk::PrimitiveTopology::TRIANGLE_LIST);
        let raster = vk::PipelineRasterizationStateCreateInfo::default().polygon_mode(vk::PolygonMode::FILL).cull_mode(vk::CullModeFlags::NONE).line_width(1.0);
        let depth = vk::PipelineDepthStencilStateCreateInfo::default().depth_test_enable(true).depth_write_enable(true).depth_compare_op(vk::CompareOp::GREATER_OR_EQUAL);
        let blends = [vk::PipelineColorBlendAttachmentState::default()
            .color_write_mask(vk::ColorComponentFlags::RGBA)
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::ONE)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .alpha_blend_op(vk::BlendOp::ADD)];
        let blend = vk::PipelineColorBlendStateCreateInfo::default().attachments(&blends);
        let viewport = vk::PipelineViewportStateCreateInfo::default().viewport_count(1).scissor_count(1);
        let multisample = vk::PipelineMultisampleStateCreateInfo::default().rasterization_samples(vk::SampleCountFlags::TYPE_1);
        let dynamic_states = [vk::DynamicState::SCISSOR, vk::DynamicState::VIEWPORT];
        let dynamic = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);
        let formats = [format];
        let mut rendering = vk::PipelineRenderingCreateInfo::default().color_attachment_formats(&formats).depth_attachment_format(vk::Format::D32_SFLOAT);
        let info = [vk::GraphicsPipelineCreateInfo::default()
            .stages(&stages)
            .vertex_input_state(&vertex_input)
            .input_assembly_state(&assembly)
            .rasterization_state(&raster)
            .depth_stencil_state(&depth)
            .color_blend_state(&blend)
            .viewport_state(&viewport)
            .multisample_state(&multisample)
            .dynamic_state(&dynamic)
            .layout(layout)
            .push_next(&mut rendering)];
        let pipeline = unsafe { d.create_graphics_pipelines(vk::PipelineCache::null(), &info, None) }.map(|v| v[0]).ok();
        unsafe {
            d.destroy_shader_module(vm, None);
            d.destroy_shader_module(fm, None);
        }
        let pipeline = pipeline?;
        gpu.name(pipeline, "Pipeline cursed:impostor");
        Some(Composite { pipeline, layout, set_layout, format })
    }

    fn destroy_composite(gpu: &Gpu, c: Composite) {
        unsafe {
            gpu.device.destroy_pipeline(c.pipeline, None);
            gpu.device.destroy_pipeline_layout(c.layout, None);
            gpu.device.destroy_descriptor_set_layout(c.set_layout, None);
        }
    }

    pub fn destroy(&mut self, gpu: &Gpu) {
        if let Some(c) = self.composite.take() {
            Self::destroy_composite(gpu, c);
        }
        if let Some(i) = self.images.take() {
            unsafe {
                for v in i.color_layers.iter().chain(&i.depth_layers).chain([&i.color_all, &i.depth_all]) {
                    gpu.device.destroy_image_view(*v, None);
                }
            }
            Doomed::Texture(i.color).destroy(gpu);
            Doomed::Texture(i.depth).destroy(gpu);
        }
        unsafe {
            if self.linear != vk::Sampler::null() {
                gpu.device.destroy_sampler(self.linear, None);
                gpu.device.destroy_sampler(self.nearest, None);
            }
        }
    }
}

/// The faces this frame's view looks into: the directions of its corners, edges' middles and centre.
fn seen_faces(view_proj: &M4) -> u32 {
    let Some(inv) = inverse(view_proj) else { return (1 << FACES) - 1 };
    let mut seen = 0;
    for y in [-1.0f32, 0.0, 1.0] {
        for x in [-1.0f32, 0.0, 1.0] {
            let d = ray(&inv, x, y);
            let a = d.map(f32::abs);
            let f = if a[0] >= a[1] && a[0] >= a[2] {
                if d[0] > 0.0 { 0 } else { 1 }
            } else if a[1] >= a[2] {
                if d[1] > 0.0 { 2 } else { 3 }
            } else if d[2] > 0.0 {
                4
            } else {
                5
            };
            seen |= 1 << f;
        }
    }
    seen
}
