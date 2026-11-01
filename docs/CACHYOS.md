# Cursed Recode на CachyOS (Linux)

Игра — Windows-бинарни (движок на Vulkan, Windows-JDK из `runtime.zip`), но протокол дистрибутива
платформенно-нейтрален. Поэтому на Linux две рабочие схемы.

## Схема A: нативный синк + Wine (рекомендуется)

`linux-loader/` — маленькая нативная Rust-программа без единого Win32-вызова: проверяет подпись
`start.json`, скачивает и сверяет весь билд и ассеты Mojang, распаковывает `runtime.zip` и запускает
игру через Wine. Windows-лоадер (его окно, TPM, самокопирование) тут не участвует вовсе.

```bash
sudo pacman -S --needed rust curl wine vulkan-tools
cd linux-loader
cargo build --release
./target/release/cursed-linux-loader               # синк в ~/Cursed Recode и запуск игры
./target/release/cursed-linux-loader --sync-only   # только скачать (первый раз, посмотреть)
```

Vulkan движка Wine проводит к вашему драйверу (winevulkan). Проверьте, что `vulkaninfo` видит GPU
(AMD/Intel — пакет `mesa-vulkan-drivers`, NVIDIA — проприетарный драйвер).

Полезные флаги: `--home <папка>`, `--wine <путь>`, `--launch-only`, `--base <url>` (см. `--help`).

## Схема B: Windows-лоадер под Wine

```bash
wine recode_loader.exe        # из релиза dist
```

Его лог: `~/.wine/dosdevices/c:/Cursed Recode/loader/loader.log`, плюс строки `[loader] …` прямо в
терминале. Если окно не появляется — соберите диагностику (ниже) и приложите файл.

## Диагностика (если что-то не заводится)

```bash
tools/cachyos-diag.sh путь/к/recode_loader.exe
```

Собирает в один файл: версии wine и драйверов, `vulkaninfo`, вывод запуска лоадера под Wine
и хвост его лога.

## Что нативно, а что нет

| Слой | Статус |
|---|---|
| протокол дистрибутива (start.json, подпись, object-*, ассеты) | нативно (`linux-loader`) |
| JDK | Windows-сборка из `runtime.zip`, работает под Wine |
| движок и окна (Vulkan, Win32, twin) | под Wine (winevulkan) |
| полный нативный порт движка на Linux | не входит — это отдельный большой проект |
