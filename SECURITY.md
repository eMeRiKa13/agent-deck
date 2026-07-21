# Security policy

## Supported version

Agent Deck is currently distributed as source code only. Security fixes target
the latest commit on the `main` branch. No downloadable binary is maintained or
supported by this project.

## Report a vulnerability

Use [GitHub private vulnerability reporting](https://github.com/eMeRiKa13/agent-deck/security/advisories/new)
to report a security issue. Do not disclose a suspected vulnerability in a
public issue, discussion, or pull request.

Include the affected files or versions, reproduction steps, expected impact,
and any suggested mitigation. Reports are reviewed on a best-effort basis; this
showcase project does not promise a response or remediation deadline.

## Sensitive data boundary

Agent Deck reads local conversation data and authenticated provider sessions.
Provider tokens stay in the Rust backend and are not serialized to the Vue
interface. A security report is especially helpful if it identifies a path that
could expose credentials, conversation content, or local files outside this
boundary.

## Known RustSec informational warnings

The RustSec audit completed without a vulnerability on July 21, 2026. It
reported these non-failing informational advisories from transitive
dependencies:

- `RUSTSEC-2024-0411` through `RUSTSEC-2024-0420` cover unmaintained GTK3
  bindings, and `RUSTSEC-2024-0429` covers `glib`. These crates are locked for
  other Tauri targets and are not present in the Apple Silicon dependency tree.
- `RUSTSEC-2024-0370` covers the unmaintained `proc-macro-error` crate, which is
  not present in the Apple Silicon dependency tree.
- `RUSTSEC-2025-0075`, `RUSTSEC-2025-0080`, `RUSTSEC-2025-0081`,
  `RUSTSEC-2025-0098`, and `RUSTSEC-2025-0100` cover unmaintained `unic-*`
  crates pulled in by `tauri-utils` through `urlpattern`.

These warnings remain visible in CI and should be removed when upstream Tauri
dependencies no longer require the affected crates.
