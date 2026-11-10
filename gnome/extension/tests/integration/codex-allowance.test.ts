import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {clickButton, containsText, failure, findActor, indicator as getIndicator, required} from '../support/actors.js';
import {afterPaint, waitOrFail} from '../support/shell-events.js';

export async function run() {
    try {
        Main.overview.hide();
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        indicator.menu.open(false);
        const root = findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-root'));
        if (!root) throw new Error('Codex credit allowance is missing from the menu');
        await waitOrFail(
            () =>
                findActor(
                    root,
                    actor =>
                        actor.has_style_class_name?.('tmb-spend-summary') &&
                        containsText(actor, '2.125 credits of 10 credits'),
                ),
            'Codex credit allowance is missing from the menu',
        );
        const disclosure = findActor(root, actor => actor.has_style_class_name?.('tmb-disclosure'));
        if (!disclosure || !containsText(disclosure, 'Credit details'))
            throw new Error('Codex credit details are missing');
        clickButton(disclosure);
        await waitOrFail(
            () =>
                findActor(
                    root,
                    actor =>
                        actor.has_style_class_name?.('tmb-key-value') &&
                        containsText(actor, 'Remaining') &&
                        containsText(actor, '7.875 credits'),
                ),
            'Codex remaining credits are missing',
        );
        if (
            !findActor(
                root,
                actor =>
                    actor.has_style_class_name?.('tmb-key-value') &&
                    containsText(actor, 'Allowance source') &&
                    containsText(actor, 'Workspace Owner'),
            )
        )
            throw new Error('Codex allowance source is missing');
        await afterPaint();
        const directory = GLib.getenv('TMB_SCREENSHOT_DIR');
        if (directory) {
            GLib.mkdir_with_parents(directory, 0o755);
            const [left, top] = root.get_transformed_position();
            const x = Math.max(0, Math.floor(left) - 8);
            const y = Math.max(0, Math.floor(top) - 8);
            const width = Math.min(global.stage.width - x, Math.ceil(root.width) + 16);
            const height = Math.min(global.stage.height - y, Math.ceil(root.height) + 8);
            const stream = Gio.File.new_for_path(GLib.build_filenamev([directory, 'codex-allowance-menu.png'])).replace(
                null,
                false,
                Gio.FileCreateFlags.REPLACE_DESTINATION,
                null,
            );
            await new Promise<void>((resolve, reject) => {
                new Shell.Screenshot().screenshot_area(x, y, width, height, stream, (source, result) => {
                    try {
                        required(source).screenshot_area_finish(result);
                        stream.close(null);
                        resolve();
                    } catch (error) {
                        reject(failure(error));
                    }
                });
            });
        }
        clickButton(
            required(
                findActor(
                    root,
                    actor => actor.has_style_class_name?.('tmb-disclosure') && containsText(actor, 'Credit details'),
                ),
            ),
        );
        await waitOrFail(
            () =>
                !findActor(
                    root,
                    actor => actor.has_style_class_name?.('tmb-key-value') && containsText(actor, 'Remaining'),
                ),
            'Codex credit details did not collapse',
        );
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
