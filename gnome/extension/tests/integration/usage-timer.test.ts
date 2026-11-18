import GLib from 'gi://GLib';
import {closeInstantly, openInstantly} from '../../src/menu-animation.js';
import {accessibleName, findActor, indicator as getIndicator, required} from '../support/actors.js';
import {hideOverview, waitOrFail} from '../support/shell-events.js';

export async function run() {
    const original = GLib.timeout_add_seconds;
    const originalAge = GLib.timeout_add;
    try {
        await hideOverview();
        const indicator = required(getIndicator());
        await waitOrFail(() => accessibleName(indicator)?.includes('CC 5h'), 'The extension did not load demo usage');
        let tick: GLib.SourceFunc | undefined;
        let ageTick: {id: number; callback: GLib.SourceFunc} | undefined;
        GLib.timeout_add_seconds = (priority, interval, callback) => {
            if (interval === 60) tick = callback;
            return original(priority, interval, callback);
        };
        GLib.timeout_add = (priority, interval, callback) => {
            const id = originalAge(priority, interval, callback);
            if (!ageTick) ageTick = {id, callback};
            return id;
        };
        try {
            openInstantly(indicator.menu);
        } finally {
            GLib.timeout_add_seconds = original;
            GLib.timeout_add = originalAge;
        }
        const card = () => findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-card'));
        await waitOrFail(() => card(), 'The Usage page or its timers did not load');
        if (!tick || !ageTick) throw new Error('The Usage page or its timers did not load');
        const before = card();
        if (tick(null) !== GLib.SOURCE_CONTINUE)
            throw new Error('The Usage minute timer did not reload the visible cards');
        await waitOrFail(() => card() !== before, 'The Usage minute timer did not reload the visible cards');
        const updated = required(
            findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-header-updated')),
        );
        updated.text = 'Outdated';
        if (ageTick.callback(null) !== GLib.SOURCE_REMOVE)
            throw new Error('The header age timer did not fetch the current visible age');
        await waitOrFail(
            () => updated.text !== 'Outdated',
            'The header age timer did not fetch the current visible age',
        );
        closeInstantly(indicator.menu);
    } finally {
        GLib.timeout_add_seconds = original;
        GLib.timeout_add = originalAge;
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
