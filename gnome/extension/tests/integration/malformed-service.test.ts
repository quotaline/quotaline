import GLib from 'gi://GLib';
import {closeInstantly, openInstantly} from '../../src/menu-animation.js';
import {containsText, indicator as getIndicator} from '../support/actors.js';
import {hideOverview, waitOrFail} from '../support/shell-events.js';

function waitFor(check: () => unknown) {
    return waitOrFail(check, 'Invalid service reply did not reach the menu');
}

export async function run() {
    try {
        await hideOverview();
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        openInstantly(indicator.menu);
        await waitFor(
            () =>
                containsText(indicator.menu.box, 'Invalid service response') &&
                containsText(indicator.menu.box, 'Connecting to the local service'),
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
