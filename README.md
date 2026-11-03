# Cursed Recode

Клиент и движок Cursed Recode: **дистрибутив и лоадер**. Сама игра (движок на Vulkan,
Windows-JDK) — Windows-бинарни и на Linux идёт под Wine; лоадер на обеих системах свой.

- Сайт: **<https://kpanics.github.io/games/>**
- Скачать для Windows: [recode_loader.exe](https://github.com/kpanics/games/releases/download/dist/recode_loader.exe)
  (релиз [`dist`](https://github.com/kpanics/games/releases/tag/dist))
- Linux (CachyOS и прочие): [docs/CACHYOS.md](docs/CACHYOS.md)

## Что здесь

| Путь | Что |
|---|---|
| `linux-loader/` | нативный лоадер на Linux: окно (вкладки, новости, аккаунты), синк, запуск под Wine |
| `dist/` | сборка дистрибутива: подписанный `start.json`, объекты по SHA-256, релизы `dist` и `dist-objects` |
| `packaging/` | PKGBUILD и `.desktop` для CachyOS/Arch |
| `site/` | статическая страница для GitHub Pages |
| `docs/CACHYOS.md` | инструкция и диагностика на Linux |
| `Cursed-Recode-main/`, `Cursed-Recode-Loader-main/` | исходники игры и Windows-лоадера |
| `recode-site-and-backend/` | исходники сайта и API аккаунтов (cursedclient.xyz) |
| `.github/workflows/build.yml` | CI: сборка, релизы, живой синк-тест на Linux |

## Как это работает

Манифест `start.json` подписан ed25519 — лоадер не примет билд без верной подписи. Каждый файл
билда лежит в релизе-складе [`dist-objects`](https://github.com/kpanics/games/releases/tag/dist-objects)
как `object-<sha256>` и качается по своему хешу; ассеты — с CDN Mojang. Витрина
[`dist`](https://github.com/kpanics/games/releases/tag/dist) держит один файл — `recode_loader.exe`.

Аккаунты (регистрация, вход, имя игрока) — через API [cursedclient.xyz](https://cursedclient.xyz);
исходник — в `recode-site-and-backend/`.

## Лицензия

Cursed Recode — произведение Lunuca, MIT с указанием автора (см. `Cursed-Recode-Loader-main/LICENSE`).
Minecraft — Mojang; ассеты качаются с их CDN, в репозитории их нет.
