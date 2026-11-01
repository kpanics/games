//! The cosmetics' files read off the disk on a thread of their own (a model's few megabytes never stall a frame): a
//! model asked for is read, then its materials' textures (beside it, .rtx for each .png it names); what is read waits
//! for the frame thread to take it to the GPU (cosmetics/render.rs). Files live under one folder (the game's
//! cursed-recode/cosmetics, said by Java; CURSED_DEV_COSMETICS for the stand), each by its path from there; in the
//! game's they come from our server as first wanted (cosmetics/remote.rs): `read` never waits for one (the frame's
//! and the menu's threads ask again), `read_wait` does (threads of their own).

use super::model::Model;
use super::remote;
use super::texture::Image;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex, OnceLock};

pub enum Loaded {
    Model(String, Arc<Model>),
    Texture(String, Image),
}

struct Store {
    dir: Mutex<Option<PathBuf>>,
    asked: Mutex<HashSet<String>>,
    ready: Mutex<Vec<Loaded>>,
    models: Mutex<HashMap<String, Arc<Model>>>,
    /// What could not be read (not there, broken): the hands give up on it and leave the game's own.
    failed: Mutex<HashSet<String>>,
    send: Mutex<Option<Sender<String>>>,
}

fn store() -> &'static Store {
    static S: OnceLock<Store> = OnceLock::new();
    S.get_or_init(|| Store {
        dir: Mutex::new(std::env::var("CURSED_DEV_COSMETICS").ok().map(PathBuf::from)),
        asked: Mutex::new(HashSet::new()),
        ready: Mutex::new(Vec::new()),
        models: Mutex::new(HashMap::new()),
        failed: Mutex::new(HashSet::new()),
        send: Mutex::new(None),
    })
}

/// The folder the files are under (the stand's own wins).
pub fn set_dir(dir: &str) {
    if std::env::var("CURSED_DEV_COSMETICS").is_ok() || dir.is_empty() {
        return;
    }
    *store().dir.lock().unwrap() = Some(PathBuf::from(dir));
    super::remote::start(PathBuf::from(dir));
}

/// The texture beside a model for a material's file name (".png" as VRF names it → ".rtx").
pub fn texture_key(model_key: &str, file: &str) -> String {
    if file.is_empty() {
        return String::new();
    }
    let dir = model_key.rsplit_once('/').map_or("", |(d, _)| d);
    let stem = file.rsplit_once('.').map_or(file, |(s, _)| s);
    if dir.is_empty() {
        format!("{stem}.rtx")
    } else {
        format!("{dir}/{stem}.rtx")
    }
}

/// A model wanted (read once; its textures after it).
pub fn want(key: &str) {
    let s = store();
    if !s.asked.lock().unwrap().insert(key.to_string()) {
        return;
    }
    let mut send = s.send.lock().unwrap();
    if send.is_none() {
        // a few readers (a model waiting for the server does not hold up another's)
        let (tx, rx) = channel::<String>();
        let rx = Arc::new(Mutex::new(rx));
        for n in 0..3 {
            let rx = rx.clone();
            std::thread::Builder::new()
                .name(format!("Cursed cosmetics {n}"))
                .spawn(move || loop {
                    let next = rx.lock().unwrap().recv();
                    match next {
                        Ok(key) => load(&key),
                        Err(_) => break,
                    }
                })
                .ok();
        }
        *send = Some(tx);
    }
    if let Some(tx) = send.as_ref() {
        let _ = tx.send(key.to_string());
    }
}

/// Whether a file is there under the folder (or the server has it, to come when wanted).
pub fn exists(key: &str) -> bool {
    let Some(dir) = store().dir.lock().unwrap().clone() else { return false };
    if key.contains("..") {
        return false;
    }
    if remote::active() {
        return remote::here(key) || remote::known(key);
    }
    dir.join(key).is_file()
}

/// A file under the folder (its key the path from there); one from the server not here yet is asked for, and none
/// for now (asked again later: `coming` says whether it will be).
pub fn read(key: &str) -> Option<Vec<u8>> {
    let dir = store().dir.lock().unwrap().clone()?;
    // (no way out of the folder)
    if key.contains("..") {
        return None;
    }
    if remote::active() && !remote::here(key) {
        remote::want(key);
        return None;
    }
    std::fs::read(dir.join(key)).ok()
}

/// A file under the folder, waited for when it comes from the server (a thread of its own only).
pub fn read_wait(key: &str) -> Option<Vec<u8>> {
    if remote::active() && !key.contains("..") {
        remote::wait(key);
    }
    let dir = store().dir.lock().unwrap().clone()?;
    if key.contains("..") {
        return None;
    }
    std::fs::read(dir.join(key)).ok()
}

/// Whether a file not here is still to come from the server.
pub fn coming(key: &str) -> bool {
    remote::coming(key)
}

/// Whether anything is still coming from the server.
pub fn busy() -> bool {
    remote::busy()
}

fn load(key: &str) {
    let s = store();
    let Some(bytes) = read_wait(key) else {
        eprintln!("[cosmetics] {key}: not there");
        s.failed.lock().unwrap().insert(key.to_string());
        return;
    };
    if key.ends_with(".rcm") {
        match Model::parse(&bytes) {
            Ok(m) => {
                let m = Arc::new(m);
                let mut textures: Vec<String> = Vec::new();
                for mat in &m.materials {
                    for f in [&mat.base, &mat.normal, &mat.packed] {
                        let k = texture_key(key, f);
                        if !k.is_empty() && !textures.contains(&k) {
                            textures.push(k);
                        }
                    }
                }
                s.models.lock().unwrap().insert(key.to_string(), m.clone());
                s.ready.lock().unwrap().push(Loaded::Model(key.to_string(), m));
                // its textures all asked of the server at once (they come side by side), then read in turn
                for t in &textures {
                    remote::want(t);
                }
                for t in textures {
                    if s.asked.lock().unwrap().insert(t.clone()) {
                        load(&t);
                    }
                }
            }
            Err(e) => {
                eprintln!("[cosmetics] {key}: {e}");
                s.failed.lock().unwrap().insert(key.to_string());
            }
        }
    } else if key.ends_with(".rtx") {
        match Image::parse(&bytes) {
            Ok(i) => s.ready.lock().unwrap().push(Loaded::Texture(key.to_string(), i)),
            Err(e) => eprintln!("[cosmetics] {key}: {e}"),
        }
    }
}

/// A texture made here rather than read (a name tag's plate): to the GPU with what was read.
pub fn ready_texture(key: &str, img: Image) {
    let s = store();
    s.asked.lock().unwrap().insert(key.to_string());
    s.ready.lock().unwrap().push(Loaded::Texture(key.to_string(), img));
}

/// What was read since the last asking (for the GPU).
pub fn take_ready() -> Vec<Loaded> {
    std::mem::take(&mut *store().ready.lock().unwrap())
}

/// A model read, for its poses.
pub fn model(key: &str) -> Option<Arc<Model>> {
    store().models.lock().unwrap().get(key).cloned()
}

/// Whether a model could not be read.
pub fn failed(key: &str) -> bool {
    store().failed.lock().unwrap().contains(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn textures_lie_beside_their_model() {
        assert_eq!(texture_key("knives/karambit/model.rcm", "karambit_color_png_1a.png"), "knives/karambit/karambit_color_png_1a.rtx");
        assert_eq!(texture_key("model.rcm", "a.png"), "a.rtx");
        assert_eq!(texture_key("x/model.rcm", ""), "");
    }
}
