//! Dev (CURSED_DEV_PROFILE=seconds): a sampling profiler of the frame thread. A thread of its own suspends it every
//! 200 µs, reads where it is (its instruction pointer) and lets it go on; after the given seconds of the world the
//! samples are named through dbghelp and the PDB beside the DLL, and the functions the thread was in most go to the log.
//! Only the function it was in (no callers): enough to see where the thread's time goes.

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};

/// The frame thread's id, set when it starts (frame.rs).
pub static FRAME_THREAD: AtomicU32 = AtomicU32::new(0);

#[repr(C, align(16))]
struct Context {
    bytes: [u8; 1232],
}

const CONTEXT_CONTROL: u32 = 0x0010_0001;
const RIP_AT: usize = 0xF8;
const FLAGS_AT: usize = 0x30;

type GetThreadContext = unsafe extern "system" fn(*mut c_void, *mut Context) -> i32;
type SymInitializeW = unsafe extern "system" fn(*mut c_void, *const u16, i32) -> i32;
type SymFromAddrW = unsafe extern "system" fn(*mut c_void, u64, *mut u64, *mut SymbolInfoW) -> i32;
type SymSetOptions = unsafe extern "system" fn(u32) -> u32;

#[repr(C)]
struct SymbolInfoW {
    size_of_struct: u32,
    type_index: u32,
    reserved: [u64; 2],
    index: u32,
    size: u32,
    mod_base: u64,
    flags: u32,
    value: u64,
    address: u64,
    register: u32,
    scope: u32,
    tag: u32,
    name_len: u32,
    max_name_len: u32,
    name: [u16; 512],
}

fn proc(module: &str, name: &str) -> *const c_void {
    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};
    let m = std::ffi::CString::new(module).unwrap();
    let n = std::ffi::CString::new(name).unwrap();
    unsafe {
        let h = LoadLibraryA(m.as_ptr() as *const u8);
        if h.is_null() {
            return std::ptr::null();
        }
        GetProcAddress(h, n.as_ptr() as *const u8).map_or(std::ptr::null(), |f| f as *const c_void)
    }
}

/// The module an address is in, as name+offset (when no symbol names it).
fn module_of(addr: u64) -> String {
    use windows_sys::Win32::System::LibraryLoader::{GetModuleFileNameW, GetModuleHandleExW};
    unsafe {
        let mut h = std::ptr::null_mut();
        if GetModuleHandleExW(0x4 | 0x2, addr as usize as *const u16, &mut h) == 0 {
            return format!("0x{addr:x}");
        }
        let mut buf = [0u16; 260];
        let n = GetModuleFileNameW(h, buf.as_mut_ptr(), 260) as usize;
        let path = String::from_utf16_lossy(&buf[..n]);
        let name = path.rsplit('\\').next().unwrap_or(&path).to_string();
        format!("{name}+0x{:x}", addr - h as u64)
    }
}

/// Starts the profiler if CURSED_DEV_PROFILE asks for it (once).
pub fn start() {
    let Some(seconds) = std::env::var("CURSED_DEV_PROFILE").ok().and_then(|v| v.parse::<f64>().ok()) else { return };
    static STARTED: std::sync::Once = std::sync::Once::new();
    STARTED.call_once(|| {
        let _ = std::thread::Builder::new().name("Cursed profiler".into()).spawn(move || run(seconds));
    });
}

fn run(seconds: f64) {
    use windows_sys::Win32::System::Threading::{OpenThread, ResumeThread, SuspendThread, THREAD_GET_CONTEXT, THREAD_SUSPEND_RESUME};
    let get_context = proc("kernel32.dll", "GetThreadContext");
    if get_context.is_null() {
        return;
    }
    let get_context: GetThreadContext = unsafe { std::mem::transmute(get_context) };
    // wait for the frame thread and the world (the samples start 20 s in, past the loading)
    while FRAME_THREAD.load(Ordering::Relaxed) == 0 {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    std::thread::sleep(std::time::Duration::from_secs(20));
    let tid = FRAME_THREAD.load(Ordering::Relaxed);
    let thread = unsafe { OpenThread(THREAD_SUSPEND_RESUME | THREAD_GET_CONTEXT, 0, tid) };
    if thread.is_null() {
        return;
    }
    let mut counts: HashMap<u64, u32> = HashMap::new();
    let mut total = 0u32;
    let end = std::time::Instant::now() + std::time::Duration::from_secs_f64(seconds);
    let mut ctx = Context { bytes: [0; 1232] };
    while std::time::Instant::now() < end {
        unsafe {
            if SuspendThread(thread) != u32::MAX {
                ctx.bytes[FLAGS_AT..FLAGS_AT + 4].copy_from_slice(&CONTEXT_CONTROL.to_le_bytes());
                // nothing allocated while it is held (it may hold the heap's lock)
                let got = get_context(thread, &mut ctx) != 0;
                ResumeThread(thread);
                if got {
                    let rip = u64::from_le_bytes(ctx.bytes[RIP_AT..RIP_AT + 8].try_into().unwrap());
                    *counts.entry(rip).or_default() += 1;
                    total += 1;
                }
            }
        }
        let t = std::time::Instant::now();
        while t.elapsed() < std::time::Duration::from_micros(200) {
            std::hint::spin_loop();
        }
    }
    // names
    let init = proc("dbghelp.dll", "SymInitializeW");
    let from_addr = proc("dbghelp.dll", "SymFromAddrW");
    let options = proc("dbghelp.dll", "SymSetOptions");
    let mut by_name: HashMap<String, u32> = HashMap::new();
    unsafe {
        let process = windows_sys::Win32::System::Threading::GetCurrentProcess();
        if !options.is_null() {
            let o: SymSetOptions = std::mem::transmute(options);
            o(0x2 | 0x10); // SYMOPT_UNDNAME | SYMOPT_DEFERRED_LOADS
        }
        // (Rust's own backtraces may have initialized dbghelp already: then it says invalid parameter, and the modules
        // loaded since are made known with SymRefreshModuleList)
        let ok = !init.is_null() && {
            let i: SymInitializeW = std::mem::transmute(init);
            i(process, std::ptr::null(), 1) != 0 || windows_sys::Win32::Foundation::GetLastError() == 87
        };
        let refresh = proc("dbghelp.dll", "SymRefreshModuleList");
        if ok && !refresh.is_null() {
            let r: unsafe extern "system" fn(*mut c_void) -> i32 = std::mem::transmute(refresh);
            r(process);
        }
        eprintln!("[profile] symbols: init {ok} (error {})", windows_sys::Win32::Foundation::GetLastError());
        for (&rip, &n) in &counts {
            let mut name = module_of(rip);
            if ok && !from_addr.is_null() {
                let f: SymFromAddrW = std::mem::transmute(from_addr);
                let mut info: SymbolInfoW = std::mem::zeroed();
                info.size_of_struct = 88;
                info.max_name_len = 511;
                let mut disp = 0u64;
                if f(process, rip, &mut disp, &mut info) != 0 {
                    name = String::from_utf16_lossy(&info.name[..(info.name_len as usize).min(511)]);
                } else {
                    static SAID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
                    if SAID.fetch_add(1, Ordering::Relaxed) < 2 {
                        eprintln!("[profile] no symbol for {name}: error {}", windows_sys::Win32::Foundation::GetLastError());
                    }
                }
            }
            *by_name.entry(name).or_default() += n;
        }
    }
    let mut list: Vec<(String, u32)> = by_name.into_iter().collect();
    list.sort_by(|a, b| b.1.cmp(&a.1));
    eprintln!("[profile] frame thread, {total} samples over {seconds} s:");
    for (name, n) in list.iter().take(60) {
        eprintln!("[profile] {:5.1}%  {}", 100.0 * *n as f64 / total.max(1) as f64, name);
    }
}
