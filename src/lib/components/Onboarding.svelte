<script lang="ts">
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import {
    validateLlamaDir,
    pickFolder,
    saveSettings,
    runtimeStatus,
    runtimeInstall,
    runtimeCancelInstall,
    ensureDefaultModelsDir,
    formatBytes,
    type Settings,
    type RuntimeStatus,
    type RuntimeProgress,
  } from "$lib/api";

  let { settings, oncomplete }: {
    settings: Settings;
    oncomplete: (s: Settings) => void;
  } = $props();

  let rt = $state<RuntimeStatus | null>(null);
  let rtLoading = $state(true);

  // Режим: авто-установка vs ручной путь.
  let manual = $state(false);
  let llamaDir = $state(settings.llama_dir ?? "");
  let llamaValid = $state<boolean | null>(null);
  let checking = $state(false);

  let modelFolders = $state<string[]>([]);

  // Установка runtime.
  let installing = $state(false);
  let progress = $state<RuntimeProgress | null>(null);
  let installError = $state<string | null>(null);
  let installedPath = $state<string | null>(settings.llama_dir);
  let installedTag = $state<string | null>(settings.runtime_tag);
  let installedBackend = $state<string | null>(settings.runtime_backend);
  let installedLabel = $state<string | null>(null);

  let saving = $state(false);

  let unlisten: UnlistenFn | null = null;
  $effect(() => {
    listen<RuntimeProgress>("runtime-progress", (e) => {
      progress = e.payload;
      if (e.payload.error) installError = e.payload.error;
    }).then((u) => (unlisten = u));
    return () => unlisten?.();
  });

  async function init() {
    rtLoading = true;
    try {
      rt = await runtimeStatus();
      // Дефолтная папка моделей — portable models рядом с exe.
      if (settings.model_folders.length) {
        modelFolders = [...settings.model_folders];
      } else {
        const models = await ensureDefaultModelsDir();
        modelFolders = [models];
      }
      if (rt.installed && rt.llama_dir) {
        installedPath = rt.llama_dir;
        installedTag = rt.tag;
        installedBackend = rt.backend;
        installedLabel = rt.backend_label;
        llamaDir = rt.llama_dir;
        llamaValid = true;
      } else if (settings.llama_dir) {
        llamaDir = settings.llama_dir;
        await checkLlama();
      }
    } catch (e) {
      installError = String(e);
    } finally {
      rtLoading = false;
    }
  }
  init();

  async function checkLlama() {
    if (!llamaDir) {
      llamaValid = false;
      return;
    }
    checking = true;
    llamaValid = await validateLlamaDir(llamaDir);
    checking = false;
  }

  async function browseLlama() {
    const dir = await pickFolder("Папка с llama-server.exe");
    if (dir) {
      llamaDir = dir;
      installedPath = null; // ручной путь
      await checkLlama();
    }
  }

  async function addModelFolder() {
    const dir = await pickFolder("Папка с моделями (.gguf)");
    if (dir && !modelFolders.includes(dir)) {
      modelFolders = [...modelFolders, dir];
    }
  }

  function removeFolder(f: string) {
    modelFolders = modelFolders.filter((x) => x !== f);
  }

  async function installEngine() {
    if (installing) return;
    installing = true;
    installError = null;
    progress = null;
    try {
      const st = await runtimeInstall(null);
      rt = st;
      installedPath = st.llama_dir;
      installedTag = st.tag;
      installedBackend = st.backend;
      installedLabel = st.backend_label;
      if (st.llama_dir) {
        llamaDir = st.llama_dir;
        llamaValid = true;
      }
      // Убедимся, что models есть в списке.
      if (st.default_models_dir && !modelFolders.includes(st.default_models_dir)) {
        modelFolders = [st.default_models_dir, ...modelFolders];
      }
      manual = false;
    } catch (e) {
      const msg = String(e);
      if (!msg.includes("отменена") && !msg.includes("Отменена")) {
        installError = msg;
      }
    } finally {
      installing = false;
    }
  }

  async function cancelInstall() {
    await runtimeCancelInstall();
  }

  const engineOk = $derived(
    (!!installedPath && llamaValid !== false) || llamaValid === true,
  );
  const canFinish = $derived(engineOk && modelFolders.length > 0 && !installing);

  async function finish() {
    if (!canFinish) return;
    saving = true;
    // Ручной путь → не managed; auto-install / уже найденный portable → managed.
    const usedManual = manual && !installedPath;
    const dir = (usedManual ? llamaDir : installedPath) ?? llamaDir;
    const updated: Settings = {
      ...settings,
      llama_dir: dir,
      model_folders: modelFolders,
      onboarded: true,
      runtime_managed: !usedManual && (!!installedTag || !!rt?.installed),
      runtime_tag: usedManual ? null : (installedTag ?? rt?.tag ?? null),
      runtime_backend: usedManual ? null : (installedBackend ?? rt?.backend ?? null),
    };
    await saveSettings(updated);
    saving = false;
    oncomplete(updated);
  }

  const pct = $derived(
    progress && progress.total > 0
      ? Math.min(100, (progress.downloaded / progress.total) * 100)
      : 0,
  );
</script>

<div class="onb-wrap">
  <div class="glass onb-card">
    <div class="brand">
      <div class="logo-orb"></div>
      <div>
        <h1>LlamaLauncher</h1>
        <p class="sub">Локальные нейросети в один клик</p>
      </div>
    </div>

    <p class="lead">
      Сейчас поставим движок <b>llama.cpp</b> рядом с программой (portable) и
      подготовим папку для моделей. Ничего вручную скачивать не нужно.
    </p>

    <!-- Шаг 1: движок -->
    <section class="field">
      <span class="lbl">1 · Движок llama.cpp</span>

      {#if rtLoading}
        <div class="hint muted">Проверяю…</div>
      {:else if installedPath && !installing && !manual}
        <div class="engine-ok">
          <span class="ok">✓ Движок готов</span>
          {#if installedTag || installedLabel}
            <span class="engine-meta">
              {#if installedTag}{installedTag}{/if}
              {#if installedTag && installedLabel} · {/if}
              {#if installedLabel}{installedLabel}{/if}
            </span>
          {/if}
          <button class="btn tiny" onclick={() => installEngine()} disabled={installing}>
            Переустановить
          </button>
        </div>
        <p class="path-hint" title={installedPath}>{installedPath}</p>
      {:else if !manual}
        <div class="auto-box">
          {#if rt}
            <p class="auto-why">
              Для этого ПК: <b>{rt.recommended_label}</b>
            </p>
          {/if}
          <button
            class="btn btn-primary install"
            onclick={installEngine}
            disabled={installing}
          >
            {installing ? "Устанавливаю…" : "↓ Установить автоматически"}
          </button>
          <button class="linkish" onclick={() => (manual = true)} disabled={installing}>
            У меня уже есть llama.cpp…
          </button>
        </div>
      {:else}
        <div class="row">
          <input
            class="input"
            bind:value={llamaDir}
            oninput={() => (llamaValid = null)}
            onblur={checkLlama}
            placeholder="Папка с llama-server.exe"
          />
          <button class="btn" onclick={browseLlama}>Обзор…</button>
        </div>
        <div class="hint">
          {#if checking}
            <span class="muted">Проверяю…</span>
          {:else if llamaValid === true}
            <span class="ok">✓ llama-server.exe найден</span>
          {:else if llamaValid === false}
            <span class="bad">✕ llama-server.exe не найден в этой папке</span>
          {:else}
            <span class="muted">Укажи папку, где лежит llama-server.exe</span>
          {/if}
        </div>
        <button class="linkish" onclick={() => (manual = false)}>← Автоустановка</button>
      {/if}

      {#if installing || progress}
        <div class="dl">
          <div class="dl-top">
            <span class="dl-stage">{progress?.stage ?? "Готовлю…"}</span>
            {#if progress && progress.total > 0}
              <span class="dl-num">
                {formatBytes(progress.downloaded)} / {formatBytes(progress.total)}
                · {pct.toFixed(0)}%
              </span>
            {/if}
            {#if installing}
              <button class="btn tiny" onclick={cancelInstall}>Отмена</button>
            {/if}
          </div>
          <div class="bar">
            <div
              class="bar-fill {progress && progress.total > 0 ? '' : 'indet'}"
              style="width:{progress && progress.total > 0 ? pct : 100}%"
            ></div>
          </div>
          {#if progress?.file}
            <div class="dl-file" title={progress.file}>{progress.file}</div>
          {/if}
        </div>
      {/if}

      {#if installError}
        <div class="bad err">{installError}</div>
      {/if}
    </section>

    <!-- Шаг 2: модели -->
    <section class="field">
      <span class="lbl">2 · Папки с моделями (.gguf)</span>
      {#each modelFolders as folder (folder)}
        <div class="folder-chip">
          <span class="path" title={folder}>{folder}</span>
          <button class="x" onclick={() => removeFolder(folder)} aria-label="Убрать">✕</button>
        </div>
      {/each}
      <button class="btn add" onclick={addModelFolder}>+ Добавить папку</button>
      <p class="hint muted">
        По умолчанию модели кладутся в папку <b>models</b> рядом с программой.
      </p>
    </section>

    <button class="btn btn-primary finish" disabled={!canFinish || saving} onclick={finish}>
      {saving ? "Сохраняю…" : "Начать"}
    </button>
  </div>
</div>

<style>
  .onb-wrap {
    height: 100vh;
    display: grid;
    place-items: center;
    padding: 24px;
  }
  .onb-card {
    width: min(560px, 100%);
    padding: 34px 34px 30px;
    display: flex;
    flex-direction: column;
    gap: 22px;
    animation: rise 0.4s cubic-bezier(0.2, 0.7, 0.2, 1);
    max-height: calc(100vh - 48px);
    overflow-y: auto;
  }
  @keyframes rise {
    from { opacity: 0; transform: translateY(14px); }
    to { opacity: 1; transform: translateY(0); }
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 16px;
  }
  .logo-orb {
    width: 46px;
    height: 46px;
    border-radius: 13px;
    background:
      radial-gradient(circle at 32% 26%, #ffbb74, var(--accent) 42%, #b45f16 78%, #5c2f08);
    box-shadow:
      0 6px 22px var(--accent-glow),
      inset 0 1px 2px rgba(255, 236, 210, 0.6),
      inset 0 -4px 10px rgba(0, 0, 0, 0.4);
    flex: none;
  }
  h1 { font-size: 22px; }
  .sub { margin: 2px 0 0; color: var(--text-1); font-size: 13px; }
  .lead { margin: 0; color: var(--text-1); line-height: 1.5; }
  .field { display: flex; flex-direction: column; gap: 10px; }
  .lbl {
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-2);
    font-weight: 600;
  }
  .row { display: flex; gap: 10px; }
  .row .input { flex: 1; }
  .hint { font-size: 12.5px; min-height: 16px; }
  .ok { color: var(--ok); }
  .bad { color: var(--danger); }
  .muted { color: var(--text-2); }
  .err { font-size: 12.5px; line-height: 1.4; }

  .auto-box { display: flex; flex-direction: column; gap: 10px; align-items: stretch; }
  .auto-why { margin: 0; font-size: 13px; color: var(--text-1); }
  .install { padding: 12px; font-size: 14.5px; }
  .linkish {
    align-self: flex-start;
    background: none; border: none; padding: 0;
    color: var(--text-2); font-size: 12.5px; cursor: pointer;
    text-decoration: underline; text-underline-offset: 3px;
  }
  .linkish:hover { color: var(--accent-hover); }
  .linkish:disabled { opacity: .5; cursor: default; }

  .engine-ok {
    display: flex; flex-wrap: wrap; align-items: center; gap: 10px 14px;
  }
  .engine-meta {
    font-family: var(--font-mono); font-size: 12px; color: var(--text-1);
    letter-spacing: -.02em;
  }
  .path-hint {
    margin: 0; font-size: 11px; color: var(--text-2);
    font-family: var(--font-mono); letter-spacing: -.02em;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
    direction: rtl; text-align: left;
  }
  .tiny { padding: 5px 10px; font-size: 12px; }

  .dl {
    display: flex; flex-direction: column; gap: 7px;
    padding: 12px 14px;
    background: rgba(0,0,0,.22);
    border: 1px solid var(--border);
    border-radius: var(--radius-m);
  }
  .dl-top { display: flex; align-items: center; gap: 10px; }
  .dl-stage { flex: 1; font-size: 13px; font-weight: 500; }
  .dl-num {
    font-size: 11.5px; color: var(--text-2);
    font-family: var(--font-mono); font-variant-numeric: tabular-nums;
  }
  .dl-file {
    font-size: 11px; color: var(--text-2);
    font-family: var(--font-mono); letter-spacing: -.02em;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .bar { height: 7px; border-radius: 4px; background: rgba(0,0,0,.35); overflow: hidden; }
  .bar-fill {
    height: 100%; border-radius: 4px;
    background: linear-gradient(90deg, var(--accent-press), var(--accent-hover));
    box-shadow: 0 0 10px var(--accent-glow);
    transition: width .2s ease;
  }
  .bar-fill.indet { animation: indet 1.1s ease-in-out infinite; }
  @keyframes indet {
    0% { opacity: .5; } 50% { opacity: 1; } 100% { opacity: .5; }
  }

  .folder-chip {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 12px;
    background: rgba(0,0,0,.22);
    border: 1px solid var(--border);
    border-radius: var(--radius-m);
  }
  .folder-chip .path {
    flex: 1;
    font-size: 12.5px;
    color: var(--text-1);
    font-family: var(--font-mono);
    letter-spacing: -.02em;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    direction: rtl;
    text-align: left;
  }
  .x {
    color: var(--text-2);
    font-size: 12px;
    padding: 2px 6px;
    border-radius: 6px;
  }
  .x:hover { color: var(--danger); background: rgba(255,107,107,.12); }
  .add { align-self: flex-start; }
  .finish { margin-top: 4px; padding: 13px; font-size: 15px; }
</style>
