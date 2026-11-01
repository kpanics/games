//! What a row of the chat shows and how it lies in a width: a player's line (the rank's badge or word, the clan, the
//! name, a title, the words with chips for commands, places and sites, your name lit), a server's quiet line, a card
//! (an event, a request, a teleport, a restart, a check). Laid out once for a width and a look and kept with the row
//! (store.rs Row::laid): words are measured only when something changes, not every frame.

use super::parse::rich::{self, Span};
use super::parse::{self as parse, Chan, Prize, Rq, Tone};
use super::parse::Kind as Said;
use super::store::{Row, Run, Store, Tap, F_GLYPH, F_ITEM, F_OBF, F_STRIKE, F_UNDER};
use crate::gui::draw::Rgba;
use crate::gui::i18n::tr;
use crate::gui::text::{fonts, Face};
use crate::gui::theme::fade;

/// How the chat looks this frame (the chat's own px are the HUD's at 1920 × 1080: `k` px each).
#[derive(Clone)]
pub struct Look {
    pub k: f32,
    pub size: f32,
    pub lh: f32,
    /// the game's GUI unit in px (a pack's glyphs are measured in it)
    pub gu: f32,
    pub base: Rgba,
    pub dim: Rgba,
    pub accent: Rgba,
    pub ment: Rgba,
    pub me: String,
    /// the search's words (lowercase), when it is on
    pub q: String,
    pub heads: bool,
    /// a face's size (px)
    pub head: f32,
    /// how far a message's wrapped lines start in from its first (px)
    pub indent: f32,
    /// the server's colours: 0 as sent, 1 softened toward the chat's own, 2 the chat's own
    pub tones: u8,
    /// the players' names: 0 plain, 1 bold, 2 bold in the accent
    pub names: u8,
    /// the ranks' badges, words and prefixes before a name
    pub ranks: bool,
}

impl Look {
    /// A server's colour as the chat shows it: as sent (the darkest lifted to be read), halfway to `base`, or `base`.
    pub fn tone(&self, c: Option<u32>, base: Rgba) -> Rgba {
        match self.tones {
            2 => base,
            1 => crate::gui::draw::mix(readable(c, base), base, 0.5),
            _ => readable(c, base),
        }
    }

    /// A player's name: its face and colour as set.
    fn nick(&self, name: &str) -> Atom {
        let face = if self.names == 0 { Face::Regular } else { Face::Semi };
        Atom { ak: Ak::Nick, ..Atom::words(name, face, self.size, if self.names == 2 { self.accent } else { self.base }) }
    }

    /// What a layout depends on, as one number (a row laid for another is laid again).
    pub fn key(&self) -> u64 {
        let mut h: u64 = 0xcbf29ce484222325;
        let mut eat = |v: u64| {
            h ^= v;
            h = h.wrapping_mul(0x100000001b3);
        };
        for f in [self.k, self.size, self.lh, self.gu] {
            eat(f.to_bits() as u64);
        }
        // (Streamer Mode's words put aside: their widths are others')
        eat(crate::gui::mask::generation());
        for c in [self.base, self.dim, self.accent, self.ment] {
            for v in c {
                eat((v * 255.0) as u64);
            }
        }
        for b in self.me.bytes().chain(self.q.bytes()) {
            eat(b as u64);
        }
        eat(self.heads as u64);
        for f in [self.head, self.indent] {
            eat(f.to_bits() as u64);
        }
        eat(self.tones as u64 * 16 + self.names as u64 * 2 + self.ranks as u64);
        // (the letters' family and weight as the element sets them: their widths with them)
        let (family, bolder) = crate::gui::text::style();
        eat(family as u64 * 2 + bolder as u64);
        h
    }
}

#[derive(Clone, PartialEq)]
pub enum Ak {
    Words,
    /// a pack's glyphs (Java draws them: its key, width)
    Glyph(f32, f32),
    /// an item the server shows (Java draws its picture before the words)
    Item(f32),
    /// 0 plain, 1 a place, 2 a site, 3 a command
    Chip(u8),
    /// a chip that does nothing (→ you, an announcement)
    Tag,
    Nick,
    Me,
}

#[derive(Clone)]
pub struct Atom {
    pub text: String,
    pub face: Face,
    pub size: f32,
    pub col: Rgba,
    pub ak: Ak,
    pub fl: u32,
    pub tap: Option<Tap>,
    /// room after it (px)
    pub gap: f32,
    pub found: bool,
    /// what the server says when the pointer is over it
    pub hover: Option<String>,
}

impl Atom {
    pub fn words(text: &str, face: Face, size: f32, col: Rgba) -> Atom {
        Atom { text: text.to_string(), face, size, col, ak: Ak::Words, fl: 0, tap: None, gap: 0.0, found: false, hover: None }
    }
    fn gap(mut self, g: f32) -> Atom {
        self.gap = g;
        self
    }
}

/// A server's line's mark in the face's place: its tone (monochrome: a quiet dot, a bright one, a ring).
#[derive(Clone, Copy)]
pub enum Mark {
    Dot(Tone),
    /// an icon of the menu's (gui.comp icon())
    Icon(u32),
}

#[derive(Clone)]
pub enum Icon {
    Face(String),
    /// an item Java draws (HudLayer icon 24): 0 a beacon, 1 a chest, 2 a pickaxe, 3 a totem, 4 an emerald, 5 a sword,
    /// 6 a compass, 7 an ender pearl, 8 a clock
    Item(u8),
}

#[derive(Clone)]
pub struct Btn {
    pub label: String,
    /// 0 plain, 1 the main one (light), 2 quiet
    pub style: u8,
    pub which: u8,
}

#[derive(Clone)]
pub enum Body {
    /// a player's line: whose face (none for a line that goes on from the one before), its atoms
    Line { face: Option<String>, atoms: Vec<Atom> },
    Sys { mark: Mark, atoms: Vec<Atom> },
    Card { icon: Icon, title: Vec<Atom>, sub: Vec<Atom>, btns: Vec<Btn>, fuse: bool, state: Option<String>, done: bool },
}

/// An atom placed: which, its line, x from the left, its words there, its width.
#[derive(Clone)]
pub struct Placed {
    pub ai: usize,
    pub line: usize,
    pub x: f32,
    pub text: String,
    pub w: f32,
}

/// A row laid: its body, its height, its atoms placed (a card: its title's, then its words'), how many lines each,
/// where its last line ends (×N goes after it).
#[derive(Clone)]
pub struct Laid {
    pub body: Body,
    pub h: f32,
    pub a: Vec<Placed>,
    pub a_lines: usize,
    pub b: Vec<Placed>,
    pub b_lines: usize,
    pub end: [f32; 2],
    /// a line that goes on from the one before (no face, no name; less room over it)
    pub cont: bool,
}

/// A server's colour on the client's glass: as it is, the darkest ones lifted to be read.
pub fn readable(c: Option<u32>, base: Rgba) -> Rgba {
    let Some(c) = c else { return base };
    let r = crate::gui::draw::hex(c & 0xFFFFFF, 1.0);
    let l = 0.3 * r[0] + 0.59 * r[1] + 0.11 * r[2];
    if l < 0.3 {
        let k = (0.3 - l) / 0.3 * 0.75;
        [r[0] + (0.55 - r[0]) * k, r[1] + (0.55 - r[1]) * k, r[2] + (0.6 - r[2]) * k, 1.0]
    } else {
        r
    }
}

fn hexc(c: u32) -> Rgba {
    crate::gui::draw::hex(c & 0xFFFFFF, 1.0)
}

pub fn is_name(s: &str) -> bool {
    let n = s.chars().count();
    (2..=16).contains(&n) && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn is_link(w: &str) -> bool {
    let bare = w.trim_start_matches("https://").trim_start_matches("http://");
    let host = bare.split('/').next().unwrap_or("");
    let Some(dot) = host.rfind('.') else { return false };
    let tld = &host[dot + 1..];
    dot > 0
        && ["xyz", "ru", "su", "me", "pw", "com", "gg", "net", "org"].contains(&tld.to_ascii_lowercase().as_str())
        && host[..dot].split('.').all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
}

fn int_of(w: &str) -> Option<i64> {
    let t = w.trim_end_matches([',', ';', '.']);
    if t.is_empty() || t.len() > 7 {
        return None;
    }
    t.parse().ok()
}

/// Commands that take a player after them (the chip takes the name too).
const WITH_NAME: &[&str] = &["/msg", "/m", "/w", "/tell", "/tpa", "/tpahere", "/call", "/duel", "/trade", "/pay", "/invite", "/r"];

/// The chip of a command, a place or a site.
fn chip(text: String, kind: u8, tap: Tap, space: bool, look: &Look) -> Atom {
    Atom { text, face: Face::Mono, size: look.size * 0.8, col: look.base, ak: Ak::Chip(kind), fl: 0, tap: Some(tap), gap: if space { look.size * 0.3 } else { 0.0 }, found: false, hover: None }
}

pub fn place_chip(at: &str, look: &Look) -> Atom {
    chip(at.to_string(), 1, Tap::Copy(at.to_string(), "Coordinates copied"), false, look)
}

/// A text's words as atoms: commands, places and sites as chips, your name lit (not in your own words), the search's
/// words marked.
#[allow(clippy::too_many_arguments)]
fn words_into(out: &mut Vec<Atom>, text: &str, col: Rgba, face: Face, look: &Look, fl: u32, tap: Option<Tap>, light_me: bool) {
    // words with the spaces after them
    let mut toks: Vec<String> = Vec::new();
    let mut cur = String::new();
    for ch in text.chars() {
        cur.push(ch);
        if ch == ' ' {
            toks.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        toks.push(cur);
    }
    let size = look.size;
    let mut plain = String::new();
    let flush = |plain: &mut String, out: &mut Vec<Atom>| {
        if plain.is_empty() {
            return;
        }
        let p = std::mem::take(plain);
        // the search's words marked inside the plain words
        if !look.q.is_empty() {
            let lo = p.to_lowercase();
            if lo.len() == p.len() {
                let mut at = 0;
                while let Some(i) = lo[at..].find(&look.q) {
                    let a = at + i;
                    let b = a + look.q.len();
                    if !(p.is_char_boundary(a) && p.is_char_boundary(b)) {
                        break;
                    }
                    if a > at {
                        out.push(Atom { fl, tap: tap.clone(), ..Atom::words(&p[at..a], face, size, col) });
                    }
                    out.push(Atom { fl, tap: tap.clone(), found: true, ..Atom::words(&p[a..b], face, size, col) });
                    at = b;
                }
                if at < p.len() {
                    out.push(Atom { fl, tap: tap.clone(), ..Atom::words(&p[at..], face, size, col) });
                }
                return;
            }
        }
        out.push(Atom { fl, tap: tap.clone(), ..Atom::words(&p, face, size, col) });
    };
    let mut i = 0;
    while i < toks.len() {
        let t = toks[i].as_str();
        let w = t.trim_end();
        let space = t.ends_with(' ');
        // a place: three numbers in a row
        if i + 2 < toks.len() {
            if let (Some(a), Some(b), Some(c)) = (int_of(w), int_of(toks[i + 1].trim_end()), int_of(toks[i + 2].trim_end())) {
                if (-64..=320).contains(&b) {
                    flush(&mut plain, out);
                    let xyz = format!("{a} {b} {c}");
                    out.push(chip(xyz.clone(), 1, Tap::Copy(xyz, "Coordinates copied"), toks[i + 2].ends_with(' '), look));
                    i += 3;
                    continue;
                }
            }
        }
        // a command (a player's name after it with some)
        if w.len() >= 3 && w.starts_with('/') && w[1..].chars().all(|c| c.is_ascii_lowercase()) {
            flush(&mut plain, out);
            let mut cmd = w.to_string();
            let mut sp = space;
            if WITH_NAME.contains(&w) && i + 1 < toks.len() && is_name(toks[i + 1].trim_end()) {
                cmd = format!("{} {}", w, toks[i + 1].trim_end());
                sp = toks[i + 1].ends_with(' ');
                i += 1;
            }
            out.push(chip(cmd.clone(), 3, Tap::Field(format!("{cmd} ")), sp, look));
            i += 1;
            continue;
        }
        let bare = w.trim_end_matches(['.', ',', '!', '?', ')', ':']);
        if is_link(bare) {
            flush(&mut plain, out);
            out.push(chip(bare.to_string(), 2, Tap::Copy(bare.to_string(), "Link copied"), space, look));
            if bare.len() < w.len() {
                plain.push_str(&w[bare.len()..]);
                if space {
                    plain.push(' ');
                }
            }
            i += 1;
            continue;
        }
        if light_me && !look.me.is_empty() && bare.eq_ignore_ascii_case(&look.me) {
            flush(&mut plain, out);
            out.push(Atom { ak: Ak::Me, gap: if space { size * 0.28 } else { 0.0 }, ..Atom::words(bare, Face::Semi, size, look.base) });
            if bare.len() < w.len() {
                plain.push_str(&w[bare.len()..]);
                if space {
                    plain.push(' ');
                }
            }
            i += 1;
            continue;
        }
        plain.push_str(t);
        i += 1;
    }
    flush(&mut plain, out);
}

/// The key of a pack's glyphs or an item of a message's run (HudLayer icons 21/22: the message's id, the run).
pub fn key(id: i32, run: usize) -> f32 {
    ((id.rem_euclid(32768)) * 256 + run.min(255) as i32) as f32
}

/// A server's spans as atoms: its glyphs (Java's), its items, its clickable words, the words' own colours.
#[allow(clippy::too_many_arguments)]
fn spans_into(out: &mut Vec<Atom>, spans: &[Span], id: i32, runs: Option<&Vec<Run>>, look: &Look, tint: Option<u32>, default: Rgba, light_me: bool) {
    for sp in spans {
        let run = runs.and_then(|r| r.get(sp.run.max(0) as usize));
        if sp.flags & F_GLYPH != 0 || (!sp.text.is_empty() && sp.text.chars().all(|c| rich::packs(c) || c == ' ')) {
            let gw = run.map_or(8.0, |r| {
                let n = r.text.chars().count().max(1) as f32;
                r.gw * sp.text.chars().count() as f32 / n
            });
            out.push(Atom { ak: Ak::Glyph(key(id, sp.run.max(0) as usize), gw * look.gu), gap: look.size * 0.2, ..Atom::words("", Face::Regular, look.size, look.base) });
            continue;
        }
        let col = look.tone(sp.colour.or(tint), default);
        let face = if sp.flags & rich::BOLD != 0 { Face::Semi } else { Face::Regular };
        let tap = (sp.click >= 0).then_some(Tap::Game(id, sp.click as usize));
        if run.is_some_and(|r| r.fl & F_ITEM != 0) {
            out.push(Atom {
                text: sp.text.trim().to_string(),
                face: Face::Mono,
                size: look.size * 0.8,
                col,
                ak: Ak::Item(key(id, sp.run.max(0) as usize)),
                fl: 0,
                tap,
                gap: look.size * 0.3,
                found: false,
                hover: run.map(|r| r.hover.clone()).filter(|h| !h.is_empty()),
            });
            continue;
        }
        let from = out.len();
        words_into(out, &sp.text, col, face, look, sp.flags & (F_UNDER | F_STRIKE | F_OBF), tap, light_me);
        if let Some(h) = run.map(|r| r.hover.clone()).filter(|h| !h.is_empty()) {
            for a in out[from..].iter_mut() {
                a.hover = Some(h.clone());
            }
        }
    }
}

impl Store {
    /// What a row shows: a player's line, a server's quiet word, a card. `cont`: a player's line going on from the
    /// one before (its words only).
    pub fn body(&self, r: &Row, look: &Look, cont: bool) -> Body {
        let k = look.k;
        let size = look.size;
        let t2 = look.dim;
        let first_id = r.lines.first().map_or(r.id, |l| l.0);
        let runs = |id: i32| self.runs.get(&id);
        let sys = |mark: Mark, label: Option<&str>, words: &[Span]| {
            let mut a = Vec::new();
            if let Some(l) = label.filter(|l| !l.is_empty()) {
                a.push(Atom::words(&l.to_uppercase(), Face::Mono, size * 0.7, fade(look.dim, 0.75)).gap(8.0 * k));
            }
            spans_into(&mut a, words, first_id, runs(first_id), look, None, t2, true);
            Body::Sys { mark, atoms: a }
        };
        match &r.kind {
            Said::Chat(c) => {
                let mut a = Vec::new();
                let id = first_id;
                if !cont {
                    if let Some(bc) = &c.bc {
                        a.push(Atom { ak: Ak::Tag, ..Atom::words(bc, Face::Mono, size * 0.78, look.base) }.gap(6.0 * k));
                    }
                    // the rank's badge (the pack's glyph, as the server sent it)
                    if let Some(g) = c.rank_glyph.filter(|_| look.ranks) {
                        if let Some((lid, sp)) = r.lines.iter().flat_map(|(lid, l)| l.iter().map(move |s| (*lid, s))).find(|(_, s)| s.text.contains(g)) {
                            let gw = runs(lid).and_then(|v| v.get(sp.run.max(0) as usize)).map_or(8.0, |run| {
                                let n = run.text.chars().count().max(1) as f32;
                                run.gw / n
                            });
                            a.push(Atom { ak: Ak::Glyph(key(lid, sp.run.max(0) as usize), gw * look.gu), gap: 6.0 * k, ..Atom::words("", Face::Regular, size, look.base) });
                        }
                    }
                    if let Some((w, col, dimmed)) = c.rank_word.as_ref().filter(|_| look.ranks) {
                        let col = look.tone(*col, look.base);
                        a.push(Atom::words(w, Face::Semi, size * 0.86, if *dimmed { fade(col, 0.6) } else { col }).gap(6.0 * k));
                    }
                    if let Some((w, col, bold)) = c.prefix.as_ref().filter(|_| look.ranks) {
                        a.push(Atom::words(w, if *bold { Face::Semi } else { Face::Regular }, size * 0.86, look.tone(*col, look.base)).gap(6.0 * k));
                    }
                    if let Some((clan, col)) = &c.clan {
                        a.push(Atom::words(clan, Face::Mono, size * 0.78, look.tone(*col, look.dim)).gap(6.0 * k));
                    }
                    a.push(Atom { tap: Some(Tap::Field(String::new())), ..look.nick(&c.player) }.gap(if c.plus { 2.0 * k } else { 7.0 * k }));
                    if c.plus {
                        a.push(Atom::words("+", Face::Semi, size, hexc(0xffb000)).gap(7.0 * k));
                    }
                    if let Some((ttl, col)) = &c.title {
                        a.push(Atom::words(ttl, Face::Regular, size * 0.82, look.tone(*col, fade(look.dim, 0.8))).gap(7.0 * k));
                    }
                    if c.ch == Some(Chan::Local) {
                        a.push(Atom::words(&tr("near").to_lowercase(), Face::Mono, size * 0.7, fade(look.dim, 0.7)).gap(7.0 * k));
                    }
                    if let Some(last) = a.last_mut() {
                        last.gap = 8.0 * k;
                    }
                }
                spans_into(&mut a, &c.text, id, runs(id), look, c.tint, look.base, !r.mine);
                Body::Line { face: (!cont).then(|| c.player.clone()), atoms: a }
            }
            Said::Whisper { from, text, mine } => {
                let mut a = Vec::new();
                if *mine {
                    a.push(Atom::words(tr("you"), Face::Semi, size, look.base).gap(6.0 * k));
                    a.push(Atom { ak: Ak::Tag, ..Atom::words(&format!("→ {from}"), Face::Mono, size * 0.78, look.base) }.gap(8.0 * k));
                } else {
                    a.push(look.nick(from).gap(6.0 * k));
                    a.push(Atom { ak: Ak::Tag, ..Atom::words(&format!("→ {}", tr("you")), Face::Mono, size * 0.78, look.base) }.gap(8.0 * k));
                }
                spans_into(&mut a, text, first_id, runs(first_id), look, None, look.base, !*mine);
                Body::Line { face: Some(if *mine { self.me.clone() } else { from.clone() }), atoms: a }
            }
            Said::Status { tone, label, words } => sys(Mark::Dot(*tone), label.as_deref(), words),
            Said::Section { name, words, .. } => sys(Mark::Dot(parse::section_tone(words)), Some(name), words),
            Said::Remind { text, cmds } => {
                let mut a = Vec::new();
                words_into(&mut a, text, t2, Face::Regular, look, 0, None, true);
                for c in cmds {
                    a.push(chip(c.clone(), 3, Tap::Field(format!("{c} ")), true, look));
                }
                Body::Sys { mark: Mark::Dot(Tone::Info), atoms: a }
            }
            Said::Kill { victim, text } => {
                let mut a = vec![Atom::words(victim, Face::Semi, size * 0.95, look.base).gap(6.0 * k)];
                words_into(&mut a, text, t2, Face::Regular, look, 0, None, true);
                Body::Sys { mark: Mark::Icon(5), atoms: a }
            }
            Said::Promo { text, link } => {
                let mut a = Vec::new();
                words_into(&mut a, text, t2, Face::Regular, look, 0, None, true);
                if let Some(l) = link {
                    let (kind, tap) = if l.starts_with('/') { (3, Tap::Field(format!("{l} "))) } else { (2, Tap::Copy(l.clone(), "Link copied")) };
                    a.push(chip(l.trim_start_matches("https://").trim_start_matches("http://").to_string(), kind, tap, false, look));
                }
                Body::Sys { mark: Mark::Dot(Tone::Info), atoms: a }
            }
            Said::Boss { name, text } => {
                let mut a = vec![Atom::words(name, Face::Semi, size * 0.95, look.base).gap(6.0 * k)];
                words_into(&mut a, text, t2, Face::Regular, look, 0, None, true);
                Body::Sys { mark: Mark::Dot(Tone::Bad), atoms: a }
            }
            Said::Win { player, prize, from } => {
                let mut a = Vec::new();
                if player.eq_ignore_ascii_case(&self.me) {
                    a.push(Atom::words(tr("You won"), Face::Semi, size * 0.95, look.base).gap(6.0 * k));
                } else {
                    a.push(Atom::words(player, Face::Semi, size * 0.95, look.base).gap(5.0 * k));
                    a.push(Atom::words(tr("won"), Face::Regular, size * 0.95, t2).gap(6.0 * k));
                }
                let what = match prize {
                    Prize::Rank(rk) => format!("{} {rk}", tr("rank")),
                    Prize::Money(m) => format!("{m} {}", tr("coins")),
                    Prize::Text(tx) => tx.clone(),
                };
                a.push(Atom { ak: Ak::Chip(0), ..Atom::words(&what, Face::Mono, size * 0.8, look.base) }.gap(6.0 * k));
                if !from.is_empty() {
                    a.push(Atom::words(&format!("· {from}"), Face::Regular, size * 0.9, fade(look.dim, 0.7)));
                }
                Body::Sys { mark: Mark::Icon(6), atoms: a }
            }
            Said::Airdrop { title, loot, more, x, z, .. } => {
                let ttl = if title.is_empty() { tr("Airdrop").to_string() } else { title.clone() };
                let mut sub = Vec::new();
                let mut s = loot.iter().map(|(n, c)| format!("{n} ×{c}")).collect::<Vec<_>>().join(" · ");
                if *more > 0 {
                    s.push_str(&format!(" · {} {more}", tr("and more")));
                }
                words_into(&mut sub, &s, t2, Face::Regular, look, 0, None, true);
                let at = format!("{x} {z}");
                let title = vec![Atom::words(&ttl, Face::Semi, size * 0.97, look.base).gap(8.0 * k), place_chip(&at, look)];
                Body::Card { icon: Icon::Item(1), title, sub, btns: Vec::new(), fuse: false, state: None, done: false }
            }
            Said::Event { ev, state, pos, player, clan, wave, stats, enemy, cmd, text, .. } => {
                use parse::EvState;
                let mut sub = Vec::new();
                let b = |w: &str| Atom::words(w, Face::Medium, size * 0.88, look.base).gap(5.0 * k);
                let p = |w: &str| Atom::words(w, Face::Regular, size * 0.88, t2).gap(5.0 * k);
                match state {
                    EvState::Summoned => {
                        sub.push(p(tr("Summoned by")));
                        sub.push(b(player));
                    }
                    EvState::Opened => {
                        sub.push(b(player));
                        sub.push(p(tr("opened it")));
                    }
                    EvState::Taken => {
                        sub.push(p(tr("Clan")));
                        sub.push(b(clan));
                        sub.push(p(tr("took it")));
                        let st = stats.iter().map(|(n, v)| format!("{} {v}", n.to_lowercase())).collect::<Vec<_>>().join(" · ");
                        if !st.is_empty() {
                            sub.push(p(&format!("· {st}")));
                        }
                    }
                    EvState::Wave => sub.push(p(&format!("{} {wave}", tr("Wave")))),
                    EvState::Clear => sub.push(p(tr("Cleared"))),
                    EvState::Appear => {
                        if !enemy.is_empty() {
                            sub.push(b(enemy));
                        }
                        words_into(&mut sub, text, t2, Face::Regular, look, 0, None, true);
                        if !cmd.is_empty() {
                            sub.push(chip(cmd.clone(), 3, Tap::Field(format!("{cmd} ")), false, look));
                        }
                    }
                }
                let mut title = vec![Atom::words(ev.name(), Face::Semi, size * 0.97, look.base).gap(8.0 * k)];
                if let Some((x, y, z)) = pos {
                    let at = match y {
                        Some(y) => format!("{x} {y} {z}"),
                        None => format!("{x} {z}"),
                    };
                    title.push(place_chip(&at, look));
                }
                let icon = match ev {
                    parse::Ev::Beacon => 0,
                    parse::Ev::Airdrop => 1,
                    parse::Ev::Mine => 2,
                    parse::Ev::Talisman => 3,
                    parse::Ev::Merchant => 4,
                    parse::Ev::Siege => 5,
                };
                Body::Card { icon: Icon::Item(icon), title, sub, btns: Vec::new(), fuse: false, state: None, done: matches!(state, EvState::Opened | EvState::Clear) }
            }
            Said::Request { rq, from, secs, kit, stake, clan, .. } => {
                let what = match rq {
                    Rq::Tpa => tr("wants to teleport to you").to_string(),
                    Rq::TpaHere => tr("calls you over").to_string(),
                    Rq::Trade => tr("offers a trade").to_string(),
                    Rq::Duel => tr("challenges you to a duel").to_string(),
                    Rq::Clan => format!("{} {clan}", tr("invites you to the clan")),
                };
                let title = vec![
                    Atom { ak: Ak::Nick, ..Atom::words(from, Face::Semi, size * 0.97, look.base) }.gap(6.0 * k),
                    Atom::words(&what, Face::Regular, size * 0.97, t2),
                ];
                let s = if *rq == Rq::Duel {
                    format!("{} {kit} · {} {stake}", tr("Kit"), tr("stake"))
                } else if *secs > 0 {
                    // how long it waits (what is left is the line under it: the words are not counted down)
                    let dur = if *secs >= 60 && secs % 60 == 0 { format!("{} {}", secs / 60, tr("min")) } else { format!("{secs} {}", tr("s")) };
                    format!("{} {dur}", tr("Waits"))
                } else {
                    tr("Answer when you like").to_string()
                };
                let sub = vec![Atom::words(&s, Face::Regular, size * 0.86, fade(look.dim, 0.8))];
                let expired = r.secs > 0.0 && self.now > r.until && r.state == 0;
                let state = match r.state {
                    1 => Some(tr("accepted").to_string()),
                    2 => Some(tr("declined").to_string()),
                    _ if expired => Some(tr("expired").to_string()),
                    _ => None,
                };
                let mut btns = Vec::new();
                if state.is_none() {
                    btns.push(Btn { label: tr("Decline").to_string(), style: 2, which: 1 });
                    btns.push(Btn { label: tr("Accept").to_string(), style: 1, which: 0 });
                }
                Body::Card { icon: Icon::Face(from.clone()), title, sub, btns, fuse: *secs > 0 && state.is_none(), state, done: r.state != 0 || expired }
            }
            Said::Tp { .. } => {
                let done = r.state == 3;
                Body::Card {
                    icon: Icon::Item(7),
                    title: vec![Atom::words(tr("Teleporting"), Face::Semi, size * 0.97, look.base)],
                    sub: vec![Atom::words(tr("Don't move or take damage"), Face::Regular, size * 0.86, fade(look.dim, 0.8))],
                    btns: Vec::new(),
                    fuse: !done && self.now < r.until,
                    state: done.then(|| tr("done").to_string()),
                    done,
                }
            }
            Said::Restart { .. } => {
                let done = r.state == 3;
                Body::Card {
                    icon: Icon::Item(8),
                    title: vec![Atom::words(tr("Server restart"), Face::Semi, size * 0.97, look.base)],
                    sub: vec![Atom::words(tr("The server will restart — finish what you're doing"), Face::Regular, size * 0.86, fade(look.dim, 0.8))],
                    btns: Vec::new(),
                    fuse: !done && self.now < r.until,
                    state: done.then(|| tr("called off").to_string()),
                    done,
                }
            }
            Said::Greeting { title, cmds, links } => {
                let mut sub = Vec::new();
                for c in cmds {
                    sub.push(chip(c.clone(), 3, Tap::Field(format!("{c} ")), true, look));
                }
                for l in links {
                    sub.push(chip(l.clone(), 2, Tap::Copy(l.clone(), "Link copied"), true, look));
                }
                Body::Card { icon: Icon::Item(6), title: vec![Atom::words(title, Face::Semi, size * 0.97, look.base)], sub, btns: Vec::new(), fuse: false, state: None, done: false }
            }
            Said::Check { via, link, ask, .. } => {
                let mut sub = Vec::new();
                let mut s = via.clone();
                if let Some(a) = ask {
                    s.push_str(&format!(" · {a}"));
                }
                words_into(&mut sub, &s, t2, Face::Regular, look, 0, None, true);
                if let Some(l) = link {
                    sub.push(chip(l.clone(), 2, Tap::Copy(l.clone(), "Link copied"), false, look));
                }
                Body::Card {
                    icon: Icon::Item(6),
                    title: vec![Atom::words(tr("Moderator check"), Face::Semi, size * 0.97, look.base)],
                    sub,
                    btns: Vec::new(),
                    fuse: r.secs > 0.0 && self.now < r.until,
                    state: None,
                    done: false,
                }
            }
            Said::CheckMsg { text } | Said::CheckEnd { text } => {
                let mut a = Vec::new();
                words_into(&mut a, text, t2, Face::Regular, look, 0, None, true);
                Body::Sys { mark: Mark::Dot(Tone::Bad), atoms: a }
            }
            _ => {
                // what no rule knows: as the server sent it (its colours, its glyphs, its clicks), a face before it
                // when Java knew whose it is
                let mut a = Vec::new();
                for (li, (lid, l)) in r.lines.iter().enumerate() {
                    if li > 0 {
                        a.push(Atom::words("\n", Face::Regular, size, look.base));
                    }
                    spans_into(&mut a, l, *lid, runs(*lid), look, None, look.base, !r.mine && r.cat == 1);
                }
                if r.who.is_empty() || r.cat != 1 {
                    Body::Sys { mark: Mark::Dot(Tone::Info), atoms: a }
                } else {
                    Body::Line { face: (!cont).then(|| r.who.clone()), atoms: a }
                }
            }
        }
    }
}

// ------------------------------------------------------------------------------------------------ laying out

pub fn chip_pad(a: &Atom, k: f32) -> (f32, f32) {
    match a.ak {
        Ak::Chip(1) | Ak::Chip(2) => (20.0 * k, 7.0 * k),
        Ak::Chip(_) | Ak::Tag => (7.0 * k, 7.0 * k),
        Ak::Item(_) => (24.0 * k, 7.0 * k),
        Ak::Me => (4.0 * k, 4.0 * k),
        _ => (0.0, 0.0),
    }
}

/// Atoms into lines `max` wide: words break between them (a too long one by letters), the rest whole.
pub fn lay(atoms: &[Atom], max: f32, k: f32, indent: f32) -> (Vec<Placed>, usize) {
    let f = fonts();
    let mut out = Vec::new();
    let mut line = 0;
    let mut x = 0.0f32;
    // (the indent is never more than half of the room; the first line has none)
    let indent = indent.min(max * 0.5);
    let from = |line: usize| if line == 0 { 0.0 } else { indent };
    for (ai, a) in atoms.iter().enumerate() {
        if a.text == "\n" {
            line += 1;
            x = indent;
            continue;
        }
        match a.ak {
            Ak::Words => {
                let mut tok = String::new();
                let mut toks: Vec<String> = Vec::new();
                for ch in a.text.chars() {
                    if ch == '\n' {
                        if !tok.is_empty() {
                            toks.push(std::mem::take(&mut tok));
                        }
                        toks.push("\n".into());
                        continue;
                    }
                    tok.push(ch);
                    if ch == ' ' {
                        toks.push(std::mem::take(&mut tok));
                    }
                }
                if !tok.is_empty() {
                    toks.push(tok);
                }
                for t in toks {
                    if t == "\n" {
                        line += 1;
                        x = indent;
                        continue;
                    }
                    let w = f.width(a.face, a.size, &t);
                    // (a trailing space may hang past the edge)
                    let w_seen = if t.ends_with(' ') { f.width(a.face, a.size, t.trim_end()) } else { w };
                    if x + w_seen > max && x > from(line) && !t.trim().is_empty() {
                        line += 1;
                        x = indent;
                    }
                    if w_seen > max - from(line) {
                        let mut cur = String::new();
                        for ch in t.chars() {
                            let mut next = cur.clone();
                            next.push(ch);
                            if f.width(a.face, a.size, &next) + x > max && !cur.is_empty() {
                                let cw = f.width(a.face, a.size, &cur);
                                out.push(Placed { ai, line, x, text: std::mem::take(&mut cur), w: cw });
                                line += 1;
                                x = indent;
                                next = ch.to_string();
                            }
                            cur = next;
                        }
                        if !cur.is_empty() {
                            let cw = f.width(a.face, a.size, &cur);
                            out.push(Placed { ai, line, x, text: cur, w: cw });
                            x += cw;
                        }
                        continue;
                    }
                    if x <= from(line) && t.trim().is_empty() {
                        continue;
                    }
                    out.push(Placed { ai, line, x, text: t, w });
                    x += w;
                }
                x += a.gap;
            }
            _ => {
                let (pl, pr) = chip_pad(a, k);
                let w = match a.ak {
                    Ak::Glyph(_, gw) => gw,
                    _ => f.width(a.face, a.size, &a.text) + pl + pr,
                };
                if x + w > max && x > from(line) {
                    line += 1;
                    x = indent;
                }
                out.push(Placed { ai, line, x, text: a.text.clone(), w });
                x += w + a.gap;
            }
        }
    }
    let lines = out.iter().map(|p| p.line + 1).max().unwrap_or(1);
    (out, lines)
}

pub fn btn_w(b: &Btn, look: &Look) -> f32 {
    fonts().width(Face::Medium, look.size * 0.86, &b.label) + 24.0 * look.k
}

/// The room before a row's words: the face's (or the server line's mark's).
pub fn gutter(look: &Look) -> f32 {
    if look.heads {
        look.head + 8.0 * look.k
    } else {
        18.0 * look.k
    }
}

/// A row laid in `w` (its whole width, the room before its words counted).
pub fn lay_row(body: Body, w: f32, look: &Look, cont: bool) -> Laid {
    let k = look.k;
    let pad_y = if cont { 1.0 * k } else { 4.0 * k };
    match body {
        Body::Line { face, atoms } => {
            let gut = gutter(look);
            let (a, n) = lay(&atoms, (w - gut).max(40.0 * k), k, look.indent);
            let end = a.iter().filter(|p| p.line + 1 == n).last().map_or([gut, pad_y], |p| [gut + p.x + p.w, pad_y + (n - 1) as f32 * look.lh]);
            Laid { h: n as f32 * look.lh + pad_y + 4.0 * k, body: Body::Line { face, atoms }, a, a_lines: n, b: Vec::new(), b_lines: 0, end, cont }
        }
        Body::Sys { mark, atoms } => {
            let gut = gutter(look);
            let (a, n) = lay(&atoms, (w - gut).max(40.0 * k), k, look.indent);
            let lh = look.lh * 0.96;
            let end = a.iter().filter(|p| p.line + 1 == n).last().map_or([gut, 4.0 * k], |p| [gut + p.x + p.w, 4.0 * k + (n - 1) as f32 * lh]);
            Laid { h: n as f32 * lh + 8.0 * k, body: Body::Sys { mark, atoms }, a, a_lines: n, b: Vec::new(), b_lines: 0, end, cont: false }
        }
        Body::Card { icon, title, sub, btns, fuse, state, done } => {
            let bw: f32 = btns.iter().map(|b| btn_w(b, look) + 6.0 * k).sum::<f32>()
                + state.as_ref().map_or(0.0, |s| fonts().width(Face::Mono, look.size * 0.76, s) + 10.0 * k);
            let tw = (w - 24.0 * k - 50.0 * k - bw).max(80.0 * k);
            let (a, an) = lay(&title, tw, k, 0.0);
            let (b, bn) = if sub.is_empty() { (Vec::new(), 0) } else { lay(&sub, tw, k, 0.0) };
            let text_h = an as f32 * look.lh + bn as f32 * look.lh * 0.88;
            let h = text_h.max(38.0 * k) + 22.0 * k + if fuse { 10.0 * k } else { 0.0 } + 8.0 * k;
            Laid { h, body: Body::Card { icon, title, sub, btns, fuse, state, done }, a, a_lines: an, b, b_lines: bn, end: [0.0; 2], cont: false }
        }
    }
}

impl Store {
    /// The row at `i` laid for `w` and the look (kept: made again only when what it depends on changed); `cont` it
    /// goes on from the row before.
    pub fn laid(&mut self, i: usize, w: f32, look: &Look, look_key: u64, cont: bool) -> &Laid {
        let r = &self.rows[i];
        let expired = r.secs > 0.0 && self.now > r.until && r.state == 0;
        let mut h = look_key ^ (w.to_bits() as u64).wrapping_mul(0x9e3779b97f4a7c15);
        h = h.wrapping_mul(0x100000001b3) ^ ((r.version as u64) << 1 | cont as u64) ^ ((expired as u64) << 40);
        let fresh = !matches!(&r.laid, Some((k, _)) if *k == h);
        if fresh {
            let body = self.body(&self.rows[i], look, cont);
            let laid = lay_row(body, w, look, cont);
            self.rows[i].laid = Some((h, laid));
        }
        &self.rows[i].laid.as_ref().unwrap().1
    }
}
