//! Defaults for an absent settings file only. Older, sparse configs keep their original fallback values.

use super::{Cat, Gui};

impl Gui {
    pub(super) fn first_run(&mut self) {
        // Disable the interface groups, keeping their layout ready for when the player enables a group.
        for module in &mut self.mods {
            if module.cat == Cat::Hud && !module.plain {
                module.on = false;
            }
        }
        self.tour_due = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::{binds::Bind, MODULE_CATS};
    use std::{fs, path::PathBuf};

    struct Room(PathBuf);
    impl Room {
        fn new() -> Self {
            let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
            let root = std::env::temp_dir().join(format!("recode-first-run-{}-{stamp}", std::process::id()));
            fs::create_dir(&root).unwrap();
            fs::create_dir(root.join("cursed-recode")).unwrap();
            Self(root)
        }

        fn gui(&self) -> Gui {
            let mut g = Gui::new();
            g.dir = Some(self.0.clone());
            g.tour_due = false;
            g
        }

        fn file(&self) -> PathBuf {
            self.0.join("cursed-recode/menu.txt")
        }
    }
    impl Drop for Room {
        fn drop(&mut self) {
            let root = self.0.canonicalize().unwrap();
            assert_eq!(root.parent().unwrap(), std::env::temp_dir().canonicalize().unwrap());
            assert!(root.file_name().unwrap().to_string_lossy().starts_with("recode-first-run-"));
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn first_launch_disables_interface_and_preserves_its_layout_for_later() {
        let room = Room::new();
        let mut g = room.gui();
        let elements: Vec<_> = g.mods.iter().filter(|m| m.cat == Cat::HudEl).map(|m| (m.id, m.on)).collect();
        g.ensure_loaded();
        assert!(g.loaded && g.tour_due);
        for m in &g.mods {
            if !m.plain && (MODULE_CATS.contains(&m.cat) || m.cat == Cat::Hud) {
                assert!(!m.on, "{} starts enabled", m.id);
            }
        }
        for (id, on) in elements {
            assert_eq!(g.mods[g.mod_index(id)].on, on, "{id}: keep the layout behind the disabled group");
        }
        g.save();
        let mut again = room.gui();
        again.ensure_loaded();
        assert!(!again.tour_due);
        assert_eq!(g.dump(), again.dump());
    }

    #[test]
    fn existing_sparse_configs_keep_legacy_defaults_and_explicit_switches() {
        let room = Room::new();
        fs::write(room.file(), "custom-chat.#on=0\ncustom-hotbar.#on=1\n").unwrap();
        let mut g = room.gui();
        let old: Vec<_> = g.mods.iter().filter(|m| m.cat == Cat::Hud).map(|m| (m.id, m.on)).collect();
        g.ensure_loaded();
        assert!(!g.tour_due);
        for (id, on) in old {
            assert_eq!(g.mods[g.mod_index(id)].on, if id == "custom-chat" { false } else { on }, "{id}");
        }
        assert!(g.mods[g.mod_index("custom-hotbar")].on);
    }

    #[test]
    fn user_choices_survive_the_next_launch() {
        let room = Room::new();
        let mut g = room.gui();
        g.ensure_loaded();
        let hud = g.mod_index("hud");
        g.mods[hud].on = true;
        g.mods[hud].binds = vec![Bind::one("F7")];
        let chat = g.mod_index("custom-chat");
        g.mods[chat].on = true;
        g.save();
        let mut again = room.gui();
        again.ensure_loaded();
        assert!(!again.tour_due);
        assert_eq!(g.dump(), again.dump());
        assert!(again.mods[hud].on && again.mods[chat].on);
        assert!(!again.mods[again.mod_index("custom-hotbar")].on);
        assert_eq!(again.mods[hud].binds[0].keys, ["F7"]);
    }

    #[test]
    fn unreadable_existing_settings_are_not_treated_as_a_new_install() {
        let room = Room::new();
        fs::write(room.file(), [0xff, 0xfe, 0xfd]).unwrap();
        let mut g = room.gui();
        let before = g.dump();
        g.load();
        assert!(!g.tour_due);
        assert_eq!(g.dump(), before);
        assert_eq!(fs::read(room.file()).unwrap(), [0xff, 0xfe, 0xfd]);
    }
}
