//! A CS sniper rifle in the hands (the AWP, the SSG 08), as CS2 has it — its numbers from its weapons.vdata, its kick
//! as VRF works CS2's out (AimPunchServices, RecoilPattern, UniformRandomStream; MIT): the scope's two zoom levels and
//! their fields of view, the inaccuracy (standing, crouching, moving, in the air, after a shot — the scope's lines blur
//! with it), the kick of a shot into the aim punch and the view punch (the camera's), the bolt (no shot, no scope till
//! the cycle time has passed).
//!
//! Its bow is the rifle: drawn, the scope; let go, the shot (the game's arrow flies as ever, unseen).

use std::sync::OnceLock;

/// A sniper's numbers (weapons.vdata; two of each: [unscoped, scoped]).
pub struct Weapon {
    pub deploy: f32,
    pub cycle: f32,
    /// The zoom levels' fields of view (degrees across a 4:3 picture, as CS's fov).
    pub zoom_fov: [f32; 2],
    pub max_speed: [f32; 2],
    pub inaccuracy_stand: [f32; 2],
    pub inaccuracy_crouch: [f32; 2],
    pub inaccuracy_fire: [f32; 2],
    pub inaccuracy_move: [f32; 2],
    pub inaccuracy_jump_initial: f32,
    pub recovery_stand: f32,
    pub recovery_crouch: f32,
    pub recoil_seed: i32,
    pub recoil_angle: [f32; 2],
    pub recoil_angle_variance: [f32; 2],
    pub recoil_magnitude: [f32; 2],
    pub recoil_magnitude_variance: [f32; 2],
    /// The muzzle in the viewmodel's space (forward, left, up; inches): where the tracer starts.
    pub muzzle: [f32; 3],
    pub sound_shot: &'static str,
    pub sound_zoom: &'static str,
    pub sound_unzoom: &'static str,
}

pub static AWP: Weapon = Weapon {
    deploy: 1.266667,
    cycle: 1.455,
    zoom_fov: [40.0, 10.0],
    max_speed: [200.0, 100.0],
    inaccuracy_stand: [0.0808, 0.002],
    inaccuracy_crouch: [0.0606, 0.0015],
    inaccuracy_fire: [0.05385, 0.05385],
    inaccuracy_move: [0.17648, 0.17648],
    inaccuracy_jump_initial: 0.17286,
    recovery_stand: 0.34539,
    recovery_crouch: 0.24671,
    recoil_seed: 4100,
    recoil_angle: [0.0, 0.0],
    recoil_angle_variance: [20.0, 20.0],
    recoil_magnitude: [78.0, 25.0],
    recoil_magnitude_variance: [15.0, 2.0],
    muzzle: [54.128, -5.022, -3.124],
    sound_shot: "Weapon_AWP.Single",
    sound_zoom: "Weapon_AWP.Zoom",
    sound_unzoom: "Weapon_AWP.ZoomOut",
};

pub static SSG08: Weapon = Weapon {
    deploy: 1.0,
    cycle: 1.25,
    zoom_fov: [40.0, 15.0],
    max_speed: [230.0, 230.0],
    inaccuracy_stand: [0.0317, 0.003],
    inaccuracy_crouch: [0.02378, 0.0028],
    inaccuracy_fire: [0.02292, 0.02292],
    inaccuracy_move: [0.12345, 0.12345],
    inaccuracy_jump_initial: 0.20872,
    recovery_stand: 0.142096,
    recovery_crouch: 0.055783,
    recoil_seed: 1278,
    recoil_angle: [0.0, 0.0],
    recoil_angle_variance: [20.0, 20.0],
    recoil_magnitude: [33.0, 25.0],
    recoil_magnitude_variance: [15.0, 2.0],
    muzzle: [48.901, -5.688, -3.681],
    sound_shot: "Weapon_SSG08.Single",
    sound_zoom: "Weapon_SSG08.Zoom",
    sound_unzoom: "Weapon_SSG08.Zoom",
};

/// Valve's CUniformRandomStream (Numerical Recipes' ran1, its shuffle table): the recoil patterns are drawn from it.
pub struct UniformRandom {
    state: i32,
    previous: i32,
    table: [i32; 32],
}

impl UniformRandom {
    const MULTIPLIER: i32 = 16807;
    const MODULUS: i32 = i32::MAX;
    const QUOTIENT: i32 = 127773;
    const REMAINDER: i32 = 2836;
    const DIVISOR: i32 = 1 + (Self::MODULUS - 1) / 32;

    pub fn new(seed: i32) -> UniformRandom {
        let mut r = UniformRandom { state: 0, previous: 0, table: [0; 32] };
        r.seed(seed);
        r
    }

    pub fn seed(&mut self, seed: i32) {
        self.state = if seed < 0 { seed } else { -seed };
        self.previous = 0;
    }

    fn advance(&mut self) {
        let k = self.state / Self::QUOTIENT;
        self.state = Self::MULTIPLIER.wrapping_mul(self.state - k * Self::QUOTIENT).wrapping_sub(Self::REMAINDER.wrapping_mul(k));
        if self.state < 0 {
            self.state = self.state.wrapping_add(Self::MODULUS);
        }
    }

    fn next(&mut self) -> i32 {
        if self.state <= 0 || self.previous == 0 {
            self.state = if -self.state < 1 { 1 } else { -self.state };
            for i in (0..32 + 8).rev() {
                self.advance();
                if i < 32 {
                    self.table[i] = self.state;
                }
            }
            self.previous = self.table[0];
        }
        self.advance();
        let slot = (self.previous / Self::DIVISOR) as usize;
        self.previous = self.table[slot];
        self.table[slot] = self.state;
        self.previous
    }

    pub fn float(&mut self, low: f32, high: f32) -> f32 {
        let fraction = (self.next() as f32 * (1.0 / Self::MODULUS as f32)).min(1.0 - 1.2e-7);
        fraction * (high - low) + low
    }
}

/// A weapon's first kick of each mode (a sniper's shots are each a first: the recoil index has unwound by the next):
/// (angle, magnitude), [unscoped, scoped].
fn first_kicks(w: &Weapon) -> [(f32, f32); 2] {
    std::array::from_fn(|mode| {
        let mut r = UniformRandom::new(w.recoil_seed);
        let angle = w.recoil_angle[mode] + r.float(-w.recoil_angle_variance[mode], w.recoil_angle_variance[mode]);
        let magnitude = w.recoil_magnitude[mode] + r.float(-w.recoil_magnitude_variance[mode], w.recoil_magnitude_variance[mode]);
        (angle, magnitude)
    })
}

/// CS2's aim punch (CCSPlayer_AimPunchServices): a shot pushes a velocity that dies away; the punch takes it in and
/// falls back both by a share and by a step a second. Degrees (pitch, yaw); up is negative, as Source's.
#[derive(Default, Clone, Copy, Debug)]
pub struct AimPunch {
    velocity: [f32; 2],
    pub punch: [f32; 2],
}

impl AimPunch {
    /// From the punch to where the bullets go; to the view (0.45 of that); to the weapon (another 0.325 past it).
    pub const BULLET: f32 = 2.0;
    pub const VIEW: f32 = Self::BULLET * 0.45;
    pub const VIEWMODEL: f32 = Self::VIEW + Self::BULLET * 0.325;
    /// The share of a kick the view punch takes besides.
    pub const VIEW_PUNCH: f32 = 0.055;

    /// A shot's kick (`angle` 0 straight up, to the right as it grows); the view punch it adds back.
    pub fn kick(&mut self, angle: f32, magnitude: f32) -> [f32; 2] {
        let (s, c) = angle.to_radians().sin_cos();
        let k = [c * magnitude, s * magnitude];
        self.velocity[0] -= k[0];
        self.velocity[1] -= k[1];
        [-k[0] * Self::VIEW_PUNCH, -k[1] * Self::VIEW_PUNCH]
    }

    pub fn update(&mut self, dt: f32) {
        let v = (-4.5 * dt).exp();
        self.velocity = self.velocity.map(|x| x * v);
        let e = (-8.0 * dt).exp();
        let p = self.punch.map(|x| x * e);
        let len = (p[0] * p[0] + p[1] * p[1]).sqrt();
        let lin = 18.0 * dt;
        let keep = if len > lin { 1.0 - lin / len } else { 0.0 };
        self.punch = [p[0] * keep + self.velocity[0] * dt, p[1] * keep + self.velocity[1] * dt];
    }
}

/// What the player is doing, as the inaccuracy needs it: speed along the ground and up (inches a second), on the
/// ground, crouching, walking (the shift's slow walk: none here, the game has none).
#[derive(Clone, Copy, Debug, Default)]
pub struct Moving {
    pub speed: f32,
    pub vertical: f32,
    pub on_ground: bool,
    pub crouching: bool,
}

pub struct Rifle {
    pub w: &'static Weapon,
    kicks: [(f32, f32); 2],
    /// 0 none, 1 or 2 the zoom level.
    pub zoom: u8,
    /// How far the field of view is toward the zoom's (0..1; the zoom time is 0.05 s).
    zoom_t: f32,
    zoom_from: f32,
    pub aim: AimPunch,
    /// The view's own punch (degrees), dying away at 18 a second.
    pub view_punch: [f32; 2],
    /// The inaccuracy from shots (m_fAccuracyPenalty), back to none over the recovery time.
    penalty: f32,
    /// Seconds till the bolt is back (the cycle time from a shot).
    pub cycle_left: f32,
    pub moving: Moving,
}

fn kicks_of(w: &'static Weapon) -> [(f32, f32); 2] {
    static AWP_K: OnceLock<[(f32, f32); 2]> = OnceLock::new();
    static SSG_K: OnceLock<[(f32, f32); 2]> = OnceLock::new();
    if std::ptr::eq(w, &AWP) {
        *AWP_K.get_or_init(|| first_kicks(&AWP))
    } else {
        *SSG_K.get_or_init(|| first_kicks(&SSG08))
    }
}

/// Source's jump: its speed up (inches a second), the air's inaccuracy scales by its root.
const JUMP_IMPULSE: f32 = 301.993;

impl Rifle {
    pub fn new(w: &'static Weapon) -> Rifle {
        Rifle { w, kicks: kicks_of(w), zoom: 0, zoom_t: 1.0, zoom_from: 0.0, aim: AimPunch::default(), view_punch: [0.0; 2], penalty: 0.0, cycle_left: 0.0, moving: Moving::default() }
    }

    fn mode(&self) -> usize {
        (self.zoom > 0) as usize
    }

    /// Whether the bolt is back (a shot, the scope may be had again).
    pub fn ready(&self) -> bool {
        self.cycle_left <= 0.0
    }

    /// Into the scope (the bow drawn) or another level of it; `None` out of it. Its sound, when it changed.
    pub fn scope(&mut self, level: Option<u8>, current_fov: f32) -> Option<&'static str> {
        let want = level.unwrap_or(0).min(2);
        if want == self.zoom || (want > 0 && !self.ready()) {
            return None;
        }
        self.zoom_from = current_fov;
        self.zoom_t = 0.0;
        let into = want > self.zoom || want > 0;
        self.zoom = want;
        Some(if into && want > 0 { self.w.sound_zoom } else { self.w.sound_unzoom })
    }

    /// A shot (the bow let go): the kick, the inaccuracy, the bolt; out of the scope (m_bUnzoomsAfterShot). Its sound.
    pub fn shoot(&mut self, current_fov: f32) -> &'static str {
        let (angle, magnitude) = self.kicks[self.mode()];
        let vp = self.aim.kick(angle, magnitude);
        self.view_punch[0] += vp[0];
        self.view_punch[1] += vp[1];
        self.penalty += self.w.inaccuracy_fire[self.mode()];
        self.cycle_left = self.w.cycle;
        if self.zoom > 0 {
            self.zoom_from = current_fov;
            self.zoom_t = 0.0;
            self.zoom = 0;
        }
        self.w.sound_shot
    }

    pub fn update(&mut self, dt: f32) {
        self.aim.update(dt);
        let d = (-18.0 * dt).exp();
        self.view_punch = self.view_punch.map(|x| x * d);
        // the shots' inaccuracy falls to a tenth each recovery time
        let recovery = if self.moving.crouching { self.w.recovery_crouch } else { self.w.recovery_stand };
        self.penalty *= 0.1f32.powf(dt / recovery.max(1e-3));
        self.cycle_left = (self.cycle_left - dt).max(0.0);
        self.zoom_t = (self.zoom_t + dt / 0.05).min(1.0);
    }

    /// The inaccuracy now (CS's GetInaccuracy): standing or crouching, the shots', moving (past a third of the weapon's
    /// speed, all of it by its 0.95, to the quarter power), in the air (by the root of the speed up).
    pub fn inaccuracy(&self) -> f32 {
        let m = self.mode();
        let w = self.w;
        let mut a = if self.moving.crouching { w.inaccuracy_crouch[m] } else { w.inaccuracy_stand[m] } + self.penalty;
        let max = w.max_speed[m];
        let lo = max * 0.34;
        let hi = max * 0.95;
        let scale = ((self.moving.speed - lo) / (hi - lo)).clamp(0.0, 1.0);
        if scale > 0.0 {
            a += scale.powf(0.25) * w.inaccuracy_move[m];
        }
        if !self.moving.on_ground {
            let root_max = JUMP_IMPULSE.sqrt();
            let root = self.moving.vertical.abs().sqrt();
            let air = ((root - root_max * 0.25) / (root_max * 0.75) * w.inaccuracy_jump_initial).clamp(0.0, 2.0 * w.inaccuracy_jump_initial);
            a += air;
        }
        a.min(1.0)
    }

    /// The field of view (vertical, degrees) for the game's own: the zoom level's (CS's across 4:3 made vertical),
    /// eased over the zoom time.
    pub fn fov(&self, base: f32) -> f32 {
        let target = match self.zoom {
            0 => base,
            z => 2.0 * ((self.w.zoom_fov[z as usize - 1] * 0.5).to_radians().tan() * 0.75).atan().to_degrees(),
        };
        let from = if self.zoom_from > 0.0 { self.zoom_from } else { base };
        from + (target - from) * self.zoom_t
    }

    /// The camera's turn from the kick (degrees, the game's: pitch down positive, yaw right positive).
    pub fn camera(&self) -> [f32; 2] {
        let p = self.aim.punch[0] * AimPunch::VIEW + self.view_punch[0];
        let y = self.aim.punch[1] * AimPunch::VIEW + self.view_punch[1];
        [p, -y]
    }

    /// The weapon's own turn past the camera's (degrees, the game's way).
    pub fn weapon(&self) -> [f32; 2] {
        let k = AimPunch::VIEWMODEL - AimPunch::VIEW;
        [self.aim.punch[0] * k, -self.aim.punch[1] * k]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valves_random_stream_is_its_own() {
        // VRF's test vectors (UniformRandomStreamTest)
        for (seed, bits) in [(38965, [0x3ED2ED85u32, 0x3F5EC3F3, 0x3F1090EB, 0x3DCA36B1]), (-7, [0x3DFB91B4, 0x3EDB606B, 0x3F24CD14, 0x3EEF9005]), (0, [0x3ED4FDDE, 0x3DBC5817, 0x3F41A41E, 0x3F079A6F])] {
            let mut r = UniformRandom::new(seed);
            for b in bits {
                assert_eq!(r.float(0.0, 1.0).to_bits(), b, "seed {seed}");
            }
        }
    }

    #[test]
    fn a_shot_kicks_the_view_up_and_it_settles() {
        let mut r = Rifle::new(&AWP);
        let (angle, mag) = r.kicks[0];
        assert!((-20.0..=20.0).contains(&angle) && (63.0..=93.0).contains(&mag), "{angle} {mag}");
        assert!(r.ready());
        r.shoot(70.0);
        assert!(!r.ready());
        // a frame on: the view is turned up (the game's pitch up is negative)
        let mut most = 0.0f32;
        for _ in 0..30 {
            r.update(1.0 / 120.0);
            most = most.min(r.camera()[0]);
        }
        assert!(most < -1.0, "{most}");
        for _ in 0..240 {
            r.update(1.0 / 120.0);
        }
        // two seconds on: back, the bolt too
        assert!(r.camera()[0].abs() < 0.05 && r.ready(), "{:?}", r.camera());
    }

    #[test]
    fn the_scope_narrows_the_view_and_a_shot_leaves_it() {
        let mut r = Rifle::new(&AWP);
        assert!(r.scope(Some(1), 70.0).is_some());
        r.update(0.1);
        // 40 across 4:3: about 30.7 up and down
        assert!((r.fov(70.0) - 30.7).abs() < 0.2, "{}", r.fov(70.0));
        r.shoot(r.fov(70.0));
        assert_eq!(r.zoom, 0);
        // not again till the bolt is back
        assert!(r.scope(Some(1), 70.0).is_none());
        // scoped and still: the lines sharp; running: blurred
        let mut r = Rifle::new(&AWP);
        r.scope(Some(1), 70.0);
        assert!(r.inaccuracy() < 0.01);
        r.moving = Moving { speed: 170.0, vertical: 0.0, on_ground: true, crouching: false };
        assert!(r.inaccuracy() > 0.15);
    }
}
