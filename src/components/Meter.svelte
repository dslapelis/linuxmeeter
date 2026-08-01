<script lang="ts">
  import { onMount } from "svelte";
  import type { MeterKey } from "../lib/types";
  import { clearAllClips, clearClip, getMeter, registerRenderer } from "../lib/state/meters";

  interface Props {
    key: MeterKey;
    segmented?: boolean;
  }
  let { key, segmented = false }: Props = $props();

  let canvas: HTMLCanvasElement;

  const CLIP_H = 4;
  const GAP = 2;
  const W = 10;

  let colors = {
    trough: "#0A0B0D",
    green: "#2BD96A",
    amber: "#F5B800",
    red: "#FF453A",
    hold: "rgba(255,255,255,0.85)",
    clip: "#FF2E43",
  };

  function readColors() {
    const css = getComputedStyle(document.documentElement);
    colors = {
      trough: css.getPropertyValue("--bg-0").trim(),
      green: css.getPropertyValue("--meter-green").trim(),
      amber: css.getPropertyValue("--meter-amber").trim(),
      red: css.getPropertyValue("--meter-red").trim(),
      hold: css.getPropertyValue("--meter-peak").trim(),
      clip: css.getPropertyValue("--clip").trim(),
    };
  }

  function zoneColor(db: number): string {
    return db >= -6 ? colors.red : db >= -18 ? colors.amber : colors.green;
  }

  onMount(() => {
    readColors();
    const dpr = window.devicePixelRatio || 1;
    const ctx = canvas.getContext("2d", { alpha: false })!;
    const state = getMeter(key);

    // Backing store follows the rendered size (height is flexible).
    const resize = () => {
      canvas.width = W * dpr;
      canvas.height = Math.max(1, Math.round(canvas.clientHeight * dpr));
    };
    resize();
    const ro = new ResizeObserver(resize);
    ro.observe(canvas);

    let visible = true;
    const io = new IntersectionObserver((entries) => {
      visible = entries[0]?.isIntersecting ?? true;
    });
    io.observe(canvas);

    const draw = () => {
      if (!visible) return;
      const H = canvas.height / dpr;
      const barTop = CLIP_H + GAP;
      const barH = H - barTop;
      const yFor = (db: number) => barTop + (1 - Math.max(0, Math.min(1, (db + 60) / 60))) * barH;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.fillStyle = colors.trough;
      ctx.fillRect(0, 0, W, H);
      ctx.fillStyle = "rgba(255,255,255,0.06)";
      for (const t of [-6, -18, -40]) ctx.fillRect(0, Math.round(yFor(t)), W, 1);

      for (let ch = 0; ch < 2; ch++) {
        const x = ch * 6;
        const rms = state.rms[ch]!;
        const peak = state.peak[ch]!;
        const hold = state.hold[ch]!;
        if (segmented) {
          for (let y = H - 4; y >= barTop; y -= 4) {
            const segDb = -60 + (1 - (y + 3 - barTop) / barH) * 60;
            ctx.fillStyle = rms >= segDb ? zoneColor(segDb) : "rgba(255,255,255,0.03)";
            ctx.fillRect(x, y, 4, 3);
          }
        } else {
          const zones: Array<[number, number, string]> = [
            [-60, -18, colors.green],
            [-18, -6, colors.amber],
            [-6, 0, colors.red],
          ];
          ctx.globalAlpha = 0.9;
          for (const [lo, hi, col] of zones) {
            const top = Math.min(hi, rms);
            if (top > lo) {
              ctx.fillStyle = col;
              ctx.fillRect(x, yFor(top), 4, yFor(lo) - yFor(top));
            }
          }
          ctx.globalAlpha = 1;
          if (peak > -60) {
            ctx.fillStyle = zoneColor(peak);
            ctx.fillRect(x, Math.round(yFor(peak)) - 1, 4, 2);
          }
        }
        if (hold > -59) {
          ctx.fillStyle = colors.hold;
          ctx.fillRect(x, Math.round(yFor(hold)), 4, 1);
        }
      }

      ctx.fillStyle = state.clip ? colors.clip : "rgba(255,255,255,0.05)";
      ctx.fillRect(0, 0, W, CLIP_H);
    };

    const unregister = registerRenderer(key, draw);
    return () => {
      unregister();
      io.disconnect();
      ro.disconnect();
    };
  });

  function onclick(e: MouseEvent) {
    if (e.altKey) clearAllClips();
    else clearClip(key);
  }
</script>

<canvas bind:this={canvas} {onclick} title="Click to clear clip (Alt: clear all)"></canvas>

<style>
  canvas {
    width: 10px;
    height: 100%;
    min-height: var(--fader-h);
    display: block;
  }
</style>
