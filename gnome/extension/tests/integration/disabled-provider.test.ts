import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {clickButton, failure, findActor, indicator as getIndicator, required} from '../support/actors.js';
import {afterPaint} from '../support/shell-events.js';

export async function run() {
    try {
        Main.overview.hide();
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        indicator.menu.open(false);
        await afterPaint();
        const root = indicator.menu.box;
        clickButton(required(findActor(root, actor => actor.accessible_name === 'Settings')));
        const toggle = findActor(root, actor => actor.accessible_name === 'Enable Antigravity');
        if (
            !toggle ||
            toggle.checked ||
            !findActor(root, actor => actor.accessible_name === 'Connection details for Antigravity')
        )
            throw new Error('Discovered Antigravity is not visible and disabled in Settings');
        const row = findActor(
            root,
            actor => actor.has_style_class_name?.('tmb-setting-row') && actor.get_children()[0]?.text === 'Antigravity',
        );
        if (
            !row ||
            !findActor(row, actor => actor.text === 'Off') ||
            findActor(row, actor => actor.accessible_name === 'Check Antigravity again')
        )
            throw new Error('Disabled Antigravity still shows a connection or refresh control');
        const directory = GLib.getenv('TMB_SCREENSHOT_DIR');
        if (directory) {
            GLib.mkdir_with_parents(directory, 0o755);
            const stream = Gio.File.new_for_path(
                GLib.build_filenamev([directory, 'settings-disabled-antigravity.png']),
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
