//! The Custom Scoreboard's element (Interface → Custom Scoreboard): the server's sidebar as it sent it, written in the
//! HUD's letters on its glass (Java's HudBoard and HudText: the server's colours and its pack's glyphs stay). Its title
//! (a divider under it), its lines; the scores at the right in a quiet mono, left out when they only count the lines
//! (6, 5, 4…); a line that changed lit for a moment. As wide as its longest line, easing as lines come and go.

use super::hudtext::RText;
use super::*;

/// The sidebar as Java has it this frame (HudBoard.append).
#[derive(Clone, Default)]
pub struct Board {
    pub title: RText,
    /// each line: its words, its score, since it changed (s)
    pub lines: Vec<(RText, RText, f32)>,
    /// the scores only count the lines
    pub counter: bool,
}

impl Board {
    /// From Java's numbers at `at` (how many lines, -1: none; whether the scores count them; each line's time since it
    /// changed) and its text (the title's runs, then each line's and its score's).
    pub fn parse(g: &dyn Fn(usize) -> f64, at: usize, line: &mut dyn FnMut() -> String) -> (Option<Board>, usize) {
        let n = g(at);
        if n < 0.0 {
            return (None, 2);
        }
        let n = (n as usize).min(32);
        let title = RText::parse(0, &line());
        let mut lines = Vec::with_capacity(n);
        for i in 0..n {
            let name = RText::parse(1 + i as u32, &line());
            let score = RText::parse(33 + i as u32, &line());
            lines.push((name, score, g(at + 2 + i) as f32));
        }
        (Some(Board { title, lines, counter: g(at + 1) > 0.5 }), 2 + n)
    }
}

/// Where the sidebar is (said back to Java in the state, as before: nothing of it is drawn there now).
#[derive(Clone, Copy, Default)]
pub struct BoardPlace {
    pub corner: [f32; 2],
    pub width: f32,
    pub alpha: f32,
    pub rel: [f32; 2],
}

/// The example's lines (the menu's preview, the editor with no sidebar): a server's usual.
const SAMPLE: [(&str, &str); 4] = [("Balance", "12 400"), ("Kills", "7"), ("Clan", "Cursed"), ("Online", "128")];

/// A line that is only a site's address ("funtime.su", "play.server.net"): no spaces, a dot with letters on both sides.
fn site_line(s: &str) -> bool {
    let t = s.trim().trim_start_matches("www.");
    if t.is_empty() || t.len() > 40 || t.contains(' ') {
        return false;
    }
    let Some(dot) = t.rfind('.') else { return false };
    let (name, tld) = (&t[..dot], &t[dot + 1..]);
    !name.is_empty() && tld.len() >= 2 && tld.len() <= 10 && tld.chars().all(|c| c.is_alphabetic()) && name.chars().all(|c| c.is_alphanumeric() || c == '.' || c == '-')
}

impl Gui {
    pub(super) fn el_board(&mut self, c: &mut Canvas, x: &Ctx) -> Option<[f32; 2]> {
        let s = x.s;
        let p = x.pad();
        let example = x.sample && (!x.icons || self.hud.facts.board.is_none());
        let b = if example {
            Board {
                title: RText::words("Recode"),
                lines: SAMPLE.iter().enumerate().map(|(i, (k, v))| (RText::words(&format!("{}: {v}", tr(k))), RText::words(&(SAMPLE.len() - i).to_string()), 99.0)).collect(),
                counter: true,
            }
        } else {
            self.hud.facts.board.clone()?
        };
        let icons = x.icons && !example;
        // what the player keeps out: the server's site at the foot, the empty lines it spaces its board with
        let mut b = b;
        if x.on("Hide the site line") {
            b.lines.retain(|l| !site_line(&l.0.plain()));
        }
        if x.on("Hide empty lines") {
            b.lines.retain(|l| !l.0.plain().trim().is_empty());
        }
        let tk = super::text::scale();
        let (fs, tfs) = (12.5 * s, 14.0 * s * (x.num("Title size") / 100.0).max(0.3));
        let title = x.on("Title") && !b.title.is_empty();
        let divider = title && x.on("Divider");
        let scores = match x.pick("Numbers") {
            0 => !b.counter,
            1 => true,
            _ => false,
        };
        let lh = fs * tk * 1.55 + x.num("Line spacing") * s;
        let gap = 14.0 * s;
        let sfs = fs * 0.92;
        // as wide as its longest line (the scores' column at the right)
        let names: Vec<f32> = b.lines.iter().map(|l| l.0.width(fs, Face::Regular)).collect();
        let score_w: Vec<f32> = b.lines.iter().map(|l| if scores { fonts().width(Face::Mono, sfs, l.1.plain().trim()) } else { 0.0 }).collect();
        let title_w = if title { b.title.width(tfs, Face::Semi) } else { 0.0 };
        let mut cw = title_w.max(x.num("Least width").max(40.0) * s);
        for (nw, sw) in names.iter().zip(&score_w) {
            cw = cw.max(nw + if *sw > 0.0 { gap + sw } else { 0.0 });
        }
        let th = if title { tfs * tk * 1.45 + if divider { 9.0 * s } else { 3.0 * s } } else { 0.0 };
        // (the last line needs only its own height, not the spacing after it)
        let body = if b.lines.is_empty() { 0.0 } else { (b.lines.len() - 1) as f32 * lh + fs * tk * 1.3 };
        let w = cw + 2.0 * p;
        let h = (th + body).max(fs * tk) + 2.0 * p;
        x.plate(c, [M, M, M + w, M + h]);
        let (cx, cy) = (M + p, M + p);
        if title {
            let tx = match x.pick("Title align") {
                0 => cx,
                2 => cx + cw - title_w,
                _ => cx + (cw - title_w) / 2.0,
            };
            self.rtext(c, &b.title, tx, cy + tfs * tk * 0.95, tfs, Face::Semi, x.el.text, icons);
        }
        if divider {
            let y = cy + tfs * tk * 1.45 + 3.0 * s;
            c.rect([cx, y, cx + cw, y + s.max(1.0)], 0.5 * s, fade(x.el.text2, 0.4));
        }
        let la = x.pick("Lines align");
        for (i, (name, score, since)) in b.lines.iter().enumerate() {
            let top = cy + th + i as f32 * lh;
            let base = top + fs * tk * 1.02;
            // a line that changed: lit a moment
            if x.on("Flash changes") && *since < 0.9 {
                let k = 1.0 - since / 0.9;
                c.rect([cx - 4.0 * s, top - 1.0 * s, cx + cw + 4.0 * s, top + fs * tk * 1.35], 5.0 * s, fade(x.el.accent, 0.14 * k * k));
            }
            let room = cw - if score_w[i] > 0.0 { score_w[i] + gap } else { 0.0 };
            let nx = match la {
                0 => cx,
                2 => cx + room - names[i],
                _ => cx + (room - names[i]) / 2.0,
            };
            self.rtext(c, name, nx, base, fs, Face::Regular, x.el.text, icons);
            if score_w[i] > 0.0 {
                c.text_r(cx + cw, base, sfs, Face::Mono, score.plain().trim(), x.col("Numbers colour", x.el.text2));
            }
        }
        if x.icons {
            self.hud.board_place.rel = [cx - M, cy - M];
            self.hud.board_place.width = cw;
        }
        Some([w, h])
    }
}
