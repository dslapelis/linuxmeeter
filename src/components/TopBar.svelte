<script lang="ts">
  import { onMount } from "svelte";
  import { ipc } from "../lib/ipc";
  import { mixer } from "../lib/state/mixer.svelte";
  import { startDragging, toggleMaximize } from "../lib/window";
  import WindowControls from "./WindowControls.svelte";

  let autostart = $state(false);
  onMount(async () => {
    autostart = await ipc.getAutostart();
  });
  function toggleAutostart() {
    autostart = !autostart;
    void ipc.setAutostart(autostart);
  }

  /** Drag only from non-interactive surface (buttons stop propagation naturally
   *  because we check the event target). */
  function onpointerdown(e: PointerEvent) {
    if (e.button !== 0) return;
    const el = e.target as HTMLElement;
    if (el.closest("button, input, [data-no-drag]")) return;
    void startDragging();
  }
  function ondblclick(e: MouseEvent) {
    const el = e.target as HTMLElement;
    if (el.closest("button, input, [data-no-drag]")) return;
    void toggleMaximize();
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -- window-drag surface, not a semantic control -->
<header class="topbar" {onpointerdown} {ondblclick}>
  <div class="wordmark">LINUXMEETER</div>
  <div class="center">
    <button class="chip" title="Profile">
      {mixer.profileName}
      <svg width="10" height="10" viewBox="0 0 10 10" fill="none">
        <path d="M2.5 4l2.5 2.5L7.5 4" stroke="currentColor" stroke-width="1.25" />
      </svg>
    </button>
    <button
      class="toggle"
      class:active={mixer.takeDefaultOutput}
      aria-pressed={mixer.takeDefaultOutput}
      title="Route all desktop audio through the System strip (makes linuxmeeter the default output device)"
      onclick={() => mixer.setDefaultOutput(!mixer.takeDefaultOutput)}
    >
      DEFAULT OUT
    </button>
    <button
      class="toggle"
      class:active={autostart}
      aria-pressed={autostart}
      title="Start linuxmeeter (minimized to tray) at login so virtual devices always exist"
      onclick={toggleAutostart}
    >
      AUTOSTART
    </button>
  </div>
  <button class="iconbtn" title="Settings">
    <svg width="14" height="14" viewBox="0 0 16 16" fill="none">
      <circle cx="8" cy="8" r="2.2" stroke="currentColor" stroke-width="1.25" />
      <path
        d="M8 1.5v2M8 12.5v2M1.5 8h2M12.5 8h2M3.4 3.4l1.4 1.4M11.2 11.2l1.4 1.4M12.6 3.4l-1.4 1.4M4.8 11.2l-1.4 1.4"
        stroke="currentColor"
        stroke-width="1.25"
      />
    </svg>
  </button>
  <WindowControls />
</header>

<style>
  .topbar {
    flex: none;
    height: var(--topbar-h);
    background: var(--bg-1);
    border-bottom: 1px solid var(--border-0);
    display: flex;
    align-items: center;
    padding-left: 12px;
    gap: 12px;
  }
  .wordmark {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.14em;
    color: var(--text-2);
  }
  .wordmark::before {
    content: "";
    width: 8px;
    height: 8px;
    background: var(--accent);
    border-radius: 1px;
  }
  .center {
    margin: 0 auto;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .chip {
    height: 24px;
    padding: 0 8px;
    display: flex;
    align-items: center;
    gap: 6px;
    background: var(--bg-3);
    border: 1px solid var(--border-0);
    border-radius: 2px;
    box-shadow: inset 0 1px 0 var(--hl-top);
    font-size: 12px;
    color: var(--text-1);
  }
  .chip:hover {
    background: var(--bg-4);
  }
  .toggle {
    height: 24px;
    padding: 0 8px;
    font: 500 10px var(--mono);
    letter-spacing: 0.06em;
    background: var(--bg-3);
    color: var(--text-3);
    border: 1px solid var(--border-0);
    border-radius: 2px;
    box-shadow: inset 0 1px 0 var(--hl-top);
  }
  .toggle:hover {
    background: var(--bg-4);
    color: var(--text-2);
  }
  .toggle.active {
    background: var(--accent-glow);
    color: var(--accent);
    border-color: var(--accent);
  }
  .chip svg {
    color: var(--text-3);
  }
  .iconbtn {
    width: 28px;
    height: 24px;
    border-radius: 2px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-2);
  }
  .iconbtn:hover {
    background: var(--bg-4);
    color: var(--text-1);
  }
</style>
