import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import St from 'gi://St';
import type {TestActor} from '../support/actors.js';
import {clickButton, failure, findActor, indicator as getIndicator, instance, required} from '../support/actors.js';
import {waitOrFail} from '../support/shell-events.js';

export async function run() {
    try {
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        const expected = required(GLib.getenv('TMB_EXPECT_LABELS')).split(',');
        const labels = () =>
            (findActor(indicator, actor => actor.has_style_class_name?.('tmb-panel-cells'))?.get_children() ?? [])
                .filter(actor => actor.has_style_class_name?.('tmb-panel-cell'))
                .map(cell => cell.get_children()[0]?.text);
        await waitOrFail(() => labels().length === expected.length, 'Compact panel labels did not load');
        if (labels().join(',') !== expected.join(','))
            throw new Error(`Compact panel labels differ from provider names: ${labels().join(',')}`);
        indicator.menu.open(false);
        const root = findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-root'));
        clickButton(required(findActor(root, actor => actor.accessible_name === 'Settings')));
        const editors = () => {
            const values: string[] = [];
            const visit = (actor: TestActor) => {
                if (actor.has_style_class_name?.('tmb-short-entry')) values.push(instance(actor, St.Entry).get_text());
                for (const child of actor.get_children()) visit(child);
            };
            visit(required(root));
            return values;
        };
        await waitOrFail(() => editors().length === expected.length, 'Model editors did not load');
        if (editors().join(',') !== expected.join(','))
            throw new Error(`Model editors did not use compact defaults: ${editors().join(',')}`);
        const directory = GLib.getenv('TMB_SCREENSHOT_DIR');
        if (directory) {
            GLib.mkdir_with_parents(directory, 0o755);
            const stream = Gio.File.new_for_path(
                GLib.build_filenamev([directory, required(GLib.getenv('TMB_LABEL_SCREENSHOT'))]),
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
