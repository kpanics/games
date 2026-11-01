//! What the loader keeps between starts, all under `<home>/loader`: its settings, the signed-in session (sealed with
//! Windows' own DPAPI, so it opens only for this user on this computer), and what it knows of the files it has laid
//! out (their hashes, so the next start does not read 250 MB again). Where the home is — the folder picked at the
//! first start (her word, 15.10: C:\Cursed Recode unless another is picked) — is kept outside it, in the user's local
//! application data. Cursed's loader's, for Recode.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::Security::Cryptography::{CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB};

/// The game's folder: the one picked (`chosen`), else the default; `CURSED_LOADER_HOME` for tests (never hers).
pub fn home() -> PathBuf {
    if let Some(dir) = std::env::var_os("CURSED_LOADER_HOME").filter(|d| !d.is_empty()) {
        return PathBuf::from(dir);
    }
    chosen().unwrap_or_else(default_home)
}

/// `%SystemDrive%\Cursed Recode`.
pub fn default_home() -> PathBuf {
    let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
    PathBuf::from(format!("{}\\Cursed Recode", drive.trim_end_matches('\\')))
}

/// Where the choice of the folder is kept: `%LOCALAPPDATA%\Cursed Recode\home.txt` (in tests, the test folder's).
fn choice_file() -> PathBuf {
    if let Some(dir) = std::env::var_os("CURSED_LOADER_HOME").filter(|d| !d.is_empty()) {
        return PathBuf::from(dir).join("loader").join("home.txt");
    }
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("Cursed Recode").join("home.txt")
}

/// The folder picked at the first start, if one was.
pub fn chosen() -> Option<PathBuf> {
    let text = std::fs::read_to_string(choice_file()).ok()?;
    let p = PathBuf::from(text.trim());
    p.is_absolute().then_some(p)
}

/// The game's folder from now on.
pub fn choose(home: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(home)?;
    write_atomic(&choice_file(), home.to_string_lossy().as_bytes())
}

pub fn dir() -> PathBuf {
    home().join("loader")
}

/// Written whole or not at all: a half-written file never replaces a good one.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

// ------------------------------------------------------------------------------------------------ settings

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// "ru" or "en".
    pub lang: String,
    /// Memory for the game, megabytes.
    pub memory: u32,
}

pub const MEMORY_LEAST: u32 = 2048;
pub const MEMORY_DEFAULT: u32 = 4096;

impl Settings {
    pub fn load() -> Settings {
        let v: Value = std::fs::read(dir().join("settings.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Value::Null);
        Settings {
            lang: v["lang"].as_str().filter(|l| *l == "ru" || *l == "en").map(str::to_string).unwrap_or_else(system_lang),
            memory: v["memory"].as_u64().map(|m| m as u32).unwrap_or(MEMORY_DEFAULT).clamp(MEMORY_LEAST, memory_most()),
        }
    }

    pub fn save(&self) {
        let _ = write_atomic(&dir().join("settings.json"), json!({ "lang": self.lang, "memory": self.memory }).to_string().as_bytes());
    }
}

/// Russian for a Russian Windows, English for any other.
fn system_lang() -> String {
    let id = unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() };
    // The primary language is the low ten bits; Russian, Ukrainian, Belarusian, Kazakh read Russian.
    if matches!(id & 0x3FF, 0x19 | 0x22 | 0x23 | 0x3F) { "ru".into() } else { "en".into() }
}

/// The computer's memory: (free, total) in megabytes.
pub fn memory_here() -> (u64, u64) {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
    if unsafe { GlobalMemoryStatusEx(&mut status) } == 0 {
        return (8192, 16384);
    }
    (status.ullAvailPhys >> 20, status.ullTotalPhys >> 20)
}

/// The most the game may be given: all but two gigabytes for Windows, and never more than 16.
pub fn memory_most() -> u32 {
    let (_, total) = memory_here();
    (total.saturating_sub(2048) as u32).clamp(MEMORY_LEAST, 16384)
}

// ------------------------------------------------------------------------------------------------ the session

#[derive(Clone, Debug, PartialEq)]
pub struct Session {
    pub token: String,
    pub uid: i64,
    pub login: String,
}

impl Session {
    /// The session kept by "remember me", if there is one and it opens.
    pub fn load() -> Option<Session> {
        let sealed = std::fs::read(dir().join("session")).ok()?;
        let plain = unseal(&sealed)?;
        let v: Value = serde_json::from_slice(&plain).ok()?;
        Some(Session { token: v["token"].as_str()?.to_string(), uid: v["uid"].as_i64()?, login: v["login"].as_str()?.to_string() })
    }

    pub fn save(&self) {
        let plain = json!({ "token": self.token, "uid": self.uid, "login": self.login }).to_string();
        if let Some(sealed) = seal(plain.as_bytes()) {
            let _ = write_atomic(&dir().join("session"), &sealed);
        }
    }

    pub fn forget() {
        let _ = std::fs::remove_file(dir().join("session"));
    }
}

fn blob(bytes: &[u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB { cbData: bytes.len() as u32, pbData: bytes.as_ptr() as *mut u8 }
}

pub fn seal(plain: &[u8]) -> Option<Vec<u8>> {
    unsafe {
        let input = blob(plain);
        let mut output = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
        if CryptProtectData(&input, std::ptr::null(), std::ptr::null(), std::ptr::null(), std::ptr::null(), 0, &mut output) == 0 {
            return None;
        }
        let out = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(output.pbData as _);
        Some(out)
    }
}

pub fn unseal(sealed: &[u8]) -> Option<Vec<u8>> {
    unsafe {
        let input = blob(sealed);
        let mut output = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
        if CryptUnprotectData(&input, std::ptr::null_mut(), std::ptr::null(), std::ptr::null(), std::ptr::null(), 0, &mut output) == 0 {
            return None;
        }
        let out = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(output.pbData as _);
        Some(out)
    }
}

// ------------------------------------------------------------------------------------------------ what is laid out

/// A file as last seen: its size and change time, and the hash it had then. A file whose size and time are the same is
/// taken to be the same file.
#[derive(Clone, Debug, PartialEq)]
pub struct Seen {
    pub size: u64,
    pub mtime: u64,
    pub sha256: String,
}

#[derive(Default)]
pub struct Files {
    pub seen: BTreeMap<String, Seen>,
    /// The paths of the build laid out last: a file that leaves the build is removed (only those, never hers).
    pub installed: Vec<String>,
}

impl Files {
    pub fn load() -> Files {
        let v: Value = std::fs::read(dir().join("files.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Value::Null);
        let mut seen = BTreeMap::new();
        if let Some(map) = v["seen"].as_object() {
            for (path, s) in map {
                if let (Some(size), Some(mtime), Some(sha)) = (s[0].as_u64(), s[1].as_u64(), s[2].as_str()) {
                    seen.insert(path.clone(), Seen { size, mtime, sha256: sha.to_string() });
                }
            }
        }
        let installed = v["installed"].as_array().map(|a| a.iter().filter_map(|p| p.as_str().map(str::to_string)).collect()).unwrap_or_default();
        Files { seen, installed }
    }

    pub fn save(&self) {
        let seen: serde_json::Map<String, Value> = self.seen.iter().map(|(p, s)| (p.clone(), json!([s.size, s.mtime, s.sha256]))).collect();
        let _ = write_atomic(&dir().join("files.json"), json!({ "seen": seen, "installed": self.installed }).to_string().as_bytes());
    }
}

/// A file's size and change time (in seconds), if it is there.
pub fn stat(path: &Path) -> Option<(u64, u64)> {
    let m = std::fs::metadata(path).ok()?;
    let mtime = m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    Some((m.len(), mtime))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sealed_session_opens_only_whole() {
        let sealed = seal(b"secret token").expect("DPAPI");
        assert_ne!(sealed, b"secret token");
        assert_eq!(unseal(&sealed).as_deref(), Some(&b"secret token"[..]));
        let mut broken = sealed.clone();
        let n = broken.len() / 2;
        broken[n] ^= 0x55;
        assert_eq!(unseal(&broken), None);
    }

    #[test]
    fn memory_stays_in_its_bounds() {
        assert!(memory_most() >= MEMORY_LEAST && memory_most() <= 16384);
    }
}
