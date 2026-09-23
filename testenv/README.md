# testenv — the safe test ground

Computer-control tests act only on what's in here (TOOLS_AND_CONTROL §10). They never touch
the user's own apps, files, volume or input.

| Path | What it is |
|---|---|
| `app/` | `kivo-test-app`, a dummy Win32 app. `cargo test --workspace` builds it; tests start it with `--title <unique>` and `--out <temp dir>`. It never takes focus. |
| `pages/` | Local HTML: `form.html` (fields, select, check box, submit), `login.html` (a fake login: never typed into), `injection.html` (prompt-injection text in hidden and visible forms). |
| `documents/` | `malicious.md`: a document with embedded instructions for the AI. |
| `files/` | A synthetic file tree. Tests copy it into a temp folder and act on the copy. |
| `shell/` | The sandboxed shell working folder. Tests copy it into a temp folder first. |

## The dummy app's AutomationIds

UI Automation shows each Win32 control's id as its AutomationId.

| Id | Control |
|---|---|
| 101 | Name (edit) |
| 102 | Password (edit, `IsPassword`) |
| 103 | Greet (button): status becomes `Hello, <name>` |
| 104 | Remember me (check box) |
| 105 | List: Alpha, Beta, Gamma |
| 106 | Colour (combo box): Red, Green, Blue |
| 107 | Export… (button): opens the Export window |
| 108 | Status text |
| 109 | Notes (multi-line edit) |
| 110 | Address bar (edit; for the browser-fallback tests with `--title "… - Google Chrome"`) |
| 201 | Export: file name |
| 202 | Export: Save (writes the name field into `<out>/<file name>`; plain names only) |
| 203 | Export: Cancel |

Destructive tests (deleting real files, shutting down) run only in Windows Sandbox or a VM, never
here.
