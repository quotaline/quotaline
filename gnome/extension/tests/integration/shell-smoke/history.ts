import Atk from 'gi://Atk';
import Clutter from 'gi://Clutter';
import GLib from 'gi://GLib';
import Pango from 'gi://Pango';
import St from 'gi://St';
import type {TestActor} from '../../support/actors.js';
import {
    accessibleName,
    buttonLabel,
    clickButton,
    contentOf,
    instance,
    optionalInstance,
    required,
} from '../../support/actors.js';
import type {SmokeContext} from './helpers.js';
import {
    afterPaint,
    capture,
    checkMark,
    containsText,
    findActor,
    findActors,
    serviceSettings,
    serviceState,
    setSetting,
    waitOrFail,
} from './helpers.js';

export async function run({
    indicator,
    root,
    footer,
    openWithFastClock,
    press,
}: Pick<SmokeContext, 'indicator' | 'root' | 'footer' | 'openWithFastClock' | 'press'>) {
    const menuIsOpen = () => indicator.menu.isOpen;
    const tabs = required(required(root.get_children()[0]).get_children()[1]).get_children();
    await afterPaint(() => {
        clickButton(required(tabs[1]));
    });
    if (
        required(
            required(
                required(findActor(footer, actor => buttonLabel(actor) === 'Copy diagnostics')).get_parent(),
            ).get_parent(),
        ).visible
    )
        throw new Error('Settings footer actions are visible on History');
    if (GLib.getenv('TMB_EXPECT_DEMO') === '1')
        await waitOrFail(
            () => containsText(contentOf(root.get_children()[1]), '13 models'),
            'History did not summarize the tracked models',
        );
    if (GLib.getenv('TMB_EXPECT_DEMO') === '1') {
        await setSetting('selected_windows', ['claude:session']);
        await waitOrFail(
            () => containsText(contentOf(root.get_children()[1]), '1 model'),
            'History did not follow the selected model',
        );
        await capture('history-selected-model');
        await setSetting('selected_windows', []);
        await waitOrFail(
            () =>
                containsText(
                    contentOf(root.get_children()[1]),
                    'No models selected. Choose models in Settings to see history.',
                ),
            'History did not explain an empty model selection',
        );
        await capture('history-no-models');
        await setSetting('has_custom_selection', false);
        await waitOrFail(
            () => containsText(contentOf(root.get_children()[1]), '13 models'),
            'History did not restore the default model selection',
        );
        const providers = serviceSettings().providers;
        await setSetting('providers', {...providers, claude: false});
        await waitOrFail(
            () => containsText(contentOf(root.get_children()[1]), '9 models'),
            'History did not remove a disabled provider',
        );
        await setSetting('providers', providers);
        await waitOrFail(
            () => containsText(contentOf(root.get_children()[1]), '13 models'),
            'History did not restore an enabled provider',
        );
    }
    if (GLib.getenv('TMB_EXPECT_DEMO') === '1') {
        const {history_days: originalDays, history_rollup: originalRollup} = serviceSettings();
        const followedNow = instance(
            required(findActor(root, actor => buttonLabel(actor) === 'Now')),
            St.Widget,
        ).has_style_class_name('tmb-tab-selected');
        clickButton(required(findActor(root, actor => buttonLabel(actor) === '30d')));
        await waitOrFail(
            () =>
                findActor(
                    root,
                    actor =>
                        buttonLabel(actor) === '30d' &&
                        instance(actor, St.Widget).has_style_class_name('tmb-tab-selected'),
                ) && findActor(root, actor => actor.has_style_class_name?.('tmb-chart')),
            'Thirty-day History capture did not load',
        );
        clickButton(required(findActor(root, actor => buttonLabel(actor) === 'Day')));
        await waitOrFail(
            () =>
                findActor(
                    root,
                    actor =>
                        buttonLabel(actor) === 'Day' &&
                        instance(actor, St.Widget).has_style_class_name('tmb-tab-selected'),
                ) && findActor(root, actor => actor.has_style_class_name?.('tmb-chart')),
            'Daily History capture did not load',
        );
        await capture('history');
        const originalPeriodLabel = followedNow ? 'Now' : originalDays === 1 ? 'Today' : `${originalDays}d`;
        clickButton(required(findActor(root, actor => buttonLabel(actor) === originalPeriodLabel)));
        await waitOrFail(
            () =>
                findActor(
                    root,
                    actor =>
                        buttonLabel(actor) === originalPeriodLabel &&
                        instance(actor, St.Widget).has_style_class_name('tmb-tab-selected'),
                ),
            'History period was not restored',
        );
        if (followedNow) await setSetting('history_days', originalDays);
        const originalRollupLabel = originalRollup.charAt(0).toUpperCase() + originalRollup.slice(1);
        clickButton(required(findActor(root, actor => buttonLabel(actor) === originalRollupLabel)));
        await waitOrFail(() => serviceSettings().history_rollup === originalRollup, 'History rollup was not restored');
    } else {
        await capture('history');
    }
    if (root.width > 880) throw new Error(`History menu exceeds 880px: ${root.width}`);
    if (GLib.getenv('TMB_EXPECT_DEMO') === '1') {
        const nextReading = required(
            findActor(contentOf(root.get_children()[1]), actor => buttonLabel(actor) === 'Next reading'),
        );
        const [, nextY] = nextReading.get_transformed_position();
        const [, scrollY] = required(root.get_children()[1]).get_transformed_position();
        if (nextY + nextReading.height > scrollY + required(root.get_children()[1]).height)
            throw new Error('History reading controls are clipped by the footer');
        indicator.menu.close(false);
        openWithFastClock();
        await afterPaint(() => {});
        const historyBeforeTick = findActor(root, actor => actor.has_style_class_name?.('tmb-chart'));
        await waitOrFail(
            () => findActor(root, actor => actor.has_style_class_name?.('tmb-chart')) !== historyBeforeTick,
            'Open History menu did not reload on its minute timer',
        );
        indicator.menu.close(false);
        indicator.menu.open(false);
        await afterPaint(() => {});
        const page = contentOf(root.get_children()[1]);
        const chart = findActor(page, actor => actor.has_style_class_name?.('tmb-chart'));
        if (
            !chart?.reactive ||
            !instance(chart, St.Widget).can_focus ||
            !chart.accessible_name.includes('Left and Right move between readings')
        )
            throw new Error('History chart was not rendered');
        if (
            chart.width < 250 ||
            chart.get_transformed_position()[0] + chart.width > root.get_transformed_position()[0] + root.width - 8
        )
            throw new Error(`History chart exceeds the available menu width: ${root.width}, ${chart.width}`);
        const exportControl = required(findActor(page, actor => buttonLabel(actor) === 'Export CSV'));
        if (
            exportControl.get_transformed_position()[0] + exportControl.width >
            root.get_transformed_position()[0] + root.width - 8
        )
            throw new Error(
                `History export control is clipped: ${JSON.stringify({
                    exportX: exportControl.get_transformed_position()[0],
                    exportWidth: exportControl.width,
                    rootX: root.get_transformed_position()[0],
                    rootWidth: root.width,
                    rowX: required(exportControl.get_parent()).get_transformed_position()[0],
                    rowWidth: required(exportControl.get_parent()).width,
                    children: required(exportControl.get_parent())
                        .get_children()
                        .map(actor => [
                            buttonLabel(instance(actor, St.Button)),
                            actor.width,
                            actor.get_transformed_position()[0],
                        ]),
                })}`,
            );
        const touch = global.stage.context
            .get_backend()
            .get_default_seat()
            .create_virtual_device(Clutter.InputDeviceType.TOUCHSCREEN_DEVICE);
        const historyChart = () => findActor(page, actor => actor.has_style_class_name?.('tmb-chart'));
        // A late load from reopening the menu can rebuild the chart under the finger, so touch the new one again.
        while (!containsText(page, 'Selected ')) {
            const currentChart = historyChart();
            const [chartX, chartY] = required(currentChart).get_transformed_position();
            touch.notify_touch_down(
                GLib.get_monotonic_time(),
                0,
                chartX + required(currentChart).width / 2,
                chartY + required(currentChart).height / 2,
            );
            await afterPaint(() => {});
            touch.notify_touch_up(GLib.get_monotonic_time(), 0);
            await waitOrFail(
                () => containsText(page, 'Selected ') || historyChart() !== currentChart,
                'Touching the History chart did not select a reading',
            );
        }
        await capture('history-touch-selected');
        touch.run_dispose();
        const clearPointerSelection = required(findActor(page, actor => buttonLabel(actor) === 'Clear selection'));
        clickButton(clearPointerSelection);
        const keyboardChart = required(findActor(page, actor => actor.has_style_class_name?.('tmb-chart')));
        keyboardChart.grab_key_focus();
        await press(Clutter.KEY_Right);
        await waitOrFail(() => containsText(page, 'Selected '), 'Right Arrow did not select a History reading');
        const firstKeyboardReading = page
            .get_children()
            .find((actor: TestActor) => actor.text?.startsWith('Selected '))?.text;
        const focusedChart = findActor(page, actor => actor.has_style_class_name?.('tmb-chart'));
        if (!focusedChart?.has_key_focus()) throw new Error('History chart lost focus after selecting a reading');
        await capture('history-keyboard-selected');
        await press(Clutter.KEY_Right);
        await waitOrFail(
            () =>
                page.get_children().find((actor: TestActor) => actor.text?.startsWith('Selected '))?.text !==
                firstKeyboardReading,
            'Right Arrow did not advance the History reading',
        );
        await press(Clutter.KEY_Left);
        await waitOrFail(
            () =>
                page.get_children().find((actor: TestActor) => actor.text?.startsWith('Selected '))?.text ===
                firstKeyboardReading,
            'Left Arrow did not move back to the prior History reading',
        );
        await press(Clutter.KEY_a);
        if (
            page.get_children().find((actor: TestActor) => actor.text?.startsWith('Selected '))?.text !==
                firstKeyboardReading ||
            !menuIsOpen()
        )
            throw new Error('An unrelated key changed the History reading');
        await press(Clutter.KEY_BackSpace);
        await waitOrFail(
            () => !page.get_children().some((actor: TestActor) => actor.text?.startsWith('Selected ')),
            `Backspace did not clear the focused History reading: ${JSON.stringify({
                selected: page.get_children().find((actor: TestActor) => actor.text?.startsWith('Selected '))?.text,
                menu: indicator.menu.isOpen,
            })}`,
        );
        if (!menuIsOpen())
            throw new Error(
                `Backspace did not clear the focused History reading: ${JSON.stringify({
                    selected: page.get_children().find((actor: TestActor) => actor.text?.startsWith('Selected '))?.text,
                    menu: indicator.menu.isOpen,
                })}`,
            );
        const attribution = findActor(page, actor => actor.has_style_class_name?.('tmb-history-attribution'));
        if (
            attribution?.get_children().filter(actor => actor.has_style_class_name?.('tmb-history-provider-icon'))
                .length !== 6 ||
            !containsText(attribution, 'enabled models · step line · selected time zone')
        )
            throw new Error('History attribution does not identify its provider data');
        const legendValues = findActors(
            required(findActor(page, actor => actor.has_style_class_name?.('tmb-history-legend'))),
            actor => actor.has_style_class_name?.('tmb-legend-toggle'),
        ).map(actor => actor.accessible_name);
        const firstSeries = required(findActor(page, actor => actor.has_style_class_name?.('tmb-legend-toggle')));
        instance(firstSeries, St.Widget).set_hover(true);
        if (!instance(firstSeries, St.Widget).has_style_class_name('tmb-legend-highlighted'))
            throw new Error('Hovering a History series did not highlight it');
        await afterPaint(() => {});
        await capture('history-hovered');
        const [currentFirstSeries, secondSeries] = findActors(page, actor =>
            actor.has_style_class_name?.('tmb-legend-toggle'),
        );
        instance(required(currentFirstSeries), St.Widget).set_hover(true);
        instance(required(secondSeries), St.Widget).set_hover(true);
        instance(required(currentFirstSeries), St.Widget).set_hover(false);
        if (
            instance(required(currentFirstSeries), St.Widget).has_style_class_name('tmb-legend-highlighted') ||
            !instance(required(secondSeries), St.Widget).has_style_class_name('tmb-legend-highlighted')
        )
            throw new Error('Moving across History series left the previous highlight active');
        instance(required(secondSeries), St.Widget).set_hover(false);
        if (instance(required(secondSeries), St.Widget).has_style_class_name('tmb-legend-highlighted'))
            throw new Error('History series highlight stayed after hover ended');
        const renderedChart = required(findActor(page, actor => actor.has_style_class_name?.('tmb-chart')));
        const scroll = root.get_children()[1];
        const footerY = required(root.get_children()[2]).get_transformed_position()[1];
        const chartBottom = renderedChart.get_transformed_position()[1] + renderedChart.height;
        if (chartBottom > footerY)
            await afterPaint(() => {
                instance(scroll, St.ScrollView)
                    .get_vadjustment()
                    .set_value(
                        instance(scroll, St.ScrollView).get_vadjustment().get_value() + chartBottom - footerY + 8,
                    );
            });
        if (
            renderedChart.get_transformed_position()[1] + renderedChart.height > footerY ||
            renderedChart.get_transformed_position()[1] < required(scroll).get_transformed_position()[1]
        )
            throw new Error('History chart axis is hidden behind the footer');
        let reading = page
            .get_children()
            .find((actor: TestActor) => actor.get_children().some(child => buttonLabel(child) === 'Next reading'));
        if (!reading) throw new Error('History reading inspector is missing');
        clickButton(required(reading.get_children().find((actor: TestActor) => buttonLabel(actor) === 'Next reading')));
        await waitOrFail(() => containsText(page, 'Selected '), 'History reading inspector did not select a reading');
        const selectedLegend = required(findActor(page, actor => actor.has_style_class_name?.('tmb-history-legend')));
        const selectedValues = findActors(selectedLegend, actor =>
            actor.has_style_class_name?.('tmb-legend-toggle'),
        ).map(actor => actor.accessible_name);
        if (
            containsText(selectedLegend.get_children()[0], 'Range values') ||
            JSON.stringify(selectedValues) === JSON.stringify(legendValues)
        )
            throw new Error(
                `Selecting a chart reading did not update legend values: ${JSON.stringify({
                    header: required(selectedLegend.get_children()[0]).get_children()[0]?.text,
                    selectedValues,
                    legendValues,
                })}`,
            );
        await capture('history-selected');
        const legendName = findActor(
            page,
            actor => actor.has_style_class_name?.('tmb-legend-name') && required(actor.text).includes('Antigravity'),
        );
        if (
            !legendName ||
            legendName.width < 132 ||
            !instance(legendName, St.Label).clutter_text.get_line_wrap() ||
            instance(legendName, St.Label).clutter_text.get_ellipsize() !== Pango.EllipsizeMode.NONE
        )
            throw new Error('Long History series names are truncated');
        reading = page
            .get_children()
            .find((actor: TestActor) => actor.get_children().some(child => buttonLabel(child) === 'Next reading'));
        const previousReading = page
            .get_children()
            .find((actor: TestActor) => actor.text?.startsWith('Selected '))?.text;
        clickButton(
            required(
                required(reading)
                    .get_children()
                    .find((actor: TestActor) => buttonLabel(actor) === 'Next reading'),
            ),
        );
        await waitOrFail(
            () =>
                page.get_children().find((actor: TestActor) => actor.text?.startsWith('Selected '))?.text !==
                previousReading,
            'History reading inspector did not move to the next date',
        );
        const firstReading = page.get_children().find((actor: TestActor) => actor.text?.startsWith('Selected '))?.text;
        if (
            !firstReading ||
            !findActor(
                page,
                actor =>
                    actor.has_style_class_name?.('tmb-history-legend-header') && !containsText(actor, 'Range values'),
            )
        )
            throw new Error('History reading inspector did not update the legend date');
        reading = page
            .get_children()
            .find((actor: TestActor) => actor.get_children().some(child => buttonLabel(child) === 'Next reading'));
        clickButton(
            required(
                required(reading)
                    .get_children()
                    .find((actor: TestActor) => buttonLabel(actor) === 'Next reading'),
            ),
        );
        await waitOrFail(
            () =>
                page.get_children().find((actor: TestActor) => actor.text?.startsWith('Selected '))?.text !==
                firstReading,
            'History reading inspector did not advance',
        );
        reading = page
            .get_children()
            .find((actor: TestActor) => actor.get_children().some(child => buttonLabel(child) === 'Clear selection'));
        clickButton(
            required(
                required(reading)
                    .get_children()
                    .find((actor: TestActor) => buttonLabel(actor) === 'Clear selection'),
            ),
        );
        if (
            page.get_children().some((actor: TestActor) => actor.text?.startsWith('Selected ')) ||
            !findActor(
                page,
                actor =>
                    actor.has_style_class_name?.('tmb-history-legend-header') && containsText(actor, 'Range values'),
            )
        )
            throw new Error('History reading selection did not clear');
        const details = required(findActor(page, actor => actor.accessible_name === 'Data details'));
        if (details.get_transformed_position()[0] > page.get_transformed_position()[0] + 30)
            throw new Error('History data details disclosure is not aligned with the chart');
        clickButton(details);
        if (!containsText(page, 'plotted points · 13 series'))
            throw new Error('History data details did not show chart sample counts');
        const historyScroll = instance(root.get_children()[1], St.ScrollView).get_vadjustment();
        historyScroll.set_value(historyScroll.upper - historyScroll.page_size);
        await afterPaint(() => {});
        await capture('history-details');
        historyScroll.set_value(0);
        const sevenDayRange = findActor(page, actor => actor.has_style_class_name?.('tmb-history-path'))?.text;
        await afterPaint(() => {
            clickButton(required(findActor(page, actor => buttonLabel(actor) === 'Today')));
        });
        await waitOrFail(
            () => findActor(page, actor => actor.has_style_class_name?.('tmb-history-path'))?.text !== sevenDayRange,
            'Today did not narrow the History chart range',
        );
        clickButton(required(findActor(page, actor => buttonLabel(actor) === 'Day')));
        await waitOrFail(
            () => serviceSettings().history_rollup === 'day' && serviceSettings().history_days === 7,
            'Daily rollup did not expand a one-day chart to seven days',
        );
        await afterPaint(() => {
            clickButton(required(findActor(page, actor => buttonLabel(actor) === 'Today')));
        });
        await waitOrFail(
            () => serviceSettings().history_rollup === 'hour' && serviceSettings().history_days === 1,
            'Today did not switch a daily rollup to hourly data',
        );
        clickButton(required(findActor(page, actor => actor.accessible_name === 'UTC')));
        await waitOrFail(
            () =>
                serviceSettings().history_utc &&
                findActor(page, actor => actor.accessible_name === 'UTC' && checkMark(actor) === '☑'),
            'Today chart did not switch to UTC',
        );
        await afterPaint(() => {});
        clickButton(required(findActor(page, actor => actor.accessible_name === 'UTC')));
        await waitOrFail(() => !serviceSettings().history_utc, 'Today chart did not return to local time');
        clickButton(required(findActor(page, actor => buttonLabel(actor) === '7d')));
        await waitOrFail(
            () => findActor(page, actor => actor.has_style_class_name?.('tmb-history-path'))?.text === sevenDayRange,
            'History did not restore the seven-day chart range',
        );
        const navigation = page
            .get_children()
            .find((actor: TestActor) => actor.get_children().some(child => buttonLabel(child) === 'Now'));
        if (!navigation) throw new Error('History navigation is missing');
        const originalRange = findActor(page, actor => actor.has_style_class_name?.('tmb-history-path'))?.text;
        if (!originalRange?.includes('–')) throw new Error('History date range is missing');
        clickButton(required(findActor(page, actor => actor.accessible_name === 'UTC')));
        await waitOrFail(
            () =>
                serviceSettings().history_utc &&
                findActor(page, actor => actor.accessible_name === 'UTC' && checkMark(actor) === '☑'),
            'History UTC switch did not update',
        );
        let rollups = page
            .get_children()
            .find((actor: TestActor) => actor.get_children().some(child => child.text === 'Rollup'));
        if (
            !rollups ||
            !['Minute', 'Hour', 'Day'].every(value =>
                required(rollups)
                    .get_children()
                    .some(child => buttonLabel(child) === value),
            )
        )
            throw new Error('Minute, Hour, and Day rollups are missing');
        clickButton(required(rollups.get_children().find((actor: TestActor) => buttonLabel(actor) === 'Hour')));
        await waitOrFail(
            () =>
                findActor(
                    page,
                    actor =>
                        buttonLabel(actor) === 'Hour' &&
                        instance(actor, St.Widget).has_style_class_name('tmb-tab-selected'),
                ),
            'Hourly rollup did not update',
        );
        rollups = page
            .get_children()
            .find((actor: TestActor) => actor.get_children().some(child => child.text === 'Rollup'));
        if (
            !instance(
                required(
                    required(rollups)
                        .get_children()
                        .find((actor: TestActor) => buttonLabel(actor) === 'Hour'),
                ),
                St.Widget,
            ).has_style_class_name('tmb-tab-selected')
        )
            throw new Error('Hourly rollup was not selected');
        clickButton(required(findActor(page, actor => buttonLabel(actor) === 'Previous reading')));
        await waitOrFail(() => containsText(page, 'Selected '), 'History reading was not selected before paging');
        clickButton(required(findActor(page, actor => buttonLabel(actor) === '←')));
        await waitOrFail(() => {
            const range = findActor(page, actor => actor.has_style_class_name?.('tmb-history-path'))?.text;
            return range && range !== originalRange && !containsText(page, 'Selected ');
        }, 'Paging did not clear a selected reading outside the new History range');
        clickButton(
            required(
                required(
                    page
                        .get_children()
                        .find((actor: TestActor) => actor.get_children().some(child => buttonLabel(child) === 'Now')),
                )
                    .get_children()
                    .find((actor: TestActor) => buttonLabel(actor) === 'Now'),
            ),
        );
        await waitOrFail(
            () =>
                findActor(
                    page,
                    actor =>
                        buttonLabel(actor) === 'Now' &&
                        instance(actor, St.Widget).has_style_class_name('tmb-tab-selected'),
                ),
            'Now did not resume after paging',
        );
        clickButton(
            required(
                required(
                    page
                        .get_children()
                        .find((actor: TestActor) =>
                            actor.get_children().some(child => buttonLabel(child) === 'Custom'),
                        ),
                )
                    .get_children()
                    .find((actor: TestActor) => buttonLabel(actor) === 'Custom'),
            ),
        );
        const custom = page
            .get_children()
            .find((actor: TestActor) => actor.get_children().some(child => buttonLabel(child) === 'Apply dates'));
        if (!custom) throw new Error('Custom history dates are missing');
        const originalDate = instance(required(custom.get_children()[0]), St.Entry).get_text();
        const originalEnd = instance(required(custom.get_children()[2]), St.Entry).get_text();
        const startPicker = custom.get_children()[1];
        clickButton(required(startPicker));
        const calendar = findActor(page, actor => actor.has_style_class_name?.('tmb-date-calendar'));
        if (!calendar?.visible) throw new Error('Start date picker did not open the calendar');
        clickButton(required(startPicker));
        if (calendar.is_visible()) throw new Error('Start date picker did not close on a second click');
        instance(required(custom.get_children()[0]), St.Entry).set_text('not-a-date');
        clickButton(required(startPicker));
        if (
            !calendar.is_visible() ||
            instance(required(custom.get_children()[0]), St.Entry).get_text() !== 'not-a-date'
        )
            throw new Error('Opening the calendar changed an invalid typed date');
        clickButton(required(custom.get_children()[3]));
        if (!calendar.is_visible() || instance(required(custom.get_children()[2]), St.Entry).get_text() !== originalEnd)
            throw new Error('Switching date pickers changed the end date');
        instance(required(custom.get_children()[0]), St.Entry).set_text('2026-11-04');
        clickButton(required(startPicker));
        const chosenDay = findActor(
            calendar,
            actor =>
                actor.has_style_class_name?.('calendar-day') &&
                buttonLabel(actor) === '05' &&
                !actor.has_style_class_name('calendar-other-month'),
        );
        if (!chosenDay?.reactive) throw new Error('Calendar day cannot be clicked');
        clickButton(chosenDay);
        if (instance(required(custom.get_children()[0]), St.Entry).get_text() !== '2026-11-05' || calendar.is_visible())
            throw new Error('Calendar selection did not update the start date');
        instance(required(custom.get_children()[2]), St.Entry).set_text('2026-11-05');
        clickButton(required(custom.get_children()[3]));
        clickButton(
            required(findActor(calendar, actor => actor.has_style_class_name?.('calendar-change-month-forward'))),
        );
        if (
            !calendar.is_visible() ||
            instance(required(custom.get_children()[2]), St.Entry).get_text() !== '2026-11-05'
        )
            throw new Error('Changing the calendar month committed a date');
        clickButton(required(findActor(page, actor => buttonLabel(actor) === 'Use selected day')));
        if (instance(required(custom.get_children()[2]), St.Entry).get_text() !== '2026-12-05' || calendar.is_visible())
            throw new Error('Using the calendar month selection did not update the end date');
        instance(required(custom.get_children()[2]), St.Entry).set_text(originalEnd);
        instance(required(custom.get_children()[0]), St.Entry).set_text('not-a-date');
        clickButton(required(custom.get_children()[4]));
        await waitOrFail(() => containsText(page, 'Enter dates as YYYY-MM-DD'), 'Malformed custom date was accepted');
        const malformed = page
            .get_children()
            .find((actor: TestActor) => actor.get_children().some(child => buttonLabel(child) === 'Apply dates'));
        instance(required(required(malformed).get_children()[0]), St.Entry).set_text('2026-02-30');
        clickButton(required(required(malformed).get_children()[4]));
        await waitOrFail(
            () => containsText(page, 'Choose a valid date range') && !containsText(page, 'Enter dates as YYYY-MM-DD'),
            'Impossible custom date was accepted',
        );
        const reversed = page
            .get_children()
            .find((actor: TestActor) => actor.get_children().some(child => buttonLabel(child) === 'Apply dates'));
        instance(required(required(reversed).get_children()[0]), St.Entry).set_text('2999-01-01');
        clickButton(required(required(reversed).get_children()[4]));
        await waitOrFail(
            () =>
                containsText(page, 'Choose a valid date range') &&
                page
                    .get_children()
                    .find((actor: TestActor) =>
                        actor.get_children().some(child => buttonLabel(child) === 'Apply dates'),
                    ) !== reversed,
            'Reversed custom date range was accepted',
        );
        const corrected = page
            .get_children()
            .find((actor: TestActor) => actor.get_children().some(child => buttonLabel(child) === 'Apply dates'));
        instance(required(required(corrected).get_children()[0]), St.Entry).set_text(originalDate);
        clickButton(required(required(corrected).get_children()[4]));
        await waitOrFail(
            () =>
                findActor(page, actor => actor.has_style_class_name?.('tmb-chart')) &&
                findActor(
                    page,
                    actor =>
                        buttonLabel(actor) === 'Custom' &&
                        instance(actor, St.Widget).has_style_class_name('tmb-tab-selected'),
                ) &&
                !page
                    .get_children()
                    .some((actor: TestActor) =>
                        actor.get_children().some(child => buttonLabel(child) === 'Apply dates'),
                    ),
            'Custom history range did not load',
        );
        const customPeriod = page
            .get_children()
            .find((actor: TestActor) => actor.get_children().some(child => buttonLabel(child) === 'Custom'));
        if (
            !instance(
                required(
                    required(customPeriod)
                        .get_children()
                        .find((actor: TestActor) => buttonLabel(actor) === 'Custom'),
                ),
                St.Widget,
            ).has_style_class_name('tmb-tab-selected')
        )
            throw new Error('Custom history period was not selected');
        clickButton(
            required(
                required(customPeriod)
                    .get_children()
                    .find((actor: TestActor) => buttonLabel(actor) === 'Now'),
            ),
        );
        await waitOrFail(
            () =>
                optionalInstance(
                    page
                        .get_children()
                        .find((actor: TestActor) => actor.get_children().some(child => buttonLabel(child) === 'Custom'))
                        ?.get_children()
                        .find((actor: TestActor) => buttonLabel(actor) === 'Now'),
                    St.Widget,
                )?.has_style_class_name('tmb-tab-selected'),
            'Now did not resume the custom period',
        );
        clickButton(
            required(
                required(
                    page
                        .get_children()
                        .find((actor: TestActor) =>
                            actor.get_children().some(child => buttonLabel(child) === 'Custom'),
                        ),
                )
                    .get_children()
                    .find((actor: TestActor) => buttonLabel(actor) === 'Custom'),
            ),
        );
        await waitOrFail(
            () =>
                page
                    .get_children()
                    .some((actor: TestActor) =>
                        actor.get_children().some(child => buttonLabel(child) === 'Apply dates'),
                    ),
            'Custom date picker did not reopen',
        );
        clickButton(
            required(
                required(
                    page
                        .get_children()
                        .find((actor: TestActor) =>
                            actor.get_children().some(child => buttonLabel(child) === 'Apply dates'),
                        ),
                ).get_children()[4],
            ),
        );
        await waitOrFail(
            () =>
                findActor(
                    page,
                    actor =>
                        buttonLabel(actor) === 'Custom' &&
                        instance(actor, St.Widget).has_style_class_name('tmb-tab-selected'),
                ),
            'Custom history period did not update',
        );
        const metric = findActor(
            page,
            actor => actor.accessible_name === 'Metric' && buttonLabel(actor)?.startsWith('Usage %'),
        );
        if (!metric) throw new Error('History metric picker is missing');
        clickButton(metric);
        const options = findActor(page, actor => actor.has_style_class_name?.('tmb-metric-options'));
        if (
            !options ||
            options.height < 200 ||
            options.width > 400 ||
            ['Windows', 'Claude and Codex', 'Claude', 'Codex', 'Projects'].some(
                group => !containsText(options, group),
            ) ||
            !findActor(options, actor => actor.accessible_name === 'Choose metric: surfaceUsagePercent')
        )
            throw new Error('Grouped History metric choices are missing');
        await capture('history-metric-picker');
        const metricScroll = instance(options, St.ScrollView).get_vadjustment();
        metricScroll.set_value(metricScroll.upper - metricScroll.page_size);
        await afterPaint(() => {});
        const lastMetric = required(
            findActor(options, actor => actor.accessible_name === 'Choose metric: projectMessages'),
        );
        const menuY = options.get_transformed_position()[1];
        const lastY = lastMetric.get_transformed_position()[1];
        if (lastY < menuY || lastY + lastMetric.height > menuY + options.height)
            throw new Error('The final History metric is not visible after scrolling');
        await capture('history-metric-picker-bottom');
        metricScroll.set_value(0);
        clickButton(
            required(findActor(options, actor => actor.accessible_name === 'Choose metric: surfaceUsagePercent')),
        );
        await waitOrFail(
            () =>
                findActor(
                    page,
                    actor => actor.accessible_name === 'Metric' && buttonLabel(actor)?.startsWith('Usage by surface'),
                ) && findActor(page, actor => actor.has_style_class_name?.('tmb-chart')),
            'Daily analytics metric was not selected',
        );
        await capture('history-surface');
        clickButton(required(findActor(page, actor => buttonLabel(actor) === 'Today')));
        await waitOrFail(
            () =>
                serviceSettings().history_days === 1 &&
                serviceSettings().history_metric === 'surfaceUsagePercent' &&
                findActor(
                    page,
                    actor =>
                        buttonLabel(actor) === 'Today' &&
                        instance(actor, St.Widget).has_style_class_name('tmb-tab-selected'),
                ),
            'Today did not select the UTC day for a daily metric',
        );
        clickButton(required(findActor(page, actor => buttonLabel(actor) === '30d')));
        await waitOrFail(
            () =>
                findActor(
                    page,
                    actor =>
                        buttonLabel(actor) === '30d' &&
                        instance(actor, St.Widget).has_style_class_name('tmb-tab-selected'),
                ) && findActor(page, actor => actor.has_style_class_name?.('tmb-chart')),
            'Thirty-day History period was not selected',
        );
        clickButton(required(findActor(page, actor => actor.accessible_name === 'Metric')));
        clickButton(required(findActor(page, actor => actor.accessible_name === 'Choose metric: creditBalance')));
        await waitOrFail(
            () =>
                findActor(
                    page,
                    actor => actor.accessible_name === 'Metric' && buttonLabel(actor)?.startsWith('Credit balance'),
                ) &&
                containsText(page, 'series · latest') &&
                findActor(page, actor => accessibleName(actor)?.startsWith('☑ Available credits ·')) &&
                !page.get_children().some((actor: TestActor) => actor.accessible_name === 'Stacked'),
            'Credit balance history is missing its latest-value line layout',
        );
        await capture('history-credit-balance');
        clickButton(required(findActor(page, actor => actor.accessible_name === 'Metric')));
        clickButton(required(findActor(page, actor => actor.accessible_name === 'Choose metric: modelCredits')));
        await waitOrFail(
            () =>
                findActor(
                    page,
                    actor => actor.accessible_name === 'Metric' && buttonLabel(actor)?.startsWith('Credits by model'),
                ) && findActor(page, actor => actor.has_style_class_name?.('tmb-chart')),
            'Model credit metric was not selected',
        );
        const stack = required(
            page
                .get_children()
                .find((actor: TestActor) => actor.accessible_name === 'Stacked' && checkMark(actor) === '☐'),
        );
        if (
            !optionalInstance(stack, St.Button)?.toggle_mode ||
            stack.get_accessible().ref_state_set().contains_state(Atk.StateType.CHECKED)
        )
            throw new Error('Additive model metric has no stack control');
        clickButton(stack);
        if (
            !page
                .get_children()
                .some(
                    (actor: TestActor) =>
                        actor.accessible_name === 'Stacked' &&
                        checkMark(actor) === '☑' &&
                        actor.get_accessible().ref_state_set().contains_state(Atk.StateType.CHECKED),
                )
        )
            throw new Error('Stacked chart setting did not update');
        await capture('history-credits-stacked');
        await waitOrFail(() => {
            const settings = serviceSettings();
            return (
                settings.last_tab === 'History' &&
                settings.history_metric === 'modelCredits' &&
                settings.history_stacked
            );
        }, 'History choices were not saved');
        const modelSeries = findActor(page, actor => accessibleName(actor)?.startsWith('☑ gpt-5.3-codex'));
        if (!modelSeries) throw new Error('Model series toggle is missing');
        clickButton(modelSeries);
        await waitOrFail(
            () => serviceSettings().history_hidden_keys.includes('codex:gpt-5.3-codex'),
            'Hidden chart series was not saved',
        );
        await capture('history-credits-hidden');
        if (!containsText(page, 'gpt-5.3-codex') || !containsText(page, 'gpt-5.3-codex-spark'))
            throw new Error('Model credit legend is incomplete');
        await waitOrFail(() => {
            const restore = findActor(page, actor => accessibleName(actor)?.startsWith('☐ gpt-5.3-codex ·'));
            if (!restore) return false;
            // The next service signal can replace this actor before an awaiting continuation runs.
            clickButton(restore);
            return true;
        }, 'Model series restore toggle is missing');
        await waitOrFail(
            () => !serviceSettings().history_hidden_keys.includes('codex:gpt-5.3-codex'),
            'Model series could not be restored',
        );
        const beforeIsolation = serviceSettings().history_hidden_keys;
        const isolate = findActor(page, actor => accessibleName(actor)?.startsWith('☑ gpt-5.3-codex-spark'));
        if (!isolate) throw new Error('Model series isolation is missing');
        isolate.grab_key_focus();
        if (global.stage.key_focus !== isolate)
            throw new Error(`Model series could not take keyboard focus: ${global.stage.key_focus?.accessible_name}`);
        await press(Clutter.KEY_i);
        await waitOrFail(
            () => {
                const hidden = serviceSettings().history_hidden_keys;
                return (
                    hidden.includes('codex:gpt-5.3-codex') &&
                    !hidden.includes('codex:gpt-5.3-codex-spark') &&
                    hidden.length > beforeIsolation.length &&
                    Boolean(findActor(page, actor => accessibleName(actor)?.startsWith('☑ gpt-5.3-codex-spark')))
                );
            },
            `Isolating a model did not hide the other series: before=${JSON.stringify(beforeIsolation)} after=${JSON.stringify(serviceSettings().history_hidden_keys)} focus=${global.stage.key_focus.accessible_name}`,
        );
        if (!accessibleName(global.stage.key_focus)?.startsWith('☑ gpt-5.3-codex-spark'))
            throw new Error('History lost keyboard focus after isolating a series');
        await press(Clutter.KEY_I);
        await waitOrFail(
            () => JSON.stringify(serviceSettings().history_hidden_keys) === JSON.stringify(beforeIsolation),
            'Repeating isolation did not restore the saved selection',
        );
        for (const [id, title] of [
            ...serviceState()
                .metric_definitions.slice(3)
                .map(item => [item.id, item.title]),
            ['usage', 'Usage %'],
        ] as const) {
            const picker = required(findActor(page, actor => actor.accessible_name === 'Metric'));
            const previous = buttonLabel(optionalInstance(picker, St.Button));
            clickButton(picker);
            const choice = findActor(page, actor => actor.accessible_name === `Choose metric: ${id}`);
            if (!choice) throw new Error(`${id} is missing from the grouped metric picker`);
            clickButton(choice);
            await waitOrFail(
                () =>
                    serviceSettings().history_metric === id &&
                    findActor(
                        page,
                        actor => actor.accessible_name === 'Metric' && buttonLabel(actor)?.startsWith(title),
                    ) &&
                    findActor(page, actor => actor.has_style_class_name?.('tmb-chart')) &&
                    findActor(page, actor => actor.has_style_class_name?.('tmb-history-attribution')),
                `${previous} to ${title} did not render its chart and attribution: saved=${serviceSettings().history_metric}, ` +
                    `shown=${buttonLabel(
                        optionalInstance(
                            findActor(page, actor => actor.accessible_name === 'Metric'),
                            St.Button,
                        ),
                    )}, ` +
                    `empty=${containsText(page, 'No recorded data')}`,
            );
        }
    }
}
