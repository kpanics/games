//! Saved Kill Message lists: old settings remain the default; server profiles are independent copies.
use super::{data::Setting, play::KILL_LISTS, theme};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SETTING: &str = "Message profiles";

/// Substitute once: an item's custom name containing braces must stay literal.
pub fn render(template: &str, values: [&str; 7]) -> String {
    const KEYS: [&[&str]; 7] = [
        &["name", "ник", "нік", "лақап ат"],
        &["hp", "хп"],
        &["weapon", "оружие", "зброя", "қару"],
        &["distance", "дистанция", "відстань", "қашықтық"],
        &["streak", "серия", "серія"],
        &["me", "я", "мен"],
        &["server", "сервер"],
    ];
    let mut rest = template;
    let mut out = String::new();
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let Some(close) = rest[open..].find('}').map(|n| open + n) else {
            out.push_str(&rest[open..]);
            return out;
        };
        let key = &rest[open + 1..close];
        if let Some(i) = KEYS.iter().position(|keys| keys.contains(&key)) {
            out.push_str(values[i]);
        } else {
            out.push_str(&rest[open..=close]);
        }
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    out
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub lists: [Vec<String>; 5],
    pub streaks: BTreeMap<u32, Vec<String>>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Book {
    pub streaks: BTreeMap<u32, Vec<String>>,
    pub servers: BTreeMap<String, Profile>,
}

impl Book {
    pub fn read(set: &[Setting]) -> Self {
        serde_json::from_str(theme::text(set, SETTING)).unwrap_or_default()
    }
    pub fn defaults(&self, set: &[Setting]) -> Profile {
        Profile {
            lists: std::array::from_fn(|i| {
                theme::text(set, KILL_LISTS[i].1)
                    .split('\u{1e}')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect()
            }),
            streaks: self.streaks.clone(),
        }
    }
    pub fn profile(&self, set: &[Setting], server: &str) -> Profile {
        crate::client_marks::server_key(server)
            .and_then(|s| self.servers.get(&s))
            .cloned()
            .unwrap_or_else(|| self.defaults(set))
    }
}

impl Profile {
    pub fn lines(&self, kind: usize, streak: Option<u32>) -> &[String] {
        if kind == 1 {
            if let Some(lines) = streak.and_then(|n| self.streaks.get(&n)) {
                return lines;
            }
        }
        self.lists.get(kind).map(Vec::as_slice).unwrap_or(&[])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn localized_tokens_work_without_expanding_values_again() {
        let values = ["Steve", "2", "Sword {server}", "42", "3", "Lunuca", "one.test"];
        assert_eq!(
            render("{ник} {хп} {оружие} {дистанция} {серия} {сервер}", values),
            "Steve 2 Sword {server} 42 3 one.test"
        );
        assert_eq!(render("{нік} {зброя} {відстань} {серія}", values), "Steve Sword {server} 42 3");
        assert_eq!(render("{лақап ат} {қару} {қашықтық} {мен}", values), "Steve Sword {server} 42 Lunuca");
        assert_eq!(render("{name} {unknown} {", values), "Steve {unknown} {");
    }
    #[test]
    fn server_lists_and_exact_streaks_survive_serialization() {
        let mut book = Book::default();
        let set = Vec::new();
        book.streaks.insert(3, vec!["default three".into()]);
        let mut server = book.defaults(&set);
        server.lists[0] = vec!["server kill".into()];
        server.streaks.insert(3, vec!["server three".into()]);
        book.servers.insert("one.test".into(), server);
        let book: Book = serde_json::from_str(&serde_json::to_string(&book).unwrap()).unwrap();
        assert_eq!(book.profile(&set, "ONE.TEST:25565").lines(1, Some(3)), ["server three"]);
        assert_eq!(book.profile(&set, "two.test").lines(1, Some(3)), ["default three"]);
        assert!(book.profile(&set, "two.test").lines(1, Some(4)).is_empty());
        assert_eq!(book.profile(&set, "one.test").lines(0, None), ["server kill"]);
    }
}
