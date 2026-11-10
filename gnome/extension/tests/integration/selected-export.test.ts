import GLib from 'gi://GLib';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {buttonLabel, clickButton, findActor, indicator as getIndicator, required} from '../support/actors.js';
import {waitOrFail} from '../support/shell-events.js';

function waitFor(check: () => unknown) {
    return waitOrFail(check, 'Selected period export did not finish');
}

export async function run() {
    try {
        Main.overview.hide();
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        indicator.menu.open(false);
        await waitFor(() => findActor(indicator.menu.box, actor => actor.accessible_name === 'History'));
        clickButton(required(findActor(indicator.menu.box, actor => actor.accessible_name === 'History')));
        await waitFor(() =>
            findActor(indicator.menu.box, actor => buttonLabel(actor) === 'Export CSV' && actor.reactive),
        );
        const path = GLib.getenv('TMB_EXPORT_PATH');
        if (!path || GLib.file_test(path, GLib.FileTest.EXISTS))
            throw new Error('Selected period export path is not empty');
        clickButton(required(findActor(indicator.menu.box, actor => buttonLabel(actor) === 'Export CSV')));
        await waitFor(() => findActor(indicator.menu.box, actor => actor.text === `Exported ${path}`));
        const [loaded, bytes] = GLib.file_get_contents(path);
        if (
            !loaded ||
            bytes.length <= 80 ||
            !String.fromCharCode(...bytes.slice(0, 80)).startsWith('timestamp,key,label,used_percent,resets_at')
        )
            throw new Error('Selected period export did not contain usage history');
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
