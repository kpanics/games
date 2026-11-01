//! The server and exact-streak controls in the existing messages card.
use super::{
    ak,
    data::Kind,
    draw::{mix, Canvas},
    i18n::{tr, trn},
    messages::{Book, SETTING},
    play::KILL_LISTS,
    text::{fonts, Face},
    theme::{self, pal},
    Gui, Hit, A_HOV,
};

pub const CONTROL: u8 = 89;

#[derive(Default)]
pub struct State {
    pub server: String,
    pub streak: Option<u32>,
    pub field: u8, // 0 message, 1 server address, 2 exact streak
    pub book: Book,
    pub reset_at: Option<f32>,
    source: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn message_profiles_edit_independently_and_roundtrip_in_module_configs() {
        let mut g = Gui::new();
        let i = g.mod_index("kill-message");
        g.lines_open(i);
        g.message_put(i, 0, &["global".into()]);
        g.message_control(1, i, 0);
        g.card_field = "ONE.TEST:25565".into();
        assert!(g.message_enter(i));
        assert_eq!(g.play_ui.message.server, "one.test");
        g.message_put(i, 0, &["server".into()]);
        g.message_control(4, i, 1);
        g.card_field = "3".into();
        assert!(g.message_enter(i));
        g.message_put(i, 1, &["three {name}".into()]);
        let saved = g.dump();
        let vals = saved
            .lines()
            .filter_map(|s| s.split_once('='))
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let mut restored = Gui::new();
        restored.apply_vals(&vals, super::super::configs::P_MODULES);
        let book = Book::read(&restored.mods[i].set);
        assert_eq!(book.profile(&restored.mods[i].set, "one.test").lines(0, None), ["server"]);
        assert_eq!(book.profile(&restored.mods[i].set, "one.test").lines(1, Some(3)), ["three {name}"]);
        assert_eq!(book.profile(&restored.mods[i].set, "two.test").lines(0, None), ["global"]);
        g.message_control(7, i, 0);
        assert!(g.play_ui.message.book.servers.contains_key("one.test"));
        g.message_control(7, i, 0);
        assert!(!g.play_ui.message.book.servers.contains_key("one.test"));
        assert_eq!(g.message_lines(i, 0), ["global"]);
        g.play_ui.message.book.streaks.insert(3, vec!["all three".into()]);
        g.play_ui.message.server = "new.test".into();
        g.message_control(5, i, 1);
        assert_eq!(g.play_ui.message.streak, Some(3));
        g.message_control(6, i, 1);
        assert!(g.play_ui.message.book.streaks.contains_key(&3));
        assert!(!g.play_ui.message.book.servers["new.test"].streaks.contains_key(&3));
        // Editing another row after deleting an earlier one must keep its intended position.
        g.message_control(0, i, 0);
        g.message_put(i, 0, &["first".into(), "second".into(), "third".into()]);
        g.lines_press(Hit::Card(85, 2), 0);
        g.lines_press(Hit::Card(86, 0), 0);
        g.card_field = "changed third".into();
        g.lines_enter();
        assert_eq!(g.message_lines(i, 0), ["second", "changed third"]);
    }

    #[test]
    fn message_card_layout_stays_inside_small_and_large_windows() {
        // Language and font settings are global; isolate this visual-layout check from other tests.
        if std::env::var_os("CURSED_MESSAGE_LAYOUT_CHILD").is_none() {
            let result = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "gui::messageui::tests::message_card_layout_stays_inside_small_and_large_windows",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env("CURSED_MESSAGE_LAYOUT_CHILD", "1")
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}{}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
            return;
        }
        let mut dynamic = Vec::new();
        for lang in ["ru_ru", "en_us", "uk_ua", "kk_kz"] {
            crate::menus::lang::set(lang);
            let letters: Vec<char> = ["All servers", "At {n} kills", "Remove", "Every streak", "Messages"]
                .iter()
                .flat_map(|key| tr(key).chars())
                .chain("ӘәІіҢңҒғҮүҰұҚқӨөҺһ".chars())
                .collect();
            for face in [Face::Regular, Face::Medium, Face::Bold, Face::Semi] {
                for &ch in &letters {
                    fonts().glyph(face, ch);
                }
            }
            for _ in 0..400 {
                if ![Face::Regular, Face::Medium, Face::Bold, Face::Semi]
                    .iter()
                    .any(|&face| letters.iter().any(|&ch| super::super::dynfont::waiting(face, ch)))
                {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            for (w, h, r) in [(960.0, 540.0, 12.0), (1600.0, 900.0, 18.0), (2560.0, 1440.0, 22.0)] {
                let mut g = Gui::new();
                g.w = w;
                g.h = h;
                g.r = r;
                g.dt = 0.1;
                g.now = 10.0;
                g.mouse = [w * 0.5, h * 0.5];
                let i = g.mod_index("kill-message");
                g.lines_open(i);
                g.play_ui.message.server = "mc.example.test".into();
                g.message_put(i, 0, &["gg {name}".into()]);
                g.play_ui.message.streak = Some(3);
                g.message_put(i, 1, &["{name}, это третья победа подряд".into(), "{streak} — и это только начало".into()]);
                let at = [w * 0.5, h * 0.5, w * 0.5, h * 0.5];
                let mut c = Canvas::new(w, h);
                for _ in 0..20 {
                    g.now += 0.1;
                    g.hits.clear();
                    c.p.clear();
                    g.lines_card(&mut c, i, 1, at);
                }
                let bounds = g.card_rect;
                assert!(bounds[0] >= 0.0 && bounds[1] >= 0.0 && bounds[2] <= w && bounds[3] <= h, "{lang}: {bounds:?}");
                for (rect, hit) in &g.hits {
                    if matches!(hit, Hit::Card(CONTROL | 84 | 87, _)) {
                        assert!(rect[2] > rect[0] && rect[3] > rect[1], "{lang}: {hit:?} {rect:?}");
                        assert!(
                            rect[0] >= bounds[0] && rect[1] >= bounds[1] && rect[2] <= bounds[2] && rect[3] <= bounds[3],
                            "{lang}: {hit:?} {rect:?}"
                        );
                    }
                }
                assert_eq!(
                    g.hits.iter().filter(|(_, hit)| matches!(hit, Hit::Card(88, _))).count(),
                    7,
                    "all placeholders must fit: {lang}"
                );
                if w == 1600.0 {
                    if let Some(path) = std::env::var_os("CURSED_TEST_CARD_DUMP") {
                        let path = std::path::PathBuf::from(path);
                        std::fs::create_dir_all(&path).unwrap();
                        dynamic.extend(super::super::dynfont::drain());
                        std::fs::write(
                            path.join(format!("messages-{lang}.json")),
                            serde_json::to_vec(&serde_json::json!({"w":w,"h":h,"shapes":c.p,"dynamic":dynamic})).unwrap(),
                        )
                        .unwrap();
                    }
                }
            }
        }
    }
}

impl Gui {
    pub(super) fn message_open(&mut self, i: usize) {
        self.play_ui.message = State {
            book: Book::read(&self.mods[i].set),
            source: theme::text(&self.mods[i].set, SETTING).into(),
            ..State::default()
        };
    }
    pub(super) fn message_sync(&mut self, i: usize) {
        let source = theme::text(&self.mods[i].set, SETTING);
        if self.play_ui.message.source != source {
            self.play_ui.message.book = Book::read(&self.mods[i].set);
            self.play_ui.message.source = source.to_string();
        }
    }
    pub(super) fn message_lines(&self, i: usize, tab: usize) -> Vec<String> {
        let state = &self.play_ui.message;
        if let Some(profile) = state.book.servers.get(&state.server) {
            return profile.lines(tab, state.streak).to_vec();
        }
        if tab == 1 {
            if let Some(lines) = state.streak.and_then(|n| state.book.streaks.get(&n)) {
                return lines.clone();
            }
        }
        theme::text(&self.mods[i].set, KILL_LISTS[tab].1)
            .split('\u{1e}')
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect()
    }
    pub(super) fn message_put(&mut self, i: usize, tab: usize, lines: &[String]) {
        let fallback = self.play_ui.message.book.defaults(&self.mods[i].set);
        let state = &mut self.play_ui.message;
        if state.server.is_empty() && (tab != 1 || state.streak.is_none()) {
            if let Some(setting) = self.mods[i].set.iter_mut().find(|s| s.label == KILL_LISTS[tab].1) {
                setting.kind = Kind::Text(lines.join("\u{1e}"));
            }
        } else if !state.server.is_empty() {
            let profile = state.book.servers.entry(state.server.clone()).or_insert(fallback);
            if let (1, Some(n)) = (tab, state.streak) {
                profile.streaks.insert(n, lines.to_vec());
            } else {
                profile.lists[tab] = lines.to_vec();
            }
        } else if let Some(n) = state.streak {
            state.book.streaks.insert(n, lines.to_vec());
        }
        self.message_save(i);
    }
    fn message_save(&mut self, i: usize) {
        if let Some(s) = self.mods[i].set.iter_mut().find(|s| s.label == SETTING) {
            self.play_ui.message.source = serde_json::to_string(&self.play_ui.message.book).unwrap_or_default();
            s.kind = Kind::Text(self.play_ui.message.source.clone());
        }
        self.kill_messages_order_reset();
        self.save();
    }
    pub(super) fn message_field_reset(&mut self) {
        self.card_field.clear();
        self.play_ui.editing = None;
        self.play_ui.message.field = 0;
        self.play_ui.message.reset_at = None;
        self.card_scroll = 0.0;
        self.card_scroll_to = 0.0;
        self.play_ui.tab_at = self.now;
    }
    pub(super) fn message_char(&mut self, ch: char) {
        let state = &self.play_ui.message;
        let valid = match state.field {
            1 => ch.is_ascii_alphanumeric() || ".-:_[]".contains(ch),
            2 => ch.is_ascii_digit(),
            _ => !ch.is_control(),
        };
        let max = match state.field {
            1 => 80,
            2 => 10,
            _ => 240,
        };
        if valid && self.card_field.chars().count() < max {
            self.card_field.push(ch);
        }
    }
    pub(super) fn message_enter(&mut self, i: usize) -> bool {
        match self.play_ui.message.field {
            1 => {
                if let Some(server) = crate::client_marks::server_key(&self.card_field) {
                    self.play_ui.message.server = server;
                    self.play_ui.message.streak = None;
                    self.message_field_reset();
                } else {
                    self.play_ui.wrong_at = Some(self.now);
                }
            }
            2 => {
                if let Ok(n) = self.card_field.parse::<u32>() {
                    if n >= 2 {
                        self.play_ui.message.streak = Some(n);
                        let lines = self.message_lines(i, 1);
                        self.message_put(i, 1, &lines);
                        self.message_field_reset();
                        return true;
                    }
                }
                self.play_ui.wrong_at = Some(self.now);
            }
            _ => return false,
        }
        true
    }
    pub(super) fn message_control(&mut self, n: u16, i: usize, tab: usize) {
        self.play_ui.editing = None;
        match n {
            0 => {
                self.play_ui.message.server.clear();
                self.play_ui.message.streak = None;
            }
            1 => {
                self.message_field_reset();
                self.play_ui.message.field = 1;
                self.card_field = if self.play_ui.message.server.is_empty() {
                    crate::client_marks::scope().server.unwrap_or_default()
                } else {
                    self.play_ui.message.server.clone()
                };
                return;
            }
            2 => {
                let state = &mut self.play_ui.message;
                let all: Vec<String> = std::iter::once(String::new()).chain(state.book.servers.keys().cloned()).collect();
                let at = all.iter().position(|s| s == &state.server).map_or(0, |n| (n + 1) % all.len());
                state.server = all[at].clone();
                state.streak = None;
            }
            3 => self.play_ui.message.streak = None,
            4 if tab == 1 => {
                self.message_field_reset();
                self.play_ui.message.field = 2;
                self.card_field = self.play_ui.message.streak.map(|n| n.to_string()).unwrap_or_default();
                return;
            }
            5 if tab == 1 => {
                let state = &mut self.play_ui.message;
                let rules = state.book.servers.get(&state.server).map(|p| &p.streaks).unwrap_or(&state.book.streaks);
                state.streak = rules.keys().copied().filter(|n| *n >= 2).find(|n| state.streak.is_none_or(|old| *n > old));
            }
            6 if tab == 1 => {
                let fallback = self.play_ui.message.book.defaults(&self.mods[i].set);
                let state = &mut self.play_ui.message;
                if let Some(n) = state.streak.take() {
                    if state.server.is_empty() {
                        state.book.streaks.remove(&n);
                    } else {
                        state.book.servers.entry(state.server.clone()).or_insert(fallback).streaks.remove(&n);
                    }
                    self.message_save(i);
                }
            }
            7 if !self.play_ui.message.server.is_empty() => {
                let state = &mut self.play_ui.message;
                if state.reset_at.is_some_and(|t| self.now - t <= 4.0) {
                    state.book.servers.remove(&state.server);
                    state.streak = None;
                    self.message_save(i);
                } else {
                    state.reset_at = Some(self.now);
                    return;
                }
            }
            _ => return,
        }
        self.message_field_reset();
        self.sound_tick(0.45);
    }
    fn message_button(&mut self, c: &mut Canvas, rect: [f32; 4], n: u16, label: &str, selected: bool) {
        let hit = Hit::Card(CONTROL, n);
        let hover = self.an(ak(A_HOV, 98_000 + n as usize), if self.hover == hit { 1.0 } else { 0.0 }, 14.0, 0.0);
        c.rect(rect, 0.6 * self.r, mix(if selected { pal().f3 } else { pal().f1 }, pal().f4, hover));
        let label = fonts().fit(Face::Medium, 0.68 * self.r, label, rect[2] - rect[0] - 0.8 * self.r);
        c.text_c(
            (rect[0] + rect[2]) * 0.5,
            rect[1] + 1.28 * self.r,
            0.68 * self.r,
            Face::Medium,
            &label,
            if selected { pal().t1 } else { pal().t3 },
        );
        self.hits.push((rect, hit));
    }
    pub(super) fn message_controls(&mut self, c: &mut Canvas, x0: f32, x1: f32, y: f32, tab: usize) {
        let r = self.r;
        let server = self.play_ui.message.server.clone();
        let label = if server.is_empty() {
            tr("Server address").to_string()
        } else {
            server.clone()
        };
        self.message_button(c, [x0, y, x0 + 8.0 * r, y + 1.9 * r], 0, tr("All servers"), server.is_empty());
        self.message_button(c, [x0 + 8.4 * r, y, x1 - 2.8 * r, y + 1.9 * r], 1, &label, !server.is_empty());
        self.message_button(c, [x1 - 2.4 * r, y, x1, y + 1.9 * r], 2, "›", false);
        let y = y + 2.4 * r;
        if tab == 1 {
            let streak = self.play_ui.message.streak;
            let label = streak
                .map(|n| trn("At {n} kills", &[("n", n.to_string())]))
                .unwrap_or_else(|| tr("Exact streak").to_string());
            self.message_button(c, [x0, y, x0 + 8.0 * r, y + 1.9 * r], 3, tr("Every streak"), streak.is_none());
            self.message_button(c, [x0 + 8.4 * r, y, x1 - 8.4 * r, y + 1.9 * r], 4, &label, streak.is_some());
            self.message_button(c, [x1 - 8.0 * r, y, x1 - 5.6 * r, y + 1.9 * r], 5, "›", false);
            if streak.is_some() {
                self.message_button(c, [x1 - 5.2 * r, y, x1, y + 1.9 * r], 6, tr("Remove"), false);
            }
        } else {
            let own = self.play_ui.message.book.servers.contains_key(&server);
            let note = if own {
                tr("Separate lists for this server")
            } else {
                tr("Default message lists")
            };
            let note = fonts().fit(Face::Regular, 0.66 * r, note, x1 - x0 - 8.5 * r);
            c.text(x0, y + 1.25 * r, 0.66 * r, Face::Regular, &note, pal().t4);
            if own {
                let armed = self.play_ui.message.reset_at.is_some_and(|t| self.now - t <= 4.0);
                self.message_button(
                    c,
                    [x1 - 8.0 * r, y, x1, y + 1.9 * r],
                    7,
                    if armed { tr("Reset this server?") } else { tr("Use defaults") },
                    false,
                );
            }
        }
    }
}
