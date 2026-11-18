---
title: Architecture
description: Shared Rust rules, platform adapters, and native interfaces.
weight: 2
---

Quotaline has one Cargo workspace and two native interfaces. Shared rules live in topic crates under `crates/`, each
owning one subject, with no UI toolkit dependency. Platform adapters handle operating-system access: credentials,
network requests, persistence, and native bindings.

| Path                                 | Responsibility                                                       |
| ------------------------------------ | -------------------------------------------------------------------- |
| `crates/catalog`                     | The provider list, refresh policy, and projector contract.           |
| `crates/presentation/quota`          | A provider's quota reading and the projections views derive from it. |
| `crates/settings`                    | The settings document, its validation, and history preferences.      |
| `crates/presentation/status-bar`     | Window selection, short labels, status layout, and widget rows.      |
| `crates/presentation/notifications`  | Notification planning and wording.                                   |
| `crates/refresh`                     | Refresh scheduling, request coalescing, and retry backoff.           |
| `crates/data/analytics`              | History charts, analytics series, and incremental fetch progress.    |
| `crates/http`                        | Failure classification and the transport request pipelines use.      |
| `crates/accounts/credentials`        | Credential selection, token refresh, and save recovery.              |
| `crates/logging`                     | Shared log line format, redaction, and retention.                    |
| `crates/accounts/setup`              | Setup issues, resource grants, primary actions, and sign-in flow.    |
| `crates/presentation/text`           | Text and time wording and the shared interface copy.                 |
| `crates/data/export`                 | The usage export file and command output.                            |
| `crates/demo`                        | Preview data for demo mode and screenshots.                          |
| `crates/providers/<provider>`        | One crate per provider: requests, fetch pipeline, and projection.    |
| `crates/accounts/credential-sources` | Shared credential discovery rules.                                   |
| `crates/data/history`                | SQLite history and export behavior for both desktops.                |
| `crates/data/storage`                | Private file access and atomic writes.                               |
| `crates/engine`                      | Credential and file access, HTTP requests, and transcript I/O.       |
| `gnome/src`                          | The D-Bus service, refresh scheduling, and state sent to the Shell.  |
| `gnome/extension`                    | GNOME Shell controls, charts, accessibility, and panel placement.    |
| `macos/platform`                     | macOS credential, persistence, and native access policies.           |
| `macos/adapter`                      | UniFFI records and functions that expose Rust rules to Swift.        |
| `macos/src`                          | The macOS app, status item, popover, widgets, and native services.   |

Each Rust crate keeps implementation in `src/` and tests in `tests/`. GNOME Shell code is strict TypeScript in
`gnome/extension/src/`, with the `extension.ts` entry point at the extension root and the D-Bus payload types in
`src/service-state.ts`. `just extension-build` compiles it with `tsc` alone, one readable ES module per source file,
into `target/extension/`, which packaging, installs, demos, and every Shell test load. Its TypeScript unit tests in
`gnome/extension/tests/` mirror the source paths; Shell scenarios are grouped under `tests/integration/`. Both run
against that build. Swift code is in `macos/src/` and its tests and fixtures are in `macos/tests/`. Run `just lines` at
the repository root to compare source and test line counts by component. It reports generated Swift bindings separately.

Swift retains macOS-specific clients and Apple API access. Move a decision to Rust when multiple frontends need it,
while keeping visible text, layout, colors, and widgets in the native interfaces. The GNOME extension receives facts
from the Rust service through D-Bus and does not read provider credentials.

Provider response rules belong in that provider's crate, such as `crates/providers/claude`. Platform adapters implement
the I/O that obtains each response. A new provider needs a shared definition and projector, adapter code for each
supported platform, and tests that pass provider data through the public boundary. See
[Provider development](/contributing/providers/) for the current contract.

The [build guide](/contributing/build/) gives platform commands. [Testing](/contributing/testing/) covers Rust, GNOME
Shell, Swift package, and native macOS checks.
