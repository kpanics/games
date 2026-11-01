//! A CS rifle's tracer (its weapon_tracers_rifle, in our light): a short bright streak flying from the muzzle along
//! the shot to where it struck, in the time a bullet's tracer takes; the arrow itself is not drawn (her word, 24.10).
//! Java gives its ends in the world (the muzzle, the first block on the way); the world's effects draw it
//! (fx/Tracers.java asks for this frame's streaks).

use std::sync::Mutex;
use std::time::Instant;

/// How fast it flies (blocks a second), how long it is (blocks).
const SPEED: f64 = 260.0;
const LENGTH: f64 = 4.0;

struct Tracer {
    from: [f64; 3],
    to: [f64; 3],
    at: Instant,
}

static TRACERS: Mutex<Vec<Tracer>> = Mutex::new(Vec::new());

/// A shot's tracer, from `from` to `to` (world).
pub fn shot(from: [f64; 3], to: [f64; 3]) {
    let mut t = TRACERS.lock().unwrap();
    if t.len() >= 8 {
        t.remove(0);
    }
    t.push(Tracer { from, to, at: Instant::now() });
}

/// The streaks now: each its tail and its head (world) and how strong (0..1, fading as it arrives).
pub fn streaks() -> Vec<([f64; 3], [f64; 3], f32)> {
    let mut t = TRACERS.lock().unwrap();
    let now = Instant::now();
    let mut out = Vec::new();
    t.retain(|tr| {
        let d: [f64; 3] = std::array::from_fn(|k| tr.to[k] - tr.from[k]);
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-6);
        let flown = now.duration_since(tr.at).as_secs_f64() * SPEED;
        if flown - LENGTH > len {
            return false;
        }
        let head = flown.min(len);
        let tail = (flown - LENGTH).max(0.0).min(len);
        let at = |x: f64| std::array::from_fn(|k| tr.from[k] + d[k] * (x / len));
        let k = if flown > len { (1.0 - (flown - len) / LENGTH) as f32 } else { 1.0 };
        out.push((at(tail), at(head), k.clamp(0.0, 1.0)));
        true
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tracer_flies_and_is_gone() {
        shot([0.0, 0.0, 0.0], [0.0, 0.0, 10.0]);
        let s = streaks();
        assert_eq!(s.len(), 1);
        assert!(s[0].1[2] <= 10.0 && s[0].0[2] <= s[0].1[2]);
        std::thread::sleep(std::time::Duration::from_millis(80));
        // 10 blocks and its length behind: gone by now (260 a second)
        assert!(streaks().is_empty());
    }
}
