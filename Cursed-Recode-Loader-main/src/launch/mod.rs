//! The launch: the main menu's picture — a valley at dusk, the fire under the hovered word — and the card before it
//! (v12, «one line»: the wings drawn by strokes, CURSED rewritten into RECODE by a point burning along its line with the
//! game's loading, the wings lying down as the horizon, the world loading from it in chunks; the ways out and into a
//! world), drawn by our compute shaders (shaders/launch, SPIR-V in the DLL: the window before the game draws them before
//! the game's own shader compiler is up).
//!
//! Each frame: the scene (nothing of it until the horizon is there), the light over it (fx.comp), the sparks, the
//! glow, all put together into the picture, which is copied into the game's texture. Ported from the approved web
//! preview (Desktop\Cursed Recode Launch\v12); timeline.rs has its numbers.

pub mod ink;
pub mod sound;
pub mod timeline;

use crate::engine::gpu::{fail, image_in, retire, Compute};
use crate::vk::device::Gpu;
use crate::vk::encoder::{memory_barrier, Encoder};
use crate::vk::memory::Place;
use crate::vk::resources::{Texture, View};
use ash::vk;
use std::sync::atomic::{AtomicBool, AtomicU32};
use timeline::smooth;

/// Lines of light the menu draws (comp.comp LINES).
pub const LINES: usize = 12;
const UBO: u64 = 56 * 16;
const MENU_UBO: u64 = (LINES as u64 * 2 + 6) * 16;
const SMOKE_W: u32 = 192;
const SMOKE_H: u32 = 160;
/// The text atlas's spread, atlas px either side of the outline (tools/text_atlas.py SPREAD).
const ATLAS_SPREAD: f32 = 12.0;

pub(crate) fn words(bytes: &[u8]) -> Vec<u32> {
    bytes.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

/// What the caller says each frame.
#[derive(Clone, Copy)]
pub struct Inputs {
    /// The card's second (timeline.rs tl): the story — it waits on the title while the game loads.
    pub t: f32,
    /// The card's second that never waits: the forge's sparks, the word's heat, the kick finish on it.
    pub fx: f32,
    /// Always running: the fire, the water, the clouds.
    pub life: f32,
    pub dt: f32,
    /// From the flash on; before it, the entrance (the smoke gathers by `gather`).
    pub launched: bool,
    pub gather: f32,
    /// Start the entrance's smoke afresh: dust in the dark.
    pub smoke_reset: bool,
    /// x, y in 0..1 of the picture (y down), on.
    pub cursor: [f32; 3],
    pub look: [f32; 2],
    pub boost: f32,
    pub flash_c: f32,
    /// The picture's light, 1 normally (it dims on the way out).
    pub fade: f32,
    /// Picture px per ui px (the preview's css px).
    pub ui: f32,
    /// Where the title lands in the menu: the word's left edge, the build line's top (picture px from the top).
    pub left: f32,
    pub foot_top: f32,
    /// The burning lines under words: x0, y (from the bottom), x1, progress, strength (picture px).
    pub hover: [[f32; 5]; 2],
    pub hover_sparks: f32,
    pub lines: [[f32; 8]; LINES],
    /// The music's buttons: x, y (bottom left, from the bottom), size, kind (-1 none); their brightness, strength.
    pub icons: [[f32; 4]; 3],
    pub icon_light: [f32; 4],
    /// The music's record: x, y (bottom left, from the bottom), size, strength; its turn (rad), whether it plays.
    pub disc: [f32; 8],
    pub no_rays: bool,
    pub no_bloom: bool,
    /// The fire under the words is the menus' own (menus/, drawn over this picture): here only its sparks.
    pub menus_fire: bool,
    /// What the menus' screens ask of the world (shaders/launch/launch.glsl Knob0..5): KNOBS for the main menu.
    pub knobs: [f32; 24],
    /// The camera's place for the screen once the menu is up: along the river, height, pitch, yaw; the field of view.
    pub pose: [f32; 5],
    /// Which scene (v12) and its own second (Run: `t`).
    pub phase: timeline::Phase,
    pub pt: f32,
    /// The game's loading (Run, until it is ready) or the world's (Enter), 0..1; when the world had loaded (Enter's
    /// seconds, negative: not yet).
    pub load: f32,
    pub loaded_at: f32,
    /// The world's frame (going into a world, leaving one): its view, its width over its height.
    pub world: Option<(vk::ImageView, f32)>,
    /// Seconds since the window was closed while the launch played (the pane thaws away; -1: it was not).
    pub quit: f32,
}

/// The world as the main menu shows it (the approved valley).
pub const KNOBS: [f32; 24] = [1.0, 0.0, 1.0, 41.0, 0.0, 0.0, 6.0, 41.0, 1.0, 1e4, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0];
pub const POSE: [f32; 5] = [0.0, 22.0, -0.085, 0.0, 70.0];

/// The game's options the valley follows: render distance, clouds, their distance, particles, brightness, the field
/// of view, the graphics preset, the background's blur (menus/mod.rs gets them from Java each frame; the window before
/// the game reads them from options.txt). Render distance 0: not known yet.
static OPTIONS: std::sync::Mutex<[f32; 8]> = std::sync::Mutex::new([0.0; 8]);

pub fn set_options(o: [f32; 8]) {
    *OPTIONS.lock().unwrap() = o;
}

/// The options `o` on a valley `s` (the camera's place, then the knobs): how far one sees, clouds and how far, sparks,
/// brightness, the field of view, reflections (the graphics). Returns the background's blur for the other screens.
pub fn options_into(s: &mut [f32; 29], o: &[f32; 8]) -> f32 {
    if o[0] <= 0.0 {
        return 0.0;
    }
    let rd = o[0];
    let k = |i: usize| 5 + i;
    s[k(8)] = (12.0 / rd.max(2.0)).powf(1.15);
    s[k(9)] = if rd >= 12.0 { 1e4 } else { rd * 34.0 };
    s[k(10)] = [0.0, 0.55, 1.0][(o[1] as usize).min(2)];
    s[k(11)] = (128.0 / o[2].max(2.0)).powf(0.9).min(30.0);
    s[k(14)] = [1.0, 0.45, 0.15][(o[3] as usize).min(2)];
    s[k(15)] = 0.55 + o[4] * 0.9;
    s[4] = o[5];
    s[k(12)] = if o[6] < 0.5 { 0.0 } else { 1.0 };
    (o[7] / 10.0).powf(1.6) * 0.7
}

/// The main menu's valley as the game's options make it (the camera's place, then the knobs): the same in the menu,
/// on the ways into and out of a world, and in the window before the game — none of them leaps into another.
pub fn valley() -> [f32; 29] {
    let mut s = [0.0f32; 29];
    s[..5].copy_from_slice(&POSE);
    s[5..].copy_from_slice(&KNOBS);
    let o = *OPTIONS.lock().unwrap();
    options_into(&mut s, &o);
    s
}

/// The valley the game drew last (its place and knobs, as they were — gliding or not): the window before the game goes
/// on from it on the way out.
static DRAWN: std::sync::Mutex<Option<[f32; 29]>> = std::sync::Mutex::new(None);

pub fn drew(pose: &[f32; 5], knobs: &[f32; 24]) {
    let mut s = [0.0f32; 29];
    s[..5].copy_from_slice(pose);
    s[5..].copy_from_slice(knobs);
    *DRAWN.lock().unwrap() = Some(s);
}

pub fn last_drawn() -> Option<[f32; 29]> {
    *DRAWN.lock().unwrap()
}

pub(crate) struct Image {
    pub(crate) t: Box<Texture>,
    pub(crate) v: Box<View>,
}

/// Made once: the land, her logo, the menu's text, the title's letters, the smoke's homes and state.
struct Fixed {
    height: Image,
    logo: Image,
    atlas: Image,
    title: Image,
    smoke: [Image; 2],
    home: Image,
}

/// Made for a picture size.
struct Sized {
    w: u32,
    h: u32,
    sw: u32,
    sh: u32,
    scene: Image,
    /// the light over the world (fx.comp)
    fx: Image,
    part: Image,
    acc_part: Image,
    acc_smoke: Image,
    trail: Image,
    mips: Vec<Image>,
    rays: Image,
    out: Image,
    text_cov: Image,
    text_glow: Image,
    /// The picture under the ink (launch/ink.rs).
    inked: Image,
    /// The background's blur (the menus'): the scene down a few levels and back up.
    blur: Vec<Image>,
}

/// A renderer made on another thread: just made, it holds no pictures yet (they come with its first frame).
pub struct Made(pub Launch);
unsafe impl Send for Made {}

pub struct Launch {
    sky: Compute,
    world: Compute,
    height: Compute,
    sparks: Compute,
    resolve: Compute,
    down: Compute,
    up: Compute,
    rays: Compute,
    comp: Compute,
    ink: Compute,
    fxc: Compute,
    text: Compute,
    sim: Compute,
    splat: Compute,
    linear: vk::Sampler,
    mipped: vk::Sampler,
    fixed: Option<Fixed>,
    sized: Option<Sized>,
    smoke_alpha: f32,
    smoke_burst: f32,
    smoke_init: bool,
    smoke_cur: usize,
    /// What the pictures are made from, unpacked with the pipelines (off the frame that first draws).
    prepared: Option<Prepared>,
    /// Development (CURSED_DEV_LAUNCHPROF): GPU timestamps between the stages, two frames in turn; the last read.
    prof: Option<(vk::QueryPool, u32)>,
    pub prof_ms: [f32; PROF],
    /// The terrain the land map was made for (Knob0, the seed): made again when a new world's asks another.
    terrain: Option<[f32; 6]>,
    /// The title atlas's capital (atlas px).
    cap: f32,
    /// The burning point last frame (ui px, the clock then) and its speed: its sparks are born behind it.
    point_was: Option<(f32, f32)>,
    point_speed: f32,
}

/// The stages timed: smoke, scene, sparks, glow, rays, text, picture.
pub const PROF: usize = 7;

/// Her logo, the text's fields, the title's letters, the smoke's homes: unpacked and ready to upload.
struct Prepared {
    logo: Vec<u8>,
    atlas: Vec<u8>,
    title: Vec<u8>,
    homes: Vec<f32>,
}

impl Prepared {
    fn new() -> Prepared {
        let base = include_bytes!("../../assets/launch/logo_base.r8");
        // the wings: her logo made even (the left wing the right's mirror about the tip: v12's logo_sym)
        let sym = include_bytes!("../../assets/launch/logo_sym.r8");
        Prepared {
            logo: sym.iter().flat_map(|&g| [g, g, g, 255]).collect(),
            atlas: miniz_oxide::inflate::decompress_to_vec_zlib(include_bytes!("../../assets/launch/text_atlas.r8.z")).expect("The text atlas is broken"),
            title: miniz_oxide::inflate::decompress_to_vec_zlib(include_bytes!("../../assets/launch/title_atlas.r8.z")).expect("The title atlas is broken"),
            homes: homes(base),
        }
    }
}

use vk::DescriptorType as D;

impl Launch {
    pub fn new(gpu: &Gpu) -> Launch {
        let c = |name: &str, spv: &[u8], kinds: &[vk::DescriptorType], push: u32| Compute::new(gpu, &format!("launch_{name}"), &words(spv), kinds, push);
        let ub = D::UNIFORM_BUFFER;
        let st = D::STORAGE_IMAGE;
        let sa = D::COMBINED_IMAGE_SAMPLER;
        let sampler = |filter: vk::Filter, mips: bool| {
            let info = vk::SamplerCreateInfo::default()
                .mag_filter(filter)
                .min_filter(filter)
                .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
                .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .max_lod(if mips { 16.0 } else { 0.0 });
            fail(unsafe { gpu.device.create_sampler(&info, None) }, "Can't make a launch sampler")
        };
        Launch {
            sky: c("scene_sky", include_bytes!("../../shaders/launch/spv/scene_sky.spv"), &[ub, st, sa, sa], 0),
            world: c("scene_world", include_bytes!("../../shaders/launch/spv/scene_world.spv"), &[ub, st, sa, sa], 0),
            height: c("height", include_bytes!("../../shaders/launch/spv/height.spv"), &[ub, st], 0),
            sparks: c("sparks", include_bytes!("../../shaders/launch/spv/sparks.spv"), &[ub, st, sa], 0),
            resolve: c("resolve", include_bytes!("../../shaders/launch/spv/resolve.spv"), &[ub, st, st], 32),
            down: c("down", include_bytes!("../../shaders/launch/spv/down.spv"), &[ub, sa, sa, st], 32),
            up: c("up", include_bytes!("../../shaders/launch/spv/up.spv"), &[ub, sa, sa, st], 32),
            rays: c("rays", include_bytes!("../../shaders/launch/spv/rays.spv"), &[ub, sa, st], 32),
            comp: c("comp", include_bytes!("../../shaders/launch/spv/comp.spv"), &[ub, sa, sa, sa, sa, sa, sa, st, ub, st, st, sa, sa, sa], 0),
            ink: c("ink", include_bytes!("../../shaders/launch/spv/ink.spv"), &[ub, sa, st, ub], 0),
            fxc: c("fx", include_bytes!("../../shaders/launch/spv/fx.spv"), &[ub, st, sa, sa, sa], 0),
            text: c("text", include_bytes!("../../shaders/launch/spv/text.spv"), &[ub, sa, st, st, D::STORAGE_BUFFER], 32),
            sim: c("smoke_sim", include_bytes!("../../shaders/launch/spv/smoke_sim.spv"), &[ub, st, st, st], 0),
            splat: c("smoke_splat", include_bytes!("../../shaders/launch/spv/smoke_splat.spv"), &[ub, st, st, st], 0),
            linear: sampler(vk::Filter::LINEAR, false),
            mipped: sampler(vk::Filter::LINEAR, true),
            fixed: None,
            sized: None,
            smoke_alpha: 0.0,
            smoke_burst: -1.0,
            smoke_init: true,
            smoke_cur: 0,
            prepared: Some(Prepared::new()),
            prof: std::env::var_os("CURSED_DEV_LAUNCHPROF").map(|_| {
                let info = vk::QueryPoolCreateInfo::default().query_type(vk::QueryType::TIMESTAMP).query_count(2 * (PROF as u32 + 1));
                (fail(unsafe { gpu.device.create_query_pool(&info, None) }, "Can't make the launch's query pool"), 0)
            }),
            prof_ms: [0.0; PROF],
            terrain: None,
            cap: 115.5,
            point_was: None,
            point_speed: 0.0,
        }
    }

    /// One frame of the launch into `out` (the game's texture, RGBA8, the picture's size). `glyphs`: the menu's text,
    /// 16 numbers a glyph (shaders/launch/text.comp).
    pub fn frame(&mut self, gpu: &Gpu, enc: &mut Encoder, i: &Inputs, glyphs: &[f32], out: &Texture) {
        let cmd = enc.cmd(gpu);
        if self.fixed.is_none() {
            self.fixed = Some(self.make_fixed(gpu, enc, cmd));
        }
        let (w, h) = (out.width.max(1), out.height.max(1));
        if self.sized.as_ref().map_or(true, |s| s.w != w || s.h != h) {
            if let Some(old) = self.sized.take() {
                for img in [old.scene, old.fx, old.part, old.acc_part, old.acc_smoke, old.trail, old.rays, old.out, old.text_cov, old.text_glow, old.inked].into_iter().chain(old.mips).chain(old.blur) {
                    retire(enc, img.t, img.v);
                }
            }
            self.sized = Some(make_sized(gpu, cmd, w, h));
        }
        if i.smoke_reset {
            self.smoke_init = true;
            self.smoke_alpha = 0.0;
            self.smoke_burst = -1.0;
        }
        enc.segment(gpu, cmd, "Launch", "launch (compute)");
        if let Some((pool, n)) = self.prof.as_mut() {
            // the frame before last is done (two in flight): its times
            let half = PROF as u32 + 1;
            let base = (*n % 2) * half;
            let mut v = [0u64; PROF + 1];
            if *n >= 2 && unsafe { gpu.device.get_query_pool_results(*pool, base, &mut v, vk::QueryResultFlags::TYPE_64) }.is_ok() {
                for k in 0..PROF {
                    self.prof_ms[k] = (v[k + 1].saturating_sub(v[k])) as f32 * gpu.info.timestamp_period / 1e6;
                }
            }
            unsafe {
                gpu.device.cmd_reset_query_pool(cmd, *pool, base, half);
                gpu.sync2_d.cmd_write_timestamp2(cmd, vk::PipelineStageFlags2::ALL_COMMANDS, *pool, base);
            }
            *n += 1;
        }
        self.record(gpu, enc, cmd, i, glyphs, out);
        enc.segment_end(gpu, cmd);
    }

    fn record(&mut self, gpu: &Gpu, enc: &mut Encoder, cmd: vk::CommandBuffer, i: &Inputs, glyphs: &[f32], out: &Texture) {
        let s = self.sized.as_ref().unwrap();
        let f = self.fixed.as_ref().unwrap();
        let (pw, ph) = (s.w as f32, s.h as f32);
        // the scene's layout and this frame's numbers (timeline.rs), in ui px
        let ui = i.ui.max(1e-3);
        let lay = timeline::layout(pw / ui, ph / ui, i.left / ui, i.foot_top / ui, self.cap);
        let scene_t = if i.phase == timeline::Phase::Run { i.t } else { i.pt };
        let fr = timeline::frame(i.phase, scene_t, i.load, i.loaded_at, &lay, self.cap, i.quit, i.fx);
        // the valley's clock: its own, or stilled (the photograph, the engraving; a way out)
        let life = if fr.pane.life >= 0.0 { fr.pane.life } else { i.life };
        let cam = timeline::camera(fr.cam_m, life, s.sw as f32 / s.sh as f32, i.look, i.pose);
        let lc = timeline::light_pos(&cam);
        let menu_k = fr.menu_k;
        let k_px = s.sw as f32 / pw;
        let cursor = timeline::cursor_light(&cam, i.cursor, fr.cur_k);
        let hf = i.hover[0];
        // the point's speed (its sparks are born where it was): from its move since the last frame
        if fr.point[1] > 0.0 {
            if let Some((x, at)) = self.point_was {
                let dt = i.life - at;
                if dt > 1e-4 && dt < 0.2 {
                    let v = (fr.point[0] - x) / dt;
                    self.point_speed += (v - self.point_speed) * (dt * 6.0).min(1.0);
                }
            }
            self.point_was = Some((fr.point[0], i.life));
        } else {
            self.point_was = None;
            self.point_speed = 0.0;
        }
        let smoke_on = false;

        // the frame's numbers (shaders/launch/launch.glsl)
        let mut u = [0f32; 56 * 4];
        let mut put = |at: usize, v: [f32; 4]| u[at * 4..at * 4 + 4].copy_from_slice(&v);
        put(0, [cam.pos[0], cam.pos[1], cam.pos[2], cam.tan_h]);
        put(1, [cam.right[0], cam.right[1], cam.right[2], cam.aspect]);
        put(2, [cam.up[0], cam.up[1], cam.up[2], life]);
        put(3, [cam.fwd[0], cam.fwd[1], cam.fwd[2], -64.0]);
        // the light behind the ridges (no logo in the sky any more: its size, its axes none)
        put(4, [lc[0], lc[1], lc[2], 0.0]);
        put(8, [0.0, 0.0, 0.0, 0.0]);
        put(9, [fr.glow, i.boost, 0.0, -1.0]);
        put(10, [0.0, i.flash_c, 0.0, 0.0]);
        put(11, cursor);
        put(12, [s.sw as f32, s.sh as f32, pw, ph]);
        put(13, [0.0, if i.no_bloom { 0.0 } else { 1.0 }, 0.0, 0.72 * menu_k]);
        put(14, [if pw > ph { 0.7 * menu_k } else { 0.0 }, i.fade, i.ui, 0.0]);
        put(15, [0.0, 0.0, 0.5, 0.5]);
        put(16, [0.12, 0.0, ink::startled(), fr.pane.hold]);
        put(20, [i.hover[0][0], i.hover[0][1], i.hover[0][2], i.hover[0][3]]);
        put(21, [i.hover[1][0], i.hover[1][1], i.hover[1][2], i.hover[1][3]]);
        let fire = if i.menus_fire { 0.0 } else { 1.0 };
        put(22, [i.hover[0][4] * fire, i.hover[1][4] * fire, i.hover_sparks * hf[4] * smooth(0.1, 0.5, hf[3]), 0.0]);
        put(25, [i.dt.min(1.0 / 30.0), life, 0.0, -1.0]);
        put(26, [s.sw as f32 / s.sh as f32, 0.0, 0.0, 1.5f32.max(i.ui * k_px)]);
        for k in 0..6 {
            put(28 + k, [i.knobs[k * 4], i.knobs[k * 4 + 1], i.knobs[k * 4 + 2], i.knobs[k * 4 + 3]]);
        }
        // v12
        put(34, [fr.rise_t, fr.rise_dir, fr.lit, if fr.scene_on { 1.0 } else { 0.0 }]);
        put(35, [fr.ridge[0], fr.ridge[1], fr.ridge[2], 0.0]);
        let (vw, vh) = (lay.view[0], lay.view[1]);
        put(36, [lay.tip[0], vh - lay.tip[1], vw, vh]);
        put(37, [lay.logo, fr.logo_i, fr.stroke, fr.spread]);
        put(38, fr.hl);
        put(39, [fr.ember, s.sw as f32 / vw, 0.0, 0.0]);
        put(40, fr.rule);
        put(41, fr.rule_c);
        put(42, fr.point);
        put(43, [fr.sparks[0], fr.sparks[1], fr.sparks[2], self.point_speed]);
        let tt = fr.title;
        put(44, [tt.left, tt.clip, tt.s, fr.title_a]);
        put(45, [tt.pitch, tt.cap * 0.16, tt.cap, 12.0]);
        for (k, l) in fr.letters.iter().enumerate() {
            put(46 + k, *l);
        }
        put(52, fr.world);
        put(53, [vh - lay.tip[1], 0.0, 0.0, 0.0]);
        // the world's frame lies over the picture «by its cut»: filling it, no bars (the game's targets are kept bottom
        // row first: read upside down)
        let war = i.world.map_or(16.0 / 9.0, |w| w.1.max(0.1));
        let var = vw / vh;
        let (sx, sy) = if var > war { (1.0, war / var) } else { (var / war, 1.0) };
        put(54, [sx, -sy, (1.0 - sx) / 2.0, 1.0 - (1.0 - sy) / 2.0]);
        let world_on = i.world.is_some() && fr.world[0] > 0.0;
        put(52, if world_on { fr.world } else { [0.0; 4] });
        let piece = enc.transient.mapped(gpu, UBO, gpu.info.min_uniform_align.max(16) as u64, UBO, 1);
        unsafe { std::ptr::copy_nonoverlapping(u.as_ptr() as *const u8, piece.host, UBO as usize) };
        let mipped = self.mipped;
        let ubo = vk::DescriptorBufferInfo { buffer: unsafe { (*piece.buffer).raw }, offset: piece.offset, range: UBO };
        let mut lines = [0f32; LINES * 8 + 24];
        for (n, line) in i.lines.iter().enumerate() {
            lines[n * 8..n * 8 + 8].copy_from_slice(line);
        }
        for (n, ic) in i.icons.iter().enumerate() {
            lines[LINES * 8 + n * 4..LINES * 8 + n * 4 + 4].copy_from_slice(ic);
        }
        lines[LINES * 8 + 12..LINES * 8 + 16].copy_from_slice(&i.icon_light);
        lines[LINES * 8 + 16..LINES * 8 + 24].copy_from_slice(&i.disc);
        let menu = enc.transient.mapped(gpu, MENU_UBO, gpu.info.min_uniform_align.max(16) as u64, MENU_UBO, 1);
        unsafe { std::ptr::copy_nonoverlapping(lines.as_ptr() as *const u8, menu.host, MENU_UBO as usize) };
        let menu_ubo = vk::DescriptorBufferInfo { buffer: unsafe { (*menu.buffer).raw }, offset: menu.offset, range: MENU_UBO };

        let d = &gpu.device;
        let lin = self.linear;
        let run = |c: &Compute, binds: &[Bind], push: Option<&[u8]>, groups: [u32; 3]| {
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
            memory_barrier(gpu, cmd);
        };
        let g8 = |w: u32, h: u32| [w.div_ceil(8), h.div_ceil(8), 1];
        let push32 = |a: [i32; 4], b: [f32; 4]| -> [u8; 32] {
            let mut p = [0u8; 32];
            unsafe {
                std::ptr::copy_nonoverlapping(a.as_ptr() as *const u8, p.as_mut_ptr(), 16);
                std::ptr::copy_nonoverlapping(b.as_ptr() as *const u8, p.as_mut_ptr().add(16), 16);
            }
            p
        };
        let pushf = |a: [f32; 4], b: [i32; 4]| -> [u8; 32] {
            let mut p = [0u8; 32];
            unsafe {
                std::ptr::copy_nonoverlapping(a.as_ptr() as *const u8, p.as_mut_ptr(), 16);
                std::ptr::copy_nonoverlapping(b.as_ptr() as *const u8, p.as_mut_ptr().add(16), 16);
            }
            p
        };

        let prof = self.prof.map(|(pool, n)| (pool, ((n + 1) % 2) * (PROF as u32 + 1)));
        let mark = |k: u32| {
            if let Some((pool, base)) = prof {
                unsafe { gpu.sync2_d.cmd_write_timestamp2(cmd, vk::PipelineStageFlags2::ALL_COMMANDS, pool, base + k) };
            }
        };
        // 1) the smoke: a step of the flow, its threads into the fading trail
        if smoke_on {
            let (a, b) = (&f.smoke[self.smoke_cur], &f.smoke[self.smoke_cur ^ 1]);
            run(&self.sim, &[Bind::Ubo(ubo), Bind::Storage(a.v.raw), Bind::Storage(f.home.v.raw), Bind::Storage(b.v.raw)], None, g8(SMOKE_W, SMOKE_H));
            self.smoke_cur ^= 1;
            self.smoke_init = false;
            let now = &f.smoke[self.smoke_cur];
            run(&self.splat, &[Bind::Ubo(ubo), Bind::Storage(now.v.raw), Bind::Storage(f.home.v.raw), Bind::Storage(s.acc_smoke.v.raw)], None, [(SMOKE_W * SMOKE_H).div_ceil(64), 1, 1]);
            let fade = (-(i.dt.min(1.0 / 20.0)) * 32.0).exp();
            let p = push32([1, s.sw as i32, s.sh as i32, 0], [fade, 0.0, 0.0, 0.0]);
            run(&self.resolve, &[Bind::Ubo(ubo), Bind::Storage(s.acc_smoke.v.raw), Bind::Storage(s.trail.v.raw)], Some(&p), g8(s.sw, s.sh));
        }
        mark(1);
        // the land, made again when a new world's terrain asks for another (it glides there: menus/)
        let terrain = [i.knobs[0], i.knobs[1], i.knobs[2], i.knobs[3], i.knobs[4], i.knobs[5]];
        if self.terrain != Some(terrain) {
            self.terrain = Some(terrain);
            run(&self.height, &[Bind::Ubo(ubo), Bind::Storage(f.height.v.raw)], None, [384 / 8, 768 / 8, 1]);
        }
        // 2) the world (v12: black until the horizon is there; the shader stops at once then)
        run(&self.world, &[Bind::Ubo(ubo), Bind::Storage(s.scene.v.raw), Bind::Sampled(f.height.v.raw, lin), Bind::Sampled(f.logo.v.raw, lin)], None, g8(s.sw, s.sh));
        mark(2);
        // 3) sparks, then into a picture
        run(&self.sparks, &[Bind::Ubo(ubo), Bind::Storage(s.acc_part.v.raw), Bind::Sampled(s.scene.v.raw, lin)], None, [680u32.div_ceil(64), 1, 1]);
        let p = push32([0, s.sw as i32, s.sh as i32, 0], [0.0; 4]);
        run(&self.resolve, &[Bind::Ubo(ubo), Bind::Storage(s.acc_part.v.raw), Bind::Storage(s.part.v.raw)], Some(&p), g8(s.sw, s.sh));
        // the light over the world: the wings, the lines, the point, the letters' heat
        run(&self.fxc, &[Bind::Ubo(ubo), Bind::Storage(s.fx.v.raw), Bind::Sampled(s.scene.v.raw, lin), Bind::Sampled(f.logo.v.raw, lin), Bind::Sampled(f.title.v.raw, mipped)], None, g8(s.sw, s.sh));
        mark(3);
        // 4) the glow: down five levels, back up
        for (n, m) in s.mips.iter().enumerate() {
            let (src_v, sw, sh) = if n == 0 { (s.scene.v.raw, s.sw, s.sh) } else { (s.mips[n - 1].v.raw, s.mips[n - 1].t.width, s.mips[n - 1].t.height) };
            let p = pushf([1.0 / sw as f32, 1.0 / sh as f32, m.t.width as f32, m.t.height as f32], [if n == 0 { 1 } else { 0 }, 0, 0, 0]);
            run(&self.down, &[Bind::Ubo(ubo), Bind::Sampled(src_v, lin), Bind::Sampled(s.fx.v.raw, lin), Bind::Storage(m.v.raw)], Some(&p), g8(m.t.width, m.t.height));
        }
        for n in (1..s.mips.len()).rev() {
            let (src, dst) = (&s.mips[n], &s.mips[n - 1]);
            let p = pushf([1.0 / src.t.width as f32, 1.0 / src.t.height as f32, dst.t.width as f32, dst.t.height as f32], [0; 4]);
            run(&self.up, &[Bind::Ubo(ubo), Bind::Sampled(src.v.raw, lin), Bind::Sampled(src.v.raw, lin), Bind::Storage(dst.v.raw)], Some(&p), g8(dst.t.width, dst.t.height));
        }
        // the background's blur (the menus'): the scene itself down and back up, no glow's threshold
        let blur_k = i.knobs[18];
        if blur_k > 0.0 {
            let levels = (1 + (blur_k.clamp(0.0, 1.0) * 3.0).round() as usize).min(s.blur.len());
            for n in 0..levels {
                let (src_v, sw, sh) = if n == 0 { (s.scene.v.raw, s.sw, s.sh) } else { (s.blur[n - 1].v.raw, s.blur[n - 1].t.width, s.blur[n - 1].t.height) };
                let m = &s.blur[n];
                let p = pushf([1.0 / sw as f32, 1.0 / sh as f32, m.t.width as f32, m.t.height as f32], [2, 0, 0, 0]);
                run(&self.down, &[Bind::Ubo(ubo), Bind::Sampled(src_v, lin), Bind::Sampled(s.part.v.raw, lin), Bind::Storage(m.v.raw)], Some(&p), g8(m.t.width, m.t.height));
            }
            for n in (1..levels).rev() {
                let (src, dst) = (&s.blur[n], &s.blur[n - 1]);
                let p = pushf([1.0 / src.t.width as f32, 1.0 / src.t.height as f32, dst.t.width as f32, dst.t.height as f32], [2, 0, 0, 0]);
                run(&self.up, &[Bind::Ubo(ubo), Bind::Sampled(src.v.raw, lin), Bind::Sampled(src.v.raw, lin), Bind::Storage(dst.v.raw)], Some(&p), g8(dst.t.width, dst.t.height));
            }
        }
        mark(4);
        // 5) (v11's rays: none in v12)
        mark(5);
        // 6) the menu's text, then the picture
        let count = glyphs.len() / 16;
        if count > 0 {
            let bytes = (count * 64) as u64;
            let piece = enc.transient.mapped(gpu, bytes, 16, bytes, 1);
            unsafe { std::ptr::copy_nonoverlapping(glyphs.as_ptr() as *const u8, piece.host, bytes as usize) };
            let list = vk::DescriptorBufferInfo { buffer: unsafe { (*piece.buffer).raw }, offset: piece.offset, range: bytes };
            let p = push32([0, s.w as i32, s.h as i32, 0], [ATLAS_SPREAD, f.atlas.t.width as f32, f.atlas.t.height as f32, 0.0]);
            run(
                &self.text,
                &[Bind::Ubo(ubo), Bind::Sampled(f.atlas.v.raw, lin), Bind::Storage(s.text_cov.v.raw), Bind::Storage(s.text_glow.v.raw), Bind::Buffer(list)],
                Some(&p),
                [count as u32, 1, 1],
            );
        }
        mark(6);
        run(
            &self.comp,
            &[
                Bind::Ubo(ubo),
                Bind::Sampled(s.scene.v.raw, lin),
                Bind::Sampled(s.part.v.raw, lin),
                Bind::Sampled(s.mips[0].v.raw, lin),
                Bind::Sampled(f.title.v.raw, mipped),
                Bind::Sampled(s.rays.v.raw, lin),
                Bind::Sampled(s.trail.v.raw, lin),
                Bind::Storage(s.out.v.raw),
                Bind::Ubo(menu_ubo),
                Bind::Storage(s.text_cov.v.raw),
                Bind::Storage(s.text_glow.v.raw),
                Bind::Sampled(s.blur[0].v.raw, lin),
                Bind::Sampled(s.fx.v.raw, lin),
                Bind::Sampled(if world_on { i.world.unwrap().0 } else { s.scene.v.raw }, lin),
            ],
            None,
            g8(s.w, s.h),
        );
        // the ink over it (launch/ink.rs): see-through over the desktop until it has spread
        let pane = fr.pane.on;
        if pane {
            let block = fr.pane.block(i.fx, pw / ph);
            let bytes = (block.len() * 4) as u64;
            let piece = enc.transient.mapped(gpu, bytes, gpu.info.min_uniform_align.max(16) as u64, bytes, 1);
            unsafe { std::ptr::copy_nonoverlapping(block.as_ptr() as *const u8, piece.host, bytes as usize) };
            let iubo = vk::DescriptorBufferInfo { buffer: unsafe { (*piece.buffer).raw }, offset: piece.offset, range: bytes };
            run(&self.ink, &[Bind::Ubo(ubo), Bind::Sampled(s.out.v.raw, lin), Bind::Storage(s.inked.v.raw), Bind::Ubo(iubo)], None, g8(s.w, s.h));
        }
        mark(7);
        // into the game's texture
        let range = vk::ImageSubresourceLayers { aspect_mask: vk::ImageAspectFlags::COLOR, mip_level: 0, base_array_layer: 0, layer_count: 1 };
        let corner = vk::Offset3D { x: s.w as i32, y: s.h as i32, z: 1 };
        let to_corner = vk::Offset3D { x: out.width as i32, y: out.height as i32, z: 1 };
        let blit = vk::ImageBlit { src_subresource: range, src_offsets: [vk::Offset3D::default(), corner], dst_subresource: range, dst_offsets: [vk::Offset3D::default(), to_corner] };
        let last = if pane { s.inked.t.image } else { s.out.t.image };
        unsafe { d.cmd_blit_image(cmd, last, vk::ImageLayout::GENERAL, out.image, vk::ImageLayout::GENERAL, &[blit], vk::Filter::NEAREST) };
        memory_barrier(gpu, cmd);
        let _ = enc;
    }

    fn make_fixed(&mut self, gpu: &Gpu, enc: &mut Encoder, cmd: vk::CommandBuffer) -> Fixed {
        let img = |name: &str, format: vk::Format, w: u32, h: u32| {
            let (t, v) = image_in(gpu, cmd, name, format, w, h, 1, false);
            Image { t, v }
        };
        // the land: made with the frame's numbers (record(): a new world's terrain may ask for another)
        let height = img("Launch land", vk::Format::R8G8B8A8_UNORM, 384, 768);
        self.terrain = None;
        let _ = enc;
        // her logo (the shape and its heat)
        let pre = self.prepared.take().unwrap_or_else(Prepared::new);
        let logo_px = pre.logo;
        let logo = img("Launch logo", vk::Format::R8G8B8A8_UNORM, 192, 192);
        upload(gpu, enc, cmd, &logo.t, &logo_px);
        // the menu's text: distance fields (tools/text_atlas.py)
        let packed = pre.atlas;
        let (aw, ah) = (u32::from_le_bytes(packed[0..4].try_into().unwrap()), u32::from_le_bytes(packed[4..8].try_into().unwrap()));
        let atlas = mipped_image(gpu, cmd, "Launch text", vk::Format::R8_UNORM, aw, ah, 1);
        upload(gpu, enc, cmd, &atlas.t, &packed[8..]);
        // the title's letters (tools/title_atlas.py): CURSED, RECODE; mips for the caption's size
        let tp = pre.title;
        let (tw, th) = (u32::from_le_bytes(tp[0..4].try_into().unwrap()), u32::from_le_bytes(tp[4..8].try_into().unwrap()));
        self.cap = f32::from_le_bytes(tp[8..12].try_into().unwrap());
        let title = mipped_image(gpu, cmd, "Launch title letters", vk::Format::R8_UNORM, tw, th, 32 - tw.max(th).leading_zeros());
        upload(gpu, enc, cmd, &title.t, &tp[12..]);
        make_mips(gpu, cmd, &title.t);
        // the smoke: homes on her logo (72%, by its brightness) and round it
        let home_data = pre.homes;
        let home = img("Launch smoke homes", vk::Format::R32G32B32A32_SFLOAT, SMOKE_W, SMOKE_H);
        upload(gpu, enc, cmd, &home.t, unsafe { std::slice::from_raw_parts(home_data.as_ptr() as *const u8, home_data.len() * 4) });
        let smoke = [img("Launch smoke a", vk::Format::R32G32B32A32_SFLOAT, SMOKE_W, SMOKE_H), img("Launch smoke b", vk::Format::R32G32B32A32_SFLOAT, SMOKE_W, SMOKE_H)];
        Fixed { height, logo, atlas, title, smoke, home }
    }

    /// Lets the pictures made for a size go (the GPU is idle): between the launch and the farewell nothing is drawn,
    /// and the game should have that memory. The next frame makes them again.
    pub fn trim(&mut self, gpu: &Gpu) {
        if let Some(s) = self.sized.take() {
            for img in [s.scene, s.fx, s.part, s.acc_part, s.acc_smoke, s.trail, s.rays, s.out, s.text_cov, s.text_glow, s.inked].into_iter().chain(s.mips).chain(s.blur) {
                free_image(gpu, img);
            }
        }
    }

    pub fn destroy(mut self, gpu: &Gpu) {
        let d = &gpu.device;
        if let Some(f) = self.fixed.take() {
            let [a, b] = f.smoke;
            for img in [f.height, f.logo, f.atlas, f.title, a, b, f.home] {
                free_image(gpu, img);
            }
        }
        self.trim(gpu);
        for c in [self.sky, self.world, self.height, self.sparks, self.resolve, self.down, self.up, self.rays, self.comp, self.ink, self.fxc, self.text, self.sim, self.splat] {
            c.destroy(gpu);
        }
        unsafe {
            d.destroy_sampler(self.linear, None);
            d.destroy_sampler(self.mipped, None);
            if let Some((pool, _)) = self.prof {
                d.destroy_query_pool(pool, None);
            }
        }
    }
}

pub(crate) fn free_image(gpu: &Gpu, img: Image) {
    unsafe {
        gpu.device.destroy_image_view(img.v.raw, None);
        gpu.device.destroy_image(img.t.image, None);
    }
    let mut t = img.t;
    if let Some(a) = t.alloc.take() {
        gpu.memory.free(&gpu.device, a);
    }
}

enum Bind {
    Ubo(vk::DescriptorBufferInfo),
    Buffer(vk::DescriptorBufferInfo),
    Storage(vk::ImageView),
    Sampled(vk::ImageView, vk::Sampler),
}

/// The scene at 0.8 of the picture, no more than 1.6 million pixels (as the preview).
fn make_sized(gpu: &Gpu, cmd: vk::CommandBuffer, w: u32, h: u32) -> Sized {
    let cap = (1.6e6 / (w as f32 * h as f32)).sqrt().min(1.0);
    let sw = ((w as f32 * 0.8 * cap).round() as u32).max(64);
    let sh = ((h as f32 * 0.8 * cap).round() as u32).max(36);
    let img = |name: &str, format: vk::Format, w: u32, h: u32| {
        let (t, v) = image_in(gpu, cmd, name, format, w, h, 1, false);
        Image { t, v }
    };
    let f16 = vk::Format::R16G16B16A16_SFLOAT;
    let mut mips = Vec::new();
    let mut blur = Vec::new();
    let (mut mw, mut mh) = (sw, sh);
    for n in 0..5 {
        mw = (mw >> 1).max(1);
        mh = (mh >> 1).max(1);
        mips.push(img(&format!("Launch glow {n}"), f16, mw, mh));
        if n < 4 {
            blur.push(img(&format!("Launch blur {n}"), f16, mw, mh));
        }
    }
    Sized {
        w,
        h,
        sw,
        sh,
        scene: img("Launch scene", f16, sw, sh),
        fx: img("Launch light", f16, sw, sh),
        part: img("Launch sparks", f16, sw, sh),
        acc_part: img("Launch sparks sum", vk::Format::R32_UINT, sw, sh),
        acc_smoke: img("Launch smoke sum", vk::Format::R32_UINT, sw, sh),
        trail: img("Launch smoke trail", f16, sw, sh),
        mips,
        rays: img("Launch rays", f16, (sw >> 2).max(1), (sh >> 2).max(1)),
        out: img("Launch picture", vk::Format::R8G8B8A8_UNORM, w, h),
        text_cov: img("Launch text", vk::Format::R32_UINT, w, h),
        text_glow: img("Launch text glow", vk::Format::R32_UINT, w, h),
        inked: img("Launch ink", vk::Format::R8G8B8A8_UNORM, w, h),
        blur,
    }
}

/// An image with mips (sampled, copied to and from), laid out GENERAL.
pub(crate) fn mipped_image(gpu: &Gpu, cmd: vk::CommandBuffer, name: &str, format: vk::Format, w: u32, h: u32, mips: u32) -> Image {
    let d = &gpu.device;
    let info = vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .format(format)
        .extent(vk::Extent3D { width: w, height: h, depth: 1 })
        .mip_levels(mips)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::OPTIMAL)
        .usage(vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::TRANSFER_SRC)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED);
    let raw = fail(unsafe { d.create_image(&info, None) }, "Can't make a launch image");
    let req = unsafe { d.get_image_memory_requirements(raw) };
    let alloc = fail(gpu.memory.allocate(d, req, Place::Device, false), "Can't allocate a launch image");
    fail(unsafe { d.bind_image_memory(raw, alloc.memory, alloc.offset) }, "Can't bind a launch image");
    gpu.name(raw, name);
    let t = Box::new(Texture {
        image: raw,
        alloc: Some(alloc),
        format,
        aspect: vk::ImageAspectFlags::COLOR,
        width: w,
        height: h,
        layers: 1,
        mips,
        cube: false,
        closed: AtomicBool::new(true),
        views: AtomicU32::new(1),
    });
    let range = vk::ImageSubresourceRange { aspect_mask: vk::ImageAspectFlags::COLOR, base_mip_level: 0, level_count: mips, base_array_layer: 0, layer_count: 1 };
    let vinfo = vk::ImageViewCreateInfo::default().image(raw).view_type(vk::ImageViewType::TYPE_2D).format(format).subresource_range(range);
    let view_raw = fail(unsafe { d.create_image_view(&vinfo, None) }, "Can't make a launch image view");
    let v = Box::new(View { raw: view_raw, texture: &*t as *const Texture, base_mip: 0 });
    let barrier = [vk::ImageMemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
        .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
        .dst_access_mask(vk::AccessFlags2::MEMORY_READ | vk::AccessFlags2::MEMORY_WRITE)
        .old_layout(vk::ImageLayout::UNDEFINED)
        .new_layout(vk::ImageLayout::GENERAL)
        .image(raw)
        .subresource_range(range)];
    unsafe { gpu.sync2_d.cmd_pipeline_barrier2(cmd, &vk::DependencyInfo::default().image_memory_barriers(&barrier)) };
    Image { t, v }
}

/// Pixels into mip 0 of `t` (tightly packed, its format) through staging memory.
pub(crate) fn upload(gpu: &Gpu, enc: &mut Encoder, cmd: vk::CommandBuffer, t: &Texture, px: &[u8]) {
    let n = px.len() as u64;
    let piece = enc.transient.staging(gpu, n, 16, n, 1);
    unsafe { std::ptr::copy_nonoverlapping(px.as_ptr(), piece.host, px.len()) };
    let region = vk::BufferImageCopy {
        buffer_offset: piece.offset,
        buffer_row_length: 0,
        buffer_image_height: 0,
        image_subresource: vk::ImageSubresourceLayers { aspect_mask: vk::ImageAspectFlags::COLOR, mip_level: 0, base_array_layer: 0, layer_count: 1 },
        image_offset: vk::Offset3D::default(),
        image_extent: vk::Extent3D { width: t.width, height: t.height, depth: 1 },
    };
    unsafe { gpu.device.cmd_copy_buffer_to_image(cmd, (*piece.buffer).raw, t.image, vk::ImageLayout::GENERAL, &[region]) };
    memory_barrier(gpu, cmd);
}

/// Each mip from the one above, halved with a linear filter.
pub(crate) fn make_mips(gpu: &Gpu, cmd: vk::CommandBuffer, t: &Texture) {
    let (mut w, mut h) = (t.width as i32, t.height as i32);
    for m in 1..t.mips {
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let layers = |mip: u32| vk::ImageSubresourceLayers { aspect_mask: vk::ImageAspectFlags::COLOR, mip_level: mip, base_array_layer: 0, layer_count: 1 };
        let blit = vk::ImageBlit {
            src_subresource: layers(m - 1),
            src_offsets: [vk::Offset3D::default(), vk::Offset3D { x: w, y: h, z: 1 }],
            dst_subresource: layers(m),
            dst_offsets: [vk::Offset3D::default(), vk::Offset3D { x: nw, y: nh, z: 1 }],
        };
        unsafe { gpu.device.cmd_blit_image(cmd, t.image, vk::ImageLayout::GENERAL, t.image, vk::ImageLayout::GENERAL, &[blit], vk::Filter::LINEAR) };
        memory_barrier(gpu, cmd);
        w = nw;
        h = nh;
    }
}

/// The smoke's homes (x, y, kind, seed per thread): 72% on her logo, where it is bright (the preview's buildHomes),
/// the rest scattered round it.
fn homes(base: &[u8]) -> Vec<f32> {
    let (w, h) = (192usize, 192usize);
    let mut cdf = vec![0f32; w * h];
    let mut sum = 0f32;
    for (n, &g) in base.iter().enumerate() {
        let b = g as f32 / 255.0;
        sum += if b > 0.04 { b.powf(1.3) } else { 0.0 };
        cdf[n] = sum;
    }
    let mut seed: u32 = 4242;
    let mut rnd = move || {
        seed = seed.wrapping_add(0x6D2B_79F5);
        let mut t = (seed ^ (seed >> 15)).wrapping_mul(1 | seed);
        t = t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t)) ^ t;
        ((t ^ (t >> 14)) as f64 / 4_294_967_296.0) as f32
    };
    let n = (SMOKE_W * SMOKE_H) as usize;
    let mut data = vec![0f32; n * 4];
    for i in 0..n {
        let (hx, hy, kind);
        if rnd() < 0.72 {
            let r = rnd() * sum;
            let at = cdf.partition_point(|&c| c < r).min(w * h - 1);
            let u = ((at % w) as f32 + rnd()) / w as f32;
            let v = ((at / w) as f32 + rnd()) / h as f32;
            hx = (u - 0.509) * 1.12;
            hy = (0.866 - v) * 1.12;
            kind = 1.0;
        } else {
            let a = rnd() * std::f32::consts::TAU;
            let rr = 0.15 + 1.25 * rnd().sqrt();
            hx = a.cos() * rr * 1.6;
            hy = a.sin() * rr;
            kind = 0.0;
        }
        data[i * 4..i * 4 + 4].copy_from_slice(&[hx, hy, kind, rnd()]);
    }
    data
}
