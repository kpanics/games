//! The world's frames put together at the window's size (shaders/engine/taa.comp): the level is drawn into a smaller
//! target of ours, moved by a fraction of a pixel each frame (cursed.recode.engine.Upscale); here this frame and the
//! ones before it make the picture, which goes into the game's main target before the hand and the GUI are drawn.
//!
//! Two history images at the window's size take turns: one read (last frame's picture), one written (this frame's),
//! then copied into the main target.

use super::gpu::{fail, image_in, retire, Compute};
use crate::vk::device::Gpu;
use crate::vk::encoder::{memory_barrier, Encoder};
use crate::vk::resources::{Doomed, Texture, View};
use ash::vk;

/// The shader's uniform block (shaders: Taa), bytes.
const UBO: u64 = 64 + 64 + 16 + 16 + 16;
/// How much of this frame goes into the picture: the rest is the frames before.
const BLEND: f32 = 0.1;

/// What the light in the air needs from the engine this frame.
pub struct Air {
    pub sunvis: vk::ImageView,
    pub linear: vk::Sampler,
    pub ubo: vk::Buffer,
    pub ubo_offset: u64,
    pub ubo_size: u64,
    pub frame: i32,
    /// Whether the air's light is on (else only the glow).
    pub air: bool,
    pub glow: bool,
}

pub struct Upscale {
    pipeline: Option<Compute>,
    volume_pipeline: Option<Compute>,
    /// The light in the air at half the small picture's size, and a 1x1 one until then.
    volume: Option<(Box<Texture>, Box<View>, u32, u32)>,
    volume_blank: Option<(Box<Texture>, Box<View>)>,
    /// The glow round glowing blocks (glow.comp): three levels from half the small picture down, and their size.
    glow_pipeline: Option<Compute>,
    glow: Vec<(Box<Texture>, Box<View>)>,
    glow_size: (u32, u32),
    linear: vk::Sampler,
    nearest: vk::Sampler,
    history: Vec<(Box<Texture>, Box<View>)>,
    size: (u32, u32),
    current: usize,
    valid: bool,
    prev_view_proj: [f32; 16],
    prev_camera: [f64; 3],
}

/// A view of one of the game's 2D textures (colour or depth), for this frame only.
pub(crate) fn temp_view(gpu: &Gpu, enc: &mut Encoder, t: &Texture) -> vk::ImageView {
    let info = vk::ImageViewCreateInfo::default()
        .image(t.image)
        .view_type(vk::ImageViewType::TYPE_2D)
        .format(t.format)
        .subresource_range(vk::ImageSubresourceRange { aspect_mask: t.view_aspect(), base_mip_level: 0, level_count: 1, base_array_layer: 0, layer_count: 1 });
    let raw = fail(unsafe { gpu.device.create_image_view(&info, None) }, "Can't make a view for the upscale");
    enc.defer(Doomed::View(Box::new(View { raw, texture: t as *const Texture, base_mip: 0 })));
    raw
}

/// `m` moved back by the jitter: the translation in clip space taken off (row 0 and 1 less jitter times row 3).
fn unjitter(m: &[f32; 16], j: [f32; 2]) -> [f32; 16] {
    let mut out = *m;
    for c in 0..4 {
        out[c * 4] -= j[0] * m[c * 4 + 3];
        out[c * 4 + 1] -= j[1] * m[c * 4 + 3];
    }
    out
}

impl Upscale {
    pub fn new(gpu: &Gpu) -> Upscale {
        let sampler = |filter: vk::Filter| {
            let info = vk::SamplerCreateInfo::default()
                .mag_filter(filter)
                .min_filter(filter)
                .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
                .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .max_lod(0.0);
            fail(unsafe { gpu.device.create_sampler(&info, None) }, "Can't make the upscale sampler")
        };
        Upscale {
            pipeline: None,
            volume_pipeline: None,
            volume: None,
            volume_blank: None,
            glow_pipeline: None,
            glow: Vec::new(),
            glow_size: (0, 0),
            linear: sampler(vk::Filter::LINEAR),
            nearest: sampler(vk::Filter::NEAREST),
            history: Vec::new(),
            size: (0, 0),
            current: 0,
            valid: false,
            prev_view_proj: [0.0; 16],
            prev_camera: [0.0; 3],
        }
    }

    pub fn set_glow_shader(&mut self, gpu: &Gpu, spirv: &[u32]) {
        let kinds = [vk::DescriptorType::COMBINED_IMAGE_SAMPLER, vk::DescriptorType::COMBINED_IMAGE_SAMPLER, vk::DescriptorType::STORAGE_IMAGE];
        if let Some(old) = self.glow_pipeline.replace(Compute::new(gpu, "engine_glow", spirv, &kinds, 16)) {
            old.destroy(gpu);
        }
    }

    /// The glow's levels from this frame's small picture (outside any rendering): true when there is one to add.
    fn make_glow(&mut self, gpu: &Gpu, enc: &mut Encoder, cmd: vk::CommandBuffer, color_view: vk::ImageView, depth_view: vk::ImageView, w: u32, h: u32) -> bool {
        let Some(c) = self.glow_pipeline.as_ref() else { return false };
        if self.glow_size != (w, h) || self.glow.len() != 3 {
            for (t, v) in self.glow.drain(..) {
                retire(enc, t, v);
            }
            for i in 0..3u32 {
                let (lw, lh) = ((w >> (i + 1)).max(1), (h >> (i + 1)).max(1));
                self.glow.push(image_in(gpu, cmd, &format!("Engine glow {i}"), vk::Format::R16G16B16A16_SFLOAT, lw, lh, 1, false));
            }
            self.glow_size = (w, h);
        }
        let d = &gpu.device;
        let size = |i: usize| ((w >> (i + 1)).max(1), (h >> (i + 1)).max(1));
        let run = |mode: i32, source: vk::ImageView, target: usize| {
            let (lw, lh) = size(target);
            let infos = [
                vk::DescriptorImageInfo { sampler: self.linear, image_view: source, image_layout: vk::ImageLayout::GENERAL },
                vk::DescriptorImageInfo { sampler: self.nearest, image_view: depth_view, image_layout: vk::ImageLayout::GENERAL },
                vk::DescriptorImageInfo { sampler: vk::Sampler::null(), image_view: self.glow[target].1.raw, image_layout: vk::ImageLayout::GENERAL },
            ];
            let writes = [
                vk::WriteDescriptorSet::default().dst_binding(0).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(std::slice::from_ref(&infos[0])),
                vk::WriteDescriptorSet::default().dst_binding(1).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(std::slice::from_ref(&infos[1])),
                vk::WriteDescriptorSet::default().dst_binding(2).descriptor_type(vk::DescriptorType::STORAGE_IMAGE).image_info(std::slice::from_ref(&infos[2])),
            ];
            let push = [mode, lw as i32, lh as i32, 0];
            unsafe {
                d.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, c.pipeline);
                gpu.push_d.cmd_push_descriptor_set(cmd, vk::PipelineBindPoint::COMPUTE, c.layout, 0, &writes);
                d.cmd_push_constants(cmd, c.layout, vk::ShaderStageFlags::COMPUTE, 0, std::slice::from_raw_parts(push.as_ptr() as *const u8, 16));
                d.cmd_dispatch(cmd, lw.div_ceil(8), lh.div_ceil(8), 1);
            }
            memory_barrier(gpu, cmd);
        };
        enc.segment(gpu, cmd, "Engine", "glow (compute)");
        let views: Vec<vk::ImageView> = self.glow.iter().map(|g| g.1.raw).collect();
        run(0, color_view, 0);
        run(1, views[0], 1);
        run(1, views[1], 2);
        run(2, views[2], 1);
        run(2, views[1], 0);
        enc.segment_end(gpu, cmd);
        true
    }

    pub fn set_volume_shader(&mut self, gpu: &Gpu, spirv: &[u32]) {
        let kinds = [
            vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            vk::DescriptorType::STORAGE_IMAGE,
            vk::DescriptorType::UNIFORM_BUFFER,
        ];
        if let Some(old) = self.volume_pipeline.replace(Compute::new(gpu, "engine_volume", spirv, &kinds, 128)) {
            old.destroy(gpu);
        }
    }

    pub fn set_shader(&mut self, gpu: &Gpu, spirv: &[u32]) {
        let kinds = [
            vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            vk::DescriptorType::STORAGE_IMAGE,
            vk::DescriptorType::UNIFORM_BUFFER,
            vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
        ];
        let c = Compute::new(gpu, "engine_taa", spirv, &kinds, 0);
        if let Some(old) = self.pipeline.replace(c) {
            old.destroy(gpu);
        }
    }

    pub fn ready(&self) -> bool {
        self.pipeline.is_some()
    }

    /// This frame of the level (`color`, `depth`: the small target's) into `out` (the game's main target colour).
    /// `view_proj` is the frame's terrain matrix as drawn (moved by `jitter`, clip space), `camera` where it was.
    #[allow(clippy::too_many_arguments)]
    pub fn resolve(
        &mut self,
        gpu: &Gpu,
        enc: &mut Encoder,
        color: &Texture,
        depth: &Texture,
        out: &Texture,
        jitter: [f32; 2],
        view_proj: [f32; 16],
        camera: [f64; 3],
        air: Option<Air>,
    ) {
        if self.pipeline.is_none() {
            return;
        }
        let cmd = enc.cmd(gpu);
        if self.volume_blank.is_none() {
            self.volume_blank = Some(image_in(gpu, cmd, "Engine blank air", vk::Format::R16G16B16A16_SFLOAT, 1, 1, 1, false));
        }
        let (w, h) = (out.width, out.height);
        if self.size != (w, h) || self.history.len() != 2 {
            for (t, v) in self.history.drain(..) {
                retire(enc, t, v);
            }
            for i in 0..2 {
                self.history.push(image_in(gpu, cmd, &format!("Engine picture {i}"), vk::Format::A2B10G10R10_UNORM_PACK32, w, h, 1, false));
            }
            self.size = (w, h);
            self.valid = false;
        }
        let color_view = temp_view(gpu, enc, color);
        let depth_view = temp_view(gpu, enc, depth);
        let inv = super::gpu::invert(&view_proj);
        // The light in the air: a walk for every other pixel of the small picture.
        let mut air_on = false;
        if let (Some(a), Some(vc)) = (air.as_ref().filter(|a| a.air), self.volume_pipeline.as_ref()) {
            let (vw, vh) = (color.width.div_ceil(2), color.height.div_ceil(2));
            if self.volume.as_ref().map_or(true, |v| v.2 != vw || v.3 != vh) {
                if let Some((t, v, _, _)) = self.volume.take() {
                    retire(enc, t, v);
                }
                let (t, v) = image_in(gpu, cmd, "Engine air", vk::Format::R16G16B16A16_SFLOAT, vw, vh, 1, false);
                self.volume = Some((t, v, vw, vh));
            }
            let target = self.volume.as_ref().unwrap();
            let base = [camera[0].floor(), camera[1].floor(), camera[2].floor()];
            let ints = [base[0] as i32, base[1] as i32, base[2] as i32, a.frame];
            let frac = [(camera[0] - base[0]) as f32, (camera[1] - base[1]) as f32, (camera[2] - base[2]) as f32, 0.0f32];
            let sizes = [color.width as i32, color.height as i32, vw as i32, vh as i32];
            let mut push = [0u8; 128];
            unsafe {
                std::ptr::copy_nonoverlapping(inv.as_ptr() as *const u8, push.as_mut_ptr(), 64);
                std::ptr::copy_nonoverlapping(ints.as_ptr() as *const u8, push.as_mut_ptr().add(64), 16);
                std::ptr::copy_nonoverlapping(frac.as_ptr() as *const u8, push.as_mut_ptr().add(80), 16);
                std::ptr::copy_nonoverlapping(sizes.as_ptr() as *const u8, push.as_mut_ptr().add(96), 16);
            }
            let infos = [
                vk::DescriptorImageInfo { sampler: self.nearest, image_view: depth_view, image_layout: vk::ImageLayout::GENERAL },
                vk::DescriptorImageInfo { sampler: a.linear, image_view: a.sunvis, image_layout: vk::ImageLayout::GENERAL },
                vk::DescriptorImageInfo { sampler: vk::Sampler::null(), image_view: target.1.raw, image_layout: vk::ImageLayout::GENERAL },
            ];
            let buffer = [vk::DescriptorBufferInfo { buffer: a.ubo, offset: a.ubo_offset, range: a.ubo_size }];
            let writes = [
                vk::WriteDescriptorSet::default().dst_binding(0).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(std::slice::from_ref(&infos[0])),
                vk::WriteDescriptorSet::default().dst_binding(1).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(std::slice::from_ref(&infos[1])),
                vk::WriteDescriptorSet::default().dst_binding(2).descriptor_type(vk::DescriptorType::STORAGE_IMAGE).image_info(std::slice::from_ref(&infos[2])),
                vk::WriteDescriptorSet::default().dst_binding(3).descriptor_type(vk::DescriptorType::UNIFORM_BUFFER).buffer_info(&buffer),
            ];
            let d = &gpu.device;
            enc.segment(gpu, cmd, "Engine", "air (compute)");
            unsafe {
                d.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, vc.pipeline);
                gpu.push_d.cmd_push_descriptor_set(cmd, vk::PipelineBindPoint::COMPUTE, vc.layout, 0, &writes);
                d.cmd_push_constants(cmd, vc.layout, vk::ShaderStageFlags::COMPUTE, 0, &push);
                d.cmd_dispatch(cmd, vw.div_ceil(8), vh.div_ceil(8), 1);
            }
            enc.segment_end(gpu, cmd);
            memory_barrier(gpu, cmd);
            air_on = true;
        }
        let glow_on = air.as_ref().is_some_and(|a| a.glow) && self.make_glow(gpu, enc, cmd, color_view, depth_view, color.width, color.height);
        let c = self.pipeline.as_ref().unwrap();
        let volume_view = if air_on { self.volume.as_ref().unwrap().1.raw } else { self.volume_blank.as_ref().unwrap().1.raw };
        let glow_view = if glow_on { self.glow[0].1.raw } else { self.volume_blank.as_ref().unwrap().1.raw };
        let read = &self.history[self.current ^ 1];
        let write = &self.history[self.current];

        let now = unjitter(&view_proj, jitter);
        let delta = [(camera[0] - self.prev_camera[0]) as f32, (camera[1] - self.prev_camera[1]) as f32, (camera[2] - self.prev_camera[2]) as f32];
        // A jump (teleport, another world) starts over.
        let valid = self.valid && delta.iter().map(|d| d * d).sum::<f32>() < 64.0 * 64.0;
        let piece = enc.transient.mapped(gpu, UBO, gpu.info.min_uniform_align.max(16) as u64, UBO, 1);
        unsafe {
            let dst = piece.host as *mut f32;
            std::ptr::copy_nonoverlapping(inv.as_ptr(), dst, 16);
            std::ptr::copy_nonoverlapping(self.prev_view_proj.as_ptr(), dst.add(16), 16);
            let more = [
                delta[0], delta[1], delta[2], if valid { 1.0 } else { 0.0 },
                jitter[0] * color.width as f32 * 0.5, jitter[1] * color.height as f32 * 0.5, BLEND, (air_on as i32 | (glow_on as i32) << 1) as f32,
            ];
            std::ptr::copy_nonoverlapping(more.as_ptr(), dst.add(32), 8);
            let sizes = [w as i32, h as i32, color.width as i32, color.height as i32];
            std::ptr::copy_nonoverlapping(sizes.as_ptr() as *const f32, dst.add(40), 4);
        }
        let infos = [
            vk::DescriptorImageInfo { sampler: self.linear, image_view: color_view, image_layout: vk::ImageLayout::GENERAL },
            vk::DescriptorImageInfo { sampler: self.nearest, image_view: depth_view, image_layout: vk::ImageLayout::GENERAL },
            vk::DescriptorImageInfo { sampler: self.linear, image_view: read.1.raw, image_layout: vk::ImageLayout::GENERAL },
            vk::DescriptorImageInfo { sampler: vk::Sampler::null(), image_view: write.1.raw, image_layout: vk::ImageLayout::GENERAL },
            vk::DescriptorImageInfo { sampler: self.linear, image_view: volume_view, image_layout: vk::ImageLayout::GENERAL },
            vk::DescriptorImageInfo { sampler: self.linear, image_view: glow_view, image_layout: vk::ImageLayout::GENERAL },
        ];
        let buffer = [vk::DescriptorBufferInfo { buffer: unsafe { (*piece.buffer).raw }, offset: piece.offset, range: UBO }];
        let writes = [
            vk::WriteDescriptorSet::default().dst_binding(0).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(std::slice::from_ref(&infos[0])),
            vk::WriteDescriptorSet::default().dst_binding(1).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(std::slice::from_ref(&infos[1])),
            vk::WriteDescriptorSet::default().dst_binding(2).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(std::slice::from_ref(&infos[2])),
            vk::WriteDescriptorSet::default().dst_binding(3).descriptor_type(vk::DescriptorType::STORAGE_IMAGE).image_info(std::slice::from_ref(&infos[3])),
            vk::WriteDescriptorSet::default().dst_binding(4).descriptor_type(vk::DescriptorType::UNIFORM_BUFFER).buffer_info(&buffer),
            vk::WriteDescriptorSet::default().dst_binding(5).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(std::slice::from_ref(&infos[4])),
            vk::WriteDescriptorSet::default().dst_binding(6).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(std::slice::from_ref(&infos[5])),
        ];
        let d = &gpu.device;
        enc.segment(gpu, cmd, "Engine", "upscale (compute)");
        unsafe {
            d.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, c.pipeline);
            gpu.push_d.cmd_push_descriptor_set(cmd, vk::PipelineBindPoint::COMPUTE, c.layout, 0, &writes);
            d.cmd_dispatch(cmd, w.div_ceil(16), h.div_ceil(16), 1);
        }
        memory_barrier(gpu, cmd);
        enc.segment_end(gpu, cmd);
        enc.segment(gpu, cmd, "Engine", "upscale copy");
        let range = vk::ImageSubresourceLayers { aspect_mask: vk::ImageAspectFlags::COLOR, mip_level: 0, base_array_layer: 0, layer_count: 1 };
        let corner = vk::Offset3D { x: w as i32, y: h as i32, z: 1 };
        let blit = vk::ImageBlit { src_subresource: range, src_offsets: [vk::Offset3D::default(), corner], dst_subresource: range, dst_offsets: [vk::Offset3D::default(), corner] };
        unsafe { d.cmd_blit_image(cmd, write.0.image, vk::ImageLayout::GENERAL, out.image, vk::ImageLayout::GENERAL, &[blit], vk::Filter::NEAREST) };
        enc.segment_end(gpu, cmd);
        memory_barrier(gpu, cmd);
        self.current ^= 1;
        self.valid = true;
        self.prev_view_proj = now;
        self.prev_camera = camera;
    }

    pub fn destroy(&mut self, gpu: &Gpu) {
        let d = &gpu.device;
        for (t, v) in self.history.drain(..) {
            unsafe {
                d.destroy_image_view(v.raw, None);
                d.destroy_image(t.image, None);
            }
            let mut t = t;
            if let Some(a) = t.alloc.take() {
                gpu.memory.free(d, a);
            }
        }
        if let Some(c) = self.pipeline.take() {
            c.destroy(gpu);
        }
        if let Some(c) = self.volume_pipeline.take() {
            c.destroy(gpu);
        }
        if let Some(c) = self.glow_pipeline.take() {
            c.destroy(gpu);
        }
        for (t, v) in self.glow.drain(..) {
            unsafe {
                d.destroy_image_view(v.raw, None);
                d.destroy_image(t.image, None);
            }
            let mut t = t;
            if let Some(a) = t.alloc.take() {
                gpu.memory.free(d, a);
            }
        }
        for (t, v) in self.volume.take().map(|(t, v, _, _)| (t, v)).into_iter().chain(self.volume_blank.take()) {
            unsafe {
                d.destroy_image_view(v.raw, None);
                d.destroy_image(t.image, None);
            }
            let mut t = t;
            if let Some(a) = t.alloc.take() {
                gpu.memory.free(d, a);
            }
        }
        unsafe {
            d.destroy_sampler(self.linear, None);
            d.destroy_sampler(self.nearest, None);
        }
    }
}
