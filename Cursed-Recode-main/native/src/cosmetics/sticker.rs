//! CS2's stickers on a weapon (and, ours only, on a knife): where they lie — CS2's markup of the model
//! (items/<item>/markup.txt) has its slots in a flat picture of the weapon's side (each slot's middle, how many stickers
//! go across the picture, its turn; the polygons a sticker may be moved within), and where that picture lies on the
//! body; a sticker is pressed onto the body from that side, as CS2 lays them — and how each looks: its
//! csgo_weapon_sticker material's switches, values and textures (stickers/<material>.txt, pack_stickers.py). The look
//! is drawn by shaders/cosmetics/sticker.frag over the body, CS2's own sticker shader's surface carried over.

use super::model::Model;
use super::render::StickerDraw;
use super::store;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

/// At most so many on an item (CS2's).
pub const MAX: usize = 5;

/// A slot of a body's markup: its index, its middle (the markup's own u along the weapon and v down), how many
/// stickers across the picture, its turn (degrees), its use (Map, Team1, Team2, Autograph).
#[derive(Clone, Debug, PartialEq)]
pub struct Slot {
    pub legacy: bool,
    pub index: u8,
    pub uv: [f32; 2],
    pub scale: f32,
    pub rotation: f32,
    pub id: String,
}

/// A place a charm may hang (CS2's charm placement): a quad of a body, its corners in a bone's own frame (inches; a
/// grid's order: 0 1 / 2 3), whether it holds the charm's own place.
#[derive(Clone, Debug, PartialEq)]
pub struct CharmPlace {
    pub legacy: bool,
    pub bone: String,
    pub corners: [[f32; 3]; 4],
    pub default: bool,
}

/// A model's markup: its stickers' slots, the polygons they may be moved within (by body), where its picture lies on
/// each body (z = z0 − u·l, y = y0 − v·l in the model: z along the barrel, y up; the stickers on its +x side), the
/// places its charm may hang.
#[derive(Clone, Debug, Default)]
pub struct Markup {
    pub slots: Vec<Slot>,
    pub polys: Vec<(bool, Vec<[f32; 2]>)>,
    pub maps: Vec<(bool, [f32; 3])>,
    pub charms: Vec<CharmPlace>,
}

impl Markup {
    pub fn parse(text: &str) -> Markup {
        let mut m = Markup::default();
        for line in text.lines() {
            let f: Vec<&str> = line.split_whitespace().collect();
            let num = |i: usize| f.get(i).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
            match f.first().copied() {
                Some("slot") if f.len() >= 8 => m.slots.push(Slot {
                    legacy: f[1] == "legacy",
                    index: f[2].parse().unwrap_or(0),
                    uv: [num(3), num(4)],
                    scale: num(5).max(1.0),
                    rotation: num(6),
                    id: f[7].to_string(),
                }),
                Some("poly") if f.len() >= 8 => {
                    let v: Vec<f32> = f[2..].iter().filter_map(|x| x.parse().ok()).collect();
                    m.polys.push((f[1] == "legacy", v.chunks_exact(2).map(|c| [c[0], c[1]]).collect()));
                }
                Some("map") if f.len() >= 5 => m.maps.push((f[1] == "legacy", [num(2), num(3), num(4)])),
                Some("charm") if f.len() >= 15 => m.charms.push(CharmPlace {
                    legacy: f[1] == "legacy",
                    bone: f[2].to_string(),
                    corners: std::array::from_fn(|c| std::array::from_fn(|k| num(3 + c * 3 + k))),
                    default: f.get(15) == Some(&"default"),
                }),
                _ => {}
            }
        }
        m.slots.sort_by_key(|s| (s.legacy, s.index));
        m
    }

    /// A body's slots, in their order.
    pub fn slots_of(&self, legacy: bool) -> impl Iterator<Item = &Slot> {
        self.slots.iter().filter(move |s| s.legacy == legacy)
    }

    /// The slot `index` of a body, else its first.
    pub fn slot(&self, legacy: bool, index: u8) -> Option<&Slot> {
        self.slots_of(legacy).find(|s| s.index == index).or_else(|| self.slots_of(legacy).next())
    }

    /// Whether a point of the markup lies where a sticker may be (the body's polygons: triangles, three corners each).
    pub fn allowed(&self, legacy: bool, uv: [f32; 2]) -> bool {
        let polys: Vec<&Vec<[f32; 2]>> = self.polys.iter().filter(|(l, _)| *l == legacy).map(|(_, p)| p).collect();
        polys.is_empty() || polys.iter().any(|p| p.chunks_exact(3).any(|t| in_triangle(uv, t[0], t[1], t[2])))
    }

    /// Where the markup's picture lies on a body (z0, y0, l), when it says.
    pub fn map(&self, legacy: bool) -> Option<[f32; 3]> {
        self.maps.iter().find(|(l, _)| *l == legacy).map(|(_, m)| *m)
    }

    /// A point of the markup on the body's side (z along the barrel, y up).
    pub fn to_model(&self, legacy: bool, uv: [f32; 2]) -> Option<[f32; 2]> {
        let [z0, y0, l] = self.map(legacy)?;
        Some([z0 - uv[0] * l, y0 - uv[1] * l])
    }

    /// A point of the body's side (z, y) in the markup.
    pub fn to_markup(&self, legacy: bool, zy: [f32; 2]) -> Option<[f32; 2]> {
        let [z0, y0, l] = self.map(legacy)?;
        Some([(z0 - zy[0]) / l, (y0 - zy[1]) / l])
    }
}

/// `p`'s weights in the triangle a, b, c (none when the triangle has no area).
fn barycentric(p: [f32; 2], a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> Option<[f32; 3]> {
    let det = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
    if det.abs() < 1e-12 {
        return None;
    }
    let w0 = ((b[1] - c[1]) * (p[0] - c[0]) + (c[0] - b[0]) * (p[1] - c[1])) / det;
    let w1 = ((c[1] - a[1]) * (p[0] - c[0]) + (a[0] - c[0]) * (p[1] - c[1])) / det;
    Some([w0, w1, 1.0 - w0 - w1])
}

/// Whether `p` lies in the triangle a, b, c (one with no area holds nothing).
pub fn in_triangle(p: [f32; 2], a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> bool {
    barycentric(p, a, b, c).is_some_and(|w| w.iter().all(|&x| x >= -1e-6))
}

/// The markup of an item's model (its folder: "items/weapon_awp"), read once.
pub fn markup(dir: &str) -> Arc<Markup> {
    static KEPT: OnceLock<Mutex<HashMap<String, Arc<Markup>>>> = OnceLock::new();
    let kept = KEPT.get_or_init(Default::default);
    if let Some(m) = kept.lock().unwrap().get(dir) {
        return m.clone();
    }
    let key = format!("{dir}/markup.txt");
    let text = store::read(&key).map(|b| String::from_utf8_lossy(&b).into_owned());
    let m = Arc::new(Markup::parse(text.as_deref().unwrap_or_default()));
    // (kept once had, or once it never will be)
    if text.is_some() || !store::coming(&key) {
        kept.lock().unwrap().insert(dir.to_string(), m.clone());
    }
    m
}

/// Where a ray (the model as bound) first meets a model's triangles of the materials `on`: how far along it, the point,
/// the triangle's normal turned toward where the ray came from.
pub fn raycast(m: &Model, on: &[u32], origin: [f32; 3], dir: [f32; 3]) -> Option<(f32, [f32; 3], [f32; 3])> {
    let mut best: Option<(f32, [f32; 3], [f32; 3])> = None;
    for part in m.parts.iter().filter(|p| on.contains(&p.material)) {
        for t in m.indices[part.first as usize..(part.first + part.count) as usize].chunks_exact(3) {
            let [a, b, c] = [t[0], t[1], t[2]].map(|i| m.position(i as usize));
            // Möller–Trumbore, both faces
            let (e1, e2) = (sub(b, a), sub(c, a));
            let pv = cross(dir, e2);
            let det = dot(e1, pv);
            if det.abs() < 1e-12 {
                continue;
            }
            let inv = 1.0 / det;
            let tv = sub(origin, a);
            let u = dot(tv, pv) * inv;
            if !(0.0..=1.0).contains(&u) {
                continue;
            }
            let qv = cross(tv, e1);
            let v = dot(dir, qv) * inv;
            if v < 0.0 || u + v > 1.0 {
                continue;
            }
            let d = dot(e2, qv) * inv;
            if d > 1e-6 && best.as_ref().is_none_or(|(bd, ..)| d < *bd) {
                let mut n = unit(cross(e1, e2));
                if dot(n, dir) > 0.0 {
                    n = scale(n, -1.0);
                }
                best = Some((d, std::array::from_fn(|k| origin[k] + dir[k] * d), n));
            }
        }
    }
    best
}

/// Where a slot of a body's markup puts a sticker: pressed onto the body (its materials `on`) from its +x side at the
/// slot's middle, as wide as the slot's share of the picture, turned as the slot says.
pub fn slot_place(m: &Model, on: &[u32], mk: &Markup, slot: &Slot) -> Option<Placed> {
    let [z, y] = mk.to_model(slot.legacy, slot.uv)?;
    let l = mk.map(slot.legacy)?[2];
    let x = m.bounds().1[0] + 0.1;
    let (_, at, _) = raycast(m, on, [x, y, z], [-1.0, 0.0, 0.0])?;
    let normal = [1.0, 0.0, 0.0];
    Some(Placed {
        name: String::new(),
        at,
        normal,
        up: turned([0.0, 1.0, 0.0], normal, -slot.rotation),
        size: l / slot.scale,
        wear: 0.0,
    })
}

/// A sticker put on an item: which (its kit's name), where (the model as bound: its middle, its normal out of the
/// body, its up along the body), how big (its width, metres), how worn (0..1).
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    pub name: String,
    pub at: [f32; 3],
    pub normal: [f32; 3],
    pub up: [f32; 3],
    pub size: f32,
    pub wear: f32,
}

impl Placed {
    /// A separate copy on the opposite body surface; asymmetric models need a ray, not a negated coordinate.
    pub fn opposite(&self, m: &Model, on: &[u32], rifle: bool) -> Option<Placed> {
        if !self.at.iter().chain(&self.normal).chain(&self.up).all(|v| v.is_finite()) || dot(self.normal, self.normal) < 1e-8 {
            return None;
        }
        let n = if rifle { [self.normal[0].signum(), 0.0, 0.0] } else { unit(self.normal) };
        let (lo, hi) = m.bounds();
        let span = sub(hi, lo);
        let reach = dot(span, span).sqrt() + 0.1;
        let origin = sub(self.at, scale(n, reach));
        let (_, at, surface) = raycast(m, on, origin, n)?;
        if dot(sub(at, self.at), n) >= -1e-6 {
            return None;
        }
        let normal = if rifle { scale(n, -1.0) } else { surface };
        let mut up = sub(self.up, scale(normal, dot(self.up, normal)));
        if dot(up, up) < 1e-8 {
            let base = if normal[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [0.0, 0.0, 1.0] };
            up = sub(base, scale(normal, dot(base, normal)));
        }
        Some(Placed {
            at,
            normal,
            up: unit(up),
            ..self.clone()
        })
    }

    /// Back-side stickers must not intercept pointer actions on the visible side.
    pub fn faces_ray(&self, dir: [f32; 3]) -> bool {
        dot(self.normal, dir) < -1e-5
    }

    /// As the loadout keeps it: "name@x,y,z,nx,ny,nz,ux,uy,uz,size,wear".
    pub fn dump(&self) -> String {
        let n: Vec<String> = self
            .at
            .iter()
            .chain(&self.normal)
            .chain(&self.up)
            .chain([&self.size, &self.wear])
            .map(|v| format!("{v:.5}"))
            .collect();
        format!("{}@{}", self.name, n.join(","))
    }

    pub fn parse(s: &str) -> Option<Placed> {
        let (name, rest) = s.split_once('@')?;
        let v: Vec<f32> = rest.split(',').filter_map(|x| x.parse().ok()).collect();
        if name.is_empty() || v.len() < 11 {
            return None;
        }
        Some(Placed {
            name: name.to_string(),
            at: [v[0], v[1], v[2]],
            normal: [v[3], v[4], v[5]],
            up: [v[6], v[7], v[8]],
            size: v[9].max(1e-4),
            wear: v[10].clamp(0.0, 1.0),
        })
    }
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scale(a: [f32; 3], k: f32) -> [f32; 3] {
    a.map(|x| x * k)
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn unit(a: [f32; 3]) -> [f32; 3] {
    scale(a, 1.0 / dot(a, a).sqrt().max(1e-12))
}

/// `v` turned about the unit axis `k` by `deg` degrees.
pub fn turned(v: [f32; 3], k: [f32; 3], deg: f32) -> [f32; 3] {
    let (s, c) = deg.to_radians().sin_cos();
    let kv = cross(k, v);
    let kd = dot(k, v);
    std::array::from_fn(|i| v[i] * c + kv[i] * s + k[i] * kd * (1.0 - c))
}

/// The bits of sticker.frag's switches a look's material sets (and the shader's defaults where it says nothing).
fn flags(l: &Look) -> u32 {
    let f = |n: &str, or: bool| l.flags.get(n).copied().unwrap_or(or);
    [
        f("g_bHolographicSticker0", false),
        f("g_bMetallicSticker0", false),
        f("g_bGlitterSticker0", false),
        f("g_bPaperBackingSticker0", false),
        f("g_bAutomaticPBRColorFittingSticker0", true),
        f("g_bLegacyTintMultiplySticker0", false),
        f("g_bSelfIllumSticker0", false),
        f("g_bClampSpectrumVSticker0", true),
        f("g_bPreserveRoughnessSticker0", false),
    ]
    .iter()
    .enumerate()
    .fold(0, |acc, (i, &on)| acc | ((on as u32) << i))
}

/// A sticker put on a body (its materials by name) as the renderer draws it: none while its look is not read.
pub fn draw(p: &Placed, materials: Vec<String>) -> Option<StickerDraw> {
    let cat = super::catalog::get();
    let material = cat.stickers.iter().find(|s| s.name == p.name)?.material.clone();
    let l = look(&material)?;
    l.want();
    let n = unit(p.normal);
    let up = unit(sub(p.up, scale(n, dot(p.up, n))));
    let across = cross(up, n);
    let size = p.size.max(1e-4);
    let tex = |k: &str| l.textures.get(k).cloned().unwrap_or_default();
    let tint = l.vec("g_vColorTintSticker0", [1.0, 1.0, 1.0, 0.0]);
    let bias = l.vec("g_vWearBiasSticker0", [1.0, 1.0, 0.0, 0.0]);
    Some(StickerDraw {
        materials,
        projection: [
            [p.at[0], p.at[1], p.at[2], size * 0.6],
            [across[0] / size, across[1] / size, across[2] / size, 0.0],
            [up[0] / size, up[1] / size, up[2] / size, 0.0],
            [n[0], n[1], n[2], 0.0],
        ],
        flags: flags(&l),
        wear: p.wear,
        color_boost: l.float("g_flColorBoostSticker0", 1.0),
        sfx_boost: l.float("g_flSfxColorBoostSticker0", 1.0),
        tint: [tint[0], tint[1], tint[2], l.float("g_flTintSaturateSticker0", 1.0)],
        wearing: [
            bias[0],
            bias[1],
            l.float("g_fWearScratchesSticker0", 1.0),
            l.float("g_flGlitterScaleSticker0", 1.0),
        ],
        textures: [
            "g_tSticker0",
            "g_tNormalRoughnessSticker0",
            "g_tSfxMaskSticker0",
            "g_tHoloSpectrumSticker0",
            "g_tGlitterNormalSticker0",
            "g_tStickerScratches",
        ]
        .map(tex),
    })
}

/// The materials of a model a sticker lies on: its body's, those a finish is worked onto (items/<item>/finish/<it>.txt).
pub fn body_materials(model_key: &str, m: &Model) -> Vec<(u32, String)> {
    static KEPT: OnceLock<Mutex<HashMap<String, Vec<(u32, String)>>>> = OnceLock::new();
    let kept = KEPT.get_or_init(Default::default);
    if let Some(v) = kept.lock().unwrap().get(model_key) {
        return v.clone();
    }
    let dir = model_key.rsplit_once('/').map_or("", |(d, _)| d);
    let v: Vec<(u32, String)> = m
        .materials
        .iter()
        .enumerate()
        .filter(|(_, mt)| store::exists(&format!("{dir}/finish/{}.txt", mt.name)))
        .map(|(i, mt)| (i as u32, mt.name.clone()))
        .collect();
    kept.lock().unwrap().insert(model_key.to_string(), v.clone());
    v
}

/// The draws of an item's stickers over its model (its key, the model read).
pub fn draws(model_key: &str, m: &Model, placed: &[Placed]) -> Vec<StickerDraw> {
    if placed.is_empty() {
        return Vec::new();
    }
    let names: Vec<String> = body_materials(model_key, m).into_iter().map(|(_, n)| n).collect();
    placed.iter().take(MAX).filter_map(|p| draw(p, names.clone())).collect()
}

/// How a sticker looks: its material's switches (g_b…), values (g_fl…, g_f…), vectors (g_v…), textures by name.
#[derive(Clone, Debug, Default)]
pub struct Look {
    pub flags: HashMap<String, bool>,
    pub floats: HashMap<String, f32>,
    pub vecs: HashMap<String, [f32; 4]>,
    pub textures: HashMap<String, String>,
}

impl Look {
    pub fn parse(text: &str) -> Look {
        let mut l = Look::default();
        for line in text.lines() {
            let f: Vec<&str> = line.split_whitespace().collect();
            match (f.first().copied(), f.get(1)) {
                (Some("flag"), Some(n)) => {
                    l.flags.insert(n.to_string(), f.get(2) == Some(&"1"));
                }
                (Some("float"), Some(n)) => {
                    l.floats.insert(n.to_string(), f.get(2).and_then(|v| v.parse().ok()).unwrap_or(0.0));
                }
                (Some("vec"), Some(n)) => {
                    let v: Vec<f32> = f[2..].iter().filter_map(|x| x.parse().ok()).collect();
                    l.vecs.insert(n.to_string(), std::array::from_fn(|k| v.get(k).copied().unwrap_or(0.0)));
                }
                (Some("tex"), Some(n)) => {
                    l.textures.insert(n.to_string(), f.get(2).unwrap_or(&"").to_string());
                }
                _ => {}
            }
        }
        l
    }

    pub fn flag(&self, n: &str) -> bool {
        self.flags.get(n).copied().unwrap_or(false)
    }

    pub fn float(&self, n: &str, or: f32) -> f32 {
        self.floats.get(n).copied().unwrap_or(or)
    }

    pub fn vec(&self, n: &str, or: [f32; 4]) -> [f32; 4] {
        self.vecs.get(n).copied().unwrap_or(or)
    }

    /// Its textures to read (each once: the store keeps what it read).
    pub fn want(&self) {
        for k in self.textures.values() {
            store::want(k);
        }
    }
}

/// A sticker's look by its material (its path under stickers/), read once; none while it is not there.
pub fn look(material: &str) -> Option<Arc<Look>> {
    static KEPT: OnceLock<Mutex<HashMap<String, Option<Arc<Look>>>>> = OnceLock::new();
    let kept = KEPT.get_or_init(Default::default);
    if let Some(l) = kept.lock().unwrap().get(material) {
        return l.clone();
    }
    let key = format!("stickers/{material}.txt");
    let l = store::read(&key).map(|b| Arc::new(Look::parse(&String::from_utf8_lossy(&b))));
    if l.is_some() || !store::coming(&key) {
        kept.lock().unwrap().insert(material.to_string(), l.clone());
    }
    l
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markup_reads_its_slots_and_polygons() {
        let m = Markup::parse("slot model 1 0.176 -0.394 19.6 0.0 Autograph\nslot model 0 0.337 -0.368 19.6 0.0 Map\nslot legacy 0 0.3 -0.3 22 0 Map\npoly model 0.0 -0.3 0.5 -0.3 0.0 -0.5\nmap model 0.4 -0.34 1.39\n");
        let s: Vec<u8> = m.slots_of(false).map(|s| s.index).collect();
        assert_eq!(s, vec![0, 1]);
        assert_eq!(m.slot(false, 0).unwrap().uv, [0.337, -0.368]);
        assert_eq!(m.slot(true, 4).unwrap().index, 0);
        assert!(m.allowed(false, [0.1, -0.35]));
        assert!(!m.allowed(false, [0.4, -0.45]));
        // a body with no polygons takes a sticker anywhere
        assert!(m.allowed(true, [0.9, 0.9]));
        // the picture on the body and back
        let zy = m.to_model(false, [0.337, -0.368]).unwrap();
        assert!((zy[0] - (0.4 - 0.337 * 1.39)).abs() < 1e-5 && (zy[1] - (-0.34 + 0.368 * 1.39)).abs() < 1e-5);
        let back = m.to_markup(false, zy).unwrap();
        assert!((back[0] - 0.337).abs() < 1e-5 && (back[1] + 0.368).abs() < 1e-5);
        assert_eq!(m.to_model(true, [0.0, 0.0]), None);
        let c = Markup::parse("charm model weapon_offset 1 2 3 4 5 6 7 8 9 10 11 12 default\ncharm legacy rail 0 0 0 0 0 0 0 0 0 0 0 0\n");
        assert_eq!(c.charms.len(), 2);
        assert!(c.charms[0].default && !c.charms[0].legacy && c.charms[0].corners[3] == [10.0, 11.0, 12.0]);
        assert!(c.charms[1].legacy && !c.charms[1].default && c.charms[1].bone == "rail");
    }

    #[test]
    fn a_ray_meets_the_nearest_triangle() {
        let mut m = Model::default();
        let tri = |x: f32| [[x, -1.0, -1.0], [x, 1.0, -1.0], [x, 0.0, 1.0]];
        m.set_triangles(&[tri(0.0), tri(0.5)]);
        let (d, at, n) = raycast(&m, &[0], [2.0, 0.0, 0.0], [-1.0, 0.0, 0.0]).unwrap();
        assert!((d - 1.5).abs() < 1e-5 && (at[0] - 0.5).abs() < 1e-5 && n[0] > 0.99, "{d} {at:?} {n:?}");
        assert!(raycast(&m, &[0], [2.0, 5.0, 0.0], [-1.0, 0.0, 0.0]).is_none());
    }

    #[test]
    fn a_placed_sticker_keeps_through_the_loadout() {
        let p = Placed {
            name: "dh_gologo1".into(),
            at: [0.01, 0.2, 0.11],
            normal: [1.0, 0.0, 0.0],
            up: [0.0, 1.0, 0.0],
            size: 0.04,
            wear: 0.25,
        };
        assert_eq!(Placed::parse(&p.dump()), Some(p));
        assert_eq!(Placed::parse("x@1,2"), None);
    }

    #[test]
    fn opposite_copy_uses_the_surface_and_remains_independent() {
        let mut m = Model::default();
        let tri = |x| [[x, -1.0, -1.0], [x, 1.0, -1.0], [x, 0.0, 1.0]];
        m.set_triangles(&[tri(-0.02), tri(0.05)]);
        let original = Placed {
            name: "test".into(),
            at: [0.05, 0.0, 0.0],
            normal: [1.0, 0.0, 0.0],
            up: [0.0, 0.8, 0.6],
            size: 0.04,
            wear: 0.25,
        };
        let mut copy = original.opposite(&m, &[0], true).unwrap();
        assert!((copy.at[0] + 0.02).abs() < 1e-5, "asymmetric body: {:?}", copy.at);
        assert_eq!(copy.normal, [-1.0, 0.0, 0.0]);
        assert_eq!(
            (copy.name.as_str(), copy.size, copy.wear),
            (original.name.as_str(), original.size, original.wear)
        );
        assert!(dot(copy.up, original.up) > 0.999);
        let restored = Placed::parse(&copy.dump()).unwrap();
        let back = restored.opposite(&m, &[0], true).unwrap();
        assert!((back.at[0] - original.at[0]).abs() < 1e-5);
        assert!(original.faces_ray([-1.0, 0.0, 0.0]) && !copy.faces_ray([-1.0, 0.0, 0.0]));
        assert!(!original.faces_ray([1.0, 0.0, 0.0]) && copy.faces_ray([1.0, 0.0, 0.0]));
        copy.at[1] = 0.2;
        copy.up = turned(copy.up, copy.normal, 35.0);
        copy.wear = 0.8;
        assert_eq!((original.at[1], original.up, original.wear), (0.0, [0.0, 0.8, 0.6], 0.25));
        assert_eq!(restored.normal, [-1.0, 0.0, 0.0]);
    }

    #[test]
    fn opposite_copy_handles_knives_and_missing_surfaces() {
        let mut m = Model::default();
        let tri = |y| [[-1.0, y, -1.0], [1.0, y, -1.0], [0.0, y, 1.0]];
        m.set_triangles(&[tri(-0.01), tri(0.02)]);
        let p = Placed {
            name: "test".into(),
            at: [0.0, 0.02, 0.0],
            normal: [0.0, 1.0, 0.0],
            up: [0.0, 0.0, 1.0],
            size: 0.04,
            wear: 0.0,
        };
        let copy = p.opposite(&m, &[0], false).unwrap();
        assert!((copy.at[1] + 0.01).abs() < 1e-5);
        assert!(copy.normal[1] < -0.99 && dot(copy.up, copy.normal).abs() < 1e-5);
        assert!(p.opposite(&m, &[99], false).is_none());
        m.set_triangles(&[tri(0.02)]);
        assert!(p.opposite(&m, &[0], false).is_none());
        assert!(Placed { normal: [0.0; 3], ..p.clone() }.opposite(&m, &[0], false).is_none());
        assert!(Placed { at: [f32::NAN, 0.0, 0.0], ..p }.opposite(&m, &[0], false).is_none());
    }

    #[test]
    fn a_turn_about_an_axis() {
        let v = turned([0.0, 1.0, 0.0], [1.0, 0.0, 0.0], 90.0);
        assert!((v[2] - 1.0).abs() < 1e-5 && v[1].abs() < 1e-5, "{v:?}");
    }

    #[test]
    fn a_look_reads_its_material() {
        let l = Look::parse(
            "flag g_bHolographicSticker0 1\nfloat g_flColorBoostSticker0 1.5\nvec g_vWearBiasSticker0 0.85 0 0 0\ntex g_tSticker0 stickers/tex/a.rtx\n",
        );
        assert!(l.flag("g_bHolographicSticker0"));
        assert!(!l.flag("g_bGlitterSticker0"));
        assert_eq!(l.float("g_flColorBoostSticker0", 1.0), 1.5);
        assert_eq!(l.vec("g_vWearBiasSticker0", [1.0; 4])[0], 0.85);
        assert_eq!(l.textures["g_tSticker0"], "stickers/tex/a.rtx");
    }

    #[test]
    #[ignore]
    fn probe_placements() {
        for item in ["weapon_awp", "weapon_ssg08"] {
            let m = Model::parse(&std::fs::read(format!(r"D:\CS2\work\out\items\{item}\model.rcm")).unwrap()).unwrap();
            let mk = Markup::parse(&std::fs::read_to_string(format!(r"D:\CS2\work\out\items\{item}\markup.txt")).unwrap());
            let on: Vec<u32> = m
                .materials
                .iter()
                .enumerate()
                .filter(|(_, mt)| std::path::Path::new(&format!(r"D:\CS2\work\out\items\{item}\finish\{}.txt", mt.name)).is_file())
                .map(|(i, _)| i as u32)
                .collect();
            let names = ["std_aces_high_holo", "std_luck_foil", "std_lemon", "std_fearsome"];
            let v: Vec<String> = mk
                .slots_of(false)
                .zip(names)
                .filter_map(|(s, n)| {
                    slot_place(&m, &on, &mk, s).map(|mut p| {
                        p.name = n.into();
                        let copy = p.opposite(&m, &on, true).expect("opposite body surface at the real CS sticker slot");
                        assert!(copy.normal[0] < -0.99 && copy.at[0] < p.at[0]);
                        assert!((copy.at[1] - p.at[1]).abs() < 1e-5 && (copy.at[2] - p.at[2]).abs() < 1e-5);
                        assert!(copy.opposite(&m, &on, true).is_some());
                        p.dump()
                    })
                })
                .collect();
            println!("{item} {}", v.join(";"));
        }
    }
}
