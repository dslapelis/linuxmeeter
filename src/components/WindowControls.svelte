<script lang="ts">
  import { close, minimize } from "../lib/window";
</script>

<div class="controls">
  <button class="winbtn" title="Minimize" onclick={() => minimize()}>
    <svg width="12" height="12" viewBox="0 0 12 12"><path d="M2 6h8" stroke="currentColor" stroke-width="1.25" /></svg>
  </button>
  <button class="winbtn close" title="Close (stays in tray)" onclick={() => close()}>
    <svg width="12" height="12" viewBox="0 0 12 12"><path d="M2.5 2.5l7 7M9.5 2.5l-7 7" stroke="currentColor" stroke-width="1.25" /></svg>
  </button>
</div>

<style>
  .controls {
    display: flex;
    align-items: center;
    height: 100%;
  }
  .winbtn {
    width: 46px;
    height: var(--topbar-h);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-2);
  }
  .winbtn:hover {
    background: var(--bg-4);
    color: var(--text-1);
  }
  .winbtn.close:hover {
    background: var(--mute);
    color: #fff;
  }
</style>
