#!/usr/bin/env node
// Writes a channel's update manifest (DIST-07): the `latest.json` format of tauri-plugin-updater v2
// for the installers of one release, from the minisign signatures the bundler wrote beside them
// (`createUpdaterArtifacts`). The release workflow uploads it as `<channel>.json` to the
// `updates` release once the owner approves publishing (REL-11).
//
//   node scripts/update-manifest.mjs --tag v1.2.0 --dir <bundle dir> --out stable.json [--notes notes.md]
//
// Channel from the tag: vX.Y.Z → stable, vX.Y.Z-beta.N → beta, vX.Y.Z-exp.N → experimental.
import { existsSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { basename, join } from "node:path";

const REPO = "MadBlast0/K.I.V.O";

export function channelOf(tag) {
  const version = tag.replace(/^v/, "");
  if (!/^\d+\.\d+\.\d+(-(beta|exp)\.\d+)?$/.test(version)) throw new Error(`not a release tag: ${tag}`);
  if (version.includes("-exp.")) return "experimental";
  if (version.includes("-beta.")) return "beta";
  return "stable";
}

/** Every installer with a signature beside it, searched one level down (`nsis/`, `msi/`). */
function installers(dir) {
  const found = [];
  for (const sub of ["", "nsis", "msi"]) {
    const d = join(dir, sub);
    if (!existsSync(d)) continue;
    for (const name of readdirSync(d)) {
      const file = join(d, name);
      if ((name.endsWith("-setup.exe") || name.endsWith(".msi")) && existsSync(`${file}.sig`)) {
        found.push({ file, kind: name.endsWith(".msi") ? "msi" : "nsis", arch: name.includes("arm64") ? "aarch64" : "x86_64" });
      }
    }
  }
  return found;
}

export function manifest({ tag, dir, notes = "", date = new Date() }) {
  const version = tag.replace(/^v/, "");
  const platforms = {};
  for (const i of installers(dir)) {
    const entry = {
      signature: readFileSync(`${i.file}.sig`, "utf8").trim(),
      url: `https://github.com/${REPO}/releases/download/${tag}/${encodeURIComponent(basename(i.file))}`,
    };
    platforms[`windows-${i.arch}-${i.kind}`] = entry;
    // The per-user NSIS installer is what a plain `windows-<arch>` means (the primary download).
    if (i.kind === "nsis") platforms[`windows-${i.arch}`] = entry;
  }
  if (Object.keys(platforms).length === 0) throw new Error(`no signed installers in ${dir}`);
  return { version, notes, pub_date: date.toISOString().replace(/\.\d{3}Z$/, "Z"), platforms };
}

const arg = (name) => {
  const i = process.argv.indexOf(`--${name}`);
  return i > 0 ? process.argv[i + 1] : undefined;
};

if (process.argv[1] && basename(process.argv[1]) === "update-manifest.mjs") {
  const tag = arg("tag");
  const dir = arg("dir");
  if (!tag || !dir) {
    console.error("usage: update-manifest.mjs --tag vX.Y.Z --dir <bundle dir> [--out file] [--notes notes.md]");
    process.exit(2);
  }
  const notesFile = arg("notes");
  const notes = notesFile && existsSync(notesFile) ? readFileSync(notesFile, "utf8").trim() : "";
  const out = arg("out") ?? `${channelOf(tag)}.json`;
  writeFileSync(out, `${JSON.stringify(manifest({ tag, dir, notes }), null, 2)}\n`);
  console.log(`${out}: ${channelOf(tag)} ${tag}`);
}
