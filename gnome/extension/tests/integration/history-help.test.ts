import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import St from 'gi://St';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import type {TestActor} from '../support/actors.js';
import {
    buttonLabel,
    clickButton,
    failure,
    findActor,
    indicator as getIndicator,
    instance,
    optionalInstance,
    required,
} from '../support/actors.js';
import {waitOrFail} from '../support/shell-events.js';

function waitFor(check: () => unknown, failure: string | (() => string) = 'History help did not open') {
    return waitOrFail(check, failure);
}

export async function run() {
    try {
        Main.overview.hide();
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        indicator.menu.open(false);
        await waitFor(() => findActor(indicator.menu.box, actor => actor.accessible_name === 'History'));
        clickButton(required(findActor(indicator.menu.box, actor => actor.accessible_name === 'History')));
        const mode = GLib.getenv('TMB_HISTORY_HELP') ?? 'metric';
        if (mode === 'stack') {
            await waitFor(() => findActor(indicator.menu.box, actor => actor.accessible_name === 'Metric'));
            clickButton(required(findActor(indicator.menu.box, actor => actor.accessible_name === 'Metric')));
            await waitFor(() =>
                findActor(indicator.menu.box, actor => actor.accessible_name === 'Choose metric: modelCredits'),
            );
            clickButton(
                required(
                    findActor(indicator.menu.box, actor => actor.accessible_name === 'Choose metric: modelCredits'),
                ),
            );
        }
        if (mode === 'from' || mode === 'to' || mode === 'apply') {
            if (!findActor(indicator.menu.box, actor => actor.hint_text === 'From YYYY-MM-DD')) {
                await waitFor(() => findActor(indicator.menu.box, actor => buttonLabel(actor) === 'Custom'));
                clickButton(required(findActor(indicator.menu.box, actor => buttonLabel(actor) === 'Custom')));
            }
        }
        const names: Record<string, string> = {
            metric: 'Metric',
            chart: 'tmb-chart',
            utc: 'UTC',
            legend: 'tmb-legend-toggle',
            from: 'From YYYY-MM-DD',
            to: 'To YYYY-MM-DD',
            apply: 'Apply dates',
            stack: 'Stacked',
        };
        const phrases: Record<string, string> = {
            metric: 'groups name which providers',
            chart: 'Left and Right to move between visible dates',
            utc: 'UTC boundaries for window buckets',
            legend: 'Double-click or press I',
            from: 'Sets the first day',
            to: 'Sets the last day',
            apply: 'Loads the selected date range',
            stack: 'one daily column',
        };
        const target = (actor: TestActor) =>
            actor.accessible_name === names[mode] ||
            buttonLabel(actor) === names[mode] ||
            actor.has_style_class_name?.(required(names[mode])) ||
            actor.hint_text === names[mode];
        await waitFor(() => findActor(indicator.menu.box, target), `History ${mode} control did not render`);
        if (mode === 'utc' || mode === 'legend') {
            const scroll = required(findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-scroll')));
            const adjustment = instance(scroll, St.ScrollView).get_vadjustment();
            await waitFor(() => adjustment.upper > adjustment.page_size, `History ${mode} scroll was not allocated`);
            adjustment.set_value(adjustment.upper - adjustment.page_size);
        }
        await waitFor(
            () => {
                const control = findActor(indicator.menu.box, target);
                const focus = mode === 'from' || mode === 'to' ? instance(control, St.Entry).clutter_text : control;
                if (!focus?.has_key_focus()) focus?.grab_key_focus();
                return findActor(
                    Main.layoutManager.uiGroup,
                    actor =>
                        actor.has_style_class_name?.('tmb-tooltip') &&
                        actor.visible &&
                        findActor(actor, child => child.text?.includes(required(phrases[mode]))),
                );
            },
            () =>
                `History ${mode} help did not open: focus=${
                    optionalInstance(findActor(indicator.menu.box, target), St.Entry)?.clutter_text.has_key_focus() ??
                    findActor(indicator.menu.box, target)?.has_key_focus()
                } ` +
                `tooltip=${findActor(Main.layoutManager.uiGroup, actor => actor.has_style_class_name?.('tmb-tooltip'))?.visible}`,
        );
        const screenshots = GLib.getenv('TMB_SCREENSHOT_DIR');
        if (screenshots && mode === 'metric') {
            await new Promise<void>(resolve => {
                const signal = global.stage.connect('after-paint', () => {
                    global.stage.disconnect(signal);
                    resolve();
                });
                global.stage.queue_redraw();
            });
            GLib.mkdir_with_parents(screenshots, 0o755);
            const stream = Gio.File.new_for_path(GLib.build_filenamev([screenshots, 'history-help.png'])).replace(
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
        indicator.menu.close(false);
        await waitFor(
            () =>
                !findActor(
                    Main.layoutManager.uiGroup,
                    actor => actor.has_style_class_name?.('tmb-tooltip') && actor.visible,
                ),
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
