import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import {buttonLabel, clickButton, failure, findActor, indicator as getIndicator, required} from '../support/actors.js';
import {afterPaint, waitOrFail} from '../support/shell-events.js';

export async function run() {
    try {
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        indicator.menu.open(false);
        const root = findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-root'));
        const spend = () => findActor(root, actor => actor.has_style_class_name?.('tmb-spend'));
        if (!root) throw new Error('Cursor spend control did not reach Usage');
        await waitOrFail(() => spend()?.get_children().length, 'Cursor spend control did not reach Usage');
        const summary = findActor(spend(), actor => actor.has_style_class_name?.('tmb-spend-summary'));
        if (summary?.text !== 'Off · $1.00 of $1.00 · 100% used')
            throw new Error(`Disabled Cursor spend summary is wrong: ${summary?.text}`);
        if (
            !findActor(spend(), actor => actor.has_style_class_name?.('tmb-spend-reached')) ||
            !findActor(spend(), actor => actor.has_style_class_name?.('tmb-meter-fill'))
        )
            throw new Error('Cursor limit warning or meter is missing');
        clickButton(required(findActor(root, actor => buttonLabel(actor)?.startsWith('▸ Details'))));
        if (!findActor(root, actor => actor.has_style_class_name?.('tmb-key-value')))
            throw new Error('Cursor detail row did not expand');
        clickButton(required(findActor(root, actor => buttonLabel(actor)?.startsWith('▾ Details'))));
        if (findActor(root, actor => actor.has_style_class_name?.('tmb-key-value')))
            throw new Error('Cursor detail row did not collapse');
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
                GLib.build_filenamev([directory, 'cursor-disabled-spend-menu.png']),
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
