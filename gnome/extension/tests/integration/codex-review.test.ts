import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import St from 'gi://St';
import {closeInstantly, openInstantly} from '../../src/menu-animation.js';
import {failure, findActor, indicator as getIndicator, instance, required} from '../support/actors.js';
import {afterPaint, waitOrFail} from '../support/shell-events.js';

export async function run() {
    try {
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        openInstantly(indicator.menu);
        const root = findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-root'));
        const review = () => findActor(root, actor => actor.has_style_class_name?.('tmb-review-summary'));
        if (!root) throw new Error('Codex review summary did not reach Usage');
        await waitOrFail(() => review(), 'Codex review summary did not reach Usage');
        if (required(required(review()).get_children()[1]).text !== GLib.getenv('TMB_REVIEW_EXPECTED'))
            throw new Error(`Wrong Codex review summary: ${required(required(review()).get_children()[1]).text}`);
        const card = findActor(
            root,
            actor => actor.has_style_class_name?.('tmb-card') && findActor(actor, child => child.text === 'Codex'),
        );
        if (!card || !findActor(card, actor => actor.has_style_class_name?.('tmb-review-summary')))
            throw new Error('Code reviews are outside the Codex card');
        const directory = GLib.getenv('TMB_SCREENSHOT_DIR');
        if (directory) {
            instance(required(root.get_children()[1]), St.ScrollView)
                .get_vadjustment()
                .set_value(card.get_y() - 35);
            await afterPaint();
            const [left, top] = root.get_transformed_position();
            const x = Math.max(0, Math.floor(left) - 8);
            const y = Math.max(0, Math.floor(top) - 8);
            const width = Math.min(global.stage.width - x, Math.ceil(root.width) + 16);
            const height = Math.min(global.stage.height - y, Math.ceil(root.height) + 16);
            const file = required(GLib.getenv('TMB_REVIEW_EXPECTED')).startsWith('0')
                ? 'codex-review-old-menu.png'
                : 'codex-review-menu.png';
            const stream = Gio.File.new_for_path(GLib.build_filenamev([directory, file])).replace(
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
