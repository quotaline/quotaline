---
title: Install the terminal dashboard
description: Build the dashboard from source and open a demo or a desktop quota snapshot.
weight: 1
---

Build from source on Linux or macOS with [Rust](https://rustup.rs/).

```sh
git clone https://github.com/quotaline/quotaline
cd quotaline
cargo install --locked --path crates/tui
quotaline-tui --demo
```

Demo mode uses sample quota readings and needs only a terminal. To see your desktop readings, turn on **Settings > Data
\> Collection > Write usage.json** in Quotaline and wait for the next provider refresh. Run `quotaline-tui` to open the
export from that install.

| Desktop app               | Default file                                         |
| ------------------------- | ---------------------------------------------------- |
| macOS Direct or Homebrew  | `~/Library/Application Support/Quotaline/usage.json` |
| GNOME Shell               | `~/.local/share/quotaline/usage.json`                |
| GNOME Shell with XDG data | `$XDG_DATA_HOME/quotaline/usage.json`                |

Pass `--file PATH` for a copy of the export or for the macOS App Store build. Quote paths that contain spaces.

```sh
quotaline-tui --file "$HOME/Library/Containers/tech.bernat.quotaline/Data/Library/Application Support/Quotaline/usage.json"
```

The dashboard reads only the redacted file. Provider sign-in and refreshes run in the desktop app.
