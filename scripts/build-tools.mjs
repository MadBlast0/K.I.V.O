#!/usr/bin/env node
// Fetches what building KIVO's graphics-card speech engine (whisper.cpp on Vulkan, in kivo-infer
// on Windows x64) needs beyond Rust, MSVC and CMake, without installers or admin rights:
//
//   - the Vulkan SDK's parts: Khronos' headers, Google's `glslc` shader compiler and an import
//     library for the Vulkan loader (`vulkan-1.dll`, which graphics drivers install);
//   - libclang from the LLVM release, for whisper-rs's bindings (the ones it bundles are Linux's).
//
// Every download is pinned and checked by SHA-256. Installed KIVO needs none of this: only the
// graphics driver's loader.
//
//   pnpm build-tools [--quiet]
//
// The files go to `.tools/` in the repository (ignored by git); `.cargo/config.toml` points
// VULKAN_SDK and LIBCLANG_PATH there unless the environment already sets them (CI installs
// LunarG's SDK and has LLVM). `pnpm dev` and `pnpm build` run this first; it returns at once when
// everything is already in place.

import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { copyFileSync, createReadStream, createWriteStream, cpSync, existsSync, mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { Readable } from "node:stream";
import { pipeline } from "node:stream/promises";
import { fileURLToPath } from "node:url";

const HEADERS = "vulkan-sdk-1.4.357.0";
const LLVM = "clang+llvm-23.1.2-x86_64-pc-windows-msvc";
const FILES = {
  headers: {
    url: `https://github.com/KhronosGroup/Vulkan-Headers/archive/refs/tags/${HEADERS}.tar.gz`,
    name: `vulkan-headers-${HEADERS}.tar.gz`,
    sha256: "e87dce08116151f6b6d7de6b6faf41498e87e6cf848ff16fa3bd5402190ad4a3",
  },
  // shaderc's continuous release build (linked from google/shaderc's downloads.md).
  shaderc: {
    url: "https://storage.googleapis.com/shaderc/artifacts/prod/graphics_shader_compiler/shaderc/windows-vs2022-amd64-release/continuous/42/20260914-072522/install.zip",
    name: "shaderc-20260914-072522.zip",
    sha256: "60d4413cf321e729512e58cb447789223a13d6d2ebd9721313abd19f89efd262",
  },
  // LLVM's own release (the digest GitHub publishes for the asset); only libclang is kept.
  llvm: {
    url: `https://github.com/llvm/llvm-project/releases/download/llvmorg-23.1.2/${LLVM}.tar.xz`,
    name: `${LLVM}.tar.xz`,
    sha256: "8fb91cdc44fcbbdcf6b3ffd0a1f9859abd14a3c3aae4423c2b6d4a4f90bf0095",
  },
};

// Only Windows x64 builds the graphics-card engine (kivo-infer's Cargo.toml).
if (process.platform !== "win32" || process.arch !== "x64") process.exit(0);

const quiet = process.argv.includes("--quiet");
const root = join(dirname(fileURLToPath(import.meta.url)), "..", ".tools");
const downloads = join(root, "downloads");
const sdk = join(root, "vulkan-sdk");
const llvm = join(root, "llvm");
// What `.tools` was made from; a match means there is nothing to do.
const stamp = join(root, "stamp.txt");
const made = Object.values(FILES)
  .map((f) => f.sha256)
  .join(" ");
if (existsSync(stamp) && readFileSync(stamp, "utf8") === made) {
  if (!quiet) console.log(`The build tools are ready in ${root}`);
  process.exit(0);
}
const tar = join(process.env.SystemRoot ?? "C:\\Windows", "System32", "tar.exe");
mkdirSync(downloads, { recursive: true });

async function sha256(file) {
  const hash = createHash("sha256");
  await pipeline(createReadStream(file), hash);
  return hash.digest("hex");
}

async function fetchChecked({ url, name, sha256: expected }) {
  const file = join(downloads, name);
  if (!existsSync(file)) {
    process.stdout.write(`downloading ${name}… `);
    const response = await fetch(url);
    if (!response.ok) throw new Error(`${url}: HTTP ${response.status}`);
    await pipeline(Readable.fromWeb(response.body), createWriteStream(`${file}.part`));
    renameSync(`${file}.part`, file);
    console.log("done");
  }
  const actual = await sha256(file);
  if (actual !== expected) {
    rmSync(file);
    throw new Error(`${name}: SHA-256 ${actual}, expected ${expected} (deleted; run again)`);
  }
  return file;
}

function run(program, args, options = {}) {
  const result = spawnSync(program, args, { encoding: "utf8", ...options });
  if (result.status !== 0) throw new Error(`${program} ${args.join(" ")}\n${result.stdout}${result.stderr}`);
  return result.stdout;
}

/** The names a DLL exports, read from its PE export table. */
function exports(dll) {
  const b = readFileSync(dll);
  const pe = b.readUInt32LE(0x3c);
  if (b.readUInt32LE(pe) !== 0x4550) throw new Error(`${dll} isn't a PE file`);
  const optional = pe + 24;
  if (b.readUInt16LE(optional) !== 0x20b) throw new Error(`${dll} isn't 64-bit`);
  const exportRva = b.readUInt32LE(optional + 112);
  const sectionCount = b.readUInt16LE(pe + 6);
  const sections = optional + b.readUInt16LE(pe + 20);
  const offset = (rva) => {
    for (let i = 0; i < sectionCount; i++) {
      const s = sections + i * 40;
      const va = b.readUInt32LE(s + 12);
      const size = Math.max(b.readUInt32LE(s + 8), b.readUInt32LE(s + 16));
      if (rva >= va && rva < va + size) return rva - va + b.readUInt32LE(s + 20);
    }
    throw new Error(`RVA ${rva} outside every section`);
  };
  const dir = offset(exportRva);
  const count = b.readUInt32LE(dir + 24);
  const names = offset(b.readUInt32LE(dir + 32));
  const found = [];
  for (let i = 0; i < count; i++) {
    const at = offset(b.readUInt32LE(names + i * 4));
    found.push(b.toString("latin1", at, b.indexOf(0, at)));
  }
  return found;
}

/** MSVC's `lib.exe` for x64, found through vswhere. */
function msvcLib() {
  const vswhere = join(process.env["ProgramFiles(x86)"] ?? "C:\\Program Files (x86)", "Microsoft Visual Studio", "Installer", "vswhere.exe");
  const found = run(vswhere, ["-latest", "-products", "*", "-find", "VC\\Tools\\MSVC\\**\\bin\\Hostx64\\x64\\lib.exe"]);
  const lib = found.split(/\r?\n/).find(Boolean);
  if (!lib) throw new Error("MSVC's lib.exe wasn't found: install the Visual Studio Build Tools (C++)");
  return lib;
}

const headers = await fetchChecked(FILES.headers);
const shaderc = await fetchChecked(FILES.shaderc);
const llvmArchive = await fetchChecked(FILES.llvm);
const work = join(root, "work");
rmSync(work, { recursive: true, force: true });
mkdirSync(work, { recursive: true });

// vulkan-sdk/Include: the Khronos headers (vulkan.h and vulkan.hpp).
run(tar, ["-xf", headers, "-C", work]);
rmSync(join(sdk, "Include"), { recursive: true, force: true });
cpSync(join(work, `Vulkan-Headers-${HEADERS}`, "include"), join(sdk, "Include"), { recursive: true });

// vulkan-sdk/Bin: glslc.
run(tar, ["-xf", shaderc, "-C", work, "install/bin/glslc.exe"]);
mkdirSync(join(sdk, "Bin"), { recursive: true });
copyFileSync(join(work, "install", "bin", "glslc.exe"), join(sdk, "Bin", "glslc.exe"));
run(join(sdk, "Bin", "glslc.exe"), ["--version"]);

// vulkan-sdk/Lib/vulkan-1.lib: an import library for the loader the driver installed.
const loader = join(process.env.SystemRoot ?? "C:\\Windows", "System32", "vulkan-1.dll");
if (!existsSync(loader)) throw new Error("vulkan-1.dll is missing: install or update the graphics driver");
const names = exports(loader).filter((n) => n.startsWith("vk"));
if (names.length < 100) throw new Error(`vulkan-1.dll exports only ${names.length} vk functions`);
mkdirSync(join(sdk, "Lib"), { recursive: true });
const def = join(work, "vulkan-1.def");
writeFileSync(def, `LIBRARY vulkan-1.dll\nEXPORTS\n${names.map((n) => `  ${n}`).join("\n")}\n`);
run(msvcLib(), ["/nologo", `/def:${def}`, "/machine:x64", `/out:${join(sdk, "Lib", "vulkan-1.lib")}`]);

// llvm/: libclang.dll and clang's own headers (stddef.h and the like), where libclang looks.
process.stdout.write("unpacking libclang… ");
const resource = join("lib", "clang", "23", "include");
run(tar, ["-xf", llvmArchive, "-C", work, `${LLVM}/bin/libclang.dll`, `${LLVM}/lib/clang/23/include`]);
rmSync(llvm, { recursive: true, force: true });
mkdirSync(join(llvm, "bin"), { recursive: true });
copyFileSync(join(work, LLVM, "bin", "libclang.dll"), join(llvm, "bin", "libclang.dll"));
cpSync(join(work, LLVM, resource), join(llvm, resource), { recursive: true });
console.log("done");

rmSync(work, { recursive: true, force: true });
writeFileSync(stamp, made);
console.log(`\nThe build tools are in ${root} (Vulkan loader: ${names.length} functions).`);
