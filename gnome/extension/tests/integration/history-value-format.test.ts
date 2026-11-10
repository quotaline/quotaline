import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Pango from 'gi://Pango';
import Shell from 'gi://Shell';
import St from 'gi://St';
import {
    buttonLabel,
    clickButton,
    failure,
    findActor,
    indicator as getIndicator,
    instance,
    required,
} from '../support/actors.js';
import {waitOrFail} from '../support/shell-events.js';

export async function run() {
    try {
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        indicator.menu.open(false);
        const root = findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-root'));
        const metric = GLib.getenv('TMB_HISTORY_FORMAT');
        const title = metric === 'costUSD' ? 'API-equivalent cost' : 'Input tokens';
        const picker = findActor(root, actor => actor.accessible_name === 'Metric');
        if (!picker) throw new Error('History metric picker is missing');
        if (!instance(picker, St.Button).get_label().startsWith(title)) {
            clickButton(picker);
            const choice = findActor(root, actor => actor.accessible_name === `Choose metric: ${metric}`);
            if (!choice) throw new Error(`History metric ${metric} is missing`);
            clickButton(choice);
        }
        await waitOrFail(
            () =>
                findActor(root, actor => actor.accessible_name === 'Metric' && buttonLabel(actor)?.startsWith(title)) &&
                findActor(root, actor => actor.has_style_class_name?.('tmb-history-intro')) &&
                findActor(root, actor => actor.has_style_class_name?.('tmb-legend-toggle')),
            `${title} History did not load`,
        );
        const summary = required(findActor(root, actor => actor.has_style_class_name?.('tmb-history-intro'))).text;
        const legend = required(
            findActor(root, actor => actor.has_style_class_name?.('tmb-legend-toggle')),
        ).accessible_name;
        const axis = required(findActor(root, actor => actor.has_style_class_name?.('tmb-chart'))).accessible_name;
        if (metric === 'costUSD') {
            if (
                !required(summary).startsWith('$') ||
                !required(summary).endsWith(' total') ||
                !legend.startsWith('☑ fable · $') ||
                required(summary).includes('USD') ||
                legend.includes('USD')
            )
                throw new Error(`History cost values are not currency: ${summary}; ${legend}`);
            if (!/Y-axis \$0\.00 to \$[0-9]+\.[0-9]{2}/.test(axis))
                throw new Error(`History cost axis is not currency: ${axis}`);
            const value = required(findActor(root, actor => actor.has_style_class_name?.('tmb-legend-value')));
            if (
                value.width < value.get_preferred_width(-1)[1] ||
                instance(value, St.Label).clutter_text.get_ellipsize() !== Pango.EllipsizeMode.NONE
            )
                throw new Error('History currency value is clipped');
        } else if (
            !/[0-9](?:\.[0-9])?[KMB] tokens total$/.test(required(summary)) ||
            !/^☑ Claude · fable · [0-9](?:\.[0-9])?[KMB] · /.test(legend)
        ) {
            throw new Error(`History token values are not compact: ${summary}; ${legend}`);
        } else if (!/Y-axis 0 to [0-9]+(?:\.[0-9])?[KMB]/.test(axis)) {
            throw new Error(`History token axis is not compact: ${axis}`);
        }
        const directory = GLib.getenv('TMB_SCREENSHOT_DIR');
        if (directory) {
            GLib.mkdir_with_parents(directory, 0o755);
            const stream = Gio.File.new_for_path(
                GLib.build_filenamev([directory, `history-format-${metric}.png`]),
            ).replace(null, false, Gio.FileCreateFlags.REPLACE_DESTINATION, null);
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
