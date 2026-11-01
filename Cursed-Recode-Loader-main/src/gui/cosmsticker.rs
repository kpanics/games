//! The Cosmetics page's stickers (CS2's, on the AWP and the SSG 08 — and, ours only, on a knife): in the look, the row
//! of the item's stickers, and — pressed — its five places in the panel in place of the rest, the chosen one's wear,
//! its turn and its way off, and every sticker CS2 has by its own picture, searched by name. A sticker is carried from
//! the panel onto the item (shown where it would lie as it is carried; let go off the item, nothing changes) or
//! pressed on in its place; on the item it is taken where it is and moved — a rifle's within where CS2 lets it lie,
//! pressed on from its side; a knife's anywhere, as the knife faces there — turned by the wheel over it, taken off by
//! the right button.

use super::draw::{grey, hex, mix, Canvas};
use super::i18n::{ru, tr};
use super::text::{fonts, Face};
use super::theme::pal;
use super::{ak, Gui, Hit, A_HOV};
use crate::cosmetics::catalog::{self, Kind};
use crate::cosmetics::loadout::{self, Slot};
use crate::cosmetics::sticker::{self, Placed};
use crate::cosmetics::{preview, store};

/// The parts (Hit::Cosm).
pub(super) const STICKER_ROW: u8 = 23;
pub(super) const STICKER_SLOT: u8 = 24;
pub(super) const STICKER_PICK: u8 = 25;
pub(super) const STICKER_SEARCH: u8 = 26;
pub(super) const STICKER_REMOVE: u8 = 27;
pub(super) const STICKER_WEAR: u8 = 28;
pub(super) const STICKER_TURN: u8 = 29;
pub(super) const STICKER_OPPOSITE: u8 = 60;
/// What is held (cosm.drag): the chosen sticker's wear slider, its turn's, the sticker itself on the item.
pub(super) const DRAG_WEAR: u8 = 4;
pub(super) const DRAG_TURN: u8 = 5;
pub(super) const DRAG_MOVE: u8 = 6;
/// A sticker carried from the panel onto the item.
pub(super) const DRAG_CARRY: u8 = 30;

/// A sticker carried from the panel (its name): where the press began, whether it has left there, whether it is on
/// the item now (shown there in its place `slot`), what was in that place before (put back when it is let go off the
/// item), how it comes (in that place, or where CS2 lays a new one).
#[derive(Clone, Debug)]
pub struct Carry {
    pub name: String,
    pub from: [f32; 2],
    pub moved: bool,
    pub on_item: bool,
    pub slot: usize,
    pub backup: Option<Placed>,
    pub base: Option<Placed>,
}

/// A knife's sticker's width (metres: CS2's on a rifle are some 7 cm; a blade takes less).
const KNIFE_SIZE: f32 = 0.032;

fn name_of(s: &catalog::Sticker) -> &str {
    if ru() {
        &s.ru
    } else {
        &s.en
    }
}

fn unit(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-9);
    v.map(|x| x / l)
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// `v` along the surface of normal `n` (none when it stands straight out of it).
fn along(v: [f32; 3], n: [f32; 3]) -> Option<[f32; 3]> {
    let d = dot(v, n);
    let w = [v[0] - n[0] * d, v[1] - n[1] * d, v[2] - n[2] * d];
    (dot(w, w) > 1e-8).then(|| unit(w))
}

/// The up a sticker's turn is counted from: the model's up along its surface (its along-the-barrel where the surface
/// faces up or down).
fn reference_up(n: [f32; 3]) -> [f32; 3] {
    along([0.0, 1.0, 0.0], n).or_else(|| along([0.0, 0.0, 1.0], n)).unwrap_or([0.0, 1.0, 0.0])
}

/// A sticker's turn (degrees, −180..180) from its up (a hair from none: none).
fn turn_of(p: &Placed) -> f32 {
    let n = unit(p.normal);
    let r = reference_up(n);
    let deg = dot(cross(r, p.up), n).atan2(dot(r, p.up)).to_degrees();
    if deg.abs() < 0.05 {
        0.0
    } else {
        deg
    }
}

impl Gui {
    /// The stickers whose name has the search's words (CS2's order), kept for the search.
    fn sticker_found(&mut self) -> Vec<usize> {
        let q = self.cosm.sticker_q.trim().to_lowercase();
        if self.cosm.sticker_found.0 != q || self.cosm.sticker_found.1.is_empty() {
            let cat = catalog::get();
            let words: Vec<&str> = q.split_whitespace().collect();
            let found = cat
                .stickers
                .iter()
                .enumerate()
                .filter(|(_, s)| {
                    let hay = format!("{} {} {}", s.ru, s.en, s.name).to_lowercase();
                    words.iter().all(|w| hay.contains(w))
                })
                .map(|(i, _)| i)
                .collect();
            self.cosm.sticker_found = (q, found);
        }
        self.cosm.sticker_found.1.clone()
    }

    /// A sticker's own picture, small (its look's g_tSticker0, a mip of it), asked for when it is not there yet.
    fn sticker_thumb(&mut self, name: &str) -> Option<([f32; 4], [u32; 2], f32)> {
        let cat = catalog::get();
        let material = cat.stickers.iter().find(|s| s.name == name)?.material.clone();
        let file = sticker::look(&material)?.textures.get("g_tSticker0")?.clone();
        self.cosm_pic(&file, 128)
    }

    /// The row of the look's stickers: how many of the five, the way to them.
    pub(super) fn sticker_row(&mut self, c: &mut Canvas, b: [f32; 4], count: usize) {
        let r = self.r;
        let hit = Hit::Cosm(STICKER_ROW, 0);
        let hv = self.an(ak(A_HOV, 34_500), if self.hover == hit { 1.0 } else { 0.0 }, 14.0, 0.0);
        c.rect(b, 0.6 * r, mix(pal().f1, pal().f2, hv));
        let mid = (b[1] + b[3]) * 0.5 + 0.28 * r;
        c.text(b[0] + 0.7 * r, mid, 0.72 * r, Face::Medium, tr("Stickers"), pal().t3);
        c.text_r(
            b[2] - 1.4 * r,
            mid,
            0.72 * r,
            Face::Semi,
            &format!("{count} / {}", sticker::MAX),
            mix(pal().t2, pal().t1, hv),
        );
        let m = [b[2] - 0.75 * r, (b[1] + b[3]) * 0.5];
        let pts = if self.cosm.picking_sticker {
            [[m[0] - 0.35 * r, m[1] - 0.15 * r], [m[0], m[1] + 0.2 * r], [m[0] + 0.35 * r, m[1] - 0.15 * r]]
        } else {
            [[m[0] - 0.2 * r, m[1] - 0.35 * r], [m[0] + 0.15 * r, m[1]], [m[0] - 0.2 * r, m[1] + 0.35 * r]]
        };
        c.polyline(&pts, 1.5, mix(pal().t3, pal().t1, hv));
        self.hits.push((b, hit));
    }

    /// The stickers in the panel `pr` (in place of the look's settings): the five places, the chosen one's wear and
    /// turn and its way off, the search, every sticker by its picture.
    pub(super) fn sticker_panel(&mut self, c: &mut Canvas, pr: [f32; 4], placed: &[Placed]) {
        let r = self.r;
        let (x0, x1) = (pr[0] + 0.9 * r, pr[2] - 0.9 * r);
        let mut y = pr[1];
        // the five places
        let gap = 0.4 * r;
        let side = (x1 - x0 - gap * (sticker::MAX as f32 - 1.0)) / sticker::MAX as f32;
        for i in 0..sticker::MAX {
            let b = [x0 + i as f32 * (side + gap), y, x0 + i as f32 * (side + gap) + side, y + side];
            let hit = Hit::Cosm(STICKER_SLOT, i as u16);
            let chosen = self.cosm.sticker_sel == i;
            let hv = self.an(
                ak(A_HOV, 34_600 + i),
                if chosen {
                    1.0
                } else if self.hover == hit {
                    0.5
                } else {
                    0.0
                },
                14.0,
                0.0,
            );
            c.rect(b, 0.5 * r, mix(pal().f1, pal().f3, hv));
            match placed.get(i) {
                Some(p) => {
                    if let Some((uv, _, seen)) = self.sticker_thumb(&p.name) {
                        let k = 0.12 * side;
                        c.pic([b[0] + k, b[1] + k, b[2] - k, b[3] - k], uv, seen, false, 1.0, 1.0, 0.0);
                    }
                }
                None => {
                    let m = [(b[0] + b[2]) * 0.5, (b[1] + b[3]) * 0.5];
                    let a = 0.22 * side;
                    let col = mix(pal().t4, pal().t2, hv);
                    c.polyline(&[[m[0] - a, m[1]], [m[0] + a, m[1]]], 1.5, col);
                    c.polyline(&[[m[0], m[1] - a], [m[0], m[1] + a]], 1.5, col);
                }
            }
            if chosen {
                c.rect([b[0] + 0.3 * r, b[3] - 0.16 * r, b[2] - 0.3 * r, b[3] - 0.06 * r], 0.05 * r, grey(1.0, 0.85));
            }
            self.hits.push((b, hit));
        }
        y += side + 0.9 * r;
        // the chosen one: its name, its wear, its turn, off
        if let Some(p) = placed.get(self.cosm.sticker_sel).cloned() {
            let cat = catalog::get();
            let name = cat
                .stickers
                .iter()
                .find(|s| s.name == p.name)
                .map_or(p.name.clone(), |s| name_of(s).to_string());
            let off = Hit::Cosm(STICKER_REMOVE, 0);
            let hv = self.an(ak(A_HOV, 34_700), if self.hover == off { 1.0 } else { 0.0 }, 14.0, 0.0);
            let label = tr("Take off");
            let lw = fonts().width(Face::Medium, 0.66 * r, label) + 1.2 * r;
            let ob = [x1 - lw, y - 0.95 * r, x1, y + 0.45 * r];
            c.rect(ob, 0.5 * r, mix(pal().f1, pal().f3, hv));
            c.text_c((ob[0] + ob[2]) * 0.5, y + 0.02 * r, 0.66 * r, Face::Medium, label, mix(pal().t2, pal().t1, hv));
            self.hits.push((ob, off));
            c.text(
                x0,
                y,
                0.78 * r,
                Face::Semi,
                &fonts().fit(Face::Semi, 0.78 * r, &name, ob[0] - x0 - 0.5 * r),
                pal().t1,
            );
            y += 1.3 * r;
            for (k, (label, value, t, part, anim)) in [
                (tr("Wear"), format!("{:.2}", p.wear), p.wear, STICKER_WEAR, 34_710),
                (
                    tr("Rotation"),
                    format!("{:.0}°", turn_of(&p)),
                    (turn_of(&p) + 180.0) / 360.0,
                    STICKER_TURN,
                    34_720,
                ),
            ]
            .into_iter()
            .enumerate()
            {
                c.text(x0, y, 0.68 * r, Face::Medium, label, pal().t3);
                c.text_r(x1, y, 0.68 * r, Face::Mono, &value, pal().t2);
                y += 0.55 * r;
                self.cosm.sticker_tracks[k] = [x0, x1];
                self.cosm_slider(c, [x0, y, x1, y + 1.4 * r], t, Hit::Cosm(part, 0), anim);
                y += 2.0 * r;
            }
            let available = placed.len() < sticker::MAX;
            let hit = Hit::Cosm(STICKER_OPPOSITE, 0);
            let hv = self.an(ak(A_HOV, 34_725), if available && self.hover == hit { 1.0 } else { 0.0 }, 14.0, 0.0);
            let b = [x0, y, x1, y + 1.8 * r];
            c.rect(b, 0.5 * r, mix(pal().f1, pal().f3, hv));
            let label = tr(if available { "Copy to other side" } else { "No free sticker slots" });
            let label = fonts().fit(Face::Medium, 0.68 * r, label, x1 - x0 - 1.0 * r);
            c.text_c(
                (x0 + x1) * 0.5,
                y + 1.14 * r,
                0.68 * r,
                Face::Medium,
                &label,
                if available { mix(pal().t2, pal().t1, hv) } else { pal().t4 },
            );
            if available {
                self.hits.push((b, hit));
            }
            y += 2.4 * r;
        } else {
            c.text(
                x0,
                y,
                0.68 * r,
                Face::Regular,
                &fonts().fit(Face::Regular, 0.68 * r, tr("Pick a sticker"), x1 - x0),
                pal().t4,
            );
            y += 1.2 * r;
        }
        // the search
        let sb = [x0, y, x1, y + 1.8 * r];
        let hit = Hit::Cosm(STICKER_SEARCH, 0);
        let typing = self.cosm.sticker_typing;
        let hv = self.an(
            ak(A_HOV, 34_730),
            if typing {
                1.0
            } else if self.hover == hit {
                0.5
            } else {
                0.0
            },
            14.0,
            0.0,
        );
        c.rect(sb, 0.6 * r, mix(pal().f1, pal().f2, hv));
        let q = self.cosm.sticker_q.clone();
        let base = (sb[1] + sb[3]) * 0.5 + 0.26 * r;
        if q.is_empty() && !typing {
            c.text(sb[0] + 0.7 * r, base, 0.7 * r, Face::Regular, tr("Search"), pal().t4);
        } else {
            let shown = fonts().fit(Face::Regular, 0.7 * r, &q, sb[2] - sb[0] - 1.6 * r);
            let w = c.text(sb[0] + 0.7 * r, base, 0.7 * r, Face::Regular, &shown, pal().t1);
            if typing && (self.now * 2.0).fract() < 0.5 {
                c.rect([sb[0] + 0.75 * r + w, base - 0.62 * r, sb[0] + 0.82 * r + w, base + 0.12 * r], 0.0, pal().t2);
            }
        }
        self.hits.push((sb, hit));
        y = sb[3] + 0.5 * r;
        // every sticker found, by its picture
        let found = self.sticker_found();
        let cols = 4usize;
        let gap = 0.35 * r;
        let cw = (x1 - x0 - gap * (cols as f32 - 1.0)) / cols as f32;
        let ch = cw + 0.35 * r;
        let rows = found.len().div_ceil(cols);
        let view = [pr[0], y, pr[2], pr[3] - 0.5 * r];
        self.cosm.sticker_area = view;
        self.cosm.sticker_content = (rows as f32 * (ch + gap) - gap - (view[3] - view[1])).max(0.0);
        self.cosm.sticker_scroll_to = self.cosm.sticker_scroll_to.clamp(0.0, self.cosm.sticker_content);
        self.cosm.sticker_scroll += (self.cosm.sticker_scroll_to - self.cosm.sticker_scroll) * (1.0 - (-self.dt * 14.0).exp());
        if found.is_empty() {
            c.text_c((x0 + x1) * 0.5, y + 1.6 * r, 0.7 * r, Face::Regular, tr("Nothing found"), pal().t4);
            return;
        }
        let cat = catalog::get();
        c.push_clip(view);
        let first = ((self.cosm.sticker_scroll / (ch + gap)).floor().max(0.0) as usize) * cols;
        for (n, &si) in found.iter().enumerate().skip(first) {
            let (cx, cy) = (n % cols, n / cols);
            let b0 = [x0 + cx as f32 * (cw + gap), y + cy as f32 * (ch + gap) - self.cosm.sticker_scroll];
            if b0[1] > view[3] {
                break;
            }
            let b = [b0[0], b0[1], b0[0] + cw, b0[1] + ch];
            let Some(s) = cat.stickers.get(si) else { continue };
            let hit = Hit::Cosm(STICKER_PICK, n as u16);
            let hv = self.an(ak(A_HOV, 35_000 + n % 4096), if self.hover == hit { 1.0 } else { 0.0 }, 14.0, 0.0);
            c.rect(b, 0.5 * r, mix(pal().f1, pal().f2, hv));
            let thumb = self.sticker_thumb(&s.name);
            let seen = thumb.map_or(0.0, |t| t.2);
            self.cosm_waiting(c, [b[0] + 0.25 * cw, b[1] + 0.25 * cw, b[2] - 0.25 * cw, b[1] + 0.75 * cw], seen, n);
            if let Some((uv, _, seen)) = thumb {
                let k = 0.1 * cw * (1.0 - 0.3 * hv) + 0.03 * cw * (1.0 - seen);
                c.pic([b[0] + k, b[1] + k, b[2] - k, b[1] + cw - k], uv, seen, false, 1.0, 1.0, 0.0);
            }
            c.rect(
                [b[0] + 0.3 * r, b[3] - 0.24 * r, b[2] - 0.3 * r, b[3] - 0.14 * r],
                0.05 * r,
                hex(catalog::rarity_colour(Kind::Rifle, &s.rarity), 0.9),
            );
            self.hits.push(([b[0], b[1].max(view[1]), b[2], b[3].min(view[3])], hit));
        }
        c.pop_clip();
        // the name of the one under the pointer, over the panel's foot
        if let Some(Hit::Cosm(STICKER_PICK, n)) = Some(self.hover) {
            if let Some(s) = found.get(n as usize).and_then(|&si| cat.stickers.get(si)) {
                let t = fonts().fit(Face::Medium, 0.64 * r, name_of(s), x1 - x0 - 1.2 * r);
                let w = fonts().width(Face::Medium, 0.64 * r, &t) + 1.2 * r;
                let tb = [(x0 + x1 - w) * 0.5, view[3] - 1.6 * r, (x0 + x1 + w) * 0.5, view[3] - 0.3 * r];
                c.rect(tb, 0.6 * r, grey(0.06, 0.92));
                c.text_c((tb[0] + tb[2]) * 0.5, tb[3] - 0.42 * r, 0.64 * r, Face::Medium, &t, pal().t1);
            }
        }
    }

    /// Where a new sticker goes on the look's item (the place `i` of its five): a rifle's at CS2's own slots, pressed on
    /// from its side; a knife's where the look's middle meets it, side by side, as the knife faces there.
    fn sticker_new_place(&self, i: usize) -> Option<Placed> {
        let p = self.cosm.last_shown.clone()?;
        let key = preview::model_key(&p.item, &p.paint);
        let m = store::model(&key)?;
        let on: Vec<u32> = sticker::body_materials(&key, &m).into_iter().map(|(k, _)| k).collect();
        let kind = catalog::get().item(&p.item).map(|it| it.kind)?;
        if kind == Kind::Rifle {
            let legacy = key.ends_with("legacy.rcm");
            let mk = sticker::markup(key.rsplit_once('/')?.0);
            let slots: Vec<sticker::Slot> = mk.slots_of(legacy).cloned().collect();
            let slot = slots.get(i % slots.len().max(1))?;
            let mut placed = sticker::slot_place(&m, &on, &mk, slot)?;
            if i >= slots.len() {
                // more than its slots: beside the first, toward the butt
                let z = placed.at[2] - placed.size * 1.1;
                let x = m.bounds().1[0] + 0.1;
                placed.at = sticker::raycast(&m, &on, [x, placed.at[1], z], [-1.0, 0.0, 0.0])?.1;
            }
            return Some(placed);
        }
        // a knife: where it shows nearest the look's middle (a karambit's curve leaves the middle empty), the next
        // ones further along
        let w = p.rect[2] - p.rect[0];
        let start = [(p.rect[0] + p.rect[2]) * 0.5 + (i as f32 - 2.0) * w * 0.06, (p.rect[1] + p.rect[3]) * 0.5];
        for ring in 0..24 {
            let rad = ring as f32 * w * 0.02;
            let steps = if ring == 0 { 1 } else { 12 };
            for k in 0..steps {
                let a = k as f32 / steps as f32 * std::f32::consts::TAU;
                let at = [start[0] + rad * a.cos(), start[1] + rad * a.sin()];
                let Some((o, d)) = preview::ray(&p, at) else { continue };
                if let Some((_, hit, n)) = sticker::raycast(&m, &on, o, d) {
                    return Some(Placed {
                        name: String::new(),
                        at: hit,
                        normal: n,
                        up: reference_up(n),
                        size: KNIFE_SIZE,
                        wear: 0.0,
                    });
                }
            }
        }
        None
    }

    /// The look's stickers changed: the item on (the same, its finish the same) takes them at once.
    fn stickers_changed(&mut self) {
        let Some(l) = self.cosm.look.as_ref() else { return };
        let cat = catalog::get();
        let kind = cat.item(&l.item).map_or(Kind::Knife, |i| i.kind);
        let (item, paint, stickers) = (l.item.clone(), l.paint.clone(), l.stickers.clone());
        loadout::change(|lo| {
            if let Some(e) = lo.get_mut(Slot::of(&item, kind)).filter(|e| e.item == item && e.paint == paint) {
                e.stickers = stickers.clone();
            }
        });
    }

    /// A press on the stickers' parts.
    fn sticker_copy_opposite(&mut self) {
        let Some(l) = self.cosm.look.as_ref() else { return };
        if l.stickers.len() >= sticker::MAX {
            self.toast(tr("No free sticker slots").into());
            return;
        }
        let Some(original) = l.stickers.get(self.cosm.sticker_sel) else { return };
        let key = preview::model_key(&l.item, &l.paint);
        let Some(m) = store::model(&key) else {
            self.toast(tr("Wait for the model to load").into());
            return;
        };
        let on: Vec<u32> = sticker::body_materials(&key, &m).into_iter().map(|(k, _)| k).collect();
        let rifle = catalog::get().item(&l.item).is_some_and(|i| i.kind == Kind::Rifle);
        let Some(copy) = original.opposite(&m, &on, rifle) else {
            self.toast(tr("No surface on the other side").into());
            return;
        };
        let l = self.cosm.look.as_mut().unwrap();
        self.cosm.sticker_sel = l.stickers.len();
        l.stickers.push(copy);
        self.stickers_changed();
        self.sound_tick(0.5);
        self.toast(tr("Sticker copied to the other side").into());
    }

    pub(super) fn sticker_press(&mut self, part: u8, n: u16) {
        match part {
            STICKER_OPPOSITE => self.sticker_copy_opposite(),
            STICKER_ROW => {
                self.cosm.picking_sticker = !self.cosm.picking_sticker;
                self.cosm.picking_look = false;
                self.cosm.picking_charm = false;
                self.cosm.sticker_typing = false;
                self.sound_tick(0.6);
            }
            STICKER_SLOT => {
                self.cosm.sticker_sel = (n as usize).min(sticker::MAX - 1);
                self.sound_tick(0.5);
            }
            STICKER_SEARCH => {
                self.cosm.sticker_typing = true;
                self.sound_tick(0.4);
            }
            STICKER_REMOVE => {
                let sel = self.cosm.sticker_sel;
                if let Some(l) = self.cosm.look.as_mut() {
                    if sel < l.stickers.len() {
                        l.stickers.remove(sel);
                    }
                }
                self.stickers_changed();
                self.switch_sound(false, 0.5);
            }
            STICKER_WEAR | STICKER_TURN => {
                self.cosm.drag = Some((if part == STICKER_WEAR { DRAG_WEAR } else { DRAG_TURN }, self.mouse));
                self.sticker_move(self.mouse[0], self.mouse[1]);
            }
            STICKER_PICK => {
                let found = self.sticker_found();
                let cat = catalog::get();
                let Some(name) = found.get(n as usize).and_then(|&si| cat.stickers.get(si)).map(|s| s.name.clone()) else {
                    return;
                };
                let have = self.cosm.look.as_ref().map_or(0, |l| l.stickers.len());
                let sel = self.cosm.sticker_sel.min(have);
                // into the chosen place (what is there as it was, its name the new one's), else a new one as CS2 lays it
                let (slot, backup) = if sel < have {
                    (sel, self.cosm.look.as_ref().map(|l| l.stickers[sel].clone()))
                } else {
                    (have, None)
                };
                let base = match &backup {
                    Some(b) => Some(b.clone()),
                    None => self.sticker_new_place(have).or_else(|| self.sticker_new_place(0)),
                }
                .map(|p| Placed { name: name.clone(), ..p });
                self.cosm.carry = Some(Carry {
                    name,
                    from: self.mouse,
                    moved: false,
                    on_item: false,
                    slot,
                    backup,
                    base,
                });
                self.cosm.drag = Some((DRAG_CARRY, self.mouse));
            }
            _ => {}
        }
    }

    /// The pointer pressed on the item in the look while its stickers are shown: the sticker under it chosen and held
    /// to move (as far from the pointer as it was taken); true when there was one.
    pub(super) fn sticker_grab(&mut self) -> bool {
        if !self.cosm.picking_sticker {
            return false;
        }
        let Some(i) = self.sticker_under(self.mouse) else { return false };
        let at = self.sticker_screen(i).unwrap_or(self.mouse);
        self.cosm.sticker_sel = i;
        self.cosm.grab_off = [self.mouse[0] - at[0], self.mouse[1] - at[1]];
        self.cosm.drag = Some((DRAG_MOVE, self.mouse));
        self.sound_tick(0.35);
        true
    }

    /// The look's sticker under the pointer (the menu's own coordinates) on the item, the nearest of them: its place.
    pub(super) fn sticker_under(&self, mouse: [f32; 2]) -> Option<usize> {
        let l = self.cosm.look.as_ref()?;
        let p = self.cosm.last_shown.as_ref()?;
        let [mx, my, sc, drop, _] = self.xf;
        let sc = sc.max(1e-3);
        let back = |q: [f32; 2]| [mx + (q[0] - mx) / sc, my + (q[1] - drop - my) / sc];
        let mut best: Option<(usize, f32)> = None;
        for (i, s) in l.stickers.iter().enumerate() {
            let Some(c0) = preview::project(p, s.at) else { continue };
            let Some((_, dir)) = preview::ray(p, c0) else { continue };
            if !s.faces_ray(dir) {
                continue;
            }
            let up = unit(s.up);
            let edge = std::array::from_fn(|k| s.at[k] + up[k] * s.size * 0.5);
            let rad = preview::project(p, edge).map_or(0.0, |e| ((e[0] - c0[0]).powi(2) + (e[1] - c0[1]).powi(2)).sqrt()) / sc;
            let c0 = back(c0);
            let d = ((mouse[0] - c0[0]).powi(2) + (mouse[1] - c0[1]).powi(2)).sqrt();
            if d <= rad.max(1.1 * self.r) && best.is_none_or(|(_, b)| d < b) {
                best = Some((i, d));
            }
        }
        best.map(|b| b.0)
    }

    /// The wheel over a sticker on the item: it turns (15 degrees a notch) and is chosen; true when there was one.
    pub(super) fn sticker_spin(&mut self, dy: f32) -> bool {
        if !self.cosm.picking_sticker || self.cosm.in_hand {
            return false;
        }
        let Some(i) = self.sticker_under(self.mouse) else { return false };
        if let Some(p) = self.cosm.look.as_mut().and_then(|l| l.stickers.get_mut(i)) {
            let deg = turn_of(p) - dy.signum() * 15.0;
            let deg = (deg + 180.0).rem_euclid(360.0) - 180.0;
            let n = unit(p.normal);
            p.up = sticker::turned(reference_up(n), n, (deg / 5.0).round() * 5.0);
        }
        self.cosm.sticker_sel = i;
        self.stickers_changed();
        self.sound_tick(0.25);
        true
    }

    /// The right button on a sticker on the item: it comes off; true when there was one.
    pub(super) fn sticker_take_off_at(&mut self) -> bool {
        if !self.cosm.picking_sticker || self.cosm.in_hand {
            return false;
        }
        let Some(i) = self.sticker_under(self.mouse) else { return false };
        if let Some(l) = self.cosm.look.as_mut() {
            l.stickers.remove(i);
            self.cosm.sticker_sel = self.cosm.sticker_sel.min(l.stickers.len());
        }
        self.stickers_changed();
        self.switch_sound(false, 0.5);
        true
    }

    /// The look's stickers as before the carry: its place back to what was there (or the new one gone).
    fn carry_restore(&mut self, k: &Carry) {
        let Some(l) = self.cosm.look.as_mut() else { return };
        match &k.backup {
            Some(b) if k.slot < l.stickers.len() => l.stickers[k.slot] = b.clone(),
            None if k.slot < l.stickers.len() => l.stickers.truncate(k.slot),
            _ => {}
        }
    }

    /// The carried sticker follows the pointer: where it meets the item it is shown lying there, else as it was.
    pub(super) fn sticker_carry_move(&mut self, x: f32, y: f32) {
        let Some(mut k) = self.cosm.carry.take() else { return };
        k.moved |= (x - k.from[0]).abs() + (y - k.from[1]).abs() > 6.0;
        if k.moved {
            self.carry_restore(&k);
            k.on_item = false;
            if let Some(base) = k.base.clone() {
                if let Some(l) = self.cosm.look.as_mut() {
                    if k.slot < l.stickers.len() {
                        l.stickers[k.slot] = base;
                    } else {
                        l.stickers.push(base);
                    }
                }
                self.cosm.sticker_sel = k.slot;
                k.on_item = self.sticker_place_at([x, y]);
                if !k.on_item {
                    self.carry_restore(&k);
                }
            }
        }
        self.cosm.carry = Some(k);
    }

    /// The carried sticker let go: on the item, it stays there; pressed only, it goes on in its place (a new one where
    /// CS2 lays it); let go off the item, nothing changes.
    pub(super) fn sticker_carry_drop(&mut self) {
        let Some(k) = self.cosm.carry.take() else { return };
        if !k.moved {
            self.carry_restore(&k);
            let Some(base) = k.base else { return };
            if let Some(l) = self.cosm.look.as_mut() {
                if k.slot < l.stickers.len() {
                    l.stickers[k.slot].name = k.name.clone();
                } else if l.stickers.len() < sticker::MAX {
                    l.stickers.push(base);
                }
            }
        } else if !k.on_item {
            self.carry_restore(&k);
            self.sound_tick(0.3);
            return;
        }
        self.cosm.sticker_sel = k.slot;
        self.cosm.sticker_typing = false;
        self.stickers_changed();
        self.switch_sound(true, if k.moved { 0.6 } else { 0.5 });
    }

    /// What is carried from the panel (a sticker, a charm), under the pointer: its picture, fading as it lies on the
    /// item (it shows there itself).
    pub(super) fn carry_ghost(&mut self, c: &mut Canvas) {
        let r = self.r;
        // (which, as it is now: its name, a charm's or not, on the item)
        let what = match (&self.cosm.carry, &self.cosm.carry_charm) {
            (Some(k), _) if k.moved => (k.name.clone(), false, k.on_item),
            (_, Some(k)) if k.moved => match &k.name {
                Some(name) => (name.clone(), true, k.on_item),
                None => return,
            },
            _ => return,
        };
        let (name, charm, on) = what;
        let pic = if charm { self.cosm_icon("charms", &name) } else { self.sticker_thumb(&name) };
        let a = self.an(ak(A_HOV, 34_900), if on { 0.22 } else { 1.0 }, 16.0, 1.0);
        let Some((uv, [iw, ih], seen)) = pic else { return };
        let side = 3.2 * r;
        let k = (side / iw.max(1) as f32).min(side / ih.max(1) as f32);
        let (w, h) = (iw as f32 * k, ih as f32 * k);
        let m = [self.mouse[0], self.mouse[1] + 0.2 * r];
        c.shadow(
            [m[0] - w * 0.4, m[1] - h * 0.3, m[0] + w * 0.4, m[1] + h * 0.45],
            0.8 * r,
            1.2 * r,
            grey(0.0, 0.35 * a),
        );
        c.pic(
            [m[0] - w * 0.5, m[1] - h * 0.5, m[0] + w * 0.5, m[1] + h * 0.5],
            uv,
            seen * a,
            false,
            1.0,
            1.0,
            0.0,
        );
    }

    /// The look's stickers on the item while they are shown: a faint ring round the chosen one, a brighter one round the
    /// one under the pointer (it is taken, turned, taken off there).
    pub(super) fn sticker_marks(&mut self, c: &mut Canvas) {
        if !self.cosm.picking_sticker || self.cosm.in_hand || self.cosm.carry.as_ref().is_some_and(|k| k.moved) {
            return;
        }
        let Some(l) = self.cosm.look.as_ref() else { return };
        let Some(p) = self.cosm.last_shown.clone() else { return };
        let n = l.stickers.len();
        let under = if self.cosm.drag.is_none() { self.sticker_under(self.mouse) } else { None };
        let [mx, my, sc, drop, _] = self.xf;
        let sc = sc.max(1e-3);
        for i in 0..n {
            let Some(s) = self.cosm.look.as_ref().and_then(|l| l.stickers.get(i)).cloned() else {
                continue;
            };
            let Some(c0) = preview::project(&p, s.at) else { continue };
            let up = unit(s.up);
            let edge = std::array::from_fn(|k| s.at[k] + up[k] * s.size * 0.5);
            let rad = preview::project(&p, edge).map_or(0.0, |e| ((e[0] - c0[0]).powi(2) + (e[1] - c0[1]).powi(2)).sqrt()) / sc;
            let m = [mx + (c0[0] - mx) / sc, my + (c0[1] - drop - my) / sc];
            let want = if under == Some(i) {
                1.0
            } else if self.cosm.sticker_sel == i {
                0.35
            } else {
                0.0
            };
            let k = self.an(ak(A_HOV, 34_920 + i), want, 14.0, 0.0);
            if k > 0.01 {
                c.ring(m, rad.max(1.1 * self.r) * (1.1 + 0.08 * k), 1.4, grey(1.0, 0.75 * k));
            }
        }
    }

    /// The chosen sticker to where the pointer (the menu's) meets the item; false when it meets none of the body, or
    /// (a rifle's) falls where CS2 lets no sticker lie.
    fn sticker_place_at(&mut self, mouse: [f32; 2]) -> bool {
        let Some(p) = self.cosm.last_shown.clone() else { return false };
        let [mx, my, sc, drop, _] = self.xf;
        let at = [mx + (mouse[0] - mx) * sc, my + (mouse[1] - my) * sc + drop];
        let Some((o, d)) = preview::ray(&p, at) else { return false };
        let key = preview::model_key(&p.item, &p.paint);
        let Some(m) = store::model(&key) else { return false };
        let on: Vec<u32> = sticker::body_materials(&key, &m).into_iter().map(|(k, _)| k).collect();
        let Some((_, hit, n)) = sticker::raycast(&m, &on, o, d) else { return false };
        let rifle = catalog::get().item(&p.item).is_some_and(|i| i.kind == Kind::Rifle);
        let sel = self.cosm.sticker_sel;
        let Some(cur) = self.cosm.look.as_ref().and_then(|l| l.stickers.get(sel)).cloned() else {
            return false;
        };
        let moved = if rifle {
            // pressed on from the side, where CS2 lets it lie
            let legacy = key.ends_with("legacy.rcm");
            let mk = sticker::markup(key.rsplit_once('/').map_or("", |(d, _)| d));
            let ok = mk.to_markup(legacy, [hit[2], hit[1]]).is_none_or(|uv| mk.allowed(legacy, uv));
            let sign = if cur.normal[0] < 0.0 { -1.0 } else { 1.0 };
            let (lo, hi) = m.bounds();
            let x = if sign < 0.0 { lo[0] - 0.1 } else { hi[0] + 0.1 };
            let side = sticker::raycast(&m, &on, [x, hit[1], hit[2]], [-sign, 0.0, 0.0]).map(|h| h.1);
            match (ok, side) {
                (true, Some(at)) => Some(Placed { at, ..cur }),
                _ => None,
            }
        } else {
            // as the knife faces there, its turn kept
            let turn = turn_of(&cur);
            let up = sticker::turned(reference_up(n), n, turn);
            Some(Placed { at: hit, normal: n, up, ..cur })
        };
        let Some(moved) = moved else { return false };
        if let Some(l) = self.cosm.look.as_mut() {
            l.stickers[sel] = moved;
        }
        true
    }

    /// Where the look's sticker `i` shows (the menu's own coordinates, the window's scaling and lift taken out):
    /// development (dev.rs "cosmst").
    pub(super) fn sticker_screen(&self, i: usize) -> Option<[f32; 2]> {
        let p = self.cosm.last_shown.as_ref()?;
        let q = self.cosm.look.as_ref()?.stickers.get(i)?.at;
        let [px, py] = preview::project(p, q)?;
        let [mx, my, sc, drop, _] = self.xf;
        Some([mx + (px - mx) / sc.max(1e-3), my + (py - drop - my) / sc.max(1e-3)])
    }

    /// What is held of the stickers follows the pointer: a slider, or the sticker on the item.
    pub(super) fn sticker_move(&mut self, x: f32, y: f32) {
        let Some((what, _)) = self.cosm.drag else { return };
        let sel = self.cosm.sticker_sel;
        match what {
            DRAG_WEAR | DRAG_TURN => {
                let [a, b] = self.cosm.sticker_tracks[(what == DRAG_TURN) as usize];
                let t = ((x - a) / (b - a).max(1.0)).clamp(0.0, 1.0);
                if let Some(p) = self.cosm.look.as_mut().and_then(|l| l.stickers.get_mut(sel)) {
                    if what == DRAG_WEAR {
                        p.wear = (t * 100.0).round() / 100.0;
                    } else {
                        // in steps of 5 degrees (0 as laid)
                        let deg = ((t * 360.0 - 180.0) / 5.0).round() * 5.0;
                        let n = unit(p.normal);
                        p.up = sticker::turned(reference_up(n), n, deg);
                    }
                }
            }
            DRAG_MOVE => {
                let off = self.cosm.grab_off;
                self.sticker_place_at([x - off[0], y - off[1]]);
            }
            _ => return,
        }
        self.stickers_changed();
    }

    /// A key while the stickers' search is typed into: true when it was the search's.
    pub(super) fn sticker_key(&mut self, escape: bool, enter: bool, backspace: bool) -> bool {
        if !self.cosm.sticker_typing {
            return false;
        }
        if backspace {
            self.cosm.sticker_q.pop();
            self.cosm.sticker_scroll_to = 0.0;
        }
        if escape || enter {
            self.cosm.sticker_typing = false;
        }
        true
    }

    /// A letter typed while the stickers' search is: true when it was the search's.
    pub(super) fn sticker_char(&mut self, ch: char) -> bool {
        if !self.cosm.sticker_typing {
            return false;
        }
        if self.cosm.sticker_q.chars().count() < 32 {
            self.cosm.sticker_q.push(ch);
            self.cosm.sticker_scroll_to = 0.0;
        }
        true
    }

    /// The wheel over the stickers found: true when it scrolled them.
    pub(super) fn sticker_wheel(&mut self, dy: f32) -> bool {
        let [x, y] = self.mouse;
        let a = self.cosm.sticker_area;
        if !(self.cosm.look.is_some() && self.cosm.picking_sticker && x >= a[0] && x <= a[2] && y >= a[1] && y <= a[3]) {
            return false;
        }
        self.cosm.sticker_scroll_to = (self.cosm.sticker_scroll_to - dy * 3.2 * self.r).clamp(0.0, self.cosm.sticker_content);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opposite_copy_controls_fit_and_never_replace_a_full_set() {
        const TEST: &str = "gui::cosmsticker::tests::opposite_copy_controls_fit_and_never_replace_a_full_set";
        if std::env::var_os("CURSED_STICKER_UI_CHILD").is_none() {
            let result = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
                .env("CURSED_STICKER_UI_CHILD", "1")
                .env_remove("CURSED_DEV_COSMETICS")
                .env_remove("CURSED_DEV_HOST")
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}{}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
            return;
        }
        let mut dynamic = Vec::new();
        for lang in ["ru_ru", "en_us", "uk_ua", "kk_kz"] {
            crate::menus::lang::set(lang);
            let letters: Vec<char> = "ӘәІіҢңҒғҮүҰұҚқӨөҺһ".chars().collect();
            for face in [Face::Regular, Face::Medium, Face::Semi] {
                for &ch in &letters {
                    fonts().glyph(face, ch);
                }
            }
            for _ in 0..400 {
                if ![Face::Regular, Face::Medium, Face::Semi]
                    .iter()
                    .any(|&face| letters.iter().any(|&ch| crate::gui::dynfont::waiting(face, ch)))
                {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            let p = Placed {
                name: "Crown (Foil)".into(),
                at: [0.02, 0.0, 0.0],
                normal: [1.0, 0.0, 0.0],
                up: [0.0, 1.0, 0.0],
                size: 0.04,
                wear: 0.25,
            };
            let mut g = Gui::new();
            g.r = 18.0;
            g.dt = 0.1;
            g.now = 10.0;
            g.cosm.look = Some(super::super::cosmui::Look {
                item: "test".into(),
                paint: String::new(),
                seed: 0,
                wear: 0.0,
                yaw: 0.0,
                pitch: 0.0,
                zoom: 1.0,
                at: 0.0,
                zoom_to: 1.0,
                aim: [0.0; 2],
                spin: [0.0; 2],
                was: [0.0; 2],
                vel: [0.0; 2],
                hand_turn: [0.0; 2],
                stattrak: false,
                charm: None,
                charm_offset: [0.0; 3],
                stickers: vec![p.clone()],
                nametag: None,
                overlay: None,
            });
            for full in [false, true] {
                let placed = vec![p.clone(); if full { sticker::MAX } else { 1 }];
                g.cosm.look.as_mut().unwrap().stickers = placed.clone();
                let mut c = Canvas::new(340.0, 440.0);
                g.hits.clear();
                g.sticker_panel(&mut c, [0.0, 16.0, 340.0, 424.0], &placed);
                let copy = g.hits.iter().find(|(_, h)| *h == Hit::Cosm(STICKER_OPPOSITE, 0));
                assert_eq!(copy.is_some(), !full);
                if let Some((b, _)) = copy {
                    let search = g.hits.iter().find(|(_, h)| *h == Hit::Cosm(STICKER_SEARCH, 0)).unwrap().0;
                    assert!(b[0] >= 0.0 && b[2] <= 340.0 && b[3] < search[1]);
                    assert!(fonts().width(Face::Medium, 0.68 * g.r, tr("Copy to other side")) <= b[2] - b[0] - g.r, "{lang}");
                }
                if full {
                    g.sticker_press(STICKER_OPPOSITE, 0);
                    assert_eq!(g.cosm.look.as_ref().unwrap().stickers, placed);
                }
                if let Some(path) = std::env::var_os("CURSED_TEST_CARD_DUMP") {
                    let path = std::path::PathBuf::from(path);
                    std::fs::create_dir_all(&path).unwrap();
                    dynamic.extend(crate::gui::dynfont::drain());
                    std::fs::write(
                        path.join(format!("stickers-{lang}-{full}.json")),
                        serde_json::to_vec(&serde_json::json!({"w":340,"h":440,"shapes":c.p,"dynamic":dynamic})).unwrap(),
                    )
                    .unwrap();
                }
            }
        }
    }

    #[test]
    fn a_turn_reads_back() {
        let n = [1.0, 0.0, 0.0];
        for deg in [-90.0f32, 0.0, 35.0, 170.0] {
            let p = Placed {
                name: String::new(),
                at: [0.0; 3],
                normal: n,
                up: sticker::turned(reference_up(n), n, deg),
                size: 0.05,
                wear: 0.0,
            };
            assert!((turn_of(&p) - deg).abs() < 1e-3, "{deg} {}", turn_of(&p));
        }
    }
}
