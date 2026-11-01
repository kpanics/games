//! Ordered transforms from the supplied Newcode 2.09, Catlavan 1.16.5 and Nurik sources.
//! Unlike editor keyframes, these preserve the source formulas and matrix multiplication order.
use std::f32::consts::PI;

pub(super) const SOURCES: [&str; 4] = ["Recode", "Nurik", "Catlavan", "Newcode"];
pub(super) const NURIK: [&str; 4] = ["Swing 1", "Swing 2", "Swing 3", "Swing 4"];
pub(super) const CAT: [&str; 11] = [
    "Mode 1", "Mode 2", "Mode 3", "Mode 4", "Mode 5", "Mode 6", "Mode 7", "360", "Fade", "Slant",
    "New",
];
pub(super) const NEWCODE: [&str; 9] = [
    "None",
    "Self",
    "Block",
    "Block Old",
    "Smooth",
    "Rotate",
    "Break",
    "Slap",
    "Push",
];

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Op {
    Move(f32, f32, f32),
    Rotate(f32, f32, f32, f32),
    Scale(f32),
}
#[derive(Default)]
pub(super) struct Ops(pub Vec<Op>);
impl Ops {
    fn t(&mut self, x: f32, y: f32, z: f32) {
        self.0.push(Op::Move(x, y, z));
    }
    fn r(&mut self, x: f32, y: f32, z: f32, a: f32) {
        self.0.push(Op::Rotate(x, y, z, a));
    }
    fn x(&mut self, a: f32) {
        self.r(1.0, 0.0, 0.0, a);
    }
    fn y(&mut self, a: f32) {
        self.r(0.0, 1.0, 0.0, a);
    }
    fn z(&mut self, a: f32) {
        self.r(0.0, 0.0, 1.0, a);
    }
    fn scale(&mut self, s: f32) {
        self.0.push(Op::Scale(s));
    }
    pub fn wire(&self) -> String {
        self.0
            .iter()
            .map(|op| match op {
                Op::Move(x, y, z) => format!("t {x} {y} {z}"),
                Op::Rotate(x, y, z, a) => format!("r {x} {y} {z} {a}"),
                Op::Scale(s) => format!("s {s}"),
            })
            .collect::<Vec<_>>()
            .join(";")
    }
}
// MathHelper's default 65536-entry sine table. Keep float argument/index rounding from the source.
fn sin(a: f32) -> f32 {
    (((a * 10430.378) as i32 & 65535) as f64 * std::f64::consts::TAU / 65536.0).sin() as f32
}
fn vanilla(o: &mut Ops, p: f32, d: f32, power: f32) {
    let a = sin(p * p * PI);
    let b = sin(p.sqrt() * PI);
    o.y(d * (45.0 + a * -20.0));
    o.z(d * b * -20.0);
    o.x(b * -power);
    o.y(d * -45.0);
}
#[derive(Default)]
pub(super) struct State {
    context: String,
    active: bool,
    flipped: bool,
}
impl State {
    pub fn flipped(&self) -> bool {
        self.flipped
    }
    pub fn phase(&mut self, context: &str, p: f32) -> bool {
        if self.context != context {
            *self = Self {
                context: context.into(),
                ..Default::default()
            };
        }
        let active = p > 0.01;
        if active && !self.active {
            self.flipped = !self.flipped;
        }
        self.active = active;
        self.flipped
    }
}
#[derive(Clone, Copy)]
pub(super) struct Input {
    pub p: f32,
    pub direction: f32,
    pub angle: f32,
    pub power: f32,
    pub equip: f32,
    pub time_ms: u64,
    pub time_ticks: f32,
    pub spinning: bool,
    pub smoothness: f32,
    pub active: bool,
    pub tick_active: bool,
    pub flipped: bool,
    pub kind: u8,
}

pub(super) fn render(source: usize, mode: usize, a: Input) -> Ops {
    let mut o = Ops::default();
    let p = a.p.clamp(0.0, 1.0);
    let d = a.direction;
    let wave = (p as f64 * std::f64::consts::PI).sin() as f32;
    let root = sin(p.sqrt() * PI);
    let square = sin(p * p * PI);
    match source {
        1 => match mode {
            // Nurik handlers replace the entire hand translation; the bridge removes the vanilla base first.
            0 => {
                o.t(d * 0.56, -0.52, -0.72);
                vanilla(&mut o, p, d, a.power * 10.0);
            }
            1 => {
                o.t(d * 0.56, -0.52, -0.72);
                o.x(if a.spinning {
                    -a.time_ticks * 25.0 / a.smoothness.max(3.0)
                } else {
                    -p * 360.0
                });
            }
            2 => {
                o.t(d * 0.1, 0.0, -0.2);
                o.t(d * 0.5, -0.4, -0.82);
                o.y(d * 90.0);
                o.z(d * -60.0);
                o.x(-80.0 - a.power * 10.0 * sin(p * PI));
            }
            _ => {
                o.t(
                    d * -0.4 * root,
                    0.2 * sin(p.sqrt() * (PI * 2.0)),
                    -0.2 * sin(p * PI),
                );
                o.t(d * 0.56, -0.52, -0.72);
                vanilla(&mut o, p, d, 80.0);
            }
        },
        2 => match mode {
            0 => {
                o.y(d * (20.0 + square / 4.0 * -10.0));
                o.z(d * root * -20.0);
                o.x(root * -a.power * 10.0);
                o.y(-d * 45.0);
            }
            1 => {
                o.t(0.0, 0.15, -0.3);
                o.y(d * 90.0);
                o.z(d * -60.0);
                o.x(-a.angle - a.power * 10.0 * wave);
            }
            2 => o.x(p * PI - a.power * 10.0 * wave),
            3 => {
                o.t(0.0, 0.15, -0.3);
                o.y(d * 70.0);
                o.z(d * -30.0);
                o.scale(0.9);
                o.x(-a.angle - a.power * 10.0 * wave);
            }
            4 => {
                if a.tick_active {
                    o.y(d * 45.0);
                    o.x(root * -20.0);
                    o.z(d * root * -20.0);
                    o.x(root * -80.0);
                    o.t(d * 0.4, 0.2, 0.2);
                    o.t(d * -0.5, 0.08, 0.0);
                    o.y(d * 20.0);
                    o.x(-80.0);
                    o.y(d * 20.0);
                }
            }
            5 => vanilla(&mut o, p, d, a.power * 10.0),
            6 => {
                o.t(d * (0.4 - wave * 0.3), 0.0, -wave * 0.2);
                o.y(d * 90.0);
                o.z(d * -30.0);
                o.x(-70.0 - 30.0 * wave);
            }
            7 => o.x(-360.0 * p),
            8 => {
                let b = sin((p * p).sqrt() * PI);
                o.y(d * (45.0 + square * -5.0));
                o.z(d * b * -20.0);
                o.x(b * -(a.power * 8.0));
                o.y(d * -45.0);
            }
            9 => {
                o.t(0.0, 0.0, -0.3 * wave);
                o.x(wave * -35.0);
                o.z(wave * 35.0);
            }
            _ => cat_new(&mut o, a),
        },
        3 => match mode {
            1 | 2 => {
                o.y(90.0);
                o.z(if mode == 1 { -70.0 } else { -30.0 });
                o.x(-a.angle - a.power * 10.0 * wave);
            }
            3 => {
                o.t(0.56, -0.44, -0.71999997);
                o.t(0.0, (a.equip / 3.0) * -0.6, 0.0);
                o.y(45.0);
                o.x(square * -20.0);
                o.z(root * -20.0);
                o.x(root * -80.0);
                o.t(0.4, 0.2, 0.2);
                o.t(-0.5, 0.08, 0.0);
                o.y(20.0);
                o.x(-80.0);
                o.y(20.0);
            }
            4 => {
                o.t(d * 0.56, -0.52, -0.72);
                o.y(d * (45.0 + square / 4.0 * -20.0));
                o.z(d * root * -20.0);
                o.x(root * -a.power);
                o.y(d * -45.0);
            }
            5 => {
                o.t(d * 0.56, -0.52, -0.72);
                o.x(wave * -a.power);
            }
            6 => {
                o.t(0.8, -0.4, -1.1);
                o.y(-45.0);
                o.x(root * -40.0);
                o.t(-0.5, 0.2, 0.0);
                o.y(30.0);
                o.x(-80.0);
                o.y(60.0);
            }
            7 => {
                let f12 = 0.2 * sin(p.sqrt() * (PI * 2.0));
                let f19 = if f12 > 0.5 { 1.0 - f12 } else { f12 };
                o.t(0.96, -0.02, -0.72);
                o.y(45.0);
                o.t(-0.5, 0.2, 0.0);
                o.y(30.0);
                o.x(-80.0);
                o.y(60.0);
                o.t(0.3, 0.0, 0.4);
                o.t(0.0, 0.5, 0.0);
                o.r(1.0, 0.0, -1.0, 90.0);
                o.t(0.6, 0.5, 0.0);
                o.r(1.0, 0.0, -1.0, -90.0);
                o.r(1.0, 0.0, -1.0, -10.0);
                o.r(10.0, 10.0, -9.0, -f19 * 10.0);
                o.x(-10.0);
                o.t(0.0, 0.0, -0.5);
                let clock = a.time_ms % 255;
                let n = if clock > 127 { 255 - clock } else { clock } as f32;
                if a.active {
                    o.r(1.0, 0.0, 1.0, -n * (a.power / 200.0));
                }
                o.t(0.0, 0.0, 0.5);
            }
            8 => {
                o.t(0.46, -0.3, -0.72);
                o.y(45.0);
                o.y(square * -20.0);
                o.z(root * -20.0);
                o.x(root * -40.0);
                o.t(-0.5, 0.2, 0.0);
                o.y(30.0);
                o.x(-80.0);
                o.y(60.0);
            }
            _ => vanilla(&mut o, p, d, 80.0),
        },
        _ => {}
    }
    o
}
fn cat_new(o: &mut Ops, a: Input) {
    let p = a.p;
    let d = a.direction;
    let v = sin(if p < 0.6 {
        p.clamp(0.0, 0.12506) * 12.56
    } else {
        p.clamp(0.62532, 0.75038) * 12.56
    });
    let q = sin(p * PI);
    let c = 1.70158_f32 * 1.525;
    let b = if q < 0.5 {
        ((2.0 * q) as f64).powi(2) * (((c + 1.0) * 2.0 * q - c) as f64) / 2.0
    } else {
        (((2.0 * q - 2.0) as f64).powi(2) * (((c + 1.0) * (q * 2.0 - 2.0) + c) as f64) + 2.0) / 2.0
    } as f32;
    // kind: sword, axe, shovel, tool, spear, blocking, other.
    let k = a.kind;
    let forward = a.flipped || k == 1 || k == 4 || k == 5;
    if forward && k != 2 {
        if k == 0 || k == 1 {
            o.t(0.8 * d * v, 0.3 * v, -0.5 * b);
            o.y(15.0 * v * d);
            o.x(20.0 * v);
            o.z(-70.0 * v * d);
            o.x(-(if k == 0 { 40.0 } else { 30.0 }) * b);
        } else if k == 4 {
            o.t(0.0, 0.0, 0.45 * v);
            o.t(-0.25 * d * b, -0.35 * v, -0.6 * b);
            o.t(0.0, 0.1 * b, 0.0);
            o.y(15.0 * v * d);
            o.z(30.0 * v * d);
        } else if k == 3 {
            o.t(0.1 * d * v, 0.1 * v, -0.5 * b);
            o.x(30.0 * v);
            o.z(-20.0 * v * d);
            o.x(-40.0 * b);
        } else if k != 5 {
            o.t(0.1 * d * v, 0.1 * v, -0.1 * b);
            o.x(30.0 * v);
            o.z(-10.0 * v * d);
            o.x(-40.0 * b);
            o.y(10.0 * b * d);
        } else {
            o.t(0.1 * d * v, 0.1 * v, -0.2 * b);
            o.x(10.0 * v);
            o.z(-10.0 * v * d);
            o.x(-20.0 * b);
        }
    } else if k == 2 {
        o.t(0.0, 0.15 * v, -0.25 * v);
        o.t(0.0, 0.0, -0.2 * b);
        o.y(15.0 * v);
        o.x(-35.0 * v);
        o.x(30.0 * b);
    } else if k == 0 {
        o.t(-0.55 * d * v, -0.8 * v, -0.77 * b);
        o.y(5.0 * v * d);
        o.x(30.0 * v);
        o.z(70.0 * v * d);
        o.x(-50.0 * b);
    } else {
        o.t(
            0.1 * d * v,
            0.1 * v,
            if k == 3 { -0.5 * b } else { -0.1 * b },
        );
        o.x(30.0 * v);
        o.z(if k == 3 { -20.0 * v * d } else { -10.0 * v * d });
        o.x(-40.0 * b);
        if k != 3 {
            o.y(10.0 * b * d);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matches_original_java_renderers_across_progress_hands_settings_and_items() {
        let cases: serde_json::Value =
            serde_json::from_str(include_str!("../../test-data/swing-source-reference.json"))
                .unwrap();
        for (index, c) in cases.as_array().unwrap().iter().enumerate() {
            let f = |key: &str| c[key].as_f64().unwrap() as f32;
            let a = Input {
                p: f("p"),
                direction: f("d"),
                angle: f("angle"),
                power: f("power"),
                equip: f("equip"),
                time_ms: c["time"].as_u64().unwrap(),
                time_ticks: f("time_ticks"),
                spinning: c["spinning"].as_bool().unwrap(),
                smoothness: f("smoothness"),
                active: c["active"].as_bool().unwrap(),
                tick_active: c["active"].as_bool().unwrap(),
                flipped: c["flipped"].as_bool().unwrap(),
                kind: c["kind"].as_u64().unwrap() as u8,
            };
            let ops = render(
                c["source"].as_u64().unwrap() as usize,
                c["mode"].as_u64().unwrap() as usize,
                a,
            );
            let actual: Vec<Vec<f32>> = ops
                .0
                .into_iter()
                .filter_map(|op| match op {
                    Op::Move(x, y, z) if x != 0.0 || y != 0.0 || z != 0.0 => {
                        Some(vec![0.0, x, y, z])
                    }
                    Op::Rotate(x, y, z, angle) if angle != 0.0 => Some(vec![1.0, x, y, z, angle]),
                    Op::Scale(v) if v != 1.0 => Some(vec![2.0, v]),
                    _ => None,
                })
                .collect();
            let expected: Vec<Vec<f32>> = serde_json::from_value(c["ops"].clone()).unwrap();
            assert_eq!(
                actual.len(),
                expected.len(),
                "case {index}: source {} mode {} p {}",
                c["source"],
                c["mode"],
                c["p"]
            );
            for (i, (got, want)) in actual.iter().zip(&expected).enumerate() {
                assert_eq!(got.len(), want.len());
                // Axis signs are equivalent when all signs, including angle, are reversed.
                let mut want = want.clone();
                if got[0] == 1.0
                    && got[1..4]
                        .iter()
                        .zip(&want[1..4])
                        .all(|(x, y)| (*x + *y).abs() < 1e-6)
                {
                    for v in &mut want[1..] {
                        *v = -*v;
                    }
                }
                for (j, (x, y)) in got.iter().zip(&want).enumerate() {
                    assert!(
                        (x - y).abs() < 0.0001,
                        "case {index}, op {i}, component {j}: {x} != {y}, source {} mode {} p {}",
                        c["source"],
                        c["mode"],
                        c["p"]
                    );
                }
            }
        }
    }

    #[test]
    fn alternating_swings_reset_with_world_or_mode_and_do_not_flip_each_frame() {
        let mut s = State::default();
        assert!(!s.phase("world1", 0.0));
        assert!(s.phase("world1", 0.2));
        assert!(s.phase("world1", 0.8));
        assert!(s.phase("world1", 0.0));
        assert!(!s.phase("world1", 0.2));
        assert!(s.phase("world2", 0.2));
        assert!(!s.phase("world3", 0.0));
    }
}
