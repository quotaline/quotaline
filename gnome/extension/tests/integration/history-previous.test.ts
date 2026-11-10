import GLib from 'gi://GLib';
import {buttonLabel, clickButton, findActor, indicator as getIndicator, required} from '../support/actors.js';
import {proxyCalls} from '../support/dbus.js';
import {afterPaint, waitOrFail} from '../support/shell-events.js';

export async function run() {
    const call = proxyCalls.call;
    try {
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        indicator.menu.open(false);
        const root = findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-root'));
        if (!root) throw new Error('Quotaline menu is missing');
        clickButton(required(findActor(root, actor => actor.accessible_name === 'History')));
        const previous = () => findActor(root, actor => buttonLabel(actor) === 'Previous reading');
        await waitOrFail(() => previous(), 'History has no previous-reading control');
        clickButton(required(previous()));
        const selected = () =>
            findActor(root, actor => actor.has_style_class_name?.('tmb-section') && actor.text?.startsWith('Selected '))
                ?.text;
        await waitOrFail(() => selected(), 'Previous reading did not select the latest point');
        const latest = selected();
        clickButton(required(previous()));
        await waitOrFail(
            () => selected() && selected() !== latest,
            'Previous reading did not move to an earlier point',
        );
        const intercept = (invalid: boolean, hold: boolean) => {
            const attempt: {release: (() => void) | null} = {release: null};
            let first = true;
            proxyCalls.call = function (method, parameters, ...args) {
                if (method !== 'StepHistoryReading' || !first) {
                    call.call(this, method, parameters, ...args);
                    return;
                }
                first = false;
                if (invalid) {
                    const [, current, directions] = required(parameters).deep_unpack() as [number, number, number[]];
                    parameters = new GLib.Variant('(iiai)', [-1, current, directions]);
                }
                if (hold) {
                    const callback = args[3];
                    args[3] = (proxy, result) => {
                        attempt.release = () => {
                            callback(proxy, result);
                        };
                    };
                }
                call.call(this, method, parameters, ...args);
            };
            return attempt;
        };
        const next = () => findActor(root, actor => buttonLabel(actor) === 'Next reading');
        const before = selected();
        const stale = intercept(false, true);
        clickButton(required(next()));
        clickButton(required(next()));
        await waitOrFail(() => stale.release && selected() !== before, 'Queued History reading events lost a step');
        const settled = selected();
        required<() => void>(stale.release)();
        await afterPaint();
        if (selected() !== settled) throw new Error('A late History reading response changed the current selection');
        const staleError = intercept(true, true);
        clickButton(required(previous()));
        clickButton(required(previous()));
        await waitOrFail(
            () => staleError.release && selected() !== settled,
            'History reading did not advance past a delayed error',
        );
        required<() => void>(staleError.release)();
        await afterPaint();
        if (findActor(root, actor => actor.text?.includes('Invalid history reading index')))
            throw new Error('A late History reading error replaced the current state');
        intercept(true, false);
        clickButton(required(previous()));
        await waitOrFail(
            () => findActor(root, actor => actor.text?.includes('Invalid history reading index')),
            'History reading error was hidden',
        );
        proxyCalls.call = call;
        const selectedBeforeRecovery = selected();
        clickButton(required(previous()));
        await waitOrFail(
            () => selected() !== selectedBeforeRecovery,
            'History reading did not recover after a service error',
        );
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
