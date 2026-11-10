import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {
    buttonLabel,
    clickButton,
    containsText,
    failure,
    findActor,
    indicator as getIndicator,
    required,
} from '../support/actors.js';
import {waitOrFail} from '../support/shell-events.js';

function waitFor(check: () => unknown) {
    return waitOrFail(check, 'Delayed History response did not reach the menu');
}

function afterPaint() {
    return new Promise<void>(resolve => {
        const signal = global.stage.connect('after-paint', () => {
            global.stage.disconnect(signal);
            resolve();
        });
        global.stage.queue_redraw();
    });
}

function releaseHistory() {
    return new Promise<void>((resolve, reject) => {
        Gio.DBus.session.call(
            'tech.bernat.Quotaline.Gnome',
            '/tech/bernat/Quotaline/Gnome',
            'tech.bernat.Quotaline.Gnome',
            'ReleaseHistory',
            null,
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

function waitForHistoryCall() {
    return new Promise<void>((resolve, reject) => {
        Gio.DBus.session.call(
            'tech.bernat.Quotaline.Gnome',
            '/tech/bernat/Quotaline/Gnome',
            'tech.bernat.Quotaline.Gnome',
            'WaitForHistoryCall',
            null,
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
        await waitFor(() => containsText(indicator.menu.box, 'Cost history error'));
        clickButton(required(findActor(indicator.menu.box, actor => actor.accessible_name === 'History')));
        await waitFor(() => containsText(indicator.menu.box, 'Loading history'));
        await waitForHistoryCall();
        clickButton(required(findActor(indicator.menu.box, actor => buttonLabel(actor) === '←')));
        clickButton(required(findActor(indicator.menu.box, actor => buttonLabel(actor) === 'Now')));
        await waitForHistoryCall();
        await releaseHistory();
        await afterPaint();
        await releaseHistory();
        await waitFor(() => containsText(indicator.menu.box, 'History unavailable'));
        clickButton(required(findActor(indicator.menu.box, actor => buttonLabel(actor) === 'Retry')));
        await waitForHistoryCall();
        indicator.menu.close(false);
        await afterPaint();
        if (!Main.extensionManager.disableExtension('quotaline@bernat.tech') || getIndicator())
            throw new Error('Disabling with a pending History reply left the panel item behind');
        await releaseHistory();
        await new Promise<void>(resolve =>
            GLib.idle_add(GLib.PRIORITY_DEFAULT_IDLE, () => {
                resolve();
                return GLib.SOURCE_REMOVE;
            }),
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
