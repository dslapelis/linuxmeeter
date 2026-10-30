<script lang="ts">
  import { mixer } from "../lib/state/mixer.svelte";
  import InputStrip from "./InputStrip.svelte";
  import BusStrip from "./BusStrip.svelte";

  let hardware = $derived(mixer.strips.filter((s) => s.kind === "hardware"));
  let virtual = $derived(mixer.strips.filter((s) => s.kind === "virtual"));
</script>

<main class="rack">
  {#if hardware.length}
    <section class="group" style:--columns={hardware.length}>
      <div class="grouplabel">Hardware</div>
      <div class="striprow">
        {#each hardware as strip (strip.id)}
          <InputStrip {strip} />
        {/each}
      </div>
    </section>
  {/if}

  {#if virtual.length}
    <section class="group" style:--columns={virtual.length}>
      <div class="grouplabel">Virtual</div>
      <div class="striprow">
        {#each virtual as strip (strip.id)}
          <InputStrip {strip} />
        {/each}
      </div>
    </section>
  {/if}

  <section class="group busgroup" style:--columns={Math.max(1, mixer.buses.length)}>
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
    flex-wrap: wrap;
    align-items: stretch;
    align-content: stretch;
    min-width: 0;
    min-height: 0;
    gap: 12px;
    padding: 12px;
    background: var(--bg-1);
    overflow: auto;
  }
  .group {
    display: flex;
    flex-direction: column;
    gap: 8px;
    --channel-min: 120px;
    flex: var(--columns) 1 calc(var(--columns) * var(--channel-min) + (var(--columns) - 1) * 8px);
    min-width: 0;
    contain: layout paint style;
  }
  .busgroup {
    --channel-min: 96px;
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
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, var(--channel-min)), 1fr));
    align-items: stretch;
    gap: 8px;
    flex: 1;
  }
</style>
