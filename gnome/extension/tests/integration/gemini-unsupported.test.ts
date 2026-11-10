import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Pango from 'gi://Pango';
import Shell from 'gi://Shell';
import St from 'gi://St';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {clickButton, failure, findActor, indicator as getIndicator, instance, required} from '../support/actors.js';
import {afterPaint, waitOrFail} from '../support/shell-events.js';

export async function run() {
    try {
        Main.overview.hide();
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        indicator.menu.open(false);
        const root = findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-root'));
        if (!root) throw new Error('Unsupported Gemini account did not show setup guidance');
        await waitOrFail(
            () =>
                findActor(root, actor => actor.accessible_name === 'Set up Gemini') &&
                findActor(root, actor => actor.text?.includes('Login with Google for personal Gemini accounts')),
            'Unsupported Gemini account did not show setup guidance',
        );
        await afterPaint();
        const explanation = findActor(
            root,
            actor =>
                actor.has_style_class_name?.('tmb-card-meta') &&
                actor.text?.includes('Login with Google for personal Gemini accounts'),
        );
        if (
            !explanation ||
            instance(explanation, St.Label).clutter_text.get_ellipsize() !== Pango.EllipsizeMode.NONE ||
            !instance(explanation, St.Label).clutter_text.get_line_wrap() ||
            explanation.height < 24
        )
            throw new Error('Unsupported Gemini account explanation is clipped');
        const directory = GLib.getenv('TMB_SCREENSHOT_DIR');
        if (directory) {
            GLib.mkdir_with_parents(directory, 0o755);
            const [left, top] = root.get_transformed_position();
            const x = Math.max(0, Math.floor(left) - 8);
            const y = Math.max(0, Math.floor(top) - 8);
            const stream = Gio.File.new_for_path(
                GLib.build_filenamev([directory, 'gemini-unsupported-menu.png']),
            ).replace(null, false, Gio.FileCreateFlags.REPLACE_DESTINATION, null);
            await new Promise<void>((resolve, reject) => {
                new Shell.Screenshot().screenshot_area(
                    x,
                    y,
                    Math.min(global.stage.width - x, Math.ceil(root.width) + 16),
                    Math.min(global.stage.height - y, Math.ceil(root.height) + 8),
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
        clickButton(required(findActor(root, actor => actor.accessible_name === 'Set up Gemini')));
        await waitOrFail(
            () =>
                findActor(
                    root,
                    actor => actor.accessible_name === 'Settings' && actor.has_style_class_name?.('tmb-tab-selected'),
                ) && findActor(root, actor => actor.accessible_name === 'Connection details for Gemini'),
            'Gemini setup action did not open provider settings',
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
