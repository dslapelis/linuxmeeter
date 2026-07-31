/** Window chrome helpers — no-ops outside Tauri so the browser mock works. */
import { IS_TAURI } from "./ipc";

type ResizeDirection =
  | "North"
  | "South"
  | "East"
  | "West"
  | "NorthEast"
  | "NorthWest"
  | "SouthEast"
  | "SouthWest";

async function win() {
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  return getCurrentWindow();
}

/** Compositor-native move — correct on both Wayland (xdg-toplevel) and X11. */
export async function startDragging(): Promise<void> {
  if (IS_TAURI) await (await win()).startDragging();
}

export async function startResizeDragging(direction: ResizeDirection): Promise<void> {
  if (IS_TAURI) await (await win()).startResizeDragging(direction);
}

export async function minimize(): Promise<void> {
  if (IS_TAURI) await (await win()).minimize();
}

export async function toggleMaximize(): Promise<void> {
  if (IS_TAURI) await (await win()).toggleMaximize();
}

export async function close(): Promise<void> {
  if (IS_TAURI) await (await win()).close();
}
