import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {clickButton, failure, findActor, indicator as getIndicator, required} from '../support/actors.js';
import {waitOrFail} from '../support/shell-events.js';

function waitFor(check: () => unknown) {
    return waitOrFail(check, 'Malformed log response did not reach Settings');
}

function logRequestCount() {
    return new Promise<number>((resolve, reject) => {
        Gio.DBus.session.call(
            'tech.bernat.Quotaline.Gnome',
            '/tech/bernat/Quotaline/Gnome',
            'tech.bernat.Quotaline.Gnome',
            'GetLogRequestCount',
            null,
            new GLib.VariantType('(t)'),
            Gio.DBusCallFlags.NONE,
            5000,
            null,
            (connection, result) => {
                try {
                    resolve((required(connection).call_finish(result).deep_unpack() as [number])[0]);
                } catch (error) {
                    reject(failure(error));
                }
            },
        );
    });
}

export async function run() {
    try {
        Main.overview.hide();
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        indicator.menu.open(false);
        await waitFor(() => findActor(indicator.menu.box, actor => actor.accessible_name === 'Settings'));
        clickButton(required(findActor(indicator.menu.box, actor => actor.accessible_name === 'Settings')));
        await waitFor(() =>
            findActor(
                indicator.menu.box,
                actor => actor.has_style_class_name?.('tmb-notice') && !actor.text?.includes('Cost history error'),
            ),
        );
        await new Promise<void>(resolve => {
            const signal = global.stage.connect('after-paint', () => {
                global.stage.disconnect(signal);
                resolve();
            });
            global.stage.queue_redraw();
        });
        if ((await logRequestCount()) !== 1) throw new Error('Malformed log response started another fetch');
        if (!findActor(indicator.menu.box, actor => actor.text === 'No matching log entries'))
            throw new Error('Malformed log response left the log loading');
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
