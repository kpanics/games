//! Free Look (her word, 25.10: "очень красиво и кастомизированно"): on its key the view goes round the player while
//! they keep going where they went — the mouse turns the camera, not them. The camera eases after the mouse (as smooth
//! as set), flies out of the head to its distance and back in, the wheel brings it nearer or farther; let go, it turns
//! back to where the player looks (or the player turns to where it looked). A ring round the crosshair shows where
//! the player goes while the camera looks elsewhere.
//!
//! Java (cursed.recode.fx.FreeLook) asks each frame ("fl\tframe\tdt\tyaw\tpitch\tscreen\tfirst") and puts the camera
//! where it is told; the mouse's turns and the wheel come here while it looks ("fl\tturn\tdx\tdy", "fl\tscroll\tdy").

use super::draw::{grey, Canvas};
use super::theme::{self, fade, pal};
use super::Gui;

#[derive(Clone, Copy, PartialEq, Debug, Default)]
enum Phase {
    #[default]
    Off,
    /// looking round (the key held, or pressed once with Toggle)
    Look,
    /// let go: easing back
    Back,
}

/// Free Look as it is: what it is doing, where the mouse has put the camera and where the camera is (eased after it),
/// how far it is from the head now and is going to be, where the player looked as it began, what Java is to do once.
#[derive(Default)]
pub(super) struct Fl {
    phase: Phase,
    want: [f32; 2],
    cam: [f32; 2],
    dist: f32,
    dist_to: f32,
    dist_velocity: f32,
    configured_dist: f32,
    height: f32,
    height_velocity: f32,
    /// the game's view was the first person as it began: the camera flies out of the head and back in
    from_head: bool,
    /// the player's look this frame
    player: [f32; 2],
    /// "Look there": the player turned to where the camera looked, once (Java does it)
    turn_to: Option<[f32; 2]>,
    /// how much of the ring round the crosshair is seen
    pub shown: f32,
}

const MOD: &str = "free-look";
/// How near the head the camera flies out from and back to (nearer, the head is seen from inside: there the view is the
/// eyes' again).
const HEAD: f32 = 1.0;

/// Exact critically damped spring: the same flight at different frame rates.
fn spring(value: &mut f32, velocity: &mut f32, target: f32, dt: f32) {
    let delta = *value - target;
    let step = (*velocity + 18.0 * delta) * dt;
    let decay = (-18.0 * dt).exp();
    *value = target + (delta + step) * decay;
    *velocity = (*velocity - 18.0 * step) * decay;
}

/// Java supplies the eight world-ray distances; -1 means that ray missed.
fn clearance(distance: f32, hits: &[f32]) -> f32 {
    if !distance.is_finite() || distance <= 0.0001 || hits.len() != 8 {
        return 0.0;
    }
    let mut allowed = distance;
    for &hit in hits {
        if !hit.is_finite() || (hit < 0.0 && hit != -1.0) {
            return 0.0;
        }
        if hit >= 0.0 {
            allowed = allowed.min((hit - 0.05).max(0.0));
        }
    }
    (allowed / distance).clamp(0.0, 1.0)
}

fn distance(set: &[super::data::Setting]) -> f32 {
    let value = theme::num(set, "Distance");
    if value.is_finite() {
        value.clamp(1.0, 12.0)
    } else {
        4.0
    }
}

fn wrap(a: f32) -> f32 {
    let mut a = (a + 180.0) % 360.0;
    if a < 0.0 {
        a += 360.0;
    }
    a - 180.0
}

impl Gui {
    fn fl_set(&self) -> Option<Vec<super::data::Setting>> {
        self.mods.iter().find(|m| m.id == MOD && m.on).map(|m| m.set.clone())
    }

    fn fl_choice(set: &[super::data::Setting], label: &str) -> &'static str {
        match theme::find(set, label) {
            Some(super::data::Kind::Choice { v, opts }) => opts.get(*v).copied().unwrap_or(""),
            _ => "",
        }
    }

    /// Its key pressed (bind_fire): Hold looks while held; Toggle looks, pressed again turns back.
    pub(super) fn fl_press(&mut self) {
        let Some(set) = self.fl_set() else { return };
        if self.hidden_here(MOD) {
            return;
        }
        let toggle = Gui::fl_choice(&set, "Key") == "Toggle";
        match self.fl.phase {
            Phase::Look if toggle => self.fl_release(),
            Phase::Look => {}
            _ => self.fl_begin(&set),
        }
    }

    /// The keys let go (world_key): held, it turns back once none of its keys is still down.
    pub(super) fn fl_keys_up(&mut self, held: &[String]) {
        if self.fl.phase != Phase::Look {
            return;
        }
        let Some(set) = self.fl_set() else { return };
        if Gui::fl_choice(&set, "Key") == "Toggle" {
            return;
        }
        let still = self
            .mods
            .iter()
            .find(|m| m.id == MOD)
            .is_some_and(|m| m.binds.iter().any(|b| !b.keys.is_empty() && b.keys.iter().all(|k| held.contains(k))));
        if !still {
            self.fl_release();
        }
    }

    fn fl_begin(&mut self, set: &[super::data::Setting]) {
        let p = self.fl.player;
        // (from where it is, when it was still turning back)
        let from_back = self.fl.phase == Phase::Back;
        self.fl.phase = Phase::Look;
        if !from_back {
            self.fl.want = p;
            self.fl.cam = p;
            self.fl.dist = if self.fl.from_head {
                if theme::on(set, "Fly out of the head") {
                    HEAD
                } else {
                    distance(set)
                }
            } else {
                4.0
            };
            self.fl.dist_velocity = 0.0;
            self.fl.height = 0.0;
            self.fl.height_velocity = 0.0;
        } else {
            self.fl.want = self.fl.cam;
        }
        self.fl.dist_to = distance(set);
        self.fl.configured_dist = self.fl.dist_to;
    }

    fn fl_release(&mut self) {
        let set = self.fl_set().unwrap_or_default();
        if Gui::fl_choice(&set, "On release") == "Look there" {
            self.fl.turn_to = Some([self.fl.cam[0], self.fl.cam[1]]);
            self.fl.phase = Phase::Off;
            return;
        }
        // turned back the short way
        self.fl.cam[0] = self.fl.player[0] + wrap(self.fl.cam[0] - self.fl.player[0]);
        self.fl.want = self.fl.cam;
        self.fl.phase = if theme::on(&set, "Smooth return") { Phase::Back } else { Phase::Off };
    }

    /// Java's verb: "frame" each frame (the answer is where the camera goes), "turn" the mouse's turns, "scroll" the
    /// wheel (1: taken).
    pub(super) fn fl_call(&mut self, verb: &str, rest: &str) -> String {
        let n: Vec<f32> = rest.split('\t').filter_map(|v| v.parse().ok()).collect();
        let g = |i: usize| n.get(i).copied().unwrap_or(0.0);
        match verb {
            "reset" => {
                self.fl = Fl::default();
                String::new()
            }
            "clip" => clearance(g(0), n.get(1..).unwrap_or_default()).to_string(),
            "turn" => {
                if self.fl.phase != Phase::Look {
                    return "0".into();
                }
                let set = self.fl_set().unwrap_or_default();
                let k = 0.15 * theme::num(&set, "Sensitivity").max(0.05);
                let inv = if theme::on(&set, "Invert up and down") { -1.0 } else { 1.0 };
                self.fl.want[0] += g(0) * k;
                self.fl.want[1] = (self.fl.want[1] + g(1) * k * inv).clamp(-90.0, 90.0);
                "1".into()
            }
            "scroll" => {
                let set = self.fl_set().unwrap_or_default();
                if self.fl.phase != Phase::Look || !theme::on(&set, "Wheel changes the distance") {
                    return "0".into();
                }
                // each step of the wheel an eighth nearer or farther
                self.fl.dist_to = (self.fl.dist_to * 1.125f32.powf(-g(0))).clamp(1.0, 12.0);
                "1".into()
            }
            "frame" => self.fl_frame(g(0), [g(1), g(2)], g(3) > 0.5, g(4) > 0.5),
            _ => String::new(),
        }
    }

    /// One frame: the camera eased after the mouse (or back to the player's look), its distance; "cam yaw pitch
    /// distance view height" (view: 0 as the game's, 1 behind, 2 in front, 3 the first person), "turn yaw pitch" once, or
    /// nothing at all while it is off.
    fn fl_frame(&mut self, dt: f32, player: [f32; 2], screen: bool, first: bool) -> String {
        let dt = if dt.is_finite() { dt.clamp(0.0, 0.1) } else { 0.0 };
        self.fl.player = player;
        let mut out = Vec::new();
        if let Some(t) = self.fl.turn_to.take() {
            out.push(format!("turn {} {}", t[0], t[1]));
        }
        let set = self.fl_set();
        // off (or switched off, or banned here): at once as it was
        let Some(set) = set.filter(|_| !self.hidden_here(MOD)) else {
            self.fl.phase = Phase::Off;
            self.fl.from_head = first;
            return out.join("\n");
        };
        if self.fl.phase == Phase::Off {
            self.fl.from_head = first;
            return out.join("\n");
        }
        // a menu over the world while held: its key's letting go is the menu's, so it turns back now
        if screen && self.fl.phase == Phase::Look && Gui::fl_choice(&set, "Key") == "Hold" {
            self.fl_release();
        }
        let back = self.fl.phase == Phase::Back;
        let configured = distance(&set);
        if configured != self.fl.configured_dist {
            self.fl.configured_dist = configured;
            self.fl.dist_to = configured;
        }
        // where it goes: the mouse's, or the player's look (the short way round)
        let to = if back {
            [self.fl.cam[0] + wrap(player[0] - self.fl.cam[0]), player[1]]
        } else {
            self.fl.want
        };
        let smooth = theme::num(&set, "Smoothness") / 100.0;
        let speed = if back {
            7.0 * theme::num(&set, "Return speed").max(0.1)
        } else {
            60.0 * (1.0 - smooth) + 7.0 * smooth
        };
        let k = 1.0 - (-dt * speed).exp();
        self.fl.cam[0] += (to[0] - self.fl.cam[0]) * k;
        self.fl.cam[1] += (to[1] - self.fl.cam[1]) * k;
        // the distance: out of the head (from 0) and back in as it returns, the wheel's own
        let fly = self.fl.from_head && theme::on(&set, "Fly out of the head");
        // (back into the head only once it is behind it again: turned first, then in — never past the face)
        let dist_to = if back && fly {
            let left = wrap(to[0] - self.fl.cam[0]).abs().max((to[1] - self.fl.cam[1]).abs());
            let k = ((left - 6.0) / 24.0).clamp(0.0, 1.0);
            HEAD - 0.2 + (self.fl.dist_to - HEAD + 0.2) * k
        } else if back && !self.fl.from_head {
            4.0
        } else {
            self.fl.dist_to
        };
        spring(&mut self.fl.dist, &mut self.fl.dist_velocity, dist_to, dt);
        let view_name = Gui::fl_choice(&set, "View");
        let height = theme::num(&set, "Height");
        let height_to = if back || view_name == "First person" || (view_name == "As it is" && self.fl.from_head) || !height.is_finite() {
            0.0
        } else {
            height.clamp(-3.0, 3.0)
        };
        spring(&mut self.fl.height, &mut self.fl.height_velocity, height_to, dt);
        if back {
            let near = wrap(to[0] - self.fl.cam[0]).abs() < 0.4
                && (to[1] - self.fl.cam[1]).abs() < 0.4
                && (!fly || self.fl.dist < HEAD + 0.1)
                && (self.fl.from_head || (self.fl.dist - 4.0).abs() < 0.05)
                && self.fl.height.abs() < 0.015;
            if near {
                self.fl.phase = Phase::Off;
                return out.join("\n");
            }
        }
        let view = match view_name {
            "Behind" => 1,
            "In front" => 2,
            "First person" => 3,
            _ => 0,
        };
        out.push(format!(
            "cam {} {} {} {} {}",
            self.fl.cam[0],
            self.fl.cam[1],
            self.fl.dist.max(0.05),
            view,
            self.fl.height
        ));
        out.join("\n")
    }

    /// The ring round the crosshair (the HUD's frame): where the player goes, while the camera looks elsewhere.
    pub(super) fn fl_draw(&mut self, c: &mut Canvas, w: f32, h: f32) {
        let on = self.fl.phase != Phase::Off && self.fl_set().is_some_and(|s| theme::on(&s, "Indicator"));
        self.fl.shown += ((if on { 1.0 } else { 0.0 }) - self.fl.shown) * (1.0 - (-self.dt * 10.0).exp());
        if self.fl.shown < 0.01 {
            return;
        }
        let a = self.fl.shown;
        let r = self.r;
        let cen = [w / 2.0, h / 2.0];
        let rad = 2.2 * r + (1.0 - a) * 0.8 * r;
        // (a dark edge under it: seen over snow and sky as over the night)
        c.ring(cen, rad, 3.5, grey(0.0, 0.16 * a));
        c.ring(cen, rad, 1.5, grey(1.0, 0.22 * a));
        // the way the player goes, from where the camera looks: up is ahead of it
        let off = wrap(self.fl.player[0] - self.fl.cam[0]).to_radians();
        let span = 0.36;
        let pts: Vec<[f32; 2]> = (0..=12)
            .map(|i| {
                let t = off - span / 2.0 + span * i as f32 / 12.0;
                [cen[0] + rad * t.sin(), cen[1] - rad * t.cos()]
            })
            .collect();
        c.polyline(&pts, 5.0, grey(0.0, 0.28 * a));
        c.polyline(&pts, 2.5, fade(pal().accent, 0.95 * a));
        let tip = [cen[0] + (rad + 0.55 * r) * off.sin(), cen[1] - (rad + 0.55 * r) * off.cos()];
        let d = 0.2 * r;
        c.rect(
            [tip[0] - d - 1.5, tip[1] - d - 1.5, tip[0] + d + 1.5, tip[1] + d + 1.5],
            d + 1.5,
            grey(0.0, 0.3 * a),
        );
        c.rect([tip[0] - d, tip[1] - d, tip[0] + d, tip[1] + d], d, fade(pal().t1, a));
    }
}

#[cfg(test)]
mod tests {
    use super::super::data::Kind;
    use super::*;

    fn put(g: &mut Gui, label: &str, v: &str) {
        let i = g.mod_index(MOD);
        for s in g.mods[i].set.iter_mut().filter(|s| s.label == label) {
            match &mut s.kind {
                Kind::Choice { v: x, opts } => *x = opts.iter().position(|o| *o == v).unwrap(),
                Kind::Toggle(t) => *t = v == "1",
                Kind::Slider { v: x, .. } => *x = v.parse().unwrap(),
                _ => {}
            }
        }
    }

    fn cam(s: &str) -> Vec<f32> {
        s.lines()
            .find(|l| l.starts_with("cam "))
            .map(|l| l[4..].split(' ').map(|v| v.parse().unwrap()).collect())
            .unwrap_or_default()
    }

    #[test]
    fn spring_and_clearance_keep_motion_smooth_and_inside_the_nearest_hit() {
        let (mut a, mut av, mut b, mut bv) = (0.0, 0.0, 0.0, 0.0);
        spring(&mut a, &mut av, 2.0, 0.1);
        for _ in 0..10 {
            spring(&mut b, &mut bv, 2.0, 0.01);
        }
        assert!((a - b).abs() < 0.0001 && (av - bv).abs() < 0.0001);
        assert!(a > 0.0 && a < 2.0);
        assert_eq!(clearance(4.0, &[-1.0; 8]), 1.0);
        assert!((clearance(4.0, &[-1.0, 3.0, -1.0, 2.0, -1.0, -1.0, -1.0, -1.0]) - 0.4875).abs() < 0.0001);
        assert_eq!(clearance(4.0, &[0.02; 8]), 0.0);
        assert_eq!(clearance(f32::NAN, &[-1.0; 8]), 0.0);
        assert_eq!(clearance(4.0, &[f32::NAN; 8]), 0.0);
        assert_eq!(clearance(4.0, &[-1.0; 7]), 0.0);
    }

    #[test]
    fn height_and_live_distance_ease_out_and_return_to_the_original_camera() {
        let mut g = Gui::new();
        let i = g.mod_index(MOD);
        g.mods[i].on = true;
        put(&mut g, "Key", "Toggle");
        put(&mut g, "Height", "2");
        put(&mut g, "Distance", "8");
        g.fl_call("frame", "0.016\t0\t0\t0\t0");
        g.fl_press();
        let first = cam(&g.fl_call("frame", "0.016\t0\t0\t0\t0"));
        assert!(first[4] > 0.0 && first[4] < 2.0 && first[2] > 4.0 && first[2] < 8.0);
        let mut last = Vec::new();
        for _ in 0..100 {
            last = cam(&g.fl_call("frame", "0.016\t0\t0\t0\t0"));
        }
        assert!((last[4] - 2.0).abs() < 0.001 && (last[2] - 8.0).abs() < 0.001);
        put(&mut g, "Height", "-1");
        put(&mut g, "Distance", "6");
        for _ in 0..100 {
            last = cam(&g.fl_call("frame", "0.016\t0\t0\t0\t0"));
        }
        assert!((last[4] + 1.0).abs() < 0.001 && (last[2] - 6.0).abs() < 0.001);
        let saved = g.snapshot("camera", super::super::configs::P_MODULES);
        let (values, _, _) = super::super::configs::parse(&saved);
        let mut restored = Gui::new();
        restored.apply_vals(&values, super::super::configs::P_MODULES);
        assert_eq!(theme::num(&restored.mods[i].set, "Height"), -1.0);
        g.fl_press();
        let mut returned = false;
        for _ in 0..200 {
            let next = cam(&g.fl_call("frame", "0.016\t0\t0\t0\t0"));
            if next.is_empty() {
                returned = true;
                break;
            }
            last = next;
        }
        assert!(returned && last[4].abs() < 0.02 && (last[2] - 4.0).abs() < 0.06, "{last:?}");
    }

    #[test]
    fn first_person_and_leaving_the_world_never_keep_a_height_offset() {
        let mut g = Gui::new();
        let i = g.mod_index(MOD);
        g.mods[i].on = true;
        put(&mut g, "Height", "2");
        for view in ["First person", "As it is"] {
            put(&mut g, "View", view);
            g.fl_call("frame", "0.016\t0\t0\t0\t1");
            g.fl_press();
            for _ in 0..30 {
                assert_eq!(cam(&g.fl_call("frame", "0.016\t0\t0\t0\t1"))[4], 0.0);
            }
            g.fl_call("reset", "");
            assert!(g.fl_call("frame", "0.016\t0\t0\t0\t1").is_empty());
        }
        assert_eq!(g.fl_call("clip", "4\t-1\t-1\t-1\t2\t-1\t-1\t-1\t-1"), "0.4875");
    }

    #[test]
    fn the_camera_goes_round_and_comes_back() {
        let mut g = Gui::new();
        let i = g.mod_index(MOD);
        // off: nothing, its key nothing
        g.fl_press();
        assert_eq!(g.fl_call("frame", "0.016\t10\t5\t0\t1"), "");
        g.mods[i].on = true;
        put(&mut g, "Smoothness", "0");
        assert_eq!(g.fl_call("frame", "0.016\t10\t5\t0\t1"), "");
        // pressed: the mouse is the camera's, the player stays
        g.fl_press();
        assert_eq!(g.fl_call("turn", "600\t0"), "1");
        let mut last = Vec::new();
        for _ in 0..60 {
            last = cam(&g.fl_call("frame", "0.016\t10\t5\t0\t1"));
        }
        assert!((last[0] - 100.0).abs() < 0.5, "{last:?}");
        // out of the head to its distance, behind
        assert!((last[2] - 4.0).abs() < 0.05 && last[3] == 1.0, "{last:?}");
        // let go (Hold): back the short way, then nothing
        g.fl_keys_up(&[]);
        let mut n = 0;
        while !g.fl_call("frame", "0.016\t10\t5\t0\t1").is_empty() {
            n += 1;
            assert!(n < 400);
        }
        assert_eq!(g.fl_call("turn", "10\t0"), "0");
    }

    #[test]
    fn toggled_it_stays_and_looking_there_turns_the_player() {
        let mut g = Gui::new();
        let i = g.mod_index(MOD);
        g.mods[i].on = true;
        put(&mut g, "Key", "Toggle");
        put(&mut g, "On release", "Look there");
        put(&mut g, "Smoothness", "0");
        g.fl_call("frame", "0.016\t0\t0\t0\t0");
        g.fl_press();
        g.fl_call("turn", "0\t200");
        for _ in 0..60 {
            g.fl_call("frame", "0.016\t0\t0\t0\t0");
        }
        // keys let go: still looking
        g.fl_keys_up(&[]);
        assert!(!cam(&g.fl_call("frame", "0.016\t0\t0\t0\t0")).is_empty());
        // pressed again: the player is turned to where it looked, and it is off
        g.fl_press();
        let s = g.fl_call("frame", "0.016\t0\t0\t0\t0");
        let t: Vec<f32> = s.strip_prefix("turn ").unwrap_or("").split(' ').filter_map(|v| v.parse().ok()).collect();
        assert!(t.len() == 2 && t[0].abs() < 0.01 && (t[1] - 30.0).abs() < 0.01, "{s}");
        assert!(cam(&s).is_empty());
        // the wheel only while it looks
        assert_eq!(g.fl_call("scroll", "1"), "0");
    }
}
