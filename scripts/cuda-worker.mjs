#!/usr/bin/env node
// Builds the CUDA speech worker (kivo-infer-cuda, VOICE-50) for development, when NVIDIA's CUDA
// Toolkit is installed (CUDA_PATH); without it nothing happens and KIVO uses Vulkan. The runtime
// runs the worker from beside itself once the CUDA pack (NVIDIA's runtime libraries, Voice →
// Models) is installed.
//   pnpm build:cuda [--quiet]
import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const quiet = process.argv.includes("--quiet");

if (process.platform !== "win32" || process.arch !== "x64") {
  if (!quiet) console.log("The CUDA worker is built on Windows x64 only.");
  process.exit(0);
}
// A session started before the toolkit was installed lacks CUDA_PATH: read the machine's setting.
const machineCudaPath = () => {
  try {
    const out = execFileSync(
      "reg",
      ["query", "HKLM\\SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment", "/v", "CUDA_PATH"],
      { encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] },
    );
    return /CUDA_PATH\s+REG_\w+\s+(.+)/.exec(out)?.[1]?.trim();
  } catch {
    return undefined;
  }
};
const cuda = process.env.CUDA_PATH ?? machineCudaPath();
if (!cuda || !existsSync(join(cuda, "bin", "nvcc.exe"))) {
  if (!quiet)
    console.log(
      "No CUDA Toolkit (CUDA_PATH): the CUDA worker isn't built, and KIVO uses Vulkan.",
    );
  process.exit(0);
}
// Visual Studio's CUDA build rules read CUDA_PATH_V<major>_<minor>, which the installer sets for new
// sessions; derive it from the toolkit folder (`…\CUDA\v13.4`) when this one started earlier.
const env = { ...process.env, CUDA_PATH: cuda };
const version = /v(\d+)\.(\d+)$/.exec(cuda.replace(/[\\/]+$/, ""));
if (version) env[`CUDA_PATH_V${version[1]}_${version[2]}`] ??= cuda;
try {
  execFileSync("cargo", ["build", "-p", "kivo-infer-cuda", "--features", "cuda"], { cwd: root, stdio: "inherit", env });
} catch {
  // The app still runs, with Vulkan; `pnpm build:cuda` shows the whole error.
  console.warn("The CUDA worker didn't build; KIVO uses Vulkan. Run `pnpm build:cuda` to see why.");
  if (!quiet) process.exit(1);
}
