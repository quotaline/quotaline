import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import St from 'gi://St';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {buttonLabel, failure, findActor, indicator as getIndicator, instance, required} from '../support/actors.js';
import {waitOrFail} from '../support/shell-events.js';

function afterPaint(action: () => void) {
    return new Promise<void>(resolve => {
        const signal = global.stage.connect('after-paint', () => {
            global.stage.disconnect(signal);
            resolve();
        });
        action();
        global.stage.queue_redraw();
    });
}

function waitFor(
    check: () => unknown,
    failure: string | (() => string) = 'Menu content did not reach the expected layout',
) {
    return waitOrFail(check, failure);
}

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

async function runScenario() {
    Main.overview.hide();
    const indicator = getIndicator();
    if (!indicator) throw new Error('Quotaline indicator is missing');
    const container = indicator.container;
    const rightBox = required(container.get_parent());
    const position = rightBox.get_children().indexOf(container);
    const centerBox = Main.panel._centerBox;
    try {
        await afterPaint(() => {
            rightBox.remove_child(container);
            centerBox.insert_child_at_index(container, 0);
        });
        await afterPaint(() => {
            indicator.menu.open(false);
        });
        const root = required(indicator.menu.box.get_children()[0]);
        if (GLib.getenv('TMB_CENTERED_COMPACT')) {
            await afterPaint(() => {
                required(
                    findActor(
                        root,
                        actor => actor.accessible_name === 'History' && actor.has_style_class_name?.('tmb-tab'),
                    ),
                ).emit('clicked', 1);
            });
            await waitFor(() => {
                const chart = findActor(root, actor => actor.has_style_class_name?.('tmb-chart'));
                const legend = findActor(root, actor => actor.has_style_class_name?.('tmb-history-legend'));
                const columns =
                    legend && findActor(legend, actor => actor.has_style_class_name?.('tmb-history-legend-columns'));
                return (
                    root.width >= 440 &&
                    root.width < 888 &&
                    chart?.has_allocation() &&
                    chart.width >= root.width - 50 &&
                    legend?.has_allocation() &&
                    legend.get_transformed_position()[1] >= chart.get_transformed_position()[1] + chart.height &&
                    columns?.get_n_children() === 2
                );
            });
            await capture('menubar-centered-medium-history-popover');
            // The clock shares the center box, so its width would decide how far the indicator sits off center.
            await afterPaint(() => {
                required(Main.panel.statusArea.dateMenu).container.hide();
            });
            await afterPaint(() => {
                required(
                    findActor(
                        root,
                        actor => actor.accessible_name === 'Settings' && actor.has_style_class_name?.('tmb-tab'),
                    ),
                ).emit('clicked', 1);
            });
            await waitFor(
                () =>
                    root.width >= 650 &&
                    root.width < 790 &&
                    findActor(root, actor => actor.has_style_class_name?.('tmb-provider-setting-status'))?.width ===
                        180,
                'Compact Settings did not size provider status between narrow and wide',
            );
            return;
        }
        await waitFor(
            () => root.width >= 790 && findActor(root, actor => actor.has_style_class_name?.('tmb-cost-summary')),
        );
        const rootX = root.get_transformed_position()[0];
        const indicatorX = indicator.get_transformed_position()[0];
        const difference = Math.abs(rootX + root.width / 2 - indicatorX - indicator.width / 2);
        if (difference > 20) throw new Error(`Menu is ${difference}px away from the centered panel content`);
        const claude = findActor(
            root,
            actor => actor.has_style_class_name?.('tmb-card') && findActor(actor, child => child.text === 'Claude'),
        );
        const top = findActor(claude, actor => actor.has_style_class_name?.('tmb-card-header-top'));
        const badges = findActor(top, actor => actor.has_style_class_name?.('tmb-card-header-badges'));
        const costs = findActor(claude, actor => actor.has_style_class_name?.('tmb-cost-summary'));
        const antigravity = findActor(
            root,
            actor =>
                actor.has_style_class_name?.('tmb-card') && findActor(actor, child => child.text === 'Antigravity'),
        );
        if (
            root.width < 790 ||
            !badges ||
            !findActor(badges, actor => actor.text === 'Max 20x') ||
            !findActor(badges, actor => actor.text === 'you@example.com') ||
            !findActor(top, actor => actor.text?.startsWith('fetched')) ||
            !findActor(antigravity, actor => actor.text === 'gemini:session') ||
            !findActor(claude, actor => actor.text === 'window · 5h') ||
            !findActor(costs, actor => actor.text === 'CLAUDE API-EQUIVALENT ESTIMATES · DAILY UTC') ||
            !['Today (UTC)', 'Yesterday (UTC)', 'Last 30 days'].every(title =>
                findActor(costs, actor => actor.text === title),
            )
        )
            throw new Error('Centered wide Usage menu lost its freshness or cost layout');
        const titleY = required(
            findActor(claude, actor => actor.text === 'Current session'),
        ).get_transformed_position()[1];
        const detailY = required(
            findActor(claude, actor => actor.text === 'window · 5h'),
        ).get_transformed_position()[1];
        if (detailY <= titleY || detailY - titleY > 20)
            throw new Error('Quota window detail is not directly below its title');
        await capture('menubar-centered-popover');
        for (const tab of ['History', 'Settings'] as const) {
            await afterPaint(() => {
                required(
                    findActor(root, actor => actor.accessible_name === tab && actor.has_style_class_name?.('tmb-tab')),
                ).emit('clicked', 1);
            });
            await waitFor(() =>
                findActor(root, actor =>
                    actor.has_style_class_name?.(tab === 'History' ? 'tmb-history-visual' : 'tmb-settings-section'),
                ),
            );
            if (required(findActor(root, actor => actor.has_style_class_name?.('tmb-page'))).width > root.width)
                throw new Error(`${tab} content exceeds the centered wide menu`);
            if (tab === 'History') {
                await waitFor(() => {
                    const chart = findActor(root, actor => actor.has_style_class_name?.('tmb-chart'));
                    const legend = findActor(root, actor => actor.has_style_class_name?.('tmb-history-legend'));
                    return (
                        root.width >= 875 &&
                        chart?.has_allocation() &&
                        chart.width >= 580 &&
                        legend?.has_allocation() &&
                        legend.get_transformed_position()[0] >= chart.get_transformed_position()[0] + chart.width
                    );
                });
            }
            await capture(`menubar-centered-${tab.toLowerCase()}-popover`);
        }
        const monitor = required(Main.layoutManager.primaryMonitor);
        const originalWidth = monitor.width;
        const panelCells = required(findActor(indicator, actor => actor.has_style_class_name?.('tmb-panel-cells')));
        const statusCells = () =>
            panelCells.get_children().filter(actor => actor.has_style_class_name?.('tmb-panel-cell'));
        const panelIcon = required(required(panelCells.get_parent()).get_children()[0]);
        try {
            monitor.width = 900;
            Main.layoutManager.emit('monitors-changed');
            await waitFor(
                () =>
                    root.width >= 280 &&
                    root.width <= Math.min(880, monitor.width - 32) &&
                    (statusCells().length > 0 || panelIcon.visible) &&
                    panelCells.get_preferred_width(-1)[1] <= Math.floor(monitor.width * 0.35) &&
                    findActor(root, actor => actor.has_style_class_name?.('tmb-settings-section')),
                () =>
                    `Resized monitor kept a ${root.width}px Settings menu, ${statusCells().length} cells, ` +
                    `${panelIcon.visible ? 'visible' : 'hidden'} icon, and ${panelCells.get_preferred_width(-1)[1]}px status width`,
            );
        } finally {
            monitor.width = originalWidth;
            Main.layoutManager.emit('monitors-changed');
            await waitFor(
                () => root.width >= 790 && statusCells().length > 0,
                () => `Restored monitor kept a ${root.width}px Settings menu and ${statusCells().length} panel cells`,
            );
        }
        try {
            monitor.width = 320;
            Main.layoutManager.emit('monitors-changed');
            await waitFor(
                () => {
                    const footer = root.get_children()[2];
                    const refresh = findActor(footer, actor => buttonLabel(actor)?.includes('Refresh'));
                    const quit = findActor(footer, actor => actor.accessible_name === 'Quit');
                    return (
                        root.width < 440 &&
                        refresh &&
                        quit &&
                        Math.abs(refresh.get_transformed_position()[1] - quit.get_transformed_position()[1]) <= 4
                    );
                },
                () => 'Monitor shrink left Quit on a separate footer row',
            );
        } finally {
            monitor.width = originalWidth;
            Main.layoutManager.emit('monitors-changed');
            await waitFor(
                () => root.width >= 790,
                () => `Restored monitor kept a ${root.width}px menu`,
            );
        }
        try {
            Main.layoutManager.primaryMonitor = null;
            required(
                findActor(root, actor => actor.accessible_name === 'Usage' && actor.has_style_class_name?.('tmb-tab')),
            ).emit('clicked', 1);
            const fallback = {x: 0, width: Math.max(800, global.stage.width)};
            if (root.width < 280 || root.width > Math.min(880, fallback.width - 32))
                throw new Error('Menu lost its width while the primary monitor was unavailable');
        } finally {
            Main.layoutManager.primaryMonitor = monitor;
        }
        indicator.menu.close(false);
    } finally {
        required(Main.panel.statusArea.dateMenu).container.show();
        if (container.get_parent()) required(container.get_parent()).remove_child(container);
        rightBox.insert_child_at_index(container, position);
        indicator.menu.close(false);
    }
    await afterPaint(() => {
        indicator.menu.open(false);
    });
    const root = required(indicator.menu.box.get_children()[0]);
    await afterPaint(() => {
        required(
            findActor(root, actor => actor.accessible_name === 'Usage' && actor.has_style_class_name?.('tmb-tab')),
        ).emit('clicked', 1);
    });
    const rootX = root.get_transformed_position()[0];
    const indicatorX = indicator.get_transformed_position()[0];
    const menuCenter = rootX + root.width / 2;
    const closestCenter = Math.min(indicatorX + indicator.width / 2, global.stage.width - root.width / 2);
    if (
        rootX < 0 ||
        rootX + root.width > global.stage.width ||
        root.width < 440 ||
        root.width !== Math.min(880, global.stage.width - 32) ||
        Math.abs(menuCenter - closestCenter) > 25
    )
        throw new Error(
            `Right-side wide menu did not fit around the panel item: ${JSON.stringify({
                rootX,
                width: root.width,
                stage: global.stage.width,
                menuCenter,
                closestCenter,
            })}`,
        );
    await capture('menubar-right-popover');
    for (const tab of ['History', 'Settings'] as const) {
        await afterPaint(() => {
            required(
                findActor(root, actor => actor.accessible_name === tab && actor.has_style_class_name?.('tmb-tab')),
            ).emit('clicked', 1);
        });
        await waitFor(() =>
            findActor(root, actor =>
                actor.has_style_class_name?.(tab === 'History' ? 'tmb-history-visual' : 'tmb-settings-section'),
            ),
        );
        const page = findActor(root, actor => actor.has_style_class_name?.('tmb-page'));
        if (!page || page.width > root.width) throw new Error(`${tab} content exceeds the centered menu width`);
        await capture(`menubar-right-${tab.toLowerCase()}-popover`);
        if (tab === 'Settings') {
            const adjustment = instance(root.get_children()[1], St.ScrollView).get_vadjustment();
            await afterPaint(() => {
                adjustment.set_value(adjustment.upper / 2 - adjustment.page_size / 2);
            });
            await capture('menubar-right-settings-middle-popover');
            await afterPaint(() => {
                adjustment.set_value(adjustment.upper - adjustment.page_size);
            });
            await capture('menubar-right-settings-bottom-popover');
        }
    }
    indicator.menu.close(false);
    Main.layoutManager.emit('monitors-changed');
    if (indicator.menu.isOpen || getIndicator() !== indicator)
        throw new Error('Monitor update reopened or removed a closed menu');
}

export async function run() {
    try {
        if (GLib.getenv('TMB_PANEL_SPACE')) await panelSpaceScenario();
        else await runScenario();
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

async function panelSpaceScenario() {
    const indicator = required(getIndicator());
    const cells = required(findActor(indicator, actor => actor.has_style_class_name?.('tmb-panel-cells')));
    const icon = required(required(cells.get_parent()).get_children()[0]);
    await waitFor(() => indicator.width > 200 && !icon.visible, 'Provider status did not fill the panel');
    const box = required(indicator.container.get_parent());
    const host = GLib.getenv('TMB_PANEL_PEER') ? required(box.get_parent()) : box;
    const neighbor = new St.Widget({width: 1000, height: 20});
    host.add_child(neighbor);
    try {
        await afterPaint(() => {
            Main.layoutManager.emit('monitors-changed');
        });
        await waitFor(() => icon.visible, 'Provider status did not yield space to another panel item');
    } finally {
        host.remove_child(neighbor);
        neighbor.destroy();
    }
}

export function finish() {}
