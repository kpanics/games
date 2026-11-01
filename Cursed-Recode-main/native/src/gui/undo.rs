//! Ctrl+Z and Ctrl+Y (Ctrl+Shift+Z too) in the menu: what was changed taken back, and done again. A step is what one
//! action changed of the settings' lines — a press let go, a key, a turn of the wheel, a field left — each line's old
//! and new; a slider's drag is one step, a word typed into a field one too. Only what the step changed goes back: what
//! the game changed meanwhile (a bind in the world) stays.

use std::collections::HashMap;

use super::i18n::{self, tr};
use super::{configs, Gui, Phase};

/// The steps kept back (and ahead).
const KEEP: usize = 200;
/// How long the input rests before what it did is a step (the wheel's turns and a quick double press are one).
const SETTLE: f32 = 0.3;

/// A line of the settings: its key, what it was, what it is (none: it was not, or is gone).
type Line = (String, Option<String>, Option<String>);

#[derive(Default)]
pub(super) struct Undo {
    back: Vec<Vec<Line>>,
    ahead: Vec<Vec<Line>>,
    /// The settings as the last step left them (none: not read since the menu opened).
    seen: Option<HashMap<String, String>>,
    /// When the input last came (none: nothing since the last step).
    poked: Option<f32>,
    /// A mouse button is held (a drag: its step is when it is let go).
    held: bool,
}

/// The settings' lines as a map.
fn lines(dump: &str) -> HashMap<String, String> {
    dump.lines().filter_map(|l| l.split_once('=')).map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

/// What changed from `a` to `b`, by the key (in its order: the same step reads the same each time).
fn diff(a: &HashMap<String, String>, b: &HashMap<String, String>) -> Vec<Line> {
    let mut out: Vec<Line> = b.iter().filter(|(k, v)| a.get(*k) != Some(v)).map(|(k, v)| (k.clone(), a.get(k).cloned(), Some(v.clone()))).collect();
    out.extend(a.iter().filter(|(k, _)| !b.contains_key(*k)).map(|(k, v)| (k.clone(), Some(v.clone()), None)));
    out.sort_by(|x, y| x.0.cmp(&y.0));
    out
}

impl Gui {
    /// Input came (a press, a release, a key, the wheel, a letter): what it changes is a step once it rests.
    pub(super) fn undo_poke(&mut self, held: Option<bool>) {
        self.undo.poked = Some(self.now);
        if let Some(h) = held {
            self.undo.held = h;
        }
    }

    /// Each frame (the menu's and the HUD's): the settings read as the menu or the HUD's layout editor opens (what
    /// changed while both were shut is no step), and a step made when the input has rested and nothing is held or typed
    /// into.
    pub(super) fn undo_tick(&mut self) {
        if self.phase == Phase::Closed && !self.hud.editing {
            self.undo.seen = None;
            self.undo.poked = None;
            self.undo.held = false;
            return;
        }
        if self.undo.seen.is_none() {
            self.undo.seen = Some(lines(&self.dump()));
            return;
        }
        let busy = self.undo.held || self.slider.is_some() || self.text_focus.is_some() || self.listen.is_some();
        if self.undo.poked.is_some_and(|t| self.now - t >= SETTLE) && !busy {
            self.undo_mark();
        }
    }

    /// What changed since the last step, a step of its own.
    fn undo_mark(&mut self) {
        self.undo.poked = None;
        let now = lines(&self.dump());
        let Some(seen) = self.undo.seen.as_ref() else {
            self.undo.seen = Some(now);
            return;
        };
        let step = diff(seen, &now);
        self.undo.seen = Some(now);
        if step.is_empty() {
            return;
        }
        self.undo.back.push(step);
        if self.undo.back.len() > KEEP {
            self.undo.back.remove(0);
        }
        self.undo.ahead.clear();
    }

    /// Ctrl+Z (`back`) or Ctrl+Y: the last step taken back, or the one taken back done again; a word says which.
    pub(super) fn undo_go(&mut self, back: bool) {
        if self.undo.seen.is_none() {
            return;
        }
        // what was just done (and has not rested yet) is a step first
        self.undo_mark();
        let from = if back { &mut self.undo.back } else { &mut self.undo.ahead };
        let Some(step) = from.pop() else {
            self.toast(tr(if back { "Nothing to undo" } else { "Nothing to redo" }).into());
            return;
        };
        let mut vals = self.undo.seen.clone().unwrap_or_default();
        let mut parts = 0;
        for (k, old, new) in &step {
            match if back { old } else { new } {
                Some(v) => vals.insert(k.clone(), v.clone()),
                None => vals.remove(k),
            };
            parts |= self.part_of(k);
        }
        // (the lines of binds are read with their owner's part)
        if parts & configs::P_BINDS != 0 {
            parts |= configs::P_BINDS;
        }
        self.apply_vals(&vals, parts);
        self.undo.seen = Some(lines(&self.dump()));
        let what = self.undo_what(&step);
        if back {
            self.undo.ahead.push(step);
        } else {
            self.undo.back.push(step);
        }
        self.sound_tick(0.55);
        let t = if back { "Undone: {what}" } else { "Redone: {what}" };
        self.toast(i18n::trn(t, &[("what", what)]));
    }

    /// What a step changed, in a word: the module's name (or the menu's part) when it is one, else how many.
    fn undo_what(&self, step: &[Line]) -> String {
        let name = |k: &str| -> String {
            // an element's place: the element's name
            if let Some(m) = k.strip_prefix("hud.").and_then(|r| r.strip_suffix(".place")).and_then(|id| self.mods.iter().find(|m| m.id == id)) {
                return tr(m.name).into();
            }
            match k.split('.').next().unwrap_or("") {
                "theme" | "text" => tr("Theme").into(),
                "menu" => tr("Menu").into(),
                "hud" | "hudx" => tr("Interface").into(),
                id => self.mods.iter().find(|m| m.id == id).map_or_else(|| id.to_string(), |m| tr(m.name).into()),
            }
        };
        let mut names: Vec<String> = step.iter().map(|l| name(&l.0)).collect();
        names.sort();
        names.dedup();
        match names.as_slice() {
            [one] => one.clone(),
            _ => i18n::trn("{n} settings", &[("n", step.len().to_string())]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_step_is_what_changed() {
        let a = lines("m.A=1\nm.B=2\nm.C=3\n");
        let b = lines("m.A=1\nm.B=5\nm.D=4\n");
        assert_eq!(
            diff(&a, &b),
            vec![("m.B".into(), Some("2".into()), Some("5".into())), ("m.C".into(), Some("3".into()), None), ("m.D".into(), None, Some("4".into()))]
        );
        assert!(diff(&a, &a).is_empty());
    }

    /// A module switched, a slider moved: Ctrl+Z takes back the slider, then the switch; Ctrl+Y does them again; what
    /// changed while the menu was shut stays.
    #[test]
    fn ctrl_z_takes_back_and_ctrl_y_does_again() {
        let mut g = Gui::new();
        g.phase = Phase::Open;
        let i = g.mods.iter().position(|m| !m.plain && m.set.iter().any(|s| matches!(s.kind, super::super::data::Kind::Slider { .. }))).unwrap();
        let si = g.mods[i].set.iter().position(|s| matches!(s.kind, super::super::data::Kind::Slider { .. })).unwrap();
        let slider = |g: &Gui| match g.mods[i].set[si].kind {
            super::super::data::Kind::Slider { v, .. } => v,
            _ => unreachable!(),
        };
        let rest = |g: &mut Gui| {
            g.now += 1.0;
            g.undo_tick();
        };
        g.undo_tick();
        let (on0, v0) = (g.mods[i].on, slider(&g));
        g.undo_poke(None);
        g.mods[i].on = !on0;
        rest(&mut g);
        g.undo_poke(None);
        if let super::super::data::Kind::Slider { v, min, max, .. } = &mut g.mods[i].set[si].kind {
            *v = if *v == *max { *min } else { *max };
        }
        rest(&mut g);
        let v1 = slider(&g);
        assert_ne!(v1, v0);
        g.bind_key_down("Left Ctrl");
        assert_eq!(g.key(false, false, false, "Z".into(), false), 1);
        assert_eq!((g.mods[i].on, slider(&g)), (!on0, v0));
        g.key(false, false, false, "Z".into(), false);
        assert_eq!((g.mods[i].on, slider(&g)), (on0, v0));
        g.key(false, false, false, "Y".into(), false);
        assert_eq!((g.mods[i].on, slider(&g)), (!on0, v0));
        g.bind_key_down("Left Shift");
        g.key(false, false, false, "Z".into(), false);
        assert_eq!((g.mods[i].on, slider(&g)), (!on0, v1));
        // shut, switched by a bind in the world, opened: Ctrl+Z takes back the slider only
        g.phase = Phase::Closed;
        g.undo_tick();
        g.mods[i].on = on0;
        g.phase = Phase::Open;
        g.undo_tick();
        g.gheld.retain(|k| k != "Left Shift");
        g.key(false, false, false, "Z".into(), false);
        assert_eq!((g.mods[i].on, slider(&g)), (on0, v0));
    }
}
