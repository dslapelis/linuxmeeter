/** Window chrome helpers — no-ops outside Tauri so the browser mock works. */
import { IS_TAURI } from "./ipc";

async function win() {
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  return getCurrentWindow();
}

/** Compositor-native move — correct on both Wayland (xdg-toplevel) and X11. */
export async function startDragging(): Promise<void> {
  if (IS_TAURI) await (await win()).startDragging();
}

export async function minimize(): Promise<void> {
  if (IS_TAURI) await (await win()).minimize();
}

export async function close(): Promise<void> {
  if (IS_TAURI) await (await win()).close();
}

/** The window is non-resizable; the app sizes it to fit the strip content. */
export async function setContentSize(width: number, height: number): Promise<void> {
  if (!IS_TAURI) return;
  const { LogicalSize } = await import("@tauri-apps/api/dpi");
  await (await win()).setSize(new LogicalSize(Math.round(width), Math.round(height)));
}
