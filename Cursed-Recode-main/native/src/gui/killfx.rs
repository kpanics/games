//! Kill Effects (her word, 25.10: to surpass the free Cursed's death cartoons — "ещё круче"): a kill marked as the
//! cartoons mark a death. The impact frame (a red and black flash, the burst from the one killed: one per kill, never a
//! shaking camera), a shockwave over the ground where they stood, their soul rising, blood or oil bursting, lightning,
//! fireworks in the client's colour, a sound. All of it on the player's screen only.
//!
//! The kill comes with Kill Message's (play.rs play_kill: who, where); what the game shows (particles, lightning,
//! fireworks, the sound) Java does on the next tick ("fx …", "sound …"); the impact frame and the shockwave are drawn
//! on the HUD by the camera Server Adaptation keeps (adapt.rs).

use super::draw::{grey, mix, Canvas};
use super::theme;
use super::Gui;
use std::time::Instant;

pub(super) const MODULE: &str = "kill-effects";
pub(super) const EFFECTS: [&str; 6] = ["Impact frame", "Shockwave", "Soul", "Blood", "Lightning", "Fireworks"];
pub(super) const SOUNDS: [&str; 5] = ["None", "Bell", "Thunder", "Deep hit", "Chime"];
const SOUND_IDS: [&str; 5] = ["", "minecraft:block.bell.use", "minecraft:entity.lightning_bolt.thunder", "minecraft:item.mace.smash_ground_heavy", "minecraft:block.amethyst_block.resonate"];

#[derive(Default)]
pub(super) struct KillFx {
    /// the shockwaves going out: where (the feet), since
    waves: Vec<([f64; 3], Instant)>,
    /// the impact frame: since, where (its middle)
    impact: Option<(Instant, [f64; 3])>,
}

const WAVE: f32 = 0.9;
const IMPACT: f32 = 0.2;

impl Gui {
    fn killfx_set(&self) -> Option<Vec<super::data::Setting>> {
        self.mods.iter().find(|m| m.id == MODULE && m.on).map(|m| m.set.clone())
    }

    fn killfx_has(set: &[super::data::Setting], what: &str) -> bool {
        match theme::find(set, "Effects") {
            Some(super::data::Kind::Multi { on, opts }) => opts.iter().zip(on).any(|(o, on)| *on && *o == what),
            _ => false,
        }
    }

    /// A kill (play_kill's facts: the one killed is a player or not, where its feet are, how tall): its effects.
    pub(super) fn killfx_kill(&mut self, player: bool, feet: Option<[f64; 3]>, tall: f64) {
        let Some(set) = self.killfx_set() else { return };
        let Some(p) = feet else { return };
        if theme::pick(&set, "Kills of") == 0 && !player {
            return;
        }
        let size = theme::num(&set, "Size").clamp(0.3, 3.0);
        let mid = [p[0], p[1] + tall / 2.0, p[2]];
        let now = Instant::now();
        if Gui::killfx_has(&set, "Impact frame") {
            self.killfx.impact = Some((now, mid));
        }
        if Gui::killfx_has(&set, "Shockwave") {
            self.killfx.waves.push((p, now));
            self.killfx.waves.retain(|w| w.1.elapsed().as_secs_f32() < WAVE + 0.3);
        }
        let at = format!("{:.2} {:.2} {:.2}", mid[0], mid[1], mid[2]);
        if Gui::killfx_has(&set, "Soul") {
            self.later(60.0, format!("fx soul {at} {size}"));
        }
        if Gui::killfx_has(&set, "Blood") {
            let oil = theme::pick(&set, "Liquid") == 1;
            self.later(0.0, format!("fx blood {at} {size} {}", oil as i32));
        }
        if Gui::killfx_has(&set, "Lightning") {
            self.later(0.0, format!("fx lightning {:.2} {:.2} {:.2}", p[0], p[1], p[2]));
        }
        if Gui::killfx_has(&set, "Fireworks") {
            let c = theme::or(theme::col(&set, "Colour"), super::theme::pal().accent, self.now);
            let rgb = theme::pack([c[0], c[1], c[2]]);
            self.later(120.0, format!("fx fireworks {:.2} {:.2} {:.2} {rgb:06x} {size}", mid[0], mid[1] + 0.8, mid[2]));
        }
        let sound = theme::pick(&set, "Sound").min(SOUND_IDS.len() - 1);
        if sound > 0 {
            let vol = theme::num(&set, "Volume") / 100.0;
            self.later(0.0, format!("sound {} {at} {vol:.2} {}", SOUND_IDS[sound], if sound == 1 { "0.6" } else { "1.0" }));
        }
    }

    /// The impact frame and the shockwaves (the HUD's frame).
    pub(super) fn killfx_draw(&mut self, c: &mut Canvas, w: f32, h: f32) {
        if self.killfx.waves.is_empty() && self.killfx.impact.is_none() {
            return;
        }
        let set = self.killfx_set().unwrap_or_default();
        let col = theme::or(theme::col(&set, "Colour"), super::theme::pal().accent, self.now);
        let size = theme::num(&set, "Size").clamp(0.3, 3.0) as f64;
        // ---- the shockwaves: two rings over the ground, out and fading
        let waves = self.killfx.waves.clone();
        for (p, since) in &waves {
            for (lag, k) in [(0.0f32, 1.0f32), (0.12, 0.6)] {
                let t = ((since.elapsed().as_secs_f32() - lag) / WAVE).clamp(0.0, 1.0);
                if t <= 0.0 || t >= 1.0 {
                    continue;
                }
                let e = 1.0 - (1.0 - t).powi(3);
                let r = (0.4 + 4.6 * e as f64) * size;
                let a = (1.0 - t).powf(1.4) * k;
                let width = (3.5 - 2.5 * t) * k;
                let n = 64;
                let y = p[1] + 0.06;
                for i in 0..n {
                    let a0 = std::f64::consts::TAU * i as f64 / n as f64;
                    let a1 = std::f64::consts::TAU * (i + 1) as f64 / n as f64;
                    self.adapt_seg(c, [p[0] + r * a0.cos(), y, p[2] + r * a0.sin()], [p[0] + r * a1.cos(), y, p[2] + r * a1.sin()], w, h, width, [col[0], col[1], col[2], col[3] * a]);
                }
            }
        }
        self.killfx.waves.retain(|w| w.1.elapsed().as_secs_f32() < WAVE + 0.2);
        // ---- the impact frame: black over the world a moment, red as it lets go, the burst from the one killed
        if let Some((since, mid)) = self.killfx.impact {
            let t = since.elapsed().as_secs_f32() / IMPACT;
            if t >= 1.0 {
                self.killfx.impact = None;
                return;
            }
            let black = if t < 0.3 { 0.62 } else { 0.62 * (1.0 - (t - 0.3) / 0.7) };
            let red = if t < 0.3 { 0.0 } else { 0.32 * (1.0 - t) };
            c.rect([0.0, 0.0, w, h], 0.0, [0.0, 0.0, 0.0, black.max(0.0)]);
            c.rect([0.0, 0.0, w, h], 0.0, [0.72, 0.02, 0.06, red.max(0.0)]);
            let q = self.adapt_clip(mid);
            if q[3] > 0.05 {
                let at = [(q[0] / q[3] * 0.5 + 0.5) * w, (1.0 - (q[1] / q[3] * 0.5 + 0.5)) * h];
                let r = self.r;
                let a = (1.0 - t).powi(2);
                // (the rays: long and short by turns, as a drawn burst; never the same twice for the same kill)
                let seed = (mid[0] * 13.1 + mid[2] * 7.7).abs().fract() as f32 * 6.28;
                for i in 0..16 {
                    let ang = seed + i as f32 * std::f32::consts::TAU / 16.0;
                    let long = if i % 2 == 0 { 1.0 } else { 0.55 };
                    let r0 = (1.2 + 3.0 * t) * r;
                    let r1 = r0 + (5.0 + 9.0 * t) * r * long;
                    c.line([at[0] + ang.cos() * r0, at[1] + ang.sin() * r0], [at[0] + ang.cos() * r1, at[1] + ang.sin() * r1], 2.4 * (1.0 - t) + 0.6, mix(grey(1.0, a), [0.95, 0.15, 0.18, a], t), 0.0);
                }
                c.ring(at, (1.0 + 4.0 * t) * r, 2.0, grey(1.0, 0.8 * a));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kill_is_marked_as_asked() {
        let mut g = Gui::new();
        // off: nothing
        g.killfx_kill(true, Some([1.0, 64.0, 2.0]), 1.8);
        assert!(g.killfx.impact.is_none());
        let i = g.mods.iter().position(|m| m.id == MODULE).unwrap();
        g.mods[i].on = true;
        // a mob, players only (the default): nothing
        g.killfx_kill(false, Some([1.0, 64.0, 2.0]), 1.8);
        assert!(g.killfx.impact.is_none());
        // a player: the impact frame, the wave, the soul and the sound asked of Java
        g.killfx_kill(true, Some([1.0, 64.0, 2.0]), 1.8);
        assert!(g.killfx.impact.is_some() && g.killfx.waves.len() == 1);
        let asked: Vec<String> = g.play_queue();
        assert!(asked.iter().any(|l| l.starts_with("fx soul 1.00 64.90 2.00")), "{asked:?}");
        assert!(asked.iter().any(|l| l.starts_with("sound minecraft:block.bell.use")), "{asked:?}");
    }
}
