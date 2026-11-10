import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import St from 'gi://St';
import * as BoxPointer from 'resource:///org/gnome/shell/ui/boxpointer.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {QuickMenuToggle} from 'resource:///org/gnome/shell/ui/quickSettings.js';
import type {PanelState} from '../../src/service-state.js';
import {statusIconTone} from '../../src/status-presentation.js';
import type {TestActor} from '../support/actors.js';
import {buttonLabel, clickButton, indicator as getIndicator, instance, required} from '../support/actors.js';
import {readProvider} from '../support/dbus.js';
import * as fixtures from '../support/state.js';
import {run as runFormats} from './shell-smoke/formats.js';
import {
    afterPaint,
    capture,
    containsText,
    findActor,
    serviceState,
    setSetting,
    UUID,
    waitOrFail,
} from './shell-smoke/helpers.js';
import {run as runHistory} from './shell-smoke/history.js';
import {run as runLifecycle} from './shell-smoke/lifecycle.js';
import {run as runSettings} from './shell-smoke/settings.js';
import {run as runUsage} from './shell-smoke/usage.js';

export function init() {}

async function runScenario() {
    const demoPhase = GLib.getenv('TMB_DEMO_PHASE');
    const source = Main.extensionManager.lookup(UUID).dir.get_uri();
    const {createChart} = (await import(`${source}/src/chart.js`)) as typeof import('../../src/chart.js');
    const {usageColor, usageStatusColor, usageTextColor} = (await import(
        `${source}/src/color.js`
    )) as typeof import('../../src/color.js');
    const {SnapshotWidget} = (await import(`${source}/src/widget.js`)) as typeof import('../../src/widget.js');
    const staticChart = createChart([], 0, 1, new Set());
    if (staticChart.reactive || !staticChart.accessible_name.includes('0 series'))
        throw new Error('A static History chart accepted input');
    staticChart.destroy();
    const emptyChart = createChart([], 0, 1, new Set(), 'usage', false, null, false, () => {});
    if (emptyChart.reactive) throw new Error('A History chart without readings accepted input');
    emptyChart.destroy();
    const emptyCountChart = createChart([], 0, 1, new Set(), 'turns', false, null, false, null, null, 'count');
    if (!emptyCountChart.accessible_name.includes('Y-axis 0 to 1'))
        throw new Error('An empty count chart has no readable axis');
    emptyCountChart.destroy();
    const indicator = getIndicator();
    if (!indicator) throw new Error('Quotaline indicator was not installed');
    const demoState = GLib.getenv('TMB_EXPECT_DEMO') === '1' ? serviceState() : null;
    const addSeconds = GLib.timeout_add_seconds;
    if (demoPhase !== 'settings' && demoPhase !== 'formats') {
        const quick = required(Main.panel.statusArea.quickSettings);
        quick.menu.open(BoxPointer.PopupAnimation.NONE);
        await waitOrFail(
            () => containsText(quick.menu.box, 'Quotaline'),
            'Snapshot widget is missing from Quick Settings',
        );
        const widget = findActor(quick.menu.box, actor => actor.title === 'Quotaline' && actor.menu, QuickMenuToggle);
        if (!widget) throw new Error('Snapshot widget has no Quick Settings menu');
        widget.menu.open(BoxPointer.PopupAnimation.NONE);
        await waitOrFail(
            () =>
                widget.menu.actor.visible &&
                widget.menu.actor.height >= 40 &&
                widget.menu.box.opacity === 255 &&
                containsText(widget.menu.box, 'small snapshot'),
            'Small snapshot is not visible',
        );
        if (
            !findActor(
                widget.menu.box,
                actor =>
                    actor instanceof St.Icon &&
                    actor.gicon instanceof Gio.FileIcon &&
                    actor.gicon.get_file().get_basename() === 'quotaline-symbolic.svg',
            )
        )
            throw new Error('Snapshot header does not use the app icon');
        if (GLib.getenv('TMB_EXPECT_DEMO') === '1') {
            if (!containsText(widget.menu.box, 'updated '))
                throw new Error('Snapshot header omits its last refresh age');
            if (
                widget.menu.box
                    .get_children()
                    .filter((actor: TestActor) => actor.has_style_class_name?.('tmb-widget-row')).length !== 3
            )
                throw new Error('Small snapshot must show three quota rows');
            await capture('widget-small');
            for (const [before, after, row, screenshot] of [
                ['small', 'medium', 'Sonnet', 'widget-medium'],
                ['medium', 'large', 'Gemini 2.5 Pro', 'widget-large'],
            ] as const) {
                await setSetting('widget_size', after);
                await waitOrFail(
                    () => containsText(widget.menu.box, `${after} snapshot`),
                    `${before} snapshot did not switch to ${after}`,
                );
                if (
                    !containsText(widget.menu.box, row) ||
                    widget.menu.box
                        .get_children()
                        .filter((actor: TestActor) => actor.has_style_class_name?.('tmb-widget-row')).length !==
                        (after === 'medium' ? 4 : 8)
                )
                    throw new Error(`${after} snapshot did not show its extra row`);
                await afterPaint(() => {});
                await capture(screenshot);
            }
            const largeRows = widget.menu.box
                .get_children()
                .filter((actor: TestActor) => actor.has_style_class_name?.('tmb-widget-row'));
            if (
                largeRows.some(
                    (actor: TestActor) =>
                        instance(actor, St.Widget).has_style_pseudo_class('insensitive') ||
                        actor.get_paint_opacity() !== 255,
                )
            )
                throw new Error('Snapshot rows are dimmed');
            if (
                !largeRows.every((actor: TestActor) =>
                    findActor(actor, child => child.has_style_class_name?.('tmb-widget-meter')),
                )
            )
                throw new Error('Snapshot quota bars are missing');
            const firstFill = findActor(largeRows[0], actor => actor.has_style_class_name?.('tmb-widget-fill'));
            const firstValue = findActor(
                largeRows[0],
                actor => actor.has_style_class_name?.('tmb-widget-value'),
                St.Widget,
            );
            const widgetUsed = required(readProvider('claude').windows[0]).used;
            const fillWidth = Math.max(2, Math.round((292 * widgetUsed) / 100));
            if (
                !firstFill ||
                firstFill.width !== fillWidth ||
                !required(instance(firstFill, St.Widget).get_style()).includes(usageColor(widgetUsed)) ||
                !required(firstValue?.get_style()).includes(usageTextColor(widgetUsed))
            )
                throw new Error('Snapshot bar does not show the raw usage share');
            if (
                required(largeRows.at(-1)).get_transformed_position()[1] + required(largeRows.at(-1)).height >
                global.stage.height
            )
                throw new Error('Large snapshot is clipped below the screen');
            await setSetting('show_as', 'remaining');
            await waitOrFail(() => {
                const used = required(readProvider('claude').windows[0]).used;
                return (
                    containsText(widget.menu.box, `${Math.round(100 - used)}%`) &&
                    findActor(widget.menu.box, actor => actor.has_style_class_name?.('tmb-widget-fill'))?.width ===
                        Math.max(2, Math.round((292 * used) / 100))
                );
            }, 'Remaining display changed the quota bar instead of its percentage');
            await setSetting('selected_windows', []);
            await waitOrFail(
                () =>
                    containsText(widget.menu.box, 'Open Quotaline to pick windows.') &&
                    !widget.menu.box
                        .get_children()
                        .some((actor: TestActor) => actor.has_style_class_name?.('tmb-widget-row')),
                'Empty snapshot still shows quota rows',
            );
            await setSetting('has_custom_selection', false);
            await setSetting('show_as', 'used');
        }
        widget.menu.close(BoxPointer.PopupAnimation.NONE);
        const icon = new Gio.FileIcon({
            file: Main.extensionManager.lookup(UUID).dir.get_child('icons').get_child('quotaline-symbolic.svg'),
        });
        const probe = new SnapshotWidget(
            () => {},
            icon,
            () => {},
        );
        probe.toggle.menu.open(BoxPointer.PopupAnimation.NONE);
        await waitOrFail(
            () => containsText(probe.toggle.menu.box, 'Open Quotaline to start tracking.'),
            'Empty snapshot should invite first-time tracking',
        );
        const snapshot: PanelState = {
            ...fixtures.serviceState({}),
            providers: {},
            cells: [],
            status_tooltip: '',
            status_icon_tone: 'normal',
            empty_usage_title: '',
            mini_bar_groups: [],
            status_variants: [],
            settings: fixtures.settings({widget_size: 'medium', show_as: 'remaining'}),
            last_refresh: Math.floor(Date.now() / 1000) - 90,
            time_presentation: {
                refresh_age: '1 min ago',
                header_updated: '',
                header_activity: '',
                provider_statuses: {},
                provider_help: {},
                next_update_ms: null,
            },
            snapshot_rows: [
                {
                    id: 'claude:session',
                    provider: 'Claude',
                    label: 'Current session',
                    used: 87,
                    reset: 'invalid',
                    updated: 1,
                    stale: true,
                },
            ],
            snapshot_attention: false,
        };
        probe.update(snapshot);
        if (
            !containsText(probe.toggle.menu.box, 'Reset unavailable · stale') ||
            !containsText(probe.toggle.menu.box, '13%') ||
            !containsText(probe.toggle.menu.box, 'updated 1 min ago') ||
            !probe.toggle.subtitle?.includes('stale')
        )
            throw new Error('Snapshot does not show an invalid reset and stale remaining quota');
        probe.update({...snapshot, snapshot_rows: []});
        if (!containsText(probe.toggle.menu.box, 'Open Quotaline to pick windows.'))
            throw new Error('Empty selection should offer model selection');
        snapshot.snapshot_attention = true;
        probe.update(snapshot);
        if (
            !findActor(
                probe.toggle.menu.box,
                actor => actor instanceof St.Icon && actor.has_style_class_name('tmb-widget-warning') && actor.visible,
            )
        )
            throw new Error('Authentication failure has no snapshot warning icon');
        snapshot.snapshot_attention = false;
        probe.update(snapshot);
        if (findActor(probe.toggle.menu.box, actor => actor.has_style_class_name?.('tmb-widget-warning'))?.visible)
            throw new Error('Snapshot warning icon remained after authentication recovered');
        required(snapshot.snapshot_rows[0]).reset = null;
        probe.update(snapshot);
        if (!containsText(probe.toggle.menu.box, 'Reset unavailable'))
            throw new Error('Snapshot does not show an unavailable reset');
        required(snapshot.snapshot_rows[0]).reset = new Date(Date.now() - 60000).toISOString();
        probe.update(snapshot);
        if (!containsText(probe.toggle.menu.box, 'Reset due')) throw new Error('Snapshot does not show a due reset');
        required(snapshot.snapshot_rows[0]).reset = new Date(Date.now() + 2 * 86400000 + 3600000).toISOString();
        probe.update(snapshot);
        if (!containsText(probe.toggle.menu.box, 'Resets in 2d'))
            throw new Error('Snapshot does not show a multi-day reset');
        snapshot.settings.widget_size = 'unknown';
        probe.update(snapshot);
        if (
            !containsText(probe.toggle.menu.box, 'unknown snapshot') ||
            !containsText(probe.toggle.menu.box, 'Claude · Current session')
        )
            throw new Error('Unknown widget size did not use a readable fallback');
        probe.toggle.menu.close(BoxPointer.PopupAnimation.NONE);
        snapshot.settings.widget_size = 'medium';
        required(snapshot.snapshot_rows[0]).reset = new Date(Date.now() + 61000).toISOString();
        probe.update(snapshot);
        GLib.timeout_add_seconds = (priority, interval, callback) =>
            addSeconds(priority, interval === 60 ? 1 : interval, callback);
        try {
            probe.toggle.menu.open(BoxPointer.PopupAnimation.NONE);
        } finally {
            GLib.timeout_add_seconds = addSeconds;
        }
        if (!containsText(probe.toggle.menu.box, 'Resets in 1 min'))
            throw new Error('Snapshot countdown did not update while Quick Settings stayed open');
        await waitOrFail(
            () => containsText(probe.toggle.menu.box, 'Resets in < 1 min'),
            'Snapshot countdown did not update while Quick Settings stayed open',
        );
        probe.destroy();
        quick.menu.close(BoxPointer.PopupAnimation.NONE);
        clickButton(widget);
    } else {
        indicator.menu.open(false);
    }
    await waitOrFail(() => indicator.menu.isOpen, 'Quick Settings tile did not open Quotaline');
    if (!indicator.menu.isOpen) throw new Error('Quotaline menu did not open');
    await afterPaint(() => {});
    const root = indicator.menu.box.get_children().find((actor: TestActor) => actor.has_style_class_name?.('tmb-root'));
    if (!root) throw new Error('Quotaline content is missing');
    const keyboard = global.stage.context
        .get_backend()
        .get_default_seat()
        .create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
    const press = async (symbol: number, control = false) => {
        const now = GLib.get_monotonic_time();
        if (control) keyboard.notify_keyval(now, Clutter.KEY_Control_L, Clutter.KeyState.PRESSED);
        keyboard.notify_keyval(now + 1000, symbol, Clutter.KeyState.PRESSED);
        keyboard.notify_keyval(now + 2000, symbol, Clutter.KeyState.RELEASED);
        if (control) keyboard.notify_keyval(now + 3000, Clutter.KEY_Control_L, Clutter.KeyState.RELEASED);
        await afterPaint(() => {});
    };
    const footer = required(root.get_children()[2]);
    if (!['Refresh', 'Report issue', 'Quit'].every(action => containsText(footer, action)))
        throw new Error('Menu footer is missing an action');
    if (
        required(
            required(
                required(findActor(footer, actor => buttonLabel(actor) === 'Copy diagnostics')).get_parent(),
            ).get_parent(),
        ).visible
    )
        throw new Error('Settings footer actions are visible on Usage');
    const adjustment = instance(root.get_children()[1], St.ScrollView).get_vadjustment();
    const panelCells = required(findActor(indicator, actor => actor.has_style_class_name?.('tmb-panel-cells')));
    const tone = statusIconTone(serviceState().cells, serviceState().providers, serviceState().settings);
    const panelDot = findActor(indicator, actor => actor.has_style_class_name?.('tmb-app-icon-dot'));
    const panelGlyph = findActor(indicator, actor => actor.has_style_class_name?.('system-status-icon'));
    if (
        !panelDot ||
        !panelGlyph ||
        panelDot.visible !== (tone === 'attention') ||
        panelGlyph.opacity !== (tone === 'offline' ? 130 : 255)
    )
        throw new Error(`Panel mark ignored the status tone: ${tone}`);
    const openWithFastClock = () => {
        GLib.timeout_add_seconds = (priority, interval, callback) =>
            addSeconds(priority, interval === 60 ? 1 : interval, callback);
        try {
            indicator.menu.open(false);
        } finally {
            GLib.timeout_add_seconds = addSeconds;
        }
    };
    if (demoPhase !== 'settings' && demoPhase !== 'formats') {
        await runUsage({
            indicator,
            root,
            footer,
            panelCells,
            openWithFastClock,
            demoState,
            usageColor,
            usageStatusColor,
        });
        await runHistory({indicator, root, footer, openWithFastClock, press});
    }
    if (demoPhase === 'usage') return;
    if (demoPhase !== 'formats') await runSettings({root, footer, adjustment, source});
    if (demoPhase === 'settings') return;
    await runFormats({demoPhase, root, adjustment, panelCells, indicator, footer, usageColor});
    await runLifecycle({root, press, indicator});
}

export async function run() {
    try {
        await runScenario();
        const complete = GLib.getenv('TMB_SHELL_SCENARIO_DONE');
        if (complete) GLib.file_set_contents(complete, 'done');
    } catch (error) {
        console.error(`Shell scenario failed: ${error instanceof Error ? error.stack : String(error)}`);
        throw error;
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
