import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import type {TestActor} from '../support/actors.js';
import {clickButton, containsText, failure, findActor, indicator as getIndicator, required} from '../support/actors.js';
import {afterPaint, waitOrFail} from '../support/shell-events.js';

async function capture(root: TestActor) {
    const directory = GLib.getenv('TMB_SCREENSHOT_DIR');
    if (!directory) return;
    GLib.mkdir_with_parents(directory, 0o755);
    const [left, top] = root.get_transformed_position();
    const x = Math.max(0, Math.floor(left) - 8);
    const y = Math.max(0, Math.floor(top) - 8);
    const width = Math.min(global.stage.width - x, Math.ceil(root.width) + 16);
    const height = Math.min(global.stage.height - y, Math.ceil(root.height) + 8);
    const stream = Gio.File.new_for_path(GLib.build_filenamev([directory, 'claude-spend-disabled-menu.png'])).replace(
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

export async function run() {
    try {
        Main.overview.hide();
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        indicator.menu.open(false);
        const root = findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-root'));
        if (!root) throw new Error('Claude did not explain why usage credits are off');
        await waitOrFail(
            () =>
                findActor(
                    root,
                    actor =>
                        actor.has_style_class_name?.('tmb-spend-summary') &&
                        containsText(actor, 'Off (Organization Disabled)'),
                ),
            'Claude did not explain why usage credits are off',
        );
        const disclosure = findActor(
            root,
            actor => actor.has_style_class_name?.('tmb-disclosure') && containsText(actor, 'Credit details'),
        );
        if (!disclosure) throw new Error('Claude credit details are missing');
        clickButton(disclosure);
        await waitOrFail(
            () =>
                findActor(
                    root,
                    actor =>
                        actor.has_style_class_name?.('tmb-key-value') &&
                        containsText(actor, 'Purchase') &&
                        containsText(actor, 'Available'),
                ),
            'Claude credit purchase availability is missing',
        );
        for (const [title, value] of [
            ['Balance', '$37.60'],
            ['Resets', 'Resets in'],
            ['Auto-reload', 'On'],
        ] as const) {
            if (
                !findActor(
                    root,
                    actor =>
                        actor.has_style_class_name?.('tmb-key-value') &&
                        containsText(actor, title) &&
                        containsText(actor, value),
                )
            )
                throw new Error(`Claude ${title} is missing from credit details`);
        }
        await afterPaint();
        await capture(root);
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
