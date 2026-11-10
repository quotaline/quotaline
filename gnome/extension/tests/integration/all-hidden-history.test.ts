import Clutter from 'gi://Clutter';
import GLib from 'gi://GLib';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {
    accessibleName,
    buttonLabel,
    clickButton,
    findActor,
    indicator as getIndicator,
    required,
} from '../support/actors.js';
import {proxyCalls} from '../support/dbus.js';
import {afterPaint, waitOrFail} from '../support/shell-events.js';

export async function run() {
    try {
        Main.overview.hide();
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        indicator.menu.open(false);
        await waitOrFail(
            () => findActor(indicator.menu.box, actor => actor.accessible_name === 'History'),
            'History tab is missing',
        );
        clickButton(required(findActor(indicator.menu.box, actor => actor.accessible_name === 'History')));
        await waitOrFail(
            () => findActor(indicator.menu.box, actor => actor.accessible_name === 'Data details'),
            'History did not load its hidden series',
        );
        if (findActor(indicator.menu.box, actor => buttonLabel(actor) === 'Previous reading'))
            throw new Error('Reading navigation appeared with every series hidden');
        const chart = required(findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-chart')));
        const call = proxyCalls.call;
        let steps = 0;
        proxyCalls.call = function (method, ...args) {
            if (method === 'StepHistoryReading') steps++;
            call.call(this, method, ...args);
        };
        try {
            chart.grab_key_focus();
            const keyboard = global.stage.context
                .get_backend()
                .get_default_seat()
                .create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
            const now = GLib.get_monotonic_time();
            keyboard.notify_keyval(now, Clutter.KEY_Right, Clutter.KeyState.PRESSED);
            keyboard.notify_keyval(now + 1000, Clutter.KEY_Right, Clutter.KeyState.RELEASED);
            await afterPaint();
            keyboard.run_dispose();
            if (steps !== 0) throw new Error('Empty History chart sent a reading step to the service');
        } finally {
            proxyCalls.call = call;
        }
        const hidden = findActor(indicator.menu.box, actor => accessibleName(actor)?.startsWith('☐'));
        if (!hidden) throw new Error('Hidden series cannot be restored');
        clickButton(hidden);
        await waitOrFail(
            () => findActor(indicator.menu.box, actor => buttonLabel(actor) === 'Previous reading'),
            'Restored series did not restore reading navigation',
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
