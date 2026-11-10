import Gio from 'gi://Gio';
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
    const call = proxyCalls.call;
    const finish = proxyCalls.call_finish;
    try {
        Main.overview.hide();
        await waitOrFail(() => accessibleName(getIndicator())?.includes('CC 5h'), 'The extension did not load');
        const indicator = required(getIndicator());
        let pendingLog: (() => void) | null = null;
        proxyCalls.call = function (method, ...args) {
            if (method === 'GetLogs' && this.g_interface_name === 'tech.bernat.Quotaline.Gnome') {
                pendingLog = () => {
                    args[4](this, Gio.Task.new(this, null, null));
                };
                return;
            }
            call.call(this, method, ...args);
        };
        indicator.menu.open(false);
        await waitOrFail(
            () => findActor(indicator.menu.box, actor => actor.accessible_name === 'Settings'),
            'Settings did not load',
        );
        clickButton(required(findActor(indicator.menu.box, actor => actor.accessible_name === 'Settings')));
        await waitOrFail(() => pendingLog, 'Settings did not request the log');
        indicator.menu.close(false);
        proxyCalls.call = call;
        proxyCalls.call_finish = () => {
            throw new Error('Fixture delayed log failure');
        };
        required<() => void>(pendingLog)();
        proxyCalls.call_finish = finish;
        indicator.menu.open(false);
        await afterPaint();
        if (findActor(indicator.menu.box, actor => actor.text?.includes('Fixture delayed log failure')))
            throw new Error('A discarded log request showed its delayed error');
        indicator.menu.close(false);
        indicator.menu.open(false);
        clickButton(required(findActor(indicator.menu.box, actor => actor.accessible_name === 'Settings')));
        await waitOrFail(
            () => findActor(indicator.menu.box, actor => buttonLabel(actor) === 'Show full log'),
            'The log controls did not reopen',
        );
        clickButton(required(findActor(indicator.menu.box, actor => buttonLabel(actor) === 'Show full log')));
        await waitOrFail(
            () => findActor(indicator.menu.box, actor => buttonLabel(actor) === 'Show recent log'),
            'The full log did not open',
        );
        let completed = false;
        proxyCalls.call = function (method, ...args) {
            if (method === 'GetLogs' && this.g_interface_name === 'tech.bernat.Quotaline.Gnome') {
                const callback = args[4];
                args[4] = (...values) => {
                    callback(...values);
                    completed = true;
                    indicator.menu.close(false);
                };
            }
            call.call(this, method, ...args);
        };
        clickButton(required(findActor(indicator.menu.box, actor => buttonLabel(actor) === 'Show recent log')));
        await waitOrFail(
            () => completed && !indicator.menu.isOpen,
            'Closing Settings did not cancel its pending log scroll',
        );
        proxyCalls.call = call;
    } finally {
        proxyCalls.call = call;
        proxyCalls.call_finish = finish;
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
