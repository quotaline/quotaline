---
title: Terminal dashboard
description: Use Quotaline in a terminal on Linux or macOS.
weight: 5
layout: platform
---

The terminal dashboard shows the desktop app's quota readings, history, settings and logs. [Install it](/tui/install/)
on Linux or macOS. It updates as the desktop backend updates and keeps the last readable quota snapshot if a reload
fails.

## Keyboard controls

| Key        | Action                                                         |
| ---------- | -------------------------------------------------------------- |
| 1, 2, 3, 4 | Open Usage, History, Settings, or Logs                         |
| ↑/↓, j/k   | Move through providers, quota windows, series, or text rows    |
| Tab        | Jump to the next provider or settings group                    |
| ←/→        | Adjust a setting, inspect a history reading, or pan long text  |
| d          | Show details for the selected provider                         |
| r          | Reload from the current source                                 |
| R, s       | Request a refresh or sign-in for the selected provider (Linux) |
| [ / ], n   | Page through history ranges or return to the current range     |
| v, i       | Toggle or isolate the selected history series                  |
| Enter      | Activate the selected setting on Linux                         |
| :, ?       | Enter a command or open the command reference                  |
| q, Ctrl+C  | Quit                                                           |

Click a footer link to change views, open the command editor, or quit. Scroll vertically to move through rows or
horizontally to pan long text. Click **Run** or **Cancel** to finish editing. Use your terminal's mouse-selection
shortcut to select text while mouse reporting is active.

## Usage

Usage groups the desktop's selected quota windows by provider. **Observed** shows how long ago the provider last
reported a reading. Each window shows its quota meter, percentage, and time until the next reset. When a provider
reports only a reset date, Usage counts calendar days.

Credit balances, spending and quota alerts appear below each provider's windows. Use the wheel or arrow keys to scroll
the list. Click a row or press **d** for secondary details. Identity visibility follows the desktop setting.

## History

![History page charting one isolated quota series over seven days, with the legend on the right](images/tui-history.webp)

History charts the visible series together. The legend starts at each series' latest reading. Hover over the plot to
inspect all series at once, and move the pointer out to return to the latest readings. Click a legend row to show or
hide its series. The arrow keys select a series or inspect readings.

Turn on **Stack compatible history series** in Settings for stacked daily charts. Use `:period 7` for a week,
`:metric ID` to change metrics, or `:range YYYY-MM-DD YYYY-MM-DD` for a custom interval. `:metrics` lists the metric
IDs.

Use `:export /absolute/path.csv` to export the current metric and range, or `:export-all /absolute/path.csv` to export
all history. Choose a new filename.

## Settings and diagnostics

![Settings page listing provider collection switches and refresh intervals](images/tui-settings.webp)

Settings covers provider collection, quota display and privacy, history, data retention, and logging. On Linux, click a
switch or press **Enter** to toggle it. Click **‹** or **›** or use the arrow keys to adjust a value. The terminal and
desktop app share these settings. Use **?** for model and support commands.

Press **4** for recent desktop logs. `:logs full` shows retained logs, `:diagnostics` displays the diagnostic report,
and `:issue` displays the issue report link. Use the left and right arrows to pan long lines.

The macOS app owns settings changes, sign-in, and refreshes. The terminal can view its settings and logs and work with
the history it has collected. Desktop menu-bar and widget layout, notification delivery, OS credential dialogs, and
permission prompts remain in the desktop frontend.
