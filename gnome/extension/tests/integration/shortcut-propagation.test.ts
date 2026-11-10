import Clutter from 'gi://Clutter';
import GLib from 'gi://GLib';
import St from 'gi://St';
import {clickButton, findActor, indicator as getIndicator, instance, optionalInstance} from '../support/actors.js';
import {afterPaint, waitOrFail} from '../support/shell-events.js';

async function press(keyboard: Clutter.VirtualInputDevice, symbol: number, control = false) {
    const now = GLib.get_monotonic_time();
    if (control) keyboard.notify_keyval(now, Clutter.KEY_Control_L, Clutter.KeyState.PRESSED);
    keyboard.notify_keyval(now + 1000, symbol, Clutter.KeyState.PRESSED);
    keyboard.notify_keyval(now + 2000, symbol, Clutter.KeyState.RELEASED);
    if (control) keyboard.notify_keyval(now + 3000, Clutter.KEY_Control_L, Clutter.KeyState.RELEASED);
    await afterPaint();
}

export async function run() {
    try {
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        indicator.menu.open(false);
        const settings = findActor(
            indicator.menu.box,
            actor => actor.accessible_name === 'Settings' && actor.has_style_class_name?.('tmb-tab'),
        );
        if (!settings) throw new Error('Settings tab is missing');
        clickButton(settings);
        await waitOrFail(
            () => findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-model-filter')),
            'Model filter did not load',
        );
        const filter = findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-model-filter'));
        if (!filter) throw new Error('Model filter is missing');
        instance(filter, St.Entry).set_text('Claude');
        filter.grab_key_focus();
        const keyboard = global.stage.context
            .get_backend()
            .get_default_seat()
            .create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
        await press(keyboard, Clutter.KEY_a, true);
        await press(keyboard, Clutter.KEY_z);
        const currentText = () =>
            optionalInstance(
                findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-model-filter')),
                St.Entry,
            )?.get_text();
        await waitOrFail(
            () => currentText() === 'z',
            `Ctrl+A did not pass through to the model filter: ${currentText()}`,
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
