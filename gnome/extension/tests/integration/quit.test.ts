import Clutter from 'gi://Clutter';
import GLib from 'gi://GLib';
import {ExtensionState} from 'resource:///org/gnome/shell/misc/extensionUtils.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {clickButton, findActor, indicator as getIndicator} from '../support/actors.js';
import {waitOrFail} from '../support/shell-events.js';

const UUID = 'quotaline@bernat.tech';

function waitFor(check: () => unknown) {
    return waitOrFail(check, 'Quit did not update the GNOME Shell panel');
}

export async function run() {
    try {
        Main.overview.hide();
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        indicator.menu.open(false);
        const quit = findActor(indicator.menu.box, actor => actor.accessible_name === 'Quit');
        if (!quit) throw new Error('Quit control is missing from the menu footer');
        if (!global.settings.get_strv('enabled-extensions').includes(UUID))
            throw new Error('The extension was not enabled before Quit');
        const action = GLib.getenv('TMB_QUIT_ACTION');
        if (action === 'button') {
            clickButton(quit);
            clickButton(quit);
        } else {
            quit.grab_key_focus();
            const keyboard = global.stage.context
                .get_backend()
                .get_default_seat()
                .create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
            const now = GLib.get_monotonic_time();
            keyboard.notify_keyval(now, Clutter.KEY_Control_L, Clutter.KeyState.PRESSED);
            keyboard.notify_keyval(
                now + 1000,
                action === 'upper' ? Clutter.KEY_Q : Clutter.KEY_q,
                Clutter.KeyState.PRESSED,
            );
            keyboard.notify_keyval(
                now + 2000,
                action === 'upper' ? Clutter.KEY_Q : Clutter.KEY_q,
                Clutter.KeyState.RELEASED,
            );
            keyboard.notify_keyval(now + 3000, Clutter.KEY_Control_L, Clutter.KeyState.RELEASED);
        }
        await waitFor(() => !getIndicator());
        if (
            !global.settings.get_strv('enabled-extensions').includes(UUID) ||
            Main.extensionManager.lookup(UUID).state !== ExtensionState.ACTIVE
        )
            throw new Error('Quit disabled the extension beyond this login session');
        if (GLib.getenv('TMB_QUIT_LIFECYCLE')) {
            const sessionMode = Main.sessionMode as {pushMode(mode: string): void; popMode(mode: string): void};
            sessionMode.pushMode('unlock-dialog');
            try {
                await waitFor(() => Main.extensionManager.lookup(UUID).state === ExtensionState.INACTIVE);
            } finally {
                sessionMode.popMode('unlock-dialog');
            }
            await waitFor(() => Main.extensionManager.lookup(UUID).state === ExtensionState.ACTIVE);
            if (getIndicator()) throw new Error('Unlocking restarted the panel after Quit');
            if (!Main.extensionManager.disableExtension(UUID))
                throw new Error('GNOME Extensions could not disable the quit extension');
            await waitFor(() => Main.extensionManager.lookup(UUID).state === ExtensionState.INACTIVE);
            if (!Main.extensionManager.enableExtension(UUID))
                throw new Error('GNOME Extensions could not re-enable the quit extension');
            await waitFor(() => Main.extensionManager.lookup(UUID).state === ExtensionState.ACTIVE && getIndicator());
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
