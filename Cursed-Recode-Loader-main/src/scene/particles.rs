//! Particles' quads, as QuadParticleRenderState.renderRotatedQuad makes them — bit for bit (CURSED_DEV_MODELCHECK).
//!
//! The game keeps a layer's particles in two arrays (12 floats: x y z, the rotation quaternion x y z w, scale, u0 u1
//! v0 v1; 2 ints: color, light); this reads them as they are and writes the layer's vertices (DefaultVertexFormat.PARTICLE:
//! position, UV0, color, UV2 — 28 bytes) into the game's BufferBuilder. Each corner is (±1, ±1, 0) rotated by the
//! quaternion (JOML Quaternionf.transform, no fused multiply-add), times the scale, plus the position.

pub const VERTEX: usize = 28;

#[inline]
fn to_abgr(color: i32) -> u32 {
    ((color & 0xFF00FF00u32 as i32) | ((color >> 16) & 0xFF) | ((color & 0xFF) << 16)) as u32
}

/// Quaternionf.transform(x, y, z) for the quaternion (qx, qy, qz, qw).
#[inline]
fn rotate(q: [f32; 4], x: f32, y: f32, z: f32) -> [f32; 3] {
    let [qx, qy, qz, qw] = q;
    let xx = qx * qx;
    let yy = qy * qy;
    let zz = qz * qz;
    let ww = qw * qw;
    let xy = qx * qy;
    let xz = qx * qz;
    let yz = qy * qz;
    let xw = qx * qw;
    let zw = qz * qw;
    let yw = qy * qw;
    let k = 1.0f32 / (xx + yy + zz + ww);
    [
        (xx - yy - zz + ww) * k * x + (2.0 * (xy - zw) * k * y + 2.0 * (xz + yw) * k * z),
        2.0 * (xy + zw) * k * x + ((yy - xx - zz + ww) * k * y + 2.0 * (yz - xw) * k * z),
        2.0 * (xz - yw) * k * x + (2.0 * (yz + xw) * k * y + (zz - xx - yy + ww) * k * z),
    ]
}

/// Writes `count` particles' vertices (4 each) to `dst`; returns the vertices written.
///
/// # Safety
/// `dst` must have room for 4 × `count` vertices.
pub unsafe fn quads(floats: &[f32], ints: &[i32], count: usize, dst: *mut u8) -> usize {
    let mut written = 0;
    for i in 0..count {
        let f = &floats[i * 12..i * 12 + 12];
        let (x, y, z) = (f[0], f[1], f[2]);
        let q = [f[3], f[4], f[5], f[6]];
        let scale = f[7];
        let (u0, u1, v0, v1) = (f[8], f[9], f[10], f[11]);
        let color = to_abgr(ints[i * 2]).to_le_bytes();
        let light = ints[i * 2 + 1].to_le_bytes();
        for (nx, ny, u, v) in [(1.0f32, -1.0f32, u1, v1), (1.0, 1.0, u1, v0), (-1.0, 1.0, u0, v0), (-1.0, -1.0, u0, v1)] {
            let r = rotate(q, nx, ny, 0.0);
            let p = [r[0] * scale + x, r[1] * scale + y, r[2] * scale + z];
            let mut out = [0u8; VERTEX];
            out[0..4].copy_from_slice(&p[0].to_le_bytes());
            out[4..8].copy_from_slice(&p[1].to_le_bytes());
            out[8..12].copy_from_slice(&p[2].to_le_bytes());
            out[12..16].copy_from_slice(&u.to_le_bytes());
            out[16..20].copy_from_slice(&v.to_le_bytes());
            out[20..24].copy_from_slice(&color);
            out[24..28].copy_from_slice(&light);
            std::ptr::copy_nonoverlapping(out.as_ptr(), dst.add(written * VERTEX), VERTEX);
            written += 1;
        }
    }
    written
}
