import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import St from 'gi://St';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {closeInstantly, openInstantly} from '../../src/menu-animation.js';
import {failure, findActor, indicator as getIndicator, instance, required} from '../support/actors.js';
import {hideOverview, waitOrFail} from '../support/shell-events.js';

function waitFor(check: () => unknown, description: string | (() => string)) {
    return waitOrFail(
        check,
        () =>
            `${typeof description === 'function' ? description() : description}; panel=${getIndicator()?.accessible_name}`,
    );
}

export async function run() {
    try {
        await hideOverview();
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        const empty = GLib.getenv('TMB_PANEL_EMPTY') === '1';
        await waitFor(
            () => (empty ? indicator.accessible_name === 'Quotaline' : /CC 5h \d+%/.test(indicator.accessible_name)),
            'Panel status did not render',
        );
        const used = empty ? null : required(indicator.accessible_name.match(/CC 5h (\d+)%/))[1];
        instance(indicator, St.Widget).set_hover(true);
        await waitFor(
            () =>
                findActor(
                    Main.layoutManager.uiGroup,
                    actor =>
                        actor.has_style_class_name?.('tmb-tooltip') &&
                        actor.visible &&
                        findActor(actor, child =>
                            empty
                                ? child.text === 'No usage yet'
                                : child.text?.includes('Claude') && child.text.includes(`${used}%`),
                        ),
                ),
            'Panel help did not appear',
        );
        const screenshots = GLib.getenv('TMB_SCREENSHOT_DIR');
        if (screenshots && !empty) {
            await new Promise<void>(resolve => {
                const signal = global.stage.connect('after-paint', () => {
                    global.stage.disconnect(signal);
                    resolve();
                });
                global.stage.queue_redraw();
            });
            GLib.mkdir_with_parents(screenshots, 0o755);
            const stream = Gio.File.new_for_path(GLib.build_filenamev([screenshots, 'panel-help.png'])).replace(
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
        if (GLib.getenv('TMB_PANEL_TRANSFER') === '1') {
            indicator.grab_key_focus();
            if (!indicator.has_key_focus()) throw new Error('Panel item did not accept keyboard focus');
            instance(indicator, St.Widget).set_hover(false);
            if (
                !findActor(
                    Main.layoutManager.uiGroup,
                    actor => actor.has_style_class_name?.('tmb-tooltip') && actor.visible,
                )
            )
                throw new Error('Panel help closed while keyboard focus remained');
            instance(indicator, St.Widget).set_hover(true);
            global.stage.set_key_focus(null);
            if (
                !findActor(
                    Main.layoutManager.uiGroup,
                    actor => actor.has_style_class_name?.('tmb-tooltip') && actor.visible,
                )
            )
                throw new Error('Panel help closed while the pointer remained');
        }
        openInstantly(indicator.menu);
        await waitFor(
            () =>
                !findActor(
                    Main.layoutManager.uiGroup,
                    actor => actor.has_style_class_name?.('tmb-tooltip') && actor.visible,
                ),
            'Panel help did not close',
        );
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
