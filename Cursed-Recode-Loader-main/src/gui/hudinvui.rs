//! Custom Inventory's screens (the inventory, the containers, creative) as one likes them, from the screen itself (her
//! word, 18.10): each block taken by its header (its title, or its top edge where it has none; a small handle shows in
//! its middle while pointed at) and put where one likes, kept with the HUD's places; a second click on a header soon
//! after puts that block back. The right button on a header, or on a block's empty place, opens the settings of it all
//! beside that block — live, as an element's of the HUD; "All blocks back" is among them. The menu's rows, lists and
//! colour wheel are the menu's code, with the panel's own hits, pointer, hover and unit swapped in while it is drawn
//! and pressed (as the HUD does).

use super::*;
use crate::gui::{Hit, Owner};

/// The panel's and the handles' animations.
const A_IU: u64 = 52;

/// The screens' own state between frames.
pub(super) struct InvUi {
    /// the settings' panel: open by the box of the block it was opened from (window px)
    pub open: Option<[f32; 4]>,
    pub at: f32,
    pub pin: Option<[f32; 2]>,
    pub rect: [f32; 4],
    pub scroll: f32,
    pub to: f32,
    /// the menu code's hits, pointer, hover and unit while the panel is (swapped in for the menu's)
    pub hits: Vec<([f32; 4], Hit)>,
    pub mouse: [f32; 2],
    pub hover: Hit,
    pub r: f32,
    /// the buttons last frame; the screen's seconds since it opened, last frame (fewer now: another screen)
    pub was: u32,
    pub since: f32,
    /// a block held by its header: which, where the pointer took it, its offset then
    pub drag: Option<(&'static str, [f32; 2], [f32; 2])>,
    /// the last press on a header: which, when (a second soon after puts it back)
    pub last: Option<(&'static str, f32)>,
    /// what a press takes next frame (worked out over this frame's layout): the left button a header's block, the
    /// right one the settings by a block's box
    pub head_at: Option<&'static str>,
    pub right_at: Option<[f32; 4]>,
    /// the guides the held block snapped to (vertical, where)
    pub guides: Vec<(bool, f32)>,
    /// all the blocks together, this frame (the panel keeps beside them)
    pub all: [f32; 4],
    /// when a screen of Custom Inventory was last drawn (none since a while: it is shut, what it had open goes); a list or
    /// the colour wheel open from the panel; the panel's size (its place before it is drawn: whose a click is)
    pub seen: f32,
    pub lists: bool,
    pub size: [f32; 2],
    /// a setting changed: saved once nothing is held
    pub dirty: bool,
}

impl Default for InvUi {
    fn default() -> InvUi {
        InvUi {
            open: None,
            at: 0.0,
            pin: None,
            rect: [0.0; 4],
            scroll: 0.0,
            to: 0.0,
            hits: Vec::new(),
            mouse: [0.0; 2],
            hover: Hit::None,
            r: 16.0,
            was: 0,
            since: f32::MAX,
            drag: None,
            last: None,
            head_at: None,
            right_at: None,
            guides: Vec::new(),
            all: [0.0; 4],
            seen: -9.0,
            lists: false,
            size: [0.0; 2],
            dirty: false,
        }
    }
}

/// The buttons this frame.
#[derive(Clone, Copy, Default)]
pub(super) struct Edges {
    pub pressed: bool,
    pub rpressed: bool,
    pub held: bool,
}

/// What the screen tells Java its clicks are: every button's (something held, the panel or its list under the
/// pointer), the left one's (a header), the right one's (a header, a block's empty place).
#[derive(Clone, Copy, Default)]
pub(super) struct Eats {
    pub all: bool,
    pub left: bool,
    pub right: bool,
    /// the panel (or its list, its wheel) open: Esc closes it, not the screen
    pub open: bool,
    /// the colour wheel's hex being typed: every key and letter is the panel's
    pub keys: bool,
}

impl Eats {
    pub fn flat(&self) -> [f32; 5] {
        [self.all as i32 as f32, self.left as i32 as f32, self.right as i32 as f32, self.open as i32 as f32, self.keys as i32 as f32]
    }
}

/// A block's header: its title's height (0: none) under its top edge, or the room over its first cells; a little above
/// its edge too.
pub(super) fn header(r: [f32; 4], title: f32, pad: f32, u: f32) -> [f32; 4] {
    [r[0], r[1] - 6.0 * u, r[2], r[1] + title.max(pad - 2.0 * u).max(0.0)]
}

fn inside4(r: [f32; 4], p: [f32; 2]) -> bool {
    p[0] >= r[0] && p[0] < r[2] && p[1] >= r[1] && p[1] < r[3]
}

/// Where a block goes as it is held: `b0` its box with no offset, `o0` its offset (design px) when it was taken at
/// `m0`, the pointer at `m` now; kept on the screen (`w` × `h`), its edges drawn to the others' (or `gap` from them) and
/// to the screen's middle. Its offset now, the guides it snapped to.
#[allow(clippy::too_many_arguments)]
pub(super) fn held_to(b0: [f32; 4], o0: [f32; 2], m0: [f32; 2], m: [f32; 2], u: f32, w: f32, h: f32, others: &[[f32; 4]], gap: f32) -> ([f32; 2], Vec<(bool, f32)>) {
    let keep = |o: [f32; 2]| -> [f32; 4] {
        let mut d = [o[0] * u, o[1] * u];
        d[0] = d[0].clamp(4.0 - b0[0], (w - 4.0 - b0[2]).max(4.0 - b0[0]));
        d[1] = d[1].clamp(4.0 - b0[1], (h - 4.0 - b0[3]).max(4.0 - b0[1]));
        [b0[0] + d[0], b0[1] + d[1], b0[2] + d[0], b0[3] + d[1]]
    };
    let mut o = [o0[0] + (m[0] - m0[0]) / u, o0[1] + (m[1] - m0[1]) / u];
    let r = keep(o);
    let snap = 7.0 * u;
    let mut guides = Vec::new();
    let (mut bx, mut by): (Option<(f32, f32)>, Option<(f32, f32)>) = (None, None);
    let mut try_x = |d: f32, at: f32| {
        if d.abs() < snap && bx.is_none_or(|(bd, _)| d.abs() < bd.abs()) {
            bx = Some((d, at));
        }
    };
    try_x(w / 2.0 - (r[0] + r[2]) / 2.0, w / 2.0);
    for q in others {
        for (mine, theirs) in [(r[0], q[0]), (r[2], q[2]), ((r[0] + r[2]) / 2.0, (q[0] + q[2]) / 2.0), (r[0], q[2] + gap), (r[2], q[0] - gap)] {
            try_x(theirs - mine, theirs);
        }
    }
    let mut try_y = |d: f32, at: f32| {
        if d.abs() < snap && by.is_none_or(|(bd, _)| d.abs() < bd.abs()) {
            by = Some((d, at));
        }
    };
    try_y(h / 2.0 - (r[1] + r[3]) / 2.0, h / 2.0);
    for q in others {
        for (mine, theirs) in [(r[1], q[1]), (r[3], q[3]), ((r[1] + r[3]) / 2.0, (q[1] + q[3]) / 2.0), (r[1], q[3] + gap), (r[3], q[1] - gap)] {
            try_y(theirs - mine, theirs);
        }
    }
    if let Some((d, at)) = bx {
        o[0] += d / u;
        guides.push((true, at));
    }
    if let Some((d, at)) = by {
        o[1] += d / u;
        guides.push((false, at));
    }
    let r = keep(o);
    ([(r[0] - b0[0]) / u, (r[1] - b0[1]) / u], guides)
}

impl Gui {
    /// Swaps the panel's hits, pointer, hover and unit in for the menu's (and back).
    fn iu_swap(&mut self) {
        std::mem::swap(&mut self.hits, &mut self.hud.inv.hits);
        std::mem::swap(&mut self.mouse, &mut self.hud.inv.mouse);
        std::mem::swap(&mut self.hover, &mut self.hud.inv.hover);
        std::mem::swap(&mut self.r, &mut self.hud.inv.r);
    }

    /// A screen's frame begins (the buttons Java saw: held 1 the left, 2 the right; pressed since the last frame 4, 8;
    /// `ask` 2: Esc): its buttons' edges; the panel's part of them (its rows, its list, its wheel). Another screen shuts
    /// what was open.
    pub(super) fn iu_begin(&mut self, buttons: u32, since: f32, ask: u8, m: [f32; 2]) -> (Edges, bool) {
        if since < self.hud.inv.since {
            let r = self.hud.inv.r;
            self.hud.inv = InvUi { r, ..InvUi::default() };
            self.pick = None;
            self.pop = None;
            self.slider = None;
        }
        self.hud.inv.since = since;
        self.hud.inv.seen = self.now;
        let was = self.hud.inv.was;
        let e = Edges {
            pressed: buttons & 4 != 0 || buttons & 1 != 0 && was & 1 == 0,
            rpressed: buttons & 8 != 0 || buttons & 2 != 0 && was & 2 == 0,
            held: buttons & 1 != 0,
        };
        // (pressed and let go between two frames: let go too, after the press is taken)
        let released = buttons & 1 == 0 && (was & 1 != 0 || e.pressed);
        self.hud.inv.was = buttons & 3;
        // Esc: the list or the wheel, then the panel
        if ask == 2 && (self.pick.take().is_some() || self.pop.take().is_some() || self.hud.inv.open.take().is_some()) {
            self.iu_save_if();
        }
        let mut took = false;
        if self.hud.inv.open.is_some() {
            let mi = self.mod_index("custom-inventory");
            self.iu_swap();
            self.mouse = m;
            if let Some((o, si)) = self.slider {
                self.slide(o, si, m[0]);
                self.hud.inv.dirty = true;
            }
            if let Some(part) = self.pick.as_ref().map(|p| p.drag).filter(|d| *d > 0) {
                self.pick_at(part, m[0], m[1]);
                self.hud.inv.dirty = true;
            }
            self.hover = self.hit_at(m);
            if e.pressed || e.rpressed {
                let button = if e.pressed { 0 } else { 1 };
                let h = self.hit_at(m);
                let had_list = self.pick.is_some() || self.pop.is_some();
                if self.press_common(h, m[0], m[1], button) {
                    took = true;
                    self.hud.inv.dirty = true;
                } else if button == 0 && self.press_rows(h, m[0], m[1], Owner::Module(mi), 0) {
                    took = true;
                    self.hud.inv.dirty = true;
                } else {
                    match (button, h) {
                        (0, Hit::PanelSwitch(i)) => {
                            self.toggle(i);
                            self.hud.inv.dirty = true;
                            took = true;
                        }
                        (0, Hit::PanelClose) => {
                            self.hud.inv.open = None;
                            took = true;
                        }
                        (_, Hit::Panel(_)) => took = true,
                        // the right button in it: nothing (it does not close it)
                        (1, _) if inside4(self.hud.inv.rect, m) => took = true,
                        _ => {
                            // past it: a list open from it closes alone; else the panel goes (the left press is the
                            // screen's; the right one only closes it, whatever it was on)
                            if had_list || self.pick.is_some() || self.pop.is_some() {
                                self.pick = None;
                                self.pop = None;
                                took = true;
                            } else {
                                self.hud.inv.open = None;
                                took = button == 1;
                            }
                        }
                    }
                }
            }
            if released {
                self.release();
            }
            self.hud.inv.lists = self.pick.is_some() || self.pop.is_some();
            self.iu_swap();
        }
        if !e.held {
            self.iu_save_if();
        }
        (e, took)
    }

    fn iu_save_if(&mut self) {
        if self.hud.inv.dirty {
            self.hud.inv.dirty = false;
            self.save();
        }
    }

    /// The headers' part of a frame: `blocks` (id, box with no offset, box now, its header) — a press on a header takes
    /// its block (a second one soon after puts it back), a right press on one (or on a block's empty place, as the last
    /// frame found it) opens the settings by it; the block held follows the pointer. Its new offset is in the places
    /// (and returned, with the block's id, for the caller to lay it out there this frame).
    pub(super) fn iu_blocks(&mut self, e: Edges, took: bool, m: [f32; 2], u: f32, w: f32, h: f32, gap: f32, blocks: &[(&'static str, [f32; 4], [f32; 4], [f32; 4])]) -> Option<(&'static str, [f32; 2])> {
        let t = self.now;
        if !took && e.rpressed {
            if let Some(r) = self.hud.inv.right_at {
                self.iu_open(r);
            }
        }
        if !took && e.pressed {
            if let Some(id) = self.hud.inv.head_at {
                let again = self.hud.inv.last.is_some_and(|(l, at)| l == id && t - at < 0.4);
                if again {
                    // a second click soon after: back to its place
                    self.hud.reset_place(id);
                    self.hud.inv.last = None;
                    self.hud.inv.drag = None;
                    self.save();
                } else {
                    let o = self.hud.places.get(id).map_or([0.0, 0.0], |p| p.1);
                    self.hud.inv.drag = Some((id, m, o));
                    self.hud.inv.last = Some((id, t));
                }
            }
        }
        self.hud.inv.guides.clear();
        let mut moved = None;
        if let Some((id, m0, o0)) = self.hud.inv.drag {
            if e.held {
                if let Some(b) = blocks.iter().find(|b| b.0 == id) {
                    let others: Vec<[f32; 4]> = blocks.iter().filter(|q| q.0 != id).map(|q| q.2).collect();
                    let (o, guides) = held_to(b.1, o0, m0, m, u, w, h, &others, gap);
                    // (a press and no move: nothing changes — a double click is not a drag)
                    if (m[0] - m0[0]).abs() + (m[1] - m0[1]).abs() > 2.0 {
                        self.hud.places.insert(id, (4, o));
                        self.hud.inv.guides = guides;
                        self.hud.inv.last = None;
                        moved = Some((id, o));
                    }
                }
            } else {
                self.hud.inv.drag = None;
                self.save();
            }
        }
        moved
    }

    /// What the pointer is over, for the next frame's press: a header (and its handle shown, lit), a block's empty
    /// place for the right button. Draws the handles and the guides. `empty` says whether the pointer is over nothing of
    /// the game's (a slot, a field, a button) inside a block that may open the settings there.
    pub(super) fn iu_after(&mut self, top: &mut Canvas, m: [f32; 2], u: f32, w: f32, h: f32, accent: Rgba, blocks: &[(&'static str, [f32; 4], [f32; 4], [f32; 4])], empty: Option<[f32; 4]>, busy: bool) -> Eats {
        let mut all = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        for b in blocks {
            all = [all[0].min(b.2[0]), all[1].min(b.2[1]), all[2].max(b.2[2]), all[3].max(b.2[3])];
        }
        if all[2] > all[0] {
            self.hud.inv.all = all;
        }
        // (the panel as drawn last, and where it goes: a click there is its own from its first frame)
        let goes = self.hud.inv.pin.map_or([0.0; 4], |p| [p[0], p[1], p[0] + self.hud.inv.size[0], p[1] + self.hud.inv.size[1]]);
        let over_panel = self.hud.inv.open.is_some() && (inside4(self.hud.inv.rect, m) || inside4(goes, m) || self.pick.is_some() || self.pop.is_some());
        let held = self.hud.inv.drag.map(|d| d.0);
        let head = if over_panel || busy && held.is_none() { None } else { held.or_else(|| blocks.iter().rev().find(|b| inside4(b.3, m)).map(|b| b.0)) };
        self.hud.inv.head_at = head;
        self.hud.inv.right_at = if over_panel {
            None
        } else if let Some(id) = head {
            blocks.iter().find(|b| b.0 == id).map(|b| b.2)
        } else {
            empty
        };
        // the handles: the header pointed at shows its own, the one held stays lit
        for (k, b) in blocks.iter().enumerate() {
            let on = if held == Some(b.0) { 1.0 } else if head == Some(b.0) { 0.7 } else { 0.0 };
            let a = self.an(ak(A_IU, 100 + k), on, 14.0, 0.0);
            if a > 0.01 {
                let cx = (b.2[0] + b.2[2]) / 2.0;
                let y = b.2[1] + 5.0 * u;
                let hw = 14.0 * u + 4.0 * u * a;
                top.rect([cx - hw, y - 1.4 * u, cx + hw, y + 1.4 * u], 1.4 * u, grey(1.0, 0.22 + 0.33 * a));
            }
        }
        for &(vertical, at) in &self.hud.inv.guides {
            let g = if vertical { [at - 0.5 * u, 0.0, at + 0.5 * u, h] } else { [0.0, at - 0.5 * u, w, at + 0.5 * u] };
            top.rect(g, 0.0, fade(accent, 0.45));
        }
        Eats {
            all: held.is_some() || over_panel,
            left: head.is_some(),
            right: self.hud.inv.right_at.is_some(),
            open: self.hud.inv.open.is_some() || self.pick.is_some() || self.pop.is_some(),
            keys: self.iu_keys(),
        }
    }

    /// Whether the panel takes the keys and the letters (the colour wheel's hex being typed in it).
    pub(super) fn iu_keys(&self) -> bool {
        self.iu_live() && self.pick.as_ref().is_some_and(|p| p.hex.is_some())
    }

    /// Whether a screen of Custom Inventory is up with its panel (drawn a moment ago).
    fn iu_live(&self) -> bool {
        self.hud.inv.open.is_some() && self.clock.elapsed().as_secs_f32() - self.hud.inv.seen < 0.3
    }

    /// Each frame of the HUD: a screen of Custom Inventory shut (none drawn for a while), what it had open goes with it
    /// — the panel, a list or the colour wheel open from it (they would come up over the chat, the menu).
    pub(super) fn iu_stale(&mut self, t: f32) {
        if t - self.hud.inv.seen < 0.3 {
            return;
        }
        self.hud.inv.open = None;
        self.hud.inv.drag = None;
        if std::mem::take(&mut self.hud.inv.lists) {
            self.pick = None;
            self.pop = None;
            self.slider = None;
        }
        self.iu_save_if();
    }

    /// The settings opened by a block (closed again by the same).
    fn iu_open(&mut self, by: [f32; 4]) {
        if self.hud.inv.open.is_some() {
            self.hud.inv.open = None;
            return;
        }
        self.hud.inv.open = Some(by);
        self.hud.inv.at = self.now;
        self.hud.inv.to = 0.0;
        self.hud.inv.scroll = 0.0;
        self.hud.inv.pin = None;
        for k in 0..3 {
            self.anim.remove(&ak(A_IU, k));
        }
        self.sound_tick(0.7);
    }

    /// The wheel over the screen: the panel's when it is under the pointer.
    pub(super) fn iu_scroll(&mut self, dy: f32) -> bool {
        if !self.iu_live() || !inside4(self.hud.inv.rect, self.hud.inv.mouse_seen()) {
            return false;
        }
        self.hud.inv.to -= dy * 3.2 * self.hud.inv.r;
        true
    }

    /// The settings' panel beside the screen's blocks (all of them: it covers none where there is room), over the screen
    /// (`w` × `h`): Custom Inventory's name, its switch, close; its rows (scrolled within it); a list or the colour wheel
    /// open from them over it all. It stays where it opened while the blocks keep clear of it, and glides aside once they
    /// would go under it (a block grown by its settings, one put there).
    pub(super) fn iu_panel(&mut self, c: &mut Canvas, w: f32, h: f32) {
        let Some(opened_by) = self.hud.inv.open else {
            self.anim.remove(&ak(A_IU, 0));
            return;
        };
        let i = self.mod_index("custom-inventory");
        let (mw, mh, mscale) = (self.w, self.h, text::scale());
        self.iu_swap();
        self.hits.clear();
        let r = ((w / 1920.0).min(h / 1080.0)).clamp(0.62, 1.6) * 16.0;
        self.r = r;
        self.w = w;
        self.h = h;
        let p = theme::Palette::of(&self.theme, self.now);
        theme::set_pal(p);
        text::set_scale(p.text);
        let k = self.an(ak(A_IU, 0), 1.0, 14.0, 0.0);
        let pw = 25.0 * r;
        let set = self.mods[i].set.clone();
        let body = self.rows_height(&set, pw - 2.4 * r);
        let ph = (4.6 * r + body + 1.2 * r).min(h - 16.0);
        let gap = 0.8 * r;
        let er = if self.hud.inv.all[2] > self.hud.inv.all[0] { self.hud.inv.all } else { opened_by };
        let place = || -> [f32; 2] {
            let x = if er[2] + gap + pw < w - 8.0 { er[2] + gap } else if er[0] - gap - pw > 8.0 { er[0] - gap - pw } else { w - pw - 8.0 };
            [x.clamp(8.0, (w - pw - 8.0).max(8.0)), er[1].clamp(8.0, (h - ph - 8.0).max(8.0))]
        };
        let covers = |p: [f32; 2]| p[0] < er[2] + gap * 0.5 && p[0] + pw > er[0] - gap * 0.5 && p[1] < er[3] && p[1] + ph > er[1];
        // (a slider being dragged, a list or the wheel open: it never moves under the pointer)
        let held = self.slider.is_some() || self.pick.is_some() || self.pop.is_some();
        let pin = match self.hud.inv.pin {
            Some(p) if held || !covers(p) => p,
            _ => place(),
        };
        self.hud.inv.pin = Some(pin);
        let x = self.an(ak(A_IU, 1), pin[0], 16.0, pin[0]);
        let y = self.an(ak(A_IU, 2), pin[1], 16.0, pin[1]);
        let rr = [x, y + (1.0 - k) * 0.5 * r, x + pw, y + ph + (1.0 - k) * 0.5 * r];
        self.hud.inv.rect = rr;
        self.hud.inv.size = [pw, ph];
        c.alpha = k;
        c.shadow(rr, 1.1 * r, 1.4 * r, grey(0.0, 0.5));
        c.rect(rr, 1.1 * r, [p.glass[0].max(0.05), p.glass[1].max(0.05), p.glass[2].max(0.06), 1.0]);
        self.hits.push((rr, Hit::Panel(i)));
        let (x0, x1) = (rr[0] + 1.2 * r, rr[2] - 1.2 * r);
        let m = self.head(i);
        let xb = [x1 - 1.8 * r, rr[1] + 1.0 * r, x1, rr[1] + 2.8 * r];
        let hxv = self.an(ak(A_IU, 3), if self.hover == Hit::PanelClose { 1.0 } else { 0.0 }, 12.0, 0.0);
        c.rect(xb, 0.55 * r, mix(p.f1, p.f3, hxv));
        let xc = [(xb[0] + xb[2]) / 2.0, (xb[1] + xb[3]) / 2.0];
        let xs = 0.28 * r;
        c.line([xc[0] - xs, xc[1] - xs], [xc[0] + xs, xc[1] + xs], 1.5, mix(p.t3, p.t1, hxv), 0.0);
        c.line([xc[0] + xs, xc[1] - xs], [xc[0] - xs, xc[1] + xs], 1.5, mix(p.t3, p.t1, hxv), 0.0);
        self.hits.push((xb, Hit::PanelClose));
        let onv = self.an(ak(crate::gui::A_SW, i), if m.on { 1.0 } else { 0.0 }, 10.0, 0.0);
        let sw = [xb[0] - 0.6 * r - 2.3 * r, rr[1] + 1.22 * r, xb[0] - 0.6 * r, rr[1] + 2.58 * r];
        let hv = self.an(ak(A_IU, 4), if self.hover == Hit::PanelSwitch(i) { 1.0 } else { 0.0 }, 12.0, 0.0);
        switch(c, sw, onv, hv, false);
        self.hits.push((sw, Hit::PanelSwitch(i)));
        let name = fonts().fit(Face::Bold, 1.2 * r, tr(m.name), sw[0] - x0 - 0.6 * r);
        c.text(x0, rr[1] + 2.35 * r, 1.2 * r, Face::Bold, &name, p.t1);
        // the rows, scrolled within it
        let inv = &mut self.hud.inv;
        inv.scroll += (inv.to - inv.scroll) * (1.0 - (-self.dt * 14.0).exp());
        let top = rr[1] + 3.6 * r;
        let clip = [rr[0], top, rr[2], rr[3] - 0.4 * r];
        let max = (body + 1.2 * r - (clip[3] - clip[1])).max(0.0);
        self.hud.inv.to = self.hud.inv.to.clamp(0.0, max);
        let h0 = self.hits.len();
        c.push_clip(clip);
        let opened = self.now - self.hud.inv.at;
        let scroll = self.hud.inv.scroll;
        self.rows(c, Owner::Module(i), x0, x1, top - scroll, |d| {
            let kk = ((opened - d * 0.5) / 0.3).clamp(0.0, 1.0);
            (smooth(0.0, 1.0, kk), (1.0 - ease_out(kk)) * 0.25 * r)
        });
        c.pop_clip();
        // the rows' hits only where they are seen
        let mut kept = Vec::new();
        for (q, hit) in self.hits.drain(h0..) {
            let cq = [q[0].max(clip[0]), q[1].max(clip[1]), q[2].min(clip[2]), q[3].min(clip[3])];
            if cq[2] > cq[0] && cq[3] > cq[1] {
                kept.push((cq, hit));
            }
        }
        self.hits.extend(kept);
        c.alpha = 1.0;
        self.popover(c);
        self.picker(c);
        self.iu_swap();
        self.w = mw;
        self.h = mh;
        text::set_scale(mscale);
    }
}

impl InvUi {
    /// Where the pointer was last seen by the panel (it is swapped out of the menu's between frames).
    fn mouse_seen(&self) -> [f32; 2] {
        self.mouse
    }
}
