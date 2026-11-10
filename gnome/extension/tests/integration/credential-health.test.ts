import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import St from 'gi://St';
import type {TestActor} from '../support/actors.js';
import {clickButton, failure, findActor, indicator as getIndicator, instance, required} from '../support/actors.js';
import {afterPaint, waitOrFail} from '../support/shell-events.js';

function visibleDetails(actor: TestActor, text: string) {
    let count =
        actor.has_style_class_name?.('tmb-provider-setting-details') &&
        actor.visible &&
        findActor(actor, child => child.text === text)
            ? 1
            : 0;
    for (const child of actor.get_children()) count += visibleDetails(child, text);
    return count;
}

export async function run() {
    try {
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        indicator.menu.open(false);
        const root = findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-root'));
        if (!root) throw new Error('Settings menu did not open');
        await waitOrFail(() => indicator.menu.isOpen && root.width > 0, 'Settings menu did not open');
        await waitOrFail(
            () =>
                findActor(
                    root,
                    actor => actor.accessible_name === 'Settings' && actor.has_style_class_name?.('tmb-tab-selected'),
                ),
            'Settings tab did not open',
        );
        const seen = new Map<string, number>();
        for (const [provider, expected] of [
            ['Claude', 'Not found'],
            ['Codex', 'Expired'],
            ['Gemini', 'Unreadable'],
            ['Antigravity', 'Not checked yet'],
            ['Cursor', 'Credential found'],
            ['GitHub Copilot', 'Not checked yet'],
        ] as const) {
            const disclosure = findActor(root, actor => actor.accessible_name === `Connection details for ${provider}`);
            if (!disclosure) throw new Error(`${provider} connection details are missing`);
            clickButton(disclosure);
            const text = `Authentication · ${expected}`;
            seen.set(text, (seen.get(text) ?? 0) + 1);
            if (visibleDetails(root, text) !== seen.get(text))
                throw new Error(`${provider} credential health is not ${expected}`);
        }
        const scroll = findActor(root, actor => actor.has_style_class_name?.('tmb-scroll'));
        const section = findActor(root, actor => actor.text === 'PROVIDERS');
        if (!scroll || !section) throw new Error('Provider settings section is missing');
        await afterPaint();
        const adjustment = instance(scroll, St.ScrollView).get_vadjustment();
        adjustment.set_value((adjustment.upper - adjustment.page_size) * 0.6);
        await afterPaint();
        const directory = GLib.getenv('TMB_SCREENSHOT_DIR');
        if (directory) {
            const [left, top] = root.get_transformed_position();
            const x = Math.max(0, Math.floor(left) - 8);
            const y = Math.max(0, Math.floor(top) - 8);
            const width = Math.min(global.stage.width - x, Math.ceil(root.width) + 16);
            const height = Math.min(global.stage.height - y, Math.ceil(root.height) + 16);
            const stream = Gio.File.new_for_path(
                GLib.build_filenamev([directory, 'credential-health-settings.png']),
            ).replace(null, false, Gio.FileCreateFlags.REPLACE_DESTINATION, null);
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
