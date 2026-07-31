<script lang="ts">
  interface Option {
    value: string;
    label: string;
  }
  interface Props {
    value: string | null;
    display: string;
    options: Option[];
    onchange?: (value: string) => void;
    disabled?: boolean;
  }
  let { value, display, options, onchange, disabled = false }: Props = $props();

  let open = $state(false);
  let root: HTMLDivElement;

  function onWindowPointerDown(e: PointerEvent) {
    if (open && root && !root.contains(e.target as Node)) open = false;
  }
</script>

<svelte:window onpointerdown={onWindowPointerDown} />

<div class="wrap" bind:this={root}>
  <button
    class="dev"
    {disabled}
    aria-haspopup="listbox"
    aria-expanded={open}
    onclick={() => {
      if (!disabled) open = !open;
    }}
  >
    <span>{display}</span>
    <svg width="10" height="10" viewBox="0 0 10 10" fill="none">
      <path d="M2.5 4l2.5 2.5L7.5 4" stroke="currentColor" stroke-width="1.25" />
    </svg>
  </button>
  {#if open}
    <div class="menu" role="listbox">
      {#each options as opt}
        <button
          class="item"
          role="option"
          aria-selected={opt.value === value}
          class:selected={opt.value === value}
          onclick={() => {
            open = false;
            onchange?.(opt.value);
          }}
        >
          {opt.label}
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .wrap {
    position: relative;
  }
  .dev {
    width: 100%;
    height: 22px;
    padding: 0 6px;
    display: flex;
    align-items: center;
    gap: 4px;
    background: var(--bg-3);
    border: 1px solid var(--border-0);
    border-radius: 2px;
    box-shadow: inset 0 1px 0 var(--hl-top);
    font-size: 11px;
    color: var(--text-2);
    text-align: left;
  }
  .dev:hover:not(:disabled) {
    background: var(--bg-4);
    color: var(--text-1);
  }
  .dev:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .dev span {
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .dev svg {
    flex: none;
    color: var(--text-3);
  }
  .menu {
    position: absolute;
    top: calc(100% + 2px);
    left: 0;
    min-width: 100%;
    max-width: 220px;
    z-index: 30;
    background: var(--bg-3);
    border: 1px solid var(--border-1);
    border-radius: 6px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.6);
    padding: 3px;
    display: flex;
    flex-direction: column;
  }
  .item {
    padding: 5px 8px;
    font-size: 11px;
    color: var(--text-2);
    text-align: left;
    border-radius: 2px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .item:hover {
    background: var(--bg-4);
    color: var(--text-1);
  }
  .item.selected {
    color: var(--accent);
  }
</style>
