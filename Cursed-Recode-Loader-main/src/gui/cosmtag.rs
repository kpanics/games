//! The Cosmetics page's name tag (CS2's: a plate on the weapon with its name — cosmetics/nametag.rs writes it): in the
//! look, its row — pressed, the name is typed into it, the plate on the item written as it is typed.

use super::draw::{mix, Canvas};
use super::i18n::tr;
use super::text::{fonts, Face};
use super::theme::pal;
use super::{ak, Gui, Hit, A_HOV};
use crate::cosmetics::catalog::{self, Kind};
use crate::cosmetics::loadout::{self, Slot};
use crate::cosmetics::nametag;

/// The part (Hit::Cosm).
pub(super) const TAG_ROW: u8 = 30;

impl Gui {
    /// The row of the look's name tag: its name (typed into while it is pressed).
    pub(super) fn tag_row(&mut self, c: &mut Canvas, b: [f32; 4], name: Option<&str>) {
        let r = self.r;
        let hit = Hit::Cosm(TAG_ROW, 0);
        let typing = self.cosm.tag_typing;
        let hv = self.an(ak(A_HOV, 34_800), if typing { 1.0 } else if self.hover == hit { 0.5 } else { 0.0 }, 14.0, 0.0);
        c.rect(b, 0.6 * r, mix(pal().f1, pal().f2, hv));
        let mid = (b[1] + b[3]) * 0.5 + 0.26 * r;
        let lw = c.text(b[0] + 0.7 * r, mid, 0.72 * r, Face::Medium, tr("Name tag"), pal().t3);
        let room = b[2] - b[0] - lw - 2.4 * r;
        let name = name.filter(|n| !n.is_empty());
        match name {
            Some(n) => {
                let shown = fonts().fit(Face::Semi, 0.72 * r, n, room);
                c.text_r(b[2] - 0.7 * r, mid, 0.72 * r, Face::Semi, &shown, mix(pal().t2, pal().t1, hv));
            }
            None if !typing => {
                c.text_r(b[2] - 0.7 * r, mid, 0.72 * r, Face::Medium, tr("None"), pal().t4);
            }
            None => {}
        }
        if typing && (self.now * 2.0).fract() < 0.5 {
            // the caret after the name
            let x = b[2] - 0.62 * r;
            c.rect([x, mid - 0.62 * r, x + 0.07 * r, mid + 0.12 * r], 0.0, pal().t2);
        }
        self.hits.push((b, hit));
    }

    /// A press on the name tag's row: typed into, or no longer.
    pub(super) fn tag_press(&mut self) {
        self.cosm.tag_typing = !self.cosm.tag_typing;
        self.cosm.sticker_typing = false;
        self.sound_tick(0.5);
    }

    /// The look's name tag changed: the item on (the same, its finish the same) takes it at once.
    fn tag_changed(&mut self) {
        let Some(l) = self.cosm.look.as_ref() else { return };
        let kind = catalog::get().item(&l.item).map_or(Kind::Knife, |i| i.kind);
        let (item, paint) = (l.item.clone(), l.paint.clone());
        let tag = l.nametag.clone().filter(|n| !n.is_empty());
        loadout::change(|lo| {
            if let Some(e) = lo.get_mut(Slot::of(&item, kind)).filter(|e| e.item == item && e.paint == paint) {
                e.nametag = tag.clone();
            }
        });
    }

    /// A key while the name is typed: true when it was the name's.
    pub(super) fn tag_key(&mut self, escape: bool, enter: bool, backspace: bool) -> bool {
        if !self.cosm.tag_typing {
            return false;
        }
        if backspace {
            if let Some(l) = self.cosm.look.as_mut() {
                let mut n = l.nametag.take().unwrap_or_default();
                n.pop();
                l.nametag = (!n.is_empty()).then_some(n);
            }
            self.tag_changed();
        }
        if escape || enter {
            self.cosm.tag_typing = false;
        }
        true
    }

    /// A letter typed while the name is: true when it was the name's.
    pub(super) fn tag_char(&mut self, ch: char) -> bool {
        if !self.cosm.tag_typing {
            return false;
        }
        if let Some(l) = self.cosm.look.as_mut() {
            let n = nametag::clean(&format!("{}{ch}", l.nametag.as_deref().unwrap_or("")));
            l.nametag = (!n.is_empty()).then_some(n);
        }
        self.tag_changed();
        true
    }
}
