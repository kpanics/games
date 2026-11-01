//! Hitboxes (the menu's module "hitboxes", shaders/engine/hitboxes.comp; the game's F3+B shows them so too): the boxes
//! of what is round the player as the game has them, the client's way — lines of light with a light running along them
//! (Box), only their corners (Corners), or glass the world bends through (Glass); where the eyes are (a line round the
//! box at their height) and which way they look (a ray out of them, narrowing, a lamp at its end); a flash when one is
//! hit, the one aimed at brighter. Each comes in as it is seen and goes as it is not. Drawn over the level's picture
//! against its depth: never through a wall.
//!
//! The boxes come from Java (cursed.recode.render.Hitboxes) each frame, of the kinds the player chose and as far as
//! the setting looks: what each is, where its box is, its eyes' height, its look, its health, its hurt.

use super::gpu::{image_in, invert, retire, Compute};
use super::ink::{bytes, dispatch, srgb_of, Bind};
use super::tesseract::over_rect;
use crate::vk::device::Gpu;
use crate::vk::encoder::Encoder;
use crate::vk::resources::{Sampler, Texture, View};
use ash::vk;
use std::time::Instant;

/// What Java says of a box (cursed.recode.render.Hitboxes's bits): aimed at; a thing with no eyes (no eye line, no look).
pub const AIMED: i32 = 1;
pub const NO_EYES: i32 = 2;
/// The kinds (cursed.recode.render.Hitboxes KIND_*): their colour's setting.
pub const KINDS: [&str; 10] = ["Players' colour", "Players' colour", "Hostile colour", "Neutral colour", "Peaceful colour", "Villagers' colour", "Items' colour",
    "Projectiles' colour", "Others' colour", "Players' colour"];
const KIND_COLOURS: [u32; 10] = [0xc8c9f0, 0xc8c9f0, 0xff5a5f, 0xf0b35a, 0x6cd38c, 0x7ab8f5, 0xe8e8f2, 0x9c8cff, 0xbdbdbd, 0xc8c9f0];
/// The most boxes a frame brings and draws (the nearest).
pub const MOST: usize = 128;
const UBO: u64 = 288 + MOST as u64 * 80;
/// How long a box takes to come in and to go (s).
const FADE: f32 = 0.22;

/// A box this frame.
#[derive(Clone, Copy, Default)]
pub struct Box3 {
    pub id: i32,
    pub kind: i32,
    pub flags: i32,
    /// Its lowest and highest corners (from the camera).
    pub lo: [f32; 3],
    pub hi: [f32; 3],
    /// Its eyes' height (from the camera, as lo's y), which way they look (a unit).
    pub eye: f32,
    pub look: [f32; 3],
    /// Its health (0..1; 1 what has none), its hurt (0..1, the game's hurt time: 1 just hit).
    pub health: f32,
    pub hurt: f32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Style {
    Box,
    Corners,
    Glass,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Colouring {
    ByKind,
    ByHealth,
    One,
}

/// The module's settings (colours linear).
#[derive(Clone, Copy)]
pub struct Settings {
    pub on: bool,
    pub style: Style,
    pub width: f32,
    pub glow: f32,
    pub fill: f32,
    pub running: bool,
    pub speed: f32,
    pub back: bool,
    pub eye_line: bool,
    pub eye_colour: [f32; 3],
    pub look: bool,
    pub look_len: f32,
    pub look_colour: [f32; 3],
    pub flash: bool,
    pub aimed: bool,
    pub fade: bool,
    pub colouring: Colouring,
    pub kinds: [[f32; 3]; 10],
    pub full: [f32; 3],
    pub low: [f32; 3],
    pub one: [f32; 3],
    pub corner: f32,
    pub dots: bool,
    pub tint: f32,
    pub refraction: f32,
    pub shine: f32,
    pub edges: f32,
    pub frost: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            on: false,
            style: Style::Box,
            width: 1.0,
            glow: 1.0,
            fill: 0.12,
            running: true,
            speed: 1.0,
            back: true,
            eye_line: true,
            eye_colour: srgb_of(0xff6b6b),
            look: true,
            look_len: 1.6,
            look_colour: srgb_of(0x7ab8ff),
            flash: true,
            aimed: true,
            fade: true,
            colouring: Colouring::ByKind,
            kinds: KIND_COLOURS.map(srgb_of),
            full: srgb_of(0x6cd38c),
            low: srgb_of(0xff5a5f),
            one: srgb_of(0xe6e6e6),
            corner: 0.24,
            dots: true,
            tint: 0.22,
            refraction: 1.0,
            shine: 1.0,
            edges: 1.0,
            frost: false,
        }
    }
}

impl Settings {
    /// From the menu's "set" line: "1\tLabel=value…".
    pub fn parse(text: &str) -> Settings {
        let mut o = Settings::default();
        let mut f = text.split('\t');
        o.on = f.next() == Some("1");
        let colour = |v: &str, was: [f32; 3]| u32::from_str_radix(v.get(..6).unwrap_or(v), 16).map(srgb_of).unwrap_or(was);
        for kv in f {
            let Some((k, v)) = kv.split_once('=') else { continue };
            let n = v.parse::<f32>().ok();
            let on = v == "1";
            match k {
                "Style" => {
                    o.style = match v {
                        "Corners" => Style::Corners,
                        "Glass" => Style::Glass,
                        _ => Style::Box,
                    }
                }
                "Line width" => o.width = n.unwrap_or(o.width).clamp(0.25, 4.0),
                "Glow" => o.glow = n.unwrap_or(o.glow).clamp(0.0, 3.0),
                "Fill" => o.fill = (n.unwrap_or(o.fill * 100.0) / 100.0).clamp(0.0, 1.0),
                "Running light" => o.running = on,
                "Light speed" => o.speed = n.unwrap_or(o.speed).clamp(0.0, 5.0),
                "Back edges" => o.back = on,
                "Eye line" => o.eye_line = on,
                "Eye line colour" => o.eye_colour = colour(v, o.eye_colour),
                "Look direction" => o.look = on,
                "Look length" => o.look_len = n.unwrap_or(o.look_len).clamp(0.2, 8.0),
                "Look colour" => o.look_colour = colour(v, o.look_colour),
                "Flash when hit" => o.flash = on,
                "Brighter when aimed at" => o.aimed = on,
                "Fade in and out" => o.fade = on,
                "Colouring" => {
                    o.colouring = match v {
                        "By health" => Colouring::ByHealth,
                        "One colour" => Colouring::One,
                        _ => Colouring::ByKind,
                    }
                }
                "Full health" => o.full = colour(v, o.full),
                "Low health" => o.low = colour(v, o.low),
                "Colour" => o.one = colour(v, o.one),
                "Corner length" => o.corner = (n.unwrap_or(o.corner * 100.0) / 100.0).clamp(0.05, 0.5),
                "Corner dots" => o.dots = on,
                "Tint" => o.tint = (n.unwrap_or(o.tint * 100.0) / 100.0).clamp(0.0, 1.0),
                "Refraction" => o.refraction = n.unwrap_or(o.refraction).clamp(0.0, 3.0),
                "Shine" => o.shine = n.unwrap_or(o.shine).clamp(0.0, 3.0),
                "Edges" => o.edges = n.unwrap_or(o.edges).clamp(0.0, 3.0),
                "Frost" => o.frost = on,
                _ => {
                    for (i, name) in KINDS.iter().enumerate() {
                        if k == *name {
                            o.kinds[i] = colour(v, o.kinds[i]);
                        }
                    }
                }
            }
        }
        o
    }

    /// A box's colour: its kind's, its health's, or the one.
    pub fn colour_of(&self, b: &Box3) -> [f32; 3] {
        match self.colouring {
            Colouring::ByKind => self.kinds[(b.kind.clamp(0, 9)) as usize],
            Colouring::ByHealth => {
                let k = b.health.clamp(0.0, 1.0);
                std::array::from_fn(|i| self.low[i] + (self.full[i] - self.low[i]) * k)
            }
            Colouring::One => self.one,
        }
    }
}

/// One box as it is drawn: what Java last said of it, how far in it is (0..1), its flash.
struct Shown {
    b: Box3,
    /// Where its box was in the world (camera + lo), kept to draw it fading out once Java no longer brings it.
    world_lo: [f64; 3],
    world_hi: [f64; 3],
    world_eye: f64,
    seen: bool,
    alpha: f32,
    flash: f32,
    hurt_was: f32,
}

pub struct Hitboxes {
    pipe: Option<Compute>,
    canvas: Option<(Box<Texture>, Box<View>)>,
    shown: Vec<Shown>,
    set: Settings,
    set_at: Option<Instant>,
    started: Instant,
    last: Option<Instant>,
}

/// Where a point from the camera falls on the frame (pixels) and its distance along the view (clip w); None behind.
fn on_screen(m: &[f32; 16], p: [f32; 3], fw: f32, fh: f32) -> Option<([f32; 2], f32)> {
    let c = |r: usize| m[r] * p[0] + m[4 + r] * p[1] + m[8 + r] * p[2] + m[12 + r];
    let w = c(3);
    if w < 0.05 {
        return None;
    }
    Some(([(c(0) / w * 0.5 + 0.5) * fw, (c(1) / w * 0.5 + 0.5) * fh], w))
}

impl Hitboxes {
    pub fn new() -> Hitboxes {
        Hitboxes { pipe: None, canvas: None, shown: Vec::new(), set: Settings::default(), set_at: None, started: Instant::now(), last: None }
    }

    /// The menu's settings, asked four times a second.
    pub fn poll(&mut self) -> Settings {
        let now = Instant::now();
        if self.set_at.is_none_or(|t| now.duration_since(t).as_secs_f32() > 0.25) {
            self.set_at = Some(now);
            self.set = Settings::parse(&crate::gui::call("set\thitboxes"));
        }
        self.set
    }

    pub fn set_shader(&mut self, gpu: &Gpu, spirv: &[u32]) {
        use vk::DescriptorType as T;
        let kinds = [T::STORAGE_IMAGE, T::COMBINED_IMAGE_SAMPLER, T::COMBINED_IMAGE_SAMPLER, T::UNIFORM_BUFFER];
        if let Some(old) = self.pipe.replace(Compute::new(gpu, "hitboxes", spirv, &kinds, 4)) {
            old.destroy(gpu);
        }
    }

    /// The boxes shown stepped by this frame's: new ones come in, the ones no longer brought go, a hurt flashes.
    fn follow(&mut self, boxes: &[Box3], camera: [f64; 3], dt: f32) {
        let s = self.set;
        let k_in = if s.fade { (dt / FADE).min(1.0) } else { 1.0 };
        for sh in &mut self.shown {
            sh.seen = false;
        }
        for b in boxes.iter().take(MOST) {
            let world_lo = [camera[0] + b.lo[0] as f64, camera[1] + b.lo[1] as f64, camera[2] + b.lo[2] as f64];
            let world_hi = [camera[0] + b.hi[0] as f64, camera[1] + b.hi[1] as f64, camera[2] + b.hi[2] as f64];
            let world_eye = camera[1] + b.eye as f64;
            match self.shown.iter_mut().find(|sh| sh.b.id == b.id) {
                Some(sh) => {
                    // a hurt that just began (the game's hurt time back at its top)
                    if s.flash && b.hurt > sh.hurt_was + 0.3 {
                        sh.flash = 1.0;
                    }
                    sh.hurt_was = b.hurt;
                    sh.b = *b;
                    sh.world_lo = world_lo;
                    sh.world_hi = world_hi;
                    sh.world_eye = world_eye;
                    sh.seen = true;
                }
                None => self.shown.push(Shown { b: *b, world_lo, world_hi, world_eye, seen: true, alpha: 0.0, flash: 0.0, hurt_was: b.hurt }),
            }
        }
        for sh in &mut self.shown {
            sh.alpha = if sh.seen { (sh.alpha + k_in).min(1.0) } else { sh.alpha - k_in };
            sh.flash *= (-dt * 5.5).exp();
            if !sh.seen {
                // drawn where it last was, as the camera moves
                sh.b.lo = [(sh.world_lo[0] - camera[0]) as f32, (sh.world_lo[1] - camera[1]) as f32, (sh.world_lo[2] - camera[2]) as f32];
                sh.b.hi = [(sh.world_hi[0] - camera[0]) as f32, (sh.world_hi[1] - camera[1]) as f32, (sh.world_hi[2] - camera[2]) as f32];
                sh.b.eye = (sh.world_eye - camera[1]) as f32;
            }
        }
        self.shown.retain(|sh| sh.alpha > 0.0);
        // the nearest first (a frame's most are its nearest)
        self.shown.sort_by(|a, b| dist2(&a.b).total_cmp(&dist2(&b.b)));
        self.shown.truncate(MOST);
    }

    /// The rectangle of the frame (pixels: lowest x, y, past the highest) a box reaches with its glow, its eyes' ray
    /// with its; the whole frame when a point of it is behind the camera.
    fn box_reach(&self, b: &Box3, m: &[f32; 16], fw: u32, fh: u32) -> [f32; 4] {
        let (w, h) = (fw as f32, fh as f32);
        let mut lo = [f32::MAX; 2];
        let mut hi = [f32::MIN; 2];
        let margin = 6.0 + 22.0 * self.set.glow.max(0.2) * self.set.width.max(0.5);
        let mut pts: Vec<[f32; 3]> = (0..8).map(|i| [if i & 1 == 1 { b.hi[0] } else { b.lo[0] }, if i & 2 == 2 { b.hi[1] } else { b.lo[1] }, if i & 4 == 4 { b.hi[2] } else { b.lo[2] }]).collect();
        if self.set.look {
            let mid = [(b.lo[0] + b.hi[0]) * 0.5, b.eye, (b.lo[2] + b.hi[2]) * 0.5];
            pts.push([mid[0] + b.look[0] * self.set.look_len, mid[1] + b.look[1] * self.set.look_len, mid[2] + b.look[2] * self.set.look_len]);
        }
        for p in pts {
            match on_screen(m, p, w, h) {
                Some((s, d)) => {
                    // nearer, the glow is wider (a line's glow is as wide as the line is thick)
                    let k = margin * (1.0 + 2.0 / d.max(0.5));
                    lo = [lo[0].min(s[0] - k), lo[1].min(s[1] - k)];
                    hi = [hi[0].max(s[0] + k), hi[1].max(s[1] + k)];
                }
                None => return [0.0, 0.0, w, h],
            }
        }
        [lo[0].max(0.0), lo[1].max(0.0), hi[0].min(w), hi[1].min(h)]
    }

    /// The rectangle of the frame all the boxes reach.
    fn reach(&self, m: &[f32; 16], fw: u32, fh: u32) -> Option<[i32; 4]> {
        let mut r = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        for sh in &self.shown {
            let b = self.box_reach(&sh.b, m, fw, fh);
            r = [r[0].min(b[0]), r[1].min(b[1]), r[2].max(b[2]), r[3].max(b[3])];
        }
        if self.shown.is_empty() {
            return None;
        }
        let r = [r[0].floor() as i32, r[1].floor() as i32, r[2].ceil() as i32, r[3].ceil() as i32];
        (r[2] > r[0] && r[3] > r[1]).then_some(r)
    }

    /// One frame over `color` (`depth` the level's): the boxes stepped by this frame's and drawn.
    #[allow(clippy::too_many_arguments)]
    pub fn frame(
        &mut self,
        gpu: &Gpu,
        enc: &mut Encoder,
        color: &Texture,
        depth: &Texture,
        boxes: &[Box3],
        camera: [f64; 3],
        view_proj: [f32; 16],
        nearest: &Sampler,
        linear: &Sampler,
    ) {
        let now = Instant::now();
        let dt = self.last.map_or(0.0, |l| now.duration_since(l).as_secs_f32()).min(0.1);
        self.last = Some(now);
        self.follow(boxes, camera, dt);
        let Some(pipe) = self.pipe.as_ref() else { return };
        if self.shown.is_empty() {
            if let Some((t, v)) = self.canvas.take() {
                retire(enc, t, v);
            }
            return;
        }
        let (fw, fh) = (color.width, color.height);
        if fw == 0 || fh == 0 {
            return;
        }
        let Some(rect) = self.reach(&view_proj, fw, fh) else { return };
        let cmd = enc.cmd(gpu);
        if self.canvas.as_ref().is_some_and(|c| c.0.width != fw || c.0.height != fh) {
            let (t, v) = self.canvas.take().unwrap();
            retire(enc, t, v);
        }
        if self.canvas.is_none() {
            self.canvas = Some(image_in(gpu, cmd, "Hitboxes over the frame", vk::Format::R8G8B8A8_UNORM, fw, fh, 1, false));
        }
        let piece = enc.transient.mapped(gpu, UBO, gpu.info.min_uniform_align.max(16) as u64, UBO, 1);
        let host = piece.host;
        let s = self.set;
        let ppb = (view_proj[1] * view_proj[1] + view_proj[5] * view_proj[5] + view_proj[9] * view_proj[9]).sqrt() * fh as f32 * 0.5;
        let style = match s.style {
            Style::Box => 0.0,
            Style::Corners => 1.0,
            Style::Glass => 2.0,
        };
        let n = self.shown.len().min(MOST);
        let inv = invert(&view_proj);
        // the way the camera looks: the frame's middle a little way out
        let mid = [inv[8] * 0.5 + inv[12], inv[9] * 0.5 + inv[13], inv[10] * 0.5 + inv[14], inv[11] * 0.5 + inv[15]];
        let fl = (mid[0] * mid[0] + mid[1] * mid[1] + mid[2] * mid[2]).sqrt() * mid[3].signum();
        let forward = if fl.abs() > 1e-9 { [mid[0] / fl, mid[1] / fl, mid[2] / fl] } else { [0.0, 0.0, -1.0] };
        bytes(host, 0, &view_proj);
        bytes(host, 64, &inv);
        bytes(host, 128, &[(now.duration_since(self.started).as_secs_f64() % 3600.0) as f32, n as f32, fw as f32, fh as f32]);
        bytes(host, 144, &rect);
        bytes(host, 160, &[ppb, style, s.width, s.glow]);
        bytes(host, 176, &[s.fill, if s.running { s.speed.max(0.001) } else { 0.0 }, s.back as i32 as f32, s.corner]);
        bytes(host, 192, &[s.tint, s.refraction, s.shine, s.edges]);
        bytes(host, 208, &[s.eye_colour[0], s.eye_colour[1], s.eye_colour[2], s.eye_line as i32 as f32]);
        bytes(host, 224, &[s.look_colour[0], s.look_colour[1], s.look_colour[2], if s.look { s.look_len } else { 0.0 }]);
        bytes(host, 240, &[forward[0], forward[1], forward[2], s.dots as i32 as f32]);
        bytes(host, 256, &[s.frost as i32 as f32, s.aimed as i32 as f32, 0.0, 0.0]);
        bytes(host, 272, &[0.0f32; 4]);
        for (i, sh) in self.shown.iter().take(n).enumerate() {
            let b = &sh.b;
            let c = s.colour_of(b);
            let alpha = smooth(sh.alpha);
            // (a thing's eyes below its box: none drawn)
            let aimed = (b.flags & AIMED != 0) as i32 as f32;
            let eye = if b.flags & NO_EYES != 0 { b.lo[1] - 1.0 } else { b.eye };
            let o = 288 + i * 80;
            bytes(host, o, &[b.lo[0], b.lo[1], b.lo[2], alpha]);
            bytes(host, o + 16, &[b.hi[0], b.hi[1], b.hi[2], sh.flash]);
            bytes(host, o + 32, &[c[0], c[1], c[2], aimed]);
            bytes(host, o + 48, &[b.look[0], b.look[1], b.look[2], eye]);
            bytes(host, o + 64, &self.box_reach(b, &view_proj, fw, fh));
        }
        let ubo = Bind::Uniform(unsafe { (*piece.buffer).raw }, piece.offset, UBO);
        let depth_view = super::upscale::temp_view(gpu, enc, depth);
        let color_view = super::upscale::temp_view(gpu, enc, color);
        let canvas = self.canvas.as_ref().unwrap();
        let set = [Bind::Storage(canvas.1.raw), Bind::Sampled(depth_view, nearest.raw), Bind::Sampled(color_view, linear.raw), ubo];
        enc.segment(gpu, cmd, "Engine", "hitboxes (compute)");
        over_rect(gpu, cmd, color, &canvas.0, rect, |cmd| {
            dispatch(gpu, cmd, pipe, &set, &0i32.to_ne_bytes(), [((rect[2] - rect[0]) as u32).div_ceil(8), ((rect[3] - rect[1]) as u32).div_ceil(8), 1]);
        });
        enc.segment_end(gpu, cmd);
    }

    pub fn destroy(&mut self, gpu: &Gpu) {
        if let Some(p) = self.pipe.take() {
            p.destroy(gpu);
        }
        if let Some((t, v)) = self.canvas.take() {
            super::ink::kill(gpu, t, v);
        }
    }
}

fn dist2(b: &Box3) -> f32 {
    let c = [(b.lo[0] + b.hi[0]) * 0.5, (b.lo[1] + b.hi[1]) * 0.5, (b.lo[2] + b.hi[2]) * 0.5];
    c[0] * c[0] + c[1] * c[1] + c[2] * c[2]
}

fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_box(id: i32, z: f32) -> Box3 {
        Box3 { id, kind: 2, flags: 0, lo: [-0.3, -1.6, z - 0.3], hi: [0.3, 0.2, z + 0.3], eye: 0.0, look: [0.0, 0.0, -1.0], health: 1.0, hurt: 0.0 }
    }

    #[test]
    fn settings_from_the_menu() {
        let s = Settings::parse("1\tStyle=Glass\tLine width=2\tFill=40\tRunning light=0\tColouring=One colour\tColour=ff0000\tCorner length=30\tTint=50\tHostile colour=00ff00");
        assert!(s.on && s.style == Style::Glass && !s.running && s.colouring == Colouring::One);
        assert_eq!((s.width, s.fill, s.corner, s.tint), (2.0, 0.4, 0.3, 0.5));
        assert!(s.one[0] > 0.99 && s.one[1] < 1e-4);
        assert!(s.kinds[2][1] > 0.99 && s.kinds[2][0] < 1e-4);
        assert!(!Settings::parse("0\tStyle=Box").on);
        assert_eq!(Settings::parse("").style, Style::Box);
    }

    #[test]
    fn colours_by_kind_health_or_one() {
        let mut s = Settings::default();
        let mut b = a_box(1, -3.0);
        assert_eq!(s.colour_of(&b), s.kinds[2]);
        s.colouring = Colouring::ByHealth;
        b.health = 0.0;
        assert_eq!(s.colour_of(&b), s.low);
        b.health = 1.0;
        assert_eq!(s.colour_of(&b), s.full);
        s.colouring = Colouring::One;
        assert_eq!(s.colour_of(&b), s.one);
    }

    #[test]
    fn boxes_come_in_flash_and_go() {
        let mut h = Hitboxes::new();
        h.set = Settings { on: true, ..Settings::default() };
        let b = a_box(5, -4.0);
        h.follow(&[b], [0.0; 3], 0.05);
        assert_eq!(h.shown.len(), 1);
        assert!(h.shown[0].alpha > 0.0 && h.shown[0].alpha < 1.0, "it comes in, not at once");
        for _ in 0..10 {
            h.follow(&[b], [0.0; 3], 0.05);
        }
        assert!((h.shown[0].alpha - 1.0).abs() < 1e-6);
        // hit: it flashes, then the flash fades
        h.follow(&[Box3 { hurt: 1.0, ..b }], [0.0; 3], 0.016);
        assert!(h.shown[0].flash > 0.9);
        for _ in 0..30 {
            h.follow(&[Box3 { hurt: 0.5, ..b }], [0.0; 3], 0.05);
        }
        assert!(h.shown[0].flash < 0.01);
        // no longer brought: it fades where it was (the camera moving), then is gone
        h.follow(&[], [1.0, 0.0, 0.0], 0.05);
        assert_eq!(h.shown.len(), 1);
        assert!((h.shown[0].b.lo[0] - (-1.3)).abs() < 1e-4, "kept where it was in the world");
        for _ in 0..10 {
            h.follow(&[], [1.0, 0.0, 0.0], 0.05);
        }
        assert!(h.shown.is_empty());
    }

    #[test]
    fn the_nearest_kept_and_it_fits() {
        let mut h = Hitboxes::new();
        let boxes: Vec<Box3> = (0..MOST as i32 + 40).map(|i| a_box(i, -2.0 - i as f32)).collect();
        h.follow(&boxes, [0.0; 3], 0.05);
        assert_eq!(h.shown.len(), MOST);
        assert!(h.shown.iter().all(|sh| sh.b.id < MOST as i32));
        assert!(UBO <= 16384, "the uniform buffer fits every GPU's least");
    }

    #[test]
    fn its_rectangle_holds_it() {
        let f = 1.0 / (35f32.to_radians().tan());
        let (n, fa) = (0.05f32, 1000.0f32);
        let a = 16.0 / 9.0;
        let m = [f / a, 0.0, 0.0, 0.0, 0.0, -f, 0.0, 0.0, 0.0, 0.0, n / (fa - n), -1.0, 0.0, 0.0, fa * n / (fa - n), 0.0];
        let mut h = Hitboxes::new();
        h.set = Settings { on: true, look: false, ..Settings::default() };
        h.follow(&[a_box(1, -6.0)], [0.0; 3], 1.0);
        let r = h.reach(&m, 1920, 1080).unwrap();
        assert!(r[0] < 960 && r[2] > 960 && r[1] < 540 && r[3] > 540);
        assert!(r[2] - r[0] < 500, "{r:?}");
        // a box round the camera: the whole frame
        h.follow(&[a_box(2, 0.0)], [0.0; 3], 1.0);
        assert_eq!(h.reach(&m, 1920, 1080), Some([0, 0, 1920, 1080]));
    }
}
