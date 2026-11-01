//! Name Tags (her word, 25.10: "неймтеги со значком Recode"): the players' names above them in the client's look —
//! on a plate of glass, the Recode mark by those who play with it, a thin line of their health under it (absorption
//! gold at its end), their armour and ping after the name, a friend's name lit; nearer, bigger, as the game's own are.
//! Only those the game itself would show a name for, as it would (a sneaking player's not through a wall): the game
//! tells who (Java's cursed.recode.fx.NameTags, its own name left out), the tags are drawn here.

use super::draw::{grey, mix, Canvas};
use super::text::{fonts, Face};
use super::theme::{self, fade};
use super::Gui;
use std::collections::{HashMap, HashSet};

pub(super) const MODULE: &str = "name-tags";

/// A player whose name is shown this frame.
#[derive(Clone, Debug, Default)]
struct Tag {
    nick: String,
    shown: String,
    at: [f64; 3],
    hp: f32,
    max: f32,
    gold: f32,
    armour: i32,
    ping: i32,
}

#[derive(Default)]
pub(super) struct Tags {
    list: Vec<Tag>,
    /// how much of each is seen (they come in and go out)
    seen: HashMap<String, f32>,
}

impl Gui {
    pub(super) fn tags_on(&self) -> bool {
        self.mods.iter().any(|m| m.id == MODULE && m.on)
    }

    /// This frame's players (Java: "nick\x1fshown\x1fx y z\x1fhp max gold\x1farmour\x1fping" a line each).
    pub(super) fn tags_set(&mut self, s: &str) {
        self.tags.list = s
            .lines()
            .filter_map(|l| {
                let f: Vec<&str> = l.split('\u{1f}').collect();
                if f.len() < 6 {
                    return None;
                }
                let at: Vec<f64> = f[2].split(' ').filter_map(|v| v.parse().ok()).collect();
                let hp: Vec<f32> = f[3].split(' ').filter_map(|v| v.parse().ok()).collect();
                (at.len() == 3 && hp.len() == 3).then(|| Tag {
                    nick: f[0].to_string(),
                    shown: f[1].to_string(),
                    at: [at[0], at[1], at[2]],
                    hp: hp[0],
                    max: hp[1].max(1.0),
                    gold: hp[2],
                    armour: f[4].parse().unwrap_or(0),
                    ping: f[5].parse().unwrap_or(-1),
                })
            })
            .collect();
    }

    /// The tags (the HUD's frame), the far ones first (the near ones over them).
    pub(super) fn tags_draw(&mut self, c: &mut Canvas, w: f32, h: f32) {
        let Some(set) = self.mods.iter().find(|m| m.id == MODULE && m.on).map(|m| m.set.clone()) else {
            self.tags.list.clear();
            self.tags.seen.clear();
            return;
        };
        let scale = theme::num(&set, "Scale").max(0.3);
        let (mark_on, hp_on, armour_on, ping_on, friends_lit) =
            (theme::on(&set, "Recode mark"), theme::on(&set, "Health"), theme::on(&set, "Armour"), theme::on(&set, "Ping"), theme::on(&set, "Friends lit"));
        let accent = super::theme::pal().accent;
        let cam = self.adapt_cam();
        let mut list = self.tags.list.clone();
        let dist = |t: &Tag| ((t.at[0] - cam[0]).powi(2) + (t.at[1] - cam[1]).powi(2) + (t.at[2] - cam[2]).powi(2)).sqrt() as f32;
        list.sort_by(|a, b| dist(b).total_cmp(&dist(a)));
        // (each comes in and goes out)
        let dt = self.dt;
        let names: HashSet<String> = list.iter().map(|t| t.nick.clone()).collect();
        for (n, v) in self.tags.seen.iter_mut() {
            let to = if names.contains(n) { 1.0 } else { 0.0 };
            *v += (to - *v) * (1.0 - (-dt * 12.0).exp());
        }
        self.tags.seen.retain(|_, v| *v > 0.01);
        let friends = self.play.friends.clone();
        for t in &list {
            let seen = *self.tags.seen.entry(t.nick.clone()).or_insert(0.0);
            let q = self.adapt_clip(t.at);
            if q[3] < 0.05 {
                continue;
            }
            let p = [(q[0] / q[3] * 0.5 + 0.5) * w, (1.0 - (q[1] / q[3] * 0.5 + 0.5)) * h];
            let d = dist(t).max(0.5);
            // as big as the game's own at this distance (a world's size), within reason
            let k = (7.0 / d).clamp(0.35, 1.6) * scale * self.hud.u.max(0.6);
            let fs = 13.0 * k;
            let friend = friends.iter().any(|f| f.eq_ignore_ascii_case(&t.nick));
            // one of ours: Recode's or Cursed's mark (crate::ours)
            let mark = if mark_on { crate::ours::client_of(&t.nick, false) } else { None };
            let name = if t.shown.trim().is_empty() { t.nick.clone() } else { t.shown.clone() };
            let mut extra = Vec::new();
            if armour_on && t.armour > 0 {
                extra.push(format!("⛨{}", t.armour));
            }
            if ping_on && t.ping >= 0 {
                extra.push(format!("{}ms", t.ping));
            }
            let extra = extra.join("  ");
            let nw = fonts().width(Face::Semi, fs, &name);
            let ew = if extra.is_empty() { 0.0 } else { fonts().width(Face::Mono, fs * 0.78, &extra) + fs * 0.6 };
            let mw = if mark.is_some() { fs * 1.15 } else { 0.0 };
            let pad = fs * 0.55;
            let pw = pad * 2.0 + mw + nw + ew;
            let ph = fs * 1.55;
            let y0 = p[1] - ph - fs * 0.2;
            let r = [p[0] - pw / 2.0, y0, p[0] + pw / 2.0, y0 + ph];
            let a = seen * (1.0 - ((d - 48.0) / 16.0).clamp(0.0, 1.0));
            if a < 0.01 {
                continue;
            }
            c.alpha = a;
            c.rect(r, ph * 0.38, [0.03, 0.03, 0.04, 0.62]);
            if friend && friends_lit {
                c.rect_line(r, ph * 0.38, 1.5, fade(accent, 0.85));
            }
            let base = y0 + ph * 0.5 + fs * 0.36;
            let mut x = r[0] + pad;
            if let Some(client) = mark {
                let m = fs * 0.9;
                crate::ours::draw(c, [x, y0 + (ph - m) / 2.0, x + m, y0 + (ph + m) / 2.0], client, 1.0);
                x += mw;
            }
            let name_col = if friend && friends_lit { mix(grey(1.0, 1.0), accent, 0.6) } else { grey(1.0, 0.96) };
            c.text(x, base, fs, Face::Semi, &name, name_col);
            if !extra.is_empty() {
                c.text(x + nw + fs * 0.6, base, fs * 0.78, Face::Mono, &extra, grey(1.0, 0.55));
            }
            // health: a thin line under the plate, absorption gold at its end
            if hp_on {
                let by = r[3] + fs * 0.22;
                let bh = (fs * 0.16).max(1.5);
                let total = (t.max + t.gold).max(1.0);
                let f_hp = (t.hp / total).clamp(0.0, 1.0);
                let f_gold = (t.gold / total).clamp(0.0, 1.0 - f_hp);
                let (x0, x1) = (r[0] + ph * 0.3, r[2] - ph * 0.3);
                let lw = x1 - x0;
                c.rect([x0, by, x1, by + bh], bh / 2.0, grey(0.0, 0.5));
                let low = (t.hp / t.max).clamp(0.0, 1.0);
                let col = mix([0.95, 0.32, 0.34, 1.0], [0.42, 0.85, 0.55, 1.0], low);
                c.rect([x0, by, x0 + lw * f_hp, by + bh], bh / 2.0, col);
                if f_gold > 0.0 {
                    c.rect([x0 + lw * f_hp, by, x0 + lw * (f_hp + f_gold), by + bh], bh / 2.0, [0.96, 0.8, 0.42, 1.0]);
                }
            }
            c.alpha = 1.0;
        }
        self.tags.list.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_players_facts_are_read() {
        let mut g = Gui::new();
        g.tags_set("Steve\u{1f}[VIP] Steve\u{1f}1 65.8 2\u{1f}14 20 4\u{1f}12\u{1f}38\nbad line");
        assert_eq!(g.tags.list.len(), 1);
        let t = &g.tags.list[0];
        assert_eq!((t.nick.as_str(), t.shown.as_str(), t.armour, t.ping), ("Steve", "[VIP] Steve", 12, 38));
        assert_eq!((t.hp, t.max, t.gold), (14.0, 20.0, 4.0));
    }
}
