#!/usr/bin/env node
// The wake glow's power (UX-16, BENCHMARKS §1 "overlay"): with the dev app running for Playwright
// (`pnpm dev:e2e`), it samples the CPU package power (RAPL) and the GPU's 3D load for 20 s at rest,
// then for 20 s while the glow plays once a second through Settings → Island → Wake glow → Preview
// — far more often than it would on wakes. It presses KIVO's own button in its page; nothing is
// typed or clicked on the desktop.
//
//   node scripts/bench-glow.mjs [seconds]

import { spawnSync } from "node:child_process";
// Playwright is the app package's dev dependency.
const { chromium } = await import(
  new URL("../apps/kivo-app/node_modules/@playwright/test/index.mjs", import.meta.url).href
);

const seconds = Number(process.argv[2] ?? 20);
const COUNTERS = ["\\Energy Meter(*_pkg)\\Power", "\\GPU Engine(*engtype_3D)\\Utilization Percentage"];

/** Averages of the package power (W) and the summed GPU 3D load (%) over `seconds`. */
function sample() {
  const script = `
    $s = Get-Counter -Counter ${COUNTERS.map((c) => `'${c}'`).join(",")} -SampleInterval 1 -MaxSamples ${seconds};
    $power = @(); $gpu = @();
    foreach ($x in $s) {
      $p = ($x.CounterSamples | Where-Object { $_.Path -like '*energy meter*' } | Measure-Object CookedValue -Sum).Sum;
      $g = ($x.CounterSamples | Where-Object { $_.Path -like '*gpu engine*' } | Measure-Object CookedValue -Sum).Sum;
      $power += $p; $gpu += $g
    }
    "{0} {1}" -f ($power | Measure-Object -Average).Average, ($gpu | Measure-Object -Average).Average`;
  const out = spawnSync("powershell", ["-NoProfile", "-Command", script], { encoding: "utf8" });
  const [power, gpu] = out.stdout.trim().split(/\s+/).map(Number);
  // RAPL reports milliwatts.
  return { watts: power / 1000, gpu };
}

const browser = await chromium.connectOverCDP(process.env.KIVO_CDP ?? "http://127.0.0.1:9223");
const page = browser
  .contexts()
  .flatMap((c) => c.pages())
  .find((p) => new URL(p.url()).pathname === "/");
if (!page) throw new Error("the Control Center isn't open: start KIVO with `pnpm dev:e2e`");
const click = (locator) => locator.evaluate((e) => e.click());
await click(page.getByRole("navigation", { name: "Main" }).getByRole("button", { name: "Settings", exact: true }));
await click(page.getByRole("tab", { name: "Island", exact: true }));
const preview = page.getByRole("button", { name: "Preview", exact: true });
await preview.waitFor({ state: "attached" });

const rest = sample();
let playing = true;
const loop = (async () => {
  while (playing) {
    await click(preview);
    await new Promise((r) => setTimeout(r, 1000));
  }
})();
const glowing = sample();
playing = false;
await loop;
await browser.close();

const round = (v) => Math.round(v * 100) / 100;
console.log(`at rest:           ${round(rest.watts)} W package, GPU 3D ${round(rest.gpu)} %`);
console.log(`glow every second: ${round(glowing.watts)} W package, GPU 3D ${round(glowing.gpu)} %`);
console.log(`added:             ${round(glowing.watts - rest.watts)} W, ${round(glowing.gpu - rest.gpu)} % GPU`);
