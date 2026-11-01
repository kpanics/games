//! A cloud config is a complete collection, including configs of individual parts and their
//! server bindings. Stage it before touching the live files; keep a restorable local copy.
use super::super::configs;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const PREFIX: &str = "RCDSYNC1-";
const MAX_CODE: usize = 250 * 1024;
const MAX_TEXT: usize = 4 * 1024 * 1024;
static NEXT: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Saved {
    pub file: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Bundle {
    pub menu: String,
    pub saved: Vec<Saved>,
    #[serde(default)]
    pub active: Option<String>,
}

fn bad(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what)
}

fn safe_file(name: &str) -> bool {
    if !name.ends_with(".cfg")
        || name.len() < 5
        || name.len() > 120
        || name.starts_with('.')
        || name
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*'))
    {
        return false;
    }
    let word = name.split('.').next().unwrap_or("").trim_end_matches([' ', '.']).to_uppercase();
    let port = word.strip_prefix("COM").or_else(|| word.strip_prefix("LPT"));
    let device = port.is_some_and(|tail| tail.chars().count() == 1 && tail.chars().all(|c| matches!(c, '0'..='9' | '¹' | '²' | '³')));
    !matches!(word.as_str(), "" | "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$" | "CLOCK$")
        && !device
        && !name.trim_end_matches(".cfg").ends_with([' ', '.'])
}

impl Bundle {
    pub fn validate(&self) -> io::Result<()> {
        if self.menu.is_empty() || self.menu.len() > 1024 * 1024 || self.saved.len() > 100 {
            return Err(bad("cloud config size"));
        }
        if self.active.as_ref().is_some_and(|n| n.len() > 160 || n.chars().any(char::is_control)) {
            return Err(bad("cloud config name"));
        }
        let mut names = HashSet::new();
        let mut size = self.menu.len();
        for f in &self.saved {
            if !safe_file(&f.file) || f.text.len() > 1024 * 1024 || !names.insert(f.file.to_lowercase()) {
                return Err(bad("cloud config file"));
            }
            size += f.text.len() + f.file.len();
        }
        if size > MAX_TEXT {
            return Err(bad("cloud config size"));
        }
        struct Count(usize);
        impl Write for Count {
            fn write(&mut self, b: &[u8]) -> io::Result<usize> {
                if self.0 + b.len() > MAX_TEXT {
                    return Err(bad("cloud config size"));
                }
                self.0 += b.len();
                Ok(b.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        // The decompression bound applies to JSON bytes too, including escaped text.
        serde_json::to_writer(&mut Count(0), self).map_err(|_| bad("cloud config size"))?;
        Ok(())
    }

    pub fn encode(&self) -> io::Result<String> {
        self.validate()?;
        let code = configs::squeeze(PREFIX, &serde_json::to_string(self).map_err(io::Error::other)?);
        if code.len() > MAX_CODE {
            return Err(bad("cloud config size"));
        }
        Ok(code)
    }

    pub fn decode(code: &str) -> io::Result<Self> {
        if code.len() > MAX_CODE {
            return Err(bad("cloud config size"));
        }
        let text = configs::unsqueeze(PREFIX, code).ok_or_else(|| bad("cloud config format"))?;
        let out: Self = serde_json::from_str(&text).map_err(io::Error::other)?;
        out.validate()?;
        Ok(out)
    }

    pub fn capture(dir: &Path, menu: String, active: Option<String>) -> io::Result<Self> {
        let base = base(dir)?;
        let folder = base.join("configs");
        let mut saved = Vec::new();
        if folder.exists() {
            plain_dir(&folder)?;
            for e in fs::read_dir(&folder)? {
                let e = e?;
                let name = e.file_name().to_string_lossy().into_owned();
                if !safe_file(&name) {
                    continue;
                }
                let m = fs::symlink_metadata(e.path())?;
                if !m.is_file() || linked(&m) {
                    continue;
                }
                if m.len() > 1024 * 1024 {
                    return Err(bad("cloud config file size"));
                }
                saved.push(Saved {
                    file: name,
                    text: fs::read_to_string(e.path())?,
                });
            }
        }
        saved.sort_by(|a, b| a.file.cmp(&b.file));
        let out = Self { menu, saved, active };
        out.validate()?;
        Ok(out)
    }
}

fn linked(m: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        m.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        m.file_type().is_symlink()
    }
}

fn plain_dir(path: &Path) -> io::Result<()> {
    let m = fs::symlink_metadata(path)?;
    if !m.is_dir() || linked(&m) {
        return Err(bad("config directory is a link"));
    }
    Ok(())
}

fn base(dir: &Path) -> io::Result<PathBuf> {
    let game = dir.canonicalize()?;
    let app = game.join("cursed-recode");
    if !app.exists() {
        fs::create_dir(&app)?;
    }
    plain_dir(&app)?;
    let app = app.canonicalize()?;
    if !app.starts_with(&game) {
        return Err(bad("config directory is outside the game"));
    }
    Ok(app)
}

fn folder(parent: &Path, name: &str) -> io::Result<PathBuf> {
    let path = parent.join(name);
    if !path.exists() {
        fs::create_dir(&path)?;
    }
    plain_dir(&path)?;
    if !path.canonicalize()?.starts_with(parent.canonicalize()?) {
        return Err(bad("config directory escaped"));
    }
    Ok(path)
}

fn unique() -> String {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("{ms}-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed))
}

fn new_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut f = OpenOptions::new().write(true).create_new(true).open(path)?;
    f.write_all(bytes)?;
    f.sync_all()
}

/// Check both absolute endpoints before moving a directory. No link is traversed, and no
/// existing destination is replaced unless it is the menu file explicitly being committed.
fn move_inside(app: &Path, from: &Path, to: &Path, replace_file: bool) -> io::Result<()> {
    let m = fs::symlink_metadata(from)?;
    if linked(&m) || !from.canonicalize()?.starts_with(app) || !to.parent().ok_or_else(|| bad("config path"))?.canonicalize()?.starts_with(app) {
        return Err(bad("config move is outside the game"));
    }
    if let Ok(dest) = fs::symlink_metadata(to) {
        if !replace_file || !m.is_file() || !dest.is_file() || linked(&dest) {
            return Err(bad("config destination exists"));
        }
    }
    fs::rename(from, to)
}

pub(super) fn metadata(dir: &Path, name: &str, bytes: &[u8]) -> io::Result<()> {
    if name.is_empty() || name.chars().any(|c| !c.is_ascii_alphanumeric() && !matches!(c, '-' | '.')) {
        return Err(bad("metadata name"));
    }
    let app = base(dir)?;
    let tmp = app.join(format!(".cloud-write-{}", unique()));
    new_file(&tmp, bytes)?;
    let answer = move_inside(&app, &tmp, &app.join(name), true);
    if answer.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    answer
}

pub(super) fn read_metadata(dir: &Path, name: &str) -> io::Result<Vec<u8>> {
    if name.is_empty() || name.chars().any(|c| !c.is_ascii_alphanumeric() && !matches!(c, '-' | '.')) {
        return Err(bad("metadata name"));
    }
    let path = base(dir)?.join(name);
    let m = fs::symlink_metadata(&path)?;
    if !m.is_file() || linked(&m) || m.len() > MAX_TEXT as u64 {
        return Err(bad("metadata file"));
    }
    fs::read(path)
}

#[derive(Clone, Serialize, Deserialize)]
struct Pending {
    backup: String,
    stage: String,
    had_configs: bool,
}

fn own_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 100 && id.bytes().all(|c| c.is_ascii_digit() || c == b'-')
}

#[derive(Clone)]
pub(in crate::gui) struct Backup {
    pub id: String,
    pub at: u64,
}

fn read_bundle(path: &Path) -> io::Result<Bundle> {
    let m = fs::symlink_metadata(path)?;
    if !m.is_file() || linked(&m) || m.len() > (MAX_TEXT + 128 * 1024) as u64 {
        return Err(bad("local cloud copy"));
    }
    let b: Bundle = serde_json::from_slice(&fs::read(path)?).map_err(io::Error::other)?;
    b.validate()?;
    Ok(b)
}

pub(super) fn backups(dir: &Path) -> io::Result<Vec<Backup>> {
    let app = base(dir)?;
    let root = folder(&app, "cloud-backups")?;
    let mut out = Vec::new();
    for e in fs::read_dir(&root)? {
        let e = e?;
        let id = e.file_name().to_string_lossy().into_owned();
        if !own_id(&id) || plain_dir(&e.path()).is_err() || read_bundle(&e.path().join("bundle.json")).is_err() {
            continue;
        }
        let at = id.split('-').next().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0) / 1000;
        out.push(Backup { id, at });
    }
    out.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| b.id.cmp(&a.id)));
    Ok(out)
}

pub(super) fn read_backup(dir: &Path, id: &str) -> io::Result<Bundle> {
    if !own_id(id) {
        return Err(bad("local cloud copy"));
    }
    let app = base(dir)?;
    let root = folder(&app, "cloud-backups")?;
    let path = root.join(id);
    plain_dir(&path)?;
    read_bundle(&path.join("bundle.json"))
}

fn keep_backup(app: &Path, before: &Bundle) -> io::Result<(String, PathBuf)> {
    let root = folder(app, "cloud-backups")?;
    let id = unique();
    let path = root.join(&id);
    fs::create_dir(&path)?;
    new_file(&path.join("bundle.json"), &serde_json::to_vec(before).map_err(io::Error::other)?)?;
    Ok((id, path))
}

pub(super) struct Prepared {
    dir: PathBuf,
    app: PathBuf,
    stage: PathBuf,
    backup: PathBuf,
    pending: Pending,
    names: Vec<String>,
}

pub(super) fn prepare(dir: &Path, before: &Bundle, after: &Bundle) -> io::Result<Prepared> {
    before.validate()?;
    after.validate()?;
    let app = base(dir)?;
    if app.join("cloud-pending.json").exists() {
        return Err(bad("unfinished cloud change"));
    }
    let cfg = app.join("configs");
    let had_configs = cfg.exists();
    if had_configs {
        plain_dir(&cfg)?;
    }
    let stage_id = unique();
    let stage = app.join(format!(".cloud-stage-{stage_id}"));
    fs::create_dir(&stage)?;
    fs::create_dir(stage.join("configs"))?;
    let (backup_id, backup) = keep_backup(&app, before)?;
    let out = Prepared {
        dir: dir.to_path_buf(),
        app,
        stage,
        backup,
        pending: Pending {
            backup: backup_id,
            stage: stage_id,
            had_configs,
        },
        names: after.saved.iter().map(|f| f.file.clone()).collect(),
    };
    new_file(&out.stage.join("after.json"), &serde_json::to_vec(after).map_err(io::Error::other)?)?;
    for f in &after.saved {
        new_file(&out.stage.join("configs").join(&f.file), f.text.as_bytes())?;
    }
    new_file(&out.stage.join("menu.txt"), after.menu.as_bytes())?;
    Ok(out)
}

impl Prepared {
    pub fn commit(self) -> io::Result<()> {
        let journal = self.app.join("cloud-pending.json");
        new_file(&journal, &serde_json::to_vec(&self.pending).map_err(io::Error::other)?)?;
        let run = || {
            if self.pending.had_configs {
                move_inside(&self.app, &self.app.join("configs"), &self.backup.join("configs"), false)?;
            }
            move_inside(&self.app, &self.stage.join("configs"), &self.app.join("configs"), false)?;
            move_inside(&self.app, &self.stage.join("menu.txt"), &self.app.join("menu.txt"), true)?;
            fs::remove_file(&journal)
        };
        if let Err(e) = run() {
            let _ = recover(&self.dir);
            return Err(e);
        }
        prune(&self.dir);
        Ok(())
    }
}

impl Drop for Prepared {
    fn drop(&mut self) {
        clean_stage(&self.app, &self.stage, &self.names);
    }
}

fn clean_stage(app: &Path, stage: &Path, names: &[String]) {
    if plain_dir(stage).is_err() || !stage.canonicalize().is_ok_and(|p| p.starts_with(app)) {
        return;
    }
    let cfg = stage.join("configs");
    if plain_dir(&cfg).is_ok() {
        for name in names {
            if safe_file(name) {
                let _ = fs::remove_file(cfg.join(name));
            }
        }
        let _ = fs::remove_dir(cfg);
    }
    for name in ["menu.txt", "after.json"] {
        let _ = fs::remove_file(stage.join(name));
    }
    let _ = fs::remove_dir(stage); // Unknown entries stay intact; never recurse into them.
}

/// Before settings are loaded after a crash: finish rolling back an interrupted replacement.
/// New files are salvaged alongside the backup; none are recursively removed.
pub(super) fn recover(dir: &Path) -> io::Result<()> {
    let app = base(dir)?;
    let journal = app.join("cloud-pending.json");
    if !journal.exists() {
        return Ok(());
    }
    let m = fs::symlink_metadata(&journal)?;
    if linked(&m) || !m.is_file() || m.len() > 4096 {
        return Err(bad("cloud recovery journal"));
    }
    let p: Pending = serde_json::from_slice(&fs::read(&journal)?).map_err(io::Error::other)?;
    if !own_id(&p.backup) || !own_id(&p.stage) {
        return Err(bad("cloud recovery path"));
    }
    let root = folder(&app, "cloud-backups")?;
    let backup = root.join(&p.backup);
    plain_dir(&backup)?;
    let before = read_bundle(&backup.join("bundle.json"))?;
    let cfg = app.join("configs");
    let old_cfg = backup.join("configs");
    if old_cfg.exists() || !p.had_configs {
        if cfg.exists() {
            move_inside(&app, &cfg, &backup.join(format!("interrupted-configs-{}", unique())), false)?;
        }
        if old_cfg.exists() {
            move_inside(&app, &old_cfg, &cfg, false)?;
        } else {
            fs::create_dir(&cfg)?;
        }
    }
    if !cfg.exists() {
        fs::create_dir(&cfg)?;
        for f in &before.saved {
            new_file(&cfg.join(&f.file), f.text.as_bytes())?;
        }
    }
    let menu = app.join("menu.txt");
    if menu.exists() {
        let m = fs::symlink_metadata(&menu)?;
        if linked(&m) || !m.is_file() {
            return Err(bad("menu is a link"));
        }
        // Keep even an interrupted new menu, for manual recovery of changes made before a crash.
        let _ = fs::copy(&menu, backup.join(format!("interrupted-menu-{}.txt", unique())));
    }
    metadata(dir, "menu.txt", before.menu.as_bytes())?;
    fs::remove_file(journal)?;
    let stage = app.join(format!(".cloud-stage-{}", p.stage));
    if let Ok(after) = read_bundle(&stage.join("after.json")) {
        clean_stage(&app, &stage, &after.saved.into_iter().map(|f| f.file).collect::<Vec<_>>());
    }
    Ok(())
}

fn prune(dir: &Path) {
    let Ok(all) = backups(dir) else { return };
    let Ok(app) = base(dir) else { return };
    for b in all.into_iter().skip(10) {
        let at = app.join("cloud-backups").join(b.id);
        if plain_dir(&at).is_err() {
            continue;
        }
        let Ok(entries) = fs::read_dir(&at) else { continue };
        let entries: Vec<_> = entries.flatten().collect();
        if entries.iter().any(|e| e.file_name() != "bundle.json" && e.file_name() != "configs") {
            continue;
        }
        let cfg = at.join("configs");
        let files: Vec<_> = if cfg.exists() {
            if plain_dir(&cfg).is_err() {
                continue;
            }
            let Ok(files) = fs::read_dir(&cfg) else { continue };
            files.flatten().collect()
        } else {
            Vec::new()
        };
        if files
            .iter()
            .any(|e| !safe_file(&e.file_name().to_string_lossy()) || !e.file_type().is_ok_and(|t| t.is_file()))
        {
            continue;
        }
        // These are only the regular files of an owned backup inside the checked game folder.
        for f in files {
            let _ = fs::remove_file(f.path());
        }
        if cfg.exists() {
            let _ = fs::remove_dir(&cfg);
        }
        let _ = fs::remove_file(at.join("bundle.json"));
        let _ = fs::remove_dir(at);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Room {
        path: PathBuf,
        parent: PathBuf,
    }
    impl Room {
        fn new() -> Self {
            let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/cloud-store-tests");
            fs::create_dir_all(&parent).unwrap();
            let parent = parent.canonicalize().unwrap();
            let path = parent.join(format!("case-{}", unique()));
            fs::create_dir(&path).unwrap();
            Self { path, parent }
        }
    }
    impl Drop for Room {
        fn drop(&mut self) {
            let checked = self.path.canonicalize().unwrap();
            assert!(checked.starts_with(&self.parent) && checked.file_name().unwrap().to_string_lossy().starts_with("case-"));
            fs::remove_dir_all(checked).unwrap();
        }
    }
    fn pack(n: u32) -> Bundle {
        Bundle {
            menu: format!("#parts=31\nm.on={n}\n"),
            saved: vec![
                Saved {
                    file: "Combat.cfg".into(),
                    text: format!("#name=Fight\n#parts=1\n#servers=example.invalid\nm.on={n}\n"),
                },
                Saved {
                    file: "Theme.cfg".into(),
                    text: "#parts=4\ntheme.palette=1\n".into(),
                },
            ],
            active: Some("Fight".into()),
        }
    }
    #[test]
    fn every_config_type_round_trips_and_paths_never_enter_the_archive() {
        let a = pack(1);
        assert_eq!(Bundle::decode(&a.encode().unwrap()).unwrap(), a);
        for name in [
            "../x.cfg",
            "C:secret.cfg",
            r"a\x.cfg",
            "CON.cfg",
            "NUL.x.cfg",
            "COM1.cfg",
            "COM¹.cfg",
            "CONOUT$.cfg",
            "x/.cfg",
        ] {
            assert!(!safe_file(name), "{name}");
        }
        let mut duplicate = a.clone();
        duplicate.saved.push(Saved {
            file: "COMBAT.cfg".into(),
            text: "x".into(),
        });
        assert!(duplicate.validate().is_err());
    }
    #[test]
    fn replace_keeps_a_complete_local_copy_and_removes_old_active_config_files() {
        let room = Room::new();
        let old = pack(1);
        let mut new = pack(2);
        new.saved.remove(1);
        let app = base(&room.path).unwrap();
        let cfg = folder(&app, "configs").unwrap();
        for f in &old.saved {
            new_file(&cfg.join(&f.file), f.text.as_bytes()).unwrap();
        }
        new_file(&app.join("menu.txt"), old.menu.as_bytes()).unwrap();
        prepare(&room.path, &old, &new).unwrap().commit().unwrap();
        assert_eq!(
            Bundle::capture(&room.path, fs::read_to_string(app.join("menu.txt")).unwrap(), new.active.clone()).unwrap(),
            new
        );
        let copies = backups(&room.path).unwrap();
        assert_eq!(copies.len(), 1);
        assert_eq!(read_backup(&room.path, &copies[0].id).unwrap(), old);
        assert!(app.join("cloud-backups").join(&copies[0].id).join("configs/Theme.cfg").is_file());
        assert!(!app.join("configs/Theme.cfg").exists());
    }
    #[test]
    fn an_interrupted_directory_switch_is_rolled_back_before_settings_load() {
        let room = Room::new();
        let (old, new) = (pack(1), pack(2));
        let app = base(&room.path).unwrap();
        let cfg = folder(&app, "configs").unwrap();
        for f in &old.saved {
            new_file(&cfg.join(&f.file), f.text.as_bytes()).unwrap();
        }
        new_file(&app.join("menu.txt"), old.menu.as_bytes()).unwrap();
        let pending = prepare(&room.path, &old, &new).unwrap();
        new_file(&app.join("cloud-pending.json"), &serde_json::to_vec(&pending.pending).unwrap()).unwrap();
        move_inside(&app, &cfg, &pending.backup.join("configs"), false).unwrap();
        move_inside(&app, &pending.stage.join("configs"), &cfg, false).unwrap();
        std::mem::forget(pending); // Simulate process termination between the config and menu swaps.
        recover(&room.path).unwrap();
        assert_eq!(
            Bundle::capture(&room.path, fs::read_to_string(app.join("menu.txt")).unwrap(), old.active.clone()).unwrap(),
            old
        );
        assert!(!app.join("cloud-pending.json").exists());
    }

    #[cfg(windows)]
    #[test]
    fn a_locked_menu_leaves_recovery_pending_until_it_can_restore_both_parts() {
        use std::os::windows::fs::OpenOptionsExt;
        let room = Room::new();
        let (old, new) = (pack(1), pack(2));
        let app = base(&room.path).unwrap();
        let cfg = folder(&app, "configs").unwrap();
        for f in &old.saved {
            new_file(&cfg.join(&f.file), f.text.as_bytes()).unwrap();
        }
        new_file(&app.join("menu.txt"), old.menu.as_bytes()).unwrap();
        let lock = OpenOptions::new().read(true).share_mode(1).open(app.join("menu.txt")).unwrap();
        assert!(prepare(&room.path, &old, &new).unwrap().commit().is_err());
        assert!(app.join("cloud-pending.json").is_file());
        drop(lock);
        recover(&room.path).unwrap();
        assert_eq!(
            Bundle::capture(&room.path, std::fs::read_to_string(app.join("menu.txt")).unwrap(), old.active.clone()).unwrap(),
            old
        );
        assert!(!app.join("cloud-pending.json").exists());
    }

    #[test]
    fn escaped_json_respects_the_decompression_bound_and_retention_preserves_unknown_files() {
        let mut large = pack(1);
        large.menu = "\\".repeat(1024 * 1024);
        large.saved = vec![Saved {
            file: "Large.cfg".into(),
            text: "\\".repeat(1024 * 1024),
        }];
        assert!(large.validate().is_err());
        let room = Room::new();
        let app = base(&room.path).unwrap();
        let mut before = pack(0);
        for n in 1..=12 {
            let after = pack(n);
            prepare(&room.path, &before, &after).unwrap().commit().unwrap();
            if n == 1 {
                let oldest = backups(&room.path).unwrap().remove(0);
                new_file(&app.join("cloud-backups").join(oldest.id).join("my-notes.txt"), b"keep").unwrap();
            }
            before = after;
        }
        let all = backups(&room.path).unwrap();
        assert!(all.len() >= 10);
        let protected = std::fs::read_dir(app.join("cloud-backups"))
            .unwrap()
            .flatten()
            .any(|e| e.path().join("my-notes.txt").is_file());
        assert!(protected);
    }
}
