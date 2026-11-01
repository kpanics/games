//! The menus' pace without changing the swap chain: out of a world the present thread waits for the monitor's refresh
//! after each present (the output's vertical blank), so the menus get one frame a refresh, evenly — as FIFO gave them.
//! The swap chain keeps the game's present mode all along: made again in another mode (FIFO in the menus, IMMEDIATE in
//! a world) NVIDIA's swap chain on DXGI came back slow for the whole session, every frame's GPU work half as long
//! again (870 FPS against 1290 measured, the same frame).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1, IDXGIOutput};

/// Whether the frames go at the monitor's pace (LaunchScreen.paced: the menus out of a world).
pub static PACED: AtomicBool = AtomicBool::new(false);

/// The output of the monitor the window was last on, found once per monitor.
struct Output {
    monitor: *mut core::ffi::c_void,
    output: Option<IDXGIOutput>,
}

// The output is used by one thread at a time, under the lock.
unsafe impl Send for Output {}

static OUTPUT: Mutex<Option<Output>> = Mutex::new(None);

fn find(monitor: *mut core::ffi::c_void) -> Option<IDXGIOutput> {
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1().ok()?;
        for i in 0.. {
            let Ok(adapter) = factory.EnumAdapters1(i) else { break };
            for j in 0.. {
                let Ok(o) = adapter.EnumOutputs(j) else { break };
                if o.GetDesc().is_ok_and(|d| d.Monitor.0 == monitor) {
                    return Some(o);
                }
            }
        }
    }
    None
}

/// After a present: when paced, until the next vertical blank of the window's monitor.
pub fn after_present(hwnd: isize) {
    if !PACED.load(Ordering::Relaxed) {
        return;
    }
    let monitor = unsafe { windows_sys::Win32::Graphics::Gdi::MonitorFromWindow(hwnd as *mut core::ffi::c_void, windows_sys::Win32::Graphics::Gdi::MONITOR_DEFAULTTONEAREST) };
    let mut slot = OUTPUT.lock().unwrap();
    if slot.as_ref().is_none_or(|o| o.monitor != monitor) {
        *slot = Some(Output { monitor, output: find(monitor) });
    }
    if let Some(o) = slot.as_ref().and_then(|o| o.output.as_ref()) {
        let _ = unsafe { o.WaitForVBlank() };
    }
}
