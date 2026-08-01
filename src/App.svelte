<script lang="ts">
  import { onMount } from "svelte";
  import { IS_TAURI } from "./lib/ipc";
  import { mixer } from "./lib/state/mixer.svelte";
  import { ui } from "./lib/state/ui.svelte";
  import TopBar from "./components/TopBar.svelte";
  import StripRack from "./components/StripRack.svelte";
  import ResizeEdges from "./components/ResizeEdges.svelte";
  import EqPanel from "./components/EqPanel.svelte";
  import DynPanel from "./components/DynPanel.svelte";

  onMount(() => {
    void mixer.init();
  });

  let eqStrip = $derived(ui.eqStrip !== null ? mixer.strip(ui.eqStrip) : undefined);
  let dynStrip = $derived(ui.dynStrip !== null ? mixer.strip(ui.dynStrip) : undefined);
</script>

<div class="app">
  <TopBar />
  {#if mixer.ready}
    <StripRack />
  {:else}
    <main class="loading">connecting…</main>
  {/if}
</div>
{#if eqStrip}
  <EqPanel strip={eqStrip} />
{/if}
{#if dynStrip}
  <DynPanel strip={dynStrip} />
{/if}
{#if IS_TAURI}
  <ResizeEdges />
{/if}

<style>
  .app {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  .loading {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--bg-1);
    color: var(--text-3);
    font: 500 11px var(--mono);
    letter-spacing: 0.08em;
  }
</style>
