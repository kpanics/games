//! The Cosmetics page's cases (cosmetics/cases.rs: CS2's weapon cases, its odds): the cases as cards; one looked into —
//! its picture floating in its light, its odds, the button that opens it, what it holds (its skins by rarity, the gold
//! card of its rare special items: pressed, them) or what the cases gave before; an opening as CS2's — the roulette
//! running under the marker and slowing, a tick as each card goes by (a press on it: at once to its end), what it gave
//! coming up large in its rarity's light, to put on, to inspect, or to open another. What is won is kept, put on as
//! anything of the page (knives, gloves, the AWP and the SSG 08); the rest stays in the collection.

use super::draw::{grey, hex, mix, Canvas};
use super::i18n::{ru, tr, trn};
use super::text::{fonts, Face};
use super::theme::pal;
use super::{ak, Gui, Hit, A_HOV};
use crate::cosmetics::cases::{self, Case, Drop, Rng, Won, ODDS, RARITIES, SPECIAL};
use crate::cosmetics::catalog::{self, Kind};
use crate::cosmetics::loadout::{self, Equip, Slot};
use crate::cosmetics::sound;

/// The parts (Hit::Cosm): a case's card, the button that opens it, the way back, the gold card, the tabs (contents,
/// history), the buttons under what was won (put on, inspect, again, back), a row of the history (put on).
pub(super) const CASE_PICK: u8 = 48;
pub(super) const CASE_OPEN: u8 = 49;
pub(super) const CASE_BACK: u8 = 50;
pub(super) const CASE_RARE: u8 = 51;
pub(super) const CASE_TAB: u8 = 52;
pub(super) const CASE_ACT: u8 = 53;
pub(super) const CASE_HIST: u8 = 54;
/// The roulette pressed on: at once to its end.
pub(super) const CASE_SKIP: u8 = 55;

/// The roulette's cards, the one won among them, how long it runs.
const CARDS: usize = 48;
const WIN: usize = 41;
const RUN: f32 = 6.2;
/// Before it runs: the case unlocked, the strip coming in.
const LEAD: f32 = 0.55;

/// An opening going on: the case, its cards, what it gave, when it began running, where under the marker it stops
/// (a share of the card won), the card last under the marker (its tick heard), when what it gave was shown, whether it
/// was put on.
#[derive(Clone, Debug)]
pub struct Spin {
    pub case: String,
    pub cards: Vec<Drop>,
    pub won: Won,
    pub t0: f32,
    pub land: f32,
    pub last: i64,
    pub shown_at: Option<f32>,
    pub on: bool,
}

/// A rarity's colour (CS2's; the rare special item's gold).
fn colour(rarity: u8) -> u32 {
    if rarity >= SPECIAL {
        0xE4AE39
    } else {
        catalog::rarity_colour(Kind::Rifle, RARITIES[rarity as usize])
    }
}

/// A rarity's name, as the page says it.
fn rarity_label(rarity: u8) -> &'static str {
    if rarity >= SPECIAL {
        "Rare Special Item"
    } else {
        catalog::rarity_name(Kind::Rifle, RARITIES[rarity as usize])
    }
}

fn case_name(c: &Case) -> &str {
    if ru() {
        &c.ru
    } else {
        &c.en
    }
}

fn drop_name(d: &Drop) -> &str {
    if ru() {
        &d.ru
    } else {
        &d.en
    }
}

/// What can go on (the page's knives, gloves and rifles): its slot.
fn slot_of(weapon: &str) -> Option<Slot> {
    catalog::get().item(weapon).map(|i| Slot::of(weapon, i.kind))
}

fn equip_of(w: &Won) -> Equip {
    Equip { item: w.weapon.clone(), paint: w.paint.clone(), seed: w.seed, wear: w.wear, stattrak: w.stattrak.then_some(0), charm: None, charm_offset: None, stickers: Vec::new(), nametag: None, overlay: None }
}

/// A drop's name split in two (the weapon, the finish) for a card's two lines.
fn two_lines(name: &str) -> (String, String) {
    match name.split_once(" | ") {
        Some((a, b)) => (a.to_string(), b.to_string()),
        None => (name.to_string(), String::new()),
    }
}

impl Gui {
    /// The cases' part of the page in `body` (under the chips): an opening, a case looked into, or the cases.
    pub(super) fn cosm_cases(&mut self, c: &mut Canvas, body: [f32; 4]) {
        let all = cases::get();
        if !self.cosm.case_sounds {
            self.cosm.case_sounds = true;
            let mut names: Vec<String> = ["UI.CrateScroll", "UI.CrateUnlock", "UI.CrateDrop", "UI.CrateKnife"].map(String::from).to_vec();
            for r in &RARITIES[..4] {
                names.push(format!("UI.CrateReveal.{r}"));
                names.push(format!("UI.CrateAwarded.{r}"));
            }
            sound::prepare(names);
        }
        if all.cases.is_empty() {
            let r = self.r;
            let m = [(body[0] + body[2]) * 0.5, body[1] + (body[3] - body[1]) * 0.4];
            self.cosm_dots(c, m, 1.0);
            c.text_c(m[0], m[1] + 2.2 * r, 0.74 * r, Face::Medium, tr("Fetching CS2's things…"), pal().t3);
            return;
        }
        if let Some(sp) = self.cosm.opening.clone() {
            if let Some(case) = all.case(&sp.case) {
                self.case_spin(c, body, case, sp);
                return;
            }
            self.cosm.opening = None;
        }
        match self.cosm.case.clone().and_then(|n| all.case(&n).cloned()) {
            Some(case) => self.case_view(c, body, &case),
            None => self.case_grid(c, body, &all.cases),
        }
    }

    /// The cases as cards, the newest first: its picture, its name, how much it holds.
    fn case_grid(&mut self, c: &mut Canvas, body: [f32; 4], list: &[Case]) {
        let r = self.r;
        let gap = 0.7 * r;
        let cols = (((body[2] - body[0]) + gap) / (9.5 * r + gap)).floor().max(1.0) as usize;
        let cw = ((body[2] - body[0]) - gap * (cols as f32 - 1.0)) / cols as f32;
        let ih = (cw - r) * 0.75;
        let ch = 0.4 * r + ih + 2.9 * r;
        let rows = list.len().div_ceil(cols);
        self.cosm.case_area = body;
        self.cosm.case_content = (rows as f32 * (ch + gap) - gap - (body[3] - body[1])).max(0.0);
        self.cosm.case_scroll_to = self.cosm.case_scroll_to.clamp(0.0, self.cosm.case_content);
        self.cosm.case_scroll += (self.cosm.case_scroll_to - self.cosm.case_scroll) * (1.0 - (-self.dt * 14.0).exp());
        c.push_clip(body);
        let a0 = c.alpha;
        let mut shown = 0usize;
        for (n, case) in list.iter().rev().enumerate() {
            let (cx, cy) = (n % cols, n / cols);
            let x0 = body[0] + cx as f32 * (cw + gap);
            let y0 = body[1] + cy as f32 * (ch + gap) - self.cosm.case_scroll;
            if y0 > body[3] || y0 + ch < body[1] {
                continue;
            }
            // (each a beat after the one before it)
            let (ca, cdy) = self.rise_c(0.22, 0.022 * shown.min(20) as f32, false);
            shown += 1;
            c.alpha = a0 * ca;
            let rr = [x0, y0 + cdy, x0 + cw, y0 + cdy + ch];
            let hit = Hit::Cosm(CASE_PICK, n as u16);
            let hv = self.an(ak(A_HOV, 37_000 + n), if self.hover == hit { 1.0 } else { 0.0 }, 14.0, 0.0);
            c.rect_lit(rr, 0.8 * r, mix(pal().f1, pal().f2, hv), grey(1.0, 0.04 * hv), 2.0);
            let ir = [rr[0] + 0.5 * r, rr[1] + 0.4 * r, rr[2] - 0.5 * r, rr[1] + 0.4 * r + ih];
            // a soft light under it, brighter under the pointer
            c.glow([(ir[0] + ir[2]) * 0.5, (ir[1] + ir[3]) * 0.55], (ir[2] - ir[0]) * 0.45, grey(1.0, 0.03 + 0.05 * hv));
            let pic = self.cosm_pic(&format!("cases/{}.rtx", case.name), 256);
            let seen = pic.map_or(0.0, |p| p.2);
            self.cosm_waiting(c, [ir[0] + 1.2 * r, ir[1] + 0.6 * r, ir[2] - 1.2 * r, ir[3] - 0.6 * r], seen, n);
            if let Some((uv, [iw, ih2], seen)) = pic {
                let k = ((ir[2] - ir[0]) / iw as f32).min((ir[3] - ir[1]) / ih2 as f32) * (0.92 + 0.08 * hv) * (0.96 + 0.04 * seen);
                let (w, h) = (iw as f32 * k, ih2 as f32 * k);
                let m = [(ir[0] + ir[2]) / 2.0, (ir[1] + ir[3]) / 2.0 - 0.15 * r * hv];
                c.pic([m[0] - w / 2.0, m[1] - h / 2.0, m[0] + w / 2.0, m[1] + h / 2.0], uv, seen, false, 1.0, 1.0, 0.0);
            }
            let nm = fonts().fit(Face::Semi, 0.78 * r, case_name(case), cw - 1.6 * r);
            c.text(rr[0] + 0.8 * r, ir[3] + 1.05 * r, 0.78 * r, Face::Semi, &nm, mix(pal().t2, pal().t1, hv));
            let skins = case.drops.iter().filter(|d| d.rarity < SPECIAL).count();
            let line = format!("{} + ★", super::cosmui::skins(skins));
            c.text(rr[0] + 0.8 * r, ir[3] + 1.9 * r, 0.62 * r, Face::Regular, &line, pal().t4);
            c.rect([rr[0] + 0.8 * r, rr[3] - 0.42 * r, rr[2] - 0.8 * r, rr[3] - 0.26 * r], 0.08 * r, hex(colour(SPECIAL), 0.9));
            self.hits.push(([rr[0], rr[1].max(body[1]), rr[2], rr[3].min(body[3])], hit));
        }
        c.alpha = a0;
        c.pop_clip();
    }

    /// The way back and a title at the top of `body` (`what` the back's own); where what is under it begins.
    fn case_head(&mut self, c: &mut Canvas, body: [f32; 4], title: &str) -> f32 {
        let r = self.r;
        let y = body[1];
        let hit = Hit::Cosm(CASE_BACK, 0);
        let hv = self.an(ak(A_HOV, 37_400), if self.hover == hit { 1.0 } else { 0.0 }, 14.0, 0.0);
        let b = [body[0], y, body[0] + 2.2 * r, y + 2.2 * r];
        c.rect(b, 0.7 * r, grey(1.0, 0.04 + 0.06 * hv));
        let m = [(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0];
        c.polyline(&[[m[0] + 0.25 * r, m[1] - 0.45 * r], [m[0] - 0.25 * r, m[1]], [m[0] + 0.25 * r, m[1] + 0.45 * r]], 1.6, mix(pal().t3, pal().t1, hv));
        self.hits.push((b, hit));
        c.text(b[2] + 0.8 * r, y + 1.5 * r, 1.0 * r, Face::Bold, title, pal().t1);
        y + 3.0 * r
    }

    /// A case looked into: at the left its picture floating in its light, the button that opens it, its odds; at the
    /// right its contents (or the history), as cards.
    fn case_view(&mut self, c: &mut Canvas, body: [f32; 4], case: &Case) {
        let r = self.r;
        let y = self.case_head(c, body, case_name(case));
        let lw = (17.0 * r).min((body[2] - body[0]) * 0.36);
        let left = [body[0], y, body[0] + lw, body[3]];
        c.rect(left, r, grey(1.0, 0.035));
        // its picture, floating a little, its light breathing
        let pw = lw - 2.0 * r;
        let bob = if pal().anim { (self.now * 1.3).sin() * 0.18 * r } else { 0.0 };
        let pc = [left[0] + lw * 0.5, left[1] + 1.0 * r + pw * 0.4];
        let breath = if pal().anim { 0.5 + 0.5 * (self.now * 1.1).sin() } else { 0.5 };
        c.glow([pc[0], pc[1] + 0.2 * pw], pw * 0.55, [0.894, 0.682, 0.224, 0.05 + 0.03 * breath]);
        c.glow(pc, pw * 0.45, grey(1.0, 0.05));
        if let Some((uv, [iw, ih], seen)) = self.cosm_pic(&format!("cases/{}.rtx", case.name), 512) {
            let k = (pw / iw as f32).min(pw * 0.75 / ih as f32) * (0.96 + 0.04 * seen);
            let (w, h) = (iw as f32 * k, ih as f32 * k);
            c.pic([pc[0] - w / 2.0, pc[1] - h / 2.0 + bob, pc[0] + w / 2.0, pc[1] + h / 2.0 + bob], uv, seen, false, 1.0, 1.0, 0.0);
        }
        let mut yy = left[1] + 1.0 * r + pw * 0.8 + 0.8 * r;
        // the button that opens it
        let ob = [left[0] + r, yy, left[2] - r, yy + 2.6 * r];
        let hit = Hit::Cosm(CASE_OPEN, 0);
        let hv = self.an(ak(A_HOV, 37_410), if self.hover == hit { 1.0 } else { 0.0 }, 14.0, 0.0);
        c.glow([(ob[0] + ob[2]) * 0.5, (ob[1] + ob[3]) * 0.5], (ob[2] - ob[0]) * 0.6, grey(1.0, 0.04 + 0.06 * hv));
        c.rect(ob, 1.3 * r, grey(0.86 + 0.14 * hv, 1.0));
        c.text_c((ob[0] + ob[2]) * 0.5, (ob[1] + ob[3]) * 0.5 + 0.3 * r, 0.82 * r, Face::Semi, tr("Open the case"), grey(0.06, 1.0));
        self.hits.push((ob, hit));
        yy = ob[3] + 1.5 * r;
        // its odds (CS2's own)
        c.text(left[0] + r, yy, 0.74 * r, Face::Semi, tr("Odds"), pal().t2);
        yy += 0.9 * r;
        for k in (0..RARITIES.len()).rev() {
            if yy > left[3] - 0.6 * r {
                break;
            }
            let rarity = k as u8;
            if rarity < SPECIAL && case.of(rarity).next().is_none() {
                continue;
            }
            let d = 0.22 * r;
            c.rect([left[0] + r, yy - 0.25 * r - d, left[0] + r + 2.0 * d, yy - 0.25 * r + d], d, hex(colour(rarity), 1.0));
            let name = fonts().fit(Face::Regular, 0.66 * r, tr(rarity_label(rarity)), lw - 6.0 * r);
            c.text(left[0] + r + 0.8 * r, yy, 0.66 * r, Face::Regular, &name, pal().t3);
            c.text_r(left[2] - r, yy, 0.66 * r, Face::Mono, &format!("{:.2}%", ODDS[k] * 100.0), pal().t2);
            yy += 1.1 * r;
        }
        // at the right: its contents or the history, by the tabs
        let right = [left[2] + 0.8 * r, y, body[2], body[3]];
        let mut tx = right[0];
        let mut rects = Vec::new();
        for label in ["Contents", "History"] {
            let w = fonts().width(Face::Medium, 0.74 * r, tr(label)) + 1.4 * r;
            rects.push([tx, right[1], tx + w, right[1] + 1.9 * r]);
            tx += w + 0.45 * r;
        }
        for (k, b) in rects.iter().enumerate() {
            let hov = self.an(ak(A_HOV, 37_420 + k), if self.hover == Hit::Cosm(CASE_TAB, k as u16) { 1.0 } else { 0.0 }, 14.0, 0.0);
            c.rect(*b, 0.95 * r, mix(pal().f1, pal().f2, hov));
        }
        let tab = self.cosm.case_tab.min(1) as usize;
        self.cosm_pill(c, &rects, tab, 37_430, 0.95 * r, pal().f3);
        for (k, (label, b)) in ["Contents", "History"].iter().zip(rects.iter()).enumerate() {
            let hit = Hit::Cosm(CASE_TAB, k as u16);
            let hv = self.an(ak(A_HOV, 37_440 + k), if tab == k { 1.0 } else if self.hover == hit { 0.45 } else { 0.0 }, 14.0, 0.0);
            c.text(b[0] + 0.7 * r, b[1] + 1.28 * r, 0.74 * r, Face::Medium, tr(label), mix(pal().t3, pal().t1, hv));
            self.hits.push((*b, hit));
        }
        let under = [right[0], right[1] + 2.6 * r, right[2], right[3]];
        if tab == 1 {
            self.case_history(c, under);
        } else {
            self.case_contents(c, under, case);
        }
    }

    /// A case's contents as cards (its skins, the rarest first; the gold card of its rare special items — pressed,
    /// them instead).
    fn case_contents(&mut self, c: &mut Canvas, area: [f32; 4], case: &Case) {
        let r = self.r;
        let rares = self.cosm.case_rares;
        let mut cards: Vec<Option<&Drop>> = if rares {
            case.of(SPECIAL).map(Some).collect()
        } else {
            let mut v: Vec<&Drop> = case.drops.iter().filter(|d| d.rarity < SPECIAL).collect();
            v.sort_by(|a, b| b.rarity.cmp(&a.rarity));
            v.into_iter().map(Some).collect()
        };
        // the gold card: last among the skins, first among the rare items (the way back to the skins)
        if rares {
            cards.insert(0, None);
        } else {
            cards.push(None);
        }
        let gap = 0.55 * r;
        let cols = (((area[2] - area[0]) + gap) / (7.6 * r + gap)).floor().max(1.0) as usize;
        let cw = ((area[2] - area[0]) - gap * (cols as f32 - 1.0)) / cols as f32;
        let ih = (cw - 0.8 * r) * 0.75;
        let ch = 0.3 * r + ih + 2.5 * r;
        let rows = cards.len().div_ceil(cols);
        self.cosm.case_area = area;
        self.cosm.case_content = (rows as f32 * (ch + gap) - gap - (area[3] - area[1])).max(0.0);
        self.cosm.case_scroll_to = self.cosm.case_scroll_to.clamp(0.0, self.cosm.case_content);
        self.cosm.case_scroll += (self.cosm.case_scroll_to - self.cosm.case_scroll) * (1.0 - (-self.dt * 14.0).exp());
        c.push_clip(area);
        let a0 = c.alpha;
        for (n, card) in cards.iter().enumerate() {
            let (cx, cy) = (n % cols, n / cols);
            let x0 = area[0] + cx as f32 * (cw + gap);
            let y0 = area[1] + cy as f32 * (ch + gap) - self.cosm.case_scroll;
            if y0 > area[3] || y0 + ch < area[1] {
                continue;
            }
            let (ca, cdy) = self.rise_c(0.2, 0.018 * n.min(24) as f32, false);
            c.alpha = a0 * ca;
            let rr = [x0, y0 + cdy, x0 + cw, y0 + cdy + ch];
            let ir = [rr[0] + 0.4 * r, rr[1] + 0.3 * r, rr[2] - 0.4 * r, rr[1] + 0.3 * r + ih];
            match card {
                Some(d) => {
                    let hv = self.an(ak(A_HOV, 37_500 + n), if self.hover == Hit::Cosm(CASE_RARE, 1000 + n as u16) { 1.0 } else { 0.0 }, 14.0, 0.0);
                    let col = colour(d.rarity);
                    c.rect(rr, 0.7 * r, mix(pal().f1, pal().f2, hv));
                    // its rarity's light at its foot
                    c.glow([(ir[0] + ir[2]) * 0.5, ir[3]], (ir[2] - ir[0]) * 0.5, hex(col, 0.1));
                    if let Some((uv, [iw, ih2], seen)) = self.cosm_icon(&d.weapon, &d.paint) {
                        let k = ((ir[2] - ir[0]) / iw as f32).min((ir[3] - ir[1]) / ih2 as f32) * (0.96 + 0.04 * seen);
                        let (w, h) = (iw as f32 * k, ih2 as f32 * k);
                        let m = [(ir[0] + ir[2]) / 2.0, (ir[1] + ir[3]) / 2.0];
                        c.pic([m[0] - w / 2.0, m[1] - h / 2.0, m[0] + w / 2.0, m[1] + h / 2.0], uv, seen, false, 1.0, 1.0, 0.0);
                    }
                    let (a, b) = two_lines(drop_name(d));
                    c.text(rr[0] + 0.6 * r, ir[3] + 0.95 * r, 0.6 * r, Face::Regular, &fonts().fit(Face::Regular, 0.6 * r, &a, cw - 1.2 * r), pal().t4);
                    c.text(rr[0] + 0.6 * r, ir[3] + 1.75 * r, 0.68 * r, Face::Semi, &fonts().fit(Face::Semi, 0.68 * r, &b, cw - 1.2 * r), pal().t1);
                    c.rect([rr[0] + 0.6 * r, rr[3] - 0.36 * r, rr[2] - 0.6 * r, rr[3] - 0.22 * r], 0.07 * r, hex(col, 0.95));
                    self.hits.push(([rr[0], rr[1].max(area[1]), rr[2], rr[3].min(area[3])], Hit::Cosm(CASE_RARE, 1000 + n as u16)));
                }
                None => {
                    // the gold card: CS2's own picture of the case's rare item, or a star
                    let hit = Hit::Cosm(CASE_RARE, 0);
                    let hv = self.an(ak(A_HOV, 37_490), if self.hover == hit { 1.0 } else { 0.0 }, 14.0, 0.0);
                    c.rect(rr, 0.7 * r, mix([0.894, 0.682, 0.224, 0.1], [0.894, 0.682, 0.224, 0.18], hv));
                    c.rect_line(rr, 0.7 * r, 1.2, [0.894, 0.682, 0.224, 0.35 + 0.3 * hv]);
                    match self.cosm_pic(&format!("cases/{}_rare.rtx", case.name), 256).filter(|_| !rares) {
                        Some((uv, [iw, ih2], seen)) => {
                            let k = ((ir[2] - ir[0]) / iw as f32).min((ir[3] - ir[1]) / ih2 as f32);
                            let (w, h) = (iw as f32 * k, ih2 as f32 * k);
                            let m = [(ir[0] + ir[2]) / 2.0, (ir[1] + ir[3]) / 2.0];
                            c.pic([m[0] - w / 2.0, m[1] - h / 2.0, m[0] + w / 2.0, m[1] + h / 2.0], uv, seen, false, 1.0, 1.0, 0.0);
                        }
                        None => {
                            let m = [(ir[0] + ir[2]) / 2.0, (ir[1] + ir[3]) / 2.0 + 0.6 * r];
                            c.text_c(m[0], m[1], 2.4 * r, Face::Bold, if rares { "←" } else { "★" }, [0.894, 0.682, 0.224, 0.9]);
                        }
                    }
                    let line = if rares { tr("The skins") } else { tr("Rare Special Item") };
                    c.text(rr[0] + 0.6 * r, ir[3] + 1.35 * r, 0.68 * r, Face::Semi, &fonts().fit(Face::Semi, 0.68 * r, line, cw - 1.2 * r), [0.894, 0.682, 0.224, 1.0]);
                    self.hits.push(([rr[0], rr[1].max(area[1]), rr[2], rr[3].min(area[3])], hit));
                }
            }
        }
        c.alpha = a0;
        c.pop_clip();
    }

    /// What the cases gave, the newest first: its icon, its name, its wear, StatTrak™, the case; what can go on, a
    /// press puts on (what is on, a tick).
    fn case_history(&mut self, c: &mut Canvas, area: [f32; 4]) {
        let r = self.r;
        let all = cases::history();
        if all.is_empty() {
            c.text_c((area[0] + area[2]) * 0.5, area[1] + 3.0 * r, 0.74 * r, Face::Medium, tr("Nothing yet: open a case"), pal().t3);
            return;
        }
        let list = cases::get();
        let (lo, _) = loadout::current();
        let rh = 3.2 * r;
        let gap = 0.35 * r;
        self.cosm.case_area = area;
        self.cosm.case_content = (all.len() as f32 * (rh + gap) - gap - (area[3] - area[1])).max(0.0);
        self.cosm.case_scroll_to = self.cosm.case_scroll_to.clamp(0.0, self.cosm.case_content);
        self.cosm.case_scroll += (self.cosm.case_scroll_to - self.cosm.case_scroll) * (1.0 - (-self.dt * 14.0).exp());
        c.push_clip(area);
        for (n, w) in all.iter().rev().enumerate() {
            let y0 = area[1] + n as f32 * (rh + gap) - self.cosm.case_scroll;
            if y0 > area[3] || y0 + rh < area[1] {
                continue;
            }
            let b = [area[0], y0, area[2], y0 + rh];
            let can = slot_of(&w.weapon).is_some();
            let hit = Hit::Cosm(CASE_HIST, n as u16);
            let hv = self.an(ak(A_HOV, 37_600 + n % 4096), if can && self.hover == hit { 1.0 } else { 0.0 }, 14.0, 0.0);
            c.rect(b, 0.7 * r, mix(pal().f1, pal().f2, hv));
            let col = colour(w.rarity);
            c.rect([b[0] + 0.3 * r, b[1] + 0.5 * r, b[0] + 0.45 * r, b[3] - 0.5 * r], 0.07 * r, hex(col, 0.95));
            let ir = [b[0] + 0.8 * r, b[1] + 0.25 * r, b[0] + 0.8 * r + (rh - 0.5 * r) * 4.0 / 3.0, b[3] - 0.25 * r];
            if let Some((uv, [iw, ih], seen)) = self.cosm_icon(&w.weapon, &w.paint) {
                let k = ((ir[2] - ir[0]) / iw as f32).min((ir[3] - ir[1]) / ih as f32);
                let (pw, ph) = (iw as f32 * k, ih as f32 * k);
                let m = [(ir[0] + ir[2]) / 2.0, (ir[1] + ir[3]) / 2.0];
                c.pic([m[0] - pw / 2.0, m[1] - ph / 2.0, m[0] + pw / 2.0, m[1] + ph / 2.0], uv, seen, false, 1.0, 1.0, 0.0);
            }
            let name = list.case(&w.case).and_then(|cs| cs.drops.iter().find(|d| d.weapon == w.weapon && d.paint == w.paint)).map_or(w.weapon.clone(), |d| drop_name(d).to_string());
            let name = if w.stattrak { format!("StatTrak™ {name}") } else { name };
            let tx = ir[2] + 0.6 * r;
            c.text(tx, b[1] + 1.35 * r, 0.72 * r, Face::Semi, &fonts().fit(Face::Semi, 0.72 * r, &name, b[2] - tx - 7.0 * r), pal().t1);
            let case = list.case(&w.case).map_or(w.case.clone(), |cs| case_name(cs).to_string());
            let second = format!("{} · {:.4} · {}", tr(super::cosmui::wear_name(w.wear)), w.wear, case);
            c.text(tx, b[1] + 2.35 * r, 0.6 * r, Face::Regular, &fonts().fit(Face::Regular, 0.6 * r, &second, b[2] - tx - 7.0 * r), pal().t4);
            // at its right: on, to put on, or kept only
            let on = slot_of(&w.weapon).and_then(|s| lo.get(s)).is_some_and(|e| e.item == w.weapon && e.paint == w.paint && e.seed == w.seed && (e.wear - w.wear).abs() < 1e-5);
            let label = if on {
                tr("Equipped")
            } else if can {
                tr("Put on")
            } else {
                tr("In the collection")
            };
            c.text_r(b[2] - 0.9 * r, (b[1] + b[3]) * 0.5 + 0.25 * r, 0.66 * r, Face::Medium, label, if can { mix(pal().t3, pal().t1, hv.max(on as i32 as f32)) } else { pal().t4 });
            if can {
                self.hits.push(([b[0], b[1].max(area[1]), b[2], b[3].min(area[3])], hit));
            }
        }
        c.pop_clip();
    }

    /// An opening: the strip running under the marker and slowing to what it gave (a tick as each card goes by), then
    /// what it gave large in its rarity's light, and what to do with it.
    fn case_spin(&mut self, c: &mut Canvas, body: [f32; 4], case: &Case, mut sp: Spin) {
        let r = self.r;
        let title = format!("{} · {}", tr("Opening"), case_name(case));
        let y = self.case_head(c, body, &title);
        let now = self.now;
        let p = ((now - sp.t0) / RUN).clamp(0.0, 1.0);
        // (a long slow end, as CS2's)
        let e = 1.0 - (1.0 - p).powf(4.2);
        let cw = 7.4 * r;
        let gap = 0.3 * r;
        let step = cw + gap;
        let ch = 7.8 * r;
        let band = [body[0], y + 1.5 * r, body[2], y + 1.5 * r + ch];
        let mid = ((band[0] + band[2]) * 0.5).floor();
        let start = 2.5 * step;
        let target = WIN as f32 * step + sp.land * cw;
        let pos = start + (target - start) * e;
        // coming in before it runs: the strip from the dark
        let come = super::smooth(sp.t0 - LEAD, sp.t0, now);
        let shown = sp.shown_at.map_or(0.0, |t| super::ease_out((now - t) / 0.35));
        let a0 = c.alpha;
        c.alpha = a0 * come * (1.0 - 0.85 * shown);
        c.rect(band, 0.8 * r, grey(0.0, 0.35));
        c.push_clip(band);
        for (i, d) in sp.cards.iter().enumerate() {
            let x0 = mid - pos + i as f32 * step;
            if x0 > band[2] || x0 + cw < band[0] {
                continue;
            }
            let col = colour(d.rarity);
            let rr = [x0, band[1] + 0.35 * r, x0 + cw, band[3] - 0.35 * r];
            c.rect(rr, 0.5 * r, grey(0.09, 1.0));
            // its rarity rising from its foot
            c.rect_lit([rr[0], rr[1] + (rr[3] - rr[1]) * 0.45, rr[2], rr[3]], 0.5 * r, hex(col, 0.0), hex(col, 0.22), 6.0);
            c.rect([rr[0], rr[3] - 0.3 * r, rr[2], rr[3]], 0.15 * r, hex(col, 1.0));
            if let Some((uv, [iw, ih], seen)) = self.cosm_icon(&d.weapon, &d.paint) {
                let ir = [rr[0] + 0.4 * r, rr[1] + 0.3 * r, rr[2] - 0.4 * r, rr[3] - 1.9 * r];
                let k = ((ir[2] - ir[0]) / iw as f32).min((ir[3] - ir[1]) / ih as f32);
                let (w, h) = (iw as f32 * k, ih as f32 * k);
                let m = [(ir[0] + ir[2]) / 2.0, (ir[1] + ir[3]) / 2.0];
                c.pic([m[0] - w / 2.0, m[1] - h / 2.0, m[0] + w / 2.0, m[1] + h / 2.0], uv, seen, false, 1.0, 1.0, 0.0);
            }
            let (_, b) = two_lines(drop_name(d));
            let b = if b.is_empty() { drop_name(d).to_string() } else { b };
            c.text_c((rr[0] + rr[2]) * 0.5, rr[3] - 0.8 * r, 0.58 * r, Face::Medium, &fonts().fit(Face::Medium, 0.58 * r, &b, cw - 0.6 * r), pal().t2);
        }
        // the ends darker, the marker in the middle
        c.veil([band[0], band[1], band[0] + 0.22 * (band[2] - band[0]), band[3]], grey(0.0, 0.6), 1.0, 1.0);
        c.veil([band[2] - 0.22 * (band[2] - band[0]), band[1], band[2], band[3]], grey(0.0, 0.6), 1.0, 2.0);
        c.pop_clip();
        let gold = [0.894, 0.682, 0.224, 1.0 - shown];
        c.rect([mid - 1.0, band[1] - 0.2 * r, mid + 1.0, band[3] + 0.2 * r], 0.0, gold);
        c.polyline(&[[mid - 0.45 * r, band[1] - 0.55 * r], [mid, band[1] - 0.05 * r], [mid + 0.45 * r, band[1] - 0.55 * r]], 2.0, gold);
        c.polyline(&[[mid - 0.45 * r, band[3] + 0.55 * r], [mid, band[3] + 0.05 * r], [mid + 0.45 * r, band[3] + 0.55 * r]], 2.0, gold);
        if p < 1.0 && come >= 1.0 {
            c.text_c(mid, band[3] + 1.8 * r, 0.62 * r, Face::Regular, tr("Press on it to see at once"), pal().t4);
        }
        self.hits.push((band, Hit::Cosm(CASE_SKIP, 0)));
        c.alpha = a0;
        // a tick as each card goes under the marker
        let under = (pos / step).floor() as i64;
        if now >= sp.t0 && under != sp.last {
            sp.last = under;
            sound::play("UI.CrateScroll");
        }
        // stopped: what it gave, once
        if p >= 1.0 && sp.shown_at.is_none() {
            sp.shown_at = Some(now);
            let rr = RARITIES[sp.won.rarity.min(3) as usize];
            sound::play(&format!("UI.CrateReveal.{rr}"));
            if sp.won.rarity >= SPECIAL {
                sound::play("UI.CrateKnife");
            }
        }
        if let Some(t) = sp.shown_at {
            self.case_won(c, [body[0], band[1], body[2], body[3]], case, &sp, super::ease_out((now - t) / 0.4));
        }
        self.cosm.opening = Some(sp);
    }

    /// What an opening gave, large in its rarity's light (`k` how far it has come up), and what to do with it: put
    /// it on, inspect it, open another, back to the case.
    fn case_won(&mut self, c: &mut Canvas, area: [f32; 4], case: &Case, sp: &Spin, k: f32) {
        let r = self.r;
        let w = &sp.won;
        let col = colour(w.rarity);
        let a0 = c.alpha;
        c.alpha = a0 * k;
        let m = [(area[0] + area[2]) * 0.5, area[1] + 5.5 * r - (1.0 - k) * 1.2 * r];
        let glow = if pal().anim { 0.85 + 0.15 * (self.now * 2.0).sin() } else { 1.0 };
        c.glow(m, 12.0 * r, hex(col, 0.22 * glow));
        c.glow(m, 6.0 * r, hex(col, 0.18 * glow));
        let size = (11.0 + 1.0 * k) * r;
        if let Some((uv, [iw, ih], seen)) = self.cosm_icon(&w.weapon, &w.paint) {
            let s = (size / iw as f32).min(size * 0.75 / ih as f32);
            let (pw, ph) = (iw as f32 * s, ih as f32 * s);
            c.pic([m[0] - pw / 2.0, m[1] - ph / 2.0, m[0] + pw / 2.0, m[1] + ph / 2.0], uv, seen, false, 1.0, 1.0, 0.0);
        }
        let d = case.drops.iter().find(|d| d.weapon == w.weapon && d.paint == w.paint);
        let name = d.map_or(w.weapon.clone(), |d| drop_name(d).to_string());
        let name = if w.stattrak { format!("StatTrak™ {name}") } else { name };
        let mut y = m[1] + size * 0.42 + 1.2 * r;
        c.text_c(m[0], y, 1.05 * r, Face::Bold, &name, pal().t1);
        y += 1.2 * r;
        c.rect([m[0] - 2.0 * r, y - 0.35 * r, m[0] + 2.0 * r, y - 0.2 * r], 0.07 * r, hex(col, 1.0));
        y += 0.9 * r;
        let line = format!("{} · {} {:.6} · {} {}", tr(rarity_label(w.rarity)), tr("Wear"), w.wear, tr("Pattern"), w.seed);
        c.text_c(m[0], y, 0.66 * r, Face::Regular, &line, pal().t3);
        let wn = tr(super::cosmui::wear_name(w.wear));
        c.text_c(m[0], y + 0.95 * r, 0.66 * r, Face::Medium, wn, pal().t2);
        y += 2.2 * r;
        // what to do with it
        let can = slot_of(&w.weapon).is_some();
        let acts: Vec<(u16, String)> = [
            (0, if sp.on { tr("Equipped") } else { tr("Put on") }),
            (1, tr("Inspect")),
            (2, tr("Open again")),
            (3, tr("To the case")),
        ]
        .into_iter()
        .filter(|(n, _)| can || *n >= 2)
        .map(|(n, s)| (n, s.to_string()))
        .collect();
        let ws: Vec<f32> = acts.iter().map(|(_, s)| fonts().width(Face::Medium, 0.74 * r, s) + 2.0 * r).collect();
        let total = ws.iter().sum::<f32>() + 0.5 * r * (ws.len() as f32 - 1.0);
        let mut x = m[0] - total * 0.5;
        for ((n, label), bw) in acts.iter().zip(ws) {
            let b = [x, y, x + bw, y + 2.1 * r];
            let hit = Hit::Cosm(CASE_ACT, *n);
            let hv = self.an(ak(A_HOV, 37_700 + *n as usize), if self.hover == hit { 1.0 } else { 0.0 }, 14.0, 0.0);
            let main = *n == 0 || (!can && *n == 2);
            if main {
                c.rect(b, 1.05 * r, grey(0.86 + 0.14 * hv, 1.0));
                c.text_c((b[0] + b[2]) * 0.5, b[3] - 0.72 * r, 0.74 * r, Face::Semi, label, grey(0.06, 1.0));
            } else {
                c.rect(b, 1.05 * r, mix(pal().f1, pal().f3, hv));
                c.text_c((b[0] + b[2]) * 0.5, b[3] - 0.72 * r, 0.74 * r, Face::Medium, label, mix(pal().t2, pal().t1, hv));
            }
            if k > 0.5 {
                self.hits.push((b, hit));
            }
            x = b[2] + 0.5 * r;
        }
        if !can {
            c.text_c(m[0], y + 3.2 * r, 0.62 * r, Face::Regular, tr("Kept in the collection: only knives, gloves, the AWP and the SSG 08 go into the hands"), pal().t4);
        }
        c.alpha = a0;
    }

    /// An opening begins: what it gives rolled and kept at once (as CS2 gives it), the strip laid out.
    fn case_start(&mut self, name: &str) {
        let all = cases::get();
        let Some(case) = all.case(name) else { return };
        let mut rng = Rng::now();
        let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let Some(mut won) = cases::roll(case, &mut rng, at) else { return };
        // (the stand's CURSED_DEV_CASE_WIN="weapon:paint": what it gives, when the case has it)
        if let Some((wp, pp)) = std::env::var("CURSED_DEV_CASE_WIN").ok().and_then(|v| v.split_once(':').map(|(a, b)| (a.to_string(), b.to_string()))) {
            if let Some(d) = case.drops.iter().find(|d| d.weapon == wp && d.paint == pp) {
                (won.weapon, won.paint, won.rarity, won.stattrak) = (d.weapon.clone(), d.paint.clone(), d.rarity, false);
            }
        }
        cases::record(&won);
        let cards: Vec<Drop> = cases::strip(case, &won, CARDS, WIN, &mut rng).into_iter().cloned().collect();
        let land = 0.15 + 0.7 * rng.unit() as f32;
        self.cosm.opening = Some(Spin { case: name.to_string(), cards, won, t0: self.now + LEAD, land, last: -1, shown_at: None, on: false });
        sound::play("UI.CrateUnlock");
        self.cosm_swap(true);
    }

    /// What was won goes on (its slot; CS's things in the hands on with it).
    fn case_equip(&mut self, w: &Won) {
        let Some(slot) = slot_of(&w.weapon) else { return };
        let e = equip_of(w);
        loadout::change(|l| {
            l.set(slot, Some(e));
            l.on = true;
        });
        self.switch_sound(true, 0.6);
        let name = self.case_won_name(w);
        self.toast(trn("{name} is on", &[("name", name)]));
    }

    fn case_won_name(&self, w: &Won) -> String {
        cases::get().case(&w.case).and_then(|c| c.drops.iter().find(|d| d.weapon == w.weapon && d.paint == w.paint)).map_or(w.weapon.clone(), |d| drop_name(d).to_string())
    }

    /// A press on the cases' parts.
    pub(super) fn case_press(&mut self, part: u8, n: u16) {
        let all = cases::get();
        match part {
            CASE_PICK => {
                if let Some(case) = all.cases.iter().rev().nth(n as usize) {
                    self.cosm_swap(true);
                    self.cosm.case = Some(case.name.clone());
                    self.cosm.case_rares = false;
                    self.cosm.case_scroll = 0.0;
                    self.cosm.case_scroll_to = 0.0;
                    sound::play("UI.CrateDrop");
                    self.sound_tick(0.6);
                }
            }
            CASE_BACK => {
                self.cosm_swap(true);
                if self.cosm.opening.take().is_none() {
                    self.cosm.case = None;
                }
                self.cosm.case_scroll = 0.0;
                self.cosm.case_scroll_to = 0.0;
                self.sound_tick(0.5);
            }
            CASE_OPEN => {
                if let Some(name) = self.cosm.case.clone() {
                    self.case_start(&name);
                }
            }
            CASE_RARE => {
                if n == 0 {
                    self.cosm.case_rares = !self.cosm.case_rares;
                    self.cosm.case_scroll_to = 0.0;
                    self.cosm_swap(true);
                    self.sound_tick(0.5);
                }
            }
            CASE_TAB => {
                if self.cosm.case_tab != n as u8 {
                    self.cosm.case_tab = n as u8;
                    self.cosm.case_scroll = 0.0;
                    self.cosm.case_scroll_to = 0.0;
                    self.cosm_swap(true);
                    self.sound_tick(0.5);
                }
            }
            CASE_SKIP => {
                if let Some(sp) = self.cosm.opening.as_mut() {
                    if self.now < sp.t0 + RUN && self.now >= sp.t0 {
                        sp.t0 = self.now - RUN;
                    }
                }
            }
            CASE_ACT => {
                let Some(sp) = self.cosm.opening.clone() else { return };
                match n {
                    0 => {
                        self.case_equip(&sp.won);
                        if let Some(s) = self.cosm.opening.as_mut() {
                            s.on = true;
                        }
                    }
                    1 => {
                        let e = equip_of(&sp.won);
                        self.cosm.opening = None;
                        self.cosm_open(&e);
                    }
                    2 => self.case_start(&sp.case),
                    _ => {
                        self.cosm_swap(true);
                        self.cosm.opening = None;
                        self.sound_tick(0.5);
                    }
                }
            }
            CASE_HIST => {
                let all = cases::history();
                if let Some(w) = all.iter().rev().nth(n as usize) {
                    self.case_equip(&w.clone());
                }
            }
            _ => {}
        }
    }

    /// The wheel over the cases' list, their contents or the history: true when it scrolled them.
    pub(super) fn case_wheel(&mut self, dy: f32) -> bool {
        let [x, y] = self.mouse;
        let a = self.cosm.case_area;
        let on = self.cosm.look.is_none() && self.cosm.item.is_none() && self.cosm.cat == super::cosmui::CASES && self.cosm.opening.is_none();
        if !(on && x >= a[0] && x <= a[2] && y >= a[1] && y <= a[3]) {
            return false;
        }
        self.cosm.case_scroll_to = (self.cosm.case_scroll_to - dy * 3.2 * self.r).clamp(0.0, self.cosm.case_content);
        true
    }
}
