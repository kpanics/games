//! A section's block geometry, as the game's SectionCompiler + ModelBlockRenderer + BlockModelLighter make it — the
//! same vertices, bit for bit (the dev check compares them), from arrays instead of the level.
//!
//! Input (filled by Java, cursed.recode.world.NativeMesher): the block states and light of the section and two blocks
//! around it, 20³ (local -2..17; x fastest, then z, then y), and the dimension's cardinal lighting. Output: the
//! vertices per layer (BLOCK format, 28 bytes), the section's visibility (VisGraph), and where the block entities
//! and fluids are — Java handles those, as the game does.

use super::blocks::*;

pub const SIDE: i32 = 20;
pub const VOLUME: usize = (SIDE * SIDE * SIDE) as usize;
pub const VERTEX: usize = 28;
/// FluidRenderer.MAX_FLUID_HEIGHT.
const MAX_FLUID_HEIGHT: f32 = 0.8888889;

/// Direction.values(): DOWN, UP, NORTH, SOUTH, WEST, EAST.
const DIRS: [[i32; 3]; 6] = [[0, -1, 0], [0, 1, 0], [0, 0, -1], [0, 0, 1], [-1, 0, 0], [1, 0, 0]];
const DOWN: usize = 0;
const UP: usize = 1;
const NORTH: usize = 2;
const SOUTH: usize = 3;
const WEST: usize = 4;
const EAST: usize = 5;

#[inline]
fn opposite(d: usize) -> usize {
    d ^ 1
}

#[inline]
fn at(p: [i32; 3]) -> usize {
    ((p[0] + 2) + (p[2] + 2) * SIDE + (p[1] + 2) * SIDE * SIDE) as usize
}

#[inline]
fn add(p: [i32; 3], d: usize) -> [i32; 3] {
    [p[0] + DIRS[d][0], p[1] + DIRS[d][1], p[2] + DIRS[d][2]]
}

/// Mth.getSeed(x, y, z).
#[inline]
pub fn mth_seed(x: i32, y: i32, z: i32) -> i64 {
    let mut i = (x.wrapping_mul(3129871) as i64) ^ (z as i64).wrapping_mul(116129781) ^ (y as i64);
    i = i.wrapping_mul(i).wrapping_mul(42317861).wrapping_add(i.wrapping_mul(11));
    i >> 16
}

/// SingleThreadedRandomSource (java.util.Random's generator), with BitRandomSource's nextInt / nextLong.
pub struct Rng {
    seed: i64,
}

impl Rng {
    const MULTIPLIER: i64 = 25214903917;
    const MASK: i64 = (1 << 48) - 1;

    pub fn new(seed: i64) -> Rng {
        let mut r = Rng { seed: 0 };
        r.set_seed(seed);
        r
    }
    #[inline]
    pub fn set_seed(&mut self, seed: i64) {
        self.seed = (seed ^ Self::MULTIPLIER) & Self::MASK;
    }
    #[inline]
    fn next(&mut self, bits: u32) -> i32 {
        let i = self.seed.wrapping_mul(Self::MULTIPLIER).wrapping_add(11) & Self::MASK;
        self.seed = i;
        (i >> (48 - bits)) as i32
    }
    pub fn next_int(&mut self, bound: i32) -> i32 {
        assert!(bound > 0, "Bound must be positive");
        if bound & (bound - 1) == 0 {
            return ((bound as i64 * self.next(31) as i64) >> 31) as i32;
        }
        loop {
            let i = self.next(31);
            let j = i % bound;
            if i.wrapping_sub(j).wrapping_add(bound - 1) >= 0 {
                return j;
            }
        }
    }
    pub fn next_long(&mut self) -> i64 {
        let i = self.next(32);
        let j = self.next(32);
        ((i as i64) << 32).wrapping_add(j as i64)
    }
}

// ---- BlockModelLighter's tables --------------------------------------------------------------------------------

/// SizeInfo: DOWN UP NORTH SOUTH WEST EAST, then the FLIP_ ones.
const SD: u8 = 0;
const SU: u8 = 1;
const SN: u8 = 2;
const SS: u8 = 3;
const SW: u8 = 4;
const SE: u8 = 5;
const FD: u8 = 6;
const FU: u8 = 7;
const FN: u8 = 8;
const FS: u8 = 9;
const FW: u8 = 10;
const FE: u8 = 11;

struct Adjacency {
    corners: [usize; 4],
    weights: [[u8; 8]; 4],
}

const ADJ: [Adjacency; 6] = [
    Adjacency {
        corners: [WEST, EAST, NORTH, SOUTH],
        weights: [[FW, SS, FW, FS, SW, FS, SW, SS], [FW, SN, FW, FN, SW, FN, SW, SN], [FE, SN, FE, FN, SE, FN, SE, SN], [FE, SS, FE, FS, SE, FS, SE, SS]],
    },
    Adjacency {
        corners: [EAST, WEST, NORTH, SOUTH],
        weights: [[SE, SS, SE, FS, FE, FS, FE, SS], [SE, SN, SE, FN, FE, FN, FE, SN], [SW, SN, SW, FN, FW, FN, FW, SN], [SW, SS, SW, FS, FW, FS, FW, SS]],
    },
    Adjacency {
        corners: [UP, DOWN, EAST, WEST],
        weights: [[SU, FW, SU, SW, FU, SW, FU, FW], [SU, FE, SU, SE, FU, SE, FU, FE], [SD, FE, SD, SE, FD, SE, FD, FE], [SD, FW, SD, SW, FD, SW, FD, FW]],
    },
    Adjacency {
        corners: [WEST, EAST, DOWN, UP],
        weights: [[SU, FW, FU, FW, FU, SW, SU, SW], [SD, FW, FD, FW, FD, SW, SD, SW], [SD, FE, FD, FE, FD, SE, SD, SE], [SU, FE, FU, FE, FU, SE, SU, SE]],
    },
    Adjacency {
        corners: [UP, DOWN, NORTH, SOUTH],
        weights: [[SU, SS, SU, FS, FU, FS, FU, SS], [SU, SN, SU, FN, FU, FN, FU, SN], [SD, SN, SD, FN, FD, FN, FD, SN], [SD, SS, SD, FS, FD, FS, FD, SS]],
    },
    Adjacency {
        corners: [DOWN, UP, NORTH, SOUTH],
        weights: [[FD, SS, FD, FS, SD, FS, SD, SS], [FD, SN, FD, FN, SD, FN, SD, SN], [FU, SN, FU, FN, SU, FN, SU, SN], [FU, SS, FU, FS, SU, FS, SU, SS]],
    },
];

/// AmbientVertexRemap.
const REMAP: [[usize; 4]; 6] = [[0, 1, 2, 3], [2, 3, 0, 1], [3, 0, 1, 2], [0, 1, 2, 3], [3, 0, 1, 2], [1, 2, 3, 0]];

// ---- ARGB and LightCoordsUtil ------------------------------------------------------------------------------------

#[inline]
fn argb(a: i32, r: i32, g: i32, b: i32) -> i32 {
    ((a & 0xFF) << 24) | ((r & 0xFF) << 16) | ((g & 0xFF) << 8) | (b & 0xFF)
}

#[inline]
fn gray(brightness: f32) -> i32 {
    let i = (brightness * 255.0).floor() as i32;
    argb(255, i, i, i)
}

#[inline]
fn multiply(lhs: i32, rhs: i32) -> i32 {
    if lhs == -1 {
        return rhs;
    }
    if rhs == -1 {
        return lhs;
    }
    let c = |v: i32, s: u32| (v >> s) & 0xFF;
    argb(c(lhs, 24) * c(rhs, 24) / 255, c(lhs, 16) * c(rhs, 16) / 255, c(lhs, 8) * c(rhs, 8) / 255, c(lhs, 0) * c(rhs, 0) / 255)
}

#[inline]
fn scale_rgb(color: i32, scale: f32) -> i32 {
    let ch = |s: u32| (((((color >> s) & 0xFF) as f32) * scale) as i32).clamp(0, 255);
    argb((color >> 24) & 0xFF, ch(16), ch(8), ch(0))
}

#[inline]
fn to_abgr(color: i32) -> u32 {
    ((color & 0xFF00FF00u32 as i32) | ((color & 0xFF0000) >> 16) | ((color & 0xFF) << 16)) as u32
}

const FULL_BRIGHT: i32 = 15728880;

#[inline]
fn sky(packed: i32) -> i32 {
    (packed >> 20) & 15
}

#[inline]
fn block(packed: i32) -> i32 {
    (packed >> 4) & 15
}

#[inline]
fn pack(block: i32, sky: i32) -> i32 {
    (block << 4) | (sky << 20)
}

fn smooth_blend(mut n1: i32, mut n2: i32, mut n3: i32, center: i32) -> i32 {
    if sky(center) > 2 || block(center) > 2 {
        for n in [&mut n1, &mut n2, &mut n3] {
            if *n == 0 {
                *n = center;
            } else if sky(*n) == 0 {
                *n |= center & 0xFF0000;
            }
        }
    }
    (n1.wrapping_add(n2).wrapping_add(n3).wrapping_add(center) >> 2) & 16711935
}

#[allow(clippy::too_many_arguments)]
fn smooth_weighted_blend(c1: i32, c2: i32, c3: i32, c4: i32, w1: f32, w2: f32, w3: f32, w4: f32) -> i32 {
    let ssky = |c: i32| ((c >> 16) & 0xFF) as f32;
    let sblock = |c: i32| (c & 0xFF) as f32;
    let i = (ssky(c1) * w1 + ssky(c2) * w2 + ssky(c3) * w3 + ssky(c4) * w4) as i32;
    let j = (sblock(c1) * w1 + sblock(c2) * w2 + sblock(c3) * w3 + sblock(c4) * w4) as i32;
    (j & 0xFF) | ((i & 0xFF) << 16)
}

#[inline]
fn with_emission(light: i32, emission: i32) -> i32 {
    if emission == 0 {
        return light;
    }
    pack(block(light).max(emission), sky(light).max(emission))
}

// ---- VisGraph ------------------------------------------------------------------------------------------------------

struct VisGraph {
    bits: [u64; 64],
    empty: i32,
}

impl VisGraph {
    #[inline]
    fn index(x: i32, y: i32, z: i32) -> usize {
        (x | (y << 8) | (z << 4)) as usize
    }
    #[inline]
    fn get(&self, i: usize) -> bool {
        self.bits[i >> 6] & (1 << (i & 63)) != 0
    }
    #[inline]
    fn set(&mut self, i: usize) {
        self.bits[i >> 6] |= 1 << (i & 63);
    }

    /// VisibilitySet's bits (direction1 + direction2 × 6).
    fn resolve(&mut self, queue: &mut Vec<u16>) -> u64 {
        if 4096 - self.empty < 256 {
            return u64::MAX;
        }
        if self.empty == 0 {
            return 0;
        }
        let mut result = 0u64;
        // INDEX_OF_EDGES's order: getIndex(l, i1, j1), so x outermost, then y, then z.
        for l in 0..16 {
            for i1 in 0..16 {
                for j1 in 0..16 {
                    if l == 0 || l == 15 || i1 == 0 || i1 == 15 || j1 == 0 || j1 == 15 {
                        let start = Self::index(l, i1, j1);
                        if !self.get(start) {
                            let set = self.flood(start, queue);
                            for a in 0..6 {
                                if set & (1 << a) != 0 {
                                    for b in 0..6 {
                                        if set & (1 << b) != 0 {
                                            result |= 1 << (a + b * 6);
                                            result |= 1 << (b + a * 6);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        result
    }

    fn flood(&mut self, start: usize, queue: &mut Vec<u16>) -> u32 {
        let mut set = 0u32;
        queue.clear();
        queue.push(start as u16);
        self.set(start);
        let mut head = 0;
        while head < queue.len() {
            let i = queue[head] as i32;
            head += 1;
            let x = i & 15;
            let y = (i >> 8) & 15;
            let z = (i >> 4) & 15;
            if x == 0 {
                set |= 1 << WEST;
            } else if x == 15 {
                set |= 1 << EAST;
            }
            if y == 0 {
                set |= 1 << DOWN;
            } else if y == 15 {
                set |= 1 << UP;
            }
            if z == 0 {
                set |= 1 << NORTH;
            } else if z == 15 {
                set |= 1 << SOUTH;
            }
            for d in 0..6 {
                let j = match d {
                    DOWN => (y != 0).then(|| i - 256),
                    UP => (y != 15).then(|| i + 256),
                    NORTH => (z != 0).then(|| i - 16),
                    SOUTH => (z != 15).then(|| i + 16),
                    WEST => (x != 0).then(|| i - 1),
                    _ => (x != 15).then(|| i + 1),
                };
                if let Some(j) = j {
                    let j = j as usize;
                    if !self.get(j) {
                        self.set(j);
                        queue.push(j as u16);
                    }
                }
            }
        }
        set
    }
}

// ---- the mesher ------------------------------------------------------------------------------------------------------

pub struct Input<'a> {
    pub states: &'a [i32],
    pub light: &'a [u8],
    pub origin: [i32; 3],
    pub ambient_occlusion: bool,
    pub cutout_leaves: bool,
    pub cardinal: [f32; 6],
}

/// The order of a layer's quads by facing: the unculled ones first, then DOWN UP NORTH SOUTH WEST EAST, then the
/// upright diagonals +x+z +x-z -x+z -x-z (blocks::facing).
pub const FACINGS: usize = 11;

#[derive(Default)]
pub struct Output {
    pub layers: [Vec<u8>; 3],
    /// Quads per facing of the solid and cutout layers, in `layers` order (translucent: all in the first).
    pub facings: [[u32; FACINGS]; 3],
    buckets: [[Vec<u8>; FACINGS]; 2],
    /// Section-local positions (x | y << 4 | z << 8) in the game's order.
    pub block_entities: Vec<u32>,
    pub fluids: Vec<u32>,
    pub visibility: u64,
    queue: Vec<u16>,
    parts: Vec<u32>,
}

impl Output {
    pub fn clear(&mut self) {
        for l in &mut self.layers {
            l.clear();
        }
        for b in self.buckets.iter_mut().flatten() {
            b.clear();
        }
        self.facings = [[0; FACINGS]; 3];
        self.block_entities.clear();
        self.fluids.clear();
        self.visibility = 0;
    }
}

/// Where quads go while meshing: the translucent layer as is, the others by facing.
struct Sink<'o> {
    layers: &'o mut [Vec<u8>; 3],
    buckets: &'o mut [[Vec<u8>; FACINGS]; 2],
}

pub enum Outcome {
    Done,
    /// A block the mesher does not reproduce: the game compiles this section.
    Fallback,
    /// The tint callback threw; its exception is pending in Java.
    Thrown,
}

/// A tint: (section-local position, state id, tint index) → ARGB, or None when Java threw.
pub type Tint<'a> = dyn FnMut(u32, i32, i32) -> Option<i32> + 'a;

struct Mesher<'a, 'b> {
    t: &'a Tables,
    inp: &'a Input<'a>,
    tint: &'a mut Tint<'b>,
    rng: Rng,
    face_cubic: bool,
    face_partial: bool,
    shape: [f32; 12],
    color: [i32; 4],
    light: [i32; 4],
    tint_known: u32,
    tint_values: [i32; 32],
    thrown: bool,
}

impl<'a, 'b> Mesher<'a, 'b> {
    #[inline]
    fn state(&self, p: [i32; 3]) -> &'a StateInfo {
        let id = self.inp.states[at(p)];
        &self.t.states[id as usize]
    }

    /// LightCoordsUtil.getLightCoords(state, pos): `state` decides emissive and emission, `pos` the raw light.
    #[inline]
    fn light_coords(&self, s: &StateInfo, p: [i32; 3]) -> i32 {
        if s.has(EMISSIVE) {
            return FULL_BRIGHT;
        }
        let l = self.inp.light[at(p)] as i32;
        let raw = pack(l & 15, l >> 4);
        if block(raw) < s.light_emission {
            (raw & 0xFF0000) | (s.light_emission << 4)
        } else {
            raw
        }
    }

    /// Block.shouldRenderFace, with the game's skipRendering overrides.
    fn should_render_face(&self, s: &StateInfo, n: &StateInfo, d: usize) -> bool {
        let neighbour_shape = n.faces[opposite(d)];
        if neighbour_shape == 1 {
            return false;
        }
        if self.skip_rendering(s, n, d) {
            return false;
        }
        if neighbour_shape == 0 {
            return true;
        }
        let own = s.faces[d];
        if own == 0 {
            return true;
        }
        self.t.occludes[own as usize * self.t.shapes + neighbour_shape as usize] != 0
    }

    /// Dev (CURSED_DEV_DEEPLEAVES): a leaf face behind two more layers of leaves (the next two blocks its way are
    /// leaves) is left out — to look at it from outside one sees through two leaf textures first.
    fn deep_leaf_face(&self, s: &StateInfo, np: [i32; 3], d: usize) -> bool {
        static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        if !*ON.get_or_init(|| std::env::var_os("CURSED_DEV_DEEPLEAVES").is_some()) {
            return false;
        }
        s.has(LEAVES) && self.state(np).has(LEAVES) && self.state(add(np, d)).has(LEAVES)
    }

    fn skip_rendering(&self, s: &StateInfo, n: &StateInfo, d: usize) -> bool {
        match s.skip {
            SKIP_SAME_BLOCK => n.block == s.block,
            SKIP_BARS => {
                let o = opposite(d);
                if n.block == s.block || (n.has(BARS_TAG) && s.has(BARS_TAG) && n.bars & (1 << (8 + o)) != 0) {
                    if d == DOWN || d == UP {
                        return true;
                    }
                    if s.bars & (1 << d) != 0 && n.bars & (1 << o) != 0 {
                        return true;
                    }
                }
                false
            }
            SKIP_LEAVES => !self.inp.cutout_leaves && n.has(LEAVES),
            SKIP_MANGROVE => n.has(MANGROVE_ROOTS) && (d == DOWN || d == UP),
            _ => false,
        }
    }

    fn collect(&mut self, node: i32, parts: &mut Vec<u32>) {
        let tree = &self.t.tree;
        let n = node as usize;
        match tree[n] {
            0 => parts.push(tree[n + 1] as u32),
            1 => {
                let count = tree[n + 1] as usize;
                let total = tree[n + 2];
                let mut i = self.rng.next_int(total);
                for e in 0..count {
                    let weight = tree[n + 3 + e * 2];
                    i -= weight;
                    if i < 0 {
                        let child = tree[n + 4 + e * 2];
                        self.collect(child, parts);
                        return;
                    }
                }
                panic!("weighted model out of range");
            }
            2 => {
                let count = tree[n + 1] as usize;
                let seed = self.rng.next_long();
                for e in 0..count {
                    self.rng.set_seed(seed);
                    let child = tree[n + 2 + e];
                    self.collect(child, parts);
                }
            }
            k => panic!("model node kind {k}"),
        }
    }

    /// BlockModelLighter.prepareQuadShape.
    fn prepare_shape(&mut self, s: &StateInfo, q: &Quad, ambient_occlusion: bool) {
        let (mut f, mut f1, mut f2, mut f3, mut f4, mut f5) = (32.0f32, 32.0f32, 32.0f32, -32.0f32, -32.0f32, -32.0f32);
        for p in &q.pos {
            f = f.min(p[0]);
            f1 = f1.min(p[1]);
            f2 = f2.min(p[2]);
            f3 = f3.max(p[0]);
            f4 = f4.max(p[1]);
            f5 = f5.max(p[2]);
        }
        if ambient_occlusion {
            self.shape[SW as usize] = f;
            self.shape[SE as usize] = f3;
            self.shape[SD as usize] = f1;
            self.shape[SU as usize] = f4;
            self.shape[SN as usize] = f2;
            self.shape[SS as usize] = f5;
            self.shape[FW as usize] = 1.0 - f;
            self.shape[FE as usize] = 1.0 - f3;
            self.shape[FD as usize] = 1.0 - f1;
            self.shape[FU as usize] = 1.0 - f4;
            self.shape[FN as usize] = 1.0 - f2;
            self.shape[FS as usize] = 1.0 - f5;
        }
        let lo = 1.0E-4f32;
        let hi = 0.9999f32;
        let d = q.direction as usize;
        self.face_partial = match d {
            DOWN | UP => f >= lo || f2 >= lo || f3 <= hi || f5 <= hi,
            NORTH | SOUTH => f >= lo || f1 >= lo || f3 <= hi || f4 <= hi,
            _ => f1 >= lo || f2 >= lo || f4 <= hi || f5 <= hi,
        };
        let full = s.has(COLLISION_FULL);
        self.face_cubic = match d {
            DOWN => f1 == f4 && (f1 < lo || full),
            UP => f1 == f4 && (f4 > hi || full),
            NORTH => f2 == f5 && (f2 < lo || full),
            SOUTH => f2 == f5 && (f5 > hi || full),
            WEST => f == f3 && (f < lo || full),
            _ => f == f3 && (f3 > hi || full),
        };
    }

    #[inline]
    fn directional(&self, q: &Quad) -> f32 {
        let d = if q.shade_override >= 0 { q.shade_override as usize } else { q.direction as usize };
        self.inp.cardinal[d]
    }

    /// BlockModelLighter.prepareQuadAmbientOcclusion.
    fn prepare_ao(&mut self, s: &StateInfo, center: [i32; 3], q: &Quad) {
        self.prepare_shape(s, q, true);
        let d = q.direction as usize;
        let bp = if self.face_cubic { add(center, d) } else { center };
        let adj = &ADJ[d];
        let c = adj.corners;
        let corner = |m: &Self, k: usize| {
            let p = add(bp, c[k]);
            let st = m.state(p);
            (m.light_coords(st, p), st.shade)
        };
        let (i, f) = corner(self, 0);
        let (j, f1) = corner(self, 1);
        let (k, f2) = corner(self, 2);
        let (l, f3) = corner(self, 3);
        let permeable = |m: &Self, k: usize| m.state(add(add(bp, c[k]), d)).has(LIGHT_PERMEABLE);
        let flag = permeable(self, 0);
        let flag1 = permeable(self, 1);
        let flag2 = permeable(self, 2);
        let flag3 = permeable(self, 3);
        let diagonal = |m: &Self, a: usize, b: usize| {
            let p = add(add(bp, c[a]), c[b]);
            let st = m.state(p);
            (st.shade, m.light_coords(st, p))
        };
        let (f4, i1) = if !flag2 && !flag { (f, i) } else { diagonal(self, 0, 2) };
        let (f5, j1) = if !flag3 && !flag { (f, i) } else { diagonal(self, 0, 3) };
        let (f6, k1) = if !flag2 && !flag1 { (f, i) } else { diagonal(self, 1, 2) };
        let (f7, l1) = if !flag3 && !flag1 { (f, i) } else { diagonal(self, 1, 3) };
        let mut i3 = self.light_coords(s, center);
        let np = add(center, d);
        let ns = self.state(np);
        if self.face_cubic || !ns.has(SOLID_RENDER) {
            i3 = self.light_coords(ns, np);
        }
        let f8 = if self.face_cubic { self.state(bp).shade } else { self.state(center).shade };
        let remap = REMAP[d];
        let f29 = (f3 + f + f5 + f8) * 0.25;
        let f30 = (f2 + f + f4 + f8) * 0.25;
        let f31 = (f2 + f1 + f6 + f8) * 0.25;
        let f32_ = (f3 + f1 + f7 + f8) * 0.25;
        if self.face_partial {
            let sh = &self.shape;
            let w = |v: usize, n: usize| sh[adj.weights[v][n * 2] as usize] * sh[adj.weights[v][n * 2 + 1] as usize];
            let mut weights = [[0f32; 4]; 4];
            for (v, row) in weights.iter_mut().enumerate() {
                for (n, x) in row.iter_mut().enumerate() {
                    *x = w(v, n);
                }
            }
            for (v, wv) in weights.iter().enumerate() {
                let b = (f29 * wv[0] + f30 * wv[1] + f31 * wv[2] + f32_ * wv[3]).clamp(0.0, 1.0);
                self.color[remap[v]] = gray(b);
            }
            let i2 = smooth_blend(l, i, j1, i3);
            let j2 = smooth_blend(k, i, i1, i3);
            let k2 = smooth_blend(k, j, k1, i3);
            let l2 = smooth_blend(l, j, l1, i3);
            for (v, wv) in weights.iter().enumerate() {
                self.light[remap[v]] = smooth_weighted_blend(i2, j2, k2, l2, wv[0], wv[1], wv[2], wv[3]);
            }
        } else {
            self.light[remap[0]] = smooth_blend(l, i, j1, i3);
            self.light[remap[1]] = smooth_blend(k, i, i1, i3);
            self.light[remap[2]] = smooth_blend(k, j, k1, i3);
            self.light[remap[3]] = smooth_blend(l, j, l1, i3);
            self.color[remap[0]] = gray(f29);
            self.color[remap[1]] = gray(f30);
            self.color[remap[2]] = gray(f31);
            self.color[remap[3]] = gray(f32_);
        }
        let scale = self.directional(q);
        for c in &mut self.color {
            *c = scale_rgb(*c, scale);
        }
    }

    /// BlockModelLighter.prepareQuadFlat.
    fn prepare_flat(&mut self, s: &StateInfo, pos: [i32; 3], light: Option<i32>, q: &Quad) {
        let l = match light {
            Some(l) => l,
            None => {
                self.prepare_shape(s, q, false);
                let p = if self.face_cubic { add(pos, q.direction as usize) } else { pos };
                self.light_coords(s, p)
            }
        };
        self.light = [l; 4];
        self.color = [gray(self.directional(q)); 4];
    }

    fn tint_color(&mut self, local: u32, sid: i32, s: &StateInfo, index: i32) -> i32 {
        if index >= s.tints || index < 0 {
            return -1;
        }
        let bit = 1u32 << (index as u32 & 31);
        if index < 32 && self.tint_known & bit != 0 {
            return self.tint_values[index as usize];
        }
        match (self.tint)(local, sid, index) {
            Some(c) => {
                if index < 32 {
                    self.tint_known |= bit;
                    self.tint_values[index as usize] = c;
                }
                c
            }
            None => {
                self.thrown = true;
                -1
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn put(&mut self, out: &mut Sink, q: &Quad, b: [f32; 3], local: u32, sid: i32, s: &StateInfo, force_solid: bool) {
        if q.tint != -1 {
            let c = self.tint_color(local, sid, s, q.tint);
            for v in &mut self.color {
                *v = multiply(*v, c);
            }
        }
        let layer = if force_solid { 0 } else { q.layer as usize };
        let mut v = [0u8; VERTEX * 4];
        for j in 0..4 {
            let p = q.pos[j];
            let o = j * VERTEX;
            let w = |at: usize, bits: u32, v: &mut [u8; VERTEX * 4]| v[o + at..o + at + 4].copy_from_slice(&bits.to_le_bytes());
            w(0, (p[0] + b[0]).to_bits(), &mut v);
            w(4, (p[1] + b[1]).to_bits(), &mut v);
            w(8, (p[2] + b[2]).to_bits(), &mut v);
            w(12, to_abgr(self.color[j]), &mut v);
            w(16, q.uv[j][0].to_bits(), &mut v);
            w(20, q.uv[j][1].to_bits(), &mut v);
            w(24, with_emission(self.light[j], q.emission) as u32, &mut v);
        }
        if layer < 2 {
            out.buckets[layer][q.facing as usize].extend_from_slice(&v);
        } else {
            out.layers[layer].extend_from_slice(&v);
        }
    }

    /// The block's offset (BlockBehaviour.Properties.offsetType), as floats added to the section-local position.
    fn offset(s: &StateInfo, wx: i32, wz: i32) -> [f32; 3] {
        if s.offset_type == OFFSET_NONE {
            return [0.0; 3];
        }
        let i = mth_seed(wx, 0, wz);
        let f = s.max_horizontal as f64;
        let h = |v: i64| -> f64 {
            let x = ((v & 15) as f32 / 15.0f32) as f64;
            let d = (x - 0.5) * 0.5;
            if d < -f {
                -f
            } else {
                d.min(f)
            }
        };
        let dx = h(i);
        let dz = h(i >> 8);
        let dy = if s.offset_type == OFFSET_XYZ { ((((i >> 4) & 15) as f32 / 15.0f32) as f64 - 1.0) * s.max_vertical as f64 } else { 0.0 };
        [dx as f32, dy as f32, dz as f32]
    }

    /// ModelBlockRenderer.tesselateBlock.
    fn block(&mut self, out: &mut Sink, parts: &mut Vec<u32>, p: [i32; 3], sid: i32, s: &'a StateInfo) {
        let o = self.inp.origin;
        let w = [o[0] + p[0], o[1] + p[1], o[2] + p[2]];
        self.rng.set_seed(mth_seed(w[0] + s.seed_offset[0], w[1] + s.seed_offset[1], w[2] + s.seed_offset[2]));
        parts.clear();
        self.collect(s.model, parts);
        if parts.is_empty() {
            return;
        }
        let off = Self::offset(s, w[0], w[2]);
        let b = [p[0] as f32 + off[0], p[1] as f32 + off[1], p[2] as f32 + off[2]];
        let local = (p[0] | (p[1] << 4) | (p[2] << 8)) as u32;
        let force_solid = !self.inp.cutout_leaves && s.has(LEAVES);
        self.tint_known = 0;
        let ao = self.inp.ambient_occlusion && s.light_emission == 0 && self.t.parts[parts[0] as usize].ambient_occlusion;
        let t = self.t;
        let mut checked = 0u32;
        let mut render = 0u32;
        for &part in parts.iter() {
            for d in 0..6 {
                let bit = 1 << d;
                let was_checked = checked & bit != 0;
                let renders = render & bit != 0;
                if was_checked && !renders {
                    continue;
                }
                let list = t.quads_of(part, d);
                if list.is_empty() {
                    continue;
                }
                let np = add(p, d);
                if !was_checked {
                    checked |= bit;
                    if self.should_render_face(s, self.state(np), d) && !self.deep_leaf_face(s, np, d) {
                        render |= bit;
                    } else {
                        continue;
                    }
                }
                if ao {
                    for &qi in list {
                        let q = &t.quads[qi as usize];
                        self.prepare_ao(s, p, q);
                        self.put(out, q, b, local, sid, s, force_solid);
                    }
                } else {
                    let l = self.light_coords(s, np);
                    for &qi in list {
                        let q = &t.quads[qi as usize];
                        self.prepare_flat(s, p, Some(l), q);
                        self.put(out, q, b, local, sid, s, force_solid);
                    }
                }
            }
            for &qi in t.quads_of(part, 6) {
                let q = &t.quads[qi as usize];
                if ao {
                    self.prepare_ao(s, p, q);
                } else {
                    self.prepare_flat(s, p, None, q);
                }
                self.put(out, q, b, local, sid, s, force_solid);
            }
            if self.thrown {
                return;
            }
        }
    }
}

// ---- fluids: FluidRenderer.tesselate and FlowingFluid.getFlow ----------------------------------------------------------

/// The tint index Java answers with the fluid model's tint (NativeMesher.Job.FLUID_TINT).
const FLUID_TINT: i32 = 1000;
/// Direction.Plane.HORIZONTAL's order.
const HORIZONTAL: [usize; 4] = [NORTH, EAST, SOUTH, WEST];

/// Mth.fastInvSqrt.
fn fast_inv_sqrt(x: f64) -> f64 {
    let d0 = 0.5 * x;
    let i = 6910469410427058090i64.wrapping_sub((x.to_bits() as i64) >> 1);
    let x = f64::from_bits(i as u64);
    x * (1.5 - d0 * x * x)
}

impl<'a, 'b> Mesher<'a, 'b> {
    #[inline]
    fn fluid_at(&self, p: [i32; 3]) -> &'a FluidInfo {
        &self.t.fluids[self.inp.states[at(p)] as usize]
    }

    /// Mth.atan2 with the game's tables.
    fn atan2(&self, mut y: f64, mut x: f64) -> f64 {
        let d0 = x * x + y * y;
        if d0.is_nan() {
            return f64::NAN;
        }
        let flag = y < 0.0;
        if flag {
            y = -y;
        }
        let flag1 = x < 0.0;
        if flag1 {
            x = -x;
        }
        let flag2 = y > x;
        if flag2 {
            std::mem::swap(&mut x, &mut y);
        }
        let d9 = fast_inv_sqrt(d0);
        x *= d9;
        y *= d9;
        let frac_bias = f64::from_bits(4805340802404319232u64);
        let d2 = frac_bias + y;
        let i = d2.to_bits() as i32 as usize;
        let d3 = self.t.asin[i];
        let d4 = self.t.cos[i];
        let d5 = d2 - frac_bias;
        let d6 = y * d4 - x * d5;
        let d7 = (6.0 + d6 * d6) * d6 * 0.16666666666666666;
        let mut d8 = d3 + d7;
        if flag2 {
            d8 = std::f64::consts::FRAC_PI_2 - d8;
        }
        if flag1 {
            d8 = std::f64::consts::PI - d8;
        }
        if flag {
            d8 = -d8;
        }
        d8
    }

    #[inline]
    fn mth_sin(&self, v: f64) -> f32 {
        self.t.sin[((v * 10430.378350470453) as i64 & 65535) as usize]
    }

    #[inline]
    fn mth_cos(&self, v: f64) -> f32 {
        self.t.sin[((v * 10430.378350470453 + 16384.0) as i64 & 65535) as usize]
    }

    /// Fluid.isSame between the fluid here and `other` (both by group; no fluid is never the same as a fluid).
    #[inline]
    fn same_fluid(f: &FluidInfo, other: &FluidInfo) -> bool {
        f.group != 0 && f.group == other.group
    }

    /// FluidRenderer.isFaceOccludedByState: the neighbour's face toward us hides our face of `height` toward `d`.
    fn fluid_face_occluded(&self, d: usize, height: f32, n: &StateInfo) -> bool {
        let shape = n.faces[opposite(d)];
        if shape == 0 {
            return false;
        }
        if shape == 1 {
            return d != UP || height == 1.0;
        }
        let s = &self.t.fluid_shapes[shape as usize];
        match d {
            DOWN => {
                debug_assert!(height == MAX_FLUID_HEIGHT);
                s.down
            }
            UP => ((height as f64) - 1.0).abs() <= 1.0e-7 && s.up_full,
            _ => {
                if (height as f64) < 1.0e-7 {
                    return false;
                }
                let k = match d {
                    NORTH => 0,
                    SOUTH => 1,
                    WEST => 2,
                    _ => 3,
                };
                height as f64 <= s.sides[k]
            }
        }
    }

    /// FluidRenderer.shouldRenderFace.
    #[inline]
    fn fluid_should_render(&self, f: &FluidInfo, d: usize, neighbour: &FluidInfo) -> bool {
        !Self::same_fluid(f, neighbour) && f.flags & (1 << (F_SELF_OCCLUDED + d as i32)) == 0
    }

    /// FluidRenderer.getHeight(level, fluid, pos, state, fluidState).
    fn fluid_height(&self, f: &FluidInfo, p: [i32; 3]) -> f32 {
        let here = self.fluid_at(p);
        if Self::same_fluid(f, here) {
            if Self::same_fluid(f, self.fluid_at(add(p, UP))) {
                1.0
            } else {
                here.height
            }
        } else if here.flags & F_LEGACY_SOLID == 0 {
            0.0
        } else {
            -1.0
        }
    }

    fn add_weighted(w: &mut [f32; 2], h: f32) {
        if h >= 0.8 {
            w[0] += h * 10.0;
            w[1] += 10.0;
        } else if h >= 0.0 {
            w[0] += h;
            w[1] += 1.0;
        }
    }

    /// FluidRenderer.calculateAverageHeight.
    fn fluid_average(&self, f: &FluidInfo, own: f32, height2: f32, height1: f32, corner: [i32; 3]) -> f32 {
        if !(height1 >= 1.0) && !(height2 >= 1.0) {
            let mut w = [0.0f32; 2];
            if height1 > 0.0 || height2 > 0.0 {
                let h = self.fluid_height(f, corner);
                if h >= 1.0 {
                    return 1.0;
                }
                Self::add_weighted(&mut w, h);
            }
            Self::add_weighted(&mut w, own);
            Self::add_weighted(&mut w, height1);
            Self::add_weighted(&mut w, height2);
            w[0] / w[1]
        } else {
            1.0
        }
    }

    /// FlowingFluid.isSolidFace for a horizontal direction.
    fn solid_face(&self, f: &FluidInfo, p: [i32; 3], d: usize) -> bool {
        let fl = self.fluid_at(p);
        if Self::same_fluid(f, fl) {
            return false;
        }
        if fl.flags & F_ICE != 0 {
            return false;
        }
        fl.flags & (1 << (F_STURDY + d as i32)) != 0
    }

    /// FlowingFluid.getFlow, then Vec3.normalize.
    fn fluid_flow(&self, f: &FluidInfo, p: [i32; 3]) -> [f64; 3] {
        let affects = |n: &FluidInfo| n.group == 0 || Self::same_fluid(f, n);
        let mut d0 = 0.0f64;
        let mut d1 = 0.0f64;
        for d in HORIZONTAL {
            let np = add(p, d);
            let nf = self.fluid_at(np);
            if affects(nf) {
                let mut h = nf.height;
                let mut f1 = 0.0f32;
                if h == 0.0 {
                    if nf.flags & F_BLOCKS_FLOW == 0 {
                        let below = self.fluid_at(add(np, DOWN));
                        if affects(below) {
                            h = below.height;
                            if h > 0.0 {
                                f1 = f.height - (h - MAX_FLUID_HEIGHT);
                            }
                        }
                    }
                } else if h > 0.0 {
                    f1 = f.height - h;
                }
                if f1 != 0.0 {
                    d0 += (DIRS[d][0] as f32 * f1) as f64;
                    d1 += (DIRS[d][2] as f32 * f1) as f64;
                }
            }
        }
        let normalize = |v: [f64; 3]| -> [f64; 3] {
            let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            if len < 1.0e-5f32 as f64 {
                [0.0; 3]
            } else {
                [v[0] / len, v[1] / len, v[2] / len]
            }
        };
        let mut v = [d0, 0.0, d1];
        if f.flags & F_FALLING != 0 {
            for d in HORIZONTAL {
                let np = add(p, d);
                if self.solid_face(f, np, d) || self.solid_face(f, add(np, UP), d) {
                    let n = normalize(v);
                    v = [n[0] + 0.0, n[1] + -6.0, n[2] + 0.0];
                    break;
                }
            }
        }
        normalize(v)
    }

    /// FluidState.shouldRenderBackwardUpFace(level, above).
    fn backward_up_face(&self, f: &FluidInfo, above: [i32; 3]) -> bool {
        for i in -1..=1 {
            for j in -1..=1 {
                let q = [above[0] + i, above[1], above[2] + j];
                if !Self::same_fluid(f, self.fluid_at(q)) && !self.state(q).has(SOLID_RENDER) {
                    return true;
                }
            }
        }
        false
    }

    /// FluidRenderer.getLightCoords: the greater of here and above (LightCoordsUtil.max).
    fn fluid_light(&self, p: [i32; 3]) -> i32 {
        let a = self.light_coords(self.state(p), p);
        let up = add(p, UP);
        let b = self.light_coords(self.state(up), up);
        pack(block(a).max(block(b)), sky(a).max(sky(b)))
    }

    #[allow(clippy::too_many_arguments)]
    fn fluid_face(&self, out: &mut Sink, layer: usize, v: [[f32; 5]; 4], color: i32, light: i32, back: bool) {
        let abgr = to_abgr(color).to_le_bytes();
        let light = (light as u32).to_le_bytes();
        let mut put = |q: &[f32; 5]| {
            let mut b = [0u8; VERTEX];
            b[0..4].copy_from_slice(&q[0].to_le_bytes());
            b[4..8].copy_from_slice(&q[1].to_le_bytes());
            b[8..12].copy_from_slice(&q[2].to_le_bytes());
            b[12..16].copy_from_slice(&abgr);
            b[16..20].copy_from_slice(&q[3].to_le_bytes());
            b[20..24].copy_from_slice(&q[4].to_le_bytes());
            b[24..28].copy_from_slice(&light);
            if layer < 2 {
                out.buckets[layer][0].extend_from_slice(&b);
            } else {
                out.layers[layer].extend_from_slice(&b);
            }
        };
        for q in &v {
            put(q);
        }
        if back {
            put(&v[0]);
            put(&v[3]);
            put(&v[2]);
            put(&v[1]);
        }
    }

    /// FluidRenderer.tesselate.
    fn fluid(&mut self, out: &mut Sink, p: [i32; 3], sid: i32, _s: &'a StateInfo) {
        let t = self.t;
        let f = &t.fluids[sid as usize];
        if f.group == 0 || f.model < 0 {
            return;
        }
        let flag = !Self::same_fluid(f, self.fluid_at(add(p, UP)));
        let flag1 = self.fluid_should_render(f, DOWN, self.fluid_at(add(p, DOWN)))
            && !self.fluid_face_occluded(DOWN, MAX_FLUID_HEIGHT, self.state(add(p, DOWN)));
        let flag2 = self.fluid_should_render(f, NORTH, self.fluid_at(add(p, NORTH)));
        let flag3 = self.fluid_should_render(f, SOUTH, self.fluid_at(add(p, SOUTH)));
        let flag4 = self.fluid_should_render(f, WEST, self.fluid_at(add(p, WEST)));
        let flag5 = self.fluid_should_render(f, EAST, self.fluid_at(add(p, EAST)));
        if !(flag || flag1 || flag5 || flag4 || flag2 || flag3) {
            return;
        }
        let model = &t.fluid_models[f.model as usize];
        let layer = model.layer as usize;
        let local = (p[0] | (p[1] << 4) | (p[2] << 8)) as u32;
        let i = if model.tinted {
            match (self.tint)(local, sid, FLUID_TINT) {
                Some(c) => c,
                None => {
                    self.thrown = true;
                    return;
                }
            }
        } else {
            -1
        };
        let card = self.inp.cardinal;
        let t = self.t;
        let states = self.inp.states;
        let st = |d: usize| -> &StateInfo { &t.states[states[at(add(p, d))] as usize] };
        let f4 = self.fluid_height(f, p);
        let (mut h0, mut h1, mut h2, mut h3);
        if f4 >= 1.0 {
            h0 = 1.0;
            h1 = 1.0;
            h2 = 1.0;
            h3 = 1.0;
        } else {
            let f5 = self.fluid_height(f, add(p, NORTH));
            let f6 = self.fluid_height(f, add(p, SOUTH));
            let f7 = self.fluid_height(f, add(p, EAST));
            let f8 = self.fluid_height(f, add(p, WEST));
            h0 = self.fluid_average(f, f4, f5, f7, add(add(p, NORTH), EAST));
            h1 = self.fluid_average(f, f4, f5, f8, add(add(p, NORTH), WEST));
            h2 = self.fluid_average(f, f4, f6, f7, add(add(p, SOUTH), EAST));
            h3 = self.fluid_average(f, f4, f6, f8, add(add(p, SOUTH), WEST));
        }
        let x = p[0] as f32;
        let y = p[1] as f32;
        let z = p[2] as f32;
        let f9 = if flag1 { 0.001f32 } else { 0.0 };
        let u = |sp: &[f32; 4], o: f32| sp[0] + (sp[1] - sp[0]) * o;
        let v = |sp: &[f32; 4], o: f32| sp[2] + (sp[3] - sp[2]) * o;
        if flag && !self.fluid_face_occluded(UP, h1.min(h3).min(h2.min(h0)), st(UP)) {
            h1 -= 0.001;
            h3 -= 0.001;
            h2 -= 0.001;
            h0 -= 0.001;
            let flow = self.fluid_flow(f, p);
            let (f10, f11, f12, f13, f14, f15, f16, f17);
            if flow[0] == 0.0 && flow[2] == 0.0 {
                let sp = &model.still;
                f10 = sp[0];
                f14 = sp[2];
                f11 = f10;
                f15 = sp[3];
                f12 = sp[1];
                f16 = f15;
                f13 = f12;
                f17 = f14;
            } else {
                let f18 = self.atan2(flow[2], flow[0]) as f32 - std::f64::consts::FRAC_PI_2 as f32;
                let f19 = self.mth_sin(f18 as f64) * 0.25;
                let f20 = self.mth_cos(f18 as f64) * 0.25;
                let sp = &model.flowing;
                f10 = u(sp, 0.5 + (-f20 - f19));
                f14 = v(sp, 0.5 + (-f20 + f19));
                f11 = u(sp, 0.5 + (-f20 + f19));
                f15 = v(sp, 0.5 + (f20 + f19));
                f12 = u(sp, 0.5 + (f20 + f19));
                f16 = v(sp, 0.5 + (f20 - f19));
                f13 = u(sp, 0.5 + (f20 - f19));
                f17 = v(sp, 0.5 + (-f20 - f19));
            }
            let j1 = self.fluid_light(p);
            let k1 = scale_rgb(i, card[UP]);
            let back = self.backward_up_face(f, add(p, UP));
            self.fluid_face(
                out,
                layer,
                [
                    [x + 0.0, y + h1, z + 0.0, f10, f14],
                    [x + 0.0, y + h3, z + 1.0, f11, f15],
                    [x + 1.0, y + h2, z + 1.0, f12, f16],
                    [x + 1.0, y + h0, z + 0.0, f13, f17],
                ],
                k1,
                j1,
                back,
            );
        }
        if flag1 {
            let sp = &model.still;
            let (f31, f32_, f33, f35) = (sp[0], sp[1], sp[2], sp[3]);
            let l = self.fluid_light(add(p, DOWN));
            let i1 = scale_rgb(i, card[DOWN]);
            self.fluid_face(
                out,
                layer,
                [[x, y + f9, z, f31, f33], [x + 1.0, y + f9, z, f32_, f33], [x + 1.0, y + f9, z + 1.0, f32_, f35], [x, y + f9, z + 1.0, f31, f35]],
                i1,
                l,
                false,
            );
        }
        let k = self.fluid_light(p);
        for d in HORIZONTAL {
            let (f34, f36, f37, f39, f38, f40, flag6) = match d {
                NORTH => (h1, h0, x, x + 1.0, z + 0.001, z + 0.001, flag2),
                SOUTH => (h2, h3, x + 1.0, x, z + 1.0 - 0.001, z + 1.0 - 0.001, flag3),
                WEST => (h3, h1, x + 0.001, x + 0.001, z + 1.0, z, flag4),
                _ => (h0, h2, x + 1.0 - 0.001, x + 1.0 - 0.001, z, z + 1.0, flag5),
            };
            let neighbour = st(d);
            if flag6 && !self.fluid_face_occluded(d, f34.max(f36), neighbour) {
                let mut sp = &model.flowing;
                let mut flag7 = false;
                if let Some(overlay) = &model.overlay {
                    if self.fluid_at(add(p, d)).flags & F_OVERLAY_NEIGHBOUR != 0 {
                        sp = overlay;
                        flag7 = true;
                    }
                }
                let f41 = u(sp, 0.0);
                let f22 = u(sp, 0.5);
                let f23 = v(sp, (1.0 - f34) * 0.5);
                let f24 = v(sp, (1.0 - f36) * 0.5);
                let f25 = v(sp, 0.5);
                let f26 = if d == NORTH || d == SOUTH { card[NORTH] } else { card[WEST] };
                let j = scale_rgb(i, card[UP] * f26);
                self.fluid_face(
                    out,
                    layer,
                    [[f37, y + f34, f38, f41, f23], [f39, y + f36, f40, f22, f24], [f39, y + f9, f40, f22, f25], [f37, y + f9, f38, f41, f25]],
                    j,
                    k,
                    !flag7,
                );
            }
        }
    }
}

/// SectionCompiler.compile's block part.
pub fn mesh(t: &Tables, inp: &Input, tint: &mut Tint, out: &mut Output) -> Outcome {
    out.clear();
    assert!(inp.states.len() >= VOLUME && inp.light.len() >= VOLUME, "mesh input size");
    // First pass, one byte per block: rows of bits along x for the section and the ring around it (bit x + 1 is x).
    // A block walled in by solid blocks on all six sides shows nothing (Block.shouldRenderFace: a full neighbour face
    // hides it whatever else), unless its model has quads that are never culled.
    let mut solid = [[0u32; 18]; 18];
    let mut kinds = [[[0u32; 16]; 16]; 7];
    const K_AIR: usize = 0;
    const K_MODEL: usize = 1;
    const K_BE: usize = 2;
    const K_FLUID: usize = 3;
    const K_UNCULLED: usize = 4;
    const K_JAVA: usize = 5;
    for y in -1..17 {
        for z in -1..17 {
            let mut row = 0u32;
            let base = at([-1, y, z]);
            let inner = (0..16).contains(&y) && (0..16).contains(&z);
            for x in 0..18usize {
                let b = t.bits[inp.states[base + x] as usize];
                row |= ((b & B_SOLID != 0) as u32) << x;
                if inner && (1..17).contains(&x) {
                    let (yy, zz, bit) = (y as usize, z as usize, 1u32 << (x - 1));
                    let mut mark = |k: usize, f: u8| {
                        if b & f != 0 {
                            kinds[k][yy][zz] |= bit;
                        }
                    };
                    mark(K_AIR, B_AIR);
                    mark(K_MODEL, B_MODEL);
                    mark(K_BE, B_BLOCK_ENTITY);
                    mark(K_FLUID, B_FLUID);
                    mark(K_UNCULLED, B_UNCULLED);
                    mark(K_JAVA, B_JAVA_ONLY);
                }
            }
            solid[(y + 1) as usize][(z + 1) as usize] = row;
        }
    }
    if kinds[K_JAVA].iter().flatten().any(|&r| r != 0) {
        return Outcome::Fallback;
    }
    let mut vis = VisGraph { bits: [0; 64], empty: 4096 };
    for y in 0..16usize {
        for z in 0..16usize {
            // VisGraph index x | z << 4 | y << 8: a row of 16 bits at y * 256 + z * 16.
            let row = ((solid[y + 1][z + 1] >> 1) & 0xFFFF) as u64;
            let at_bit = y * 256 + z * 16;
            vis.bits[at_bit >> 6] |= row << (at_bit & 63);
            vis.empty -= row.count_ones() as i32;
        }
    }
    // Block entities and fluids, in the game's order (x fastest, then y, then z).
    for z in 0..16usize {
        for y in 0..16usize {
            for (k, list) in [(K_BE, &mut out.block_entities)] {
                let mut row = kinds[k][y][z];
                while row != 0 {
                    let x = row.trailing_zeros();
                    row &= row - 1;
                    list.push(x | ((y as u32) << 4) | ((z as u32) << 8));
                }
            }
        }
    }
    let mut m = Mesher {
        t,
        inp,
        tint,
        rng: Rng::new(0),
        face_cubic: false,
        face_partial: false,
        shape: [0.0; 12],
        color: [-1; 4],
        light: [FULL_BRIGHT; 4],
        tint_known: 0,
        tint_values: [0; 32],
        thrown: false,
    };
    let mut parts = std::mem::take(&mut out.parts);
    let mut sink = Sink { layers: &mut out.layers, buckets: &mut out.buckets };
    for z in 0..16usize {
        for y in 0..16usize {
            let (sy, sz) = (y + 1, z + 1);
            let c = solid[sy][sz];
            let walled = (c << 1) & (c >> 1) & solid[sy - 1][sz] & solid[sy + 1][sz] & solid[sy][sz - 1] & solid[sy][sz + 1];
            let models = kinds[K_MODEL][y][z] & !kinds[K_AIR][y][z] & (!(walled >> 1) | kinds[K_UNCULLED][y][z]) & 0xFFFF;
            let fluids = kinds[K_FLUID][y][z];
            let mut row = models | fluids;
            while row != 0 {
                let x = row.trailing_zeros() as i32;
                row &= row - 1;
                let p = [x, y as i32, z as i32];
                let sid = inp.states[at(p)];
                let s = &t.states[sid as usize];
                // As SectionCompiler: the fluid first, then the block's model.
                if fluids & (1 << x) != 0 {
                    m.fluid(&mut sink, p, sid, s);
                }
                if models & (1 << x) != 0 && !m.thrown {
                    m.block(&mut sink, &mut parts, p, sid, s);
                }
                if m.thrown {
                    out.parts = parts;
                    return Outcome::Thrown;
                }
            }
        }
    }
    out.parts = parts;
    for l in 0..2 {
        for f in 0..FACINGS {
            let b = &out.buckets[l][f];
            out.facings[l][f] = (b.len() / (VERTEX * 4)) as u32;
            out.layers[l].extend_from_slice(b);
        }
    }
    out.facings[2][0] = (out.layers[2].len() / (VERTEX * 4)) as u32;
    out.visibility = vis.resolve(&mut out.queue);
    Outcome::Done
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_matches_java() {
        // Reference values from java.util.Random (same generator) and Mth.getSeed, computed with jshell.
        let mut r = Rng::new(42);
        assert_eq!(r.next_int(10), 0);
        assert_eq!(r.next_int(10), 3);
        assert_eq!(Rng::new(1).next_long(), -4964420948893066024);
        assert_eq!(Rng::new(42).next_int(16), 11);
        assert_eq!(Rng::new(7).next_int(1000000007), 569164229);
    }

    #[test]
    fn seed_matches_java() {
        assert_eq!(mth_seed(1, 2, 3), -33674130277896);
        assert_eq!(mth_seed(-30000000, -64, 29999999), 16785290161034);
    }

    #[test]
    fn colors() {
        assert_eq!(gray(1.0), -1);
        assert_eq!(to_abgr(0xFF112233u32 as i32), 0xFF332211);
        assert_eq!(multiply(-1, 0x123456), 0x123456);
        assert_eq!(scale_rgb(-1, 0.5), argb(255, 127, 127, 127));
    }
}
