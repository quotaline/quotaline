import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import {closeInstantly, openInstantly} from '../../src/menu-animation.js';
import {failure, findActor, indicator as getIndicator, required} from '../support/actors.js';
import {afterPaint, waitOrFail} from '../support/shell-events.js';

export async function run() {
    try {
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        openInstantly(indicator.menu);
        const root = findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-root'));
        const reset = () => findActor(root, actor => actor.has_style_class_name?.('tmb-quota-reset'));
        if (!root) throw new Error('Copilot quota reset did not reach Usage');
        await waitOrFail(() => reset()?.text?.includes('Resets in'), 'Copilot quota reset did not reach Usage');
        const day = GLib.getenv('TMB_RESET_DAY');
        const expected = new Date(`${day}T00:00:00Z`).toLocaleDateString([], {
            timeZone: 'UTC',
            month: 'short',
            day: 'numeric',
        });
        if (!required(required(reset()).text).endsWith(`\n${expected}`))
            throw new Error(`Copilot reset used a local clock time: ${required(reset()).text}`);
        await waitOrFail(
            () => root.get_transformed_position()[0] > 0 && root.width > 100 && root.height > 100,
            'Copilot Usage menu was not allocated',
        );
        await afterPaint();
        const directory = GLib.getenv('TMB_SCREENSHOT_DIR');
        if (directory) {
            GLib.mkdir_with_parents(directory, 0o755);
            const [left, top] = root.get_transformed_position();
            const stream = Gio.File.new_for_path(
                GLib.build_filenamev([directory, 'copilot-day-reset-menu.png']),
            ).replace(null, false, Gio.FileCreateFlags.REPLACE_DESTINATION, null);
            await new Promise<void>((resolve, reject) => {
                new Shell.Screenshot().screenshot_area(
                    Math.floor(left),
                    Math.floor(top),
                    Math.ceil(root.width),
                    Math.ceil(root.height),
                    stream,
                    (source, result) => {
                        try {
                            required(source).screenshot_area_finish(result);
                            stream.close(null);
                            resolve();
                        } catch (error) {
                            reject(failure(error));
                        }
                    },
                );
            });
        }
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
