import GLib from 'gi://GLib';
import {closeInstantly, openInstantly} from '../../src/menu-animation.js';
import type {TestActor} from '../support/actors.js';
import {clickButton, findActor, indicator as getIndicator, required} from '../support/actors.js';
import {hideOverview, waitOrFail} from '../support/shell-events.js';

function waitFor(check: () => unknown) {
    return waitOrFail(check, 'Metric picker did not reach the menu');
}

export async function run() {
    try {
        await hideOverview();
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        openInstantly(indicator.menu);
        await waitFor(() => findActor(indicator.menu.box, actor => actor.accessible_name === 'History'));
        clickButton(required(findActor(indicator.menu.box, actor => actor.accessible_name === 'History')));
        await waitFor(() => findActor(indicator.menu.box, actor => actor.accessible_name === 'Metric'));
        clickButton(required(findActor(indicator.menu.box, actor => actor.accessible_name === 'Metric')));
        await waitFor(() => findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-metric-options')));
        const picker = findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-metric-options'));
        const groups: (string | undefined)[] = [];
        const collect = (actor: TestActor) => {
            if (actor.has_style_class_name?.('tmb-metric-group')) groups.push(actor.text);
            for (const child of actor.get_children()) collect(child);
        };
        collect(required(picker));
        if (
            groups.length !== 1 ||
            groups[0] !== 'Windows' ||
            !findActor(picker, actor => actor.accessible_name === 'Choose metric: usage')
        )
            throw new Error(`Unavailable analytics metrics remained in the picker: ${JSON.stringify(groups)}`);
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
