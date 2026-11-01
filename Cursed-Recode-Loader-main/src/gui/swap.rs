//! Auto Swap (the menu's module "auto-swap", cursed.recode.swap.AutoSwap): the player's swaps, each "when → what →
//! where". When: a key pressed (on to the next thing of its list, round), a key held (on while held, back when let
//! go) or a condition met (back when it passes, if asked). What: a list in the order wanted, each a thing picked from
//! the inventory (known by its id, its name and its enchantments) or a kind of thing (any totem, a talisman, the best
//! chestplate…). Where: the off hand, the head, the chest, the legs, the feet. Kept with the menu's settings
//! (auto-swap.#swaps, JSON); Java reads them ("swaps") and does them — the keys are caught here with the binds
//! (world_key) and go to it as events ("swapev").
//!
//! The server's rules (the server is told by Java, "server"): on HolyWorld the module is not there at all; on
//! ReallyWorld only from the hotbar and only on keys; on FunTime only on keys, from the hotbar at once, from the
//! inventory only totems and spheres, after FunTime's pause (her word, 23.10: 4 ticks; since the timing's presets the
//! player's own, swaptime.rs). A totem never goes by itself anywhere (her word, 22.10).

use super::binds::{self, Bind};
use super::swaptime::Timing;
use serde_json::{json, Value};

pub const MODULE: &str = "auto-swap";
/// Where a swap puts its thing; "All armour": each piece whose wear is past the swap's number (%, 0 any) changed for
/// the best spare one (another of the same thing first), a piece at a time.
pub const PLACES: [&str; 6] = ["Off hand", "On the head", "On the chest", "On the legs", "On the feet", "All armour"];
/// What sets it off.
pub const TRIGGERS: [&str; 3] = ["Press", "Hold", "When"];
/// The conditions of "When" (the number each one compares, as the page says it).
pub const CONDS: [&str; 5] = ["Health below", "Durability below", "Ran out", "Falling", "Danger near"];
/// The kinds a list may hold besides things picked (Java's AutoSwap.Kinds knows each by its id).
pub const KINDS: [(&str, &str); 14] = [
    ("totem", "Any totem"),
    ("talisman", "Talisman"),
    ("shield", "Shield"),
    ("sphere", "Any sphere"),
    ("firework", "Firework"),
    ("golden_apple", "Golden apple"),
    ("enchanted_apple", "Enchanted apple"),
    ("food", "Any food"),
    ("elytra", "Elytra"),
    ("helmet", "Best helmet"),
    ("chestplate", "Best chestplate"),
    ("leggings", "Best leggings"),
    ("boots", "Best boots"),
    ("same", "The same as worn"),
];
/// The servers with rules of their own (as Java names them).
pub const HOLYWORLD: &str = "HolyWorld";
pub const REALLYWORLD: &str = "ReallyWorld";
pub const FUNTIME: &str = "FunTime";
pub const ARESMINE: &str = "AresMine";
/// Click Pearl: its keys throw (Java, cursed.recode.swap.ClickPearl, takes them as "pearlev"), its switch is its tile's.
pub const PEARL: &str = "click-pearl";
/// The modules FunTime's rules ban besides (her word, 25.10).
pub const FUNTIME_BANS: [&str; 2] = ["chunk-animator", "no-friend-damage"];
/// Settings FunTime forbids (No Render's): gone there (setting_hidden_here).
pub const FUNTIME_SETTING_BANS: [&str; 2] = ["Blindness fog", "Darkness pulse"];

/// One thing of a swap's list.
#[derive(Clone, PartialEq, Debug)]
pub enum Pick {
    /// A thing as it was picked: its id ("minecraft:player_head"), its name as shown, its enchantments ("id:level,…").
    Item { id: String, name: String, ench: String },
    /// A kind of thing (KINDS' id).
    Kind(String),
}

#[derive(Clone, PartialEq, Debug)]
pub struct Swap {
    pub on: bool,
    /// PLACES
    pub place: usize,
    pub picks: Vec<Pick>,
    /// TRIGGERS
    pub trigger: usize,
    pub bind: Option<Bind>,
    /// CONDS, and the number it compares (hearts, %, blocks)
    pub cond: usize,
    pub value: f32,
    /// What was there before comes back: when the key is let go (Hold), when the condition passes (When).
    pub back: bool,
    /// Its own timing (swaptime.rs); None: the module's.
    pub timing: Option<Timing>,
}

impl Swap {
    pub fn new() -> Swap {
        Swap { on: true, place: 0, picks: Vec::new(), trigger: 0, bind: None, cond: 0, value: 0.0, back: true, timing: None }
    }
}

fn pick_json(p: &Pick) -> Value {
    match p {
        Pick::Item { id, name, ench } => json!({"id": id, "name": name, "ench": ench}),
        Pick::Kind(k) => json!({"kind": k}),
    }
}

fn pick_from(v: &Value) -> Option<Pick> {
    if let Some(k) = v.get("kind").and_then(Value::as_str) {
        return KINDS.iter().any(|q| q.0 == k).then(|| Pick::Kind(k.to_string()));
    }
    let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let id = s("id");
    (!id.is_empty()).then(|| Pick::Item { id, name: s("name"), ench: s("ench") })
}

fn swap_json(s: &Swap) -> Value {
    let mut v = json!({
        "on": s.on,
        "place": s.place,
        "picks": s.picks.iter().map(pick_json).collect::<Vec<_>>(),
        "trigger": s.trigger,
        "keys": s.bind.as_ref().map_or(String::new(), |b| binds::save(std::slice::from_ref(b))),
        "cond": s.cond,
        "value": s.value,
        "back": s.back,
    });
    if let Some(t) = &s.timing {
        v["timing"] = t.to_json();
    }
    v
}

fn swap_from(v: &Value) -> Swap {
    let n = |k: &str, max: usize| (v.get(k).and_then(Value::as_u64).unwrap_or(0) as usize).min(max);
    let mut s = Swap::new();
    s.on = v.get("on").and_then(Value::as_bool).unwrap_or(s.on);
    s.place = n("place", PLACES.len() - 1);
    s.picks = v.get("picks").and_then(Value::as_array).map_or(Vec::new(), |a| a.iter().filter_map(pick_from).collect());
    s.trigger = n("trigger", TRIGGERS.len() - 1);
    s.bind = v.get("keys").and_then(Value::as_str).and_then(|k| binds::load(k).into_iter().next());
    s.cond = n("cond", CONDS.len() - 1);
    s.value = v.get("value").and_then(Value::as_f64).unwrap_or(0.0) as f32;
    s.back = v.get("back").and_then(Value::as_bool).unwrap_or(s.back);
    s.timing = v.get("timing").filter(|t| t.is_object()).map(Timing::from_json);
    s
}

/// The swaps as kept (one line of JSON).
pub fn save(list: &[Swap]) -> String {
    Value::Array(list.iter().map(swap_json).collect()).to_string()
}

pub fn load(s: &str) -> Vec<Swap> {
    serde_json::from_str::<Value>(s).ok().and_then(|v| v.as_array().map(|a| a.iter().map(swap_from).collect())).unwrap_or_default()
}

/// Whether a swap's thing could go by itself on this server: no totem anywhere, nothing on FunTime and ReallyWorld.
pub fn may_go_by_itself(s: &Swap, server: &str) -> bool {
    if server == FUNTIME || server == REALLYWORLD || server == HOLYWORLD {
        return false;
    }
    // "All armour" takes armour only (its list, kept for its going back to a place, is not used)
    if s.place == 5 {
        return true;
    }
    !s.picks.iter().any(|p| match p {
        Pick::Kind(k) => k == "totem" || k == "talisman",
        Pick::Item { id, .. } => id == "minecraft:totem_of_undying",
    })
}

impl super::Gui {
    /// Whether a module is not there on the server played on (its rules ban it, as the free Cursed's restrict.rs has
    /// them): Auto Swap on HolyWorld and AresMine, Click Pearl on FunTime, AresMine and HolyWorld, Chunk Animator and No
    /// Friend Damage on FunTime. Not there, it is off (Java asks "set": 0).
    pub(super) fn hidden_here(&self, id: &str) -> bool {
        let s = self.server.as_str();
        (id == MODULE && (s == HOLYWORLD || s == ARESMINE))
            || (id == PEARL && (s == FUNTIME || s == ARESMINE || s == HOLYWORLD))
            || (s == FUNTIME && FUNTIME_BANS.contains(&id))
            // Item Cooldowns: FunTime's rules forbid a cooldowns HUD, HolyWorld's as well (her word, 25.10)
            || (id == "item-cooldowns" && (s == FUNTIME || s == HOLYWORLD))
    }

    /// A setting not there on this server: gone from its list in the menu, off to the game. FunTime's rules (24.10.2026)
    /// forbid taking blindness and darkness away (her word, 25.10: "на фт сделай что бы там запрещенное из no render
    /// пропадало").
    pub(super) fn setting_hidden_here(&self, label: &str) -> bool {
        self.server.as_str() == FUNTIME && FUNTIME_SETTING_BANS.contains(&label)
    }

    fn swap_module_on(&self) -> bool {
        !self.hidden_here(MODULE) && self.mods.iter().any(|m| m.id == MODULE && m.on)
    }

    /// A key down in the world: the swaps whose keys these are, as events for Java (a held one also when let go,
    /// swap_keys_up).
    pub(super) fn swap_keys_down(&mut self, name: &str, held: &[String]) -> bool {
        if !self.swap_module_on() {
            return false;
        }
        let mut fired = false;
        for i in 0..self.swaps.len() {
            let s = &self.swaps[i];
            if !s.on || s.trigger > 1 || !s.bind.as_ref().is_some_and(|b| b.fires(name, held)) {
                continue;
            }
            if s.trigger == 1 {
                if self.swap_held.contains(&i) {
                    continue;
                }
                self.swap_held.push(i);
                self.swap_ev.push(format!("hold {i}"));
            } else {
                self.swap_ev.push(format!("press {i}"));
            }
            fired = true;
        }
        fired
    }

    /// Keys let go: the held swaps whose keys are not all down any more go back.
    pub(super) fn swap_keys_up(&mut self, held: &[String]) {
        let mut gone = Vec::new();
        let swaps = &self.swaps;
        self.swap_held.retain(|&i| {
            let still = swaps.get(i).and_then(|s| s.bind.as_ref()).is_some_and(|b| b.keys.iter().all(|k| held.contains(k)));
            if !still {
                gone.push(i);
            }
            still
        });
        for i in gone {
            self.swap_ev.push(format!("free {i}"));
        }
    }

    /// Java's calls for Auto Swap: "swaps" (them, for Java), "swapput" (all of them anew, from the page or a test),
    /// "swapev" (the keys' events since the last asking), "server" (the server played on: its rules), "swapnote" (a
    /// thing put on: its name, its place, its number for the HUD's picture of it, whether a note is wanted; the Swap
    /// Ring shows it), "swapwait" (a swap waiting out FunTime's pause, for the Swap Ring), "swapinv" (the inventory for
    /// the editor, its lines in the second field: swapui.rs Inv), "swapicons" (the things for Java to draw over the
    /// menu now).
    pub(super) fn swap_call(&mut self, verb: &str, a: &str, b: &str) -> String {
        match verb {
            // (each with whether it may go by itself here: a condition's swap waits for none where it may not)
            "swaps" => {
                let list = self.swaps.iter().map(|s| {
                    let mut v = swap_json(s);
                    v["auto"] = json!(may_go_by_itself(s, &self.server));
                    v
                });
                json!({"server": self.server, "swaps": Value::Array(list.collect())}).to_string()
            }
            "swapput" => {
                self.swaps = load(a);
                self.swap_held.clear();
                self.save();
                String::new()
            }
            "swapinv" => {
                self.swap_inv = super::swapui::Inv::read(b);
                String::new()
            }
            "swapicons" => self.swap_icons_text(),
            "swapev" => std::mem::take(&mut self.swap_ev).join("\n"),
            // Click Pearl's presses since the last asking
            "pearlev" => std::mem::take(&mut self.pearl_ev).to_string(),
            "server" => {
                self.server = a.to_string();
                if self.hidden_here(MODULE) {
                    self.swap_held.clear();
                    self.swap_ev.clear();
                }
                if self.hidden_here(PEARL) {
                    self.pearl_ev = 0;
                }
                String::new()
            }
            "swapnote" => {
                // its place, the number Java keeps the thing by (the HUD draws it), whether a note is wanted
                let mut f = b.split('\t');
                let place = f.next().and_then(|p| p.parse::<usize>().ok()).and_then(|i| PLACES.get(i)).copied().unwrap_or(PLACES[0]);
                let tok = f.next().and_then(|t| t.parse::<i64>().ok()).unwrap_or(-1);
                let noted = f.next() != Some("0");
                let at = self.clock.elapsed().as_secs_f32();
                self.hud.swap_wait = None;
                self.hud.swap_done = Some((tok, at, a.to_string()));
                if noted {
                    let mut ch = super::i18n::tr(place).chars();
                    let place: String = ch.next().map_or(String::new(), |c| c.to_lowercase().chain(ch).collect());
                    self.hud.note_icon(a.to_string(), format!("→ {place}"), at, tok);
                }
                String::new()
            }
            // a swap waiting out FunTime's pause: the number Java keeps its thing by (-1: none waits now), the ticks
            // it waits, its name
            "swapwait" => {
                let tok = a.parse::<i64>().unwrap_or(-1);
                if tok < 0 {
                    self.hud.swap_wait = None;
                } else {
                    let mut f = b.split('\t');
                    let ticks = f.next().and_then(|t| t.parse::<f32>().ok()).unwrap_or(4.0);
                    let name = f.next().unwrap_or("").to_string();
                    self.hud.swap_wait = Some((tok, self.clock.elapsed().as_secs_f32(), ticks * 0.05, name));
                }
                String::new()
            }
            _ => String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swaps_are_kept_as_they_were() {
        let mut s = Swap::new();
        s.place = 1;
        s.trigger = 1;
        s.bind = Some(Bind { keys: vec!["Left Alt".into(), "G".into()], hold: false, value: None });
        s.picks = vec![
            Pick::Item { id: "minecraft:player_head".into(), name: "Сфера Ареса".into(), ench: "minecraft:protection:4".into() },
            Pick::Kind("sphere".into()),
        ];
        s.back = false;
        let mut w = Swap::new();
        w.trigger = 2;
        w.cond = 1;
        w.value = 15.0;
        w.picks = vec![Pick::Kind("chestplate".into())];
        w.timing = Some(Timing { way: 1, stop: 2, before: 100.0, clicks: 50.0, swaps: 0.0, stand: 150.0, spread: 50.0 });
        let list = vec![s, w];
        assert_eq!(load(&save(&list)), list);
        // a kind not known is left out, a broken line is no swaps
        assert!(load(r#"[{"picks":[{"kind":"nope"}]}]"#)[0].picks.is_empty());
        assert!(load("{").is_empty());
    }

    #[test]
    fn click_pearls_keys_throw_where_it_may() {
        let mut g = super::super::Gui::new();
        let i = g.mods.iter().position(|m| m.id == PEARL).unwrap();
        let press = |g: &mut super::super::Gui| {
            g.world_key("Mouse 4", true);
            g.world_key("Mouse 4", false);
        };
        // off: its key does nothing, nor switches it on
        press(&mut g);
        assert!(!g.mods[i].on);
        assert_eq!(g.swap_call("pearlev", "", ""), "0");
        // on: each press is a throw for Java, and it stays on
        g.mods[i].on = true;
        press(&mut g);
        press(&mut g);
        assert!(g.mods[i].on);
        assert_eq!(g.swap_call("pearlev", "", ""), "2");
        assert_eq!(g.swap_call("pearlev", "", ""), "0");
        // where it is banned it is not there, its key throws nothing
        for server in [FUNTIME, ARESMINE, HOLYWORLD] {
            g.swap_call("server", server, "");
            assert!(g.hidden_here(PEARL), "{server}");
            press(&mut g);
            assert_eq!(g.swap_call("pearlev", "", ""), "0");
        }
        // Auto Swap is not on AresMine (her word, 06.10) nor HolyWorld; both are on ReallyWorld
        assert!(g.hidden_here(MODULE));
        g.swap_call("server", REALLYWORLD, "");
        assert!(!g.hidden_here(PEARL) && !g.hidden_here(MODULE));
        // Chunk Animator and No Friend Damage are not on FunTime (her word, 25.10), elsewhere they are
        for id in FUNTIME_BANS {
            assert!(g.mods.iter().any(|m| m.id == id), "{id}");
            assert!(!g.hidden_here(id));
            g.swap_call("server", FUNTIME, "");
            assert!(g.hidden_here(id), "{id}");
            g.swap_call("server", REALLYWORLD, "");
        }
        // No Render's blindness and darkness are not on FunTime (its rules, 24.10.2026): off to the game there even when on
        let nr = g.mod_index("no-render");
        g.mods[nr].on = true;
        for label in FUNTIME_SETTING_BANS {
            let s = g.mods[nr].set.iter_mut().find(|s| s.label == label).expect(label);
            s.kind = super::super::data::Kind::Toggle(true);
            assert!(!g.setting_hidden_here(label));
            g.swap_call("server", FUNTIME, "");
            assert!(g.setting_hidden_here(label), "{label}");
            g.swap_call("server", REALLYWORLD, "");
        }
        press(&mut g);
        assert_eq!(g.swap_call("pearlev", "", ""), "1");
    }

    #[test]
    fn no_totem_goes_by_itself() {
        let mut s = Swap::new();
        s.trigger = 2;
        s.picks = vec![Pick::Kind("elytra".into())];
        assert!(may_go_by_itself(&s, ""));
        assert!(!may_go_by_itself(&s, FUNTIME));
        assert!(!may_go_by_itself(&s, REALLYWORLD));
        s.picks.push(Pick::Kind("talisman".into()));
        assert!(!may_go_by_itself(&s, ""));
        s.picks = vec![Pick::Item { id: "minecraft:totem_of_undying".into(), name: String::new(), ench: String::new() }];
        assert!(!may_go_by_itself(&s, ""));
        // "All armour" does not use its list
        s.place = 5;
        assert!(may_go_by_itself(&s, ""));
        assert!(!may_go_by_itself(&s, FUNTIME));
    }
}
