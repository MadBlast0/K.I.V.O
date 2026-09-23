# Models in the repository

KIVO ships **no models** in its installer (owner decision; DECISIONS "No models in the installer").
Users choose what to download, and the model manager fetches it with sha256 checks
(DISTRIBUTION §4). This copy is only for development and tests; an installed KIVO downloads the
same file (pinned upstream commit, same hash) together with the speech model the user picks.

| File | What it is | Licence |
|---|---|---|
| `silero_vad.onnx` | Silero VAD v6: decides when someone is speaking (VOICE §3) | MIT, © Silero Team |
