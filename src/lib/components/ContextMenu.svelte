<script lang="ts" module>
  export type MenuItem =
    | {
        id: string;
        label: string;
        danger?: boolean;
        disabled?: boolean;
      }
    | { type: "sep" };
</script>

<script lang="ts">
  import { onMount } from "svelte";

  let {
    x,
    y,
    items,
    onpick,
    onclose,
  }: {
    x: number;
    y: number;
    items: MenuItem[];
    onpick: (id: string) => void;
    onclose: () => void;
  } = $props();

  let root: HTMLDivElement | undefined = $state();
  let pos = $state({ left: x, top: y });

  onMount(() => {
    const el = root;
    if (el) {
      const r = el.getBoundingClientRect();
      let left = x;
      let top = y;
      if (left + r.width > window.innerWidth - 8) left = window.innerWidth - r.width - 8;
      if (top + r.height > window.innerHeight - 8) top = window.innerHeight - r.height - 8;
      if (left < 8) left = 8;
      if (top < 8) top = 8;
      pos = { left, top };
    }

    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onclose();
    };
    const onDown = (e: MouseEvent) => {
      if (root && !root.contains(e.target as Node)) onclose();
    };
    const onScroll = () => onclose();
    window.addEventListener("keydown", onKey);
    window.addEventListener("mousedown", onDown, true);
    window.addEventListener("wheel", onScroll, true);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("mousedown", onDown, true);
      window.removeEventListener("wheel", onScroll, true);
    };
  });
</script>

<div
  class="menu"
  bind:this={root}
  style:left="{pos.left}px"
  style:top="{pos.top}px"
  role="menu"
  oncontextmenu={(e) => e.preventDefault()}
>
  {#each items as item}
    {#if "type" in item && item.type === "sep"}
      <div class="sep" role="separator"></div>
    {:else if "id" in item}
      <button
        type="button"
        class="item {item.danger ? 'danger' : ''}"
        role="menuitem"
        disabled={item.disabled}
        onclick={() => {
          if (!item.disabled) {
            onpick(item.id);
            onclose();
          }
        }}
      >
        {item.label}
      </button>
    {/if}
  {/each}
</div>

<style>
  .menu {
    position: fixed;
    z-index: 200;
    min-width: 200px;
    padding: 5px;
    border-radius: 10px;
    background: linear-gradient(180deg, #1a1a1f, #141418);
    border: 1px solid var(--border-strong);
    box-shadow:
      0 12px 40px rgba(0, 0, 0, 0.55),
      0 0 0 1px rgba(255, 250, 244, 0.04);
    animation: menu-in 0.1s ease both;
  }
  @keyframes menu-in {
    from {
      opacity: 0;
      transform: scale(0.97) translateY(-2px);
    }
    to {
      opacity: 1;
      transform: none;
    }
  }
  .item {
    display: flex;
    width: 100%;
    text-align: left;
    padding: 8px 12px;
    border-radius: 7px;
    font-size: 13px;
    font-weight: 500;
    color: var(--text-0);
    transition: background 0.1s ease;
  }
  .item:hover:not(:disabled) {
    background: var(--accent-soft);
    color: var(--accent-hover);
  }
  .item:disabled {
    opacity: 0.35;
    cursor: default;
  }
  .item.danger:hover:not(:disabled) {
    background: var(--danger-soft);
    color: var(--danger);
  }
  .sep {
    height: 1px;
    margin: 4px 8px;
    background: var(--border);
  }
</style>
