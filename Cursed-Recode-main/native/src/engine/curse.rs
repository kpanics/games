//! Chams' curse in the air (the menu's Look "Cursed", shaders/engine/curse.comp): its ash, flakes smouldering at their
//! edges as they flutter down off a cursed body, lying cold on the ground and melting away, the burnt through crumbling
//! into dust. A body is its model's boxes (cursed.recode.chams.ChamsLiquid: each cube where its pose is this frame, and
//! the light on it); the flakes and specks live on the GPU, their places kept from an anchor (a block near the first
//! body, moved when it goes far) so they stay where they are in the world as the camera moves; drawn over the level's
//! picture, each pixel's own: thin plates, specks, their edges smoothed.

use super::gpu::{fail, image_in, invert, retire, Compute};
use super::ink::{bytes, dispatch, srgb_of, Bind, Body, Look};
use super::liquid::LiquidBox;
use crate::vk::device::Gpu;
use crate::vk::encoder::{memory_barrier, Encoder};
use crate::vk::memory::Place;
use crate::vk::resources::{Buffer, Doomed, Sampler, Texture, View};
use ash::vk;
use std::time::Instant;

/// The curse's settings in the air and on the body: the veins' colour and how strongly they beat, the dead ground's
/// colour; the ash (on, its colour and its smouldering's, how much, how large its flakes, how much they smoulder, how
/// long they lie, crumbling into dust). 1 each as made.
#[derive(Clone, Copy)]
pub struct Settings {
    pub veins_colour: [f32; 3],
    pub beat: f32,
    pub dead_ground: [f32; 3],
    pub ash: bool,
    pub ash_colour: [f32; 3],
    pub smoulder_colour: [f32; 3],
    pub ash_amount: f32,
    pub flake_size: f32,
    pub smoulder: f32,
    pub lies: f32,
    pub dust: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            veins_colour: srgb_of(0x0d0c0e),
            beat: 1.0,
            dead_ground: srgb_of(0x8a8b8f),
            ash: true,
            ash_colour: srgb_of(0x48474b),
            smoulder_colour: srgb_of(0xd8d8e0),
            ash_amount: 1.0,
            flake_size: 1.0,
            smoulder: 1.0,
            lies: 8.0,
            dust: true,
        }
    }
}

impl Settings {
    /// One of the menu's settings by its label (the others are not the curse's).
    pub fn read(&mut self, k: &str, v: &str) {
        let n = v.parse::<f32>().ok();
        let colour = |was: [f32; 3]| u32::from_str_radix(v, 16).map(srgb_of).unwrap_or(was);
        match k {
            "Veins colour" => self.veins_colour = colour(self.veins_colour),
            "Veins beat" => self.beat = n.unwrap_or(self.beat),
            "Dead ground" => self.dead_ground = colour(self.dead_ground),
            "Ash" => self.ash = v == "1",
            "Ash colour" => self.ash_colour = colour(self.ash_colour),
            "Smoulder colour" => self.smoulder_colour = colour(self.smoulder_colour),
            "Ash amount" => self.ash_amount = n.unwrap_or(self.ash_amount),
            "Flake size" => self.flake_size = n.unwrap_or(self.flake_size),
            "Smoulder" => self.smoulder = n.unwrap_or(self.smoulder),
            "Ash lies" => self.lies = n.unwrap_or(self.lies).max(0.5),
            "Dust" => self.dust = v == "1",
            _ => {}
        }
    }
}

/// The most flakes, specks of dust (curse.comp MOST_FLAKES, MOST_DUST); everything drawn (its PRIMS); a tile's side
/// and how many it lists (TILE, PER_TILE: one that would hold more takes all of them).
const MOST_FLAKES: u32 = 1536;
const MOST_DUST: u32 = 2048;
const PRIMS: u32 = MOST_FLAKES + MOST_DUST;
const TILE: u32 = 16;
const PER_TILE: u32 = 384;
const UBO: u64 = 352 + 128 * 64;
/// What each quality keeps of the ash.
const KEEPS: [f32; 4] = [0.45, 0.65, 0.85, 1.0];
/// Flakes a second off a body standing, as made; a moving one lets go of more.
const FLAKES_A_SECOND: f32 = 24.0;

/// The boxes of the bodies with effects: one's own alone (`only_me`: its boxes' body below 0), or the bodies nearest the
/// camera, `most` of them (one's own first).
fn chosen(boxes: &[LiquidBox], only_me: bool, most: usize) -> Vec<LiquidBox> {
    let mut near: Vec<(i32, f32)> = Vec::new();
    for b in boxes {
        let who = b[3] as i32;
        let d = b[0] * b[0] + b[1] * b[1] + b[2] * b[2];
        match near.iter_mut().find(|n| n.0 == who) {
            Some(n) => n.1 = n.1.min(d),
            None => near.push((who, d)),
        }
    }
    near.sort_by(|a, b| (a.0 >= 0).cmp(&(b.0 >= 0)).then(a.1.total_cmp(&b.1)));
    let keep: Vec<i32> = near.iter().filter(|n| !only_me || n.0 < 0).take(if only_me { 1 } else { most.max(1) }).map(|n| n.0).collect();
    boxes.iter().filter(|b| keep.contains(&(b[3] as i32))).copied().collect()
}

pub struct Curse {
    pipe: Option<Compute>,
    /// The flakes, the specks, the counter of what the GPU let go.
    bufs: Option<[Box<Buffer>; 3]>,
    /// The tiles' counts and lists, and the frame size they are for.
    tiles: Option<(Box<Buffer>, u32, u32)>,
    canvas: Option<(Box<Texture>, Box<View>)>,
    anchor: Option<[i64; 3]>,
    started: Instant,
    last: Option<Instant>,
    /// The body's flakes owed (a part of one carried to the next frame), where the next goes in their ring.
    owed: f32,
    ring: u32,
    /// Seconds without a box (what is in the air goes on falling, then nothing runs).
    idle: f32,
}

impl Curse {
    pub fn new() -> Curse {
        Curse { pipe: None, bufs: None, tiles: None, canvas: None, anchor: None, started: Instant::now(), last: None, owed: 0.0, ring: 0, idle: 1e9 }
    }

    pub fn set_shader(&mut self, gpu: &Gpu, spirv: &[u32]) {
        use vk::DescriptorType as T;
        let kinds = [
            T::STORAGE_BUFFER,
            T::STORAGE_BUFFER,
            T::STORAGE_BUFFER,
            T::STORAGE_BUFFER,
            T::STORAGE_IMAGE,
            T::COMBINED_IMAGE_SAMPLER,
            T::COMBINED_IMAGE_SAMPLER,
            T::UNIFORM_BUFFER,
        ];
        if let Some(old) = self.pipe.replace(Compute::new(gpu, "chams_curse", spirv, &kinds, 4)) {
            old.destroy(gpu);
        }
    }

    fn let_go(&mut self, enc: &mut Encoder) {
        for b in self.bufs.take().into_iter().flatten().chain(self.tiles.take().map(|t| t.0)) {
            enc.defer(Doomed::Buffer(b));
        }
        if let Some((t, v)) = self.canvas.take() {
            retire(enc, t, v);
        }
        self.anchor = None;
        self.idle = 1e9;
    }

    /// One frame: the flakes and specks stepped, drawn over `color` (`depth` the level's); the voxel window's
    /// blocks what the ash lands on.
    #[allow(clippy::too_many_arguments)]
    pub fn frame(
        &mut self,
        gpu: &Gpu,
        enc: &mut Encoder,
        color: &Texture,
        depth: &Texture,
        boxes: &[LiquidBox],
        bodies: &[Body],
        look: &Look,
        camera: [f64; 3],
        view_proj: [f32; 16],
        sun: [f32; 4],
        sun_color: [f32; 4],
        sky: [f32; 4],
        fog: [f32; 4],
        voxels: Option<(&View, &Sampler)>,
        window: [i32; 5],
        nearest: &Sampler,
    ) {
        let now = Instant::now();
        let dt = self.last.map_or(0.0, |l| now.duration_since(l).as_secs_f32()).min(0.1);
        self.last = Some(now);
        let s = &look.curse;
        let (Some(pipe), Some((vox, vox_sampler))) = (self.pipe.as_ref(), voxels) else { return };
        if !look.on || look.kind != super::ink::KIND_CURSE || !s.ash {
            if self.bufs.is_some() {
                self.let_go(enc);
            }
            return;
        }
        self.idle = if boxes.is_empty() { self.idle + dt } else { 0.0 };
        // what lies on the ground goes on until it melts
        if self.idle > s.lies.max(9.0) + 3.0 || color.width == 0 || color.height == 0 {
            return;
        }
        let cmd = enc.cmd(gpu);
        let usage = vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST;
        if self.bufs.is_none() {
            let sizes = [MOST_FLAKES as u64 * 80, MOST_DUST as u64 * 48, 16];
            let bufs = sizes.map(|n| Box::new(fail(Buffer::with(gpu, usage, n, Place::Device), "Can't make the curse's buffers")));
            for b in &bufs {
                unsafe { gpu.device.cmd_fill_buffer(cmd, b.raw, 0, vk::WHOLE_SIZE, 0) };
            }
            memory_barrier(gpu, cmd);
            self.bufs = Some(bufs);
        }
        let (fw, fh) = (color.width, color.height);
        let (tx, ty) = (fw.div_ceil(TILE), fh.div_ceil(TILE));
        if self.tiles.as_ref().is_none_or(|t| t.1 != fw || t.2 != fh) {
            if let Some(t) = self.tiles.take() {
                enc.defer(Doomed::Buffer(t.0));
            }
            let b = Box::new(fail(Buffer::with(gpu, usage, (tx * ty * (PER_TILE + 1)) as u64 * 4, Place::Device), "Can't make the curse's tiles"));
            self.tiles = Some((b, fw, fh));
            if let Some((t, v)) = self.canvas.take() {
                retire(enc, t, v);
            }
        }
        if self.canvas.is_none() {
            self.canvas = Some(image_in(gpu, cmd, "Chams curse over the frame", vk::Format::R8G8B8A8_UNORM, fw, fh, 1, false));
        }
        // the anchor: a block near the first box, moved when it goes more than 32 blocks off (what is in the air moved
        // back by as much: it stays where it is)
        let first = boxes.iter().find(|b| b[3] < 0.0).or(boxes.first()).map(|b| [camera[0] + b[0] as f64, camera[1] + b[1] as f64, camera[2] + b[2] as f64]).unwrap_or(camera);
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
        // the bodies with effects: one's own alone (its boxes' body below 0), or the nearest as many as the player lets
        let boxes = chosen(boxes, look.only_me, look.most_bodies);
        // each box's share of their surface (the ash comes off each place as likely as it is large)
        let (boxes, _) = super::liquid::with_areas(&boxes);
        let boxes = boxes.as_slice();
        let n_boxes = boxes.len().min(super::liquid::MOST_BOXES);
        let n_bodies = boxes.iter().take(n_boxes).map(|b| b[3] as i32).collect::<std::collections::BTreeSet<_>>().len().max(1);
        let keep = KEEPS[look.quality.min(3)];
        // the body's flakes this frame: more as the bodies move (their speed, at most a sprint's)
        let mut flakes = 0u32;
        if s.ash && n_boxes > 0 {
            let moving: f32 = bodies.iter().map(|b| (b.vel[0] * b.vel[0] + b.vel[1] * b.vel[1] + b.vel[2] * b.vel[2]).sqrt().min(8.0)).sum::<f32>() / bodies.len().max(1) as f32;
            self.owed += dt * FLAKES_A_SECOND * s.ash_amount * keep * n_bodies as f32 * (1.0 + 0.45 * moving);
            flakes = (self.owed.floor() as u32).min(MOST_FLAKES / 4);
            self.owed -= self.owed.floor();
        }
        let start = self.ring;
        self.ring = (self.ring + flakes) % MOST_FLAKES;
        let piece = enc.transient.mapped(gpu, UBO, gpu.info.min_uniform_align.max(16) as u64, UBO, 1);
        let host = piece.host;
        bytes(host, 0, &view_proj);
        bytes(host, 64, &invert(&view_proj));
        bytes(host, 128, &[cam[0], cam[1], cam[2], dt]);
        bytes(host, 144, &[(now.duration_since(self.started).as_secs_f64() % 3600.0) as f32, n_boxes as f32, fw as f32, fh as f32]);
        bytes(host, 160, &[shift[0], shift[1], shift[2], look.quality as f32]);
        bytes(host, 176, &[a[0] as i32, a[1] as i32, a[2] as i32, 0]);
        bytes(host, 192, &[window[0], window[1], window[2], window[3]]);
        bytes(host, 208, &[window[4], 1, 0, 0]);
        let l = (sun[0] * sun[0] + sun[1] * sun[1] + sun[2] * sun[2]).sqrt().max(1e-4);
        bytes(host, 224, &[sun[0] / l, sun[1] / l, sun[2] / l, sun[3].max(0.0)]);
        bytes(host, 240, &[sun_color[0], sun_color[1], sun_color[2], sun[3].clamp(0.0, 1.0)]);
        bytes(host, 256, &sky);
        bytes(host, 272, &[fog[0], fog[1], fog[2], 0.0]);
        bytes(host, 288, &[start as f32, flakes as f32, s.flake_size, s.smoulder]);
        bytes(host, 304, &[s.ash as i32 as f32, (s.dust && look.quality > 0) as i32 as f32, s.lies, s.ash_amount]);
        bytes(host, 320, &[s.ash_colour[0], s.ash_colour[1], s.ash_colour[2], 0.0]);
        bytes(host, 336, &[s.smoulder_colour[0], s.smoulder_colour[1], s.smoulder_colour[2], 0.0]);
        for (i, b) in boxes.iter().take(n_boxes).enumerate() {
            bytes(host, 352 + i * 64, b);
        }
        let ubo = Bind::Uniform(unsafe { (*piece.buffer).raw }, piece.offset, UBO);
        let depth_view = super::upscale::temp_view(gpu, enc, depth);
        let (bufs, tiles, canvas) = (self.bufs.as_ref().unwrap(), self.tiles.as_ref().unwrap(), self.canvas.as_ref().unwrap());
        let set = [
            Bind::Buffer(bufs[0].raw, 0, vk::WHOLE_SIZE),
            Bind::Buffer(bufs[1].raw, 0, vk::WHOLE_SIZE),
            Bind::Buffer(bufs[2].raw, 0, vk::WHOLE_SIZE),
            Bind::Buffer(tiles.0.raw, 0, vk::WHOLE_SIZE),
            Bind::Storage(canvas.1.raw),
            Bind::Sampled(depth_view, nearest.raw),
            Bind::Sampled(vox.raw, vox_sampler.raw),
            ubo,
        ];
        enc.segment(gpu, cmd, "Engine", "chams curse (compute)");
        dispatch(gpu, cmd, pipe, &set, &0i32.to_ne_bytes(), [MOST_FLAKES.div_ceil(64), 1, 1]);
        dispatch(gpu, cmd, pipe, &set, &1i32.to_ne_bytes(), [MOST_DUST.div_ceil(64), 1, 1]);
        // the tiles' counts from nothing, then everything into their lists
        unsafe { gpu.device.cmd_fill_buffer(cmd, tiles.0.raw, 0, (tx * ty) as u64 * 4, 0) };
        memory_barrier(gpu, cmd);
        dispatch(gpu, cmd, pipe, &set, &2i32.to_ne_bytes(), [PRIMS.div_ceil(64), 1, 1]);
        // over a copy of the frame, copied back
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
        dispatch(gpu, cmd, pipe, &set, &3i32.to_ne_bytes(), [fw.div_ceil(8), fh.div_ceil(8), 1]);
        carry(canvas.0.image, color.image);
        memory_barrier(gpu, cmd);
        enc.segment_end(gpu, cmd);
    }

    pub fn destroy(&mut self, gpu: &Gpu) {
        if let Some(p) = self.pipe.take() {
            p.destroy(gpu);
        }
        for b in self.bufs.take().into_iter().flatten().chain(self.tiles.take().map(|t| t.0)) {
            super::ink::kill_buffer_now(gpu, b);
        }
        if let Some((t, v)) = self.canvas.take() {
            super::ink::kill(gpu, t, v);
        }
    }
}
