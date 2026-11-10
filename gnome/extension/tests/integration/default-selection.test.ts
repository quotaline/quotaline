import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import St from 'gi://St';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {
    clickButton,
    containsText,
    failure,
    findActor,
    indicator as getIndicator,
    instance,
    required,
} from '../support/actors.js';

function waitForPainted(check: () => unknown) {
    if (check()) return Promise.resolve();
    return new Promise<void>(resolve => {
        const signal = global.stage.connect('after-paint', () => {
            if (!check()) return;
            global.stage.disconnect(signal);
            resolve();
        });
        global.stage.queue_redraw();
    });
}

export async function run() {
    try {
        Main.overview.hide();
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        indicator.menu.open(false);
        const root = indicator.menu.box;
        await waitForPainted(() => findActor(root, actor => actor.accessible_name === 'Settings'));
        const inactiveRow = () =>
            findActor(
                root,
                actor => actor.has_style_class_name?.('tmb-window') && actor.accessible_name === 'Gemini, C, 30% used',
            );
        if (inactiveRow()) throw new Error('Usage showed a model outside the default selection');
        clickButton(required(findActor(root, actor => actor.accessible_name === 'Settings')));
        const model = () => findActor(root, actor => actor.accessible_name === 'Show Gemini C');
        await waitForPainted(() => model());
        const group = () =>
            findActor(
                root,
                actor => actor.has_style_class_name?.('tmb-model-group-card') && containsText(actor, 'Gemini'),
            );
        if (!group() || !containsText(group(), '2 of 3 shown') || required(model()).checked)
            throw new Error('Settings did not show the two default Gemini models');
        const panel = required(findActor(root, actor => actor.has_style_class_name?.('tmb-root')));
        const scroll = required(findActor(panel, actor => actor.has_style_class_name?.('tmb-scroll')));
        await waitForPainted(() => group()?.has_allocation() && panel.has_allocation() && scroll.has_allocation());
        const adjustment = instance(scroll, St.ScrollView).get_vadjustment();
        adjustment.set_value(
            adjustment.value +
                required(group()).get_transformed_position()[1] -
                panel.get_transformed_position()[1] -
                105,
        );
        const directory = GLib.getenv('TMB_SCREENSHOT_DIR');
        if (directory) {
            GLib.mkdir_with_parents(directory, 0o755);
            const stream = Gio.File.new_for_path(
                GLib.build_filenamev([directory, 'settings-default-model-selection.png']),
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
        clickButton(required(model()));
        await waitForPainted(() => model()?.checked);
        if (!containsText(group(), '3 of 3 shown'))
            throw new Error('Selecting the third Gemini model did not update Settings');
        clickButton(required(findActor(root, actor => actor.accessible_name === 'Usage')));
        await waitForPainted(() => inactiveRow());
        if (!containsText(inactiveRow(), 'inactive'))
            throw new Error('Selected inactive model was not identified in Usage');
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
