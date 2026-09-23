/**
 * Playwright UI tests against the Tauri dev build (ARCH-37, ARCHITECTURE §7). They drive the
 * running app's own WebView2 over the Chrome DevTools Protocol, so no browser is downloaded and
 * nothing else on the desktop is touched: start the app with `pnpm dev:e2e` (the dev app with a
 * debugging port on 127.0.0.1), then run `pnpm --filter kivo-app e2e`.
 */
import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: ".",
  testMatch: /.*\.e2e\.ts$/,
  timeout: 60_000,
  workers: 1,
  reporter: "list",
});
