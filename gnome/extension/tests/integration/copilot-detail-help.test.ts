import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import St from 'gi://St';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import type {TestActor} from '../support/actors.js';
import {
    buttonLabel,
    clickButton,
    failure,
    findActor,
    indicator as getIndicator,
    instance,
    required,
} from '../support/actors.js';
import {afterPaint, waitOrFail} from '../support/shell-events.js';

export async function run() {
    try {
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        indicator.menu.open(false);
        const root = findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-root'));
        const disclosure = () => findActor(root, actor => buttonLabel(actor)?.includes('Details (1)'));
        await waitOrFail(() => disclosure(), 'Copilot provider details did not load');
        clickButton(required(disclosure()));
        const row = () =>
            findActor(
                root,
                actor =>
                    actor.has_style_class_name?.('tmb-key-value') && findActor(actor, child => child.text === 'Chat'),
            );
        await waitOrFail(() => row(), 'Copilot category details did not expand');
        if (!findActor(row(), child => child.text === 'Included: 10.125 requests · Remaining: 4.05 requests'))
            throw new Error('Copilot category quantities did not match the provider response');
        await afterPaint();
        let armed: TestActor | null = null;
        const tooltip = () =>
            findActor(
                Main.layoutManager.uiGroup,
                actor => actor.has_style_class_name?.('tmb-tooltip') && actor.visible,
            );
        await waitOrFail(
            () => {
                if (row() && row() !== armed) {
                    armed = row();
                    instance(required(armed), St.Widget).set_hover(false);
                    instance(required(armed), St.Widget).set_hover(true);
                }
                return (
                    tooltip() &&
                    findActor(tooltip(), child =>
                        child.text?.includes(
                            'Categories may overlap and are not added together. Request-based billing.',
                        ),
                    )
                );
            },
            `Copilot detail explanation did not open on hover: ${tooltip()
                ?.get_children()
                .map(child => child.text)
                .join(' | ')}`,
        );
        await afterPaint();
        const directory = GLib.getenv('TMB_SCREENSHOT_DIR');
        if (directory) {
            GLib.mkdir_with_parents(directory, 0o755);
            const stream = Gio.File.new_for_path(GLib.build_filenamev([directory, 'copilot-detail-help.png'])).replace(
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
        indicator.menu.close(false);
        await waitOrFail(() => !tooltip(), 'Copilot detail explanation stayed visible after closing the menu');
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
