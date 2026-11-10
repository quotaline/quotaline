import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {buttonLabel, clickButton, findActor, indicator as getIndicator, required} from '../support/actors.js';
import {sessionCalls} from '../support/dbus.js';
import {waitOrFail} from '../support/shell-events.js';

function waitFor(check: () => unknown) {
    return waitOrFail(check, 'Full history export did not finish');
}

export async function run() {
    const replyMarker = GLib.getenv('TMB_PORTAL_REPLY_MARKER');
    const signalMarker = GLib.getenv('TMB_PORTAL_SIGNAL_MARKER');
    const sessionCall = sessionCalls.call;
    const signalSubscribe = Gio.DBus.session.signal_subscribe.bind(Gio.DBus.session);
    try {
        Main.overview.hide();
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        indicator.menu.open(false);
        await waitFor(() => findActor(indicator.menu.box, actor => actor.accessible_name === 'Settings'));
        clickButton(required(findActor(indicator.menu.box, actor => actor.accessible_name === 'Settings')));
        await waitFor(() => findActor(indicator.menu.box, actor => buttonLabel(actor) === 'Export all history CSV…'));
        const path = GLib.getenv('TMB_EXPORT_PATH');
        if (!path) throw new Error('History export fixture path is missing');
        if (replyMarker) {
            sessionCalls.call = function (...args) {
                if (args[3] === 'SaveFile') {
                    const callback = args[9];
                    args[9] = (...result) => {
                        callback(...result);
                        GLib.file_set_contents(replyMarker, '');
                    };
                }
                sessionCall.apply(this, args);
            };
        }
        if (signalMarker) {
            Gio.DBus.session.signal_subscribe = function (...args) {
                if (args[1] === 'org.freedesktop.portal.Request' && args[2] === 'Response') {
                    const callback = args[6];
                    args[6] = (...received) => {
                        callback(...received);
                        GLib.file_set_contents(signalMarker, '');
                    };
                }
                return signalSubscribe.apply(this, args);
            };
        }
        clickButton(required(findActor(indicator.menu.box, actor => buttonLabel(actor) === 'Export all history CSV…')));
        await waitFor(() => findActor(indicator.menu.box, actor => actor.text === `Exported ${path}`));
        const [loaded, bytes] = GLib.file_get_contents(path);
        if (!loaded || !String.fromCharCode(...bytes.slice(0, 80)).startsWith('kind,timestamp,key,label,used_percent,'))
            throw new Error('Settings export did not write the full history header');
        indicator.menu.close(false);
    } finally {
        sessionCalls.call = sessionCall;
        Gio.DBus.session.signal_subscribe = signalSubscribe;
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
