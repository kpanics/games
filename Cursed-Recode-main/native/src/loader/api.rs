//! The loader's side of the conversation with cursed-api (site/server/src/recode.rs and dist.rs): signing in through
//! the site (a code this loader shows, allowed on the site), starting (the build, leave to download, a ticket for one
//! start of the game), freeing this computer, the subscription, sending a crash. Cursed's loader's, for Recode.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::net as http;

/// The server: a release build's own (ours, or the test instance it was built for: CURSED_VERIFY_API, as the game's);
/// one built from the sources, CURSED_LOADER_API when it is set (tests).
pub fn base() -> String {
    let fixed = crate::vault::verify_api();
    if crate::license::RELEASE {
        return fixed.trim_end_matches('/').to_string();
    }
    std::env::var(crate::vault::env_loader_api()).ok().filter(|b| http::parse(b).is_some()).unwrap_or_else(|| fixed.into()).trim_end_matches('/').to_string()
}

/// Why a request did not give what was asked for.
#[derive(Clone, Debug, PartialEq)]
pub enum Fail {
    /// No answer at all: no connection, a timeout, a broken TLS.
    Offline(String),
    /// The server said no: its status, its word (`error`), the whole answer.
    Refused { status: u16, error: String, body: Value },
    /// The words came, but none of ours to open: something in between wrote its own, or the two sides are not of one
    /// seal. Nothing of that answer is believed (net.rs).
    Sealed,
}

impl Fail {
    pub fn error(&self) -> &str {
        match self {
            Fail::Offline(_) => "offline",
            Fail::Refused { error, .. } => error,
            Fail::Sealed => "seal",
        }
    }

    pub fn body(&self) -> Value {
        match self {
            Fail::Offline(_) => Value::Null,
            Fail::Refused { body, .. } => body.clone(),
            Fail::Sealed => Value::Null,
        }
    }

    /// What exactly failed, for the person and for us: the step and Windows' code ("tls 12175 ca", "dns 12007"), or
    /// the status of an answer that said nothing ("http 502").
    pub fn detail(&self) -> String {
        match self {
            Fail::Offline(d) => d.clone(),
            Fail::Refused { status, .. } => format!("http {status}"),
            Fail::Sealed => "seal".into(),
        }
    }
}

/// No answer: the transport's word, with what net.rs noted about it.
fn offline(word: String) -> Fail {
    let detail = http::last_failure();
    Fail::Offline(if detail.is_empty() { word } else { detail })
}

/// One request with a JSON answer: the status (a 2xx) and the answer. What is said goes sealed, and only an answer
/// sealed under the same seal is opened (net.rs); a refusal comes as it always did, those being the same words
/// whoever reads them — and an answer of something in between (a proxy that never heard of a seal) is not believed.
/// A client built with no word of its own (net.rs: `sealing`) speaks as it always did: the two sides turn the seal
/// on together, the server refusing plain words once it has the word.
pub fn call_status(method: &str, path: &str, token: Option<&str>, body: Option<&Value>) -> Result<(u16, Value), Fail> {
    let url = format!("{}{path}", base());
    let auth = token.map(|t| format!("Bearer {t}"));
    let mut headers: Vec<(&str, &str)> = vec![("Accept", "application/json")];
    let sealed = http::sealing();
    // the letter's own words, so that the answer can name them: an answer to another one is not this one's
    let mut asked = String::new();
    let payload = match body {
        Some(b) => {
            let words = if sealed {
                let (letter, nonce) = http::seal(method, path, b).ok_or(Fail::Sealed)?;
                asked = nonce;
                letter.to_string()
            } else {
                b.to_string()
            };
            headers.push(("Content-Type", "application/json"));
            Some(words.into_bytes())
        }
        None => None,
    };
    if let Some(a) = auth.as_deref() {
        headers.push(("Authorization", a));
    }
    let mut got = Vec::new();
    let head = http::request(method, &url, &headers, payload.as_deref(), &mut |_, chunk| {
        got.extend_from_slice(chunk);
        got.len() < 16 << 20
    })
    .map_err(offline)?;
    if head.status >= 300 {
        let value: Value = serde_json::from_slice(&got).unwrap_or(Value::Null);
        let error = value["error"].as_str().unwrap_or(match head.status {
            401 => crate::vault::signed_out(),
            429 => crate::vault::slow(),
            502..=504 => crate::vault::offline(),
            _ => crate::vault::busy(),
        });
        return Err(Fail::Refused { status: head.status, error: error.to_string(), body: value });
    }
    let value = match (payload.is_some(), sealed) {
        (true, true) => {
            let envelope: Value = serde_json::from_slice(&got).map_err(|_| Fail::Sealed)?;
            http::open(method, path, &asked, &envelope).ok_or(Fail::Sealed)?
        }
        // asked with nothing to seal (the account's Recode), or built without the word: as it always was
        _ => serde_json::from_slice(&got).unwrap_or(Value::Null),
    };
    Ok((head.status, value))
}

pub fn call(method: &str, path: &str, token: Option<&str>, body: Option<&Value>) -> Result<Value, Fail> {
    call_status(method, path, token, body).map(|(_, v)| v)
}

/// Asks to be signed in through the site: the code to show, the page to open, this loader's own key for the answer.
pub fn link(device: &Value) -> Result<Value, Fail> {
    call("POST", crate::vault::path_link(), None, Some(&json!({ "device": device })))
}

/// The site's answer to the link: None while it waits; the session (token, uid, login) once allowed.
pub fn link_poll(code: &str, key: &str) -> Result<Option<Value>, Fail> {
    let (status, v) = call_status("POST", crate::vault::path_link_poll(), None, Some(&json!({ "code": code, "key": key })))?;
    Ok((status == 200).then_some(v))
}

/// May this account play Recode on this computer (it is bound to it if it had none): the build, signed; a ticket.
pub fn start(token: &str, device: &Value) -> Result<Value, Fail> {
    call("POST", crate::vault::path_start(), Some(token), Some(&json!({ "device": device })))
}

/// Both files of one player's own pair, as they came under one frame: the stub that opens the seal and the
/// seal itself. One cut, one answer — two answers could straddle the window turning and hand over a stub that
/// opens nothing beside it.
pub struct Pair {
    pub stub: Vec<u8>,
    pub bin: Vec<u8>,
}

/// The word a pair comes under, then how long each of the two files is, then the two (protect::PAIR_FRAME,
/// the server's name for the same thing).
const PAIR_FRAME: &[u8; 8] = b"CRDPAIR1";

/// The word a sealed file opens with, and how long its head is (protect::SEAL_WORD / SEAL_HEAD — the
/// packer's `magic` and `HEADER`). The two halves of a pair are not the same kind of file and are not
/// looked for under the same word: the stub is a library and starts `MZ`; the sealed game is the game
/// under its seal and is **never** `MZ` — a cut that left one is a cut that went wrong.
const SEAL_WORD: &[u8; 4] = b"CRB1";
const SEAL_HEAD: usize = 64;

/// This player's own pair, cut out of the master the server keeps (`/api/loader/recode/stub`) under their
/// pair ticket: halves of their own, a sealed game of their own, their own last day. The ticket is left as it
/// is — the game opens the copy under it (protect-stub) and spends a start's own words on its own (license.rs).
/// What goes asking is sealed like any other words (the ticket is the one thing in it worth reading); what
/// comes back is the pair itself, already good for that ticket alone.
pub fn stub(token: &str, ticket: &str) -> Result<Pair, Fail> {
    let path = crate::vault::path_stub();
    let url = format!("{}{path}", base());
    let auth = format!("Bearer {token}");
    let words = json!({ "ticket": ticket });
    let body = if http::sealing() {
        http::seal("POST", path, &words).ok_or(Fail::Sealed)?.0.to_string().into_bytes()
    } else {
        words.to_string().into_bytes()
    };
    let headers = [("Content-Type", "application/json"), ("Authorization", auth.as_str())];
    let mut got = Vec::new();
    let mut big = false;
    let head = http::request("POST", &url, &headers, Some(&body), &mut |_, chunk| {
        got.extend_from_slice(chunk);
        // a pair is the game under its seal and the stub that opens it — a little over fourteen megabytes.
        // Twice that is the ceiling; anything past it is not a pair.
        big = got.len() > 32 << 20;
        !big
    })
    .map_err(offline)?;
    if head.status >= 300 {
        let value: Value = serde_json::from_slice(&got).unwrap_or(Value::Null);
        let error = value["error"].as_str().unwrap_or(match head.status {
            401 => crate::vault::signed_out(),
            429 => crate::vault::slow(),
            502..=504 => crate::vault::offline(),
            _ => crate::vault::busy(),
        });
        return Err(Fail::Refused { status: head.status, error: error.to_string(), body: value });
    }
    if big {
        return Err(Fail::Refused { status: head.status, error: crate::vault::stub().into(), body: json!({ "detail": "the pair came too big" }) });
    }
    open_pair(&got, head.status)
}

/// The frame, taken apart: the word, the two lengths, then the two files. A frame that is not one, or whose
/// lengths do not add up to what came, is refused rather than guessed at — half a pair is no pair at all.
fn open_pair(got: &[u8], status: u16) -> Result<Pair, Fail> {
    let not_one = |detail: &str| {
        Fail::Refused {
            status,
            error: crate::vault::stub().into(),
            body: json!({ "detail": detail }),
        }
    };
    if got.len() < 16 || &got[..8] != PAIR_FRAME {
        return Err(not_one("the pair is not framed"));
    }
    let stub_len = u32::from_le_bytes([got[8], got[9], got[10], got[11]]) as usize;
    let bin_len = u32::from_le_bytes([got[12], got[13], got[14], got[15]]) as usize;
    if got.len() != 16 + stub_len + bin_len {
        return Err(not_one("the pair's frame does not add up"));
    }
    let stub = got[16..16 + stub_len].to_vec();
    let bin = got[16 + stub_len..].to_vec();
    if stub.len() < 2 || &stub[..2] != b"MZ" {
        return Err(not_one("the stub is not a library"));
    }
    // the game's own seal, not a library: its own word and a body of exactly the length the head names
    // (protect::seal_is_one, the server's own check before it ever sends one).
    if bin.len() < SEAL_HEAD || &bin[..4] != SEAL_WORD {
        return Err(not_one("the seal is not one"));
    }
    let plain_len = u64::from_le_bytes(bin[8..16].try_into().unwrap_or([0; 8]));
    if (bin.len() - SEAL_HEAD) as u64 != plain_len {
        return Err(not_one("the seal's body does not add up"));
    }
    Ok(Pair { stub, bin })
}

/// Frees the account's computer for this one (once a day).
pub fn reset(token: &str, device: &Value) -> Result<Value, Fail> {
    call("POST", crate::vault::path_device_reset(), Some(token), Some(&json!({ "device": device })))
}

/// The account's subscription (active, forever, until; its keys).
pub fn recode(token: &str) -> Result<Value, Fail> {
    call("GET", crate::vault::path_account(), Some(token), None)
}

/// A look at the room (report only): what stands around the game, and how sure the look is (0-100).
/// Never a ban, never a hold — the player keeps playing and is never told a word. Her list keeps one
/// line per account and counts the same look again when it comes again.
pub fn sight(token: &str, build: i64, score: i64, sights: &Value) -> Result<Value, Fail> {
    call("POST", crate::vault::path_sight(), Some(token), Some(&json!({ "build": build, "score": score, "sights": sights })))
}

pub fn crash(token: &str, build: i64, source: &str, report: &[u8]) -> Result<(), Fail> {
    let url = format!("{}/api/crash?build={build}&source={source}", base());
    let auth = format!("Bearer {token}");
    let headers = [("Content-Type", "text/plain; charset=utf-8"), ("Authorization", auth.as_str())];
    let head = http::request("POST", &url, &headers, Some(report), &mut |_, _| true).map_err(offline)?;
    if head.status >= 300 {
        return Err(Fail::Refused { status: head.status, error: "crash".into(), body: Value::Null });
    }
    Ok(())
}

/// What says this note came from this loader and not from anyone who guessed a route: the pair
/// ticket every start hands the game, and the note itself, under one SHA-256 (TOUCH.md §3) —
/// `hex_lowercase( sha256( ticket_utf8 || 0x0A || body ) )`. The line feed between the two halves
/// is part of the contract, not a flourish: a MAC taken over the two run together is another MAC.
/// The body hashed is the body sent, byte for byte — the server opens the request and hashes what
/// it finds there, so a copy re-written on the way would be a copy nobody can check.
pub fn touch_mac(ticket: &str, body: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(ticket.as_bytes());
    h.update(b"\n");
    h.update(body);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// The MAC header, when there is a pair ticket to sign the note with. Without one there is no
/// header at all — and the note still goes: the server keeps it as `touch-unverified` and bans no
/// one on it (TOUCH.md §3). A finding nobody ever sees is worse than a claim no one can check.
fn touch_mac_header(ticket: &str, report: &[u8]) -> Option<(&'static str, String)> {
    if ticket.is_empty() {
        return None;
    }
    Some((crate::vault::touch_header(), touch_mac(ticket, report)))
}

/// A hand on the game's own memory. Sent as `source=touch`, the body exactly as hashed above; the
/// server bans on the first finding it can verify (TOUCH.md §4). The words a ban arrives under
/// (the source, the header) come out of the vault: a `strings` pass is not to find them here.
pub fn touch(token: &str, build: i64, ticket: &str, report: &[u8]) -> Result<(), Fail> {
    let source = crate::vault::touch_source();
    if report.is_empty() {
        // no note, nothing to send
        return Err(Fail::Refused {
            status: 0,
            error: source.into(),
            body: json!({ "detail": "the note is empty" }),
        });
    }
    let url = format!("{}/api/crash?build={build}&source={source}", base());
    let auth = format!("Bearer {token}");
    let mac = touch_mac_header(ticket, report);
    let mut headers = vec![("Content-Type", "text/plain; charset=utf-8"), ("Authorization", auth.as_str())];
    if let Some((name, value)) = mac.as_ref() {
        headers.push((name, value.as_str()));
    }
    let head = http::request("POST", &url, &headers, Some(report), &mut |_, _| true).map_err(offline)?;
    if head.status >= 300 {
        return Err(Fail::Refused { status: head.status, error: source.into(), body: Value::Null });
    }
    Ok(())
}

/// A file of the store (`/api/dist/...`), piece by piece into `sink`. The status comes back: 200, or 403 when the leave
/// to download has run out (start again), 401 when the session has.
pub fn fetch(path: &str, token: &str, sink: &mut dyn FnMut(&[u8]) -> bool) -> Result<u16, Fail> {
    let url = format!("{}{path}", base());
    let auth = format!("Bearer {token}");
    let mut status = 0u16;
    let head = http::request("GET", &url, &[("Authorization", auth.as_str())], None, &mut |head, chunk| {
        status = head.status;
        head.status != 200 || sink(chunk)
    })
    .map_err(offline)?;
    Ok(if status == 0 { head.status } else { status })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Known answers for `sha256( ticket_utf8 || 0x0A || body )`, lowercase hex. Each line is
    /// re-derivable with any honest SHA-256: the bytes hashed are written out beside it.
    ///
    /// 1. ticket `"T"`, body empty — hashed bytes are `54 0A` ("T\n"):
    ///    `678f81a714fbc72030f82f9980054d5cf90e6f041a367f7da2f35b0f7dafb0e5`
    /// 2. ticket `"ticket"`, body `"hello"` — hashed bytes are `74 69 63 6b 65 74 0A 68 65 6c 6c 6f`
    ///    ("ticket\nhello"):
    ///    `2eff3c77eea7934c5719c4a5384ca4089ccc0511b2a21d20ba29439f8c17f0bd`
    /// 3. ticket `"abc"`, body `"def"` — hashed bytes are `61 62 63 0A 64 65 66` ("abc\ndef"):
    ///    `d53d6b91af7caf8fe3d8021f116270137c0079d579a1e16965da80c2ed138ffb`
    ///
    /// The 0x0A is load-bearing: `"ticket" || "hello"` with a space instead is
    /// `2c43865244ce74ba35aee30839dff909939d1e9167f1099e48c0e70f7f3df6f6`, with no separator at all
    /// `12f0665e4850c37f232f080a2163ac6c5f3604b412206042141de8c9b6323592` — neither is ours.
    #[test]
    fn the_mac_is_hex_lowercase_sha256_of_ticket_then_line_feed_then_body() {
        assert_eq!(touch_mac("T", b""), "678f81a714fbc72030f82f9980054d5cf90e6f041a367f7da2f35b0f7dafb0e5");
        assert_eq!(touch_mac("ticket", b"hello"), "2eff3c77eea7934c5719c4a5384ca4089ccc0511b2a21d20ba29439f8c17f0bd");
        assert_eq!(touch_mac("abc", b"def"), "d53d6b91af7caf8fe3d8021f116270137c0079d579a1e16965da80c2ed138ffb");
        // the halves are not run together, and the separator is a line feed and nothing else
        let other = |bytes: &[u8]| {
            let mut h = Sha256::new();
            h.update(b"ticket");
            h.update(bytes);
            h.update(b"hello");
            h.finalize().iter().map(|b| format!("{b:02x}")).collect::<String>()
        };
        assert_ne!(touch_mac("ticket", b"hello"), other(b" "));
        assert_ne!(touch_mac("ticket", b"hello"), other(b""));
    }

    /// A note is signed as the contract words it: sixty-four characters, and every one of them a
    /// lowercase hex digit — a MAC the server can read without being told how it was written.
    #[test]
    fn the_mac_is_sixty_four_lowercase_hex() {
        for (ticket, body) in [("T", &b""[..]), ("ticket", &b"hello"[..]), ("", &b"body"[..]), ("ticket-42", &b"touch: patch\n"[..])] {
            let mac = touch_mac(ticket, body);
            assert_eq!(mac.len(), 64, "{ticket:?}");
            assert!(mac.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)), "{mac}");
        }
    }

    /// The words the contract spells — the header's name and the source — come out of the vault as
    /// exactly those, not as our paraphrase. (The literals live here, in tests that never ship;
    /// the binaries hold only the noise the vault opens.)
    #[test]
    fn the_header_and_the_source_are_spelled_as_the_contract() {
        assert_eq!(crate::vault::touch_header(), "X-Cursed-Mac");
        assert_eq!(crate::vault::touch_source(), "touch");
        assert_eq!(crate::vault::touch_note(), "touch: ");
    }

    /// With a pair ticket the note is signed: the header's name is the contract's, and its value
    /// is that note's own MAC — sixty-four lowercase hex, the same one `touch_mac` would write.
    #[test]
    fn a_ticketed_note_carries_x_cursed_mac_over_its_own_body() {
        let (name, mac) = touch_mac_header("ticket", b"hello").expect("a ticket signs");
        assert_eq!(name, "X-Cursed-Mac");
        assert_eq!(mac, touch_mac("ticket", b"hello"));
        assert_eq!(mac.len(), 64);
        assert!(mac.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)), "{mac}");
    }

    /// No pair ticket, no MAC — and the note still goes out as one the server can keep
    /// (`touch-unverified`, never a ban; TOUCH.md §3). The finding is not thrown away for want
    /// of a signature; it is sent so that it can be seen, and is simply not believed into a ban.
    #[test]
    fn an_unsigned_note_has_no_mac_header_but_is_not_turned_away() {
        assert!(touch_mac_header("", b"touch: patch\n").is_none());
        assert!(touch_mac_header("", b"").is_none());
    }

    /// Nothing to say, nothing to send: an empty note is turned away here rather than posted as
    /// a finding. (With an empty body the request never leaves — this test would wait on the
    /// network if it did.)
    #[test]
    fn an_empty_note_is_refused_rather_than_sent() {
        let err = touch("session-token", 42, "ticket", b"").expect_err("an empty note must not go");
        assert_eq!(err.error(), crate::vault::touch_source());
        assert!(err.body()["detail"].as_str().is_some_and(|d| d.contains("empty")), "{}", err.body());
    }

    /// How the server writes a pair under the frame (`protect::framed`): the word, the two lengths, the two.
    fn framed(stub: &[u8], bin: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(PAIR_FRAME);
        out.extend_from_slice(&(stub.len() as u32).to_le_bytes());
        out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        out.extend_from_slice(stub);
        out.extend_from_slice(bin);
        out
    }

    /// A sealed game as the packer leaves it: its own word, a head of SEAL_HEAD, and a body of
    /// exactly the length the head names at [8..16] (`protect::seal_is_one`). Note that it is
    /// **not** `MZ` — a sealed file is the game under its seal, not a library.
    fn sealed(plain: usize) -> Vec<u8> {
        let mut bin = vec![0u8; SEAL_HEAD + plain];
        bin[..4].copy_from_slice(SEAL_WORD);
        bin[8..16].copy_from_slice(&(plain as u64).to_le_bytes());
        for b in bin[SEAL_HEAD..].iter_mut() {
            *b = 0xA5;
        }
        bin
    }

    /// The two halves of a pair are not the same kind of file and are not looked for under the same
    /// word. The stub is a library and starts `MZ`; the sealed game is the game under its seal and
    /// starts `CRB1` — `protect.rs` says so in as many words ("`MZ` is the stub's word and never this
    /// one's"). An earlier check here asked both for `MZ` and turned away every honest cut with
    /// "the seal is not one"; kept here so it cannot come back.
    #[test]
    fn the_two_halves_of_a_pair_are_asked_for_their_own_words() {
        let stub = b"MZ\x90\x00-a-library".to_vec();
        let bin = sealed(48);
        assert_eq!(&bin[..4], b"CRB1");
        assert_ne!(&bin[..2], b"MZ");
        let pair = open_pair(&framed(&stub, &bin), 200).expect("an honest cut is a pair");
        assert_eq!(pair.stub, stub);
        assert_eq!(pair.bin, bin);
    }

    /// A seal that is not a seal never goes in: a stub that is not a library, a game that is not a
    /// seal, or a body whose length the head does not name. Each is turned away here rather than
    /// written to disk for the game to find out later.
    #[test]
    fn a_half_that_is_not_its_own_kind_is_refused() {
        let stub = b"MZ\x90\x00-a-library".to_vec();
        let bin = sealed(48);
        // the game half under the library's word is not a seal, whatever its length
        let mut library_word = bin.clone();
        library_word[..4].copy_from_slice(b"MZ\x90\x00");
        assert!(open_pair(&framed(&stub, &library_word), 200).is_err(), "MZ is never the seal's word");
        // a stub that is not a library
        assert!(open_pair(&framed(b"not-a-library", &bin), 200).is_err());
        // the head names a body of another length
        let mut short = bin.clone();
        short[8..16].copy_from_slice(&(plain_len_of(&bin) + 1).to_le_bytes());
        assert!(open_pair(&framed(&stub, &short), 200).is_err());
        // a frame whose lengths do not add up to what came
        let mut lie = framed(&stub, &bin);
        let wrong = (stub.len() as u32 + 1).to_le_bytes();
        lie[8..12].copy_from_slice(&wrong);
        assert!(open_pair(&lie, 200).is_err());
    }

    fn plain_len_of(bin: &[u8]) -> u64 {
        u64::from_le_bytes(bin[8..16].try_into().unwrap())
    }
}
