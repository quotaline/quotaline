import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';

// Waits as long as it takes: on a busy machine a deadline fails correct code, and the test runner stops real hangs.
export function waitFor(check: () => unknown) {
    return new Promise<void>(resolve => {
        if (check()) {
            resolve();
            return;
        }
        let poll = 0;
        const finish = () => {
            global.stage.disconnect(paint);
            Gio.DBus.session.signal_unsubscribe(changed);
            if (poll) GLib.Source.remove(poll);
            resolve();
        };
        const paint = global.stage.connect('after-paint', () => {
            if (check()) finish();
        });
        const changed = Gio.DBus.session.signal_subscribe(
            'tech.bernat.Quotaline.Gnome',
            null,
            null,
            '/tech/bernat/Quotaline/Gnome',
            null,
            Gio.DBusSignalFlags.NONE,
            () => {
                if (check()) finish();
                else global.stage.queue_redraw();
            },
        );
        // Service state such as a notification hand-off can change without repainting or signalling.
        poll = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 100, () => {
            if (!check()) return GLib.SOURCE_CONTINUE;
            poll = 0;
            finish();
            return GLib.SOURCE_REMOVE;
        });
        global.stage.queue_redraw();
    });
}

// The description is logged before waiting, so a run the test runner stops still says what never happened.
export async function waitOrFail(check: () => unknown, description: string | (() => string)) {
    if (check()) return;
    console.log(`Waiting until: ${typeof description === 'function' ? description() : description}`);
    await waitFor(check);
}

export function afterPaint() {
    return new Promise<void>(resolve => {
        const paint = global.stage.connect('after-paint', () => {
            global.stage.disconnect(paint);
            resolve();
        });
        global.stage.queue_redraw();
    });
}

// GNOME 51 can still be finishing its startup overview, and hiding it later moves key focus off the extension.
export async function hideOverview() {
    Main.overview.hide();
    await waitOrFail(() => !Main.overview.visible, 'GNOME overview did not close');
}
