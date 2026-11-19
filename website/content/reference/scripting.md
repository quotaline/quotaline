---
title: Use quota in scripts
description: Read cached JSON for shell prompts or dashboards without another provider request.
weight: 8
---

Shell prompts and custom dashboards can read the last cached quota snapshot without another provider request. Use the
command for your platform:

```sh
# macOS
"/Applications/Quotaline.app/Contents/MacOS/Quotaline" --usage-json

# GNOME Shell, after a local install
"$HOME/.local/libexec/quotaline-gnome" --usage-json
```

The command exits with JSON on stdout. Without a cache, it exits 1 with `usage export failed: no cached usage yet`; open
the app and allow a refresh to finish. It does not start the UI or fetch new data.

The JSON contains provider `id`, `plan`, `fetchedAt` and `windows`. Each window contains `id`, `label`, `usedPercent`
and an optional ISO-8601 `resetsAt`. It does not include credentials or email.

**Settings > Data > Collection > Write usage.json** writes that document to the support directory when snapshots change.
It is off by default. Leave it off unless an external script needs it; the app and widgets do not use this file.

The default location is `~/Library/Application Support/Quotaline/usage.json` on macOS and
`~/.local/share/quotaline/usage.json` on GNOME. An XDG data directory override changes the GNOME path.

```sh
jq -r '.providers[] | .id as $provider | .windows[] | "\($provider) \(.label): \(.usedPercent)% used"' \
  "$HOME/Library/Application Support/Quotaline/usage.json"
```

The example path is for Direct and Homebrew builds. Use the sandbox support path for an App Store install, or the GNOME
path above on Linux. The file may be stale if collection has stopped; inspect `fetchedAt`. Turning export off stops
future writes, so do not treat an existing file as evidence that export remains enabled.

The [terminal dashboard](/tui/) opens the same file with `quotaline-tui --file PATH` and shows quota percentages and
reset times without another provider request.
