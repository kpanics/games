//! cursed_recode.dll, the native half of Cursed Recode.
//!
//! Java loads it with System.load; `JNI_OnLoad` registers the methods of `cursed.recode.render.Native` (no
//! `Java_…` exports). Every method takes and returns plain numbers: objects are the addresses of their Rust structs,
//! byte data is an address and a length. A panic never crosses into the JVM: it becomes an IllegalStateException
//! with the panic's message and place, and the game's crash report shows it.
//!
//! The frame's commands are not here: they travel as a stream (vk/frame.rs). What is here answers at once.

/// Alt Manager: the accounts the game is played with (nicknames, Microsoft's), the one in use.
pub mod alts;
pub mod chrome;
pub mod cosmetics;
/// This computer's parts, hashed, for the license (Cursed's, with the TPM, the video card, the monitor, the firmware's key).
pub mod device;
pub mod engine;
pub mod gui;
pub mod itemphysic;
pub mod launch;
/// Recode's license in the game: the start the server confirmed, the game alive every minute, the subscription's end.
pub mod license;
/// The loader of Cursed Recode (src/bin/recode_loader.rs): sign-in through the site, the build, the start.
pub mod loader;
pub mod menus;
pub mod motion;
pub mod notice;
pub mod ours;
mod client_marks;
pub mod twin;
pub mod scene;
pub mod vk;
pub mod weather;
pub mod world;
/// Requests to the site through WinHTTP (Cursed's).
pub mod net;
/// Discord Rich Presence (as 1.21.11's): the client on the player's Discord profile.
mod rpc;
/// A server's status asked before joining it (cursed.recode.via.Via: an older one goes through ViaVersion).
pub mod via;

/// The words a dumper looks for first — the doors, the environment's names, what the server calls a refusal, the
/// labels the seal and the signature are mixed under. Written into the binaries as noise by `build.rs` and turned
/// back on the first use, so a `strings` pass finds nothing to walk in by.
pub mod vault {
    include!(concat!(env!("OUT_DIR"), "/vault.rs"));
}

/// Sixty-four bytes that nothing in this game reads.
///
/// The protection writes whose copy this is over them the moment the game is mapped: the account it was cut for and
/// a mac over that under the pair's own ticket. It is there because a dump of a running game is the one leak a
/// copy's own key cannot speak for — every player's game is the same bytes — and this is what makes one of those
/// dumps say whose it was. A mark written by hand answers to no ticket.
///
/// They are here and nowhere else because the protection needs somewhere to write and this is that somewhere.
/// Sixty-four of one byte is not a padding value and does not come up in a compiled library, so a run of them is
/// not something a reader stumbles over; and nothing in the game looks at them, so there is nothing here to patch
/// out that changes how the game plays.
#[used]
#[link_section = ".data"]
pub static CURSED_COPY: [u8; 64] = [0xA5; 64];

use jni_sys::*;
use std::cell::RefCell;
use std::ffi::{c_char, c_void, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use vk::device::Gpu;
use vk::pipeline::Pipeline;
use vk::resources::{Buffer, Doomed, Queries, Sampler, Texture, View};
use vk::surface::Surface;
use vk::Renderer;

thread_local! {
    /// Why the last `open` failed on this thread: reason, message, what is missing.
    static FAILURE: RefCell<Option<(i32, String, Vec<String>)>> = const { RefCell::new(None) };
    /// The last panic's message and place, for the exception.
    static PANIC: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// The message of the last panic on this thread (the frame thread reports its own this way).
pub fn take_panic_message() -> Option<String> {
    PANIC.with(|p| p.borrow_mut().take())
}

fn install_panic_hook() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        std::panic::set_hook(Box::new(|info| {
            let message = if let Some(s) = info.payload().downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = info.payload().downcast_ref::<String>() {
                s.clone()
            } else {
                "panic".to_string()
            };
            let place = info.location().map(|l| format!(" ({}:{})", l.file(), l.line())).unwrap_or_default();
            let text = format!("{message}{place}");
            eprintln!("[Cursed Recode native] {text}");
            PANIC.with(|p| *p.borrow_mut() = Some(text));
        }));
    });
}

unsafe fn throw(env: *mut JNIEnv, message: &str) {
    let class = ((**env).FindClass.unwrap())(env, c"java/lang/IllegalStateException".as_ptr());
    if class.is_null() {
        return;
    }
    let text = CString::new(message.replace('\0', " ")).unwrap();
    ((**env).ThrowNew.unwrap())(env, class, text.as_ptr());
}

fn guard<T>(env: *mut JNIEnv, fallback: T, f: impl FnOnce() -> T) -> T {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(v) => v,
        Err(_) => {
            let text = take_panic_message().unwrap_or_else(|| "panic".into());
            unsafe { throw(env, &format!("Cursed Recode Vulkan: {text}")) };
            fallback
        }
    }
}

fn string(env: *mut JNIEnv, s: &str) -> jstring {
    let text = CString::new(s.replace('\0', " ")).unwrap();
    unsafe { ((**env).NewStringUTF.unwrap())(env, text.as_ptr()) }
}

fn renderer<'a>(r: jlong) -> &'a Renderer {
    unsafe { &*(r as usize as *const Renderer) }
}

fn obj<'a, T>(p: jlong) -> &'a T {
    assert!(p != 0, "null object handle");
    unsafe { &*(p as usize as *const T) }
}

fn bytes<'a>(addr: jlong, len: jlong) -> &'a [u8] {
    if addr == 0 || len <= 0 {
        return &[];
    }
    unsafe { std::slice::from_raw_parts(addr as usize as *const u8, len as usize) }
}

fn into_handle<T>(v: T) -> jlong {
    Box::into_raw(Box::new(v)) as usize as jlong
}

// ---- the device -------------------------------------------------------------------------------------------------

extern "system" fn open(env: *mut JNIEnv, _: jclass, log_level: jint, labels: jboolean, validation: jboolean) -> jlong {
    // a release build draws nothing before the server confirmed its start (license.rs); the clock is started again
    // here because a dumper who took out the one at the load still has this one
    license::watch();
    license::confirm();
    guard(env, 0, || match Gpu::new(log_level, labels != 0, validation != 0) {
        Ok(gpu) => into_handle(Renderer::new(gpu)),
        Err(f) => {
            FAILURE.with(|x| *x.borrow_mut() = Some((f.reason as i32, f.message, f.missing)));
            0
        }
    })
}

extern "system" fn failure(env: *mut JNIEnv, _: jclass) -> jstring {
    let text = FAILURE.with(|x| x.borrow().as_ref().map(|f| f.1.clone())).unwrap_or_default();
    string(env, &text)
}

extern "system" fn failure_reason(_: *mut JNIEnv, _: jclass) -> jint {
    FAILURE.with(|x| x.borrow().as_ref().map_or(10, |f| f.0))
}

extern "system" fn failure_missing(env: *mut JNIEnv, _: jclass) -> jstring {
    let text = FAILURE.with(|x| x.borrow().as_ref().map(|f| f.2.join("\n"))).unwrap_or_default();
    string(env, &text)
}

extern "system" fn text(env: *mut JNIEnv, _: jclass, r: jlong, which: jint) -> jstring {
    let info = &renderer(r).gpu().info;
    let s = match which {
        0 => info.name.clone(),
        1 => info.vendor.clone(),
        2 => info.driver.clone(),
        // The compute shaders (vk/shaders.rs): 100 + i the source, 200 + i the name; past the last, nothing.
        100..=199 => vk::shaders::SOURCES.get(which as usize - 100).map_or(String::new(), |s| s.1.to_string()),
        200..=299 => vk::shaders::SOURCES.get(which as usize - 200).map_or(String::new(), |s| s.0.to_string()),
        // The engine's terrain shaders (engine/mod.rs).
        300 => engine::TERRAIN_VERTEX.to_string(),
        301 => engine::TERRAIN_FRAGMENT.to_string(),
        // For the creatures' lit twins (cursed.recode.render.EntityEngine).
        302 => engine::TRACE.to_string(),
        303 => engine::ENTITY_LIGHT.to_string(),
        304 => engine::SKY.to_string(),
        // Chams' matters in the creatures' twins (cursed.recode.chams.Chams).
        305 => engine::CHAMS.to_string(),
        _ => info.extensions.join("\n"),
    };
    string(env, &s)
}

extern "system" fn number(_: *mut JNIEnv, _: jclass, r: jlong, which: jint) -> jlong {
    let gpu = renderer(r).gpu();
    let i = &gpu.info;
    match which {
        0 => i.device_type as jlong,
        1 => i.timestamp_period.to_bits() as jlong,
        2 => i.max_anisotropy as jlong,
        3 => i.min_uniform_align as jlong,
        4 => i.max_texture as jlong,
        5 => i.max_allocation as jlong,
        6 => i.max_multi_draw as jlong,
        7 => i.max_color_attachments as jlong,
        8 => i.max_draw_indirect as jlong,
        9 => i.wireframe as jlong,
        10 => i.multi_draw as jlong,
        11 => gpu.debug.is_some() as jlong,
        12 => gpu.memory.rebar as jlong,
        13 => vk::encoder::FIRST_SUBMIT as jlong,
        _ => 0,
    }
}

extern "system" fn messages(env: *mut JNIEnv, _: jclass) -> jstring {
    let pending = std::mem::take(&mut *vk::device::MESSAGES.lock().unwrap());
    if pending.is_empty() {
        return std::ptr::null_mut();
    }
    string(env, &pending.join("\n"))
}

extern "system" fn close(env: *mut JNIEnv, _: jclass, r: jlong) {
    guard(env, (), || {
        let boxed = unsafe { Box::from_raw(r as usize as *mut Renderer) };
        boxed.destroy();
    })
}

extern "system" fn reply(_: *mut JNIEnv, _: jclass, r: jlong) -> jlong {
    renderer(r).producer().reply.as_ptr() as usize as jlong
}

extern "system" fn name(env: *mut JNIEnv, _: jclass, r: jlong, kind: jint, object: jlong, addr: jlong, len: jint) {
    guard(env, (), || {
        let gpu = renderer(r).gpu();
        let text = String::from_utf8_lossy(bytes(addr, len as jlong)).into_owned();
        match kind {
            0 => gpu.name(obj::<Buffer>(object).raw, &text),
            1 => gpu.name(obj::<Texture>(object).image, &text),
            _ => {}
        }
    })
}

// ---- the window ------------------------------------------------------------------------------------------------

extern "system" fn surface_create(env: *mut JNIEnv, _: jclass, r: jlong, hwnd: jlong) -> jlong {
    guard(env, 0, || into_handle(Surface::new(renderer(r).gpu(), hwnd as isize)))
}

extern "system" fn surface_modes(_: *mut JNIEnv, _: jclass, s: jlong) -> jint {
    obj::<Surface>(s).modes as jint
}

/// On the frame thread, after the frames sent before it; an error goes back to the game as a SurfaceException.
/// The menus out of a world: frames at the monitor's pace (vk/pace.rs), the swap chain as it is.
extern "system" fn surface_paced(_: *mut JNIEnv, _: jclass, paced: jboolean) {
    crate::vk::pace::PACED.store(paced != 0, std::sync::atomic::Ordering::Relaxed);
}

extern "system" fn surface_configure(env: *mut JNIEnv, _: jclass, r: jlong, s: jlong, w: jint, h: jint, mode: jint) -> jint {
    guard(env, 1, || {
        let result = renderer(r).call(move |worker| {
            let surface = unsafe { &mut *(s as usize as *mut Surface) };
            surface.configure(&worker.shared.gpu, w as u32, h as u32, mode)
        });
        match result {
            Ok(()) => 0,
            Err(e) => {
                FAILURE.with(|x| *x.borrow_mut() = Some((10, e, Vec::new())));
                1
            }
        }
    })
}

extern "system" fn surface_suboptimal(_: *mut JNIEnv, _: jclass, s: jlong) -> jboolean {
    obj::<Surface>(s).flags.suboptimal.load(std::sync::atomic::Ordering::Relaxed) as jboolean
}

/// Whether the swapchain went out of date on the frame thread since the last question (the game's acquire throws).
extern "system" fn surface_failed(_: *mut JNIEnv, _: jclass, s: jlong) -> jboolean {
    obj::<Surface>(s).flags.failed.swap(false, std::sync::atomic::Ordering::AcqRel) as jboolean
}

extern "system" fn surface_close(env: *mut JNIEnv, _: jclass, r: jlong, s: jlong) {
    guard(env, (), || {
        renderer(r).call(move |worker| {
            let mut boxed = unsafe { Box::from_raw(s as usize as *mut Surface) };
            boxed.destroy(&worker.shared.gpu);
        })
    })
}

// ---- objects ---------------------------------------------------------------------------------------------------

extern "system" fn buffer_create(env: *mut JNIEnv, _: jclass, r: jlong, usage: jint, size: jlong) -> jlong {
    guard(env, 0, || {
        let b = Buffer::new(renderer(r).gpu(), usage, size as u64).unwrap_or_else(|e| panic!("Failed to allocate VkBuffer: {e:?}"));
        into_handle(b)
    })
}

extern "system" fn buffer_memory(_: *mut JNIEnv, _: jclass, b: jlong) -> jlong {
    obj::<Buffer>(b).ptr() as usize as jlong
}

extern "system" fn buffer_cached(_: *mut JNIEnv, _: jclass, b: jlong) -> jboolean {
    obj::<Buffer>(b).cached() as jboolean
}

/// The texture's first layout is set by the frame thread (an `init texture` command Java writes right after).
#[allow(clippy::too_many_arguments)]
extern "system" fn texture_create(
    env: *mut JNIEnv,
    _: jclass,
    r: jlong,
    usage: jint,
    format: jint,
    aspect: jint,
    width: jint,
    height: jint,
    layers: jint,
    mips: jint,
) -> jlong {
    guard(env, 0, || {
        let t = Texture::new(
            renderer(r).gpu(),
            usage,
            ash::vk::Format::from_raw(format),
            ash::vk::ImageAspectFlags::from_raw(aspect as u32),
            width as u32,
            height as u32,
            layers as u32,
            mips as u32,
        )
        .unwrap_or_else(|e| panic!("Failed to create image: {e:?}"));
        into_handle(t)
    })
}

extern "system" fn view_create(env: *mut JNIEnv, _: jclass, r: jlong, t: jlong, base: jint, mips: jint) -> jlong {
    guard(env, 0, || {
        let v = View::new(renderer(r).gpu(), t as usize as *const Texture, base as u32, mips as u32)
            .unwrap_or_else(|e| panic!("Failed to create VkImageView: {e:?}"));
        into_handle(v)
    })
}

#[allow(clippy::too_many_arguments)]
extern "system" fn sampler_create(
    env: *mut JNIEnv,
    _: jclass,
    r: jlong,
    u: jint,
    v: jint,
    min: jint,
    mag: jint,
    anisotropy: jint,
    max_lod: jfloat,
    has_max_lod: jboolean,
) -> jlong {
    guard(env, 0, || {
        let s = Sampler::new(renderer(r).gpu(), u, v, min, mag, anisotropy, (has_max_lod != 0).then_some(max_lod))
            .unwrap_or_else(|e| panic!("Can't create sampler: {e:?}"));
        into_handle(s)
    })
}

extern "system" fn queries_create(env: *mut JNIEnv, _: jclass, r: jlong, size: jint) -> jlong {
    guard(env, 0, || into_handle(Queries::new(renderer(r).gpu(), size as u32).unwrap_or_else(|e| panic!("Cannot create query pool: {e:?}"))))
}

extern "system" fn queries_read(env: *mut JNIEnv, _: jclass, r: jlong, q: jlong, first: jint, count: jint, out: jlong) {
    guard(env, (), || {
        let q = obj::<Queries>(q);
        assert!(first >= 0 && count >= 0 && (first + count) as u32 <= q.size, "query read out of bounds");
        let out = unsafe { std::slice::from_raw_parts_mut(out as usize as *mut [u64; 2], count as usize) };
        q.read(renderer(r).gpu(), first as u32, out).unwrap_or_else(|e| panic!("Cannot fetch query results: {e:?}"));
    })
}

extern "system" fn pipeline_create(env: *mut JNIEnv, _: jclass, r: jlong, info: jlong, len: jint) -> jlong {
    guard(env, 0, || into_handle(Pipeline::compile(renderer(r).gpu(), bytes(info, len as jlong))))
}

extern "system" fn pipeline_close(env: *mut JNIEnv, _: jclass, r: jlong, p: jlong) {
    guard(env, (), || renderer(r).defer(Doomed::Pipeline(unsafe { Box::from_raw(p as usize as *mut Pipeline) })))
}

/// An object closed off the game's render thread: closed on the frame thread in its turn.
extern "system" fn close_later(env: *mut JNIEnv, _: jclass, r: jlong, kind: jint, p: jlong) {
    guard(env, (), || {
        let mut bytes = Vec::with_capacity(16);
        bytes.extend_from_slice(&vk::frame::CLOSE.to_le_bytes());
        bytes.extend_from_slice(&kind.to_le_bytes());
        bytes.extend_from_slice(&p.to_le_bytes());
        renderer(r).send(vk::frame::Message::Ops(bytes));
    })
}

// ---- the frame ---------------------------------------------------------------------------------------------------

/// A piece of the frame's stream, handed to the frame thread.
extern "system" fn frame_ops(env: *mut JNIEnv, _: jclass, r: jlong, addr: jlong, len: jint) {
    guard(env, (), || renderer(r).send(vk::frame::Message::Ops(bytes(addr, len as jlong).to_vec())))
}

/// The last piece of a frame and its submit. Returns the newest frame known done on the GPU.
extern "system" fn frame_submit(env: *mut JNIEnv, _: jclass, r: jlong, addr: jlong, len: jint) -> jlong {
    guard(env, 0, || renderer(r).submit(bytes(addr, len as jlong)) as jlong)
}

extern "system" fn completed(_: *mut JNIEnv, _: jclass, r: jlong) -> jlong {
    renderer(r).shared.progress.completed_now() as jlong
}

extern "system" fn await_frame(env: *mut JNIEnv, _: jclass, r: jlong, index: jlong, timeout: jlong) -> jboolean {
    guard(env, 0, || renderer(r).shared.await_frame(index as u64, timeout) as jboolean)
}

extern "system" fn timestamp_now(env: *mut JNIEnv, _: jclass, r: jlong) -> jlong {
    guard(env, 0, || renderer(r).call(|w| w.enc.timestamp_now(&w.shared.gpu)) as jlong)
}

/// Bytes put in the game's staging memory for a copy the stream will name; the reply holds buffer and offset.
extern "system" fn stage(env: *mut JNIEnv, _: jclass, r: jlong, addr: jlong, len: jlong) {
    guard(env, (), || {
        let rr = renderer(r);
        let p = rr.producer();
        let n = len.max(0) as u64;
        let piece = p.transient.staging(rr.gpu(), n.max(1), 4, n.max(1), 1);
        unsafe { std::ptr::copy_nonoverlapping(bytes(addr, len).as_ptr(), piece.host, n as usize) };
        p.reply[0] = piece.buffer as usize as u64;
        p.reply[1] = piece.offset;
    })
}

/// A transient piece (kind 0 cpu, 1 staging, 2 gpu, 3 mapped), left in the reply: buffer, offset, size, host address,
/// size of the block's buffer.
#[allow(clippy::too_many_arguments)]
extern "system" fn transient_alloc(env: *mut JNIEnv, _: jclass, r: jlong, kind: jint, size: jlong, align: jlong, minimum: jlong, element: jlong) {
    guard(env, (), || {
        let rr = renderer(r);
        let p = rr.producer();
        let gpu = rr.gpu();
        let (size, align, minimum, element) = (size as u64, align as u64, minimum as u64, element as u64);
        let t = &mut p.transient;
        let piece = match kind {
            0 => t.cpu(size, align, minimum, element),
            1 => t.staging(gpu, size, align, minimum, element),
            2 => t.gpu(gpu, size, align, minimum, element),
            _ => t.mapped(gpu, size, align, minimum, element),
        };
        let block = if piece.buffer.is_null() { 0 } else { unsafe { (*piece.buffer).size } };
        p.reply[..5].copy_from_slice(&[piece.buffer as usize as u64, piece.offset, piece.size, piece.host as usize as u64, block]);
    })
}

extern "system" fn transient_fits(_: *mut JNIEnv, _: jclass, r: jlong, mapped: jboolean, size: jlong, align: jlong) -> jboolean {
    renderer(r).producer().transient.fits(mapped != 0, size as u64, align as u64) as jboolean
}

// ---- the terrain (vk/terrain.rs) ------------------------------------------------------------------------------

extern "system" fn terrain_mesh(_: *mut JNIEnv, _: jclass, r: jlong) -> jint {
    renderer(r).shared.terrain_feed.new_mesh() as jint
}

#[allow(clippy::too_many_arguments)]
extern "system" fn terrain_upload(
    env: *mut JNIEnv,
    _: jclass,
    r: jlong,
    mesh: jint,
    layer: jint,
    vertices: jlong,
    vertices_len: jint,
    indices: jlong,
    indices_len: jint,
    index_count: jint,
    index_u32: jboolean,
    facings: jlong,
    x: jint,
    y: jint,
    z: jint,
) {
    guard(env, (), || {
        let v = (vertices != 0).then(|| bytes(vertices, vertices_len as jlong));
        let i = (indices != 0).then(|| bytes(indices, indices_len as jlong));
        let f = (facings != 0).then(|| {
            let b = bytes(facings, world::mesher::FACINGS as jlong * 4);
            std::array::from_fn(|k| u32::from_le_bytes(b[k * 4..k * 4 + 4].try_into().unwrap()))
        });
        renderer(r).shared.terrain_feed.upload(mesh as u32, layer as usize, v, i, index_count as u32, index_u32 != 0, f, [x, y, z]);
    })
}

extern "system" fn terrain_release(_: *mut JNIEnv, _: jclass, r: jlong, mesh: jint, after: jlong) {
    renderer(r).shared.terrain_feed.release(mesh as u32, after as u64);
}

extern "system" fn terrain_compact(_: *mut JNIEnv, _: jclass, r: jlong, packed: jboolean) {
    renderer(r).shared.terrain_feed.packed.store(packed != 0, std::sync::atomic::Ordering::Relaxed);
}

extern "system" fn terrain_reset(_: *mut JNIEnv, _: jclass, r: jlong) {
    renderer(r).shared.terrain_feed.reset();
    engine::voxels::reset();
}

// ---- the engine (engine/) ----------------------------------------------------------------------------------------

/// The block states' materials after a resource reload (cursed.recode.engine.Materials).
extern "system" fn engine_materials(env: *mut JNIEnv, _: jclass, addr: jlong, len: jint) {
    guard(env, (), || engine::voxels::load_materials(bytes(addr, len as jlong)))
}

/// A section the game leaves empty (only air): the mesher never sees it.
extern "system" fn engine_air(_: *mut JNIEnv, _: jclass, ox: jint, oy: jint, oz: jint) {
    engine::voxels::air([ox, oy, oz]);
}

// ---- the world's geometry (world/) ------------------------------------------------------------------------------

/// The block tables after a resource reload (cursed.recode.world.BlockExport).
extern "system" fn world_tables(env: *mut JNIEnv, _: jclass, addr: jlong, len: jint) {
    guard(env, (), || world::blocks::load(bytes(addr, len as jlong)))
}

/// Where things are in a mesh job's memory (cursed.recode.world.NativeMesher.Job has the same numbers).
mod job {
    pub const STATES: usize = 0;
    pub const LIGHT: usize = 32000;
    pub const CARDINAL: usize = 40000;
    pub const OUT: usize = 40064;
    /// After OUT: i32 vertices x 3, i32 block entities, i32 fluids, pad, u64 visibility, then i32 quads per facing
    /// x 3 layers x 11 (world/mesher.rs FACINGS) here, then the positions.
    pub const FACINGS: usize = OUT + 32;
    pub const POSITIONS: usize = OUT + 192;
    pub const SIZE: usize = POSITIONS + 8192 * 4;
}

thread_local! {
    /// The last section meshed on this thread, until Java copies its layers.
    static MESH: RefCell<world::mesher::Output> = RefCell::new(Default::default());
}

/// A section's blocks: 0 done, 1 the game should compile it, -1 Java threw (its exception is pending).
#[allow(clippy::too_many_arguments)]
extern "system" fn world_mesh(env: *mut JNIEnv, _: jclass, mem: jlong, ox: jint, oy: jint, oz: jint, flags: jint, job: jobject) -> jint {
    guard(env, 1, || {
        let Some(tables) = world::blocks::current() else { return 1 };
        let base = mem as usize as *mut u8;
        assert!(!base.is_null(), "mesh job memory");
        let (states, light, cardinal) = unsafe {
            (
                std::slice::from_raw_parts(base.add(job::STATES) as *const i32, world::mesher::VOLUME),
                std::slice::from_raw_parts(base.add(job::LIGHT), world::mesher::VOLUME),
                std::slice::from_raw_parts(base.add(job::CARDINAL) as *const f32, 6),
            )
        };
        engine::voxels::section([ox, oy, oz], states, light);
        let input = world::mesher::Input {
            states,
            light,
            origin: [ox, oy, oz],
            ambient_occlusion: flags & 1 != 0,
            cutout_leaves: flags & 2 != 0,
            cardinal: cardinal.try_into().unwrap(),
        };
        let method = tint_method(env, job);
        let mut tint = |local: u32, state: i32, index: i32| -> Option<i32> {
            unsafe {
                let args = [jvalue { i: local as jint }, jvalue { i: state }, jvalue { i: index }];
                let v = ((**env).CallIntMethodA.unwrap())(env, job, method, args.as_ptr());
                if ((**env).ExceptionCheck.unwrap())(env) != 0 {
                    None
                } else {
                    Some(v)
                }
            }
        };
        MESH.with(|m| {
            let mut out = m.borrow_mut();
            match world::mesher::mesh(&tables, &input, &mut tint, &mut out) {
                world::mesher::Outcome::Fallback => 1,
                world::mesher::Outcome::Thrown => -1,
                world::mesher::Outcome::Done => {
                    unsafe {
                        let o = base.add(job::OUT);
                        let ints = o as *mut i32;
                        for l in 0..3 {
                            *ints.add(l) = (out.layers[l].len() / world::mesher::VERTEX) as i32;
                        }
                        *ints.add(3) = out.block_entities.len() as i32;
                        *ints.add(4) = out.fluids.len() as i32;
                        *(o.add(24) as *mut u64) = out.visibility;
                        let f = base.add(job::FACINGS) as *mut u32;
                        for l in 0..3 {
                            for k in 0..world::mesher::FACINGS {
                                *f.add(l * world::mesher::FACINGS + k) = out.facings[l][k];
                            }
                        }
                        let list = base.add(job::POSITIONS) as *mut u32;
                        let n = out.block_entities.len();
                        assert!(n + out.fluids.len() <= 8192, "mesh job positions");
                        std::ptr::copy_nonoverlapping(out.block_entities.as_ptr(), list, n);
                        std::ptr::copy_nonoverlapping(out.fluids.as_ptr(), list.add(n), out.fluids.len());
                    }
                    0
                }
            }
        })
    })
}

/// NativeMesher.Job.tint(III)I, looked up once (the class stays loaded with the game).
fn tint_method(env: *mut JNIEnv, job: jobject) -> jmethodID {
    static METHOD: std::sync::atomic::AtomicPtr<std::ffi::c_void> = std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());
    let m = METHOD.load(std::sync::atomic::Ordering::Acquire);
    if !m.is_null() {
        return m as jmethodID;
    }
    unsafe {
        let class = ((**env).GetObjectClass.unwrap())(env, job);
        let id = ((**env).GetMethodID.unwrap())(env, class, c"tint".as_ptr(), c"(III)I".as_ptr());
        assert!(!id.is_null(), "NativeMesher.Job.tint(III)I");
        METHOD.store(id as *mut std::ffi::c_void, std::sync::atomic::Ordering::Release);
        id
    }
}

/// Copies a layer of the last meshed section into the game's buffer.
extern "system" fn world_copy(_: *mut JNIEnv, _: jclass, layer: jint, dst: jlong) {
    MESH.with(|m| {
        let out = m.borrow();
        let src = &out.layers[layer as usize];
        if !src.is_empty() {
            unsafe { std::ptr::copy_nonoverlapping(src.as_ptr(), dst as usize as *mut u8, src.len()) };
        }
    })
}

extern "system" fn world_job_size(_: *mut JNIEnv, _: jclass) -> jint {
    job::SIZE as jint
}

// ---- entity models (scene/models.rs) ------------------------------------------------------------------------------

extern "system" fn model_part(env: *mut JNIEnv, _: jclass, addr: jlong, len: jint) -> jint {
    guard(env, -1, || scene::models::add_part(bytes(addr, len as jlong)) as jint)
}

/// TextureUtil.solidify on an RGBA NativeImage's pixels (scene/textures.rs).
extern "system" fn texture_solidify(env: *mut JNIEnv, _: jclass, pixels: jlong, width: jint, height: jint) {
    guard(env, (), || {
        let (w, h) = (width.max(0) as usize, height.max(0) as usize);
        let p = unsafe { std::slice::from_raw_parts_mut(pixels as usize as *mut u32, w * h) };
        scene::textures::solidify(p, w, h);
    })
}

extern "system" fn model_quads_add(env: *mut JNIEnv, _: jclass, addr: jlong, count: jint) -> jint {
    guard(env, -1, || {
        let n = count.max(0) as usize;
        scene::models::add_quads(bytes(addr, (n * scene::models::ITEM_QUAD) as jlong), n) as jint
    })
}

extern "system" fn model_part_first(env: *mut JNIEnv, _: jclass, id: jint) -> jint {
    guard(env, -1, || scene::models::part_first(id as u32) as jint)
}

/// The compute shaders (vk/shaders.rs) compiled by Java: i32 index, i32 byte length, the SPIR-V, for each. Made on
/// the frame thread before anything uses them: the models' (vk/models.rs) and, unless `flags` says no, occlusion's.
extern "system" fn shaders_init(env: *mut JNIEnv, _: jclass, r: jlong, data: jlong, len: jint, flags: jint) {
    guard(env, (), || {
        let b = bytes(data, len as jlong);
        let mut spirv: Vec<Vec<u32>> = vec![Vec::new(); vk::shaders::SOURCES.len()];
        let mut at = 0usize;
        while at + 8 <= b.len() {
            let index = u32::from_le_bytes(b[at..at + 4].try_into().unwrap()) as usize;
            let n = u32::from_le_bytes(b[at + 4..at + 8].try_into().unwrap()) as usize;
            let code = &b[at + 8..at + 8 + n];
            spirv[index] = code.chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect();
            at += 8 + n.div_ceil(4) * 4;
        }
        assert!(spirv.iter().all(|s| !s.is_empty()), "A compute shader is missing");
        renderer(r).call(move |w| {
            let gpu = &w.shared.gpu;
            let m = vk::models::GpuModels::new(gpu, &spirv[vk::shaders::MODELS], flags & vk::shaders::CHECK_MODELS != 0);
            if let Some(mut old) = w.models.replace(m) {
                old.destroy(gpu);
            }
            w.engine.set_mask_shader(gpu, &spirv[vk::shaders::ENGINE_MASK]);
            w.engine.upscale.set_shader(gpu, &spirv[vk::shaders::ENGINE_TAA]);
            w.engine.set_light_shader(gpu, &spirv[vk::shaders::ENGINE_LIGHT]);
            w.engine.set_water_shader(gpu, &spirv[vk::shaders::ENGINE_WATER]);
            w.engine.set_sunvis_shader(gpu, &spirv[vk::shaders::ENGINE_SUNVIS]);
            w.engine.upscale.set_volume_shader(gpu, &spirv[vk::shaders::ENGINE_VOLUME]);
            w.engine.upscale.set_glow_shader(gpu, &spirv[vk::shaders::ENGINE_GLOW]);
            w.engine.set_skylut_shader(gpu, &spirv[vk::shaders::ENGINE_SKYLUT]);
            let [ia, ib, ic, id] = vk::shaders::INK;
            w.engine.ink.set_shaders(gpu, &spirv[ia], &spirv[ib], &spirv[ic], &spirv[id]);
            w.engine.ground.set_shader(gpu, &spirv[vk::shaders::GROUND]);
            w.engine.liquid.set_shader(gpu, &spirv[vk::shaders::LIQUID]);
            w.engine.curse.set_shader(gpu, &spirv[vk::shaders::CURSE]);
            w.engine.tesseract.set_shader(gpu, &spirv[vk::shaders::TESSERACT]);
            w.engine.shadow.set_shader(gpu, &spirv[vk::shaders::SHADOW]);
            w.engine.hitboxes.set_shader(gpu, &spirv[vk::shaders::HITBOXES]);
            w.engine.fx.set_shader(gpu, &spirv[vk::shaders::FX]);
            let [vv, vf] = vk::shaders::VIEWMODEL;
            w.engine.viewmodel.set_shaders(&spirv[vv], &spirv[vf]);
            let [sv, sf] = vk::shaders::SCOPE;
            w.engine.viewmodel.set_scope_shaders(&spirv[sv], &spirv[sf]);
            let [tv, tf] = vk::shaders::STICKER;
            w.engine.viewmodel.set_sticker_shaders(&spirv[tv], &spirv[tf]);
            w.engine.viewmodel.set_resolve_shader(&spirv[vk::shaders::RESOLVE]);
            w.engine.viewmodel.set_finish_vs(&spirv[vk::shaders::FINISH_VS]);
            w.engine.viewmodel.set_glove_vs(&spirv[vk::shaders::GLOVE_FINISH_VS]);
            let [iv, ifr] = vk::shaders::IMPOSTOR;
            w.terrain.impostor.set_shaders(&spirv[iv], &spirv[ifr]);
            if flags & vk::shaders::NO_OCCLUSION == 0 {
                let [a, b, c] = vk::shaders::OCCLUSION;
                let o = vk::occlusion::Occlusion::new(gpu, [&spirv[a], &spirv[b], &spirv[c]]);
                if let Some(mut old) = w.terrain.occlusion.replace(o) {
                    old.destroy(gpu);
                }
            }
        });
    })
}

/// Bytes put in the game's staging memory where a storage buffer may start; the reply holds buffer and offset.
extern "system" fn stage_storage(env: *mut JNIEnv, _: jclass, r: jlong, addr: jlong, len: jlong) {
    guard(env, (), || {
        let rr = renderer(r);
        let align = rr.gpu().info.min_storage_align.max(4);
        let p = rr.producer();
        let n = len.max(0) as u64;
        let piece = p.transient.staging(rr.gpu(), n.max(1), align, n.max(1), 1);
        unsafe { std::ptr::copy_nonoverlapping(bytes(addr, len).as_ptr(), piece.host, n as usize) };
        p.reply[0] = piece.buffer as usize as u64;
        p.reply[1] = piece.offset;
    })
}

#[allow(clippy::too_many_arguments)]
extern "system" fn model_vertices(env: *mut JNIEnv, _: jclass, records: jlong, count: jint, light: jint, overlay: jint, color: jint, dst: jlong) -> jint {
    guard(env, 0, || {
        let n = count.max(0) as usize;
        let r = bytes(records, (n * scene::models::RECORD) as jlong);
        unsafe { scene::models::vertices(r, n, light, overlay, color, dst as usize as *mut u8) as jint }
    })
}

extern "system" fn model_quads(env: *mut JNIEnv, _: jclass, header: jlong, quads: jlong, count: jint, dst: jlong) -> jint {
    guard(env, 0, || {
        let n = count.max(0) as usize;
        let h = bytes(header, scene::models::QUAD_HEADER as jlong);
        let q = bytes(quads, (n * scene::models::QUAD) as jlong);
        unsafe { scene::models::quads(h, q, n, dst as usize as *mut u8) as jint }
    })
}

thread_local! {
    /// A particle layer's arrays, copied out of the Java heap (pinning them would hold the garbage collector up).
    static PARTICLES: RefCell<(Vec<f32>, Vec<i32>)> = const { RefCell::new((Vec::new(), Vec::new())) };
}

/// A particle layer's quads, from copies of the game's arrays (QuadParticleRenderState.Storage).
extern "system" fn particle_quads(env: *mut JNIEnv, _: jclass, floats: jfloatArray, ints: jintArray, count: jint, dst: jlong) -> jint {
    guard(env, 0, || unsafe {
        let n = count.max(0) as usize;
        PARTICLES.with(|p| {
            let (f, i) = &mut *p.borrow_mut();
            f.resize(n * 12, 0.0);
            i.resize(n * 2, 0);
            ((**env).GetFloatArrayRegion.unwrap())(env, floats, 0, (n * 12) as jsize, f.as_mut_ptr());
            ((**env).GetIntArrayRegion.unwrap())(env, ints, 0, (n * 2) as jsize, i.as_mut_ptr());
            if ((**env).ExceptionCheck.unwrap())(env) != 0 {
                return 0;
            }
            scene::particles::quads(f, i, n, dst as usize as *mut u8) as jint
        })
    })
}

extern "system" fn text_glyphs(env: *mut JNIEnv, _: jclass, header: jlong, glyphs: jlong, count: jint, dst: jlong) -> jint {
    guard(env, 0, || {
        let n = count.max(0) as usize;
        let h = bytes(header, scene::text::HEADER as jlong);
        let g = bytes(glyphs, (n * scene::text::GLYPH) as jlong);
        unsafe { scene::text::glyphs(h, g, n, dst as usize as *mut u8) as jint }
    })
}

/// Translucent quads ordered for the upload (cursed.recode.scene.QuadSort): `desc` holds `count` slices of 16 bytes
/// (pointer, vertex count, 0); the indices go to `dst`. Returns the quads written.
extern "system" fn quads_sort(env: *mut JNIEnv, _: jclass, desc: jlong, count: jint, stride: jint, position: jint, mode: jint, dst: jlong, index_bytes: jint) -> jint {
    guard(env, 0, || {
        let d = bytes(desc, count.max(0) as jlong * 16);
        let slices: Vec<(usize, usize)> = d
            .chunks_exact(16)
            .map(|c| (i64::from_le_bytes(c[0..8].try_into().unwrap()) as usize, i32::from_le_bytes(c[8..12].try_into().unwrap()).max(0) as usize))
            .collect();
        unsafe { scene::sort::quads(&slices, stride as usize, position as usize, mode, dst as usize as *mut u8, index_bytes as usize) as jint }
    })
}

/// An entity model's part tree walked for the GPU's vertices (cursed.recode.scene.NativeModels, scene/walk.rs): the
/// jobs go to `jobs`; `result` gets the vertices and 1 when a pose's normals are untrusted. Returns the jobs written.
extern "system" fn model_walk(
    env: *mut JNIEnv, _: jclass, statics: jlong, dynamic: jlong, n: jint, root: jlong, jobs: jlong, abgr: jint, overlay: jint, light: jint, argb: jint, result: jlong,
) -> jint {
    guard(env, 0, || {
        let (written, vertices, untrusted) = unsafe {
            scene::walk::walk(statics as usize as *const u8, dynamic as usize as *const u8, n.max(0) as usize, root as usize as *const u8, jobs as usize as *mut u8, abgr, overlay, light, argb)
        };
        unsafe {
            (result as usize as *mut i32).write_unaligned(vertices);
            (result as usize as *mut i32).add(1).write_unaligned(untrusted as i32);
        }
        written as jint
    })
}

/// The walk for a model with the CPU's vertices (scene/walk.rs walk_cpu): `result` gets the vertices and 1 when a
/// pose's normals are untrusted; then model_walk_write writes them.
extern "system" fn model_walk_cpu(env: *mut JNIEnv, _: jclass, statics: jlong, dynamic: jlong, n: jint, root: jlong, result: jlong) {
    guard(env, (), || {
        let (vertices, untrusted) = unsafe {
            scene::walk::walk_cpu(statics as usize as *const u8, dynamic as usize as *const u8, n.max(0) as usize, root as usize as *const u8)
        };
        unsafe {
            (result as usize as *mut i32).write_unaligned(vertices);
            (result as usize as *mut i32).add(1).write_unaligned(untrusted as i32);
        }
    })
}

extern "system" fn model_walk_write(env: *mut JNIEnv, _: jclass, light: jint, overlay: jint, color: jint, dst: jlong) -> jint {
    guard(env, 0, || unsafe { scene::walk::write_cpu(light, overlay, color, dst as usize as *mut u8) as jint })
}

// ---- the window before the game (twin/mod.rs) -------------------------------------------------------------------

/// Where the game's window goes and the window before the game for it: client x y w h, frame x y w h.
extern "system" fn twin_place(env: *mut JNIEnv, _: jclass, w: jint, h: jint, at_x: jint, at_y: jint, at: jboolean) -> jintArray {
    guard(env, std::ptr::null_mut(), || {
        let p = twin::place(w, h, (at != 0).then_some([at_x, at_y]));
        let v = [p.client[0], p.client[1], p.client[2], p.client[3], p.frame[0], p.frame[1], p.frame[2], p.frame[3]];
        unsafe {
            let a = ((**env).NewIntArray.unwrap())(env, 8);
            ((**env).SetIntArrayRegion.unwrap())(env, a, 0, 8, v.as_ptr());
            a
        }
    })
}

fn twin_place_of(env: *mut JNIEnv, a: jintArray) -> twin::Place2 {
    let mut v = [0i32; 8];
    unsafe { ((**env).GetIntArrayRegion.unwrap())(env, a, 0, 8, v.as_mut_ptr()) };
    twin::Place2 { client: [v[0], v[1], v[2], v[3]], frame: [v[4], v[5], v[6], v[7]] }
}

/// Where the shown window `hwnd` is (client x y w h, frame x y w h).
extern "system" fn twin_place_of_window(env: *mut JNIEnv, _: jclass, hwnd: jlong) -> jintArray {
    guard(env, std::ptr::null_mut(), || {
        let p = twin::place_of(hwnd as usize as *mut std::ffi::c_void);
        let v = [p.client[0], p.client[1], p.client[2], p.client[3], p.frame[0], p.frame[1], p.frame[2], p.frame[3]];
        unsafe {
            let a = ((**env).NewIntArray.unwrap())(env, 8);
            ((**env).SetIntArrayRegion.unwrap())(env, a, 0, 8, v.as_ptr());
            a
        }
    })
}

/// The game joins the window before the game started before it (CURSED_TWIN): where its window goes, or null.
extern "system" fn twin_attach(env: *mut JNIEnv, _: jclass, name: jstring) -> jintArray {
    guard(env, std::ptr::null_mut(), || {
        let name = unsafe {
            let chars = ((**env).GetStringUTFChars.unwrap())(env, name, std::ptr::null_mut());
            let s = std::ffi::CStr::from_ptr(chars).to_string_lossy().into_owned();
            ((**env).ReleaseStringUTFChars.unwrap())(env, name, chars);
            s
        };
        match twin::attach(&name) {
            Some(p) => {
                let v = [p.client[0], p.client[1], p.client[2], p.client[3], p.frame[0], p.frame[1], p.frame[2], p.frame[3]];
                unsafe {
                    let a = ((**env).NewIntArray.unwrap())(env, 8);
                    ((**env).SetIntArrayRegion.unwrap())(env, a, 0, 8, v.as_ptr());
                    a
                }
            }
            None => std::ptr::null_mut(),
        }
    })
}

extern "system" fn twin_start(env: *mut JNIEnv, _: jclass, place: jintArray, mute: jboolean) -> jboolean {
    guard(env, 0, || twin::start(twin_place_of(env, place), mute != 0) as jboolean)
}

/// The card's clocks from the window before the game (twin::times): life, card, forge, bits.
extern "system" fn twin_times(env: *mut JNIEnv, _: jclass, out: jfloatArray) {
    guard(env, (), || {
        let v = twin::times();
        unsafe { ((**env).SetFloatArrayRegion.unwrap())(env, out, 0, 4, v.as_ptr()) };
    })
}

/// The game's window in our look (titlebar.rs): her icon, the bar's colours; `transitions` whether Windows animates it.
extern "system" fn window_look(env: *mut JNIEnv, _: jclass, hwnd: jlong, transitions: jboolean) {
    guard(env, (), || unsafe { twin::titlebar::style_game_window(hwnd as usize as *mut std::ffi::c_void, transitions != 0) })
}

/// This process's windows are one program on the taskbar with the window before the game's.
extern "system" fn app_id(env: *mut JNIEnv, _: jclass) {
    guard(env, (), twin::app_id)
}

/// The game's window over the other windows, the focus asked for (titlebar.rs bring_up).
extern "system" fn window_front(env: *mut JNIEnv, _: jclass, hwnd: jlong) {
    guard(env, (), || unsafe { twin::titlebar::bring_up(hwnd as usize as *mut std::ffi::c_void) })
}

/// The game's window up minimised, its taskbar button blinking (titlebar.rs show_minimised).
extern "system" fn window_minimised(env: *mut JNIEnv, _: jclass, hwnd: jlong) {
    guard(env, (), || unsafe { twin::titlebar::show_minimised(hwnd as usize as *mut std::ffi::c_void) })
}

/// Whether the window before the game is minimised (the game's window comes up minimised then).
extern "system" fn twin_hidden(env: *mut JNIEnv, _: jclass) -> jboolean {
    guard(env, 0, || twin::hidden() as jboolean)
}

// ---- the window's own title bar (chrome/) -----------------------------------------------------------------------

/// The game's window gets our title bar; its picture (under the bar) put at x y w h when `placed`, else where SDL put
/// its client area.
extern "system" fn chrome_install(env: *mut JNIEnv, _: jclass, hwnd: jlong, x: jint, y: jint, w: jint, h: jint, placed: jboolean) {
    guard(env, (), || unsafe { chrome::install(hwnd as usize as *mut std::ffi::c_void, if placed != 0 { Some([x, y, w, h]) } else { None }) })
}

/// The bar's height on the game's window now (px; 0 without it: fullscreen).
extern "system" fn chrome_inset(env: *mut JNIEnv, _: jclass) -> jint {
    guard(env, 0, chrome::inset)
}

/// The game's window put so that its picture is at x y w h (the bar over it).
extern "system" fn chrome_place(env: *mut JNIEnv, _: jclass, hwnd: jlong, x: jint, y: jint, w: jint, h: jint) {
    guard(env, (), || unsafe { chrome::place(hwnd as usize as *mut std::ffi::c_void, [x, y, w, h]) })
}

/// Where the game is, after the bar's name (a world's, a server's name; empty: nothing).
extern "system" fn chrome_status(env: *mut JNIEnv, _: jclass, text: jstring) {
    let s = java_str(env, text);
    guard(env, (), || chrome::set_status(&s))
}

extern "system" fn twin_ready(env: *mut JNIEnv, _: jclass) {
    guard(env, (), twin::ready)
}

/// The game's resources loading (0..1): the card's point runs that far along its line.
extern "system" fn twin_load(env: *mut JNIEnv, _: jclass, p: jfloat) {
    guard(env, (), || twin::load(p))
}

extern "system" fn twin_release(env: *mut JNIEnv, _: jclass) {
    guard(env, (), twin::release)
}

/// The game is closing: Discord takes the presence off the profile (waited for a moment at most).
extern "system" fn rpc_stop(env: *mut JNIEnv, _: jclass) {
    guard(env, (), || rpc::shutdown(std::time::Duration::from_millis(400)))
}

/// The way out over the game's window at `place`: `menu` = card, desk's second, title's left, build line's top, seconds
/// since the way out began, from where (0 the main menu, 1 a world); `view` = look x y, cursor x y on, hover glow, sheet
/// lightning.
extern "system" fn twin_farewell(env: *mut JNIEnv, _: jclass, place: jintArray, menu: jfloatArray, view: jfloatArray) -> jboolean {
    guard(env, 0, || {
        let mut m = [0f32; 6];
        let mut v = [0f32; 7];
        unsafe {
            ((**env).GetFloatArrayRegion.unwrap())(env, menu, 0, 6, m.as_mut_ptr());
            ((**env).GetFloatArrayRegion.unwrap())(env, view, 0, 7, v.as_mut_ptr());
        }
        twin::farewell(twin_place_of(env, place), twin::View { card: m[0], fx: m[1], left: m[2], foot_top: m[3], exit_ago: m[4], view: v, maximized: chrome::maximized(), active: chrome::active(), kind: m[5] as u32 }) as jboolean
    })
}

extern "system" fn twin_shown(env: *mut JNIEnv, _: jclass) -> jboolean {
    guard(env, 0, || twin::shown() as jboolean)
}

extern "system" fn twin_busy(env: *mut JNIEnv, _: jclass) -> jboolean {
    guard(env, 0, || twin::busy() as jboolean)
}

// ---- the launch's sound (launch/sound.rs) -----------------------------------------------------------------------

extern "system" fn launch_volume(env: *mut JNIEnv, _: jclass, master: jfloat, music: jfloat) {
    guard(env, (), || launch::sound::volume(master, music))
}

extern "system" fn launch_tick(env: *mut JNIEnv, _: jclass) {
    // the hover's tick: as heard in the preview she liked (its file is ~10 dB under that)
    guard(env, (), || launch::sound::play(launch::sound::Sfx::Tick, 3.2, 0.0))
}

fn java_str(env: *mut JNIEnv, s: jstring) -> String {
    if s.is_null() {
        return String::new();
    }
    unsafe {
        let chars = ((**env).GetStringUTFChars.unwrap())(env, s, std::ptr::null_mut());
        let out = std::ffi::CStr::from_ptr(chars).to_string_lossy().into_owned();
        ((**env).ReleaseStringUTFChars.unwrap())(env, s, chars);
        out
    }
}

/// The client's menu opens (gui/mod.rs): the window's px, the player's name, the folder its settings are kept in.
extern "system" fn gui_open(env: *mut JNIEnv, _: jclass, w: jfloat, h: jfloat, player: jstring, dir: jstring) {
    let (player, dir) = (java_str(env, player), java_str(env, dir));
    guard(env, (), || gui::open(w, h, &player, &dir))
}

/// Lays out a frame of the menu; its number for the frame op (0: nothing to draw).
extern "system" fn gui_frame(env: *mut JNIEnv, _: jclass, w: jfloat, h: jfloat) -> jint {
    guard(env, 0, || gui::frame(w, h) as jint)
}

/// The mouse in window px: action 0 moved, 1 pressed, 2 released; button 0 left, 1 right, 2 middle, more.
extern "system" fn gui_mouse(env: *mut JNIEnv, _: jclass, x: jfloat, y: jfloat, button: jint, action: jint) -> jboolean {
    guard(env, 0, || gui::mouse(x, y, button, action) as jboolean)
}

extern "system" fn gui_scroll(env: *mut JNIEnv, _: jclass, dy: jfloat) {
    guard(env, (), || gui::scroll(dy))
}

/// A key, by its name: 0 not the menu's, 1 taken, 2 the menu goes.
extern "system" fn gui_key(env: *mut JNIEnv, _: jclass, escape: jboolean, enter: jboolean, backspace: jboolean, name: jstring, menu_key: jboolean) -> jint {
    let name = java_str(env, name);
    guard(env, 0, || gui::key(escape != 0, enter != 0, backspace != 0, &name, menu_key != 0))
}

extern "system" fn gui_char(env: *mut JNIEnv, _: jclass, c: jint) {
    guard(env, (), || gui::typed(c as u32))
}

/// The window folds away; `now`: it is gone already (the screen was taken away).
extern "system" fn gui_close(env: *mut JNIEnv, _: jclass, now: jboolean) {
    guard(env, (), || gui::close(now != 0))
}

/// [shown (0 closed, 1 opening, 2 open, 3 closing), the head's x, y, size, alpha, a key is waited for]
/// Living Motion (motion.rs): a player's pose from what Java says of it (`inp`, motion::IN numbers) into `out`
/// (motion::OUT).
extern "system" fn motion_pose(env: *mut JNIEnv, _: jclass, inp: jfloatArray, out: jfloatArray) {
    guard(env, (), || {
        let mut v = [0f32; motion::IN];
        unsafe { ((**env).GetFloatArrayRegion.unwrap())(env, inp, 0, motion::IN as i32, v.as_mut_ptr()) };
        let o = motion::pose(&v);
        unsafe { ((**env).SetFloatArrayRegion.unwrap())(env, out, 0, motion::OUT as i32, o.as_ptr()) };
    })
}

/// Item Physic (itemphysic.rs): a dropped item's copies from where it is (`at`), what it is and does (`inp`,
/// itemphysic::IN numbers) into `out` (itemphysic::OUT).
extern "system" fn item_pose(env: *mut JNIEnv, _: jclass, at: jdoubleArray, inp: jfloatArray, out: jfloatArray) {
    guard(env, (), || {
        let mut p = [0f64; 3];
        let mut v = [0f32; itemphysic::IN];
        unsafe {
            ((**env).GetDoubleArrayRegion.unwrap())(env, at, 0, 3, p.as_mut_ptr());
            ((**env).GetFloatArrayRegion.unwrap())(env, inp, 0, itemphysic::IN as i32, v.as_mut_ptr());
        }
        let o = itemphysic::pose(&v, p);
        unsafe { ((**env).SetFloatArrayRegion.unwrap())(env, out, 0, itemphysic::OUT as i32, o.as_ptr()) };
    })
}

extern "system" fn gui_state(env: *mut JNIEnv, _: jclass, out: jfloatArray) {
    guard(env, (), || {
        let v = gui::state();
        unsafe { ((**env).SetFloatArrayRegion.unwrap())(env, out, 0, 6, v.as_ptr()) };
    })
}

/// A frame of the HUD (gui/hud.rs) from the game's numbers and names; its number for the frame op (0: nothing).
extern "system" fn hud_frame(env: *mut JNIEnv, _: jclass, w: jfloat, h: jfloat, nums: jdoubleArray, text: jstring) -> jint {
    let text = java_str(env, text);
    let n = unsafe { ((**env).GetArrayLength.unwrap())(env, nums) }.max(0) as usize;
    let mut v = vec![0f64; n];
    unsafe { ((**env).GetDoubleArrayRegion.unwrap())(env, nums, 0, n as i32, v.as_mut_ptr()) };
    guard(env, 0, || gui::hud_frame(w, h, &v, &text) as jint)
}

/// The icons over the HUD, 7 numbers each, into `out`: how many.
extern "system" fn hud_icons(env: *mut JNIEnv, _: jclass, out: jfloatArray) -> jint {
    icons_out(env, out, gui::hud_icons)
}

/// The icons over the open container's panel (Custom Gui), as hud_icons.
extern "system" fn hud_screen_icons(env: *mut JNIEnv, _: jclass, out: jfloatArray) -> jint {
    icons_out(env, out, gui::hud_screen_icons)
}

fn icons_out(env: *mut JNIEnv, out: jfloatArray, get: fn() -> Vec<[f32; 7]>) -> jint {
    guard(env, 0, || {
        let v = get();
        let cap = unsafe { ((**env).GetArrayLength.unwrap())(env, out) }.max(0) as usize / 7;
        let n = v.len().min(cap);
        let flat: Vec<f32> = v[..n].iter().flatten().copied().collect();
        if n > 0 {
            unsafe { ((**env).SetFloatArrayRegion.unwrap())(env, out, 0, flat.len() as i32, flat.as_ptr()) };
        }
        n as jint
    })
}

extern "system" fn hud_mouse(env: *mut JNIEnv, _: jclass, x: jfloat, y: jfloat, button: jint, action: jint) -> jboolean {
    guard(env, 0, || gui::hud_mouse(x, y, button, action) as jboolean)
}

extern "system" fn hud_scroll(env: *mut JNIEnv, _: jclass, dy: jfloat) -> jboolean {
    guard(env, 0, || gui::hud_scroll(dy) as jboolean)
}

extern "system" fn hud_key(env: *mut JNIEnv, _: jclass, escape: jboolean, enter: jboolean, backspace: jboolean) -> jboolean {
    guard(env, 0, || gui::hud_key(escape != 0, enter != 0, backspace != 0) as jboolean)
}

extern "system" fn hud_char(env: *mut JNIEnv, _: jclass, c: jint) -> jboolean {
    guard(env, 0, || gui::hud_char(c as u32) as jboolean)
}

/// A key in the world with no screen: the modules bound to it switch. Whether one did.
extern "system" fn hud_bind(env: *mut JNIEnv, _: jclass, name: jstring) -> jboolean {
    let name = java_str(env, name);
    guard(env, 0, || gui::hud_bind(&name) as jboolean)
}

/// Something the game saw: 1 a totem popped (whose), 2 the target was hit.
extern "system" fn hud_event(env: *mut JNIEnv, _: jclass, kind: jint, who: jstring) {
    let who = java_str(env, who);
    guard(env, (), || gui::hud_event(kind, &who))
}

/// The frame of what is open by an element (its settings, a list, the colour wheel), made with the HUD's last; 0 none.
extern "system" fn hud_overlay(env: *mut JNIEnv, _: jclass) -> jint {
    guard(env, 0, || gui::hud_overlay() as jint)
}

/// Custom Inventory's frame (gui/hud/hudinv.rs): its picture under the items (its number), what Java draws into `out`
/// (how many numbers).
extern "system" fn inv_frame(env: *mut JNIEnv, _: jclass, w: jfloat, h: jfloat, nums: jdoubleArray, text: jstring) -> jint {
    let text = java_str(env, text);
    let n = unsafe { ((**env).GetArrayLength.unwrap())(env, nums) }.max(0) as usize;
    let mut v = vec![0f64; n];
    unsafe { ((**env).GetDoubleArrayRegion.unwrap())(env, nums, 0, n as i32, v.as_mut_ptr()) };
    guard(env, 0, || gui::inv_frame(w, h, &v, &text) as jint)
}

/// Custom Inventory's picture over the items (its counts and wear), made with the frame; 0 none.
extern "system" fn inv_top(env: *mut JNIEnv, _: jclass) -> jint {
    guard(env, 0, || gui::inv_top() as jint)
}

/// What Java draws where (hudinv.rs Gui::inv_frame) into `out`: how many numbers.
extern "system" fn inv_out(env: *mut JNIEnv, _: jclass, out: jfloatArray) -> jint {
    guard(env, 0, || {
        let v = gui::inv_out();
        let cap = unsafe { ((**env).GetArrayLength.unwrap())(env, out) }.max(0) as usize;
        let n = v.len().min(cap);
        if n > 0 {
            unsafe { ((**env).SetFloatArrayRegion.unwrap())(env, out, 0, n as i32, v.as_ptr()) };
        }
        n as jint
    })
}

/// The frame under the open container (Custom Gui), made with the HUD's last; 0 none.
extern "system" fn hud_screen(env: *mut JNIEnv, _: jclass) -> jint {
    guard(env, 0, || gui::hud_screen() as jint)
}

extern "system" fn hud_closed(env: *mut JNIEnv, _: jclass) {
    guard(env, (), gui::hud_closed)
}

extern "system" fn hud_init(env: *mut JNIEnv, _: jclass, player: jstring, dir: jstring) {
    let (player, dir) = (java_str(env, player), java_str(env, dir));
    guard(env, (), || gui::hud_init(&player, &dir))
}

/// [the game's effect icons hidden, the layout editor asked for]
extern "system" fn hud_state(env: *mut JNIEnv, _: jclass, out: jfloatArray) {
    guard(env, (), || {
        let v = gui::hud_state();
        unsafe { ((**env).SetFloatArrayRegion.unwrap())(env, out, 0, v.len() as i32, v.as_ptr()) };
    })
}

/// Java's other words to the menu and the HUD (gui::call): one line, fields by tabs; what comes back.
extern "system" fn gui_call(env: *mut JNIEnv, _: jclass, cmd: jstring) -> jstring {
    let cmd = java_str(env, cmd);
    let out = guard(env, String::new(), || gui::call(&cmd));
    string(env, &out)
}

/// The key that opens the menu, by its name (Settings).
extern "system" fn gui_menu_key(env: *mut JNIEnv, _: jclass) -> jstring {
    let k = guard(env, String::new(), gui::menu_key);
    string(env, &k)
}

/// 0 pause/play on (returns whether it plays), 1 from the start, 2 out (the way out, a world entered), 3 in again
/// (back in the menu).
extern "system" fn launch_music(env: *mut JNIEnv, _: jclass, action: jint) -> jboolean {
    guard(env, 0, || match action {
        0 => launch::sound::music_toggle() as jboolean,
        1 => {
            launch::sound::music_restart();
            1
        }
        3 => {
            launch::sound::music_in(2.5);
            1
        }
        // into a world, out of one: no sounds of their own (her word, 15.10)
        5 | 6 => 1,
        _ => {
            // the way out of the menu, into a world: the music goes
            launch::sound::music_out(2.6);
            0
        }
    })
}

/// 0 seconds played, 1 the track's length, 2 heard (1/0).
extern "system" fn launch_music_state(env: *mut JNIEnv, _: jclass, which: jint) -> jfloat {
    guard(env, 0.0, || {
        let (pos, len, on) = launch::sound::music_state();
        match which {
            0 => pos,
            1 => len,
            _ => on as i32 as f32,
        }
    })
}

// ---- the menus (menus/): the game's screens drawn by the client ------------------------------------------------

fn java_bytes(env: *mut JNIEnv, a: jbyteArray) -> Vec<u8> {
    if a.is_null() {
        return Vec::new();
    }
    let n = unsafe { ((**env).GetArrayLength.unwrap())(env, a) }.max(0) as usize;
    let mut v = vec![0u8; n];
    if n > 0 {
        unsafe { ((**env).GetByteArrayRegion.unwrap())(env, a, 0, n as i32, v.as_mut_ptr() as *mut i8) };
    }
    v
}

/// A screen of the game opened (`kind` the page that draws it), its data as UTF-8 JSON.
extern "system" fn menus_open(env: *mut JNIEnv, _: jclass, id: jint, kind: jstring, json: jbyteArray) {
    let (kind, json) = (java_str(env, kind), java_bytes(env, json));
    guard(env, (), || menus::open(id as u32, &kind, &String::from_utf8_lossy(&json)))
}

extern "system" fn menus_data(env: *mut JNIEnv, _: jclass, id: jint, json: jbyteArray) {
    let json = java_bytes(env, json);
    guard(env, (), || menus::data(id as u32, &String::from_utf8_lossy(&json)))
}

extern "system" fn menus_close(env: *mut JNIEnv, _: jclass, id: jint) {
    guard(env, (), || menus::close(id as u32))
}

/// A frame of the menus: the window's px, their clock (s), what the screen says, whether a world is behind; what
/// they say back into `out` (64); its number for the frame op (0: nothing to draw).
extern "system" fn menus_frame(env: *mut JNIEnv, _: jclass, w: jfloat, h: jfloat, now: jfloat, inp: jfloatArray, world: jboolean, out: jfloatArray) -> jint {
    let n = unsafe { ((**env).GetArrayLength.unwrap())(env, inp) }.clamp(0, 32) as usize;
    let mut v = [0f32; 32];
    unsafe { ((**env).GetFloatArrayRegion.unwrap())(env, inp, 0, n as i32, v.as_mut_ptr()) };
    guard(env, 0, || {
        let (id, o) = menus::frame(w, h, now, &v[..n], world != 0);
        unsafe { ((**env).SetFloatArrayRegion.unwrap())(env, out, 0, 64, o.as_ptr()) };
        id as jint
    })
}

extern "system" fn menus_mouse(env: *mut JNIEnv, _: jclass, x: jfloat, y: jfloat, button: jint, action: jint) {
    guard(env, (), || menus::mouse(x, y, button, action))
}

extern "system" fn menus_scroll(env: *mut JNIEnv, _: jclass, dy: jfloat) {
    guard(env, (), || menus::scroll(dy))
}

extern "system" fn menus_key(env: *mut JNIEnv, _: jclass, name: jstring, mods: jint) {
    let name = java_str(env, name);
    guard(env, (), || menus::key(&name, mods as u32))
}

extern "system" fn menus_char(env: *mut JNIEnv, _: jclass, c: jint) {
    guard(env, (), || menus::typed(c as u32))
}

/// What the menus ask of the game since the last time, a line each (UTF-8 bytes).
extern "system" fn menus_poll(env: *mut JNIEnv, _: jclass) -> jbyteArray {
    let s = guard(env, String::new(), menus::poll);
    unsafe {
        let a = ((**env).NewByteArray.unwrap())(env, s.len() as i32);
        if !a.is_null() && !s.is_empty() {
            ((**env).SetByteArrayRegion.unwrap())(env, a, 0, s.len() as i32, s.as_ptr() as *const i8);
        }
        a
    }
}

/// What the game draws over the page this frame (menus::extras), 6 numbers each: how many.
extern "system" fn menus_extras(env: *mut JNIEnv, _: jclass, out: jfloatArray) -> jint {
    guard(env, 0, || {
        let cap = unsafe { ((**env).GetArrayLength.unwrap())(env, out) }.max(0) as usize;
        let mut v = vec![0f32; cap];
        let n = menus::extras(&mut v);
        if n > 0 {
            unsafe { ((**env).SetFloatArrayRegion.unwrap())(env, out, 0, (n * 6) as i32, v.as_ptr()) };
        }
        n as jint
    })
}

/// The game's language changed (its code): the client's own words follow.
extern "system" fn menus_lang(env: *mut JNIEnv, _: jclass, code: jstring) {
    let code = java_str(env, code);
    guard(env, (), || menus::lang::set(&code))
}

/// A picture for the menus: `w` × `h` RGBA8, rows from the top.
extern "system" fn menus_pic(env: *mut JNIEnv, _: jclass, key: jstring, w: jint, h: jint, rgba: jbyteArray) {
    let (key, px) = (java_str(env, key), java_bytes(env, rgba));
    guard(env, (), || gui::pics::put(&key, w.max(0) as u32, h.max(0) as u32, px))
}

extern "system" fn menus_pic_has(env: *mut JNIEnv, _: jclass, key: jstring) -> jboolean {
    let key = java_str(env, key);
    guard(env, 0, || gui::pics::has(&key) as jboolean)
}

// ---- the license (license.rs) --------------------------------------------------------------------------------

/// What the game is told: 0 all well; 1 the time running out (out: seconds left, why: 0 the subscription ends, 1 no
/// server); 2 go now (out: seconds since; its words: licenseWords).
extern "system" fn license_state(env: *mut JNIEnv, _: jclass, out: jfloatArray) -> jint {
    guard(env, 0, || {
        let (kind, v) = match license::now() {
            license::Say::Fine => (0, [0.0f32; 2]),
            license::Say::Closing { end, why } => (1, [end.saturating_duration_since(std::time::Instant::now()).as_secs_f32(), if why == license::Why::Ending { 0.0 } else { 1.0 }]),
            license::Say::Leave { since, .. } => (2, [since.elapsed().as_secs_f32(), 0.0]),
        };
        unsafe { ((**env).SetFloatArrayRegion.unwrap())(env, out, 0, 2, v.as_ptr()) };
        kind
    })
}

/// Go now: what the screen says, as at the start (license::Stop, JSON: cap, title, text, words [action, label, kind]).
/// "" when the game is not told to go.
extern "system" fn license_words(env: *mut JNIEnv, _: jclass) -> jstring {
    let text = guard(env, String::new(), || match license::now() {
        license::Say::Leave { stop, .. } => {
            let words: Vec<serde_json::Value> = stop.words.iter().map(|w| serde_json::json!([w.0, w.1, w.2])).collect();
            serde_json::json!({ "cap": stop.cap, "title": stop.title, "text": stop.text, "words": words }).to_string()
        }
        _ => String::new(),
    });
    string(env, &text)
}

/// The game closes: the account is free for the next start at once.
extern "system" fn license_bye(env: *mut JNIEnv, _: jclass) {
    guard(env, (), license::bye)
}

// ---- via -------------------------------------------------------------------------------------------------------

/// The protocol a server answers its status with (-1: no answer in `timeout` ms).
extern "system" fn via_ping(env: *mut JNIEnv, _: jclass, host: jstring, port: jint, timeout: jint) -> jint {
    let host = java_str(env, host);
    guard(env, -1, || via::ping(&host, port as u16, timeout.max(1) as u64))
}


// ---- registration ----------------------------------------------------------------------------------------------

macro_rules! method {
    ($name:literal, $sig:literal, $f:expr) => {
        JNINativeMethod { name: concat!($name, "\0").as_ptr() as *mut c_char, signature: concat!($sig, "\0").as_ptr() as *mut c_char, fnPtr: $f as *mut c_void }
    };
}

#[no_mangle]
pub extern "system" fn JNI_OnLoad(vm: *mut JavaVM, _reserved: *mut c_void) -> jint {
    // her notice is read first: it stands in the code that runs first (notice.rs)
    std::hint::black_box(notice::keep());
    install_panic_hook();
    // the clock starts here too: a game that has to be kept alive by the server and is not, is ended (license.rs)
    license::watch();
    // a release build asks the server about its start at once (its own thread; the device waits for the answer)
    license::start();
    // Discord shows the client from the start (its own thread; nothing waits for it)
    let _ = std::panic::catch_unwind(rpc::start);
    let mut env: *mut c_void = std::ptr::null_mut();
    unsafe {
        if ((**vm).GetEnv.unwrap())(vm, &mut env, JNI_VERSION_1_8) != JNI_OK {
            return JNI_ERR;
        }
    }
    let env = env as *mut JNIEnv;
    let methods = [
        method!("open", "(IZZ)J", open),
        method!("failure", "()Ljava/lang/String;", failure),
        method!("failureReason", "()I", failure_reason),
        method!("failureMissing", "()Ljava/lang/String;", failure_missing),
        method!("text", "(JI)Ljava/lang/String;", text),
        method!("number", "(JI)J", number),
        method!("messages", "()Ljava/lang/String;", messages),
        method!("close", "(J)V", close),
        method!("reply", "(J)J", reply),
        method!("name", "(JIJJI)V", name),
        method!("surfaceCreate", "(JJ)J", surface_create),
        method!("surfaceModes", "(J)I", surface_modes),
        method!("surfaceConfigure", "(JJIII)I", surface_configure),
        method!("surfacePaced", "(Z)V", surface_paced),
        method!("surfaceSuboptimal", "(J)Z", surface_suboptimal),
        method!("surfaceFailed", "(J)Z", surface_failed),
        method!("surfaceClose", "(JJ)V", surface_close),
        method!("bufferCreate", "(JIJ)J", buffer_create),
        method!("bufferMemory", "(J)J", buffer_memory),
        method!("bufferCached", "(J)Z", buffer_cached),
        method!("textureCreate", "(JIIIIIII)J", texture_create),
        method!("viewCreate", "(JJII)J", view_create),
        method!("samplerCreate", "(JIIIIIFZ)J", sampler_create),
        method!("queriesCreate", "(JI)J", queries_create),
        method!("queriesRead", "(JJIIJ)V", queries_read),
        method!("pipelineCreate", "(JJI)J", pipeline_create),
        method!("pipelineClose", "(JJ)V", pipeline_close),
        method!("closeLater", "(JIJ)V", close_later),
        method!("frameOps", "(JJI)V", frame_ops),
        method!("frameSubmit", "(JJI)J", frame_submit),
        method!("completed", "(J)J", completed),
        method!("await", "(JJJ)Z", await_frame),
        method!("timestampNow", "(J)J", timestamp_now),
        method!("stage", "(JJJ)V", stage),
        method!("transientAlloc", "(JIJJJJ)V", transient_alloc),
        method!("transientFits", "(JZJJ)Z", transient_fits),
        method!("terrainMesh", "(J)I", terrain_mesh),
        method!("terrainUpload", "(JIIJIJIIZJIII)V", terrain_upload),
        method!("terrainRelease", "(JIJ)V", terrain_release),
        method!("terrainReset", "(J)V", terrain_reset),
        method!("terrainCompact", "(JZ)V", terrain_compact),
        method!("worldTables", "(JI)V", world_tables),
        method!("worldMesh", "(JIIIILjava/lang/Object;)I", world_mesh),
        method!("worldCopy", "(IJ)V", world_copy),
        method!("worldJobSize", "()I", world_job_size),
        method!("modelPart", "(JI)I", model_part),
        method!("modelPartFirst", "(I)I", model_part_first),
        method!("modelQuadsAdd", "(JI)I", model_quads_add),
        method!("textureSolidify", "(JII)V", texture_solidify),
        method!("shadersInit", "(JJII)V", shaders_init),
        method!("stageStorage", "(JJJ)V", stage_storage),
        method!("modelVertices", "(JIIIIJ)I", model_vertices),
        method!("modelQuads", "(JJIJ)I", model_quads),
        method!("particleQuads", "([F[IIJ)I", particle_quads),
        method!("textGlyphs", "(JJIJ)I", text_glyphs),
        method!("quadsSort", "(JIIIIJI)I", quads_sort),
        method!("modelWalk", "(JJIJJIIIIJ)I", model_walk),
        method!("modelWalkCpu", "(JJIJJ)V", model_walk_cpu),
        method!("modelWalkWrite", "(IIIJ)I", model_walk_write),
        method!("engineMaterials", "(JI)V", engine_materials),
        method!("engineAir", "(III)V", engine_air),
        method!("twinPlace", "(IIIIZ)[I", twin_place),
        method!("twinStart", "([IZ)Z", twin_start),
        method!("twinAttach", "(Ljava/lang/String;)[I", twin_attach),
        method!("twinPlaceOf", "(J)[I", twin_place_of_window),
        method!("twinTimes", "([F)V", twin_times),
        method!("windowLook", "(JZ)V", window_look),
        method!("appId", "()V", app_id),
        method!("windowFront", "(J)V", window_front),
        method!("windowMinimised", "(J)V", window_minimised),
        method!("twinHidden", "()Z", twin_hidden),
        method!("chromeInstall", "(JIIIIZ)V", chrome_install),
        method!("chromeInset", "()I", chrome_inset),
        method!("chromePlace", "(JIIII)V", chrome_place),
        method!("chromeStatus", "(Ljava/lang/String;)V", chrome_status),
        method!("twinReady", "()V", twin_ready),
        method!("twinLoad", "(F)V", twin_load),
        method!("twinRelease", "()V", twin_release),
        method!("rpcStop", "()V", rpc_stop),
        method!("twinFarewell", "([I[F[F)Z", twin_farewell),
        method!("twinBusy", "()Z", twin_busy),
        method!("twinShown", "()Z", twin_shown),
        method!("launchVolume", "(FF)V", launch_volume),
        method!("launchTick", "()V", launch_tick),
        method!("launchMusic", "(I)Z", launch_music),
        method!("launchMusicState", "(I)F", launch_music_state),
        method!("guiOpen", "(FFLjava/lang/String;Ljava/lang/String;)V", gui_open),
        method!("guiFrame", "(FF)I", gui_frame),
        method!("guiMouse", "(FFII)Z", gui_mouse),
        method!("guiScroll", "(F)V", gui_scroll),
        method!("guiKey", "(ZZZLjava/lang/String;Z)I", gui_key),
        method!("guiChar", "(I)V", gui_char),
        method!("guiClose", "(Z)V", gui_close),
        method!("guiState", "([F)V", gui_state),
        method!("motionPose", "([F[F)V", motion_pose),
        method!("itemPose", "([D[F[F)V", item_pose),
        method!("guiMenuKey", "()Ljava/lang/String;", gui_menu_key),
        method!("guiCall", "(Ljava/lang/String;)Ljava/lang/String;", gui_call),
        method!("hudFrame", "(FF[DLjava/lang/String;)I", hud_frame),
        method!("hudIcons", "([F)I", hud_icons),
        method!("hudMouse", "(FFII)Z", hud_mouse),
        method!("hudScroll", "(F)Z", hud_scroll),
        method!("hudKey", "(ZZZ)Z", hud_key),
        method!("hudChar", "(I)Z", hud_char),
        method!("hudBind", "(Ljava/lang/String;)Z", hud_bind),
        method!("hudEvent", "(ILjava/lang/String;)V", hud_event),
        method!("hudClosed", "()V", hud_closed),
        method!("hudOverlay", "()I", hud_overlay),
        method!("hudScreen", "()I", hud_screen),
        method!("hudScreenIcons", "([F)I", hud_screen_icons),
        method!("invFrame", "(FF[DLjava/lang/String;)I", inv_frame),
        method!("invTop", "()I", inv_top),
        method!("invOut", "([F)I", inv_out),
        method!("hudInit", "(Ljava/lang/String;Ljava/lang/String;)V", hud_init),
        method!("hudState", "([F)V", hud_state),
        method!("menusOpen", "(ILjava/lang/String;[B)V", menus_open),
        method!("menusData", "(I[B)V", menus_data),
        method!("menusClose", "(I)V", menus_close),
        method!("menusFrame", "(FFF[FZ[F)I", menus_frame),
        method!("menusMouse", "(FFII)V", menus_mouse),
        method!("menusScroll", "(F)V", menus_scroll),
        method!("menusKey", "(Ljava/lang/String;I)V", menus_key),
        method!("menusChar", "(I)V", menus_char),
        method!("menusPoll", "()[B", menus_poll),
        method!("menusLang", "(Ljava/lang/String;)V", menus_lang),
        method!("menusExtras", "([F)I", menus_extras),
        method!("menusPic", "(Ljava/lang/String;II[B)V", menus_pic),
        method!("menusPicHas", "(Ljava/lang/String;)Z", menus_pic_has),
        method!("licenseState", "([F)I", license_state),
        method!("licenseWords", "()Ljava/lang/String;", license_words),
        method!("licenseBye", "()V", license_bye),
        method!("viaPing", "(Ljava/lang/String;II)I", via_ping),
    ];
    unsafe {
        let class = ((**env).FindClass.unwrap())(env, c"cursed/recode/render/Native".as_ptr());
        if class.is_null() {
            return JNI_ERR;
        }
        if ((**env).RegisterNatives.unwrap())(env, class, methods.as_ptr(), methods.len() as jint) != JNI_OK {
            return JNI_ERR;
        }
    }
    JNI_VERSION_1_8
}
