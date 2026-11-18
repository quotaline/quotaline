import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import St from 'gi://St';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {closeInstantly, openInstantly} from '../../src/menu-animation.js';
import {
    failure,
    findActor,
    indicator as getIndicator,
    instance,
    optionalInstance,
    required,
} from '../support/actors.js';
import {waitOrFail} from '../support/shell-events.js';

export async function run() {
    try {
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        openInstantly(indicator.menu);
        const root = findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-root'));
        const spend = () => findActor(root, actor => actor.has_style_class_name?.('tmb-spend'));
        const fill = () => findActor(spend(), actor => actor.has_style_class_name?.('tmb-meter-fill'));
        const panelValue = () => findActor(indicator, actor => actor.has_style_class_name?.('tmb-panel-value'));
        if (!root) throw new Error('Initial Cursor spend meter did not reach Usage');
        await waitOrFail(
            () =>
                optionalInstance(fill(), St.Widget)?.get_style() &&
                optionalInstance(panelValue(), St.Widget)?.get_style() &&
                !required(instance(required(panelValue()), St.Widget).get_style()).includes('#d8d8da'),
            'Initial Cursor spend meter did not reach Usage',
        );
        Gio.DBus.session.call_sync(
            'tech.bernat.Quotaline.Gnome',
            '/tech/bernat/Quotaline/Gnome',
            'tech.bernat.Quotaline.Gnome',
            'Refresh',
            new GLib.Variant('(s)', ['cursor']),
            new GLib.VariantType('(b)'),
            Gio.DBusCallFlags.NONE,
            2000,
            null,
        );
        await waitOrFail(
            () =>
                findActor(
                    root,
                    actor => actor.has_style_class_name?.('tmb-card-updated') && actor.text?.includes(' · '),
                ) &&
                optionalInstance(fill(), St.Widget)?.get_style()?.includes('#808080') &&
                optionalInstance(panelValue(), St.Widget)?.get_style()?.includes('#d8d8da') &&
                findActor(root, actor => actor.accessible_name === 'Retry Cursor'),
            'Failed Cursor refresh did not dim the last reading and panel value',
        );
        required(findActor(root, actor => actor.has_style_class_name?.('tmb-card-updated'))).grab_key_focus();
        await waitOrFail(
            () =>
                findActor(
                    Main.layoutManager.uiGroup,
                    actor =>
                        actor.has_style_class_name?.('tmb-tooltip') &&
                        actor.visible &&
                        findActor(actor, child => child.text === 'Cursor status') &&
                        findActor(actor, child => child.text === 'Values as of the last successful fetch.'),
                ),
            'Cached Cursor reading has no status explanation',
        );
        if (
            !findActor(root, actor => actor.has_style_class_name?.('tmb-quota-value') && actor.text === '25%') ||
            !findActor(
                spend(),
                actor =>
                    actor.has_style_class_name?.('tmb-spend-summary') && actor.text === '$2.00 of $10.00 · 20% used',
            )
        )
            throw new Error('Stale Cursor card lost its last quota or spend amount');
        await waitOrFail(
            () => root.get_transformed_position()[0] > 0 && root.width > 100 && root.height > 100,
            'Cursor Usage menu was not allocated',
        );
        const directory = GLib.getenv('TMB_SCREENSHOT_DIR');
        if (directory) {
            GLib.mkdir_with_parents(directory, 0o755);
            const [left, top] = root.get_transformed_position();
            const stream = Gio.File.new_for_path(GLib.build_filenamev([directory, 'cursor-stale-menu.png'])).replace(
                null,
                false,
                Gio.FileCreateFlags.REPLACE_DESTINATION,
                null,
            );
            await new Promise<void>((resolve, reject) => {
                new Shell.Screenshot().screenshot_area(
                    Math.floor(left),
                    Math.floor(top),
                    Math.ceil(root.width),
                    Math.ceil(root.height),
                    stream,
                    (source, result) => {
                        try {
                            required(source).screenshot_area_finish(result);
                            stream.close(null);
                            resolve();
                        } catch (error) {
                            reject(failure(error));
                        }
                    },
                );
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
