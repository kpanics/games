//! Game metadata belongs to an authenticated session, not to the account's last POST.
//! An unopened session is only a short-lived pending POST; it never grants an online mark.
use super::{Presence, MOST_STREAMS};
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Default)]
struct Game {
    streams: usize,
    order: u64,
    at: i64,
    presence: Presence,
}

#[derive(Default)]
pub(super) struct Games {
    members: HashMap<i64, HashMap<Vec<u8>, Game>>,
    order: u64,
}

impl Games {
    pub fn prune(&mut self, at: i64) {
        self.members.retain(|_, sessions| {
            sessions.retain(|_, g| g.streams > 0 || at.saturating_sub(g.at) < 120);
            !sessions.is_empty()
        });
    }

    fn entry(&mut self, uid: i64, session: &[u8], at: i64) -> &mut Game {
        self.order = self.order.wrapping_add(1);
        let sessions = self.members.entry(uid).or_default();
        sessions.retain(|_, g| g.streams > 0 || at.saturating_sub(g.at) < 120);
        if !sessions.contains_key(session)
            && sessions.values().filter(|g| g.streams == 0).count() >= MOST_STREAMS
        {
            if let Some(old) = sessions
                .iter()
                .filter(|(_, g)| g.streams == 0)
                .min_by_key(|(_, g)| g.order)
                .map(|(key, _)| key.clone())
            {
                sessions.remove(&old);
            }
        }
        let g = sessions.entry(session.to_vec()).or_default();
        g.at = at;
        g.order = self.order;
        g
    }

    pub fn open(&mut self, uid: i64, session: &[u8], at: i64) {
        self.entry(uid, session, at).streams += 1;
    }

    pub fn update(&mut self, uid: i64, session: &[u8], at: i64, p: Presence) {
        self.entry(uid, session, at).presence = p;
    }

    pub fn close(&mut self, uid: i64, session: &[u8]) {
        if let Some(sessions) = self.members.get_mut(&uid) {
            if let Some(g) = sessions.get_mut(session) {
                g.streams = g.streams.saturating_sub(1);
                if g.streams == 0 {
                    sessions.remove(session);
                }
            }
            if sessions.is_empty() {
                self.members.remove(&uid);
            }
        }
    }

    /// Friends/staff still see one account, using its most recently updated active game.
    pub fn latest(&self, uid: i64) -> Option<&Presence> {
        self.members
            .get(&uid)?
            .values()
            .filter(|g| g.streams > 0)
            .max_by_key(|g| g.order)
            .map(|g| &g.presence)
    }

    pub fn ours(&self, server: &str) -> Vec<Value> {
        let mut rows = std::collections::BTreeMap::new();
        for g in self
            .members
            .values()
            .flat_map(|s| s.values())
            .filter(|g| g.streams > 0 && g.presence.server.as_deref() == Some(server))
        {
            if let Some(mc) = &g.presence.mc {
                let client = g.presence.client.as_deref().unwrap_or("cursed");
                rows.insert(
                    (mc.to_ascii_lowercase(), client.to_string()),
                    json!({"mc":mc,"client":client}),
                );
            }
        }
        rows.into_values().collect()
    }
}

pub(super) fn server_key(raw: &str) -> Option<String> {
    let s = raw.trim().to_ascii_lowercase();
    if s.is_empty()
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

pub(super) fn reflect(p: &mut Presence, game: Option<&Presence>) -> bool {
    let empty = Presence::default();
    let game = game.unwrap_or(&empty);
    let changed = p.listening != game.listening
        || p.server != game.server
        || p.mc != game.mc
        || p.client != game.client;
    p.listening.clone_from(&game.listening);
    p.server.clone_from(&game.server);
    p.mc.clone_from(&game.mc);
    p.client.clone_from(&game.client);
    p.build = game.build;
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    fn p(nick: &str, client: &str, server: &str) -> Presence {
        Presence {
            mc: Some(nick.into()),
            client: Some(client.into()),
            server: server_key(server),
            ..Presence::default()
        }
    }

    #[test]
    fn two_clients_one_account_stay_independent() {
        let mut g = Games::default();
        g.update(
            7,
            b"session-a",
            10,
            p("Alex", "cursed", "MC.Example.org:25565"),
        );
        assert!(g.ours("mc.example.org").is_empty());
        g.open(7, b"session-a", 11);
        g.open(7, b"session-b", 12);
        g.update(7, b"session-b", 13, p("Steve", "recode", "mc.example.org"));
        assert_eq!(g.ours("mc.example.org").len(), 2);
        assert_eq!(g.latest(7).unwrap().mc.as_deref(), Some("Steve"));
        // A website or pending game POST cannot take over an active game's metadata.
        g.update(7, b"site", 14, p("SiteOnly", "recode", "mc.example.org"));
        assert_eq!(g.ours("mc.example.org").len(), 2);
        assert_eq!(g.latest(7).unwrap().mc.as_deref(), Some("Steve"));
        g.update(
            7,
            b"session-b",
            15,
            p("Steve", "recode", "other.example.org:25566"),
        );
        assert_eq!(g.ours("mc.example.org").len(), 1);
        assert_eq!(g.ours("other.example.org:25566").len(), 1);
        g.close(7, b"session-b");
        assert_eq!(g.latest(7).unwrap().mc.as_deref(), Some("Alex"));
        assert!(g.ours("other.example.org:25566").is_empty());
        g.close(7, b"session-a");
        assert!(g.latest(7).is_none());
        assert!(g.ours("mc.example.org").is_empty());
    }

    #[test]
    fn reconnect_overlap_counts_streams_but_not_players_or_hours() {
        let mut g = Games::default();
        g.open(0, b"one", 1);
        g.update(0, b"one", 2, p("Alex", "recode", "host"));
        g.open(0, b"one", 3);
        assert_eq!(g.ours("host").len(), 1);
        g.close(0, b"one");
        assert_eq!(g.ours("host").len(), 1);
        let mut aggregate = Presence {
            game: 1,
            game_counted: 42,
            since: 40,
            ..Presence::default()
        };
        reflect(&mut aggregate, g.latest(0));
        assert_eq!(
            (aggregate.game, aggregate.game_counted, aggregate.since),
            (1, 42, 40)
        );
        g.close(0, b"one");
        reflect(&mut aggregate, g.latest(0));
        assert!(aggregate.server.is_none() && aggregate.mc.is_none() && aggregate.client.is_none());
        assert!(g.ours("host").is_empty());
    }

    #[test]
    fn pending_posts_are_bounded_and_expire_without_evicting_active_games() {
        let mut g = Games::default();
        g.open(1, b"active", 1);
        g.update(1, b"active", 2, p("Active", "cursed", "host"));
        for n in 0..30 {
            g.update(
                1,
                format!("pending-{n}").as_bytes(),
                10 + n,
                p("Pending", "recode", "host"),
            );
        }
        assert_eq!(g.members[&1].len(), MOST_STREAMS + 1);
        g.prune(200);
        assert_eq!(g.members[&1].len(), 1);
        assert_eq!(g.ours("host").len(), 1);
        g.close(1, b"active");
        assert!(g.members.is_empty());
        g.update(2, b"expired", 1, p("Old", "recode", "host"));
        g.open(2, b"expired", 200);
        assert!(g.ours("host").is_empty());
    }

    #[test]
    fn server_addresses_preserve_nondefault_ports_and_ipv6() {
        assert_eq!(
            server_key(" MC.Example.Org.:25565 ").as_deref(),
            Some("mc.example.org")
        );
        assert_eq!(server_key("host:25566").as_deref(), Some("host:25566"));
        assert_eq!(server_key("[::1]:25565").as_deref(), Some("[::1]"));
        assert_eq!(
            server_key("2001:db8::25565").as_deref(),
            Some("2001:db8::25565")
        );
        assert!(server_key("host?token=foo").is_none());
    }
}
