//! What the hands hold and how it moves (the frame command VIEWMODEL asks each frame): the item in the main hand (said
//! by Java each tick, cursed.recode.cosmetics.Cosmetics), what CS item it is shown as (a knife for a sword or an axe,
//! the AWP for a bow, the SSG 08 for a crossbow), CS's hands holding it and playing its clips (cosmetics/hands.rs) —
//! then the models to draw, each with its place in the view and its skin, and the hands' own projection (CS's
//! viewmodel_fov 68). A rifle's scope, its kick and its bolt are cosmetics/rifle.rs's; the game asks here for the
//! field of view, the camera's kick, the mouse's slowing in the scope, whether the bow may be drawn. The Cosmetics
//! page's look «in the hands» has hands of its own (look_frame): what the page shows, not what the game holds.

use super::hands::{Act, Hands, Kind, Kit, INCH};
use super::model::{mul, Model, IDENTITY, M4};
use super::render::ScopeView;
use super::rifle::{Moving, Rifle, Weapon, AWP, SSG08};
use super::catalog::{self, Kind as CatKind};
use super::loadout::{self, Loadout, Slot};
use super::{sound, store};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

/// What Java says each frame: the view's yaw and pitch (degrees, the game's), the player's velocity (blocks a tick,
/// the world's axes), flags (ON_GROUND, SPRINTING, SNEAKING), the light at the eye (sky, block: 0..1), the picture's
/// width over its height.
#[derive(Clone, Copy, Debug, Default)]
pub struct Facts {
    pub yaw: f32,
    pub pitch: f32,
    pub vel: [f32; 3],
    pub flags: u32,
    pub sky_light: f32,
    pub block_light: f32,
    pub aspect: f32,
}

pub const ON_GROUND: u32 = 1;
pub const SPRINTING: u32 = 2;
pub const SNEAKING: u32 = 4;

/// What the player does with the hands, as Java sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// An attack swung (the left button); `true` when it struck something.
    Swing(bool),
    /// The right button's heavy one.
    Heavy(bool),
    /// The use key held (a bow drawn).
    UseStart,
    UseStop,
    /// The inspect key.
    Inspect,
    /// The bow let go with an arrow: a rifle's shot.
    Shoot,
    /// The wheel in the scope: its other zoom level.
    Zoom,
}

/// A model to draw this frame.
pub struct Drawn {
    pub model: String,
    pub world: M4,
    pub skin: Vec<M4>,
    /// Finishes on its materials (material, finish key).
    pub finish: Vec<(String, String)>,
    /// A StatTrak module's number (its display's material shows it).
    pub stattrak: Option<u32>,
    /// Stickers over its body.
    pub stickers: Vec<super::render::StickerDraw>,
    /// Materials coloured by another texture (a name tag's plate written).
    pub retex: Vec<(String, String)>,
    /// Our material over its finish (the renderer's look).
    pub look: Option<(u32, f32, [f32; 3])>,
}

pub struct Frame {
    pub draws: Vec<Drawn>,
    pub proj: M4,
    /// The view's rotation (rows: right, up, back in the world) — the sun into the view.
    pub basis: [[f32; 3]; 3],
    /// In a rifle's scope: the scope over the picture, no hands (m_bHideViewModelWhenZoomed).
    pub scope: Option<ScopeView>,
    /// The finishes to work (once each; the GPU draws their materials with them when done): the item's, the gloves'.
    pub finish: Vec<Arc<super::compose::Job>>,
}

/// The viewmodel's offset in its own space (forward, left, up; inches), VRF's tuning of CS2's.
const OFFSET: [f32; 3] = [5.0, -2.0, -2.0];

#[derive(Default)]
struct State {
    item: String,
    hands: Option<Hands>,
    /// The arms' and the item's joints in the clips' skeleton (Model::map_to), once the three are read.
    maps: Option<(Vec<i32>, Vec<i32>, Vec<i32>)>,
    events: Vec<Event>,
    last: Option<Instant>,
    /// How far a dynamic crosshair is open (0..1): the player's movement, eased.
    spread: f32,
    /// The item's StatTrak kills (none: not StatTrak), its charm (a rifle's), its model's attachments (by its folder).
    stattrak: Option<u32>,
    charm: Option<String>,
    /// Where the charm hangs from (off its own place: charm_at), the item's stickers, its name tag.
    charm_offset: [f32; 3],
    stickers: Vec<super::sticker::Placed>,
    nametag: Option<String>,
    /// Our material over the item's finish, over the gloves'.
    overlay: Option<super::overlay::Overlay>,
    glove_overlay: Option<super::overlay::Overlay>,
    /// The charm's swing, and the player's velocity the frame before (m/s: its shaking).
    swing: super::charm::Swing,
    vel_prev: Option<[f32; 3]>,
    attach: std::collections::HashMap<String, std::collections::HashMap<String, super::attach::Attachment>>,
    /// A charm's lean out over the weapon's body (charm::lean), by the weapon's model and the charm's.
    leans: std::collections::HashMap<String, f32>,
    /// A rifle's scope, kick and bolt, when the hands hold one.
    rifle: Option<Rifle>,
    /// The field of view the game would have, and the one given it last (degrees, vertical).
    base_fov: f32,
    fov_now: f32,
    /// The scope's bob (pixels), from the walk.
    scope_bob: [f32; 2],
    /// The kit's sounds asked to be read (once its clips are).
    sounds_asked: bool,
    /// The item's finish (cosmetics/finish.rs) and the gloves' (cosmetics/glove.rs): asked for once their files are read.
    finish: Option<Arc<super::compose::Job>>,
    glove_finish: Vec<Arc<super::compose::Job>>,
    finish_asked: bool,
    /// The loadout as last seen (a change in the menu shows at once).
    loadout_version: u64,
    /// A loadout of its own and its version (the page's look in the hands); none: the player's.
    own: Option<(Loadout, u64)>,
    /// Held in the left hand (CS's cl_righthand 0): all of it mirrored across the view.
    left: bool,
    /// The game's hands as Java last said: the main one's item, the other's; the CS item shown is the other hand's (the
    /// main one empty).
    main: String,
    off: String,
    from_off: bool,
}

/// The game's own main hand is the left (its settings), as Java last said.
static MAIN_LEFT: AtomicBool = AtomicBool::new(false);

/// Which hand CS's things are in for the main hand's item: the loadout's choice, or the game's own main hand.
pub fn main_left(l: &Loadout) -> bool {
    if l.hand_game {
        MAIN_LEFT.load(Ordering::Relaxed)
    } else {
        l.left
    }
}

/// What the hands show of the game's: the main hand's item — or, the main hand empty, the other's when CS shows it
/// (a sword in the off hand: the knife in that hand, mirrored); whether it is the other's.
fn pick(s: &State, l: &Loadout) -> (String, bool) {
    let empty = s.main.is_empty() || s.main == "minecraft:air";
    if empty && kit_for(&s.off, l).is_some() {
        (s.off.clone(), true)
    } else {
        (s.main.clone(), false)
    }
}

fn state() -> &'static Mutex<State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(State::default()))
}

/// The page's look's hands.
fn look_state() -> &'static Mutex<State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(State { base_fov: 70.0, ..State::default() }))
}

/// What the hands' loadout is: their own, else the player's (and its version).
fn current_of(s: &State) -> (Loadout, u64) {
    s.own.clone().unwrap_or_else(loadout::current)
}

/// The item the look's hands hold (its own loadout maps it to the slot it shows).
pub const LOOK_ITEM: &str = "cursed:look";

/// The hands' side: the player's choice (CT: CS's SAS sleeves; T: the Phoenix's), the stand's CURSED_DEV_COSM_ARMS
/// over it.
fn side(l: &Loadout) -> String {
    std::env::var("CURSED_DEV_COSM_ARMS").ok().filter(|v| v == "t" || v == "ct").unwrap_or_else(|| l.arms.clone())
}

/// The hands: the side's arms with its default gloves, or — with gloves on — its sleeves and the gloves' own model
/// (the gloves and the forearm, cut under that side's sleeves).
fn arms(l: &Loadout) -> (String, Option<String>, Option<(String, i32, f32)>) {
    let side = side(l);
    let on = l.get(Slot::Gloves);
    let gloves = on.map(|e| format!("items/{}/arms_{side}.rcm", e.item)).filter(|k| store::exists(k));
    let finish = on.filter(|e| !e.paint.is_empty()).map(|e| (e.paint.clone(), e.seed, e.wear));
    match gloves {
        Some(g) => (format!("arms/{side}/sleeves.rcm"), Some(g), finish),
        None => (format!("arms/{side}/arms.rcm"), None, None),
    }
}

/// A rifle kit's numbers.
fn weapon_of(k: &Kit) -> Option<&'static Weapon> {
    if k.item.contains("/weapon_awp/") {
        Some(&AWP)
    } else if k.item.contains("/weapon_ssg08/") {
        Some(&SSG08)
    } else {
        None
    }
}

/// What an item of the game is shown as (none: the game's own hand): the slot the loadout maps it to, the CS item on
/// in it — its legacy body and clips when its finish is painted for those.
fn kit_for(item: &str, l: &Loadout) -> Option<Kit> {
    // all of it off: the game's own hands
    if !l.on {
        return None;
    }
    let slot = l.slot_for(item)?;
    let e = l.get(slot)?;
    let cat = catalog::get();
    let kind = cat.item(&e.item).map(|i| i.kind).unwrap_or(if e.item.starts_with("weapon_knife") || e.item == "weapon_bayonet" { CatKind::Knife } else { CatKind::Rifle });
    let dir = format!("items/{}", e.item);
    let legacy = cat.finish(&e.item, &e.paint).is_some_and(|f| f.legacy) && store::exists(&format!("{dir}/legacy.rcm"));
    let model = if legacy { "legacy.rcm" } else { "model.rcm" };
    let clips = if legacy && store::exists(&format!("{dir}/clips_legacy.rcm")) { "clips_legacy.rcm" } else { "clips.rcm" };
    let (kind, deploy, cycle) = match kind {
        CatKind::Knife => (Kind::Knife, 1.0, 0.0),
        CatKind::Rifle if e.item == "weapon_ssg08" => (Kind::Rifle, SSG08.deploy, SSG08.cycle),
        CatKind::Rifle => (Kind::Rifle, AWP.deploy, AWP.cycle),
        CatKind::Gloves => return None,
    };
    let finish = (!e.paint.is_empty()).then(|| (e.paint.clone(), e.seed, e.wear));
    let (arms, gloves, gloves_finish) = arms(l);
    Some(Kit { arms, gloves, gloves_finish, item: format!("{dir}/{model}"), clips: format!("{dir}/{clips}"), kind, deploy, cycle, finish })
}

/// The game's hands, each tick: the main hand's item (its id, "minecraft:diamond_sword"), the off hand's, whether the
/// game's main hand is the left ("1"), tab between.
pub fn held(said: &str) {
    let mut f = said.split('\t');
    let (main, off) = (f.next().unwrap_or("").to_string(), f.next().unwrap_or("").to_string());
    MAIN_LEFT.store(f.next() == Some("1"), Ordering::Relaxed);
    let mut s = state().lock().unwrap();
    s.main = main;
    s.off = off;
    let (l, _) = current_of(&s);
    let (item, from_off) = pick(&s, &l);
    if s.item == item && s.from_off == from_off {
        // (the game's main hand changed sides: the hands go with it)
        s.left = main_left(&l) ^ s.from_off;
        return;
    }
    s.from_off = from_off;
    shown(&mut s, &item);
}

/// The hands for an item (again when the loadout changes).
fn shown(s: &mut State, item: &str) {
    s.item = item.to_string();
    let (l, v) = current_of(s);
    s.loadout_version = v;
    s.left = main_left(&l) ^ s.from_off;
    s.stattrak = stattrak_of(item, &l);
    s.charm = charm_of(item, &l);
    s.charm_offset = charm_offset_of(item, &l);
    s.stickers = stickers_of(item, &l);
    s.nametag = nametag_of(item, &l);
    s.overlay = overlay_of(item, &l);
    s.glove_overlay = l.get(Slot::Gloves).and_then(|e| e.overlay);
    s.swing = Default::default();
    let kit = kit_for(item, &l);
    s.rifle = kit.as_ref().and_then(weapon_of).map(Rifle::new);
    if kit.is_some() && kit.as_ref() == s.hands.as_ref().map(|h| &h.kit) {
        // the same CS item for another of the game's: taken out again
        s.hands = kit.map(Hands::new);
        return;
    }
    let s = &mut *s;
    s.maps = None;
    s.sounds_asked = false;
    s.finish = None;
    s.glove_finish.clear();
    s.finish_asked = false;
    s.events.clear();
    s.hands = kit.map(|k| {
        for m in [&k.arms, &k.item, &k.clips].into_iter().chain(k.gloves.as_ref()) {
            store::want(m);
        }
        Hands::new(k)
    });
}

/// The StatTrak kills of the CS item an item of the game shows (none: not StatTrak).
fn stattrak_of(item: &str, l: &Loadout) -> Option<u32> {
    l.slot_for(item).and_then(|s| l.get(s)).and_then(|e| e.stattrak)
}

/// The charm on the CS item an item of the game shows.
fn charm_of(item: &str, l: &Loadout) -> Option<String> {
    l.slot_for(item).and_then(|s| l.get(s)).and_then(|e| e.charm.clone())
}

/// Where the charm on the CS item an item of the game shows hangs from (off its own place).
fn charm_offset_of(item: &str, l: &Loadout) -> [f32; 3] {
    l.slot_for(item).and_then(|s| l.get(s)).and_then(|e| e.charm_offset).unwrap_or([0.0; 3])
}

/// The name tag on the CS item an item of the game shows.
fn nametag_of(item: &str, l: &Loadout) -> Option<String> {
    l.slot_for(item).and_then(|s| l.get(s)).and_then(|e| e.nametag.clone()).filter(|n| !n.is_empty())
}

/// Our material over the finish of the CS item an item of the game shows.
fn overlay_of(item: &str, l: &Loadout) -> Option<super::overlay::Overlay> {
    l.slot_for(item).and_then(|s| l.get(s)).and_then(|e| e.overlay)
}

/// The stickers on the CS item an item of the game shows.
fn stickers_of(item: &str, l: &Loadout) -> Vec<super::sticker::Placed> {
    l.slot_for(item).and_then(|s| l.get(s)).map_or(Vec::new(), |e| e.stickers.clone())
}

/// The loadout changed: the hands as they are when what they show is the same (a StatTrak's count, another item's
/// finish), shown anew when not.
fn refresh(s: &mut State) {
    let (l, v) = current_of(s);
    // (the player's hands: which of them CS shows may be another now)
    if s.own.is_none() {
        let (item, from_off) = pick(s, &l);
        if item != s.item || from_off != s.from_off {
            s.from_off = from_off;
            shown(s, &item);
            return;
        }
    }
    s.loadout_version = v;
    s.left = main_left(&l) ^ s.from_off;
    s.stattrak = stattrak_of(&s.item, &l);
    s.stickers = stickers_of(&s.item, &l);
    s.nametag = nametag_of(&s.item, &l);
    s.overlay = overlay_of(&s.item, &l);
    s.glove_overlay = l.get(Slot::Gloves).and_then(|e| e.overlay);
    let charm = charm_of(&s.item, &l);
    let offset = charm_offset_of(&s.item, &l);
    if offset != s.charm_offset {
        s.charm_offset = offset;
        s.swing = Default::default();
    }
    if charm != s.charm {
        s.charm = charm;
        s.swing = Default::default();
    }
    // the same models with another finish (its wear, its pattern): worked anew, the hands as they are
    let kit = kit_for(&s.item, &l);
    let bare = |k: &Kit| Kit { finish: None, gloves_finish: None, ..k.clone() };
    let same = matches!((&kit, &s.hands), (Some(k), Some(h)) if bare(k) == bare(&h.kit));
    if same {
        if let (Some(k), Some(h)) = (kit, s.hands.as_mut()) {
            if k != h.kit {
                h.kit = k;
                s.finish = None;
                s.glove_finish.clear();
                s.finish_asked = false;
            }
        }
    } else if kit.as_ref() != s.hands.as_ref().map(|h| &h.kit) {
        let item = s.item.clone();
        shown(s, &item);
    }
}

/// A kill: StatTrak's count of what is in the hands goes up, when it has one.
pub fn kill() {
    let item = state().lock().unwrap().item.clone();
    let (l, _) = loadout::current();
    // (all of it off: nothing counted)
    if !l.on {
        return;
    }
    let Some(slot) = l.slot_for(&item).filter(|s| l.get(*s).is_some_and(|e| e.stattrak.is_some())) else { return };
    loadout::change(|l| {
        if let Some(n) = l.get_mut(slot).and_then(|e| e.stattrak.as_mut()) {
            *n = n.saturating_add(1);
        }
    });
}

/// The attachments of an item's model (its folder: "items/weapon_awp"), read once.
fn attachments<'a>(s: &'a mut State, dir: &str) -> Option<&'a std::collections::HashMap<String, super::attach::Attachment>> {
    if !s.attach.contains_key(dir) {
        let key = format!("{dir}/attachments.txt");
        let text = store::read(&key).map(|b| String::from_utf8_lossy(&b).into_owned());
        // (none kept while it still comes from the server)
        if text.is_none() && store::coming(&key) {
            return None;
        }
        s.attach.insert(dir.to_string(), super::attach::parse(text.as_deref().unwrap_or_default()));
    }
    s.attach.get(dir)
}

/// A StatTrak module on a model at its attachment (the legacy body's own when it is that): its model's key, its place
/// in the model (for the joints' places `iw`).
pub fn stattrak_module(model_key: &str, item: &Model, iw: &[M4], knife: bool) -> Option<(String, M4)> {
    let dir = model_key.rsplit_once('/')?.0.to_string();
    let mut s = state().lock().unwrap();
    let atts = attachments(&mut s, &dir)?;
    let a = if model_key.ends_with("legacy.rcm") { atts.get("stattrak_legacy").or_else(|| atts.get("stattrak")) } else { atts.get("stattrak") }?;
    let at = super::attach::place(a, item, iw)?;
    let key = if knife { "stattrak/module_knife.rcm" } else { "stattrak/module.rcm" };
    store::want(key);
    store::model(key)?;
    Some((key.to_string(), super::attach::holding(&at)))
}

/// A charm on a model at its keychain (the legacy body's own when it is that).
pub struct CharmAt {
    /// The charm's model's key, its place in the model (for the joints' places given), its chain's top and its end in
    /// its own space (its first joint, its last), its lean out over the weapon's body (radians).
    pub key: String,
    pub held: M4,
    pub top: [f32; 3],
    pub end: [f32; 3],
    pub lean: f32,
}

/// The charm's own place on a model (its keychain attachment; the legacy body's own when it is that).
fn keychain(s: &mut State, model_key: &str) -> Option<super::attach::Attachment> {
    let dir = model_key.rsplit_once('/')?.0.to_string();
    let atts = attachments(s, &dir)?;
    if model_key.ends_with("legacy.rcm") { atts.get("keychain_legacy").or_else(|| atts.get("keychain")) } else { atts.get("keychain") }.cloned()
}

/// A charm's own place on a model as it stands at rest (where its offsets are counted from).
pub fn keychain_rest(model_key: &str, item: &Model) -> Option<M4> {
    let a = keychain(&mut state().lock().unwrap(), model_key)?;
    super::attach::place(&a, item, &item.globals(&item.rest()))
}

/// The charm moved off its own place by `offset` (its place's own frame, metres).
fn moved(at: &M4, offset: [f32; 3]) -> M4 {
    mul(at, &[1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, offset[0], offset[1], offset[2], 1.0])
}

fn charm_at(s: &mut State, model_key: &str, item: &Model, iw: &[M4], name: &str, offset: [f32; 3]) -> Option<CharmAt> {
    let a = keychain(s, model_key)?;
    let at = moved(&super::attach::place(&a, item, iw)?, offset);
    let key = format!("charms/{name}/model.rcm");
    store::want(&key);
    let cm = store::model(&key)?;
    let g = cm.globals(&cm.rest());
    let top = g.first().map_or([0.0; 3], |m| [m[12], m[13], m[14]]);
    let end = g.last().map_or([0.0, -0.03, 0.0], |m| [m[12], m[13], m[14]]);
    // its lean: once for the weapon, the charm and its place, the weapon as it stands at rest
    let lean_key = format!("{model_key}|{key}|{:.3},{:.3},{:.3}", offset[0], offset[1], offset[2]);
    let lean = match s.leans.get(&lean_key) {
        Some(l) => *l,
        None => {
            let rest = item.rest();
            let l = super::attach::place(&a, item, &item.globals(&rest)).map(|at| moved(&at, offset)).map_or(0.0, |at| {
                let held = mul(&super::attach::holding(&at), &[1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, top[0], top[1], top[2], 1.0]);
                let (lo, hi) = cm.bounds_skinned(&cm.skin(&cm.rest()));
                super::charm::lean(&held, &item.skinned(&item.skin(&rest)), top[1] - lo[1], hi[2] - lo[2])
            });
            s.leans.insert(lean_key, l);
            l
        }
    };
    Some(CharmAt { key, held: super::attach::holding(&at), top, end, lean })
}

/// CS2's camera for the inventory's picture of a model (its "camera_inventory" attachment; its own axes: ahead, left,
/// up), in the model as it stands (its joints' places `iw`).
pub fn inventory_camera(model_key: &str, item: &Model, iw: &[M4]) -> Option<M4> {
    let dir = model_key.rsplit_once('/')?.0.to_string();
    // gloves have none of their own: CS2's icon of them (their pair as icon_pose holds them, the left one's back and
    // the right one's palm, the fingers down to the right), the camera found by matching our picture's outline to it
    if catalog::get().item(dir.strip_prefix("items/").unwrap_or(&dir)).is_some_and(|i| i.kind == CatKind::Gloves) {
        return Some(super::attach::source_view(15.0, 266.0, -9.0));
    }
    let mut s = state().lock().unwrap();
    let a = attachments(&mut s, &dir)?.get("camera_inventory")?;
    super::attach::place(a, item, iw)
}

/// A name tag's plate on a model at its attachment (the legacy body's own when it is that): its place in the model.
pub fn nametag_module(model_key: &str, item: &Model, iw: &[M4]) -> Option<M4> {
    let dir = model_key.rsplit_once('/')?.0.to_string();
    let mut s = state().lock().unwrap();
    let atts = attachments(&mut s, &dir)?;
    let a = if model_key.ends_with("legacy.rcm") { atts.get("nametag_legacy").or_else(|| atts.get("nametag")) } else { atts.get("nametag") }?;
    let at = super::attach::place(a, item, iw)?;
    store::want(super::nametag::MODEL);
    store::model(super::nametag::MODEL)?;
    Some(super::attach::holding(&at))
}

/// A charm on a model (the page's look at it), as charm_at.
pub fn charm_module(model_key: &str, item: &Model, iw: &[M4], name: &str, offset: [f32; 3]) -> Option<CharmAt> {
    charm_at(&mut state().lock().unwrap(), model_key, item, iw, name, offset)
}

pub fn event(e: Event) {
    event_in(&mut state().lock().unwrap(), e);
}

/// What the player does with the page's look's hands.
pub fn look_event(e: Event) {
    event_in(&mut look_state().lock().unwrap(), e);
}

fn event_in(s: &mut State, e: Event) {
    if s.hands.is_none() {
        return;
    }
    // a rifle's scope and shot at once (the field of view is asked before the frame is drawn)
    let fov = if s.fov_now > 0.0 { s.fov_now } else { s.base_fov };
    if let Some(r) = s.rifle.as_mut() {
        match e {
            Event::UseStart => {
                if let Some(snd) = r.scope(Some(1), fov) {
                    sound::play(snd);
                }
            }
            Event::UseStop => {
                if let Some(snd) = r.scope(None, fov) {
                    sound::play(snd);
                }
            }
            Event::Shoot => {
                sound::play(r.shoot(fov));
            }
            Event::Zoom if r.zoom > 0 => {
                if let Some(snd) = r.scope(Some(3 - r.zoom), fov) {
                    sound::play(snd);
                }
            }
            _ => {}
        }
    }
    s.events.push(e);
}

/// The field of view for the game's own (`base`, degrees): a rifle's scope's.
pub fn fov(base: f32) -> f32 {
    let mut s = state().lock().unwrap();
    s.base_fov = base;
    let f = s.rifle.as_ref().map_or(base, |r| r.fov(base));
    s.fov_now = f;
    f
}

/// The camera's kick (degrees, the game's: pitch, yaw), when the hands hold a rifle.
pub fn camera() -> [f32; 2] {
    let s = state().lock().unwrap();
    s.rifle.as_ref().map_or([0.0; 2], |r| r.camera())
}

/// Whether the bow may be drawn: not while a rifle's bolt is still coming back.
pub fn ready() -> bool {
    let s = state().lock().unwrap();
    s.rifle.as_ref().is_none_or(|r| r.ready())
}

/// The mouse's speed in the scope (CS's zoom_sensitivity_ratio 1: as the field of view narrows, 4:3 across).
pub fn sensitivity() -> f32 {
    let s = state().lock().unwrap();
    match s.rifle.as_ref() {
        Some(r) if r.zoom > 0 => r.w.zoom_fov[r.zoom as usize - 1] / 90.0,
        _ => 1.0,
    }
}

/// Whether a rifle's scope is up (the wheel is the scope's then).
pub fn scoped() -> bool {
    let s = state().lock().unwrap();
    s.rifle.as_ref().is_some_and(|r| r.zoom > 0)
}

/// The models of a kit (its arms, its item, its clips, its gloves when it has them), once all are read.
fn models(k: &Kit) -> Option<(Arc<Model>, Arc<Model>, Arc<Model>, Option<Arc<Model>>)> {
    let gloves = match &k.gloves {
        Some(g) => Some(store::model(g)?),
        None => None,
    };
    Some((store::model(&k.arms)?, store::model(&k.item)?, store::model(&k.clips)?, gloves))
}

/// CS's crosshair over the picture while the hands hold a CS knife in the first person (CS2 gives its snipers none):
/// how it is set (the loadout's code; CS2's own), how far it is open.
pub fn crosshair() -> Option<(super::crosshair::Crosshair, f32)> {
    let s = state().lock().unwrap();
    let h = s.hands.as_ref()?;
    if h.kit.kind != Kind::Knife || s.last.is_none_or(|l| l.elapsed().as_secs_f32() > 0.25) || models(&h.kit).is_none() {
        return None;
    }
    let (l, _) = loadout::current();
    let mut x = super::crosshair::decode(&l.crosshair).unwrap_or_default();
    // its colour running round (the client's Chroma, chosen on the page)
    if l.crosshair_chroma > 0.0 {
        let [r, g, b] = super::overlay::running([x.color[0], x.color[1], x.color[2]], l.crosshair_chroma);
        x.color = [r, g, b, x.color[3]];
    }
    Some((x, s.spread))
}

/// Whether the hands hold a CS rifle now, its files read (its shot a tracer, its arrow not drawn).
pub fn rifle_shown() -> bool {
    let s = state().lock().unwrap();
    s.hands.as_ref().is_some_and(|h| h.kit.kind == Kind::Rifle && models(&h.kit).is_some())
}

/// Whether the hands are ours now (Java leaves the game's own then): an item shown, its files read.
pub fn active() -> bool {
    let s = state().lock().unwrap();
    s.hands.as_ref().is_some_and(|h| models(&h.kit).is_some())
}

/// The perspective (depth 0 near .. 1 far), vertical `fov` (radians); y up as the game's own (this renderer's
/// viewports turn the picture the way the game's projections expect).
pub fn perspective(fov: f32, aspect: f32, near: f32, far: f32) -> M4 {
    let f = 1.0 / (fov * 0.5).tan();
    [f / aspect.max(1e-3), 0.0, 0.0, 0.0, 0.0, f, 0.0, 0.0, 0.0, 0.0, far / (near - far), -1.0, 0.0, 0.0, near * far / (near - far), 0.0]
}

/// CS's viewmodel_fov 68 is across a 4:3 picture: its height's angle is kept on any other.
pub fn viewmodel_fov() -> f32 {
    2.0 * ((68f32.to_radians() * 0.5).tan() * 0.75).atan()
}

/// The view's basis in the world for the game's yaw and pitch (degrees): right, up, back.
pub fn basis(yaw: f32, pitch: f32) -> [[f32; 3]; 3] {
    let (y, p) = (yaw.to_radians(), pitch.to_radians());
    let fwd = [-y.sin() * p.cos(), -p.sin(), y.cos() * p.cos()];
    let r = {
        let c = [-fwd[2], 0.0, fwd[0]];
        let n = (c[0] * c[0] + c[2] * c[2]).sqrt().max(1e-6);
        [c[0] / n, 0.0, c[2] / n]
    };
    let up = [r[1] * fwd[2] - r[2] * fwd[1], r[2] * fwd[0] - r[0] * fwd[2], r[0] * fwd[1] - r[1] * fwd[0]];
    [r, up, [-fwd[0], -fwd[1], -fwd[2]]]
}

/// This frame's hands.
pub fn frame(f: &Facts) -> Option<Frame> {
    frame_in(&mut state().lock().unwrap(), f)
}

/// The page's look in the hands this frame: its own loadout (`l`, `version` what it shows) holding LOOK_ITEM.
pub fn look_frame(l: &Loadout, version: u64, f: &Facts) -> Option<Frame> {
    let mut guard = look_state().lock().unwrap();
    let s = &mut *guard;
    if s.own.as_ref().is_none_or(|(_, v)| *v != version) {
        s.own = Some((l.clone(), version));
    }
    if s.item != LOOK_ITEM {
        shown(s, LOOK_ITEM);
    }
    frame_in(s, f)
}

/// The page's look's hands put away: taken out again (their draw played) when next shown.
pub fn look_reset() {
    let mut s = look_state().lock().unwrap();
    s.item.clear();
    s.last = None;
}

fn frame_in(s: &mut State, f: &Facts) -> Option<Frame> {
    let now = Instant::now();
    let dt = s.last.map_or(0.0, |l| now.duration_since(l).as_secs_f32().min(0.1));
    s.last = Some(now);
    if current_of(s).1 != s.loadout_version {
        refresh(s);
    }
    let hands = s.hands.as_mut()?;
    let (arms, item, clips, gloves) = models(&hands.kit)?;
    let maps = s.maps.get_or_insert_with(|| (arms.map_to(&clips), item.map_to(&clips), gloves.as_ref().map_or(Vec::new(), |g| g.map_to(&clips))));
    let acts: Vec<Act> = std::mem::take(&mut s.events)
        .into_iter()
        .filter_map(|e| match e {
            Event::Swing(hit) => Some(Act::Swing { hit }),
            Event::Heavy(hit) => Some(Act::Heavy { hit }),
            Event::Inspect => Some(Act::Inspect),
            Event::Shoot => Some(Act::Shoot),
            Event::UseStart | Event::UseStop | Event::Zoom => None,
        })
        .collect();
    if !s.sounds_asked {
        // the kit's sounds read ahead: its clips' and the rifle's own
        s.sounds_asked = true;
        let mut names: Vec<String> = clips.clips.iter().flat_map(|c| c.events.iter().filter(|e| e.kind == 0).map(|e| e.name.clone())).collect();
        if let Some(r) = s.rifle.as_ref() {
            names.extend([r.w.sound_shot, r.w.sound_zoom, r.w.sound_unzoom].map(String::from));
        }
        sound::prepare(names);
    }
    if !s.finish_asked {
        s.finish = finish_job(&hands.kit, &item);
        if let (Some(key), Some(g), Some((paint, seed, wear))) = (hands.kit.gloves.as_ref(), gloves.as_ref(), hands.kit.gloves_finish.as_ref()) {
            s.glove_finish = super::glove::jobs(key, g, paint, *seed, *wear);
        }
        // asked again while what they are made of still comes from the server
        let short = (hands.kit.finish.is_some() && s.finish.is_none()) || (hands.kit.gloves_finish.is_some() && s.glove_finish.is_empty());
        s.finish_asked = !(short && store::busy());
    }
    let pose = hands.pose(&clips, &acts);
    for (kind, name) in hands.take_events() {
        if kind == 0 {
            sound::play(&name);
        }
    }
    let cw = clips.globals(&pose);
    let aw = arms.world_from(&maps.0, &cw, &IDENTITY);
    let gw = gloves.as_ref().map(|g| g.world_from(&maps.2, &cw, &IDENTITY));
    // the item on the "wpn" joint: its own joints where the clip puts them, from there
    let root = Hands::item_root(&clips, &cw);
    let kw: Vec<M4> = cw.iter().map(|m| mul(&root, m)).collect();
    let iw = item.world_from(&maps.1, &kw, &root);
    // the velocity into the viewmodel's space (forward, left, up; inches a second); none up or down on the ground
    let b = basis(f.yaw, f.pitch);
    let on_ground = f.flags & ON_GROUND != 0;
    let mut v = f.vel.map(|x| x * 20.0 / INCH);
    if on_ground {
        v[1] = 0.0;
    }
    let dot = |a: [f32; 3]| a[0] * v[0] + a[1] * v[1] + a[2] * v[2];
    let local = [-dot(b[2]), -dot(b[0]), dot(b[1])];
    // the crosshair opens as CS's does with a knife's running speed (250), wide in the air
    let open = if on_ground { ((v[0] * v[0] + v[2] * v[2]).sqrt() / 250.0).min(1.0) } else { 1.0 };
    s.spread += (open - s.spread) * (1.0 - (-dt * 10.0).exp());
    // Source's yaw turns the other way round
    let mut place = hands.place(-f.yaw.to_radians(), local, on_ground, OFFSET, dt);
    let kit = hands.kit.clone();
    let proj = perspective(viewmodel_fov(), f.aspect, 0.01, 50.0);
    if let Some(r) = s.rifle.as_mut() {
        let speed = (v[0] * v[0] + v[2] * v[2]).sqrt();
        r.moving = Moving { speed, vertical: f.vel[1] * 20.0 / INCH, on_ground, crouching: f.flags & SNEAKING != 0 };
        r.update(dt);
        // the weapon turned past the camera by its share of the kick
        let [p, y] = r.weapon();
        place = mul(&turn(-y, -p), &place);
        if r.zoom > 0 {
            // the scope sways with the walk (a few pixels)
            let k = 1.0 - (-dt * 10.0).exp();
            let t = hands.age();
            let amp = if on_ground { (speed / 250.0).min(1.0) * 6.0 } else { 0.0 };
            let target = [(t * 9.0).sin() * amp, (t * 18.0).sin().abs() * amp * 0.6];
            for i in 0..2 {
                s.scope_bob[i] += (target[i] - s.scope_bob[i]) * k;
            }
            let sv = ScopeView { bob: s.scope_bob, inaccuracy: r.inaccuracy(), line_width: 1, gap: 0 };
            return Some(Frame { draws: Vec::new(), proj, basis: b, scope: Some(sv), finish: s.finish.iter().chain(&s.glove_finish).cloned().collect() });
        }
    }
    // the left hand: the whole of it mirrored across the view (its sway, its kick and all)
    if s.left {
        place = mul(&[-1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0], &place);
    }
    let mut draws = vec![
        Drawn { model: kit.arms.clone(), world: place, skin: arms.skin_of(&aw), finish: Vec::new(), stattrak: None, stickers: Vec::new(), retex: Vec::new(), look: None },
        Drawn {
            model: kit.item.clone(),
            world: place,
            skin: item.skin_of(&iw),
            finish: s.finish.iter().map(|j| (j.material.clone(), j.key.clone())).collect(),
            stattrak: None,
            stickers: super::sticker::draws(&kit.item, &item, &s.stickers),
            retex: Vec::new(),
            look: s.overlay.map(|o| o.shader()),
        },
    ];
    if let (Some(key), Some(g), Some(gw)) = (kit.gloves.clone(), gloves.as_ref(), gw.as_ref()) {
        draws.push(Drawn { model: key, world: place, skin: g.skin_of(gw), finish: s.glove_finish.iter().map(|j| (j.material.clone(), j.key.clone())).collect(), stattrak: None, stickers: Vec::new(), retex: Vec::new(), look: s.glove_overlay.map(|o| o.shader()) });
    }
    // the charm on its keychain, swinging (a rifle's)
    let accel = {
        let v = f.vel.map(|x| x * 20.0);
        let a = s.vel_prev.map_or([0.0; 3], |p| std::array::from_fn(|k| (v[k] - p[k]) / dt.max(1e-3)));
        s.vel_prev = Some(v);
        a
    };
    if let Some(name) = s.charm.clone().filter(|_| kit.kind == Kind::Rifle) {
        let offset = s.charm_offset;
        if let Some(c) = charm_at(s, &kit.item, &item, &iw, &name, offset) {
            // the chain's top and its hanging as held (the view's space)
            let held = mul(&place, &c.held);
            let (t, e) = (super::charm::point(&held, c.top), super::charm::point(&held, c.end));
            let swing = s.swing.step(t, [e[0] - t[0], e[1] - t[1], e[2] - t[2]], super::charm::wall(&held, c.lean), b, accel, dt);
            let skin = store::model(&c.key).map(|cm| cm.skin(&cm.rest())).unwrap_or_default();
            draws.push(Drawn { model: c.key, world: mul(&swing, &held), skin, finish: Vec::new(), stattrak: None, stickers: Vec::new(), retex: Vec::new(), look: None });
        }
    }
    modules(s, &kit, &item, &iw, &place, &mut draws);
    Some(Frame {
        draws,
        proj,
        basis: b,
        scope: None,
        finish: s.finish.iter().chain(&s.glove_finish).cloned().collect(),
    })
}

/// The item's modules where it holds them (`iw` its joints' places, `place` its own in the view): StatTrak's, its
/// count on its display; the name tag's plate, the name written on it.
fn modules(s: &mut State, kit: &Kit, item: &Model, iw: &[M4], place: &M4, draws: &mut Vec<Drawn>) {
    if let Some(n) = s.stattrak {
        let (dir, knife) = (kit.item.rsplit_once('/').map_or("", |(d, _)| d).to_string(), kit.kind == Kind::Knife);
        if let Some(a) = attachments(s, &dir).and_then(|atts| atts.get(if kit.item.ends_with("legacy.rcm") { "stattrak_legacy" } else { "stattrak" }).or_else(|| atts.get("stattrak")).cloned()) {
            let key = if knife { "stattrak/module_knife.rcm" } else { "stattrak/module.rcm" };
            store::want(key);
            if let (Some(at), Some(_)) = (super::attach::place(&a, item, iw), store::model(key)) {
                draws.push(Drawn { model: key.to_string(), world: mul(place, &super::attach::holding(&at)), skin: Vec::new(), finish: Vec::new(), stattrak: Some(n), stickers: Vec::new(), retex: Vec::new(), look: None });
            }
        }
    }
    if let Some(name) = s.nametag.clone() {
        let dir = kit.item.rsplit_once('/').map_or("", |(d, _)| d).to_string();
        if let Some(a) = attachments(s, &dir).and_then(|atts| atts.get(if kit.item.ends_with("legacy.rcm") { "nametag_legacy" } else { "nametag" }).or_else(|| atts.get("nametag")).cloned()) {
            store::want(super::nametag::MODEL);
            if let (Some(at), Some(_)) = (super::attach::place(&a, item, iw), store::model(super::nametag::MODEL)) {
                draws.push(Drawn {
                    model: super::nametag::MODEL.to_string(),
                    world: mul(place, &super::attach::holding(&at)),
                    skin: Vec::new(),
                    finish: Vec::new(),
                    stattrak: None,
                    stickers: Vec::new(),
                    retex: vec![(super::nametag::MATERIAL.to_string(), super::nametag::texture(&name))],
                    look: None,
                });
            }
        }
    }
}

/// Which of the game's hands CS shows its item in, for the player seen from behind (F5): "main" or "off", then what it
/// is ("knife", "rifle"); none while it shows none (or its models are not here yet).
pub fn third_hand() -> String {
    let s = state().lock().unwrap();
    let Some(h) = s.hands.as_ref().filter(|h| models(&h.kit).is_some()) else { return String::new() };
    format!("{} {}", if s.from_off { "off" } else { "main" }, if h.kit.kind == Kind::Rifle { "rifle" } else { "knife" })
}

/// How the game holds an item in a hand seen from behind: its display's move (pixels), its turn (degrees about x, y and
/// z, in that order), its size; where its grip is on its picture (the item's own units, its middle at 0).
struct Display {
    t: [f32; 3],
    r: [f32; 3],
    scale: f32,
    grip: [f32; 2],
}

/// A handheld item's (item/handheld's thirdperson_righthand: a sword).
const SWORD: Display = Display { t: [0.0, 4.0, 0.5], r: [0.0, -90.0, 55.0], scale: 0.85, grip: [-0.375, -0.375] };
/// How much larger than life CS's items are held, as the game's blocky hands are: a knife, a rifle.
const KNIFE_SIZE: f32 = 1.7;
const RIFLE_SIZE: f32 = 1.1;

/// CS's item in the game's hold (the frame the game draws its item in at the hand) as the game holds its own there:
/// a knife as a sword (its blade, its grip's frame's z, along the sword's; its grip where the sword's is), a rifle ahead
/// along the arms as CS's agents hold theirs — larger than life as the game's blocky hands are (`left`: the left
/// hand's, a knife's display mirrored as the game mirrors it).
fn third_hold(rifle: bool, left: bool) -> M4 {
    let about = |axis: usize, deg: f32| -> M4 {
        let (s, c) = deg.to_radians().sin_cos();
        let mut m = IDENTITY;
        let (a, b) = ((axis + 1) % 3, (axis + 2) % 3);
        m[a * 4 + a] = c;
        m[a * 4 + b] = s;
        m[b * 4 + a] = -s;
        m[b * 4 + b] = c;
        m
    };
    let moved = |t: [f32; 3]| -> M4 {
        let mut m = IDENTITY;
        m[12] = t[0];
        m[13] = t[1];
        m[14] = t[2];
        m
    };
    let sized = |k: f32| -> M4 {
        let mut m = IDENTITY;
        m[0] = k;
        m[5] = k;
        m[10] = k;
        m
    };
    if rifle {
        // the barrel (x) ahead past the hands (the hold's −z, the arms raised ahead in the crossbow's hold), its top
        // (z) up (the hold's y), its left the rest — the same in either hand
        let axes: M4 = [0.0, 0.0, -1.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0];
        return mul(&sized(RIFLE_SIZE), &axes);
    }
    let d = &SWORD;
    let (mut t, mut r) = (d.t, d.r);
    if left {
        (t[0], r[1], r[2]) = (-t[0], -r[1], -r[2]);
    }
    // the blade (its grip's frame's z) up the sword's picture's diagonal, the flat's side (y) out of the picture, its
    // edge (x) across; its grip a little down its handle from its frame's middle
    let h = std::f32::consts::FRAC_1_SQRT_2;
    let axes: M4 = [-h, h, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, h, h, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0];
    let turn = mul(&about(0, r[0]), &mul(&about(1, r[1]), &about(2, r[2])));
    let mut m = mul(&moved(t.map(|x| x / 16.0)), &turn);
    m = mul(&m, &moved([d.grip[0] * d.scale, d.grip[1] * d.scale, 0.0]));
    m = mul(&m, &sized(KNIFE_SIZE));
    m = mul(&m, &axes);
    mul(&m, &moved([0.0, 0.0, 0.055]))
}

/// CS's item in the player's hand seen from behind (F5): at `pose` (where the game holds its item there, from the
/// camera), its model as it rests, held by its grip (CS2's weapon_offset joint: its attachments' own) turned into the
/// game's hold; its stickers on it, its charm hanging, StatTrak's module, its name tag.
pub fn third(pose: &M4) -> Option<Frame> {
    let mut guard = state().lock().unwrap();
    let s = &mut *guard;
    if current_of(s).1 != s.loadout_version {
        refresh(s);
    }
    let hands = s.hands.as_ref()?;
    let (_, item, _, _) = models(&hands.kit)?;
    let kit = hands.kit.clone();
    if !s.finish_asked {
        s.finish = finish_job(&kit, &item);
        let short = kit.finish.is_some() && s.finish.is_none();
        s.finish_asked = !(short && store::busy());
    }
    let rest = item.rest();
    let skin = item.skin(&rest);
    let iw = item.globals(&rest);
    let grip = item.joint("weapon_offset").and_then(|j| iw.get(j)).and_then(super::model::inverse).unwrap_or(IDENTITY);
    let place = mul(pose, &mul(&third_hold(kit.kind == Kind::Rifle, MAIN_LEFT.load(Ordering::Relaxed) ^ s.from_off), &grip));
    let mut draws = vec![Drawn {
        model: kit.item.clone(),
        world: place,
        skin,
        finish: s.finish.iter().map(|j| (j.material.clone(), j.key.clone())).collect(),
        stattrak: None,
        stickers: super::sticker::draws(&kit.item, &item, &s.stickers),
        retex: Vec::new(),
        look: s.overlay.map(|o| o.shader()),
    }];
    // the charm as it hangs at rest (a rifle's)
    if let Some(name) = s.charm.clone().filter(|_| kit.kind == Kind::Rifle) {
        let offset = s.charm_offset;
        if let Some(c) = charm_at(s, &kit.item, &item, &iw, &name, offset) {
            let skin = store::model(&c.key).map(|cm| cm.skin(&cm.rest())).unwrap_or_default();
            draws.push(Drawn { model: c.key, world: mul(&place, &c.held), skin, finish: Vec::new(), stattrak: None, stickers: Vec::new(), retex: Vec::new(), look: None });
        }
    }
    modules(s, &kit, &item, &iw, &place, &mut draws);
    Some(Frame { draws, proj: IDENTITY, basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], scope: None, finish: s.finish.iter().cloned().collect() })
}

/// The item's finish as chosen (the loadout's; the stand's CURSED_DEV_COSM_FINISH="paint:seed:wear" over it).
fn finish_job(kit: &Kit, item: &Model) -> Option<Arc<super::compose::Job>> {
    // the stand's choice over the loadout's
    let (paint_name, seed, wear) = match std::env::var("CURSED_DEV_COSM_FINISH").ok() {
        Some(choice) => {
            let mut parts = choice.split(':');
            (parts.next()?.to_string(), parts.next().and_then(|v| v.parse().ok()).unwrap_or(0), parts.next().and_then(|v| v.parse().ok()).unwrap_or(0.0))
        }
        None => kit.finish.clone()?,
    };
    finish_job_for(&kit.item, item, &paint_name, seed, wear)
}

/// A finish's job for a model (its key, the model read): its files read, its numbers put together, its textures
/// asked for.
pub fn finish_job_for(model_key: &str, item: &Model, paint_name: &str, seed: i32, wear: f32) -> Option<Arc<super::compose::Job>> {
    use super::compose::{Out, Pass, Vertex};
    use super::finish::{self, Combo, Inputs, Instance, Paint};
    let dir = model_key.rsplit_once('/').map_or("", |(d, _)| d).to_string();
    let text = |key: &str| store::read(key).map(|b| String::from_utf8_lossy(&b).into_owned());
    // the item's material that takes finishes (the one with inputs beside it)
    let (material, inputs_text) = item.materials.iter().find_map(|m| text(&format!("{dir}/finish/{}.txt", m.name)).map(|t| (m.name.clone(), t)))?;
    let inputs = Inputs::parse(&inputs_text, &format!("{dir}/finish"));
    let pdir = format!("paint/{paint_name}");
    let paint = Paint::parse(&text(&format!("{pdir}/paint.txt"))?, &pdir);
    let id = finish::combo_id(&inputs, &paint);
    let combo = Combo::parse(&text(&format!("finish/shaders/{id}.txt"))?);
    let spv = store::read(&format!("finish/shaders/{id}.spv"))?;
    let spirv: Vec<u32> = spv.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
    let (lo, hi) = paint.wear_range();
    let p = finish::params(&inputs, &paint, Instance { seed, wear: wear.clamp(lo, hi) });
    let textures = finish::textures(&combo, &inputs, &paint, &p);
    for k in textures.iter().filter_map(|(_, k)| k.as_ref()) {
        store::want(k);
    }
    // the colour, then the roughness and metalness (F_ROUGHNESS_MODE)
    let (spirv, info, xforms) = (Arc::new(spirv), Arc::new(combo), finish::vertex_xforms(&p));
    let address = finish::pattern_address(&info, &p);
    let passes = [(Out::Color, false), (Out::Metal, true)]
        .map(|(out, rough)| Pass {
            out,
            combo: id,
            spirv: spirv.clone(),
            consts: finish::constants(&info, &p, &finish::features(&info.features, &inputs, &paint, rough)),
            address: address.clone(),
            info: info.clone(),
            textures: textures.clone(),
            vertex: Vertex::Weapon(xforms),
        })
        .into();
    Some(Arc::new(super::compose::Job { key: format!("finish/{dir}/{material}/{paint_name}/{seed}/{wear}"), model: model_key.to_string(), material, passes, size: 2048 }))
}

/// A turn about the eye: `yaw` about up, then `pitch` about right (degrees; up and left positive).
fn turn(yaw: f32, pitch: f32) -> M4 {
    let (sy, cy) = yaw.to_radians().sin_cos();
    let (sp, cp) = pitch.to_radians().sin_cos();
    let ry: M4 = [cy, 0.0, -sy, 0.0, 0.0, 1.0, 0.0, 0.0, sy, 0.0, cy, 0.0, 0.0, 0.0, 0.0, 1.0];
    let rx: M4 = [1.0, 0.0, 0.0, 0.0, 0.0, cp, sp, 0.0, 0.0, -sp, cp, 0.0, 0.0, 0.0, 0.0, 1.0];
    mul(&ry, &rx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_view_basis_is_the_games() {
        // yaw 0 looks south (+z): right is west (-x), back is north (-z)
        let b = basis(0.0, 0.0);
        assert!((b[0][0] + 1.0).abs() < 1e-5 && (b[2][2] + 1.0).abs() < 1e-5, "{b:?}");
        // looking down: back points up
        let b = basis(0.0, 90.0);
        assert!((b[2][1] - 1.0).abs() < 1e-4, "{b:?}");
        // the viewmodel's angle: about 54° high
        assert!((viewmodel_fov().to_degrees() - 53.7).abs() < 0.2);
    }

    #[test]
    fn a_point_ahead_lands_in_the_picture() {
        let p = perspective(1.0, 1.5, 0.01, 50.0);
        // a point 1 ahead (−z), a little up: inside, depth between 0 and 1, y up as the game's
        let v = [0.0f32, 0.1, -1.0, 1.0];
        let c: [f32; 4] = std::array::from_fn(|r| (0..4).map(|k| p[k * 4 + r] * v[k]).sum());
        let (x, y, z) = (c[0] / c[3], c[1] / c[3], c[2] / c[3]);
        assert!(x.abs() < 1e-6 && y > 0.0 && (0.0..1.0).contains(&z), "{x} {y} {z}");
    }
}
