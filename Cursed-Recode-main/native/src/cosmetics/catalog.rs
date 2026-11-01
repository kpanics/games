//! The cosmetics' catalog (tools/cs2pack/catalog.py: CS2's items_game, its localisation, the icons it made): the
//! knives, the gloves, the AWP and the SSG 08 with their names, and every finish each comes in — its names, rarity,
//! wear's range, style, default seed, whether it is painted for the legacy model.

use super::store;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Knife,
    Gloves,
    Rifle,
}

#[derive(Clone, Debug)]
pub struct Item {
    /// CS2's name (weapon_knife_karambit): its folder, its icons'.
    pub name: String,
    pub kind: Kind,
    pub ru: String,
    pub en: String,
    /// Its prefabs' item_rarity (a paint's own rarity adds to it).
    pub rarity: String,
}

#[derive(Clone, Debug)]
pub struct Finish {
    pub item: String,
    pub paint: String,
    pub rarity: String,
    pub wear: (f32, f32),
    pub style: u32,
    pub seed: i32,
    pub legacy: bool,
    pub ru: String,
    pub en: String,
}

/// A charm (a rifle's keychain): its name (its folder, its icon's), its rarity, its names.
#[derive(Clone, Debug)]
pub struct Charm {
    pub name: String,
    pub rarity: String,
    pub ru: String,
    pub en: String,
}

/// A sticker: its kit's name, its rarity, its material (its path under stickers/), its names.
#[derive(Clone, Debug)]
pub struct Sticker {
    pub name: String,
    pub rarity: String,
    pub material: String,
    pub ru: String,
    pub en: String,
}

#[derive(Default, Debug)]
pub struct Catalog {
    pub items: Vec<Item>,
    pub finishes: Vec<Finish>,
    pub charms: Vec<Charm>,
    pub stickers: Vec<Sticker>,
}

impl Catalog {
    pub fn parse(text: &str) -> Catalog {
        let mut c = Catalog::default();
        for line in text.lines() {
            let f: Vec<&str> = line.split('\t').collect();
            match f.first().copied() {
                Some("item") if f.len() >= 7 => c.items.push(Item {
                    name: f[1].into(),
                    kind: match f[3] {
                        "knife" => Kind::Knife,
                        "gloves" => Kind::Gloves,
                        _ => Kind::Rifle,
                    },
                    ru: f[5].into(),
                    en: f[6].into(),
                    rarity: f.get(7).copied().unwrap_or("").into(),
                }),
                Some("charm") if f.len() >= 7 => c.charms.push(Charm { name: f[1].into(), rarity: f[2].into(), ru: f[5].into(), en: f[6].into() }),
                Some("sticker") if f.len() >= 6 => c.stickers.push(Sticker { name: f[1].into(), rarity: f[2].into(), material: f[3].into(), ru: f[4].into(), en: f[5].into() }),
                Some("paint") if f.len() >= 11 => c.finishes.push(Finish {
                    item: f[1].into(),
                    paint: f[2].into(),
                    rarity: f[3].into(),
                    wear: (f[4].parse().unwrap_or(0.0), f[5].parse().unwrap_or(1.0)),
                    style: f[6].parse().unwrap_or(0),
                    seed: f[7].parse().unwrap_or(0),
                    legacy: f[8] == "1",
                    ru: f[9].into(),
                    en: f[10].into(),
                }),
                _ => {}
            }
        }
        // the rifles' names (theirs are their prefabs')
        for i in &mut c.items {
            if i.ru.is_empty() || i.ru.starts_with("SFUI") {
                let n = match i.name.as_str() {
                    "weapon_awp" => "AWP",
                    "weapon_ssg08" => "SSG 08",
                    other => other,
                };
                i.ru = n.into();
                i.en = n.into();
            }
        }
        c
    }

    pub fn item(&self, name: &str) -> Option<&Item> {
        self.items.iter().find(|i| i.name == name)
    }

    pub fn finish(&self, item: &str, paint: &str) -> Option<&Finish> {
        self.finishes.iter().find(|f| f.item == item && f.paint == paint)
    }

    pub fn finishes_of<'a>(&'a self, item: &'a str) -> impl Iterator<Item = &'a Finish> + 'a {
        self.finishes.iter().filter(move |f| f.item == item)
    }
}

/// A finish's name as the game's market shows it, its phase or gem added (Doppler's "Phase 2", "Ruby"…).
pub fn finish_name(f: &Finish, ru: bool) -> String {
    let base = if ru { &f.ru } else { &f.en };
    let p = f.paint.as_str();
    let extra = if let Some(n) = p.strip_prefix("am_doppler_phase").or_else(|| p.strip_prefix("am_gamma_doppler_phase")) {
        Some(format!("{} {}", if ru { "Фаза" } else { "Phase" }, n.trim_start_matches(|c: char| !c.is_ascii_digit()).chars().take(1).collect::<String>()))
    } else if p.contains("ruby") {
        Some(if ru { "Рубин" } else { "Ruby" }.to_string())
    } else if p.contains("sapphire") {
        Some(if ru { "Сапфир" } else { "Sapphire" }.to_string())
    } else if p.contains("blackpearl") {
        Some(if ru { "Чёрная жемчужина" } else { "Black Pearl" }.to_string())
    } else if p.contains("emerald") {
        Some(if ru { "Изумруд" } else { "Emerald" }.to_string())
    } else {
        None
    };
    match extra {
        Some(e) => format!("{base} · {e}"),
        None => base.clone(),
    }
}

/// CS's rarities in their order (items_game "rarities": common 1 … ancient 6).
const RARITIES: [&str; 8] = ["default", "common", "uncommon", "rare", "mythical", "legendary", "ancient", "immortal"];

/// A finish's rarity on its item as the game shows it: the item's and the paint's put together as CS puts them (the
/// item's value and the paint's, less one: the AWP, "uncommon", lifts its paints by one; the SSG 08, "common", not).
pub fn rarity_on(item: &Item, f: &Finish) -> &'static str {
    let at = |r: &str| RARITIES.iter().position(|x| *x == r);
    match (at(&item.rarity), at(&f.rarity)) {
        (Some(i), Some(p)) if i > 0 && p > 0 => RARITIES[(i + p - 1).min(RARITIES.len() - 1)],
        (_, Some(p)) => RARITIES[p],
        _ => "",
    }
}

/// A rarity's name as CS's inventory says it (English: the menu's words translate it); a knife's and gloves' their ★.
pub fn rarity_name(item_kind: Kind, rarity: &str) -> &'static str {
    match item_kind {
        Kind::Knife => "★ Covert",
        Kind::Gloves => "★ Extraordinary",
        Kind::Rifle => match rarity {
            "common" => "Consumer Grade",
            "uncommon" => "Industrial Grade",
            "rare" => "Mil-Spec",
            "mythical" => "Restricted",
            "legendary" => "Classified",
            "ancient" => "Covert",
            "immortal" => "Contraband",
            _ => "Stock",
        },
    }
}

/// CS's rarity colours (the strip under a card), a knife's and gloves' as their ★'s.
pub fn rarity_colour(item_kind: Kind, rarity: &str) -> u32 {
    if item_kind != Kind::Rifle {
        return 0xE4AE39;
    }
    match rarity {
        "common" => 0xB0C3D9,
        "uncommon" => 0x5E98D9,
        "rare" => 0x4B69FF,
        "mythical" => 0x8847FF,
        "legendary" => 0xD32CE6,
        "ancient" => 0xEB4B4B,
        "immortal" | "unusual" => 0xE4AE39,
        _ => 0xB0C3D9,
    }
}

/// The catalog (read once, from the store's catalog.txt).
pub fn get() -> Arc<Catalog> {
    static C: Mutex<Option<Arc<Catalog>>> = Mutex::new(None);
    let mut c = C.lock().unwrap();
    if c.is_none() || c.as_ref().is_some_and(|x| x.items.is_empty()) {
        let text = store::read("catalog.txt").map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
        *c = Some(Arc::new(Catalog::parse(&text)));
    }
    c.clone().unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalog_reads_and_names_its_finishes() {
        let c = Catalog::parse(
            "item\tweapon_knife_karambit\t507\tknife\tweapons/x.vmdl\tКерамбит\tKarambit\tancient\nitem\tweapon_awp\t9\trifle\tw.vmdl\t\t\tuncommon\npaint\tweapon_knife_karambit\tam_doppler_phase2\tmythical\t0\t0.08\t5\t0\t0\tВолны\tDoppler\npaint\tweapon_awp\taa_awp_fade\tlegendary\t0\t0.08\t6\t0\t0\tГрадиент\tFade\n",
        );
        assert_eq!(c.items.len(), 2);
        // the AWP lifts its paints by one: its Fade, a "legendary" paint, is Covert
        let awp = c.item("weapon_awp").unwrap();
        let fade = c.finish("weapon_awp", "aa_awp_fade").unwrap();
        assert_eq!(rarity_on(awp, fade), "ancient");
        assert_eq!(rarity_name(Kind::Rifle, rarity_on(awp, fade)), "Covert");
        assert_eq!(c.item("weapon_awp").unwrap().ru, "AWP");
        let f = c.finish("weapon_knife_karambit", "am_doppler_phase2").unwrap();
        assert_eq!(f.wear, (0.0, 0.08));
        assert_eq!(finish_name(f, true), "Волны · Фаза 2");
        assert_eq!(finish_name(f, false), "Doppler · Phase 2");
        assert_eq!(rarity_colour(Kind::Knife, &f.rarity), 0xE4AE39);
    }
}
