<script lang="ts">
  import DeviceSelect from "./DeviceSelect.svelte";

  interface Option {
    value: string;
    label: string;
  }
  interface Props {
    label: string;
    busBadge?: string;
    deviceValue: string | null;
    deviceDisplay: string;
    deviceOptions: Option[];
    ondevicechange?: (value: string) => void;
    deviceDisabled?: boolean;
  }
  let { label, busBadge, deviceValue, deviceDisplay, deviceOptions, ondevicechange, deviceDisabled }: Props = $props();
</script>

<div class="head">
  <div class="nrow">
    {#if busBadge}<span class="busid">{busBadge}</span>{/if}
    <div class="name">{label}</div>
  </div>
  <DeviceSelect
    value={deviceValue}
    display={deviceDisplay}
    options={deviceOptions}
    onchange={ondevicechange}
    disabled={deviceDisabled}
  />
</div>

<style>
  .head {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .nrow {
    display: flex;
    align-items: baseline;
    gap: 6px;
    min-width: 0;
  }
  .busid {
    font: 600 10px var(--mono);
    letter-spacing: 0.08em;
    color: var(--accent);
  }
  .name {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-1);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
