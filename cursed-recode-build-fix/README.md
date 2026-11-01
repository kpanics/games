# Починка сборки Cursed-Recode-main

Проект лежит в репозитории [`kpanics/syntax-error-java`](https://github.com/kpanics/syntax-error-java)
в папке `Cursed-Recode-main/` (это исходники [Lunuca/Cursed-Recode](https://github.com/Lunuca/Cursed-Recode)
плюс добавленные `build.gradle` / `settings.gradle`). Сам код мода и движка я сверил с апстримом —
он совпадает побайтно, там всё целое. Сломана была **сборка** (workflow `Build Cursed Recode`),
и падали все три прогона подряд.

## Что именно не работало

| Прогон | Упавший шаг | Причина |
|---|---|---|
| `41308570981`, `41309480192` | `Build Java Mod` (`./gradlew build`) | В репозитории нет Gradle wrapper (`gradlew`, `gradle/wrapper/`), а `build.gradle` вообще про другую игру: `minecraft 1.21.4`, `yarn 1.21.4+build.8`, `fabric-loom 1.7`. Проект же собран под Minecraft **26.3** без обфускации (`official`-namespace) и собирается plain `javac`, а не Loom'ом. |
| `41310405530` | `Compile Java (Manual)` (упал за 1 с) | Три причины сразу:<br>1. **Нет `libs/`** — папка в `.gitignore` и собирается скриптом `tools/setup-libs.py`, а в workflow его никто не запускал, поэтому `javac` не видел ни одного класса Minecraft/Fabric (`-cp Cursed-Recode-main/libs/*` указывал в пустоту).<br>2. **JDK 21**, а `build.cmd` компилирует с `--release 25`, и `fabric.mod.json` требует `java >= 25`.<br>3. **Не наложен access widener** (`tools/aw.cmd`) — без него не компилируются обращения к приватным полям/классам игры.<br>Плюс артефакты: грузились только `*.dll` (без `CursedRecode.exe` и `cursed_twin.exe`), а jar искали в `build/libs/*.jar`, тогда как `build.cmd` кладёт его в `build/cursed-recode-0.1.0.jar`. |

При этом шаг **`Build Rust Native` проходил успешно** — нативная часть (Rust) в порядке,
её трогать не пришлось, только добавил кеш.

## Что изменено

`fix-cursed-recode-build.patch` (и он же готовым файлом — `build.yml`):

1. **JDK 25 (Temurin)** через `actions/setup-java@v5` вместо JDK 21.
2. **Python + `python tools\setup-libs.py`** — собирает `libs\`: jar игры, библиотеки Minecraft,
   Fabric Loader с зависимостями, модули Fabric API (источники для IDE уже не нужны).
3. **`call build.cmd`** — штатный путь проекта: `tools\aw.cmd` (access widener) → `javac --release 25`
   → `build\cursed-recode-0.1.0.jar`. Это ровно то, что делает разработчик локально, никаких
   самодельных списков файлов и `-cp` «руками».
4. **`cargo build --release`** в `native\` + кеш `native/target` и `~/.cargo/registry` по `Cargo.lock`.
5. **Сбор артефактов**: шаг `Collect artifacts` складывает в `dist\` jar, `CursedRecode.exe`,
   `cursed_twin.exe`, `recode_loader.exe`, `cursed_recode.dll`; `upload-artifact` отдаёт
   `Cursed-Recode-windows-x64` и падает, если ничего не собралось (`if-no-files-found: error`) —
   больше не будет «зелёного» прогона без файлов.
6. Мелочи: `concurrency` (отменяет предыдущий прогон), `permissions: contents: read`,
   `defaults.run` (`shell: cmd`, `working-directory: Cursed-Recode-main`) — из-за этого
   больше нет расхождений в путях между шагами, шаг `Toolchain versions` печатает версии
   java/javac/python/cargo для быстрой диагностики.
7. **Удалены `Cursed-Recode-main/build.gradle` и `settings.gradle`.** Они описывают Minecraft 1.21.4
   с yarn-маппингами и к этому проекту неприменимы (26.3 без обфускации, сборка через `javac`,
   не Loom). Если хочется оставить — просто не применяйте этот хунк патча, на сборку он не влияет.

## Как применить

```bash
cd syntax-error-java
git checkout main && git pull
git apply --check cursed-recode-build-fix/fix-cursed-recode-build.patch   # проверка
git apply cursed-recode-build-fix/fix-cursed-recode-build.patch
git add -A && git commit -m "Fix Cursed Recode build: JDK 25 + setup-libs + build.cmd"
git push
```

Либо просто скопировать `build.yml` в `.github/workflows/build.yml` (gradle-файлы тогда
можно оставить, но собирать ими всё равно не получится) и запустить workflow вручную
(`workflow_dispatch`).

## Локальная сборка (как в README проекта, Windows)

```cmd
python tools\setup-libs.py   :: один раз, качает libs\
build.cmd                    :: aw + javac + jar в build\cursed-recode-0.1.0.jar
run.cmd                      :: собрать + запустить игру
```

Нужен **JDK 25** (`JAVA_HOME` или путь из `tools\jdk.cmd`) и Rust stable с MSVC.

## Чего я не смог проверить

В песочнице нет ни JDK, ни Rust, и нет сетевого доступа к `piston-meta.mojang.com`,
`maven.fabricmc.net`, `cdn.modrinth.com` и Adoptium — то есть прогнать саму сборку здесь
физически нельзя. Поэтому:

* патч проверен на чистом клоне (`git apply --check` + `git apply` проходят, YAML валиден);
* шаги повторяют документированную сборку проекта 1:1 (`README.md` в `Cursed-Recode-main`);
* фактическая проверка — первый прогон workflow на вашем раннере (он качает ~300–400 МБ
  библиотек, так что первый раз будет дольше обычного; кеш ускорит следующие).

Если прогон упадёт — пришлите лог упавшего шага, доведу (например, если у `cdn.modrinth.com`
будет 403 или изменится id версии Fabric API в `tools/setup-libs.py`).
