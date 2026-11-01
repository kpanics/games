//! What the mesher knows of blocks: every block state's rendering facts and its baked model, exported by Java once per
//! resource reload (cursed.recode.world.BlockExport writes the same layout this reads).
//!
//! The export, little-endian:
//!   i32 version, i32 states, i32 quads, i32 parts, i32 shapes, i32 model-tree length (ints)
//!   occlusion: shapes × shapes bytes, padded to 4 — [self][neighbour]: 1 when the self face shows past the neighbour
//!     face (Shapes.joinIsNotEmpty(self, neighbour, ONLY_FIRST)); shape 0 is Shapes.empty(), 1 is Shapes.block()
//!   quads: per quad f32 × 12 positions, u64 × 4 packed UVs, i32 direction, tint index, shade override (-1 or a
//!     direction), light emission, layer
//!   parts: per part i32 ambient occlusion, i32 × 7 quad counts (6 directions then the unculled), then the quad indices
//!   model tree: ints; a node is 0 part | 1 n total (weight node)×n | 2 n node×n
//!   states: per state i32 flags, light emission, f32 shade brightness, i32 block id, tint sources, seed dx dy dz,
//!     offset type, f32 max horizontal, f32 max vertical, i32 × 6 face shapes, i32 skip rule, i32 bars bits,
//!     i32 model node (-1 none)

use std::sync::{Arc, RwLock};

pub const AIR: i32 = 1;
pub const MODEL: i32 = 2;
pub const SOLID_RENDER: i32 = 4;
pub const LIGHT_PERMEABLE: i32 = 8;
pub const EMISSIVE: i32 = 16;
pub const COLLISION_FULL: i32 = 32;
pub const LEAVES: i32 = 64;
pub const BARS_TAG: i32 = 128;
pub const MANGROVE_ROOTS: i32 = 256;
pub const HAS_FLUID: i32 = 512;
pub const HAS_BLOCK_ENTITY: i32 = 1024;
/// Something here the mesher does not reproduce (a model or rule it does not know): such sections stay with the game.
pub const JAVA_ONLY: i32 = 2048;

/// skipRendering overrides of the game (Block subclasses), as rules.
pub const SKIP_NONE: i32 = 0;
/// HalfTransparentBlock, PowderSnowBlock: the same block next door.
pub const SKIP_SAME_BLOCK: i32 = 1;
/// IronBarsBlock.
pub const SKIP_BARS: i32 = 2;
/// LeavesBlock: leaves next door, unless leaves are cut out.
pub const SKIP_LEAVES: i32 = 3;
/// MangroveRootsBlock: mangrove roots above or below.
pub const SKIP_MANGROVE: i32 = 4;

pub const OFFSET_NONE: i32 = 0;
pub const OFFSET_XZ: i32 = 1;
pub const OFFSET_XYZ: i32 = 2;

#[derive(Clone, Copy, Default)]
pub struct StateInfo {
    pub flags: i32,
    pub light_emission: i32,
    pub shade: f32,
    pub block: i32,
    pub tints: i32,
    pub seed_offset: [i32; 3],
    pub offset_type: i32,
    pub max_horizontal: f32,
    pub max_vertical: f32,
    pub faces: [u32; 6],
    pub skip: i32,
    /// The bars' connections (CrossCollisionBlock.PROPERTY_BY_DIRECTION): bit d (a direction's ordinal) = its value,
    /// bit 8 + d = the state has that property.
    pub bars: i32,
    pub model: i32,
}

impl StateInfo {
    #[inline]
    pub fn has(&self, flag: i32) -> bool {
        self.flags & flag != 0
    }
}

#[derive(Clone, Copy)]
pub struct Quad {
    pub pos: [[f32; 3]; 4],
    pub uv: [[f32; 2]; 4],
    pub direction: u8,
    pub tint: i32,
    pub shade_override: i32,
    pub emission: i32,
    pub layer: u8,
    /// Which way the quad really faces, from its geometry: 0 not along an axis (or not flat on an axis plane),
    /// 1 + direction otherwise. Quads facing away from the camera are not drawn at all (vk/terrain.rs).
    pub facing: u8,
}

/// The facing of a quad: its corners on one axis plane and its winding's normal along that axis (1 + direction:
/// DOWN UP NORTH SOUTH WEST EAST); on an upright plane at 45° (crosses: grass, flowers, saplings) 7 + diagonal (+x+z
/// +x-z -x+z -x-z, the winding's normal); else 0.
pub fn facing(pos: &[[f32; 3]; 4]) -> u8 {
    let sub = |a: [f32; 3], b: [f32; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let d1 = sub(pos[2], pos[0]);
    let d2 = sub(pos[3], pos[1]);
    let n = [d1[1] * d2[2] - d1[2] * d2[1], d1[2] * d2[0] - d1[0] * d2[2], d1[0] * d2[1] - d1[1] * d2[0]];
    for axis in 0..3 {
        if pos.iter().all(|p| p[axis] == pos[0][axis]) {
            let others: f32 = (0..3).filter(|&a| a != axis).map(|a| n[a].abs()).sum();
            if n[axis] == 0.0 || others > 1.0e-6 * n[axis].abs() {
                return 0;
            }
            let positive = n[axis] > 0.0;
            // Directions: DOWN 0 UP 1 (y), NORTH 2 SOUTH 3 (z), WEST 4 EAST 5 (x).
            let d = match (axis, positive) {
                (0, true) => 5,
                (0, false) => 4,
                (1, true) => 1,
                (1, false) => 0,
                (_, true) => 3,
                (_, false) => 2,
            };
            return 1 + d;
        }
    }
    let (ax, ay, az) = (n[0].abs(), n[1].abs(), n[2].abs());
    if ax > 0.0 && ay <= 1.0e-6 * ax && (ax - az).abs() <= 1.0e-4 * ax {
        let plane = |p: &[f32; 3]| n[0] * p[0] + n[2] * p[2];
        let d = plane(&pos[0]);
        if pos.iter().all(|p| (plane(p) - d).abs() <= 1.0e-4 * ax) {
            return 7 + if n[0] > 0.0 { 0 } else { 2 } + if n[2] > 0.0 { 0 } else { 1 };
        }
    }
    0
}

pub struct Part {
    pub ambient_occlusion: bool,
    /// Ranges into `Tables::part_quads`, per direction (0–5) then the unculled (6).
    pub lists: [(u32, u32); 7],
}

/// The per-state facts the mesher's first pass reads for every block, one byte each (`Tables::bits`).
pub const B_AIR: u8 = 1;
pub const B_SOLID: u8 = 2;
/// Has a model to mesh (RenderShape.MODEL).
pub const B_MODEL: u8 = 4;
pub const B_BLOCK_ENTITY: u8 = 8;
pub const B_FLUID: u8 = 16;
pub const B_JAVA_ONLY: u8 = 32;
/// Some variant of its model has quads that are never culled: such a block is meshed even when walled in.
pub const B_UNCULLED: u8 = 64;

/// A fluid model (FluidModel): its layer, sprites (u0 u1 v0 v1) and whether it is tinted.
pub struct FluidModel {
    pub layer: u8,
    pub still: [f32; 4],
    pub flowing: [f32; 4],
    pub overlay: Option<[f32; 4]>,
    pub tinted: bool,
}

/// What the fluid code needs of a block state (BlockExport.fluids).
#[derive(Clone, Copy, Default)]
pub struct FluidInfo {
    /// 0 no fluid; else the fluid's source + 1 (Fluid.isSame: the same group).
    pub group: i32,
    /// FluidState.getOwnHeight.
    pub height: f32,
    pub model: i32,
    pub flags: i32,
}

pub const F_FALLING: i32 = 1;
pub const F_BLOCKS_FLOW: i32 = 2;
pub const F_LEGACY_SOLID: i32 = 4;
pub const F_ICE: i32 = 8;
pub const F_OVERLAY_NEIGHBOUR: i32 = 16;
pub const F_STURDY: i32 = 8;
pub const F_SELF_OCCLUDED: i32 = 16;

/// How a face shape hides a fluid's face (FluidRenderer.isFaceOccludedByState).
#[derive(Clone, Copy, Default)]
pub struct FluidShape {
    pub down: bool,
    pub up_full: bool,
    /// NORTH SOUTH WEST EAST: the greatest side height hidden (-1 none).
    pub sides: [f64; 4],
}

pub struct Tables {
    pub states: Vec<StateInfo>,
    pub bits: Vec<u8>,
    pub fluid_models: Vec<FluidModel>,
    pub fluid_shapes: Vec<FluidShape>,
    pub fluids: Vec<FluidInfo>,
    /// Mth.SIN, Mth.ASIN_TAB, Mth.COS_TAB.
    pub sin: Vec<f32>,
    pub asin: Vec<f64>,
    pub cos: Vec<f64>,
    pub quads: Vec<Quad>,
    pub parts: Vec<Part>,
    pub part_quads: Vec<u32>,
    pub tree: Vec<i32>,
    pub shapes: usize,
    pub occludes: Vec<u8>,
}

impl Tables {
    #[inline]
    pub fn quads_of(&self, part: u32, list: usize) -> &[u32] {
        let (a, b) = self.parts[part as usize].lists[list];
        &self.part_quads[a as usize..b as usize]
    }
}

static TABLES: RwLock<Option<Arc<Tables>>> = RwLock::new(None);

pub fn current() -> Option<Arc<Tables>> {
    TABLES.read().unwrap().clone()
}

pub fn clear() {
    *TABLES.write().unwrap() = None;
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn i32(&mut self) -> i32 {
        let v = i32::from_le_bytes(self.b[self.at..self.at + 4].try_into().unwrap());
        self.at += 4;
        v
    }
    fn f32(&mut self) -> f32 {
        f32::from_bits(self.i32() as u32)
    }
    fn f64(&mut self) -> f64 {
        f64::from_bits(self.u64())
    }
    fn u64(&mut self) -> u64 {
        let v = u64::from_le_bytes(self.b[self.at..self.at + 8].try_into().unwrap());
        self.at += 8;
        v
    }
}

pub const VERSION: i32 = 3;

pub fn load(bytes: &[u8]) {
    let mut r = Reader { b: bytes, at: 0 };
    let version = r.i32();
    assert_eq!(version, VERSION, "block export version");
    let n_states = r.i32() as usize;
    let n_quads = r.i32() as usize;
    let n_parts = r.i32() as usize;
    let n_shapes = r.i32() as usize;
    let n_tree = r.i32() as usize;
    let occludes = bytes[r.at..r.at + n_shapes * n_shapes].to_vec();
    r.at += (n_shapes * n_shapes).div_ceil(4) * 4;
    let mut quads = Vec::with_capacity(n_quads);
    for _ in 0..n_quads {
        let mut pos = [[0f32; 3]; 4];
        for p in &mut pos {
            for c in p.iter_mut() {
                *c = r.f32();
            }
        }
        let mut uv = [[0f32; 2]; 4];
        for t in &mut uv {
            let packed = r.u64();
            // UVPair: U in the high 32 bits, V in the low, both float bits.
            *t = [f32::from_bits((packed >> 32) as u32), f32::from_bits(packed as u32)];
        }
        let direction = r.i32() as u8;
        let tint = r.i32();
        let shade_override = r.i32();
        let emission = r.i32();
        let layer = r.i32() as u8;
        quads.push(Quad { pos, uv, direction, tint, shade_override, emission, layer, facing: facing(&pos) });
    }
    let mut parts = Vec::with_capacity(n_parts);
    let mut part_quads = Vec::new();
    for _ in 0..n_parts {
        let ao = r.i32() != 0;
        let mut counts = [0u32; 7];
        for c in &mut counts {
            *c = r.i32() as u32;
        }
        let mut lists = [(0u32, 0u32); 7];
        for (i, &c) in counts.iter().enumerate() {
            let start = part_quads.len() as u32;
            for _ in 0..c {
                let q = r.i32() as u32;
                assert!((q as usize) < n_quads, "quad index");
                part_quads.push(q);
            }
            lists[i] = (start, start + c);
        }
        parts.push(Part { ambient_occlusion: ao, lists });
    }
    let tree: Vec<i32> = (0..n_tree).map(|_| r.i32()).collect();
    let mut states = Vec::with_capacity(n_states);
    for _ in 0..n_states {
        let mut s = StateInfo { flags: r.i32(), light_emission: r.i32(), shade: r.f32(), block: r.i32(), tints: r.i32(), ..Default::default() };
        s.seed_offset = [r.i32(), r.i32(), r.i32()];
        s.offset_type = r.i32();
        s.max_horizontal = r.f32();
        s.max_vertical = r.f32();
        for f in &mut s.faces {
            *f = r.i32() as u32;
            assert!((*f as usize) < n_shapes.max(2), "face shape index");
        }
        s.skip = r.i32();
        s.bars = r.i32();
        s.model = r.i32();
        states.push(s);
    }
    let n_models = r.i32() as usize;
    let sprite = |r: &mut Reader| [r.f32(), r.f32(), r.f32(), r.f32()];
    let fluid_models: Vec<FluidModel> = (0..n_models)
        .map(|_| {
            let layer = r.i32() as u8;
            let still = sprite(&mut r);
            let flowing = sprite(&mut r);
            let has_overlay = r.i32() != 0;
            let overlay = sprite(&mut r);
            let tinted = r.i32() != 0;
            FluidModel { layer, still, flowing, overlay: has_overlay.then_some(overlay), tinted }
        })
        .collect();
    let fluid_shapes: Vec<FluidShape> = (0..n_shapes)
        .map(|_| FluidShape { down: r.i32() != 0, up_full: r.i32() != 0, sides: [r.f64(), r.f64(), r.f64(), r.f64()] })
        .collect();
    let fluids: Vec<FluidInfo> = (0..n_states).map(|_| FluidInfo { group: r.i32(), height: r.f32(), model: r.i32(), flags: r.i32() }).collect();
    let sin: Vec<f32> = (0..65536).map(|_| r.f32()).collect();
    let asin: Vec<f64> = (0..257).map(|_| r.f64()).collect();
    let cos: Vec<f64> = (0..257).map(|_| r.f64()).collect();
    assert_eq!(r.at, bytes.len(), "block export length");
    let unculled = |node: i32| -> bool {
        fn walk(tree: &[i32], parts: &[Part], n: usize) -> bool {
            match tree[n] {
                0 => {
                    let (a, b) = parts[tree[n + 1] as usize].lists[6];
                    b > a
                }
                1 => (0..tree[n + 1] as usize).any(|e| walk(tree, parts, tree[n + 4 + e * 2] as usize)),
                _ => (0..tree[n + 1] as usize).any(|e| walk(tree, parts, tree[n + 2 + e] as usize)),
            }
        }
        node >= 0 && walk(&tree, &parts, node as usize)
    };
    let bits = states
        .iter()
        .map(|s| {
            let mut b = 0u8;
            let mut set = |c: bool, bit: u8| {
                if c {
                    b |= bit;
                }
            };
            set(s.has(AIR), B_AIR);
            set(s.has(SOLID_RENDER), B_SOLID);
            set(s.has(MODEL) && s.model >= 0, B_MODEL);
            set(s.has(HAS_BLOCK_ENTITY), B_BLOCK_ENTITY);
            set(s.has(HAS_FLUID), B_FLUID);
            set(s.has(JAVA_ONLY), B_JAVA_ONLY);
            set(s.has(MODEL) && unculled(s.model), B_UNCULLED);
            b
        })
        .collect();
    // The facing must agree with the face the game gave the quad wherever the quad is flat on an axis plane;
    // a disagreement would mean the winding is read the other way round and faces would go missing.
    let aligned = quads.iter().filter(|q| q.facing != 0).count();
    let agree = quads.iter().filter(|q| q.facing != 0 && q.facing - 1 == q.direction).count();
    if let Ok(mut m) = crate::vk::device::MESSAGES.lock() {
        m.push(format!("I [native] quads facing an axis: {aligned} of {}, agreeing with their face: {agree}", quads.len()));
    }
    let t = Tables { states, bits, fluid_models, fluid_shapes, fluids, sin, asin, cos, quads, parts, part_quads, tree, shapes: n_shapes, occludes };
    *TABLES.write().unwrap() = Some(Arc::new(t));
}
