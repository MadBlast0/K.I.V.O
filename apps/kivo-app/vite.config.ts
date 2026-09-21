/// <reference types="vitest/config" />
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

const host = process.env.TAURI_DEV_HOST;

// Tauri expects a fixed port and fails if it is taken.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  test: {
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
  },
  build: {
    target: "es2022",
    rolldownOptions: {
      // Two windows: the Control Center and the transparent Island overlay.
      input: { main: "index.html", overlay: "overlay.html" },
      output: {
        // Vendor code changes rarely; separate chunks keep rebuilds and cache hits cheap.
        codeSplitting: {
          groups: [
            // [\\/] matches either path separator, so the groups work with Windows paths too.
            { name: "react", test: /node_modules[\\/](react|react-dom|scheduler)[\\/]/ },
            { name: "base-ui", test: /node_modules[\\/]@base-ui[\\/]/ },
            { name: "motion", test: /node_modules[\\/](motion|motion-dom|motion-utils|framer-motion)[\\/]/ },
            { name: "icons", test: /node_modules[\\/]lucide-react[\\/]/ },
          ],
        },
      },
    },
  },
});
