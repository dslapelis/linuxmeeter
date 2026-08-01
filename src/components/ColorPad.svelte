<script lang="ts">
  /** VoiceMeeter-style "voice color" XY pad. Drives its OWN four EQ bands
   *  (indices 4..7, voice-tuned corners at 300 Hz / 400 Hz / 3 kHz / 3.5 kHz)
   *  so it stays strong regardless of how the EQ-panel bands are set:
   *  X = dark ↔ bright (±10 dB tilt across the voice range)
   *  Y = warm ↔ present (+10 dB at 3 kHz, with an inverse 400 Hz mud cut)
   *  Center = neutral. Persists as ordinary EQ params.
   */
  import type { StripState } from "../lib/types";
  import { mixer } from "../lib/state/mixer.svelte";

  interface Props {
    strip: StripState;
  }
  let { strip }: Props = $props();

  const TILT_DB = 10;
  const PRESENCE_DB = 10;
  const MUD_DB = 5;
  const B = { lowTilt: 4, mud: 5, presence: 6, highTilt: 7 } as const;

  let pad: HTMLDivElement;
  let dragging = $state(false);

  let x = $derived.by(() => {
    const low = strip.eq.bands[B.lowTilt]?.gainDb ?? 0;
    const high = strip.eq.bands[B.highTilt]?.gainDb ?? 0;
    return Math.max(-1, Math.min(1, (high - low) / (2 * TILT_DB)));
  });
  let y = $derived.by(() => {
    const presence = strip.eq.bands[B.presence]?.gainDb ?? 0;
    return Math.max(-1, Math.min(1, presence / PRESENCE_DB));
  });
  let neutral = $derived(Math.abs(x) < 0.02 && Math.abs(y) < 0.02);

  function setPad(nx: number, ny: number) {
    nx = Math.max(-1, Math.min(1, nx));
    ny = Math.max(-1, Math.min(1, ny));
    const bands = strip.eq.bands.map((b, i) => {
      if (i === B.lowTilt) return { ...b, gainDb: Math.round(-nx * TILT_DB * 10) / 10 };
      if (i === B.highTilt) return { ...b, gainDb: Math.round(nx * TILT_DB * 10) / 10 };
      if (i === B.presence) return { ...b, gainDb: Math.round(ny * PRESENCE_DB * 10) / 10 };
      if (i === B.mud) return { ...b, gainDb: Math.round(-ny * MUD_DB * 10) / 10 };
      return b;
    });
    mixer.setEqParams(strip.id, { ...strip.eq, bands, enabled: true });
  }

  function posFromEvent(e: PointerEvent): [number, number] {
    const r = pad.getBoundingClientRect();
    return [((e.clientX - r.left) / r.width) * 2 - 1, -(((e.clientY - r.top) / r.height) * 2 - 1)];
  }
  function onpointerdown(e: PointerEvent) {
    if (e.button !== 0) return;
    dragging = true;
    pad.setPointerCapture(e.pointerId);
    const [nx, ny] = posFromEvent(e);
    setPad(nx, ny);
  }
  function onpointermove(e: PointerEvent) {
    if (!dragging) return;
    const [nx, ny] = posFromEvent(e);
    setPad(nx, ny);
  }
  function onpointerup() {
    dragging = false;
  }
  function ondblclick() {
    setPad(0, 0);
  }
</script>

<div class="wrap">
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="pad"
    bind:this={pad}
    {onpointerdown}
    {onpointermove}
    {onpointerup}
    {ondblclick}
    title="Voice color — x: dark/bright · y: soft/present · double-click: neutral"
  >
    <div class="axis h"></div>
    <div class="axis v"></div>
    <div class="dot" class:hot={!neutral} style:left="{((x + 1) / 2) * 100}%" style:top="{((1 - y) / 2) * 100}%"></div>
  </div>
  <div class="lbl">COLOR</div>
</div>

<style>
  .wrap {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 3px;
  }
  .pad {
    position: relative;
    width: 100%;
    height: 56px;
    background: var(--bg-0);
    border: 1px solid var(--border-0);
    border-radius: 2px;
    box-shadow: inset 0 1px 2px rgba(0, 0, 0, 0.6);
    cursor: crosshair;
    touch-action: none;
    overflow: hidden;
  }
  .axis {
    position: absolute;
    background: rgba(255, 255, 255, 0.07);
  }
  .axis.h {
    left: 0;
    right: 0;
    top: 50%;
    height: 1px;
  }
  .axis.v {
    top: 0;
    bottom: 0;
    left: 50%;
    width: 1px;
  }
  .dot {
    position: absolute;
    width: 9px;
    height: 9px;
    border-radius: 50%;
    transform: translate(-50%, -50%);
    background: var(--bg-3);
    border: 2px solid var(--text-3);
    box-sizing: border-box;
  }
  .dot.hot {
    border-color: var(--accent);
    box-shadow: 0 0 6px var(--accent-glow);
  }
  .pad:hover .dot {
    border-color: var(--accent-hi);
  }
  .lbl {
    font-size: 10px;
    letter-spacing: 0.04em;
    color: var(--text-2);
  }
</style>
