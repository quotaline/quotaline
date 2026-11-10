import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {buttonLabel, clickButton, failure, findActor, indicator as getIndicator, required} from '../support/actors.js';
import {afterPaint, waitOrFail} from '../support/shell-events.js';

export async function run() {
    try {
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        const root = findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-root'));
        const card = () =>
            findActor(
                root,
                actor =>
                    actor.has_style_class_name?.('tmb-first-run') &&
                    findActor(actor, child => buttonLabel(child) === 'Got it'),
            );
        if (!root) throw new Error('First launch did not open the Usage onboarding card');
        await waitOrFail(
            () => indicator.menu.isOpen && card() && root.width > 0 && root.height > 0,
            'First launch did not open the Usage onboarding card',
        );
        await afterPaint();
        const detected = GLib.getenv('TMB_FIRST_RUN_DETECTED') === '1';
        const title = required(findActor(card(), actor => actor.has_style_class_name?.('tmb-first-run-title'))).text;
        const expected = GLib.getenv('TMB_FIRST_RUN_EXPECTED');
        if (
            expected
                ? title !== expected
                : detected
                  ? !required(title).startsWith('Detected: Claude, Codex')
                  : title !== 'No providers detected yet'
        )
            throw new Error(`Wrong first-run provider summary: ${title}`);
        const description = required(findActor(card(), actor => actor.has_style_class_name?.('tmb-secondary'))).text;
        if (
            detected
                ? description !== 'Review the providers to choose what the menu bar shows.'
                : !required(description).startsWith('Sign in to a supported CLI or editor')
        )
            throw new Error(`Wrong first-run guidance: ${description}`);
        if (!findActor(card(), actor => actor.has_style_class_name?.('tmb-first-run-icon')))
            throw new Error('The first-run card has no leading icon');
        if (!detected) {
            const empty = findActor(root, actor => actor.has_style_class_name?.('tmb-usage-empty'));
            if (!empty) throw new Error('The empty state is missing');
            const open = findActor(empty, actor => buttonLabel(actor) === 'Open Providers');
            const emptyTitle = findActor(empty, actor => actor.text === 'No usage available');
            if (
                !findActor(empty, actor => actor.has_style_class_name?.('tmb-usage-empty-icon')) ||
                !open ||
                !emptyTitle ||
                emptyTitle.width < emptyTitle.get_preferred_width(-1)[1] ||
                empty.get_y() <= required(card()).get_y() ||
                open.get_x() + open.width > empty.width
            )
                throw new Error('The empty state does not fit below first-run guidance');
        }
        if (expected) {
            const provider = findActor(
                root,
                actor => actor.has_style_class_name?.('tmb-card') && findActor(actor, child => child.text === 'Cursor'),
            );
            if (
                !provider ||
                !findActor(provider, actor => actor.text === 'Rate limited') ||
                !findActor(provider, actor => actor.accessible_name === 'Retry Cursor') ||
                findActor(root, actor => actor.has_style_class_name?.('tmb-usage-empty'))
            )
                throw new Error('Cursor quota error did not render as a provider card');
            await waitOrFail(() => {
                const status = findActor(root, actor => actor.text === 'Rate limited');
                if (status && !status.has_key_focus()) status.grab_key_focus();
                return findActor(
                    Main.layoutManager.uiGroup,
                    actor =>
                        actor.has_style_class_name?.('tmb-tooltip') &&
                        actor.visible &&
                        findActor(actor, child => child.text === 'Cursor status') &&
                        findActor(actor, child => child.text === 'No successful quota reading yet.'),
                );
            }, 'Provider without a reading has no status explanation');
        }
        const directory = GLib.getenv('TMB_SCREENSHOT_DIR');
        if (directory) {
            await afterPaint();
            const [left, top] = root.get_transformed_position();
            const x = Math.max(0, Math.floor(left) - 8);
            const y = Math.max(0, Math.floor(top) - 8);
            const width = Math.min(global.stage.width - x, Math.ceil(root.width) + 16);
            const height = Math.min(global.stage.height - y, Math.ceil(root.height) + 16);
            const file = expected
                ? 'first-run-cursor-api-error-menu.png'
                : detected
                  ? 'first-run-detected-menu.png'
                  : GLib.getenv('TMB_FIRST_RUN_NARROW') === '1'
                    ? 'first-run-empty-narrow-menu.png'
                    : 'first-run-empty-menu.png';
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
        if (expected) {
            clickButton(required(findActor(card(), actor => buttonLabel(actor) === 'Review providers')));
            await waitOrFail(
                () =>
                    findActor(
                        root,
                        actor =>
                            actor.accessible_name === 'Settings' && actor.has_style_class_name?.('tmb-tab-selected'),
                    ),
                'Review providers did not open Settings',
            );
            const disclosure = findActor(root, actor => actor.accessible_name === 'Connection details for Cursor');
            if (!disclosure) throw new Error('Cursor connection details are missing');
            clickButton(disclosure);
            if (!findActor(root, actor => actor.visible && actor.text === 'Authentication · Credential found'))
                throw new Error('Cursor connection details lost the valid credential after a quota error');
        }
        if (!detected) {
            clickButton(required(findActor(card(), actor => buttonLabel(actor) === 'Review providers')));
            await waitOrFail(
                () =>
                    findActor(
                        root,
                        actor =>
                            actor.accessible_name === 'Settings' && actor.has_style_class_name?.('tmb-tab-selected'),
                    ),
                'Review providers did not open Settings',
            );
            clickButton(
                required(
                    findActor(
                        root,
                        actor => actor.accessible_name === 'Usage' && actor.has_style_class_name?.('tmb-tab'),
                    ),
                ),
            );
            await waitOrFail(() => card(), 'First-run card vanished after reviewing providers');
            clickButton(required(findActor(card(), actor => buttonLabel(actor) === 'Got it')));
            await waitOrFail(() => !card(), 'Got it did not dismiss the first-run card');
        }
        indicator.menu.close(false);
        if (detected) {
            await waitOrFail(() => !Main.layoutManager._startingUp, 'GNOME Shell did not finish startup');
            if (!Main.extensionManager.disableExtension('quotaline@bernat.tech'))
                throw new Error('The first-run extension did not disable');
            if (!Main.extensionManager.enableExtension('quotaline@bernat.tech'))
                throw new Error('The first-run extension did not re-enable');
            const reloaded = required(getIndicator());
            await waitOrFail(
                () =>
                    reloaded.menu.isOpen &&
                    findActor(
                        reloaded.menu.box,
                        actor =>
                            actor.has_style_class_name?.('tmb-first-run') &&
                            findActor(actor, child => buttonLabel(child) === 'Got it'),
                    ),
                'First-run guidance did not open after Shell startup',
            );
            reloaded.menu.close(false);
        }
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
