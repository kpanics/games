//! A finish worked onto a weapon or a pair of gloves on the GPU (cosmetics/finish.rs and glove.rs put its numbers
//! together): CS2's own compositor — its pixel shader's SPIR-V as it is, its constants where it reads them, its
//! textures and samplers at its bindings (set 1, as CS2 binds them) — run over the item's texture space once for each
//! texture it makes: a weapon's colour, then its roughness and metalness (F_ROUGHNESS_MODE; shaders/cosmetics/
//! finish.vert); a glove's colour, metalness, normal and occlusion (F_OUTPUT_MODE; shaders/cosmetics/glove_finish.vert
//! with CS2's vertex constants at set 1 binding 1). The colour is sRGB; the others are written through an sRGB view as
//! CS2 does and read back as they were meant; each with its mips. The results are textures of the viewmodel's; a
//! draw names the materials it is worked onto and the finishes (render.rs Draw::finish).

use super::finish::Combo;
use super::render::{GpuTex, Viewmodel};
use crate::vk::device::Gpu;
use crate::vk::encoder::{memory_barrier, Encoder};
use crate::vk::memory::Place;
use crate::vk::resources::{Doomed, Texture, View};
use ash::vk;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::Arc;

/// What a pass makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Out {
    Color,
    /// A weapon's roughness (r) and metalness (g); a glove's metalness (g) and cloth (b).
    Metal,
    /// A glove's normal (its hemi-octahedron in rg) and roughness (b, a).
    Normal,
    Ao,
}

/// The vertex shader a pass runs: the weapons' (its transforms as push constants) or the gloves' (CS2's vertex
/// constants, a buffer of their own).
#[derive(Clone, Debug)]
pub enum Vertex {
    Weapon([[f32; 4]; 8]),
    Glove(Vec<u8>),
}

/// One texture a finish makes: CS2's compositor's combo and what it is to be given.
pub struct Pass {
    pub out: Out,
    pub combo: u32,
    pub spirv: Arc<Vec<u32>>,
    pub info: Arc<Combo>,
    pub consts: Vec<u8>,
    /// (binding, texture key — none: the default)
    pub textures: Vec<(u32, Option<String>)>,
    /// The pattern's sampler's address modes by slot (CS2's RsTextureAddressMode).
    pub address: HashMap<u32, (u32, u32)>,
    pub vertex: Vertex,
}

/// A finish to work: where its results go, what it is for, its passes.
pub struct Job {
    pub key: String,
    pub model: String,
    pub material: String,
    pub passes: Vec<Pass>,
    pub size: u32,
}

pub(super) struct ComposePipe {
    pipeline: vk::Pipeline,
    layout: vk::PipelineLayout,
    empty: vk::DescriptorSetLayout,
    set_layout: vk::DescriptorSetLayout,
}

/// A finish's results: its colour, and what else it made, each with the view that reads it as it was meant.
pub(super) struct Finished {
    pub color: GpuTex,
    pub metal: Option<(GpuTex, Box<View>)>,
    pub normal: Option<(GpuTex, Box<View>)>,
    pub ao: Option<(GpuTex, Box<View>)>,
}

fn address(m: u32) -> vk::SamplerAddressMode {
    match m {
        1 => vk::SamplerAddressMode::MIRRORED_REPEAT,
        2 => vk::SamplerAddressMode::CLAMP_TO_EDGE,
        3 => vk::SamplerAddressMode::CLAMP_TO_BORDER,
        4 => vk::SamplerAddressMode::MIRROR_CLAMP_TO_EDGE,
        _ => vk::SamplerAddressMode::REPEAT,
    }
}

/// A target of `mips` levels (GENERAL), one view per format asked (the first the one rendered into, its first level).
fn target(gpu: &Gpu, cmd: vk::CommandBuffer, name: &str, formats: &[vk::Format], size: u32, mips: u32) -> Option<(Box<Texture>, Vec<Box<View>>, vk::ImageView)> {
    let d = &gpu.device;
    let mutable = formats.len() > 1;
    let info = vk::ImageCreateInfo::default()
        .flags(if mutable { vk::ImageCreateFlags::MUTABLE_FORMAT } else { vk::ImageCreateFlags::empty() })
        .image_type(vk::ImageType::TYPE_2D)
        .format(formats[0])
        .extent(vk::Extent3D { width: size, height: size, depth: 1 })
        .mip_levels(mips)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::OPTIMAL)
        .usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_SRC | vk::ImageUsageFlags::TRANSFER_DST)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED);
    let raw = unsafe { d.create_image(&info, None) }.ok()?;
    let req = unsafe { d.get_image_memory_requirements(raw) };
    let Ok(alloc) = gpu.memory.allocate(d, req, Place::Device, false) else {
        unsafe { d.destroy_image(raw, None) };
        return None;
    };
    unsafe { d.bind_image_memory(raw, alloc.memory, alloc.offset) }.ok()?;
    gpu.name(raw, name);
    let tex = Box::new(Texture {
        image: raw,
        alloc: Some(alloc),
        format: formats[0],
        aspect: vk::ImageAspectFlags::COLOR,
        width: size,
        height: size,
        layers: 1,
        mips,
        cube: false,
        closed: AtomicBool::new(true),
        views: AtomicU32::new(1),
    });
    let view = |format: vk::Format, levels: u32| -> Option<vk::ImageView> {
        let range = vk::ImageSubresourceRange { aspect_mask: vk::ImageAspectFlags::COLOR, base_mip_level: 0, level_count: levels, base_array_layer: 0, layer_count: 1 };
        unsafe { d.create_image_view(&vk::ImageViewCreateInfo::default().image(raw).view_type(vk::ImageViewType::TYPE_2D).format(format).subresource_range(range), None) }.ok()
    };
    let views: Vec<Box<View>> = formats.iter().filter_map(|f| view(*f, mips)).map(|raw_view| Box::new(View { raw: raw_view, texture: &*tex as *const Texture, base_mip: 0 })).collect();
    let draw = view(formats[0], 1)?;
    let range = vk::ImageSubresourceRange { aspect_mask: vk::ImageAspectFlags::COLOR, base_mip_level: 0, level_count: mips, base_array_layer: 0, layer_count: 1 };
    let barrier = [vk::ImageMemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
        .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
        .dst_access_mask(vk::AccessFlags2::MEMORY_READ | vk::AccessFlags2::MEMORY_WRITE)
        .old_layout(vk::ImageLayout::UNDEFINED)
        .new_layout(vk::ImageLayout::GENERAL)
        .image(raw)
        .subresource_range(range)];
    unsafe { gpu.sync2_d.cmd_pipeline_barrier2(cmd, &vk::DependencyInfo::default().image_memory_barriers(&barrier)) };
    Some((tex, views, draw))
}

/// A SPIR-V module's entry point's name (CS2's pixel shaders' is "MainPs").
fn entry_name(code: &[u32]) -> Option<std::ffi::CString> {
    let mut i = 5;
    while i < code.len() {
        let (op, n) = (code[i] & 0xFFFF, (code[i] >> 16) as usize);
        if n == 0 {
            return None;
        }
        if op == 15 && i + 3 < code.len() {
            let bytes: Vec<u8> = code[i + 3..(i + n).min(code.len())].iter().flat_map(|w| w.to_le_bytes()).take_while(|b| *b != 0).collect();
            return std::ffi::CString::new(bytes).ok();
        }
        i += n;
    }
    None
}

/// The mips of a target made from its first level, each from the one before (linear filtering).
fn mips(gpu: &Gpu, cmd: vk::CommandBuffer, image: vk::Image, size: u32, levels: u32) {
    let mut s = size as i32;
    for l in 1..levels {
        let n = (s / 2).max(1);
        let layers = |m: u32| vk::ImageSubresourceLayers { aspect_mask: vk::ImageAspectFlags::COLOR, mip_level: m, base_array_layer: 0, layer_count: 1 };
        let blit = vk::ImageBlit {
            src_subresource: layers(l - 1),
            src_offsets: [vk::Offset3D::default(), vk::Offset3D { x: s, y: s, z: 1 }],
            dst_subresource: layers(l),
            dst_offsets: [vk::Offset3D::default(), vk::Offset3D { x: n, y: n, z: 1 }],
        };
        unsafe { gpu.device.cmd_blit_image(cmd, image, vk::ImageLayout::GENERAL, image, vk::ImageLayout::GENERAL, &[blit], vk::Filter::LINEAR) };
        memory_barrier(gpu, cmd);
        s = n;
    }
}

/// A compositor pipeline's key: the gloves' or the weapons', its combo.
fn pipe_key(p: &Pass) -> (bool, u32) {
    (matches!(p.vertex, Vertex::Glove(_)), p.combo)
}

impl Viewmodel {
    pub fn set_finish_vs(&mut self, vertex: &[u32]) {
        self.finish_vs = Some(vertex.to_vec());
    }

    pub fn set_glove_vs(&mut self, vertex: &[u32]) {
        self.glove_vs = Some(vertex.to_vec());
    }

    /// Whether a finish is settled (by its key): its results there, or it could not be worked.
    pub fn finished(&self, key: &str) -> bool {
        self.finishes.contains_key(key) || self.finish_failed.contains(key)
    }

    /// Whether a finish's results are there.
    pub fn has_finish(&self, key: &str) -> bool {
        self.finishes.contains_key(key)
    }

    fn compose_pipe(&mut self, gpu: &Gpu, pass: &Pass) -> Option<()> {
        let key = pipe_key(pass);
        if self.compose_pipes.contains_key(&key) {
            return Some(());
        }
        let glove = key.0;
        let vs = if glove { self.glove_vs.as_ref()? } else { self.finish_vs.as_ref()? };
        let d = &gpu.device;
        let module = |code: &[u32]| unsafe { d.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(code), None) }.ok();
        let (vm, fm) = (module(vs)?, module(&pass.spirv)?);
        let fragment = vk::ShaderStageFlags::FRAGMENT;
        let mut bindings = vec![vk::DescriptorSetLayoutBinding::default().binding(0).descriptor_type(vk::DescriptorType::UNIFORM_BUFFER).descriptor_count(1).stage_flags(fragment)];
        if glove {
            bindings.push(vk::DescriptorSetLayoutBinding::default().binding(1).descriptor_type(vk::DescriptorType::UNIFORM_BUFFER).descriptor_count(1).stage_flags(vk::ShaderStageFlags::VERTEX));
        }
        for (_, b) in &pass.info.textures {
            if !bindings.iter().any(|x| x.binding == *b) {
                bindings.push(vk::DescriptorSetLayoutBinding::default().binding(*b).descriptor_type(vk::DescriptorType::SAMPLED_IMAGE).descriptor_count(1).stage_flags(fragment));
            }
        }
        for (_, b) in &pass.info.samplers {
            bindings.push(vk::DescriptorSetLayoutBinding::default().binding(*b).descriptor_type(vk::DescriptorType::SAMPLER).descriptor_count(1).stage_flags(fragment));
        }
        let set_layout = unsafe { d.create_descriptor_set_layout(&vk::DescriptorSetLayoutCreateInfo::default().flags(vk::DescriptorSetLayoutCreateFlags::PUSH_DESCRIPTOR_KHR).bindings(&bindings), None) }.ok()?;
        let empty = unsafe { d.create_descriptor_set_layout(&vk::DescriptorSetLayoutCreateInfo::default(), None) }.ok()?;
        let sets = [empty, set_layout];
        let push = [vk::PushConstantRange { stage_flags: vk::ShaderStageFlags::VERTEX, offset: 0, size: 128 }];
        let layout = unsafe { d.create_pipeline_layout(&vk::PipelineLayoutCreateInfo::default().set_layouts(&sets).push_constant_ranges(&push), None) }.ok()?;
        let ps_entry = entry_name(&pass.spirv).unwrap_or_else(|| c"main".to_owned());
        let stages = [
            vk::PipelineShaderStageCreateInfo::default().stage(vk::ShaderStageFlags::VERTEX).module(vm).name(c"main"),
            vk::PipelineShaderStageCreateInfo::default().stage(fragment).module(fm).name(&ps_entry),
        ];
        let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();
        let assembly = vk::PipelineInputAssemblyStateCreateInfo::default().topology(vk::PrimitiveTopology::TRIANGLE_LIST);
        let raster = vk::PipelineRasterizationStateCreateInfo::default().polygon_mode(vk::PolygonMode::FILL).cull_mode(vk::CullModeFlags::NONE).line_width(1.0);
        let depth = vk::PipelineDepthStencilStateCreateInfo::default();
        let blends = [vk::PipelineColorBlendAttachmentState::default().color_write_mask(vk::ColorComponentFlags::RGBA).blend_enable(false)];
        let blend = vk::PipelineColorBlendStateCreateInfo::default().attachments(&blends);
        let viewport = vk::PipelineViewportStateCreateInfo::default().viewport_count(1).scissor_count(1);
        let multisample = vk::PipelineMultisampleStateCreateInfo::default().rasterization_samples(vk::SampleCountFlags::TYPE_1);
        let dynamic_states = [vk::DynamicState::SCISSOR, vk::DynamicState::VIEWPORT];
        let dynamic = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);
        let formats = [vk::Format::R8G8B8A8_SRGB];
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
            eprintln!("[cosmetics] CS2's compositor ({} combo {}) did not make a pipeline", if glove { "gloves" } else { "weapons" }, pass.combo);
            unsafe {
                d.destroy_pipeline_layout(layout, None);
                d.destroy_descriptor_set_layout(set_layout, None);
                d.destroy_descriptor_set_layout(empty, None);
            }
            return None;
        };
        gpu.name(pipeline, "Pipeline cursed:finish");
        self.compose_pipes.insert(key, ComposePipe { pipeline, layout, empty, set_layout });
        Some(())
    }

    fn finish_sampler(&mut self, gpu: &Gpu, u: u32, v: u32) -> Option<vk::Sampler> {
        if let Some(s) = self.finish_samplers.get(&(u, v)) {
            return Some(*s);
        }
        let info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
            .address_mode_u(address(u))
            .address_mode_v(address(v))
            .address_mode_w(vk::SamplerAddressMode::REPEAT)
            .max_lod(16.0)
            .anisotropy_enable(true)
            .max_anisotropy(8.0)
            .border_color(vk::BorderColor::FLOAT_TRANSPARENT_BLACK);
        let s = unsafe { gpu.device.create_sampler(&info, None) }.ok()?;
        self.finish_samplers.insert((u, v), s);
        Some(s)
    }

    /// A pass's textures' views, once each is on the GPU (one that failed to load: the default); none yet: wait.
    fn pass_views(&mut self, gpu: &Gpu, enc: &mut Encoder, pass: &Pass) -> Option<Vec<(u32, vk::ImageView)>> {
        let mut views: Vec<(u32, vk::ImageView)> = Vec::new();
        for (b, key) in &pass.textures {
            let name = pass.info.textures.iter().find(|(_, x)| x == b).map_or("", |(n, _)| n.as_str());
            let default = if name.contains("Normal") || name == "g_tSurface" { 1 } else { 0 };
            let view = match key {
                Some(k) => match self.textures.get(k) {
                    Some(t) => t.view.raw,
                    None if super::store::failed(k) => {
                        self.ensure_defaults_pub(gpu, enc);
                        self.default_view(default)
                    }
                    None => return None,
                },
                None => self.default_view(default),
            };
            if !views.iter().any(|(x, _)| x == b) {
                views.push((*b, view));
            }
        }
        Some(views)
    }

    /// A finish worked (once its textures are on the GPU): true when it is done or cannot be; false to try again.
    pub fn compose(&mut self, gpu: &Gpu, enc: &mut Encoder, job: &Job) -> bool {
        if self.finishes.contains_key(&job.key) {
            return true;
        }
        let glove = job.passes.iter().any(|p| matches!(p.vertex, Vertex::Glove(_)));
        let vs_ready = if glove { self.glove_vs.is_some() } else { self.finish_vs.is_some() };
        if !crate::vk::device::SCALAR_BLOCKS.load(std::sync::atomic::Ordering::Acquire) || !vs_ready || !self.ensure_defaults_pub(gpu, enc) {
            return vs_ready;
        }
        let mut all_views = Vec::new();
        for p in &job.passes {
            match self.pass_views(gpu, enc, p) {
                Some(v) => all_views.push(v),
                None => return false,
            }
        }
        for p in &job.passes {
            if self.compose_pipe(gpu, p).is_none() {
                self.finish_failed.insert(job.key.clone());
                return true;
            }
        }
        let size = job.size.clamp(64, 4096);
        let levels = 32 - size.leading_zeros();
        let cmd = enc.cmd(gpu);
        let mut color: Option<GpuTex> = None;
        let mut others: Vec<(Out, GpuTex, Box<View>)> = Vec::new();
        // what was made, let go when the rest cannot be
        let give_up = |enc: &mut Encoder, color: Option<GpuTex>, others: Vec<(Out, GpuTex, Box<View>)>| {
            if let Some(t) = color {
                enc.defer(Doomed::View(t.view));
                enc.defer(Doomed::Texture(t.tex));
            }
            for (_, t, read) in others {
                enc.defer(Doomed::View(read));
                enc.defer(Doomed::View(t.view));
                enc.defer(Doomed::Texture(t.tex));
            }
            true
        };
        for (p, views) in job.passes.iter().zip(&all_views) {
            let mut samplers = Vec::new();
            for (slot, b) in p.info.samplers.clone() {
                let (u, v) = p.address.get(&slot).copied().unwrap_or((0, 0));
                let Some(s) = self.finish_sampler(gpu, u, v) else { return give_up(enc, color, others) };
                samplers.push((b, s));
            }
            let formats: &[vk::Format] = if p.out == Out::Color { &[vk::Format::R8G8B8A8_SRGB] } else { &[vk::Format::R8G8B8A8_SRGB, vk::Format::R8G8B8A8_UNORM] };
            let Some((tex, mut tviews, draw)) = target(gpu, cmd, "cosmetics finish", formats, size, levels) else { return give_up(enc, color, others) };
            let pipe = &self.compose_pipes[&pipe_key(p)];
            let ubo = enc.transient.mapped(gpu, p.consts.len() as u64, 256, p.consts.len() as u64, 1);
            unsafe { std::ptr::copy_nonoverlapping(p.consts.as_ptr(), ubo.host, p.consts.len()) };
            let ubo_info = [vk::DescriptorBufferInfo { buffer: unsafe { (*ubo.buffer).raw }, offset: ubo.offset, range: p.consts.len() as u64 }];
            let vs_bytes: &[u8] = match &p.vertex {
                Vertex::Glove(b) => b,
                Vertex::Weapon(_) => &[],
            };
            let vs_ubo = (!vs_bytes.is_empty()).then(|| {
                let u = enc.transient.mapped(gpu, vs_bytes.len() as u64, 256, vs_bytes.len() as u64, 1);
                unsafe { std::ptr::copy_nonoverlapping(vs_bytes.as_ptr(), u.host, vs_bytes.len()) };
                [vk::DescriptorBufferInfo { buffer: unsafe { (*u.buffer).raw }, offset: u.offset, range: vs_bytes.len() as u64 }]
            });
            let imgs: Vec<[vk::DescriptorImageInfo; 1]> = views.iter().map(|(_, v)| [vk::DescriptorImageInfo { sampler: vk::Sampler::null(), image_view: *v, image_layout: vk::ImageLayout::GENERAL }]).collect();
            let smps: Vec<[vk::DescriptorImageInfo; 1]> = samplers.iter().map(|(_, s)| [vk::DescriptorImageInfo { sampler: *s, image_view: vk::ImageView::null(), image_layout: vk::ImageLayout::GENERAL }]).collect();
            let mut writes = vec![vk::WriteDescriptorSet::default().dst_binding(0).descriptor_type(vk::DescriptorType::UNIFORM_BUFFER).buffer_info(&ubo_info)];
            if let Some(v) = vs_ubo.as_ref() {
                writes.push(vk::WriteDescriptorSet::default().dst_binding(1).descriptor_type(vk::DescriptorType::UNIFORM_BUFFER).buffer_info(v));
            }
            for (k, (b, _)) in views.iter().enumerate() {
                writes.push(vk::WriteDescriptorSet::default().dst_binding(*b).descriptor_type(vk::DescriptorType::SAMPLED_IMAGE).image_info(&imgs[k]));
            }
            for (k, (b, _)) in samplers.iter().enumerate() {
                writes.push(vk::WriteDescriptorSet::default().dst_binding(*b).descriptor_type(vk::DescriptorType::SAMPLER).image_info(&smps[k]));
            }
            let push: Vec<u8> = match &p.vertex {
                Vertex::Weapon(x) => x.iter().flat_map(|r| r.iter().flat_map(|v| v.to_le_bytes())).collect(),
                Vertex::Glove(_) => vec![0u8; 128],
            };
            let att = [vk::RenderingAttachmentInfo::default().image_view(draw).image_layout(vk::ImageLayout::GENERAL).load_op(vk::AttachmentLoadOp::DONT_CARE).store_op(vk::AttachmentStoreOp::STORE)];
            let area = vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent: vk::Extent2D { width: size, height: size } };
            let info = vk::RenderingInfo::default().render_area(area).layer_count(1).color_attachments(&att);
            unsafe {
                gpu.render_d.cmd_begin_rendering(cmd, &info);
                gpu.device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pipe.pipeline);
                gpu.device.cmd_set_viewport(cmd, 0, &[vk::Viewport { x: 0.0, y: 0.0, width: size as f32, height: size as f32, min_depth: 0.0, max_depth: 1.0 }]);
                gpu.device.cmd_set_scissor(cmd, 0, &[area]);
                gpu.push_d.cmd_push_descriptor_set(cmd, vk::PipelineBindPoint::GRAPHICS, pipe.layout, 1, &writes);
                gpu.device.cmd_push_constants(cmd, pipe.layout, vk::ShaderStageFlags::VERTEX, 0, &push);
                gpu.device.cmd_draw(cmd, 3, 1, 0, 0);
                gpu.render_d.cmd_end_rendering(cmd);
            }
            memory_barrier(gpu, cmd);
            mips(gpu, cmd, tex.image, size, levels);
            unsafe { gpu.device.destroy_image_view(draw, None) };
            if p.out == Out::Color {
                color = Some(GpuTex { tex, view: tviews.remove(0) });
            } else {
                let read = tviews.remove(1);
                others.push((p.out, GpuTex { tex, view: tviews.remove(0) }, read));
            }
        }
        let Some(color) = color else { return true };
        let mut f = Finished { color, metal: None, normal: None, ao: None };
        for (out, t, read) in others {
            let slot = match out {
                Out::Metal => &mut f.metal,
                Out::Normal => &mut f.normal,
                _ => &mut f.ao,
            };
            *slot = Some((t, read));
        }
        self.finishes.insert(job.key.clone(), f);
        true
    }

    pub(super) fn destroy_compose(&mut self, gpu: &Gpu) {
        let d = &gpu.device;
        for (_, p) in self.compose_pipes.drain() {
            unsafe {
                d.destroy_pipeline(p.pipeline, None);
                d.destroy_pipeline_layout(p.layout, None);
                d.destroy_descriptor_set_layout(p.set_layout, None);
                d.destroy_descriptor_set_layout(p.empty, None);
            }
        }
        for (_, s) in self.finish_samplers.drain() {
            unsafe { d.destroy_sampler(s, None) };
        }
        let kill = |t: GpuTex| {
            unsafe { d.destroy_image_view(t.view.raw, None) };
            let mut tex = t.tex;
            unsafe { d.destroy_image(tex.image, None) };
            if let Some(a) = tex.alloc.take() {
                gpu.memory.free(d, a);
            }
        };
        for (_, f) in self.finishes.drain() {
            kill(f.color);
            for (t, read) in [f.metal, f.normal, f.ao].into_iter().flatten() {
                unsafe { d.destroy_image_view(read.raw, None) };
                kill(t);
            }
        }
    }
}
