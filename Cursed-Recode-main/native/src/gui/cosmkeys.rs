//! CS's keys among the game's: the inspect key (the loadout's; F as in CS2) — the player picks it on the page — taken
//! from the game's own controls it would set off too (F: the hands' swap), those moved to a free key and said in a
//! note, given back when the inspect key goes elsewhere. Java tells the controls each second and sets what is asked
//! ("gamekeys"); the hand the items are held in (the left as CS's cl_righthand 0) is chosen beside it.

use super::draw::{grey, mix, Canvas};
use super::i18n::{tr, trn};
use super::text::{fonts, Face};
use super::theme::pal;
use super::{ak, Gui, Hit, A_HOV};
use crate::cosmetics::loadout;

/// The parts (Hit::Cosm): the inspect key's chip, the hand's, the switch of all of it.
pub(super) const INSPECT_KEY: u8 = 39;
pub(super) const HAND_SIDE: u8 = 40;
pub(super) const ALL_ON: u8 = 43;

/// The keys a moved control may go to, the first free one taken (none of them the game's own by default).
const FREE: [&str; 12] = ["R", "V", "Z", "B", "H", "J", "K", "N", "U", "Y", "G", "M"];

impl Gui {
    /// The game's controls as Java tells them ("name\x1fkey\x1flabel", by \x1e; a key by Recode's name, none
    /// unbound): those on the inspect key moved, those moved for an old one given back; what Java is to set, a line
    /// each ("name\tkey").
    pub(super) fn keys_told(&mut self, list: &str) -> String {
        let mut keys: Vec<(String, String, String)> = list
            .split('\u{1e}')
            .filter_map(|e| {
                let mut f = e.split('\u{1f}');
                Some((f.next()?.to_string(), f.next().unwrap_or("").to_string(), f.next().unwrap_or("").to_string()))
            })
            // (F3's combinations are not keys of their own)
            .filter(|k| !k.0.is_empty() && !k.0.starts_with("key.debug."))
            .collect();
        let (lo, _) = loadout::current();
        // the inspect key only while CS's things are in the hands (on, a knife or a rifle put on) and there (else every
        // control the game's own again)
        let inspect = if lo.worn() && !crate::cosmetics::catalog::get().items.is_empty() { lo.inspect.clone() } else { String::new() };
        let mut out = Vec::new();
        let mut moved = Vec::new();
        // given back: what was moved off a key the inspect key no longer is — when it is still where it was put and its
        // own key is free (else the player has set it: theirs, forgotten)
        for (name, from, to) in &lo.moved {
            if *from == inspect {
                moved.push((name.clone(), from.clone(), to.clone()));
                continue;
            }
            let still = keys.iter().any(|k| k.0 == *name && k.1 == *to);
            if still && !keys.iter().any(|k| k.1 == *from) {
                out.push(format!("{name}\t{from}"));
                let label = keys.iter().find(|k| k.0 == *name).map_or(name.clone(), |k| k.2.clone());
                self.cosm.notes_due.push((label, trn("Back on {key}", &[("key", from.clone())])));
                if let Some(k) = keys.iter_mut().find(|k| k.0 == *name) {
                    k.1 = from.clone();
                }
            }
        }
        // taken: the game's controls on the inspect key, each to a free key
        if !inspect.is_empty() {
            let on: Vec<usize> = (0..keys.len()).filter(|&i| keys[i].1 == inspect).collect();
            for i in on {
                let Some(free) = FREE.iter().find(|f| **f != inspect && !keys.iter().any(|k| k.1 == **f)) else { break };
                let (name, label) = (keys[i].0.clone(), keys[i].2.clone());
                out.push(format!("{name}\t{free}"));
                self.cosm.notes_due.push((label, trn("Now on {key}: {inspect} inspects CS's items", &[("key", free.to_string()), ("inspect", inspect.clone())])));
                moved.push((name, inspect.clone(), free.to_string()));
                keys[i].1 = free.to_string();
            }
        }
        if moved != lo.moved {
            eprintln!("[cosmetics] the game's controls moved for the inspect key {inspect:?}: {}", out.join(", ").replace('\t', " to "));
            loadout::change(|l| l.moved = moved);
        }
        out.join("\n")
    }

    /// The notes of what was moved, once the HUD is seen (not while the world loads or the menu is over it): a moment
    /// after `t`, the world settled.
    pub(super) fn keys_notes(&mut self, t: f32) {
        if self.phase != super::Phase::Closed || self.cosm.notes_due.is_empty() {
            return;
        }
        for (title, line) in std::mem::take(&mut self.cosm.notes_due) {
            self.hud.note(title, line, t + 0.6);
        }
    }

    /// The inspect key's chip and the hand's, from `right` leftward on the chips' row (`y` its top): where they end.
    pub(super) fn keys_chips(&mut self, c: &mut Canvas, right: f32, y: f32) -> f32 {
        let r = self.r;
        let (lo, _) = loadout::current();
        let listening = self.cosm.key_listen;
        let key = if lo.inspect.is_empty() { tr("None").to_string() } else { lo.inspect.clone() };
        let chips = [
            (INSPECT_KEY, if listening { tr("Press a key").to_string() } else { format!("{} {}", tr("Inspect"), key) }),
            (HAND_SIDE, if lo.hand_game { tr("Hand as in the game") } else if lo.left { tr("Left hand") } else { tr("Right hand") }.to_string()),
        ];
        let mut x = right;
        for (k, (part, label)) in chips.iter().enumerate() {
            // (its width easing to its words', which cross over as they change)
            let w = fonts().width(Face::Medium, 0.74 * r, label) + 1.4 * r;
            let w = self.an(ak(A_HOV, 36_520 + k), w, 14.0, w);
            let b = [x - w, y, x, y + 1.9 * r];
            let hit = Hit::Cosm(*part, 0);
            let hv = self.an(ak(A_HOV, 36_500 + k), if self.hover == hit { 1.0 } else { 0.0 }, 14.0, 0.0);
            // waiting for its key: its light breathing
            let wait = self.an(ak(A_HOV, 36_510 + k), if listening && *part == INSPECT_KEY { 1.0 } else { 0.0 }, 10.0, 0.0);
            let breath = wait * (0.5 + 0.5 * (self.now * 4.0).sin());
            c.rect(b, 0.95 * r, mix(mix(pal().f1, pal().f3, hv), grey(1.0, 0.16), breath * 0.6));
            c.push_clip(b);
            self.cosm_words(c, 36_520 + k, b[0] + 0.7 * r, y + 1.28 * r, 0.74 * r, Face::Medium, label, mix(pal().t2, pal().t1, hv.max(wait)), false);
            c.pop_clip();
            self.hits.push((b, hit));
            x = b[0] - 0.45 * r;
        }
        x
    }

    /// The switch of all of it at the right of the chips' row (`right`, `y` its top): CS's things in the hands, or the
    /// game's own; where it ends.
    pub(super) fn keys_switch(&mut self, c: &mut Canvas, right: f32, y: f32) -> f32 {
        let r = self.r;
        let on = loadout::current().0.on;
        let label = if on { tr("On") } else { tr("Off") };
        let hit = Hit::Cosm(ALL_ON, 0);
        let hv = self.an(ak(A_HOV, 36_530), if self.hover == hit { 1.0 } else { 0.0 }, 14.0, 0.0);
        let kv = self.an(ak(A_HOV, 36_531), if on { 1.0 } else { 0.0 }, 12.0, on as i32 as f32);
        let tw = fonts().width(Face::Medium, 0.74 * r, label);
        let tw = self.an(ak(A_HOV, 36_532), tw, 14.0, tw);
        let sw = 2.0 * r;
        let b = [right - (tw + sw + 1.9 * r), y, right, y + 1.9 * r];
        c.rect(b, 0.95 * r, mix(pal().f1, pal().f3, hv));
        // its switch, lit when on
        let s = [b[0] + 0.7 * r, y + 0.5 * r, b[0] + 0.7 * r + sw, y + 1.4 * r];
        c.rect(s, 0.45 * r, mix(pal().f3, grey(1.0, 0.85), kv));
        let kx = s[0] + 0.45 * r + kv * (s[2] - s[0] - 0.9 * r);
        c.rect([kx - 0.33 * r, s[1] + 0.12 * r, kx + 0.33 * r, s[3] - 0.12 * r], 0.33 * r, mix(pal().t2, grey(0.08, 1.0), kv));
        c.push_clip(b);
        self.cosm_words(c, 36_532, s[2] + 0.5 * r, y + 1.28 * r, 0.74 * r, Face::Medium, label, mix(pal().t3, pal().t1, kv.max(hv * 0.6)), false);
        c.pop_clip();
        self.hits.push((b, hit));
        b[0] - 0.45 * r
    }

    /// A chip pressed: the inspect key waits for its new one; the hand changes side; all of it on or off.
    pub(super) fn keys_press(&mut self, part: u8) {
        if part == ALL_ON {
            let on = !loadout::current().0.on;
            loadout::change(|l| l.on = on);
            self.switch_sound(on, 0.6);
        } else if part == INSPECT_KEY {
            self.cosm.key_listen = !self.cosm.key_listen;
            self.sound_tick(0.5);
        } else {
            // as in the game, the right, the left, and round
            loadout::change(|l| {
                (l.hand_game, l.left) = match (l.hand_game, l.left) {
                    (true, _) => (false, false),
                    (false, false) => (false, true),
                    (false, true) => (true, false),
                };
            });
            self.switch_sound(true, 0.5);
        }
    }

    /// A key while the inspect key waits for its new one: it (Escape leaves it as it was, Backspace or Delete none).
    pub(super) fn keys_key(&mut self, escape: bool, backspace: bool, name: &str) -> bool {
        if !self.cosm.key_listen {
            return false;
        }
        self.cosm.key_listen = false;
        // the letter the game sends after the key is the inspect key's too: not typed in the search
        self.eat = Some(name.to_string());
        if !escape {
            let key = if backspace || name == "Delete" { String::new() } else { name.to_string() };
            loadout::change(|l| l.inspect = key);
            self.switch_sound(true, 0.5);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Gui, Kind};
    use super::*;

    /// The inspect key picked with G: the 'g' the game sends after it is not the search's; G again types.
    #[test]
    fn the_inspect_key_picked_types_nothing() {
        let was = loadout::current().0.inspect;
        let mut g = Gui::new();
        if let Some(s) = g.client.iter_mut().find(|s| s.label == "Sounds") {
            s.kind = Kind::Toggle(false);
        }
        g.keys_press(INSPECT_KEY);
        assert_eq!(g.key(false, false, false, "G".into(), false), 1);
        g.char_typed('g');
        assert!(g.q.is_empty(), "typed {:?}", g.q);
        assert_eq!(loadout::current().0.inspect, "G");
        assert!(!g.cosm.key_listen);
        g.bind_key_up("G");
        g.key(false, false, false, "G".into(), false);
        g.char_typed('g');
        assert_eq!(g.q, "g");
        loadout::change(|l| l.inspect = was);
    }
}
