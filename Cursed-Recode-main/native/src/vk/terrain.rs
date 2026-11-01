//! The world's terrain, owned here: the geometry of every chunk section in big buffers of video memory, and the
//! draws of a frame built from the list of visible sections.
//!
//! The game's own way (LevelRenderer.extractSectionDrawGroups / prepareChunkRendersIndirect) looks every section's
//! buffers up in hash maps on every frame and writes the indirect commands in Java; that was a fifth of the render
//! thread. Here a frame is one call: Java hands over the visible sections in the game's order, Rust writes the section
//! data and the indirect commands into mapped memory, and the pass draws each layer with a few indirect calls.
//!
//! The geometry is the game's own vertex format (DefaultVertexFormat.BLOCK, 28 bytes) and the game's terrain
//! pipelines draw it, so the picture is the game's picture. Meshes arrive from any thread (the game compiles sections
//! on its workers); they wait in a queue and go to the GPU at the start of the next frame.

use super::device::Gpu;
use super::encoder::{memory_barrier, Encoder};
use super::memory::{Place, Ranges};
use super::resources::{Buffer, Doomed};
use ash::vk;
use std::collections::VecDeque;
use std::sync::Mutex;

pub const VERTEX_SIZE: u64 = 28;
use crate::world::mesher::FACINGS;
/// The packed terrain vertex (cursed.recode.render.TerrainCompact): u16 × 3 position (x y z) in 1/2048 of a block
/// from -8, light u8 × 2 (block, sky: the game's 0-240; the GPU reads them signed, the shader takes the low eight bits), colour u8 × 4, u16 × 2 UV in 1/32768. 16 bytes against the
/// game's 28: the terrain's vertices are read from memory every frame, and that reading is much of what it costs.
pub const PACKED_SIZE: u64 = 16;

/// The game's BLOCK vertices packed (UNORM16 read back as value / 65535 by the GPU; the shader scales).
fn pack_vertices(v: &[u8]) -> Vec<u8> {
    let n = v.len() / VERTEX_SIZE as usize;
    let mut out = vec![0u8; n * PACKED_SIZE as usize];
    let f = |b: &[u8], at: usize| f32::from_le_bytes(b[at..at + 4].try_into().unwrap());
    let q = |x: f32, scale: f32, bias: f32| -> u16 { ((x + bias) * scale).round().clamp(0.0, 65535.0) as u16 };
    let light = |b: &[u8], at: usize| i16::from_le_bytes(b[at..at + 2].try_into().unwrap()).clamp(0, 255) as u8;
    for i in 0..n {
        let s = &v[i * 28..i * 28 + 28];
        let d = &mut out[i * 16..i * 16 + 16];
        d[0..2].copy_from_slice(&q(f(s, 0), 2048.0, 8.0).to_le_bytes());
        d[2..4].copy_from_slice(&q(f(s, 4), 2048.0, 8.0).to_le_bytes());
        d[4..6].copy_from_slice(&q(f(s, 8), 2048.0, 8.0).to_le_bytes());
        d[6] = light(s, 24);
        d[7] = light(s, 26);
        d[8..12].copy_from_slice(&s[12..16]);
        d[12..14].copy_from_slice(&q(f(s, 16), 32768.0, 0.0).to_le_bytes());
        d[14..16].copy_from_slice(&q(f(s, 20), 32768.0, 0.0).to_le_bytes());
    }
    out
}
/// CURSED_DEV_NOMERGE: translucent geometry as the game makes it.
fn no_merge() -> bool {
    static NO: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *NO.get_or_init(|| std::env::var_os("CURSED_DEV_NOMERGE").is_some())
}

/// Translucent geometry all on one level plane (the game's vertices, 28 bytes), merged: the whole-block faces with one
/// colour and one light at their four corners, one sprite the same way round, become rectangles greedily; the rest
/// stays as it is. Water in seas and lakes is such a plane (its face from above and the same face from below):
/// 131 000 quads in the test view at 15 chunks became a few thousand, and what the GPU spends on a frame's terrain
/// goes mostly by the number of triangles (12.10: the terrain with every triangle made a point still cost 220 µs of
/// 280). Nothing on one plane can cover anything else on it, so the rectangles need no sorting; each covers its blocks
/// exactly as their faces did, the shaders repeating the sprite once a block (TerrainCompact: the colour's alpha, 255
/// for every vertex the game makes, holds how; the UV is the sprite's corner; the sprite's size is a power of two of
/// the atlas). Returns the vertices, down faces first then up faces (FACINGS order), or None when the geometry is
/// not one level plane or nothing merges.
fn merge_plane(v: &[u8]) -> Option<(Vec<u8>, [u32; FACINGS])> {
    const V: usize = 28;
    const Q: usize = V * 4;
    let quads = v.len() / Q;
    if quads < 2 || v.len() % Q != 0 {
        return None;
    }
    let f = |at: usize| f32::from_le_bytes(v[at..at + 4].try_into().unwrap());
    let u = |at: usize| u32::from_le_bytes(v[at..at + 4].try_into().unwrap());
    let y = f(4);
    if (0..quads * 4).any(|k| f(k * V + 4) != y) {
        return None;
    }
    // A key per mergeable quad: (up, colour, light, umin, vmin, umax, vmax, way) and its block cell (x, z).
    type Key = (bool, u32, u32, u32, u32, u32, u32, u8);
    let mut cells: std::collections::HashMap<Key, ([u32; 16], usize)> = std::collections::HashMap::new();
    let mut up_of = vec![false; quads];
    let mut merged_from = vec![false; quads];
    let log2_exact = |size: f32| -> Option<u32> {
        let k = (1.0 / size).log2().round();
        ((5.0..=8.0).contains(&k) && 1.0 / 2f32.powi(k as i32) == size).then_some(k as u32)
    };
    for q in 0..quads {
        let b = q * Q;
        let p: [[f32; 2]; 4] = std::array::from_fn(|k| [f(b + k * V), f(b + k * V + 8)]);
        let d1 = [p[2][0] - p[0][0], p[2][1] - p[0][1]];
        let d2 = [p[3][0] - p[1][0], p[3][1] - p[1][1]];
        // the winding's normal along y: (d1 × d2).y = d1.z·d2.x − d1.x·d2.z
        let ny = d1[1] * d2[0] - d1[0] * d2[1];
        up_of[q] = ny > 0.0;
        if ny == 0.0 {
            continue;
        }
        let color = u(b + 12);
        let light = u(b + 24);
        if (1..4).any(|k| u(b + k * V + 12) != color || u(b + k * V + 24) != light) || color >> 24 != 255 {
            continue;
        }
        let (x0, z0) = (p.iter().map(|c| c[0]).fold(f32::MAX, f32::min), p.iter().map(|c| c[1]).fold(f32::MAX, f32::min));
        if x0.fract() != 0.0 || z0.fract() != 0.0 || !(0.0..16.0).contains(&x0) || !(0.0..16.0).contains(&z0) {
            continue;
        }
        let corner: [(u32, u32); 4] = std::array::from_fn(|k| (p[k][1] - z0, p[k][0] - x0)).map(|(a1, a2)| (a1.to_bits(), a2.to_bits()));
        if corner.iter().any(|&(a1, a2)| !(a1 == 0f32.to_bits() || a1 == 1f32.to_bits()) || !(a2 == 0f32.to_bits() || a2 == 1f32.to_bits())) {
            continue;
        }
        let uv: [[f32; 2]; 4] = std::array::from_fn(|k| [f(b + k * V + 16), f(b + k * V + 20)]);
        let (umin, vmin) = (uv.iter().map(|t| t[0]).fold(f32::MAX, f32::min), uv.iter().map(|t| t[1]).fold(f32::MAX, f32::min));
        let (umax, vmax) = (uv.iter().map(|t| t[0]).fold(f32::MIN, f32::max), uv.iter().map(|t| t[1]).fold(f32::MIN, f32::max));
        let (Some(ku), Some(kv)) = (log2_exact(umax - umin), log2_exact(vmax - vmin)) else { continue };
        // which way the sprite lies: u along the in-plane axis a1 (z) or a2 (x), each either way
        let way = (0u8..8).find(|&w| {
            (0..4).all(|k| {
                let (a1, a2) = (u8::from(p[k][1] - z0 == 1.0), u8::from(p[k][0] - x0 == 1.0));
                let (tu, tv) = if w & 1 != 0 { (a2, a1) } else { (a1, a2) };
                let su = u8::from(uv[k][0] == umax) ^ ((w >> 1) & 1);
                let sv = u8::from(uv[k][1] == vmax) ^ ((w >> 2) & 1);
                (uv[k][0] == umin || uv[k][0] == umax) && (uv[k][1] == vmin || uv[k][1] == vmax) && su == tu && sv == tv
            })
        });
        let Some(way) = way else { continue };
        let code = way | ((ku - 5) as u8) << 3 | ((kv - 5) as u8) << 5;
        let key = (up_of[q], color, light, umin.to_bits(), vmin.to_bits(), umax.to_bits(), vmax.to_bits(), code);
        let e = cells.entry(key).or_insert(([0; 16], q));
        let (cx, cz) = (x0 as usize, z0 as usize);
        if e.0[cz] & (1 << cx) != 0 {
            continue;
        }
        e.0[cz] |= 1 << cx;
        merged_from[q] = true;
    }
    let mergeable = merged_from.iter().filter(|&&m| m).count();
    if mergeable < 2 {
        return None;
    }
    // greedy rectangles per key: rows are z, bits are x
    let mut rects: Vec<(Key, usize, [u32; 4])> = Vec::new();
    for (key, (rows, sample)) in &cells {
        let mut rows = *rows;
        for z in 0..16 {
            while rows[z] != 0 {
                let x = rows[z].trailing_zeros();
                let mut w = 0;
                while x + w < 16 && rows[z] & (1 << (x + w)) != 0 {
                    w += 1;
                }
                let mask = (((1u64 << w) - 1) as u32) << x;
                let mut h = 1;
                while z + h < 16 && rows[z + h] & mask == mask {
                    h += 1;
                }
                for r in &mut rows[z..z + h] {
                    *r &= !mask;
                }
                rects.push((*key, *sample, [x, z as u32, w, h as u32]));
            }
        }
    }
    // down faces first, then up: FACINGS order (0 unculled, 1 DOWN, 2 UP)
    let mut out = Vec::with_capacity((quads - mergeable + rects.len()) * Q);
    let mut facings = [0u32; FACINGS];
    for up in [false, true] {
        for q in (0..quads).filter(|&q| up_of[q] == up && !merged_from[q]) {
            out.extend_from_slice(&v[q * Q..q * Q + Q]);
            facings[1 + usize::from(up)] += 1;
        }
        for (key, sample, [x, z, w, h]) in rects.iter().filter(|r| r.0 .0 == up) {
            // the sample quad's corners, each moved to the rectangle's matching corner; its winding stays
            let b = sample * Q;
            let (sx, sz) = ((0..4).map(|k| f(b + k * V)).fold(f32::MAX, f32::min), (0..4).map(|k| f(b + k * V + 8)).fold(f32::MAX, f32::min));
            for k in 0..4 {
                let mut vert = [0u8; V];
                vert.copy_from_slice(&v[b + k * V..b + k * V + V]);
                let cx = if f(b + k * V) > sx { (x + w) as f32 } else { *x as f32 };
                let cz = if f(b + k * V + 8) > sz { (z + h) as f32 } else { *z as f32 };
                vert[0..4].copy_from_slice(&cx.to_le_bytes());
                vert[8..12].copy_from_slice(&cz.to_le_bytes());
                // the colour's alpha: how the sprite repeats (TerrainCompact), the UV: the sprite's corner
                vert[15] = key.7;
                vert[16..20].copy_from_slice(&f32::from_bits(key.3).to_le_bytes());
                vert[20..24].copy_from_slice(&f32::from_bits(key.4).to_le_bytes());
                out.extend_from_slice(&vert);
            }
            facings[1 + usize::from(up)] += 1;
        }
    }
    Some((out, facings))
}

/// Dev (BENCH): how far a mesh's quads would merge — whole unit faces on one plane with one sprite (any of its turns:
/// the shader would turn each block's), one colour and one light at all four corners, greedily into rectangles.
/// Printed now and then: quads, of them mergeable, rectangles they make; per layer.
fn dev_merge_estimate(layer: usize, v: &[u8]) {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
    static TOTAL: [[AtomicU64; 6]; 3] = [const { [const { AtomicU64::new(0) }; 6] }; 3];
    static MESHES: AtomicU64 = AtomicU64::new(0);
    let f = |at: usize| f32::from_le_bytes(v[at..at + 4].try_into().unwrap());
    let u = |at: usize| u32::from_le_bytes(v[at..at + 4].try_into().unwrap());
    let quads = v.len() / (28 * 4);
    // key -> cells (16x16 bitmap rows), strict key (the exact corner UVs) too
    let mut groups: HashMap<(u8, u32, u32, [u32; 4], u32, u32, bool), [u32; 17]> = HashMap::new();
    let mut strict: HashMap<(u8, u32, u32, [u32; 8], u32, u32, bool), [u32; 17]> = HashMap::new();
    let mut eligible = 0u64;
    // free: the light and colour per pixel (not per corner): any light, the tint alone must agree
    let mut free: HashMap<(u8, u32, [u32; 4], bool, u32), [u32; 17]> = HashMap::new();
    let mut free_eligible = 0u64;
    for q in 0..quads {
        let b = q * 112;
        let p: [[f32; 3]; 4] = std::array::from_fn(|k| [f(b + k * 28), f(b + k * 28 + 4), f(b + k * 28 + 8)]);
        let color: [u32; 4] = std::array::from_fn(|k| u(b + k * 28 + 12));
        let light: [u32; 4] = std::array::from_fn(|k| u(b + k * 28 + 24));
        let uv: [[f32; 2]; 4] = std::array::from_fn(|k| [f(b + k * 28 + 16), f(b + k * 28 + 20)]);
        let uniform = !(color.iter().any(|&c| c != color[0]) || light.iter().any(|&l| l != light[0]));
        let Some(axis) = (0..3).find(|&a| p.iter().all(|c| c[a] == p[0][a])) else { continue };
        let (a1, a2) = ((axis + 1) % 3, (axis + 2) % 3);
        let lo1 = p.iter().map(|c| c[a1]).fold(f32::MAX, f32::min);
        let hi1 = p.iter().map(|c| c[a1]).fold(f32::MIN, f32::max);
        let lo2 = p.iter().map(|c| c[a2]).fold(f32::MAX, f32::min);
        let hi2 = p.iter().map(|c| c[a2]).fold(f32::MIN, f32::max);
        if hi1 - lo1 != 1.0 || hi2 - lo2 != 1.0 || lo1.fract() != 0.0 || lo2.fract() != 0.0 || !(0.0..16.0).contains(&lo1) || !(0.0..16.0).contains(&lo2) {
            continue;
        }
        let d1 = [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]];
        let d2 = [p[3][0] - p[1][0], p[3][1] - p[1][1], p[3][2] - p[1][2]];
        let n = [d1[1] * d2[2] - d1[2] * d2[1], d1[2] * d2[0] - d1[0] * d2[2], d1[0] * d2[1] - d1[1] * d2[0]];
        let front = n[axis] > 0.0;
        let rect = [
            uv.iter().map(|t| t[0]).fold(f32::MAX, f32::min).to_bits(),
            uv.iter().map(|t| t[1]).fold(f32::MAX, f32::min).to_bits(),
            uv.iter().map(|t| t[0]).fold(f32::MIN, f32::max).to_bits(),
            uv.iter().map(|t| t[1]).fold(f32::MIN, f32::max).to_bits(),
        ];
        let (i, j) = (lo1 as usize, lo2 as usize);
        // the tint: the brightest corner's hue stands for it (shade and light only darken); rounded so AO does not split it
        let hue = color.iter().map(|&c| {
            let (r, g, b) = ((c & 255) as f32, ((c >> 8) & 255) as f32, ((c >> 16) & 255) as f32);
            let m = r.max(g).max(b).max(1.0);
            ((r / m * 15.0).round() as u32) | ((g / m * 15.0).round() as u32) << 4 | ((b / m * 15.0).round() as u32) << 8
        }).max().unwrap();
        free_eligible += 1;
        let cells = free.entry((axis as u8, p[0][axis].to_bits(), rect, front, hue)).or_insert([0; 17]);
        cells[j] |= 1 << i;
        if !uniform {
            continue;
        }
        eligible += 1;
        let cells = groups.entry((axis as u8, p[0][axis].to_bits(), 0, rect, color[0], light[0], front)).or_insert([0; 17]);
        cells[j] |= 1 << i;
        // strict: the corner UVs in the order of the corners sorted by position
        let mut corners: Vec<(u32, u32, u32, u32)> = (0..4).map(|k| (p[k][a1].to_bits(), p[k][a2].to_bits(), uv[k][0].to_bits(), uv[k][1].to_bits())).collect();
        corners.sort();
        let suv: [u32; 8] = std::array::from_fn(|k| if k % 2 == 0 { corners[k / 2].2 } else { corners[k / 2].3 });
        let cells = strict.entry((axis as u8, p[0][axis].to_bits(), 0, suv, color[0], light[0], front)).or_insert([0; 17]);
        cells[j] |= 1 << i;
    }
    let greedy = |mut rows: [u32; 17]| -> u64 {
        let mut n = 0;
        for j in 0..16 {
            while rows[j] != 0 {
                let i = rows[j].trailing_zeros();
                let mut w = 0;
                while i + w < 16 && rows[j] & (1 << (i + w)) != 0 {
                    w += 1;
                }
                let mask = (((1u64 << w) - 1) as u32) << i;
                let mut h = 1;
                while j + h < 16 && rows[j + h] & mask == mask {
                    h += 1;
                }
                for r in j..j + h {
                    rows[r] &= !mask;
                }
                n += 1;
            }
        }
        n
    };
    let merged: u64 = groups.values().map(|c| greedy(*c)).sum();
    let merged_strict: u64 = strict.values().map(|c| greedy(*c)).sum();
    let merged_free: u64 = free.values().map(|c| greedy(*c)).sum();
    let t = &TOTAL[layer.min(2)];
    t[0].fetch_add(quads as u64, Relaxed);
    t[1].fetch_add(eligible, Relaxed);
    t[2].fetch_add(merged, Relaxed);
    t[3].fetch_add(merged_strict, Relaxed);
    t[4].fetch_add(free_eligible, Relaxed);
    t[5].fetch_add(merged_free, Relaxed);
    if MESHES.fetch_add(1, Relaxed) % 400 == 399 {
        for (l, name) in ["solid", "cutout", "translucent"].iter().enumerate() {
            let t = &TOTAL[l];
            let (q, e, m, ms) = (t[0].load(Relaxed), t[1].load(Relaxed), t[2].load(Relaxed), t[3].load(Relaxed));
            eprintln!("[merge] {name}: {q} quads, {e} mergeable -> {m} rectangles (same turn only: {ms}); after merging {} quads ({:.0}%)", q - e + m, 100.0 * (q - e + m) as f64 / q.max(1) as f64);
            let (fe, fm) = (t[4].load(Relaxed), t[5].load(Relaxed));
            eprintln!("[merge] {name} with light per pixel: {fe} unit faces -> {fm} rectangles; after merging {} quads ({:.0}%)", q - fe + fm, 100.0 * (q - fe + fm) as f64 / q.max(1) as f64);
        }
    }
}

/// CURSED_DEV_STALLS: what holds a frame up goes to the log.
pub(super) fn stalls() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("CURSED_DEV_STALLS").is_some())
}

/// Dev: seconds since the first call, to line the stall logs up.
pub(super) fn clock() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}

pub const LAYERS: usize = 3;
pub const TRANSLUCENT: usize = 2;
/// Pass op 17's layer for the translucent sections from FAR_WATER out only (cursed.recode.render.NativeTerrain).
pub const FAR_TRANSLUCENT: usize = 3;

/// Which of a layer's groups a draw takes.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Part {
    All,
    Far,
    Near,
}

/// From how far translucent terrain (water) is drawn the classic way when the game draws with Improved Transparency:
/// its two passes over the order-independent targets cost some 20 µs at 1440p for the far ocean of the benchmark
/// (12.10); drawn sorted, as without it, some 5. One layer of water comes out the same either way; only something
/// translucent behind the far water (no particle or shadow is drawn that far) would come out in front of it.
/// CURSED_DEV_FARWATER=blocks, 0 for none.
fn far_water() -> Option<f64> {
    static R: std::sync::OnceLock<Option<f64>> = std::sync::OnceLock::new();
    *R.get_or_init(|| match std::env::var("CURSED_DEV_FARWATER").ok().and_then(|v| v.parse::<f64>().ok()) {
        Some(r) if r <= 0.0 => None,
        Some(r) => Some(r),
        None => Some(80.0),
    })
}
/// ChunkSectionInfo: ivec3 position, float visibility.
const SECTION_INFO_SIZE: u64 = 16;
/// VkDrawIndexedIndirectCommand.
const COMMAND_SIZE: u64 = 20;
const VERTEX_BLOCK: u64 = 64 << 20;
const INDEX_BLOCK: u64 = 16 << 20;

/// One kind of geometry memory: blocks of video memory, cut by `Ranges`.
struct Arena {
    usage: vk::BufferUsageFlags,
    block_size: u64,
    blocks: Vec<(Buffer, Ranges)>,
}

#[derive(Clone, Copy, Debug)]
struct Piece {
    block: u32,
    offset: u64,
    size: u64,
}

impl Arena {
    fn new(usage: vk::BufferUsageFlags, block_size: u64) -> Self {
        Arena { usage, block_size, blocks: Vec::new() }
    }

    fn alloc(&mut self, gpu: &Gpu, size: u64, align: u64) -> Piece {
        for (i, (_, ranges)) in self.blocks.iter_mut().enumerate() {
            if let Some(offset) = ranges.take(size, align) {
                return Piece { block: i as u32, offset, size };
            }
        }
        let block_size = self.block_size.max(size.div_ceil(align) * align);
        let t0 = std::time::Instant::now();
        let buffer = Buffer::with(gpu, self.usage | vk::BufferUsageFlags::TRANSFER_DST, block_size, Place::Device)
            .unwrap_or_else(|e| panic!("Failed to allocate terrain memory: {e:?}"));
        if stalls() {
            eprintln!("[stall] terrain: a new block of {} MiB in {:.1} ms ({} now)", block_size >> 20, t0.elapsed().as_secs_f64() * 1000.0, self.blocks.len() + 1);
        }
        gpu.name(buffer.raw, "Cursed terrain");
        let mut ranges = Ranges::new(block_size);
        let offset = ranges.take(size, align).expect("a fresh block holds the piece");
        self.blocks.push((buffer, ranges));
        Piece { block: (self.blocks.len() - 1) as u32, offset, size }
    }

    fn free(&mut self, p: Piece) {
        self.blocks[p.block as usize].1.give(p.offset, p.size);
    }

    fn buffer(&self, block: u32) -> &Buffer {
        &self.blocks[block as usize].0
    }

    fn destroy(&mut self, gpu: &Gpu) {
        for (b, _) in self.blocks.drain(..) {
            Doomed::Buffer(Box::new(b)).destroy(gpu);
        }
    }
}

#[derive(Clone, Copy)]
struct LayerMesh {
    vertices: Piece,
    /// Sorted indices of their own (translucent geometry), or the shared quad pattern when `None`.
    indices: Option<Piece>,
    index_count: u32,
    index_u32: bool,
    /// Quads per facing (world/mesher.rs FACINGS order) when the native mesher made the geometry.
    facings: Option<[u32; FACINGS]>,
    /// Its vertices are packed (PACKED_SIZE) — drawn only while the terrain pipelines read packed vertices.
    packed: bool,
    /// Translucent geometry merged here (merge_plane): drawn in any order, the game's sorted indices are not used.
    own: bool,
}

#[derive(Default)]
struct Mesh {
    layers: [Option<LayerMesh>; LAYERS],
}

/// A mesh's data waiting for the next frame.
struct Upload {
    mesh: u32,
    layer: usize,
    vertices: Option<Vec<u8>>,
    indices: Option<Vec<u8>>,
    index_count: u32,
    index_u32: bool,
    facings: Option<[u32; FACINGS]>,
    own: bool,
    /// The section's origin (Store::origins).
    origin: [i32; 3],
}

#[derive(Default)]
struct Queue {
    uploads: Vec<Upload>,
    /// Each with the number of the game's terrain build it came after (see `Terrain::build`).
    releases: Vec<(u32, u64)>,
    reset: bool,
}

/// Meshes and their memory; behind the renderer's terrain mutex.
pub struct Store {
    meshes: Vec<Option<Mesh>>,
    /// Per mesh id, a bit per layer it has geometry in: what the frame's list reads of a section the faces hold, so it
    /// does not touch the mesh (a few hundred bytes somewhere in a large array: a cache miss or several each, some
    /// 20 µs a frame for the four hundred held sections at 15 chunks, 12.10).
    summary: Vec<u8>,
    /// Per mesh id, its section's origin (i32::MIN none known), and the terrain build that last listed it in view: the
    /// far terrain's faces draw every mesh in their reach, also the ones out of view (a face drawn from the sections in
    /// view only had holes where the view turned to before the face was drawn again: the sky through them, and far
    /// water and ice, drawn live, hanging in it — 12.10).
    origins: Vec<[i32; 3]>,
    seen: Vec<u64>,
    vertices: Arena,
    indices: Arena,
    /// Pieces given back, freed once the submit that last drew them is done on the GPU.
    retired: VecDeque<(u64, bool, Piece)>,
    /// Meshes the game let go of that a frame it recorded earlier may still draw: each goes once the frame thread builds
    /// a later one (mesh id, the game's build it came after).
    releasing: Vec<(u32, u64)>,
    /// The shared index pattern of quads (0 1 2 2 3 0, then +4), long enough for the biggest section.
    quad_indices: Option<Buffer>,
    quad_count: u64,
}

/// What a frame draws, per layer: runs of draws that share buffers.
#[derive(Clone, Copy)]
pub struct Group {
    vertex_block: u32,
    /// `None`: the shared quad indices.
    index_block: Option<u32>,
    index_u32: bool,
    commands: u64,
    count: u32,
    /// Opaque layers with occlusion culling: where the second copies of the commands are (vk/occlusion.rs).
    second: u64,
}

#[derive(Default)]
pub struct FrameDraws {
    pub groups: [Vec<Group>; LAYERS],
    /// The translucent groups of sections from FAR_WATER out (back to front, before the rest): with Improved
    /// Transparency drawn the classic way in the opaque pass (pass.rs, op 17 layer FAR_TRANSLUCENT), and so this
    /// frame's transparency passes draw only the groups after them (`far_drawn`).
    pub far_translucent: usize,
    pub far_drawn: bool,
    /// Where the section infos (the instance buffer) and the indirect commands are this frame.
    info: Option<(*const Buffer, u64)>,
    commands: Option<*const Buffer>,
}

/// What the game's threads hand over: meshes to upload and to release. Any thread.
pub struct Feed {
    queue: Mutex<Queue>,
    /// Terrain goes to the GPU packed (the pipelines' vertex shader unpacks; TerrainCompact).
    pub packed: std::sync::atomic::AtomicBool,
    /// Mesh ids: the next new one, the ones given back, and whether each id is in use (a release counts once).
    ids: Mutex<(u32, Vec<u32>, Vec<bool>)>,
}

/// The meshes on the GPU and this frame's draws. Render thread only.
pub struct Terrain {
    store: Store,
    pub frame: FrameDraws,
    /// The far terrain's faces (vk/impostor.rs), and this frame's draws into them per face and layer.
    pub impostor: super::impostor::Impostor,
    face_groups: [[Vec<Group>; super::impostor::HELD]; super::impostor::FACES],
    /// This frame's camera-relative ProjMat · ModelViewMat (the faces are laid over the frame with it).
    pub view_proj: [f32; 16],
    /// Once Java has compiled its shaders, unless switched off (CURSED_DEV_NOOCCLUSION).
    pub occlusion: Option<super::occlusion::Occlusion>,
}

impl Feed {
    pub fn new() -> Feed {
        Feed {
            queue: Mutex::new(Queue::default()),
            packed: std::sync::atomic::AtomicBool::new(false),
            ids: Mutex::new((1, Vec::new(), vec![false; 2])),
        }
    }


    /// A new mesh id; any thread.
    pub fn new_mesh(&self) -> u32 {
        let mut ids = self.ids.lock().unwrap();
        let id = match ids.1.pop() {
            Some(id) => id,
            None => {
                ids.0 += 1;
                ids.0 - 1
            }
        };
        if ids.2.len() <= id as usize {
            ids.2.resize(id as usize + 1, false);
        }
        ids.2[id as usize] = true;
        id
    }

    /// A layer of a mesh, or new indices for it (the translucent layer re-sorted); any thread.
    #[allow(clippy::too_many_arguments)]
    pub fn upload(
        &self,
        mesh: u32,
        layer: usize,
        vertices: Option<&[u8]>,
        indices: Option<&[u8]>,
        index_count: u32,
        index_u32: bool,
        facings: Option<[u32; FACINGS]>,
        origin: [i32; 3],
    ) {
        assert!(layer < LAYERS, "terrain layer {layer}");
        // Translucent geometry on one level plane (water in seas, lakes, rivers) merged into rectangles, here on the
        // game's thread; drawn in any order then, so the game's sorting of it is not used.
        if layer == TRANSLUCENT && self.packed.load(std::sync::atomic::Ordering::Relaxed) && !no_merge() {
            if let Some((merged, f)) = vertices.and_then(merge_plane) {
                let quads = f.iter().sum::<u32>();
                // Indices of its own (the plain quad pattern) in the arena where the game's sorted ones are, 16-bit as
                // the game's are: in the far-to-near order of translucent draws, meshes of other buffers or index
                // sizes between the game's split the draw calls (measured: 183 instead of 1, the frame thread's list
                // 70 µs longer).
                let pattern = (0..quads).flat_map(|q| {
                    let i = q * 4;
                    [i, i + 1, i + 2, i + 2, i + 3, i]
                });
                let wide = quads * 4 > 65536;
                let indices: Vec<u8> = if wide { pattern.flat_map(u32::to_le_bytes).collect() } else { pattern.flat_map(|i| (i as u16).to_le_bytes()).collect() };
                let up = Upload { mesh, layer, vertices: Some(merged), indices: Some(indices), index_count: quads * 6, index_u32: wide, facings: Some(f), own: true, origin };
                self.queue.lock().unwrap().uploads.push(up);
                return;
            }
        }
        let up = Upload {
            mesh,
            layer,
            vertices: vertices.map(<[u8]>::to_vec),
            indices: indices.map(<[u8]>::to_vec),
            index_count,
            index_u32,
            facings,
            own: false,
            origin,
        };
        self.queue.lock().unwrap().uploads.push(up);
    }

    /// The mesh is gone (replaced or its section left) after the game recorded its terrain build `after`; any thread.
    pub fn release(&self, mesh: u32, after: u64) {
        self.queue.lock().unwrap().releases.push((mesh, after));
    }

    /// Everything goes (the level changed or the renderer was reset); any thread.
    pub fn reset(&self) {
        let mut q = self.queue.lock().unwrap();
        q.uploads.clear();
        q.releases.clear();
        q.reset = true;
    }

}

/// Which facings of a section at `origin` can face a camera at `camera` (bit k in FACINGS order). A section's quads
/// lie on planes within a block and a quarter around it (models reach a block past their own, offsets a quarter).
fn facing_mask(origin: [f64; 3], camera: [f64; 3]) -> u32 {
    // CURSED_DEV_NOFACINGS: everything drawn, to compare pictures and timings.
    static ALL: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *ALL.get_or_init(|| std::env::var_os("CURSED_DEV_NOFACINGS").is_some()) {
        return (1 << FACINGS) - 1;
    }
    const MARGIN: f64 = 1.25 + 1.0e-3;
    let (x, y, z) = (camera[0] - origin[0], camera[1] - origin[1], camera[2] - origin[2]);
    let low = |c: f64| c < 16.0 + MARGIN;
    let high = |c: f64| c > -MARGIN;
    // An upright diagonal with normal (a, 0, b): shown when the camera is in front of its plane somewhere in the
    // section, a·x + b·z above the least a·px + b·pz over the section's box.
    let least = |s: f64| if s > 0.0 { -MARGIN } else { -(16.0 + MARGIN) };
    let diagonal = |a: f64, b: f64| a * x + b * z > least(a) + least(b);
    let mut m = 1u32;
    let shows = [
        low(y),
        high(y),
        low(z),
        high(z),
        low(x),
        high(x),
        diagonal(1.0, 1.0),
        diagonal(1.0, -1.0),
        diagonal(-1.0, 1.0),
        diagonal(-1.0, -1.0),
    ];
    for (k, shows) in shows.into_iter().enumerate() {
        if shows {
            m |= 1 << (k + 1);
        }
    }
    m
}

impl Terrain {
    pub fn new() -> Terrain {
        Terrain {
            store: Store {
                meshes: Vec::new(),
                summary: Vec::new(),
                origins: Vec::new(),
                seen: Vec::new(),
                vertices: Arena::new(vk::BufferUsageFlags::VERTEX_BUFFER, VERTEX_BLOCK),
                indices: Arena::new(vk::BufferUsageFlags::INDEX_BUFFER, INDEX_BLOCK),
                retired: VecDeque::new(),
                releasing: Vec::new(),
                quad_indices: None,
                quad_count: 0,
            },
            frame: FrameDraws::default(),
            impostor: super::impostor::Impostor::new(),
            face_groups: Default::default(),
            view_proj: [0.0; 16],
            occlusion: None,
        }
    }

    /// Store::summary of mesh `id` from what it has now.
    fn summarize(&mut self, id: usize) {
        let bits = self.store.meshes.get(id).and_then(Option::as_ref).map_or(0u8, |m| {
            m.layers.iter().enumerate().filter(|(_, l)| l.as_ref().is_some_and(|l| l.index_count > 0)).fold(0u8, |b, (k, _)| b | 1 << k)
        });
        if self.store.summary.len() <= id {
            self.store.summary.resize(id + 1, 0);
        }
        self.store.summary[id] = bits;
    }

    fn retire(&mut self, submit: u64, layer: &LayerMesh) {
        self.store.retired.push_back((submit, false, layer.vertices));
        if let Some(i) = layer.indices {
            self.store.retired.push_back((submit, true, i));
        }
    }

    /// Applies what the threads queued: the meshes' data goes to the GPU through staging memory, copied by the frame's
    /// command buffer before anything draws. Render thread. `build`: the number of the game's terrain build being made.
    fn apply_queue(&mut self, feed: &Feed, gpu: &Gpu, enc: &mut Encoder, build: u64) {
        let t0 = std::time::Instant::now();
        let mut bytes = 0usize;
        let mut count = 0usize;
        while let Some(&(submit, is_index, piece)) = self.store.retired.front() {
            if submit > enc.completed {
                break;
            }
            self.store.retired.pop_front();
            if is_index {
                self.store.indices.free(piece);
            } else {
                self.store.vertices.free(piece);
            }
        }
        let (uploads, releases, reset) = {
            let mut q = feed.queue.lock().unwrap();
            (std::mem::take(&mut q.uploads), std::mem::take(&mut q.releases), std::mem::replace(&mut q.reset, false))
        };
        // The game records its frames ahead of this thread: a mesh it let go of while recording build n (its section
        // got a new one, or left) may still be drawn by the builds up to n that wait here to be made. Dropped at once,
        // its section was a hole for a frame: the chunks that blinked when flying into new ones.
        self.store.releasing.extend(releases);
        let mut releases = Vec::new();
        // CURSED_DEV_RELEASENOW: as before, each at once (to compare the holes)
        static NOW: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        let now_all = *NOW.get_or_init(|| std::env::var_os("CURSED_DEV_RELEASENOW").is_some());
        self.store.releasing.retain(|&(id, after)| {
            if reset || now_all || after < build {
                releases.push(id);
                false
            } else {
                true
            }
        });
        let now = enc.current;
        if reset {
            self.impostor.reset();
        }
        if !uploads.is_empty() || !releases.is_empty() {
            self.impostor.changed();
        }
        if reset {
            self.store.summary.clear();
            self.store.origins.clear();
            self.store.seen.clear();
            let meshes = std::mem::take(&mut self.store.meshes);
            for m in meshes.into_iter().flatten() {
                for l in m.layers.into_iter().flatten() {
                    self.retire(now, &l);
                }
            }
        }
        let mut copies: Vec<(vk::Buffer, vk::Buffer, vk::BufferCopy)> = Vec::new();
        let n_releases = releases.len();
        for up in uploads {
            count += 1;
            bytes += up.vertices.as_ref().map_or(0, Vec::len) + up.indices.as_ref().map_or(0, Vec::len);
            let id = up.mesh as usize;
            if self.store.meshes.len() <= id {
                self.store.meshes.resize_with(id + 1, || None);
            }
            let previous = self.store.meshes[id].get_or_insert_with(Mesh::default).layers[up.layer];
            let mut layer = previous.unwrap_or(LayerMesh {
                vertices: Piece { block: 0, offset: 0, size: 0 },
                indices: None,
                index_count: 0,
                index_u32: false,
                facings: None,
                packed: false,
                own: false,
            });
            // the game's new order for geometry merged here: not used (any order is right for it)
            if up.vertices.is_none() && layer.own {
                continue;
            }
            if let Some(v) = &up.vertices {
                if super::stats::passes() {
                    dev_merge_estimate(up.layer, v);
                }
                if let Some(p) = previous {
                    self.store.retired.push_back((now, false, p.vertices));
                }
                let packed = feed.packed.load(std::sync::atomic::Ordering::Relaxed);
                let owned;
                let v: &[u8] = if packed {
                    owned = pack_vertices(v);
                    &owned
                } else {
                    v
                };
                let piece = self.store.vertices.alloc(gpu, v.len() as u64, if packed { PACKED_SIZE } else { VERTEX_SIZE });
                let staged = enc.transient.staging(gpu, v.len() as u64, 4, v.len() as u64, 1);
                unsafe { std::ptr::copy_nonoverlapping(v.as_ptr(), staged.host, v.len()) };
                let region = vk::BufferCopy { src_offset: staged.offset, dst_offset: piece.offset, size: v.len() as u64 };
                layer.packed = packed;
                copies.push((unsafe { (*staged.buffer).raw }, self.store.vertices.buffer(piece.block).raw, region));
                layer.vertices = piece;
                layer.index_count = up.index_count;
                // Facings fit only the vertices they were counted with, and only with the plain quad indices.
                layer.facings = up.facings.filter(|f| (up.layer != TRANSLUCENT || up.own) && f.iter().sum::<u32>() as u64 * 6 == up.index_count as u64);
                layer.own = up.own;
                if up.indices.is_none() {
                    if let Some(old) = layer.indices.take() {
                        self.store.retired.push_back((now, true, old));
                    }
                }
            }
            if let Some(ix) = &up.indices {
                if let Some(old) = layer.indices.take() {
                    self.store.retired.push_back((now, true, old));
                }
                let align = if up.index_u32 { 4 } else { 2 };
                let piece = self.store.indices.alloc(gpu, ix.len() as u64, align);
                let staged = enc.transient.staging(gpu, ix.len() as u64, 4, ix.len() as u64, 1);
                unsafe { std::ptr::copy_nonoverlapping(ix.as_ptr(), staged.host, ix.len()) };
                let region = vk::BufferCopy { src_offset: staged.offset, dst_offset: piece.offset, size: ix.len() as u64 };
                copies.push((unsafe { (*staged.buffer).raw }, self.store.indices.buffer(piece.block).raw, region));
                layer.indices = Some(piece);
                layer.index_count = up.index_count;
                layer.index_u32 = up.index_u32;
            }
            self.store.meshes[id].as_mut().unwrap().layers[up.layer] = Some(layer);
            if self.store.origins.len() <= id {
                self.store.origins.resize(id + 1, [i32::MIN; 3]);
            }
            self.store.origins[id] = up.origin;
            self.summarize(id);
        }
        for id in releases {
            if let Some(slot) = self.store.meshes.get_mut(id as usize) {
                if let Some(m) = slot.take() {
                    for l in m.layers.into_iter().flatten() {
                        self.retire(now, &l);
                    }
                }
            }
            self.summarize(id as usize);
            let mut ids = feed.ids.lock().unwrap();
            if ids.2.get(id as usize).copied().unwrap_or(false) {
                ids.2[id as usize] = false;
                ids.1.push(id);
            }
        }
        if !copies.is_empty() {
            let cmd = enc.cmd(gpu);
            enc.pass_started(gpu, cmd, "Terrain uploads");
            for (src, dst, region) in copies {
                unsafe { gpu.device.cmd_copy_buffer(cmd, src, dst, &[region]) };
            }
            memory_barrier(gpu, cmd);
            enc.pass_ended(gpu, cmd);
        }
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        if stalls() && ms > 3.0 {
            eprintln!("[stall] t={:.3} terrain: {count} uploads ({:.1} MiB), {n_releases} releases in {ms:.1} ms", clock(), bytes as f64 / 1048576.0);
        }
    }

    /// The shared quad indices, grown to `quads` quads.
    fn ensure_quads(&mut self, gpu: &Gpu, enc: &mut Encoder, quads: u64) {
        if quads <= self.store.quad_count {
            return;
        }
        let count = quads.next_power_of_two().max(4096);
        let bytes = count * 6 * 4;
        let b = Buffer::with(gpu, vk::BufferUsageFlags::INDEX_BUFFER | vk::BufferUsageFlags::TRANSFER_DST, bytes, Place::Device)
            .unwrap_or_else(|e| panic!("Failed to allocate quad indices: {e:?}"));
        let mut data = Vec::with_capacity(bytes as usize);
        for q in 0..count as u32 {
            let i = q * 4;
            for v in [i, i + 1, i + 2, i + 2, i + 3, i] {
                data.extend_from_slice(&v.to_le_bytes());
            }
        }
        enc.write_buffer(gpu, &b, 0, &data);
        if let Some(old) = self.store.quad_indices.replace(b) {
            enc.defer(Doomed::Buffer(Box::new(old)));
        }
        self.store.quad_count = count;
    }

    /// The draws of this frame. `visible` is the game's visible sections in its order, each `mesh id, x, y, z,
    /// visibility` (i32 i32 i32 i32 f32); `ordered`: translucent sections back to front, as the game draws them
    /// without improved transparency.
    ///
    /// `camera`: where the camera is in the world. Of the solid and cutout layers only the quads that can face it are
    /// drawn: a quad facing east (+x) on the plane x = c shows only from x > c, and the pipelines cull back faces
    /// anyway, so the GPU never gets what it would throw away.
    #[allow(clippy::too_many_arguments)]
    ///
    /// `number`: which of the game's terrain builds this is (they count up from 1), for the meshes it let go of.
    pub fn build(&mut self, feed: &Feed, gpu: &Gpu, enc: &mut Encoder, number: u64, visible: &[u8], ordered: bool, camera: [f64; 3], view_proj: [f32; 16]) {
        super::stats::time(super::stats::TERRAIN_UPLOADS, || self.apply_queue(feed, gpu, enc, number));
        let n = visible.len() / 20;
        let dev_t0 = std::time::Instant::now();
        self.view_proj = view_proj;
        self.impostor.plan(camera, &view_proj);
        let (live_radius, refresh) = (self.impostor.live_radius, self.impostor.refresh);
        let far_from = self.impostor.radius - super::impostor::OVERLAP;
        let mut face_lists: [[Vec<(Group, vk::DrawIndexedIndirectCommand)>; super::impostor::HELD]; super::impostor::FACES] = Default::default();
        let mut farthest = 0f64;
        let read = |i: usize, f: usize| i32::from_le_bytes(visible[i * 20 + f * 4..i * 20 + f * 4 + 4].try_into().unwrap());
        // The section infos: one per drawn section, in the game's order (firstInstance points at it).
        // with faces to draw, room for the infos of meshes out of view too (they are drawn into the faces)
        let slots_max = n as u64 + if refresh != 0 { self.store.summary.len() as u64 } else { 0 };
        let info = enc.transient.mapped(gpu, slots_max.max(1) * SECTION_INFO_SIZE, SECTION_INFO_SIZE, slots_max.max(1) * SECTION_INFO_SIZE, 1);
        let mut infos = 0u32;
        // Each section's mesh id, by its info: what the occlusion state is kept by.
        let mut ids: Vec<u32> = Vec::with_capacity(n);
        let mut max_quads = 0u64;
        let mut lists: [Vec<(Group, vk::DrawIndexedIndirectCommand)>; LAYERS] = Default::default();
        // the translucent draws' sections' distances, entry for entry
        let mut translucent_dist: Vec<f64> = Vec::new();
        let packed_now = feed.packed.load(std::sync::atomic::Ordering::Relaxed);
        let mut holes = 0;
        // dev (BENCH): quads drawn by layer, and of them the unculled ones (not on an axis plane: crosses, slopes)
        let mut dev_quads = [[0u64; 2]; LAYERS];
        // dev (BENCH): quads drawn by the section's distance (<64, 64-128, 128+ blocks) per layer
        let mut dev_far = [[0u64; 3]; LAYERS];
        for i in 0..n {
            let id = read(i, 0) as usize;
            let bits = self.store.summary.get(id).copied().unwrap_or(0);
            if bits == 0 {
                if !matches!(self.store.meshes.get(id), Some(Some(_))) {
                    holes += 1;
                }
                continue;
            }
            let corner = [read(i, 1) as f64, read(i, 2) as f64, read(i, 3) as f64];
            let dist = ((corner[0] + 8.0 - camera[0]).powi(2) + (corner[1] + 8.0 - camera[1]).powi(2) + (corner[2] + 8.0 - camera[2]).powi(2)).sqrt();
            farthest = farthest.max(dist);
            // far off with the faces holding it and none drawn this frame: its opaque layers are not looked at at all
            let held = live_radius.is_some_and(|r| dist >= r) && !(refresh != 0 && dist >= far_from);
            // and with nothing else, the section is not looked at at all (Store::summary)
            if held && bits & (1 << TRANSLUCENT) == 0 {
                continue;
            }
            let Some(Some(mesh)) = self.store.meshes.get(id) else {
                holes += 1;
                continue;
            };
            let at = infos;
            infos += 1;
            ids.push(id as u32);
            unsafe {
                let p = info.host.add(at as usize * SECTION_INFO_SIZE as usize);
                std::ptr::copy_nonoverlapping(visible.as_ptr().add(i * 20 + 4), p, 16);
            }
            if refresh != 0 {
                if self.store.seen.len() <= id {
                    self.store.seen.resize(id + 1, 0);
                }
                self.store.seen[id] = number;
            }
            for (l, layer) in mesh.layers.iter().enumerate() {
                if held && l < super::impostor::HELD {
                    continue;
                }
                let start = lists[l].len();
                let Some(lm) = layer else { continue };
                // Vertices made for the other format (the pipelines changed with a resource reload) wait for their
                // section's new upload rather than being read wrong.
                if lm.index_count == 0 || lm.packed != packed_now {
                    continue;
                }
                if lm.indices.is_none() {
                    max_quads = max_quads.max(lm.index_count as u64 / 6 + 1);
                }
                let group = Group {
                    vertex_block: lm.vertices.block,
                    index_block: lm.indices.map(|p| p.block),
                    index_u32: lm.indices.is_none() || lm.index_u32,
                    commands: 0,
                    count: 0,
                    second: 0,
                };
                let first_index = match lm.indices {
                    Some(p) => (p.offset / if lm.index_u32 { 4 } else { 2 }) as u32,
                    None => 0,
                };
                let cmd = vk::DrawIndexedIndirectCommand {
                    index_count: lm.index_count,
                    instance_count: 1,
                    first_index,
                    vertex_offset: (lm.vertices.offset / if lm.packed { PACKED_SIZE } else { VERTEX_SIZE }) as i32,
                    first_instance: at,
                };
                match lm.facings {
                    Some(f) if lm.indices.is_none() || lm.own => {
                        let o = [read(i, 1) as f64, read(i, 2) as f64, read(i, 3) as f64];
                        let shown = facing_mask(o, camera);
                        if super::stats::passes() {
                            dev_quads[l][1] += f[0] as u64;
                            let drawn = f.iter().enumerate().filter(|(k, _)| shown & (1 << k) != 0).map(|(_, &n)| n as u64).sum::<u64>();
                            dev_quads[l][0] += drawn;
                            let dist = ((o[0] + 8.0 - camera[0]).powi(2) + (o[1] + 8.0 - camera[1]).powi(2) + (o[2] + 8.0 - camera[2]).powi(2)).sqrt();
                            dev_far[l][if dist < 64.0 { 0 } else if dist < 128.0 { 1 } else { 2 }] += drawn;
                        }
                        // Runs of shown facings, each one draw over its quads of the shared quad indices (or of the mesh's own, the same pattern).
                        let mut quad = 0u32;
                        let mut run: Option<(u32, u32)> = None;
                        for (k, &n) in f.iter().enumerate() {
                            if shown & (1 << k) != 0 {
                                run = Some(match run {
                                    Some((s, len)) => (s, len + n),
                                    None => (quad, n),
                                });
                            } else if let Some((s, len)) = run.take() {
                                if len > 0 {
                                    lists[l].push((group, vk::DrawIndexedIndirectCommand { index_count: len * 6, first_index: cmd.first_index + s * 6, ..cmd }));
                                }
                            }
                            quad += n;
                        }
                        if let Some((s, len)) = run {
                            if len > 0 {
                                lists[l].push((group, vk::DrawIndexedIndirectCommand { index_count: len * 6, first_index: cmd.first_index + s * 6, ..cmd }));
                            }
                        }
                    }
                    _ => {
                        if super::stats::passes() {
                            dev_quads[l][0] += lm.index_count as u64 / 6;
                        }
                        lists[l].push((group, cmd))
                    }
                }
                if l == TRANSLUCENT {
                    translucent_dist.resize(lists[l].len(), dist);
                }
                // the opaque layers far off: into the faces drawn this frame, and not live when the faces hold them
                if l < super::impostor::HELD {
                    if refresh != 0 && dist >= far_from {
                        for (face, into) in face_lists.iter_mut().enumerate() {
                            if refresh & (1 << face) != 0 && self.impostor.in_face(face, corner) {
                                into[l].extend_from_slice(&lists[l][start..]);
                                if lists[l].len() > start {
                                    self.impostor.bound(face, corner);
                                }
                            }
                        }
                    }
                    if live_radius.is_some_and(|r| dist >= r) {
                        lists[l].truncate(start);
                    }
                }
            }
        }
        // the faces drawn this frame: the meshes in their reach that were not in view (Store::origins); dev:
        // CURSED_DEV_FACESINVIEW only those in view, as before 12.10
        static IN_VIEW: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        if refresh != 0 && !*IN_VIEW.get_or_init(|| std::env::var_os("CURSED_DEV_FACESINVIEW").is_some()) {
            let mut draws: Vec<(Group, vk::DrawIndexedIndirectCommand)> = Vec::new();
            for id in 0..self.store.summary.len() {
                if self.store.summary[id] & 0b11 == 0 || self.store.seen.get(id) == Some(&number) {
                    continue;
                }
                let Some(&o) = self.store.origins.get(id) else { continue };
                if o[0] == i32::MIN {
                    continue;
                }
                let corner = [o[0] as f64, o[1] as f64, o[2] as f64];
                let dist = ((corner[0] + 8.0 - camera[0]).powi(2) + (corner[1] + 8.0 - camera[1]).powi(2) + (corner[2] + 8.0 - camera[2]).powi(2)).sqrt();
                if dist < far_from {
                    continue;
                }
                let hit = (0..super::impostor::FACES).filter(|&f| refresh & (1 << f) != 0 && self.impostor.in_face(f, corner)).fold(0u32, |m, f| m | 1 << f);
                if hit == 0 {
                    continue;
                }
                let Some(Some(mesh)) = self.store.meshes.get(id) else { continue };
                let at = infos;
                infos += 1;
                ids.push(id as u32);
                unsafe {
                    let p = info.host.add(at as usize * SECTION_INFO_SIZE as usize) as *mut [u8; 16];
                    let mut b = [0u8; 16];
                    for k in 0..3 {
                        b[k * 4..k * 4 + 4].copy_from_slice(&o[k].to_le_bytes());
                    }
                    b[12..16].copy_from_slice(&1.0f32.to_le_bytes());
                    std::ptr::write_unaligned(p, b);
                }
                for l in 0..super::impostor::HELD {
                    let Some(lm) = mesh.layers[l].as_ref() else { continue };
                    if lm.index_count == 0 || lm.packed != packed_now {
                        continue;
                    }
                    draws.clear();
                    max_quads = max_quads.max(layer_draws(lm, at, corner, camera, &mut draws));
                    if draws.is_empty() {
                        continue;
                    }
                    for (face, into) in face_lists.iter_mut().enumerate() {
                        if hit & (1 << face) != 0 {
                            into[l].extend_from_slice(&draws);
                            self.impostor.bound(face, corner);
                        }
                    }
                }
            }
        }
        if holes > 0 && stalls() {
            eprintln!("[stall] t={:.3} terrain build {number}: {holes} sections without their mesh", clock());
        }
        let dev_t1 = std::time::Instant::now();
        self.ensure_quads(gpu, enc, max_quads);
        let total: usize = lists.iter().map(Vec::len).sum();
        // dev (CURSED_DEV_BENCH): what the terrain draws, averaged
        if super::stats::on() {
            static ACC: std::sync::Mutex<(u64, u64, u64, u64, u64)> = std::sync::Mutex::new((0, 0, 0, 0, 0));
            static QUADS: std::sync::Mutex<[[u64; 2]; LAYERS]> = std::sync::Mutex::new([[0; 2]; LAYERS]);
            static FAR: std::sync::Mutex<[[u64; 3]; LAYERS]> = std::sync::Mutex::new([[0; 3]; LAYERS]);
            let mut q = QUADS.lock().unwrap();
            let mut fr = FAR.lock().unwrap();
            for l in 0..LAYERS {
                q[l][0] += dev_quads[l][0];
                q[l][1] += dev_quads[l][1];
                for b in 0..3 {
                    fr[l][b] += dev_far[l][b];
                }
            }
            let mut a = ACC.lock().unwrap();
            let idx: u64 = lists.iter().take(2).flat_map(|l| l.iter()).map(|(_, c)| c.index_count as u64).sum();
            a.0 += 1;
            a.1 += n as u64;
            a.2 += infos as u64;
            a.3 += lists.iter().map(|l| l.len() as u64).sum::<u64>();
            a.4 += idx;
            if a.0 >= 2000 {
                eprintln!("[terrain] per build: {} sections listed, {} drawn, {} draws, {} opaque triangles, packed {}, vertex blocks {}", a.1 / a.0, a.2 / a.0, a.3 / a.0, a.4 / a.0 / 3, packed_now, self.store.vertices.blocks.len());
                eprintln!("[terrain] quads drawn (of them unculled): solid {} ({}), cutout {} ({}), translucent {}", q[0][0] / a.0, q[0][1] / a.0, q[1][0] / a.0, q[1][1] / a.0, q[2][0] / a.0);
                *q = [[0; 2]; LAYERS];
                eprintln!("[terrain] quads by distance (<64, 64-128, 128+): solid {:?}, cutout {:?}, translucent {:?}", fr[0].map(|v| v / a.0), fr[1].map(|v| v / a.0), fr[2].map(|v| v / a.0));
                *fr = [[0; 3]; LAYERS];
                *a = (0, 0, 0, 0, 0);
            }
        }
        // With occlusion culling every opaque command is there twice (first part, second part), the translucent after.
        let dev_t2 = std::time::Instant::now();
        let culling = total > 0 && self.occlusion.as_mut().is_some_and(|o| o.wanted());
        let opaque = (total - lists[TRANSLUCENT].len()) as u64;
        let slots = total as u64 + if culling { opaque } else { 0 };
        self.impostor.farthest = farthest;
        let face_total: u64 = face_lists.iter().flatten().map(|l| l.len() as u64).sum();
        let dev_m0 = std::time::Instant::now();
        let commands = enc.transient.mapped(gpu, (slots + face_total).max(1) * COMMAND_SIZE, 4, (slots + face_total).max(1) * COMMAND_SIZE, 1);
        let dev_mapped = dev_m0.elapsed();
        let mut written = 0u64;
        let write = |slot: u64, c: &vk::DrawIndexedIndirectCommand, instances: u32| {
            let bytes = [c.index_count, instances, c.first_index, c.vertex_offset as u32, c.first_instance];
            unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr() as *const u8, commands.host.add((slot * COMMAND_SIZE) as usize), COMMAND_SIZE as usize) };
        };
        self.frame.far_drawn = false;
        self.frame.far_translucent = 0;
        for (l, list) in lists.iter_mut().enumerate() {
            let translucent_order = l == TRANSLUCENT && ordered;
            // translucent: the sections from FAR_WATER out first, back to front, then the rest (in the game's order)
            let mut far_entries = 0;
            if l == TRANSLUCENT {
                let (mut far, mut near): (Vec<usize>, Vec<usize>) = (0..list.len()).partition(|&i| far_water().is_some_and(|r| translucent_dist[i] >= r));
                // back to front: the game lists sections outwards from the camera (its occlusion graph's search), so
                // reversed (a sort by distance cost some 5 µs a frame for a few hundred, 12.10)
                far.reverse();
                if translucent_order {
                    near.reverse();
                }
                far_entries = far.len();
                let old = std::mem::take(list);
                list.extend(far.iter().chain(near.iter()).map(|&i| old[i]));
            }
            let mut groups: Vec<(Group, Vec<vk::DrawIndexedIndirectCommand>)> = Vec::new();
            let mut far_groups = 0;
            for (k, (g, c)) in list.drain(..).enumerate() {
                let same = |a: &Group| a.vertex_block == g.vertex_block && a.index_block == g.index_block && a.index_u32 == g.index_u32;
                if l == TRANSLUCENT && k == far_entries {
                    far_groups = groups.len();
                }
                let near_from = if l == TRANSLUCENT && k >= far_entries { far_groups } else { 0 };
                // In translucent order (and among the far translucent ones, drawn sorted) only a run of the same
                // buffers may share a draw call: the order is the picture. Far and near never share one.
                let found = if l == TRANSLUCENT && k == far_entries {
                    None
                } else if translucent_order || (l == TRANSLUCENT && k < far_entries) {
                    groups.last_mut().filter(|(a, _)| same(a))
                } else {
                    groups[near_from..].iter_mut().find(|(a, _)| same(a))
                };
                match found {
                    Some((_, cmds)) => cmds.push(c),
                    None => groups.push((g, vec![c])),
                }
            }
            if l == TRANSLUCENT {
                self.frame.far_translucent = if far_entries > 0 && far_groups == 0 { groups.len() } else { far_groups };
            }
            self.frame.groups[l].clear();
            for (mut g, cmds) in groups {
                let shift = if culling && l == TRANSLUCENT { opaque } else { 0 };
                g.commands = commands.offset + (written + shift) * COMMAND_SIZE;
                g.count = cmds.len() as u32;
                g.second = if culling && l != TRANSLUCENT { g.commands + opaque * COMMAND_SIZE } else { 0 };
                for c in &cmds {
                    // The first copies are switched on by last frame's visibility, the second by this frame's test.
                    write(written + shift, c, 1);
                    if culling && l != TRANSLUCENT {
                        write(written + opaque, c, 0);
                    }
                    written += 1;
                }
                self.frame.groups[l].push(g);
            }
        }
        // the faces' draws (vk/impostor.rs), after the frame's own
        let mut at = slots;
        for (face, layers) in face_lists.iter_mut().enumerate() {
            for (l, list) in layers.iter_mut().enumerate() {
                let mut groups: Vec<(Group, Vec<vk::DrawIndexedIndirectCommand>)> = Vec::new();
                for (g, c) in list.drain(..) {
                    let same = |a: &Group| a.vertex_block == g.vertex_block && a.index_block == g.index_block && a.index_u32 == g.index_u32;
                    match groups.iter_mut().find(|(a, _)| same(a)) {
                        Some((_, cmds)) => cmds.push(c),
                        None => groups.push((g, vec![c])),
                    }
                }
                self.face_groups[face][l].clear();
                for (mut g, cmds) in groups {
                    g.commands = commands.offset + at * COMMAND_SIZE;
                    g.count = cmds.len() as u32;
                    g.second = 0;
                    for c in &cmds {
                        write(at, c, 1);
                        at += 1;
                    }
                    self.face_groups[face][l].push(g);
                }
            }
        }
        // dev: where the list's time goes (sections, stats, commands), every 5 s with the FPS stats
        if super::stats::on() {
            static T: std::sync::Mutex<(u64, u128, u128, u128, Option<std::time::Instant>)> = std::sync::Mutex::new((0, 0, 0, 0, None));
            static M: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            M.fetch_add(dev_mapped.as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
            let mut t = T.lock().unwrap();
            let now = std::time::Instant::now();
            t.0 += 1;
            t.1 += (dev_t1 - dev_t0).as_nanos();
            t.2 += (dev_t2 - dev_t1).as_nanos();
            t.3 += (now - dev_t2).as_nanos();
            let since = *t.4.get_or_insert(now);
            if now.duration_since(since).as_secs() >= 5 {
                let k = t.0.max(1) as f64 * 1000.0;
                eprintln!("[terrain] list us: sections {:.1}, quads+stats {:.1}, commands {:.1} (of it the mapping {:.1}); draws {}, groups {:?}", t.1 as f64 / k, t.2 as f64 / k, t.3 as f64 / k, M.swap(0, std::sync::atomic::Ordering::Relaxed) as f64 / k, total, self.frame.groups.iter().map(Vec::len).collect::<Vec<_>>());
                *t = (0, 0, 0, 0, Some(now));
            }
        }
        self.frame.info = Some((info.buffer, info.offset));
        self.frame.commands = Some(commands.buffer);
        if let Some(o) = self.occlusion.as_mut() {
            o.frame = None;
            if culling {
                let piece = enc.transient.mapped(gpu, ids.len().max(1) as u64 * 4, 4, ids.len().max(1) as u64 * 4, 1);
                unsafe { std::ptr::copy_nonoverlapping(ids.as_ptr() as *const u8, piece.host, ids.len() * 4) };
                let frame = super::occlusion::Frame {
                    commands: unsafe { (*commands.buffer).raw },
                    command_base: (commands.offset / 4) as u32,
                    opaque: opaque as u32,
                    translucent: (total as u64 - opaque) as u32,
                    ids: unsafe { (*piece.buffer).raw },
                    id_base: (piece.offset / 4) as u32,
                    infos: unsafe { (*info.buffer).raw },
                    info_base: (info.offset / 4) as u32,
                    view_proj,
                    camera,
                    tested: false,
                };
                o.begin(gpu, enc, frame, self.store.meshes.len() as u32);
            }
        }
    }

    /// Whether the opaque terrain waits for the occlusion test before its second part.
    pub fn wants_test(&self) -> bool {
        self.occlusion.as_ref().and_then(|o| o.frame).is_some_and(|f| !f.tested)
    }

    /// The occlusion test between the two parts of the opaque terrain (the pass is suspended around it).
    pub fn test(&mut self, gpu: &Gpu, enc: &mut Encoder, cmd: vk::CommandBuffer, depth: Option<&super::resources::View>) {
        if let Some(o) = self.occlusion.as_mut() {
            o.test(gpu, enc, cmd, depth);
        }
    }

    /// The second part of an opaque layer: the sections the test found visible that the first part did not draw.
    pub fn draw_second(&self, gpu: &Gpu, cmd: vk::CommandBuffer, layer: usize, max_draws: u32) {
        if !self.occlusion.as_ref().and_then(|o| o.frame).is_some_and(|f| f.tested) {
            return;
        }
        self.draw_groups(gpu, cmd, layer, max_draws, true, Part::All);
    }

    /// Draws one layer in the open pass (the pipeline and its uniforms are already set by the game).
    pub fn draw(&self, gpu: &Gpu, cmd: vk::CommandBuffer, layer: usize, max_draws: u32) {
        self.draw_groups(gpu, cmd, layer, max_draws, false, Part::All);
    }

    /// A part of the translucent layer (FrameDraws::far_translucent).
    pub fn draw_part(&self, gpu: &Gpu, cmd: vk::CommandBuffer, layer: usize, max_draws: u32, part: Part) {
        self.draw_groups(gpu, cmd, layer, max_draws, false, part);
    }

    fn draw_groups(&self, gpu: &Gpu, cmd: vk::CommandBuffer, layer: usize, max_draws: u32, second: bool, part: Part) {
        let (Some((info_buffer, info_offset)), Some(commands)) = (self.frame.info, self.frame.commands) else { return };
        let d = &gpu.device;
        let all = &self.frame.groups[layer];
        let split = self.frame.far_translucent.min(all.len());
        let groups = match part {
            Part::All => &all[..],
            Part::Far => &all[..split],
            Part::Near => &all[split..],
        };
        if groups.is_empty() {
            return;
        }
        unsafe {
            d.cmd_bind_vertex_buffers(cmd, 1, &[(*info_buffer).raw], &[info_offset]);
        }
        for g in groups {
            let vb = self.store.vertices.buffer(g.vertex_block).raw;
            let (ib, kind) = match g.index_block {
                Some(b) => (self.store.indices.buffer(b).raw, if g.index_u32 { vk::IndexType::UINT32 } else { vk::IndexType::UINT16 }),
                None => (self.store.quad_indices.as_ref().expect("quad indices").raw, vk::IndexType::UINT32),
            };
            unsafe {
                d.cmd_bind_vertex_buffers(cmd, 0, &[vb], &[0]);
                d.cmd_bind_index_buffer(cmd, ib, 0, kind);
                let mut left = g.count;
                let mut offset = if second { g.second } else { g.commands };
                if second && offset == 0 {
                    continue;
                }
                while left > 0 {
                    let n = left.min(max_draws.max(1));
                    d.cmd_draw_indexed_indirect(cmd, (*commands).raw, offset, n, COMMAND_SIZE as u32);
                    left -= n;
                    offset += n as u64 * COMMAND_SIZE;
                }
            }
        }
    }

    /// Draws the far terrain's faces due this frame (vk/impostor.rs), after the pass that drew the terrain: the game's
    /// pipelines and uniforms of its opaque layers as that pass had them, with each face's projection and view.
    pub fn refresh_impostor(&mut self, gpu: &Gpu, enc: &mut Encoder) {
        let faces = self.impostor.prepare(gpu, enc);
        if faces == 0 {
            self.impostor.captured = [None, None];
            return;
        }
        let max = gpu.info.max_draw_indirect as u32;
        let cmd = enc.cmd(gpu);
        enc.pass_started(gpu, cmd, "Far terrain faces");
        let uniforms = self.impostor.face_uniforms(gpu, enc, cmd, faces);
        for face in 0..super::impostor::FACES {
            if faces & (1 << face) == 0 {
                continue;
            }
            self.impostor.begin_face(gpu, cmd, face);
            for l in 0..super::impostor::HELD {
                let values = &uniforms[face][l];
                let p = unsafe { &*self.impostor.captured[l].as_ref().unwrap().pipeline };
                unsafe { gpu.device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, p.handle(true, true)) };
                if !super::pass::push_values(gpu, cmd, p, values) {
                    continue;
                }
                self.draw_face(gpu, cmd, face, l, max);
            }
            self.impostor.end_face(gpu, cmd, face);
        }
        self.impostor.finish(gpu, cmd);
        enc.pass_ended(gpu, cmd);
        self.impostor.captured = [None, None];
    }

    fn draw_face(&self, gpu: &Gpu, cmd: vk::CommandBuffer, face: usize, layer: usize, max_draws: u32) {
        let (Some((info_buffer, info_offset)), Some(commands)) = (self.frame.info, self.frame.commands) else { return };
        let d = &gpu.device;
        unsafe { d.cmd_bind_vertex_buffers(cmd, 1, &[(*info_buffer).raw], &[info_offset]) };
        for g in &self.face_groups[face][layer] {
            let vb = self.store.vertices.buffer(g.vertex_block).raw;
            let (ib, kind) = match g.index_block {
                Some(b) => (self.store.indices.buffer(b).raw, if g.index_u32 { vk::IndexType::UINT32 } else { vk::IndexType::UINT16 }),
                None => (self.store.quad_indices.as_ref().expect("quad indices").raw, vk::IndexType::UINT32),
            };
            unsafe {
                d.cmd_bind_vertex_buffers(cmd, 0, &[vb], &[0]);
                d.cmd_bind_index_buffer(cmd, ib, 0, kind);
                let mut left = g.count;
                let mut offset = g.commands;
                while left > 0 {
                    let n = left.min(max_draws.max(1));
                    d.cmd_draw_indexed_indirect(cmd, (*commands).raw, offset, n, COMMAND_SIZE as u32);
                    left -= n;
                    offset += n as u64 * COMMAND_SIZE;
                }
            }
        }
    }

    pub fn destroy(&mut self, gpu: &Gpu) {
        self.impostor.destroy(gpu);
        if let Some(mut o) = self.occlusion.take() {
            o.destroy(gpu);
        }
        self.store.vertices.destroy(gpu);
        self.store.indices.destroy(gpu);
        if let Some(b) = self.store.quad_indices.take() {
            Doomed::Buffer(Box::new(b)).destroy(gpu);
        }
    }
}

/// The draws of one layer of a mesh whose section info is `at` (origin `origin`): the runs of its facings seen from
/// `camera`, or the whole of it. Returns the quads the shared quad indices must reach (0 with indices of its own).
fn layer_draws(lm: &LayerMesh, at: u32, origin: [f64; 3], camera: [f64; 3], out: &mut Vec<(Group, vk::DrawIndexedIndirectCommand)>) -> u64 {
    let group = Group { vertex_block: lm.vertices.block, index_block: lm.indices.map(|p| p.block), index_u32: lm.indices.is_none() || lm.index_u32, commands: 0, count: 0, second: 0 };
    let first_index = match lm.indices {
        Some(p) => (p.offset / if lm.index_u32 { 4 } else { 2 }) as u32,
        None => 0,
    };
    let cmd = vk::DrawIndexedIndirectCommand {
        index_count: lm.index_count,
        instance_count: 1,
        first_index,
        vertex_offset: (lm.vertices.offset / if lm.packed { PACKED_SIZE } else { VERTEX_SIZE }) as i32,
        first_instance: at,
    };
    match lm.facings {
        Some(f) if lm.indices.is_none() || lm.own => {
            let shown = facing_mask(origin, camera);
            let mut quad = 0u32;
            let mut run: Option<(u32, u32)> = None;
            for (k, &n) in f.iter().enumerate() {
                if shown & (1 << k) != 0 {
                    run = Some(match run {
                        Some((s, len)) => (s, len + n),
                        None => (quad, n),
                    });
                } else if let Some((s, len)) = run.take() {
                    if len > 0 {
                        out.push((group, vk::DrawIndexedIndirectCommand { index_count: len * 6, first_index: cmd.first_index + s * 6, ..cmd }));
                    }
                }
                quad += n;
            }
            if let Some((s, len)) = run {
                if len > 0 {
                    out.push((group, vk::DrawIndexedIndirectCommand { index_count: len * 6, first_index: cmd.first_index + s * 6, ..cmd }));
                }
            }
        }
        _ => out.push((group, cmd)),
    }
    if lm.indices.is_none() {
        lm.index_count as u64 / 6 + 1
    } else {
        0
    }
}
