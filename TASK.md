# TASK.md — GGFlow backlog

Живой чеклист. **Актуально на 0.4.0** (после rebrand + зелёный CI).  
Принцип продукта: *надёжный путь «скачал → движок → модель → ответ» без терминала*, а не двадцать новых фич.

---

## Статус продукта

| Слой | Состояние |
|------|-----------|
| Идентичность | **GGFlow** · `com.ggflow.app` · GitHub `Meller2/ggflow` |
| Стек | Tauri 2 + SvelteKit 5 · Windows-first |
| Managed runtime | Pinned llama.cpp `b9963`, SHA-256, staging + smoke, CUDA/Vulkan/CPU |
| App updater | Tauri updater + silent check on boot + Settings |
| CI | Windows: check / build / fmt / clippy / test / NSIS + portable artifacts |
| Runtime integration | Job `runtime-integration` (реальные ZIP) — **schedule / workflow_dispatch** |
| CSP | Включён |
| Diagnostics | `diagnostic_report` + copy in Settings |
| Portable / installer | NSIS + portable `.exe`/`.zip` в CI и Release |

---

## Сделано (не трогать без причины)

- [x] Windows GitHub Actions (build + clippy + tests + tauri NSIS)
- [x] CSP в `tauri.conf.json`
- [x] Targets: NSIS (не `all`)
- [x] Pinned runtime digests + smoke `--version`
- [x] CUDA zip + cudart merge + layout
- [x] Авто-откат backend при **install**: CUDA → Vulkan → CPU (auto only)
- [x] UX заметка при успешном fallback + классификация падений сервера (CUDA/OOM/model/port)
- [x] Unit-тесты `fallback_chain` / digests / swap
- [x] App updater (pubkey, `latest.json`, download+install+relaunch)
- [x] Diagnostic report (copy)
- [x] Data wipe / multi-legacy migrate
- [x] Rebrand metadata (Cargo/package/README/AGENTS)
- [x] Release workflow + notes

---

## P0 — next (надёжность first-run)

Приоритет: чтобы на «типичном Windows» пользователь не застрял.

### 1. Усилить CUDA/Vulkan reliability  ← *в работе / частично*

- [x] Auto-fallback install chain
- [x] Сообщение UI, если ушли с CUDA на Vulkan/CPU
- [x] Подсказки при crash сервера (CUDA / OOM / model / port)
- [ ] **Опциональный one-click «переустановить на CPU/Vulkan»** из Running/Settings после CUDA-crash (без ручного выбора backend)
- [ ] Явная кнопка backend в Settings (expert): CPU / Vulkan / CUDA
- [ ] Периодически гонять `runtime-integration` (workflow_dispatch) после pin bump

### 2. Authenticode (SmartScreen)

- [ ] Code-signing cert (вне репо)
- [ ] Подпись NSIS + portable в release.yml
- [ ] Checksums на странице релиза

### 3. Updater UX polish

- [x] Check / download / verify / install / relaunch
- [ ] Диалог «доступна vX» вместо silent install (настройка auto/manual)
- [ ] Changelog / release notes в UI
- [ ] «Пропустить эту версию»

---

## P1 — публичный релиз

### 4. Ошибки «что / почему / что делать»

- [x] Частично: classify server-exit
- [ ] Единый `AppError` enum на Rust + i18n action hints
- [ ] Missing DLL / old NVIDIA driver — расшифровка

### 5. Runtime pin updates

- [x] Pin в приложении + digests
- [ ] Подписанный remote manifest (обновление движка без релиза app) — позже; сейчас pin = app release

### 6. Repo hygiene

- [x] Issue templates (bug / runtime / model / feature)
- [ ] Labels (P0, runtime, cuda, ui, …) — создать в GitHub UI
- [x] Dependabot (npm + cargo + actions)
- [ ] `cargo audit` / `npm audit` в CI (non-blocking ok)

### 7. README / маркетинг

- [x] EN+RU README, download badge, portable vs installer
- [ ] Screenshots / GIF
- [ ] FAQ + troubleshooting (SmartScreen, CUDA driver, VRAM)
- [ ] Privacy / security model short section

---

## P2 — после стабильного 1.0

- [ ] Профили запуска (quality / speed / long context / CPU-only)
- [ ] Expert flags: batch, tensor-split, custom args
- [ ] API panel: endpoint, curl/python snippet, copy
- [ ] Tray + minimize to tray
- [ ] Autostart (optional)
- [ ] Встроенный минимальный чат (сейчас WebUI llama-server)
- [ ] Мультимодал: mmproj pairing
- [ ] macOS / Linux (отдельный runtime pipeline)

---

## Порядок работ (сейчас)

1. ~~CI red (rustfmt)~~ ✅  
2. ~~TASK refresh + install fallback UX + crash classify~~ (этот проход)  
3. Issue templates + Dependabot  
4. One-click reinstall safer backend after CUDA crash  
5. Updater UX (confirm / skip / notes)  
6. Authenticode, когда будет сертификат  

---

## Не делать сейчас

- Редизайн ради редизайна  
- Мультимодал до стабильного text path  
- «Ещё один chat UI» пока WebUI устраивает  
- Поддержка macOS/Linux без отдельного plan  

---

## Definition of done для 1.0

На чистом Windows-PC с NVIDIA / AMD / без GPU:

1. Скачал installer или portable  
2. Onboarding → auto engine (с fallback если надо)  
3. Каталог → скачал GGUF  
4. Запуск → ready → ответ в WebUI  
5. Обновление app через in-app updater  
6. При поломке — понятный diagnostic + reinstall path  

Без терминала, без ручного поиска `llama-server.exe`, без «молчаливой» CUDA-ошибки.
