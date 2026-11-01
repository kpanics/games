//! Translucent quads put in order for the upload (StagedVertexBuffer.uploadDrawsToBuffers: skin layers, translucent
//! entity parts, text backgrounds): the game's MeshData.SortState with VertexSorting.byDistance — centres from each
//! quad's corners 0 and 2, the key as JOML works it out (no fused multiply-add), farthest first, ties in the order
//! drawn — written straight into the index buffer. A stable sort, so the order is the game's to the quad: a radix
//! sort for many quads, a merge sort for a few (a skin's layer is a draw of its own, a hundred quads or so; the radix's
//! tables cost more than such a sort), in buffers kept from call to call.

use std::cell::RefCell;

thread_local! {
    /// Keys, and the order with its scratch, kept between calls (the render thread sorts dozens of draws a frame).
    static BUFFERS: RefCell<(Vec<u32>, Vec<u32>, Vec<u32>, Vec<u64>)> = const { RefCell::new((Vec::new(), Vec::new(), Vec::new(), Vec::new())) };
}

/// Below this many quads the merge sort, from it the radix sort.
const RADIX_FROM: usize = 2048;

/// What the keys measure (VertexSorting.DISTANCE_TO_ORIGIN, VertexSorting.ORTHOGRAPHIC_Z).
pub const DISTANCE: i32 = 0;
pub const ORTHOGRAPHIC_Z: i32 = 1;

/// A float's bits made to sort as unsigned integers in the float's order.
#[inline]
fn ordered(f: f32) -> u32 {
    let b = f.to_bits();
    if b & 0x8000_0000 != 0 { !b } else { b | 0x8000_0000 }
}

/// Writes the sorted indices of the quads in `slices` (pointer, vertex count; `stride` bytes a vertex, the position's
/// three floats at `position`) to `dst`, 2 or 4 bytes an index. Returns the quads written.
///
/// # Safety
/// The slices must hold their vertices; `dst` room for six indices a quad.
pub unsafe fn quads(slices: &[(usize, usize)], stride: usize, position: usize, mode: i32, dst: *mut u8, index_bytes: usize) -> usize {
    BUFFERS.with(|b| {
        let (keys, idx, tmp, pairs) = &mut *b.borrow_mut();
        quads_in(slices, stride, position, mode, dst, index_bytes, keys, idx, tmp, pairs)
    })
}

#[allow(clippy::too_many_arguments)]
unsafe fn quads_in(
    slices: &[(usize, usize)], stride: usize, position: usize, mode: i32, dst: *mut u8, index_bytes: usize, keys: &mut Vec<u32>, idx: &mut Vec<u32>,
    tmp: &mut Vec<u32>, pairs: &mut Vec<u64>,
) -> usize {
    let total: usize = slices.iter().map(|s| s.1 / 4).sum();
    // Descending by key, stable: sort ascending by the key's complement, keeping the drawn order among equals.
    keys.clear();
    keys.reserve(total);
    for &(ptr, vertices) in slices {
        let base = ptr as *const u8;
        for q in 0..vertices / 4 {
            let a = base.add(q * 4 * stride + position) as *const f32;
            let b = base.add((q * 4 + 2) * stride + position) as *const f32;
            let x = (a.read_unaligned() + b.read_unaligned()) / 2.0;
            let y = (a.add(1).read_unaligned() + b.add(1).read_unaligned()) / 2.0;
            let z = (a.add(2).read_unaligned() + b.add(2).read_unaligned()) / 2.0;
            let key = if mode == ORTHOGRAPHIC_Z {
                -z
            } else {
                // Vector3f.distanceSquared from the origin: fma(dx, dx, fma(dy, dy, dz * dz)) without a fused op.
                let (dx, dy, dz) = (0.0 - x, 0.0 - y, 0.0 - z);
                dx * dx + (dy * dy + dz * dz)
            };
            keys.push(!ordered(key));
        }
    }
    let order: &[u32] = if keys.len() >= RADIX_FROM {
        radix(keys, idx, tmp);
        idx
    } else {
        // (key, index) in one u64: ascending by key, then by index — the stable order
        pairs.clear();
        pairs.extend(keys.iter().enumerate().map(|(i, &k)| ((k as u64) << 32) | i as u64));
        pairs.sort_unstable();
        idx.clear();
        idx.extend(pairs.iter().map(|&p| p as u32));
        idx
    };
    let mut out = dst;
    for &q in order {
        let i = q * 4;
        let six = [i, i + 1, i + 2, i + 2, i + 3, i];
        if index_bytes == 2 {
            for v in six {
                (out as *mut u16).write_unaligned(v as u16);
                out = out.add(2);
            }
        } else {
            for v in six {
                (out as *mut u32).write_unaligned(v);
                out = out.add(4);
            }
        }
    }
    order.len()
}

/// The indices of `keys` in ascending order into `idx`, equal keys in their order (LSD radix, 11 bits a pass).
fn radix(keys: &[u32], idx: &mut Vec<u32>, tmp: &mut Vec<u32>) {
    let n = keys.len();
    idx.clear();
    idx.extend(0..n as u32);
    tmp.clear();
    tmp.resize(n, 0);
    for pass in 0..3 {
        let shift = pass * 11;
        let mut count = [0usize; 2049];
        for &i in idx.iter() {
            count[((keys[i as usize] >> shift) & 0x7FF) as usize + 1] += 1;
        }
        if count[1..].iter().any(|&c| c == n) {
            continue; // every key the same in these bits: the pass changes nothing
        }
        for k in 1..2049 {
            count[k] += count[k - 1];
        }
        for &i in idx.iter() {
            let d = ((keys[i as usize] >> shift) & 0x7FF) as usize;
            tmp[count[d]] = i;
            count[d] += 1;
        }
        std::mem::swap(idx, tmp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The same order as a stable sort by the key, farthest first (Java's IntArrays.mergeSort with Floats.compare).
    #[test]
    fn as_the_game_orders() {
        for count in [1, 2, 7, 50, 300, 2047, 2048, 3000] {
            orders(count);
        }
    }

    fn orders(count: usize) {
        let mut seed = 12345u32;
        let mut rnd = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            (seed % 2000) as f32 / 100.0 - 10.0
        };
        let stride = 16usize;
        let mut v = vec![0u8; count * 4 * stride];
        for q in 0..count {
            // Some quads share a centre (ties).
            let c = if q % 7 == 0 { [1.0, 2.0, 3.0] } else { [rnd(), rnd(), rnd()] };
            for k in 0..4 {
                let off = (q * 4 + k) * stride;
                for (j, val) in c.iter().enumerate() {
                    v[off + j * 4..off + j * 4 + 4].copy_from_slice(&val.to_le_bytes());
                }
            }
        }
        let mut out = vec![0u8; count * 6 * 4];
        unsafe { quads(&[(v.as_ptr() as usize, count * 4)], stride, 0, DISTANCE, out.as_mut_ptr(), 4) };
        let got: Vec<u32> = (0..count).map(|q| u32::from_le_bytes(out[q * 24..q * 24 + 4].try_into().unwrap()) / 4).collect();
        let key = |q: usize| {
            let f = |j: usize| f32::from_le_bytes(v[q * 4 * stride + j * 4..q * 4 * stride + j * 4 + 4].try_into().unwrap());
            let (x, y, z) = (f(0), f(1), f(2));
            x * x + (y * y + z * z)
        };
        let mut want: Vec<usize> = (0..count).collect();
        want.sort_by(|&a, &b| key(b).partial_cmp(&key(a)).unwrap());
        assert_eq!(got, want.iter().map(|&q| q as u32).collect::<Vec<_>>());
    }
}
