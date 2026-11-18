import GLib from 'gi://GLib';
import {closeInstantly, openInstantly} from '../../src/menu-animation.js';
import {findActor, indicator as getIndicator} from '../support/actors.js';
import {hideOverview, waitOrFail} from '../support/shell-events.js';

function waitFor(check: () => unknown) {
    return waitOrFail(check, 'Warning notice did not appear');
}

export async function run() {
    try {
        await hideOverview();
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        openInstantly(indicator.menu);
        await waitFor(() =>
            findActor(
                indicator.menu.box,
                actor =>
                    actor.has_style_class_name?.('tmb-provider-notice-warning') &&
                    findActor(actor, child => child.text === 'Current session limit reached'),
            ),
        );
        await waitFor(() =>
            findActor(
                indicator.menu.box,
                actor =>
                    actor.accessible_name === 'Warning: Profile unavailable: HTTP 503' &&
                    actor.has_style_class_name?.('tmb-provider-notice-warning'),
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
