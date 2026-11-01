//! A glove paint worked onto a pair of gloves as CS2 works it (gloves/paints/…vcompmat: CS2's glove compositor,
//! csgo_customglove): tools/cs2pack/glove_paint.py plays the composite's assembly for each hand into paint/<name>/
//! glove.txt — the material the generation starts from and the paint's, the textures, the instance's variables, and
//! for each hand the seed's rolls (the left hand's seed; the right's CS2's g_nRandomSeedAlt, the seed + 1: checked
//! against pattern.wiki's right gloves) and each texture it makes, step by step. Here those steps are run for the
//! item's seed and wear into the constants of CS2's own compositor (its pixel shader's, and its vertex shader's at
//! their byte offsets: shaders/cosmetics/glove_finish.vert), for cosmetics/compose.rs: the colour, the metalness and
//! cloth, the normal and roughness, the occlusion.

use super::compose::{Job, Out, Pass, Vertex};
use super::finish::{self, Combo};
use super::model::Model;
use super::rifle::UniformRandom;
use super::store;
use super::vfxeval::V4;
use std::collections::HashMap;
use std::sync::Arc;

/// A step of a texture's generation.
#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    Set(String, V4),
    /// The paint's parameters (and textures), or the instance's variables, over the material's.
    Copy(String),
    /// The item's wear into this parameter.
    Wear(String),
}

#[derive(Clone, Debug, Default)]
pub struct Roll {
    /// econ_instance's seed it is seeded by (g_nRandomSeed, g_nRandomSeedAlt).
    pub seed: String,
    /// (variable, component, its bounds — none: drawn and let go)
    pub draws: Vec<(String, usize, Option<(f32, f32)>)>,
}

#[derive(Clone, Debug)]
pub struct Generate {
    pub target: String,
    pub combo: u32,
    pub ops: Vec<Op>,
}

#[derive(Clone, Debug, Default)]
pub struct Hand {
    pub rolls: Vec<Roll>,
    /// (from, to) among the instance's variables, after the rolls
    pub copies: Vec<(String, String)>,
    pub outs: Vec<Generate>,
}

/// A glove paint (paint/<name>/glove.txt).
#[derive(Clone, Debug, Default)]
pub struct GlovePaint {
    pub init: Vec<(String, V4)>,
    pub paint: Vec<(String, V4)>,
    pub tex_init: Vec<(String, String)>,
    pub tex_paint: Vec<(String, String)>,
    /// The textures' average colours (linear: TextureAverageColor in the layers' colour matrices).
    pub avg_init: Vec<(String, V4)>,
    pub avg_paint: Vec<(String, V4)>,
    pub vars: Vec<(String, V4)>,
    /// left, right
    pub hands: [Hand; 2],
    pub kit: HashMap<String, String>,
}

fn floats(it: &[&str]) -> V4 {
    let mut v = [0.0; 4];
    for (k, x) in it.iter().take(4).enumerate() {
        v[k] = x.parse().unwrap_or(0.0);
    }
    v
}

impl GlovePaint {
    /// `dir` the folder its textures are under ("paint/gloves").
    pub fn parse(text: &str, dir: &str) -> GlovePaint {
        let mut g = GlovePaint::default();
        let mut hand: Option<usize> = None;
        for line in text.lines() {
            let f: Vec<&str> = line.split_whitespace().collect();
            match f.as_slice() {
                ["mat", which, n, rest @ ..] => {
                    let v = (n.to_string(), floats(rest));
                    if *which == "paint" { g.paint.push(v) } else { g.init.push(v) }
                }
                ["tex", which, n, file] => {
                    let v = (n.to_string(), format!("{dir}/{file}"));
                    if *which == "paint" { g.tex_paint.push(v) } else { g.tex_init.push(v) }
                }
                ["avg", which, n, rest @ ..] => {
                    let v = (n.to_string(), floats(rest));
                    if *which == "paint" { g.avg_paint.push(v) } else { g.avg_init.push(v) }
                }
                ["var", n, rest @ ..] => g.vars.push((n.to_string(), floats(rest))),
                ["hand", h] => hand = Some(if *h == "right" { 1 } else { 0 }),
                ["kit", k, v] => {
                    g.kit.insert(k.to_string(), v.to_string());
                }
                _ => {
                    let Some(h) = hand.map(|h| &mut g.hands[h]) else { continue };
                    match f.as_slice() {
                        ["roll", seed] => h.rolls.push(Roll { seed: seed.to_string(), draws: Vec::new() }),
                        ["draw", var, comp, "none"] => {
                            if let Some(r) = h.rolls.last_mut() {
                                r.draws.push((var.to_string(), "xyzw".find(comp).unwrap_or(0), None));
                            }
                        }
                        ["draw", var, comp, lo, hi] => {
                            if let Some(r) = h.rolls.last_mut() {
                                r.draws.push((var.to_string(), "xyzw".find(comp).unwrap_or(0), Some((lo.parse().unwrap_or(0.0), hi.parse().unwrap_or(0.0)))));
                            }
                        }
                        ["copyvar", from, to] => h.copies.push((from.to_string(), to.to_string())),
                        ["out", target, _init, combo] => h.outs.push(Generate { target: target.to_string(), combo: combo.parse().unwrap_or(0), ops: Vec::new() }),
                        ["op", "set", n, rest @ ..] => {
                            if let Some(o) = h.outs.last_mut() {
                                o.ops.push(Op::Set(n.to_string(), floats(rest)));
                            }
                        }
                        ["op", "copy", src] => {
                            if let Some(o) = h.outs.last_mut() {
                                o.ops.push(Op::Copy(src.to_string()));
                            }
                        }
                        ["op", "wear", n] => {
                            if let Some(o) = h.outs.last_mut() {
                                o.ops.push(Op::Wear(n.to_string()));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        g
    }

    /// The wear it may have (its kit's range).
    pub fn wear_range(&self) -> (f32, f32) {
        let g = |k: &str, d: f32| self.kit.get(k).and_then(|v| v.parse().ok()).unwrap_or(d);
        (g("wear_remap_min", 0.0), g("wear_remap_max", 1.0))
    }
}

/// The instance's variables for a hand: its own, the seed's rolls over them (each roll a stream of its own; one with
/// no bounds still takes its draw — CS2's "consumable" numbers keep the hands' streams in step), then its copies.
pub fn vars(gp: &GlovePaint, hand: usize, seed: i32) -> HashMap<String, V4> {
    let mut v: HashMap<String, V4> = gp.vars.iter().cloned().collect();
    let h = &gp.hands[hand.min(1)];
    for r in &h.rolls {
        // g_nRandomSeedAlt: the seed + 1
        let s = if r.seed.ends_with("Alt") { seed + 1 } else { seed };
        let mut rng = UniformRandom::new(s);
        for (var, comp, bounds) in &r.draws {
            match bounds {
                Some((lo, hi)) => {
                    let x = rng.float(*lo, *hi);
                    v.entry(var.clone()).or_insert([0.0; 4])[*comp] = x;
                }
                None => {
                    rng.float(0.0, 1.0);
                }
            }
        }
    }
    for (from, to) in &h.copies {
        if let Some(x) = v.get(from).copied() {
            v.insert(to.clone(), x);
        }
    }
    v
}

/// The compositor's material for one texture of a hand: the starting material's, then its steps.
pub fn params(gp: &GlovePaint, out: &Generate, vars: &HashMap<String, V4>, wear: f32) -> HashMap<String, V4> {
    let mut p: HashMap<String, V4> = gp.init.iter().cloned().collect();
    for op in &out.ops {
        match op {
            Op::Set(n, v) => {
                p.insert(n.clone(), *v);
            }
            Op::Copy(src) if src == "paint" => p.extend(gp.paint.iter().cloned()),
            Op::Copy(_) => p.extend(vars.iter().map(|(k, v)| (k.clone(), *v))),
            Op::Wear(n) => {
                p.insert(n.clone(), [wear; 4]);
            }
        }
    }
    p
}

/// The textures it binds (the paint's over the starting material's when the paint is copied in).
fn textures(combo: &Combo, gp: &GlovePaint, out: &Generate) -> Vec<(u32, Option<String>)> {
    let paint = out.ops.iter().any(|o| *o == Op::Copy("paint".into()));
    combo
        .textures
        .iter()
        .map(|(name, b)| {
            let find = |l: &Vec<(String, String)>| l.iter().find(|(n, _)| n == name).map(|(_, k)| k.clone());
            let key = if paint { find(&gp.tex_paint).or_else(|| find(&gp.tex_init)) } else { find(&gp.tex_init) };
            (*b, key)
        })
        .collect()
}

/// The textures' average colours for a texture's generation (the paint's over the starting material's when the paint
/// is copied in, as its textures are).
fn averages(gp: &GlovePaint, out: &Generate) -> HashMap<String, V4> {
    let mut a: HashMap<String, V4> = gp.avg_init.iter().cloned().collect();
    if out.ops.iter().any(|o| *o == Op::Copy("paint".into())) {
        a.extend(gp.avg_paint.iter().cloned());
    }
    a
}

/// What each target is to us.
fn out_of(target: &str) -> Option<Out> {
    match target {
        "g_tColor" => Some(Out::Color),
        "g_tMetalness" => Some(Out::Metal),
        "g_tNormal" => Some(Out::Normal),
        "g_tAmbientOcclusion" => Some(Out::Ao),
        _ => None,
    }
}

/// A glove paint's jobs for a model of gloves: one for each of its gloves' materials (the left's and the right's, by
/// their names), keyed by the model's folder, the material, the paint, the seed and the wear.
pub fn jobs(model_key: &str, model: &Model, paint_name: &str, seed: i32, wear: f32) -> Vec<Arc<Job>> {
    let text = |key: &str| store::read(key).map(|b| String::from_utf8_lossy(&b).into_owned());
    let Some(gp) = text(&format!("paint/{paint_name}/glove.txt")).map(|t| GlovePaint::parse(&t, "paint/gloves")) else { return Vec::new() };
    let (lo, hi) = gp.wear_range();
    let wear = wear.clamp(lo, hi);
    let dir = model_key.rsplit_once('/').map_or("", |(d, _)| d).to_string();
    let mut out = Vec::new();
    for m in &model.materials {
        let hand = if m.name.contains("_left") {
            0
        } else if m.name.contains("_right") {
            1
        } else {
            continue;
        };
        let v = vars(&gp, hand, seed);
        let mut passes = Vec::new();
        for g in &gp.hands[hand].outs {
            let Some(o) = out_of(&g.target) else { continue };
            let Some(info) = text(&format!("finish/glove_shaders/{}.txt", g.combo)).map(|t| Combo::parse(&t)) else { continue };
            let Some(spv) = store::read(&format!("finish/glove_shaders/{}.spv", g.combo)) else { continue };
            let spirv: Vec<u32> = spv.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
            let p = params(&gp, g, &v, wear);
            let feats: Vec<f32> = info.features.iter().map(|f| p.get(f).map_or(0.0, |x| x[0])).collect();
            let consts = finish::constants_with(&info, &p, &feats, &averages(&gp, g));
            let vs = finish::constants(&info.vertex(), &p, &feats);
            let tex = textures(&info, &gp, g);
            for k in tex.iter().filter_map(|(_, k)| k.as_ref()) {
                store::want(k);
            }
            passes.push(Pass {
                out: o,
                combo: g.combo,
                spirv: Arc::new(spirv),
                address: finish::pattern_address(&info, &p),
                info: Arc::new(info),
                consts,
                textures: tex,
                vertex: Vertex::Glove(vs),
            });
        }
        if passes.iter().any(|p| p.out == Out::Color) {
            out.push(Arc::new(Job {
                key: format!("glove/{dir}/{}/{paint_name}/{seed}/{wear}", m.name),
                model: model_key.to_string(),
                material: m.name.clone(),
                passes,
                size: 2048,
            }));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_glove_paint_reads_and_rolls_each_hand_as_cs2_does() {
        let text = "mat init F_PATTERN 1\nmat init g_bFlipFixup 0\ntex init g_tPattern fade_psd_srgb.rtx\n\
                    var g_fPatternTexCoordOffset 0.0 0.0\nvar g_fPatternTexCoordRotation 0.0\n\
                    hand left\nroll g_nRandomSeed\ndraw g_fPatternTexCoordOffset x 0.0 1.0\ndraw g_fPatternTexCoordOffset y 0.0 1.0\n\
                    draw g_fPatternTexCoordRotation x 45.0 135.0\nout g_tColor init 0\nop set F_OUTPUT_MODE 0.0\nop wear g_fWearProgress\n\
                    op copy instance_params\nop set g_bFlipFixup 1.0\n\
                    hand right\nroll g_nRandomSeedAlt\ndraw g_fPatternTexCoordOffset x 0.0 1.0\nout g_tColor init 0\nop copy instance_params\n\
                    kit wear_remap_min 0.06\nkit wear_remap_max 0.8\n";
        let g = GlovePaint::parse(text, "paint/gloves");
        assert_eq!(g.tex_init[0].1, "paint/gloves/fade_psd_srgb.rtx");
        assert_eq!(g.wear_range(), (0.06, 0.8));
        // pattern.wiki's Specialist Gloves | Fade, seed 2 (the left glove): offset 0.342 0.402, rotation 72.705
        let v = vars(&g, 0, 2);
        let o = v["g_fPatternTexCoordOffset"];
        let r = v["g_fPatternTexCoordRotation"][0];
        assert!((o[0] - 0.342).abs() < 5e-4 && (o[1] - 0.402).abs() < 5e-4 && (r - 72.705).abs() < 5e-3, "{o:?} {r}");
        // the right hand: the seed + 1's stream
        let right = vars(&g, 1, 2)["g_fPatternTexCoordOffset"][0];
        assert_eq!(right, UniformRandom::new(3).float(0.0, 1.0));
        let p = params(&g, &g.hands[0].outs[0], &v, 0.3);
        assert_eq!(p["g_fWearProgress"][0], 0.3);
        assert_eq!(p["g_bFlipFixup"][0], 1.0);
        assert_eq!(p["F_OUTPUT_MODE"][0], 0.0);
        assert_eq!(p["g_fPatternTexCoordRotation"][0], r);
    }
}
