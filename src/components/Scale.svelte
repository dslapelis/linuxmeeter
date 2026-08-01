<script lang="ts">
  import { dbToPos } from "../lib/utils/db";

  const TICKS = [12, 6, 0, -6, -12, -20, -30, -40, -60];

  let h = $state(200);
</script>

<div class="scale" bind:clientHeight={h}>
  {#each TICKS as db}
    <div class="tick" class:zero={db === 0} style:top="{(1 - dbToPos(db)) * h}px">
      {db === 0 ? "0" : Math.abs(db)}<i></i>
    </div>
  {/each}
</div>

<style>
  .scale {
    position: relative;
    width: 23px;
    height: 100%;
    min-height: var(--fader-h);
  }
  .tick {
    position: absolute;
    right: 0;
    transform: translateY(-50%);
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 3px;
    width: 100%;
    font: 500 9px var(--mono);
    color: var(--text-3);
  }
  .tick i {
    display: block;
    width: 4px;
    height: 1px;
    background: var(--border-1);
  }
  .tick.zero {
    color: var(--text-2);
  }
  .tick.zero i {
    width: 7px;
    background: var(--text-3);
  }
</style>
