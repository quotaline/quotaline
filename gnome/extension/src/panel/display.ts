import Clutter from 'gi://Clutter';
import GLib from 'gi://GLib';
import St from 'gi://St';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {usageColor, usageStatusColor} from '../color.js';
import {menuWidth, statusBudget} from '../menu-geometry.js';
import type {PanelMonitor, TokenPanel} from '../panel.js';
import type {MiniBarGroup, StatusCell, StatusTier} from '../service-state.js';
import {COPY} from '../shared-copy.js';
import {timePresentation} from '../time-presentation.js';
import {box, button, clear, coloredLabel, label, meter, percent} from './support.js';

// Status cells never carry bars, which is how a Mini bars group is told apart.
type PanelCell = (StatusCell & {bars?: undefined}) | MiniBarGroup;

export const displayMethods = {
    _panelCell(this: TokenPanel, cell: PanelCell): St.Widget {
        if (cell.bars) return this._miniBarCell(cell);
        const lines = cell.text.split('\n').filter(Boolean);
        const stacked = lines.length > 1;
        const styleClass = `tmb-panel-cell${stacked ? ' tmb-panel-cell-stacked' : ''}`;
        const item = stacked ? verticalPanelBox(styleClass) : box(false, styleClass);
        const color = cell.stale
            ? this._light
                ? '#4b4b4b'
                : '#d8d8da'
            : usageStatusColor(cell.restricted ? 100 : cell.used, null, this._light);
        for (const [position, line] of lines.entries()) {
            if (stacked) {
                const text = position
                    ? coloredLabel(line, 'tmb-panel-line tmb-panel-value', color)
                    : label(line, 'tmb-panel-line');
                if (position)
                    text.set_style(
                        `color: ${color}; border-bottom: 2px solid ${
                            cell.stale ? '#808080' : usageColor(cell.restricted ? 100 : cell.used)
                        };`,
                    );
                text.x_expand = true;
                text.x_align = Clutter.ActorAlign.FILL;
                text.y_expand = true;
                text.y_align = position ? Clutter.ActorAlign.START : Clutter.ActorAlign.END;
                item.add_child(text);
                continue;
            }
            const match = /\d+(?:\.\d+)?%/.exec(line);
            if (match) {
                const before = line.slice(0, match.index);
                const after = line.slice(match.index + match[0].length);
                if (before) item.add_child(label(before, 'tmb-panel-inline'));
                item.add_child(coloredLabel(match[0], 'tmb-panel-inline tmb-panel-value', color));
                if (after) item.add_child(label(after, 'tmb-panel-inline'));
            } else {
                item.add_child(
                    cell.restricted || cell.stale
                        ? coloredLabel(line, 'tmb-panel-inline', color)
                        : label(line, 'tmb-panel-inline'),
                );
            }
        }
        return item;
    },

    _miniBarCell(this: TokenPanel, group: MiniBarGroup): St.BoxLayout {
        const item = box(false, 'tmb-panel-cell tmb-panel-mini');
        const icon = this._providerIcon(group.provider);
        icon.icon_size = 12;
        item.add_child(icon);
        const rows = verticalPanelBox('tmb-panel-mini-rows');
        if (group.bars.length > 2) rows.add_style_class_name('tmb-panel-mini-dense');
        for (const cell of group.bars) {
            const row = box(false, 'tmb-panel-mini-row');
            const name = label(cell.label, 'tmb-panel-mini-label');
            name.width = 42;
            row.add_child(name);
            row.add_child(
                meter(30, cell.used, cell.stale ? '#808080' : usageColor(cell.restricted ? 100 : cell.used), {
                    height: 4,
                    track: 'tmb-panel-mini-track',
                    fill: 'tmb-panel-mini-fill',
                    y_align: Clutter.ActorAlign.CENTER,
                }),
            );
            rows.add_child(row);
        }
        item.add_child(rows);
        item.accessible_name = `${this._providerNames[group.provider]}: ${group.bars
            .map(cell => `${cell.label} ${percent(cell.used)}`)
            .join(', ')}`;
        return item;
    },

    _render(this: TokenPanel): void {
        const state = this._state;
        this._widget?.update(this._state);
        const tone = this._state?.status_icon_tone ?? 'normal';
        this._panelGlyph.opacity = this._headerGlyph.opacity = {normal: 255, attention: 255, offline: 130}[tone];
        this._panelDot.visible = this._headerDot.visible = tone === 'attention';
        const cells = this._state?.cells ?? [];
        const signature = JSON.stringify([
            this._state?.settings.format,
            Boolean(this._state?.settings.fit_to_space),
            this._state?.settings.show_as,
            this._state?.settings.decimals,
            cells.map(cell => [cell.id, cell.text, cell.used, cell.restricted, cell.limited, cell.stale]),
        ]);
        if (signature !== this._panelSignature) {
            const container = this._indicator.container;
            const box = container.get_parent() as Clutter.Actor;
            const panel = box.get_parent() as Clutter.Actor;
            const occupiedWidth =
                box
                    .get_children()
                    .filter(actor => actor !== container && actor.visible)
                    .reduce((width, actor) => width + actor.get_preferred_width(-1)[1], 0) +
                panel
                    .get_children()
                    .filter(actor => actor !== box && actor.visible)
                    .reduce((width, actor) => width + actor.get_preferred_width(-1)[1], 0);
            const budget = statusBudget(this._monitorWidth, occupiedWidth);
            const measure = (items: PanelCell[]) =>
                items.map((cell, index) => {
                    if (index)
                        this._panelCells.add_child(
                            new St.Widget({style_class: 'tmb-panel-separator', y_align: Clutter.ActorAlign.CENTER}),
                        );
                    const actor = this._panelCell(cell);
                    this._panelCells.add_child(actor);
                    return {cell, actor};
                });
            const totalWidth = () => this._panelCells.get_preferred_width(-1)[1];
            let tier: StatusTier = 'icon';
            let entries: {cell: PanelCell; actor: St.Widget}[] = [];
            if (state)
                for (const candidate of state.status_variants) {
                    clear(this._panelCells);
                    const items =
                        candidate.tier === 'configured' && state.settings.format === 'mini-bars'
                            ? state.mini_bar_groups
                            : candidate.cells;
                    entries = measure(items);
                    tier = candidate.tier;
                    if (!state.settings.fit_to_space || totalWidth() <= budget) break;
                }
            this._panelIcon.visible = entries.length === 0;
            if (!entries.length && !cells.length) this._panelCells.add_child(label(COPY.appName, 'tmb-panel-label'));
            this._logDiagnostic('status', `tier=${tier} width=${totalWidth()} budget=${budget} cells=${cells.length}`);
            this._panelSignature = signature;
        }
        this._indicator.accessible_name = cells.length
            ? this._state?.settings.format === 'mini-bars'
                ? cells
                      .map(cell => `${this._providerNames[cell.provider]} ${cell.label} ${percent(cell.used)}`)
                      .join(', ')
                : cells.map(cell => cell.text.replaceAll('\n', ' ')).join(', ')
            : COPY.appName;
        if (this._renderedTab !== this._tab) {
            clear(this._tabs);
            for (const tab of ['Usage', 'History', 'Settings'] as const) {
                const selected = this._tab === tab;
                const control = button(
                    tab,
                    () => {
                        this._selectTab(tab);
                    },
                    `tmb-tab${selected ? ' tmb-tab-selected' : ''}`,
                );
                const content = box(false, 'tmb-tab-content');
                content.x_align = Clutter.ActorAlign.CENTER;
                content.add_child(
                    new St.Icon({
                        icon_name: {
                            Usage: 'org.gnome.SystemMonitor-symbolic',
                            History: 'document-open-recent-symbolic',
                            Settings: 'emblem-system-symbolic',
                        }[tab],
                        icon_size: 11,
                    }),
                );
                content.add_child(label(tab));
                control.set_child(content);
                control.accessible_name = tab;
                this._tabs.add_child(control);
            }
            this._renderedTab = this._tab;
        }
        if (this._indicator.menu.isOpen) {
            this._renderPage();
        }
        this._footerSecondary.visible = this._tab === 'Settings';
    },

    _panelMonitor(this: TokenPanel): PanelMonitor {
        return (
            Main.layoutManager.primaryMonitor ?? {
                x: 0,
                width: Math.max(800, global.stage.width),
                height: Math.max(720, global.stage.height),
            }
        );
    },

    _updateHeader(this: TokenPanel): void {
        this._headerUpdated.text = this._timePresentation?.header_updated ?? 'Updated never';
        this._headerActivity.text = this._timePresentation?.header_activity ?? '';
        if (this._state?.settings.demo) this._headerActivity.add_style_class_name('tmb-demo-badge');
        else this._headerActivity.remove_style_class_name('tmb-demo-badge');
        for (const {actor, id} of this._providerStatusLabels)
            actor.text = this._timePresentation?.provider_statuses[id] as string;
        if (this._ageClock) {
            GLib.Source.remove(this._ageClock);
            this._ageClock = 0;
        }
        const delay = this._indicator.menu.isOpen ? (this._timePresentation?.next_update_ms ?? null) : null;
        const state = this._state;
        if (delay !== null && state)
            this._ageClock = GLib.timeout_add(GLib.PRIORITY_DEFAULT, delay, () => {
                this._ageClock = 0;
                this._timePresentation = timePresentation(state, GLib.get_real_time() / 1_000_000);
                state.time_presentation = this._timePresentation;
                this._widget?.update(state);
                this._updateHeader();
                return GLib.SOURCE_REMOVE;
            });
    },

    _menuWidth(this: TokenPanel, monitor: PanelMonitor): number {
        return menuWidth(monitor.width, 880);
    },

    _alignMenu(this: TokenPanel, monitor: PanelMonitor, width: number): void {
        const [x] = this._indicator.get_transformed_position();
        const sourceWidth = this._indicator.width;
        const half = width / 2 + 16;
        const anchor = Math.clamp(x + sourceWidth / 2, monitor.x + half, monitor.x + monitor.width - half);
        const alignment = Math.clamp((anchor - x) / Math.max(1, sourceWidth), 0, 1);
        if (this._menuSourceAlignment === null || Math.abs(this._menuSourceAlignment - alignment) >= 0.001) {
            this._indicator.menu.setSourceAlignment(alignment);
            this._menuSourceAlignment = alignment;
        }
    },

    _renderPage(this: TokenPanel): void {
        const focusModelFilter =
            this._tab === 'Settings' && (this._modelFilterEntry?.clutter_text.has_key_focus() ?? false);
        const focusChart = this._tab === 'History' && (this._chart?.has_key_focus() ?? false);
        const focusQuota =
            this._tab === 'Usage' && this._pageTab === 'Usage'
                ? [...this._quotaRows].find(([, actor]) => actor.has_key_focus())?.[0]
                : null;
        const focusSetting =
            this._tab === 'Settings' && this._pageTab === 'Settings'
                ? [...this._settingFocus].find(([, actor]) => actor.has_key_focus())?.[0]
                : null;
        const focusHistory =
            this._tab === 'History' && this._pageTab === 'History'
                ? [...this._historyFocus].find(([, actor]) => actor.has_key_focus())?.[0]
                : null;
        const focusedLabel =
            this._tab === 'Settings' && this._pageTab === 'Settings'
                ? [...this._labelEntries].find(([, entry]) => entry.clutter_text.has_key_focus())
                : null;
        const focusedConflict =
            this._tab === 'Settings' && this._pageTab === 'Settings'
                ? [...this._labelConflictIcons].find(([, icon]) => icon.has_key_focus())?.[0]
                : null;
        const labelDraft: [string, string] | null = focusedLabel ? [focusedLabel[0], focusedLabel[1].get_text()] : null;
        if (focusedLabel) this._suppressedLabelSaves.add(focusedLabel[1]);
        const monitor = this._panelMonitor();
        this._monitorWidth = monitor.width;
        this._scroll.style = `max-height: ${Math.max(180, monitor.height - 160)}px;`;
        const width = this._menuWidth(monitor);
        this._alignMenu(monitor, width);
        this._layoutWidth = width;
        this._root.width = width;
        const geometry = JSON.stringify([
            this._state?.settings.detailed_logging,
            this._tab,
            width,
            monitor.width,
            monitor.height,
            this._menuSourceAlignment,
        ]);
        if (geometry !== this._geometrySignature) {
            this._geometrySignature = geometry;
            this._logDiagnostic(
                'geometry',
                `tab=${this._tab} menu=${width} monitor=${monitor.width}x${monitor.height} anchor=${this._menuSourceAlignment}`,
            );
        }
        this._compact = width < 790;
        this._headerInfo.visible = width >= 480;
        this._headerBalance.visible = width >= 800;
        if (width < 800) this._root.add_style_class_name('tmb-root-compact-header');
        else this._root.remove_style_class_name('tmb-root-compact-header');
        if (width < 440) this._root.add_style_class_name('tmb-root-narrow');
        else this._root.remove_style_class_name('tmb-root-narrow');
        const narrowFooter = width < 440;
        this._footer.orientation = narrowFooter ? Clutter.Orientation.VERTICAL : Clutter.Orientation.HORIZONTAL;
        if (narrowFooter && this._footerQuit.get_parent() === this._footer) {
            this._footer.remove_child(this._footerQuit);
            this._footerPrimary.add_child(this._footerQuit);
        } else if (!narrowFooter && this._footerQuit.get_parent() === this._footerPrimary) {
            this._footerPrimary.remove_child(this._footerQuit);
            this._footer.add_child(this._footerQuit);
        }
        this._footerPrimary.x_expand = narrowFooter;
        this._footerPrimarySpacer.visible = narrowFooter;
        this._footerSpacer.visible = !narrowFooter;
        this._footerSecondary.orientation = width < 320 ? Clutter.Orientation.VERTICAL : Clutter.Orientation.HORIZONTAL;
        this._providerRows.clear();
        this._quotaRows.clear();
        this._providerStatusLabels = [];
        this._settingFocus.clear();
        this._historyFocus.clear();
        this._labelEntries.clear();
        this._labelConflictIcons.clear();
        clear(this._page);
        this._pageTab = this._state ? this._tab : null;
        this._modelFilterEntry = null;
        this._chart = null;
        const state = this._state;
        if (!state) {
            this._page.add_child(label('Connecting to the local service…'));
            this._page.add_child(label(this._error || 'Install or start Quotaline service.', 'tmb-secondary'));
            this._updateHeader();
            return;
        }
        // Errors clear on the next good state; a completed action's notice stays until the user moves on.
        for (const notice of [this._error, this._notice].filter(Boolean))
            this._page.add_child(label(notice, 'tmb-notice'));
        // The casts below undo narrowing from the resets above: each render call sets the chart or entry again.
        if (this._tab === 'Usage') {
            this._renderUsage(state);
            if (focusQuota) this._quotaRows.get(focusQuota)?.grab_key_focus();
        } else if (this._tab === 'History') {
            this._renderHistory(state);
            if (focusChart) (this._chart as St.DrawingArea | null)?.grab_key_focus();
            if (focusHistory) this._historyFocus.get(focusHistory)?.grab_key_focus();
        } else {
            this._renderSettings(state, labelDraft);
            if (focusModelFilter) (this._modelFilterEntry as St.Entry | null)?.grab_key_focus();
            if (focusSetting) this._settingFocus.get(focusSetting)?.grab_key_focus();
            if (labelDraft) this._labelEntries.get(labelDraft[0])?.clutter_text.grab_key_focus();
            if (focusedConflict) this._labelConflictIcons.get(focusedConflict)?.grab_key_focus();
        }
        this._updateHeader();
    },

    _selectTab(this: TokenPanel, tab: string): void {
        this._tab = tab;
        this._notice = null;
        if (tab !== 'Settings') {
            this._discardLog();
            this._modelFilterEntry = null;
        }
        this._metricOpen = false;
        this._set('last_tab', tab);
        this._render();
        this._logDiagnostic(
            'tabs',
            `tab=${tab} width=${this._root.width} height=${this._root.get_preferred_height(-1)[1]}`,
        );
        if (tab === 'Usage') this._loadCosts();
        if (tab === 'History') this._loadHistory();
    },
};

function verticalPanelBox(styleClass: string): St.Widget {
    return new St.Widget({
        layout_manager: new Clutter.BoxLayout({orientation: Clutter.Orientation.VERTICAL}),
        style_class: styleClass,
    });
}

export type DisplayMethods = typeof displayMethods;
