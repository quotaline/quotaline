---
title: Install on GNOME
description: Build the Shell extension and local service without root access.
weight: 1
---

Use GNOME Shell 50 or 51 with [mise](https://mise.jdx.dev/installing-mise.html), `gnome-extensions`, and
`systemctl --user`.

```sh
git clone https://github.com/quotaline/quotaline
cd quotaline
mise install --locked
mise exec -- just install
```

The installer builds the Rust service, installs it under `~/.local/libexec`, starts the service, and installs the
extension under `~/.local/share/gnome-shell/extensions`. It enables the extension for your next login and does not need
root access. A running Wayland Shell may not discover a newly installed extension and may retain previously loaded
JavaScript after an update. Log out and back in to load the new extension code.

Use `mise exec -- just uninstall` to remove the extension and service. Your settings and history remain in your home
directory. At later logins, GNOME starts the enabled extension and activates the service on demand. Use GNOME Extensions
to control whether the panel item starts at login.

Open **Settings → Demo data** to see all six provider cards without signing in. Turn demo data off before checking your
own accounts.

Next, [connect a provider](/start/connect/) or open the [interface guide](/gnome/interface/).
