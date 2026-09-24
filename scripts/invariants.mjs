#!/usr/bin/env node
// The architecture invariants checklist (plan §160, ARCHITECTURE §8, PLAN-31): runs the tests
// that enforce each of the twelve invariants and prints one line per invariant. It fails when
// any test fails; a test that skipped (a speech model or the worker isn't on this PC) shows as
// "not run" and fails the checklist too, since nothing was proven.
//
//   pnpm invariants            (set KIVO_*_DIR for the speech-model tests, as in CLAUDE.md)
import { spawnSync } from "node:child_process";

/** [invariant, [[cargo test args...], ...]] — each test named exactly. */
const CHECKS = [
  ["1. The runtime stays provider-independent", [["-p", "kivo-core", "--test", "no_provider_dependencies", "kivo_core_has_no_provider_or_platform_dependency"]]],
  ["2. The UI is not the core runtime", [["-p", "kivo-core", "--test", "no_provider_dependencies", "the_app_reaches_kivo_only_through_ipc"]]],
  [
    "3. Deterministic actions never need heavy AI",
    [
      ["-p", "kivo-intent", "--lib", "the_fast_path_ratio_and_grammar_latency_are_tracked"],
      ["-p", "kivo-runtime", "--test", "spoken_command", "saying_mute_mutes_with_no_ai_and_a_brief_answer"],
    ],
  ],
  [
    "4. AI output can't bypass the permission engine",
    [
      ["-p", "kivo-security", "--lib", "hard_limits_hold_in_every_mode_including_bypass"],
      ["-p", "kivo-runtime", "--test", "brain_turns", "the_brains_tool_calls_go_through_the_permission_engine"],
      ["-p", "kivo-runtime", "--test", "security_suite"],
    ],
  ],
  [
    "5. Native control before vision and raw input",
    [
      ["-p", "kivo-tools", "--lib", "uia_is_used_first_for_a_desktop_app_and_says_so"],
      ["-p", "kivo-tools", "--lib", "custom_drawn_apps_fall_to_vision_and_input_only_when_allowed"],
    ],
  ],
  ["6. MCP isn't needed for internal actions", [["-p", "kivo-runtime", "--test", "spoken_command", "a_typed_command_is_treated_like_a_spoken_one"]]],
  [
    "7. Voice supports interruption",
    [
      ["-p", "kivo-core", "--lib", "barge_in_starts_a_new_turn"],
      ["-p", "kivo-runtime", "--test", "spoken_command", "talking_over_kivo_interrupts_it_and_is_heard_as_a_new_request"],
    ],
  ],
  [
    "8. Long-running work is cancellable",
    [
      ["-p", "kivo-runtime", "--test", "spoken_command", "cancelling_stops_every_layer_within_100_ms"],
      ["-p", "kivo-runtime", "--test", "task_turns", "timeouts_pauses_and_checks"],
    ],
  ],
  [
    "9. Provider failures are isolated",
    [
      ["-p", "kivo-runtime", "--test", "brain_turns", "a_failing_provider_is_reported_plainly_and_kivo_carries_on"],
      ["-p", "kivo-brain", "--lib", "an_agent_that_dies_mid_prompt_is_an_error_not_a_crash"],
      ["-p", "kivo-runtime", "--test", "spoken_command", "a_crashed_speech_worker_fails_the_turn_aloud_and_comes_back"],
    ],
  ],
  [
    "10. Secrets stay out of model context",
    [
      ["-p", "kivo-core", "--lib", "debug_never_shows_the_value"],
      ["-p", "kivo-core", "--doc", "Secret"],
      ["-p", "kivo-store", "--lib", "masks_known_secret_formats_anywhere"],
      ["-p", "kivo-runtime", "--lib", "a_bundle_has_the_facts_and_no_secrets_or_content"],
    ],
  ],
  [
    "11. The user can disable cloud processing",
    [
      ["-p", "kivo-runtime", "--lib", "cloud_brains_are_skipped_when_cloud_processing_is_off"],
      ["-p", "kivo-voice", "--lib", "only_cloud_engines_send_data_off_the_device"],
    ],
  ],
  [
    "12. Useful offline mode",
    [
      ["-p", "kivo-brain", "--lib", "sensitive_and_offline_requests_stay_on_this_pc"],
      ["-p", "kivo-runtime", "--test", "m7_acceptance"],
    ],
  ],
];

function run(args) {
  const result = spawnSync("cargo", ["test", ...args, "--", "--nocapture"], {
    encoding: "utf8",
    maxBuffer: 256 * 1024 * 1024,
    shell: false,
  });
  const out = `${result.stdout}\n${result.stderr}`;
  const passed = [...out.matchAll(/test result: ok\. (\d+) passed/g)].reduce((n, m) => n + Number(m[1]), 0);
  const failed = result.status !== 0;
  const skipped = /skipping/i.test(out);
  if (failed) return "red";
  if (passed === 0 || skipped) return "not run";
  return "green";
}

let bad = 0;
for (const [invariant, tests] of CHECKS) {
  const results = tests.map((t) => ({ test: t[t.length - 1], result: run(t) }));
  const worst = results.find((r) => r.result === "red") ?? results.find((r) => r.result === "not run");
  const mark = worst ? (worst.result === "red" ? "✗" : "…") : "✓";
  if (worst) bad += 1;
  console.log(`${mark} ${invariant}`);
  for (const r of results) if (r.result !== "green") console.log(`    ${r.result}: ${r.test}`);
}
console.log(bad === 0 ? "\nAll twelve invariants hold." : `\n${bad} invariant(s) not proven.`);
process.exit(bad === 0 ? 0 : 1);
