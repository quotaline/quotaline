import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import {clickButton, failure, findActor, indicator as getIndicator, required} from '../support/actors.js';
import {waitOrFail} from '../support/shell-events.js';

export async function run() {
    try {
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        indicator.menu.open(false);
        const root = findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-root'));
        const status = (name: string) => {
            const row = findActor(
                root,
                actor =>
                    actor.has_style_class_name?.('tmb-setting-row') &&
                    actor.get_children().some(child => child.text === name),
            );
            return row && findActor(row, actor => actor.has_style_class_name?.('tmb-provider-setting-status'))?.text;
        };
        await waitOrFail(
            () => status('Claude') && status('Codex') && status('Gemini'),
            'Provider connection sources did not reach Settings',
        );
        if (status('Claude') !== 'Off' || status('Codex') !== 'Off' || status('Gemini') !== 'Off')
            throw new Error('Disabled providers did not show Off');
        for (const [name, source] of [
            ['Claude', '/fixture/auth.json'],
            ['Codex', 'Keychain'],
            ['Gemini', 'Not checked yet'],
        ] as const) {
            clickButton(required(findActor(root, actor => actor.accessible_name === `Connection details for ${name}`)));
            if (
                !findActor(
                    root,
                    actor =>
                        actor.has_style_class_name?.('tmb-provider-setting-details') &&
                        actor.visible &&
                        findActor(actor, child => child.text === `Authentication · ${source}`),
                )
            )
                throw new Error(`${name} authentication source is missing from connection details`);
        }
        const directory = GLib.getenv('TMB_SCREENSHOT_DIR');
        if (directory) {
            GLib.mkdir_with_parents(directory, 0o755);
            const stream = Gio.File.new_for_path(
                GLib.build_filenamev([directory, 'settings-provider-sources.png']),
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
