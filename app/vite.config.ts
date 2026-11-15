import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

// Port 1620 with strictPort, matching `build.devUrl` in tauri.conf.json.
export default defineConfig({
  clearScreen: false,
  plugins: [react(), tailwindcss()],
  resolve: { tsconfigPaths: true },
  server: {
    port: 1620,
    strictPort: true,
    // src-tauri builds into the root target/, which Vite would watch.
    watch: { ignored: ["**/target/**", "**/src-tauri/**"] },
  },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: { target: "es2022" },
  test: { environment: "jsdom", setupFiles: ["src/test/setup.ts"] },
});
