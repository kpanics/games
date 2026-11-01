//! A cosmetic's model (.rcm, tools/cs2pack/rcm.py makes it from what VRF exports of Source 2's): its vertices with
//! their skin, its triangles by material, its materials' textures, its skeleton and its animations (each moved bone's
//! tracks resampled at a steady rate). A pose is each joint's local translation, rotation and scale; the skin's
//! matrices are each joint's place in the model times the inverse of where it was bound.

/// A vertex as the GPU takes it: position, normal, tangent (w its handedness), uv, four joints, four weights (bytes
/// summing to 255). 56 bytes.
pub const VERTEX: usize = 56;

#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    pub name: String,
    pub base: String,
    pub normal: String,
    /// Metalness, roughness, occlusion packed (glTF's: b metal, g roughness).
    pub packed: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Part {
    pub first: u32,
    pub count: u32,
    pub material: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Trs {
    pub t: [f32; 3],
    pub r: [f32; 4],
    pub s: [f32; 3],
}

impl Trs {
    pub const REST: Trs = Trs { t: [0.0; 3], r: [0.0, 0.0, 0.0, 1.0], s: [1.0; 3] };

    /// As a column-major matrix: translate · rotate · scale.
    pub fn matrix(&self) -> M4 {
        let [x, y, z, w] = self.r;
        let (xx, yy, zz) = (x * x, y * y, z * z);
        let (xy, xz, yz, wx, wy, wz) = (x * y, x * z, y * z, w * x, w * y, w * z);
        let [sx, sy, sz] = self.s;
        [
            (1.0 - 2.0 * (yy + zz)) * sx,
            2.0 * (xy + wz) * sx,
            2.0 * (xz - wy) * sx,
            0.0,
            2.0 * (xy - wz) * sy,
            (1.0 - 2.0 * (xx + zz)) * sy,
            2.0 * (yz + wx) * sy,
            0.0,
            2.0 * (xz + wy) * sz,
            2.0 * (yz - wx) * sz,
            (1.0 - 2.0 * (xx + yy)) * sz,
            0.0,
            self.t[0],
            self.t[1],
            self.t[2],
            1.0,
        ]
    }

    /// Between `a` and `b` by `f` (rotations along the short way).
    pub fn lerp(a: &Trs, b: &Trs, f: f32) -> Trs {
        let l3 = |p: [f32; 3], q: [f32; 3]| std::array::from_fn(|i| p[i] + (q[i] - p[i]) * f);
        Trs { t: l3(a.t, b.t), r: nlerp(a.r, b.r, f), s: l3(a.s, b.s) }
    }
}

pub type M4 = [f32; 16];

/// An affine matrix's inverse (its 3×3 part's, any scale; none when it has none).
pub fn inverse(m: &M4) -> Option<M4> {
    let a = |r: usize, c: usize| m[c * 4 + r];
    let det = a(0, 0) * (a(1, 1) * a(2, 2) - a(2, 1) * a(1, 2)) - a(0, 1) * (a(1, 0) * a(2, 2) - a(1, 2) * a(2, 0)) + a(0, 2) * (a(1, 0) * a(2, 1) - a(1, 1) * a(2, 0));
    if det.abs() < 1e-12 {
        return None;
    }
    let k = 1.0 / det;
    let inv = [
        [(a(1, 1) * a(2, 2) - a(2, 1) * a(1, 2)) * k, (a(0, 2) * a(2, 1) - a(0, 1) * a(2, 2)) * k, (a(0, 1) * a(1, 2) - a(0, 2) * a(1, 1)) * k],
        [(a(1, 2) * a(2, 0) - a(1, 0) * a(2, 2)) * k, (a(0, 0) * a(2, 2) - a(0, 2) * a(2, 0)) * k, (a(1, 0) * a(0, 2) - a(0, 0) * a(1, 2)) * k],
        [(a(1, 0) * a(2, 1) - a(2, 0) * a(1, 1)) * k, (a(2, 0) * a(0, 1) - a(0, 0) * a(2, 1)) * k, (a(0, 0) * a(1, 1) - a(1, 0) * a(0, 1)) * k],
    ];
    let t = [m[12], m[13], m[14]];
    let mut r = [0f32; 16];
    for row in 0..3 {
        for col in 0..3 {
            r[col * 4 + row] = inv[row][col];
        }
        r[12 + row] = -(inv[row][0] * t[0] + inv[row][1] * t[1] + inv[row][2] * t[2]);
    }
    r[15] = 1.0;
    Some(r)
}

pub fn mul(a: &M4, b: &M4) -> M4 {
    let mut m = [0f32; 16];
    for c in 0..4 {
        for r in 0..4 {
            m[c * 4 + r] = (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum();
        }
    }
    m
}

pub const IDENTITY: M4 = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

/// Between two rotations, normalised (fine for the small steps between frames and blends).
pub fn nlerp(a: [f32; 4], b: [f32; 4], f: f32) -> [f32; 4] {
    let dot: f32 = (0..4).map(|i| a[i] * b[i]).sum();
    let sign = if dot < 0.0 { -1.0 } else { 1.0 };
    let q: [f32; 4] = std::array::from_fn(|i| a[i] + (b[i] * sign - a[i]) * f);
    let n = q.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-12);
    q.map(|v| v / n)
}

#[derive(Clone, Debug)]
pub struct Joint {
    pub name: String,
    pub parent: i32,
    pub rest: Trs,
    pub inverse_bind: M4,
}

/// A bone's tracks in a clip: which of translation (bit 0), rotation (1), scale (2) it moves, each frame's.
#[derive(Clone, Debug)]
pub struct Track {
    pub joint: u16,
    pub what: u8,
    pub frames: Vec<Trs>,
}

/// What a clip says happens at a time in it (CS2's clip events): a sound event (kind 0), a particle (1).
#[derive(Clone, Debug, PartialEq)]
pub struct ClipEvent {
    pub time: f32,
    pub kind: u8,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct Clip {
    pub name: String,
    pub fps: f32,
    pub frames: u32,
    pub looping: bool,
    pub tracks: Vec<Track>,
    pub events: Vec<ClipEvent>,
}

impl Clip {
    /// How long it lasts (s).
    pub fn length(&self) -> f32 {
        (self.frames.saturating_sub(1)) as f32 / self.fps.max(1e-6)
    }

    /// Its pose at `t` (s; past its end, its last frame, or round when it loops) over `pose` (the bones it does not move
    /// keep theirs).
    pub fn sample_into(&self, t: f32, pose: &mut [Trs]) {
        let last = self.frames.saturating_sub(1) as f32;
        let mut x = t * self.fps;
        if self.looping && last > 0.0 {
            x = x.rem_euclid(last);
        }
        let x = x.clamp(0.0, last);
        let k = (x.floor() as usize).min(self.frames.saturating_sub(1) as usize);
        let k2 = (k + 1).min(self.frames.saturating_sub(1) as usize);
        let f = x - k as f32;
        for tr in &self.tracks {
            let Some(p) = pose.get_mut(tr.joint as usize) else { continue };
            let v = Trs::lerp(&tr.frames[k], &tr.frames[k2], f);
            if tr.what & 1 != 0 {
                p.t = v.t;
            }
            if tr.what & 2 != 0 {
                p.r = v.r;
            }
            if tr.what & 4 != 0 {
                p.s = v.s;
            }
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Model {
    /// The vertices as the GPU takes them (VERTEX bytes each).
    pub vertices: Vec<u8>,
    pub indices: Vec<u32>,
    pub parts: Vec<Part>,
    pub materials: Vec<Material>,
    pub joints: Vec<Joint>,
    pub clips: Vec<Clip>,
}

struct Reader<'a> {
    d: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.at.checked_add(n).filter(|&e| e <= self.d.len()).ok_or("the model ends early")?;
        let s = &self.d[self.at..end];
        self.at = end;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn i32(&mut self) -> Result<i32, String> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn f32(&mut self) -> Result<f32, String> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn fs<const N: usize>(&mut self) -> Result<[f32; N], String> {
        let mut out = [0f32; N];
        for v in out.iter_mut() {
            *v = self.f32()?;
        }
        Ok(out)
    }
    fn str(&mut self) -> Result<String, String> {
        let n = self.u16()? as usize;
        Ok(String::from_utf8_lossy(self.take(n)?).into_owned())
    }
}

/// The most of each a model may hold (a broken file is refused before it asks for gigabytes).
const MOST: u32 = 1 << 22;

impl Model {
    pub fn parse(d: &[u8]) -> Result<Model, String> {
        let mut r = Reader { d, at: 0 };
        if r.take(4)? != b"RCM1" {
            return Err("not a cosmetic model".into());
        }
        let counts: Vec<u32> = (0..6).map(|_| r.u32()).collect::<Result<_, _>>()?;
        if counts.iter().any(|&c| c > MOST) {
            return Err("the model is too big".into());
        }
        let [nv, ni, np, nm, nj, nc] = [counts[0], counts[1], counts[2], counts[3], counts[4], counts[5]].map(|c| c as usize);
        let vertices = r.take(nv * VERTEX)?.to_vec();
        let indices = (0..ni).map(|_| r.u32()).collect::<Result<Vec<_>, _>>()?;
        if indices.iter().any(|&i| i as usize >= nv) {
            return Err("an index past the vertices".into());
        }
        let parts = (0..np)
            .map(|_| Ok(Part { first: r.u32()?, count: r.u32()?, material: r.u32()? }))
            .collect::<Result<Vec<_>, String>>()?;
        let materials = (0..nm)
            .map(|_| Ok(Material { name: r.str()?, base: r.str()?, normal: r.str()?, packed: r.str()? }))
            .collect::<Result<Vec<_>, String>>()?;
        let mut joints = Vec::with_capacity(nj);
        for _ in 0..nj {
            let name = r.str()?;
            let parent = r.i32()?;
            let rest = Trs { t: r.fs()?, r: r.fs()?, s: r.fs()? };
            let inverse_bind = r.fs()?;
            joints.push(Joint { name, parent, rest, inverse_bind });
        }
        // parents come before their children (the order VRF writes; every pose is walked once in order)
        for (k, j) in joints.iter().enumerate() {
            if j.parent >= k as i32 {
                return Err(format!("joint {k}'s parent comes after it"));
            }
        }
        let mut clips = Vec::with_capacity(nc);
        for _ in 0..nc {
            let name = r.str()?;
            let fps = r.f32()?;
            let frames = r.u32()?;
            let looping = r.u8()? != 0;
            let nt = r.u32()?;
            if frames == 0 || frames > MOST || nt > nj as u32 {
                return Err(format!("clip {name}: bad sizes"));
            }
            let mut tracks = Vec::with_capacity(nt as usize);
            for _ in 0..nt {
                let joint = r.u16()?;
                let what = r.u8()?;
                if joint as usize >= nj {
                    return Err(format!("clip {name}: a track of no joint"));
                }
                let rest = joints[joint as usize].rest;
                let mut fr = Vec::with_capacity(frames as usize);
                for _ in 0..frames {
                    let mut p = rest;
                    if what & 1 != 0 {
                        p.t = r.fs()?;
                    }
                    if what & 2 != 0 {
                        p.r = r.fs()?;
                    }
                    if what & 4 != 0 {
                        p.s = r.fs()?;
                    }
                    fr.push(p);
                }
                tracks.push(Track { joint, what, frames: fr });
            }
            clips.push(Clip { name, fps, frames, looping, tracks, events: Vec::new() });
        }
        // the clips' events after them, when there are any ("EVT1": each its clip, time, kind, name)
        if r.d.len() >= r.at + 8 && &r.d[r.at..r.at + 4] == b"EVT1" {
            r.take(4)?;
            let n = r.u32()?;
            if n > MOST {
                return Err("too many clip events".into());
            }
            for _ in 0..n {
                let clip = r.u16()? as usize;
                let time = r.f32()?;
                let kind = r.u8()?;
                let name = r.str()?;
                if let Some(c) = clips.get_mut(clip) {
                    c.events.push(ClipEvent { time, kind, name });
                }
            }
        }
        Ok(Model { vertices, indices, parts, materials, joints, clips })
    }

    pub fn clip(&self, name: &str) -> Option<&Clip> {
        self.clips.iter().find(|c| c.name == name)
    }

    pub fn joint(&self, name: &str) -> Option<usize> {
        self.joints.iter().position(|j| j.name == name)
    }

    /// The box round its vertices as `skin` places them (each by its joints and weights, as the vertex shader does) —
    /// only those its triangles draw (a material left out keeps its vertices).
    pub fn bounds_skinned(&self, skin: &[M4]) -> ([f32; 3], [f32; 3]) {
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        for q in self.skinned(skin) {
            for k in 0..3 {
                lo[k] = lo[k].min(q[k]);
                hi[k] = hi[k].max(q[k]);
            }
        }
        (lo, hi)
    }

    /// Its vertices its triangles draw, as `skin` places them.
    pub fn skinned(&self, skin: &[M4]) -> Vec<[f32; 3]> {
        let n = self.vertices.len() / VERTEX;
        let mut drawn = vec![false; n];
        for &i in &self.indices {
            if let Some(d) = drawn.get_mut(i as usize) {
                *d = true;
            }
        }
        let mut out = Vec::with_capacity(n);
        for i in (0..n).filter(|&i| drawn[i]) {
            let p = self.position(i);
            let o = i * VERTEX;
            let mut q = [0f32; 3];
            let mut total = 0.0;
            for k in 0..4 {
                let w = self.vertices[o + 52 + k] as f32 / 255.0;
                let Some(m) = skin.get(self.vertices[o + 48 + k] as usize).filter(|_| w > 0.0) else { continue };
                for r in 0..3 {
                    q[r] += w * (m[r] * p[0] + m[4 + r] * p[1] + m[8 + r] * p[2] + m[12 + r]);
                }
                total += w;
            }
            out.push(if total > 0.0 { q.map(|x| x / total) } else { p });
        }
        out
    }

    /// The rest pose.
    pub fn rest(&self) -> Vec<Trs> {
        self.joints.iter().map(|j| j.rest).collect()
    }

    /// Each joint's place in the model for `pose`.
    pub fn globals(&self, pose: &[Trs]) -> Vec<M4> {
        let mut g: Vec<M4> = Vec::with_capacity(self.joints.len());
        for (k, j) in self.joints.iter().enumerate() {
            let local = pose.get(k).unwrap_or(&j.rest).matrix();
            let m = if j.parent >= 0 { mul(&g[j.parent as usize], &local) } else { local };
            g.push(m);
        }
        g
    }

    /// The skin's matrices for `pose` (what the vertex shader takes: each joint's place times its inverse bind).
    pub fn skin(&self, pose: &[Trs]) -> Vec<M4> {
        self.globals(pose).iter().zip(&self.joints).map(|(g, j)| mul(g, &j.inverse_bind)).collect()
    }

    /// For each of this model's joints, the joint of `other` (a clip's skeleton) of the same name (-1 none).
    pub fn map_to(&self, other: &Model) -> Vec<i32> {
        self.joints.iter().map(|j| other.joints.iter().position(|o| o.name == j.name).map_or(-1, |k| k as i32)).collect()
    }

    /// This model's joints' places for another skeleton's (`other_world`, by `map` from map_to), as VRF's
    /// SkeletonRetargeter does: a joint the other has takes its place there, the others follow their parent at rest
    /// (a root under `root`).
    pub fn world_from(&self, map: &[i32], other_world: &[M4], root: &M4) -> Vec<M4> {
        let mut w: Vec<M4> = Vec::with_capacity(self.joints.len());
        for (k, j) in self.joints.iter().enumerate() {
            let m = match map.get(k).copied().filter(|&o| o >= 0).and_then(|o| other_world.get(o as usize)) {
                Some(o) => *o,
                None => {
                    let parent = if j.parent >= 0 { w[j.parent as usize] } else { *root };
                    mul(&parent, &j.rest.matrix())
                }
            };
            w.push(m);
        }
        w
    }

    /// The skin's matrices for the joints' places in the model (each times its inverse bind).
    pub fn skin_of(&self, world: &[M4]) -> Vec<M4> {
        world.iter().zip(&self.joints).map(|(g, j)| mul(g, &j.inverse_bind)).collect()
    }

    /// The vertex `i`'s position (model space, as bound).
    pub fn position(&self, i: usize) -> [f32; 3] {
        let o = i * VERTEX;
        std::array::from_fn(|k| f32::from_le_bytes(self.vertices[o + k * 4..o + k * 4 + 4].try_into().unwrap()))
    }

    /// A model of bare triangles (one material, no joints): for tests.
    #[cfg(test)]
    pub fn set_triangles(&mut self, tris: &[[[f32; 3]; 3]]) {
        self.vertices.clear();
        self.indices.clear();
        for t in tris {
            for p in t {
                let mut v = vec![0u8; VERTEX];
                for (k, x) in p.iter().enumerate() {
                    v[k * 4..k * 4 + 4].copy_from_slice(&x.to_le_bytes());
                }
                self.indices.push((self.vertices.len() / VERTEX) as u32);
                self.vertices.extend_from_slice(&v);
            }
        }
        self.parts = vec![Part { first: 0, count: self.indices.len() as u32, material: 0 }];
    }

    /// The vertex `i`'s normal (as bound).
    pub fn normal(&self, i: usize) -> [f32; 3] {
        let o = i * VERTEX + 12;
        std::array::from_fn(|k| f32::from_le_bytes(self.vertices[o + k * 4..o + k * 4 + 4].try_into().unwrap()))
    }

    /// The vertex `i`'s texture coordinates.
    pub fn uv(&self, i: usize) -> [f32; 2] {
        let o = i * VERTEX + 40;
        std::array::from_fn(|k| f32::from_le_bytes(self.vertices[o + k * 4..o + k * 4 + 4].try_into().unwrap()))
    }

    /// The box round its vertices as bound (min, max).
    pub fn bounds(&self) -> ([f32; 3], [f32; 3]) {
        let n = self.vertices.len() / VERTEX;
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        for i in 0..n {
            let p = self.position(i);
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        (lo, hi)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn an_affine_matrix_and_its_inverse_undo_each_other() {
        // turned a quarter about y, scaled 0.7, moved
        let m: M4 = [0.0, 0.0, -0.7, 0.0, 0.0, 0.7, 0.0, 0.0, 0.7, 0.0, 0.0, 0.0, 0.3, -1.2, 2.5, 1.0];
        let back = inverse(&m).unwrap();
        let both = mul(&back, &m);
        for (k, v) in both.iter().enumerate() {
            assert!((v - IDENTITY[k]).abs() < 1e-5, "{both:?}");
        }
        assert_eq!(inverse(&[0.0; 16]), None);
    }

    fn s(out: &mut Vec<u8>, v: &str) {
        out.extend_from_slice(&(v.len() as u16).to_le_bytes());
        out.extend_from_slice(v.as_bytes());
    }

    /// A model of two joints (the second a unit above the first) and one triangle on the second, with a clip that
    /// turns the root a quarter round about z over a second.
    pub(crate) fn tiny() -> Vec<u8> {
        let mut d = b"RCM1".to_vec();
        for c in [3u32, 3, 1, 1, 2, 1] {
            d.extend_from_slice(&c.to_le_bytes());
        }
        for p in [[0.0f32, 1.0, 0.0], [1.0, 1.0, 0.0], [0.0, 2.0, 0.0]] {
            for v in p.iter().chain(&[0.0, 0.0, 1.0]).chain(&[1.0, 0.0, 0.0, 1.0]).chain(&[0.0, 0.0]) {
                d.extend_from_slice(&v.to_le_bytes());
            }
            d.extend_from_slice(&[1, 0, 0, 0, 255, 0, 0, 0]);
        }
        for i in [0u32, 1, 2] {
            d.extend_from_slice(&i.to_le_bytes());
        }
        for v in [0u32, 3, 0] {
            d.extend_from_slice(&v.to_le_bytes());
        }
        s(&mut d, "m");
        s(&mut d, "base.png");
        s(&mut d, "");
        s(&mut d, "");
        // joints: root at the origin; its child a unit up (inverse bind: down a unit)
        for (name, parent, t, ib_y) in [("root", -1i32, [0.0f32, 0.0, 0.0], 0.0f32), ("arm", 0, [0.0, 1.0, 0.0], -1.0)] {
            s(&mut d, name);
            d.extend_from_slice(&parent.to_le_bytes());
            for v in t.iter().chain(&[0.0, 0.0, 0.0, 1.0]).chain(&[1.0, 1.0, 1.0]) {
                d.extend_from_slice(&v.to_le_bytes());
            }
            let mut ib = IDENTITY;
            ib[13] = ib_y;
            for v in ib {
                d.extend_from_slice(&v.to_le_bytes());
            }
        }
        s(&mut d, "turn");
        d.extend_from_slice(&1.0f32.to_le_bytes());
        d.extend_from_slice(&2u32.to_le_bytes());
        d.push(0);
        d.extend_from_slice(&1u32.to_le_bytes());
        d.extend_from_slice(&0u16.to_le_bytes());
        d.push(2);
        let h = std::f32::consts::FRAC_1_SQRT_2;
        for q in [[0.0f32, 0.0, 0.0, 1.0], [0.0, 0.0, h, h]] {
            for v in q {
                d.extend_from_slice(&v.to_le_bytes());
            }
        }
        d
    }

    fn apply(m: &M4, p: [f32; 3]) -> [f32; 3] {
        std::array::from_fn(|r| m[r] * p[0] + m[4 + r] * p[1] + m[8 + r] * p[2] + m[12 + r])
    }

    #[test]
    fn a_model_is_read_and_posed() {
        let m = Model::parse(&tiny()).unwrap();
        assert_eq!((m.vertices.len() / VERTEX, m.indices.len(), m.joints.len(), m.clips.len()), (3, 3, 2, 1));
        assert_eq!(m.materials[0].base, "base.png");
        // at rest the skin leaves the vertices where they were bound
        let rest = m.skin(&m.rest());
        let p = apply(&rest[1], m.position(1));
        assert!((p[0] - 1.0).abs() < 1e-5 && (p[1] - 1.0).abs() < 1e-5, "{p:?}");
        // the clip's end: the root turned a quarter round z — the arm's vertex (1, 1) goes to (-1, 1)
        let c = m.clip("turn").unwrap();
        assert!((c.length() - 1.0).abs() < 1e-6);
        let mut pose = m.rest();
        c.sample_into(1.0, &mut pose);
        let p = apply(&m.skin(&pose)[1], m.position(1));
        assert!((p[0] + 1.0).abs() < 1e-4 && (p[1] - 1.0).abs() < 1e-4, "{p:?}");
        // half way: an eighth round
        c.sample_into(0.5, &mut pose);
        let p = apply(&m.skin(&pose)[1], [0.0, 1.0, 0.0]);
        let h = std::f32::consts::FRAC_1_SQRT_2;
        assert!((p[0] + h).abs() < 1e-3 && (p[1] - h).abs() < 1e-3, "{p:?}");
    }

    #[test]
    fn a_model_takes_a_clips_places_by_name() {
        // the clip's skeleton: the tiny model's own (its "arm" turned by the clip); a model of the same joints
        let m = Model::parse(&tiny()).unwrap();
        let map = m.map_to(&m);
        assert_eq!(map, vec![0, 1]);
        let mut pose = m.rest();
        m.clip("turn").unwrap().sample_into(1.0, &mut pose);
        let world = m.globals(&pose);
        let w = m.world_from(&map, &world, &IDENTITY);
        assert_eq!(w, world);
        // a joint the clip lacks follows its parent at rest
        let none = vec![0, -1];
        let w = m.world_from(&none, &world, &IDENTITY);
        let p = apply(&w[1], [0.0, 0.0, 0.0]);
        assert!((p[0] + 1.0).abs() < 1e-4 && p[1].abs() < 1e-4, "{p:?}");
        let skin = m.skin_of(&w);
        assert_eq!(skin.len(), 2);
    }

    #[test]
    fn a_broken_model_is_refused() {
        let d = tiny();
        assert!(Model::parse(&d[..d.len() - 3]).is_err());
        assert!(Model::parse(b"RCM0").is_err());
        let mut bad = d.clone();
        // an index past the vertices
        let at = 4 + 24 + 3 * VERTEX;
        bad[at..at + 4].copy_from_slice(&9u32.to_le_bytes());
        assert!(Model::parse(&bad).is_err());
    }

    /// The converter's own output for a real model, when it is on this machine (Dota's Tinker, tools/cs2pack/rcm.py).
    #[test]
    fn a_converted_model_reads() {
        let Ok(d) = std::fs::read("D:/CS2/work/dota/tinker.rcm") else { return };
        let m = Model::parse(&d).unwrap();
        assert_eq!(m.joints.len(), 45);
        let c = m.clip("run_anim").unwrap();
        let mut pose = m.rest();
        c.sample_into(0.4, &mut pose);
        let skin = m.skin(&pose);
        assert!(skin.iter().all(|s| s.iter().all(|v| v.is_finite())));
        let (lo, hi) = m.bounds();
        assert!(hi[1] > lo[1]);
    }
}
