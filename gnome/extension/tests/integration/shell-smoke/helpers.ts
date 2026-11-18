import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import St from 'gi://St';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {closeInstantly, openInstantly} from '../../../src/menu-animation.js';
import type {Indicator} from '../../../src/panel.js';
import type {LogEntry, ServiceState, StatusCell} from '../../../src/service-state.js';
import {statusCells} from '../../../src/status-cells.js';
import type {TestActor} from '../../support/actors.js';
import {
    containsText,
    failure,
    findActor,
    findActors,
    indicator as getIndicator,
    required,
} from '../../support/actors.js';

export const UUID = 'quotaline@bernat.tech';

import {waitFor, waitOrFail} from '../../support/shell-events.js';

export {waitFor, waitOrFail};

export function afterPaint(action: () => void) {
    return new Promise<void>(resolve => {
        const signal = global.stage.connect('after-paint', () => {
            global.stage.disconnect(signal);
            resolve();
        });
        action();
        global.stage.queue_redraw();
    });
}

export function changedAfter(action: () => void) {
    return new Promise<void>((resolve, reject) => {
        const finish = (error: unknown) => {
            Gio.DBus.session.signal_unsubscribe(signal);
            if (error) reject(failure(error));
            else resolve();
        };
        const signal = Gio.DBus.session.signal_subscribe(
            'tech.bernat.Quotaline.Gnome',
            'tech.bernat.Quotaline.Gnome',
            'Changed',
            '/tech/bernat/Quotaline/Gnome',
            null,
            Gio.DBusSignalFlags.NONE,
            () => {
                finish(null);
            },
        );
        try {
            action();
        } catch (error) {
            finish(error);
        }
    });
}

export function serviceState() {
    const [state] = Gio.DBus.session
        .call_sync(
            'tech.bernat.Quotaline.Gnome',
            '/tech/bernat/Quotaline/Gnome',
            'tech.bernat.Quotaline.Gnome',
            'GetState',
            null,
            new GLib.VariantType('(s)'),
            Gio.DBusCallFlags.NONE,
            2000,
            null,
        )
        .deep_unpack() as [string];
    const parsed = JSON.parse(state) as ServiceState;
    return {...parsed, cells: statusCells(parsed)};
}

export function serviceSettings() {
    return serviceState().settings;
}

export function serviceLogs(full = false) {
    const [raw] = Gio.DBus.session
        .call_sync(
            'tech.bernat.Quotaline.Gnome',
            '/tech/bernat/Quotaline/Gnome',
            'tech.bernat.Quotaline.Gnome',
            'GetLogs',
            new GLib.Variant('(b)', [full]),
            new GLib.VariantType('(s)'),
            Gio.DBusCallFlags.NONE,
            2000,
            null,
        )
        .deep_unpack() as [string];
    return JSON.parse(raw) as LogEntry[];
}

export function setSetting(key: string, value: unknown) {
    return new Promise<void>((resolve, reject) => {
        Gio.DBus.session.call(
            'tech.bernat.Quotaline.Gnome',
            '/tech/bernat/Quotaline/Gnome',
            'tech.bernat.Quotaline.Gnome',
            'SetSetting',
            new GLib.Variant('(ss)', [key, JSON.stringify(value)]),
            new GLib.VariantType('(s)'),
            Gio.DBusCallFlags.NONE,
            12000,
            null,
            (connection, result) => {
                try {
                    required(connection).call_finish(result);
                    resolve();
                } catch (error) {
                    reject(failure(error));
                }
            },
        );
    });
}

export function checkMark(actor: TestActor | null | undefined) {
    return actor && (actor.checked ? '☑' : '☐');
}

export async function copiedTextAfter(
    action: () => void,
    ready: (value: string | undefined) => unknown = value => value !== undefined,
) {
    const clipboard = St.Clipboard.get_default();
    const setText = clipboard.set_text.bind(clipboard);
    let copied: string | undefined;
    clipboard.set_text = function (type, value) {
        if (type === St.ClipboardType.CLIPBOARD) {
            copied = value;
            global.stage.queue_redraw();
        }
        setText.call(this, type, value);
    };
    try {
        action();
        await waitOrFail(() => ready(copied), 'Clipboard action did not write its expected text');
        return required(copied);
    } finally {
        clipboard.set_text = setText;
    }
}

export async function capture(name: string) {
    const directory = GLib.getenv('TMB_SCREENSHOT_DIR');
    if (
        !directory ||
        (GLib.getenv('TMB_SCREENSHOT_ALL') !== '1' &&
            !['usage', 'history', 'settings', 'menubar-stacked'].includes(name))
    ) {
        await afterPaint(() => {});
        return;
    }
    GLib.mkdir_with_parents(directory, 0o755);
    const stream = Gio.File.new_for_path(GLib.build_filenamev([directory, `${name}.png`])).replace(
        null,
        false,
        Gio.FileCreateFlags.REPLACE_DESTINATION,
        null,
    );
    await new Promise<void>((resolve, reject) => {
        new Shell.Screenshot().screenshot(false, stream, (source, result) => {
            try {
                required(source).screenshot_finish(result);
                stream.close(null);
                resolve();
            } catch (error) {
                reject(failure(error));
            }
        });
    });
    const indicator = getIndicator();
    const root = name.startsWith('widget-')
        ? findActor(
              required(Main.panel.statusArea.quickSettings).menu.box,
              actor => actor.title === 'Quotaline' && actor.menu,
          )?.menu?.actor
        : indicator?.menu.isOpen
          ? findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-root'))
          : null;
    if (!root) return;
    const [left, top] = root.get_transformed_position();
    const x = Math.max(0, Math.floor(left));
    const y = Math.max(0, Math.floor(top));
    const width = Math.min(global.stage.width - x, Math.ceil(root.width));
    const height = Math.min(global.stage.height - y, Math.ceil(root.height));
    const menuStream = Gio.File.new_for_path(GLib.build_filenamev([directory, `${name}-menu.png`])).replace(
        null,
        false,
        Gio.FileCreateFlags.REPLACE_DESTINATION,
        null,
    );
    await new Promise<void>((resolve, reject) => {
        new Shell.Screenshot().screenshot_area(x, y, width, height, menuStream, (source, result) => {
            try {
                required(source).screenshot_area_finish(result);
                menuStream.close(null);
                resolve();
            } catch (error) {
                reject(failure(error));
            }
        });
    });
}

export async function captureStatus(name: string) {
    const directory = GLib.getenv('TMB_SCREENSHOT_DIR');
    if (!directory) return;
    GLib.mkdir_with_parents(directory, 0o755);
    const indicator = required(getIndicator());
    const wasOpen = indicator.menu.isOpen;
    if (wasOpen) {
        closeInstantly(indicator.menu);
        indicator.set_hover(false);
        await afterPaint(() => {});
    }
    try {
        const [indicatorX] = indicator.get_transformed_position();
        const stream = Gio.File.new_for_path(GLib.build_filenamev([directory, `${name}.png`])).replace(
            null,
            false,
            Gio.FileCreateFlags.REPLACE_DESTINATION,
            null,
        );
        await new Promise<void>((resolve, reject) => {
            new Shell.Screenshot().screenshot_area(
                Math.max(0, Math.round(indicatorX + indicator.width - 520)),
                0,
                520,
                32,
                stream,
                (source, result) => {
                    try {
                        required(source).screenshot_area_finish(result);
                        stream.close(null);
                        resolve();
                    } catch (error) {
                        reject(failure(error));
                    }
                },
            );
        });
    } finally {
        if (wasOpen) openInstantly(indicator.menu);
    }
}

export {containsText, findActor, findActors};

export interface SmokeContext {
    indicator: Indicator;
    root: TestActor;
    footer: TestActor;
    panelCells: TestActor;
    adjustment: St.Adjustment;
    source: string;
    demoPhase: string | null;
    demoState: (ServiceState & {cells: StatusCell[]}) | null;
    openWithFastClock: () => void;
    press: (symbol: number, control?: boolean) => Promise<void>;
    usageColor: typeof import('../../../src/color.js').usageColor;
    usageStatusColor: typeof import('../../../src/color.js').usageStatusColor;
    usageTextColor: typeof import('../../../src/color.js').usageTextColor;
}
