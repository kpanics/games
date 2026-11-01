//! The second press confirms a reset, as it already does for a whole module section.
use super::{
    i18n::{self, tr},
    Gui,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Target {
    Module(&'static str),
    Element(&'static str),
    Theme,
    Interface,
}

impl Gui {
    pub(super) fn reset_armed(&self, target: Target) -> bool {
        self.reset_arm.is_some_and(|(old, at)| old == target && (0.0..3.0).contains(&(self.now - at)))
    }

    pub(super) fn confirm_reset(&mut self, target: Target) -> bool {
        self.section_arm = None;
        if self.reset_armed(target) {
            self.reset_arm = None;
            return true;
        }
        let name = match target {
            Target::Module(id) | Target::Element(id) => {
                let Some(m) = self.mods.iter().find(|m| m.id == id) else {
                    self.reset_arm = None;
                    return false;
                };
                if m.plain {
                    tr(m.name)
                } else {
                    i18n::nm(m.name)
                }
            }
            Target::Theme => tr("Theme"),
            Target::Interface => tr("Interface"),
        }
        .to_string();
        self.reset_arm = Some((target, self.now));
        self.toast(i18n::trn("{what}: press again to reset", &[("what", name)]));
        self.sound_tick(0.5);
        false
    }

    pub(super) fn request_module_reset(&mut self, i: usize) {
        let Some(m) = self.mods.get(i) else { return };
        if self.confirm_reset(Target::Module(m.id)) {
            self.reset_module(i);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::{binds::Bind, data::Kind, Owner, Phase};

    fn changed_module() -> (Gui, usize) {
        let mut g = Gui::new();
        let i = g.mods.iter().position(|m| m.id == "auto-sprint").unwrap();
        g.mods[i].on = true;
        g.mods[i].binds = vec![Bind::one("F6")];
        let mode = g.mods[i].set.iter_mut().find(|s| s.label == "Mode").unwrap();
        if let Kind::Choice { v, .. } = &mut mode.kind {
            *v = 0;
        }
        g.now = 1.0;
        (g, i)
    }

    #[test]
    fn reset_needs_two_presses_and_undo_keeps_the_switch_and_binds() {
        let (mut g, i) = changed_module();
        let before = g.dump();
        g.phase = Phase::Open;
        g.undo_tick();
        g.request_module_reset(i);
        assert_eq!(g.dump(), before);
        g.now += 0.2;
        g.request_module_reset(i);
        assert!(!g.mods[i].set.iter().any(|s| s.changed()));
        assert!(g.mods[i].on);
        assert_eq!(g.mods[i].binds[0].keys, ["F6"]);
        g.undo_go(true);
        assert_eq!(g.dump(), before);
    }

    #[test]
    fn confirmation_expires_and_cannot_move_to_another_target() {
        let (mut g, i) = changed_module();
        g.request_module_reset(i);
        g.now = 4.1;
        g.request_module_reset(i);
        assert!(g.mods[i].set.iter().any(|s| s.changed()));
        assert!(!g.confirm_reset(Target::Theme));
        g.request_module_reset(i);
        assert!(g.mods[i].set.iter().any(|s| s.changed()));
        g.close(true);
        g.request_module_reset(i);
        assert!(g.mods[i].set.iter().any(|s| s.changed()));
    }

    #[test]
    fn element_and_theme_actions_wait_for_confirmation() {
        let mut g = Gui::new();
        let i = g.custom_add("text");
        g.mods[i].on = false;
        let text = g.mods[i].set.iter_mut().find(|s| s.label == "Line 1").unwrap();
        if let Kind::Text(v) = &mut text.kind {
            *v = "Keep me".into();
        }
        let before = g.dump();
        g.action(Owner::Module(i), "Reset this element");
        assert_eq!(before, g.dump());
        g.now += 0.1;
        g.action(Owner::Module(i), "Reset this element");
        assert!(g.mods[i].on && !g.mods[i].set.iter().any(|s| s.changed()));
        let colour = g.theme.iter_mut().find(|s| matches!(s.kind, Kind::Colour(_))).unwrap();
        if let Kind::Colour(v) = &mut colour.kind {
            v.rgb ^= 0xabcdef;
        }
        g.action(Owner::Theme, "Reset the theme");
        assert!(g.theme.iter().any(|s| s.changed()));
        g.now += 0.1;
        g.action(Owner::Theme, "Reset the theme");
        assert!(!g.theme.iter().any(|s| s.changed()));
    }
}
