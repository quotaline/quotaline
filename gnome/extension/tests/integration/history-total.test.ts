import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import {
    accessibleName,
    buttonLabel,
    clickButton,
    containsText,
    failure,
    findActor,
    indicator as getIndicator,
    required,
} from '../support/actors.js';
import {waitOrFail} from '../support/shell-events.js';

export async function run() {
    try {
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        indicator.menu.open(false);
        const root = findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-root'));
        if (!root) throw new Error('Quotaline menu is missing');
        clickButton(required(findActor(root, actor => actor.accessible_name === 'History')));
        const legend = (name: string) =>
            findActor(
                root,
                actor =>
                    actor.has_style_class_name?.('tmb-legend-toggle') && accessibleName(actor)?.includes(` ${name} ·`),
            );
        await waitOrFail(
            () =>
                containsText(root, '8 total') &&
                legend('cli') &&
                legend('web') &&
                legend('Surface · cli') &&
                legend('Model · other'),
            'Turns history lost a day without a total or double-counted its breakdown',
        );
        if (legend('total')) throw new Error('Turns history drew a total alongside its detailed series');
        if (
            !required(legend('cli')).accessible_name.includes(' · 2 ·') ||
            !required(legend('Surface · cli')).accessible_name.includes(' · 4 ·')
        )
            throw new Error('Count values do not match the macOS compact legend format');
        clickButton(required(legend('Surface · cli')));
        await waitOrFail(
            () => containsText(root, '8 total') && legend('Surface · cli')?.accessible_name.startsWith('☐'),
            'Hiding a breakdown changed the canonical count',
        );
        const directory = GLib.getenv('TMB_SCREENSHOT_DIR');
        if (directory) {
            GLib.mkdir_with_parents(directory, 0o755);
            const stream = Gio.File.new_for_path(GLib.build_filenamev([directory, 'history-turns-total.png'])).replace(
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
        const next = () => findActor(root, actor => buttonLabel(actor) === 'Next reading');
        clickButton(required(next()));
        await waitOrFail(
            () =>
                legend('cli')?.accessible_name.includes(' · — ·') &&
                legend('Surface · cli')?.accessible_name.includes(' · 4 ·'),
            'Inspecting yesterday did not mark the absent current-day series',
        );
        clickButton(required(next()));
        await waitOrFail(
            () =>
                legend('Surface · cli')?.accessible_name.includes(' · — ·') &&
                legend('cli')?.accessible_name.includes(' · 2 ·'),
            'Inspecting today did not mark the absent previous-day series',
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
