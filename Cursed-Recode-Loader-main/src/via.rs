//! A server's status asked the way the server list asks it, before a connection to it is made: which protocol it
//! speaks (cursed.recode.via.Via: an older one is joined through ViaVersion).

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

/// The client's own protocol (26.3), said in the handshake.
const OWN: i32 = 777;

/// The protocol the server answers its status with; -1 when it does not answer in `timeout_ms`.
pub fn ping(host: &str, port: u16, timeout_ms: u64) -> i32 {
    ask(host, port, timeout_ms).unwrap_or(-1)
}

fn varint(out: &mut Vec<u8>, x: i32) {
    let mut v = x as u32;
    loop {
        if v & !0x7f == 0 {
            out.push(v as u8);
            return;
        }
        out.push((v & 0x7f) as u8 | 0x80);
        v >>= 7;
    }
}

/// A VarInt at `at`: its value and where it ends.
fn read_varint(b: &[u8], at: usize) -> Option<(i32, usize)> {
    let mut v: u32 = 0;
    for i in 0..5 {
        let byte = *b.get(at + i)?;
        v |= ((byte & 0x7f) as u32) << (7 * i);
        if byte & 0x80 == 0 {
            return Some((v as i32, at + i + 1));
        }
    }
    None
}

fn ask(host: &str, port: u16, timeout_ms: u64) -> Option<i32> {
    let t = Duration::from_millis(timeout_ms);
    let addr = (host, port).to_socket_addrs().ok()?.next()?;
    let mut s = TcpStream::connect_timeout(&addr, t).ok()?;
    s.set_read_timeout(Some(t)).ok()?;
    s.set_write_timeout(Some(t)).ok()?;
    // the handshake (id 0: protocol, host, port, 1 for status), then the status request (id 0, nothing else)
    let mut hs = Vec::new();
    varint(&mut hs, 0);
    varint(&mut hs, OWN);
    varint(&mut hs, host.len() as i32);
    hs.extend_from_slice(host.as_bytes());
    hs.extend_from_slice(&port.to_be_bytes());
    varint(&mut hs, 1);
    let mut out = Vec::new();
    varint(&mut out, hs.len() as i32);
    out.extend_from_slice(&hs);
    out.extend_from_slice(&[1, 0]);
    s.write_all(&out).ok()?;
    // the answer: its length, its id 0, its JSON (a string)
    let mut got = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let n = s.read(&mut buf).ok()?;
        if n == 0 {
            return None;
        }
        got.extend_from_slice(&buf[..n]);
        if let Some((len, at)) = read_varint(&got, 0) {
            if got.len() >= at + len.max(0) as usize {
                let (id, at) = read_varint(&got, at)?;
                if id != 0 {
                    return None;
                }
                let (slen, at) = read_varint(&got, at)?;
                let text = std::str::from_utf8(got.get(at..at + slen.max(0) as usize)?).ok()?;
                let json: serde_json::Value = serde_json::from_str(text).ok()?;
                return json.get("version")?.get("protocol")?.as_i64().map(|v| v as i32);
            }
        }
        if got.len() > 1 << 20 {
            return None;
        }
    }
}
