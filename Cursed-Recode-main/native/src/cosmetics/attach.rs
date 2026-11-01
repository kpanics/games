//! Where CS2's models hold their extras (tools/cs2pack/attachments.py: items/<item>/attachments.txt): a weapon's
//! StatTrak module, its name tag, its charm, a model's inventory camera — each on a bone, an offset (inches) and a
//! turn in that bone's own frame, as Source has them. VRF's glTF keeps the bones' own frames (only scaled to metres:
//! the turn from Source's axes is baked into the roots), so a bone's place carries an attachment as it is; a model
//! held there (made by VRF in glTF's axes) is turned back to Source's first.

use super::model::{mul, Model, Trs, M4};
use std::collections::HashMap;

/// Source's inches in metres (VRF's SourceToGltfScale).
const INCH: f32 = 0.0254;
/// VRF's SourceToGltfRotation (x, y, z, w): the turn from Source's axes to glTF's.
const TO_GLTF: [f32; 4] = [-0.5, -0.5, -0.5, 0.5];

#[derive(Clone, Debug, PartialEq)]
pub struct Attachment {
    pub bone: String,
    pub offset: [f32; 3],
    pub rotation: [f32; 4],
}

/// A model's attachments by name.
pub fn parse(text: &str) -> HashMap<String, Attachment> {
    let mut out = HashMap::new();
    for line in text.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if let ["att", name, bone, rest @ ..] = f.as_slice() {
            let v: Vec<f32> = rest.iter().filter_map(|x| x.parse().ok()).collect();
            if v.len() == 7 {
                out.insert(name.to_string(), Attachment { bone: bone.to_string(), offset: [v[0], v[1], v[2]], rotation: [v[3], v[4], v[5], v[6]] });
            }
        }
    }
    out
}

/// The attachment's place in the model for its joints' places (`world`, as Model::world_from gives them).
pub fn place(a: &Attachment, model: &Model, world: &[M4]) -> Option<M4> {
    let bone = world.get(model.joint(&a.bone)?)?;
    let local = Trs { t: a.offset.map(|x| x * INCH), r: a.rotation, s: [1.0; 3] }.matrix();
    Some(mul(bone, &local))
}

/// A model made by VRF (in glTF's axes) held at an attachment's place: turned back to Source's axes first.
pub fn holding(at: &M4) -> M4 {
    let [x, y, z, w] = TO_GLTF;
    let back = Trs { t: [0.0; 3], r: [-x, -y, -z, w], s: [1.0; 3] }.matrix();
    mul(at, &back)
}

/// A camera turned as Source's angles turn one (`pitch` down, `yaw` about up, `roll` about ahead: degrees, as
/// AngleVectors), in a model's own space (glTF's axes, as VRF makes it): its columns ahead, left and up, as an
/// attachment's place has them.
pub fn source_view(pitch: f32, yaw: f32, roll: f32) -> M4 {
    let (sp, cp) = pitch.to_radians().sin_cos();
    let (sy, cy) = yaw.to_radians().sin_cos();
    let (sr, cr) = roll.to_radians().sin_cos();
    let to = Trs { t: [0.0; 3], r: TO_GLTF, s: [1.0; 3] }.matrix();
    let gltf = |v: [f32; 3]| -> [f32; 3] { std::array::from_fn(|k| to[k] * v[0] + to[4 + k] * v[1] + to[8 + k] * v[2]) };
    let ahead = [cp * cy, cp * sy, -sp];
    let left = [sr * sp * cy - cr * sy, sr * sp * sy + cr * cy, sr * cp];
    let up = [cr * sp * cy + sr * sy, cr * sp * sy - sr * cy, cr * cp];
    let (a, l, u) = (gltf(ahead), gltf(left), gltf(up));
    [a[0], a[1], a[2], 0.0, l[0], l[1], l[2], 0.0, u[0], u[1], u[2], 0.0, 0.0, 0.0, 0.0, 1.0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_attachment_reads_and_a_model_turns_back() {
        let a = parse("att stattrak weapon_offset -4.3 0.8 -1.2 0.0 0.0 0.0 1.0\natt broken x 1 2\n");
        assert_eq!(a.len(), 1);
        assert_eq!(a["stattrak"].bone, "weapon_offset");
        // VRF's turn and its inverse make nothing
        let to = Trs { t: [0.0; 3], r: TO_GLTF, s: [1.0; 3] }.matrix();
        let both = holding(&to);
        let id = super::super::model::IDENTITY;
        assert!(both.iter().zip(id.iter()).all(|(p, q)| (p - q).abs() < 1e-6), "{both:?}");
    }

    /// Source's up is glTF's; a camera level and unturned looks along Source's ahead (glTF's z), its axes square.
    #[test]
    fn a_source_camera_in_gltf_axes() {
        let c = source_view(0.0, 0.0, 0.0);
        let near = |a: &[f32], b: [f32; 3]| a.iter().zip(b).all(|(p, q)| (p - q).abs() < 1e-5);
        assert!(near(&c[8..11], [0.0, 1.0, 0.0]), "{c:?}");
        assert!(near(&c[0..3], [0.0, 0.0, 1.0]), "{c:?}");
        let g = source_view(-20.0, 140.0, -35.0);
        let dot = |i: usize, j: usize| (0..3).map(|k| g[i * 4 + k] * g[j * 4 + k]).sum::<f32>();
        for (i, j) in [(0, 1), (0, 2), (1, 2)] {
            assert!(dot(i, j).abs() < 1e-5);
        }
        for i in 0..3 {
            assert!((dot(i, i) - 1.0).abs() < 1e-5);
        }
    }
}
