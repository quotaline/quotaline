import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import St from 'gi://St';
import {clickButton, failure, findActor, indicator as getIndicator, instance, required} from '../support/actors.js';
import {readProvider, readProviderSettings, readState} from '../support/dbus.js';
import {afterPaint, waitOrFail} from '../support/shell-events.js';

function serviceState() {
    return readState();
}

export async function run() {
    try {
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        indicator.menu.open(false);
        const root = indicator.menu.box;
        const toggle = () => findActor(root, actor => actor.accessible_name === 'Enable GitHub Copilot');
        const status = () => {
            const row = findActor(
                root,
                actor =>
                    actor.has_style_class_name?.('tmb-setting-row') &&
                    actor.get_children()[0]?.text === 'GitHub Copilot',
            );
            return row && findActor(row, actor => actor.has_style_class_name?.('tmb-provider-setting-status'))?.text;
        };
        await waitOrFail(
            () => readProvider('copilot').credential_health === 'missing' && toggle() && status() === 'Not configured',
            'Undiscovered Copilot did not reach Settings',
        );
        if (
            required(toggle()).checked ||
            findActor(root, actor => actor.accessible_name === 'Check GitHub Copilot again')
        )
            throw new Error('Undiscovered Copilot has an active control before setup');
        const scroll = required(findActor(root, actor => actor.has_style_class_name?.('tmb-scroll')));
        await afterPaint();
        const adjustment = instance(scroll, St.ScrollView).get_vadjustment();
        adjustment.set_value((adjustment.upper - adjustment.page_size) * 0.6);
        await afterPaint();
        const directory = GLib.getenv('TMB_SCREENSHOT_DIR');
        if (directory) {
            const panel = required(findActor(root, actor => actor.has_style_class_name?.('tmb-root')));
            const [left, top] = panel.get_transformed_position();
            const x = Math.max(0, Math.floor(left) - 8);
            const y = Math.max(0, Math.floor(top) - 8);
            const width = Math.min(global.stage.width - x, Math.ceil(panel.width) + 16);
            const height = Math.min(global.stage.height - y, Math.ceil(panel.height) + 16);
            const stream = Gio.File.new_for_path(
                GLib.build_filenamev([directory, 'settings-undiscovered-provider.png']),
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
        clickButton(required(toggle()));
        await waitOrFail(
            () =>
                toggle()?.checked &&
                status() === 'Not configured' &&
                serviceState().settings.configured_providers.includes('copilot'),
            'Explicit Copilot activation did not persist',
        );
        if (findActor(root, actor => actor.accessible_name === 'Check GitHub Copilot again'))
            throw new Error('Undiscovered Copilot gained a polling control');
        clickButton(required(toggle()));
        await waitOrFail(
            () => !toggle()?.checked && status() === 'Off' && readProviderSettings('copilot').enabled === false,
            'Explicit Copilot deactivation did not persist',
        );
        clickButton(required(toggle()));
        await waitOrFail(
            () =>
                toggle()?.checked && status() === 'Not configured' && readProviderSettings('copilot').enabled === true,
            'Copilot could not be re-enabled',
        );
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
