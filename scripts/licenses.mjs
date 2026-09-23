#!/usr/bin/env node
// Writes the libraries KIVO ships and their licences, for Settings → About (UX-31): every Rust
// crate the three programs (kivo-runtime, kivo-app, kivo-infer) are built from, following normal
// and build dependencies only, and every npm package in the app's production dependencies.
// Model licences come from the model manifest at run time. The installer's generated
// THIRD_PARTY_NOTICES (DIST-16) is separate.
//
//   pnpm licenses:gen
import { execFileSync, execSync } from "node:child_process";
import { writeFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const out = join(root, "apps", "kivo-app", "src", "generated", "licenses.json");
const SHIPPED = ["kivo-runtime", "kivo-app", "kivo-infer"];

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
const rust = [...seen]
  .filter((id) => !workspace.has(id))
  .map((id) => byId.get(id))
  .map((p) => ({ name: p.name, version: p.version, license: p.license ?? p.license_file ?? "see crate" }))
  .toSorted((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version));

// A fixed command (pnpm is a .cmd shim on Windows, which needs the shell).
const npmRaw = JSON.parse(
  execSync("pnpm licenses list --json --prod", {
    cwd: join(root, "apps", "kivo-app"),
    maxBuffer: 64 * 1024 * 1024,
    encoding: "utf8",
  }),
);
const npm = Object.values(npmRaw)
  .flat()
  .flatMap((p) => p.versions.map((v) => ({ name: p.name, version: v, license: p.license })))
  .toSorted((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version));

mkdirSync(dirname(out), { recursive: true });
writeFileSync(out, `${JSON.stringify({ rust, npm }, null, 2)}\n`);
console.log(`${rust.length} crates and ${npm.length} npm packages → ${out}`);
