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
    required,
} from '../support/actors.js';
import {afterPaint, waitOrFail} from '../support/shell-events.js';

function accessibleNames(root: TestActor) {
    const names: string[] = [];
    const collect = (actor: TestActor) => {
        if (actor.accessible_name) names.push(actor.accessible_name);
        for (const child of actor.get_children()) collect(child);
    };
    collect(root);
    return names.join('; ');
}

async function capture(name: string) {
    const directory = GLib.getenv('TMB_SCREENSHOT_DIR');
    if (!directory) return;
    GLib.mkdir_with_parents(directory, 0o755);
    const stream = Gio.File.new_for_path(GLib.build_filenamev([directory, name])).replace(
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

export async function run() {
    try {
        Main.overview.hide();
        await waitOrFail(() => !Main.overview.visible, 'Overview stayed open during quota capture');
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        if (GLib.getenv('TMB_CENTERED')) {
            const container = indicator.container;
            required(container.get_parent()).remove_child(container);
            Main.panel._centerBox.insert_child_at_index(container, 0);
            await waitOrFail(
                () => indicator.get_transformed_position()[0] > global.stage.width / 4,
                'Cached quota panel item did not reach the center',
            );
        }
        indicator.menu.open(false);
        const root = required(findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-root')));
        const tabs = () => required(required(root.get_children()[0]).get_children()[1]).get_children();
        clickButton(required(tabs()[0]));
        await waitOrFail(
            () => findActor(root, actor => actor.accessible_name === 'Claude, ✶, 20% used'),
            `Cached quota did not reach Usage: ${accessibleNames(root)}`,
        );
        const card = findActor(
            root,
            actor =>
                actor.has_style_class_name?.('tmb-card') &&
                findActor(actor, child => child.accessible_name === 'Claude, ✶, 20% used'),
        );
        const noDuration = findActor(
            card,
            actor => actor.has_style_class_name?.('tmb-window') && actor.accessible_name === 'Claude, ✶, 20% used',
        );
        if (
            !noDuration ||
            findActor(noDuration, actor => actor.has_style_class_name?.('tmb-quota-pace')) ||
            findActor(noDuration, actor => actor.has_style_class_name?.('tmb-meter-expected'))
        )
            throw new Error('Quota without a duration showed a pace');
        const noReset = findActor(
            card,
            actor => actor.has_style_class_name?.('tmb-window') && actor.accessible_name === 'Claude, Claude, 35% used',
        );
        if (
            !noReset ||
            !findActor(
                noReset,
                actor => actor.has_style_class_name?.('tmb-quota-pace') && actor.text === 'Learning pace',
            )
        )
            throw new Error('Quota without a reset did not show unknown pace');
        const spend = findActor(card, actor => actor.has_style_class_name?.('tmb-spend'));
        if (!spend || !findActor(spend, actor => actor.text === '12.4 of 50 · 25% used'))
            throw new Error('Currency-free spend summary is missing');
        await waitOrFail(() => {
            const left = root.get_transformed_position()[0];
            return (
                root.width >= Math.min(440, global.stage.width - 32) &&
                root.width <= Math.min(880, global.stage.width - 32) &&
                left >= 0 &&
                left + root.width <= global.stage.width
            );
        }, `Cached quota did not fit ${root.width}px menu on ${global.stage.width}px monitor`);
        const size = root.width < 790 ? 'compact' : 'wide';
        await afterPaint();
        await capture(`cached-quota-${size}.png`);
        const details = findActor(root, actor => buttonLabel(actor)?.includes('Credit details'));
        if (!details) throw new Error('Credit details control disappeared after the redraw');
        clickButton(details);
        await waitOrFail(
            () => findActor(root, actor => actor.text === '$1.30'),
            'Balance without a currency did not use USD',
        );
        clickButton(required(tabs()[2]));
        await waitOrFail(
            () => findActor(root, actor => actor.accessible_name === 'Claude ✶ short label, 6 characters maximum'),
            `Symbolic window label did not reach Settings: ${accessibleNames(root)}`,
        );
        const symbol = required(
            findActor(root, actor => actor.accessible_name === 'Claude ✶ short label, 6 characters maximum'),
        );
        const plain = findActor(
            root,
            actor => actor.accessible_name === 'Claude Claude short label, 6 characters maximum',
            St.Entry,
        );
        if (instance(symbol, St.Entry).get_text() !== '✶' || plain?.get_text() !== 'CLA')
            throw new Error(`Short labels changed: ${instance(symbol, St.Entry).get_text()}, ${plain?.get_text()}`);
        await capture(`cached-settings-${size}.png`);
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
