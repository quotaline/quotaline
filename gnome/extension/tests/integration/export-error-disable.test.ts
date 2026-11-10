import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import {ExtensionState} from 'resource:///org/gnome/shell/misc/extensionUtils.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {
    accessibleName,
    buttonLabel,
    clickButton,
    findActor,
    indicator as getIndicator,
    required,
} from '../support/actors.js';
import {afterPaint, waitOrFail} from '../support/shell-events.js';

const UUID = 'quotaline@bernat.tech';

export async function run() {
    const subscribe = Gio.DBus.session.signal_subscribe.bind(Gio.DBus.session);
    try {
        Main.overview.hide();
        await waitOrFail(() => accessibleName(getIndicator())?.includes('CC 5h'), 'The extension did not load');
        const indicator = required(getIndicator());
        indicator.menu.open(false);
        await waitOrFail(
            () => findActor(indicator.menu.box, actor => actor.accessible_name === 'Settings'),
            'Settings did not load',
        );
        clickButton(required(findActor(indicator.menu.box, actor => actor.accessible_name === 'Settings')));
        await waitOrFail(
            () => findActor(indicator.menu.box, actor => buttonLabel(actor) === 'Export all history CSV…'),
            'The export control did not load',
        );
        await afterPaint();
        const response = {rejected: false};
        Gio.DBus.session.signal_subscribe = function (busName, interfaceName, signalName, ...args) {
            if (
                busName === 'org.freedesktop.portal.Desktop' &&
                interfaceName === 'org.freedesktop.portal.Request' &&
                signalName === 'Response'
            ) {
                response.rejected = true;
                throw new Error('Fixture portal subscription failed');
            }
            return subscribe.call(this, busName, interfaceName, signalName, ...args);
        };
        clickButton(required(findActor(indicator.menu.box, actor => buttonLabel(actor) === 'Export all history CSV…')));
        if (!response.rejected) throw new Error('The save dialog did not subscribe to its response');
        if (!Main.extensionManager.disableExtension(UUID))
            throw new Error('The extension did not disable after the save dialog failed');
        await waitOrFail(
            () => Main.extensionManager.lookup(UUID).state === ExtensionState.INACTIVE && !getIndicator(),
            'The failed save dialog kept the panel item installed',
        );
    } finally {
        Gio.DBus.session.signal_subscribe = subscribe;
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
