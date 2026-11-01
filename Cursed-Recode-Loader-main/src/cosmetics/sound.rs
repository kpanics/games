//! CS2's sounds of what the hands hold (tools/cs2pack/sounds.py packs them): its sound events — a shot, the bolt, a
//! knife's swish, the inspect's catch — each its sounds to pick from, its volume, its pitch and the spread of it, its
//! delay, the events it sets off with it; played through waveOut on a thread of their own at the game's volume
//! (master and players, said by Java). The clips say when (their events, cosmetics/hands.rs); the rifle says its shot
//! and its scope (cosmetics/rifle.rs).

use super::store;
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use windows_sys::Win32::Media::Audio::*;
use windows_sys::Win32::Media::MMSYSERR_NOERROR;
use windows_sys::Win32::System::Threading::{CreateEventW, WaitForSingleObject};

const RATE: usize = 44_100;
/// Frames a buffer, buffers queued: ~40 ms ahead (a shot is heard with the click).
const CHUNK: usize = 441;
const QUEUE: usize = 4;
/// The most heard at once.
const MOST: usize = 24;

/// A sound event (events.txt's line).
#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub volume: f32,
    pub pitch: f32,
    pub spread: (f32, f32),
    pub delay: f32,
    pub sounds: Vec<String>,
    pub children: Vec<String>,
}

/// events.txt read: name → its event.
pub fn parse_events(text: &str) -> HashMap<String, Event> {
    let mut out = HashMap::new();
    for line in text.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 6 {
            continue;
        }
        let num = |k: usize, d: f32| f.get(k).and_then(|v| v.parse().ok()).unwrap_or(d);
        let list = |k: usize| f.get(k).map_or(Vec::new(), |v| v.split(',').filter(|s| !s.is_empty()).map(String::from).collect());
        out.insert(f[0].to_string(), Event { volume: num(1, 1.0), pitch: num(2, 1.0), spread: (num(3, 0.0), num(4, 0.0)), delay: num(5, 0.0), sounds: list(6), children: list(7) });
    }
    out
}

struct Voice {
    data: Arc<Vec<f32>>,
    at: f64,
    step: f64,
    delay: usize,
    gain: f32,
}

struct Mixer {
    voices: Vec<Voice>,
    volume: f32,
}

impl Mixer {
    fn fill(&mut self, out: &mut [i16]) {
        let frames = out.len() / 2;
        let mut mix = vec![0f32; frames * 2];
        self.voices.retain_mut(|v| {
            let mut i = 0;
            if v.delay > 0 {
                let skip = v.delay.min(frames);
                v.delay -= skip;
                i = skip;
            }
            let n = v.data.len() / 2;
            while i < frames {
                let k = v.at as usize;
                if k + 1 >= n {
                    return false;
                }
                let f = (v.at - k as f64) as f32;
                let l = v.data[k * 2] + (v.data[k * 2 + 2] - v.data[k * 2]) * f;
                let r = v.data[k * 2 + 1] + (v.data[k * 2 + 3] - v.data[k * 2 + 1]) * f;
                mix[i * 2] += l * v.gain;
                mix[i * 2 + 1] += r * v.gain;
                v.at += v.step;
                i += 1;
            }
            true
        });
        if let Some(t) = tap() {
            // the stand's ear (its game is silent): the mix as it would sound, into a file (16-bit stereo, raw)
            use std::io::Write;
            let bytes: Vec<u8> = mix.iter().flat_map(|m| ((m.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes()).collect();
            let _ = t.lock().unwrap().write_all(&bytes);
        }
        for (o, m) in out.iter_mut().zip(&mix) {
            // soft past the top: two shots at once do not crack
            let x = m * self.volume;
            let y = if x.abs() <= 0.8 { x } else { x.signum() * (0.8 + 0.2 * ((x.abs() - 0.8) / 0.2).tanh()) };
            *o = (y.clamp(-1.0, 1.0) * 32767.0) as i16;
        }
    }
}

/// The stand's file the mix goes to besides (CURSED_DEV_COSM_SOUNDTAP).
fn tap() -> Option<&'static Mutex<std::fs::File>> {
    static TAP: OnceLock<Option<Mutex<std::fs::File>>> = OnceLock::new();
    TAP.get_or_init(|| std::env::var("CURSED_DEV_COSM_SOUNDTAP").ok().and_then(|p| std::fs::File::create(p).ok()).map(Mutex::new)).as_ref()
}

struct Bank {
    events: Option<HashMap<String, Event>>,
    sounds: HashMap<String, Arc<Vec<f32>>>,
    asked: std::collections::HashSet<String>,
    seed: u32,
    /// when each event was last heard (the same one again at once is one sound, not two at once, louder)
    last: HashMap<String, std::time::Instant>,
}

fn mixer() -> &'static Mutex<Mixer> {
    static M: OnceLock<Mutex<Mixer>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(Mixer { voices: Vec::new(), volume: 1.0 }))
}

fn bank() -> &'static Mutex<Bank> {
    static B: OnceLock<Mutex<Bank>> = OnceLock::new();
    B.get_or_init(|| Mutex::new(Bank { events: None, sounds: HashMap::new(), asked: Default::default(), seed: 0x2545_F491, last: HashMap::new() }))
}

fn decode(ogg: &[u8]) -> Option<Vec<f32>> {
    let mut r = lewton::inside_ogg::OggStreamReader::new(Cursor::new(ogg)).ok()?;
    let ch = r.ident_hdr.audio_channels as usize;
    // The packer writes 44.1 kHz mono/stereo. Reject a different rate rather than change its pitch.
    if !(1..=2).contains(&ch) || r.ident_hdr.audio_sample_rate != RATE as u32 {
        return None;
    }
    let mut out = Vec::new();
    while let Some(p) = r.read_dec_packet_itl().ok()? {
        for f in p.chunks_exact(ch) {
            out.push(f[0] as f32 / 32768.0);
            out.push(f[if ch == 2 { 1 } else { 0 }] as f32 / 32768.0);
        }
    }
    (out.len() >= 4).then_some(out)
}

// Disk/network waits must not hold the bank used by play_at on the frame thread.
fn events_from(b: &Mutex<Bank>, read: impl FnOnce() -> Option<Vec<u8>>) -> Option<HashMap<String, Event>> {
    if let Some(events) = b.lock().unwrap().events.clone() {
        return Some(events);
    }
    let bytes = read()?;
    let events = parse_events(std::str::from_utf8(&bytes).ok()?);
    if events.is_empty() {
        return None;
    }
    let mut b = b.lock().unwrap();
    Some(b.events.get_or_insert(events).clone())
}

impl Bank {
    fn finish_sound(&mut self, name: String, pcm: Option<Vec<f32>>) {
        if let Some(pcm) = pcm.filter(|p| p.len() >= 4) {
            self.sounds.insert(name, Arc::new(pcm));
        } else {
            // A temporarily missing or invalid file can be requested on the next preparation.
            self.asked.remove(&name);
        }
    }
}

/// The game's volume for these (master × players, 0..1).
pub fn volume(v: f32) {
    mixer().lock().unwrap().volume = v.clamp(0.0, 1.0);
}

/// The events' sounds read and decoded ahead, on a thread (the kit's clips' events, the rifle's shot): `names` the
/// events, their children with them.
pub fn prepare(names: Vec<String>) {
    std::thread::Builder::new()
        .name("Cursed cosmetics sound".into())
        .spawn(move || {
            let Some(events) = events_from(bank(), || store::read_wait("sound/events.txt")) else { return };
            let mut want: Vec<String> = names;
            let mut seen = std::collections::HashSet::new();
            let mut files = Vec::new();
            while let Some(n) = want.pop() {
                if !seen.insert(n.clone()) {
                    continue;
                }
                if let Some(e) = events.get(&n) {
                    files.extend(e.sounds.iter().cloned());
                    want.extend(e.children.iter().cloned());
                }
            }
            for f in files {
                if !bank().lock().unwrap().asked.insert(f.clone()) {
                    continue;
                }
                let pcm = store::read_wait(&format!("sound/{f}.ogg")).and_then(|d| decode(&d));
                bank().lock().unwrap().finish_sound(f, pcm);
            }
        })
        .ok();
}

/// An event heard (its sounds not read yet: not heard).
pub fn play(name: &str) {
    play_at(name, 1.0, 0.0, 0);
}

fn play_at(name: &str, gain: f32, delay: f32, depth: u32) {
    if depth > 3 {
        return;
    }
    let (data, pitch, ev) = {
        let mut b = bank().lock().unwrap();
        let Some(e) = b.events.as_ref().and_then(|m| m.get(name)).cloned() else { return };
        // (heard a moment ago: once is enough — a quick switch of the hands set it off again, two at once were louder)
        if depth == 0 && b.last.get(name).is_some_and(|t| t.elapsed().as_secs_f32() < 0.06) {
            return;
        }
        b.last.insert(name.to_string(), std::time::Instant::now());
        b.seed ^= b.seed << 13;
        b.seed ^= b.seed >> 17;
        b.seed ^= b.seed << 5;
        let r = (b.seed >> 8) as f32 / 16_777_216.0;
        // one of its sounds that is read (a variant still coming was silence: the sound "now there, now not")
        let ready: Vec<&String> = e.sounds.iter().filter(|s| b.sounds.contains_key(*s)).collect();
        let pick = if ready.is_empty() { None } else { Some(ready[(b.seed as usize >> 3) % ready.len()].clone()) };
        let data = pick.and_then(|p| b.sounds.get(&p).cloned());
        let pitch = e.pitch * (1.0 + e.spread.0 + (e.spread.1 - e.spread.0) * r);
        (data, pitch, e)
    };
    let delay = delay + ev.delay;
    if let Some(data) = data {
        start();
        let mut m = mixer().lock().unwrap();
        if m.voices.len() < MOST {
            m.voices.push(Voice { data, at: 0.0, step: pitch.max(0.1) as f64, delay: (delay * RATE as f32) as usize, gain: gain * ev.volume });
        }
    }
    for c in &ev.children {
        play_at(c, gain, delay, depth + 1);
    }
}

static RUNNING: AtomicBool = AtomicBool::new(false);

fn start() {
    if RUNNING.swap(true, Ordering::AcqRel) {
        return;
    }
    let _ = std::thread::Builder::new().name("Cursed cosmetics mixer".into()).spawn(|| unsafe { run() });
}

unsafe fn run() {
    let fmt = WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_PCM as u16,
        nChannels: 2,
        nSamplesPerSec: RATE as u32,
        nAvgBytesPerSec: (RATE * 4) as u32,
        nBlockAlign: 4,
        wBitsPerSample: 16,
        cbSize: 0,
    };
    let event = CreateEventW(std::ptr::null(), 0, 0, std::ptr::null());
    let mut out: HWAVEOUT = std::ptr::null_mut();
    if waveOutOpen(&mut out, WAVE_MAPPER, &fmt, event as usize, 0, CALLBACK_EVENT) != MMSYSERR_NOERROR {
        RUNNING.store(false, Ordering::Release);
        return;
    }
    let mut bufs: Vec<Vec<i16>> = (0..QUEUE).map(|_| vec![0i16; CHUNK * 2]).collect();
    let mut heads: Vec<WAVEHDR> = (0..QUEUE).map(|_| std::mem::zeroed()).collect();
    for i in 0..QUEUE {
        heads[i].lpData = bufs[i].as_mut_ptr() as *mut u8;
        heads[i].dwBufferLength = (CHUNK * 4) as u32;
        waveOutPrepareHeader(out, &mut heads[i], std::mem::size_of::<WAVEHDR>() as u32);
        heads[i].dwFlags |= WHDR_DONE;
    }
    loop {
        for i in 0..QUEUE {
            if heads[i].dwFlags & WHDR_DONE != 0 {
                mixer().lock().unwrap().fill(&mut bufs[i]);
                heads[i].dwFlags &= !WHDR_DONE;
                waveOutWrite(out, &mut heads[i], std::mem::size_of::<WAVEHDR>() as u32);
            }
        }
        WaitForSingleObject(event, 20);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_are_read_as_packed() {
        let m = parse_events("Weapon_AWP.Single\t1\t1\t-0.03\t0.03\t0\tweapons.awp.awp_01,weapons.awp.awp_02\tWeapon_AWP.SingleDistant\nbad line\n");
        let e = &m["Weapon_AWP.Single"];
        assert_eq!(e.sounds.len(), 2);
        assert_eq!(e.children, vec!["Weapon_AWP.SingleDistant".to_string()]);
        assert!((e.spread.0 + 0.03).abs() < 1e-6);
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn the_mix_plays_a_sound_through_at_its_pitch() {
        let mut m = Mixer { voices: Vec::new(), volume: 1.0 };
        // a stereo ramp of 100 frames, played at double speed: done in about 50
        let data: Vec<f32> = (0..200).map(|i| (i / 2) as f32 / 400.0).collect();
        m.voices.push(Voice { data: Arc::new(data), at: 0.0, step: 2.0, delay: 10, gain: 1.0 });
        let mut out = vec![0i16; 2 * 100];
        m.fill(&mut out);
        assert_eq!(out[0], 0);
        assert!(out[2 * 12] > 0);
        assert!(m.voices.is_empty());
    }
    fn empty_bank() -> Bank {
        Bank { events: None, sounds: HashMap::new(), asked: Default::default(), seed: 1, last: HashMap::new() }
    }

    #[test]
    fn failed_event_load_retries_and_does_not_block_playback_bank() {
        let b = Mutex::new(empty_bank());
        assert!(events_from(&b, || { assert!(b.try_lock().is_ok()); None }).is_none());
        assert!(events_from(&b, || Some(b"broken".to_vec())).is_none());
        assert!(b.lock().unwrap().events.is_none());
        let events = events_from(&b, || {
            assert!(b.try_lock().is_ok(), "network reader must not hold the playback bank");
            Some(b"shot\t1\t1\t0\t0\t0\tclip\t".to_vec())
        }).unwrap();
        assert_eq!(events["shot"].sounds, vec!["clip"]);
        assert!(events_from(&b, || panic!("loaded events must be reused")).unwrap().contains_key("shot"));
    }

    #[test]
    fn failed_audio_does_not_become_a_ready_silent_variant() {
        let mut b = empty_bank();
        for bad in [None, Some(Vec::new()), decode(b"broken ogg")] {
            assert!(b.asked.insert("clip".into()));
            b.finish_sound("clip".into(), bad);
            assert!(!b.asked.contains("clip"));
            assert!(!b.sounds.contains_key("clip"));
        }
        assert!(b.asked.insert("clip".into()));
        b.finish_sound("clip".into(), Some(vec![0.1, 0.2, 0.3, 0.4]));
        assert_eq!(b.sounds["clip"].len(), 4);
        assert!(!b.asked.insert("clip".into()));
    }

    #[test]
    #[ignore = "requires CURSED_TEST_SOUND_PACK pointing to the local CS2 sound directory"]
    fn packed_audio_is_decodable_and_every_variant_exists() {
        let root = std::path::PathBuf::from(std::env::var("CURSED_TEST_SOUND_PACK").unwrap());
        let events = parse_events(&std::fs::read_to_string(root.join("events.txt")).unwrap());
        assert!(!events.is_empty());
        let mut files = std::collections::HashSet::new();
        for (name, event) in &events {
            for child in &event.children {
                assert!(events.contains_key(child), "{name}: missing child {child}");
            }
            files.extend(event.sounds.iter().cloned());
        }
        for file in &files {
            let bytes = std::fs::read(root.join(format!("{file}.ogg"))).unwrap();
            let pcm = decode(&bytes).unwrap_or_else(|| panic!("{file}: invalid audio"));
            assert!(pcm.iter().any(|x| x.abs() > 0.0001), "{file}: silent audio");
            let mut mixer = Mixer { voices: vec![Voice { data: Arc::new(pcm), at: 0.0, step: 1.0, delay: 0, gain: 1.0 }], volume: 0.0 };
            let mut muted = [1i16; CHUNK * 2];
            mixer.fill(&mut muted);
            assert!(muted.iter().all(|&x| x == 0));
        }
        eprintln!("Validated {} sound events and {} audio variants without playback", events.len(), files.len());
    }

}
