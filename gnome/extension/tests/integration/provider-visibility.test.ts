import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import St from 'gi://St';
import {closeInstantly, openInstantly} from '../../src/menu-animation.js';
import type {TestActor} from '../support/actors.js';
import {accessibleName, failure, findActor, indicator as getIndicator, instance, required} from '../support/actors.js';
import {afterPaint, waitOrFail} from '../support/shell-events.js';

function providerNames(actor: TestActor): string[] {
    return [
        ...(accessibleName(actor)?.startsWith('Connection details for ')
            ? [actor.accessible_name.slice('Connection details for '.length)]
            : []),
        ...actor.get_children().flatMap(providerNames),
    ];
}

export async function run() {
    try {
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        openInstantly(indicator.menu);
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
        const mode = GLib.getenv('TMB_PROVIDER_VISIBILITY');
        const names = providerNames(root);
        const expected = mode === 'mixed' ? ['Claude', 'Codex', 'Gemini', 'Cursor'] : [];
        if (JSON.stringify(names) !== JSON.stringify(expected))
            throw new Error(`Settings providers: ${JSON.stringify(names)}; expected ${JSON.stringify(expected)}`);
        const empty = findActor(
            root,
            actor => actor.text === 'No providers discovered. Select Show all providers to set one up.',
        );
        if (Boolean(empty) !== (mode !== 'mixed'))
            throw new Error('Settings provider empty state does not match discovery');
        const advanced = [
            'Decrease analytics interval',
            'Decrease pace workdays',
            'Refresh expired Claude, Codex, Gemini and Antigravity tokens',
            'Credits expiring',
        ];
        if (mode !== 'mixed') {
            for (const name of advanced) {
                const present = Boolean(
                    findActor(
                        root,
                        actor =>
                            actor.accessible_name === name ||
                            (name.startsWith('Refresh expired Claude') &&
                                actor.accessible_name === 'Refresh expired tokens'),
                    ),
                );
                if (present !== (mode === 'overrides' || (name === 'Credits expiring' && mode !== 'empty')))
                    throw new Error(`${name} visibility does not follow active providers or saved settings`);
            }
            if (!findActor(root, actor => actor.accessible_name === 'Write usage.json'))
                throw new Error('Usage file control disappeared with advanced settings');
        }
        if (mode === 'mixed') {
            const disabled = findActor(root, actor => actor.accessible_name === 'Enable Gemini');
            const limited = findActor(root, actor => actor.text === 'Rate limited');
            const connected = findActor(root, actor => actor.text === 'Connected · Signed in');
            const unreadable = findActor(root, actor => actor.text === 'unreadable');
            if (!disabled || disabled.checked || !limited || !connected || !unreadable)
                throw new Error('A discovered provider has the wrong availability');
        }
        const section = findActor(root, actor => actor.text === 'PROVIDERS');
        const scroll = findActor(root, actor => actor.has_style_class_name?.('tmb-scroll'));
        if (!section || !scroll) throw new Error('Provider settings section is missing');
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
                GLib.build_filenamev([directory, `provider-visibility-${mode}.png`]),
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
