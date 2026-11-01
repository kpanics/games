//! The world as voxels, for the engine's rays (shadows, light, reflections): every block the mesher sees becomes 16
//! bits — what it is to a ray (its class), how much sky reaches it, and its material (a colour and a glow, from a
//! palette of 256). The game's compile threads hand sections in as they mesh them (lib.rs world_mesh); the frame
//! thread takes what changed and copies it to the GPU (engine/gpu.rs).
//!
//! A voxel: bits 0–3 class, 4–7 sky light, 8–15 material. Sections are kept here whole, [y][z][x] like the mesher's
//! input, so a window moving over the world can fill itself from them.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, RwLock};

/// What a block is to a ray.
pub const AIR: u16 = 0;
pub const FULL: u16 = 1;
pub const SLAB_LOW: u16 = 2;
pub const SLAB_TOP: u16 = 3;
pub const LEAVES: u16 = 4;
pub const GLASS: u16 = 5;
pub const WATER: u16 = 6;
/// Plants, torches, rails: nothing to a ray.
pub const PLANT: u16 = 7;
/// Stairs and the like: a ray stops in them.
pub const PARTIAL: u16 = 8;
/// Fences, panes, doors: a ray mostly passes.
pub const THIN: u16 = 9;
/// Plants the wind moves (terrain.vsh): to a ray the same as PLANT.
pub const SWAY: u16 = 10;
/// Metal blocks (terrain.fsh makes them shine): to a ray the same as FULL.
pub const SHINY: u16 = 11;

/// Whether a ray can be stopped or dimmed by this class (the coarse grid marks such bricks).
pub fn blocks_rays(class: u16) -> bool {
    !matches!(class, AIR | PLANT | SWAY)
}

/// Palette entries 216 and up are glowing materials; below, a 6×6×6 cube of colours.
pub const GLOWS: usize = 216;

/// What Java exports per block state (cursed.recode.engine.Materials).
pub struct Materials {
    /// class | material << 8, by block state id.
    pub states: Vec<u16>,
    /// RGBA float per entry: the albedo (a = 1), then the glow (rgb, a = strength 0..1).
    pub palette: Vec<[f32; 8]>,
    /// The game's cloud map, 256 x 256, 1 where a cloud is (for the clouds' shadow); None without one.
    pub clouds: Option<Vec<u8>>,
    pub epoch: u64,
}

static MATERIALS: RwLock<Option<Arc<Materials>>> = RwLock::new(None);

pub fn materials() -> Option<Arc<Materials>> {
    MATERIALS.read().unwrap().clone()
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn unpack(rgb: u32) -> [f32; 3] {
    [((rgb >> 16) & 255) as f32 / 255.0, ((rgb >> 8) & 255) as f32 / 255.0, (rgb & 255) as f32 / 255.0]
}

/// Java's export: i32 count, then per state u8 class, u8 light emission, u16 0, u32 albedo RGB, u32 glow RGB.
pub fn load_materials(bytes: &[u8]) {
    let count = i32::from_le_bytes(bytes[0..4].try_into().unwrap()).max(0) as usize;
    assert!(bytes.len() >= 4 + count * 12, "materials: {} bytes for {count} states", bytes.len());
    let mut palette = vec![[0.0f32; 8]; 256];
    for r in 0..6 {
        for g in 0..6 {
            for b in 0..6 {
                let c = [r as f32 / 5.0, g as f32 / 5.0, b as f32 / 5.0];
                palette[r * 36 + g * 6 + b] = [srgb_to_linear(c[0]), srgb_to_linear(c[1]), srgb_to_linear(c[2]), 1.0, 0.0, 0.0, 0.0, 0.0];
            }
        }
    }
    // Glows: one entry per distinct (colour, strength), the nearest one once they run out.
    let mut glows: Vec<([f32; 3], [f32; 3], u32)> = Vec::new();
    let mut states = Vec::with_capacity(count);
    for i in 0..count {
        let at = 4 + i * 12;
        let class = (bytes[at] & 15) as u16;
        let emission = bytes[at + 1].min(15) as u32;
        let albedo = unpack(u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()));
        let glow = unpack(u32::from_le_bytes(bytes[at + 8..at + 12].try_into().unwrap()));
        let material = if emission > 0 {
            let lin = [srgb_to_linear(glow[0]), srgb_to_linear(glow[1]), srgb_to_linear(glow[2])];
            let base = [srgb_to_linear(albedo[0]), srgb_to_linear(albedo[1]), srgb_to_linear(albedo[2])];
            let near = glows.iter().position(|g| g.2 == emission && (0..3).all(|k| (g.1[k] - lin[k]).abs() < 0.04));
            let index = match near {
                Some(n) => n,
                None if glows.len() < 256 - GLOWS => {
                    glows.push((base, lin, emission));
                    glows.len() - 1
                }
                None => {
                    let d = |g: &([f32; 3], [f32; 3], u32)| (0..3).map(|k| (g.1[k] - lin[k]).powi(2)).sum::<f32>() + ((g.2 as f32 - emission as f32) / 15.0).powi(2);
                    (0..glows.len()).min_by(|&a, &b| d(&glows[a]).total_cmp(&d(&glows[b]))).unwrap()
                }
            };
            GLOWS + index
        } else {
            let q = |c: f32| (c.clamp(0.0, 1.0) * 5.0).round() as usize;
            q(albedo[0]) * 36 + q(albedo[1]) * 6 + q(albedo[2])
        };
        states.push(class | (material as u16) << 8);
    }
    for (i, (base, lin, emission)) in glows.iter().enumerate() {
        palette[GLOWS + i] = [base[0], base[1], base[2], 1.0, lin[0], lin[1], lin[2], *emission as f32 / 15.0];
    }
    // After the states: the cloud map's size (256, or 0 without one), then its cells.
    let rest = &bytes[4 + count * 12..];
    let clouds = (rest.len() >= 4 + 65536 && i32::from_le_bytes(rest[0..4].try_into().unwrap()) == 256).then(|| rest[4..4 + 65536].to_vec());
    let epoch = materials().map_or(1, |m| m.epoch + 1);
    *MATERIALS.write().unwrap() = Some(Arc::new(Materials { states, palette, clouds, epoch }));
    // Sections meshed with the old table keep their old numbers until they are meshed again: the world reloads with
    // the resources anyway.
}

#[derive(Clone)]
pub enum Section {
    /// Every voxel the same (all air, all stone).
    Uniform(u16),
    Data(Box<[u16; 4096]>),
}

impl Section {
    pub fn voxel(&self, i: usize) -> u16 {
        match self {
            Section::Uniform(v) => *v,
            Section::Data(d) => d[i],
        }
    }
}

/// A section of air under the open sky: what a section the game leaves empty is.
pub const OPEN_AIR: u16 = 15 << 4;

#[derive(Default)]
struct Store {
    sections: HashMap<[i32; 3], Section>,
    dirty: Vec<[i32; 3]>,
    queued: HashSet<[i32; 3]>,
    /// Counts resets: the GPU side starts over when it changes.
    epoch: u64,
}

static STORE: Mutex<Option<Store>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut Store) -> R) -> R {
    let mut s = STORE.lock().unwrap();
    f(s.get_or_insert_with(Store::default))
}

fn put(key: [i32; 3], section: Section) {
    with(|s| {
        s.sections.insert(key, section);
        if s.queued.insert(key) {
            s.dirty.push(key);
        }
    })
}

/// A section the mesher was given: `states` and `light` are its 20³ input (two blocks round it), `origin` its
/// lowest corner in blocks.
pub fn section(origin: [i32; 3], states: &[i32], light: &[u8]) {
    let Some(m) = materials() else { return };
    let mut data = Box::new([0u16; 4096]);
    let mut first = None;
    let mut uniform = true;
    for y in 0..16 {
        for z in 0..16 {
            for x in 0..16 {
                let at = (x + 2) + (z + 2) * 20 + (y + 2) * 400;
                let state = states[at];
                let base = if state >= 0 && (state as usize) < m.states.len() { m.states[state as usize] } else { 0 };
                let sky = ((light[at] >> 4) & 15) as u16;
                let v = base | sky << 4;
                data[x + z * 16 + y * 256] = v;
                match first {
                    None => first = Some(v),
                    Some(f) if f != v => uniform = false,
                    _ => {}
                }
            }
        }
    }
    let key = [origin[0] >> 4, origin[1] >> 4, origin[2] >> 4];
    put(key, if uniform { Section::Uniform(first.unwrap_or(0)) } else { Section::Data(data) });
}

/// A section with no blocks at all (the game does not mesh it).
pub fn air(origin: [i32; 3]) {
    put([origin[0] >> 4, origin[1] >> 4, origin[2] >> 4], Section::Uniform(OPEN_AIR));
}

/// The level went (another world, another dimension): everything goes.
pub fn reset() {
    with(|s| {
        let epoch = s.epoch + 1;
        *s = Store { epoch, ..Store::default() };
    })
}

pub fn epoch() -> u64 {
    with(|s| s.epoch)
}

/// The sections that changed since the last call, at most `max`.
pub fn take_dirty(max: usize) -> Vec<[i32; 3]> {
    with(|s| {
        let n = s.dirty.len().min(max);
        let taken: Vec<[i32; 3]> = s.dirty.drain(..n).collect();
        for k in &taken {
            s.queued.remove(k);
        }
        taken
    })
}

/// Runs `f` with the section at `key` (None when the store has never seen it).
pub fn read<R>(key: [i32; 3], f: impl FnOnce(Option<&Section>) -> R) -> R {
    with(|s| f(s.sections.get(&key)))
}

/// How many sections are kept (dev statistics).
pub fn count() -> usize {
    with(|s| s.sections.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_cube_and_glows() {
        let mut b = Vec::new();
        b.extend_from_slice(&3i32.to_le_bytes());
        // air, stone-ish grey, a torch
        for (class, em, albedo, glow) in [(AIR, 0u8, 0u32, 0u32), (FULL, 0, 0x7F7F7F, 0), (PLANT, 14, 0xFFD080, 0xFFC060)] {
            b.push(class as u8);
            b.push(em);
            b.extend_from_slice(&[0, 0]);
            b.extend_from_slice(&albedo.to_le_bytes());
            b.extend_from_slice(&glow.to_le_bytes());
        }
        load_materials(&b);
        let m = materials().unwrap();
        assert_eq!(m.states[0] & 15, AIR);
        assert_eq!(m.states[1] & 15, FULL);
        assert!(((m.states[1] >> 8) as usize) < GLOWS);
        assert_eq!((m.states[2] >> 8) as usize, GLOWS);
        assert!(m.palette[GLOWS][7] > 0.9);
    }
}
