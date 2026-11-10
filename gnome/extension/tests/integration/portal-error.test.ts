import GLib from 'gi://GLib';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {buttonLabel, clickButton, findActor, indicator as getIndicator, required} from '../support/actors.js';
import {sessionCalls} from '../support/dbus.js';
import {waitOrFail} from '../support/shell-events.js';

function waitFor(check: () => unknown) {
    return waitOrFail(check, 'Save dialog error did not reach Settings');
}

export async function run() {
    const call = sessionCalls.call;
    try {
        Main.overview.hide();
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        indicator.menu.open(false);
        await waitFor(() => findActor(indicator.menu.box, actor => actor.accessible_name === 'Settings'));
        clickButton(required(findActor(indicator.menu.box, actor => actor.accessible_name === 'Settings')));
        await waitFor(() => findActor(indicator.menu.box, actor => buttonLabel(actor) === 'Export all history CSV…'));
        const replyMarker = GLib.getenv('TMB_PORTAL_REPLY_MARKER');
        if (replyMarker) {
            sessionCalls.call = function (...args) {
                if (args[3] === 'SaveFile') {
                    const callback = args[9];
                    args[9] = (...result) => {
                        callback(...result);
                        GLib.file_set_contents(replyMarker, '');
                    };
                }
                call.apply(this, args);
            };
        }
        clickButton(required(findActor(indicator.menu.box, actor => buttonLabel(actor) === 'Export all history CSV…')));
        const expected = GLib.getenv('TMB_EXPECT_ERROR');
        await waitFor(() =>
            findActor(
                indicator.menu.box,
                actor => actor.has_style_class_name?.('tmb-notice') && actor.text?.includes(required(expected)),
            ),
        );
        if (GLib.file_test(required(GLib.getenv('TMB_EXPORT_PATH')), GLib.FileTest.EXISTS))
            throw new Error('Failed save dialog still wrote an export');
        indicator.menu.close(false);
    } finally {
        sessionCalls.call = call;
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
