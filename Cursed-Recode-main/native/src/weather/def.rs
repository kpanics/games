//! The weather's particle systems as the game keeps them (assets/weather/systems.json: their KeyValues3 as JSON, made by
//! tools/weather_pack.py), read into what the simulation runs: each function's numbers, with Source 2's own defaults
//! where the file leaves them out (as ValveResourceFormat's particle renderer reads them, MIT). Functions the file turns
//! off are left out; a function the weather does not use is not read.

use serde_json::Value;

pub type V3 = [f32; 3];

/// A particle's fields by the engine's numbers (ParticleField).
pub const POS: u8 = 0;
pub const LIFE: u8 = 1;
pub const PREV: u8 = 2;
pub const RADIUS: u8 = 3;
pub const ROLL: u8 = 4;
pub const ROLL_SPEED: u8 = 5;
pub const COLOR: u8 = 6;
pub const ALPHA: u8 = 7;
pub const TRAIL: u8 = 10;
pub const YAW: u8 = 12;
pub const ALPHA2: u8 = 16;
pub const SCRATCH: u8 = 18;
pub const PITCH: u8 = 20;
pub const SCRATCH1: u8 = 26;

/// Whether a field is an angle: authored in degrees, kept in radians.
pub fn angle(field: u8) -> bool {
    matches!(field, ROLL | ROLL_SPEED | YAW | PITCH)
}

fn f(o: &Value, k: &str, d: f32) -> f32 {
    o.get(k).and_then(Value::as_f64).map_or(d, |v| v as f32)
}

fn int(o: &Value, k: &str, d: i32) -> i32 {
    o.get(k).and_then(Value::as_i64).map_or(d, |v| v as i32)
}

fn b(o: &Value, k: &str, d: bool) -> bool {
    o.get(k).and_then(Value::as_bool).unwrap_or(d)
}

fn s<'a>(o: &'a Value, k: &str) -> &'a str {
    o.get(k).and_then(Value::as_str).unwrap_or("")
}

/// A vector: written out, or a literal vector input.
fn v3(o: &Value, k: &str, d: V3) -> V3 {
    let Some(v) = o.get(k) else { return d };
    let a = v.as_array().or_else(|| v.get("m_vLiteralValue").and_then(Value::as_array));
    match a {
        Some(a) if a.len() >= 3 => std::array::from_fn(|i| a[i].as_f64().unwrap_or(0.0) as f32),
        _ => d,
    }
}

/// A colour of 0..255 as 0..1.
fn colour(o: &Value, k: &str, d: V3) -> V3 {
    match o.get(k).and_then(Value::as_array) {
        Some(a) if a.len() >= 3 => std::array::from_fn(|i| a[i].as_f64().unwrap_or(255.0) as f32 / 255.0),
        _ => d,
    }
}

fn int_field(o: &Value, k: &str, d: u8) -> u8 {
    int(o, k, d as i32).clamp(0, 255) as u8
}

/// A number input: a literal, a random one (the same for a particle all its life when drawn at its birth), or a
/// control point's component times a factor.
#[derive(Clone, Copy, Debug)]
pub enum Num {
    Lit(f32),
    Rand { lo: f32, hi: f32, flip: bool },
    Cp { cp: usize, comp: usize, mult: f32 },
}

fn num(o: &Value, k: &str, d: f32) -> Num {
    let Some(v) = o.get(k) else { return Num::Lit(d) };
    if let Some(x) = v.as_f64() {
        return Num::Lit(x as f32);
    }
    match s(v, "m_nType") {
        "PF_TYPE_LITERAL" => Num::Lit(f(v, "m_flLiteralValue", d)),
        "PF_TYPE_RANDOM_UNIFORM" => Num::Rand { lo: f(v, "m_flRandomMin", 0.0), hi: f(v, "m_flRandomMax", 0.0), flip: b(v, "m_bHasRandomSignFlip", false) },
        "PF_TYPE_CONTROL_POINT_COMPONENT" => Num::Cp {
            cp: int(v, "m_nControlPoint", 0).clamp(0, 63) as usize,
            comp: int(v, "m_nVectorComponent", 0).clamp(0, 2) as usize,
            mult: if s(v, "m_nMapType") == "PF_MAP_TYPE_MULT" { f(v, "m_flMultFactor", 1.0) } else { 1.0 },
        },
        _ => Num::Lit(d),
    }
}

/// When a function runs at full strength over the system's life: faded in, out, over and over (the operator's own
/// window, m_flOp…); without one, always.
#[derive(Clone, Copy, Debug, Default)]
pub struct Window {
    start_in: f32,
    end_in: f32,
    start_out: f32,
    end_out: f32,
    period: f32,
    always: bool,
}

impl Window {
    fn of(o: &Value) -> Window {
        let w = Window {
            start_in: f(o, "m_flOpStartFadeInTime", 0.0),
            end_in: f(o, "m_flOpEndFadeInTime", 0.0),
            start_out: f(o, "m_flOpStartFadeOutTime", 0.0),
            end_out: f(o, "m_flOpEndFadeOutTime", 0.0),
            period: f(o, "m_flOpFadeOscillatePeriod", 0.0),
            always: false,
        };
        Window { always: w.start_in == 0.0 && w.end_in == 0.0 && w.start_out == 0.0 && w.end_out == 0.0, ..w }
    }

    /// Its strength when the system is `age` seconds old.
    pub fn strength(&self, age: f32) -> f32 {
        if self.always {
            return 1.0;
        }
        let mut t = age;
        if self.period > 0.0 {
            t = (t / self.period).fract();
        }
        let (fis, fos_end) = (self.start_in, self.end_out);
        if fis > t || (fos_end > 0.0 && fos_end < t) {
            return 0.0;
        }
        let fie = self.end_in.max(fis);
        let fos = self.start_out.max(fie);
        let foe = fos_end.max(fos);
        let lerp = |f1: f32, f2: f32, i1: f32, i2: f32, x: f32| f1 + (f2 - f1) * (x - i1) / (i2 - i1);
        let mut k: f32 = 1.0;
        if fie > t && fie > fis {
            k = k.min(lerp(0.0, 1.0, fis, fie, t));
        }
        if t > fos && foe > fos {
            k = k.min(lerp(0.0, 1.0, foe, fos, t));
        }
        k
    }
}

/// A function with its window.
#[derive(Clone, Debug)]
pub struct Fun<T> {
    pub f: T,
    pub w: Window,
}

#[derive(Clone, Debug)]
pub enum Pre {
    /// A control point put at a place in another's frame (or the world's).
    SetCp { cp: usize, pos: V3, once: bool, world: bool, head: usize },
    /// A control point put somewhere at random round another, again every `rate` seconds.
    RandomCp { cp: usize, lo: V3, hi: V3, world: bool, head: usize, rate: f32, interp: f32 },
}

#[derive(Clone, Debug)]
pub struct Emitter {
    pub rate: Num,
    pub duration: f32,
    pub start: f32,
    /// The rate for each particle of the parent system (0: not by them).
    pub per_parent: f32,
}

#[derive(Clone, Debug)]
pub enum Init {
    Float { field: u8, value: Num, strength: Num },
    Sphere { cp: usize, r_lo: Num, r_hi: Num, s_lo: Num, s_hi: Num, s_exp: f32, local_lo: V3, local_hi: V3, bias: V3, bias_abs: V3, local: bool },
    Sequence { lo: i32, hi: i32 },
    VelocityNoise { lo: V3, hi: V3, scale: f32, scale_loc: f32, offset: f32, offset_loc: V3, cp: usize },
    Offset { lo: V3, hi: V3, local: bool, proportional: bool },
    Color { lo: V3, hi: V3, tint_lo: V3, tint_hi: V3, tint_perc: f32, tint_cp: usize, add: bool },
    YawFlip { percent: f32 },
    FromParent { velocity_scale: f32, increment: f32, random: bool, subframe: bool },
    InheritParent { scale: f32, field: u8, increment: i32, random: bool },
    OnGround { offset: f32, max: f32 },
}

#[derive(Clone, Debug)]
pub enum Op {
    Decay,
    Movement { gravity: V3, drag: f32 },
    FadeIn { lo: f32, hi: f32, exp: f32, proportional: bool },
    FadeOut { lo: f32, hi: f32, exp: f32, bias: f32, proportional: bool },
    FadeInSimple { time: f32, field: u8 },
    FadeOutSimple { time: f32, field: u8 },
    Oscillate { field: u8, rate: (f32, f32), freq: (f32, f32), start: (f32, f32), end: (f32, f32), mult: f32, add: f32, proportional: bool, window_proportional: bool },
    OscillateVec { field: u8, rate: (V3, V3), freq: (V3, V3), mult: f32, add: f32, proportional: bool },
    Radius { start: f32, end: f32, from: Num, to: Num, ease: bool, bias: Num },
    Cull { cp: usize, distance: Num, offset: V3, inside: bool },
    LoopInSphere { cp: usize, distance: f32, scale: V3 },
    VecNoise { field: u8, lo: V3, hi: V3, scale: f32, time_scale: f32, additive: bool },
    Clamp { field: u8, lo: Num, hi: Num },
    ColorFade { to: V3, start: f32, end: f32, ease: bool },
    RampSpline { field: u8, rate: (f32, f32), start: f32, end: f32, ease_out: bool },
    PlaceOnGround { offset: f32, max: f32, trace_offset: f32 },
}

#[derive(Clone, Debug)]
pub enum Force {
    Twist { amount: f32, axis: V3, local: bool, cp: usize },
    Random { lo: V3, hi: V3 },
}

/// How a renderer's particles are coloured (shared by sprites and trails).
#[derive(Clone, Debug)]
pub struct Look {
    pub texture: String,
    pub additive: bool,
    pub overbright: f32,
    /// Its diffuse amount and self-illumination together.
    pub factor: f32,
    /// How much of it is added rather than laid over (1: all).
    pub add_self: f32,
    /// The texture's alpha mapped from this range to 0..1 (none: as it is).
    pub remap: Option<(f32, f32)>,
    pub rate: f32,
    pub blend_frames: bool,
    pub gamma: bool,
    pub saturate: bool,
    /// Tinted by the light where it is.
    pub light: bool,
    /// Faded where it meets what is behind it.
    pub feather: bool,
    pub radius_scale: f32,
    pub alpha_scale: f32,
}

#[derive(Clone, Debug)]
pub enum Render {
    /// Quads: turned to the camera, or lying flat in the world.
    Sprite { look: Look, flat: bool, min_size: f32, max_size: f32 },
    /// A ribbon along the way it moves.
    Trail { look: Look, max_len: f32, min_len: f32, len_scale: f32, ratio: f32, tail_colour: V3, tail_alpha: f32, len_fade_in: f32 },
}

#[derive(Clone, Debug)]
pub struct Child {
    pub name: String,
    pub delay: f32,
}

/// A particle system.
#[derive(Clone, Debug)]
pub struct Def {
    pub name: String,
    pub max: usize,
    pub colour: V3,
    pub alpha: f32,
    pub radius: f32,
    pub life: f32,
    pub step_max: f32,
    pub pre: Vec<Fun<Pre>>,
    pub emitters: Vec<Fun<Emitter>>,
    pub inits: Vec<Init>,
    pub ops: Vec<Fun<Op>>,
    pub forces: Vec<Fun<Force>>,
    pub renders: Vec<Render>,
    pub children: Vec<Child>,
}

fn on(o: &Value) -> bool {
    !b(o, "m_bDisableOperator", false)
}

fn list<'a>(o: &'a Value, k: &str) -> impl Iterator<Item = &'a Value> {
    o.get(k).and_then(Value::as_array).into_iter().flatten().filter(|x| on(x))
}

fn look(o: &Value) -> Look {
    let texture = o
        .get("m_vecTexturesInput")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .map(|t| s(t, "m_hTexture"))
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| s(o, "m_hTexture"))
        .to_string();
    let (zero, one) = (f(o, "m_flSourceAlphaValueToMapToZero", 0.0), f(o, "m_flSourceAlphaValueToMapToOne", 1.0));
    Look {
        texture,
        additive: s(o, "m_nOutputBlendMode") == "PARTICLE_OUTPUT_BLEND_MODE_ADD",
        overbright: f(o, "m_flOverbrightFactor", 1.0),
        factor: f(o, "m_flDiffuseAmount", 1.0) + f(o, "m_flSelfIllumAmount", 0.0),
        add_self: f(o, "m_flAddSelfAmount", 0.0),
        remap: (zero < one && (zero != 0.0 || one != 1.0)).then_some((zero, one)),
        rate: f(o, "m_flAnimationRate", 0.1),
        blend_frames: b(o, "m_bBlendFramesSeq0", true),
        gamma: b(o, "m_bGammaCorrectVertexColors", true),
        saturate: b(o, "m_bSaturateColorPreAlphaBlend", true),
        light: b(o, "m_bTintByGlobalLight", false),
        feather: s(o, "m_nFeatheringMode").starts_with("PARTICLE_DEPTH_FEATHERING_ON"),
        radius_scale: match num(o, "m_flRadiusScale", 1.0) {
            Num::Lit(x) => x,
            _ => 1.0,
        },
        alpha_scale: match num(o, "m_flAlphaScale", 1.0) {
            Num::Lit(x) => x,
            _ => 1.0,
        },
    }
}

fn lit(o: &Value, k: &str, d: f32) -> f32 {
    match num(o, k, d) {
        Num::Lit(x) => x,
        _ => d,
    }
}

impl Def {
    pub fn read(name: &str, o: &Value) -> Def {
        let constant = o.get("m_ConstantColor").and_then(Value::as_array);
        let (constant_colour, alpha) = match constant {
            Some(a) if a.len() >= 4 => (std::array::from_fn(|i| a[i].as_f64().unwrap_or(255.0) as f32 / 255.0), a[3].as_f64().unwrap_or(255.0) as f32 / 255.0),
            _ => ([1.0; 3], 1.0),
        };
        let step_max = f(o, "m_flMaximumTimeStep", 0.1).max(f(o, "m_flMinimumTimeStep", 0.0));
        let mut d = Def {
            name: name.to_string(),
            max: int(o, "m_nMaxParticles", 1000).clamp(1, 5000) as usize,
            colour: constant_colour,
            alpha,
            radius: f(o, "m_flConstantRadius", 5.0),
            life: f(o, "m_flConstantLifespan", 1.0),
            step_max: if step_max > 0.0 { step_max } else { 0.1 },
            pre: Vec::new(),
            emitters: Vec::new(),
            inits: Vec::new(),
            ops: Vec::new(),
            forces: Vec::new(),
            renders: Vec::new(),
            children: Vec::new(),
        };
        for x in list(o, "m_PreEmissionOperators") {
            let w = Window::of(x);
            let set = match s(x, "_class") {
                "C_OP_SetSingleControlPointPosition" => {
                    let placed = x.get("m_bUseWorldLocation").is_some() || x.get("m_nHeadLocation").is_some();
                    Some(Pre::SetCp {
                        cp: int(x, "m_nCP1", 1).clamp(0, 63) as usize,
                        pos: v3(x, "m_vecCP1Pos", [128.0, 0.0, 0.0]),
                        once: b(x, "m_bSetOnce", false),
                        world: placed && b(x, "m_bUseWorldLocation", false),
                        head: if placed { int(x, "m_nHeadLocation", 0).clamp(0, 63) as usize } else { 0 },
                    })
                }
                "C_OP_SetRandomControlPointPosition" => Some(Pre::RandomCp {
                    cp: int(x, "m_nCP1", 1).clamp(0, 63) as usize,
                    lo: v3(x, "m_vecCPMinPos", [0.0; 3]),
                    hi: v3(x, "m_vecCPMaxPos", [0.0; 3]),
                    world: b(x, "m_bUseWorldLocation", false),
                    head: int(x, "m_nHeadLocation", 0).clamp(0, 63) as usize,
                    rate: lit(x, "m_flReRandomRate", -1.0),
                    interp: lit(x, "m_flInterpolation", 1.0),
                }),
                _ => None,
            };
            if let Some(f) = set {
                d.pre.push(Fun { f, w });
            }
        }
        for x in list(o, "m_Emitters") {
            if s(x, "_class") == "C_OP_ContinuousEmitter" {
                d.emitters.push(Fun {
                    f: Emitter {
                        rate: num(x, "m_flEmitRate", 100.0),
                        duration: lit(x, "m_flEmissionDuration", 0.0),
                        start: lit(x, "m_flStartTime", 0.0),
                        per_parent: f(x, "m_flScalePerParentParticle", 0.0),
                    },
                    w: Window::of(x),
                });
            }
        }
        for x in list(o, "m_Initializers") {
            let i = match s(x, "_class") {
                "C_INIT_InitFloat" => Some(Init::Float { field: int_field(x, "m_nOutputField", RADIUS), value: num(x, "m_InputValue", 0.0), strength: num(x, "m_InputStrength", 1.0) }),
                "C_INIT_CreateWithinSphere" => Some(Init::Sphere {
                    cp: int(x, "m_nControlPointNumber", 0).clamp(0, 63) as usize,
                    r_lo: num(x, "m_fRadiusMin", 0.0),
                    r_hi: num(x, "m_fRadiusMax", 0.0),
                    s_lo: num(x, "m_fSpeedMin", 0.0),
                    s_hi: num(x, "m_fSpeedMax", 0.0),
                    s_exp: f(x, "m_fSpeedRandExp", 1.0).clamp(-255.0, 255.0),
                    local_lo: v3(x, "m_LocalCoordinateSystemSpeedMin", [0.0; 3]),
                    local_hi: v3(x, "m_LocalCoordinateSystemSpeedMax", [0.0; 3]),
                    bias: v3(x, "m_vecDistanceBias", [1.0; 3]),
                    bias_abs: v3(x, "m_vecDistanceBiasAbs", [0.0; 3]),
                    local: b(x, "m_bLocalCoords", false),
                }),
                "C_INIT_RandomSequence" => Some(Init::Sequence { lo: int(x, "m_nSequenceMin", 0), hi: int(x, "m_nSequenceMax", 0) }),
                "C_INIT_InitialVelocityNoise" => Some(Init::VelocityNoise {
                    lo: v3(x, "m_vecOutputMin", [0.0; 3]),
                    hi: v3(x, "m_vecOutputMax", [1.0; 3]),
                    scale: lit(x, "m_flNoiseScale", 1.0),
                    scale_loc: lit(x, "m_flNoiseScaleLoc", 0.01),
                    offset: lit(x, "m_flOffset", 0.0),
                    offset_loc: v3(x, "m_vecOffsetLoc", [0.0; 3]),
                    cp: int(x, "m_nControlPointNumber", 0).clamp(0, 63) as usize,
                }),
                "C_INIT_PositionOffset" => Some(Init::Offset { lo: v3(x, "m_OffsetMin", [0.0; 3]), hi: v3(x, "m_OffsetMax", [0.0; 3]), local: b(x, "m_bLocalCoords", false), proportional: b(x, "m_bProportional", false) }),
                "C_INIT_RandomColor" => Some(Init::Color {
                    lo: colour(x, "m_ColorMin", [1.0; 3]),
                    hi: colour(x, "m_ColorMax", [1.0; 3]),
                    tint_lo: colour(x, "m_TintMin", [0.0; 3]),
                    tint_hi: colour(x, "m_TintMax", [1.0; 3]),
                    tint_perc: f(x, "m_flTintPerc", 0.0),
                    tint_cp: int(x, "m_nTintCP", 0).clamp(0, 63) as usize,
                    add: s(x, "m_nTintBlendMode") == "PARTICLE_COLOR_BLEND_ADD",
                }),
                "C_INIT_RandomYawFlip" => Some(Init::YawFlip { percent: f(x, "m_flPercent", 0.5) }),
                "C_INIT_CreateFromParentParticles" => Some(Init::FromParent {
                    velocity_scale: f(x, "m_flVelocityScale", 0.0),
                    increment: f(x, "m_flIncrement", 1.0),
                    random: b(x, "m_bRandomDistribution", false),
                    subframe: b(x, "m_bSubFrame", true),
                }),
                "C_INIT_InheritFromParentParticles" => Some(Init::InheritParent {
                    scale: f(x, "m_flScale", 1.0),
                    field: int_field(x, "m_nFieldOutput", RADIUS),
                    increment: int(x, "m_nIncrement", 1),
                    random: b(x, "m_bRandomDistribution", false),
                }),
                "C_INIT_PositionPlaceOnGround" => Some(Init::OnGround { offset: lit(x, "m_flOffset", 0.0), max: lit(x, "m_flMaxTraceLength", 128.0) }),
                _ => None,
            };
            if let Some(i) = i {
                d.inits.push(i);
            }
        }
        for x in list(o, "m_Operators") {
            let op = match s(x, "_class") {
                "C_OP_Decay" => Some(Op::Decay),
                "C_OP_BasicMovement" => Some(Op::Movement { gravity: v3(x, "m_Gravity", [0.0; 3]), drag: lit(x, "m_fDrag", 0.0) }),
                "C_OP_FadeIn" => Some(Op::FadeIn {
                    lo: f(x, "m_flFadeInTimeMin", 0.25),
                    hi: f(x, "m_flFadeInTimeMax", 0.25),
                    exp: f(x, "m_flFadeInTimeExp", 1.0),
                    proportional: b(x, "m_bProportional", true),
                }),
                "C_OP_FadeOut" => Some(Op::FadeOut {
                    lo: f(x, "m_flFadeOutTimeMin", 0.25),
                    hi: f(x, "m_flFadeOutTimeMax", 0.25),
                    exp: f(x, "m_flFadeOutTimeExp", 1.0),
                    bias: match f(x, "m_flFadeBias", 0.5) {
                        0.0 => 0.5,
                        v => v,
                    },
                    proportional: b(x, "m_bProportional", true),
                }),
                "C_OP_FadeInSimple" => Some(Op::FadeInSimple { time: f(x, "m_flFadeInTime", 0.25), field: int_field(x, "m_nFieldOutput", ALPHA) }),
                "C_OP_FadeOutSimple" => Some(Op::FadeOutSimple { time: f(x, "m_flFadeOutTime", 0.25), field: int_field(x, "m_nFieldOutput", ALPHA) }),
                "C_OP_OscillateScalar" => Some(Op::Oscillate {
                    field: int_field(x, "m_nField", ALPHA),
                    rate: (f(x, "m_RateMin", 0.0), f(x, "m_RateMax", 0.0)),
                    freq: (f(x, "m_FrequencyMin", 1.0), f(x, "m_FrequencyMax", 1.0)),
                    start: (f(x, "m_flStartTime_min", 0.0), f(x, "m_flStartTime_max", 0.0)),
                    end: (f(x, "m_flEndTime_min", 1.0), f(x, "m_flEndTime_max", 1.0)),
                    mult: f(x, "m_flOscMult", 2.0),
                    add: f(x, "m_flOscAdd", 0.5),
                    proportional: b(x, "m_bProportional", true),
                    window_proportional: b(x, "m_bProportionalOp", true),
                }),
                "C_OP_OscillateVector" => Some(Op::OscillateVec {
                    field: int_field(x, "m_nField", POS),
                    rate: (v3(x, "m_RateMin", [0.0; 3]), v3(x, "m_RateMax", [0.0; 3])),
                    freq: (v3(x, "m_FrequencyMin", [1.0; 3]), v3(x, "m_FrequencyMax", [1.0; 3])),
                    mult: lit(x, "m_flOscMult", 2.0),
                    add: lit(x, "m_flOscAdd", 0.5),
                    proportional: b(x, "m_bProportional", true),
                }),
                "C_OP_InterpolateRadius" => Some(Op::Radius {
                    start: f(x, "m_flStartTime", 0.0),
                    end: f(x, "m_flEndTime", 1.0),
                    from: num(x, "m_flStartScale", 1.0),
                    to: num(x, "m_flEndScale", 1.0),
                    ease: b(x, "m_bEaseInAndOut", false),
                    bias: num(x, "m_flBias", 0.0),
                }),
                "C_OP_DistanceCull" => Some(Op::Cull {
                    cp: int(x, "m_nControlPoint", 0).clamp(0, 63) as usize,
                    distance: num(x, "m_flDistance", 0.0),
                    offset: v3(x, "m_vecPointOffset", [0.0; 3]),
                    inside: b(x, "m_bCullInside", false),
                }),
                "C_OP_MovementLoopInsideSphere" => Some(Op::LoopInSphere {
                    cp: int(x, "m_nCP", 0).clamp(0, 63) as usize,
                    distance: lit(x, "m_flDistance", 100.0),
                    scale: v3(x, "m_vecScale", [1.0; 3]),
                }),
                "C_OP_VectorNoise" => Some(Op::VecNoise {
                    field: int_field(x, "m_nFieldOutput", COLOR),
                    lo: v3(x, "m_vecOutputMin", [0.0; 3]),
                    hi: v3(x, "m_vecOutputMax", [1.0; 3]),
                    scale: f(x, "m_fl4NoiseScale", 0.1),
                    time_scale: f(x, "m_flNoiseAnimationTimeScale", 0.0),
                    additive: b(x, "m_bAdditive", false),
                }),
                "C_OP_ClampScalar" => Some(Op::Clamp { field: int_field(x, "m_nFieldOutput", RADIUS), lo: num(x, "m_flOutputMin", 0.0), hi: num(x, "m_flOutputMax", 1.0) }),
                "C_OP_ColorInterpolate" => Some(Op::ColorFade {
                    to: colour(x, "m_ColorFade", [1.0; 3]),
                    start: f(x, "m_flFadeStartTime", 0.0),
                    end: f(x, "m_flFadeEndTime", 1.0),
                    ease: b(x, "m_bEaseInOut", true),
                }),
                "C_OP_RampScalarSpline" => Some(Op::RampSpline {
                    field: int_field(x, "m_nField", RADIUS),
                    rate: (f(x, "m_RateMin", 0.0), f(x, "m_RateMax", 0.0)),
                    start: f(x, "m_flStartTime_min", 0.0),
                    end: f(x, "m_flEndTime_max", 1.0),
                    ease_out: b(x, "m_bEaseOut", false),
                }),
                "C_OP_MovementPlaceOnGround" => Some(Op::PlaceOnGround {
                    offset: lit(x, "m_flOffset", 0.0),
                    max: lit(x, "m_flMaxTraceLength", 128.0),
                    trace_offset: lit(x, "m_flTraceOffset", 64.0),
                }),
                _ => None,
            };
            if let Some(f) = op {
                d.ops.push(Fun { f, w: Window::of(x) });
            }
        }
        for x in list(o, "m_ForceGenerators") {
            let force = match s(x, "_class") {
                "C_OP_TwistAroundAxis" => Some(Force::Twist {
                    amount: f(x, "m_fForceAmount", 0.0),
                    axis: v3(x, "m_TwistAxis", [0.0, 0.0, 1.0]),
                    local: b(x, "m_bLocalSpace", false),
                    cp: int(x, "m_nControlPointNumber", 0).clamp(0, 63) as usize,
                }),
                "C_OP_RandomForce" => Some(Force::Random { lo: v3(x, "m_MinForce", [0.0; 3]), hi: v3(x, "m_MaxForce", [0.0; 3]) }),
                // the world's wind (a map's entity): there is none
                _ => None,
            };
            if let Some(f) = force {
                d.forces.push(Fun { f, w: Window::of(x) });
            }
        }
        for x in list(o, "m_Renderers") {
            match s(x, "_class") {
                "C_OP_RenderSprites" => d.renders.push(Render::Sprite {
                    look: look(x),
                    flat: s(x, "m_nOrientationType") == "PARTICLE_ORIENTATION_WORLD_Z_ALIGNED",
                    min_size: lit(x, "m_flMinSize", 0.0),
                    max_size: lit(x, "m_flMaxSize", 5000.0),
                }),
                "C_OP_RenderTrails" => d.renders.push(Render::Trail {
                    look: look(x),
                    max_len: f(x, "m_flMaxLength", 2000.0),
                    min_len: f(x, "m_flMinLength", 0.0),
                    len_scale: f(x, "m_flLengthScale", 1.0),
                    ratio: f(x, "m_flConstrainRadiusToLengthRatio", 1.0),
                    tail_colour: v3(x, "m_vecTailColorScale", [1.0; 3]),
                    tail_alpha: lit(x, "m_flTailAlphaScale", 1.0),
                    len_fade_in: f(x, "m_flLengthFadeInTime", 0.0),
                }),
                _ => {}
            }
        }
        for x in o.get("m_Children").and_then(Value::as_array).into_iter().flatten() {
            if !b(x, "m_bDisableChild", false) {
                d.children.push(Child { name: s(x, "m_ChildRef").to_string(), delay: f(x, "m_flDelay", 0.0) });
            }
        }
        d
    }
}
