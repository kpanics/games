//! An entity model's part tree walked natively for the GPU's vertices (cursed.recode.scene.NativeModels): what
//! ModelPart.render does with the pose stack — translateAndRotate per part, visibility, skipDraw — and one job per
//! drawn part in ModelJobs' layout (shaders/models.comp makes the vertices). Java lays the tree out once (pre-order:
//! each node's subtree size, its vertices in the mesh) and each frame only the parts' numbers.
//!
//! The pose follows PoseStack.Pose: translate by the part's offset / 16, rotate by the Euler angles in the order Z Y X
//! (Quaternionf.rotationZYX), scale; the normal matrix rotates with it and takes the scale's signs. A scale that is
//! not the same on all three axes makes the normals untrusted: then nothing is written and the model goes the CPU way.

/// Static, per node: subtree size, first vertex in the mesh, vertices, part id (4 ints).
pub const STATIC: usize = 16;
/// Per frame, per node: flags (1 visible, 2 skipDraw; bits 16-31 a cut limb's half's bend at its cut, radians × 8192,
/// signed), x y z, xRot yRot zRot, xScale yScale zScale.
pub const DYNAMIC: usize = 40;
/// The root's pose: the Matrix4f (16 floats, column by column) then the Matrix3f (9).
pub const ROOT: usize = 100;
pub const JOB: usize = 128;
const VERTEX: i32 = 36;

type M4 = [[f32; 4]; 4]; // [column][row], as JOML's mCR
type M3 = [[f32; 3]; 3];

#[inline]
fn rd_i32(p: *const u8, at: usize) -> i32 {
    unsafe { (p.add(at) as *const i32).read_unaligned() }
}
#[inline]
fn rd_f32(p: *const u8, at: usize) -> f32 {
    unsafe { (p.add(at) as *const f32).read_unaligned() }
}
#[inline]
fn wr_f32(p: *mut u8, at: usize, v: f32) {
    unsafe { (p.add(at) as *mut f32).write_unaligned(v) }
}
#[inline]
fn wr_i32(p: *mut u8, at: usize, v: i32) {
    unsafe { (p.add(at) as *mut i32).write_unaligned(v) }
}

/// Matrix4f.translate (JOML's fma without fusing: a * b + c).
fn translate(m: &mut M4, x: f32, y: f32, z: f32) {
    for r in 0..4 {
        m[3][r] = m[0][r] * x + (m[1][r] * y + (m[2][r] * z + m[3][r]));
    }
}

/// The rotation Rz * Ry * Rx (Quaternionf.rotationZYX(z, y, x)) as columns.
fn zyx(z: f32, y: f32, x: f32) -> M3 {
    let (sx, cx) = x.sin_cos();
    let (sy, cy) = y.sin_cos();
    let (sz, cz) = z.sin_cos();
    [
        [cy * cz, cy * sz, -sy],
        [sx * sy * cz - cx * sz, sx * sy * sz + cx * cz, sx * cy],
        [cx * sy * cz + sx * sz, cx * sy * sz - sx * cz, cx * cy],
    ]
}

fn rotate4(m: &mut M4, r: &M3) {
    let old = *m;
    for j in 0..3 {
        for row in 0..4 {
            m[j][row] = old[0][row] * r[j][0] + old[1][row] * r[j][1] + old[2][row] * r[j][2];
        }
    }
}

fn rotate3(m: &mut M3, r: &M3) {
    let old = *m;
    for j in 0..3 {
        for row in 0..3 {
            m[j][row] = old[0][row] * r[j][0] + old[1][row] * r[j][1] + old[2][row] * r[j][2];
        }
    }
}

thread_local! {
    /// The last CPU walk's parts as models.rs records (walk_cpu, then write_cpu writes their vertices).
    static CPU: std::cell::RefCell<(Vec<u8>, Vec<u8>, usize)> = const { std::cell::RefCell::new((Vec::new(), Vec::new(), 0)) };
}

/// The walk for a model drawn with the CPU's vertices (a sorted, translucent draw): the same poses as for the GPU's
/// jobs, kept as records of scene/models.rs for `write_cpu`. Returns (vertices, untrusted).
///
/// # Safety
/// As `walk`.
pub unsafe fn walk_cpu(statics: *const u8, dynamic: *const u8, n: usize, root: *const u8) -> (i32, bool) {
    CPU.with(|c| {
        let (jobs, records, count) = &mut *c.borrow_mut();
        jobs.clear();
        jobs.resize(n.max(1) * JOB, 0);
        let (written, vertices, untrusted) = walk(statics, dynamic, n, root, jobs.as_mut_ptr(), 0, 0, 0, 0);
        if untrusted {
            *count = 0;
            return (0, true);
        }
        records.clear();
        records.resize(written.max(1) * super::models::RECORD, 0);
        for k in 0..written {
            let j = jobs.as_ptr().add(k * JOB);
            let r = records.as_mut_ptr().add(k * super::models::RECORD);
            wr_i32(r, 0, rd_i32(j, 112));
            wr_i32(r, 4, 0); // a general matrix: no identity or translation shortcuts
            wr_i32(r, 8, 1); // trusted normals (an untrusted pose stopped the walk)
            wr_i32(r, 12, rd_i32(j, 124));
            // the pose's four columns (rows 0-2 from the job, row 3 of an affine matrix), then the normal matrix
            for col in 0..4 {
                for row in 0..3 {
                    wr_f32(r, 16 + (col * 4 + row) * 4, rd_f32(j, (col * 3 + row) * 4));
                }
                wr_f32(r, 16 + (col * 4 + 3) * 4, if col == 3 { 1.0 } else { 0.0 });
            }
            for k2 in 0..9 {
                wr_f32(r, 80 + k2 * 4, rd_f32(j, 48 + k2 * 4));
            }
        }
        *count = written;
        (vertices, false)
    })
}

/// The vertices of the last `walk_cpu` into `dst`; returns how many.
///
/// # Safety
/// `dst` must have room for the vertices `walk_cpu` returned.
pub unsafe fn write_cpu(light: i32, overlay: i32, color: i32, dst: *mut u8) -> usize {
    CPU.with(|c| {
        let (_, records, count) = &*c.borrow();
        super::models::vertices(records, *count, light, overlay, color, dst)
    })
}

/// Writes the jobs of the drawn parts to `jobs`; returns (jobs, vertices, untrusted).
///
/// # Safety
/// The pointers must hold `n` nodes, the root and room for a job per drawn node.
#[allow(clippy::too_many_arguments)]
pub unsafe fn walk(statics: *const u8, dynamic: *const u8, n: usize, root: *const u8, jobs: *mut u8, abgr: i32, overlay: i32, light: i32, argb: i32) -> (usize, i32, bool) {
    let mut m0: M4 = [[0.0; 4]; 4];
    for c in 0..4 {
        for r in 0..4 {
            m0[c][r] = rd_f32(root, (c * 4 + r) * 4);
        }
    }
    let mut n0: M3 = [[0.0; 3]; 3];
    for c in 0..3 {
        for r in 0..3 {
            n0[c][r] = rd_f32(root, 64 + (c * 3 + r) * 4);
        }
    }
    // The pose of each open ancestor: index by depth; `ends[d]` is where the node at depth d's subtree ends.
    let mut poses: Vec<(M4, M3)> = Vec::with_capacity(16);
    let mut ends: Vec<usize> = Vec::with_capacity(16);
    let mut written = 0usize;
    let mut vertices = 0i32;
    let mut i = 0usize;
    while i < n {
        while let Some(&e) = ends.last() {
            if i >= e {
                ends.pop();
                poses.pop();
            } else {
                break;
            }
        }
        let s = statics.add(i * STATIC);
        let subtree = rd_i32(s, 0).max(1) as usize;
        let d = dynamic.add(i * DYNAMIC);
        let flags = rd_i32(d, 0);
        if flags & 1 == 0 {
            // Not visible: neither it nor anything under it.
            i += subtree;
            continue;
        }
        let (mut m, mut nm) = match poses.last() {
            Some(p) => *p,
            None => (m0, n0),
        };
        let (x, y, z) = (rd_f32(d, 4), rd_f32(d, 8), rd_f32(d, 12));
        translate(&mut m, x / 16.0, y / 16.0, z / 16.0);
        let (rx, ry, rz) = (rd_f32(d, 16), rd_f32(d, 20), rd_f32(d, 24));
        if rx != 0.0 || ry != 0.0 || rz != 0.0 {
            let r = zyx(rz, ry, rx);
            rotate4(&mut m, &r);
            rotate3(&mut nm, &r);
        }
        let (sx, sy, sz) = (rd_f32(d, 28), rd_f32(d, 32), rd_f32(d, 36));
        if sx != 1.0 || sy != 1.0 || sz != 1.0 {
            for r in 0..4 {
                m[0][r] *= sx;
                m[1][r] *= sy;
                m[2][r] *= sz;
            }
            if sx.abs() == sy.abs() && sy.abs() == sz.abs() {
                if sx < 0.0 || sy < 0.0 || sz < 0.0 {
                    for r in 0..3 {
                        nm[0][r] *= sx.signum();
                        nm[1][r] *= sy.signum();
                        nm[2][r] *= sz.signum();
                    }
                }
            } else {
                return (0, 0, true);
            }
        }
        let count = rd_i32(s, 8);
        if flags & 2 == 0 && count > 0 {
            let j = jobs.add(written * JOB);
            let cols: [usize; 4] = [0, 1, 2, 3];
            let mut at = 0;
            for &c in &cols {
                for r in 0..3 {
                    wr_f32(j, at, m[c][r]);
                    at += 4;
                }
            }
            for c in 0..3 {
                for r in 0..3 {
                    wr_f32(j, at, nm[c][r]);
                    at += 4;
                }
            }
            wr_i32(j, 84, rd_i32(s, 4));
            wr_i32(j, 88, count);
            wr_i32(j, 92, vertices * VERTEX);
            wr_i32(j, 96, abgr);
            wr_i32(j, 100, overlay);
            wr_i32(j, 104, light);
            wr_i32(j, 108, 0);
            wr_i32(j, 112, rd_i32(s, 12));
            wr_i32(j, 116, argb);
            wr_i32(j, 120, -1);
            wr_f32(j, 124, (flags >> 16) as f32 / 8192.0);
            written += 1;
            vertices += count;
        }
        if subtree > 1 {
            poses.push((m, nm));
            ends.push(i + subtree);
        }
        i += 1;
    }
    (written, vertices, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A root at identity with one child: the child's pose is the parent's times its own.
    #[test]
    fn child_follows_parent() {
        let mut statics = vec![0u8; 2 * STATIC];
        let mut dynamic = vec![0u8; 2 * DYNAMIC];
        let put = |b: &mut Vec<u8>, at: usize, v: i32| b[at..at + 4].copy_from_slice(&v.to_le_bytes());
        let putf = |b: &mut Vec<u8>, at: usize, v: f32| b[at..at + 4].copy_from_slice(&v.to_le_bytes());
        put(&mut statics, 0, 2);
        put(&mut statics, 8, 24);
        put(&mut statics, STATIC, 1);
        put(&mut statics, STATIC + 8, 24);
        for k in 0..2 {
            put(&mut dynamic, k * DYNAMIC, 1);
            for s in 0..3 {
                putf(&mut dynamic, k * DYNAMIC + 28 + s * 4, 1.0);
            }
        }
        putf(&mut dynamic, 4, 16.0); // root moves 1 along x
        putf(&mut dynamic, DYNAMIC + 24, std::f32::consts::FRAC_PI_2); // child turns 90 degrees round z
        putf(&mut dynamic, DYNAMIC + 8, 16.0); // child moves 1 along y (in the root's frame)
        let mut root = vec![0u8; ROOT];
        for c in 0..4 {
            putf(&mut root, (c * 4 + c) * 4, 1.0);
        }
        for c in 0..3 {
            putf(&mut root, 64 + (c * 3 + c) * 4, 1.0);
        }
        let mut jobs = vec![0u8; 2 * JOB];
        let (w, v, u) = unsafe { walk(statics.as_ptr(), dynamic.as_ptr(), 2, root.as_ptr(), jobs.as_mut_ptr(), 0, 0, 0, 0) };
        assert_eq!((w, v, u), (2, 48, false));
        let f = |j: usize, k: usize| f32::from_le_bytes(jobs[j * JOB + k * 4..j * JOB + k * 4 + 4].try_into().unwrap());
        // Child: translation (1, 1, 0); its x axis turned onto y.
        assert!((f(1, 9) - 1.0).abs() < 1e-6 && (f(1, 10) - 1.0).abs() < 1e-6);
        assert!(f(1, 0).abs() < 1e-6 && (f(1, 1) - 1.0).abs() < 1e-6);
        assert_eq!(i32::from_le_bytes(jobs[JOB + 92..JOB + 96].try_into().unwrap()), 24 * 36);
    }
}
