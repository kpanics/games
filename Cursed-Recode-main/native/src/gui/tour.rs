//! The menu's tour for a new player: seven steps, each lighting its part of the menu (the rest of it dimmed) with a
//! card by it — the sections, a module, the search, the configs, the interface, Ctrl+Z. It comes once, the first time
//! the menu opens on a computer that never had the client's settings; Esc or "Skip" ends it; Settings' "Hints" keeps
//! it from coming, "Show the tour" brings it back. Its light and its card glide from one step to the next.

use super::data::Cat;
use super::draw::{grey, mix, Canvas};
use super::i18n::tr;
use super::text::{fonts, Face};
use super::theme::{self, pal};
use super::{ak, ease_out, smooth, Gui, Hit, Phase, A_HOV};

/// The steps: what is lit, the card's title and words.
const STEPS: [(&str, &str); 7] = [
    ("Welcome to Recode", "A short tour of the menu: seven steps, a minute. Esc skips it; Settings can show it again."),
    ("Sections", "Modules live in sections: Player, Visuals, World, Misc. The number by each is how many of them are on."),
    ("A module", "A press switches it on, the arrows open its settings, the middle button gives it a key."),
    ("Search", "Any word finds it — a module, a setting, an option, in Russian or English, even typed in the other layout. “/” starts it; the key by it shows every bind on one page."),
    ("Configs", "Save the client as a config, share it by a code, give it to a server: it loads as you join, and your own comes back as you leave."),
    ("Interface", "The HUD: every element, its look and its place on the screen — “Edit the layout” moves them."),
    ("Changed something by mistake?", "Ctrl+Z takes it back, Ctrl+Y does it again; the round arrow puts a module or a whole section back as it came. That is all — have a good game."),
];

/// The tour on now: its step, when the step came.
pub(super) struct Tour {
    step: usize,
    at: f32,
}

impl Gui {
    /// The tour from its start (the modules' first section shown: a module is lit on the third step).
    pub(super) fn tour_start(&mut self) {
        if !matches!(self.cat, Cat::Player | Cat::Render | Cat::World | Cat::Misc) || !self.q.is_empty() {
            self.q.clear();
            self.open_cat(Cat::Player);
        }
        self.card = None;
        self.pop = None;
        self.pick = None;
        self.tour = Some(Tour { step: 0, at: self.now });
        self.sound_tick(0.6);
    }

    /// Each frame of the open menu: the tour due (a new player's first menu, its hints on) starts once it is open.
    pub(super) fn tour_due_check(&mut self) {
        if self.tour_due && self.phase == Phase::Open && self.now - self.opened_at > 0.45 {
            self.tour_due = false;
            if theme::on(&self.client, "Hints") {
                self.tour_start();
            }
        }
    }

    fn tour_go(&mut self, to: Option<usize>) {
        match to {
            Some(s) if s < STEPS.len() => {
                if let Some(t) = self.tour.as_mut() {
                    t.step = s;
                    t.at = self.now;
                }
                self.sound_tick(0.5);
            }
            _ => {
                self.tour = None;
                self.switch_sound(false, 0.5);
            }
        }
    }

    /// A press while the tour is on: its buttons; elsewhere nothing (the menu under it waits).
    pub(super) fn tour_press(&mut self, h: Hit) {
        let Some(step) = self.tour.as_ref().map(|t| t.step) else { return };
        match h {
            Hit::Tour(1) => self.tour_go(Some(step + 1)),
            Hit::Tour(2) => self.tour_go(None),
            Hit::Tour(3) => self.tour_go(step.checked_sub(1)),
            _ => {}
        }
    }

    /// A key while the tour is on: Esc ends it, Enter and → go on, ← goes back.
    pub(super) fn tour_key(&mut self, escape: bool, enter: bool, name: &str) -> bool {
        let Some(step) = self.tour.as_ref().map(|t| t.step) else { return false };
        if escape {
            self.tour_go(None);
        } else if enter || name == "Right" || name == "Space" {
            self.tour_go(Some(step + 1));
        } else if name == "Left" {
            if let Some(s) = step.checked_sub(1) {
                self.tour_go(Some(s));
            }
        }
        true
    }

    /// Where a step's light goes: its part of the menu as this frame laid it out.
    fn tour_target(&self, step: usize) -> [f32; 4] {
        let r = self.r;
        let find = |want: &dyn Fn(&Hit) -> bool| self.hits.iter().filter(|(_, h)| want(h)).map(|(q, _)| *q).reduce(|a, b| [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])]);
        let win = [self.win[0], self.win[1], self.win[0] + self.win[2], self.win[1] + self.win[3]];
        let q = match step {
            1 => find(&|h| matches!(h, Hit::Nav(Cat::Player | Cat::Render | Cat::World | Cat::Misc))),
            2 => self.hits.iter().find(|(_, h)| matches!(h, Hit::Tile(_))).map(|(q, _)| *q),
            3 => find(&|h| matches!(h, Hit::Search | Hit::BindsBtn)),
            4 => find(&|h| matches!(h, Hit::Nav(Cat::Configs))),
            5 => find(&|h| matches!(h, Hit::Nav(Cat::Hud))),
            _ => None,
        };
        // (a little room round what is lit)
        q.map_or(win, |q| [q[0] - 0.35 * r, q[1] - 0.35 * r, q[2] + 0.35 * r, q[3] + 0.35 * r])
    }

    /// The tour over the menu: the rest dimmed round the lit part, its card by it.
    pub(super) fn tour_draw(&mut self, c: &mut Canvas) {
        let Some((step, at)) = self.tour.as_ref().map(|t| (t.step, t.at)) else {
            self.an(ak(A_HOV, 9950), 0.0, 8.0, 0.0);
            return;
        };
        let r = self.r;
        let k = self.an(ak(A_HOV, 9950), 1.0, 8.0, 0.0);
        let goal = self.tour_target(step);
        let hole: [f32; 4] = std::array::from_fn(|n| self.an(ak(A_HOV, 9951 + n), goal[n], 11.0, goal[n]));
        let (w, h) = (self.w, self.h);
        // the veil round the light (the whole screen takes the press: the menu under it waits)
        let dim = grey(0.0, 0.58 * k);
        c.rect([0.0, 0.0, w, hole[1]], 0.0, dim);
        c.rect([0.0, hole[3], w, h], 0.0, dim);
        c.rect([0.0, hole[1], hole[0], hole[3]], 0.0, dim);
        c.rect([hole[2], hole[1], w, hole[3]], 0.0, dim);
        let whole = step == 0 || step == STEPS.len() - 1;
        if !whole {
            let pulse = 0.5 + 0.5 * ((self.now - at) * 3.0).sin();
            c.rect_line(hole, 0.75 * r, 1.5, theme::fade(pal().accent, (0.55 + 0.3 * pulse) * k));
        }
        self.hits.push(([0.0, 0.0, w, h], Hit::Tour(0)));
        // the card: by the light (right of it, else left, else under it); over the middle of the window for the first
        // and the last
        let (title, words) = STEPS[step];
        let cw = 22.0 * r;
        let lines = fonts().wrap(Face::Regular, 0.8 * r, tr(words), cw - 2.4 * r);
        let ch = 7.2 * r + lines.len() as f32 * 1.2 * r;
        let (mut cx, mut cy) = if whole {
            (self.win[0] + (self.win[2] - cw) / 2.0, self.win[1] + (self.win[3] - ch) / 2.0)
        } else if goal[2] + 1.2 * r + cw < w - 8.0 {
            (goal[2] + 1.2 * r, goal[1])
        } else if goal[0] - 1.2 * r - cw > 8.0 {
            (goal[0] - 1.2 * r - cw, goal[1])
        } else {
            (goal[0], goal[3] + 1.0 * r)
        };
        cx = cx.clamp(8.0, (w - cw - 8.0).max(8.0));
        cy = cy.clamp(8.0, (h - ch - 8.0).max(8.0));
        let cx = self.an(ak(A_HOV, 9955), cx, 11.0, cx);
        let cy = self.an(ak(A_HOV, 9956), cy, 11.0, cy);
        // each step's words rise in
        let e = self.now - at;
        let rise = ease_out(smooth(0.0, 0.35, e));
        let card = [cx, cy, cx + cw, cy + ch];
        let base = c.alpha;
        c.alpha = base * k;
        c.shadow(card, r, 3.0 * r, grey(0.0, 0.6));
        let g = pal().glass;
        c.rect(card, r, [g[0].max(0.05), g[1].max(0.05), g[2].max(0.055), 0.98]);
        c.rect_line(card, r, 1.0, grey(1.0, 0.06));
        self.hits.push((card, Hit::Tour(0)));
        let (x0, x1) = (card[0] + 1.2 * r, card[2] - 1.2 * r);
        let dy = (1.0 - rise) * 0.4 * r;
        c.alpha = base * k * rise;
        c.text(x0, card[1] + 3.2 * r + dy, 1.05 * r, Face::Bold, tr(title), pal().t1);
        let mut y = card[1] + 4.45 * r + dy;
        for l in &lines {
            c.text(x0, y, 0.8 * r, Face::Regular, l, pal().t2);
            y += 1.2 * r;
        }
        c.alpha = base * k;
        // the steps as dots over the title, the one now long
        let mut dx = x0;
        for n in 0..STEPS.len() {
            let on = self.an(ak(A_HOV, 9960 + n), if n == step { 1.0 } else { 0.0 }, 12.0, 0.0);
            let dw = 0.42 * r + 0.9 * r * on;
            c.rect([dx, card[1] + 1.2 * r, dx + dw, card[1] + 1.62 * r], 0.21 * r, mix(grey(1.0, 0.18), pal().accent, on));
            dx += dw + 0.3 * r;
        }
        // Back (from the second), Skip, Next / Done
        let by = card[3] - 2.75 * r;
        let last = step + 1 == STEPS.len();
        let next = tr(if last { "Done" } else { "Next" });
        let nw = fonts().width(Face::Semi, 0.78 * r, next) + 2.2 * r;
        let nb = [x1 - nw, by, x1, by + 2.1 * r];
        let hv = self.an(ak(A_HOV, 9970), if self.hover == Hit::Tour(1) { 1.0 } else { 0.0 }, 12.0, 0.0);
        c.glow([(nb[0] + nb[2]) / 2.0, (nb[1] + nb[3]) / 2.0], nw * 0.55, theme::fade(pal().accent, 0.08 + 0.08 * hv));
        c.rect(nb, 0.62 * r, mix(theme::fade(pal().accent, 0.9), pal().accent, hv));
        c.text_c((nb[0] + nb[2]) / 2.0, nb[1] + 1.38 * r, 0.78 * r, Face::Semi, next, grey(0.04, 1.0));
        self.hits.push((nb, Hit::Tour(1)));
        let mut bx = nb[0] - 0.4 * r;
        for (id, label) in [(2u8, "Skip"), (3u8, "Back")] {
            if (id == 3 && step == 0) || (id == 2 && last) {
                continue;
            }
            let lw = fonts().width(Face::Medium, 0.78 * r, tr(label)) + 1.6 * r;
            let b = [bx - lw, by, bx, by + 2.1 * r];
            let hv = self.an(ak(A_HOV, 9970 + id as usize), if self.hover == Hit::Tour(id) { 1.0 } else { 0.0 }, 12.0, 0.0);
            c.rect(b, 0.62 * r, mix(grey(1.0, 0.0), pal().f3, hv));
            c.text_c((b[0] + b[2]) / 2.0, b[1] + 1.38 * r, 0.78 * r, Face::Medium, tr(label), mix(pal().t3, pal().t1, hv));
            self.hits.push((b, Hit::Tour(id)));
            bx = b[0] - 0.2 * r;
        }
        c.alpha = base;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tour_goes_on_back_and_ends() {
        let mut g = Gui::new();
        g.tour_start();
        assert_eq!(g.tour.as_ref().map(|t| t.step), Some(0));
        g.tour_press(Hit::Tour(1));
        g.tour_key(false, true, "");
        assert_eq!(g.tour.as_ref().map(|t| t.step), Some(2));
        g.tour_key(false, false, "Left");
        assert_eq!(g.tour.as_ref().map(|t| t.step), Some(1));
        for _ in 0..STEPS.len() {
            g.tour_press(Hit::Tour(1));
        }
        assert!(g.tour.is_none());
        g.tour_start();
        g.tour_key(true, false, "Escape");
        assert!(g.tour.is_none());
    }
}
