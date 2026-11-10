import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import St from 'gi://St';
import * as BoxPointer from 'resource:///org/gnome/shell/ui/boxpointer.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {QuickMenuToggle} from 'resource:///org/gnome/shell/ui/quickSettings.js';
import {
    clickButton,
    failure,
    findActor,
    indicator as getIndicator,
    instance,
    optionalInstance,
    required,
} from '../support/actors.js';
import {waitOrFail} from '../support/shell-events.js';

async function capture(name: string) {
    const directory = GLib.getenv('TMB_SCREENSHOT_DIR');
    if (!directory) return;
    GLib.mkdir_with_parents(directory, 0o755);
    const stream = Gio.File.new_for_path(GLib.build_filenamev([directory, `${name}.png`])).replace(
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
        await waitOrFail(() => !Main.overview.visible, 'GNOME overview did not close');
        const light = GLib.getenv('TMB_THEME_MODE') === 'light';
        if (light) {
            new Gio.Settings({schema_id: 'org.gnome.desktop.interface'}).set_string('color-scheme', 'prefer-light');
            await waitOrFail(
                () => St.Settings.get().color_scheme === St.SystemColorScheme.PREFER_LIGHT,
                'Light Shell theme did not activate',
            );
        }
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        const quick = required(Main.panel.statusArea.quickSettings);
        quick.menu.open(BoxPointer.PopupAnimation.NONE);
        const widget = required(
            findActor(quick.menu.box, actor => actor.title === 'Quotaline' && actor.menu, QuickMenuToggle),
        );
        widget.menu.open(BoxPointer.PopupAnimation.NONE);
        await waitOrFail(
            () =>
                widget.menu.box.opacity === 255 &&
                findActor(
                    widget.menu.box,
                    actor =>
                        actor.has_style_class_name?.('tmb-widget-value') &&
                        required(instance(actor, St.Widget).get_style()).includes(light ? '#4b4b4b' : '#d8d8da'),
                ) &&
                findActor(
                    widget.menu.box,
                    actor =>
                        actor.has_style_class_name?.('tmb-widget-fill') &&
                        required(instance(actor, St.Widget).get_style()).includes('#808080'),
                ),
            'Stale snapshot values or meters are not grey',
        );
        quick.menu.close(BoxPointer.PopupAnimation.NONE);
        instance(indicator, St.Widget).set_hover(true);
        await waitOrFail(
            () =>
                findActor(
                    Main.layoutManager.uiGroup,
                    actor =>
                        actor.has_style_class_name?.('tmb-tooltip') &&
                        actor.visible &&
                        findActor(
                            actor,
                            child => child.text?.includes('Claude') && !child.text.includes('more limits'),
                        ),
                ),
            'One retained limit did not get a compact panel summary',
        );
        instance(indicator, St.Widget).set_hover(false);
        indicator.menu.open(false);
        const root = indicator.menu.box;
        const menu = required(findActor(root, actor => actor.has_style_class_name?.('tmb-root')));
        if (!indicator.has_allocation() && (menu.width < 280 || menu.width > global.stage.width - 32))
            throw new Error(`Recovery menu opened before panel allocation at ${menu.width}px`);
        const actionName = GLib.getenv('TMB_RECOVERY_ACTION');
        const captureName = GLib.getenv('TMB_RECOVERY_CAPTURE');
        const summary = GLib.getenv('TMB_RECOVERY_SUMMARY');
        const detail = GLib.getenv('TMB_RECOVERY_DETAIL');
        if (!actionName || !captureName || !summary || !detail) throw new Error('Recovery fixture action is missing');
        await waitOrFail(
            () =>
                findActor(root, actor => actor.accessible_name === actionName) &&
                findActor(
                    root,
                    actor =>
                        actor.has_style_class_name?.('tmb-card-updated') &&
                        actor.text?.startsWith(
                            actionName === 'Sign in to Claude' ? 'Sign-in required · ' : 'Unavailable · ',
                        ),
                ) &&
                findActor(root, actor => actor.has_style_class_name?.('tmb-card-meta') && actor.text === summary) &&
                findActor(root, actor => actor.has_style_class_name?.('tmb-quota-value') && actor.text === '42%'),
            `Stale Claude card lost its reading or ${actionName} action`,
        );
        await waitOrFail(() => {
            if (!indicator.has_allocation()) return false;
            const monitor = required(Main.layoutManager.primaryMonitor);
            const x = menu.get_transformed_position()[0];
            const center = indicator.get_transformed_position()[0] + indicator.width / 2;
            return (
                menu.width >= 280 &&
                menu.width <= Math.min(880, monitor.width - 32) &&
                x >= monitor.x &&
                x + menu.width <= monitor.x + monitor.width &&
                center >= x - 20 &&
                center <= x + menu.width + 20
            );
        }, `Recovery menu did not fit the available monitor width: ${menu.width}px`);
        const panelCells = findActor(indicator, actor => actor.has_style_class_name?.('tmb-panel-cells'));
        if (!panelCells) throw new Error('Stale Claude panel value is not grey');
        await waitOrFail(
            () =>
                findActor(
                    panelCells,
                    actor =>
                        actor.has_style_class_name?.('tmb-panel-value') &&
                        required(instance(actor, St.Widget).get_style()).includes(light ? '#4b4b4b' : '#d8d8da'),
                ),
            'Stale Claude panel value is not grey',
        );
        await capture(`stale-claude-${captureName}-card`);
        Gio.DBus.session.call_sync(
            'tech.bernat.Quotaline.Gnome',
            '/tech/bernat/Quotaline/Gnome',
            'tech.bernat.Quotaline.Gnome',
            'SetSetting',
            new GLib.Variant('(ss)', ['format', '"mini-bars"']),
            new GLib.VariantType('(s)'),
            Gio.DBusCallFlags.NONE,
            2000,
            null,
        );
        await waitOrFail(
            () =>
                findActor(
                    panelCells,
                    actor =>
                        actor.has_style_class_name?.('tmb-panel-mini-fill') &&
                        required(instance(actor, St.Widget).get_style()).includes('#808080'),
                ),
            'Stale Claude mini bar is not grey',
        );
        await capture(`stale-claude-${captureName}-mini-bars`);
        if (actionName === 'Sign in to Claude') {
            const setup = required(findActor(root, actor => actor.accessible_name === 'Set up Claude'));
            if (!optionalInstance(setup, St.Widget)?.has_style_class_name('tmb-icon-button'))
                throw new Error('Stale Claude card lost its compact setup action');
            clickButton(setup);
            await waitOrFail(
                () =>
                    findActor(
                        root,
                        actor =>
                            actor.accessible_name === 'Settings' && actor.has_style_class_name?.('tmb-tab-selected'),
                    ) && findActor(root, actor => actor.accessible_name === 'Connection details for Claude'),
                'Claude setup action did not open provider controls',
            );
            clickButton(
                required(
                    findActor(
                        root,
                        actor => actor.accessible_name === 'Usage' && actor.has_style_class_name?.('tmb-tab'),
                    ),
                ),
            );
            await waitOrFail(
                () => findActor(root, actor => actor.accessible_name === actionName),
                'Claude setup action did not return to the Usage card',
            );
        }
        clickButton(required(findActor(root, actor => actor.accessible_name === actionName)));
        await waitOrFail(
            () =>
                findActor(
                    root,
                    actor => actor.accessible_name === 'Settings' && actor.has_style_class_name?.('tmb-tab-selected'),
                ) && findActor(root, actor => actor.accessible_name === 'Connection details for Claude'),
            `Claude card ${actionName} action did not open provider controls`,
        );
        await waitOrFail(() => {
            const control = findActor(root, actor => actor.accessible_name === 'Sign in to Claude');
            const scroll = findActor(root, actor => actor.has_style_class_name?.('tmb-scroll'));
            const issue = findActor(
                root,
                actor =>
                    actor.has_style_class_name?.('tmb-provider-detail') &&
                    actor.text?.startsWith('Issue · ') &&
                    actor.text.includes(detail),
            );
            if (!control || !scroll || !issue) return false;
            const y = control.get_transformed_position()[1];
            const scrollY = scroll.get_transformed_position()[1];
            return y >= scrollY && y + control.height <= scrollY + scroll.height;
        }, 'Claude recovery details or controls are missing from Settings');
        await capture(`stale-claude-${captureName}-settings`);
        if (actionName === 'Set up Claude') {
            clickButton(
                required(
                    findActor(
                        root,
                        actor => actor.accessible_name === 'Usage' && actor.has_style_class_name?.('tmb-tab'),
                    ),
                ),
            );
            await waitOrFail(
                () => findActor(root, actor => actor.accessible_name === actionName),
                'Claude setup action did not return with the Usage card',
            );
            clickButton(required(findActor(root, actor => actor.accessible_name === actionName)));
            if (!Main.extensionManager.disableExtension('quotaline@bernat.tech') || getIndicator())
                throw new Error('Disabling during setup navigation left the panel item behind');
        } else {
            indicator.menu.close(false);
        }
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
