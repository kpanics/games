//! CS2's viewmodel, as the game has it (VRF's ViewmodelSceneNode, MIT, is the reference: its states, its timings, its
//! sway and bob): the agent's first-person arms and the item in them, posed by the item's clips (the arms' skeleton
//! "characters/viewmodel" with the item's own beside it, tools/cs2pack/clips.py). The arms take the clips' places by
//! joint name (Model::world_from); the item sits on the "wpn" joint and its own joints take theirs.
//!
//! States: Draw (taken out: a second before it can be used), Idle (looping, faded into over 0.35 s), LookAt (the
//! inspect key; another only from half way through the one playing), Attack (a knife's light swing: 0.4 s before the
//! next, 0.1 s more when it hit), AltAttack (its heavy one: 1 s), Shoot (a rifle's shot, its bolt in the clip). A clip
//! over goes back to Idle.

use super::model::{mul, Model, Trs, IDENTITY, M4};
use std::time::Instant;

/// What an item is in the hands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Knife,
    Rifle,
}

/// The files of what the hands hold (keys under the store's folder).
#[derive(Clone, Debug, PartialEq)]
pub struct Kit {
    /// The side's arms (its default gloves on), or its sleeves alone when gloves are on.
    pub arms: String,
    /// The gloves on (their model for the side: the gloves and the forearm under the sleeves), and their paint (paint,
    /// seed, wear) when they have one.
    pub gloves: Option<String>,
    pub gloves_finish: Option<(String, i32, f32)>,
    pub item: String,
    pub clips: String,
    pub kind: Kind,
    /// A knife's deploy duration; a rifle's, and its cycle (the time between shots, m_flCycleTime).
    pub deploy: f32,
    pub cycle: f32,
    /// Its finish (paint, seed, wear), when it has one.
    pub finish: Option<(String, i32, f32)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Idle,
    Draw,
    LookAt,
    Attack,
    AltAttack,
    Shoot,
}

/// What the player did with the hands this frame (cosmetics/view.rs Event, as the hands take it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Swing { hit: bool },
    Heavy { hit: bool },
    Shoot,
    Inspect,
}

/// The sway (cl_wpn_sway_interp, cl_wpn_sway_scale): past yaws to sample the view a moment ago.
struct Lag {
    history: Vec<(f32, f32)>,
}

impl Lag {
    const INTERP: f32 = 0.1;
    const SCALE: f32 = 0.32;

    fn calc(&mut self, yaw: f32, now: f32) -> [f32; 3] {
        self.history.push((now, yaw));
        let keep = self.history.iter().position(|&(t, _)| t >= now - 1.0).unwrap_or(0);
        if keep > 0 {
            self.history.drain(..keep);
        }
        let at = now - Self::INTERP;
        let mut past = self.history[0].1;
        for w in self.history.windows(2) {
            let ((t0, y0), (t1, y1)) = (w[0], w[1]);
            if t0 <= at && at <= t1 {
                let f = if t1 > t0 { (at - t0) / (t1 - t0) } else { 0.0 };
                let mut d = y1 - y0;
                d = (d + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
                past = y0 + d * f;
                break;
            }
            if t1 <= at {
                past = y1;
            }
        }
        let mut d = yaw - past;
        d = (d + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        // (forward, left, up), as VRF's: forward·(1 − cos), left·(−sin)
        [(1.0 - d.cos()) * Self::SCALE, -d.sin() * Self::SCALE, 0.0]
    }
}

pub struct Hands {
    pub kit: Kit,
    pub state: State,
    clip: Option<String>,
    since: Instant,
    /// How far into the clip its events were last taken (s; below 0 none yet), and those passed since, for the frame.
    heard: f32,
    passed: Vec<(u8, String)>,
    /// The pose the last clip left (blended out of over the new one's fade).
    from: Option<Vec<Trs>>,
    fade: f32,
    deploy_left: f32,
    attack: f32,
    alt: f32,
    last: Instant,
    lag: Lag,
    bob: [f32; 3],
    start: Instant,
    variant: usize,
}

/// Source's inch in our metres.
pub const INCH: f32 = 0.0254;
/// VRF's turn at a glTF skeleton's roots, undone (the item's own space onto the arms' "wpn").
const C_INV: [f32; 4] = [0.5, 0.5, 0.5, 0.5];

fn quat_matrix(q: [f32; 4]) -> M4 {
    Trs { t: [0.0; 3], r: q, s: [1.0; 3] }.matrix()
}

impl Hands {
    pub fn new(kit: Kit) -> Hands {
        let now = Instant::now();
        let deploy = kit.deploy;
        Hands {
            kit,
            state: State::Draw,
            clip: None,
            since: now,
            heard: -1.0,
            passed: Vec::new(),
            from: None,
            fade: 0.0,
            deploy_left: deploy,
            attack: 0.0,
            alt: 0.0,
            last: now,
            lag: Lag { history: Vec::new() },
            bob: [0.0; 3],
            start: now,
            variant: 0,
        }
    }

    /// The clip for a state, by the names CS gives them (one of several at random: idle1/idle2, light_miss1/2…).
    fn clip_for(&mut self, clips: &Model, s: State, hit: bool) -> Option<String> {
        let names: &[&str] = match (s, self.kit.kind, hit) {
            (State::Idle, _, _) => &["idle1", "idle"],
            (State::Draw, _, _) => &["draw"],
            (State::LookAt, _, _) => &["lookat01", "lookat"],
            (State::Attack, Kind::Knife, false) => &["light_miss1", "light_miss2"],
            (State::Attack, Kind::Knife, true) => &["light_hit1", "light_hit2", "light_miss1"],
            (State::AltAttack, Kind::Knife, false) => &["heavy_miss1"],
            (State::AltAttack, Kind::Knife, true) => &["heavy_hit1", "heavy_miss1"],
            (State::Shoot, _, _) | (State::Attack, Kind::Rifle, _) | (State::AltAttack, Kind::Rifle, _) => &["shoot1", "shoot"],
        };
        let found: Vec<&str> = names.iter().copied().filter(|n| clips.clips.iter().any(|c| c.name == *n)).collect();
        if found.is_empty() {
            return None;
        }
        // light swings alternate their two clips, as CS does
        self.variant = self.variant.wrapping_add(1);
        let pick = if found.len() > 1 && matches!(s, State::Attack) { found[self.variant % found.len()] } else { found[0] };
        Some(pick.to_string())
    }

    fn set(&mut self, clips: &Model, s: State, hit: bool, current: &[Trs]) {
        let Some(c) = self.clip_for(clips, s, hit) else { return };
        self.state = s;
        // Idle and LookAt fade in over 0.35 s; the rest cut in (VRF SetState)
        self.fade = if matches!(s, State::Idle | State::LookAt) { 0.35 } else { 0.0 };
        self.from = Some(current.to_vec());
        self.clip = Some(c);
        self.since = Instant::now();
        self.heard = -1.0;
    }

    /// The clip's events passed since last asked (a looping one's again each time round).
    fn pass_events(&mut self, clips: &Model, now: Instant) {
        let Some(c) = self.clip.as_ref().and_then(|n| clips.clip(n)) else { return };
        let len = c.length().max(1e-3);
        let t = now.duration_since(self.since).as_secs_f32();
        let (from, to) = (self.heard, t);
        for e in &c.events {
            let mut hit = e.time > from && e.time <= to;
            if self.state == State::Idle && !hit {
                // round again: the same time a loop later
                let k = ((to - e.time) / len).floor();
                hit = k >= 1.0 && e.time + k * len > from && e.time + k * len <= to;
            }
            if hit || (from < 0.0 && e.time <= 0.0 && to >= 0.0) {
                self.passed.push((e.kind, e.name.clone()));
            }
        }
        self.heard = to;
    }

    /// The clips' events passed (kind, name): sounds to play, particles to show.
    pub fn take_events(&mut self) -> Vec<(u8, String)> {
        std::mem::take(&mut self.passed)
    }

    fn deployed(&self, dt: f32) -> bool {
        self.deploy_left <= dt * 0.5
    }

    /// The pose this frame (of the clips' skeleton), the acts taken in; the clock moved on by `dt`.
    pub fn pose(&mut self, clips: &Model, acts: &[Act]) -> Vec<Trs> {
        let now = Instant::now();
        let dt = now.duration_since(self.last).as_secs_f32().min(0.1);
        self.last = now;
        self.deploy_left = (self.deploy_left - dt).max(0.0);
        self.attack = (self.attack - dt).max(0.0);
        self.alt = (self.alt - dt).max(0.0);
        let mut current = self.sample(clips, now);
        if self.clip.is_none() {
            self.set(clips, State::Draw, false, &current);
            current = self.sample(clips, now);
        }
        for &a in acts {
            match a {
                Act::Swing { hit } if self.kit.kind == Kind::Knife && self.deployed(dt) && self.attack <= 0.0 => {
                    self.set(clips, State::Attack, hit, &current);
                    self.attack = 0.4 + if hit { 0.1 } else { 0.0 };
                    self.alt = self.attack;
                }
                Act::Heavy { hit } if self.kit.kind == Kind::Knife && self.deployed(dt) && self.alt <= 0.0 => {
                    self.set(clips, State::AltAttack, hit, &current);
                    self.alt = 1.0 + if hit { 0.1 } else { 0.0 };
                    self.attack = self.alt;
                }
                Act::Shoot if self.kit.kind == Kind::Rifle => {
                    self.set(clips, State::Shoot, false, &current);
                    self.attack = self.kit.cycle;
                }
                Act::Inspect if self.can_inspect(clips, now) => self.set(clips, State::LookAt, false, &current),
                _ => {}
            }
        }
        self.pass_events(clips, now);
        // a clip over: back to idle
        if self.state != State::Idle {
            let over = self.clip.as_ref().and_then(|c| clips.clip(c)).is_none_or(|c| now.duration_since(self.since).as_secs_f32() >= c.length());
            if over {
                let cur = self.sample(clips, now);
                self.set(clips, State::Idle, false, &cur);
            }
        }
        self.sample(clips, now)
    }

    /// Whether an inspect may start: not while one plays, until half of it is over.
    fn can_inspect(&self, clips: &Model, now: Instant) -> bool {
        if self.state != State::LookAt {
            return !matches!(self.state, State::Draw);
        }
        let len = self.clip.as_ref().and_then(|c| clips.clip(c)).map_or(0.0, |c| c.length());
        len <= 0.0 || now.duration_since(self.since).as_secs_f32() >= len * 0.5
    }

    /// Seconds since taken out.
    pub fn age(&self) -> f32 {
        self.start.elapsed().as_secs_f32()
    }

    /// Whether the item may be used now (taken out, not cycling): a rifle's bolt, a bow's draw waits for it.
    pub fn ready(&self) -> bool {
        self.deploy_left <= 0.0 && self.attack <= 0.0
    }

    fn sample(&self, clips: &Model, now: Instant) -> Vec<Trs> {
        let mut pose = clips.rest();
        let Some(c) = self.clip.as_ref().and_then(|n| clips.clip(n)) else { return pose };
        let mut t = now.duration_since(self.since).as_secs_f32();
        if self.state == State::Idle {
            t %= c.length().max(1e-3);
        }
        c.sample_into(t, &mut pose);
        if let Some(from) = &self.from {
            let k = if self.fade > 0.0 { (now.duration_since(self.since).as_secs_f32() / self.fade).clamp(0.0, 1.0) } else { 1.0 };
            if k < 1.0 {
                for (p, f) in pose.iter_mut().zip(from) {
                    *p = Trs::lerp(f, p, k);
                }
            }
        }
        pose
    }

    /// The viewmodel's place in the view (our view space: x right, y up, −z ahead) for the clips' space (x left, y
    /// up, z ahead): turned half round y, moved by the offset, the sway and the bob — VRF's UpdateTransforms, in
    /// Source's inches and its viewmodel space (forward, left, up). `yaw` the view's (radians, Source's way: up as
    /// the view turns left), `vel` the player's velocity there (inches a second), `dt` the frame's seconds.
    pub fn place(&mut self, yaw: f32, vel: [f32; 3], on_ground: bool, offset: [f32; 3], dt: f32) -> M4 {
        let now = self.start.elapsed().as_secs_f32();
        let lag = self.lag.calc(yaw, now);
        // past 800 a second the bob grows slower and slower (surf speeds keep it on the screen)
        let speed = (vel[0] * vel[0] + vel[1] * vel[1] + vel[2] * vel[2]).sqrt();
        let (refer, over) = (800.0f32, 0.15 * 800.0);
        let bob_speed = if speed <= refer { speed } else { refer + over * (1.0 - (-(speed - refer) / over).exp()) };
        let k = if speed > 1e-4 { bob_speed / speed } else { 0.0 };
        let mut target = [vel[0] * k * 0.005, vel[1] * k * 0.005, vel[2] * k * 0.005];
        // sideways leads instead of trailing; up and down only lifts
        target[1] = -target[1] * 0.3;
        target[2] = target[2].abs() * 0.3;
        // half way there each of CS's ticks (64 a second)
        let f = 1.0 - 0.5f32.powf(dt * 64.0);
        for i in 0..3 {
            self.bob[i] += (target[i] - self.bob[i]) * f;
        }
        let amp = if on_ground { ((speed - 150.0) / 150.0).clamp(0.0, 1.0) * 0.1 } else { 0.0 };
        let w = (now * 18.0).sin() * amp;
        let walk = [w, 0.5 * w, w];
        let o: [f32; 3] = std::array::from_fn(|i| offset[i] - self.bob[i] - walk[i] + lag[i]);
        // the clips' space (x left, y up, z ahead) into the view's: half round y; the offset (forward, left, up) in it
        let mut m: M4 = [-1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
        m[12] = -o[1] * INCH;
        m[13] = o[2] * INCH;
        m[14] = -o[0] * INCH;
        m
    }

    /// The item's place for the arms' clip world: on the "wpn" joint, its own space turned back (C⁻¹).
    pub fn item_root(clips: &Model, world: &[M4]) -> M4 {
        match clips.joint("wpn").and_then(|k| world.get(k)) {
            Some(w) => mul(w, &quat_matrix(C_INV)),
            None => IDENTITY,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sway_trails_a_turn_and_settles() {
        let mut lag = Lag { history: Vec::new() };
        // still: nothing
        for k in 0..20 {
            let v = lag.calc(0.0, k as f32 * 0.01);
            assert!(v[0].abs() < 1e-6 && v[1].abs() < 1e-6);
        }
        // a quick turn: the gun trails sideways
        let v = lag.calc(0.3, 0.21);
        assert!(v[1].abs() > 0.05, "{v:?}");
        // held there past the window: back to nothing
        lag.calc(0.3, 0.3);
        let v = lag.calc(0.3, 0.45);
        assert!(v[1].abs() < 1e-4, "{v:?}");
    }
}
