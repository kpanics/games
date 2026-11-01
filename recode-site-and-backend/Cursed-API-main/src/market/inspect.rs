//! What an uploaded pack is, whether it is safe, what it changes, and the clean copy the site serves.
//!
//! The site never hands out the bytes it was given. Every file is taken out, looked at for what it claims to be
//! (a PNG is decoded to its last row, a sound must be Ogg, a font a font), and only what belongs in a pack goes into a
//! new archive. Refused outright:
//! - programs (Windows, Linux, macOS, Java), by their name or by their first bytes: a pack has no business carrying one;
//! - paths that leave the pack;
//! - links;
//! - the zip tricks zip.rs catches.
//! Left out, with a word about it: scripts and archives inside (shader packs carry their authors' tools), junk (macOS
//! forks, Thumbs.db, editor files), files the game never reads, pictures that don't decode, names Minecraft can't
//! address, single files over 128 MB. Kept wherever they lie: licences, readmes and credits. Fixed, with a word too: a
//! pack zipped inside a folder, and backslashes in names.
//!
//! What it changes is read from where its files are: blocks, items, the interface, sounds, fonts, OptiFine's
//! connected textures and emissive layers…; its resolution from its block textures; its game versions from
//! pack.mcmeta (format.rs). A shader pack: its programs, dimensions, options, profiles, languages, and what its
//! options say it does (shadows, volumetric light, reflections…).

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs::File;

use serde_json::{json, Value};

use super::format;
use super::zip::{self, Bad, Entry, Limits};

pub const MAX_ARCHIVE: u64 = 250 * 1024 * 1024;
pub const MAX_UNPACKED: u64 = 1024 * 1024 * 1024;
pub const MAX_FILES: usize = 30_000;
const MAX_FILE: u64 = 128 * 1024 * 1024;
const MAX_TEXT: u64 = 8 * 1024 * 1024;
const MAX_SIDE: u32 = 8192;
const MAX_PIXELS: u64 = 64 * 1024 * 1024;
const BIG_TEXTURE: u32 = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Resources,
    Shaders,
}

impl Kind {
    pub fn word(self) -> &'static str {
        match self {
            Kind::Resources => "resourcepack",
            Kind::Shaders => "shaderpack",
        }
    }
}

/// Why a pack was refused: what, and where in it.
#[derive(Debug, Clone, PartialEq)]
pub struct Refusal {
    pub message: String,
    pub path: Option<String>,
}

impl From<Bad> for Refusal {
    fn from(b: Bad) -> Self {
        Refusal { message: b.0, path: None }
    }
}

fn refuse<T>(message: &str, path: Option<&str>) -> Result<T, Refusal> {
    Err(Refusal { message: message.to_string(), path: path.map(str::to_string) })
}

#[derive(Debug)]
pub struct Checked {
    pub kind: Kind,
    pub facts: Value,
    pub title: String,
    pub icon: Option<Vec<u8>>, // pack.png, for the card when there are no pictures yet
    pub size: u64,             // of the clean archive
}

// ------------------------------------------------------------------------------------------------ names

enum Name {
    Folder,
    Fine(String, bool), // the name; whether backslashes had to become slashes
    Unreadable,         // not UTF-8: no resource path can be made of it
    Leaves(String),     // .., an absolute path, a drive
    Windows(String),    // a name Windows can't have (CON, a trailing dot…): useless and a trap when unzipped
}

fn name_of(raw: &[u8]) -> Name {
    let Ok(text) = std::str::from_utf8(raw) else {
        return Name::Unreadable;
    };
    let fixed = text.contains('\\');
    let mut s = text.replace('\\', "/");
    while let Some(rest) = s.strip_prefix("./") {
        s = rest.to_string();
    }
    if s.ends_with('/') {
        return Name::Folder;
    }
    if s.starts_with('/') || s.chars().nth(1) == Some(':') || s.chars().any(|c| c.is_control()) {
        return Name::Leaves(s);
    }
    let parts: Vec<&str> = s.split('/').collect();
    if parts.iter().any(|p| p.is_empty() || *p == "." || *p == "..") || parts.len() > 32 || s.len() > 240 {
        return Name::Leaves(s);
    }
    const RESERVED: &[&str] = &["con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9"];
    if parts.iter().any(|p| p.ends_with('.') || p.ends_with(' ') || RESERVED.contains(&p.split('.').next().unwrap_or("").to_ascii_lowercase().as_str()) || p.contains(['<', '>', '"', '|', '?', '*', ':'])) {
        return Name::Windows(s);
    }
    Name::Fine(s, fixed)
}

fn extension(name: &str) -> String {
    let base = name.rsplit('/').next().unwrap_or(name);
    match base.rfind('.') {
        Some(i) if i > 0 => base[i + 1..].to_ascii_lowercase(),
        _ => String::new(),
    }
}

/// Junk that rides along in archives: dropped without a second thought.
fn junk(name: &str) -> bool {
    let base = name.rsplit('/').next().unwrap_or(name);
    name.split('/').any(|p| matches!(p, "__MACOSX" | ".git" | ".svn" | ".hg" | ".idea" | ".vscode" | ".github") || p.starts_with("._"))
        || matches!(base, ".DS_Store" | "Thumbs.db" | "desktop.ini" | ".gitignore" | ".gitattributes" | ".gitmodules" | ".editorconfig")
        || matches!(extension(name).as_str(), "psd" | "psb" | "xcf" | "kra" | "ase" | "aseprite" | "pdn" | "sai" | "clip" | "blend" | "blend1" | "bbmodel" | "tmp" | "bak" | "log" | "url")
}

/// Programs. One refuses the whole pack: a pack has no reason to carry one.
fn program(name: &str) -> bool {
    matches!(
        extension(name).as_str(),
        "exe" | "dll" | "com" | "scr" | "pif" | "msi" | "msp" | "hta" | "lnk" | "jar" | "class" | "apk" | "app" | "dmg" | "pkg" | "deb" | "rpm" | "appimage" | "reg"
            | "cpl" | "msc" | "sys" | "drv" | "ocx" | "so" | "dylib" | "elf" | "iso" | "img" | "vhd" | "vhdx" | "inf" | "gadget" | "xll" | "docm" | "xlsm" | "pptm"
    )
}

/// Scripts: a shader pack's author may leave a tool in it. Left out: the clean copy never carries one.
fn script(name: &str) -> bool {
    matches!(
        extension(name).as_str(),
        "bat" | "cmd" | "ps1" | "psm1" | "psd1" | "vbs" | "vbe" | "js" | "jse" | "mjs" | "wsf" | "wsh" | "sh" | "bash" | "zsh" | "fish" | "command" | "py" | "pyw" | "pyc"
            | "pl" | "rb" | "php" | "run" | "lua"
    )
}

/// Licences, readmes, credits: kept wherever they lie (a pack shared without its licence may not be shared at all).
fn doc(name: &str) -> bool {
    let base = name.rsplit('/').next().unwrap_or(name);
    let stem = base.split('.').next().unwrap_or(base).to_ascii_uppercase();
    matches!(extension(name).as_str(), "txt" | "md" | "readme")
        || matches!(stem.as_str(), "LICENSE" | "LICENCE" | "COPYING" | "README" | "CREDITS" | "NOTICE" | "AUTHORS" | "CHANGELOG")
}

fn archive_name(name: &str) -> bool {
    matches!(extension(name).as_str(), "zip" | "rar" | "7z" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "zst" | "cab" | "mcpack" | "mcaddon" | "mrpack" | "war" | "ear")
}

/// What the first bytes say, whatever the name says: a program (refused), or an archive or a script (left out).
/// `raw`: a shader pack's raw texture (.dat, .bin), random bytes that may start like an archive by chance: only
/// programs count there.
fn sniff(data: &[u8], raw: bool) -> Option<&'static str> {
    let at = |i: usize, sig: &[u8]| data.len() >= i + sig.len() && &data[i..i + sig.len()] == sig;
    if at(0, b"MZ") && data.len() >= 64 {
        let pe = u32::from_le_bytes([data[60], data[61], data[62], data[63]]) as usize;
        if at(pe, b"PE\0\0") {
            return Some("a Windows program");
        }
    }
    if at(0, b"\x7fELF") {
        return Some("a Linux program");
    }
    if at(0, &[0xCA, 0xFE, 0xBA, 0xBE]) || at(0, &[0xCF, 0xFA, 0xED, 0xFE]) || at(0, &[0xCE, 0xFA, 0xED, 0xFE]) {
        return Some("a program (Java class or macOS)");
    }
    if raw {
        return None;
    }
    if at(0, b"PK\x03\x04") || at(0, b"PK\x05\x06") || at(0, b"Rar!\x1a\x07") || at(0, &[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C]) || at(0, &[0x1F, 0x8B]) || at(0, b"MSCF") || at(257, b"ustar") {
        return Some("an archive");
    }
    if at(0, b"#!") {
        return Some("a script");
    }
    None
}

// ---------------------------------------------------------------------------------------------- contents

fn is_text(data: &[u8]) -> bool {
    !data.contains(&0)
}

/// JSON the way Minecraft's reader forgives it: comments and trailing commas out, a BOM off.
fn relaxed_json(data: &[u8]) -> Option<Value> {
    let text = std::str::from_utf8(data.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(data)).ok()?;
    if let Ok(v) = serde_json::from_str(text) {
        return Some(v);
    }
    let mut out = String::with_capacity(text.len());
    let b = text.as_bytes();
    let (mut i, mut in_string) = (0, false);
    while i < b.len() {
        let c = b[i];
        if in_string {
            out.push(c as char);
            if c == b'\\' && i + 1 < b.len() {
                out.push(b[i + 1] as char);
                i += 1;
            } else if c == b'"' {
                in_string = false;
            }
        } else if c == b'"' {
            in_string = true;
            out.push('"');
        } else if c == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        } else if c == b'/' && b.get(i + 1) == Some(&b'*') {
            i += 2;
            while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                i += 1;
            }
            i += 2;
            continue;
        } else if c == b',' {
            let next = b[i + 1..].iter().find(|x| !x.is_ascii_whitespace());
            if matches!(next, Some(b'}') | Some(b']')) {
                i += 1;
                continue;
            }
            out.push(',');
        } else {
            out.push(c as char);
        }
        i += 1;
    }
    // Bytes were pushed one by one: rebuild the UTF-8 before parsing.
    let bytes: Vec<u8> = out.chars().map(|c| c as u32 as u8).collect();
    serde_json::from_slice(&bytes).ok()
}

/// A PNG decoded to its last row (row by row: never the whole picture in memory). Its size, or what is wrong.
fn png_size(data: &[u8]) -> Result<(u32, u32), String> {
    if !data.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("not a PNG".into());
    }
    let dec = png::Decoder::new_with_limits(std::io::Cursor::new(data), png::Limits { bytes: 256 << 20 });
    let mut r = dec.read_info().map_err(|e| format!("damaged ({e})"))?;
    let (w, h) = (r.info().width, r.info().height);
    if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE || w as u64 * h as u64 > MAX_PIXELS {
        return Err(format!("{w}×{h} is too large"));
    }
    while r.next_row().map_err(|e| format!("damaged ({e})"))?.is_some() {}
    Ok((w, h))
}

/// Plain words of a text component (pack.mcmeta's description): strings, "text", "extra", arrays; § codes out.
fn plain(v: &Value) -> String {
    let mut s = String::new();
    fn walk(v: &Value, s: &mut String) {
        match v {
            Value::String(t) => s.push_str(t),
            Value::Array(a) => a.iter().for_each(|x| walk(x, s)),
            Value::Object(o) => {
                if let Some(t) = o.get("text") {
                    walk(t, s);
                } else if let Some(t) = o.get("translate") {
                    walk(t, s);
                }
                if let Some(e) = o.get("extra") {
                    walk(e, s);
                }
            }
            Value::Number(n) => s.push_str(&n.to_string()),
            Value::Bool(b) => s.push_str(if *b { "true" } else { "false" }),
            Value::Null => {}
        }
    }
    walk(v, &mut s);
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '§' {
            chars.next();
        } else {
            out.push(c);
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(300).collect()
}

/// A name for the pack from its file's name: "Ashfall_v1.4-[32x].zip" → "Ashfall v1.4 [32x]".
fn title_from(file_name: &str) -> String {
    let stem = file_name.rsplit(['/', '\\']).next().unwrap_or(file_name);
    let stem = stem.strip_suffix(".zip").or_else(|| stem.strip_suffix(".ZIP")).unwrap_or(stem);
    let words: String = stem.chars().map(|c| if c == '_' || c == '+' { ' ' } else { c }).collect();
    let t: String = words.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(48).collect();
    if t.is_empty() { "Untitled pack".into() } else { t }
}

fn resource_path_ok(rest: &str) -> bool {
    rest.bytes().all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'/' | b'.' | b'_' | b'-'))
}

fn namespace_ok(ns: &str) -> bool {
    !ns.is_empty() && ns.bytes().all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'-'))
}

// ------------------------------------------------------------------------------------------------ notes

/// What was fixed, left out and noticed, for the author.
#[derive(Default)]
struct Notes {
    fixed: BTreeSet<String>,
    dropped: BTreeMap<&'static str, Vec<String>>, // why → which
    warnings: Vec<String>,
}

impl Notes {
    fn drop(&mut self, why: &'static str, name: &str) {
        self.dropped.entry(why).or_default().push(name.to_string());
    }

    fn json(&self) -> Value {
        let dropped: serde_json::Map<String, Value> = self
            .dropped
            .iter()
            .map(|(why, names)| (why.to_string(), json!({ "count": names.len(), "some": names.iter().take(12).collect::<Vec<_>>() })))
            .collect();
        json!({ "fixed": self.fixed, "dropped": dropped, "warnings": self.warnings })
    }
}

// ------------------------------------------------------------------------------------------------ the check

/// The pack in `src` (its `len` bytes, uploaded as `file_name`): refused, or checked with its clean copy in `out`.
pub fn check(src: &File, len: u64, file_name: &str, out: File) -> Result<Checked, Refusal> {
    if len > MAX_ARCHIVE {
        return refuse(&format!("the archive is bigger than {} MB", MAX_ARCHIVE >> 20), None);
    }
    let list = zip::directory(src, len, &Limits { entries: MAX_FILES + 20_000, directory: 32 << 20, name: 1024 })?;
    let mut notes = Notes::default();

    // Every name: refused, dropped, or kept.
    let mut kept: Vec<(String, &Entry)> = Vec::new();
    let mut seen = HashSet::new();
    let mut unpacked = 0u64;
    for e in &list {
        if e.symlink {
            return refuse("a link to something outside the pack", Some(&String::from_utf8_lossy(&e.name)));
        }
        let name = match name_of(&e.name) {
            Name::Folder => continue,
            Name::Unreadable => {
                notes.drop("names that aren't readable text", &String::from_utf8_lossy(&e.name));
                continue;
            }
            Name::Leaves(n) => return refuse("a path that leads out of the pack", Some(&n)),
            Name::Windows(n) => {
                notes.drop("names Windows can't have", &n);
                continue;
            }
            Name::Fine(n, fixed) => {
                if fixed {
                    notes.fixed.insert("names used \\ instead of /".into());
                }
                n
            }
        };
        if junk(&name) {
            notes.drop("junk (system and editor files)", &name);
            continue;
        }
        if program(&name) {
            return refuse("a program inside the pack", Some(&name));
        }
        if script(&name) {
            notes.drop("scripts", &name);
            continue;
        }
        if archive_name(&name) {
            notes.drop("archives inside the pack", &name);
            continue;
        }
        if !seen.insert(name.clone()) {
            return refuse("two files with the same name", Some(&name));
        }
        if e.size > MAX_FILE {
            notes.drop("single files over 128 MB", &name);
            continue;
        }
        unpacked += e.size;
        if unpacked > MAX_UNPACKED {
            return refuse(&format!("the pack is bigger than {} MB unpacked", MAX_UNPACKED >> 20), None);
        }
        kept.push((name, e));
    }
    if kept.len() > MAX_FILES {
        return refuse(&format!("more than {MAX_FILES} files"), None);
    }

    // Zipped with its folder around it (the most common mistake): the folder comes off.
    let has = |kept: &Vec<(String, &Entry)>, n: &str| kept.iter().any(|(k, _)| k == n);
    let has_dir = |kept: &Vec<(String, &Entry)>, d: &str| kept.iter().any(|(k, _)| k.starts_with(d));
    if !has(&kept, "pack.mcmeta") && !has_dir(&kept, "shaders/") {
        let tops: BTreeSet<&str> = kept.iter().filter(|(k, _)| k.contains('/')).map(|(k, _)| k.split('/').next().unwrap_or("")).collect();
        let loose_docs = kept.iter().filter(|(k, _)| !k.contains('/')).all(|(k, _)| matches!(extension(k).as_str(), "txt" | "md"));
        if tops.len() == 1 && loose_docs {
            let top = format!("{}/", tops.iter().next().unwrap());
            if has(&kept, &format!("{top}pack.mcmeta")) || has_dir(&kept, &format!("{top}shaders/")) {
                notes.fixed.insert(format!("the pack was inside a folder ({})", top.trim_end_matches('/')));
                for (k, _) in kept.iter_mut() {
                    if let Some(rest) = k.strip_prefix(&top) {
                        *k = rest.to_string();
                    }
                }
            }
        }
    }

    let shader_program = |k: &str| {
        let Some(rest) = k.strip_prefix("shaders/") else { return false };
        let file = rest.rsplit('/').next().unwrap_or(rest);
        matches!(extension(file).as_str(), "fsh" | "vsh" | "gsh" | "csh")
            && ["gbuffers_", "composite", "deferred", "final", "shadow", "prepare", "begin", "setup"].iter().any(|p| file.starts_with(p))
    };
    let kind = if has(&kept, "shaders/shaders.properties") || kept.iter().any(|(k, _)| shader_program(k)) {
        Kind::Shaders
    } else if has(&kept, "pack.mcmeta") {
        Kind::Resources
    } else {
        return refuse("this is neither a resource pack (no pack.mcmeta at its top) nor a shader pack (no shaders folder)", None);
    };

    kept.sort_by(|a, b| a.0.cmp(&b.0));
    let mut w = zip::Writer::new(out);
    let title = title_from(file_name);
    let result = match kind {
        Kind::Resources => resources(src, &kept, &mut w, &mut notes),
        Kind::Shaders => shaders(src, &kept, &mut w, &mut notes),
    };
    let (mut facts, icon) = result?;
    if w.count() == 0 {
        return refuse("nothing in the pack is something the game reads", None);
    }
    let files = w.count();
    let size = w.finish().map_err(|e| Refusal { message: format!("the clean copy couldn't be written ({e})"), path: None })?;
    let o = facts.as_object_mut().expect("facts are an object");
    o.insert("kind".into(), json!(kind.word()));
    o.insert("files".into(), json!(files));
    o.insert("unpacked".into(), json!(unpacked));
    o.insert("archive".into(), json!(size));
    o.insert("uploaded".into(), json!(len));
    if let Value::Object(n) = notes.json() {
        o.extend(n);
    }
    Ok(Checked { kind, facts, title, icon, size })
}

/// Checks a kept file for what its name says it is; Err(why) leaves it out.
fn content(ext: &str, data: &[u8], pngs: &mut Vec<(u32, u32)>) -> Result<(), &'static str> {
    match ext {
        "png" => match png_size(data) {
            Ok(wh) => {
                pngs.push(wh);
                Ok(())
            }
            Err(_) => Err("pictures that don't decode"),
        },
        "ogg" => if data.starts_with(b"OggS") { Ok(()) } else { Err("sounds that aren't Ogg") },
        "ttf" | "otf" => {
            if data.starts_with(&[0, 1, 0, 0]) || data.starts_with(b"OTTO") || data.starts_with(b"true") || data.starts_with(b"ttcf") {
                Ok(())
            } else {
                Err("fonts that aren't fonts")
            }
        }
        "dat" | "bin" => Ok(()),
        _ => {
            if data.len() as u64 > MAX_TEXT {
                Err("text files over 8 MB")
            } else if is_text(data) {
                Ok(())
            } else {
                Err("text files that are binary inside")
            }
        }
    }
}

type Found = (Value, Option<Vec<u8>>);

fn io_refusal(e: std::io::Error) -> Refusal {
    Refusal { message: format!("the clean copy couldn't be written ({e})"), path: None }
}

fn resources(src: &File, kept: &[(String, &Entry)], w: &mut zip::Writer, notes: &mut Notes) -> Result<Found, Refusal> {
    // pack.mcmeta first: without it read there is nothing to go on.
    let meta_entry = kept.iter().find(|(k, _)| k == "pack.mcmeta").map(|(_, e)| *e).expect("checked by the caller");
    let meta_bytes = zip::read(src, meta_entry)?;
    let Some(meta) = relaxed_json(&meta_bytes) else {
        return refuse("pack.mcmeta isn't JSON the game can read", Some("pack.mcmeta"));
    };
    let Some(pack) = meta.get("pack").filter(|p| p.is_object()) else {
        return refuse("pack.mcmeta has no \"pack\" section", Some("pack.mcmeta"));
    };
    let description = pack.get("description").map(plain).unwrap_or_default();
    let game = match format::declared(pack) {
        Ok((min, max)) => json!({ "min": min.to_string(), "max": max.to_string(), "versions": format::versions(min, max), "fits": format::fits(min, max), "problem": null }),
        Err(problem) => json!({ "min": null, "max": null, "versions": null, "fits": false, "problem": problem }),
    };
    if game["fits"] == json!(false) {
        notes.warnings.push(match game["versions"].as_str() {
            Some(v) => format!("made for {v}: Minecraft {} will say it's for another version (most of it usually still works)", format::GAME_NAME),
            None => format!("pack.mcmeta doesn't say its versions the way {} wants: {}", format::GAME_NAME, game["problem"].as_str().unwrap_or("")),
        });
    }
    // Overlays (1.20.2+): more folders of assets, each for some versions.
    let overlays: Vec<String> = meta
        .pointer("/overlays/entries")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|x| x.get("directory").and_then(Value::as_str)).filter(|d| namespace_ok(d)).map(str::to_string).collect())
        .unwrap_or_default();

    let mut namespaces = BTreeSet::new();
    let mut changes = BTreeSet::new();
    let (mut textures, mut sounds, mut animated, mut big, mut big_bytes, mut bad_json) = (0usize, 0usize, 0usize, 0usize, 0u64, Vec::new());
    let mut block_widths: BTreeMap<u32, usize> = BTreeMap::new();
    let mut item_widths: BTreeMap<u32, usize> = BTreeMap::new();
    let mut icon = None;

    for (name, e) in kept {
        let ext = extension(name);
        // Where it is: the root, assets/<namespace>/…, or an overlay's assets.
        let (in_assets, ns, rest) = {
            let mut p = name.as_str();
            if let Some((top, after)) = p.split_once('/') {
                if overlays.iter().any(|o| o == top) {
                    p = after;
                }
            }
            match p.strip_prefix("assets/").and_then(|r| r.split_once('/')) {
                Some((ns, rest)) => (true, ns.to_string(), rest.to_string()),
                None => (false, String::new(), p.to_string()),
            }
        };
        if doc(name) {
            // A licence or a readme: kept wherever it lies.
        } else if !in_assets {
            let root_ok = name == "pack.mcmeta" || name == "pack.png";
            if !root_ok {
                notes.drop(if name.contains('/') { "files outside assets/ (the game doesn't look there)" } else { "loose files at the top" }, name);
                continue;
            }
        } else {
            if !namespace_ok(&ns) {
                notes.drop("folders in assets/ with names the game can't use", name);
                continue;
            }
            let allowed = matches!(ext.as_str(), "png" | "mcmeta" | "json" | "ogg" | "fsh" | "vsh" | "glsl" | "txt" | "properties" | "jem" | "jpm" | "ttf" | "otf" | "lang");
            if !allowed {
                notes.drop("files the game never reads", name);
                continue;
            }
            if !resource_path_ok(&rest) {
                notes.drop("names the game can't use (capital letters, spaces…)", name);
                continue;
            }
        }
        if !matches!(ext.as_str(), "png" | "ogg" | "ttf" | "otf" | "dat" | "bin") && e.size > MAX_TEXT {
            notes.drop("text files over 8 MB", name);
            continue;
        }
        let data = zip::read(src, e).map_err(|b| Refusal { message: b.0, path: Some(name.clone()) })?;
        match sniff(&data, matches!(ext.as_str(), "dat" | "bin")) {
            Some(what) if what.contains("program") => return refuse(&format!("{what} inside the pack, named like something else"), Some(name)),
            Some("an archive") => {
                notes.drop("archives inside the pack", name);
                continue;
            }
            Some(_) => {
                notes.drop("scripts", name);
                continue;
            }
            None => {}
        }
        let mut pngs = Vec::new();
        if let Err(why) = content(&ext, &data, &mut pngs) {
            notes.drop(why, name);
            continue;
        }
        if matches!(ext.as_str(), "json" | "mcmeta" | "jem" | "jpm") && name != "pack.mcmeta" && relaxed_json(&data).is_none() {
            bad_json.push(name.clone());
        }
        if name == "pack.png" {
            icon = Some(data.clone());
        }
        if in_assets {
            namespaces.insert(ns.clone());
            let r = rest.as_str();
            let under = |p: &str| r.starts_with(p);
            if ext == "png" {
                textures += 1;
                if let Some(&(pw, ph)) = pngs.first() {
                    if pw.max(ph) > BIG_TEXTURE {
                        big += 1;
                        big_bytes += data.len() as u64;
                    }
                    if under("textures/block/") {
                        *block_widths.entry(pw).or_default() += 1;
                    } else if under("textures/item/") {
                        *item_widths.entry(pw).or_default() += 1;
                    }
                }
            }
            if ext == "ogg" {
                sounds += 1;
            }
            if name.ends_with(".png.mcmeta") && data.windows(9).any(|x| x == b"animation") {
                animated += 1;
            }
            let area = if under("textures/block/") {
                Some("blocks")
            } else if under("textures/item/") {
                Some("items")
            } else if under("textures/gui/") || under("optifine/gui/") {
                Some("gui")
            } else if under("sounds/") || r == "sounds.json" {
                Some("sounds")
            } else if under("font/") || under("textures/font/") {
                Some("fonts")
            } else if under("optifine/ctm/") {
                Some("ctm")
            } else if r == "optifine/emissive.properties" {
                Some("emissive")
            } else if under("textures/entity/equipment/") || under("textures/models/armor/") {
                Some("armor")
            } else if under("textures/entity/") {
                Some("entities")
            } else if under("textures/particle/") || under("particles/") {
                Some("particles")
            } else if under("textures/environment/") || under("optifine/sky/") || under("mcpatcher/sky/") {
                Some("sky")
            } else if under("textures/painting/") {
                Some("paintings")
            } else if under("textures/mob_effect/") {
                Some("effects")
            } else if under("textures/map/") {
                Some("maps")
            } else if under("textures/colormap/") || under("optifine/colormap/") {
                Some("colours")
            } else if under("models/") || under("blockstates/") || under("items/") || under("equipment/") {
                Some("models")
            } else if under("lang/") {
                Some("languages")
            } else if under("texts/") {
                Some("texts")
            } else if under("shaders/") || under("post_effect/") {
                Some("shaders")
            } else if under("optifine/cit/") || under("mcpatcher/cit/") {
                Some("cit")
            } else if under("optifine/cem/") || under("optifine/random/") || under("optifine/mob/") {
                Some("cem")
            } else if under("optifine/anim/") {
                Some("animated")
            } else {
                None
            };
            if let Some(a) = area {
                changes.insert(a);
            }
        }
        let squeeze = ext != "ogg"; // PNGs too: many packs save them barely compressed
        w.add(name, &data, squeeze).map_err(io_refusal)?;
    }
    if animated > 0 {
        changes.insert("animated");
    }
    if !bad_json.is_empty() {
        notes.warnings.push(format!("{} JSON files the game may not read: {}", bad_json.len(), bad_json.iter().take(5).cloned().collect::<Vec<_>>().join(", ")));
    }
    if namespaces.is_empty() {
        notes.warnings.push("there's nothing in assets/: the pack changes nothing in the game".into());
    }
    // Its resolution: the width most of its block textures have (items, if it has no blocks).
    let widths = if block_widths.is_empty() { &item_widths } else { &block_widths };
    let resolution = widths.iter().max_by_key(|(w, n)| (**n, **w)).map(|(w, _)| format!("{w}x"));
    if big > 0 {
        notes.warnings.push(format!("{big} textures over {BIG_TEXTURE} px: {} MB of the pack", big_bytes / 1_000_000));
    }
    let facts = json!({
        "description": description,
        "game": game,
        "namespaces": namespaces,
        "overlays": overlays,
        "changes": changes,
        "resolution": resolution,
        "textures": textures,
        "sounds": sounds,
        "animated": animated,
        "big": { "count": big, "bytes": big_bytes },
    });
    Ok((facts, icon))
}

fn shaders(src: &File, kept: &[(String, &Entry)], w: &mut zip::Writer, notes: &mut Notes) -> Result<Found, Refusal> {
    let mut programs = BTreeSet::new();
    let mut dimensions = BTreeSet::new();
    let mut options = BTreeSet::new();
    let mut profiles = BTreeSet::new();
    let mut languages = BTreeSet::new();
    let mut iris = false;

    for (name, e) in kept {
        let ext = extension(name);
        match name.strip_prefix("shaders/") {
            _ if doc(name) => {}
            None => {
                {
                    notes.drop(if name.contains('/') { "folders outside shaders/ (the game doesn't look there)" } else { "loose files at the top" }, name);
                    continue;
                }
            }
            Some(_) => {
                let allowed = matches!(ext.as_str(), "fsh" | "vsh" | "gsh" | "csh" | "tcs" | "tes" | "glsl" | "inc" | "properties" | "lang" | "txt" | "md" | "png" | "mcmeta" | "json" | "dat" | "bin");
                if !allowed {
                    notes.drop("files the game never reads", name);
                    continue;
                }
            }
        }
        if !matches!(ext.as_str(), "png" | "ogg" | "ttf" | "otf" | "dat" | "bin") && e.size > MAX_TEXT {
            notes.drop("text files over 8 MB", name);
            continue;
        }
        let data = zip::read(src, e).map_err(|b| Refusal { message: b.0, path: Some(name.clone()) })?;
        match sniff(&data, matches!(ext.as_str(), "dat" | "bin")) {
            Some(what) if what.contains("program") => return refuse(&format!("{what} inside the pack, named like something else"), Some(name)),
            Some("an archive") => {
                notes.drop("archives inside the pack", name);
                continue;
            }
            Some(_) => {
                notes.drop("scripts", name);
                continue;
            }
            None => {}
        }
        let mut pngs = Vec::new();
        if let Err(why) = content(&ext, &data, &mut pngs) {
            notes.drop(why, name);
            continue;
        }
        if let Some(rest) = name.strip_prefix("shaders/") {
            let file = rest.rsplit('/').next().unwrap_or(rest);
            let dir = if rest.contains('/') { rest.split('/').next().unwrap_or("") } else { "" };
            if matches!(ext.as_str(), "fsh" | "vsh" | "gsh" | "csh") && (dir.is_empty() || dir.starts_with("world")) {
                programs.insert(file.rsplit_once('.').map_or(file, |(s, _)| s).to_string());
                dimensions.insert(match dir {
                    "" | "world0" => "overworld",
                    "world-1" => "nether",
                    "world1" => "end",
                    _ => "other",
                });
            }
            if ext == "lang" && dir == "lang" {
                languages.insert(file.trim_end_matches(".lang").to_ascii_lowercase());
            }
            let text = String::from_utf8_lossy(&data);
            if matches!(ext.as_str(), "glsl" | "fsh" | "vsh" | "gsh" | "csh" | "inc") {
                // OptiFine's options: #define NAME value // [a b c], or a plain #define switched from the menu.
                for line in text.lines() {
                    let l = line.trim_start().trim_start_matches("//").trim_start();
                    if let Some(def) = l.strip_prefix("#define ") {
                        // A switch (no value) with a comment, or a value with a list of choices in its comment.
                        let (head, comment) = match def.split_once("//") {
                            Some((h, c)) => (h, Some(c)),
                            None => (def, None),
                        };
                        let mut parts = head.split_whitespace();
                        if let (Some(n), Some(c)) = (parts.next(), comment) {
                            let named = n.chars().all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_');
                            if named && (parts.next().is_none() || c.contains('[')) {
                                options.insert(n.to_string());
                            }
                        }
                    }
                }
            }
            if rest == "shaders.properties" {
                for line in text.lines() {
                    let l = line.trim();
                    if let Some(p) = l.strip_prefix("profile.") {
                        if let Some((n, _)) = p.split_once('=') {
                            profiles.insert(n.trim().to_string());
                        }
                    }
                    if l.starts_with("iris.features.required") {
                        iris = true;
                    }
                }
            }
        }
        let squeeze = true;
        w.add(name, &data, squeeze).map_err(io_refusal)?;
    }
    if programs.is_empty() {
        return refuse("the shaders folder has no shader programs in it", None);
    }
    // What it does, as its programs and its options' names say.
    let mut features = BTreeSet::new();
    if programs.iter().any(|p| p.starts_with("shadow")) {
        features.insert("Shadows");
    }
    if programs.contains("gbuffers_water") {
        features.insert("Water");
    }
    let said = |words: &[&str]| options.iter().any(|o| o.split('_').any(|t| words.contains(&t)));
    for (feature, words) in [
        ("Volumetric light", &["VL", "VOLUMETRIC", "GODRAYS", "GODRAY", "LIGHTSHAFTS"][..]),
        ("Reflections", &["REFLECTION", "REFLECTIONS", "SSR"]),
        ("Clouds", &["CLOUDS", "CLOUD"]),
        ("Bloom", &["BLOOM"]),
        ("Depth of field", &["DOF"]),
        ("Motion blur", &["MOTIONBLUR", "BLUR"]),
        ("Anti-aliasing", &["TAA", "FXAA", "SMAA", "AA"]),
        ("Waving plants", &["WAVING"]),
        ("PBR", &["PBR", "NORMALMAP", "SPECULAR", "LABPBR"]),
        ("Ambient occlusion", &["AO", "SSAO"]),
        ("Aurora", &["AURORA"]),
        ("Wet surfaces", &["PUDDLES", "PUDDLE", "WET"]),
        ("Colored light", &["COLORED", "COLOURED", "FLOODFILL"]),
    ] {
        if said(words) {
            features.insert(feature);
        }
    }
    if iris {
        notes.warnings.push("made for Iris: parts of it may not run on OptiFine".into());
    }
    let facts = json!({
        "programs": programs.len(),
        "dimensions": dimensions,
        "features": features,
        "options": options.len(),
        "profiles": profiles,
        "languages": languages,
        "iris": iris,
    });
    Ok((facts, None))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market::zip::tests::{temp, Raw};
    use std::io::{Read, Seek};

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, w, h);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut wr = enc.write_header().unwrap();
            wr.write_image_data(&vec![120u8; (w * h * 4) as usize]).unwrap();
        }
        out
    }

    fn run(files: &[(&str, &[u8])]) -> Result<(Checked, Vec<(String, Vec<u8>)>), Refusal> {
        let mut r = Raw::new();
        for (n, d) in files {
            r.file(n, d, true, None, None, 20, 0);
        }
        let z = r.finish();
        let src = temp(&z);
        let out = temp(&[]);
        let c = check(&src, z.len() as u64, "My_Pack_v1.zip", out.try_clone().unwrap())?;
        let mut bytes = Vec::new();
        let mut o = out;
        o.rewind().unwrap();
        o.read_to_end(&mut bytes).unwrap();
        let f = temp(&bytes);
        let list = zip::directory(&f, bytes.len() as u64, &zip::tests::LIMITS).unwrap();
        let files = list.iter().map(|e| (String::from_utf8(e.name.clone()).unwrap(), zip::read(&f, e).unwrap())).collect();
        Ok((c, files))
    }

    const META: &[u8] = r#"{"pack": {"pack_format": 34, "supported_formats": [34, 46], "description": "§6Ashfall §7— stone"}}"#.as_bytes();

    #[test]
    fn a_resource_pack_is_read_and_cleaned() {
        let stone = png(32, 32);
        let (c, files) = run(&[
            ("Ashfall/pack.mcmeta", META),
            ("Ashfall/pack.png", &png(64, 64)),
            ("Ashfall/assets/minecraft/textures/block/stone.png", &stone),
            ("Ashfall/assets/minecraft/textures/block/dirt.png", &stone),
            ("Ashfall/assets/minecraft/textures/item/stick.png", &png(16, 16)),
            ("Ashfall/assets/minecraft/textures/gui/widgets.png", &png(256, 256)),
            ("Ashfall/assets/minecraft/optifine/emissive.properties", b"suffix.emissive=_e"),
            ("Ashfall/assets/minecraft/textures/block/Stone Copy.png", &stone),
            ("Ashfall/assets/minecraft/textures/block/stone.psd", b"8BPS"),
            ("__MACOSX/Ashfall/._pack.mcmeta", b"junk"),
            ("Ashfall/Thumbs.db", b"junk"),
        ])
        .unwrap();
        assert_eq!(c.kind, Kind::Resources);
        assert_eq!(c.title, "My Pack v1");
        let f = &c.facts;
        assert_eq!(f["resolution"], "32x");
        assert_eq!(f["description"], "Ashfall — stone");
        assert_eq!(f["game"]["versions"], "1.21–1.21.4");
        assert_eq!(f["game"]["fits"], false);
        assert_eq!(f["changes"], json!(["blocks", "emissive", "gui", "items"]));
        assert!(f["fixed"][0].as_str().unwrap().contains("inside a folder"));
        let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["assets/minecraft/optifine/emissive.properties", "assets/minecraft/textures/block/dirt.png", "assets/minecraft/textures/block/stone.png", "assets/minecraft/textures/gui/widgets.png", "assets/minecraft/textures/item/stick.png", "pack.mcmeta", "pack.png"]);
        assert_eq!(files[2].1, stone);
        assert!(c.icon.is_some());
        assert!(f["dropped"].as_object().unwrap().len() >= 2);
    }

    #[test]
    fn programs_refuse_the_pack() {
        let mut pe = b"MZ".to_vec();
        pe.resize(128, 0);
        pe[60] = 64;
        pe[64..68].copy_from_slice(b"PE\0\0");
        for (name, body) in [("assets/minecraft/run.exe", &b"x"[..]), ("assets/minecraft/sounds/a.ogg", &pe[..]), ("assets/minecraft/textures/stone.png", &pe[..])] {
            let err = run(&[("pack.mcmeta", META), (name, body)]).unwrap_err();
            assert!(err.message.contains("program"), "{name}: {}", err.message);
        }
    }

    #[test]
    fn scripts_and_archives_are_left_out_licences_kept() {
        let (c, files) = run(&[
            ("pack.mcmeta", META),
            ("tools/build.py", b"print(1)"),
            ("assets/minecraft/textures/a.zip", b"PK\x03\x04"),
            ("assets/minecraft/lang/x.json", b"PK\x03\x04 dressed as json"),
            ("LICENSE", b"MIT"),
            ("assets/mymod/README.md", b"hi"),
        ])
        .unwrap();
        let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["LICENSE", "assets/mymod/README.md", "pack.mcmeta"]);
        assert_eq!(c.facts["dropped"]["scripts"]["count"], 1);
        assert_eq!(c.facts["dropped"]["archives inside the pack"]["count"], 2);
    }

    #[test]
    fn a_path_out_of_the_pack_is_refused() {
        let err = run(&[("pack.mcmeta", META), ("../../evil.png", &png(16, 16))]).unwrap_err();
        assert!(err.message.contains("leads out"));
        let err = run(&[("pack.mcmeta", META), ("/etc/cron.d/x", b"x")]).unwrap_err();
        assert!(err.message.contains("leads out"));
    }

    #[test]
    fn a_fake_png_is_left_out() {
        let (c, files) = run(&[("pack.mcmeta", META), ("assets/minecraft/textures/block/stone.png", b"\x89PNG\r\n\x1a\nnot really")]).unwrap();
        assert_eq!(files.len(), 1);
        assert!(c.facts["dropped"]["pictures that don't decode"]["count"] == 1);
    }

    #[test]
    fn a_huge_png_is_left_out() {
        // A tiny file that claims 20000×20000 pixels: a picture bomb.
        let mut bomb = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut bomb, 20000, 1);
            enc.set_color(png::ColorType::Grayscale);
            let mut wr = enc.write_header().unwrap();
            wr.write_image_data(&vec![0u8; 20000]).unwrap();
        }
        let (c, _) = run(&[("pack.mcmeta", META), ("assets/minecraft/textures/block/stone.png", &bomb)]).unwrap();
        assert_eq!(c.facts["dropped"]["pictures that don't decode"]["count"], 1);
    }

    #[test]
    fn a_shader_pack() {
        let (c, files) = run(&[
            ("BSL/shaders/shaders.properties", b"profile.LOW=!SHADOW\nprofile.HIGH=SHADOW\n"),
            ("BSL/shaders/gbuffers_terrain.fsh", b"#version 120\n#define WAVING_PLANTS // wave\n#define VL_STRENGTH 1.0 // [0.5 1.0 2.0]\n"),
            ("BSL/shaders/gbuffers_water.vsh", b"void main() {}"),
            ("BSL/shaders/shadow.vsh", b"void main() {}"),
            ("BSL/shaders/world-1/composite.fsh", b"void main() {}"),
            ("BSL/shaders/lang/ru_RU.lang", b"option.VL=\xd0\x9e\xd0\xb1\xd1\x8a\xd1\x91\xd0\xbc"),
            ("BSL/shaders/tex/noise.png.mcmeta", b"{\"texture\": {\"blur\": true}}"),
            ("BSL/readme.txt", b"hi"),
        ])
        .unwrap();
        assert_eq!(c.kind, Kind::Shaders);
        let f = &c.facts;
        assert_eq!(f["features"], json!(["Shadows", "Volumetric light", "Water", "Waving plants"]));
        assert_eq!(f["dimensions"], json!(["nether", "overworld"]));
        assert_eq!(f["profiles"], json!(["HIGH", "LOW"]));
        assert_eq!(f["languages"], json!(["ru_ru"]));
        assert_eq!(f["options"], 2);
        assert_eq!(files.len(), 8); // seven of shaders/ and the readme, at the top once the folder came off
    }

    #[test]
    fn neither_kind_is_refused() {
        assert!(run(&[("readme.txt", b"hi"), ("textures/stone.png", &png(16, 16))]).unwrap_err().message.contains("neither"));
    }

    #[test]
    fn json_with_comments_is_forgiven() {
        assert!(relaxed_json(b"{ // a comment\n \"a\": [1, 2,], /* more */ \"b\": \"x // not a comment\" }").is_some());
        assert_eq!(relaxed_json("{\"t\": \"Пепел\",}".as_bytes()).unwrap()["t"], "Пепел");
    }
}
