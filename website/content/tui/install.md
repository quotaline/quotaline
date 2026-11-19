---
title: Install the terminal dashboard
description: Build the dashboard from source on Linux or macOS and point it at your desktop data.
weight: 1
---

The terminal dashboard runs on Linux and macOS. Windows is not supported yet. Releases do not ship a dashboard binary,
so you build it from source with [Rust](https://rustup.rs/). The repository pins its Rust version, and rustup downloads
it on the first build.

## Build and install

```sh
git clone https://github.com/quotaline/quotaline
cd quotaline
cargo install --locked --path crates/tui
```

Cargo places `quotaline-tui` in `~/.cargo/bin`. If your shell cannot find the command, add that directory to your
`PATH`.

Check the build with demo data. It needs no account and no desktop app:

```sh
quotaline-tui --demo
```

![Usage page in demo mode with quota meters and reset countdowns for Antigravity, Claude and Codex](images/tui-demo.webp)

Press `q` to quit. Demo mode shows sample quota readings only; History, Settings and Logs need your desktop data.

## Connect to your desktop data

Run `quotaline-tui` without arguments. It reads from the desktop app on your platform, so install that first.

### Linux

[Install Quotaline for GNOME](/gnome/install/). The installer sets up a session service, which collects readings and
starts on demand. The dashboard and the GNOME extension both talk to this service, so they show the same readings and
share settings. Run the dashboard inside your desktop session, where the service is reachable.

### macOS

[Install the Quotaline app](/macos/install/) from any channel and open it once so it records a reading. The dashboard
reads the app's snapshots, settings, history and logs. It finds the Direct and Homebrew builds' support directory and
the App Store container. Settings changes, sign-in and refreshes stay in the app.

## Inspect an export

On any supported platform, `--file` opens a [usage export](/reference/scripting/) without a desktop app:

```sh
quotaline-tui --file "$HOME/Downloads/usage.json"
```

## Update and uninstall

To update, pull the repository and run the install command again:

```sh
git pull
cargo install --locked --path crates/tui
```

To remove the dashboard, run `cargo uninstall quotaline-tui`. Your desktop app's settings and history stay in place.

Next, read the [dashboard guide](/tui/).
