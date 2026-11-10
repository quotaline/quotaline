import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Pango from 'gi://Pango';
import Shell from 'gi://Shell';
import St from 'gi://St';
import type {ServiceState} from '../../src/service-state.js';
import {
    accessibleName,
    clickButton,
    failure,
    findActor,
    indicator as getIndicator,
    instance,
    required,
} from '../support/actors.js';
import {afterPaint, waitOrFail} from '../support/shell-events.js';

function hiddenKeys() {
    const raw = (
        Gio.DBus.session
            .call_sync(
                'tech.bernat.Quotaline.Gnome',
                '/tech/bernat/Quotaline/Gnome',
                'tech.bernat.Quotaline.Gnome',
                'GetState',
                new GLib.Variant('()', []),
                new GLib.VariantType('(s)'),
                Gio.DBusCallFlags.NONE,
                2000,
                null,
            )
            .deep_unpack() as [string]
    )[0];
    return (JSON.parse(raw) as ServiceState).settings.history_hidden_keys;
}

export async function run() {
    try {
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        indicator.menu.open(false);
        const root = findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-root'));
        if (!root) throw new Error('Quotaline menu is missing');
        clickButton(required(findActor(root, actor => actor.accessible_name === 'History')));
        const isolate = () => findActor(root, actor => actor.has_style_class_name?.('tmb-legend-toggle'));
        await waitOrFail(() => isolate(), 'History has no series to isolate');
        required(isolate()).grab_key_focus();
        const keyboard = global.stage.context
            .get_backend()
            .get_default_seat()
            .create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
        const now = GLib.get_monotonic_time();
        keyboard.notify_keyval(now, Clutter.KEY_i, Clutter.KeyState.PRESSED);
        keyboard.notify_keyval(now + 1000, Clutter.KEY_i, Clutter.KeyState.RELEASED);
        const selected = () =>
            findActor(
                root,
                actor => actor.has_style_class_name?.('tmb-legend-toggle') && accessibleName(actor)?.startsWith('☑ '),
            );
        await waitOrFail(() => hiddenKeys().length > 0 && selected(), 'Isolating a series did not leave one visible');
        const before = JSON.stringify(hiddenKeys());
        const isolatedCount = hiddenKeys().length;
        required(selected()).checked = false;
        clickButton(required(selected()));
        if (JSON.stringify(hiddenKeys()) !== before || !selected()?.checked)
            throw new Error('History hid the last visible series');
        const mark = required(findActor(selected(), actor => actor.has_style_class_name?.('tmb-legend-check')));
        await waitOrFail(() => mark.width > 0, 'The History checkbox is clipped');
        if (mark.width < mark.get_preferred_width(-1)[1]) throw new Error('The History checkbox is clipped');
        if (findActor(root, actor => actor.has_style_class_name?.('tmb-legend-isolate')))
            throw new Error('History shows a separate isolation control');
        const help = findActor(root, actor => actor.text?.includes('Double-click or press I to isolate it.'), St.Label);
        if (!help?.clutter_text.get_line_wrap() || help.clutter_text.get_ellipsize() !== Pango.EllipsizeMode.NONE)
            throw new Error('History isolation instructions are missing');
        const scroll = root.get_children()[1];
        const adjustment = instance(required(scroll), St.ScrollView).get_vadjustment();
        adjustment.set_value(
            adjustment.value +
                required(selected()).get_transformed_position()[1] -
                required(scroll).get_transformed_position()[1] -
                80,
        );
        await waitOrFail(() => {
            const rowY = required(selected()).get_transformed_position()[1];
            const scrollY = required(scroll).get_transformed_position()[1];
            return rowY >= scrollY && rowY + required(selected()).height <= scrollY + required(scroll).height;
        }, 'History series did not scroll into view');
        await afterPaint();
        const touch = global.stage.context
            .get_backend()
            .get_default_seat()
            .create_virtual_device(Clutter.InputDeviceType.TOUCHSCREEN_DEVICE);
        let nextTapAt = 0;
        const doubleTap = async () => {
            await afterPaint();
            const firstTapHides = hiddenKeys().length === 0;
            const clickAt = Math.max(GLib.get_monotonic_time(), nextTapAt);
            nextTapAt = clickAt + 40000 + global.stage.context.get_settings().double_click_time * 1000 + 2000;
            for (const offset of [0, 40000] as const) {
                let row = isolate();
                const scrollY = required(scroll).get_transformed_position()[1];
                const rowY = required(row).get_transformed_position()[1];
                if (rowY < scrollY || rowY + required(row).height > scrollY + required(scroll).height) {
                    adjustment.set_value(adjustment.value + rowY - scrollY - 80);
                    await afterPaint();
                    row = isolate();
                }
                const [x, y] = required(row).get_transformed_position();
                const centerY = y + required(row).height / 2;
                let clicked = false;
                const signal = instance(required(row), St.Button).connect('clicked', () => {
                    clicked = true;
                    global.stage.queue_redraw();
                });
                touch.notify_touch_down(clickAt + offset, 0, x + 15, centerY);
                touch.notify_touch_up(clickAt + offset + 1000, 0);
                try {
                    await waitOrFail(() => clicked, 'The History touch did not activate its series');
                } finally {
                    required(row).disconnect(signal);
                }
                await afterPaint();
                if (offset === 0 && firstTapHides)
                    await waitOrFail(
                        () => hiddenKeys().length === 1 && isolate() !== row && isolate()?.has_allocation(),
                        'The first touch did not rebuild the History legend',
                    );
            }
            await afterPaint();
        };
        await doubleTap();
        await waitOrFail(
            () => hiddenKeys().length === 0,
            `Double-clicking the History series did not restore the others: ${JSON.stringify(hiddenKeys())}`,
        );
        const directory = GLib.getenv('TMB_SCREENSHOT_DIR');
        if (directory) {
            GLib.mkdir_with_parents(directory, 0o755);
            const stream = Gio.File.new_for_path(GLib.build_filenamev([directory, 'history-last-series.png'])).replace(
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
        await doubleTap();
        await waitOrFail(
            () => hiddenKeys().length === isolatedCount && required(isolate()).accessible_name.startsWith('☑ '),
            `Double-clicking a visible History series did not isolate it: ${JSON.stringify({
                hidden: hiddenKeys(),
                row: isolate()?.accessible_name,
                menuOpen: indicator.menu.isOpen,
            })}`,
        );
        await doubleTap();
        await waitOrFail(
            () => hiddenKeys().length === 0,
            `Repeating a History double-click did not restore the other series: ${JSON.stringify(hiddenKeys())}`,
        );
        touch.run_dispose();
        keyboard.run_dispose();
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
