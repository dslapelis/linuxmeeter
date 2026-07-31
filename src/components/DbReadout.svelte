<script lang="ts">
  import { fmtDb } from "../lib/utils/db";

  interface Props {
    value: number;
  }
  let { value }: Props = $props();
</script>

<div class="readout">{fmtDb(value)}</div>

<style>
  .readout {
    font: 500 11px var(--mono);
    letter-spacing: 0.02em;
    color: var(--text-1);
    text-align: center;
    margin-top: -2px;
  }
</style>
