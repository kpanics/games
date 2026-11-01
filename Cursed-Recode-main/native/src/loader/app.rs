//! The loader's window and its life (her words, 15.10): it looks like the game's main menu — the valley, big words —
//! and «Играть» turns this same window into the game's launch. Its picture is the game's own: the launch's renderer
//! draws the valley (launch/), the menus' draw the words (menus/loader.rs, and the game's settings page for its
//! settings), the game window's bar is its bar (chrome/: dragging, snapping, its buttons), shown by the compositor at
//! the monitor's rate (twin/present.rs). Nothing of its own music (her word): the corner shows what plays on the
//! computer.
//!
//! Its screens: the folder at the first start, the sign-in through the site (a code here, «allow» there), the menu
//! (Play, Continue — the server joined last —, settings, links, the account top right, what's new bottom left), and
//! what stops it (no subscription, another computer, a ban, no connection). Play: the valley goes into
//! its outlines and their lines fill with light as the build comes (half of it: «412 из 640 МБ» under them), the
//! game is started, and this process becomes the window before the game for it (twin::link_for_loader): the story
//! goes on from the outlines, the lines' light is the game's loading, the picture fills in, the game's window takes
//! over; this one hides, an icon in the tray says it waits, and the farewell when the game closes is its own. Then the
//! menu again.

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ash::vk;
use serde_json::{json, Value};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use super::{api, game, install, store, sync};
use crate::engine::gpu::{image_in, Compute};
use crate::launch::{Inputs, Launch, LINES};
use crate::twin::present::{self, Composed};
use crate::vk::device::Gpu;
use crate::vk::encoder::{memory_barrier, Encoder};
use crate::vk::resources::{Texture, View};
use crate::vk::Progress;

const CLASS: &str = "CursedRecodeLoader";
const SITE: &str = "https://cursedclient.xyz";
const TELEGRAM: &str = "https://t.me/crsdclient";
const DISCORD: &str = "https://discord.gg/crsdclient";
/// Where the menu's story stands (the launch's card long over: the menu up, settled).
pub(super) const MENU_T: f32 = 1000.0;
/// The window before the game goes on from this second of its story: the outlines drawn, the loading begun.
const STORY_FROM: f32 = crate::launch::ink::it::LOAD_FROM;
/// The lines' light the build lights (the game's loading the rest).
const BUILD_LIGHT: f32 = 0.5;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

// ---------------------------------------------------------------------------------------------------- the window

/// What the window heard (its procedure, the main thread's).
pub(super) enum In {
    Move(f32, f32),
    /// button (0 left, 1 right, 2 middle), action (1 down, 2 up), where
    Button(i32, i32, f32, f32),
    Wheel(f32),
    Key(String, u32),
    Char(u32),
    Close,
}

pub(super) static INPUT: Mutex<Vec<In>> = Mutex::new(Vec::new());
/// The bar's height on the window (its picture is under it): the pointer's place in the picture.
pub(super) static BAR: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

fn push(e: In) {
    if let Ok(mut q) = INPUT.lock() {
        q.push(e);
    }
}

fn key_name(vk: u32) -> Option<String> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
    let n = match vk as u16 {
        VK_RETURN => "Return",
        VK_ESCAPE => "Escape",
        VK_BACK => "Backspace",
        VK_TAB => "Tab",
        VK_LEFT => "Left",
        VK_RIGHT => "Right",
        VK_UP => "Up",
        VK_DOWN => "Down",
        VK_DELETE => "Delete",
        VK_HOME => "Home",
        VK_END => "End",
        VK_SPACE => "Space",
        _ => {
            let c = vk as u8 as char;
            return (c.is_ascii_uppercase() || c.is_ascii_digit()).then(|| c.to_string());
        }
    };
    Some(n.into())
}

fn mods() -> u32 {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_MENU, VK_SHIFT};
    let down = |k: u16| unsafe { GetKeyState(k as i32) } < 0;
    (down(VK_SHIFT) as u32) | (down(VK_CONTROL) as u32) << 1 | (down(VK_MENU) as u32) << 2
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    let at = || {
        let x = (l & 0xFFFF) as i16 as f32;
        let y = ((l >> 16) & 0xFFFF) as i16 as f32 - BAR.load(Ordering::Relaxed) as f32;
        (x, y)
    };
    match msg {
        WM_MOUSEMOVE => {
            let (x, y) = at();
            push(In::Move(x, y));
            0
        }
        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN => {
            let (x, y) = at();
            windows_sys::Win32::UI::Input::KeyboardAndMouse::SetCapture(hwnd);
            push(In::Button(if msg == WM_LBUTTONDOWN { 0 } else if msg == WM_RBUTTONDOWN { 1 } else { 2 }, 1, x, y));
            0
        }
        WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP => {
            let (x, y) = at();
            windows_sys::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture();
            push(In::Button(if msg == WM_LBUTTONUP { 0 } else if msg == WM_RBUTTONUP { 1 } else { 2 }, 2, x, y));
            0
        }
        WM_MOUSEWHEEL => {
            push(In::Wheel(((w >> 16) & 0xFFFF) as i16 as f32 / 120.0));
            0
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            if let Some(n) = key_name(w as u32) {
                push(In::Key(n, mods()));
            }
            if msg == WM_SYSKEYDOWN {
                return DefWindowProcW(hwnd, msg, w, l);
            }
            0
        }
        WM_CHAR => {
            if w as u32 >= 32 {
                push(In::Char(w as u32));
            }
            0
        }
        WM_SETCURSOR if (l & 0xFFFF) as u32 == HTCLIENT => {
            SetCursor(LoadCursorW(std::ptr::null_mut(), IDC_ARROW));
            1
        }
        WM_GETMINMAXINFO => {
            let mm = &mut *(l as *mut MINMAXINFO);
            mm.ptMinTrackSize = POINT { x: 860, y: 520 };
            0
        }
        WM_DPICHANGED => {
            let r = &*(l as *const RECT);
            SetWindowPos(hwnd, std::ptr::null_mut(), r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOZORDER | SWP_NOACTIVATE);
            0
        }
        WM_CLOSE => {
            push(In::Close);
            0
        }
        _ => DefWindowProcW(hwnd, msg, w, l),
    }
}

/// The window at the game window's place `p` (its picture where the game's will be), the bar its own (chrome/).
unsafe fn make_window(p: &crate::twin::Place2) -> HWND {
    make_window_as(p, CLASS)
}

/// The same window of another class (loader/stop.rs: not the loader's, which another loader looks for).
pub(super) unsafe fn make_window_as(p: &crate::twin::Place2, class: &str) -> HWND {
    let class = wide(class);
    let instance = windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null());
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(proc),
        hInstance: instance,
        hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
        lpszClassName: class.as_ptr(),
        ..std::mem::zeroed()
    };
    RegisterClassExW(&wc);
    let title = wide(crate::twin::titlebar::TITLE);
    let ex = WS_EX_APPWINDOW | windows_sys::Win32::UI::WindowsAndMessaging::WS_EX_NOREDIRECTIONBITMAP;
    let hwnd = CreateWindowExW(ex, class.as_ptr(), title.as_ptr(), WS_OVERLAPPEDWINDOW, p.frame[0], p.frame[1], p.frame[2], p.frame[3], std::ptr::null_mut(), std::ptr::null_mut(), instance, std::ptr::null());
    if hwnd.is_null() {
        return hwnd;
    }
    crate::twin::titlebar::set_icons(hwnd);
    crate::chrome::install(hwnd, Some(p.client));
    hwnd
}

pub(super) fn client_size(hwnd: HWND) -> (u32, u32) {
    let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    unsafe { GetClientRect(hwnd, &mut r) };
    ((r.right - r.left).max(1) as u32, (r.bottom - r.top).max(1) as u32)
}

pub(super) fn open_url(url: &str) {
    // tests (a build from the sources): the page is not opened on her screen
    if !crate::license::RELEASE && std::env::var_os("CURSED_LOADER_NOBROWSER").is_some() {
        super::log(&format!("would open {url}"));
        return;
    }
    let (op, u) = (wide("open"), wide(url));
    unsafe {
        windows_sys::Win32::UI::Shell::ShellExecuteW(std::ptr::null_mut(), op.as_ptr(), u.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL);
    }
}

/// A folder picked in Windows' own dialog (the first start's «another folder»).
unsafe fn pick_folder(owner: HWND, title: &str) -> Option<PathBuf> {
    use windows::core::HSTRING;
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
    use windows::Win32::UI::Shell::{FileOpenDialog, IFileOpenDialog, FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, SIGDN_FILESYSPATH};
    let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    let dlg: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
    let opts = dlg.GetOptions().ok()?;
    dlg.SetOptions(opts | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM).ok()?;
    let _ = dlg.SetTitle(&HSTRING::from(title));
    dlg.Show(windows::Win32::Foundation::HWND(owner as _)).ok()?;
    let item = dlg.GetResult().ok()?;
    let name = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
    let text = name.to_string().ok();
    CoTaskMemFree(Some(name.0 as *const _));
    text.map(PathBuf::from)
}

// ------------------------------------------------------------------------------------------------- the picture

/// The device and what draws on it for the window's life (made again after the game, which had its own).
pub(super) struct Gfx {
    gpu: Box<Gpu>,
    launch: Option<Launch>,
    launch_maker: Option<std::thread::JoinHandle<crate::launch::Made>>,
    gui: Option<crate::gui::render::Render>,
    gui_maker: Option<std::thread::JoinHandle<crate::gui::render::Made>>,
    painter: Option<crate::chrome::paint::Painter>,
    compose: Compute,
    sampler: vk::Sampler,
    timeline: vk::Semaphore,
    enc: Encoder,
    progress: Progress,
    sized: Option<Sized>,
    pending: Option<(u64, usize)>,
    frame: u64,
}

/// What is the window's size: the picture (under the bar), the bar, the whole, the compositor's swap chain.
struct Sized {
    w: u32,
    h: u32,
    bar_h: u32,
    card: (Box<Texture>, Box<View>),
    menu: (Box<Texture>, Box<View>),
    bar: (Box<Texture>, Box<View>),
    surf: (Box<Texture>, Box<View>),
    out: Composed,
}

fn free(gpu: &Gpu, img: (Box<Texture>, Box<View>)) {
    let (mut t, v) = img;
    unsafe {
        gpu.device.destroy_image_view(v.raw, None);
        gpu.device.destroy_image(t.image, None);
    }
    if let Some(a) = t.alloc.take() {
        gpu.memory.free(&gpu.device, a);
    }
}

impl Gfx {
    pub(super) fn new() -> Option<Gfx> {
        let gpu = Box::new(Gpu::new(0, false, false).ok()?);
        let gpu_ref: &'static Gpu = unsafe { &*(&*gpu as *const Gpu) };
        let launch_maker = Some(std::thread::spawn(move || crate::launch::Made(Launch::new(gpu_ref))));
        let gui_maker = Some(std::thread::spawn(move || crate::gui::render::Made(crate::gui::render::Render::new(gpu_ref))));
        let compose = Compute::new(
            &gpu,
            "loader_window",
            &crate::launch::words(include_bytes!("../../shaders/launch/spv/loader.spv")),
            &[
                vk::DescriptorType::UNIFORM_BUFFER,
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                vk::DescriptorType::STORAGE_IMAGE,
                vk::DescriptorType::UNIFORM_BUFFER,
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            ],
            0,
        );
        let sampler = unsafe {
            gpu.device
                .create_sampler(
                    &vk::SamplerCreateInfo::default()
                        .mag_filter(vk::Filter::LINEAR)
                        .min_filter(vk::Filter::LINEAR)
                        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE),
                    None,
                )
                .ok()?
        };
        let timeline = Encoder::timeline(&gpu);
        let enc = Encoder::new(&gpu, timeline);
        Some(Gfx { gpu, launch: None, launch_maker, gui: None, gui_maker, painter: None, compose, sampler, timeline, enc, progress: Progress::new(), sized: None, pending: None, frame: 0 })
    }

    /// The launch's renderer, once made (its pipelines compile beside the window).
    pub(super) fn ready(&mut self) -> bool {
        if self.launch.is_none() && self.launch_maker.as_ref().is_some_and(|m| m.is_finished()) {
            self.launch = self.launch_maker.take().and_then(|m| m.join().ok()).map(|m| m.0);
        }
        if self.gui.is_none() && self.gui_maker.as_ref().is_some_and(|m| m.is_finished()) {
            self.gui = self.gui_maker.take().and_then(|m| m.join().ok()).map(|m| m.0);
        }
        self.launch.is_some() && self.gui.is_some()
    }

    /// Made again for the window's client area `cw` × `ch` (the bar's `bar_h` on top).
    pub(super) fn size(&mut self, hwnd: HWND, cw: u32, ch: u32, bar_h: u32) -> bool {
        if self.sized.as_ref().is_some_and(|s| s.w == cw && s.h + s.bar_h == ch && s.bar_h == bar_h) {
            return true;
        }
        let gpu: &Gpu = &self.gpu;
        unsafe {
            let _ = gpu.device.device_wait_idle();
        }
        self.pending = None;
        if let Some(s) = self.sized.take() {
            s.out.destroy(gpu);
            for img in [s.card, s.menu, s.bar, s.surf] {
                free(gpu, img);
            }
        }
        let h = ch.saturating_sub(bar_h).max(1);
        let out = match unsafe { Composed::new(gpu, hwnd as *mut core::ffi::c_void, cw, ch) } {
            Ok(c) => c,
            Err(e) => {
                super::log(&format!("the window's swap chain: {e}"));
                return false;
            }
        };
        let cmd = self.enc.cmd(gpu);
        let card = image_in(gpu, cmd, "Loader valley", vk::Format::R8G8B8A8_UNORM, cw, h, 1, false);
        let menu = image_in(gpu, cmd, "Loader menus", vk::Format::R8G8B8A8_UNORM, cw, h, 1, false);
        let bar = image_in(gpu, cmd, "Loader bar", vk::Format::R8G8B8A8_UNORM, cw, bar_h.max(1), 1, false);
        let surf = image_in(gpu, cmd, "Loader window", vk::Format::R8G8B8A8_UNORM, cw, ch, 1, false);
        if self.painter.is_none() {
            self.painter = Some(crate::chrome::paint::Painter::new(gpu, &mut self.enc, cmd));
        }
        self.sized = Some(Sized { w: cw, h, bar_h, card, menu, bar, surf, out });
        true
    }

    /// One frame: the valley (`i`), the menus' picture over it (`built`), the bar (`look`), seen `fade`.
    pub(super) fn frame(&mut self, i: &Inputs, built: Option<crate::gui::Built>, look: &crate::chrome::Look, fade: f32) {
        let Some(s) = self.sized.as_mut() else { return };
        let gpu: &Gpu = &self.gpu;
        s.out.wait(50);
        if let Some((index, slot)) = self.pending.take() {
            if self.enc.await_submit(gpu, index, 1_000_000_000) {
                s.out.present(slot);
            }
        }
        let slot = (self.frame % 2) as usize;
        let cmd = self.enc.cmd(gpu);
        if let Some(p) = self.painter.as_mut() {
            p.draw(gpu, &mut self.enc, cmd, look, s.bar.1.raw);
            memory_barrier(gpu, cmd);
        }
        if let Some(l) = self.launch.as_mut() {
            l.frame(gpu, &mut self.enc, i, &[], &s.card.0);
        }
        match (built, self.gui.as_mut()) {
            (Some(b), Some(g)) => g.frame(gpu, &mut self.enc, &b, &s.menu.0, Some(&s.card.0)),
            _ => unsafe {
                let range = vk::ImageSubresourceRange { aspect_mask: vk::ImageAspectFlags::COLOR, base_mip_level: 0, level_count: 1, base_array_layer: 0, layer_count: 1 };
                gpu.device.cmd_clear_color_image(cmd, s.menu.0.image, vk::ImageLayout::GENERAL, &vk::ClearColorValue { float32: [0.0; 4] }, &[range]);
            },
        }
        let cmd = self.enc.cmd(gpu);
        memory_barrier(gpu, cmd);
        let ch = s.h + s.bar_h;
        let u: [f32; 12] = [s.w as f32, ch as f32, s.bar_h as f32, fade, 0.0, s.bar_h as f32, s.w as f32, s.h as f32, 1.0, 0.0, 0.0, 0.0];
        let piece = self.enc.transient.mapped(gpu, 48, gpu.info.min_uniform_align.max(16) as u64, 48, 1);
        unsafe { std::ptr::copy_nonoverlapping(u.as_ptr() as *const u8, piece.host, 48) };
        let ubo = vk::DescriptorBufferInfo { buffer: unsafe { (*piece.buffer).raw }, offset: piece.offset, range: 48 };
        let dummy = self.enc.transient.mapped(gpu, 896, gpu.info.min_uniform_align.max(16) as u64, 896, 1);
        let dubo = vk::DescriptorBufferInfo { buffer: unsafe { (*dummy.buffer).raw }, offset: dummy.offset, range: 896 };
        let images = [
            vk::DescriptorImageInfo { sampler: self.sampler, image_view: s.card.1.raw, image_layout: vk::ImageLayout::GENERAL },
            vk::DescriptorImageInfo { sampler: vk::Sampler::null(), image_view: s.surf.1.raw, image_layout: vk::ImageLayout::GENERAL },
            vk::DescriptorImageInfo { sampler: self.sampler, image_view: s.bar.1.raw, image_layout: vk::ImageLayout::GENERAL },
            vk::DescriptorImageInfo { sampler: self.sampler, image_view: s.menu.1.raw, image_layout: vk::ImageLayout::GENERAL },
        ];
        let writes = [
            vk::WriteDescriptorSet::default().dst_binding(0).descriptor_type(vk::DescriptorType::UNIFORM_BUFFER).buffer_info(std::slice::from_ref(&dubo)),
            vk::WriteDescriptorSet::default().dst_binding(1).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(std::slice::from_ref(&images[0])),
            vk::WriteDescriptorSet::default().dst_binding(2).descriptor_type(vk::DescriptorType::STORAGE_IMAGE).image_info(std::slice::from_ref(&images[1])),
            vk::WriteDescriptorSet::default().dst_binding(3).descriptor_type(vk::DescriptorType::UNIFORM_BUFFER).buffer_info(std::slice::from_ref(&ubo)),
            vk::WriteDescriptorSet::default().dst_binding(4).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(std::slice::from_ref(&images[2])),
            vk::WriteDescriptorSet::default().dst_binding(5).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(std::slice::from_ref(&images[3])),
        ];
        unsafe {
            gpu.device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.compose.pipeline);
            gpu.push_d.cmd_push_descriptor_set(cmd, vk::PipelineBindPoint::COMPUTE, self.compose.layout, 0, &writes);
            gpu.device.cmd_dispatch(cmd, s.w.div_ceil(8), ch.div_ceil(8), 1);
        }
        memory_barrier(gpu, cmd);
        s.out.idle(slot);
        present::copy_into(gpu, cmd, s.surf.0.image, s.out.images[slot], s.w, ch);
        self.enc.submit(gpu, &[], &self.progress);
        self.pending = Some((self.enc.current - 1, slot));
        self.frame += 1;
    }

    pub(super) fn destroy(mut self) {
        let gpu: &Gpu = &self.gpu;
        unsafe {
            let _ = gpu.device.device_wait_idle();
        }
        if let Some(s) = self.sized.take() {
            s.out.destroy(gpu);
            for img in [s.card, s.menu, s.bar, s.surf] {
                free(gpu, img);
            }
        }
        if let Some(m) = self.launch_maker.take() {
            if let Ok(l) = m.join() {
                l.0.destroy(gpu);
            }
        }
        if let Some(l) = self.launch.take() {
            l.destroy(gpu);
        }
        if let Some(m) = self.gui_maker.take() {
            if let Ok(g) = m.join() {
                g.0.destroy(gpu);
            }
        }
        if let Some(g) = self.gui.take() {
            g.destroy(gpu);
        }
        if let Some(p) = self.painter.take() {
            p.destroy(gpu);
        }
        self.compose.destroy(gpu);
        unsafe { gpu.device.destroy_sampler(self.sampler, None) };
        self.enc.destroy(gpu);
        unsafe { gpu.device.destroy_semaphore(self.timeline, None) };
        gpu.destroy();
    }
}

// ------------------------------------------------------------------------------------------------- the screens

#[derive(Clone, Debug, PartialEq)]
enum Screen {
    /// The first start: where the game goes.
    Setup,
    SignIn,
    /// The sign-in through the site: the code shown, the page opened; its state.
    Linking { code: String, key: String, url: String, state: &'static str },
    /// Asking the server (the start: may this account play here).
    Busy,
    Menu,
    Account,
    Links,
    Settings { section: String },
    /// What stops it: its caption, title, words and the words to press.
    Message { cap: String, title: String, text: String, words: Vec<(String, String, i64)> },
    /// Play: the build checked and fetched, the valley in its outlines.
    Sync,
}

/// What a thread brought back.
enum Done {
    Device(Value),
    Link(Result<Value, api::Fail>),
    Poll(Result<Option<Value>, api::Fail>),
    Start(Result<Value, api::Fail>),
    Synced(Result<(), String>),
    Ticket(Result<Value, api::Fail>),
    Reset(Result<Value, api::Fail>),
}

/// Play under way: since when (the outlines' second), the build's progress, where it is going.
struct Play {
    since: f32,
    progress: Arc<sync::Progress>,
    server: Option<String>,
    /// The build is in place and the ticket came: the game is started once the outlines are whole and the words gone.
    ticket: Option<Value>,
    /// Cancelled (or failed): the outlines go back into the valley from this second.
    back_at: Option<f32>,
    failed: Option<(String, String)>,
}

fn ru(settings: &store::Settings) -> bool {
    settings.lang != "en"
}

/// The loader's words in its language: `t(ru, en)`.
fn tf(ru: bool) -> impl Fn(&str, &str) -> String {
    move |r, e| if ru { r } else { e }.to_string()
}

struct App {
    hwnd: HWND,
    gfx: Option<Gfx>,
    ease: crate::chrome::Ease,
    settings: store::Settings,
    session: Option<store::Session>,
    device: Option<Value>,
    started: Option<Value>,
    manifest: Option<sync::Manifest>,
    screen: Screen,
    page: (u32, String, String),
    /// When the page was closed with nothing after it (the words gone before the window before the game).
    page_closed: Option<f32>,
    next_id: u32,
    tx: Sender<Done>,
    rx: Receiver<Done>,
    epoch: Instant,
    mouse: [f32; 2],
    cur: [f32; 5],
    look: [f32; 2],
    flash_at: f64,
    next_flash: f64,
    clicks: f32,
    play: Option<Play>,
    shown: bool,
    shown_at: f32,
    quit: bool,
    /// Tests: a screen shown at once stays (the start's answer does not take the menu over it).
    hold_screen: bool,
    poll_at: f32,
    after_device: Option<Box<dyn FnOnce(&mut App)>>,
}

impl App {
    fn t(&self, r: &str, e: &str) -> String {
        if ru(&self.settings) { r } else { e }.to_string()
    }

    fn now(&self) -> f32 {
        self.epoch.elapsed().as_secs_f32()
    }

    fn spawn(&self, f: impl FnOnce() -> Done + Send + 'static) {
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(f());
        });
    }

    fn go(&mut self, s: Screen) {
        self.screen = s;
    }

    /// The page for the screen now (its kind, its data).
    fn page_of(&self) -> (&'static str, String, Value) {
        let t = tf(ru(&self.settings));
        let nick = self.session.as_ref().map(|s| s.login.clone()).unwrap_or_default();
        let media = crate::gui::media::now(None).map(|m| json!({ "title": m.title, "artist": m.artist, "pos": m.pos, "len": m.len, "playing": m.playing }));
        let build = match &self.manifest {
            Some(m) => format!("Cursed · {} · {}", m.version, t(&format!("сборка {}", m.build), &format!("build {}", m.build))),
            None => "Cursed".to_string(),
        };
        let news: Vec<String> = self.manifest.as_ref().map(|m| m.notes.lines().map(str::trim).filter(|l| !l.is_empty()).map(|l| l.trim_start_matches(['-', '+', '•', ' ']).to_string()).collect()).unwrap_or_default();
        let base = |key: &str, cap: String, title: String, face: &str, text: String, words: Vec<(String, String, i64)>| -> (&'static str, String, Value) {
            let words: Vec<Value> = words.into_iter().map(|w| json!([w.0, w.1, w.2])).collect();
            ("loader", key.to_string(), json!({ "cap": cap, "title": title, "title_face": face, "text": text, "words": words, "nick": nick, "media": media, "build": build, "news": news, "sub": self.sub_line() }))
        };
        let w = |a: &str, label: String, kind: i64| (a.to_string(), label, kind);
        match &self.screen {
            Screen::Setup => base(
                "setup",
                t("Папка игры", "The game's folder"),
                store::home().to_string_lossy().to_string(),
                "book",
                t("Сюда скачается Recode — около 650 МБ. Папку можно выбрать другую.", "Recode goes here — about 650 MB. Another folder can be picked."),
                vec![w("install", t("Установить сюда", "Install here"), 1), w("browse", t("Другая папка", "Another folder"), 0), w("quit", t("Выход", "Quit"), 2)],
            ),
            Screen::SignIn => base(
                "signin",
                t("Вход", "Sign in"),
                t("Через сайт", "Through the site"),
                "book",
                t("Откроем cursedclient.xyz в браузере — там подтвердишь вход своим аккаунтом. Пароль сюда вводить не нужно.", "cursedclient.xyz opens in the browser: allow the sign-in there with your account. No password goes in here."),
                vec![w("link", t("Войти", "Sign in"), 1), w("quit", t("Выход", "Quit"), 2)],
            ),
            Screen::Linking { code, state, .. } => {
                let text = match *state {
                    "denied" => t("Вход отклонён на сайте.", "The sign-in was turned down on the site."),
                    "gone" => t("Код устарел. Попробуй ещё раз.", "The code ran out. Try again."),
                    "offline" => t("Нет связи с сайтом. Проверь интернет.", "No connection to the site. Check your internet."),
                    _ => t("В браузере открылась страница входа. Проверь, что код тот же, и нажми «Разрешить».", "The sign-in page opened in the browser. Check the code is the same and press «Allow»."),
                };
                let words = if *state == "waiting" { vec![w("reopen", t("Открыть страницу снова", "Open the page again"), 0), w("cancel", t("Отмена", "Cancel"), 2)] } else { vec![w("link", t("Ещё раз", "Again"), 0), w("cancel", t("Отмена", "Cancel"), 2)] };
                base("link", t("Вход через сайт", "Sign in through the site"), code.clone(), "thin", text, words)
            }
            Screen::Busy => base("busy", t("Подожди", "A moment"), String::new(), "book", t("Проверяю подписку…", "Checking the subscription…"), vec![]),
            Screen::Menu => {
                let mut words = vec![w("play", t("Играть", "Play"), 1)];
                if let Some((name, addr)) = game::last_server(&store::home()) {
                    let shown = if name.is_empty() { addr } else { name };
                    words.push(("continue".into(), format!("{} · {}", t("Продолжить", "Continue"), shown), 0));
                }
                words.push(w("settings", t("Настройки", "Settings"), 0));
                words.push(w("links", t("Ссылки", "Links"), 0));
                words.push(w("quit", t("Выход", "Quit"), 2));
                base("menu", String::new(), String::new(), "thin", String::new(), words)
            }
            Screen::Account => base(
                "account",
                t("Аккаунт", "Account"),
                nick.clone(),
                "thin",
                self.sub_text(),
                vec![w("site", t("Личный кабинет", "Your account on the site"), 0), w("switch", t("Сменить аккаунт", "Another account"), 0), w("back", t("Назад", "Back"), 2)],
            ),
            Screen::Links => base(
                "links",
                t("Ссылки", "Links"),
                String::new(),
                "book",
                String::new(),
                vec![w("open-site", t("Сайт", "Site"), 0), w("open-tg", "Telegram".into(), 0), w("open-discord", "Discord".into(), 0), w("back", t("Назад", "Back"), 2)],
            ),
            Screen::Message { cap, title, text, words } => base("message", cap.clone(), title.clone(), "thin", text.clone(), words.clone()),
            Screen::Sync => {
                let p = self.play.as_ref();
                let (cap, title, text) = match p {
                    Some(pl) if pl.back_at.is_none() && pl.ticket.is_none() => {
                        let pr = &pl.progress;
                        let total = pr.total.load(Ordering::Relaxed);
                        let done = pr.shown();
                        if total == 0 {
                            let (c, n) = (pr.checked.load(Ordering::Relaxed), pr.to_check.load(Ordering::Relaxed));
                            (t("Проверяю файлы", "Checking the files"), if n > 0 { format!("{c} / {n}") } else { String::new() }, String::new())
                        } else {
                            // whole megabytes, a tenth of one while there are few of them
                            let small = total < 20 << 20;
                            let mb = |b: u64| if small { format!("{:.1}", b as f64 / 1048576.0) } else { format!("{}", (b as f64 / 1048576.0).round() as u64) };
                            let speed = self.speed();
                            let text = if pr.net_down.load(Ordering::Relaxed) { t("Нет связи — жду её", "No connection — waiting for it") } else { speed };
                            (t("Скачиваю игру", "Getting the game"), t(&format!("{} из {} МБ", mb(done), mb(total)), &format!("{} of {} MB", mb(done), mb(total))), text)
                        }
                    }
                    _ => (String::new(), String::new(), String::new()),
                };
                let words = if p.is_some_and(|pl| pl.back_at.is_none() && pl.ticket.is_none()) { vec![w("cancel-play", t("Отмена", "Cancel"), 2)] } else { vec![] };
                if cap.is_empty() {
                    // the words go before the window before the game takes over (it draws none)
                    return ("none", "none".into(), Value::Null);
                }
                base("sync", cap, title, "thin", text, words)
            }
            Screen::Settings { section } => ("settings", format!("settings:{section}"), self.settings_data(section)),
        }
    }

    fn speed(&self) -> String {
        let Some(p) = self.play.as_ref() else { return String::new() };
        let secs = (self.now() - p.since).max(0.5);
        let done = p.progress.done.load(Ordering::Relaxed) as f32;
        let rate = done / secs;
        let left = (p.progress.total.load(Ordering::Relaxed) as f32 - done).max(0.0);
        if rate < 1.0 {
            return String::new();
        }
        let eta = (left / rate).ceil() as u32;
        let eta = if eta >= 60 { self.t(&format!("{} мин {} с", eta / 60, eta % 60), &format!("{} min {} s", eta / 60, eta % 60)) } else { self.t(&format!("{eta} с"), &format!("{eta} s")) };
        format!("{:.1} {} · {} {}", rate / 1048576.0, self.t("МБ/с", "MB/s"), self.t("осталось", "left"), eta)
    }

    /// The subscription, short (under the account's name).
    fn sub_line(&self) -> String {
        let r = self.started.as_ref().map(|s| s["recode"].clone()).unwrap_or(Value::Null);
        if r["forever"].as_bool() == Some(true) {
            return self.t("Recode навсегда", "Recode forever");
        }
        match r["until"].as_i64() {
            Some(u) if r["active"].as_bool() == Some(true) => format!("Recode {} {}", self.t("до", "until"), date(u)),
            _ => String::new(),
        }
    }

    fn sub_text(&self) -> String {
        let r = self.started.as_ref().map(|s| s["recode"].clone()).unwrap_or(Value::Null);
        if r["forever"].as_bool() == Some(true) {
            return self.t("Подписка Recode — навсегда.", "Recode subscription: forever.");
        }
        match r["until"].as_i64() {
            Some(u) => {
                let days = ((u - unix_now()) as f64 / 86_400.0).ceil().max(0.0) as i64;
                self.t(
                    &format!("Подписка Recode — до {} (осталось {days} дн.). Продлить: ключ в личном кабинете.", date(u)),
                    &format!("Recode subscription until {} ({days} days left). Extend it with a key in your account on the site.", date(u)),
                )
            }
            None => String::new(),
        }
    }

    /// The loader's settings in the game's settings page (menus/settings.rs).
    fn settings_data(&self, section: &str) -> Value {
        let t = tf(ru(&self.settings));
        let sections = json!([
            { "id": "game", "name": t("Игра", "Game") },
            { "id": "pc", "name": t("Компьютер", "Computer") },
            { "id": "folder", "name": t("Папка", "Folder") },
        ]);
        let most = store::memory_most();
        let mem = self.settings.memory;
        let v = (mem.saturating_sub(store::MEMORY_LEAST)) as f32 / (most.saturating_sub(store::MEMORY_LEAST)).max(1) as f32;
        let rows = match section {
            "pc" => json!([
                { "t": "text", "s": t("Подписка привязана к одному компьютеру. Сбросить привязку можно раз в сутки — тогда игра запустится на другом.", "The subscription is tied to one computer. The tie can be freed once a day: then the game starts on another."), "sec": "pc" },
                { "t": "button", "id": 10, "s": t("Сбросить привязку ПК", "Free the computer"), "sec": "pc" },
            ]),
            "folder" => json!([
                { "t": "text", "s": store::home().to_string_lossy(), "sec": "folder" },
                { "t": "button", "id": 20, "s": t("Открыть папку", "Open the folder"), "sec": "folder" },
                { "t": "button", "id": 21, "s": t("Проверить файлы", "Check the files"), "sec": "folder" },
                { "t": "button", "id": 22, "s": t("Сменить папку", "Another folder"), "sec": "folder" },
            ]),
            _ => json!([
                { "t": "slider", "id": 1, "label": t("Память для игры", "Memory for the game"), "value": format!("{:.1} {}", mem as f32 / 1024.0, t("ГБ", "GB")), "v": v, "sec": "game",
                  "tip": t("Сколько оперативной памяти получит игра. Больше — не всегда лучше: Windows тоже нужна память.", "How much memory the game gets. More is not always better: Windows needs some too.") },
                { "t": "cycle", "id": 2, "label": t("Язык", "Language"), "values": ["Русский", "English"], "index": if ru(&self.settings) { 0 } else { 1 }, "sec": "game" },
            ]),
        };
        json!({ "section": if section.is_empty() { "game" } else { section }, "sections": sections, "rows": rows })
    }
}

fn unix_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// A day as people write it (dd.mm.yyyy), local time near enough (the server's seconds).
fn date(unix: i64) -> String {
    // days since 1970-01-01 to a civil date (Howard Hinnant's)
    let z = (unix + 3 * 3600).div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{d:02}.{m:02}.{}", if m <= 2 { y + 1 } else { y })
}

// ------------------------------------------------------------------------------------------------- what happens

impl App {
    /// The device's parts are known (they take a moment: the TPM, the disk): what waited for them goes on.
    fn with_device(&mut self, f: impl FnOnce(&mut App) + 'static) {
        if self.device.is_some() {
            f(self);
        } else {
            self.after_device = Some(Box::new(f));
        }
    }

    /// Signed in: may this account play here (the start). A computer without a TPM 2.0 is asked about too (her word,
    /// 20.10: the server tells it by its other parts); its screen stays for a server that would want one again.
    fn check(&mut self) {
        let Some(s) = self.session.clone() else {
            self.go(Screen::SignIn);
            return;
        };
        self.go(Screen::Busy);
        self.with_device(move |a| {
            let device = a.device.clone().unwrap_or(Value::Null);
            a.spawn(move || Done::Start(api::start(&s.token, &device)));
        });
    }

    fn tpm(&mut self) {
        let t = tf(ru(&self.settings));
        self.go(Screen::Message {
            cap: t("Не запустится", "It will not start"),
            title: t("Нужен TPM 2.0", "TPM 2.0 is needed"),
            text: t(
                "Recode работает только на компьютерах с TPM 2.0 (он есть почти везде с 2016 года, но бывает выключен).\nВключить: зайди в BIOS (Del или F2 при включении ПК) и включи Intel PTT (на Intel) или AMD fTPM (на AMD), сохрани и перезагрузись.",
                "Recode runs only on computers with TPM 2.0 (almost every one since 2016 has it, sometimes switched off).\nTo switch it on: open the BIOS (Del or F2 as the computer starts), switch on Intel PTT (Intel) or AMD fTPM (AMD), save and restart.",
            ),
            words: vec![("retry".into(), t("Проверить снова", "Check again"), 0), ("quit".into(), t("Выход", "Quit"), 2)],
        });
    }

    /// What the server said no to (the start or the play), as a screen.
    ///
    /// What it was said no to is looked up among the words the binaries keep as noise (crate::vault), so the choice
    /// of what to show is not sitting next to the word itself for a dumper to read.
    fn refused(&mut self, f: &api::Fail) {
        let t = tf(ru(&self.settings));
        let v = f.body();
        let w = |a: &str, label: String, kind: i64| (a.to_string(), label, kind);
        let said = f.error();
        let (cap, title, text, words) = if said == crate::vault::signed_out() {
            store::Session::forget();
            self.session = None;
            self.go(Screen::SignIn);
            return;
        } else if said == crate::vault::no_recode() {
            (
                t("Подписка", "Subscription"),
                t("Подписки нет", "No subscription"),
                t("На этом аккаунте нет подписки Recode или она закончилась. Ключ покупается на FunPay и активируется в личном кабинете на сайте.", "This account has no Recode subscription, or it has ended. A key is bought on FunPay and activated in your account on the site."),
                vec![w("site", t("Личный кабинет", "Your account"), 0), w("retry", t("Проверить снова", "Check again"), 0), w("switch", t("Другой аккаунт", "Another account"), 0), w("quit", t("Выход", "Quit"), 2)],
            )
        } else if said == crate::vault::no_tpm() {
            self.tpm();
            return;
        } else if said == crate::vault::other_device() || said == crate::vault::device_taken() {
            let from = v["reset_from"].as_i64().unwrap_or(0);
            let can = from <= unix_now();
            let text = if said == crate::vault::device_taken() {
                t("Этот компьютер уже привязан к другому аккаунту.", "This computer is tied to another account.")
            } else if can {
                t("Подписка привязана к другому компьютеру. Сбросить привязку можно раз в сутки — тогда она перейдёт на этот.", "The subscription is tied to another computer. It can be freed once a day: then it moves to this one.")
            } else {
                t(&format!("Подписка привязана к другому компьютеру. Сбросить привязку можно после {}.", date(from)), &format!("The subscription is tied to another computer. It can be freed after {}.", date(from)))
            };
            let mut words = Vec::new();
            if can && said == crate::vault::other_device() {
                words.push(w("reset", t("Сбросить привязку", "Free it"), 0));
            }
            words.push(w("switch", t("Другой аккаунт", "Another account"), 0));
            words.push(w("quit", t("Выход", "Quit"), 2));
            (t("Другой компьютер", "Another computer"), t("Привязка", "Tied"), text, words)
        } else if said == crate::vault::banned() {
            let reason = v["reason"].as_str().unwrap_or("").trim().to_string();
            let mut text = if reason.is_empty() { String::new() } else { format!("{}: {reason}.", t("Причина", "Reason")) };
            if let Some(u) = v["until"].as_i64() {
                text.push_str(&t(&format!(" До {}.", date(u)), &format!(" Until {}.", date(u))));
            }
            (t("Аккаунт", "Account"), t("Заблокирован", "Banned"), text.trim().to_string(), vec![w("switch", t("Другой аккаунт", "Another account"), 0), w("quit", t("Выход", "Quit"), 2)])
        } else if said == crate::vault::offline() || said == crate::vault::busy() || said == crate::vault::slow() {
            (
                t("Связь", "Connection"),
                t("Нет связи с сервером", "No connection to the server"),
                t("Проверь интернет. Если он есть — сервер Cursed ненадолго недоступен.", "Check your internet. If it works, the Cursed server is away for a moment."),
                vec![w("retry", t("Повторить", "Try again"), 0), w("quit", t("Выход", "Quit"), 2)],
            )
        } else if said == crate::vault::stub() {
            (
                t("Запуск", "Start"),
                t("Не вышло", "It did not work"),
                {
                    let more = v["detail"].as_str().unwrap_or("").trim().to_string();
                    if more.is_empty() {
                        t("Сервер не отдал библиотеку запуска. Нажми «Играть» ещё раз.", "The server did not hand out the library to start with. Press Play again.")
                    } else {
                        more
                    }
                },
                vec![w("retry", t("Повторить", "Try again"), 0), w("quit", t("Выход", "Quit"), 2)],
            )
        } else {
            (t("Ошибка", "Error"), t("Не вышло", "It did not work"), format!("{said} ({})", f.detail()), vec![w("retry", t("Повторить", "Try again"), 0), w("quit", t("Выход", "Quit"), 2)])
        };
        self.go(Screen::Message { cap, title, text, words });
    }

    fn done(&mut self, d: Done) {
        match d {
            Done::Device(v) => {
                self.device = Some(v);
                if let Some(f) = self.after_device.take() {
                    f(self);
                }
            }
            Done::Link(r) => match r {
                Ok(v) => {
                    let url = v["url"].as_str().unwrap_or("").to_string();
                    open_url(&url);
                    self.go(Screen::Linking { code: v["code"].as_str().unwrap_or("").into(), key: v["key"].as_str().unwrap_or("").into(), url, state: "waiting" });
                    self.poll_at = self.now() + 2.0;
                }
                Err(f) => {
                    let state = if f.error() == "offline" { "offline" } else { "gone" };
                    self.go(Screen::Linking { code: String::new(), key: String::new(), url: String::new(), state });
                }
            },
            Done::Poll(r) => {
                let Screen::Linking { code, key, url, .. } = self.screen.clone() else { return };
                match r {
                    Ok(Some(v)) => {
                        let s = store::Session { token: v["token"].as_str().unwrap_or("").into(), uid: v["uid"].as_i64().unwrap_or(0), login: v["login"].as_str().unwrap_or("").into() };
                        s.save();
                        super::log(&format!("signed in as {}", s.login));
                        self.session = Some(s);
                        bring_to_front(self.hwnd);
                        self.check();
                    }
                    Ok(None) => self.poll_at = self.now() + 2.0,
                    Err(f) => {
                        let state = match f.error() {
                            "link-denied" => "denied",
                            "offline" => "offline",
                            _ => "gone",
                        };
                        self.go(Screen::Linking { code, key, url, state });
                    }
                }
            }
            Done::Start(r) => match r {
                Ok(v) => match sync::verify(v["manifest"].as_str().unwrap_or(""), v["sig"].as_str().unwrap_or("")) {
                    Ok(m) => {
                        if let Some(s) = self.session.as_mut() {
                            if let Some(login) = v["login"].as_str() {
                                s.login = login.to_string();
                            }
                        }
                        // a newer loader in the build: it takes this one's place and starts
                        if let Some(entry) = install::wants_update(&m) {
                            let token = self.session.as_ref().map(|s| s.token.clone()).unwrap_or_default();
                            match install::fetch_update(&entry, &token).and_then(|new| install::swap_and_restart(&new)) {
                                Ok(()) => {
                                    self.quit = true;
                                    return;
                                }
                                Err(e) => super::log(&format!("the loader's update: {e}")),
                            }
                        }
                        self.manifest = Some(m);
                        self.started = Some(v);
                        if !matches!(self.screen, Screen::Sync) && !self.hold_screen {
                            self.go(Screen::Menu);
                        }
                    }
                    Err(e) => {
                        super::log(&format!("the build's manifest: {e}"));
                        self.refused(&api::Fail::Refused { status: 0, error: "manifest".into(), body: Value::Null });
                    }
                },
                Err(f) => self.refused(&f),
            },
            Done::Synced(r) => {
                if self.play.is_none() {
                    return;
                }
                match r {
                    Ok(()) => {
                        let token = self.session.as_ref().map(|s| s.token.clone()).unwrap_or_default();
                        let device = self.device.clone().unwrap_or(Value::Null);
                        let home = store::home();
                        let m = self.manifest.clone();
                        self.spawn(move || {
                            let v = match api::start(&token, &device) {
                                Ok(v) => v,
                                Err(f) => return Done::Ticket(Err(f)),
                            };
                            // a master pair on the server: this start's own library goes in place before the game
                            match m.as_ref().map(|m| super::own_library(&token, &v, &home, m)).unwrap_or(Ok(None)) {
                                Ok(_) => Done::Ticket(Ok(v)),
                                Err(f) => Done::Ticket(Err(f)),
                            }
                        });
                    }
                    Err(e) => {
                        super::log(&format!("the build: {e}"));
                        let now = self.epoch.elapsed().as_secs_f32();
                        let ru = ru(&self.settings);
                        let p = self.play.as_mut().unwrap();
                        if p.back_at.is_none() {
                            p.back_at = Some(now);
                            p.failed = (!p.progress.cancel.load(Ordering::Relaxed)).then(|| {
                                if sync::is_network(&e) {
                                    (if ru { "Нет связи с сервером" } else { "No connection" }.to_string(), if ru { "Скачивание прервалось. Проверь интернет и нажми «Играть» ещё раз — продолжится с того же места." } else { "The download broke off. Check your internet and press Play again: it goes on from where it was." }.to_string())
                                } else {
                                    (if ru { "Не скачалось" } else { "It did not come" }.to_string(), e.clone())
                                }
                            });
                        }
                    }
                }
            }
            Done::Ticket(r) => match r {
                Ok(v) => {
                    if let Some(p) = self.play.as_mut() {
                        p.ticket = Some(v);
                    }
                }
                Err(f) => {
                    if let Some(p) = self.play.as_mut() {
                        p.back_at = Some(self.epoch.elapsed().as_secs_f32());
                    }
                    self.refused(&f);
                }
            },
            Done::Reset(r) => match r {
                Ok(_) => self.check(),
                Err(f) => {
                    if f.error() == "wait" {
                        let from = f.body()["reset_from"].as_i64().unwrap_or(0);
                        let t = tf(ru(&self.settings));
                        self.go(Screen::Message {
                            cap: t("Привязка", "Tied"),
                            title: t("Пока нельзя", "Not yet"),
                            text: t(&format!("Сбросить привязку можно раз в сутки — следующий раз после {}.", date(from)), &format!("The computer can be freed once a day: next after {}.", date(from))),
                            words: vec![("back-menu".into(), t("Назад", "Back"), 0)],
                        });
                    } else {
                        self.refused(&f);
                    }
                }
            },
        }
    }

    /// What the menus asked for.
    fn act(&mut self, a: &str) {
        let (word, arg) = a.split_once(' ').unwrap_or((a, ""));
        match word {
            "quit" => self.quit = true,
            "install" => {
                let home = store::home();
                if let Err(e) = store::choose(&home) {
                    super::log(&format!("the folder {}: {e}", home.display()));
                    return;
                }
                // the loader goes to its place in it and starts from there
                if install::ensure_installed() {
                    self.quit = true;
                } else {
                    self.after_setup();
                }
            }
            "browse" => {
                let title = self.t("Папка для Cursed Recode", "A folder for Cursed Recode");
                if let Some(mut p) = unsafe { pick_folder(self.hwnd, &title) } {
                    // a folder that has things of its own in it: Recode goes into a folder of its own there
                    let ours = p.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("Cursed Recode"));
                    let empty = std::fs::read_dir(&p).map(|mut d| d.next().is_none()).unwrap_or(true);
                    if !ours && !empty {
                        p = p.join("Cursed Recode");
                    }
                    let _ = store::choose(&p);
                    // the page anew (the folder it shows)
                    self.page.1 = String::new();
                }
            }
            "link" => {
                self.with_device(|a| {
                    let device = a.device.clone().unwrap_or(Value::Null);
                    a.spawn(move || Done::Link(api::link(&device)));
                });
            }
            "reopen" => {
                if let Screen::Linking { url, .. } = &self.screen {
                    open_url(url);
                }
            }
            "cancel" => self.go(if self.session.is_some() { Screen::Menu } else { Screen::SignIn }),
            "retry" => self.check(),
            "switch" => {
                store::Session::forget();
                self.session = None;
                self.started = None;
                self.go(Screen::SignIn);
            }
            "reset" | "press" if word == "reset" || arg == "10" => {
                let Some(s) = self.session.clone() else { return };
                let device = self.device.clone().unwrap_or(Value::Null);
                self.go(Screen::Busy);
                self.spawn(move || Done::Reset(api::reset(&s.token, &device)));
            }
            "play" | "continue" => self.play(if word == "continue" { game::last_server(&store::home()).map(|s| s.1) } else { None }),
            "cancel-play" => {
                if let Some(p) = self.play.as_mut() {
                    p.progress.cancel.store(true, Ordering::Relaxed);
                }
            }
            "settings" => self.go(Screen::Settings { section: "game".into() }),
            "links" => self.go(Screen::Links),
            "account" if self.session.is_some() => self.go(Screen::Account),
            "back" | "back-menu" | "close" => self.go(if self.session.is_some() && self.started.is_some() { Screen::Menu } else if self.session.is_some() { Screen::Busy } else { Screen::SignIn }),
            "site" => open_url(&format!("{SITE}/#/dashboard/recode")),
            "open-site" => open_url(SITE),
            "open-tg" => open_url(TELEGRAM),
            "open-discord" => open_url(DISCORD),
            "media" => crate::gui::media::control(arg),
            "section" => self.go(Screen::Settings { section: arg.to_string() }),
            "slide" => {
                let mut it = arg.split(' ');
                if let (Some("1"), Some(v)) = (it.next(), it.next().and_then(|v| v.parse::<f32>().ok())) {
                    let most = store::memory_most();
                    let mb = store::MEMORY_LEAST as f32 + v.clamp(0.0, 1.0) * (most - store::MEMORY_LEAST) as f32;
                    // in steps of half a gigabyte
                    self.settings.memory = ((mb / 512.0).round() * 512.0) as u32;
                    self.settings.memory = self.settings.memory.clamp(store::MEMORY_LEAST, most);
                    self.settings.save();
                }
            }
            "cycle" => {
                let mut it = arg.split(' ');
                if let (Some("2"), Some(to)) = (it.next(), it.next()) {
                    self.settings.lang = if to == "1" { "en".into() } else { "ru".into() };
                    self.settings.save();
                    crate::menus::lang::set(if ru(&self.settings) { "ru_ru" } else { "en_us" });
                }
            }
            "press" => match arg {
                "20" => open_url(&store::home().to_string_lossy()),
                "21" => {
                    // every file read again on the next Play (what was seen of them is forgotten)
                    let _ = std::fs::remove_file(store::dir().join("files.json"));
                    self.play(None);
                }
                "22" => {
                    let title = self.t("Новая папка для Cursed Recode", "A new folder for Cursed Recode");
                    if let Some(p) = unsafe { pick_folder(self.hwnd, &title) } {
                        let p = if p.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("Cursed Recode")) { p } else { p.join("Cursed Recode") };
                        if store::choose(&p).is_ok() && install::ensure_installed() {
                            self.quit = true;
                        }
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    /// After the folder: signed in already, or not.
    fn after_setup(&mut self) {
        self.session = store::Session::load();
        if self.session.is_some() {
            self.check();
        } else {
            self.go(Screen::SignIn);
        }
    }

    /// Play: the build made the same as the server's (the valley into its outlines meanwhile), then the game.
    fn play(&mut self, server: Option<String>) {
        let (Some(s), Some(m)) = (self.session.clone(), self.manifest.clone()) else { return };
        if self.play.is_some() {
            return;
        }
        let progress = sync::shared();
        self.play = Some(Play { since: self.now(), progress: progress.clone(), server, ticket: None, back_at: None, failed: None });
        self.go(Screen::Sync);
        let device = self.device.clone().unwrap_or(Value::Null);
        self.spawn(move || {
            let home = store::home();
            let renew = || api::start(&s.token, &device).is_ok();
            Done::Synced(sync::run(&home, &m, &s.token, &renew, &progress))
        });
    }
}

fn bring_to_front(hwnd: HWND) {
    unsafe {
        ShowWindow(hwnd, SW_RESTORE);
        SetForegroundWindow(hwnd);
    }
}

// ------------------------------------------------------------------------------------------------- the frames

impl App {
    /// The page up to date with the screen (a new page when the screen changed, its data otherwise).
    fn sync_page(&mut self) {
        let (kind, key, data) = self.page_of();
        let text = data.to_string();
        if kind == "none" {
            if self.page.0 != 0 {
                crate::menus::close(self.page.0);
                self.page = (0, key, String::new());
                self.page_closed = Some(self.now());
            } else if self.page_closed.is_none() {
                self.page_closed = Some(self.now());
            }
            return;
        }
        self.page_closed = None;
        if self.page.0 == 0 || self.page.1 != key {
            if self.page.0 != 0 {
                crate::menus::close(self.page.0);
            }
            self.next_id += 1;
            crate::menus::open(self.next_id, kind, &text);
            self.page = (self.next_id, key, text);
        } else if self.page.2 != text {
            crate::menus::data(self.page.0, &text);
            self.page.2 = text;
        }
    }

    fn input(&mut self) {
        let events: Vec<In> = std::mem::take(&mut *INPUT.lock().unwrap());
        for e in events {
            match e {
                In::Move(x, y) => {
                    self.mouse = [x, y];
                    crate::menus::mouse(x, y, 0, 0);
                }
                In::Button(b, a, x, y) => {
                    self.mouse = [x, y];
                    crate::menus::mouse(x, y, b, a);
                }
                In::Wheel(d) => crate::menus::scroll(d),
                In::Key(k, m) => crate::menus::key(&k, m),
                In::Char(c) => crate::menus::typed(c),
                In::Close => {
                    if self.play.is_none() {
                        self.quit = true;
                    }
                }
            }
        }
    }

    /// The launch's numbers for this frame: the menu's valley (or its outlines while the build comes), as the game's
    /// main menu makes them (cursed.recode.launch.LaunchScreen).
    fn inputs(&mut self, said: &[f32; 64], w: f32, h: f32, dt: f32) -> Inputs {
        let d = self.now();
        let toward = |v: f32, to: f32, rate: f32| v + (to - v) * (dt * rate).min(1.0);
        let (cx, cy) = (self.mouse[0] / w.max(1.0), self.mouse[1] / h.max(1.0));
        self.look[0] = toward(self.look[0], (cx.clamp(0.0, 1.0) - 0.5) * 0.05, 1.8);
        self.look[1] = toward(self.look[1], -(cy.clamp(0.0, 1.0) - 0.5) * 0.028, 1.8);
        self.cur[0] = toward(self.cur[0], cx, 5.0);
        self.cur[1] = toward(self.cur[1], cy, 5.0);
        self.cur[2] = toward(self.cur[2], 1.0, 2.0);
        // sheet lightning now and then, and a press lights the sky a moment
        let now = self.epoch.elapsed().as_secs_f64();
        if said[19] != self.clicks {
            self.clicks = said[19];
            self.flash_at = now - 0.05;
        }
        if self.next_flash == 0.0 {
            self.next_flash = now + 4.0 + rand01() * 5.0;
        }
        if now >= self.next_flash {
            self.flash_at = now;
            self.next_flash = now + 7.0 + rand01() * 9.0;
        }
        let pulse = |y: f64| if y < 0.0 { 0.0 } else { (-y * 6.5).exp() * (1.0 - (-y * 45.0).exp()) };
        let x = now - self.flash_at;
        let flash = ((pulse(x) + 0.55 * pulse(x - 0.2) + 0.3 * pulse(x - 0.45)) * 0.9) as f32;
        let mut hover = [[0f32; 5]; 2];
        for (k, h) in hover.iter_mut().enumerate() {
            h.copy_from_slice(&said[1 + k * 5..6 + k * 5]);
        }
        let main = crate::launch::valley();
        let (mut pose, mut knobs): ([f32; 5], [f32; 24]) = (main[..5].try_into().unwrap(), main[5..].try_into().unwrap());
        if said[24] > 1.0 {
            pose.copy_from_slice(&said[20..25]);
            knobs.copy_from_slice(&said[25..49]);
        }
        // Play: the valley into its outlines, their light the build's (back into the valley when cancelled)
        let (phase, pt, load) = match self.play.as_ref() {
            Some(p) => {
                let x = match p.back_at {
                    Some(b) => (b - p.since).min(crate::launch::ink::ie::LINES_TO) - (d - b),
                    None => d - p.since,
                };
                let pr = &p.progress;
                let total = pr.total.load(Ordering::Relaxed);
                let frac = if p.ticket.is_some() {
                    1.0
                } else if total > 0 {
                    0.1 + 0.9 * pr.shown() as f32 / total as f32
                } else {
                    let n = pr.to_check.load(Ordering::Relaxed).max(1);
                    0.1 * pr.checked.load(Ordering::Relaxed) as f32 / n as f32
                };
                (crate::launch::timeline::Phase::Enter, x.max(0.0), BUILD_LIGHT * frac.clamp(0.0, 1.0) / 0.95)
            }
            None => (crate::launch::timeline::Phase::Run, MENU_T, 1.0),
        };
        Inputs {
            t: MENU_T,
            fx: d,
            life: d,
            dt,
            launched: true,
            gather: 0.0,
            smoke_reset: false,
            cursor: [self.cur[0], self.cur[1], self.cur[2]],
            look: self.look,
            boost: said[11],
            flash_c: flash,
            fade: 1.0,
            ui: (h / 1300.0).max(1.0),
            left: said[13],
            foot_top: said[14],
            hover,
            hover_sparks: 1.0,
            lines: [[0.0; 8]; LINES],
            icons: [[0.0, 0.0, 0.0, -1.0]; 3],
            icon_light: [0.0; 4],
            disc: [0.0; 8],
            no_rays: false,
            no_bloom: false,
            menus_fire: true,
            knobs,
            pose,
            phase,
            pt: if phase == crate::launch::timeline::Phase::Run { MENU_T } else { pt },
            load,
            loaded_at: -1.0,
            world: None,
            quit: -1.0,
        }
    }

    /// The game's options the valley follows (the game's folder's, its defaults before there is one).
    fn options(&self) -> [f32; 8] {
        crate::twin::valley_options(&store::home()).unwrap_or([12.0, 2.0, 128.0, 0.0, 0.5, 70.0, 1.0, 5.0])
    }
}

pub(super) fn rand01() -> f64 {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos());
    (n % 10_000) as f64 / 10_000.0
}

// ------------------------------------------------------------------------------------------------- the tray

/// The tray's icon while the game plays (her word: the loader waits there till the game is closed).
struct Tray {
    hwnd: HWND,
}

impl Tray {
    unsafe fn new(owner: HWND, tip: &str) -> Tray {
        use windows_sys::Win32::UI::Shell::{Shell_NotifyIconW, NIF_ICON, NIF_TIP, NIM_ADD, NOTIFYICONDATAW};
        let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = owner;
        nid.uID = 1;
        nid.uFlags = NIF_ICON | NIF_TIP;
        nid.hIcon = SendMessageW(owner, WM_GETICON, ICON_SMALL as WPARAM, 0) as _;
        let t = wide(tip);
        let n = t.len().min(nid.szTip.len());
        nid.szTip[..n].copy_from_slice(&t[..n]);
        Shell_NotifyIconW(NIM_ADD, &nid);
        Tray { hwnd: owner }
    }

    unsafe fn remove(self) {
        use windows_sys::Win32::UI::Shell::{Shell_NotifyIconW, NIM_DELETE, NOTIFYICONDATAW};
        let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = self.hwnd;
        nid.uID = 1;
        Shell_NotifyIconW(NIM_DELETE, &nid);
    }
}

// ------------------------------------------------------------------------------------------------- the life

/// The window's whole life (loader::main): see the module.
pub fn run() -> i32 {
    unsafe {
        windows_sys::Win32::UI::HiDpi::SetProcessDpiAwarenessContext(windows_sys::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    crate::twin::app_id();
    // the crows were startled off the tree a while before the loader opened (its valley is alive, not a drawing): they
    // fly on through the launch into the game the same
    crate::launch::ink::set_startled(crate::launch::ink::life(0.0) - 20.0);
    let settings = store::Settings::load();
    crate::menus::lang::set(if settings.lang == "en" { "en_us" } else { "ru_ru" });
    let at = std::env::var("CURSED_DEV_WINDOW_AT").ok().and_then(|v| {
        let mut p = v.split(',');
        Some([p.next()?.trim().parse().ok()?, p.next()?.trim().parse().ok()?])
    });
    let place = crate::twin::place(0, 0, at);
    let hwnd = unsafe { make_window(&place) };
    if hwnd.is_null() {
        super::log("no window");
        return 1;
    }
    let (tx, rx) = channel();
    let mut app = App {
        hwnd,
        gfx: None,
        ease: crate::chrome::Ease::default(),
        settings,
        session: None,
        device: None,
        started: None,
        manifest: None,
        screen: Screen::Busy,
        page: (0, String::new(), String::new()),
        page_closed: None,
        next_id: 0,
        tx,
        rx,
        epoch: Instant::now(),
        mouse: [-1.0, -1.0],
        cur: [0.5, 0.5, 0.0, 0.0, 0.0],
        look: [0.0; 2],
        flash_at: -100.0,
        next_flash: 0.0,
        clicks: 0.0,
        play: None,
        shown: false,
        shown_at: 0.0,
        quit: false,
        hold_screen: false,
        poll_at: f32::MAX,
        after_device: None,
    };
    // this computer's parts take a moment (the TPM, the disk): asked aside
    app.spawn(|| Done::Device(crate::device::parts_json()));
    let dev = std::env::var("CURSED_LOADER_DEV").is_ok_and(|v| v == "1") || std::env::var_os("CURSED_LOADER_HOME").is_some();
    if store::chosen().is_none() && !dev {
        app.go(Screen::Setup);
    } else {
        app.after_setup();
    }
    // tests (a build from the sources): a screen shown at once (CURSED_LOADER_SCREEN=setup|tpm|signin|account|links)
    if !crate::license::RELEASE {
        app.hold_screen = std::env::var_os("CURSED_LOADER_SCREEN").is_some();
        match std::env::var("CURSED_LOADER_SCREEN").as_deref() {
            Ok("setup") => app.go(Screen::Setup),
            Ok("tpm") => app.tpm(),
            Ok("signin") => app.go(Screen::SignIn),
            Ok("links") => app.go(Screen::Links),
            _ => {}
        }
    }
    let mut last = Instant::now();
    while !app.quit {
        // the window's messages
        unsafe {
            let mut msg: MSG = std::mem::zeroed();
            while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        while let Ok(d) = app.rx.try_recv() {
            app.done(d);
        }
        if let Screen::Linking { code, key, state: "waiting", .. } = &app.screen {
            if app.now() >= app.poll_at {
                app.poll_at = f32::MAX;
                let (code, key) = (code.clone(), key.clone());
                app.spawn(move || Done::Poll(api::link_poll(&code, &key)));
            }
        }
        app.input();
        // Play: back from the outlines (cancelled or failed), or on into the game
        let state = app.play.as_ref().map(|p| (p.back_at, p.ticket.is_some(), p.since));
        if let Some((back_at, ticket, since)) = state {
            if let Some(b) = back_at {
                if app.now() - b > 1.0 {
                    let failed = app.play.take().and_then(|p| p.failed);
                    if let Some((title, text)) = failed {
                        let t = tf(ru(&app.settings));
                        app.go(Screen::Message { cap: t("Игра", "The game"), title, text, words: vec![("play".into(), t("Играть", "Play"), 0), ("back-menu".into(), t("Назад", "Back"), 2)] });
                    } else if matches!(app.screen, Screen::Sync) {
                        app.go(Screen::Menu);
                    }
                }
            } else if ticket && app.now() - since > crate::launch::ink::ie::LINES_TO + 0.5 && app.page.0 == 0 && app.page_gone_at(0.4) {
                app.launch_game();
                last = Instant::now();
                continue;
            }
        }
        if unsafe { IsIconic(hwnd) } != 0 {
            std::thread::sleep(Duration::from_millis(30));
            continue;
        }
        let dt = last.elapsed().as_secs_f32().min(0.1);
        last = Instant::now();
        app.sync_page();
        app.draw(dt);
    }
    if app.page.0 != 0 {
        crate::menus::close(app.page.0);
    }
    if let Some(g) = app.gfx.take() {
        g.destroy();
    }
    unsafe {
        DestroyWindow(hwnd);
    }
    0
}

impl App {
    /// The menus' page has sunk away (`secs` after it was closed).
    fn page_gone_at(&self, secs: f32) -> bool {
        self.page_closed.is_some_and(|t| self.now() - t > secs)
    }

    fn draw(&mut self, dt: f32) {
        if self.gfx.is_none() {
            match Gfx::new() {
                Some(g) => self.gfx = Some(g),
                None => {
                    super::log("no Vulkan device");
                    self.quit = true;
                    return;
                }
            }
        }
        let (cw, ch) = client_size(self.hwnd);
        let dpi = unsafe { windows_sys::Win32::UI::HiDpi::GetDpiForWindow(self.hwnd) }.max(96);
        let bar_h = crate::chrome::bar_height(dpi) as u32;
        BAR.store(bar_h as i32, Ordering::Relaxed);
        let hwnd = self.hwnd;
        let g = self.gfx.as_mut().unwrap();
        if !g.size(hwnd, cw, ch, bar_h) {
            self.quit = true;
            return;
        }
        let ready = g.ready();
        let (w, h) = (cw as f32, ch.saturating_sub(bar_h) as f32);
        let mut inp = [0f32; 32];
        inp[15] = 0.5;
        inp[16..24].copy_from_slice(&self.options());
        let now = self.now();
        let (id, said) = crate::menus::frame(w, h, now, &inp, false);
        let built = if id != 0 { crate::gui::take(id) } else { None };
        for line in crate::menus::poll().lines().map(str::to_string).collect::<Vec<_>>() {
            self.act(&line);
        }
        let i = self.inputs(&said, w, h, dt);
        let look = self.ease.next(cw as i32).clone();
        // the window comes once the valley is there, the picture fading up from black
        let fade = if ready { ((now - self.shown_at) / 0.45).clamp(0.0, 1.0) } else { 0.0 };
        let g = self.gfx.as_mut().unwrap();
        if ready {
            g.frame(&i, built, &look, fade);
        }
        if ready && !self.shown {
            self.shown = true;
            self.shown_at = now;
            unsafe {
                ShowWindow(hwnd, SW_SHOWNORMAL);
                SetForegroundWindow(hwnd);
            }
        }
    }

    /// The build is in place and the ticket came: the game starts, and this process becomes the window before the game
    /// for it (its story from the outlines this window shows now); this window hides once that one is up over it, an
    /// icon in the tray meanwhile; when the game has gone (its farewell played), the menu again.
    fn launch_game(&mut self) {
        let Some(mut p) = self.play.take() else { return };
        let ticket = p.ticket.take().unwrap_or(Value::Null);
        let (Some(m), Some(s)) = (self.manifest.clone(), self.session.clone()) else { return };
        let home = store::home();
        let name = format!("CursedRecodeLoader-{}", std::process::id());
        let place = crate::twin::place_of(self.hwnd);
        let shift = self.now() - STORY_FROM;
        crate::launch::ink::set_light_base(BUILD_LIGHT);
        let log = Some(store::dir().join("twin.log"));
        let mute = std::env::var_os("CURSED_DEV_MUTE").is_some();
        let home_text = home.to_string_lossy().to_string();
        let Some(link) = crate::twin::link_for_loader(&name, place, STORY_FROM, shift, self.hwnd as isize, Some(&home_text), mute, log) else {
            super::log("the window before the game: no link");
            return;
        };
        // the pair ticket this start handed the game: a note of a hand on its memory is signed with
        // this same one (TOUCH.md §3)
        let pair_ticket = ticket["ticket"].as_str().unwrap_or("");
        let running = game::start(&home, &m, self.settings.memory, &s.login, pair_ticket, ticket["start_ticket"].as_str().unwrap_or(""), Some(&name), p.server.as_deref());
        if let Ok(r) = running.as_ref() {
            link.game_started(r.child.id());
        }
        let mut running = match running {
            Ok(r) => r,
            Err(e) => {
                super::log(&format!("the game did not start: {e}"));
                let t = tf(ru(&self.settings));
                self.go(Screen::Message { cap: t("Игра", "The game"), title: t("Не запустилась", "It did not start"), text: e, words: vec![("play".into(), t("Играть", "Play"), 0), ("back-menu".into(), t("Назад", "Back"), 2)] });
                return;
            }
        };
        let pid = running.child.id();
        super::log(&format!("the game started, pid {pid}"));
        // the window before the game: its own thread; this one keeps drawing the outlines till that one is up over it
        let twin = std::thread::Builder::new().name("Cursed twin".into()).spawn(move || crate::twin::serve_loader(link)).ok();
        let mut last = Instant::now();
        let mut tray: Option<Tray> = None;
        loop {
            unsafe {
                let mut msg: MSG = std::mem::zeroed();
                while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
            let _ = std::mem::take(&mut *INPUT.lock().unwrap());
            let visible = unsafe { IsWindowVisible(self.hwnd) } != 0;
            if visible {
                // the same outlines as the other draws, till it takes over
                let dt = last.elapsed().as_secs_f32().min(0.1);
                last = Instant::now();
                self.play = Some(Play { since: p.since, progress: p.progress.clone(), server: None, ticket: Some(Value::Null), back_at: None, failed: None });
                self.draw(dt);
                self.play = None;
            } else {
                if let Some(g) = self.gfx.take() {
                    g.destroy();
                    tray = Some(unsafe { Tray::new(self.hwnd, "Cursed Recode") });
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            if twin.as_ref().is_none_or(|t| t.is_finished()) {
                break;
            }
        }
        if let Some(t) = twin {
            let _ = t.join();
        }
        if let Some(t) = tray.take() {
            unsafe { t.remove() };
        }
        let (code, report, source) = game::until_it_goes(&mut running, &home, &m, Some(&s.token), Some(pair_ticket));
        super::log(&format!("the game went, exit {code:?}"));
        crate::launch::ink::set_light_base(0.0);
        if let Some(report) = report {
            // a touch is its own kind of note and travels under its MAC; anything else is the old
            // crash path, untouched
            let is_touch = source == crate::vault::touch_source();
            let sent = if is_touch {
                api::touch(&s.token, m.build, pair_ticket, report.as_bytes()).is_ok()
            } else {
                api::crash(&s.token, m.build, source, report.as_bytes()).is_ok()
            };
            let t = tf(ru(&self.settings));
            self.go(Screen::Message {
                cap: t("Игра", "The game"),
                title: if source == "protect" || is_touch {
                    t("Игра была закрыта", "The game was closed")
                } else {
                    t("Закрылась с ошибкой", "It closed with an error")
                },
                text: if sent { t("Отчёт ушёл нам — разберёмся.", "The report came to us: we will look into it.") } else { t("Отчёт не отправился.", "The report did not go.") },
                words: vec![("play".into(), t("Играть", "Play"), 0), ("back-menu".into(), t("Назад", "Back"), 2)],
            });
        } else {
            self.go(Screen::Menu);
        }
        // the menu again, in a window made anew (the game's process had the device; this one's went)
        self.page = (0, String::new(), String::new());
        self.page_closed = None;
        self.shown = false;
        unsafe {
            ShowWindow(self.hwnd, SW_HIDE);
        }
        // the subscription may have changed meanwhile: asked again
        self.check_quiet();
    }

    /// The start asked again without leaving the screen (after the game).
    fn check_quiet(&mut self) {
        let Some(s) = self.session.clone() else { return };
        let device = self.device.clone().unwrap_or(Value::Null);
        self.spawn(move || Done::Start(api::start(&s.token, &device)));
    }
}
