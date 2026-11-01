//! Entity models' vertices, as ModelPart.Cube.compile makes them — bit for bit (CURSED_DEV_MODELCHECK compares).
//!
//! A model part's cubes are fixed once baked, so Java hands each part's polygons over once (`add_part`). When the game
//! renders a model, Java walks the part tree as it does (the pose stack, visibility), writes one record per drawn part
//! — the part, its pose and normal matrices — and this writes every vertex of the model in one call, straight into the
//! game's BufferBuilder (DefaultVertexFormat.ENTITY, 36 bytes).
//!
//! The arithmetic is JOML's without fused multiply-add (joml.useMathFma is off): Matrix4f.transformPosition is
//! m00·x + (m10·y + (m20·z + m30)), with JOML's shortcuts for identity and translation matrices; Matrix3f.transform
//! the same without the translation; Vector3f.normalize multiplies by 1 / (float) sqrt((double) length²).

use std::sync::RwLock;

pub const VERTEX: usize = 36;
/// i32 part, i32 matrix properties, i32 trusted normals, f32 the bend at a cut half's cut (radians, Living Motion's
/// mitred joints: its marked corners turn by it), f32 × 16 pose (column-major), f32 × 9 normal matrix.
pub const RECORD: usize = 116;

const PROPERTY_IDENTITY: i32 = 4;
const PROPERTY_TRANSLATION: i32 = 8;

struct Polygon {
    normal: [f32; 3],
    /// Vertex.worldX/Y/Z (x / 16), u, v.
    vertices: [[f32; 5]; 4],
    /// Each corner on a cut half's cut, and the depth of the line it turns about there (/ 16): none, or (true, z).
    bent: [Option<f32>; 4],
}

/// A mesh vertex's info word for a corner on a cut half's cut (parts only; items keep theirs): bit 31, the line's depth
/// in pixels + 128 in bits 16-23.
pub const BENT: u32 = 1 << 31;

/// A corner (x y z, / 16) turned `bend` radians about the line along x at its own height, `depth` deep (/ 16): as the
/// part's xRot turns it (JOML's rotateX).
#[inline]
pub fn bent(v: [f32; 3], depth: f32, bend: f32) -> [f32; 3] {
    let (s, c) = bend.sin_cos();
    let dz = v[2] - depth;
    [v[0], v[1] - s * dz, depth + c * dz]
}

static PARTS: RwLock<Vec<Vec<Polygon>>> = RwLock::new(Vec::new());

/// What the GPU draws from (vk/models.rs, shaders/models.comp): model parts' vertices and items' quads, per vertex
/// x y z u v, the face's normal x y z (f32 bits) and an info word (items: bits 0-3 light emission, bit 4 tinted, bits
/// 8-15 tint index, 255 for an index no submit's first two layers have). `first` is each part's first vertex. Only ever
/// appended to, so the frame thread uploads what is new.
pub struct GpuMesh {
    pub data: Vec<[u32; MESH_WORDS]>,
    pub first: Vec<u32>,
}

pub const MESH_WORDS: usize = 9;

pub static GPU_MESH: RwLock<GpuMesh> = RwLock::new(GpuMesh { data: Vec::new(), first: Vec::new() });

/// The part's first vertex in GPU_MESH.
pub fn part_first(id: u32) -> u32 {
    GPU_MESH.read().unwrap().first[id as usize]
}

/// A part's polygons: i32 count, then per polygon f32 × 3 normal and 4 × (x y z u v); a cut limb's half (Living
/// Motion's joints) then f32 × 2: its cut's height and the line's depth (pixels), its corners there marked. Returns its
/// id.
pub fn add_part(bytes: &[u8]) -> u32 {
    let f = |i: usize| f32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap());
    let n = i32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
    let size = 4 + n * 23 * 4;
    assert!(bytes.len() == size || bytes.len() == size + 8, "model part size");
    let hinge = (bytes.len() == size + 8).then(|| (f(1 + n * 23) / 16.0, f(2 + n * 23) / 16.0));
    let mut polygons = Vec::with_capacity(n);
    for p in 0..n {
        let b = 1 + p * 23;
        let mut vertices = [[0f32; 5]; 4];
        for (v, out) in vertices.iter_mut().enumerate() {
            let at = b + 3 + v * 5;
            *out = [f(at) / 16.0, f(at + 1) / 16.0, f(at + 2) / 16.0, f(at + 3), f(at + 4)];
        }
        let bent = std::array::from_fn(|k| hinge.filter(|h| (vertices[k][1] - h.0).abs() < 1e-4).map(|h| h.1));
        polygons.push(Polygon { normal: [f(b), f(b + 1), f(b + 2)], vertices, bent });
    }
    let mut parts = PARTS.write().unwrap();
    let mut mesh = GPU_MESH.write().unwrap();
    let first = mesh.data.len() as u32;
    mesh.first.push(first);
    for p in &polygons {
        for (v, b) in p.vertices.iter().zip(p.bent) {
            let w = [v[0], v[1], v[2], v[3], v[4], p.normal[0], p.normal[1], p.normal[2]];
            let info = b.map_or(0, |z| BENT | (((z * 16.0).round() as i32 + 128).clamp(0, 255) as u32) << 16);
            mesh.data.push([w[0].to_bits(), w[1].to_bits(), w[2].to_bits(), w[3].to_bits(), w[4].to_bits(), w[5].to_bits(), w[6].to_bits(), w[7].to_bits(), info]);
        }
    }
    parts.push(polygons);
    (parts.len() - 1) as u32
}

/// Items' quads for the GPU, `count` of them: per quad 4 × (f32 x y z, u32 u, u32 v — the packed UV's halves), f32 × 3
/// its face's unit normal, u32 info. Returns the first quad's first vertex in GPU_MESH (the rest follow, 4 per quad).
pub const ITEM_QUAD: usize = 24 * 4;

pub fn add_quads(bytes: &[u8], count: usize) -> u32 {
    assert_eq!(bytes.len(), count * ITEM_QUAD, "item quads size");
    let w = |i: usize| u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap());
    let mut mesh = GPU_MESH.write().unwrap();
    let first = mesh.data.len() as u32;
    for q in 0..count {
        let b = q * 24;
        for j in 0..4 {
            let at = b + j * 5;
            mesh.data.push([w(at), w(at + 1), w(at + 2), w(at + 3), w(at + 4), w(b + 20), w(b + 21), w(b + 22), w(b + 23)]);
        }
    }
    first
}

#[inline]
fn normal_byte(c: f32) -> u8 {
    // BufferBuilder.normalIntValue: (byte) ((int) (Mth.clamp(c, -1, 1) * 127) & 0xFF).
    let c = if c < -1.0 { -1.0 } else { c.min(1.0) };
    ((c * 127.0) as i32 & 0xFF) as u8
}

#[inline]
fn to_abgr(color: i32) -> u32 {
    ((color & 0xFF00FF00u32 as i32) | ((color >> 16) & 0xFF) | ((color & 0xFF) << 16)) as u32
}

/// Writes the vertices of `count` records to `dst`; returns how many vertices were written.
///
/// # Safety
/// `dst` must have room for every vertex of the records' parts.
pub unsafe fn vertices(records: &[u8], count: usize, light: i32, overlay: i32, color: i32, dst: *mut u8) -> usize {
    let parts = PARTS.read().unwrap();
    let abgr = to_abgr(color).to_le_bytes();
    let overlay = overlay.to_le_bytes();
    let light = light.to_le_bytes();
    let mut written = 0usize;
    for r in 0..count {
        let rec = &records[r * RECORD..(r + 1) * RECORD];
        let i = |k: usize| i32::from_le_bytes(rec[k * 4..k * 4 + 4].try_into().unwrap());
        let f = |k: usize| f32::from_le_bytes(rec[k * 4..k * 4 + 4].try_into().unwrap());
        let part = &parts[i(0) as usize];
        let props = i(1);
        let trusted = i(2) != 0;
        let bend = f(3);
        let m: [f32; 16] = std::array::from_fn(|k| f(4 + k));
        let nm: [f32; 9] = std::array::from_fn(|k| f(20 + k));
        for poly in part {
            let [x, y, z] = poly.normal;
            let mut n = [
                nm[0] * x + (nm[3] * y + nm[6] * z),
                nm[1] * x + (nm[4] * y + nm[7] * z),
                nm[2] * x + (nm[5] * y + nm[8] * z),
            ];
            if !trusted {
                let len2 = n[0] * n[0] + (n[1] * n[1] + n[2] * n[2]);
                let inv = 1.0f32 / ((len2 as f64).sqrt() as f32);
                n = [n[0] * inv, n[1] * inv, n[2] * inv];
            }
            let nb = [normal_byte(n[0]), normal_byte(n[1]), normal_byte(n[2]), 0];
            for (v, b) in poly.vertices.iter().zip(poly.bent) {
                let (px, py, pz) = match b {
                    Some(depth) if bend != 0.0 => {
                        let q = bent([v[0], v[1], v[2]], depth, bend);
                        (q[0], q[1], q[2])
                    }
                    _ => (v[0], v[1], v[2]),
                };
                let p = if props & PROPERTY_IDENTITY != 0 {
                    [px, py, pz]
                } else if props & PROPERTY_TRANSLATION != 0 {
                    [px + m[12], py + m[13], pz + m[14]]
                } else {
                    [
                        m[0] * px + (m[4] * py + (m[8] * pz + m[12])),
                        m[1] * px + (m[5] * py + (m[9] * pz + m[13])),
                        m[2] * px + (m[6] * py + (m[10] * pz + m[14])),
                    ]
                };
                let mut out = [0u8; VERTEX];
                out[0..4].copy_from_slice(&p[0].to_le_bytes());
                out[4..8].copy_from_slice(&p[1].to_le_bytes());
                out[8..12].copy_from_slice(&p[2].to_le_bytes());
                out[12..16].copy_from_slice(&abgr);
                out[16..20].copy_from_slice(&v[3].to_le_bytes());
                out[20..24].copy_from_slice(&v[4].to_le_bytes());
                out[24..28].copy_from_slice(&overlay);
                out[28..32].copy_from_slice(&light);
                out[32..36].copy_from_slice(&nb);
                std::ptr::copy_nonoverlapping(out.as_ptr(), dst.add(written * VERTEX), VERTEX);
                written += 1;
            }
        }
    }
    written
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_bytes_match_java() {
        // (byte) ((int) (c * 127) & 0xFF) for a few values, computed by hand with Java's truncation.
        assert_eq!(normal_byte(1.0), 127);
        assert_eq!(normal_byte(-1.0), 0x81);
        assert_eq!(normal_byte(0.5), 63);
        assert_eq!(normal_byte(-0.5), 0xC1);
        assert_eq!(normal_byte(2.0), 127);
        assert_eq!(normal_byte(-0.0), 0);
    }

    /// A bend mitred: the upper half's corners on the cut turned by half the bend, the lower half's back by half in its
    /// own frame (which the whole bend turned) — they land on the same place, the plane half way round.
    #[test]
    fn mitred_halves_meet() {
        let bend = 1.4f32;
        let (cut, depth) = (4.0 / 16.0, 0.0);
        for z in [-2.0f32 / 16.0, 0.0, 2.25 / 16.0] {
            // the upper half's corner, in the arm's frame
            let up = bent([0.1, cut, z], depth, bend * 0.5);
            // the lower half's corner, in the joint's frame (at the cut), then the joint's turn into the arm's frame
            let low = bent([0.1, 0.0, z], depth, -bend * 0.5);
            let (s, c) = bend.sin_cos();
            let low = [low[0], cut + (c * low[1] - s * low[2]), s * low[1] + c * low[2]];
            for k in 0..3 {
                assert!((up[k] - low[k]).abs() < 1e-5, "{z}: {up:?} {low:?}");
            }
        }
        // a part handed over with its cut: the corners there marked, the others not
        let mut bytes = 1i32.to_le_bytes().to_vec();
        for v in [0.0f32, 0.0, -1.0] {
            bytes.extend(v.to_le_bytes());
        }
        for (y, z) in [(4.0f32, -2.0f32), (4.0, 2.0), (-2.0, 2.0), (-2.0, -2.0)] {
            for v in [1.0f32, y, z, 0.0, 0.0] {
                bytes.extend(v.to_le_bytes());
            }
        }
        bytes.extend(4.0f32.to_le_bytes());
        bytes.extend(0.0f32.to_le_bytes());
        let id = add_part(&bytes);
        let marked: Vec<bool> = PARTS.read().unwrap()[id as usize][0].bent.iter().map(Option::is_some).collect();
        assert_eq!(marked, vec![true, true, false, false]);
        let first = part_first(id) as usize;
        assert_ne!(GPU_MESH.read().unwrap().data[first][8] & BENT, 0);
    }
}

// ---- item quads (VertexConsumer.putBakedQuad) ------------------------------------------------------------------------

/// A run's header: i32 matrix properties, i32 trusted normals, i32 light, i32 overlay, f32 × 16 pose, f32 × 9 normal.
pub const QUAD_HEADER: usize = 116;
/// A quad: f32 × 12 positions, u64 × 4 packed UVs, f32 × 3 its face's unit normal, i32 light emission, i32 color.
pub const QUAD: usize = 100;

/// putBakedQuad for `count` quads that share a pose, light and overlay; returns the vertices written (4 per quad).
///
/// # Safety
/// `dst` must have room for 4 × `count` vertices.
pub unsafe fn quads(header: &[u8], quads: &[u8], count: usize, dst: *mut u8) -> usize {
    let i = |k: usize| i32::from_le_bytes(header[k * 4..k * 4 + 4].try_into().unwrap());
    let f = |k: usize| f32::from_le_bytes(header[k * 4..k * 4 + 4].try_into().unwrap());
    let props = i(0);
    let trusted = i(1) != 0;
    let light = i(2);
    let overlay = i(3).to_le_bytes();
    let m: [f32; 16] = std::array::from_fn(|k| f(4 + k));
    let nm: [f32; 9] = std::array::from_fn(|k| f(20 + k));
    let mut written = 0usize;
    for q in 0..count {
        let rec = &quads[q * QUAD..(q + 1) * QUAD];
        let qf = |k: usize| f32::from_le_bytes(rec[k * 4..k * 4 + 4].try_into().unwrap());
        let qi = |k: usize| i32::from_le_bytes(rec[k * 4..k * 4 + 4].try_into().unwrap());
        let [x, y, z] = [qf(20), qf(21), qf(22)];
        let mut n = [
            nm[0] * x + (nm[3] * y + nm[6] * z),
            nm[1] * x + (nm[4] * y + nm[7] * z),
            nm[2] * x + (nm[5] * y + nm[8] * z),
        ];
        if !trusted {
            let len2 = n[0] * n[0] + (n[1] * n[1] + n[2] * n[2]);
            let inv = 1.0f32 / ((len2 as f64).sqrt() as f32);
            n = [n[0] * inv, n[1] * inv, n[2] * inv];
        }
        let nb = [normal_byte(n[0]), normal_byte(n[1]), normal_byte(n[2]), 0];
        let emission = qi(23);
        let abgr = to_abgr(qi(24)).to_le_bytes();
        // QuadInstance.getLightCoordsWithEmission: LightCoordsUtil.lightCoordsWithEmission.
        let light = if emission == 0 {
            light
        } else {
            let sky = ((light >> 20) & 15).max(emission);
            let block = ((light >> 4) & 15).max(emission);
            (block << 4) | (sky << 20)
        }
        .to_le_bytes();
        for j in 0..4 {
            let (px, py, pz) = (qf(j * 3), qf(j * 3 + 1), qf(j * 3 + 2));
            let p = if props & PROPERTY_IDENTITY != 0 {
                [px, py, pz]
            } else if props & PROPERTY_TRANSLATION != 0 {
                [px + m[12], py + m[13], pz + m[14]]
            } else {
                [
                    m[0] * px + (m[4] * py + (m[8] * pz + m[12])),
                    m[1] * px + (m[5] * py + (m[9] * pz + m[13])),
                    m[2] * px + (m[6] * py + (m[10] * pz + m[14])),
                ]
            };
            let uv = u64::from_le_bytes(rec[48 + j * 8..56 + j * 8].try_into().unwrap());
            let mut out = [0u8; VERTEX];
            out[0..4].copy_from_slice(&p[0].to_le_bytes());
            out[4..8].copy_from_slice(&p[1].to_le_bytes());
            out[8..12].copy_from_slice(&p[2].to_le_bytes());
            out[12..16].copy_from_slice(&abgr);
            out[16..20].copy_from_slice(&((uv >> 32) as u32).to_le_bytes());
            out[20..24].copy_from_slice(&(uv as u32).to_le_bytes());
            out[24..28].copy_from_slice(&overlay);
            out[28..32].copy_from_slice(&light);
            out[32..36].copy_from_slice(&nb);
            std::ptr::copy_nonoverlapping(out.as_ptr(), dst.add(written * VERTEX), VERTEX);
            written += 1;
        }
    }
    written
}
