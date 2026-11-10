import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import St from 'gi://St';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {
    buttonLabel,
    clickButton,
    containsText,
    failure,
    findActor,
    indicator as getIndicator,
    instance,
    required,
} from '../support/actors.js';
import {afterPaint, waitOrFail} from '../support/shell-events.js';

function waitFor(check: () => unknown) {
    return waitOrFail(check, 'Codex cost history did not reach Usage');
}

export async function run() {
    try {
        Main.overview.hide();
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        indicator.menu.open(false);
        await waitFor(() => {
            const codex = findActor(
                indicator.menu.box,
                actor => actor.has_style_class_name?.('tmb-card') && findActor(actor, child => child.text === 'Codex'),
            );
            return codex && containsText(codex, 'Loading cost history');
        });
        Gio.DBus.session.call_sync(
            'tech.bernat.Quotaline.Gnome',
            '/tech/bernat/Quotaline/Gnome',
            'tech.bernat.Quotaline.Gnome',
            'ReleaseCost',
            null,
            null,
            Gio.DBusCallFlags.NONE,
            2000,
            null,
        );
        const expected = Number(GLib.getenv('TMB_COST_EXPECTED'));
        await waitFor(() => {
            const codex = findActor(
                indicator.menu.box,
                actor => actor.has_style_class_name?.('tmb-card') && findActor(actor, child => child.text === 'Codex'),
            );
            return (
                codex &&
                containsText(codex, 'CODEX API-EQUIVALENT ESTIMATES') &&
                containsText(codex, `$${expected.toFixed(2)}`)
            );
        });
        const currentCodex = () =>
            findActor(
                indicator.menu.box,
                actor => actor.has_style_class_name?.('tmb-card') && findActor(actor, child => child.text === 'Codex'),
            );
        const help = () =>
            findActor(
                Main.layoutManager.uiGroup,
                actor => actor.has_style_class_name?.('tmb-tooltip') && actor.visible,
            );
        const tiles = required(findActor(currentCodex(), actor => actor.has_style_class_name?.('tmb-cost-tiles')));
        const scroll = required(findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-scroll')));
        const adjustment = instance(scroll, St.ScrollView).get_vadjustment();
        adjustment.set_value(
            adjustment.value + tiles.get_transformed_position()[1] - scroll.get_transformed_position()[1] - 100,
        );
        await afterPaint();
        tiles.grab_key_focus();
        await waitFor(() => help()?.get_children()[0]?.text === 'Codex API-equivalent estimates · daily UTC');
        await afterPaint();
        if (
            !required(required(required(help()).get_children()[1]).text).includes(
                'Models over the last 30 days:\nCodex · gpt-5: ',
            )
        )
            throw new Error('Codex cost tiles do not explain the model estimate');
        const directory = GLib.getenv('TMB_SCREENSHOT_DIR');
        if (expected > 0 && directory) {
            GLib.mkdir_with_parents(directory, 0o755);
            const stream = Gio.File.new_for_path(GLib.build_filenamev([directory, 'codex-cost-help.png'])).replace(
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
        required(findActor(currentCodex(), actor => actor.accessible_name === 'Refresh Codex')).grab_key_focus();
        await waitFor(() => help()?.get_children()[0]?.text === 'Refresh Codex');
        if (!required(required(required(help()).get_children()[1]).text).includes('Last-known values remain visible'))
            throw new Error('Provider refresh action has no recovery explanation');
        const disclosure = findActor(currentCodex(), actor => buttonLabel(actor)?.includes('Model cost estimates'));
        if (!disclosure) throw new Error('Codex model costs are missing');
        clickButton(disclosure);
        await waitFor(() => containsText(currentCodex(), 'gpt-5'));
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
