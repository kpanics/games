//! A finish worked onto a weapon as CS2 works it (its composite materials: weapons/paints/…vcompmat): the paint's
//! material (its style, colours, pattern…), the weapon's inputs (its masks, ambient occlusion, positions, own colour
//! and metalness), the item's own (its seed, its wear) — the seed rolling the pattern's offset and rotation and the
//! wear's and grunge's placing with Valve's random stream, in the order the composite says — into the constants of
//! CS2's own compositor (csgo_customweapon's pixel shader, its SPIR-V as it is: tools/cs2pack/finish_shader.py), which
//! cosmetics/render.rs runs twice: the colour, then the roughness and metalness (F_ROUGHNESS_MODE).
//!
//! Files (tools/cs2pack): the weapon's "<model dir>/finish/<material>.txt", the paint's "paint/<name>/paint.txt",
//! the compositor's "finish/shaders/<combo>.spv" and ".txt".

use super::rifle::UniformRandom;
use super::vfxeval::{self, Context, V4};
use std::collections::HashMap;

/// The compositor's static combos: each S_ combo's feature and its index (CS2's csgo_customweapon ps).
const COMBOS: [(&str, u32); 11] = [
    ("F_PAINT_STYLE", 1),
    ("F_OVERRIDE_NORMAL", 9),
    ("F_USE_ALL_MASKS", 18),
    ("F_ROUGHNESS_TEXTURE", 36),
    ("F_PEARLESCENCE_MASK", 72),
    ("F_METALNESS_TEXTURE", 144),
    ("F_SEPARATE_CHANNEL_INPUTS", 288),
    ("F_OVERLAY_TEXTURE", 576),
    ("F_CASE_HARDENING", 1152),
    ("F_CASE_HARDENING_TRILINEAR", 2304),
    ("F_PATINA_AGE", 4608),
];

#[derive(Clone, Debug, PartialEq)]
pub struct Const {
    pub name: String,
    pub offset: usize,
    /// 1 float, 2..4 its width, 5 an int, 9 a bool.
    pub kind: u32,
    pub default: V4,
    pub expr: Vec<u8>,
}

/// A combo's needs (its .txt).
#[derive(Clone, Debug, Default)]
pub struct Combo {
    pub size: usize,
    pub consts: Vec<Const>,
    pub textures: Vec<(String, u32)>,
    /// (slot, binding)
    pub samplers: Vec<(u32, u32)>,
    /// (slot, 'U' or 'V', its address mode's expression)
    pub address: Vec<(u32, char, Vec<u8>)>,
    pub features: Vec<String>,
    /// The vertex shader's constants, when ours lays them as CS2's (the gloves': "vsize", "vconst" lines).
    pub vsize: usize,
    pub vconsts: Vec<Const>,
}

fn floats(it: &[&str]) -> V4 {
    let mut v = [0.0; 4];
    for (k, x) in it.iter().take(4).enumerate() {
        v[k] = x.parse().unwrap_or(0.0);
    }
    v
}

impl Combo {
    pub fn parse(text: &str) -> Combo {
        let mut c = Combo::default();
        let constant = |name: &str, off: &str, kind: &str, rest: &[&str]| Const {
            name: name.to_string(),
            offset: off.parse().unwrap_or(0),
            kind: kind.parse().unwrap_or(1),
            default: floats(&rest[..rest.len().min(4)]),
            expr: rest.get(4).map(|h| vfxeval::hex(h)).unwrap_or_default(),
        };
        for line in text.lines() {
            let f: Vec<&str> = line.split_whitespace().collect();
            match f.as_slice() {
                ["size", n] => c.size = n.parse().unwrap_or(0),
                ["const", name, off, kind, rest @ ..] => c.consts.push(constant(name, off, kind, rest)),
                ["vsize", n] => c.vsize = n.parse().unwrap_or(0),
                ["vconst", name, off, kind, rest @ ..] => c.vconsts.push(constant(name, off, kind, rest)),
                ["tex", name, b] => c.textures.push((name.to_string(), b.parse().unwrap_or(0))),
                ["sampler", slot, b] => c.samplers.push((slot.parse().unwrap_or(0), b.parse().unwrap_or(0))),
                ["address", slot, axis, e] => {
                    let a = (slot.parse().unwrap_or(0), axis.chars().next().unwrap_or('U'), vfxeval::hex(e));
                    if !c.address.contains(&a) {
                        c.address.push(a);
                    }
                }
                ["feature", name, _] => c.features.push(name.to_string()),
                _ => {}
            }
        }
        c
    }

    /// The vertex shader's constants as a combo of their own (for `constants`).
    pub fn vertex(&self) -> Combo {
        Combo { size: self.vsize, consts: self.vconsts.clone(), ..Combo::default() }
    }
}

/// A weapon's inputs for a material (its finish/<material>.txt): the material it replaces, its parameters,
/// features and textures (keys under the store).
#[derive(Clone, Debug, Default)]
pub struct Inputs {
    pub material: String,
    pub params: Vec<(String, V4)>,
    pub features: Vec<(String, f32)>,
    pub textures: Vec<(String, String)>,
}

impl Inputs {
    pub fn parse(text: &str, dir: &str) -> Inputs {
        let mut i = Inputs::default();
        for line in text.lines() {
            let f: Vec<&str> = line.split_whitespace().collect();
            match f.as_slice() {
                ["material", m] => i.material = m.to_string(),
                ["param", n, rest @ ..] => i.params.push((n.to_string(), floats(rest))),
                ["feature", n, v] => i.features.push((n.to_string(), v.parse().unwrap_or(0.0))),
                ["tex", n, file] => i.textures.push((n.to_string(), format!("{dir}/{file}"))),
                _ => {}
            }
        }
        i
    }
}

/// A paint (paint/<name>/paint.txt).
#[derive(Clone, Debug, Default)]
pub struct Paint {
    pub features: Vec<(String, f32)>,
    pub params: Vec<(String, V4)>,
    pub textures: Vec<(String, String)>,
    /// (container, name, value)
    pub vars: Vec<(String, String, V4)>,
    /// (container.name, component, its bounds — none: not rolled)
    pub rolls: Vec<(String, usize, Option<(f32, f32)>)>,
    pub kit: HashMap<String, String>,
}

impl Paint {
    /// `dir` the paint's folder under the store ("paint/aa_fade"); "shared/…" textures are its parent's.
    pub fn parse(text: &str, dir: &str) -> Paint {
        let parent = dir.rsplit_once('/').map_or("", |(p, _)| p);
        let mut p = Paint::default();
        for line in text.lines() {
            let f: Vec<&str> = line.split_whitespace().collect();
            match f.as_slice() {
                ["feature", n, v] => p.features.push((n.to_string(), v.parse().unwrap_or(0.0))),
                ["param", n, rest @ ..] => p.params.push((n.to_string(), floats(rest))),
                ["tex", n, file] => {
                    let key = if let Some(s) = file.strip_prefix("shared/") { format!("{parent}/shared/{s}") } else { format!("{dir}/{file}") };
                    p.textures.push((n.to_string(), key));
                }
                ["var", cont, n, rest @ ..] => p.vars.push((cont.to_string(), n.to_string(), floats(rest))),
                ["roll", var, comp, "none"] => p.rolls.push((var.to_string(), "xyzw".find(comp).unwrap_or(0), None)),
                ["roll", var, comp, lo, hi] => {
                    p.rolls.push((var.to_string(), "xyzw".find(comp).unwrap_or(0), Some((lo.parse().unwrap_or(0.0), hi.parse().unwrap_or(0.0)))))
                }
                ["kit", k, v] => {
                    p.kit.insert(k.to_string(), v.to_string());
                }
                _ => {}
            }
        }
        p
    }

    /// The wear it may have (its kit's range).
    pub fn wear_range(&self) -> (f32, f32) {
        let g = |k: &str, d: f32| self.kit.get(k).and_then(|v| v.parse().ok()).unwrap_or(d);
        (g("wear_remap_min", 0.0), g("wear_remap_max", 1.0))
    }
}

/// What the item is: its seed (its pattern), its wear (0 new .. 1 battle-scarred).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Instance {
    pub seed: i32,
    pub wear: f32,
}

/// Whether a seed's roll draws a number for a variable the paint does not roll (CS2 rolls the composite's list; one
/// with no bounds keeps its value — and, as CS:GO's did, still takes its draw: the patterns stay where players know them).
const DRAW_UNROLLED: bool = true;

/// The material's parameters once all is put together (by name): the paint's, the weapon's over them, the item's
/// and the seed's rolls over those (as the composite copies its containers: composite_inputs, econ_instance,
/// grunge_wear).
pub fn params(inputs: &Inputs, paint: &Paint, it: Instance) -> HashMap<String, V4> {
    let mut p: HashMap<String, V4> = HashMap::new();
    for (n, v) in &paint.params {
        p.insert(n.clone(), *v);
    }
    for (n, v) in &inputs.params {
        p.insert(n.clone(), *v);
    }
    // the containers' own: the item's (its seed and wear), the wear's and grunge's placing
    let mut vars: HashMap<String, V4> = HashMap::new();
    for (cont, n, v) in &paint.vars {
        vars.insert(format!("{cont}.{n}"), *v);
    }
    vars.insert("econ_instance.g_nRandomSeed".into(), [it.seed as f32; 4]);
    vars.insert("econ_instance.g_flWearAmount".into(), [it.wear; 4]);
    let mut rng = UniformRandom::new(it.seed);
    for (var, comp, bounds) in &paint.rolls {
        match bounds {
            Some((lo, hi)) => {
                let x = rng.float(*lo, *hi);
                vars.entry(var.clone()).or_insert([0.0; 4])[*comp] = x;
            }
            None if DRAW_UNROLLED => {
                rng.float(0.0, 1.0);
            }
            None => {}
        }
    }
    for cont in ["econ_instance", "grunge_wear"] {
        for (k, v) in &vars {
            if let Some(n) = k.strip_prefix(&format!("{cont}.")) {
                p.insert(n.to_string(), *v);
            }
        }
    }
    p
}

/// The features (by the combo's order): the paint's, the weapon's, then `roughness` (F_ROUGHNESS_MODE: the second pass).
pub fn features(combo_features: &[String], inputs: &Inputs, paint: &Paint, roughness: bool) -> Vec<f32> {
    combo_features
        .iter()
        .map(|f| {
            if f == "F_ROUGHNESS_MODE" {
                return roughness as i32 as f32;
            }
            inputs.features.iter().chain(&paint.features).find(|(n, _)| n == f).map_or(0.0, |(_, v)| *v)
        })
        .collect()
}

/// The static combo for the features (the paint's and the weapon's).
pub fn combo_id(inputs: &Inputs, paint: &Paint) -> u32 {
    COMBOS
        .iter()
        .map(|(f, k)| inputs.features.iter().chain(&paint.features).find(|(n, _)| n == f).map_or(0, |(_, v)| *v as u32) * k)
        .sum()
}

/// A shader variable's type code for a 4×4 matrix (VfxVariableType).
const MATRIX: u32 = 19;
const IDENTITY16: vfxeval::M16 = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

/// The constants' bytes for a pass.
pub fn constants(combo: &Combo, params: &HashMap<String, V4>, features: &[f32]) -> Vec<u8> {
    constants_with(combo, params, features, &HashMap::new())
}

/// The constants' bytes for a pass whose expressions read its textures' average colours (by name).
pub fn constants_with(combo: &Combo, params: &HashMap<String, V4>, features: &[f32], averages: &HashMap<String, V4>) -> Vec<u8> {
    let tokens: HashMap<u32, V4> = params.iter().map(|(n, v)| (vfxeval::token(n), *v)).collect();
    let avg: HashMap<u32, V4> = averages.iter().map(|(n, v)| (vfxeval::token(n), *v)).collect();
    let cx = Context { params: &tokens, features, averages: &avg };
    let mut out = vec![0u8; combo.size.max(16)];
    for c in &combo.consts {
        if c.kind == MATRIX {
            // a colour matrix (row_major mat4, a colour times it as a row): Valve's M·v laid as M's transpose
            let m = (!c.expr.is_empty()).then(|| vfxeval::eval_matrix(&c.expr, &cx)).flatten().unwrap_or(IDENTITY16);
            for r in 0..4 {
                for k in 0..4 {
                    let at = c.offset + (r * 4 + k) * 4;
                    if at + 4 <= out.len() {
                        out[at..at + 4].copy_from_slice(&m[k * 4 + r].to_le_bytes());
                    }
                }
            }
            continue;
        }
        let mut v = params.get(&c.name).copied().unwrap_or(c.default);
        if !c.expr.is_empty() {
            if let Some(e) = vfxeval::eval(&c.expr, &cx) {
                v = e;
            }
        }
        let put = |out: &mut Vec<u8>, at: usize, b: [u8; 4]| {
            if at + 4 <= out.len() {
                out[at..at + 4].copy_from_slice(&b);
            }
        };
        match c.kind {
            // an int, int2, int3, int4 (a glove pattern's palette indices); a bool
            5..=8 => {
                for i in 0..(c.kind as usize - 4) {
                    put(&mut out, c.offset + i * 4, (v[i] as i32).to_le_bytes());
                }
            }
            9 => put(&mut out, c.offset, ((v[0] != 0.0) as i32).to_le_bytes()),
            k => {
                for i in 0..(k.clamp(1, 4) as usize) {
                    put(&mut out, c.offset + i * 4, v[i].to_le_bytes());
                }
            }
        }
    }
    out
}

/// CS2's texture transform (its expression for g_v…TexCoordXform0/1, decompiled): rotation, scale (the weapon's size
/// unless ignored), offset — each rounded to a hundredth — about the middle.
pub fn xform(rotation: f32, scale: f32, offset: [f32; 2], ignore_size: bool, uv_scale: f32) -> (V4, V4) {
    let r2 = |x: f32| ((x + 0.005) * 100.0).floor() / 100.0;
    let v0 = r2(rotation);
    let v1 = r2(scale * if ignore_size { 1.0 } else { uv_scale });
    let (ox, oy) = (r2(offset[0]), r2(offset[1]));
    let v3 = v0 * std::f32::consts::PI / 180.0;
    let (v5, v4) = v3.sin_cos();
    let v6 = 0.5 / if v1 != 0.0 { v1 } else { 1.0 };
    let (v8, v7) = (-v3).sin_cos();
    let v9 = v6 * v7 - v6 * v8;
    let v10 = v9 * v8 + v6 * v7;
    (
        [v4 * v1, -v5 * v1, 0.0, v1 * v4 * v9 + v1 * -v5 * v10 + (ox - 0.5)],
        [v5 * v1, v4 * v1, 0.0, v1 * v5 * v9 + v1 * v4 * v10 + (oy - 0.5)],
    )
}

/// The vertex shader's transforms (shaders/cosmetics/finish.vert's push constants): the pattern's, the wear's, the
/// grunge's, the overlay's, two rows each.
pub fn vertex_xforms(p: &HashMap<String, V4>) -> [V4; 8] {
    let g = |n: &str| p.get(n).copied().unwrap_or([0.0; 4]);
    let ignore = g("g_bIgnoreWeaponSizeScale")[0] != 0.0;
    let uv = p.get("g_flUvScale1").map_or(1.0, |v| v[0]);
    let one = |prefix: &str, default_scale: f32| {
        let s = p.get(&format!("g_fl{prefix}TexCoordScale")).map_or(default_scale, |v| v[0]);
        let r = g(&format!("g_fl{prefix}TexCoordRotation"))[0];
        let o = g(&format!("g_v{prefix}TexCoordOffset"));
        xform(r, s, [o[0], o[1]], ignore, uv)
    };
    let (p0, p1) = one("Pattern", 1.0);
    let (w0, w1) = one("Wear", 1.0);
    let (g0, g1) = one("Grunge", 1.0);
    let (o0, o1) = one("Overlay", 1.0);
    [p0, p1, w0, w1, g0, g1, o0, o1]
}

/// The pattern sampler's address modes (U, V): CS2's RsTextureAddressMode (0 wrap, 1 mirror, 2 clamp, 3 border, 4 mirror
/// once), from the material's g_nPattern…Sampling.
pub fn pattern_address(combo: &Combo, params: &HashMap<String, V4>) -> HashMap<u32, (u32, u32)> {
    let tokens: HashMap<u32, V4> = params.iter().map(|(n, v)| (vfxeval::token(n), *v)).collect();
    let cx = Context { params: &tokens, features: &[], averages: &HashMap::new() };
    let mut out: HashMap<u32, (u32, u32)> = HashMap::new();
    for (slot, axis, e) in &combo.address {
        let m = vfxeval::eval(e, &cx).map_or(0, |v| v[0] as u32);
        let a = out.entry(*slot).or_insert((0, 0));
        if *axis == 'U' {
            a.0 = m;
        } else {
            a.1 = m;
        }
    }
    out
}

/// The textures to bind for a combo (binding, key): the weapon's inputs and the paint's by name (the paint's masks
/// when it overrides them); none for what neither has (the default then).
pub fn textures(combo: &Combo, inputs: &Inputs, paint: &Paint, p: &HashMap<String, V4>) -> Vec<(u32, Option<String>)> {
    let override_masks = p.get("g_bOverrideMasks").is_some_and(|v| v[0] != 0.0);
    combo
        .textures
        .iter()
        .map(|(name, b)| {
            let from = |l: &Vec<(String, String)>| l.iter().find(|(n, _)| n == name).map(|(_, k)| k.clone());
            let key = if name == "g_tMasks" && override_masks { from(&paint.textures).or_else(|| from(&inputs.textures)) } else { from(&inputs.textures).or_else(|| from(&paint.textures)) };
            (*b, key)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_paint_and_its_weapon_are_read() {
        let inputs = Inputs::parse("material weapon_knife_karambit\nparam g_flUvScale1 0.438\ntex g_tMasks knife_karam_masks.rtx\n", "knife/karambit/finish");
        assert_eq!(inputs.material, "weapon_knife_karambit");
        assert_eq!(inputs.textures[0].1, "knife/karambit/finish/knife_karam_masks.rtx");
        let paint = Paint::parse(
            "feature F_PAINT_STYLE 5\nparam g_vColor0 0.86 0.82 0.80 1\ntex g_tPattern fade.rtx\ntex g_tWear shared/paint_wear.rtx\nvar econ_instance g_flPatternTexCoordRotation -55.0\nroll econ_instance.g_vPatternTexCoordOffset x none\nroll econ_instance.g_flPatternTexCoordRotation x -55.0 -65.0\nkit wear_remap_max 0.08\n",
            "paint/aa_fade",
        );
        assert_eq!(paint.textures[1].1, "paint/shared/paint_wear.rtx");
        assert_eq!(paint.rolls.len(), 2);
        assert_eq!(paint.wear_range(), (0.0, 0.08));
        assert_eq!(combo_id(&inputs, &paint), 5);
        // the seed rolls the rotation between -55 and -65; the wear is the item's
        let p = params(&inputs, &paint, Instance { seed: 661, wear: 0.03 });
        let r = p["g_flPatternTexCoordRotation"][0];
        assert!((-65.0..=-55.0).contains(&r), "{r}");
        assert_eq!(p["g_flWearAmount"][0], 0.03);
        assert_eq!(p["g_flUvScale1"][0], 0.438);
        // another seed, another rotation; the same seed, the same
        let q = params(&inputs, &paint, Instance { seed: 662, wear: 0.03 });
        assert_ne!(q["g_flPatternTexCoordRotation"][0], r);
        assert_eq!(params(&inputs, &paint, Instance { seed: 661, wear: 0.0 })["g_flPatternTexCoordRotation"][0], r);
    }

    #[test]
    fn the_transform_is_cs2s() {
        // no turn, scale 1, no offset: the coordinates as they are
        let (x0, x1) = xform(0.0, 1.0, [0.0, 0.0], true, 1.0);
        let at = |x: V4, u: f32, v: f32| u * x[0] + v * x[1] + x[3];
        assert!((at(x0, 0.3, 0.7) - 0.3).abs() < 1e-5 && (at(x1, 0.3, 0.7) - 0.7).abs() < 1e-5, "{x0:?} {x1:?}");
        // scale 2 (rounded to hundredths as CS2 does): twice as fast; the weapon's size counts unless ignored
        let (x0, _) = xform(0.0, 2.0, [0.0, 0.0], false, 0.5);
        assert!((at(x0, 0.3, 0.0) - 0.3).abs() < 1e-5);
    }

    #[test]
    fn constants_go_where_the_combo_says() {
        let combo = Combo::parse("size 32\nconst g_flWearAmount 4 1 0.0 0.0 0.0 0.0\nconst g_vColor0 8 3 0.5 0.5 0.5 0.0 1d69c3c315061f0000\nconst bRoughnessMode 20 9 0 0 0 0\nfeature F_ROUGHNESS_MODE 0\n");
        let mut p = HashMap::new();
        p.insert("g_flWearAmount".to_string(), [0.25; 4]);
        p.insert("g_vColor0".to_string(), [1.0, 0.5, 0.0, 1.0]);
        let b = constants(&combo, &p, &[1.0]);
        let f = |o: usize| f32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        assert_eq!(f(4), 0.25);
        // the colour to linear light (its expression)
        assert!((f(8) - 1.0).abs() < 1e-6 && (f(12) - 0.21404).abs() < 1e-4 && f(16) == 0.0);
        assert_eq!(&b[20..24], &[0, 0, 0, 0]);
    }
}
