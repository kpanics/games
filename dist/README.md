# Дистрибутив «на GitHub»

Вместо сервера (`cursed-api` на cursedclient.xyz) — GitHub Release этого репозитория. Всё, что делал
сервер, лежит файлами; подпись манифеста — своя ed25519-пара, публичная половина вшита в лоадер при
сборке.

## Как это устроено

| Что делал сервер | Что вместо него |
|---|---|
| `POST /api/loader/recode/start` (манифест + подпись + тикеты) | `GET <release>/start.json` — тот же JSON, статический |
| `GET /api/dist/object/<sha256>` (файлы билда) | `GET <release>/object-<sha256>` — ассеты релиза |
| `GET /api/dist/asset/<sha1>` (ассеты Minecraft) | CDN Mojang: `resources.download.minecraft.net` |
| вход через сайт (код + «разрешить») | локальная сессия «Player» (аккаунтов нет) |
| лицензия/тикеты/подписка | не нужны: сборка без `CURSED_RELEASE` играет как есть |
| краш-репорты, «взгляд в комнату» | никуда не уходят |

Активируется адресом: если база (при сборке `CURSED_VERIFY_API`, в dev — `CURSED_LOADER_API`) — это
`https://github.com/.../releases/download/dist`, лоадер переключается в этот режим (`loader/api.rs`
`github()`).

## Файлы релиза (тег `dist`)

- `start.json` — статический ответ старта: манифест билда (строкой) + ed25519-подпись + тикеты-заглушки
- `object-<sha256>` — каждый файл билда, ровно байтами из манифеста:
  - `bin/CursedRecode.exe` (host: Java в его процессе), `bin/cursed_twin.exe`, `bin/cursed_recode.dll`
  - `loader/recode_loader.exe` — сам лоадер (вид `loader`: self-update по хешу)
  - `game/minecraft-26.3.jar` (с наложенным access widener), `game/cursed-recode-<v>.jar` (мод)
  - `libraries/...` — fabric-лоудер, библиотеки MC, модули fabric-api, ViaVersion
  - `runtime.zip` — JDK 25 (вид `zip`: один объект, распаковывается в `runtime/`, корневая папка
    внутри срезается; маркер `.unpacked-<hash>` говорит, что этот билд уже распакован)
  - `assets/indexes/<id>.json` (вид `assets-index`; сами ассеты лоадер берёт с CDN Mojang)
  - `log4j2.xml` (вид `config`)

## Сборка и публикация

Всё делает `build.yml` (джоба `dist`, на push в `main`), вручную:

```
python dist/make_dist.py --home dist-home --out dist-out --key <hex> --build 7 \
    --version 0.1.0 --fetch-assets 26.3 --publish
```

Ключи: `dist/make_dist.py` подписывает приватной половиной (`CURSED_DIST_KEY` — секрет репозитория),
лоадер собран с публичной (`CURSED_VERIFY_KEY` при `cargo build`). Пара генерируется один раз:

```
python -c "from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey as K; \
k=K.generate(); print(k.private_bytes_raw().hex()); print(k.public_key().public_bytes_raw().hex())"
```

## Проверка связки

Тот же CI запускает на Windows-раннере

```
set CURSED_LOADER_HOME=<пустая папка>
native\target\release\recode_loader.exe --headless --token github --sync
```

— лоадер читает `start.json` с GitHub, проверяет подпись, скачивает весь билд (включая `runtime.zip`
и ассеты Mojang) и сверяет каждый хеш. Это и есть вся связка, кроме окна самой игры.

Игроку: скачать `loader/recode_loader.exe` из релиза, запустить (права администратора — так задумано),
выбрать папку — лоадер скачает всё сам и стартует игру (`Play`).
