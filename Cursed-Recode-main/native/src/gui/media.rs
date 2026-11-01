//! What plays on the computer (the HUD's music): Windows' own media controls (whatever player: Spotify, a browser's
//! Yandex or SoundCloud tab, the system player), asked on a thread of its own twice a second: title, artist, where in
//! it, how long, playing or not, and its cover (written to a file Java draws from).

use std::sync::Mutex;
use std::time::Instant;

#[derive(Clone, Default)]
pub struct Now {
    pub title: String,
    pub artist: String,
    /// seconds into it when asked, its length, whether it plays
    pub pos: f32,
    pub len: f32,
    pub playing: bool,
    /// the cover's file and its number (it changes with the cover)
    pub cover: Option<(String, u32)>,
}

struct State {
    now: Option<Now>,
    at: Option<Instant>,
    started: bool,
    dir: Option<std::path::PathBuf>,
    cover_key: String,
    cover_seq: u32,
    /// the picture shown (a hash of its bytes), when the song last changed, how many times it was asked
    cover_hash: u64,
    key_at: Option<Instant>,
    asks: u32,
}

static STATE: Mutex<State> = Mutex::new(State { now: None, at: None, started: false, dir: None, cover_key: String::new(), cover_seq: 0, cover_hash: 0, key_at: None, asks: 0 });

/// How long after a song changes its cover is asked again each time (players give the new title first and the new
/// picture a moment later: asked once, the last song's picture stayed); after that, now and then.
const COVER_SETTLES: f32 = 8.0;

/// What plays now (its place in it moving on while it plays); starts asking the first time.
pub fn now(dir: Option<&std::path::Path>) -> Option<Now> {
    let mut s = STATE.lock().unwrap();
    if !s.started {
        s.started = true;
        s.dir = dir.map(|d| d.join("cursed-recode"));
        std::thread::Builder::new().name("Recode media".into()).spawn(run).ok();
    }
    // development: CURSED_DEV_COVER=a picture's file — shown as the cover of what plays (its cut on the HUD looked at
    // without a player)
    if let Some(p) = std::env::var_os("CURSED_DEV_COVER") {
        let mut n = s.now.clone().unwrap_or(Now { title: "Cover".into(), artist: "CURSED_DEV_COVER".into(), pos: 30.0, len: 200.0, playing: true, cover: None });
        n.cover = Some((p.to_string_lossy().into_owned(), 1));
        return Some(n);
    }
    let mut n = s.now.clone()?;
    if n.playing {
        if let Some(at) = s.at {
            n.pos = (n.pos + at.elapsed().as_secs_f32()).min(if n.len > 0.0 { n.len } else { f32::MAX });
        }
    }
    Some(n)
}

fn run() {
    use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};
    unsafe {
        let _ = RoInitialize(RO_INIT_MULTITHREADED);
    }
    loop {
        let got = ask();
        {
            let mut s = STATE.lock().unwrap();
            match got {
                Some((mut n, key, bytes)) => {
                    // another song: none shown till its picture is read (or the last one's, when it is the same)
                    let new_song = key != s.cover_key;
                    if new_song {
                        s.cover_key = key.clone();
                        s.key_at = Some(Instant::now());
                    }
                    let mut cover = if new_song && bytes.is_none() { None } else { s.now.as_ref().and_then(|o| o.cover.clone()) };
                    // a picture other than the one shown: to a file of its own
                    if let (Some(bytes), Some(dir)) = (bytes, s.dir.clone()) {
                        let h = {
                            use std::hash::{Hash, Hasher};
                            let mut hs = std::collections::hash_map::DefaultHasher::new();
                            bytes.hash(&mut hs);
                            hs.finish()
                        };
                        if h != s.cover_hash || cover.is_none() {
                            s.cover_seq += 1;
                            let f = dir.join(format!("cover-{}.img", s.cover_seq % 4));
                            let _ = std::fs::create_dir_all(&dir);
                            if std::fs::write(&f, &bytes).is_ok() {
                                s.cover_hash = h;
                                cover = Some((f.to_string_lossy().into_owned(), s.cover_seq));
                            }
                        }
                    }
                    n.cover = cover;
                    s.now = Some(n);
                    s.at = Some(Instant::now());
                }
                None => {
                    s.now = None;
                    s.cover_key.clear();
                    s.cover_hash = 0;
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}

/// The current session asked: what plays, the cover's key (title and artist) and its cover's bytes — read with a new
/// song, each time for a while after it, then every fifth second (the caller keeps a picture only when it differs).
fn ask() -> Option<(Now, String, Option<Vec<u8>>)> {
    use windows::Media::Control::{GlobalSystemMediaTransportControlsSessionManager as Manager, GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status};
    let mgr = Manager::RequestAsync().ok()?.get().ok()?;
    // the one that plays (Windows' "current" may be a paused video in a browser while music plays elsewhere); none
    // playing: Windows' own
    let playing_one = mgr.GetSessions().ok().and_then(|all| {
        (0..all.Size().unwrap_or(0)).filter_map(|i| all.GetAt(i).ok()).find(|s| s.GetPlaybackInfo().ok().and_then(|i| i.PlaybackStatus().ok()).is_some_and(|st| st == Status::Playing))
    });
    let session = match playing_one {
        Some(s) => s,
        None => mgr.GetCurrentSession().ok()?,
    };
    let props = session.TryGetMediaPropertiesAsync().ok()?.get().ok()?;
    let title = props.Title().ok()?.to_string();
    if title.trim().is_empty() {
        return None;
    }
    let artist = props.Artist().map(|a| a.to_string()).unwrap_or_default();
    let tl = session.GetTimelineProperties().ok()?;
    let ticks = |d: windows::Foundation::TimeSpan| d.Duration as f32 / 10_000_000.0;
    let mut pos = tl.Position().map(ticks).unwrap_or(0.0);
    let len = tl.EndTime().map(ticks).unwrap_or(0.0);
    let playing = session.GetPlaybackInfo().ok().and_then(|i| i.PlaybackStatus().ok()).is_some_and(|s| s == Status::Playing);
    // the place is as of its last update: on to now while it plays
    if playing {
        if let Ok(upd) = tl.LastUpdatedTime() {
            // DateTime: 100 ns since 1601
            let now_ft = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() / 100) + 116_444_736_000_000_000) as i64;
            let since = (now_ft - upd.UniversalTime) as f32 / 10_000_000.0;
            if (0.0..36_000.0).contains(&since) {
                pos += since;
            }
        }
    }
    let key = format!("{title}\u{1}{artist}");
    let want = {
        let mut s = STATE.lock().unwrap();
        s.asks = s.asks.wrapping_add(1);
        key != s.cover_key || s.key_at.is_some_and(|at| at.elapsed().as_secs_f32() < COVER_SETTLES) || s.asks % 10 == 0
    };
    let bytes = if want { cover(&props) } else { None };
    Some((Now { title, artist, pos, len, playing, cover: None }, key, bytes))
}

fn cover(props: &windows::Media::Control::GlobalSystemMediaTransportControlsSessionMediaProperties) -> Option<Vec<u8>> {
    use windows::Storage::Streams::DataReader;
    let r = props.Thumbnail().ok()?;
    let stream = r.OpenReadAsync().ok()?.get().ok()?;
    let size = stream.Size().ok()? as u32;
    if size == 0 || size > 16 << 20 {
        return None;
    }
    let reader = DataReader::CreateDataReader(&stream).ok()?;
    reader.LoadAsync(size).ok()?.get().ok()?;
    let mut buf = vec![0u8; size as usize];
    reader.ReadBytes(&mut buf).ok()?;
    Some(buf)
}

/// What plays, told to go back, pause or play on, or go on to the next (the loader's corner: "prev", "toggle", "next").
pub fn control(cmd: &str) {
    let cmd = cmd.to_string();
    std::thread::spawn(move || {
        use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager as Manager;
        use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};
        unsafe {
            let _ = RoInitialize(RO_INIT_MULTITHREADED);
        }
        let Some(session) = Manager::RequestAsync().ok().and_then(|r| r.get().ok()).and_then(|m| m.GetCurrentSession().ok()) else { return };
        let op = match cmd.as_str() {
            "prev" => session.TrySkipPreviousAsync(),
            "next" => session.TrySkipNextAsync(),
            _ => session.TryTogglePlayPauseAsync(),
        };
        if let Ok(op) = op {
            let _ = op.get();
        }
    });
}
