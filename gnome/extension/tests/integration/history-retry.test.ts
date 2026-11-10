import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import {
    accessibleName,
    buttonLabel,
    clickButton,
    containsText,
    failure,
    findActor,
    indicator as getIndicator,
    required,
} from '../support/actors.js';
import {waitOrFail} from '../support/shell-events.js';

export async function run() {
    try {
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        indicator.menu.open(false);
        clickButton(required(findActor(indicator.menu.box, actor => actor.accessible_name === 'History')));
        await waitOrFail(() => containsText(indicator.menu.box, '32%'), 'Initial History chart did not load');
        clickButton(required(findActor(indicator.menu.box, actor => buttonLabel(actor) === 'Now')));
        await waitOrFail(
            () =>
                containsText(indicator.menu.box, 'Update failed:') &&
                containsText(indicator.menu.box, 'History request failed'),
            'Failed History update did not explain the error',
        );
        if (
            !containsText(indicator.menu.box, '32%') ||
            !findActor(indicator.menu.box, actor => accessibleName(actor)?.startsWith('History chart with 1 series'))
        )
            throw new Error('Failed History update discarded the previous chart');
        const directory = GLib.getenv('TMB_SCREENSHOT_DIR');
        if (directory) {
            GLib.mkdir_with_parents(directory, 0o755);
            const stream = Gio.File.new_for_path(
                GLib.build_filenamev([directory, 'history-update-failed.png']),
            ).replace(null, false, Gio.FileCreateFlags.REPLACE_DESTINATION, null);
            await new Promise<void>((resolve, reject) => {
                new Shell.Screenshot().screenshot(false, stream, (source, result) => {
                    try {
                        required(source).screenshot_finish(result);
                        stream.close(null);
                        resolve();
                    } catch (error) {
                        reject(failure(error));
                    }
                });
            });
        }
        clickButton(required(findActor(indicator.menu.box, actor => buttonLabel(actor) === 'Retry')));
        await waitOrFail(() => containsText(indicator.menu.box, '52%'), 'Retry did not refresh the History chart');
        if (containsText(indicator.menu.box, 'Update failed'))
            throw new Error('Retry did not refresh the History chart');
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
