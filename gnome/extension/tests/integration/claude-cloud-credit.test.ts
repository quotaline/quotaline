import GLib from 'gi://GLib';
import St from 'gi://St';
import {closeInstantly, openInstantly} from '../../src/menu-animation.js';
import {findActor, indicator as getIndicator} from '../support/actors.js';
import {waitOrFail} from '../support/shell-events.js';

export async function run() {
    try {
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        openInstantly(indicator.menu);
        const section = () => findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-cloud-credits'));
        await waitOrFail(() => section(), 'Claude cloud credits did not reach Usage');
        const summary = findActor(section(), actor => actor.has_style_class_name?.('tmb-spend-summary'))?.text;
        const expected = GLib.getenv('TMB_CLOUD_CASE');
        if (
            !summary?.includes('$83.00 of $250.00 left') ||
            (expected === 'expired' && !summary.includes('Expired')) ||
            (expected === 'locked' && !summary.includes('Unavailable')) ||
            (expected === 'stale' && (summary.includes('Expires') || summary.includes('Expired')))
        )
            throw new Error(`Claude cloud credit ${expected} summary is wrong: ${summary}`);
        const fill = findActor(section(), actor => actor.has_style_class_name?.('tmb-meter-fill'), St.Widget);
        if (!fill?.get_style()?.includes('#808080'))
            throw new Error(`Unavailable cloud credit meter is not muted: ${fill?.get_style()}`);
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
