// BENCH-14: the release regression gate.
//   pnpm test:scripts
import { test } from "node:test";
import assert from "node:assert/strict";
import { gate } from "./bench-gate.mjs";

const run = (metrics) => ({
  machine: { id: "ref-mid" },
  suites: [{ suite: "e2e", metrics }],
});
const m = (name, p50, unit = "ms", lowerIsBetter = true) => ({ name, p50, unit, lowerIsBetter });

test("a p50 more than 10% worse blocks; noise and improvements don't", () => {
  const before = run([m("wake → chime", 100), m("mute", 20), m("ipc ping", 0.3), m("idle RAM", 80, "MB"), m("fast-path share", 90, "%", false)]);
  const after = run([m("wake → chime", 112), m("mute", 20.9), m("ipc ping", 0.9), m("idle RAM", 70, "MB"), m("fast-path share", 50, "%", false)]);
  const { regressions, missing } = gate(before, after);
  assert.deepEqual(
    regressions.map((r) => r.metric),
    ["e2e / wake → chime"],
  );
  assert.equal(regressions[0].change.toFixed(2), "0.12");
  assert.deepEqual(missing, []);
});

test("a metric that wasn't measured again is reported, and machines must match", () => {
  const { missing } = gate(run([m("wake → chime", 100)]), run([]));
  assert.deepEqual(missing, ["e2e / wake → chime"]);
  assert.throws(() => gate(run([]), { machine: { id: "other" }, suites: [] }), /different machines/);
});
