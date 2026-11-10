import GLib from 'gi://GLib';
import St from 'gi://St';
import {buttonLabel, clickButton, findActor, indicator as getIndicator, instance, required} from '../support/actors.js';
import {waitOrFail} from '../support/shell-events.js';

export async function run() {
    try {
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        indicator.menu.open(false);
        const root = findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-root'));
        if (!root) throw new Error('Quotaline menu is missing');
        clickButton(required(findActor(root, actor => actor.accessible_name === 'History')));
        const range = () => findActor(root, actor => actor.has_style_class_name?.('tmb-history-path'))?.text;
        await waitOrFail(() => range()?.length, 'History did not load its current period');
        const period = (text: string) => findActor(root, actor => buttonLabel(actor) === text);
        if (
            !instance(required(period('Now')), St.Widget).has_style_class_name('tmb-tab-selected') ||
            instance(required(period('Today')), St.Widget).has_style_class_name('tmb-tab-selected')
        )
            throw new Error('History did not start in the live Today period');
        clickButton(required(period('Day')));
        await waitOrFail(
            () => instance(required(period('Now')), St.Widget).has_style_class_name('tmb-tab-selected'),
            'Daily rollup stopped the live History period',
        );
        const before = range();
        clickButton(required(period('7d')));
        await waitOrFail(
            () =>
                instance(required(period('7d')), St.Widget).has_style_class_name('tmb-tab-selected') &&
                !instance(required(period('Now')), St.Widget).has_style_class_name('tmb-tab-selected') &&
                range() !== before,
            'Seven-day History period did not anchor its end',
        );
        const current = range();
        clickButton(required(findActor(root, actor => buttonLabel(actor) === '←')));
        await waitOrFail(() => range() && range() !== current, 'History did not page backward');
        clickButton(required(findActor(root, actor => buttonLabel(actor) === '→')));
        await waitOrFail(() => range() === current, `History did not return to the current period: ${range()}`);
        clickButton(required(period('Now')));
        await waitOrFail(
            () =>
                instance(required(period('Now')), St.Widget).has_style_class_name('tmb-tab-selected') &&
                !instance(required(period('7d')), St.Widget).has_style_class_name('tmb-tab-selected'),
            'History did not resume following Now',
        );
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
