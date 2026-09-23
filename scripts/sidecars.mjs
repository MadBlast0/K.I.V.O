// Builds KIVO's sidecars (the runtime and the speech worker) in release mode and puts them where
// Tauri's `externalBin` expects them: `apps/kivo-app/src-tauri/binaries/<name>-<target-triple>`
// (RELEASE.md §2 step 3, DIST-01). Usage: `node scripts/sidecars.mjs [--target <triple>]`.
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const SIDECARS = ["kivo-runtime", "kivo-infer"];
const root = join(dirname(fileURLToPath(import.meta.url)), "..");

const at = process.argv.indexOf("--target");
const explicit = at > 0 ? process.argv[at + 1] : undefined;
const target =
  explicit ??
  /^host: (\S+)$/m.exec(execFileSync("rustc", ["-vV"], { encoding: "utf8" }))?.[1];
if (!target) throw new Error("couldn't tell the Rust target triple (rustc -vV)");

const args = ["build", "--release", "--locked", ...SIDECARS.flatMap((b) => ["--bin", b])];
if (explicit) args.push("--target", explicit);
execFileSync("cargo", args, { cwd: root, stdio: "inherit" });

const exe = target.includes("windows") ? ".exe" : "";
const built = join(root, "target", ...(explicit ? [explicit] : []), "release");
const out = join(root, "apps", "kivo-app", "src-tauri", "binaries");
mkdirSync(out, { recursive: true });
for (const name of SIDECARS) {
  copyFileSync(join(built, name + exe), join(out, `${name}-${target}${exe}`));
  console.log(`sidecar: ${name}-${target}${exe}`);
}
