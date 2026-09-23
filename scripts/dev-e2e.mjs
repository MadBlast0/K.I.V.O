// `pnpm dev:e2e`: the dev app (`pnpm dev`) with its WebView2 debugging port open on 127.0.0.1, so
// the Playwright UI tests can drive it (ARCH-37). Never used for a release build.
import { spawn } from "node:child_process";

const port = process.env.KIVO_CDP_PORT ?? "9223";
// Tauri's own WebView2 flags, plus the debugging port (the variable replaces Tauri's list).
const args = [
  "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection",
  `--remote-debugging-port=${port}`,
  "--remote-debugging-address=127.0.0.1",
].join(" ");
const child = spawn("pnpm", ["dev"], {
  stdio: "inherit",
  shell: true,
  env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: args },
});
child.on("exit", (code) => process.exit(code ?? 0));
