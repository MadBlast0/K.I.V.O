# Catalogs

KIVO's catalogs (DISCOVERY §3, DISC-17), published here and fetched by KIVO once a day:

| File | What it is |
|---|---|
| `cli.json` | The CLI install catalog: each agent's install and adapter-install commands |
| `connectors.json` | The connector directory |
| `plugins.json` | The plugin index |
| `models.json` | The model catalog |

Each is `{ "kind": "<name>", "version": <n>, "entries": [...] }` with a detached ed25519 signature
beside it (`<name>.json.sig`, base64). KIVO uses a fetched catalog only when a key in `keys.txt`
signed it and its version is newer than the one it has; otherwise the catalog built into KIVO
applies.

```bash
pnpm catalog:sign keygen   # once: makes the owner's key (kept outside the repository) and adds its public key to keys.txt
pnpm catalog:sign          # signs every catalogs/*.json
```
