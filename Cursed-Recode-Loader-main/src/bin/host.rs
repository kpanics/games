//! Cursed Recode as one program (CursedRecode.exe): the window before the game is up at once, and the game runs in
//! this same process — Java is loaded into it (jvm.dll, the JNI invocation API) instead of being a java.exe of its
//! own. So Windows sees one program, «Cursed Recode» with her icon, on the taskbar and in the task manager.
//!
//! Its command line is java's:  CursedRecode.exe [JVM options...] <main class> [arguments...]
//! (the class path also as --cp-file <file>, for one too long for a command line).
//! Java is found in --jdk <dir> (consumed here), else a `runtime` folder beside this program, else JAVA_HOME.
//! A release started not from the loader (no ticket: opened by hand) never starts Java: the window before the game goes
//! into the valley and says so (license.rs, loader/stop.rs).
//! Development: CURSED_DEV_WINDOW_AT (where the windows go), CURSED_DEV_MUTE, CURSED_DEV_TWINLOG=file.
#![windows_subsystem = "windows"]

use jni_sys::*;
use std::ffi::{c_void, CString};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

/// The process's exit code once Java asks to end (its exit hook).
static EXIT: AtomicU32 = AtomicU32::new(0);

/// This start's words, for the game's library (license.rs `from_the_program`). `no_ticket` took them
/// out of the environment before Java, into this program's stash; the library is its own copy of the
/// license and its stash is empty. Handed once (build.rs exports it).
#[no_mangle]
pub unsafe extern "system" fn cursed_recode_start(buf: *mut u8, len: usize) -> usize {
    cursed_recode::license::hand_start(buf, len)
}

fn main() {
    std::hint::black_box(cursed_recode::notice::keep());
    let started = std::time::Instant::now();
    // Both tickets leave the environment before anything else in this program writes to it.
    // `SetEnvironmentVariable` never edits the block: it builds a new one and frees the old, and
    // the old is still readable in the heap with whatever it held — so a `CURSED_TWIN=…` written
    // first would free a block that still has both of them in it. (A JVM would keep a copy of the
    // whole block the moment it starts: `System.getenv` caches all of it.) `no_ticket` only ever
    // asks how long they are; the taking is the second line, and the pair's own ticket goes
    // straight into the stub's hands.
    let unticketed = cursed_recode::license::no_ticket();
    cursed_recode::license::keep_the_pair_from_java();
    unsafe {
        // never the power-saving mode Windows may give a process whose windows it thinks unseen: the game loads here
        use windows_sys::Win32::System::Threading::*;
        let state = PROCESS_POWER_THROTTLING_STATE { Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION, ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED, StateMask: 0 };
        SetProcessInformation(GetCurrentProcess(), ProcessPowerThrottling, &state as *const _ as *const c_void, std::mem::size_of_val(&state) as u32);
    }
    unsafe {
        // a console (development: started from one, or with its output handed over) keeps Java's output
        windows_sys::Win32::System::Console::AttachConsole(windows_sys::Win32::System::Console::ATTACH_PARENT_PROCESS);
    }
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut jdk: Option<PathBuf> = None;
    if let Some(i) = args.iter().position(|a| a == "--jdk") {
        jdk = args.get(i + 1).map(PathBuf::from);
        args.drain(i..(i + 2).min(args.len()));
    }
    let jdk = jdk
        .or_else(|| std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.join("runtime"))).filter(|p| p.join("bin").is_dir()))
        .or_else(|| std::env::var_os("JAVA_HOME").map(PathBuf::from));

    // JVM options and the main class with its arguments, as java takes them
    let (mut jvm, mut main_class, mut game) = (Vec::<String>::new(), None::<String>, Vec::<String>::new());
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        if main_class.is_some() {
            game.push(a);
            continue;
        }
        match a.as_str() {
            "-cp" | "-classpath" | "--class-path" => {
                if let Some(v) = it.next() {
                    jvm.push(format!("-Djava.class.path={}", class_path(&v)));
                }
            }
            // a class path too long for a command line (IntelliJ's): in a file
            "--cp-file" => {
                if let Some(v) = it.next().and_then(|f| std::fs::read_to_string(f).ok()) {
                    jvm.push(format!("-Djava.class.path={}", class_path(v.trim())));
                }
            }
            // the VM takes these in their one-word form (java's launcher joins them so)
            "--add-exports" | "--add-opens" | "--add-modules" | "--enable-native-access" => {
                if let Some(v) = it.next() {
                    jvm.push(format!("{a}={v}"));
                }
            }
            s if s.starts_with('-') => jvm.push(a),
            _ => main_class = Some(a),
        }
    }

    // the window before the game: up now, in this process; the game joins it by name. Started by the loader, it is the
    // loader's (Cursed Recode.exe: its own window goes on into the launch, loader/app.rs): nothing to start here
    let given = std::env::var("CURSED_TWIN").ok().filter(|n| !n.is_empty());
    let name = given.clone().unwrap_or_else(|| format!("CursedRecode-{}", std::process::id()));
    std::env::set_var("CURSED_TWIN", &name);
    let get = |k: &str| game.iter().position(|a| a == k).and_then(|i| game.get(i + 1)).cloned();
    let (mut w, mut h): (i32, i32) = (get("--width").and_then(|v| v.parse().ok()).unwrap_or(0), get("--height").and_then(|v| v.parse().ok()).unwrap_or(0));
    if w == 854 && h == 480 {
        w = 0;
        h = 0;
    }
    // development: where the windows go (CURSED_DEV_WINDOW_AT; CURSED_DEV_TWIN_AT places them and leaves the focus be)
    let at = std::env::var("CURSED_DEV_WINDOW_AT").or_else(|_| std::env::var("CURSED_DEV_TWIN_AT")).ok().and_then(|v| {
        let mut p = v.split(',');
        Some([p.next()?.trim().parse().ok()?, p.next()?.trim().parse().ok()?])
    });
    // the game's folder (its options: the valley's look, the volumes); started by hand, the one above this program's bin\
    let game_dir = get("--gameDir").or_else(|| {
        let home = std::env::current_exe().ok()?.parent()?.parent()?.to_path_buf();
        home.join("options.txt").is_file().then(|| home.to_string_lossy().into_owned())
    });
    let fullscreen = game_dir
        .as_ref()
        .map(|d| Path::new(d).join("options.txt"))
        .and_then(|p| std::fs::read_to_string(p).ok())
        .is_some_and(|t| t.contains("fullscreen:true"));
    // Its own process, a child of this one (the task manager shows them as one, «Cursed Recode»): in this process the
    // game's making its Vulkan device held the driver for 30–40 ms, and the window's frames with it.
    // CURSED_DEV_TWIN_INPROC: as a thread of this process instead.
    let twin = if given.is_some() || fullscreen || std::env::var_os("CURSED_DEV_NOTWIN").is_some() {
        None
    } else {
        let dir = game_dir.clone();
        let mute = std::env::var_os("CURSED_DEV_MUTE").is_some();
        let log = std::env::var_os("CURSED_DEV_TWINLOG").map(PathBuf::from);
        let exe = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.join("cursed_twin.exe"))).filter(|p| p.is_file());
        match exe {
            Some(exe) if std::env::var_os("CURSED_DEV_TWIN_INPROC").is_none() => {
                let mut c = std::process::Command::new(exe);
                // The window's program keeps the game's window and nothing of its tickets. It is a
                // second process beside the game — a dump of it is as easy to take as of this one —
                // and neither ticket is any of its business: it draws the chrome and that is all.
                // `env_remove` on the child and not on this process: the stub still reads the
                // pair's ticket from here when it opens the seal.
                c.env_remove(cursed_recode::vault::env_ticket());
                c.env_remove(cursed_recode::vault::env_start());
                c.arg("--name").arg(&name);
                if w > 0 && h > 0 {
                    c.arg("--width").arg(w.to_string()).arg("--height").arg(h.to_string());
                }
                if let Some([x, y]) = at {
                    c.arg("--at").arg(format!("{x},{y}"));
                }
                if let Some(d) = &dir {
                    c.arg("--game-dir").arg(d);
                }
                if mute {
                    c.arg("--mute");
                }
                if let Some(l) = &log {
                    c.arg("--log").arg(l);
                }
                c.spawn().ok().map(|_| ())
            }
            _ => {
                let name2 = name.clone();
                std::thread::Builder::new().name("Cursed twin".into()).spawn(move || cursed_recode::twin::serve_here(&name2, w, h, at, dir.as_deref(), mute, started, log)).ok().map(|_| ())
            }
        }
    };

    // a release started not from the loader (no ticket of its): said in the window before the game, before Java
    if unticketed {
        cursed_recode::license::refuse_unticketed();
    }
    let Some(jdk) = jdk else { return fail("Java was not found (no --jdk, no runtime folder, no JAVA_HOME)") };
    let Some(main_class) = main_class else { return fail("No main class given") };

    // Java, in this process
    let code = unsafe { run_java(&jdk, &jvm, &main_class, &game) };
    let code = match code {
        Ok(c) => c,
        Err(e) => return fail(&e),
    };
    // the farewell plays on after the game is gone
    if twin.is_some() {
        cursed_recode::twin::wait_farewell(std::time::Duration::from_secs(8));
    }
    std::process::exit(code);
}

/// A class path as java's launcher reads it: `dir\*` is every jar in dir.
fn class_path(cp: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    for part in cp.split(';').filter(|p| !p.is_empty()) {
        if let Some(dir) = part.strip_suffix("\\*").or_else(|| part.strip_suffix("/*")) {
            if let Ok(rd) = std::fs::read_dir(dir) {
                let mut jars: Vec<String> = rd
                    .filter_map(|e| e.ok().map(|e| e.path()))
                    .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("jar")))
                    .map(|p| p.to_string_lossy().into_owned())
                    .collect();
                jars.sort();
                out.extend(jars);
            }
        } else {
            out.push(part.to_string());
        }
    }
    out.join(";")
}

fn fail(why: &str) {
    if std::env::var_os("CURSED_DEV_NODIALOG").is_some() {
        eprintln!("Cursed Recode could not start: {why}");
        std::process::exit(1);
    }
    let text: Vec<u16> = format!("Cursed Recode could not start.\n\n{why}").encode_utf16().chain(std::iter::once(0)).collect();
    let title: Vec<u16> = "Cursed Recode".encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), windows_sys::Win32::UI::WindowsAndMessaging::MB_ICONERROR);
    }
    std::process::exit(1);
}

/// Java asks the process to end (System.exit, or the game's last thread done): the farewell first.
unsafe extern "system" fn on_exit(code: jint) {
    EXIT.store(code as u32, Ordering::Release);
    cursed_recode::twin::wait_farewell(std::time::Duration::from_secs(8));
    std::process::exit(code);
}

type CreateJavaVm = unsafe extern "system" fn(*mut *mut JavaVM, *mut *mut c_void, *mut c_void) -> jint;

/// Loads jvm.dll from `jdk`, makes the VM with `options`, runs `main_class`.main(`args`) on a thread of its own
/// (a big stack, as java's launcher gives it), and waits for the VM to end. Returns the exit code.
unsafe fn run_java(jdk: &Path, options: &[String], main_class: &str, args: &[String]) -> Result<i32, String> {
    use windows_sys::Win32::System::LibraryLoader::*;
    let bin: Vec<u16> = jdk.join("bin").as_os_str().to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect();
    SetDllDirectoryW(bin.as_ptr());
    let jvm_dll = jdk.join("bin").join("server").join("jvm.dll");
    let wide: Vec<u16> = jvm_dll.as_os_str().to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect();
    let lib = LoadLibraryExW(wide.as_ptr(), std::ptr::null_mut(), LOAD_WITH_ALTERED_SEARCH_PATH);
    if lib.is_null() {
        return Err(format!("Can't load {}", jvm_dll.display()));
    }
    let Some(create) = GetProcAddress(lib, c"JNI_CreateJavaVM".as_ptr() as *const u8) else { return Err("jvm.dll has no JNI_CreateJavaVM".into()) };
    let create: CreateJavaVm = std::mem::transmute(create);
    let opts: Vec<String> = options.to_vec();
    let main_class = main_class.replace('.', "/");
    let args = args.to_vec();
    let thread = std::thread::Builder::new().name("main".into()).stack_size(16 << 20).spawn(move || -> Result<i32, String> {
        let c_opts: Vec<CString> = opts.iter().map(|o| CString::new(o.as_str()).unwrap_or_default()).collect();
        let mut jopts: Vec<JavaVMOption> = c_opts.iter().map(|c| JavaVMOption { optionString: c.as_ptr() as *mut _, extraInfo: std::ptr::null_mut() }).collect();
        let exit_name = CString::new("exit").unwrap();
        jopts.push(JavaVMOption { optionString: exit_name.as_ptr() as *mut _, extraInfo: on_exit as *mut c_void });
        let launcher = CString::new("-Dsun.java.launcher=SUN_STANDARD").unwrap();
        jopts.push(JavaVMOption { optionString: launcher.as_ptr() as *mut _, extraInfo: std::ptr::null_mut() });
        let mut init = JavaVMInitArgs { version: 0x0015_0000, nOptions: jopts.len() as jint, options: jopts.as_mut_ptr(), ignoreUnrecognized: JNI_FALSE };
        let mut vm: *mut JavaVM = std::ptr::null_mut();
        let mut env: *mut JNIEnv = std::ptr::null_mut();
        let r = create(&mut vm, &mut env as *mut *mut JNIEnv as *mut *mut c_void, &mut init as *mut JavaVMInitArgs as *mut c_void);
        if r != JNI_OK {
            return Err(format!("JNI_CreateJavaVM failed ({r})"));
        }
        let f = &**env;
        let cname = CString::new(main_class.as_str()).unwrap();
        let cls = (f.FindClass.unwrap())(env, cname.as_ptr());
        if cls.is_null() {
            (f.ExceptionDescribe.unwrap())(env);
            return Err(format!("Main class {main_class} not found"));
        }
        let m = (f.GetStaticMethodID.unwrap())(env, cls, c"main".as_ptr(), c"([Ljava/lang/String;)V".as_ptr());
        if m.is_null() {
            (f.ExceptionDescribe.unwrap())(env);
            return Err(format!("{main_class} has no main"));
        }
        let string = (f.FindClass.unwrap())(env, c"java/lang/String".as_ptr());
        let array = (f.NewObjectArray.unwrap())(env, args.len() as jsize, string, std::ptr::null_mut());
        for (i, a) in args.iter().enumerate() {
            // UTF-16 for the arguments (paths and names may be Cyrillic): NewString, not NewStringUTF
            let u: Vec<u16> = a.encode_utf16().collect();
            let s = (f.NewString.unwrap())(env, u.as_ptr(), u.len() as jsize);
            (f.SetObjectArrayElement.unwrap())(env, array, i as jsize, s);
            (f.DeleteLocalRef.unwrap())(env, s);
        }
        (f.CallStaticVoidMethodA.unwrap())(env, cls, m, [jvalue { l: array }].as_ptr());
        let mut code = 0;
        if (f.ExceptionCheck.unwrap())(env) != 0 {
            (f.ExceptionDescribe.unwrap())(env);
            code = 1;
        }
        // as java's launcher: the VM ends when its last thread that is not a daemon is done
        ((**vm).DestroyJavaVM.unwrap())(vm);
        Ok(code)
    });
    match thread {
        Ok(t) => t.join().unwrap_or_else(|_| Err("The game's thread fell".into())),
        Err(e) => Err(format!("Can't start the game's thread: {e}")),
    }
}
