#!/usr/bin/env node
// Sets KIVO's version everywhere it is declared (RELEASE.md §1), then prints the commands that
// commit, tag and push the release. Tag pushes are what run CI and the release workflows.
//
//   pnpm release:version 0.1.0
//   pnpm release:version 0.2.0-beta.1

import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const version = process.argv[2];
const SEMVER = /^\d+\.\d+\.\d+(-(beta|exp)\.\d+)?$/;

if (!version || !SEMVER.test(version)) {
  console.error("Usage: pnpm release:version X.Y.Z   (or X.Y.Z-beta.N / X.Y.Z-exp.N)");
  process.exit(1);
}

/** Replaces exactly one match of `pattern` in `file`, or stops with an error. */
function replaceOnce(file, pattern, replacement) {
  const path = join(root, file);
  const text = readFileSync(path, "utf8");
  const matches = text.match(new RegExp(pattern.source, "gm"));
  if (matches?.length !== 1) {
    console.error(`${file}: expected one version line, found ${matches?.length ?? 0}`);
    process.exit(1);
  }
  writeFileSync(path, text.replace(pattern, replacement));
  console.log(`  ${file}`);
}

console.log(`Setting the version to ${version}:`);
replaceOnce("Cargo.toml", /^version = "[^"]*"$/m, `version = "${version}"`);
for (const file of ["package.json", "apps/kivo-app/package.json", "apps/kivo-app/src-tauri/tauri.conf.json"]) {
  replaceOnce(file, /^ {2}"version": "[^"]*",$/m, `  "version": "${version}",`);
}
// Refresh the workspace crates' entries in Cargo.lock (no other dependency changes).
execFileSync("cargo", ["update", "--workspace", "--quiet"], { cwd: root, stdio: "inherit" });
console.log("  Cargo.lock");

console.log(`
Next:
  git commit -am "chore: release v${version}"
  git tag v${version}
  git push origin main v${version}`);
