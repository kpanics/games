//! What Streamer Mode hides in every word the client draws (streamer.rs makes the list; Canvas's text and the fonts'
//! widths read it, and Java's text through StringDecomposer the same list): a word put for another, or a server's
//! address — the whole host it is the end of ("mc.funtime.su", "play.funtime.su") — put as dots.

use std::borrow::Cow;
use std::sync::RwLock;

#[derive(Clone, Debug, PartialEq)]
pub enum Mask {
    /// these letters, as they are, put as those
    Word(String, String),
    /// a host ending in this (its letters before it, as a host's are, with it)
    Host(String, String),
}

static MASKS: RwLock<Vec<Mask>> = RwLock::new(Vec::new());
/// How many times the list changed (what measured words with it measures them again: the chat's rows).
static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn generation() -> u64 {
    GENERATION.load(std::sync::atomic::Ordering::Relaxed)
}

/// The tests that set the list, one at a time (it is the whole client's).
#[cfg(test)]
pub static TESTS: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn set(v: Vec<Mask>) {
    if let Ok(mut m) = MASKS.write() {
        if *m != v {
            *m = v;
            GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

fn host_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '.' || c == '-'
}

/// `s` with what is hidden put in its place (itself, when nothing is).
pub fn masked(s: &str) -> Cow<'_, str> {
    let Ok(m) = MASKS.read() else { return Cow::Borrowed(s) };
    if m.is_empty() || s.is_empty() {
        return Cow::Borrowed(s);
    }
    let mut out: Cow<str> = Cow::Borrowed(s);
    for k in m.iter() {
        match k {
            Mask::Word(from, to) => {
                if !from.is_empty() && out.contains(from.as_str()) {
                    out = Cow::Owned(out.replace(from.as_str(), to));
                }
            }
            Mask::Host(end, to) => {
                if end.is_empty() {
                    continue;
                }
                let low = out.to_lowercase();
                if !low.contains(end.as_str()) || low.len() != out.len() {
                    continue;
                }
                // each host ending there, from its first letter to its end (a port after it too)
                let mut res = String::with_capacity(out.len());
                let mut i = 0;
                let src = out.to_string();
                while let Some(at) = low[i..].find(end.as_str()).map(|p| p + i) {
                    let mut a = at;
                    while a > i && src[..a].chars().next_back().is_some_and(host_char) {
                        a -= src[..a].chars().next_back().map_or(1, char::len_utf8);
                    }
                    let mut b = at + end.len();
                    let rest = &src[b..];
                    if let Some(port) = rest.strip_prefix(':') {
                        let n = port.chars().take_while(char::is_ascii_digit).count();
                        if n > 0 {
                            b += 1 + n;
                        }
                    }
                    res.push_str(&src[i..a]);
                    res.push_str(to);
                    i = b;
                }
                res.push_str(&src[i..]);
                out = Cow::Owned(res);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_and_hosts_are_put_aside() {
        let _one = TESTS.lock().unwrap_or_else(|e| e.into_inner());
        set(vec![Mask::Word("Lunuca".into(), "Streamer".into()), Mask::Host("funtime.su".into(), "•••".into())]);
        assert_eq!(masked("Lunuca killed Steve"), "Streamer killed Steve");
        assert_eq!(masked("Join play.FunTime.su:25565 now"), "Join ••• now");
        assert_eq!(masked("mc.funtime.su"), "•••");
        assert_eq!(masked("nothing here"), "nothing here");
        set(Vec::new());
        assert_eq!(masked("Lunuca"), "Lunuca");
    }
}
