---
title: Install the terminal dashboard
description: Build the dashboard from source and connect to the desktop frontend.
weight: 1
---

Build on Linux or macOS with [Rust](https://rustup.rs/).

```sh
git clone https://github.com/quotaline/quotaline
cd quotaline
cargo install --locked --path crates/tui
quotaline-tui --demo
```

Run `quotaline-tui` without arguments to use the desktop data.

- **Linux** uses Quotaline’s session service. The terminal and the [GNOME extension](/gnome/install/) are clients of the
  same service. Run the dashboard in that desktop session with the extension enabled.
- **macOS** reads the app's local snapshots, settings, history, and logs. Launch Quotaline to collect a reading first.
  The dashboard uses the Direct or Homebrew support directory when present and checks the App Store container as well.

Run `quotaline-tui --file PATH` to inspect a standalone [usage export](/reference/scripting/). For example:

```sh
quotaline-tui --file "$HOME/Downloads/usage.json"
```

The default desktop mode works with **Write usage.json** turned off. Demo mode and explicit file mode show current quota
snapshots and leave desktop actions disabled.
