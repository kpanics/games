//! Item Cooldowns (her word, 25.10: "красиво, с вещами FT и HW"; "вид — оба: иконки с кольцом / список с полосками"):
//! what the player used and how long until it works again — the game's own cooldowns, which are also how the servers
//! hold back their own things (FunTime's trap, plast, disorientation…, HolyWorld's stun trap…), each named as the
//! server names it. Shown where the module says: its panel (the Cooldowns element: icons in rings, or a list of bars),
//! the seconds over the hotbar's slots, rings by the crosshair; a sound as one is ready. FunTime and HolyWorld forbid
//! it (swap.rs hidden_here): there it is gone.

use super::*;

pub(super) const MODULE: &str = "item-cooldowns";

impl Gui {
    /// Item Cooldowns on (and not forbidden here) and shown at `place` ("Panel", "Hotbar", "Crosshair").
    pub(super) fn cool_at(&self, place: &str) -> bool {
        if self.hidden_here(MODULE) {
            return false;
        }
        self.mods.iter().find(|m| m.id == MODULE && m.on).is_some_and(|m| match theme::find(&m.set, "Where") {
            Some(Kind::Multi { on, opts }) => opts.iter().zip(on).any(|(o, on)| *on && *o == place),
            _ => false,
        })
    }

    /// Seconds as the panel shows them (tenths under ten, when it says so).
    pub(super) fn cool_secs(&self, secs: f32) -> String {
        let tenths = self.mods.iter().find(|m| m.id == "cooldowns").is_none_or(|m| theme::on(&m.set, "Tenths under 10 s"));
        if secs < 10.0 && tenths {
            format!("{:.1}", secs.max(0.0))
        } else {
            format!("{}", secs.ceil() as i32)
        }
    }

    /// The panel as a list: each thing's icon, its name, its seconds, a bar draining as it cools.
    pub(super) fn cool_list(&mut self, c: &mut Canvas, x: &Ctx, list: &[(usize, Cool)]) -> Option<[f32; 2]> {
        let s = x.s;
        let p = x.pad();
        let names = x.on("Names");
        let secs_on = x.on("Seconds");
        let w = theme::num(&x.set, "List width").max(100.0) * s;
        let row = if names { 22.0 * s } else { 16.0 * s };
        let hgt = 2.0 * p + row * list.len() as f32 - 4.0 * s;
        x.plate(c, [M, M, M + w, M + hgt]);
        let bar_col = x.col("Ring", x.el.accent);
        for (k, (idx, cd)) in list.iter().enumerate() {
            let y = M + p + row * k as f32;
            let icon = 12.0 * s;
            let iy = y + (row - 4.0 * s - icon) / 2.0;
            if *idx == usize::MAX || !x.icons {
                c.rect([M + p, iy, M + p + icon, iy + icon], 2.5 * s, grey(1.0, 0.25));
            } else {
                self.hud.icons.push([2.0, *idx as f32, M + p, iy, icon, icon, c.alpha]);
            }
            let tx = M + p + icon + 6.0 * s;
            let right = M + w - p;
            let secs = self.cool_secs(cd.secs);
            if secs_on {
                c.text_r(right, y + if names { 9.5 * s } else { 8.0 * s }, 9.0 * s, Face::Mono, &secs, x.el.text);
            }
            let sw = if secs_on { fonts().width(Face::Mono, 9.0 * s, &secs) + 6.0 * s } else { 0.0 };
            if names {
                // (a long name stops short of the seconds)
                let room = right - sw - tx;
                let mut name = cd.name.clone();
                while !name.is_empty() && fonts().width(Face::Medium, 9.5 * s, &name) > room {
                    name.pop();
                }
                c.text(tx, y + 9.5 * s, 9.5 * s, Face::Medium, &name, x.el.text);
            }
            let by = if names { y + 13.5 * s } else { y + (row - 4.0 * s) / 2.0 - 1.25 * s };
            let bx1 = if names { right } else { right - sw };
            c.rect([tx, by, bx1, by + 2.5 * s], 1.25 * s, grey(1.0, 0.1));
            c.rect([tx, by, tx + (bx1 - tx) * cd.frac.clamp(0.0, 1.0), by + 2.5 * s], 1.25 * s, bar_col);
        }
        Some([w, hgt])
    }

    /// Rings by the crosshair (under it, in a row): each thing cooling, its icon in a ring draining, its seconds.
    pub(super) fn cool_cross(&mut self, c: &mut Canvas, w: f32, h: f32) {
        let show = self.cool_at("Crosshair") && !self.hud.editing;
        let list: Vec<(usize, Cool)> = if show { self.hud.facts.cools.iter().cloned().enumerate().take(5).collect() } else { Vec::new() };
        let k = self.an(ak(A_HUD, 9100), if list.is_empty() { 0.0 } else { 1.0 }, 10.0, 0.0);
        if k < 0.01 || list.is_empty() {
            return;
        }
        let s = self.hud.u;
        let gap = 32.0 * s;
        let x0 = w / 2.0 - gap * (list.len() as f32 - 1.0) / 2.0;
        let y = h / 2.0 + 34.0 * s + (1.0 - k) * 4.0 * s;
        let accent = theme::pal().accent;
        c.alpha = k;
        for (n, (idx, cd)) in list.iter().enumerate() {
            let cc = [x0 + gap * n as f32, y];
            // (a dark disc under it: seen over the sky as over the ground)
            c.rect([cc[0] - 13.0 * s, cc[1] - 13.0 * s, cc[0] + 13.0 * s, cc[1] + 13.0 * s], 13.0 * s, grey(0.0, 0.32));
            c.ring(cc, 12.0 * s, 2.0 * s, grey(1.0, 0.16));
            c.arc(cc, 12.0 * s, 2.0 * s, cd.frac.clamp(0.0, 1.0), accent);
            self.hud.icons.push([2.0, *idx as f32, cc[0] - 7.5 * s, cc[1] - 7.5 * s, 15.0 * s, 15.0 * s, k]);
            let secs = self.cool_secs(cd.secs);
            c.text_c(cc[0] + 0.7 * s, cc[1] + 25.7 * s, 9.0 * s, Face::Semi, &secs, grey(0.0, 0.55));
            c.text_c(cc[0], cc[1] + 25.0 * s, 9.0 * s, Face::Semi, &secs, grey(1.0, 0.95));
        }
        c.alpha = 1.0;
    }

    /// Each HUD frame: a thing gone from the cooldowns as its time ran out — a soft sound, if it says so.
    pub(super) fn cool_sound(&mut self) {
        let now: Vec<(String, f32)> = self.hud.facts.cools.iter().map(|c| (c.name.clone(), c.secs)).collect();
        let was = std::mem::replace(&mut self.hud.cool_last, now);
        let on = self.mods.iter().find(|m| m.id == MODULE && m.on).is_some_and(|m| theme::on(&m.set, "Sound when ready")) && !self.hidden_here(MODULE);
        if !on {
            return;
        }
        let ready = was.iter().any(|(n, secs)| *secs < 0.75 && !self.hud.cool_last.iter().any(|(m, _)| m == n));
        if ready {
            crate::launch::sound::play(crate::launch::sound::Sfx::Shimmer, 0.5 * self.volume(), 0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::swap::{FUNTIME, HOLYWORLD, REALLYWORLD};
    use super::*;

    #[test]
    fn shown_where_chosen_and_gone_where_forbidden() {
        let mut g = Gui::new();
        assert!(!g.cool_at("Panel"));
        let i = g.mods.iter().position(|m| m.id == MODULE).unwrap();
        g.mods[i].on = true;
        assert!(g.cool_at("Panel") && g.cool_at("Hotbar") && !g.cool_at("Crosshair"));
        for server in [FUNTIME, HOLYWORLD] {
            g.swap_call("server", server, "");
            assert!(g.hidden_here(MODULE) && !g.cool_at("Panel"), "{server}");
        }
        g.swap_call("server", REALLYWORLD, "");
        assert!(!g.hidden_here(MODULE) && g.cool_at("Panel"));
        // the seconds: tenths under ten, whole above
        assert_eq!((g.cool_secs(4.26), g.cool_secs(12.2)), ("4.3".to_string(), "13".to_string()));
    }
}
