//! Where the native renderer's time goes, for measuring (CURSED_DEV_BENCH in the environment): per frame, the time of
//! the queue submit, of waiting for the GPU, of presenting, of recording passes and of building the terrain. Every
//! 10 seconds the averages go to the game's log (through the debug messages).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::Instant;

pub const SUBMIT: usize = 0;
pub const WAIT: usize = 1;
pub const PRESENT: usize = 2;
pub const PASS: usize = 3;
pub const TERRAIN: usize = 4;
/// The GPU's own time for a submit (timestamps at its first and last command).
pub const GPU: usize = 5;
/// Full memory barriers recorded (a count, shown per frame).
pub const BARRIERS: usize = 6;
/// Terrain occlusion: opaque draws tested, and of them hidden (counts, read back from the GPU).
pub const OCCLUSION_TESTED: usize = 7;
pub const OCCLUSION_HIDDEN: usize = 8;
/// Acquiring the window's image (on the present thread, ahead of the frame).
pub const ACQUIRE: usize = 9;
/// The terrain's uploads applied (inside TERRAIN).
pub const TERRAIN_UPLOADS: usize = 10;
/// The GPU idle from one submit's last command to the next one's first, when under a millisecond (the GPU had work).
pub const GPU_GAP: usize = 11;
const NAMES: [&str; 12] = ["submit", "wait GPU", "present", "record passes", "terrain", "GPU busy", "barriers x1000", "opaque draws tested x1000", "hidden x1000", "acquire", "terrain uploads", "GPU idle between submits"];

static TOTALS: [AtomicU64; 12] = [const { AtomicU64::new(0) }; 12];

/// Adds `n` to `slot` when measuring (times in nanoseconds; counts × 1000 so they print as a count).
pub fn add(slot: usize, n: u64) {
    if on() {
        TOTALS[slot].fetch_add(n, Ordering::Relaxed);
    }
}
static FRAMES: AtomicU64 = AtomicU64::new(0);
static WINDOW: OnceLock<std::sync::Mutex<Instant>> = OnceLock::new();

pub fn on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("CURSED_DEV_BENCH").is_some() || std::env::var_os("CURSED_DEV_FPS").is_some())
}

/// The GPU's time per pass and per pipeline too (CURSED_DEV_BENCH; CURSED_DEV_FPS leaves them out: each of their
/// timestamps waits for everything before it, and they slow the very frame they measure).
pub fn passes() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("CURSED_DEV_BENCH").is_some())
}

/// Times `f` into `slot` when measuring.
pub fn time<T>(slot: usize, f: impl FnOnce() -> T) -> T {
    if !on() {
        return f();
    }
    let t = Instant::now();
    let r = f();
    TOTALS[slot].fetch_add(t.elapsed().as_nanos() as u64, Ordering::Relaxed);
    r
}

/// Time per command code of the frame stream (pass calls 1-17, frame commands 100-126), and how many.
static OPS: [AtomicU64; 128] = [const { AtomicU64::new(0) }; 128];
static OP_COUNTS: [AtomicU64; 128] = [const { AtomicU64::new(0) }; 128];

/// Adds one command's time.
pub fn op(code: i32, ns: u64) {
    let i = (code as usize).min(127);
    OPS[i].fetch_add(ns, Ordering::Relaxed);
    OP_COUNTS[i].fetch_add(1, Ordering::Relaxed);
}

/// One frame done; every 10 seconds the averages become a message.
pub fn frame() {
    if !on() {
        return;
    }
    FRAMES.fetch_add(1, Ordering::Relaxed);
    let window = WINDOW.get_or_init(|| std::sync::Mutex::new(Instant::now()));
    let mut start = window.lock().unwrap();
    if start.elapsed().as_secs() < 10 {
        return;
    }
    *start = Instant::now();
    let frames = FRAMES.swap(0, Ordering::Relaxed).max(1);
    let mut text = format!("I [native] {frames} frames, per frame:");
    for (i, name) in NAMES.iter().enumerate() {
        let ns = TOTALS[i].swap(0, Ordering::Relaxed);
        text.push_str(&format!(" {name} {:.1} us;", ns as f64 / frames as f64 / 1000.0));
    }
    let mut ops: Vec<(usize, u64, u64)> = (0..128)
        .map(|i| (i, OPS[i].swap(0, Ordering::Relaxed), OP_COUNTS[i].swap(0, Ordering::Relaxed)))
        .filter(|o| o.2 > 0)
        .collect();
    ops.sort_by(|a, b| b.1.cmp(&a.1));
    let mut by_op = format!("I [native] frame thread per command, us per frame:");
    for (code, ns, n) in ops.iter().take(16) {
        by_op.push_str(&format!(" #{code} {:.1} ({:.0}x);", *ns as f64 / frames as f64 / 1000.0, *n as f64 / frames as f64));
    }
    if let Ok(mut m) = super::device::MESSAGES.lock() {
        m.push(text);
        m.push(by_op);
    }
}
