# Security Policy

KIVO is a privileged desktop assistant. It can control apps, files and input, so security reports
are taken seriously.

## Supported versions

KIVO is pre-release. There are no supported releases yet. Once releases begin, this section will
list which versions receive security fixes (Stable and the latest Beta).

## Reporting a vulnerability

**Please do not open public issues for vulnerabilities.**

Use GitHub's **private vulnerability reporting**: the "Report a vulnerability" button on the
repository's Security tab. Include:

- the affected component (runtime, IPC, permission engine, browser extension, MCP, updater, …);
- steps to reproduce, or a proof of concept;
- the impact, as you understand it.

**Expected response:** acknowledgement within 7 days, and a triage decision within 14 days.
Coordinated disclosure after a fix; reporters are credited unless they prefer otherwise.

## Especially interesting areas

- **Permission-engine bypass:** a tool running without the required confirmation.
- **Prompt injection** that causes an outbound action or data exfiltration.
- **Local IPC:** another user or process driving the runtime.
- **Secrets** leaking into prompts, logs or diagnostics.
- **Update or signature verification** bypass.
- **Browser extension or native-messaging host** abuse.
- **Voice spoofing** leading to a High-risk action (by design, voice alone should never be enough).

The threat model is in [docs/architecture/SECURITY.md](docs/architecture/SECURITY.md).
