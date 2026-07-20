# llama-launcher

**English** | [Русский](README.ru.md)

A desktop launcher for [llama.cpp](https://github.com/ggml-org/llama.cpp) — download models from Hugging Face, auto-install the engine for your GPU, tune launch flags for your hardware, and run `llama-server` with one click.

Built with [Tauri](https://tauri.app/), [SvelteKit](https://svelte.dev/) and TypeScript. **Windows-first** (x64).

[![CI](https://github.com/Meller2/llama-launcher/actions/workflows/ci.yml/badge.svg)](https://github.com/Meller2/llama-launcher/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Meller2/llama-launcher?include_prereleases)](https://github.com/Meller2/llama-launcher/releases)

## Download

Pre-release builds: **[Releases](https://github.com/Meller2/llama-launcher/releases)**

| Asset | Use when |
|-------|----------|
| `llama-launcher_*_x64-setup.exe` | Normal install (**recommended**) |
| `llama-launcher-v*-portable.exe` / `.zip` | No installer — drop into a folder and run |

> `*.sig` and `latest.json` are for **in-app auto-update**. You do not need them manually.

## Features

- **Managed runtime** — downloads official llama.cpp builds (CUDA / Vulkan / CPU), pinned + checksummed
- **True portable mode** — if the app folder is writable, **settings, engine, and models** all live next to the exe (USB-friendly)
- **Model catalog** — curated recommendations + Hugging Face search / resumable GGUF downloads
- **Local models** — scan folders, reveal in Explorer, one-click launch
- **Auto-config** — VRAM / RAM / CPU aware launch parameters
- **Server lifecycle** — start / stop / logs / ready status; optional open chat UI when ready
- **Onboarding** — language (RU/EN), expertise level, first-run engine install
- **Data reset** — Settings → remove runtime / default models / cache / settings without uninstalling the app
- **Signed app updates** — check and install from inside the app

## Portable layout

When the folder next to the executable is writable (portable / USB / dev):

```
llama-launcher.exe
settings.json
runtime/<tag>/<backend>/llama-server.exe
models/
```

Copy the whole folder to another PC — prefs come with you.

If that folder is not writable (e.g. Program Files), data goes to:

```
%LOCALAPPDATA%\com.llamalauncher.app\
  settings.json
  runtime\...
  models\
```

## Development

Prerequisites: [Node.js](https://nodejs.org/), [Rust](https://www.rust-lang.org/tools/install), and the [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your OS.

```bash
npm install
npm run tauri dev      # full app (Tauri + Vite on :1420)
npm run check          # svelte-check
npm run tauri build    # NSIS + updater artifacts (when signing keys present)
```

Rust only (`src-tauri/`): `cargo build`, `cargo clippy`, `cargo test`.

## License

[MIT](LICENSE)
