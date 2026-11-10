import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import St from 'gi://St';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {
    accessibleName,
    clickButton,
    failure,
    findActor,
    indicator as getIndicator,
    instance,
    required,
} from '../support/actors.js';
import {waitOrFail} from '../support/shell-events.js';

function waitFor(check: () => unknown) {
    return waitOrFail(check, 'Distinct derived short labels did not render');
}

export async function run() {
    try {
        Main.overview.hide();
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        indicator.menu.open(false);
        await waitFor(() => indicator.accessible_name.includes('SPK2'));
        const root = indicator.menu.box.get_children()[0];
        clickButton(
            required(
                findActor(
                    root,
                    actor => actor.accessible_name === 'Settings' && actor.has_style_class_name?.('tmb-tab'),
                ),
            ),
        );
        await waitFor(() =>
            findActor(root, actor => accessibleName(actor)?.startsWith('Codex Spark Beta short label')),
        );
        const alpha = required(
            findActor(root, actor => accessibleName(actor)?.startsWith('Codex Spark Alpha short label')),
        );
        const beta = required(
            findActor(root, actor => accessibleName(actor)?.startsWith('Codex Spark Beta short label')),
        );
        if (instance(alpha, St.Entry).get_text() !== 'SPK' || instance(beta, St.Entry).get_text() !== 'SPK2')
            throw new Error('Unselected models changed the unique labels');
        const directory = GLib.getenv('TMB_SCREENSHOT_DIR');
        if (directory) {
            GLib.mkdir_with_parents(directory, 0o755);
            const stream = Gio.File.new_for_path(GLib.build_filenamev([directory, 'models-label-derived.png'])).replace(
                null,
                false,
                Gio.FileCreateFlags.REPLACE_DESTINATION,
                null,
            );
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
