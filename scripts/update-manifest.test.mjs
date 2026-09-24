// DIST-07: the update manifest for a channel, from the installers' updater signatures.
//   pnpm test:scripts
import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { channelOf, manifest } from "./update-manifest.mjs";

test("the channel comes from the tag", () => {
  assert.equal(channelOf("v1.2.0"), "stable");
  assert.equal(channelOf("v1.2.0-beta.3"), "beta");
  assert.equal(channelOf("v1.2.0-exp.1"), "experimental");
  assert.throws(() => channelOf("v1.2"), /not a release tag/);
  assert.throws(() => channelOf("updates"), /not a release tag/);
});

test("signed installers become the manifest's platforms, unsigned ones don't", () => {
  const dir = mkdtempSync(join(tmpdir(), "kivo-manifest-"));
  mkdirSync(join(dir, "nsis"));
  mkdirSync(join(dir, "msi"));
  writeFileSync(join(dir, "nsis", "KIVO_1.2.0_x64-setup.exe"), "exe");
  writeFileSync(join(dir, "nsis", "KIVO_1.2.0_x64-setup.exe.sig"), "c2lnbmF0dXJlLW5zaXM=\n");
  writeFileSync(join(dir, "msi", "KIVO_1.2.0_x64_en-US.msi"), "msi");
  writeFileSync(join(dir, "msi", "KIVO_1.2.0_x64_en-US.msi.sig"), "c2lnbmF0dXJlLW1zaQ==");
  writeFileSync(join(dir, "KIVO_1.2.0_arm64-setup.exe"), "arm, not signed");
  const m = manifest({ tag: "v1.2.0", dir, notes: "Faster wake word.", date: new Date("2026-10-01T09:30:00.123Z") });
  assert.equal(m.version, "1.2.0");
  assert.equal(m.notes, "Faster wake word.");
  assert.equal(m.pub_date, "2026-10-01T09:30:00Z");
  assert.deepEqual(Object.keys(m.platforms).toSorted(), ["windows-x86_64", "windows-x86_64-msi", "windows-x86_64-nsis"]);
  assert.equal(m.platforms["windows-x86_64"].signature, "c2lnbmF0dXJlLW5zaXM=");
  assert.equal(
    m.platforms["windows-x86_64-msi"].url,
    "https://github.com/MadBlast0/K.I.V.O/releases/download/v1.2.0/KIVO_1.2.0_x64_en-US.msi",
  );
  assert.deepEqual(m.platforms["windows-x86_64"], m.platforms["windows-x86_64-nsis"]);
});

test("a release with no signed installer has no manifest", () => {
  const dir = mkdtempSync(join(tmpdir(), "kivo-manifest-"));
  writeFileSync(join(dir, "KIVO_1.2.0_x64-setup.exe"), "exe");
  assert.throws(() => manifest({ tag: "v1.2.0", dir }), /no signed installers/);
});
