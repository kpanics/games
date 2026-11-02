//! Нативная половина лоадера Cursed Recode на Linux (docs/CACHYOS.md). Всё, что не требует окна:
//! манифест и его ed25519-подпись (та же пара, что у Windows-лоадера), синк object-<sha256> с релиза
//! и ассетов с CDN Mojang, распаковка runtime.zip, настройки - и запуск игры (bin/CursedRecode.exe,
//! Windows-JDK из runtime.zip) через Wine со всеми путями, которые Win32-стороной ждут буквально.
//!
//! Порядок и види файлов - как у Windows-лоадера (Cursed-Recode-Loader-main/src/loader), глазами
//! Linux: тот же start.json, те же хеши, тот же home с loader/ внутри.

use std::collections::{HashSet, VecDeque};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde_json::Value;
use sha1::Sha1;
use sha2::{Digest, Sha256};

/// База дистрибутива «на GitHub»: технический релиз-склад с тегом `dist-objects` (витрина `dist`
/// держит только recode_loader.exe; объекты и start.json - там, переключается --base).
pub const BASE: &str = "https://github.com/kpanics/games/releases/download/dist-objects";
/// Публичная половина ключа манифеста — та же, что вшита в Windows-лоадер (CURSED_VERIFY_KEY CI).
pub const KEY: &str = "553f8e75906c6f2cee9b76f2243625ece155f07b10a8a9779af58356b061db2c";
/// Ассеты Minecraft — собственность Mojang, качаем с их CDN (как и Windows-лоадер).
pub const ASSETS: &str = "https://resources.download.minecraft.net";

// ---------------------------------------------------------------------------------------------- hex и хеши

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hex_bytes(s: &str) -> Option<Vec<u8>> {
    (s.len() % 2 == 0).then(|| (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect::<Option<Vec<u8>>>()).flatten()
}

pub fn sha256_file(path: &Path) -> Result<String, String> {
    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex(&h.finalize()))
}

fn sha1_file(path: &Path) -> Result<String, String> {
    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut h = Sha1::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex(&h.finalize()))
}

/// Скачивание — системным curl: TLS, редиректы (релизы GitHub уходят на CDN) и повторы — его забота.
fn curl(url: &str, dst: &Path) -> Result<(), String> {
    let ok = Command::new("curl")
        .args(["-fsSL", "--retry", "5", "--retry-delay", "2", "--retry-all-errors", "--connect-timeout", "20", "-o"])
        .arg(dst)
        .arg(url)
        .status()
        .map_err(|e| format!("curl не запускается: {e} (он установлен?)"))?;
    if !ok.success() {
        return Err(format!("curl: {url} (код {})", ok.code().unwrap_or(-1)));
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------- манифест

pub struct Entry {
    pub path: String,
    pub sha256: String,
    pub size: u64,
    pub kind: String,
}

pub struct Manifest {
    pub build: i64,
    pub version: String,
    pub notes: String,
    pub launch: Value,
    pub files: Vec<Entry>,
}

/// Манифест, если подпись над его точным текстом — наша (как sync::verify Windows-лоадера).
pub fn verify_manifest(text: &str, sig: &str) -> Result<Manifest, String> {
    let key: [u8; 32] = hex_bytes(KEY).and_then(|k| k.try_into().ok()).ok_or("ключ")?;
    let key = VerifyingKey::from_bytes(&key).map_err(|_| "ключ")?;
    let sig: [u8; 64] = hex_bytes(sig).and_then(|s| s.try_into().ok()).ok_or("подпись")?;
    key.verify(text.as_bytes(), &Signature::from_bytes(&sig)).map_err(|_| "подпись не сошлась".to_string())?;
    parse_manifest(text)
}

fn parse_manifest(text: &str) -> Result<Manifest, String> {
    let v: Value = serde_json::from_str(text).map_err(|_| "манифест".to_string())?;
    let mut files = Vec::new();
    for f in v["files"].as_array().ok_or("манифест: files")? {
        let path = f["path"].as_str().ok_or("манифест: path")?.to_string();
        if path.is_empty() || path.starts_with('/') || path.contains('\\') || path.contains(':')
            || path.split('/').any(|p| p.is_empty() || p == "." || p == "..")
        {
            return Err(format!("манифест: путь {path}"));
        }
        files.push(Entry {
            sha256: f["sha256"].as_str().filter(|s| s.len() == 64).ok_or("манифест: sha256")?.to_ascii_lowercase(),
            size: f["size"].as_u64().ok_or("манифест: size")?,
            kind: f["kind"].as_str().unwrap_or("").to_string(),
            path,
        });
    }
    Ok(Manifest {
        build: v["build"].as_i64().ok_or("манифест: build")?,
        version: v["version"].as_str().unwrap_or("").to_string(),
        notes: v["notes"].as_str().unwrap_or("").to_string(),
        launch: v["launch"].clone(),
        files,
    })
}

/// Свежий start.json с базы: скачан и проверен подписью.
pub fn fetch_start(base: &str) -> Result<Manifest, String> {
    let tmp = std::env::temp_dir().join("cursed-linux-start.json");
    curl(&format!("{base}/start.json"), &tmp)?;
    let text = std::fs::read_to_string(&tmp).map_err(|e| format!("start.json: {e}"))?;
    let start: Value = serde_json::from_str(&text).map_err(|e| format!("start.json: {e}"))?;
    verify_manifest(start["manifest"].as_str().unwrap_or(""), start["sig"].as_str().unwrap_or(""))
}

// ---------------------------------------------------------------------------------------------- настройки

#[derive(Clone)]
pub struct Settings {
    pub memory: u32,
    /// "ru" | "en"
    pub lang: String,
    /// Какой wine (имя или путь); пусто - "wine".
    pub wine: String,
    /// Папка игры, если выбрана другая; пусто - стандартная (на «диске C:» Wine).
    pub home: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { memory: 4096, lang: "ru".into(), wine: String::new(), home: String::new() }
    }
}

impl Settings {
    /// Настройки лоадера — там же, где их держит Windows-лоадер: `<home>/loader/settings.json`.
    pub fn load(home: &Path) -> Settings {
        let mut s = Settings::default();
        if let Ok(text) = std::fs::read_to_string(home.join("loader").join("settings.json")) {
            if let Ok(v) = serde_json::from_str::<Value>(&text) {
                if let Some(m) = v["memory"].as_u64() {
                    s.memory = m.clamp(1024, 32768) as u32;
                }
                if let Some(l) = v["lang"].as_str() {
                    s.lang = if l == "en" { "en".into() } else { "ru".into() };
                }
                if let Some(w) = v["wine"].as_str() {
                    s.wine = w.into();
                }
                if let Some(h) = v["home"].as_str() {
                    s.home = h.into();
                }
            }
        }
        s
    }

    pub fn save(&self, home: &Path) {
        let dir = home.join("loader");
        let _ = std::fs::create_dir_all(&dir);
        let v = serde_json::json!({ "memory": self.memory, "lang": self.lang, "wine": self.wine, "home": self.home });
        let tmp = dir.join("settings.tmp");
        if std::fs::write(&tmp, v.to_string()).is_ok() {
            let _ = std::fs::rename(&tmp, dir.join("settings.json"));
        }
    }

    pub fn wine_binary(&self) -> String {
        if self.wine.trim().is_empty() { "wine".into() } else { self.wine.trim().into() }
    }
}

/// Префикс Wine: $WINEPREFIX, иначе ~/.wine.
pub fn wine_prefix() -> PathBuf {
    if let Some(p) = std::env::var_os("WINEPREFIX").filter(|p| !p.is_empty()) {
        return PathBuf::from(p);
    }
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".wine")
}

/// Папка игры. По умолчанию - на «диске C:» Wine (`<prefix>/drive_c/CursedRecode`): у Wine 11 в новом
/// режиме путей файлы из $HOME он зовёт `\unix\home\…` без диска, и Windows-код JDK такие пути не
/// находит («could not find java.dll»); на C: пути - канонические `C:\…`, как на Windows. Папка без
/// пробелов: JNI-сторона JVM известна слабостью к пробелам («Failed setting boot class path»).
pub fn default_home() -> PathBuf {
    wine_prefix().join("drive_c").join("CursedRecode")
}

/// Папка игры с учётом выбора (настройки/флаг).
pub fn home_of(settings: &Settings, given: Option<&Path>) -> PathBuf {
    if let Some(g) = given {
        return g.to_path_buf();
    }
    if !settings.home.trim().is_empty() {
        return PathBuf::from(settings.home.trim());
    }
    default_home()
}

/// Windows-путь к папке внутри префикса: файлы на «диске C:» — как `C:\…`; вне его — спросим у
/// winepath (для папок, выбранных вручную где-нибудь в $HOME).
pub fn windows_path(path: &Path, wine: &str) -> Result<String, String> {
    let prefix = wine_prefix();
    if let Ok(rel) = path.strip_prefix(prefix.join("drive_c")) {
        let parts: Vec<String> = rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).filter(|s| !s.is_empty()).collect();
        if !parts.is_empty() {
            return Ok(format!("C:\\{}", parts.join("\\")));
        }
    }
    let out = Command::new(wine).arg("winepath").arg("-w").arg(path).output().map_err(|e| format!("winepath: {e}"))?;
    if !out.status.success() {
        return Err(format!("winepath не смог: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

// ---------------------------------------------------------------------------------------------- синк

/// Что скачать: путь в папке игры, ожидаемый хеш, размер и адрес.
struct Job {
    path: String,
    hash: String,
    size: u64,
    url: String,
    /// Ассет: SHA-1 (файлы билда — SHA-256).
    sha1: bool,
}

/// Что лоадер делает, для окна: числа, которые видно.
pub struct Progress {
    pub bytes: AtomicU64,
    pub files: AtomicU64,
    pub total_bytes: AtomicU64,
    pub total_files: AtomicU64,
    /// 0 - проверяем, 1 - качаем, 2 - распаковываем, 3 - готово, 4 - не вышло.
    pub phase: AtomicU8,
    pub done: AtomicBool,
}

impl Progress {
    pub fn new() -> Arc<Progress> {
        Arc::new(Progress {
            bytes: AtomicU64::new(0),
            files: AtomicU64::new(0),
            total_bytes: AtomicU64::new(0),
            total_files: AtomicU64::new(0),
            phase: AtomicU8::new(0),
            done: AtomicBool::new(false),
        })
    }
}

/// Файлы билда: всё, что не Windows-лоадер (его роль тут играет эта программа).
fn plan_files(home: &Path, m: &Manifest, base: &str) -> Vec<Job> {
    let mut jobs = Vec::new();
    for f in &m.files {
        if f.kind == "loader" {
            continue;
        }
        let full = home.join(&f.path);
        let have = if f.kind == "zip" {
            zip_marker(&zip_dir(&full), &f.sha256).is_file()
        } else {
            full.is_file()
                && std::fs::metadata(&full).map(|md| md.len() == f.size).unwrap_or(false)
                && sha256_file(&full).map(|h| h == f.sha256).unwrap_or(false)
        };
        if have {
            continue;
        }
        jobs.push(Job { url: format!("{base}/object-{}", f.sha256), hash: f.sha256.clone(), size: f.size, path: f.path.clone(), sha1: false });
    }
    jobs
}

/// Ассеты игры по её индексу (он сам - файл билда, скачан первым): путь и хеш Mojang, проверка размером.
fn asset_jobs(home: &Path, m: &Manifest) -> Vec<Job> {
    let Some(index) = m.files.iter().find(|f| f.kind == "assets-index") else { return Vec::new() };
    let Ok(text) = std::fs::read(home.join(&index.path)) else { return Vec::new() };
    let Ok(v) = serde_json::from_slice::<Value>(&text) else { return Vec::new() };
    let mut jobs = Vec::new();
    for x in v["objects"].as_object().into_iter().flatten().map(|(_, x)| x) {
        let (Some(hash), Some(size)) = (x["hash"].as_str().filter(|h| h.len() == 40), x["size"].as_u64()) else { continue };
        let hash = hash.to_ascii_lowercase();
        let path = format!("assets/objects/{}/{}", &hash[..2], hash);
        let have = home.join(&path).is_file() && std::fs::metadata(home.join(&path)).map(|md| md.len() == size).unwrap_or(false);
        if !have {
            let url = format!("{ASSETS}/{}/{}", &hash[..2], hash);
            jobs.push(Job { path, hash, size, url, sha1: true });
        }
    }
    jobs
}

/// Один файл: во временного, хеш и размер сверены — на место (как fetch_one Windows-лоадера).
fn fetch_one(home: &Path, job: &Job, p: &Progress) -> Result<(), String> {
    let full = home.join(&job.path);
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", job.path))?;
    }
    let mut name = full.file_name().unwrap_or_default().to_os_string();
    name.push(".part");
    let part = full.with_file_name(name);
    curl(&job.url, &part)?;
    let hash_ok = if job.sha1 { sha1_file(&part).map(|h| h == job.hash).unwrap_or(false) } else { sha256_file(&part).map(|h| h == job.hash).unwrap_or(false) };
    if !hash_ok || std::fs::metadata(&part).map(|md| md.len() != job.size).unwrap_or(true) {
        let _ = std::fs::remove_file(&part);
        return Err(format!("{}: пришло не то, что в манифесте", job.path));
    }
    std::fs::rename(&part, &full).map_err(|e| format!("{}: {e}", job.path))?;
    p.bytes.fetch_add(job.size, Ordering::Relaxed);
    p.files.fetch_add(1, Ordering::Relaxed);
    Ok(())
}

/// Восемь потоков над общей очередью; что не пришло — возвращается в очередь и пробуется ещё раз
/// после паузы (CDN Mojang порой не отвечает на часть из пяти тысяч подряд, потом отвечает —
/// как fetch_all Windows-лоадера, только кругов три).
fn sync_all(home: &Path, jobs: Vec<Job>, p: &Progress) -> Result<(), String> {
    let mut jobs = jobs;
    let mut last = String::new();
    for round in 0..3 {
        if jobs.is_empty() {
            return Ok(());
        }
        if round > 0 {
            println!("  {} файлов ещё раз после паузы ({})", jobs.len(), last);
            std::thread::sleep(Duration::from_secs(3));
        }
        let queue = Mutex::new(std::mem::take(&mut jobs));
        let failed: Mutex<Vec<Job>> = Mutex::new(Vec::new());
        std::thread::scope(|s| {
            for _ in 0..8 {
                s.spawn(|| loop {
                    let Some(job) = queue.lock().unwrap().pop() else { break };
                    if let Err(e) = fetch_one(home, &job, p) {
                        eprintln!("  {e}");
                        failed.lock().unwrap().push(job);
                    }
                });
            }
        });
        jobs = failed.into_inner().unwrap();
        if let Some(j) = jobs.first() {
            last = j.path.clone();
        }
    }
    if jobs.is_empty() {
        Ok(())
    } else {
        Err(format!("не пришли {} файлов (первый: {})", jobs.len(), jobs[0].path))
    }
}

fn zip_dir(full: &Path) -> PathBuf {
    full.with_extension("")
}

fn zip_marker(dir: &Path, hash: &str) -> PathBuf {
    dir.join(format!(".unpacked-{}", &hash[..16]))
}

/// Папки билда, что пришли одним объектом (`runtime.zip` → `runtime/`), распаковываются на месте:
/// маркер внутри называет объект, из которого папка вышла, — другой билд распакуется заново.
pub fn unpack_zips(home: &Path, m: &Manifest) -> Result<(), String> {
    for f in &m.files {
        if f.kind != "zip" {
            continue;
        }
        let full = home.join(&f.path);
        if zip_marker(&zip_dir(&full), &f.sha256).is_file() {
            continue;
        }
        println!("  распаковка {} …", f.path);
        let dir = zip_dir(&full);
        unpack_zip(&full, &dir).map_err(|e| format!("{}: {e}", f.path))?;
        std::fs::write(zip_marker(&dir, &f.sha256), f.sha256.as_bytes()).map_err(|e| format!("{}: {e}", f.path))?;
        let _ = std::fs::remove_file(&full);
    }
    Ok(())
}

/// Имя члена архива, приведённое к порядку: PowerShell Compress-Archive пишет пути с обратными
/// слешами («bin\java.exe»), что на Windows — сепаратор, а на Linux — просто символ в имени файла.
/// Нормализуем и выбрасываем всё, что суется наружу (`..`, абсолютные пути).
fn safe_name(raw: &str) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for part in raw.replace('\\', "/").split('/') {
        if part.is_empty() || part == "." || part.contains(':') || part.contains('\0') {
            continue;
        }
        if part == ".." {
            return None;
        }
        out.push(part);
    }
    (!out.as_os_str().is_empty()).then_some(out)
}

/// Один объект билда, раскрытый в папку. Верхнюю папку внутри (у JDK - своя `jdk-25…/`) срезаем;
/// члены берутся по индексу и кладутся по нормализованному имени, а не по тому, что архив сам написал.
fn unpack_zip(object: &Path, into: &Path) -> Result<(), String> {
    let f = std::fs::File::open(object).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(f).map_err(|e| e.to_string())?;
    let names: Vec<(usize, PathBuf)> = (0..archive.len())
        .filter_map(|i| {
            let e = archive.by_index(i).ok()?;
            let name = safe_name(e.name())?;
            (!e.is_dir()).then_some((i, name))
        })
        .collect();
    let peel = {
        let tops = names.iter().map(|(_, n)| n.components().next()).collect::<HashSet<_>>();
        tops.len() == 1 && tops.into_iter().next().is_some_and(|t| t.is_some()) && names.iter().any(|(_, n)| n.components().count() >= 2)
    };
    for (i, name) in &names {
        let rel: PathBuf = if peel { name.components().skip(1).collect() } else { name.clone() };
        if rel.as_os_str().is_empty() {
            continue;
        }
        let out = into.join(&rel);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", rel.display()))?;
        }
        let mut e = archive.by_index(*i).map_err(|e| e.to_string())?;
        let mut o = std::fs::File::create(&out).map_err(|e| format!("{}: {e}", rel.display()))?;
        std::io::copy(&mut e, &mut o).map_err(|e| format!("{}: {e}", rel.display()))?;
    }
    Ok(())
}

/// Полный синк: манифест (подпись проверена), файлы билда, ассеты, распаковка. Прогресс — для окна.
pub fn sync(home: &Path, base: &str, p: &Progress) -> Result<Manifest, String> {
    let m = fetch_start(base)?;
    std::fs::create_dir_all(home).map_err(|e| e.to_string())?;
    p.phase.store(1, Ordering::Relaxed);
    let mut jobs = plan_files(home, &m, base);
    // индекс ассетов — первым: по нему план ассетов (как sync::plan Windows-лоадера)
    if let Some(idx) = m.files.iter().find(|f| f.kind == "assets-index") {
        if !home.join(&idx.path).is_file() {
            if let Some(at) = jobs.iter().position(|j| j.path == idx.path) {
                let j = jobs.remove(at);
                fetch_one(home, &j, p)?;
            }
        }
    }
    jobs.extend(asset_jobs(home, &m));
    p.total_files.store(jobs.len() as u64, Ordering::Relaxed);
    p.total_bytes.store(jobs.iter().map(|j| j.size).sum(), Ordering::Relaxed);
    if !jobs.is_empty() {
        sync_all(home, jobs, p)?;
    }
    p.phase.store(2, Ordering::Relaxed);
    unpack_zips(home, &m)?;
    ensure_runtimes(home);
    p.phase.store(3, Ordering::Relaxed);
    Ok(m)
}

/// VC-рантаймы (msvcp140, vcruntime140…) — из runtime/bin в bin/: Windows ищет зависимости DLL
/// рядом с exe и в системе, не рядом с самой DLL; в префиксе Wine их может не быть, и тогда
/// jvm.dll «не грузится». Копируются только если их ещё нет.
pub fn ensure_runtimes(home: &Path) {
    let from = home.join("runtime").join("bin");
    let to = home.join("bin");
    let Ok(list) = std::fs::read_dir(&from) else { return };
    for e in list.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if (name.starts_with("msvcp") || name.starts_with("vcruntime") || name.starts_with("concrt")) && name.ends_with(".dll") {
            let dst = to.join(&name);
            if !dst.is_file() {
                let _ = std::fs::create_dir_all(&to);
                let _ = std::fs::copy(e.path(), &dst);
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------- запуск игры

/// `${root}` в аргументах билда — папка игры, как её видит Windows (`C:\…`).
fn fill(s: &str, root_win: &str) -> String {
    s.replace("${root}", root_win)
}

/// Запуск игры: то же, что делает loader/game.rs — host (bin/CursedRecode.exe, Java внутри него) с
/// JDK билда, флагами и classpath из манифеста, — под Wine и с путями, которые Win32-стороной
/// ждут буквально: `--jdk` и нативная библиотека — абсолютные `C:\…` (относительный --jdk под Wine
/// роняет загрузку jvm.dll, путь в $HOME — «could not find java.dll» нового режима путей Wine).
/// Вывод игры — в `<home>/loader/game.log` (окно лоадера показывает его хвост).
pub fn launch(home: &Path, m: &Manifest, settings: &Settings, server: Option<&str>) -> Result<Child, String> {
    let wine = settings.wine_binary();
    let launch = &m.launch;
    let host = launch["host"].as_str().ok_or("launch: host")?;
    if !home.join(host).is_file() {
        return Err(format!("{host}: нет такого файла — сначала синхронизация"));
    }
    let root_win = windows_path(home, &wine)?;
    let log = home.join("loader").join("game.log");
    std::fs::create_dir_all(log.parent().unwrap_or(home)).map_err(|e| e.to_string())?;
    let log_file = std::fs::OpenOptions::new().create(true).write(true).truncate(true).open(&log).map_err(|e| format!("{log:?}: {e}"))?;
    let mut cmd = Command::new(&wine);
    cmd.current_dir(home).arg(host).stdout(Stdio::from(log_file.try_clone().map_err(|e| e.to_string())?)).stderr(Stdio::from(log_file));
    if let Some(jdk) = launch["jdk"].as_str() {
        if home.join(jdk).join("bin").is_dir() {
            let win = windows_path(&home.join(jdk), &wine)?;
            cmd.arg("--jdk").arg(win);
        }
    }
    let least = launch["memory"]["least"].as_u64().unwrap_or(2048) as u32;
    cmd.arg(format!("-Xmx{}m", settings.memory.max(least)));
    if let Some(nat) = launch["jvm"].as_array().and_then(|a| {
        a.iter().filter_map(|x| x.as_str()).find_map(|s| s.strip_prefix("-Dcursed.recode.native="))
    }) {
        // нативная библиотека — абсолютным Windows-путём: относительный ./bin/… движок ищет от cwd
        // процесса, а не от папки игры
        let win = windows_path(&home.join(nat.trim_start_matches("${root}/")), &wine)?;
        cmd.arg(format!("-Dcursed.recode.native={win}"));
    }
    for a in launch["jvm"].as_array().into_iter().flatten().filter_map(|a| a.as_str()) {
        if a.starts_with("-Dcursed.recode.native=") {
            continue;
        }
        cmd.arg(fill(a, &root_win));
    }
    let cp = launch["classpath"]
        .as_array()
        .ok_or("launch: classpath")?
        .iter()
        .filter_map(|p| p.as_str())
        .map(|p| fill(p, &root_win))
        .collect::<Vec<String>>()
        .join(";");
    cmd.arg("-cp").arg(cp).arg(launch["main"].as_str().ok_or("launch: main")?);
    // --assetIndex/--assetsDir/--gameDir уже в аргументах билда (манифест)
    for a in launch["game"].as_array().into_iter().flatten().filter_map(|a| a.as_str()) {
        cmd.arg(fill(a, &root_win));
    }
    cmd.arg("--username").arg("Player");
    if let Some(s) = server.filter(|s| !s.is_empty()) {
        cmd.arg("--quickPlayMultiplayer").arg(s);
    }
    cmd.env("CURSED_BUILD", m.build.to_string());
    cmd.spawn().map_err(|e| format!("wine не запускается: {e} (пакет wine установлен?)"))
}

impl Manifest {
    /// Идентификатор индекса ассетов этого билда (имя файла без .json).
    pub fn assets_id(&self) -> String {
        self.files
            .iter()
            .find(|f| f.kind == "assets-index")
            .map(|f| Path::new(&f.path).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default())
            .unwrap_or_default()
    }
}

/// Живой хвост лога игры для окна лоадера: помнит, до чего дочитал, и берёт только новое — лог
/// растёт по ходу игры, и перечитывать его целиком каждые 100 мс ни к чему.
pub struct LogFollow {
    path: PathBuf,
    offset: u64,
    lines: VecDeque<String>,
    pending: Vec<u8>,
}

impl LogFollow {
    pub fn new(home: &Path) -> LogFollow {
        LogFollow { path: home.join("loader").join("game.log"), offset: 0, lines: VecDeque::new(), pending: Vec::new() }
    }

    /// Последние `keep` строк: что дописалось с прошлого раза — дочитано и поднято.
    pub fn tail(&mut self, keep: usize) -> Vec<String> {
        use std::io::{Seek, SeekFrom};
        let len = std::fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0);
        if len < self.offset {
            // лог начат заново (новый запуск игры пишет с нуля)
            self.offset = 0;
            self.lines.clear();
            self.pending.clear();
        }
        if len > self.offset {
            if let Ok(mut f) = std::fs::File::open(&self.path) {
                if f.seek(SeekFrom::Start(self.offset)).is_ok() {
                    let mut buf = Vec::new();
                    if f.read_to_end(&mut buf).is_ok() {
                        self.offset += buf.len() as u64;
                        self.pending.extend(buf);
                    }
                }
            }
        }
        // целые строки — в кольцо (перевод строки не бывает серединой знака UTF-8, резать по нему
        // всегда можно); хвост без перевода — ждать следующих байтов
        if let Some(at) = self.pending.iter().rposition(|&b| b == b'\n') {
            for l in String::from_utf8_lossy(&self.pending[..=at]).lines() {
                self.lines.push_back(l.to_string());
            }
            self.pending.drain(..=at);
        }
        while self.lines.len() > 512 {
            self.lines.pop_front();
        }
        self.lines.iter().rev().take(keep).rev().cloned().collect()
    }
}

/// Сколько байт занимает папка (для экрана настроек).
pub fn dir_bytes(path: &Path) -> u64 {
    let mut out = 0u64;
    if let Ok(list) = std::fs::read_dir(path) {
        for e in list.flatten() {
            let p = e.path();
            if p.is_dir() {
                out += dir_bytes(&p);
            } else if let Ok(md) = e.metadata() {
                out += md.len();
            }
        }
    }
    out
}

/// Одна попытка всего цикла для CLI: синк (если нужен) и запуск.
pub fn run_cli(home: &Path, base: &str, wine: &str, sync_only: bool, launch_only: bool) -> Result<(), String> {
    let mut settings = Settings::load(home);
    if !wine.is_empty() {
        settings.wine = wine.to_string();
    }
    let m = if launch_only { fetch_start(base)? } else { sync(home, base, &Progress::new())? };
    if sync_only {
        println!("синхронизация: на диске {:.0} MB", dir_bytes(home) as f64 / 1048576.0);
        return Ok(());
    }
    let mut child = launch(home, &m, &settings, None)?;
    let code = child.wait().map_err(|e| e.to_string())?;
    if !code.success() {
        return Err(format!("игра ушла с кодом {}", code.code().unwrap_or(-1)));
    }
    Ok(())
}
