//! A render pass: what the game draws into a set of textures, between `createRenderPass` and `submitRenderPass`.
//!
//! Java does not cross into Rust for every call of a pass. It writes the calls into a buffer (CursedPass) and hands
//! the buffer over when the pass ends or the buffer is full; `run` reads them in order and records them. Nothing in a
//! pass's calls needs an answer, so the game cannot tell the difference, and a frame of the world — thousands of
//! draws — crosses the border a few dozen times instead of tens of thousands.
//!
//! The calls and their bytes (little-endian; CursedPass writes the same list):
//!  1 pipeline     i64 pipeline
//!  2 uniform      i32 index, i32 kind: 0 none | 1 buffer i64 buffer, i64 offset, i64 length | 2 image i64 view, i64 sampler
//!  3 push         i32 length, the bytes, padded to 4
//!  4 scissor      i32 x, y, width, height
//!  5 no scissor
//!  6 vertices     i32 slot, i64 buffer, i64 offset
//!  7 indices      i64 buffer, i32 type (0 short, 1 int)
//!  8 drawIndexed  i32 count, instances, first index, vertex offset, first instance
//!  9 draw         i32 count, instances, first vertex, first instance
//! 10 multiDrawIndexed  i32 instances, first instance, draws, then per draw i32 first index, count, vertex offset
//! 11 multiDraw    i32 instances, first instance, draws, then per draw i32 first vertex, count
//! 12 drawIndexedIndirect  i64 buffer, i64 offset, i32 draws
//! 13 drawIndirect i64 buffer, i64 offset, i32 draws
//! 14 push group   string (i32 length, UTF-8, zeros to a multiple of 4)
//! 15 pop group
//! 16 timestamp    i64 queries, i32 index
//! 17 terrain      i32 layer: the frame's terrain of that layer (vk/terrain.rs), with the pipeline already set. After
//!                 the cutout layer (the last opaque one) the pass is suspended for the occlusion test
//!                 (vk/occlusion.rs) and resumed, and the second part of both opaque layers is drawn.

use super::device::Gpu;
use super::encoder::{fragment_barrier, memory_barrier, write_timestamp, Encoder};
use super::pipeline::{Pipeline, Reader, Uniform, PREPASS, PREPASS_CUTOUT, SHADE};
use super::resources::{Buffer, Doomed, Queries, Sampler, Texture, View};
use super::terrain::{Part, Terrain, FAR_TRANSLUCENT};
use ash::vk;

#[derive(Clone, Copy)]
pub enum Value {
    None,
    Buffer { buffer: *const Buffer, offset: u64, length: u64 },
    Image { view: *const View, sampler: *const Sampler },
}

pub struct Pass {
    cmd: vk::CommandBuffer,
    label: String,
    has_depth: bool,
    area: vk::Rect2D,
    pipeline: *const Pipeline,
    uniforms: Vec<Value>,
    dirty: bool,
    groups: u32,
    /// What it renders into, to resume after a suspension: colour views (null where absent), the depth view.
    colors: Vec<vk::ImageView>,
    /// The first colour attachment (the engine copies it before water for reflections).
    color0: Option<*const View>,
    depth: Option<*const View>,
    size: (u32, u32),
    scissor: vk::Rect2D,
    /// The solid terrain layer's pipeline and uniforms, for its second part after the cutout layer.
    solid: Option<(*const Pipeline, Vec<Value>)>,
    /// What the engine's uniforms (named Cursed…) are bound to (engine/gpu.rs); the frame thread's, outlives the pass.
    engine: *mut crate::engine::gpu::Engine,
    /// The opaque terrain's depth went first this pass (the engine's depth pass): its lit pass tests for equality.
    prepassed: bool,
    /// Dev (CURSED_DEV_SKIP): the current pipeline's draws are left out.
    skip: bool,
    /// The textures it draws into (Encoder::unsettled once it ends).
    drawn: Vec<*const Texture>,
    /// The depth attachment's clear as the pass began, if it was cleared.
    pub cleared_depth: Option<f32>,
    /// The sky's calls (frame.rs holds its pass): drawn behind this pass's opaque world (sky_behind).
    pub late_sky: Option<Vec<u8>>,
    /// Drawing the sky behind (its pipelines' `behind` variants).
    behind: bool,
    /// The picture so far was taken for the chams this pass (chams_before_draw).
    chams_taken: bool,
    /// The vertex and index buffers bound, and the push constants: put back after the sky.
    vertex_bound: Vec<(u32, vk::Buffer, u64)>,
    index_bound: Option<(vk::Buffer, vk::IndexType)>,
    pushed: Vec<u8>,
}

const SOLID: usize = 0;
const CUTOUT: usize = 1;
const TRANSLUCENT: usize = 2;

/// The pass's command buffer (the same one across a suspension).
fn gpu_cmd(p: &Pass) -> vk::CommandBuffer {
    p.cmd
}

fn view<'a>(p: i64) -> &'a View {
    unsafe { &*(p as usize as *const View) }
}

fn label(gpu: &Gpu, cmd: vk::CommandBuffer, text: &str) {
    if let Some(d) = gpu.labels() {
        if let Ok(c) = std::ffi::CString::new(text.replace('\0', " ")) {
            let l = vk::DebugUtilsLabelEXT::default().label_name(&c);
            unsafe { d.cmd_begin_debug_utils_label(cmd, &l) };
        }
    }
}

fn unlabel(gpu: &Gpu, cmd: vk::CommandBuffer) {
    if let Some(d) = gpu.labels() {
        unsafe { d.cmd_end_debug_utils_label(cmd) };
    }
}

impl Pass {
    /// Starts a pass from its description (CursedEncoder.createRenderPass packs it):
    /// label; colour attachments (i32 count, each i32 present, i64 view, i32 cleared, f32 r g b a);
    /// depth (i32 present, i64 view, i32 cleared, f64 depth); render area i32 x y width height.
    pub fn begin(gpu: &Gpu, enc: &mut Encoder, desc: &[u8], engine: *mut crate::engine::gpu::Engine) -> Pass {
        // The render area comes last; the attachments take the clears waiting for them (Encoder::take_clear).
        let area = {
            let mut a = Reader::new(desc);
            a.str();
            let n = a.i32();
            for _ in 0..n {
                a.i32();
                a.i64();
                a.i32();
                for _ in 0..4 {
                    a.f32();
                }
            }
            if a.i32() != 0 {
                a.i64();
                a.i32();
                a.f64();
            }
            vk::Rect2D { offset: vk::Offset2D { x: a.i32(), y: a.i32() }, extent: vk::Extent2D { width: a.i32().max(0) as u32, height: a.i32().max(0) as u32 } }
        };
        let mut r = Reader::new(desc);
        let text = r.str();
        // Taken before the pass's command buffer. A clear of an attachment it cannot take is recorded first, as it was
        // asked; the others wait on for the passes that draw into their textures (Improved Transparency's targets for
        // its passes after the opaque one: four clears of their own and a barrier cost the GPU some 16 µs at 1440p,
        // 12.10), unless this pass samples one (push_descriptors).
        let mut taken: Vec<(*const super::resources::Texture, super::encoder::PendingClear)> = Vec::new();
        if !enc.pending_clears.is_empty() {
            let mut peek = Reader::new(desc);
            peek.str();
            let n = peek.i32();
            let mut views = Vec::new();
            for _ in 0..n {
                let present = peek.i32() != 0;
                let v = peek.i64();
                peek.i32();
                for _ in 0..4 {
                    peek.f32();
                }
                if present {
                    views.push(v);
                }
            }
            if peek.i32() != 0 {
                views.push(peek.i64());
            }
            let mut attached = Vec::new();
            for v in views {
                if let Some(p) = enc.take_clear(view(v), area) {
                    taken.push((view(v).texture() as *const _, p));
                }
                attached.push(view(v).texture() as *const Texture);
            }
            enc.flush_clears_of(gpu, &attached);
        }
        let waiting = |t: *const super::resources::Texture| taken.iter().find(|(at, _)| std::ptr::eq(*at, t)).map(|(_, p)| p);
        // only render passes since the last full barrier: their attachments are waited for by the fragments
        let after_pass = std::mem::take(&mut enc.after_pass);
        let after_models = std::mem::take(&mut enc.after_models);
        let cmd = enc.cmd(gpu);
        if after_pass && enc.unsettled.iter().any(|u| enc.vertex_read.contains(u)) {
            memory_barrier(gpu, cmd);
            enc.settled();
        } else if after_pass || after_models {
            // the passes' attachments for the fragments, the models' vertices (models.rs) for the vertex input
            fragment_barrier(gpu, cmd, after_pass, after_models);
        }
        let mut drawn = Vec::new();
        let mut colors = Vec::new();
        let mut color_views = Vec::new();
        let mut color0: Option<*const View> = None;
        let mut out = (0u32, 0u32);
        let n = r.i32();
        for _ in 0..n {
            let present = r.i32() != 0;
            let v = r.i64();
            let cleared = r.i32() != 0;
            let rgba = [r.f32(), r.f32(), r.f32(), r.f32()];
            color_views.push(if present { view(v).raw } else { vk::ImageView::null() });
            if present && color0.is_none() {
                color0 = Some(view(v) as *const View);
            }
            if present {
                let v = view(v);
                drawn.push(v.texture() as *const Texture);
                out = (v.texture().width_at(v.base_mip), v.texture().height_at(v.base_mip));
                // the pass's own clear, else one waiting for it
                let (cleared, rgba) = match waiting(v.texture() as *const _).and_then(|p| p.color) {
                    Some(c) if !cleared => (true, c),
                    _ => (cleared, rgba),
                };
                let mut a = vk::RenderingAttachmentInfo::default()
                    .image_view(v.raw)
                    .image_layout(vk::ImageLayout::GENERAL)
                    .store_op(vk::AttachmentStoreOp::STORE)
                    .load_op(if cleared { vk::AttachmentLoadOp::CLEAR } else { vk::AttachmentLoadOp::LOAD });
                if cleared {
                    a = a.clear_value(vk::ClearValue { color: vk::ClearColorValue { float32: rgba } });
                }
                colors.push(a);
            } else {
                colors.push(
                    vk::RenderingAttachmentInfo::default()
                        .image_view(vk::ImageView::null())
                        .image_layout(vk::ImageLayout::UNDEFINED)
                        .load_op(vk::AttachmentLoadOp::DONT_CARE)
                        .store_op(vk::AttachmentStoreOp::DONT_CARE),
                );
            }
        }
        let has_depth = r.i32() != 0;
        let mut depth = vk::RenderingAttachmentInfo::default();
        let mut depth_view = None;
        let mut cleared_depth = None;
        if has_depth {
            let v = view(r.i64());
            depth_view = Some(v as *const View);
            drawn.push(v.texture() as *const Texture);
            let cleared = r.i32() != 0;
            let value = r.f64();
            let (cleared, value) = match waiting(v.texture() as *const _).and_then(|p| p.depth) {
                Some(d) if !cleared => (true, d as f64),
                _ => (cleared, value),
            };
            if n == 0 {
                out = (v.texture().width_at(v.base_mip), v.texture().height_at(v.base_mip));
            }
            depth = depth
                .image_view(v.raw)
                .image_layout(vk::ImageLayout::GENERAL)
                .store_op(vk::AttachmentStoreOp::STORE)
                .load_op(if cleared { vk::AttachmentLoadOp::CLEAR } else { vk::AttachmentLoadOp::LOAD });
            if cleared {
                depth = depth.clear_value(vk::ClearValue { depth_stencil: vk::ClearDepthStencilValue { depth: value as f32, stencil: 0 } });
                cleared_depth = Some(value as f32);
            }
        }
        let area = vk::Rect2D {
            offset: vk::Offset2D { x: r.i32(), y: r.i32() },
            extent: vk::Extent2D { width: r.i32().max(0) as u32, height: r.i32().max(0) as u32 },
        };
        enc.pass_started(gpu, cmd, &text);
        label(gpu, cmd, &text);
        let mut info = vk::RenderingInfo::default().render_area(area).layer_count(1).view_mask(0).color_attachments(&colors);
        if has_depth {
            info = info.depth_attachment(&depth);
        }
        unsafe {
            gpu.render_d.cmd_begin_rendering(cmd, &info);
            let viewport = vk::Viewport { x: 0.0, y: 0.0, width: out.0 as f32, height: out.1 as f32, min_depth: 0.0, max_depth: 1.0 };
            gpu.device.cmd_set_viewport(cmd, 0, &[viewport]);
            gpu.device.cmd_set_scissor(cmd, 0, &[area]);
        }
        enc.in_pass = true;
        Pass {
            cmd,
            label: text,
            has_depth,
            area,
            pipeline: std::ptr::null(),
            uniforms: Vec::new(),
            dirty: false,
            groups: 0,
            colors: color_views,
            color0,
            depth: depth_view,
            size: out,
            scissor: area,
            solid: None,
            engine,
            prepassed: false,
            skip: false,
            drawn,
            cleared_depth,
            late_sky: None,
            behind: false,
            chams_taken: false,
            vertex_bound: Vec::new(),
            index_bound: None,
            pushed: Vec::new(),
        }
    }

    /// Whether a pass described so would draw into exactly what this one does, over the same area, nothing cleared:
    /// then it goes on in this one (frame.rs, `parked`).
    pub fn same_targets(&self, desc: &[u8], enc: &Encoder) -> bool {
        if self.late_sky.is_some() {
            return false;
        }
        let mut r = Reader::new(desc);
        r.str();
        let n = r.i32().max(0) as usize;
        if n != self.colors.len() {
            return false;
        }
        for k in 0..n {
            let present = r.i32() != 0;
            let v = r.i64();
            let cleared = r.i32() != 0;
            for _ in 0..4 {
                r.f32();
            }
            let raw = if present { view(v).raw } else { vk::ImageView::null() };
            if cleared || raw != self.colors[k] || (present && enc.clear_waits(view(v).texture)) {
                return false;
            }
        }
        let has_depth = r.i32() != 0;
        if has_depth != self.depth.is_some() {
            return false;
        }
        if has_depth {
            let v = r.i64();
            let cleared = r.i32() != 0;
            r.f64();
            if cleared || self.depth.is_some_and(|d| unsafe { (*d).raw } != view(v).raw) || enc.clear_waits(view(v).texture) {
                return false;
            }
        }
        let area = [r.i32(), r.i32(), r.i32(), r.i32()];
        area == [self.area.offset.x, self.area.offset.y, self.area.extent.width as i32, self.area.extent.height as i32]
    }

    /// Goes on as the next pass of the game's (`same_targets`): its label, nothing bound. No barrier between them, no
    /// end and new start of the rendering — in a pass the GPU keeps the draws' order itself (12.10: Improved
    /// Transparency's terrain and features passes of each stage).
    pub fn go_on_as(&mut self, gpu: &Gpu, enc: &mut Encoder, desc: &[u8]) {
        let text = Reader::new(desc).str();
        enc.segment_end(gpu, self.cmd);
        for _ in 0..self.groups {
            unlabel(gpu, self.cmd);
        }
        self.groups = 0;
        unlabel(gpu, self.cmd);
        label(gpu, self.cmd, &text);
        self.label = text;
        self.pipeline = std::ptr::null();
        self.uniforms.clear();
        self.dirty = true;
        self.skip = false;
        self.solid = None;
        self.prepassed = false;
        self.scissor = self.area;
        unsafe { gpu.device.cmd_set_scissor(self.cmd, 0, &[self.area]) };
    }

    /// Records calls held from another pass (frame.rs: the sky's, when it cannot wait).
    pub fn replay(&mut self, gpu: &Gpu, enc: &mut Encoder, terrain: &mut Terrain, ops: &[u8]) {
        let mut r = Reader::new(ops);
        while !r.done() {
            let op = r.i32();
            self.call(op, &mut r, gpu, enc, terrain);
        }
    }

    /// The sky (`late_sky`), behind what the pass drew so far — all of it opaque and in the depth (draws that are not
    /// bring it here first, as do the impostor, the translucent terrain and the pass's end): with depth tested for
    /// equality with its clear, at that depth, so it fills only what is still empty. The game draws it first, over
    /// the whole screen, and the world over most of it (some 25 µs at 1440p on an RTX 4060, 12.10); the picture is the
    /// same, the sky never being in the depth.
    fn sky_behind(&mut self, gpu: &Gpu, enc: &mut Encoder, terrain: &mut Terrain) {
        let Some(ops) = self.late_sky.take() else { return };
        let depth = self.cleared_depth.unwrap_or(0.0);
        let saved = (self.pipeline, std::mem::take(&mut self.uniforms), self.scissor, self.skip);
        let bound = (self.vertex_bound.clone(), self.index_bound, self.pushed.clone());
        let d = &gpu.device;
        let (w, h) = (self.size.0 as f32, self.size.1 as f32);
        unsafe { d.cmd_set_viewport(self.cmd, 0, &[vk::Viewport { x: 0.0, y: 0.0, width: w, height: h, min_depth: depth, max_depth: depth }]) };
        self.behind = true;
        self.replay(gpu, enc, terrain, &ops);
        self.behind = false;
        (self.pipeline, self.uniforms, self.scissor, self.skip) = saved;
        (self.vertex_bound, self.index_bound, self.pushed) = bound;
        unsafe {
            d.cmd_set_viewport(self.cmd, 0, &[vk::Viewport { x: 0.0, y: 0.0, width: w, height: h, min_depth: 0.0, max_depth: 1.0 }]);
            d.cmd_set_scissor(self.cmd, 0, &[self.scissor]);
            for &(slot, b, o) in &self.vertex_bound {
                d.cmd_bind_vertex_buffers(self.cmd, slot, &[b], &[o]);
            }
            if let Some((b, kind)) = self.index_bound {
                d.cmd_bind_index_buffer(self.cmd, b, 0, kind);
            }
        }
        if !self.pipeline.is_null() {
            self.bind_pipeline(gpu, self.pipeline);
            if !self.pushed.is_empty() {
                let layout = self.pipeline().layout;
                unsafe { d.cmd_push_constants(self.cmd, layout, vk::ShaderStageFlags::ALL, 0, &self.pushed) };
            }
            let p = unsafe { &*self.pipeline };
            enc.segment(gpu, self.cmd, &self.label, &p.name);
        }
    }

    /// Before a draw that is not opaque or not in the depth: the sky first.
    fn sky_before_draw(&mut self, gpu: &Gpu, enc: &mut Encoder, terrain: &mut Terrain) {
        if self.late_sky.is_some() && !self.behind && !self.pipeline().opaque {
            self.sky_behind(gpu, enc, terrain);
        }
    }

    /// Before the first draw of a creature's chams in the pass (cursed.recode.chams): the picture and its depth so far
    /// (the engine's CursedScene), what is behind them — their negative, their lens, their liquid metal's reflection.
    /// The sky held behind the opaque world goes in first: the chams are opaque, so it would come only after them.
    fn chams_before_draw(&mut self, gpu: &Gpu, enc: &mut Encoder, terrain: &mut Terrain) {
        if self.chams_taken || self.pipeline.is_null() || !self.pipeline().chams {
            return;
        }
        self.chams_taken = true;
        if self.late_sky.is_some() && !self.behind {
            self.sky_behind(gpu, enc, terrain);
        }
        if let (Some(color), Some(depth)) = (self.color0, self.depth) {
            self.suspend(gpu, enc);
            unsafe { &mut *self.engine }.capture(gpu, enc, self.cmd, unsafe { &*color }, unsafe { &*depth });
            self.resume(gpu, enc);
        }
    }

    /// Ends the rendering for work outside it (compute), keeping everything drawn.
    fn suspend(&mut self, gpu: &Gpu, enc: &mut Encoder) {
        enc.segment_end(gpu, self.cmd);
        unsafe { gpu.render_d.cmd_end_rendering(self.cmd) };
        memory_barrier(gpu, self.cmd);
        enc.settled();
    }

    /// Renders on into the same attachments, loading what is there.
    fn resume(&mut self, gpu: &Gpu, enc: &mut Encoder) {
        memory_barrier(gpu, self.cmd);
        enc.settled();
        let load = |v: vk::ImageView| {
            if v == vk::ImageView::null() {
                vk::RenderingAttachmentInfo::default()
                    .image_view(v)
                    .image_layout(vk::ImageLayout::UNDEFINED)
                    .load_op(vk::AttachmentLoadOp::DONT_CARE)
                    .store_op(vk::AttachmentStoreOp::DONT_CARE)
            } else {
                vk::RenderingAttachmentInfo::default()
                    .image_view(v)
                    .image_layout(vk::ImageLayout::GENERAL)
                    .load_op(vk::AttachmentLoadOp::LOAD)
                    .store_op(vk::AttachmentStoreOp::STORE)
            }
        };
        let colors: Vec<vk::RenderingAttachmentInfo> = self.colors.iter().map(|&v| load(v)).collect();
        let depth = self.depth.map(|v| load(unsafe { (*v).raw }));
        let mut info = vk::RenderingInfo::default().render_area(self.area).layer_count(1).view_mask(0).color_attachments(&colors);
        if let Some(d) = depth.as_ref() {
            info = info.depth_attachment(d);
        }
        unsafe {
            gpu.render_d.cmd_begin_rendering(self.cmd, &info);
            let viewport = vk::Viewport { x: 0.0, y: 0.0, width: self.size.0 as f32, height: self.size.1 as f32, min_depth: 0.0, max_depth: 1.0 };
            gpu.device.cmd_set_viewport(self.cmd, 0, &[viewport]);
            gpu.device.cmd_set_scissor(self.cmd, 0, &[self.scissor]);
        }
        self.dirty = true;
        if !self.pipeline.is_null() {
            let p = unsafe { &*self.pipeline };
            enc.segment(gpu, self.cmd, &self.label, &p.name);
        }
    }

    fn bind_pipeline(&mut self, gpu: &Gpu, p: *const Pipeline) {
        self.pipeline = p;
        let p = unsafe { &*p };
        let handle = p.handle(self.has_depth, unsafe { &*self.engine }.vanilla());
        unsafe { gpu.device.cmd_bind_pipeline(self.cmd, vk::PipelineBindPoint::GRAPHICS, handle) };
        self.dirty = true;
    }

    /// After the first part of the opaque terrain (the game's cutout layer is current): the occlusion test outside the
    /// rendering, then the second part of cutout, and of solid with its own pipeline and uniforms, then cutout's again.
    fn terrain_second_part(&mut self, gpu: &Gpu, enc: &mut Encoder, terrain: &mut Terrain) {
        self.suspend(gpu, enc);
        let depth = self.depth.map(|v| unsafe { &*v });
        terrain.test(gpu, enc, self.cmd, depth);
        self.resume(gpu, enc);
        let max = gpu.info.max_draw_indirect as u32;
        self.push_descriptors(gpu, enc);
        terrain.draw_second(gpu, self.cmd, CUTOUT, max);
        if let Some((solid, uniforms)) = self.solid.take() {
            let (cutout, cutout_uniforms) = (self.pipeline, std::mem::replace(&mut self.uniforms, uniforms));
            self.bind_pipeline(gpu, solid);
            self.push_descriptors(gpu, enc);
            terrain.draw_second(gpu, self.cmd, SOLID, max);
            self.uniforms = cutout_uniforms;
            self.bind_pipeline(gpu, cutout);
        }
    }

    pub fn end(mut self, gpu: &Gpu, enc: &mut Encoder, terrain: &mut Terrain) {
        self.sky_behind(gpu, enc, terrain);
        enc.segment_end(gpu, self.cmd);
        unsafe { gpu.render_d.cmd_end_rendering(self.cmd) };
        for _ in 0..self.groups {
            unlabel(gpu, self.cmd);
        }
        unlabel(gpu, self.cmd);
        enc.pass_ended(gpu, self.cmd);
        // the barrier waits for what comes next (Encoder::after_pass)
        enc.unsettled.extend_from_slice(&self.drawn);
        enc.after_pass = true;
        enc.in_pass = false;
    }

    fn pipeline(&self) -> &Pipeline {
        assert!(!self.pipeline.is_null(), "Draw in pass {} without a pipeline", self.label);
        unsafe { &*self.pipeline }
    }

    /// Records one call Java wrote; `op` was read already, its fields follow in `r`.
    pub fn call(&mut self, op: i32, r: &mut Reader, gpu: &Gpu, enc: &mut Encoder, terrain: &mut Terrain) {
        // dev: CURSED_DEV_PASSDUMP=label prints that pass's calls, one frame in 4000
        static DUMP: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
        if let Some(l) = DUMP.get_or_init(|| std::env::var("CURSED_DEV_PASSDUMP").ok()) {
            if self.label.contains(l.as_str()) && enc.current % 4000 == 0 {
                let mut q = *r;
                let args: Vec<i64> = match op {
                    1 => vec![q.i64()],
                    6 => vec![q.i32() as i64, q.i64(), q.i64()],
                    7 => vec![q.i64(), q.i32() as i64],
                    8 => (0..5).map(|_| q.i32() as i64).collect(),
                    9 => (0..4).map(|_| q.i32() as i64).collect(),
                    10 | 11 => (0..3).map(|_| q.i32() as i64).collect(),
                    _ => vec![],
                };
                let name = if op == 1 { unsafe { (*(args[0] as usize as *const Pipeline)).name.clone() } } else { String::new() };
                eprintln!("[passdump] {} op {op} {args:?} {name}", self.label);
            }
        }
        let cmd = self.cmd;
        let d = &gpu.device;
        {
            match op {
                1 => {
                    let p = r.i64() as usize as *const Pipeline;
                    self.pipeline = p;
                    let p = unsafe { &*p };
                    let far_at_one = self.cleared_depth == Some(1.0);
                    let handle = if self.behind && p.behind[0] != vk::Pipeline::null() { p.behind[far_at_one as usize] } else { p.handle(self.has_depth, unsafe { &*self.engine }.vanilla()) };
                    assert!(handle != vk::Pipeline::null(), "Pipeline {} tests depth but pass {} has no depth attachment", p.name, self.label);
                    self.uniforms.clear();
                    self.uniforms.resize(p.uniforms.len(), Value::None);
                    self.dirty = true;
                    self.skip = dev_skip(&p.name) || cheap_oit_skip(&p.name);
                    enc.segment(gpu, cmd, &self.label, &p.name);
                    unsafe { d.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, handle) };
                }
                2 => {
                    let index = r.i32() as usize;
                    let value = match r.i32() {
                        1 => Value::Buffer { buffer: r.i64() as usize as *const Buffer, offset: r.i64() as u64, length: r.i64() as u64 },
                        2 => Value::Image { view: r.i64() as usize as *const View, sampler: r.i64() as usize as *const Sampler },
                        _ => Value::None,
                    };
                    if index < self.uniforms.len() {
                        self.uniforms[index] = value;
                    }
                    self.dirty = true;
                }
                3 => {
                    let n = r.i32().max(0) as usize;
                    let bytes = r.bytes(n.div_ceil(4) * 4);
                    let layout = self.pipeline().layout;
                    unsafe { d.cmd_push_constants(cmd, layout, vk::ShaderStageFlags::ALL, 0, &bytes[..n]) };
                    self.pushed.clear();
                    self.pushed.extend_from_slice(&bytes[..n]);
                }
                4 => {
                    let rect = vk::Rect2D {
                        offset: vk::Offset2D { x: r.i32(), y: r.i32() },
                        extent: vk::Extent2D { width: r.i32().max(0) as u32, height: r.i32().max(0) as u32 },
                    };
                    self.scissor = rect;
                    unsafe { d.cmd_set_scissor(cmd, 0, &[rect]) };
                }
                5 => {
                    self.scissor = self.area;
                    unsafe { d.cmd_set_scissor(cmd, 0, &[self.area]) }
                }
                6 => {
                    let slot = r.u32();
                    let b = unsafe { &*(r.i64() as usize as *const Buffer) };
                    let offset = r.i64() as u64;
                    unsafe { d.cmd_bind_vertex_buffers(cmd, slot, &[b.raw], &[offset]) };
                    self.vertex_bound.retain(|v| v.0 != slot);
                    self.vertex_bound.push((slot, b.raw, offset));
                }
                7 => {
                    let b = unsafe { &*(r.i64() as usize as *const Buffer) };
                    let kind = if r.i32() == 0 { vk::IndexType::UINT16 } else { vk::IndexType::UINT32 };
                    unsafe { d.cmd_bind_index_buffer(cmd, b.raw, 0, kind) };
                    self.index_bound = Some((b.raw, kind));
                }
                8 => {
                    let (count, instances, first, vertex_offset, first_instance) = (r.u32(), r.u32(), r.u32(), r.i32(), r.u32());
                    if self.skip {
                        return;
                    }
                    self.sky_before_draw(gpu, enc, terrain);
                    self.chams_before_draw(gpu, enc, terrain);
                    self.push_descriptors(gpu, enc);
                    unsafe { d.cmd_draw_indexed(cmd, count, instances, first, vertex_offset, first_instance) };
                }
                9 => {
                    let (count, instances, first, first_instance) = (r.u32(), r.u32(), r.u32(), r.u32());
                    if self.skip {
                        return;
                    }
                    self.sky_before_draw(gpu, enc, terrain);
                    self.chams_before_draw(gpu, enc, terrain);
                    self.push_descriptors(gpu, enc);
                    unsafe { d.cmd_draw(cmd, count, instances, first, first_instance) };
                }
                10 => {
                    let (instances, first_instance, draws) = (r.u32(), r.u32(), r.u32());
                    let bytes = r.bytes(draws as usize * 12);
                    if self.skip {
                        return;
                    }
                    self.sky_before_draw(gpu, enc, terrain);
                    self.chams_before_draw(gpu, enc, terrain);
                    self.push_descriptors(gpu, enc);
                    let md = gpu.multi_draw_d.as_ref().expect("multiDrawIndexed without VK_EXT_multi_draw");
                    unsafe {
                        (md.fp().cmd_draw_multi_indexed_ext)(
                            cmd,
                            draws,
                            bytes.as_ptr() as *const vk::MultiDrawIndexedInfoEXT,
                            instances,
                            first_instance,
                            12,
                            std::ptr::null(),
                        )
                    };
                }
                11 => {
                    let (instances, first_instance, draws) = (r.u32(), r.u32(), r.u32());
                    let bytes = r.bytes(draws as usize * 8);
                    if self.skip {
                        return;
                    }
                    self.sky_before_draw(gpu, enc, terrain);
                    self.chams_before_draw(gpu, enc, terrain);
                    self.push_descriptors(gpu, enc);
                    let md = gpu.multi_draw_d.as_ref().expect("multiDraw without VK_EXT_multi_draw");
                    unsafe {
                        (md.fp().cmd_draw_multi_ext)(cmd, draws, bytes.as_ptr() as *const vk::MultiDrawInfoEXT, instances, first_instance, 8)
                    };
                }
                12 => {
                    let b = unsafe { &*(r.i64() as usize as *const Buffer) };
                    let (offset, draws) = (r.i64() as u64, r.u32());
                    if self.skip {
                        return;
                    }
                    self.sky_before_draw(gpu, enc, terrain);
                    self.chams_before_draw(gpu, enc, terrain);
                    self.push_descriptors(gpu, enc);
                    unsafe { d.cmd_draw_indexed_indirect(cmd, b.raw, offset, draws, 20) };
                }
                13 => {
                    let b = unsafe { &*(r.i64() as usize as *const Buffer) };
                    let (offset, draws) = (r.i64() as u64, r.u32());
                    if self.skip {
                        return;
                    }
                    self.sky_before_draw(gpu, enc, terrain);
                    self.chams_before_draw(gpu, enc, terrain);
                    self.push_descriptors(gpu, enc);
                    unsafe { d.cmd_draw_indirect(cmd, b.raw, offset, draws, 16) };
                }
                14 => {
                    let text = r.str();
                    self.groups += 1;
                    label(gpu, cmd, &text);
                }
                15 => {
                    assert!(self.groups > 0, "Can't pop more debug groups than was pushed!");
                    self.groups -= 1;
                    unlabel(gpu, cmd);
                }
                16 => {
                    let q = unsafe { &*(r.i64() as usize as *const Queries) };
                    let index = r.u32();
                    write_timestamp(gpu, cmd, q, index);
                }
                17 => {
                    let raw = r.i32() as usize;
                    // the far translucent sections, classic, in the opaque pass (Improved Transparency: FrameDraws::far_translucent)
                    let (layer, part) = match raw {
                        FAR_TRANSLUCENT => {
                            terrain.frame.far_drawn = true;
                            (TRANSLUCENT, Part::Far)
                        }
                        TRANSLUCENT if terrain.frame.far_drawn => (TRANSLUCENT, Part::Near),
                        l => (l, Part::All),
                    };
                    self.pipeline();
                    if self.skip {
                        return;
                    }
                    if layer == TRANSLUCENT {
                        self.sky_behind(gpu, enc, terrain);
                    }
                    if layer == SOLID || layer == CUTOUT {
                        self.capture_terrain(terrain, layer);
                    }
                    let max = gpu.info.max_draw_indirect as u32;
                    let p = unsafe { &*self.pipeline };
                    let engine = unsafe { &mut *self.engine };
                    let variants = (p.variant(PREPASS), p.variant(PREPASS_CUTOUT), p.variant(SHADE));
                    match (layer, variants) {
                        (SOLID, (Some(depth_solid), Some(depth_cutout), Some(shade))) if self.has_depth && engine.prepass() => {
                            // The engine's opaque terrain: both layers' depth first, the shadow mask from it, then each
                            // pixel lit once (the layers tested for equal depth).
                            enc.segment(gpu, cmd, &self.label, "engine depth");
                            unsafe { d.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, depth_solid) };
                            self.dirty = true;
                            self.push_descriptors(gpu, enc);
                            terrain.draw(gpu, cmd, SOLID, max);
                            unsafe { d.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, depth_cutout) };
                            terrain.draw(gpu, cmd, CUTOUT, max);
                            self.suspend(gpu, enc);
                            if let Some(depth) = self.depth {
                                engine.make_mask(gpu, enc, self.cmd, unsafe { &*depth });
                            }
                            self.resume(gpu, enc);
                            enc.segment(gpu, self.cmd, &self.label, "engine lit solid");
                            unsafe { d.cmd_bind_pipeline(gpu_cmd(self), vk::PipelineBindPoint::GRAPHICS, shade) };
                            self.dirty = true;
                            self.push_descriptors(gpu, enc);
                            terrain.draw(gpu, self.cmd, SOLID, max);
                            self.prepassed = true;
                        }
                        (CUTOUT, (_, _, Some(shade))) if self.prepassed => {
                            unsafe { d.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, shade) };
                            self.dirty = true;
                            self.push_descriptors(gpu, enc);
                            terrain.draw(gpu, cmd, CUTOUT, max);
                        }
                        _ => {
                            // Water and glass: the picture and its depth so far, for their reflections.
                            if layer == TRANSLUCENT && engine.wants_scene() && p.uniforms.iter().any(|u| u.name == "CursedScene") {
                                if let (Some(color), Some(depth)) = (self.color0, self.depth) {
                                    self.suspend(gpu, enc);
                                    engine.capture(gpu, enc, self.cmd, unsafe { &*color }, unsafe { &*depth });
                                    self.resume(gpu, enc);
                                }
                            }
                            self.push_descriptors(gpu, enc);
                            terrain.draw_part(gpu, cmd, layer, max, part);
                        }
                    }
                    if layer == SOLID {
                        self.solid = Some((self.pipeline, self.uniforms.clone()));
                    } else if layer == CUTOUT && terrain.wants_test() {
                        self.terrain_second_part(gpu, enc, terrain);
                    }
                    if layer == CUTOUT {
                        // the opaque world is in: the sky behind it, then the far terrain over the sky
                        self.sky_behind(gpu, enc, terrain);
                        self.lay_impostor(gpu, enc, terrain);
                    }
                }
                op => panic!("Unknown pass call {op} in pass {}", self.label),
            }
        }
    }

    /// This frame's opaque terrain layer, as drawn here, for the far terrain's faces (vk/impostor.rs): the game's own
    /// multidraw terrain pipelines in a pass with one colour target and depth, the engine's shaders off.
    fn capture_terrain(&self, terrain: &mut Terrain, layer: usize) {
        let engine = unsafe { &*self.engine };
        terrain.impostor.vanilla = engine.vanilla() && !self.prepassed;
        if !self.has_depth || self.colors.len() != 1 {
            return;
        }
        let p = self.pipeline();
        if !p.name.contains("terrain_multidraw") {
            return;
        }
        let find = |n: &str| p.uniforms.iter().position(|u| u.name == n);
        static SAID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let say = |why: &str| {
            if SAID.fetch_add(1, std::sync::atomic::Ordering::Relaxed) < 4 {
                eprintln!("[impostor] no capture of {}: {why} (uniforms {:?})", p.name, p.uniforms.iter().map(|u| u.name.as_str()).collect::<Vec<_>>());
            }
        };
        let (Some(proj), Some(tu)) = (find("Projection"), find("TerrainUniform")) else {
            say("no Projection/TerrainUniform");
            return;
        };
        let Some(Value::Buffer { buffer, offset, length }) = self.uniforms.get(tu).copied() else {
            say("no TerrainUniform buffer");
            return;
        };
        if length < 64 {
            say("a short TerrainUniform");
            return;
        }
        let Some(c0) = self.color0 else { return };
        // the engine's uniforms (named Cursed...) as the draws push them
        let mut uniforms = self.uniforms.clone();
        for (i, slot) in p.uniforms.iter().enumerate() {
            if matches!(uniforms[i], Value::None) && slot.name.starts_with("Cursed") {
                uniforms[i] = match engine.binding(&slot.name) {
                    Some(crate::engine::gpu::EngineValue::Image(view, sampler)) => Value::Image { view, sampler },
                    Some(crate::engine::gpu::EngineValue::Buffer(buffer, offset, length)) => Value::Buffer { buffer, offset, length },
                    None => Value::None,
                };
            }
        }
        terrain.impostor.captured[layer] = Some(super::impostor::Capture {
            pipeline: self.pipeline,
            uniforms,
            projection: proj,
            terrain: tu,
            terrain_source: (unsafe { (*buffer).raw }, offset, length),
            color_format: unsafe { (*c0).texture().format },
        });
    }

    /// After the opaque terrain: the far terrain's faces over what is drawn (vk/impostor.rs), then the game's
    /// pipeline, uniforms and scissor again.
    fn lay_impostor(&mut self, gpu: &Gpu, enc: &mut Encoder, terrain: &mut Terrain) {
        if !self.has_depth || self.colors.len() != 1 {
            return;
        }
        let Some(c0) = self.color0 else { return };
        let format = unsafe { (*c0).texture().format };
        let view_proj = terrain.view_proj;
        enc.segment(gpu, self.cmd, &self.label, "far terrain composite");
        if terrain.impostor.composite(gpu, enc, self.cmd, format, self.size, &view_proj) {
            if !self.pipeline.is_null() {
                self.bind_pipeline(gpu, self.pipeline);
            }
            let viewport = vk::Viewport { x: 0.0, y: 0.0, width: self.size.0 as f32, height: self.size.1 as f32, min_depth: 0.0, max_depth: 1.0 };
            unsafe {
                gpu.device.cmd_set_viewport(self.cmd, 0, &[viewport]);
                gpu.device.cmd_set_scissor(self.cmd, 0, &[self.scissor]);
            }
        }
    }

    fn push_descriptors(&mut self, gpu: &Gpu, enc: &mut Encoder) {
        if !self.dirty {
            return;
        }
        // A vertex shader reading a texture a pass drew into since the last full barrier (fragment_barrier does not
        // hold the vertices back): the pass is split around a full one. The lightmap's pass is followed by other work
        // as it is, so in practice this is a guard.
        // a texture with a clear still waiting (Pass::begin): the clear goes first, outside the rendering
        if !enc.pending_clears.is_empty() && self.samples_waiting_clear(enc) {
            self.suspend(gpu, enc);
            enc.flush_clears(gpu);
            self.resume(gpu, enc);
        }
        if self.pipeline().vertex_samplers != 0 && self.reads_unsettled(enc) {
            static SPLITS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let n = SPLITS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if n.is_power_of_two() {
                eprintln!("[barriers] pass {} split for a vertex shader's read ({} so far)", self.label, n + 1);
            }
            self.suspend(gpu, enc);
            self.resume(gpu, enc);
        }
        let p = self.pipeline();
        // On the stack (a push per draw that changed something: the heap's allocations were a tenth of the frame
        // thread's time, 12.10); a push set holds 32 descriptors at most.
        const MAX: usize = 32;
        assert!(p.uniforms.len() <= MAX, "Pipeline {} has more than {MAX} uniforms", p.name);
        let mut buffers = [vk::DescriptorBufferInfo::default(); MAX];
        let mut images = [vk::DescriptorImageInfo::default(); MAX];
        let mut texels = [vk::BufferView::null(); MAX];
        // Which list and where in it, per binding.
        let mut refs = [(0u8, 0usize); MAX];
        let (mut nb, mut ni, mut nt) = (0, 0, 0);
        for (i, slot) in p.uniforms.iter().enumerate() {
            let mut value = self.uniforms[i];
            if matches!(value, Value::None) && slot.name.starts_with("Cursed") {
                let engine = unsafe { &*self.engine };
                value = match engine.binding(&slot.name) {
                    Some(crate::engine::gpu::EngineValue::Image(view, sampler)) => Value::Image { view, sampler },
                    Some(crate::engine::gpu::EngineValue::Buffer(buffer, offset, length)) => Value::Buffer { buffer, offset, length },
                    None => Value::None,
                };
            }
            match (slot.kind, value) {
                (Uniform::Buffer, Value::Buffer { buffer, offset, length }) => {
                    buffers[nb] = vk::DescriptorBufferInfo { buffer: unsafe { (*buffer).raw }, offset, range: length };
                    refs[i] = (0, nb);
                    nb += 1;
                }
                (Uniform::Sampler, Value::Image { view, sampler }) => {
                    images[ni] = vk::DescriptorImageInfo {
                        sampler: unsafe { (*sampler).raw },
                        image_view: unsafe { (*view).raw },
                        image_layout: vk::ImageLayout::GENERAL,
                    };
                    refs[i] = (1, ni);
                    ni += 1;
                }
                (Uniform::Texel(format), Value::Buffer { buffer, offset, length }) => {
                    let info = vk::BufferViewCreateInfo::default().buffer(unsafe { (*buffer).raw }).format(format).offset(offset).range(length);
                    let v = unsafe { gpu.device.create_buffer_view(&info, None) }
                        .unwrap_or_else(|e| panic!("Couldn't create buffer view for texel buffer: {e:?}"));
                    enc.defer(Doomed::BufferView(v));
                    texels[nt] = v;
                    refs[i] = (2, nt);
                    nt += 1;
                }
                (kind, _) => panic!("Missing uniform {} (should be {kind:?}) in pipeline {}", slot.name, p.name),
            }
        }
        let n = p.uniforms.len();
        let mut writes = [vk::WriteDescriptorSet::default(); MAX];
        for (i, &(list, at)) in refs[..n].iter().enumerate() {
            let w = vk::WriteDescriptorSet::default().dst_binding(i as u32).dst_array_element(0);
            writes[i] = match list {
                0 => w.descriptor_type(vk::DescriptorType::UNIFORM_BUFFER).buffer_info(std::slice::from_ref(&buffers[at])),
                1 => w.descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(std::slice::from_ref(&images[at])),
                _ => w.descriptor_type(vk::DescriptorType::UNIFORM_TEXEL_BUFFER).texel_buffer_view(std::slice::from_ref(&texels[at])),
            };
        }
        unsafe { gpu.push_d.cmd_push_descriptor_set(self.cmd, vk::PipelineBindPoint::GRAPHICS, p.layout, 0, &writes[..n]) };
        self.dirty = false;
    }
}

/// The bytes of a call of a pass that can be held for later (the sky's: frame.rs), past its code; None for one that
/// cannot (a timestamp, the terrain, a pipeline without a variant behind the world).
pub fn held_call_len(op: i32, r: &Reader) -> Option<usize> {
    let mut p = *r;
    let start = p.at();
    match op {
        1 => {
            let pipeline = p.i64() as usize as *const Pipeline;
            if pipeline.is_null() || unsafe { (*pipeline).behind }.contains(&vk::Pipeline::null()) {
                return None;
            }
        }
        2 => {
            p.i32();
            match p.i32() {
                1 => {
                    p.bytes(24);
                }
                2 => {
                    p.bytes(16);
                }
                _ => {}
            }
        }
        3 | 14 => {
            let n = p.i32().max(0) as usize;
            p.bytes(n.div_ceil(4) * 4);
        }
        4 => {
            p.bytes(16);
        }
        5 | 15 => {}
        6 | 8 | 12 | 13 => {
            p.bytes(20);
        }
        7 => {
            p.bytes(12);
        }
        9 => {
            p.bytes(16);
        }
        10 | 11 => {
            p.bytes(8);
            let draws = p.u32() as usize;
            p.bytes(draws * if op == 10 { 12 } else { 8 });
        }
        _ => return None,
    }
    Some(p.at() - start)
}

impl Pass {
    /// Whether the current pipeline samples a texture whose clear is still waiting (Encoder::pending_clears).
    fn samples_waiting_clear(&self, enc: &Encoder) -> bool {
        let p = self.pipeline();
        let engine = unsafe { &*self.engine };
        p.uniforms.iter().enumerate().any(|(i, slot)| {
            let value = match self.uniforms.get(i).copied() {
                Some(Value::None) | None if slot.name.starts_with("Cursed") => match engine.binding(&slot.name) {
                    Some(crate::engine::gpu::EngineValue::Image(view, _)) => Value::Image { view, sampler: std::ptr::null() },
                    _ => Value::None,
                },
                Some(v) => v,
                None => Value::None,
            };
            matches!(value, Value::Image { view, .. } if enc.clear_waits(unsafe { (*view).texture }))
        })
    }

    /// Whether the current pipeline's vertex shaders sample a texture of `Encoder::unsettled` (not one this pass draws
    /// into: that would be a feedback loop, not this); what they sample is remembered (`Encoder::vertex_read`).
    fn reads_unsettled(&self, enc: &mut Encoder) -> bool {
        let p = self.pipeline();
        let mut unsettled = false;
        for i in 0..p.uniforms.len().min(64) {
            if p.vertex_samplers & (1 << i) == 0 {
                continue;
            }
            let Value::Image { view, .. } = self.uniforms[i] else { continue };
            let t = unsafe { (*view).texture };
            if !enc.vertex_read.contains(&t) {
                if enc.vertex_read.len() >= 64 {
                    enc.vertex_read.clear();
                }
                enc.vertex_read.push(t);
            }
            unsettled |= enc.unsettled.contains(&t) && !self.drawn.contains(&t);
        }
        unsettled
    }
}

/// Dev (CURSED_DEV_SKIP=name,name): the draws of pipelines with one of these in their name are left out, to measure
/// what they cost.
fn dev_skip(name: &str) -> bool {
    static LIST: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    let list = LIST.get_or_init(|| std::env::var("CURSED_DEV_SKIP").map(|v| v.split(',').filter(|s| !s.is_empty()).map(String::from).collect()).unwrap_or_default());
    list.iter().any(|s| name.contains(s.as_str()))
}

/// Pushes `values` as the uniforms of `p` (buffers and images: what terrain uses). False when one is of another kind.
pub(super) fn push_values(gpu: &Gpu, cmd: vk::CommandBuffer, p: &Pipeline, values: &[Value]) -> bool {
    let mut buffers = Vec::with_capacity(values.len());
    let mut images = Vec::with_capacity(values.len());
    let mut refs = Vec::with_capacity(values.len());
    for (i, slot) in p.uniforms.iter().enumerate() {
        match (slot.kind, values.get(i).copied().unwrap_or(Value::None)) {
            (Uniform::Buffer, Value::Buffer { buffer, offset, length }) => {
                buffers.push(vk::DescriptorBufferInfo { buffer: unsafe { (*buffer).raw }, offset, range: length });
                refs.push((0, buffers.len() - 1));
            }
            (Uniform::Sampler, Value::Image { view, sampler }) => {
                images.push(vk::DescriptorImageInfo { sampler: unsafe { (*sampler).raw }, image_view: unsafe { (*view).raw }, image_layout: vk::ImageLayout::GENERAL });
                refs.push((1, images.len() - 1));
            }
            _ => return false,
        }
    }
    let writes: Vec<vk::WriteDescriptorSet> = refs
        .iter()
        .enumerate()
        .map(|(i, &(list, at))| {
            let w = vk::WriteDescriptorSet::default().dst_binding(i as u32).dst_array_element(0);
            if list == 0 {
                w.descriptor_type(vk::DescriptorType::UNIFORM_BUFFER).buffer_info(std::slice::from_ref(&buffers[at]))
            } else {
                w.descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(std::slice::from_ref(&images[at]))
            }
        })
        .collect();
    unsafe { gpu.push_d.cmd_push_descriptor_set(cmd, vk::PipelineBindPoint::GRAPHICS, p.layout, 0, &writes) };
    true
}

/// Improved Transparency without its depth bounds (cursed.recode.render.CheapOit): their draws and their cull are left
/// out (not the copy of the depth for the clouds in the same pass). CURSED_DEV_VANILLA_OIT: the game's own.
fn cheap_oit_skip(name: &str) -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("CURSED_DEV_VANILLA_OIT").is_none())
        && (name.contains("pipeline/oit_depth_bounds_") || name.ends_with("pipeline/blit_depth_bounds"))
}
