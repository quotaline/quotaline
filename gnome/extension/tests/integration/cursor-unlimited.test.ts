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
        const card = () => findActor(root, actor => actor.has_style_class_name?.('tmb-card'));
        if (!root) throw new Error('Unlimited Cursor card did not reach Usage');
        await waitOrFail(() => card(), 'Unlimited Cursor card did not reach Usage');
        const notice = findActor(card(), actor => actor.has_style_class_name?.('tmb-provider-notice'));
        if (
            !notice ||
            !findActor(notice, actor => actor.text === 'This plan has unlimited usage.') ||
            findActor(card(), actor => actor.has_style_class_name?.('tmb-window'))
        )
            throw new Error('Unlimited Cursor card has the wrong notice or quota rows');
        await waitOrFail(
            () => root.get_transformed_position()[0] > 0 && root.width > 100 && root.height > 100,
            'Cursor Usage menu was not allocated',
        );
        await afterPaint();
        const directory = GLib.getenv('TMB_SCREENSHOT_DIR');
        if (directory) {
            GLib.mkdir_with_parents(directory, 0o755);
            const [left, top] = root.get_transformed_position();
            const stream = Gio.File.new_for_path(
                GLib.build_filenamev([directory, 'cursor-unlimited-menu.png']),
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
