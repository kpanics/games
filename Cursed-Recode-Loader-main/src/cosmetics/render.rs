//! The cosmetics' models drawn in the hands (the frame command VIEWMODEL, vk/frame.rs), as CS draws its viewmodel:
//! over the level's picture, with a depth of their own (nothing of the world cuts into the hands), in a view and a
//! projection of their own. Their vertices are skinned in the vertex shader (shaders/cosmetics/viewmodel.vert), lit
//! in the fragment shader (viewmodel.frag). The models and textures go to the GPU once, when first drawn. A sniper's
//! scope is a pass of its own over the whole picture (scope.frag, CS2's own): the picture copied, seen through it.
//! Stickers are laid over their body after it, each a pass of the body's own triangles (sticker.vert, sticker.frag):
//! blended over it at its own depth.

use super::model::{Model, Part, M4, VERTEX};
use super::texture::Image;
use crate::vk::device::Gpu;
use crate::vk::encoder::{memory_barrier, Encoder};
use crate::vk::memory::Place;
use crate::vk::resources::{Buffer, Doomed, Sampler, Texture, View};
use ash::vk;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32};

/// The light round the hands: toward the sun in the view (w how much of it), its colour (w the sky's light at the
/// player, 0..1), the sky overhead (w the block light, 0..1), the horizon (w exposure), x the time; the Cosmetics
/// page's studio (none in the game: light.glsl's view.studio — its shade lifted, a light from behind along the edges,
/// its brights rolled off).
#[derive(Clone, Copy, Debug)]
pub struct Light {
    pub sun: [f32; 4],
    pub sun_color: [f32; 4],
    pub sky: [f32; 4],
    pub horizon: [f32; 4],
    pub params: [f32; 4],
    pub studio: [f32; 4],
}

impl Default for Light {
    fn default() -> Light {
        Light { sun: [0.3, 0.8, 0.5, 1.0], sun_color: [1.0, 0.96, 0.9, 1.0], sky: [0.45, 0.62, 0.95, 0.0], horizon: [0.75, 0.82, 0.92, 1.0], params: [0.0; 4], studio: [0.0; 4] }
    }
}

/// The scope over the picture: its bob (pixels), the weapon's inaccuracy, the lines' width, the gap round the middle.
#[derive(Clone, Copy, Debug, Default)]
pub struct ScopeView {
    pub bob: [f32; 2],
    pub inaccuracy: f32,
    pub line_width: u32,
    pub gap: i32,
}

/// One model to draw: which (its key), where in the view, its skin's matrices.
pub struct Draw<'a> {
    pub model: &'a str,
    pub world: M4,
    pub skin: &'a [M4],
    /// Finishes worked onto its materials: each material's name, its finish's key (cosmetics/compose.rs).
    pub finish: &'a [(&'a str, &'a str)],
    /// A StatTrak module's number (on its display's material).
    pub stattrak: Option<u32>,
    /// Stickers laid over its body.
    pub stickers: &'a [StickerDraw],
    /// Materials whose colour another texture gives (a material's name, the texture's key): a name tag's plate written.
    pub retex: &'a [(String, String)],
    /// Our material over its finish (viewmodel.frag's look: its kind, how much, its colour, linear).
    pub look: Option<(u32, f32, [f32; 3])>,
    /// Its reflection in the page's floor (its place mirrored under the floor; viewmodel.frag's bit 4).
    pub mirror: bool,
}

/// A sticker over a body (cosmetics/sticker.rs makes it): the body's materials it lies on, sticker.vert's projection
/// (its middle in the model as bound, w how far it reaches; its across and its up over its size; its normal),
/// sticker.frag's material (its switches, wear, boosts; tint; wear's bias, scratches, glitter's scale), its textures'
/// keys (the picture, normal and roughness, effects' mask, spectrum, glitter's normal, scratches).
#[derive(Clone, Debug, Default)]
pub struct StickerDraw {
    pub materials: Vec<String>,
    pub projection: [[f32; 4]; 4],
    pub flags: u32,
    pub wear: f32,
    pub color_boost: f32,
    pub sfx_boost: f32,
    pub tint: [f32; 4],
    pub wearing: [f32; 4],
    pub textures: [String; 6],
}

impl StickerDraw {
    /// Its push constants (sticker.vert's 64 bytes, then sticker.frag's).
    fn push(&self) -> Vec<u8> {
        let mut p = Vec::with_capacity(128);
        for v in self.projection.iter().flatten() {
            p.extend_from_slice(&v.to_le_bytes());
        }
        p.extend_from_slice(&self.flags.to_le_bytes());
        for v in [self.wear, self.color_boost, self.sfx_boost] {
            p.extend_from_slice(&v.to_le_bytes());
        }
        for v in self.tint.iter().chain(&self.wearing) {
            p.extend_from_slice(&v.to_le_bytes());
        }
        p
    }
}

pub(super) struct GpuTex {
    pub tex: Box<Texture>,
    pub view: Box<View>,
}

struct GpuModel {
    vb: Box<Buffer>,
    ib: Box<Buffer>,
    parts: Vec<Part>,
    /// Each material's textures (base, normal, packed): keys into the textures, "" the default.
    mats: Vec<[String; 3]>,
    /// Each material's name (a finish replaces one by it).
    names: Vec<String>,
}

struct Pipe {
    /// (the depth it tests against: ours, nearer smaller — or the level's own, the game's reversed: nearer larger)
    depth: vk::Format,
    reversed: bool,
    pipeline: vk::Pipeline,
    /// The same writing depth alone (a body's; else null).
    depth_only: vk::Pipeline,
    layout: vk::PipelineLayout,
    set_layout: vk::DescriptorSetLayout,
    format: vk::Format,
}

#[derive(Default)]
pub struct Viewmodel {
    shaders: Option<(Vec<u32>, Vec<u32>)>,
    /// When each sticker's picture was first there to draw (it comes in from then, not at once).
    appeared: HashMap<String, std::time::Instant>,
    /// One for each picture's format drawn into (the level's, the menu's): kept, never made again while in use.
    pipes: Vec<Pipe>,
    scope_shaders: Option<(Vec<u32>, Vec<u32>)>,
    /// The stickers' shaders, their pipelines (one for each picture's format), their samplers (their pictures clamped
    /// at their edges; a spectrum's rows clamped, round across).
    sticker_shaders: Option<(Vec<u32>, Vec<u32>)>,
    sticker_pipes: Vec<Pipe>,
    edge: Option<Box<Sampler>>,
    rows: Option<Box<Sampler>>,
    scope_pipe: Option<Pipe>,
    /// The picture copied for the scope to look through.
    scope_copy: Option<GpuTex>,
    /// The page's look drawn larger (frame_large): the shader bringing it down, its pipeline, the larger picture.
    resolve_fs: Option<Vec<u32>>,
    resolve_pipe: Option<Pipe>,
    large: Option<GpuTex>,
    clamp: Option<Box<Sampler>>,
    /// Our depths and the pictures' views, the last two of each (the level's and the menu's look at an item, drawn in
    /// the same frame, each keep theirs).
    depths: Vec<GpuTex>,
    color_views: Vec<(vk::Image, vk::Format, Box<View>)>,
    /// The level's depth's views (CS's item in the player's hand from behind is hidden behind what stands in front).
    depth_views: Vec<(vk::Image, vk::Format, Box<View>)>,
    models: HashMap<String, GpuModel>,
    pub(super) textures: HashMap<String, GpuTex>,
    /// White (sRGB), a flat normal, and "neither metal nor shiny, nothing occluded".
    defaults: Option<[GpuTex; 3]>,
    sampler: Option<Box<Sampler>>,
    /// Finishes (cosmetics/compose.rs): the vertex shaders the compositors run with (the weapons', the gloves'), their
    /// pipelines by kind and combo, their samplers by address modes, the results by key.
    pub(super) finish_vs: Option<Vec<u32>>,
    pub(super) glove_vs: Option<Vec<u32>>,
    pub(super) compose_pipes: HashMap<(bool, u32), super::compose::ComposePipe>,
    pub(super) finish_samplers: HashMap<(u32, u32), vk::Sampler>,
    pub(super) finishes: HashMap<String, super::compose::Finished>,
    pub(super) finish_failed: std::collections::HashSet<String>,
}

fn fail<T>(r: Result<T, vk::Result>, what: &str) -> Option<T> {
    match r {
        Ok(v) => Some(v),
        Err(e) => {
            eprintln!("[cosmetics] {what}: {e:?}");
            None
        }
    }
}

/// An image of `mips` levels in GENERAL layout, its view of them all.
fn image(gpu: &Gpu, cmd: vk::CommandBuffer, name: &str, format: vk::Format, aspect: vk::ImageAspectFlags, usage: vk::ImageUsageFlags, w: u32, h: u32, mips: u32) -> Option<GpuTex> {
    let d = &gpu.device;
    let info = vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .format(format)
        .extent(vk::Extent3D { width: w, height: h, depth: 1 })
        .mip_levels(mips)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::OPTIMAL)
        .usage(usage)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED);
    let raw = fail(unsafe { d.create_image(&info, None) }, "an image")?;
    let req = unsafe { d.get_image_memory_requirements(raw) };
    let Some(alloc) = fail(gpu.memory.allocate(d, req, Place::Device, false), "an image's memory") else {
        unsafe { d.destroy_image(raw, None) };
        return None;
    };
    fail(unsafe { d.bind_image_memory(raw, alloc.memory, alloc.offset) }, "binding an image")?;
    gpu.name(raw, name);
    let tex = Box::new(Texture {
        image: raw,
        alloc: Some(alloc),
        format,
        aspect,
        width: w,
        height: h,
        layers: 1,
        mips,
        cube: false,
        closed: AtomicBool::new(true),
        views: AtomicU32::new(1),
    });
    let range = vk::ImageSubresourceRange { aspect_mask: aspect, base_mip_level: 0, level_count: mips, base_array_layer: 0, layer_count: 1 };
    let vinfo = vk::ImageViewCreateInfo::default().image(raw).view_type(vk::ImageViewType::TYPE_2D).format(format).subresource_range(range);
    let view_raw = fail(unsafe { d.create_image_view(&vinfo, None) }, "an image view")?;
    let view = Box::new(View { raw: view_raw, texture: &*tex as *const Texture, base_mip: 0 });
    let barrier = [vk::ImageMemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
        .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
        .dst_access_mask(vk::AccessFlags2::MEMORY_READ | vk::AccessFlags2::MEMORY_WRITE)
        .old_layout(vk::ImageLayout::UNDEFINED)
        .new_layout(vk::ImageLayout::GENERAL)
        .image(raw)
        .subresource_range(range)];
    unsafe { gpu.sync2_d.cmd_pipeline_barrier2(cmd, &vk::DependencyInfo::default().image_memory_barriers(&barrier)) };
    Some(GpuTex { tex, view })
}

fn retire(enc: &mut Encoder, t: GpuTex) {
    enc.defer(Doomed::View(t.view));
    enc.defer(Doomed::Texture(t.tex));
}

impl Viewmodel {
    pub fn set_shaders(&mut self, vertex: &[u32], fragment: &[u32]) {
        self.shaders = Some((vertex.to_vec(), fragment.to_vec()));
    }

    pub fn set_scope_shaders(&mut self, vertex: &[u32], fragment: &[u32]) {
        self.scope_shaders = Some((vertex.to_vec(), fragment.to_vec()));
    }

    pub fn set_resolve_shader(&mut self, fragment: &[u32]) {
        self.resolve_fs = Some(fragment.to_vec());
    }

    pub fn set_sticker_shaders(&mut self, vertex: &[u32], fragment: &[u32]) {
        self.sticker_shaders = Some((vertex.to_vec(), fragment.to_vec()));
    }

    /// Whether a texture is on the GPU.
    pub fn has_texture(&self, key: &str) -> bool {
        self.textures.contains_key(key)
    }

    pub fn has(&self, key: &str) -> bool {
        self.models.contains_key(key)
    }

    /// A texture to the GPU (kept by `key`; again when it is there already does nothing).
    pub fn upload_texture(&mut self, gpu: &Gpu, enc: &mut Encoder, key: &str, img: &Image) {
        if self.textures.contains_key(key) || img.mips.is_empty() {
            return;
        }
        if let Some(t) = Self::texture(gpu, enc, key, img) {
            self.textures.insert(key.to_string(), t);
        }
    }

    fn texture(gpu: &Gpu, enc: &mut Encoder, name: &str, img: &Image) -> Option<GpuTex> {
        let format = if img.wide {
            vk::Format::R16G16B16A16_UNORM
        } else if img.srgb {
            vk::Format::R8G8B8A8_SRGB
        } else {
            vk::Format::R8G8B8A8_UNORM
        };
        let total: usize = img.mips.iter().map(|m| m.len()).sum();
        let piece = enc.transient.staging(gpu, total as u64, 16, total as u64, 1);
        let cmd = enc.cmd(gpu);
        let t = image(gpu, cmd, name, format, vk::ImageAspectFlags::COLOR, vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST, img.width, img.height, img.mips.len() as u32)?;
        let mut regions = Vec::with_capacity(img.mips.len());
        let mut at = 0u64;
        let (mut w, mut h) = (img.width, img.height);
        for (k, m) in img.mips.iter().enumerate() {
            unsafe { std::ptr::copy_nonoverlapping(m.as_ptr(), piece.host.add(at as usize), m.len()) };
            regions.push(vk::BufferImageCopy {
                buffer_offset: piece.offset + at,
                buffer_row_length: 0,
                buffer_image_height: 0,
                image_subresource: vk::ImageSubresourceLayers { aspect_mask: vk::ImageAspectFlags::COLOR, mip_level: k as u32, base_array_layer: 0, layer_count: 1 },
                image_offset: vk::Offset3D::default(),
                image_extent: vk::Extent3D { width: w, height: h, depth: 1 },
            });
            at += m.len() as u64;
            w = (w / 2).max(1);
            h = (h / 2).max(1);
        }
        unsafe { gpu.device.cmd_copy_buffer_to_image(cmd, (*piece.buffer).raw, t.tex.image, vk::ImageLayout::GENERAL, &regions) };
        memory_barrier(gpu, cmd);
        Some(t)
    }

    /// A model to the GPU (kept by `key`): its vertices and indices, its materials' texture keys (`textures` maps a
    /// material's file names to keys already uploaded; one missing reads as the default).
    pub fn upload_model(&mut self, gpu: &Gpu, enc: &mut Encoder, key: &str, model: &Model, textures: &dyn Fn(&str) -> String) {
        if self.models.contains_key(key) || model.indices.is_empty() {
            return;
        }
        let vb_size = model.vertices.len() as u64;
        let ib_size = (model.indices.len() * 4) as u64;
        let Some(vb) = fail(Buffer::with(gpu, vk::BufferUsageFlags::VERTEX_BUFFER | vk::BufferUsageFlags::TRANSFER_DST, vb_size, Place::Device), "a model's vertices") else { return };
        let Some(ib) = fail(Buffer::with(gpu, vk::BufferUsageFlags::INDEX_BUFFER | vk::BufferUsageFlags::TRANSFER_DST, ib_size, Place::Device), "a model's indices") else {
            enc.defer(Doomed::Buffer(Box::new(vb)));
            return;
        };
        let piece = enc.transient.staging(gpu, vb_size + ib_size, 16, vb_size + ib_size, 1);
        unsafe {
            std::ptr::copy_nonoverlapping(model.vertices.as_ptr(), piece.host, model.vertices.len());
            std::ptr::copy_nonoverlapping(model.indices.as_ptr() as *const u8, piece.host.add(vb_size as usize), ib_size as usize);
        }
        let cmd = enc.cmd(gpu);
        unsafe {
            let src = (*piece.buffer).raw;
            gpu.device.cmd_copy_buffer(cmd, src, vb.raw, &[vk::BufferCopy { src_offset: piece.offset, dst_offset: 0, size: vb_size }]);
            gpu.device.cmd_copy_buffer(cmd, src, ib.raw, &[vk::BufferCopy { src_offset: piece.offset + vb_size, dst_offset: 0, size: ib_size }]);
        }
        memory_barrier(gpu, cmd);
        let mats = model.materials.iter().map(|m| [textures(&m.base), textures(&m.normal), textures(&m.packed)]).collect();
        let names = model.materials.iter().map(|m| m.name.clone()).collect();
        self.models.insert(key.to_string(), GpuModel { vb: Box::new(vb), ib: Box::new(ib), parts: model.parts.clone(), mats, names });
    }

    pub(super) fn ensure_defaults_pub(&mut self, gpu: &Gpu, enc: &mut Encoder) -> bool {
        self.ensure_defaults(gpu, enc)
    }

    /// A default texture's view (0 white, 1 a flat normal, 2 plain).
    pub(super) fn default_view(&self, k: usize) -> vk::ImageView {
        self.defaults.as_ref().map_or(vk::ImageView::null(), |d| d[k.min(2)].view.raw)
    }

    fn ensure_defaults(&mut self, gpu: &Gpu, enc: &mut Encoder) -> bool {
        if self.defaults.is_none() {
            let w = Self::texture(gpu, enc, "cosmetics white", &Image::solid([255, 255, 255, 255], true));
            let n = Self::texture(gpu, enc, "cosmetics flat", &Image::solid([128, 128, 255, 255], false));
            let p = Self::texture(gpu, enc, "cosmetics plain", &Image::solid([255, 128, 0, 255], false));
            if let (Some(w), Some(n), Some(p)) = (w, n, p) {
                self.defaults = Some([w, n, p]);
            }
        }
        if self.sampler.is_none() {
            self.sampler = Sampler::new(gpu, 0, 0, 1, 1, 8, None).ok().map(Box::new);
        }
        if self.edge.is_none() {
            self.edge = Sampler::new(gpu, 1, 1, 1, 1, 8, None).ok().map(Box::new);
        }
        if self.rows.is_none() {
            self.rows = Sampler::new(gpu, 0, 1, 1, 1, 1, None).ok().map(Box::new);
        }
        self.defaults.is_some() && self.sampler.is_some() && self.edge.is_some() && self.rows.is_some()
    }

    /// The pipeline for a picture's format (made the first time).
    /// (`level`: the level's depth's format — tested as the game's, reversed — else ours)
    fn ensure_pipe(&mut self, gpu: &Gpu, format: vk::Format, level: Option<vk::Format>) -> Option<usize> {
        let depth = level.unwrap_or(vk::Format::D32_SFLOAT);
        if let Some(i) = self.pipes.iter().position(|p| p.format == format && p.depth == depth && p.reversed == level.is_some()) {
            return Some(i);
        }
        self.pipes.push(self.make_pipe(gpu, format, false, level)?);
        Some(self.pipes.len() - 1)
    }

    /// The stickers' pipeline for a picture's format (made the first time).
    fn ensure_sticker_pipe(&mut self, gpu: &Gpu, format: vk::Format, level: Option<vk::Format>) -> Option<usize> {
        let depth = level.unwrap_or(vk::Format::D32_SFLOAT);
        if let Some(i) = self.sticker_pipes.iter().position(|p| p.format == format && p.depth == depth && p.reversed == level.is_some()) {
            return Some(i);
        }
        self.sticker_pipes.push(self.make_pipe(gpu, format, true, level)?);
        Some(self.sticker_pipes.len() - 1)
    }

    /// A model's pipeline, or (`sticker`) a sticker's: seven textures, its projection and material pushed (the vertex
    /// shader's and the fragment shader's), blended over the body at its depth, which it leaves as it is. Against our
    /// own depth (nearer smaller), or (`level`, its format) the level's own (the game's reversed: nearer larger).
    fn make_pipe(&self, gpu: &Gpu, format: vk::Format, sticker: bool, level: Option<vk::Format>) -> Option<Pipe> {
        let depth_format = level.unwrap_or(vk::Format::D32_SFLOAT);
        let compare = if level.is_some() { vk::CompareOp::GREATER_OR_EQUAL } else { vk::CompareOp::LESS_OR_EQUAL };
        let (vs, fs) = if sticker { self.sticker_shaders.as_ref()? } else { self.shaders.as_ref()? };
        let d = &gpu.device;
        let module = |code: &[u32]| unsafe { d.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(code), None) }.ok();
        let (vm, fm) = (module(vs)?, module(fs)?);
        let sampled = |b: u32| vk::DescriptorSetLayoutBinding::default().binding(b).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).descriptor_count(1).stage_flags(vk::ShaderStageFlags::FRAGMENT);
        let mut bindings = vec![
            vk::DescriptorSetLayoutBinding::default().binding(0).descriptor_type(vk::DescriptorType::UNIFORM_BUFFER).descriptor_count(1).stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default().binding(1).descriptor_type(vk::DescriptorType::STORAGE_BUFFER).descriptor_count(1).stage_flags(vk::ShaderStageFlags::VERTEX),
        ];
        bindings.extend((2..if sticker { 9 } else { 6 }).map(sampled));
        let set_layout = unsafe { d.create_descriptor_set_layout(&vk::DescriptorSetLayoutCreateInfo::default().flags(vk::DescriptorSetLayoutCreateFlags::PUSH_DESCRIPTOR_KHR).bindings(&bindings), None) }.ok()?;
        let set_layouts = [set_layout];
        // a material's flags (bit 0: its packed texture is a finish's, CS2's: roughness r, metalness g); a sticker's
        // projection and material
        let push = [if sticker {
            vk::PushConstantRange { stage_flags: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, offset: 0, size: 128 }
        } else {
            vk::PushConstantRange { stage_flags: vk::ShaderStageFlags::FRAGMENT, offset: 0, size: 32 }
        }];
        let layout = unsafe { d.create_pipeline_layout(&vk::PipelineLayoutCreateInfo::default().set_layouts(&set_layouts).push_constant_ranges(&push), None) }.ok()?;
        let entry = c"main";
        let stages = [
            vk::PipelineShaderStageCreateInfo::default().stage(vk::ShaderStageFlags::VERTEX).module(vm).name(entry),
            vk::PipelineShaderStageCreateInfo::default().stage(vk::ShaderStageFlags::FRAGMENT).module(fm).name(entry),
        ];
        let vbind = [vk::VertexInputBindingDescription { binding: 0, stride: VERTEX as u32, input_rate: vk::VertexInputRate::VERTEX }];
        let attr = |location: u32, format: vk::Format, offset: u32| vk::VertexInputAttributeDescription { location, binding: 0, format, offset };
        let attrs = [
            attr(0, vk::Format::R32G32B32_SFLOAT, 0),
            attr(1, vk::Format::R32G32B32_SFLOAT, 12),
            attr(2, vk::Format::R32G32B32A32_SFLOAT, 24),
            attr(3, vk::Format::R32G32_SFLOAT, 40),
            attr(4, vk::Format::R8G8B8A8_UINT, 48),
            attr(5, vk::Format::R8G8B8A8_UNORM, 52),
        ];
        let vertex_input = vk::PipelineVertexInputStateCreateInfo::default().vertex_binding_descriptions(&vbind).vertex_attribute_descriptions(&attrs);
        let assembly = vk::PipelineInputAssemblyStateCreateInfo::default().topology(vk::PrimitiveTopology::TRIANGLE_LIST);
        // both faces: a model's thin parts (a blade's edge, a glove's cuff) are seen from either side
        let raster = vk::PipelineRasterizationStateCreateInfo::default().polygon_mode(vk::PolygonMode::FILL).cull_mode(vk::CullModeFlags::NONE).line_width(1.0);
        let depth = vk::PipelineDepthStencilStateCreateInfo::default().depth_test_enable(true).depth_write_enable(!sticker).depth_compare_op(compare);
        let blends = [if sticker {
            // over the body by its alpha; the picture's own alpha left as the body made it
            vk::PipelineColorBlendAttachmentState::default()
                .color_write_mask(vk::ColorComponentFlags::R | vk::ColorComponentFlags::G | vk::ColorComponentFlags::B)
                .blend_enable(true)
                .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
                .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
                .color_blend_op(vk::BlendOp::ADD)
                .src_alpha_blend_factor(vk::BlendFactor::ZERO)
                .dst_alpha_blend_factor(vk::BlendFactor::ONE)
                .alpha_blend_op(vk::BlendOp::ADD)
        } else {
            // whole (the page's look coming in: by its alpha)
            vk::PipelineColorBlendAttachmentState::default()
                .color_write_mask(vk::ColorComponentFlags::RGBA)
                .blend_enable(true)
                .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
                .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
                .color_blend_op(vk::BlendOp::ADD)
                .src_alpha_blend_factor(vk::BlendFactor::ONE)
                .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
                .alpha_blend_op(vk::BlendOp::ADD)
        }];
        let blend = vk::PipelineColorBlendStateCreateInfo::default().attachments(&blends);
        let viewport = vk::PipelineViewportStateCreateInfo::default().viewport_count(1).scissor_count(1);
        let multisample = vk::PipelineMultisampleStateCreateInfo::default().rasterization_samples(vk::SampleCountFlags::TYPE_1);
        let dynamic_states = [vk::DynamicState::SCISSOR, vk::DynamicState::VIEWPORT];
        let dynamic = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);
        let formats = [format];
        let mut rendering = vk::PipelineRenderingCreateInfo::default().color_attachment_formats(&formats).depth_attachment_format(depth_format);
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
        // a body's depth alone: a reflection's nearest laid first, its faint colour then laid once (not seen through
        // itself)
        let depth_only = if sticker {
            None
        } else {
            let blends = [vk::PipelineColorBlendAttachmentState::default().color_write_mask(vk::ColorComponentFlags::empty())];
            let blend = vk::PipelineColorBlendStateCreateInfo::default().attachments(&blends);
            let mut rendering = vk::PipelineRenderingCreateInfo::default().color_attachment_formats(&formats).depth_attachment_format(depth_format);
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
            unsafe { d.create_graphics_pipelines(vk::PipelineCache::null(), &info, None) }.map(|v| v[0]).ok()
        };
        unsafe {
            d.destroy_shader_module(vm, None);
            d.destroy_shader_module(fm, None);
        }
        let Some(pipeline) = pipeline else {
            unsafe {
                if let Some(p) = depth_only {
                    d.destroy_pipeline(p, None);
                }
                d.destroy_pipeline_layout(layout, None);
                d.destroy_descriptor_set_layout(set_layout, None);
            }
            return None;
        };
        gpu.name(pipeline, if sticker { "Pipeline cursed:sticker" } else { "Pipeline cursed:viewmodel" });
        Some(Pipe { depth: depth_format, reversed: level.is_some(), pipeline, depth_only: depth_only.unwrap_or(vk::Pipeline::null()), layout, set_layout, format })
    }

    /// The scope's pipeline: no vertices (one triangle over all), no depth, the picture overwritten.
    /// A pass over a part of the picture (one triangle past its corners, scope.vert): the scope's (its uniforms, the
    /// picture copied) or the resolve's (`resolve`: the larger picture and where it goes, laid over what is there by
    /// how much it covers).
    fn make_screen_pipe(&self, gpu: &Gpu, format: vk::Format, resolve: bool) -> Option<Pipe> {
        let vs = &self.scope_shaders.as_ref()?.0;
        let fs = if resolve { self.resolve_fs.as_ref()? } else { &self.scope_shaders.as_ref()?.1 };
        let d = &gpu.device;
        let module = |code: &[u32]| unsafe { d.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(code), None) }.ok();
        let (vm, fm) = (module(vs)?, module(fs)?);
        let sampled = |b: u32| vk::DescriptorSetLayoutBinding::default().binding(b).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).descriptor_count(1).stage_flags(vk::ShaderStageFlags::FRAGMENT);
        let bindings = if resolve {
            vec![sampled(0)]
        } else {
            vec![vk::DescriptorSetLayoutBinding::default().binding(0).descriptor_type(vk::DescriptorType::UNIFORM_BUFFER).descriptor_count(1).stage_flags(vk::ShaderStageFlags::FRAGMENT), sampled(1)]
        };
        let set_layout = unsafe { d.create_descriptor_set_layout(&vk::DescriptorSetLayoutCreateInfo::default().flags(vk::DescriptorSetLayoutCreateFlags::PUSH_DESCRIPTOR_KHR).bindings(&bindings), None) }.ok()?;
        let set_layouts = [set_layout];
        let push = [vk::PushConstantRange { stage_flags: vk::ShaderStageFlags::FRAGMENT, offset: 0, size: 16 }];
        let mut layout_info = vk::PipelineLayoutCreateInfo::default().set_layouts(&set_layouts);
        if resolve {
            layout_info = layout_info.push_constant_ranges(&push);
        }
        let layout = unsafe { d.create_pipeline_layout(&layout_info, None) }.ok()?;
        let entry = c"main";
        let stages = [
            vk::PipelineShaderStageCreateInfo::default().stage(vk::ShaderStageFlags::VERTEX).module(vm).name(entry),
            vk::PipelineShaderStageCreateInfo::default().stage(vk::ShaderStageFlags::FRAGMENT).module(fm).name(entry),
        ];
        let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();
        let assembly = vk::PipelineInputAssemblyStateCreateInfo::default().topology(vk::PrimitiveTopology::TRIANGLE_LIST);
        let raster = vk::PipelineRasterizationStateCreateInfo::default().polygon_mode(vk::PolygonMode::FILL).cull_mode(vk::CullModeFlags::NONE).line_width(1.0);
        let depth = vk::PipelineDepthStencilStateCreateInfo::default();
        let blends = [if resolve {
            // (it comes premultiplied by how much it covers)
            vk::PipelineColorBlendAttachmentState::default()
                .color_write_mask(vk::ColorComponentFlags::RGBA)
                .blend_enable(true)
                .src_color_blend_factor(vk::BlendFactor::ONE)
                .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
                .color_blend_op(vk::BlendOp::ADD)
                .src_alpha_blend_factor(vk::BlendFactor::ONE)
                .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
                .alpha_blend_op(vk::BlendOp::ADD)
        } else {
            vk::PipelineColorBlendAttachmentState::default().color_write_mask(vk::ColorComponentFlags::RGBA).blend_enable(false)
        }];
        let blend = vk::PipelineColorBlendStateCreateInfo::default().attachments(&blends);
        let viewport = vk::PipelineViewportStateCreateInfo::default().viewport_count(1).scissor_count(1);
        let multisample = vk::PipelineMultisampleStateCreateInfo::default().rasterization_samples(vk::SampleCountFlags::TYPE_1);
        let dynamic_states = [vk::DynamicState::SCISSOR, vk::DynamicState::VIEWPORT];
        let dynamic = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);
        let formats = [format];
        let mut rendering = vk::PipelineRenderingCreateInfo::default().color_attachment_formats(&formats);
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
        let Some(pipeline) = pipeline else {
            unsafe {
                d.destroy_pipeline_layout(layout, None);
                d.destroy_descriptor_set_layout(set_layout, None);
            }
            return None;
        };
        gpu.name(pipeline, if resolve { "Pipeline cursed:resolve" } else { "Pipeline cursed:scope" });
        Some(Pipe { depth: vk::Format::UNDEFINED, reversed: false, pipeline, depth_only: vk::Pipeline::null(), layout, set_layout, format })
    }

    /// A view of the picture to draw into (made again when the picture is another).
    fn color_view(&mut self, gpu: &Gpu, enc: &mut Encoder, color: &Texture) -> Option<vk::ImageView> {
        if let Some((_, _, v)) = self.color_views.iter().find(|(i, f, _)| *i == color.image && *f == color.format) {
            return Some(v.raw);
        }
        if self.color_views.len() >= 2 {
            let (_, _, v) = self.color_views.remove(0);
            enc.defer(Doomed::View(v));
        }
        let range = vk::ImageSubresourceRange { aspect_mask: vk::ImageAspectFlags::COLOR, base_mip_level: 0, level_count: 1, base_array_layer: 0, layer_count: 1 };
        let vinfo = vk::ImageViewCreateInfo::default().image(color.image).view_type(vk::ImageViewType::TYPE_2D).format(color.format).subresource_range(range);
        let raw = fail(unsafe { gpu.device.create_image_view(&vinfo, None) }, "the picture's view")?;
        // (a view of an image the game owns: its texture pointer is not ours to keep, the view only)
        self.color_views.push((color.image, color.format, Box::new(View { raw, texture: color as *const Texture, base_mip: 0 })));
        Some(raw)
    }

    /// A view of the level's depth to test against (made again when it is another).
    fn depth_view(&mut self, gpu: &Gpu, enc: &mut Encoder, depth: &Texture) -> Option<vk::ImageView> {
        if let Some((_, _, v)) = self.depth_views.iter().find(|(i, f, _)| *i == depth.image && *f == depth.format) {
            return Some(v.raw);
        }
        if self.depth_views.len() >= 2 {
            let (_, _, v) = self.depth_views.remove(0);
            enc.defer(Doomed::View(v));
        }
        let range = vk::ImageSubresourceRange { aspect_mask: vk::ImageAspectFlags::DEPTH, base_mip_level: 0, level_count: 1, base_array_layer: 0, layer_count: 1 };
        let vinfo = vk::ImageViewCreateInfo::default().image(depth.image).view_type(vk::ImageViewType::TYPE_2D).format(depth.format).subresource_range(range);
        let raw = fail(unsafe { gpu.device.create_image_view(&vinfo, None) }, "the level's depth's view")?;
        self.depth_views.push((depth.image, depth.format, Box::new(View { raw, texture: depth as *const Texture, base_mip: 0 })));
        Some(raw)
    }

    /// The draws into the level's picture `color` with the level's own depth (`depth`, the game's reversed: far at 0),
    /// `proj` the level's own from the eye: CS's item in the player's hand seen from behind (F5), hidden behind what
    /// stands in front of it.
    #[allow(clippy::too_many_arguments)]
    pub fn frame_world(&mut self, gpu: &Gpu, enc: &mut Encoder, color: &Texture, depth: &Texture, proj: &M4, light: &Light, draws: &[Draw]) {
        if draws.is_empty() || self.shaders.is_none() || !self.ensure_defaults(gpu, enc) || depth.width != color.width || depth.height != color.height {
            return;
        }
        let Some(cview) = self.color_view(gpu, enc, color) else { return };
        let Some(dview) = self.depth_view(gpu, enc, depth) else { return };
        let (w, h) = (color.width.max(1), color.height.max(1));
        self.pass(gpu, enc, cview, color.format, [w, h], [0.0, 0.0, w as f32, h as f32], false, proj, light, draws, Some((dview, depth.format)));
    }

    /// The scope over `color` (GENERAL layout): the picture copied, then seen through CS2's lens into it.
    pub fn scope(&mut self, gpu: &Gpu, enc: &mut Encoder, color: &Texture, sv: &ScopeView) {
        if self.scope_shaders.is_none() {
            return;
        }
        if self.scope_pipe.as_ref().is_none_or(|p| p.format != color.format) {
            if let Some(old) = self.scope_pipe.take() {
                Self::destroy_pipe(gpu, old);
            }
            self.scope_pipe = self.make_screen_pipe(gpu, color.format, false);
        }
        if self.clamp.is_none() {
            self.clamp = Sampler::new(gpu, 1, 1, 1, 1, 1, Some(0.0)).ok().map(Box::new);
        }
        let (w, h) = (color.width.max(1), color.height.max(1));
        let cmd = enc.cmd(gpu);
        if self.scope_copy.as_ref().is_none_or(|c| c.tex.width != w || c.tex.height != h || c.tex.format != color.format) {
            if let Some(old) = self.scope_copy.take() {
                retire(enc, old);
            }
            self.scope_copy = image(gpu, cmd, "cosmetics scope", color.format, vk::ImageAspectFlags::COLOR, vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST, w, h, 1);
        }
        let Some(cview) = self.color_view(gpu, enc, color) else { return };
        let (Some(pipe), Some(copy), Some(clamp)) = (self.scope_pipe.as_ref(), self.scope_copy.as_ref(), self.clamp.as_ref()) else { return };
        let layers = vk::ImageSubresourceLayers { aspect_mask: vk::ImageAspectFlags::COLOR, mip_level: 0, base_array_layer: 0, layer_count: 1 };
        let region = vk::ImageCopy { src_subresource: layers, src_offset: vk::Offset3D::default(), dst_subresource: layers, dst_offset: vk::Offset3D::default(), extent: vk::Extent3D { width: w, height: h, depth: 1 } };
        unsafe { gpu.device.cmd_copy_image(cmd, color.image, vk::ImageLayout::GENERAL, copy.tex.image, vk::ImageLayout::GENERAL, &[region]) };
        memory_barrier(gpu, cmd);
        let mut u = Vec::with_capacity(32);
        for v in [w as f32, h as f32, sv.bob[0], sv.bob[1], sv.inaccuracy] {
            u.extend_from_slice(&v.to_le_bytes());
        }
        u.extend_from_slice(&sv.line_width.to_le_bytes());
        u.extend_from_slice(&sv.gap.to_le_bytes());
        u.extend_from_slice(&0f32.to_le_bytes());
        let ubo = enc.transient.mapped(gpu, u.len() as u64, 256, u.len() as u64, 1);
        unsafe { std::ptr::copy_nonoverlapping(u.as_ptr(), ubo.host, u.len()) };
        let ubo_info = [vk::DescriptorBufferInfo { buffer: unsafe { (*ubo.buffer).raw }, offset: ubo.offset, range: u.len() as u64 }];
        let img = [vk::DescriptorImageInfo { sampler: clamp.raw, image_view: copy.view.raw, image_layout: vk::ImageLayout::GENERAL }];
        let writes = [
            vk::WriteDescriptorSet::default().dst_binding(0).descriptor_type(vk::DescriptorType::UNIFORM_BUFFER).buffer_info(&ubo_info),
            vk::WriteDescriptorSet::default().dst_binding(1).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(&img),
        ];
        let color_att = [vk::RenderingAttachmentInfo::default().image_view(cview).image_layout(vk::ImageLayout::GENERAL).load_op(vk::AttachmentLoadOp::DONT_CARE).store_op(vk::AttachmentStoreOp::STORE)];
        let area = vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent: vk::Extent2D { width: w, height: h } };
        let info = vk::RenderingInfo::default().render_area(area).layer_count(1).color_attachments(&color_att);
        unsafe {
            gpu.render_d.cmd_begin_rendering(cmd, &info);
            gpu.device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pipe.pipeline);
            gpu.device.cmd_set_viewport(cmd, 0, &[vk::Viewport { x: 0.0, y: 0.0, width: w as f32, height: h as f32, min_depth: 0.0, max_depth: 1.0 }]);
            gpu.device.cmd_set_scissor(cmd, 0, &[area]);
            gpu.push_d.cmd_push_descriptor_set(cmd, vk::PipelineBindPoint::GRAPHICS, pipe.layout, 0, &writes);
            gpu.device.cmd_draw(cmd, 3, 1, 0, 0);
            gpu.render_d.cmd_end_rendering(cmd);
        }
        memory_barrier(gpu, cmd);
    }

    fn destroy_pipe(gpu: &Gpu, p: Pipe) {
        unsafe {
            gpu.device.destroy_pipeline(p.pipeline, None);
            if p.depth_only != vk::Pipeline::null() {
                gpu.device.destroy_pipeline(p.depth_only, None);
            }
            gpu.device.destroy_pipeline_layout(p.layout, None);
            gpu.device.destroy_descriptor_set_layout(p.set_layout, None);
        }
    }

    /// The draws over `color` (the level's picture, GENERAL layout) with `proj` (the hands' own projection) and
    /// `light`, in its part `viewport` (all of it without); `ss` times larger first, then brought down, when more than
    /// 1 (the Cosmetics page's look). Models not uploaded yet are passed over.
    #[allow(clippy::too_many_arguments)]
    pub fn frame(&mut self, gpu: &Gpu, enc: &mut Encoder, color: &Texture, proj: &M4, light: &Light, draws: &[Draw], viewport: Option<[f32; 4]>, ss: u32) {
        if draws.is_empty() || self.shaders.is_none() || !self.ensure_defaults(gpu, enc) {
            return;
        }
        if let Some(vp) = viewport.filter(|_| ss > 1) {
            if self.frame_large(gpu, enc, color, proj, light, draws, vp, ss) {
                return;
            }
        }
        let Some(cview) = self.color_view(gpu, enc, color) else { return };
        let (w, h) = (color.width.max(1), color.height.max(1));
        self.pass(gpu, enc, cview, color.format, [w, h], viewport.unwrap_or([0.0, 0.0, w as f32, h as f32]), false, proj, light, draws, None);
    }

    /// The draws `ss` times larger into a picture of their own (the part `vp` of `color` to whole pixels, that many
    /// times across and down: no more than 4096), over nothing; then brought down into that part — each pixel the mean
    /// of its own ss × ss, in light (resolve.frag), laid over the picture as much as they cover it. False when it
    /// cannot be (its shader not here yet, the part too small): they are drawn as they are then.
    #[allow(clippy::too_many_arguments)]
    fn frame_large(&mut self, gpu: &Gpu, enc: &mut Encoder, color: &Texture, proj: &M4, light: &Light, draws: &[Draw], vp: [f32; 4], ss: u32) -> bool {
        if self.resolve_fs.is_none() || self.scope_shaders.is_none() {
            return false;
        }
        let (w, h) = (color.width.max(1), color.height.max(1));
        let x0 = vp[0].round().clamp(0.0, w as f32) as u32;
        let y0 = vp[1].round().clamp(0.0, h as f32) as u32;
        let x1 = (vp[2].round().clamp(0.0, w as f32) as u32).max(x0);
        let y1 = (vp[3].round().clamp(0.0, h as f32) as u32).max(y0);
        let (pw, ph) = (x1 - x0, y1 - y0);
        let ss = ss.min(4096 / pw.max(ph).max(1));
        if pw < 2 || ph < 2 || ss < 2 {
            return false;
        }
        let (bw, bh) = (pw * ss, ph * ss);
        if self.resolve_pipe.as_ref().is_none_or(|p| p.format != color.format) {
            if let Some(old) = self.resolve_pipe.take() {
                Self::destroy_pipe(gpu, old);
            }
            self.resolve_pipe = self.make_screen_pipe(gpu, color.format, true);
        }
        if self.clamp.is_none() {
            self.clamp = Sampler::new(gpu, 1, 1, 1, 1, 1, Some(0.0)).ok().map(Box::new);
        }
        if self.large.as_ref().is_none_or(|t| t.tex.width != bw || t.tex.height != bh || t.tex.format != color.format) {
            if let Some(old) = self.large.take() {
                retire(enc, old);
            }
            let cmd = enc.cmd(gpu);
            self.large = image(gpu, cmd, "cosmetics large", color.format, vk::ImageAspectFlags::COLOR, vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED, bw, bh, 1);
        }
        let Some(target) = self.large.as_ref().map(|t| t.view.raw) else { return false };
        if self.resolve_pipe.is_none() || self.clamp.is_none() {
            return false;
        }
        // (the projection frames `vp`: the larger picture is all of that part)
        self.pass(gpu, enc, target, color.format, [bw, bh], [0.0, 0.0, bw as f32, bh as f32], true, proj, light, draws, None);
        let Some(cview) = self.color_view(gpu, enc, color) else { return true };
        let (Some(pipe), Some(large), Some(clamp)) = (self.resolve_pipe.as_ref(), self.large.as_ref(), self.clamp.as_ref()) else { return true };
        let cmd = enc.cmd(gpu);
        let img = [vk::DescriptorImageInfo { sampler: clamp.raw, image_view: large.view.raw, image_layout: vk::ImageLayout::GENERAL }];
        let writes = [vk::WriteDescriptorSet::default().dst_binding(0).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(&img)];
        let mut push = Vec::with_capacity(16);
        for v in [x0 as i32, y0 as i32, ss as i32, 0] {
            push.extend_from_slice(&v.to_le_bytes());
        }
        let color_att = [vk::RenderingAttachmentInfo::default().image_view(cview).image_layout(vk::ImageLayout::GENERAL).load_op(vk::AttachmentLoadOp::LOAD).store_op(vk::AttachmentStoreOp::STORE)];
        let area = vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent: vk::Extent2D { width: w, height: h } };
        let part = vk::Rect2D { offset: vk::Offset2D { x: x0 as i32, y: y0 as i32 }, extent: vk::Extent2D { width: pw, height: ph } };
        let info = vk::RenderingInfo::default().render_area(area).layer_count(1).color_attachments(&color_att);
        unsafe {
            gpu.render_d.cmd_begin_rendering(cmd, &info);
            gpu.device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pipe.pipeline);
            gpu.device.cmd_set_viewport(cmd, 0, &[vk::Viewport { x: x0 as f32, y: y0 as f32, width: pw as f32, height: ph as f32, min_depth: 0.0, max_depth: 1.0 }]);
            gpu.device.cmd_set_scissor(cmd, 0, &[part]);
            gpu.push_d.cmd_push_descriptor_set(cmd, vk::PipelineBindPoint::GRAPHICS, pipe.layout, 0, &writes);
            gpu.device.cmd_push_constants(cmd, pipe.layout, vk::ShaderStageFlags::FRAGMENT, 0, &push);
            gpu.device.cmd_draw(cmd, 3, 1, 0, 0);
            gpu.render_d.cmd_end_rendering(cmd);
        }
        memory_barrier(gpu, cmd);
        true
    }

    /// The draws into `target` (a picture `size` big of `format`, GENERAL layout) in its part `vp`: over what it holds,
    /// or over nothing (`clear`); against our own depth, or the level's (`level`: its view and format, kept).
    #[allow(clippy::too_many_arguments)]
    fn pass(&mut self, gpu: &Gpu, enc: &mut Encoder, target: vk::ImageView, format: vk::Format, [w, h]: [u32; 2], vp: [f32; 4], clear: bool, proj: &M4, light: &Light, draws: &[Draw], level: Option<(vk::ImageView, vk::Format)>) {
        let Some(pi) = self.ensure_pipe(gpu, format, level.map(|l| l.1)) else { return };
        let si = if draws.iter().any(|d| !d.stickers.is_empty()) { self.ensure_sticker_pipe(gpu, format, level.map(|l| l.1)) } else { None };
        let cmd = enc.cmd(gpu);
        // our depth, the size of the picture
        if level.is_none() && !self.depths.iter().any(|d| d.tex.width == w && d.tex.height == h) {
            if self.depths.len() >= 2 {
                let old = self.depths.remove(0);
                retire(enc, old);
            }
            let Some(d) = image(gpu, cmd, "cosmetics depth", vk::Format::D32_SFLOAT, vk::ImageAspectFlags::DEPTH, vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT, w, h, 1) else { return };
            self.depths.push(d);
        }
        let depth_view = match level {
            Some((v, _)) => v,
            None => match self.depths.iter().find(|d| d.tex.width == w && d.tex.height == h) {
                Some(d) => d.view.raw,
                None => return,
            },
        };
        let pipe = &self.pipes[pi];
        let defaults = self.defaults.as_ref().unwrap();
        let sampler = self.sampler.as_ref().unwrap().raw;
        // the view's uniforms
        let mut u = Vec::with_capacity(144);
        proj.iter().for_each(|v| u.extend_from_slice(&v.to_le_bytes()));
        for v in [light.sun, light.sun_color, light.sky, light.horizon, light.params, light.studio] {
            v.iter().for_each(|x| u.extend_from_slice(&x.to_le_bytes()));
        }
        let ubo = enc.transient.mapped(gpu, u.len() as u64, 256, u.len() as u64, 1);
        unsafe { std::ptr::copy_nonoverlapping(u.as_ptr(), ubo.host, u.len()) };
        let ubo_info = [vk::DescriptorBufferInfo { buffer: unsafe { (*ubo.buffer).raw }, offset: ubo.offset, range: u.len() as u64 }];
        let color_att = [vk::RenderingAttachmentInfo::default()
            .image_view(target)
            .image_layout(vk::ImageLayout::GENERAL)
            .load_op(if clear { vk::AttachmentLoadOp::CLEAR } else { vk::AttachmentLoadOp::LOAD })
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue { color: vk::ClearColorValue { float32: [0.0; 4] } })];
        let depth_att = vk::RenderingAttachmentInfo::default()
            .image_view(depth_view)
            .image_layout(vk::ImageLayout::GENERAL)
            .load_op(if level.is_some() { vk::AttachmentLoadOp::LOAD } else { vk::AttachmentLoadOp::CLEAR })
            .store_op(if level.is_some() { vk::AttachmentStoreOp::STORE } else { vk::AttachmentStoreOp::DONT_CARE })
            .clear_value(vk::ClearValue { depth_stencil: vk::ClearDepthStencilValue { depth: 1.0, stencil: 0 } });
        let area = vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent: vk::Extent2D { width: w, height: h } };
        let info = vk::RenderingInfo::default().render_area(area).layer_count(1).color_attachments(&color_att).depth_attachment(&depth_att);
        // a part of the picture (the Cosmetics page's look at an item), or all of it
        let x0 = vp[0].clamp(0.0, w as f32);
        let y0 = vp[1].clamp(0.0, h as f32);
        let scissor = vk::Rect2D {
            offset: vk::Offset2D { x: x0 as i32, y: y0 as i32 },
            extent: vk::Extent2D { width: (vp[2].min(w as f32) - x0).max(1.0) as u32, height: (vp[3].min(h as f32) - y0).max(1.0) as u32 },
        };
        unsafe {
            gpu.render_d.cmd_begin_rendering(cmd, &info);
            gpu.device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pipe.pipeline);
            gpu.device.cmd_set_viewport(cmd, 0, &[vk::Viewport { x: vp[0], y: vp[1], width: (vp[2] - vp[0]).max(1.0), height: (vp[3] - vp[1]).max(1.0), min_depth: 0.0, max_depth: 1.0 }]);
            gpu.device.cmd_set_scissor(cmd, 0, &[scissor]);
        }
        // the reflections' depth first, then their colour (each laid once, its nearest), then the rest
        let mirrored = draws.iter().any(|d| d.mirror) && pipe.depth_only != vk::Pipeline::null();
        let first = draws.iter().filter(|d| mirrored && d.mirror).map(|d| (d, true));
        let order: Vec<(&Draw, bool)> = first.chain(draws.iter().map(|d| (d, false))).collect();
        let mut depth_bound = false;
        for (draw, depth_pass) in order {
            let Some(m) = self.models.get(draw.model) else { continue };
            if depth_pass != depth_bound {
                unsafe { gpu.device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, if depth_pass { pipe.depth_only } else { pipe.pipeline }) };
                depth_bound = depth_pass;
            }
            // the pose: the model's place, then each joint's skin matrix
            let bytes = 64 * (1 + draw.skin.len().max(1));
            let pose = enc.transient.mapped(gpu, bytes as u64, 256, bytes as u64, 1);
            unsafe {
                let dst = pose.host as *mut f32;
                std::ptr::copy_nonoverlapping(draw.world.as_ptr(), dst, 16);
                if draw.skin.is_empty() {
                    std::ptr::copy_nonoverlapping(super::model::IDENTITY.as_ptr(), dst.add(16), 16);
                }
                for (k, s) in draw.skin.iter().enumerate() {
                    std::ptr::copy_nonoverlapping(s.as_ptr(), dst.add(16 + k * 16), 16);
                }
            }
            let pose_info = [vk::DescriptorBufferInfo { buffer: unsafe { (*pose.buffer).raw }, offset: pose.offset, range: bytes as u64 }];
            unsafe {
                gpu.device.cmd_bind_vertex_buffers(cmd, 0, &[m.vb.raw], &[0]);
                gpu.device.cmd_bind_index_buffer(cmd, m.ib.raw, 0, vk::IndexType::UINT32);
            }
            for part in &m.parts {
                let mat = m.mats.get(part.material as usize);
                let tex = |k: usize| -> vk::ImageView {
                    mat.and_then(|mm| self.textures.get(&mm[k])).map_or(defaults[k].view.raw, |t| t.view.raw)
                };
                // a finish worked onto this material: a weapon's colour and CS2's roughness and metalness (the occlusion
                // its own); a glove's colour, normal (hemi-octahedron, roughness), metalness and cloth, occlusion
                let name = m.names.get(part.material as usize);
                let fin = draw.finish.iter().find(|(mat, _)| name.is_some_and(|n| n == mat)).and_then(|(_, k)| self.finishes.get(*k));
                let read = |t: &Option<(GpuTex, Box<View>)>, or: vk::ImageView| t.as_ref().map_or(or, |(_, r)| r.raw);
                let (mut views, flags) = match fin {
                    Some(f) if f.normal.is_some() => ([f.color.view.raw, read(&f.normal, tex(1)), read(&f.metal, tex(2)), read(&f.ao, tex(2))], 2u32),
                    Some(f) => ([f.color.view.raw, tex(1), read(&f.metal, tex(2)), tex(2)], 1u32),
                    None => ([tex(0), tex(1), tex(2), tex(2)], 0u32),
                };
                if let Some(t) = draw.retex.iter().find(|(mat, _)| name.is_some_and(|n| n == mat)).and_then(|(_, k)| self.textures.get(k)) {
                    views[0] = t.view.raw;
                }
                // StatTrak's display: its number's digits out of the atlas, lit by themselves
                let shows = draw.stattrak.filter(|_| name.is_some_and(|n| n.contains("display")));
                let knife = shows.is_some() && name.is_some_and(|n| n.contains("knife"));
                let flags = flags | if shows.is_some() { 4 } else { 0 } | if knife { 8 } else { 0 } | if draw.mirror { 16 } else { 0 };
                let mut push = flags.to_le_bytes().to_vec();
                push.extend_from_slice(&shows.unwrap_or(0).to_le_bytes());
                // our material over the finish (the item's own parts; a StatTrak display keeps its digits)
                let (look, strength, colour) = draw.look.filter(|_| shows.is_none()).unwrap_or((0, 0.0, [0.0; 3]));
                push.extend_from_slice(&look.to_le_bytes());
                push.extend_from_slice(&strength.to_le_bytes());
                for v in colour.iter().chain(&[0.0]) {
                    push.extend_from_slice(&v.to_le_bytes());
                }
                let imgs = [0, 1, 2, 3].map(|k| [vk::DescriptorImageInfo { sampler, image_view: views[k], image_layout: vk::ImageLayout::GENERAL }]);
                let writes = [
                    vk::WriteDescriptorSet::default().dst_binding(0).descriptor_type(vk::DescriptorType::UNIFORM_BUFFER).buffer_info(&ubo_info),
                    vk::WriteDescriptorSet::default().dst_binding(1).descriptor_type(vk::DescriptorType::STORAGE_BUFFER).buffer_info(&pose_info),
                    vk::WriteDescriptorSet::default().dst_binding(2).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(&imgs[0]),
                    vk::WriteDescriptorSet::default().dst_binding(3).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(&imgs[1]),
                    vk::WriteDescriptorSet::default().dst_binding(4).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(&imgs[2]),
                    vk::WriteDescriptorSet::default().dst_binding(5).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(&imgs[3]),
                ];
                unsafe {
                    gpu.push_d.cmd_push_descriptor_set(cmd, vk::PipelineBindPoint::GRAPHICS, pipe.layout, 0, &writes);
                    gpu.device.cmd_push_constants(cmd, pipe.layout, vk::ShaderStageFlags::FRAGMENT, 0, &push);
                    gpu.device.cmd_draw_indexed(cmd, part.count, 1, part.first, 0, 0);
                }
            }
            // its stickers over the body, each over the parts of the materials it lies on (once its picture is here)
            let Some(sp) = si.map(|i| &self.sticker_pipes[i]).filter(|_| !depth_pass) else { continue };
            let mut bound = false;
            for st in draw.stickers {
                let Some(picture) = self.textures.get(&st.textures[0]) else { continue };
                if !bound {
                    unsafe { gpu.device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, sp.pipeline) };
                    bound = true;
                }
                let tex = |k: usize, or: usize| self.textures.get(&st.textures[k]).map_or(defaults[or].view.raw, |t| t.view.raw);
                let (edge, rows) = (self.edge.as_ref().unwrap().raw, self.rows.as_ref().unwrap().raw);
                let views = [(picture.view.raw, edge), (tex(1, 1), edge), (tex(2, 2), edge), (tex(3, 0), sampler), (tex(4, 1), sampler), (tex(5, 0), sampler), (tex(3, 0), rows)];
                let imgs = views.map(|(v, s)| [vk::DescriptorImageInfo { sampler: s, image_view: v, image_layout: vk::ImageLayout::GENERAL }]);
                let mut writes = vec![
                    vk::WriteDescriptorSet::default().dst_binding(0).descriptor_type(vk::DescriptorType::UNIFORM_BUFFER).buffer_info(&ubo_info),
                    vk::WriteDescriptorSet::default().dst_binding(1).descriptor_type(vk::DescriptorType::STORAGE_BUFFER).buffer_info(&pose_info),
                ];
                for (k, img) in imgs.iter().enumerate() {
                    writes.push(vk::WriteDescriptorSet::default().dst_binding(2 + k as u32).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(img));
                }
                // (coming in over a third of a second from when its picture was first there)
                let since = self.appeared.entry(st.textures[0].clone()).or_insert_with(std::time::Instant::now).elapsed().as_secs_f32();
                let seen = (since / 0.35).clamp(0.0, 1.0);
                let mut push = st.push();
                push.extend_from_slice(&(seen * seen * (3.0 - 2.0 * seen)).to_le_bytes());
                for part in &m.parts {
                    if !m.names.get(part.material as usize).is_some_and(|n| st.materials.iter().any(|x| x == n)) {
                        continue;
                    }
                    unsafe {
                        gpu.push_d.cmd_push_descriptor_set(cmd, vk::PipelineBindPoint::GRAPHICS, sp.layout, 0, &writes);
                        gpu.device.cmd_push_constants(cmd, sp.layout, vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, 0, &push);
                        gpu.device.cmd_draw_indexed(cmd, part.count, 1, part.first, 0, 0);
                    }
                }
            }
            if bound {
                unsafe { gpu.device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pipe.pipeline) };
            }
        }
        unsafe { gpu.render_d.cmd_end_rendering(cmd) };
        memory_barrier(gpu, cmd);
    }

    pub fn destroy(&mut self, gpu: &Gpu) {
        self.destroy_compose(gpu);
        for p in self.pipes.drain(..).chain(self.sticker_pipes.drain(..)).chain(self.scope_pipe.take()).chain(self.resolve_pipe.take()) {
            Self::destroy_pipe(gpu, p);
        }
        let d = &gpu.device;
        let kill_tex = |t: GpuTex| unsafe {
            d.destroy_image_view(t.view.raw, None);
            let mut tex = t.tex;
            d.destroy_image(tex.image, None);
            if let Some(a) = tex.alloc.take() {
                gpu.memory.free(d, a);
            }
        };
        for (_, t) in self.textures.drain() {
            kill_tex(t);
        }
        if let Some(ds) = self.defaults.take() {
            for t in ds {
                kill_tex(t);
            }
        }
        for t in self.depths.drain(..).chain(self.scope_copy.take()).chain(self.large.take()) {
            kill_tex(t);
        }
        for (_, _, v) in self.color_views.drain(..).chain(self.depth_views.drain(..)) {
            unsafe { d.destroy_image_view(v.raw, None) };
        }
        for (_, m) in self.models.drain() {
            for mut b in [m.vb, m.ib] {
                unsafe { d.destroy_buffer(b.raw, None) };
                if let Some(a) = b.alloc.take() {
                    gpu.memory.free(d, a);
                }
            }
        }
        for s in [self.sampler.take(), self.clamp.take(), self.edge.take(), self.rows.take()].into_iter().flatten() {
            unsafe { d.destroy_sampler(s.raw, None) };
        }
    }
}
