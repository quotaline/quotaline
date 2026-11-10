import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import {ExtensionState} from 'resource:///org/gnome/shell/misc/extensionUtils.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {buttonLabel, clickButton, findActor, indicator as getIndicator, required} from '../support/actors.js';
import {afterPaint, waitOrFail} from '../support/shell-events.js';

const UUID = 'quotaline@bernat.tech';

function waitFor(check: () => unknown) {
    return waitOrFail(check, 'Pending save dialog prevented extension shutdown');
}

function waitForMarker(path: string) {
    return new Promise<void>(resolve => {
        if (GLib.file_test(path, GLib.FileTest.EXISTS)) {
            resolve();
            return;
        }
        const monitor = required(Gio.File.new_for_path(path).get_parent()).monitor_directory(
            Gio.FileMonitorFlags.NONE,
            null,
        );
        let settled = false;
        const finish = () => {
            if (settled) return;
            settled = true;
            monitor.cancel();
            resolve();
        };
        monitor.connect('changed', () => {
            if (GLib.file_test(path, GLib.FileTest.EXISTS)) finish();
        });
        // The marker can appear between the first check and the monitor starting.
        if (GLib.file_test(path, GLib.FileTest.EXISTS)) finish();
    });
}

export async function run() {
    try {
        Main.overview.hide();
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        indicator.menu.open(false);
        await waitFor(() => findActor(indicator.menu.box, actor => actor.accessible_name === 'Settings'));
        clickButton(required(findActor(indicator.menu.box, actor => actor.accessible_name === 'Settings')));
        await waitFor(() => findActor(indicator.menu.box, actor => buttonLabel(actor) === 'Export all history CSV…'));
        clickButton(required(findActor(indicator.menu.box, actor => buttonLabel(actor) === 'Export all history CSV…')));
        await waitFor(() => GLib.file_test(required(GLib.getenv('TMB_PORTAL_MARKER')), GLib.FileTest.EXISTS));
        await afterPaint();
        if (!Main.extensionManager.disableExtension(UUID))
            throw new Error('Extension did not disable during a pending save dialog');
        await waitFor(() => Main.extensionManager.lookup(UUID).state === ExtensionState.INACTIVE && !getIndicator());
        if (GLib.getenv('TMB_PORTAL_WAIT_CLOSE') === '1') {
            GLib.file_set_contents(required(GLib.getenv('TMB_PORTAL_DISABLED')), '');
            await waitForMarker(required(GLib.getenv('TMB_PORTAL_CLOSED')));
        }
        if (GLib.file_test(required(GLib.getenv('TMB_EXPORT_PATH')), GLib.FileTest.EXISTS))
            throw new Error('Disabling the extension wrote an export');
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
