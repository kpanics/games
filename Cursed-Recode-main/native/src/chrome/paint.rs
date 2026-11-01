//! The bar drawn (shaders/chrome/chrome.comp) into an image on a device: the game's (vk/surface.rs, every frame, over
//! the swap chain's top rows) or the window before the game's (twin, once a showing: it is still there).

use super::text::{metrics, Face};
use super::Look;
use crate::engine::gpu::{fail, image_in, Compute};
use crate::launch::{free_image, make_mips, mipped_image, upload, words, Image};
use crate::vk::device::Gpu;
use crate::vk::encoder::{memory_barrier, Encoder};
use ash::vk;

/// Her icon's side in the bar at 96 DPI, and the bar's left margin.
const ICON_96: f32 = 16.0;
const LEFT_96: f32 = 12.0;

pub struct Painter {
    pipe: Compute,
    atlas: Image,
    icon: Image,
    sampler: vk::Sampler,
    /// The bar's own picture, its size (the game's: the swap chain's width × the bar).
    strip: Option<(Image, u32, u32)>,
    glyphs: Vec<f32>,
}

impl Painter {
    /// Made on `gpu`; the atlas and her icon recorded into `cmd`.
    pub fn new(gpu: &Gpu, enc: &mut Encoder, cmd: vk::CommandBuffer) -> Painter {
        let ub = vk::DescriptorType::UNIFORM_BUFFER;
        let sa = vk::DescriptorType::COMBINED_IMAGE_SAMPLER;
        let pipe = Compute::new(
            gpu,
            "chrome",
            &words(include_bytes!("../../shaders/chrome/spv/chrome.spv")),
            &[ub, sa, sa, vk::DescriptorType::STORAGE_IMAGE, vk::DescriptorType::STORAGE_BUFFER],
            0,
        );
        let packed = miniz_oxide::inflate::decompress_to_vec_zlib(include_bytes!("../../assets/launch/text_atlas.r8.z")).expect("The text atlas is broken");
        let (aw, ah) = (u32::from_le_bytes(packed[0..4].try_into().unwrap()), u32::from_le_bytes(packed[4..8].try_into().unwrap()));
        let atlas = mipped_image(gpu, cmd, "Chrome text", vk::Format::R8_UNORM, aw, ah, 1);
        upload(gpu, enc, cmd, &atlas.t, &packed[8..]);
        let icon = mipped_image(gpu, cmd, "Chrome icon", vk::Format::R8G8B8A8_UNORM, 64, 64, 7);
        upload(gpu, enc, cmd, &icon.t, include_bytes!("../../assets/icons/recode_64.rgba"));
        make_mips(gpu, cmd, &icon.t);
        let sampler = fail(
            unsafe {
                gpu.device.create_sampler(
                    &vk::SamplerCreateInfo::default()
                        .mag_filter(vk::Filter::LINEAR)
                        .min_filter(vk::Filter::LINEAR)
                        .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
                        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .max_lod(16.0),
                    None,
                )
            },
            "Can't make the chrome's sampler",
        );
        Painter { pipe, atlas, icon, sampler, strip: None, glyphs: Vec::new() }
    }

    /// The bar as `look` says into `out` (a storage image at least its size, RGBA8, rows from the top).
    pub fn draw(&mut self, gpu: &Gpu, enc: &mut Encoder, cmd: vk::CommandBuffer, look: &Look, out: vk::ImageView) {
        let (w, h, s) = (look.width as f32, look.height as f32, look.scale.max(1.0));
        // her icon and the name, the place after it (the menu's small capitals)
        let m = metrics();
        self.glyphs.clear();
        let isz = (ICON_96 * s).round();
        let ix = (LEFT_96 * s).round();
        let iy = ((h - isz) * 0.5).floor();
        let size = 10.0 * s;
        let base = (h * 0.5 + m.ascent(Face::Mono, size) * 0.5 - 0.5 * s).round();
        let mut x = m.emit(&mut self.glyphs, Face::Mono, "CURSED RECODE", ix + isz + 9.0 * s, base, size, 0.16, [0.84, 0.84, 0.84, 1.0]);
        if !look.status.is_empty() {
            x += 8.0 * s;
            x = m.emit(&mut self.glyphs, Face::Mono, "·", x, base, size, 0.16, [0.42, 0.42, 0.42, 1.0]);
            m.emit(&mut self.glyphs, Face::Mono, &look.status.to_uppercase(), x + 6.0 * s, base, size, 0.12, [0.46, 0.46, 0.46, 1.0]);
        }
        let bw = super::button_width((96.0 * s) as u32) as f32;
        // (no text under the caption buttons)
        let mut kept = Vec::with_capacity(self.glyphs.len());
        for g in self.glyphs.chunks_exact(12) {
            if g[2] < w - 3.0 * bw - 6.0 * s {
                kept.extend_from_slice(g);
            }
        }
        let count = kept.len() / 12;
        let mut u = [0f32; 32];
        u[0..4].copy_from_slice(&[w, h, s, look.time]);
        u[4..8].copy_from_slice(&[look.active, look.maximized as u32 as f32, count as f32, bw]);
        u[8..11].copy_from_slice(&look.hover);
        u[12..15].copy_from_slice(&look.press);
        u[16..19].copy_from_slice(&look.lines);
        u[20..24].copy_from_slice(&[ix, iy, isz, 1.0]);
        u[24..28].copy_from_slice(&[m.spread, m.atlas[0] as f32, m.atlas[1] as f32, 0.0]);
        u[28..32].copy_from_slice(&[look.down, look.pill, look.pill_hover, 0.0]);
        let align = gpu.info.min_uniform_align.max(16) as u64;
        let piece = enc.transient.mapped(gpu, 128, align, 128, 1);
        unsafe { std::ptr::copy_nonoverlapping(u.as_ptr() as *const u8, piece.host, 128) };
        let ubo = vk::DescriptorBufferInfo { buffer: unsafe { (*piece.buffer).raw }, offset: piece.offset, range: 128 };
        let bytes = (kept.len().max(12) * 4) as u64;
        let list = enc.transient.mapped(gpu, bytes, 16, bytes, 1);
        if !kept.is_empty() {
            unsafe { std::ptr::copy_nonoverlapping(kept.as_ptr() as *const u8, list.host, kept.len() * 4) };
        }
        let glyphs = vk::DescriptorBufferInfo { buffer: unsafe { (*list.buffer).raw }, offset: list.offset, range: bytes };
        let images = [
            vk::DescriptorImageInfo { sampler: self.sampler, image_view: self.atlas.v.raw, image_layout: vk::ImageLayout::GENERAL },
            vk::DescriptorImageInfo { sampler: self.sampler, image_view: self.icon.v.raw, image_layout: vk::ImageLayout::GENERAL },
            vk::DescriptorImageInfo { sampler: vk::Sampler::null(), image_view: out, image_layout: vk::ImageLayout::GENERAL },
        ];
        let writes = [
            vk::WriteDescriptorSet::default().dst_binding(0).descriptor_type(vk::DescriptorType::UNIFORM_BUFFER).buffer_info(std::slice::from_ref(&ubo)),
            vk::WriteDescriptorSet::default().dst_binding(1).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(std::slice::from_ref(&images[0])),
            vk::WriteDescriptorSet::default().dst_binding(2).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(std::slice::from_ref(&images[1])),
            vk::WriteDescriptorSet::default().dst_binding(3).descriptor_type(vk::DescriptorType::STORAGE_IMAGE).image_info(std::slice::from_ref(&images[2])),
            vk::WriteDescriptorSet::default().dst_binding(4).descriptor_type(vk::DescriptorType::STORAGE_BUFFER).buffer_info(std::slice::from_ref(&glyphs)),
        ];
        let d = &gpu.device;
        unsafe {
            d.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.pipe.pipeline);
            gpu.push_d.cmd_push_descriptor_set(cmd, vk::PipelineBindPoint::COMPUTE, self.pipe.layout, 0, &writes);
            d.cmd_dispatch(cmd, (look.width.max(1) as u32).div_ceil(8), (look.height.max(1) as u32).div_ceil(8), 1);
        }
        memory_barrier(gpu, cmd);
    }

    /// The bar into the painter's own picture (made again when the size changes): its image, to copy from.
    pub fn draw_strip(&mut self, gpu: &Gpu, enc: &mut Encoder, cmd: vk::CommandBuffer, look: &Look) -> vk::Image {
        let (w, h) = (look.width.max(1) as u32, look.height.max(1) as u32);
        if self.strip.as_ref().is_none_or(|s| s.1 != w || s.2 != h) {
            if let Some((img, _, _)) = self.strip.take() {
                crate::engine::gpu::retire(enc, img.t, img.v);
            }
            let (t, v) = image_in(gpu, cmd, "Chrome bar", vk::Format::R8G8B8A8_UNORM, w, h, 1, false);
            self.strip = Some((Image { t, v }, w, h));
        }
        let view = self.strip.as_ref().unwrap().0.v.raw;
        self.draw(gpu, enc, cmd, look, view);
        self.strip.as_ref().unwrap().0.t.image
    }

    pub fn destroy(self, gpu: &Gpu) {
        free_image(gpu, self.atlas);
        free_image(gpu, self.icon);
        if let Some((img, _, _)) = self.strip {
            free_image(gpu, img);
        }
        unsafe { gpu.device.destroy_sampler(self.sampler, None) };
        self.pipe.destroy(gpu);
    }
}
