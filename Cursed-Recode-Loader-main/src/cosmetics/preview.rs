//! The Cosmetics page's look at an item (gui/cosmui.rs puts one in the menu's frame that shows it, and where): the
//! item alone, its finish worked as it would be on, turned as the player turns it (no turning of its own: her word; a
//! fling carries on and slows), lit as in a studio, coming in once all of it is there, over a dark glossy floor that
//! mirrors it — drawn by the engine into the menu's picture after the menu (engine/gpu.rs viewmodel_preview).

use super::model::{mul, Model, M4};
use super::store;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, PartialEq)]
pub struct Preview {
    /// Where the item stands, in the menu's picture (px); the whole of its place (its floor and its reflection under it).
    pub rect: [f32; 4],
    pub frame: [f32; 4],
    pub item: String,
    pub paint: String,
    pub seed: i32,
    pub wear: f32,
    /// The player's turn of it (radians) and how near (1 as framed).
    pub yaw: f32,
    pub pitch: f32,
    pub zoom: f32,
    /// StatTrak's module on it, its number.
    pub stattrak: Option<u32>,
    /// The charm on it (a rifle's): its name, where it hangs from (off its own place).
    pub charm: Option<String>,
    pub charm_offset: [f32; 3],
    /// The stickers on it.
    pub stickers: Vec<super::sticker::Placed>,
    /// Its name tag, our material over its finish.
    pub nametag: Option<String>,
    pub overlay: Option<super::overlay::Overlay>,
    /// Where its light comes from: its turn about up, its height (radians).
    pub light: [f32; 2],
    /// How much of it the page shows (1; less as it goes away).
    pub alpha: f32,
    /// In CS's hands, as the game would hold it (its yaw and pitch the view's then).
    pub in_hand: bool,
    /// How much larger it is drawn, then brought down (the loadout's quality: 1..3).
    pub ss: u8,
}

/// The look's own loadout for its hands: the player's with the look's item on its slot (as it is shown, not yet on) —
/// gloves held with the player's knife (or a plain karambit) — and the item the hands hold mapped to that slot; its
/// version (what it shows).
pub fn hand_loadout(p: &Preview) -> Option<(super::loadout::Loadout, u64)> {
    use super::catalog::Kind;
    use super::loadout::{Equip, Slot};
    use std::hash::{Hash, Hasher};
    let (mut l, _) = super::loadout::current();
    // (the page shows it in the hands whether or not they are the game's)
    l.on = true;
    let kind = super::catalog::get().item(&p.item)?.kind;
    let slot = Slot::of(&p.item, kind);
    let charm_offset = (p.charm_offset != [0.0; 3]).then_some(p.charm_offset);
    let nametag = p.nametag.clone().filter(|n| !n.is_empty());
    l.set(slot, Some(Equip { item: p.item.clone(), paint: p.paint.clone(), seed: p.seed, wear: p.wear, stattrak: p.stattrak, charm: p.charm.clone(), charm_offset, stickers: p.stickers.clone(), nametag, overlay: p.overlay }));
    let held = if kind == Kind::Gloves {
        if l.get(Slot::Knife).is_none() {
            l.set(Slot::Knife, Some(Equip { item: "weapon_knife_karambit".into(), paint: String::new(), seed: 0, wear: 0.0, stattrak: None, charm: None, charm_offset: None, stickers: Vec::new(), nametag: None, overlay: None }));
        }
        Slot::Knife
    } else {
        slot
    };
    l.map = vec![(super::view::LOOK_ITEM.rsplit_once(':').map_or("", |(_, w)| w).to_string(), Some(held))];
    let mut h = std::collections::hash_map::DefaultHasher::new();
    l.dump().hash(&mut h);
    Some((l, h.finish()))
}

/// How far the engine has brought the last look in (0 while its model, textures or finish are still coming, 1 shown):
/// the page waits with a sign of it until then.
static SEEN: AtomicU32 = AtomicU32::new(0);

pub fn set_seen(k: f32) {
    SEEN.store(k.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
}

pub fn seen() -> f32 {
    f32::from_bits(SEEN.load(Ordering::Relaxed))
}

/// The item's model key (its legacy body when its finish is painted for it).
pub fn model_key(item: &str, paint: &str) -> String {
    let dir = format!("items/{item}");
    let legacy = super::catalog::get().finish(item, paint).is_some_and(|f| f.legacy) && store::exists(&format!("{dir}/legacy.rcm"));
    format!("{dir}/{}", if legacy { "legacy.rcm" } else { "model.rcm" })
}

/// The look's field of view (vertical, degrees).
pub const FOV: f32 = 32.0;

/// How the model stands in the look: CS2's own pose for its inventory's picture when it has one (gloves: the pair held
/// together), else as bound — its skin's matrices, the box round it so, its joints' places (kept for the model shown).
pub fn pose(m: &Model, key: &str) -> (Vec<M4>, ([f32; 3], [f32; 3]), Vec<M4>) {
    type Kept = (String, Vec<M4>, ([f32; 3], [f32; 3]), Vec<M4>);
    static LAST: Mutex<Option<Kept>> = Mutex::new(None);
    let mut last = LAST.lock().unwrap();
    if last.as_ref().is_none_or(|(k, ..)| k != key) {
        let mut rest = m.rest();
        if let Some(c) = m.clip("icon_pose") {
            c.sample_into(0.0, &mut rest);
        }
        let skin = m.skin(&rest);
        let bounds = m.bounds_skinned(&skin);
        *last = Some((key.to_string(), skin, bounds, m.globals(&rest)));
    }
    let (_, skin, bounds, joints) = last.as_ref().unwrap();
    (skin.clone(), *bounds, joints.clone())
}

/// A box's sides from the longest: which of its axes, how long.
fn sides((lo, hi): ([f32; 3], [f32; 3])) -> ([usize; 3], [f32; 3]) {
    let e = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
    let mut ax = [0usize, 1, 2];
    ax.sort_by(|a, b| e[*b].total_cmp(&e[*a]));
    (ax, e)
}

/// The model laid as CS's inventory shows its things: its longest side across, the next one up, its thinnest toward
/// the eye (a knife's flat, a rifle's side) — turned, never mirrored.
fn lay(ax: [usize; 3]) -> M4 {
    let mut r = [0f32; 16];
    r[15] = 1.0;
    for (to, &from) in ax.iter().enumerate() {
        r[from * 4 + to] = 1.0;
    }
    // an odd shuffle of the axes would mirror it: its depth turned round instead
    let inversions = (0..3).flat_map(|i| (i + 1..3).map(move |j| (i, j))).filter(|&(i, j)| ax[i] > ax[j]).count();
    if inversions % 2 == 1 {
        r[ax[2] * 4 + 2] = -1.0;
    }
    r
}

/// The look from CS2's inventory camera (`cam`: its place in the model, its own axes ahead, left, up): the model
/// turned so that its right, its up and its back are the look's.
fn facing(cam: &M4) -> M4 {
    let unit = |v: [f32; 3]| {
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-9);
        v.map(|x| x / l)
    };
    let ahead = unit([cam[0], cam[1], cam[2]]);
    let left = unit([cam[4], cam[5], cam[6]]);
    let up = unit([cam[8], cam[9], cam[10]]);
    let mut r = [0f32; 16];
    for k in 0..3 {
        r[k * 4] = -left[k];
        r[k * 4 + 1] = up[k];
        r[k * 4 + 2] = -ahead[k];
    }
    r[15] = 1.0;
    r
}

/// How the look turns the model: as CS2's inventory camera sees it (`cam`), else laid by its box (`bounds`, as it
/// stands) as CS's inventory lays things.
pub fn turn(bounds: ([f32; 3], [f32; 3]), cam: Option<&M4>) -> M4 {
    cam.map_or_else(|| lay(sides(bounds).0), facing)
}

/// The model as the look turns it: the box round its drawn vertices (as `skin` places them), and some of them (enough
/// to frame it by).
#[derive(Clone, Debug, Default)]
pub struct Turned {
    pub lo: [f32; 3],
    pub hi: [f32; 3],
    pub points: Vec<[f32; 3]>,
}

/// The model turned by `turn` (kept for the model shown).
pub fn turned(key: &str, m: &Model, skin: &[M4], turn: &M4) -> Arc<Turned> {
    type Kept = (String, M4, Arc<Turned>);
    static LAST: Mutex<Option<Kept>> = Mutex::new(None);
    let mut last = LAST.lock().unwrap();
    if last.as_ref().is_none_or(|(k, t, _)| k != key || t != turn) {
        let all = m.skinned(skin);
        let every = all.len().div_ceil(4096).max(1);
        let mut t = Turned { lo: [f32::MAX; 3], hi: [f32::MIN; 3], points: Vec::with_capacity(all.len() / every + 1) };
        for (n, v) in all.iter().enumerate() {
            let q: [f32; 3] = std::array::from_fn(|i| turn[i] * v[0] + turn[4 + i] * v[1] + turn[8 + i] * v[2]);
            for i in 0..3 {
                t.lo[i] = t.lo[i].min(q[i]);
                t.hi[i] = t.hi[i].max(q[i]);
            }
            if n % every == 0 {
                t.points.push(q);
            }
        }
        *last = Some((key.to_string(), *turn, Arc::new(t)));
    }
    last.as_ref().unwrap().2.clone()
}

/// The player's turn of the look (radians: about up, then about across).
fn turned_by(yaw: f32, pitch: f32) -> M4 {
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    let ry: M4 = [cy, 0.0, -sy, 0.0, 0.0, 1.0, 0.0, 0.0, sy, 0.0, cy, 0.0, 0.0, 0.0, 0.0, 1.0];
    let rx: M4 = [1.0, 0.0, 0.0, 0.0, 0.0, cp, sp, 0.0, 0.0, -sp, cp, 0.0, 0.0, 0.0, 0.0, 1.0];
    mul(&rx, &ry)
}

/// The model (as `turned` turns it) with its middle at the origin, its width across 1.
fn centre_of(turned: &Turned) -> M4 {
    let (lo, hi) = (turned.lo, turned.hi);
    let c = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5, (lo[2] + hi[2]) * 0.5];
    let s = 1.0 / (hi[0] - lo[0]).max(1e-4);
    [s, 0.0, 0.0, 0.0, 0.0, s, 0.0, 0.0, 0.0, 0.0, s, 0.0, -c[0] * s, -c[1] * s, -c[2] * s, 1.0]
}

/// Where the model sits: turned by `turn` (as `turned` is), its middle at the origin, its width across 1, turned by
/// the player's turn; then set back from the eye and aside so that, as it first shows, it stands in the middle of the
/// place and each of its parts within its eight tenths across and seven up — the near ones the larger (`aspect` its
/// width over its height; −z ahead). Without CS2's camera (`cam`) a little turned, as the look always was. Turned so
/// that it would reach under where it reaches as it first stands (turned any way about up), it is lifted as far: the
/// floor under it stays where it is, and the item moves only as it must.
pub fn place(turned: &Turned, turn: &M4, p: &Preview, aspect: f32, cam: bool) -> M4 {
    let centre = centre_of(turned);
    let (o, back, [y0, p0]) = aside(turned, &centre, aspect, cam);
    let now = mul(&turned_by(p.yaw + y0, p.pitch + p0), &centre);
    let lowest = turned.points.iter().map(|q| now[1] * q[0] + now[5] * q[1] + now[9] * q[2] + now[13]).fold(0.0f32, f32::min);
    // (down to a little over the floor: `ground` lays it 0.05 under)
    let lift = (low(turned, &centre, p0) - 0.035 - lowest).max(0.0);
    // nearer by the wheel: what stands in the middle stays there
    let z = p.zoom.clamp(0.5, 3.0);
    let at: M4 = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, o[0] / z, o[1] / z + lift, -back / z, 1.0];
    mul(&at, &mul(&turned_by(p.yaw + y0, p.pitch + p0), &mul(&centre, turn)))
}

/// How far below its middle the model (`centre`d) reaches tilted by `tilt` (radians, about across) after any turn
/// about up: each point's height tilted less its reach round up, the lowest of them.
fn low(turned: &Turned, centre: &M4, tilt: f32) -> f32 {
    let (sp, cp) = tilt.sin_cos();
    turned
        .points
        .iter()
        .map(|q| {
            let v: [f32; 3] = std::array::from_fn(|k| centre[k] * q[0] + centre[4 + k] * q[1] + centre[8 + k] * q[2] + centre[12 + k]);
            cp * v[1] - sp.abs() * (v[0] * v[0] + v[2] * v[2]).sqrt()
        })
        .fold(0.0f32, f32::min)
}

/// How far aside and back from the eye the look sets the model (`centre`d) as it first shows, and that first turn of it
/// (about up, about across).
fn aside(turned: &Turned, centre: &M4, aspect: f32, cam: bool) -> ([f32; 2], f32, [f32; 2]) {
    let (y0, p0) = if cam { (0.0, 0.0) } else { (0.35, 0.12) };
    // as it first shows: its parts where they would be, the place's share of them (|x| / (back − z) across, up)
    let first = mul(&turned_by(y0, p0), centre);
    let pts: Vec<[f32; 3]> = turned.points.iter().map(|q| std::array::from_fn(|k| first[k] * q[0] + first[4 + k] * q[1] + first[8 + k] * q[2] + first[12 + k])).collect();
    let t = (FOV.to_radians() * 0.5).tan();
    let (kx, ky) = (0.8 * t * aspect.max(0.2), 0.7 * t);
    let fit = |o: [f32; 2]| pts.iter().fold(0.1f32, |d, v| d.max(((v[0] + o[0]).abs() / kx).max((v[1] + o[1]).abs() / ky) + v[2]));
    // aside until what it shows stands in the middle (its near end the larger), each time as far as it then needs
    let mut o = [0f32; 2];
    let mut back = fit(o);
    for _ in 0..4 {
        let (mut ux, mut uy) = ([f32::MAX, f32::MIN], [f32::MAX, f32::MIN]);
        for v in &pts {
            let d = (back - v[2]).max(1e-3);
            let (x, y) = ((v[0] + o[0]) / d, (v[1] + o[1]) / d);
            ux = [ux[0].min(x), ux[1].max(x)];
            uy = [uy[0].min(y), uy[1].max(y)];
        }
        o[0] -= (ux[0] + ux[1]) * 0.5 * back;
        o[1] -= (uy[0] + uy[1]) * 0.5 * back;
        back = fit(o);
    }
    (o, back, [y0, p0])
}

/// The floor under the look's item: the item's middle in the look's space, and the floor's height — as low as the item
/// reaches turning about its up as it first stands, a little lower (turning it never takes it through the floor, a
/// tilt lifts it: `place`; its reflection begins right under it). It stays where it is as the item is turned: only
/// the wheel's nearness moves it (the eye nearer).
pub fn ground(p: &Preview, m: &Model, key: &str, aspect: f32) -> ([f32; 3], f32) {
    let (skin, bounds, joints) = pose(m, key);
    let cam = super::view::inventory_camera(key, m, &joints);
    let t = turn(bounds, cam.as_ref());
    let tu = turned(key, m, &skin, &t);
    let centre = centre_of(&tu);
    let (o, back, [_, p0]) = aside(&tu, &centre, aspect, cam.is_some());
    let z = p.zoom.clamp(0.5, 3.0);
    let mid = [o[0] / z, o[1] / z, -back / z];
    (mid, mid[1] + low(&tu, &centre, p0) - 0.05)
}

/// Where the floor right under the look's item shows in the menu's picture (px), and how many px the item's width
/// is there; once its model is read.
pub fn under(p: &Preview) -> Option<([f32; 2], f32)> {
    let key = model_key(&p.item, &p.paint);
    let m = store::model(&key)?;
    let (w, h) = ((p.rect[2] - p.rect[0]).max(1.0), (p.rect[3] - p.rect[1]).max(1.0));
    let (mid, floor) = ground(p, &m, &key, w / h);
    let t = (FOV.to_radians() * 0.5).tan();
    let d = (-mid[2]).max(1e-3);
    let (nx, ny) = (mid[0] / (d * t * w / h), -floor / (d * t));
    Some(([p.rect[0] + (nx + 1.0) * 0.5 * w, p.rect[1] + (ny + 1.0) * 0.5 * h], 0.5 * h / (d * t)))
}

/// Where the look puts the model (its key, the model read; `aspect` the place's width over its height): its place in
/// the look, its skin's matrices and its joints' places as it stands, CS2's inventory camera's place when it has one.
pub fn world(p: &Preview, m: &Model, key: &str, aspect: f32) -> (M4, Vec<M4>, Vec<M4>, Option<M4>) {
    let (skin, bounds, joints) = pose(m, key);
    let cam = super::view::inventory_camera(key, m, &joints);
    let t = turn(bounds, cam.as_ref());
    let world = place(&turned(key, m, &skin, &t), &t, p, aspect, cam.is_some());
    (world, skin, joints, cam)
}

/// The ray (the model as bound: from, along) under a point of the menu's picture (px) in the look, once its model is
/// read.
pub fn ray(p: &Preview, at: [f32; 2]) -> Option<([f32; 3], [f32; 3])> {
    let key = model_key(&p.item, &p.paint);
    let m = store::model(&key)?;
    let (w, h) = ((p.rect[2] - p.rect[0]).max(1.0), (p.rect[3] - p.rect[1]).max(1.0));
    let (world, ..) = world(p, &m, &key, w / h);
    let back = super::model::inverse(&world)?;
    let t = (FOV.to_radians() * 0.5).tan();
    // the place's middle the look's ahead (−z), its up the look's up (the picture runs down)
    let nx = (at[0] - p.rect[0]) / w * 2.0 - 1.0;
    let ny = (at[1] - p.rect[1]) / h * 2.0 - 1.0;
    let d = [nx * t * w / h, -ny * t, -1.0];
    let o: [f32; 3] = std::array::from_fn(|k| back[12 + k]);
    let dir: [f32; 3] = std::array::from_fn(|k| back[k] * d[0] + back[4 + k] * d[1] + back[8 + k] * d[2]);
    let l = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt().max(1e-9);
    Some((o, dir.map(|x| x / l)))
}

/// Where a point of the model (as bound) shows in the menu's picture (px), once its model is read.
pub fn project(p: &Preview, q: [f32; 3]) -> Option<[f32; 2]> {
    let key = model_key(&p.item, &p.paint);
    let m = store::model(&key)?;
    let (w, h) = ((p.rect[2] - p.rect[0]).max(1.0), (p.rect[3] - p.rect[1]).max(1.0));
    let (world, ..) = world(p, &m, &key, w / h);
    let v: [f32; 3] = std::array::from_fn(|k| world[k] * q[0] + world[4 + k] * q[1] + world[8 + k] * q[2] + world[12 + k]);
    if v[2] > -1e-4 {
        return None;
    }
    let t = (FOV.to_radians() * 0.5).tan();
    let (nx, ny) = (v[0] / (-v[2] * t * w / h), -v[1] / (-v[2] * t));
    Some([p.rect[0] + (nx + 1.0) * 0.5 * w, p.rect[1] + (ny + 1.0) * 0.5 * h])
}

/// The look's charm swinging as the player turns the item: its turn about its top (the look's space, its down the
/// world's; `top` and `rest` its chain's top and its hanging there as held). A swing of its own, begun again for
/// another item or charm and after a pause.
pub fn charm_swing(id: &str, top: [f32; 3], rest: [f32; 3], side: [f32; 3]) -> M4 {
    static LAST: Mutex<Option<(String, super::charm::Swing, std::time::Instant)>> = Mutex::new(None);
    let mut last = LAST.lock().unwrap();
    let now = std::time::Instant::now();
    if last.as_ref().is_none_or(|(k, _, at)| k != id || now.duration_since(*at).as_secs_f32() > 0.5) {
        *last = Some((id.to_string(), Default::default(), now));
    }
    let (_, swing, at) = last.as_mut().unwrap();
    let dt = now.duration_since(*at).as_secs_f32();
    *at = now;
    swing.step(top, rest, side, [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], [0.0; 3], dt)
}

/// The finish's jobs for the preview — a weapon's one, a pair of gloves' one for each hand (made again only when
/// what it shows changes).
pub fn jobs(p: &Preview, m: &Model, key: &str) -> Vec<Arc<super::compose::Job>> {
    static LAST: Mutex<Option<(String, Vec<Arc<super::compose::Job>>)>> = Mutex::new(None);
    if p.paint.is_empty() {
        return Vec::new();
    }
    let want = format!("{key}|{}|{}|{}", p.paint, p.seed, p.wear);
    let mut last = LAST.lock().unwrap();
    if last.as_ref().map(|(k, _)| k) != Some(&want) {
        let gloves = super::catalog::get().item(&p.item).is_some_and(|i| i.kind == super::catalog::Kind::Gloves);
        let j: Vec<Arc<super::compose::Job>> = if gloves { super::glove::jobs(key, m, &p.paint, p.seed, p.wear) } else { super::view::finish_job_for(key, m, &p.paint, p.seed, p.wear).into_iter().collect() };
        // (asked again while what it is made of still comes from the server)
        if j.is_empty() && store::busy() {
            return j;
        }
        *last = Some((want, j));
    }
    last.as_ref().map(|(_, j)| j.clone()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_turned_item_is_framed_with_its_near_end_inside() {
        // a rod a metre long, turned toward the eye at one end (as CS2's camera sees a rifle from its muzzle's side)
        let (a, b) = ([-0.6, 0.0, -0.3], [0.6, 0.1, 0.3]);
        let t = Turned { lo: [-0.6, 0.0, -0.3], hi: [0.6, 0.1, 0.3], points: vec![a, b] };
        let p = Preview { rect: [0.0; 4], frame: [0.0; 4], item: String::new(), paint: String::new(), seed: 0, wear: 0.0, yaw: 0.0, pitch: 0.0, zoom: 1.0, stattrak: None, charm: None, charm_offset: [0.0; 3], stickers: Vec::new(), nametag: None, overlay: None, light: [0.0; 2], alpha: 1.0, in_hand: false, ss: 1 };
        let w = place(&t, &crate::cosmetics::model::IDENTITY, &p, 1.5, true);
        let tan = (FOV.to_radians() * 0.5).tan();
        let mut widest = 0f32;
        for q in [a, b] {
            let v: [f32; 3] = std::array::from_fn(|k| w[k] * q[0] + w[4 + k] * q[1] + w[8 + k] * q[2] + w[12 + k]);
            let across = v[0].abs() / -v[2] / (tan * 1.5);
            assert!(across <= 0.8 + 1e-4, "{across}");
            widest = widest.max(across);
        }
        // the nearer end fills its share
        assert!(widest > 0.79, "{widest}");
    }

    #[test]
    #[ignore]
    fn probe_a_ray_comes_back_to_its_point() {
        store::set_dir(r"D:\CS2\work\out");
        let p = Preview { rect: [624.0, 353.0, 1252.0, 893.0], frame: [624.0, 353.0, 1252.0, 893.0], item: "weapon_awp".into(), paint: "cu_awp_asimov".into(), seed: 0, wear: 0.0, yaw: 0.0, pitch: 0.0, zoom: 1.0, stattrak: None, charm: None, charm_offset: [0.0; 3], stickers: Vec::new(), nametag: None, overlay: None, light: [0.0; 2], alpha: 1.0, in_hand: false, ss: 1 };
        let key = model_key(&p.item, &p.paint);
        store::want(&key);
        let mut m = None;
        for _ in 0..200 {
            m = store::model(&key);
            if m.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let m = m.expect("the model read");
        let (w, h) = (p.rect[2] - p.rect[0], p.rect[3] - p.rect[1]);
        let _ = (w, h);
        // a point of the receiver's side, to the screen as the look draws it, and back
        let q = [0.0196f32, 0.198, 0.25];
        let px = project(&p, q).unwrap();
        let (o, d) = ray(&p, px).unwrap();
        // the nearest the ray comes to the point
        let oq = [q[0] - o[0], q[1] - o[1], q[2] - o[2]];
        let along = oq[0] * d[0] + oq[1] * d[1] + oq[2] * d[2];
        let miss = (0..3).map(|k| (oq[k] - d[k] * along).powi(2)).sum::<f32>().sqrt();
        println!("px {px:?} ray {o:?} {d:?} miss {miss}");
        let on: Vec<u32> = (0..m.materials.len() as u32).collect();
        println!("hit {:?}", super::super::sticker::raycast(&m, &on, o, d));
        assert!(miss < 1e-3, "{miss}");
    }

    #[test]
    fn laying_an_item_down_turns_it_and_never_mirrors_it() {
        let perms = [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]];
        for ax in perms {
            let r = lay(ax);
            // its longest side across, the next up, the thinnest toward the eye (each ±)
            for (to, &from) in ax.iter().enumerate() {
                assert_eq!(r[from * 4 + to].abs(), 1.0, "{ax:?}");
            }
            let m = |c: usize, row: usize| r[c * 4 + row];
            let det = m(0, 0) * (m(1, 1) * m(2, 2) - m(2, 1) * m(1, 2)) - m(1, 0) * (m(0, 1) * m(2, 2) - m(2, 1) * m(0, 2))
                + m(2, 0) * (m(0, 1) * m(1, 2) - m(1, 1) * m(0, 2));
            assert_eq!(det, 1.0, "{ax:?} mirrors");
        }
    }
}
