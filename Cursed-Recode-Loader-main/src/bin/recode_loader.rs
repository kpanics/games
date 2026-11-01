//! The loader of Cursed Recode («Cursed Recode.exe» on the player's computer): see src/loader.
#![windows_subsystem = "windows"]

fn main() {
    std::hint::black_box(cursed_recode::notice::keep());
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--headless") {
        unsafe {
            windows_sys::Win32::System::Console::AttachConsole(windows_sys::Win32::System::Console::ATTACH_PARENT_PROCESS);
        }
        std::process::exit(cursed_recode::loader::headless(&args[2..]));
    }
    std::process::exit(cursed_recode::loader::main());
}
