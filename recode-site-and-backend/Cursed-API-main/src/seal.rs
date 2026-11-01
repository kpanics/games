//! The seal on what the loader and the game say to this server (the client's `native/src/net.rs`): AES-256-GCM
//! under a key that is the build's word and the computer's mark mixed, tied to the door it goes to, the minute it
//! was written in, and — for an answer — the letter it answers. On top of TLS, so that a library hooked onto the
//! client's HTTP and a certificate of someone else's both see letters they cannot read; one they did read cannot be
//! played again, carried to another door, or answered twice over.
//!
//! The two sides turn it on together. With no word here (CURSED_SEAL_KEY, sixty-four hex) the words are taken as
//! they always were and the answers come plain; with it, only sealed words are taken and only sealed answers given.
//! A client built with no word of its own speaks the old way, and the server that has the word turns it away — which
//! is how a build and this server go over to the seal in one step, and back again by taking the word out.
//!
//! An answer carries the letter it answers (`r`), and both are under one tag: an answer taken from this door is not
//! an answer to a later one. The game's side asks for that too — it will not open an answer that is not to its own
//! question (client `native/src/net.rs`).

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use axum::http::StatusCode;
use axum::response::Response;
use serde::de::DeserializeOwned;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::fail;

/// How an envelope is written (its `v`): one way here, one way down there.
const WAY: u64 = 2;
/// A letter is good for this long either way (s): what was written earlier is not played again, and a clock a
/// little off is still let in.
const AGE: i64 = 180;
/// The nonce, as the envelope carries it: twelve bytes, twice as many letters.
const NONCE: usize = 12;

/// The build's word, shared with every client built for this server. Its absence is not a failure: it is this
/// server speaking as it always did.
fn master() -> Option<[u8; 32]> {
    let text = std::env::var("CURSED_SEAL_KEY").ok().filter(|t| t.len() == 64)?;
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(text.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(out)
}

/// Whether this server keeps the seal (it has the word): then nothing comes in plain and nothing goes out plain.
pub fn keeping() -> bool {
    master().is_some()
}

/// The key the two sides speak under: the build's word and the computer's mark, one SHA-256 apart.
fn talk(mark: &str) -> Option<[u8; 32]> {
    let mut h = Sha256::new();
    h.update(b"cursed-seal/v1|");
    h.update(master()?);
    h.update(b"|");
    h.update(mark.as_bytes());
    let mut out = [0u8; 32];
    out.copy_from_slice(&h.finalize());
    Some(out)
}

/// What the seal is tied to: the door, the minute, the letter's own words, and — for an answer — the letter it
/// answers. A letter taken from one door does not open another; one from before does not open now; and an answer
/// taken once is not an answer to a later question.
fn tied(method: &str, path: &str, at: i64, nonce: &str, asked: &str) -> Vec<u8> {
    format!("{method}\n{path}\n{at}\n{nonce}\n{asked}").into_bytes()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 {
        return None;
    }
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok()).collect()
}

/// One AES-256-GCM call. Sealing gives the words with the tag at their end; opening takes them that way and only a
/// check of the tag leaves them worth anything. What the tag covers is the words and `tied` both.
fn gcm(encrypt: bool, key: &[u8; 32], nonce: &[u8; NONCE], tied: &[u8], data: &[u8]) -> Option<Vec<u8>> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let n = Nonce::from_slice(nonce);
    let what = Payload { msg: data, aad: tied };
    if encrypt {
        cipher.encrypt(n, what).ok()
    } else {
        cipher.decrypt(n, what).ok()
    }
}

/// The letters opened already, so that one is never taken twice. Only so much is kept as the window is wide — what
/// is older than that the clock refuses anyway.
static SEEN: OnceLock<Mutex<HashMap<String, i64>>> = OnceLock::new();

fn seen() -> MutexGuard<'static, HashMap<String, i64>> {
    SEEN.get_or_init(|| Mutex::new(HashMap::new())).lock().unwrap_or_else(|e| e.into_inner())
}

/// Whether this letter has been opened already. A letter only as old as the window may be new; what has been opened
/// once is never opened again.
fn first_time(mark: &str, nonce: &str, at: i64, now: i64) -> bool {
    let mut seen = seen();
    if seen.len() > 4096 {
        seen.retain(|_, when| now - *when <= AGE);
    }
    let key = format!("{mark}\n{nonce}");
    if seen.contains_key(&key) {
        return false;
    }
    seen.insert(key, at);
    true
}

/// The loader's and the game's words, opened. What comes back is whether they came sealed, the mark they were
/// sealed under (the answer goes under the same), the letter's own words (`asked`, which an answer names), and the
/// words themselves.
///
/// With no word on this server the words are taken as they always were. With one, nothing that is not a letter of
/// ours is taken at all.
pub fn words<T: DeserializeOwned>(method: &str, path: &str, body: &[u8]) -> Result<(bool, String, String, T), Response> {
    if !keeping() {
        let plain: T = serde_json::from_slice(body).map_err(|_| fail(StatusCode::BAD_REQUEST, "body"))?;
        return Ok((false, String::new(), String::new(), plain));
    }
    let envelope: Value = serde_json::from_slice(body).map_err(|_| fail(StatusCode::BAD_REQUEST, "seal"))?;
    if envelope["v"].as_u64() != Some(WAY) {
        return Err(fail(StatusCode::BAD_REQUEST, "seal"));
    }
    let mark = envelope["k"].as_str().filter(|s| s.len() == 64).ok_or_else(|| fail(StatusCode::BAD_REQUEST, "seal"))?;
    let nonce = envelope["n"].as_str().filter(|s| s.len() == NONCE * 2).ok_or_else(|| fail(StatusCode::BAD_REQUEST, "seal"))?;
    let at = envelope["t"].as_i64().ok_or_else(|| fail(StatusCode::BAD_REQUEST, "seal"))?;
    let now = crate::now();
    if (now - at).abs() > AGE {
        return Err(fail(StatusCode::BAD_REQUEST, "seal"));
    }
    if !first_time(mark, nonce, at, now) {
        // the same letter twice: the second one is not a letter at all any more
        return Err(fail(StatusCode::BAD_REQUEST, "seal"));
    }
    let key = talk(mark).ok_or_else(|| fail(StatusCode::SERVICE_UNAVAILABLE, "seal"))?;
    let raw = unhex(envelope["c"].as_str().ok_or_else(|| fail(StatusCode::BAD_REQUEST, "seal"))?).ok_or_else(|| fail(StatusCode::BAD_REQUEST, "seal"))?;
    if raw.len() < 16 {
        return Err(fail(StatusCode::BAD_REQUEST, "seal"));
    }
    let nonce_bytes: [u8; NONCE] = unhex(nonce).ok_or_else(|| fail(StatusCode::BAD_REQUEST, "seal"))?.as_slice().try_into().map_err(|_| fail(StatusCode::BAD_REQUEST, "seal"))?;
    // a letter answers nothing; its own words are what an answer will name
    let opened = gcm(false, &key, &nonce_bytes, &tied(method, path, at, nonce, ""), &raw).ok_or_else(|| fail(StatusCode::BAD_REQUEST, "seal"))?;
    let plain: T = serde_json::from_slice(&opened).map_err(|_| fail(StatusCode::BAD_REQUEST, "body"))?;
    Ok((true, mark.to_string(), nonce.to_string(), plain))
}

/// What goes back to the loader or the game, sealed under the same seal its words came with — or as it always was,
/// when they came that way. The answer names the letter it answers (`asked`, empty when the words came plain): one
/// taken from this door is not an answer to a later one. The words themselves come back here for the caller to give
/// them their own status.
pub fn said(sealed: bool, mark: &str, method: &str, path: &str, asked: &str, body: Value) -> Result<Value, Response> {
    if !sealed {
        return Ok(body);
    }
    let key = talk(mark).ok_or_else(|| fail(StatusCode::SERVICE_UNAVAILABLE, "seal"))?;
    let mut nonce = [0u8; NONCE];
    if !random(&mut nonce) {
        return Err(fail(StatusCode::SERVICE_UNAVAILABLE, "seal"));
    }
    let at = crate::now();
    let nonce_hex = hex(&nonce);
    let sealed_words = gcm(true, &key, &nonce, &tied(method, path, at, &nonce_hex, asked), body.to_string().as_bytes()).ok_or_else(|| fail(StatusCode::SERVICE_UNAVAILABLE, "seal"))?;
    Ok(serde_json::json!({ "v": WAY, "k": mark, "n": nonce_hex, "t": at, "r": asked, "c": hex(&sealed_words) }))
}

/// Twelve bytes of one's own for an answer: never the same twice over.
fn random(out: &mut [u8]) -> bool {
    use rand_core::{OsRng, RngCore};
    OsRng.try_fill_bytes(out).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The word the tests speak under, and the lock that keeps only one of them changing it at a time.
    static STAND: Mutex<()> = Mutex::new(());

    fn stand() -> std::sync::MutexGuard<'static, ()> {
        let guard = STAND.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CURSED_SEAL_KEY", "2a".repeat(32));
        // every test writes its own letters; what one opened is not another's
        seen().clear();
        guard
    }

    fn letter(method: &str, path: &str, told: &Value) -> String {
        let mark = "0b".repeat(32);
        let key = talk(&mark).expect("word");
        let mut nonce = [0u8; NONCE];
        assert!(random(&mut nonce), "a nonce of one's own");
        let at = crate::now();
        let nonce_hex = hex(&nonce);
        let sealed_words = gcm(true, &key, &nonce, &tied(method, path, at, &nonce_hex, ""), told.to_string().as_bytes()).expect("sealed");
        serde_json::json!({ "v": WAY, "k": mark, "n": nonce_hex, "t": at, "c": hex(&sealed_words) }).to_string()
    }

    #[test]
    fn the_words_actually_travel() {
        let _held = stand();
        let told = serde_json::json!({ "device": { "win": "ab".repeat(32) } });
        let (sealed, mark, asked, got) = words::<Value>("POST", "/api/loader/link", letter("POST", "/api/loader/link", &told).as_bytes()).expect("opened");
        assert!(sealed, "they came sealed");
        assert_eq!(mark, "0b".repeat(32));
        assert_eq!(asked.len(), NONCE * 2, "the answer will name this one");
        assert_eq!(got, told);
    }

    #[test]
    fn an_answer_is_not_an_answer_to_a_later_question() {
        let _held = stand();
        let told = serde_json::json!({ "state": "waiting" });
        let (sealed, mark, asked, _) = words::<Value>("POST", "/api/loader/link/poll", letter("POST", "/api/loader/link/poll", &told).as_bytes()).expect("opened");
        let answer = said(sealed, &mark, "POST", "/api/loader/link/poll", &asked, serde_json::json!({ "code": "RCD-5555" })).expect("said");
        // the answer names the letter it answers, and its seal is over that name
        assert_eq!(answer["r"].as_str(), Some(asked.as_str()), "the answer names the letter it answers");
        // the same answer offered as the answer to another letter: the name has to be the one it answered
        let mut other = answer.clone();
        other["r"] = serde_json::json!("ff".repeat(NONCE));
        let key = talk(&mark).expect("word");
        let nonce: [u8; NONCE] = unhex(other["n"].as_str().unwrap()).unwrap().as_slice().try_into().unwrap();
        let raw = unhex(other["c"].as_str().unwrap()).unwrap();
        assert!(
            gcm(false, &key, &nonce, &tied("POST", "/api/loader/link/poll", other["t"].as_i64().unwrap(), other["n"].as_str().unwrap(), other["r"].as_str().unwrap()), &raw).is_none(),
            "a name that is not the one it answered does not open"
        );
        // and the one it did answer: it opens
        assert!(gcm(false, &key, &nonce, &tied("POST", "/api/loader/link/poll", answer["t"].as_i64().unwrap(), answer["n"].as_str().unwrap(), &asked), &unhex(answer["c"].as_str().unwrap()).unwrap()).is_some());
    }

    #[test]
    fn a_letter_is_tied_to_the_door_it_came_to() {
        let _held = stand();
        let body = letter("POST", "/api/loader/link", &serde_json::json!({ "code": "RCD-1111" }));
        assert!(words::<Value>("POST", "/api/loader/recode/start", body.as_bytes()).is_err(), "another door does not take it");
        assert!(words::<Value>("GET", "/api/loader/link", body.as_bytes()).is_err());
    }

    #[test]
    fn a_letter_is_never_taken_twice() {
        let _held = stand();
        let body = letter("POST", "/api/loader/link", &serde_json::json!({ "code": "RCD-2222" }));
        assert!(words::<Value>("POST", "/api/loader/link", body.as_bytes()).is_ok());
        assert!(words::<Value>("POST", "/api/loader/link", body.as_bytes()).is_err(), "the second one is not a letter at all");
    }

    #[test]
    fn plain_words_are_not_taken_once_the_word_is_here() {
        let _held = stand();
        assert!(keeping());
        assert!(words::<Value>("POST", "/api/loader/link", b"{\"device\":{}}").is_err());
    }

    #[test]
    fn without_the_word_this_server_speaks_as_it_always_did() {
        let _held = stand();
        std::env::remove_var("CURSED_SEAL_KEY");
        assert!(!keeping());
        let (sealed, mark, asked, got) = words::<Value>("POST", "/api/loader/link", b"{\"device\":{}}").expect("plain");
        assert!(!sealed);
        assert!(mark.is_empty());
        assert!(asked.is_empty());
        assert_eq!(got, serde_json::json!({ "device": {} }));
        // and what it says comes back plain too
        let back = said(false, "", "POST", "/api/loader/link", "", serde_json::json!({ "code": "RCD-3333" })).expect("said");
        assert_eq!(back, serde_json::json!({ "code": "RCD-3333" }));
    }

    #[test]
    fn touched_words_do_not_open() {
        let _held = stand();
        let mut envelope: Value = serde_json::from_str(&letter("POST", "/api/loader/link", &serde_json::json!({ "code": "RCD-4444" }))).unwrap();
        let mut c = unhex(envelope["c"].as_str().unwrap()).unwrap();
        c[0] ^= 1;
        envelope["c"] = serde_json::json!(hex(&c));
        assert!(words::<Value>("POST", "/api/loader/link", envelope.to_string().as_bytes()).is_err(), "the tag covers the words");
    }
}
