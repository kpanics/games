//! The chams' ink in the air (cursed.recode.chams.ChamsInk): ink let into still water, simulated, seen and laid over the
//! level's picture — the look "Ink" as real as it gets, whatever it costs (her word, 19.10).
//!
//! A grid of 128 × 64 × 128 cells (sixteen to a block: 8 × 4 × 8 blocks) round the nearest body that lets ink go,
//! addressed modulo its size as it follows (shaders/engine/ink_sim.comp): the water's velocity and the ink stepped about
//! 60 times a second, each step as long as the time it covers (no frame shows two steps' worth, the next none), the bodies
//! and their hands moved along their way from the frame before across a frame's steps — carried along, the bodies
//! stirring it, its swirls kept, made incompressible (Jacobi), the ink carried
//! forward and back (MacCormack) on a grid of its own twice as fine (256 × 128 × 256, its sheets finer than the swirls
//! that carry them, and swirls finer than the water's of their own), the sun's and the sky's reach through it; the
//! engine's voxel window its walls. Tracer specks carried
//! by its flow draw its finest threads into a buffer the size of the frame (ink_part.comp). A ray through the grid for
//! each pixel of the frame, passing bricks with no ink at once, its edges frayed by a grain carried on the flow
//! (ink_draw.comp; between steps it looks back along the flow as far as the ink has gone since); laid over the frame with the
//! threads (ink_comp.comp), on a copy of the frame (the game's target is not a storage image), copied back.

use super::gpu::{fail, image3d, image_in, invert, retire, Compute};
use crate::vk::device::Gpu;
use crate::vk::encoder::{memory_barrier, Encoder};
use crate::vk::memory::Place;
use crate::vk::resources::{Buffer, Doomed, Sampler, Texture, View};
use ash::vk;
use std::time::Instant;

/// The most bodies that let ink go at once (the shaders' Bodies[8]).
pub const MOST_BODIES: usize = 8;
/// The grid (cells): x, y, z.
const SIZE: [u32; 3] = [128, 64, 128];
/// The ink's own grid (twice as fine), and its bricks (eight of its cells a side).
const FINE: [u32; 3] = [256, 128, 256];
const BRICKS: [u32; 3] = [32, 16, 32];
/// Its sub-bricks (four of its cells a side: the bricks are made from them).
const SUBS: [u32; 3] = [64, 32, 64];
/// The noise the finer swirls and the grain read (a side; ink_sim.comp stage 9).
const NOISE: u32 = 64;
const CELLS: f64 = 16.0;
/// A step of the water (s; about: each is as long as the time it covers), and the most steps a frame catches up.
const STEP: f32 = 1.0 / 60.0;
const MOST_STEPS: u32 = 3;
/// Each body's floats in the shaders' Bodies (seven vec4s: feet, velocity, hands, and where the feet and hands were).
const EACH: usize = 28;
/// The specks' ring (a power of two), each 64 bytes; each body's sources (ink_part.comp SOURCES).
const SPECKS: u32 = 1 << 18;
const SOURCES: u32 = 56;
/// A round of specks is let go so many times a second (EMITS, by quality), a slow frame letting go as many rounds as fit
/// in it, at most this many: faster frames do not crowd the ring, slower ones do not draw their threads in long straight
/// pieces.
const MOST_ROUNDS: u32 = 4;
/// The uniform blocks' sizes (bytes): ink_sim's Ink, ink_part's Part, ink_draw's Draw.
const SIM_UBO: u64 = 96 + 4 * EACH as u64 * MOST_BODIES as u64;
const PART_UBO: u64 = 176 + 4 * EACH as u64 * MOST_BODIES as u64;
const DRAW_UBO: u64 = 240;
/// How long the water goes on after the last body let ink go (s): its ink fades out, then nothing is done.
const LINGER: f32 = 40.0;

/// The look's settings the engine reads (the menu's module "chams", gui::call "set", asked four times a second):
/// whether the module is on, which look (KIND_INK, KIND_LIQUID, KIND_CURSE); the ink's: whether there is smoke and
/// threads, the dye (its linear colour thin over white), how much ink a body lets go, how long the smoke lasts (s), how
/// dark the threads, the body's shine and how fast its pattern flows; the liquid's (liquid.rs); the curse's: the
/// flesh's colour (linear), its veins (on, how fast they crawl), how dark; its ash (curse.rs); the traces on
/// the ground (on, how long they last, how large); the quality (0 low .. 3 ultra: as made).
#[derive(Clone, Copy)]
pub struct Look {
    pub on: bool,
    pub kind: usize,
    pub smoke: bool,
    pub threads: bool,
    pub dye: [f32; 3],
    pub density: f32,
    pub lasts: f32,
    pub threads_strength: f32,
    pub shine: f32,
    pub flow: f32,
    pub liquid: super::liquid::Settings,
    pub flesh: [f32; 3],
    pub veins: bool,
    pub veins_speed: f32,
    pub darkness: f32,
    pub curse: super::curse::Settings,
    /// the shadow's (shadow.rs)
    pub shadow: super::shadow::Settings,
    pub traces: bool,
    pub traces_last: f32,
    pub traces_size: f32,
    pub quality: usize,
    /// the effects from one's own body alone, from how many bodies at most
    pub only_me: bool,
    pub most_bodies: usize,
}

/// The looks (the menu's "Look", in its order).
pub const KIND_INK: usize = 0;
pub const KIND_LIQUID: usize = 1;
pub const KIND_CURSE: usize = 2;
pub const KIND_SHADOW: usize = 3;

/// The dye the look was made with (gui/data.rs INK_COLOUR), linear: every colour is taken against it.
fn made_dye() -> [f32; 3] {
    srgb(crate::gui::data::INK_COLOUR)
}

/// An sRGB colour (0xrrggbb) as linear.
pub fn srgb_of(rgb: u32) -> [f32; 3] {
    srgb(rgb)
}

fn srgb(rgb: u32) -> [f32; 3] {
    let c = |v: u32| {
        let x = (v & 255) as f32 / 255.0;
        if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) }
    };
    [c(rgb >> 16), c(rgb >> 8), c(rgb)]
}

impl Default for Look {
    fn default() -> Look {
        Look {
            on: true,
            kind: KIND_INK,
            smoke: true,
            threads: true,
            dye: made_dye(),
            density: 1.0,
            lasts: LINGER,
            threads_strength: 1.0,
            shine: 1.0,
            flow: 1.0,
            liquid: super::liquid::Settings::default(),
            flesh: srgb(crate::gui::data::CURSE_COLOUR),
            veins: true,
            veins_speed: 1.0,
            darkness: 1.0,
            curse: super::curse::Settings::default(),
            shadow: super::shadow::Settings::default(),
            traces: true,
            traces_last: 20.0,
            traces_size: 1.0,
            quality: 3,
            only_me: false,
            most_bodies: MOST_BODIES,
        }
    }
}

impl Look {
    fn read() -> Look {
        let mut o = Look::default();
        let text = crate::gui::call("set\tchams");
        o.on = text.split('\t').next() != Some("0");
        for f in text.split('\t').skip(1) {
            let Some((k, v)) = f.split_once('=') else { continue };
            let n = v.parse::<f32>().ok();
            match k {
                "Smoke" => o.smoke = v == "1",
                "Threads" => o.threads = v == "1",
                "Colour" => o.dye = u32::from_str_radix(v, 16).map(srgb).unwrap_or(o.dye),
                "Smoke density" => o.density = n.unwrap_or(o.density),
                "Smoke lasts" => o.lasts = n.unwrap_or(o.lasts).max(1.0),
                "Threads strength" => o.threads_strength = n.unwrap_or(o.threads_strength),
                "Shine" => o.shine = n.unwrap_or(o.shine),
                "Pattern speed" => o.flow = n.unwrap_or(o.flow),
                "Quality" => o.quality = ["Low", "Medium", "High", "Ultra"].iter().position(|q| *q == v).unwrap_or(3),
                "Look" => o.kind = ["Ink", "Polymorph", "Cursed", "Shadow"].iter().position(|q| *q == v).unwrap_or(KIND_INK),
                "Flesh colour" => o.flesh = u32::from_str_radix(v, 16).map(srgb).unwrap_or(o.flesh),
                "Veins" => o.veins = v == "1",
                "Veins speed" => o.veins_speed = n.unwrap_or(o.veins_speed),
                "Darkness" => o.darkness = n.unwrap_or(o.darkness),
                "Traces on the ground" => o.traces = v == "1",
                "Traces last" => o.traces_last = n.unwrap_or(o.traces_last).max(1.0),
                "Traces size" => o.traces_size = n.unwrap_or(o.traces_size),
                "Effects from" => o.only_me = v == "Only me",
                "Bodies with effects" => o.most_bodies = n.map_or(o.most_bodies, |x| x.round().clamp(1.0, MOST_BODIES as f32) as usize),
                _ => {
                    o.liquid.read(k, v);
                    o.curse.read(k, v);
                    o.shadow.read(k, v);
                }
            }
        }
        o
    }

    /// The dye against the one the look was made with, each colour (1 as made).
    fn tint(&self) -> [f32; 3] {
        let d = made_dye();
        std::array::from_fn(|i| self.dye[i].max(0.002) / d[i])
    }

    /// How much light a block of ink at 1 takes out of each colour (ink_draw.comp TAKES as made: its blue-black): as much
    /// as the dye leaves of it thin, a little at least (white ink still thickens the water).
    fn takes(&self) -> [f32; 3] {
        let d = made_dye();
        let made = [34.0f32, 30.0, 19.0];
        std::array::from_fn(|i| (made[i] * self.dye[i].clamp(0.002, 0.999).ln() / d[i].ln()).max(3.0))
    }

    /// The body's look for the creatures' shaders (CursedChamsTint, CursedChamsLook, CursedChamsMore[4]): the ink's
    /// tint, its shine and flow; the curse's flesh, how dark, how fast its veins crawl, whether there are any, how
    /// strongly they beat, their colour; the liquid's (liquid.rs); the shadow's eyes (their colour, whether they glow),
    /// how much smoke.
    pub fn ubo(&self) -> [f32; 24] {
        let mut u = [0.0f32; 24];
        match self.kind {
            KIND_CURSE => {
                let (f, v) = (self.flesh, self.curse.veins_colour);
                u[..12].copy_from_slice(&[f[0], f[1], f[2], 0.0, self.darkness, self.veins_speed, self.veins as i32 as f32, self.curse.beat, v[0], v[1], v[2], 0.0]);
            }
            KIND_LIQUID => u = self.liquid.ubo(),
            KIND_SHADOW => {
                let (e, s) = (self.shadow.eyes_colour, &self.shadow);
                u[..8].copy_from_slice(&[e[0], e[1], e[2], s.eyes as i32 as f32, s.amount, s.wisps as i32 as f32, 0.0, 0.0]);
            }
            _ => {
                let t = self.tint();
                u[..8].copy_from_slice(&[t[0], t[1], t[2], 0.0, self.shine, self.flow, 0.0, 0.0]);
            }
        }
        u
    }

    /// Whether the ink's smoke and threads run (the ink's look, either of them on).
    pub fn ink_runs(&self) -> bool {
        self.kind == KIND_INK && (self.smoke || self.threads)
    }
}

/// What each quality costs (low .. ultra): Jacobi steps of the pressure, steps of the water a second, the ray's step
/// (water cells), whether the ink is seen at half the frame's size, rounds of specks a second.
const JACOBIS: [u32; 4] = [8, 12, 18, 24];
const RATES: [f32; 4] = [30.0, 45.0, 60.0, 60.0];
const RAY_STEPS: [f32; 4] = [0.5, 0.4, 0.3, 0.25];
const HALF: [bool; 4] = [true, true, false, false];
const EMITS: [f32; 4] = [45.0, 60.0, 75.0, 90.0];

/// A body that lets ink go, from the camera (blocks): its feet, height, velocity (blocks a second), how much, its
/// hands (where, how fast); where its feet and hands were the frame before (from the camera now); how much its hands let
/// go (0 a mob's), how wide it is against a player.
#[derive(Clone, Copy, Default)]
pub struct Body {
    pub feet: [f32; 3],
    pub height: f32,
    pub vel: [f32; 3],
    pub amount: f32,
    pub hands: [[f32; 4]; 2],
    pub was_feet: [f32; 3],
    pub was_hands: [[f32; 3]; 2],
    pub hands_on: f32,
    pub wide: f32,
}

/// The grid's fields: the velocity (two, in turn), the ink (now, next, carried forward: on its finer grid), the pressure
/// (two, in turn), how much flows out, the sun's and the sky's reach, the most ink in each brick, the noise (made
/// once; its last channel alone again, for the ink's grain), which of each cell's neighbours are blocks, the most ink in
/// each sub-brick, the velocity's curl.
struct Grid {
    vel: Vec<(Box<Texture>, Box<View>)>,
    ink: Vec<(Box<Texture>, Box<View>)>,
    pres: Vec<(Box<Texture>, Box<View>)>,
    div: (Box<Texture>, Box<View>),
    light: (Box<Texture>, Box<View>),
    bricks: (Box<Texture>, Box<View>),
    noise: (Box<Texture>, Box<View>),
    walls: (Box<Texture>, Box<View>),
    noise_a: (Box<Texture>, Box<View>),
    sub: (Box<Texture>, Box<View>),
    curl: (Box<Texture>, Box<View>),
    noise_made: bool,
}

/// What a dispatch binds, in its set's order.
#[derive(Clone, Copy)]
pub(super) enum Bind {
    Sampled(vk::ImageView, vk::Sampler),
    Storage(vk::ImageView),
    Buffer(vk::Buffer, u64, u64),
    Uniform(vk::Buffer, u64, u64),
}

pub struct Ink {
    sim: Option<Compute>,
    draw: Option<Compute>,
    comp: Option<Compute>,
    part: Option<Compute>,
    /// Linear and repeating (the grid is addressed modulo its size); nearest at the edge (depth).
    repeat: vk::Sampler,
    nearest: vk::Sampler,
    grid: Option<Grid>,
    /// The specks' ring and each source's last speck.
    specks: Option<(Box<Buffer>, Box<Buffer>)>,
    /// The threads' darkness per pixel of the frame, which of its tiles (8 by 8 pixels) a brick with ink covers, and
    /// its size (pixels).
    splat: Option<(Box<Buffer>, Box<Buffer>, u32, u32)>,
    /// The ink seen, at the frame's size.
    picture: Option<(Box<Texture>, Box<View>)>,
    /// The frame's copy the ink is laid over.
    canvas: Option<(Box<Texture>, Box<View>)>,
    vel_now: usize,
    ink_now: usize,
    pres_now: usize,
    origin: [i32; 3],
    /// How far the grid's box is ahead of the body it goes round (cells, eased): a body that moves has most of the box
    /// behind it, where its trail is.
    lead: [f32; 3],
    placed: bool,
    started: Instant,
    last: Option<Instant>,
    owed: f32,
    since_body: f32,
    since_emit: f32,
    steps: u32,
    serial: u32,
    emits: u32,
    frames: u32,
    /// Dev (CURSED_DEV_INKDEBUG): 1 the grid's box and its ink, 2 where the laying over reaches, 3 the threads alone.
    debug: i32,
    /// Dev (CURSED_DEV_INKLOG): a line a second in the log, when the last went.
    log: Option<Instant>,
    /// The look's settings, and when they were asked for.
    pub look: Look,
    look_at: Option<Instant>,
}

pub(super) fn bytes<T: Copy>(dst: *mut u8, at: usize, v: &[T]) {
    unsafe { std::ptr::copy_nonoverlapping(v.as_ptr() as *const u8, dst.add(at), std::mem::size_of_val(v)) };
}

/// An image freed at once (its work done: on shutting down).
pub(super) fn kill(gpu: &Gpu, t: Box<Texture>, v: Box<View>) {
    kill_image(gpu, t, v);
}

fn kill_image(gpu: &Gpu, t: Box<Texture>, v: Box<View>) {
    let d = &gpu.device;
    unsafe {
        d.destroy_image_view(v.raw, None);
        d.destroy_image(t.image, None);
    }
    let mut t = t;
    if let Some(a) = t.alloc.take() {
        gpu.memory.free(d, a);
    }
}

/// A buffer freed at once (its work done: on shutting down).
pub(super) fn kill_buffer_now(gpu: &Gpu, b: Box<Buffer>) {
    kill_buffer(gpu, b);
}

fn kill_buffer(gpu: &Gpu, b: Box<Buffer>) {
    let d = &gpu.device;
    unsafe { d.destroy_buffer(b.raw, None) };
    let mut b = b;
    if let Some(a) = b.alloc.take() {
        gpu.memory.free(d, a);
    }
}

/// One dispatch: its set pushed, its push constants, its groups, a full barrier after it.
pub(super) fn dispatch(gpu: &Gpu, cmd: vk::CommandBuffer, c: &Compute, binds: &[Bind], push: &[u8], groups: [u32; 3]) {
    let images: Vec<vk::DescriptorImageInfo> = binds
        .iter()
        .map(|b| match *b {
            Bind::Sampled(view, sampler) => vk::DescriptorImageInfo { sampler, image_view: view, image_layout: vk::ImageLayout::GENERAL },
            Bind::Storage(view) => vk::DescriptorImageInfo { sampler: vk::Sampler::null(), image_view: view, image_layout: vk::ImageLayout::GENERAL },
            _ => vk::DescriptorImageInfo::default(),
        })
        .collect();
    let buffers: Vec<vk::DescriptorBufferInfo> = binds
        .iter()
        .map(|b| match *b {
            Bind::Buffer(buffer, offset, range) | Bind::Uniform(buffer, offset, range) => vk::DescriptorBufferInfo { buffer, offset, range },
            _ => vk::DescriptorBufferInfo::default(),
        })
        .collect();
    let writes: Vec<vk::WriteDescriptorSet> = binds
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let w = vk::WriteDescriptorSet::default().dst_binding(i as u32);
            match b {
                Bind::Sampled(..) => w.descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(std::slice::from_ref(&images[i])),
                Bind::Storage(..) => w.descriptor_type(vk::DescriptorType::STORAGE_IMAGE).image_info(std::slice::from_ref(&images[i])),
                Bind::Buffer(..) => w.descriptor_type(vk::DescriptorType::STORAGE_BUFFER).buffer_info(std::slice::from_ref(&buffers[i])),
                Bind::Uniform(..) => w.descriptor_type(vk::DescriptorType::UNIFORM_BUFFER).buffer_info(std::slice::from_ref(&buffers[i])),
            }
        })
        .collect();
    let d = &gpu.device;
    unsafe {
        d.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, c.pipeline);
        gpu.push_d.cmd_push_descriptor_set(cmd, vk::PipelineBindPoint::COMPUTE, c.layout, 0, &writes);
        if !push.is_empty() {
            d.cmd_push_constants(cmd, c.layout, vk::ShaderStageFlags::COMPUTE, 0, push);
        }
        d.cmd_dispatch(cmd, groups[0].max(1), groups[1].max(1), groups[2].max(1));
    }
    memory_barrier(gpu, cmd);
}

impl Ink {
    pub fn new(gpu: &Gpu) -> Ink {
        let sampler = |filter: vk::Filter, mode: vk::SamplerAddressMode| {
            let info = vk::SamplerCreateInfo::default()
                .mag_filter(filter)
                .min_filter(filter)
                .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
                .address_mode_u(mode)
                .address_mode_v(mode)
                .address_mode_w(mode)
                .max_lod(0.0);
            fail(unsafe { gpu.device.create_sampler(&info, None) }, "Can't make the ink's sampler")
        };
        Ink {
            sim: None,
            draw: None,
            comp: None,
            part: None,
            repeat: sampler(vk::Filter::LINEAR, vk::SamplerAddressMode::REPEAT),
            nearest: sampler(vk::Filter::NEAREST, vk::SamplerAddressMode::CLAMP_TO_EDGE),
            grid: None,
            specks: None,
            splat: None,
            picture: None,
            canvas: None,
            vel_now: 0,
            ink_now: 0,
            pres_now: 0,
            origin: [0; 3],
            lead: [0.0; 3],
            placed: false,
            started: Instant::now(),
            last: None,
            owed: 0.0,
            since_body: LINGER,
            since_emit: 0.0,
            steps: 0,
            serial: 0,
            emits: 0,
            frames: 0,
            debug: std::env::var("CURSED_DEV_INKDEBUG").ok().and_then(|v| v.trim().parse().ok()).unwrap_or(0),
            log: std::env::var_os("CURSED_DEV_INKLOG").map(|_| Instant::now()),
            look: Look::default(),
            look_at: None,
        }
    }

    /// The look's settings from the menu, at most four times a second (the engine's frame asks each frame).
    pub fn poll_look(&mut self) {
        let now = Instant::now();
        if self.look_at.map_or(true, |t| now.duration_since(t).as_secs_f32() >= 0.25) {
            self.look_at = Some(now);
            self.look = Look::read();
        }
    }

    /// Its four shaders, compiled by Java (lib.rs shaders_init): the simulation, the ray, the laying over, the specks.
    pub fn set_shaders(&mut self, gpu: &Gpu, sim: &[u32], draw: &[u32], comp: &[u32], part: &[u32]) {
        use vk::DescriptorType as T;
        let img = T::COMBINED_IMAGE_SAMPLER;
        let sto = T::STORAGE_IMAGE;
        let sim_kinds = [img, sto, img, sto, sto, sto, sto, img, img, sto, T::UNIFORM_BUFFER, sto, sto, img, sto, sto, sto, sto];
        let draw_kinds = [img, img, img, sto, T::UNIFORM_BUFFER, img, img, img, T::STORAGE_BUFFER];
        let comp_kinds = [sto, img, img, T::STORAGE_BUFFER, T::STORAGE_BUFFER];
        let part_kinds = [T::STORAGE_BUFFER, img, img, img, T::STORAGE_BUFFER, T::UNIFORM_BUFFER, T::STORAGE_BUFFER, img, img, T::STORAGE_BUFFER];
        for (slot, name, spirv, kinds, push) in [
            (&mut self.sim, "ink_sim", sim, &sim_kinds[..], 4u32),
            (&mut self.draw, "ink_draw", draw, &draw_kinds[..], 0u32),
            (&mut self.comp, "ink_comp", comp, &comp_kinds[..], 48u32),
            (&mut self.part, "ink_part", part, &part_kinds[..], 4u32),
        ] {
            if let Some(old) = slot.replace(Compute::new(gpu, name, spirv, kinds, push)) {
                old.destroy(gpu);
            }
        }
    }

    fn make_grid(gpu: &Gpu, cmd: vk::CommandBuffer) -> Grid {
        let f3 = |name: &str, format: vk::Format| image3d(gpu, cmd, name, format, SIZE);
        Grid {
            vel: (0..2).map(|i| f3(&format!("Ink velocity {i}"), vk::Format::R16G16B16A16_SFLOAT)).collect(),
            ink: ["now", "next", "forward"].iter().map(|n| image3d(gpu, cmd, &format!("Ink {n}"), vk::Format::R16_SFLOAT, FINE)).collect(),
            pres: (0..2).map(|i| f3(&format!("Ink pressure {i}"), vk::Format::R16_SFLOAT)).collect(),
            div: f3("Ink divergence", vk::Format::R16_SFLOAT),
            light: f3("Ink sunlight", vk::Format::R16G16_SFLOAT),
            bricks: image3d(gpu, cmd, "Ink bricks", vk::Format::R16_SFLOAT, BRICKS),
            noise: image3d(gpu, cmd, "Ink noise", vk::Format::R16G16B16A16_SFLOAT, [NOISE; 3]),
            walls: f3("Ink walls", vk::Format::R8_UINT),
            noise_a: image3d(gpu, cmd, "Ink grain noise", vk::Format::R16_SFLOAT, [NOISE; 3]),
            sub: image3d(gpu, cmd, "Ink sub-bricks", vk::Format::R16_SFLOAT, SUBS),
            curl: f3("Ink curl", vk::Format::R32G32B32A32_SFLOAT),
            noise_made: false,
        }
    }

    /// One frame of the ink after the level: the water stepped, the specks moved and drawn, the ink seen and laid over
    /// `color` (the level's picture as it goes to the hand and the GUI; `depth` the level's depth). `bodies` from the
    /// camera (blocks); `camera` and `view_proj` the terrain's this frame; `window` the voxel window's lowest block,
    /// width and height.
    #[allow(clippy::too_many_arguments)]
    pub fn frame(
        &mut self,
        gpu: &Gpu,
        enc: &mut Encoder,
        color: &Texture,
        depth: &Texture,
        bodies: &[Body],
        fog: [f32; 4],
        camera: [f64; 3],
        view_proj: [f32; 16],
        sun: [f32; 4],
        sun_color: [f32; 4],
        sky: [f32; 4],
        voxels: Option<(&View, &Sampler)>,
        window: [i32; 5],
        _linear: &Sampler,
    ) {
        let now = Instant::now();
        let dt = self.last.map_or(0.0, |l| now.duration_since(l).as_secs_f32()).min(0.25);
        self.last = Some(now);
        self.since_body = if bodies.is_empty() { self.since_body + dt } else { 0.0 };
        if let Some(at) = self.log {
            if now.duration_since(at).as_secs_f32() >= 1.0 {
                self.log = Some(now);
                let b = bodies.first().map_or([0.0; 3], |b| b.feet);
                let line = format!(
                    "I [native] ink: bodies {} first {:.1},{:.1},{:.1} since {:.1} shaders {} voxels {} grid {} origin {:?} owed {:.3} steps {} emits {} size {}x{} smoke {} threads {} density {:.2} lasts {:.0} quality {} hands {:.1} wide {:.2}",
                    bodies.len(), b[0], b[1], b[2], self.since_body,
                    self.sim.is_some() && self.draw.is_some() && self.comp.is_some() && self.part.is_some(), voxels.is_some(), self.grid.is_some(),
                    self.origin, self.owed, self.steps, self.emits, color.width, color.height, self.look.smoke, self.look.threads, self.look.density,
                    self.look.lasts, self.look.quality, bodies.first().map_or(-1.0, |b| b.hands_on), bodies.first().map_or(-1.0, |b| b.wide)
                );
                if let Ok(mut m) = crate::vk::device::MESSAGES.lock() {
                    m.push(line);
                }
            }
        }
        let (Some(sim), Some(draw), Some(comp), Some(part), Some((vox, vox_sampler))) = (self.sim.as_ref(), self.draw.as_ref(), self.comp.as_ref(), self.part.as_ref(), voxels)
        else {
            return;
        };
        let look = self.look;
        let q = look.quality.min(3);
        // the module off, or another look: the ink gone at once, its memory let go (made afresh when it is on again)
        if !look.on || !look.ink_runs() {
            self.let_go(enc);
            return;
        }
        if self.since_body >= look.lasts || color.width == 0 || color.height == 0 {
            return;
        }
        let cmd = enc.cmd(gpu);
        if self.grid.is_none() {
            self.grid = Some(Self::make_grid(gpu, cmd));
            self.placed = false;
        }
        if self.specks.is_none() {
            let usage = vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST;
            let specks = Box::new(fail(Buffer::with(gpu, usage, SPECKS as u64 * 64, Place::Device), "Can't make the ink's specks"));
            let last = Box::new(fail(Buffer::with(gpu, usage, (MOST_BODIES as u64) * SOURCES as u64 * 4, Place::Device), "Can't make the ink's sources"));
            unsafe {
                // no speck yet (its age below 0), no source's last yet
                gpu.device.cmd_fill_buffer(cmd, specks.raw, 0, vk::WHOLE_SIZE, (-1.0f32).to_bits());
                gpu.device.cmd_fill_buffer(cmd, last.raw, 0, vk::WHOLE_SIZE, u32::MAX);
            }
            memory_barrier(gpu, cmd);
            self.specks = Some((specks, last));
        }
        let (fw, fh) = (color.width, color.height);
        if self.splat.as_ref().map_or(true, |s| s.2 != fw || s.3 != fh) {
            if let Some((b, t, _, _)) = self.splat.take() {
                enc.defer(Doomed::Buffer(b));
                enc.defer(Doomed::Buffer(t));
            }
            let usage = vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST;
            let b = Box::new(fail(Buffer::with(gpu, usage, fw as u64 * fh as u64 * 4, Place::Device), "Can't make the ink's threads"));
            let tiles = fw.div_ceil(8) as u64 * fh.div_ceil(8) as u64 * 4;
            let t = Box::new(fail(Buffer::with(gpu, usage, tiles, Place::Device), "Can't make the ink's tiles"));
            self.splat = Some((b, t, fw, fh));
            for (t, v) in self.picture.take().into_iter().chain(self.canvas.take()) {
                retire(enc, t, v);
            }
        }
        // at the frame's own size (the ink's frayed edges are finer than a half-size picture holds), half at the low
        // qualities
        let (pw, ph) = if HALF[q] { (fw.div_ceil(2), fh.div_ceil(2)) } else { (fw, fh) };
        if self.picture.as_ref().map_or(false, |p| p.0.width != pw || p.0.height != ph) {
            if let Some((t, v)) = self.picture.take() {
                retire(enc, t, v);
            }
        }
        if self.picture.is_none() {
            self.picture = Some(image_in(gpu, cmd, "Ink seen", vk::Format::R16G16B16A16_SFLOAT, pw, ph, 1, false));
        }
        if self.canvas.is_none() {
            self.canvas = Some(image_in(gpu, cmd, "Ink over the frame", vk::Format::R8G8B8A8_UNORM, fw, fh, 1, false));
        }
        let depth_view = super::upscale::temp_view(gpu, enc, depth);
        let seconds = (now.duration_since(self.started).as_secs_f64() % 3600.0) as f32;
        let window4 = [window[0], window[1], window[2], window[3]];
        let window_size = [window[4], 0, 0, 0];

        // the bodies as Java chose them: the one the grid goes round first (one's own, else the nearest to the camera),
        // then the nearest others near it (cursed.recode.chams.ChamsInk.choose)
        let order: Vec<&Body> = bodies.iter().take(MOST_BODIES).collect();
        let cells = |v: f32, axis: usize| ((camera[axis] + v as f64) * CELLS) as f32;
        let at = |a: [f32; 3], b: [f32; 3], f: f32| {
            let v = [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f, a[2] + (b[2] - a[2]) * f];
            [cells(v[0], 0), cells(v[1], 1), cells(v[2], 2)]
        };
        // the bodies (world cells) as far along their way from the frame before as `to`, and where they were at `from`
        let block_at = |from: f32, to: f32| {
            let mut block = [0f32; EACH * MOST_BODIES];
            for (i, b) in order.iter().enumerate() {
                let o = i * EACH;
                let (feet, feet0) = (at(b.was_feet, b.feet, to), at(b.was_feet, b.feet, from));
                block[o..o + 4].copy_from_slice(&[feet[0], feet[1], feet[2], b.height * CELLS as f32]);
                block[o + 4..o + 8].copy_from_slice(&[b.vel[0] * CELLS as f32, b.vel[1] * CELLS as f32, b.vel[2] * CELLS as f32, b.amount]);
                block[o + 16..o + 20].copy_from_slice(&[feet0[0], feet0[1], feet0[2], 0.0]);
                for k in 0..2 {
                    let h = b.hands[k];
                    let (hand, hand0) = (at(b.was_hands[k], [h[0], h[1], h[2]], to), at(b.was_hands[k], [h[0], h[1], h[2]], from));
                    block[o + 8 + k * 4..o + 12 + k * 4].copy_from_slice(&[hand[0], hand[1], hand[2], h[3] * CELLS as f32]);
                    // its hands' weight (0 a mob's), how wide it is
                    block[o + 20 + k * 4..o + 24 + k * 4].copy_from_slice(&[hand0[0], hand0[1], hand0[2], if k == 0 { b.hands_on } else { b.wide }]);
                }
            }
            block
        };
        let count = order.len();
        let mut target = self.origin;
        if let Some(b) = order.first() {
            // the grid round the nearest body: its feet a little above the floor of it, the box ahead of it as it
            // moves (her word: flying, the smoke went and came in jerks — the trail left a box centred on the body
            // 4 blocks behind it): a third of a second of its way, at most 44 cells (the body then 20 cells inside the
            // front); rising, more of it below
            let f = [cells(b.feet[0], 0), cells(b.feet[1], 1), cells(b.feet[2], 2)];
            let k = 0.33 * CELLS as f32;
            let want = [(b.vel[0] * k).clamp(-44.0, 44.0), (b.vel[1] * k).clamp(0.0, 18.0), (b.vel[2] * k).clamp(-44.0, 44.0)];
            let ease = 1.0 - (-dt * 5.0).exp();
            for a in 0..3 {
                self.lead[a] += (want[a] - self.lead[a]) * ease;
            }
            target = [
                (f[0] + self.lead[0]).floor() as i32 - SIZE[0] as i32 / 2,
                (f[1] - self.lead[1]).floor() as i32 - 10,
                (f[2] + self.lead[2]).floor() as i32 - SIZE[2] as i32 / 2,
            ];
        }
        let groups = [SIZE[0] / 8, SIZE[1] / 4, SIZE[2] / 8];
        let fine_groups = [FINE[0] / 8, FINE[1] / 4, FINE[2] / 8];
        let brick_groups = [BRICKS[0] / 8, BRICKS[1] / 4, BRICKS[2] / 8];
        let sub_groups = [SUBS[0] / 8, SUBS[1] / 4, SUBS[2] / 8];
        let sun_dir = {
            let l = (sun[0] * sun[0] + sun[1] * sun[1] + sun[2] * sun[2]).sqrt();
            if l > 1e-4 { [sun[0] / l, sun[1] / l, sun[2] / l] } else { [0.0, 1.0, 0.0] }
        };

        // the water's steps: as many as fit in the time owed (a step taken a little early rather than a frame with none
        // and the next with two), each as long as its share of it; between them the ray looks back along the flow
        let step = 1.0 / RATES[q];
        self.owed += dt;
        let mut n = (self.owed / step + 0.15) as u32;
        let grid = self.grid.as_mut().unwrap();
        // the noise is made on a step: the first frame takes one
        let noise_made = grid.noise_made;
        if !noise_made {
            n = n.max(1);
        }
        let mut step_dt = step;
        if n > 0 {
            n = n.min(MOST_STEPS);
            step_dt = (self.owed / n as f32).clamp(0.5 * step, 2.0 * step.max(STEP));
            self.owed = 0.0;
        }
        grid.noise_made |= n > 0;
        let grid = &*grid;
        for step in 0..n {
            let before = if self.placed { self.origin } else { target };
            self.origin = target;
            self.placed = true;
            let piece = enc.transient.mapped(gpu, SIM_UBO, gpu.info.min_uniform_align.max(16) as u64, SIM_UBO, 1);
            let host = piece.host;
            bytes(host, 0, &[self.origin[0], self.origin[1], self.origin[2], self.steps as i32]);
            bytes(host, 16, &[before[0], before[1], before[2], 0]);
            bytes(host, 32, &window4);
            bytes(host, 48, &window_size);
            bytes(host, 64, &[sun_dir[0], sun_dir[1], sun_dir[2], step_dt]);
            // how much ink the bodies let go, how fast the ink thins (as made at 40 s)
            bytes(host, 80, &[count as f32, seconds, look.density * look.smoke as i32 as f32, LINGER / look.lasts]);
            bytes(host, 96, &block_at(step as f32 / n as f32, (step + 1) as f32 / n as f32));
            let ubo = Bind::Uniform(unsafe { (*piece.buffer).raw }, piece.offset, SIM_UBO);
            let (va, vb) = (self.vel_now, self.vel_now ^ 1);
            let (ink_now, ink_next, ink_fwd) = (self.ink_now, self.ink_now ^ 1, 2);
            let v = |i: usize| grid.vel[i].1.raw;
            let k = |i: usize| grid.ink[i].1.raw;
            let p = |i: usize| grid.pres[i].1.raw;
            let set = |vel_in: usize, vel_out: usize, ink_in: usize, ink_out: usize, pres_in: usize, pres_out: usize| {
                [
                    Bind::Sampled(v(vel_in), self.repeat),
                    Bind::Storage(v(vel_out)),
                    Bind::Sampled(k(ink_in), self.repeat),
                    Bind::Storage(k(ink_out)),
                    Bind::Storage(p(pres_in)),
                    Bind::Storage(p(pres_out)),
                    Bind::Storage(grid.div.1.raw),
                    Bind::Sampled(k(ink_fwd), self.repeat),
                    Bind::Sampled(vox.raw, vox_sampler.raw),
                    Bind::Storage(grid.light.1.raw),
                    ubo,
                    Bind::Storage(grid.bricks.1.raw),
                    Bind::Storage(grid.noise.1.raw),
                    Bind::Sampled(grid.noise.1.raw, self.repeat),
                    Bind::Storage(grid.walls.1.raw),
                    Bind::Storage(grid.noise_a.1.raw),
                    Bind::Storage(grid.sub.1.raw),
                    Bind::Storage(grid.curl.1.raw),
                ]
            };
            let stage = |s: i32| s.to_ne_bytes();
            // 9 the noise, once (the first step)
            if !noise_made && step == 0 {
                dispatch(gpu, cmd, sim, &set(va, vb, ink_now, ink_next, self.pres_now, self.pres_now ^ 1), &stage(9), [NOISE / 8, NOISE / 4, NOISE / 8]);
            }
            // 0 carried along, stirred (vel a → b); 11 its curl; 1 its curl kept, the ink sinking (b → a); 2 how much
            // flows out (dev: each group of stages timed apart, CURSED_DEV_BENCH)
            enc.segment(gpu, cmd, "Engine", "ink water 0-2 stirred (compute)");
            dispatch(gpu, cmd, sim, &set(va, vb, ink_now, ink_next, self.pres_now, self.pres_now ^ 1), &stage(0), groups);
            dispatch(gpu, cmd, sim, &set(vb, va, ink_now, ink_next, self.pres_now, self.pres_now ^ 1), &stage(11), groups);
            dispatch(gpu, cmd, sim, &set(vb, va, ink_now, ink_next, self.pres_now, self.pres_now ^ 1), &stage(1), groups);
            dispatch(gpu, cmd, sim, &set(va, vb, ink_now, ink_next, self.pres_now, self.pres_now ^ 1), &stage(2), groups);
            // 3 the pressure, in turns (warm from the step before)
            enc.segment(gpu, cmd, "Engine", "ink water 3 pressure (compute)");
            for _ in 0..JACOBIS[q] {
                dispatch(gpu, cmd, sim, &set(va, vb, ink_now, ink_next, self.pres_now, self.pres_now ^ 1), &stage(3), groups);
                self.pres_now ^= 1;
            }
            // 4 its slope taken off (a → b); 5 the ink carried forward; 6 carried back and corrected (on the ink's
            // grid); 10 the sub-bricks, 8 the bricks; 7 the sun's and the sky's reach round the bricks with ink
            enc.segment(gpu, cmd, "Engine", "ink water 4 slope (compute)");
            dispatch(gpu, cmd, sim, &set(va, vb, ink_now, ink_next, self.pres_now, self.pres_now ^ 1), &stage(4), groups);
            self.vel_now = vb;
            // without smoke the water still carries the threads, the ink is not stepped
            if !look.smoke {
                self.steps = self.steps.wrapping_add(1);
                continue;
            }
            enc.segment(gpu, cmd, "Engine", "ink water 5-6 carried (compute)");
            dispatch(gpu, cmd, sim, &set(vb, va, ink_now, ink_fwd, self.pres_now, self.pres_now ^ 1), &stage(5), fine_groups);
            dispatch(gpu, cmd, sim, &set(vb, va, ink_now, ink_next, self.pres_now, self.pres_now ^ 1), &stage(6), fine_groups);
            enc.segment(gpu, cmd, "Engine", "ink water 8 bricks (compute)");
            dispatch(gpu, cmd, sim, &set(vb, va, ink_next, ink_now, self.pres_now, self.pres_now ^ 1), &stage(10), sub_groups);
            dispatch(gpu, cmd, sim, &set(vb, va, ink_next, ink_now, self.pres_now, self.pres_now ^ 1), &stage(8), brick_groups);
            enc.segment(gpu, cmd, "Engine", "ink water 7 light (compute)");
            dispatch(gpu, cmd, sim, &set(vb, va, ink_next, ink_now, self.pres_now, self.pres_now ^ 1), &stage(7), groups);
            self.ink_now = ink_next;
            self.steps = self.steps.wrapping_add(1);
        }
        if n > 0 {
            enc.segment_end(gpu, cmd);
        }

        // the specks: let go (EMITS rounds a second), carried, drawn into the threads' buffer
        let (specks, last) = self.specks.as_ref().unwrap();
        let (splat, tiles, _, _) = self.splat.as_ref().unwrap();
        self.since_emit += dt;
        let every = 1.0 / EMITS[q];
        let rounds = if look.threads && count > 0 && self.since_emit >= every { ((self.since_emit / every) as u32).clamp(1, MOST_ROUNDS) } else { 0 };
        if rounds > 0 {
            self.since_emit = (self.since_emit - rounds as f32 * every).clamp(0.0, every);
        }
        let emitting = count as u32 * SOURCES * rounds;
        let piece = enc.transient.mapped(gpu, PART_UBO, gpu.info.min_uniform_align.max(16) as u64, PART_UBO, 1);
        let host = piece.host;
        let cam_cells = [(camera[0] * CELLS) as f32, (camera[1] * CELLS) as f32, (camera[2] * CELLS) as f32];
        bytes(host, 0, &view_proj);
        bytes(host, 64, &[cam_cells[0], cam_cells[1], cam_cells[2], seconds]);
        bytes(host, 80, &[self.origin[0], self.origin[1], self.origin[2], 0]);
        bytes(host, 96, &window4);
        bytes(host, 112, &window_size);
        bytes(host, 128, &[self.serial as i32, emitting as i32, SPECKS as i32, count as i32]);
        bytes(host, 144, &[fw as i32, fh as i32, 0, 0]);
        bytes(host, 160, &[dt, self.emits as f32, look.threads_strength, 0.0]);
        bytes(host, 176, &block_at(0.0, 1.0));
        let part_set = [
            Bind::Buffer(specks.raw, 0, vk::WHOLE_SIZE),
            Bind::Sampled(grid.vel[self.vel_now].1.raw, self.repeat),
            Bind::Sampled(vox.raw, vox_sampler.raw),
            Bind::Sampled(depth_view, self.nearest),
            Bind::Buffer(splat.raw, 0, vk::WHOLE_SIZE),
            Bind::Uniform(unsafe { (*piece.buffer).raw }, piece.offset, PART_UBO),
            Bind::Buffer(last.raw, 0, vk::WHOLE_SIZE),
            Bind::Sampled(grid.noise.1.raw, self.repeat),
            Bind::Sampled(grid.bricks.1.raw, self.nearest),
            Bind::Buffer(tiles.raw, 0, vk::WHOLE_SIZE),
        ];
        enc.segment(gpu, cmd, "Engine", "ink threads (compute)");
        if emitting > 0 {
            dispatch(gpu, cmd, part, &part_set, &0i32.to_ne_bytes(), [emitting.div_ceil(256), 1, 1]);
            dispatch(gpu, cmd, part, &part_set, &3i32.to_ne_bytes(), [(count as u32 * SOURCES).div_ceil(256), 1, 1]);
            self.serial = self.serial.wrapping_add(emitting);
            self.emits = self.emits.wrapping_add(1);
        }
        if dt > 0.0 && look.threads {
            dispatch(gpu, cmd, part, &part_set, &1i32.to_ne_bytes(), [SPECKS / 256, 1, 1]);
        }
        unsafe { gpu.device.cmd_fill_buffer(cmd, splat.raw, 0, vk::WHOLE_SIZE, 0) };
        memory_barrier(gpu, cmd);
        if look.threads {
            dispatch(gpu, cmd, part, &part_set, &2i32.to_ne_bytes(), [SPECKS / 256, 1, 1]);
        }
        // the tiles of the frame the bricks with ink cover (the ray is cast in them alone); none without smoke
        enc.segment(gpu, cmd, "Engine", "ink tiles (compute)");
        unsafe { gpu.device.cmd_fill_buffer(cmd, tiles.raw, 0, vk::WHOLE_SIZE, 0) };
        memory_barrier(gpu, cmd);
        if look.smoke {
            dispatch(gpu, cmd, part, &part_set, &4i32.to_ne_bytes(), [(BRICKS[0] * BRICKS[1] * BRICKS[2]).div_ceil(256), 1, 1]);
        }
        enc.segment_end(gpu, cmd);

        // the ink seen
        let picture = self.picture.as_ref().unwrap();
        let piece = enc.transient.mapped(gpu, DRAW_UBO, gpu.info.min_uniform_align.max(16) as u64, DRAW_UBO, 1);
        let host = piece.host;
        let rel = [
            ((camera[0] * CELLS) - self.origin[0] as f64) as f32,
            ((camera[1] * CELLS) - self.origin[1] as f64) as f32,
            ((camera[2] * CELLS) - self.origin[2] as f64) as f32,
        ];
        bytes(host, 0, &invert(&view_proj));
        bytes(host, 64, &[rel[0], rel[1], rel[2], self.frames as f32]);
        bytes(host, 80, &[self.origin[0], self.origin[1], self.origin[2], 0]);
        bytes(host, 96, &[pw as i32, ph as i32, depth.width as i32, depth.height as i32]);
        bytes(host, 112, &[sun_dir[0], sun_dir[1], sun_dir[2], sun[3].max(0.0)]);
        bytes(host, 128, &sun_color);
        bytes(host, 144, &sky);
        bytes(host, 160, &[fog[0], fog[1], fog[2], if self.debug == 1 { -1.0 } else { fog[3].max(0.0) }]);
        bytes(host, 176, &[seconds, self.owed, dt, RAY_STEPS[q]]);
        let (takes, tint) = (look.takes(), look.tint());
        bytes(host, 192, &[takes[0], takes[1], takes[2], 0.0]);
        // what thin ink gives back (as made: blue-grey, as dye lit in water)
        bytes(host, 208, &[0.10 * tint[0], 0.13 * tint[1], 0.22 * tint[2], 0.0]);
        bytes(host, 224, &[fw as i32, fh as i32, 0, 0]);
        let draw_set = [
            Bind::Sampled(depth_view, self.nearest),
            Bind::Sampled(grid.ink[self.ink_now].1.raw, self.repeat),
            Bind::Sampled(grid.light.1.raw, self.repeat),
            Bind::Storage(picture.1.raw),
            Bind::Uniform(unsafe { (*piece.buffer).raw }, piece.offset, DRAW_UBO),
            Bind::Sampled(grid.vel[self.vel_now].1.raw, self.repeat),
            Bind::Sampled(grid.bricks.1.raw, self.nearest),
            Bind::Sampled(grid.noise_a.1.raw, self.repeat),
            Bind::Buffer(tiles.raw, 0, vk::WHOLE_SIZE),
        ];
        if look.smoke {
            enc.segment(gpu, cmd, "Engine", "ink seen (compute)");
            dispatch(gpu, cmd, draw, &draw_set, &[], [pw.div_ceil(8), ph.div_ceil(8), 1]);
            enc.segment_end(gpu, cmd);
        }

        // laid over a copy of the frame, copied back
        let canvas = self.canvas.as_ref().unwrap();
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
        enc.segment(gpu, cmd, "Engine", "ink over the frame (compute)");
        carry(color.image, canvas.0.image);
        memory_barrier(gpu, cmd);
        let sizes = [
            fw as i32,
            fh as i32,
            if self.debug == 2 { -(pw as i32) } else { pw as i32 },
            if self.debug == 3 { -(ph as i32) } else { ph as i32 },
        ];
        let comp_set = [
            Bind::Storage(canvas.1.raw),
            Bind::Sampled(picture.1.raw, self.nearest),
            Bind::Sampled(depth_view, self.nearest),
            Bind::Buffer(splat.raw, 0, vk::WHOLE_SIZE),
            Bind::Buffer(tiles.raw, 0, vk::WHOLE_SIZE),
        ];
        let mut push = [0u8; 48];
        bytes(push.as_mut_ptr(), 0, &sizes);
        bytes(push.as_mut_ptr(), 16, &[takes[0], takes[1], takes[2], 0.0]);
        bytes(push.as_mut_ptr(), 32, &[tint[0], tint[1], tint[2], 0.0]);
        dispatch(gpu, cmd, comp, &comp_set, &push, [fw.div_ceil(16), fh.div_ceil(16), 1]);
        carry(canvas.0.image, color.image);
        memory_barrier(gpu, cmd);
        enc.segment_end(gpu, cmd);
        self.frames = self.frames.wrapping_add(1);
    }

    /// The water and the specks given up (their images and buffers freed when the frame's work is done).
    fn let_go(&mut self, enc: &mut Encoder) {
        if let Some(g) = self.grid.take() {
            for (t, v) in g.vel.into_iter().chain(g.ink).chain(g.pres).chain([g.div, g.light, g.bricks, g.noise, g.walls, g.noise_a, g.sub, g.curl]) {
                retire(enc, t, v);
            }
        }
        if let Some((s, l)) = self.specks.take() {
            enc.defer(Doomed::Buffer(s));
            enc.defer(Doomed::Buffer(l));
        }
        (self.vel_now, self.ink_now, self.pres_now) = (0, 0, 0);
        (self.owed, self.since_emit) = (0.0, 0.0);
    }

    pub fn destroy(&mut self, gpu: &Gpu) {
        for c in [self.sim.take(), self.draw.take(), self.comp.take(), self.part.take()].into_iter().flatten() {
            c.destroy(gpu);
        }
        if let Some(g) = self.grid.take() {
            for (t, v) in g.vel.into_iter().chain(g.ink).chain(g.pres).chain([g.div, g.light, g.bricks, g.noise, g.walls, g.noise_a, g.sub, g.curl]) {
                kill_image(gpu, t, v);
            }
        }
        if let Some((s, l)) = self.specks.take() {
            kill_buffer(gpu, s);
            kill_buffer(gpu, l);
        }
        if let Some((b, t, _, _)) = self.splat.take() {
            kill_buffer(gpu, b);
            kill_buffer(gpu, t);
        }
        for (t, v) in self.picture.take().into_iter().chain(self.canvas.take()) {
            kill_image(gpu, t, v);
        }
        unsafe {
            gpu.device.destroy_sampler(self.repeat, None);
            gpu.device.destroy_sampler(self.nearest, None);
        }
    }
}
