//! Chams' traces on the ground (shaders/engine/ground.comp): a map of the ground round the bodies, 64 by 64 blocks at
//! 8 texels a block, addressed modulo its size as it moves with them (what comes into it is empty) — how much of a
//! trace each place has, how fresh it is, the height of the ground it lies on — stepped each frame (fading over the
//! time the player set, the bodies standing on the ground leaving theirs) and drawn over the level's picture where the
//! ground is, as the look leaves it: ink soaked into stains, the liquid in puddles, the curse's dying ground. Before the
//! ink's smoke goes over the picture (the stains are under it).

use super::gpu::{image_in, invert, retire, Compute};
use super::ink::{bytes, dispatch, Bind, Body, Look};
use crate::vk::device::Gpu;
use crate::vk::encoder::{memory_barrier, Encoder};
use crate::vk::resources::{Sampler, Texture, View};
use ash::vk;
use std::time::Instant;

/// The map's side (texels) and how many to a block.
const SIZE: u32 = 512;
const PER_BLOCK: f64 = 8.0;
const UBO: u64 = 384;

pub struct Ground {
    pipe: Option<Compute>,
    map: Option<(Box<Texture>, Box<View>)>,
    canvas: Option<(Box<Texture>, Box<View>)>,
    /// The map's lowest corner (world texels, x and z), whether it was placed yet.
    origin: [i32; 2],
    placed: bool,
    started: Instant,
    last: Option<Instant>,
    /// Seconds without a body: past a trace's time nothing is left to step or draw.
    idle: f32,
}

impl Ground {
    pub fn new() -> Ground {
        Ground { pipe: None, map: None, canvas: None, origin: [0; 2], placed: false, started: Instant::now(), last: None, idle: 1e9 }
    }

    /// Its shader, compiled by Java (lib.rs shaders_init).
    pub fn set_shader(&mut self, gpu: &Gpu, spirv: &[u32]) {
        use vk::DescriptorType as T;
        let kinds = [T::STORAGE_IMAGE, T::STORAGE_IMAGE, T::COMBINED_IMAGE_SAMPLER, T::COMBINED_IMAGE_SAMPLER, T::UNIFORM_BUFFER];
        if let Some(old) = self.pipe.replace(Compute::new(gpu, "chams_ground", spirv, &kinds, 4)) {
            old.destroy(gpu);
        }
    }

    /// The map given up (the module off, the traces off): made afresh when wanted again.
    fn let_go(&mut self, enc: &mut Encoder) {
        for (t, v) in self.map.take().into_iter().chain(self.canvas.take()) {
            retire(enc, t, v);
        }
        self.placed = false;
        self.idle = 1e9;
    }

    /// One frame: the map stepped, the traces over `color` (`depth` the level's), the bodies from the camera.
    #[allow(clippy::too_many_arguments)]
    pub fn frame(
        &mut self,
        gpu: &Gpu,
        enc: &mut Encoder,
        color: &Texture,
        depth: &Texture,
        bodies: &[Body],
        look: &Look,
        camera: [f64; 3],
        view_proj: [f32; 16],
        sun: [f32; 4],
        sky: [f32; 4],
        fog: [f32; 4],
        voxels: Option<(&View, &Sampler)>,
        window: [i32; 5],
        nearest: &Sampler,
    ) {
        let now = Instant::now();
        let dt = self.last.map_or(0.0, |l| now.duration_since(l).as_secs_f32()).min(0.25);
        self.last = Some(now);
        let (Some(pipe), Some((vox, vox_sampler))) = (self.pipe.as_ref(), voxels) else { return };
        if !look.on || !look.traces {
            if self.map.is_some() {
                self.let_go(enc);
            }
            return;
        }
        self.idle = if bodies.is_empty() { self.idle + dt } else { 0.0 };
        if self.idle > look.traces_last + 1.0 || color.width == 0 || color.height == 0 {
            return;
        }
        let cmd = enc.cmd(gpu);
        if self.map.is_none() {
            let (t, v) = image_in(gpu, cmd, "Chams traces", vk::Format::R32G32B32A32_SFLOAT, SIZE, SIZE, 1, false);
            let range = vk::ImageSubresourceRange { aspect_mask: vk::ImageAspectFlags::COLOR, base_mip_level: 0, level_count: 1, base_array_layer: 0, layer_count: 1 };
            unsafe { gpu.device.cmd_clear_color_image(cmd, t.image, vk::ImageLayout::GENERAL, &vk::ClearColorValue { float32: [0.0; 4] }, &[range]) };
            memory_barrier(gpu, cmd);
            self.map = Some((t, v));
            self.placed = false;
        }
        let (fw, fh) = (color.width, color.height);
        if self.canvas.as_ref().is_some_and(|c| c.0.width != fw || c.0.height != fh) {
            if let Some((t, v)) = self.canvas.take() {
                retire(enc, t, v);
            }
        }
        if self.canvas.is_none() {
            self.canvas = Some(image_in(gpu, cmd, "Chams traces over the frame", vk::Format::R8G8B8A8_UNORM, fw, fh, 1, false));
        }
        // the map round the first body (one's own when it is in the look): its feet in the middle
        let before = if self.placed { self.origin } else { [i32::MIN / 2; 2] };
        if let Some(b) = bodies.first() {
            let fx = ((camera[0] + b.feet[0] as f64) * PER_BLOCK).floor() as i32;
            let fz = ((camera[2] + b.feet[2] as f64) * PER_BLOCK).floor() as i32;
            self.origin = [fx - SIZE as i32 / 2, fz - SIZE as i32 / 2];
            self.placed = true;
        }
        let cam_i = [camera[0].floor() as i32, camera[1].floor() as i32, camera[2].floor() as i32];
        let cam_f = [(camera[0] - cam_i[0] as f64) as f32, (camera[1] - cam_i[1] as f64) as f32, (camera[2] - cam_i[2] as f64) as f32];
        let piece = enc.transient.mapped(gpu, UBO, gpu.info.min_uniform_align.max(16) as u64, UBO, 1);
        let host = piece.host;
        bytes(host, 0, &invert(&view_proj));
        bytes(host, 64, &[self.origin[0], self.origin[1], before[0], before[1]]);
        bytes(host, 80, &[cam_i[0], cam_i[1], cam_i[2], 0]);
        bytes(host, 96, &[cam_f[0], cam_f[1], cam_f[2], 0.0]);
        bytes(host, 112, &[dt, look.kind as f32, look.traces_last, look.traces_size]);
        let n = bodies.len().min(8);
        bytes(host, 128, &[(now.duration_since(self.started).as_secs_f64() % 3600.0) as f32, n as f32, fw as f32, fh as f32]);
        bytes(host, 144, &[window[0], window[1], window[2], window[3]]);
        bytes(host, 160, &[window[4], 0, 0, 0]);
        let mut feet = [0f32; 32];
        for (i, b) in bodies.iter().take(8).enumerate() {
            let v = (b.vel[0] * b.vel[0] + b.vel[2] * b.vel[2]).sqrt();
            feet[i * 4..i * 4 + 4].copy_from_slice(&[b.feet[0], b.feet[1], b.feet[2], v]);
        }
        bytes(host, 176, &feet);
        let (tint, more) = match look.kind {
            super::ink::KIND_LIQUID => {
                let (c, d) = (look.liquid.colour, look.liquid.deep);
                ([c[0], c[1], c[2], look.liquid.metal], [d[0], d[1], d[2], look.liquid.clear])
            }
            super::ink::KIND_CURSE => {
                let (g, v) = (look.curse.dead_ground, look.curse.veins_colour);
                ([g[0], g[1], g[2], 0.0], [v[0], v[1], v[2], 0.0])
            }
            // the shadow's: soot
            super::ink::KIND_SHADOW => ([0.018, 0.018, 0.02, 0.0], [0.0; 4]),
            _ => ([look.dye[0], look.dye[1], look.dye[2], 0.0], [0.0; 4]),
        };
        bytes(host, 304, &tint);
        bytes(host, 368, &more);
        bytes(host, 320, &sky);
        bytes(host, 336, &[fog[0], fog[1], fog[2], 0.0]);
        let l = (sun[0] * sun[0] + sun[1] * sun[1] + sun[2] * sun[2]).sqrt().max(1e-4);
        bytes(host, 352, &[sun[0] / l, sun[1] / l, sun[2] / l, sun[3].max(0.0)]);
        let ubo = Bind::Uniform(unsafe { (*piece.buffer).raw }, piece.offset, UBO);
        let depth_view = super::upscale::temp_view(gpu, enc, depth);
        let (map, canvas) = (self.map.as_ref().unwrap(), self.canvas.as_ref().unwrap());
        let set = [Bind::Storage(map.1.raw), Bind::Storage(canvas.1.raw), Bind::Sampled(depth_view, nearest.raw), Bind::Sampled(vox.raw, vox_sampler.raw), ubo];
        enc.segment(gpu, cmd, "Engine", "chams traces (compute)");
        dispatch(gpu, cmd, pipe, &set, &0i32.to_ne_bytes(), [SIZE / 8, SIZE / 8, 1]);
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
        dispatch(gpu, cmd, pipe, &set, &1i32.to_ne_bytes(), [fw.div_ceil(8), fh.div_ceil(8), 1]);
        carry(canvas.0.image, color.image);
        memory_barrier(gpu, cmd);
        enc.segment_end(gpu, cmd);
    }

    pub fn destroy(&mut self, gpu: &Gpu) {
        if let Some(p) = self.pipe.take() {
            p.destroy(gpu);
        }
        for (t, v) in self.map.take().into_iter().chain(self.canvas.take()) {
            super::ink::kill(gpu, t, v);
        }
    }
}
