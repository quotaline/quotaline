import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {buttonLabel, clickButton, findActor, indicator as getIndicator, required} from '../support/actors.js';
import {waitOrFail} from '../support/shell-events.js';

function waitFor(check: () => unknown) {
    return waitOrFail(check, 'Save dialog control did not reach Settings');
}

export async function run() {
    try {
        Main.overview.hide();
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        indicator.menu.open(false);
        await waitFor(() => findActor(indicator.menu.box, actor => actor.accessible_name === 'Settings'));
        clickButton(required(findActor(indicator.menu.box, actor => actor.accessible_name === 'Settings')));
        await waitFor(() => findActor(indicator.menu.box, actor => buttonLabel(actor) === 'Export all history CSV…'));
        await new Promise<void>((resolve, reject) => {
            const signal = Gio.DBus.session.signal_subscribe(
                'org.freedesktop.portal.Desktop',
                'org.freedesktop.portal.Request',
                'Response',
                null,
                null,
                Gio.DBusSignalFlags.NONE,
                (_connection, _sender, _path, _interface, _name, parameters) => {
                    Gio.DBus.session.signal_unsubscribe(signal);
                    if ((parameters.deep_unpack() as [number])[0] === 1) resolve();
                    else reject(new Error('Save dialog did not cancel'));
                },
            );
            clickButton(
                required(findActor(indicator.menu.box, actor => buttonLabel(actor) === 'Export all history CSV…')),
            );
        });
        await new Promise<void>(resolve =>
            GLib.idle_add(GLib.PRIORITY_DEFAULT_IDLE, () => {
                resolve();
                return GLib.SOURCE_REMOVE;
            }),
        );
        if (
            GLib.file_test(required(GLib.getenv('TMB_EXPORT_PATH')), GLib.FileTest.EXISTS) ||
            findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-notice'))
        )
            throw new Error('Cancelled save dialog changed the menu or wrote a file');
        indicator.menu.close(false);
    } finally {
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
