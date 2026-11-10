import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {
    accessibleName,
    buttonLabel,
    clickButton,
    containsText,
    findActor,
    indicator as getIndicator,
    required,
} from '../support/actors.js';
import {proxyCalls, proxyFactory} from '../support/dbus.js';
import {waitOrFail} from '../support/shell-events.js';

const UUID = 'quotaline@bernat.tech';

export async function run() {
    const originalCreate = proxyFactory.new_for_bus;
    const original = Gio.DBusProxy.new_for_bus_finish.bind(Gio.DBusProxy);
    const originalConnect = proxyCalls.connect;
    const originalCall = proxyCalls.call;
    const originalLaunch = Gio.AppInfo.launch_default_for_uri.bind(Gio.AppInfo);
    try {
        Main.overview.hide();
        await waitOrFail(() => accessibleName(getIndicator())?.includes('CC 5h'), 'The initial extension did not load');
        let pendingProxy: (() => void) | null = null;
        proxyFactory.new_for_bus = function (...args) {
            if (args[5] === 'tech.bernat.Quotaline.Gnome') {
                const callback = args[7];
                args[7] = (...values) => {
                    pendingProxy = () => {
                        callback(...values);
                    };
                    global.stage.queue_redraw();
                };
            }
            originalCreate.apply(this, args);
        };
        if (!Main.extensionManager.disableExtension(UUID) || !Main.extensionManager.enableExtension(UUID))
            throw new Error('The extension did not restart for a delayed proxy');
        await waitOrFail(() => pendingProxy, 'The proxy callback did not arrive');
        if (!Main.extensionManager.disableExtension(UUID) || getIndicator())
            throw new Error('The delayed proxy left its panel item installed');
        required<() => void>(pendingProxy)();
        proxyFactory.new_for_bus = originalCreate;
        if (!Main.extensionManager.enableExtension(UUID))
            throw new Error('The extension did not recover after a delayed proxy');
        await waitOrFail(
            () => accessibleName(getIndicator())?.includes('CC 5h'),
            'The extension did not recover after a delayed proxy',
        );
        if (!Main.extensionManager.disableExtension(UUID)) throw new Error('The extension did not disable');
        Gio.DBusProxy.new_for_bus_finish = () => {
            throw new Error('Fixture proxy initialization failed');
        };
        if (!Main.extensionManager.enableExtension(UUID)) throw new Error('The extension did not enable');
        const indicator = getIndicator();
        if (!indicator) throw new Error('The extension did not restore its panel item');
        indicator.menu.open(false);
        await waitOrFail(
            () => containsText(indicator.menu.box, 'Fixture proxy initialization failed'),
            'The proxy setup failure was not explained in the menu',
        );
        let draft = '';
        Gio.AppInfo.launch_default_for_uri = uri => {
            draft = uri;
            global.stage.queue_redraw();
            return true;
        };
        clickButton(required(findActor(indicator.menu.box, actor => buttonLabel(actor) === 'Report issue')));
        await waitOrFail(
            () => draft.includes('/issues/new?'),
            'Report issue did not open an offline draft after proxy failure',
        );
        if (!decodeURIComponent(draft).includes('Quotaline service unavailable.'))
            throw new Error('Report issue did not open an offline draft after proxy failure');
        Gio.AppInfo.launch_default_for_uri = originalLaunch;
        indicator.menu.close(false);
        if (!Main.extensionManager.disableExtension(UUID) || getIndicator())
            throw new Error('The failed proxy left its panel item behind after disable');
        Gio.DBusProxy.new_for_bus_finish = original;
        proxyCalls.connect = function (this: Gio.DBusProxy, signal, callback) {
            if (signal === 'g-signal' && this.g_interface_name === 'tech.bernat.Quotaline.Gnome')
                throw new Error('Fixture signal connection failed');
            return originalConnect.call(this, signal, callback);
        };
        if (!Main.extensionManager.enableExtension(UUID))
            throw new Error('The extension did not enable after the proxy setup failure');
        const partial = required(getIndicator());
        partial.menu.open(false);
        await waitOrFail(
            () => containsText(partial.menu.box, 'Fixture signal connection failed'),
            'The proxy signal failure was not explained in the menu',
        );
        partial.menu.close(false);
        if (!Main.extensionManager.disableExtension(UUID) || getIndicator())
            throw new Error('The partial proxy setup left its panel item behind');
        proxyCalls.connect = originalConnect;
        let pendingPanelCall: (() => void) | null = null;
        proxyCalls.call = function (method, ...args) {
            if (method === 'SetPanelOpen' && this.g_interface_name === 'tech.bernat.Quotaline.Gnome') {
                pendingPanelCall = () => {
                    args[4](this, Gio.Task.new(this, null, null));
                };
                return;
            }
            originalCall.call(this, method, ...args);
        };
        if (!Main.extensionManager.enableExtension(UUID))
            throw new Error('The extension did not enable for the pending panel call');
        await waitOrFail(
            () => accessibleName(getIndicator())?.includes('CC 5h'),
            'The extension did not load before its pending panel call',
        );
        if (!Main.extensionManager.disableExtension(UUID) || getIndicator())
            throw new Error('The pending panel call kept the panel item installed');
        required<() => void>(pendingPanelCall)();
    } finally {
        proxyFactory.new_for_bus = originalCreate;
        Gio.DBusProxy.new_for_bus_finish = original;
        proxyCalls.connect = originalConnect;
        proxyCalls.call = originalCall;
        Gio.AppInfo.launch_default_for_uri = originalLaunch;
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
