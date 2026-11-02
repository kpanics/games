//! Окно лоадера на Linux: слева узкая полоса вкладок (Играть, Новости, Аккаунт, Настройки,
//! Ссылки), справа содержимое — как у настольных приложений. Тексты ru/en, шрифты Geist — те же
//! файлы, что у Windows-лоадера; растеризация FreeType (fontdue путал регистры кириллицы на
//! кривых сегментах cmap, FreeType читает её верно и хинтит глифы).
//!
//! Аккаунты — через API сайта Cursed (соседний репозиторий recode-site-and-backend, docs/API.md):
//! регистрация (`POST /api/register`), вход лоадера токеном (`POST /api/token`, без Origin),
//! проверка сессии (`GET /api/me`). Логин вошедшего идёт в игру как имя игрока.
//!
//! Рендер — CPU-композитинг (winit + softbuffer + FreeType) и он не работает зря: фон готовится
//! один раз на размер окна, глифы кэшируются, статичные вкладки перерисовываются только по
//! событиям. HiDPI: раскладка в логических пикселях, растеризация — в физических.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Child;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::sync::{mpsc, Arc};
use std::time::Instant;

use cursed_linux_loader as lib;
use freetype::face::LoadFlag;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowAttributes, WindowId};

// ---------------------------------------------------------------------------------------------- шрифты

static REG: &[u8] = include_bytes!("../../assets/fonts/geist_400.ttf");
static MED: &[u8] = include_bytes!("../../assets/fonts/geist_500.ttf");
static SEMI: &[u8] = include_bytes!("../../assets/fonts/geist_600.ttf");
static BOLD: &[u8] = include_bytes!("../../assets/fonts/geist_700.ttf");
static MONO: &[u8] = include_bytes!("../../assets/fonts/geistmono_500.ttf");

/// Сырой RGBA 64×64 — тот же квадрат, что в recode.ico лоадера.
static ICON: &[u8] = include_bytes!("../../assets/icon.rgba");

/// Лицо шрифта: обычный, средний, полужирный, жирный, моно.
#[derive(Copy, Clone)]
#[allow(dead_code)] // Bold — в наборе Geist и пригодится, лица не выключаются из набора
enum F {
    Reg,
    Med,
    Semi,
    Bold,
    Mono,
}

/// Растеризованный глиф: шаг перена в логических пикселях, растр — в физических.
struct Glyph {
    adv: f32,
    left: i32,
    top: i32,
    w: usize,
    h: usize,
    cov: Vec<u8>,
}

/// Пять лиц Geist с кэшем растра: глиф растеризуется при первом обращении (FreeType, в
/// физическом размере текущего масштаба экрана) и дальше берётся готовым.
struct Text {
    faces: [freetype::Face; 5],
    k: Cell<f32>,
    cache: RefCell<HashMap<(u8, u16, char), Rc<Glyph>>>,
}

impl Text {
    fn new() -> Text {
        let lib = freetype::Library::init().expect("FreeType");
        let mk = |b: &[u8]| lib.new_memory_face(b.to_vec(), 0).expect("шрифт Geist");
        let faces = [mk(REG), mk(MED), mk(SEMI), mk(BOLD), mk(MONO)];
        // лица держат свою ссылку на библиотеку (FT_Reference_Library) — сама она больше не нужна
        Text { faces, k: Cell::new(1.0), cache: RefCell::new(HashMap::new()) }
    }

    fn set_k(&self, k: f32) {
        if (self.k.get() - k).abs() > 1e-6 {
            self.k.set(k);
            self.cache.borrow_mut().clear();
        }
    }

    fn glyph(&self, f: F, size: f32, ch: char) -> Rc<Glyph> {
        let key = (f as u8, (size * 4.0).round() as u16, ch);
        let hit = self.cache.borrow().get(&key).cloned();
        if let Some(g) = hit {
            return g;
        }
        let k = self.k.get();
        let px = ((size * k).round() as u32).max(1);
        let face = &self.faces[f as usize];
        let g = match face.set_pixel_sizes(px, px).and_then(|_| face.load_char(ch as usize, LoadFlag::RENDER)) {
            Ok(()) => {
                let slot = face.glyph();
                let bm = slot.bitmap();
                let (w, rows, pitch) = (bm.width() as usize, bm.rows() as usize, bm.pitch().max(0) as usize);
                let mut cov = vec![0u8; w * rows];
                let buf = bm.buffer();
                for y in 0..rows {
                    for x in 0..w {
                        cov[y * w + x] = buf.get(y * pitch + x).copied().unwrap_or(0);
                    }
                }
                Rc::new(Glyph {
                    adv: slot.advance().x as f32 / 64.0 / k,
                    left: slot.bitmap_left(),
                    top: slot.bitmap_top(),
                    w,
                    h: rows,
                    cov,
                })
            }
            Err(_) => Rc::new(Glyph { adv: size * 0.6, left: 0, top: 0, w: 0, h: 0, cov: Vec::new() }),
        };
        self.cache.borrow_mut().insert(key, g.clone());
        g
    }

    fn width(&self, f: F, size: f32, text: &str) -> f32 {
        text.chars().map(|ch| self.glyph(f, size, ch).adv).sum()
    }

    fn ascent(&self, f: F, size: f32) -> f32 {
        self.glyph(f, size, 'H').top as f32 / self.k.get()
    }

    fn wrap(&self, f: F, size: f32, text: &str, max_w: f32) -> Vec<String> {
        let mut lines = Vec::new();
        for raw in text.split('\n') {
            let mut line = String::new();
            for word in raw.split(' ') {
                let try_line = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
                if self.width(f, size, &try_line) > max_w && !line.is_empty() {
                    lines.push(line);
                    line = word.to_string();
                } else {
                    line = try_line;
                }
            }
            lines.push(line);
        }
        lines
    }

    fn draw(&self, can: &mut Canvas, f: F, size: f32, x: f32, base: f32, c: u32, text: &str) -> f32 {
        let k = self.k.get();
        let mut pen = x;
        for ch in text.chars() {
            let g = self.glyph(f, size, ch);
            let gx = pen * k + g.left as f32;
            let gy = base * k - g.top as f32;
            let w = g.w.max(1);
            for (i, a) in g.cov.iter().enumerate() {
                if *a == 0 {
                    continue;
                }
                can.blend(gx as i64 + (i % w) as i64, gy as i64 + (i / w) as i64, c, *a);
            }
            pen += g.adv;
        }
        pen - x
    }
}

// ---------------------------------------------------------------------------------------------- палитра

const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    ((r as u32) << 16) | ((g as u32) << 8) | b as u32
}
const BG_TOP: u32 = rgb(10, 12, 17);
const BG_BOT: u32 = rgb(17, 20, 29);
const RAIL_BG: u32 = rgb(12, 14, 19);
const PANEL: u32 = rgb(23, 26, 34);
const PANEL_EDGE: u32 = rgb(44, 49, 63);
const TEXT: u32 = rgb(232, 234, 240);
const DIM: u32 = rgb(148, 155, 170);
const MAIN_BG: u32 = rgb(232, 234, 240);
const MAIN_TEXT: u32 = rgb(16, 19, 26);
const TRACK: u32 = rgb(38, 43, 54);
const RED: u32 = rgb(255, 122, 122);
const GREEN: u32 = rgb(126, 200, 145);

fn lighten(c: u32, k: f32) -> u32 {
    let ch = |v: u32| ((v as f32 * (1.0 - k) + 255.0 * k).clamp(0.0, 255.0)) as u32;
    rgb(ch((c >> 16) & 255) as u8, ch((c >> 8) & 255) as u8, ch(c & 255) as u8)
}

// ---------------------------------------------------------------------------------------------- холст

/// Кадр в физических пикселях; прямоугольники задаются логическими и умножаются на масштаб.
struct Canvas<'a> {
    w: usize,
    h: usize,
    k: f32,
    buf: &'a mut [u32],
}

impl<'a> Canvas<'a> {
    fn blend(&mut self, x: i64, y: i64, c: u32, a: u8) {
        if x < 0 || y < 0 || x >= self.w as i64 || y >= self.h as i64 || a == 0 {
            return;
        }
        let at = y as usize * self.w + x as usize;
        if a == 255 {
            self.buf[at] = c;
            return;
        }
        let dst = self.buf[at];
        let k = a as f32 / 255.0;
        let mix = |s: u32, d: u32| (s as f32 * k + d as f32 * (1.0 - k) + 0.5) as u32;
        self.buf[at] = rgb(
            mix((c >> 16) & 255, (dst >> 16) & 255).min(255) as u8,
            mix((c >> 8) & 255, (dst >> 8) & 255).min(255) as u8,
            mix(c & 255, dst & 255).min(255) as u8,
        );
    }

    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, c: u32) {
        let k = self.k;
        let x0 = (x * k).ceil() as i64;
        let y0 = (y * k).ceil() as i64;
        let x1 = ((x + w) * k).floor() as i64;
        let y1 = ((y + h) * k).floor() as i64;
        for yy in y0..y1 {
            for xx in x0..x1 {
                self.blend(xx, yy, c, 255);
            }
        }
    }

    /// Прямоугольник со скруглёнными углами: центральные полосы + четвертинки круга по строкам.
    fn round(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, c: u32) {
        let r = r.min(w / 2.0).min(h / 2.0);
        if r <= 0.5 {
            self.rect(x, y, w, h, c);
            return;
        }
        self.rect(x + r, y, w - 2.0 * r, h, c);
        self.rect(x, y + r, w, h - 2.0 * r, c);
        let rr = r * r;
        let rows = r.ceil() as usize;
        for i in 0..rows {
            let dy = i as f32 + 0.5 - r;
            let span = (rr - dy * dy).max(0.0).sqrt();
            if span <= 0.0 {
                continue;
            }
            let ytop = y + i as f32;
            let ybot = y + h - i as f32 - 1.0;
            self.rect(x + r - span, ytop, span, 1.0, c);
            self.rect(x + w - r, ytop, span, 1.0, c);
            self.rect(x + r - span, ybot, span, 1.0, c);
            self.rect(x + w - r, ybot, span, 1.0, c);
        }
    }

    /// Круг (заливка), в логических координатах.
    fn disc(&mut self, cx: f32, cy: f32, r: f32, c: u32) {
        let k = self.k;
        let (cx, cy, r) = (cx * k, cy * k, r * k);
        let rr = r * r;
        let y0 = (cy - r).ceil() as i64;
        let y1 = (cy + r).ceil() as i64;
        for y in y0..y1 {
            let dy = y as f32 + 0.5 - cy;
            let half = (rr - dy * dy).max(0.0).sqrt();
            if half <= 0.0 {
                continue;
            }
            let x0 = (cx - half).ceil() as i64;
            let x1 = (cx + half).ceil() as i64;
            for x in x0..x1 {
                self.blend(x, y, c, 255);
            }
        }
    }

    /// Затемнение всего кадра (полупрозрачная вуаль под панелью поверх вкладок).
    fn veil(&mut self, a: u8) {
        let black = rgb(0, 0, 0);
        for y in 0..self.h as i64 {
            for x in 0..self.w as i64 {
                self.blend(x, y, black, a);
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------- значки вкладок

/// Значки вкладок — простые геометрические, 22×22 в логических пикселях; bg — цвет подложки,
/// которым «дырявятся» контуры (полоса вкладок плоская, поэтому так можно).

fn icon_play(can: &mut Canvas, x: f32, y: f32, s: f32, c: u32) {
    let n = s as i64;
    for i in 0..n {
        let half = i.min(n - 1 - i).max(0) as f32;
        can.rect(x, y + i as f32, 2.0 * half + 0.6, 1.0, c);
    }
}

fn icon_news(can: &mut Canvas, x: f32, y: f32, s: f32, c: u32, bg: u32) {
    can.round(x, y, s, s, 4.0, c);
    can.round(x + 1.6, y + 1.6, s - 3.2, s - 3.2, 3.0, bg);
    for i in 0..3 {
        let w = s - 8.0 - if i == 2 { s * 0.25 } else { 0.0 };
        can.rect(x + 4.0, y + 4.6 + i as f32 * (s - 10.0) / 3.0, w, 1.6, c);
    }
}

fn icon_person(can: &mut Canvas, x: f32, y: f32, s: f32, c: u32) {
    can.disc(x + s / 2.0, y + s * 0.34, s * 0.20, c);
    can.round(x + s * 0.14, y + s * 0.60, s * 0.72, s * 0.32, s * 0.16, c);
}

fn icon_gear(can: &mut Canvas, x: f32, y: f32, s: f32, c: u32, bg: u32) {
    let (cx, cy, r) = (x + s / 2.0, y + s / 2.0, s * 0.30);
    can.disc(cx, cy, r, c);
    can.disc(cx, cy, r * 0.45, bg);
    for a in 0..8 {
        let t = a as f32 * std::f32::consts::TAU / 8.0;
        can.disc(cx + r * 1.3 * t.cos(), cy + r * 1.3 * t.sin(), s * 0.09, c);
    }
}

fn icon_link(can: &mut Canvas, x: f32, y: f32, s: f32, c: u32, bg: u32) {
    can.round(x, y + s * 0.28, s * 0.64, s * 0.44, s * 0.22, c);
    can.round(x + s * 0.36, y + s * 0.28, s * 0.64, s * 0.44, s * 0.22, c);
    can.round(x + s * 0.10, y + s * 0.42, s * 0.44, s * 0.16, s * 0.08, bg);
    can.round(x + s * 0.46, y + s * 0.42, s * 0.44, s * 0.16, s * 0.08, bg);
}

fn icon_power(can: &mut Canvas, x: f32, y: f32, s: f32, c: u32, bg: u32) {
    let (cx, cy, r) = (x + s / 2.0, y + s * 0.56, s * 0.32);
    can.disc(cx, cy, r, c);
    can.disc(cx, cy, r * 0.62, bg);
    can.rect(cx - 0.9, y + s * 0.04, 1.8, s * 0.48, c);
}

// ---------------------------------------------------------------------------------------------- состояние

#[derive(Copy, Clone, PartialEq)]
enum Tab {
    Play,
    News,
    Account,
    Settings,
    Links,
}

impl Tab {
    fn index(self) -> usize {
        match self {
            Tab::Play => 0,
            Tab::News => 1,
            Tab::Account => 2,
            Tab::Settings => 3,
            Tab::Links => 4,
        }
    }
}

/// Поле формы аккаунта, которое в фокусе (куда идёт ввод).
#[derive(Copy, Clone, PartialEq)]
enum Field {
    SignInLogin,
    SignInPass,
    RegLogin,
    RegEmail,
    RegPass,
}

/// Что редактируют в наложенном поле ввода.
#[derive(Copy, Clone, PartialEq)]
enum EditKind {
    Folder,
    Wine,
    Api,
}

#[derive(Clone)]
struct FieldEdit {
    kind: EditKind,
    buf: String,
}

#[derive(Clone, PartialEq)]
enum Hit {
    Action(String),
    Slider,
    Focus(Field),
}

#[derive(Clone)]
struct Rect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl Rect {
    fn has(&self, x: f32, y: f32) -> bool {
        x >= self.x && x <= self.x + self.w && y >= self.y && y <= self.y + self.h
    }
}

enum ToUi {
    Started(Arc<lib::Progress>),
    SyncDone(Result<lib::Manifest, String>),
    Launched(Result<Child, String>),
    SignIn(Result<lib::Auth, String>),
    Register(Result<lib::Auth, String>),
    Me(Result<lib::Me, String>),
}

/// Вся картина окна: вкладка, настройки, прогресс, кнопки последнего кадра.
struct Ui {
    tx: Sender<ToUi>,
    rx: Receiver<ToUi>,
    txtext: Text,
    tab: Tab,
    section: usize,
    message: Option<(String, String)>,
    edit: Option<FieldEdit>,
    settings: lib::Settings,
    home: PathBuf,
    base: String,
    manifest: Option<lib::Manifest>,
    progress: Option<Arc<lib::Progress>>,
    child: Option<Child>,
    sync_at: Option<Instant>,
    auto_launch: bool,
    syncing: bool,
    sign_login: String,
    sign_pass: String,
    reg_login: String,
    reg_email: String,
    reg_pass: String,
    focus: Option<Field>,
    auth_busy: bool,
    /// Строка под формами: (текст, ошибка ли).
    form_note: Option<(String, bool)>,
    mouse: (f32, f32),
    hits: Vec<(Rect, Hit)>,
    tick: u64,
    drag: bool,
    /// Масштаб экрана (физические пиксели на логический).
    k: f32,
    /// Готовый фон: градиент пересчитывается только при смене размера окна.
    bg: Vec<u32>,
    bg_w: usize,
    bg_h: usize,
    /// Размер папки — считается один раз на заход в раздел, а не каждым кадром.
    folder_bytes: Option<u64>,
    log: lib::LogFollow,
}

const RAIL_W: f32 = 82.0;
const CPAD: f32 = 48.0;

impl Ui {
    fn t(&self, ru: &str, en: &str) -> String {
        if self.settings.lang == "en" { en.into() } else { ru.into() }
    }

    fn chosen(&self) -> bool {
        self.home.join("loader").join(".chosen").is_file()
    }

    fn choose(&self) {
        let _ = std::fs::create_dir_all(self.home.join("loader"));
        let _ = std::fs::write(self.home.join("loader").join(".chosen"), b"1");
    }

    fn message(&mut self, title: String, text: String) {
        self.message = Some((title, text));
    }

    fn signed_in(&self) -> bool {
        !self.settings.account.trim().is_empty()
    }

    fn account_name(&self) -> String {
        let n = self.settings.account.trim();
        if n.is_empty() { self.t("Гость", "Guest").into() } else { n.into() }
    }

    /// Синк в фоне; если просили — сразу запуск игры. Второй поверх идущего не стартует.
    fn begin_sync(&mut self, auto_launch: bool) {
        if self.syncing {
            return;
        }
        self.syncing = true;
        self.auto_launch = auto_launch;
        self.sync_at = Some(Instant::now());
        self.tab = Tab::Play;
        let tx = self.tx.clone();
        let home = self.home.clone();
        let base = self.base.clone();
        let settings = self.settings.clone();
        std::thread::spawn(move || {
            let p = lib::Progress::new();
            let _ = tx.send(ToUi::Started(p.clone()));
            match lib::sync(&home, &base, &p) {
                Ok(m) => {
                    if auto_launch {
                        let child = lib::launch(&home, &m, &settings, None);
                        let _ = tx.send(ToUi::SyncDone(Ok(m)));
                        let _ = tx.send(ToUi::Launched(child));
                    } else {
                        let _ = tx.send(ToUi::SyncDone(Ok(m)));
                    }
                }
                Err(e) => {
                    p.phase.store(4, Ordering::Relaxed);
                    let _ = tx.send(ToUi::SyncDone(Err(e)));
                }
            }
        });
    }

    fn do_sign_in(&mut self) {
        if self.auth_busy || self.sign_login.trim().is_empty() || self.sign_pass.is_empty() {
            return;
        }
        self.auth_busy = true;
        self.form_note = Some((self.t("Проверяю…", "Checking…"), false));
        let tx = self.tx.clone();
        let api = self.settings.api_base();
        let login = self.sign_login.trim().to_string();
        let pass = self.sign_pass.clone();
        std::thread::spawn(move || {
            let _ = tx.send(ToUi::SignIn(lib::sign_in(&api, &login, &pass)));
        });
    }

    fn do_register(&mut self) {
        if self.auth_busy {
            return;
        }
        let login = self.reg_login.trim().to_string();
        let email = self.reg_email.trim().to_string();
        let pass = self.reg_pass.clone();
        // правила те же, что проверяет сервер (docs/API.md) — скажем раньше, без поездки
        let bad = if !(3..=16).contains(&login.chars().count()) || !login.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
            Some(self.t("Логин: 3–16 знаков, строчная латиница, цифры и _", "Login: 3–16 characters, lowercase a–z, 0–9 and _"))
        } else if !email.contains('@') || email.len() < 5 {
            Some(self.t("Почта не похожа на настоящую", "The e-mail does not look like one"))
        } else if pass.chars().count() < 10 {
            Some(self.t("Пароль: минимум 10 знаков", "Password: at least 10 characters"))
        } else {
            None
        };
        if let Some(e) = bad {
            self.form_note = Some((e, true));
            return;
        }
        self.auth_busy = true;
        self.form_note = Some((self.t("Создаю аккаунт…", "Making the account…"), false));
        let tx = self.tx.clone();
        let api = self.settings.api_base();
        std::thread::spawn(move || {
            let _ = tx.send(ToUi::Register(lib::register(&api, &login, &email, &pass)));
        });
    }

    fn sign_out(&mut self) {
        self.settings.token.clear();
        self.settings.account.clear();
        self.settings.uid = 0;
        self.settings.save(&self.home);
        self.sign_pass.clear();
        self.reg_pass.clear();
        self.form_note = Some((self.t("Вышел.", "Signed out."), false));
    }

    /// Раз в 100 мс: что сказали фоновые потоки, жива ли игра. Ответ — было ли что-то, что видно.
    fn on_tick(&mut self) -> bool {
        let mut changed = false;
        loop {
            match self.rx.try_recv() {
                Ok(ToUi::Started(p)) => {
                    self.progress = Some(p);
                    changed = true;
                }
                Ok(ToUi::SyncDone(r)) => {
                    self.syncing = false;
                    match r {
                        Ok(m) => self.manifest = Some(m),
                        Err(e) => {
                            self.progress = None;
                            let title = self.t("Синхронизация не вышла", "The sync did not finish");
                            self.message(title, e);
                        }
                    }
                    changed = true;
                }
                Ok(ToUi::Launched(r)) => match r {
                    Ok(c) => {
                        self.child = Some(c);
                        self.log = lib::LogFollow::new(&self.home);
                        self.tab = Tab::Play;
                    }
                    Err(e) => {
                        let title = self.t("Игра не запустилась", "The game did not start");
                        self.message(title, e);
                    }
                },
                Ok(ToUi::SignIn(r)) => {
                    self.auth_busy = false;
                    match r {
                        Ok(a) => {
                            self.settings.token = a.token;
                            self.settings.uid = a.uid;
                            self.settings.account = a.login.clone();
                            self.settings.save(&self.home);
                            self.sign_pass.clear();
                            self.reg_pass.clear();
                            self.focus = None;
                            let hi = self.t("Вошёл как", "Signed in as");
                            self.form_note = Some((format!("{hi} {}.", a.login), false));
                        }
                        Err(e) => self.form_note = Some((e, true)),
                    }
                    changed = true;
                }
                Ok(ToUi::Register(r)) => {
                    self.auth_busy = false;
                    match r {
                        Ok(a) => {
                            self.settings.token = a.token;
                            self.settings.uid = a.uid;
                            self.settings.account = a.login.clone();
                            self.settings.save(&self.home);
                            self.sign_pass.clear();
                            self.reg_pass.clear();
                            self.focus = None;
                            let made = self.t("Аккаунт создан:", "The account is made:");
                            let name = self.t("в игре ты", "in the game you are");
                            self.form_note = Some((format!("{made} {} — {name} {}.", a.login, a.login), false));
                        }
                        Err(e) => self.form_note = Some((e, true)),
                    }
                    changed = true;
                }
                Ok(ToUi::Me(r)) => match r {
                    Ok(me) => {
                        self.settings.uid = me.uid;
                        self.settings.account = me.login;
                        self.settings.save(&self.home);
                        changed = true;
                    }
                    Err(e) => {
                        if e.contains("сессия кончилась") {
                            self.settings.token.clear();
                            self.settings.account.clear();
                            self.settings.uid = 0;
                            self.settings.save(&self.home);
                            changed = true;
                        }
                    }
                },
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
        let exited = match self.child.as_mut() {
            Some(c) => c.try_wait().ok().flatten(),
            None => None,
        };
        if let Some(status) = exited {
            self.child = None;
            if !status.success() {
                let tail = self.log.tail(8).join("\n");
                let title = self.t("Игра закрылась с ошибкой", "The game closed with an error");
                let code = self.t("Код выхода:", "Exit code:");
                self.message(title, format!("{code} {}.\n\n{}", status.code().unwrap_or(-1), tail));
            }
            changed = true;
        }
        changed
    }

    fn click(&mut self, mx: f32, my: f32) {
        // поверху: что нарисовано последним, то и ловит клик (вуали поверх вкладок)
        for (r, h) in self.hits.iter().rev().cloned().collect::<Vec<_>>() {
            if !r.has(mx, my) {
                continue;
            }
            match h {
                Hit::Action(a) => self.on_action(&a),
                Hit::Focus(f) => self.focus = Some(f),
                Hit::Slider => {
                    self.drag = true;
                    self.slide(mx, &r);
                }
            }
            return;
        }
    }

    fn slide(&mut self, mx: f32, r: &Rect) {
        let k = ((mx - r.x) / r.w).clamp(0.0, 1.0);
        self.settings.memory = ((2048.0 + k * (16384.0 - 2048.0)) / 256.0).round() as u32 * 256;
        self.settings.save(&self.home);
    }

    fn on_action(&mut self, action: &str) {
        match action {
            "quit" => std::process::exit(0),
            "tab-0" => self.tab = Tab::Play,
            "tab-1" => self.tab = Tab::News,
            "tab-2" => self.tab = Tab::Account,
            "tab-3" => self.tab = Tab::Settings,
            "tab-4" => self.tab = Tab::Links,
            "install" => {
                self.choose();
                self.begin_sync(false);
            }
            "browse" | "change-folder" => {
                self.edit = Some(FieldEdit { kind: EditKind::Folder, buf: self.home.to_string_lossy().into_owned() });
            }
            "wine-edit" => {
                self.edit = Some(FieldEdit { kind: EditKind::Wine, buf: self.settings.wine_binary() });
            }
            "api-edit" => {
                self.edit = Some(FieldEdit { kind: EditKind::Api, buf: self.settings.api_base() });
            }
            "edit-ok" => {
                if let Some(e) = self.edit.take() {
                    let buf = e.buf.trim().to_string();
                    match e.kind {
                        EditKind::Folder => {
                            if !buf.is_empty() {
                                self.home = PathBuf::from(&buf);
                                self.settings.home = buf;
                                self.settings.save(&self.home);
                                self.log = lib::LogFollow::new(&self.home);
                            }
                        }
                        EditKind::Wine => {
                            self.settings.wine = buf;
                            self.settings.save(&self.home);
                        }
                        EditKind::Api => {
                            self.settings.api = buf;
                            self.settings.save(&self.home);
                        }
                    }
                }
            }
            "edit-cancel" => self.edit = None,
            "message-ok" => self.message = None,
            "play" => {
                if self.child.is_some() {
                    let title = self.t("Игра уже запущена", "The game is already running");
                    let text = self.t("Одна игра за раз: дождись, пока эта закроется.", "One game at a time: wait for this one to close.");
                    self.message(title, text);
                } else {
                    self.choose();
                    self.begin_sync(true);
                }
            }
            "check-files" => {
                self.folder_bytes = None;
                self.begin_sync(false);
            }
            "sign-in" => self.do_sign_in(),
            "register" => self.do_register(),
            "sign-out" => self.sign_out(),
            "lang" => {
                self.settings.lang = if self.settings.lang == "ru" { "en".into() } else { "ru".into() };
                self.settings.save(&self.home);
            }
            "sec0" => self.section = 0,
            "sec1" => self.section = 1,
            "sec2" => {
                if self.folder_bytes.is_none() {
                    self.folder_bytes = Some(lib::dir_bytes(&self.home));
                }
                self.section = 2;
            }
            "open-folder" => {
                let _ = std::process::Command::new("xdg-open").arg(&self.home).spawn();
            }
            "site" => {
                let _ = std::process::Command::new("xdg-open").arg("https://github.com/kpanics/games").spawn();
            }
            "cursed-site" => {
                let _ = std::process::Command::new("xdg-open").arg("https://cursedclient.xyz").spawn();
            }
            "releases" => {
                let _ = std::process::Command::new("xdg-open").arg("https://github.com/kpanics/games/releases/tag/dist").spawn();
            }
            "docs" => {
                let _ = std::process::Command::new("xdg-open").arg("https://github.com/kpanics/games/blob/main/docs/CACHYOS.md").spawn();
            }
            _ => {}
        }
    }

    // -------------------------------------------------------------------------------------------- отрисовка

    fn button(&mut self, can: &mut Canvas, x: f32, y: f32, w: f32, h: f32, size: f32, label: &str, action: &str, main: bool) -> f32 {
        let hovered = self.mouse.0 >= x && self.mouse.0 <= x + w && self.mouse.1 >= y && self.mouse.1 <= y + h;
        let f = if main { F::Semi } else { F::Reg };
        let tw = self.txtext.width(f, size, label);
        let bg = if main {
            if hovered { lighten(MAIN_BG, 0.08) } else { MAIN_BG }
        } else if hovered {
            lighten(PANEL, 0.10)
        } else {
            PANEL
        };
        let r = (h / 4.6).min(12.0);
        if main {
            can.round(x, y, w, h, r, bg);
        } else {
            can.round(x, y, w, h, r, PANEL_EDGE);
            can.round(x + 1.0, y + 1.0, w - 2.0, h - 2.0, (r - 1.0).max(1.0), bg);
        }
        let asc = self.txtext.ascent(f, size);
        self.txtext.draw(can, f, size, x + (w - tw) / 2.0, y + h / 2.0 + asc / 2.0 - 1.0, if main { MAIN_TEXT } else { TEXT }, label);
        self.hits.push((Rect { x, y, w, h }, Hit::Action(action.to_string())));
        y + h + 10.0
    }

    /// Поле формы: подпись, рамка, значение (пароль — звёздочками), каретка в фокусе.
    fn field_box(&mut self, can: &mut Canvas, x: f32, y: f32, w: f32, id: Field, label: &str, secret: bool) -> f32 {
        self.txtext.draw(can, F::Reg, 12.5, x, y + 10.0, DIM, label);
        let by = y + 20.0;
        let on = self.focus == Some(id);
        let edge = if on { rgb(96, 108, 128) } else { PANEL_EDGE };
        can.round(x, by, w, 40.0, 9.0, edge);
        can.round(x + 1.2, by + 1.2, w - 2.4, 37.6, 8.0, PANEL);
        let value = if secret { "*".repeat(self.field_text(id).chars().count()) } else { self.field_text(id) };
        let shown: String = if value.chars().count() > 46 {
            format!("…{}", value.chars().skip(value.chars().count() - 45).collect::<String>())
        } else {
            value
        };
        let caret = if on && self.tick % 10 < 5 { "|" } else { "" };
        self.txtext.draw(can, F::Mono, 14.0, x + 14.0, by + 25.0, TEXT, &format!("{shown}{caret}"));
        self.hits.push((Rect { x, y: by, w, h: 40.0 }, Hit::Focus(id)));
        by + 40.0 + 14.0
    }

    fn field_text(&self, id: Field) -> String {
        match id {
            Field::SignInLogin => self.sign_login.clone(),
            Field::SignInPass => self.sign_pass.clone(),
            Field::RegLogin => self.reg_login.clone(),
            Field::RegEmail => self.reg_email.clone(),
            Field::RegPass => self.reg_pass.clone(),
        }
    }

    fn field_put(&mut self, id: Field, ch: Option<char>) {
        let slot = match id {
            Field::SignInLogin => &mut self.sign_login,
            Field::SignInPass => &mut self.sign_pass,
            Field::RegLogin => &mut self.reg_login,
            Field::RegEmail => &mut self.reg_email,
            Field::RegPass => &mut self.reg_pass,
        };
        match ch {
            Some(c) => slot.push(c),
            None => {
                slot.pop();
            }
        }
    }

    /// Полоса вкладок слева: значки с подписями, внизу — выключение.
    fn rail(&mut self, can: &mut Canvas, h: f32) {
        can.rect(0.0, 0.0, RAIL_W, h, RAIL_BG);
        let labels = [
            self.t("Играть", "Play"),
            self.t("Новости", "News"),
            self.t("Аккаунт", "Account"),
            self.t("Настройки", "Settings"),
            self.t("Ссылки", "Links"),
        ];
        let top = 18.0;
        let step = 64.0;
        for (i, label) in labels.iter().enumerate() {
            let y = top + i as f32 * step;
            let on = self.tab.index() == i;
            if on {
                can.round(9.0, y, RAIL_W - 18.0, 54.0, 12.0, PANEL);
            }
            let ic = RAIL_W / 2.0 - 11.0;
            let c = if on { TEXT } else { DIM };
            match i {
                0 => icon_play(can, ic + 1.0, y + 8.0, 22.0, c),
                1 => icon_news(can, ic, y + 8.0, 22.0, c, RAIL_BG),
                2 => icon_person(can, ic, y + 8.0, 22.0, c),
                3 => icon_gear(can, ic, y + 8.0, 22.0, c, RAIL_BG),
                _ => icon_link(can, ic, y + 8.0, 22.0, c, RAIL_BG),
            }
            let lw = self.txtext.width(F::Med, 10.0, label);
            self.txtext.draw(can, F::Med, 10.0, RAIL_W / 2.0 - lw / 2.0, y + 46.0, if on { TEXT } else { DIM }, label);
            // пока идёт синк — под «Играть» проценты
            if i == 0 && self.syncing {
                if let Some(p) = &self.progress {
                    let total = p.total_bytes.load(Ordering::Relaxed);
                    if total > 0 {
                        let part = (p.bytes.load(Ordering::Relaxed) * 100 / total).min(100);
                        let s = format!("{part}%");
                        let sw = self.txtext.width(F::Mono, 10.0, &s);
                        self.txtext.draw(can, F::Mono, 10.0, RAIL_W / 2.0 - sw / 2.0, y + 58.0, DIM, &s);
                    }
                }
            }
            self.hits.push((Rect { x: 6.0, y: y - 2.0, w: RAIL_W - 12.0, h: 58.0 }, Hit::Action(format!("tab-{i}"))));
        }
        let py = h - 56.0;
        icon_power(can, RAIL_W / 2.0 - 11.0, py, 22.0, DIM, RAIL_BG);
        let quit = self.t("Выход", "Quit");
        let qw = self.txtext.width(F::Med, 10.0, &quit);
        self.txtext.draw(can, F::Med, 10.0, RAIL_W / 2.0 - qw / 2.0, py + 38.0, DIM, &quit);
        self.hits.push((Rect { x: 6.0, y: py - 6.0, w: RAIL_W - 12.0, h: 52.0 }, Hit::Action("quit".into())));
    }

    fn cap(&mut self, can: &mut Canvas, x: f32, y: f32, text: &str, size: f32) -> f32 {
        self.txtext.draw(can, F::Semi, size, x, y + self.txtext.ascent(F::Semi, size), TEXT, text);
        y + size + 14.0
    }

    /// Кадр: фон, полоса вкладок, вкладка, поверх — вуали (правка поля, сообщение).
    fn draw(&mut self, can: &mut Canvas, w: f32, h: f32) {
        if self.bg_w != can.w || self.bg_h != can.h || self.bg.len() != can.buf.len() {
            let mut bg = vec![0u32; can.buf.len()];
            for row in 0..can.h {
                let kk = if can.h <= 1 { 0.0 } else { row as f32 / can.h as f32 };
                let mix = |a: u32, b: u32| (((a as f32) * (1.0 - kk) + (b as f32) * kk + 0.5) as u32).min(255);
                let c = rgb(
                    mix((BG_TOP >> 16) & 255, (BG_BOT >> 16) & 255) as u8,
                    mix((BG_TOP >> 8) & 255, (BG_BOT >> 8) & 255) as u8,
                    mix(BG_TOP & 255, BG_BOT & 255) as u8,
                );
                bg[row * can.w..(row + 1) * can.w].fill(c);
            }
            self.bg = bg;
            self.bg_w = can.w;
            self.bg_h = can.h;
        }
        if self.bg.len() == can.buf.len() {
            can.buf.copy_from_slice(&self.bg);
        } else {
            can.buf.fill(BG_TOP);
        }
        self.txtext.set_k(can.k);
        self.k = can.k;
        self.hits.clear();
        self.rail(can, h);

        let x0 = RAIL_W + CPAD;
        let cw = w - RAIL_W - CPAD * 2.0;
        match self.tab {
            Tab::Play => self.tab_play(can, x0, cw, w, h),
            Tab::News => self.tab_news(can, x0, cw, w, h),
            Tab::Account => self.tab_account(can, x0, cw, w, h),
            Tab::Settings => self.tab_settings(can, x0, cw, w, h),
            Tab::Links => self.tab_links(can, x0, cw, w, h),
        }

        if self.edit.is_some() {
            self.overlay_edit(can, w, h);
        }
        if self.message.is_some() {
            self.overlay_message(can, w, h);
        }
    }

    fn tab_play(&mut self, can: &mut Canvas, x0: f32, cw: f32, w: f32, h: f32) {
        let _ = cw;
        self.txtext.draw(can, F::Semi, 24.0, x0, 64.0, TEXT, "Cursed Recode");
        if let Some(m) = &self.manifest {
            let sub = format!("{} {} · {}", self.t("билд", "build"), m.build, m.version);
            self.txtext.draw(can, F::Reg, 13.5, x0, 92.0, DIM, &sub);
        }
        // кто вошёл — справа
        let acc = self.account_name();
        let aw = self.txtext.width(F::Med, 13.0, &acc);
        let ax = w - CPAD - aw - 20.0;
        icon_person(can, ax - 4.0, 48.0, 16.0, DIM);
        self.txtext.draw(can, F::Med, 13.0, ax + 16.0, 60.0, DIM, &acc);

        if !self.chosen() {
            // первый экран: куда ставить игру
            let y = 130.0;
            let sub = self.t(
                "Сюда скачается Recode — около 650 МБ. Папку можно выбрать другую.",
                "Recode goes here — about 650 MB. Another folder can be picked.",
            );
            for (i, line) in self.txtext.wrap(F::Reg, 15.5, &sub, w - RAIL_W - CPAD * 2.0).into_iter().enumerate() {
                self.txtext.draw(can, F::Reg, 15.5, x0, y + i as f32 * 23.0, DIM, &line);
            }
            let py = y + 54.0;
            self.txtext.draw(can, F::Mono, 15.0, x0, py + self.txtext.ascent(F::Mono, 15.0), TEXT, &self.home.to_string_lossy());
            let by = py + 40.0;
            let bw = 340.0;
            let y = self.button(can, x0, by, bw, 46.0, 17.0, &self.t("Установить сюда", "Install here"), "install", true);
            self.button(can, x0, y, bw, 46.0, 17.0, &self.t("Другая папка", "Another folder"), "browse", false);
            return;
        }

        if self.syncing {
            let (done_b, total_b, done_f, total_f, phase) = match &self.progress {
                Some(p) => (
                    p.bytes.load(Ordering::Relaxed),
                    p.total_bytes.load(Ordering::Relaxed),
                    p.files.load(Ordering::Relaxed),
                    p.total_files.load(Ordering::Relaxed),
                    p.phase.load(Ordering::Relaxed),
                ),
                None => (0, 0, 0, 0, 0),
            };
            let title = if phase == 2 {
                self.t("Распаковываю", "Unpacking")
            } else {
                self.t("Скачиваю игру", "Getting the game")
            };
            let y = self.cap(can, x0, 130.0, &title, 26.0);
            let mb = |b: u64| b / 1048576;
            if total_b == 0 {
                self.txtext.draw(can, F::Reg, 16.0, x0, y + 30.0, DIM, &self.t("Всё уже на месте", "Everything is already there"));
            } else {
                let big = format!("{} / {} MB", mb(done_b), mb(total_b));
                self.txtext.draw(can, F::Semi, 30.0, x0, y + 34.0, TEXT, &big);
                let files = format!("{} / {} {}", done_f, total_f, self.t("файлов", "files"));
                self.txtext.draw(can, F::Reg, 14.0, x0, y + 62.0, DIM, &files);
                if let Some(at) = self.sync_at {
                    let secs = at.elapsed().as_secs_f32().max(0.5);
                    let speed = format!("{:.1} {}", done_b as f32 / 1048576.0 / secs, self.t("МБ/с", "MB/s"));
                    let sw = self.txtext.width(F::Reg, 14.0, &speed);
                    self.txtext.draw(can, F::Reg, 14.0, w - CPAD - sw, y + 62.0, DIM, &speed);
                }
                let bar_w = (w - RAIL_W - CPAD * 2.0).min(560.0);
                let by = y + 100.0;
                can.round(x0, by, bar_w, 6.0, 3.0, TRACK);
                let part = done_b as f32 / total_b as f32;
                if part > 0.0 {
                    can.round(x0, by, (bar_w * part).max(6.0), 6.0, 3.0, MAIN_BG);
                }
            }
            let dots = ".".repeat((self.tick % 8) as usize / 2 + 1);
            if phase == 0 {
                let status = format!("{}{}", self.t("Проверяю файлы", "Checking the files"), dots);
                self.txtext.draw(can, F::Reg, 13.0, x0, y + 130.0, DIM, &status);
            }
            return;
        }

        if self.child.is_some() {
            let y = self.cap(can, x0, 130.0, &self.t("Игра запущена", "The game is running"), 26.0);
            let sub = self.t(
                "Вывод игры — ниже и в loader/game.log. Окно лоадера можно свернуть: игра продолжает работать.",
                "The game's output is below and in loader/game.log. This window can be minimized: the game keeps running.",
            );
            for (i, line) in self.txtext.wrap(F::Reg, 14.0, &sub, w - RAIL_W - CPAD * 2.0).into_iter().enumerate() {
                self.txtext.draw(can, F::Reg, 14.0, x0, y + i as f32 * 21.0, DIM, &line);
            }
            let mut ly = y + 54.0;
            for line in &self.log.tail(16) {
                let short: String = if line.chars().count() > 120 { line.chars().take(120).collect() } else { line.clone() };
                self.txtext.draw(can, F::Mono, 12.5, x0, ly, DIM, &short);
                ly += 18.0;
            }
            return;
        }

        // главное: большая кнопка
        let by = h * 0.30;
        let y = self.button(can, x0, by, 320.0, 64.0, 20.0, &self.t("Играть", "Play"), "play", true);
        self.button(can, x0, y + 6.0, 220.0, 46.0, 17.0, &self.t("Проверить файлы", "Check the files"), "check-files", false);
        let who = if self.signed_in() {
            self.t("В игре ты — под своим логином.", "In the game you are under your login.")
        } else {
            self.t("Вход не обязателен: без него игрок — «Player».", "Signing in is optional: without it the player is \"Player\".")
        };
        self.txtext.draw(can, F::Reg, 13.0, x0, by + 140.0, DIM, who);

        if let Some(m) = &self.manifest {
            if !m.notes.is_empty() {
                let notes = format!("{} · {}", self.t("Что нового", "What's new"), m.notes.replace('\n', " "));
                let lines = self.txtext.wrap(F::Reg, 13.0, &notes, w - RAIL_W - CPAD * 2.0);
                let mut ny = h - 40.0 - lines.len() as f32 * 19.0;
                for line in &lines {
                    self.txtext.draw(can, F::Reg, 13.0, x0, ny, DIM, line);
                    ny += 19.0;
                }
            }
        }
    }

    fn tab_news(&mut self, can: &mut Canvas, x0: f32, cw: f32, _w: f32, _h: f32) {
        let y = self.cap(can, x0, 64.0, &self.t("Новости", "News"), 24.0);
        // что известно о билде — скопировать заранее: дальше идут кнопки (им нужен &mut self)
        let known = self.manifest.as_ref().map(|m| (m.build, m.version.clone(), m.notes.clone()));
        let Some((build, version, notes)) = known else {
            let sub = self.t(
                "Пока ничего не известно: нажми «Проверить файлы» — лоадер возьмёт с GitHub свежий билд и его новости.",
                "Nothing yet: press \"Check the files\" — the loader will take the fresh build and its news from GitHub.",
            );
            for (i, line) in self.txtext.wrap(F::Reg, 15.0, &sub, cw).into_iter().enumerate() {
                self.txtext.draw(can, F::Reg, 15.0, x0, y + 10.0 + i as f32 * 22.0, DIM, &line);
            }
            self.button(can, x0, y + 90.0, 220.0, 46.0, 17.0, &self.t("Проверить файлы", "Check the files"), "check-files", true);
            return;
        };
        let head = format!("{} {} · {}", self.t("билд", "build"), build, version);
        self.txtext.draw(can, F::Med, 15.0, x0, y + 6.0, TEXT, &head);
        let notes = if notes.is_empty() { self.t("Заметок к билду нет.", "No notes for this build.") } else { notes };
        let mut ny = y + 38.0;
        for line in self.txtext.wrap(F::Reg, 15.0, &notes, cw) {
            self.txtext.draw(can, F::Reg, 15.0, x0, ny, DIM, &line);
            ny += 23.0;
        }
        let by = (ny + 26.0).max(y + 120.0);
        let y = self.button(can, x0, by, 220.0, 46.0, 17.0, &self.t("Проверить файлы", "Check the files"), "check-files", false);
        self.button(can, x0, y, 220.0, 46.0, 17.0, &self.t("Открыть релизы", "Open the releases"), "releases", false);
    }

    fn tab_account(&mut self, can: &mut Canvas, x0: f32, cw: f32, _w: f32, _h: f32) {
        let y = self.cap(can, x0, 64.0, &self.t("Аккаунт", "Account"), 24.0);

        if self.signed_in() {
            let hi = self.t("Вошёл как", "Signed in as");
            self.txtext.draw(can, F::Reg, 14.0, x0, y + 8.0, DIM, &hi);
            self.txtext.draw(can, F::Semi, 30.0, x0, y + 50.0, TEXT, &self.settings.account);
            let uid = format!("UID {}", self.settings.uid);
            self.txtext.draw(can, F::Mono, 13.0, x0, y + 80.0, DIM, &uid);
            let note = self.t("В игре ты под этим именем; вход сохранён на 180 дней.", "In the game you are under this name; the sign-in lasts 180 days.");
            for (i, line) in self.txtext.wrap(F::Reg, 13.0, &note, cw).into_iter().enumerate() {
                self.txtext.draw(can, F::Reg, 13.0, x0, y + 112.0 + i as f32 * 19.0, DIM, &line);
            }
            let by = y + 170.0;
            let y = self.button(can, x0, by, 220.0, 46.0, 17.0, &self.t("Выйти", "Sign out"), "sign-out", false);
            self.button(can, x0, y, 220.0, 46.0, 17.0, &self.t("Сайт Cursed", "The Cursed site"), "cursed-site", false);
            return;
        }

        // две колонки: вход и регистрация (на узком окне — одна под другой)
        let two = cw >= 620.0;
        let colw = if two { (cw - 28.0) / 2.0 } else { cw.min(460.0) };
        let dots = if self.auth_busy { ".".repeat((self.tick % 8) as usize / 2 + 1) } else { String::new() };
        let start = y + 6.0;

        // колонка входа
        let mut sy = start;
        self.txtext.draw(can, F::Semi, 17.0, x0, sy + 8.0, TEXT, &self.t("Вход", "Sign in"));
        sy += 26.0;
        sy = self.field_box(can, x0, sy, colw, Field::SignInLogin, &self.t("Логин или почта", "Login or e-mail"), false);
        sy = self.field_box(can, x0, sy, colw, Field::SignInPass, &self.t("Пароль", "Password"), true);
        let sign_end = self.button(can, x0, sy + 4.0, colw.min(240.0), 46.0, 17.0, &format!("{}{}", self.t("Войти", "Sign in"), dots), "sign-in", true);

        // колонка регистрации
        let rx = if two { x0 + colw + 28.0 } else { x0 };
        let mut ry = if two { start } else { sign_end + 24.0 };
        self.txtext.draw(can, F::Semi, 17.0, rx, ry + 8.0, TEXT, &self.t("Регистрация", "Register"));
        ry += 26.0;
        ry = self.field_box(can, rx, ry, colw, Field::RegLogin, &self.t("Логин", "Login"), false);
        ry = self.field_box(can, rx, ry, colw, Field::RegEmail, &self.t("Почта", "E-mail"), false);
        ry = self.field_box(can, rx, ry, colw, Field::RegPass, &self.t("Пароль", "Password"), true);
        let reg_end = self.button(can, rx, ry + 4.0, colw.min(240.0), 46.0, 17.0, &format!("{}{}", self.t("Создать аккаунт", "Make an account"), dots), "register", true);

        // правила и ответ сервера
        let rules = self.t(
            "Логин — 3–16 знаков (a–z, 0–9, _), пароль — от 10. Аккаунт сайта Cursed: одно имя и для форума, и для игры.",
            "Login is 3–16 characters (a–z, 0–9, _), password from 10. One account of the Cursed site: one name for the forum and the game.",
        );
        let note_y = reg_end.max(sign_end) + 16.0;
        for (i, line) in self.txtext.wrap(F::Reg, 12.0, &rules, cw).into_iter().enumerate() {
            self.txtext.draw(can, F::Reg, 12.0, x0, note_y + i as f32 * 17.0, DIM, &line);
        }
        if let Some((text, err)) = &self.form_note {
            let c = if *err { RED } else { GREEN };
            let at = note_y + self.txtext.wrap(F::Reg, 12.0, &rules, cw).len() as f32 * 17.0 + 10.0;
            for (i, line) in self.txtext.wrap(F::Reg, 13.5, text, cw).into_iter().enumerate() {
                self.txtext.draw(can, F::Reg, 13.5, x0, at + i as f32 * 20.0, c, &line);
            }
        }
    }

    fn tab_settings(&mut self, can: &mut Canvas, x0: f32, cw: f32, _w: f32, h: f32) {
        let _ = self.cap(can, x0, 64.0, &self.t("Настройки", "Settings"), 24.0);
        let names = [self.t("Игра", "Game"), self.t("Компьютер", "Computer"), self.t("Папка", "Folder")];
        let mut x = x0;
        let y = 106.0;
        for (i, name) in names.iter().enumerate() {
            let tw = self.txtext.width(F::Med, 14.0, name) + 28.0;
            let on = i == self.section;
            can.round(x, y, tw, 34.0, 8.0, if on { PANEL } else { TRACK });
            let asc = self.txtext.ascent(F::Med, 14.0);
            self.txtext.draw(can, F::Med, 14.0, x + 14.0, y + 17.0 + asc / 2.0 - 1.0, if on { TEXT } else { DIM }, name);
            self.hits.push((Rect { x, y, w: tw, h: 34.0 }, Hit::Action(format!("sec{i}"))));
            x += tw + 8.0;
        }
        let y = y + 58.0;
        let sw = cw.min(520.0);
        match self.section {
            0 => {
                self.txtext.draw(can, F::Reg, 15.0, x0, y + 10.0, TEXT, &self.t("Память для игры", "Memory for the game"));
                let val = format!("{:.1} {}", self.settings.memory as f32 / 1024.0, self.t("ГБ", "GB"));
                let vw = self.txtext.width(F::Mono, 14.0, &val);
                self.txtext.draw(can, F::Mono, 14.0, x0 + sw - vw, y + 10.0, DIM, &val);
                let sy = y + 42.0;
                can.round(x0, sy, sw, 6.0, 3.0, TRACK);
                let part = ((self.settings.memory as f32 - 2048.0) / (16384.0 - 2048.0)).clamp(0.0, 1.0);
                let fill = (sw * part).max(6.0);
                can.round(x0, sy, fill, 6.0, 3.0, MAIN_BG);
                can.round(x0 + fill - 7.0, sy - 5.0, 14.0, 16.0, 7.0, MAIN_BG);
                self.hits.push((Rect { x: x0, y: sy - 10.0, w: sw, h: 26.0 }, Hit::Slider));
                for (i, line) in self.txtext.wrap(F::Reg, 12.5, &self.t("Сколько оперативной памяти получит игра.", "How much memory the game gets."), sw).into_iter().enumerate() {
                    self.txtext.draw(can, F::Reg, 12.5, x0, sy + 32.0 + i as f32 * 18.0, DIM, &line);
                }
                let y2 = sy + 80.0;
                self.txtext.draw(can, F::Reg, 15.0, x0, y2 + 10.0, TEXT, &self.t("Язык", "Language"));
                let lang_now = if self.settings.lang == "ru" { "Русский" } else { "English" };
                let lw = self.txtext.width(F::Mono, 14.0, lang_now) + 28.0;
                can.round(x0 + sw - lw, y2, lw, 34.0, 8.0, PANEL);
                self.txtext.draw(can, F::Mono, 14.0, x0 + sw - lw + 14.0, y2 + 19.0, TEXT, lang_now);
                self.hits.push((Rect { x: x0 + sw - lw, y: y2, w: lw, h: 34.0 }, Hit::Action("lang".into())));
            }
            1 => {
                self.txtext.draw(can, F::Reg, 15.0, x0, y + 10.0, TEXT, &self.t("Wine", "Wine"));
                let wine = self.settings.wine_binary();
                let edit = self.t("Изменить", "Change");
                let ew = self.txtext.width(F::Med, 14.0, &edit) + 28.0;
                can.round(x0 + sw - ew, y, ew, 34.0, 8.0, PANEL);
                self.txtext.draw(can, F::Med, 14.0, x0 + sw - ew + 14.0, y + 19.0, TEXT, &edit);
                self.hits.push((Rect { x: x0 + sw - ew, y, w: ew, h: 34.0 }, Hit::Action("wine-edit".into())));
                for (i, line) in self.txtext.wrap(F::Reg, 12.5, &self.t("Какой wine запускать (по умолчанию — wine из PATH).", "Which wine to run (default: wine from PATH)."), sw).into_iter().enumerate() {
                    self.txtext.draw(can, F::Reg, 12.5, x0, y + 76.0 + i as f32 * 18.0, DIM, &line);
                }
                let shown: String = if wine.chars().count() > 60 {
                    format!("…{}", wine.chars().skip(wine.chars().count() - 59).collect::<String>())
                } else {
                    wine
                };
                self.txtext.draw(can, F::Mono, 14.0, x0, y + 48.0, DIM, &shown);

                let y2 = y + 130.0;
                self.txtext.draw(can, F::Reg, 15.0, x0, y2 + 10.0, TEXT, &self.t("API аккаунтов", "Account API"));
                let api = self.settings.api_base();
                let aw = self.txtext.width(F::Med, 14.0, &edit) + 28.0;
                can.round(x0 + sw - aw, y2, aw, 34.0, 8.0, PANEL);
                self.txtext.draw(can, F::Med, 14.0, x0 + sw - aw + 14.0, y2 + 19.0, TEXT, &edit);
                self.hits.push((Rect { x: x0 + sw - aw, y: y2, w: aw, h: 34.0 }, Hit::Action("api-edit".into())));
                for (i, line) in self.txtext.wrap(F::Reg, 12.5, &self.t("Сервер регистрации и входа (сайт Cursed).", "The server of registration and sign-in (the Cursed site)."), sw).into_iter().enumerate() {
                    self.txtext.draw(can, F::Reg, 12.5, x0, y2 + 76.0 + i as f32 * 18.0, DIM, &line);
                }
                self.txtext.draw(can, F::Mono, 14.0, x0, y2 + 48.0, DIM, &api);
            }
            _ => {
                self.txtext.draw(can, F::Reg, 15.0, x0, y + 10.0, TEXT, &self.t("Папка игры", "The game's folder"));
                let size = format!("{:.1} GB", self.folder_bytes.unwrap_or(0) as f64 / 1073741824.0);
                let szw = self.txtext.width(F::Mono, 14.0, &size);
                self.txtext.draw(can, F::Mono, 14.0, x0 + sw - szw, y + 10.0, DIM, &size);
                self.txtext.draw(can, F::Mono, 14.0, x0, y + 42.0, DIM, &self.home.to_string_lossy());
                let y = y + 70.0;
                let y = self.button(can, x0, y, sw, 46.0, 17.0, &self.t("Открыть папку", "Open the folder"), "open-folder", false);
                let y = self.button(can, x0, y, sw, 46.0, 17.0, &self.t("Проверить файлы", "Check the files"), "check-files", false);
                let _ = self.button(can, x0, y, sw, 46.0, 17.0, &self.t("Сменить папку", "Another folder"), "change-folder", false);
                let _ = h;
            }
        }
    }

    fn tab_links(&mut self, can: &mut Canvas, x0: f32, _cw: f32, _w: f32, _h: f32) {
        let y = self.cap(can, x0, 64.0, &self.t("Ссылки", "Links"), 24.0);
        let y = self.button(can, x0, y + 8.0, 340.0, 46.0, 17.0, &self.t("Сайт (GitHub)", "Site (GitHub)"), "site", false);
        let y = self.button(can, x0, y, 340.0, 46.0, 17.0, &self.t("Сайт Cursed", "The Cursed site"), "cursed-site", false);
        let y = self.button(can, x0, y, 340.0, 46.0, 17.0, &self.t("Релизы", "Releases"), "releases", false);
        let _ = self.button(can, x0, y, 340.0, 46.0, 17.0, &self.t("Linux-инструкция", "Linux guide"), "docs", false);
    }

    /// Вуаль с полем ввода (папка, wine, API).
    fn overlay_edit(&mut self, can: &mut Canvas, w: f32, h: f32) {
        let Some(edit) = self.edit.clone() else { return };
        can.veil(140);
        // весь экран — «проглотитель» кликов: сквозь вуаль вкладки не нажимаются
        self.hits.push((Rect { x: 0.0, y: 0.0, w, h }, Hit::Action("noop".into())));
        let pw = (w - RAIL_W).min(560.0);
        let ph = 330.0;
        let px = RAIL_W + (w - RAIL_W - pw) / 2.0;
        let py = (h - ph) / 2.0;
        can.round(px, py, pw, ph, 14.0, PANEL_EDGE);
        can.round(px + 1.5, py + 1.5, pw - 3.0, ph - 3.0, 13.0, PANEL);
        let (cap, title, sub) = match edit.kind {
            EditKind::Folder => (
                self.t("Папка", "Folder"),
                self.t("Другая папка", "Another folder"),
                self.t("Полный путь к папке игры (создастся, если её нет).", "The full path of the game's folder (made if there is none)."),
            ),
            EditKind::Wine => (
                self.t("Wine", "Wine"),
                self.t("Какой wine", "Which wine"),
                self.t("Путь к wine или его имя (пусто — wine из PATH).", "The path to wine or its name (empty: wine from PATH)."),
            ),
            EditKind::Api => (
                self.t("API", "API"),
                self.t("Сервер аккаунтов", "The account server"),
                self.t("Адрес API регистрации и входа.", "The address of the sign-in API."),
            ),
        };
        let mut y = py + 26.0;
        let cap_up = cap.to_uppercase();
        self.txtext.draw(can, F::Med, 12.0, px + 28.0, y + self.txtext.ascent(F::Med, 12.0), DIM, &cap_up);
        y += 24.0;
        self.txtext.draw(can, F::Semi, 20.0, px + 28.0, y + self.txtext.ascent(F::Semi, 20.0), TEXT, &title);
        y += 34.0;
        for (i, line) in self.txtext.wrap(F::Reg, 13.0, &sub, pw - 56.0).into_iter().enumerate() {
            self.txtext.draw(can, F::Reg, 13.0, px + 28.0, y + i as f32 * 19.0, DIM, &line);
        }
        y += self.txtext.wrap(F::Reg, 13.0, &sub, pw - 56.0).len() as f32 * 19.0 + 8.0;
        let fw = pw - 56.0;
        can.round(px + 28.0, y, fw, 42.0, 9.0, rgb(96, 108, 128));
        can.round(px + 29.2, y + 1.2, fw - 2.4, 39.6, 8.0, rgb(16, 19, 26));
        let caret = if self.tick % 10 < 5 { "|" } else { "" };
        let shown: String = if edit.buf.chars().count() > 56 {
            format!("…{}", edit.buf.chars().skip(edit.buf.chars().count() - 55).collect::<String>())
        } else {
            edit.buf.clone()
        };
        self.txtext.draw(can, F::Mono, 14.0, px + 42.0, y + 26.0, TEXT, &format!("{shown}{caret}"));
        let y = y + 58.0;
        let y = self.button(can, px + 28.0, y, 180.0, 44.0, 16.0, &self.t("ОК", "OK"), "edit-ok", true);
        self.button(can, px + 28.0, y, 180.0, 44.0, 16.0, &self.t("Отмена", "Cancel"), "edit-cancel", false);
    }

    /// Вуаль с сообщением (ошибки синка и запуска, краш игры).
    fn overlay_message(&mut self, can: &mut Canvas, w: f32, h: f32) {
        let Some((title, text)) = self.message.clone() else { return };
        can.veil(150);
        self.hits.push((Rect { x: 0.0, y: 0.0, w, h }, Hit::Action("noop".into())));
        let pw = (w - RAIL_W).min(560.0);
        let px = RAIL_W + (w - RAIL_W - pw) / 2.0;
        let lines = self.txtext.wrap(F::Reg, 14.0, &text, pw - 56.0);
        let ph = 176.0 + lines.len() as f32 * 21.0;
        let py = (h - ph) / 2.0;
        can.round(px, py, pw, ph, 14.0, PANEL_EDGE);
        can.round(px + 1.5, py + 1.5, pw - 3.0, ph - 3.0, 13.0, PANEL);
        let cap = self.t("Не получилось", "It did not work");
        let cap_up = cap.to_uppercase();
        self.txtext.draw(can, F::Med, 12.0, px + 28.0, py + 30.0, DIM, &cap_up);
        self.txtext.draw(can, F::Semi, 20.0, px + 28.0, py + 70.0, TEXT, &title);
        let mut y = py + 104.0;
        for line in &lines {
            self.txtext.draw(can, F::Reg, 14.0, px + 28.0, y, DIM, line);
            y += 21.0;
        }
        self.button(can, px + 28.0, y + 14.0, 180.0, 44.0, 16.0, &self.t("Понятно", "I see"), "message-ok", true);
    }
}

// ---------------------------------------------------------------------------------------------- окно и события

struct Ticker(u64);

struct App {
    ui: Ui,
    /// Отрисовка: замыкание владеет окном, контекстом и поверхностью softbuffer — их настоящие
    /// типы не называются, а значит не зависят от сигнатур мягкого буфера.
    draw: Option<Box<dyn FnMut(&mut Ui)>>,
}

thread_local! {
    static WINDOW: std::cell::RefCell<Option<&'static Window>> = const { std::cell::RefCell::new(None) };
}

fn redraw() {
    WINDOW.with(|w| {
        if let Some(win) = w.borrow().as_ref() {
            win.request_redraw();
        }
    });
}

fn window_scale() -> f32 {
    WINDOW.with(|w| w.borrow().as_ref().map(|win| win.scale_factor() as f32).unwrap_or(1.0)).max(0.1)
}

impl ApplicationHandler<Ticker> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.draw.is_some() {
            return;
        }
        use winit::platform::x11::WindowAttributesExtX11;
        let icon = winit::window::Icon::from_rgba(ICON.to_vec(), 64, 64).ok();
        let attrs = WindowAttributes::default()
            .with_title("Cursed Recode")
            .with_name("cursed-recode", "cursed-recode")
            .with_inner_size(winit::dpi::LogicalSize::new(960.0, 640.0))
            .with_min_inner_size(winit::dpi::LogicalSize::new(840.0, 560.0))
            .with_window_icon(icon);
        let Ok(window) = event_loop.create_window(attrs) else {
            eprintln!("не вышло открыть окно (X11 или Wayland?)");
            std::process::exit(1);
        };
        let window: &'static Window = Box::leak(Box::new(window));
        WINDOW.with(|w| *w.borrow_mut() = Some(window));
        self.ui.k = window.scale_factor() as f32;
        let context = match softbuffer::Context::new(window) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("softbuffer: {e}");
                std::process::exit(1);
            }
        };
        let mut surface = match softbuffer::Surface::new(&context, window) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("softbuffer: {e}");
                std::process::exit(1);
            }
        };
        let draw = move |ui: &mut Ui| {
            let (w, h) = {
                let s = window.inner_size();
                (s.width, s.height)
            };
            let k = window.scale_factor() as f32;
            let _ = surface.resize(
                std::num::NonZeroU32::new(w.max(1)).unwrap(),
                std::num::NonZeroU32::new(h.max(1)).unwrap(),
            );
            let Ok(mut buffer) = surface.buffer_mut() else { return };
            {
                let mut can = Canvas { w: w.max(1) as usize, h: h.max(1) as usize, k, buf: &mut buffer[..] };
                ui.draw(&mut can, w as f32 / k, h as f32 / k);
            }
            let _ = buffer.present();
        };
        self.draw = Some(Box::new(draw));
        window.request_redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::CursorMoved { position, .. } => {
                let k = window_scale();
                self.ui.mouse = (position.x as f32 / k, position.y as f32 / k);
                if self.ui.drag {
                    for (r, h) in self.ui.hits.clone() {
                        if h == Hit::Slider {
                            self.ui.slide(self.ui.mouse.0, &r);
                        }
                    }
                }
                redraw();
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => {
                let (mx, my) = self.ui.mouse;
                self.ui.click(mx, my);
                redraw();
            }
            WindowEvent::MouseInput { state: ElementState::Released, .. } => {
                self.ui.drag = false;
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state != ElementState::Pressed {
                    return;
                }
                if self.ui.edit.is_some() {
                    // наложенное поле ввода: всё в него
                    match event.logical_key {
                        Key::Named(NamedKey::Backspace) => {
                            if let Some(e) = self.ui.edit.as_mut() {
                                e.buf.pop();
                            }
                        }
                        Key::Named(NamedKey::Escape) => self.ui.on_action("edit-cancel"),
                        Key::Named(NamedKey::Enter) => self.ui.on_action("edit-ok"),
                        Key::Character(s) => {
                            if let Some(e) = self.ui.edit.as_mut() {
                                e.buf.push_str(&s);
                            }
                        }
                        _ => {}
                    }
                } else if let Some(f) = self.ui.focus {
                    // форма аккаунта: ввод в поле, Enter — кнопка формы
                    match event.logical_key {
                        Key::Named(NamedKey::Backspace) => self.ui.field_put(f, None),
                        Key::Named(NamedKey::Escape) => self.ui.focus = None,
                        Key::Named(NamedKey::Enter) => match f {
                            Field::SignInLogin | Field::SignInPass => self.ui.do_sign_in(),
                            Field::RegLogin | Field::RegEmail | Field::RegPass => self.ui.do_register(),
                        },
                        Key::Character(s) => self.ui.field_put(f, s.chars().next()),
                        _ => {}
                    }
                } else if let Key::Named(NamedKey::Escape) = event.logical_key {
                    self.ui.focus = None;
                }
                redraw();
            }
            WindowEvent::RedrawRequested => {
                if let Some(draw) = self.draw.as_mut() {
                    draw(&mut self.ui);
                }
            }
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => redraw(),
            _ => {}
        }
    }

    /// Тик каждые 100 мс: будит окно только если что-то видно менялось или экран живой
    /// (загрузка, каретка, хвост лога, ответ API) — вкладки в покое спят между событиями.
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, t: Ticker) {
        self.ui.tick = t.0;
        let changed = self.ui.on_tick();
        let live = self.ui.syncing
            || self.ui.child.is_some()
            || self.ui.auth_busy
            || self.ui.edit.is_some()
            || (self.ui.focus.is_some() && self.ui.tick % 5 == 0);
        if changed || live {
            redraw();
        }
    }
}

fn main() {
    let home = lib::home_of(&lib::Settings::default(), None);
    let (tx, rx) = mpsc::channel::<ToUi>();
    let log = lib::LogFollow::new(&home);
    let mut ui = Ui {
        tx,
        rx,
        txtext: Text::new(),
        tab: Tab::Play,
        section: 0,
        message: None,
        edit: None,
        settings: lib::Settings::load(&home),
        home,
        base: std::env::var("CURSED_LOADER_API").unwrap_or_else(|_| lib::BASE.to_string()),
        manifest: None,
        progress: None,
        child: None,
        sync_at: None,
        auto_launch: false,
        syncing: false,
        sign_login: String::new(),
        sign_pass: String::new(),
        reg_login: String::new(),
        reg_email: String::new(),
        reg_pass: String::new(),
        focus: None,
        auth_busy: false,
        form_note: None,
        mouse: (-100.0, -100.0),
        hits: Vec::new(),
        tick: 0,
        drag: false,
        k: 1.0,
        bg: Vec::new(),
        bg_w: 0,
        bg_h: 0,
        folder_bytes: None,
        log,
    };
    // папка уже выбрана - сразу к делу: проверить файлы и в меню
    if ui.chosen() {
        ui.begin_sync(false);
    }
    // сохранённый вход: проверить, что токен ещё жив (API сайта)
    if !ui.settings.token.is_empty() && !ui.settings.account.is_empty() {
        let tx = ui.tx.clone();
        let api = ui.settings.api_base();
        let token = ui.settings.token.clone();
        std::thread::spawn(move || {
            let _ = tx.send(ToUi::Me(lib::whoami(&api, &token)));
        });
    }
    let mut app = App { ui, draw: None };

    let event_loop = EventLoop::<Ticker>::with_user_event().build().expect("событийный цикл");
    let proxy = event_loop.create_proxy();
    std::thread::spawn(move || {
        let mut n = 0u64;
        loop {
            std::thread::sleep(std::time::Duration::from_millis(100));
            n += 1;
            if proxy.send_event(Ticker(n)).is_err() {
                return;
            }
        }
    });
    event_loop.run_app(&mut app).expect("окно");
}
