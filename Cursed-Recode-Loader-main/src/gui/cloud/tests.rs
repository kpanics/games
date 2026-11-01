//! Exercise the real WinHTTP client, GUI settings and transactional files against a loopback
//! service. No production account, token or game directory is involved.
use super::*;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
struct Config {
    id: i64,
    name: String,
    code: String,
    version: i64,
    own: bool,
    history: Vec<(i64, String)>,
    invitations: Vec<Value>,
}
impl Config {
    fn json(&self) -> Value {
        let mut v = json!({"id":self.id,"name":self.name,"client":"recode","version":self.version,"own":self.own,
            "data":{"format":FORMAT,"archive":self.code}});
        if self.own {
            v["codes"] = json!(self.invitations);
        }
        v
    }
    fn change(&mut self, code: String) {
        self.history.push((self.version, self.code.clone()));
        self.version += 1;
        self.code = code;
    }
}
struct Database {
    configs: Vec<Config>,
    active: i64,
    puts: usize,
    capable: bool,
    next_invitation: u64,
    redeemed: std::collections::HashSet<i64>,
    presence_posts: Vec<Value>,
    players: Vec<Value>,
}
impl Database {
    fn add(&mut self, pack: &Bundle, own: bool) -> i64 {
        let id = self.configs.len() as i64 + 1;
        self.configs.push(Config {
            id,
            name: format!("Config {id}"),
            code: pack.encode().unwrap(),
            version: 1,
            own,
            history: vec![],
            invitations: vec![],
        });
        id
    }
    fn get(&mut self, id: i64) -> &mut Config {
        self.configs.iter_mut().find(|c| c.id == id).unwrap()
    }
    fn request(&mut self, method: &str, path: &str, body: Value) -> (u16, Value) {
        match (method, path) {
            ("GET", "/api/me") => return (200, json!({"uid":55,"login":"fixture"})),
            ("POST", "/api/me/presence") => {
                self.presence_posts.push(body);
                return (200, json!({"ok":true}));
            }
            ("GET", path) if path.starts_with("/api/game/ours?") => return (200, json!({"players":self.players})),
            ("GET", "/api/configs?client=recode") => {
                return (
                    200,
                    json!({"active":self.active,"versioned_updates":self.capable,"versioned_history":self.capable,
                "mine":self.configs.iter().filter(|c|c.own).map(Config::json).collect::<Vec<_>>(),
                "shared":self.configs.iter().filter(|c|!c.own).map(Config::json).collect::<Vec<_>>()}),
                )
            }
            ("POST", "/api/configs") => {
                let pack = Bundle::decode(body["data"]["archive"].as_str().unwrap()).unwrap();
                let id = self.add(&pack, true);
                self.active = id;
                return (200, json!({"config":self.get(id).json()}));
            }
            ("POST", "/api/configs/redeem") => {
                assert_eq!(body["client"], "recode");
                let code = body["code"].as_str().unwrap();
                let found = self.configs.iter().find(|c| c.invitations.iter().any(|i| i["code"] == code)).map(|c| c.id);
                let Some(id) = found else { return (404, Value::Null) };
                let already = self.redeemed.contains(&id);
                let c = self.get(id);
                if c.own {
                    return (400, Value::Null);
                }
                let invitation = c.invitations.iter_mut().find(|i| i["code"] == code).unwrap();
                if !already {
                    let uses = invitation["uses"].as_i64().unwrap();
                    if uses >= invitation["max_uses"].as_i64().unwrap() {
                        return (410, Value::Null);
                    }
                    invitation["uses"] = json!(uses + 1);
                }
                let summary = c.json();
                self.redeemed.insert(id);
                self.active = id;
                return (200, json!({"config":summary}));
            }
            _ => {}
        }
        let (route, query) = path.split_once('?').unwrap_or((path, ""));
        let parts: Vec<_> = route.trim_matches('/').split('/').collect();
        if parts.len() < 3 || parts[0] != "api" || parts[1] != "configs" {
            return (404, Value::Null);
        }
        let Ok(id) = parts[2].parse::<i64>() else { return (404, Value::Null) };
        if !self.configs.iter().any(|c| c.id == id) {
            return (404, Value::Null);
        }
        if method == "GET" && parts.len() == 3 {
            return (200, self.get(id).json());
        }
        if method == "POST" && parts.get(3) == Some(&"active") {
            self.active = id;
            return (200, json!({"ok":true}));
        }
        if method == "PUT" && parts.len() == 3 {
            self.puts += 1;
            let c = self.get(id);
            if !c.own {
                return (403, Value::Null);
            }
            if body["version"].as_i64() != Some(c.version) {
                return (409, json!({"error":"version"}));
            }
            c.change(body["data"]["archive"].as_str().unwrap().to_string());
            return (200, json!({"version":c.version}));
        }
        if parts.get(3) == Some(&"codes") {
            self.next_invitation += 1;
            let next = self.next_invitation;
            let c = self.get(id);
            if !c.own {
                return (403, Value::Null);
            }
            if method == "POST" && parts.len() == 4 {
                if c.invitations.len() >= 10 {
                    return (400, Value::Null);
                }
                let code = format!("CODE-{next:04}");
                c.invitations.push(json!({"code":code,"max_uses":body["max_uses"],"uses":0}));
                return (200, json!({"code":code}));
            }
            if method == "DELETE" && parts.len() == 5 {
                let old = c.invitations.len();
                c.invitations.retain(|i| i["code"] != parts[4]);
                return (if old == c.invitations.len() { 404 } else { 200 }, json!({}));
            }
        }
        if parts.get(3) == Some(&"history") {
            let c = self.get(id);
            if !c.own {
                return (403, Value::Null);
            }
            if method == "GET" {
                return (
                    200,
                    json!({"versions":c.history.iter().rev().map(|(version,code)|json!({
                    "version":version,"at":1792870000u64,"bytes":code.len()})).collect::<Vec<_>>()}),
                );
            }
            if method == "POST" && parts.len() == 5 {
                if query.strip_prefix("version=").and_then(|v| v.parse::<i64>().ok()) != Some(c.version) {
                    return (409, json!({"error":"version"}));
                }
                let version = parts[4].parse::<i64>().unwrap();
                let old = c.history.iter().find(|(v, _)| *v == version).map(|(_, code)| code.clone());
                if let Some(code) = old {
                    c.change(code);
                    return (200, json!({"ok":true}));
                }
            }
        }
        (404, Value::Null)
    }
}
struct Fixture {
    auth: Auth,
    db: Arc<Mutex<Database>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    root: PathBuf,
    old_gui: Option<Gui>,
}
impl Fixture {
    fn new() -> Self {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/cloud-network-tests");
        std::fs::create_dir_all(&base).unwrap();
        let root = base.canonicalize().unwrap().join(format!("case-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let mut gui = Gui::new();
        gui.dir = Some(root.clone());
        gui.phase = super::super::Phase::Closed;
        let old_gui = super::super::GUI.lock().unwrap().replace(gui);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let db = Arc::new(Mutex::new(Database {
            configs: vec![],
            active: 0,
            puts: 0,
            capable: true,
            next_invitation: 0,
            redeemed: std::collections::HashSet::new(),
            presence_posts: vec![],
            players: vec![],
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let (data, done) = (db.clone(), stop.clone());
        let thread = std::thread::spawn(move || {
            while !done.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((socket, _)) => Self::serve(socket, &data),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(5)),
                    Err(e) => panic!("{e}"),
                }
            }
        });
        update(|v| *v = View::default());
        STALE.store(true, Ordering::Relaxed);
        Self {
            auth: Auth {
                api: format!("http://{address}"),
                token: "fixture-only".into(),
            },
            db,
            stop,
            thread: Some(thread),
            root,
            old_gui,
        }
    }
    fn serve(mut s: TcpStream, db: &Mutex<Database>) {
        s.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let mut bytes = Vec::new();
        let mut chunk = [0u8; 8192];
        let (head, body) = loop {
            let n = match s.read(&mut chunk) {
                Ok(n) => n,
                Err(_) => return,
            };
            if n == 0 {
                return;
            }
            bytes.extend_from_slice(&chunk[..n]);
            assert!(bytes.len() <= 1024 * 1024);
            if let Some(at) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let head = String::from_utf8(bytes[..at].to_vec()).unwrap();
                let len = head
                    .lines()
                    .find_map(|l| {
                        l.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|v| v.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                if bytes.len() >= at + 4 + len {
                    break (head, bytes[at + 4..at + 4 + len].to_vec());
                }
            }
        };
        let mut words = head.lines().next().unwrap().split_whitespace();
        let (method, path) = (words.next().unwrap(), words.next().unwrap());
        assert!(head.lines().any(|l| l.eq_ignore_ascii_case("Authorization: Bearer fixture-only")));
        let body = if body.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&body).unwrap()
        };
        let (status, json) = db.lock().unwrap().request(method, path, body);
        let text = json.to_string();
        let _ = write!(
            s,
            "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
            text.len()
        );
    }
    fn db<R>(&self, f: impl FnOnce(&mut Database) -> R) -> R {
        f(&mut self.db.lock().unwrap())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let joined = self.thread.take().unwrap().join();
        if !std::thread::panicking() {
            joined.unwrap();
        }
        *super::super::GUI.lock().unwrap_or_else(|e| e.into_inner()) = self.old_gui.take();
        update(|v| *v = View::default());
        let checked = self.root.canonicalize().unwrap();
        let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/cloud-network-tests")
            .canonicalize()
            .unwrap();
        assert!(checked.starts_with(parent) && checked.file_name().unwrap().to_string_lossy().starts_with("case-"));
        std::fs::remove_dir_all(checked).unwrap();
    }
}
fn flip(pack: &Bundle, key: &str) -> Bundle {
    let mut out = pack.clone();
    let mut found = false;
    out.menu = pack
        .menu
        .lines()
        .map(|line| {
            if let Some((k, v)) = line.split_once('=') {
                if k == key {
                    found = true;
                    return format!("{k}={}\n", if v == "1" { "0" } else { "1" });
                }
            }
            format!("{line}\n")
        })
        .collect();
    assert!(found, "missing {key}");
    out
}
fn local_flip(index: usize) {
    super::super::with(|g| {
        g.mods[index].on = !g.mods[index].on;
        g.save();
    });
}
fn remote_change(f: &Fixture, id: i64, pack: Bundle) {
    f.db(|d| d.get(id).change(pack.encode().unwrap()));
    STALE.store(true, Ordering::Relaxed);
}

fn isolated(name: &str) -> bool {
    // Applying configs touches the process-wide GUI and language state. Other unit tests
    // deliberately reset that state; exercise each complete workflow in its own process.
    const FLAG: &str = "CURSED_TEST_CLOUD_ISOLATED";
    if std::env::var(FLAG).ok().as_deref() == Some("1") {
        return false;
    }
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture", "--test-threads=1"])
        .env(FLAG, "1")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    true
}

#[test]
fn presence_does_not_depend_on_cloud_and_reconnects_without_stale_badges() {
    if isolated(concat!(module_path!(), "::presence_does_not_depend_on_cloud_and_reconnects_without_stale_badges")) { return; }
    use crate::client_marks::{self, Client};
    let f = Fixture::new();
    std::env::set_var("CURSED_DEV_CLOUD_API", &f.auth.api);
    std::env::set_var("CURSED_DEV_CLOUD_TOKEN", &f.auth.token);
    f.db(|d| { d.capable = false; d.players = vec![json!({"mc":"Alex","client":"cursed"}), json!({"mc":"Steve","client":"recode"})]; });
    // No cloud snapshot, UID, config fetch or files are needed to announce the game.
    client_marks::context("MC.EXAMPLE.ORG:25565", "RealPlayer");
    let mut p = presence::Presence::default();
    p.tick(&f.auth);
    assert_eq!(client_marks::client_of("alex"), Some(Client::Cursed));
    assert_eq!(client_marks::client_of("STEVE"), Some(Client::Recode));
    assert_eq!(f.db(|d| d.presence_posts[0].clone()), json!({"server":"mc.example.org","mc":"RealPlayer","client":"recode","build":option_env!("CURSED_BUILD").and_then(|v|v.parse::<i64>().ok()),"listening":null}));
    p.tick(&f.auth);
    assert_eq!(f.db(|d| d.presence_posts.len()), 1);
    presence::OPENED.fetch_add(1, Ordering::Relaxed);
    f.db(|d| d.players.clear());
    p.tick(&f.auth); // reconnect forces a new announcement before the normal ten-second poll
    assert_eq!(f.db(|d| d.presence_posts.len()), 2);
    assert_eq!(client_marks::client_of("Alex"), None);
    client_marks::context("", "RealPlayer");
    p.tick(&f.auth);
    assert!(f.db(|d| d.presence_posts.last().unwrap()["server"].is_null()));
    assert!(f.db(|d| d.configs.is_empty() && d.puts == 0));
    std::env::remove_var("CURSED_DEV_CLOUD_API");
    std::env::remove_var("CURSED_DEV_CLOUD_TOKEN");
    client_marks::clear();
}

#[test]
fn held_binds_are_never_uploaded_as_permanent_settings() {
    if isolated(concat!(module_path!(), "::held_binds_are_never_uploaded_as_permanent_settings")) {
        return;
    }
    let f = Fixture::new();
    let i = super::super::with(|g| {
        g.ensure_loaded();
        let i = g.mods.iter().position(|m| m.id == "auto-sprint").unwrap();
        g.mods[i].binds.push(super::super::binds::Bind {
            keys: vec!["K".into()],
            hold: true,
            value: None,
        });
        g.save();
        i
    });
    let mut rt = Runtime::default();
    rt.cycle(&f.auth, None).unwrap();
    let id = rt.remote.as_ref().unwrap().id;
    super::super::with(|g| {
        assert!(!g.mods[i].on);
        assert!(g.world_key("K", true));
        assert!(g.mods[i].on);
    });
    assert_eq!(rt.cycle(&f.auth, None), Err(603));
    assert_eq!(f.db(|d| (d.get(id).version, d.puts)), (1, 0));
    super::super::with(|g| {
        g.world_key("K", false);
        assert!(!g.mods[i].on);
    });
    rt.cycle(&f.auth, None).unwrap();
    assert!(!view().conflict);
    assert!(rt.pending.is_none());
    assert_eq!(f.db(|d| (d.get(id).version, d.puts)), (1, 0));
}

#[test]
fn cloud_replaces_active_interface_without_writing_the_old_look_back() {
    if isolated(concat!(module_path!(), "::cloud_replaces_active_interface_without_writing_the_old_look_back")) {
        return;
    }
    use super::super::{data::Kind, theme};
    fn accent(g: &mut Gui, rgb: u32) {
        if let Kind::Colour(c) = &mut g.theme.iter_mut().find(|s| s.label == "Accent").unwrap().kind {
            c.rgb = rgb;
            c.auto = false;
        }
    }
    let _fixture = Fixture::new();
    super::super::with(|g| {
        g.ensure_loaded();
        let i = g.mod_index("server-adaptation");
        g.mods[i].on = true;
        for setting in &mut g.mods[i].set {
            match (setting.label, &mut setting.kind) {
                ("Mode", Kind::Choice { v, .. }) => *v = 0,
                ("By the server", Kind::Toggle(v)) => *v = false,
                _ => {}
            }
        }
        accent(g, 0xff6600);
        assert!(g.cfg_write("Look", configs::P_ALL, None));
        g.cfg_interface(0, 0);
        accent(g, 0x123456);
        g.hud.facts.server = "cloud.test".into();
        g.profile_follow();
        assert_eq!(theme::col(&g.theme, "Accent").rgb, 0xff6600);
        accent(g, 0xff7700);
        g.save();
    });
    let (dir, before) = snapshot().unwrap();
    assert!(before.menu.contains("theme.Accent=123456/"));
    assert!(before.saved[0].text.contains("theme.Accent=ff7700/"));
    let mut after = before.clone();
    after.menu = after.menu.replace("theme.Accent=123456/", "theme.Accent=abcdef/");
    after.saved[0].text = after.saved[0].text.replace("theme.Accent=ff7700/", "theme.Accent=00aa99/");
    let applied = apply(&dir, &before, &after, &before, true).unwrap();
    assert_eq!(applied, after);
    super::super::with(|g| {
        assert!(g.interface_profile.is_none());
        assert_eq!(theme::col(&g.theme, "Accent").rgb, 0xabcdef);
        g.profile_follow();
        assert_eq!(theme::col(&g.theme, "Accent").rgb, 0x00aa99);
        g.save();
    });
    let (_, saved) = snapshot().unwrap();
    assert!(saved.menu.contains("theme.Accent=abcdef/"));
    assert!(saved.saved[0].text.contains("theme.Accent=00aa99/"));
}

#[test]
fn invitations_are_owned_revocable_and_loaded_through_the_recode_field() {
    if isolated(concat!(module_path!(), "::invitations_are_owned_revocable_and_loaded_through_the_recode_field")) {
        return;
    }
    let f = Fixture::new();
    let mut rt = Runtime::default();
    rt.cycle(&f.auth, None).unwrap();
    let own = rt.remote.as_ref().unwrap().id;
    let baseline = rt.baseline.remote.clone();
    rt.cycle(&f.auth, Some(Job::MakeInvitation(own))).unwrap();
    assert_eq!(view().invitations.len(), 1);
    assert_eq!(view().invitations[0].max_uses, 1);
    assert_eq!(rt.baseline.remote, baseline);
    assert_eq!(f.db(|d| d.get(own).version), 1);
    let code = view().invitations[0].code.clone();
    super::super::with(|g| {
        g.card = Some(super::super::Card::Cloud { at: [0.; 4], tab: 3, page: 0 });
        g.cloud_codes = vec![code.clone()];
        assert!(g.cloud_press(super::super::Hit::Cloud(17, 0, 0), 0));
        assert!(g.outbox.iter().any(|s| s.contains(&code))); // Only an ask; this test never changes the OS clipboard.
        g.outbox.clear();
        assert!(g.cloud_press(super::super::Hit::Cloud(18, 0, own), 0));
        assert!(JOBS.lock().unwrap().is_empty());
        g.now += 5.0;
        assert!(g.cloud_press(super::super::Hit::Cloud(18, 0, own), 0)); // expired: ask again, no removal
        assert!(JOBS.lock().unwrap().is_empty());
        assert!(g.cloud_press(super::super::Hit::Cloud(18, 0, own), 0));
    });
    let revoke = JOBS.lock().unwrap().pop_front().unwrap().job;
    assert!(matches!(&revoke, Job::RevokeInvitation(id, c) if *id == own && *c == code));
    rt.cycle(&f.auth, Some(revoke)).unwrap();
    update(|v| v.busy = false);
    assert!(view().invitations.is_empty());
    let before = snapshot().unwrap().1;
    assert_eq!(rt.cycle(&f.auth, Some(Job::RedeemInvitation("NOT-A-CODE".into()))), Err(604));
    assert_eq!(snapshot().unwrap().1, before);
    let key = super::super::with(|g| format!("{}.#on", g.mods[0].id));
    let shared = flip(&before, &key);
    let other = f.db(|d| {
        let id = d.add(&shared, false);
        d.get(id).invitations.push(json!({"code":"JOIN-TEST","max_uses":1,"uses":0}));
        id
    });
    super::super::with(|g| {
        g.paste("join-test");
        assert_eq!(g.card_field, "JOIN-TEST");
        assert_eq!(g.key(false, true, false, "Return".into(), false), 1);
    });
    let command = JOBS.lock().unwrap().pop_front().unwrap();
    assert_eq!(command.revision, REVISION.load(Ordering::Relaxed));
    assert!(matches!(command.job, Job::RedeemInvitation(_)));
    rt.cycle(&f.auth, Some(command.job)).unwrap();
    update(|v| v.busy = false);
    assert_eq!(rt.remote.as_ref().unwrap().id, other);
    assert!(!view().own);
    assert!(view().invitations.is_empty());
    assert_eq!(f.db(|d| d.active), other);
    assert_eq!(snapshot().unwrap().1, shared);
    assert_eq!(f.db(|d| d.get(other).invitations[0]["uses"].as_i64()), Some(1));
    assert_eq!(rt.cycle(&f.auth, Some(Job::MakeInvitation(other))), Err(403));
}

#[test]
fn two_devices_conflicts_history_shared_configs_and_active_switches_use_real_network_and_files() {
    if isolated(concat!(
        module_path!(),
        "::two_devices_conflicts_history_shared_configs_and_active_switches_use_real_network_and_files"
    )) {
        return;
    }
    let f = Fixture::new();
    let (dir, initial) = snapshot().unwrap();
    // A partial config and its server assignment travel with the full collection.
    let cfg = dir.join("cursed-recode/configs");
    std::fs::create_dir(&cfg).unwrap();
    std::fs::write(
        cfg.join("Only-theme.cfg"),
        "#name=Only theme\n#parts=4\n#servers=example.invalid\ntheme.Scale=1\n",
    )
    .unwrap();
    let keys = super::super::with(|g| (0..4).map(|i| format!("{}.#on", g.mods[i].id)).collect::<Vec<_>>());
    let mut rt = Runtime::default();
    rt.cycle(&f.auth, None)
        .unwrap_or_else(|e| panic!("first cloud cycle: {e}, WinHTTP: {}", crate::net::last_failure()));
    assert_eq!(f.db(|d| d.configs.len()), 1);
    let id = rt.remote.as_ref().unwrap().id;
    rt.cycle(&f.auth, None).unwrap();
    assert_eq!(f.db(|d| (d.configs.len(), d.puts)), (1, 0));
    assert_eq!(rt.remote.as_ref().unwrap().pack.saved.len(), 1);

    local_flip(0);
    rt.cycle(&f.auth, None).unwrap();
    rt.pending.as_mut().unwrap().1 = Instant::now() - Duration::from_secs(4);
    rt.cycle(&f.auth, None).unwrap();
    assert_eq!(f.db(|d| d.get(id).version), 2);
    let changed = flip(&rt.remote.as_ref().unwrap().pack, &keys[1]);
    remote_change(&f, id, changed.clone());
    rt.cycle(&f.auth, None).unwrap();
    assert_eq!(snapshot().unwrap().1, changed);
    assert!(!storage::backups(&dir).unwrap().is_empty());

    local_flip(0);
    let mine = snapshot().unwrap().1;
    let mut theirs = flip(&changed, &keys[2]);
    theirs.menu.push_str("future.setting=kept\n");
    remote_change(&f, id, theirs);
    let writes = f.db(|d| d.puts);
    rt.cycle(&f.auth, None).unwrap();
    assert!(view().conflict);
    assert_eq!(snapshot().unwrap().1, mine);
    assert_eq!(f.db(|d| d.puts), writes);
    let version = rt.remote.as_ref().unwrap().version;
    rt.cycle(&f.auth, Some(Job::KeepLocal(id, version))).unwrap();
    assert!(f.db(|d| Bundle::decode(&d.get(id).code).unwrap().menu.contains("future.setting=kept\n")));
    rt.cycle(&f.auth, Some(Job::History)).unwrap();
    assert!(!view().versions.is_empty());

    // A third device changes the server after this device's last read.
    local_flip(3);
    let stale = rt.remote.as_ref().unwrap().version;
    let next = flip(&rt.remote.as_ref().unwrap().pack, &keys[2]);
    f.db(|d| d.get(id).change(next.encode().unwrap()));
    let history = f.db(|d| d.get(id).history.len());
    assert_eq!(rt.cycle(&f.auth, Some(Job::KeepLocal(id, stale))), Err(409));
    assert_eq!(f.db(|d| d.get(id).history.len()), history);
    failed(409);
    rt.cycle(&f.auth, None).unwrap();
    assert!(view().conflict);
    let version = rt.remote.as_ref().unwrap().version;
    rt.cycle(&f.auth, Some(Job::TakeCloud(id, version))).unwrap();
    assert!(!view().conflict);
    let copies = storage::backups(&dir).unwrap();
    assert!(storage::read_backup(&dir, &copies[0].id).unwrap().menu.contains("future.setting=kept\n"));

    // Following an active config must not upload the snapshot captured before the follow.
    let new = flip(&snapshot().unwrap().1, &keys[3]);
    let other = f.db(|d| {
        let n = d.add(&new, true);
        d.active = n;
        n
    });
    STALE.store(true, Ordering::Relaxed);
    rt.cycle(&f.auth, None).unwrap();
    assert_eq!(rt.baseline.id, other);
    assert!(rt.pending.is_none());
    assert!(!view().conflict);
    let writes = f.db(|d| d.puts);
    rt.cycle(&f.auth, None).unwrap();
    assert_eq!(f.db(|d| d.puts), writes);
    let another = flip(&rt.remote.as_ref().unwrap().pack, &keys[0]);
    remote_change(&f, other, another);
    rt.cycle(&f.auth, None).unwrap();
    let expected = rt.remote.as_ref().unwrap().version;
    let concurrent = flip(&rt.remote.as_ref().unwrap().pack, &keys[2]);
    f.db(|d| d.get(other).change(concurrent.encode().unwrap()));
    assert_eq!(rt.cycle(&f.auth, Some(Job::RestoreVersion(other, 1, expected))), Err(409));
    assert_eq!(f.db(|d| d.get(other).code.clone()), concurrent.encode().unwrap());
    failed(409);
    rt.cycle(&f.auth, None).unwrap();
    let expected = rt.remote.as_ref().unwrap().version;
    rt.cycle(&f.auth, Some(Job::RestoreVersion(other, 1, expected))).unwrap();
    assert_eq!(rt.remote.as_ref().unwrap().pack, new);

    let mut shared = flip(&snapshot().unwrap().1, &keys[1]);
    shared.menu.push_str("next-build.option=kept\n");
    let shared_id = f.db(|d| {
        let n = d.add(&shared, false);
        d.active = n;
        n
    });
    STALE.store(true, Ordering::Relaxed);
    rt.cycle(&f.auth, None).unwrap();
    local_flip(2);
    let writes = f.db(|d| d.puts);
    rt.cycle(&f.auth, None).unwrap();
    assert_eq!(f.db(|d| d.puts), writes);
    assert!(!view().conflict);
    assert_eq!(view().status, "Shared config: local changes stay on this PC");
    rt.cycle(&f.auth, Some(Job::KeepLocal(shared_id, 1))).unwrap();
    assert!(rt.remote.as_ref().unwrap().own);
    assert_ne!(rt.remote.as_ref().unwrap().id, shared_id);
    assert!(rt.remote.as_ref().unwrap().pack.menu.contains("next-build.option=kept\n"));
    assert_eq!(f.db(|d| d.get(shared_id).version), 1);

    // An unsupported active config still lets the player select a usable one.
    let good = rt.remote.as_ref().unwrap().id;
    let broken = f.db(|d| {
        let n = d.add(&shared, true);
        d.get(n).code = "not-a-config".into();
        d.active = n;
        n
    });
    STALE.store(true, Ordering::Relaxed);
    rt.cycle(&f.auth, None).unwrap();
    assert_eq!(view().status, "Cloud config has another format");
    assert_eq!(view().active, 0);
    rt.cycle(&f.auth, Some(Job::Activate(good))).unwrap();
    assert_eq!(f.db(|d| d.active), good);
    assert_ne!(good, broken);

    // Reconnecting to an API without compare-and-set support never writes.
    f.db(|d| d.capable = false);
    let before = f.db(|d| (d.configs.len(), d.puts));
    assert_eq!(rt.cycle(&f.auth, Some(Job::Refresh)), Err(602));
    assert!(!view().online);
    assert!(rt.remote.is_none());
    assert!(rt.last_fetch.is_none());
    assert_eq!(rt.cycle(&f.auth, Some(Job::KeepLocal(good, 1))), Err(602));
    assert_eq!(Runtime::default().cycle(&f.auth, None), Err(602));
    assert_eq!(f.db(|d| (d.configs.len(), d.puts)), before);
    assert_eq!(initial.saved.len(), 0);
}

#[test]
fn sharing_card_layout_uses_real_fonts_without_a_game_window() {
    if std::env::var_os("CURSED_SHARING_LAYOUT_CHILD").is_none() {
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "gui::cloud::tests::sharing_card_layout_uses_real_fonts_without_a_game_window",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("CURSED_SHARING_LAYOUT_CHILD", "1")
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        return;
    }
    use crate::gui::{
        draw::Canvas,
        text::{fonts, Face},
        Hit,
    };
    for face in [Face::Regular, Face::Medium, Face::Bold, Face::Semi, Face::Mono] {
        for ch in "ӘәІіҢңҒғҮүҰұҚқӨөҺһ".chars() {
            fonts().glyph(face, ch);
        }
    }
    for _ in 0..400 {
        if ![Face::Regular, Face::Medium, Face::Bold, Face::Semi, Face::Mono]
            .iter()
            .any(|&face| "ӘәІіҢңҒғҮүҰұҚқӨөҺһ".chars().any(|ch| crate::gui::dynfont::waiting(face, ch)))
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let mut dynamic = Vec::new();
    for lang in ["ru_ru", "en_us", "uk_ua", "kk_kz"] {
        crate::menus::lang::set(lang);
        update(|v| {
            *v = View {
                status: "Cloud is up to date",
                account: "preview".into(),
                active: 1,
                name: "Everyday".into(),
                version: 3,
                own: true,
                online: true,
                invitations: (1..=4)
                    .map(|n| Invitation {
                        code: format!("DEMO-CODE-{n}"),
                        max_uses: 1,
                        uses: 0,
                    })
                    .collect(),
                ..View::default()
            }
        });
        for (w, h, r) in [(960.0, 540.0, 12.0), (1600.0, 900.0, 18.0)] {
            let mut g = Gui::new();
            g.w = w;
            g.h = h;
            g.r = r;
            g.dt = 0.1;
            g.now = 10.0;
            g.win = [0.0, 0.0, w, h];
            g.cloud_revoke = Some((1, "DEMO-CODE-1".into(), g.now + 4.0));
            let mut c = Canvas::new(w, h);
            for _ in 0..20 {
                g.now += 0.1;
                g.hits.clear();
                c.p.clear();
                g.cloud_card(&mut c, [0.0; 4], 3, 0);
            }
            let bounds = g.card_rect;
            assert!(bounds[0] >= 0.0 && bounds[1] >= 0.0 && bounds[2] <= w && bounds[3] <= h, "{lang}: {bounds:?}");
            for (rect, hit) in &g.hits {
                if matches!(hit, Hit::Cloud(..)) {
                    assert!(rect[2] > rect[0] && rect[3] > rect[1], "{lang}: {hit:?}");
                    assert!(
                        rect[0] >= bounds[0] && rect[1] >= bounds[1] && rect[2] <= bounds[2] && rect[3] <= bounds[3],
                        "{lang}: {hit:?} {rect:?}"
                    );
                }
            }
            if w == 1600.0 {
                if let Some(path) = std::env::var_os("CURSED_TEST_CARD_DUMP") {
                    let path = std::path::PathBuf::from(path);
                    std::fs::create_dir_all(&path).unwrap();
                    dynamic.extend(crate::gui::dynfont::drain());
                    std::fs::write(
                        path.join(format!("sharing-{lang}.json")),
                        serde_json::to_vec(&serde_json::json!({"w":w,"h":h,"shapes":c.p,"dynamic":dynamic})).unwrap(),
                    )
                    .unwrap();
                }
            }
        }
    }
}
