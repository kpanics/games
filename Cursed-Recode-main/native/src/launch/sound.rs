//! The launch's sound: the approved preview's effects (rendered from its own synthesis, Desktop\Cursed Recode
//! Launch\tools\render_sfx.mjs) and her track, mixed here and played through waveOut. The window before the game
//! sounds long before the game's sound engine is up, and the music runs on from the flash into the menu unbroken.
//!
//! Her rules (10.10): no hum or drone, no fire's roar (only its crackle), no sound on pressing — a tick on hovering;
//! the spark going out on the way out. The music quiet (0.05), in with the flash over 7 s, out over its last 6 s, a
//! pause, again from the start.

use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use windows_sys::Win32::Media::Audio::*;
use windows_sys::Win32::Media::MMSYSERR_NOERROR;
use windows_sys::Win32::System::Threading::{CreateEventW, WaitForSingleObject};

const RATE: usize = 44_100;
/// Frames a waveOut buffer, buffers in the queue: ~80 ms of sound ahead.
const CHUNK: usize = 882;
const QUEUE: usize = 4;
const MUSIC_LEVEL: f32 = 0.05;
/// Where her track is begun (s): its first half minute is near silence rising (her word, 15.10: from where it sounds).
const MUSIC_FROM: f32 = 32.0;

#[derive(Clone, Copy)]
pub enum Sfx {
    Spark,
    Crackle,
    Whoosh,
    Shimmer,
    Tick,
    Snuff,
    /// v12's own (made here, synth): the line drawn to the window's edges, the window opening along it, a feather's
    /// stroke, a letter rewritten, the wings opening / the world unloading, the window closing onto the line
    Line,
    Open,
    Stroke(u8),
    Letter(u8),
    SweepUp,
    SweepDown,
    Close,
}

/// v12's sounds, made from the preview's synthesis (v12/js/sound.js): bands of noise sweeping in pitch, a dry ring; a
/// little room after them (its reverb's send, as a few echoes). Stereo, 44.1 kHz.
mod synth {
    use super::RATE;
    const OUT: f32 = 0.85;

    struct Noise(u32);
    impl Noise {
        fn next(&mut self) -> f32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 17;
            self.0 ^= self.0 << 5;
            (self.0 >> 8) as f32 / 8_388_608.0 - 1.0
        }
    }

    /// WebAudio's envelope: from nothing, a line up to `amp` at `up` s, then down exponentially to nothing at `end` s.
    fn env(t: f32, up: f32, amp: f32, end: f32) -> f32 {
        if t < up {
            0.0001 + (amp - 0.0001) * t / up
        } else if t < end {
            amp * (0.0001f32 / amp).powf((t - up) / (end - up))
        } else {
            0.0
        }
    }

    /// The room: a few echoes, softer and duller each (the preview's convolver send).
    fn room(dry: &mut Vec<f32>, send: f32) {
        let taps = [(0.023, 0.5), (0.041, 0.36), (0.067, 0.26), (0.097, 0.19), (0.139, 0.13), (0.193, 0.08)];
        let tail = (0.25 * RATE as f32) as usize * 2;
        let n = dry.len();
        dry.resize(n + tail, 0.0);
        let src: Vec<f32> = dry[..n].to_vec();
        for (d, g) in taps {
            let off = (d * RATE as f32) as usize * 2;
            let mut lp = [0.0f32; 2];
            for (k, &v) in src.iter().enumerate() {
                let c = k & 1;
                lp[c] += (v - lp[c]) * 0.35;
                dry[k + off] += lp[c] * g * send;
            }
        }
    }

    /// A band of noise, its middle sliding from `f0` to `f1` (Hz) over `dur` s; panned (-1 left .. 1 right).
    pub fn hiss(dur: f32, f0: f32, f1: f32, amp: f32, q: f32, pan: f32, seed: u32) -> Vec<f32> {
        let n = (dur * RATE as f32) as usize + 64;
        let mut out = vec![0.0f32; n * 2];
        let mut rng = Noise(seed | 1);
        let (mut x1, mut x2, mut y1, mut y2) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        let (mut b0, mut b2, mut a1, mut a2) = (0.0, 0.0, 0.0, 0.0);
        let x = (pan.clamp(-1.0, 1.0) + 1.0) * 0.5;
        let (gl, gr) = if pan == 0.0 { (1.0, 1.0) } else { ((x * std::f32::consts::FRAC_PI_2).cos(), (x * std::f32::consts::FRAC_PI_2).sin()) };
        for i in 0..n {
            let t = i as f32 / RATE as f32;
            if i % 16 == 0 {
                // the band's middle (an exponential slide), the filter (RBJ band-pass, 0 dB at its peak)
                let f = f0 * (f1 / f0).powf((t / dur).min(1.0));
                let w0 = std::f32::consts::TAU * f / RATE as f32;
                let alpha = w0.sin() / (2.0 * q);
                let a0 = 1.0 + alpha;
                b0 = alpha / a0;
                b2 = -alpha / a0;
                a1 = -2.0 * w0.cos() / a0;
                a2 = (1.0 - alpha) / a0;
            }
            let xn = rng.next();
            let y = b0 * xn + b2 * x2 - a1 * y1 - a2 * y2;
            x2 = x1;
            x1 = xn;
            y2 = y1;
            y1 = y;
            let v = y * env(t, dur * 0.3, amp, dur) * OUT;
            out[i * 2] = v * gl;
            out[i * 2 + 1] = v * gr;
        }
        room(&mut out, 0.35);
        out
    }

    /// A letter rewritten: a dry metal tick, each lower than the last (`i` its place).
    pub fn letter(i: u8) -> Vec<f32> {
        let f = 1900.0 - i as f32 * 120.0;
        let n = (0.12 * RATE as f32) as usize;
        let mut out = vec![0.0f32; n * 2];
        for k in 0..n {
            let t = k as f32 / RATE as f32;
            let s = (std::f32::consts::TAU * f * t).sin() * env(t, 0.002, 0.03, 0.09);
            let ph = (f * 2.76 * t).fract();
            let tri = (4.0 * (ph - 0.5).abs() - 1.0) * env(t, 0.001, 0.008, 0.05);
            let v = (s + tri) * OUT;
            out[k * 2] = v;
            out[k * 2 + 1] = v;
        }
        room(&mut out, 0.3);
        out
    }
}

struct Voice {
    data: Arc<Vec<f32>>,
    at: usize,
    delay: usize,
    gain: f32,
}

struct Music {
    reader: Option<lewton::inside_ogg::OggStreamReader<Cursor<&'static [u8]>>>,
    pending: Vec<f32>,
    pending_at: usize,
    /// Frames played since the start.
    pos: usize,
    len: usize,
    gain: f32,
    target: f32,
    /// Per frame, how much of the way to the target.
    rate: f32,
    wanted: bool,
    paused: bool,
    /// Frames of the pause before it starts again (after its end).
    wait: usize,
}

struct Mixer {
    voices: Vec<Voice>,
    sfx: Vec<Arc<Vec<f32>>>,
    music: Music,
    master: f32,
    music_volume: f32,
    seed: u32,
}

static MIXER: OnceLock<Mutex<Mixer>> = OnceLock::new();
static RUNNING: AtomicBool = AtomicBool::new(false);
/// Development (the launch's pictures): no sound device, the mix made with the pictures' clock (`pump`).
static OFFLINE: AtomicBool = AtomicBool::new(false);

/// Development: mix with the pictures' clock instead of a sound device (CURSED_DEV_SOUNDTAP keeps it).
pub fn offline() {
    OFFLINE.store(true, Ordering::Release);
}

/// Development: `dt` seconds of the mix made now (offline only).
pub fn pump(dt: f32) {
    if !OFFLINE.load(Ordering::Acquire) {
        return;
    }
    static CARRY: Mutex<f32> = Mutex::new(0.0);
    let mut c = CARRY.lock().unwrap();
    *c += dt * RATE as f32;
    let n = *c as usize;
    *c -= n as f32;
    if n > 0 {
        let mut buf = vec![0i16; n * 2];
        mixer().lock().unwrap().fill(&mut buf);
    }
}
static MUSIC_OGG: &[u8] = include_bytes!("../../assets/launch/music.ogg");

fn tap() -> Option<&'static Mutex<std::fs::File>> {
    static TAP: OnceLock<Option<Mutex<std::fs::File>>> = OnceLock::new();
    TAP.get_or_init(|| std::env::var("CURSED_DEV_SOUNDTAP").ok().and_then(|p| std::fs::File::create(p).ok()).map(Mutex::new)).as_ref()
}

fn decode(ogg: &[u8]) -> Vec<f32> {
    let mut out = Vec::new();
    if let Ok(mut r) = lewton::inside_ogg::OggStreamReader::new(Cursor::new(ogg)) {
        let ch = r.ident_hdr.audio_channels as usize;
        while let Ok(Some(p)) = r.read_dec_packet_itl() {
            for f in p.chunks(ch.max(1)) {
                let l = f[0] as f32 / 32768.0;
                let rr = if ch > 1 { f[1] as f32 / 32768.0 } else { l };
                out.push(l);
                out.push(rr);
            }
        }
    }
    out
}

/// An Ogg stream's length in frames: the granule position of its last page.
fn ogg_frames(ogg: &[u8]) -> usize {
    let mut at = ogg.len().saturating_sub(4);
    while at > 0 {
        if &ogg[at..at + 4] == b"OggS" && at + 14 <= ogg.len() {
            return u64::from_le_bytes(ogg[at + 6..at + 14].try_into().unwrap()) as usize;
        }
        at -= 1;
    }
    0
}

fn mixer() -> &'static Mutex<Mixer> {
    MIXER.get_or_init(|| {
        let s = |b: &[u8]| Arc::new(decode(b));
        let sfx = vec![
            s(include_bytes!("../../assets/launch/sfx/spark.ogg")),
            s(include_bytes!("../../assets/launch/sfx/whoosh.ogg")),
            s(include_bytes!("../../assets/launch/sfx/shimmer.ogg")),
            s(include_bytes!("../../assets/launch/sfx/tick.ogg")),
            s(include_bytes!("../../assets/launch/sfx/snuff.ogg")),
            s(include_bytes!("../../assets/launch/sfx/crackle0.ogg")),
            s(include_bytes!("../../assets/launch/sfx/crackle1.ogg")),
            s(include_bytes!("../../assets/launch/sfx/crackle2.ogg")),
            s(include_bytes!("../../assets/launch/sfx/crackle3.ogg")),
            s(include_bytes!("../../assets/launch/sfx/crackle4.ogg")),
            s(include_bytes!("../../assets/launch/sfx/crackle5.ogg")),
            s(include_bytes!("../../assets/launch/sfx/crackle6.ogg")),
            s(include_bytes!("../../assets/launch/sfx/crackle7.ogg")),
        ];
        // v12's: 13 the line, 14 the opening, 15..24 the strokes, 24..30 the letters, 30 up, 31 down, 32 the closing
        let mut sfx = sfx;
        sfx.push(Arc::new(mixed(&[synth::hiss(1.01, 1800.0, 5200.0, 0.05, 1.6, -0.6, 11), synth::hiss(1.01, 1900.0, 5600.0, 0.05, 1.6, 0.6, 12)])));
        sfx.push(Arc::new(synth::hiss(1.23, 700.0, 2600.0, 0.06, 0.7, 0.0, 13)));
        for k in 0..9u32 {
            let pan = [-0.3, 0.25, -0.1, 0.4, -0.42, 0.12, 0.33, -0.22, 0.05][k as usize];
            sfx.push(Arc::new(synth::hiss(0.32, 2400.0 + k as f32 * 300.0, 7000.0, 0.022, 2.2, pan, 20 + k)));
        }
        for i in 0..6u8 {
            sfx.push(Arc::new(synth::letter(i)));
        }
        sfx.push(Arc::new(synth::hiss(1.6, 500.0, 3400.0, 0.05, 0.6, 0.0, 40)));
        sfx.push(Arc::new(synth::hiss(1.6, 3000.0, 450.0, 0.05, 0.6, 0.0, 41)));
        sfx.push(Arc::new(synth::hiss(0.95, 2600.0, 800.0, 0.055, 0.7, 0.0, 42)));
        let len = ogg_frames(MUSIC_OGG);
        Mutex::new(Mixer {
            voices: Vec::new(),
            sfx,
            music: Music { reader: None, pending: Vec::new(), pending_at: 0, pos: 0, len, gain: 0.0, target: 0.0, rate: 0.0, wanted: false, paused: false, wait: 0 },
            master: 1.0,
            music_volume: 1.0,
            seed: 0x9E37_79B9,
        })
    })
}

/// Starts the sound thread (once).
pub fn start() {
    if RUNNING.swap(true, Ordering::AcqRel) {
        return;
    }
    let _ = mixer();
    if OFFLINE.load(Ordering::Acquire) {
        return;
    }
    let _ = std::thread::Builder::new().name("Cursed launch sound".into()).spawn(|| unsafe { run() });
}

/// The program of its own that keeps the music (the game's side), if there is one.
fn far() -> Option<&'static crate::twin::link::Link> {
    crate::twin::external()
}

fn ask(l: &crate::twin::link::Link, cmd: u32) {
    l.music_cmd.store(cmd, Ordering::Release);
    l.music_seq.fetch_add(1, Ordering::AcqRel);
}

/// The game's volumes, 0..1: master and music.
pub fn volume(master: f32, music: f32) {
    if let Some(l) = far() {
        crate::twin::link::set_f32(&l.volume[0], master);
        crate::twin::link::set_f32(&l.volume[1], music);
    }
    let mut m = mixer().lock().unwrap();
    m.master = master.clamp(0.0, 1.0);
    m.music_volume = music.clamp(0.0, 1.0);
}

/// An effect, `gain` on top of its own level, after `delay` seconds.
pub fn play(s: Sfx, gain: f32, delay: f32) {
    start();
    let mut m = mixer().lock().unwrap();
    let data = match s {
        Sfx::Spark => m.sfx[0].clone(),
        Sfx::Whoosh => m.sfx[1].clone(),
        Sfx::Shimmer => m.sfx[2].clone(),
        Sfx::Tick => m.sfx[3].clone(),
        Sfx::Snuff => m.sfx[4].clone(),
        Sfx::Crackle => {
            m.seed ^= m.seed << 13;
            m.seed ^= m.seed >> 17;
            m.seed ^= m.seed << 5;
            m.sfx[5 + (m.seed % 8) as usize].clone()
        }
        Sfx::Line => m.sfx[13].clone(),
        Sfx::Open => m.sfx[14].clone(),
        Sfx::Stroke(k) => m.sfx[15 + (k as usize).min(8)].clone(),
        Sfx::Letter(i) => m.sfx[24 + (i as usize).min(5)].clone(),
        Sfx::SweepUp => m.sfx[30].clone(),
        Sfx::SweepDown => m.sfx[31].clone(),
        Sfx::Close => m.sfx[32].clone(),
    };
    m.voices.push(Voice { data, at: 0, delay: (delay.max(0.0) * RATE as f32) as usize, gain });
}

fn approach(m: &mut Music, target: f32, seconds: f32) {
    m.target = target;
    // setTargetAtTime with time constant seconds / 3.2, per frame
    m.rate = 1.0 - (-3.2 / (seconds.max(0.02) * RATE as f32)).exp();
}

/// The music comes in (from where it was, or the start).
pub fn music_in(fade: f32) {
    if let Some(l) = far() {
        return ask(l, 3);
    }
    start();
    let mut m = mixer().lock().unwrap();
    m.music.wanted = true;
    if !m.music.paused {
        approach(&mut m.music, MUSIC_LEVEL, fade);
    }
}

/// The music goes, and starts from the beginning next time.
pub fn music_out(fade: f32) {
    if let Some(l) = far() {
        return ask(l, 2);
    }
    let mut m = mixer().lock().unwrap();
    m.music.wanted = false;
    approach(&mut m.music, 0.0, fade);
}

/// Pause or play on; returns whether it plays now.
pub fn music_toggle() -> bool {
    if let Some(l) = far() {
        let on = crate::twin::link::f32_of(&l.music[2]) > 0.5;
        ask(l, 0);
        return !on;
    }
    start();
    let mut m = mixer().lock().unwrap();
    if !m.music.paused && m.music.target > 0.0 {
        m.music.paused = true;
        approach(&mut m.music, 0.0, 0.6);
        false
    } else {
        m.music.paused = false;
        m.music.wanted = true;
        approach(&mut m.music, MUSIC_LEVEL, 1.4);
        true
    }
}

/// From the start, softly.
pub fn music_restart() {
    if let Some(l) = far() {
        return ask(l, 1);
    }
    start();
    let mut m = mixer().lock().unwrap();
    m.music.reader = None;
    m.music.pos = 0;
    m.music.pending.clear();
    m.music.gain = 0.0;
    m.music.paused = false;
    m.music.wanted = true;
    approach(&mut m.music, MUSIC_LEVEL, 1.4);
}

/// Seconds played, the track's length, whether it is heard.
pub fn music_state() -> (f32, f32, bool) {
    if let Some(l) = far() {
        use crate::twin::link::f32_of;
        return (f32_of(&l.music[0]), f32_of(&l.music[1]), f32_of(&l.music[2]) > 0.5);
    }
    let m = mixer().lock().unwrap();
    (m.music.pos as f32 / RATE as f32, m.music.len as f32 / RATE as f32, m.music.target > 0.0 && !m.music.paused)
}

impl Mixer {
    fn fill(&mut self, out: &mut [i16]) {
        let frames = out.len() / 2;
        let mut buf = vec![0f32; frames * 2];
        // effects
        for v in self.voices.iter_mut() {
            for f in 0..frames {
                if v.delay > 0 {
                    v.delay -= 1;
                    continue;
                }
                if v.at * 2 + 1 >= v.data.len() {
                    break;
                }
                buf[f * 2] += v.data[v.at * 2] * v.gain;
                buf[f * 2 + 1] += v.data[v.at * 2 + 1] * v.gain;
                v.at += 1;
            }
        }
        self.voices.retain(|v| v.delay > 0 || v.at * 2 + 1 < v.data.len());
        // music: streamed from the track; its last 6 s fade, a pause of 2.2 s, then again
        let mu = &mut self.music;
        // gone out: next time from the start
        if !mu.wanted && mu.target == 0.0 && mu.gain < 1e-4 && mu.pos > 0 {
            mu.reader = None;
            mu.pending.clear();
            mu.pending_at = 0;
            mu.pos = 0;
            mu.gain = 0.0;
        }
        let audible = mu.gain > 1e-5 || mu.target > 0.0;
        if audible && !mu.paused || mu.gain > 1e-5 {
            for f in 0..frames {
                mu.gain += (mu.target - mu.gain) * mu.rate;
                if mu.wait > 0 {
                    mu.wait -= 1;
                    continue;
                }
                if mu.pending_at * 2 >= mu.pending.len() {
                    if mu.reader.is_none() {
                        mu.reader = lewton::inside_ogg::OggStreamReader::new(Cursor::new(MUSIC_OGG)).ok();
                        mu.pos = 0;
                        // from where it sounds, not its silent start
                        let from = (MUSIC_FROM * RATE as f32) as u64;
                        if let Some(r) = mu.reader.as_mut() {
                            if r.seek_absgp_pg(from).is_ok() {
                                mu.pos = from as usize;
                            }
                        }
                    }
                    let next = mu.reader.as_mut().and_then(|r| r.read_dec_packet_itl().ok().flatten());
                    match next {
                        Some(p) => {
                            mu.pending = p.iter().map(|&x| x as f32 / 32768.0).collect();
                            mu.pending_at = 0;
                        }
                        None => {
                            // the end: a pause, then from the start (if it is still wanted)
                            mu.reader = None;
                            mu.pending.clear();
                            mu.pending_at = 0;
                            mu.pos = 0;
                            mu.wait = (2.2 * RATE as f32) as usize;
                            if !mu.wanted {
                                mu.target = 0.0;
                                mu.gain = 0.0;
                            }
                            continue;
                        }
                    }
                    if mu.pending.is_empty() {
                        continue;
                    }
                }
                let left = (mu.len.saturating_sub(mu.pos)) as f32 / RATE as f32;
                let end = (left / 6.0).clamp(0.0, 1.0);
                let g = mu.gain * end * self.music_volume;
                buf[f * 2] += mu.pending[mu.pending_at * 2] * g;
                buf[f * 2 + 1] += mu.pending[mu.pending_at * 2 + 1] * g;
                mu.pending_at += 1;
                mu.pos += 1;
            }
        }
        // development: CURSED_DEV_SOUNDTAP=file keeps what is mixed (before the volume), to check the sound unheard
        if let Some(tap) = tap() {
            use std::io::Write;
            let bytes: Vec<u8> = buf.iter().flat_map(|x| ((x.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes()).collect();
            let _ = tap.lock().unwrap().write_all(&bytes);
        }
        for (o, x) in out.iter_mut().zip(buf.iter()) {
            // a soft limit (the preview's compressor kept the sum in check)
            let y = x * self.master;
            let y = if y.abs() < 0.8 { y } else { y.signum() * (0.8 + 0.2 * ((y.abs() - 0.8) / 0.2).tanh()) };
            *o = (y.clamp(-1.0, 1.0) * 32767.0) as i16;
        }
    }
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
        WaitForSingleObject(event, 30);
    }
}

// ---- what sounds when -----------------------------------------------------------------------------------------

struct Events {
    fired: u32,
    /// The claw's taps heard so far while the word waits for the game.
    taps: u32,
}

static EVENTS: Mutex<Events> = Mutex::new(Events { fired: 0, taps: 0 });

fn once(e: &mut Events, bit: u32) -> bool {
    let first = e.fired & bit == 0;
    e.fired |= bit;
    first
}

/// A launch from the start (the next card plays its sounds again).
pub fn reset() {
    let mut e = EVENTS.lock().unwrap();
    e.fired = 0;
    e.taps = 0;
}

/// Two sounds as one (the longer's length).
fn mixed(parts: &[Vec<f32>]) -> Vec<f32> {
    let n = parts.iter().map(|p| p.len()).max().unwrap_or(0);
    let mut out = vec![0.0f32; n];
    for p in parts {
        for (o, v) in out.iter_mut().zip(p) {
            *o += v;
        }
    }
    out
}

/// The game takes the card over (the desk's second `d`, the story's `t`): what was heard before is not played again
/// here.
pub fn card_from(d: f32, t: f32) {
    launch_at(d, t, 0.0, true);
}

/// The launch («ink»): no sounds of its own (her word, 15.10: «оно тут не очень»), the music only — from the start, her
/// track rising. `d` the desk's second (the clock that never waits), `t` the story's.
pub fn launch(d: f32, t: f32, dt: f32) {
    launch_at(d, t, dt, false);
}

fn launch_at(d: f32, _t: f32, _dt: f32, silent: bool) {
    let mut e = EVENTS.lock().unwrap();
    if d >= 0.3 && once(&mut e, 1 << 9) && !silent {
        drop(e);
        music_in(1.0);
    }
}

/// A way out: no sounds of its own either (the music goes as the menu asks: launch_music).
pub fn way_out(_e_s: f32, _x: &super::timeline::Exit) {}
