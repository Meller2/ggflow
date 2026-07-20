# llama-launcher

[English](README.md) | **Русский**

Десктопный лаунчер для [llama.cpp](https://github.com/ggml-org/llama.cpp) — скачивание моделей с Hugging Face, автоустановка движка под видеокарту, подбор параметров под железо и запуск `llama-server` в один клик.

Стек: [Tauri](https://tauri.app/), [SvelteKit](https://svelte.dev/), TypeScript. **В первую очередь Windows** (x64).

[![CI](https://github.com/Meller2/llama-launcher/actions/workflows/ci.yml/badge.svg)](https://github.com/Meller2/llama-launcher/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Meller2/llama-launcher?include_prereleases)](https://github.com/Meller2/llama-launcher/releases)

## Скачать

Pre-release сборки: **[Releases](https://github.com/Meller2/llama-launcher/releases)**

| Файл | Когда |
|------|--------|
| `llama-launcher_*_x64-setup.exe` | Обычная установка (**рекомендуется**) |
| `llama-launcher-v*-portable.exe` / `.zip` | Без установщика — положил в папку и запустил |

> `*.sig` и `latest.json` нужны только для **автообновления** из приложения. Вручную их скачивать не нужно.

## Возможности

- **Managed runtime** — официальные сборки llama.cpp (CUDA / Vulkan / CPU), закреплённый релиз + проверка SHA-256
- **Настоящий portable** — если в папку с exe можно писать, **настройки, движок и модели** лежат рядом с программой (удобно на флешке)
- **Каталог моделей** — подборка рекомендаций + поиск на Hugging Face и докачка GGUF с возобновлением
- **Локальные модели** — сканирование папок, открытие в Проводнике, запуск одной кнопкой
- **Автонастройка** — параметры запуска с учётом VRAM / RAM / CPU
- **Жизненный цикл сервера** — старт / стоп / лог / статус готовности; опционально открыть чат, когда модель загрузилась
- **Онбординг** — язык (RU/EN), уровень сложности, установка движка при первом запуске
- **Сброс данных** — в Настройках: runtime / models / кэш / settings без удаления самой программы
- **Подписанные обновления** — проверка и установка из интерфейса

## Раскладка portable

Если рядом с exe можно писать (portable / USB / dev):

```
llama-launcher.exe
settings.json
runtime/<tag>/<backend>/llama-server.exe
models/
```

Скопировал папку на другой ПК — настройки едут с тобой.

Если писать нельзя (например, Program Files), данные уходят в:

```
%LOCALAPPDATA%\com.llamalauncher.app\
  settings.json
  runtime\...
  models\
```

## Разработка

Нужны: [Node.js](https://nodejs.org/), [Rust](https://www.rust-lang.org/tools/install) и [зависимости Tauri](https://tauri.app/start/prerequisites/) под вашу ОС.

```bash
npm install
npm run tauri dev      # приложение целиком (Tauri + Vite на :1420)
npm run check          # svelte-check
npm run tauri build    # NSIS + артефакты updater (при наличии ключей)
```

Только Rust (`src-tauri/`): `cargo build`, `cargo clippy`, `cargo test`.

## Лицензия

[MIT](LICENSE)
