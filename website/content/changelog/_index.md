---
title: Changelog
description: Changes shipped in each Quotaline release.
weight: 8
---

Earlier macOS releases were published from the
[archived repository](https://github.com/quotaline/token-menu-bar-macos/releases).

{{< draft-changelog >}}

<!-- release notes -->

<a name="release-0-1-7"></a>

## 0.1.7 (2026-10-02)

<a name="release-0-1-7-added"></a>

### Added

- Track Codex credit balances and update Claude cost estimates and plan labels.

<a name="release-0-1-7-fixed"></a>

### Fixed

- Fix a macOS 27 menu bar redraw loop and the popover position after reopening Settings.

<a name="release-0-1-6"></a>

## 0.1.6 (2026-09-20)

<a name="release-0-1-6-fixed"></a>

### Fixed

- Report a Claude limit as reached only when usage reaches 100%.

<a name="release-0-1-5"></a>

## 0.1.5 (2026-09-20)

<a name="release-0-1-5-fixed"></a>

### Fixed

- Fix stalls and jumps in the stepper, group toggle, and reopened popover.

<a name="release-0-1-4"></a>

## 0.1.4 (2026-09-19)

<a name="release-0-1-4-changes"></a>

### Changes

- Reduce requested provider permissions.

<a name="release-0-1-4-documentation"></a>

### Documentation

- Explain provider access.

<a name="release-0-1-3"></a>

## 0.1.3 (2026-09-19)

<a name="release-0-1-3-added"></a>

### Added

- Add provider marks.

<a name="release-0-1-3-documentation"></a>

### Documentation

- Add a privacy link and third-party notices.

<a name="release-0-1-2"></a>

## 0.1.2 (2026-09-19)

<a name="release-0-1-2-fixed"></a>

### Fixed

- Fix relaunch in the macOS sandbox.

<a name="release-0-1-1"></a>

## 0.1.1 (2026-09-18)

<a name="release-0-1-1-added"></a>

### Added

- Raise the minimum supported macOS version to 15.

<a name="release-0-1-1-packaging"></a>

### Packaging

- Fix release signing and Homebrew packaging.

<a name="release-0-1-0"></a>

## 0.1.0 (2026-09-16)

<a name="release-0-1-0-added"></a>

### Added

- First macOS release, with quota tracking for Claude, Codex, Gemini CLI, Antigravity, Cursor, and GitHub Copilot.
- Add local usage history, widgets, notifications, and Direct and Homebrew downloads.
