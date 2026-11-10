import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {clickButton, failure, findActor, indicator as getIndicator, required} from '../support/actors.js';
import {afterPaint, waitOrFail} from '../support/shell-events.js';

function call(method: string, parameters: GLib.Variant | null = null) {
    return new Promise<void>((resolve, reject) => {
        Gio.DBus.session.call(
            'tech.bernat.Quotaline.Gnome',
            '/tech/bernat/Quotaline/Gnome',
            'tech.bernat.Quotaline.Gnome',
            method,
            parameters,
            null,
            Gio.DBusCallFlags.NONE,
            5000,
            null,
            (connection, result) => {
                try {
                    required(connection).call_finish(result);
                    resolve();
                } catch (error) {
                    reject(failure(error));
                }
            },
        );
    });
}

export async function run() {
    try {
        Main.overview.hide();
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        indicator.menu.open(false);
        await waitOrFail(
            () => findActor(indicator.menu.box, actor => actor.accessible_name === 'Settings'),
            'Settings tab did not appear',
        );
        clickButton(required(findActor(indicator.menu.box, actor => actor.accessible_name === 'Settings')));
        await call('WaitForActivityCall');
        await call('SetRetentionDays', new GLib.Variant('(t)', [90]));
        indicator.menu.close(false);
        indicator.menu.open(false);
        await call('WaitForActivityCall');
        await waitOrFail(
            () => findActor(indicator.menu.box, actor => actor.text === '90 days'),
            'New retention setting did not reach the menu',
        );
        await call('ReleaseActivity');
        await afterPaint();
        await afterPaint();
        if (
            findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-notice')) ||
            !findActor(indicator.menu.box, actor => actor.text === '90 days')
        )
            throw new Error('Older model activity reply replaced the new setting');
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
