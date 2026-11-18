import GLib from 'gi://GLib';
import {closeInstantly, openInstantly} from '../../src/menu-animation.js';
import {findActor, indicator as getIndicator, required} from '../support/actors.js';
import {readProvider} from '../support/dbus.js';
import {hideOverview, waitOrFail} from '../support/shell-events.js';

function waitFor(check: () => unknown) {
    return waitOrFail(check, 'Remaining values did not appear in Usage');
}

export async function run() {
    try {
        await hideOverview();
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        const remaining = `${Math.round(100 - required(readProvider('claude').windows[0]).used)}%`;
        openInstantly(indicator.menu);
        await waitFor(
            () =>
                findActor(indicator.menu.box, actor => actor.text === 'Current session') &&
                findActor(
                    indicator.menu.box,
                    actor => actor.has_style_class_name?.('tmb-quota-value') && actor.text === remaining,
                ),
        );
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
