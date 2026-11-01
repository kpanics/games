//! A short-lived list of clients in this exact game context. Network replies from an
//! old server/account must never put its badges onto players in the next connection.
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Client {
    Cursed,
    Recode,
}
impl Client {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "cursed" => Some(Self::Cursed),
            "recode" => Some(Self::Recode),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Scope {
    pub server: Option<String>,
    pub nick: String,
    generation: u64,
}

#[derive(Default)]
struct Cache {
    scope: Scope,
    players: HashMap<String, Client>,
    at: Option<Instant>,
}

pub fn server_key(raw: &str) -> Option<String> {
    let s = raw.trim().to_ascii_lowercase();
    if s.is_empty()
        || s == "singleplayer"
        || s.len() > 80
        || !s
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b".-:_[]".contains(&c))
    {
        return None;
    }
    let s = if s.matches(':').count() == 1 || s.starts_with('[') {
        s.strip_suffix(":25565").unwrap_or(&s)
    } else {
        &s
    };
    let s = s.trim_end_matches('.');
    (!s.is_empty()).then(|| s.to_string())
}

impl Cache {
    fn context(&mut self, server: &str, nick: &str) {
        let server = server_key(server);
        if self.scope.server != server || self.scope.nick != nick {
            self.clear();
            self.scope.server = server;
            self.scope.nick = nick.into();
        }
    }
    fn clear(&mut self) {
        self.scope.generation = self.scope.generation.wrapping_add(1);
        self.players.clear();
        self.at = None;
    }
    fn replace(&mut self, scope: &Scope, list: &[(String, Client)], at: Instant) {
        if self.scope != *scope {
            return;
        }
        self.players.clear();
        for (nick, client) in list.iter().take(10000) {
            if nick.is_empty()
                || nick.len() > 16
                || !nick.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
            {
                continue;
            }
            self.players
                .entry(nick.to_ascii_lowercase())
                .and_modify(|old| {
                    // If both clients publish the same nick, resolve it consistently in either client.
                    if *client == Client::Recode {
                        *old = *client;
                    }
                })
                .or_insert(*client);
        }
        self.at = Some(at);
    }
    fn get(&self, nick: &str, at: Instant) -> Option<Client> {
        if self
            .at
            .is_none_or(|old| at.saturating_duration_since(old) >= Duration::from_secs(35))
        {
            return None;
        }
        self.players.get(&nick.to_ascii_lowercase()).copied()
    }
}

fn cache() -> &'static Mutex<Cache> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(Cache::default()))
}
pub fn context(server: &str, nick: &str) {
    cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .context(server, nick);
}
pub fn scope() -> Scope {
    cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .scope
        .clone()
}
pub fn clear() {
    cache().lock().unwrap_or_else(|e| e.into_inner()).clear();
}
pub fn replace(scope: &Scope, list: &[(String, Client)]) {
    cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .replace(scope, list, Instant::now());
}
pub fn client_of(nick: &str) -> Option<Client> {
    cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(nick, Instant::now())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn badges_expire_and_old_server_account_or_nick_replies_are_ignored() {
        let mut c = Cache::default();
        let now = Instant::now();
        c.context("MC.EXAMPLE.ORG:25565", "Me");
        let a = c.scope.clone();
        c.replace(&a, &[("Alex".into(), Client::Recode)], now);
        assert_eq!(c.get("ALEX", now), Some(Client::Recode));
        assert_eq!(c.get("Alex", now + Duration::from_secs(35)), None);
        c.context("other.org", "Me");
        c.replace(&a, &[("Alex".into(), Client::Recode)], now);
        assert_eq!(c.get("Alex", now), None);
        c.context("mc.example.org", "Me");
        c.replace(&a, &[("Alex".into(), Client::Recode)], now);
        assert_eq!(c.get("Alex", now), None); // A -> B -> A is still a different connection.
        let b = c.scope.clone();
        c.clear(); // account changed, same server
        c.replace(&b, &[("Alex".into(), Client::Cursed)], now);
        assert_eq!(c.get("Alex", now), None);
        let b = c.scope.clone();
        c.context("mc.example.org", "OtherMe");
        c.replace(&b, &[("Alex".into(), Client::Cursed)], now);
        assert_eq!(c.get("Alex", now), None);
    }
    #[test]
    fn nickname_validation_and_duplicate_clients_are_deterministic() {
        let mut c = Cache::default();
        let now = Instant::now();
        let s = c.scope.clone();
        c.replace(
            &s,
            &[
                ("ALEX".into(), Client::Recode),
                ("Alex".into(), Client::Cursed),
                ("bad nick".into(), Client::Cursed),
            ],
            now,
        );
        assert_eq!(c.players.len(), 1);
        assert_eq!(c.get("alex", now), Some(Client::Recode));
        assert_eq!(
            server_key(" MC.Example.Org.:25565 ").as_deref(),
            Some("mc.example.org")
        );
        assert_eq!(server_key("[::1]:25565").as_deref(), Some("[::1]"));
        assert_eq!(
            server_key("2001:db8::25565").as_deref(),
            Some("2001:db8::25565")
        );
        assert_eq!(server_key("host:25566").as_deref(), Some("host:25566"));
        assert!(server_key("Singleplayer").is_none());
    }
}
