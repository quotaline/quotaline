import Clutter from 'gi://Clutter';
import type Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import St from 'gi://St';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
import * as QuickSettings from 'resource:///org/gnome/shell/ui/quickSettings.js';
import {usageColor, usageTextColor} from './color.js';
import {meter} from './panel/support.js';
import {until} from './presentation.js';
import type {PanelState, SnapshotRow} from './service-state.js';
import {COPY} from './shared-copy.js';
import {snapshotStale} from './widget-data.js';

const LIMITS: Partial<Record<string, number>> & {small: number} = {small: 3, medium: 4, large: 8};

export class SnapshotWidget {
    _onOpen: () => void;
    _icon: Gio.Icon;
    _onRefresh: () => void;
    _state: PanelState | null;
    _timer: number;
    indicator: QuickSettings.SystemIndicator;
    toggle: QuickSettings.QuickMenuToggle;
    _warningIcon: St.Icon;
    _clickId: number;
    _openId: number;

    constructor(onOpen: () => void, icon: Gio.Icon, onRefresh: () => void) {
        this._onOpen = onOpen;
        this._icon = icon;
        this._onRefresh = onRefresh;
        this._state = null;
        this._timer = 0;
        this.indicator = new QuickSettings.SystemIndicator();
        this.toggle = new QuickSettings.QuickMenuToggle({
            title: COPY.appName,
            subtitle: 'Waiting for quota data',
            gicon: icon,
            toggleMode: false,
        });
        this._warningIcon = new St.Icon({
            icon_name: 'dialog-warning-symbolic',
            icon_size: 14,
            style_class: 'tmb-widget-warning',
            visible: false,
        });
        this.toggle.menu.addHeaderSuffix(this._warningIcon);
        this._clickId = this.toggle.connect('clicked', this._onOpen);
        this.indicator.quickSettingsItems.push(this.toggle);
        Main.panel.statusArea.quickSettings?.addExternalIndicator(this.indicator);
        this._openId = this.toggle.menu.connect('open-state-changed', (_menu, open) => {
            if (open) {
                this.update(this._state);
                this._onRefresh();
                this._timer = GLib.timeout_add_seconds(GLib.PRIORITY_DEFAULT, 60, () => {
                    this.update(this._state);
                    this._onRefresh();
                    return GLib.SOURCE_CONTINUE;
                });
            } else {
                GLib.Source.remove(this._timer);
                this._timer = 0;
            }
        });
        this.update(null);
    }

    update(state: PanelState | null): void {
        this._state = state;
        const rows = state?.snapshot_rows ?? [];
        const size = state?.settings.widget_size ?? 'small';
        const display = state?.settings.show_as ?? 'used';
        const now = Date.now() / 1000;
        const attention = state?.snapshot_attention ?? false;
        const stale = snapshotStale(rows, now);
        this._warningIcon.visible = attention;
        const notes = [attention ? 'sign-in needed' : null, stale ? 'stale' : null].filter(Boolean);
        this.toggle.subtitle = rows.length
            ? `${rows.length} tracked limits${notes.length ? ` · ${notes.join(' · ')}` : ''}`
            : attention
              ? COPY.signInNeeded
              : 'Open to choose limits';
        // GNOME 51 opens only a menu that already has items, so the rows cannot wait for the menu to open.
        this.toggle.menu.removeAll();
        const updated = state?.last_refresh == null ? '' : ` · updated ${state.time_presentation.refresh_age}`;
        this.toggle.menu.setHeader(
            this._icon,
            COPY.appName,
            state ? `${size} snapshot${updated}${stale ? ' · stale' : ''}` : 'Waiting for local service',
        );
        if (!rows.length) {
            this.toggle.menu.addMenuItem(
                new PopupMenu.PopupMenuItem(state?.last_refresh ? COPY.widgetPickWindows : COPY.widgetStartTracking, {
                    reactive: false,
                }),
            );
        } else {
            const theme = St.Settings.get();
            const light = theme.color_scheme === St.SystemColorScheme.PREFER_LIGHT && !theme.high_contrast;
            for (const row of rows.slice(0, LIMITS[size] ?? LIMITS.small))
                this.toggle.menu.addMenuItem(snapshotRow(row, size, display, now, light));
        }
    }

    destroy(): void {
        if (this._timer) GLib.Source.remove(this._timer);
        this.toggle.menu.disconnect(this._openId);
        this.toggle.disconnect(this._clickId);
        this.indicator.quickSettingsItems.forEach(item => {
            item.destroy();
        });
        this.indicator.destroy();
        this._state = null;
    }
}

function snapshotRow(
    row: SnapshotRow,
    size: string,
    display: string,
    now: number,
    light: boolean,
): PopupMenu.PopupBaseMenuItem {
    const item = new PopupMenu.PopupBaseMenuItem({
        activate: false,
        hover: false,
        can_focus: false,
        style_class: 'tmb-widget-row',
    });
    const content = new St.BoxLayout({
        orientation: Clutter.Orientation.VERTICAL,
        x_expand: true,
        style_class: 'tmb-widget-content',
    });
    const heading = new St.BoxLayout({x_expand: true, style_class: 'tmb-widget-heading'});
    const title = new St.Label({
        text: size === 'small' ? row.label : `${row.provider} · ${row.label}`,
        x_expand: true,
        style_class: 'tmb-widget-name',
    });
    const value = Math.round(display === 'remaining' ? 100 - row.used : row.used);
    const stale = row.stale || !row.updated || now - row.updated > 3600;
    const color = stale ? '#808080' : usageColor(row.used);
    const textColor = stale ? (light ? '#4b4b4b' : '#d8d8da') : usageTextColor(row.used, null, light);
    heading.add_child(title);
    heading.add_child(
        new St.Label({text: `${value}%`, style_class: 'tmb-widget-value', style: `color: ${textColor};`}),
    );
    content.add_child(heading);
    content.add_child(meter(292, row.used, color, {height: 4, track: 'tmb-widget-meter', fill: 'tmb-widget-fill'}));
    if (size !== 'small') {
        content.add_child(
            new St.Label({text: `${resetText(row.reset)}${stale ? ' · stale' : ''}`, style_class: 'tmb-widget-reset'}),
        );
    }
    item.add_child(content);
    item.accessible_name = `${title.text}, ${value}% ${display}, ${resetText(row.reset)}`;
    return item;
}

function resetText(stamp: string | null): string {
    // Date.parse(null) is NaN, which reads as an unavailable reset.
    return Number.isNaN(Date.parse(stamp as string)) ? COPY.resetUnavailable : until(stamp);
}
