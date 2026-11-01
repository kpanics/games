//! Alt Manager: the accounts the game is played with — nicknames (what pirate servers take) and Microsoft accounts (a
//! licence: Microsoft's refresh token, sealed for this Windows user by DPAPI, sent nowhere but to Microsoft), the one in
//! use, kept in <game>/cursed-recode/alts.json. The menus (the main menu's and the pause's corner, the client's menu)
//! show the list and change it here; Java puts the account to use (the game's session) and signs in with Microsoft
//! (cursed.recode.alts), asked through `poll`, and says how it goes through `status`. The free Cursed's accounts
//! (C:\Cursed\cursed\accounts.json) can be taken over: its nicknames, and its Microsoft accounts with their keys as they
//! are sealed (the same DPAPI, the same Windows user opens them).

use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

/// How many accounts the list holds.
pub const LIMIT: usize = 24;
const FILE: &str = "alts.json";
/// The free Cursed's accounts (its own folder: C:\Cursed).
const CURSED_FILE: &str = r"C:\Cursed\cursed\accounts.json";

/// An account: its nickname, its Microsoft side (its profile's id, its key sealed, as hex), when it was last played.
#[derive(Clone, Debug, PartialEq)]
pub struct Alt {
    pub name: String,
    pub ms: Option<(String, String)>,
    pub used: u64,
}

/// What Java said last of what it does (a sign-in under way, done, gone wrong): its kind (0 working, 1 done, 2 wrong),
/// the words (English: the menus put them in the game's language), what goes after them as it is (a code, a name),
/// since when (seconds).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Status {
    pub kind: u8,
    pub text: String,
    pub detail: String,
    pub at: f64,
}

struct Store {
    dir: Option<PathBuf>,
    alts: Vec<Alt>,
    active: String,
    launched: String,
    loaded: bool,
    /// What Java is asked to do, a line each: "use <name>", "login".
    asks: Vec<String>,
    status: Status,
    /// Whatever changes the list, a number more (a menu draws again what it shows).
    seq: u64,
    /// The free Cursed's accounts taken over once (lower case): one taken out of the list here is not offered again.
    cursed_seen: Vec<String>,
}

static STORE: Mutex<Store> = Mutex::new(Store {
    dir: None,
    alts: Vec::new(),
    active: String::new(),
    launched: String::new(),
    loaded: false,
    asks: Vec::new(),
    status: Status { kind: 0, text: String::new(), detail: String::new(), at: 0.0 },
    seq: 0,
    cursed_seen: Vec::new(),
});

fn store() -> MutexGuard<'static, Store> {
    STORE.lock().unwrap_or_else(|p| p.into_inner())
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn clock() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}

/// What a server takes as a name: letters, digits and _, 3 to 16 of them.
pub fn valid(name: &str) -> bool {
    (3..=16).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

impl Store {
    fn find(&self, name: &str) -> Option<usize> {
        self.alts.iter().position(|a| a.name.eq_ignore_ascii_case(name))
    }

    fn file(&self) -> Option<PathBuf> {
        self.dir.as_ref().map(|d| d.join("cursed-recode").join(FILE))
    }

    fn save(&mut self) {
        self.seq += 1;
        let Some(f) = self.file() else { return };
        let list: Vec<Value> = self
            .alts
            .iter()
            .map(|a| {
                let mut o = json!({ "name": a.name, "used": a.used });
                if let Some((uuid, key)) = &a.ms {
                    o["uuid"] = json!(uuid);
                    o["key"] = json!(key);
                }
                o
            })
            .collect();
        let text = serde_json::to_string_pretty(&json!({ "active": self.active, "accounts": list, "cursed_seen": self.cursed_seen })).unwrap_or_default();
        if let Some(parent) = f.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        // written aside, then put in its place: a write cut off (the game killed) leaves the list as it was
        let tmp = f.with_extension("json.tmp");
        if std::fs::write(&tmp, text).is_ok() && std::fs::rename(&tmp, &f).is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
    }

    /// The account to use now: kept, said to Java.
    fn choose(&mut self, i: usize) {
        self.alts[i].used = now();
        self.active = self.alts[i].name.clone();
        self.asks.push(format!("use {}", self.active));
        self.save();
    }
}

/// Read once, as the game starts (`launched` the name the loader started it with, `dir` the game's folder): the
/// account to put to use when it is not that one.
pub fn load(launched: &str, dir: &str) -> Option<String> {
    let mut s = store();
    if s.loaded {
        return None;
    }
    s.loaded = true;
    s.launched = launched.to_string();
    if !dir.is_empty() {
        s.dir = Some(PathBuf::from(dir));
    }
    if let Some(text) = s.file().and_then(|f| std::fs::read_to_string(f).ok()) {
        let parsed = serde_json::from_str::<Value>(&text);
        if parsed.is_err() {
            // kept aside as it is (its keys with it): the next save starts a new list
            if let Some(f) = s.file() {
                let _ = std::fs::rename(&f, f.with_extension(format!("broken-{}.json", now())));
            }
        }
        if let Ok(v) = parsed {
            for a in v["accounts"].as_array().into_iter().flatten() {
                let name = a["name"].as_str().unwrap_or("");
                if !valid(name) || s.find(name).is_some() || s.alts.len() >= LIMIT {
                    continue;
                }
                let ms = match (a["uuid"].as_str(), a["key"].as_str()) {
                    (Some(u), Some(k)) if !u.is_empty() && !k.is_empty() => Some((u.to_string(), k.to_string())),
                    _ => None,
                };
                s.alts.push(Alt { name: name.to_string(), ms, used: a["used"].as_u64().unwrap_or(0) });
            }
            if let Some(active) = v["active"].as_str() {
                if let Some(i) = s.find(active) {
                    s.active = s.alts[i].name.clone();
                }
            }
            s.cursed_seen = v["cursed_seen"].as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_lowercase).collect();
        }
    }
    // the name the game was started with is always there
    if valid(launched) && s.find(launched).is_none() && s.alts.len() < LIMIT {
        s.alts.insert(0, Alt { name: launched.to_string(), ms: None, used: 0 });
    }
    if s.active.is_empty() {
        s.active = launched.to_string();
    }
    s.seq += 1;
    (s.active != launched).then(|| s.active.clone())
}

pub fn list() -> Vec<Alt> {
    store().alts.clone()
}

pub fn active() -> String {
    store().active.clone()
}

pub fn seq() -> u64 {
    store().seq
}

pub fn status() -> Status {
    store().status.clone()
}

/// A nickname added and put to use; what is wrong with it otherwise.
pub fn add(name: &str) -> Result<(), &'static str> {
    let name = name.trim();
    if !valid(name) {
        return Err(if name.len() < 3 || name.len() > 16 { "3 to 16 letters" } else { "Only Latin letters, digits and _" });
    }
    let mut s = store();
    if let Some(i) = s.find(name) {
        s.choose(i);
        return Ok(());
    }
    if s.alts.len() >= LIMIT {
        return Err("The list is full");
    }
    s.alts.push(Alt { name: name.to_string(), ms: None, used: 0 });
    let i = s.alts.len() - 1;
    s.choose(i);
    Ok(())
}

/// An account of the list put to use.
pub fn use_alt(name: &str) {
    let mut s = store();
    if let Some(i) = s.find(name) {
        if s.alts[i].name != s.active {
            s.choose(i);
        }
    }
}

/// An account out of the list (not the one in use).
pub fn remove(name: &str) -> bool {
    let mut s = store();
    match s.find(name) {
        Some(i) if s.alts[i].name != s.active => {
            s.alts.remove(i);
            s.save();
            true
        }
        _ => false,
    }
}

/// Microsoft's sign-in asked for (Java opens the browser).
pub fn ask_login() {
    let mut s = store();
    if !s.asks.iter().any(|a| a == "login") {
        s.asks.push("login".into());
    }
    s.status = Status { kind: 0, text: "Opening the browser…".into(), detail: String::new(), at: clock() };
    s.seq += 1;
}

/// What Java is asked to do since it last asked, a line each.
pub fn poll() -> String {
    std::mem::take(&mut store().asks).join("\n")
}

pub fn set_status(kind: u8, text: &str, detail: &str) {
    let mut s = store();
    s.status = Status { kind, text: text.to_string(), detail: detail.to_string(), at: clock() };
    s.seq += 1;
}

/// A Microsoft account signed in: listed (its key sealed) and put to use.
pub fn ms_add(name: &str, uuid: &str, refresh: &str) -> Result<(), &'static str> {
    if !valid(name) {
        return Err("The profile's name cannot be played with");
    }
    let key = seal(refresh).ok_or("Windows could not seal the account's key")?;
    let mut s = store();
    let i = match s.find(name) {
        Some(i) => i,
        None => {
            if s.alts.len() >= LIMIT {
                return Err("The list is full");
            }
            s.alts.push(Alt { name: name.to_string(), ms: None, used: 0 });
            s.alts.len() - 1
        }
    };
    s.alts[i].name = name.to_string();
    s.alts[i].ms = Some((uuid.to_string(), key));
    s.choose(i);
    Ok(())
}

/// A Microsoft account's refresh token, opened ("" when there is none or this Windows user cannot open it).
pub fn ms_key(name: &str) -> String {
    let s = store();
    s.find(name).and_then(|i| s.alts[i].ms.as_ref()).and_then(|(_, k)| unseal(k)).unwrap_or_default()
}

/// A Microsoft account's profile id ("" for a nickname).
pub fn ms_uuid(name: &str) -> String {
    let s = store();
    s.find(name).and_then(|i| s.alts[i].ms.as_ref()).map(|(u, _)| u.clone()).unwrap_or_default()
}

/// Microsoft gave a new refresh token: sealed and kept.
pub fn ms_rekey(name: &str, refresh: &str) {
    let Some(key) = seal(refresh) else { return };
    let mut s = store();
    if let Some(i) = s.find(name) {
        if let Some(ms) = s.alts[i].ms.as_mut() {
            ms.1 = key;
            s.save();
        }
    }
}

// ------------------------------------------------------------------------------------------------ the free Cursed's

/// The free Cursed's accounts not in the list yet (from its file `path`): nicknames, and its Microsoft ones (name, id,
/// sealed key).
fn cursed_accounts(path: &Path) -> Vec<Alt> {
    let Ok(text) = std::fs::read_to_string(path) else { return Vec::new() };
    let Ok(v) = serde_json::from_str::<Value>(&text) else { return Vec::new() };
    let mut out: Vec<Alt> = Vec::new();
    let ms: Map<String, Value> = v["microsoft"].as_object().cloned().unwrap_or_default();
    for name in v["accounts"].as_array().into_iter().flatten().filter_map(Value::as_str) {
        if valid(name) && !out.iter().any(|a| a.name.eq_ignore_ascii_case(name)) {
            out.push(Alt { name: name.to_string(), ms: None, used: 0 });
        }
    }
    for (name, m) in ms {
        if let (Some(u), Some(k)) = (m["uuid"].as_str(), m["key"].as_str()) {
            let ms = Some((u.to_string(), k.to_string()));
            match out.iter_mut().find(|a| a.name.eq_ignore_ascii_case(&name)) {
                Some(a) => a.ms = ms,
                None if valid(&name) => out.push(Alt { name, ms, used: 0 }),
                None => {}
            }
        }
    }
    out
}

/// How many of the free Cursed's accounts would come now: those not in the list yet (as many as it has room for), and
/// Microsoft keys for ones in it without.
pub fn cursed_new() -> usize {
    cursed_new_from(Path::new(CURSED_FILE))
}

fn cursed_new_from(path: &Path) -> usize {
    let s = store();
    let found = cursed_accounts(path);
    let new = found.iter().filter(|a| s.find(&a.name).is_none() && !s.cursed_seen.contains(&a.name.to_lowercase())).count();
    let keys = found.iter().filter(|a| s.find(&a.name).is_some_and(|i| a.ms.is_some() && s.alts[i].ms.is_none())).count();
    new.min(LIMIT.saturating_sub(s.alts.len())) + keys
}

/// The free Cursed's accounts taken over (its Microsoft keys with them); how many came.
pub fn take_cursed() -> usize {
    take_from(Path::new(CURSED_FILE))
}

fn take_from(path: &Path) -> usize {
    let found = cursed_accounts(path);
    let mut s = store();
    let mut n = 0;
    for a in found {
        let low = a.name.to_lowercase();
        match s.find(&a.name) {
            Some(i) => {
                if a.ms.is_some() && s.alts[i].ms.is_none() {
                    s.alts[i].ms = a.ms;
                    n += 1;
                }
            }
            // (one taken before and taken out of the list since stays out)
            None if s.cursed_seen.contains(&low) => continue,
            None if s.alts.len() < LIMIT => {
                s.alts.push(a);
                n += 1;
            }
            None => continue,
        }
        if !s.cursed_seen.contains(&low) {
            s.cursed_seen.push(low);
        }
    }
    s.save();
    n
}

// ------------------------------------------------------------------------------------------------ a random name

/// A nickname that looks like a player's: a word and a word, a name with its year or a few digits, a word with _.
pub fn random() -> String {
    const A: [&str; 32] = [
        "Dark", "Shadow", "Frost", "Night", "Silent", "Wolf", "Fox", "Ghost", "Raven", "Storm", "Blaze", "Void", "Echo", "Nova", "Lunar", "Pixel",
        "Crimson", "Toxic", "Iron", "Rogue", "Neon", "Cold", "Wild", "Lucky", "Mad", "Sly", "Grim", "Swift", "Quiet", "Red", "Black", "Ice",
    ];
    const B: [&str; 24] = [
        "Hunter", "Rider", "Blade", "Walker", "King", "Slayer", "Knight", "Fang", "Byte", "Spark", "Bear", "Hawk", "Tiger", "Ninja", "Pilot", "Ranger",
        "Viper", "Ghost", "Fury", "Shot", "Claw", "Wing", "Storm", "Mage",
    ];
    const NAMES: [&str; 24] = [
        "artem", "kirill", "danya", "nikita", "egor", "maks", "vlad", "sasha", "misha", "dima", "ilya", "roma", "lesha", "vanya", "timur", "denis",
        "nastya", "katya", "masha", "dasha", "sonya", "alina", "polina", "lera",
    ];
    // a few draws from the clock's nanoseconds, stirred
    let mut x = SystemTime::now().duration_since(UNIX_EPOCH).map_or(7, |d| d.as_nanos() as u64) ^ 0x9E37_79B9_7F4A_7C15;
    let mut next = |n: usize| {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        (x % n as u64) as usize
    };
    for _ in 0..16 {
        let name = match next(4) {
            0 => format!("{}{}", A[next(A.len())], B[next(B.len())]),
            1 => format!("{}{}", NAMES[next(NAMES.len())], [2007, 2008, 2009, 2010, 2011, 2012, 228, 1337, 777, 666][next(10)]),
            2 => format!("{}_{}", A[next(A.len())].to_lowercase(), B[next(B.len())].to_lowercase()),
            _ => format!("{}{}", B[next(B.len())], 10 + next(990)),
        };
        if valid(&name) && store().find(&name).is_none() {
            return name;
        }
    }
    format!("Player{}", 100 + next(9900))
}

// ------------------------------------------------------------------------------------------------ the keys sealed

/// For this Windows user alone (DPAPI, as the free Cursed seals its keys: they open here too), as hex.
fn seal(plain: &str) -> Option<String> {
    use windows_sys::Win32::Security::Cryptography::{CryptProtectData, CRYPT_INTEGER_BLOB};
    let data = plain.as_bytes();
    unsafe {
        let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
        let mut output = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
        if CryptProtectData(&input, std::ptr::null(), std::ptr::null(), std::ptr::null(), std::ptr::null(), 0, &mut output) == 0 {
            return None;
        }
        let out = std::slice::from_raw_parts(output.pbData, output.cbData as usize).iter().map(|b| format!("{b:02x}")).collect();
        windows_sys::Win32::Foundation::LocalFree(output.pbData as _);
        Some(out)
    }
}

fn unseal(hex: &str) -> Option<String> {
    use windows_sys::Win32::Security::Cryptography::{CryptUnprotectData, CRYPT_INTEGER_BLOB};
    if hex.len() % 2 != 0 {
        return None;
    }
    let data: Vec<u8> = (0..hex.len()).step_by(2).map(|i| hex.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok())).collect::<Option<_>>()?;
    unsafe {
        let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
        let mut output = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
        if CryptUnprotectData(&input, std::ptr::null_mut(), std::ptr::null(), std::ptr::null(), std::ptr::null(), 0, &mut output) == 0 {
            return None;
        }
        let out = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        windows_sys::Win32::Foundation::LocalFree(output.pbData as _);
        String::from_utf8(out).ok()
    }
}

// ------------------------------------------------------------------------------------------------ Java's words

/// Java's calls (gui call "alts"): "load <launched>\t<dir>" (the account to put to use, "" the one started with),
/// "poll" (what it is asked to do), "status <0|1|2>\t<words>\t<detail>", "list" (JSON: the accounts, the one in use),
/// "mskey <name>", "msuuid <name>", "msadd <name>\t<uuid>\t<refresh>" ("" or what is wrong), "msrekey <name>\t<refresh>".
pub fn call(verb: &str, rest: &str) -> String {
    let mut f = rest.split('\t');
    let mut arg = || f.next().unwrap_or("").to_string();
    match verb {
        "load" => {
            let (launched, dir) = (arg(), arg());
            load(&launched, &dir).unwrap_or_default()
        }
        "poll" => poll(),
        "status" => {
            let (kind, text, detail) = (arg(), arg(), arg());
            set_status(kind.parse().unwrap_or(0), &text, &detail);
            String::new()
        }
        "list" => {
            let s = store();
            let list: Vec<Value> = s.alts.iter().map(|a| json!({ "name": a.name, "ms": a.ms.is_some(), "uuid": a.ms.as_ref().map_or("", |m| &m.0) })).collect();
            json!({ "active": s.active, "accounts": list }).to_string()
        }
        // a word in the game's language (the sign-in's page in the browser), the key itself when it has none
        "word" => {
            let key = arg();
            if crate::menus::lang::known(&key) {
                crate::menus::lang::tr(&key).to_string()
            } else {
                key
            }
        }
        "mskey" => ms_key(&arg()),
        "msuuid" => ms_uuid(&arg()),
        "msadd" => {
            let (name, uuid, refresh) = (arg(), arg(), arg());
            ms_add(&name, &uuid, &refresh).err().unwrap_or("").to_string()
        }
        "msrekey" => {
            let (name, refresh) = (arg(), arg());
            ms_rekey(&name, &refresh);
            String::new()
        }
        _ => String::new(),
    }
}

#[cfg(test)]
pub(crate) fn reset_for_test() {
    let mut s = store();
    *s = Store { dir: None, alts: Vec::new(), active: String::new(), launched: String::new(), loaded: false, asks: Vec::new(), status: Status::default(), seq: 0, cursed_seen: Vec::new() };
}

#[cfg(test)]
pub(crate) static TEST_LOCK: Mutex<()> = Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_a_server_takes() {
        assert!(valid("Lunuca") && valid("a_b") && valid("Player_123456789"));
        assert!(!valid("ab") && !valid("toolongname_123456") && !valid("Лунука") && !valid("a b") && !valid("a-b"));
    }

    #[test]
    fn the_list_kept_and_put_to_use() {
        let _l = TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        reset_for_test();
        let dir = std::env::temp_dir().join(format!("recode-alts-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(load("Lunuca", dir.to_str().unwrap()), None);
        assert_eq!(active(), "Lunuca");
        assert!(add("ab").is_err());
        assert!(add("Lunuca_pvp").is_ok());
        assert_eq!(active(), "Lunuca_pvp");
        assert_eq!(poll(), "use Lunuca_pvp");
        assert_eq!(poll(), "");
        // the one in use stays, another goes
        assert!(!remove("Lunuca_pvp"));
        use_alt("lunuca");
        assert_eq!(active(), "Lunuca");
        assert!(remove("Lunuca_pvp"));
        add("Steve228").unwrap();
        // read again: the one in use is put to use at the start
        reset_for_test();
        assert_eq!(load("Lunuca", dir.to_str().unwrap()), Some("Steve228".into()));
        assert_eq!(list().iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), vec!["Lunuca", "Steve228"]);
        let _ = std::fs::remove_dir_all(&dir);
        reset_for_test();
    }

    #[test]
    fn keys_sealed_open_again() {
        let s = seal("refresh-token-123").unwrap();
        assert!(!s.contains("refresh"));
        assert_eq!(unseal(&s).as_deref(), Some("refresh-token-123"));
        assert_eq!(unseal("zz"), None);
        // not hex, a letter cut in two: no key, no panic
        assert_eq!(unseal("aéb"), None);
    }

    #[test]
    fn the_free_cursed_taken_over() {
        let _l = TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        reset_for_test();
        let dir = std::env::temp_dir().join(format!("recode-alts-cursed-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let key = seal("ms-refresh").unwrap();
        let file = dir.join("accounts.json");
        std::fs::write(&file, json!({ "active": "Two", "accounts": ["One", "Two", "x"], "microsoft": { "Notch": { "uuid": "069a79f444e94726a5befca90e38aaf5", "key": key } } }).to_string()).unwrap();
        load("One", "");
        assert_eq!(cursed_new_from(&file), 2);
        assert_eq!(take_from(&file), 2);
        let names: Vec<String> = list().into_iter().map(|a| a.name).collect();
        assert_eq!(names, vec!["One", "Two", "Notch"]);
        assert_eq!(ms_key("notch"), "ms-refresh");
        assert_eq!(ms_uuid("Notch"), "069a79f444e94726a5befca90e38aaf5");
        // again: nothing more; one taken out here is not offered again
        assert_eq!(take_from(&file), 0);
        assert!(remove("Two"));
        assert_eq!(take_from(&file), 0);
        assert!(list().iter().all(|a| a.name != "Two"));
        assert_eq!(cursed_new_from(&file), 0);
        // a full list: the new ones are not counted (nor taken) till there is room
        for n in 0..LIMIT - 2 {
            add(&format!("Filler{n}")).unwrap();
        }
        std::fs::write(&file, json!({ "accounts": ["One", "Three"] }).to_string()).unwrap();
        assert_eq!(list().len(), LIMIT);
        assert_eq!(cursed_new_from(&file), 0);
        assert_eq!(take_from(&file), 0);
        assert!(remove("Filler0"));
        assert_eq!(cursed_new_from(&file), 1);
        assert_eq!(take_from(&file), 1);
        let _ = std::fs::remove_dir_all(&dir);
        reset_for_test();
    }

    #[test]
    fn a_list_that_cannot_be_read_is_kept_aside() {
        let _l = TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        reset_for_test();
        let dir = std::env::temp_dir().join(format!("recode-alts-broken-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = dir.join("cursed-recode").join(FILE);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "{\"active\": \"Ste").unwrap();
        assert_eq!(load("Lunuca", dir.to_str().unwrap()), None);
        add("Steve228").unwrap();
        let kept: Vec<String> = std::fs::read_dir(file.parent().unwrap()).unwrap().filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert!(kept.iter().any(|n| n.starts_with("alts.broken-")), "{kept:?}");
        // the new list is whole, nothing left aside half written
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(v["active"], "Steve228");
        assert!(!kept.iter().any(|n| n.ends_with(".tmp")));
        let _ = std::fs::remove_dir_all(&dir);
        reset_for_test();
    }

    #[test]
    fn random_names_are_names() {
        let _l = TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        for _ in 0..200 {
            let n = random();
            assert!(valid(&n), "{n}");
        }
    }
}
