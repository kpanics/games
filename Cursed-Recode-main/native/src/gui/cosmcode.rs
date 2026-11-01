//! The Cosmetics page's codes: the item in the look — its finish, wear, pattern, StatTrak™, charm and where it hangs,
//! stickers, name tag, material — as a code to copy ("RCS1-…", the loadout's own line squeezed), and a code pasted
//! opened in the look as it was.

use super::draw::{mix, Canvas};
use super::i18n::tr;
use super::theme::pal;
use super::{ak, Gui, Hit, A_HOV};
use crate::cosmetics::catalog;
use crate::cosmetics::loadout::{self, Equip, Slot};

/// The parts (Hit::Cosm).
pub(super) const CODE_COPY: u8 = 35;
pub(super) const CODE_PASTE: u8 = 36;

const PREFIX: &str = "RCS1-";

/// The item as the look shows it, as the loadout would keep it (StatTrak's count the item on's).
fn equip_of(gui: &Gui) -> Option<Equip> {
    let l = gui.cosm.look.as_ref()?;
    let kind = catalog::get().item(&l.item).map(|i| i.kind)?;
    let (lo, _) = loadout::current();
    let kept = lo.get(Slot::of(&l.item, kind)).filter(|e| e.item == l.item && e.paint == l.paint).and_then(|e| e.stattrak);
    Some(Equip {
        item: l.item.clone(),
        paint: l.paint.clone(),
        seed: l.seed,
        wear: l.wear,
        stattrak: l.stattrak.then(|| kept.unwrap_or(0)),
        charm: l.charm.clone(),
        charm_offset: (l.charm.is_some() && l.charm_offset != [0.0; 3]).then_some(l.charm_offset),
        stickers: l.stickers.clone(),
        nametag: l.nametag.clone().filter(|n| !n.is_empty()),
        overlay: l.overlay,
    })
}

/// An item's code.
pub fn encode(e: &Equip) -> String {
    super::configs::squeeze(PREFIX, &e.dump())
}

/// A code back to its item (none: not one of ours, or an item the catalog has not).
pub fn decode(code: &str) -> Option<Equip> {
    let e = Equip::parse(&super::configs::unsqueeze(PREFIX, code)?)?;
    catalog::get().item(&e.item).is_some().then_some(e)
}

impl Gui {
    /// The look's two code buttons at the panel's top right (`at` their right edge and top): copy, paste.
    pub(super) fn code_buttons(&mut self, c: &mut Canvas, at: [f32; 2]) -> f32 {
        let r = self.r;
        let side = 1.6 * r;
        let mut x = at[0];
        for (part, k) in [(CODE_PASTE, 0usize), (CODE_COPY, 1)] {
            let b = [x - side, at[1], x, at[1] + side];
            let hit = Hit::Cosm(part, 0);
            let hv = self.an(ak(A_HOV, 35_400 + k), if self.hover == hit { 1.0 } else { 0.0 }, 14.0, 0.0);
            c.rect(b, 0.45 * r, mix(pal().f1, pal().f3, hv));
            let col = mix(pal().t3, pal().t1, hv);
            let m = [(b[0] + b[2]) * 0.5, (b[1] + b[3]) * 0.5];
            let u = 0.17 * r;
            let rect = |c: &mut Canvas, x0: f32, y0: f32, x1: f32, y1: f32| {
                c.polyline(&[[x0, y0], [x1, y0], [x1, y1], [x0, y1], [x0, y0]], 1.3, col);
            };
            if part == CODE_COPY {
                // two sheets, one over the other
                rect(c, m[0] - 2.2 * u, m[1] - 1.2 * u, m[0] + 0.8 * u, m[1] + 2.2 * u);
                c.polyline(&[[m[0] - 1.0 * u, m[1] - 1.2 * u], [m[0] - 1.0 * u, m[1] - 2.4 * u], [m[0] + 2.2 * u, m[1] - 2.4 * u], [m[0] + 2.2 * u, m[1] + 1.0 * u], [m[0] + 0.8 * u, m[1] + 1.0 * u]], 1.3, col);
            } else {
                // a board, its clip on top
                rect(c, m[0] - 2.0 * u, m[1] - 1.7 * u, m[0] + 2.0 * u, m[1] + 2.4 * u);
                rect(c, m[0] - 0.9 * u, m[1] - 2.4 * u, m[0] + 0.9 * u, m[1] - 1.2 * u);
            }
            self.hits.push((b, hit));
            x -= side + 0.35 * r;
        }
        at[0] - x
    }

    /// A press on a code button: the item's code copied, or the clipboard asked for one.
    pub(super) fn code_press(&mut self, part: u8) {
        match part {
            CODE_COPY => {
                if let Some(e) = equip_of(self) {
                    self.copy(&encode(&e));
                    self.toast(tr("The item's code is copied").to_string());
                }
            }
            CODE_PASTE => {
                self.cosm.paste_item = true;
                self.outbox.push("paste".into());
            }
            _ => {}
        }
        self.sound_tick(0.5);
    }

    /// The clipboard came for an item: its code opened in the look, or a word that it is not one.
    pub(super) fn item_paste(&mut self, text: &str) {
        match decode(text.trim()) {
            Some(e) => {
                self.cosm_open(&e);
                self.toast(tr("The item from the code").to_string());
            }
            None => self.toast(tr("That is not an item's code").to_string()),
        }
    }

    /// Development (dev.rs "icode"): the look's item's code, said (the clipboard left alone).
    pub(super) fn item_code_said(&self) {
        if let Some(e) = equip_of(self) {
            eprintln!("[gui dev] item code {}", encode(&e));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_items_code_reads_back() {
        let e = Equip::parse("weapon_awp|cu_awp_asimov|0|0.19|123457|kc_missinglink_cat@0.40000,0.00000,-0.02000||Lunuca's AWP|3,0.75,ff7020").unwrap();
        let code = super::super::configs::squeeze(PREFIX, &e.dump());
        assert!(code.starts_with(PREFIX));
        let back = Equip::parse(&super::super::configs::unsqueeze(PREFIX, &code).unwrap()).unwrap();
        assert_eq!(back, e);
        assert_eq!(super::super::configs::unsqueeze(PREFIX, "RCD1-abc"), None);
    }
}
