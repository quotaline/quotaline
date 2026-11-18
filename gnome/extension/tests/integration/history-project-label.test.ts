import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import {closeInstantly, openInstantly} from '../../src/menu-animation.js';
import type {TestActor} from '../support/actors.js';
import {failure, findActor, indicator as getIndicator, required} from '../support/actors.js';
import {waitOrFail} from '../support/shell-events.js';

function actorNames(actor: TestActor): string[] {
    return [actor.text, actor.accessible_name, ...actor.get_children().flatMap(actorNames)].filter(
        value => typeof value === 'string',
    );
}

function legendNames(actor: TestActor): string[] {
    return [
        actor.has_style_class_name?.('tmb-legend-toggle') ? actor.accessible_name : null,
        ...actor.get_children().flatMap(legendNames),
    ].filter((value): value is string => typeof value === 'string');
}

export async function run() {
    try {
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        openInstantly(indicator.menu);
        const root = findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-root'));
        await waitOrFail(
            () => findActor(root, actor => actor.has_style_class_name?.('tmb-legend-toggle')),
            `Project History did not load: ${actorNames(required(root)).join(' · ').slice(0, 2000)}`,
        );
        const names = actorNames(required(root)).join('\n');
        const privateNames = GLib.getenv('TMB_PROJECT_PRIVATE') === '1';
        const expected = required(GLib.getenv('TMB_PROJECT_LABELS')).split('|');
        const paths = required(GLib.getenv('TMB_PROJECT_PATHS')).split('|');
        const legends = legendNames(required(root));
        if (
            expected.some(name => !legends.some(legend => legend.includes(` ${name} ·`))) ||
            paths.some(path => !expected.includes(path) && names.includes(path)) ||
            (privateNames && names.includes('secret-project'))
        )
            throw new Error(`Project History labels expose paths or lose distinct projects: ${names}`);
        const directory = GLib.getenv('TMB_SCREENSHOT_DIR');
        if (directory) {
            GLib.mkdir_with_parents(directory, 0o755);
            const stream = Gio.File.new_for_path(
                GLib.build_filenamev([directory, `history-project-${GLib.getenv('TMB_PROJECT_CAPTURE')}.png`]),
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
        closeInstantly(indicator.menu);
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
