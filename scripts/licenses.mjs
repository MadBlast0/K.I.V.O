#!/usr/bin/env node
// Writes the libraries KIVO ships and their licences, for Settings → About (UX-31): every Rust
// crate the programs (kivo-runtime, kivo-app, kivo-infer and its CUDA build, kivo-infer-cuda) are
// built from, following normal and build dependencies only, and every npm package in the app's
// production dependencies.
// Model licences come from the model manifest at run time.
//
// It also writes THIRD_PARTY_NOTICES.txt (DIST-16) beside the Tauri config, for the installer and
// About: the licence and notice files each of those crates and packages carries, verbatim, and for
// a package that carries none, its licence id with one copy of that licence's standard text taken
// from another package that does. The file is generated for each build and not committed.
//
//   pnpm licenses:gen
import { execFileSync, execSync } from "node:child_process";
import { existsSync, readdirSync, readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const out = join(root, "apps", "kivo-app", "src", "generated", "licenses.json");
const SHIPPED = ["kivo-runtime", "kivo-app", "kivo-infer", "kivo-infer-cuda"];

const meta = JSON.parse(
  execFileSync("cargo", ["metadata", "--format-version", "1", "--locked"], {
    cwd: root,
    maxBuffer: 256 * 1024 * 1024,
    encoding: "utf8",
  }),
);
const byId = new Map(meta.packages.map((p) => [p.id, p]));
const nodes = new Map(meta.resolve.nodes.map((n) => [n.id, n]));
const workspace = new Set(meta.workspace_members);
const seen = new Set();
const stack = meta.packages.filter((p) => SHIPPED.includes(p.name) && workspace.has(p.id)).map((p) => p.id);
while (stack.length > 0) {
  const id = stack.pop();
  if (seen.has(id)) continue;
  seen.add(id);
  for (const dep of nodes.get(id)?.deps ?? []) {
    // Normal and build dependencies ship (build scripts can embed code); dev ones don't.
    if (dep.dep_kinds.some((k) => k.kind !== "dev")) stack.push(dep.pkg);
  }
}
const rustPackages = [...seen]
  .filter((id) => !workspace.has(id))
  .map((id) => byId.get(id))
  .toSorted((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version));
const rust = rustPackages.map((p) => ({
  name: p.name,
  version: p.version,
  license: p.license ?? p.license_file ?? "see crate",
}));

// A fixed command (pnpm is a .cmd shim on Windows, which needs the shell).
const npmRaw = JSON.parse(
  execSync("pnpm licenses list --json --prod", {
    cwd: join(root, "apps", "kivo-app"),
    maxBuffer: 64 * 1024 * 1024,
    encoding: "utf8",
  }),
);
const npmPackages = Object.values(npmRaw)
  .flat()
  .flatMap((p) => p.versions.map((v, i) => ({ name: p.name, version: v, license: p.license, dir: p.paths?.[i] })))
  .toSorted((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version));
const npm = npmPackages.map(({ name, version, license }) => ({ name, version, license }));

mkdirSync(dirname(out), { recursive: true });
writeFileSync(out, `${JSON.stringify({ rust, npm }, null, 2)}\n`);
console.log(`${rust.length} crates and ${npm.length} npm packages → ${out}`);

// THIRD_PARTY_NOTICES.txt (DIST-16).
const LICENSE_FILE = /^(licen[cs]e|copying|notice|copyright|unlicense)([-._].*)?$/i;
function licenseFiles(dir) {
  if (!dir || !existsSync(dir)) return [];
  return readdirSync(dir, { withFileTypes: true })
    .filter((e) => e.isFile() && LICENSE_FILE.test(e.name))
    .map((e) => ({ name: e.name, text: readFileSync(join(dir, e.name), "utf8").replace(/\r\n/g, "\n").trim() }))
    .toSorted((a, b) => a.name.localeCompare(b.name));
}
const entries = [
  ...rustPackages.map((p) => ({
    title: `${p.name} ${p.version} (crate)`,
    license: p.license ?? p.license_file ?? "",
    files: licenseFiles(dirname(p.manifest_path)),
  })),
  ...npmPackages.map((p) => ({ title: `${p.name} ${p.version} (npm)`, license: p.license ?? "", files: licenseFiles(p.dir) })),
];
/** A licence id's standard text, from the first package whose licence file is that licence. */
const standard = new Map();
for (const e of entries) {
  for (const f of e.files) {
    const id = /apache/i.test(f.name) ? "Apache-2.0" : /mit/i.test(f.name) ? "MIT" : e.license;
    if (id && !/\s/.test(id) && !standard.has(id)) standard.set(id, f.text);
  }
}
const lines = [
  "KIVO: third-party notices",
  "",
  "KIVO includes the following open-source software. Each entry gives the package, its licence and",
  "the licence and notice files it carries, as published. Speech and language models are separate",
  "downloads with their own licences, shown before each download and in Settings → About.",
  "",
];
const missing = new Set();
const printed = new Map();
for (const e of entries) {
  lines.push("=".repeat(78), e.title, `Licence: ${e.license || "not stated"}`, "=".repeat(78), "");
  if (e.files.length === 0) {
    lines.push("(No licence file in the package; the standard text of its licence is at the end.)", "");
    for (const id of e.license.split(/\s+(?:OR|AND|WITH)\s+|[()/]/)) {
      if (id.trim()) missing.add(id.trim());
    }
  }
  for (const f of e.files) {
    // Identical texts (the Apache licence, mostly) are printed once and referred to after.
    const first = printed.get(f.text);
    if (first) {
      lines.push(`--- ${f.name}: the same text as ${first} above ---`, "");
    } else {
      printed.set(f.text, `${e.title}'s ${f.name}`);
      lines.push(`--- ${f.name} ---`, f.text, "");
    }
  }
}
lines.push("=".repeat(78), "Standard licence texts", "=".repeat(78), "");
for (const id of [...missing].toSorted()) {
  lines.push(`--- ${id} ---`, standard.get(id) ?? "(See https://spdx.org/licenses/ for this licence's text.)", "");
}
const notices = join(root, "apps", "kivo-app", "src-tauri", "THIRD_PARTY_NOTICES.txt");
writeFileSync(notices, `${lines.join("\n")}\n`);
const without = entries.filter((e) => e.files.length === 0).length;
console.log(`${entries.length} packages (${without} without a licence file) → ${notices}`);
