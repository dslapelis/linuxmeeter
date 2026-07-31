<script lang="ts">
  import { vdrag } from "../lib/utils/vdrag";
  import { dbToPos, posToDb } from "../lib/utils/db";

  interface Props {
    value: number;
    onchange: (db: number) => void;
    onstart?: () => void;
    onend?: () => void;
  }
  let { value, onchange, onstart, onend }: Props = $props();

  const TRAVEL = 200;
  let handleTop = $derived((1 - dbToPos(value)) * TRAVEL - 7);

  function nudge(e: KeyboardEvent) {
    const step = e.shiftKey ? 0.1 : 0.5;
    let db = value === -Infinity ? -72 : value;
    switch (e.key) {
      case "ArrowUp":
        db += step;
        break;
      case "ArrowDown":
        db -= step;
        break;
      case "PageUp":
        db += 3;
        break;
      case "PageDown":
        db -= 3;
        break;
      case "Home":
        db = 0;
        break;
      case "End":
        db = -Infinity;
        break;
      default:
        return;
    }
    e.preventDefault();
    onchange(db === -Infinity ? db : Math.max(-72, Math.min(12, db)));
  }
</script>

<div
  class="fader"
  role="slider"
  tabindex="0"
  aria-label="Gain"
  aria-valuemin={-72}
  aria-valuemax={12}
  aria-valuenow={value === -Infinity ? -72 : Math.round(value * 10) / 10}
  onkeydown={nudge}
  use:vdrag={{
    get: () => dbToPos(value),
    set: (p) => onchange(posToDb(p)),
    pixels: TRAVEL,
    reset: () => dbToPos(0),
    wheelStep: 0.5 / 84,
    jump: (e, el) => 1 - (e.clientY - el.getBoundingClientRect().top) / TRAVEL,
    onStart: onstart,
    onEnd: onend,
  }}
>
  <div class="track"></div>
  <div class="handle" style:top="{handleTop}px"></div>
</div>

<style>
  .fader {
    position: relative;
    width: 34px;
    height: var(--fader-h);
    cursor: ns-resize;
    touch-action: none;
  }
  .track {
    position: absolute;
    left: 50%;
    transform: translateX(-50%);
    top: 0;
    bottom: 0;
    width: 4px;
    background: var(--bg-0);
    border-radius: 2px;
    box-shadow: inset 0 1px 2px rgba(0, 0, 0, 0.6);
  }
  .handle {
    position: absolute;
    left: 0;
    width: 34px;
    height: 14px;
    background: var(--bg-3);
    border: 1px solid var(--border-1);
    border-radius: 2px;
    box-shadow: inset 0 1px 0 var(--hl-top), 0 1px 2px rgba(0, 0, 0, 0.4);
  }
  .handle::after {
    content: "";
    position: absolute;
    left: 4px;
    right: 4px;
    top: 50%;
    height: 2px;
    transform: translateY(-50%);
    background: var(--text-1);
    border-radius: 1px;
  }
  .fader:hover .handle {
    background: var(--bg-4);
  }
  .fader:global(.dragging) .handle {
    border-color: var(--accent);
  }
</style>
