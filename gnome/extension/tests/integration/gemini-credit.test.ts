import GLib from 'gi://GLib';
import {closeInstantly, openInstantly} from '../../src/menu-animation.js';
import {findActor, indicator as getIndicator} from '../support/actors.js';
import {waitOrFail} from '../support/shell-events.js';

export async function run() {
    try {
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        openInstantly(indicator.menu);
        await waitOrFail(
            () =>
                findActor(
                    indicator.menu.box,
                    actor => actor.has_style_class_name?.('tmb-card-meta') && actor.text === 'Credits $2.50 remaining',
                ),
            'Gemini credit balance is missing from Usage',
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
