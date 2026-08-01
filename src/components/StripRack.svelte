<script lang="ts">
  import { mixer } from "../lib/state/mixer.svelte";
  import { setContentSize } from "../lib/window";
  import InputStrip from "./InputStrip.svelte";
  import BusStrip from "./BusStrip.svelte";

  let hardware = $derived(mixer.strips.filter((s) => s.kind === "hardware"));
  let virtual = $derived(mixer.strips.filter((s) => s.kind === "virtual"));

  const WINDOW_H = 800;
  let rack: HTMLElement;

  // The window is non-resizable; fit it to the strip content whenever the
  // strip/bus population changes.
  $effect(() => {
    void mixer.strips.length;
    void mixer.buses.length;
    if (!rack) return;
    requestAnimationFrame(() => {
      const groups = Array.from(rack.querySelectorAll<HTMLElement>(".group"));
      if (groups.length === 0) return;
      const width = groups.reduce((sum, g) => sum + g.offsetWidth, 0) + 16 * (groups.length + 1);
      void setContentSize(width, WINDOW_H);
    });
  });
</script>

<main class="rack" bind:this={rack}>
  <section class="group">
    <div class="grouplabel">Hardware</div>
    <div class="striprow">
      {#each hardware as strip (strip.id)}
        <InputStrip {strip} />
      {/each}
    </div>
  </section>

  <section class="group">
    <div class="grouplabel">Virtual</div>
    <div class="striprow">
      {#each virtual as strip (strip.id)}
        <InputStrip {strip} />
      {/each}
    </div>
  </section>

  <section class="group busgroup">
    <div class="grouplabel">Buses</div>
    <div class="striprow">
      {#each mixer.buses as bus (bus.id)}
        <BusStrip {bus} />
      {/each}
    </div>
  </section>
</main>

<style>
  .rack {
    flex: 1;
    display: flex;
    align-items: stretch;
    gap: 16px;
    padding: 16px;
    background: var(--bg-1);
    overflow: hidden;
  }
  .group {
    display: flex;
    flex-direction: column;
    gap: 8px;
    flex: none;
    contain: layout paint style;
  }
  .grouplabel {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-2);
    padding-left: 2px;
  }
  .striprow {
    display: flex;
    align-items: stretch;
    gap: 8px;
    flex: 1;
  }
</style>
