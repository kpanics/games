//! The engine on the frame thread: the voxel window on the GPU and the frame's engine uniforms.
//!
//! The window is 256 × the world's height × 256 blocks round the camera, kept as 2D array images (x, z, layer = y),
//! addressed modulo 256 so moving only fills the sections that came in. Two images: the voxels themselves (R16_UINT,
//! engine/voxels.rs), and a coarse grid of 4³ bricks (R8_UINT, 1 where something in the brick stops or dims a ray)
//! that lets rays jump over empty air. A palette image holds the materials (row 0 albedo, row 1 glow).
//!
//! Java sends the engine's command once per frame of the world, before the level is drawn (cursed.recode.engine.Engine,
//! frame command 128): it moves the window, uploads what changed, and writes the uniforms terrain reads.

use super::voxels::{self, Section};
use crate::vk::device::Gpu;
use crate::vk::encoder::{memory_barrier, Encoder};
use crate::vk::memory::Place;
use crate::vk::resources::{Buffer, Doomed, Sampler, Texture, View};
use ash::vk;
use std::sync::atomic::{AtomicBool, AtomicU32};

/// The window's width in blocks (and in sections, 16).
pub const WIDTH: i32 = 256;
const SECTIONS: i32 = WIDTH / 16;
/// Sections copied to the GPU per frame at most.
const BUDGET: usize = 256;
/// The uniform block terrain reads (shaders: CursedEngine), bytes.
/// 128 bytes of numbers, then the 16x16 columns' tops (ivec4 x 64).
pub const UBO_SIZE: u64 = 128 + 1024 + 32 + 1024 + 16 + 96;

/// What Java sends each frame (frame command 128).
#[derive(Clone, Copy, Default, Debug)]
pub struct Params {
    pub camera: [f64; 3],
    pub min_y: i32,
    pub height: i32,
    pub sun: [f32; 4],
    pub sun_color: [f32; 4],
    pub moon: [f32; 4],
    pub sky: [f32; 4],
    pub time: f32,
    pub flags: i32,
    /// How big the world is drawn against the window (engine/upscale.rs): textures pick their detail for the window.
    pub scale: f32,
    /// The player in water: feet round the camera, how hard they push (0 not in water).
    pub mover: [f32; 4],
    /// Creatures and players that cast shadows: centre from the camera's block, half size (CursedEntities).
    pub casters: [[f32; 6]; 32],
    pub caster_count: usize,
    /// The clouds: how far they drifted along x (blocks), their bottom (y), 1 when shown.
    pub clouds: [f32; 3],
    /// How wet the ground is, 0..1 (CursedExtra.x): it soaks up over the rain and dries after.
    pub wet: f32,
}

/// Params.flags.
pub const SHADOWS: i32 = 1;
pub const LIGHT: i32 = 2;
pub const WATER: i32 = 8;
pub const REFLECT: i32 = 4;
/// The glow round glowing blocks (shaders/engine/glow.comp).
pub const GLOW: i32 = 128;
/// Plants and leaves sway (terrain.vsh).
pub const WIND: i32 = 512;
/// The sky shaped by the air (shaders/engine/sky.glsl).
pub const SKY: i32 = 1024;
/// Light in the air (shaders/engine/volume.comp, sunvis.comp).
pub const VOLUME: i32 = 32;

struct Images {
    voxels: Box<Texture>,
    voxels_view: Box<View>,
    coarse: Box<Texture>,
    coarse_view: Box<View>,
    height: i32,
}

pub struct Engine {
    /// The chams' ink in the air (engine/ink.rs).
    pub ink: super::ink::Ink,
    /// The chams' traces on the ground (engine/ground.rs).
    pub ground: super::ground::Ground,
    /// The chams' polymorphic liquid (engine/liquid.rs).
    pub liquid: super::liquid::Liquid,
    /// The chams' curse in the air: its ash (engine/curse.rs).
    pub curse: super::curse::Curse,
    /// The chams' shadow in the air: its smoke (engine/shadow.rs).
    pub shadow: super::shadow::Shadow,
    /// Target ESP's tesseract (engine/tesseract.rs).
    pub tesseract: super::tesseract::Tesseract,
    /// Hitboxes (engine/hitboxes.rs).
    pub hitboxes: super::hitboxes::Hitboxes,
    /// The world's effects (engine/fx.rs).
    pub fx: super::fx::Fx,
    /// The cosmetics' models in the hands (cosmetics/render.rs, frame command VIEWMODEL).
    pub viewmodel: crate::cosmetics::render::Viewmodel,
    images: Option<Images>,
    palette: Option<(Box<Texture>, Box<View>)>,
    palette_epoch: u64,
    /// The palette holds the game's cloud map (the clouds' shadow).
    has_clouds: bool,
    nearest: Box<Sampler>,
    /// Which section each slot of the window holds: ((sx mod 16) + (sz mod 16) * 16) * layers + (sy - min).
    slots: Vec<Option<[i32; 3]>>,
    /// The window (its corner x, z, lowest section, layers) when every slot held its section: nothing changed since,
    /// `fill` does not look through the slots again (some six thousand of them: 20 µs every frame, 12.10).
    complete: Option<[i32; 4]>,
    /// Per slot, one above the highest voxel in it a ray can meet (0 none): the columns' tops come from them.
    tops: Vec<u8>,
    store_epoch: u64,
    materials_epoch: u64,
    min_section: i32,
    /// This frame's uniforms (transient memory).
    ubo: Option<(*const Buffer, u64)>,
    pub params: Params,
    pub uploaded: u64,
    /// The shadow mask (shaders/engine/mask.comp): its compute pipeline once Java compiled it, the image at the size of
    /// the depth it was made from, and a 1x1 one bound until then.
    mask_pipeline: Option<Compute>,
    mask: Option<(Box<Texture>, Box<View>, u32, u32)>,
    mask_blank: Option<(Box<Texture>, Box<View>)>,
    /// The frame's terrain camera and ProjMat * ModelViewMat (frame command 122), for the mask.
    camera: [f64; 3],
    view_proj: [f32; 16],
    no_prepass: bool,
    pub upscale: super::upscale::Upscale,
    /// Coloured light (shaders/engine/light.comp): light and bounce, twice (one pair read, one written per step).
    light: Option<[(Box<Texture>, Box<View>); 4]>,
    light_pipeline: Option<Compute>,
    linear: Box<Sampler>,
    blank3d: Option<(Box<Texture>, Box<View>)>,
    /// Which pair of the light grids holds the latest step (0 or 2).
    light_latest: usize,
    frames: u64,
    /// Moving water (shaders/engine/water.comp): two height images taking turns, the latest, steps at 60 a second.
    water: Option<[(Box<Texture>, Box<View>); 2]>,
    water_latest: usize,
    water_pipeline: Option<Compute>,
    water_clock: Option<std::time::Instant>,
    water_due: f64,
    water_steps: u32,
    /// How much of the sun reaches each block round the camera (shaders/engine/sunvis.comp).
    sunvis: Option<(Box<Texture>, Box<View>)>,
    sunvis_pipeline: Option<Compute>,
    sunvis_steps: u32,
    skylut_pipeline: Option<Compute>,
    /// The sun the sky's table was made for.
    sky_lut_sun: Option<[f32; 3]>,
    clock: Option<std::time::Instant>,
    started: std::time::Instant,
    /// The page's look coming in: its model, since when it was asked for, since when all of it is there.
    preview_in: Option<(String, f32, Option<f32>)>,
    light_due: f64,
    sunvis_due: f64,
    /// The picture and its depth before water (reflections on screen): colour, depth, size.
    scene: Option<((Box<Texture>, Box<View>), (Box<Texture>, Box<View>), u32, u32)>,
}

/// A depth image (D32) to copy depth into and sample, laid out GENERAL, recorded into `cmd`.
fn depth_image(gpu: &Gpu, cmd: vk::CommandBuffer, name: &str, width: u32, height: u32) -> (Box<Texture>, Box<View>) {
    let d = &gpu.device;
    let format = vk::Format::D32_SFLOAT;
    let info = vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .format(format)
        .extent(vk::Extent3D { width, height, depth: 1 })
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::OPTIMAL)
        .usage(vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED);
    let raw = fail(unsafe { d.create_image(&info, None) }, "Can't make an engine depth image");
    let req = unsafe { d.get_image_memory_requirements(raw) };
    let alloc = fail(gpu.memory.allocate(d, req, Place::Device, false), "Can't allocate an engine depth image");
    fail(unsafe { d.bind_image_memory(raw, alloc.memory, alloc.offset) }, "Can't bind an engine depth image");
    gpu.name(raw, name);
    let texture = Box::new(Texture {
        image: raw,
        alloc: Some(alloc),
        format,
        aspect: vk::ImageAspectFlags::DEPTH,
        width,
        height,
        layers: 1,
        mips: 1,
        cube: false,
        closed: AtomicBool::new(true),
        views: AtomicU32::new(1),
    });
    let range = vk::ImageSubresourceRange { aspect_mask: vk::ImageAspectFlags::DEPTH, base_mip_level: 0, level_count: 1, base_array_layer: 0, layer_count: 1 };
    let vinfo = vk::ImageViewCreateInfo::default().image(raw).view_type(vk::ImageViewType::TYPE_2D).format(format).subresource_range(range);
    let view_raw = fail(unsafe { d.create_image_view(&vinfo, None) }, "Can't make an engine depth view");
    let view = Box::new(View { raw: view_raw, texture: &*texture as *const Texture, base_mip: 0 });
    let barrier = [vk::ImageMemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
        .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
        .dst_access_mask(vk::AccessFlags2::MEMORY_READ | vk::AccessFlags2::MEMORY_WRITE)
        .old_layout(vk::ImageLayout::UNDEFINED)
        .new_layout(vk::ImageLayout::GENERAL)
        .image(raw)
        .subresource_range(range)];
    unsafe { gpu.sync2_d.cmd_pipeline_barrier2(cmd, &vk::DependencyInfo::default().image_memory_barriers(&barrier)) };
    memory_barrier(gpu, cmd);
    (texture, view)
}

/// The light grid's size (blocks): x, y, z.
const GRID: [u32; 3] = [128, 64, 128];

/// A 3D image in video memory (light grid), laid out GENERAL and cleared to 0, recorded into `cmd`.
pub(crate) fn image3d(gpu: &Gpu, cmd: vk::CommandBuffer, name: &str, format: vk::Format, size: [u32; 3]) -> (Box<Texture>, Box<View>) {
    let d = &gpu.device;
    let info = vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_3D)
        .format(format)
        .extent(vk::Extent3D { width: size[0], height: size[1], depth: size[2] })
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::OPTIMAL)
        .usage(vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_DST)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED);
    let raw = fail(unsafe { d.create_image(&info, None) }, "Can't make a light grid");
    let req = unsafe { d.get_image_memory_requirements(raw) };
    let alloc = fail(gpu.memory.allocate(d, req, Place::Device, false), "Can't allocate a light grid");
    fail(unsafe { d.bind_image_memory(raw, alloc.memory, alloc.offset) }, "Can't bind a light grid");
    gpu.name(raw, name);
    let texture = Box::new(Texture {
        image: raw,
        alloc: Some(alloc),
        format,
        aspect: vk::ImageAspectFlags::COLOR,
        width: size[0],
        height: size[1],
        layers: size[2],
        mips: 1,
        cube: false,
        closed: AtomicBool::new(true),
        views: AtomicU32::new(1),
    });
    let range = vk::ImageSubresourceRange { aspect_mask: vk::ImageAspectFlags::COLOR, base_mip_level: 0, level_count: 1, base_array_layer: 0, layer_count: 1 };
    let vinfo = vk::ImageViewCreateInfo::default().image(raw).view_type(vk::ImageViewType::TYPE_3D).format(format).subresource_range(range);
    let view_raw = fail(unsafe { d.create_image_view(&vinfo, None) }, "Can't make a light grid view");
    let view = Box::new(View { raw: view_raw, texture: &*texture as *const Texture, base_mip: 0 });
    let barrier = [vk::ImageMemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
        .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
        .dst_access_mask(vk::AccessFlags2::MEMORY_READ | vk::AccessFlags2::MEMORY_WRITE)
        .old_layout(vk::ImageLayout::UNDEFINED)
        .new_layout(vk::ImageLayout::GENERAL)
        .image(raw)
        .subresource_range(range)];
    unsafe {
        gpu.sync2_d.cmd_pipeline_barrier2(cmd, &vk::DependencyInfo::default().image_memory_barriers(&barrier));
        d.cmd_clear_color_image(cmd, raw, vk::ImageLayout::GENERAL, &vk::ClearColorValue { float32: [0.0; 4] }, &[range]);
    }
    memory_barrier(gpu, cmd);
    (texture, view)
}

/// A compute pipeline with one push-descriptor set.
pub(crate) struct Compute {
    pub pipeline: vk::Pipeline,
    pub layout: vk::PipelineLayout,
    set_layout: vk::DescriptorSetLayout,
    module: vk::ShaderModule,
}

impl Compute {
    /// `kinds` the set's bindings in order, `push` the push constants' size (0 none).
    pub fn new(gpu: &Gpu, name: &str, spirv: &[u32], kinds: &[vk::DescriptorType], push: u32) -> Compute {
        let d = &gpu.device;
        let module = fail(unsafe { d.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(spirv), None) }, "Can't make an engine shader");
        let bindings: Vec<vk::DescriptorSetLayoutBinding> = kinds
            .iter()
            .enumerate()
            .map(|(i, &k)| vk::DescriptorSetLayoutBinding::default().binding(i as u32).descriptor_type(k).descriptor_count(1).stage_flags(vk::ShaderStageFlags::COMPUTE))
            .collect();
        let set_info = vk::DescriptorSetLayoutCreateInfo::default().flags(vk::DescriptorSetLayoutCreateFlags::PUSH_DESCRIPTOR_KHR).bindings(&bindings);
        let set_layout = fail(unsafe { d.create_descriptor_set_layout(&set_info, None) }, "Can't make an engine layout");
        let ranges = [vk::PushConstantRange { stage_flags: vk::ShaderStageFlags::COMPUTE, offset: 0, size: push }];
        let set_layouts = [set_layout];
        let mut layout_info = vk::PipelineLayoutCreateInfo::default().set_layouts(&set_layouts);
        if push > 0 {
            layout_info = layout_info.push_constant_ranges(&ranges);
        }
        let layout = fail(unsafe { d.create_pipeline_layout(&layout_info, None) }, "Can't make an engine layout");
        let stage = vk::PipelineShaderStageCreateInfo::default().stage(vk::ShaderStageFlags::COMPUTE).module(module).name(c"main");
        let info = [vk::ComputePipelineCreateInfo::default().stage(stage).layout(layout)];
        let pipeline = unsafe { d.create_compute_pipelines(vk::PipelineCache::null(), &info, None) }.map(|v| v[0]).map_err(|(_, e)| e);
        let pipeline = fail(pipeline, "Can't make an engine pipeline");
        gpu.name(pipeline, &format!("Pipeline cursed:{name}"));
        Compute { pipeline, layout, set_layout, module }
    }

    pub fn destroy(self, gpu: &Gpu) {
        let d = &gpu.device;
        unsafe {
            d.destroy_pipeline(self.pipeline, None);
            d.destroy_pipeline_layout(self.layout, None);
            d.destroy_descriptor_set_layout(self.set_layout, None);
            d.destroy_shader_module(self.module, None);
        }
    }
}

/// The mask shader's push constants: inverse view-projection, camera block, its fraction, size.
const MASK_PUSH: u32 = 112;

/// The inverse of a column-major 4x4 matrix (by cofactors, in doubles).
pub(crate) fn invert(m: &[f32; 16]) -> [f32; 16] {
    let a = |c: usize, r: usize| m[c * 4 + r] as f64;
    let mut cof = [[0f64; 4]; 4];
    for r in 0..4 {
        for c in 0..4 {
            let rows: Vec<usize> = (0..4).filter(|&x| x != r).collect();
            let cols: Vec<usize> = (0..4).filter(|&x| x != c).collect();
            let det2 = |r0: usize, r1: usize, c0: usize, c1: usize| a(c0, r0) * a(c1, r1) - a(c1, r0) * a(c0, r1);
            let m3 = a(cols[0], rows[0]) * det2(rows[1], rows[2], cols[1], cols[2]) - a(cols[1], rows[0]) * det2(rows[1], rows[2], cols[0], cols[2])
                + a(cols[2], rows[0]) * det2(rows[1], rows[2], cols[0], cols[1]);
            cof[r][c] = if (r + c) % 2 == 0 { m3 } else { -m3 };
        }
    }
    let det: f64 = (0..4).map(|c| a(c, 0) * cof[0][c]).sum();
    let det = if det.abs() < 1e-30 { 1e-30 } else { det };
    let mut inv = [0f64; 16];
    // inverse[r][c] = cofactor[c][r] / det, stored column-major.
    for r in 0..4 {
        for c in 0..4 {
            inv[c * 4 + r] = cof[c][r] / det;
        }
    }
    std::array::from_fn(|i| inv[i] as f32)
}

/// The camera's place and matrix moved to the eye. The view's bob (GameRenderer.bobView) is in the projection: the eye
/// sits a little off the camera's place (up to a tenth of a block while running), and every pixel's ray starts there.
/// The passes that cast their rays from their coordinates' middle (the effects, the hitboxes, Target ESP, the chams'
/// ink frame) get these, their Java the eye's place (cursed.recode.render.NativeTerrain.eye); else the sky's rays
/// (taken from a point a tenth of a block out) went astray and what was drawn against the sky was lost, and what
/// stands for a body (its boxes) slid off it by the bob.
pub(crate) fn from_eye(camera: [f64; 3], view_proj: &[f32; 16]) -> ([f64; 3], [f32; 16]) {
    let inv = invert(view_proj);
    // the eye: what the projection sends to (0, 0, z, 0), the inverse's third column
    let w = inv[11];
    if w.abs() < 1e-12 {
        return (camera, *view_proj);
    }
    let e = [inv[8] / w, inv[9] / w, inv[10] / w];
    let mut m = *view_proj;
    for r in 0..4 {
        m[12 + r] += view_proj[r] * e[0] + view_proj[4 + r] * e[1] + view_proj[8 + r] * e[2];
    }
    ([camera[0] + e[0] as f64, camera[1] + e[1] as f64, camera[2] + e[2] as f64], m)
}

pub(crate) fn fail<T>(r: Result<T, vk::Result>, what: &str) -> T {
    r.unwrap_or_else(|e| panic!("{what}: {e:?}"))
}

/// A 2D array image (or a plain 2D one with 1 layer) in video memory, its view, laid out GENERAL and cleared to 0.
#[allow(clippy::too_many_arguments)]
fn image(gpu: &Gpu, enc: &mut Encoder, name: &str, format: vk::Format, width: u32, height: u32, layers: u32, array: bool) -> (Box<Texture>, Box<View>) {
    let cmd = enc.cmd(gpu);
    image_in(gpu, cmd, name, format, width, height, layers, array)
}

/// The same, its first layout and clear recorded into `cmd`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn image_in(gpu: &Gpu, cmd: vk::CommandBuffer, name: &str, format: vk::Format, width: u32, height: u32, layers: u32, array: bool) -> (Box<Texture>, Box<View>) {
    let d = &gpu.device;
    let info = vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .format(format)
        .extent(vk::Extent3D { width, height, depth: 1 })
        .mip_levels(1)
        .array_layers(layers)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::OPTIMAL)
        .usage(vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::TRANSFER_SRC | vk::ImageUsageFlags::STORAGE)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED);
    let raw = fail(unsafe { d.create_image(&info, None) }, "Can't make an engine image");
    let req = unsafe { d.get_image_memory_requirements(raw) };
    let alloc = fail(gpu.memory.allocate(d, req, Place::Device, false), "Can't allocate an engine image");
    fail(unsafe { d.bind_image_memory(raw, alloc.memory, alloc.offset) }, "Can't bind an engine image");
    gpu.name(raw, name);
    let texture = Box::new(Texture {
        image: raw,
        alloc: Some(alloc),
        format,
        aspect: vk::ImageAspectFlags::COLOR,
        width,
        height,
        layers,
        mips: 1,
        cube: false,
        closed: AtomicBool::new(true),
        views: AtomicU32::new(1),
    });
    let range = vk::ImageSubresourceRange { aspect_mask: vk::ImageAspectFlags::COLOR, base_mip_level: 0, level_count: 1, base_array_layer: 0, layer_count: layers };
    let vinfo = vk::ImageViewCreateInfo::default()
        .image(raw)
        .view_type(if array { vk::ImageViewType::TYPE_2D_ARRAY } else { vk::ImageViewType::TYPE_2D })
        .format(format)
        .subresource_range(range);
    let view_raw = fail(unsafe { d.create_image_view(&vinfo, None) }, "Can't make an engine image view");
    let view = Box::new(View { raw: view_raw, texture: &*texture as *const Texture, base_mip: 0 });
    let barrier = [vk::ImageMemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
        .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
        .dst_access_mask(vk::AccessFlags2::MEMORY_READ | vk::AccessFlags2::MEMORY_WRITE)
        .old_layout(vk::ImageLayout::UNDEFINED)
        .new_layout(vk::ImageLayout::GENERAL)
        .image(raw)
        .subresource_range(range)];
    unsafe {
        gpu.sync2_d.cmd_pipeline_barrier2(cmd, &vk::DependencyInfo::default().image_memory_barriers(&barrier));
        d.cmd_clear_color_image(cmd, raw, vk::ImageLayout::GENERAL, &vk::ClearColorValue { uint32: [0; 4] }, &[range]);
    }
    memory_barrier(gpu, cmd);
    (texture, view)
}

pub(crate) fn retire(enc: &mut Encoder, texture: Box<Texture>, view: Box<View>) {
    enc.defer(Doomed::View(view));
    enc.defer(Doomed::Texture(texture));
}

/// The 4³ bricks of a section ([y][z][x], 64 bytes): 1 where something in it stops or dims a ray.
fn bricks(s: &Section, out: &mut [u8]) {
    match s {
        Section::Uniform(v) => out[..64].fill(voxels::blocks_rays(v & 15) as u8),
        Section::Data(d) => {
            out[..64].fill(0);
            for y in 0..16 {
                for z in 0..16 {
                    for x in 0..16 {
                        if voxels::blocks_rays(d[x + z * 16 + y * 256] & 15) {
                            out[(x >> 2) + (z >> 2) * 4 + (y >> 2) * 16] = 1;
                        }
                    }
                }
            }
        }
    }
}

impl Engine {
    pub fn new(gpu: &Gpu) -> Engine {
        let info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::NEAREST)
            .min_filter(vk::Filter::NEAREST)
            .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
            .address_mode_u(vk::SamplerAddressMode::REPEAT)
            .address_mode_v(vk::SamplerAddressMode::REPEAT)
            .address_mode_w(vk::SamplerAddressMode::REPEAT)
            .max_lod(0.0);
        let nearest = Box::new(Sampler { raw: fail(unsafe { gpu.device.create_sampler(&info, None) }, "Can't make the engine sampler") });
        Engine {
            ink: super::ink::Ink::new(gpu),
            ground: super::ground::Ground::new(),
            liquid: super::liquid::Liquid::new(),
            curse: super::curse::Curse::new(),
            shadow: super::shadow::Shadow::new(),
            tesseract: super::tesseract::Tesseract::new(),
            hitboxes: super::hitboxes::Hitboxes::new(),
            fx: super::fx::Fx::new(),
            viewmodel: Default::default(),
            images: None,
            palette: None,
            palette_epoch: 0,
            has_clouds: false,
            nearest,
            slots: Vec::new(),
            complete: None,
            tops: Vec::new(),
            store_epoch: u64::MAX,
            materials_epoch: u64::MAX,
            min_section: 0,
            ubo: None,
            params: Params::default(),
            uploaded: 0,
            mask_pipeline: None,
            mask: None,
            mask_blank: None,
            camera: [0.0; 3],
            view_proj: [0.0; 16],
            no_prepass: std::env::var_os("CURSED_DEV_NOPREPASS").is_some(),
            upscale: super::upscale::Upscale::new(gpu),
            light: None,
            light_pipeline: None,
            blank3d: None,
            light_latest: 0,
            frames: 0,
            water: None,
            water_latest: 0,
            water_pipeline: None,
            water_clock: None,
            water_due: 0.0,
            water_steps: 0,
            sunvis: None,
            sunvis_pipeline: None,
            sunvis_steps: 0,
            skylut_pipeline: None,
            sky_lut_sun: None,
            clock: None,
            started: std::time::Instant::now(),
            preview_in: None,
            light_due: 0.0,
            sunvis_due: 0.0,
            scene: None,
            linear: Box::new(Sampler {
                raw: fail(
                    unsafe {
                        gpu.device.create_sampler(
                            &vk::SamplerCreateInfo::default()
                                .mag_filter(vk::Filter::LINEAR)
                                .min_filter(vk::Filter::LINEAR)
                                .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
                                .address_mode_u(vk::SamplerAddressMode::REPEAT)
                                .address_mode_v(vk::SamplerAddressMode::REPEAT)
                                .address_mode_w(vk::SamplerAddressMode::REPEAT)
                                .max_lod(0.0),
                            None,
                        )
                    },
                    "Can't make the engine's linear sampler",
                ),
            }),
        }
    }

    /// The mask's compute shader, compiled by Java (lib.rs shaders_init).
    pub fn set_mask_shader(&mut self, gpu: &Gpu, spirv: &[u32]) {
        let kinds = [
            vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            vk::DescriptorType::STORAGE_IMAGE,
            vk::DescriptorType::UNIFORM_BUFFER,
        ];
        if let Some(old) = self.mask_pipeline.replace(Compute::new(gpu, "engine_mask", spirv, &kinds, MASK_PUSH)) {
            old.destroy(gpu);
        }
    }

    /// The coloured light's compute shader, compiled by Java (lib.rs shaders_init).
    pub fn set_light_shader(&mut self, gpu: &Gpu, spirv: &[u32]) {
        let kinds = [
            vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            vk::DescriptorType::STORAGE_IMAGE,
            vk::DescriptorType::STORAGE_IMAGE,
            vk::DescriptorType::STORAGE_IMAGE,
            vk::DescriptorType::STORAGE_IMAGE,
            vk::DescriptorType::UNIFORM_BUFFER,
        ];
        if let Some(old) = self.light_pipeline.replace(Compute::new(gpu, "engine_light", spirv, &kinds, 16)) {
            old.destroy(gpu);
        }
    }

    /// One step of the coloured light (a block further each frame), after the voxels and the uniforms of the frame
    /// (outside any rendering): from the latest pair into the other, which terrain then reads.
    fn spread_light(&mut self, gpu: &Gpu, enc: &mut Encoder, p: &Params) {
        let Some(c) = self.light_pipeline.as_ref() else { return };
        let cmd = enc.cmd(gpu);
        if self.light.is_none() {
            let make = |name: &str| image3d(gpu, cmd, name, vk::Format::R8G8B8A8_UNORM, GRID);
            self.light = Some([make("Engine light A"), make("Engine bounce A"), make("Engine light B"), make("Engine bounce B")]);
        }
        let (Some(images), Some(grids)) = (self.images.as_ref(), self.light.as_ref()) else { return };
        let Some((pal, pal_view)) = self.palette.as_ref() else { return };
        let _ = pal;
        let (ubo, ubo_offset) = self.ubo.unwrap();
        let origin = [
            p.camera[0].floor() as i32 - GRID[0] as i32 / 2,
            p.camera[1].floor() as i32 - GRID[1] as i32 / 2,
            p.camera[2].floor() as i32 - GRID[2] as i32 / 2,
            0,
        ];
        let mut push = [0u8; 16];
        unsafe { std::ptr::copy_nonoverlapping(origin.as_ptr() as *const u8, push.as_mut_ptr(), 16) };
        let d = &gpu.device;
        enc.segment(gpu, cmd, "Engine", "light (compute)");
        {
            let from = self.light_latest;
            let to = from ^ 2;
            let s = self.nearest.raw;
            let infos = [
                vk::DescriptorImageInfo { sampler: s, image_view: images.voxels_view.raw, image_layout: vk::ImageLayout::GENERAL },
                vk::DescriptorImageInfo { sampler: s, image_view: pal_view.raw, image_layout: vk::ImageLayout::GENERAL },
                vk::DescriptorImageInfo { sampler: vk::Sampler::null(), image_view: grids[from].1.raw, image_layout: vk::ImageLayout::GENERAL },
                vk::DescriptorImageInfo { sampler: vk::Sampler::null(), image_view: grids[from + 1].1.raw, image_layout: vk::ImageLayout::GENERAL },
                vk::DescriptorImageInfo { sampler: vk::Sampler::null(), image_view: grids[to].1.raw, image_layout: vk::ImageLayout::GENERAL },
                vk::DescriptorImageInfo { sampler: vk::Sampler::null(), image_view: grids[to + 1].1.raw, image_layout: vk::ImageLayout::GENERAL },
            ];
            let buffer = [vk::DescriptorBufferInfo { buffer: unsafe { (*ubo).raw }, offset: ubo_offset, range: UBO_SIZE }];
            let w = |i: usize, kind: vk::DescriptorType| vk::WriteDescriptorSet::default().dst_binding(i as u32).descriptor_type(kind).image_info(std::slice::from_ref(&infos[i]));
            let writes = [
                w(0, vk::DescriptorType::COMBINED_IMAGE_SAMPLER),
                w(1, vk::DescriptorType::COMBINED_IMAGE_SAMPLER),
                w(2, vk::DescriptorType::STORAGE_IMAGE),
                w(3, vk::DescriptorType::STORAGE_IMAGE),
                w(4, vk::DescriptorType::STORAGE_IMAGE),
                w(5, vk::DescriptorType::STORAGE_IMAGE),
                vk::WriteDescriptorSet::default().dst_binding(6).descriptor_type(vk::DescriptorType::UNIFORM_BUFFER).buffer_info(&buffer),
            ];
            unsafe {
                d.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, c.pipeline);
                gpu.push_d.cmd_push_descriptor_set(cmd, vk::PipelineBindPoint::COMPUTE, c.layout, 0, &writes);
                d.cmd_push_constants(cmd, c.layout, vk::ShaderStageFlags::COMPUTE, 0, &push);
                d.cmd_dispatch(cmd, GRID[0] / 4, GRID[1] / 4, GRID[2] / 4);
            }
            memory_barrier(gpu, cmd);
            self.light_latest = to;
        }
        enc.segment_end(gpu, cmd);
    }

    /// Whether the engine draws nothing this frame ("Off" in Video Settings): the game's own shaders draw instead.
    pub fn vanilla(&self) -> bool {
        self.params.flags & (0xFF | WIND | SKY) == 0
    }

    /// Whether water takes reflections from the picture (reflections on).
    pub fn wants_scene(&self) -> bool {
        self.params.flags & REFLECT != 0 && self.ready()
    }

    /// The picture so far and its depth copied (outside the rendering, in the pass's command buffer).
    pub fn capture(&mut self, gpu: &Gpu, enc: &mut Encoder, cmd: vk::CommandBuffer, color: &View, depth: &View) {
        let ct = color.texture();
        let dt = depth.texture();
        let (w, h) = (ct.width, ct.height);
        if dt.width != w || dt.height != h || dt.format != vk::Format::D32_SFLOAT {
            return;
        }
        if self.scene.as_ref().map_or(true, |s| s.2 != w || s.3 != h || s.0 .0.format != ct.format) {
            if let Some((c, d, _, _)) = self.scene.take() {
                retire(enc, c.0, c.1);
                retire(enc, d.0, d.1);
            }
            let c = image_in(gpu, cmd, "Engine picture before water", ct.format, w, h, 1, false);
            let d = depth_image(gpu, cmd, "Engine depth before water", w, h);
            self.scene = Some((c, d, w, h));
        }
        let (c, d, _, _) = self.scene.as_ref().unwrap();
        let extent = vk::Extent3D { width: w, height: h, depth: 1 };
        let color_layers = vk::ImageSubresourceLayers { aspect_mask: vk::ImageAspectFlags::COLOR, mip_level: color.base_mip, base_array_layer: 0, layer_count: 1 };
        let color_dst = vk::ImageSubresourceLayers { aspect_mask: vk::ImageAspectFlags::COLOR, mip_level: 0, base_array_layer: 0, layer_count: 1 };
        let depth_layers = vk::ImageSubresourceLayers { aspect_mask: vk::ImageAspectFlags::DEPTH, mip_level: 0, base_array_layer: 0, layer_count: 1 };
        let copy = |src, dst| vk::ImageCopy { src_subresource: src, src_offset: vk::Offset3D::default(), dst_subresource: dst, dst_offset: vk::Offset3D::default(), extent };
        unsafe {
            gpu.device.cmd_copy_image(cmd, ct.image, vk::ImageLayout::GENERAL, c.0.image, vk::ImageLayout::GENERAL, &[copy(color_layers, color_dst)]);
            gpu.device.cmd_copy_image(cmd, dt.image, vk::ImageLayout::GENERAL, d.0.image, vk::ImageLayout::GENERAL, &[copy(depth_layers, depth_layers)]);
        }
        memory_barrier(gpu, cmd);
    }

    /// The sun's reach's compute shader, compiled by Java (lib.rs shaders_init).
    pub fn set_sunvis_shader(&mut self, gpu: &Gpu, spirv: &[u32]) {
        let kinds = [
            vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            vk::DescriptorType::STORAGE_IMAGE,
            vk::DescriptorType::UNIFORM_BUFFER,
            vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
        ];
        if let Some(old) = self.sunvis_pipeline.replace(Compute::new(gpu, "engine_sunvis", spirv, &kinds, 48)) {
            old.destroy(gpu);
        }
    }

    /// An eighth of the sun's reach traced again (outside any rendering).
    fn trace_sunvis(&mut self, gpu: &Gpu, enc: &mut Encoder, p: &Params) {
        let Some(c) = self.sunvis_pipeline.as_ref() else { return };
        let cmd = enc.cmd(gpu);
        if self.sunvis.is_none() {
            self.sunvis = Some(image3d(gpu, cmd, "Engine sun reach", vk::Format::R8_UNORM, GRID));
        }
        let (Some(images), Some((_, view)), Some((_, palette))) = (self.images.as_ref(), self.sunvis.as_ref(), self.palette.as_ref()) else { return };
        let (ubo, ubo_offset) = self.ubo.unwrap();
        let base = [p.camera[0].floor() as i32, p.camera[1].floor() as i32, p.camera[2].floor() as i32, 0];
        let origin = [base[0] - GRID[0] as i32 / 2, base[1] - GRID[1] as i32 / 2, base[2] - GRID[2] as i32 / 2, 0];
        let frame = [self.sunvis_steps as i32, 0, 0, 0];
        let mut push = [0u8; 48];
        unsafe {
            std::ptr::copy_nonoverlapping(origin.as_ptr() as *const u8, push.as_mut_ptr(), 16);
            std::ptr::copy_nonoverlapping(base.as_ptr() as *const u8, push.as_mut_ptr().add(16), 16);
            std::ptr::copy_nonoverlapping(frame.as_ptr() as *const u8, push.as_mut_ptr().add(32), 16);
        }
        let s = self.nearest.raw;
        let infos = [
            vk::DescriptorImageInfo { sampler: s, image_view: images.voxels_view.raw, image_layout: vk::ImageLayout::GENERAL },
            vk::DescriptorImageInfo { sampler: s, image_view: images.coarse_view.raw, image_layout: vk::ImageLayout::GENERAL },
            vk::DescriptorImageInfo { sampler: vk::Sampler::null(), image_view: view.raw, image_layout: vk::ImageLayout::GENERAL },
            // The palette with the cloud map: the clouds' shadow in the air.
            vk::DescriptorImageInfo { sampler: s, image_view: palette.raw, image_layout: vk::ImageLayout::GENERAL },
        ];
        let buffer = [vk::DescriptorBufferInfo { buffer: unsafe { (*ubo).raw }, offset: ubo_offset, range: UBO_SIZE }];
        let writes = [
            vk::WriteDescriptorSet::default().dst_binding(0).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(std::slice::from_ref(&infos[0])),
            vk::WriteDescriptorSet::default().dst_binding(1).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(std::slice::from_ref(&infos[1])),
            vk::WriteDescriptorSet::default().dst_binding(2).descriptor_type(vk::DescriptorType::STORAGE_IMAGE).image_info(std::slice::from_ref(&infos[2])),
            vk::WriteDescriptorSet::default().dst_binding(3).descriptor_type(vk::DescriptorType::UNIFORM_BUFFER).buffer_info(&buffer),
            vk::WriteDescriptorSet::default().dst_binding(4).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(std::slice::from_ref(&infos[3])),
        ];
        let d = &gpu.device;
        enc.segment(gpu, cmd, "Engine", "sun reach (compute)");
        unsafe {
            d.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, c.pipeline);
            gpu.push_d.cmd_push_descriptor_set(cmd, vk::PipelineBindPoint::COMPUTE, c.layout, 0, &writes);
            d.cmd_push_constants(cmd, c.layout, vk::ShaderStageFlags::COMPUTE, 0, &push);
            d.cmd_dispatch(cmd, GRID[0] / 4, GRID[1] / 4, GRID[2] / 4);
        }
        memory_barrier(gpu, cmd);
        enc.segment_end(gpu, cmd);
    }

    /// The water's compute shader, compiled by Java (lib.rs shaders_init).
    pub fn set_water_shader(&mut self, gpu: &Gpu, spirv: &[u32]) {
        let kinds = [vk::DescriptorType::STORAGE_IMAGE, vk::DescriptorType::STORAGE_IMAGE];
        if let Some(old) = self.water_pipeline.replace(Compute::new(gpu, "engine_water", spirv, &kinds, 32)) {
            old.destroy(gpu);
        }
    }

    /// The water's steps due since the last frame (60 a second, at most 4 at once), outside any rendering.
    fn move_water(&mut self, gpu: &Gpu, enc: &mut Encoder, p: &Params) {
        let Some(c) = self.water_pipeline.as_ref() else { return };
        let now = std::time::Instant::now();
        let elapsed = self.water_clock.map_or(0.0, |t| now.duration_since(t).as_secs_f64());
        self.water_clock = Some(now);
        self.water_due = (self.water_due + elapsed * 60.0).min(4.0);
        let cmd = enc.cmd(gpu);
        if self.water.is_none() {
            let make = |name: &str| image_in(gpu, cmd, name, vk::Format::R16G16_SFLOAT, 256, 256, 1, false);
            self.water = Some([make("Engine water A"), make("Engine water B")]);
        }
        let steps = self.water_due.floor() as u32;
        if steps == 0 {
            return;
        }
        self.water_due -= steps as f64;
        let grids = self.water.as_ref().unwrap();
        let origin = [(p.camera[0] * 4.0).floor() as i32 - 128, (p.camera[2] * 4.0).floor() as i32 - 128];
        let mover = [((p.camera[0] + p.mover[0] as f64) * 4.0) as f32, ((p.camera[2] + p.mover[2] as f64) * 4.0) as f32, p.mover[3], p.sky[3]];
        let d = &gpu.device;
        enc.segment(gpu, cmd, "Engine", "water (compute)");
        for _ in 0..steps {
            let (from, to) = (self.water_latest, self.water_latest ^ 1);
            let infos = [
                vk::DescriptorImageInfo { sampler: vk::Sampler::null(), image_view: grids[from].1.raw, image_layout: vk::ImageLayout::GENERAL },
                vk::DescriptorImageInfo { sampler: vk::Sampler::null(), image_view: grids[to].1.raw, image_layout: vk::ImageLayout::GENERAL },
            ];
            let writes = [
                vk::WriteDescriptorSet::default().dst_binding(0).descriptor_type(vk::DescriptorType::STORAGE_IMAGE).image_info(std::slice::from_ref(&infos[0])),
                vk::WriteDescriptorSet::default().dst_binding(1).descriptor_type(vk::DescriptorType::STORAGE_IMAGE).image_info(std::slice::from_ref(&infos[1])),
            ];
            self.water_steps = self.water_steps.wrapping_add(1);
            let info = [self.water_steps as i32, origin[0], origin[1], 0];
            let mut push = [0u8; 32];
            unsafe {
                std::ptr::copy_nonoverlapping(mover.as_ptr() as *const u8, push.as_mut_ptr(), 16);
                std::ptr::copy_nonoverlapping(info.as_ptr() as *const u8, push.as_mut_ptr().add(16), 16);
                d.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, c.pipeline);
                gpu.push_d.cmd_push_descriptor_set(cmd, vk::PipelineBindPoint::COMPUTE, c.layout, 0, &writes);
                d.cmd_push_constants(cmd, c.layout, vk::ShaderStageFlags::COMPUTE, 0, &push);
                d.cmd_dispatch(cmd, 16, 16, 1);
            }
            memory_barrier(gpu, cmd);
            self.water_latest = to;
        }
        enc.segment_end(gpu, cmd);
    }

    /// The chams' ink after the level (frame command 133, engine/ink.rs): its steps and it over `color`, the level's
    /// picture (`depth` its depth); the voxel window's blocks its walls.
    pub fn ink_frame(&mut self, gpu: &Gpu, enc: &mut Encoder, color: &Texture, depth: &Texture, bodies: &[super::ink::Body], boxes: &[super::liquid::LiquidBox], fog: [f32; 4]) {
        let p = self.params;
        let (ox, oz) = Self::origin(&p);
        let window = [ox * 16, self.min_section * 16, oz * 16, WIDTH, self.images.as_ref().map_or(0, |i| i.height)];
        let voxels = self.images.as_ref().map(|i| (&*i.voxels_view, &*self.nearest));
        // the traces on the ground first: the smoke goes over them
        let look = self.ink.look;
        // (from the eye: their bodies and boxes are given from there, cursed.recode.chams.ChamsInk)
        let (camera, view_proj) = from_eye(self.camera, &self.view_proj);
        self.ground.frame(gpu, enc, color, depth, bodies, &look, camera, view_proj, p.sun, p.sky, fog, voxels, window, &self.nearest);
        self.liquid.frame(gpu, enc, color, depth, boxes, &look, camera, view_proj, p.sun, p.sun_color, p.sky, fog, voxels, window, &self.linear, &self.nearest);
        self.curse.frame(gpu, enc, color, depth, boxes, bodies, &look, camera, view_proj, p.sun, p.sun_color, p.sky, fog, voxels, window, &self.nearest);
        self.shadow.frame(gpu, enc, color, depth, boxes, &look, camera, view_proj, p.sun, p.sky, &self.nearest);
        self.ink.frame(gpu, enc, color, depth, bodies, fog, camera, view_proj, p.sun, p.sun_color, p.sky, voxels, window, &self.linear);
    }

    /// Target ESP after the chams (frame command 134, engine/tesseract.rs): its marks over `color`, the level's picture
    /// (`depth` its depth); the voxel window's blocks what stops their light.
    pub fn target_frame(&mut self, gpu: &Gpu, enc: &mut Encoder, color: &Texture, depth: &Texture, targets: &[super::tesseract::Target]) {
        let p = self.params;
        let (ox, oz) = Self::origin(&p);
        let window = [ox * 16, self.min_section * 16, oz * 16, WIDTH, self.images.as_ref().map_or(0, |i| i.height)];
        let voxels = self.images.as_ref().map(|i| (&*i.voxels_view, &*self.nearest));
        self.tesseract.poll();
        let (camera, view_proj) = from_eye(self.camera, &self.view_proj);
        self.tesseract.frame(gpu, enc, color, depth, targets, camera, view_proj, voxels, window, &self.nearest);
    }

    /// Hitboxes after Target ESP (frame command 135, engine/hitboxes.rs): the boxes over `color`, the level's picture
    /// (`depth` its depth).
    pub fn hitboxes_frame(&mut self, gpu: &Gpu, enc: &mut Encoder, color: &Texture, depth: &Texture, boxes: &[super::hitboxes::Box3]) {
        self.hitboxes.poll();
        let (camera, view_proj) = from_eye(self.camera, &self.view_proj);
        self.hitboxes.frame(gpu, enc, color, depth, boxes, camera, view_proj, &self.nearest, &self.linear);
    }

    /// The world's effects after the hitboxes (frame command 136, engine/fx.rs): the shapes the modules put in the world
    /// over `color`, the level's picture (`depth` its depth).
    pub fn fx_frame(&mut self, gpu: &Gpu, enc: &mut Encoder, color: &Texture, depth: &Texture, shapes: &[super::fx::Shape], letters: Option<[(vk::ImageView, u32, u32); 2]>) {
        let (camera, view_proj) = from_eye(self.camera, &self.view_proj);
        // World Effects' weather (weather/): its particles after the modules' shapes
        let mut weather = Vec::new();
        crate::weather::frame(camera, &view_proj, self.params.sun[3], self.params.moon[3], &mut weather);
        if weather.is_empty() {
            self.fx.frame(gpu, enc, color, depth, shapes, camera, view_proj, &self.nearest, &self.linear, letters);
        } else {
            let mut all = shapes.to_vec();
            all.append(&mut weather);
            self.fx.frame(gpu, enc, color, depth, &all, camera, view_proj, &self.nearest, &self.linear, letters);
        }
    }

    /// The cosmetics' models in the hands over `color` (frame command 137, cosmetics/render.rs): what the store read
    /// goes to the GPU, the hands' state says what to draw (cosmetics/view.rs), the sun or the moon and the sky light
    /// it as the world round is lit.
    pub fn viewmodel_frame(&mut self, gpu: &Gpu, enc: &mut Encoder, color: &Texture, facts: crate::cosmetics::view::Facts) {
        self.viewmodel_uploads(gpu, enc);
        let Some(f) = crate::cosmetics::view::frame(&facts) else { return };
        // the item's finish and the gloves' worked (once their textures are here; their draws name them)
        for job in &f.finish {
            if !self.viewmodel.finished(&job.key) {
                self.viewmodel.compose(gpu, enc, job);
            }
        }
        if let Some(sv) = f.scope {
            self.viewmodel.scope(gpu, enc, color, &sv);
            return;
        }
        let p = self.params;
        let b = f.basis;
        let to_view = |v: [f32; 4]| -> [f32; 3] { std::array::from_fn(|k| b[k][0] * v[0] + b[k][1] * v[1] + b[k][2] * v[2]) };
        // the sun by day, the moon (pale, blue) by night
        let (dir, strength, tint) = if p.sun[3] >= p.moon[3] { (to_view(p.sun), p.sun[3], [p.sun_color[0], p.sun_color[1], p.sun_color[2]]) } else { (to_view(p.moon), p.moon[3], [0.55, 0.62, 0.8]) };
        let rain = p.sky[3];
        let light = crate::cosmetics::render::Light {
            sun: [dir[0], dir[1], dir[2], strength * (1.0 - 0.8 * rain)],
            sun_color: [tint[0], tint[1], tint[2], facts.sky_light],
            sky: [p.sky[0], p.sky[1], p.sky[2], facts.block_light],
            horizon: [p.sky[0] * 0.7 + 0.25, p.sky[1] * 0.7 + 0.25, p.sky[2] * 0.7 + 0.25, 1.0],
            params: [p.time, 0.0, 0.0, 0.0],
            studio: [0.0; 4],
        };
        let fins: Vec<Vec<(&str, &str)>> = f.draws.iter().map(|d| d.finish.iter().map(|(m, k)| (m.as_str(), k.as_str())).collect()).collect();
        let draws: Vec<crate::cosmetics::render::Draw> =
            f.draws.iter().zip(&fins).map(|(d, fin)| crate::cosmetics::render::Draw { model: &d.model, world: d.world, skin: &d.skin, finish: fin, stattrak: d.stattrak, stickers: &d.stickers, retex: &d.retex, look: d.look, mirror: false }).collect();
        self.viewmodel.frame(gpu, enc, color, &f.proj, &light, &draws, None, 1);
    }

    /// CS's item in the player's own hand seen from behind (F5; frame command 138, cosmetics/view.rs third): its model
    /// at `pose` (where the game holds its item, from the camera), into the level's picture with the level's depth
    /// (hidden behind what stands in front, its own depth left there for what comes after), lit as the world round is
    /// (`sky`, `block`: the light at the player, 0..1).
    #[allow(clippy::too_many_arguments)]
    pub fn third_frame(&mut self, gpu: &Gpu, enc: &mut Encoder, color: &Texture, depth: &Texture, pose: &[f32; 16], sky: f32, block: f32) {
        self.viewmodel_uploads(gpu, enc);
        let Some(f) = crate::cosmetics::view::third(pose) else { return };
        for job in &f.finish {
            if !self.viewmodel.finished(&job.key) {
                self.viewmodel.compose(gpu, enc, job);
            }
        }
        let p = self.params;
        // the sun by day, the moon (pale, blue) by night: in the world (the model is lit where it stands, from the eye)
        let (dir, strength, tint) = if p.sun[3] >= p.moon[3] { ([p.sun[0], p.sun[1], p.sun[2]], p.sun[3], [p.sun_color[0], p.sun_color[1], p.sun_color[2]]) } else { ([p.moon[0], p.moon[1], p.moon[2]], p.moon[3], [0.55, 0.62, 0.8]) };
        let rain = p.sky[3];
        let light = crate::cosmetics::render::Light {
            sun: [dir[0], dir[1], dir[2], strength * (1.0 - 0.8 * rain)],
            sun_color: [tint[0], tint[1], tint[2], sky],
            sky: [p.sky[0], p.sky[1], p.sky[2], block],
            horizon: [p.sky[0] * 0.7 + 0.25, p.sky[1] * 0.7 + 0.25, p.sky[2] * 0.7 + 0.25, 1.0],
            params: [p.time, 0.0, 0.0, 0.0],
            studio: [0.0; 4],
        };
        let (_, view_proj) = from_eye(self.camera, &self.view_proj);
        let fins: Vec<Vec<(&str, &str)>> = f.draws.iter().map(|d| d.finish.iter().map(|(m, k)| (m.as_str(), k.as_str())).collect()).collect();
        let draws: Vec<crate::cosmetics::render::Draw> =
            f.draws.iter().zip(&fins).map(|(d, fin)| crate::cosmetics::render::Draw { model: &d.model, world: d.world, skin: &d.skin, finish: fin, stattrak: d.stattrak, stickers: &d.stickers, retex: &d.retex, look: d.look, mirror: false }).collect();
        self.viewmodel.frame_world(gpu, enc, color, depth, &view_proj, &light, &draws);
    }

    /// The Cosmetics page's look at an item over the menu's picture (cosmetics/preview.rs): its model, its finish
    /// worked, lit as in a studio, in the part of the picture the page keeps for it.
    pub fn viewmodel_preview(&mut self, gpu: &Gpu, enc: &mut Encoder, out: &Texture, p: &crate::cosmetics::preview::Preview) {
        use crate::cosmetics::{preview, store};
        self.viewmodel_uploads(gpu, enc);
        if p.in_hand {
            self.viewmodel_hand_preview(gpu, enc, out, p);
            return;
        }
        let key = preview::model_key(&p.item, &p.paint);
        store::want(&key);
        // (its model still coming: the page shows that it waits)
        let Some(m) = store::model(&key) else {
            self.look_fade(&key, false);
            return;
        };
        let jobs = preview::jobs(p, &m, &key);
        for j in &jobs {
            if !self.viewmodel.finished(&j.key) {
                self.viewmodel.compose(gpu, enc, j);
            }
        }
        // (its finish too: none yet while what it is made of still comes from the server)
        let finish_due = !p.paint.is_empty() && jobs.is_empty() && store::busy();
        let ready = self.model_there(&key) && jobs.iter().all(|j| self.viewmodel.finished(&j.key)) && !finish_due;
        let fade = self.look_fade(&key, ready) * p.alpha.clamp(0.0, 1.0);
        if fade <= 0.0 {
            return;
        }
        let aspect = (p.rect[2] - p.rect[0]).max(1.0) / (p.rect[3] - p.rect[1]).max(1.0);
        let (world, skin, joints, _) = preview::world(p, &m, &key, aspect);
        // StatTrak's module, its count shown
        let knife = crate::cosmetics::catalog::get().item(&p.item).is_some_and(|i| i.kind == crate::cosmetics::catalog::Kind::Knife);
        let module = p.stattrak.and_then(|n| crate::cosmetics::view::stattrak_module(&key, &m, &joints, knife).map(|(k, at)| (k, crate::cosmetics::model::mul(&world, &at), n)));
        // the charm on its keychain, hanging, swinging as the item is turned
        let charm = p.charm.as_ref().and_then(|name| crate::cosmetics::view::charm_module(&key, &m, &joints, name, p.charm_offset)).map(|c| {
            use crate::cosmetics::{charm, model::mul};
            let held = mul(&world, &c.held);
            let (t, e) = (charm::point(&held, c.top), charm::point(&held, c.end));
            let swing = preview::charm_swing(&format!("{}|{}", p.item, c.key), t, [e[0] - t[0], e[1] - t[1], e[2] - t[2]], charm::wall(&held, c.lean));
            let skin = store::model(&c.key).map(|cm| cm.skin(&cm.rest())).unwrap_or_default();
            (c.key, mul(&swing, &held), skin)
        });
        // the menu's picture runs down from its top (the level's up from its bottom): its up the other way; framed in
        // the item's place, drawn over the whole of its look (its floor and its reflection under it)
        let mut proj = crate::cosmetics::view::perspective(preview::FOV.to_radians(), aspect, 0.01, 20.0);
        proj[5] = -proj[5];
        let frame = if p.frame[2] > p.frame[0] + 1.0 && p.frame[3] > p.frame[1] + 1.0 { p.frame } else { p.rect };
        let (fw, fh) = (frame[2] - frame[0], frame[3] - frame[1]);
        let within: crate::cosmetics::model::M4 = [
            (p.rect[2] - p.rect[0]) / fw, 0.0, 0.0, 0.0,
            0.0, (p.rect[3] - p.rect[1]) / fh, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            (p.rect[0] + p.rect[2] - frame[0] - frame[2]) / fw, (p.rect[1] + p.rect[3] - frame[1] - frame[3]) / fh, 0.0, 1.0,
        ];
        let proj = crate::cosmetics::model::mul(&within, &proj);
        // the floor under it (its height in the look's space), the item mirrored in it
        let (_, floor) = preview::ground(p, &m, &key, aspect);
        let mirror: crate::cosmetics::model::M4 = [1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 2.0 * floor, 0.0, 1.0];
        let light = self.look_light(p, fade, floor);
        let fin: Vec<(&str, &str)> = jobs.iter().map(|j| (j.material.as_str(), j.key.as_str())).collect();
        let stickers = crate::cosmetics::sticker::draws(&key, &m, &p.stickers);
        // the name tag's plate, the name written on it
        let nametag = p.nametag.as_ref().filter(|n| !n.is_empty()).and_then(|n| {
            crate::cosmetics::view::nametag_module(&key, &m, &joints).map(|at| (crate::cosmetics::model::mul(&world, &at), vec![(crate::cosmetics::nametag::MATERIAL.to_string(), crate::cosmetics::nametag::texture(n))]))
        });
        let mut draws = vec![crate::cosmetics::render::Draw { model: &key, world, skin: &skin, finish: &fin, stattrak: None, stickers: &stickers, retex: &[], look: p.overlay.map(|o| o.shader()), mirror: false }];
        if let Some((k, w, n)) = module.as_ref() {
            draws.push(crate::cosmetics::render::Draw { model: k, world: *w, skin: &[], finish: &[], stattrak: Some(*n), stickers: &[], retex: &[], look: None, mirror: false });
        }
        if let Some((k, w, skin)) = charm.as_ref() {
            draws.push(crate::cosmetics::render::Draw { model: k, world: *w, skin, finish: &[], stattrak: None, stickers: &[], retex: &[], look: None, mirror: false });
        }
        if let Some((w, retex)) = nametag.as_ref() {
            draws.push(crate::cosmetics::render::Draw { model: crate::cosmetics::nametag::MODEL, world: *w, skin: &[], finish: &[], stattrak: None, stickers: &[], retex, look: None, mirror: false });
        }
        // its reflection first (under the floor, behind it), then the item
        let mut all: Vec<crate::cosmetics::render::Draw> = draws.iter().map(|d| crate::cosmetics::render::Draw { world: crate::cosmetics::model::mul(&mirror, &d.world), stickers: &[], mirror: true, ..*d }).collect();
        all.extend(draws);
        self.viewmodel.frame(gpu, enc, out, &proj, &light, &all, Some(frame), p.ss as u32);
    }

    /// The page's look «in the hands»: CS's hands holding it as the game would (cosmetics/view.rs look_frame: hands of
    /// their own, the page's loadout — the look's item on its slot), over the whole of its place, lit as the look is.
    fn viewmodel_hand_preview(&mut self, gpu: &Gpu, enc: &mut Encoder, out: &Texture, p: &crate::cosmetics::preview::Preview) {
        use crate::cosmetics::{preview, render::Draw, view};
        let Some((l, version)) = preview::hand_loadout(p) else { return };
        let frame = if p.frame[2] > p.frame[0] + 1.0 && p.frame[3] > p.frame[1] + 1.0 { p.frame } else { p.rect };
        let aspect = (frame[2] - frame[0]).max(1.0) / (frame[3] - frame[1]).max(1.0);
        // the look turned by the pointer: the view's (the hands sway after it)
        let facts = view::Facts { yaw: p.yaw.to_degrees(), pitch: p.pitch.to_degrees(), vel: [0.0; 3], flags: view::ON_GROUND, sky_light: 1.0, block_light: 0.0, aspect };
        let key = format!("hands:{}:{}", p.item, p.paint);
        let Some(f) = view::look_frame(&l, version, &facts) else {
            self.look_fade(&key, false);
            return;
        };
        for job in &f.finish {
            if !self.viewmodel.finished(&job.key) {
                self.viewmodel.compose(gpu, enc, job);
            }
        }
        let ready = f.draws.iter().all(|d| self.model_there(&d.model)) && f.finish.iter().all(|j| self.viewmodel.finished(&j.key));
        let fade = self.look_fade(&key, ready) * p.alpha.clamp(0.0, 1.0);
        if fade <= 0.0 {
            return;
        }
        // the menu's picture runs down from its top: its up the other way; the game's view a little nearer and the
        // hands lifted into the place (it is narrower than the game's picture: no empty dark over them)
        let mut proj = f.proj;
        proj[5] = -proj[5];
        let aside = if crate::cosmetics::view::main_left(&l) { 0.08 } else { -0.08 };
        let nearer: crate::cosmetics::model::M4 = [1.2, 0.0, 0.0, 0.0, 0.0, 1.2, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, aside, -0.2, 0.0, 1.0];
        let proj = crate::cosmetics::model::mul(&nearer, &proj);
        let light = self.look_light(p, fade, 0.0);
        let fins: Vec<Vec<(&str, &str)>> = f.draws.iter().map(|d| d.finish.iter().map(|(m, k)| (m.as_str(), k.as_str())).collect()).collect();
        let draws: Vec<Draw> = f.draws.iter().zip(&fins).map(|(d, fin)| Draw { model: &d.model, world: d.world, skin: &d.skin, finish: fin, stattrak: d.stattrak, stickers: &d.stickers, retex: &d.retex, look: d.look, mirror: false }).collect();
        self.viewmodel.frame(gpu, enc, out, &proj, &light, &draws, Some(frame), p.ss as u32);
    }

    /// Whether a model is on the GPU with its materials' textures (or those that never come given up).
    fn model_there(&self, key: &str) -> bool {
        use crate::cosmetics::store;
        let Some(m) = store::model(key) else { return false };
        let there = |f: &str| {
            let t = store::texture_key(key, f);
            t.is_empty() || self.viewmodel.has_texture(&t) || store::failed(&t)
        };
        self.viewmodel.has(key) && m.materials.iter().all(|mat| there(&mat.base) && there(&mat.normal) && there(&mat.packed))
    }

    /// How far the page's look (`key` what it shows) has come in: once all of it is there (`ready`, or a few seconds on
    /// when nothing more comes from the server, half a minute when it still does: what never comes is not waited for),
    /// over a third of a second, not in pieces; said to the page too.
    fn look_fade(&mut self, key: &str, ready: bool) -> f32 {
        let now = self.started.elapsed().as_secs_f32();
        if self.preview_in.as_ref().is_none_or(|(k, ..)| k != key) {
            self.preview_in = Some((key.to_string(), now, None));
        }
        let fade = match self.preview_in.as_mut() {
            Some((_, asked, at)) => {
                let waited = now - *asked;
                if at.is_none() && (ready || (waited > 4.0 && !crate::cosmetics::store::busy()) || waited > 30.0) {
                    *at = Some(now);
                }
                at.map_or(0.0, |t| {
                    let k = ((now - t) / 0.35).clamp(0.0, 1.0);
                    k * k * (3.0 - 2.0 * k)
                })
            }
            None => 0.0,
        };
        crate::cosmetics::preview::set_seen(fade);
        fade
    }

    /// The page's look's light, a studio's: its key from where the page turned it, a cool sky, the place bright,
    /// exposed brighter than the hands in the world (`fade` how much of it is seen yet; `floor` its floor's height,
    /// under which its reflection fades).
    fn look_light(&self, p: &crate::cosmetics::preview::Preview, fade: f32, floor: f32) -> crate::cosmetics::render::Light {
        let ([sy, cy], [sp, cp]) = (p.light[0].sin_cos().into(), p.light[1].sin_cos().into());
        crate::cosmetics::render::Light {
            sun: [sy * cp, sp, cy * cp, 1.1],
            sun_color: [1.0, 0.97, 0.92, 1.0],
            sky: [0.55, 0.6, 0.68, 0.0],
            horizon: [0.62, 0.64, 0.66, 1.6],
            // the time (our materials move), how little of it is seen yet, the floor's height and how far under it the
            // reflection fades
            params: [self.params.time, 1.0 - fade, floor, 0.22],
            // a studio's: its shade lifted, a cool light from behind along its edges, its brights rolled off softly
            studio: [0.15, 0.22, 1.0, 0.0],
        }
    }

    /// What the store read since, to the GPU (the hands' frame and the page's look both take it).
    fn viewmodel_uploads(&mut self, gpu: &Gpu, enc: &mut Encoder) {
        use crate::cosmetics::store::{self, Loaded};
        for l in store::take_ready() {
            match l {
                Loaded::Model(key, m) => {
                    let k = key.clone();
                    self.viewmodel.upload_model(gpu, enc, &key, &m, &|f| store::texture_key(&k, f));
                }
                Loaded::Texture(key, img) => self.viewmodel.upload_texture(gpu, enc, &key, &img),
            }
        }
    }

    /// The level's frames put together into the main target (frame command 129).
    pub fn resolve(&mut self, gpu: &Gpu, enc: &mut Encoder, color: &Texture, depth: &Texture, out: &Texture, jitter: [f32; 2]) {
        // The light in the air goes in with this frame when the sun's reach is there.
        let air_on = self.params.flags & VOLUME != 0 && self.sunvis.is_some();
        let glow_on = self.params.flags & GLOW != 0;
        let air = match (air_on || glow_on, self.sunvis.as_ref().or(self.blank3d.as_ref()), self.ubo) {
            (true, Some((_, view)), Some((ubo, offset))) => Some(super::upscale::Air {
                sunvis: view.raw,
                linear: self.linear.raw,
                ubo: unsafe { (*ubo).raw },
                ubo_offset: offset,
                ubo_size: UBO_SIZE,
                frame: self.frames as i32,
                air: air_on,
                glow: glow_on,
            }),
            _ => None,
        };
        self.upscale.resolve(gpu, enc, color, depth, out, jitter, self.view_proj, self.camera, air);
    }

    /// The terrain's camera and matrix this frame (frame command 122).
    pub fn view(&mut self, camera: [f64; 3], view_proj: [f32; 16]) {
        self.camera = camera;
        self.view_proj = view_proj;
    }

    /// Whether opaque terrain goes depth first, then the mask, then lit once per pixel.
    pub fn prepass(&self) -> bool {
        !self.no_prepass && self.mask_pipeline.is_some() && self.ready() && self.params.flags & SHADOWS != 0
    }

    /// The shadow mask from `depth` (the opaque terrain's, laid down by the depth pass), outside any rendering.
    pub fn make_mask(&mut self, gpu: &Gpu, enc: &mut Encoder, cmd: vk::CommandBuffer, depth: &View) {
        if self.mask_pipeline.is_none() {
            return;
        }
        let t = depth.texture();
        let (w, h) = (t.width_at(depth.base_mip), t.height_at(depth.base_mip));
        if self.mask.as_ref().map_or(true, |m| m.2 != w || m.3 != h) {
            if let Some((tex, view, _, _)) = self.mask.take() {
                retire(enc, tex, view);
            }
            let (tex, view) = image_in(gpu, cmd, "Engine shadow mask", vk::Format::R32G32_SFLOAT, w.div_ceil(2), h.div_ceil(2), 1, false);
            self.mask = Some((tex, view, w, h));
        }
        let c = self.mask_pipeline.as_ref().unwrap();
        let images = self.images.as_ref().unwrap();
        let (_, mask_view, _, _) = self.mask.as_ref().unwrap();
        let (ubo, ubo_offset) = self.ubo.unwrap();
        let s = self.nearest.raw;
        let infos = [
            vk::DescriptorImageInfo { sampler: s, image_view: depth.raw, image_layout: vk::ImageLayout::GENERAL },
            vk::DescriptorImageInfo { sampler: s, image_view: images.voxels_view.raw, image_layout: vk::ImageLayout::GENERAL },
            vk::DescriptorImageInfo { sampler: s, image_view: images.coarse_view.raw, image_layout: vk::ImageLayout::GENERAL },
            vk::DescriptorImageInfo { sampler: vk::Sampler::null(), image_view: mask_view.raw, image_layout: vk::ImageLayout::GENERAL },
        ];
        let buffer = [vk::DescriptorBufferInfo { buffer: unsafe { (*ubo).raw }, offset: ubo_offset, range: UBO_SIZE }];
        let writes = [
            vk::WriteDescriptorSet::default().dst_binding(0).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(std::slice::from_ref(&infos[0])),
            vk::WriteDescriptorSet::default().dst_binding(1).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(std::slice::from_ref(&infos[1])),
            vk::WriteDescriptorSet::default().dst_binding(2).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(std::slice::from_ref(&infos[2])),
            vk::WriteDescriptorSet::default().dst_binding(3).descriptor_type(vk::DescriptorType::STORAGE_IMAGE).image_info(std::slice::from_ref(&infos[3])),
            vk::WriteDescriptorSet::default().dst_binding(4).descriptor_type(vk::DescriptorType::UNIFORM_BUFFER).buffer_info(&buffer),
        ];
        let inv = invert(&self.view_proj);
        let base = [self.camera[0].floor(), self.camera[1].floor(), self.camera[2].floor()];
        let ints = [base[0] as i32, base[1] as i32, base[2] as i32, 0];
        let frac = [(self.camera[0] - base[0]) as f32, (self.camera[1] - base[1]) as f32, (self.camera[2] - base[2]) as f32, 0.0f32];
        let size = [w as i32, h as i32, w.div_ceil(2) as i32, h.div_ceil(2) as i32];
        let mut push = [0u8; MASK_PUSH as usize];
        unsafe {
            std::ptr::copy_nonoverlapping(inv.as_ptr() as *const u8, push.as_mut_ptr(), 64);
            std::ptr::copy_nonoverlapping(ints.as_ptr() as *const u8, push.as_mut_ptr().add(64), 16);
            std::ptr::copy_nonoverlapping(frac.as_ptr() as *const u8, push.as_mut_ptr().add(80), 16);
            std::ptr::copy_nonoverlapping(size.as_ptr() as *const u8, push.as_mut_ptr().add(96), 16);
            let d = &gpu.device;
            enc.segment(gpu, cmd, "Engine", "shadow mask (compute)");
            d.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, c.pipeline);
            gpu.push_d.cmd_push_descriptor_set(cmd, vk::PipelineBindPoint::COMPUTE, c.layout, 0, &writes);
            d.cmd_push_constants(cmd, c.layout, vk::ShaderStageFlags::COMPUTE, 0, &push);
            d.cmd_dispatch(cmd, w.div_ceil(2).div_ceil(8), h.div_ceil(2).div_ceil(8), 1);
            enc.segment_end(gpu, cmd);
        }
        memory_barrier(gpu, cmd);
    }

    pub fn ready(&self) -> bool {
        self.images.is_some() && self.palette.is_some() && self.ubo.is_some()
    }

    /// One frame of the world (frame command 128): the window follows the camera, changes go to the GPU, the uniforms
    /// are written. Outside any render pass.
    pub fn frame(&mut self, gpu: &Gpu, enc: &mut Encoder, p: Params) {
        self.params = p;
        let height = (p.height.max(16) + 15) / 16 * 16;
        if self.images.as_ref().map_or(true, |i| i.height != height) {
            if let Some(old) = self.images.take() {
                retire(enc, old.voxels, old.voxels_view);
                retire(enc, old.coarse, old.coarse_view);
            }
            let (voxels, voxels_view) = image(gpu, enc, "Engine voxels", vk::Format::R16_UINT, WIDTH as u32, WIDTH as u32, height as u32, true);
            let (coarse, coarse_view) = image(gpu, enc, "Engine bricks", vk::Format::R8_UINT, (WIDTH / 4) as u32, (WIDTH / 4) as u32, (height / 4) as u32, true);
            self.images = Some(Images { voxels, voxels_view, coarse, coarse_view, height });
            self.slots = vec![None; (SECTIONS * SECTIONS * (height / 16)) as usize];
            self.complete = None;
            self.tops = vec![0; self.slots.len()];
        }
        if self.palette.is_none() {
            // Rows 0 and 1 the materials, rows 2..257 the cloud map (red), rows 258..321 the sky's table (sky_lut.comp).
            self.palette = Some(image(gpu, enc, "Engine palette", vk::Format::R32G32B32A32_SFLOAT, 256, 322, 1, false));
        }
        if self.mask_blank.is_none() {
            self.mask_blank = Some(image(gpu, enc, "Engine blank mask", vk::Format::R32G32_SFLOAT, 1, 1, 1, false));
        }
        if self.blank3d.is_none() {
            let cmd = enc.cmd(gpu);
            self.blank3d = Some(image3d(gpu, cmd, "Engine blank light", vk::Format::R8G8B8A8_UNORM, [1, 1, 1]));
        }
        let materials = voxels::materials();
        if let Some(m) = materials.as_ref() {
            if m.epoch != self.palette_epoch {
                self.upload_palette(gpu, enc, &m.palette, m.clouds.as_deref());
                self.has_clouds = m.clouds.is_some();
                self.palette_epoch = m.epoch;
            }
        }
        let store_epoch = voxels::epoch();
        let materials_epoch = materials.as_ref().map_or(0, |m| m.epoch);
        let min_section = p.min_y >> 4;
        if store_epoch != self.store_epoch || materials_epoch != self.materials_epoch || min_section != self.min_section {
            self.slots.iter_mut().for_each(|s| *s = None);
            self.complete = None;
            self.store_epoch = store_epoch;
            self.materials_epoch = materials_epoch;
            self.min_section = min_section;
        }
        // the voxels only for an engine that draws (quality off: none of it is read)
        if p.flags != 0 {
            self.fill(gpu, enc, p);
        }
        self.ink.poll_look();
        self.write_ubo(gpu, enc, p);
        // A step every other frame: light still spreads faster than the eye follows.
        self.frames += 1;
        // The light grid steps 30 times a second and the sun's reach 32 times (a sixteenth each): by the clock, not
        // by the frame, so a fast frame does not pay for them every time.
        let now = std::time::Instant::now();
        let dt = self.clock.map_or(1.0, |t| now.duration_since(t).as_secs_f64()).min(0.25);
        self.clock = Some(now);
        self.light_due = (self.light_due + dt * 30.0).min(2.0);
        self.sunvis_due = (self.sunvis_due + dt * 32.0).min(2.0);
        if p.flags & LIGHT != 0 && (self.light_due >= 1.0 || self.light.is_none()) {
            self.light_due = (self.light_due - 1.0).max(0.0);
            self.spread_light(gpu, enc, &p);
        }
        if p.flags & WATER != 0 {
            self.move_water(gpu, enc, &p);
        }
        if p.flags & VOLUME != 0 && (self.sunvis_due >= 1.0 || self.sunvis.is_none()) {
            self.sunvis_due = (self.sunvis_due - 1.0).max(0.0);
            self.sunvis_steps = self.sunvis_steps.wrapping_add(1);
            self.trace_sunvis(gpu, enc, &p);
        }
        // The sky's table again when the sun has moved (a few times a second with the day going, never when it stands).
        let sun = [p.sun[0], p.sun[1], p.sun[2]];
        let moved = self.sky_lut_sun.map_or(true, |s| (0..3).map(|k| (s[k] - sun[k]).abs()).fold(0.0f32, f32::max) > 1e-3);
        if p.flags & SKY != 0 && moved {
            self.make_sky_lut(gpu, enc);
            self.sky_lut_sun = Some(sun);
        }
    }

    /// The sky's table's compute shader, compiled by Java (lib.rs shaders_init).
    pub fn set_skylut_shader(&mut self, gpu: &Gpu, spirv: &[u32]) {
        let kinds = [vk::DescriptorType::STORAGE_IMAGE, vk::DescriptorType::UNIFORM_BUFFER];
        if let Some(old) = self.skylut_pipeline.replace(Compute::new(gpu, "engine_skylut", spirv, &kinds, 0)) {
            old.destroy(gpu);
        }
    }

    /// The sky's shape for this frame's sun into the palette's rows 258..321 (sky.glsl reads it).
    fn make_sky_lut(&mut self, gpu: &Gpu, enc: &mut Encoder) {
        let (Some(c), Some((_, palette)), Some((ubo, ubo_offset))) = (self.skylut_pipeline.as_ref(), self.palette.as_ref(), self.ubo) else { return };
        let cmd = enc.cmd(gpu);
        let image = [vk::DescriptorImageInfo { sampler: vk::Sampler::null(), image_view: palette.raw, image_layout: vk::ImageLayout::GENERAL }];
        let buffer = [vk::DescriptorBufferInfo { buffer: unsafe { (*ubo).raw }, offset: ubo_offset, range: UBO_SIZE }];
        let writes = [
            vk::WriteDescriptorSet::default().dst_binding(0).descriptor_type(vk::DescriptorType::STORAGE_IMAGE).image_info(&image),
            vk::WriteDescriptorSet::default().dst_binding(1).descriptor_type(vk::DescriptorType::UNIFORM_BUFFER).buffer_info(&buffer),
        ];
        let d = &gpu.device;
        enc.segment(gpu, cmd, "Engine", "sky table (compute)");
        unsafe {
            d.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, c.pipeline);
            gpu.push_d.cmd_push_descriptor_set(cmd, vk::PipelineBindPoint::COMPUTE, c.layout, 0, &writes);
            d.cmd_dispatch(cmd, 256 / 8, 64 / 8, 1);
        }
        memory_barrier(gpu, cmd);
        enc.segment_end(gpu, cmd);
    }

    fn upload_palette(&mut self, gpu: &Gpu, enc: &mut Encoder, palette: &[[f32; 8]], clouds: Option<&[u8]>) {
        let (tex, _) = self.palette.as_ref().unwrap();
        let bytes = 256 * 258 * 16;
        let piece = enc.transient.staging(gpu, bytes, 16, bytes, 1);
        unsafe {
            let dst = piece.host as *mut f32;
            std::ptr::write_bytes(dst, 0, 256 * 258 * 4);
            for (i, e) in palette.iter().enumerate().take(256) {
                for k in 0..4 {
                    *dst.add(i * 4 + k) = e[k];
                    *dst.add(256 * 4 + i * 4 + k) = e[4 + k];
                }
            }
            if let Some(c) = clouds {
                for (i, &v) in c.iter().enumerate().take(65536) {
                    *dst.add(2 * 256 * 4 + i * 4) = v as f32;
                }
            }
        }
        let region = vk::BufferImageCopy {
            buffer_offset: piece.offset,
            buffer_row_length: 256,
            buffer_image_height: 258,
            image_subresource: vk::ImageSubresourceLayers { aspect_mask: vk::ImageAspectFlags::COLOR, mip_level: 0, base_array_layer: 0, layer_count: 1 },
            image_offset: vk::Offset3D::default(),
            image_extent: vk::Extent3D { width: 256, height: 258, depth: 1 },
        };
        let cmd = enc.cmd(gpu);
        unsafe { gpu.device.cmd_copy_buffer_to_image(cmd, (*piece.buffer).raw, tex.image, vk::ImageLayout::GENERAL, &[region]) };
        memory_barrier(gpu, cmd);
    }

    /// The window's lowest section corner (x, z) for this camera.
    fn origin(p: &Params) -> (i32, i32) {
        let sx = (p.camera[0].floor() as i32) >> 4;
        let sz = (p.camera[2].floor() as i32) >> 4;
        (sx - SECTIONS / 2, sz - SECTIONS / 2)
    }

    fn slot(&self, key: [i32; 3]) -> Option<usize> {
        let layers = self.images.as_ref()?.height / 16;
        let ly = key[1] - self.min_section;
        if ly < 0 || ly >= layers {
            return None;
        }
        Some(((key[0].rem_euclid(SECTIONS) + key[2].rem_euclid(SECTIONS) * SECTIONS) * layers + ly) as usize)
    }

    /// Copies the sections the window lacks or that changed, nearest first, within the frame's budget.
    fn fill(&mut self, gpu: &Gpu, enc: &mut Encoder, p: Params) {
        let layers = self.images.as_ref().unwrap().height / 16;
        let (ox, oz) = Self::origin(&p);
        let inside = |k: [i32; 3]| k[0] >= ox && k[0] < ox + SECTIONS && k[2] >= oz && k[2] < oz + SECTIONS;
        let mut wanted: Vec<[i32; 3]> = Vec::new();
        let window = [ox, oz, self.min_section, layers];
        let mut changed = false;
        for k in voxels::take_dirty(4096) {
            if inside(k) {
                if let Some(s) = self.slot(k) {
                    // Changed: copied again even though the slot holds it.
                    self.slots[s] = None;
                    changed = true;
                }
            }
        }
        if !changed && self.complete == Some(window) {
            return;
        }
        for dz in 0..SECTIONS {
            for dx in 0..SECTIONS {
                for ly in 0..layers {
                    let key = [ox + dx, self.min_section + ly, oz + dz];
                    let s = self.slot(key).unwrap();
                    if self.slots[s] != Some(key) {
                        wanted.push(key);
                    }
                }
            }
        }
        if wanted.is_empty() {
            self.complete = Some(window);
            return;
        }
        self.complete = None;
        let cam = [(p.camera[0].floor() as i32) >> 4, (p.camera[1].floor() as i32) >> 4, (p.camera[2].floor() as i32) >> 4];
        let dist = |k: &[i32; 3]| (k[0] - cam[0]).pow(2) + (k[1] - cam[1]).pow(2) * 2 + (k[2] - cam[2]).pow(2);
        wanted.sort_by_key(dist);
        wanted.truncate(BUDGET);
        let images = self.images.as_ref().unwrap();
        let (vox_image, coarse_image) = (images.voxels.image, images.coarse.image);
        let cmd = enc.cmd(gpu);
        let min_y = self.min_section;
        // In pieces that fit a transient block: 48 sections of voxels (8 KiB each) and their bricks.
        for chunk in wanted.chunks(48) {
            let n = chunk.len() as u64;
            let vox_bytes = n * 8192;
            let piece = enc.transient.staging(gpu, vox_bytes + n * 64, 16, vox_bytes + n * 64, 1);
            let mut vox_regions = Vec::with_capacity(chunk.len());
            let mut coarse_regions = Vec::with_capacity(chunk.len());
            for (i, &key) in chunk.iter().enumerate() {
                let at = i as u64 * 8192;
                let brick_at = vox_bytes + i as u64 * 64;
                voxels::read(key, |s| unsafe {
                    let dst = std::slice::from_raw_parts_mut(piece.host.add(at as usize) as *mut u16, 4096);
                    let bdst = std::slice::from_raw_parts_mut(piece.host.add(brick_at as usize), 64);
                    match s {
                        Some(Section::Data(d)) => dst.copy_from_slice(&d[..]),
                        Some(Section::Uniform(v)) => dst.fill(*v),
                        // Never meshed (not loaded): open air.
                        None => dst.fill(voxels::OPEN_AIR),
                    }
                    bricks(s.unwrap_or(&Section::Uniform(voxels::OPEN_AIR)), bdst);
                });
                let top = voxels::read(key, |s| match s {
                    Some(Section::Data(d)) => (0..16).rev().find(|&y| (0..256).any(|i| voxels::blocks_rays(d[y * 256 + i] & 15))).map_or(0, |y| y as u8 + 1),
                    Some(Section::Uniform(v)) if voxels::blocks_rays(v & 15) => 16,
                    _ => 0,
                });
                let (tx, tz, ty) = (key[0].rem_euclid(SECTIONS) * 16, key[2].rem_euclid(SECTIONS) * 16, (key[1] - min_y) * 16);
                vox_regions.push(vk::BufferImageCopy {
                    buffer_offset: piece.offset + at,
                    buffer_row_length: 16,
                    buffer_image_height: 16,
                    image_subresource: vk::ImageSubresourceLayers { aspect_mask: vk::ImageAspectFlags::COLOR, mip_level: 0, base_array_layer: ty as u32, layer_count: 16 },
                    image_offset: vk::Offset3D { x: tx, y: tz, z: 0 },
                    image_extent: vk::Extent3D { width: 16, height: 16, depth: 1 },
                });
                coarse_regions.push(vk::BufferImageCopy {
                    buffer_offset: piece.offset + brick_at,
                    buffer_row_length: 4,
                    buffer_image_height: 4,
                    image_subresource: vk::ImageSubresourceLayers { aspect_mask: vk::ImageAspectFlags::COLOR, mip_level: 0, base_array_layer: (ty / 4) as u32, layer_count: 4 },
                    image_offset: vk::Offset3D { x: tx / 4, y: tz / 4, z: 0 },
                    image_extent: vk::Extent3D { width: 4, height: 4, depth: 1 },
                });
                let s = self.slot(key).unwrap();
                self.slots[s] = Some(key);
                self.tops[s] = top;
            }
            unsafe {
                let src = (*piece.buffer).raw;
                gpu.device.cmd_copy_buffer_to_image(cmd, src, vox_image, vk::ImageLayout::GENERAL, &vox_regions);
                gpu.device.cmd_copy_buffer_to_image(cmd, src, coarse_image, vk::ImageLayout::GENERAL, &coarse_regions);
            }
            self.uploaded += n;
        }
        memory_barrier(gpu, cmd);
    }

    fn write_ubo(&mut self, gpu: &Gpu, enc: &mut Encoder, p: Params) {
        let piece = enc.transient.mapped(gpu, UBO_SIZE, gpu.info.min_uniform_align.max(16) as u64, UBO_SIZE, 1);
        let (ox, oz) = Self::origin(&p);
        let mut ints = [0i32; 8];
        ints[0] = ox * 16;
        ints[1] = self.min_section * 16;
        ints[2] = oz * 16;
        ints[3] = WIDTH;
        ints[4] = WIDTH;
        ints[5] = self.images.as_ref().map_or(0, |i| i.height);
        ints[6] = p.flags;
        // The columns of the window (16x16 sections): the world y above which nothing stops a ray; and the highest.
        let layers = self.images.as_ref().map_or(1, |i| i.height / 16) as usize;
        let mut columns = [0i32; 256];
        let mut highest = self.min_section * 16;
        for c in 0..256usize {
            let (cx, cz) = (c % 16, c / 16);
            let base = (cx + cz * 16) * layers;
            let mut top = self.min_section * 16;
            for ly in (0..layers).rev() {
                let t = self.tops.get(base + ly).copied().unwrap_or(0);
                if t > 0 {
                    top = (self.min_section + ly as i32) * 16 + t as i32;
                    break;
                }
            }
            columns[c] = top;
            highest = highest.max(top);
        }
        ints[7] = highest;
        // CursedSpare.x: the frame's number (the soft shadows' spot on the sun's disc).
        // CursedTime.z: seconds of real time (an hour round), for the wind.
        let seconds = (self.started.elapsed().as_secs_f64() % 3600.0) as f32;
        // CursedTime.w the clouds' drift; CursedSpare.z their bottom from the camera's block, w 1 when they cast shadows.
        let clouds_on = p.clouds[2] > 0.5 && self.has_clouds;
        let cloud_bottom = p.clouds[1] - p.camera[1].floor() as f32;
        // CursedSpare.y: how many shadow casters (CursedEntities, after the camera).
        let floats: [[f32; 4]; 6] =
            [p.sun, p.sun_color, p.moon, p.sky, [p.time, p.scale.clamp(0.25, 1.0), seconds, p.clouds[0]], [(self.frames % 65536) as f32, p.caster_count as f32, cloud_bottom, if clouds_on { 1.0 } else { 0.0 }]];
        unsafe {
            let dst = piece.host;
            std::ptr::copy_nonoverlapping(ints.as_ptr() as *const u8, dst, 32);
            std::ptr::copy_nonoverlapping(floats.as_ptr() as *const u8, dst.add(32), 96);
            std::ptr::copy_nonoverlapping(columns.as_ptr() as *const u8, dst.add(128), 1024);
            // The camera's block and the camera from it (the creatures' shaders have no Globals of the game).
            let block = [p.camera[0].floor(), p.camera[1].floor(), p.camera[2].floor()];
            let ints = [block[0] as i32, block[1] as i32, block[2] as i32, 0];
            let frac = [(p.camera[0] - block[0]) as f32, (p.camera[1] - block[1]) as f32, (p.camera[2] - block[2]) as f32, 0.0f32];
            std::ptr::copy_nonoverlapping(ints.as_ptr() as *const u8, dst.add(1152), 16);
            std::ptr::copy_nonoverlapping(frac.as_ptr() as *const u8, dst.add(1168), 16);
            let mut casters = [[0.0f32; 4]; 64];
            for (i, c) in p.casters.iter().take(p.caster_count).enumerate() {
                casters[i * 2] = [c[0], c[1], c[2], 0.0];
                casters[i * 2 + 1] = [c[3].max(0.05), c[4].max(0.05), c[5].max(0.05), 0.0];
            }
            std::ptr::copy_nonoverlapping(casters.as_ptr() as *const u8, dst.add(1184), 1024);
            let extra = [p.wet, 0.0f32, 0.0, 0.0];
            std::ptr::copy_nonoverlapping(extra.as_ptr() as *const u8, dst.add(2208), 16);
            // the chams' body: its tint, shine and flow, and more of the look (CursedChamsTint, CursedChamsLook,
            // CursedChamsMore)
            let chams = self.ink.look.ubo();
            std::ptr::copy_nonoverlapping(chams.as_ptr() as *const u8, dst.add(2224), 96);
        }
        self.ubo = Some((piece.buffer, piece.offset));
    }

    /// What terrain binds in its engine slots (shaders: CursedVoxels, CursedBricks, CursedPalette, CursedEngine), by
    /// name; None when the name is not the engine's.
    pub fn binding(&self, name: &str) -> Option<EngineValue> {
        let s = &*self.nearest as *const Sampler;
        match name {
            "CursedVoxels" => self.images.as_ref().map(|i| EngineValue::Image(&*i.voxels_view as *const View, s)),
            "CursedBricks" => self.images.as_ref().map(|i| EngineValue::Image(&*i.coarse_view as *const View, s)),
            "CursedPalette" => self.palette.as_ref().map(|(_, v)| EngineValue::Image(&**v as *const View, s)),
            "CursedEngine" => self.ubo.map(|(b, o)| EngineValue::Buffer(b, o, UBO_SIZE)),
            "CursedLight" => self.light.as_ref().map(|g| EngineValue::Image(&*g[self.light_latest].1 as *const View, &*self.linear as *const Sampler)).or(self.blank_3d()),
            "CursedBounce" => self.light.as_ref().map(|g| EngineValue::Image(&*g[self.light_latest + 1].1 as *const View, &*self.linear as *const Sampler)).or(self.blank_3d()),
            "CursedWater" => self
                .water
                .as_ref()
                .map(|g| EngineValue::Image(&*g[self.water_latest].1 as *const View, &*self.linear as *const Sampler))
                .or(self.mask_blank.as_ref().map(|m| EngineValue::Image(&*m.1 as *const View, &*self.linear as *const Sampler))),
            "CursedScene" => self
                .scene
                .as_ref()
                .map(|s| EngineValue::Image(&*s.0 .1 as *const View, &*self.linear as *const Sampler))
                .or(self.mask_blank.as_ref().map(|m| EngineValue::Image(&*m.1 as *const View, &*self.linear as *const Sampler))),
            "CursedSceneDepth" => self
                .scene
                .as_ref()
                .map(|s| EngineValue::Image(&*s.1 .1 as *const View, &*self.nearest as *const Sampler))
                .or(self.mask_blank.as_ref().map(|m| EngineValue::Image(&*m.1 as *const View, &*self.nearest as *const Sampler))),
            "CursedMask" => self
                .mask
                .as_ref()
                .map(|m| &*m.1 as *const View)
                .or(self.mask_blank.as_ref().map(|m| &*m.1 as *const View))
                .map(|v| EngineValue::Image(v, s)),
            _ => None,
        }
    }

    fn blank_3d(&self) -> Option<EngineValue> {
        self.blank3d.as_ref().map(|b| EngineValue::Image(&*b.1 as *const View, &*self.linear as *const Sampler))
    }

    pub fn destroy(&mut self, gpu: &Gpu) {
        self.ink.destroy(gpu);
        self.ground.destroy(gpu);
        self.liquid.destroy(gpu);
        self.curse.destroy(gpu);
        self.shadow.destroy(gpu);
        self.tesseract.destroy(gpu);
        self.hitboxes.destroy(gpu);
        self.fx.destroy(gpu);
        self.viewmodel.destroy(gpu);
        let d = &gpu.device;
        let kill = |t: Box<Texture>, v: Box<View>| unsafe {
            d.destroy_image_view(v.raw, None);
            d.destroy_image(t.image, None);
            let mut t = t;
            if let Some(a) = t.alloc.take() {
                gpu.memory.free(d, a);
            }
        };
        if let Some(i) = self.images.take() {
            kill(i.voxels, i.voxels_view);
            kill(i.coarse, i.coarse_view);
        }
        if let Some((t, v)) = self.palette.take() {
            kill(t, v);
        }
        if let Some((t, v, _, _)) = self.mask.take() {
            kill(t, v);
        }
        if let Some((t, v)) = self.mask_blank.take() {
            kill(t, v);
        }
        if let Some(c) = self.mask_pipeline.take() {
            c.destroy(gpu);
        }
        self.upscale.destroy(gpu);
        if let Some(grids) = self.light.take() {
            for (t, v) in grids {
                kill(t, v);
            }
        }
        if let Some((t, v)) = self.blank3d.take() {
            kill(t, v);
        }
        if let Some(c) = self.light_pipeline.take() {
            c.destroy(gpu);
        }
        if let Some(grids) = self.water.take() {
            for (t, v) in grids {
                kill(t, v);
            }
        }
        if let Some(c) = self.water_pipeline.take() {
            c.destroy(gpu);
        }
        if let Some((t, v)) = self.sunvis.take() {
            kill(t, v);
        }
        if let Some((c, d, _, _)) = self.scene.take() {
            kill(c.0, c.1);
            kill(d.0, d.1);
        }
        if let Some(c) = self.sunvis_pipeline.take() {
            c.destroy(gpu);
        }
        unsafe { d.destroy_sampler(self.linear.raw, None) };
        unsafe { d.destroy_sampler(self.nearest.raw, None) };
    }
}

pub enum EngineValue {
    Image(*const View, *const Sampler),
    Buffer(*const Buffer, u64, u64),
}
