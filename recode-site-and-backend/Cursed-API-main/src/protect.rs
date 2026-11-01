//! The master a release is sealed as, and the player's own pair cut out of it.
//!
//! `protect-pack` packs the game's library into a pair: `cursed_recode.bin` (the game under its seal) and a
//! stub named after the library that opens it. That master pair never goes out — not the stub, not the seal,
//! not its address card (`master.meta`). What a player gets is a pair of their own, cut out of the master:
//! halves of their own, a sealed file of their own and their own last day. One crack of one player's pair
//! opens that copy and nothing else (protect-pack's `mint`, of `Desktop\Cursed Protect`).
//!
//! The cutting is that program's and not this file's, and that is the point. There used to be a second
//! account of it here — one half rewritten so the minted stub rebuilt the master's key, under a fold of three
//! terms where the stub takes six — and the two did not agree: a stub cut here opened nothing the packer had
//! packed. One place turns a pair out now, it is the place the master was made in, and it is asked by
//! command. What comes back is checked here before it is handed over anyway: a pair that does not open is
//! not a pair.
//!
//! A pair is keyed by the player's ticket, and that ticket has a window of its own: the same for as long as
//! the window lasts, a new one when it turns over, so the game a player already has goes on opening while the
//! window stands and nobody re-downloads fourteen megabytes over a start that changed nothing. When the window
//! turns, the ticket with it — and the pair that was under the old ticket is so much noise. Nothing about a
//! player's ticket is kept here: it is what a key of this server's says it is, over their name and the
//! window's number.
//!
//! The folder holds the master and the tool that cuts it; it sits under the dist root but out of `objects/`,
//! where nothing serves it from.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The marker a meta file opens with.
const META_FIRST: &str = "CRD_MASTER1";
/// The key halves are 32 bytes each (protect-container's `KEY_LEN`).
const KEY_LEN: usize = 32;
/// How long a player's own pair is good for, at most. Under the key as well as in the file, so moving it is
/// another key and no game. The same number of days the build's own key is turned over on (Release.java,
/// `KEY_DAYS`) — a leaked pair has a last day, and so does the key it was cut from.
pub const KEEP_DAYS: u64 = 14;

/// The window a player's pair ticket lives in. Everyone's turns over together, on the clock's own grid, so a
/// ticket that leaked has a day on the calendar when it stops being anyone's — and a pair under it stops
/// opening the moment its ticket is not the one the server hands out any more.
pub const PAIR_WINDOW: i64 = KEEP_DAYS as i64 * 86_400;

/// The frame a pair goes out under: the word, then how long each of the two files is, then the two. One answer
/// has to carry both halves of one cut — two answers could straddle a turn of the window and hand over a stub
/// that opens nothing beside it.
pub const PAIR_FRAME: [u8; 8] = *b"CRDPAIR1";

/// The master pair's address card: where the two halves sit in the stub, and the ticket the pair was packed
/// under. The halves themselves stay in the stub — this alone is nothing. Read only to know the card is one;
/// the offsets are the packer's to use. Its text is kept as it came, so it can be put back as it was.
pub struct Meta {
    pub half_a_at: usize,
    pub half_b_at: usize,
    pub ticket: Vec<u8>,
    pub text: String,
}

/// The master a player's pair is cut out of.
pub struct Master {
    pub stub: Vec<u8>,
    pub bin: Vec<u8>,
    pub meta: Meta,
}

/// One player's own pair: their stub and their sealed game. Both are theirs and neither opens the master's.
pub struct Pair {
    pub stub: Vec<u8>,
    pub bin: Vec<u8>,
}

/// The folder the master lives in, under the dist root: nothing of the store serves it.
pub fn folder(root: &Path) -> PathBuf {
    root.join("protect")
}

/// Whether a master and the tool to cut it are here (the start says so, so the loader knows to ask for its
/// pair). All four: a master without the cutter is a master nobody can use, and a cutter without a master is
/// nothing to cut.
pub fn there(root: &Path) -> bool {
    let f = folder(root);
    f.join("cursed_recode.dll").is_file()
        && f.join("master.meta").is_file()
        && f.join("cursed_recode.bin").is_file()
        && cutter(&f).is_file()
}

/// What a release asks before it lets go: is the master up here whole, and if not, which of the four is
/// missing. The whole one is Ok and says where it stands; anything less is Err and names what it lacks —
/// a release that cannot cut a pair must not go out, and this is the question it asks (Release.java).
pub fn status(root: &Path) -> Result<String, String> {
    let f = folder(root);
    let mut lacking: Vec<&str> = Vec::new();
    if !f.join("cursed_recode.dll").is_file() {
        lacking.push("cursed_recode.dll");
    }
    if !f.join("cursed_recode.bin").is_file() {
        lacking.push("cursed_recode.bin");
    }
    if !f.join("master.meta").is_file() {
        lacking.push("master.meta");
    }
    if !cutter(&f).is_file() {
        lacking.push(cutter_name());
    }
    if lacking.is_empty() {
        Ok(format!("protect: whole ({})", f.display()))
    } else {
        Err(format!("protect: {} lacks {}", f.display(), lacking.join(", ")))
    }
}

/// Whether a build is sealed at all: its manifest says so. A start is cut a pair only then and only while a
/// master is here — a build of any other kind brings its own library among its files and keeps it (a master
/// left over from a sealed build must never be written over one of those).
pub fn sealed(manifest: &str) -> bool {
    // The library itself is not one of the manifest's files and never was: it is cut for the player at the
    // start. What the manifest says is that this build loads a library of that kind. The key is written
    // only when it is true (dist.rs), and here it has to say so whatever the spacing — a key that is there
    // and says no is not a key that says yes.
    let Some(rest) = manifest.split("\"seal\":").nth(1) else { return false };
    rest.trim_start().starts_with("true")
}

/// The word a sealed file opens with, and how long its head is (`protect-container`'s `magic` and `HEADER`).
/// A sealed file is the game under its seal and is not a library — `MZ` is the stub's word and never this
/// one's, whatever a cut that went wrong might leave behind.
const SEAL_WORD: [u8; 4] = *b"CRB1";
const SEAL_HEAD: usize = 64;

/// Whether these bytes are a seal of the packer's shape: its own word, and a body of exactly the length the
/// head names. The turn of the key is the packer's to prove and cannot be done here — the cutting is asked
/// by command so that the key exists in one place. What this is for is the rest of it: a half-written room,
/// a cutter that wrote nothing, a file of some other kind. A pair that is not even this shape never goes out.
fn seal_is_one(bin: &[u8]) -> bool {
    if bin.len() < SEAL_HEAD || bin[..4] != SEAL_WORD {
        return false;
    }
    let plain_len = u64::from_le_bytes(bin[8..16].try_into().unwrap());
    (bin.len() - SEAL_HEAD) as u64 == plain_len
}

/// The master pair and its card, read whole. Called at a start and at a play: a few hundred kilobytes for
/// the stub, the game's own seal beside it, never cached — a new release is in place the moment it is put there.
pub fn load(root: &Path) -> Result<Master, String> {
    let f = folder(root);
    let stub = fs::read(f.join("cursed_recode.dll")).map_err(|e| format!("the master stub: {e}"))?;
    let bin = fs::read(f.join("cursed_recode.bin")).map_err(|e| format!("the master seal: {e}"))?;
    let meta = Meta::read(&f.join("master.meta"))?;
    if meta.half_a_at + KEY_LEN > stub.len() || meta.half_b_at + KEY_LEN > stub.len() {
        return Err("the master meta points outside the master stub".into());
    }
    Ok(Master { stub, bin, meta })
}

// ------------------------------------------------------------------------------------------------ the window

/// The number of the window `at` falls in. The same number for everyone in it.
pub fn window_of(at: i64) -> i64 {
    at / PAIR_WINDOW
}

/// When the window `at` falls in ends (seconds since the epoch).
pub fn window_end(at: i64) -> i64 {
    (window_of(at) + 1) * PAIR_WINDOW
}

/// How long the pair cut now stays the one the server hands out (s). What the loader is told so it knows when
/// its own copy has had its turn.
pub fn pair_seconds(at: i64) -> i64 {
    (window_end(at) - at).max(0)
}

/// How many days to give the pair cut now, so its own last day — which is under the key, and the only one the
/// stub answers to when there is no server to ask — lands on the window's end and not a fortnight past it.
pub fn pair_keep_days(at: i64) -> u64 {
    let left = window_end(at) - at;
    ((left + 86_399) / 86_400).clamp(1, KEEP_DAYS as i64) as u64
}

/// This player's pair ticket for the window `at` falls in. The same while the window stands; a new one the
/// moment it turns. Nothing about it is kept: it is what a key of this server's says over the player's name
/// and the window's number, so a dump of the base tells no one anyone's ticket.
pub fn pair_ticket(root: &Path, uid: i64, at: i64) -> String {
    use sha2::{Digest, Sha256};
    let key = pair_key(root);
    let mut fold = Vec::with_capacity(key.len() + 11 + 16);
    fold.extend_from_slice(&key);
    fold.extend_from_slice(b"cursed-pair");
    fold.extend_from_slice(&uid.to_le_bytes());
    fold.extend_from_slice(&window_of(at).to_le_bytes());
    // a keyed hash over a shape that is always the same: an account's name and the window's number under a
    // key only this server holds. Hex, because that is what a ticket is everywhere else here.
    Sha256::digest(&fold).iter().map(|b| format!("{b:02x}")).collect()
}

/// Whether `ticket` is this player's for the window `at` falls in — or the one before, because a start may
/// come just as the window turns and the pair it asks for was named under the old one.
pub fn pair_ticket_is(root: &Path, uid: i64, ticket: &str, at: i64) -> bool {
    let now = pair_ticket(root, uid, at);
    if ticket == now {
        return true;
    }
    ticket == pair_ticket(root, uid, at - PAIR_WINDOW)
}

/// The two files of a pair under the frame (see `PAIR_FRAME`).
pub fn framed(pair: &Pair) -> Vec<u8> {
    let mut out = Vec::with_capacity(16 + pair.stub.len() + pair.bin.len());
    out.extend_from_slice(&PAIR_FRAME);
    out.extend_from_slice(&(pair.stub.len() as u32).to_le_bytes());
    out.extend_from_slice(&(pair.bin.len() as u32).to_le_bytes());
    out.extend_from_slice(&pair.stub);
    out.extend_from_slice(&pair.bin);
    out
}

/// The key pair tickets are made under: 32 bytes of this server's own, in the folder nothing serves, made the
/// first time it is asked for and never moved. Not in the base — a dump of that is a dump of accounts, and this
/// is what turns an account's name into the one string their game is keyed by.
fn pair_key(root: &Path) -> Vec<u8> {
    use rand_core::{OsRng, RngCore};
    let at = folder(root).join(".pair.key");
    if let Ok(k) = fs::read(&at) {
        if k.len() == 32 {
            return k;
        }
    }
    let mut fresh = [0u8; 32];
    OsRng.fill_bytes(&mut fresh);
    let _ = fs::create_dir_all(folder(root));
    let _ = fs::write(&at, &fresh);
    // this one is nobody's but ours: a box has more than one account on it, and this is the key every
    // player's ticket on it is made under
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&at, fs::Permissions::from_mode(0o600));
    }
    fresh.to_vec()
}

// ------------------------------------------------------------------------------------------------ the card

impl Meta {
    /// The text `protect-pack` writes: the marker, the two offsets and the master's ticket as hex. The text
    /// itself is kept as it came — a card is put back the way the packer wrote it and never re-set in words of
    /// ours, so a line the packer cares about and this reader passes over is not lost between the two.
    pub fn read(path: &Path) -> Result<Self, String> {
        let text = fs::read_to_string(path).map_err(|e| format!("master.meta: {e}"))?;
        let mut lines = text.lines();
        if lines.next() != Some(META_FIRST) {
            return Err("master.meta is not one".into());
        }
        let mut a = None;
        let mut b = None;
        let mut ticket = None;
        for line in lines {
            let Some((k, v)) = line.split_once('=') else { continue };
            match k {
                "half_a_at" => a = v.parse().ok(),
                "half_b_at" => b = v.parse().ok(),
                "ticket" => ticket = unhex(v).ok(),
                _ => {}
            }
        }
        Ok(Self {
            half_a_at: a.ok_or("master.meta has no half_a_at")?,
            half_b_at: b.ok_or("master.meta has no half_b_at")?,
            ticket: ticket.ok_or("master.meta has no ticket")?,
            text,
        })
    }
}

impl Master {
    /// This player's own pair, cut by `protect-pack` out of the master: halves of their own, a sealed file of
    /// their own, their own last day, and their account written into the stub. Only that ticket opens it.
    ///
    /// The cut is over the master as it is held here and not over whatever is standing in the folder — a
    /// release's self-check cuts the master it has just read, before that master is put up in place of the
    /// one before, and a start cuts the one it read at the start of this request.
    ///
    /// The ticket goes in a file and not on the command line: a command line is read by any program on the
    /// machine and kept in its history, and this one is the string their whole copy is keyed by. The account
    /// is a number and goes on the command line — it names whose pair this is and is not a secret; the stub
    /// marks the mapped game with it at load, so a dump of a running game says whose it was.
    pub fn cut(&self, root: &Path, ticket: &str, keep_days: u64, uid: i64) -> Result<Pair, String> {
        let f = folder(root);
        let tool = cutter(&f);
        if !tool.is_file() {
            return Err(format!("{} is not here", tool.display()));
        }

        // A room of its own: the master as it is held here, the two it makes, and the ticket that must not
        // be on the command line. Nothing of it is left behind.
        let room = std::env::temp_dir().join(format!("cursed-mint-{}", crate::random_hex(8)));
        let out = room.join("out");
        fs::create_dir_all(&out).map_err(|e| format!("the mint's room: {e}"))?;
        let stub_at = room.join("cursed_recode.dll");
        let meta_at = room.join("master.meta");
        let bin_at = room.join("cursed_recode.bin");
        let ticket_at = room.join("ticket");
        fs::write(&stub_at, &self.stub).map_err(|e| format!("the master stub: {e}"))?;
        fs::write(&meta_at, &self.meta.text).map_err(|e| format!("master.meta: {e}"))?;
        fs::write(&bin_at, &self.bin).map_err(|e| format!("the master seal: {e}"))?;
        fs::write(&ticket_at, ticket.as_bytes()).map_err(|e| format!("the ticket's file: {e}"))?;

        let ran = Command::new(&tool)
            .arg("mint")
            .arg(&stub_at)
            .arg(&meta_at)
            .arg(&bin_at)
            .arg(&out)
            .arg("--ticket-file")
            .arg(&ticket_at)
            .arg("--keep-days")
            .arg(keep_days.to_string())
            .arg("--uid")
            .arg(uid.to_string())
            .output();
        let _ = fs::remove_file(&ticket_at);

        let ran = ran.map_err(|e| format!("{} would not run: {e}", tool.display()))?;
        if !ran.status.success() {
            let _ = fs::remove_dir_all(&room);
            return Err(format!(
                "{}: {}",
                tool.display(),
                String::from_utf8_lossy(&ran.stderr).trim()
            ));
        }

        let stub = fs::read(out.join("cursed_recode.dll")).map_err(|e| format!("the cut stub: {e}"))?;
        let bin = fs::read(out.join("cursed_recode.bin")).map_err(|e| format!("the cut seal: {e}"))?;
        let _ = fs::remove_dir_all(&room);

        // a pair that does not open is not a pair. The packer checks the turn of the key before it writes;
        // this is the rest of that look, taken here over the very bytes about to be handed over, so a
        // half-written room or a cutter that went wrong is caught and not sent.
        if stub.len() < 2 || &stub[..2] != b"MZ" {
            return Err("the cut stub is not a library".into());
        }
        if !seal_is_one(&bin) {
            return Err("the cut seal is not one".into());
        }
        Ok(Pair { stub, bin })
    }
}

/// The program that turns a master into a player's own pair. Built for the machine the server runs on —
/// the same source as the Windows one the release is cut with (`Desktop\Cursed Protect`), only made for
/// this system — and named for that system here so nothing is guessing which one it is.
fn cutter(folder: &Path) -> PathBuf {
    folder.join(cutter_name())
}

fn cutter_name() -> &'static str {
    if cfg!(windows) { "protect-pack.exe" } else { "protect-pack" }
}

fn unhex(text: &str) -> Result<Vec<u8>, String> {
    if text.is_empty() {
        return Ok(Vec::new());
    }
    if text.len() % 2 != 0 {
        return Err("hex with a half byte".into());
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).map_err(|_| "not hex".to_string()))
        .collect()
}

/// A master on its way in from a release: checked whole, then put in place of the one before. A release that
/// brought none leaves the master alone — the store has no pair in it and the one here is what starts go on
/// being cut from.
///
/// The cutter comes with it. It is a program of ours and not a secret, but it is not a file of the store
/// either — nothing serves this folder.
pub fn take_in(root: &Path, from: &Path) -> Result<(), String> {
    if !from.is_dir() {
        return Ok(());
    }
    let stub = fs::read(from.join("cursed_recode.dll")).map_err(|e| format!("protect: {e}"))?;
    let bin = fs::read(from.join("cursed_recode.bin")).map_err(|e| format!("protect: {e}"))?;
    let meta = Meta::read(&from.join("master.meta"))?;
    if meta.half_a_at + KEY_LEN > stub.len() || meta.half_b_at + KEY_LEN > stub.len() {
        return Err("protect: the master meta points outside the master stub".into());
    }

    // the cutter first: the check below runs it. Without one there is a master nobody can cut, which is
    // worse than none at all — the start would say a pair is coming and nothing would come.
    let f = folder(root);
    fs::create_dir_all(&f).map_err(|e| format!("protect: {e}"))?;
    let from_tool = from.join(cutter_name());
    if !from_tool.is_file() {
        return Err(format!("protect: no {} in the release", cutter_name()));
    }
    fs::copy(&from_tool, cutter(&f)).map_err(|e| format!("protect: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(cutter(&f), fs::Permissions::from_mode(0o755));
    }

    // cut once here, under a ticket of our own: a master whose halves do not sit where its meta says, or
    // whose seal will not open, is not worth putting up. It is cut as it was read, not as it will stand —
    // the one here is still the one before, and a check of that would say nothing of this one. Nothing of
    // this cut is kept. Account zero: this one is nobody's, and a pair cut for nobody leaves no name in
    // anything — which is what the account is there for.
    let text = meta.text.clone();
    let master = Master { stub: stub.clone(), bin: bin.clone(), meta };
    master
        .cut(root, &crate::random_hex(32), KEEP_DAYS, 0)
        .map_err(|e| format!("protect: {e}"))?;

    fs::write(f.join("cursed_recode.dll"), &stub).map_err(|e| format!("protect: {e}"))?;
    fs::write(f.join("cursed_recode.bin"), &bin).map_err(|e| format!("protect: {e}"))?;
    fs::write(f.join("master.meta"), text).map_err(|e| format!("protect: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A master as `protect-pack` leaves it: its card is read, and the offsets are where the halves sit.
    #[test]
    fn the_card_is_read_the_way_the_packer_writes_it() {
        let room = std::env::temp_dir().join(format!("cursed-meta-{}", crate::random_hex(6)));
        std::fs::create_dir_all(&room).unwrap();
        let path = room.join("master.meta");
        std::fs::write(&path, format!("{META_FIRST}\nhalf_a_at=12\nhalf_b_at=34\nticket={}\n", hex(b"server-ticket"))).unwrap();
        let meta = Meta::read(&path).unwrap();
        assert_eq!(meta.half_a_at, 12);
        assert_eq!(meta.half_b_at, 34);
        assert_eq!(meta.ticket, b"server-ticket".to_vec());
        let _ = std::fs::remove_dir_all(&room);
    }

    /// A card that is not one says so rather than being believed.
    #[test]
    fn a_card_that_is_not_one_says_so() {
        let room = std::env::temp_dir().join(format!("cursed-meta-{}", crate::random_hex(6)));
        std::fs::create_dir_all(&room).unwrap();
        let path = room.join("master.meta");
        std::fs::write(&path, "half_a_at=12\n").unwrap();
        assert!(Meta::read(&path).is_err());
        let _ = std::fs::remove_dir_all(&room);
    }

    /// The sealed flag is the build's own word, and a build that never said it is not under one. A key that
    /// is there and says no is no key at all.
    #[test]
    fn a_build_that_says_nothing_of_a_seal_is_not_under_one() {
        assert!(!sealed(r#"{"version": "1", "files": []}"#));
        assert!(sealed(r#"{"seal": true, "files": []}"#));
        assert!(sealed(r#"{"seal":true,"files":[]}"#));
        assert!(!sealed(r#"{"seal": false, "files": []}"#));
    }

    /// A seal is the packer's own file and not a library: its word, and a body of the length its head names.
    /// Anything else is a cut that went wrong and goes nowhere.
    #[test]
    fn a_seal_is_the_packers_own_file_and_not_a_library() {
        let mut seal = vec![0u8; SEAL_HEAD + 100];
        seal[..4].copy_from_slice(&SEAL_WORD);
        seal[8..16].copy_from_slice(&100u64.to_le_bytes());
        assert!(seal_is_one(&seal));
        // a library is not one, whatever it is that wrote it
        assert!(!seal_is_one(b"MZ\x90\x00 not a seal at all"));
        // neither is one whose head is cut off, whose word is another's, or whose body is not the length it says
        assert!(!seal_is_one(&seal[..SEAL_HEAD]));
        let mut foreign = seal.clone();
        foreign[..4].copy_from_slice(b"PK\x03\x04");
        assert!(!seal_is_one(&foreign));
        let mut short = seal.clone();
        short.pop();
        assert!(!seal_is_one(&short));
    }

    /// A ticket is the window's and the player's: it holds while the window stands, it is another's when the
    /// window turns, and it is nobody else's whatever they ask.
    #[test]
    fn a_ticket_is_the_players_and_the_windows() {
        let root = std::env::temp_dir().join(format!("cursed-pair-{}", crate::random_hex(6)));
        std::fs::create_dir_all(&root).unwrap();
        let at = PAIR_WINDOW * 3 + 1000;
        let mine = pair_ticket(&root, 7, at);
        assert_eq!(mine.len(), 64);
        assert_eq!(mine, pair_ticket(&root, 7, at + 10), "the same window, the same ticket");
        assert!(pair_ticket_is(&root, 7, &mine, at));
        assert!(pair_ticket_is(&root, 7, &mine, at + PAIR_WINDOW), "the one just turned is still taken");
        assert_ne!(mine, pair_ticket(&root, 8, at), "another player, another ticket");
        assert_ne!(mine, pair_ticket(&root, 7, at + PAIR_WINDOW * 2), "two windows on, another ticket");
        assert!(!pair_ticket_is(&root, 8, &mine, at));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The pair's own last day lands on the window's end — never a fortnight past it, and never a day early
    /// enough for the stub to give up before the window does.
    #[test]
    fn the_last_day_is_the_windows() {
        let at = PAIR_WINDOW * 5 + PAIR_WINDOW - 10;
        assert_eq!(pair_keep_days(at), 1, "an hour before the turn: one day");
        assert_eq!(pair_seconds(at), 10);
        let at = PAIR_WINDOW * 5 + 86_400;
        assert_eq!(pair_keep_days(at), KEEP_DAYS - 1);
        assert_eq!(pair_seconds(at), PAIR_WINDOW - 86_400);
    }

    /// One answer, both files, under one frame — with their lengths first so nothing has to guess where the
    /// stub ends and the seal begins.
    #[test]
    fn a_pair_goes_out_under_one_frame() {
        let pair = Pair { stub: vec![0x4d, 0x5a, 1], bin: vec![0x4d, 0x5a, 2, 3] };
        let frame = framed(&pair);
        assert_eq!(&frame[..8], &PAIR_FRAME);
        assert_eq!(&frame[8..12], &[3, 0, 0, 0]);
        assert_eq!(&frame[12..16], &[4, 0, 0, 0]);
        assert_eq!(&frame[16..], &[0x4d, 0x5a, 1, 0x4d, 0x5a, 2, 3]);
        assert_eq!(frame.len(), 16 + 3 + 4);
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
}
