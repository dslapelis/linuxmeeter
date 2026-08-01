<script lang="ts">
  import { vdrag } from "../lib/utils/vdrag";

  interface Props {
    label: string;
    min: number;
    max: number;
    value: number;
    defaultValue: number;
    onchange: (v: number) => void;
    fmt?: (v: number) => string;
    /** When set, the processor can be bypassed: label click toggles, arc dims when off. */
    enabled?: boolean;
    onenabledchange?: (on: boolean) => void;
    /** "log" gives musically-even travel for time/ratio ranges (min must be > 0). */
    taper?: "linear" | "log";
  }
  let {
    label,
    min,
    max,
    value,
    defaultValue,
    onchange,
    fmt = (v) => `${v.toFixed(1)} dB`,
    enabled,
    onenabledchange,
    taper = "linear",
  }: Props = $props();

  let bypassed = $derived(enabled === false);

  function toT(v: number): number {
    return taper === "log" ? Math.log(v / min) / Math.log(max / min) : (v - min) / (max - min);
  }
  function fromT(t: number): number {
    return taper === "log" ? min * Math.pow(max / min, t) : min + (max - min) * t;
  }

  const A0 = -135;
  const SWEEP = 270;

  let hovering = $state(false);
  let t = $derived(Math.max(0, Math.min(1, toT(value))));
  let angle = $derived(A0 + SWEEP * t);

  function polar(r: number, deg: number): [number, number] {
    const a = ((deg - 90) * Math.PI) / 180;
    return [14 + r * Math.cos(a), 14 + r * Math.sin(a)];
  }
  function arcPath(r: number, a0: number, a1: number): string {
    const [x0, y0] = polar(r, a0);
    const [x1, y1] = polar(r, a1);
    return `M ${x0.toFixed(2)} ${y0.toFixed(2)} A ${r} ${r} 0 ${a1 - a0 > 180 ? 1 : 0} 1 ${x1.toFixed(2)} ${y1.toFixed(2)}`;
  }

  const trackPath = arcPath(12, A0, A0 + SWEEP);
  let valuePath = $derived(t > 0.004 ? arcPath(12, A0, angle) : "");
</script>

<div
  class="knob"
  role="slider"
  tabindex="0"
  aria-label={label}
  aria-valuemin={min}
  aria-valuemax={max}
  aria-valuenow={Math.round(value * 10) / 10}
  onpointerenter={() => (hovering = true)}
  onpointerleave={() => (hovering = false)}
  onkeydown={(e) => {
    const step = e.shiftKey ? 0.002 : 0.02;
    if (e.key === "ArrowUp") onchange(fromT(Math.min(1, toT(value) + step)));
    else if (e.key === "ArrowDown") onchange(fromT(Math.max(0, toT(value) - step)));
    else return;
    e.preventDefault();
  }}
  use:vdrag={{
    get: () => toT(value),
    set: (v) => onchange(fromT(v)),
    pixels: 150,
    reset: () => toT(defaultValue),
    wheelStep: 0.025,
  }}
>
  <svg width="28" height="28" viewBox="0 0 28 28">
    <path d={trackPath} stroke="var(--border-1)" stroke-width="2" fill="none" />
    {#if valuePath}
      <path d={valuePath} stroke={bypassed ? "var(--text-3)" : "var(--accent)"} stroke-width="2" fill="none" />
    {/if}
    <circle cx="14" cy="14" r="8.5" fill="var(--bg-3)" stroke="var(--border-0)" />
    <line x1="14" y1="11.5" x2="14" y2="7" stroke="var(--text-1)" stroke-width="2" stroke-linecap="round" transform="rotate({angle} 14 14)" />
  </svg>
  {#if onenabledchange}
    <button
      class="lbl lblbtn"
      class:val={hovering}
      class:on={enabled}
      title="{label}: click to {enabled ? 'bypass' : 'enable'}"
      onclick={(e) => {
        e.stopPropagation();
        onenabledchange?.(!enabled);
      }}
      onpointerdown={(e) => e.stopPropagation()}
      ondblclick={(e) => e.stopPropagation()}
    >
      {hovering ? fmt(value) : label}
    </button>
  {:else}
    <div class="lbl" class:val={hovering}>{hovering ? fmt(value) : label}</div>
  {/if}
</div>

<style>
  .knob {
    width: 34px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    cursor: ns-resize;
    touch-action: none;
  }
  .lbl {
    font-size: 10px;
    letter-spacing: 0.04em;
    color: var(--text-2);
    white-space: nowrap;
  }
  .lbl.val {
    font: 500 10px var(--mono);
    color: var(--text-1);
    letter-spacing: 0;
  }
  .lblbtn {
    cursor: pointer;
    border-radius: 2px;
    padding: 0 3px;
  }
  .lblbtn:hover {
    background: var(--bg-4);
    color: var(--text-1);
  }
  .lblbtn.on {
    color: var(--accent);
  }
  .lblbtn.on.val {
    color: var(--text-1);
  }
</style>
