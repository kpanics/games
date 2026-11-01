//! Terrain occlusion culling on the GPU, in two parts per frame, so nothing hidden last frame stays missing:
//!
//! 1. Before the frame (the TERRAIN command): each opaque draw command of the frame's sections is there twice
//!    (vk/terrain.rs). The first copies are drawn only for sections visible last frame (shaders/occlusion_prep.comp);
//!    the second copies are not drawn yet.
//! 2. After the first part of the opaque terrain is in the depth buffer (the game's cutout layer): the pass is
//!    suspended, a depth pyramid is made from the depth buffer (occlusion_reduce.comp), every section's box is tested
//!    against it (occlusion_cull.comp) — the second copies of the visible sections not drawn yet are drawn then, and
//!    the translucent draws later only for visible sections. The results are next frame's "visible last frame".
//!
//! The pyramid holds only terrain drawn this frame, so moving entities never hide anything and a section revealed by
//! the camera's movement is drawn in the same frame. CURSED_DEV_NOOCCLUSION draws every section the game sends.
//!
//! It pays only where much is hidden (behind hills, in towns, among buildings): the pyramid and the test cost about as
//! much as a quarter of the opaque terrain. So it watches what it hides (the share of the tested opaque indices, read
//! back three frames later) and, when that stays under a quarter, rests for a second — every section drawn as the
//! game sends them — then looks again.

use super::device::Gpu;
use super::encoder::{memory_barrier, Encoder};
use super::memory::Place;
use super::resources::{Buffer, Doomed, Texture, View};
use ash::vk;
use std::sync::atomic::{AtomicBool, AtomicU32};

/// A compute pipeline with one push-descriptor set.
struct Compute {
    pipeline: vk::Pipeline,
    layout: vk::PipelineLayout,
    set_layout: vk::DescriptorSetLayout,
    module: vk::ShaderModule,
}

fn fail<T>(r: Result<T, vk::Result>, what: &str) -> T {
    r.unwrap_or_else(|e| panic!("{what}: {e:?}"))
}

impl Compute {
    fn new(gpu: &Gpu, name: &str, spirv: &[u32], kinds: &[vk::DescriptorType], push: u32) -> Compute {
        let d = &gpu.device;
        let module = fail(unsafe { d.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(spirv), None) }, "Can't make an occlusion shader");
        let bindings: Vec<vk::DescriptorSetLayoutBinding> = kinds
            .iter()
            .enumerate()
            .map(|(i, &k)| {
                vk::DescriptorSetLayoutBinding::default().binding(i as u32).descriptor_type(k).descriptor_count(1).stage_flags(vk::ShaderStageFlags::COMPUTE)
            })
            .collect();
        let set_info = vk::DescriptorSetLayoutCreateInfo::default().flags(vk::DescriptorSetLayoutCreateFlags::PUSH_DESCRIPTOR_KHR).bindings(&bindings);
        let set_layout = fail(unsafe { d.create_descriptor_set_layout(&set_info, None) }, "Can't make an occlusion descriptor layout");
        let ranges = [vk::PushConstantRange { stage_flags: vk::ShaderStageFlags::COMPUTE, offset: 0, size: push }];
        let set_layouts = [set_layout];
        let layout_info = vk::PipelineLayoutCreateInfo::default().set_layouts(&set_layouts).push_constant_ranges(&ranges);
        let layout = fail(unsafe { d.create_pipeline_layout(&layout_info, None) }, "Can't make an occlusion pipeline layout");
        let stage = vk::PipelineShaderStageCreateInfo::default().stage(vk::ShaderStageFlags::COMPUTE).module(module).name(c"main");
        let info = [vk::ComputePipelineCreateInfo::default().stage(stage).layout(layout)];
        let pipeline = unsafe { d.create_compute_pipelines(vk::PipelineCache::null(), &info, None) }.map(|v| v[0]).map_err(|(_, e)| e);
        let pipeline = fail(pipeline, "Can't make an occlusion pipeline");
        gpu.name(pipeline, &format!("Pipeline cursed:{name}"));
        Compute { pipeline, layout, set_layout, module }
    }

    /// Binds the pipeline and its descriptors (buffers whole, images with the sampler), pushes the constants.
    fn bind(&self, gpu: &Gpu, cmd: vk::CommandBuffer, writes: &[Binding], push: &[u8]) {
        let mut buffers = Vec::new();
        let mut images = Vec::new();
        for b in writes {
            match *b {
                Binding::Buffer(raw) => buffers.push(vk::DescriptorBufferInfo { buffer: raw, offset: 0, range: vk::WHOLE_SIZE }),
                Binding::Range(raw, offset, range) => buffers.push(vk::DescriptorBufferInfo { buffer: raw, offset, range }),
                Binding::Sampled(view, sampler) => {
                    images.push(vk::DescriptorImageInfo { sampler, image_view: view, image_layout: vk::ImageLayout::GENERAL })
                }
                Binding::Storage(view) => {
                    images.push(vk::DescriptorImageInfo { sampler: vk::Sampler::null(), image_view: view, image_layout: vk::ImageLayout::GENERAL })
                }
            }
        }
        let (mut bi, mut ii) = (0, 0);
        let sets: Vec<vk::WriteDescriptorSet> = writes
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let w = vk::WriteDescriptorSet::default().dst_binding(i as u32);
                match b {
                    Binding::Buffer(_) | Binding::Range(..) => {
                        bi += 1;
                        w.descriptor_type(vk::DescriptorType::STORAGE_BUFFER).buffer_info(std::slice::from_ref(&buffers[bi - 1]))
                    }
                    Binding::Sampled(..) => {
                        ii += 1;
                        w.descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(std::slice::from_ref(&images[ii - 1]))
                    }
                    Binding::Storage(_) => {
                        ii += 1;
                        w.descriptor_type(vk::DescriptorType::STORAGE_IMAGE).image_info(std::slice::from_ref(&images[ii - 1]))
                    }
                }
            })
            .collect();
        unsafe {
            gpu.device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.pipeline);
            gpu.push_d.cmd_push_descriptor_set(cmd, vk::PipelineBindPoint::COMPUTE, self.layout, 0, &sets);
            gpu.device.cmd_push_constants(cmd, self.layout, vk::ShaderStageFlags::COMPUTE, 0, push);
        }
    }

    fn destroy(&self, gpu: &Gpu) {
        let d = &gpu.device;
        unsafe {
            d.destroy_pipeline(self.pipeline, None);
            d.destroy_pipeline_layout(self.layout, None);
            d.destroy_descriptor_set_layout(self.set_layout, None);
            d.destroy_shader_module(self.module, None);
        }
    }
}

enum Binding {
    Buffer(vk::Buffer),
    /// A part of a buffer: offset, length.
    Range(vk::Buffer, u64, u64),
    Sampled(vk::ImageView, vk::Sampler),
    Storage(vk::ImageView),
}

fn words(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

/// The depth pyramid: R32F, level 0 half the depth buffer rounded up to powers of two (reads past the depth repeat its
/// edge), each level exactly half the one before.
struct Pyramid {
    texture: Option<Box<Texture>>,
    /// One single-level view per level (sampled when making the next, storage when made).
    levels: Vec<vk::ImageView>,
    /// All levels, for the test.
    all: vk::ImageView,
    /// The depth buffer's size it was made for.
    depth: (u32, u32),
    sizes: Vec<(u32, u32)>,
}

/// What the frame's terrain needs culled (vk/terrain.rs fills it at the TERRAIN command).
#[derive(Clone, Copy)]
pub struct Frame {
    pub commands: vk::Buffer,
    /// The first opaque command's word in `commands`; the second copies follow the first ones, then the translucent.
    pub command_base: u32,
    pub opaque: u32,
    pub translucent: u32,
    pub ids: vk::Buffer,
    pub id_base: u32,
    pub infos: vk::Buffer,
    pub info_base: u32,
    pub view_proj: [f32; 16],
    pub camera: [f64; 3],
    /// The test ran (the second copies may be drawn).
    pub tested: bool,
}

pub struct Occlusion {
    prep: Compute,
    reduce: Compute,
    cull: Compute,
    sampler: vk::Sampler,
    /// Visibility per mesh id: [this frame, last frame] swap at each frame's start.
    vis: [Option<Buffer>; 2],
    vis_len: u32,
    now: usize,
    pyramid: Option<Pyramid>,
    pub frame: Option<Frame>,
    /// Dev counts per frame, a slot of COUNT_SLOT bytes for each of the last COUNT_SLOTS frames (read back 3 later).
    counts: Option<Buffer>,
    frames: u64,
    /// The hidden share of tested opaque indices, smoothed over frames.
    share: f32,
    /// Frames tested since the last rest.
    tested: u32,
    rest_until: Option<std::time::Instant>,
    /// The opaque indices it hid, averaged as `share`.
    hidden: f32,
}

/// Under this share hidden, culling costs more than it saves.
const WORTH: f32 = 0.25;
const REST: std::time::Duration = std::time::Duration::from_secs(1);

const MARGIN: f32 = 1.0;
/// Levels past this cover boxes of over 2^(MAX_LEVELS) pixels: those count as visible (they are near and big).
const MAX_LEVELS: u32 = 8;
const COUNT_SLOT: u64 = 256;
const COUNT_SLOTS: u64 = 4;

impl Occlusion {
    /// `spirv`: prep, reduce, cull.
    pub fn new(gpu: &Gpu, spirv: [&[u32]; 3]) -> Occlusion {
        use vk::DescriptorType as T;
        let prep = Compute::new(gpu, "occlusion_prep", spirv[0], &[T::STORAGE_BUFFER, T::STORAGE_BUFFER, T::STORAGE_BUFFER], 16);
        let reduce = Compute::new(
            gpu,
            "occlusion_reduce",
            spirv[1],
            &[T::COMBINED_IMAGE_SAMPLER, T::STORAGE_IMAGE, T::STORAGE_IMAGE, T::STORAGE_IMAGE, T::STORAGE_IMAGE],
            32,
        );
        let cull = Compute::new(
            gpu,
            "occlusion_cull",
            spirv[2],
            &[T::STORAGE_BUFFER, T::STORAGE_BUFFER, T::STORAGE_BUFFER, T::STORAGE_BUFFER, T::STORAGE_BUFFER, T::COMBINED_IMAGE_SAMPLER, T::STORAGE_BUFFER],
            128,
        );
        let info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::NEAREST)
            .min_filter(vk::Filter::NEAREST)
            .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .max_lod(vk::LOD_CLAMP_NONE);
        let sampler = fail(unsafe { gpu.device.create_sampler(&info, None) }, "Can't make the occlusion sampler");
        let counts = Buffer::with(gpu, vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST, COUNT_SLOT * COUNT_SLOTS, Place::Readback)
            .unwrap_or_else(|e| panic!("Failed to allocate the occlusion counts: {e:?}"));
        Occlusion {
            prep,
            reduce,
            cull,
            sampler,
            vis: [None, None],
            vis_len: 0,
            now: 0,
            pyramid: None,
            frame: None,
            counts: Some(counts),
            frames: 0,
            share: 1.0,
            tested: 0,
            rest_until: None,
            hidden: 1.0e9,
        }
    }

    /// Whether this frame's terrain is culled (false while resting).
    pub fn wanted(&mut self) -> bool {
        match self.rest_until {
            Some(t) if std::time::Instant::now() < t => false,
            Some(_) => {
                self.rest_until = None;
                self.tested = 0;
                self.share = 1.0;
                self.hidden = 1.0e9;
                true
            }
            None => true,
        }
    }

    /// The frame's start: last frame's results become "last", the visibility grows to `ids` mesh ids, and the first
    /// copies of the opaque commands are switched on for sections visible last frame.
    pub fn begin(&mut self, gpu: &Gpu, enc: &mut Encoder, frame: Frame, ids: u32) {
        self.now ^= 1;
        self.frames += 1;
        if let Some(c) = self.counts.as_ref() {
            // The slot of three tested frames ago is done (at most two frames are in flight); this frame's starts at zero.
            if self.frames > COUNT_SLOTS {
                let old = (self.frames + 1) % COUNT_SLOTS;
                let p = unsafe { c.ptr().add((old * COUNT_SLOT) as usize) as *const u32 };
                let (tested, hidden, indices, hidden_indices) = unsafe { (*p, *p.add(1), *p.add(2), *p.add(3)) };
                super::stats::add(super::stats::OCCLUSION_TESTED, tested as u64 * 1000);
                super::stats::add(super::stats::OCCLUSION_HIDDEN, hidden as u64 * 1000);
                if indices > 0 {
                    self.share = self.share * 0.75 + 0.25 * (hidden_indices as f32 / indices as f32);
                    self.hidden = self.hidden * 0.75 + 0.25 * hidden_indices as f32;
                    self.tested += 1;
                    // Worth it when what it hides costs more than the pyramid and the test: measured on an RTX 4060
                    // at 1440p, the opaque terrain ~0.086 µs per thousand indices, the pyramid ~18 µs per million
                    // pixels and the test ~10 µs. In open land it hid a third of the terrain and still cost more.
                    let pixels = self.pyramid.as_ref().map_or(0.0, |p| p.depth.0 as f32 * p.depth.1 as f32);
                    let saves = self.hidden * 0.086e-3;
                    let costs = pixels * 18e-6 + 10.0;
                    // dev: CURSED_DEV_OCCLUSIONALWAYS never rests (what it would hide and cost)
                    static ALWAYS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
                    let always = *ALWAYS.get_or_init(|| std::env::var_os("CURSED_DEV_OCCLUSIONALWAYS").is_some());
                    if super::stats::passes() && self.frames % 2000 == 0 {
                        eprintln!("[occlusion] share hidden {:.2}, hidden indices {:.0} of {indices}, saves {saves:.0} us, costs {costs:.0} us", self.share, self.hidden);
                    }
                    if !always && self.tested >= 8 && (self.share < WORTH || saves < costs) {
                        self.rest_until = Some(std::time::Instant::now() + REST);
                    }
                }
            }
            let cmd = enc.cmd(gpu);
            unsafe { gpu.device.cmd_fill_buffer(cmd, c.raw, (self.frames % COUNT_SLOTS) * COUNT_SLOT, 16, 0) };
            memory_barrier(gpu, cmd);
        }
        if ids > self.vis_len {
            let len = ids.next_power_of_two().max(4096);
            let flags = vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST | vk::BufferUsageFlags::TRANSFER_SRC;
            for slot in 0..2 {
                let bigger = Buffer::with(gpu, flags, len as u64 * 4, Place::Device).unwrap_or_else(|e| panic!("Failed to allocate the occlusion state: {e:?}"));
                gpu.name(bigger.raw, "Cursed terrain visibility");
                let cmd = enc.cmd(gpu);
                unsafe { gpu.device.cmd_fill_buffer(cmd, bigger.raw, 0, vk::WHOLE_SIZE, 0) };
                memory_barrier(gpu, cmd);
                if let Some(old) = self.vis[slot].take() {
                    enc.copy_buffer(gpu, &old, 0, &bigger, 0, self.vis_len as u64 * 4);
                    enc.defer(Doomed::Buffer(Box::new(old)));
                }
                self.vis[slot] = Some(bigger);
            }
            self.vis_len = len;
        }
        if frame.opaque > 0 {
            let cmd = enc.cmd(gpu);
            enc.pass_started(gpu, cmd, "Terrain occlusion (first part)");
            let last = self.vis[self.now ^ 1].as_ref().unwrap().raw;
            self.prep.bind(
                gpu,
                cmd,
                &[Binding::Buffer(frame.commands), Binding::Buffer(frame.ids), Binding::Buffer(last)],
                &words(&[frame.command_base, frame.opaque, frame.id_base, self.vis_len]),
            );
            unsafe { gpu.device.cmd_dispatch(cmd, frame.opaque.div_ceil(64), 1, 1) };
            memory_barrier(gpu, cmd);
            enc.pass_ended(gpu, cmd);
        }
        self.frame = Some(frame);
    }

    /// Between the two parts of the opaque terrain, outside the suspended pass: the depth pyramid of `depth` and the
    /// test of every section. `cmd` is the pass's command buffer. Without a depth buffer every section is visible.
    pub fn test(&mut self, gpu: &Gpu, enc: &mut Encoder, cmd: vk::CommandBuffer, depth: Option<&View>) {
        let Some(mut frame) = self.frame else { return };
        let size = depth.map_or((2, 2), |d| (d.texture().width_at(d.base_mip), d.texture().height_at(d.base_mip)));
        if self.pyramid.as_ref().is_none_or(|p| depth.is_some() && p.depth != size) {
            if let Some(old) = self.pyramid.take() {
                Self::retire(enc, old);
            }
            let p = Self::make_pyramid(gpu, size);
            Self::first_layout(gpu, cmd, &p);
            self.pyramid = Some(p);
        }
        let pyramid = self.pyramid.as_ref().unwrap();
        enc.pass_started(gpu, cmd, "Terrain occlusion (pyramid)");
        let levels = if let Some(depth) = depth {
            // The pyramid, four levels a dispatch: from the depth buffer, then from the last level made.
            let n = pyramid.sizes.len();
            let mut src = (depth.raw, size);
            for first in (0..n).step_by(4) {
                let count = (n - first).min(4);
                let dst = pyramid.sizes[first];
                let push: Vec<u8> = [src.1 .0, src.1 .1, dst.0, dst.1, count as u32, 0, 0, 0].iter().flat_map(|v| v.to_le_bytes()).collect();
                let level = |k: usize| Binding::Storage(pyramid.levels[(first + k).min(n - 1)]);
                self.reduce.bind(gpu, cmd, &[Binding::Sampled(src.0, self.sampler), level(0), level(1), level(2), level(3)], &push);
                unsafe { gpu.device.cmd_dispatch(cmd, dst.0.div_ceil(16), dst.1.div_ceil(16), 1) };
                memory_barrier(gpu, cmd);
                src = (pyramid.levels[first + count - 1], pyramid.sizes[first + count - 1]);
            }
            n as u32
        } else {
            0
        };
        enc.pass_ended(gpu, cmd);
        enc.pass_started(gpu, cmd, "Terrain occlusion (test)");
        // The test: second copies of the opaque commands, then the translucent ones.
        let count = frame.opaque + frame.translucent;
        if count > 0 {
            let block = [frame.camera[0].floor(), frame.camera[1].floor(), frame.camera[2].floor()];
            let mut push = Vec::with_capacity(128);
            for v in frame.view_proj {
                push.extend_from_slice(&v.to_le_bytes());
            }
            for k in 0..3 {
                push.extend_from_slice(&((frame.camera[k] - block[k]) as f32).to_le_bytes());
            }
            push.extend_from_slice(&MARGIN.to_le_bytes());
            for b in block {
                push.extend_from_slice(&(b as i32).to_le_bytes());
            }
            push.extend_from_slice(&levels.to_le_bytes());
            let first_second = frame.command_base + frame.opaque * 5;
            for v in [first_second, frame.id_base, frame.info_base, count, self.vis_len, frame.opaque, size.0, size.1] {
                push.extend_from_slice(&v.to_le_bytes());
            }
            let now = self.vis[self.now].as_ref().unwrap().raw;
            let last = self.vis[self.now ^ 1].as_ref().unwrap().raw;
            self.cull.bind(
                gpu,
                cmd,
                &[
                    Binding::Buffer(frame.commands),
                    Binding::Buffer(frame.ids),
                    Binding::Buffer(frame.infos),
                    Binding::Buffer(last),
                    Binding::Buffer(now),
                    Binding::Sampled(pyramid.all, self.sampler),
                    Binding::Range(self.counts.as_ref().unwrap().raw, (self.frames % COUNT_SLOTS) * COUNT_SLOT, 16),
                ],
                &push,
            );
            unsafe { gpu.device.cmd_dispatch(cmd, count.div_ceil(64), 1, 1) };
            memory_barrier(gpu, cmd);
        }
        enc.pass_ended(gpu, cmd);
        frame.tested = true;
        self.frame = Some(frame);
    }

    fn make_pyramid(gpu: &Gpu, depth: (u32, u32)) -> Pyramid {
        let w = depth.0.div_ceil(2).max(1).next_power_of_two();
        let h = depth.1.div_ceil(2).max(1).next_power_of_two();
        let mips = (32 - w.max(h).leading_zeros()).min(MAX_LEVELS);
        let info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk::Format::R32_SFLOAT)
            .extent(vk::Extent3D { width: w, height: h, depth: 1 })
            .mip_levels(mips)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        let d = &gpu.device;
        let image = fail(unsafe { d.create_image(&info, None) }, "Can't make the depth pyramid");
        let req = unsafe { d.get_image_memory_requirements(image) };
        let alloc = fail(gpu.memory.allocate(d, req, Place::Device, false), "Can't allocate the depth pyramid");
        fail(unsafe { d.bind_image_memory(image, alloc.memory, alloc.offset) }, "Can't bind the depth pyramid");
        gpu.name(image, "Cursed depth pyramid");
        let view = |base: u32, count: u32| {
            let info = vk::ImageViewCreateInfo::default()
                .image(image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(vk::Format::R32_SFLOAT)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: base,
                    level_count: count,
                    base_array_layer: 0,
                    layer_count: 1,
                });
            fail(unsafe { d.create_image_view(&info, None) }, "Can't make a depth pyramid view")
        };
        let levels: Vec<vk::ImageView> = (0..mips).map(|m| view(m, 1)).collect();
        let all = view(0, mips);
        let sizes = (0..mips).map(|m| ((w >> m).max(1), (h >> m).max(1))).collect();
        // The layout every image of this renderer keeps: GENERAL, set once.
        let texture = Box::new(Texture {
            image,
            alloc: Some(alloc),
            format: vk::Format::R32_SFLOAT,
            aspect: vk::ImageAspectFlags::COLOR,
            width: w,
            height: h,
            layers: 1,
            mips,
            cube: false,
            closed: AtomicBool::new(true),
            views: AtomicU32::new(0),
        });
        Pyramid { texture: Some(texture), levels, all, depth, sizes }
    }

    /// A new pyramid's first (and only) layout, GENERAL.
    fn first_layout(gpu: &Gpu, cmd: vk::CommandBuffer, p: &Pyramid) {
        let Some(t) = &p.texture else { return };
        let barrier = [vk::ImageMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
            .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
            .dst_access_mask(vk::AccessFlags2::MEMORY_READ | vk::AccessFlags2::MEMORY_WRITE)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::GENERAL)
            .image(t.image)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: t.mips,
                base_array_layer: 0,
                layer_count: 1,
            })];
        unsafe { gpu.sync2_d.cmd_pipeline_barrier2(cmd, &vk::DependencyInfo::default().image_memory_barriers(&barrier)) };
    }

    fn retire(enc: &mut Encoder, mut p: Pyramid) {
        for v in p.levels.drain(..).chain(std::iter::once(p.all)) {
            enc.defer(Doomed::View(Box::new(View { raw: v, texture: std::ptr::null(), base_mip: 0 })));
        }
        if let Some(t) = p.texture.take() {
            enc.defer(Doomed::Texture(t));
        }
    }

    pub fn destroy(&mut self, gpu: &Gpu) {
        self.prep.destroy(gpu);
        self.reduce.destroy(gpu);
        self.cull.destroy(gpu);
        unsafe { gpu.device.destroy_sampler(self.sampler, None) };
        for b in self.vis.iter_mut().filter_map(Option::take) {
            Doomed::Buffer(Box::new(b)).destroy(gpu);
        }
        if let Some(c) = self.counts.take() {
            Doomed::Buffer(Box::new(c)).destroy(gpu);
        }
        if let Some(mut p) = self.pyramid.take() {
            for v in p.levels.drain(..).chain(std::iter::once(p.all)) {
                unsafe { gpu.device.destroy_image_view(v, None) };
            }
            if let Some(t) = p.texture.take() {
                Doomed::Texture(t).destroy(gpu);
            }
        }
    }
}
