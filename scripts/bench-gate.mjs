#!/usr/bin/env node
// The release regression gate (BENCHMARKS §4, BENCH-14): compares a release's full benchmark run
// with the previous release's on the same reference machine. Any lower-is-better metric whose p50
// got more than 10% worse blocks the release (plan §159). Every such metric is gated, which
// covers the budgeted ones; changes smaller than the noise floor (1 ms, 1 MB, 0.5 percentage
// points) don't count.
//
//   node scripts/bench-gate.mjs bench-results/<previous>.json bench-results/<this>.json
import { readFileSync } from "node:fs";
import { basename } from "node:path";

export const LIMIT = 0.1;
const FLOOR = { ms: 1, µs: 1000, mb: 1, "%": 0.5 };

/** Metrics by "suite / name" with their p50 and unit. */
function metrics(run) {
  const out = new Map();
  for (const suite of run.suites ?? []) {
    for (const m of suite.metrics ?? []) {
      if (m.lowerIsBetter && typeof m.p50 === "number") out.set(`${suite.suite} / ${m.name}`, m);
    }
  }
  return out;
}

/** The regressions that block, and what couldn't be compared. */
export function gate(before, after) {
  if (before.machine?.id !== after.machine?.id) {
    throw new Error(`different machines: ${before.machine?.id} vs ${after.machine?.id}`);
  }
  const old = metrics(before);
  const now = metrics(after);
  const regressions = [];
  const missing = [];
  for (const [key, m] of old) {
    const n = now.get(key);
    if (!n) {
      missing.push(key);
      continue;
    }
    const floor = FLOOR[String(m.unit).toLowerCase()] ?? 0;
    const worse = n.p50 - m.p50;
    if (worse > floor && worse > m.p50 * LIMIT) {
      regressions.push({ metric: key, unit: m.unit, before: m.p50, after: n.p50, change: m.p50 === 0 ? Infinity : worse / m.p50 });
    }
  }
  return { regressions, missing };
}

if (process.argv[1] && basename(process.argv[1]) === "bench-gate.mjs") {
  const [a, b] = process.argv.slice(2);
  if (!a || !b) {
    console.error("usage: bench-gate.mjs <previous release.json> <this release.json>");
    process.exit(2);
  }
  const { regressions, missing } = gate(JSON.parse(readFileSync(a, "utf8")), JSON.parse(readFileSync(b, "utf8")));
  for (const k of missing) console.warn(`not measured this time: ${k}`);
  for (const r of regressions) {
    const pct = Number.isFinite(r.change) ? `+${(r.change * 100).toFixed(1)}%` : "new cost";
    console.error(`REGRESSION ${r.metric}: ${r.before.toFixed(2)} → ${r.after.toFixed(2)} ${r.unit} (${pct})`);
  }
  if (regressions.length > 0 || missing.length > 0) {
    console.error(`\nThe release is blocked: ${regressions.length} regression(s), ${missing.length} metric(s) missing.`);
    process.exit(1);
  }
  console.log("No regression over 10%.");
}
