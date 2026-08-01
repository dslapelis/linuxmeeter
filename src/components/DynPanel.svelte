<script lang="ts">
  import type { StripState } from "../lib/types";
  import { mixer } from "../lib/state/mixer.svelte";
  import { ui } from "../lib/state/ui.svelte";
  import Knob from "./Knob.svelte";

  interface Props {
    strip: StripState;
  }
  let { strip }: Props = $props();

  // ---- compressor transfer curve ----
  const CW = 150;
  const CH = 150;
  const DB_MIN = -60; // both axes: -60..0 dBFS

  let canvas: HTMLCanvasElement;
  let dragging = $state(false);

  const xFor = (db: number) => ((db - DB_MIN) / -DB_MIN) * CW;
  const yFor = (db: number) => CH - ((db - DB_MIN) / -DB_MIN) * CH;

  function transfer(inDb: number): number {
    const c = strip.comp;
    const out = inDb <= c.thresholdDb ? inDb : c.thresholdDb + (inDb - c.thresholdDb) / Math.max(1, c.ratio);
    return out + c.makeupDb;
  }

  function setComp(patch: Partial<StripState["comp"]>, enable = true) {
    mixer.setCompParams(strip.id, { ...strip.comp, ...patch, ...(enable ? { enabled: true } : {}) });
  }
  function setGate(patch: Partial<StripState["gate"]>, enable = true) {
    mixer.setGateParams(strip.id, { ...strip.gate, ...patch, ...(enable ? { enabled: true } : {}) });
  }

  $effect(() => {
    const c = strip.comp;
    void c; // track
    const ctx = canvas?.getContext("2d");
    if (!ctx) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = CW * dpr;
    canvas.height = CH * dpr;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    const css = getComputedStyle(document.documentElement);
    const col = (n: string) => css.getPropertyValue(n).trim();

    ctx.fillStyle = col("--bg-0");
    ctx.fillRect(0, 0, CW, CH);

    // grid every 12 dB + unity diagonal
    ctx.strokeStyle = "rgba(255,255,255,0.05)";
    ctx.lineWidth = 1;
    for (let db = -48; db < 0; db += 12) {
      const x = Math.round(xFor(db)) + 0.5;
      const y = Math.round(yFor(db)) + 0.5;
      ctx.beginPath();
      ctx.moveTo(x, 0);
      ctx.lineTo(x, CH);
      ctx.moveTo(0, y);
      ctx.lineTo(CW, y);
      ctx.stroke();
    }
    ctx.strokeStyle = "rgba(255,255,255,0.12)";
    ctx.setLineDash([3, 3]);
    ctx.beginPath();
    ctx.moveTo(xFor(DB_MIN), yFor(DB_MIN));
    ctx.lineTo(xFor(0), yFor(0));
    ctx.stroke();
    ctx.setLineDash([]);

    // transfer curve
    ctx.strokeStyle = c.enabled ? col("--accent") : col("--text-3");
    ctx.lineWidth = 2;
    ctx.beginPath();
    for (let px = 0; px <= CW; px += 2) {
      const inDb = DB_MIN + (px / CW) * -DB_MIN;
      const y = yFor(Math.max(DB_MIN, Math.min(0, transfer(inDb))));
      if (px === 0) ctx.moveTo(px, y);
      else ctx.lineTo(px, y);
    }
    ctx.stroke();

    // knee point (threshold, threshold+makeup)
    const kx = xFor(c.thresholdDb);
    const ky = yFor(Math.max(DB_MIN, Math.min(0, c.thresholdDb + c.makeupDb)));
    ctx.beginPath();
    ctx.arc(kx, ky, 5, 0, Math.PI * 2);
    ctx.fillStyle = col("--bg-2");
    ctx.fill();
    ctx.lineWidth = 2;
    ctx.strokeStyle = c.enabled ? col("--accent") : col("--text-2");
    ctx.stroke();
  });

  function canvasPos(e: PointerEvent): [number, number] {
    const r = canvas.getBoundingClientRect();
    return [((e.clientX - r.left) / r.width) * CW, ((e.clientY - r.top) / r.height) * CH];
  }
  function onpointerdown(e: PointerEvent) {
    if (e.button !== 0) return;
    dragging = true;
    canvas.setPointerCapture(e.pointerId);
    onpointermove(e);
  }
  function onpointermove(e: PointerEvent) {
    if (!dragging) return;
    const [mx, my] = canvasPos(e);
    // x -> threshold; y offset above the knee -> makeup
    const thresholdDb = Math.round(Math.max(-60, Math.min(0, DB_MIN + (mx / CW) * -DB_MIN)));
    const outDb = DB_MIN + ((CH - my) / CH) * -DB_MIN;
    const makeupDb = Math.round(Math.max(-12, Math.min(24, outDb - thresholdDb)) * 2) / 2;
    setComp({ thresholdDb, makeupDb });
  }
  function onpointerup() {
    dragging = false;
  }

  function close() {
    ui.dynStrip = null;
  }
  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape") close();
  }

  const fmtMs = (v: number) => (v < 10 ? `${v.toFixed(1)} ms` : `${Math.round(v)} ms`);
</script>

<svelte:window {onkeydown} />

<!-- scrim click closes; Esc handled on window -->
<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="scrim" onclick={(e) => e.target === e.currentTarget && close()}>
  <div class="panel" role="dialog" aria-label="Dynamics">
    <header>
      <div class="title">DYNAMICS <span class="sub">{strip.label}</span></div>
      <button class="closebtn" title="Close" onclick={close}>
        <svg width="11" height="11" viewBox="0 0 12 12"><path d="M2.5 2.5l7 7M9.5 2.5l-7 7" stroke="currentColor" stroke-width="1.25" /></svg>
      </button>
    </header>

    <section>
      <div class="sechead">
        <span class="secname">COMPRESSOR</span>
        <button
          class="enbtn"
          class:active={strip.comp.enabled}
          aria-pressed={strip.comp.enabled}
          onclick={() => mixer.setCompParams(strip.id, { ...strip.comp, enabled: !strip.comp.enabled })}
        >
          {strip.comp.enabled ? "ON" : "OFF"}
        </button>
      </div>
      <div class="comprow">
        <canvas
          bind:this={canvas}
          style:width="{CW}px"
          style:height="{CH}px"
          {onpointerdown}
          {onpointermove}
          {onpointerup}
          title="Drag: threshold (x) + makeup (y)"
        ></canvas>
        <div class="knobs">
          <Knob label="THRESH" min={-60} max={0} value={strip.comp.thresholdDb} defaultValue={-18} onchange={(v) => setComp({ thresholdDb: v })} />
          <Knob label="RATIO" min={1} max={20} taper="log" value={strip.comp.ratio} defaultValue={4} fmt={(v) => `${v.toFixed(1)}:1`} onchange={(v) => setComp({ ratio: Math.round(v * 10) / 10 })} />
          <Knob label="MAKEUP" min={-12} max={24} value={strip.comp.makeupDb} defaultValue={0} onchange={(v) => setComp({ makeupDb: Math.round(v * 2) / 2 })} />
          <Knob label="ATTACK" min={0.1} max={500} taper="log" value={strip.comp.attackMs} defaultValue={20} fmt={fmtMs} onchange={(v) => setComp({ attackMs: Math.round(v * 10) / 10 })} />
          <Knob label="RELEASE" min={10} max={2000} taper="log" value={strip.comp.releaseMs} defaultValue={100} fmt={fmtMs} onchange={(v) => setComp({ releaseMs: Math.round(v) })} />
        </div>
      </div>
    </section>

    <div class="sep"></div>

    <section>
      <div class="sechead">
        <span class="secname">GATE</span>
        <button
          class="enbtn"
          class:active={strip.gate.enabled}
          aria-pressed={strip.gate.enabled}
          onclick={() => mixer.setGateParams(strip.id, { ...strip.gate, enabled: !strip.gate.enabled })}
        >
          {strip.gate.enabled ? "ON" : "OFF"}
        </button>
      </div>
      <div class="knobs gaterow">
        <Knob label="THRESH" min={-70} max={0} value={strip.gate.thresholdDb} defaultValue={-40} onchange={(v) => setGate({ thresholdDb: v })} />
        <Knob label="ATTACK" min={0.1} max={500} taper="log" value={strip.gate.attackMs} defaultValue={20} fmt={fmtMs} onchange={(v) => setGate({ attackMs: Math.round(v * 10) / 10 })} />
        <Knob label="RELEASE" min={10} max={2000} taper="log" value={strip.gate.releaseMs} defaultValue={100} fmt={fmtMs} onchange={(v) => setGate({ releaseMs: Math.round(v) })} />
        <Knob label="HOLD" min={1} max={1000} taper="log" value={strip.gate.holdMs} defaultValue={50} fmt={fmtMs} onchange={(v) => setGate({ holdMs: Math.round(v) })} />
      </div>
    </section>
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 40;
    background: rgba(10, 11, 13, 0.7);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .panel {
    width: 420px;
    background: var(--bg-2);
    border: 1px solid var(--border-1);
    border-radius: 6px;
    box-shadow: inset 0 1px 0 var(--hl-top), 0 8px 24px rgba(0, 0, 0, 0.6);
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .title {
    flex: 1;
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.08em;
  }
  .title .sub {
    color: var(--text-2);
    font-weight: 500;
    margin-left: 6px;
  }
  .closebtn {
    width: 24px;
    height: 22px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-2);
    border-radius: 2px;
  }
  .closebtn:hover {
    background: var(--mute);
    color: #fff;
  }
  .sep {
    height: 1px;
    background: var(--border-0);
    margin: 0 -12px;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .sechead {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .secname {
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.08em;
    color: var(--text-2);
  }
  .enbtn {
    height: 20px;
    padding: 0 10px;
    font: 500 10px var(--mono);
    letter-spacing: 0.06em;
    background: var(--bg-3);
    color: var(--text-3);
    border: 1px solid var(--border-0);
    border-radius: 2px;
    box-shadow: inset 0 1px 0 var(--hl-top);
  }
  .enbtn.active {
    background: var(--accent-glow);
    color: var(--accent);
    border-color: var(--accent);
  }
  canvas {
    display: block;
    border: 1px solid var(--border-0);
    border-radius: 2px;
    cursor: crosshair;
    touch-action: none;
    flex: none;
  }
  .comprow {
    display: flex;
    gap: 14px;
    align-items: flex-start;
  }
  .knobs {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 10px 6px;
    flex: 1;
  }
  .gaterow {
    grid-template-columns: repeat(4, 1fr);
  }
</style>
