//! Fake Player, "Двойник-фотостудия" (her choice, 25.10): doubles only the player sees — as many as they put, each
//! where they looked — standing and looking at them, as their mirror (every move of theirs, mirrored across the glass
//! between them), or going after them along their own path a moment later. Hit, a double flinches and is thrown back
//! as a player would be; the blows never leave the game. Their skin, armour and what they hold are the player's, or a
//! nick's.
//!
//! Java tells the player's state each tick ("fake\ttick\t…") and puts the doubles where they are said to be (cursed.
//! recode.fx.FakePlayers): "fake n x y z yaw pitch body sneak swing" a line each, "fakeclear" when none is to be.

use super::theme;
use super::Gui;
use std::collections::VecDeque;

pub(super) const MODULE: &str = "fake-player";
pub(super) const MODES: [&str; 3] = ["Stands", "Mirror", "Repeats you"];
/// The most doubles at once.
const MOST: usize = 8;
/// How far behind "Repeats you" goes (ticks).
const LAG: usize = 30;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct State {
    pos: [f64; 3],
    yaw: f32,
    pitch: f32,
    body: f32,
    sneak: bool,
    swing: bool,
}

#[derive(Clone, Debug, Default)]
struct Double {
    /// where it was put, and the glass between it and the player then (a point on it, its normal: the mirror's)
    at: [f64; 3],
    glass: ([f64; 3], [f64; 3]),
    /// thrown back: its speed, how far it is off where it was put
    vel: [f64; 3],
    off: [f64; 3],
    /// where it was last as it went after the player (it waits there when it would step into them)
    last: Option<[f64; 3]>,
    pose: Option<[f32; 18]>,
    pose_now: [f32; 18],
}

#[derive(Default)]
pub(super) struct Fakes {
    list: Vec<Double>,
    /// the player's last states (for "Repeats you")
    past: VecDeque<State>,
    /// put one where the player looks, at the next tick
    put: bool,
    clear: bool,
    was_on: bool,
    editing: Option<usize>,
    pub(super) preview: Option<[f32; 10]>,
}

fn wrap(a: f32) -> f32 {
    (a + 180.0).rem_euclid(360.0) - 180.0
}

/// The yaw (the game's: 0 south, 90 west) looking from `from` at `to`.
fn yaw_to(from: [f64; 3], to: [f64; 3]) -> f32 {
    let (dx, dz) = (to[0] - from[0], to[2] - from[2]);
    (-dx.atan2(dz)).to_degrees() as f32
}

/// A point mirrored across the glass (a point on it, its normal of length 1).
fn mirror(p: [f64; 3], glass: ([f64; 3], [f64; 3])) -> [f64; 3] {
    let (a, n) = glass;
    let d = (p[0] - a[0]) * n[0] + (p[2] - a[2]) * n[2];
    [p[0] - 2.0 * d * n[0], p[1], p[2] - 2.0 * d * n[2]]
}

/// A yaw mirrored across the glass: its direction reflected.
fn mirror_yaw(yaw: f32, n: [f64; 3]) -> f32 {
    let r = (yaw as f64).to_radians();
    let (x, z) = (-r.sin(), r.cos());
    let d = x * n[0] + z * n[2];
    let (mx, mz) = (x - 2.0 * d * n[0], z - 2.0 * d * n[2]);
    (-mx.atan2(mz)).to_degrees() as f32
}

pub(super) const POSES: [&str; 6] = ["Live", "Standing", "Sitting", "Greeting", "Guard", "Custom"];
pub(super) const POSE_AXES: [&str; 18] = [
    "Body pitch", "Body yaw", "Body roll", "Head pitch", "Head yaw", "Head roll",
    "Right arm pitch", "Right arm yaw", "Right arm roll", "Left arm pitch", "Left arm yaw", "Left arm roll",
    "Right leg pitch", "Right leg yaw", "Right leg roll", "Left leg pitch", "Left leg yaw", "Left leg roll",
];

fn studio_pose(set: &[super::data::Setting]) -> Option<[f32; 18]> {
    let preset = theme::pick(set, "Pose");
    if preset == 0 { return None; }
    let mut p = [0.0_f32; 18];
    match preset {
        2 => { p[12] = -90.0; p[14] = 12.0; p[15] = -90.0; p[17] = -12.0; }
        3 => { p[6] = -145.0; p[8] = -25.0; p[4] = -12.0; }
        4 => { p[6] = -75.0; p[7] = -25.0; p[9] = -65.0; p[10] = 30.0; p[0] = 6.0; }
        5 => for (v, label) in p.iter_mut().zip(POSE_AXES) {
            let n = theme::num(set, label);
            *v = if n.is_finite() { n.clamp(-180.0, 180.0) } else { 0.0 };
        },
        _ => {}
    }
    for v in &mut p { *v = v.to_radians(); }
    Some(p)
}

impl Gui {
    /// The studio shows the selected double itself, with its real skin, outfit and pose.
    /// Java only supplies the game's entity model, in the rectangle and clip chosen here.
    pub(super) fn fakes_preview(&mut self, c: &mut super::draw::Canvas, area: [f32; 4], module: usize, reveal: f32) {
        use super::draw::grey;
        use super::text::{fonts, Face};
        use super::theme::{fade, pal};
        use super::i18n::{tr, trn};
        let clip = c.clip_now();
        let r = self.r;
        let top = area[1].max(clip[1]);
        let bottom = area[3].min(clip[3]);
        let center = [(area[0] + area[2]) * 0.5, (top + bottom) * 0.5];
        let alpha = c.alpha * super::ease_out(reveal);
        c.rect(area, 0.75 * r, pal().f1);
        if self.fakes.list.is_empty() {
            let words = fonts().fit(Face::Regular, 0.8 * r, tr("Enable Fake Player to see your double"), area[2] - area[0] - 3.0 * r);
            c.text_c(center[0], center[1], 0.8 * r, Face::Regular, &words, pal().t3);
            return;
        }
        let n = (theme::num(&self.mods[module].set, "Edit double").max(1.0) as usize - 1).min(self.fakes.list.len() - 1);
        let size = (area[2] - area[0] - 3.0 * r).min((bottom - top - 5.0 * r).max(r)).min(29.0 * r);
        let x = center[0] - size * 0.5;
        let y = center[1] - size * 0.5 - 0.8 * r;
        c.shadow([center[0] - size * 0.16, y + size * 0.86, center[0] + size * 0.16, y + size * 0.89], r, 1.8 * r, grey(0.0, 0.3));
        let title = trn("Double {n}", &[("n", (n + 1).to_string())]);
        c.text_c(center[0], y + size + 1.1 * r, 0.9 * r, Face::Semi, &title, pal().t1);
        let pose = POSES.get(theme::pick(&self.mods[module].set, "Pose")).copied().unwrap_or("Live");
        c.text_c(center[0], y + size + 2.4 * r, 0.7 * r, Face::Regular, tr(pose), fade(pal().t3, reveal));
        self.fakes.preview = Some([n as f32, x, y, size, alpha, clip[0], clip[1], clip[2], clip[3], -15.0]);
    }

    fn fakes_set(&self) -> Option<Vec<super::data::Setting>> {
        self.mods.iter().find(|m| m.id == MODULE && m.on).map(|m| m.set.clone())
    }

    /// The card's actions: one more where the player looks; none.
    pub(super) fn fakes_put(&mut self) {
        self.fakes.put = true;
    }

    pub(super) fn fakes_clear(&mut self) {
        self.fakes.clear = true;
    }

    /// A double hit (Java, the attack held back): thrown back from the player, as a hit throws.
    pub(super) fn fakes_hit(&mut self, n: usize, from: [f64; 3]) {
        let Some(set) = self.fakes_set() else { return };
        if !theme::on(&set, "Takes knockback") {
            return;
        }
        let Some(d) = self.fakes.list.get_mut(n) else { return };
        let here = [d.at[0] + d.off[0], d.at[1] + d.off[1], d.at[2] + d.off[2]];
        let (dx, dz) = (here[0] - from[0], here[2] - from[2]);
        let l = (dx * dx + dz * dz).sqrt().max(0.01);
        d.vel = [dx / l * 0.42, 0.36, dz / l * 0.42];
    }

    /// Each tick ("x y z yaw pitch body sneak swing | lx ly lz": the player, where they look): the doubles' places.
    pub(super) fn fakes_tick(&mut self, s: &str) -> String {
        let v: Vec<f64> = s.split(|c| c == ' ' || c == '|').filter_map(|x| x.trim().parse().ok()).collect();
        let Some(mut set) = self.fakes_set() else {
            let was = std::mem::replace(&mut self.fakes.was_on, false);
            self.fakes.list.clear();
            self.fakes.past.clear();
            self.fakes.editing = None;
            return if was { "fakeclear".into() } else { String::new() };
        };
        if v.len() < 11 {
            return String::new();
        }
        let me = State { pos: [v[0], v[1], v[2]], yaw: v[3] as f32, pitch: v[4] as f32, body: v[5] as f32, sneak: v[6] > 0.5, swing: v[7] > 0.5 };
        let look = [v[8], v[9], v[10]];
        self.fakes.past.push_back(me);
        while self.fakes.past.len() > LAG + 1 {
            self.fakes.past.pop_front();
        }
        let mut out = Vec::new();
        // on: one at once, where the player looks
        if !self.fakes.was_on && self.fakes.list.is_empty() {
            self.fakes.put = true;
        }
        self.fakes.was_on = true;
        if std::mem::take(&mut self.fakes.clear) {
            self.fakes.list.clear();
            out.push("fakeclear".to_string());
        }
        let added = std::mem::take(&mut self.fakes.put) && self.fakes.list.len() < MOST;
        if added {
            let (dx, dz) = (me.pos[0] - look[0], me.pos[2] - look[2]);
            let l = (dx * dx + dz * dz).sqrt().max(0.01);
            let n = [dx / l, 0.0, dz / l];
            // (the glass halfway between them)
            let mid = [(me.pos[0] + look[0]) / 2.0, look[1], (me.pos[2] + look[2]) / 2.0];
            self.fakes.list.push(Double { at: look, glass: (mid, n), pose: studio_pose(&set), ..Default::default() });
        }
        let count = self.fakes.list.len().max(1);
        let selected = if added { count - 1 } else { (theme::num(&set, "Edit double").max(1.0) as usize - 1).min(count - 1) };
        // New doubles become the selection. Its slider lists only doubles that actually exist.
        for setting in &mut set {
            if setting.label == "Edit double" {
                if let super::data::Kind::Slider { v, max, .. } = &mut setting.kind { *v = (selected + 1) as f32; *max = count as f32; }
            }
        }
        if let Some(module) = self.mods.iter_mut().find(|m| m.id == MODULE) {
            if let Some(setting) = module.set.iter_mut().find(|s| s.label == "Edit double") {
                if let super::data::Kind::Slider { v, max, .. } = &mut setting.kind { *v = (selected + 1) as f32; *max = count as f32; }
            }
        }
        // Selecting another double loads its own pose into the editor instead of overwriting
        // it with the last double's sliders. New doubles begin with the current studio setup.
        if self.fakes.editing.is_some_and(|old| old != selected) {
            if let Some(d) = self.fakes.list.get(selected) {
                let saved = d.pose;
                for setting in &mut set {
                    if setting.label == "Pose" {
                        if let super::data::Kind::Choice { v, .. } = &mut setting.kind { *v = if saved.is_some() { 5 } else { 0 }; }
                    } else if let Some(axis) = POSE_AXES.iter().position(|name| *name == setting.label) {
                        if let super::data::Kind::Slider { v, .. } = &mut setting.kind { *v = saved.map_or(0.0, |p| p[axis].to_degrees()); }
                    }
                }
                if let Some(module) = self.mods.iter_mut().find(|m| m.id == MODULE) { module.set = set.clone(); }
                self.save();
            }
        }
        self.fakes.editing = Some(selected);
        let pose = studio_pose(&set);
        if let Some(d) = self.fakes.list.get_mut(selected) { d.pose = pose; }
        let mode = theme::pick(&set, "Mode");
        let faces = theme::on(&set, "Looks at you");
        for (i, d) in self.fakes.list.iter_mut().enumerate() {
            // thrown back: it flies, falls back to where it was put and stays where it lands
            if d.vel != [0.0; 3] || d.off[1] > 0.0 {
                for k in 0..3 {
                    d.off[k] += d.vel[k];
                }
                d.vel[0] *= 0.6;
                d.vel[2] *= 0.6;
                d.vel[1] -= 0.08;
                if d.off[1] <= 0.0 {
                    d.off[1] = 0.0;
                    d.vel = [0.0; 3];
                }
            }
            let st = match mode {
                1 => {
                    let p = mirror(me.pos, d.glass);
                    State { pos: p, yaw: mirror_yaw(me.yaw, d.glass.1), body: mirror_yaw(me.body, d.glass.1), ..me }
                }
                // (where the player was a moment ago, as they were; never into them: there it waits, looking at them)
                2 => {
                    let old = *self.fakes.past.front().unwrap_or(&me);
                    let near = ((old.pos[0] - me.pos[0]).powi(2) + (old.pos[2] - me.pos[2]).powi(2)).sqrt() < 1.6;
                    if near {
                        let l = d.last.unwrap_or(d.at);
                        let yaw = yaw_to(l, me.pos);
                        State { pos: l, yaw, pitch: 0.0, body: yaw, sneak: false, swing: false }
                    } else {
                        d.last = Some(old.pos);
                        old
                    }
                }
                _ => {
                    let here = [d.at[0] + d.off[0], d.at[1] + d.off[1], d.at[2] + d.off[2]];
                    let yaw = if faces { yaw_to(here, me.pos) } else { wrap(yaw_to(d.glass.0, d.at)) };
                    State { pos: here, yaw, pitch: 0.0, body: yaw, sneak: false, swing: false }
                }
            };
            let pos = if mode == 0 { st.pos } else { [st.pos[0] + d.off[0], st.pos[1] + d.off[1], st.pos[2] + d.off[2]] };
            out.push(format!("fake {i} {:.3} {:.3} {:.3} {:.1} {:.1} {:.1} {} {}", pos[0], pos[1], pos[2], st.yaw, st.pitch, st.body, st.sneak as i32, st.swing as i32));
            if let Some(target) = d.pose {
                for (v, to) in d.pose_now.iter_mut().zip(target) { *v += (to - *v) * 0.35; }
                out.push(format!("fakepose {i} {}", d.pose_now.iter().map(|v| format!("{v:.5}")).collect::<Vec<_>>().join(" ")));
            } else { out.push(format!("fakepose {i} live")); }
        }
        // Equipment follows the native selection, including a newly added double. The Java
        // entity map is still growing while that tick is applied and cannot clamp the index.
        out.push(format!("fakewear {}", if theme::on(&set, "Copies your armour") { selected as isize } else { -1 }));
        out.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn studio_custom_pose_is_bounded_and_kept_in_configs() {
        use super::super::data::Kind;
        let mut g = Gui::new();
        let i = g.mods.iter().position(|m| m.id == MODULE).unwrap();
        g.mods[i].on = true;
        for s in &mut g.mods[i].set {
            if s.label == "Pose" { if let Kind::Choice { v, .. } = &mut s.kind { *v = 5; } }
            if s.label == "Right arm pitch" { if let Kind::Slider { v, .. } = &mut s.kind { *v = -90.0; } }
        }
        assert!((studio_pose(&g.mods[i].set).unwrap()[6] + std::f32::consts::FRAC_PI_2).abs() < 0.001);
        let (values, _, parts) = super::super::configs::parse(&g.snapshot("Pose", super::super::configs::P_ALL));
        let mut copy = Gui::new(); copy.apply_vals(&values, parts);
        assert_eq!(studio_pose(&g.mods[i].set), studio_pose(&copy.mods[i].set));
        let said = g.fakes_tick("0 64 0 0 0 0 0 0 | 0 64 3");
        assert!(said.contains("fakepose 0 "));
        assert!(g.fakes.list[0].pose.unwrap()[6] < -1.5);
        let first = g.fakes.list[0].pose;
        g.fakes.put = true;
        for s in &mut g.mods[i].set {
            if s.label == "Edit double" { if let Kind::Slider { v, .. } = &mut s.kind { *v = 8.0; } }
        }
        let said = g.fakes_tick("0 64 0 0 0 0 0 0 | 0 64 3");
        assert!(said.lines().any(|l| l == "fakewear 1"), "{said}");
        assert_eq!(g.fakes.list[0].pose, first);
    }

    #[test]
    fn a_double_is_put_and_mirrors() {
        let mut g = Gui::new();
        assert_eq!(g.fakes_tick("0 64 0 0 0 0 0 0 | 0 64 4"), "");
        let i = g.mods.iter().position(|m| m.id == MODULE).unwrap();
        g.mods[i].on = true;
        // on: one where the player looks, looking back at them (south of them: it faces north)
        let s = g.fakes_tick("0 64 0 0 0 0 0 0 | 0 64 4");
        assert!(s.starts_with("fake 0 0.000 64.000 4.000 180.0") || s.starts_with("fake 0 0.000 64.000 4.000 -180.0"), "{s}");
        // the mirror: the player a step to the glass, the double a step to it too; turned the other way
        for st in g.mods[i].set.iter_mut() {
            if let super::super::data::Kind::Choice { v, opts } = &mut st.kind {
                if st.label == "Mode" {
                    *v = opts.iter().position(|o| *o == "Mirror").unwrap();
                }
            }
        }
        let s = g.fakes_tick("0 64 1 0 0 0 0 1 | 0 64 9");
        let f: Vec<f64> = s.split(' ').skip(2).filter_map(|v| v.parse().ok()).collect();
        assert!((f[2] - 3.0).abs() < 1e-6, "{s}");
        assert!((wrap(f[3] as f32).abs() - 180.0).abs() < 0.2, "{s}");
        assert!(s.lines().next().unwrap().ends_with(" 0 1"), "{s}");
        // a hit throws it back (in the stands)
        g.fakes_hit(0, [0.0, 64.0, 0.0]);
        assert!(g.fakes.list[0].vel[2] > 0.0);
        // off: cleared once
        g.mods[i].on = false;
        assert_eq!(g.fakes_tick("0 64 0 0 0 0 0 0 | 0 64 4"), "fakeclear");
        assert_eq!(g.fakes_tick("0 64 0 0 0 0 0 0 | 0 64 4"), "");
    }
}
