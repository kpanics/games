//! The programs' resources (assets/app/app.rc: her icon, the name «Cursed Recode») compiled with the
//! Windows SDK's rc.exe and linked into the binaries (not the DLL). Without the SDK the binaries go without them.
//!
//! The manifest is a resource of its own and goes into the loader alone: the program a player opens is the one
//! Windows asks for administrator, and without it that program does not start. The game it begins is a child of it
//! and comes along already inside that door.

use std::path::{Path, PathBuf};

fn rc_exe() -> Option<PathBuf> {
    let kits = Path::new(r"C:\Program Files (x86)\Windows Kits\10\bin");
    let mut versions: Vec<PathBuf> = std::fs::read_dir(kits).ok()?.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.join(r"x64\rc.exe").is_file()).collect();
    versions.sort();
    versions.pop().map(|v| v.join(r"x64\rc.exe"))
}

/// The notice for src/notice.rs: as written with a zero at its end, and in UTF-16 (two zero bytes at its end).
fn notice() {
    println!("cargo:rerun-if-changed=assets/notice.txt");
    let text = std::fs::read_to_string(PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("assets").join("notice.txt")).expect("assets/notice.txt");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let mut plain = text.clone().into_bytes();
    plain.push(0);
    std::fs::write(out.join("notice.bin"), plain).expect("write the notice");
    let mut wide: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
    wide.extend_from_slice(&[0, 0]);
    std::fs::write(out.join("notice16.bin"), wide).expect("write the notice");
}

/// The words a dumper looks for first, and the ones that say which door is which: the paths, the environment's
/// names, what the server calls a refusal, and the labels the seal and the signature are mixed under. In the
/// binaries they are not themselves — `build.rs` turns each into noise and `vault` in lib.rs turns it back on the
/// first use. A `strings` pass over the DLL then finds nothing to walk in by, and the check a cracker came for is
/// not sitting next to the word "banned".
fn vault() {
    println!("cargo:rerun-if-changed=build.rs");
    // What goes into it from outside: cargo does not run this again on its own when only one of these changes, and a
    // build for the server would keep the test instance's door and key (build 39, 19.10.2026)
    for name in ["CURSED_VERIFY_API", "CURSED_VERIFY_KEY", "CURSED_SEAL_KEY", "CURSED_OBF_SEED"] {
        println!("cargo:rerun-if-env-changed={name}");
    }
    // (the name it is called by, the words themselves). The names live only here and in the source that calls
    // them; the binaries get the names neither as text nor as anything else.
    const KEPT: &[(&str, &str)] = &[
        // the doors
        ("path_alive", "/api/client/recode/alive"),
        ("path_bye", "/api/client/recode/bye"),
        ("path_verify", "/api/client/recode/verify"),
        ("path_sight", "/api/client/recode/sight"),
        ("path_link", "/api/loader/link"),
        ("path_link_poll", "/api/loader/link/poll"),
        ("path_device_reset", "/api/loader/recode/device/reset"),
        ("path_start", "/api/loader/recode/start"),
        ("path_stub", "/api/loader/recode/stub"),
        ("path_account", "/api/recode"),
        ("path_crash", "{}/api/crash?build={build}&source={source}"),
        // the environment's names — what a dumper greps to find where the build is told who it is
        ("env_build", "CURSED_BUILD"),
        ("env_dev_license", "CURSED_DEV_LICENSE"),
        ("env_loader_api", "CURSED_LOADER_API"),
        ("env_release", "CURSED_RELEASE"),
        ("env_seal", "CURSED_SEAL"),
        ("env_seal_key", "CURSED_SEAL_KEY"),
        ("env_start", "CURSED_START"),
        ("env_ticket", "CURSED_TICKET"),
        ("env_verify_api", "CURSED_VERIFY_API"),
        ("env_verify_key", "CURSED_VERIFY_KEY"),
        // what the server calls a refusal — the words the menu matches on
        ("banned", "banned"),
        ("device_taken", "device-taken"),
        ("ended", "ended"),
        ("in_game", "in-game"),
        ("loader", "loader"),
        ("no_recode", "no-recode"),
        ("no_tpm", "no-tpm"),
        ("offline", "offline"),
        ("offline_long", "offline-long"),
        ("other_device", "other-device"),
        ("outdated", "outdated"),
        ("signed_out", "signed-out"),
        ("ticket", "ticket"),
        ("busy", "busy"),
        ("slow", "slow"),
        ("stub", "stub"),
        ("ban", "ban"),
        ("taken", "taken"),
        ("kick", "kick"),
        ("ending", "ending"),
        ("refuse", "refuse"),
        // the algorithm's own labels: the words the seal and the signature are mixed under
        ("device_kdf", "cursed-device|"),
        ("mark_kdf", "cursed-seal-mark|"),
        ("seal_kdf", "cursed-seal/v1|"),
        ("verify_word", "cursed-client-verify/1\n{nonce}\n{ticket}\n{uid}\n{build}"),
        // a look at the room (loader/game.rs `a_look`): what kind of sign it is — the list is closed
        // on the server too, so both sides name them the same. The weights the score is counted with
        // are here for the same reason: a dumper reading them beside the code reads the whole test.
        ("sight_script", "script-ours"),
        ("sight_module", "module-ours"),
        ("sight_near", "file-near"),
        ("sight_dates", "date-cluster"),
        ("sight_tool", "tool"),
        ("sight_debugger", "debugger"),
        ("sight_dumper", "dumper"),
        ("sight_sandbox", "sandbox"),
        ("sight_parent", "parent"),
        ("sight_names", "dump|unpack|decompile|scylla|imprec|sieve|cheat"),
        ("sight_guest", "vmtoolsd|vboxservice|vboxtray|qemu-ga|xenservice|vdservice|vmwaretray|vmwareuser"),
        ("sight_score", "30 12 18 10 28 34 34 22 16"),
        // a hand put on the game's own memory (TOUCH.md): the six ways in — closed here and on the
        // server and in the library's own vault, so the three never name them differently — and what
        // the note is signed with. A `strings` pass finds neither the kinds nor the header a ban
        // arrives under, and the words a script has to carry to be a script are not sitting beside
        // the code that looks for them.
        ("touch_kinds", "patch|hook|break|write|inject|script"),
        ("touch_source", "touch"),
        ("touch_note", "touch: "),
        ("touch_header", "X-Cursed-Mac"),
        // Two kinds of name, and they are not the same question. `touch_pokes` is the words that
        // are never anybody's name for ordinary work — a substring of a file's name is enough.
        // `touch_nouns` is the words that also name ordinary modder tooling (`resource_unpacker.py`
        // is the ordinary Tuesday), so they fire only when they ARE the file's own stem
        // (`injector.py`, `injector64.exe`) or beside our name (`recode_ripper.py`). The library's
        // own vault keeps the same split under the same names (Cursed Protect\stub\build.rs).
        ("touch_pokes", "readprocessmemory|writeprocessmemory|openprocess|createremotethread|virtualallocex|virtualprotectex|ntwritevirtualmemory|ntreadvirtualmemory|manualmap|setwindowshookex|getthreadcontext|setthreadcontext|hardwarebreakpoint|processhollowing|dllinject|memedit|memorypatch|aobscan|sigscan|frida|pymem|cheatengine|memsearch|patternscan"),
        ("touch_nouns", "injector|ripper|dumper|unpacker|decompiler"),
        // what the note is called when the game leaves it beside itself. The stub keeps the same
        // word under the same name (Cursed Protect\stub\build.rs) and writes to it; this end reads
        // it. One of them naming it alone is how a note gets written somewhere nothing looks.
        ("note_name", "caught.txt"),
        // our own names — what a script or a foreign process has no business carrying. The loader's,
        // the game's and the library's are ours and are nobody's sight; a *third* thing wearing one
        // of these is somebody making a copy of us.
        ("ours_name", "recode"),
        ("ours_dll", "cursed_recode.dll"),
        // ours
        ("ours", "https://cursedclient.xyz"),
        ("cabinet", "https://cursedclient.xyz/#/dashboard/recode"),
        ("host", "host"),
        ("open_loader", "open-loader"),
    ];
    let seed: u32 = std::env::var("CURSED_OBF_SEED").ok().and_then(|s| u32::from_str_radix(s.trim().trim_start_matches("0x"), 16).ok()).unwrap_or(0xA5F1_3C2D);
    // What only the build knows: the key the server signs with, the word the seal is under, the server itself.
    // These used to be `option_env!` — the name is gone at compile time but the value was sitting in the binary as
    // itself. Now they go the way of every other word here.
    let mut kept: Vec<(String, String)> = KEPT.iter().map(|(n, w)| ((*n).to_string(), (*w).to_string())).collect();
    kept.push((
        "verify_key".to_string(),
        std::env::var("CURSED_VERIFY_KEY").unwrap_or_else(|_| "8e8332e23690f42940c844fb6b6e7db663cb8e1544e0cc9f76afe480e9e72f54".to_string()),
    ));
    kept.push(("seal_key".to_string(), std::env::var("CURSED_SEAL_KEY").unwrap_or_default()));
    kept.push((
        "verify_api".to_string(),
        std::env::var("CURSED_VERIFY_API").unwrap_or_else(|_| "https://cursedclient.xyz".to_string()),
    ));
    let mut blob = Vec::new();
    let mut lens = Vec::new();
    let mut names = String::new();
    for (i, (name, words)) in kept.iter().enumerate() {
        let at = blob.len() as u32;
        lens.push(words.len() as u16);
        for (j, b) in words.as_bytes().iter().enumerate() {
            blob.push(b ^ mix(at + j as u32, seed));
        }
        // what the source calls it by: a thin hand to the words, nothing of the words in it
        names.push_str(&format!("/// The words kept under `{name}` ({}).\n#[inline]\npub fn {name}() -> &'static str {{ table()[{i}] }}\n\n", words.len()));
    }
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("vault.rs");
    let blob: String = blob.iter().map(|b| format!("{b},")).collect();
    let lens: String = lens.iter().map(|n| format!("{n},")).collect();
    let words = format!(
        r#"// Written by build.rs. Nothing here is the words themselves.
const SEED: u32 = {seed};
const BLOB: &[u8] = &[{blob}];
const LENS: &[u16] = &[{lens}];

/// One byte of the noise the words are kept under: where they sit decides it, and the seed of this build.
#[inline]
fn mix(pos: u32) -> u8 {{
    let mut x = pos ^ SEED.wrapping_mul(0x9E37_79B9);
    x = x.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 13;
    x = x.wrapping_mul(0xC2B2_AE35);
    x ^= x >> 16;
    x as u8
}}

/// The words, back as themselves, once. Opening every one of them costs less than one frame of the game; after
/// that they are just there, and the noise stays noise to anyone reading the bytes.
fn table() -> &'static [&'static str] {{
    use std::sync::OnceLock;
    static OPENED: OnceLock<Vec<&'static str>> = OnceLock::new();
    OPENED.get_or_init(|| {{
        let mut out = Vec::with_capacity(LENS.len());
        let mut at = 0usize;
        for &n in LENS {{
            let n = n as usize;
            let mut plain = BLOB[at..at + n].to_vec();
            for (j, b) in plain.iter_mut().enumerate() {{
                *b ^= mix((at + j) as u32);
            }}
            // every one of them is plain words that were written as UTF-8, so this cannot go wrong
            out.push(String::from_utf8(plain).unwrap_or_default().into_boxed_str());
            at += n;
        }}
        out.into_iter().map(Box::leak).map(|s| &*s).collect()
    }})
}}

{names}"#
    );
    std::fs::write(&out, words).expect("write the vault");
}

/// One byte of the noise the words are kept under. The same one is in the generated file — kept here so the
/// two are not two thoughts that could drift apart.
fn mix(pos: u32, seed: u32) -> u8 {
    let mut x = pos ^ seed.wrapping_mul(0x9E37_79B9);
    x = x.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 13;
    x = x.wrapping_mul(0xC2B2_AE35);
    x ^= x >> 16;
    x as u8
}

fn main() {
    notice();
    vault();
    // the game's program hands this start's words to the game's library by this name (host.rs, license.rs
    // `from_the_program`): a program exports nothing unless told
    println!("cargo:rustc-link-arg-bin=CursedRecode=/EXPORT:cursed_recode_start");
    let dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("assets").join("app");
    for f in ["app.rc", "app.manifest"] {
        println!("cargo:rerun-if-changed={}", dir.join(f).display());
    }
    println!("cargo:rerun-if-changed=assets/icons/recode.ico");
    let Some(rc) = rc_exe() else {
        println!("cargo:warning=rc.exe not found: the programs go without their icon and name");
        return;
    };
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let app = out_dir.join("app.res");
    if compile(&rc, &dir, &dir.join("app.rc"), &app) {
        println!("cargo:rustc-link-arg-bins={}", app.display());
    } else {
        println!("cargo:warning=rc.exe failed: the programs go without their icon and name");
    }
    // The door: only the loader's own resource asks for administrator, and only the loader is linked it.
    let manifest = dir.join("app.manifest");
    let loader_rc = out_dir.join("loader.rc");
    let written = std::fs::write(&loader_rc, format!("1 24 \"{}\"\n", manifest.to_string_lossy().replace('\\', "/")));
    let loader_res = out_dir.join("loader.res");
    if written.is_ok() && compile(&rc, &out_dir, &loader_rc, &loader_res) {
        println!("cargo:rustc-link-arg-bin=recode_loader={}", loader_res.display());
    } else {
        println!("cargo:warning=rc.exe failed: the loader goes without its ask for administrator");
    }
}

/// One .rc into one .res: `rc.exe` names them in full, and takes the .rc from its own folder.
fn compile(rc: &Path, at: &Path, source: &Path, into: &Path) -> bool {
    std::process::Command::new(rc)
        .current_dir(at)
        .arg("/nologo")
        .arg("/fo")
        .arg(into)
        .arg(source)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
