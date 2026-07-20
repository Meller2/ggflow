# AGENTS.md — GGFlow

Desktop launcher for [llama.cpp](https://github.com/ggml-org/llama.cpp). Product name **GGFlow** (repo was once `llama-launcher`). Wraps `llama-server`: local GGUF scan, Hugging Face download, managed runtime install (CPU / Vulkan / CUDA 12.4), hardware auto-config, process lifecycle, signed app updates, in-app data reset.

| Identity | Value |
|----------|--------|
| Display name | GGFlow |
| npm / crate / exe | `ggflow` |
| Tauri identifier / AppData | `com.ggflow.app` |
| GitHub | https://github.com/Meller2/ggflow |
| Current version | 0.4.0 (see `package.json` + `tauri.conf.json`) |

**Stack:** Tauri v2 (Rust) + SvelteKit (Svelte 5, TypeScript).  
**Windows-first.** DXGI + `GlobalMemoryStatusEx` for hardware; `taskkill` / `CREATE_NO_WINDOW` for process control. Code comments largely Russian; UI i18n `ru` / `en` + expertise (`beginner` / `intermediate` / `expert`).

**Plugins:** `dialog`, `opener`, `process`, `updater`. Frameless window + `Titlebar.svelte`.

**Workspace note:** This is a standalone product — keep the clone **outside** any `llama.cpp` source tree. After a fresh checkout (or copy without `node_modules` / `target`): `npm install`, then `npm run tauri dev`.

## Commands

```bash
npm install
npm run tauri dev      # full app (Tauri + Vite on :1420)
npm run tauri build    # NSIS + updater artifacts when signing keys present
npm run check          # svelte-kit sync + svelte-check
npm run dev            # frontend only — invoke() fails without Tauri
```

Rust (`src-tauri/`): `cargo build`, `cargo clippy`, `cargo test`, `cargo fmt --check`.  
CI: `.github/workflows/ci.yml` (Windows). Release: `.github/workflows/release.yml` on tags `v*.*.*`.

## Architecture

Three layers, kept in sync **by hand**:

1. **Rust** (`src-tauri/src/`) — commands registered in `lib.rs` `invoke_handler`. **New command → register there.**

   | Module | Role |
   |--------|------|
   | `config` | Settings load/save in `app_dir()` (portable-first; atomic Windows save), multi-legacy migrate, `setup_version` |
   | `models` | GGUF scan (depth≤8, max 5000, no symlinks) + meta + `reveal_in_folder` |
   | `server` | `llama-server` lifecycle, logs, readiness events |
   | `hardware` | VRAM/RAM/CPU (Windows DXGI; else nvidia-smi / meminfo) |
   | `autoconfig` | Launch flags from HW + GGUF meta |
   | `hf` | HF search + resumable download |
   | `runtime` | Pinned llama.cpp install → `runtime/<tag>/<backend>/` |
   | `diagnostics` | `diagnostic_report` for bug reports |
   | `data_reset` | `wipe_app_data` (runtime / models / cache / settings) |

2. **API** (`src/lib/api.ts`) — `invoke()` + TS types mirroring Rust structs. Change both sides together.

3. **UI** (`src/routes/+page.svelte` + `src/lib/components/`) — SPA tabs: Models / Catalog / Running / Settings. `adapter-static`, no SSR.

Frontend stores: `server.svelte.ts`, `prefs.svelte.ts`, `i18n.ts`, `recommended.ts`.  
Components: `LocalModels`, `Catalog`, `Running`, `Settings`, `Onboarding`, `Titlebar`, `ContextMenu`, `Icon`.

### Server lifecycle (events)

| Event | Meaning |
|-------|---------|
| `server-log` | stdout/stderr line (buffer ≤2000) |
| `server-ready` | model ready; payload = port |
| `server-timeout` | not ready in 180s |
| `server-exit` | process ended; exit code |

Also: `download-progress`, `runtime-progress`, `models-changed`.  
Readiness: log `"listening"` + port, or HTTP `GET /health` watchdog. Generation id + one-shot `mark_ready`. Manual stop via `taskkill` non-zero exit is OK. `server::shutdown` on window close.

### Launch flags

`LaunchDefaults` → `LaunchConfig` → `server::build_args`. Defaults: 16k ctx, q4_0 KV, ngl 99, port 8080. Always: host 127.0.0.1, flash-attn, jinja. Optional tools/MCP. Autoconfig: VRAM reserve ~1.5 GiB, RAM ~3 GiB.

Add a flag in all three: Rust structs → `api.ts` → `build_args`.

### Managed runtime

Pinned tag + SHA-256 (`PINNED_TAG` / `PINNED_DIGESTS`, currently `b9963`). Download → hash → staging → smoke `llama-server --version` → atomic swap. CUDA merges cudart zip.

```
{app_dir}/settings.json
{app_dir}/runtime/<tag>/<backend>/llama-server.exe
{app_dir}/models/
```

`app_dir` = next to exe if writable (true portable), else `%LOCALAPPDATA%\com.ggflow.app\`.  
Legacy data dirs still scanned: `com.llamalauncher.app`, `com.ilzat.llama-launcher`.

### Settings

Same `app_dir` as runtime/models (not Tauri Roaming alone). `#[serde(default)]` for forward-compat. Windows-safe atomic save. Wipe clears all known settings paths so migrate cannot resurrect.

### Releases

Artifacts: `ggflow_<ver>_x64-setup.exe` (+ `.sig`), `latest.json`, `ggflow-v<ver>-portable.exe` / `.zip`.  
Notes: `.github/release-notes.md` or `.github/releases/vX.Y.Z.md`. Draft pre-release → publish after verify.

## Conventions

- Small focused diffs; Russian comments OK.
- New command: `lib.rs` → `api.ts` → UI.
- Svelte 5 runes for new state; i18n via `prefs.t()`.
- No drive-by refactors / large frontend test suite unless asked.
- Do not commit secrets, GGUF binaries, `node_modules/`, `src-tauri/target/`.
- Version bump: `package.json` + `Cargo.toml` + `Cargo.lock` package version + `tauri.conf.json`.

## Key paths

```
src-tauri/src/lib.rs          # commands, close → shutdown
src-tauri/src/server.rs       # process + events
src-tauri/src/runtime.rs      # PINNED_TAG, DATA_DIR_NAME, install
src-tauri/src/config.rs       # Settings, portable path
src-tauri/src/data_reset.rs   # wipe_app_data
src/lib/api.ts                # IPC types
src/lib/server.svelte.ts      # frontend server state
src/routes/+page.svelte       # shell
src/lib/components/           # feature UI
.github/workflows/            # ci.yml, release.yml
README.md                     # EN + RU in-page anchors
```
