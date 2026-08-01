<script lang="ts">
  import type { StripState } from "../lib/types";
  import { mixer } from "../lib/state/mixer.svelte";
  import { ui } from "../lib/state/ui.svelte";
  import StripHeader from "./StripHeader.svelte";
  import Meter from "./Meter.svelte";
  import Scale from "./Scale.svelte";
  import Fader from "./Fader.svelte";
  import DbReadout from "./DbReadout.svelte";
  import Knob from "./Knob.svelte";
  import RoutingToggleGroup from "./RoutingToggleGroup.svelte";
  import MuteSoloButtons from "./MuteSoloButtons.svelte";
  import ColorPad from "./ColorPad.svelte";

  interface Props {
    strip: StripState;
  }
  let { strip }: Props = $props();

  let deviceOptions = $derived(
    strip.kind === "hardware"
      ? mixer.devices.filter((d) => d.mediaClass === "Audio/Source").map((d) => ({ value: d.nodeName, label: d.description }))
      : [],
  );
  let deviceDisplay = $derived(
    strip.kind === "hardware"
      ? (mixer.devices.find((d) => d.nodeName === strip.hwKey)?.description ?? strip.hwKey ?? "—")
      : `lm.strip.${strip.id}`,
  );
</script>

<div class="strip" class:offline={!strip.online}>
  <StripHeader
    label={strip.label}
    deviceValue={strip.hwKey}
    {deviceDisplay}
    {deviceOptions}
    deviceDisabled={strip.kind === "virtual"}
    ondevicechange={(v) => mixer.setStripDevice(strip.id, v)}
  />

  <div class="mf">
    <Meter key={`s:${strip.id}`} />
    <Scale />
    <Fader
      value={strip.gainDb}
      onchange={(db) => mixer.setGain({ strip: strip.id }, db)}
      onstart={() => mixer.beginGainDrag({ strip: strip.id })}
      onend={() => mixer.endGainDrag({ strip: strip.id })}
    />
  </div>
  <DbReadout value={strip.gainDb} />

  <div class="sep"></div>
  <div class="knobs">
    <Knob
      label="GATE"
      min={-70}
      max={0}
      value={strip.gate.thresholdDb}
      defaultValue={-40}
      enabled={strip.gate.enabled}
      onchange={(v) => mixer.setGateParams(strip.id, { ...strip.gate, thresholdDb: v, enabled: true })}
      onenabledchange={(on) => mixer.setGateParams(strip.id, { ...strip.gate, enabled: on })}
    />
    <Knob
      label="COMP"
      min={-60}
      max={0}
      value={strip.comp.thresholdDb}
      defaultValue={-18}
      enabled={strip.comp.enabled}
      onchange={(v) => mixer.setCompParams(strip.id, { ...strip.comp, thresholdDb: v, enabled: true })}
      onenabledchange={(on) => mixer.setCompParams(strip.id, { ...strip.comp, enabled: on })}
    />
    <div class="panelbtns">
      <button
        class="pbtn"
        class:active={strip.eq.enabled}
        aria-pressed={strip.eq.enabled}
        title="Open equalizer"
        onclick={() => (ui.eqStrip = strip.id)}
      >
        EQ
      </button>
      <button
        class="pbtn"
        class:active={strip.gate.enabled || strip.comp.enabled}
        aria-pressed={strip.gate.enabled || strip.comp.enabled}
        title="Open dynamics (gate + compressor)"
        onclick={() => (ui.dynStrip = strip.id)}
      >
        DYN
      </button>
    </div>
  </div>

  <ColorPad {strip} />

  <div class="sep"></div>
  <RoutingToggleGroup routes={strip.routes} ontoggle={(bus, on) => mixer.setRoute(strip.id, bus, on)} />

  <MuteSoloButtons
    mute={strip.mute}
    onmute={(on) => mixer.setMute({ strip: strip.id }, on)}
    solo={strip.solo}
    onsolo={(on) => mixer.setSolo(strip.id, on)}
  />
</div>

<style>
  .strip {
    width: var(--strip-w);
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
    align-items: flex-start;
    gap: 8px;
  }
  .panelbtns {
    display: flex;
    flex-direction: column;
    gap: 3px;
    margin-top: 1px;
  }
  .pbtn {
    height: 19px;
    width: 30px;
    font: 500 9px var(--mono);
    letter-spacing: 0.05em;
    background: var(--bg-3);
    color: var(--text-3);
    border: 1px solid var(--border-0);
    border-radius: 2px;
    box-shadow: inset 0 1px 0 var(--hl-top);
  }
  .pbtn:hover {
    background: var(--bg-4);
    color: var(--text-2);
  }
  .pbtn.active {
    background: var(--accent-glow);
    color: var(--accent);
    border-color: var(--accent);
  }
</style>
