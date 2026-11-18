import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import {closeInstantly, openInstantly} from '../../src/menu-animation.js';
import {
    accessibleName,
    buttonLabel,
    clickButton,
    failure,
    findActor,
    indicator as getIndicator,
    required,
} from '../support/actors.js';
import {hideOverview, waitOrFail} from '../support/shell-events.js';

function enableActivity() {
    return new Promise<void>((resolve, reject) => {
        Gio.DBus.session.call(
            'tech.bernat.Quotaline.Gnome',
            '/tech/bernat/Quotaline/Gnome',
            'tech.bernat.Quotaline.Gnome',
            'EnableActivity',
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
        openInstantly(indicator.menu);
        await waitOrFail(
            () => findActor(indicator.menu.box, actor => actor.accessible_name === 'Settings'),
            'Settings tab did not appear',
        );
        clickButton(required(findActor(indicator.menu.box, actor => actor.accessible_name === 'Settings')));
        await waitOrFail(
            () => findActor(indicator.menu.box, actor => actor.text?.startsWith('Model list unavailable:')),
            'Model request failure did not appear in Settings',
        );
        const retry = findActor(indicator.menu.box, actor => buttonLabel(actor) === 'Retry model list');
        if (!retry) throw new Error('Model list retry is missing');
        await enableActivity();
        clickButton(retry);
        await waitOrFail(
            () => findActor(indicator.menu.box, actor => accessibleName(actor)?.startsWith('Show Claude ')),
            'Model list did not recover after retry',
        );
        if (findActor(indicator.menu.box, actor => actor.text?.startsWith('Model list unavailable:')))
            throw new Error('Model request failure remained after retry');
        closeInstantly(indicator.menu);
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
