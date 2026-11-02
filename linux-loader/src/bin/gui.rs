//! Окно лоадера на Linux — верное окну Windows-лоадера по экранам, текстам и шрифтам (Geist):
//! Setup (папка игры), Menu (Играть/Настройки/Ссылки…), Sync (прогресс), Settings, Message, Playing
//! (живой хвост лога игры). Фон — тихий градиент вместо живой «долины» Windows-лоадера (тот рендер
//! Win32-завязан), панели — стеклянные, кнопки — те же ряды. Входа через сайт нет: дистрибутив на
//! GitHub (dist/README.md), сессия локальная, игрок — «Player».
//!
//! Рендер — CPU-композитинг (winit + softbuffer + fontdue): меню - статичная 2D-картинка, для неё
//! этого довольно; Vulkan начинает работать, когда начинается сама игра (под Wine, winevulkan).

use std::path::PathBuf;
use std::process::Child;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::{mpsc, Arc};
use std::time::Instant;

use cursed_linux_loader as lib;
use fontdue::Font;
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

struct Fonts {
    reg: Font,
    med: Font,
    semi: Font,
    #[allow(dead_code)]
    bold: Font,
    mono: Font,
}

fn fonts() -> Fonts {
    let mk = |b: &[u8]| Font::from_bytes(b, fontdue::FontSettings::default()).expect("шрифт Geist");
    Fonts { reg: mk(REG), med: mk(MED), semi: mk(SEMI), bold: mk(BOLD), mono: mk(MONO) }
}

// ---------------------------------------------------------------------------------------------- палитра

const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    ((r as u32) << 16) | ((g as u32) << 8) | b as u32
}
const BG_TOP: u32 = rgb(10, 12, 17);
const BG_BOT: u32 = rgb(17, 20, 29);
const PANEL: u32 = rgb(23, 26, 34);
const PANEL_EDGE: u32 = rgb(44, 49, 63);
const TEXT: u32 = rgb(232, 234, 240);
const DIM: u32 = rgb(148, 155, 170);
const MAIN_BG: u32 = rgb(232, 234, 240);
const MAIN_TEXT: u32 = rgb(16, 19, 26);
const TRACK: u32 = rgb(38, 43, 54);

fn lighten(c: u32, k: f32) -> u32 {
    let ch = |v: u32| ((v as f32 * (1.0 - k) + 255.0 * k).clamp(0.0, 255.0)) as u32;
    rgb(ch((c >> 16) & 255) as u8, ch((c >> 8) & 255) as u8, ch(c & 255) as u8)
}

// ---------------------------------------------------------------------------------------------- холст

struct Canvas<'a> {
    w: usize,
    h: usize,
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
        let x0 = x.ceil() as i64;
        let y0 = y.ceil() as i64;
        let x1 = (x + w).floor() as i64;
        let y1 = (y + h).floor() as i64;
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
}

// ---------------------------------------------------------------------------------------------- текст

fn draw_text(can: &mut Canvas, f: &Font, size: f32, x: f32, base: f32, c: u32, text: &str) -> f32 {
    let mut pen = x;
    for ch in text.chars() {
        let (m, cov) = f.rasterize(ch, size);
        let (gw, gh) = (m.width, m.height);
        if gw == 0 || gh == 0 {
            pen += m.advance_width;
            continue;
        }
        let gx = pen + m.xmin as f32;
        let gy = base + m.ymin as f32;
        for (i, a) in cov.iter().enumerate() {
            if *a == 0 {
                continue;
            }
            can.blend(gx as i64 + (i % gw) as i64, gy as i64 + (i / gw) as i64, c, *a);
        }
        pen += m.advance_width;
    }
    pen - x
}

fn width_of(f: &Font, size: f32, text: &str) -> f32 {
    text.chars().map(|ch| f.rasterize(ch, size).0.advance_width).sum()
}

fn ascent_of(f: &Font, size: f32) -> f32 {
    -f.rasterize('H', size).0.ymin as f32
}

fn wrap(f: &Font, size: f32, text: &str, max_w: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for raw in text.split('\n') {
        let mut line = String::new();
        for word in raw.split(' ') {
            let try_line = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if width_of(f, size, &try_line) > max_w && !line.is_empty() {
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

// ---------------------------------------------------------------------------------------------- экраны и состояние

#[derive(Clone)]
enum Screen {
    Setup,
    Busy,
    Menu,
    Sync,
    Settings(usize),
    Links,
    Message { title: String, text: String },
    Playing,
    Field,
}

#[derive(Clone, PartialEq)]
enum Hit {
    Action(String),
    Slider,
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
}

/// Вся картина окна: экран, настройки, прогресс, кнопки последнего кадра.
struct Ui {
    fonts: Fonts,
    screen: Screen,
    settings: lib::Settings,
    home: PathBuf,
    base: String,
    manifest: Option<lib::Manifest>,
    progress: Option<Arc<lib::Progress>>,
    child: Option<Child>,
    sync_at: Option<Instant>,
    auto_launch: bool,
    rx: Receiver<ToUi>,
    mouse: (f32, f32),
    hits: Vec<(Rect, Hit)>,
    field: String,
    tick: u64,
    drag: bool,
}

const PAD: f32 = 56.0;

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
        self.screen = Screen::Message { title, text };
    }

    /// Синк в фоне; если просили — сразу запуск и экран Playing.
    fn begin_sync(&mut self, auto_launch: bool) {
        self.auto_launch = auto_launch;
        self.sync_at = Some(Instant::now());
        self.screen = Screen::Busy;
        let (tx, rx) = mpsc::channel::<ToUi>();
        self.rx = rx;
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

    /// Раз в 100 мс: что сказали фоновые потоки, жива ли игра.
    fn on_tick(&mut self) {
        loop {
            match self.rx.try_recv() {
                Ok(ToUi::Started(p)) => {
                    self.progress = Some(p);
                    self.screen = Screen::Sync;
                }
                Ok(ToUi::SyncDone(r)) => match r {
                    Ok(m) => {
                        self.manifest = Some(m);
                        if !self.auto_launch {
                            self.screen = Screen::Menu;
                        }
                    }
                    Err(e) => {
                        self.progress = None;
                        self.message(self.t("Синхронизация не вышла", "The sync did not finish"), e);
                    }
                },
                Ok(ToUi::Launched(r)) => match r {
                    Ok(c) => {
                        self.child = Some(c);
                        self.screen = Screen::Playing;
                    }
                    Err(e) => {
                        self.message(self.t("Игра не запустилась", "The game did not start"), e);
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
                let tail = lib::log_tail(&self.home, 8).join("\n");
                self.message(
                    self.t("Игра закрылась с ошибкой", "The game closed with an error"),
                    format!("{} {}.\n\n{}", self.t("Код выхода:", "Exit code:"), status.code().unwrap_or(-1), tail),
                );
            } else {
                self.screen = Screen::Menu;
            }
        }
    }

    fn click(&mut self, mx: f32, my: f32) {
        for (r, h) in self.hits.clone() {
            if !r.has(mx, my) {
                continue;
            }
            match h {
                Hit::Action(a) => self.on_action(&a),
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
            "install" => {
                self.choose();
                self.begin_sync(false);
            }
            "browse" | "change-folder" => {
                self.field = self.home.to_string_lossy().into_owned();
                self.screen = Screen::Field;
            }
            "field-ok" => {
                let text = self.field.trim().to_string();
                if !text.is_empty() {
                    self.home = PathBuf::from(&text);
                    self.settings.home = text;
                    self.settings.save(&self.home);
                }
                self.screen = Screen::Setup;
            }
            "field-cancel" => self.screen = Screen::Setup,
            "play" => {
                if self.child.is_some() {
                    self.message(
                        self.t("Игра уже запущена", "The game is already running"),
                        self.t("Одна игра за раз: дождись, пока эта закроется.", "One game at a time: wait for this one to close."),
                    );
                } else {
                    self.choose();
                    self.begin_sync(true);
                }
            }
            "settings" => self.screen = Screen::Settings(0),
            "links" => self.screen = Screen::Links,
            "back" => self.screen = Screen::Menu,
            "sec0" => self.screen = Screen::Settings(0),
            "sec1" => self.screen = Screen::Settings(1),
            "sec2" => self.screen = Screen::Settings(2),
            "lang" => {
                self.settings.lang = if self.settings.lang == "ru" { "en".into() } else { "ru".into() };
                self.settings.save(&self.home);
            }
            "wine-edit" => {
                self.field = self.settings.wine_binary();
                self.screen = Screen::Field;
            }
            "open-folder" => {
                let _ = std::process::Command::new("xdg-open").arg(&self.home).spawn();
            }
            "check-files" => self.begin_sync(false),
            "site" => {
                let _ = std::process::Command::new("xdg-open").arg("https://github.com/kpanics/games").spawn();
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

    fn button(&mut self, can: &mut Canvas, x: f32, y: f32, w: f32, label: &str, action: &str, main: bool) -> f32 {
        let h = 46.0;
        let hovered = self.mouse.0 >= x && self.mouse.0 <= x + w && self.mouse.1 >= y && self.mouse.1 <= y + h;
        let f = if main { &self.fonts.semi } else { &self.fonts.reg };
        let size = 17.0;
        let tw = width_of(f, size, label);
        let bg = if main {
            if hovered { lighten(MAIN_BG, 0.08) } else { MAIN_BG }
        } else if hovered {
            lighten(PANEL, 0.10)
        } else {
            PANEL
        };
        if main {
            can.round(x, y, w, h, 10.0, bg);
        } else {
            can.round(x, y, w, h, 10.0, PANEL_EDGE);
            can.round(x + 1.0, y + 1.0, w - 2.0, h - 2.0, 9.0, bg);
        }
        draw_text(can, f, size, x + (w - tw) / 2.0, y + h / 2.0 + ascent_of(f, size) / 2.0 - 1.0, if main { MAIN_TEXT } else { TEXT }, label);
        self.hits.push((Rect { x, y, w, h }, Hit::Action(action.to_string())));
        y + h + 10.0
    }

    fn background(can: &mut Canvas, w: f32, h: f32) {
        let steps = h as usize;
        for i in 0..steps {
            let k = i as f32 / steps as f32;
            let mix = |a: u32, b: u32| (((a as f32) * (1.0 - k) + (b as f32) * k + 0.5) as u32).min(255);
            let c = rgb(
                mix((BG_TOP >> 16) & 255, (BG_BOT >> 16) & 255) as u8,
                mix((BG_TOP >> 8) & 255, (BG_BOT >> 8) & 255) as u8,
                mix(BG_TOP & 255, BG_BOT & 255) as u8,
            );
            can.rect(0.0, i as f32, w, 1.0, c);
        }
    }

    fn cap_title(&mut self, can: &mut Canvas, w: f32, cap: &str, title: &str, sub: &str) -> f32 {
        let mut y = 64.0;
        draw_text(can, &self.fonts.med, 13.0, PAD, y + ascent_of(&self.fonts.med, 13.0), DIM, cap.to_uppercase().as_str());
        y += 30.0;
        if !title.is_empty() {
            draw_text(can, &self.fonts.semi, 26.0, PAD, y + ascent_of(&self.fonts.semi, 26.0), TEXT, title);
            y += 40.0;
        } else {
            y += 10.0;
        }
        if !sub.is_empty() {
            for line in wrap(&self.fonts.reg, 15.5, sub, w - PAD * 2.0) {
                draw_text(can, &self.fonts.reg, 15.5, PAD, y + ascent_of(&self.fonts.reg, 15.5), DIM, &line);
                y += 23.0;
            }
            y += 14.0;
        }
        y
    }

    fn draw(&mut self, can: &mut Canvas, w: f32, h: f32) {
        Self::background(can, w, h);
        self.hits.clear();
        let bw = (w - PAD * 2.0).min(420.0);
        match self.screen.clone() {
            Screen::Setup => {
                let y = self.cap_title(
                    can, w,
                    &self.t("Папка игры", "The game's folder"),
                    "Cursed Recode",
                    &self.t(
                        "Сюда скачается Recode — около 650 МБ. Папку можно выбрать другую.",
                        "Recode goes here — about 650 MB. Another folder can be picked.",
                    ),
                );
                draw_text(can, &self.fonts.mono, 15.0, PAD, y + ascent_of(&self.fonts.mono, 15.0), TEXT, &self.home.to_string_lossy());
                let y = y + 44.0;
                let y = self.button(can, PAD, y, bw, &self.t("Установить сюда", "Install here"), "install", true);
                let y = self.button(can, PAD, y, bw, &self.t("Другая папка", "Another folder"), "browse", false);
                self.button(can, PAD, y, bw, &self.t("Выход", "Quit"), "quit", false);
            }
            Screen::Busy => {
                self.cap_title(can, w, &self.t("Подожди", "A moment"), &self.t("Проверяю файлы…", "Checking the files…"), "");
                let dots = ".".repeat((self.tick % 8) as usize / 2 + 1);
                draw_text(can, &self.fonts.reg, 15.0, PAD, 180.0, DIM, &dots);
            }
            Screen::Menu => {
                draw_text(can, &self.fonts.semi, 24.0, PAD, 74.0, TEXT, "Cursed Recode");
                if let Some(m) = &self.manifest {
                    let sub = format!("{} {}", self.t("билд", "build"), m.build);
                    draw_text(can, &self.fonts.reg, 13.5, PAD, 100.0, DIM, &sub);
                }
                let login = "Player";
                let lw = width_of(&self.fonts.med, 14.0, login);
                draw_text(can, &self.fonts.med, 14.0, w - PAD - lw, 74.0, DIM, login);

                if let Some(m) = &self.manifest {
                    if !m.notes.is_empty() {
                        let notes = format!("{} · {}", self.t("Что нового", "What's new"), m.notes.replace('\n', " "));
                        let lines = wrap(&self.fonts.reg, 13.0, &notes, (w - PAD * 2.0).min(520.0));
                        let mut ny = h - 40.0 - lines.len() as f32 * 19.0;
                        for line in lines {
                            draw_text(can, &self.fonts.reg, 13.0, PAD, ny, DIM, &line);
                            ny += 19.0;
                        }
                    }
                }

                let mut y = h * 0.32;
                y = self.button(can, PAD, y, bw, &self.t("Играть", "Play"), "play", true);
                y = self.button(can, PAD, y, bw, &self.t("Настройки", "Settings"), "settings", false);
                y = self.button(can, PAD, y, bw, &self.t("Ссылки", "Links"), "links", false);
                self.button(can, PAD, y, bw, &self.t("Выход", "Quit"), "quit", false);
            }
            Screen::Sync => {
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
                } else if phase == 4 {
                    self.t("Не получилось", "It did not work")
                } else {
                    self.t("Скачиваю игру", "Getting the game")
                };
                let y = self.cap_title(can, w, &self.t("Подожди", "A moment"), &title, "");
                let mb = |b: u64| b / 1048576;
                if total_b == 0 {
                    draw_text(can, &self.fonts.reg, 16.0, PAD, y + 30.0, DIM, &self.t("Всё уже на месте", "Everything is already there"));
                } else {
                    let big = format!("{} / {} MB", mb(done_b), mb(total_b));
                    draw_text(can, &self.fonts.semi, 30.0, PAD, y + 34.0, TEXT, &big);
                    let files = format!("{} / {} {}", mb(done_f), mb(total_f), self.t("файлов", "files"));
                    draw_text(can, &self.fonts.reg, 14.0, PAD, y + 62.0, DIM, &files);
                    if let Some(at) = self.sync_at {
                        let secs = at.elapsed().as_secs_f32().max(0.5);
                        let speed = format!("{:.1} {}", done_b as f32 / 1048576.0 / secs, self.t("МБ/с", "MB/s"));
                        let sw = width_of(&self.fonts.reg, 14.0, &speed);
                        draw_text(can, &self.fonts.reg, 14.0, w - PAD - sw, y + 62.0, DIM, &speed);
                    }
                    let bar_w = (w - PAD * 2.0).min(560.0);
                    let by = y + 100.0;
                    can.round(PAD, by, bar_w, 6.0, 3.0, TRACK);
                    let k = done_b as f32 / total_b as f32;
                    if k > 0.0 {
                        can.round(PAD, by, (bar_w * k).max(6.0), 6.0, 3.0, MAIN_BG);
                    }
                }
                let dots = ".".repeat((self.tick % 8) as usize / 2 + 1);
                let status = if phase == 0 { format!("{}{}", self.t("Проверяю файлы", "Checking the files"), dots) } else { String::new() };
                if !status.is_empty() {
                    draw_text(can, &self.fonts.reg, 13.0, PAD, y + 130.0, DIM, &status);
                }
            }
            Screen::Settings(sec) => {
                let y = self.cap_title(can, w, &self.t("Настройки", "Settings"), "", "");
                let names = [self.t("Игра", "Game"), self.t("Компьютер", "Computer"), self.t("Папка", "Folder")];
                let mut x = PAD;
                for (i, name) in names.iter().enumerate() {
                    let tw = width_of(&self.fonts.med, 14.0, name) + 28.0;
                    let on = i == sec;
                    can.round(x, y, tw, 34.0, 8.0, if on { PANEL } else { TRACK });
                    draw_text(can, &self.fonts.med, 14.0, x + 14.0, y + 17.0 + ascent_of(&self.fonts.med, 14.0) / 2.0 - 1.0, if on { TEXT } else { DIM }, name);
                    self.hits.push((Rect { x, y, w: tw, h: 34.0 }, Hit::Action(format!("sec{i}"))));
                    x += tw + 8.0;
                }
                let y = y + 58.0;
                let sw = (w - PAD * 2.0).min(520.0);
                match sec {
                    0 => {
                        draw_text(can, &self.fonts.reg, 15.0, PAD, y + 10.0, TEXT, &self.t("Память для игры", "Memory for the game"));
                        let val = format!("{:.1} {}", self.settings.memory as f32 / 1024.0, self.t("ГБ", "GB"));
                        let vw = width_of(&self.fonts.mono, 14.0, &val);
                        draw_text(can, &self.fonts.mono, 14.0, PAD + sw - vw, y + 10.0, DIM, &val);
                        let sy = y + 42.0;
                        can.round(PAD, sy, sw, 6.0, 3.0, TRACK);
                        let k = ((self.settings.memory as f32 - 2048.0) / (16384.0 - 2048.0)).clamp(0.0, 1.0);
                        let fill = (sw * k).max(6.0);
                        can.round(PAD, sy, fill, 6.0, 3.0, MAIN_BG);
                        can.round(PAD + fill - 7.0, sy - 5.0, 14.0, 16.0, 7.0, MAIN_BG);
                        self.hits.push((Rect { x: PAD, y: sy - 10.0, w: sw, h: 26.0 }, Hit::Slider));
                        for (i, line) in wrap(&self.fonts.reg, 12.5, &self.t("Сколько оперативной памяти получит игра.", "How much memory the game gets."), sw).into_iter().enumerate() {
                            draw_text(can, &self.fonts.reg, 12.5, PAD, sy + 32.0 + i as f32 * 18.0, DIM, &line);
                        }
                        let y2 = sy + 80.0;
                        draw_text(can, &self.fonts.reg, 15.0, PAD, y2 + 10.0, TEXT, &self.t("Язык", "Language"));
                        let lang_now = if self.settings.lang == "ru" { "Русский" } else { "English" };
                        let lw = width_of(&self.fonts.mono, 14.0, lang_now) + 28.0;
                        can.round(PAD + sw - lw, y2, lw, 34.0, 8.0, PANEL);
                        draw_text(can, &self.fonts.mono, 14.0, PAD + sw - lw + 14.0, y2 + 19.0, TEXT, lang_now);
                        self.hits.push((Rect { x: PAD + sw - lw, y: y2, w: lw, h: 34.0 }, Hit::Action("lang".into())));
                    }
                    1 => {
                        draw_text(can, &self.fonts.reg, 15.0, PAD, y + 10.0, TEXT, &self.t("Wine", "Wine"));
                        let wine = self.settings.wine_binary();
                        let edit = self.t("Изменить", "Change");
                        let ew = width_of(&self.fonts.med, 14.0, &edit) + 28.0;
                        can.round(PAD + sw - ew, y, ew, 34.0, 8.0, PANEL);
                        draw_text(can, &self.fonts.med, 14.0, PAD + sw - ew + 14.0, y + 19.0, TEXT, &edit);
                        self.hits.push((Rect { x: PAD + sw - ew, y, w: ew, h: 34.0 }, Hit::Action("wine-edit".into())));
                        for (i, line) in wrap(&self.fonts.reg, 12.5, &self.t("Какой wine запускать (по умолчанию — wine из PATH).", "Which wine to run (default: wine from PATH)."), sw).into_iter().enumerate() {
                            draw_text(can, &self.fonts.reg, 12.5, PAD, y + 76.0 + i as f32 * 18.0, DIM, &line);
                        }
                        let shown: String = if wine.chars().count() > 60 {
                            format!("…{}", wine.chars().skip(wine.chars().count() - 59).collect::<String>())
                        } else {
                            wine.clone()
                        };
                        draw_text(can, &self.fonts.mono, 14.0, PAD, y + 48.0, DIM, &shown);
                    }
                    _ => {
                        draw_text(can, &self.fonts.reg, 15.0, PAD, y + 10.0, TEXT, &self.t("Папка игры", "The game's folder"));
                        let size = format!("{:.1} GB", lib::dir_bytes(&self.home) as f64 / 1073741824.0);
                        let szw = width_of(&self.fonts.mono, 14.0, &size);
                        draw_text(can, &self.fonts.mono, 14.0, PAD + sw - szw, y + 10.0, DIM, &size);
                        draw_text(can, &self.fonts.mono, 14.0, PAD, y + 42.0, DIM, &self.home.to_string_lossy());
                        let y = y + 70.0;
                        let y = self.button(can, PAD, y, sw, &self.t("Открыть папку", "Open the folder"), "open-folder", false);
                        let y = self.button(can, PAD, y, sw, &self.t("Проверить файлы", "Check the files"), "check-files", false);
                        self.button(can, PAD, y, sw, &self.t("Сменить папку", "Another folder"), "change-folder", false);
                    }
                }
                self.button(can, PAD, h - PAD - 46.0, bw, &self.t("Назад", "Back"), "back", false);
            }
            Screen::Links => {
                let y = self.cap_title(can, w, &self.t("Ссылки", "Links"), "", "");
                let y = self.button(can, PAD, y, bw, &self.t("Сайт (GitHub)", "Site (GitHub)"), "site", false);
                let y = self.button(can, PAD, y, bw, &self.t("Релизы", "Releases"), "releases", false);
                let y = self.button(can, PAD, y, bw, &self.t("Linux-инструкция", "Linux guide"), "docs", false);
                self.button(can, PAD, y, bw, &self.t("Назад", "Back"), "back", false);
            }
            Screen::Message { title, text } => {
                let y = self.cap_title(can, w, &self.t("Не получилось", "It did not work"), &title, &text);
                self.button(can, PAD, y.max(h * 0.55), bw, &self.t("Назад", "Back"), "back", true);
            }
            Screen::Playing => {
                let y = self.cap_title(
                    can, w,
                    &self.t("Игра", "Game"),
                    &self.t("Игра запущена", "The game is running"),
                    &self.t(
                        "Вывод игры — ниже и в loader/game.log. Окно лоадера можно свернуть: игра продолжает работать.",
                        "The game's output is below and in loader/game.log. This window can be minimized: the game keeps running.",
                    ),
                );
                let mut ly = y;
                for line in lib::log_tail(&self.home, 14) {
                    let short: String = if line.chars().count() > 110 { line.chars().take(110).collect() } else { line };
                    draw_text(can, &self.fonts.mono, 12.5, PAD, ly, DIM, &short);
                    ly += 18.0;
                }
                self.button(can, PAD, h - PAD - 46.0, bw, &self.t("Свернуть в меню", "Back to the menu"), "back", false);
            }
            Screen::Field => {
                let y = self.cap_title(
                    can, w,
                    &self.t("Папка", "Folder"),
                    &self.t("Другая папка", "Another folder"),
                    &self.t(
                        "Полный путь к папке игры (создастся, если её нет).",
                        "The full path of the game's folder (made if there is none).",
                    ),
                );
                let fw = (w - PAD * 2.0).min(560.0);
                can.round(PAD, y, fw, 44.0, 10.0, PANEL_EDGE);
                can.round(PAD + 1.0, y + 1.0, fw - 2.0, 42.0, 9.0, PANEL);
                let caret = if self.tick % 10 < 5 { "|" } else { "" };
                let shown = if self.field.chars().count() > 52 { format!("…{}", self.field.chars().skip(self.field.chars().count() - 51).collect::<String>()) } else { self.field.clone() };
                draw_text(can, &self.fonts.mono, 15.0, PAD + 16.0, y + 27.0, TEXT, &format!("{shown}{caret}"));
                let y = y + 64.0;
                let y = self.button(can, PAD, y, bw, &self.t("ОК", "OK"), "field-ok", true);
                self.button(can, PAD, y, bw, &self.t("Отмена", "Cancel"), "field-cancel", false);
            }
        }
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

impl ApplicationHandler<Ticker> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.draw.is_some() {
            return;
        }
        let attrs = WindowAttributes::default()
            .with_title("Cursed Recode")
            .with_inner_size(winit::dpi::LogicalSize::new(900.0, 600.0))
            .with_min_inner_size(winit::dpi::LogicalSize::new(720.0, 480.0));
        let Ok(window) = event_loop.create_window(attrs) else {
            eprintln!("не вышло открыть окно (X11 или Wayland?)");
            std::process::exit(1);
        };
        let window: &'static Window = Box::leak(Box::new(window));
        WINDOW.with(|w| *w.borrow_mut() = Some(window));
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
            let _ = surface.resize(
                std::num::NonZeroU32::new(w.max(1)).unwrap(),
                std::num::NonZeroU32::new(h.max(1)).unwrap(),
            );
            let Ok(mut buffer) = surface.buffer_mut() else { return };
            {
                let mut can = Canvas { w: w.max(1) as usize, h: h.max(1) as usize, buf: &mut buffer[..] };
                ui.draw(&mut can, w as f32, h as f32);
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
                self.ui.mouse = (position.x as f32, position.y as f32);
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
                if matches!(self.ui.screen, Screen::Field) {
                    match event.logical_key {
                        Key::Named(NamedKey::Backspace) => {
                            self.ui.field.pop();
                        }
                        Key::Named(NamedKey::Escape) => self.ui.on_action("field-cancel"),
                        Key::Named(NamedKey::Enter) => self.ui.on_action("field-ok"),
                        Key::Character(s) => self.ui.field.push_str(&s),
                        _ => {}
                    }
                } else if let Key::Named(NamedKey::Escape) = event.logical_key {
                    let to_menu = matches!(
                        self.ui.screen,
                        Screen::Settings(_) | Screen::Links | Screen::Message { .. } | Screen::Playing
                    );
                    if to_menu {
                        self.ui.screen = Screen::Menu;
                    }
                }
                redraw();
            }
            WindowEvent::RedrawRequested => {
                if let Some(draw) = self.draw.as_mut() {
                    draw(&mut self.ui);
                }
            }
            WindowEvent::Resized(_) => redraw(),
            _ => {}
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _t: Ticker) {
        self.ui.tick = self.ui.tick.wrapping_add(1);
        self.ui.on_tick();
        redraw();
    }
}

fn main() {
    let home = lib::home_of(&lib::Settings::default(), None);
    let mut ui = Ui {
        fonts: fonts(),
        screen: Screen::Setup,
        settings: lib::Settings::load(&home),
        home,
        base: std::env::var("CURSED_LOADER_API").unwrap_or_else(|_| lib::BASE.to_string()),
        manifest: None,
        progress: None,
        child: None,
        sync_at: None,
        auto_launch: false,
        rx: mpsc::channel().1,
        mouse: (-100.0, -100.0),
        hits: Vec::new(),
        field: String::new(),
        tick: 0,
        drag: false,
    };
    // папка уже выбрана - сразу к делу: проверить файлы и в меню
    if ui.chosen() {
        ui.begin_sync(false);
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
