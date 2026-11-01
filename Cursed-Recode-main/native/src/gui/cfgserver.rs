//! Servers' profiles: a saved config given to servers (its head's `#servers=host,host`) loads its parts as one joins
//! one of them, keeps what is changed there (written back into it), and the player's own comes back as they leave. The
//! menu's own file keeps the player's own all the while (a game shut on such a server starts as their own).

mod interface;
pub(super) use interface::{modes_of, with_modes, write_config, InterfaceProfile, INTERFACE_PARTS};

use std::collections::HashMap;

use super::configs::parse;
use super::i18n::{self, tr};
use super::Gui;

/// A server's profile on now: the config, its file, the server, its parts, the player's own settings and config's name
/// before it.
pub(super) struct Profile {
    file: std::path::PathBuf,
    host: String,
    parts: u8,
    before: HashMap<String, String>,
    loaded_before: Option<String>,
}

/// The server as a profile knows it: its address in lower case, the default port left out; none in the main menu or
/// a world of one's own.
pub(super) fn host_of(server: &str) -> String {
    let s = server.trim().to_lowercase();
    if s == "singleplayer" {
        return String::new();
    }
    s.strip_suffix(":25565").unwrap_or(&s).trim_end_matches('.').to_string()
}

/// The servers a config's text is given to.
pub(super) fn servers_of(text: &str) -> Vec<String> {
    text.lines()
        .find_map(|l| l.strip_prefix("#servers="))
        .map_or(Vec::new(), |v| v.split(',').map(host_of).filter(|h| !h.is_empty()).collect())
}

/// A config's text with its servers as given (the line gone when none).
pub(super) fn with_servers(text: &str, servers: &[String]) -> String {
    let nl = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut out: Vec<&str> = text.lines().filter(|l| !l.starts_with("#servers=")).collect();
    let line = format!("#servers={}", servers.join(","));
    if !servers.is_empty() {
        // after the head (its name and parts)
        let at = out.iter().take_while(|l| l.starts_with('#')).count();
        out.insert(at, &line);
    }
    let mut s = out.join(nl);
    s.push_str(nl);
    s
}

fn lines(dump: &str) -> HashMap<String, String> {
    dump.lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

impl Gui {
    pub(super) fn profiles_use(&self, file: &std::path::Path) -> bool {
        self.profile.as_ref().is_some_and(|p| p.file == file) || self.interface_profile.as_ref().is_some_and(|p| p.file == file)
    }

    pub(super) fn profiles_reload(&mut self) {
        self.cfg_host.clear();
        self.cfg_interface_context = None;
        self.profile_follow();
    }

    pub(super) fn own_config_name(&self) -> Option<String> {
        self.profile.as_ref().map_or_else(|| self.loaded_cfg.clone(), |p| p.loaded_before.clone())
    }

    /// Each frame of the HUD: the server joined or left, its profile loaded or the player's own back.
    pub(super) fn profile_follow(&mut self) {
        // (the stand's world is its own: CURSED_DEV_HOST plays a server)
        static DEV: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
        let dev = DEV.get_or_init(|| std::env::var("CURSED_DEV_HOST").ok().map(|h| host_of(&h)).filter(|h| !h.is_empty()));
        let host = dev.clone().unwrap_or_else(|| host_of(&self.hud.facts.server));
        if host != self.cfg_host {
            if self.profile.is_some() || self.interface_profile.is_some() {
                self.profile_leave(true);
            }
            self.cfg_host = host.clone();
            if !host.is_empty() {
                self.cfg_scan();
                if let Some(s) = self.saved.iter().find(|s| servers_of(&s.text).contains(&host)).cloned() {
                    let (vals, _, have) = parse(&s.text);
                    let before = lines(&self.dump());
                    self.apply_vals(&vals, have);
                    let loaded_before = std::mem::replace(&mut self.loaded_cfg, Some(s.name.clone()));
                    self.profile = Some(Profile {
                        file: s.file.clone(),
                        host: host.clone(),
                        parts: have,
                        before,
                        loaded_before,
                    });
                    self.profile_say(host, i18n::trn("{name} is on", &[("name", s.name)]));
                }
            }
        }
        self.interface_follow();
    }

    /// What a profile did, said where the player looks: over the menu when it is open, in the HUD's notifications when
    /// they play.
    fn profile_say(&mut self, server: String, line: String) {
        if self.phase == super::Phase::Closed {
            let t = self.now;
            self.hud.note(server, line, t);
        } else {
            self.toast(format!("{server}: {line}"));
        }
    }

    /// The profile on now left: what was changed kept in it; the player's own back (`back`), or what is on now kept as
    /// theirs (a config loaded by hand over it).
    pub(super) fn profile_leave(&mut self, back: bool) {
        self.profile_keep();
        let ui = self.interface_profile.take();
        let server = self.profile.take();
        if ui.is_none() && server.is_none() {
            return;
        }
        if back {
            if let Some(p) = ui {
                self.profile_restore(&p.before, p.parts);
            }
            if let Some(p) = server {
                self.profile_restore(&p.before, p.parts);
                self.loaded_cfg = p.loaded_before;
                self.profile_say(p.host, tr("Your own settings are back").to_string());
            }
        }
        self.save();
        self.cfg_scan();
    }

    /// Each layer keeps only what it owns; the interface must not leak into the server's config.
    pub(super) fn profile_keep(&self) {
        self.interface_keep();
        let Some(p) = self.profile.as_ref() else { return };
        let Ok(old) = std::fs::read_to_string(&p.file) else { return };
        let text = self.profile_merge(&old, &self.without_interface(), p.parts);
        if text != old {
            let _ = write_config(&p.file, &text);
        }
    }

    /// The private settings under both temporary layers.
    pub(super) fn own_dump(&self) -> String {
        let dump = self.without_interface();
        self.profile
            .as_ref()
            .map_or_else(|| dump.clone(), |p| self.profile_rewind(&dump, &p.before, p.parts))
    }

    /// A saved config given to the server one is on, or taken from it; the card stays on it.
    pub(super) fn cfg_server(&mut self, i: usize, host: &str, give: bool) {
        let Some(file) = self.saved.get(i).map(|s| s.file.clone()) else { return };
        self.profile_keep();
        self.cfg_scan();
        let Some(s) = self.saved.iter().find(|s| s.file == file).cloned() else {
            return;
        };
        let mut servers = servers_of(&s.text);
        servers.retain(|h| h != host);
        let mut changes = Vec::new();
        if give {
            // (a server has one profile: taken from the config that had it)
            for o in self.saved.iter().filter(|o| o.file != s.file && servers_of(&o.text).iter().any(|h| h == host)) {
                let left: Vec<String> = servers_of(&o.text).into_iter().filter(|h| h != host).collect();
                changes.push((o.file.clone(), o.text.clone(), with_servers(&o.text, &left)));
            }
            servers.push(host.to_string());
        }
        changes.push((s.file.clone(), s.text.clone(), with_servers(&s.text, &servers)));
        if interface::write_configs(&changes).is_err() {
            self.toast(tr("Could not save config").into());
            return;
        }
        self.cfg_scan();
        if let Some(j) = self.saved.iter().position(|o| o.file == s.file) {
            if let Some(super::Card::Cfg { i, .. }) = self.card.as_mut() {
                *i = j;
            }
        }
        self.switch_sound(give, 0.5);
        if give {
            self.toast(i18n::trn("{name} loads on {server}", &[("name", s.name), ("server", host.to_string())]));
        }
        // on that server now: the profile follows at once
        if self.cfg_host == host {
            self.profile_leave(true);
            self.cfg_host.clear();
            self.cfg_interface_context = None;
            self.profile_follow();
        }
    }

    /// The servers a saved config is given to.
    pub(super) fn cfg_servers(&self, i: usize) -> Vec<String> {
        self.saved.get(i).map_or(Vec::new(), |s| servers_of(&s.text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_is_its_address_without_the_default_port() {
        assert_eq!(host_of("MC.FunTime.su:25565"), "mc.funtime.su");
        assert_eq!(host_of("play.x.net:25570"), "play.x.net:25570");
        assert_eq!(host_of("Singleplayer"), "");
        assert_eq!(host_of(""), "");
    }

    /// Given to FunTime: it loads there, what is changed there stays its, the own file keeps the own, and the own
    /// comes back on leaving.
    #[test]
    fn a_profile_loads_on_its_server_and_the_own_comes_back() {
        let dir = std::env::temp_dir().join(format!("cr-profile-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut g = Gui::new();
        g.dir = Some(dir.clone());
        let i = g.mods.iter().position(|m| m.id == "auto-sprint").unwrap();
        let j = g.mods.iter().position(|m| m.id == "auto-respawn").unwrap();
        g.mods[i].on = true;
        g.cfg_write("FT", super::super::configs::P_MODULES, None);
        g.mods[i].on = false;
        g.cfg_scan();
        let k = g.saved.iter().position(|s| s.name == "FT").unwrap();
        g.cfg_server(k, "mc.funtime.su", true);
        assert_eq!(
            g.cfg_servers(g.saved.iter().position(|s| s.name == "FT").unwrap()),
            vec!["mc.funtime.su".to_string()]
        );
        // joins
        g.hud.facts.server = "mc.FunTime.su:25565".into();
        g.profile_follow();
        assert!(g.mods[i].on);
        g.mods[j].on = true;
        assert!(g.own_dump().contains("auto-sprint.#on=0\n") && g.own_dump().contains("auto-respawn.#on=0\n"));
        // leaves
        g.hud.facts.server = String::new();
        g.profile_follow();
        assert!(!g.mods[i].on && !g.mods[j].on);
        let text = std::fs::read_to_string(dir.join("cursed-recode").join("configs").join("FT.cfg")).unwrap();
        assert!(text.contains("auto-respawn.#on=1") && text.contains("#servers=mc.funtime.su"), "{text}");
        // and back: what was changed there is there again
        g.hud.facts.server = "mc.funtime.su".into();
        g.profile_follow();
        assert!(g.mods[i].on && g.mods[j].on);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn servers_go_after_the_head_and_away() {
        let t = "#name=FT\n#parts=31\nhud.x=1\n";
        let g = with_servers(t, &["mc.funtime.su".into()]);
        assert_eq!(g, "#name=FT\n#parts=31\n#servers=mc.funtime.su\nhud.x=1\n");
        assert_eq!(servers_of(&g), vec!["mc.funtime.su".to_string()]);
        assert_eq!(with_servers(&g, &[]), t);
        // the line is no setting
        let (vals, name, parts) = parse(&g);
        assert_eq!((name.as_str(), parts, vals.len()), ("FT", 31, 1));
    }
}
