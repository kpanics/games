//! What the window before the game and the game say to each other: a named shared memory when it is a program of its
//! own (src/bin/cursed_twin.rs — started before Java, the game finds it by the name in its CURSED_TWIN environment
//! variable), a block in this process when the game starts it itself. Numbers and flags both sides poll; nothing waits
//! on the other.
//!
//! Both draw the card from one clock: `epoch`, the program's start (QPC, the same counter in every process), and how
//! long the card waited for the game (`held`). So at the hand-over the game's picture is the window's own, frame for
//! frame — the fire, the clouds, the title — and the window can simply go.
//!
//! It keeps the menu's music for the whole session (the music runs on from the flash into the menu unbroken), so the
//! game's music buttons and volumes go through here too; on the way out the game asks it for the farewell and ends at
//! once, the spark goes out in that program.

use std::sync::atomic::{AtomicI32, AtomicI64, AtomicU32, Ordering};
use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Memory::{CreateFileMappingW, MapViewOfFile, OpenFileMappingW, FILE_MAP_ALL_ACCESS, PAGE_READWRITE};

pub const MAGIC: u32 = 0x4C54_5245; // "ERTL" (the layout of 16.10: the loader's valley clock and crows go on in the game)

/// Link.flags: from the game.
pub const READY: u32 = 1;
pub const RELEASE: u32 = 2;
pub const FAREWELL: u32 = 4;
/// The game's window was maximised when its way out began (the bar's maximise button is a restore one then).
pub const MAXIMIZED: u32 = 8;
/// …and not the window in use (its bar dimmed).
pub const INACTIVE: u32 = 16;
/// The game has joined the link (twin::attach): a game the loader started that never does is not waited for.
pub const JOINED: u32 = 32;
/// From the window before the game: it is minimised now (the game's window comes up minimised then, its taskbar button
/// blinking).
pub const HIDDEN: u32 = 64;
/// Link.state: of the window before the game.
pub const PLAYING: u32 = 1;
pub const IDLE: u32 = 2;
pub const LEAVING: u32 = 3;
pub const DONE: u32 = 4;

#[repr(C)]
pub struct Link {
    pub magic: AtomicU32,
    /// 1 once `place` is written.
    pub placed: AtomicU32,
    /// The game's window: picture x y w h, visible frame x y w h (physical px).
    pub place: [AtomicI32; 8],
    /// The program's start (QPC): `life`, the clock that never waits, is the seconds since.
    pub epoch: AtomicI64,
    /// The card's second on screen (f32 bits), negative before the flash.
    pub card: AtomicU32,
    /// How long the card waited on its title for the game (f32 seconds); final once `hold_done` is 1.
    pub held: AtomicU32,
    pub hold_done: AtomicU32,
    pub flags: AtomicU32,
    pub state: AtomicU32,
    pub game_pid: AtomicU32,
    /// The farewell: where the game's window is; the card's second and the forge's, the title's place (f32 bits).
    pub farewell_place: [AtomicI32; 8],
    pub farewell: [AtomicU32; 4],
    /// When the game's way out began, and when it asked for the farewell (QPC).
    pub exit_qpc: AtomicI64,
    pub ask_qpc: AtomicI64,
    /// The game's view as it asked: look x y, the cursor's light x y on, the hover's glow, sheet lightning (f32).
    pub view: [AtomicU32; 7],
    /// The game's volumes (f32 bits; master, music).
    pub volume: [AtomicU32; 2],
    /// The music: a command (0 pause/on, 1 from the start, 2 out, 3 in) numbered by `music_seq`; its state back
    /// (seconds played, length, heard — f32 bits).
    pub music_seq: AtomicU32,
    pub music_cmd: AtomicU32,
    pub music: [AtomicU32; 3],
    /// 1 once the farewell's first picture is up over the game's window (the game hides its own then).
    pub shown: AtomicU32,
    /// The game's resources loading (f32 bits, 0..1; negative: it has not said yet): the card's point runs along its
    /// line no further than it.
    pub load: AtomicU32,
    /// The way out: 0 from the main menu, 1 from a world.
    pub exit_kind: AtomicU32,
    /// The valley the game drew last as it asked for the farewell (f32 bits: the camera's place, then the knobs;
    /// launch::valley), once `scene_set` is 1: the way out goes on from it, not from the defaults.
    pub scene: [AtomicU32; 29],
    pub scene_set: AtomicU32,
    /// The valley's second the crows were startled off the tree (f32 bits, -1 not; launch/ink.rs startled): the way
    /// out's crows fly on from where the game's were.
    pub startled: AtomicU32,
    /// How far the valley's clock is ahead of the desk's (f32 bits, launch/ink.rs life_shift): the loader's valley,
    /// alive before the game was started, goes on in the window before the game and in the game without a leap.
    pub life_shift: AtomicU32,
}

pub fn f32_of(a: &AtomicU32) -> f32 {
    f32::from_bits(a.load(Ordering::Acquire))
}

pub fn set_f32(a: &AtomicU32, v: f32) {
    a.store(v.to_bits(), Ordering::Release);
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn init(link: &Link) {
    set_f32(&link.card, -1.0);
    set_f32(&link.load, -1.0);
    set_f32(&link.startled, -1.0);
    set_f32(&link.volume[0], 1.0);
    set_f32(&link.volume[1], 1.0);
    link.magic.store(MAGIC, Ordering::Release);
}

/// Makes the link named `name` (the window before the game), clean. Lives as long as the process: a second game the
/// same loader starts gets the same name, and Windows hands back the mapping the first one left — its flags (joined,
/// released, the farewell asked) would have the new window go at once and leave the new game unshown (19.10.2026).
pub fn create(name: &str) -> Option<&'static Link> {
    unsafe {
        let n = wide(name);
        let h = CreateFileMappingW(INVALID_HANDLE_VALUE, std::ptr::null(), PAGE_READWRITE, 0, std::mem::size_of::<Link>() as u32, n.as_ptr());
        if h.is_null() {
            return None;
        }
        let view = MapViewOfFile(h, FILE_MAP_ALL_ACCESS, 0, 0, std::mem::size_of::<Link>());
        if view.Value.is_null() {
            CloseHandle(h);
            return None;
        }
        // (all zeroes is a valid Link: atomics)
        std::ptr::write_bytes(view.Value as *mut u8, 0, std::mem::size_of::<Link>());
        let link = &*(view.Value as *const Link);
        init(link);
        Some(link)
    }
}

/// A link inside this process (the game starts the window itself). Lives as long as the process.
pub fn local() -> &'static Link {
    // all zeroes is a valid Link (atomics); the memory is never freed
    let link: &'static Link = unsafe { &*(Box::leak(Box::new(std::mem::MaybeUninit::<Link>::zeroed())).as_ptr()) };
    init(link);
    link
}

/// Finds the link named `name` (the game), waiting up to `wait_ms` for its place.
pub fn open(name: &str, wait_ms: u64) -> Option<&'static Link> {
    unsafe {
        let n = wide(name);
        let h = OpenFileMappingW(FILE_MAP_ALL_ACCESS, 0, n.as_ptr());
        if h.is_null() {
            return None;
        }
        let view = MapViewOfFile(h, FILE_MAP_ALL_ACCESS, 0, 0, std::mem::size_of::<Link>());
        if view.Value.is_null() {
            CloseHandle(h);
            return None;
        }
        let link = &*(view.Value as *const Link);
        let until = std::time::Instant::now() + std::time::Duration::from_millis(wait_ms);
        while link.magic.load(Ordering::Acquire) != MAGIC || link.placed.load(Ordering::Acquire) == 0 {
            if std::time::Instant::now() > until {
                return None;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        Some(link)
    }
}

/// The performance counter, the same in every process.
pub fn qpc() -> i64 {
    let mut v = 0i64;
    unsafe { windows_sys::Win32::System::Performance::QueryPerformanceCounter(&mut v) };
    v
}

fn freq() -> f64 {
    static F: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *F.get_or_init(|| {
        let mut f = 0i64;
        unsafe { windows_sys::Win32::System::Performance::QueryPerformanceFrequency(&mut f) };
        f.max(1) as f64
    })
}

/// Seconds from QPC `a` to QPC `b`.
pub fn secs(a: i64, b: i64) -> f32 {
    ((b - a) as f64 / freq()) as f32
}

/// The QPC `secs` after `epoch`.
pub fn qpc_at(epoch: i64, secs: f32) -> i64 {
    epoch + (secs as f64 * freq()) as i64
}

/// The QPC `ago` back from now.
pub fn qpc_ago(ago: std::time::Duration) -> i64 {
    qpc() - (ago.as_secs_f64() * freq()) as i64
}

impl Link {
    /// Seconds since the program started (the clock that never waits).
    pub fn life(&self) -> f32 {
        secs(self.epoch.load(Ordering::Acquire), qpc())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A link made again under the same name (the loader's second game) starts clean: nothing of the first game's.
    #[test]
    fn a_link_made_again_is_clean() {
        let name = format!("CursedRecodeLinkTest-{}", std::process::id());
        let first = create(&name).expect("made");
        first.flags.store(READY | RELEASE | FAREWELL | JOINED, Ordering::Release);
        first.state.store(DONE, Ordering::Release);
        first.placed.store(1, Ordering::Release);
        first.shown.store(1, Ordering::Release);
        first.hold_done.store(1, Ordering::Release);
        first.game_pid.store(1234, Ordering::Release);
        first.ask_qpc.store(qpc(), Ordering::Release);
        let second = create(&name).expect("made again");
        for (what, v) in [("flags", &second.flags), ("state", &second.state), ("placed", &second.placed), ("shown", &second.shown), ("hold_done", &second.hold_done), ("game_pid", &second.game_pid)] {
            assert_eq!(v.load(Ordering::Acquire), 0, "{what} left from the first game");
        }
        assert_eq!(second.ask_qpc.load(Ordering::Acquire), 0);
        assert_eq!(second.magic.load(Ordering::Acquire), MAGIC);
        assert_eq!(f32_of(&second.card), -1.0);
        assert_eq!(f32_of(&second.volume[1]), 1.0);
    }
}
