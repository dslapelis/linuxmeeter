/** The universal continuous-control grammar (Svelte action):
 * vertical drag / Shift = 10× fine / Esc = cancel / double-click = reset /
 * wheel steps (Shift = fine). Values are normalized 0..1; components map to
 * their own ranges.
 */
export interface VDragOptions {
  get(): number;
  set(v: number): void;
  /** Pixels of travel covering the full 0..1 range. */
  pixels: number;
  /** Normalized value applied on double-click. */
  reset(): number;
  /** Normalized step per wheel notch. */
  wheelStep: number;
  /** Optional: map a pointerdown to a jump-to position (fader track click). */
  jump?(e: PointerEvent, el: HTMLElement): number;
  onStart?(): void;
  onEnd?(): void;
}

const clamp01 = (v: number) => Math.max(0, Math.min(1, v));

export function vdrag(el: HTMLElement, opts: VDragOptions): { destroy(): void } {
  let dragging = false;
  let startVal = 0;
  let val = 0;
  let lastY = 0;

  const down = (e: PointerEvent) => {
    if (e.button !== 0) return;
    e.preventDefault();
    el.setPointerCapture(e.pointerId);
    dragging = true;
    startVal = val = opts.get();
    lastY = e.clientY;
    if (opts.jump) {
      val = clamp01(opts.jump(e, el));
      opts.set(val);
    }
    el.classList.add("dragging");
    opts.onStart?.();
  };

  const move = (e: PointerEvent) => {
    if (!dragging) return;
    const fine = e.shiftKey ? 0.1 : 1;
    val = clamp01(val + ((lastY - e.clientY) / opts.pixels) * fine);
    lastY = e.clientY;
    opts.set(val);
  };

  const up = () => {
    if (!dragging) return;
    dragging = false;
    el.classList.remove("dragging");
    opts.onEnd?.();
  };

  const key = (e: KeyboardEvent) => {
    if (dragging && e.key === "Escape") {
      dragging = false;
      el.classList.remove("dragging");
      opts.set(startVal);
      opts.onEnd?.();
    }
  };

  const dbl = () => opts.set(clamp01(opts.reset()));

  const wheel = (e: WheelEvent) => {
    e.preventDefault();
    const dir = e.deltaY < 0 ? 1 : -1;
    const step = e.shiftKey ? opts.wheelStep / 5 : opts.wheelStep;
    opts.set(clamp01(opts.get() + dir * step));
  };

  el.addEventListener("pointerdown", down);
  el.addEventListener("pointermove", move);
  el.addEventListener("pointerup", up);
  el.addEventListener("pointercancel", up);
  el.addEventListener("dblclick", dbl);
  el.addEventListener("wheel", wheel, { passive: false });
  window.addEventListener("keydown", key);

  return {
    destroy() {
      el.removeEventListener("pointerdown", down);
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
      el.removeEventListener("pointercancel", up);
      el.removeEventListener("dblclick", dbl);
      el.removeEventListener("wheel", wheel);
      window.removeEventListener("keydown", key);
    },
  };
}
