---
title: Terminal dashboard
description: View the desktop quota snapshot in a terminal on Linux or macOS.
weight: 5
layout: platform
---

The terminal dashboard reads the [desktop usage export](/reference/scripting/) on Linux and macOS. Start with
[installation and demo mode](/tui/install/) or open the dashboard once Quotaline has written `usage.json`.

The provider panel shows the highest reported usage percentage and its quota window for each provider. The detail panel
shows each quota window, its usage, and its reset time if one is available. The **Observed** time is when the desktop
app collected the provider reading. Times are shown in UTC.

| Key       | Action                       |
| --------- | ---------------------------- |
| ↑/↓, j/k  | Move within the active panel |
| Tab       | Switch between panels        |
| r         | Reload the snapshot          |
| q, Ctrl+C | Quit                         |

The panels stack in narrower terminals. The dashboard checks the file every two seconds. If a read fails, it keeps the
previous readings on screen with a **Stale** warning until it can read the file again. Check the observed time when the
desktop app is closed or has stopped writing the export.
