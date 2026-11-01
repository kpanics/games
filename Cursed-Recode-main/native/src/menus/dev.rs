//! Development: the menus driven by a script on the test stand (its window is never active: no real mouse or keys).
//! CURSED_DEV_MENUS="12:move .1 .62; 12.6:click; 14:key Escape; 15:type hello; 16:wheel -2; 17:hold 1.2; 18:wait pause" — seconds
//! of the menus' clock from the first frame, the pointer in fractions of the window.

use super::Menus;
use std::sync::OnceLock;

struct Step {
    at: f32,
    what: String,
}

fn script() -> &'static Option<Vec<Step>> {
    static S: OnceLock<Option<Vec<Step>>> = OnceLock::new();
    S.get_or_init(|| {
        let s = std::env::var("CURSED_DEV_MENUS").ok()?;
        let mut v = Vec::new();
        for part in s.split(';').map(str::trim).filter(|p| !p.is_empty()) {
            let (at, what) = part.split_once(':')?;
            v.push(Step { at: at.trim().parse().ok()?, what: what.trim().to_string() });
        }
        eprintln!("[menus dev] {} steps", v.len());
        Some(v)
    })
}

pub fn on() -> bool {
    script().is_some()
}

impl Menus {
    /// The script's steps due by now (called each frame before the screens draw).
    pub fn dev_run(&mut self) {
        let Some(steps) = script() else { return };
        if self.dev_t0 < 0.0 {
            self.dev_t0 = self.now;
        }
        let mut t = self.now - self.dev_t0;
        while self.dev_at < steps.len() && steps[self.dev_at].at <= t {
            let what = steps[self.dev_at].what.clone();
            // "wait pause": the script stops here until that page is up; its time then counts from this step's
            if let Some(kind) = what.strip_prefix("wait ") {
                if self.current().map_or(true, |c| c.kind != kind.trim()) {
                    break;
                }
                self.dev_t0 = self.now - steps[self.dev_at].at;
                t = steps[self.dev_at].at;
                self.dev_at += 1;
                eprintln!("[menus dev] {t:.2}: {what} (up)");
                continue;
            }
            self.dev_at += 1;
            eprintln!("[menus dev] {t:.2}: {what}");
            let w: Vec<&str> = what.split_whitespace().collect();
            match w.first().copied() {
                Some("move") if w.len() >= 3 => {
                    self.mouse = [w[1].parse::<f32>().unwrap_or(0.5) * self.w, w[2].parse::<f32>().unwrap_or(0.5) * self.h];
                }
                Some("click") => {
                    let [x, y] = self.mouse;
                    self.press(x, y, 0);
                    self.release(x, y, 0);
                }
                Some("dbl") => {
                    let [x, y] = self.mouse;
                    self.press(x, y, 16);
                    self.release(x, y, 16);
                }
                Some("down") => {
                    let [x, y] = self.mouse;
                    self.press(x, y, 0);
                }
                Some("up") => {
                    let [x, y] = self.mouse;
                    self.release(x, y, 0);
                }
                Some("key") if w.len() >= 2 => self.page_event(super::Event::Key(w[1..].join(" "), 0)),
                Some("type") => {
                    for ch in what[5..].chars() {
                        self.page_event(super::Event::Char(ch));
                    }
                }
                Some("wheel") if w.len() >= 2 => self.page_event(super::Event::Scroll(w[1].parse().unwrap_or(0.0))),
                _ => eprintln!("[menus dev] what is «{what}»?"),
            }
        }
    }
}
