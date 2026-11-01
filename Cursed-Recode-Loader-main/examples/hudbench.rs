//! The HUD's frame on the CPU, without the game: the stand's scene (effects, cooldowns, a target, the combat timer)
//! laid out again and again.
//!     cargo run --release --example hudbench

use cursed_recode::gui;
use std::time::Instant;

fn main() {
    // the numbers HudLayer sends (hud.rs Facts::parse): a head of 20, 3 effects, 2 cooldowns, a target
    let mut n = vec![0f64; 20];
    n[0] = 600.0;
    n[1] = 12.0;
    n[2] = 420.5;
    n[3] = 152.0;
    n[4] = 402.3;
    n[6] = 315.0;
    n[10] = 3.0;
    n[11] = 2.0;
    n[12] = 1.0;
    n[13] = 25.0;
    n[14] = 0.6;
    n[16] = 25_000.0;
    n.extend([1.0, 94.0, 0.0, 0.0, 0.0, 41.0, 0.0, 0.0, 0.0, 6.0, 1.0, 0.0]);
    n.extend([0.6, 12.0, 0.3, 1.5]);
    n.extend([13.0, 20.0, 4.0, 15.0, 3.2, 0.0, 7.0, 1.0, 1.0]);
    let text = "Singleplayer\nСкорость\nСила\nСлабость\nEnder Pearl\nShield\ndominoscka\n";
    for _ in 0..200 {
        let id = gui::hud_frame(1904.0, 1041.0, &n, text);
        gui::take(id);
    }
    let runs = 3000;
    let t0 = Instant::now();
    for _ in 0..runs {
        let id = gui::hud_frame(1904.0, 1041.0, &n, text);
        gui::take(id);
    }
    println!("{:.1} us a frame", t0.elapsed().as_secs_f64() / runs as f64 * 1e6);
    // the binning alone: 280 small shapes over the screen
    let mut c = gui::draw::Canvas::new(1904.0, 1041.0);
    for i in 0..280 {
        let x = (i % 20) as f32 * 90.0 + 10.0;
        let y = (i / 20) as f32 * 70.0 + 10.0;
        c.rect([x, y, x + 60.0, y + 20.0], 4.0, [1.0, 1.0, 1.0, 1.0]);
    }
    let t0 = Instant::now();
    for _ in 0..runs {
        std::hint::black_box(c.bins());
    }
    println!("bins: {:.1} us", t0.elapsed().as_secs_f64() / runs as f64 * 1e6);
}

