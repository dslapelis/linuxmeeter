<script lang="ts">
  import type { BusState } from "../lib/types";
  import { mixer } from "../lib/state/mixer.svelte";
  import StripHeader from "./StripHeader.svelte";
  import Meter from "./Meter.svelte";
  import Scale from "./Scale.svelte";
  import Fader from "./Fader.svelte";
  import DbReadout from "./DbReadout.svelte";
  import Knob from "./Knob.svelte";
  import MuteSoloButtons from "./MuteSoloButtons.svelte";

  interface Props {
    bus: BusState;
  }
  let { bus }: Props = $props();

  let isVirtual = $derived(bus.targetHwKey === null);
  let deviceOptions = $derived(
    isVirtual
      ? []
      : mixer.devices.filter((d) => d.mediaClass === "Audio/Sink").map((d) => ({ value: d.nodeName, label: d.description })),
  );
  let deviceDisplay = $derived(
    isVirtual
      ? "virtual source"
      : (mixer.devices.find((d) => d.nodeName === bus.targetHwKey)?.description ?? bus.targetHwKey ?? "—"),
  );
</script>

<div class="strip" class:offline={!bus.online}>
  <StripHeader
    label={bus.label}
    busBadge={bus.id}
    deviceValue={bus.targetHwKey}
    {deviceDisplay}
    {deviceOptions}
    deviceDisabled={isVirtual}
    ondevicechange={(v) => mixer.setBusTarget(bus.id, v)}
  />

  <div class="mf">
    <Meter key={`b:${bus.id}`} />
    <Scale />
    <Fader
      value={bus.gainDb}
      onchange={(db) => mixer.setGain({ bus: bus.id }, db)}
      onstart={() => mixer.beginGainDrag({ bus: bus.id })}
      onend={() => mixer.endGainDrag({ bus: bus.id })}
    />
  </div>
  <DbReadout value={bus.gainDb} />

  <div class="sep"></div>
  <div class="knobs">
    <Knob
      label="LIM"
      min={-12}
      max={0}
      value={bus.limiter.thresholdDb}
      defaultValue={-1}
      enabled={bus.limiter.enabled}
      onchange={(v) => mixer.setLimiterParams(bus.id, { ...bus.limiter, thresholdDb: v, enabled: true })}
      onenabledchange={(on) => mixer.setLimiterParams(bus.id, { ...bus.limiter, enabled: on })}
    />
  </div>

  <MuteSoloButtons mute={bus.mute} onmute={(on) => mixer.setMute({ bus: bus.id }, on)} />
</div>

<style>
  .strip {
    width: var(--bus-w);
    flex: none;
    background: var(--bg-2);
    border: 1px solid var(--border-0);
    border-radius: 4px;
    box-shadow: inset 0 1px 0 var(--hl-top);
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .strip.offline {
    opacity: 0.45;
  }
  .sep {
    height: 1px;
    background: var(--border-0);
    margin: 0 -8px;
  }
  .mf {
    display: flex;
    justify-content: center;
    gap: 5px;
    padding-top: 8px;
    /* Extra strip height becomes fader/meter travel, not blank card. */
    flex: 1;
    min-height: calc(var(--fader-h) + 8px);
  }
  .knobs {
    display: flex;
    justify-content: center;
    gap: 8px;
  }
</style>
