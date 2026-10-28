# Security policy

Quotaline is a macOS menu bar app and GNOME Shell extension that reads AI coding tools' credentials to show quota usage.
This policy says how to report a security problem and how we decide whether a report is one.

## Reporting a vulnerability

**Do not open a public issue for a security problem.** Use
[GitHub's private vulnerability reporting](https://github.com/quotaline/quotaline/security/advisories/new). If neither
works for you, email [gaborjbernat@gmail.com](mailto:gaborjbernat@gmail.com).

Include what the attacker controls and what they gain, the steps or a proof of concept, and the versions and platform
you tested. Write the report yourself; unreviewed tool or model output takes longer to triage.

## What to expect

Volunteers maintain Quotaline, with no on-call rotation. We aim to acknowledge a report within **5 working days** and
assess it within **15 working days**. Fixes are developed in a private fork attached to the draft advisory, ship in a
new release, and the advisory, with a CVE requested through GitHub, goes out with that release. The default embargo is
**90 days** from the report; we agree any change with the reporter. We credit reporters in the advisory under the name
they choose, or not at all. There is no bug bounty.

## Supported versions

Only the latest release receives fixes. Confirm the problem reproduces on it before reporting.

## Vulnerability or bug

A report is a vulnerability when an attacker who controls only an input that the [threat model](THREAT_MODEL.md) lists
as untrusted breaks a guarantee Quotaline makes. A report that first needs control of something the threat model trusts,
such as your unlocked account and Keychain, or the provider clients whose credentials it reads, is a bug: we fix it in
the open and publish no advisory. Classify by who supplies the dangerous input, not by how bad the outcome sounds.

**In scope**

- Credential disclosure: a token or credential file content reaching logs, diagnostics, exports, the widget, or any host
  other than the provider's documented destination.
- Unauthorized file access or modification beyond the documented credential sources and Quotaline's own data
  directories, including writes when token refresh is off.
- Account mixing: one account's data or credentials used for or shown as another's.
- Requests that send credentials to an unintended host.
- The signing, notarization, update and release workflows, including accepting an unsigned update.

**Out of scope**

- Provider API availability or incorrect quota values, unless they expose data or cross an access boundary.
- Vulnerabilities in the provider clients; report those to their vendors.
- An attacker with access to your unlocked account and Keychain.
- A dependency CVE without evidence that the vulnerable code is reachable from Quotaline.

If you cannot tell which side a finding falls on, report it privately and we will decide.

## Credential handling

[Provider sources](website/content/reference/providers/_index.md) documents all six integrations, their credential
stores and endpoint families. Supported sources include client files and Keychain items; Copilot can also read a token
from the app's launch environment.

On macOS, background Keychain reads prohibit user interaction. Denied reads appear in provider setup rather than
repeated permission prompts. The App Store build requests security-scoped grants for supported client resources. On
GNOME, the local Rust service reads supported client files and the Secret Service keyring under the logged-in user.

Token refresh is off by default. When enabled, Claude, Codex and Gemini may write updated credentials to their original
source. Antigravity refreshes remain in memory because its client owns the Keychain item. Cursor and Copilot sources
remain read-only. Refreshing shared credentials can invalidate a value retained by a running CLI; the settings reference
explains that trade-off.

Environment variables affect the app only when its process receives them. Finder and login-item launches do not read
shell startup files. Check Authentication and Connection details in Settings before enabling credential writes.

## Stored data and network traffic

On macOS, Direct and Homebrew support files live under `~/Library/Application Support/Quotaline/`. App Store files live
under `~/Library/Containers/tech.bernat.quotaline/Data/Library/Application Support/Quotaline/`. On GNOME, settings,
history, and cached snapshots live under `~/.config/quotaline/`, `~/.local/share/quotaline/`, and `~/.cache/quotaline/`
by default.

The snapshot cache can contain plan names and account email. History stores quota and analytics records, with account
scoping and configurable retention. Hiding account/project details masks presentation, not the files on disk. Exported
history and copied diagnostics remain under the user's control.

The optional `usage.json` export contains provider IDs, plans, timestamps and quota rows, without credential or email
fields. Widget snapshots contain labels, percentages and reset times. The App Store widget uses
`group.tech.bernat.quotaline`; Direct and Homebrew use `<Team ID>.tech.bernat.quotaline`.

Logs sanitize URLs and sensitive values. The HTTP logger records metadata without request headers or response bodies.
Detailed logging adds refresh timing and UI geometry and is off by default. Review reports before sharing them;
sanitization is not a substitute for inspecting information about your account and local paths.

Usage requests go to the [documented provider destinations](website/content/reference/providers/_index.md), including
Antigravity's discovered loopback service and Copilot enterprise hosts where configured. Direct macOS releases also
contact the update feed. Browser actions such as Source and Report Issue open external pages on request.

The app has no telemetry service or crash-upload endpoint. Verification and package-test fixtures use generated data and
injected credential/network boundaries; they must not read a user's provider accounts.
