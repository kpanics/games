//! CLI-половина лоадера (docs/CACHYOS.md): то же, что и окно, но в терминале. Окно — в gui.rs.

use std::path::PathBuf;

use cursed_linux_loader as lib;

fn usage() {
    println!(
        "cursed-linux-loader — нативная половина лоадера Cursed Recode на Linux\n\
         \n\
         запуск: cursed-linux-loader [флаги]\n\
         --home <папка>   где живёт игра (по умолчанию — на «диске C:» Wine: <WINEPREFIX>/drive_c/CursedRecode)\n\
         --sync-only      только скачать и проверить, не запускать\n\
         --launch-only    не синхронизировать, сразу запустить\n\
         --wine <путь>    какой wine использовать (по умолчанию $WINE или wine)\n\
         --base <url>     база дистрибутива (по умолчанию — релиз-склад dist-objects этого репозитория)\n\
         \n\
         Игра (bin/CursedRecode.exe и JDK из runtime.zip) — Windows-бинарни и идут под Wine;\
         \nVulkan движка Wine проводит к вашему драйверу (winevulkan)."
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        usage();
        return;
    }
    let get = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let home = get("--home").map(PathBuf::from);
    let base = get("--base").unwrap_or_else(|| lib::BASE.to_string());
    let wine = get("--wine").or_else(|| std::env::var("WINE").ok()).unwrap_or_default();
    let sync_only = args.iter().any(|a| a == "--sync-only");
    let launch_only = args.iter().any(|a| a == "--launch-only");

    let settings = lib::Settings::default();
    let home = lib::home_of(&settings, home.as_deref());
    println!("Cursed Recode на Linux — папка {}", home.display());
    if let Err(e) = lib::run_cli(&home, &base, &wine, sync_only, launch_only) {
        eprintln!("не вышло: {e}");
        std::process::exit(1);
    }
}
