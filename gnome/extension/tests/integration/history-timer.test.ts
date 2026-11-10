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
import {waitOrFail} from '../support/shell-events.js';

export async function run() {
    const call = proxyCalls.call;
    const addSeconds = GLib.timeout_add_seconds;
    try {
        Main.overview.hide();
        await waitOrFail(() => accessibleName(getIndicator())?.includes('CC 5h'), 'The extension did not load');
        const indicator = required(getIndicator());
        let requests = 0;
        let tick: GLib.SourceFunc | undefined;
        proxyCalls.call = function (method, ...args) {
            if (method === 'GetHistoryView') requests++;
            call.call(this, method, ...args);
        };
        indicator.menu.open(false);
        await waitOrFail(
            () => findActor(indicator.menu.box, actor => actor.accessible_name === 'History'),
            'The History tab did not load',
        );
        clickButton(required(findActor(indicator.menu.box, actor => actor.accessible_name === 'History')));
        await waitOrFail(() => requests > 0, 'History did not load');
        indicator.menu.close(false);
        requests = 0;
        GLib.timeout_add_seconds = (priority, interval, callback) => {
            if (interval === 60) tick = callback;
            return addSeconds(priority, interval, callback);
        };
        try {
            indicator.menu.open(false);
        } finally {
            GLib.timeout_add_seconds = addSeconds;
        }
        await waitOrFail(() => requests > 0 && tick, 'History did not load after reopening the menu');
        const initialRequests = requests;
        required(tick)(null);
        await waitOrFail(() => requests > initialRequests, 'The History minute timer did not refresh the chart');
        await waitOrFail(
            () => findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-history-path'))?.text,
            'The History date range did not load',
        );
        const previous = findActor(indicator.menu.box, actor => buttonLabel(actor) === '←');
        if (!previous) throw new Error('The previous period control did not load');
        const beforePage = requests;
        const beforeRange = findActor(indicator.menu.box, actor =>
            actor.has_style_class_name?.('tmb-history-path'),
        )?.text;
        clickButton(previous);
        await waitOrFail(
            () =>
                requests > beforePage &&
                findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-history-path'))?.text !==
                    beforeRange,
            'The previous period control did not request a fixed date range',
        );
        const pagedRequests = requests;
        required(tick)(null);
        if (requests !== pagedRequests)
            throw new Error(`The History minute timer refreshed a fixed date range: ${pagedRequests} → ${requests}`);
        indicator.menu.close(false);
    } finally {
        proxyCalls.call = call;
        GLib.timeout_add_seconds = addSeconds;
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
