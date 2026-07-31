<script lang="ts">
  /** Invisible resize regions for the decorationless window (Wayland gives no
   *  server-side resize borders with CSD). Rendered only inside Tauri. */
  import { startResizeDragging } from "../lib/window";

  const edges = [
    { dir: "North", style: "top:0;left:4px;right:4px;height:4px;cursor:n-resize" },
    { dir: "South", style: "bottom:0;left:4px;right:4px;height:4px;cursor:s-resize" },
    { dir: "West", style: "left:0;top:4px;bottom:4px;width:4px;cursor:w-resize" },
    { dir: "East", style: "right:0;top:4px;bottom:4px;width:4px;cursor:e-resize" },
    { dir: "NorthWest", style: "top:0;left:0;width:8px;height:8px;cursor:nw-resize" },
    { dir: "NorthEast", style: "top:0;right:0;width:8px;height:8px;cursor:ne-resize" },
    { dir: "SouthWest", style: "bottom:0;left:0;width:8px;height:8px;cursor:sw-resize" },
    { dir: "SouthEast", style: "bottom:0;right:0;width:8px;height:8px;cursor:se-resize" },
  ] as const;
</script>

{#each edges as edge}
  <!-- svelte-ignore a11y_no_static_element_interactions -- invisible resize region -->
  <div
    class="edge"
    style={edge.style}
    onpointerdown={(e) => {
      if (e.button === 0) void startResizeDragging(edge.dir);
    }}
  ></div>
{/each}

<style>
  .edge {
    position: fixed;
    z-index: 100;
  }
</style>
