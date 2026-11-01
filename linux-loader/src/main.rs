//! Cursed Recode на Linux (CachyOS и прочих): нативная половина лоадера — подпись манифеста,
//! синхронизация билда и ассетов, распаковка runtime — и запуск игры (bin/CursedRecode.exe и
//! Windows-JDK из runtime.zip) через Wine. Протокол тот же «дистрибутив на GitHub» (dist/README.md):
//! start.json с ed25519-подписью, object-<sha256>, ассеты Mojang с их же CDN.
//!
//! Нативный порт самого движка сюда не входит: движок и окна остаются Windows-бинарями и работают
//! под Wine (Vulkan — через winevulkan к вашему драйверу). Этим лоадер подменяет только Windows-лоадер,
//! который под Wine запускается не у всех (docs/CACHYOS.md, «Диагностика»).

use std::collections::HashSet;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde_json::Value;
use sha1::Sha1;
use sha2::{Digest, Sha256};

/// База дистрибутива «на GitHub»: релиз с тегом `dist` этого репозитория (переключается --base).
const BASE: &str = "https://github.com/kpanics/games/releases/download/dist";
/// Публичная половина ключа манифеста — та же, что вшита в Windows-лоадер (CURSED_VERIFY_KEY CI).
const KEY: &str = "553f8e75906c6f2cee9b76f2243625ece155f07b10a8a9779af58356b061db2c";
/// Ассеты Minecraft — собственность Mojang, качаем с их CDN (как и Windows-лоадер).
const ASSETS: &str = "https://resources.download.minecraft.net";

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hex_bytes(s: &str) -> Option<Vec<u8>> {
    (s.len() % 2 == 0).then(|| (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect::<Option<Vec<u8>>>()).flatten()
}

fn sha256_file(path: &Path) -> Result<String, String> {
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

struct Entry {
    path: String,
    sha256: String,
    size: u64,
    kind: String,
}

struct Manifest {
    build: i64,
    version: String,
    launch: Value,
    files: Vec<Entry>,
}

/// Манифест, если подпись над его точным текстом — наша (как sync::verify Windows-лоадера).
fn verify_manifest(text: &str, sig: &str) -> Result<Manifest, String> {
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
        launch: v["launch"].clone(),
        files,
    })
}

/// Что скачать: путь в папке игры, ожидаемый хеш, размер и адрес.
struct Job {
    path: String,
    hash: String,
    size: u64,
    url: String,
    /// Ассет: SHA-1 (файлы билда — SHA-256).
    sha1: bool,
}

/// Файлы билда: всё, что не Windows-лоадер (его роль здесь играет эта программа).
fn plan_files(home: &Path, m: &Manifest, base: &str) -> Vec<Job> {
    let mut jobs = Vec::new();
    for f in &m.files {
        if f.kind == "loader" {
            continue;
        }
        let full = home.join(&f.path);
        let done = if f.kind == "zip" {
            zip_marker(&zip_dir(&full), &f.sha256).is_file()
        } else {
            full.is_file()
                && std::fs::metadata(&full).map(|md| md.len() == f.size).unwrap_or(false)
                && sha256_file(&full).map(|h| h == f.sha256).unwrap_or(false)
        };
        if done {
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

struct Progress {
    bytes: AtomicU64,
    files: AtomicU64,
    total_bytes: u64,
    total_files: usize,
    done: AtomicBool,
}

impl Progress {
    fn quiet() -> Progress {
        Progress { bytes: AtomicU64::new(0), files: AtomicU64::new(0), total_bytes: 0, total_files: 0, done: AtomicBool::new(false) }
    }
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
fn unpack_zips(home: &Path, m: &Manifest) -> Result<(), String> {
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

fn dir_bytes(path: &Path) -> u64 {
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

/// `${root}` в аргументах билда: игра запускается из папки билда (cwd), поэтому корень — текущая папка.
fn fill(s: &str) -> String {
    s.replace("${root}", ".")
}

/// Запуск игры: то же, что делает loader/game.rs — host (bin/CursedRecode.exe, Java внутри него) с
/// JDK билда, флагами и classpath из манифеста, — только под Wine. Разделитель classpath — «;»:
/// его разбирает Windows-JVM из runtime.zip.
fn launch(home: &Path, m: &Manifest, wine: &str, memory: u32) -> Result<(), String> {
    let launch = &m.launch;
    let host = launch["host"].as_str().ok_or("launch: host")?;
    if !home.join(host).is_file() {
        return Err(format!("{host}: нет такого файла — сначала синхронизация"));
    }
    let mut cmd = Command::new(wine);
    cmd.current_dir(home).arg(host);
    if let Some(jdk) = launch["jdk"].as_str() {
        if home.join(jdk).join("bin").is_dir() {
            cmd.arg("--jdk").arg(jdk);
        }
    }
    let least = launch["memory"]["least"].as_u64().unwrap_or(2048) as u32;
    cmd.arg(format!("-Xmx{}m", memory.max(least)));
    for a in launch["jvm"].as_array().into_iter().flatten().filter_map(|a| a.as_str()) {
        cmd.arg(fill(a));
    }
    let cp = launch["classpath"]
        .as_array()
        .ok_or("launch: classpath")?
        .iter()
        .filter_map(|p| p.as_str())
        .collect::<Vec<&str>>()
        .join(";");
    cmd.arg("-cp").arg(cp).arg(launch["main"].as_str().ok_or("launch: main")?);
    for a in launch["game"].as_array().into_iter().flatten().filter_map(|a| a.as_str()) {
        cmd.arg(fill(a));
    }
    cmd.arg("--username").arg("Player");
    cmd.env("CURSED_BUILD", m.build.to_string());
    println!("запуск игры ({wine})…");
    let code = cmd.status().map_err(|e| format!("wine не запускается: {e} (пакет wine установлен?)"))?;
    if !code.success() {
        return Err(format!("игра ушла с кодом {}", code.code().unwrap_or(-1)));
    }
    Ok(())
}

fn usage() {
    println!(
        "cursed-linux-loader — нативная половина лоадера Cursed Recode на Linux\n\
         \n\
         запуск: cursed-linux-loader [флаги]\n\
         --home <папка>   где живёт игра (по умолчанию ~/Cursed Recode)\n\
         --sync-only      только скачать и проверить, не запускать\n\
         --launch-only    не синхронизировать, сразу запустить\n\
         --wine <путь>    какой wine использовать (по умолчанию $WINE или wine)\n\
         --base <url>     база дистрибутива (по умолчанию — релиз `dist` этого репозитория)\n\
         \n\
         Игра (bin/CursedRecode.exe и JDK из runtime.zip) — Windows-бинарни и идут под Wine;\n\
         Vulkan движка Wine проводит к вашему драйверу (winevulkan)."
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        usage();
        return;
    }
    let get = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let home = get("--home")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join("Cursed Recode"));
    let base = get("--base").unwrap_or_else(|| BASE.to_string());
    let wine = get("--wine").or_else(|| std::env::var("WINE").ok()).unwrap_or_else(|| "wine".to_string());
    let sync_only = args.iter().any(|a| a == "--sync-only");
    let launch_only = args.iter().any(|a| a == "--launch-only");

    println!("Cursed Recode на Linux — папка {}", home.display());
    if let Err(e) = run(&home, &base, &wine, sync_only, launch_only) {
        eprintln!("не вышло: {e}");
        std::process::exit(1);
    }
}

fn run(home: &Path, base: &str, wine: &str, sync_only: bool, launch_only: bool) -> Result<(), String> {
    let tmp = std::env::temp_dir().join("cursed-linux-start.json");
    curl(&format!("{base}/start.json"), &tmp)?;
    let text = std::fs::read_to_string(&tmp).map_err(|e| format!("start.json: {e}"))?;
    let start: Value = serde_json::from_str(&text).map_err(|e| format!("start.json: {e}"))?;
    let m = verify_manifest(start["manifest"].as_str().unwrap_or(""), start["sig"].as_str().unwrap_or(""))?;
    println!("билд {} ({})", m.build, m.version);

    if !launch_only {
        std::fs::create_dir_all(home).map_err(|e| e.to_string())?;
        let t0 = Instant::now();
        let mut jobs = plan_files(home, &m, base);
        // индекс ассетов — первым: по нему план ассетов (как sync::plan Windows-лоадера)
        if let Some(idx) = m.files.iter().find(|f| f.kind == "assets-index") {
            if !home.join(&idx.path).is_file() {
                if let Some(at) = jobs.iter().position(|j| j.path == idx.path) {
                    let j = jobs.remove(at);
                    fetch_one(home, &j, &Progress::quiet())?;
                }
            }
        }
        jobs.extend(asset_jobs(home, &m));
        if jobs.is_empty() {
            println!("всё уже на месте");
        } else {
            let p = Arc::new(Progress {
                bytes: AtomicU64::new(0),
                files: AtomicU64::new(0),
                total_bytes: jobs.iter().map(|j| j.size).sum(),
                total_files: jobs.len(),
                done: AtomicBool::new(false),
            });
            let watcher = {
                let p = p.clone();
                std::thread::spawn(move || loop {
                    std::thread::sleep(Duration::from_secs(2));
                    if p.done.load(Ordering::Relaxed) {
                        break;
                    }
                    println!(
                        "  {}/{} файлов, {:.0}/{:.0} MB",
                        p.files.load(Ordering::Relaxed),
                        p.total_files,
                        p.bytes.load(Ordering::Relaxed) as f64 / 1048576.0,
                        p.total_bytes as f64 / 1048576.0
                    );
                })
            };
            let r = sync_all(home, jobs, &p);
            p.done.store(true, Ordering::Relaxed);
            let _ = watcher.join();
            r?;
            unpack_zips(home, &m)?;
            println!("синхронизация: {:.0} с, на диске {:.0} MB", t0.elapsed().as_secs(), dir_bytes(home) as f64 / 1048576.0);
        }
    }
    if sync_only {
        return Ok(());
    }
    launch(home, &m, wine, 4096)
}
