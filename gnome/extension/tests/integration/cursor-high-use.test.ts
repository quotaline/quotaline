import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import St from 'gi://St';
import {closeInstantly, openInstantly} from '../../src/menu-animation.js';
import {failure, findActor, indicator as getIndicator, optionalInstance, required} from '../support/actors.js';
import {waitOrFail} from '../support/shell-events.js';

export async function run() {
    try {
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        openInstantly(indicator.menu);
        const root = findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-root'));
        const quota = () => findActor(root, actor => actor.has_style_class_name?.('tmb-window'));
        if (!root) throw new Error('The exhausted Cursor quota did not reach Usage');
        await waitOrFail(() => {
            const window = quota();
            return (
                window && findActor(window, actor => actor.has_style_class_name?.('tmb-quota-value'))?.text === '100%'
            );
        }, 'The exhausted Cursor quota did not reach Usage');
        const fill = findActor(quota(), actor => actor.has_style_class_name?.('tmb-meter-fill'), St.Widget);
        if (!fill?.get_style()?.includes('#ff8873'))
            throw new Error(`The exhausted quota does not use the warning color: ${fill?.get_style()}`);
        const panelValue = findActor(
            indicator,
            actor => actor.has_style_class_name?.('tmb-panel-value') && actor.text === '100%',
            St.Widget,
        );
        if (!optionalInstance(panelValue, St.Widget)?.get_style()?.includes('#ff8873'))
            throw new Error(
                `The exhausted panel value is not legible: ${optionalInstance(panelValue, St.Widget)?.get_style()}`,
            );
        await waitOrFail(
            () => root.get_transformed_position()[0] > 0 && root.width > 100 && root.height > 100,
            'Cursor Usage menu was not allocated',
        );
        const directory = GLib.getenv('TMB_SCREENSHOT_DIR');
        if (directory) {
            GLib.mkdir_with_parents(directory, 0o755);
            const full = Gio.File.new_for_path(GLib.build_filenamev([directory, 'cursor-exhausted.png'])).replace(
                null,
                false,
                Gio.FileCreateFlags.REPLACE_DESTINATION,
                null,
            );
            await new Promise<void>((resolve, reject) => {
                new Shell.Screenshot().screenshot(false, full, (source, result) => {
                    try {
                        required(source).screenshot_finish(result);
                        full.close(null);
                        resolve();
                    } catch (error) {
                        reject(failure(error));
                    }
                });
            });
            const [left, top] = root.get_transformed_position();
            const stream = Gio.File.new_for_path(
                GLib.build_filenamev([directory, 'cursor-exhausted-menu.png']),
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
