//! No Sound (her word, 25.10: "визер, активация портала в Энд, тотемы, взрывы и прочее — отключаемо"): the game's loud
//! sounds left out (or made quieter), each kind its own switch, and any others by the words of their names. Java asks
//! as each sound starts ("nosound\t<its id>": how loud it may be, nothing when as it is).

use super::theme;
use super::Gui;

pub(super) const MODULE: &str = "no-sound";

/// The kinds, by their switches, and the beginnings of the sounds' names they hold.
pub(super) const KINDS: [(&str, &[&str]); 14] = [
    ("Wither", &["entity.wither."]),
    ("Ender Dragon", &["entity.ender_dragon."]),
    ("End portal", &["block.end_portal.spawn", "block.end_portal_frame.fill"]),
    ("Totems", &["item.totem.use"]),
    ("Explosions", &["entity.generic.explode", "entity.dragon_fireball.explode", "entity.tnt.primed", "entity.creeper.primed"]),
    ("Thunder", &["entity.lightning_bolt."]),
    ("Raid horns", &["event.raid.horn"]),
    ("Elder Guardian", &["entity.elder_guardian.curse"]),
    ("Warden", &["entity.warden."]),
    ("Fireworks", &["entity.firework_rocket."]),
    ("Anvils", &["block.anvil."]),
    ("Bells", &["block.bell."]),
    ("Rain", &["weather.rain"]),
    ("Portals", &["block.portal."]),
];

impl Gui {
    /// How loud a sound starting may be (0..1), or nothing when it is left as it is.
    pub(super) fn nosound_call(&self, id: &str) -> String {
        let name = id.trim().rsplit(':').next().unwrap_or("").to_lowercase();
        // a CS rifle in the hands: the bow's and the crossbow's own sounds are not its (its shot and bolt are CS's)
        if (name.starts_with("item.crossbow.") || name == "entity.arrow.shoot") && crate::cosmetics::view::rifle_shown() {
            return "0.00".into();
        }
        let Some(set) = self.mods.iter().find(|m| m.id == MODULE && m.on).map(|m| &m.set) else { return String::new() };
        if name.is_empty() {
            return String::new();
        }
        let kind = KINDS.iter().any(|(label, starts)| theme::on(set, label) && starts.iter().any(|s| name.starts_with(s)));
        let more = theme::text(set, "More sounds").to_lowercase();
        let listed = more.split(',').map(str::trim).any(|w| !w.is_empty() && name.contains(w));
        if !kind && !listed {
            return String::new();
        }
        format!("{:.2}", (theme::num(set, "Their volume") / 100.0).clamp(0.0, 1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loud_sounds_are_left_out_as_asked() {
        let mut g = Gui::new();
        assert_eq!(g.nosound_call("minecraft:entity.wither.spawn"), "");
        let i = g.mods.iter().position(|m| m.id == MODULE).unwrap();
        g.mods[i].on = true;
        assert_eq!(g.nosound_call("minecraft:entity.wither.spawn"), "0.00");
        assert_eq!(g.nosound_call("minecraft:item.totem.use"), "0.00");
        assert_eq!(g.nosound_call("minecraft:entity.player.hurt"), "");
        // one of its own list, and quieter instead of gone
        for s in g.mods[i].set.iter_mut() {
            match s.label {
                "More sounds" => s.kind = super::super::data::Kind::Text("player.hurt".into()),
                "Their volume" => {
                    if let super::super::data::Kind::Slider { v, .. } = &mut s.kind {
                        *v = 25.0;
                    }
                }
                _ => {}
            }
        }
        assert_eq!(g.nosound_call("minecraft:entity.player.hurt"), "0.25");
    }
}
