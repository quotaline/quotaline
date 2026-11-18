import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import St from 'gi://St';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {failure, findActor, indicator as getIndicator, instance, required} from '../support/actors.js';
import {disableExtension} from '../support/extension.js';
import {afterPaint, hideOverview, waitOrFail} from '../support/shell-events.js';

function call(method: string) {
    return new Promise<void>((resolve, reject) => {
        Gio.DBus.session.call(
            'tech.bernat.Quotaline.Gnome',
            '/tech/bernat/Quotaline/Gnome',
            'tech.bernat.Quotaline.Gnome',
            method,
            null,
            null,
            Gio.DBusCallFlags.NONE,
            5000,
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

export async function run() {
    try {
        await hideOverview();
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        await call('WaitForStateCall');
        instance(indicator, St.Widget).set_hover(true);
        await waitOrFail(
            () =>
                findActor(
                    Main.layoutManager.uiGroup,
                    actor =>
                        actor.has_style_class_name?.('tmb-tooltip') &&
                        actor.visible &&
                        findActor(actor, child => child.text === 'No usage yet'),
                ),
            'The panel item did not explain its pending first reading',
        );
        await disableExtension('Disabling with a pending state reply left the panel item behind');
        await call('ReleaseState');
        await afterPaint();
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
