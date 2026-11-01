//! Cosmetics (the menu's section "Cosmetics": Counter-Strike 2, Minecraft and Dota 2 — the last two still to come): the
//! models of CS2's knives, gloves and rifles in the hands and on the player, their finishes, the page to look them
//! over. Her word, 24.10: «превзойти» — not a model stuck in a blocky hand, CS as it is: its hands, its animations,
//! its sounds, its scope.

pub mod attach;
pub mod cases;
pub mod catalog;
pub mod charm;
pub mod nametag;
pub mod overlay;
pub mod sticker;
pub mod compose;
pub mod crosshair;
pub mod finish;
pub mod glove;
pub mod hands;
pub mod loadout;
pub mod model;
pub mod preview;
pub mod render;
pub mod rifle;
pub mod sound;
pub mod tracer;
pub mod vfxeval;
pub mod store;
pub mod fetch;
pub mod remote;
pub mod texture;
pub mod view;

/// A key down in the world (gui/cards.rs world_key): whether it was the hands' (the inspect key — the loadout's, F as
/// in CS2 — while they are CS's).
pub fn world_key(name: &str) -> bool {
    if !name.is_empty() && name == loadout::current().0.inspect && view::active() {
        view::event(view::Event::Inspect);
        return true;
    }
    false
}

/// Java's calls (the GUI's "cosm"): "dir PATH" where the files are, "held ITEM" the main hand's item each tick,
/// "event NAME" swing / heavy (with " hit" when it struck) / use / unuse / inspect / shoot / zoom ("shoot" with its
/// tracer's ends answers whether it was a CS rifle's: "1"/"0"), "active" whether
/// the hands are ours ("1"/"0"); a rifle's: "fov BASE" the field of view, "camera" its kick ("pitch yaw sens scoped"), "ready"
/// whether the bow may be drawn, "sens" the mouse's speed, "scoped", "volume V" the game's for CS's sounds, "tracers"
/// the shots' streaks to draw, "crosshair" whether CS's crosshair is drawn.
pub fn call(a: &str, b: &str) -> String {
    match a {
        "dir" => {
            store::set_dir(b);
            // what the player has on: beside the cosmetics' folder
            if let Some(parent) = std::path::Path::new(b).parent() {
                loadout::set_path(parent.join("cosmetics.txt"));
                cases::set_history(parent.join("case_history.txt"));
            }
        }
        "held" => view::held(b),
        "event" => {
            // a shot with its tracer's ends: "shoot sx sy sz ex ey ez" — a CS rifle's only (all of it off, or the game's
            // own bow in the hands: its arrow flies as ever)
            if let Some(rest) = b.strip_prefix("shoot ") {
                if !view::rifle_shown() {
                    return "0".into();
                }
                let v: Vec<f64> = rest.split(' ').filter_map(|x| x.parse().ok()).collect();
                if v.len() == 6 {
                    tracer::shot([v[0], v[1], v[2]], [v[3], v[4], v[5]]);
                }
                view::event(view::Event::Shoot);
                return "1".into();
            }
            let e = match b {
                "swing" => Some(view::Event::Swing(false)),
                "swing hit" => Some(view::Event::Swing(true)),
                "heavy" => Some(view::Event::Heavy(false)),
                "heavy hit" => Some(view::Event::Heavy(true)),
                "use" => Some(view::Event::UseStart),
                "unuse" => Some(view::Event::UseStop),
                "inspect" => Some(view::Event::Inspect),
                "shoot" => Some(view::Event::Shoot),
                "zoom" => Some(view::Event::Zoom),
                _ => None,
            };
            if let Some(e) = e {
                view::event(e);
            }
        }
        "active" => return (view::active() as i32).to_string(),
        "third" => return view::third_hand(),
        "fov" => return view::fov(b.parse().unwrap_or(70.0)).to_string(),
        "camera" => {
            // the frame's kick, with the mouse's speed and the scope (asked once a frame)
            let [p, y] = view::camera();
            return format!("{p} {y} {} {}", view::sensitivity(), view::scoped() as i32);
        }
        "ready" => return (view::ready() as i32).to_string(),
        "sens" => return view::sensitivity().to_string(),
        "volume" => sound::volume(b.parse().unwrap_or(1.0)),
        // the tracers' streaks this frame: "ax ay az bx by bz k" each, ";" between
        "tracers" => {
            return tracer::streaks()
                .iter()
                .map(|(a, b, k)| format!("{} {} {} {} {} {} {k}", a[0], a[1], a[2], b[0], b[1], b[2]))
                .collect::<Vec<_>>()
                .join(";");
        }
        "scoped" => return (view::scoped() as i32).to_string(),
        // a kill (the HUD's: the one fought died after the player's hit or shot): StatTrak's for what is in the hands
        "kill" => view::kill(),
        // whether CS's crosshair is drawn (the HUD's) this frame: the game's own is not then
        "crosshair" => return (view::crosshair().is_some() as i32).to_string(),
        _ => {}
    }
    String::new()
}

#[cfg(test)]
mod tests {
    /// No CS rifle in the hands (all of it off, or the game's own bow): the shot is not ours — no tracer, the arrow
    /// drawn as ever.
    #[test]
    fn a_shot_without_a_cs_rifle_is_the_games() {
        assert_eq!(super::call("event", "shoot 777 0 0 777 0 10"), "0");
        assert!(super::tracer::streaks().iter().all(|t| t.0[0] != 777.0));
    }
}
