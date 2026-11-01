//! The Cosmetics page's charms (CS2's keychains, on the AWP and the SSG 08): in the look at a rifle, the row of the
//! charm on it, and — pressed — the charms in the panel in place of the rest: their icons (CS2's own), none first, a
//! press puts one on (on the rifle that is on at once; with the rest when it is put on); one carried onto the item
//! hangs where it is let go (shown there as it is carried; let go off the item, nothing changes), and the charm on
//! it is taken there and moved along it.

use super::draw::{hex, mix, Canvas};
use super::i18n::{ru, tr};
use super::text::{fonts, Face};
use super::theme::pal;
use super::{ak, Gui, Hit, A_HOV};
use crate::cosmetics::catalog::{self, Kind};
use crate::cosmetics::loadout::{self, Slot};
use crate::cosmetics::{charm, preview, sticker, store};

/// The parts (Hit::Cosm).
pub(super) const CHARM_ROW: u8 = 21;
pub(super) const CHARM_PICK: u8 = 22;
/// What is held (cosm.drag): the charm, moved along the item; a charm carried from the panel onto the item.
pub(super) const DRAG_CHARM: u8 = 7;
pub(super) const DRAG_CARRY_CHARM: u8 = 31;

/// A charm carried from the panel: which (its place in the panel, its name), where the press began, whether it has
/// left there, whether it hangs on the item now, the charm and its place before (put back when it is let go off it).
#[derive(Clone, Debug)]
pub struct CharmCarry {
    pub n: u16,
    pub name: Option<String>,
    pub from: [f32; 2],
    pub moved: bool,
    pub on_item: bool,
    pub backup: (Option<String>, [f32; 3]),
}

fn charm_name(c: &catalog::Charm) -> &str {
    if ru() {
        &c.ru
    } else {
        &c.en
    }
}

impl Gui {
    /// The row of the look's charm (a rifle's): its name, the way to the charms.
    pub(super) fn charm_row(&mut self, c: &mut Canvas, b: [f32; 4], charm: Option<&str>) {
        let r = self.r;
        let hit = Hit::Cosm(CHARM_ROW, 0);
        let hv = self.an(ak(A_HOV, 34_000), if self.hover == hit { 1.0 } else { 0.0 }, 14.0, 0.0);
        c.rect(b, 0.6 * r, mix(pal().f1, pal().f2, hv));
        let mid = (b[1] + b[3]) * 0.5 + 0.28 * r;
        c.text(b[0] + 0.7 * r, mid, 0.72 * r, Face::Medium, tr("Charm"), pal().t3);
        let cat = catalog::get();
        let name = charm.and_then(|n| cat.charms.iter().find(|x| x.name == n)).map_or(tr("None").to_string(), |x| charm_name(x).to_string());
        let w = b[2] - b[0] - 6.0 * r;
        c.text_r(b[2] - 1.4 * r, mid, 0.72 * r, Face::Semi, &fonts().fit(Face::Semi, 0.72 * r, &name, w), mix(pal().t2, pal().t1, hv));
        // its chevron: the way to the charms, down while they are shown
        let m = [b[2] - 0.75 * r, (b[1] + b[3]) * 0.5];
        let pts = if self.cosm.picking_charm {
            [[m[0] - 0.35 * r, m[1] - 0.15 * r], [m[0], m[1] + 0.2 * r], [m[0] + 0.35 * r, m[1] - 0.15 * r]]
        } else {
            [[m[0] - 0.2 * r, m[1] - 0.35 * r], [m[0] + 0.15 * r, m[1]], [m[0] - 0.2 * r, m[1] + 0.35 * r]]
        };
        c.polyline(&pts, 1.5, mix(pal().t3, pal().t1, hv));
        self.hits.push((b, hit));
    }

    /// The charms in the panel `pr` (in place of the look's settings): none first, then each by its icon.
    pub(super) fn charm_picker(&mut self, c: &mut Canvas, pr: [f32; 4], on: Option<&str>) {
        let r = self.r;
        let cat = catalog::get();
        let (x0, x1) = (pr[0] + 0.9 * r, pr[2] - 0.9 * r);
        let top = pr[1];
        let cols = 3usize;
        let gap = 0.45 * r;
        let cw = (x1 - x0 - gap * (cols as f32 - 1.0)) / cols as f32;
        let ch = cw * 0.75 + 1.6 * r;
        let count = cat.charms.len() + 1;
        let rows = count.div_ceil(cols);
        let view = [pr[0], top, pr[2], pr[3] - 0.6 * r];
        self.cosm.charm_area = view;
        self.cosm.charm_content = (rows as f32 * (ch + gap) - gap - (view[3] - view[1])).max(0.0);
        self.cosm.charm_scroll_to = self.cosm.charm_scroll_to.clamp(0.0, self.cosm.charm_content);
        self.cosm.charm_scroll += (self.cosm.charm_scroll_to - self.cosm.charm_scroll) * (1.0 - (-self.dt * 14.0).exp());
        c.push_clip(view);
        for n in 0..count {
            let (cx, cy) = (n % cols, n / cols);
            let b0 = [x0 + cx as f32 * (cw + gap), top + cy as f32 * (ch + gap) - self.cosm.charm_scroll];
            let b = [b0[0], b0[1], b0[0] + cw, b0[1] + ch];
            if b[3] < view[1] || b[1] > view[3] {
                continue;
            }
            let charm = if n == 0 { None } else { cat.charms.get(n - 1) };
            let chosen = charm.map(|x| x.name.as_str()) == on;
            let hit = Hit::Cosm(CHARM_PICK, n as u16);
            let hv = self.an(ak(A_HOV, 34_100 + n), if chosen { 1.0 } else if self.hover == hit { 0.5 } else { 0.0 }, 14.0, 0.0);
            c.rect(b, 0.6 * r, mix(pal().f1, pal().f3, hv));
            let ir = [b[0] + 0.3 * r, b[1] + 0.2 * r, b[2] - 0.3 * r, b[1] + 0.2 * r + cw * 0.75 - 0.6 * r];
            match charm {
                Some(x) => {
                    let icon = self.cosm_icon("charms", &x.name);
                    self.cosm_waiting(c, [ir[0] + 0.6 * r, ir[1] + 0.4 * r, ir[2] - 0.6 * r, ir[3] - 0.4 * r], icon.map_or(0.0, |i| i.2), n);
                    if let Some((uv, [iw, ih], seen)) = icon {
                        let k = ((ir[2] - ir[0]) / iw as f32).min((ir[3] - ir[1]) / ih as f32) * (0.96 + 0.04 * seen);
                        let (w, h) = (iw as f32 * k, ih as f32 * k);
                        let m = [(ir[0] + ir[2]) / 2.0, (ir[1] + ir[3]) / 2.0];
                        c.pic([m[0] - w / 2.0, m[1] - h / 2.0, m[0] + w / 2.0, m[1] + h / 2.0], uv, seen, false, 1.0, 1.0, 0.0);
                    }
                    let name = fonts().fit(Face::Medium, 0.56 * r, charm_name(x), cw - 0.6 * r);
                    c.text(b[0] + 0.3 * r, b[3] - 0.55 * r, 0.56 * r, Face::Medium, &name, mix(pal().t3, pal().t1, hv));
                    c.rect([b[0] + 0.3 * r, b[3] - 0.24 * r, b[2] - 0.3 * r, b[3] - 0.14 * r], 0.05 * r, hex(catalog::rarity_colour(Kind::Rifle, &x.rarity), 0.9));
                }
                None => {
                    c.text_c((b[0] + b[2]) * 0.5, (b[1] + b[3]) * 0.5 + 0.25 * r, 0.7 * r, Face::Medium, tr("None"), mix(pal().t3, pal().t1, hv));
                }
            }
            self.hits.push(([b[0], b[1].max(view[1]), b[2], b[3].min(view[3])], hit));
        }
        c.pop_clip();
    }

    /// The charm on the look's item applied to the item on (the same, its finish the same) at once.
    fn charm_changed(&mut self) {
        let Some(l) = self.cosm.look.as_ref() else { return };
        let (item, paint, charm) = (l.item.clone(), l.paint.clone(), l.charm.clone());
        let offset = (l.charm_offset != [0.0; 3]).then_some(l.charm_offset);
        loadout::change(|lo| {
            if let Some(e) = lo.get_mut(Slot::of(&item, Kind::Rifle)).filter(|e| e.item == item && e.paint == paint) {
                e.charm = charm.clone();
                e.charm_offset = offset.filter(|_| charm.is_some());
            }
        });
    }

    /// The pointer pressed on the item in the look while its charms are shown and one is on: pressed on the charm (by
    /// where it hangs from), it is taken there and held to move; true when it was.
    pub(super) fn charm_grab(&mut self) -> bool {
        let on = self.cosm.picking_charm && self.cosm.look.as_ref().is_some_and(|l| l.charm.is_some());
        let Some(at) = self.charm_screen().filter(|_| on) else { return false };
        // (the charm hangs under where it hangs from)
        let r = self.r;
        let (dx, dy) = (self.mouse[0] - at[0], self.mouse[1] - (at[1] + 1.0 * r));
        if dx * dx + dy * dy > (2.4 * r).powi(2) || !self.charm_place_at(self.mouse) {
            return false;
        }
        self.cosm.drag = Some((DRAG_CHARM, self.mouse));
        self.sound_tick(0.35);
        true
    }

    /// Where the look's charm hangs from on the item (the menu's own coordinates).
    fn charm_screen(&self) -> Option<[f32; 2]> {
        let p = self.cosm.last_shown.as_ref()?;
        let l = self.cosm.look.as_ref()?;
        let key = preview::model_key(&p.item, &p.paint);
        let m = store::model(&key)?;
        let own = crate::cosmetics::view::keychain_rest(&key, &m)?;
        let q = charm::point(&own, l.charm_offset);
        let [px, py] = preview::project(p, q)?;
        let [mx, my, sc, drop, _] = self.xf;
        Some([mx + (px - mx) / sc.max(1e-3), my + (py - drop - my) / sc.max(1e-3)])
    }

    /// The carried charm follows the pointer: where it meets the item it hangs there, else the item is as it was.
    pub(super) fn charm_carry_move(&mut self, x: f32, y: f32) {
        let Some(mut k) = self.cosm.carry_charm.take() else { return };
        k.moved |= (x - k.from[0]).abs() + (y - k.from[1]).abs() > 6.0;
        if k.moved && k.name.is_some() {
            if let Some(l) = self.cosm.look.as_mut() {
                l.charm = k.name.clone();
            }
            k.on_item = self.charm_place_at([x, y]);
            if !k.on_item {
                self.charm_carry_restore(&k);
            }
        }
        self.cosm.carry_charm = Some(k);
    }

    /// The look's charm and its place as before the carry.
    fn charm_carry_restore(&mut self, k: &CharmCarry) {
        let Some(l) = self.cosm.look.as_mut() else { return };
        if (l.charm.clone(), l.charm_offset) != k.backup {
            (l.charm, l.charm_offset) = k.backup.clone();
            self.charm_changed();
        }
    }

    /// The carried charm let go: on the item it stays hanging there; pressed only, it is put on (as a press always
    /// did); let go off the item, nothing changes.
    pub(super) fn charm_carry_drop(&mut self) {
        let Some(k) = self.cosm.carry_charm.take() else { return };
        if !k.moved || k.name.is_none() {
            self.charm_carry_restore(&k);
            self.charm_press_pick(k.n);
            return;
        }
        if k.on_item {
            self.charm_changed();
            self.switch_sound(true, 0.6);
        } else {
            self.charm_carry_restore(&k);
            self.sound_tick(0.3);
        }
    }

    /// The charm to the place it may hang (CS2's charm places) nearest where the pointer (the menu's) meets the item;
    /// false when it meets none of it.
    pub(super) fn charm_place_at(&mut self, mouse: [f32; 2]) -> bool {
        let Some(p) = self.cosm.last_shown.clone() else { return false };
        let [mx, my, sc, drop, _] = self.xf;
        let at = [mx + (mouse[0] - mx) * sc, my + (mouse[1] - my) * sc + drop];
        let Some((o, d)) = preview::ray(&p, at) else { return false };
        let key = preview::model_key(&p.item, &p.paint);
        let Some(m) = store::model(&key) else { return false };
        let all: Vec<u32> = (0..m.materials.len() as u32).collect();
        let Some((_, hit, _)) = sticker::raycast(&m, &all, o, d) else { return false };
        let mk = sticker::markup(key.rsplit_once('/').map_or("", |(d, _)| d));
        let places = charm::places(&mk, key.ends_with("legacy.rcm"));
        let Some(q) = charm::nearest(&places, hit) else { return false };
        // off the charm's own place, in its frame
        let Some(own) = crate::cosmetics::view::keychain_rest(&key, &m).and_then(|a| crate::cosmetics::model::inverse(&a)) else { return false };
        let offset = charm::point(&own, q);
        if let Some(l) = self.cosm.look.as_mut() {
            l.charm_offset = offset;
        }
        self.charm_changed();
        true
    }

    /// A press on the charms' parts.
    pub(super) fn charm_press(&mut self, part: u8, n: u16) {
        match part {
            CHARM_ROW => {
                self.cosm.picking_charm = !self.cosm.picking_charm;
                self.cosm.picking_look = false;
                self.cosm.charm_scroll = 0.0;
                self.cosm.charm_scroll_to = 0.0;
                self.sound_tick(0.6);
            }
            CHARM_PICK => {
                // carried (a press alone puts it on)
                let cat = catalog::get();
                let name = if n == 0 { None } else { cat.charms.get(n as usize - 1).map(|x| x.name.clone()) };
                let backup = self.cosm.look.as_ref().map_or((None, [0.0; 3]), |l| (l.charm.clone(), l.charm_offset));
                self.cosm.carry_charm = Some(CharmCarry { n, name, from: self.mouse, moved: false, on_item: false, backup });
                self.cosm.drag = Some((DRAG_CARRY_CHARM, self.mouse));
            }
            _ => {}
        }
    }

    /// A charm in the panel pressed (and let go where it was): put on.
    fn charm_press_pick(&mut self, n: u16) {
        let cat = catalog::get();
        let charm = if n == 0 { None } else { cat.charms.get(n as usize - 1).map(|x| x.name.clone()) };
        if let Some(l) = self.cosm.look.as_mut() {
            l.charm = charm.clone();
        }
        // the rifle on (the same, its finish the same) takes it at once
        self.charm_changed();
        self.cosm.picking_charm = false;
        self.switch_sound(charm.is_some(), 0.5);
    }
}
