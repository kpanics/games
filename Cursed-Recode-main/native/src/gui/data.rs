//! What the GUI lists: the modules (names and words from Cursed, the ones the servers' rules allow) and the parts of
//! Recode's engine (World). The modules themselves come to Recode later: here they are their switches and settings.

/// A colour as the player sets it: its rgb, how opaque, a hue that turns by itself (chroma: turns a second, 0 still),
/// and whether it simply follows the theme's (an element's colours do until they are set).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Col {
    pub rgb: u32,
    pub a: f32,
    pub chroma: f32,
    pub auto: bool,
}

impl Col {
    pub const fn new(rgb: u32) -> Col {
        Col { rgb, a: 1.0, chroma: 0.0, auto: false }
    }
    pub const fn alpha(rgb: u32, a: f32) -> Col {
        Col { rgb, a, chroma: 0.0, auto: false }
    }
    /// Follows the theme (`rgb` only what the picker starts from).
    pub const fn theme(rgb: u32) -> Col {
        Col { rgb, a: 1.0, chroma: 0.0, auto: true }
    }
}

#[derive(Clone, PartialEq)]
pub enum Kind {
    Toggle(bool),
    Slider { min: f32, max: f32, step: f32, v: f32, unit: &'static str },
    Choice { v: usize, opts: Vec<&'static str> },
    Multi { on: Vec<bool>, opts: Vec<&'static str> },
    Colour(Col),
    Key(String),
    Text(String),
    Header,
    /// The engine's cost per frame (Quality).
    Cost,
    /// A button (its label is what it does).
    Action,
}

#[derive(Clone)]
pub struct Setting {
    pub label: &'static str,
    pub kind: Kind,
    /// What it was made with: "reset" puts it back.
    pub def: Kind,
    /// Keys that change it from the world (binds.rs): a switch flips, a choice goes on, a button is pressed.
    pub binds: Vec<super::binds::Bind>,
    /// Shown only while a choice of the same list (its label) is one of these (a bit each), or a switch is on (bit 1).
    pub when: Option<(&'static str, u32)>,
}

impl Setting {
    pub fn new(label: &'static str, kind: Kind) -> Setting {
        Setting { label, def: kind.clone(), kind, binds: Vec::new(), when: None }
    }
    /// Whether it is shown in `set` (its list) now.
    pub fn shown(&self, set: &[Setting]) -> bool {
        let Some((label, mask)) = self.when else { return true };
        match set.iter().find(|x| x.label == label).map(|x| &x.kind) {
            Some(Kind::Choice { v, .. }) => mask & (1 << *v) != 0,
            Some(Kind::Toggle(on)) => (mask & 2 != 0) == *on,
            _ => true,
        }
    }
    /// Set to other than its default (a reset is offered).
    pub fn changed(&self) -> bool {
        !matches!(self.kind, Kind::Header | Kind::Cost | Kind::Action) && self.kind != self.def
    }
    pub fn reset(&mut self) {
        self.kind = self.def.clone();
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cat {
    Player,
    Render,
    World,
    Misc,
    Configs,
    Hud,
    Theme,
    Forum,
    Cosmetics,
    Music,
    Settings,
    /// The HUD's elements: inside the Hud module (Interface), not a section of their own.
    HudEl,
    /// The engine's parts (quality, shadows, light, water, weather): in Settings, its Graphics, not a section of their
    /// own (her word, 25.10: the World section is the world's modules).
    Graphics,
}

pub const MODULE_CATS: [Cat; 4] = [Cat::Player, Cat::Render, Cat::World, Cat::Misc];
pub const CLIENT_CATS: [Cat; 7] = [Cat::Configs, Cat::Hud, Cat::Theme, Cat::Forum, Cat::Cosmetics, Cat::Music, Cat::Settings];

impl Cat {
    pub fn name(self) -> &'static str {
        match self {
            Cat::Player => "Player",
            Cat::Render => "Render",
            Cat::World => "World",
            Cat::Misc => "Misc",
            Cat::Configs => "Configs",
            Cat::Hud => "Interface",
            Cat::Theme => "Theme",
            Cat::Forum => "Forum",
            Cat::Cosmetics => "Cosmetics",
            Cat::Music => "Music",
            Cat::Settings => "Settings",
            Cat::HudEl => "Interface",
            Cat::Graphics => "Graphics",
        }
    }
}

#[derive(Clone)]
pub struct Module {
    pub id: &'static str,
    pub cat: Cat,
    pub sub: Option<&'static str>,
    pub name: &'static str,
    pub desc: &'static str,
    pub on: bool,
    /// Its keys (binds.rs), and the ones it came with (a reset puts them back).
    pub binds: Vec<super::binds::Bind>,
    pub def_binds: Vec<super::binds::Bind>,
    /// A group of the engine's parts (World): no switch of its own.
    pub plain: bool,
    pub set: Vec<Setting>,
}

/// Chams' ink as the dye looks thin over white (its sRGB): what the look was made with, its blue-black (engine/ink.rs
/// takes how much of each colour a block of ink at 1 takes out from it).
pub const INK_COLOUR: u32 = 0x727ea3;
/// Chams' polymorphic liquid as made: water a little grey-blue.
pub const LIQUID_COLOUR: u32 = 0xb9c6d2;
/// Chams' cursed flesh as made: ash grey.
pub const CURSE_COLOUR: u32 = 0x8e8c8a;

/// Chams' liquids (the menu's Liquid: Water, Mercury, Oil): the settings of the substance each sets, its colours (the
/// liquid's own, what its depth takes the light to, its highlights, its rim) and its optics and matter (how clear, how
/// much it bends and mirrors what is round it, how much of a metal, how rough, its rainbow film, its bubbles, how
/// thick, how far its surfaces reach to join, how heavy).
pub const LIQUID_COLOURS: [&str; 4] = ["Liquid colour", "Depth colour", "Highlight colour", "Rim colour"];
pub const LIQUID_SLIDERS: [&str; 10] = ["Transparency", "Refraction", "Reflection", "Metalness", "Roughness", "Iridescence", "Bubbles", "Viscosity", "Merging", "Weight"];
pub const LIQUIDS: [([u32; 4], [f32; 10]); 3] = [
    // water: clear, a little blue in its depth, bending what is behind it; a few bubbles in it
    ([LIQUID_COLOUR, 0x4f7487, 0xffffff, 0xdce6ee], [92.0, 1.0, 1.0, 0.0, 3.0, 0.0, 1.0, 0.6, 1.0, 1.0]),
    // mercury: a mirror, heavy, beading up
    ([0xc9ccd1, 0x2b2e33, 0xffffff, 0xeef0f3], [0.0, 0.0, 1.0, 100.0, 4.0, 0.0, 0.0, 0.4, 1.3, 1.8]),
    // oil: black, thick, a rainbow film over its gloss
    ([0x16161a, 0x050506, 0xe6e8f0, 0x8d93a3], [6.0, 1.5, 1.3, 0.0, 6.0, 70.0, 0.0, 1.6, 0.8, 0.9]),
];

fn t(label: &'static str, v: bool) -> Setting {
    Setting::new(label, Kind::Toggle(v))
}
fn s(label: &'static str, min: f32, max: f32, step: f32, v: f32, unit: &'static str) -> Setting {
    Setting::new(label, Kind::Slider { min, max, step, v, unit })
}
fn c(label: &'static str, v: &str, opts: &[&'static str]) -> Setting {
    Setting::new(label, Kind::Choice { v: opts.iter().position(|o| *o == v).unwrap_or(0), opts: opts.to_vec() })
}
fn m(label: &'static str, opts: &[&'static str], on: &[&str]) -> Setting {
    Setting::new(label, Kind::Multi { on: opts.iter().map(|o| on.contains(o)).collect(), opts: opts.to_vec() })
}
fn col(label: &'static str, v: u32) -> Setting {
    Setting::new(label, Kind::Colour(Col::new(v)))
}
fn cola(label: &'static str, v: Col) -> Setting {
    Setting::new(label, Kind::Colour(v))
}
fn k(label: &'static str, v: &str) -> Setting {
    Setting::new(label, Kind::Key(v.to_string()))
}
fn h(label: &'static str) -> Setting {
    Setting::new(label, Kind::Header)
}
fn x(label: &'static str, v: &str) -> Setting {
    Setting::new(label, Kind::Text(v.to_string()))
}
/// These settings shown only while the choice `label` is one of `opts` (its options' indices).
fn only(label: &'static str, opts: &[usize], set: Vec<Setting>) -> Vec<Setting> {
    let mask = opts.iter().fold(0u32, |m, i| m | 1 << i);
    set.into_iter().map(|mut s| {
        s.when = Some((label, mask));
        s
    }).collect()
}
fn act(label: &'static str) -> Setting {
    Setting::new(label, Kind::Action)
}

#[allow(clippy::too_many_arguments)]
fn md(cat: Cat, sub: Option<&'static str>, id: &'static str, name: &'static str, on: bool, bind: Option<&str>, desc: &'static str, set: Vec<Setting>) -> Module {
    let binds: Vec<super::binds::Bind> = bind.map(super::binds::Bind::one).into_iter().collect();
    Module { id, cat, sub, name, desc, on, def_binds: binds.clone(), binds, plain: false, set }
}

fn grp(id: &'static str, name: &'static str, desc: &'static str, set: Vec<Setting>) -> Module {
    Module { id, cat: Cat::Graphics, sub: None, name, desc, on: true, binds: Vec::new(), def_binds: Vec::new(), plain: true, set }
}

/// The engine's parts, their order in the presets (World), and the presets themselves.
pub const ENGINE_KEYS: [&str; 19] = [
    "Sun shadows", "Soft shadows", "Mob shadows", "Cloud shadows", "Leaves let light through", "Coloured light", "Lamps glow", "Light in the air", "Morning fog", "Sky",
    "Reflections", "Ripples", "Refraction", "Caustics", "Foam", "Wind in leaves", "Wet surfaces", "Puddles", "Metals shine",
];
pub const PRESETS: [(&str, f32, &[&str]); 4] = [
    ("Off", 100.0, &[]),
    ("Low", 67.0, &["Coloured light", "Lamps glow", "Sky", "Wind in leaves"]),
    ("Medium", 80.0, &["Coloured light", "Lamps glow", "Sky", "Wind in leaves", "Sun shadows", "Mob shadows", "Cloud shadows", "Leaves let light through", "Reflections", "Ripples", "Caustics", "Foam", "Wet surfaces", "Puddles", "Metals shine"]),
    ("High", 100.0, &ENGINE_KEYS),
];
/// The frame's time on an RTX 4060 at 1280×720 for each preset (measured 08.10).
pub const MEASURED: [f32; 4] = [0.507, 0.735, 1.1, 1.212];
/// What each part costs, for a mix of one's own (an estimate from the same measures).
pub fn part_cost(k: &str) -> f32 {
    match k {
        "Sun shadows" => 0.2,
        "Soft shadows" => 0.04,
        "Mob shadows" => 0.01,
        "Cloud shadows" => 0.012,
        "Leaves let light through" => 0.015,
        "Coloured light" => 0.05,
        "Lamps glow" => 0.035,
        "Light in the air" => 0.045,
        "Morning fog" => 0.01,
        "Sky" => 0.02,
        "Reflections" => 0.07,
        "Ripples" => 0.05,
        "Refraction" => 0.02,
        "Caustics" => 0.01,
        "Foam" => 0.006,
        "Wind in leaves" => 0.012,
        "Wet surfaces" => 0.006,
        "Puddles" => 0.005,
        _ => 0.006,
    }
}

pub fn modules() -> Vec<Module> {
    use super::swaptime as st;
    use Cat::*;
    let (pl, vi, mi, wd) = (Player, Render, Misc, World);
    vec![
        // ---- Player
        md(pl, None, "auto-sprint", "Auto Sprint", false, None, "Three sprint modes: always, forward only, or smart with hunger and movement checks.",
            [vec![c("Mode", "Smart", &["Always", "Forward only", "Smart"])],
             only("Mode", &[2], vec![t("Swim in water", true)]), vec![t("Show in list", true)]].concat()),
        // Auto Swap (gui/swap.rs, cursed.recode.swap): the swaps themselves are its own list, not settings
        md(pl, None, "auto-swap", "Auto Swap", false, None, "Puts what you choose into the off hand, on the head or on as armour: on a key, while it is held, or by itself when it is time. From the whole inventory, the best of what fits first.",
            [
                vec![
                    t("Only from the hotbar", false), t("Notifications", true), t("Sound", true),
                    // its timing (swaptime.rs): a preset sets the rest; one of them changed after, the preset is Custom
                    h("Timing"), c(st::L_PRESET, "Standard", &st::PRESETS), c(st::L_WAY, "One press", &st::WAYS), t(st::L_CLOSE, true),
                    s(st::L_BEFORE, st::RANGES[0].0, st::RANGES[0].1, 50.0, 0.0, " ms"), s(st::L_CLICKS, st::RANGES[1].0, st::RANGES[1].1, 50.0, 0.0, " ms"),
                    s(st::L_SWAPS, st::RANGES[2].0, st::RANGES[2].1, 50.0, 0.0, " ms"), s(st::L_FT, st::FT_RANGE.0, st::FT_RANGE.1, 50.0, 200.0, " ms"),
                    c(st::L_STOP, "Before the swap", &st::STOPS),
                ],
                only(st::L_STOP, &[0], vec![s(st::L_STAND, st::RANGES[3].0, st::RANGES[3].1, 50.0, 150.0, " ms")]),
                only(st::L_STOP, &[0, 1], vec![m(st::L_KEYS, &st::KEYS, &st::KEYS)]),
                vec![s(st::L_SPREAD, st::RANGES[4].0, st::RANGES[4].1, 50.0, 0.0, " ms")],
                // never shown: the preset its settings were set from last (their round arrows go back to it)
                only(st::L_STOP, &[], vec![c(st::L_BASE, "Standard", &st::PRESETS[..4])]),
            ].concat()),
        // Click Pearl (cursed.recode.swap.ClickPearl): its keys throw, only them (gui/swap.rs PEARL)
        md(pl, None, "click-pearl", "Click Pearl", false, Some("Mouse 4"), "On its key or a mouse button throws a pearl: from the off hand, the hotbar or the whole inventory, and goes back to what you held. Only on a key, never in a menu.",
            vec![c("Speed", "One tick", &["One tick", "Like by hand"]), t("Back to the held slot", true), t("Off hand first", true), t("Only from the hotbar", false),
                 c("Stack", "Nearest", &["Nearest", "Smallest", "Largest"]), t("Not point-blank", false), s("Point-blank distance", 1.0, 4.0, 0.5, 2.0, " blocks"),
                 t("Sound when there is none", true)]),
        md(pl, None, "no-friend-damage", "No Friend Damage", false, None, "Your hits never land on your friends. Look at a player and press its key: a friend (again: no longer). Your teammates too, if you like. The list is yours, kept here; Auto Accept trusts it.",
            vec![k("Add a friend", "Middle"), t("Teammates are friends", true), t("Mark over friends", true), t("Notify", true), act("Friends")]),
        md(pl, None, "auto-respawn", "Auto Respawn", false, None, "Comes back by itself after a death, when its time is up (the death screen still seen for that long). Never in hardcore.", vec![s("Delay", 0.0, 3000.0, 100.0, 800.0, " ms")]),
        md(pl, None, "fake-player", "Fake Player", false, None, "Doubles only you see, for a picture or a fight: as many as you put, each where you look — standing and looking at you, your mirror (every move of yours, mirrored), or going after you along your path a moment later. Hit, one flinches and is thrown back; your blows never leave the game.",
            vec![
                c("Mode", "Stands", &super::fakeplayer::MODES), t("Looks at you", true), x("Nickname", ""), t("Copies your armour", true), t("Takes knockback", true),
                act("Put one here"), act("Remove all"), h("Pose editor"),
                s("Edit double", 1.0, 8.0, 1.0, 1.0, ""), c("Pose", "Live", &super::fakeplayer::POSES),
            ].into_iter().chain(super::fakeplayer::POSE_AXES.into_iter().flat_map(|label| only("Pose", &[5], vec![s(label, -180.0, 180.0, 1.0, 0.0, "°")]))).collect()),
        // ---- Render · Players
        md(vi, Some("Players"), "name-tags", "Name Tags", false, None, "Names above players in the client's style: the Recode mark by those who play with it, their health, what they wear, their ping.",
            vec![s("Scale", 0.6, 1.6, 0.05, 1.0, "x"), t("Recode mark", true), t("Health", true), t("Armour", true), t("Ping", false), t("Friends lit", true)]),
        md(vi, Some("Players"), "target-esp", "Target ESP", false, None, "Marks the one you aim at, only while you can see them: a tesseract, a four-dimensional cube turning round their chest, its light on them and on the ground.",
            vec![
                c("Style", "Tesseract", &["Tesseract"]), col("Colour", 0xffffff), s("Speed", 0.2, 2.5, 0.05, 1.0, "x"), s("Size", 0.5, 2.0, 0.05, 1.0, "x"),
                t("Trails", true), t("Flash on a hit", true), t("Light on the world", true), s("Light strength", 0.0, 2.0, 0.05, 1.0, "x"),
                h("Who"), t("Players", true), t("Hostile mobs", true), t("Peaceful mobs", true), t("Villagers and golems", true),
                s("Distance", 3.0, 64.0, 1.0, 24.0, " blocks"),
            ]),
        md(vi, Some("Players"), "hitboxes", "Hitboxes", false, None, "The boxes of what is round you as the game has them, the client's way (F3+B shows them so too): lines of light with a light running along them, only their corners, or glass the world bends through; where the eyes are and which way they look; a flash when one is hit, the one aimed at brighter. Only what you can see.",
            [
                vec![
                    c("Style", "Box", &["Box", "Corners", "Glass"]),
                    h("Who"), t("Yourself (F5)", false), t("Players", true), t("NPCs and mannequins", true), t("Hostile mobs", true), t("Neutral mobs", true),
                    t("Peaceful mobs", false), t("Villagers and golems", false), t("Items on the ground", false), t("Arrows and projectiles", false),
                    t("Boats, minecarts and the rest", false), s("Distance", 4.0, 64.0, 1.0, 24.0, " blocks"),
                    h("Lines"), s("Line width", 0.25, 4.0, 0.05, 1.0, "x"), s("Glow", 0.0, 3.0, 0.05, 1.0, "x"), s("Fill", 0.0, 100.0, 1.0, 12.0, "%"),
                    t("Running light", true), s("Light speed", 0.0, 5.0, 0.05, 1.0, "x"), t("Back edges", true),
                    h("Eyes"), t("Eye line", true), col("Eye line colour", 0xff6b6b), t("Look direction", true), s("Look length", 0.2, 8.0, 0.1, 1.6, " blocks"),
                    col("Look colour", 0x7ab8ff),
                    h("Moments"), t("Flash when hit", true), t("Brighter when aimed at", true), t("Fade in and out", true),
                    h("Colours"), c("Colouring", "By kind", &["By kind", "By health", "One colour"]),
                ],
                only("Colouring", &[0], vec![
                    col("Players' colour", 0xc8c9f0), col("Hostile colour", 0xff5a5f), col("Neutral colour", 0xf0b35a), col("Peaceful colour", 0x6cd38c),
                    col("Villagers' colour", 0x7ab8f5), col("Items' colour", 0xe8e8f2), col("Projectiles' colour", 0x9c8cff), col("Others' colour", 0xbdbdbd),
                ]),
                only("Colouring", &[1], vec![col("Full health", 0x6cd38c), col("Low health", 0xff5a5f)]),
                only("Colouring", &[2], vec![col("Colour", 0xe6e6e6)]),
                only("Style", &[1], vec![h("The corners"), s("Corner length", 5.0, 50.0, 1.0, 24.0, "%"), t("Corner dots", true)]),
                only("Style", &[2], vec![
                    h("The glass"), s("Tint", 0.0, 100.0, 1.0, 22.0, "%"), s("Refraction", 0.0, 3.0, 0.05, 1.0, "x"), s("Shine", 0.0, 3.0, 0.05, 1.0, "x"),
                    s("Edges", 0.0, 3.0, 0.05, 1.0, "x"), t("Frost", false),
                ]),
            ].concat()),
        md(vi, Some("Players"), "chams", "Chams", false, None, "Bodies in one of four looks, each to the smallest thing your own: ink let into water (smoke and threads off a body of dark liquid), a polymorphic liquid (water, mercury, oil or your own: drops of all sizes flying off the body, splashing on the ground and flowing back into it), a curse (grey flesh with dark veins crawling through it, ash smouldering off it, the ground dying under it), a shadow (thick black smoke, fraying and trailing behind, thin slits of light where the creature's eyes are). Each leaves its traces on the ground. Only what you can see.",
            [
                vec![
                    c("Look", "Ink", &["Ink", "Polymorph", "Cursed", "Shadow"]),
                    h("Who"), t("Yourself (F5)", true), t("Your hands", true), t("Players", true), t("Teammates", true), t("NPCs and mannequins", false),
                    t("Villagers and golems", false), t("Hostile mobs", false), t("Peaceful mobs", false), t("Items on the ground", false),
                    t("Arrows and projectiles", false), t("Experience orbs", false), t("Boats, minecarts and the rest", false),
                    s("Distance", 4.0, 128.0, 4.0, 64.0, " blocks"),
                    h("Which parts"), t("Body", true), t("Armour", true), t("Held item", true), t("Cape and elytra", true), t("Skin's outer layer", false),
                ],
                only("Look", &[0], vec![
                    h("The ink"), t("Pattern on the body", true), t("Smoke", true), t("Threads", true), col("Colour", INK_COLOUR),
                    s("Smoke density", 0.2, 2.0, 0.05, 1.0, "x"), s("Smoke lasts", 5.0, 60.0, 1.0, 40.0, " s"), s("Threads strength", 0.0, 2.0, 0.05, 1.0, "x"),
                    s("Shine", 0.0, 2.0, 0.05, 1.0, "x"), s("Pattern speed", 0.0, 3.0, 0.05, 1.0, "x"),
                ]),
                // the substance's settings as water has them (LIQUIDS: a preset chosen sets them; changed, the choice is Own)
                only("Look", &[1], vec![
                    h("The liquid"), c("Liquid", "Water", &["Water", "Mercury", "Oil", "Own"]),
                    h("Colours"), col("Liquid colour", LIQUIDS[0].0[0]), col("Depth colour", LIQUIDS[0].0[1]), col("Highlight colour", LIQUIDS[0].0[2]),
                    col("Rim colour", LIQUIDS[0].0[3]),
                    h("Optics"), s("Transparency", 0.0, 100.0, 1.0, LIQUIDS[0].1[0], "%"), s("Refraction", 0.0, 2.0, 0.05, LIQUIDS[0].1[1], "x"),
                    s("Reflection", 0.0, 2.0, 0.05, LIQUIDS[0].1[2], "x"), s("Metalness", 0.0, 100.0, 1.0, LIQUIDS[0].1[3], "%"),
                    s("Roughness", 0.0, 100.0, 1.0, LIQUIDS[0].1[4], "%"), s("Iridescence", 0.0, 100.0, 1.0, LIQUIDS[0].1[5], "%"),
                    s("Bubbles", 0.0, 2.0, 0.05, LIQUIDS[0].1[6], "x"),
                    h("Matter"), s("Viscosity", 0.2, 2.0, 0.05, LIQUIDS[0].1[7], "x"), s("Merging", 0.3, 2.0, 0.05, LIQUIDS[0].1[8], "x"), s("Weight", 0.2, 2.0, 0.05, LIQUIDS[0].1[9], "x"),
                    h("Behaviour"), t("Drops", true), s("Drops amount", 0.2, 2.0, 0.05, 1.0, "x"), s("Drop size", 0.5, 2.0, 0.05, 1.0, "x"),
                    s("Scatter", 0.2, 2.0, 0.05, 1.0, "x"), s("Return speed", 0.2, 2.0, 0.05, 1.0, "x"), s("Restlessness", 0.0, 2.0, 0.05, 1.0, "x"),
                    t("Splashes", true), t("Crawl back", true),
                ]),
                // never shown: the preset the substance's settings were set from last (their round arrows go back to it)
                only("Look", &[], vec![c("Liquid base", "Water", &["Water", "Mercury", "Oil"])]),
                only("Look", &[3], vec![
                    h("The shadow"), t("Glowing eyes", true), col("Eyes colour", 0xffffff), t("Wisps", true), s("Smoke amount", 0.0, 2.0, 0.05, 1.0, "x"),
                    t("Smoke trail", true),
                ]),
                only("Look", &[2], vec![
                    h("The curse"), col("Flesh colour", CURSE_COLOUR), col("Veins colour", 0x0d0c0e), t("Veins", true), s("Veins speed", 0.0, 3.0, 0.05, 1.0, "x"),
                    s("Darkness", 0.0, 2.0, 0.05, 1.0, "x"), s("Veins beat", 0.0, 2.0, 0.05, 1.0, "x"), col("Dead ground", 0x8a8b8f),
                    h("The ash"), t("Ash", true), col("Ash colour", 0x48474b), col("Smoulder colour", 0xd8d8e0), s("Ash amount", 0.2, 2.0, 0.05, 1.0, "x"),
                    s("Flake size", 0.5, 2.0, 0.05, 1.0, "x"), s("Smoulder", 0.0, 2.0, 0.05, 1.0, "x"), s("Ash lies", 1.0, 30.0, 1.0, 8.0, " s"), t("Dust", true),
                ]),
                vec![
                    h("Traces"), t("Traces on the ground", true), s("Traces last", 3.0, 60.0, 1.0, 20.0, " s"), s("Traces size", 0.5, 2.0, 0.05, 1.0, "x"),
                    h("Cost"), c("Quality", "Ultra", &["Low", "Medium", "High", "Ultra"]), c("Effects from", "Everyone near", &["Only me", "Everyone near"]),
                    s("Bodies with effects", 1.0, 8.0, 1.0, 8.0, ""),
                ],
            ].concat()),
        // Living Motion (native motion.rs, cursed.recode.motion): the walk worked out from how each player moves
        md(vi, Some("Players"), "living-motion", "Living Motion", false, None, "The players' walk as a living body's: a step by the ground covered (the feet never slide), the hips sitting as the stride is long, arms on springs, a lean with speed and into turns, a level head; standing, breath. Yours and the others' (on your screen).",
            vec![c("Style", "Natural", &["Natural", "Nimble", "Heavy"]), c("Players", "Me and others", &["Me and others", "Just me"]),
                 s("Bounce", 0.0, 200.0, 10.0, 100.0, "%"), s("Lean", 0.0, 200.0, 10.0, 100.0, "%"), s("Arm swing", 0.0, 200.0, 10.0, 100.0, "%"),
                 s("Stride", 50.0, 150.0, 5.0, 100.0, "%"), t("Breathing", true)]),
        md(vi, Some("Players"), "trajectories", "Trajectories", false, None, "Where a pearl, an arrow or a potion will land before you throw, and where what flies in sight is going.",
            vec![c("Throwing", "Pearl", &["Pearl", "Arrow", "Potion"]), t("What others throw", true), c("Landing", "Ring", &["Ring", "Cross", "Block"])]),
        md(vi, Some("Players"), "kill-effects", "Kill Effects", false, None, "A kill marked as the free Cursed's death cartoons mark a death: the impact frame (black and red for a moment, a burst from the one killed), a shockwave over the ground, their soul rising, blood or oil, lightning, fireworks in your colour, a sound. On your screen only.",
            vec![
                m("Effects", &super::killfx::EFFECTS, &["Impact frame", "Shockwave", "Soul"]),
                c("Kills of", "Players", &["Players", "Players and mobs"]), c("Liquid", "Blood", &["Blood", "Oil"]),
                cola("Colour", Col::theme(0xffffff)), s("Size", 0.5, 2.0, 0.05, 1.0, "x"),
                c("Sound", "Bell", &super::killfx::SOUNDS), s("Volume", 0.0, 100.0, 5.0, 60.0, "%"),
            ]),
        md(vi, Some("Players"), "hit-effects", "Hit Effects", false, None, "What your blows look like where they land; the damage rising off the one hit; the hurt flash in your colour.",
            vec![c("Effect", "Slash", &["Stars", "Slash", "Shockwave", "Sparks", "Blood"]), s("Size", 0.5, 2.0, 0.05, 1.0, "x"), t("Damage numbers", true), col("Hurt flash", 0xff5a5a)]),
        // ---- Render · Camera
        md(vi, Some("Camera"), "swing-animations", "Swing Animations", false, None, "How the hand swings: the game's own, one of the clients' styles, or your own — made in its editor a moment at a time (where the item is and how it is turned), seen on your hand as you make it.",
            [
                vec![c("Animation source", "Recode", &super::swingport::SOURCES)],
                only("Animation source", &[0], vec![c("Style", "Recode", &super::viewmodel::STYLES), s("Speed", 0.5, 2.0, 0.05, 1.0, "x"), act("Edit the swing")]),
                only("Animation source", &[1], vec![c("Nurik style", "Swing 1", &super::swingport::NURIK), s("Nurik strength", 1.0, 10.0, 1.0, 8.0, ""), t("Spinning", false), s("Spin smoothness", 3.0, 10.0, 1.0, 8.0, "")]),
                only("Animation source", &[2], vec![c("Catlavan style", "Mode 1", &super::swingport::CAT), s("Equip motion", 0.0, 1.0, 0.1, 0.0, "")]),
                only("Animation source", &[3], vec![c("Newcode style", "None", &super::swingport::NEWCODE)]),
                only("Animation source", &[2, 3], vec![s("Swing angle", 0.0, 360.0, 1.0, 100.0, "°"), s("Swing power", 1.0, 10.0, 1.0, 8.0, ""), s("Swing ticks", 1.0, 20.0, 1.0, 11.0, "")]),
                // the player's own (Custom), keyframes as viewmodel.rs keeps them: never a row, made in its editor
                only("Style", &[], vec![x("Keyframes", &super::viewmodel::keys_dump(&super::Gui::swing_preset("Recode")))]),
            ].concat()),
        md(vi, Some("Camera"), "free-look", "Free Look", false, Some("Left Alt"), "On its key the view goes round you while you keep going where you went: the mouse turns the camera, not you.",
            [
                vec![c("Key", "Hold", &["Hold", "Toggle"]), c("View", "Behind", &["Behind", "In front", "First person", "As it is"]),
                     s("Distance", 1.0, 12.0, 0.5, 4.0, " blocks"), s("Height", -3.0, 3.0, 0.1, 0.0, " blocks"), t("Wheel changes the distance", true), t("Fly out of the head", true),
                     s("Sensitivity", 0.2, 3.0, 0.05, 1.0, "x"), t("Invert up and down", false), s("Smoothness", 0.0, 100.0, 5.0, 40.0, "%"),
                     c("On release", "Look back", &["Look back", "Look there"]), t("Smooth return", true)],
                only("Smooth return", &[1], vec![s("Return speed", 0.3, 3.0, 0.1, 1.0, "x")]),
                vec![t("Indicator", true)],
            ].concat()),
        md(vi, Some("Camera"), "view-model", "View Model", false, None, "Where each hand holds its item, turned and sized as you like, for the kinds of items you choose; a preset to start from.",
            [
                vec![
                    c("Preset", "Classic", &["Classic", "Low", "Centered", "Small", "Far", "Custom"]), t("Same for both hands", true),
                    m("Item kinds", &super::viewmodel::KINDS, &super::viewmodel::KINDS),
                    h("Right hand"),
                    s("Right X", -2.0, 2.0, 0.01, 0.0, ""), s("Right Y", -2.0, 2.0, 0.01, 0.0, ""), s("Right Z", -2.0, 2.0, 0.01, 0.0, ""), s("Right Size", 0.1, 3.0, 0.01, 1.0, "x"),
                    s("Right Turn X", -180.0, 180.0, 1.0, 0.0, "°"), s("Right Turn Y", -180.0, 180.0, 1.0, 0.0, "°"), s("Right Turn Z", -180.0, 180.0, 1.0, 0.0, "°"),
                ],
                only("Same for both hands", &[0], vec![
                    h("Left hand"),
                    s("Left X", -2.0, 2.0, 0.01, 0.0, ""), s("Left Y", -2.0, 2.0, 0.01, 0.0, ""), s("Left Z", -2.0, 2.0, 0.01, 0.0, ""), s("Left Size", 0.1, 3.0, 0.01, 1.0, "x"),
                    s("Left Turn X", -180.0, 180.0, 1.0, 0.0, "°"), s("Left Turn Y", -180.0, 180.0, 1.0, 0.0, "°"), s("Left Turn Z", -180.0, 180.0, 1.0, 0.0, "°"),
                ]),
            ].concat()),
        md(vi, Some("Camera"), "motion-blur", "Motion Blur", false, None, "The world smeared along the way it moved on the screen since the last frame, so a turn runs smoothly instead of jumping from frame to frame: as long as at sixty frames a second whatever your frame rate, the hand and the interface sharp, only the turn if you like.",
            vec![s("Strength", 5.0, 100.0, 1.0, 55.0, "%"), t("Only when turning", true), c("Quality", "Medium", &["Low", "Medium", "High"]),
                s("Longest smear", 2.0, 15.0, 0.5, 6.0, "%")]),
        md(vi, Some("Camera"), "aspect-ratio", "Aspect Ratio", false, None, "The world drawn as on a screen of another width to height and stretched over yours (what is seen wider), the hand too if you like, the interface as it is; it goes from one to another softly.",
            vec![c("Ratio", "4:3", &["4:3", "5:4", "3:2", "16:10", "1:1"]), t("The hand too", true), t("Smooth change", true)]),
        md(vi, Some("Camera"), "hands", "Hands", false, None, "Your hands burn with the client's own flame.",
            vec![s("Fire", 0.3, 1.6, 0.05, 1.0, "x"), col("Core", 0xffffff), col("Flame", 0xcfcfcf), t("Heat haze", true), c("Flare", "On a crit", &["On a crit", "On any hit", "Off"])]),
        // ---- Render · Effects
        md(wd, Some("Effects"), "block-outline", "Block Outline", false, None, "The block you look at outlined your way: its exact shape or its box, whole edges or only their corners, lines of light with a light running round them, one colour or two going into each other; its faces filled faintly and the one you aim at lit; what of it is hidden faint, dashed or not shown. It glides from block to block and grows in; as it breaks a fill rises up its sides and over its top and cracks of light spread over it; broken, it flashes outward and throws sparks; a block you place pops into its place.",
            [
                vec![
                    h("The shape"), c("Shape", "Exact", &["Exact", "Box"]), c("Edges", "Whole", &["Whole", "Corners"]),
                ],
                only("Edges", &[1], vec![s("Corner length", 10.0, 50.0, 1.0, 25.0, "%")]),
                vec![
                    c("Hidden edges", "Faint", &["Not shown", "Faint", "Dashed"]),
                    h("Lines"), s("Line width", 0.5, 4.0, 0.05, 1.0, "x"), s("Glow", 0.0, 3.0, 0.05, 1.0, "x"), t("Running light", false),
                ],
                only("Running light", &[1], vec![s("Light speed", 0.2, 4.0, 0.05, 1.0, "x")]),
                vec![
                    t("Pulse", false),
                    h("Colours"), c("Colouring", "One colour", &["One colour", "Gradient", "Shimmer"]), col("Colour", 0xffffff),
                ],
                only("Colouring", &[1, 2], vec![col("Second colour", 0x8a8a8a)]),
                vec![
                    h("Faces"), s("Fill", 0.0, 60.0, 1.0, 6.0, "%"), t("Aimed face", true),
                ],
                only("Aimed face", &[1], vec![s("Face light", 0.0, 100.0, 1.0, 16.0, "%")]),
                vec![
                    h("Moments"), t("Glide", true),
                ],
                only("Glide", &[1], vec![s("Glide speed", 25.0, 300.0, 5.0, 100.0, "%")]),
                vec![
                    t("Grow in", true), t("Fill as it breaks", true), t("Cracks", true), col("Breaking colour", 0xffffff),
                    t("Flash when broken", true), t("Sparks when broken", true),
                ],
                only("Sparks when broken", &[1], vec![s("Sparks", 4.0, 64.0, 1.0, 24.0, "")]),
                vec![t("Pop when placed", true)],
            ].concat()),
        md(wd, Some("Effects"), "chunk-grid", "Chunk Grid", false, None, "The chunks' borders the client's way (F3+G shows them so too): lines laid on the ground round you, your chunk's walls of glass with a grid of blocks and its sections' lines, beams of light at its corners and its neighbours'; the walls glide after you into the next chunk, a pulse goes out over the ground as you enter it.",
            vec![
                h("The ground"), t("Lines on the ground", true), s("Radius", 1.0, 8.0, 1.0, 3.0, " chunks"),
                h("The walls"), t("Walls", true), s("Wall opacity", 0.0, 60.0, 1.0, 12.0, "%"), t("Block grid", true), c("Grid step", "2", &["1", "2", "4"]),
                t("Sections", true), s("Height", 8.0, 64.0, 1.0, 24.0, " blocks"), t("Your section", true), t("Through blocks", false),
                h("The corners"), t("Corner beams", true), t("Neighbours' corners", true),
                h("Moments"), t("Glide", true), t("Pulse on entering", true), t("Fade in and out", true),
                h("Look"), s("Line width", 0.5, 3.0, 0.05, 1.0, "x"), s("Glow", 0.0, 3.0, 0.05, 1.0, "x"), col("Colour", 0xc9ced8), col("Your chunk", 0xffffff),
            ]),
        md(wd, Some("Effects"), "full-bright", "Full Bright", false, None, "No more darkness, coming in and going out softly: the game's gamma pushed past its own (the shade still darker than the open, the light's differences kept) or a night vision's even light; the Darkness of the deep dark and a boss's darkening taken off.",
            vec![c("Mode", "Gamma", &["Gamma", "Night vision"]), s("Strength", 10.0, 100.0, 1.0, 100.0, "%"), s("Fade", 0.0, 2.0, 0.05, 0.6, " s"), t("No Darkness effect", true)]),
        md(vi, Some("Effects"), "no-sound", "No Sound", false, None, "Leaves the game's loud sounds out, or makes them quieter, each kind its own switch: the wither, the dragon, the End portal opening, totems, explosions, thunder, raid horns, and any other by the words of its name.",
            vec![
                h("Kinds"), t("Wither", true), t("Ender Dragon", true), t("End portal", true), t("Totems", true), t("Explosions", true), t("Thunder", true),
                t("Raid horns", true), t("Elder Guardian", true), t("Warden", false), t("Fireworks", false), t("Anvils", false), t("Bells", false), t("Rain", false),
                t("Portals", false),
                h("More"), x("More sounds", ""), s("Their volume", 0.0, 100.0, 5.0, 0.0, "%"),
            ]),
        md(vi, Some("Effects"), "no-render", "No Render", false, None, "Leaves out of the game what gets in the way, each thing its own switch: what covers the view, what shakes or warps it, the fog of effects and liquids, what falls and bursts in the world. What a server forbids is not there on it.",
            vec![
                h("Over the view"), t("Fire", true), t("Water overlay", false), t("Inside a block", true), t("Pumpkin", true), t("Powder snow", true), t("Totem animation", false),
                h("The camera"), t("Hurt camera shake", true), t("Nausea and portal", true),
                h("Effects"), t("Blindness fog", false), t("Darkness pulse", false),
                h("Liquids"), t("Clear water", false), t("Clear lava", false),
                h("The world"), t("Rain and snow", false), t("Explosions", false), t("Potion swirls", false), t("Fireworks", false), t("Falling blocks", false),
            ]),
        md(vi, Some("Effects"), "jump-circle", "Jump Circle", false, None, "Where you jump, a wave goes out over the ground: laid on the blocks as they are, climbing a little up what it meets, stopped by walls and drops; a shock bending the air round its edge, ripples as on water, dust thrown up or embers. Dust of the block you stood on, in its colour, flies, falls and bounces on the blocks. Landing from a height makes one too, bigger the higher; and, if you like, the others' jumps.",
            [
                vec![
                    c("Style", "Shockwave", &["Shockwave", "Ripples", "Dust", "Embers"]), s("Size", 0.5, 2.5, 0.05, 1.0, "x"), s("Duration", 0.4, 2.5, 0.05, 1.0, " s"),
                    s("Line width", 0.3, 3.0, 0.05, 1.0, "x"), s("Glow", 0.0, 3.0, 0.05, 1.0, "x"), col("Colour", 0xffffff),
                    h("The world"), t("Stops at walls and drops", true), t("Bend the air", true), t("Ring in the ground's colour", false), t("Dust", true),
                ],
                only("Dust", &[1], vec![s("Dust amount", 10.0, 250.0, 5.0, 100.0, "%"), t("Dust in the ground's colour", true)]),
                vec![h("When"), t("On jumping", true), t("On landing", true)],
                only("On landing", &[1], vec![s("Landing from", 1.0, 12.0, 0.5, 2.5, " blocks"), t("Bigger from higher", true)]),
                vec![t("Others' jumps", false)],
            ].concat()),
        md(vi, Some("Effects"), "trails", "Trails", false, None, "What is left behind on the way: a ribbon of light standing on it or lying along it, brighter toward its edges with a flow running back along it, narrowing to its tail; a comet's tail shedding sparks off its head; smoke curling up and thinning; or footprints of light going out one by one. Yours, the others', the paths of pearls and arrows in flight, the tips of elytra's wings.",
            [
                vec![c("Style", "Ribbon", &["Ribbon", "Comet", "Smoke", "Footsteps"])],
                only("Style", &[0], vec![c("Ribbon", "Standing", &["Standing", "Lying"]), c("Height", "Waist", &["Feet", "Waist", "Head", "Whole body"]), t("Flow", true)]),
                only("Style", &[1, 2], vec![s("Amount", 10.0, 300.0, 5.0, 100.0, "%")]),
                vec![
                    s("Length", 0.3, 5.0, 0.1, 1.8, " s"), s("Trail width", 0.3, 3.0, 0.05, 1.0, "x"), s("Glow", 0.0, 3.0, 0.05, 1.0, "x"),
                    s("Opacity", 10.0, 100.0, 1.0, 100.0, "%"),
                    h("Colours"), c("Colouring", "One colour", &["One colour", "Gradient", "Shimmer"]), col("Colour", 0xffffff),
                ],
                only("Colouring", &[1, 2], vec![col("Second colour", 0x6a6a6a)]),
                vec![
                    h("Whose"), t("Yourself", true), t("In first person", true), t("Other players", false), t("Elytra wingtips", true),
                    // what is thrown and shot, each kind its own (her word, 25.10: an egg's trail seen, not asked for)
                    h("What flies"), t("Pearls", true), t("Arrows and tridents", true), t("Snowballs and eggs", false), t("Potions and bottles", false),
                ],
            ].concat()),
        md(vi, Some("Effects"), "china-hat", "China Hat", false, None, "A hat over the head, sitting on it as the head is posed: it nods and turns with it, sways after it on a spring when the head moves fast, turns slowly. Glass the world bends through with a light going round it, a paper lantern lit from inside with tassels swinging off its brim, cut crystal throwing glints off its faces, woven straw, or a halo floating over the head.",
            vec![
                c("Style", "Glass", &["Glass", "Lantern", "Crystal", "Straw", "Halo"]), s("Brim width", 0.5, 2.0, 0.05, 1.0, "x"), s("Height", 0.3, 2.5, 0.05, 1.0, "x"),
                s("Spin", 0.0, 3.0, 0.05, 0.4, "x"), s("Glow", 0.0, 3.0, 0.05, 1.0, "x"), s("Opacity", 10.0, 100.0, 1.0, 100.0, "%"), col("Colour", 0xffffff),
                t("Sways", true),
                h("Whose"), t("Yourself", true), t("In first person", false), t("Other players", false),
            ]),
        // World Effects (native weather/): the weather of the paid CS2 client fatality.win's, Dota 2's own particles
        md(wd, Some("Effects"), "world-effects", "World Effects", false, None, "Weather round you: snow — fine flakes glinting as they whirl in gusts, flakes drifting past close by, snow mist running low over the ground; or ash — flakes of ash, sparks with fiery tails, burning embers trailing flame and smoke, embers smouldering on the ground. Falling from as high as you set, in its own colours or in yours.",
            [
                vec![c("Atmosphere", "Snow", &["Snow", "Ash and embers"]), s("Height", 0.0, 16.0, 0.5, 5.0, " blocks"), t("Own colour", false)],
                only("Own colour", &[1], vec![col("Colour", 0xb8dcff)]),
            ]
            .concat()),
        // Item Physic (native itemphysic.rs, cursed.recode.fx.ItemPhysic): dropped items as bodies
        md(wd, Some("Effects"), "item-physic", "Item Physic", false, None, "Dropped items are bodies: thrown turning, they bounce as their stuff does, tip over edges, settle on a face and lie in heaps; they float and rock on water, smoke and burn up in lava, and walking through them kicks them about.",
            vec![s("Bounce", 0.0, 2.0, 0.05, 1.0, "x"), s("Spin", 0.0, 2.0, 0.05, 1.0, "x"), t("Piles", true), t("Kick", true), t("Water", true),
                 t("Lava and fire", true), t("Landing dust", true), t("Sounds", true), s("Volume", 0.0, 2.0, 0.05, 1.0, "x"), t("Pickup flight", true)]),
        md(wd, Some("Effects"), "chunk-animator", "Chunk Animator", false, None, "The world comes into sight moving in, a piece at a time, as it loads.",
            vec![c("Style", "Rise", &["Rise", "Fall", "Grow"]), s("Length", 0.2, 3.0, 0.05, 0.8, " s")]),
        md(vi, Some("Effects"), "custom-pet", "Custom Pet", false, None, "A companion of your own by your side, drawn by the client itself.", vec![c("Pet", "Jellyfish", &["Jellyfish", "Wisp"]), t("Follows closely", true)]),
        // ---- Render · Interface
        md(vi, Some("Interface"), "shulker-preview", "Shulker Preview", false, None, "What a shulker box holds, seen without opening it: a grid in its colour under its name in the inventory; over a box you look at on the ground or in a frame, and over a placed one you have opened before; the ender chest as you saw it last.",
            vec![
                h("Where"), t("In the inventory", true), t("In your hand", true), t("On the ground", true), t("Placed boxes", true), t("Ender chest", true),
                x("Find item", ""), h("Look"), c("Style", "Grid", &["Grid", "Totals"]), t("Box colour", true), s("Distance", 2.0, 16.0, 1.0, 8.0, " blocks"),
            ]),
        md(vi, Some("Interface"), "item-cooldowns", "Item Cooldowns", false, None, "How long until what you used works again: a pearl, a chorus fruit, an apple, a shield, and the servers' own things (a trap, a plast, a stun…), by the names the server gives them. In its panel (placed in the Interface), over the hotbar's slots, by the crosshair.",
            vec![m("Where", &["Panel", "Hotbar", "Crosshair"], &["Panel", "Hotbar"]), t("Sound when ready", false)]),
        md(vi, Some("Interface"), "better-minecraft", "Better Minecraft", false, None, "Small things the game's own interface lacks.", vec![t("Saturation like AppleSkin", true), t("Armour durability", true)]),
        md(wd, Some("Interface"), "world-music", "World Music", false, None, "The words of the song playing (the client's own music, or whatever your player plays), written into the world round you as they are sung: each line in pieces, each piece standing on its own in sight at its own distance, height and side, the blocks in front hiding it; written in light letter by letter as the voice goes, cooling to the sung colour, then burning away into embers and ash.",
            vec![
                s("Size", 0.6, 1.8, 0.05, 1.0, "x"), s("Spread", 4.0, 14.0, 0.5, 8.0, " blocks"), s("Pieces at once", 1.0, 6.0, 1.0, 4.0, ""),
                c("Font weight", "Semi", &["Regular", "Medium", "Semi", "Bold"]), col("Colour", 0xffffff), col("Sung colour", 0xf3e2c0), s("Glow", 0.0, 3.0, 0.05, 1.0, "x"),
                s("Offset", -1.5, 1.5, 0.05, 0.0, " s"), t("Ash when they go", true), t("Through blocks", false),
            ]),
        // ---- World: Recode's engine
        grp("quality", "Quality", "Each step turns a set of the engine's parts on: Low is light and glow, Medium adds shadows and water, High adds soft shadows and light in the air.",
            vec![c("Graphics", "High", &["Off", "Low", "Medium", "High"]), s("World resolution", 50.0, 100.0, 1.0, 100.0, "%"), Setting::new("GPU per frame · RTX 4060 · 720p", Kind::Cost)]),
        grp("shadows", "Shadows", "The sun casts real shadows: from blocks, from mobs and players, from the clouds passing over.",
            vec![t("Sun shadows", true), t("Soft shadows", true), t("Mob shadows", true), t("Cloud shadows", true), t("Leaves let light through", true)]),
        grp("light", "Light", "Torches, lava and lamps light the blocks round them in their own colour; the light hangs in the air at dawn.",
            vec![t("Coloured light", true), t("Lamps glow", true), t("Light in the air", true), t("Morning fog", true), t("Sky", true)]),
        grp("water", "Water", "Water reflects the world and bends what is under it, ripples where you walk and rain falls, lights the bottom.",
            vec![t("Reflections", true), t("Ripples", true), t("Refraction", true), t("Caustics", true), t("Foam", true)]),
        grp("weather", "Weather", "Leaves move in the wind, stone darkens in the rain, puddles gather and dry, metal catches the light.",
            vec![t("Wind in leaves", true), t("Wet surfaces", true), t("Puddles", true), t("Metals shine", true)]),
        // ---- Misc
        md(wd, None, "waypoints", "Waypoints", false, None, "Marks in the world: your own, the servers' events from the chat, with distance.",
            vec![t("Server events", true), t("Distance", true), t("Beam to the sky", true), k("List", "M")]),
        md(wd, None, "death-coords", "Death Coords", false, None, "Where you died, said in the chat (pressed, copied) and kept in a list of its own: when, on which server, in which world, of what.",
            vec![t("Line in the chat", true), s("Keep the last", 1.0, 100.0, 1.0, 30.0, ""), act("Deaths")]),
        md(mi, None, "voice-chat", "Voice Chat", false, None, "Talk with the players near you and with your group, heard from where they stand.",
            vec![c("Talk", "Push", &["Push", "Voice"]), k("Push to talk", "V"), s("Volume", 0.0, 200.0, 5.0, 100.0, "%"), t("Group only", false)]),
        md(mi, None, "streamer-mode", "Streamer Mode", false, Some("F8"), "While you stream or share your screen, what tells who and where you are is not on it: your nick (shown as another), the others' nicks, the server's address, the coordinates, your skin. On by its switch, or by itself while OBS runs or Discord streams the game (Go Live; its overlay counts too).",
            vec![
                c("Turns on", "By its switch", &super::streamer::WHEN), x("Shown nick", "Player"),
                m("Hide", &super::streamer::HIDE, &["My nick", "Server address", "Coordinates"]),
                t("Mark on the screen", false), t("Say when it turns on", true),
            ]),
        md(mi, None, "kill-message", "Kill Message", false, None, "Says something in the chat when you kill: as many lines as you like, at random or in turn, with what happened in them — {name}, {hp}, {weapon}, {distance}, {streak}. Lines of their own for a streak, a revenge, a kill on your last hearts, a long shot.",
            [
                vec![
                    c("Order", "At random", &["At random", "In turn", "At random, no repeats"]), act("Messages"),
                    c("Send to", "The chat", &["The chat", "Only me"]), x("Before each", ""),
                    s("Not more often than", 0.0, 30.0, 0.5, 3.0, " s"), s("Delay", 0.0, 2000.0, 50.0, 300.0, " ms"), t("Players only", true),
                    h("Their own lines"), t("Streaks", true), t("Revenge", true), t("Last hearts", true), t("Long shots", true), s("Long shot from", 10.0, 80.0, 1.0, 30.0, " blocks"),
                ],
                // the lists themselves (lines by \u{1e}): edited in their card, never as a row
                only("Order", &[], vec![
                    x("Lines", "gg {name}\u{1e}ez {name}\u{1e}{name}, sit\u{1e}gg wp {name}"),
                    x("Streak lines", "{streak} in a row\u{1e}who's next? {streak} already"),
                    x("Revenge lines", "revenge on {name}\u{1e}remember me, {name}?"),
                    x("Last hearts lines", "clutched with {hp} hearts\u{1e}{hp} hearts was enough"),
                    x("Long shot lines", "{distance} blocks away, {name}\u{1e}sniped from {distance} blocks"),
                    x(super::messages::SETTING, ""),
                ]),
            ].concat()),
        md(mi, None, "auto-accept", "Auto Accept", false, None, "Accepts what is asked of you as the server's own button would: teleports, duels, invitations, trades. From everyone, your friends, or the nicks you list, after a human moment.",
            vec![
                h("What"), t("Teleport requests", true), t("Duels", false), t("Invitations", false), t("Trades", false),
                t("Separate request settings", false), t("Notify", true), t("Sound when accepted", false),
                s("Accept interval", 0.0, 60.0, 0.5, 1.0, " s"),
            ].into_iter().chain(only("Separate request settings", &[0], vec![
                h("From whom"), c("From", "Friends", &["Everyone", "Friends", "Friends and the list", "Only the list"]), x("The list", ""),
                s("Delay", 0.0, 5000.0, 50.0, 600.0, " ms"),
            ])).chain(super::play::ACCEPT_FIELDS.into_iter().flat_map(|f| only("Separate request settings", &[1], vec![
                h(f[0]), c(f[1], "Friends", &["Everyone", "Friends", "Friends and the list", "Only the list"]), x(f[2], ""),
                s(f[3], 0.0, 5000.0, 50.0, 600.0, " ms"),
            ]))).collect()),
        // Server Adaptation (gui/adapt.rs): the servers' own things, as the mode has them
        md(mi, None, "server-adaptation", "Server Adaptation", false, None, "Helpers for the servers' own things on ReallyWorld, HolyWorld, FunTime and their copies: while you hold a trap, a plast, a stun, where it will reach (lit when someone is inside); how long one you used still holds, over its place; keys that use them from the hotbar or the inventory. On ReallyWorld: the chat's words it mutes for held back, /fix all after a fight. The mode is yours (or the server's); what a server forbids stays forbidden there whatever the mode.",
            [
                vec![
                    c("Mode", "ReallyWorld", &super::adapt::MODES), t("By the server", false),
                    h("Zones"), t("Zones", true), cola("Zone colour", Col::theme(0xffffff)), cola("Someone inside", Col::new(0x6cd38c)), s("Zone thickness", 0.5, 4.0, 0.5, 2.0, " px"),
                    h("Timers"), t("Timers", true), s("Trap time", 3.0, 60.0, 1.0, 15.0, " s"),
                ],
                only("Mode", &[2], vec![t("Dragon trap", false), s("Dragon trap time", 5.0, 60.0, 1.0, 30.0, " s"), s("Plast time", 3.0, 60.0, 1.0, 20.0, " s")]),
                vec![h("Keys"), k("Experience scroll", "")],
                only("Mode", &[0], vec![k("Anti Fly", ""), k("Ender trap", "")]),
                only("Mode", &[1], vec![k("Stun trap", ""), k("Chorus trap", ""), k("Stun", ""), k("Jack o'Lantern", ""), k("Snowball", "")]),
                only("Mode", &[2], vec![k("Trap", ""), k("Plast", ""), k("Disorientation", ""), k("Sheer dust", ""), k("Fire tornado", ""), k("God aura", ""), k("Freeze snowball", "")]),
                only("Mode", &[0], vec![
                    h("ReallyWorld"), t("Chat filter", true),
                    x("Words", "хуй, хуе, хуё, пизд, ебат, ебан, ебал, ебл, уеб, бля, сука, суки, мудак, мудил, гандон, пидор, пидар, шлюх, долбо, залуп, funtime, фантайм, holyworld, холиворлд, discord.gg, t.me/"),
                    t("/fix all after a fight", false),
                ]),
            ].concat()),
    ]
}

/// The menu's own settings (Settings).
pub fn client_settings() -> Vec<Setting> {
    vec![k("Open the menu", "Right Shift"), t("Sounds", true), s("Volume", 0.0, 100.0, 5.0, 70.0, "%"), t("Previews", true), t("Hints", true), act("Show the tour"), h("Everything"), act("Reset everything")]
}

/// How the whole client looks (Theme), in one list: the colours, the glass, the world behind any screen, the motion,
/// the menu's layout, the HUD's size and editing. The menu, the HUD, the chat, the inventories and the menus all read
/// these (theme.rs, by their labels); an element only has its own colour over them.
pub fn theme_settings() -> Vec<Setting> {
    vec![
        h("Code"),
        act("Copy the theme code"),
        act("Paste a theme code"),
        act("Edit the words"),
        h("Colours"),
        col("Accent", 0xececec), col("Text", 0xececec), col("Secondary text", 0xa3a3a3), col("Surfaces", 0xffffff), col("Switches", 0xe6e6e6),
        h("Glass"),
        cola("Glass", Col::alpha(0x040404, 0.9)), s("Blur", 0.0, 100.0, 5.0, 80.0, "%"), s("World colour through", 0.0, 100.0, 5.0, 0.0, "%"),
        s("Corner radius", 0.0, 200.0, 5.0, 100.0, "%"), t("Shadow", true), t("Sheen", true), t("Cursor light", true),
        h("World behind the screens"),
        cola("Dim colour", Col::new(0x000000)), s("Dim the world", 0.0, 100.0, 5.0, 55.0, "%"), s("World colour around", 0.0, 100.0, 5.0, 0.0, "%"),
        h("Motion"),
        t("Animations", true), s("Speed", 25.0, 300.0, 5.0, 100.0, "%"), c("Opening animation", "Glass", &["Glass", "Fade", "None"]),
        h("Menu"),
        s("Menu size", 70.0, 150.0, 5.0, 100.0, "%"), s("Window width", 70.0, 130.0, 5.0, 100.0, "%"), s("Window height", 70.0, 130.0, 5.0, 100.0, "%"),
        s("Text size", 80.0, 130.0, 5.0, 100.0, "%"), c("Columns", "2", &["1", "2", "3"]), t("Descriptions", true), t("Binds on tiles", true), t("Keep where it was moved", true),
        s("Sidebar shade", 0.0, 100.0, 5.0, 40.0, "%"),
        h("Interface"),
        s("Interface scale", 50.0, 200.0, 5.0, 100.0, "%"), s("Interface text", 80.0, 130.0, 5.0, 100.0, "%"), c("Padding", "Normal", &["Compact", "Normal", "Roomy"]),
        t("Hide with F3", false), t("Hide the game's effect icons", true),
        h("Editing"),
        t("Snap to the edges", true), t("Snap to elements", true), t("Guides", true), s("Grid", 0.0, 32.0, 2.0, 0.0, " px"),
        act("Edit the layout"), act("Reset the interface"),
        h("Everything"),
        act("Reset the theme"),
    ]
}

/// Settings renamed (their key now, as it was saved before): a value saved under the old name is read for the new.
/// The chat's «Lines» (17.10): the word was the watermark separators' too, «Линии» in Russian; here it is rows.
pub const RENAMED: [(&str, &str); 5] = [
    ("chat.Lines shown", "chat.Lines"),
    // Trails' "Pearls and arrows" became a switch a kind (25.10)
    ("trails.Pearls", "trails.Pearls and arrows"),
    ("trails.Arrows and tridents", "trails.Pearls and arrows"),
    ("chams.Effects from", "chams.Smoke from"),
    ("chams.Bodies with effects", "chams.Bodies with smoke"),
];

/// The HUD's look as it was saved before it became a part of the Theme (14.10): its old label, the theme's.
pub const HUD_LOOK_MOVED: [(&str, &str); 9] = [
    ("Scale", "Interface scale"), ("Text size", "Interface text"), ("Padding", "Padding"), ("Hide with F3", "Hide with F3"),
    ("Hide the game's effect icons", "Hide the game's effect icons"), ("Snap to the edges", "Snap to the edges"), ("Snap to elements", "Snap to elements"),
    ("Guides", "Guides"), ("Grid", "Grid"),
];

/// Where an element sits: an anchor of the screen (0..8: rows of left, middle, right; top, middle, bottom) and how far
/// from it (px of a 1920x1080 screen at the HUD's scale).
#[derive(Clone)]
pub struct Elem {
    pub id: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    pub on: bool,
    pub anchor: u8,
    pub off: [f32; 2],
    pub def: (bool, u8, [f32; 2]),
    pub set: Vec<Setting>,
    /// The module it is a part of in the Interface (None: the Hud's own; Some: «Custom Hotbar»…).
    pub group: Option<&'static str>,
}

fn el(id: &'static str, name: &'static str, desc: &'static str, anchor: u8, off: [f32; 2], own: Vec<Setting>) -> Elem {
    // what every element has, then its own
    let mut set = vec![
        h("Look"),
        s("Scale", 50.0, 200.0, 5.0, 100.0, "%"), s("Opacity", 10.0, 100.0, 5.0, 100.0, "%"), t("Background", true),
        c("Style", "Glass", &["Glass", "Blur", "Interference", "Colour"]),
        cola("Glass", Col::theme(0x040404)), cola("Text", Col::theme(0xececec)), cola("Secondary text", Col::theme(0xa3a3a3)), cola("Accent", Col::theme(0xececec)),
        s("Corners", 0.0, 300.0, 5.0, 100.0, "%"), s("Inner space", 40.0, 250.0, 5.0, 100.0, "%"), t("Shadow", true), t("Line of light", false),
        h("Letters"),
        s("Text size", 60.0, 200.0, 5.0, 100.0, "%"), c("Font", "Geist", &["Geist", "Geist Mono"]), t("Bold", false),
        s("Text shadow", 0.0, 100.0, 5.0, 0.0, "%"),
        h("Shows"),
    ];
    set.extend(own);
    set.push(act("Reset this element"));
    Elem { id, name, desc, on: true, anchor, off, def: (true, anchor, off), set, group: None }
}

/// An element without some of the look every element has (it does not use them): never a row.
fn without(mut e: Elem, labels: &[&str]) -> Elem {
    for st in e.set.iter_mut().filter(|st| labels.contains(&st.label)) {
        st.when = Some(("Style", 0));
    }
    e
}

/// A part of the Custom Hotbar (Interface): an element of the HUD of its own (dragged in the editor), no plate behind
/// it by default (the hotbar has one).
fn part(id: &'static str, name: &'static str, desc: &'static str, off: [f32; 2], plate: bool, own: Vec<Setting>) -> Elem {
    let mut e = el(id, name, desc, 7, off, own);
    e.group = Some("Custom Hotbar");
    for s in e.set.iter_mut() {
        // (with no plate the words get a shadow: over a bright world they stay read)
        let k = match s.label {
            "Background" => Kind::Toggle(plate),
            "Text shadow" if !plate => Kind::Slider { min: 0.0, max: 100.0, step: 5.0, v: 45.0, unit: "%" },
            _ => continue,
        };
        s.kind = k.clone();
        s.def = k;
    }
    e
}

/// What a bar of the body is drawn as (health, food, armour, air, the mount).
const BAR_KINDS: [&str; 4] = ["Bar", "Segments", "Icons", "Number"];

/// The Custom Hotbar's parts (Interface → Custom Hotbar): the game's hotbar and bars drawn by the client, each
/// moved and changed as any element of the HUD.
pub fn hotbar_elements() -> Vec<Elem> {
    vec![
        part("hb-hotbar", "Hotbar", "The nine slots and the off hand: the held one lit, counts, wear, cooldowns, the held item's name. A shelf: your health, food and level over the slots on the same glass, the experience along its foot.", [0.0, -10.0], true,
            vec![c("Layout", "Row", &["Row", "Column", "Two rows"]), c("Slot shape", "Rounded", &["Rounded", "Square", "Circle"]),
                s("Slot size", 28.0, 72.0, 1.0, 44.0, " px"), s("Gap", 0.0, 16.0, 1.0, 4.0, " px"), t("Empty slots", true),
                c("Held slot", "Lift", &["Lift", "Glow", "Fill", "Line"]), cola("Held colour", Col::theme(0xffffff)), s("Slide speed", 25.0, 300.0, 5.0, 100.0, "%"), s("Stretch", 0.0, 100.0, 5.0, 60.0, "%"),
                t("Slot numbers", false), t("Counts", true), t("Durability", true), c("Durability as", "Bar", &["Bar", "Ring", "Number"]),
                t("Wear colours", true), t("Cooldowns", true), c("Off hand", "Auto", &["Auto", "Left", "Right", "Hidden"]),
                t("Item name", true), s("Item name time", 0.5, 6.0, 0.25, 2.0, " s"), s("Name after", 0.0, 600.0, 10.0, 160.0, " ms"), t("Attack strength", true),
                h("Shelf"),
                c("Bars", "On the shelf", &["On the shelf", "Apart"]), s("Bar thickness", 2.0, 8.0, 1.0, 4.0, " px"), t("Numbers", true),
                t("Level", true), t("Experience edge", true), t("Armour", true), t("Air", true),
                t("Lost health trail", true), t("Low health pulse", true), t("Effect tints", false)]),
        part("hb-xp", "Experience", "Your level and how far to the next; a flash at a new one.", [0.0, -74.0], false,
            vec![c("Kind", "Line", &["Line", "Bar", "Level only"]), s("Width", 100.0, 700.0, 2.0, 444.0, " px"), s("Thickness", 1.0, 14.0, 1.0, 3.0, " px"),
                t("Level", true), c("Level at", "Centre", &["Centre", "Above", "Left", "Right"]), cola("Experience colour", Col::theme(0xffffff)), t("Level up flash", true)]),
        part("hb-health", "Health", "Your health with absorption over it: a bar, segments, hearts or a number; the lost health's trail.", [-124.0, -92.0], false,
            vec![c("Kind", "Bar", &BAR_KINDS), c("Fills from", "Left", &["Left", "Right"]), s("Width", 80.0, 460.0, 2.0, 196.0, " px"), s("Thickness", 3.0, 24.0, 1.0, 8.0, " px"),
                t("Icon", true), t("Number", true), cola("Health colour", Col::theme(0xffffff)), cola("Absorption colour", Col::new(0xf3cb6c)),
                t("Lost health trail", true), t("Low health pulse", true), s("Low health", 5.0, 60.0, 5.0, 30.0, "%"), t("Effect tints", true)]),
        part("hb-food", "Food", "How full you are, saturation inside it; hunger tints it.", [124.0, -92.0], false,
            vec![c("Kind", "Bar", &BAR_KINDS), c("Fills from", "Right", &["Left", "Right"]), s("Width", 80.0, 460.0, 2.0, 196.0, " px"), s("Thickness", 3.0, 24.0, 1.0, 8.0, " px"),
                t("Icon", true), t("Number", true), cola("Food colour", Col::theme(0xffffff)), t("Saturation", true), cola("Saturation colour", Col::new(0xf3cb6c)),
                t("Low food pulse", true), t("Effect tints", true)]),
        part("hb-armor", "Armour", "Your armour points and toughness.", [-124.0, -116.0], false,
            vec![c("Kind", "Bar", &BAR_KINDS), c("Fills from", "Left", &["Left", "Right"]), s("Width", 80.0, 460.0, 2.0, 196.0, " px"), s("Thickness", 3.0, 24.0, 1.0, 6.0, " px"),
                t("Icon", true), t("Number", true), cola("Armour colour", Col::theme(0xffffff)), t("Toughness", true), t("Hide with none", true)]),
        part("hb-air", "Air", "Your breath under water: only while it runs out.", [124.0, -116.0], false,
            vec![c("Kind", "Bar", &BAR_KINDS), c("Fills from", "Right", &["Left", "Right"]), s("Width", 80.0, 460.0, 2.0, 196.0, " px"), s("Thickness", 3.0, 24.0, 1.0, 6.0, " px"),
                t("Icon", true), t("Number", false), cola("Air colour", Col::new(0x9fd8ff))]),
        part("hb-mount", "Mount", "What you ride: its health and the jump being charged.", [0.0, -144.0], false,
            vec![c("Kind", "Bar", &BAR_KINDS), c("Fills from", "Left", &["Left", "Right"]), s("Width", 80.0, 460.0, 2.0, 196.0, " px"), s("Thickness", 3.0, 24.0, 1.0, 8.0, " px"),
                t("Icon", true), t("Number", true), cola("Mount colour", Col::theme(0xffffff)), t("Jump", true), cola("Jump colour", Col::new(0xf3cb6c))]),
    ]
}

/// The Custom Chat's element (Interface → Custom Chat): closed, one glass panel behind its newest lines over the world;
/// open, a window of its own (channels and conversations on the left, the messages, the field). Placed in the editor,
/// its window stretched by its corner.
pub fn chat_elements() -> Vec<Elem> {
    let mut e = el("chat", "Chat", "The chat: closed, one glass panel behind its newest lines; open, a window of its own with the channels, your conversations, the messages and the field. Placed and sized as you like.", 6, [8.0, -150.0],
        vec![
            h("Closed"),
            s("Width", 300.0, 1400.0, 10.0, 640.0, " px"), s("Lines shown", 2.0, 30.0, 1.0, 10.0, ""),
            c("Newest", "At the bottom", &["At the bottom", "At the top"]),
            t("Panel", true), c("Panel size", "Full width", &["Full width", "Fit the lines"]), s("Panel opacity", 0.0, 100.0, 5.0, 100.0, "%"),
            t("Fade out", true), s("Fade after", 2.0, 120.0, 1.0, 10.0, " s"), c("Fading", "Line by line", &["Line by line", "All together"]),
            c("New lines", "Rise", &["Rise", "Fade", "At once"]),
            h("Opened"),
            s("Window width", 600.0, 1800.0, 10.0, 1000.0, " px"), s("Window height", 320.0, 1000.0, 10.0, 580.0, " px"),
            c("Opens", "Grows", &["Grows", "Slides in", "Fades in", "From a point"]), s("Opening speed", 25.0, 300.0, 5.0, 100.0, "%"),
            c("Behind it", "Nothing", &["Nothing", "Dim", "Blur"]), s("Behind strength", 5.0, 100.0, 5.0, 50.0, "%"),
            c("Left column", "Whole", &["Whole", "Icons", "Hidden"]), s("Left column width", 160.0, 360.0, 4.0, 228.0, " px"),
            c("Field at", "Bottom", &["Bottom", "Top"]),
            h("Messages"),
            s("Line spacing", 80.0, 180.0, 5.0, 100.0, "%"), s("Message gap", 0.0, 16.0, 1.0, 0.0, " px"), s("Wrapped lines indent", 0.0, 40.0, 1.0, 0.0, " px"),
            c("Server colours", "As sent", &["As sent", "Softened", "One colour"]),
            c("Player names", "Bold", &["Plain", "Bold", "Accent"]), t("Rank prefixes", true),
            c("Time", "On hover", &["Always", "On hover", "Never"]), t("Seconds", false), t("24-hour clock", true),
            t("Player heads", true), s("Heads size", 12.0, 30.0, 1.0, 18.0, " px"), c("Heads shape", "Rounded faces", &["Square faces", "Rounded faces", "Round faces"]),
            t("Stack repeats", true), t("Server ads", false),
            t("Highlight mentions", true), cola("Mentions colour", Col::theme(0xececec)), t("Mention sound", true),
            t("Event cards", true), t("Pinned events", true),
            t("Pause dividers", false), s("Pause", 1.0, 30.0, 1.0, 5.0, " min"),
            h("Conversations"),
            c("Bubbles", "On both sides", &["On both sides", "All on the left"]), s("Bubble corners", 0.0, 100.0, 5.0, 65.0, "%"),
            cola("Your bubbles", Col::theme(0xececec)), cola("Their bubbles", Col::alpha(0xffffff, 0.075)),
            h("Channels"),
            t("Players channel", true), t("Private channel", true), t("Server channel", true), t("Mentions channel", true),
            x("Own channel 1", ""), x("Channel 1 words", ""), x("Own channel 2", ""), x("Channel 2 words", ""), x("Own channel 3", ""), x("Channel 3 words", ""),
            h("Behaviour"),
            t("Smooth scrolling", true), s("History", 100.0, 2000.0, 50.0, 500.0, ""), act("Hidden players"),
        ]);
    // (its own sections instead of "Shows")
    e.set.retain(|s| !(s.label == "Shows" && matches!(s.kind, Kind::Header)));
    e.group = Some("Custom Chat");
    vec![e]
}

/// The Custom Scoreboard's element (Interface → Custom Scoreboard): the server's sidebar on the client's glass.
pub fn board_elements() -> Vec<Elem> {
    let mut e = el("scoreboard", "Scoreboard", "The server's sidebar: where it is, its size, title and lines aligned, the score numbers, the glass, a line that changed lit.", 5, [-10.0, 0.0],
        vec![t("Title", true), c("Title align", "Centre", &["Left", "Centre", "Right"]), c("Lines align", "Left", &["Left", "Centre", "Right"]),
            c("Numbers", "Auto", &["Auto", "Always", "Never"]), cola("Numbers colour", Col::theme(0xa3a3a3)), s("Line spacing", 0.0, 12.0, 1.0, 2.0, ""),
            s("Title size", 60.0, 200.0, 5.0, 100.0, "%"), s("Least width", 60.0, 400.0, 5.0, 110.0, " px"),
            t("Divider", true), t("Flash changes", true), s("Max lines", 1.0, 15.0, 1.0, 15.0, ""),
            t("Hide the site line", false), t("Hide empty lines", false)]);
    e.group = Some("Custom Scoreboard");
    vec![e]
}

/// Other Custom Things' elements (Interface → Other Custom Things): the boss bars, the title, the action bar, the name
/// tags' look.
pub fn other_elements() -> Vec<Elem> {
    let mut v = vec![
        el("bossbars", "Boss bars", "The boss bars: their fill easing with a trail, the server's notches and colours, each sliding in and out.", 1, [0.0, 10.0],
            vec![s("Width", 120.0, 700.0, 2.0, 364.0, " px"), s("Thickness", 2.0, 16.0, 1.0, 5.0, " px"), s("Gap", 2.0, 30.0, 1.0, 8.0, " px"),
                t("Names", true), s("Names size", 60.0, 200.0, 5.0, 100.0, "%"), t("Percent", false),
                c("Colours", "As the server", &["As the server", "Accent"]), c("Notches", "As the server", &["As the server", "None"]),
                t("Lost trail", true), t("Colour glow", true)]),
        // (titles are the server's words in its colours: no accent of the client's in them)
        without(el("titles", "Titles", "The title and subtitle: where, how big, how they come in, a shade or glass behind them.", 4, [0.0, -150.0],
            vec![c("Animation", "Pop", &["Pop", "Rise", "Fade"]), s("Animation speed", 25.0, 300.0, 5.0, 100.0, "%"),
                s("Title size", 50.0, 200.0, 5.0, 100.0, "%"), t("Subtitle", true), s("Subtitle size", 50.0, 200.0, 5.0, 100.0, "%")]), &["Accent"]),
        el("actionbar", "Action bar", "The line over the hotbar (a server's mana, timers…): where, how big, its pill.", 7, [0.0, -160.0],
            vec![c("Backdrop", "Glass", &["Glass", "Shade", "None"]), c("Server colours", "As sent", &["As sent", "One colour"])]),
        el("holograms", "Holograms", "The name tags over players and things: their plate, a shadow, their size, fading with distance instead of popping out.", 4, [0.0, 0.0],
            vec![c("Plate", "As the game", &["As the game", "None", "Colour"]), s("Plate opacity", 0.0, 100.0, 5.0, 25.0, "%"), cola("Plate colour", Col::new(0x000000)),
                t("Words shadow", false), t("Fade with distance", true), s("Fade from", 8.0, 60.0, 1.0, 40.0, " m"), s("Tag size", 50.0, 200.0, 5.0, 100.0, "%")]),
    ];
    for e in v.iter_mut() {
        e.group = Some("Other Custom Things");
    }
    v
}

/// Every element of the HUD, the Hud's own and the Custom Hotbar's, Chat's, Scoreboard's and Other Custom Things'.
pub fn all_elements() -> Vec<Elem> {
    let mut v = hud_elements();
    v.extend(hotbar_elements());
    v.extend(f3_elements());
    v.extend(chat_elements());
    v.extend(board_elements());
    v.extend(other_elements());
    v
}

pub fn hud_elements() -> Vec<Elem> {
    vec![
        el("watermark", "Watermark", "The client's name with your frames, ping and the time.", 0, [24.0, 24.0],
            vec![t("Logo", true), cola("Logo colour", Col::theme(0xffffff)), x("Title", "Recode"), act("Placeholders"), t("FPS", true), t("Ping", true), t("Clock", true), t("24-hour clock", true),
                t("Seconds", false), t("Server", false), c("Separators", "Dots", &["Dots", "Lines", "Spaces"])]),
        el("keybinds", "Keybinds", "The modules that are on, with their keys.", 0, [24.0, 76.0],
            vec![t("Title", false), x("Title text", "Modules"), act("Placeholders"), t("Keys", true), c("Order", "As enabled", &["As enabled", "By name", "By length"]),
                s("Most rows", 1.0, 20.0, 1.0, 10.0, ""), t("Only modules with a key", false), t("Light up when switched", true)]),
        el("coords", "Coordinates", "Where you are, the other side of the portal, which way you face, how fast you go.", 6, [24.0, -24.0],
            vec![t("XYZ", true), t("Other dimension", true), t("Facing", true), t("Speed", true), c("Decimals", "0", &["0", "1", "2"])]),
        el("music", "Music", "What plays, with its cover and its waveform.", 2, [-24.0, 24.0],
            vec![t("Cover", true), t("Waveform", true), cola("Waveform colour", Col::theme(0xffffff)), t("Artist", true), t("Time", true), t("Only while playing", true)]),
        el("effects", "Effects", "Your effects, each in a ring of the time it has left.", 2, [-24.0, 94.0],
            vec![t("Rings", true), cola("Ring", Col::theme(0xffffff)), cola("Harmful", Col::new(0xff8a3d)), t("Level", true), t("Time", true),
                c("Order", "As given", &["As given", "By time left", "By name"]), t("Blink before the end", true)]),
        el("target", "Target", "The one you fight: their health, armour, totems; a live portrait.", 4, [210.0, 110.0],
            vec![c("Portrait", "3D model", &["3D model", "Head", "None"]), t("Armour", true), t("Totems", true), t("Health number", true), t("Distance", false),
                cola("Health", Col::theme(0xffffff)), cola("Absorption", Col::new(0xf3cb6c)), t("Lost health trail", true), t("Shake on a hit", true), s("Shown after a hit", 1.0, 30.0, 0.5, 4.0, " s")]),
        el("cooldowns", "Cooldowns", "Item Cooldowns' panel: what you used and how long until it is ready, as icons in rings or a list of bars.", 4, [0.0, 64.0],
            [
                vec![c("Kind", "Icons", &["Icons", "List"])],
                only("Kind", &[0], vec![t("Rings", true)]),
                vec![cola("Ring", Col::theme(0xffffff)), t("Seconds", true), t("Tenths under 10 s", true)],
                only("Kind", &[1], vec![t("Names", true), s("List width", 100.0, 260.0, 5.0, 150.0, " px")]),
            ].concat()),
        el("swap-ring", "Swap Ring", "What a swap puts on, by the crosshair: on FunTime in a ring of the pause before it goes on.", 4, [0.0, 40.0],
            vec![t("Rings", true), cola("Ring", Col::theme(0xffffff)), t("Thing's name", true), c("Shown", "On every swap", &["On every swap", "Only the pause"]),
                s("Stays", 0.3, 3.0, 0.1, 0.9, " s")]),
        el("notes", "Notifications", "A word when a module is switched, a totem pops, someone dies, a fight ends.", 8, [-24.0, -76.0],
            vec![s("Duration", 1.0, 10.0, 0.5, 3.5, " s"), s("Most at once", 1.0, 8.0, 1.0, 4.0, ""),
                t("Modules", true), t("Totems", true), t("Kills", true), t("Combat", true)]),
    ]
}

/// Custom F3's cards (Interface → Custom F3, gui/hudf3.rs): what the game's F3 shows that a player uses, each card an
/// element of the HUD dragged where you like; while F3 is on (or always, if set so).
pub fn f3_elements() -> Vec<Elem> {
    let f3 = |id: &'static str, name: &'static str, desc: &'static str, anchor: u8, off: [f32; 2], own: Vec<Setting>| {
        let mut e = el(id, name, desc, anchor, off, own);
        e.group = Some("Custom F3");
        e
    };
    vec![
        f3("f3-position", "Position", "Where you are: the coordinates, the block, the chunk and where in it, which way you face, the world you are in.", 0, [24.0, 132.0],
            vec![t("Title", true), t("Only with F3", true), t("XYZ", true), c("Decimals", "2", &["0", "1", "2", "3"]), t("Block", true), t("Chunk", true),
                t("In the chunk", true), t("Facing", true), t("Pitch", true), t("Dimension", true), t("Other dimension", true)]),
        f3("f3-performance", "Performance", "How the game runs: the frames and their graph, the frame's time, the ping, the memory, the video card, the entities.", 2, [-24.0, 132.0],
            vec![t("Title", true), t("Only with F3", true), t("FPS", true), t("Graph", true), cola("Graph colour", Col::theme(0xffffff)), t("Frame time", true),
                t("Ping", true), t("Memory", true), t("Memory bar", true), t("Video card", true), t("Entities", true)]),
        f3("f3-world", "World", "The world round you: the biome, the light of the sky and of the blocks, the time, the day, the weather, the moon, the difficulty.", 0, [24.0, 352.0],
            vec![t("Title", true), t("Only with F3", true), t("Biome", true), t("Light", true), t("Time", true), t("Day", true), t("Weather", true), t("Moon", true),
                t("Difficulty", true)]),
        f3("f3-target", "Looking at", "What the crosshair is on: the block (its id, its place, its state, the liquid in it) or the creature (its kind, its health, how far).", 2, [-24.0, 352.0],
            vec![t("Title", true), t("Only with F3", true), t("Block", true), t("Block id", true), t("Block place", true), t("State", true), s("State lines", 1.0, 8.0, 1.0, 4.0, ""),
                t("Liquid", true), t("Creature", true), t("Creature id", true), t("Health", true), t("Distance", true)]),
    ]
}

/// The HUD's section of the menu: its look as a whole (a group, no switch) and its elements, each a module with its
/// switch and settings.
pub fn hud_modules() -> Vec<Module> {
    for e in all_elements() {
        let labels: Vec<&str> = e.set.iter().filter(|s| !matches!(s.kind, Kind::Header | Kind::Action | Kind::Cost)).map(|s| s.label).collect();
        for (n, l) in labels.iter().enumerate() {
            debug_assert!(!labels[n + 1..].contains(l), "{}: two settings named {l} (they are saved and read by name)", e.id);
        }
    }
    let mut v = vec![Module {
        id: "hud",
        cat: Cat::Hud,
        sub: None,
        name: "Hud",
        desc: "Every element of the interface on the screen: what each shows and how it looks; dragged where you like in the editor.",
        on: true,
        binds: Vec::new(),
        def_binds: Vec::new(),
        plain: false,
        set: Vec::new(),
    }];
    for e in hud_elements() {
        v.push(Module { id: e.id, cat: Cat::HudEl, sub: None, name: e.name, desc: e.desc, on: e.on, binds: Vec::new(), def_binds: Vec::new(), plain: false, set: e.set });
    }
    // the Custom Hotbar: its own module in the Interface, its parts elements of the HUD
    v.push(Module {
        id: "custom-hotbar",
        cat: Cat::Hud,
        sub: None,
        name: "Custom Hotbar",
        desc: "The hotbar, health, food, armour, air, experience and the mount drawn by the client: each moved and changed as you like.",
        on: true,
        binds: Vec::new(),
        def_binds: Vec::new(),
        plain: false,
        set: Vec::new(),
    });
    for e in hotbar_elements() {
        v.push(Module { id: e.id, cat: Cat::HudEl, sub: e.group, name: e.name, desc: e.desc, on: e.on, binds: Vec::new(), def_binds: Vec::new(), plain: false, set: e.set });
    }
    // Custom F3: its module in the Interface, its cards elements of the HUD
    v.push(Module {
        id: "custom-f3",
        cat: Cat::Hud,
        sub: None,
        name: "Custom F3",
        desc: "F3 in the HUD's style: where you are, how the game runs, the world, what you look at — four cards, each moved and changed as you like; the game's own words of F3 and its keys a player has no use for are gone.",
        on: true,
        binds: Vec::new(),
        def_binds: Vec::new(),
        plain: false,
        set: Vec::new(),
    });
    for e in f3_elements() {
        v.push(Module { id: e.id, cat: Cat::HudEl, sub: e.group, name: e.name, desc: e.desc, on: e.on, binds: Vec::new(), def_binds: Vec::new(), plain: false, set: e.set });
    }
    // the Custom Chat: its module in the Interface, the chat an element of the HUD
    v.push(Module {
        id: "custom-chat",
        cat: Cat::Hud,
        sub: None,
        name: "Custom Chat",
        desc: "The chat drawn by the client: glass behind it, lines rising smoothly, times, faces, repeats counted, mentions lit — placed and sized as you like.",
        on: true,
        binds: Vec::new(),
        def_binds: Vec::new(),
        plain: false,
        set: Vec::new(),
    });
    for e in chat_elements() {
        v.push(Module { id: e.id, cat: Cat::HudEl, sub: e.group, name: e.name, desc: e.desc, on: e.on, binds: Vec::new(), def_binds: Vec::new(), plain: false, set: e.set });
    }
    // the Custom Scoreboard: its module, the sidebar an element of the HUD
    v.push(Module {
        id: "custom-scoreboard",
        cat: Cat::Hud,
        sub: None,
        name: "Custom Scoreboard",
        desc: "The server's sidebar on the client's glass: placed and sized as you like, aligned, numbers kept, recoloured or hidden, changes lit.",
        on: true,
        binds: Vec::new(),
        def_binds: Vec::new(),
        plain: false,
        set: Vec::new(),
    });
    for e in board_elements() {
        v.push(Module { id: e.id, cat: Cat::HudEl, sub: e.group, name: e.name, desc: e.desc, on: e.on, binds: Vec::new(), def_binds: Vec::new(), plain: false, set: e.set });
    }
    // Other Custom Things: the boss bars, titles, the action bar and the name tags
    v.push(Module {
        id: "other-custom",
        cat: Cat::Hud,
        sub: None,
        name: "Other Custom Things",
        desc: "Boss bars, titles, the action bar and the name tags over things, drawn and moved by the client: smooth, placed and sized as you like.",
        on: true,
        binds: Vec::new(),
        def_binds: Vec::new(),
        plain: false,
        set: Vec::new(),
    });
    for e in other_elements() {
        v.push(Module { id: e.id, cat: Cat::HudEl, sub: e.group, name: e.name, desc: e.desc, on: e.on, binds: Vec::new(), def_binds: Vec::new(), plain: false, set: e.set });
    }
    // Custom Inventory: the inventory and the containers as blocks of the client's glass
    v.push(Module {
        id: "custom-inventory",
        cat: Cat::Hud,
        sub: None,
        name: "Custom Inventory",
        desc: "The inventory and the containers in the game's own arrangement, as blocks of glass: the player, the armour, the crafting, the backpack, the hotbar, a chest's block over them. Your cells, gaps, corners and colours.",
        on: true,
        binds: Vec::new(),
        def_binds: Vec::new(),
        plain: false,
        set: vec![
            h("Look"),
            s("Scale", 60.0, 160.0, 5.0, 100.0, "%"), cola("Glass", Col::theme(0x040404)), s("Corners", 0.0, 300.0, 5.0, 100.0, "%"),
            s("Inner space", 4.0, 30.0, 1.0, 12.0, " px"), s("Block gap", 0.0, 40.0, 1.0, 10.0, " px"), t("Titles", true), t("Effects", true),
            h("Cells"),
            s("Cell size", 28.0, 64.0, 1.0, 40.0, " px"), s("Cell gap", 0.0, 16.0, 1.0, 5.0, " px"), s("Cell corners", 0.0, 100.0, 5.0, 30.0, "%"),
            cola("Cell colour", Col::theme(0x000000)), cola("Hover colour", Col::theme(0xffffff)), cola("Chosen ring", Col::theme(0xececec)),
            h("Creative"),
            c("Left panel", "Whole", &["Whole", "Icons"]),
            h("Places"),
            act("All blocks back"),
        ],
    });
    // Custom Gui: every container screen on the client's glass
    v.push(Module {
        id: "custom-gui",
        cat: Cat::Hud,
        sub: None,
        name: "Custom Gui",
        desc: "Every container of the game — the inventory, chests, furnaces, villagers, the anvil, the beacon… — on the client's glass: soft slots, a light that slides to the slot under the pointer, light words, its own tooltips, opening smoothly.",
        on: true,
        binds: Vec::new(),
        def_binds: Vec::new(),
        plain: false,
        set: vec![
            h("Look"),
            c("Style", "Glass", &["Glass", "Blur", "Interference", "Colour"]), cola("Glass", Col::theme(0x040404)), s("Corners", 0.0, 300.0, 5.0, 100.0, "%"),
            t("Shadow", true), t("Line of light", false), s("Panel margin", 0.0, 12.0, 1.0, 3.0, " px"),
            h("Slots"),
            s("Slot opacity", 0.0, 40.0, 1.0, 8.0, "%"), cola("Slot colour", Col::new(0xffffff)),
            c("Hover", "Glow", &["Glow", "Fill", "Lift", "None"]), cola("Hover colour", Col::theme(0xffffff)), s("Hover speed", 25.0, 300.0, 5.0, 100.0, "%"),
            h("Words"),
            cola("Labels colour", Col::theme(0xececec)), t("Own tooltips", true),
            h("Screen"),
            s("Dim", 0.0, 90.0, 5.0, 35.0, "%"), c("Opening", "Grow", &["Grow", "Rise", "None"]), s("Opening speed", 25.0, 300.0, 5.0, 100.0, "%"),
            t("Keep server menus", true),
        ],
    });
    v
}

#[cfg(test)]
mod tests {
    /// A new player's client: every function off (her word, 25.10: "все функции при заходе первом в клиент и
    /// пользователя должны быть выключены").
    #[test]
    fn every_function_is_off_at_first() {
        let on: Vec<&str> = super::modules().iter().filter(|m| !m.plain && super::MODULE_CATS.contains(&m.cat) && m.on).map(|m| m.id).collect();
        assert!(on.is_empty(), "on at first: {on:?}");
    }
}
