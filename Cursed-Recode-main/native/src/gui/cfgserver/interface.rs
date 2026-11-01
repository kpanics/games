//! A saved interface follows Server Adaptation above the server's ordinary config.
use super::*;
use crate::gui::{
    adapt::MODES,
    configs::{P_HUD, P_MENU, P_THEME},
};

pub(in crate::gui) const INTERFACE_PARTS: u8 = P_THEME | P_MENU | P_HUD;

pub(in crate::gui) fn write_config(file: &std::path::Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let parent = file.parent().ok_or_else(|| std::io::Error::other("Config has no folder"))?;
    let temp = parent.join(format!(".recode-config-{}-{}.tmp", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    let mut stream = std::fs::OpenOptions::new().write(true).create_new(true).open(&temp)?;
    let wrote = stream.write_all(text.as_bytes());
    drop(stream);
    let result = wrote.and_then(|_| std::fs::rename(&temp, file));
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

pub(in crate::gui) fn write_configs(changes: &[(std::path::PathBuf, String, String)]) -> std::io::Result<()> {
    for (at, (path, _, text)) in changes.iter().enumerate() {
        if let Err(error) = write_config(path, text) {
            for (path, old, _) in changes[..at].iter().rev() {
                let _ = write_config(path, old);
            }
            return Err(error);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::{configs::P_ALL, data::Kind, theme};
    fn isolated(name: &str) -> bool {
        if std::env::var("CURSED_INTERFACE_TEST_CHILD").ok().as_deref() == Some(name) {
            return false;
        }
        let test = format!("gui::cfgserver::interface::tests::{name}");
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", &test, "--nocapture", "--test-threads=1"])
            .env("CURSED_INTERFACE_TEST_CHILD", name)
            .env_remove("CURSED_DEV_HOST")
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        true
    }
    struct Room(std::path::PathBuf);
    impl Room {
        fn new() -> Self {
            let time = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
            let path = std::env::temp_dir().join(format!("recode-ui-profile-{}-{time}", std::process::id()));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Room {
        fn drop(&mut self) {
            let root = self.0.canonicalize().unwrap();
            assert_eq!(root.parent().unwrap(), std::env::temp_dir().canonicalize().unwrap());
            std::fs::remove_dir_all(root).unwrap();
        }
    }
    fn color(g: &mut Gui, rgb: u32) {
        if let Kind::Colour(c) = &mut g.theme.iter_mut().find(|s| s.label == "Accent").unwrap().kind {
            c.rgb = rgb;
            c.auto = false;
        }
    }
    fn mode(g: &mut Gui, value: usize) {
        let i = g.mod_index("server-adaptation");
        g.mods[i].on = true;
        for s in &mut g.mods[i].set {
            match (&s.label, &mut s.kind) {
                (&"Mode", Kind::Choice { v, .. }) => *v = value,
                (&"By the server", Kind::Toggle(v)) => *v = false,
                _ => {}
            }
        }
    }
    fn index(g: &Gui, name: &str) -> usize {
        g.saved.iter().position(|s| s.name == name).unwrap()
    }
    fn save(g: &mut Gui, name: &str, rgb: u32) {
        color(g, rgb);
        g.cfg_write(name, P_ALL, None);
    }
    fn rgb(g: &Gui) -> u32 {
        theme::col(&g.theme, "Accent").rgb
    }
    fn persisted(g: &Gui, name: &str) -> String {
        std::fs::read_to_string(&g.saved[index(g, name)].file).unwrap()
    }

    #[test]
    fn modes_follow_their_own_interface_and_return_to_the_private_settings() {
        if isolated("modes_follow_their_own_interface_and_return_to_the_private_settings") {
            return;
        }
        let room = Room::new();
        let mut g = Gui::new();
        g.dir = Some(room.0.clone());
        mode(&mut g, 0);
        let sprint = g.mod_index("auto-sprint");
        g.mods[sprint].on = true;
        save(&mut g, "RW", 0xaa1100);
        save(&mut g, "HW", 0x11aa00);
        save(&mut g, "FT", 0x0011aa);
        g.mods[sprint].on = false;
        color(&mut g, 0x123456);
        g.loaded_cfg = Some("Own".into());
        for (m, name) in MODES.iter().enumerate() {
            let short = match *name {
                "ReallyWorld" => "RW",
                "HolyWorld" => "HW",
                _ => "FT",
            };
            let i = index(&g, short);
            g.cfg_interface(i, m);
        }
        g.hud.facts.server = "copy.test".into();
        g.profile_follow();
        assert_eq!(rgb(&g), 0xaa1100);
        assert!(!g.mods[sprint].on);
        assert_eq!(g.own_config_name().as_deref(), Some("Own"));
        color(&mut g, 0xbb2200);
        g.save();
        assert!(g.own_dump().contains("theme.Accent=123456/"));
        mode(&mut g, 1);
        g.profile_follow();
        assert_eq!(rgb(&g), 0x11aa00);
        assert_eq!(g.adapt_mode(), Some(1));
        color(&mut g, 0x22bb00);
        g.save();
        mode(&mut g, 0);
        g.profile_follow();
        assert_eq!(rgb(&g), 0xbb2200);
        g.hud.facts.server.clear();
        g.profile_follow();
        assert_eq!(rgb(&g), 0x123456);
        assert!(persisted(&g, "HW").contains("theme.Accent=22bb00/"));
        assert!(persisted(&g, "RW").contains("#ui-modes=ReallyWorld"));
    }

    #[test]
    fn interface_does_not_leak_into_the_server_profile_or_private_file() {
        if isolated("interface_does_not_leak_into_the_server_profile_or_private_file") {
            return;
        }
        let room = Room::new();
        let mut g = Gui::new();
        g.dir = Some(room.0.clone());
        mode(&mut g, 0);
        save(&mut g, "Look", 0xff6600);
        let look = index(&g, "Look");
        g.cfg_interface(look, 0);
        let sprint = g.mod_index("auto-sprint");
        g.mods[sprint].on = true;
        save(&mut g, "Server", 0x112200);
        let server = index(&g, "Server");
        g.cfg_server(server, "copy.test", true);
        g.mods[sprint].on = false;
        color(&mut g, 0x123456);
        g.hud.facts.server = "copy.test".into();
        g.profile_follow();
        assert_eq!(rgb(&g), 0xff6600);
        assert!(g.mods[sprint].on);
        color(&mut g, 0xff7700);
        g.save();
        assert!(persisted(&g, "Look").contains("theme.Accent=ff7700/"));
        assert!(persisted(&g, "Server").contains("theme.Accent=112200/"));
        assert!(g.own_dump().contains("theme.Accent=123456/"));
        g.hud.facts.server.clear();
        g.profile_follow();
        assert_eq!(rgb(&g), 0x123456);
        assert!(!g.mods[sprint].on);
    }

    #[test]
    fn same_file_has_one_owner_and_manual_load_stays_until_context_changes() {
        if isolated("same_file_has_one_owner_and_manual_load_stays_until_context_changes") {
            return;
        }
        let room = Room::new();
        let mut g = Gui::new();
        g.dir = Some(room.0.clone());
        mode(&mut g, 0);
        save(&mut g, "Both", 0xff6600);
        let i = index(&g, "Both");
        g.cfg_interface(i, 0);
        let i = index(&g, "Both");
        g.cfg_server(i, "copy.test", true);
        color(&mut g, 0x123456);
        g.hud.facts.server = "copy.test".into();
        g.profile_follow();
        assert!(g.profile.is_some());
        assert!(g.interface_profile.is_none());
        color(&mut g, 0xff7700);
        g.save();
        g.hud.facts.server.clear();
        g.profile_follow();
        assert_eq!(rgb(&g), 0x123456);
        assert!(persisted(&g, "Both").contains("theme.Accent=ff7700/"));
        g.hud.facts.server = "copy.test".into();
        g.profile_follow();
        g.cfg_load("#parts=4\ntheme.Accent=abcdef/1/0/0\n", P_THEME, "Manual");
        g.profile_follow();
        assert_eq!(rgb(&g), 0xabcdef);
        mode(&mut g, 1);
        g.profile_follow();
        assert_eq!(rgb(&g), 0xabcdef);
        mode(&mut g, 0);
        g.profile_follow();
        assert_eq!(rgb(&g), 0xff7700);
        g.hud.facts.server.clear();
        g.profile_follow();
        assert_eq!(rgb(&g), 0xabcdef);
    }

    #[test]
    fn mode_metadata_preserves_other_headers_and_unknown_values() {
        let raw = "#name=A\r\n#parts=31\r\n#servers=one.test\r\nfuture.setting=yes\r\n";
        let saved = with_modes(raw, &[2, 0, 2, 99]);
        assert_eq!(modes_of(&saved), [0, 2]);
        assert!(saved.contains("#servers=one.test\r\n"));
        assert_eq!(with_modes(&saved, &[]), raw);
        let g = Gui::new();
        let merged = g.profile_merge(&saved, &g.dump(), P_THEME);
        assert!(merged.contains("future.setting=yes\r\n"));
        assert_eq!(modes_of(&merged), [0, 2]);
        let merged = g.profile_merge(
            "theme.Accent=old\ntheme.Accent=duplicate\ntext.Name=removed\ntheme.Future=keep\n",
            "theme.Accent=new\n",
            P_THEME,
        );
        assert_eq!(merged.matches("theme.Accent=new").count(), 2);
        assert!(!merged.contains("text.Name="));
        assert!(merged.contains("theme.Future=keep"));
    }

    #[test]
    fn active_profile_can_be_renamed_or_deleted_without_resurrecting_old_files() {
        if isolated("active_profile_can_be_renamed_or_deleted_without_resurrecting_old_files") {
            return;
        }
        let room = Room::new();
        let mut g = Gui::new();
        g.dir = Some(room.0.clone());
        mode(&mut g, 0);
        save(&mut g, "Look", 0xff6600);
        let i = index(&g, "Look");
        g.cfg_interface(i, 0);
        color(&mut g, 0x123456);
        g.hud.facts.server = "copy.test".into();
        g.profile_follow();
        color(&mut g, 0xff7700);
        g.save();
        g.cfg_scan();
        let i = index(&g, "Look");
        let old = g.saved[i].file.clone();
        assert!(g.cfg_write("Renamed", P_ALL, Some(i)));
        assert!(!old.exists());
        assert_eq!(rgb(&g), 0xff7700);
        let i = index(&g, "Renamed");
        let file = g.saved[i].file.clone();
        assert!(g.interface_uses(&file));
        assert_eq!(modes_of(&persisted(&g, "Renamed")), [0]);
        assert!(g.cfg_write("renamed", P_ALL, Some(i)));
        assert_eq!(g.saved[index(&g, "renamed")].file, file);
        assert!(g.cfg_delete(&file));
        assert_eq!(rgb(&g), 0x123456);
        g.save();
        assert!(!file.exists());
        assert!(!old.exists());
    }

    #[test]
    fn rebinding_and_partial_overwrite_restore_the_right_baseline() {
        if isolated("rebinding_and_partial_overwrite_restore_the_right_baseline") {
            return;
        }
        let room = Room::new();
        let mut g = Gui::new();
        g.dir = Some(room.0.clone());
        mode(&mut g, 0);
        save(&mut g, "First", 0xff6600);
        save(&mut g, "Second", 0x00ff66);
        let i = index(&g, "First");
        g.cfg_interface(i, 0);
        color(&mut g, 0x123456);
        g.hud.facts.server = "copy.test".into();
        g.profile_follow();
        assert_eq!(rgb(&g), 0xff6600);
        let i = index(&g, "Second");
        g.cfg_interface(i, 0);
        assert_eq!(rgb(&g), 0x00ff66);
        assert!(modes_of(&persisted(&g, "First")).is_empty());
        let i = index(&g, "Second");
        assert!(g.cfg_write("Second", crate::gui::configs::P_MODULES, Some(i)));
        assert!(modes_of(&persisted(&g, "Second")).is_empty());
        assert!(g.interface_profile.is_none());
        assert_eq!(rgb(&g), 0x123456);
        let first = persisted(&g, "First");
        let second = persisted(&g, "Second");
        let i = index(&g, "First");
        assert!(!g.cfg_write("Second", P_ALL, Some(i)));
        assert_eq!(persisted(&g, "First"), first);
        assert_eq!(persisted(&g, "Second"), second);
    }

    #[test]
    fn failed_binding_write_keeps_both_original_files() {
        if isolated("failed_binding_write_keeps_both_original_files") {
            return;
        }
        let room = Room::new();
        let mut g = Gui::new();
        g.dir = Some(room.0.clone());
        save(&mut g, "First", 0xff6600);
        save(&mut g, "Second", 0x00ff66);
        let i = index(&g, "First");
        g.cfg_interface(i, 0);
        let i = index(&g, "First");
        g.cfg_server(i, "copy.test", true);
        let first = persisted(&g, "First");
        let second = persisted(&g, "Second");
        let i = index(&g, "Second");
        struct Restore(std::path::PathBuf, std::fs::Permissions);
        impl Drop for Restore {
            fn drop(&mut self) {
                std::fs::set_permissions(&self.0, self.1.clone()).unwrap();
            }
        }
        let file = g.saved[i].file.clone();
        let permissions = std::fs::metadata(&file).unwrap().permissions();
        let _restore = Restore(file.clone(), permissions.clone());
        let mut readonly = permissions;
        readonly.set_readonly(true);
        std::fs::set_permissions(&file, readonly).unwrap();
        g.cfg_interface(i, 0);
        let i = index(&g, "Second");
        g.cfg_server(i, "copy.test", true);
        assert_eq!(persisted(&g, "First"), first);
        assert_eq!(persisted(&g, "Second"), second);
        assert!(std::fs::read_dir(file.parent().unwrap())
            .unwrap()
            .all(|e| !e.unwrap().file_name().to_string_lossy().ends_with(".tmp")));
    }

    #[test]
    fn custom_hud_settings_and_bindings_survive_a_different_layout() {
        if isolated("custom_hud_settings_and_bindings_survive_a_different_layout") {
            return;
        }
        use crate::gui::binds::Bind;
        let room = Room::new();
        let mut g = Gui::new();
        g.dir = Some(room.0.clone());
        mode(&mut g, 0);
        g.custom_ensure("x-text-2|text");
        save(&mut g, "Look", 0xff6600);
        let i = index(&g, "Look");
        g.cfg_interface(i, 0);
        g.custom_ensure("x-text-1|text");
        let i = g.mod_index("x-text-1");
        g.mods[i].binds = vec![Bind::one("K")];
        let line = g.mods[i].set.iter_mut().find(|s| s.label == "Line 1").unwrap();
        line.kind = Kind::Text("My own HUD".into());
        line.binds = vec![Bind::one("L")];
        let sprint = g.mod_index("auto-sprint");
        g.mods[sprint].binds = vec![Bind::one("P")];
        g.hud.facts.server = "copy.test".into();
        g.profile_follow();
        assert_eq!(g.custom_list(), "x-text-2|text");
        g.mods[sprint].binds = vec![Bind::one("J")];
        let own = g.own_dump();
        assert!(own.contains("x-text-1.Line 1=My own HUD"));
        assert!(own.contains("x-text-1.#binds=K"));
        assert!(own.contains("x-text-1.Line 1#binds=L"));
        assert!(!own.contains("x-text-2.#binds="));
        assert!(!own.contains("x-text-2.Line 1="));
        g.hud.facts.server.clear();
        g.profile_follow();
        assert_eq!(g.custom_list(), "x-text-1|text");
        let i = g.mod_index("x-text-1");
        assert_eq!(g.mods[i].binds[0].keys, ["K"]);
        let line = g.mods[i].set.iter().find(|s| s.label == "Line 1").unwrap();
        assert!(matches!(&line.kind,Kind::Text(v) if v=="My own HUD"));
        assert_eq!(line.binds[0].keys, ["L"]);
        assert_eq!(g.mods[sprint].binds[0].keys, ["J"]);
    }

    #[test]
    fn interface_card_fits_four_languages_and_small_windows() {
        if isolated("interface_card_fits_four_languages_and_small_windows") {
            return;
        }
        use crate::gui::{
            draw::Canvas,
            text::{fonts, Face},
            Card, Hit,
        };
        let mut dynamic = Vec::new();
        for lang in ["ru_ru", "en_us", "uk_ua", "kk_kz"] {
            crate::menus::lang::set(lang);
            let letters: Vec<char> = "ӘәІіҢңҒғҮүҰұҚқӨөҺһ".chars().collect();
            for face in [Face::Regular, Face::Medium, Face::Bold, Face::Semi] {
                for &ch in &letters {
                    fonts().glyph(face, ch);
                }
            }
            for _ in 0..400 {
                if ![Face::Regular, Face::Medium, Face::Bold, Face::Semi]
                    .iter()
                    .any(|&face| letters.iter().any(|&ch| crate::gui::dynfont::waiting(face, ch)))
                {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            for (w, h, r) in [(960.0, 540.0, 12.0), (1600.0, 900.0, 18.0)] {
                let room = Room::new();
                let mut g = Gui::new();
                g.dir = Some(room.0.clone());
                g.w = w;
                g.h = h;
                g.r = r;
                g.dt = 0.1;
                g.now = 10.0;
                save(&mut g, "Night", 0x78a8ff);
                let i = index(&g, "Night");
                g.cfg_interface(i, 0);
                let i = index(&g, "Night");
                g.cfg_host = "mc.example.test".into();
                g.cfg_parts = P_ALL;
                g.card_field = "Night".into();
                g.card = Some(Card::Cfg {
                    i,
                    at: [w * 0.4, h * 0.4, w * 0.4, h * 0.4],
                });
                let mut c = Canvas::new(w, h);
                for _ in 0..20 {
                    g.now += 0.1;
                    g.hits.clear();
                    c.p.clear();
                    g.cfg_card(&mut c);
                }
                let b = g.card_rect;
                assert!(b[0] >= 0.0 && b[1] >= 0.0 && b[2] <= w && b[3] <= h, "{lang}: {b:?}");
                for (rect, hit) in &g.hits {
                    if matches!(hit, Hit::Card(..)) {
                        assert!(
                            rect[0] >= b[0] && rect[1] >= b[1] && rect[2] <= b[2] && rect[3] <= b[3],
                            "{lang}: {hit:?} {rect:?} {b:?}"
                        );
                    }
                }
                assert_eq!(g.hits.iter().filter(|(_, hit)| matches!(hit, Hit::Card(52, _))).count(), 3);
                if w == 1600.0 {
                    if let Some(path) = std::env::var_os("CURSED_TEST_CARD_DUMP") {
                        let path = std::path::PathBuf::from(path);
                        std::fs::create_dir_all(&path).unwrap();
                        dynamic.extend(crate::gui::dynfont::drain());
                        std::fs::write(
                            path.join(format!("interface-{lang}.json")),
                            serde_json::to_vec(&serde_json::json!({"w":w,"h":h,"shapes":c.p,"dynamic":dynamic})).unwrap(),
                        )
                        .unwrap();
                    }
                }
            }
        }
    }
}

pub(in crate::gui) struct InterfaceProfile {
    pub(super) mode: usize,
    pub(super) file: std::path::PathBuf,
    pub(super) parts: u8,
    pub(super) before: HashMap<String, String>,
}

pub(in crate::gui) fn modes_of(text: &str) -> Vec<usize> {
    text.lines().find_map(|l| l.strip_prefix("#ui-modes=")).map_or(Vec::new(), |s| {
        MODES
            .iter()
            .enumerate()
            .filter(|(_, name)| s.split(',').any(|part| part.trim().eq_ignore_ascii_case(name)))
            .map(|(i, _)| i)
            .collect()
    })
}

pub(in crate::gui) fn with_modes(text: &str, modes: &[usize]) -> String {
    let nl = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut rows: Vec<&str> = text.lines().filter(|l| !l.starts_with("#ui-modes=")).collect();
    let names: Vec<&str> = MODES.iter().enumerate().filter(|(i, _)| modes.contains(i)).map(|(_, name)| *name).collect();
    let header = format!("#ui-modes={}", names.join(","));
    if !names.is_empty() {
        let at = rows.iter().take_while(|l| l.starts_with('#')).count();
        rows.insert(at, &header);
    }
    rows.join(nl) + nl
}

impl Gui {
    pub(in crate::gui) fn interface_uses(&self, file: &std::path::Path) -> bool {
        self.interface_profile
            .as_ref()
            .is_some_and(|p| p.file == file && self.adapt_mode() == Some(p.mode))
    }
    /// Restore just one layer in a dump, retaining everything else.
    pub(super) fn profile_rewind(&self, dump: &str, before: &HashMap<String, String>, parts: u8) -> String {
        fn customs(values: &HashMap<String, String>) -> std::collections::HashSet<&str> {
            values
                .get("hudx.list")
                .map(String::as_str)
                .unwrap_or("")
                .split(',')
                .filter_map(|v| v.split_once('|').map(|(id, _)| id))
                .collect()
        }
        let current = lines(dump);
        let had = customs(before);
        let now = customs(&current);
        let custom_bind = |key: &str| parts & P_HUD != 0 && key.starts_with("x-") && self.part_of(key) == crate::gui::configs::P_BINDS;
        let mut values: Vec<(String, String)> = current
            .iter()
            .filter(|(k, _)| self.part_of(k) & parts == 0 && !(custom_bind(k) && !had.contains(k.split('.').next().unwrap_or(""))))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        values.extend(
            before
                .iter()
                .filter(|(k, _)| {
                    self.part_of(k) & parts != 0
                        || (custom_bind(k) && had.contains(k.split('.').next().unwrap_or("")) && !now.contains(k.split('.').next().unwrap_or("")))
                })
                .map(|(k, v)| (k.clone(), v.clone())),
        );
        values.sort();
        values.into_iter().map(|(k, v)| format!("{k}={v}\n")).collect()
    }

    pub(super) fn profile_restore(&mut self, before: &HashMap<String, String>, parts: u8) {
        let restored = lines(&self.profile_rewind(&self.dump(), before, parts));
        // HUD elements can disappear in another layout. Restore their bindings as well;
        // all surviving elements and ordinary modules keep their current bindings.
        let take = parts | if parts & P_HUD != 0 { crate::gui::configs::P_BINDS } else { 0 };
        self.apply_vals(&restored, take);
    }

    pub(super) fn without_interface(&self) -> String {
        let dump = self.dump();
        self.interface_profile
            .as_ref()
            .map_or_else(|| dump.clone(), |p| self.profile_rewind(&dump, &p.before, p.parts))
    }

    /// Update only the owned parts; preserve other settings, unknown fields and binding metadata.
    pub(super) fn profile_merge(&self, old: &str, dump: &str, parts: u8) -> String {
        let values = lines(dump);
        let mut seen = std::collections::HashSet::new();
        let nl = if old.contains("\r\n") { "\r\n" } else { "\n" };
        let mut rows = Vec::new();
        for row in old.lines() {
            if let Some((key, _)) = row.split_once('=') {
                if self.part_of(key) & parts != 0 {
                    if let Some(value) = values.get(key) {
                        rows.push(format!("{key}={value}"));
                        seen.insert(key);
                        continue;
                    }
                    if key.starts_with("text.") {
                        continue;
                    }
                }
            }
            rows.push(row.to_string());
        }
        let mut extra: Vec<_> = values
            .into_iter()
            .filter(|(k, _)| self.part_of(k) & parts != 0 && !seen.contains(k.as_str()))
            .collect();
        extra.sort();
        rows.extend(extra.into_iter().map(|(k, v)| format!("{k}={v}")));
        rows.join(nl) + nl
    }

    pub(super) fn interface_keep(&self) {
        let Some(p) = &self.interface_profile else { return };
        let Ok(old) = std::fs::read_to_string(&p.file) else { return };
        let text = self.profile_merge(&old, &self.dump(), p.parts);
        if text != old {
            let _ = write_config(&p.file, &text);
        }
    }

    pub(super) fn interface_leave(&mut self, back: bool) {
        self.interface_keep();
        if let Some(p) = self.interface_profile.take() {
            if back {
                self.profile_restore(&p.before, p.parts);
            }
        }
    }

    pub(super) fn interface_follow(&mut self) {
        let mode = if self.cfg_host.is_empty() { None } else { self.adapt_mode() };
        let context = (self.cfg_host.clone(), mode);
        if self.cfg_interface_context.as_ref() == Some(&context) {
            return;
        }
        self.cfg_interface_context = Some(context);
        self.interface_leave(true);
        let Some(mode) = mode else { return };
        self.cfg_scan();
        let Some(saved) = self.saved.iter().find(|s| modes_of(&s.text).contains(&mode)).cloned() else {
            return;
        };
        // The ordinary server profile already owns these same settings in this file.
        if self.profile.as_ref().is_some_and(|p| p.file == saved.file) {
            return;
        }
        let (values, _, have) = parse(&saved.text);
        let parts = have & INTERFACE_PARTS;
        if parts == 0 {
            return;
        }
        let before = lines(&self.dump());
        self.apply_vals(&values, parts);
        self.interface_profile = Some(InterfaceProfile {
            mode,
            file: saved.file,
            parts,
            before,
        });
        self.profile_say(MODES[mode].into(), i18n::trn("Interface from {name}", &[("name", saved.name)]));
    }

    pub(in crate::gui) fn cfg_interface(&mut self, index: usize, mode: usize) {
        if mode >= MODES.len() {
            return;
        }
        let Some(file) = self.saved.get(index).map(|s| s.file.clone()) else { return };
        self.profile_keep();
        self.cfg_scan();
        let Some(saved) = self.saved.iter().find(|s| s.file == file).cloned() else {
            return;
        };
        if saved.parts & INTERFACE_PARTS == 0 {
            return;
        }
        let mut modes = modes_of(&saved.text);
        let give = !modes.contains(&mode);
        modes.retain(|m| *m != mode);
        let mut changes = Vec::new();
        if give {
            for other in self.saved.iter().filter(|s| s.file != file && modes_of(&s.text).contains(&mode)) {
                let keep: Vec<_> = modes_of(&other.text).into_iter().filter(|m| *m != mode).collect();
                changes.push((other.file.clone(), other.text.clone(), with_modes(&other.text, &keep)));
            }
            modes.push(mode);
        }
        changes.push((file.clone(), saved.text.clone(), with_modes(&saved.text, &modes)));
        if write_configs(&changes).is_err() {
            self.toast(tr("Could not save config").into());
            return;
        }
        self.cfg_interface_context = None;
        self.interface_follow();
        self.cfg_scan();
        if let Some(index) = self.saved.iter().position(|s| s.file == file) {
            if let Some(super::super::Card::Cfg { i, .. }) = &mut self.card {
                *i = index;
            }
        }
        self.switch_sound(give, 0.5);
    }
}
