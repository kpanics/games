//! Valve's compiled material expressions run (a shader variable's m_pCompiledExpression: how a material's numbers
//! become the shader's — a paint's colours to linear light, its pattern's transform from its scale, rotation and
//! offset), as VRF's VfxEval (MIT) reads them: a stack machine of four-wide values — literals, the material's
//! parameters (by the MurmurHash2 of their lowercased names), the shader's features, local variables, operators,
//! functions, branches — and of colour matrices (a layer's tint, contrast, saturation, brightness: VRF's
//! VfxEvalFunctions, checked against its own tests).

use std::collections::HashMap;

pub type V4 = [f32; 4];

/// Valve's MurmurHash2 of a name, lowercased (seed 0x31415926): how an expression names a material's parameter.
pub fn token(name: &str) -> u32 {
    let data = name.to_ascii_lowercase().into_bytes();
    let m: u32 = 0x5BD1_E995;
    let mut h: u32 = 0x3141_5926 ^ data.len() as u32;
    let mut chunks = data.chunks_exact(4);
    for c in &mut chunks {
        let mut k = u32::from_le_bytes([c[0], c[1], c[2], c[3]]);
        k = k.wrapping_mul(m);
        k ^= k >> 24;
        k = k.wrapping_mul(m);
        h = h.wrapping_mul(m) ^ k;
    }
    let rest = chunks.remainder();
    if rest.len() == 3 {
        h ^= (rest[2] as u32) << 16;
    }
    if rest.len() >= 2 {
        h ^= (rest[1] as u32) << 8;
    }
    if !rest.is_empty() {
        h ^= rest[0] as u32;
        h = h.wrapping_mul(m);
    }
    h ^= h >> 13;
    h = h.wrapping_mul(m);
    h ^= h >> 15;
    h
}

/// What an expression may read: the material's parameters (by token), the shader's features (by index), its
/// textures' average colours (by token: TextureAverageColor, a texture's "reflectivity" in linear light).
pub struct Context<'a> {
    pub params: &'a HashMap<u32, V4>,
    pub features: &'a [f32],
    pub averages: &'a HashMap<u32, V4>,
}

/// A 4×4 matrix, row-major; a colour goes through it as M·(r, g, b, 1).
pub type M16 = [f32; 16];

/// A value on the stack: four wide, or a matrix.
#[derive(Clone, Copy, Debug)]
enum Val {
    V(V4),
    M(M16),
}

impl Val {
    fn v(self) -> V4 {
        match self {
            Val::V(v) => v,
            Val::M(m) => [m[0], m[1], m[2], m[3]],
        }
    }
}

const IDENTITY: M16 = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

/// System.Numerics' matrices (row vectors: v·M; translation in the last row), as VRF builds its colour matrices.
mod numerics {
    use super::M16;

    pub fn mul(a: &M16, b: &M16) -> M16 {
        std::array::from_fn(|i| {
            let (r, c) = (i / 4, i % 4);
            (0..4).map(|k| a[r * 4 + k] * b[k * 4 + c]).sum()
        })
    }

    pub fn transpose(a: &M16) -> M16 {
        std::array::from_fn(|i| a[(i % 4) * 4 + i / 4])
    }

    pub fn scale(s: [f32; 3]) -> M16 {
        [s[0], 0.0, 0.0, 0.0, 0.0, s[1], 0.0, 0.0, 0.0, 0.0, s[2], 0.0, 0.0, 0.0, 0.0, 1.0]
    }

    pub fn translation(t: [f32; 3]) -> M16 {
        [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, t[0], t[1], t[2], 1.0]
    }

    /// Matrix4x4.CreateFromAxisAngle.
    pub fn axis_angle(a: [f32; 3], angle: f32) -> M16 {
        let (x, y, z) = (a[0], a[1], a[2]);
        let (sa, ca) = angle.sin_cos();
        let (xx, yy, zz, xy, xz, yz) = (x * x, y * y, z * z, x * y, x * z, y * z);
        [
            xx + ca * (1.0 - xx),
            xy - ca * xy + sa * z,
            xz - ca * xz - sa * y,
            0.0,
            xy - ca * xy - sa * z,
            yy + ca * (1.0 - yy),
            yz - ca * yz + sa * x,
            0.0,
            xz - ca * xz + sa * y,
            yz - ca * yz - sa * x,
            zz + ca * (1.0 - zz),
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
        ]
    }

    /// Vector3.Transform: v·M.
    pub fn transform(v: [f32; 3], m: &M16) -> [f32; 3] {
        std::array::from_fn(|c| v[0] * m[c] + v[1] * m[4 + c] + v[2] * m[8 + c] + m[12 + c])
    }

    fn norm(v: [f32; 3]) -> [f32; 3] {
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        v.map(|x| x / l)
    }

    /// Rec. 709's luminance, as a unit axis.
    pub fn luminance() -> [f32; 3] {
        norm([0.2126, 0.7152, 0.0722])
    }

    /// The turn taking the luminance axis onto z.
    pub fn luminance_rotation() -> M16 {
        let l = luminance();
        let cross = [l[1], -l[0], 0.0];
        let len = (cross[0] * cross[0] + cross[1] * cross[1]).sqrt();
        let angle = len.atan2(l[2]);
        axis_angle(norm(cross), angle)
    }
}

/// VRF's MatrixColorCorrect2: contrast about `offset`, brightness, saturation about the luminance axis (`csb`: x
/// contrast, y saturation, z brightness).
pub fn color_correct2(csb: [f32; 3], offset: [f32; 3]) -> M16 {
    use numerics::*;
    let l = luminance();
    let rot = luminance_rotation();
    let mut r = mul(&mul(&translation(offset.map(|x| -x)), &scale([csb[0]; 3])), &translation(offset));
    r = mul(&r, &scale([csb[2]; 3]));
    r = mul(&r, &scale(l));
    r = mul(&r, &rot);
    r = mul(&r, &scale([csb[1], csb[1], 1.0]));
    r = mul(&r, &transpose(&rot));
    r = mul(&r, &scale(l.map(|x| 1.0 / x)));
    transpose(&r)
}

/// VRF's MatrixColorTint2: a luminance-keeping tint toward `rgb`'s hue and saturation.
pub fn color_tint2(rgb: [f32; 3], strength: f32) -> M16 {
    use numerics::*;
    let max = rgb[0].max(rgb[1]).max(rgb[2]);
    let min = rgb[0].min(rgb[1]).min(rgb[2]);
    let saturation = if max == 0.0 { 0.0 } else { (max - min) / max };
    let l = luminance();
    let rot = luminance_rotation();
    let lum_scale = scale(l);
    let gray = transform(rgb, &mul(&lum_scale, &rot));
    let desat = 1.0 - saturation;
    let sat = 1.0 - saturation * saturation * strength;
    let mut r = mul(&lum_scale, &rot);
    r = mul(&r, &translation([-gray[0], -gray[1], 0.0]));
    r = mul(&r, &scale([desat, desat, sat]));
    r = mul(&r, &translation([gray[0], gray[1], (1.0 - sat) * gray[2]]));
    r = mul(&r, &transpose(&rot));
    r = mul(&r, &scale(l.map(|x| 1.0 / x)));
    transpose(&r)
}

/// Two colour matrices one after the other (b's first: M = a·b).
fn mat_mul(a: &M16, b: &M16) -> M16 {
    numerics::mul(a, b)
}

fn all(x: f32) -> V4 {
    [x; 4]
}

fn map(a: V4, f: impl Fn(f32) -> f32) -> V4 {
    a.map(f)
}

fn zip(a: V4, b: V4, f: impl Fn(f32, f32) -> f32) -> V4 {
    std::array::from_fn(|k| f(a[k], b[k]))
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// How many arguments each function takes (VfxEval's FUNCTION_REF).
const ARGS: [usize; 58] = [
    1, 1, 1, 1, 1, 1, 1, 3, 3, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 3, 4, 3, 2, 0, 2, 2, 1, 1, 2, 1, 1, 1, 1, 2, 1, 1, 1, 0, 1, 1, 1, 2, 2, 1, 2, 1, 1, 1, 1, 1, 2,
    3, 5, 5,
];

fn call(id: u8, args: &[Val]) -> Val {
    let m = |k: usize| match args[k] {
        Val::M(m) => m,
        Val::V(_) => IDENTITY,
    };
    let three = |v: V4| [v[0], v[1], v[2]];
    match id {
        // TextureAverageColor: its argument already the texture's average (the parameter read gives it)
        0x28 => return args[0],
        0x29 => return Val::M(IDENTITY),
        0x2A => return Val::M(numerics::transpose(&numerics::scale(three(args[0].v())))),
        0x2B => return Val::M(numerics::transpose(&numerics::translation(three(args[0].v())))),
        0x2E => return Val::M(mat_mul(&m(0), &m(1))),
        0x30 => return Val::M(color_correct2(three(args[0].v()), three(args[1].v()))),
        0x36 => return Val::M(color_tint2(three(args[0].v()), args[1].v()[0])),
        0x37 => {
            let rgb = three(args[0].v());
            return Val::M(match args[2].v()[0] as i32 {
                1 => color_tint2(rgb, args[1].v()[0]),
                2 => numerics::scale(rgb.map(|c| c * 2.0)),
                _ => numerics::scale(rgb),
            });
        }
        _ => {}
    }
    let a: Vec<V4> = args.iter().map(|v| v.v()).collect();
    let x = |k: usize| a[k];
    Val::V(match id {
        0x00 => map(x(0), f32::sin),
        0x01 => map(x(0), f32::cos),
        0x02 => map(x(0), f32::tan),
        0x03 => map(x(0), |v| v - v.floor()),
        0x04 => map(x(0), f32::floor),
        0x05 => map(x(0), f32::ceil),
        0x06 => map(x(0), |v| v.clamp(0.0, 1.0)),
        0x07 => std::array::from_fn(|k| x(0)[k].max(x(1)[k]).min(x(2)[k])),
        0x08 => std::array::from_fn(|k| x(0)[k] + (x(1)[k] - x(0)[k]) * x(2)[k]),
        0x09 => all((0..4).map(|k| x(0)[k] * x(1)[k]).sum()),
        0x0A => all((0..3).map(|k| x(0)[k] * x(1)[k]).sum()),
        0x0B => all((0..2).map(|k| x(0)[k] * x(1)[k]).sum()),
        0x0C => map(x(0), f32::ln),
        0x0D => map(x(0), f32::log2),
        0x0E => map(x(0), f32::log10),
        0x0F => map(x(0), f32::exp),
        0x10 => map(x(0), f32::exp2),
        0x11 => map(x(0), f32::sqrt),
        0x12 => map(x(0), |v| 1.0 / v.sqrt()),
        0x13 => map(x(0), |v| if v > 0.0 { 1.0 } else if v < 0.0 { -1.0 } else { 0.0 }),
        0x14 => map(x(0), f32::abs),
        0x15 => zip(x(0), x(1), f32::powf),
        0x16 => zip(x(0), x(1), |e, v| if v >= e { 1.0 } else { 0.0 }),
        0x17 => std::array::from_fn(|k| {
            let t = ((x(2)[k] - x(0)[k]) / (x(1)[k] - x(0)[k])).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        }),
        0x18 => [x(0)[0], x(1)[0], x(2)[0], x(3)[0]],
        0x19 => [x(0)[0], x(1)[0], x(2)[0], 0.0],
        0x1A => [x(0)[0], x(1)[0], 0.0, 0.0],
        0x1C => zip(x(0), x(1), f32::min),
        0x1D => zip(x(0), x(1), f32::max),
        0x1E => map(x(0), linear_to_srgb),
        0x1F => map(x(0), srgb_to_linear),
        0x21 | 0x32 => {
            let l = (x(0)[0] * x(0)[0] + x(0)[1] * x(0)[1] + x(0)[2] * x(0)[2]).sqrt();
            if l > 0.0 {
                map(x(0), |v| v / l)
            } else {
                x(0)
            }
        }
        0x22 => all((x(0)[0] * x(0)[0] + x(0)[1] * x(0)[1] + x(0)[2] * x(0)[2]).sqrt()),
        0x23 => map(x(0), |v| v * v),
        0x34 => map(x(0), f32::to_radians),
        0x35 => map(x(0), f32::to_degrees),
        0x38 | 0x39 => std::array::from_fn(|k| {
            let (v, a0, a1, b0, b1) = (x(0)[k], x(1)[k], x(2)[k], x(3)[k], x(4)[k]);
            let mut t = if a1 != a0 { (v - a0) / (a1 - a0) } else { 0.0 };
            if id == 0x39 {
                t = t.clamp(0.0, 1.0);
            }
            b0 + (b1 - b0) * t
        }),
        _ => all(0.0),
    })
}

/// An expression's value (none: it could not be read).
pub fn eval(blob: &[u8], cx: &Context) -> Option<V4> {
    run(blob, cx).map(Val::v)
}

/// A matrix expression's value (none: it could not be read, or it is not a matrix).
pub fn eval_matrix(blob: &[u8], cx: &Context) -> Option<M16> {
    match run(blob, cx)? {
        Val::M(m) => Some(m),
        Val::V(_) => None,
    }
}

fn run(blob: &[u8], cx: &Context) -> Option<Val> {
    let mut stack: Vec<Val> = Vec::new();
    let mut locals: HashMap<u8, Val> = HashMap::new();
    let mut pos = 0usize;
    let u16_at = |p: usize| -> Option<usize> { Some(u16::from_le_bytes(blob.get(p..p + 2)?.try_into().ok()?) as usize) };
    let u32_at = |p: usize| -> Option<u32> { Some(u32::from_le_bytes(blob.get(p..p + 4)?.try_into().ok()?)) };
    let mut steps = 0;
    while pos < blob.len() {
        steps += 1;
        if steps > 10_000 {
            return None;
        }
        let op = blob[pos];
        pos += 1;
        match op {
            0x00 => return stack.pop(),
            0x01 => {}
            0x02 => pos = u16_at(pos)?,
            0x04 => {
                let (t, f) = (u16_at(pos)?, u16_at(pos + 2)?);
                let c = stack.pop()?.v();
                pos = if c[0] != 0.0 { t } else { f };
            }
            0x06 => {
                let id = *blob.get(pos)?;
                pos += 2;
                let n = *ARGS.get(id as usize)?;
                if stack.len() < n {
                    return None;
                }
                let args = stack.split_off(stack.len() - n);
                stack.push(call(id, &args));
            }
            0x07 => {
                stack.push(Val::V(all(f32::from_le_bytes(blob.get(pos..pos + 4)?.try_into().ok()?))));
                pos += 4;
            }
            0x08 => {
                let v = stack.pop()?;
                locals.insert(*blob.get(pos)?, v);
                pos += 1;
            }
            0x09 => {
                stack.push(*locals.get(blob.get(pos)?)?);
                pos += 1;
            }
            0x0A | 0x0B | 0x0D..=0x17 => {
                let (b, a) = (stack.pop()?.v(), stack.pop()?.v());
                let r = match op {
                    0x0A => zip(a, b, |x, y| ((x != 0.0) || (y != 0.0)) as i32 as f32),
                    0x0B => zip(a, b, |x, y| ((x != 0.0) && (y != 0.0)) as i32 as f32),
                    0x0D => zip(a, b, |x, y| (x == y) as i32 as f32),
                    0x0E => zip(a, b, |x, y| (x != y) as i32 as f32),
                    0x0F => zip(a, b, |x, y| (x > y) as i32 as f32),
                    0x10 => zip(a, b, |x, y| (x >= y) as i32 as f32),
                    0x11 => zip(a, b, |x, y| (x < y) as i32 as f32),
                    0x12 => zip(a, b, |x, y| (x <= y) as i32 as f32),
                    0x13 => zip(a, b, |x, y| x + y),
                    0x14 => zip(a, b, |x, y| x - y),
                    0x15 => zip(a, b, |x, y| x * y),
                    0x16 => zip(a, b, |x, y| x / y),
                    0x17 => zip(a, b, |x, y| x % y),
                    _ => return None,
                };
                stack.push(Val::V(r));
            }
            0x0C => {
                let a = stack.pop()?.v();
                stack.push(Val::V(map(a, |v| (v == 0.0) as i32 as f32)));
            }
            0x18 => {
                let a = stack.pop()?.v();
                stack.push(Val::V(map(a, |v| -v)));
            }
            0x19 | 0x1D => {
                let t = u32_at(pos)?;
                pos += 4;
                // a texture's parameter reads as its average colour
                let v = cx.params.get(&t).or_else(|| cx.averages.get(&t)).copied().unwrap_or([0.0; 4]);
                stack.push(Val::V(v));
            }
            0x1F => {
                let t = u32_at(pos)?;
                pos += 4;
                stack.push(Val::V(all(cx.params.contains_key(&t) as i32 as f32)));
            }
            0x1A => {
                let f = *blob.get(pos)? as usize;
                pos += 1;
                stack.push(Val::V(all(cx.features.get(f).copied().unwrap_or(0.0))));
            }
            0x22 => {
                pos += 1;
                stack.push(Val::V(all(0.0)));
            }
            0x1E => {
                let s = *blob.get(pos)?;
                pos += 1;
                let a = stack.pop()?.v();
                stack.push(Val::V(std::array::from_fn(|k| a[((s >> (k * 2)) & 3) as usize])));
            }
            _ => return None,
        }
    }
    stack.pop()
}

/// A hex string's bytes.
pub fn hex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).filter_map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_hash_as_valves() {
        // the tokens CS2's customweapon shader's expressions carry
        assert_eq!(token("g_vColor0"), 0x15C3_C369);
        assert_eq!(token("g_vPaintDurability"), 0xF214_79CB);
        assert_eq!(token("g_nPatternTextureHorizontalSampling"), 0x0FB8_FA28);
    }

    #[test]
    fn a_paints_colour_goes_linear_and_its_pattern_turns() {
        let mut params = HashMap::new();
        params.insert(token("g_vColor0"), [0.5, 0.5, 0.5, 1.0]);
        params.insert(token("g_vPaintDurability"), [0.25, 0.0, 0.0, 0.0]);
        let cx = Context { params: &params, features: &[], averages: &HashMap::new() };
        // SrgbGammaToLinear(g_vColor0)
        let c = eval(&hex("1d69c3c315061f0000"), &cx).unwrap();
        assert!((c[0] - 0.21404).abs() < 1e-4, "{c:?}");
        // 1.0 - g_vPaintDurability
        let d = eval(&hex("070000803f1dcb7914f21400"), &cx).unwrap();
        assert!((d[0] - 0.75).abs() < 1e-6);
        // g_vPatternTexCoordXform0 (CS2's, decompiled): rotation -55, scale 1.26, offset -0.7, the size ignored
        let x0 = "1d9da9a45d070ad7a33b13070000c84215060400070000c8421608001d2268dbdd1d07fceb79042b003300070000803f0238001dba7e565415070ad7a33b13070000c84215060400070000c8421608011da49496f81e00070ad7a33b13070000c84215060400070000c842160802090007db0f49401507000034431608030903060100080409030600000805070000003f090107000000000e049e00a300090102a800070000803f16080609031806010008070903180600000808090609071509060908151408090909090815090609071513080a090409011509051809011507000000000901090415090915090109051815090a15130902070000003f141306180000";
        params.insert(token("g_flPatternTexCoordRotation"), [-55.0; 4]);
        params.insert(token("g_flPatternTexCoordScale"), [1.26; 4]);
        params.insert(token("g_vPatternTexCoordOffset"), [-0.7, -0.7, 0.0, 0.0]);
        params.insert(token("g_bIgnoreWeaponSizeScale"), [1.0; 4]);
        let cx = Context { params: &params, features: &[], averages: &HashMap::new() };
        let v = eval(&hex(x0), &cx).unwrap();
        // float4(cos·s, -sin·s, 0, …): rotation -55° → cos 0.5736, -sin 0.8192; ×1.26
        assert!((v[0] - 0.5736 * 1.26).abs() < 1e-3 && (v[1] - 0.8192 * 1.26).abs() < 1e-3 && v[2] == 0.0, "{v:?}");
    }

    #[test]
    fn colour_matrices_are_vrfs() {
        let close = |a: &M16, b: &M16| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5);
        // grey and white: no tint
        assert!(close(&color_tint2([0.5; 3], 1.0), &IDENTITY));
        assert!(close(&color_tint2([1.0; 3], 1.0), &IDENTITY));
        // VRF's TestMatrixColorTint2_WarmColor and _BluishColor
        let warm: M16 = [
            0.16458952, 0.44803202, 0.0045659216, 0.57826537, 0.039589547, 0.57303196, 0.0045659216, 0.053265363, 0.039589554, 0.44803208, 0.12956592,
            -0.034234628, 0.0, 0.0, 0.0, 1.0,
        ];
        assert!(close(&color_tint2([0.8, 0.2, 0.1], 0.5), &warm), "{:?}", color_tint2([0.8, 0.2, 0.1], 0.5));
        let blue: M16 = [
            0.351208, 0.20228605, 0.0020615086, 0.07141057, 0.017874645, 0.5356194, 0.00206151, 0.27141058, 0.01787465, 0.20228608, 0.3353949, 0.47141054,
            0.0, 0.0, 0.0, 1.0,
        ];
        assert!(close(&color_tint2([0.3, 0.6, 0.9], 1.0), &blue));
        // VRF's TestMatrixColorCorrect2_Identity and _Adjusted
        assert!(close(&color_correct2([1.0; 3], [0.5; 3]), &IDENTITY));
        let adjusted: M16 = [
            1.468957, 0.32770345, 0.003339633, -0.095573045, 0.028956933, 1.7677034, 0.0033396427, -0.11957306, 0.02895683, 0.32770318, 1.4433398,
            -0.14357306, 0.0, 0.0, 0.0, 1.0,
        ];
        assert!(close(&color_correct2([1.2, 0.8, 1.5], [0.3, 0.4, 0.5]), &adjusted), "{:?}", color_correct2([1.2, 0.8, 1.5], [0.3, 0.4, 0.5]));
    }
}
