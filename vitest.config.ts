import { defineConfig } from "vitest/config";

// Frontend unit tests cover the pure math (fader taper, EQ response, meter
// ballistics), so they need neither a DOM nor the Svelte compiler.
export default defineConfig({
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
  },
});
