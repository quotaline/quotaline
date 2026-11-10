import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import type {Indicator} from '../../src/panel.js';
import {
    accessibleName,
    buttonLabel,
    clickButton,
    containsText,
    failure,
    findActor,
    indicator as getIndicator,
    required,
} from '../support/actors.js';
import {proxyCalls} from '../support/dbus.js';
import {waitOrFail} from '../support/shell-events.js';

async function captureMenu(name: string, indicator: Indicator) {
    const directory = GLib.getenv('TMB_SCREENSHOT_DIR');
    if (!directory) return;
    const root = findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-root'));
    if (!root) throw new Error('The service menu was not available for capture');
    GLib.mkdir_with_parents(directory, 0o755);
    const full = Gio.File.new_for_path(GLib.build_filenamev([directory, `${name}.png`])).replace(
        null,
        false,
        Gio.FileCreateFlags.REPLACE_DESTINATION,
        null,
    );
    await new Promise<void>((resolve, reject) => {
        new Shell.Screenshot().screenshot(false, full, (source, result) => {
            try {
                required(source).screenshot_finish(result);
                full.close(null);
                resolve();
            } catch (error) {
                reject(failure(error));
            }
        });
    });
    const [left, top] = root.get_transformed_position();
    const x = Math.max(0, Math.floor(left) - 8);
    const y = Math.max(0, Math.floor(top) - 8);
    const width = Math.min(global.stage.width - x, Math.ceil(root.width) + 16);
    const height = Math.min(global.stage.height - y, Math.ceil(root.height) + 16);
    const stream = Gio.File.new_for_path(GLib.build_filenamev([directory, `${name}-menu.png`])).replace(
        null,
        false,
        Gio.FileCreateFlags.REPLACE_DESTINATION,
        null,
    );
    await new Promise<void>((resolve, reject) => {
        new Shell.Screenshot().screenshot_area(x, y, width, height, stream, (source, result) => {
            try {
                required(source).screenshot_area_finish(result);
                stream.close(null);
                resolve();
            } catch (error) {
                reject(failure(error));
            }
        });
    });
}

export async function run() {
    const call = proxyCalls.call;
    const launchUri = Gio.AppInfo.launch_default_for_uri.bind(Gio.AppInfo);
    try {
        Main.overview.hide();
        await waitOrFail(
            () => accessibleName(getIndicator())?.includes('CC 5h'),
            'The extension did not load before service restart',
        );
        const indicator = required(getIndicator());
        let stateRequests = 0;
        let refreshRequests = 0;
        let lateCosts: (() => void) | null = null;
        proxyCalls.call = function (method, ...args) {
            if (method === 'GetState' && this.g_interface_name === 'tech.bernat.Quotaline.Gnome') stateRequests++;
            if (method === 'Refresh' && this.g_interface_name === 'tech.bernat.Quotaline.Gnome') refreshRequests++;
            if (
                method === 'GetSpendSummary' &&
                this.g_interface_name === 'tech.bernat.Quotaline.Gnome' &&
                lateCosts === null
            ) {
                const reply = args[4];
                lateCosts = () => {};
                args[4] = (...result) => {
                    lateCosts = () => {
                        reply(...result);
                    };
                };
            }
            call.call(this, method, ...args);
        };
        indicator.menu.open(false);
        await waitOrFail(
            () => containsText(indicator.menu.box, 'Current session'),
            'The initial provider state did not load',
        );
        GLib.file_set_contents(required(GLib.getenv('TMB_SERVICE_READY')), '');
        await waitOrFail(
            () => containsText(indicator.menu.box, 'Connecting to the local service'),
            'Service shutdown did not clear the provider state',
        );
        // A cost reply that lands after the service is gone must not read the cleared provider state.
        required<() => void>(lateCosts)();
        await captureMenu('service-disconnected', indicator);
        let draft = '';
        Gio.AppInfo.launch_default_for_uri = uri => {
            draft = uri;
            global.stage.queue_redraw();
            return true;
        };
        clickButton(required(findActor(indicator.menu.box, actor => buttonLabel(actor) === 'Report issue')));
        await waitOrFail(
            () => draft.includes('/issues/new?'),
            'Report issue did not open an offline draft after service loss',
        );
        if (!decodeURIComponent(draft).includes('Quotaline service unavailable.'))
            throw new Error('Report issue did not open an offline draft after service loss');
        Gio.AppInfo.launch_default_for_uri = launchUri;
        const refresh = findActor(indicator.menu.box, actor => buttonLabel(actor)?.includes('Refresh'));
        if (!refresh) throw new Error('The disconnected menu has no refresh action');
        clickButton(refresh);
        await waitOrFail(
            () => refreshRequests > 0 && containsText(indicator.menu.box, 'ServiceUnknown'),
            'Refresh did not explain the disconnected service',
        );
        const beforeReopen = stateRequests;
        indicator.menu.close(false);
        indicator.menu.open(false);
        await waitOrFail(
            () => stateRequests > beforeReopen && containsText(indicator.menu.box, 'ServiceUnknown'),
            'Reopening the menu did not preserve the disconnected state',
        );
        const beforeRecovery = stateRequests;
        GLib.file_set_contents(required(GLib.getenv('TMB_SERVICE_LOST')), '');
        await waitOrFail(
            () =>
                stateRequests > beforeRecovery &&
                containsText(indicator.menu.box, 'Current session') &&
                indicator.accessible_name.includes('CC 5h'),
            `Service recovery did not reload provider state: ${beforeRecovery} → ${stateRequests}`,
        );
        const menuRoot = required(findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-root')));
        await waitOrFail(() => {
            const [indicatorLeft] = indicator.get_transformed_position();
            const [left] = menuRoot.get_transformed_position();
            return (
                indicatorLeft + indicator.width <= global.stage.width &&
                menuRoot.width >= 500 &&
                left >= 0 &&
                left + menuRoot.width <= global.stage.width
            );
        }, 'The wider provider panel pushed the open menu off screen');
        await captureMenu('service-reconnected', indicator);
        indicator.menu.close(false);
    } finally {
        proxyCalls.call = call;
        Gio.AppInfo.launch_default_for_uri = launchUri;
        const directory = GLib.getenv('TMB_JS_COVERAGE_DIR');
        if (directory) {
            GLib.mkdir_with_parents(directory, 0o755);
            GLib.file_set_contents(
                GLib.build_filenamev([directory, `${GLib.uuid_string_random()}.json`]),
                JSON.stringify(globalThis.__coverage__ ?? {}),
            );
        }
    }
}

export function finish() {}
