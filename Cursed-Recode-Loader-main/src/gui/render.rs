//! The menu's picture (gui/mod.rs lays out the shapes): the game's picture blurred for the glass, then every shape
//! over its tiles (shaders/gui/gui.comp), into the game's texture, which Java's screen puts over the game.
//! The frame op (vk/frame.rs GUI) comes in the game's frame after the world is drawn and before its GUI.

use super::draw::FLOATS;
use super::Built;
use crate::engine::gpu::{fail, image_in, retire, Compute};
use crate::engine::upscale::temp_view;
use crate::launch::{free_image, make_mips, mipped_image, upload, words, Image};
use crate::vk::device::Gpu;
use crate::vk::encoder::{memory_barrier, Encoder};
use crate::vk::resources::Texture;
use ash::vk;
use vk::DescriptorType as D;

const UBO: u64 = 12 * 16;
/// Levels of the glass's blur at most (each half the one before, from half the picture).
const LEVELS: usize = 5;

pub struct Render {
    gui: Compute,
    down: Compute,
    up: Compute,
    linear: vk::Sampler,
    mipped: vk::Sampler,
    fixed: Option<Fixed>,
    /// the menu's pictures, the HUD's, the menus' (menus/)
    sized: [Option<Sized>; 3],
    /// the menus' pictures (gui/pics.rs), made with the first of them
    pics: Option<Image>,
    /// the letters from the system's fonts (gui/dynfont.rs), made with the first of them
    dynfont: Option<Image>,
}

impl Render {
    /// The letters' atlases of distance fields (the faces', the system fonts' letters: the faces' again until the first
    /// of those is made), their views and sizes; for the world's words (engine/fx.rs).
    pub fn letters(&self) -> Option<[(vk::ImageView, u32, u32); 2]> {
        let f = self.fixed.as_ref()?;
        let a = (f.atlas.v.raw, f.atlas.t.width, f.atlas.t.height);
        let b = self.dynfont.as_ref().map_or(a, |d| (d.v.raw, d.t.width, d.t.height));
        Some([a, b])
    }
}

/// A renderer made on another thread (its pipelines compile there).
pub struct Made(pub Render);
unsafe impl Send for Made {}

struct Fixed {
    atlas: Image,
    logo: Image,
}

struct Sized {
    w: u32,
    h: u32,
    pic: Image,
    levels: Vec<Image>,
    /// the tiles drawn last frame (the HUD draws only those with shapes, and those that had some, to clear them), and
    /// the texture it went into (a new one is copied whole)
    last: Vec<u32>,
    fresh: bool,
    out: vk::Image,
}

enum Bind {
    Ubo(vk::DescriptorBufferInfo),
    Buffer(vk::DescriptorBufferInfo),
    Storage(vk::ImageView),
    Sampled(vk::ImageView, vk::Sampler),
}

impl Render {
    pub fn new(gpu: &Gpu) -> Render {
        let c = |name: &str, spv: &[u8], kinds: &[vk::DescriptorType], push: u32| Compute::new(gpu, &format!("gui_{name}"), &words(spv), kinds, push);
        let sampler = |mips: bool| {
            let info = vk::SamplerCreateInfo::default()
                .mag_filter(vk::Filter::LINEAR)
                .min_filter(vk::Filter::LINEAR)
                .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
                .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .max_lod(if mips { 16.0 } else { 0.0 });
            fail(unsafe { gpu.device.create_sampler(&info, None) }, "Can't make a menu sampler")
        };
        let (ub, sb, sa, st) = (D::UNIFORM_BUFFER, D::STORAGE_BUFFER, D::COMBINED_IMAGE_SAMPLER, D::STORAGE_IMAGE);
        Render {
            gui: c("gui", include_bytes!("../../shaders/gui/spv/gui.spv"), &[ub, sb, sb, sa, sa, sa, st, sa, sa, sa], 0),
            down: c("down", include_bytes!("../../shaders/gui/spv/down.spv"), &[ub, sa, st], 32),
            up: c("up", include_bytes!("../../shaders/gui/spv/up.spv"), &[ub, sa, st], 32),
            linear: sampler(false),
            mipped: sampler(true),
            fixed: None,
            sized: [None, None, None],
            pics: None,
            dynfont: None,
        }
    }

    fn make_fixed(gpu: &Gpu, enc: &mut Encoder, cmd: vk::CommandBuffer) -> Fixed {
        // the text's fields (tools/gui_atlas.py)
        let packed = miniz_oxide::inflate::decompress_to_vec_zlib(include_bytes!("../../assets/gui/text_atlas.r8.z")).expect("The menu's text atlas is broken");
        let (aw, ah) = (u32::from_le_bytes(packed[0..4].try_into().unwrap()), u32::from_le_bytes(packed[4..8].try_into().unwrap()));
        let atlas = mipped_image(gpu, cmd, "Menu text", vk::Format::R8_UNORM, aw, ah, 1);
        upload(gpu, enc, cmd, &atlas.t, &packed[8..]);
        // her logo, the launch's (its heat as the brightness), with mips: it is drawn small
        let base = include_bytes!("../../assets/launch/logo_base.r8");
        let logo = mipped_image(gpu, cmd, "Menu logo", vk::Format::R8_UNORM, 192, 192, 8);
        upload(gpu, enc, cmd, &logo.t, base);
        make_mips(gpu, cmd, &logo.t);
        Fixed { atlas, logo }
    }

    /// One frame of the menu into `out` (the game's texture, RGBA8, the window's size); `back` the game's picture
    /// as it is now (the world drawn, its GUI not yet), for the glass.
    pub fn frame(&mut self, gpu: &Gpu, enc: &mut Encoder, b: &Built, out: &Texture, back: Option<&Texture>) {
        let cmd = enc.cmd(gpu);
        if self.fixed.is_none() {
            self.fixed = Some(Self::make_fixed(gpu, enc, cmd));
        }
        let (w, h) = (out.width.max(1), out.height.max(1));
        let layer = (b.layer as usize).min(2);
        // the menus' new pictures into their atlas
        let pending = super::pics::drain();
        if self.pics.is_none() && (!pending.is_empty() || super::pics::any()) {
            let side = super::pics::SIZE;
            self.pics = Some(mipped_image(gpu, cmd, "Menus pictures", vk::Format::R8G8B8A8_UNORM, side, side, 1));
        }
        // the letters drawn from the system's fonts since the last frame
        let letters = super::dynfont::drain();
        if self.dynfont.is_none() && (!letters.is_empty() || super::dynfont::any()) {
            let side = super::dynfont::SIZE;
            self.dynfont = Some(mipped_image(gpu, cmd, "Menus letters", vk::Format::R8_UNORM, side, side, 1));
        }
        if let (Some(img), false) = (self.dynfont.as_ref(), letters.is_empty()) {
            for (at, px) in &letters {
                let n = px.len() as u64;
                let piece = enc.transient.staging(gpu, n.max(4), 16, n.max(4), 1);
                unsafe { std::ptr::copy_nonoverlapping(px.as_ptr(), piece.host, px.len()) };
                let region = vk::BufferImageCopy {
                    buffer_offset: piece.offset,
                    buffer_row_length: 0,
                    buffer_image_height: 0,
                    image_subresource: vk::ImageSubresourceLayers { aspect_mask: vk::ImageAspectFlags::COLOR, mip_level: 0, base_array_layer: 0, layer_count: 1 },
                    image_offset: vk::Offset3D { x: at[0] as i32, y: at[1] as i32, z: 0 },
                    image_extent: vk::Extent3D { width: at[2], height: at[3], depth: 1 },
                };
                unsafe { gpu.device.cmd_copy_buffer_to_image(cmd, (*piece.buffer).raw, img.t.image, vk::ImageLayout::GENERAL, &[region]) };
            }
            memory_barrier(gpu, cmd);
        }
        if let (Some(pics), false) = (self.pics.as_ref(), pending.is_empty()) {
            for (at, px) in &pending {
                let n = px.len() as u64;
                let piece = enc.transient.staging(gpu, n, 16, n, 1);
                unsafe { std::ptr::copy_nonoverlapping(px.as_ptr(), piece.host, px.len()) };
                let region = vk::BufferImageCopy {
                    buffer_offset: piece.offset,
                    buffer_row_length: 0,
                    buffer_image_height: 0,
                    image_subresource: vk::ImageSubresourceLayers { aspect_mask: vk::ImageAspectFlags::COLOR, mip_level: 0, base_array_layer: 0, layer_count: 1 },
                    image_offset: vk::Offset3D { x: at[0] as i32, y: at[1] as i32, z: 0 },
                    image_extent: vk::Extent3D { width: at[2], height: at[3], depth: 1 },
                };
                unsafe { gpu.device.cmd_copy_buffer_to_image(cmd, (*piece.buffer).raw, pics.t.image, vk::ImageLayout::GENERAL, &[region]) };
            }
            memory_barrier(gpu, cmd);
        }
        if self.sized[layer].as_ref().map_or(true, |s| s.w != w || s.h != h) {
            if let Some(old) = self.sized[layer].take() {
                for img in std::iter::once(old.pic).chain(old.levels) {
                    retire(enc, img.t, img.v);
                }
            }
            let img = |name: &str, format: vk::Format, w: u32, h: u32| {
                let (t, v) = image_in(gpu, cmd, name, format, w, h, 1, false);
                Image { t, v }
            };
            let mut levels = Vec::new();
            let (mut lw, mut lh) = (w, h);
            for n in 0..LEVELS {
                lw = (lw >> 1).max(1);
                lh = (lh >> 1).max(1);
                levels.push(img(&format!("Menu glass {n}"), vk::Format::R16G16B16A16_SFLOAT, lw, lh));
            }
            self.sized[layer] = Some(Sized { w, h, pic: img("Menu picture", vk::Format::R8G8B8A8_UNORM, w, h), levels, last: Vec::new(), fresh: true, out: vk::Image::null() });
        }
        let seg = if layer == 1 { "HUD" } else { "Menu" };
        enc.segment(gpu, cmd, seg, "glass blur");
        // the HUD: only the tiles with shapes now or a frame ago (all of them when its pictures are new, while the world
        // behind a screen is veiled, and the frame after: the veil is everywhere)
        let (tx0, ty0) = (b.tiles_x, h.div_ceil(super::draw::TILE));
        let mut tiles: Option<Vec<u32>> = None;
        if layer == 1 {
            let sz = self.sized[1].as_mut().unwrap();
            let now: Vec<u32> = (0..tx0 * ty0).filter(|&t| b.runs.get(t as usize * 2 + 1).is_some_and(|&n| n > 0)).collect();
            let veiled = b.look.veil > 0.001;
            let fresh = sz.fresh || sz.out != out.image || veiled;
            if !fresh {
                let mut all = now.clone();
                all.extend(sz.last.iter().copied().filter(|t| !now.contains(t)));
                tiles = Some(all);
            }
            sz.last = now;
            sz.fresh = veiled;
            sz.out = out.image;
        }
        let s = self.sized[layer].as_ref().unwrap();
        let f = self.fixed.as_ref().unwrap();
        let d = &gpu.device;
        let run = |c: &Compute, binds: &[Bind], push: Option<&[u8]>, groups: [u32; 3], barrier: bool| {
            let mut infos: Vec<vk::DescriptorImageInfo> = Vec::new();
            let mut bufs: Vec<vk::DescriptorBufferInfo> = Vec::new();
            for b in binds {
                match *b {
                    Bind::Ubo(x) | Bind::Buffer(x) => bufs.push(x),
                    Bind::Storage(v) => infos.push(vk::DescriptorImageInfo { sampler: vk::Sampler::null(), image_view: v, image_layout: vk::ImageLayout::GENERAL }),
                    Bind::Sampled(v, smp) => infos.push(vk::DescriptorImageInfo { sampler: smp, image_view: v, image_layout: vk::ImageLayout::GENERAL }),
                }
            }
            let (mut ii, mut bi) = (0, 0);
            let mut writes: Vec<vk::WriteDescriptorSet> = Vec::new();
            for (n, b) in binds.iter().enumerate() {
                let w = vk::WriteDescriptorSet::default().dst_binding(n as u32);
                writes.push(match b {
                    Bind::Ubo(_) => {
                        bi += 1;
                        w.descriptor_type(D::UNIFORM_BUFFER).buffer_info(std::slice::from_ref(&bufs[bi - 1]))
                    }
                    Bind::Buffer(_) => {
                        bi += 1;
                        w.descriptor_type(D::STORAGE_BUFFER).buffer_info(std::slice::from_ref(&bufs[bi - 1]))
                    }
                    Bind::Storage(_) => {
                        ii += 1;
                        w.descriptor_type(D::STORAGE_IMAGE).image_info(std::slice::from_ref(&infos[ii - 1]))
                    }
                    Bind::Sampled(..) => {
                        ii += 1;
                        w.descriptor_type(D::COMBINED_IMAGE_SAMPLER).image_info(std::slice::from_ref(&infos[ii - 1]))
                    }
                });
            }
            unsafe {
                d.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, c.pipeline);
                gpu.push_d.cmd_push_descriptor_set(cmd, vk::PipelineBindPoint::COMPUTE, c.layout, 0, &writes);
                if let Some(p) = push {
                    d.cmd_push_constants(cmd, c.layout, vk::ShaderStageFlags::COMPUTE, 0, p);
                }
                d.cmd_dispatch(cmd, groups[0], groups[1], groups[2]);
            }
            if barrier {
                memory_barrier(gpu, cmd);
            }
        };
        let g8 = |w: u32, h: u32| [w.div_ceil(8), h.div_ceil(8), 1];
        let push = |a: [f32; 4], b: [f32; 4]| -> [u8; 32] {
            let mut p = [0u8; 32];
            unsafe {
                std::ptr::copy_nonoverlapping(a.as_ptr() as *const u8, p.as_mut_ptr(), 16);
                std::ptr::copy_nonoverlapping(b.as_ptr() as *const u8, p.as_mut_ptr().add(16), 16);
            }
            p
        };
        let l = &b.look;
        let (tx, ty) = (tx0, ty0);
        let base = b.runs.len() + b.idx.len();
        let (list_at, list_n) = tiles.as_ref().map_or((0.0, 0.0), |t| (base as f32, t.len() as f32));
        // the frame's numbers (shaders/gui/gui.glsl)
        let u: [f32; 48] = [
            l.win[0], l.win[1], l.win[2], l.win[3],
            l.open, l.line, l.heat, l.line_mode,
            l.veil, l.rail, l.radius, l.fade,
            l.cursor[0], l.cursor[1], l.cursor[2], l.time,
            w as f32, h as f32, if back.is_some() { 1.0 } else { 0.0 }, (tx * ty * 2) as f32,
            f.atlas.t.width as f32, f.atlas.t.height as f32, super::text::SPREAD, tx as f32,
            l.glass[0], l.glass[1], l.glass[2], l.glass[3],
            l.veil_col[0], l.veil_col[1], l.veil_col[2], l.veil_col[3],
            l.theme[0], l.theme[1], l.theme[2], l.theme[3],
            l.fire[0], l.fire[1], l.fire[2], l.fire[3],
            list_at, list_n, 0.0, 0.0,
            l.anim[0], l.anim[1], l.anim[2], l.anim[3],
        ];
        let piece = enc.transient.mapped(gpu, UBO, gpu.info.min_uniform_align.max(16) as u64, UBO, 1);
        unsafe { std::ptr::copy_nonoverlapping(u.as_ptr() as *const u8, piece.host, UBO as usize) };
        let ubo = vk::DescriptorBufferInfo { buffer: unsafe { (*piece.buffer).raw }, offset: piece.offset, range: UBO };
        // 1) the glass: the game's picture down a few levels and back up (more levels, more blur)
        let back_view = back.map(|t| temp_view(gpu, enc, t));
        if let (Some(src), Some(t), false) = (back_view, back, b.areas.is_empty()) {
            // the picture is stored bottom row first (gui.comp pic()): the areas, in its rows
            let flipped: Vec<[f32; 4]> = b.areas.iter().map(|a| [a[0], h as f32 - a[3], a[2], h as f32 - a[1]]).collect();
            let b_areas = &flipped;
            let n = (1 + (l.blur.clamp(0.0, 1.0) * (LEVELS - 1) as f32).round() as usize).min(LEVELS);
            // only under the window and round it as far as the levels below reach: a level's texel takes 2 either
            // side of the one above, so its margin is twice the next one's and 2 (3 at the smallest)
            let area = |lv: &Image, k: usize, ar: [f32; 4]| -> ([f32; 2], [u32; 2]) {
                let f = (1u32 << (k + 1)) as f32;
                let m = (5u32 << (n - 1 - k)) as f32 - 2.0;
                let x0 = (ar[0] / f - m).floor().max(0.0);
                let y0 = (ar[1] / f - m).floor().max(0.0);
                let x1 = (ar[2] / f + m).ceil().min(lv.t.width as f32);
                let y1 = (ar[3] / f + m).ceil().min(lv.t.height as f32);
                ([x0, y0], [(x1 - x0).max(1.0) as u32, (y1 - y0).max(1.0) as u32])
            };
            // each area at a level, then the barrier before the next level
            let last = b_areas.len() - 1;
            for (k, lv) in s.levels.iter().take(n).enumerate() {
                let (sv, sw, sh) = if k == 0 { (src, t.width, t.height) } else { (s.levels[k - 1].v.raw, s.levels[k - 1].t.width, s.levels[k - 1].t.height) };
                for (ai, &ar) in b_areas.iter().enumerate() {
                    let (at, size) = area(lv, k, ar);
                    let p = push([1.0 / sw as f32, 1.0 / sh as f32, lv.t.width as f32, lv.t.height as f32], [at[0], at[1], 0.0, 0.0]);
                    run(&self.down, &[Bind::Ubo(ubo), Bind::Sampled(sv, self.linear), Bind::Storage(lv.v.raw)], Some(&p), g8(size[0], size[1]), ai == last);
                }
            }
            for k in (1..n).rev() {
                let (src, dst) = (&s.levels[k], &s.levels[k - 1]);
                for (ai, &ar) in b_areas.iter().enumerate() {
                    let (at, size) = area(dst, k - 1, ar);
                    let p = push([1.0 / src.t.width as f32, 1.0 / src.t.height as f32, dst.t.width as f32, dst.t.height as f32], [at[0], at[1], 0.0, 0.0]);
                    run(&self.up, &[Bind::Ubo(ubo), Bind::Sampled(src.v.raw, self.linear), Bind::Storage(dst.v.raw)], Some(&p), g8(size[0], size[1]), ai == last);
                }
            }
        }
        // 2) the shapes, tile by tile
        enc.segment(gpu, cmd, seg, "shapes");
        let buffer = |enc: &mut Encoder, bytes: &[u8]| {
            let n = (bytes.len() as u64).max(16);
            let piece = enc.transient.mapped(gpu, n, 16, n, 1);
            unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), piece.host, bytes.len()) };
            vk::DescriptorBufferInfo { buffer: unsafe { (*piece.buffer).raw }, offset: piece.offset, range: n }
        };
        let as_bytes = |v: &[u32]| unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, v.len() * 4) }.to_vec();
        let prims = buffer(enc, unsafe { std::slice::from_raw_parts(b.prims.as_ptr() as *const u8, b.prims.len() * 4) });
        debug_assert!(b.prims.len() % FLOATS == 0);
        let mut lists = b.runs.clone();
        lists.extend_from_slice(&b.idx);
        if let Some(t) = &tiles {
            lists.extend_from_slice(t);
        }
        let lists = buffer(enc, &as_bytes(&lists));
        run(
            &self.gui,
            &[
                Bind::Ubo(ubo),
                Bind::Buffer(prims),
                Bind::Buffer(lists),
                Bind::Sampled(f.atlas.v.raw, self.linear),
                Bind::Sampled(f.logo.v.raw, self.mipped),
                Bind::Sampled(s.levels[0].v.raw, self.linear),
                Bind::Storage(s.pic.v.raw),
                Bind::Sampled(back_view.unwrap_or(s.levels[0].v.raw), self.linear),
                Bind::Sampled(self.pics.as_ref().map_or(f.logo.v.raw, |p| p.v.raw), self.linear),
                Bind::Sampled(self.dynfont.as_ref().map_or(f.atlas.v.raw, |p| p.v.raw), self.linear),
            ],
            None,
            if let Some(t) = &tiles { [t.len().max(1) as u32, 1, 1] } else { [tx, ty, 1] },
            true,
        );
        // into the game's texture
        enc.segment(gpu, cmd, seg, "copy");
        let range = vk::ImageSubresourceLayers { aspect_mask: vk::ImageAspectFlags::COLOR, mip_level: 0, base_array_layer: 0, layer_count: 1 };
        let regions: Vec<vk::ImageCopy> = match &tiles {
            // only the tiles drawn (the rest of the texture is as it was: empty)
            // as runs along each row of tiles, a region each
            Some(t) => {
                let mut sorted = t.clone();
                sorted.sort_unstable();
                let mut out_r = Vec::new();
                let mut i = 0;
                while i < sorted.len() {
                    let (row, x0) = (sorted[i] / tx, sorted[i] % tx);
                    let mut x1 = x0;
                    while i + 1 < sorted.len() && sorted[i + 1] == sorted[i] + 1 && sorted[i + 1] / tx == row {
                        i += 1;
                        x1 = sorted[i] % tx;
                    }
                    i += 1;
                    let (x, y) = (x0 * super::draw::TILE, row * super::draw::TILE);
                    let off = vk::Offset3D { x: x as i32, y: y as i32, z: 0 };
                    let wd = ((x1 + 1) * super::draw::TILE).min(w) - x;
                    out_r.push(vk::ImageCopy { src_subresource: range, src_offset: off, dst_subresource: range, dst_offset: off, extent: vk::Extent3D { width: wd, height: super::draw::TILE.min(h - y), depth: 1 } });
                }
                out_r
            }
            None => vec![vk::ImageCopy { src_subresource: range, src_offset: vk::Offset3D::default(), dst_subresource: range, dst_offset: vk::Offset3D::default(), extent: vk::Extent3D { width: w, height: h, depth: 1 } }],
        };
        if !regions.is_empty() {
            unsafe { d.cmd_copy_image(cmd, s.pic.t.image, vk::ImageLayout::GENERAL, out.image, vk::ImageLayout::GENERAL, &regions) };
        }
        memory_barrier(gpu, cmd);
        enc.segment_end(gpu, cmd);
    }

    /// Lets the pictures made for a size go once the GPU is past them (the menu closed): the game should have that
    /// memory. The next frame makes them again.
    pub fn trim(&mut self, enc: &mut Encoder) {
        for layer in [0, 2] {
            if let Some(s) = self.sized[layer].take() {
                for img in std::iter::once(s.pic).chain(s.levels) {
                    retire(enc, img.t, img.v);
                }
            }
        }
    }

    /// At the end (the GPU idle).
    pub fn destroy(mut self, gpu: &Gpu) {
        if let Some(f) = self.fixed.take() {
            free_image(gpu, f.atlas);
            free_image(gpu, f.logo);
        }
        if let Some(p) = self.pics.take() {
            free_image(gpu, p);
        }
        if let Some(p) = self.dynfont.take() {
            free_image(gpu, p);
        }
        for s in self.sized.iter_mut().filter_map(Option::take) {
            for img in std::iter::once(s.pic).chain(s.levels) {
                free_image(gpu, img);
            }
        }
        for c in [self.gui, self.down, self.up] {
            c.destroy(gpu);
        }
        unsafe {
            gpu.device.destroy_sampler(self.linear, None);
            gpu.device.destroy_sampler(self.mipped, None);
        }
    }
}
