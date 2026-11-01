//! Shulker Preview's panels over the world (Java's cursed.recode.fx.ShulkerPreview says each frame which boxes, and
//! where they are on the screen: the one looked at on the ground or in a frame, a placed one opened before, the one in
//! the hand): each uses the hotbar material with a box-coloured edge, its name and nine slots across (Java draws the
//! items: icon kind 11). They come in rising and go out softly; the hand's rests over the hotbar.

use super::draw::{grey, Canvas};
use super::text::{fonts, Face};
use super::theme::{fade, ElemLook, HudLook};
use super::{ak, Gui};

const A_SHULK: u64 = 62;

/// A box shown: who it is (its fade is its own), Java's number for its items, where on the screen (window px, the
/// box's top), how big (by its distance), its colour, the hand's or the world's, its name, each slot's count.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct ShulkPanel {
    pub id: String,
    pub slot: usize,
    pub at: [f32; 2],
    pub scale: f32,
    pub tint: u32,
    pub hand: bool,
    pub name: String,
    pub counts: Vec<u32>,
    pub items: Vec<String>,
}

#[derive(Default)]
pub(super) struct Shulk {
    pub now: Vec<ShulkPanel>,
    /// those no longer told, fading out as they were last
    pub gone: Vec<ShulkPanel>,
}

fn key(id: &str) -> usize {
    id.bytes().fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619)) as usize & 0xffff
}

/// Java's line: panels by \x1e, each "id\x1fslot\x1fx\x1fy\x1fscale\x1ftint\x1fhand\x1fname\x1fcounts".
pub(super) fn parse(s: &str) -> Vec<ShulkPanel> {
    s.split('\u{1e}')
        .filter_map(|p| {
            let f: Vec<&str> = p.split('\u{1f}').collect();
            if f.len() < 9 {
                return None;
            }
            Some(ShulkPanel {
                id: f[0].to_string(),
                slot: f[1].parse().ok()?,
                at: [f[2].parse().ok()?, f[3].parse().ok()?],
                scale: f[4].parse::<f32>().ok()?.clamp(0.3, 1.5),
                tint: u32::from_str_radix(f[5], 16).unwrap_or(0x8a7a92),
                hand: f[6] == "1",
                name: f[7].to_string(),
                counts: f[8].split(' ').filter_map(|c| c.parse().ok()).collect(),
                items: f.get(9).map_or(Vec::new(), |s| s.split('\u{1d}').map(String::from).collect()),
            })
        })
        .collect()
}

/// Match all words against a localised item name and its registry id. Decisions stay native;
/// Java only supplies the item facts and draws the icons indicated by the result.
fn item_matches(query: &str, item: &str) -> bool {
    let item = item.to_lowercase();
    query.to_lowercase().split_whitespace().all(|word| item.contains(word))
}

impl Gui {
    fn shulk_query(&self) -> String {
        self.mods.iter().find(|m| m.id == "shulker-preview" && m.on)
            .map_or(String::new(), |m| super::theme::text(&m.set, "Find item").trim().to_string())
    }

    pub(super) fn shulk_filter(&self, items: &str) -> String {
        let query = self.shulk_query();
        items.split('\u{1d}').map(|item| if item_matches(&query, item) { '1' } else { '0' }).collect()
    }

    /// This frame's boxes (Java): those gone keep fading as they were.
    pub(super) fn shulk_set(&mut self, s: &str) {
        let now = parse(s);
        let old = std::mem::take(&mut self.shulk.now);
        for p in old {
            if !now.iter().any(|n| n.id == p.id) {
                self.shulk.gone.retain(|g| g.id != p.id);
                self.shulk.gone.push(p);
            }
        }
        self.shulk.gone.retain(|g| !now.iter().any(|n| n.id == g.id));
        self.shulk.now = now;
    }

    pub(super) fn shulk_draw(&mut self, c: &mut Canvas) {
        let panels: Vec<(ShulkPanel, bool)> = self.shulk.now.iter().map(|p| (p.clone(), true)).chain(self.shulk.gone.iter().map(|p| (p.clone(), false))).collect();
        let mut done = Vec::new();
        for (p, live) in panels {
            let k = self.an(ak(A_SHULK, key(&p.id)), if live { 1.0 } else { 0.0 }, 11.0, 0.0);
            if !live && k < 0.01 {
                done.push(p.id.clone());
                continue;
            }
            self.shulk_panel(c, &p, k);
        }
        self.shulk.gone.retain(|g| !done.contains(&g.id));
    }

    fn shulk_panel(&mut self, c: &mut Canvas, p: &ShulkPanel, k: f32) {
        let query = self.shulk_query();
        let searching = !query.is_empty();
        let (w, h) = (self.hud.w, self.hud.h);
        let u = self.hud.u;
        let s = u * p.scale * (0.94 + 0.06 * k);
        let n = p.counts.len().max(1);
        let cols = n.min(9);
        let rows = n.div_ceil(9);
        let cell = 26.0 * s;
        let pad = 7.0 * s;
        let title = if p.name.is_empty() { 0.0 } else { 16.0 * s };
        let pw = cols as f32 * cell + 2.0 * pad;
        let ph = title + rows as f32 * cell + 2.0 * pad;
        // where: over the hotbar (the hand's), over the box (the world's)
        let (cx, bottom) = if p.hand {
            let top = self.hotbar_top().map_or(h - 96.0 * u, |f| h - f * h);
            (w / 2.0, top - 10.0 * u)
        } else {
            (p.at[0], p.at[1] - 6.0 * s)
        };
        let lift = (1.0 - k) * 10.0 * u;
        let x0 = (cx - pw / 2.0).clamp(4.0, (w - pw - 4.0).max(4.0));
        let y0 = (bottom - ph + lift).clamp(4.0, (h - ph - 4.0).max(4.0));
        let r = [x0, y0, x0 + pw, y0 + ph];
        let look = HudLook::of(&self.hud_set(), self.now);
        let settings = self.mods.iter().find(|m| m.id == "hb-hotbar").map(|m| m.set.as_slice()).unwrap_or(&[]);
        let el = ElemLook::of(settings, &look, self.now);
        let a0 = c.alpha;
        c.alpha = a0 * k * el.alpha;
        let tint = [((p.tint >> 16) & 255) as f32 / 255.0, ((p.tint >> 8) & 255) as f32 / 255.0, (p.tint & 255) as f32 / 255.0, 1.0];
        el.draw_plate(c, r, &look, s, self.now);
        // the box's colour along its top edge
        c.rect([r[0] + 10.0 * s, r[1], r[2] - 10.0 * s, r[1] + 2.0 * s], 1.0 * s, fade(tint, 0.9));
        if title > 0.0 {
            let nm = fonts().fit(Face::Medium, 10.5 * s, &p.name, pw - 2.0 * pad);
            c.text(r[0] + pad, r[1] + pad + 10.0 * s, 10.5 * s, Face::Medium, &nm, el.text);
        }
        let gy = r[1] + pad + title;
        let isz = cell * 0.7;
        for (i, &count) in p.counts.iter().enumerate() {
            let (col, row) = (i % 9, i / 9);
            let cell_r = [r[0] + pad + col as f32 * cell + 1.5 * s, gy + row as f32 * cell + 1.5 * s, r[0] + pad + (col + 1) as f32 * cell - 1.5 * s, gy + (row + 1) as f32 * cell - 1.5 * s];
            let matched = !searching || p.items.get(i).is_some_and(|item| item_matches(&query, item));
            c.rect(cell_r, 5.0 * s, if searching && matched && count > 0 { fade(el.accent, 0.3) } else { grey(1.0, 0.055) });
            if count == 0 {
                continue;
            }
            let (mx, my) = ((cell_r[0] + cell_r[2]) / 2.0, (cell_r[1] + cell_r[3]) / 2.0);
            // the slots come in one after another
            let wave = ((k * 1.6 - i as f32 * 0.012).clamp(0.0, 1.0)).powf(0.8);
            self.hud.icons.push([11.0, (p.slot * 64 + i) as f32, mx - isz / 2.0, my - isz / 2.0, isz, isz, c.alpha * wave * if matched { 1.0 } else { 0.2 }]);
            if count > 1 {
                let txt = if count >= 1000 { format!("{}k", count / 1000) } else { count.to_string() };
                let ts = 8.5 * s;
                c.text_r(cell_r[2] - 1.5 * s + 0.6 * s, cell_r[3] - 1.5 * s + 0.6 * s, ts, Face::Mono, &txt, grey(0.0, 0.7 * wave));
                c.text_r(cell_r[2] - 1.5 * s, cell_r[3] - 1.5 * s, ts, Face::Mono, &txt, fade(el.text, wave));
            }
        }
        c.alpha = a0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_preview_uses_hotbar_material_without_box_tint() {
        let mut g = Gui::new();
        g.hud.w = 1280.0;
        g.hud.h = 720.0;
        g.hud.u = 1.0;
        let mut p = ShulkPanel { id: "test".into(), slot: 0, at: [640.0, 400.0], scale: 1.0,
            tint: 0xff0000, hand: false, name: String::new(), counts: vec![0; 27], items: vec![] };
        let look = HudLook::of(&g.hud_set(), g.now);
        let index = g.mod_index("hb-hotbar");
        for style in 0..4 {
            for setting in &mut g.mods[index].set {
                if setting.label == "Style" {
                    if let super::super::data::Kind::Choice { v, .. } = &mut setting.kind { *v = style; }
                }
            }
            let el = ElemLook::of(&g.mods[index].set, &look, g.now);
            let mut expected = Canvas::new(1280.0, 720.0);
            expected.alpha = el.alpha;
            el.draw_plate(&mut expected, [516.0, 302.0, 764.0, 394.0], &look, 1.0, g.now);
            assert!(!expected.p.is_empty());
            for tint in [0xff0000, 0x00ff00, 0x896a98] {
                p.tint = tint;
                let mut actual = Canvas::new(1280.0, 720.0);
                g.shulk_panel(&mut actual, &p, 1.0);
                assert_eq!(&actual.p[..expected.p.len()], expected.p.as_slice());
                assert_eq!(actual.glass_rects, expected.glass_rects);
            }
        }
    }

    #[test]
    fn search_matches_names_ids_and_all_words_in_any_case() {
        assert!(item_matches("алмаз меч", "minecraft:diamond_sword Алмазный меч"));
        assert!(item_matches("DIAMOND", "minecraft:diamond_sword Алмазный меч"));
        assert!(!item_matches("алмаз лук", "minecraft:diamond_sword Алмазный меч"));
        assert!(item_matches("  ", ""));
        let p = parse("box\u{1f}0\u{1f}0\u{1f}0\u{1f}1\u{1f}ffffff\u{1f}1\u{1f}Box\u{1f}1 2\u{1f}Stone\u{1d}Diamond");
        assert_eq!(p[0].items, vec!["Stone", "Diamond"]);
    }

    #[test]
    fn javas_line_is_read() {
        let l = parse("e42\u{1f}0\u{1f}960.5\u{1f}400\u{1f}0.8\u{1f}f9801d\u{1f}0\u{1f}Loot\u{1f}1 0 64\u{1e}bad");
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].counts, vec![1, 0, 64]);
        assert_eq!(l[0].tint, 0xf9801d);
        assert!(!l[0].hand);
    }
}
