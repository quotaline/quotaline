import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {closeInstantly, openInstantly} from '../../src/menu-animation.js';
import {failure, findActor, indicator as getIndicator, required} from '../support/actors.js';
import {afterPaint, hideOverview, waitOrFail} from '../support/shell-events.js';

export async function run() {
    try {
        await hideOverview();
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        openInstantly(indicator.menu);
        const status = (name: string) =>
            findActor(
                indicator.menu.box,
                actor => actor.accessible_name === `${name} status` && actor.has_style_class_name?.('tmb-card-updated'),
            );
        await waitOrFail(
            () =>
                required(status('Claude')?.text).startsWith('refreshing… · showing ') &&
                required(status('Antigravity')?.text).startsWith('fetched '),
            'Refreshing Claude did not keep the other provider current',
        );
        await afterPaint();
        required(status('Claude')).grab_key_focus();
        await waitOrFail(
            () =>
                findActor(
                    Main.layoutManager.uiGroup,
                    actor =>
                        actor.has_style_class_name?.('tmb-tooltip') &&
                        actor.visible &&
                        findActor(actor, child => child.text === 'Claude status') &&
                        findActor(
                            actor,
                            child =>
                                child.text ===
                                'Fetching new values; the ones below are from the last successful fetch.',
                        ),
                ),
            `Refreshing card did not explain its retained values: ${findActor(Main.layoutManager.uiGroup, actor =>
                actor.has_style_class_name?.('tmb-tooltip'),
            )
                ?.get_children()
                .map(child => child.text)
                .join(' | ')}`,
        );
        const directory = GLib.getenv('TMB_SCREENSHOT_DIR');
        if (directory) {
            GLib.mkdir_with_parents(directory, 0o755);
            await afterPaint();
            const card = findActor(
                indicator.menu.box,
                actor =>
                    actor.has_style_class_name?.('tmb-card') &&
                    findActor(actor, child => child.accessible_name === 'Claude status'),
            );
            if (!card) throw new Error('Refreshing Claude card disappeared before capture');
            const [left, top] = card.get_transformed_position();
            const x = Math.max(0, Math.floor(left));
            const y = Math.max(0, Math.floor(top));
            const width = Math.min(global.stage.width - x, Math.ceil(card.width));
            const height = Math.min(global.stage.height - y, 170);
            const stream = Gio.File.new_for_path(
                GLib.build_filenamev([directory, 'usage-refreshing-provider.png']),
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
