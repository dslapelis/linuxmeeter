<script lang="ts">
  interface Props {
    mute: boolean;
    onmute: (on: boolean) => void;
    solo?: boolean;
    onsolo?: (on: boolean) => void;
  }
  let { mute, onmute, solo, onsolo }: Props = $props();
</script>

<div class="ms">
  <button class="msbtn m" class:active={mute} aria-pressed={mute} onclick={() => onmute(!mute)}>M</button>
  {#if onsolo}
    <button class="msbtn s" class:active={solo} aria-pressed={solo} onclick={() => onsolo?.(!solo)}>S</button>
  {/if}
</div>

<style>
  .ms {
    display: flex;
    gap: 4px;
  }
  .msbtn {
    flex: 1;
    height: 22px;
    font: 500 10px var(--mono);
    letter-spacing: 0.06em;
    background: var(--bg-3);
    color: var(--text-3);
    border: 1px solid var(--border-0);
    border-radius: 2px;
    box-shadow: inset 0 1px 0 var(--hl-top);
  }
  .msbtn:hover {
    background: var(--bg-4);
    color: var(--text-2);
  }
  .msbtn.m.active {
    background: var(--mute);
    color: #1a0506;
    border-color: var(--mute);
  }
  .msbtn.s.active {
    background: var(--solo);
    color: #1a1002;
    border-color: var(--solo);
  }
</style>
