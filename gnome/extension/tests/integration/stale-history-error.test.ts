import GLib from 'gi://GLib';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {buttonLabel, clickButton, findActor, indicator as getIndicator, required} from '../support/actors.js';
import {proxyCalls} from '../support/dbus.js';
import {waitOrFail} from '../support/shell-events.js';

export async function run() {
    const call = proxyCalls.call;
    const callFinish = proxyCalls.call_finish;
    try {
        Main.overview.hide();
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        let releaseHistory: (() => void) | null = null;
        let delayFirst = true;
        proxyCalls.call = function (method, parameters, flags, timeout, cancellable, callback) {
            if (method === 'GetHistoryView' && delayFirst) {
                delayFirst = false;
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
        await waitOrFail(() => releaseHistory, 'The first History reply did not arrive');
        clickButton(required(findActor(indicator.menu.box, actor => buttonLabel(actor) === 'Now')));
        await waitOrFail(
            () => findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-chart')),
            'The current History chart did not load',
        );
        proxyCalls.call_finish = () => {
            throw new Error('stale fixture failure');
        };
        required<() => void>(releaseHistory)();
        proxyCalls.call_finish = callFinish;
        if (
            !findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-chart')) ||
            findActor(indicator.menu.box, actor => actor.text?.includes('stale fixture failure'))
        )
            throw new Error('A stale History error replaced the current chart');
        indicator.menu.close(false);
    } finally {
        proxyCalls.call = call;
        proxyCalls.call_finish = callFinish;
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
