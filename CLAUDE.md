# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

Desktop launcher for [llama.cpp](https://github.com/ggml-org/llama.cpp). Wraps `llama-server.exe`: scans local GGUF models, downloads them from Hugging Face, auto-configures launch flags for the detected hardware, and manages the server process lifecycle. Tauri v2 backend (Rust) + SvelteKit frontend (Svelte 5, TypeScript).

**Windows-first.** Hardware detection uses DXGI (VRAM) and `GlobalMemoryStatusEx` (RAM); the server is stopped with `taskkill` and spawned with `CREATE_NO_WINDOW`. Porting to other OSes means replacing those pieces. Code comments are in Russian.

## Commands

```bash
npm run tauri dev      # run the full app (Tauri window + Vite dev server on :1420)
npm run tauri build    # production bundle
npm run check          # svelte-kit sync + svelte-check (TS/Svelte type check)
npm run dev            # frontend only, no Tauri backend — invoke() calls will fail
```

Rust: `cargo build` / `cargo clippy` from `src-tauri/`. There is no test suite.

## Architecture

Three layers, kept in sync by hand:

1. **Rust backend** (`src-tauri/src/`) — domain modules registered as Tauri commands in `lib.rs`'s `invoke_handler`. Any new command must be added there. Modules: `config` (Settings load/save in app config dir), `models` (GGUF scan + metadata parse), `server` (llama-server lifecycle), `hardware` (VRAM/RAM detect), `autoconfig` (pick flags from hardware + GGUF), `hf` (Hugging Face search/download).

2. **API layer** (`src/lib/api.ts`) — thin `invoke()` wrappers plus TypeScript interfaces that **mirror the Rust structs** (`Settings`, `LaunchConfig`, `ModelInfo`, `GgufMeta`, `ServerStatus`, …). When you change a `#[derive(Serialize/Deserialize)]` struct in Rust, update the matching interface here or the boundary breaks silently.

3. **UI** (`src/routes/+page.svelte` + `src/lib/components/`) — a single SPA page with tab switching (Модели / Каталог / Запущено / Настройки). SvelteKit uses `adapter-static` in SPA mode (`fallback: index.html`); there is no SSR or routing beyond this one page.

### Server lifecycle is event-driven

Starting/stopping the server is a command, but status flows back through Tauri **events**, not return values. `src/lib/server.svelte.ts` (`serverState`, a Svelte 5 runes store) listens for:

- `server-log` — a stdout/stderr line (buffer capped at 2000 lines)
- `server-ready` — server bound its port; payload is the port
- `server-timeout` — server didn't become ready within `READY_TIMEOUT` (180s); payload is a message. The process may still be alive (stuck loading a model); the UI surfaces the error but leaves the decision to stop to the user.
- `server-exit` — process ended; payload is the exit code

Readiness is detected two ways: the log watcher matches `"listening"` + port, and a fallback watchdog thread polls the TCP port. `mark_ready` (a one-shot `AtomicBool` swap) guarantees `server-ready` fires at most once regardless of which detector wins. The watcher/watchdog threads are tagged with a generation `id` so a stale thread can't clobber a newer launch's state. A non-zero exit after a manual stop is expected (`taskkill`) and is **not** surfaced as an error — only self-crashes are. `server::shutdown` is called from the window `CloseRequested` handler in `lib.rs` so the child process isn't orphaned when the app closes. Preserve these distinctions when touching either side.

All `ServerState`/`DownloadState` mutex access goes through poison-tolerant `.lock()` helpers (`unwrap_or_else(|e| e.into_inner())`) — a panic in one thread must not brick the whole app.

### Launch flags

Defaults live in `config::LaunchDefaults` (ported from a `llama.bat`: 16k ctx, q4_0 KV cache, ngl 99, port 8080). `server::build_args` maps a `LaunchConfig` to `llama-server` CLI args. `autoconfig` overrides defaults based on detected hardware + parsed GGUF metadata. These three must stay consistent when adding a launch parameter — new field in `LaunchDefaults`/`LaunchConfig` (Rust) → `api.ts` interface → `build_args` mapping.

### Downloads (`hf.rs`)

`hf_download` streams into `<file>.part` then renames on success. On cancel/failure the `.part` is **kept**, and the next download of the same file resumes via HTTP `Range` (206 → append; 200 → server ignored Range, restart from zero). After the stream ends, `downloaded` is checked against `Content-Length` (when known) before rename — a truncated 200 response is rejected rather than saved as valid. File writes use `tokio::fs` so the async executor isn't blocked. A single-slot mutex (`DownloadState.active`) forbids concurrent downloads.

### Hardware detection is per-OS

`hardware.rs` gates `detect_gpu`/`detect_ram` behind `#[cfg(windows)]` (DXGI + `GlobalMemoryStatusEx`) vs `#[cfg(not(windows))]` (`nvidia-smi` for GPU, `/proc/meminfo` or `sysctl hw.memsize` for RAM). `detect_hardware` calls the same two function names on every platform. Non-NVIDIA GPUs off Windows fall back to CPU mode.

### Settings forward-compat

`Settings` and `LaunchDefaults` carry `#[serde(default)]` at the struct level so a config written by an older build (missing newer fields) still deserializes field-by-field instead of `unwrap_or_default()` wiping the whole file. Keep this attribute when adding fields.
