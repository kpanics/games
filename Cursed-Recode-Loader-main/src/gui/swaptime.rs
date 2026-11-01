//! Auto Swap's timing (swap.rs; Java's cursed.recode.swap.AutoSwap clicks at the ticks said here): when a swap's first
//! click goes — after its key (or its condition), after the swap before, after FunTime's pause; how the player stands
//! for it (before it, only while it clicks, not at all) and which keys are let go; how it clicks in the inventory (one
//! press of a swap key over the slot, or three left clicks carrying the thing, as autoswap 3.0 does) and the gaps
//! between the clicks; a random spread over each delay. The module's settings, set by a preset (one of them changed
//! after, the preset is Custom), or a swap's own. Her word, 23.10: «лучше больше кастомизации» — whoever wants their own
//! delays sets them. A tick is 50 ms: every delay is a whole number of ticks.

use super::data::{Kind, Setting};
use super::swap::{FUNTIME, MODULE};
use serde_json::{json, Value};

/// The presets, and Custom (one of their settings changed after).
pub const PRESETS: [&str; 5] = ["Standard", "Like autoswap 3.0", "Fast", "Careful", "Custom"];
/// How a swap from the inventory clicks.
pub const WAYS: [&str; 2] = ["One press", "Three clicks"];
/// How the player stands for a swap from the inventory.
pub const STOPS: [&str; 3] = ["Before the swap", "While clicking", "Off"];
/// The keys let go while the player stands (a bit each, in this order).
pub const KEYS: [&str; 4] = ["Walking", "Sprint", "Jump", "Sneak"];

/// The module's settings of it (data.rs makes them).
pub const L_PRESET: &str = "Preset";
/// Never shown: the preset the settings were set from last (their round arrows go back to its values).
pub const L_BASE: &str = "Preset base";
pub const L_WAY: &str = "Clicks";
pub const L_CLOSE: &str = "Close the inventory";
pub const L_BEFORE: &str = "Delay before";
pub const L_CLICKS: &str = "Between clicks";
pub const L_SWAPS: &str = "Between swaps";
pub const L_FT: &str = "FunTime pause";
pub const L_STOP: &str = "Stop";
pub const L_STAND: &str = "Stop time";
pub const L_KEYS: &str = "Let go";
pub const L_SPREAD: &str = "Random spread";

/// A swap's timing (the module's, or a swap's own): how it clicks (WAYS), how the player stands (STOPS), and in ms the
/// delay after its key, between its clicks, after the swap before, the stand before it, the random spread.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Timing {
    pub way: usize,
    pub stop: usize,
    pub before: f32,
    pub clicks: f32,
    pub swaps: f32,
    pub stand: f32,
    pub spread: f32,
}

/// A preset: its timing, FunTime's pause (ms), whether the inventory is told closed after, the keys let go (KEYS' bits).
#[derive(Clone, Copy)]
struct Preset {
    t: Timing,
    ft: f32,
    close: bool,
    keys: u8,
}

const ALL_KEYS: u8 = 0b1111;
/// Standard is the module as it was (build 53): one press, the player standing 3 ticks before it, FunTime's 4 ticks.
/// Like autoswap 3.0: its three left clicks a tick apart, the keys let go only while they go (not the sneak), the
/// inventory not told closed, no pause. Fast: no stop, no delays. Careful: slower than Standard everywhere, spread.
const PRESET_VALUES: [Preset; 4] = [
    Preset { t: Timing { way: 0, stop: 0, before: 0.0, clicks: 0.0, swaps: 0.0, stand: 150.0, spread: 0.0 }, ft: 200.0, close: true, keys: ALL_KEYS },
    Preset { t: Timing { way: 1, stop: 1, before: 0.0, clicks: 50.0, swaps: 0.0, stand: 150.0, spread: 0.0 }, ft: 0.0, close: false, keys: 0b0111 },
    Preset { t: Timing { way: 0, stop: 2, before: 0.0, clicks: 0.0, swaps: 0.0, stand: 150.0, spread: 0.0 }, ft: 200.0, close: true, keys: ALL_KEYS },
    Preset { t: Timing { way: 0, stop: 0, before: 100.0, clicks: 100.0, swaps: 300.0, stand: 250.0, spread: 50.0 }, ft: 300.0, close: true, keys: ALL_KEYS },
];

impl Timing {
    /// The Standard preset's (a swap's own starts from the module's).
    pub fn standard() -> Timing {
        PRESET_VALUES[0].t
    }

    pub fn to_json(&self) -> Value {
        json!({"way": self.way, "stop": self.stop, "before": self.before, "clicks": self.clicks, "swaps": self.swaps, "stand": self.stand, "spread": self.spread})
    }

    /// As kept (what is missing or out of range as Standard has it).
    pub fn from_json(v: &Value) -> Timing {
        let d = Timing::standard();
        let n = |k: &str, max: usize, or: usize| v.get(k).and_then(Value::as_u64).map_or(or, |x| (x as usize).min(max));
        let ms = |k: &str, (min, max): (f32, f32), or: f32| v.get(k).and_then(Value::as_f64).map_or(or, |x| snap(x as f32).clamp(min, max));
        Timing {
            way: n("way", WAYS.len() - 1, d.way),
            stop: n("stop", STOPS.len() - 1, d.stop),
            before: ms("before", RANGES[0], d.before),
            clicks: ms("clicks", RANGES[1], d.clicks),
            swaps: ms("swaps", RANGES[2], d.swaps),
            stand: ms("stand", RANGES[3], d.stand),
            spread: ms("spread", RANGES[4], d.spread),
        }
    }

    /// Its delays in the order of SLIDERS (the editor's sliders for a swap's own).
    pub fn ms(&self, k: usize) -> f32 {
        [self.before, self.clicks, self.swaps, self.stand, self.spread][k]
    }

    pub fn set_ms(&mut self, k: usize, v: f32) {
        let v = snap(v).clamp(RANGES[k].0, RANGES[k].1);
        match k {
            0 => self.before = v,
            1 => self.clicks = v,
            2 => self.swaps = v,
            3 => self.stand = v,
            _ => self.spread = v,
        }
    }
}

/// The delays' sliders (a swap's own, and the module's): their labels, and their least and most (ms, a tick a step).
pub const SLIDERS: [&str; 5] = [L_BEFORE, L_CLICKS, L_SWAPS, L_STAND, L_SPREAD];
pub const RANGES: [(f32, f32); 5] = [(0.0, 1000.0), (0.0, 500.0), (0.0, 2000.0), (50.0, 1000.0), (0.0, 250.0)];
/// FunTime's pause: its least and most.
pub const FT_RANGE: (f32, f32) = (0.0, 1000.0);

/// To whole ticks (50 ms).
fn snap(ms: f32) -> f32 {
    (ms / 50.0).round() * 50.0
}

/// A delay in ticks, spread by up to `spread` ms either way (`rnd` 0..1).
fn ticks(ms: f32, spread: f32, rnd: &mut impl FnMut() -> f32) -> i64 {
    let ms = if spread > 0.0 { ms + spread * (2.0 * rnd() - 1.0) } else { ms };
    (ms.max(0.0) / 50.0).round() as i64
}

/// When a swap's clicks go (ticks): the first's, the tick at whose end the keys are let go (-1: they are not), the
/// gaps after the first and the second click, how it clicks, whether the inventory is told closed after, the keys.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Plan {
    pub due: i64,
    pub stop_from: i64,
    pub gaps: [i64; 2],
    pub way: usize,
    pub close: bool,
    pub keys: u8,
}

/// The plan of a swap with timing `t`: by the inventory's clicks or by hand (`clicks`), waiting FunTime's pause `ft`
/// (ms; None: not on FunTime, or not from above the hotbar), asked at tick `at`, the swap before done at `last`, now
/// `now`. Its first click goes after the delay after its key, after the delay after the swap before, after FunTime's
/// pause after both (their rule: after the key and after the swap before), never before now. The stand: before the
/// swap, the keys let go that long before its first click (the click waits for the stand when it would be sooner);
/// while clicking, from the tick of its first click (the game reads the keys a tick on: they are let go at the end of
/// the tick before). Only a swap by the inventory's clicks stands: one by hand goes on the run, as a player's does.
#[allow(clippy::too_many_arguments)]
pub fn plan(t: &Timing, ft: Option<f32>, close: bool, keys: u8, clicks: bool, at: i64, last: i64, now: i64, rnd: &mut impl FnMut() -> f32) -> Plan {
    let mut due = (at + ticks(t.before, t.spread, rnd)).max(last + ticks(t.swaps, t.spread, rnd)).max(now);
    if let Some(ft) = ft.filter(|_| clicks) {
        due = due.max(at.max(last) + ticks(ft, 0.0, rnd));
    }
    let mut stop_from = -1;
    if clicks && keys != 0 {
        let stand = match t.stop {
            0 => ticks(t.stand, t.spread, rnd).max(1),
            1 => 1,
            _ => 0,
        };
        if stand > 0 {
            stop_from = (due - stand).max(now);
            due = due.max(stop_from + stand);
        }
    }
    let gaps = [ticks(t.clicks, t.spread, rnd), ticks(t.clicks, t.spread, rnd)];
    Plan { due, stop_from, gaps, way: t.way, close, keys: if stop_from < 0 { 0 } else { keys } }
}

/// What a preset sets a setting of the module to (shaped as its default: a slider's range, a choice's options).
fn preset_kind(p: &Preset, s: &Setting) -> Option<Kind> {
    let num = |v: f32| match s.def {
        Kind::Slider { min, max, step, unit, .. } => Some(Kind::Slider { min, max, step, v, unit }),
        _ => None,
    };
    let choice = |v: usize| match &s.def {
        Kind::Choice { opts, .. } => Some(Kind::Choice { v, opts: opts.clone() }),
        _ => None,
    };
    match s.label {
        L_WAY => choice(p.t.way),
        L_STOP => choice(p.t.stop),
        L_BEFORE => num(p.t.before),
        L_CLICKS => num(p.t.clicks),
        L_SWAPS => num(p.t.swaps),
        L_STAND => num(p.t.stand),
        L_SPREAD => num(p.t.spread),
        L_FT => num(p.ft),
        L_CLOSE => Some(Kind::Toggle(p.close)),
        L_KEYS => match &s.def {
            Kind::Multi { opts, .. } => Some(Kind::Multi { on: (0..opts.len()).map(|i| p.keys & (1 << i) != 0).collect(), opts: opts.clone() }),
            _ => None,
        },
        _ => None,
    }
}

/// The module's settings of its timing, as they are now.
pub fn module_timing(set: &[Setting]) -> (Timing, f32, bool, u8) {
    let get = |l: &str| set.iter().find(|s| s.label == l).map(|s| &s.kind);
    let num = |l: &str, or: f32| match get(l) {
        Some(Kind::Slider { v, .. }) => *v,
        _ => or,
    };
    let pick = |l: &str| match get(l) {
        Some(Kind::Choice { v, .. }) => *v,
        _ => 0,
    };
    let d = PRESET_VALUES[0];
    let t = Timing {
        way: pick(L_WAY),
        stop: pick(L_STOP),
        before: num(L_BEFORE, d.t.before),
        clicks: num(L_CLICKS, d.t.clicks),
        swaps: num(L_SWAPS, d.t.swaps),
        stand: num(L_STAND, d.t.stand),
        spread: num(L_SPREAD, d.t.spread),
    };
    let close = !matches!(get(L_CLOSE), Some(Kind::Toggle(false)));
    let keys = match get(L_KEYS) {
        Some(Kind::Multi { on, .. }) => on.iter().enumerate().fold(0u8, |m, (i, o)| if *o { m | 1 << i } else { m }),
        _ => d.keys,
    };
    (t, num(L_FT, d.ft), close, keys)
}

/// A random number 0..1 (xorshift*, from the clock first).
fn rnd01() -> f32 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static STATE: AtomicU64 = AtomicU64::new(0);
    let mut x = STATE.load(Ordering::Relaxed);
    if x == 0 {
        x = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(7, |d| d.as_nanos() as u64) | 1;
    }
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    STATE.store(x, Ordering::Relaxed);
    ((x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 40) as f32) / (1u64 << 24) as f32
}

impl super::Gui {
    /// The timing's preset: one chosen sets its settings (and is where their round arrows go back to); one of them
    /// changed after, the preset is Custom; all as a preset has them again, that preset.
    pub(super) fn timing_follow(&mut self) {
        let seen = self.timing_seen;
        let Some(m) = self.mods.iter_mut().find(|m| m.id == MODULE) else { return };
        let set = &mut m.set;
        let at = |set: &[Setting], l: &str| set.iter().position(|s| s.label == l);
        let (Some(pi), Some(bi)) = (at(set, L_PRESET), at(set, L_BASE)) else { return };
        let pick = |s: &Setting| if let Kind::Choice { v, .. } = s.kind { v } else { 0 };
        let n = PRESET_VALUES.len();
        let preset = pick(&set[pi]);
        let mut base = pick(&set[bi]).min(n - 1);
        if preset < n {
            base = preset;
        }
        let chosen = preset != seen && preset < n;
        let mut same = true;
        for s in set.iter_mut() {
            let Some(want) = preset_kind(&PRESET_VALUES[base], s) else { continue };
            if chosen {
                s.kind = want.clone();
            }
            let close = match (&s.kind, &want) {
                (Kind::Slider { v, step, .. }, Kind::Slider { v: w, .. }) => (*v - *w).abs() <= *step * 0.5,
                (k, w) => k == w,
            };
            if close {
                s.kind = want.clone();
            }
            same &= close;
            s.def = want;
        }
        let now = if same { base } else { n };
        if let Kind::Choice { v, .. } = &mut set[pi].kind {
            *v = now;
        }
        if let Kind::Choice { v, .. } = &mut set[bi].kind {
            *v = base;
        }
        self.timing_seen = now;
    }

    /// Read back from a save: its preset as it was (what was read stays); one saved before there were presets (build
    /// 53 and before: FunTime's pause and the stop in ticks, the stop a switch) has them in ms, and its preset is the
    /// one they make.
    pub(super) fn timing_read(&mut self, vals: &std::collections::HashMap<String, String>) {
        let old = |k: &str| vals.get(&format!("{MODULE}.{k}"));
        let has = |k: &str| vals.contains_key(&format!("{MODULE}.{k}"));
        // saved before the presets: its timing is Standard's but for what it had (not what the menu had before it)
        let before_presets = !has(L_PRESET) && ["Pause on FunTime", "Stop before a swap", "Stop length"].iter().any(|k| has(k));
        if let Some(m) = self.mods.iter_mut().find(|m| m.id == MODULE) {
            for s in m.set.iter_mut() {
                if before_presets {
                    if let Some(k) = preset_kind(&PRESET_VALUES[0], s) {
                        s.kind = k;
                    }
                    if s.label == L_PRESET || s.label == L_BASE {
                        if let Kind::Choice { v, .. } = &mut s.kind {
                            *v = 0;
                        }
                    }
                }
                let was = match s.label {
                    L_FT if !has(L_FT) => old("Pause on FunTime").and_then(|v| v.parse::<f32>().ok()).map(|t| t * 50.0),
                    L_STAND if !has(L_STAND) => old("Stop length").and_then(|v| v.parse::<f32>().ok()).map(|t| t * 50.0),
                    _ => None,
                };
                if let (Some(ms), Kind::Slider { v, min, max, .. }) = (was, &mut s.kind) {
                    *v = snap(ms).clamp(*min, *max);
                }
                if s.label == L_STOP && !has(L_STOP) && old("Stop before a swap").is_some_and(|v| v == "0") {
                    if let Kind::Choice { v, .. } = &mut s.kind {
                        *v = 2;
                    }
                }
            }
        }
        // the preset read is the one seen: it does not set its settings over what was read
        self.timing_seen = self.mods.iter().find(|m| m.id == MODULE).and_then(|m| m.set.iter().find(|s| s.label == L_PRESET)).map_or(0, |s| match s.kind {
            Kind::Choice { v, .. } => v,
            _ => 0,
        });
        self.timing_follow();
    }

    /// Java's "swaptime": a swap coming up (`a` its number, -1 none), "clicks above at last now" — whether it goes by
    /// the inventory's clicks, whether its thing is from above the hotbar, the tick it was asked at, the tick the swap
    /// before was done at, now. Its plan: "due stop_from gap1 gap2 way close keys".
    pub(super) fn swap_time(&mut self, a: &str, b: &str) -> String {
        self.timing_follow();
        let mut f = b.split('\t');
        let mut flag = || f.next() == Some("1");
        let (clicks, above) = (flag(), flag());
        let nums: Vec<i64> = b.split('\t').skip(2).map(|x| x.parse::<i64>().unwrap_or(0)).collect();
        let (at, last, now) = (nums.first().copied().unwrap_or(0), nums.get(1).copied().unwrap_or(-1000), nums.get(2).copied().unwrap_or(0));
        let Some(m) = self.mods.iter().find(|m| m.id == MODULE) else { return String::new() };
        let (mut t, ft, close, keys) = module_timing(&m.set);
        if let Some(own) = a.parse::<usize>().ok().and_then(|i| self.swaps.get(i)).and_then(|s| s.timing) {
            t = own;
        }
        let ft = (self.server == FUNTIME && above).then_some(ft);
        let p = plan(&t, ft, close, keys, clicks, at, last, now, &mut rnd01);
        format!("{}\t{}\t{}\t{}\t{}\t{}\t{}", p.due, p.stop_from, p.gaps[0], p.gaps[1], p.way, p.close as i32, p.keys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none() -> impl FnMut() -> f32 {
        || 0.5
    }

    #[test]
    fn standard_is_the_module_as_it_was() {
        let t = Timing::standard();
        // anywhere: the player stands 3 ticks, then the click
        let p = plan(&t, None, true, ALL_KEYS, true, 100, -1000, 100, &mut none());
        assert_eq!((p.stop_from, p.due, p.way, p.keys), (100, 103, 0, ALL_KEYS));
        // FunTime from the inventory: 4 ticks after the key, the stand inside them
        let p = plan(&t, Some(200.0), true, ALL_KEYS, true, 100, -1000, 100, &mut none());
        assert_eq!((p.stop_from, p.due), (101, 104));
        // and 4 after the swap before
        let p = plan(&t, Some(200.0), true, ALL_KEYS, true, 100, 103, 103, &mut none());
        assert_eq!(p.due, 107);
        // by hand: at once, on the run
        let p = plan(&t, Some(200.0), true, ALL_KEYS, false, 100, -1000, 100, &mut none());
        assert_eq!((p.stop_from, p.due, p.keys), (-1, 100, 0));
    }

    #[test]
    fn like_autoswap_its_clicks_a_tick_apart_the_keys_let_go_only_then() {
        let t = PRESET_VALUES[1].t;
        let p = plan(&t, Some(PRESET_VALUES[1].ft), false, 0b0111, true, 100, -1000, 100, &mut none());
        // the keys let go at the end of the tick asked: the click's tick is read without them
        assert_eq!((p.stop_from, p.due, p.gaps, p.way, p.close, p.keys), (100, 101, [1, 1], 1, false, 0b0111));
    }

    #[test]
    fn delays_add_up_and_never_go_before_now() {
        let t = Timing { way: 0, stop: 2, before: 300.0, clicks: 100.0, swaps: 500.0, stand: 150.0, spread: 0.0 };
        let p = plan(&t, None, true, ALL_KEYS, true, 100, 98, 100, &mut none());
        // after the key 6, after the swap before 10: the later
        assert_eq!((p.due, p.stop_from, p.gaps), (108, -1, [2, 2]));
        // asked long ago, waiting its turn: now
        let p = plan(&Timing { swaps: 0.0, before: 0.0, ..t }, None, true, ALL_KEYS, true, 10, 50, 120, &mut none());
        assert_eq!(p.due, 120);
        // the stand longer than the delay: the click waits for it
        let s = Timing { stop: 0, stand: 400.0, ..t };
        let p = plan(&s, None, true, ALL_KEYS, true, 100, -1000, 100, &mut none());
        assert_eq!((p.stop_from, p.due), (100, 108));
        // no key to let go: no stand
        let p = plan(&s, None, true, 0, true, 100, -1000, 100, &mut none());
        assert_eq!((p.stop_from, p.due, p.keys), (-1, 106, 0));
    }

    #[test]
    fn the_spread_stays_within_its_ms() {
        let t = Timing { way: 1, stop: 2, before: 200.0, clicks: 100.0, swaps: 0.0, stand: 150.0, spread: 100.0 };
        let (mut lo, mut hi) = (i64::MAX, i64::MIN);
        let mut x = 0.0f32;
        let mut r = || {
            x = (x + 0.137) % 1.0;
            x
        };
        for _ in 0..200 {
            let p = plan(&t, None, true, ALL_KEYS, true, 0, -1000, 0, &mut r);
            lo = lo.min(p.due);
            hi = hi.max(p.due);
            assert!((0..=4).contains(&p.gaps[0]) && (0..=4).contains(&p.gaps[1]));
        }
        // 200 ± 100 ms: 2..6 ticks, and not always the same
        assert!(lo >= 2 && hi <= 6 && lo < hi, "{lo} {hi}");
        assert!((0..10_000).map(|_| rnd01()).all(|v| (0.0..1.0).contains(&v)));
    }

    #[test]
    fn a_swaps_own_timing_is_kept() {
        let t = Timing { way: 1, stop: 1, before: 250.0, clicks: 50.0, swaps: 1000.0, stand: 300.0, spread: 50.0 };
        assert_eq!(Timing::from_json(&t.to_json()), t);
        // out of range or not ticks: to the nearest tick within
        let v: Value = serde_json::from_str(r#"{"way":7,"before":5000,"clicks":72}"#).unwrap();
        let r = Timing::from_json(&v);
        assert_eq!((r.way, r.before, r.clicks, r.stand), (1, 1000.0, 50.0, 150.0));
    }

    #[test]
    fn a_preset_sets_its_settings_and_a_change_makes_it_custom() {
        let mut g = super::super::Gui::new();
        let mi = g.mod_index(MODULE);
        let at = |g: &super::super::Gui, l: &str| g.mods[mi].set.iter().position(|s| s.label == l).unwrap();
        let choice = |g: &super::super::Gui, l: &str| match g.mods[mi].set[at(g, l)].kind {
            Kind::Choice { v, .. } => v,
            _ => 99,
        };
        g.timing_follow();
        assert_eq!(choice(&g, L_PRESET), 0);
        assert_eq!(module_timing(&g.mods[mi].set), (PRESET_VALUES[0].t, 200.0, true, ALL_KEYS));
        // Like autoswap 3.0 chosen: all of it set, none shown as changed
        let pi = at(&g, L_PRESET);
        if let Kind::Choice { v, .. } = &mut g.mods[mi].set[pi].kind {
            *v = 1;
        }
        g.timing_follow();
        assert_eq!(module_timing(&g.mods[mi].set), (PRESET_VALUES[1].t, 0.0, false, 0b0111));
        assert!(!g.mods[mi].set[at(&g, L_CLICKS)].changed());
        // one changed: Custom, from it; put back: it again
        let ci = at(&g, L_CLICKS);
        if let Kind::Slider { v, .. } = &mut g.mods[mi].set[ci].kind {
            *v = 150.0;
        }
        g.timing_follow();
        assert_eq!((choice(&g, L_PRESET), choice(&g, L_BASE)), (4, 1));
        g.mods[mi].set[ci].reset();
        g.timing_follow();
        assert_eq!(choice(&g, L_PRESET), 1);
        // the server asks: the plan of the module's (Like autoswap 3.0 here)
        assert_eq!(g.swap_time("-1", "1\t1\t100\t-1000\t100"), "101\t100\t1\t1\t1\t0\t7");
    }

    #[test]
    fn a_save_from_before_the_presets_is_read_in_ms() {
        let mut g = super::super::Gui::new();
        let mi = g.mod_index(MODULE);
        let vals = |kv: &[(&str, &str)]| kv.iter().map(|(k, v)| (format!("{MODULE}.{k}"), v.to_string())).collect::<std::collections::HashMap<_, _>>();
        // build 53's: as Standard has it (4 ticks, a stop of 3): Standard
        g.apply_vals(&vals(&[("Pause on FunTime", "4"), ("Stop before a swap", "1"), ("Stop length", "3")]), super::super::configs::P_MODULES);
        let (t, ft, _, _) = module_timing(&g.mods[mi].set);
        assert_eq!((t.stop, t.stand, ft), (0, 150.0, 200.0));
        assert!(matches!(g.mods[mi].set.iter().find(|s| s.label == L_PRESET).map(|s| &s.kind), Some(Kind::Choice { v: 0, .. })));
        // its own: 8 ticks on FunTime, no stop: Custom, and they stay
        g.apply_vals(&vals(&[("Pause on FunTime", "8"), ("Stop before a swap", "0"), ("Stop length", "5")]), super::super::configs::P_MODULES);
        let (t, ft, _, _) = module_timing(&g.mods[mi].set);
        assert_eq!((t.stop, t.stand, ft), (2, 250.0, 400.0));
        assert!(matches!(g.mods[mi].set.iter().find(|s| s.label == L_PRESET).map(|s| &s.kind), Some(Kind::Choice { v: 4, .. })));
        // one of build 53 read over another preset: Standard's but for what it had
        let mut g = super::super::Gui::new();
        g.apply_vals(&vals(&[(L_PRESET, "Like autoswap 3.0"), (L_BASE, "Like autoswap 3.0"), (L_WAY, "Three clicks"), (L_CLOSE, "0"), (L_FT, "0"), (L_STOP, "While clicking")]), super::super::configs::P_MODULES);
        g.apply_vals(&vals(&[("Pause on FunTime", "6"), ("Stop before a swap", "1"), ("Stop length", "3")]), super::super::configs::P_MODULES);
        let (t, ft, close, _) = module_timing(&g.mods[mi].set);
        assert_eq!((t.way, t.stop, t.stand, ft, close), (0, 0, 150.0, 300.0, true));
        // a new one: as it was saved
        let mut g = super::super::Gui::new();
        g.apply_vals(&vals(&[(L_PRESET, "Fast"), (L_BASE, "Fast"), (L_STOP, "Off"), (L_FT, "200")]), super::super::configs::P_MODULES);
        let (t, ft, _, _) = module_timing(&g.mods[mi].set);
        assert_eq!((t.stop, ft), (2, 200.0));
        assert!(matches!(g.mods[mi].set.iter().find(|s| s.label == L_PRESET).map(|s| &s.kind), Some(Kind::Choice { v: 2, .. })));
    }
}
