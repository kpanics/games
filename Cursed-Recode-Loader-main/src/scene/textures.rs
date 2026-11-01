//! Texture work of the resource reload, natively — the same pixels as the game's Java.
//!
//! `solidify` is TextureUtil.solidify (before a cutout sprite's mip levels): every fully transparent pixel takes the
//! colour (alpha 0) of the nearest non-transparent one, found breadth-first from all of them at once. The order is the
//! game's — sources queued x by x, each column top to bottom; neighbours tried +x, -x, +y, -y; a pixel is taken by the
//! first that reaches it nearer — so where two are equally near the same one wins.

use std::collections::VecDeque;

/// `pixels`: the image's RGBA bytes as little-endian words (NativeImage's memory; alpha is the top byte).
pub fn solidify(pixels: &mut [u32], width: usize, height: usize) {
    let n = width * height;
    assert!(pixels.len() >= n, "image smaller than its size");
    let mut color = vec![0u32; n];
    let mut distance = vec![i32::MAX; n];
    let mut queue: VecDeque<u32> = VecDeque::with_capacity(n);
    for x in 0..width {
        for y in 0..height {
            let at = x + y * width;
            let p = pixels[at];
            if p >> 24 != 0 {
                distance[at] = 0;
                color[at] = p;
                queue.push_back(at as u32);
            }
        }
    }
    const DIRECTIONS: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
    while let Some(from) = queue.pop_front() {
        let from = from as usize;
        let (x, y) = ((from % width) as i32, (from / width) as i32);
        for (dx, dy) in DIRECTIONS {
            let (nx, ny) = (x + dx, y + dy);
            if nx >= 0 && ny >= 0 && (nx as usize) < width && (ny as usize) < height {
                let to = nx as usize + ny as usize * width;
                if distance[to] > distance[from] + 1 {
                    distance[to] = distance[from] + 1;
                    color[to] = color[from];
                    queue.push_back(to as u32);
                }
            }
        }
    }
    for at in 0..n {
        if pixels[at] >> 24 == 0 {
            // ARGB.color(0, rgb): the same three colour bytes, alpha 0.
            pixels[at] = color[at] & 0x00FF_FFFF;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_wins_and_ties_go_to_the_first_queued() {
        // 3×1: opaque red at x=0, opaque blue at x=2; the middle is as near to both: x=0 was queued first.
        let red = 0xFF00_00FF;
        let blue = 0xFFFF_0000;
        let mut p = vec![red, 0, blue];
        solidify(&mut p, 3, 1);
        assert_eq!(p, vec![red, red & 0x00FF_FFFF, blue]);
    }

    #[test]
    fn nothing_opaque_leaves_black() {
        let mut p = vec![0x0012_3456; 4];
        solidify(&mut p, 2, 2);
        assert_eq!(p, vec![0; 4]);
    }
}
