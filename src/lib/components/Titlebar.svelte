<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { prefs } from "$lib/prefs.svelte";
  import { serverState } from "$lib/server.svelte";
  import Icon from "./Icon.svelte";

  const win = getCurrentWindow();
  let maximized = $state(false);

  async function syncMax() {
    try {
      maximized = await win.isMaximized();
    } catch {
      /* browser / no tauri */
    }
  }
  syncMax();
  win.onResized(() => {
    syncMax();
  }).catch(() => {});

  async function minimize() {
    try {
      await win.minimize();
    } catch {
      /* */
    }
  }
  async function toggleMax() {
    try {
      await win.toggleMaximize();
      await syncMax();
    } catch {
      /* */
    }
  }
  async function close() {
    try {
      await win.close();
    } catch {
      /* */
    }
  }

</script>

<header class="titlebar" data-tauri-drag-region>
  <div class="left" data-tauri-drag-region>
    <img class="logo" src="/logo1.png" alt="" draggable="false" />
    <span class="app-name" data-tauri-drag-region>{prefs.t("app.name")}</span>
    {#if serverState.running}
      <span
        class="pill {serverState.ready ? 'ok' : 'busy'}"
        title={serverState.modelName ?? ""}
      >
        <span class="dot"></span>
        {#if serverState.ready}
          {serverState.modelName
            ? serverState.modelName.replace(/\.gguf$/i, "")
            : prefs.t("app.tab.running")}
        {:else}
          {prefs.t("models.launching")}
        {/if}
      </span>
    {/if}
  </div>

  <!-- Двойной клик по пустой зоне = maximize (как в Windows). -->
  <div
    class="drag-fill"
    data-tauri-drag-region
    role="presentation"
    ondblclick={toggleMax}
  ></div>

  <div class="controls">
    <button class="win-btn" onclick={minimize} aria-label="Minimize" tabindex="-1">
      <Icon name="minimize" size={14} stroke={1.75} />
    </button>
    <button class="win-btn" onclick={toggleMax} aria-label="Maximize" tabindex="-1">
      <Icon name={maximized ? "restore" : "maximize"} size={13} stroke={1.75} />
    </button>
    <button class="win-btn close" onclick={close} aria-label="Close" tabindex="-1">
      <Icon name="close" size={14} stroke={1.75} />
    </button>
  </div>
</header>

<style>
  .titlebar {
    height: var(--titlebar-h);
    flex: none;
    display: flex;
    align-items: stretch;
    background: linear-gradient(180deg, rgba(255, 250, 244, 0.035), transparent 90%),
      var(--bg-1);
    border-bottom: 1px solid var(--border);
    user-select: none;
    z-index: 50;
  }

  .left {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 12px 0 14px;
    min-width: 0;
  }
  .logo {
    width: 22px;
    height: 22px;
    object-fit: contain;
    border-radius: 5px;
    -webkit-user-drag: none;
  }
  .app-name {
    font-family: var(--font-display);
    font-weight: 600;
    font-size: 13px;
    letter-spacing: -0.02em;
    color: var(--text-0);
    white-space: nowrap;
  }

  .pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    max-width: 220px;
    margin-left: 6px;
    padding: 3px 10px 3px 8px;
    border-radius: 999px;
    font-size: 11.5px;
    font-weight: 500;
    color: var(--text-1);
    background: var(--surface);
    border: 1px solid var(--border);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pill .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    flex: none;
  }
  .pill.ok {
    color: var(--ok);
    border-color: rgba(75, 208, 127, 0.28);
    background: rgba(75, 208, 127, 0.08);
  }
  .pill.ok .dot {
    background: var(--ok);
    box-shadow: 0 0 8px var(--ok-glow);
  }
  .pill.busy {
    color: var(--warn);
    border-color: rgba(255, 194, 71, 0.3);
    background: rgba(255, 194, 71, 0.08);
  }
  .pill.busy .dot {
    background: var(--warn);
    animation: pulse-dot 1s ease infinite;
  }
  @keyframes pulse-dot {
    50% {
      opacity: 0.35;
    }
  }

  .drag-fill {
    flex: 1;
    min-width: 24px;
  }

  .controls {
    display: flex;
    align-items: stretch;
    flex: none;
    /* Не drag — иначе кнопки не кликаются */
  }
  .win-btn {
    width: 46px;
    display: grid;
    place-items: center;
    color: var(--text-1);
    transition: background 0.12s ease, color 0.12s ease;
  }
  .win-btn:hover {
    background: var(--surface-hover);
    color: var(--text-0);
  }
  .win-btn.close:hover {
    background: #e81123;
    color: #fff;
  }
  .win-btn:active {
    background: var(--surface-active);
  }
  .win-btn.close:active {
    background: #c50f1f;
  }
</style>
