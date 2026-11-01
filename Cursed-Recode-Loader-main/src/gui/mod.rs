//! The client's menu (Right Shift in a world): sections on the left, the modules as tiles on the right; the left
//! button switches a module, the right one (or its arrows) opens its settings. All of it lives here: the state, the
//! layout, the motion, the input; Java's screen (cursed.recode.gui.ClickGui) only hands over the mouse and the keys,
//! and puts the picture (render.rs, shaders/gui) over the game. The window rises out of a burning line, as the menu.

pub mod data;
mod dev;
pub mod draw;
pub mod render;
pub mod text;
pub mod theme;
pub mod dynfont;
pub mod hud;
pub mod pics;
pub mod i18n;
mod i18n_more;
pub mod binds;
mod cards;
mod configs;
mod modulecode;
mod reset;
mod hudpanel;
pub mod hudx;
pub mod media;
mod words;
mod swap;
mod swaptime;
mod swapui;
mod altsui;
mod cosmcharm;
mod cosmcode;
mod cosmhand;
mod cosmkeys;
mod cosmlook;
mod cosmstage;
mod cosmsticker;
mod cosmtag;
mod cosmui;
mod cosmxhair;
mod cosmcase;
mod cosmsets;
mod undo;
mod search;
mod cfgserver;
mod cloud;
mod tour;
mod startup;
mod play;
mod messages;
mod messageui;
mod playui;
mod hudshulker;
mod viewmodel;
mod swingport;
mod swingui;
mod freelook;
mod adapt;
mod mask;
mod streamer;
mod killfx;
mod nosound;
mod nametags;
mod fakeplayer;

use data::{Cat, Kind, Module, Setting, CLIENT_CATS, ENGINE_KEYS, MEASURED, MODULE_CATS, PRESETS};
use draw::{grey, mix, Canvas, Rgba};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;
use text::{fonts, Face};
use theme::pal;
use i18n::tr;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Hit {
    None,
    Header,
    Window,
    Nav(Cat),
    Seg(usize),
    Search,
    Tile(usize),
    TileSwitch(usize),
    TileExpand(usize),
    Panel(usize),
    PanelSwitch(usize),
    PanelBind(usize),
    PanelClose,
    /// an open module's round arrow: its settings back as they came; the section's (by its filter), every module's
    PanelReset(usize),
    ModuleCode(usize, bool),
    SectionReset,
    /// the tour (tour.rs): 0 its veil and card, 1 next, 2 skip, 3 back
    Tour(u8),
    Toggle(usize),
    Slider(usize),
    Choice(usize, usize),
    Drop(usize),
    Chip(usize, usize),
    Colour(usize),
    KeyRow(usize),
    TextRow(usize),
    Pop(usize),
    /// the Configs page: its buttons (0, which), a saved config (1, which)
    Cfg(u8, u16),
    Outside,
    /// a setting's reset (it differs from its default), a button row
    Reset(usize),
    Action(usize),
    /// the colour picker: 0 inside it, 1 the wheel, 2 brightness, 3 opacity, 4 hex, 5 a swatch (which), 6 follow the
    /// theme, 7 chroma, 8 chroma's speed, 9 reset
    Pick(u8, u8),
    /// the HUD's header button: its layout editor
    EditLayout,
    /// a HUD element (the chat, the editor)
    HudElem(usize),
    /// the search over an element's settings (by the element)
    PopFind,
    /// the card over the menu (cards.rs): which part of it, which row
    Card(u8, u16),
    Cloud(u8, i64, i64),
    /// the key icon by the search: every bind on one page
    BindsBtn,
    /// the language by the account: the card of every language
    LangBtn,
    /// the account's row: Alt Manager's card (altsui.rs)
    AltBtn,
    /// a module's keys on its tile, in its header (the card of its binds)
    TileBind(usize),
    PanelBinds(usize),
    /// a setting's keys by its label
    RowBind(usize),
    /// a card of the Hud module's panel: that element chosen; one of its "+" (which kind)
    HudPick(usize),
    HudAdd(usize),
    /// a word of the words' page (its place in the list), its round arrow
    Word(u32),
    WordBack(u32),
    /// Auto Swap's list and editor (swapui.rs): which part, which one
    Swap(u8, u16),
    /// the Cosmetics page (cosmui.rs): which part, which one
    Cosm(u8, u16),
}

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Closed,
    Opening,
    Open,
    Closing,
}

/// Whose settings the open panel shows: a module (the HUD's elements are modules of the HUD section), the menu's own,
/// the theme.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Owner {
    Module(usize),
    Client,
    Theme,
    /// the Cosmetics page's colours (its material's, its crosshair's): the picker works on them as on any setting
    Cosm,
}

/// What a bind is on: a module (its switch), or one of an owner's settings.
#[derive(Clone, Copy, PartialEq, Debug)]
enum BindTo {
    Module(usize),
    Setting(Owner, usize),
}

/// A key waited for: a bind (on what, which of its binds; None a new one), or a key setting's key.
#[derive(Clone, Copy, PartialEq)]
enum Listen {
    Bind(BindTo, Option<usize>),
    Key(Owner, usize),
    /// an Auto Swap's key (which swap)
    Swap(usize),
}

/// The card over the menu (cards.rs): the binds of a module or a setting, and where it was opened from.
#[derive(Clone, PartialEq)]
enum Card {
    Binds { to: BindTo, at: [f32; 4] },
    /// every language of the game, with a search
    Lang { at: [f32; 4] },
    /// a config to make of the client now; a saved one; one from a code (its text, the parts it holds)
    NewCfg { at: [f32; 4] },
    Cfg { i: usize, at: [f32; 4] },
    Cloud { at: [f32; 4], tab: usize, page: usize },
    Import { text: String, have: u8, at: [f32; 4] },
    ModuleImport { module: String, text: String, at: [f32; 4] },
    /// the word typed before everything goes back
    Reset { at: [f32; 4] },
    /// every placeholder with its value now; a press puts it in the element's text being typed
    Placeholders { i: usize, at: [f32; 4] },
    /// every account (Alt Manager), a new nickname typed straight in
    Alts { at: [f32; 4] },
    /// Death Coords' list, No Friend Damage's friends (playui.rs)
    Deaths { at: [f32; 4] },
    Friends { at: [f32; 4] },
    /// Kill Message's lines (the module's number, the list open)
    Lines { i: usize, tab: usize, at: [f32; 4] },
}

#[derive(Clone)]
enum Pop {
    List { owner: Owner, set: usize, at: [f32; 4] },
}

/// The colour picker open on a colour setting: where it was opened from, the hue, saturation and value it holds (kept
/// apart from the colour so a hue survives black or grey), what the mouse drags, the hex being typed.
#[derive(Clone)]
struct Picker {
    owner: Owner,
    set: usize,
    at: [f32; 4],
    hsv: [f32; 3],
    drag: u8,
    hex: Option<String>,
    rect: [f32; 4],
    /// where it was drawn: the wheel's centre and radius, the sliders' left and right
    geo: [f32; 5],
}

/// The picker's ready colours.
const SWATCHES: [u32; 12] = [0xffffff, 0xd6d6d6, 0x8a8a8a, 0x26262b, 0xff5a5a, 0xff8a3d, 0xffd34a, 0x7cf2a2, 0x4fd6ff, 0x6a8bff, 0xb47bff, 0xff6fcf];

pub struct Gui {
    mods: Vec<Module>,
    client: Vec<Setting>,
    /// the configs on this computer, the one loaded last, the parts a card takes, what a paste is for (0 nothing),
    /// a delete pressed once (the next does it), the word that says what happened
    saved: Vec<configs::Saved>,
    loaded_cfg: Option<String>,
    cfg_parts: u8,
    /// How tall a saved config's card needs to be (as it was drawn last: its chips wrap as the language has them).
    cfg_card_need: f32,
    paste_for: u8,
    paste_module: Option<String>,
    card_arm: bool,
    /// The section's reset pressed once (which, when): a second press within 3 s does it.
    section_arm: Option<(Cat, f32)>,
    reset_arm: Option<(reset::Target, f32)>,
    /// What the search found (search.rs), worked out once per query; a setting it found, lit as its module opens
    /// (the module, the setting, when).
    found_memo: search::Memo,
    flash: Option<(usize, usize, f32)>,
    /// Servers' profiles (cfgserver.rs): the server last seen, the profile on now.
    cfg_host: String,
    profile: Option<cfgserver::Profile>,
    interface_profile: Option<cfgserver::InterfaceProfile>,
    cfg_interface_context: Option<(String, Option<usize>)>,
    /// The tour of the menu (tour.rs): on now; due (a new player's first menu).
    tour: Option<tour::Tour>,
    tour_due: bool,
    toast: Option<(String, f32)>,
    cat: Cat,
    filter: [usize; 4],
    /// Settings' part: 0 the menu's, 1 the graphics (the engine's parts).
    settings_tab: usize,
    q: String,
    typing: bool,
    open: Option<usize>,
    open_at: f32,
    scroll: f32,
    scroll_to: f32,
    content: f32,
    w: f32,
    h: f32,
    r: f32,
    win: [f32; 4],
    dx: f32,
    dy: f32,
    drag: Option<[f32; 4]>,
    slider: Option<(Owner, usize)>,
    mouse: [f32; 2],
    hover: Hit,
    tick_at: f32,
    phase: Phase,
    phase_at: f32,
    shown_at: f32,
    now: f32,
    clock: Instant,
    last: f32,
    dt: f32,
    anim: HashMap<u64, f32>,
    hits: Vec<([f32; 4], Hit)>,
    listen: Option<Listen>,
    /// the keys held down in the menu, and those pressed since a bind began to be recorded
    gheld: Vec<String>,
    /// Ctrl+Z and Ctrl+Y: the steps of the settings taken back and done again (undo.rs).
    undo: undo::Undo,
    rec: Vec<String>,
    /// the key a bind being recorded (or a key setting) just took: the letter the game sends after it is not typed
    /// anywhere, nor its repeats while it is held
    eat: Option<String>,
    /// the modules that act in the game (play.rs): what they keep between ticks
    play: play::Play,
    play_ui: playui::PlayUi,
    /// Shulker Preview's panels over the world (hudshulker.rs)
    shulk: hudshulker::Shulk,
    /// View Model's preset as it was last seen (vm_follow)
    vm_seen: usize,
    swing_port: swingport::State,
    /// Swing Animations' editor over the world (swingui.rs)
    swing_ed: swingui::SwingEd,
    /// Free Look's camera (freelook.rs)
    fl: freelook::Fl,
    /// Server Adaptation's facts, timers and asks (adapt.rs)
    adapt: adapt::Adapt,
    /// Streamer Mode: on now, what it looked at (streamer.rs)
    streamer: streamer::Streamer,
    /// Kill Effects' waves and impact frame (killfx.rs)
    killfx: killfx::KillFx,
    /// Name Tags' players this frame (nametags.rs)
    tags: nametags::Tags,
    /// Fake Player's doubles (fakeplayer.rs)
    fakes: fakeplayer::Fakes,
    /// the Cosmetics page's colours as settings, for the client's own picker (cosmlook.rs, cosmxhair.rs)
    cosm_cols: Vec<Setting>,
    /// the keys held in the world, and the held binds on now (what they switched, to be put back as they go)
    wheld: Vec<String>,
    holds: Vec<(BindTo, Vec<String>, cards::Prior)>,
    /// what a pressed bind with a value put its setting from (pressed again, it goes back)
    bind_back: Vec<(BindTo, Kind)>,
    /// the card over the menu, when it opened; every bind on one page (the key icon by the search)
    card: Option<Card>,
    card_at: f32,
    /// the card a press past it just closed: a press on what opened it leaves it closed (not opened again at once)
    card_shut: Option<Card>,
    /// Chams' Liquid as it was last seen (liquid_follow: changed to a preset, the preset is set)
    liquid_seen: usize,
    /// Auto Swap's timing preset as it was last seen (swaptime.rs timing_follow)
    timing_seen: usize,
    binds_page: bool,
    /// the words' page (Theme → Edit the words): open, its search, the word being changed and what is typed for it
    words_page: bool,
    word_q: String,
    word_focus: Option<String>,
    word_buf: String,
    /// the Hud module's chosen element (a module's index)
    hud_sel: usize,
    /// the game's languages (code, name, region) and the one chosen; what is typed in a card, its list's scroll, where
    /// it is
    langs: Vec<(String, String, String)>,
    lang_sel: String,
    card_field: String,
    card_scroll: f32,
    card_scroll_to: f32,
    card_rect: [f32; 4],
    cloud_local: Vec<String>,
    cloud_codes: Vec<String>,
    cloud_revoke: Option<(i64, String, f32)>,
    cloud_recovery_blocked: bool,
    cloud_seen_version: i64,
    /// the text setting last typed in (a placeholder pressed goes there)
    last_text: Option<(Owner, usize)>,
    /// what the menu asks of the game (Java polls it: a language, the clipboard…), a line each
    outbox: Vec<String>,
    /// Auto Swap (swap.rs): the player's swaps, the keys' events for Java, the held swaps on now; the server played
    /// on (its rules: "", "FunTime", "ReallyWorld", "HolyWorld")
    swaps: Vec<swap::Swap>,
    swap_ev: Vec<String>,
    swap_held: Vec<usize>,
    server: String,
    /// its editor (swapui.rs): the swap chosen, the templates shown, the inventory Java sent, the things for Java to
    /// draw this frame, what is dragged (where the chip was taken, where it would go), the number's track, how tall it
    /// needs to be, a delete pressed once (when)
    swap_sel: usize,
    swap_tpl: bool,
    swap_inv: swapui::Inv,
    swap_icons: Vec<swapui::Icon>,
    /// Alt Manager's card: a cross pressed once (whose, when), what is wrong with the nickname typed (since when), the
    /// free Cursed's accounts not here yet, the faces Java draws this frame
    /// Click Pearl's key presses not yet taken by Java ("pearlev")
    pearl_ev: u32,
    alt_armed: Option<(String, f32)>,
    alt_wrong: Option<(&'static str, f32)>,
    alt_cursed: usize,
    alt_faces: Vec<altsui::AltFace>,
    /// the Cosmetics page (cosmui.rs)
    cosm: cosmui::Page,
    swap_drag: Option<swapui::Drag>,
    swap_grab: [f32; 2],
    swap_drop_at: usize,
    swap_track: [f32; 2],
    /// the tracks of a swap's own delays (swapui.rs swap_tslider)
    swap_ttrack: [[f32; 2]; 5],
    swap_need: f32,
    swap_arm: Option<f32>,
    /// when another swap (or the templates) came into the editor
    swap_shown_at: f32,
    /// the list's chips as drawn last (a quick flick is let go before a frame draws it lifted), the lifted one (where,
    /// its thing's place among the icons: those before it under it are not drawn)
    swap_chips: Vec<[f32; 4]>,
    swap_lift: Option<([f32; 4], usize)>,
    /// where the word over the menu is (nothing of Auto Swap's is drawn over it)
    toast_rect: [f32; 4],
    pop: Option<Pop>,
    text_focus: Option<(Owner, usize)>,
    /// which settings `rows` draws (a search's: None all of them)
    rows_mask: Option<Vec<bool>>,
    heat: HashMap<usize, f32>,
    player: String,
    dir: Option<std::path::PathBuf>,
    loaded: bool,
    /// Development: the script it plays on itself (dev.rs), and when the window last opened.
    dev: Option<dev::Script>,
    opened_at: f32,
    /// How the menu looks (Theme), the colour picker, the HUD (its elements are modules of Cat::Hud; where each sits,
    /// what happens in the game for it, its editor), and the hits, pointer and hover of whichever is being drawn.
    theme: Vec<Setting>,
    pick: Option<Picker>,
    pub hud: hud::Hud,
    /// The HUD's editor asked to be opened from the menu (Java opens it).
    want_editor: bool,
    /// Development: the HUD's script (CURSED_DEV_HUD).
    hud_dev: Option<dev::Script>,
    /// The shader's numbers for the window (render.rs): rect, open, line, heat, line mode, veil, rail, cursor light.
    pub look: Look,
    /// How the window is drawn this frame against its layout: its middle, scale, drop, alpha (Java's head follows).
    xf: [f32; 5],
    /// A section, its part or a filter changed: when, and what the right side showed (it fades out as the new rises,
    /// the side staying as it is): its tiles, and its header (with the section only).
    swap_at: f32,
    swap_from: Vec<f32>,
    tiles_last: Vec<f32>,
    /// the layout's settings the window glides to (Theme: size, width, height, text)
    lay_to: [f32; 4],
    head_at: f32,
    head_from: Vec<f32>,
    head_last: Vec<f32>,
}

#[derive(Clone, Copy, Default)]
pub struct Look {
    pub win: [f32; 4],
    pub open: f32,
    pub line: f32,
    pub heat: f32,
    pub line_mode: f32,
    pub veil: f32,
    pub rail: f32,
    pub radius: f32,
    pub cursor: [f32; 3],
    pub time: f32,
    pub blur: f32,
    /// the whole's fade; the glass's tint (and how much of it); the dim's colour (and how much); the world's colour
    /// through it, the rail's shade, sheen, shadow; the burning line's colour (and the cursor's light)
    pub fade: f32,
    pub glass: Rgba,
    pub veil_col: Rgba,
    pub theme: [f32; 4],
    pub fire: [f32; 4],
    /// the glass opening's: the window's alpha, how frosted it is (0 the world sharp), the light crossing it (below 0
    /// none)
    pub anim: [f32; 4],
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let k = ((x - a) / (b - a)).clamp(0.0, 1.0);
    k * k * (3.0 - 2.0 * k)
}
fn ease_out(k: f32) -> f32 {
    1.0 - (1.0 - k.clamp(0.0, 1.0)).powi(3)
}
fn inside(r: [f32; 4], p: [f32; 2]) -> bool {
    p[0] >= r[0] && p[0] < r[2] && p[1] >= r[1] && p[1] < r[3]
}
fn ak(tag: u64, i: usize) -> u64 {
    (tag << 32) | i as u64
}

const A_IND: u64 = 1;
const A_TX: u64 = 2;
const A_TY: u64 = 3;
const A_TH: u64 = 4;
const A_TW: u64 = 5;
const A_HOV: u64 = 6;
const A_SW: u64 = 7;
const A_SEG: u64 = 8;
const A_SEGW: u64 = 9;
const A_SET: u64 = 10;
const A_POP: u64 = 12;
const A_ARR: u64 = 13;
const A_VIS: u64 = 14;
const A_LAY: u64 = 15;
const A_DRAIN: u64 = 16;
const A_BVAL: u64 = 17;
/// The theme's settings that lay the menu out (they change it only once let go).
const LAYOUT: [&str; 4] = ["Menu size", "Window width", "Window height", "Text size"];

impl Gui {
    fn new() -> Gui {
        let mut mods = data::modules();
        mods.extend(data::hud_modules());
        Gui {
            mods,
            client: data::client_settings(),
            saved: Vec::new(),
            loaded_cfg: None,
            cfg_parts: configs::P_ALL,
            cfg_card_need: 0.0,
            paste_for: 0,
            paste_module: None,
            card_arm: false,
            section_arm: None,
            reset_arm: None,
            found_memo: Default::default(),
            flash: None,
            cfg_host: String::new(),
            profile: None,
            interface_profile: None,
            cfg_interface_context: None,
            tour: None,
            // (the stand shows it: CURSED_DEV_TOUR)
            tour_due: std::env::var_os("CURSED_DEV_TOUR").is_some(),
            toast: None,
            cat: Cat::Player,
            filter: [0, 0, 0, 0],
            settings_tab: 0,
            q: String::new(),
            typing: false,
            open: None,
            open_at: 0.0,
            scroll: 0.0,
            scroll_to: 0.0,
            content: 0.0,
            w: 1.0,
            h: 1.0,
            r: 16.0,
            win: [0.0; 4],
            dx: 0.0,
            dy: 0.0,
            drag: None,
            slider: None,
            mouse: [-1.0, -1.0],
            hover: Hit::None,
            tick_at: 0.0,
            phase: Phase::Closed,
            phase_at: 0.0,
            shown_at: -1.0,
            now: 0.0,
            clock: Instant::now(),
            last: 0.0,
            dt: 0.016,
            anim: HashMap::new(),
            hits: Vec::new(),
            listen: None,
            gheld: Vec::new(),
            undo: Default::default(),
            rec: Vec::new(),
            eat: None,
            play: Default::default(),
            play_ui: Default::default(),
            shulk: Default::default(),
            vm_seen: usize::MAX,
            swing_port: Default::default(),
            swing_ed: Default::default(),
            fl: Default::default(),
            adapt: Default::default(),
            streamer: Default::default(),
            killfx: Default::default(),
            tags: Default::default(),
            fakes: Default::default(),
            cosm_cols: vec![
                Setting::new("Material colour", Kind::Colour(data::Col::new(0xffffff))),
                Setting::new("Crosshair colour", Kind::Colour(data::Col::new(0x4bff4b))),
            ],
            wheld: Vec::new(),
            holds: Vec::new(),
            bind_back: Vec::new(),
            card: None,
            card_at: 0.0,
            card_shut: None,
            liquid_seen: 0,
            timing_seen: 0,
            binds_page: false,
            words_page: false,
            word_q: String::new(),
            word_focus: None,
            word_buf: String::new(),
            hud_sel: 0,
            langs: Vec::new(),
            lang_sel: String::new(),
            card_field: String::new(),
            card_scroll: 0.0,
            card_scroll_to: 0.0,
            card_rect: [0.0; 4],
            cloud_local: Vec::new(),
            cloud_codes: Vec::new(),
            cloud_revoke: None,
            cloud_recovery_blocked: false,
            cloud_seen_version: 0,
            last_text: None,
            outbox: Vec::new(),
            swaps: Vec::new(),
            swap_ev: Vec::new(),
            swap_held: Vec::new(),
            server: String::new(),
            swap_sel: 0,
            swap_tpl: false,
            swap_inv: swapui::Inv::default(),
            swap_icons: Vec::new(),
            pearl_ev: 0,
            alt_armed: None,
            alt_wrong: None,
            alt_cursed: 0,
            alt_faces: Vec::new(),
            swap_drag: None,
            swap_grab: [0.0; 2],
            swap_drop_at: 0,
            swap_track: [0.0, 1.0],
            swap_ttrack: [[0.0, 1.0]; 5],
            swap_need: 0.0,
            swap_arm: None,
            swap_shown_at: -10.0,
            cosm: cosmui::Page::default(),
            swap_chips: Vec::new(),
            swap_lift: None,
            toast_rect: [0.0; 4],
            pop: None,
            text_focus: None,
            rows_mask: None,
            heat: HashMap::new(),
            player: "Player".into(),
            dir: None,
            loaded: false,
            dev: dev::Script::from_env(),
            opened_at: 0.0,
            look: Look::default(),
            theme: data::theme_settings(),
            pick: None,
            hud: hud::Hud::new(),
            want_editor: false,
            hud_dev: dev::Script::hud_from_env(),
            xf: [0.0, 0.0, 1.0, 0.0, 1.0],
            swap_at: -10.0,
            swap_from: Vec::new(),
            tiles_last: Vec::new(),
            lay_to: [1.0; 4],
            head_at: -10.0,
            head_from: Vec::new(),
            head_last: Vec::new(),
        }
    }

    // ------------------------------------------------------------------ motion
    /// A value that eases toward `target` (`speed` per second, exponential); first seen it starts at `start`.
    /// An animation put at `v` at once (it eases from there).
    fn an_set(&mut self, k: u64, v: f32) -> f32 {
        self.anim.insert(k, v);
        v
    }

    fn an(&mut self, k: u64, target: f32, speed: f32, start: f32) -> f32 {
        let p = pal();
        let dt = if p.anim { self.dt * p.speed } else { 10.0 };
        let v = self.anim.entry(k).or_insert(start);
        *v += (target - *v) * (1.0 - (-dt * speed).exp());
        if (target - *v).abs() < 0.0005 {
            *v = target;
        }
        *v
    }

    /// Seconds since `on` became true (-1 while it is not; -1 too with the animations off).
    fn since(&mut self, k: u64, on: bool) -> f32 {
        let anim = pal().anim;
        let dt = self.dt * pal().speed;
        let v = self.anim.entry(k).or_insert(-1.0);
        *v = if !on || !anim { -1.0 } else if *v < 0.0 { 0.0 } else { *v + dt };
        *v
    }

    /// How far an element `d` seconds into the window's rising has risen: (seen, px down).
    fn rise(&self, d: f32) -> (f32, f32) {
        match self.phase {
            Phase::Closing => {
                let k = 1.0 - smooth(0.0, 0.16, self.now - self.phase_at);
                (k, (1.0 - k) * 0.3 * self.r)
            }
            Phase::Closed => (0.0, 0.0),
            _ => {
                if self.shown_at < 0.0 {
                    return (0.0, 0.5 * self.r);
                }
                let p = pal();
                if !p.anim {
                    return (1.0, 0.0);
                }
                let k = ((self.now - self.shown_at - d / p.speed) * p.speed / 0.5).clamp(0.0, 1.0);
                let e = ease_out(k);
                (smooth(0.0, 0.8, k), (1.0 - e) * 0.5 * self.r)
            }
        }
    }

    /// The right side's rising: with the window's, and again (briefly, gently, `s` in for a tile further down) when
    /// what it shows changes; `head`: the title and the tabs over the tiles, which change with the section only.
    fn rise_c(&self, d: f32, s: f32, head: bool) -> (f32, f32) {
        let (a, dy) = self.rise(d);
        let p = pal();
        let at = if head { self.head_at } else { self.swap_at };
        if !p.anim {
            return (a, dy);
        }
        // a beat after the old begins to go, quick at first and settling
        let k = ((self.now - at - 0.03 - s / p.speed) * p.speed / 0.24).clamp(0.0, 1.0);
        if k >= 1.0 {
            return (a, dy);
        }
        (a * (1.0 - (1.0 - k) * (1.0 - k)), dy + (1.0 - ease_out(k)) * 0.45 * self.r)
    }

    /// What the right side showed before it changed, fading out and drifting up a little as the new rises.
    fn swap_ghost(&self, c: &mut Canvas, head: bool) {
        let (at, from) = if head { (self.head_at, &self.head_from) } else { (self.swap_at, &self.swap_from) };
        let p = pal();
        let k = if p.anim { ((self.now - at) * p.speed / 0.15).clamp(0.0, 1.0) } else { 1.0 };
        if k >= 1.0 || from.is_empty() {
            return;
        }
        let start = c.p.len() / draw::FLOATS;
        c.p.extend_from_slice(from);
        c.scale_about(start, [0.0, 0.0], 1.0, [0.0, -ease_out(k) * 0.3 * self.r]);
        c.fade_from(start, (1.0 - k) * (1.0 - k));
    }

    /// The right side is to show something else: the tiles (and with a section, the header) cross over.
    fn swap(&mut self, section: bool) {
        self.swap_from = self.tiles_last.clone();
        self.swap_at = self.now;
        if section {
            self.head_from = self.head_last.clone();
            self.head_at = self.now;
        }
    }

    fn sound_tick(&mut self, k: f32) {
        if self.now - self.tick_at < 0.045 || !self.sounds() {
            return;
        }
        self.tick_at = self.now;
        crate::launch::sound::play(crate::launch::sound::Sfx::Tick, 3.2 * k * self.volume(), 0.0);
    }

    /// A switch: on, the client's crackle of fire; off, the flame snuffed. Every switch of the menu and the HUD sounds
    /// so (a module's at its full, a setting's softer).
    fn switch_sound(&mut self, on: bool, k: f32) {
        if !self.sounds() {
            return;
        }
        use crate::launch::sound::{play, Sfx};
        let v = self.volume() * k;
        if on {
            for d in 0..3 {
                play(Sfx::Crackle, 0.35 * v * if d == 0 { 1.0 } else { 0.7 }, d as f32 * 0.035);
            }
        } else {
            play(Sfx::Snuff, 0.55 * v, 0.0);
        }
    }

    fn sounds(&self) -> bool {
        matches!(self.client.iter().find(|s| s.label == "Sounds").map(|s| &s.kind), Some(Kind::Toggle(true)))
    }

    fn volume(&self) -> f32 {
        match self.client.iter().find(|s| s.label == "Volume").map(|s| &s.kind) {
            Some(Kind::Slider { v, .. }) => *v / 70.0,
            _ => 1.0,
        }
    }

    fn previews(&self) -> bool {
        matches!(self.client.iter().find(|s| s.label == "Previews").map(|s| &s.kind), Some(Kind::Toggle(true)))
    }

    fn set_list(&mut self, o: Owner) -> &mut Vec<Setting> {
        match o {
            Owner::Module(i) => &mut self.mods[i].set,
            Owner::Client => &mut self.client,
            Owner::Theme => &mut self.theme,
            Owner::Cosm => &mut self.cosm_cols,
        }
    }

    fn set_ref(&self, o: Owner) -> &Vec<Setting> {
        match o {
            Owner::Module(i) => &self.mods[i].set,
            Owner::Client => &self.client,
            Owner::Theme => &self.theme,
            Owner::Cosm => &self.cosm_cols,
        }
    }

    // ------------------------------------------------------------------ the frame
    fn frame(&mut self, w: f32, h: f32) -> Canvas {
        cloud::start();
        self.fakes.preview = None;
        self.liquid_follow();
        self.timing_follow();
        self.vm_follow();
        let t = self.clock.elapsed().as_secs_f32();
        self.dt = (t - self.last).clamp(0.0, 0.1);
        self.last = t;
        self.now = t;
        self.dev_run();
        self.undo_tick();
        self.tour_due_check();
        self.picked_in();
        self.swap_icons.clear();
        self.alt_faces.clear();
        self.swap_lift = None;
        let mut p = theme::Palette::of(&self.theme, t);
        // the menu's size, its window's and its text's glide to what they are set to; while one of them is dragged they
        // wait for it to be let go (the window resizing under the pointer moved the slider away from it: it jumped)
        let held = matches!(self.slider, Some((Owner::Theme, si)) if self.theme.get(si).is_some_and(|s| LAYOUT.contains(&s.label)));
        if !held {
            self.lay_to = [p.size, p.ww, p.wh, p.text];
        }
        let lay: [f32; 4] = std::array::from_fn(|i| self.an(ak(A_LAY, i), self.lay_to[i], 9.0, self.lay_to[i]));
        [p.size, p.ww, p.wh, p.text] = lay;
        theme::set_pal(p);
        text::set_scale(p.text);
        self.w = w.max(1.0);
        self.h = h.max(1.0);
        // no bigger than the screen holds the window at (it was cut and its side ran over itself)
        self.r = (((w / 1920.0).min(h / 1080.0)).clamp(0.62, 1.6) * 16.0 * p.size).min(h / (47.0 * p.wh + 5.5)).min(w / (76.0 * p.ww + 2.5));
        let r = self.r;
        // the opening, as the theme says: the glass (the window comes up a little and grows to its size, the world
        // behind it frosts over, a light crosses it once), a fade, or at once; at the theme's speed
        let style = if p.anim { p.opening } else { 3 };
        let e = (self.now - self.phase_at) * p.speed;
        let (mut k, line, heat, mut veil, mode, mut fade) = (0.0, 0.0, 0.0, 0.0, 0.0, 1.0);
        // the glass's own: the window's alpha, how frosted, the light crossing it (where, 0..1; below 0 none), its size,
        // how far below its place
        let (mut wa, mut frost, mut sweep, mut scale, mut drop) = (1.0, 1.0, -1.0, 1.0, 0.0);
        match self.phase {
            Phase::Opening if style == 0 => {
                let q = (e / 0.5).min(1.0);
                k = 1.0;
                wa = smooth(0.0, 0.42, q);
                scale = 0.955 + 0.045 * (1.0 - (1.0 - q).powi(4));
                drop = (1.0 - ease_out(q)) * 1.1 * r;
                frost = smooth(0.05, 0.8, q);
                veil = smooth(0.0, 0.7, q);
                if p.sheen {
                    sweep = -0.3 + 1.6 * smooth(0.18, 1.0, q);
                }
                if q >= 0.12 && self.shown_at < 0.0 {
                    self.shown_at = self.now;
                }
                if q >= 1.0 {
                    self.phase = Phase::Open;
                }
            }
            Phase::Opening => {
                let q = if style == 2 { e / 0.32 } else { 1.0 };
                k = 1.0;
                veil = smooth(0.0, 1.0, q);
                fade = smooth(0.0, 1.0, q);
                if self.shown_at < 0.0 {
                    self.shown_at = self.now - if style == 3 { 10.0 } else { 0.0 };
                }
                if q >= 1.0 {
                    self.phase = Phase::Open;
                }
            }
            Phase::Open => {
                k = 1.0;
                veil = 1.0;
            }
            Phase::Closing if style == 0 => {
                let q = (e / 0.24).min(1.0);
                let f = smooth(0.0, 1.0, q);
                k = 1.0;
                wa = 1.0 - f;
                scale = 1.0 - 0.035 * f;
                drop = f * 0.5 * r;
                frost = 1.0 - f;
                veil = 1.0 - f;
                if q >= 1.0 {
                    self.phase = Phase::Closed;
                    self.save();
                }
            }
            Phase::Closing => {
                let q = if style == 2 { e / 0.22 } else { 1.0 };
                k = 1.0;
                veil = 1.0 - smooth(0.0, 1.0, q);
                fade = 1.0 - smooth(0.0, 1.0, q);
                if q >= 1.0 {
                    self.phase = Phase::Closed;
                    self.save();
                }
            }
            Phase::Closed => {}
        }
        // the window: centred, moved by its header (or always back in the middle)
        let (ww, wh) = ((76.0 * r * p.ww).min(w - 2.0 * r), (47.0 * r * p.wh).min(h - 4.6 * r));
        if !p.keep && self.drag.is_none() {
            self.dx = 0.0;
            self.dy = 0.0;
        }
        let x = ((w - ww) / 2.0 + self.dx).clamp(8.0, (w - ww - 8.0).max(8.0));
        let y = ((h - wh) / 2.0 - 0.9 * r + self.dy).clamp(8.0, (h - wh - 8.0).max(8.0));
        self.win = [x, y, ww, wh];
        let rail = 15.0 * r;
        // the window as drawn this frame (the glass's opening scales and lifts it; its layout stays where it will be)
        let mid = [x + ww / 2.0, y + wh / 2.0];
        let shown = [mid[0] - ww * scale / 2.0, mid[1] + drop - wh * scale / 2.0, ww * scale, wh * scale];
        self.xf = [mid[0], mid[1], scale, drop, wa];
        // the world round the window as grey as the theme says; in Interface its colour (and the HUD over it) comes back
        let drain_to = if self.cat == Cat::Hud { 0.0 } else { 1.0 - p.around };
        let drain = self.an(ak(A_DRAIN, 0), drain_to, 5.0, drain_to);
        self.look = Look {
            win: shown,
            open: k,
            line,
            heat,
            line_mode: mode,
            veil,
            rail: rail * scale,
            radius: 1.25 * r * p.radius * scale,
            cursor: [self.mouse[0], self.mouse[1], k],
            time: self.now,
            blur: p.blur,
            fade,
            glass: p.glass,
            veil_col: p.veil,
            theme: [p.world, p.rail, p.sheen as i32 as f32, p.shadow as i32 as f32],
            fire: [1.0, 1.0, 1.0, p.cursor as i32 as f32],
            anim: [wa, frost, sweep, drain],
        };
        self.scroll += (self.scroll_to - self.scroll) * (1.0 - (-self.dt * 14.0).exp());
        let mut c = Canvas::new(w, h);
        self.hits.clear();
        if k > 0.0 {
            // the window unfolds from its middle: what is in it, cut to what is open
            let hh = wh * 0.5 * k;
            let cy = y + wh / 2.0;
            c.push_clip([x, cy - hh, x + ww, cy + hh]);
            self.hits.push(([x, y, x + ww, y + wh], Hit::Window));
            self.hits.push(([x, y, x + ww, y + 4.9 * r], Hit::Header));
            self.side(&mut c, x, y, wh);
            self.main(&mut c, x + rail, y, ww - rail, wh);
            c.pop_clip();
            self.popover(&mut c);
            self.picker(&mut c);
            self.card(&mut c);
            self.toast_draw(&mut c);
            self.tour_draw(&mut c);
        }
        if scale != 1.0 || drop != 0.0 || wa < 1.0 {
            c.scale_about(0, mid, scale, [0.0, drop]);
            c.fade_from(0, wa);
        }
        text::set_scale(1.0);
        c
    }

    // ------------------------------------------------------------------ left: brand, search, sections, account
    fn side(&mut self, c: &mut Canvas, x: f32, y: f32, wh: f32) {
        let r = self.r;
        let (a, dyv) = self.rise(0.05);
        c.alpha = a;
        let lx = x + 1.55 * r;
        c.glow([lx + 1.02 * r, y + 2.5 * r + dyv], 1.6 * r, grey(1.0, 0.12));
        c.logo([lx, y + 1.45 * r + dyv, lx + 2.05 * r, y + 3.5 * r + dyv], pal().accent);
        c.text(lx + 2.8 * r, y + 2.45 * r + dyv, 1.08 * r, Face::Semi, "Recode", pal().t1);
        c.text(lx + 2.8 * r, y + 3.25 * r + dyv, 0.64 * r, Face::Mono, "alpha 0.1 · 26.3", pal().t4);
        // search
        let (a, dyv) = self.rise(0.1);
        c.alpha = a;
        let sr = [x + r, y + 4.9 * r + dyv, x + 11.55 * r, y + 7.3 * r + dyv];
        // every bind on one page: the key icon by the search
        let bb = [x + 11.95 * r, sr[1], x + 14.0 * r, sr[3]];
        let bv = self.an(ak(A_HOV, 901), if self.binds_page || self.hover == Hit::BindsBtn { 1.0 } else { 0.0 }, 12.0, 0.0);
        let typing = self.since(ak(A_HOV, 904), self.hover == Hit::BindsBtn);
        c.rect(bb, 0.75 * r, mix(pal().f1, pal().f3, bv));
        cards::key_icon(c, [(bb[0] + bb[2]) / 2.0, (bb[1] + bb[3]) / 2.0], 1.15 * r, typing, mix(pal().t4, pal().t1, bv));
        self.hits.push((bb, Hit::BindsBtn));
        let hv = self.an(ak(A_HOV, 900), if self.typing || self.hover == Hit::Search { 1.0 } else { 0.0 }, 12.0, 0.0);
        c.rect(sr, 0.75 * r, mix(pal().f1, pal().f2, hv));
        let ic = [sr[0] + 1.25 * r, sr[1] + 1.15 * r];
        let col = mix(pal().t4, pal().t2, hv);
        c.ring([ic[0] - 0.1 * r, ic[1] - 0.1 * r], 0.36 * r, 1.4, col);
        c.line([ic[0] + 0.17 * r, ic[1] + 0.17 * r], [ic[0] + 0.45 * r, ic[1] + 0.45 * r], 1.4, col, 0.0);
        let base = sr[1] + 1.48 * r;
        if self.q.is_empty() && !self.typing {
            c.text(sr[0] + 2.05 * r, base, 0.82 * r, Face::Regular, tr("Search"), pal().t4);
        } else {
            let wq = c.text(sr[0] + 2.05 * r, base, 0.82 * r, Face::Regular, &self.q, pal().t1);
            if self.typing && (self.now * 2.0).fract() < 0.55 {
                c.rect([sr[0] + 2.1 * r + wq, sr[1] + 0.62 * r, sr[0] + 2.1 * r + wq + 1.5, sr[3] - 0.62 * r], 0.0, pal().accent);
            }
        }
        let kw = fonts().width(Face::Mono, 0.62 * r, "/");
        c.rect([sr[2] - 0.6 * r - kw - 0.76 * r, sr[1] + 0.68 * r, sr[2] - 0.6 * r, sr[3] - 0.68 * r], 0.32 * r, pal().f2);
        c.text(sr[2] - 0.98 * r - kw, base - 0.05 * r, 0.62 * r, Face::Mono, "/", pal().t4);
        self.hits.push((sr, Hit::Search));
        // sections: the modules', the client's; one mark of the open one, gliding from one to the other across both
        let mut yy = y + 8.45 * r;
        let mods_y = yy + 1.35 * r;
        let client_y = mods_y + MODULE_CATS.len() as f32 * 2.72 * r + 0.6 * r + 1.35 * r;
        let ind_y = MODULE_CATS.iter().position(|&k| k == self.cat).map(|i| mods_y + i as f32 * 2.72 * r)
            .or_else(|| CLIENT_CATS.iter().position(|&k| k == self.cat).map(|i| client_y + i as f32 * 2.72 * r));
        self.nav_mark(c, x, ind_y);
        let (a, dyv) = self.rise(0.14);
        c.alpha = a;
        c.text(x + 1.8 * r, yy + 0.75 * r + dyv, 0.7 * r, Face::Semi, tr("Modules"), pal().t4);
        yy += 1.35 * r;
        self.nav_group(c, x, yy, &MODULE_CATS, 0.17);
        yy += MODULE_CATS.len() as f32 * 2.72 * r + 0.6 * r;
        let (a, dyv) = self.rise(0.3);
        c.alpha = a;
        c.text(x + 1.8 * r, yy + 0.75 * r + dyv, 0.7 * r, Face::Semi, tr("Client"), pal().t4);
        yy += 1.35 * r;
        self.nav_group(c, x, yy, &CLIENT_CATS, 0.33);
        // the account (Java draws the head here)
        let (a, dyv) = self.rise(0.42);
        c.alpha = a;
        let by = y + wh - 3.4 * r + dyv;
        let ar = [x + r, by - 0.15 * r, x + 14.0 * r - (fonts().width(Face::Mono, 0.66 * r, "RU") + 2.6 * r) - 0.4 * r, by + 2.65 * r];
        let av = self.an(ak(A_HOV, 903), if self.hover == Hit::AltBtn || matches!(self.card, Some(Card::Alts { .. })) { 1.0 } else { 0.0 }, 12.0, 0.0);
        c.rect(ar, 0.75 * r, grey(1.0, 0.06 * av));
        self.hits.push((ar, Hit::AltBtn));
        let name = self.player.clone();
        c.text(x + 4.35 * r, by + 0.95 * r, 0.86 * r, Face::Semi, &name, pal().t1);
        let cfg = format!("{} · {}", tr("config"), self.loaded_cfg.clone().unwrap_or_else(|| tr("your own").to_string()));
        // the language's chip is at the row's right: the line stops short of it
        let room = 14.0 * r - 3.6 * r - (fonts().width(Face::Mono, 0.66 * r, "RU") + 2.6 * r) - 0.6 * r;
        let cfg = fonts().fit(Face::Mono, 0.64 * r, &cfg, room);
        c.text(x + 4.35 * r, by + 1.75 * r, 0.64 * r, Face::Mono, &cfg, pal().t4);
        // the language: its code, the card of every language
        let code = if self.lang_sel.is_empty() { crate::menus::lang::code() } else { self.lang_sel.clone() };
        let short = code.split('_').next().unwrap_or("en").to_uppercase();
        let lw = fonts().width(Face::Mono, 0.66 * r, &short) + 2.6 * r;
        let lb = [x + 14.0 * r - lw, by + 0.2 * r, x + 14.0 * r, by + 2.3 * r];
        let lv = self.an(ak(A_HOV, 902), if self.hover == Hit::LangBtn || matches!(self.card, Some(Card::Lang { .. })) { 1.0 } else { 0.0 }, 12.0, 0.0);
        c.rect(lb, 0.65 * r, mix(pal().f1, pal().f3, lv));
        globe(c, [lb[0] + 1.05 * r, (lb[1] + lb[3]) / 2.0], 0.42 * r, mix(pal().t3, pal().t1, lv));
        c.text(lb[0] + 1.75 * r, (lb[1] + lb[3]) / 2.0 + 0.24 * r, 0.66 * r, Face::Mono, &short, mix(pal().t2, pal().t1, lv));
        self.hits.push((lb, Hit::LangBtn));
        c.alpha = 1.0;
    }

    /// The open section's mark: it glides to the section from wherever it was, the modules' or the client's (its edge
    /// ahead quick, the one behind following: it stretches on the way and gathers again), and fades while a search or
    /// the binds show instead of a section.
    fn nav_mark(&mut self, c: &mut Canvas, x: f32, ind_y: Option<f32>) {
        let r = self.r;
        let iw = 13.0 * r;
        let ih = 2.6 * r;
        let searching = !self.q.is_empty() || self.binds_page;
        let shown = self.an(ak(A_IND, 2), if ind_y.is_some() && !searching { 1.0 } else { 0.0 }, 12.0, 1.0);
        let Some(iy) = ind_y.or_else(|| self.anim.get(&ak(A_IND, 0)).copied()) else {
            return;
        };
        let top_now = self.anim.get(&ak(A_IND, 0)).copied().unwrap_or(iy);
        let up = iy < top_now - 0.5;
        let down = iy > top_now + 0.5;
        let top = self.an(ak(A_IND, 0), iy, if up { 20.0 } else { 11.0 }, iy);
        let bottom = self.an(ak(A_IND, 1), iy + ih, if down { 20.0 } else { 11.0 }, iy + ih);
        if shown <= 0.001 {
            return;
        }
        let (a, dyv) = self.rise(0.17);
        c.alpha = a * shown;
        let rr = [x + r, top + dyv, x + r + iw, bottom.max(top + ih * 0.6) + dyv];
        c.rect_lit(rr, 0.75 * r, grey(1.0, 0.11), grey(1.0, 0.035), 1.0);
        c.line([rr[0] + 0.33 * r, rr[1] + 0.82 * r], [rr[0] + 0.33 * r, rr[3] - 0.82 * r], 2.0, pal().accent, 5.0);
        c.alpha = 1.0;
    }

    fn nav_group(&mut self, c: &mut Canvas, x: f32, y0: f32, cats: &[Cat], d0: f32) {
        let r = self.r;
        let iw = 13.0 * r;
        let ih = 2.6 * r;
        let searching = !self.q.is_empty() || self.binds_page;
        for (i, &cat) in cats.iter().enumerate() {
            let (a, dyv) = self.rise(d0 + i as f32 * 0.035);
            c.alpha = a;
            let ry = y0 + i as f32 * 2.72 * r + dyv;
            let rr = [x + r, ry, x + r + iw, ry + ih];
            let act = cat == self.cat && !searching;
            let hov = self.hover == Hit::Nav(cat);
            let hv = self.an(ak(A_HOV, 100 + cat as usize), if hov { 1.0 } else { 0.0 }, 12.0, 0.0);
            let col = if act { pal().t1 } else { mix(pal().t3, pal().t2, hv) };
            icon(c, cat, [rr[0] + 0.95 * r, ry + 0.74 * r], 1.12 * r, col, hv, self.now, act);
            c.text(rr[0] + 2.9 * r, ry + 1.6 * r, 0.875 * r, Face::Medium, tr(cat.name()), col);
            let n = self.count(cat);
            if let Some(n) = n {
                c.text_r(rr[2] - 0.8 * r, ry + 1.58 * r, 0.66 * r, Face::Mono, &n.to_string(), if act { pal().t3 } else { pal().t5 });
            }
            self.hits.push((rr, Hit::Nav(cat)));
        }
    }

    fn count(&self, cat: Cat) -> Option<usize> {
        match cat {
            Cat::Graphics => Some(ENGINE_KEYS.iter().filter(|k| self.engine(k)).count()),
            Cat::Configs => Some(self.saved.len()),
            Cat::Forum | Cat::Cosmetics | Cat::Music | Cat::Settings | Cat::Theme => None,
            _ => Some(self.mods.iter().filter(|m| m.cat == cat && m.on && !m.plain).count()),
        }
    }

    fn engine(&self, name: &str) -> bool {
        self.mods.iter().filter(|m| m.cat == Cat::Graphics).flat_map(|m| m.set.iter()).any(|s| s.label == name && matches!(s.kind, Kind::Toggle(true)))
    }

    // ------------------------------------------------------------------ right: header, tiles
    fn list(&self) -> Vec<usize> {
        if !self.q.is_empty() {
            return self.found().into_iter().filter(|f| !self.hidden_here(self.mods[f.i].id)).map(|f| f.i).collect();
        }
        let f = match self.cat {
            Cat::Player => self.filter[0],
            Cat::Misc => self.filter[1],
            Cat::Render => self.filter[2],
            Cat::World => self.filter[3],
            _ => 0,
        };
        let cat = self.shown_cat();
        (0..self.mods.len())
            .filter(|&i| {
                let m = &self.mods[i];
                m.cat == cat && (f == 0 || (f == 1) == m.on) && !self.hidden_here(m.id)
            })
            .collect()
    }

    /// The modules' category the section shows: its own, or (Settings' Graphics) the engine's parts.
    fn shown_cat(&self) -> Cat {
        if self.cat == Cat::Settings && self.settings_tab == 1 {
            Cat::Graphics
        } else {
            self.cat
        }
    }

    fn main(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, wh: f32) {
        let r = self.r;
        self.swap_ghost(c, true);
        let h0 = c.p.len() / draw::FLOATS;
        let (a, dyv) = self.rise_c(0.12, 0.0, true);
        c.alpha = a;
        let title = if !self.q.is_empty() { tr("Search").to_string() } else if self.binds_page { tr("Binds").to_string() } else if self.words_page { tr("Words").to_string() } else { tr(self.cat.name()).to_string() };
        c.text(x + 1.7 * r, y + 3.1 * r + dyv, 1.62 * r, Face::Bold, &title, pal().t1);
        let line = if !self.q.is_empty() {
            i18n::trn("{n} found for “{q}”", &[("n", self.list().len().to_string()), ("q", self.q.clone())])
        } else if self.words_page {
            tr("Any word of the menu and the HUD, as you like it; the round arrow puts it back").to_string()
        } else if self.binds_page {
            let all = self.all_binds();
            let n: usize = all.iter().map(|a| a.1.len()).sum();
            i18n::trn("{n} binds on {m} things · the right button on a setting binds it, the middle one on a module", &[("n", n.to_string()), ("m", all.len().to_string())])
        } else {
            match self.cat {
                Cat::Configs => i18n::trn("{n} configs on this computer", &[("n", self.saved.len().to_string())]),
                Cat::Settings if self.settings_tab == 1 => i18n::trn("{n} of {m} parts of the engine on", &[("n", self.count(Cat::Graphics).unwrap_or(0).to_string()), ("m", ENGINE_KEYS.len().to_string())]),
                Cat::Settings => tr("How the menu opens and sounds").into(),
                Cat::Theme => tr("The look of the whole client: the menu, the HUD, the chat, the inventories").into(),
                Cat::Cosmetics => tr("Knives, gloves and rifles of Counter-Strike 2 in your hands").into(),
                Cat::Forum | Cat::Music => tr("In a later build").into(),
                cat => i18n::trn("{n} of {m} on", &[("n", self.count(cat).unwrap_or(0).to_string()), ("m", self.mods.iter().filter(|m| m.cat == cat && !m.plain).count().to_string())]),
            }
        };
        c.text(x + 1.7 * r, y + 4.3 * r + dyv, 0.8 * r, Face::Regular, &line, pal().t3);
        // the sub-sections or the filter
        let (a, dyv) = self.rise_c(0.16, 0.0, true);
        c.alpha = a;
        if self.q.is_empty() && !self.binds_page && !self.words_page {
            let (opts, cur): (Vec<&str>, usize) = match self.cat {
                Cat::Render => (vec!["All", "On", "Off"], self.filter[2]),
                Cat::Player => (vec!["All", "On", "Off"], self.filter[0]),
                Cat::Misc => (vec!["All", "On", "Off"], self.filter[1]),
                Cat::World => (vec!["All", "On", "Off"], self.filter[3]),
                Cat::Settings => (vec!["Menu", "Graphics"], self.settings_tab),
                _ => (vec![], 0),
            };
            if self.cat == Cat::Hud {
                let label = tr("Edit the layout");
                let bw = fonts().width(Face::Semi, 0.8 * r, label) + 2.4 * r;
                let b = [x + w - 1.7 * r - bw, y + 2.0 * r + dyv, x + w - 1.7 * r, y + 4.3 * r + dyv];
                let hv = self.an(ak(A_HOV, 310), if self.hover == Hit::EditLayout { 1.0 } else { 0.0 }, 12.0, 0.0);
                c.glow([(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0], bw * 0.6, theme::fade(pal().accent, 0.08 + 0.08 * hv));
                c.rect(b, 0.7 * r, mix(theme::fade(pal().accent, 0.9), pal().accent, hv));
                c.text_c((b[0] + b[2]) / 2.0, b[1] + 1.45 * r, 0.8 * r, Face::Semi, label, grey(0.05, 1.0));
                self.hits.push((b, Hit::EditLayout));
            }
            if !opts.is_empty() {
                let size = 0.78 * r;
                let pad = 0.78 * r;
                let ws: Vec<f32> = opts.iter().map(|o| fonts().width(Face::Medium, size, tr(o)) + 2.0 * pad).collect();
                let total: f32 = ws.iter().sum::<f32>() + 0.44 * r;
                let sx = x + w - 1.7 * r - total;
                let sy = y + 2.0 * r + dyv;
                let sh = 2.3 * r;
                c.rect([sx, sy, sx + total, sy + sh], 0.7 * r, pal().f1);
                let mut ox = sx + 0.22 * r;
                let cx: f32 = ws[..cur].iter().sum::<f32>() + sx + 0.22 * r;
                let tx = self.an(ak(A_SEG, 0), cx, 16.0, cx);
                let tw = self.an(ak(A_SEGW, 0), ws[cur], 16.0, ws[cur]);
                c.rect([tx, sy + 0.22 * r, tx + tw, sy + sh - 0.22 * r], 0.5 * r, pal().f4);
                for (i, o) in opts.iter().enumerate() {
                    let rr = [ox, sy, ox + ws[i], sy + sh];
                    let hv = self.an(ak(A_HOV, 300 + i), if self.hover == Hit::Seg(i) { 1.0 } else { 0.0 }, 12.0, 0.0);
                    let col = if i == cur { pal().t1 } else { mix(pal().t3, pal().t2, hv) };
                    c.text_c(ox + ws[i] / 2.0, sy + 1.43 * r, size, Face::Medium, tr(o), col);
                    self.hits.push((rr, Hit::Seg(i)));
                    ox += ws[i];
                }
                // the section's modules back as they came (only while one of them is not): a round arrow, its word
                // under the pointer; the second press within 3 s does it
                if matches!(self.cat, Cat::Player | Cat::Render | Cat::World | Cat::Misc) {
                    let pv = self.an(ak(A_HOV, 320), if self.section_dirty(self.cat) { 1.0 } else { 0.0 }, 10.0, 0.0);
                    let armed = self.section_arm.is_some_and(|(cat, t)| cat == self.cat && self.now - t < 3.0);
                    if pv > 0.01 {
                        let hv = self.an(ak(A_HOV, 321), if self.hover == Hit::SectionReset || armed { 1.0 } else { 0.0 }, 12.0, 0.0);
                        let av = self.an(ak(A_HOV, 323), if armed { 1.0 } else { 0.0 }, 12.0, 0.0);
                        let label = tr(if armed { "Again to reset the section" } else { "Reset the section" });
                        let lw0 = fonts().width(Face::Medium, 0.74 * r, label) + 0.7 * r;
                        let lw = self.an(ak(A_HOV, 322), lw0, 14.0, lw0);
                        let bw = (sh + lw * hv) * pv;
                        let b = [sx - 0.5 * r - bw, sy, sx - 0.5 * r, sy + sh];
                        let base = c.alpha;
                        c.alpha = base * pv;
                        c.rect(b, 0.7 * r, mix(mix(pal().f1, pal().f3, hv), pal().f4, av));
                        reset_icon(c, [b[0] + sh / 2.0 * pv, sy + sh / 2.0], 0.42 * r, mix(pal().t3, pal().t1, hv));
                        if hv > 0.01 {
                            c.push_clip(b);
                            c.alpha = base * pv * hv;
                            c.text(b[0] + sh - 0.1 * r, sy + 1.42 * r, 0.74 * r, Face::Medium, label, mix(pal().t2, pal().t1, av));
                            c.pop_clip();
                        }
                        c.alpha = base;
                        if pv > 0.5 {
                            self.hits.push((b, Hit::SectionReset));
                        }
                    }
                }
            }
        }
        self.head_last = c.p[h0 * draw::FLOATS..].to_vec();
        // the tiles
        let area = [x + 1.7 * r, y + 5.6 * r, x + w - 1.7 * r, y + wh - 0.4 * r];
        c.push_clip(area);
        self.hits.push((area, Hit::Outside));
        self.swap_ghost(c, false);
        let t0 = c.p.len() / draw::FLOATS;
        let h0 = self.hits.len();
        match (self.cat, self.q.is_empty()) {
            (_, true) if self.binds_page => self.binds_page(c, area),
            (_, true) if self.words_page => self.words_page(c, area),
            (Cat::Configs, true) => self.configs_page(c, area),
            (Cat::Settings, true) if self.settings_tab == 1 => self.tiles(c, area),
            (Cat::Settings, true) => self.page(c, area, Owner::Client, "Menu", "How the menu opens and sounds."),
            (Cat::Theme, true) => self.page(c, area, Owner::Theme, "Theme", "Change anything; the round arrow by a setting puts it back."),
            (Cat::Cosmetics, true) => self.cosm_page(c, area),
            (Cat::Forum, true) | (Cat::Music, true) => self.stub(c, area),
            _ => self.tiles(c, area),
        }
        // what is scrolled out of sight takes no clicks (a row under the title or the tabs took theirs)
        let kept: Vec<([f32; 4], Hit)> = self.hits.drain(h0..).filter_map(|(q, hit)| {
            let cq = [q[0].max(area[0]), q[1].max(area[1]), q[2].min(area[2]), q[3].min(area[3])];
            (cq[2] > cq[0] && cq[3] > cq[1]).then_some((cq, hit))
        }).collect();
        self.hits.extend(kept);
        self.tiles_last = c.p[t0 * draw::FLOATS..].to_vec();
        c.pop_clip();
        c.alpha = 1.0;
    }

    /// Places tiles in the theme's columns; an open one takes a whole row and the next fill the gap it left.
    fn place(&self, count: usize, open: Option<usize>, area: [f32; 4], tall: f32) -> Vec<[f32; 4]> {
        let r = self.r;
        let cols = pal().cols.max(1);
        let gap = 0.62 * r;
        let cw = (area[2] - area[0] - gap * (cols - 1) as f32) / cols as f32;
        let th = 4.55 * r;
        let cx = |col: usize| area[0] + col as f32 * (cw + gap);
        let mut out = vec![[0.0; 4]; count];
        let mut y = area[1];
        let mut col = 0;
        let mut holes: Vec<[f32; 2]> = Vec::new();
        for (i, o) in out.iter_mut().enumerate() {
            if Some(i) == open {
                if col > 0 {
                    for k in col..cols {
                        holes.push([cx(k), y]);
                    }
                    y += th + gap;
                    col = 0;
                }
                *o = [area[0], y, area[2], y + tall];
                y += tall + gap;
                continue;
            }
            if !holes.is_empty() {
                let hl = holes.remove(0);
                *o = [hl[0], hl[1], hl[0] + cw, hl[1] + th];
                continue;
            }
            *o = [cx(col), y, cx(col) + cw, y + th];
            col += 1;
            if col == cols {
                col = 0;
                y += th + gap;
            }
        }
        out
    }

    fn scroll_clamp(&mut self, area: [f32; 4], bottom: f32) {
        let max = (bottom + self.scroll - area[3] + 1.2 * self.r).max(0.0);
        self.content = max;
        self.scroll_to = self.scroll_to.clamp(0.0, max);
    }

    fn tiles(&mut self, c: &mut Canvas, area: [f32; 4]) {
        let r = self.r;
        let list = self.list();
        let open_pos = self.open.and_then(|o| list.iter().position(|&i| i == o));
        if self.open.is_some() && open_pos.is_none() {
            self.open = None;
        }
        let tall = open_pos.map_or(0.0, |p| if self.is_hud_module(list[p]) { self.hud_panel_height(area[2] - area[0]) } else { self.panel_height(Owner::Module(list[p]), area[2] - area[0]) });
        let rects = self.place(list.len(), open_pos, area, tall);
        let bottom = rects.iter().map(|q| q[3]).fold(area[1], f32::max) - self.scroll;
        self.scroll_clamp(area, bottom);
        if list.is_empty() {
            let rr = [area[0], area[1], area[2], area[1] + 4.55 * r];
            c.rect(rr, r, pal().f1);
            c.text(rr[0] + 1.15 * r, rr[1] + 1.9 * r, 0.93 * r, Face::Semi, tr("Nothing here"), pal().t2);
            c.text(rr[0] + 1.15 * r, rr[1] + 3.2 * r, 0.76 * r, Face::Regular, tr(if self.q.is_empty() { "No module matches this filter." } else { "Modules and settings are searched by their names. Esc clears." }), pal().t4);
        }
        // a tile new to the list (a search typed) fades in where it is
        self.anim.retain(|k, _| k >> 32 != A_VIS || list.contains(&((k & 0xffff_ffff) as usize)));
        for (n, &i) in list.iter().enumerate() {
            let target = rects[n];
            // where it is drawn eases to where it belongs (tiles slide when one opens)
            let tx = self.an(ak(A_TX, i), target[0], 14.0, target[0]);
            let ty = self.an(ak(A_TY, i), target[1], 14.0, target[1]);
            let tw = self.an(ak(A_TW, i), target[2] - target[0], 14.0, target[2] - target[0]);
            let th = self.an(ak(A_TH, i), target[3] - target[1], 14.0, target[3] - target[1]);
            let pv = self.an(ak(A_VIS, i), 1.0, 12.0, 0.0);
            let (a, dyv) = self.rise_c(0.22 + (n as f32 * 0.028).min(0.3), (n as f32 * 0.012).min(0.1), false);
            let (a, dyv) = (a * pv, dyv + (1.0 - pv) * 0.3 * r);
            c.alpha = a;
            let rr = [tx, ty - self.scroll + dyv, tx + tw, ty - self.scroll + dyv + th];
            if rr[3] < area[1] - 2.0 || rr[1] > area[3] + 2.0 {
                continue;
            }
            if Some(n) == open_pos && self.is_hud_module(i) {
                self.hud_panel(c, rr, i);
            } else if Some(n) == open_pos {
                self.panel(c, rr, Owner::Module(i));
            } else {
                self.tile(c, rr, i);
            }
        }
        c.alpha = 1.0;
    }

    fn tile(&mut self, c: &mut Canvas, rr: [f32; 4], i: usize) {
        let r = self.r;
        let m = self.head(i);
        let hov = matches!(self.hover, Hit::Tile(j) | Hit::TileSwitch(j) | Hit::TileExpand(j) if j == i);
        let hv = self.an(ak(A_HOV, 1000 + i), if hov { 1.0 } else { 0.0 }, 12.0, 0.0);
        let on = m.on && !m.plain;
        let onv = self.an(ak(A_SW, i), if on { 1.0 } else { 0.0 }, 10.0, if on { 1.0 } else { 0.0 });
        c.rect_lit(rr, r, mix(pal().f1, pal().f2, hv), grey(1.0, 0.075 * onv + 0.02 * hv * onv), 2.0);
        // the name, hot for a moment when the module is lit
        let heat = self.heat.get(&i).map_or(0.0, |t0| 1.0 - ((self.now - t0) / 0.9).clamp(0.0, 1.0));
        let name_col = mix(mix(pal().t2, pal().t1, onv), pal().accent, heat);
        let tag = if self.q.is_empty() {
            String::new()
        } else if let Some(si) = self.found_of(i).and_then(|f| f.set) {
            format!("{} · {}", tr(m.cat.name()), tr(self.mods[i].set[si].label))
        } else {
            format!("{}{}", tr(m.cat.name()), m.sub.filter(|_| m.cat != Cat::Render).map_or(String::new(), |s| format!(" · {}", tr(s))))
        };
        let mname = if m.plain { tr(m.name) } else { i18n::nm(m.name) };
        let nx = rr[0] + 1.15 * r;
        if heat > 0.0 {
            c.glow([nx + fonts().width(Face::Semi, 0.93 * r, mname) / 2.0, rr[1] + 1.65 * r], 3.0 * r, grey(1.0, 0.18 * heat));
        }
        let nw = c.text(nx, rr[1] + 1.95 * r, 0.93 * r, Face::Semi, mname, name_col);
        if !tag.is_empty() {
            c.text(nx + nw + 0.55 * r, rr[1] + 1.95 * r, 0.68 * r, Face::Medium, &tag, pal().t4);
        }
        // right side: key, arrows, switch (or the group's count)
        let mut right = rr[2] - 1.0 * r;
        if m.plain {
            let s = if m.id == "quality" { tr(&self.preset_name()).to_string() } else { group_on(&self.mods[i]) };
            let w = fonts().width(Face::Mono, 0.66 * r, &s) + 0.84 * r;
            c.rect([right - w, rr[1] + 1.82 * r, right, rr[1] + 2.88 * r], 0.36 * r, pal().f2);
            c.text(right - w + 0.42 * r, rr[1] + 2.58 * r, 0.66 * r, Face::Mono, &s, pal().t2);
            right -= w + 0.6 * r;
        } else {
            let sw = [right - 2.3 * r, rr[1] + 1.6 * r, right, rr[1] + 2.96 * r];
            switch(c, sw, onv, hv, false);
            self.hits.push(([sw[0] - 0.3 * r, rr[1], rr[2], rr[3]], Hit::TileSwitch(i)));
            right = sw[0] - 0.75 * r;
        }
        // the arrows: always there, brighter under the pointer; a left click on them opens the settings
        let av = self.an(ak(A_ARR, i), if self.hover == Hit::TileExpand(i) { 1.0 } else if hov { 0.6 } else { 0.0 }, 12.0, 0.0);
        let ac = [right - 0.5 * r, rr[1] + 2.28 * r];
        expand_icon(c, ac, 0.95 * r, mix(pal().t5, pal().t1, av));
        self.hits.push(([ac[0] - 0.9 * r, rr[1] + 0.9 * r, ac[0] + 0.9 * r, rr[3] - 0.9 * r], Hit::TileExpand(i)));
        right = ac[0] - 0.95 * r;
        // its keys, only when it has some (or one is being recorded): the middle button records, a press opens them
        let waiting = matches!(self.listen, Some(Listen::Bind(BindTo::Module(j), _)) if j == i);
        if (!m.binds.is_empty() && pal().binds) || waiting {
            let label = if waiting { self.rec_label() } else { cards::binds_label(&m.binds) };
            let w = fonts().width(Face::Mono, 0.66 * r, &label) + 0.84 * r;
            let kb = [right - w, rr[1] + 1.82 * r, right, rr[1] + 2.88 * r];
            let kv = self.an(ak(A_HOV, 3000 + i), if self.hover == Hit::TileBind(i) { 1.0 } else { 0.0 }, 12.0, 0.0);
            let pulse = if waiting { 0.5 + 0.5 * (self.now * 7.0).sin() } else { 0.0 };
            c.rect(kb, 0.36 * r, mix(mix(pal().f2, pal().f3, kv), grey(1.0, 0.22), pulse));
            c.text(kb[0] + 0.42 * r, rr[1] + 2.58 * r, 0.66 * r, Face::Mono, &label, if waiting { pal().t1 } else { mix(pal().t2, pal().t1, kv) });
            self.hits.push((kb, Hit::TileBind(i)));
            right -= w + 0.6 * r;
        }
        if pal().desc {
            let desc = fonts().fit(Face::Regular, 0.76 * r, tr(m.desc), right - nx - 0.4 * r);
            c.text(nx, rr[1] + 3.2 * r, 0.76 * r, Face::Regular, &desc, mix(pal().t4, pal().t3, onv));
        }
        // under all of it the tile itself (pushed first: what is over it wins)
        let at = self.hits.iter().rposition(|h| matches!(h.1, Hit::TileSwitch(j) | Hit::TileExpand(j) if j == i)).unwrap_or(self.hits.len());
        self.hits.insert(at.min(self.hits.len()).saturating_sub(if m.plain { 0 } else { 1 }), (rr, Hit::Tile(i)));
    }

    /// What a tile or a panel shows of a module (not its settings: they are not copied each frame).
    fn head(&self, i: usize) -> Head {
        let m = &self.mods[i];
        Head { id: m.id, cat: m.cat, sub: m.sub, name: m.name, desc: m.desc, on: m.on, binds: m.binds.clone(), plain: m.plain }
    }

    /// Chams' liquid: a preset chosen (Water, Mercury, Oil) sets the substance's settings (data::LIQUIDS) and is
    /// where their round arrows go back to; one of them changed after, the choice is Own; all as the preset has them
    /// again, the preset.
    pub(super) fn liquid_follow(&mut self) {
        let seen = self.liquid_seen;
        let Some(m) = self.mods.iter_mut().find(|m| m.id == "chams") else { return };
        let set = &mut m.set;
        let at = |set: &[Setting], l: &str| set.iter().position(|s| s.label == l);
        let (Some(li), Some(bi)) = (at(set, "Liquid"), at(set, "Liquid base")) else { return };
        let pick = |s: &Setting| if let Kind::Choice { v, .. } = s.kind { v } else { 0 };
        let liquid = pick(&set[li]);
        let mut base = pick(&set[bi]).min(data::LIQUIDS.len() - 1);
        if liquid < data::LIQUIDS.len() {
            base = liquid;
        }
        let chosen = liquid != seen && liquid < data::LIQUIDS.len();
        let (colours, sliders) = data::LIQUIDS[base];
        let mut same = true;
        for s in set.iter_mut() {
            let want = if let Some(k) = data::LIQUID_COLOURS.iter().position(|l| *l == s.label) {
                Kind::Colour(data::Col::new(colours[k]))
            } else if let Some(k) = data::LIQUID_SLIDERS.iter().position(|l| *l == s.label) {
                let Kind::Slider { min, max, step, unit, .. } = s.def else { continue };
                Kind::Slider { min, max, step, v: sliders[k], unit }
            } else {
                continue;
            };
            if chosen {
                s.kind = want.clone();
            }
            let close = match (&s.kind, &want) {
                (Kind::Slider { v, step, .. }, Kind::Slider { v: w, .. }) => (*v - *w).abs() <= *step * 0.5,
                (k, w) => k == w,
            };
            if close {
                s.kind = want.clone();
            }
            same &= close;
            s.def = want;
        }
        let now = if same { base } else { data::LIQUIDS.len() };
        if let Kind::Choice { v, .. } = &mut set[li].kind {
            *v = now;
        }
        if let Kind::Choice { v, .. } = &mut set[bi].kind {
            *v = base;
        }
        self.liquid_seen = now;
    }

    fn preset_name(&self) -> String {
        let on: Vec<&str> = ENGINE_KEYS.iter().copied().filter(|k| self.engine(k)).collect();
        let res = self.slider_value(Owner::Module(self.mod_index("quality")), "World resolution");
        for (name, r, list) in PRESETS {
            if list.len() == on.len() && list.iter().all(|k| on.contains(k)) && (res - r).abs() < 0.5 {
                return name.into();
            }
        }
        "Custom".into()
    }

    fn mod_index(&self, id: &str) -> usize {
        self.mods.iter().position(|m| m.id == id).unwrap_or(0)
    }

    fn slider_value(&self, o: Owner, label: &str) -> f32 {
        match self.set_ref(o).iter().find(|s| s.label == label).map(|s| &s.kind) {
            Some(Kind::Slider { v, .. }) => *v,
            _ => 0.0,
        }
    }

    // ------------------------------------------------------------------ an open module: interference, its settings
    fn rows_height(&self, set: &[Setting], width: f32) -> f32 {
        let r = self.r;
        set.iter()
            .filter(|s| s.shown(set) && !self.setting_hidden_here(s.label))
            .map(|s| match &s.kind {
                Kind::Header => 1.9 * r,
                Kind::Multi { opts, .. } => {
                    // label over chips, wrapped
                    let mut lines = 1.0;
                    let mut x = 0.0;
                    for o in opts {
                        let w = fonts().width(Face::Medium, 0.76 * r, tr(o)) + 1.4 * r + 0.8 * r;
                        if x + w > width {
                            lines += 1.0;
                            x = 0.0;
                        }
                        x += w + 0.35 * r;
                    }
                    1.9 * r + lines * 2.0 * r
                }
                Kind::Cost => 3.2 * r,
                Kind::Action => 2.9 * r,
                _ => 2.45 * r,
            })
            .sum()
    }

    fn panel_height(&self, o: Owner, total_w: f32) -> f32 {
        let r = self.r;
        // Auto Swap: its swaps under its settings, its editor on the left (as tall as it needs)
        let editor = matches!(o, Owner::Module(i) if self.mods[i].id == swap::MODULE);
        let split = self.previews() || editor;
        let pw = if split { total_w * 0.4 - 2.2 * r } else { total_w - 2.2 * r };
        let set = self.set_ref(o);
        let desc = match o {
            Owner::Module(i) => fonts().wrap(Face::Regular, 0.8 * r, tr(self.mods[i].desc), pw).len() as f32,
            _ => 1.0,
        };
        let mut h = 3.6 * r + desc * 1.2 * r + 1.0 * r + self.rows_height(set, pw) + 3.8 * r;
        if editor {
            h = (h + self.swap_list_height()).max(self.swap_need);
        }
        h.max(if split { 25.5 * r } else { 0.0 })
    }

    fn panel(&mut self, c: &mut Canvas, rr: [f32; 4], o: Owner) {
        let r = self.r;
        let opened = self.now - self.open_at;
        let reveal = (opened / 0.6).clamp(0.0, 1.0);
        c.rect(rr, r, grey(1.0, 0.05));
        let Owner::Module(i) = o else { return };
        self.hits.push((rr, Hit::Panel(i)));
        let m = self.head(i);
        let previews = self.previews();
        // Auto Swap's editor is in the preview's place, previews or not
        let editor = m.id == swap::MODULE;
        let px = if previews || editor { rr[0] + (rr[2] - rr[0]) * 0.6 } else { rr[0] };
        // the preview's place: interference for now, opening from a line of light
        if editor {
            let st = [rr[0] + 0.5 * r, rr[1] + 0.5 * r, px - 0.1 * r, rr[3] - 0.5 * r];
            self.swap_editor(c, st);
        } else if previews {
            let st = [rr[0] + 0.5 * r, rr[1] + 0.5 * r, px - 0.1 * r, rr[3] - 0.5 * r];
            if m.cat == Cat::HudEl {
                // a HUD element shows itself, as its settings make it now
                self.hud_preview(c, st, i, reveal);
            } else if m.id == "auto-accept" {
                self.accept_preview(c, st, i, reveal);
            } else if m.id == "fake-player" {
                self.fakes_preview(c, st, i, reveal);
            } else if m.id == "custom-gui" {
                // the containers as Custom Gui makes them now
                self.custom_gui_preview(c, st, reveal, i);
            } else if m.id == "custom-inventory" {
                // the inventory's blocks as its settings make them now
                self.inventory_preview(c, st, reveal, i);
            } else {
                self.stage(c, st, reveal, i as f32, m.on || m.plain, m.name);
            }
        }
        let x0 = px + 1.1 * r;
        let x1 = rr[2] - 1.0 * r;
        let rise = |d: f32| -> (f32, f32) {
            let k = ((opened - d) / 0.4).clamp(0.0, 1.0);
            (smooth(0.0, 1.0, k), (1.0 - ease_out(k)) * 0.35 * r)
        };
        let base_alpha = c.alpha;
        let (a, dy) = rise(0.12);
        c.alpha = base_alpha * a;
        let mut y = rr[1] + 1.05 * r + dy;
        // header: name, key, switch, close
        let xb = [x1 - 1.95 * r, y, x1, y + 1.95 * r];
        let hxv = self.an(ak(A_HOV, 2000), if self.hover == Hit::PanelClose { 1.0 } else { 0.0 }, 12.0, 0.0);
        c.rect(xb, 0.6 * r, mix(pal().f1, pal().f3, hxv));
        let xc = [xb[0] + 0.975 * r, xb[1] + 0.975 * r];
        let xs = 0.3 * r;
        let xcol = mix(pal().t3, pal().t1, hxv);
        c.line([xc[0] - xs, xc[1] - xs], [xc[0] + xs, xc[1] + xs], 1.5, xcol, 0.0);
        c.line([xc[0] + xs, xc[1] - xs], [xc[0] - xs, xc[1] + xs], 1.5, xcol, 0.0);
        self.hits.push((xb, Hit::PanelClose));
        let mut right = xb[0] - 0.65 * r;
        if !m.plain {
            let onv = self.an(ak(A_SW, i), if m.on { 1.0 } else { 0.0 }, 10.0, 0.0);
            let sw = [right - 2.3 * r, y + 0.3 * r, right, y + 1.66 * r];
            let hv = self.an(ak(A_HOV, 2001), if self.hover == Hit::PanelSwitch(i) { 1.0 } else { 0.0 }, 12.0, 0.0);
            switch(c, sw, onv, hv, false);
            self.hits.push((sw, Hit::PanelSwitch(i)));
            right = sw[0] - 0.65 * r;
            // its binds open the card of them; with none, a key icon in their place
            let wait = matches!(self.listen, Some(Listen::Bind(BindTo::Module(j), _)) if j == i);
            if m.binds.is_empty() && !wait {
                let ib = [right - 1.95 * r, y, right, y + 1.95 * r];
                let iv = self.an(ak(A_HOV, 2003), if self.hover == Hit::PanelBinds(i) { 1.0 } else { 0.0 }, 12.0, 0.0);
                let typing = self.since(ak(A_HOV, 2004), self.hover == Hit::PanelBinds(i));
                c.rect(ib, 0.6 * r, mix(pal().f1, pal().f3, iv));
                cards::key_icon(c, [(ib[0] + ib[2]) / 2.0, (ib[1] + ib[3]) / 2.0], 1.15 * r, typing, mix(pal().t3, pal().t1, iv));
                self.hits.push((ib, Hit::PanelBinds(i)));
                right = ib[0] - 0.55 * r;
            } else {
                let label = if wait { self.rec_label() } else { cards::binds_label(&m.binds) };
                let w = fonts().width(Face::Mono, 0.66 * r, &label) + 0.84 * r;
                let kb = [right - w, y + 0.45 * r, right, y + 1.51 * r];
                let kv = self.an(ak(A_HOV, 2002), if self.hover == Hit::PanelBind(i) { 1.0 } else { 0.0 }, 12.0, 0.0);
                let pulse = if wait { 0.5 + 0.5 * (self.now * 7.0).sin() } else { 0.0 };
                c.rect(kb, 0.36 * r, mix(mix(pal().f2, pal().f3, kv), grey(1.0, 0.22), pulse));
                c.text(kb[0] + 0.42 * r, kb[1] + 0.76 * r, 0.66 * r, Face::Mono, &label, pal().t1);
                self.hits.push((kb, Hit::PanelBind(i)));
                right = kb[0] - 0.6 * r;
            }
        }
        // its settings back as they came (only while one of them is not): a round arrow, its word under it while the
        // pointer is on it (drawn over the panel, last)
        let changed = self.mods[i].set.iter().any(Setting::changed);
        let pv = self.an(ak(A_HOV, 2005), if changed { 1.0 } else { 0.0 }, 10.0, 0.0);
        let mut tip = None;
        if pv > 0.01 {
            let hv = self.an(ak(A_HOV, 2006), if self.hover == Hit::PanelReset(i) { 1.0 } else { 0.0 }, 12.0, 0.0);
            let bw = 1.95 * r * pv;
            let ib = [right - bw, y, right, y + 1.95 * r];
            c.alpha = base_alpha * a * pv;
            c.rect(ib, 0.6 * r, mix(pal().f1, pal().f3, hv));
            reset_icon(c, [(ib[0] + ib[2]) / 2.0, y + 0.975 * r], 0.42 * r * pv, mix(pal().t3, pal().t1, hv));
            c.alpha = base_alpha * a;
            if pv > 0.5 {
                self.hits.push((ib, Hit::PanelReset(i)));
            }
            if hv > 0.01 {
                tip = Some(([(ib[0] + ib[2]) / 2.0, ib[3]], hv * pv * a));
            }
            right = ib[0] - 0.55 * r * pv;
        }
        let name = fonts().fit(Face::Bold, 1.32 * r, if m.plain { tr(m.name) } else { i18n::nm(m.name) }, right - x0);
        c.text(x0, y + 1.42 * r, 1.32 * r, Face::Bold, &name, pal().t1);
        y += 2.6 * r;
        let (a, dy2) = rise(0.16);
        c.alpha = base_alpha * a;
        for line in fonts().wrap(Face::Regular, 0.8 * r, tr(m.desc), x1 - x0) {
            c.text(x0, y + 0.9 * r + dy2 - dy, 0.8 * r, Face::Regular, &line, pal().t3);
            y += 1.2 * r;
        }
        y += 0.85 * r;
        self.module_code_buttons(c, i, [x0, y, x1, y + 1.9 * r]);
        y += 2.6 * r;
        c.alpha = base_alpha;
        self.rows(c, Owner::Module(i), x0, x1, y, rise);
        if editor {
            let below = y + self.rows_height(&self.mods[i].set, x1 - x0);
            self.swap_list(c, x0, x1, below, rise);
        }
        if let Some((at, k)) = tip {
            let label = tr(if self.reset_armed(reset::Target::Module(self.mods[i].id)) { "Press again to reset" } else { "Reset" });
            let tw = fonts().width(Face::Medium, 0.72 * r, label) + 1.4 * r;
            let ty = at[1] + 0.35 * r + (1.0 - k) * 0.3 * r;
            let tb = [(at[0] - tw / 2.0).min(rr[2] - 0.4 * r - tw), ty, 0.0, ty + 1.7 * r];
            let tb = [tb[0], tb[1], tb[0] + tw, tb[3]];
            c.alpha = base_alpha * k;
            c.shadow(tb, 0.6 * r, 1.2 * r, grey(0.0, 0.45));
            let g = pal().glass;
            c.rect(tb, 0.6 * r, [g[0].max(0.06), g[1].max(0.06), g[2].max(0.065), 0.97]);
            c.rect(tb, 0.6 * r, pal().f2);
            c.text_c((tb[0] + tb[2]) / 2.0, tb[1] + 1.1 * r, 0.72 * r, Face::Medium, label, pal().t1);
            c.alpha = base_alpha;
        }
    }

    /// The settings, one under another, each rising in its turn. A setting other than its default shows a round arrow
    /// that puts it back (a right click on its row does too).
    fn rows(&mut self, c: &mut Canvas, o: Owner, x0: f32, x1: f32, mut y: f32, rise: impl Fn(f32) -> (f32, f32)) {
        let r = self.r;
        let base_alpha = c.alpha;
        let n = self.set_ref(o).len();
        let okey = match o {
            Owner::Module(i) => i * 64,
            Owner::Client => 9000,
            Owner::Theme => 9500,
            Owner::Cosm => 9800,
        };
        for si in 0..n {
            if self.rows_mask.as_ref().is_some_and(|m| !m.get(si).copied().unwrap_or(true)) {
                continue;
            }
            // a setting of another of the choices (Chams' looks), or one this server forbids: not shown
            if !self.set_ref(o)[si].shown(self.set_ref(o)) || self.setting_hidden_here(self.set_ref(o)[si].label) {
                continue;
            }
            let s = self.set_ref(o)[si].clone();
            let (a, dy) = rise(0.18 + (si as f32 * 0.03).min(0.3));
            c.alpha = base_alpha * a;
            let yy = y + dy;
            // (a list longer than 64 — the chat's — goes on far from the next module's)
            let hkey = 20_000 + okey + if si < 64 { si } else { 1_000_000 + okey + si };
            match &s.kind {
                Kind::Header => {
                    c.text(x0, yy + 1.45 * r, 0.7 * r, Face::Semi, tr(s.label), pal().t4);
                    y += 1.9 * r;
                    continue;
                }
                Kind::Cost => {
                    let (ms, exact) = self.cost();
                    c.text(x0, yy + 1.1 * r, 0.76 * r, Face::Regular, tr(s.label), pal().t3);
                    c.text_r(x1, yy + 1.1 * r, 0.76 * r, Face::Mono, &format!("{}{:.2} ms", if exact { "" } else { "≈ " }, ms), pal().t1);
                    let f = self.an(ak(A_SET, hkey), (ms / 1.4).min(1.0), 8.0, 0.0);
                    c.rect([x0, yy + 1.75 * r, x1, yy + 2.05 * r], r, grey(1.0, 0.08));
                    c.rect_lit([x0, yy + 1.75 * r, x0 + (x1 - x0) * f, yy + 2.05 * r], r, theme::fade(pal().accent, 0.4), pal().accent, 1.0);
                    y += 3.2 * r;
                    continue;
                }
                Kind::Action => {
                    // a button the width of the row; resets are quieter than what opens something
                    let b = [x0, yy + 0.45 * r, x1, yy + 2.45 * r];
                    let hv = self.an(ak(A_HOV, hkey), if self.hover == Hit::Action(si) { 1.0 } else { 0.0 }, 12.0, 0.0);
                    let reset = s.label.starts_with("Reset");
                    c.rect(b, 0.6 * r, mix(pal().f2, pal().f4, hv));
                    if reset {
                        reset_icon(c, [b[0] + 1.2 * r, (b[1] + b[3]) / 2.0], 0.42 * r, mix(pal().t3, pal().t1, hv));
                    }
                    c.text(b[0] + if reset { 2.1 * r } else { 0.9 * r }, b[1] + 1.32 * r, 0.8 * r, Face::Medium, tr(s.label), mix(pal().t2, pal().t1, hv));
                    self.hits.push((b, Hit::Action(si)));
                    y += 2.9 * r;
                    continue;
                }
                _ => {}
            }
            let rh = if let Kind::Multi { .. } = s.kind { self.rows_height(std::slice::from_ref(&s), x1 - x0) } else { 2.45 * r };
            let rr = [x0 - 0.35 * r, yy, x1 + 0.35 * r, yy + rh];
            let hov = self.hits_row(si) && inside(rr, self.mouse);
            let hv = self.an(ak(A_HOV, hkey), if hov { 1.0 } else { 0.0 }, 12.0, 0.0);
            // the setting the search found, lit as its module opens (after the rows rise), fading
            let lit = match self.flash {
                Some((fi, fs, t0)) if o == Owner::Module(fi) && fs == si => {
                    let e = self.now - t0;
                    smooth(0.25, 0.55, e) * (1.0 - smooth(1.6, 2.6, e))
                }
                _ => 0.0,
            };
            c.rect(rr, 0.55 * r, grey(1.0, 0.03 * hv + 0.09 * lit));
            if lit > 0.0 {
                c.rect_line(rr, 0.55 * r, 1.0, grey(1.0, 0.35 * lit));
            }
            let label_col = mix(pal().t2, pal().t1, hv);
            let mid = yy + 1.225 * r;
            // its binds by its label (only when it has some, or one is being recorded)
            let bwait = matches!(self.listen, Some(Listen::Bind(BindTo::Setting(oo, sj), _)) if oo == o && sj == si);
            let blabel = if bwait { Some(self.rec_label()) } else if s.binds.is_empty() { None } else { Some(cards::binds_label(&s.binds)) };
            let bw = blabel.as_ref().map_or(0.0, |l| fonts().width(Face::Mono, 0.6 * r, l) + 0.75 * r + 0.55 * r);
            // the left edge of what the row holds on its right (the reset goes before it)
            let mut cl = x1;
            match &s.kind {
                Kind::Toggle(v) => {
                    c.text(x0, mid + 0.29 * r, 0.82 * r, Face::Regular, tr(s.label), label_col);
                    let onv = self.an(ak(A_SET, hkey), if *v { 1.0 } else { 0.0 }, 10.0, if *v { 1.0 } else { 0.0 });
                    switch(c, [x1 - 2.02 * r, mid - 0.6 * r, x1, mid + 0.6 * r], onv, hv, true);
                    self.hits.push((rr, Hit::Toggle(si)));
                    cl = x1 - 2.02 * r;
                }
                Kind::Slider { min, max, step, v, unit } => {
                    let lw = c.text(x0, mid + 0.29 * r, 0.82 * r, Face::Regular, tr(s.label), label_col) + bw;
                    let d = if *step < 1.0 { if *step < 0.1 { 2 } else { 1 } } else { 0 };
                    let vs = format!("{:.*}{}", d, v, i18n::unit(unit));
                    c.text_r(x1, mid + 0.27 * r, 0.74 * r, Face::Mono, &vs, pal().t1);
                    let vw = (4.4 * r).max(fonts().width(Face::Mono, 0.74 * r, &vs));
                    // the track: its usual length, shorter when a long label (another language's) needs the room — and
                    // the round arrow back to its default, when it has one, between them
                    let gap = if s.changed() { 2.6 * r } else { 1.1 * r };
                    let t0 = (x1 - vw - 0.75 * r - 8.4 * r).max(x0 + lw + gap).min(x1 - vw - 0.75 * r - 3.6 * r);
                    let tr = [t0, mid - 0.15 * r, x1 - vw - 0.75 * r, mid + 0.15 * r];
                    let target = if max > min && v.is_finite() { ((v - min) / (max - min)).clamp(0.0, 1.0) } else { 0.0 };
                    let f = self.an(ak(A_SET, hkey), target, if self.slider == Some((o, si)) { 40.0 } else { 9.0 }, 0.0);
                    c.rect(tr, r, grey(1.0, 0.1));
                    c.rect_lit([tr[0], tr[1], tr[0] + (tr[2] - tr[0]) * f, tr[3]], r, theme::fade(pal().accent, 0.45), pal().accent, 1.0);
                    let th = [tr[0] + (tr[2] - tr[0]) * f, mid];
                    let big = if hov || self.slider == Some((o, si)) { 1.22 } else { 1.0 };
                    c.shadow([th[0] - 0.46 * r * big, th[1] - 0.46 * r * big, th[0] + 0.46 * r * big, th[1] + 0.46 * r * big], 0.46 * r * big, 0.4 * r, grey(0.0, 0.5));
                    c.rect([th[0] - 0.46 * r * big, th[1] - 0.46 * r * big, th[0] + 0.46 * r * big, th[1] + 0.46 * r * big], 0.46 * r * big, pal().accent);
                    self.hits.push(([tr[0] - 0.5 * r, yy, tr[2] + 0.5 * r, yy + rh], Hit::Slider(si)));
                    cl = tr[0] - 0.4 * r;
                }
                Kind::Choice { v, opts } => {
                    let lw = c.text(x0, mid + 0.29 * r, 0.82 * r, Face::Regular, tr(s.label), label_col) + bw;
                    // the words side by side when they fit beside the label (in the language shown), else a list
                    let seg_w: f32 = opts.iter().map(|o| fonts().width(Face::Medium, 0.74 * r, tr(o)) + 1.24 * r).sum::<f32>() + 0.36 * r;
                    let reset_room = if s.changed() { 1.6 * r } else { 0.0 };
                    let short = opts.len() <= 4 && opts.iter().map(|o| o.len()).sum::<usize>() <= 22 && seg_w <= x1 - x0 - lw - 1.4 * r - reset_room;
                    if short {
                        let size = 0.74 * r;
                        let pad = 0.62 * r;
                        let ws: Vec<f32> = opts.iter().map(|o| fonts().width(Face::Medium, size, tr(o)) + 2.0 * pad).collect();
                        let total = ws.iter().sum::<f32>() + 0.36 * r;
                        let sx = x1 - total;
                        let sh = 1.75 * r;
                        let sy = mid - sh / 2.0;
                        c.rect([sx, sy, x1, sy + sh], 0.58 * r, pal().f1);
                        let cur = (*v).min(opts.len() - 1);
                        let custom = s.label == "Graphics" && self.preset_name() == "Custom";
                        let cx: f32 = ws[..cur].iter().sum::<f32>() + sx + 0.18 * r;
                        let tx = self.an(ak(A_SEG, hkey), cx, 16.0, cx);
                        let tw = self.an(ak(A_SEGW, hkey), if custom { 0.0 } else { ws[cur] }, 16.0, ws[cur]);
                        if tw > 1.0 {
                            c.rect([tx, sy + 0.18 * r, tx + tw, sy + sh - 0.18 * r], 0.42 * r, pal().f4);
                        }
                        let mut ox = sx + 0.18 * r;
                        for (oi, op) in opts.iter().enumerate() {
                            let hit = Hit::Choice(si, oi);
                            let ohv = self.an(ak(A_HOV, 40_000 + hkey * 8 + oi), if self.hover == hit { 1.0 } else { 0.0 }, 12.0, 0.0);
                            let col = if oi == cur && !custom { pal().t1 } else { mix(pal().t3, pal().t2, ohv) };
                            c.text_c(ox + ws[oi] / 2.0, mid + 0.27 * r, size, Face::Medium, tr(op), col);
                            self.hits.push(([ox, sy, ox + ws[oi], sy + sh], hit));
                            ox += ws[oi];
                        }
                        cl = sx;
                    } else {
                        let val = tr(opts[(*v).min(opts.len() - 1)]);
                        let w = fonts().width(Face::Medium, 0.78 * r, val) + 2.4 * r;
                        let dd = [x1 - w, mid - 0.9 * r, x1, mid + 0.9 * r];
                        let open = matches!(self.pop, Some(Pop::List { owner, set, .. }) if owner == o && set == si);
                        let dv = self.an(ak(A_HOV, 50_000 + hkey), if open || self.hover == Hit::Drop(si) { 1.0 } else { 0.0 }, 12.0, 0.0);
                        c.rect(dd, 0.58 * r, mix(pal().f1, pal().f3, dv));
                        c.text(dd[0] + 0.78 * r, mid + 0.28 * r, 0.78 * r, Face::Medium, val, pal().t1);
                        chevron(c, [dd[2] - 0.85 * r, mid], 0.32 * r, pal().t3, open);
                        self.hits.push((dd, Hit::Drop(si)));
                        cl = dd[0];
                    }
                }
                Kind::Multi { on, opts } => {
                    c.text(x0, yy + 1.35 * r, 0.82 * r, Face::Regular, tr(s.label), label_col);
                    let mut cx = x0;
                    let mut cy = yy + 1.9 * r;
                    for (oi, op) in opts.iter().enumerate() {
                        let w = fonts().width(Face::Medium, 0.76 * r, tr(op)) + 1.4 * r + 0.8 * r;
                        if cx + w > x1 + 0.01 {
                            cx = x0;
                            cy += 2.0 * r;
                        }
                        let ch = [cx, cy, cx + w, cy + 1.62 * r];
                        let hit = Hit::Chip(si, oi);
                        let cv = self.an(ak(A_SET, 60_000 + hkey * 8 + oi), if on[oi] { 1.0 } else { 0.0 }, 10.0, if on[oi] { 1.0 } else { 0.0 });
                        let chv = self.an(ak(A_HOV, 60_000 + hkey * 8 + oi), if self.hover == hit { 1.0 } else { 0.0 }, 12.0, 0.0);
                        c.rect(ch, r, mix(mix(pal().f1, pal().f2, chv), pal().f4, cv));
                        let dot = [ch[0] + 0.85 * r, ch[1] + 0.81 * r];
                        if cv > 0.05 {
                            c.glow(dot, 0.5 * r, theme::fade(pal().accent, 0.5 * cv));
                        }
                        c.rect([dot[0] - 0.18 * r, dot[1] - 0.18 * r, dot[0] + 0.18 * r, dot[1] + 0.18 * r], 0.18 * r, mix(grey(1.0, 0.2), pal().accent, cv));
                        c.text(ch[0] + 1.4 * r, ch[1] + 1.08 * r, 0.76 * r, Face::Medium, tr(op), mix(pal().t3, pal().t1, cv.max(chv * 0.6)));
                        self.hits.push((ch, hit));
                        cx += w + 0.35 * r;
                    }
                    cl = x0 + fonts().width(Face::Regular, 0.82 * r, tr(s.label)) + 2.2 * r;
                }
                Kind::Colour(v) => {
                    c.text(x0, mid + 0.29 * r, 0.82 * r, Face::Regular, tr(s.label), label_col);
                    let shown = self.colour_now(o, s.label, *v);
                    let label = if v.auto {
                        tr("Theme").to_string()
                    } else if v.chroma > 0.0 {
                        tr("Chroma").to_string()
                    } else if v.a < 0.995 {
                        format!("{:06X} {:.0}%", v.rgb, v.a * 100.0)
                    } else {
                        format!("{:06X}", v.rgb)
                    };
                    let w = fonts().width(Face::Mono, 0.7 * r, &label) + 2.9 * r;
                    let cs = [x1 - w, mid - 0.85 * r, x1, mid + 0.85 * r];
                    let open = self.pick.as_ref().is_some_and(|pk| pk.owner == o && pk.set == si);
                    let cv = self.an(ak(A_HOV, 70_000 + hkey), if open || self.hover == Hit::Colour(si) { 1.0 } else { 0.0 }, 12.0, 0.0);
                    c.rect(cs, 0.58 * r, mix(pal().f1, pal().f3, cv));
                    c.text(cs[0] + 0.75 * r, mid + 0.26 * r, 0.7 * r, Face::Mono, &label, pal().t2);
                    let sw = [cs[2] - 1.5 * r, mid - 0.5 * r, cs[2] - 0.5 * r, mid + 0.5 * r];
                    c.glow([cs[2] - 1.0 * r, mid], 0.9 * r, [shown[0], shown[1], shown[2], 0.35 * shown[3]]);
                    if shown[3] < 0.995 {
                        c.rect_checker(sw, 0.5 * r, theme::fade(shown, 0.0), shown);
                    }
                    c.rect(sw, 0.5 * r, shown);
                    self.hits.push((cs, Hit::Colour(si)));
                    cl = cs[0];
                }
                Kind::Key(v) => {
                    c.text(x0, mid + 0.29 * r, 0.82 * r, Face::Regular, tr(s.label), label_col);
                    let wait = self.listen == Some(Listen::Key(o, si));
                    let label = if wait { "Press a key".to_string() } else { v.clone() };
                    let w = fonts().width(Face::Mono, 0.66 * r, &label) + 0.84 * r;
                    let kb = [x1 - w, mid - 0.53 * r, x1, mid + 0.53 * r];
                    let pulse = if wait { 0.5 + 0.5 * (self.now * 7.0).sin() } else { 0.0 };
                    c.rect(kb, 0.36 * r, mix(pal().f2, grey(1.0, 0.22), pulse));
                    c.text(kb[0] + 0.42 * r, mid + 0.24 * r, 0.66 * r, Face::Mono, &label, if v == "None" && !wait { pal().t4 } else { pal().t1 });
                    self.hits.push((kb, Hit::KeyRow(si)));
                    cl = kb[0];
                }
                Kind::Text(v) => {
                    c.text(x0, mid + 0.29 * r, 0.82 * r, Face::Regular, tr(s.label), label_col);
                    let focus = self.text_focus == Some((o, si));
                    let tb = [x1 - 9.5 * r, mid - 0.88 * r, x1, mid + 0.88 * r];
                    let fv = self.an(ak(A_HOV, 90_000 + hkey), if focus { 1.0 } else { 0.0 }, 14.0, 0.0);
                    c.rect(tb, 0.55 * r, mix(pal().f1, pal().f3, fv));
                    // longer than the field: its end stays in sight, the start slides out under the field's edge
                    let tw = fonts().width(Face::Regular, 0.78 * r, v);
                    let room = tb[2] - tb[0] - 1.4 * r;
                    let shift = self.an(ak(A_SET, 90_000 + hkey), (tw - room).max(0.0), 18.0, (tw - room).max(0.0));
                    c.push_clip([tb[0] + 0.35 * r, tb[1], tb[2] - 0.35 * r, tb[3]]);
                    c.text(tb[0] + 0.65 * r - shift, mid + 0.28 * r, 0.78 * r, Face::Regular, v, pal().t1);
                    if focus && (self.now * 2.0).fract() < 0.55 {
                        let cx = tb[0] + 0.7 * r + tw - shift;
                        c.rect([cx, mid - 0.5 * r, cx + 1.5, mid + 0.5 * r], 0.0, pal().accent);
                    }
                    c.pop_clip();
                    self.hits.push((tb, Hit::TextRow(si)));
                    cl = tb[0];
                }
                _ => {}
            }
            if let Some(l) = blabel {
                let base = if matches!(s.kind, Kind::Multi { .. }) { yy + 1.35 * r } else { mid + 0.29 * r };
                let lx = x0 + fonts().width(Face::Regular, 0.82 * r, tr(s.label)) + 0.55 * r;
                let kb = [lx, base - 0.86 * r, lx + bw - 0.55 * r, base + 0.3 * r];
                let kv = self.an(ak(A_HOV, 95_000 + hkey), if self.hover == Hit::RowBind(si) { 1.0 } else { 0.0 }, 12.0, 0.0);
                let pulse = if bwait { 0.5 + 0.5 * (self.now * 7.0).sin() } else { 0.0 };
                c.rect(kb, 0.32 * r, mix(mix(pal().f2, pal().f3, kv), grey(1.0, 0.22), pulse));
                c.text(kb[0] + 0.37 * r, base - 0.05 * r, 0.6 * r, Face::Mono, &l, if bwait { pal().t1 } else { mix(pal().t2, pal().t1, kv) });
                self.hits.push((kb, Hit::RowBind(si)));
            }
            // other than its default: the round arrow back to it
            if s.changed() {
                let ic = [cl - 1.05 * r, mid];
                let rv = self.an(ak(A_HOV, 80_000 + hkey), if self.hover == Hit::Reset(si) { 1.0 } else { 0.0 }, 14.0, 0.0);
                c.rect([ic[0] - 0.75 * r, ic[1] - 0.75 * r, ic[0] + 0.75 * r, ic[1] + 0.75 * r], 0.75 * r, grey(1.0, 0.03 + 0.09 * rv));
                reset_icon(c, ic, 0.36 * r, mix(pal().t3, pal().t1, rv));
                self.hits.push(([ic[0] - 0.8 * r, ic[1] - 0.8 * r, ic[0] + 0.8 * r, ic[1] + 0.8 * r], Hit::Reset(si)));
            }
            y += rh;
        }
        c.alpha = base_alpha;
    }

    /// What a colour setting shows now: its own, or (following the theme) the theme's of the same name.
    fn colour_now(&self, o: Owner, label: &str, v: data::Col) -> Rgba {
        if !v.auto {
            return theme::now(v, self.now);
        }
        let look = theme::HudLook::of(&self.theme, self.now);
        let _ = o;
        match label {
            "Glass" => look.glass,
            "Text" | "Health" => look.text,
            "Secondary text" => look.text2,
            _ => look.accent,
        }
    }

    fn hits_row(&self, _si: usize) -> bool {
        true
    }

    /// The preview's place: interference in the client's colours, opening from a line of light like the window.
    #[allow(clippy::too_many_arguments)]
    fn stage(&mut self, c: &mut Canvas, st: [f32; 4], reveal: f32, seed: f32, lit: bool, name: &str) {
        let r = self.r;
        let open = ease_out(((reveal - 0.12) / 0.7).clamp(0.0, 1.0));
        let cy = (st[1] + st[3]) / 2.0;
        let hh = (st[3] - st[1]) / 2.0 * open;
        c.push_clip([st[0], cy - hh, st[2], cy + hh]);
        let strength = if lit { 1.0 } else { 0.55 };
        c.interference(st, 0.75 * r, self.now, seed, strength);
        // the module's name faint in the noise, and a line of light running down it
        let size = 1.1 * r;
        c.text_c((st[0] + st[2]) / 2.0, cy + size * 0.35, size, Face::Semi, name, grey(1.0, 0.16 * strength));
        let sweep = st[1] + (st[3] - st[1]) * ((self.now * 0.23 + seed * 0.37).fract());
        c.line([st[0] + 0.75 * r, sweep], [st[2] - 0.75 * r, sweep], 1.0, grey(1.0, 0.1 * strength), 6.0);
        c.pop_clip();
        // the line it opens from, splitting to the edges
        let lk = ((reveal) / 0.22).clamp(0.0, 1.0);
        let fade = 1.0 - smooth(0.22, 0.9, reveal);
        if fade > 0.0 {
            let wl = (st[2] - st[0]) * lk;
            let x0 = (st[0] + st[2]) / 2.0 - wl / 2.0;
            for yy in [cy - hh, cy + hh] {
                c.line([x0, yy], [x0 + wl, yy], 1.0, grey(1.0, fade), 8.0);
            }
        }
    }

    fn cost(&self) -> (f32, bool) {
        let p = self.preset_name();
        if let Some(i) = PRESETS.iter().position(|q| q.0 == p) {
            return (MEASURED[i], true);
        }
        let res = self.slider_value(Owner::Module(self.mod_index("quality")), "World resolution") / 100.0;
        let sum: f32 = ENGINE_KEYS.iter().filter(|k| self.engine(k)).map(|k| data::part_cost(k)).sum();
        (0.507 + sum * (0.45 + 0.55 * res * res), false)
    }

    // ------------------------------------------------------------------ Settings, the stubs
    /// A page of one owner's settings under a title (Settings, Theme).
    fn page(&mut self, c: &mut Canvas, area: [f32; 4], o: Owner, title: &str, line: &str) {
        let r = self.r;
        let (a, dyv) = self.rise_c(0.22, 0.0, false);
        c.alpha = a;
        let set = self.set_ref(o).clone();
        let h = 4.0 * r + self.rows_height(&set, area[2] - area[0] - 2.3 * r) + 1.5 * r;
        let rr = [area[0], area[1] - self.scroll + dyv, area[2], area[1] - self.scroll + dyv + h];
        self.scroll_clamp(area, rr[3]);
        c.rect(rr, r, grey(1.0, 0.05));
        c.text(rr[0] + 1.15 * r, rr[1] + 2.2 * r, 1.32 * r, Face::Bold, tr(title), pal().t1);
        c.text(rr[0] + 1.15 * r, rr[1] + 3.35 * r, 0.8 * r, Face::Regular, tr(line), pal().t3);
        self.rows(c, o, rr[0] + 1.15 * r, rr[2] - 1.15 * r, rr[1] + 4.0 * r, |_| (1.0, 0.0));
        c.alpha = 1.0;
    }

    fn stub(&mut self, c: &mut Canvas, area: [f32; 4]) {
        let r = self.r;
        let (a, dyv) = self.rise_c(0.22, 0.0, false);
        c.alpha = a;
        let rr = [area[0], area[1] + dyv, area[2], (area[1] + 24.0 * r).min(area[3]) + dyv];
        c.rect(rr, r, grey(1.0, 0.05));
        let st = [rr[0] + 0.5 * r, rr[1] + 0.5 * r, rr[2] - 0.5 * r, rr[3] - 0.5 * r];
        let (title, line) = match self.cat {
            Cat::Forum => ("Forum", "The forum of cursedclient.xyz in the game: threads, answers, your conversations."),
            Cat::Cosmetics => ("Cosmetics", "Capes, hats, pets and wings: tried on here before you wear them."),
            _ => ("Music", "Your tracks and the radio, with the record from the menu."),
        };
        let seed = self.cat as usize as f32 * 7.0;
        c.push_clip(st);
        c.interference(st, 0.75 * r, self.now, seed, 0.8);
        let cx = (st[0] + st[2]) / 2.0;
        let cy = (st[1] + st[3]) / 2.0;
        c.glow([cx, cy], 9.0 * r, grey(0.0, 0.55));
        c.text_c(cx, cy - 0.2 * r, 2.2 * r, Face::Bold, tr(title), grey(1.0, 0.92));
        c.text_c(cx, cy + 1.5 * r, 0.82 * r, Face::Regular, tr(line), pal().t2);
        c.text_c(cx, cy + 3.1 * r, 0.66 * r, Face::Mono, tr("in a later build"), pal().t4);
        c.pop_clip();
        c.alpha = 1.0;
    }

    // ------------------------------------------------------------------ the popovers
    fn popover(&mut self, c: &mut Canvas) {
        let r = self.r;
        let Some(p) = self.pop.clone() else {
            self.anim.remove(&ak(A_POP, 0));
            return;
        };
        let k = self.an(ak(A_POP, 0), 1.0, 14.0, 0.0);
        match p {
            Pop::List { owner, set, at } => {
                let (cur, opts) = match &self.set_list(owner)[set].kind {
                    Kind::Choice { v, opts } => (*v, opts.clone()),
                    _ => return,
                };
                let w = opts.iter().map(|o| fonts().width(Face::Medium, 0.8 * r, tr(o))).fold(0.0, f32::max) + 3.6 * r;
                let h = opts.len() as f32 * 2.2 * r + 0.64 * r;
                let mut x = at[2] - w;
                let mut y = at[3] + 0.4 * r;
                if y + h > self.h - 8.0 {
                    y = at[1] - h - 0.4 * r;
                }
                x = x.clamp(8.0, self.w - w - 8.0);
                let rr = [x, y - (1.0 - k) * 0.4 * r, x + w, y + h - (1.0 - k) * 0.4 * r];
                c.alpha = k;
                c.shadow(rr, 0.8 * r, 2.4 * r, grey(0.0, 0.6));
                let g = pal().glass;
                c.rect(rr, 0.8 * r, [g[0].max(0.07), g[1].max(0.07), g[2].max(0.07), 0.97]);
                for (i, o) in opts.iter().enumerate() {
                    let ir = [rr[0] + 0.32 * r, rr[1] + 0.32 * r + i as f32 * 2.2 * r, rr[2] - 0.32 * r, rr[1] + 0.32 * r + (i + 1) as f32 * 2.2 * r];
                    let hv = self.an(ak(A_HOV, 9500 + i), if self.hover == Hit::Pop(i) { 1.0 } else { 0.0 }, 14.0, 0.0);
                    c.rect(ir, 0.5 * r, grey(1.0, 0.065 * hv));
                    c.text(ir[0] + 0.7 * r, ir[1] + 1.38 * r, 0.8 * r, Face::Medium, tr(o), if i == cur { pal().t1 } else { mix(pal().t2, pal().t1, hv) });
                    if i == cur {
                        let cc = [ir[2] - 1.0 * r, ir[1] + 1.1 * r];
                        c.polyline(&[[cc[0] - 0.32 * r, cc[1]], [cc[0] - 0.08 * r, cc[1] + 0.25 * r], [cc[0] + 0.34 * r, cc[1] - 0.25 * r]], 1.5, pal().t1);
                    }
                    self.hits.push((ir, Hit::Pop(i)));
                }
                c.alpha = 1.0;
            }
        }
    }

    // ------------------------------------------------------------------ input
    fn hit_at(&self, p: [f32; 2]) -> Hit {
        for (r, h) in self.hits.iter().rev() {
            if inside(*r, p) {
                return *h;
            }
        }
        Hit::None
    }

    fn mouse_move(&mut self, x: f32, y: f32) {
        self.mouse = [x, y];
        if let Some(d) = self.drag {
            self.dx = d[2] + x - d[0];
            self.dy = d[3] + y - d[1];
        }
        if let Some((o, si)) = self.slider {
            self.slide(o, si, x);
        }
        if let Some(part) = self.pick.as_ref().map(|p| p.drag).filter(|d| *d > 0) {
            self.pick_at(part, x, y);
        }
        self.swap_move(x, y);
        self.cosm_move(x, y);
        let h = self.hit_at(self.mouse);
        if h != self.hover {
            self.hover = h;
            if self.phase == Phase::Open {
                let k = match h {
                    Hit::Nav(_) => 1.0,
                    Hit::Tile(_) | Hit::Seg(_) => 0.55,
                    Hit::TileExpand(_) | Hit::Pop(_) | Hit::Choice(..) | Hit::Chip(..) | Hit::Cfg(..) | Hit::Action(_) | Hit::Reset(_) | Hit::EditLayout => 0.35,
                    Hit::TileBind(_) | Hit::PanelBind(_) | Hit::PanelBinds(_) | Hit::RowBind(_) | Hit::BindsBtn | Hit::LangBtn | Hit::AltBtn | Hit::HudPick(_) => 0.35,
                    Hit::Card(p, _) if p > 0 => 0.3,
                    Hit::Pick(5, _) => 0.25,
                    Hit::Swap(0 | 2 | 3 | 4, _) => 0.35,
                    Hit::Swap(8 | 10 | 13 | 17, _) => 0.3,
                    Hit::Cosm(1 | 2 | 5 | 11 | 12, _) => 0.35,
                    Hit::Cosm(3 | 4, _) => 0.25,
                    Hit::Cosm(13..=19, _) => 0.3,
                    Hit::Cosm(20 | 21 | 23 | 26 | 27 | 30 | 31 | 35 | 36 | 38 | 39 | 40 | 41 | 43, _) => 0.35,
                    Hit::Cosm(22 | 24 | 25 | 32 | 33, _) => 0.25,
                    _ => 0.0,
                };
                if k > 0.0 {
                    self.sound_tick(k);
                }
            }
        }
    }

    fn slide(&mut self, o: Owner, si: usize, x: f32) {
        let r = self.r;
        // the track's place: the same sums as rows() draws it
        let Some((rect, _)) = self.hits.iter().find(|h| h.1 == Hit::Slider(si)).copied() else { return };
        let (t0, t1) = (rect[0] + 0.5 * r, rect[2] - 0.5 * r);
        let k = ((x - t0) / (t1 - t0)).clamp(0.0, 1.0);
        let mut changed = false;
        if let Kind::Slider { min, max, step, v, .. } = &mut self.set_list(o)[si].kind {
            let nv = ((*min + (*max - *min) * k) / *step).round() * *step;
            let nv = nv.clamp(*min, *max);
            if (nv - *v).abs() > 1e-4 {
                *v = nv;
                changed = true;
            }
        }
        if changed {
            self.sound_tick(0.2);
        }
    }

    fn toggle(&mut self, i: usize) {
        let m = &mut self.mods[i];
        if m.plain {
            return;
        }
        m.on = !m.on;
        if m.on {
            self.heat.insert(i, self.now);
            self.hud.lit_seq += 1;
            self.hud.lit.insert(i, self.hud.lit_seq);
        }
        self.switch_sound(self.mods[i].on, 1.0);
    }

    fn expand(&mut self, i: usize) {
        if self.open == Some(i) {
            self.open = None;
        } else {
            self.open = Some(i);
            self.open_at = self.now;
            self.flash = self.found_of(i).and_then(|f| f.set).map(|si| (i, si, self.now));
            self.sound_tick(0.7);
        }
        self.pop = None;
        self.text_focus = None;
    }

    fn open_cat(&mut self, cat: Cat) {
        if cat == Cat::Configs {
            self.cfg_scan();
        }
        if self.cat != cat || !self.q.is_empty() {
            // the right side crosses over to the section; the side, the window stay as they are
            self.swap(true);
            self.cat = cat;
            self.open = None;
            self.scroll_to = 0.0;
            self.scroll = 0.0;
            for k in [A_TX, A_TY, A_TW, A_TH] {
                self.anim.retain(|kk, _| kk >> 32 != k);
            }
        }
        if self.binds_page || self.words_page {
            self.binds_page = false;
            self.word_end();
            self.words_page = false;
            self.swap(true);
            self.scroll_to = 0.0;
            self.scroll = 0.0;
        }
        self.q.clear();
        self.typing = false;
        self.pop = None;
    }

    /// The tiles cross over to the section's other part or filter.
    fn open_part(&mut self, i: usize) {
        let cur = match self.cat {
            Cat::Player => self.filter[0],
            Cat::Misc => self.filter[1],
            Cat::Render => self.filter[2],
            Cat::World => self.filter[3],
            Cat::Settings => self.settings_tab,
            _ => return,
        };
        if cur == i {
            return;
        }
        self.swap(false);
        match self.cat {
            Cat::Player => self.filter[0] = i,
            Cat::Render => self.filter[2] = i,
            Cat::World => self.filter[3] = i,
            Cat::Settings => self.settings_tab = i,
            _ => self.filter[1] = i,
        }
        self.open = None;
        self.scroll_to = 0.0;
        self.scroll = 0.0;
        for k in [A_TX, A_TY, A_TW, A_TH] {
            self.anim.retain(|kk, _| kk >> 32 != k);
        }
    }

    fn press(&mut self, x: f32, y: f32, button: i32) -> bool {
        self.undo_poke(Some(true));
        self.mouse_move(x, y);
        self.card_shut = None;
        if self.phase != Phase::Open && self.phase != Phase::Opening {
            return true;
        }
        let h = self.hit_at([x, y]);
        // the tour takes every press while it is on
        if self.tour.is_some() {
            self.tour_press(h);
            return true;
        }
        if self.card.is_some() && self.card_press(h, button) {
            return true;
        }
        if self.press_common(h, x, y, button) {
            return true;
        }
        if !matches!(h, Hit::Search) {
            self.typing = false;
        }
        // Auto Swap's editor: the left button and the right
        if let Hit::Swap(part, idx) = h {
            if button == 0 || button == 1 {
                self.swap_press(part, idx as usize, button);
            }
            return true;
        }
        if let Hit::Cosm(part, n) = h {
            self.cosm_press(part, n, button);
            return true;
        }
        let owner = self.owner();
        let at = self.hits.iter().rev().find(|q| q.1 == h).map_or([x, y, x, y], |q| q.0);
        if button == 2 {
            // the middle button binds: a module (its first bind recorded anew), a setting
            match h {
                Hit::Tile(i) | Hit::TileSwitch(i) | Hit::TileExpand(i) | Hit::TileBind(i) | Hit::PanelSwitch(i) | Hit::PanelBind(i) | Hit::PanelBinds(i) if i < 10_000 && !self.mods[i].plain => {
                    let slot = cards::first_slot(&self.mods[i].binds);
                    self.start_bind(BindTo::Module(i), slot);
                }
                Hit::Toggle(si) | Hit::Slider(si) | Hit::Choice(si, _) | Hit::Drop(si) | Hit::Chip(si, _) | Hit::Colour(si) | Hit::RowBind(si) | Hit::Action(si) => {
                    let to = BindTo::Setting(owner, si);
                    let slot = cards::first_slot(self.binds_of(to));
                    self.start_bind(to, slot);
                }
                _ => {}
            }
            return true;
        }
        if button == 1 {
            match h {
                Hit::TileBind(i) | Hit::PanelBind(i) | Hit::PanelBinds(i) => self.open_binds(BindTo::Module(i), at),
                Hit::Tile(i) | Hit::TileSwitch(i) | Hit::TileExpand(i) => self.expand(i % 10_000),
                Hit::Panel(_) => self.open = None,
                _ => {
                    self.press_rows(h, x, y, owner, 1);
                }
            }
            return true;
        }
        if button != 0 {
            return true;
        }
        if self.press_rows(h, x, y, owner, 0) {
            return true;
        }
        if self.binds_page && self.binds_page_press(h) {
            return true;
        }
        if self.words_page && self.words_press(h) {
            return true;
        }
        if self.hud_panel_press(h) {
            return true;
        }
        if self.cat == Cat::Configs && self.q.is_empty() && !self.binds_page && self.configs_press(h, at) {
            return true;
        }
        match h {
            Hit::LangBtn => self.open_lang(at),
            Hit::AltBtn => self.open_alts(at),
            Hit::BindsBtn => {
                self.word_end();
                self.words_page = false;
                self.binds_page = !self.binds_page;
                self.swap(true);
                self.open = None;
                self.q.clear();
                self.typing = false;
                self.scroll_to = 0.0;
                self.scroll = 0.0;
                self.sound_tick(0.6);
            }
            Hit::TileBind(i) | Hit::PanelBind(i) | Hit::PanelBinds(i) => self.open_binds(BindTo::Module(i), at),
            Hit::RowBind(si) => self.open_binds(BindTo::Setting(owner, si), at),
            Hit::Header => self.drag = Some([x, y, self.dx, self.dy]),
            Hit::Nav(c) => self.open_cat(c),
            Hit::Seg(i) => self.open_part(i),
            Hit::Search => self.typing = true,
            Hit::Tile(i) => {
                if i >= 10_000 || self.mods[i].plain {
                    self.expand(i % 10_000);
                } else {
                    self.toggle(i);
                }
            }
            Hit::TileSwitch(i) | Hit::PanelSwitch(i) => self.toggle(i),
            Hit::TileExpand(i) => self.expand(i % 10_000),
            Hit::PanelClose => self.open = None,
            Hit::PanelReset(i) => self.request_module_reset(i),
            Hit::ModuleCode(i, paste) => self.module_code_press(i, paste),
            Hit::SectionReset => {
                self.reset_arm = None;
                if self.section_arm.is_some_and(|(cat, t)| cat == self.cat && self.now - t < 3.0) {
                    self.section_arm = None;
                    self.reset_section(self.cat);
                } else {
                    self.section_arm = Some((self.cat, self.now));
                    self.sound_tick(0.5);
                }
            }
            Hit::EditLayout => {
                self.want_editor = true;
                self.close(false);
            }
            _ => {}
        }
        true
    }

    /// Whose settings a row press means: the page's, or the open module's (a search lists modules too).
    fn owner(&self) -> Owner {
        match self.cat {
            Cat::Settings if self.q.is_empty() && self.settings_tab == 0 => Owner::Client,
            Cat::Theme if self.q.is_empty() => Owner::Theme,
            _ => self.open.filter(|&o| o < 10_000 && self.cat != Cat::Configs || !self.q.is_empty()).map_or(Owner::Client, |o| Owner::Module(if self.is_hud_module(o) { self.hud_sel } else { o })),
        }
    }

    /// What every press goes through first, the menu's and the HUD editor's: a key being waited for, an open list or
    /// picker (a press past them closes them). True when it was theirs.
    fn press_common(&mut self, h: Hit, x: f32, y: f32, button: i32) -> bool {
        if let Some(l) = self.listen {
            if button != 0 && button != 1 {
                let name = if button == 2 { "Middle".to_string() } else { format!("Mouse {}", button + 1) };
                if matches!(l, Listen::Bind(..) | Listen::Swap(_)) {
                    self.bind_mouse(&name);
                } else {
                    self.set_key(l, name);
                }
                return true;
            }
            if !matches!(h, Hit::PanelBind(_) | Hit::KeyRow(_) | Hit::Card(..) | Hit::TileBind(_) | Hit::RowBind(_) | Hit::Swap(9, _)) {
                self.listen = None;
                self.rec.clear();
            }
        }
        if self.pick.is_some() {
            if let Hit::Pick(part, idx) = h {
                if button == 0 {
                    self.pick_press(part, idx, x, y);
                }
                return true;
            }
            self.pick = None;
            if matches!(h, Hit::Colour(_)) {
                return true;
            }
        }
        if self.pop.is_some() && !matches!(h, Hit::Pop(_)) {
            self.pop = None;
            if matches!(h, Hit::Drop(_)) {
                return true;
            }
        }
        if !matches!(h, Hit::TextRow(_)) {
            self.text_focus = None;
        }
        false
    }

    /// A press on a settings row of `owner`; the right button puts a changed setting back. True when it was a row's.
    fn press_rows(&mut self, h: Hit, x: f32, y: f32, owner: Owner, button: i32) -> bool {
        if button == 1 {
            // the right button on a setting: the card of its binds (the round arrow still puts it back)
            if let Hit::Toggle(si) | Hit::Slider(si) | Hit::Choice(si, _) | Hit::Drop(si) | Hit::Chip(si, _) | Hit::Colour(si) | Hit::RowBind(si) | Hit::Action(si) = h {
                let at = self.hits.iter().rev().find(|q| q.1 == h).map_or([x, y, x, y], |q| q.0);
                let at = [x, at[1], x + 1.0, at[3]];
                if self.bind_does(BindTo::Setting(owner, si)).is_some() {
                    self.open_binds(BindTo::Setting(owner, si), at);
                } else {
                    self.reset_setting(owner, si);
                }
                return true;
            }
            if let Hit::KeyRow(si) | Hit::TextRow(si) | Hit::Reset(si) = h {
                self.reset_setting(owner, si);
                return true;
            }
            return false;
        }
        match h {
            Hit::Toggle(si) => {
                let mut lit = None;
                if let Kind::Toggle(v) = &mut self.set_list(owner)[si].kind {
                    *v = !*v;
                    lit = Some(*v);
                }
                if let Some(on) = lit {
                    self.switch_sound(on, 0.55);
                }
            }
            Hit::Slider(si) => {
                self.slider = Some((owner, si));
                self.slide(owner, si, x);
            }
            Hit::Choice(si, oi) => {
                let mut graphics = false;
                let s = &mut self.set_list(owner)[si];
                if let Kind::Choice { v, .. } = &mut s.kind {
                    *v = oi;
                    graphics = s.label == "Graphics";
                }
                if graphics {
                    self.apply_preset(oi);
                }
                self.sound_tick(0.5);
            }
            Hit::Drop(si) => {
                let at = self.hits.iter().find(|q| q.1 == h).map_or([x, y, x, y], |q| q.0);
                self.pop = Some(Pop::List { owner, set: si, at });
            }
            Hit::Chip(si, oi) => {
                let mut lit = None;
                if let Kind::Multi { on, .. } = &mut self.set_list(owner)[si].kind {
                    on[oi] = !on[oi];
                    lit = Some(on[oi]);
                }
                if let Some(on) = lit {
                    self.switch_sound(on, 0.45);
                }
            }
            Hit::Colour(si) => {
                let at = self.hits.iter().find(|q| q.1 == h).map_or([x, y, x, y], |q| q.0);
                self.open_picker(owner, si, at);
            }
            Hit::KeyRow(si) => self.listen = Some(Listen::Key(owner, si)),
            Hit::TextRow(si) => {
                self.text_focus = Some((owner, si));
                self.last_text = Some((owner, si));
            }
            Hit::Pop(oi) => {
                if let Some(Pop::List { owner, set, .. }) = self.pop.clone() {
                    let mut graphics = false;
                    let s = &mut self.set_list(owner)[set];
                    if let Kind::Choice { v, .. } = &mut s.kind {
                        *v = oi;
                        graphics = s.label == "Graphics";
                    }
                    if graphics {
                        self.apply_preset(oi);
                    }
                }
                self.pop = None;
                self.sound_tick(0.5);
            }
            Hit::Reset(si) => self.reset_setting(owner, si),
            Hit::Action(si) => {
                let label = self.set_ref(owner)[si].label;
                self.action(owner, label);
            }
            _ => return false,
        }
        true
    }

    fn reset_setting(&mut self, owner: Owner, si: usize) {
        let s = &mut self.set_list(owner)[si];
        if !s.changed() {
            return;
        }
        s.reset();
        let graphics = s.label == "Graphics";
        if graphics {
            if let Kind::Choice { v, .. } = self.set_ref(owner)[si].kind {
                self.apply_preset(v);
            }
        }
        if self.pick.as_ref().is_some_and(|p| p.owner == owner && p.set == si) {
            self.pick = None;
        }
        self.sound_tick(0.5);
    }

    /// A module's settings back as they came (its switch and its keys stay); Ctrl+Z brings them back.
    fn reset_module(&mut self, i: usize) {
        self.mods[i].set.iter_mut().for_each(Setting::reset);
        // Graphics' preset: its parts as it says
        if let Some(Kind::Choice { v, .. }) = self.mods[i].set.iter().find(|s| s.label == "Graphics").map(|s| s.kind.clone()) {
            self.apply_preset(v);
        }
        if self.pick.as_ref().is_some_and(|p| p.owner == Owner::Module(i)) {
            self.pick = None;
        }
        self.text_focus = None;
        self.switch_sound(false, 0.6);
        let name = if self.mods[i].plain { tr(self.mods[i].name) } else { i18n::nm(self.mods[i].name) }.to_string();
        self.toast(i18n::trn("{what}: as it came · Ctrl+Z brings it back", &[("what", name)]));
    }

    /// Whether a section's modules (those seen here) are not as they came: a setting, or a switch.
    fn section_dirty(&self, cat: Cat) -> bool {
        self.mods.iter().filter(|m| m.cat == cat && !self.hidden_here(m.id)).any(|m| (!m.plain && m.on != default_on(m.id)) || m.set.iter().any(Setting::changed))
    }

    /// A section's modules (those seen here) back as they came: their settings and switches (their keys stay); Ctrl+Z
    /// brings them back.
    fn reset_section(&mut self, cat: Cat) {
        for i in 0..self.mods.len() {
            if self.mods[i].cat != cat || self.hidden_here(self.mods[i].id) {
                continue;
            }
            let m = &mut self.mods[i];
            m.set.iter_mut().for_each(Setting::reset);
            if !m.plain {
                m.on = default_on(m.id);
            }
        }
        self.pick = None;
        self.text_focus = None;
        self.switch_sound(false, 0.8);
        self.toast(i18n::trn("{what}: as it came · Ctrl+Z brings it back", &[("what", tr(cat.name()).to_string())]));
    }

    /// A button row: the theme's or the HUD's reset, an element's own, the HUD's layout editor.
    fn action(&mut self, owner: Owner, label: &str) {
        match label {
            "Reset the theme" => {
                if !self.confirm_reset(reset::Target::Theme) { return; }
                self.theme.iter_mut().for_each(Setting::reset);
            }
            "Reset the interface" => {
                if !self.confirm_reset(reset::Target::Interface) { return; }
                for m in self.mods.iter_mut().filter(|m| m.cat == Cat::HudEl) {
                    m.set.iter_mut().for_each(Setting::reset);
                    m.on = true;
                }
                self.hud.reset_all();
            }
            "Reset this element" => {
                if let Owner::Module(i) = owner {
                    if !self.confirm_reset(reset::Target::Element(self.mods[i].id)) { return; }
                    self.mods[i].set.iter_mut().for_each(Setting::reset);
                    self.mods[i].on = true;
                    self.hud.reset_place(self.mods[i].id);
                }
            }
            "Edit the layout" => {
                self.want_editor = true;
                self.close(false);
            }
            "Edit the swing" => self.swing_open(),
            "Show the tour" => {
                self.tour_start();
                return;
            }
            // Fake Player's: one more where you look, none
            "Put one here" => {
                self.fakes_put();
                self.close(false);
            }
            "Remove all" => self.fakes_clear(),
            "Hidden players" => hud::chat_hidden(),
            "Deaths" => self.deaths_open(),
            "Friends" => self.friends_open(),
            "Messages" => {
                if let Owner::Module(i) = owner {
                    self.lines_open(i);
                }
            }
            "All blocks back" => self.inv_places_back(),
            "Copy the theme code" => {
                let code = configs::encode(&self.snapshot(tr("Theme"), configs::P_THEME | configs::P_HUD));
                self.copy(&code);
                self.toast(tr("The theme's code is copied: the menu's and the HUD's looks").to_string());
            }
            "Remove this element" => {
                if let Owner::Module(i) = owner {
                    self.custom_remove(i);
                }
            }
            "Choose a picture" => {
                if let Owner::Module(i) = owner {
                    hudx::choose_picture(self.mods[i].id);
                }
            }
            "Placeholders" => {
                let again = matches!(self.card_shut.take(), Some(Card::Placeholders { i: j, .. }) if Owner::Module(j) == owner);
                if let (Owner::Module(i), false) = (owner, again) {
                    let m = self.mouse;
                    self.card = Some(Card::Placeholders { i, at: [m[0], m[1], m[0], m[1]] });
                    self.card_field.clear();
                    self.card_scroll = 0.0;
                    self.card_scroll_to = 0.0;
                    self.card_at = self.now;
                    self.anim.remove(&ak(A_POP, 2));
                }
            }
            "Edit the words" => {
                self.words_page = true;
                self.binds_page = false;
                self.word_q.clear();
                self.word_focus = None;
                self.swap(true);
                self.scroll_to = 0.0;
                self.scroll = 0.0;
            }
            "Paste a theme code" => {
                self.paste_for = configs::P_THEME | configs::P_HUD;
                self.outbox.push("paste".into());
            }
            "Reset everything" => {
                if !matches!(self.card_shut.take(), Some(Card::Reset { .. })) {
                    let m = self.mouse;
                    self.card = Some(Card::Reset { at: [m[0] - 13.0 * self.r, m[1], m[0] + 13.0 * self.r, m[1]] });
                    self.card_field.clear();
                    self.card_at = self.now;
                    self.anim.remove(&ak(A_POP, 2));
                }
            }
            _ => {}
        }
        self.pick = None;
        self.sound_tick(0.7);
    }

    // ------------------------------------------------------------------ the colour picker
    fn open_picker(&mut self, owner: Owner, set: usize, at: [f32; 4]) {
        let s = &self.set_ref(owner)[set];
        let Kind::Colour(v) = s.kind else { return };
        let shown = self.colour_now(owner, s.label, v);
        let rgb = theme::pack([shown[0], shown[1], shown[2]]);
        self.pick = Some(Picker { owner, set, at, hsv: theme::hsv(rgb), drag: 0, hex: None, rect: [0.0; 4], geo: [0.0; 5] });
        self.anim.remove(&ak(A_POP, 1));
        self.sound_tick(0.6);
    }

    fn pick_col(&mut self, f: impl FnOnce(&mut data::Col)) {
        let Some((o, si)) = self.pick.as_ref().map(|p| (p.owner, p.set)) else { return };
        if let Some(Kind::Colour(v)) = self.set_list(o).get_mut(si).map(|s| &mut s.kind) {
            f(v);
        }
    }

    fn pick_hsv(&mut self) {
        let Some([h, s, v]) = self.pick.as_ref().map(|p| p.hsv) else { return };
        let rgb = theme::from_hsv(h, s, v);
        self.pick_col(|c| {
            c.rgb = rgb;
            c.auto = false;
        });
    }

    fn pick_press(&mut self, part: u8, idx: u8, x: f32, y: f32) {
        match part {
            1 | 2 | 3 | 8 => {
                if let Some(p) = self.pick.as_mut() {
                    p.drag = part;
                    p.hex = None;
                }
                self.pick_at(part, x, y);
            }
            4 => {
                let cur = self.pick.as_ref().map(|p| theme::from_hsv(p.hsv[0], p.hsv[1], p.hsv[2])).unwrap_or(0);
                if let Some(p) = self.pick.as_mut() {
                    p.hex = Some(format!("{cur:06X}"));
                }
            }
            5 => {
                let rgb = SWATCHES[idx as usize % SWATCHES.len()];
                if let Some(p) = self.pick.as_mut() {
                    p.hsv = theme::hsv(rgb);
                    p.hex = None;
                }
                self.pick_col(|c| {
                    c.rgb = rgb;
                    c.auto = false;
                });
                self.sound_tick(0.4);
            }
            6 => self.pick_col(|c| c.auto = !c.auto),
            7 => {
                self.pick_col(|c| {
                    c.chroma = if c.chroma > 0.0 { 0.0 } else { 1.0 };
                    c.auto = false;
                });
            }
            9 => {
                let Some((o, si)) = self.pick.as_ref().map(|p| (p.owner, p.set)) else { return };
                self.set_list(o)[si].reset();
                let s = &self.set_ref(o)[si];
                if let Kind::Colour(v) = s.kind {
                    let shown = self.colour_now(o, s.label, v);
                    let hsv = theme::hsv(theme::pack([shown[0], shown[1], shown[2]]));
                    if let Some(p) = self.pick.as_mut() {
                        p.hsv = hsv;
                        p.hex = None;
                    }
                }
                self.sound_tick(0.5);
            }
            _ => {}
        }
    }

    /// The pointer on the picker's part `part`: the wheel picks hue and saturation, the sliders the rest.
    fn pick_at(&mut self, part: u8, x: f32, y: f32) {
        let Some(g) = self.pick.as_ref().map(|p| p.geo) else { return };
        let k = ((x - g[3]) / (g[4] - g[3]).max(1.0)).clamp(0.0, 1.0);
        match part {
            1 => {
                let (dx, dy) = (x - g[0], y - g[1]);
                let h = ((-dy).atan2(dx) / std::f32::consts::TAU).rem_euclid(1.0);
                let s = ((dx * dx + dy * dy).sqrt() / g[2].max(1.0)).min(1.0);
                if let Some(p) = self.pick.as_mut() {
                    p.hsv[0] = h;
                    p.hsv[1] = s;
                    if p.hsv[2] < 0.05 {
                        p.hsv[2] = 1.0;
                    }
                }
                self.pick_hsv();
            }
            2 => {
                if let Some(p) = self.pick.as_mut() {
                    p.hsv[2] = k;
                }
                self.pick_hsv();
            }
            3 => self.pick_col(|c| c.a = (k * 100.0).round() / 100.0),
            8 => self.pick_col(|c| c.chroma = 0.1 + 2.9 * k),
            _ => {}
        }
    }

    /// The picker over whatever is drawn (the menu, or the HUD's editor): a wheel of hue and saturation, brightness and
    /// opacity, the hex, chroma, ready colours, following the theme, back to its default.
    fn picker(&mut self, c: &mut Canvas) {
        let Some(pk) = self.pick.clone() else {
            self.anim.remove(&ak(A_POP, 1));
            return;
        };
        let Some(s) = self.set_ref(pk.owner).get(pk.set).cloned() else {
            self.pick = None;
            return;
        };
        let Kind::Colour(col) = s.kind else {
            self.pick = None;
            return;
        };
        let can_follow = matches!(s.def, Kind::Colour(d) if d.auto);
        let r = self.r;
        let k = self.an(ak(A_POP, 1), 1.0, 14.0, 0.0);
        let w = 18.5 * r;
        let h = 33.5 * r + if col.chroma > 0.0 { 2.5 * r } else { 0.0 };
        let mut x = pk.at[2] - w;
        let mut y = pk.at[3] + 0.5 * r;
        if y + h > self.h - 8.0 {
            y = (pk.at[1] - h - 0.5 * r).max(8.0);
        }
        x = x.clamp(8.0, (self.w - w - 8.0).max(8.0));
        let rr = [x, y - (1.0 - k) * 0.5 * r, x + w, y + h - (1.0 - k) * 0.5 * r];
        let base = c.alpha;
        c.alpha = k;
        c.shadow(rr, r, 3.0 * r, grey(0.0, 0.6));
        let g = pal().glass;
        c.rect(rr, r, [g[0].max(0.07), g[1].max(0.07), g[2].max(0.075), 0.97]);
        self.hits.push((rr, Hit::Pick(0, 0)));
        let (x0, x1) = (rr[0] + 1.2 * r, rr[2] - 1.2 * r);
        // what it is, and the colour itself
        c.text(x0, rr[1] + 1.95 * r, 0.86 * r, Face::Semi, tr(s.label), pal().t1);
        let shown = self.colour_now(pk.owner, s.label, col);
        let sw = [x1 - 2.6 * r, rr[1] + 0.95 * r, x1, rr[1] + 2.35 * r];
        c.rect_checker(sw, 0.5 * r, theme::fade(shown, 0.0), shown);
        c.rect(sw, 0.5 * r, shown);
        // the wheel: hue round it, saturation out from the middle; the knob where the colour is
        let rad = 7.0 * r;
        let wc = [(rr[0] + rr[2]) / 2.0, rr[1] + 3.4 * r + rad];
        c.glow(wc, rad * 1.25, grey(0.0, 0.35));
        c.wheel(wc, rad, 1.0);
        let [hh, ss, vv] = pk.hsv;
        // darker as the brightness goes down (the wheel itself stays bright: the slider below holds the brightness)
        if vv < 0.98 {
            c.rect([wc[0] - rad, wc[1] - rad, wc[0] + rad, wc[1] + rad], rad, grey(0.0, (1.0 - vv) * 0.55));
        }
        let ang = hh * std::f32::consts::TAU;
        let kp = [wc[0] + ang.cos() * ss * rad, wc[1] - ang.sin() * ss * rad];
        c.shadow([kp[0] - 0.7 * r, kp[1] - 0.7 * r, kp[0] + 0.7 * r, kp[1] + 0.7 * r], 0.7 * r, 0.5 * r, grey(0.0, 0.6));
        c.rect([kp[0] - 0.7 * r, kp[1] - 0.7 * r, kp[0] + 0.7 * r, kp[1] + 0.7 * r], 0.7 * r, grey(1.0, 1.0));
        let pure = theme::rgb_of(theme::from_hsv(hh, ss, vv));
        c.rect([kp[0] - 0.5 * r, kp[1] - 0.5 * r, kp[0] + 0.5 * r, kp[1] + 0.5 * r], 0.5 * r, [pure[0], pure[1], pure[2], 1.0]);
        self.hits.push(([wc[0] - rad, wc[1] - rad, wc[0] + rad, wc[1] + rad], Hit::Pick(1, 0)));
        // brightness and opacity
        let full = theme::rgb_of(theme::from_hsv(hh, ss, 1.0));
        let bar = |c: &mut Canvas, yy: f32, f: f32| {
            let t = [x0 + f * (x1 - x0), yy];
            c.shadow([t[0] - 0.62 * r, t[1] - 0.62 * r, t[0] + 0.62 * r, t[1] + 0.62 * r], 0.62 * r, 0.4 * r, grey(0.0, 0.55));
            c.rect([t[0] - 0.62 * r, t[1] - 0.62 * r, t[0] + 0.62 * r, t[1] + 0.62 * r], 0.62 * r, grey(1.0, 1.0));
        };
        let vy = wc[1] + rad + 1.9 * r;
        c.rect_lit([x0, vy - 0.42 * r, x1, vy + 0.42 * r], 0.42 * r, grey(0.0, 1.0), [full[0], full[1], full[2], 1.0], 1.0);
        bar(c, vy, vv);
        self.hits.push(([x0 - 0.6 * r, vy - 1.0 * r, x1 + 0.6 * r, vy + 1.0 * r], Hit::Pick(2, 0)));
        let ay = vy + 2.1 * r;
        let cur = theme::rgb_of(theme::from_hsv(hh, ss, vv));
        c.rect_checker([x0, ay - 0.42 * r, x1, ay + 0.42 * r], 0.42 * r, [cur[0], cur[1], cur[2], 0.0], [cur[0], cur[1], cur[2], 1.0]);
        bar(c, ay, col.a);
        self.hits.push(([x0 - 0.6 * r, ay - 1.0 * r, x1 + 0.6 * r, ay + 1.0 * r], Hit::Pick(3, 0)));
        // the hex, typed; chroma (a hue that turns by itself), its speed
        let ry = ay + 1.6 * r;
        let hb = [x0, ry, x0 + 8.0 * r, ry + 2.0 * r];
        let editing = pk.hex.is_some();
        let hv = self.an(ak(A_HOV, 9800), if editing || self.hover == Hit::Pick(4, 0) { 1.0 } else { 0.0 }, 12.0, 0.0);
        c.rect(hb, 0.55 * r, mix(pal().f1, pal().f3, hv));
        let hex = pk.hex.clone().unwrap_or_else(|| format!("{:06X}", theme::from_hsv(hh, ss, vv)));
        let tw = c.text(hb[0] + 0.7 * r, ry + 1.35 * r, 0.8 * r, Face::Mono, &format!("#{hex}"), pal().t1);
        if editing && (self.now * 2.0).fract() < 0.55 {
            c.rect([hb[0] + 0.75 * r + tw, ry + 0.5 * r, hb[0] + 0.75 * r + tw + 1.5, ry + 1.5 * r], 0.0, pal().accent);
        }
        self.hits.push((hb, Hit::Pick(4, 0)));
        c.text(hb[2] + 1.0 * r, ry + 1.35 * r, 0.8 * r, Face::Regular, tr("Chroma"), pal().t2);
        let cv = self.an(ak(A_SET, 9801), if col.chroma > 0.0 { 1.0 } else { 0.0 }, 10.0, 0.0);
        let chs = [x1 - 2.02 * r, ry + 0.4 * r, x1, ry + 1.6 * r];
        switch(c, chs, cv, 0.0, true);
        self.hits.push(([hb[2] + 0.6 * r, ry, x1, ry + 2.0 * r], Hit::Pick(7, 0)));
        let mut yy = ry + 2.6 * r;
        if col.chroma > 0.0 {
            c.text(x0, yy + 1.2 * r, 0.76 * r, Face::Regular, tr("Speed"), pal().t3);
            let (sx0, sx1) = (x0 + 4.0 * r, x1);
            c.rect([sx0, yy + 0.8 * r, sx1, yy + 1.1 * r], r, grey(1.0, 0.1));
            let f = ((col.chroma - 0.1) / 2.9).clamp(0.0, 1.0);
            // a rainbow up to where it is set
            for i in 0..12 {
                let a0 = sx0 + (sx1 - sx0) * f * i as f32 / 12.0;
                let a1 = sx0 + (sx1 - sx0) * f * (i + 1) as f32 / 12.0;
                let rc = theme::rgb_of(theme::from_hsv(i as f32 / 12.0 + self.now * col.chroma * 0.25, 0.8, 1.0));
                c.rect([a0, yy + 0.8 * r, a1, yy + 1.1 * r], 0.0, [rc[0], rc[1], rc[2], 1.0]);
            }
            let t = [sx0 + (sx1 - sx0) * f, yy + 0.95 * r];
            c.rect([t[0] - 0.5 * r, t[1] - 0.5 * r, t[0] + 0.5 * r, t[1] + 0.5 * r], 0.5 * r, grey(1.0, 1.0));
            self.hits.push(([sx0 - 0.5 * r, yy, sx1 + 0.5 * r, yy + 2.0 * r], Hit::Pick(8, 0)));
            if let Some(p) = self.pick.as_mut() {
                p.geo = [wc[0], wc[1], rad, sx0, sx1];
            }
            yy += 2.5 * r;
        }
        // the ready colours
        let sp = (x1 - x0) / 6.0;
        for (i, &sc) in SWATCHES.iter().enumerate() {
            let cc = [x0 + sp * ((i % 6) as f32 + 0.5), yy + 1.0 * r + (i / 6) as f32 * 2.2 * r];
            let hit = Hit::Pick(5, i as u8);
            let hv = self.an(ak(A_HOV, 9810 + i), if self.hover == hit { 1.0 } else { 0.0 }, 14.0, 0.0);
            let rad = 0.62 * r * (1.0 + 0.2 * hv);
            if !col.auto && col.rgb == sc {
                c.ring(cc, rad + 0.3 * r, 1.5, pal().t1);
            }
            c.rect([cc[0] - rad, cc[1] - rad, cc[0] + rad, cc[1] + rad], rad, draw::hex(sc, 1.0));
            self.hits.push(([cc[0] - sp / 2.0, cc[1] - 1.1 * r, cc[0] + sp / 2.0, cc[1] + 1.1 * r], hit));
        }
        yy += 4.6 * r;
        // follow the theme (an element's colours), back to the default
        let by = yy;
        let bw = if can_follow { (x1 - x0 - 0.6 * r) / 2.0 } else { x1 - x0 };
        if can_follow {
            let b = [x0, by, x0 + bw, by + 2.1 * r];
            let hv = self.an(ak(A_HOV, 9830), if self.hover == Hit::Pick(6, 0) { 1.0 } else { 0.0 }, 12.0, 0.0);
            c.rect(b, 0.6 * r, if col.auto { pal().f4 } else { mix(pal().f2, pal().f3, hv) });
            c.text_c((b[0] + b[2]) / 2.0, by + 1.38 * r, 0.78 * r, Face::Medium, tr("As the theme"), if col.auto { pal().t1 } else { pal().t2 });
            self.hits.push((b, Hit::Pick(6, 0)));
        }
        let b = [x1 - bw, by, x1, by + 2.1 * r];
        let hv = self.an(ak(A_HOV, 9831), if self.hover == Hit::Pick(9, 0) { 1.0 } else { 0.0 }, 12.0, 0.0);
        c.rect(b, 0.6 * r, mix(pal().f2, pal().f3, hv));
        let lw = fonts().width(Face::Medium, 0.78 * r, tr("Default"));
        let mx = (b[0] + b[2]) / 2.0;
        reset_icon(c, [mx - lw / 2.0 - 0.75 * r, by + 1.05 * r], 0.34 * r, mix(pal().t3, pal().t1, hv));
        c.text(mx - lw / 2.0 + 0.1 * r, by + 1.38 * r, 0.78 * r, Face::Medium, tr("Default"), mix(pal().t2, pal().t1, hv));
        self.hits.push((b, Hit::Pick(9, 0)));
        if col.chroma <= 0.0 {
            if let Some(p) = self.pick.as_mut() {
                p.geo = [wc[0], wc[1], rad, x0, x1];
            }
        }
        if let Some(p) = self.pick.as_mut() {
            p.rect = rr;
        }
        c.alpha = base;
    }

    fn apply_preset(&mut self, p: usize) {
        let (_, res, list) = PRESETS[p];
        for m in self.mods.iter_mut().filter(|m| m.cat == Cat::Graphics) {
            for s in m.set.iter_mut() {
                if let Kind::Toggle(v) = &mut s.kind {
                    if ENGINE_KEYS.contains(&s.label) {
                        *v = list.contains(&s.label);
                    }
                }
                if s.label == "World resolution" {
                    if let Kind::Slider { v, .. } = &mut s.kind {
                        *v = res;
                    }
                }
            }
        }
    }

    fn release(&mut self) {
        self.undo_poke(Some(false));
        self.drag = None;
        self.slider = None;
        self.swap_release();
        self.cosm_release();
        if let Some(p) = self.pick.as_mut() {
            p.drag = 0;
        }
    }

    /// The wheel only ever scrolls: a setting under the pointer is changed by the mouse's buttons, never by passing it.
    fn wheel(&mut self, dy: f32) {
        self.undo_poke(None);
        if self.tour.is_some() {
            return;
        }
        if matches!(self.card, Some(Card::Lang { .. } | Card::Placeholders { .. } | Card::Alts { .. } | Card::Deaths { .. } | Card::Friends { .. } | Card::Lines { .. })) && inside(self.card_rect, self.mouse) {
            self.card_scroll_to -= dy * 3.2 * self.r;
            return;
        }
        if self.cat == Cat::Cosmetics && self.q.is_empty() && !self.binds_page && !self.words_page && self.cosm_wheel(dy) {
            return;
        }
        self.scroll_to = (self.scroll_to - dy * 3.2 * self.r).clamp(0.0, self.content);
    }

    fn set_key(&mut self, l: Listen, name: String) {
        match l {
            Listen::Bind(..) | Listen::Swap(_) => {}
            Listen::Key(o, si) => {
                if let Kind::Key(v) = &mut self.set_list(o)[si].kind {
                    *v = name;
                }
            }
        }
        self.listen = None;
    }

    /// A key: 0 not ours, 1 taken, 2 the menu should go.
    fn key(&mut self, escape: bool, enter: bool, backspace: bool, name: String, menu_key: bool) -> i32 {
        self.undo_poke(None);
        self.bind_key_down(&name);
        if self.tour.is_some() && self.tour_key(escape, enter, &name) {
            return 1;
        }
        // a bind being recorded takes every key (and the letter after it)
        if self.bind_key(escape, backspace, &name) {
            return 1;
        }
        if let Some(hex) = self.pick.as_ref().and_then(|p| p.hex.clone()) {
            if backspace {
                if let Some(p) = self.pick.as_mut().and_then(|p| p.hex.as_mut()) {
                    p.pop();
                }
            } else if enter {
                let full = if hex.len() == 3 { hex.chars().flat_map(|c| [c, c]).collect() } else { hex.clone() };
                if let Ok(v) = u32::from_str_radix(&full, 16).map(|v| v & 0xffffff).map_err(|_| ()).and_then(|v| if full.len() == 6 { Ok(v) } else { Err(()) }) {
                    if let Some(p) = self.pick.as_mut() {
                        p.hsv = theme::hsv(v);
                    }
                    self.pick_col(|c| {
                        c.rgb = v;
                        c.auto = false;
                    });
                }
                if let Some(p) = self.pick.as_mut() {
                    p.hex = None;
                }
            } else if escape {
                if let Some(p) = self.pick.as_mut() {
                    p.hex = None;
                }
            }
            return 1;
        }
        if escape && self.pick.is_some() {
            self.pick = None;
            return 1;
        }
        if self.words_page && self.card.is_none() && self.words_key(escape, enter, backspace) {
            return 1;
        }
        if escape && self.card.is_some() {
            self.card = None;
            return 1;
        }
        if matches!(self.card, Some(Card::Placeholders { .. })) {
            if backspace {
                self.card_field.pop();
                self.card_scroll_to = 0.0;
            }
            return 1;
        }
        if matches!(self.card, Some(Card::Alts { .. })) {
            if backspace {
                self.card_field.pop();
                self.alt_wrong = None;
            } else if enter {
                self.alts_enter();
            }
            return 1;
        }
        if matches!(self.card, Some(Card::Friends { .. })) {
            if backspace {
                self.card_field.pop();
            } else if enter {
                self.friends_enter();
            }
            return 1;
        }
        if matches!(self.card, Some(Card::Lines { .. })) {
            if name=="V" && self.gheld.iter().any(|k| matches!(k.as_str(), "Left Ctrl"|"Right Ctrl")) {
                self.outbox.push("paste".into());
                return 1;
            }
            if backspace {
                self.card_field.pop();
                return 1;
            } else if enter {
                self.lines_enter();
                return 1;
            } else if escape && self.lines_escape() {
                return 1;
            }
            if !escape {
                return 1;
            }
        }
        if matches!(self.card, Some(Card::Lang { .. })) {
            if backspace {
                self.card_field.pop();
                self.card_scroll_to = 0.0;
            } else if enter {
                if let Some(&li) = self.langs_found().first() {
                    self.lang_choose(li);
                }
            }
            return 1;
        }
        if matches!(self.card,Some(Card::Cloud {tab:3,..})) {
            if backspace {self.card_field.pop();}
            else if enter {self.cloud_press(Hit::Cloud(19,0,0),0);}
            else if name=="V" && self.gheld.iter().any(|k|matches!(k.as_str(),"Left Ctrl"|"Right Ctrl")) {
                self.outbox.push("paste".into());
            }
            return 1;
        }
        if matches!(self.card, Some(Card::ModuleImport { .. })) {
            if enter { self.cfg_card_press(Hit::Card(modulecode::APPLY, 0)); }
            return 1;
        }
        if matches!(self.card, Some(Card::NewCfg { .. } | Card::Cfg { .. } | Card::Import { .. } | Card::Reset { .. })) {
            if backspace {
                self.card_field.pop();
            } else if enter {
                let part = match self.card {
                    Some(Card::NewCfg { .. }) => 42,
                    Some(Card::Import { .. }) => 44,
                    Some(Card::Reset { .. }) => 41,
                    _ => 46,
                };
                self.cfg_card_press(Hit::Card(part, 0));
            }
            return 1;
        }
        if self.keys_key(escape, backspace, &name) || self.sticker_key(escape, enter, backspace) || self.tag_key(escape, enter, backspace) || self.sets_key(escape, enter, backspace) || self.hand_key(&name) {
            return 1;
        }
        if let Some(l) = self.listen {
            self.eat = Some(name.clone());
            let n = if escape || backspace || name == "Delete" { "None".to_string() } else { name };
            self.set_key(l, n);
            return 1;
        }
        if let Some((o, si)) = self.text_focus {
            if backspace {
                if let Kind::Text(v) = &mut self.set_list(o)[si].kind {
                    v.pop();
                }
            }
            if escape || enter {
                self.text_focus = None;
            }
            return 1;
        }
        // something over the menu that types nothing (a card, a list, the colour picker): Backspace and Enter are not
        // the search's under it
        if (self.card.is_some() || self.pop.is_some() || self.pick.is_some()) && (backspace || enter) {
            return 1;
        }
        // Ctrl+Z takes the last change back, Ctrl+Y (or Ctrl+Shift+Z) does it again
        let held = |a: &str, b: &str| self.gheld.iter().any(|k| k == a || k == b);
        if held("Left Ctrl", "Right Ctrl") && (name == "Z" || name == "Y") {
            let shift = held("Left Shift", "Right Shift");
            self.undo_go(name == "Z" && !shift);
            return 1;
        }
        if self.typing || !self.q.is_empty() {
            if backspace {
                if self.q.chars().count() == 1 {
                    self.swap(true);
                }
                self.q.pop();
                return 1;
            }
            if enter {
                if let Some(&i) = self.list().first() {
                    self.typing = false;
                    self.expand(i);
                }
                return 1;
            }
            if escape {
                if !self.q.is_empty() {
                    self.swap(true);
                }
                self.q.clear();
                self.typing = false;
                return 1;
            }
        }
        if escape {
            if self.pop.is_some() {
                self.pop = None;
                return 1;
            }
            if self.open.is_some() && !matches!(self.cat, Cat::Settings | Cat::Theme) {
                self.open = None;
                return 1;
            }
            // the Cosmetics page: a level up (a panel of the look, the look, the item), the menu closed at the top
            if self.cat == Cat::Cosmetics && self.cosm_escape() {
                return 1;
            }
            self.close(false);
            return 2;
        }
        if menu_key && !self.typing {
            self.close(false);
            return 2;
        }
        0
    }

    fn char_typed(&mut self, c: char) {
        self.undo_poke(None);
        if self.listen.is_some() || self.eat.is_some() || c.is_control() {
            return;
        }
        if let Some(h) = self.pick.as_mut().and_then(|p| p.hex.as_mut()) {
            if c.is_ascii_hexdigit() && h.len() < 6 {
                h.push(c.to_ascii_uppercase());
            }
            return;
        }
        if matches!(self.card, Some(Card::Alts { .. })) {
            if self.card_field.chars().count() < 16 && !c.is_whitespace() {
                self.card_field.push(c);
                self.alt_wrong = None;
            }
            return;
        }
        if matches!(self.card, Some(Card::Friends { .. })) {
            if self.card_field.chars().count() < 16 && (c.is_ascii_alphanumeric() || c == '_') {
                self.card_field.push(c);
            }
            return;
        }
        if matches!(self.card, Some(Card::Lines { .. })) {
            self.message_char(c);
            return;
        }
        if matches!(self.card, Some(Card::Lang { .. } | Card::Placeholders { .. })) {
            if self.card_field.chars().count() < 24 {
                self.card_field.push(c);
                self.card_scroll_to = 0.0;
            }
            return;
        }
        if matches!(self.card,Some(Card::Cloud {tab:3,..})) {
            if self.card_field.len()<32 && (c.is_ascii_alphanumeric() || c=='-') {
                self.card_field.push(c.to_ascii_uppercase());
            }
            return;
        }
        if matches!(self.card, Some(Card::ModuleImport { .. })) { return; }
        if matches!(self.card, Some(Card::NewCfg { .. } | Card::Cfg { .. } | Card::Import { .. } | Card::Reset { .. })) {
            if self.card_field.chars().count() < 40 {
                self.card_field.push(c);
                self.card_arm = false;
            }
            return;
        }
        if self.sticker_char(c) || self.tag_char(c) || self.sets_char(c) {
            return;
        }
        if let Some((o, si)) = self.text_focus {
            let long = matches!(o, Owner::Module(i) if self.mods[i].id.starts_with("x-") || self.mods[i].cat == Cat::HudEl);
            if let Kind::Text(v) = &mut self.set_list(o)[si].kind {
                if v.chars().count() < if long { 240 } else { 32 } {
                    v.push(c);
                }
            }
            return;
        }
        if self.words_page && self.card.is_none() {
            self.words_char(c);
            return;
        }
        if self.card.is_some() || self.pop.is_some() || self.pick.is_some() {
            return;
        }
        if c == '/' && !self.typing {
            self.typing = true;
            return;
        }
        if self.typing || c.is_alphanumeric() {
            // a Russian layout typed means Latin keys
            const RU: &str = "йцукенгшщзхъфывапролджэячсмитьбю";
            const EN: &str = "qwertyuiop[]asdfghjkl;'zxcvbnm,.";
            let lc = c.to_lowercase().next().unwrap_or(c);
            let ch = RU.chars().position(|x| x == lc).and_then(|i| EN.chars().nth(i)).unwrap_or(c);
            if self.q.is_empty() {
                // the section crosses over to the search's results
                self.swap(true);
            }
            if self.q.chars().count() < 24 {
                self.q.push(ch);
            }
            self.typing = true;
            self.open = None;
            self.scroll_to = 0.0;
        }
    }

    /// The settings from the disk, once (the HUD needs them from the game's first frame, before the menu opens).
    fn ensure_loaded(&mut self) {
        if !self.loaded && self.dir.is_some() {
            if let Some(dir) = self.dir.clone() {
                self.cloud_recovery_blocked = !cloud::recover(&dir);
                if self.cloud_recovery_blocked {
                    return;
                }
            }
            self.load();
            self.loaded = true;
        }
    }

    fn open_menu(&mut self, w: f32, h: f32) {
        self.ensure_loaded();
        self.w = w;
        self.h = h;
        self.swap_drag = None;
        if self.phase != Phase::Open {
            self.phase = Phase::Opening;
            self.phase_at = self.clock.elapsed().as_secs_f32();
            self.opened_at = self.phase_at;
            self.shown_at = -1.0;
            if let Some(d) = self.dev.as_mut() {
                d.restart();
                let wall = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
                eprintln!("[gui dev] open at {wall}");
            }
            self.drag = None;
            self.slider = None;
            self.listen = None;
            self.pop = None;
            self.card = None;
            self.gheld.clear();
            self.rec.clear();
            self.eat = None;
            if self.langs.is_empty() {
                self.outbox.push("langs".into());
            }
        }
    }

    /// The window folds into its line; `now`: it is gone already (the screen was taken away).
    fn close(&mut self, now: bool) {
        self.reset_arm = None;
        self.section_arm = None;
        // a key a bind took, still held as the menu goes: what the chat or a field types after is theirs
        self.eat = None;
        // the tour shut with the menu is done
        self.tour = None;
        // Auto Swap's drag: its button's release goes to the game
        self.swap_drag = None;
        if now {
            if self.phase != Phase::Closed {
                self.phase = Phase::Closed;
                self.save();
            }
            return;
        }
        if matches!(self.phase, Phase::Open | Phase::Opening) {
            self.phase = Phase::Closing;
            self.phase_at = self.clock.elapsed().as_secs_f32();
            self.pop = None;
            self.pick = None;
            self.listen = None;
            self.typing = false;
            self.text_focus = None;
            self.drag = None;
            self.slider = None;
        }
    }

    // ------------------------------------------------------------------ what is kept between sessions
    fn file(&self) -> Option<std::path::PathBuf> {
        self.dir.as_ref().map(|d| d.join("cursed-recode").join("menu.txt"))
    }

    fn save(&self) {
        if self.cloud_recovery_blocked {
            return;
        }
        let Some(f) = self.file() else { return };
        cloud::changed();
        // a server's profile on: what was changed there is its, the player's own is the file's
        self.profile_keep();
        let mut out = self.own_dump();
        // the theme's version: 2 the main menu's colours (14.10)
        out.push_str("theme.#v=3\n");
        // Click Pearl's: 2 it throws (23.10)
        out.push_str("click-pearl.#v=2\n");
        let _ = std::fs::create_dir_all(f.parent().unwrap());
        let _ = std::fs::write(f, out);
    }

    fn load(&mut self) {
        let Some(f) = self.file() else { return };
        let text = match std::fs::read_to_string(f) {
            Ok(text) => text,
            Err(e) => {
                // Only absent settings mean a new player; unreadable existing settings are not a fresh install.
                if e.kind() == std::io::ErrorKind::NotFound {
                    self.first_run();
                }
                return;
            }
        };
        let mut vals: HashMap<String, String> = HashMap::new();
        for l in text.lines() {
            if let Some((k, v)) = l.split_once('=') {
                vals.insert(k.to_string(), v.to_string());
            }
        }
        self.apply_vals(&vals, configs::P_ALL);
        // Click Pearl was a card alone before 23.10, its "Only from the hotbar" on: it takes from the whole inventory now
        if !vals.contains_key("click-pearl.#v") {
            if let Some(s) = self.mods.iter_mut().find(|m| m.id == swap::PEARL).and_then(|m| m.set.iter_mut().find(|s| s.label == "Only from the hotbar")) {
                s.reset();
            }
        }
        // the HUD's switch was a setting of its look before the Hud module
        if !vals.contains_key("hud.#on") && vals.get("hud-look.Show the interface").map(String::as_str) == Some("0") {
            if let Some(m) = self.mods.iter_mut().find(|m| m.id == "hud") {
                m.on = false;
            }
        }
        // the HUD's look was a module of its own before it became a part of the Theme (14.10): its values come over
        let moved: HashMap<String, String> = data::HUD_LOOK_MOVED
            .iter()
            .filter(|(_, new)| !vals.contains_key(&format!("theme.{new}")))
            .filter_map(|(old, new)| vals.get(&format!("hud-look.{old}")).map(|v| (format!("theme.{new}"), v.clone())))
            .collect();
        if !moved.is_empty() {
            self.apply_vals(&moved, configs::P_THEME);
        }
        // before the HUD wrote the server's words itself (15.10): the boss bars and the title were no cards (their
        // Background off by default) and the title sat lower; as it came, it goes to the new
        if vals.get("theme.#v").and_then(|v| v.parse::<u32>().ok()).unwrap_or(0) < 3 {
            for id in ["bossbars", "titles"] {
                if vals.get(&format!("{id}.Background")).map(String::as_str) == Some("0") {
                    if let Some(s) = self.mods.iter_mut().find(|m| m.id == id).and_then(|m| m.set.iter_mut().find(|s| s.label == "Background")) {
                        s.reset();
                    }
                }
            }
            if vals.get("hud.titles.place").map(String::as_str) == Some("4/0/-40") {
                if let Some(d) = self.hud.defaults.get("titles").copied() {
                    self.hud.places.insert("titles", d);
                }
            }
        }
        // settings of before the main menu's colours (14.10): what was left as it came goes to the new colours; what
        // the player had changed stays theirs
        if !vals.contains_key("theme.#v") {
            const OLD: &[(&str, &str)] = &[
                ("theme.Accent", "ffffff/1/0/0"), ("theme.Text", "f4f4f4/1/0/0"), ("theme.Secondary text", "bdbdbd/1/0/0"), ("theme.Switches", "ffffff/1/0/0"),
                ("theme.Glass", "0a0a0b/0.7/0/0"), ("theme.Blur", "70"), ("theme.World colour through", "60"), ("theme.Sidebar shade", "20"), ("theme.Dim the world", "45"),
                ("hud-look.Text", "f4f4f4/1/0/0"), ("hud-look.Secondary text", "8d8d8d/1/0/0"), ("hud-look.Accent", "ffffff/1/0/0"), ("hud-look.Glass", "0a0a0c/0.62/0/0"),
            ];
            for (key, old) in OLD {
                if vals.get(*key).map(String::as_str) != Some(*old) {
                    continue;
                }
                let (owner, label) = key.split_once('.').unwrap();
                let list = if owner == "theme" { Some(&mut self.theme) } else { self.mods.iter_mut().find(|m| m.id == owner).map(|m| &mut m.set) };
                if let Some(s) = list.and_then(|l| l.iter_mut().find(|s| s.label == label)) {
                    s.reset();
                }
            }
        }
    }
}

struct Head {
    id: &'static str,
    cat: Cat,
    sub: Option<&'static str>,
    name: &'static str,
    desc: &'static str,
    on: bool,
    binds: Vec<binds::Bind>,
    plain: bool,
}

fn group_on(m: &Module) -> String {
    let n = m.set.iter().filter(|s| matches!(s.kind, Kind::Toggle(_))).count();
    let on = m.set.iter().filter(|s| matches!(s.kind, Kind::Toggle(true))).count();
    format!("{on}/{n}")
}

/// A switch: the track lights up, the knob dark on it.
fn switch(c: &mut Canvas, r: [f32; 4], on: f32, hover: f32, small: bool) {
    let h = r[3] - r[1];
    let pad = h * if small { 0.15 } else { 0.147 };
    let kn = h - 2.0 * pad;
    if on > 0.01 {
        c.glow([(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0], (r[2] - r[0]) * 0.9, grey(1.0, 0.1 * on));
    }
    let sw = pal().switch;
    c.rect_lit(r, h / 2.0, mix(grey(1.0, 0.1), theme::fade(sw, 0.62), on), mix(grey(1.0, 0.1), sw, on), 1.0);
    let x = r[0] + pad + (r[2] - r[0] - 2.0 * pad - kn) * on;
    let kcol = mix(mix(grey(0.56, 1.0), grey(0.69, 1.0), hover), grey(0.05, 1.0), on);
    c.rect([x, r[1] + pad, x + kn, r[1] + pad + kn], kn / 2.0, kcol);
}

/// A round arrow: back to the default.
/// Whether a module comes on (as data.rs makes it), by its id.
fn default_on(id: &str) -> bool {
    static ON: std::sync::OnceLock<HashMap<&'static str, bool>> = std::sync::OnceLock::new();
    ON.get_or_init(|| data::modules().into_iter().map(|m| (m.id, m.on)).collect()).get(id).copied().unwrap_or(false)
}

fn reset_icon(c: &mut Canvas, cc: [f32; 2], s: f32, col: Rgba) {
    let mut pts = Vec::new();
    for i in 0..=12 {
        let a = 0.35 + i as f32 / 12.0 * 4.9;
        pts.push([cc[0] + a.cos() * s, cc[1] - a.sin() * s]);
    }
    c.polyline(&pts, 1.4, col);
    // the head at its start, pointing on round
    let a = 0.35f32;
    let tip = [cc[0] + a.cos() * s, cc[1] - a.sin() * s];
    c.line(tip, [tip[0] - 0.55 * s, tip[1] - 0.15 * s], 1.4, col, 0.0);
    c.line(tip, [tip[0] + 0.05 * s, tip[1] - 0.6 * s], 1.4, col, 0.0);
}

/// A globe: the language.
fn globe(c: &mut Canvas, cc: [f32; 2], s: f32, col: Rgba) {
    c.ring(cc, s, 1.3, col);
    c.line([cc[0] - s, cc[1]], [cc[0] + s, cc[1]], 1.1, col, 0.0);
    let mut pts = Vec::new();
    for i in 0..=12 {
        let a = std::f32::consts::PI * (i as f32 / 12.0 - 0.5);
        pts.push([cc[0] + a.cos() * s * 0.45, cc[1] + a.sin() * s]);
    }
    c.polyline(&pts, 1.1, col);
    let mut pts = Vec::new();
    for i in 0..=12 {
        let a = std::f32::consts::PI * (i as f32 / 12.0 + 0.5);
        pts.push([cc[0] + a.cos() * s * 0.45, cc[1] + a.sin() * s]);
    }
    c.polyline(&pts, 1.1, col);
}

/// Two arrows apart: the settings open.
fn expand_icon(c: &mut Canvas, cc: [f32; 2], s: f32, col: Rgba) {
    let k = s / 24.0;
    let p = |x: f32, y: f32| [cc[0] + (x - 12.0) * k, cc[1] + (y - 12.0) * k];
    c.polyline(&[p(14.0, 4.0), p(20.0, 4.0), p(20.0, 10.0)], 1.4, col);
    c.line(p(20.0, 4.0), p(13.5, 10.5), 1.4, col, 0.0);
    c.polyline(&[p(10.0, 20.0), p(4.0, 20.0), p(4.0, 14.0)], 1.4, col);
    c.line(p(4.0, 20.0), p(10.5, 13.5), 1.4, col, 0.0);
}

fn chevron(c: &mut Canvas, cc: [f32; 2], s: f32, col: Rgba, up: bool) {
    let d = if up { -1.0 } else { 1.0 };
    c.polyline(&[[cc[0] - s, cc[1] - 0.5 * s * d], [cc[0], cc[1] + 0.5 * s * d], [cc[0] + s, cc[1] - 0.5 * s * d]], 1.4, col);
}

/// The sections' line icons, alive under the pointer.
#[allow(clippy::too_many_arguments)]
fn icon(c: &mut Canvas, cat: Cat, at: [f32; 2], s: f32, col: Rgba, hover: f32, now: f32, act: bool) {
    let k = s / 24.0;
    let p = |x: f32, y: f32| [at[0] + x * k, at[1] + y * k];
    let w = 1.4;
    match cat {
        Cat::Player => {
            c.ring(p(12.0, 8.0 - 1.3 * hover), 3.6 * k, w, col);
            let mut pts = Vec::new();
            for i in 0..=10 {
                let a = std::f32::consts::PI * (1.0 + i as f32 / 10.0);
                pts.push(p(12.0 + a.cos() * 7.0, 20.3 + a.sin() * 6.0));
            }
            c.polyline(&pts, w, col);
        }
        Cat::Render => {
            c.ring(p(12.0, 12.0), 8.4 * k, w, col);
            let turn = hover * 1.05 + if act { now * 0.45 } else { 0.0 };
            for i in 0..6 {
                let a = i as f32 * std::f32::consts::PI / 3.0 + turn;
                let b = a + 1.25;
                c.line(p(12.0 + 8.4 * a.cos(), 12.0 + 8.4 * a.sin()), p(12.0 + 3.3 * b.cos(), 12.0 + 3.3 * b.sin()), w, col, 0.0);
            }
        }
        Cat::World => {
            let sun_y = 13.2 - hover * 1.8 + if act { (now * 1.2).sin() * 1.2 } else { 0.6 };
            // the sun behind the saddle: only its top shows over the ridge line
            c.push_clip([at[0], at[1], at[0] + s, p(0.0, 13.6)[1]]);
            c.ring(p(12.0, sun_y), 2.7 * k, w, col);
            c.pop_clip();
            c.polyline(&[p(2.5, 15.4), p(7.0, 10.4), p(10.0, 13.0), p(12.0, 14.1), p(14.0, 13.0), p(17.0, 10.4), p(21.5, 15.4)], w, col);
            c.line(p(4.0, 19.2), p(20.0, 19.2), w, col, 0.0);
        }
        Cat::Misc => {
            let g = 1.0 - 0.2 * hover;
            for (x, y) in [(7.3, 7.3), (16.7, 7.3), (7.3, 16.7), (16.7, 16.7)] {
                let (cx, cy) = (12.0 + (x - 12.0) * g, 12.0 + (y - 12.0) * g);
                c.ring(p(cx, cy), 2.8 * k, w, col);
            }
        }
        Cat::Configs => {
            let sp = 1.5 * hover;
            c.polyline(&[p(12.0, 4.2 - sp), p(20.0, 8.4 - sp), p(12.0, 12.6 - sp), p(4.0, 8.4 - sp), p(12.0, 4.2 - sp)], w, col);
            c.polyline(&[p(4.0, 12.2), p(12.0, 16.4), p(20.0, 12.2)], w, col);
            c.polyline(&[p(4.0, 15.9 + sp), p(12.0, 20.1 + sp), p(20.0, 15.9 + sp)], w, col);
        }
        Cat::Forum => {
            c.polyline(&[p(4.0, 5.0), p(20.0, 5.0), p(20.0, 15.5), p(10.0, 15.5), p(5.5, 19.5), p(5.5, 15.5), p(4.0, 15.5), p(4.0, 5.0)], w, col);
            let d = hover * 1.0;
            c.line(p(8.0, 9.0 - d * 0.5), p(16.0, 9.0 - d * 0.5), w, col, 0.0);
            c.line(p(8.0, 12.0), p(13.0 + d * 2.0, 12.0), w, col, 0.0);
        }
        Cat::Cosmetics => {
            // a crown
            let up = hover * 1.5;
            c.polyline(&[p(4.0, 18.0), p(4.0, 8.0 - up), p(8.5, 12.5), p(12.0, 6.0 - up), p(15.5, 12.5), p(20.0, 8.0 - up), p(20.0, 18.0), p(4.0, 18.0)], w, col);
        }
        Cat::Music => {
            let b = hover * 1.2 + if act { (now * 3.0).sin().abs() * 0.8 } else { 0.0 };
            c.ring(p(7.5, 17.5 - b * 0.3), 2.6 * k, w, col);
            c.ring(p(17.0, 15.5 - b * 0.3), 2.6 * k, w, col);
            c.polyline(&[p(10.1, 17.5 - b * 0.3), p(10.1, 5.5), p(19.6, 3.5), p(19.6, 15.5 - b * 0.3)], w, col);
        }
        Cat::Hud | Cat::HudEl => {
            // a screen with its corners marked, a card in it rising under the pointer
            let up = hover * 1.6;
            c.polyline(&[p(3.5, 8.0), p(3.5, 4.5), p(7.0, 4.5)], w, col);
            c.polyline(&[p(17.0, 4.5), p(20.5, 4.5), p(20.5, 8.0)], w, col);
            c.polyline(&[p(20.5, 16.0), p(20.5, 19.5), p(17.0, 19.5)], w, col);
            c.polyline(&[p(7.0, 19.5), p(3.5, 19.5), p(3.5, 16.0)], w, col);
            c.rect([p(7.5, 0.0)[0], p(0.0, 9.5 - up)[1], p(16.5, 0.0)[0], p(0.0, 14.5 - up)[1]], 1.6 * k, col);
        }
        Cat::Theme => {
            // a drop of colour, its highlight turning
            c.polyline(&[p(12.0, 3.5), p(17.0, 10.5), p(18.0, 14.0), p(16.6, 17.6), p(12.0, 20.0), p(7.4, 17.6), p(6.0, 14.0), p(7.0, 10.5), p(12.0, 3.5)], w, col);
            let a = now * if act { 1.2 } else { 0.0 } + hover * 1.4;
            c.line(p(12.0 + 2.6 * (a + 2.3).cos(), 14.0 + 2.6 * (a + 2.3).sin()), p(12.0 + 2.6 * (a + 3.0).cos(), 14.0 + 2.6 * (a + 3.0).sin()), w, col, 0.0);
        }
        Cat::Settings | Cat::Graphics => {
            let sh = hover * 3.0;
            c.line(p(4.0, 8.0), p(20.0, 8.0), w, col, 0.0);
            c.line(p(4.0, 16.0), p(20.0, 16.0), w, col, 0.0);
            c.rect([p(13.0 - sh, 6.0)[0], p(0.0, 6.0)[1], p(17.0 - sh, 10.0)[0], p(0.0, 10.0)[1]], 2.0 * k, grey(0.07, 1.0));
            c.ring(p(15.0 - sh, 8.0), 2.0 * k, w, col);
            c.rect([p(7.0 + sh, 14.0)[0], p(0.0, 14.0)[1], p(11.0 + sh, 18.0)[0], p(0.0, 18.0)[1]], 2.0 * k, grey(0.07, 1.0));
            c.ring(p(9.0 + sh, 16.0), 2.0 * k, w, col);
        }
    }
}

// ---------------------------------------------------------------------- the game's side (lib.rs's JNI calls)

static GUI: Mutex<Option<Gui>> = Mutex::new(None);
/// Frames made on the game's thread, taken by the frame thread in their turn (vk/frame.rs GUI).
static FRAMES: Mutex<Vec<(u32, Built)>> = Mutex::new(Vec::new());
static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

pub struct Built {
    pub prims: Vec<f32>,
    pub runs: Vec<u32>,
    pub idx: Vec<u32>,
    pub tiles_x: u32,
    pub look: Look,
    /// where the world is blurred for glass (x0 y0 x1 y1 each)
    pub areas: Vec<[f32; 4]>,
    /// 0 the menu, 1 the HUD (each its own pictures)
    pub layer: u8,
    /// The Cosmetics page's look at an item, drawn by the engine over this frame (cosmetics/preview.rs)
    pub preview: Option<crate::cosmetics::preview::Preview>,
}

fn with<R>(f: impl FnOnce(&mut Gui) -> R) -> R {
    let mut g = GUI.lock().unwrap();
    f(g.get_or_insert_with(Gui::new))
}

pub fn open(w: f32, h: f32, player: &str, dir: &str) {
    with(|g| {
        g.player = player.to_string();
        if !dir.is_empty() {
            g.dir = Some(std::path::PathBuf::from(dir));
        }
        g.open_menu(w, h)
    })
}

/// Lays out and draws one frame; returns its number for the frame op (0 nothing to draw).
pub fn frame(w: f32, h: f32) -> u32 {
    let built = with(|g| {
        if g.phase == Phase::Closed {
            return None;
        }
        let started = Instant::now();
        let mut c = g.frame(w, h);
        static FADE: Mutex<i18n::Crossfade> = Mutex::new(i18n::Crossfade::new());
        FADE.lock().unwrap().apply(&mut c);
        let (runs, idx, tx) = c.bins();
        bench(started, c.p.len() / draw::FLOATS);
        let wr = g.look.win;
        let area = if g.look.open > 0.0 { hud_union(c.glass_area, [wr[0], wr[1], wr[0] + wr[2], wr[1] + wr[3]]) } else { c.glass_area };
        // the look at an item only while the menu is fully open (it does not fade with it)
        let preview = g.cosm_shown().filter(|_| g.phase == Phase::Open);
        Some(Built { prims: c.p, runs, idx, tiles_x: tx, look: g.look, areas: area.into_iter().collect(), layer: 0, preview })
    });
    push(built)
}

fn hud_union(a: Option<[f32; 4]>, b: [f32; 4]) -> Option<[f32; 4]> {
    Some(match a {
        Some(g) => [g[0].min(b[0]), g[1].min(b[1]), g[2].max(b[2]), g[3].max(b[3])],
        None => b,
    })
}

/// A frame made: numbered and kept for the frame thread.
pub(crate) fn push(built: Option<Built>) -> u32 {
    let Some(b) = built else { return 0 };
    let id = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut f = FRAMES.lock().unwrap();
    f.retain(|(n, _)| id.wrapping_sub(*n) < 8);
    f.push((id, b));
    id
}

/// Development (CURSED_DEV_BENCH): the menu's frame on the CPU, in the log every 600 frames.
fn bench(started: Instant, prims: usize) {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if !*ON.get_or_init(|| std::env::var_os("CURSED_DEV_BENCH").is_some()) {
        return;
    }
    static SUM: Mutex<(f64, u32, usize)> = Mutex::new((0.0, 0, 0));
    let mut s = SUM.lock().unwrap();
    s.0 += started.elapsed().as_secs_f64();
    s.1 += 1;
    s.2 = s.2.max(prims);
    if s.1 == 600 {
        eprintln!("[gui] frame on the CPU: {:.1} us on average, {} shapes at most", s.0 / 600.0 * 1e6, s.2);
        *s = (0.0, 0, 0);
    }
}

/// The frame thread takes its frame (and lets older ones go).
pub fn take(id: u32) -> Option<Built> {
    let mut f = FRAMES.lock().unwrap();
    let at = f.iter().position(|(n, _)| *n == id)?;
    let b = f.remove(at).1;
    // the HUD's, its settings' and the menu's frames come together: only those well behind go
    f.retain(|(n, _)| id.wrapping_sub(*n) < 6 || *n > id);
    Some(b)
}

pub fn mouse(x: f32, y: f32, button: i32, action: i32) -> bool {
    with(|g| match action {
        // a script plays (dev.rs): the stand's pointer is not ours
        0 if g.dev.is_some() => false,
        1 => g.press(x, y, button),
        2 => {
            g.release();
            true
        }
        _ => {
            g.mouse_move(x, y);
            false
        }
    })
}

pub fn scroll(dy: f32) {
    with(|g| g.wheel(dy))
}

pub fn key(escape: bool, enter: bool, backspace: bool, name: &str, menu_key: bool) -> i32 {
    with(|g| g.key(escape, enter, backspace, name.to_string(), menu_key))
}

pub fn typed(c: u32) {
    if let Some(ch) = char::from_u32(c) {
        with(|g| g.char_typed(ch))
    }
}

pub fn close(now: bool) {
    with(|g| g.close(now))
}

/// Java's other words to the menu and the HUD, one line, its fields by tabs; what comes back (often nothing):
///   key 1|0 <name>        a key or mouse button down or up in the world (the binds); "1" if a bind went off
///   gkeyup <name>         a key let go in the menu
///   hudkey 1|0 <name>     a key in the chat or the layout editor; "1" if it was a bind's being recorded
///   poll                  what the menu asks of the game, a line each
///   paste <text>          the clipboard, asked for
pub fn call(cmd: &str) -> String {
    let mut f = cmd.splitn(3, '\t');
    let verb = f.next().unwrap_or("");
    let a = f.next().unwrap_or("");
    let b = f.next().unwrap_or("");
    match verb {
        "key" => with(|g| g.world_key(b, a == "1") as i32).to_string(),
        "gkeyup" => {
            with(|g| g.bind_key_up(a));
            String::new()
        }
        "hudkey" => with(|g| {
            if !g.hud_input() {
                return "0".to_string();
            }
            g.undo_poke(None);
            g.in_hud(|g| {
                if a == "1" {
                    g.bind_key_down(b);
                    if g.bind_key(b == "Escape", b == "Backspace", b) {
                        return "1".to_string();
                    }
                    // the layout editor's Ctrl+Z and Ctrl+Y, as the menu's (nothing typed into)
                    let held = |a: &str, b: &str| g.gheld.iter().any(|k| k == a || k == b);
                    if g.hud.editing && g.text_focus.is_none() && held("Left Ctrl", "Right Ctrl") && (b == "Z" || b == "Y") {
                        let shift = held("Left Shift", "Right Shift");
                        g.undo_go(b == "Z" && !shift);
                        return "1".to_string();
                    }
                    "0".to_string()
                } else {
                    g.bind_key_up(b);
                    "0".to_string()
                }
            })
        }),
        "poll" => with(|g| std::mem::take(&mut g.outbox).join("\n")),
        // what plays (the HUD's music: the game's own, or Windows' media controls): title, artist, where in it (s), its
        // length (s), 1 playing; nothing when nothing plays (World Music)
        "music" => with(|g| {
            g.hud.facts.music.as_ref().map_or(String::new(), |m| format!("{}\t{}\t{}\t{}\t{}", m.title.replace('\t', " "), m.artist.replace('\t', " "), m.pos, m.len, m.playing as i32))
        }),
        // `b` laid out in face `a` (text.rs Face's number) for the world's words (World Music, engine/fx.rs TEXT): its
        // width (em), then each letter: its box from the pen (em: left, top down from the baseline, width, height), its
        // rectangle in its atlas (px: x, y, width, height) and its atlas (0 the faces', 1 the system fonts'), by spaces,
        // the letters by semicolons; the ascent and descent (em) first, and 1 when a letter is still being drawn
        "glyphs" => text::layout(a.parse().unwrap_or(0), b),
        // a module the game draws by itself (cursed.recode.chams.Chams): on (1 or 0), then its first choice's option
        "mod" => with(|g| {
            g.mods.iter().find(|m| m.id == a).map_or(String::new(), |m| {
                let choice = m.set.iter().find_map(|s| match &s.kind {
                    Kind::Choice { v, opts } => opts.get(*v).copied(),
                    _ => None,
                });
                format!("{}\t{}", m.on as i32, choice.unwrap_or(""))
            })
        }),
        // a module's switch and all its settings, by their labels: "1\tLabel=value\t…" (a switch 1 or 0, a slider its
        // number, a choice its option, a colour its rgb in hex)
        "set" => with(|g| {
            if a == "chams" {
                g.liquid_follow();
            }
            if a == swap::MODULE {
                g.timing_follow();
            }
            let hidden = g.hidden_here(a);
            g.mods.iter().find(|m| m.id == a).map_or(String::new(), |m| {
                let mut out = ((m.on && !hidden) as i32).to_string();
                for s in &m.set {
                    let v = match &s.kind {
                        // (one this server forbids: off to the game)
                        Kind::Toggle(v) => ((*v && !g.setting_hidden_here(s.label)) as i32).to_string(),
                        Kind::Slider { v, .. } => v.to_string(),
                        Kind::Choice { v, opts } => opts.get(*v).copied().unwrap_or("").to_string(),
                        Kind::Colour(c) => format!("{:06x}", c.rgb),
                        _ => continue,
                    };
                    out.push('\t');
                    out.push_str(s.label);
                    out.push('=');
                    out.push_str(&v);
                }
                out
            })
        }),
        // a module's setting set by its label (the stand: DevCommands "set"): "module\tLabel=value" as "set" gives them;
        // "on=1" its switch
        "put" => with(|g| {
            let Some((label, v)) = b.split_once('=') else { return String::new() };
            if let Some(m) = g.mods.iter_mut().find(|m| m.id == a) {
                if label == "on" {
                    m.on = v == "1";
                }
                // its binds, as menu.txt keeps them (the stand: a module's key on the keyboard)
                if label == "#binds" {
                    m.binds = binds::load(v);
                }
                if let Some(s) = m.set.iter_mut().find(|s| s.label == label) {
                    match &mut s.kind {
                        Kind::Toggle(t) => *t = v == "1",
                        Kind::Slider { v: x, .. } => *x = v.parse().unwrap_or(*x),
                        Kind::Choice { v: x, opts } => *x = opts.iter().position(|o| *o == v).unwrap_or(*x),
                        // a colour: "rrggbb" its own (not the theme's), or whole as a save keeps it ("rgb/a/chroma/auto")
                        Kind::Colour(c) => {
                            let mut f = v.split('/');
                            c.rgb = u32::from_str_radix(f.next().unwrap_or(""), 16).unwrap_or(c.rgb);
                            c.a = f.next().and_then(|x| x.parse().ok()).unwrap_or(c.a);
                            c.chroma = f.next().and_then(|x| x.parse().ok()).unwrap_or(0.0);
                            c.auto = f.next() == Some("1");
                        }
                        // the options on, by "|" (as a save keeps them)
                        Kind::Multi { on, opts } => {
                            for (i, o) in opts.iter().enumerate() {
                                on[i] = v.split('|').any(|q| q == *o);
                            }
                        }
                        // a key (its name as the binds give it; empty: none), a text
                        Kind::Key(k) => *k = v.to_string(),
                        Kind::Text(t) => *t = v.to_string(),
                        _ => {}
                    }
                }
            }
            String::new()
        }),
        // the HUD's plates last drawn (fx/WorldMusic keeps the world's words clear of them): "w h" (the canvas), then
        // ";x0 y0 x1 y1" each (its px)
        "plates" => {
            let p = HUD_PLATES.lock().unwrap();
            let mut out = format!("{} {}", p.0, p.1);
            for r in &p.2 {
                out.push_str(&format!(";{} {} {} {}", r[0], r[1], r[2], r[3]));
            }
            out
        }
        // Auto Swap (swap.rs): its swaps for Java, all of them anew, the keys' events, the server played on
        "swaps" | "swapput" | "swapev" | "server" | "swapnote" | "swapwait" | "swapinv" | "swapicons" | "pearlev" => with(|g| g.swap_call(verb, a, b)),
        // its timing (swaptime.rs): a swap coming up, when its clicks go
        "swaptime" => with(|g| g.swap_time(a, b)),
        // Alt Manager (crate::alts): its sub-verb, the rest
        "alts" => crate::alts::call(a, b),
        // the modules that act in the game (play.rs): each tick's facts, what to do
        "play" => with(|g| g.play_call(a, b)),
        // a hand's pose and swing this frame (viewmodel.rs)
        "hand" => with(|g| g.hand_call(&format!("{a}\t{b}"))),
        // Free Look's camera each frame, the mouse's turns and the wheel while it looks (freelook.rs)
        "fl" => with(|g| g.fl_call(a, b)),
        // Streamer Mode: the facts each second, what the game's words put aside (streamer.rs)
        "streamer" => with(|g| g.streamer_call(b)),
        // No Sound: how loud a sound starting may be (nosound.rs)
        "nosound" => with(|g| g.nosound_call(a)),
        // Name Tags: this frame's players; "1" while it draws them (the game's own left out)
        // Fake Player: the player each tick, the doubles' places ("tick"); a double hit ("hit n x y z")
        "fake" => with(|g| match a {
            "tick" => g.fakes_tick(b),
            "hit" => {
                let v: Vec<f64> = b.split(' ').filter_map(|x| x.parse().ok()).collect();
                if v.len() >= 4 {
                    g.fakes_hit(v[0] as usize, [v[1], v[2], v[3]]);
                }
                String::new()
            }
            _ => String::new(),
        }),
        "tags" => with(|g| {
            g.tags_set(b);
            (g.tags_on() as i32).to_string()
        }),
        "shulk-filter" => with(|g| g.shulk_filter(a)),
        "fake-preview" => with(|g| g.fakes.preview.map_or(String::new(), |v| v.iter().map(|f| format!("{f:.3}")).collect::<Vec<_>>().join(" "))),
        // Shulker Preview's boxes over the world this frame (hudshulker.rs)
        "shulkers" => {
            with(|g| g.shulk_set(a));
            String::new()
        }
        // Cosmetics in the hands (crate::cosmetics): its sub-verb, the rest
        "cosm" => crate::cosmetics::call(a, b),
        // the game's controls (each second, from Java): what of them CS's inspect key moves (cosmkeys.rs), a line each
        "gamekeys" => with(|g| g.keys_told(a)),
        // the faces of Alt Manager's card in the client's menu, for Java to draw (altsui.rs)
        "altfaces" => with(|g| g.alt_faces_text()),
        // a module switched from the game's keys (F3+B Hitboxes, F3+G Chunk Grid): "module\t1" on, "module\t0" off; kept
        "switch" => with(|g| {
            if let Some(i) = g.mods.iter().position(|m| m.id == a) {
                if g.mods[i].on != (b == "1") {
                    g.toggle(i);
                    g.save();
                }
            }
            String::new()
        }),
        // Custom Inventory's own buttons (hud/hudcreative.rs): the creative rail whole or its icons
        "inv" => with(|g| {
            if a == "rail" {
                let mi = g.mod_index("custom-inventory");
                if let Some(s) = g.mods[mi].set.iter_mut().find(|s| s.label == "Left panel") {
                    if let Kind::Choice { v, .. } = &mut s.kind {
                        *v = 1 - (*v).min(1);
                    }
                }
                g.save();
            }
            String::new()
        }),
        // the client's own chat (hud/hudnchat.rs)
        "chatadd" => {
            let stack = with(|g| {
                let i = g.mod_index("chat");
                theme::on(&g.mods[i].set, "Stack repeats")
            });
            hud::nchat_add(a, stack);
            String::new()
        }
        "chatclear" => {
            hud::nchat_clear();
            String::new()
        }
        "chatin" => {
            hud::nchat_input(a);
            String::new()
        }
        "chatserver" => {
            hud::nchat_server(&format!("{a}	{b}"));
            String::new()
        }
        "chatfind" => {
            hud::nchat_find(a == "1");
            String::new()
        }
        "chatfaces" => hud::nchat_faces(),
        "chatonline" => {
            hud::nchat_online(&format!("{a}\t{b}"));
            String::new()
        }
        "chatto" => hud::nchat_to(),
        "chatsent" => {
            hud::nchat_sent(a);
            String::new()
        }
        "langs" => {
            with(|g| g.set_langs(a, b));
            String::new()
        }
        // the player's pictures: number, file, GIF speed — a line each
        "images" => with(|g| {
            g.picked_in();
            g.mods
                .iter()
                .filter(|m| m.id.starts_with("x-image") && m.on)
                .filter_map(|m| {
                    let f = theme::text(&m.set, "File").trim().to_string();
                    (!f.is_empty()).then(|| format!("{}\t{}\t{}", m.id.rsplit('-').next().unwrap_or("0"), f, theme::num(&m.set, "GIF speed")))
                })
                .chain(g.hud.cover.as_ref().map(|(f, n)| format!("-1\t{f}\t1\t{n}")))
                .collect::<Vec<_>>()
                .join("\n")
        }),
        // a picture's own size, read by Java (a picture keeps its shape by it)
        "picsize" => {
            let mut it = b.split('\t');
            let (w, h) = (it.next().and_then(|v| v.parse().ok()).unwrap_or(0.0), it.next().and_then(|v| v.parse().ok()).unwrap_or(0.0));
            hudx::PIC_SIZES.lock().unwrap().insert(a.to_string(), (w, h));
            String::new()
        }
        "paste" => {
            let text = if b.is_empty() { a.to_string() } else { format!("{a}\t{b}") };
            with(|g| g.paste(&text));
            String::new()
        }
        _ => String::new(),
    }
}

/// [shown (0 closed, 1 opening, 2 open, 3 closing), head x, y, size, its alpha, the menu key is waited for]
pub fn state() -> [f32; 6] {
    with(|g| {
        let r = g.r;
        let (a, dy) = g.rise(0.42);
        let phase = match g.phase {
            Phase::Closed => 0.0,
            Phase::Opening => 1.0,
            Phase::Open => 2.0,
            Phase::Closing => 3.0,
        };
        let open = g.look.open;
        // where the window is drawn now (the glass's opening scales and lifts it)
        let [mx, my, sc, drop, wa] = g.xf;
        let (hx, hy) = (g.win[0] + 1.55 * r, g.win[1] + g.win[3] - 3.4 * r + dy + 0.15 * r);
        let (hx, hy) = (mx + (hx - mx) * sc, my + (hy - my) * sc + drop);
        [phase, hx, hy, 2.05 * r * sc, a * wa * if open > 0.97 { 1.0 } else { 0.0 }, g.listen.is_some() as i32 as f32]
    })
}

/// The key that opens the menu (Settings), as its name.
pub fn menu_key() -> String {
    with(|g| g.menu_key_name())
}

impl Gui {
    fn menu_key_name(&self) -> String {
        match self.client.iter().find(|s| s.label == "Open the menu").map(|s| &s.kind) {
            Some(Kind::Key(v)) => v.clone(),
            _ => "Right Shift".into(),
        }
    }
}

// ---------------------------------------------------------------------- the HUD (hud.rs)

/// Lays out and draws a frame of the HUD from what the game knows (hud::Facts::parse); its number, 0 nothing to draw.
pub fn hud_frame(w: f32, h: f32, nums: &[f64], text: &str) -> u32 {
    cloud::start();
    let started = Instant::now();
    let (built, over, screen) = with(|g| {
        let facts = hud::Facts::parse(nums, text);
        let r = g.hud_frame(w, h, facts);
        // Custom Gui: the panel under the open container (the HUD hidden or not)
        let screen = g.screen_frame(w, h).map(|s| {
            let (runs, idx, tx) = s.bins();
            let areas: Vec<[f32; 4]> = s.glass_area.into_iter().collect();
            let blur = theme::HudLook::of(&g.theme, g.now).blur;
            Built { prims: s.p, runs, idx, tiles_x: tx, look: Look { fade: 1.0, time: g.now, blur, ..Look::default() }, areas, layer: 0, preview: None }
        });
        let Some((mut c, o)) = r else {
            HUD_PLATES.lock().unwrap().2.clear();
            return (None, None, screen);
        };
        *HUD_PLATES.lock().unwrap() = (c.w, c.h, c.glass_rects.clone());
        static FADE: Mutex<i18n::Crossfade> = Mutex::new(i18n::Crossfade::new());
        FADE.lock().unwrap().apply(&mut c);
        let look = Look { fade: 1.0, time: g.now, blur: theme::HudLook::of(&g.theme, g.now).blur, ..Look::default() };
        // the world behind an open screen, greyed and darkened (no window: only the veil, over the whole picture)
        let p = pal();
        let veiled = Look { veil: g.hud.veil, veil_col: p.veil, anim: [0.0, 0.0, -1.0, 1.0 - p.around], ..look };
        let make = |c: Canvas, layer: u8, look: Look| {
            let (runs, idx, tx) = c.bins();
            let areas = if layer == 1 { c.glass_merged() } else { c.glass_area.into_iter().collect() };
            Built { prims: c.p, runs, idx, tiles_x: tx, look, areas, layer, preview: None }
        };
        let menu_blur = pal().blur;
        (Some(make(c, 1, veiled)), o.map(|o| make(o, 0, Look { blur: menu_blur, ..look })), screen)
    });
    hud_bench(started, built.as_ref().map_or(0, |b| b.prims.len() / draw::FLOATS));
    HUD_OVER.store(push(over), std::sync::atomic::Ordering::Relaxed);
    HUD_SCREEN.store(push(screen), std::sync::atomic::Ordering::Relaxed);
    push(built)
}

/// Custom Inventory (hud/hudinv.rs): the survival inventory's frame from what the game knows; its picture under the
/// items (its number), the one over them (inv_top), what Java draws where (inv_out). The creative inventory's
/// (hud/hudcreative.rs) when its text says so on its first line.
pub fn inv_frame(w: f32, h: f32, nums: &[f64], text: &str) -> u32 {
    let (under, top) = with(|g| {
        let (c, t, out) = if let Some(rest) = text.strip_prefix("creative\n") {
            g.creative_frame(w, h, &hud::CrF::parse(nums, rest))
        } else if let Some(rest) = text.strip_prefix("pack\n") {
            g.pack_frame(w, h, &hud::PackF::parse(nums, rest))
        } else {
            g.inv_frame(w, h, &hud::InvF::parse(nums, text))
        };
        *INV_OUT.lock().unwrap() = out;
        let now = g.now;
        let make = |c: Canvas, layer: u8| {
            let (runs, idx, tx) = c.bins();
            let areas: Vec<[f32; 4]> = c.glass_area.into_iter().collect();
            Built { prims: c.p, runs, idx, tiles_x: tx, look: Look { fade: 1.0, time: now, blur: 1.0, ..Look::default() }, areas, layer, preview: None }
        };
        (make(c, 0), make(t, 2))
    });
    INV_TOP.store(push(Some(top)), std::sync::atomic::Ordering::Relaxed);
    push(Some(under))
}

static INV_TOP: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
static INV_OUT: Mutex<Vec<f32>> = Mutex::new(Vec::new());

pub fn inv_top() -> u32 {
    INV_TOP.swap(0, std::sync::atomic::Ordering::Relaxed)
}

pub fn inv_out() -> Vec<f32> {
    INV_OUT.lock().unwrap().clone()
}

/// The frame under the open container (Custom Gui), made with the HUD's last (0 none): Java puts it under the
/// container's picture.
static HUD_SCREEN: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub fn hud_screen() -> u32 {
    HUD_SCREEN.swap(0, std::sync::atomic::Ordering::Relaxed)
}

/// Development (CURSED_DEV_BENCH): the HUD's frame on the CPU, in the log every 600 frames.
fn hud_bench(started: Instant, prims: usize) {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if !*ON.get_or_init(|| std::env::var_os("CURSED_DEV_BENCH").is_some()) {
        return;
    }
    static SUM: Mutex<(f64, u32, usize)> = Mutex::new((0.0, 0, 0));
    let mut s = SUM.lock().unwrap();
    s.0 += started.elapsed().as_secs_f64();
    s.1 += 1;
    s.2 = s.2.max(prims);
    if s.1 == 600 {
        eprintln!("[hud] frame on the CPU: {:.1} us on average, {} shapes at most", s.0 / 600.0 * 1e6, s.2);
        *s = (0.0, 0, 0);
    }
}

/// The HUD's plates last drawn (the canvas' px, with their shadows' margin) and the canvas' size: what the world's words
/// keep clear of (fx/WorldMusic, the call "plates").
static HUD_PLATES: Mutex<(f32, f32, Vec<[f32; 4]>)> = Mutex::new((0.0, 0.0, Vec::new()));

/// The frame of what is open by a HUD element, made with the HUD's last (0 none): the chat or the editor puts it over all.
static HUD_OVER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub fn hud_overlay() -> u32 {
    HUD_OVER.swap(0, std::sync::atomic::Ordering::Relaxed)
}

/// The icons Java draws over the HUD: [kind, index, x, y, w, h, alpha] in window px (hud.rs says which kind is what).
pub fn hud_icons() -> Vec<[f32; 7]> {
    with(|g| g.hud.icons.clone())
}

/// The icons Java draws over the open container's panel (Custom Gui: the player's pictures there).
pub fn hud_screen_icons() -> Vec<[f32; 7]> {
    with(|g| g.hud.screen_icons.clone())
}

pub fn hud_mouse(x: f32, y: f32, button: i32, action: i32) -> bool {
    with(|g| g.hud_mouse(x, y, button, action))
}

pub fn hud_scroll(dy: f32) -> bool {
    with(|g| g.hud_scroll(dy))
}

pub fn hud_key(escape: bool, enter: bool, backspace: bool) -> bool {
    with(|g| g.hud_key(escape, enter, backspace))
}

pub fn hud_char(c: u32) -> bool {
    char::from_u32(c).is_some_and(|ch| with(|g| g.hud_char(ch)))
}

pub fn hud_bind(name: &str) -> bool {
    with(|g| g.hud_bind(name))
}

pub fn hud_event(kind: i32, who: &str) {
    with(|g| g.hud_event(kind, who))
}

pub fn hud_closed() {
    with(|g| g.hud_closed())
}

/// The player and the folder the settings are in, before the menu was ever opened (the HUD reads them at once).
pub fn hud_init(player: &str, dir: &str) {
    with(|g| {
        if !player.is_empty() {
            g.player = player.to_string();
        }
        if !dir.is_empty() && g.dir.is_none() {
            g.dir = Some(std::path::PathBuf::from(dir));
        }
    })
}

/// [the game's effect icons hidden, the layout editor asked for (once), the Custom Hotbar on (82..92: which of the
/// game's own hotbar and bars it draws instead, and how high it stands), then the Custom Chat's (Java's HudChat.state): on, where its words go (x, the last line's bottom),
/// the room before them and where a face goes in it, faces, how new lines come (0 at once, 1 rising, 2 sliding in, 3
/// fading in), the animations' speed, smooth scrolling, repeats counted, fading out, after how long, how many kept,
/// mentions lit, their sound, copying, its width (px), lines closed and open, its size, its own input field, the game's
/// marks by messages (not secure, modified…); then the Custom Scoreboard's (Java's HudBoard.state): on, its words'
/// corner, their width, the title, its alignment, the lines', the numbers (as the server's, the accent, hidden), its
/// size, the spacing (half units), the gap under the title (units), how seen, the accent, shadows, most lines]
pub fn hud_state() -> [f32; 96] {
    with(|g| {
        let want = std::mem::take(&mut g.want_editor);
        let hud_on = g.mods.iter().any(|m| m.cat == Cat::Hud && m.id == "hud" && m.on);
        let hotbar = g.group_on("Custom Hotbar") && hud_on;
        let mut v = [0.0f32; 96];
        v[0] = g.hud.hide_effects as i32 as f32;
        v[1] = want as i32 as f32;
        v[2] = hotbar as i32 as f32;
        // the Custom Hotbar's parts it draws itself, the game's own of each left out: the slots, health, food, armour,
        // air, the experience's bar, the level, the mount's health, its jump, the held item's name. A part switched off
        // is the game's again (her word, 25.10). Then how high its hotbar stands (a part of the HUD's height): what of
        // the game's is given back rests over it.
        if hotbar {
            let parts = ["hb-hotbar", "hb-health", "hb-food", "hb-armor", "hb-air", "hb-xp", "hb-xp", "hb-mount", "hb-mount", "hb-hotbar"];
            for (k, id) in parts.iter().enumerate() {
                v[82 + k] = g.part_on(id) as i32 as f32;
            }
            v[92] = g.hotbar_top().unwrap_or(0.0);
        }
        // the chat is the client's own (chat/): the old hybrid's numbers (3..24, Java's HudChat) stay off
        let i = g.mod_index("chat");
        let chat_on = g.group_on("Custom Chat") && hud_on && g.mods[i].on;
        let i = g.mod_index("scoreboard");
        if g.group_on("Custom Scoreboard") && hud_on && g.mods[i].on {
            let set = &g.mods[i].set;
            let on = |l: &str| theme::on(set, l) as i32 as f32;
            let pl = g.hud.board_place;
            v[25] = 1.0;
            v[26] = pl.corner[0];
            v[27] = pl.corner[1];
            v[28] = pl.width;
            v[29] = on("Title");
            v[30] = theme::pick(set, "Title align") as f32;
            v[31] = theme::pick(set, "Lines align") as f32;
            v[32] = theme::pick(set, "Numbers") as f32;
            v[33] = (theme::num(set, "Scale") / 100.0).max(0.3);
            v[34] = theme::num(set, "Line spacing");
            v[35] = if theme::on(set, "Title") { if theme::on(set, "Divider") { 6.0 } else { 3.0 } } else { 0.0 };
            v[36] = pl.alpha;
            v[37] = g.hud.board_accent as f32;
            v[38] = (theme::num(set, "Text shadow") > 0.0) as i32 as f32;
            v[39] = theme::num(set, "Max lines");
        }
        // Other Custom Things: each part where its words go, how seen, its size; the name tags' look
        if g.group_on("Other Custom Things") && hud_on {
            v[40] = 1.0;
            let el = |id: &str| {
                let i = g.mod_index(id);
                (g.mods[i].on, &g.mods[i].set, g.hud.placed.get(id).copied().unwrap_or_default())
            };
            let (on, set, pl) = el("bossbars");
            v[41] = on as i32 as f32;
            v[42] = pl.abs[0];
            v[43] = pl.abs[1];
            v[44] = pl.step;
            v[46] = (theme::num(set, "Scale") / 100.0).max(0.3);
            v[47] = pl.alpha;
            v[48] = theme::on(set, "Names") as i32 as f32;
            let (on, set, pl) = el("titles");
            v[50] = on as i32 as f32;
            v[51] = pl.abs[0];
            v[52] = pl.abs[1];
            v[53] = (theme::num(set, "Scale") / 100.0).max(0.3);
            v[54] = theme::pick(set, "Animation") as f32;
            v[55] = pl.alpha;
            v[56] = theme::num(set, "Animation speed") / 100.0;
            let (on, set, pl) = el("actionbar");
            v[57] = on as i32 as f32;
            v[58] = pl.abs[0];
            v[59] = pl.abs[1];
            v[60] = (theme::num(set, "Scale") / 100.0).max(0.3);
            v[61] = pl.alpha;
            let (on, set, _) = el("holograms");
            v[62] = on as i32 as f32;
            v[63] = theme::pick(set, "Plate") as f32;
            v[64] = theme::num(set, "Plate opacity") / 100.0;
            let pc = theme::or(theme::col(set, "Plate colour"), [0.0, 0.0, 0.0, 1.0], 0.0);
            v[65] = (((pc[0].clamp(0.0, 1.0) * 255.0) as u32) << 16 | ((pc[1].clamp(0.0, 1.0) * 255.0) as u32) << 8 | (pc[2].clamp(0.0, 1.0) * 255.0) as u32) as f32;
            v[66] = theme::on(set, "Words shadow") as i32 as f32;
            v[67] = theme::on(set, "Fade with distance") as i32 as f32;
            v[68] = (theme::num(set, "Tag size") / 100.0).max(0.3);
            v[69] = theme::num(set, "Fade from");
        }
        // Custom Gui: the wells' colour, the words', the dimming, the opening, the server's menus, tooltips
        if let Some(i) = g.mods.iter().position(|m| m.id == "custom-gui").filter(|&i| g.mods[i].on) {
            let set = &g.mods[i].set;
            let t = g.now;
            let rgb = |c: [f32; 4]| (((c[0].clamp(0.0, 1.0) * 255.0) as u32) << 16 | ((c[1].clamp(0.0, 1.0) * 255.0) as u32) << 8 | (c[2].clamp(0.0, 1.0) * 255.0) as u32) as f32;
            v[70] = 1.0;
            v[71] = theme::num(set, "Slot opacity") / 100.0;
            v[72] = rgb(theme::or(theme::col(set, "Slot colour"), [1.0; 4], t));
            v[73] = rgb(theme::or(theme::col(set, "Labels colour"), [0.925, 0.925, 0.925, 1.0], t));
            v[74] = theme::num(set, "Dim") / 100.0;
            v[75] = theme::pick(set, "Opening") as f32;
            v[76] = theme::num(set, "Opening speed") / 100.0;
            v[77] = theme::on(set, "Keep server menus") as i32 as f32;
            v[78] = theme::on(set, "Own tooltips") as i32 as f32;
        }
        // the chat drawn by the client (chat/; Java's NChat sends it the messages and the field)
        v[80] = chat_on as i32 as f32;
        // Custom Inventory: the survival inventory as the client's character screen
        v[79] = g.mods.iter().any(|m| m.id == "custom-inventory" && m.on) as i32 as f32;
        // Custom F3: its cards instead of the game's words of F3 (Java's HudF3)
        v[81] = (hud_on && g.group_on("Custom F3")) as i32 as f32;
        v
    })
}

impl Gui {
    /// Whether the module a group of the HUD's elements belongs to is on («Custom Hotbar»: the custom-hotbar module).
    pub(super) fn group_on(&self, group: &str) -> bool {
        let id = match group {
            "Custom Hotbar" => "custom-hotbar",
            "Custom F3" => "custom-f3",
            "Custom Chat" => "custom-chat",
            "Custom Scoreboard" => "custom-scoreboard",
            "Other Custom Things" => "other-custom",
            _ => return true,
        };
        self.mods.iter().any(|m| m.id == id && m.on)
    }
}
