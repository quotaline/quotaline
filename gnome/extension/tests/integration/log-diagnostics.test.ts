import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import type {LogEntry} from '../../src/service-state.js';
import {clickButton, failure, findActor, indicator as getIndicator} from '../support/actors.js';

const BUS = 'tech.bernat.Quotaline.Gnome';
const PATH = '/tech/bernat/Quotaline/Gnome';

function call(method: string, signature: string, values: unknown, result: string) {
    const [json] = Gio.DBus.session
        .call_sync(
            BUS,
            PATH,
            BUS,
            method,
            new GLib.Variant(signature, values),
            new GLib.VariantType(result),
            Gio.DBusCallFlags.NONE,
            2000,
            null,
        )
        .deep_unpack() as [string];
    return json;
}

function logs() {
    return JSON.parse(call('GetLogs', '(b)', [false], '(s)')) as LogEntry[];
}

function waitForLog(check: (entries: LogEntry[]) => unknown, action: () => void) {
    return new Promise<void>((resolve, reject) => {
        let settled = false;
        const finish = (error: unknown) => {
            if (settled) return;
            settled = true;
            Gio.DBus.session.signal_unsubscribe(subscription);
            if (error) reject(failure(error));
            else resolve();
        };
        const subscription = Gio.DBus.session.signal_subscribe(
            BUS,
            BUS,
            'LogChanged',
            PATH,
            null,
            Gio.DBusSignalFlags.NONE,
            () => {
                if (check(logs())) finish(null);
            },
        );
        try {
            action();
            if (check(logs())) finish(null);
        } catch (error) {
            finish(error);
        }
    });
}

export async function run() {
    try {
        Main.overview.hide();
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        indicator.menu.open(false);
        await waitForLog(
            entries =>
                entries.some(
                    entry =>
                        entry.category === 'geometry' &&
                        entry.message.includes('menu=') &&
                        entry.message.includes('monitor='),
                ),
            () => call('SetSetting', '(ss)', ['detailed_logging', 'true'], '(s)'),
        );
        await waitForLog(
            entries =>
                entries.some(
                    entry =>
                        entry.category === 'status' &&
                        entry.message.includes('tier=') &&
                        entry.message.includes('budget='),
                ),
            () => call('SetSetting', '(ss)', ['format', JSON.stringify('inline')], '(s)'),
        );
        const history = findActor(
            indicator.menu.box,
            actor => actor.accessible_name === 'History' && actor.has_style_class_name?.('tmb-tab'),
        );
        if (!history) throw new Error('History tab is missing');
        await waitForLog(
            entries =>
                entries.some(
                    entry =>
                        entry.category === 'tabs' &&
                        entry.message.includes('tab=History') &&
                        entry.message.includes('height='),
                ) && entries.some(entry => entry.category === 'geometry' && entry.message.includes('tab=History')),
            () => {
                clickButton(history);
            },
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
