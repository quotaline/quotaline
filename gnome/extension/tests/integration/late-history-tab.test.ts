import GLib from 'gi://GLib';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {accessibleName, clickButton, findActor, indicator as getIndicator, required} from '../support/actors.js';
import {proxyCalls} from '../support/dbus.js';
import {waitOrFail} from '../support/shell-events.js';

export async function run() {
    const call = proxyCalls.call;
    try {
        Main.overview.hide();
        await waitOrFail(() => accessibleName(getIndicator())?.includes('CC 5h'), 'The extension did not load');
        const indicator = required(getIndicator());
        let releaseHistory: (() => void) | null = null;
        proxyCalls.call = function (method, parameters, flags, timeout, cancellable, callback) {
            if (method === 'GetHistoryView' && this.g_interface_name === 'tech.bernat.Quotaline.Gnome') {
                call.call(this, method, parameters, flags, timeout, cancellable, (proxy, result) => {
                    releaseHistory = () => {
                        callback(proxy, result);
                    };
                    global.stage.queue_redraw();
                });
                return;
            }
            call.call(this, method, parameters, flags, timeout, cancellable, callback);
        };
        indicator.menu.open(false);
        await waitOrFail(
            () => findActor(indicator.menu.box, actor => actor.accessible_name === 'History'),
            'The History tab did not load',
        );
        clickButton(required(findActor(indicator.menu.box, actor => actor.accessible_name === 'History')));
        await waitOrFail(() => releaseHistory, 'The History request did not return');
        clickButton(required(findActor(indicator.menu.box, actor => actor.accessible_name === 'Usage')));
        required<() => void>(releaseHistory)();
        if (
            !findActor(
                indicator.menu.box,
                actor => actor.accessible_name === 'Usage' && actor.has_style_class_name?.('tmb-tab-selected'),
            ) ||
            !findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-card'))
        )
            throw new Error('A late History reply replaced the Usage page');
        indicator.menu.close(false);
    } finally {
        proxyCalls.call = call;
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
