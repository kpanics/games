//! The menus' pictures (worlds, servers' icons, packs' icons, faces): pixels handed over by Java, packed into one
//! atlas the GUI's shader reads (render.rs uploads what is new each frame). When it is full it is packed again from
//! the pixels kept here, pictures unused for a while left out.

use std::collections::HashMap;
use std::sync::Mutex;

/// The atlas's side, px (RGBA8: 16 MB, made only once a picture is first put).
pub const SIZE: u32 = 2048;
/// Air round each picture (smoothed pictures read their neighbours' edge).
const PAD: u32 = 2;

struct Entry {
    at: [u32; 4],
    px: std::sync::Arc<Vec<u8>>,
    used: u64,
}

struct Shelf {
    y: u32,
    h: u32,
    x: u32,
}

#[derive(Default)]
struct Store {
    map: HashMap<String, Entry>,
    shelves: Vec<Shelf>,
    /// Pictures to copy into the atlas: x, y, w, h and their pixels.
    pending: Vec<([u32; 4], std::sync::Arc<Vec<u8>>)>,
    frame: u64,
}

static STORE: Mutex<Option<Store>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut Store) -> R) -> R {
    let mut s = STORE.lock().unwrap();
    f(s.get_or_insert_with(Store::default))
}

impl Store {
    fn place(&mut self, w: u32, h: u32) -> Option<[u32; 2]> {
        let (pw, ph) = (w + PAD * 2, h + PAD * 2);
        if pw > SIZE || ph > SIZE {
            return None;
        }
        for s in self.shelves.iter_mut() {
            if ph <= s.h && s.x + pw <= SIZE {
                let at = [s.x + PAD, s.y + PAD];
                s.x += pw;
                return Some(at);
            }
        }
        let y = self.shelves.last().map_or(0, |s| s.y + s.h);
        if y + ph > SIZE {
            return None;
        }
        self.shelves.push(Shelf { y, h: ph, x: pw });
        Some([PAD, y + PAD])
    }

    /// Everything packed again, tallest first; pictures not drawn for a while are let go.
    fn repack(&mut self) {
        let frame = self.frame;
        self.map.retain(|_, e| frame.saturating_sub(e.used) < 600);
        self.shelves.clear();
        self.pending.clear();
        let mut keys: Vec<String> = self.map.keys().cloned().collect();
        keys.sort_by_key(|k| std::cmp::Reverse(self.map[k].at[3]));
        for k in keys {
            let (w, h) = (self.map[&k].at[2], self.map[&k].at[3]);
            match self.place(w, h) {
                Some(at) => {
                    let e = self.map.get_mut(&k).unwrap();
                    e.at = [at[0], at[1], w, h];
                    self.pending.push((e.at, e.px.clone()));
                }
                None => {
                    self.map.remove(&k);
                }
            }
        }
    }
}

/// A picture `w` × `h`, RGBA8 rows from the top, under `key` (a new one under the same key takes its place).
pub fn put(key: &str, w: u32, h: u32, rgba: Vec<u8>) {
    if w == 0 || h == 0 || rgba.len() < (w * h * 4) as usize {
        return;
    }
    with(|s| {
        let px = std::sync::Arc::new(rgba);
        if let Some(e) = s.map.get_mut(key) {
            if e.at[2] == w && e.at[3] == h {
                e.px = px.clone();
                s.pending.push((e.at, px));
                return;
            }
            s.map.remove(key);
        }
        let at = match s.place(w, h) {
            Some(a) => a,
            None => {
                s.repack();
                match s.place(w, h) {
                    Some(a) => a,
                    None => return,
                }
            }
        };
        let e = Entry { at: [at[0], at[1], w, h], px: px.clone(), used: s.frame };
        s.pending.push((e.at, px));
        s.map.insert(key.to_string(), e);
    })
}

pub fn has(key: &str) -> bool {
    with(|s| s.map.contains_key(key))
}

/// Where `key` is in the atlas (x0 y0 x1 y1, atlas px), its size; it counts as used.
pub fn get(key: &str) -> Option<([f32; 4], [u32; 2])> {
    with(|s| {
        let f = s.frame;
        s.map.get_mut(key).map(|e| {
            e.used = f;
            let [x, y, w, h] = e.at;
            ([x as f32, y as f32, (x + w) as f32, (y + h) as f32], [w, h])
        })
    })
}

/// The renderer's frame: what to copy into the atlas now.
pub fn drain() -> Vec<([u32; 4], std::sync::Arc<Vec<u8>>)> {
    with(|s| {
        s.frame += 1;
        std::mem::take(&mut s.pending)
    })
}

/// Whether any picture was ever put (the atlas is made then).
pub fn any() -> bool {
    with(|s| !s.map.is_empty() || !s.pending.is_empty())
}
