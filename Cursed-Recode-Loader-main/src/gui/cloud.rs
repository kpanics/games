//! Account-backed Recode configs. The loader has already established the game's session.
//! All HTTP and staging happen on workers; applying compares the captured local data again.
mod policy;
mod presence;
mod storage;
#[cfg(test)]
mod tests;
mod ui;

use super::{configs, Gui};
use policy::{Baseline, Decision};
use serde_json::{json, Value};
use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, Once, OnceLock};
use std::time::{Duration, Instant};
use storage::Bundle;

const FORMAT: &str = "recode-workspace-1";
static START: Once = Once::new();
static STALE: AtomicBool = AtomicBool::new(true);
static JOBS: Mutex<VecDeque<Command>> = Mutex::new(VecDeque::new());
static REVISION: AtomicU64 = AtomicU64::new(0);
static MODULE_KEYS: OnceLock<std::collections::HashSet<String>> = OnceLock::new();
struct Command {
    job: Job,
    revision: u64,
}
pub(super) fn changed() {
    REVISION.fetch_add(1, Ordering::Relaxed);
}

#[derive(Clone)]
pub(super) struct Entry {
    pub id: i64,
    pub name: String,
    pub own: bool,
    pub version: i64,
}
#[derive(Clone)]
pub(super) struct Version {
    pub version: i64,
    pub at: u64,
    pub bytes: u64,
}
#[derive(Clone)]
pub(super) struct Invitation {
    pub code: String,
    pub max_uses: i64,
    pub uses: i64,
}
fn invitations(v: &Value) -> Vec<Invitation> {
    v.as_array()
        .into_iter()
        .flatten()
        .take(10)
        .filter_map(|c| {
            let code = c["code"].as_str()?;
            if !safe_invitation(code) {
                return None;
            }
            Some(Invitation {
                code: code.into(),
                max_uses: c["max_uses"].as_i64()?,
                uses: c["uses"].as_i64()?,
            })
        })
        .collect()
}
fn safe_invitation(code: &str) -> bool {
    !code.is_empty() && code.len() <= 32 && code.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
}
#[derive(Clone)]
pub(super) struct View {
    pub status: &'static str,
    pub account: String,
    pub active: i64,
    pub name: String,
    pub version: i64,
    pub own: bool,
    pub online: bool,
    pub busy: bool,
    pub conflict: bool,
    pub local_on: usize,
    pub local_files: usize,
    pub remote_on: usize,
    pub remote_files: usize,
    pub entries: Vec<Entry>,
    pub versions: Vec<Version>,
    pub invitations: Vec<Invitation>,
    pub backups: Vec<storage::Backup>,
}
impl Default for View {
    fn default() -> Self {
        Self {
            status: "Sign in through the loader",
            account: String::new(),
            active: 0,
            name: String::new(),
            version: 0,
            own: false,
            online: false,
            busy: false,
            conflict: false,
            entries: Vec::new(),
            versions: Vec::new(),
            invitations: Vec::new(),
            backups: Vec::new(),
            local_on: 0,
            local_files: 0,
            remote_on: 0,
            remote_files: 0,
        }
    }
}
fn state() -> &'static Mutex<View> {
    static STATE: OnceLock<Mutex<View>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(View::default()))
}
fn update(f: impl FnOnce(&mut View)) {
    f(&mut state().lock().unwrap_or_else(|e| e.into_inner()));
}
pub(super) fn view() -> View {
    state().lock().unwrap_or_else(|e| e.into_inner()).clone()
}

#[derive(Clone)]
pub(super) enum Job {
    Refresh,
    History,
    Create(String),
    Activate(i64),
    KeepLocal(i64, i64),
    TakeCloud(i64, i64),
    RestoreVersion(i64, i64, i64),
    RestoreLocal(String),
    MakeInvitation(i64),
    RevokeInvitation(i64, String),
    RedeemInvitation(String),
}
pub(super) fn queue(job: Job) {
    let mut q = JOBS.lock().unwrap_or_else(|e| e.into_inner());
    if q.len() < 8 {
        q.push_back(Command {
            job,
            revision: REVISION.load(Ordering::Relaxed),
        });
    }
    drop(q);
    update(|v| v.busy = true);
}

#[derive(Clone)]
struct Auth {
    api: String,
    token: String,
}
fn auth() -> Option<Auth> {
    // A stand may use a local fixture and its own fake credential. A real game session is
    // never sent to a development override, and releases ignore these variables completely.
    if !crate::license::RELEASE {
        if let Ok(api) = std::env::var("CURSED_DEV_CLOUD_API") {
            let port = api.strip_prefix("http://127.0.0.1:")?.trim_end_matches('/');
            if port.is_empty() || !port.bytes().all(|c| c.is_ascii_digit()) || port.parse::<u16>().ok()? == 0 {
                return None;
            }
            return Some(Auth {
                api: api.trim_end_matches('/').to_string(),
                token: std::env::var("CURSED_DEV_CLOUD_TOKEN").ok()?,
            });
        }
    }
    Some(Auth {
        api: crate::license::site_api().to_string(),
        token: crate::license::site_session()?,
    })
}
fn same_auth(a: &Auth) -> bool {
    auth().is_some_and(|now| now.api == a.api && now.token == a.token)
}

fn call(a: &Auth, method: &str, path: &str, body: Option<&Value>) -> Result<Value, u16> {
    let bearer = format!("Bearer {}", a.token);
    let headers = [("Authorization", bearer.as_str()), ("Content-Type", "application/json")];
    let text = body.map(Value::to_string);
    let mut bytes = Vec::new();
    let head = crate::net::request(
        method,
        &format!("{}{path}", a.api),
        &headers,
        text.as_deref().map(str::as_bytes),
        &mut |_, part| {
            if bytes.len() + part.len() > 1024 * 1024 {
                return false;
            }
            bytes.extend_from_slice(part);
            true
        },
    )
    .map_err(|_| 0u16)?;
    if !(200..300).contains(&head.status) {
        return Err(head.status);
    }
    if bytes.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_slice(&bytes).map_err(|_| 0u16)
}

/// Stable ordering keeps a server profile's temporary selection from appearing as a change
/// to the player's own settings. The studio's current selection is editor focus, not a pose.
fn menu(g: &Gui) -> String {
    let text = g.own_dump();
    let mut vals: BTreeMap<String, String> = text
        .lines()
        .filter_map(|l| l.split_once('='))
        .filter(|(key, _)| *key != "fake-player.Edit double")
        .map(|(k, v)| (k.into(), v.into()))
        .collect();
    // An explicit empty bind clears the other device's bind when this PC is chosen.
    for (id, list) in g.mods.iter().map(|m| (m.id, &m.set)).chain([("menu", &g.client), ("theme", &g.theme)]) {
        for setting in list {
            vals.entry(format!("{id}.{}#binds", setting.label)).or_default();
        }
    }
    vals.insert("theme.#v".into(), "3".into());
    vals.insert("click-pearl.#v".into(), "2".into());
    format!(
        "#parts={}\n{}",
        configs::P_ALL,
        vals.into_iter().map(|(k, v)| format!("{k}={v}\n")).collect::<String>()
    )
}
fn enabled(menu: &str) -> usize {
    let Some(keys) = MODULE_KEYS.get() else {
        return 0;
    };
    menu.lines()
        .filter_map(|l| l.split_once('='))
        .filter(|(k, v)| keys.contains(*k) && *v == "1")
        .count()
}
fn snapshot() -> Result<(std::path::PathBuf, Bundle), u16> {
    let data = super::with(|g| {
        let dir = g.dir.clone().ok_or(603u16)?;
        g.ensure_loaded();
        if !g.loaded {
            return Err(600u16);
        }
        if !g.holds.is_empty() {
            return Err(603);
        }
        MODULE_KEYS.get_or_init(|| {
            g.mods
                .iter()
                .filter(|m| !m.plain && !matches!(m.cat, super::data::Cat::HudEl | super::data::Cat::Hud))
                .map(|m| format!("{}.#on", m.id))
                .collect()
        });
        g.profile_keep();
        Ok((dir, menu(g), g.own_config_name(), REVISION.load(Ordering::Relaxed)))
    })?;
    let pack = Bundle::capture(&data.0, data.1, data.2).map_err(|_| 600u16)?;
    if super::with(|_| REVISION.load(Ordering::Relaxed)) != data.3 {
        return Err(603);
    }
    Ok((data.0, pack))
}

fn apply(dir: &std::path::Path, before: &Bundle, after: &Bundle, backup: &Bundle, explicit: bool) -> Result<Bundle, u16> {
    let ready = super::with(|g| explicit || (g.phase == super::Phase::Closed && !g.hud.editing));
    if !ready {
        return Err(603);
    }
    let staged = storage::prepare(dir, backup, after).map_err(|_| 600u16)?;
    super::with(|g| {
        if !g.holds.is_empty() {
            return Err(603);
        }
        g.profile_keep();
        let current = Bundle::capture(dir, menu(g), g.own_config_name()).map_err(|_| 600u16)?;
        if &current != before {
            return Err(603);
        }
        if staged.commit().is_err() {
            g.cloud_recovery_blocked = true;
            g.loaded = false;
            g.toast(super::i18n::tr("Cloud files need recovery").to_string());
            return Err(600);
        }
        // profile_keep above captured its changes. Do not call profile_leave after replacing
        // the directory: it would write the old profile into the newly downloaded collection.
        g.profile = None;
        g.interface_profile = None;
        g.cfg_interface_context = None;
        g.cfg_host.clear();
        let (mut vals, _, _) = configs::parse(&after.menu);
        vals.remove("fake-player.Edit double");
        g.apply_vals(&vals, configs::P_ALL);
        g.loaded_cfg = after.active.clone();
        g.cfg_scan();
        // The staged menu is already written atomically. A normal save here would rewrite
        // that good file in place and also discard settings belonging to newer clients.
        g.toast(super::i18n::tr("Settings loaded").to_string());
        Bundle::capture(dir, menu(g), g.own_config_name()).map_err(|_| 600u16)
    })
}

pub(super) fn recover(dir: &std::path::Path) -> bool {
    if storage::recover(dir).is_err() {
        update(|v| v.status = "Cloud files need recovery");
        return false;
    }
    true
}

fn stream(a: Auth) {
    if cfg!(test) {
        return;
    }
    let _ = std::thread::Builder::new().name("recode-presence".into()).spawn(move || {
        while same_auth(&a) {
            let bearer = format!("Bearer {}", a.token);
            let mut pending = Vec::new();
            let mut opened = false;
            let _ = crate::net::request(
                "GET",
                &format!("{}/api/live?kind=game", a.api),
                &[("Authorization", bearer.as_str())],
                None,
                &mut |head, part| {
                    if head.status != 200 || !same_auth(&a) {
                        return false;
                    }
                    if !opened {
                        opened = true;
                        presence::OPENED.fetch_add(1, Ordering::Relaxed);
                    }
                    pending.extend_from_slice(part);
                    while let Some(end) = pending.iter().position(|b| *b == b'\n') {
                        let line: Vec<_> = pending.drain(..=end).collect();
                        if let Some(data) = line.strip_prefix(b"data:") {
                            if let Ok(v) = serde_json::from_slice::<Value>(data) {
                                if matches!(v["t"].as_str(), Some("config" | "config_active" | "config_gone")) {
                                    STALE.store(true, Ordering::Relaxed);
                                }
                            }
                        }
                    }
                    if pending.len() > 16 * 1024 {
                        pending.clear();
                    }
                    true
                },
            );
            std::thread::sleep(Duration::from_secs(5));
        }
    });
}

#[derive(Clone)]
struct Remote {
    id: i64,
    name: String,
    version: i64,
    own: bool,
    code: String,
    pack: Bundle,
    invitations: Vec<Invitation>,
}
fn read_remote(a: &Auth, id: i64) -> Result<Remote, u16> {
    let v = call(a, "GET", &format!("/api/configs/{id}"), None)?;
    if v["id"].as_i64() != Some(id) || v["client"].as_str() != Some("recode") || v["data"]["format"].as_str() != Some(FORMAT) {
        return Err(601);
    }
    let code = v["data"]["archive"].as_str().ok_or(601u16)?.to_string();
    let pack = Bundle::decode(&code).map_err(|_| 601u16)?;
    Ok(Remote {
        id,
        name: v["name"].as_str().unwrap_or("Recode").to_string(),
        version: v["version"].as_i64().filter(|n| *n > 0).ok_or(601u16)?,
        own: v["own"].as_bool().unwrap_or(false),
        code,
        pack,
        invitations: invitations(&v["codes"]),
    })
}

#[derive(Default)]
struct Runtime {
    uid: Option<i64>,
    baseline: Baseline,
    remote: Option<Remote>,
    last_fetch: Option<Instant>,
    pending: Option<(String, Instant)>,
}
impl Runtime {
    fn merge_menu(&self, local: &str, remote: &str) -> String {
        let known = Bundle::decode(&self.baseline.local).map_or(String::new(), |b| b.menu);
        policy::merge_menu(local, remote, &known)
    }
    fn apply(&self, dir: &std::path::Path, before: &Bundle, after: &Bundle, explicit: bool) -> Result<Bundle, u16> {
        let prior = Bundle::decode(&self.baseline.remote).map_or(String::new(), |b| b.menu);
        let disk = storage::read_metadata(dir, "menu.txt")
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
            .unwrap_or_default();
        let all = policy::merge_menu(&disk, &prior, "");
        let mut backup = before.clone();
        backup.menu = self.merge_menu(&before.menu, &all);
        apply(dir, before, after, &backup, explicit)
    }
    fn marker(&self) -> String {
        format!("cloud-state-{}.json", self.uid.unwrap_or(0))
    }
    fn persist(&self, dir: &std::path::Path) -> Result<(), u16> {
        storage::metadata(dir, &self.marker(), &serde_json::to_vec(&self.baseline).map_err(|_| 600u16)?).map_err(|_| 600u16)
    }
    fn acknowledge(&mut self, dir: &std::path::Path, r: Remote, local: &Bundle) -> Result<(), u16> {
        self.baseline = Baseline {
            id: r.id,
            version: r.version,
            remote: r.code.clone(),
            local: local.encode().map_err(|_| 413u16)?,
        };
        self.remote = Some(r);
        self.pending = None;
        self.persist(dir)?;
        self.show("Cloud is up to date", false);
        self.local_history(dir);
        Ok(())
    }
    fn show(&self, status: &'static str, conflict: bool) {
        update(|v| {
            v.status = status;
            v.conflict = conflict;
            if let Some(r) = &self.remote {
                if v.active != r.id {
                    v.versions.clear();
                }
                v.active = r.id;
                v.name = r.name.clone();
                v.version = r.version;
                v.own = r.own;
                v.remote_files = r.pack.saved.len();
                v.remote_on = enabled(&r.pack.menu);
                v.invitations = r.invitations.clone();
            } else {
                v.active = 0;
                v.name.clear();
                v.version = 0;
                v.own = false;
                v.versions.clear();
                v.remote_on = 0;
                v.remote_files = 0;
                v.invitations.clear();
            }
        });
    }
    fn local_history(&self, dir: &std::path::Path) {
        if let Ok(copies) = storage::backups(dir) {
            update(|v| v.backups = copies);
        }
    }
    fn fetch(&mut self, a: &Auth, dir: &std::path::Path, local: &Bundle, auto_create: bool) -> Result<(), u16> {
        let list = call(a, "GET", "/api/configs?client=recode", None)?;
        if list["versioned_updates"].as_bool() != Some(true) || list["versioned_history"].as_bool() != Some(true) {
            self.last_fetch = None;
            self.remote = None;
            self.show("Cloud server update is required", false);
            update(|v| v.online = false);
            return Err(602);
        }
        let entries: Vec<Entry> = ["mine", "shared"]
            .into_iter()
            .flat_map(|k| list[k].as_array().into_iter().flatten())
            .take(200)
            .filter_map(|v| {
                Some(Entry {
                    id: v["id"].as_i64()?,
                    name: v["name"].as_str()?.into(),
                    own: v["own"].as_bool().unwrap_or(false),
                    version: v["version"].as_i64().unwrap_or(1),
                })
            })
            .collect();
        update(|v| {
            v.entries = entries.clone();
            v.online = true;
        });
        let id = list["active"].as_i64().unwrap_or(0);
        if id == 0 {
            self.remote = None;
            // A first account starts from what is on this PC. A deleted config is not silently
            // recreated, nor does an account with existing inactive configs acquire duplicates.
            if auto_create && self.baseline.id == 0 && entries.is_empty() {
                self.create(a, dir, local, "Recode")?;
            } else {
                self.show("Choose a cloud config", false);
            }
        } else {
            let r = match read_remote(a, id) {
                Ok(r) => r,
                Err(e @ (404 | 601)) => {
                    // A missing or newer-format active config must not trap the user:
                    // the list, another selection and a fresh own copy remain usable.
                    self.remote = None;
                    self.show(
                        if e == 601 {
                            "Cloud config has another format"
                        } else {
                            "Choose a cloud config"
                        },
                        false,
                    );
                    self.last_fetch = Some(Instant::now());
                    return Ok(());
                }
                Err(e) => return Err(e),
            };
            if self.baseline.id != id {
                let follow = self.baseline.id != 0 && local.encode().ok().as_deref() == Some(self.baseline.local.as_str());
                if follow {
                    match self.apply(dir, local, &r.pack, false) {
                        Ok(applied) => self.acknowledge(dir, r, &applied)?,
                        Err(603) => {
                            self.remote = Some(r);
                            self.show("Cloud changes are waiting", false);
                        }
                        Err(e) => return Err(e),
                    }
                } else {
                    self.baseline = Baseline::default();
                    self.remote = Some(r);
                }
            } else {
                self.remote = Some(r);
            }
        }
        self.last_fetch = Some(Instant::now());
        let shown = view();
        self.show(shown.status, shown.conflict);
        Ok(())
    }
    fn create(&mut self, a: &Auth, dir: &std::path::Path, local: &Bundle, name: &str) -> Result<(), u16> {
        let name: String = name.chars().filter(|c| !c.is_control()).take(40).collect();
        let mut uploaded = local.clone();
        if let Some(r) = &self.remote {
            uploaded.menu = self.merge_menu(&local.menu, &r.pack.menu);
        }
        let code = uploaded.encode().map_err(|_| 413u16)?;
        let v = call(
            a,
            "POST",
            "/api/configs",
            Some(&json!({"name":if name.is_empty() {"Recode"} else {&name},
            "client":"recode","activate":true,"data":{"format":FORMAT,"archive":code}})),
        )?;
        let id = v["config"]["id"].as_i64().filter(|n| *n > 0).ok_or(601u16)?;
        let r = Remote {
            id,
            name: v["config"]["name"].as_str().unwrap_or(&name).into(),
            version: v["config"]["version"].as_i64().unwrap_or(1),
            own: true,
            code,
            pack: uploaded,
            invitations: Vec::new(),
        };
        self.acknowledge(dir, r, local)?;
        self.last_fetch = None;
        Ok(())
    }
    fn upload(&mut self, a: &Auth, dir: &std::path::Path, local: &Bundle, expected: i64) -> Result<(), u16> {
        let r = self.remote.clone().ok_or(404u16)?;
        if !r.own {
            return Err(403);
        }
        let mut merged = local.clone();
        merged.menu = self.merge_menu(&local.menu, &r.pack.menu);
        let code = merged.encode().map_err(|_| 413u16)?;
        let v = call(
            a,
            "PUT",
            &format!("/api/configs/{}", r.id),
            Some(&json!({"version":expected,"data":{"format":FORMAT,"archive":code}})),
        )?;
        let new = Remote {
            version: v["version"].as_i64().ok_or(601u16)?,
            code,
            pack: merged,
            ..r
        };
        self.acknowledge(dir, new, local)
    }
    fn history(&self, a: &Auth, dir: &std::path::Path) -> Result<(), u16> {
        self.local_history(dir);
        let Some(r) = &self.remote else { return Ok(()) };
        if !r.own {
            update(|v| v.versions.clear());
            return Ok(());
        }
        let v = call(a, "GET", &format!("/api/configs/{}/history", r.id), None)?;
        let versions = v["versions"]
            .as_array()
            .into_iter()
            .flatten()
            .take(30)
            .filter_map(|v| {
                Some(Version {
                    version: v["version"].as_i64()?,
                    at: v["at"].as_u64().unwrap_or(0),
                    bytes: v["bytes"].as_u64().unwrap_or(0),
                })
            })
            .collect();
        update(|v| v.versions = versions);
        Ok(())
    }
    fn cycle(&mut self, a: &Auth, job: Option<Job>) -> Result<(), u16> {
        let (dir, mut local) = snapshot()?;
        update(|v| {
            v.local_files = local.saved.len();
            v.local_on = enabled(&local.menu);
        });
        if self.uid.is_none() {
            let me = call(a, "GET", "/api/me", None)?;
            self.uid = Some(me["uid"].as_i64().filter(|n| *n >= 0).ok_or(401u16)?);
            self.baseline = storage::read_metadata(&dir, &self.marker())
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or_default();
            update(|v| v.account = me["login"].as_str().unwrap_or("").into());
            self.local_history(&dir);
        }
        if self.last_fetch.is_none_or(|t| t.elapsed() >= Duration::from_secs(10))
            || STALE.swap(false, Ordering::Relaxed)
            || matches!(job, Some(Job::Refresh | Job::Activate(_) | Job::Create(_)))
        {
            self.fetch(a, &dir, &local, !matches!(job, Some(Job::Create(_))))?;
            // Fetch can follow another device's active config. Continue from the settings
            // just applied, never from the pre-fetch snapshot.
            local = snapshot()?.1;
        }
        if self.last_fetch.is_none() {
            return Ok(());
        }
        match job {
            Some(Job::MakeInvitation(id)) => {
                let r = self.remote.as_ref().filter(|r| r.id == id && r.own).ok_or(403u16)?;
                call(a, "POST", &format!("/api/configs/{}/codes", r.id), Some(&json!({"max_uses":1})))?;
                self.last_fetch = None;
                self.remote = Some(read_remote(a, id)?);
                let current = view();
                self.show("Invitation created", current.conflict);
                return Ok(());
            }
            Some(Job::RevokeInvitation(id, code)) => {
                let r = self.remote.as_ref().filter(|r| r.id == id && r.own).ok_or(403u16)?;
                if !safe_invitation(&code) {
                    return Err(604);
                }
                call(a, "DELETE", &format!("/api/configs/{}/codes/{code}", r.id), None)?;
                self.last_fetch = None;
                self.remote = Some(read_remote(a, id)?);
                let current = view();
                self.show("Invitation revoked", current.conflict);
                return Ok(());
            }
            Some(Job::RedeemInvitation(code)) => {
                if !safe_invitation(&code) {
                    return Err(604);
                }
                let v = call(a, "POST", "/api/configs/redeem", Some(&json!({"code":code,"client":"recode"}))).map_err(|e| {
                    if matches!(e, 400 | 404 | 410) {
                        604
                    } else {
                        e
                    }
                })?;
                let id = v["config"]["id"].as_i64().filter(|n| *n > 0).ok_or(601u16)?;
                self.last_fetch = None;
                let r = read_remote(a, id)?;
                let after = self.apply(&dir, &local, &r.pack, true)?;
                self.acknowledge(&dir, r, &after)?;
                return Ok(());
            }
            Some(Job::Create(name)) => return self.create(a, &dir, &local, &name),
            Some(Job::Activate(id)) => {
                let r = read_remote(a, id)?;
                call(a, "POST", &format!("/api/configs/{id}/active"), None)?;
                self.last_fetch = None;
                let after = self.apply(&dir, &local, &r.pack, true)?;
                self.acknowledge(&dir, r, &after)?;
                self.last_fetch = None;
                return Ok(());
            }
            Some(Job::History) => return self.history(a, &dir),
            Some(Job::RestoreVersion(id, version, expected)) => {
                let r = self.remote.as_ref().filter(|r| r.id == id && r.own).ok_or(403u16)?;
                if r.version != expected {
                    return Err(409);
                }
                call(a, "POST", &format!("/api/configs/{}/history/{version}?version={expected}", r.id), None)?;
                let r = read_remote(a, id)?;
                let after = self.apply(&dir, &local, &r.pack, true)?;
                return self.acknowledge(&dir, r, &after);
            }
            Some(Job::KeepLocal(id, version)) => {
                let r = self.remote.as_ref().filter(|r| r.id == id && r.version == version).ok_or(409u16)?;
                if !r.own {
                    return self.create(a, &dir, &local, "Recode");
                }
                return self.upload(a, &dir, &local, version);
            }
            Some(Job::TakeCloud(id, version)) => {
                let r = read_remote(a, id)?;
                if r.version != version {
                    self.remote = Some(r);
                    return Err(409);
                }
                let after = self.apply(&dir, &local, &r.pack, true)?;
                return self.acknowledge(&dir, r, &after);
            }
            Some(Job::RestoreLocal(_)) => return Ok(()), // handled even without a site session below
            Some(Job::Refresh) | None => {}
        }
        let Some(r) = self.remote.clone() else { return Ok(()) };
        let code = local.encode().map_err(|_| 413u16)?;
        match policy::decide(&code, &r.code, &self.baseline, r.own) {
            Decision::Synced => {
                if code == r.code && (self.baseline.id != r.id || self.baseline.remote != r.code) {
                    self.acknowledge(&dir, r, &local)?;
                } else {
                    self.show("Cloud is up to date", false);
                }
            }
            Decision::Upload => {
                if self.pending.as_ref().is_none_or(|(before, _)| *before != code) {
                    self.pending = Some((code, Instant::now()));
                }
                self.show("Saving to the cloud", false);
                if self.pending.as_ref().is_some_and(|(_, t)| t.elapsed() >= Duration::from_secs(3)) {
                    self.upload(a, &dir, &local, r.version)?;
                }
            }
            Decision::Download => {
                let applied = self.apply(&dir, &local, &r.pack, false)?;
                self.acknowledge(&dir, r, &applied)?;
            }
            Decision::Conflict => {
                self.pending = None;
                self.show("Choose which settings to keep", true);
            }
            Decision::SharedLocal => {
                self.pending = None;
                self.show("Shared config: local changes stay on this PC", false);
            }
        }
        Ok(())
    }
}

fn failed(code: u16) {
    let status = match code {
        401 => "Sign in through the loader",
        403 => "This cloud config is read only",
        409 => {
            STALE.store(true, Ordering::Relaxed);
            "The cloud changed again; review both versions"
        }
        413 => "Cloud workspace is too large",
        600 => "Cloud files need recovery",
        601 => "Cloud config has another format",
        602 => "Cloud server update is required",
        603 => "Cloud changes are waiting",
        604 => "This cloud code is unavailable or belongs to another client",
        _ => "Cloud unavailable; settings stay on this PC",
    };
    update(|v| {
        v.status = status;
        v.busy = false;
        if code == 409 {
            v.conflict = true;
        }
        if code == 0 || code == 401 || code >= 500 && code < 600 {
            v.online = false;
        }
    });
}

fn worker() {
    let mut previous: Option<Auth> = None;
    let mut runtime = Runtime::default();
    let mut retry_after: Option<Instant> = None;
    loop {
        std::thread::sleep(Duration::from_secs(1));
        let command = JOBS.lock().unwrap_or_else(|e| e.into_inner()).pop_front();
        if command.as_ref().is_some_and(|c| {
            matches!(
                c.job,
                Job::Activate(_) | Job::TakeCloud(..) | Job::RestoreVersion(..) | Job::RestoreLocal(_) | Job::RedeemInvitation(_)
            ) && c.revision != REVISION.load(Ordering::Relaxed)
        }) {
            failed(603);
            continue;
        }
        let mut job = command.map(|c| c.job);
        if matches!(job, Some(Job::History | Job::Refresh)) {
            if let Ok((dir, _)) = snapshot() {
                runtime.local_history(&dir);
            }
        }
        if let Some(Job::RestoreLocal(id)) = &job {
            let result = snapshot().and_then(|(dir, before)| {
                let old = storage::read_backup(&dir, id).map_err(|_| 600u16)?;
                runtime.apply(&dir, &before, &old, true)?;
                runtime.local_history(&dir);
                update(|v| {
                    v.status = "Local copy restored";
                    v.busy = false;
                });
                Ok(())
            });
            if let Err(e) = result {
                failed(e);
            }
            continue;
        }
        let Some(a) = auth() else {
            if previous.take().is_some() {
                runtime = Runtime::default();
                update(|v| *v = View::default());
            }
            update(|v| {
                v.status = "Sign in through the loader";
                v.busy = false;
            });
            continue;
        };
        let changed = previous.as_ref().is_none_or(|p| p.api != a.api || p.token != a.token);
        if changed {
            if previous.is_some() {
                job = None;
                JOBS.lock().unwrap_or_else(|e| e.into_inner()).clear();
            }
            previous = Some(a.clone());
            runtime = Runtime::default();
            retry_after = None;
            update(|v| *v = View::default());
            STALE.store(true, Ordering::Relaxed);
        }
        if job.is_none() && retry_after.is_some_and(|at| Instant::now() < at) {
            continue;
        }
        match runtime.cycle(&a, job) {
            Ok(()) => retry_after = None,
            Err(e) => {
                failed(e);
                if e == 0 || e >= 500 && e != 603 {
                    retry_after = Some(Instant::now() + Duration::from_secs(10));
                }
            }
        }
        update(|v| v.busy = false);
    }
}

pub(super) fn start() {
    if cfg!(test) || START.is_completed() {
        return;
    }
    START.call_once(|| {
        let _ = std::thread::Builder::new().name("recode-cloud".into()).spawn(worker);
        let _ = std::thread::Builder::new().name("recode-marks".into()).spawn(presence::run);
    });
}
