//! World Effects (the menu's module "world-effects"): the weather of the paid CS2 client fatality.win one to one (her
//! word, 23.10) — Dota 2's own weather particles ("Weather: Snow", "Weather: Ash"; particles/rain_fx), which that client
//! carries and lets the game run. Here they run as Source 2 runs them (sim.rs, the functions read from their own files,
//! def.rs) and are drawn by the effects' pass with their own textures (engine/fx.rs, kind PART; assets/weather, made by
//! tools/weather_pack.py).
//!
//! The systems' control point 0 is the camera raised by the card's height (her word, 23.10: higher than where the game
//! has it), facing the way it looks (level): the snow falls in a disc ahead of it, its closest flakes round it, the
//! drift on the ground. With the card's own colour each particle is that colour as bright as it was (fx.comp). The game's units are kept (a block is 40 of them, a player 72 as
//! in Source), z up, counted from a point near the camera that moves along with it.

mod def;
mod sim;

use crate::engine::fx::{Shape, PART, PART_ADD, PART_FEATHER, PART_REMAP, PART_SATURATE, PART_TRAIL};
use def::{Def, Look, Render, V3};
use serde_json::Value;
use sim::{add, cross, dot, len, mul, norm, sub, Cp, Sys, CPS, P};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// The game's units a block.
pub const UNITS: f32 = 40.0;
const SNOW: &str = "particles/rain_fx/econ_snow.vpcf";
const ASH: &str = "particles/rain_fx/econ_weather_ash.vpcf";

/// A texture's sequence: clamped at its end or looping, its whole time, its frames (how long each, its number).
pub struct Seq {
    pub clamp: bool,
    pub total: f32,
    pub frames: Vec<(f32, u32)>,
}

/// The weather's textures in one picture: its size, its levels (each half the one before, RGBA), each frame's place
/// in it (u0, v0, u1, v1), each texture's sequences.
pub struct Atlas {
    pub w: u32,
    pub h: u32,
    pub levels: Vec<Vec<u8>>,
    pub rects: Vec<[f32; 4]>,
    sheets: HashMap<String, Vec<Seq>>,
}

struct Data {
    defs: Vec<Def>,
    index: HashMap<String, usize>,
    atlas: Atlas,
}

static DATA: OnceLock<Option<Data>> = OnceLock::new();
static LOADING: AtomicBool = AtomicBool::new(false);

fn log(text: &str) {
    if let Ok(mut m) = crate::vk::device::MESSAGES.lock() {
        m.push(format!("I World Effects: {text}"));
    }
}

fn load() -> Result<Data, String> {
    let systems: Value = serde_json::from_slice(include_bytes!("../../assets/weather/systems.json")).map_err(|e| e.to_string())?;
    let sheet: Value = serde_json::from_slice(include_bytes!("../../assets/weather/atlas.json")).map_err(|e| e.to_string())?;
    let rgba = miniz_oxide::inflate::decompress_to_vec_zlib(include_bytes!("../../assets/weather/atlas.rgba.z")).map_err(|e| format!("{e:?}"))?;
    let size = sheet.get("size").and_then(Value::as_array).ok_or("no size")?;
    let (w, h) = (size[0].as_u64().unwrap_or(0) as u32, size[1].as_u64().unwrap_or(0) as u32);
    if rgba.len() != (w * h * 4) as usize {
        return Err(format!("the atlas is {} bytes, not {w}x{h}", rgba.len()));
    }
    let mut defs = Vec::new();
    let mut index = HashMap::new();
    for (name, o) in systems.as_object().ok_or("no systems")? {
        index.insert(name.clone(), defs.len());
        defs.push(Def::read(name, o));
    }
    let frames = sheet.get("frames").and_then(Value::as_u64).unwrap_or(0) as usize;
    let mut rects = vec![[0.0f32; 4]; frames.max(1)];
    let mut sheets = HashMap::new();
    for (tex, seqs) in sheet.get("textures").and_then(Value::as_object).ok_or("no textures")? {
        let mut out = Vec::new();
        for s in seqs.as_array().into_iter().flatten() {
            let mut fs = Vec::new();
            for f in s.get("frames").and_then(Value::as_array).into_iter().flatten() {
                let i = f.get("i").and_then(Value::as_u64).unwrap_or(0) as u32;
                if let Some(r) = f.get("r").and_then(Value::as_array).filter(|r| r.len() == 4) {
                    if let Some(slot) = rects.get_mut(i as usize) {
                        *slot = std::array::from_fn(|k| r[k].as_f64().unwrap_or(0.0) as f32);
                    }
                }
                fs.push((f.get("t").and_then(Value::as_f64).unwrap_or(1.0) as f32, i));
            }
            out.push(Seq { clamp: s.get("clamp").and_then(Value::as_bool).unwrap_or(false), total: s.get("total").and_then(Value::as_f64).unwrap_or(1.0) as f32, frames: fs });
        }
        sheets.insert(tex.clone(), out);
    }
    Ok(Data { defs, index, atlas: Atlas { w, h, levels: levels(rgba, w, h, 4), rects, sheets } })
}

/// The picture and its smaller levels (each pixel the four under it, weighed by how much of each is there).
fn levels(top: Vec<u8>, w: u32, h: u32, count: usize) -> Vec<Vec<u8>> {
    let mut out = vec![top];
    let (mut lw, mut lh) = (w as usize, h as usize);
    while out.len() < count && lw > 1 && lh > 1 {
        let src = out.last().unwrap();
        let (nw, nh) = (lw / 2, lh / 2);
        let mut dst = vec![0u8; nw * nh * 4];
        for y in 0..nh {
            for x in 0..nw {
                let mut sum = [0f32; 4];
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let o = ((y * 2 + dy) * lw + x * 2 + dx) * 4;
                    let a = src[o + 3] as f32;
                    for c in 0..3 {
                        sum[c] += src[o + c] as f32 * a;
                    }
                    sum[3] += a;
                }
                let o = (y * nw + x) * 4;
                if sum[3] > 0.0 {
                    for c in 0..3 {
                        dst[o + c] = (sum[c] / sum[3]).round().clamp(0.0, 255.0) as u8;
                    }
                }
                dst[o + 3] = (sum[3] / 4.0).round() as u8;
            }
        }
        out.push(dst);
        lw = nw;
        lh = nh;
    }
    out
}

/// The weather's data once it is read (on another thread, the first time it is wanted; none until then).
fn data() -> Option<&'static Data> {
    if let Some(d) = DATA.get() {
        return d.as_ref();
    }
    if !LOADING.swap(true, Ordering::AcqRel) {
        std::thread::spawn(|| {
            let t = Instant::now();
            let d = load();
            match &d {
                Ok(d) => log(&format!("{} systems, {} frames read in {} ms", d.defs.len(), d.atlas.rects.len(), t.elapsed().as_millis())),
                Err(e) => log(&format!("the weather's files could not be read: {e}")),
            }
            let _ = DATA.set(d.ok());
        });
    }
    None
}

/// The atlas, when read (engine/fx.rs puts it on the GPU).
pub fn atlas() -> Option<&'static Atlas> {
    data().map(|d| &d.atlas)
}

struct State {
    which: &'static str,
    root: Option<Sys>,
    cps: [Cp; CPS],
    /// The point the simulation counts from (the game's units, in the world).
    origin: [f64; 3],
    camera: Option<[f64; 3]>,
    last: Option<Instant>,
    look: V3,
    set_at: Option<Instant>,
    on: bool,
    /// How high over the eyes (blocks), and the colour asked for (linear; none: their own).
    height: f32,
    seed: u32,
    /// The ground under columns asked lately: (x, z, the block it was looked down from) → the height of its top.
    ground: HashMap<(i32, i32, i32), Option<f32>>,
    ground_at: Option<Instant>,
    ground_epoch: u64,
    logged_at: Option<Instant>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);
static RECOLOUR: Mutex<Option<[f32; 3]>> = Mutex::new(None);

/// The colour the weather's particles are painted (linear), when the card asks for its own (engine/fx.rs).
pub fn recolour() -> Option<[f32; 3]> {
    *RECOLOUR.lock().unwrap_or_else(|e| e.into_inner())
}

/// The world (the game's x, y, z) as the game's units: east, north, up.
fn source(p: [f64; 3]) -> [f64; 3] {
    [p[0] * UNITS as f64, -p[2] * UNITS as f64, p[1] * UNITS as f64]
}

/// A way in the game's units as the world's (blocks).
fn world(v: V3) -> [f32; 3] {
    [v[0] / UNITS, v[2] / UNITS, -v[1] / UNITS]
}

/// What a block is to the ground (engine/voxels.rs): solid, and the height of its top in it.
fn top_of(class: u16) -> Option<f32> {
    use crate::engine::voxels::*;
    match class {
        FULL | LEAVES | GLASS | PARTIAL | SHINY | WATER | SLAB_TOP => Some(1.0),
        SLAB_LOW => Some(0.5),
        _ => None,
    }
}

fn voxel(x: i32, y: i32, z: i32) -> Option<u16> {
    crate::engine::voxels::read([x >> 4, y >> 4, z >> 4], |s| s.map(|s| s.voxel(((x & 15) + (z & 15) * 16 + (y & 15) * 256) as usize)))
}

/// The ground under a block column from `y` down, `depth` blocks at most: its top (blocks); a column starting in a block
/// looks up out of it first.
fn ground_under(x: i32, mut y: i32, z: i32, depth: i32) -> Option<f32> {
    for _ in 0..16 {
        match voxel(x, y, z).map(|v| top_of(v & 15)) {
            Some(Some(_)) => y += 1,
            _ => break,
        }
    }
    for k in 0..depth.max(1) {
        let v = voxel(x, y - k, z)?;
        if let Some(top) = top_of(v & 15) {
            return Some((y - k) as f32 + top);
        }
    }
    None
}

/// Each frame of the effects' pass: the weather stepped on and its particles, as the pass's shapes from the camera
/// (`camera` in the world, `view_proj` from it), lit by the sun's and the moon's strength.
pub fn frame(camera: [f64; 3], view_proj: &[f32; 16], sun: f32, moon: f32, out: &mut Vec<Shape>) {
    let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let st = guard.get_or_insert_with(|| State {
        which: SNOW,
        root: None,
        cps: [Cp::default(); CPS],
        origin: [0.0; 3],
        camera: None,
        last: None,
        look: [1.0, 0.0, 0.0],
        set_at: None,
        on: false,
        height: 5.0,
        seed: 1,
        ground: HashMap::new(),
        ground_at: None,
        ground_epoch: 0,
        logged_at: None,
    });
    let now = Instant::now();
    if st.set_at.is_none_or(|t| now.duration_since(t).as_secs_f32() > 0.25) {
        st.set_at = Some(now);
        let line = crate::gui::call("set\tworld-effects");
        let mut f = line.split('\t');
        st.on = f.next() == Some("1");
        let (mut which, mut own, mut colour) = (SNOW, false, None);
        for (k, v) in f.filter_map(|kv| kv.split_once('=')) {
            match k {
                "Atmosphere" => which = if v == "Ash and embers" { ASH } else { SNOW },
                "Height" => st.height = v.parse::<f32>().unwrap_or(st.height).clamp(0.0, 32.0),
                "Own colour" => own = v == "1",
                "Colour" => colour = u32::from_str_radix(v, 16).ok().map(|c| [(c >> 16) & 255, (c >> 8) & 255, c & 255].map(|x| linear(x as f32 / 255.0))),
                _ => {}
            }
        }
        *RECOLOUR.lock().unwrap_or_else(|e| e.into_inner()) = if own { colour.or(Some([0.48, 0.72, 1.0])) } else { None };
        if which != st.which {
            st.which = which;
            st.root = None;
        }
    }
    let dt = st.last.map_or(0.0, |t| now.duration_since(t).as_secs_f32()).min(0.1);
    st.last = Some(now);
    if !st.on {
        st.root = None;
        return;
    }
    let Some(data) = data() else { return };
    // the camera in the game's units; far from where it was (another place, another world): it starts anew
    let cam = source(camera);
    if st.camera.is_some_and(|c| (0..3).map(|i| (c[i] - camera[i]).powi(2)).sum::<f64>() > 64.0 * 64.0) {
        st.root = None;
    }
    st.camera = Some(camera);
    if st.root.is_none() {
        let Some(&i) = data.index.get(st.which) else { return };
        st.seed = st.seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        st.root = Some(Sys::new(i, &data.defs, &|n| data.index.get(n).copied(), st.seed));
        st.origin = cam;
        st.cps = [Cp::default(); CPS];
    }
    // the origin kept near the camera (the positions stay small)
    let rel = [(cam[0] - st.origin[0]) as f32, (cam[1] - st.origin[1]) as f32, (cam[2] - st.origin[2]) as f32];
    if len(rel) > 4096.0 {
        st.root.as_mut().unwrap().shift(rel);
        for c in st.cps.iter_mut() {
            c.pos = sub(c.pos, rel);
        }
        st.origin = cam;
    }
    let eye = [(cam[0] - st.origin[0]) as f32, (cam[1] - st.origin[1]) as f32, (cam[2] - st.origin[2]) as f32];
    // the camera's frame from its matrix: right, up, the way it looks (the world's, blocks)
    let inv = crate::engine::gpu::invert(view_proj);
    let un = |x: f32, y: f32| -> [f32; 3] {
        let p = [
            inv[0] * x + inv[4] * y + inv[8] * 0.5 + inv[12],
            inv[1] * x + inv[5] * y + inv[9] * 0.5 + inv[13],
            inv[2] * x + inv[6] * y + inv[10] * 0.5 + inv[14],
            inv[3] * x + inv[7] * y + inv[11] * 0.5 + inv[15],
        ];
        [p[0] / p[3], p[1] / p[3], p[2] / p[3]]
    };
    let (mid, r, u) = (un(0.0, 0.0), un(0.5, 0.0), un(0.0, 0.5));
    let right = norm(sub(r, mid));
    let up = norm(sub(mid, u));
    let fwd = norm(mid);
    // control point 0: the camera, facing the way it looks along the ground (the game's units)
    let level = [fwd[0], -fwd[2], 0.0];
    if len(level) > 0.05 {
        st.look = norm(level);
    }
    st.cps[0] = Cp { pos: add(eye, [0.0, 0.0, st.height * UNITS]), fwd: Some(st.look) };
    // the ground's heights asked lately, forgotten each second and when the world's blocks change
    let epoch = crate::engine::voxels::epoch();
    if st.ground_at.is_none_or(|t| now.duration_since(t).as_secs_f32() > 1.0) || epoch != st.ground_epoch {
        st.ground.clear();
        st.ground_at = Some(now);
        st.ground_epoch = epoch;
    }
    let origin = st.origin;
    let cache = &mut st.ground;
    let mut ground = |p: V3, max: f32| -> Option<f32> {
        let (x, y, z) = (origin[0] + p[0] as f64, origin[1] + p[1] as f64, origin[2] + p[2] as f64);
        let (bx, by, bz) = ((x / UNITS as f64).floor() as i32, (z / UNITS as f64).floor() as i32, (-y / UNITS as f64).floor() as i32);
        let depth = (max / UNITS).ceil() as i32;
        let top = *cache.entry((bx, bz, by)).or_insert_with(|| ground_under(bx, by, bz, depth.min(256)));
        top.map(|t| (t as f64 * UNITS as f64 - origin[2]) as f32)
    };
    let root = st.root.as_mut().unwrap();
    if dt > 0.0 {
        root.step(dt, &data.defs, &mut st.cps, &mut ground, None);
    }
    // the light here: the sky's, as much of it as reaches the camera, by day or by the moon's light
    let b = [camera[0].floor() as i32, camera[1].floor() as i32, camera[2].floor() as i32];
    let sky = voxel(b[0], b[1], b[2]).map_or(15.0, |v| ((v >> 4) & 15) as f32) / 15.0;
    let day = sun.clamp(0.0, 1.0);
    let night = [0.32, 0.36, 0.5];
    let open = 0.25 + 0.75 * sky;
    let tint: V3 = std::array::from_fn(|i| (night[i] + (1.0 - night[i]) * day).max(moon.clamp(0.0, 1.0) * 0.35) * open);
    let cam = Cam { eye, right, up, fwd };
    let before = out.len();
    draw(root, data, &cam, tint, out);
    // dev: how many there are and how many are drawn, now and then (CURSED_DEV_WEATHER)
    if *DEV.get_or_init(|| std::env::var_os("CURSED_DEV_WEATHER").is_some()) && st.logged_at.is_none_or(|t| now.duration_since(t).as_secs_f32() > 2.0) {
        st.logged_at = Some(now);
        log(&format!("{}: {} particles, {} drawn", data.defs[root.def].name, root.count(), out.len() - before));
        fn each(s: &Sys, data: &Data, eye: V3) {
            if !s.parts.is_empty() {
                let n = s.parts.len() as f32;
                let up = s.parts.iter().map(|p| (p.pos[2] - eye[2]) / UNITS).sum::<f32>() / n;
                let far = s.parts.iter().map(|p| len(sub(p.pos, eye)) / UNITS).sum::<f32>() / n;
                let alpha = s.parts.iter().map(|p| p.alpha).sum::<f32>() / n;
                let radius = s.parts.iter().map(|p| p.radius / UNITS).sum::<f32>() / n;
                log(&format!("  {}: {} at {up:.1} blocks up, {far:.1} away, alpha {alpha:.2}, radius {radius:.2}", data.defs[s.def].name, s.parts.len()));
            }
            s.children.iter().for_each(|c| each(c, data, eye));
        }
        each(root, data, eye);
    }
}

static DEV: OnceLock<bool> = OnceLock::new();

struct Cam {
    eye: V3,
    right: [f32; 3],
    up: [f32; 3],
    fwd: [f32; 3],
}

/// The frame a particle shows now: its sequence's frame by its age, the next, how far between (none: not blended).
fn frame_of(seqs: &[Seq], p: &P, rate: f32, blend: bool) -> (u32, u32, f32) {
    if seqs.is_empty() {
        return (0, 0, 0.0);
    }
    let s = &seqs[(p.seq.max(0) as usize) % seqs.len()];
    let n = s.frames.len();
    if n == 0 {
        return (0, 0, 0.0);
    }
    if n < 2 {
        return (s.frames[0].1, s.frames[0].1, 0.0);
    }
    let total = if s.total > 0.0 { s.total } else { 1.0 };
    let passes = p.age * rate;
    let at = total * if s.clamp { passes.clamp(0.0, 1.0) } else { passes - passes.floor() };
    let mut from = 0.0;
    for k in 0..n - 1 {
        let t = s.frames[k].0;
        if from + t > at {
            let b = if blend && t > 0.0 { (at - from) / t } else { 0.0 };
            return (s.frames[k].1, s.frames[k + 1].1, b);
        }
        from += t;
    }
    let last = s.frames[n - 1];
    if s.clamp {
        (last.1, last.1, 0.0)
    } else {
        let span = total - from;
        (last.1, s.frames[0].1, if blend && span > 0.0 { (at - from) / span } else { 0.0 })
    }
}

fn linear(c: f32) -> f32 {
    c.max(0.0).powf(2.2)
}

/// The flags and the brightness a renderer's particles share.
fn flags_of(look: &Look) -> u32 {
    let mut f = 0;
    // added rather than laid over: its own blend, or added to itself in full (embers, fire)
    if look.additive || look.add_self >= 1.0 {
        f |= PART_ADD;
    }
    if look.feather {
        f |= PART_FEATHER;
    }
    if look.saturate {
        f |= PART_SATURATE;
    }
    if look.remap.is_some() {
        f |= PART_REMAP;
    }
    f
}

fn colour_of(look: &Look, p: &P, tint: V3) -> [f32; 3] {
    let k = look.overbright * look.factor;
    let c = if look.gamma { p.color.map(linear) } else { p.color };
    std::array::from_fn(|i| c[i] * k * if look.light { tint[i] } else { 1.0 })
}

/// A way turned round an axis (unit) by an angle.
fn rotate(v: [f32; 3], axis: [f32; 3], a: f32) -> [f32; 3] {
    let (s, c) = a.sin_cos();
    add(add(mul(v, c), mul(cross(axis, v), s)), mul(axis, dot(axis, v) * (1.0 - c)))
}

fn draw(sys: &Sys, data: &Data, cam: &Cam, tint: V3, out: &mut Vec<Shape>) {
    let d = &data.defs[sys.def];
    for r in &d.renders {
        match r {
            Render::Sprite { look, flat, min_size, max_size } => {
                let Some(seqs) = data.atlas.sheets.get(&look.texture) else { continue };
                let flags = flags_of(look);
                let (zero, one) = look.remap.unwrap_or((0.0, 1.0));
                for p in &sys.parts {
                    let rel = sub(p.pos, cam.eye);
                    let c = world(rel);
                    let dist = len(rel);
                    let mut radius = p.radius * look.radius_scale;
                    if p.radius > 0.0 {
                        radius = radius.max(min_size * dist).min(max_size * dist);
                    }
                    let alpha = p.alpha * look.alpha_scale;
                    if radius <= 0.0 || alpha < 1.0 / 255.0 {
                        continue;
                    }
                    let half = radius / UNITS;
                    // behind the camera: nothing of it seen
                    if dot(c, cam.fwd) < -half * 1.5 {
                        continue;
                    }
                    let (r0, u0) = if *flat { ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0]) } else { (cam.right, cam.up) };
                    let (s, co) = p.roll.sin_cos();
                    let mut right = add(mul(r0, co), mul(u0, s));
                    let up = sub(mul(u0, co), mul(r0, s));
                    if p.yaw != 0.0 {
                        right = rotate(right, norm(up), p.yaw);
                    }
                    let (f0, f1, blend) = frame_of(seqs, p, look.rate, look.blend_frames);
                    let o = add(sub(c, mul(right, half)), mul(up, half));
                    let rgb = colour_of(look, p, tint);
                    out.push(Shape {
                        v: [
                            [o[0], o[1], o[2], f0 as f32],
                            [right[0] * 2.0 * half, right[1] * 2.0 * half, right[2] * 2.0 * half, f1 as f32],
                            [-up[0] * 2.0 * half, -up[1] * 2.0 * half, -up[2] * 2.0 * half, blend],
                            [PART as f32, flags as f32, zero, one],
                        ],
                        colour: [rgb[0], rgb[1], rgb[2], alpha],
                    });
                }
            }
            Render::Trail { look, max_len, min_len, len_scale, ratio, tail_colour, tail_alpha, len_fade_in } => {
                let Some(seqs) = data.atlas.sheets.get(&look.texture) else { continue };
                let flags = (flags_of(look) & !PART_REMAP) | PART_TRAIL;
                let tail = {
                    let t = tail_colour.map(|x| (x.clamp(0.0, 1.0) * 255.0).round() as u32);
                    ((t[0] << 16 | t[1] << 8 | t[2]) + 1) as f32
                };
                for p in &sys.parts {
                    let moved = sub(p.prev, p.pos);
                    if len(moved) < 1e-6 || sys.dt <= 0.0 {
                        continue;
                    }
                    let dir = norm(moved);
                    let mut length = len_scale * p.trail * len(moved) / sys.dt;
                    length *= (p.age / len_fade_in.max(1e-9)).min(1.0);
                    length = length.clamp(*min_len, *max_len);
                    if length <= 0.0 {
                        continue;
                    }
                    let radius = (p.radius * look.radius_scale).min(ratio * length);
                    let alpha = p.alpha * p.alpha2 * look.alpha_scale;
                    if radius <= 0.0 || alpha < 1.0 / 255.0 {
                        continue;
                    }
                    let head = p.pos;
                    let rel = sub(head, cam.eye);
                    let c = world(rel);
                    if dot(c, cam.fwd) < -(length + radius) / UNITS {
                        continue;
                    }
                    let along = world(mul(dir, length));
                    let across = norm(cross(world(dir), world(rel)));
                    let across = if len(across) < 0.5 { norm(cross(world(dir), [0.0, 1.0, 0.0])) } else { across };
                    let w = radius / UNITS;
                    let o = sub(c, mul(across, w));
                    let (f0, f1, blend) = frame_of(seqs, p, look.rate, look.blend_frames);
                    let rgb = colour_of(look, p, tint);
                    out.push(Shape {
                        v: [[o[0], o[1], o[2], f0 as f32], [across[0] * 2.0 * w, across[1] * 2.0 * w, across[2] * 2.0 * w, f1 as f32], [along[0], along[1], along[2], blend], [PART as f32, flags as f32, tail, *tail_alpha]],
                        colour: [rgb[0], rgb[1], rgb[2], alpha.min(1.0)],
                    });
                }
            }
        }
    }
    for c in &sys.children {
        draw(c, data, cam, tint, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loaded() -> Data {
        load().expect("the weather's files read")
    }

    #[test]
    fn the_files_read_and_every_texture_is_there() {
        let d = loaded();
        assert!(d.index.contains_key(SNOW) && d.index.contains_key(ASH));
        for def in &d.defs {
            for r in &def.renders {
                let (Render::Sprite { look, .. } | Render::Trail { look, .. }) = r;
                assert!(d.atlas.sheets.contains_key(&look.texture), "{}: {}", def.name, look.texture);
            }
        }
        assert_eq!(d.atlas.levels.len(), 4);
    }

    /// The snow and the ash run: their particles come out, stay round the camera's disc ahead, none of them lost to
    /// numbers going wrong.
    #[test]
    fn snow_and_ash_fill_the_air_round_the_camera() {
        let d = loaded();
        for (name, least) in [(SNOW, 1500), (ASH, 300)] {
            let mut cps = [Cp::default(); CPS];
            let mut sys = Sys::new(d.index[name], &d.defs, &|n| d.index.get(n).copied(), 7);
            let mut ground = |p: V3, _max: f32| Some(p[2].min(0.0) - 64.0);
            for _ in 0..240 {
                cps[0] = Cp { pos: [0.0, 0.0, 64.0], fwd: Some([1.0, 0.0, 0.0]) };
                sys.step(1.0 / 60.0, &d.defs, &mut cps, &mut ground, None);
            }
            let n = sys.count();
            assert!(n >= least, "{name}: {n} particles");
            // the snow's centre is ahead of the camera, 500 units along the way it looks
            assert!((cps[16].pos[0] - 500.0).abs() < 1e-3, "{:?}", cps[16].pos);
            fn check(s: &Sys) {
                for p in &s.parts {
                    assert!(p.pos.iter().all(|v| v.is_finite()) && p.radius.is_finite() && p.alpha.is_finite());
                    assert!(len(sub(p.pos, [500.0, 0.0, 64.0])) < 4000.0, "{:?}", p.pos);
                }
                s.children.iter().for_each(check);
            }
            check(&sys);
        }
    }

    #[test]
    fn levels_halve() {
        let l = levels(vec![255; 8 * 8 * 4], 8, 8, 4);
        assert_eq!(l.iter().map(Vec::len).collect::<Vec<_>>(), vec![256, 64, 16, 4]);
    }
}
