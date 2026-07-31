<script lang="ts">
  import { BUS_IDS, type BusId } from "../lib/types";

  interface Props {
    routes: Record<BusId, boolean>;
    ontoggle: (bus: BusId, on: boolean) => void;
  }
  let { routes, ontoggle }: Props = $props();
</script>

<div class="routes">
  {#each BUS_IDS as bus}
    <button
      class="rbtn"
      class:active={routes[bus]}
      aria-pressed={routes[bus]}
      onclick={() => ontoggle(bus, !routes[bus])}
    >
      {bus}
    </button>
  {/each}
</div>

<style>
  .routes {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 4px;
  }
  .rbtn {
    height: 22px;
    font: 500 10px var(--mono);
    letter-spacing: 0.06em;
    background: var(--bg-3);
    color: var(--text-3);
    border: 1px solid var(--border-0);
    border-radius: 2px;
    box-shadow: inset 0 1px 0 var(--hl-top);
  }
  .rbtn:hover {
    background: var(--bg-4);
    color: var(--text-2);
  }
  .rbtn.active {
    background: var(--accent-glow);
    color: var(--accent);
    border-color: var(--accent);
  }
</style>
