//! The frame thread: it reads the stream of commands the game's thread writes (CursedEncoder, CursedPass) and records
//! them into Vulkan, submits each frame and presents it, while the game already describes the next frame.
//!
//! The stream is little-endian; strings and byte runs are padded to 4 (pipeline.rs Reader). Inside a render pass the
//! pass calls of pass.rs (1–17) follow; outside:
//! 100 pass begin        i32 length, the pass description (pass.rs Pass::begin)
//! 101 pass end
//! 110 clear colour      i64 texture, f32 r g b a
//! 111 clear depth       i64 texture, f64 depth
//! 112 clear both        i64 colour, f32 r g b a, i64 depth texture, f64 depth
//! 113 clear region      i64 colour, f32 r g b a, i64 depth texture, f64 depth, i32 x y width height mip
//! 114 copy buffer       i64 source, i64 offset, i64 destination, i64 offset, i64 length
//! 115 buffer → texture  i64 source, i64 offset, i32 x y row-length image-height texel-bytes, i64 texture, i32 x y width height mip layer
//! 116 texture → buffer  i64 texture, i64 buffer, i64 offset, i32 mip x y width height
//! 117 texture → texture i64 source, i64 destination, i32 mip dx dy sx sy width height
//! 118 timestamp         i64 queries, i32 index
//! 119 init texture      i64 texture (its first layout)
//! 120 init copy         i64 source, i64 offset, i64 buffer, i64 length (a new buffer's first content)
//! 121 close             i32 kind (0 buffer, 1 texture, 2 view, 3 sampler, 4 queries), i64 object
//! 122 terrain           i32 count, i32 ordered, f64 × 3 camera, f32 × 16 ProjMat · ModelViewMat (column-major), then
//!                       count × (i32 mesh, x, y, z, f32 visibility)
//! 123 acquire           i64 surface
//! 124 blit              i64 surface, i64 view
//! 125 submit            i32 count, then count × (u64 source, u64 destination, u64 size): the game's mapped copies
//! 126 present           i64 surface
//! 127 models            i64 jobs buffer, i64 offset, i32 count, i64 target buffer (vk/models.rs)
//! 128 engine            f64 × 3 camera, i32 min y, i32 height, f32 × 4 sun (direction, strength), f32 × 4 its
//!                       colour (rgb, light left in shade), f32 × 4 moon, f32 × 4 sky (rgb, rain), f32 time, i32 flags,
//!                       f32 the world's size against the window, f32 × 4 the player in water (feet round the
//!                       camera, push) (engine/gpu.rs)
//! 129 upscale           i64 colour texture, i64 depth texture (the level's, smaller), i64 out texture, f32 × 2 the
//!                       frame's move in clip space (engine/upscale.rs)
//! 130 copy regions      i64 source buffer, i64 target buffer, i32 count, count × (u64 from, u64 to, u64 size)
//! 131 launch            i64 out texture (RGBA8), f32 t fx life dt, i32 flags (1 launched, 2 smoke afresh, 4 no rays,
//!                       8 no bloom), f32 gather, cursor x y on, look x y, boost, sheet lightning, fade, ui px, the
//!                       title's left, the build line's top, 2 × (x0 y x1 progress strength) burning lines, f32 their
//!                       sparks, i32 lines, lines × f32 × 8, f32 × 12 the music's buttons, f32 × 4 their light, f32 × 8 the record (x y size strength, turn plays),
//!                       i32 glyphs, glyphs × f32 × 16 (launch/mod.rs)
//! 132 gui               i64 out texture (RGBA8), i64 the game's picture (0 none), i32 the frame gui/mod.rs made (0: the
//!                       menu closed, its pictures go) (gui/render.rs)

use super::encoder::Encoder;
use super::pass::Pass;
use super::pipeline::Reader;
use super::resources::{Buffer, Doomed, Queries, Sampler, Texture, View};
use super::surface::Surface;
use super::terrain::Terrain;
use super::Shared;
use ash::vk;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::Arc;

pub const PASS_BEGIN: i32 = 100;
pub const PASS_END: i32 = 101;
pub const CLEAR_COLOR: i32 = 110;
pub const CLEAR_DEPTH: i32 = 111;
pub const CLEAR_BOTH: i32 = 112;
pub const CLEAR_REGION: i32 = 113;
pub const COPY_BUFFER: i32 = 114;
pub const BUFFER_TO_TEXTURE: i32 = 115;
pub const TEXTURE_TO_BUFFER: i32 = 116;
pub const TEXTURE_TO_TEXTURE: i32 = 117;
pub const TIMESTAMP: i32 = 118;
pub const INIT_TEXTURE: i32 = 119;
pub const INIT_COPY: i32 = 120;
pub const CLOSE: i32 = 121;
pub const TERRAIN: i32 = 122;
pub const ACQUIRE: i32 = 123;
pub const BLIT: i32 = 124;
pub const SUBMIT: i32 = 125;
pub const PRESENT: i32 = 126;
pub const MODELS: i32 = 127;
pub const ENGINE: i32 = 128;
pub const UPSCALE: i32 = 129;
pub const COPY_REGIONS: i32 = 130;
pub const LAUNCH: i32 = 131;
pub const GUI: i32 = 132;
/// The chams' ink after the level (engine/ink.rs).
pub const INK: i32 = 133;
/// Target ESP after the chams (engine/tesseract.rs).
pub const TARGET: i32 = 134;
/// Hitboxes after Target ESP (engine/hitboxes.rs).
pub const HITBOXES: i32 = 135;
/// The world's effects after the hitboxes (engine/fx.rs).
pub const FX: i32 = 136;
/// The cosmetics' models in the hands (cosmetics/): the picture, then the frame's facts (cosmetics/view.rs Facts).
pub const VIEWMODEL: i32 = 137;
/// CS's item in the player's hand seen from behind (cosmetics/view.rs third): the level's picture and its depth, the
/// hold (16 numbers, from the camera), the light at the player (sky, block).
pub const THIRD: i32 = 138;

pub enum Message {
    /// A piece of the stream.
    Ops(Vec<u8>),
    /// Something to do on this thread after all before it (configuring a surface, a timestamp, a sync).
    Call(Box<dyn FnOnce(&mut Worker) + Send>),
    Exit(mpsc::SyncSender<()>),
}

pub struct Worker {
    pub shared: Arc<Shared>,
    pub enc: Encoder,
    pass: Option<Pass>,
    pub terrain: Terrain,
    /// Entity vertices on the GPU, once Java has compiled its shader (vk/models.rs).
    pub models: Option<super::models::GpuModels>,
    pub engine: Box<crate::engine::gpu::Engine>,
    /// The main menu's picture and the card before it (launch/mod.rs): its pipelines are made beside the game's
    /// loading (the driver compiles them the first time), so the menu's first frame does not wait for them.
    pub launch: Option<crate::launch::Launch>,
    launch_maker: Option<std::thread::JoinHandle<crate::launch::Made>>,
    /// The client's menu (gui/render.rs), its pipelines made the same way.
    gui: Option<crate::gui::render::Render>,
    gui_maker: Option<std::thread::JoinHandle<crate::gui::render::Made>>,
    /// The sky's pass, held to be drawn behind the opaque world of the pass after it (pass.rs, sky_behind).
    sky: Option<HeldSky>,
    /// A pass the game ended, kept open while the next one may draw into the same textures (Pass::same_targets).
    parked: Option<Pass>,
}

/// Dev: CURSED_DEV_NOPASSMERGE ends every pass as the game does.
fn merge_passes() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("CURSED_DEV_NOPASSMERGE").is_none())
}

/// The sky's pass as Java sent it: its description, its calls, the textures it draws into, whether it ended.
struct HeldSky {
    desc: Vec<u8>,
    ops: Vec<u8>,
    targets: Vec<*const Texture>,
    area: [i32; 4],
    ended: bool,
}

/// A pass's colour and depth textures and render area, from its description (Pass::begin's layout).
fn pass_targets(desc: &[u8]) -> (String, Vec<*const Texture>, bool, bool, [i32; 4]) {
    let mut r = Reader::new(desc);
    let label = r.str();
    let mut targets = Vec::new();
    let mut cleared = false;
    let n = r.i32();
    let mut colors = 0;
    for _ in 0..n {
        let present = r.i32() != 0;
        let v = r.i64();
        cleared |= r.i32() != 0;
        for _ in 0..4 {
            r.f32();
        }
        if present {
            colors += 1;
            targets.push(obj::<View>(v).texture);
        }
    }
    let depth = r.i32() != 0;
    if depth {
        targets.push(obj::<View>(r.i64()).texture);
        cleared |= r.i32() != 0;
        r.f64();
    }
    let area = [r.i32(), r.i32(), r.i32(), r.i32()];
    (label, targets, colors == 1 && depth, cleared, area)
}

/// Dev: CURSED_DEV_EARLYSKY draws the sky first, as the game does.
fn late_sky() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("CURSED_DEV_EARLYSKY").is_none())
}

fn obj<'a, T>(p: i64) -> &'a T {
    assert!(p != 0, "null object handle");
    unsafe { &*(p as usize as *const T) }
}

fn obj_mut<'a, T>(p: i64) -> &'a mut T {
    assert!(p != 0, "null object handle");
    unsafe { &mut *(p as usize as *mut T) }
}

impl Worker {
    pub fn run(shared: Arc<Shared>, messages: mpsc::Receiver<Message>) {
        // Every frame passes through here, as through the game's render thread (which the game runs at the highest
        // priority): at the normal one, the busy threads of a game loading chunks (the sections' meshes, the world's
        // generation, the collector) took it off the core for a scheduler's turn at a time — 15 to 30 ms with nothing
        // drawn. CURSED_DEV_FRAMENORMAL leaves it normal, for comparing.
        if std::env::var_os("CURSED_DEV_FRAMENORMAL").is_none() {
            unsafe {
                use windows_sys::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_HIGHEST};
                SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST);
            }
        }
        super::sampler::FRAME_THREAD.store(super::thread_id(), std::sync::atomic::Ordering::Relaxed);
        super::sampler::start();
        let enc = Encoder::new(&shared.gpu, shared.timeline);
        let engine = Box::new(crate::engine::gpu::Engine::new(&shared.gpu));
        let maker_shared = shared.clone();
        let launch_maker = std::thread::Builder::new()
            .name("Cursed launch pipelines".into())
            .spawn(move || crate::launch::Made(crate::launch::Launch::new(&maker_shared.gpu)))
            .ok();
        let gui_shared = shared.clone();
        let gui_maker = std::thread::Builder::new()
            .name("Cursed menu pipelines".into())
            .spawn(move || crate::gui::render::Made(crate::gui::render::Render::new(&gui_shared.gpu)))
            .ok();
        let mut w = Worker { shared: shared.clone(), enc, pass: None, terrain: Terrain::new(), models: None, engine, launch: None, launch_maker, gui: None, gui_maker, sky: None, parked: None };
        let mut dead = false;
        while let Ok(m) = messages.recv() {
            if let Message::Exit(reply) = m {
                if !dead {
                    let _ = catch_unwind(AssertUnwindSafe(|| w.shutdown()));
                }
                let _ = reply.send(());
                return;
            }
            if dead {
                continue;
            }
            let result = catch_unwind(AssertUnwindSafe(|| match m {
                Message::Ops(bytes) => super::stats::time(super::stats::PASS, || w.ops(&bytes)),
                Message::Call(f) => {
                    w.end_parked();
                    w.sky_now("a call");
                    f(&mut w)
                }
                Message::Exit(_) => unreachable!(),
            }));
            if result.is_err() {
                let text = crate::take_panic_message().unwrap_or_else(|| "panic".into());
                *shared.failure.lock().unwrap() = Some(text);
                // The waits on frames give up when they see the failure.
                shared.progress.submitted(u64::MAX);
                dead = true;
            }
        }
    }

    fn shutdown(&mut self) {
        let gpu = &self.shared.gpu;
        if let Some(mut m) = self.models.take() {
            m.destroy(gpu);
        }
        self.engine.destroy(gpu);
        if let Some(l) = self.launch.take() {
            l.destroy(gpu);
        }
        if let Some(Ok(m)) = self.launch_maker.take().map(|m| m.join()) {
            m.0.destroy(gpu);
        }
        if let Some(g) = self.gui.take() {
            g.destroy(gpu);
        }
        if let Some(Ok(m)) = self.gui_maker.take().map(|m| m.join()) {
            m.0.destroy(gpu);
        }
        self.enc.destroy(gpu);
        self.terrain.destroy(gpu);
    }

    /// A texture goes when it is closed and its last view is closed.
    fn release_texture(&mut self, t: *const Texture) {
        let tex = unsafe { &*t };
        let before = tex.views.fetch_sub(1, Ordering::AcqRel);
        assert!(before > 0, "Too many views removed from texture");
        if before == 1 && tex.closed.load(Ordering::Acquire) {
            self.enc.defer(Doomed::Texture(unsafe { Box::from_raw(t as *mut Texture) }));
        }
    }

    fn ops(&mut self, bytes: &[u8]) {
        let shared = self.shared.clone();
        let gpu = &shared.gpu;
        let mut r = Reader::new(bytes);
        let timed = super::stats::on();
        // CURSED_DEV_STALLS: an op that held the frame thread up goes to the log
        static STALLS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        let stalls = *STALLS.get_or_init(|| std::env::var_os("CURSED_DEV_STALLS").is_some());
        while !r.done() {
            let op = r.i32();
            let slow = stalls.then(std::time::Instant::now);
            self.ops_one(op, &mut r, gpu, &shared, timed);
            if let Some(t) = slow {
                let ms = t.elapsed().as_secs_f64() * 1000.0;
                if ms > 8.0 {
                    eprintln!("[stall] frame op {op} ({}) took {ms:.1} ms", if self.pass.is_some() { "in a pass" } else { "outside" });
                }
            }
        }
    }

    /// An op while the sky's pass is held: true when it went into the held sky.
    fn sky_op(&mut self, op: i32, r: &mut Reader) -> bool {
        let Some(sky) = self.sky.as_mut() else { return false };
        if !sky.ended {
            match op {
                PASS_END => {
                    sky.ended = true;
                    return true;
                }
                INIT_COPY | INIT_TEXTURE | CLOSE => return false,
                _ => {}
            }
            if let Some(n) = super::pass::held_call_len(op, r) {
                sky.ops.extend_from_slice(&op.to_le_bytes());
                sky.ops.extend_from_slice(r.bytes(n));
                return true;
            }
            // a call that cannot wait: the pass goes as it came and takes this call
            self.sky_now("a call that cannot wait");
            return false;
        }
        match op {
            PASS_BEGIN | INIT_COPY | INIT_TEXTURE | CLOSE | COPY_BUFFER | COPY_REGIONS | MODELS | TERRAIN | TIMESTAMP => {}
            CLEAR_COLOR | CLEAR_DEPTH | CLEAR_BOTH => {
                // a clear of the sky's textures comes after it, as asked
                let mut p = *r;
                let mut cleared = vec![p.i64()];
                if op == CLEAR_BOTH {
                    for _ in 0..4 {
                        p.f32();
                    }
                    cleared.push(p.i64());
                }
                if cleared.iter().any(|&t| sky.targets.iter().any(|&s| s as usize as i64 == t)) {
                    self.sky_now("a clear of its textures");
                }
            }
            _ => self.sky_now("another command"),
        }
        false
    }

    /// The held sky as the game sent it, now: a pass of its own (left open when its end has not come yet).
    pub fn sky_now(&mut self, why: &str) {
        let Some(sky) = self.sky.take() else { return };
        static SAID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        if SAID.fetch_add(1, Ordering::Relaxed) < 8 {
            eprintln!("[sky] drawn first, as the game does: {why}");
        }
        let shared = self.shared.clone();
        let gpu = &shared.gpu;
        let mut pass = Pass::begin(gpu, &mut self.enc, &sky.desc, &mut *self.engine);
        pass.replay(gpu, &mut self.enc, &mut self.terrain, &sky.ops);
        if sky.ended {
            pass.end(gpu, &mut self.enc, &mut self.terrain);
        } else {
            self.pass = Some(pass);
        }
    }

    /// The parked pass ends (anything but a pass into the same textures came).
    pub fn end_parked(&mut self) {
        if let Some(p) = self.parked.take() {
            let shared = self.shared.clone();
            p.end(&shared.gpu, &mut self.enc, &mut self.terrain);
        }
    }

    fn ops_one(&mut self, op: i32, r: &mut Reader, gpu: &super::device::Gpu, shared: &Arc<Shared>, timed: bool) {
        if self.parked.is_some() {
            if op == PASS_BEGIN {
                let mut peek = *r;
                let n = peek.i32().max(0) as usize;
                let desc = peek.bytes(n.div_ceil(4) * 4);
                if self.parked.as_ref().unwrap().same_targets(&desc[..n], &self.enc) {
                    *r = peek;
                    let mut pass = self.parked.take().unwrap();
                    pass.go_on_as(gpu, &mut self.enc, &desc[..n]);
                    self.pass = Some(pass);
                    return;
                }
            }
            self.end_parked();
        }
        if self.sky.is_some() && self.sky_op(op, r) {
            return;
        }
        {
            let started = timed.then(std::time::Instant::now);
            if let Some(pass) = self.pass.as_mut() {
                // a new buffer's first content, a new texture's first layout, a close: the frame's, not the pass's
                // (they go to the frame's first command buffer or wait for the GPU); the game grows its dynamic
                // transforms' buffer mid-pass, making the new one and closing the old
                if op != PASS_END && op != INIT_COPY && op != INIT_TEXTURE && op != CLOSE {
                    pass.call(op, r, gpu, &mut self.enc, &mut self.terrain);
                    if let Some(t) = started {
                        super::stats::op(op, t.elapsed().as_nanos() as u64);
                    }
                    return;
                }
            }
            // dev (BENCH): the GPU's time of the commands between passes (clears, copies), on the timeline too
            let gpu_timed = super::stats::passes() && matches!(op, 110..=117 | 122 | 127..=132) && self.pass.is_none();
            if gpu_timed {
                let c = self.enc.cmd(gpu);
                self.enc.pass_started(gpu, c, &format!("op {op}"));
            }
            // A clear waits for the pass that renders into its texture; anything else touching the game's textures (not the buffer copies, not the engine's own images) comes after
            // it as asked (Encoder::clear_color).
            // (never inside a pass: a texture closed meanwhile is destroyed only after the submit, which flushes them)
            if self.pass.is_none() && !matches!(op, PASS_BEGIN | CLEAR_COLOR | CLEAR_DEPTH | CLEAR_BOTH | COPY_BUFFER | COPY_REGIONS | TIMESTAMP | INIT_TEXTURE | INIT_COPY | TERRAIN | ACQUIRE | MODELS | ENGINE | PRESENT) {
                self.enc.flush_clears(gpu);
            }
            self.op(op, r, gpu, shared);
            if gpu_timed && !self.enc.in_pass {
                let c = self.enc.cmd(gpu);
                self.enc.pass_ended(gpu, c);
            }
            if let Some(t) = started {
                super::stats::op(op, t.elapsed().as_nanos() as u64);
            }
        }
    }

    fn op(&mut self, op: i32, r: &mut Reader, gpu: &super::device::Gpu, shared: &Arc<Shared>) {
        {
            match op {
                PASS_BEGIN => {
                    let n = r.i32().max(0) as usize;
                    let desc = r.bytes(n.div_ceil(4) * 4);
                    assert!(self.pass.is_none(), "A render pass is already open");
                    let (label, targets, one_color_and_depth, cleared, area) = pass_targets(&desc[..n]);
                    // the sky's pass is held for the pass after it, which draws the opaque world into the same textures
                    if let Some(sky) = self.sky.take() {
                        if sky.targets == targets && sky.area == area {
                            let mut pass = Pass::begin(gpu, &mut self.enc, &desc[..n], &mut *self.engine);
                            if matches!(pass.cleared_depth, Some(d) if d == 0.0 || d == 1.0) {
                                pass.late_sky = Some(sky.ops);
                            } else {
                                pass.replay(gpu, &mut self.enc, &mut self.terrain, &sky.ops);
                            }
                            self.pass = Some(pass);
                            return;
                        }
                        self.sky = Some(sky);
                        self.sky_now("another pass after it");
                    }
                    if late_sky() && label == "Sky" && one_color_and_depth && !cleared {
                        self.sky = Some(HeldSky { desc: desc[..n].to_vec(), ops: Vec::new(), targets, area, ended: false });
                        return;
                    }
                    self.pass = Some(Pass::begin(gpu, &mut self.enc, &desc[..n], &mut *self.engine));
                }
                PASS_END => {
                    let pass = self.pass.take().expect("Cannot submit a renderpass if one hasn't been started!");
                    // kept open for the next pass if it draws into the same textures (Pass::same_targets)
                    if merge_passes() && self.terrain.impostor.captured[0].is_none() && self.sky.is_none() {
                        self.parked = Some(pass);
                        return;
                    }
                    pass.end(gpu, &mut self.enc, &mut self.terrain);
                    // the far terrain's faces due this frame, with what the pass drew the terrain with (vk/impostor.rs)
                    if self.terrain.impostor.captured[0].is_some() {
                        self.terrain.refresh_impostor(gpu, &mut self.enc);
                    }
                }
                CLEAR_COLOR => {
                    let t = obj::<Texture>(r.i64());
                    let rgba = [r.f32(), r.f32(), r.f32(), r.f32()];
                    self.enc.clear_color(gpu, t, rgba);
                }
                CLEAR_DEPTH => {
                    let t = obj::<Texture>(r.i64());
                    let d = r.f64();
                    self.enc.clear_depth(gpu, t, d as f32);
                }
                CLEAR_BOTH => {
                    let c = obj::<Texture>(r.i64());
                    let rgba = [r.f32(), r.f32(), r.f32(), r.f32()];
                    let dt = obj::<Texture>(r.i64());
                    let d = r.f64();
                    self.enc.clear_color_depth(gpu, c, rgba, dt, d as f32);
                }
                CLEAR_REGION => {
                    let c = obj::<Texture>(r.i64());
                    let rgba = [r.f32(), r.f32(), r.f32(), r.f32()];
                    let dt = obj::<Texture>(r.i64());
                    let d = r.f64();
                    let rect = [r.i32(), r.i32(), r.i32(), r.i32()];
                    let mip = r.u32();
                    self.enc.clear_region(gpu, c, rgba, dt, d as f32, rect, mip);
                }
                COPY_BUFFER => {
                    let (src, so, dst, doff, len) = (obj::<Buffer>(r.i64()), r.i64() as u64, obj::<Buffer>(r.i64()), r.i64() as u64, r.i64() as u64);
                    self.enc.copy_buffer(gpu, src, so, dst, doff, len);
                }
                COPY_REGIONS => {
                    let (src, dst) = (obj::<Buffer>(r.i64()), obj::<Buffer>(r.i64()));
                    let n = r.i32().max(0) as usize;
                    let mut regions = Vec::with_capacity(n);
                    for _ in 0..n {
                        regions.push(vk::BufferCopy { src_offset: r.i64() as u64, dst_offset: r.i64() as u64, size: r.i64() as u64 });
                    }
                    self.enc.copy_regions(gpu, src, dst, &regions);
                }
                BUFFER_TO_TEXTURE => {
                    let src = obj::<Buffer>(r.i64());
                    let so = r.i64() as u64;
                    let rect = [r.u32(), r.u32(), r.u32(), r.u32()];
                    let block = r.u32() as u64;
                    let t = obj::<Texture>(r.i64());
                    let dst = [r.i32(), r.i32()];
                    let size = [r.u32(), r.u32()];
                    let (mip, layer) = (r.u32(), r.u32());
                    self.enc.copy_buffer_to_texture(gpu, src, so, rect, block, t, dst, size, mip, layer);
                }
                TEXTURE_TO_BUFFER => {
                    let t = obj::<Texture>(r.i64());
                    let b = obj::<Buffer>(r.i64());
                    let off = r.i64() as u64;
                    let (mip, x, y, w, h) = (r.u32(), r.i32(), r.i32(), r.u32(), r.u32());
                    self.enc.copy_texture_to_buffer(gpu, t, b, off, mip, x, y, w, h);
                }
                TEXTURE_TO_TEXTURE => {
                    let src = obj::<Texture>(r.i64());
                    let dst = obj::<Texture>(r.i64());
                    let (mip, dx, dy, sx, sy, w, h) = (r.u32(), r.i32(), r.i32(), r.i32(), r.i32(), r.u32(), r.u32());
                    self.enc.copy_texture_to_texture(gpu, src, dst, mip, dx, dy, sx, sy, w, h);
                }
                TIMESTAMP => {
                    let q = obj::<Queries>(r.i64());
                    let index = r.u32();
                    self.enc.timestamp(gpu, q, index);
                }
                INIT_TEXTURE => {
                    let t = obj::<Texture>(r.i64());
                    self.enc.init_texture(gpu, t);
                }
                INIT_COPY => {
                    let src = obj::<Buffer>(r.i64());
                    let so = r.i64() as u64;
                    let dst = obj::<Buffer>(r.i64());
                    let len = r.i64() as u64;
                    self.enc.init_copy(gpu, src, so, dst, len);
                }
                CLOSE => {
                    let kind = r.i32();
                    let p = r.i64();
                    self.close(kind, p);
                }
                TERRAIN => {
                    let count = r.i32().max(0) as usize;
                    let ordered = r.i32() != 0;
                    let number = r.i64() as u64;
                    let camera = [r.f64(), r.f64(), r.f64()];
                    let view_proj: [f32; 16] = std::array::from_fn(|_| r.f32());
                    let list = r.bytes(count * 20);
                    self.engine.view(camera, view_proj);
                    super::stats::time(super::stats::TERRAIN, || self.terrain.build(&shared.terrain_feed, gpu, &mut self.enc, number, list, ordered, camera, view_proj));
                }
                ACQUIRE => obj_mut::<Surface>(r.i64()).acquire(gpu),
                BLIT => {
                    let s = obj_mut::<Surface>(r.i64());
                    let v = obj::<View>(r.i64());
                    s.blit(gpu, &mut self.enc, v);
                }
                SUBMIT => {
                    let n = r.i32().max(0) as usize;
                    let copies: Vec<(vk::Buffer, vk::Buffer, u64)> = (0..n)
                        .map(|_| {
                            let src = vk::Handle::from_raw(r.i64() as u64);
                            let dst = vk::Handle::from_raw(r.i64() as u64);
                            (src, dst, r.i64() as u64)
                        })
                        .collect();
                    assert!(self.pass.is_none(), "Submit while a render pass is open");
                    for d in std::mem::take(&mut *shared.pending.lock().unwrap()) {
                        self.enc.defer(d);
                    }
                    self.enc.submit(gpu, &copies, &shared.progress);
                }
                PRESENT => {
                    let s = obj_mut::<Surface>(r.i64());
                    s.present(&self.shared);
                    // CURSED_DEV_STALLS: a frame shown late goes to the log
                    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
                    static LAST: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
                    if *ON.get_or_init(|| std::env::var_os("CURSED_DEV_STALLS").is_some()) {
                        let now = std::time::Instant::now();
                        let mut last = LAST.lock().unwrap();
                        if let Some(l) = *last {
                            let ms = now.duration_since(l).as_secs_f64() * 1000.0;
                            if ms > 30.0 {
                                eprintln!("[stall] t={:.3} present: {ms:.0} ms since the frame before", super::terrain::clock());
                            }
                        }
                        *last = Some(now);
                    }
                }
                MODELS => {
                    let jobs = obj::<Buffer>(r.i64());
                    let offset = r.i64() as u64;
                    let count = r.i32().max(0) as u32;
                    let target = obj::<Buffer>(r.i64());
                    let models = self.models.as_mut().expect("Models dispatched before their shader was made");
                    models.dispatch(gpu, &mut self.enc, jobs, offset, count, target);
                }
                ENGINE => {
                    let camera = [r.f64(), r.f64(), r.f64()];
                    let (min_y, height) = (r.i32(), r.i32());
                    let mut v4 = || [r.f32(), r.f32(), r.f32(), r.f32()];
                    let (sun, sun_color, moon, sky) = (v4(), v4(), v4(), v4());
                    let time = r.f32();
                    let flags = r.i32();
                    let scale = r.f32();
                    let mover = [r.f32(), r.f32(), r.f32(), r.f32()];
                    let mut casters = [[0.0f32; 6]; 32];
                    let count = (r.i32().max(0) as usize).min(32);
                    for c in casters.iter_mut().take(count) {
                        *c = [r.f32(), r.f32(), r.f32(), r.f32(), r.f32(), r.f32()];
                    }
                    let clouds = [r.f32(), r.f32(), r.f32()];
                    let wet = r.f32();
                    let p = crate::engine::gpu::Params { camera, min_y, height, sun, sun_color, moon, sky, time, flags, scale, mover, casters, caster_count: count, clouds, wet };
                    self.engine.frame(gpu, &mut self.enc, p);
                }
                UPSCALE => {
                    let color = obj::<Texture>(r.i64());
                    let depth = obj::<Texture>(r.i64());
                    let out = obj::<Texture>(r.i64());
                    let jitter = [r.f32(), r.f32()];
                    self.engine.resolve(gpu, &mut self.enc, color, depth, out, jitter);
                }
                INK => {
                    let color = obj::<Texture>(r.i64());
                    let depth = obj::<Texture>(r.i64());
                    let fog = [r.f32(), r.f32(), r.f32(), r.f32()];
                    let count = (r.i32().max(0) as usize).min(crate::engine::ink::MOST_BODIES);
                    let mut bodies = [crate::engine::ink::Body::default(); crate::engine::ink::MOST_BODIES];
                    for b in bodies.iter_mut().take(count) {
                        b.feet = [r.f32(), r.f32(), r.f32()];
                        b.height = r.f32();
                        b.vel = [r.f32(), r.f32(), r.f32()];
                        b.amount = r.f32();
                        b.hands = [[r.f32(), r.f32(), r.f32(), r.f32()], [r.f32(), r.f32(), r.f32(), r.f32()]];
                        b.was_feet = [r.f32(), r.f32(), r.f32()];
                        b.was_hands = [[r.f32(), r.f32(), r.f32()], [r.f32(), r.f32(), r.f32()]];
                        b.hands_on = r.f32();
                        b.wide = r.f32();
                    }
                    // the liquid's boxes (cursed.recode.chams.ChamsLiquid)
                    let n = r.i32().max(0) as usize;
                    let mut boxes = Vec::with_capacity(n.min(crate::engine::liquid::MOST_BOXES));
                    for _ in 0..n {
                        let b: crate::engine::liquid::LiquidBox = std::array::from_fn(|_| r.f32());
                        if boxes.len() < crate::engine::liquid::MOST_BOXES {
                            boxes.push(b);
                        }
                    }
                    self.engine.ink_frame(gpu, &mut self.enc, color, depth, &bodies[..count], &boxes, fog);
                }
                TARGET => {
                    let color = obj::<Texture>(r.i64());
                    let depth = obj::<Texture>(r.i64());
                    let n = r.i32().max(0) as usize;
                    let mut targets = Vec::with_capacity(n.min(crate::engine::tesseract::MOST_TARGETS));
                    for _ in 0..n {
                        let (id, flags) = (r.i32(), r.i32());
                        let (feet, width, height, yaw) = ([r.f32(), r.f32(), r.f32()], r.f32(), r.f32(), r.f32());
                        let count = r.i32().max(0) as usize;
                        let mut boxes = Vec::with_capacity(count.min(crate::engine::tesseract::MOST_BOXES));
                        for _ in 0..count {
                            let b: [f32; 16] = std::array::from_fn(|_| r.f32());
                            if boxes.len() < crate::engine::tesseract::MOST_BOXES {
                                boxes.push(b);
                            }
                        }
                        if targets.len() < crate::engine::tesseract::MOST_TARGETS {
                            targets.push(crate::engine::tesseract::Target { id, flags, feet, width, height, yaw, boxes });
                        }
                    }
                    self.engine.target_frame(gpu, &mut self.enc, color, depth, &targets);
                }
                HITBOXES => {
                    let color = obj::<Texture>(r.i64());
                    let depth = obj::<Texture>(r.i64());
                    let n = r.i32().max(0) as usize;
                    let mut boxes = Vec::with_capacity(n.min(crate::engine::hitboxes::MOST));
                    for _ in 0..n {
                        let (id, kind, flags) = (r.i32(), r.i32(), r.i32());
                        let lo = [r.f32(), r.f32(), r.f32()];
                        let hi = [r.f32(), r.f32(), r.f32()];
                        let eye = r.f32();
                        let look = [r.f32(), r.f32(), r.f32()];
                        let (health, hurt) = (r.f32(), r.f32());
                        if boxes.len() < crate::engine::hitboxes::MOST {
                            boxes.push(crate::engine::hitboxes::Box3 { id, kind, flags, lo, hi, eye, look, health, hurt });
                        }
                    }
                    self.engine.hitboxes_frame(gpu, &mut self.enc, color, depth, &boxes);
                }
                FX => {
                    let color = obj::<Texture>(r.i64());
                    let depth = obj::<Texture>(r.i64());
                    let n = r.i32().max(0) as usize;
                    let mut shapes = Vec::with_capacity(n.min(crate::engine::fx::MOST));
                    for _ in 0..n {
                        let v: [[f32; 4]; 4] = std::array::from_fn(|_| [r.f32(), r.f32(), r.f32(), r.f32()]);
                        let colour = [r.f32(), r.f32(), r.f32(), r.f32()];
                        if shapes.len() < crate::engine::fx::MOST {
                            shapes.push(crate::engine::fx::Shape { v, colour });
                        }
                    }
                    let letters = self.gui.as_ref().and_then(|g| g.letters());
                    self.engine.fx_frame(gpu, &mut self.enc, color, depth, &shapes, letters);
                }
                THIRD => {
                    let color = obj::<Texture>(r.i64());
                    let depth = obj::<Texture>(r.i64());
                    let pose: [f32; 16] = std::array::from_fn(|_| r.f32());
                    let (sky, block) = (r.f32(), r.f32());
                    self.engine.third_frame(gpu, &mut self.enc, color, depth, &pose, sky, block);
                }
                VIEWMODEL => {
                    let color = obj::<Texture>(r.i64());
                    let facts = crate::cosmetics::view::Facts {
                        yaw: r.f32(),
                        pitch: r.f32(),
                        vel: [r.f32(), r.f32(), r.f32()],
                        flags: r.i32() as u32,
                        sky_light: r.f32(),
                        block_light: r.f32(),
                        aspect: color.width as f32 / color.height.max(1) as f32,
                    };
                    self.engine.viewmodel_frame(gpu, &mut self.enc, color, facts);
                }
                LAUNCH => {
                    let out = obj::<Texture>(r.i64());
                    let (t, fx, life, dt) = (r.f32(), r.f32(), r.f32(), r.f32());
                    let flags = r.i32();
                    let gather = r.f32();
                    let cursor = [r.f32(), r.f32(), r.f32()];
                    let look = [r.f32(), r.f32()];
                    let (boost, flash_c, fade, ui, left, foot_top) = (r.f32(), r.f32(), r.f32(), r.f32(), r.f32(), r.f32());
                    let mut hover = [[0f32; 5]; 2];
                    for h in hover.iter_mut() {
                        *h = [r.f32(), r.f32(), r.f32(), r.f32(), r.f32()];
                    }
                    let hover_sparks = r.f32();
                    let count = (r.i32().max(0) as usize).min(crate::launch::LINES);
                    let mut lines = [[0f32; 8]; crate::launch::LINES];
                    for l in lines.iter_mut().take(count) {
                        *l = std::array::from_fn(|_| r.f32());
                    }
                    let i = crate::launch::Inputs {
                        t,
                        fx,
                        life,
                        dt,
                        launched: flags & 1 != 0,
                        gather,
                        smoke_reset: flags & 2 != 0,
                        cursor,
                        look,
                        boost,
                        flash_c,
                        fade,
                        ui,
                        left,
                        foot_top,
                        hover,
                        hover_sparks,
                        lines,
                        icons: [[0.0, 0.0, 0.0, -1.0]; 3],
                        icon_light: [0.0; 4],
                        disc: [0.0; 8],
                        no_rays: flags & 4 != 0,
                        no_bloom: flags & 8 != 0,
                        menus_fire: flags & 16 != 0,
                        knobs: crate::launch::KNOBS,
                        pose: crate::launch::POSE,
                        phase: crate::launch::timeline::Phase::Run,
                        pt: t,
                        load: 1.0,
                        loaded_at: -1.0,
                        world: None,
                        quit: -1.0,
                    };
                    let icons: [[f32; 4]; 3] = std::array::from_fn(|_| [r.f32(), r.f32(), r.f32(), r.f32()]);
                    let icon_light = [r.f32(), r.f32(), r.f32(), r.f32()];
                    let disc: [f32; 8] = std::array::from_fn(|_| r.f32());
                    let glyphs = (r.i32().max(0) as usize).min(4096);
                    let text: Vec<f32> = (0..glyphs * 16).map(|_| r.f32()).collect();
                    // the menus' screens' world (their knobs and the camera's place; none: the main menu's, as the
                    // game's options make it — the ways into and out of a world)
                    let main = crate::launch::valley();
                    let mut pose: [f32; 5] = main[..5].try_into().unwrap();
                    let mut knobs: [f32; 24] = main[5..].try_into().unwrap();
                    if r.i32() != 0 {
                        // as the menus say it (menus/mod.rs out POSE): the camera's place first
                        for p in pose.iter_mut() {
                            *p = r.f32();
                        }
                        for k in knobs.iter_mut() {
                            *k = r.f32();
                        }
                    }
                    // (the window before the game goes on from it on the way out)
                    crate::launch::drew(&pose, &knobs);
                    // v12: which scene, its second, the loading, the world's frame (LaunchFrame.scene)
                    use crate::launch::timeline::Phase;
                    let phase = match r.i32() {
                        1 => Phase::ExitMenu,
                        2 => Phase::Enter,
                        3 => Phase::World,
                        4 => Phase::ExitWorld,
                        _ => Phase::Run,
                    };
                    let (pt, load, loaded_at) = (r.f32(), r.f32(), r.f32());
                    let world_h = r.i64();
                    let world = if world_h != 0 {
                        let wt = obj::<Texture>(world_h);
                        Some((crate::engine::upscale::temp_view(gpu, &mut self.enc, wt), wt.width as f32 / wt.height.max(1) as f32))
                    } else {
                        None
                    };
                    // the card's sounds: from the window before the game while it plays it, from here after
                    if i.launched && phase == Phase::Run && !crate::twin::owns_card() {
                        crate::launch::sound::launch(i.fx, i.t, i.dt);
                    }
                    let i = crate::launch::Inputs { icons, icon_light, disc, knobs, pose, phase, pt, load, loaded_at, world, ..i };
                    let started = std::time::Instant::now();
                    let made = self.launch.is_none();
                    if made {
                        self.launch = self.launch_maker.take().and_then(|m| m.join().ok()).map(|m| m.0);
                    }
                    let launch = self.launch.get_or_insert_with(|| crate::launch::Launch::new(gpu));
                    let after_new = started.elapsed();
                    launch.frame(gpu, &mut self.enc, &i, &text, out);
                    let took = started.elapsed();
                    if made || took.as_millis() >= 8 {
                        if let Ok(mut m) = super::device::MESSAGES.lock() {
                            m.push(format!("I [launch] frame t {:.2}: {:.1} ms{}", i.t, took.as_secs_f64() * 1e3,
                                if made { format!(" (made: {:.1} ms)", after_new.as_secs_f64() * 1e3) } else { String::new() }));
                        }
                    }
                }
                GUI => {
                    let out = r.i64();
                    let back = r.i64();
                    let id = r.i32() as u32;
                    if id == 0 {
                        if let Some(g) = self.gui.as_mut() {
                            g.trim(&mut self.enc);
                        }
                    } else if let Some(b) = crate::gui::take(id) {
                        if self.gui.is_none() {
                            self.gui = self.gui_maker.take().and_then(|m| m.join().ok()).map(|m| m.0);
                        }
                        let g = self.gui.get_or_insert_with(|| crate::gui::render::Render::new(gpu));
                        let back = if back != 0 { Some(obj::<Texture>(back)) } else { None };
                        g.frame(gpu, &mut self.enc, &b, obj::<Texture>(out), back);
                        // the Cosmetics page's look at an item, over the menu
                        if let Some(p) = &b.preview {
                            self.engine.viewmodel_preview(gpu, &mut self.enc, obj::<Texture>(out), p);
                        }
                    }
                }
                op => panic!("Unknown frame command {op}"),
            }
        }
    }

    fn close(&mut self, kind: i32, p: i64) {
        match kind {
            0 => self.enc.defer(Doomed::Buffer(unsafe { Box::from_raw(p as usize as *mut Buffer) })),
            1 => {
                let t = obj::<Texture>(p);
                if !t.closed.swap(true, Ordering::AcqRel) {
                    self.release_texture(t);
                }
            }
            2 => {
                let v = unsafe { Box::from_raw(p as usize as *mut View) };
                let texture = v.texture;
                self.enc.defer(Doomed::View(v));
                self.release_texture(texture);
            }
            3 => self.enc.defer(Doomed::Sampler(unsafe { Box::from_raw(p as usize as *mut Sampler) })),
            4 => self.enc.defer(Doomed::Queries(unsafe { Box::from_raw(p as usize as *mut Queries) })),
            k => panic!("Unknown object kind {k} to close"),
        }
    }
}
