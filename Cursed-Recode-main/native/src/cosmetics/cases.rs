//! CS2's weapon cases for the Cosmetics page (tools/cs2pack/cases.py: the store's cases.txt): each case, the skins it
//! holds by rarity and the rare special items (knives, gloves) it holds; an opening's roll as CS2's odds have it
//! (Mil-Spec 79.92 %, Restricted 15.98 %, Classified 3.2 %, Covert 0.64 %, a rare special item 0.26 %), its wear and
//! its pattern at random, StatTrak™ one time in ten (gloves never); what was won, kept beside the loadout
//! (case_history.txt), the newest last.

use super::store;
use std::sync::{Arc, Mutex, OnceLock};

/// The rarities, rarest last: their names as cases.txt has them (the rare special item's our own), CS2's odds.
pub const RARITIES: [&str; 5] = ["rare", "mythical", "legendary", "ancient", "special"];
pub const ODDS: [f64; 5] = [0.7992, 0.1598, 0.032, 0.0064, 0.0026];
/// A rare special item's colour (CS2's ★ gold).
pub const SPECIAL: u8 = 4;

#[derive(Clone, Debug, PartialEq)]
pub struct Drop {
    pub weapon: String,
    /// Its finish (empty: a knife's vanilla).
    pub paint: String,
    pub rarity: u8,
    pub wear: (f32, f32),
    pub ru: String,
    pub en: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Case {
    pub name: String,
    pub ru: String,
    pub en: String,
    /// Its skins, then its rare special items (their names the catalog's).
    pub drops: Vec<Drop>,
}

impl Case {
    pub fn of(&self, rarity: u8) -> impl Iterator<Item = &Drop> {
        self.drops.iter().filter(move |d| d.rarity == rarity)
    }
}

#[derive(Debug, Default)]
pub struct Cases {
    pub cases: Vec<Case>,
}

impl Cases {
    pub fn parse(text: &str, cat: &super::catalog::Catalog) -> Cases {
        let mut cases: Vec<Case> = Vec::new();
        for line in text.lines() {
            let f: Vec<&str> = line.split('\t').collect();
            match f.as_slice() {
                ["case", name, _series, ru, en, ..] => cases.push(Case { name: name.to_string(), ru: ru.to_string(), en: en.to_string(), drops: Vec::new() }),
                ["drop", case, rarity, weapon, paint, lo, hi, ru, en, ..] => {
                    let Some(r) = RARITIES.iter().position(|x| x == rarity) else { continue };
                    if let Some(c) = cases.iter_mut().find(|c| c.name == *case) {
                        let wear = (lo.parse().unwrap_or(0.0), hi.parse().unwrap_or(1.0));
                        c.drops.push(Drop { weapon: weapon.to_string(), paint: paint.to_string(), rarity: r as u8, wear, ru: ru.to_string(), en: en.to_string() });
                    }
                }
                ["rare", case, weapon, paint, ..] => {
                    let Some(item) = cat.item(weapon) else { continue };
                    let finish = cat.finish(weapon, paint);
                    if !paint.is_empty() && finish.is_none() {
                        continue;
                    }
                    let (ru, en) = match finish {
                        Some(f) => (format!("★ {} | {}", item.ru, f.ru), format!("★ {} | {}", item.en, f.en)),
                        None => (format!("★ {}", item.ru), format!("★ {}", item.en)),
                    };
                    let wear = finish.map_or((0.0, 1.0), |f| f.wear);
                    if let Some(c) = cases.iter_mut().find(|c| c.name == *case) {
                        c.drops.push(Drop { weapon: weapon.to_string(), paint: paint.to_string(), rarity: SPECIAL, wear, ru, en });
                    }
                }
                _ => {}
            }
        }
        cases.retain(|c| !c.drops.is_empty());
        Cases { cases }
    }

    pub fn case(&self, name: &str) -> Option<&Case> {
        self.cases.iter().find(|c| c.name == name)
    }
}

/// The cases (read once the store has them, the catalog read: the rare items' names are its).
pub fn get() -> Arc<Cases> {
    static C: Mutex<Option<Arc<Cases>>> = Mutex::new(None);
    let mut c = C.lock().unwrap();
    if c.as_ref().is_none_or(|x| x.cases.is_empty()) {
        let cat = super::catalog::get();
        if cat.items.is_empty() {
            return Arc::new(Cases::default());
        }
        store::want("cases.txt");
        let text = store::read("cases.txt").map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
        *c = Some(Arc::new(Cases::parse(&text, &cat)));
    }
    c.clone().unwrap()
}

/// What an opening gave: when (unix seconds), from which case, what, how rare, its wear and pattern, StatTrak™.
#[derive(Clone, Debug, PartialEq)]
pub struct Won {
    pub at: u64,
    pub case: String,
    pub weapon: String,
    pub paint: String,
    pub rarity: u8,
    pub wear: f32,
    pub seed: i32,
    pub stattrak: bool,
}

impl Won {
    pub fn dump(&self) -> String {
        format!("{}\t{}\t{}\t{}\t{}\t{:.6}\t{}\t{}", self.at, self.case, self.weapon, self.paint, self.rarity, self.wear, self.seed, self.stattrak as u8)
    }

    pub fn parse(line: &str) -> Option<Won> {
        let f: Vec<&str> = line.split('\t').collect();
        let [at, case, weapon, paint, rarity, wear, seed, st] = f.as_slice() else { return None };
        Some(Won {
            at: at.parse().ok()?,
            case: case.to_string(),
            weapon: weapon.to_string(),
            paint: paint.to_string(),
            rarity: rarity.parse::<u8>().ok()?.min(SPECIAL),
            wear: wear.parse().ok()?,
            seed: seed.parse().ok()?,
            stattrak: *st == "1",
        })
    }

    /// Whether a pair of gloves (never StatTrak™).
    pub fn gloves(weapon: &str) -> bool {
        !weapon.starts_with("weapon_")
    }
}

/// A little generator of our own (splitmix64): a roll's numbers.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed ^ 0x9E37_79B9_7F4A_7C15)
    }

    /// Seeded from the clock.
    pub fn now() -> Rng {
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
        Rng::new(t)
    }

    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// In [0, 1).
    pub fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn below(&mut self, n: usize) -> usize {
        (self.unit() * n as f64) as usize % n.max(1)
    }
}

/// A rarity as CS2's odds give it (a case without any of it: the next commoner one it has).
pub fn rarity(case: &Case, rng: &mut Rng) -> u8 {
    let u = rng.unit();
    let mut acc = 0.0;
    let mut r = 0u8;
    for k in (0..RARITIES.len()).rev() {
        acc += ODDS[k];
        if u < acc {
            r = k as u8;
            break;
        }
    }
    while r > 0 && case.of(r).next().is_none() {
        r -= 1;
    }
    r
}

/// One of a case's things of a rarity, at random.
pub fn pick<'a>(case: &'a Case, rarity: u8, rng: &mut Rng) -> Option<&'a Drop> {
    let all: Vec<&Drop> = case.of(rarity).collect();
    (!all.is_empty()).then(|| all[rng.below(all.len())])
}

/// An opening: what it gives (at `at`, unix seconds).
pub fn roll(case: &Case, rng: &mut Rng, at: u64) -> Option<Won> {
    let r = rarity(case, rng);
    let d = pick(case, r, rng)?;
    let wear = d.wear.0 + (d.wear.1 - d.wear.0) * rng.unit() as f32;
    let seed = rng.below(1001) as i32;
    let stattrak = !Won::gloves(&d.weapon) && rng.unit() < 0.1;
    Some(Won { at, case: case.name.clone(), weapon: d.weapon.clone(), paint: d.paint.clone(), rarity: r, wear, seed, stattrak })
}

/// The roulette's cards (`n` of them, by the same odds, none of them rare special items but the one that may be won):
/// what was won at `win`.
pub fn strip<'a>(case: &'a Case, won: &Won, n: usize, win: usize, rng: &mut Rng) -> Vec<&'a Drop> {
    let mut out = Vec::with_capacity(n);
    for k in 0..n {
        if k == win {
            if let Some(d) = case.drops.iter().find(|d| d.weapon == won.weapon && d.paint == won.paint) {
                out.push(d);
                continue;
            }
        }
        let mut r = rarity(case, rng).min(3);
        while r > 0 && case.of(r).next().is_none() {
            r -= 1;
        }
        if let Some(d) = pick(case, r, rng) {
            out.push(d);
        }
    }
    out
}

fn history_path() -> &'static Mutex<Option<std::path::PathBuf>> {
    static P: OnceLock<Mutex<Option<std::path::PathBuf>>> = OnceLock::new();
    P.get_or_init(|| Mutex::new(None))
}

/// Where what was won is kept (cursed-recode/case_history.txt).
pub fn set_history(path: std::path::PathBuf) {
    *history_path().lock().unwrap() = Some(path);
}

/// What was won, the newest last (the last 500 kept).
pub fn history() -> Vec<Won> {
    let Some(p) = history_path().lock().unwrap().clone() else { return Vec::new() };
    std::fs::read_to_string(p).map(|t| t.lines().filter_map(Won::parse).collect()).unwrap_or_default()
}

/// One more won, kept at once.
pub fn record(w: &Won) {
    let Some(p) = history_path().lock().unwrap().clone() else { return };
    let mut all = history();
    all.push(w.clone());
    let from = all.len().saturating_sub(500);
    let text: String = all[from..].iter().map(|w| w.dump() + "\n").collect();
    let _ = std::fs::write(p, text);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> (Cases, super::super::catalog::Catalog) {
        let cat = super::super::catalog::Catalog::parse(
            "item\tweapon_knife_kukri\t526\tknife\tx.vmdl\tКукри\tKukri Knife\tancient\npaint\tweapon_knife_kukri\taa_fade\tancient\t0\t0.08\t6\t0\t0\tГрадиент\tFade\n",
        );
        let text = "case\tcrate_community_33\t393\tКейс «Киловатт»\tKilowatt Case\n\
            drop\tcrate_community_33\trare\tweapon_elite\tdual_berettas_lethal_grin\t0\t1\tDual Berettas | Смертельная ухмылка\tDual Berettas | Hideout\n\
            drop\tcrate_community_33\tancient\tweapon_awp\tawp_tempered\t0\t0.7\tAWP | Закалка\tAWP | Chromatic Aberration\n\
            rare\tcrate_community_33\tweapon_knife_kukri\taa_fade\n\
            rare\tcrate_community_33\tweapon_knife_kukri\t\n\
            rare\tcrate_community_33\tweapon_knife_missing\taa_fade\n";
        (Cases::parse(text, &cat), cat)
    }

    #[test]
    fn cases_read_with_their_rare_items_named_by_the_catalog() {
        let (c, _) = sample();
        let k = c.case("crate_community_33").unwrap();
        assert_eq!(k.drops.len(), 4);
        assert_eq!(k.of(SPECIAL).count(), 2);
        assert_eq!(k.of(SPECIAL).next().unwrap().en, "★ Kukri Knife | Fade");
        assert_eq!(k.of(3).next().unwrap().wear, (0.0, 0.7));
    }

    #[test]
    fn the_odds_are_cs2s_and_a_win_keeps() {
        let (c, _) = sample();
        let k = c.case("crate_community_33").unwrap();
        let mut rng = Rng::new(7);
        let mut seen = [0usize; 5];
        for _ in 0..200_000 {
            seen[rarity(k, &mut rng) as usize] += 1;
        }
        // (no Restricted or Classified in it: those fall to Mil-Spec)
        let share = |n: usize| n as f64 / 200_000.0;
        assert!((share(seen[0]) - (0.7992 + 0.1598 + 0.032)).abs() < 0.005, "{seen:?}");
        assert!((share(seen[3]) - 0.0064).abs() < 0.001, "{seen:?}");
        assert!((share(seen[4]) - 0.0026).abs() < 0.0006, "{seen:?}");
        let w = roll(k, &mut rng, 1_793_000_000).unwrap();
        assert_eq!(Won::parse(&w.dump()).map(|x| (x.weapon, x.paint, x.seed)), Some((w.weapon.clone(), w.paint.clone(), w.seed)));
        let s = strip(k, &w, 40, 33, &mut rng);
        assert_eq!(s.len(), 40);
        assert!(s[33].weapon == w.weapon && s[33].paint == w.paint);
        assert!(s.iter().enumerate().all(|(i, d)| i == 33 || d.rarity < SPECIAL));
    }
}
