//! The Cosmetics page's look «in the hands»: the toggle at the top right of the item's place (the showcase, or CS's
//! hands holding it as the game would), and those hands as in the game — the left button a swing (a rifle's shot),
//! the right its heavy one, the inspect key the inspect; the pointer dragged turns the view, the hands swaying after it.

use super::draw::{grey, mix, Canvas};
use super::i18n::{tr, trn};
use super::text::{fonts, Face};
use super::theme::pal;
use super::{ak, Cat, Gui, Hit, A_HOV};
use crate::cosmetics::catalog::{self, Kind};
use crate::cosmetics::view::{self, Event};

/// The part (Hit::Cosm): the toggle's chip (n 0 the showcase, 1 the hands).
pub(super) const HAND: u8 = 38;

const MODES: [&str; 2] = ["Showcase", "In the hands"];

impl Gui {
    /// The toggle at the top right of the item's place (the chosen one's light slides to it).
    pub(super) fn hand_chips(&mut self, c: &mut Canvas, view: [f32; 4]) {
        let r = self.r;
        let on = self.cosm.in_hand as usize;
        let mut x = view[2] - 0.6 * r;
        let mut rects = [[0.0; 4]; MODES.len()];
        for (k, name) in MODES.iter().enumerate().rev() {
            let w = fonts().width(Face::Medium, 0.62 * r, tr(name)) + 1.1 * r;
            rects[k] = [x - w, view[1] + 0.6 * r, x, view[1] + 2.0 * r];
            x = rects[k][0] - 0.3 * r;
        }
        // dark under (the hands may be bright)
        for (k, b) in rects.iter().enumerate() {
            let hov = self.an(ak(A_HOV, 36_300 + k), if self.hover == Hit::Cosm(HAND, k as u16) { 1.0 } else { 0.0 }, 14.0, 0.0);
            c.rect(*b, 0.7 * r, mix(grey(0.0, 0.42), grey(0.12, 0.6), hov));
        }
        self.cosm_pill(c, &rects, on, 36_020, 0.7 * r, grey(0.25, 0.7));
        for (k, (name, b)) in MODES.iter().zip(rects).enumerate() {
            let hit = Hit::Cosm(HAND, k as u16);
            let hv = self.an(ak(A_HOV, 35_500 + k), if on == k { 1.0 } else if self.hover == hit { 0.5 } else { 0.0 }, 14.0, 0.0);
            c.text_c((b[0] + b[2]) * 0.5, b[3] - 0.45 * r, 0.62 * r, Face::Medium, tr(name), mix(pal().t3, pal().t1, hv));
            self.hits.push((b, hit));
        }
    }

    /// A toggle's chip pressed: the look in the hands (taken out anew) or on show.
    pub(super) fn hand_press(&mut self, n: u16) {
        let on = n == 1;
        if self.cosm.in_hand == on {
            return;
        }
        self.cosm.in_hand = on;
        if let Some(l) = self.cosm.look.as_mut() {
            l.hand_turn = [0.0; 2];
            l.spin = [0.0; 2];
            l.vel = [0.0; 2];
        }
        if on {
            view::look_reset();
        }
        self.cosm_swap(false);
        self.sound_tick(0.5);
    }

    /// Whether the look shows a rifle in the hands (its left button shoots; it has no heavy one).
    fn hand_rifle(&self) -> bool {
        self.cosm.look.as_ref().and_then(|l| catalog::get().item(&l.item).map(|i| i.kind == Kind::Rifle)).unwrap_or(false)
    }

    /// The hint under the item in the hands: what its buttons do (the inspect key the player's).
    pub(super) fn hand_hint(&self) -> String {
        let key = crate::cosmetics::loadout::current().0.inspect;
        let key = if key.is_empty() { tr("None").to_string() } else { key };
        if self.hand_rifle() {
            trn("Left button to shoot, {key} to inspect; drag to look round", &[("key", key)])
        } else {
            trn("Left button to swing, right for the heavy one, {key} to inspect; drag to look round", &[("key", key)])
        }
    }

    /// A press on the item's place let go where it began, in the hands: the left button (0) a swing or a rifle's
    /// shot, the right one (1) a knife's heavy swing.
    pub(super) fn hand_click(&mut self, button: i32) {
        let e = match (button, self.hand_rifle()) {
            (0, true) => Event::Shoot,
            (0, false) => Event::Swing(false),
            (1, false) => Event::Heavy(false),
            _ => return,
        };
        view::look_event(e);
    }

    /// A key on the menu: the inspect key inspects what the look's hands hold.
    pub(super) fn hand_key(&mut self, name: &str) -> bool {
        let shown = self.cat == Cat::Cosmetics && self.cosm.in_hand && self.cosm.look.is_some();
        if shown && !name.is_empty() && name == crate::cosmetics::loadout::current().0.inspect {
            view::look_event(Event::Inspect);
            // (its letter typed nowhere)
            self.eat = Some(name.to_string());
            return true;
        }
        false
    }
}
