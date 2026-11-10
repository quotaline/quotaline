import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import St from 'gi://St';
import type {TestActor} from '../support/actors.js';
import {
    buttonLabel,
    clickButton,
    contentOf,
    failure,
    findActor,
    indicator as getIndicator,
    instance,
    required,
} from '../support/actors.js';
import {readProvider} from '../support/dbus.js';
import {waitOrFail} from '../support/shell-events.js';

function afterPaint() {
    return new Promise<void>(resolve => {
        const signal = global.stage.connect('after-paint', () => {
            global.stage.disconnect(signal);
            resolve();
        });
        global.stage.queue_redraw();
    });
}

function waitFor(check: () => unknown, failure: string | (() => string)) {
    return waitOrFail(check, failure);
}

async function capture(name: string) {
    const directory = GLib.getenv('TMB_SCREENSHOT_DIR');
    if (!directory) return;
    name = name.replaceAll('compact', GLib.getenv('TMB_CAPTURE_VARIANT') ?? 'compact');
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
    const indicator = getIndicator();
    if (!indicator?.menu.isOpen) return;
    const root = findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-root'));
    if (!root) return;
    const [left, top] = root.get_transformed_position();
    const x = Math.max(0, Math.floor(left) - 8);
    const y = Math.max(0, Math.floor(top) - 8);
    const width = Math.min(global.stage.width - x, Math.ceil(root.width) + 16);
    const height = Math.min(global.stage.height - y, Math.ceil(root.height) + 16);
    const menuStream = Gio.File.new_for_path(GLib.build_filenamev([directory, `${name}-menu.png`])).replace(
        null,
        false,
        Gio.FileCreateFlags.REPLACE_DESTINATION,
        null,
    );
    await new Promise<void>((resolve, reject) => {
        new Shell.Screenshot().screenshot_area(x, y, width, height, menuStream, (source, result) => {
            try {
                required(source).screenshot_area_finish(result);
                menuStream.close(null);
                resolve();
            } catch (error) {
                reject(failure(error));
            }
        });
    });
}

export async function run() {
    try {
        const indicator = getIndicator();
        if (!indicator) throw new Error('Compact menu bar indicator is missing');
        indicator.menu.open(false);
        await waitFor(
            () => findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-root')),
            'Compact menu did not open',
        );
        const root = findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-root'));
        if (!root || root.width >= 790 || root.width > global.stage.width - 20)
            throw new Error('Narrow monitor did not select the compact menu');
        clickButton(
            required(
                findActor(root, actor => actor.accessible_name === 'Usage' && actor.has_style_class_name?.('tmb-tab')),
            ),
        );
        await waitFor(
            () =>
                contentOf(required(root.get_children()[1]))
                    .get_children()
                    .filter((actor: TestActor) => actor.has_style_class_name?.('tmb-card')).length === 6,
            'Compact Usage lost provider cards',
        );
        await afterPaint();
        const header = required(findActor(root, actor => actor.has_style_class_name?.('tmb-header')));
        const headerInfo = required(findActor(header, actor => actor.has_style_class_name?.('tmb-header-info')));
        const headerTitle = findActor(header, actor => actor.text === 'Quotaline');
        const tabs = findActor(header, actor => actor.has_style_class_name?.('tmb-tabs'));
        if (
            !headerTitle ||
            !tabs ||
            headerInfo.visible !== root.width >= 480 ||
            header.height >= 50 ||
            tabs.get_transformed_position()[0] + tabs.width > root.get_transformed_position()[0] + root.width - 8 ||
            (headerInfo.visible &&
                Math.abs(headerTitle.get_transformed_position()[1] - tabs.get_transformed_position()[1]) > 12)
        )
            throw new Error(
                `Compact title and tabs do not fit on one header row: ${JSON.stringify({
                    width: root.width,
                    titleVisible: headerInfo.visible,
                    height: header.height,
                    tabsRight: tabs && tabs.get_transformed_position()[0] + tabs.width,
                    menuRight: root.get_transformed_position()[0] + root.width - 8,
                    titleY: headerTitle?.get_transformed_position()[1],
                    tabsY: tabs?.get_transformed_position()[1],
                })}`,
            );
        const footer = root.get_children()[2];
        const refresh = findActor(footer, actor => buttonLabel(actor)?.includes('Refresh'));
        const quit = findActor(footer, actor => actor.accessible_name === 'Quit');
        if (
            !refresh ||
            !quit ||
            (root.width < 440 &&
                (Math.abs(refresh.get_transformed_position()[1] - quit.get_transformed_position()[1]) > 4 ||
                    quit.get_transformed_position()[0] < refresh.get_transformed_position()[0] + refresh.width))
        )
            throw new Error('Compact footer leaves Quit on a separate row or overlaps Refresh');
        const cards = contentOf(required(root.get_children()[1]))
            .get_children()
            .filter((actor: TestActor) => actor.has_style_class_name?.('tmb-card'));
        const antigravityBody = required(cards[0]).get_children()[1];
        const rows = required(antigravityBody)
            .get_children()
            .filter((actor: TestActor) => actor.has_style_class_name?.('tmb-window'));
        const antigravity = readProvider('antigravity').windows;
        const paceTitle = {
            on_track: 'On pace',
            ahead: 'Ahead',
            behind: 'Under pace',
            unknown: 'Learning pace',
            exhausted: 'Limit reached',
        };
        const group = required(antigravityBody)
            .get_children()
            .find((actor: TestActor) => actor.has_style_class_name?.('tmb-quota-group-reset'));
        if (root.width >= 600) {
            const [title, meterColumn, value, reset] = required(required(rows[0]).get_children()[0]).get_children();
            const [track, pace] = required(meterColumn).get_children();
            if (
                rows.length !== 2 ||
                group ||
                required(track).width < 160 ||
                required(title).get_x() >= required(meterColumn).get_x() ||
                required(meterColumn).get_x() >= required(value).get_x() ||
                required(value).get_x() >= required(reset).get_x() ||
                required(pace).get_y() <= required(track).get_y() ||
                !required(required(pace).text).startsWith(paceTitle[required(antigravity[0]).pace.status]) ||
                !required(required(reset).text).includes('Resets in')
            )
                throw new Error('Medium quota rows or resets do not fit');
        } else {
            const [primary, track, pace] = required(rows[0]).get_children();
            const [title, value] = required(primary).get_children();
            if (
                rows.length !== 2 ||
                group ||
                required(track).width < root.width - 100 ||
                required(title).get_x() >= required(value).get_x() ||
                required(track).get_y() <= required(primary).get_y() ||
                required(track).get_transformed_position()[1] <
                    required(title).get_transformed_position()[1] + required(title).height + 2 ||
                required(pace).get_y() <= required(track).get_y() ||
                !required(required(pace).text).startsWith(paceTitle[required(antigravity[0]).pace.status]) ||
                !required(required(required(rows[1]).get_children()[2]).text).startsWith(
                    paceTitle[required(antigravity[1]).pace.status],
                ) ||
                rows.some(
                    (actor: TestActor) =>
                        !findActor(
                            actor,
                            child =>
                                child.has_style_class_name?.('tmb-quota-reset-compact') &&
                                required(child.text).includes('Resets in'),
                        ),
                ) ||
                !findActor(
                    required(cards[1]).get_children()[1],
                    actor =>
                        actor.has_style_class_name?.('tmb-quota-reset-compact') &&
                        required(actor.text).includes('Resets in'),
                )
            )
                throw new Error('Compact quota rows or resets do not fit');
        }
        await capture('usage-compact');
        const adjustment = instance(required(root.get_children()[1]), St.ScrollView).get_vadjustment();
        const currentCards = contentOf(required(root.get_children()[1]))
            .get_children()
            .filter((actor: TestActor) => actor.has_style_class_name?.('tmb-card'));
        const codexCredits = findActor(currentCards[2], actor => actor.has_style_class_name?.('tmb-credit-metrics'));
        const renewal = findActor(currentCards[2], actor => buttonLabel(actor)?.startsWith('Renews '));
        if (
            !renewal ||
            renewal.get_transformed_position()[0] + renewal.width >
                required(currentCards[2]).get_transformed_position()[0] + required(currentCards[2]).width - 8
        )
            throw new Error('Codex renewal chip overflows the compact card');
        const creditTiles = codexCredits
            ?.get_children()
            .flatMap(child => (child.has_style_class_name?.('tmb-credit-tile') ? [child] : child.get_children()));
        if (
            creditTiles?.length !== 4 ||
            creditTiles.some(
                tile =>
                    tile.get_transformed_position()[0] + tile.width >
                    required(currentCards[2]).get_transformed_position()[0] + required(currentCards[2]).width - 10,
            )
        )
            throw new Error('Codex credit metrics overflow the compact card');
        adjustment.set_value(required(cards[2]).get_y() - 40);
        await afterPaint();
        await capture('usage-compact-credits');
        adjustment.set_value(0);
        clickButton(required(required(required(root.get_children()[0]).get_children()[1]).get_children()[1]));
        await waitFor(
            () =>
                findActor(contentOf(required(root.get_children()[1])), actor =>
                    actor.has_style_class_name?.('tmb-chart'),
                ),
            'Compact History chart did not load',
        );
        const chart = findActor(contentOf(required(root.get_children()[1])), actor =>
            actor.has_style_class_name?.('tmb-chart'),
        );
        await afterPaint();
        const legend = findActor(contentOf(required(root.get_children()[1])), actor =>
            actor.has_style_class_name?.('tmb-history-legend'),
        );
        const columns =
            legend &&
            findActor(legend, actor => actor.has_style_class_name?.('tmb-history-legend-columns'))?.get_children();
        await capture('history-compact-top');
        const oneColumn = root.width < 440;
        if (
            !chart ||
            !legend ||
            chart.get_transformed_position()[0] + chart.width > root.get_transformed_position()[0] + root.width - 12 ||
            legend.get_transformed_position()[1] < chart.get_transformed_position()[1] + chart.height ||
            columns?.length !== (oneColumn ? 1 : 2) ||
            required(columns[0]).get_n_children() !== (oneColumn ? 13 : 7) ||
            (!oneColumn &&
                (required(columns[1]).get_n_children() !== 6 ||
                    required(columns[0]).get_transformed_position()[0] + required(columns[0]).width >
                        required(columns[1]).get_transformed_position()[0])) ||
            required(columns.at(-1)).get_transformed_position()[0] + required(columns.at(-1)).width >
                root.get_transformed_position()[0] + root.width - 12
        )
            throw new Error(
                `Compact History layout: width ${root.width}, chart ${chart?.width}, ` +
                    `columns ${columns?.length}, rows ${JSON.stringify(columns?.map(column => column.get_n_children()))}, ` +
                    `chart x ${chart?.get_transformed_position()[0]}, legend y ${legend?.get_transformed_position()[1]}`,
            );
        adjustment.set_value(adjustment.upper - adjustment.page_size);
        await afterPaint();
        await capture('history-compact-bottom');
        clickButton(required(required(required(root.get_children()[0]).get_children()[1]).get_children()[2]));
        adjustment.set_value(0);
        await waitFor(
            () =>
                findActor(contentOf(required(root.get_children()[1])), actor =>
                    actor.has_style_class_name?.('tmb-status-preview'),
                ),
            'Compact Settings did not load',
        );
        await afterPaint();
        const page = contentOf(required(root.get_children()[1]));
        if (
            !findActor(page, actor => buttonLabel(actor) === 'At 100%') ||
            !findActor(page, actor => buttonLabel(actor) === 'Left')
        )
            throw new Error('Compact Settings did not shorten wide display labels');
        await capture('settings-compact-top');
        const collection = findActor(
            page,
            actor => actor.has_style_class_name?.('tmb-setting-row') && actor.get_children()[0]?.text === 'Collection',
        );
        const exportRow = findActor(
            page,
            actor => actor.has_style_class_name?.('tmb-setting-row') && actor.get_children()[0]?.text === 'Export',
        );
        if (!collection) throw new Error('Compact data controls are missing');
        const collectionControls = [
            'Increase analytics interval',
            'Decrease analytics interval',
            'Increase pace workdays',
            'Decrease pace workdays',
            'Write usage.json',
        ].map(name => findActor(collection, actor => actor.accessible_name === name));
        if (
            !exportRow ||
            collectionControls.some(
                actor =>
                    !actor ||
                    actor.get_transformed_position()[0] < root.get_transformed_position()[0] + 12 ||
                    actor.get_transformed_position()[0] + actor.width >
                        root.get_transformed_position()[0] + root.width - 12 ||
                    actor.get_transformed_position()[1] + actor.height > exportRow.get_transformed_position()[1],
            )
        )
            throw new Error(
                `Compact data controls leave the menu: ${root.get_transformed_position()[0]} ` +
                    `${root.width}, ${collectionControls
                        .map(
                            actor =>
                                `${actor?.accessible_name}: ${actor?.get_transformed_position()[0]} + ${actor?.width}`,
                        )
                        .join(', ')}`,
            );
        adjustment.set_value(
            adjustment.value + collection.get_transformed_position()[1] - root.get_transformed_position()[1] - 100,
        );
        await afterPaint();
        await capture('settings-compact-data');
        adjustment.set_value(0);
        await afterPaint();
        const currentPage = contentOf(required(root.get_children()[1]));
        const previewRows =
            findActor(currentPage, actor => actor.has_style_class_name?.('tmb-status-preview'))?.get_children() ?? [];
        if (
            previewRows.length < 2 ||
            previewRows.reduce((count, row) => count + row.get_n_children(), 0) !== 13 ||
            previewRows.some(
                row =>
                    row.get_n_children() < 2 ||
                    row
                        .get_children()
                        .some(
                            cell =>
                                cell.get_transformed_position()[0] + cell.width >
                                root.get_transformed_position()[0] + root.width - 12,
                        ),
            )
        )
            throw new Error(
                `Compact menu preview has an orphan cell or overflows: ` +
                    previewRows
                        .map(row =>
                            row
                                .get_children()
                                .map(cell => cell.width)
                                .join('+'),
                        )
                        .join(','),
            );
        const modelFilter = findActor(currentPage, actor => actor.has_style_class_name?.('tmb-model-filter'));
        const hideUnused = findActor(currentPage, actor => actor.accessible_name === 'Hide unused in range');
        const modelNote = findActor(
            currentPage,
            actor =>
                actor.has_style_class_name?.('tmb-setting-note') &&
                required(actor.text).includes('Checked limits appear'),
        );
        await capture('settings-compact-models');
        if (
            !modelFilter ||
            !hideUnused ||
            !modelNote ||
            modelFilter.get_transformed_position()[0] + modelFilter.width >
                root.get_transformed_position()[0] + root.width - 12 ||
            hideUnused.get_transformed_position()[0] + hideUnused.width >
                root.get_transformed_position()[0] + root.width - 12 ||
            hideUnused.get_transformed_position()[1] < modelFilter.get_transformed_position()[1] + modelFilter.height ||
            modelNote.get_transformed_position()[1] < hideUnused.get_transformed_position()[1] + hideUnused.height
        )
            throw new Error(
                `Compact model controls overlap or leave the menu: menu ${JSON.stringify(root.get_transformed_position())} ` +
                    `${root.width}, filter ${JSON.stringify(modelFilter?.get_transformed_position())} ${modelFilter?.width}x${modelFilter?.height}, ` +
                    `checkbox ${JSON.stringify(hideUnused?.get_transformed_position())} ${hideUnused?.width}x${hideUnused?.height}, ` +
                    `note ${JSON.stringify(modelNote?.get_transformed_position())}`,
            );
        adjustment.set_value(
            adjustment.value + modelFilter.get_transformed_position()[1] - root.get_transformed_position()[1] - 100,
        );
        await afterPaint();
        await capture('settings-compact-models-visible');
        const firstModel = required(
            findActor(page, actor => actor.has_style_class_name?.('tmb-model-group-card')),
        ).get_children()[1];
        const usage = required(findActor(firstModel, actor => actor.has_style_class_name?.('tmb-model-percent')));
        const meter = findActor(firstModel, actor => actor.has_style_class_name?.('tmb-model-meter'));
        const select = findActor(firstModel, actor => actor.accessible_name === 'Show Claude Current session');
        const name = findActor(firstModel, actor => actor.has_style_class_name?.('tmb-model-name'));
        const input = required(findActor(firstModel, actor => actor.has_style_class_name?.('tmb-short-entry')));
        const all = required(findActor(page, actor => actor.accessible_name === 'Show all Claude models'));
        const moveUp = findActor(firstModel, actor => actor.accessible_name === 'Move Claude Current session up');
        const moveDown = findActor(firstModel, actor => actor.accessible_name === 'Move Claude Current session down');
        const providerUp = findActor(page, actor => actor.accessible_name === 'Move Claude up');
        if (
            [select, name, input].some(
                actor =>
                    !actor ||
                    actor.get_transformed_position()[0] < root.get_transformed_position()[0] + 12 ||
                    actor.get_transformed_position()[0] + actor.width >
                        root.get_transformed_position()[0] + root.width - 12 ||
                    actor.get_transformed_position()[1] < required(firstModel).get_transformed_position()[1] ||
                    actor.get_transformed_position()[1] + actor.height >
                        required(firstModel).get_transformed_position()[1] + required(firstModel).height,
            ) ||
            usage.get_transformed_position()[0] + usage.width > root.get_transformed_position()[0] + root.width - 12 ||
            usage.get_preferred_width(-1)[1] > usage.width ||
            !meter ||
            meter.get_transformed_position()[1] <= usage.get_transformed_position()[1] + usage.height ||
            (root.width < 440 &&
                meter.get_transformed_position()[1] + meter.height > input.get_transformed_position()[1]) ||
            meter.get_transformed_position()[0] + meter.width > root.get_transformed_position()[0] + root.width - 12 ||
            !moveUp ||
            !moveDown ||
            moveUp.reactive ||
            !moveDown.reactive ||
            !providerUp ||
            moveUp.get_transformed_position()[0] < input.get_transformed_position()[0] + input.width ||
            moveDown.get_transformed_position()[0] + moveDown.width >
                root.get_transformed_position()[0] + root.width - 12 ||
            (root.width < 440 && providerUp.get_transformed_position()[1] <= all.get_transformed_position()[1]) ||
            all.get_transformed_position()[0] + all.width > root.get_transformed_position()[0] + root.width - 12
        )
            throw new Error(
                `Compact model contents are clipped: row ${JSON.stringify(required(firstModel).get_transformed_position())} ` +
                    `${required(firstModel).width}x${required(firstModel).height}, controls ${[
                        select,
                        name,
                        input,
                        usage,
                        all,
                    ]
                        .map(
                            actor =>
                                `${JSON.stringify(actor?.get_transformed_position())} ${actor?.width}x${actor?.height}`,
                        )
                        .join('; ')}, ` +
                    `usage preferred ${usage.get_preferred_width(-1)[1]}`,
            );
        adjustment.set_value(0);
        const setFormat = (value: string) =>
            Gio.DBus.session.call_sync(
                'tech.bernat.Quotaline.Gnome',
                '/tech/bernat/Quotaline/Gnome',
                'tech.bernat.Quotaline.Gnome',
                'SetSetting',
                new GLib.Variant('(ss)', ['format', JSON.stringify(value)]),
                new GLib.VariantType('(s)'),
                Gio.DBusCallFlags.NONE,
                2000,
                null,
            );
        setFormat('custom');
        await waitFor(
            () => findActor(page, actor => actor.accessible_name === 'Template' && actor.width > root.width / 2),
            'Compact custom template editor did not load',
        );
        const template = findActor(page, actor => actor.accessible_name === 'Template' && actor.width > root.width / 2);
        if (
            !template ||
            template.get_transformed_position()[0] + template.width >
                root.get_transformed_position()[0] + root.width - 14
        )
            throw new Error('Compact custom template editor is clipped or too narrow');
        await capture('settings-compact-template');
        setFormat('stacked');
        await afterPaint();
        const search = findActor(page, actor => actor.accessible_name === 'Search log');
        if (
            !search ||
            search.get_transformed_position()[0] + search.width > root.get_transformed_position()[0] + root.width - 14
        )
            throw new Error(
                `Compact Log search exceeds menu: x ${search?.get_transformed_position()[0]}, ` +
                    `width ${search?.width}, menu x ${root.get_transformed_position()[0]}, width ${root.width}`,
            );
        adjustment.set_value(adjustment.upper - adjustment.page_size);
        await afterPaint();
        if (root.width < 360) {
            const notifications = findActor(page, actor => actor.accessible_name === 'Notifications');
            const threshold = findActor(page, actor => actor.accessible_name === 'Notify at 100%');
            if (
                !notifications ||
                !threshold ||
                threshold.get_transformed_position()[1] <= notifications.get_transformed_position()[1] ||
                threshold.get_preferred_width(-1)[1] > threshold.width ||
                threshold.get_transformed_position()[0] + threshold.width >
                    root.get_transformed_position()[0] + root.width - 12
            )
                throw new Error('Small monitor clips notification thresholds');
        }
        if (root.width >= 440 && root.width < 650) {
            const resets = findActor(page, actor => actor.accessible_name === 'Window resets');
            const sound = findActor(page, actor => actor.accessible_name === 'Sound');
            if (
                !resets ||
                !sound ||
                sound.get_transformed_position()[1] <= resets.get_transformed_position()[1] ||
                sound.get_transformed_position()[0] + sound.width > root.get_transformed_position()[0] + root.width - 12
            )
                throw new Error('Middle-width notification events do not wrap within the menu');
        }
        await capture('settings-compact');
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
