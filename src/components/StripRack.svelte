<script lang="ts">
  import { mixer } from "../lib/state/mixer.svelte";
  import InputStrip from "./InputStrip.svelte";
  import BusStrip from "./BusStrip.svelte";

  let hardware = $derived(mixer.strips.filter((s) => s.kind === "hardware"));
  let virtual = $derived(mixer.strips.filter((s) => s.kind === "virtual"));
</script>

<main class="rack">
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
    /* Spare width distributes between the groups instead of pooling into one
       void before the buses; 16px is the minimum gap when space is tight. */
    justify-content: space-between;
    gap: 16px;
    padding: 16px;
    background: var(--bg-1);
    overflow-x: auto;
    overflow-y: auto;
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
  .busgroup {
    position: sticky;
    right: 16px;
    background: var(--bg-1);
    padding-left: 16px;
    border-left: 1px solid var(--border-0);
    box-shadow: -16px 0 24px rgba(10, 11, 13, 0.55);
  }
</style>
