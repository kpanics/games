//! Development (CURSED_DEV_GUI): a script the menu plays on itself once it opens, because the test stand's window
//! takes no input. Steps split by ';', each "seconds action [target] [numbers]", the seconds counted from the opening:
//!
//!   move <target>             the pointer to the target's middle
//!   left <target>, right ...  a click there
//!   open <module id>          its settings open (left so if they are: scripts play again at each opening)
//!   drag <target> dx dy       pressed there, moved by dx dy px over half a second, let go
//!   drag2 <target> <target>   pressed at the one, moved to the other over half a second, let go there
//!   key <name>                a key by its name (escape, enter, backspace, or as SDL names it: "Right Shift")
//!   type <text>               letters typed
//!   scroll <dy>               the wheel
//!   cosm <item> [<paint>]     the Cosmetics page: that item's finishes, or the look at one ("-" its vanilla)
//!   xpaste <code>             a crosshair's code as if pasted (the clipboard left alone)
//!
//! Targets: nav:<Section>, tile:<module id>, switch:<module id>, expand:<module id>, panel-switch, close, header,
//! seg:<n>, search, toggle:<n>, slider:<n>, action:<n> (a button row), card:<part>,<n> (the card open), cosm:<part>,<n>
//! (the Cosmetics page's), at:<x>,<y> (window px). While a
//! script runs the game's own pointer is not heard. Each step is written to the log with the time, and the opening
//! with the wall clock (for the stand's shots).

use super::data::{Cat, CLIENT_CATS, MODULE_CATS};
use super::{Gui, Hit};

#[derive(Default)]
pub struct Script {
    steps: Vec<(f32, String)>,
    next: usize,
    drag: Option<([f32; 2], [f32; 2], f32)>,
}

fn cat_named(n: &str) -> Option<Cat> {
    MODULE_CATS.iter().chain(CLIENT_CATS.iter()).copied().find(|c| c.name().eq_ignore_ascii_case(n))
}

impl Script {
    pub fn from_env() -> Option<Script> {
        let text = std::env::var("CURSED_DEV_GUI").ok()?;
        let mut steps = Vec::new();
        for s in text.split(';').map(str::trim).filter(|s| !s.is_empty()) {
            let (t, rest) = s.split_once(' ')?;
            steps.push((t.parse().ok()?, rest.trim().to_string()));
        }
        Some(Script { steps, next: 0, drag: None })
    }

    /// The HUD's script (CURSED_DEV_HUD), played from the layout editor's opening (hud_dev_run).
    pub fn hud_from_env() -> Option<Script> {
        let text = std::env::var("CURSED_DEV_HUD").ok()?;
        let mut steps = Vec::new();
        for s in text.split(';').map(str::trim).filter(|s| !s.is_empty()) {
            let (t, rest) = s.split_once(' ')?;
            steps.push((t.parse().ok()?, rest.trim().to_string()));
        }
        Some(Script { steps, next: 0, drag: None })
    }

    pub fn restart(&mut self) {
        self.next = 0;
        self.drag = None;
    }
}

impl Gui {
    fn target(&self, spec: &str) -> Option<[f32; 2]> {
        let (kind, arg) = spec.split_once(':').unwrap_or((spec, ""));
        let module = |id: &str| self.mods.iter().position(|m| m.id == id);
        let want: Box<dyn Fn(&Hit) -> bool> = match kind {
            "nav" => {
                let c = cat_named(arg)?;
                Box::new(move |h| *h == Hit::Nav(c))
            }
            "tile" => {
                let i = module(arg)?;
                Box::new(move |h| *h == Hit::Tile(i))
            }
            "switch" => {
                let i = module(arg)?;
                Box::new(move |h| *h == Hit::TileSwitch(i))
            }
            "expand" => {
                let i = module(arg)?;
                Box::new(move |h| *h == Hit::TileExpand(i))
            }
            "panel-switch" => Box::new(|h| matches!(h, Hit::PanelSwitch(_))),
            "close" => Box::new(|h| *h == Hit::PanelClose),
            "header" => Box::new(|h| *h == Hit::Header),
            "editlayout" => Box::new(|h| *h == Hit::EditLayout),
            "search" => Box::new(|h| *h == Hit::Search),
            "seg" => {
                let n: usize = arg.parse().ok()?;
                Box::new(move |h| *h == Hit::Seg(n))
            }
            "toggle" => {
                let n: usize = arg.parse().ok()?;
                Box::new(move |h| *h == Hit::Toggle(n))
            }
            "slider" => {
                let n: usize = arg.parse().ok()?;
                Box::new(move |h| *h == Hit::Slider(n))
            }
            "at" => {
                let (x, y) = arg.split_once(',')?;
                return Some([x.parse().ok()?, y.parse().ok()?]);
            }
            // the swing's editor's parts (swingui.rs: part, number)
            "swing" => {
                let (a, b) = arg.split_once(',').unwrap_or((arg, "0"));
                let (a, b): (u8, u16) = (a.parse().ok()?, b.parse().ok()?);
                let q = self.swing_ed.hits.iter().find(|h| h.0 == a && h.1 == b)?.2;
                return Some([(q[0] + q[2]) / 2.0, (q[1] + q[3]) / 2.0]);
            }
            "elem" => {
                let i = module(arg)?;
                Box::new(move |h| *h == Hit::HudElem(i))
            }
            "colour" => {
                let n: usize = arg.parse().ok()?;
                Box::new(move |h| *h == Hit::Colour(n))
            }
            "choice" => {
                let (a, b) = arg.split_once(',')?;
                let (a, b): (usize, usize) = (a.parse().ok()?, b.parse().ok()?);
                Box::new(move |h| *h == Hit::Choice(a, b))
            }
            "pick" => {
                let (a, b) = arg.split_once(',').unwrap_or((arg, "0"));
                let (a, b): (u8, u8) = (a.parse().ok()?, b.parse().ok()?);
                Box::new(move |h| *h == Hit::Pick(a, b))
            }
            "action" => {
                let n: usize = arg.parse().ok()?;
                Box::new(move |h| *h == Hit::Action(n))
            }
            "reset" => {
                let n: usize = arg.parse().ok()?;
                Box::new(move |h| *h == Hit::Reset(n))
            }
            "text" => {
                let n: usize = arg.parse().ok()?;
                Box::new(move |h| *h == Hit::TextRow(n))
            }
            "binds" => Box::new(|h| *h == Hit::BindsBtn),
            "langbtn" => Box::new(|h| *h == Hit::LangBtn),
            "altbtn" => Box::new(|h| *h == Hit::AltBtn),
            "cfg" => {
                let (a, b) = arg.split_once(',').unwrap_or((arg, "0"));
                let (a, b): (u8, u16) = (a.parse().ok()?, b.parse().ok()?);
                Box::new(move |h| *h == Hit::Cfg(a, b))
            }
            "hudpick" => {
                let i = module(arg)?;
                Box::new(move |h| *h == Hit::HudPick(i))
            }
            "tilebind" => {
                let i = module(arg)?;
                Box::new(move |h| *h == Hit::TileBind(i))
            }
            "panelbinds" => Box::new(|h| matches!(h, Hit::PanelBinds(_))),
            "rowbind" => {
                let n: usize = arg.parse().ok()?;
                Box::new(move |h| *h == Hit::RowBind(n))
            }
            "card" => {
                let (a, b) = arg.split_once(',').unwrap_or((arg, "0"));
                let (a, b): (u8, u16) = (a.parse().ok()?, b.parse().ok()?);
                Box::new(move |h| *h == Hit::Card(a, b))
            }
            "cosmst" => return self.sticker_screen(arg.parse().ok()?),
            "cosm" => {
                let (a, b) = arg.split_once(',').unwrap_or((arg, "0"));
                let (a, b): (u8, u16) = (a.parse().ok()?, b.parse().ok()?);
                Box::new(move |h| *h == Hit::Cosm(a, b))
            }
            _ => return None,
        };
        let (r, _) = self.hits.iter().rev().find(|(_, h)| want(h))?;
        // the header's strip: its middle would be under the title; take a point left of the sections' rail end
        if kind == "header" {
            return Some([r[0] + (r[2] - r[0]) * 0.5, r[1] + 1.2 * self.r]);
        }
        Some([(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0])
    }

    /// Plays the steps that are due (called at the start of a frame, on the last frame's layout).
    pub(super) fn dev_run(&mut self) {
        let Some(mut s) = self.dev.take() else { return };
        let t = self.now - self.opened_at;
        if let Some((from, d, t0)) = s.drag {
            let k = ((t - t0) / 0.5).clamp(0.0, 1.0);
            self.mouse_move(from[0] + d[0] * k, from[1] + d[1] * k);
            if k >= 1.0 {
                self.release();
                s.drag = None;
            }
        }
        while s.next < s.steps.len() && s.steps[s.next].0 <= t {
            let step = s.steps[s.next].1.clone();
            s.next += 1;
            eprintln!("[gui dev] {t:.2} {step}");
            let mut words = step.split_whitespace();
            let action = words.next().unwrap_or("");
            let rest: Vec<&str> = words.collect();
            let at = rest.first().and_then(|w| self.target(w));
            match (action, at) {
                ("move", Some(p)) => self.mouse_move(p[0], p[1]),
                ("left", Some(p)) | ("right", Some(p)) | ("middle", Some(p)) => {
                    let b = match action {
                        "left" => 0,
                        "right" => 1,
                        _ => 2,
                    };
                    self.mouse_move(p[0], p[1]);
                    self.press(p[0], p[1], b);
                    self.release();
                }
                ("keyup", _) => self.bind_key_up(&rest.join(" ")),
                ("selfcode", _) => {
                    // the client's own code pasted back (a round trip of the codes, the clipboard left alone)
                    let code = super::configs::encode(&self.snapshot("Round trip", super::configs::P_ALL));
                    eprintln!("[gui dev] code of {} characters", code.len());
                    self.paste_for = super::configs::P_ALL;
                    self.paste_code(&code);
                }
                ("chars", _) => {
                    for c in rest.join(" ").chars() {
                        self.char_typed(c);
                    }
                }
                ("lang", _) => {
                    // lang <code>: the language card's choice (the language goes through Java as in the game)
                    if let Some(li) = self.langs.iter().position(|l| rest.first() == Some(&l.0.as_str())) {
                        self.lang_choose(li);
                    } else {
                        eprintln!("[gui dev] no language {:?} among {}", rest.first(), self.langs.len());
                    }
                }
                ("world", _) => {
                    // world <1|0> <key name>: a key down or up in the world (the binds)
                    let down = rest.first() == Some(&"1");
                    let name = rest[1..].join(" ");
                    let fired = self.world_key(&name, down);
                    eprintln!("[gui dev] world {name} {down}: {fired}");
                }
                // drag2 <from> <to>: pressed at one target, let go at another
                ("drag2", Some(p)) => {
                    let Some(q) = rest.get(1).and_then(|w| self.target(w)) else {
                        eprintln!("[gui dev] no target for: {step}");
                        continue;
                    };
                    self.mouse_move(p[0], p[1]);
                    self.press(p[0], p[1], 0);
                    s.drag = Some((p, [q[0] - p[0], q[1] - p[1]], t));
                }
                // (rdrag: held by the right button)
                ("drag", Some(p)) | ("rdrag", Some(p)) => {
                    let dx = rest.get(1).and_then(|v| v.parse().ok()).unwrap_or(0.0);
                    let dy = rest.get(2).and_then(|v| v.parse().ok()).unwrap_or(0.0);
                    self.mouse_move(p[0], p[1]);
                    self.press(p[0], p[1], if action == "rdrag" { 1 } else { 0 });
                    s.drag = Some((p, [dx, dy], t));
                }
                ("key", _) => {
                    let name = rest.join(" ");
                    let n = name.to_lowercase();
                    let menu = name == self.menu_key_name();
                    self.key(n == "escape", n == "enter", n == "backspace", name, menu);
                }
                ("type", _) => {
                    for c in rest.join(" ").chars() {
                        self.char_typed(c);
                    }
                }
                ("open", _) => {
                    if let Some(i) = rest.first().and_then(|id| self.mods.iter().position(|m| m.id == *id)) {
                        if self.open != Some(i) {
                            self.expand(i);
                        }
                    }
                }
                ("scroll", _) => self.wheel(rest.first().and_then(|v| v.parse().ok()).unwrap_or(0.0)),
                ("cosm", _) => self.cosm_dev(rest.first().copied().unwrap_or(""), rest.get(1).copied()),
                ("xpaste", _) => self.cosm_paste(&rest.join(" ")),
                ("icode", _) => self.item_code_said(),
                ("ipaste", _) => self.item_paste(&rest.join(" ")),
                _ => eprintln!("[gui dev] no target for: {step}"),
            }
        }
        self.dev = Some(s);
    }
}

impl Gui {
    /// The HUD's script (CURSED_DEV_HUD), from the editor's opening: the same steps and targets as the menu's (and
    /// elem:<id>, colour:<n>, pick:<part>,<n>, choice:<n>,<m>, action:<n>, reset:<n>), done by the HUD's input.
    pub(super) fn hud_dev_run(&mut self, since: f32) {
        let Some(mut s) = self.hud_dev.take() else { return };
        if let Some((from, d, t0)) = s.drag {
            let k = ((since - t0) / 0.5).clamp(0.0, 1.0);
            self.hud.dev_mouse = Some([from[0] + d[0] * k, from[1] + d[1] * k]);
            if k >= 1.0 {
                self.hud_mouse(from[0] + d[0], from[1] + d[1], 0, 2);
                s.drag = None;
            }
        }
        while s.next < s.steps.len() && s.steps[s.next].0 <= since {
            let step = s.steps[s.next].1.clone();
            s.next += 1;
            eprintln!("[hud dev] {since:.2} {step}");
            let mut words = step.split_whitespace();
            let action = words.next().unwrap_or("");
            let rest: Vec<&str> = words.collect();
            let at = rest.first().and_then(|w| self.in_hud_ref(|g| g.target(w)));
            match (action, at) {
                ("move", Some(p)) => self.hud.dev_mouse = Some(p),
                ("left", Some(p)) | ("right", Some(p)) => {
                    let b = if action == "left" { 0 } else { 1 };
                    self.hud.dev_mouse = Some(p);
                    self.hud_mouse(p[0], p[1], 0, 0);
                    self.hud_mouse(p[0], p[1], b, 1);
                    self.hud_mouse(p[0], p[1], b, 2);
                }
                ("drag", Some(p)) => {
                    let dx = rest.get(1).and_then(|v| v.parse().ok()).unwrap_or(0.0);
                    let dy = rest.get(2).and_then(|v| v.parse().ok()).unwrap_or(0.0);
                    self.hud.dev_mouse = Some(p);
                    self.hud_mouse(p[0], p[1], 0, 0);
                    self.hud_mouse(p[0], p[1], 0, 1);
                    s.drag = Some((p, [dx, dy], since));
                }
                ("key", _) => {
                    let n = rest.join(" ").to_lowercase();
                    self.hud_key(n == "escape", n == "enter", n == "backspace");
                }
                ("type", _) => {
                    for c in rest.join(" ").chars() {
                        self.hud_char(c);
                    }
                }
                ("scroll", _) => {
                    self.hud_scroll(rest.first().and_then(|v| v.parse().ok()).unwrap_or(0.0));
                }
                ("bind", _) => {
                    self.hud_bind(&rest.join(" "));
                }
                ("totem", _) => self.hud_event(1, &rest.join(" ")),
                _ => eprintln!("[hud dev] no target for: {step}"),
            }
        }
        self.hud_dev = Some(s);
    }
}
