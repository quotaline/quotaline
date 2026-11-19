---
title: Install the terminal dashboard
description: Download the dashboard for Linux, macOS or Windows and run it without a desktop app.
weight: 1
---

The terminal dashboard is a single program, `quotaline-tui`. It collects quota from the command-line tools you already
signed in to, so it needs no desktop app. Each [release](https://github.com/quotaline/quotaline/releases/latest)
includes a build for Linux, macOS and Windows, with a `.sha256` file next to each download.

## Linux

Download the x86_64 archive, check it, and put the program on your `PATH`:

```sh
base=https://github.com/quotaline/quotaline/releases/latest/download
curl -fLO "$base/quotaline-tui-linux-x86_64.tar.gz" -O "$base/quotaline-tui-linux-x86_64.tar.gz.sha256"
sha256sum --check quotaline-tui-linux-x86_64.tar.gz.sha256
mkdir -p ~/.local/bin
tar -xzf quotaline-tui-linux-x86_64.tar.gz -C ~/.local/bin quotaline-tui
```

If you also run the [GNOME extension](/gnome/install/), both use the same data folder. They show the same readings,
history and settings, and only one of them collects at a time.

## macOS

The macOS download is one universal binary for Apple silicon and Intel Macs. It is signed with Quotaline's Developer ID
and notarized by Apple:

```sh
base=https://github.com/quotaline/quotaline/releases/latest/download
curl -fLO "$base/quotaline-tui-macos-universal.zip" -O "$base/quotaline-tui-macos-universal.zip.sha256"
shasum -a 256 --check quotaline-tui-macos-universal.zip.sha256
mkdir -p ~/.local/bin
unzip -o quotaline-tui-macos-universal.zip quotaline-tui -d ~/.local/bin
```

Claude Code and GitHub Copilot keep their sign-ins in the Keychain. The first time the dashboard reads one, macOS asks
whether to allow it. Choose **Always Allow** to skip the question next time.

The dashboard keeps its own settings and history in `~/Library/Application Support/Quotaline Terminal`. It does not
share them with the [menu bar app](/macos/install/).

## Windows

Download the x86_64 archive in PowerShell, check it, and unpack it into a folder of your own:

```powershell
$base = "https://github.com/quotaline/quotaline/releases/latest/download"
$zip = "quotaline-tui-windows-x86_64.zip"
Invoke-WebRequest "$base/$zip" -OutFile $zip
Invoke-WebRequest "$base/$zip.sha256" -OutFile "$zip.sha256"
(Get-FileHash $zip -Algorithm SHA256).Hash -eq (Get-Content "$zip.sha256").Split(" ")[0]
Expand-Archive $zip "$env:LOCALAPPDATA\Programs\Quotaline"
```

The check prints `True` when the download is intact. Add `%LOCALAPPDATA%\Programs\Quotaline` to your user `PATH` so
`quotaline-tui` runs from any folder. Settings and history live in `%APPDATA%\quotaline`.

## First run

Start the dashboard:

```sh
quotaline-tui
```

It lists every provider you have already signed in to with its command-line tool. With none yet, it explains how to sign
in:

![Usage page on a new machine, asking you to sign in with the sign-in command](images/tui-first-run.webp)

Type `:sign-in claude` and press Enter, or name `codex`, `gemini`, `cursor` or `copilot`. The dashboard pauses, runs
that tool's own login, such as `claude auth login`, in the same terminal, then refreshes. Later, select a provider and
press `s` to sign in again. Antigravity signs in through its own app. Press `q` to quit.

To look around without any accounts, run `quotaline-tui --demo`:

![Usage page in demo mode with quota meters and reset countdowns for Antigravity, Claude and Codex](images/tui-demo.webp)

To read a [usage export](/reference/scripting/), run `quotaline-tui --file PATH`. Demo and export views are read-only.

## Build from source

With [Rust](https://rustup.rs/) installed, build the latest code from the repository instead:

```sh
git clone https://github.com/quotaline/quotaline
cargo install --locked --path quotaline/crates/tui
```

Cargo places `quotaline-tui` in `~/.cargo/bin`.

## Update and uninstall

To update, download the new release and replace the program. To uninstall, delete `quotaline-tui`, or run
`cargo uninstall quotaline-tui` if you built it. Delete the data folder named above for your system to remove settings
and history too.

Next, learn the [terminal interface](/tui/interface/).
