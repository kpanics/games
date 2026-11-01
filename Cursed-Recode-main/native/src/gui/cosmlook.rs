//! The Cosmetics page's materials (cosmetics/overlay.rs: Recode's own looks over a CS2 finish — the Shadow's smoke,
//! the Polymorph's mercury, the Cursed's ash, a glow, an iridescence, a colour of its own): in the look, its row, and —
//! pressed — the looks in the panel in place of the rest (none first), how much of it, its colour; the item on takes
//! it at once.

use super::draw::{hex, mix, Canvas};
use super::i18n::tr;
use super::text::{fonts, Face};
use super::theme::pal;
use super::{ak, Gui, Hit, Owner, A_HOV};
use crate::cosmetics::catalog::{self, Kind};
use crate::cosmetics::loadout::{self, Slot};
use crate::cosmetics::overlay::{index_of, Overlay, KINDS};

/// The parts (Hit::Cosm).
pub(super) const LOOK_ROW: u8 = 31;
pub(super) const LOOK_PICK: u8 = 32;
pub(super) const LOOK_COLOUR: u8 = 33;
pub(super) const LOOK_STRENGTH: u8 = 34;
/// What is held (cosm.drag): how much of it.
pub(super) const DRAG_STRENGTH: u8 = 8;

fn rgb([r, g, b]: [u8; 3]) -> u32 {
    (r as u32) << 16 | (g as u32) << 8 | b as u32
}

impl Gui {
    /// The row of the look's material: its name, the way to the looks.
    pub(super) fn look_row(&mut self, c: &mut Canvas, b: [f32; 4], o: Option<Overlay>) {
        let r = self.r;
        let hit = Hit::Cosm(LOOK_ROW, 0);
        let hv = self.an(ak(A_HOV, 34_900), if self.hover == hit { 1.0 } else { 0.0 }, 14.0, 0.0);
        c.rect(b, 0.6 * r, mix(pal().f1, pal().f2, hv));
        let mid = (b[1] + b[3]) * 0.5 + 0.26 * r;
        c.text(b[0] + 0.7 * r, mid, 0.72 * r, Face::Medium, tr("Material"), pal().t3);
        let name = o.and_then(|o| index_of(o.kind)).map_or(tr("None"), |i| tr(KINDS[i].0));
        let w = c.text_r(b[2] - 1.4 * r, mid, 0.72 * r, Face::Semi, name, mix(pal().t2, pal().t1, hv));
        if let Some(o) = o {
            // its colour, beside its name
            let m = [b[2] - 1.4 * r - w - 0.55 * r, (b[1] + b[3]) * 0.5];
            c.rect([m[0] - 0.22 * r, m[1] - 0.22 * r, m[0] + 0.22 * r, m[1] + 0.22 * r], 0.22 * r, hex(rgb(o.colour_now()), 1.0));
        }
        let m = [b[2] - 0.75 * r, (b[1] + b[3]) * 0.5];
        let pts = if self.cosm.picking_look {
            [[m[0] - 0.35 * r, m[1] - 0.15 * r], [m[0], m[1] + 0.2 * r], [m[0] + 0.35 * r, m[1] - 0.15 * r]]
        } else {
            [[m[0] - 0.2 * r, m[1] - 0.35 * r], [m[0] + 0.15 * r, m[1]], [m[0] - 0.2 * r, m[1] + 0.35 * r]]
        };
        c.polyline(&pts, 1.5, mix(pal().t3, pal().t1, hv));
        self.hits.push((b, hit));
    }

    /// The looks in the panel `pr`: none and each by name (its own colour beside it), how much of it, its colour (the
    /// client's own picker opens from its row: hue, brightness, Chroma, hex, the ready ones, its own again).
    pub(super) fn look_panel(&mut self, c: &mut Canvas, pr: [f32; 4], o: Option<Overlay>) {
        let r = self.r;
        let (x0, x1) = (pr[0] + 0.9 * r, pr[2] - 0.9 * r);
        let mut y = pr[1];
        let gap = 0.35 * r;
        let cw = (x1 - x0 - gap) / 2.0;
        let ch = 1.9 * r;
        let chosen_at = o.and_then(|o| index_of(o.kind)).map_or(0, |i| i + 1);
        for n in 0..=KINDS.len() {
            let (cx, cy) = (n % 2, n / 2);
            let b = [x0 + cx as f32 * (cw + gap), y + cy as f32 * (ch + gap), x0 + cx as f32 * (cw + gap) + cw, y + cy as f32 * (ch + gap) + ch];
            let chosen = chosen_at == n;
            let hit = Hit::Cosm(LOOK_PICK, n as u16);
            let hv = self.an(ak(A_HOV, 35_000 + 4100 + n), if chosen { 1.0 } else if self.hover == hit { 0.5 } else { 0.0 }, 14.0, 0.0);
            c.rect(b, 0.55 * r, mix(pal().f1, pal().f3, hv));
            let mid = (b[1] + b[3]) * 0.5 + 0.25 * r;
            let label = if n == 0 { tr("None") } else { tr(KINDS[n - 1].0) };
            let mut tx = b[0] + 0.6 * r;
            if n > 0 {
                let m = [tx + 0.25 * r, (b[1] + b[3]) * 0.5];
                // its own colour, breathing a little (the looks move)
                let k = 0.85 + 0.15 * (self.now * 2.2 + n as f32).sin();
                c.rect([m[0] - 0.25 * r, m[1] - 0.25 * r, m[0] + 0.25 * r, m[1] + 0.25 * r], 0.25 * r, hex(rgb(KINDS[n - 1].2), k));
                tx += 0.85 * r;
            }
            c.text(tx, mid, 0.68 * r, Face::Medium, &fonts().fit(Face::Medium, 0.68 * r, label, b[2] - tx - 0.3 * r), mix(pal().t3, pal().t1, hv));
            self.hits.push((b, hit));
        }
        y += (KINDS.len() / 2 + 1) as f32 * (ch + gap) + 0.6 * r;
        let Some(o) = o else { return };
        // how much of it
        c.text(x0, y, 0.68 * r, Face::Medium, tr("Strength"), pal().t3);
        c.text_r(x1, y, 0.68 * r, Face::Mono, &format!("{:.0}%", o.strength * 100.0), pal().t2);
        y += 0.55 * r;
        self.cosm.look_track = [x0, x1];
        self.cosm_slider(c, [x0, y, x1, y + 1.4 * r], o.strength, Hit::Cosm(LOOK_STRENGTH, 0), 35_200);
        y += 2.2 * r;
        // its colour: a row as the menu's colour rows are, the picker opening by it
        let b = [x0, y, x1, y + 1.9 * r];
        let hit = Hit::Cosm(LOOK_COLOUR, 0);
        let open = self.pick.as_ref().is_some_and(|p| p.owner == Owner::Cosm && p.set == 0);
        let hv = self.an(ak(A_HOV, 35_300), if open { 1.0 } else if self.hover == hit { 0.5 } else { 0.0 }, 14.0, 0.0);
        c.rect(b, 0.55 * r, mix(pal().f1, pal().f3, hv));
        let mid = (b[1] + b[3]) * 0.5 + 0.25 * r;
        c.text(b[0] + 0.6 * r, mid, 0.68 * r, Face::Medium, tr("Colour"), mix(pal().t3, pal().t1, hv));
        let now = o.colour_now();
        let hexs = format!("#{:02X}{:02X}{:02X}", now[0], now[1], now[2]);
        let w = c.text_r(b[2] - 2.2 * r, mid, 0.66 * r, Face::Mono, &hexs, pal().t2);
        let sw = [b[2] - 1.75 * r, b[1] + 0.45 * r, b[2] - 0.55 * r, b[3] - 0.45 * r];
        c.rect(sw, 0.3 * r, hex(rgb(now), 1.0));
        if o.chroma > 0.0 {
            c.text_r(b[2] - 2.2 * r - w - 0.5 * r, mid, 0.6 * r, Face::Medium, tr("Chroma"), pal().t3);
        }
        self.cosm.look_colour_at = b;
        self.hits.push((b, hit));
        self.look_colour_follow();
    }

    /// The picker open on the material's colour: what it says taken into the look (its brightness and hue the colour,
    /// its transparency how much of the material, its Chroma the hue running round); the item on takes it as the hand
    /// lets go.
    fn look_colour_follow(&mut self) {
        let Some((owner, set, drag)) = self.pick.as_ref().map(|p| (p.owner, p.set, p.drag)) else { return };
        if owner != Owner::Cosm || set != 0 {
            return;
        }
        let super::data::Kind::Colour(col) = self.cosm_cols[0].kind else { return };
        let Some(o) = self.cosm.look.as_mut().and_then(|l| l.overlay.as_mut()) else { return };
        let want = Overlay { colour: [(col.rgb >> 16) as u8, (col.rgb >> 8) as u8, col.rgb as u8], strength: (col.a * 100.0).round() / 100.0, chroma: col.chroma, ..*o };
        if want != *o {
            *o = want;
            if drag == 0 {
                self.look_changed();
            } else {
                self.cosm.look_dirty = true;
            }
        } else if drag == 0 && std::mem::take(&mut self.cosm.look_dirty) {
            self.look_changed();
        }
    }

    /// The look's material changed: the item on (the same, its finish the same) takes it at once.
    fn look_changed(&mut self) {
        let Some(l) = self.cosm.look.as_ref() else { return };
        let kind = catalog::get().item(&l.item).map_or(Kind::Knife, |i| i.kind);
        let (item, paint, o) = (l.item.clone(), l.paint.clone(), l.overlay);
        loadout::change(|lo| {
            if let Some(e) = lo.get_mut(Slot::of(&item, kind)).filter(|e| e.item == item && e.paint == paint) {
                e.overlay = o;
            }
        });
    }

    /// A press on the materials' parts.
    pub(super) fn look_press(&mut self, part: u8, n: u16) {
        match part {
            LOOK_ROW => {
                self.cosm.picking_look = !self.cosm.picking_look;
                self.cosm.picking_charm = false;
                self.cosm.picking_sticker = false;
                self.sound_tick(0.6);
            }
            LOOK_PICK => {
                if let Some(l) = self.cosm.look.as_mut() {
                    // another look comes in its own colour; the same again keeps how it was
                    let want = KINDS.get((n as usize).wrapping_sub(1)).map(|k| k.1);
                    l.overlay = match (want, l.overlay) {
                        (None, _) => None,
                        (Some(k), Some(o)) if o.kind == k => Some(o),
                        (Some(k), _) => Some(Overlay::of(k)),
                    };
                }
                // (the picker on the colour of the look before: closed)
                if self.pick.as_ref().is_some_and(|p| p.owner == Owner::Cosm && p.set == 0) {
                    self.pick = None;
                }
                self.look_changed();
                self.switch_sound(n != 0, 0.5);
            }
            LOOK_COLOUR => {
                let _ = n;
                let Some(o) = self.cosm.look.as_ref().and_then(|l| l.overlay) else { return };
                if self.pick.as_ref().is_some_and(|p| p.owner == Owner::Cosm && p.set == 0) {
                    self.pick = None;
                    self.sound_tick(0.4);
                    return;
                }
                // the picker on a setting of the page's own: the look's colour in it, its own colour the default
                let own = index_of(o.kind).map_or(0xffffff, |i| rgb(KINDS[i].2));
                self.cosm_cols[0].kind = super::data::Kind::Colour(super::data::Col { rgb: rgb(o.colour), a: o.strength, chroma: o.chroma, auto: false });
                self.cosm_cols[0].def = super::data::Kind::Colour(super::data::Col::new(own));
                self.open_picker(Owner::Cosm, 0, self.cosm.look_colour_at);
            }
            LOOK_STRENGTH => {
                self.cosm.drag = Some((DRAG_STRENGTH, self.mouse));
                self.look_strength(self.mouse[0]);
            }
            _ => {}
        }
    }

    /// How much of the material, from where the pointer is on its slider.
    pub(super) fn look_strength(&mut self, x: f32) {
        let [a, b] = self.cosm.look_track;
        let t = ((x - a) / (b - a).max(1.0)).clamp(0.0, 1.0);
        if let Some(o) = self.cosm.look.as_mut().and_then(|l| l.overlay.as_mut()) {
            o.strength = (t * 100.0).round() / 100.0;
        }
        self.look_changed();
    }
}
