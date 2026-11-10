import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import St from 'gi://St';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import type {TestActor} from '../support/actors.js';
import {
    buttonLabel,
    clickButton,
    contentOf,
    failure,
    findActor,
    indicator as getIndicator,
    required,
} from '../support/actors.js';
import {afterPaint, waitOrFail} from '../support/shell-events.js';

async function capture(name: string, root: TestActor) {
    const directory = GLib.getenv('TMB_SCREENSHOT_DIR');
    if (!directory) return;
    GLib.mkdir_with_parents(directory, 0o755);
    const [left, top] = root.get_transformed_position();
    const x = Math.max(0, Math.floor(left) - 8);
    const y = Math.max(0, Math.floor(top) - 8);
    const width = Math.min(global.stage.width - x, Math.ceil(root.width) + 16);
    const height = Math.min(global.stage.height - y, Math.ceil(root.height) + 8);
    if (width < 1 || height < 1)
        throw new Error(
            `Menu capture has no visible area: ${JSON.stringify([
                x,
                y,
                width,
                height,
                global.stage.width,
                global.stage.height,
                root.width,
                root.height,
            ])}`,
        );
    const stream = Gio.File.new_for_path(GLib.build_filenamev([directory, `${name}.png`])).replace(
        null,
        false,
        Gio.FileCreateFlags.REPLACE_DESTINATION,
        null,
    );
    await new Promise<void>((resolve, reject) => {
        new Shell.Screenshot().screenshot_area(x, y, width, height, stream, (source, result) => {
            try {
                required(source).screenshot_area_finish(result);
                stream.close(null);
                resolve();
            } catch (error) {
                reject(failure(error));
            }
        });
    });
}

function cardBadges(root: TestActor) {
    const card = findActor(contentOf(root.get_children()[1]), actor => actor.has_style_class_name?.('tmb-card'));
    if (!card) return null;
    const badges: string[] = [];
    const collect = (actor: TestActor) => {
        if (actor.has_style_class_name?.('tmb-card-header-badges'))
            badges.push(...actor.get_children().map(child => required(child.text ?? buttonLabel(child))));
        else actor.get_children().forEach(collect);
    };
    collect(card);
    return badges;
}

export async function run() {
    try {
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        const centered = Boolean(GLib.getenv('TMB_CENTERED'));
        if (centered) {
            const container = indicator.container;
            required(container.get_parent()).remove_child(container);
            Main.panel._centerBox.insert_child_at_index(container, 0);
            await waitOrFail(
                () => indicator.get_transformed_position()[0] > global.stage.width / 4,
                'Organization panel item did not reach the center',
            );
        }
        indicator.menu.open(false);
        const root = required(findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-root')));
        if (centered)
            await waitOrFail(() => root.width >= 790, 'Centered organization card did not use the wide layout');
        const compact = root.width < 560;
        await waitOrFail(
            () => cardBadges(root)?.includes('Example Team'),
            'Claude organization chip did not reach Usage',
        );
        if (required(cardBadges(root)).join() !== 'Pro,profile@example.com,Example Team')
            throw new Error(`Claude account chips are out of order: ${JSON.stringify(cardBadges(root))}`);
        clickButton(required(findActor(root, actor => buttonLabel(actor) === 'Example Team')));
        const copiedWorkspace = await new Promise<string>(resolve => {
            St.Clipboard.get_default().get_text(St.ClipboardType.CLIPBOARD, (_clipboard, value) => {
                resolve(value);
            });
        });
        if (copiedWorkspace !== 'Example Team')
            throw new Error('Claude workspace chip did not copy its displayed value');
        await waitOrFail(
            () => global.stage.width > 100 && global.stage.height > 100 && root.width > 100 && root.height > 100,
            'Claude Usage menu was not allocated',
        );
        await afterPaint();
        await capture(
            compact
                ? 'claude-organization-compact-menu'
                : centered
                  ? 'claude-organization-wide-menu'
                  : 'claude-organization-menu',
            root,
        );
        clickButton(required(required(required(root.get_children()[0]).get_children()[1]).get_children()[2]));
        const privacy = findActor(
            contentOf(required(root.get_children()[1])),
            actor => actor.accessible_name === 'Hide account and project details',
        );
        if (!privacy) throw new Error('Privacy control is missing');
        clickButton(privacy);
        clickButton(required(required(required(root.get_children()[0]).get_children()[1]).get_children()[0]));
        await waitOrFail(
            () => cardBadges(root)?.join() === 'Pro,account,workspace',
            `Privacy did not mask both chips: ${JSON.stringify(cardBadges(root))}`,
        );
        clickButton(required(findActor(root, actor => buttonLabel(actor) === 'account')));
        const copiedAccount = await new Promise<string>(resolve => {
            St.Clipboard.get_default().get_text(St.ClipboardType.CLIPBOARD, (_clipboard, value) => {
                resolve(value);
            });
        });
        if (copiedAccount !== 'account') throw new Error('Private Claude account chip copied the original identity');
        await capture(
            compact ? 'claude-masked-compact-menu' : centered ? 'claude-masked-wide-menu' : 'claude-masked-menu',
            root,
        );
        clickButton(required(required(required(root.get_children()[0]).get_children()[1]).get_children()[2]));
        clickButton(
            required(
                findActor(
                    contentOf(required(root.get_children()[1])),
                    actor => actor.accessible_name === 'Hide account and project details',
                ),
            ),
        );
        clickButton(required(required(required(root.get_children()[0]).get_children()[1]).get_children()[0]));
        await waitOrFail(
            () => cardBadges(root)?.join() === 'Pro,profile@example.com,Example Team',
            'Restoring privacy did not restore account chips',
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
