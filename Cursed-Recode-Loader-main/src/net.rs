//! Requests to the site, through Windows' own HTTP stack (WinHTTP): its TLS, its proxy settings, its HTTP/2, no
//! library of our own for any of it. Everything here blocks; it runs on threads of its own (license.rs), never on the
//! game's. The same as Cursed's (its native/src/net/http.rs).

// Cursed's whole: what the game does not use yet the loader will (its downloads).
#![allow(dead_code)]

use std::cell::RefCell;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use windows_sys::Win32::Foundation::GetLastError;
use windows_sys::Win32::Networking::WinHttp::*;
use windows_sys::Win32::Security::Cryptography::*;

/// One session for the whole client: WinHTTP keeps its connections alive and shares them between the workers.
struct Session(*mut c_void);
unsafe impl Send for Session {}
unsafe impl Sync for Session {}

static SESSION: OnceLock<Option<Session>> = OnceLock::new();
/// A second session only for our own site checked against ISRG_ROOTS. Its own pool of connections: one let through
/// with an unknown root is never reused by a request of the main session (which would not check it again); every
/// request of this one is checked, a reused connection too.
static OWN_SESSION: OnceLock<Option<Session>> = OnceLock::new();
/// Our site has needed ISRG_ROOTS once: its requests go to OWN_SESSION straight away.
static OWN_ROOTS_NEEDED: AtomicBool = AtomicBool::new(false);
/// Why the session could not be opened (GetLastError of WinHttpOpen), 0 when it was.
static SESSION_ERROR: AtomicU32 = AtomicU32::new(0);

const DECOMPRESS_ALL: u32 = 0x3;

/// Let's Encrypt's roots (ISRG Root X1 and X2, as letsencrypt.org gives them). Only for our own site, and only when
/// Windows itself does not know the root (a store that never got it: Windows Update cut off). Browsers carry their own
/// roots the same way, which is why the site opens in them while WinHTTP says "unknown CA".
static ISRG_ROOTS: [&[u8]; 2] = [include_bytes!("../assets/certs/isrg-root-x1.der"), include_bytes!("../assets/certs/isrg-root-x2.der")];

thread_local! {
    /// What the last failed request on this thread came to, in more words than its error: the step and Windows' code
    /// ("tls 12175 ca", "dns 12007"...). The error words stay as they were (the menus match them); the loader shows this.
    static DETAIL: RefCell<String> = const { RefCell::new(String::new()) };
}

/// The last failure on this thread in detail, "" when the last request went through.
pub fn last_failure() -> String {
    DETAIL.with(|d| d.borrow().clone())
}

fn note(detail: String) {
    DETAIL.with(|d| *d.borrow_mut() = detail);
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn session() -> Option<*mut c_void> {
    SESSION.get_or_init(open_session).as_ref().map(|s| s.0)
}

fn own_session() -> Option<*mut c_void> {
    OWN_SESSION.get_or_init(open_session).as_ref().map(|s| s.0)
}

fn open_session() -> Option<Session> {
    unsafe {
        // Modrinth asks every client to say who it is.
        let agent = wide(&format!("Cursed Recode/{} (+{}; Minecraft 26.3)", env!("CARGO_PKG_VERSION"), crate::vault::ours()));
        let mut h = WinHttpOpen(agent.as_ptr(), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, std::ptr::null(), std::ptr::null(), 0);
        if h.is_null() {
            // AUTOMATIC_PROXY is Windows 8.1 and newer; older ones take the WinHTTP proxy settings instead.
            SESSION_ERROR.store(GetLastError(), Ordering::Relaxed);
            h = WinHttpOpen(agent.as_ptr(), WINHTTP_ACCESS_TYPE_DEFAULT_PROXY, std::ptr::null(), std::ptr::null(), 0);
        }
        if h.is_null() {
            SESSION_ERROR.store(GetLastError(), Ordering::Relaxed);
            return None;
        }
        SESSION_ERROR.store(0, Ordering::Relaxed);
        // Resolve, connect, send, receive: a pack of 250 MB over a slow line still arrives, a dead server is let go.
        WinHttpSetTimeouts(h, 10_000, 10_000, 30_000, 30_000);
        let flags: u32 = DECOMPRESS_ALL;
        WinHttpSetOption(h, WINHTTP_OPTION_DECOMPRESSION, &flags as *const u32 as *const c_void, 4);
        let http2: u32 = WINHTTP_PROTOCOL_FLAG_HTTP2;
        WinHttpSetOption(h, WINHTTP_OPTION_ENABLE_HTTP_PROTOCOL, &http2 as *const u32 as *const c_void, 4);
        Some(Session(h))
    }
}

struct Handle(*mut c_void);
impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { WinHttpCloseHandle(self.0) };
        }
    }
}

/// An address taken apart: whether it is https, the host, the port, and the path with its query.
pub struct Url {
    pub secure: bool,
    pub host: String,
    pub port: u16,
    pub path: String,
}

pub fn parse(url: &str) -> Option<Url> {
    let (secure, rest) = if let Some(r) = url.strip_prefix("https://") {
        (true, r)
    } else if let Some(r) = url.strip_prefix("http://") {
        (false, r)
    } else {
        return None;
    };
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) if !h.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) => (h, p.parse().ok()?),
        _ => (authority, if secure { 443 } else { 80 }),
    };
    if host.is_empty() {
        return None;
    }
    Some(Url { secure, host: host.to_string(), port, path: path.to_string() })
}

/// What came back: its status and, when asked for, its headers.
pub struct Head {
    pub status: u16,
    /// The length the server announced, 0 when it did not say.
    pub length: u64,
    /// The answer's tag (ETag), "" when it has none: asked again with it, an unchanged answer is only "not modified".
    pub etag: String,
}

/// Our own site: the only one whose certificate may also be checked against ISRG_ROOTS.
fn ours(host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    host == "cursedclient.xyz" || host.ends_with(".cursedclient.xyz")
}

/// What WinHTTP tells about the request in flight (its status callback, through the request's context value).
struct Watch {
    /// WINHTTP_CALLBACK_STATUS_FLAG_* of a refused certificate.
    secure: u32,
    /// Check the server's certificate against ISRG_ROOTS before anything is sent (Windows' own check is told to let an
    /// unknown root through).
    own_roots: bool,
    host: String,
    /// The own check failed and the request was closed before sending.
    refused: bool,
}

unsafe extern "system" fn on_status(h: *mut c_void, context: usize, status: u32, info: *mut c_void, length: u32) {
    let Some(watch) = (context as *mut Watch).as_mut() else { return };
    if status == WINHTTP_CALLBACK_STATUS_SECURE_FAILURE && !info.is_null() && length >= 4 {
        watch.secure |= *(info as *const u32);
    } else if status == WINHTTP_CALLBACK_STATUS_SENDING_REQUEST && watch.own_roots && !watch.refused {
        // The documented moment for a check of one's own: the connection is up, nothing of the request has gone out.
        let mut cert: *mut CERT_CONTEXT = std::ptr::null_mut();
        let mut size = std::mem::size_of::<*mut CERT_CONTEXT>() as u32;
        let got = WinHttpQueryOption(h, WINHTTP_OPTION_SERVER_CERT_CONTEXT, &mut cert as *mut _ as *mut c_void, &mut size) != 0 && !cert.is_null();
        let trusted = got && by_own_roots(cert, &watch.host);
        if !cert.is_null() {
            CertFreeCertificateContext(cert);
        }
        if !trusted {
            // Closing it here cancels the send (ERROR_WINHTTP_OPERATION_CANCELLED): the server gets nothing.
            watch.refused = true;
            WinHttpCloseHandle(h);
        }
    }
}

/// The server's certificate checked as Windows would, but with only ISRG_ROOTS for roots: the chain it sent, the dates,
/// the signatures, server use, and the name.
unsafe fn by_own_roots(cert: *const CERT_CONTEXT, host: &str) -> bool {
    let roots = CertOpenStore(CERT_STORE_PROV_MEMORY, 0, 0, 0, std::ptr::null());
    if roots.is_null() {
        return false;
    }
    for der in ISRG_ROOTS {
        CertAddEncodedCertificateToStore(roots, X509_ASN_ENCODING, der.as_ptr(), der.len() as u32, CERT_STORE_ADD_ALWAYS, std::ptr::null_mut());
    }
    let mut config: CERT_CHAIN_ENGINE_CONFIG = std::mem::zeroed();
    config.cbSize = std::mem::size_of::<CERT_CHAIN_ENGINE_CONFIG>() as u32;
    config.hExclusiveRoot = roots;
    let mut engine: HCERTCHAINENGINE = 0;
    let mut trusted = false;
    if CertCreateCertificateChainEngine(&config, &mut engine) != 0 {
        let mut server_auth = szOID_PKIX_KP_SERVER_AUTH as *mut u8;
        let mut para: CERT_CHAIN_PARA = std::mem::zeroed();
        para.cbSize = std::mem::size_of::<CERT_CHAIN_PARA>() as u32;
        para.RequestedUsage.dwType = USAGE_MATCH_TYPE_AND;
        para.RequestedUsage.Usage.cUsageIdentifier = 1;
        para.RequestedUsage.Usage.rgpszUsageIdentifier = &mut server_auth;
        let mut chain: *mut CERT_CHAIN_CONTEXT = std::ptr::null_mut();
        // The server sends the whole chain: nothing to fetch.
        if CertGetCertificateChain(engine, cert, std::ptr::null(), (*cert).hCertStore, &para, CERT_CHAIN_CACHE_ONLY_URL_RETRIEVAL, std::ptr::null(), &mut chain) != 0 && !chain.is_null() {
            let mut name = wide(host);
            let mut https: HTTPSPolicyCallbackData = std::mem::zeroed();
            https.Anonymous.cbStruct = std::mem::size_of::<HTTPSPolicyCallbackData>() as u32;
            https.dwAuthType = AUTHTYPE_SERVER;
            https.pwszServerName = name.as_mut_ptr();
            let mut policy: CERT_CHAIN_POLICY_PARA = std::mem::zeroed();
            policy.cbSize = std::mem::size_of::<CERT_CHAIN_POLICY_PARA>() as u32;
            policy.pvExtraPolicyPara = &mut https as *mut _ as *mut c_void;
            let mut status: CERT_CHAIN_POLICY_STATUS = std::mem::zeroed();
            status.cbSize = std::mem::size_of::<CERT_CHAIN_POLICY_STATUS>() as u32;
            trusted = CertVerifyCertificateChainPolicy(CERT_CHAIN_POLICY_SSL, chain, &policy, &mut status) != 0 && status.dwError == 0;
            CertFreeCertificateChain(chain);
        }
        CertFreeCertificateChainEngine(engine);
    }
    CertCloseStore(roots, 0);
    trusted
}

/// Does one request. The body of the answer goes to `sink` piece by piece as it arrives, with the head first
/// known; `sink` returning false stops it (a download cancelled). An error is a short word of what failed
/// (`last_failure` has it in detail).
pub fn request(method: &str, url: &str, headers: &[(&str, &str)], body: Option<&[u8]>, sink: &mut dyn FnMut(&Head, &[u8]) -> bool) -> Result<Head, String> {
    note(String::new());
    let Some(url) = parse(url) else {
        note("address".into());
        return Err("address".into());
    };
    let Some(session) = session() else {
        note(format!("winhttp-open {}", SESSION_ERROR.load(Ordering::Relaxed)));
        return Err("winhttp".into());
    };
    let own = url.secure && ours(&url.host);
    if own && OWN_ROOTS_NEEDED.load(Ordering::Relaxed) {
        if let Some(own_session) = own_session() {
            return with_own_roots(own_session, &url, method, headers, body, sink);
        }
    }
    let mut watch = Watch { secure: 0, own_roots: false, host: url.host.clone(), refused: false };
    let first = attempt(session, &url, method, headers, body, sink, &mut watch);
    // Windows does not know the root of our own certificate: once more with ours (nothing has come to `sink` yet,
    // a refused certificate stops before any answer).
    if first.is_err() && own && watch.secure & WINHTTP_CALLBACK_STATUS_FLAG_INVALID_CA != 0 {
        if let Some(own_session) = own_session() {
            let second = with_own_roots(own_session, &url, method, headers, body, sink);
            if second.is_ok() {
                OWN_ROOTS_NEEDED.store(true, Ordering::Relaxed);
            }
            return second;
        }
    }
    first
}

/// A request of OWN_SESSION: Windows lets an unknown root through, and the callback checks the server against
/// ISRG_ROOTS before anything is sent.
fn with_own_roots(own_session: *mut c_void, url: &Url, method: &str, headers: &[(&str, &str)], body: Option<&[u8]>, sink: &mut dyn FnMut(&Head, &[u8]) -> bool) -> Result<Head, String> {
    let mut watch = Watch { secure: 0, own_roots: true, host: url.host.clone(), refused: false };
    let got = attempt(own_session, url, method, headers, body, sink, &mut watch);
    if got.is_err() && watch.refused {
        note("tls own-roots".into());
        return Err("tls".into());
    }
    got
}

fn attempt(session: *mut c_void, url: &Url, method: &str, headers: &[(&str, &str)], body: Option<&[u8]>, sink: &mut dyn FnMut(&Head, &[u8]) -> bool, watch: &mut Watch) -> Result<Head, String> {
    unsafe {
        let host = wide(&url.host);
        let connection = Handle(WinHttpConnect(session, host.as_ptr(), url.port, 0));
        if connection.0.is_null() {
            let code = GetLastError();
            note(format!("connect {code}"));
            return Err(format!("connect {code}"));
        }
        let verb = wide(method);
        let path = wide(&url.path);
        let flags = if url.secure { WINHTTP_FLAG_SECURE } else { 0 };
        let mut request = Handle(WinHttpOpenRequest(connection.0, verb.as_ptr(), path.as_ptr(), std::ptr::null(), std::ptr::null(), std::ptr::null(), flags));
        if request.0.is_null() {
            let code = GetLastError();
            note(format!("open {code}"));
            return Err(format!("open {code}"));
        }
        // `watch` outlives `request` (it is the caller's): the callback never sees it gone.
        let context = watch as *mut Watch as usize;
        WinHttpSetOption(request.0, WINHTTP_OPTION_CONTEXT_VALUE, &context as *const usize as *const c_void, std::mem::size_of::<usize>() as u32);
        WinHttpSetStatusCallback(request.0, Some(on_status), WINHTTP_CALLBACK_FLAG_SECURE_FAILURE | WINHTTP_CALLBACK_STATUS_SENDING_REQUEST, 0);
        if watch.own_roots {
            let ignore: u32 = SECURITY_FLAG_IGNORE_UNKNOWN_CA;
            WinHttpSetOption(request.0, WINHTTP_OPTION_SECURITY_FLAGS, &ignore as *const u32 as *const c_void, 4);
        }
        let mut lines = String::new();
        for (name, value) in headers {
            lines.push_str(name);
            lines.push_str(": ");
            lines.push_str(value);
            lines.push_str("\r\n");
        }
        let lines = wide(&lines);
        let (data, size) = match body {
            Some(b) => (b.as_ptr() as *const c_void, b.len() as u32),
            None => (std::ptr::null(), 0),
        };
        // The headers' length: -1 means "up to the terminating zero".
        if WinHttpSendRequest(request.0, lines.as_ptr(), u32::MAX, data, size, size, 0) == 0 {
            let code = GetLastError();
            if watch.refused {
                // The callback has closed it already.
                request.0 = std::ptr::null_mut();
            }
            return Err(failed(code, watch));
        }
        if WinHttpReceiveResponse(request.0, std::ptr::null_mut()) == 0 {
            return Err(failed(GetLastError(), watch));
        }
        let status = number(request.0, WINHTTP_QUERY_STATUS_CODE).unwrap_or(0) as u16;
        let length = number(request.0, WINHTTP_QUERY_CONTENT_LENGTH).unwrap_or(0);
        let etag = text(request.0, WINHTTP_QUERY_ETAG).unwrap_or_default();
        let head = Head { status, length, etag };
        let mut buffer = vec![0u8; 64 * 1024];
        loop {
            // What has come so far, read as it is: a stream (the live line) gives its events one at a time, and a read
            // asked for more than has come would wait for the rest of a buffer that never fills.
            let mut ready = 0u32;
            if WinHttpQueryDataAvailable(request.0, &mut ready) == 0 {
                return Err(failed(GetLastError(), watch));
            }
            if ready == 0 {
                break;
            }
            let mut read = 0u32;
            let want = (ready as usize).min(buffer.len()) as u32;
            if WinHttpReadData(request.0, buffer.as_mut_ptr() as *mut c_void, want, &mut read) == 0 {
                return Err(failed(GetLastError(), watch));
            }
            if read == 0 {
                break;
            }
            if !sink(&head, &buffer[..read as usize]) {
                return Err("cancelled".into());
            }
        }
        Ok(head)
    }
}

/// A numeric header (the status, the length), if the answer has it.
unsafe fn number(request: *mut c_void, what: u32) -> Option<u64> {
    let mut value = 0u64;
    let mut size = 8u32;
    // WINHTTP_QUERY_FLAG_NUMBER64 gives a 64-bit number (a pack's length can pass 4 GB in principle).
    const NUMBER64: u32 = 0x0800_0000;
    if WinHttpQueryHeaders(request, what | NUMBER64, std::ptr::null(), &mut value as *mut u64 as *mut c_void, &mut size, std::ptr::null_mut()) != 0 {
        return Some(value);
    }
    let mut small = 0u32;
    let mut size = 4u32;
    (WinHttpQueryHeaders(request, what | WINHTTP_QUERY_FLAG_NUMBER, std::ptr::null(), &mut small as *mut u32 as *mut c_void, &mut size, std::ptr::null_mut()) != 0).then_some(small as u64)
}

/// A header as text, if the answer has it.
unsafe fn text(request: *mut c_void, what: u32) -> Option<String> {
    let mut buf = vec![0u16; 256];
    let mut size = (buf.len() * 2) as u32;
    if WinHttpQueryHeaders(request, what, std::ptr::null(), buf.as_mut_ptr() as *mut c_void, &mut size, std::ptr::null_mut()) == 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buf[..(size / 2) as usize]))
}

/// A failure noted in detail for `last_failure` (the step, Windows' code, why a certificate was refused), and given
/// back as the word it always was.
fn failed(code: u32, watch: &Watch) -> String {
    let step = match code {
        12007 => "dns",                                  // the name did not resolve
        12029 => "connect",                              // no connection could be made (blocked, firewall, server down)
        12030 | 12031 => "reset",                        // the connection was cut or reset on the way
        12002 => "timeout",
        12175 | 12045 | 12037 | 12038 | 12157 | 12169 | 12170 | 12179 => "tls",
        12017 if watch.refused => "tls",                 // our own check closed it
        12166 | 12167 | 12178 | 12180 | 12185 => "proxy", // the proxy settings or their script
        _ => "net",
    };
    let mut detail = format!("{step} {code}");
    let why = [
        (WINHTTP_CALLBACK_STATUS_FLAG_INVALID_CA, "ca"),
        (WINHTTP_CALLBACK_STATUS_FLAG_CERT_DATE_INVALID, "date"),
        (WINHTTP_CALLBACK_STATUS_FLAG_CERT_CN_INVALID, "name"),
        (WINHTTP_CALLBACK_STATUS_FLAG_CERT_REVOKED, "revoked"),
        (WINHTTP_CALLBACK_STATUS_FLAG_INVALID_CERT, "cert"),
        (WINHTTP_CALLBACK_STATUS_FLAG_CERT_WRONG_USAGE, "usage"),
        (WINHTTP_CALLBACK_STATUS_FLAG_SECURITY_CHANNEL_ERROR, "channel"),
    ];
    for (flag, word) in why {
        if watch.secure & flag != 0 {
            detail.push(' ');
            detail.push_str(word);
        }
    }
    if watch.refused {
        detail.push_str(" own-roots");
    }
    note(detail);
    offline(code)
}

/// The failures that mean "no connection" read as one word the menu can say.
fn offline(code: u32) -> String {
    match code {
        12002 => "timeout".into(),                       // ERROR_WINHTTP_TIMEOUT
        12007 => "offline".into(),                       // ERROR_WINHTTP_NAME_NOT_RESOLVED
        12029 | 12030 | 12031 => "offline".into(),       // cannot connect, connection aborted, reset
        12175 | 12045 | 12037 | 12038 => "tls".into(),   // secure failure, invalid CA, dates, name
        other => format!("net {other}"),
    }
}

// ---- the seal on the conversation with our own --------------------------------------------------------

/// How an envelope is written (its `v`): one way here, one way up there, and a letter of another way is not opened.
const SEAL_WAY: u64 = 2;
/// A letter is good for this long either way (s). The one up there holds to it too: what was written five minutes
/// ago is not played again, and a clock five minutes off is still let in.
const SEAL_AGE: i64 = 180;
/// The nonce, as the envelope carries it: twelve bytes, twice as many letters.
const SEAL_NONCE: usize = 12;

/// What the loader and the game say to our own server is sealed on the way out and opened on the way in, on top of
/// TLS. A library hooked onto WinHTTP inside the program (the usual way to watch what a client sends) and a
/// certificate of someone else's both see a letter they cannot read; and one they did read cannot be played again or
/// carried to another door — every letter is tied to the door it is for and to the minute it was written in.
///
/// The envelope is plain JSON around the sealed words: `v` (how it is written), `k` (this computer's mark — not a
/// secret; it is what the key is mixed from), `n` (its own twelve bytes, never twice over), `t` (when), `c` (the
/// words with the tag at their end, hex). The key is the build's own word mixed with the mark, so one computer's
/// letters are not another's.
///
/// Honest about its bounds: the build's word lives inside the program, and someone who unpacks the program reads
/// what it says. What this stops is the watching of a program that runs and the replay of what was watched.

/// The build's own word, shared with the server: baked in at the build (CURSED_SEAL_KEY, sixty-four hex), or one
/// given at the start (CURSED_SEAL) for a stand. A client without one sends its words as they always were — the
/// server that has the word will not take them, and that is how the two turn on together.
pub fn master() -> Option<[u8; 32]> {
    let text = std::env::var(crate::vault::env_seal()).ok().map(|t| t).or_else(|| { let k = crate::vault::seal_key(); if k.is_empty() { None } else { Some(k.to_string()) } }).filter(|t| t.len() == 64)?;
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(text.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(out)
}

/// Whether this one seals at all: it has the word, so its words go sealed and only sealed answers are opened.
pub fn sealing() -> bool {
    master().is_some()
}

/// This computer's mark (`k`): the parts of it the server already knows, hashed together. Not a secret — it is what
/// the build's word is mixed with, so that one computer's letters are not another's.
pub fn mark() -> String {
    static MARK: OnceLock<String> = OnceLock::new();
    MARK.get_or_init(|| {
        let text = crate::device::parts_json().to_string();
        let mut h = Sha256::new();
        h.update(crate::vault::mark_kdf().as_bytes());
        h.update(text.as_bytes());
        h.finalize().iter().map(|b| format!("{b:02x}")).collect()
    })
    .clone()
}

/// The key the two sides speak under: the build's word and this computer's mark, one SHA-256 apart.
fn talk(mark: &str) -> Option<[u8; 32]> {
    let mut h = Sha256::new();
    h.update(crate::vault::seal_kdf().as_bytes());
    h.update(master()?);
    h.update(b"|");
    h.update(mark.as_bytes());
    let mut out = [0u8; 32];
    out.copy_from_slice(&h.finalize());
    Some(out)
}

/// What the seal is tied to: the door it goes to, the minute it was written in, the letter's own words, and — for
/// an answer — the letter it answers. A letter taken from one door does not open another; one from before does not
/// open now; and an answer taken once is not an answer to a later question.
fn tied(method: &str, path: &str, at: i64, nonce: &str, asked: &str) -> Vec<u8> {
    format!("{method}\n{path}\n{at}\n{nonce}\n{asked}").into_bytes()
}

fn unix_now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
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

/// One's own twelve bytes for a letter: never the same twice over.
fn twelve() -> Option<[u8; SEAL_NONCE]> {
    let mut n = [0u8; SEAL_NONCE];
    let status = unsafe { BCryptGenRandom(std::ptr::null_mut(), n.as_mut_ptr(), n.len() as u32, BCRYPT_USE_SYSTEM_PREFERRED_RNG) };
    (status == 0).then_some(n)
}

/// One AES-256-GCM call through the system's own. `encrypt` seals `data` in place and writes the tag; `decrypt`
/// takes the tag as it stands and only a check of it leaves `data` worth anything. What the tag covers is the words
/// and `tied` both, so neither can be moved without the other.
fn gcm(encrypt: bool, key: &[u8; 32], nonce: &[u8; SEAL_NONCE], tied: &[u8], data: &mut [u8], tag: &mut [u8; 16]) -> Result<(), i32> {
    type Alg = *mut c_void;
    type Key = *mut c_void;
    const OK: i32 = 0;
    unsafe {
        let mut alg: Alg = std::ptr::null_mut();
        let mut status = BCryptOpenAlgorithmProvider(&mut alg, BCRYPT_AES_ALGORITHM, std::ptr::null(), 0);
        if status != OK {
            return Err(status);
        }
        let close = |alg: Alg| {
            BCryptCloseAlgorithmProvider(alg, 0);
        };
        status = BCryptSetProperty(alg, BCRYPT_CHAINING_MODE, BCRYPT_CHAIN_MODE_GCM as *const u8, 32, 0);
        if status != OK {
            close(alg);
            return Err(status);
        }
        let mut object_len = 0u32;
        let mut copied = 0u32;
        status = BCryptGetProperty(alg, BCRYPT_OBJECT_LENGTH, &mut object_len as *mut u32 as *mut u8, 4, &mut copied, 0);
        if status != OK {
            close(alg);
            return Err(status);
        }
        let mut object = vec![0u8; object_len as usize];
        let mut hkey: Key = std::ptr::null_mut();
        status = BCryptGenerateSymmetricKey(alg, &mut hkey, object.as_mut_ptr(), object.len() as u32, key.as_ptr(), key.len() as u32, 0);
        if status != OK {
            close(alg);
            return Err(status);
        }
        let info = BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO {
            cbSize: core::mem::size_of::<BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO>() as u32,
            dwInfoVersion: BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO_VERSION,
            pbNonce: nonce.as_ptr() as *mut u8,
            cbNonce: nonce.len() as u32,
            pbAuthData: tied.as_ptr() as *mut u8,
            cbAuthData: tied.len() as u32,
            pbTag: tag.as_mut_ptr(),
            cbTag: tag.len() as u32,
            pbMacContext: std::ptr::null_mut(),
            cbMacContext: 0,
            cbAAD: 0,
            cbData: data.len() as u64,
            dwFlags: 0,
        };
        let mut written = 0u32;
        status = if encrypt {
            BCryptEncrypt(hkey, data.as_ptr(), data.len() as u32, &info as *const _ as *const c_void, std::ptr::null_mut(), 0, data.as_mut_ptr(), data.len() as u32, &mut written, 0)
        } else {
            BCryptDecrypt(hkey, data.as_ptr(), data.len() as u32, &info as *const _ as *const c_void, std::ptr::null_mut(), 0, data.as_mut_ptr(), data.len() as u32, &mut written, 0)
        };
        BCryptDestroyKey(hkey);
        close(alg);
        if status != OK {
            return Err(status);
        }
        Ok(())
    }
}

/// One's words to the server, sealed, and the letter's own words (`n`) with them: an answer has to name this one
/// before it is believed. `path` is the door alone (as it is asked for, without the site's own address): the seal is
/// tied to it, and an answer is opened under the same.
pub fn seal(method: &str, path: &str, body: &serde_json::Value) -> Option<(serde_json::Value, String)> {
    let mark = mark();
    let key = talk(&mark)?;
    let nonce = twelve()?;
    let at = unix_now();
    let nonce_hex = hex(&nonce);
    let mut data = body.to_string().into_bytes();
    let mut tag = [0u8; 16];
    gcm(true, &key, &nonce, &tied(method, path, at, &nonce_hex, ""), &mut data, &mut tag).ok()?;
    data.extend_from_slice(&tag);
    Some((serde_json::json!({ "v": SEAL_WAY, "k": mark, "n": nonce_hex, "t": at, "c": hex(&data) }), nonce_hex))
}

/// The answers opened already, so that one is never taken twice here either. Only so much is kept as the window is
/// wide — what is older than that the clock refuses anyway. Written after the tag holds: a letter that was never
/// ours does not spend another one's place.
static HEARD: OnceLock<std::sync::Mutex<std::collections::HashMap<String, i64>>> = OnceLock::new();

fn heard(nonce: &str) -> bool {
    HEARD.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new())).lock().unwrap_or_else(|e| e.into_inner()).contains_key(nonce)
}

fn remember(nonce: &str) {
    let mut map = HEARD.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new())).lock().unwrap_or_else(|e| e.into_inner());
    if map.len() > 1024 {
        let now = unix_now();
        map.retain(|_, when| now - *when <= SEAL_AGE);
    }
    map.insert(nonce.to_string(), unix_now());
}

/// The server's words, opened. Anything that is not an envelope of ours is not believed, and anything that is not
/// the answer to this one's own question is not believed either — `asked` is the letter that went out. The words
/// within are only worth as much as the tag that came with them.
pub fn open(method: &str, path: &str, asked: &str, envelope: &serde_json::Value) -> Option<serde_json::Value> {
    if envelope["v"].as_u64() != Some(SEAL_WAY) {
        return None;
    }
    let mark = envelope["k"].as_str().filter(|s| s.len() == 64)?;
    let nonce_hex = envelope["n"].as_str().filter(|s| s.len() == SEAL_NONCE * 2)?;
    let at = envelope["t"].as_i64()?;
    if (unix_now() - at).abs() > SEAL_AGE {
        return None;
    }
    // an answer names the letter it answers; one that names another is not this one's answer
    if envelope["r"].as_str().unwrap_or("") != asked {
        return None;
    }
    if heard(nonce_hex) {
        return None;
    }
    let nonce: [u8; SEAL_NONCE] = unhex(nonce_hex)?.as_slice().try_into().ok()?;
    let mut data = unhex(envelope["c"].as_str()?)?;
    if data.len() < 16 {
        return None;
    }
    let split = data.len() - 16;
    let mut tag = [0u8; 16];
    tag.copy_from_slice(&data[split..]);
    data.truncate(split);
    gcm(false, &talk(mark)?, &nonce, &tied(method, path, at, nonce_hex, asked), &mut data, &mut tag).ok()?;
    let plain = serde_json::from_slice(&data).ok();
    if plain.is_some() {
        // the tag held: this one is spent. A letter that was never ours did not spend it.
        remember(nonce_hex);
    }
    plain
}

#[cfg(test)]
mod seal_tests {
    use super::*;
    use serde_json::json;
    use std::sync::{Mutex, MutexGuard};

    /// The word is process-wide and the tests run side by side: one stand at a time.
    static STAND: Mutex<()> = Mutex::new(());

    /// The word the seal's tests speak under: one of their own, never one of a build. Every letter here is sealed
    /// and opened under the same word, the way a build and its server share one.
    fn stand() -> MutexGuard<'static, ()> {
        let guard = STAND.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var(crate::vault::env_seal(), "2a".repeat(32));
        guard
    }

    /// An answer, as the server writes it: sealed under the same seal, naming the letter it answers.
    fn answer(asked: &str, said: &serde_json::Value) -> serde_json::Value {
        let mark = crate::net::mark();
        let key = talk(&mark).expect("word");
        let nonce = twelve().expect("a nonce of one's own");
        let at = unix_now();
        let nonce_hex = hex(&nonce);
        let mut data = said.to_string().into_bytes();
        let mut tag = [0u8; 16];
        gcm(true, &key, &nonce, &tied("POST", "/api/loader/link/poll", at, &nonce_hex, asked), &mut data, &mut tag).expect("sealed");
        data.extend_from_slice(&tag);
        json!({ "v": SEAL_WAY, "k": mark, "n": nonce_hex, "t": at, "r": asked, "c": hex(&data) })
    }

    #[test]
    fn the_words_actually_travel() {
        let _held = stand();
        let words = json!({ "device": { "win": "ab".repeat(32) }, "n": 7 });
        let (letter, asked) = seal("POST", "/api/loader/link", &words).expect("sealed");
        // a letter goes one way; what comes back is an answer to it
        let got = open("POST", "/api/loader/link/poll", &asked, &answer(&asked, &json!({ "code": "RCD-1111" }))).expect("opened");
        assert_eq!(got, json!({ "code": "RCD-1111" }));
        assert_eq!(letter["v"], json!(SEAL_WAY));
    }

    #[test]
    fn an_answer_to_another_letter_is_not_this_ones() {
        let _held = stand();
        let (_, asked) = seal("POST", "/api/loader/link/poll", &json!({ "state": "waiting" })).expect("sealed");
        let said = answer(&asked, &json!({ "code": "RCD-2222" }));
        // the one it answers opens
        assert!(open("POST", "/api/loader/link/poll", &asked, &said).is_some());
        // the same answer offered to another question: not this one's
        assert!(open("POST", "/api/loader/link/poll", &"ff".repeat(SEAL_NONCE), &said).is_none());
        // and the same question again: the answer was taken once already
        assert!(open("POST", "/api/loader/link/poll", &asked, &said).is_none(), "an answer is taken once");
        // a name that is not in the letter at all: its seal is over the one it answered
        let mut renamed = said.clone();
        renamed["r"] = json!("ee".repeat(SEAL_NONCE));
        assert!(open("POST", "/api/loader/link/poll", &"ee".repeat(SEAL_NONCE), &renamed).is_none());
    }

    #[test]
    fn a_letter_is_tied_to_the_door_it_went_to() {
        let _held = stand();
        let (letter, _) = seal("POST", "/api/loader/recode/start", &json!({ "ticket": "cd".repeat(32) })).expect("sealed");
        // the same letter, carried to another door: its seal is over the door it was written for
        assert!(open("POST", "/api/loader/recode/device/reset", "", &letter).is_none());
        assert!(open("GET", "/api/loader/recode/start", "", &letter).is_none());
        assert!(open("POST", "/api/loader/recode/start", "", &letter).is_some());
    }

    #[test]
    fn a_letter_is_tied_to_the_minute_it_was_written_in() {
        let _held = stand();
        let (mut letter, _) = seal("POST", "/api/client/recode/verify", &json!({ "build": 1 })).expect("sealed");
        letter["t"] = json!(letter["t"].as_i64().unwrap_or(0) + 1);
        assert!(open("POST", "/api/client/recode/verify", "", &letter).is_none(), "a letter rewritten for another minute does not open");
    }

    #[test]
    fn a_letter_of_another_mark_does_not_open_here() {
        let _held = stand();
        let (mut letter, _) = seal("POST", "/api/loader/link", &json!({ "device": {} })).expect("sealed");
        letter["k"] = json!("00".repeat(32));
        assert!(open("POST", "/api/loader/link", "", &letter).is_none(), "the key is the word and the mark together");
    }

    #[test]
    fn words_that_were_not_sealed_by_us_are_not_believed() {
        let _held = stand();
        // a plain answer of a refusal or of something in between: not one of ours to open
        assert!(open("POST", "/api/loader/link", "", &json!({ "error": "signed-out" })).is_none());
        assert!(open("POST", "/api/loader/link", "", &json!({ "v": 1 })).is_none(), "the way alone is not a letter");
        // a letter whose words were touched: the tag does not hold
        let (mut letter, _) = seal("POST", "/api/loader/link", &json!({ "code": "RCD-1111" })).expect("sealed");
        let mut c = unhex(letter["c"].as_str().unwrap()).expect("hex");
        c[0] ^= 1;
        letter["c"] = json!(hex(&c));
        assert!(open("POST", "/api/loader/link", "", &letter).is_none(), "the tag covers the words");
    }

    #[test]
    fn two_letters_never_share_a_nonce() {
        let _held = stand();
        let (_, a) = seal("POST", "/api/loader/link", &json!({})).expect("sealed");
        let (_, b) = seal("POST", "/api/loader/link", &json!({})).expect("sealed");
        assert_ne!(a, b, "a letter is never the one before");
    }

    #[test]
    fn without_a_word_this_one_does_not_seal() {
        let _held = stand();
        std::env::remove_var("CURSED_SEAL");
        assert!(!sealing(), "a build with no word of its own speaks as it always did");
        assert!(seal("POST", "/api/loader/link", &json!({})).is_none());
    }

    #[test]
    fn a_word_that_is_not_sixty_four_hex_does_not_seal() {
        let _held = stand();
        std::env::set_var(crate::vault::env_seal(), "nope");
        assert!(!sealing());
        assert!(seal("POST", "/api/loader/link", &json!({})).is_none());
    }
}
