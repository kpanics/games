//! The game window's look: its title bar in our colours (dark, her icon, «Cursed Recode», the caption buttons), set on
//! the game's window through DWM — and the same bar drawn here with GDI, so the window before the game shows from the
//! first burned pixel the very frame the game's window will have. Nothing changes at the hand-over.

use windows_sys::Win32::Foundation::{COLORREF, HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

/// The bar and the window's edge (0x00BBGGRR).
pub const CAPTION: COLORREF = 0x0011_1111;
/// The title's colour.
pub const TEXT: COLORREF = 0x00E6_E6E6;
pub const TITLE: &str = "Cursed Recode";

/// The caption colour as 0..1 rgb.
pub fn caption_rgb() -> [f32; 3] {
    let c = CAPTION;
    [(c & 0xFF) as f32 / 255.0, ((c >> 8) & 0xFF) as f32 / 255.0, ((c >> 16) & 0xFF) as f32 / 255.0]
}

/// Win11 and later (the bar's colours and the round corners are attributes of DWM since build 22000).
pub fn windows11() -> bool {
    static W: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *W.get_or_init(|| {
        let mut v: windows_sys::Win32::System::SystemInformation::OSVERSIONINFOW = unsafe { std::mem::zeroed() };
        v.dwOSVersionInfoSize = std::mem::size_of_val(&v) as u32;
        // RtlGetVersion: GetVersionEx lies to programs without a manifest
        type F = unsafe extern "system" fn(*mut windows_sys::Win32::System::SystemInformation::OSVERSIONINFOW) -> i32;
        unsafe {
            let ntdll = windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(wide("ntdll.dll").as_ptr());
            match windows_sys::Win32::System::LibraryLoader::GetProcAddress(ntdll, c"RtlGetVersion".as_ptr() as *const u8) {
                Some(f) => {
                    let f: F = std::mem::transmute(f);
                    f(&mut v);
                    v.dwBuildNumber >= 22000
                }
                None => false,
            }
        }
    })
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Her icon at `size` px (the nearest image of the .ico not smaller than it).
pub unsafe fn icon(size: i32) -> HICON {
    const ICO: &[u8] = include_bytes!("../../assets/icons/recode.ico");
    let count = u16::from_le_bytes([ICO[4], ICO[5]]) as usize;
    let mut best: Option<(i32, usize, usize)> = None;
    for i in 0..count {
        let e = &ICO[6 + i * 16..6 + i * 16 + 16];
        let w = if e[0] == 0 { 256 } else { e[0] as i32 };
        let len = u32::from_le_bytes([e[8], e[9], e[10], e[11]]) as usize;
        let at = u32::from_le_bytes([e[12], e[13], e[14], e[15]]) as usize;
        let better = match best {
            None => true,
            Some((bw, _, _)) => (w >= size && (bw < size || w < bw)) || (bw < size && w > bw),
        };
        if better {
            best = Some((w, at, len));
        }
    }
    let Some((_, at, len)) = best else { return std::ptr::null_mut() };
    CreateIconFromResourceEx(ICO[at..at + len].as_ptr(), len as u32, 1, 0x0003_0000, size, size, LR_DEFAULTCOLOR)
}

/// Her icon on a window (the title bar, the taskbar, Alt+Tab), at its DPI.
pub unsafe fn set_icons(hwnd: HWND) {
    let dpi = windows_sys::Win32::UI::HiDpi::GetDpiForWindow(hwnd).max(96);
    let big = windows_sys::Win32::UI::HiDpi::GetSystemMetricsForDpi(SM_CXICON, dpi).max(32);
    let small = windows_sys::Win32::UI::HiDpi::GetSystemMetricsForDpi(SM_CXSMICON, dpi).max(16);
    for (which, size) in [(ICON_BIG, big), (ICON_SMALL, small)] {
        let i = icon(size);
        if !i.is_null() {
            SendMessageW(hwnd, WM_SETICON, which as usize, i as isize);
        }
    }
}

/// The game's window in our look: her icon, our title bar colours, no animation when it appears (it appears under the
/// window before the game, on the same picture). `transitions`: whether Windows animates it (back on after the hand-over).
pub unsafe fn style_game_window(hwnd: HWND, transitions: bool) {
    let set = |attr: i32, v: u32| {
        windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute(hwnd, attr as u32, &v as *const u32 as *const core::ffi::c_void, 4);
    };
    use windows_sys::Win32::Graphics::Dwm::*;
    set(DWMWA_TRANSITIONS_FORCEDISABLED, (!transitions) as u32);
    set(DWMWA_USE_IMMERSIVE_DARK_MODE, 1);
    if windows11() {
        set(DWMWA_CAPTION_COLOR, CAPTION);
        set(DWMWA_BORDER_COLOR, CAPTION);
        set(DWMWA_TEXT_COLOR, TEXT);
        set(DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND as u32);
    }
    set_icons(hwnd);
}

/// The game's window over the other windows. A window shown by a process in the background is put under the window
/// the player is in, and its activation refused; passing through the always-on-top band raises it regardless (as in
/// Cursed's window.rs bring_up). It stays under the window before the game, the only window over it then.
pub unsafe fn bring_up(hwnd: HWND) {
    let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE;
    SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, flags);
    SetWindowPos(hwnd, HWND_NOTOPMOST, 0, 0, 0, 0, flags);
    SetForegroundWindow(hwnd);
}

/// The game's window up minimised, not taking the focus, its taskbar button blinking until it is opened: the window
/// before the game was minimised when the game was ready (her word, 19.10.2026).
pub unsafe fn show_minimised(hwnd: HWND) {
    ShowWindow(hwnd, SW_SHOWMINNOACTIVE);
    let mut f = FLASHWINFO { cbSize: std::mem::size_of::<FLASHWINFO>() as u32, hwnd, dwFlags: FLASHW_TRAY | FLASHW_TIMERNOFG, uCount: 0, dwTimeout: 0 };
    FlashWindowEx(&mut f);
}

/// The bar as Windows 11 draws it for a window `width` px wide (its visible frame), `height` px high (from the frame's
/// top to the picture), at `scale` (DPI / 96): RGBA, rows from the top. The caption font and the caption buttons'
/// glyphs are the system's; sizes measured on a real window of the game.
pub unsafe fn render(width: i32, height: i32, scale: f32) -> Vec<u8> {
    let (w, h) = (width.max(1), height.max(1));
    let screen = GetDC(std::ptr::null_mut());
    let dc = CreateCompatibleDC(screen);
    let mut bmi: BITMAPINFO = std::mem::zeroed();
    bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth = w;
    bmi.bmiHeader.biHeight = -h;
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI_RGB;
    let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
    let dib = CreateDIBSection(screen, &bmi, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
    ReleaseDC(std::ptr::null_mut(), screen);
    let old = SelectObject(dc, dib);
    let brush = CreateSolidBrush(CAPTION);
    FillRect(dc, &RECT { left: 0, top: 0, right: w, bottom: h }, brush);
    DeleteObject(brush);
    let px = |v: f32| (v * scale).round() as i32;
    // her icon, small
    let isz = px(16.0);
    let ix = px(BAR.icon_x);
    let iy = (h - isz) / 2;
    let ic = icon(isz);
    if !ic.is_null() {
        DrawIconEx(dc, ix, iy, ic, isz, isz, 0, std::ptr::null_mut(), DI_NORMAL);
        DestroyIcon(ic);
    }
    SetBkMode(dc, TRANSPARENT as i32);
    // the title, in the caption font
    let mut ncm: NONCLIENTMETRICSW = std::mem::zeroed();
    ncm.cbSize = std::mem::size_of::<NONCLIENTMETRICSW>() as u32;
    let dpi = (96.0 * scale).round() as u32;
    windows_sys::Win32::UI::HiDpi::SystemParametersInfoForDpi(SPI_GETNONCLIENTMETRICS, ncm.cbSize, &mut ncm as *mut _ as *mut core::ffi::c_void, 0, dpi);
    let mut lf = ncm.lfCaptionFont;
    lf.lfQuality = CLEARTYPE_NATURAL_QUALITY as u8;
    let font = CreateFontIndirectW(&lf);
    let of = SelectObject(dc, font);
    SetTextColor(dc, TEXT);
    let title = wide(TITLE);
    let mut tr = RECT { left: ix + isz + px(BAR.text_gap), top: 0, right: w, bottom: h + px(BAR.text_dy) * 2 };
    DrawTextW(dc, title.as_ptr(), -1, &mut tr, DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
    SelectObject(dc, of);
    DeleteObject(font);
    // the caption buttons: minimise, maximise, close, each in its cell from the right
    let face = if windows11() { "Segoe Fluent Icons" } else { "Segoe MDL2 Assets" };
    let gf = CreateFontW(-px(10.0), 0, 0, 0, FW_NORMAL as i32, 0, 0, 0, DEFAULT_CHARSET as u32, OUT_DEFAULT_PRECIS as u32, CLIP_DEFAULT_PRECIS as u32, CLEARTYPE_NATURAL_QUALITY as u32, DEFAULT_PITCH as u32, wide(face).as_ptr());
    let og = SelectObject(dc, gf);
    SetTextColor(dc, 0x00FF_FFFF);
    let cell = px(BAR.button_w);
    for (n, glyph) in ['\u{E8BB}', '\u{E922}', '\u{E921}'].iter().enumerate() {
        let right = w - cell * n as i32;
        let mut r = RECT { left: right - cell, top: 0, right, bottom: h };
        let g = wide(&glyph.to_string());
        DrawTextW(dc, g.as_ptr(), -1, &mut r, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
    }
    SelectObject(dc, og);
    DeleteObject(gf);
    GdiFlush();
    // BGRA (GDI leaves alpha 0) → RGBA, opaque
    let src = std::slice::from_raw_parts(bits as *const u8, (w * h * 4) as usize);
    let mut out = vec![0u8; (w * h * 4) as usize];
    for (o, s) in out.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
        o.copy_from_slice(&[s[2], s[1], s[0], 255]);
    }
    SelectObject(dc, old);
    DeleteObject(dib);
    DeleteDC(dc);
    out
}

/// The bar's layout in px at 96 DPI (Windows 11), measured on the game's window.
struct Bar {
    icon_x: f32,
    text_gap: f32,
    text_dy: f32,
    button_w: f32,
}
const BAR: Bar = Bar { icon_x: 8.0, text_gap: 6.0, text_dy: 0.0, button_w: 46.0 };
