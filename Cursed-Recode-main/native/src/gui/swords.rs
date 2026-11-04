//! The swords' models (Render · Effects · Swords): a custom 3D model in place of the game's swords, chosen in
//! the menu. The models live in `<game>/cursed-recode/swords/<name>/model.json` — the game's own model format,
//! as Blockbench saves a «Java Item Model» (`elements`, and its `display` how it is held; without either the
//! handheld's holding is lent) — with its pictures in `textures/` beside it, in the model referred to as
//! `cursed-recode:sword/<file>`. The chosen one is written by the game's side (cursed.recode.Swords) into
//! `<game>/cursed-recode/pack` — a pack of the game's own (cursed.recode.UserPack) — as its
//! `assets/minecraft/items/*_sword.json`, and the resources reload (the menu's `sword <name>` ask, GuiAsks).
//! «Vanilla» puts the game's own swords back. A folder's name is the model's name: Latin lower case, digits,
//! `_ - .` (the game's resource paths have them so).

use super::data::Kind;
use super::i18n;
use super::{Gui, Owner};
use std::path::PathBuf;

pub const MODULE: &str = "sword-model";
/// The choice's label in the module's settings (data.rs keeps the placeholder; `refresh` puts the names in).
pub const LABEL: &str = "Model";

/// The names found so far, leaked for the settings' `'static` options; kept so a name met again is not leaked
/// twice. «Vanilla» always first.
static NAMES: std::sync::Mutex<Vec<&'static str>> = std::sync::Mutex::new(Vec::new());

fn leak(names: &[String]) -> Vec<&'static str> {
    let mut kept = NAMES.lock().unwrap();
    let mut out = vec!["Vanilla"];
    for n in names {
        if let Some(seen) = kept.iter().find(|s| **s == n.as_str()) {
            out.push(seen);
        } else {
            let leaked: &'static str = Box::leak(n.clone().into_boxed_str());
            kept.push(leaked);
            out.push(leaked);
        }
    }
    out
}

/// Whether a folder's name can be a model's (the game's resource paths: Latin lower case, digits, `_ - .`).
fn valid(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-' || c == '.')
}

/// The folder the models are in (`<game>/cursed-recode/swords`), made if the game's place is known.
fn folder(g: &Gui) -> Option<PathBuf> {
    g.dir.as_ref().map(|d| d.join("cursed-recode").join("swords"))
}

/// What was applied last (`cursed-recode/swords.txt`, written by the game's side when it applied a model).
fn applied(g: &Gui) -> String {
    g.dir
        .as_ref()
        .and_then(|d| std::fs::read_to_string(d.join("cursed-recode").join("swords.txt")).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// The folders with a `model.json` in them, by name, sorted; none when the game's place is not known.
fn scan(g: &Gui) -> Vec<String> {
    let Some(dir) = folder(g) else { return Vec::new() };
    let _ = std::fs::create_dir_all(&dir);
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| valid(n) && dir.join(n).join("model.json").is_file())
        .collect();
    names.sort();
    names.dedup();
    names
}

impl Gui {
    /// The models' names as they are now, and the module's choice put to what was applied last: when the menu
    /// opens (the game's place just learned) and when the module's panel opens (a model may have been added).
    pub(super) fn swords_refresh(&mut self) {
        let names = leak(&scan(self));
        let applied = applied(self);
        if let Some(m) = self.mods.iter_mut().find(|m| m.id == MODULE) {
            // the switch first (the choice's names go into the settings below): on while a model of ours
            // is applied (the pack on the disk is the truth)
            m.on = names.contains(&applied.as_str()) && applied != "Vanilla";
            if let Some(s) = m.set.iter_mut().find(|s| s.label == LABEL) {
                let cur = match &s.kind {
                    Kind::Choice { v, opts } => opts.get(*v).copied().unwrap_or("Vanilla"),
                    _ => "Vanilla",
                };
                // what the choice shows: what was applied (the pack on the disk is the truth), else what it was
                let want = if names.contains(&applied.as_str()) {
                    applied.as_str()
                } else if names.contains(&cur) {
                    cur
                } else {
                    "Vanilla"
                };
                let v = names.iter().position(|n| *n == want).unwrap_or(0);
                s.kind = Kind::Choice { v, opts: names.clone() };
                s.def = Kind::Choice { v: 0, opts: names };
            }
        }
    }

    /// A model chosen in the module's settings: tell the game (it writes the pack and reloads; «Vanilla» puts
    /// the game's own swords back), and say so. The module's switch off also means the game's own swords.
    pub(super) fn swords_pick(&mut self, owner: Owner) {
        let Owner::Module(i) = owner else { return };
        if self.mods.get(i).map(|m| m.id) != Some(MODULE) {
            return;
        }
        let on = self.mods[i].on;
        let name = self.mods[i]
            .set
            .iter()
            .find(|s| s.label == LABEL)
            .and_then(|s| match &s.kind {
                Kind::Choice { v, opts } => Some(opts.get(*v).copied().unwrap_or("Vanilla")),
                _ => None,
            })
            .unwrap_or("Vanilla");
        let name = if on { name } else { "Vanilla" };
        self.outbox.push(format!("sword\t{name}"));
        let what = if name == "Vanilla" { "the game's own" } else { name };
        self.toast(i18n::trn("The resources reload: {what} swords", &[("what", what.to_string())]));
    }

    /// The module's switch flipped: on puts the chosen model back, off the game's own swords.
    pub(super) fn swords_toggle(&mut self, i: usize) {
        if self.mods.get(i).map(|m| m.id) != Some(MODULE) {
            return;
        }
        let name = if self.mods[i].on {
            match &self.mods[i].set.iter().find(|s| s.label == LABEL).map(|s| &s.kind) {
                Some(Kind::Choice { v, opts }) => opts.get(*v).copied().unwrap_or("Vanilla"),
                _ => "Vanilla",
            }
        } else {
            "Vanilla"
        };
        self.outbox.push(format!("sword\t{name}"));
        let what = if name == "Vanilla" { "the game's own" } else { name };
        self.toast(i18n::trn("The resources reload: {what} swords", &[("what", what.to_string())]));
    }

    /// The button «Open the folder»: the models' folder in the file manager, made first; its place said either
    /// way (the folder may not open: Wine's file manager is its own).
    pub(super) fn swords_folder(&mut self) {
        let Some(dir) = folder(self) else {
            self.toast(i18n::tr("The game's place is not known yet: open the menu in a world").to_string());
            return;
        };
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.to_string_lossy().into_owned();
        open_folder(&dir);
        self.toast(i18n::trn("The models' folder: {path} — a model is a folder with model.json in it (Blockbench: Java Item Model), its pictures in textures/ beside it", &[("path", path)]));
    }
}

/// The folder in the file manager (Wine's own under it; nothing when it is not there).
fn open_folder(path: &std::path::Path) {
    use std::os::windows::ffi::OsStrExt;
    let op: Vec<u16> = "open\0".encode_utf16().collect();
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    // SW_SHOWNORMAL; over 32 is a success (an HINSTANCE greater than that)
    unsafe {
        windows_sys::Win32::UI::Shell::ShellExecuteW(std::ptr::null_mut(), op.as_ptr(), wide.as_ptr(), std::ptr::null(), std::ptr::null(), 1);
    }
}
