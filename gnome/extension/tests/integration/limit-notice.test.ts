import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import {closeInstantly, openInstantly} from '../../src/menu-animation.js';
import {accessibleName, failure, findActor, indicator as getIndicator, required} from '../support/actors.js';
import {afterPaint, hideOverview, waitOrFail} from '../support/shell-events.js';

export async function run() {
    try {
        await hideOverview();
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        openInstantly(indicator.menu);
        await waitOrFail(
            () =>
                findActor(
                    indicator.menu.box,
                    actor =>
                        accessibleName(actor)?.startsWith('Claude, ') && actor.accessible_name.endsWith(', 100% used'),
                ),
            'Full Claude limit did not appear in Usage',
        );
        const card = required(
            findActor(
                indicator.menu.box,
                actor =>
                    actor.has_style_class_name?.('tmb-card') &&
                    findActor(
                        actor,
                        child =>
                            accessibleName(child)?.startsWith('Claude, ') &&
                            child.accessible_name.endsWith(', 100% used'),
                    ),
            ),
        );
        const notices = card
            .get_children()
            .flatMap(child => child.get_children())
            .filter(actor => actor.has_style_class_name?.('tmb-provider-notice'));
        if (
            !notices.some(notice => findActor(notice, child => child.text === 'General limit reached')) ||
            notices.some(notice => findActor(notice, child => child.text === 'Selected limit reached'))
        )
            throw new Error('Usage repeated a full selected limit notice or hid the general notice');
        const directory = GLib.getenv('TMB_SCREENSHOT_DIR');
        if (directory) {
            GLib.mkdir_with_parents(directory, 0o755);
            await afterPaint();
            const visibleCard = findActor(
                indicator.menu.box,
                actor =>
                    actor.has_style_class_name?.('tmb-card') &&
                    findActor(
                        actor,
                        child =>
                            accessibleName(child)?.startsWith('Claude, ') &&
                            child.accessible_name.endsWith(', 100% used'),
                    ),
            );
            if (!visibleCard) throw new Error('Full Claude limit disappeared before capture');
            const [left, top] = visibleCard.get_transformed_position();
            const x = Math.max(0, Math.floor(left));
            const y = Math.max(0, Math.floor(top));
            const width = Math.min(global.stage.width - x, Math.ceil(visibleCard.width));
            const height = Math.min(global.stage.height - y, 270);
            const stream = Gio.File.new_for_path(GLib.build_filenamev([directory, 'usage-full-limit.png'])).replace(
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
