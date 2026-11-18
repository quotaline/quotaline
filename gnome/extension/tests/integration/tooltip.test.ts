import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import St from 'gi://St';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {closeInstantly, openInstantly} from '../../src/menu-animation.js';
import type {TestActor} from '../support/actors.js';
import {failure, findActor, indicator as getIndicator, instance, required} from '../support/actors.js';
import {hideOverview, waitOrFail} from '../support/shell-events.js';

function waitFor(check: () => unknown, failure: string | (() => string) = 'Quota help did not update') {
    return waitOrFail(check, failure);
}

export async function run() {
    try {
        await hideOverview();
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        openInstantly(indicator.menu);
        const hover = GLib.getenv('TMB_TOOLTIP_INPUT') === 'hover';
        await waitFor(() =>
            findActor(
                indicator.menu.box,
                actor =>
                    actor.has_style_class_name?.('tmb-window') &&
                    findActor(actor, child => child.text === 'Current session'),
            ),
        );
        if (hover)
            await waitFor(() => findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-cost-tiles')));
        if (hover)
            await waitFor(() => {
                const actor = findActor(
                    indicator.menu.box,
                    item =>
                        item.has_style_class_name?.('tmb-window') &&
                        findActor(item, child => child.text === 'Current session'),
                );
                return actor?.get_transformed_position().every(Number.isFinite);
            }, 'Quota row was not allocated');
        let row = findActor(
            indicator.menu.box,
            actor =>
                actor.has_style_class_name?.('tmb-window') &&
                findActor(actor, child => child.text === 'Current session'),
        );
        if (!hover) required(row).grab_key_focus();
        if (!hover && !required(row).has_key_focus()) throw new Error('Quota row did not accept keyboard focus');
        if (hover) {
            let armed: TestActor | null = null;
            await waitFor(() => {
                const current = findActor(
                    indicator.menu.box,
                    actor =>
                        actor.has_style_class_name?.('tmb-window') &&
                        findActor(actor, child => child.text === 'Current session'),
                );
                if (current && current !== armed) {
                    armed = current;
                    row = current;
                    instance(row, St.Widget).set_hover(false);
                    instance(row, St.Widget).set_hover(true);
                }
                return findActor(
                    Main.layoutManager.uiGroup,
                    actor => actor.has_style_class_name?.('tmb-tooltip') && actor.visible,
                );
            }, 'Quota help did not open');
        } else {
            await waitFor(
                () =>
                    findActor(
                        Main.layoutManager.uiGroup,
                        actor => actor.has_style_class_name?.('tmb-tooltip') && actor.visible,
                    ),
                'Quota help did not open',
            );
        }
        const popup = required(
            findActor(
                Main.layoutManager.uiGroup,
                actor => actor.has_style_class_name?.('tmb-tooltip') && actor.visible,
            ),
        );
        const body = findActor(popup, child => child.has_style_class_name?.('tmb-tooltip-body'))?.text;
        if (
            !findActor(popup, child => child.text === 'Current session') ||
            !body?.startsWith('Used ') ||
            !body.includes('Window duration:') ||
            (!body.includes('Even pace would be') && !body.includes('Pace ratio:') && !body.includes('Limit reached.'))
        )
            throw new Error(
                `Quota help content is wrong: ${popup
                    .get_children()
                    .map(child => child.text)
                    .join(' | ')}`,
            );
        if (
            !hover &&
            !findActor(
                indicator.menu.box,
                actor =>
                    actor.has_style_class_name?.('tmb-window') &&
                    findActor(actor, child => child.text === 'Current session'),
            )?.has_key_focus()
        )
            throw new Error('Usage refresh lost focus from the quota row');
        const screenshots = GLib.getenv('TMB_SCREENSHOT_DIR');
        if (screenshots && !hover) {
            await new Promise<void>(resolve => {
                const signal = global.stage.connect('after-paint', () => {
                    global.stage.disconnect(signal);
                    resolve();
                });
                global.stage.queue_redraw();
            });
            GLib.mkdir_with_parents(screenshots, 0o755);
            const stream = Gio.File.new_for_path(GLib.build_filenamev([screenshots, 'quota-help.png'])).replace(
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
        if (hover) {
            instance(required(row), St.Widget).set_hover(false);
            await waitFor(
                () =>
                    !findActor(
                        Main.layoutManager.uiGroup,
                        actor => actor.has_style_class_name?.('tmb-tooltip') && actor.visible,
                    ),
                'Quota help did not close after pointer exit',
            );
        }
        closeInstantly(indicator.menu);
        await waitFor(
            () =>
                !findActor(
                    Main.layoutManager.uiGroup,
                    actor => actor.has_style_class_name?.('tmb-tooltip') && actor.visible,
                ),
        );
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
