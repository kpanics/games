//! A module's own code: previewed before applying, scoped to that module and its HUD parts.
use super::draw::{mix, Canvas};
use super::i18n::{self, tr};
use super::text::{fonts, Face};
use super::theme::pal;
use super::{ak, configs, BindTo, Card, Gui, Hit, Owner, A_HOV};
use std::collections::HashMap;

/// Reserved for the module import card; the words page already uses Card(60).
pub(super) const APPLY: u8 = 220;

#[derive(serde::Serialize, serde::Deserialize)]
struct Payload {
    module: String,
    values: HashMap<String, String>,
    #[serde(default)]
    custom: Option<String>,
}

fn custom_ids(list: &str) -> Option<Vec<String>> {
    let mut ids = Vec::new();
    for part in list.split(',').filter(|p| !p.is_empty()) {
        let (id, kind) = part.split_once('|')?;
        if !super::hudx::KINDS.iter().any(|k| k.0 == kind)
            || id
                .strip_prefix(&format!("x-{kind}-"))
                .and_then(|n| n.parse::<u32>().ok())
                .is_none_or(|n| n == 0)
            || ids.iter().any(|old| old == id)
            || ids.len() >= 512
        {
            return None;
        }
        ids.push(id.to_string());
    }
    Some(ids)
}

impl Gui {
    pub(super) fn module_code_hint(&self, id: &str) -> &'static str {
        tr(
            if matches!(id, "hud" | "custom-hotbar" | "custom-f3" | "custom-chat" | "custom-scoreboard" | "other-custom") {
                "Apply this module's settings, switch and binds? Its HUD parts are included."
            } else {
                "Apply this module's settings, switch and binds?"
            },
        )
    }
    fn module_code_owners(&self, id: &str) -> Option<Vec<String>> {
        self.mods.iter().find(|m| m.id == id)?;
        let group = match id {
            "hud" => Some(None),
            "custom-hotbar" => Some(Some("Custom Hotbar")),
            "custom-f3" => Some(Some("Custom F3")),
            "custom-chat" => Some(Some("Custom Chat")),
            "custom-scoreboard" => Some(Some("Custom Scoreboard")),
            "other-custom" => Some(Some("Other Custom Things")),
            _ => None,
        };
        Some(
            self.mods
                .iter()
                .filter(|m| m.id == id || group.is_some_and(|g| m.cat == super::data::Cat::HudEl && m.sub == g))
                .map(|m| m.id.to_string())
                .collect(),
        )
    }

    fn module_code_key(key: &str, owners: &[String]) -> bool {
        if key.starts_with("hud.") && key.ends_with(".place") {
            owners.iter().any(|id| key == format!("hud.{id}.place"))
        } else {
            owners.iter().any(|id| key.starts_with(&format!("{id}.")))
        }
    }

    fn module_code_held(&self, owners: &[String]) -> bool {
        self.holds.iter().any(|(to, _, _)| {
            let i = match to {
                BindTo::Module(i) | BindTo::Setting(Owner::Module(i), _) => *i,
                _ => return false,
            };
            self.mods.get(i).is_some_and(|m| owners.iter().any(|id| id == m.id))
        })
    }

    fn module_code_read(&self, text: &str, expected: Option<&str>) -> Option<(Payload, Vec<String>)> {
        let p: Payload = serde_json::from_str(&configs::unsqueeze("RCDM1-", text)?).ok()?;
        if expected.is_some_and(|id| id != p.module) {
            return None;
        }
        let mut owners = self.module_code_owners(&p.module)?;
        if let Some(list) = &p.custom {
            if p.module != "hud" {
                return None;
            }
            owners.retain(|id| !id.starts_with("x-"));
            owners.extend(custom_ids(list)?);
        }
        if p.values.is_empty() || p.values.keys().any(|key| !Self::module_code_key(key, &owners)) {
            return None;
        }
        owners.retain(|id| p.values.keys().any(|key| key.starts_with(&format!("{id}."))));
        Some((p, owners))
    }

    pub(super) fn module_code_buttons(&mut self, c: &mut Canvas, i: usize, b: [f32; 4]) {
        let r = self.r;
        let width = (b[2] - b[0] - 0.45 * r) / 2.0;
        for (n, label) in ["Copy code", "Paste code"].into_iter().enumerate() {
            let hit = Hit::ModuleCode(i, n == 1);
            let hv = self.an(ak(A_HOV, 2_100_000 + i * 2 + n), if self.hover == hit { 1.0 } else { 0.0 }, 12.0, 0.0);
            let x = b[0] + n as f32 * (width + 0.45 * r);
            let q = [x, b[1], x + width, b[3]];
            c.rect(q, 0.55 * r, mix(pal().f1, pal().f3, hv));
            let label = fonts().fit(Face::Medium, 0.66 * r, tr(label), width - 0.8 * r);
            c.text_c(
                x + width * 0.5,
                (b[1] + b[3]) * 0.5 + 0.23 * r,
                0.66 * r,
                Face::Medium,
                &label,
                mix(pal().t3, pal().t1, hv),
            );
            self.hits.push((q, hit));
        }
    }

    pub(super) fn module_code_press(&mut self, i: usize, paste: bool) {
        let Some(m) = self.mods.get(i) else { return };
        let id = m.id.to_string();
        if paste {
            self.paste_for = 0;
            self.paste_module = Some(id);
            self.outbox.push("paste".into());
            return;
        }
        let Some(owners) = self.module_code_owners(&id) else { return };
        if self.module_code_held(&owners) {
            self.toast(tr("Release the module's held keys first").into());
            return;
        }
        let (mut values, _, _) = configs::parse(&self.dump());
        values.retain(|key, _| Self::module_code_key(key, &owners));
        let custom = (id == "hud").then(|| self.custom_list());
        let p = Payload { module: id, values, custom };
        let code = configs::squeeze("RCDM1-", &serde_json::to_string(&p).unwrap());
        self.copy(&code);
        self.toast(i18n::trn("Code of {name} copied", &[("name", self.mods[i].name.to_string())]));
        self.sound_tick(0.5);
    }

    pub(super) fn module_code_paste(&mut self, text: &str, expected: Option<&str>) {
        let Some((p, _)) = self.module_code_read(text, expected) else {
            self.toast(tr("This is not a code for this module").into());
            return;
        };
        self.card = Some(Card::ModuleImport {
            module: p.module,
            text: text.to_string(),
            at: [self.mouse[0], self.mouse[1], self.mouse[0], self.mouse[1]],
        });
        self.card_at = self.now;
        self.pop = None;
        self.pick = None;
        self.text_focus = None;
        self.anim.remove(&ak(super::A_POP, 2));
        self.sound_tick(0.5);
    }

    pub(super) fn module_code_apply(&mut self, text: &str, expected: &str) -> bool {
        let Some((p, owners)) = self.module_code_read(text, Some(expected)) else {
            return false;
        };
        if self.module_code_held(&self.module_code_owners(expected).unwrap_or_default()) {
            self.toast(tr("Release the module's held keys first").into());
            return false;
        }
        if let Some(list) = &p.custom {
            self.custom_ensure(list);
        }
        self.apply_vals_scoped(&p.values, configs::P_ALL, Some(&owners));
        self.save();
        self.switch_sound(true, 0.6);
        self.toast(i18n::trn(
            "{name} loaded",
            &[("name", self.mods.iter().find(|m| m.id == expected).map_or(expected, |m| m.name).to_string())],
        ));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::{
        binds::Bind,
        cards::Prior,
        data::{Col, Kind, Setting},
    };

    fn isolated(name: &str) -> bool {
        if std::env::var("CURSED_MODULE_TEST_CHILD").ok().as_deref() == Some(name) {
            return false;
        }
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", &format!("gui::modulecode::tests::{name}"), "--nocapture", "--test-threads=1"])
            .env("CURSED_MODULE_TEST_CHILD", name)
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
    fn index(g: &Gui, id: &str) -> usize {
        g.mods.iter().position(|m| m.id == id).unwrap()
    }
    fn copied(g: &mut Gui, id: &str) -> String {
        g.outbox.clear();
        g.module_code_press(index(g, id), false);
        g.outbox.iter().find_map(|v| v.strip_prefix("copy\t")).expect("module code").to_string()
    }
    fn change(set: &mut [Setting]) {
        for s in set {
            match &mut s.kind {
                Kind::Toggle(v) => *v = !*v,
                Kind::Slider { min, max, v, .. } => *v = if *v == *max { *min } else { *max },
                Kind::Choice { v, opts } => *v = (*v + 1) % opts.len(),
                Kind::Multi { on, .. } => on.iter_mut().for_each(|v| *v = !*v),
                Kind::Colour(v) => {
                    *v = Col {
                        rgb: 0x123456,
                        a: 0.42,
                        chroma: 0.37,
                        auto: !v.auto,
                    }
                }
                Kind::Key(v) => *v = "F7".into(),
                Kind::Text(v) => *v = "Проверка = Қазақ | текст".into(),
                _ => continue,
            }
            if matches!(s.kind, Kind::Toggle(_) | Kind::Choice { .. }) {
                s.binds = vec![Bind::one("F8")];
            }
        }
    }

    #[test]
    fn module_code_keeps_other_settings_binds_and_held_actions() {
        if isolated("module_code_keeps_other_settings_binds_and_held_actions") {
            return;
        }
        let mut g = Gui::new();
        let i = index(&g, "auto-sprint");
        let other = index(&g, "no-friend-damage");
        g.mods[i].on = true;
        g.mods[i].binds = vec![Bind::one("F6")];
        change(&mut g.mods[i].set);
        let expected = configs::parse(&g.dump()).0;
        let code = copied(&mut g, "auto-sprint");
        g.mods[i].on = false;
        g.mods[i].binds.clear();
        for s in &mut g.mods[i].set {
            s.reset();
            s.binds.clear();
        }
        g.mods[other].on = true;
        g.mods[other].binds = vec![Bind::one("K")];
        change(&mut g.mods[other].set);
        g.holds.push((BindTo::Module(other), vec!["K".into()], Prior::Module(false)));
        let before = configs::parse(&g.dump()).0;
        g.module_code_paste(&code, Some("auto-sprint"));
        assert!(matches!(g.card, Some(Card::ModuleImport { .. })));
        assert_eq!(before, configs::parse(&g.dump()).0, "preview is read only");
        g.cfg_card_press(Hit::Card(60, 0));
        assert!(matches!(g.card, Some(Card::ModuleImport { .. })));
        assert_eq!(before, configs::parse(&g.dump()).0, "a different page action cannot apply the import");
        g.cfg_card_press(Hit::Card(APPLY, 0));
        assert!(g.card.is_none());
        let after = configs::parse(&g.dump()).0;
        for key in before.keys().chain(expected.keys()) {
            assert_eq!(
                after.get(key),
                if key.starts_with("auto-sprint.") {
                    expected.get(key)
                } else {
                    before.get(key)
                },
                "{key}"
            );
        }
        assert_eq!(g.holds.len(), 1);
        assert!(!g.module_code_apply(&code, "no-friend-damage"));
        let mut bad: Payload = serde_json::from_str(&configs::unsqueeze("RCDM1-", &code).unwrap()).unwrap();
        bad.values.insert("no-friend-damage.#on".into(), "0".into());
        let poisoned = configs::squeeze("RCDM1-", &serde_json::to_string(&bad).unwrap());
        assert!(!g.module_code_apply(&poisoned, "auto-sprint"));
        assert_eq!(after, configs::parse(&g.dump()).0);
        g.holds.push((BindTo::Module(i), vec!["Q".into()], Prior::Module(false)));
        assert!(!g.module_code_apply(&code, "auto-sprint"));
        g.outbox.clear();
        g.module_code_press(i, false);
        assert!(g.outbox.is_empty());
    }

    #[test]
    fn hud_group_code_keeps_other_groups_and_restores_custom_widgets() {
        if isolated("hud_group_code_keeps_other_groups_and_restores_custom_widgets") {
            return;
        }
        let mut source = Gui::new();
        let owners = source.module_code_owners("custom-hotbar").unwrap();
        assert!(owners.len() > 1);
        for m in &mut source.mods {
            if owners.iter().any(|id| id == m.id) {
                m.on = true;
                change(&mut m.set);
            }
        }
        let expected = configs::parse(&source.dump()).0;
        let code = copied(&mut source, "custom-hotbar");
        let mut dest = Gui::new();
        let other = index(&dest, "custom-chat");
        dest.mods[other].binds = vec![Bind::one("M")];
        let before = configs::parse(&dest.dump()).0;
        assert!(dest.module_code_apply(&code, "custom-hotbar"));
        let after = configs::parse(&dest.dump()).0;
        for key in before.keys().chain(expected.keys()) {
            assert_eq!(
                after.get(key),
                if Gui::module_code_key(key, &owners) {
                    expected.get(key)
                } else {
                    before.get(key)
                },
                "{key}"
            );
        }
        source.custom_add("text");
        source.custom_add("card");
        let source_text = index(&source, "x-text-1");
        change(&mut source.mods[source_text].set);
        let expected = configs::parse(&source.dump()).0;
        let code = copied(&mut source, "hud");
        dest.custom_add("image");
        assert!(dest.module_code_apply(&code, "hud"));
        assert_eq!(dest.custom_list(), source.custom_list());
        let after = configs::parse(&dest.dump()).0;
        for (key, value) in &expected {
            if key.starts_with("x-text-1.") {
                assert_eq!(after.get(key), Some(value), "{key}");
            }
        }
        assert_eq!(dest.mods[other].binds[0].keys, ["M"]);
        assert!(custom_ids("auto-sprint|text").is_none());
        assert!(custom_ids("x-text-1|text,x-text-1|text").is_none());
    }

    #[test]
    fn every_setting_survives_each_config_part_combination() {
        if isolated("every_setting_survives_each_config_part_combination") {
            return;
        }
        let mut source = Gui::new();
        for kind in ["text", "card", "image"] {
            source.custom_add(kind);
        }
        for m in &mut source.mods {
            if !m.plain {
                m.on = !m.on;
                m.binds = vec![Bind::one("F6")];
            }
            change(&mut m.set);
        }
        change(&mut source.client);
        change(&mut source.theme);
        // Editing the individual timing values selects Custom in the real menu.
        let swap = index(&source, super::super::swap::MODULE);
        let preset = source.mods[swap].set.iter_mut().find(|s| s.label == "Preset").unwrap();
        if let Kind::Choice { v, .. } = &mut preset.kind {
            *v = 4;
        }
        let chams = index(&source, "chams");
        let liquid = source.mods[chams].set.iter_mut().find(|s| s.label == "Liquid").unwrap();
        if let Kind::Choice { v, opts } = &mut liquid.kind {
            *v = opts.iter().position(|name| *name == "Own").unwrap();
        }
        i18n::set_word("Search", Some("Мой поиск"));
        let dump = source.dump();
        let mut keys = std::collections::HashSet::new();
        for key in dump.lines().filter_map(|line| line.split_once('=').map(|p| p.0)) {
            assert!(keys.insert(key), "duplicate persisted setting: {key}");
        }
        let source_values = configs::parse(&dump).0;
        for parts in 1..=configs::P_ALL {
            i18n::clear_words();
            let mut dest = Gui::new();
            for kind in ["text", "card", "image"] {
                dest.custom_add(kind);
            }
            let before = configs::parse(&dest.dump()).0;
            let text = source_values
                .iter()
                .filter(|(k, _)| source.part_of(k) & parts != 0)
                .map(|(k, v)| format!("{k}={v}\n"))
                .collect::<String>();
            let encoded = configs::encode(&format!("#parts={parts}\n{text}"));
            let (values, _, have) = configs::parse(&configs::decode(&encoded).unwrap());
            assert_eq!(have, parts);
            dest.apply_vals(&values, have);
            let after = configs::parse(&dest.dump()).0;
            for key in before.keys().chain(source_values.keys()) {
                let expected = if source.part_of(key) & parts != 0 {
                    source_values.get(key)
                } else {
                    before.get(key)
                };
                assert_eq!(after.get(key), expected, "parts={parts}, {key}");
            }
        }
    }

    #[test]
    fn invalid_config_numbers_preserve_finite_settings() {
        if isolated("invalid_config_numbers_preserve_finite_settings") {
            return;
        }
        let mut g = Gui::new();
        g.custom_add("text");
        let before = configs::parse(&g.dump()).0;
        let mut bad = before.clone();
        let slider = g
            .mods
            .iter()
            .flat_map(|m| m.set.iter().map(move |s| (m.id, s)))
            .find(|(_, s)| matches!(s.kind, Kind::Slider { .. }))
            .unwrap();
        let slider_key = format!("{}.{}", slider.0, slider.1.label);
        bad.insert(slider_key.clone(), "NaN".into());
        bad.insert("hud.x-text-1.place".into(), "4/NaN/inf".into());
        let colour = g
            .mods
            .iter()
            .flat_map(|m| m.set.iter().map(move |s| (m.id, s)))
            .find(|(_, s)| matches!(s.kind, Kind::Colour(_)))
            .unwrap();
        let colour_key = format!("{}.{}", colour.0, colour.1.label);
        let old = before[&colour_key].split('/').collect::<Vec<_>>();
        bad.insert(colour_key.clone(), format!("{}/NaN/inf/{}", old[0], old[3]));
        g.apply_vals(&bad, configs::P_ALL);
        let after = configs::parse(&g.dump()).0;
        assert_eq!(after.get(&slider_key), before.get(&slider_key));
        assert_eq!(after.get(&colour_key), before.get(&colour_key));
        assert_eq!(after.get("hud.x-text-1.place"), before.get("hud.x-text-1.place"));
    }

    #[test]
    fn module_import_preview_fits_all_four_languages() {
        if isolated("module_import_preview_fits_all_four_languages") {
            return;
        }
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
                let mut g = Gui::new();
                g.w = w;
                g.h = h;
                g.r = r;
                g.dt = 0.1;
                g.now = 10.0;
                g.win = [80.0, 40.0, w - 160.0, h - 80.0];
                let i = index(&g, "auto-accept");
                let code = copied(&mut g, "auto-accept");
                g.module_code_paste(&code, Some("auto-accept"));
                let mut c = Canvas::new(w, h);
                for _ in 0..20 {
                    g.now += 0.1;
                    g.hits.clear();
                    c.p.clear();
                    g.module_code_buttons(&mut c, i, [20.0, 15.0, 20.0 + 18.0 * r, 15.0 + 1.9 * r]);
                    g.cfg_card(&mut c);
                }
                let card = g.card_rect;
                assert!(card[0] >= 0.0 && card[1] >= 0.0 && card[2] <= w && card[3] <= h);
                for (b, hit) in &g.hits {
                    if matches!(hit, Hit::Card(..)) {
                        assert!(b[0] >= card[0] && b[1] >= card[1] && b[2] <= card[2] && b[3] <= card[3], "{lang}: {b:?}");
                    }
                }
                for label in ["Copy code", "Paste code"] {
                    assert!(
                        fonts().width(Face::Medium, 0.66 * r, tr(label)) <= (18.0 * r - 0.45 * r) / 2.0 - 0.8 * r,
                        "{lang}: {label}"
                    );
                }
                assert!(g.hits.iter().any(|(_, h)| *h == Hit::Card(APPLY, 0)));
                if w == 960.0 {
                    if let Some(path) = std::env::var_os("CURSED_TEST_CARD_DUMP") {
                        let path = std::path::PathBuf::from(path);
                        std::fs::create_dir_all(&path).unwrap();
                        dynamic.extend(crate::gui::dynfont::drain());
                        std::fs::write(
                            path.join(format!("module-code-{lang}.json")),
                            serde_json::to_vec(&serde_json::json!({"w":w,"h":h,"shapes":c.p,"dynamic":dynamic})).unwrap(),
                        )
                        .unwrap();
                    }
                }
                g.cfg_card_press(Hit::Card(40, 0));
                assert!(g.card.is_none());
            }
        }
    }
}
