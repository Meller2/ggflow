<script lang="ts">
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import {
    validateLlamaDir,
    pickFolder,
    saveSettings,
    runtimeStatus,
    runtimeInstall,
    runtimeCancelInstall,
    formatBytes,
    type Settings,
    type RuntimeStatus,
    type RuntimeProgress,
  } from "$lib/api";

  let { settings, onchange }: {
    settings: Settings;
    onchange: (s: Settings) => void;
  } = $props();

  let draft = $state<Settings>(structuredClone($state.snapshot(settings)));
  let llamaValid = $state<boolean | null>(null);
  let saved = $state(false);

  let rt = $state<RuntimeStatus | null>(null);
  let installing = $state(false);
  let progress = $state<RuntimeProgress | null>(null);
  let installError = $state<string | null>(null);

  let unlisten: UnlistenFn | null = null;
  $effect(() => {
    listen<RuntimeProgress>("runtime-progress", (e) => {
      progress = e.payload;
      if (e.payload.error) installError = e.payload.error;
    }).then((u) => (unlisten = u));
    return () => unlisten?.();
  });

  async function loadRt() {
    try {
      rt = await runtimeStatus();
    } catch {
      rt = null;
    }
  }
  loadRt();

  async function checkLlama() {
    if (!draft.llama_dir) { llamaValid = false; return; }
    llamaValid = await validateLlamaDir(draft.llama_dir);
  }

  async function browseLlama() {
    const dir = await pickFolder("Папка с llama-server.exe");
    if (dir) {
      draft.llama_dir = dir;
      draft.runtime_managed = false;
      await checkLlama();
    }
  }

  async function addFolder() {
    const dir = await pickFolder("Папка с моделями (.gguf)");
    if (dir && !draft.model_folders.includes(dir)) {
      draft.model_folders = [...draft.model_folders, dir];
    }
  }

  function removeFolder(f: string) {
    draft.model_folders = draft.model_folders.filter((x) => x !== f);
  }

  async function installEngine() {
    if (installing) return;
    installing = true;
    installError = null;
    progress = null;
    try {
      const st = await runtimeInstall(null);
      rt = st;
      if (st.llama_dir) {
        draft.llama_dir = st.llama_dir;
        draft.runtime_managed = true;
        draft.runtime_tag = st.tag;
        draft.runtime_backend = st.backend;
        llamaValid = true;
      }
      if (st.default_models_dir && !draft.model_folders.includes(st.default_models_dir)) {
        draft.model_folders = [...draft.model_folders, st.default_models_dir];
      }
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

  async function save() {
    await saveSettings($state.snapshot(draft));
    onchange($state.snapshot(draft));
    saved = true;
    setTimeout(() => (saved = false), 1800);
  }

  $effect(() => { if (llamaValid === null) checkLlama(); });

  const KV_OPTS = ["f16", "q8_0", "q4_0"];
  const pct = $derived(
    progress && progress.total > 0
      ? Math.min(100, (progress.downloaded / progress.total) * 100)
      : 0,
  );
</script>

<div class="page">
  <header><h2>Настройки</h2></header>

  <div class="glass block">
    <span class="lbl">Движок llama.cpp</span>

    {#if rt?.installed || draft.runtime_managed}
      <div class="engine-row">
        <div>
          <div class="engine-title">
            {#if llamaValid}
              <span class="ok">✓ Установлен</span>
            {:else}
              <span class="bad">✕ Не найден</span>
            {/if}
            {#if draft.runtime_tag || draft.runtime_backend || rt?.backend_label}
              <span class="meta">
                {draft.runtime_tag ?? rt?.tag ?? ""}
                {#if (draft.runtime_tag || rt?.tag) && (rt?.backend_label || draft.runtime_backend)} · {/if}
                {rt?.backend_label ?? draft.runtime_backend ?? ""}
              </span>
            {/if}
          </div>
          {#if draft.llama_dir}
            <p class="path-hint" title={draft.llama_dir}>{draft.llama_dir}</p>
          {/if}
          {#if draft.runtime_managed || rt?.installed}
            <p class="hint muted">
              Managed runtime: рядом с программой, если туда можно писать; иначе в LocalAppData.
            </p>
          {/if}
        </div>
        <button class="btn" onclick={installEngine} disabled={installing}>
          {installing ? "Устанавливаю…" : "Обновить / переустановить"}
        </button>
      </div>
    {:else}
      <p class="hint muted">
        Движок можно поставить автоматически
        {#if rt} (рекомендуем: <b>{rt.recommended_label}</b>){/if}.
      </p>
      <button class="btn btn-primary" onclick={installEngine} disabled={installing}>
        {installing ? "Устанавливаю…" : "↓ Установить llama.cpp"}
      </button>
    {/if}

    {#if installing || (progress && !progress.done && !progress.canceled)}
      <div class="dl">
        <div class="dl-top">
          <span>{progress?.stage ?? "Готовлю…"}</span>
          {#if progress && progress.total > 0}
            <span class="dl-num">
              {formatBytes(progress.downloaded)} / {formatBytes(progress.total)} · {pct.toFixed(0)}%
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
      </div>
    {/if}
    {#if installError}
      <div class="bad hint">{installError}</div>
    {/if}

    <details class="adv">
      <summary>Указать папку вручную</summary>
      <div class="row">
        <input class="input" bind:value={draft.llama_dir}
          oninput={() => { llamaValid = null; draft.runtime_managed = false; }}
          onblur={checkLlama} />
        <button class="btn" onclick={browseLlama}>Обзор…</button>
      </div>
      <div class="hint">
        {#if llamaValid === true}<span class="ok">✓ llama-server.exe найден</span>
        {:else if llamaValid === false}<span class="bad">✕ llama-server.exe не найден</span>
        {:else}<span class="muted">Проверка…</span>{/if}
      </div>
    </details>
  </div>

  <div class="glass block">
    <span class="lbl">Папки с моделями</span>
    {#each draft.model_folders as f (f)}
      <div class="chip">
        <span class="path" title={f}>{f}</span>
        <button class="x" onclick={() => removeFolder(f)} aria-label="Убрать">✕</button>
      </div>
    {/each}
    <button class="btn add" onclick={addFolder}>+ Добавить папку</button>
  </div>

  <div class="glass block">
    <span class="lbl">Параметры запуска по умолчанию</span>
    <div class="grid">
      <div class="fld">
        <span class="fl">Контекст</span>
        <input class="input" type="number" min="512" step="512" bind:value={draft.defaults.ctx} />
      </div>
      <div class="fld">
        <span class="fl">KV-квант</span>
        <select class="input" bind:value={draft.defaults.kv_quant}>
          {#each KV_OPTS as k}<option value={k}>{k}</option>{/each}
        </select>
      </div>
      <div class="fld">
        <span class="fl">Потоки CPU</span>
        <input class="input" type="number" min="1" max="64" bind:value={draft.defaults.threads} />
      </div>
      <div class="fld">
        <span class="fl">Слоёв на GPU (-ngl)</span>
        <input class="input" type="number" min="0" max="999" bind:value={draft.defaults.ngl} />
      </div>
      <div class="fld">
        <span class="fl">Порт</span>
        <input class="input" type="number" min="1024" max="65535" bind:value={draft.defaults.port} />
      </div>
      <div class="fld check">
        <label class="chk">
          <input type="checkbox" bind:checked={draft.defaults.tools} />
          <span>Инструменты (--tools all)</span>
        </label>
      </div>
    </div>
  </div>

  <div class="save-row">
    <button class="btn btn-primary" onclick={save}>Сохранить</button>
    {#if saved}<span class="saved-msg">✓ Сохранено</span>{/if}
  </div>
</div>

<style>
  .page { display: flex; flex-direction: column; gap: 16px; overflow-y: auto; padding-right: 6px; }
  h2 { font-size: 20px; }
  .block { padding: 18px 20px; display: flex; flex-direction: column; gap: 12px; }
  .lbl {
    font-size: 12px; text-transform: uppercase; letter-spacing: .06em;
    color: var(--text-2); font-weight: 600;
  }
  .row { display: flex; gap: 10px; }
  .row .input { flex: 1; }
  .hint { font-size: 12.5px; }
  .ok { color: var(--ok); } .bad { color: var(--danger); } .muted { color: var(--text-2); }

  .engine-row {
    display: flex; justify-content: space-between; align-items: flex-start; gap: 14px;
  }
  .engine-title { display: flex; flex-wrap: wrap; align-items: center; gap: 8px 12px; }
  .meta {
    font-family: var(--font-mono); font-size: 12px; color: var(--text-1);
    letter-spacing: -.02em;
  }
  .path-hint {
    margin: 6px 0 0; font-size: 11px; color: var(--text-2);
    font-family: var(--font-mono); letter-spacing: -.02em;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
    direction: rtl; text-align: left; max-width: 420px;
  }
  .tiny { padding: 5px 10px; font-size: 12px; }

  .dl {
    display: flex; flex-direction: column; gap: 7px;
    padding: 12px 14px;
    background: rgba(0,0,0,.22);
    border: 1px solid var(--border);
    border-radius: var(--radius-m);
  }
  .dl-top { display: flex; align-items: center; gap: 10px; font-size: 13px; }
  .dl-top > span:first-child { flex: 1; font-weight: 500; }
  .dl-num {
    font-size: 11.5px; color: var(--text-2);
    font-family: var(--font-mono); font-variant-numeric: tabular-nums;
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

  .adv { font-size: 13px; color: var(--text-1); }
  .adv summary { cursor: pointer; color: var(--text-2); margin-bottom: 10px; }
  .adv[open] summary { margin-bottom: 12px; }
  .adv .row { margin-bottom: 8px; }

  .chip {
    display: flex; align-items: center; gap: 10px;
    padding: 9px 12px; background: rgba(0,0,0,.22);
    border: 1px solid var(--border); border-radius: var(--radius-m);
  }
  .chip .path {
    flex: 1; font-size: 12.5px; color: var(--text-1);
    font-family: var(--font-mono); letter-spacing: -.02em;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
    direction: rtl; text-align: left;
  }
  .x { color: var(--text-2); font-size: 12px; padding: 2px 6px; border-radius: 6px; }
  .x:hover { color: var(--danger); background: rgba(255,107,107,.12); }
  .add { align-self: flex-start; }
  .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 14px; }
  .fld { display: flex; flex-direction: column; gap: 6px; }
  .fl { font-size: 12.5px; color: var(--text-1); }
  .check { justify-content: flex-end; }
  .chk { display: flex; align-items: center; gap: 8px; font-size: 13px; cursor: pointer; }
  .chk input { width: 16px; height: 16px; accent-color: var(--accent); }
  .save-row { display: flex; align-items: center; gap: 14px; padding-bottom: 8px; }
  .saved-msg { color: var(--ok); font-size: 13px; }
</style>
