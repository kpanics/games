//! The loader in its place: the first start (from Downloads, wherever) copies it to `<home>\Cursed Recode.exe`, puts a
//! shortcut on the desktop and starts it from there. A newer loader in the build is fetched next to it, the running
//! one steps aside (Windows lets a running program be renamed), and the new one starts. No installer. Cursed's
//! loader's, for Recode (the folder is the one picked at the first start).
//!
//! `CURSED_LOADER_DEV=1` runs it where it is (tests); with `CURSED_LOADER_HOME` the shortcut goes into that folder,
//! never onto her desktop.

use std::path::{Path, PathBuf};

use super::store;
use super::sync::{self, Entry, Manifest};

pub const EXE: &str = "Cursed Recode.exe";

fn dev() -> bool {
    std::env::var("CURSED_LOADER_DEV").is_ok_and(|v| v == "1")
}

pub fn installed_path() -> PathBuf {
    store::home().join(EXE)
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

/// Leftovers of the last update.
pub fn cleanup() {
    let _ = std::fs::remove_file(installed_path().with_extension("exe.old"));
    let _ = std::fs::remove_file(installed_path().with_extension("exe.new"));
}

/// The loader this one replaced is still running (its file could not be removed yet).
pub fn old_in_use() -> bool {
    installed_path().with_extension("exe.old").exists()
}

/// Makes sure the loader runs from its place. True when it has just put itself there and started that copy (this
/// one should end).
pub fn ensure_installed() -> bool {
    if dev() {
        return false;
    }
    let Ok(me) = std::env::current_exe() else { return false };
    let target = installed_path();
    if same_file(&me, &target) {
        return false;
    }
    if std::fs::create_dir_all(store::home()).is_err() {
        return false;
    }
    // A copy of the same loader there already is fine; another one is replaced by this one.
    let same = sync::sha256_file(&me).is_some() && sync::sha256_file(&me) == sync::sha256_file(&target);
    if !same {
        let tmp = target.with_extension("exe.new");
        if std::fs::copy(&me, &tmp).is_err() {
            return false;
        }
        if target.exists() {
            let _ = std::fs::rename(&target, target.with_extension("exe.old"));
        }
        if std::fs::rename(&tmp, &target).is_err() {
            return false;
        }
    }
    let _ = shortcut(&target);
    // (this one is still running as that one starts: it waits for it to go, as after an update)
    std::process::Command::new(&target).arg("--updated").current_dir(store::home()).spawn().is_ok()
}

/// A newer loader in this build, if this is the installed one (a loader run from elsewhere, in tests, never
/// replaces itself).
pub fn wants_update(m: &Manifest) -> Option<Entry> {
    if dev() {
        return None;
    }
    let me = std::env::current_exe().ok()?;
    if !same_file(&me, &installed_path()) {
        return None;
    }
    sync::loader_update(m)
}

pub fn fetch_update(entry: &Entry, token: &str) -> Result<PathBuf, String> {
    let new = installed_path().with_extension("exe.new");
    sync::fetch_loader(entry, token, &new)?;
    Ok(new)
}

/// The running loader steps aside for the new one, which starts.
pub fn swap_and_restart(new: &Path) -> Result<(), String> {
    let target = installed_path();
    let old = target.with_extension("exe.old");
    let _ = std::fs::remove_file(&old);
    std::fs::rename(&target, &old).map_err(|e| e.to_string())?;
    if let Err(e) = std::fs::rename(new, &target) {
        let _ = std::fs::rename(&old, &target);
        return Err(e.to_string());
    }
    std::process::Command::new(&target).arg("--updated").current_dir(store::home()).spawn().map_err(|e| e.to_string())?;
    Ok(())
}

/// Where the shortcut goes: her desktop, or the test folder.
fn desktop() -> Option<PathBuf> {
    if std::env::var_os("CURSED_LOADER_HOME").is_some() {
        return Some(store::home().join("Desktop"));
    }
    unsafe {
        use windows::Win32::UI::Shell::{FOLDERID_Desktop, SHGetKnownFolderPath, KF_FLAG_DEFAULT};
        let path = SHGetKnownFolderPath(&FOLDERID_Desktop, KF_FLAG_DEFAULT, None).ok()?;
        let text = path.to_string().ok();
        windows::Win32::System::Com::CoTaskMemFree(Some(path.0 as *const _));
        text.map(PathBuf::from)
    }
}

/// "Cursed Recode" on the desktop, pointing at the installed loader.
pub fn shortcut(target: &Path) -> Result<(), String> {
    use windows::core::{Interface, HSTRING};
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, IPersistFile, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
    let dir = desktop().ok_or("desktop")?;
    let _ = std::fs::create_dir_all(&dir);
    let lnk = dir.join("Cursed Recode.lnk");
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).map_err(|e| e.to_string())?;
        let exe = HSTRING::from(target.to_string_lossy().as_ref());
        link.SetPath(&exe).map_err(|e| e.to_string())?;
        link.SetWorkingDirectory(&HSTRING::from(store::home().to_string_lossy().as_ref())).map_err(|e| e.to_string())?;
        link.SetDescription(&HSTRING::from("Cursed Recode")).map_err(|e| e.to_string())?;
        link.SetIconLocation(&exe, 0).map_err(|e| e.to_string())?;
        let file: IPersistFile = link.cast().map_err(|e| e.to_string())?;
        file.Save(&HSTRING::from(lnk.to_string_lossy().as_ref()), true).map_err(|e| e.to_string())?;
    }
    Ok(())
}
