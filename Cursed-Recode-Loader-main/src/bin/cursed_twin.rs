//! The window before the game as a program of its own (twin/link.rs): the launcher starts it before Java, so the
//! spark is up at once; the game joins it by the name in its CURSED_TWIN environment variable.
//!
//!   cursed_twin.exe --name <link> [--width W --height H] [--at X,Y] [--game-dir D] [--mute] [--log file]
#![windows_subsystem = "windows"]

fn main() {
    std::hint::black_box(cursed_recode::notice::keep());
    let started = std::time::Instant::now();
    let args: Vec<String> = std::env::args().collect();
    let get = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let name = get("--name").unwrap_or_else(|| "CursedRecodeTwin".into());
    let mut w: i32 = get("--width").and_then(|v| v.parse().ok()).unwrap_or(0);
    let mut h: i32 = get("--height").and_then(|v| v.parse().ok()).unwrap_or(0);
    // the game's default 854 × 480 is a stamp on today's screens: then most of the screen
    if w == 854 && h == 480 {
        w = 0;
        h = 0;
    }
    let at = get("--at").and_then(|v| {
        let mut p = v.split(',');
        Some([p.next()?.trim().parse().ok()?, p.next()?.trim().parse().ok()?])
    });
    let dir = get("--game-dir");
    let mute = args.iter().any(|a| a == "--mute");
    let log = get("--log").map(std::path::PathBuf::from);
    // development: the launch's pictures at given seconds, no window (twin::Shots)
    if let Some(dir) = get("--shots") {
        let times: Vec<f32> = get("--times").unwrap_or_default().split(',').filter_map(|v| v.trim().parse().ok()).collect();
        let dt = get("--dt").and_then(|v| v.parse().ok()).unwrap_or(1.0 / 120.0);
        let ready_at = get("--ready").and_then(|v| v.parse().ok()).unwrap_or(9.0);
        // --said S: the game's resources say how far they are from S seconds on (nothing before)
        let said_from = get("--said").and_then(|v| v.parse().ok()).unwrap_or(4.0f32).min(ready_at);
        let (w, h) = if w > 0 { (w, h) } else { (1843, 1036) };
        // --farewell KIND,AFTER: the way out (0 the menu, 1 a world) taken over AFTER seconds into it
        let farewell = get("--farewell").and_then(|v| {
            let mut it = v.split(',');
            Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?))
        });
        // --menu: the main menu's valley (the story over), at the seconds asked of its own clock
        let menu = args.iter().any(|a| a == "--menu");
        cursed_recode::twin::shots(cursed_recode::twin::Shots { dir: dir.into(), times, dt, ready_at, said_from, farewell, menu }, w, h);
        return;
    }
    cursed_recode::twin::serve(&name, w, h, at, dir.as_deref(), mute, started, log);
}
