//! The world's effects (frame command 136, shaders/engine/fx.comp): shapes of light the modules of Java put in the
//! world each frame (cursed.recode.render.Fx: Chunk Grid, Block Outline, Jump Circle, Trails, China Hat, World
//! Effects…), drawn over the level's picture against its depth in one pass. Each shape is six vectors: its own four,
//! its colour and strength, and its rectangle on the frame (made here). The shapes are sorted here into the squares of
//! BIN pixels they reach (from the drawn rectangle's corner); each 8x8 group of pixels keeps those of its square's list
//! whose rectangle reaches it.
//!
//! The shapes (fx.comp's kinds): a segment of light (a core as thick as it is, never thinner than a pixel's worth of
//! light, and its glow), a lamp, a ring laid on what is there (the level's surface: it follows the blocks, climbs a
//! wall it meets), a grid laid on it, a wall of glass with a grid on it, a cone of glass, a box's faces, a sprite turned
//! to the camera (round, a leaf, a flake, an ember), a piece of a ribbon of light, a footprint laid on the surface, a
//! particle of the weather's (weather/: a quad of a frame of its atlas, added to the picture or laid over it).

use super::gpu::{image_in, invert, retire, Compute};
use super::ink::{bytes, dispatch, Bind};
use super::tesseract::over_rect;
use crate::vk::device::Gpu;
use crate::vk::encoder::Encoder;
use crate::vk::resources::{Sampler, Texture, View};
use ash::vk;
use std::time::Instant;

pub const SEG: u32 = 0;
pub const DOT: u32 = 1;
pub const RING: u32 = 2;
pub const GRID: u32 = 3;
pub const WALL: u32 = 4;
pub const CONE: u32 = 5;
pub const BOX: u32 = 6;
pub const SPRITE: u32 = 7;
pub const RIBBON: u32 = 8;
pub const STAMP: u32 = 9;
pub const FOG: u32 = 10;
pub const BLUR: u32 = 11;
pub const TEXT: u32 = 12;
/// A particle of the weather (native/src/weather): v[0] its quad's top left corner and its frame, v[1] the way across
/// (its whole width) and the next frame, v[2] the way down and how far it is into the next frame, v[3].zw what its
/// flags say (the alpha's range, or a trail's tail: its colour packed, its alpha).
pub const PART: u32 = 13;
/// A particle's flags: added to the picture (else laid over it), faded where it meets what is behind it, its colour
/// held under white before its alpha, its texture's alpha mapped from a range, a trail fading to its tail.
pub const PART_ADD: u32 = 2;
pub const PART_FEATHER: u32 = 4;
pub const PART_SATURATE: u32 = 8;
pub const PART_REMAP: u32 = 16;
pub const PART_TRAIL: u32 = 32;
/// Shown faintly through what is in front (a wall).
pub const XRAY: u32 = 1;

/// The most shapes a frame draws (the weather's snow is some thousands).
pub const MOST: usize = 16384;
/// Each shape's numbers from Java: four vectors and its colour (20).
pub const EACH: usize = 20;
const HEAD: u64 = 384;
/// The squares the shapes are sorted into (pixels; groups of 8 fit them whole).
const BIN: i32 = 32;

/// The shapes reaching each square of `rect` (BIN pixels, rows from its corner): each square's first entry and the
/// next's (one more than there are squares), then the shapes' numbers in their order.
fn bins(rect: [i32; 4], kept: &[[f32; 4]]) -> (Vec<u32>, i32, i32) {
    let (bx, by) = ((rect[2] - rect[0] + BIN - 1) / BIN, (rect[3] - rect[1] + BIN - 1) / BIN);
    let n = (bx * by) as usize;
    let span = |r: &[f32; 4]| {
        let x0 = ((r[0].floor() as i32 - rect[0]) / BIN).clamp(0, bx - 1);
        let y0 = ((r[1].floor() as i32 - rect[1]) / BIN).clamp(0, by - 1);
        let x1 = ((r[2].ceil() as i32 - 1 - rect[0]) / BIN).clamp(0, bx - 1);
        let y1 = ((r[3].ceil() as i32 - 1 - rect[1]) / BIN).clamp(0, by - 1);
        (x0, y0, x1, y1)
    };
    let mut first = vec![0u32; n + 1];
    for r in kept {
        let (x0, y0, x1, y1) = span(r);
        for y in y0..=y1 {
            for x in x0..=x1 {
                first[(y * bx + x) as usize + 1] += 1;
            }
        }
    }
    for i in 0..n {
        first[i + 1] += first[i];
    }
    let total = first[n] as usize;
    let mut out = first.clone();
    out.resize(n + 1 + total, 0);
    let mut at = first;
    for (i, r) in kept.iter().enumerate() {
        let (x0, y0, x1, y1) = span(r);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let b = (y * bx + x) as usize;
                out[n + 1 + at[b] as usize] = i as u32;
                at[b] += 1;
            }
        }
    }
    (out, bx, by)
}

/// A shape: its four vectors (the fourth: [kind, flags, glow, its own]), its colour and strength.
#[derive(Clone, Copy, Default, Debug)]
pub struct Shape {
    pub v: [[f32; 4]; 4],
    pub colour: [f32; 4],
}

impl Shape {
    pub fn kind(&self) -> u32 {
        self.v[3][0] as u32
    }
}

/// How far ahead of the camera a point is in sight (blocks along the view).
const NEAR: f32 = 0.05;

/// Where a point from the camera falls on the frame (pixels) and its distance along the view; None behind.
fn on_screen(m: &[f32; 16], p: [f32; 3], fw: f32, fh: f32) -> Option<([f32; 2], f32)> {
    let c = |r: usize| m[r] * p[0] + m[4 + r] * p[1] + m[8 + r] * p[2] + m[12 + r];
    let w = c(3);
    if w < NEAR * 0.999 {
        return None;
    }
    Some(([(c(0) / w * 0.5 + 0.5) * fw, (c(1) / w * 0.5 + 0.5) * fh], w))
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn xyz(v: [f32; 4]) -> [f32; 3] {
    [v[0], v[1], v[2]]
}

/// The points a shape is within (from the camera), and how far its light reaches past them (blocks).
fn hull(s: &Shape) -> (Vec<[f32; 3]>, f32) {
    let v = &s.v;
    let glow = v[3][2].max(0.0);
    match s.kind() {
        SEG => (vec![xyz(v[0]), xyz(v[1])], v[0][3] * v[1][3].max(1.0) * (4.0 + 6.0 * glow)),
        DOT => (vec![xyz(v[0])], v[0][3] * (3.0 + 5.0 * glow)),
        RING => {
            // its circle round its middle, as high as the band it climbs
            let (c, r, band) = (xyz(v[0]), v[0][3] + v[1][0], v[1][1]);
            let mut p = Vec::new();
            for &dy in &[-band, band] {
                for &(dx, dz) in &[(-r, -r), (r, -r), (r, r), (-r, r)] {
                    p.push([c[0] + dx, c[1] + dy, c[2] + dz]);
                }
            }
            // (its glow, and the picture it bends round its edge)
            (p, v[1][0] * 3.0 + 0.5)
        }
        GRID => {
            let (c, r) = (xyz(v[0]), v[0][3]);
            let mut p = Vec::new();
            for &dy in &[-r * 0.5, r * 0.5] {
                for &(dx, dz) in &[(-r, -r), (r, -r), (r, r), (-r, r)] {
                    p.push([c[0] + dx, c[1] + dy, c[2] + dz]);
                }
            }
            (p, 0.5)
        }
        WALL => {
            let (o, u, w) = (xyz(v[0]), xyz(v[1]), xyz(v[2]));
            (vec![o, add(o, u), add(o, w), add(add(o, u), w)], v[1][3] * 4.0)
        }
        CONE => {
            let (a, h, ax, r) = (xyz(v[0]), v[0][3], xyz(v[1]), v[1][3]);
            let base = [a[0] + ax[0] * h, a[1] + ax[1] * h, a[2] + ax[2] * h];
            let mut p = vec![a];
            for &(dx, dz) in &[(-r, -r), (r, -r), (r, r), (-r, r)] {
                for &dy in &[-r, r] {
                    p.push([base[0] + dx, base[1] + dy, base[2] + dz]);
                }
            }
            (p, 0.1)
        }
        BOX => {
            let (lo, hi) = (xyz(v[0]), xyz(v[1]));
            ((0..8).map(|i| [if i & 1 == 1 { hi[0] } else { lo[0] }, if i & 2 == 2 { hi[1] } else { lo[1] }, if i & 4 == 4 { hi[2] } else { lo[2] }]).collect(), 0.05)
        }
        TEXT => {
            let (o, u, w) = (xyz(v[0]), xyz(v[1]), xyz(v[2]));
            (vec![o, add(o, u), add(o, w), add(add(o, u), w)], 0.1)
        }
        PART => {
            let (o, u, w) = (xyz(v[0]), xyz(v[1]), xyz(v[2]));
            (vec![o, add(o, u), add(o, w), add(add(o, u), w)], 0.0)
        }
        RIBBON => {
            let (a, b, d) = (xyz(v[0]), xyz(v[1]), xyz(v[2]));
            let at = |p: [f32; 3], w: f32, s: f32| [p[0] + d[0] * w * s, p[1] + d[1] * w * s, p[2] + d[2] * w * s];
            (vec![at(a, v[0][3], 1.0), at(a, v[0][3], -1.0), at(b, v[1][3], 1.0), at(b, v[1][3], -1.0)], 0.05)
        }
        STAMP => {
            let (c, r, band) = (xyz(v[0]), v[0][3] * 1.2, v[1][3]);
            let mut p = Vec::new();
            for &dy in &[-band, band] {
                for &(dx, dz) in &[(-r, -r), (r, -r), (r, r), (-r, r)] {
                    p.push([c[0] + dx, c[1] + dy, c[2] + dz]);
                }
            }
            (p, 0.05)
        }
        _ => (vec![xyz(v[0])], v[0][3] * 1.5),
    }
}

pub struct Fx {
    pipe: Option<Compute>,
    canvas: Option<(Box<Texture>, Box<View>)>,
    started: Instant,
    /// Last frame's camera and matrix, and when (the motion blur's: where each pixel was then).
    before: Option<([f64; 3], [f32; 16], Instant)>,
    /// The weather's atlas on the GPU (with its smaller levels) and its sampler, once it was read.
    parts: Option<crate::launch::Image>,
    parts_sampler: Option<Box<Sampler>>,
}

impl Fx {
    pub fn new() -> Fx {
        Fx { pipe: None, canvas: None, started: Instant::now(), before: None, parts: None, parts_sampler: None }
    }

    pub fn set_shader(&mut self, gpu: &Gpu, spirv: &[u32]) {
        use vk::DescriptorType as T;
        let kinds = [T::STORAGE_IMAGE, T::COMBINED_IMAGE_SAMPLER, T::COMBINED_IMAGE_SAMPLER, T::UNIFORM_BUFFER, T::STORAGE_BUFFER, T::COMBINED_IMAGE_SAMPLER,
            T::COMBINED_IMAGE_SAMPLER, T::COMBINED_IMAGE_SAMPLER, T::STORAGE_BUFFER, T::STORAGE_BUFFER];
        if let Some(old) = self.pipe.replace(Compute::new(gpu, "fx", spirv, &kinds, 4)) {
            old.destroy(gpu);
        }
    }

    /// A shape's rectangle on the frame (pixels: lowest x, y, past the highest), its light's reach round it. What of it
    /// is behind the camera is cut off where it crosses into sight (a beam passing by takes its strip of the frame, not
    /// the whole). None: it is off the frame.
    fn rect(s: &Shape, m: &[f32; 16], fw: u32, fh: u32) -> Option<[f32; 4]> {
        let (w, h) = (fw as f32, fh as f32);
        // fog fills what is seen through it, the blur all there is: the whole frame
        if s.kind() == FOG || s.kind() == BLUR {
            return Some([0.0, 0.0, w, h]);
        }
        let ppb = (m[1] * m[1] + m[5] * m[5] + m[9] * m[9]).sqrt() * h * 0.5;
        let (pts, reach) = hull(s);
        // the light's least reach in pixels (a line is never thinner than a pixel's worth of light, its glow round that);
        // a particle is only its quad
        let floor = if s.kind() == PART { 1.0 } else { 4.0 + 4.0 * s.v[3][2].max(0.0) };
        let mut lo = [f32::MAX; 2];
        let mut hi = [f32::MIN; 2];
        let mut grow = |sp: [f32; 2], d: f32| {
            let k = reach * ppb / d + floor;
            lo = [lo[0].min(sp[0] - k), lo[1].min(sp[1] - k)];
            hi = [hi[0].max(sp[0] + k), hi[1].max(sp[1] + k)];
        };
        let depth = |p: [f32; 3]| m[3] * p[0] + m[7] * p[1] + m[11] * p[2] + m[15];
        let mut any = false;
        for (i, &p) in pts.iter().enumerate() {
            if let Some((sp, d)) = on_screen(m, p, w, h) {
                grow(sp, d);
                any = true;
            }
            // where the hull's edges from it cross into sight
            for &q in &pts[i + 1..] {
                let (dp, dq) = (depth(p), depth(q));
                if (dp < NEAR) != (dq < NEAR) {
                    let t = (NEAR - dp) / (dq - dp);
                    let x = [p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t, p[2] + (q[2] - p[2]) * t];
                    if let Some((sp, d)) = on_screen(m, x, w, h) {
                        grow(sp, d);
                        any = true;
                    }
                }
            }
        }
        if !any {
            return None;
        }
        let r = [lo[0].max(0.0), lo[1].max(0.0), hi[0].min(w), hi[1].min(h)];
        (r[2] > r[0] && r[3] > r[1]).then_some(r)
    }

    /// One frame over `color` (`depth` the level's): the shapes drawn; letters from `letters`, the menu's two atlases of
    /// distance fields (gui/render.rs: its faces', the system fonts' letters; their views and sizes), when there.
    #[allow(clippy::too_many_arguments)]
    pub fn frame(
        &mut self,
        gpu: &Gpu,
        enc: &mut Encoder,
        color: &Texture,
        depth: &Texture,
        shapes: &[Shape],
        camera: [f64; 3],
        view_proj: [f32; 16],
        nearest: &Sampler,
        linear: &Sampler,
        letters: Option<[(vk::ImageView, u32, u32); 2]>,
    ) {
        let now = Instant::now();
        let before = self.before.replace((camera, view_proj, now));
        let Some(pipe) = self.pipe.as_ref() else { return };
        let (fw, fh) = (color.width, color.height);
        if fw == 0 || fh == 0 {
            return;
        }
        // the shapes on the frame, with their rectangles
        let mut kept: Vec<(&Shape, [f32; 4])> = Vec::new();
        for s in shapes.iter().take(MOST) {
            if s.colour[3] <= 0.0 {
                continue;
            }
            if let Some(r) = Self::rect(s, &view_proj, fw, fh) {
                kept.push((s, r));
            }
        }
        if kept.is_empty() {
            if let Some((t, v)) = self.canvas.take() {
                retire(enc, t, v);
            }
            return;
        }
        let mut all = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        for (_, r) in &kept {
            all = [all[0].min(r[0]), all[1].min(r[1]), all[2].max(r[2]), all[3].max(r[3])];
        }
        let rect = [all[0].floor() as i32, all[1].floor() as i32, all[2].ceil() as i32, all[3].ceil() as i32];
        if rect[2] <= rect[0] || rect[3] <= rect[1] {
            return;
        }
        let cmd = enc.cmd(gpu);
        if self.canvas.as_ref().is_some_and(|c| c.0.width != fw || c.0.height != fh) {
            let (t, v) = self.canvas.take().unwrap();
            retire(enc, t, v);
        }
        if self.canvas.is_none() {
            self.canvas = Some(image_in(gpu, cmd, "Effects over the frame", vk::Format::R8G8B8A8_UNORM, fw, fh, 1, false));
        }
        // the weather's atlas, once it is read: up with its levels, its sampler blending them
        let atlas = crate::weather::atlas();
        if let (None, Some(a)) = (self.parts.as_ref(), atlas) {
            let img = crate::launch::mipped_image(gpu, cmd, "Weather atlas", vk::Format::R8G8B8A8_UNORM, a.w, a.h, a.levels.len() as u32);
            for (level, px) in a.levels.iter().enumerate() {
                let n = px.len() as u64;
                let piece = enc.transient.staging(gpu, n, 16, n, 1);
                unsafe { std::ptr::copy_nonoverlapping(px.as_ptr(), piece.host, px.len()) };
                let region = vk::BufferImageCopy {
                    buffer_offset: piece.offset,
                    buffer_row_length: 0,
                    buffer_image_height: 0,
                    image_subresource: vk::ImageSubresourceLayers { aspect_mask: vk::ImageAspectFlags::COLOR, mip_level: level as u32, base_array_layer: 0, layer_count: 1 },
                    image_offset: vk::Offset3D::default(),
                    image_extent: vk::Extent3D { width: (a.w >> level).max(1), height: (a.h >> level).max(1), depth: 1 },
                };
                unsafe { gpu.device.cmd_copy_buffer_to_image(cmd, (*piece.buffer).raw, img.t.image, vk::ImageLayout::GENERAL, &[region]) };
            }
            crate::vk::encoder::memory_barrier(gpu, cmd);
            self.parts = Some(img);
            if let Ok(s) = Sampler::new(gpu, 1, 1, 1, 1, 1, Some(a.levels.len() as f32 - 1.0)) {
                self.parts_sampler = Some(Box::new(s));
            }
        }
        let inv = invert(&view_proj);
        // the camera's right and up (a unit each): the frame's middle row and column a little way out
        let un = |x: f32, y: f32| -> [f32; 3] {
            let p = [inv[0] * x + inv[4] * y + inv[8] * 0.5 + inv[12], inv[1] * x + inv[5] * y + inv[9] * 0.5 + inv[13], inv[2] * x + inv[6] * y + inv[10] * 0.5 + inv[14], inv[3] * x + inv[7] * y + inv[11] * 0.5 + inv[15]];
            [p[0] / p[3], p[1] / p[3], p[2] / p[3]]
        };
        let (mid, right, up) = (un(0.0, 0.0), un(0.5, 0.0), un(0.0, 0.5));
        let norm = |a: [f32; 3]| {
            let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt().max(1e-9);
            [a[0] / l, a[1] / l, a[2] / l]
        };
        let rv = norm([right[0] - mid[0], right[1] - mid[1], right[2] - mid[2]]);
        // (the frame's y down: up is the other way)
        let uv = norm([mid[0] - up[0], mid[1] - up[1], mid[2] - up[2]]);
        let fwd = norm(mid);
        let block = [camera[0].floor(), camera[1].floor(), camera[2].floor()];
        let ppb = (view_proj[1] * view_proj[1] + view_proj[5] * view_proj[5] + view_proj[9] * view_proj[9]).sqrt() * fh as f32 * 0.5;
        let head = enc.transient.mapped(gpu, HEAD, gpu.info.min_uniform_align.max(16) as u64, HEAD, 1);
        let host = head.host;
        bytes(host, 0, &view_proj);
        bytes(host, 64, &inv);
        bytes(host, 128, &[(Instant::now().duration_since(self.started).as_secs_f64() % 3600.0) as f32, kept.len() as f32, fw as f32, fh as f32]);
        bytes(host, 144, &rect);
        bytes(host, 160, &[rv[0], rv[1], rv[2], ppb]);
        bytes(host, 176, &[uv[0], uv[1], uv[2], 0.0]);
        bytes(host, 192, &[fwd[0], fwd[1], fwd[2], 0.0]);
        bytes(host, 208, &[block[0] as i32, block[1] as i32, block[2] as i32, 0]);
        bytes(host, 224, &[(camera[0] - block[0]) as f32, (camera[1] - block[1]) as f32, (camera[2] - block[2]) as f32, 0.0]);
        // the letters' atlases' sizes (none: no letters drawn)
        bytes(host, 240, &letters.map_or([0.0f32; 4], |l| [l[0].1 as f32, l[0].2 as f32, l[1].1 as f32, l[1].2 as f32]));
        // the weather's atlas: its size, its smaller levels (none: no particles drawn)
        let parts_info = match (self.parts.as_ref(), atlas) {
            (Some(_), Some(a)) if self.parts_sampler.is_some() => [a.w as f32, a.h as f32, a.levels.len() as f32 - 1.0, 1.0],
            _ => [0.0f32; 4],
        };
        bytes(host, 336, &parts_info);
        // the shapes' squares: how many across, where their lists start in the bins' buffer, their size
        let rects: Vec<[f32; 4]> = kept.iter().map(|(_, r)| *r).collect();
        let (binned, bx, by) = bins(rect, &rects);
        bytes(host, 352, &[bx, bx * by + 1, BIN, 0]);
        // the weather's own colour, when asked (w 1)
        bytes(host, 368, &crate::weather::recolour().map_or([0.0f32; 4], |c| [c[0], c[1], c[2], 1.0]));
        // last frame's matrix, the camera's move since (from now to then), the frame's time against a 60th of a second
        // (none when last frame was long ago: no blur then)
        match before.filter(|b| now.duration_since(b.2).as_secs_f32() < 0.1) {
            Some((cam0, m0, t0)) => {
                let dt = now.duration_since(t0).as_secs_f32().max(1e-4);
                bytes(host, 256, &m0);
                bytes(host, 320, &[(camera[0] - cam0[0]) as f32, (camera[1] - cam0[1]) as f32, (camera[2] - cam0[2]) as f32, (1.0 / 60.0) / dt]);
            }
            None => {
                bytes(host, 256, &view_proj);
                bytes(host, 320, &[0.0f32; 4]);
            }
        }
        let size = (kept.len() * 96) as u64;
        let body = enc.transient.mapped(gpu, size, gpu.info.min_storage_align.max(16), size, 1);
        for (i, (s, r)) in kept.iter().enumerate() {
            let o = i * 96;
            for k in 0..4 {
                bytes(body.host, o + k * 16, &s.v[k]);
            }
            bytes(body.host, o + 64, &s.colour);
            bytes(body.host, o + 80, r);
        }
        let ubo = Bind::Uniform(unsafe { (*head.buffer).raw }, head.offset, HEAD);
        let ssbo = Bind::Buffer(unsafe { (*body.buffer).raw }, body.offset, size);
        // the atlas's frames (u0, v0, u1, v1 each)
        let rects: &[[f32; 4]] = atlas.map_or(&[[0.0; 4]], |a| &a.rects);
        let fsize = (rects.len() * 16) as u64;
        let frames = enc.transient.mapped(gpu, fsize, gpu.info.min_storage_align.max(16), fsize, 1);
        for (i, r) in rects.iter().enumerate() {
            bytes(frames.host, i * 16, r);
        }
        let frames = Bind::Buffer(unsafe { (*frames.buffer).raw }, frames.offset, fsize);
        let bsize = (binned.len() * 4) as u64;
        let bin_buf = enc.transient.mapped(gpu, bsize, gpu.info.min_storage_align.max(16), bsize, 1);
        unsafe { std::ptr::copy_nonoverlapping(binned.as_ptr() as *const u8, bin_buf.host, binned.len() * 4) };
        let bin_bind = Bind::Buffer(unsafe { (*bin_buf.buffer).raw }, bin_buf.offset, bsize);
        let depth_view = super::upscale::temp_view(gpu, enc, depth);
        let color_view = super::upscale::temp_view(gpu, enc, color);
        let canvas = self.canvas.as_ref().unwrap();
        let (l0, l1) = letters.map_or((depth_view, depth_view), |l| (l[0].0, l[1].0));
        let (pv, ps) = match (self.parts.as_ref(), self.parts_sampler.as_ref()) {
            (Some(i), Some(s)) => (i.v.raw, s.raw),
            _ => (depth_view, linear.raw),
        };
        let set = [
            Bind::Storage(canvas.1.raw),
            Bind::Sampled(depth_view, nearest.raw),
            Bind::Sampled(color_view, linear.raw),
            ubo,
            ssbo,
            Bind::Sampled(l0, linear.raw),
            Bind::Sampled(l1, linear.raw),
            Bind::Sampled(pv, ps),
            frames,
            bin_bind,
        ];
        enc.segment(gpu, cmd, "Engine", "effects (compute)");
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
        if let Some(img) = self.parts.take() {
            super::ink::kill(gpu, img.t, img.v);
        }
        if let Some(s) = self.parts_sampler.take() {
            unsafe { gpu.device.destroy_sampler(s.raw, None) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proj() -> [f32; 16] {
        let f = 1.0 / (35f32.to_radians().tan());
        let (n, fa) = (0.05f32, 1000.0f32);
        let a = 16.0 / 9.0;
        [f / a, 0.0, 0.0, 0.0, 0.0, -f, 0.0, 0.0, 0.0, 0.0, n / (fa - n), -1.0, 0.0, 0.0, fa * n / (fa - n), 0.0]
    }

    fn shape(kind: u32, v0: [f32; 4], v1: [f32; 4], v2: [f32; 4]) -> Shape {
        Shape { v: [v0, v1, v2, [kind as f32, 0.0, 1.0, 0.0]], colour: [1.0, 1.0, 1.0, 1.0] }
    }

    #[test]
    fn rectangles_hold_their_shapes() {
        let m = proj();
        // a segment ahead: a small rectangle round the middle
        let s = shape(SEG, [-0.5, 0.0, -6.0, 0.02], [0.5, 0.0, -6.0, 1.0], [0.0; 4]);
        let r = Fx::rect(&s, &m, 1920, 1080).unwrap();
        assert!(r[0] < 960.0 && r[2] > 960.0 && r[1] < 540.0 && r[3] > 540.0, "{r:?}");
        assert!(r[2] - r[0] < 600.0);
        // a beam up past the side, from behind the camera to ahead: its strip of the frame, not the whole
        let s = shape(SEG, [3.0, -40.0, 2.0, 0.03], [3.0, 40.0, -8.0, 1.0], [0.0; 4]);
        let r = Fx::rect(&s, &m, 1920, 1080).unwrap();
        assert!(r[0] > 960.0 && r[2] - r[0] < 1000.0, "{r:?}");
        // wholly behind: none
        let s = shape(SEG, [0.0, 0.0, 1.0, 0.02], [0.0, 1.0, 6.0, 1.0], [0.0; 4]);
        assert_eq!(Fx::rect(&s, &m, 1920, 1080), None);
        // a wall round the camera: the whole frame
        let s = shape(WALL, [-8.0, -10.0, -3.0, 1.0], [16.0, 0.0, 0.0, 0.02], [0.0, 20.0, 0.0, 0.1]);
        assert_eq!(Fx::rect(&s, &m, 1920, 1080), Some([0.0, 0.0, 1920.0, 1080.0]));
        // off to the side, past the frame: none
        let s = shape(DOT, [500.0, 0.0, -6.0, 0.05], [0.0; 4], [0.0; 4]);
        assert_eq!(Fx::rect(&s, &m, 1920, 1080), None);
        // a ring round the feet a few blocks ahead
        let s = shape(RING, [0.0, -1.6, -5.0, 1.5], [0.1, 0.6, 0.0, 0.0], [0.0; 4]);
        let r = Fx::rect(&s, &m, 1920, 1080).unwrap();
        assert!(r[3] > 540.0, "below the middle: {r:?}");
    }

    #[test]
    fn shapes_go_into_the_squares_they_reach() {
        // a rectangle of 100x70 from (10, 20): 4x3 squares
        let rect = [10, 20, 110, 90];
        let kept = [[12.0, 22.0, 14.0, 24.0], [30.0, 40.0, 80.0, 60.0], [100.0, 80.0, 110.0, 90.0]];
        let (b, bx, by) = bins(rect, &kept);
        assert_eq!((bx, by), (4, 3));
        let n = (bx * by) as usize;
        let list = |x: i32, y: i32| -> Vec<u32> {
            let i = (y * bx + x) as usize;
            b[n + 1 + b[i] as usize..n + 1 + b[i + 1] as usize].to_vec()
        };
        assert_eq!(list(0, 0), vec![0, 1]);
        assert_eq!(list(1, 1), vec![1]);
        assert_eq!(list(2, 1), vec![1, 2]);
        assert_eq!(list(0, 2), Vec::<u32>::new());
        assert_eq!(list(3, 2), vec![2]);
        assert_eq!(b[n] as usize, b.len() - n - 1);
    }

    #[test]
    fn every_kind_has_a_hull() {
        for k in 0..=PART {
            let s = shape(k, [0.0, 0.0, -4.0, 0.5], [0.0, -1.0, 0.0, 0.5], [0.0, 1.0, 0.0, 0.5]);
            let (p, reach) = hull(&s);
            assert!(!p.is_empty() && reach.is_finite(), "kind {k}");
        }
    }

    #[test]
    fn the_eye_off_the_camera() {
        // the view's bob in the projection: a step aside and down in the view (GameRenderer.bobView), turned
        let p = proj();
        let (c, s) = (0.6f32.cos(), 0.6f32.sin());
        // a turn round y, then the bob's move in the view
        let turn = [c, 0.0, -s, 0.0, 0.0, 1.0, 0.0, 0.0, s, 0.0, c, 0.0, 0.0, 0.0, 0.0, 1.0];
        let bob = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.04, -0.09, 0.0, 1.0];
        let mul = |a: &[f32; 16], b: &[f32; 16]| -> [f32; 16] { std::array::from_fn(|i| (0..4).map(|k| a[k * 4 + i % 4] * b[(i / 4) * 4 + k]).sum()) };
        let vp = mul(&mul(&p, &bob), &turn);
        let (camera, m) = super::super::gpu::from_eye([10.0, 64.0, -3.0], &vp);
        let e = [(camera[0] - 10.0) as f32, (camera[1] - 64.0) as f32, (camera[2] + 3.0) as f32];
        assert!((e[0] * e[0] + e[1] * e[1] + e[2] * e[2]).sqrt() > 0.05, "the eye moved: {e:?}");
        let clip = |m: &[f32; 16], q: [f32; 3]| -> [f32; 4] { std::array::from_fn(|r| m[r] * q[0] + m[4 + r] * q[1] + m[8 + r] * q[2] + m[12 + r]) };
        // the eye is where the rays start: nothing ahead of it (w 0), in the middle
        let at = clip(&m, [0.0, 0.0, 0.0]);
        assert!(at[0].abs() < 1e-4 && at[1].abs() < 1e-4 && at[3].abs() < 1e-4, "{at:?}");
        // a point anywhere falls where it fell, its coordinates from the eye
        for q in [[1.0f32, 0.5, -6.0], [-3.0, 2.0, -20.0], [0.2, -1.5, -2.5]] {
            let (a, b) = (clip(&vp, q), clip(&m, [q[0] - e[0], q[1] - e[1], q[2] - e[2]]));
            for r in 0..4 {
                assert!((a[r] - b[r]).abs() < 1e-4 * a[r].abs().max(1.0), "{q:?}: {a:?} {b:?}");
            }
        }
    }
}
