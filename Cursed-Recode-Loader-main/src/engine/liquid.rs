//! Chams' polymorphic liquid (the menu's Look "Polymorph", shaders/engine/liquid.comp): a body in it is its model's
//! boxes (cursed.recode.chams.ChamsLiquid gives each cube where its pose is this frame, and the light on it) and the
//! drops round it, drawn as one liquid over the level's picture — their distances one smooth union, so what comes near
//! joins as a liquid does. The drops live on the GPU: they come out of the body's surface, swirl round it on a shell,
//! tremble, go back and sink in; some fall, splash on the ground and lie there or crawl back to the body. Their places
//! are kept from an anchor (a block near the first body, moved when it goes far) so they stay where they are in the
//! world as the camera moves. What the liquid is the player makes (the menu's Liquid: water, mercury, oil or their
//! own): its colours, how clear, how much it bends and mirrors, how much of a metal, how rough, its film and bubbles,
//! how thick and heavy.

use super::gpu::{fail, image_in, invert, retire, Compute};
use super::ink::{bytes, dispatch, srgb_of, Bind, Look};
use crate::gui::data::{LIQUIDS, LIQUID_COLOURS, LIQUID_SLIDERS};
use crate::vk::device::Gpu;
use crate::vk::encoder::{memory_barrier, Encoder};
use crate::vk::memory::Place;
use crate::vk::resources::{Buffer, Doomed, Sampler, Texture, View};
use ash::vk;
use std::time::Instant;

/// The liquid's settings: its colours (its own, what its depth takes the light to, its highlights', its rim's:
/// linear), how clear (0..1), how much it bends what is behind it and mirrors what is round it (1 as made), how much
/// of a metal, how rough, its rainbow film (0..1), its bubbles (1 as made), how thick, how heavy; the drops (on, how
/// many, how large, how far they reach to flow together, how far they fly out, how fast they come back, how restless:
/// 1 each as made), their splashes, their crawling back.
#[derive(Clone, Copy)]
pub struct Settings {
    pub colour: [f32; 3],
    pub deep: [f32; 3],
    pub highlight: [f32; 3],
    pub rim: [f32; 3],
    pub clear: f32,
    pub refraction: f32,
    pub reflection: f32,
    pub metal: f32,
    pub rough: f32,
    pub film: f32,
    pub bubbles: f32,
    pub thick: f32,
    pub merging: f32,
    pub heavy: f32,
    pub drops: bool,
    pub amount: f32,
    pub size: f32,
    pub scatter: f32,
    pub back: f32,
    pub restless: f32,
    pub splashes: bool,
    pub crawl: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        let mut s = Settings {
            colour: [0.0; 3],
            deep: [0.0; 3],
            highlight: [0.0; 3],
            rim: [0.0; 3],
            clear: 0.0,
            refraction: 0.0,
            reflection: 0.0,
            metal: 0.0,
            rough: 0.0,
            film: 0.0,
            bubbles: 0.0,
            thick: 1.0,
            merging: 1.0,
            heavy: 1.0,
            drops: true,
            amount: 1.0,
            size: 1.0,
            scatter: 1.0,
            back: 1.0,
            restless: 1.0,
            splashes: true,
            crawl: true,
        };
        // water, as the menu makes it
        let (colours, sliders) = LIQUIDS[0];
        for (k, l) in LIQUID_COLOURS.iter().enumerate() {
            s.read(l, &format!("{:06x}", colours[k]));
        }
        for (k, l) in LIQUID_SLIDERS.iter().enumerate() {
            s.read(l, &sliders[k].to_string());
        }
        s
    }
}

impl Settings {
    /// One of the menu's settings by its label (the others are not the liquid's).
    pub fn read(&mut self, k: &str, v: &str) {
        let n = v.parse::<f32>().ok();
        let colour = |was: [f32; 3]| u32::from_str_radix(v, 16).map(srgb_of).unwrap_or(was);
        let pct = |was: f32| n.map_or(was, |x| (x / 100.0).clamp(0.0, 1.0));
        match k {
            "Liquid colour" => self.colour = colour(self.colour),
            "Depth colour" => self.deep = colour(self.deep),
            "Highlight colour" => self.highlight = colour(self.highlight),
            "Rim colour" => self.rim = colour(self.rim),
            "Transparency" => self.clear = pct(self.clear),
            "Refraction" => self.refraction = n.unwrap_or(self.refraction),
            "Reflection" => self.reflection = n.unwrap_or(self.reflection),
            "Metalness" => self.metal = pct(self.metal),
            "Roughness" => self.rough = pct(self.rough),
            "Iridescence" => self.film = pct(self.film),
            "Bubbles" => self.bubbles = n.unwrap_or(self.bubbles),
            "Viscosity" => self.thick = n.unwrap_or(self.thick),
            "Merging" => self.merging = n.unwrap_or(self.merging),
            "Weight" => self.heavy = n.unwrap_or(self.heavy),
            "Drops" => self.drops = v == "1",
            "Drops amount" => self.amount = n.unwrap_or(self.amount),
            "Drop size" => self.size = n.unwrap_or(self.size),
            "Scatter" => self.scatter = n.unwrap_or(self.scatter),
            "Return speed" => self.back = n.unwrap_or(self.back),
            "Restlessness" => self.restless = n.unwrap_or(self.restless),
            "Splashes" => self.splashes = v == "1",
            "Crawl back" => self.crawl = v == "1",
            _ => {}
        }
    }

    /// For the creatures' shaders (chams.glsl ch_liquid: the hands, what a body wears and holds): its colour and how
    /// clear; how much it mirrors, how far its drops join, how restless, how much of a metal; its depth's colour and how
    /// much it bends; its highlights' colour and its film; its rim's colour and how rough.
    pub fn ubo(&self) -> [f32; 24] {
        let (c, d, h, r) = (self.colour, self.deep, self.highlight, self.rim);
        [
            c[0], c[1], c[2], self.clear, self.reflection, self.merging, self.restless, self.metal, d[0], d[1], d[2], self.refraction, h[0], h[1], h[2], self.film, r[0], r[1], r[2],
            self.rough, 0.0, 0.0, 0.0, 0.0,
        ]
    }
}

/// The most boxes, the drops (the bodies', then the splashes' droplets: liquid.comp MAIN_DROPS, SPLASH_DROPS), the drops
/// alive as made (the menu's amount 1), a tile's side and how many it lists (TILE, PER_TILE).
pub const MOST_BOXES: usize = 128;
const MAIN_DROPS: u32 = 1280;
const MOST_DROPS: u32 = MAIN_DROPS + 256;
const DROPS_AS_MADE: f32 = 420.0;
/// What each quality keeps of the drops.
const KEEPS: [f32; 4] = [0.45, 0.65, 0.85, 1.0];
const TILE: u32 = 16;
const PER_TILE: u32 = 256;
const UBO: u64 = 416 + 128 * 64;

/// A box: its middle from the camera (w its body's number), its three half-sides as vectors (w of the first the block
/// light on its body, of the second the sky's; of the third with_areas' share).
pub type LiquidBox = [f32; 16];

/// A player's body as boxes (blocks³: head, trunk, four limbs): the drops as made are a player's.
const PLAYER_VOLUME: f32 = 0.41;

/// The boxes with each one's share of their surface so far in the w of its third half-side (0..1, the last 1: what the
/// drops and the ash come out of is chosen by it, each place as likely as it is large — an arrow's planes get their few,
/// a trunk its many), and how much body they are (blocks³, every half-side at least liquid.comp's THINNEST: an
/// arrow's flat planes are next to nothing of it).
pub fn with_areas(boxes: &[LiquidBox]) -> (Vec<LiquidBox>, f32) {
    let cross = |u: [f32; 3], v: [f32; 3]| {
        let c = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
        (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt()
    };
    let areas: Vec<f32> = boxes
        .iter()
        .map(|b| {
            let (x, y, z) = ([b[4], b[5], b[6]], [b[8], b[9], b[10]], [b[12], b[13], b[14]]);
            8.0 * (cross(x, y) + cross(y, z) + cross(z, x))
        })
        .collect();
    let total: f32 = areas.iter().sum();
    let len = |x: f32, y: f32, z: f32| (x * x + y * y + z * z).sqrt().max(0.012);
    let volume: f32 = boxes.iter().map(|b| 8.0 * len(b[4], b[5], b[6]) * len(b[8], b[9], b[10]) * len(b[12], b[13], b[14])).sum();
    let mut so_far = 0.0;
    let out = boxes
        .iter()
        .zip(&areas)
        .map(|(b, a)| {
            so_far += a;
            let mut b = *b;
            b[15] = if total > 1e-9 { (so_far / total).min(1.0) } else { 1.0 };
            b
        })
        .collect();
    (out, volume)
}

pub struct Liquid {
    pipe: Option<Compute>,
    drops: Option<Box<Buffer>>,
    counters: Option<Box<Buffer>>,
    /// The tiles' counts and lists, and the frame size they are for.
    tiles: Option<(Box<Buffer>, u32, u32)>,
    canvas: Option<(Box<Texture>, Box<View>)>,
    anchor: Option<[i64; 3]>,
    started: Instant,
    last: Option<Instant>,
    /// Seconds without a box (the drops shrink away, then nothing runs).
    idle: f32,
}

impl Liquid {
    pub fn new() -> Liquid {
        Liquid { pipe: None, drops: None, counters: None, tiles: None, canvas: None, anchor: None, started: Instant::now(), last: None, idle: 1e9 }
    }

    pub fn set_shader(&mut self, gpu: &Gpu, spirv: &[u32]) {
        use vk::DescriptorType as T;
        let kinds = [
            T::STORAGE_BUFFER,
            T::STORAGE_BUFFER,
            T::STORAGE_BUFFER,
            T::STORAGE_IMAGE,
            T::COMBINED_IMAGE_SAMPLER,
            T::COMBINED_IMAGE_SAMPLER,
            T::COMBINED_IMAGE_SAMPLER,
            T::UNIFORM_BUFFER,
        ];
        if let Some(old) = self.pipe.replace(Compute::new(gpu, "chams_liquid", spirv, &kinds, 4)) {
            old.destroy(gpu);
        }
    }

    fn let_go(&mut self, enc: &mut Encoder) {
        for b in self.drops.take().into_iter().chain(self.counters.take()).chain(self.tiles.take().map(|t| t.0)) {
            enc.defer(Doomed::Buffer(b));
        }
        if let Some((t, v)) = self.canvas.take() {
            retire(enc, t, v);
        }
        self.anchor = None;
        self.idle = 1e9;
    }

    /// One frame: the drops stepped, the liquid over `color` (`depth` the level's); the voxel window's blocks what the
    /// drops land on.
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
        sun_color: [f32; 4],
        sky: [f32; 4],
        fog: [f32; 4],
        voxels: Option<(&View, &Sampler)>,
        window: [i32; 5],
        linear: &Sampler,
        nearest: &Sampler,
    ) {
        let now = Instant::now();
        let dt = self.last.map_or(0.0, |l| now.duration_since(l).as_secs_f32()).min(0.1);
        self.last = Some(now);
        let (Some(pipe), Some((vox, vox_sampler))) = (self.pipe.as_ref(), voxels) else { return };
        if !look.on || look.kind != super::ink::KIND_LIQUID {
            if self.drops.is_some() {
                self.let_go(enc);
            }
            return;
        }
        self.idle = if boxes.is_empty() { self.idle + dt } else { 0.0 };
        // (the drops lying on the ground dry away within that)
        if self.idle > 6.0 || color.width == 0 || color.height == 0 {
            return;
        }
        let cmd = enc.cmd(gpu);
        let usage = vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST;
        if self.drops.is_none() {
            let d = Box::new(fail(Buffer::with(gpu, usage, MOST_DROPS as u64 * 48, Place::Device), "Can't make the liquid's drops"));
            let c = Box::new(fail(Buffer::with(gpu, usage, 16, Place::Device), "Can't make the liquid's counters"));
            unsafe {
                gpu.device.cmd_fill_buffer(cmd, d.raw, 0, vk::WHOLE_SIZE, 0);
                gpu.device.cmd_fill_buffer(cmd, c.raw, 0, vk::WHOLE_SIZE, 0);
            }
            memory_barrier(gpu, cmd);
            self.drops = Some(d);
            self.counters = Some(c);
        }
        let (fw, fh) = (color.width, color.height);
        let (tx, ty) = (fw.div_ceil(TILE), fh.div_ceil(TILE));
        if self.tiles.as_ref().is_none_or(|t| t.1 != fw || t.2 != fh) {
            if let Some(t) = self.tiles.take() {
                enc.defer(Doomed::Buffer(t.0));
            }
            let b = Box::new(fail(Buffer::with(gpu, usage, (tx * ty * (PER_TILE + 1)) as u64 * 4, Place::Device), "Can't make the liquid's tiles"));
            self.tiles = Some((b, fw, fh));
            if let Some((t, v)) = self.canvas.take() {
                retire(enc, t, v);
            }
        }
        if self.canvas.is_none() {
            self.canvas = Some(image_in(gpu, cmd, "Chams liquid over the frame", vk::Format::R8G8B8A8_UNORM, fw, fh, 1, false));
        }
        // the anchor: a block near the first box, moved when it goes more than 32 blocks off (the drops moved back by
        // as much: they stay where they are)
        let first = boxes.first().map(|b| [camera[0] + b[0] as f64, camera[1] + b[1] as f64, camera[2] + b[2] as f64]).unwrap_or(camera);
        let want = [first[0].floor() as i64, first[1].floor() as i64, first[2].floor() as i64];
        let mut shift = [0f32; 3];
        match self.anchor {
            Some(a) if (0..3).all(|k| (a[k] - want[k]).abs() <= 32) => {}
            Some(a) => {
                shift = std::array::from_fn(|k| (want[k] - a[k]) as f32);
                self.anchor = Some(want);
            }
            None => self.anchor = Some(want),
        }
        let a = self.anchor.unwrap();
        let cam = [(camera[0] - a[0] as f64) as f32, (camera[1] - a[1] as f64) as f32, (camera[2] - a[2] as f64) as f32];
        let n_boxes = boxes.len().min(MOST_BOXES);
        let (boxes, volume) = with_areas(&boxes[..n_boxes]);
        let s = &look.liquid;
        let quality = look.quality.min(3);
        let piece = enc.transient.mapped(gpu, UBO, gpu.info.min_uniform_align.max(16) as u64, UBO, 1);
        let host = piece.host;
        bytes(host, 0, &view_proj);
        bytes(host, 64, &invert(&view_proj));
        bytes(host, 128, &[cam[0], cam[1], cam[2], dt]);
        bytes(host, 144, &[(now.duration_since(self.started).as_secs_f64() % 3600.0) as f32, n_boxes as f32, fw as f32, fh as f32]);
        // as many drops as there is body: a player's as made, an arrow's a few, a crowd's more (all of them at most)
        let live = (DROPS_AS_MADE * s.amount * KEEPS[quality] * (volume / PLAYER_VOLUME).min(1.6)).clamp(0.0, MAIN_DROPS as f32);
        bytes(host, 160, &[live, s.size, s.merging, s.restless]);
        bytes(host, 176, &[s.colour[0], s.colour[1], s.colour[2], s.clear]);
        bytes(host, 192, &sky);
        bytes(host, 208, &[fog[0], fog[1], fog[2], sun[3].clamp(0.0, 1.0)]);
        let l = (sun[0] * sun[0] + sun[1] * sun[1] + sun[2] * sun[2]).sqrt().max(1e-4);
        bytes(host, 224, &[sun[0] / l, sun[1] / l, sun[2] / l, sun[3].max(0.0)]);
        bytes(host, 240, &[s.reflection, s.drops as i32 as f32, quality as f32, s.rough]);
        bytes(host, 256, &[shift[0], shift[1], shift[2], s.metal]);
        bytes(host, 272, &[s.deep[0], s.deep[1], s.deep[2], s.refraction]);
        bytes(host, 288, &[s.highlight[0], s.highlight[1], s.highlight[2], s.film]);
        bytes(host, 304, &[s.rim[0], s.rim[1], s.rim[2], s.bubbles]);
        bytes(host, 320, &[s.thick, s.heavy, s.scatter, s.back]);
        bytes(host, 336, &[s.splashes as i32 as f32, s.crawl as i32 as f32, 1.0, 0.0]);
        bytes(host, 352, &[a[0] as i32, a[1] as i32, a[2] as i32, 0]);
        bytes(host, 368, &[window[0], window[1], window[2], window[3]]);
        bytes(host, 384, &[window[4], 0, 0, 0]);
        bytes(host, 400, &[sun_color[0], sun_color[1], sun_color[2], 0.0]);
        for (i, b) in boxes.iter().take(n_boxes).enumerate() {
            bytes(host, 416 + i * 64, b);
        }
        let ubo = Bind::Uniform(unsafe { (*piece.buffer).raw }, piece.offset, UBO);
        let depth_view = super::upscale::temp_view(gpu, enc, depth);
        let color_view = super::upscale::temp_view(gpu, enc, color);
        let (drops, counters, tiles, canvas) = (self.drops.as_ref().unwrap(), self.counters.as_ref().unwrap(), self.tiles.as_ref().unwrap(), self.canvas.as_ref().unwrap());
        let set = [
            Bind::Buffer(drops.raw, 0, vk::WHOLE_SIZE),
            Bind::Buffer(tiles.0.raw, 0, vk::WHOLE_SIZE),
            Bind::Buffer(counters.raw, 0, vk::WHOLE_SIZE),
            Bind::Storage(canvas.1.raw),
            Bind::Sampled(depth_view, nearest.raw),
            Bind::Sampled(color_view, linear.raw),
            Bind::Sampled(vox.raw, vox_sampler.raw),
            ubo,
        ];
        enc.segment(gpu, cmd, "Engine", "chams liquid (compute)");
        // the splashes' droplets, then the drops (which let new droplets go)
        dispatch(gpu, cmd, pipe, &set, &0i32.to_ne_bytes(), [MOST_DROPS.div_ceil(64), 1, 1]);
        dispatch(gpu, cmd, pipe, &set, &3i32.to_ne_bytes(), [MAIN_DROPS.div_ceil(64), 1, 1]);
        // the tiles' counts from nothing, then the boxes and drops into their lists
        unsafe { gpu.device.cmd_fill_buffer(cmd, tiles.0.raw, 0, (tx * ty) as u64 * 4, 0) };
        memory_barrier(gpu, cmd);
        dispatch(gpu, cmd, pipe, &set, &1i32.to_ne_bytes(), [(MOST_BOXES as u32 + MOST_DROPS).div_ceil(64), 1, 1]);
        // over a copy of the frame (the frame itself read behind the liquid), copied back
        let d = &gpu.device;
        let range = vk::ImageSubresourceLayers { aspect_mask: vk::ImageAspectFlags::COLOR, mip_level: 0, base_array_layer: 0, layer_count: 1 };
        let extent = vk::Extent3D { width: fw, height: fh, depth: 1 };
        let corner = vk::Offset3D { x: fw as i32, y: fh as i32, z: 1 };
        let same = color.format == vk::Format::R8G8B8A8_UNORM;
        let carry = |src: vk::Image, dst: vk::Image| unsafe {
            if same {
                let copy = vk::ImageCopy { src_subresource: range, src_offset: vk::Offset3D::default(), dst_subresource: range, dst_offset: vk::Offset3D::default(), extent };
                d.cmd_copy_image(cmd, src, vk::ImageLayout::GENERAL, dst, vk::ImageLayout::GENERAL, &[copy]);
            } else {
                let blit = vk::ImageBlit { src_subresource: range, src_offsets: [vk::Offset3D::default(), corner], dst_subresource: range, dst_offsets: [vk::Offset3D::default(), corner] };
                d.cmd_blit_image(cmd, src, vk::ImageLayout::GENERAL, dst, vk::ImageLayout::GENERAL, &[blit], vk::Filter::NEAREST);
            }
        };
        carry(color.image, canvas.0.image);
        memory_barrier(gpu, cmd);
        dispatch(gpu, cmd, pipe, &set, &2i32.to_ne_bytes(), [fw.div_ceil(8), fh.div_ceil(8), 1]);
        carry(canvas.0.image, color.image);
        memory_barrier(gpu, cmd);
        enc.segment_end(gpu, cmd);
    }

    pub fn destroy(&mut self, gpu: &Gpu) {
        if let Some(p) = self.pipe.take() {
            p.destroy(gpu);
        }
        for b in self.drops.take().into_iter().chain(self.counters.take()).chain(self.tiles.take().map(|t| t.0)) {
            super::ink::kill_buffer_now(gpu, b);
        }
        if let Some((t, v)) = self.canvas.take() {
            super::ink::kill(gpu, t, v);
        }
    }
}
