#!/usr/bin/env node
// Signs KIVO's catalogs (DISC-17) with the owner's ed25519 key. `keygen` makes the key once: the
// private key goes to %APPDATA%\KIVO-signing\catalog.key (never into the repository) and its
// public key is added to catalogs/keys.txt. Without an argument, every catalogs/*.json gets a
// detached base64 signature beside it (<name>.json.sig). Node's own crypto, no dependencies.
import { generateKeyPairSync, createPrivateKey, sign } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync, appendFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const catalogs = join(root, "catalogs");
const keyDir = join(process.env.APPDATA ?? join(process.env.HOME ?? ".", ".config"), "KIVO-signing");
const keyFile = join(keyDir, "catalog.key");

if (process.argv[2] === "keygen") {
  if (existsSync(keyFile)) {
    console.error(`A key already exists: ${keyFile}`);
    process.exit(1);
  }
  const { publicKey, privateKey } = generateKeyPairSync("ed25519");
  mkdirSync(keyDir, { recursive: true });
  writeFileSync(keyFile, privateKey.export({ type: "pkcs8", format: "pem" }), { mode: 0o600 });
  // The raw 32-byte public key: the last 32 bytes of its SPKI form.
  const raw = publicKey.export({ type: "spki", format: "der" }).subarray(-32);
  appendFileSync(join(catalogs, "keys.txt"), `${raw.toString("base64")}\n`);
  console.log(`Key made: ${keyFile}\nIts public key was added to catalogs/keys.txt.`);
  process.exit(0);
}

if (!existsSync(keyFile)) {
  console.error("No signing key yet. Run: pnpm catalog:sign keygen");
  process.exit(1);
}
const key = createPrivateKey(readFileSync(keyFile));
for (const name of readdirSync(catalogs).filter((f) => f.endsWith(".json"))) {
  const body = readFileSync(join(catalogs, name));
  const doc = JSON.parse(body.toString("utf8"));
  if (typeof doc.kind !== "string" || typeof doc.version !== "number") {
    console.error(`${name}: needs "kind" and a numeric "version"`);
    process.exit(1);
  }
  writeFileSync(join(catalogs, `${name}.sig`), sign(null, body, key).toString("base64"));
  console.log(`Signed ${name} (version ${doc.version})`);
}
