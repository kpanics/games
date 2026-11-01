# Cursed Recode

Клиент Cursed Recode (Minecraft 26.3) и его нативный движок (Rust).

## Сборка и запуск

- `run.cmd` — собрать и запустить; лишние аргументы уходят игре
- `build.cmd` — `build\cursed-recode-0.1.0.jar` (Fabric-профиль 26.3, лоудер ≥ 0.19.5)
- IntelliJ: открыть папку, конфигурация Minecraft Client (JDK temurin-25)

`libs\` собирает `python tools\setup-libs.py` (из `%APPDATA%\.minecraft` или скачиванием); access widener накладывает `tools\aw.cmd` — build/run вызывают его сами. Ассеты — из `%APPDATA%\.minecraft\assets`, игровая папка — `run\`.

## Структура

`src\main\java\cursed\recode\` — код мода (миксины по настоящим именам, refmap не нужен); `native\` — движок (Rust, шейдеры в `native\shaders\`); `tools\` — aw.cmd, decompile.cmd, setup-libs.py.

## Лицензия

MIT с указанием автора — см. [LICENSE](LICENSE). Можно всё, включая форки и продажу; единственное условие — [Lunuca](https://github.com/Lunuca) в авторах каждого форка и продукта из него.
