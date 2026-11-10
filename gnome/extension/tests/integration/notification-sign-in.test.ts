import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import St from 'gi://St';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {clickButton, failure, findActor, indicator as getIndicator, instance, required} from '../support/actors.js';
import {waitOrFail} from '../support/shell-events.js';

async function afterPaint(action = () => {}) {
    await new Promise<void>(resolve => {
        const signal = global.stage.connect('after-paint', () => {
            global.stage.disconnect(signal);
            resolve();
        });
        action();
        global.stage.queue_redraw();
    });
}

async function capture(name: string) {
    const directory = GLib.getenv('TMB_SCREENSHOT_DIR');
    if (!directory) return;
    GLib.mkdir_with_parents(directory, 0o755);
    const stream = Gio.File.new_for_path(GLib.build_filenamev([directory, `${name}.png`])).replace(
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

export async function run() {
    try {
        Main.overview.hide();
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        indicator.menu.open(false);
        await waitOrFail(
            () => findActor(indicator.menu.box, actor => actor.get_text?.() === 'No usage available'),
            'The provider state did not load in the menu',
        );
        indicator.menu.close(false);
        const proxy = Gio.DBusProxy.new_for_bus_sync(
            Gio.BusType.SESSION,
            Gio.DBusProxyFlags.NONE,
            null,
            'tech.bernat.Quotaline.Gnome',
            '/tech/bernat/Quotaline/Gnome',
            'tech.bernat.Quotaline.Gnome',
            null,
        );
        proxy.call_sync('TestRequestSignIn', new GLib.Variant('(s)', ['unknown']), Gio.DBusCallFlags.NONE, 5000, null);
        await afterPaint();
        if (indicator.menu.isOpen) throw new Error('Unknown provider notification opened the menu');
        proxy.call_sync('TestRequestSignIn', new GLib.Variant('(s)', ['claude']), Gio.DBusCallFlags.NONE, 5000, null);
        proxy.call_sync('TestRequestSignIn', new GLib.Variant('(s)', ['claude']), Gio.DBusCallFlags.NONE, 5000, null);
        await waitOrFail(
            () =>
                indicator.menu.isOpen &&
                findActor(
                    indicator.menu.box,
                    actor => actor.accessible_name === 'Settings' && actor.has_style_class_name?.('tmb-tab-selected'),
                ) &&
                findActor(indicator.menu.box, actor => actor.accessible_name === 'Sign in to Claude'),
            'Notification did not open Claude sign-in controls',
        );
        const root = required(findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-root')));
        // Late replies such as the model list rebuild the Settings page, so look the control up on every check.
        const signIn = () => findActor(indicator.menu.box, actor => actor.accessible_name === 'Sign in to Claude');
        const scroll = required(findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-scroll')));
        await waitOrFail(() => {
            const indicatorCenter = indicator.get_transformed_position()[0] + indicator.width / 2;
            const menuX = root.get_transformed_position()[0];
            return (
                root.width >= 280 &&
                root.width <= Math.min(880, global.stage.width - 32) &&
                menuX >= 0 &&
                menuX + root.width <= global.stage.width &&
                indicatorCenter >= menuX - 20 &&
                indicatorCenter <= menuX + root.width + 20
            );
        }, 'Notification menu did not align with its panel item');
        await waitOrFail(() => {
            const control = signIn();
            const y = required(control).get_transformed_position()[1];
            const scrollY = scroll.get_transformed_position()[1];
            return required(control).mapped && y >= scrollY && y + required(control).height <= scrollY + scroll.height;
        }, 'Claude sign-in controls opened outside the visible Settings area');
        await capture('notification-sign-in-settings');
        await afterPaint(() => {
            instance(scroll, St.ScrollView).get_vadjustment().set_value(0);
        });
        await capture('notification-sign-in-settings-top');
        proxy.call_sync('SetSetting', new GLib.Variant('(ss)', ['demo', 'true']), Gio.DBusCallFlags.NONE, 5000, null);
        const originalLaunch = Gio.AppInfo.launch_default_for_uri.bind(Gio.AppInfo);
        try {
            const uri = await new Promise<string>(resolve => {
                Gio.AppInfo.launch_default_for_uri = value => {
                    resolve(value);
                    return true;
                };
                clickButton(
                    required(findActor(indicator.menu.box, actor => actor.accessible_name === 'Sign in to Claude')),
                );
            });
            if (!uri.endsWith('/provider-setup.html#claude'))
                throw new Error(`Settings sign-in opened the wrong guide: ${uri}`);
        } finally {
            Gio.AppInfo.launch_default_for_uri = originalLaunch;
        }
        indicator.menu.close(false);
        Gio.AppInfo.launch_default_for_uri = () => true;
        try {
            for (const destination of ['Usage', null] as const) {
                proxy.call_sync(
                    'TestRequestSignIn',
                    new GLib.Variant('(s)', ['claude']),
                    Gio.DBusCallFlags.NONE,
                    5000,
                    null,
                );
                await waitOrFail(
                    () =>
                        indicator.menu.isOpen &&
                        findActor(
                            indicator.menu.box,
                            actor =>
                                actor.accessible_name === 'Settings' &&
                                actor.has_style_class_name?.('tmb-tab-selected'),
                        ),
                    'Follow-up sign-in notification did not open Settings',
                );
                if (destination)
                    clickButton(
                        required(
                            findActor(
                                indicator.menu.box,
                                actor =>
                                    actor.accessible_name === destination && actor.has_style_class_name?.('tmb-tab'),
                            ),
                        ),
                    );
                else indicator.menu.close(false);
                await afterPaint();
                await afterPaint();
                if (
                    destination &&
                    !findActor(
                        indicator.menu.box,
                        actor =>
                            actor.accessible_name === destination && actor.has_style_class_name?.('tmb-tab-selected'),
                    )
                )
                    throw new Error('Pending provider scroll changed the selected tab');
                indicator.menu.close(false);
            }
        } finally {
            Gio.AppInfo.launch_default_for_uri = originalLaunch;
        }
        const handedOff = (
            proxy
                .call_sync(
                    'TestNotificationHandoffCount',
                    new GLib.Variant('()', []),
                    Gio.DBusCallFlags.NONE,
                    5000,
                    null,
                )
                .deep_unpack() as [number]
        )[0];
        proxy.call_sync('TestQueueNotification', new GLib.Variant('()', []), Gio.DBusCallFlags.NONE, 5000, null);
        await waitOrFail(
            () =>
                (
                    proxy
                        .call_sync(
                            'TestPendingNotificationCount',
                            new GLib.Variant('()', []),
                            Gio.DBusCallFlags.NONE,
                            5000,
                            null,
                        )
                        .deep_unpack() as [number]
                )[0] === 0,
            'Native Shell did not drain the planned notification',
        );
        await waitOrFail(
            () =>
                (
                    proxy
                        .call_sync(
                            'TestNotificationHandoffCount',
                            new GLib.Variant('()', []),
                            Gio.DBusCallFlags.NONE,
                            5000,
                            null,
                        )
                        .deep_unpack() as [number]
                )[0] > handedOff,
            'Native Shell did not hand off the planned notification',
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
