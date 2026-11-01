//! The menu's search: modules found by their names, their settings' names and options and their descriptions — as
//! shown (in the game's language) and in English, the query typed in the other keyboard layout too ("ybr" is "ник").
//! A module's own name comes first, then a setting's, then the words that tell of it; a setting found is named on the
//! module's tile and lit for a moment when the module opens.

use super::data::Kind;
use super::i18n::{self, tr};
use super::Gui;

/// A module found: by its name (rank 0), a setting's name or option (1, which setting), its description (2).
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) struct Found {
    pub i: usize,
    pub rank: u8,
    pub set: Option<usize>,
}

/// What was found for a query in a language (the search is asked each frame; it is worked out once per query).
pub(super) type Memo = std::cell::RefCell<Option<(String, i18n::Lang, Vec<Found>)>>;

/// The keys of English's QWERTY and Russian's ЙЦУКЕН, in the same places (and Ukrainian's own letters, theirs).
const EN: &str = "qwertyuiop[]asdfghjkl;'zxcvbnm,.`";
const RU: &str = "йцукенгшщзхъфывапролджэячсмитьбюё";
const UK: [(char, char); 3] = [('і', 's'), ('ї', ']'), ('є', '\'')];

/// The query as typed in the other layout, key by key.
pub(super) fn other_layout(q: &str) -> String {
    q.chars()
        .map(|c| {
            if let Some(p) = EN.chars().position(|e| e == c) {
                RU.chars().nth(p).unwrap_or(c)
            } else if let Some(p) = RU.chars().position(|r| r == c) {
                EN.chars().nth(p).unwrap_or(c)
            } else {
                UK.iter().find(|u| u.0 == c).map_or(c, |u| u.1)
            }
        })
        .collect()
}

/// Whether any of the queries (in lower case) is in `text`.
fn has(text: &str, qs: &[String]) -> bool {
    let t = text.to_lowercase();
    qs.iter().any(|q| t.contains(q.as_str()))
}

impl Gui {
    /// Every module the query finds, the best first (the menu's own order within a rank); none hidden here left out.
    pub(super) fn found(&self) -> Vec<Found> {
        let q = self.q.trim().to_lowercase();
        let lang = i18n::lang();
        if let Some((mq, ml, v)) = self.found_memo.borrow().as_ref() {
            if *mq == q && *ml == lang {
                return v.clone();
            }
        }
        let other = other_layout(&q);
        let qs = if other != q { vec![q.clone(), other] } else { vec![q.clone()] };
        let words = |s: &str| has(s, &qs) || has(tr(s), &qs);
        let mut out = Vec::new();
        for (i, m) in self.mods.iter().enumerate() {
            let opts = |k: &Kind| match k {
                Kind::Choice { opts, .. } | Kind::Multi { opts, .. } => opts.iter().any(|o| words(o)),
                _ => false,
            };
            let f = if words(m.name) || has(i18n::nm(m.name), &qs) {
                Some((0, None))
            } else if let Some(si) = m.set.iter().position(|s| words(s.label) || opts(&s.kind)) {
                Some((1, Some(si)))
            } else if words(m.desc) {
                Some((2, None))
            } else {
                None
            };
            if let Some((rank, set)) = f {
                out.push(Found { i, rank, set });
            }
        }
        out.sort_by_key(|f| f.rank);
        *self.found_memo.borrow_mut() = Some((q, lang, out.clone()));
        out
    }

    /// How the query found a module (when it did).
    pub(super) fn found_of(&self, i: usize) -> Option<Found> {
        if self.q.is_empty() {
            return None;
        }
        self.found().into_iter().find(|f| f.i == i)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_other_layout_is_key_by_key() {
        assert_eq!(other_layout("ybr"), "ник");
        assert_eq!(other_layout("ник"), "ybr");
        assert_eq!(other_layout("auto 3"), "фгещ 3");
        assert_eq!(other_layout("ії"), "s]");
    }

    #[test]
    fn a_module_is_found_by_its_name_first_then_by_a_setting() {
        let mut g = Gui::new();
        g.q = "auto sprint".into();
        let f = g.found();
        let i = g.mods.iter().position(|m| m.id == "auto-sprint").unwrap();
        assert_eq!(f.first().map(|f| (f.i, f.rank)), Some((i, 0)));
        // a setting's name: the module, that setting named
        let (mi, si) = g.mods.iter().enumerate().find_map(|(mi, m)| m.set.iter().position(|s| s.label == "Copies your armour").map(|si| (mi, si))).unwrap();
        g.q = "copies your".into();
        assert!(g.found().iter().any(|f| f.i == mi && f.rank == 1 && f.set == Some(si)));
        assert_eq!(g.found_of(mi).and_then(|f| f.set), Some(si));
    }
}
