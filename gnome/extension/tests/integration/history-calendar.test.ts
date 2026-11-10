import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import St from 'gi://St';
import type {HistoryAction, HistoryView, HistoryViewResponse} from '../../src/service-state.js';
import {
    buttonLabel,
    clickButton,
    failure,
    findActor,
    indicator as getIndicator,
    instance,
    required,
} from '../support/actors.js';
import {proxyCalls} from '../support/dbus.js';
import {waitOrFail} from '../support/shell-events.js';

function dayStart(timestamp: number, utc: boolean) {
    const date = new Date(timestamp * 1000);
    if (utc) date.setUTCHours(0, 0, 0, 0);
    else date.setHours(0, 0, 0, 0);
    return date.getTime() / 1000;
}

function shiftDay(timestamp: number, count: number, utc: boolean) {
    const date = new Date(timestamp * 1000);
    if (utc) date.setUTCDate(date.getUTCDate() + count);
    else date.setDate(date.getDate() + count);
    return date.getTime() / 1000;
}

function stamp(date: Date) {
    return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`;
}

function currentView() {
    return new Promise<HistoryView>((resolve, reject) => {
        Gio.DBus.session.call(
            'tech.bernat.Quotaline.Gnome',
            '/tech/bernat/Quotaline/Gnome',
            'tech.bernat.Quotaline.Gnome',
            'GetHistoryView',
            new GLib.Variant('(ssi)', [
                JSON.stringify({kind: 'load'}),
                Intl.DateTimeFormat().resolvedOptions().timeZone,
                -new Date().getTimezoneOffset() * 60,
            ]),
            new GLib.VariantType('(s)'),
            Gio.DBusCallFlags.NONE,
            12000,
            null,
            (connection, result) => {
                try {
                    const [json] = required(connection).call_finish(result).deep_unpack() as [string];
                    const response = JSON.parse(json) as HistoryViewResponse;
                    if (response.error !== null) throw new Error(response.error);
                    resolve(response.view);
                } catch (error) {
                    reject(failure(error));
                }
            },
        );
    });
}

export async function run() {
    const original = proxyCalls.call;
    const requests: {action: Record<string, unknown>; done: boolean}[] = [];
    try {
        proxyCalls.call = function (method, parameters, ...args) {
            if (method === 'GetHistoryView') {
                const [json] = required(parameters).deep_unpack() as [string, string, number];
                const request = {action: JSON.parse(json) as Record<string, unknown>, done: false};
                const callback = args[3];
                args[3] = (...values) => {
                    try {
                        callback(...values);
                        return;
                    } finally {
                        request.done = true;
                        global.stage.queue_redraw();
                    }
                };
                requests.push(request);
            }
            original.call(this, method, parameters, ...args);
        };
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        indicator.menu.open(false);
        const root = findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-root'));
        if (!root) throw new Error('Quotaline menu is missing');
        clickButton(required(findActor(root, actor => actor.accessible_name === 'History')));
        await waitOrFail(
            () => requests.length > 0 && requests.at(-1)?.done,
            'History did not request its initial period',
        );
        const control = (text: string) => findActor(root, actor => buttonLabel(actor) === text);
        const click = async (text: string) => {
            const before = requests.length;
            const actions: Record<string, HistoryAction> = {
                '←': {kind: 'page', forward: false},
                '→': {kind: 'page', forward: true},
                Now: {kind: 'now'},
                Today: {kind: 'period', days: 1},
            };
            const expected = actions[text] ?? {kind: 'period', days: Number.parseInt(text, 10)};
            const actor = control(text);
            if (!actor) throw new Error(`Missing History control: ${text}`);
            clickButton(actor);
            await waitOrFail(
                () =>
                    requests.slice(before).some(request => {
                        return (
                            request.done &&
                            Object.entries(expected).every(([key, value]) => request.action[key] === value)
                        );
                    }),
                `History control did not request data: ${text} (${JSON.stringify({
                    open: indicator.menu.isOpen,
                    before,
                    last: requests.at(-1),
                })})`,
            );
            return currentView();
        };
        const chooseMetric = async (id: string) => {
            clickButton(required(findActor(root, actor => actor.accessible_name === 'Metric')));
            const before = requests.length;
            clickButton(required(findActor(root, actor => actor.accessible_name === `Choose metric: ${id}`)));
            await waitOrFail(
                () => requests.length > before && requests.at(-1)?.done,
                `History metric did not load: ${id}`,
            );
            return currentView();
        };
        const initial = await currentView();
        if (initial.start !== dayStart(initial.end, false)) throw new Error('Now did not begin at local midnight');
        const today = await click('Today');
        if (today.start !== shiftDay(today.end, -1, false))
            throw new Error('Fixed Today period did not span a calendar day');
        const yesterday = await click('←');
        if (yesterday.end !== dayStart(today.end, false) || yesterday.start !== shiftDay(yesterday.end, -1, false))
            throw new Error('Today did not page to the previous full day');
        await click('→');
        if (!instance(required(control('Now')), St.Widget).has_style_class_name('tmb-tab-selected'))
            throw new Error('Forward paging did not resume Now');
        const week = await click('7d');
        if (week.start !== shiftDay(week.end, -7, false))
            throw new Error('Seven-day period did not use local calendar days');
        const utcControl = () => findActor(root, actor => actor.accessible_name === 'UTC');
        const beforeUtc = requests.length;
        clickButton(required(utcControl()));
        await waitOrFail(
            () => requests.length > beforeUtc && requests.at(-1)?.done,
            'UTC change did not reload History',
        );
        const utcWeek = await currentView();
        if (utcWeek.start !== shiftDay(utcWeek.end, -7, true))
            throw new Error('UTC change kept local calendar boundaries');
        const priorWeek = await click('←');
        if (priorWeek.end !== utcWeek.start || priorWeek.start !== shiftDay(priorWeek.end, -7, true))
            throw new Error('Seven-day paging did not retain UTC calendar boundaries');
        const restoredWeek = await click('→');
        if (
            !instance(required(control('Now')), St.Widget).has_style_class_name('tmb-tab-selected') ||
            restoredWeek.end < utcWeek.end ||
            restoredWeek.end > Math.ceil(Date.now() / 1000) ||
            restoredWeek.start !== shiftDay(restoredWeek.end, -7, true)
        )
            throw new Error(
                `Seven-day forward paging did not resume Now: ${JSON.stringify(restoredWeek)} / ${JSON.stringify(utcWeek)}`,
            );
        await click('Now');
        const beforeLocal = requests.length;
        clickButton(required(utcControl()));
        await waitOrFail(
            () => requests.length > beforeLocal && requests.at(-1)?.done,
            'Local time change did not reload History',
        );
        clickButton(required(control('Custom')));
        const apply = () => control('Apply dates');
        await waitOrFail(() => apply(), 'Custom date inputs did not open');
        const yesterdayDate = new Date();
        yesterdayDate.setDate(yesterdayDate.getDate() - 1);
        instance(required(required(apply()).get_parent()).get_children()[0], St.Entry).set_text(stamp(yesterdayDate));
        instance(required(required(apply()).get_parent()).get_children()[2], St.Entry).set_text(stamp(new Date()));
        const beforeCustom = requests.length;
        clickButton(required(apply()));
        await waitOrFail(() => requests.length > beforeCustom && requests.at(-1)?.done, 'Custom period did not load');
        const custom = await currentView();
        const beforeCustomUtc = requests.length;
        clickButton(required(utcControl()));
        await waitOrFail(
            () => requests.length > beforeCustomUtc && requests.at(-1)?.done,
            'Custom UTC period did not load',
        );
        const customUtc = await currentView();
        if (customUtc.start !== Date.parse(`${stamp(yesterdayDate)}T00:00:00Z`) / 1000)
            throw new Error('Custom UTC period kept local date boundaries');
        const beforeCustomLocal = requests.length;
        clickButton(required(utcControl()));
        await waitOrFail(
            () => requests.length > beforeCustomLocal && requests.at(-1)?.done,
            'Custom period did not restore local date boundaries',
        );
        if ((await currentView()).start !== custom.start)
            throw new Error('Custom period did not restore local date boundaries');
        const customMetric = await chooseMetric('inputTokens');
        if (customMetric.start !== customUtc.start || customMetric.end !== shiftDay(customMetric.start, 2, true))
            throw new Error(
                `Daily metric did not rebase the custom period to UTC: ${JSON.stringify(customMetric)} / ${JSON.stringify(customUtc)}`,
            );
        const customQuota = await chooseMetric('usage');
        const quotaViewedAt = Date.now();
        if (customQuota.start !== custom.start || customQuota.end < custom.end || customQuota.end - custom.end > 30)
            throw new Error(
                `Quota metric did not restore the custom local period: ${JSON.stringify(customQuota)} / ${JSON.stringify(custom)}`,
            );
        const duration = customQuota.end - customQuota.start;
        const earlier = await click('←');
        const elapsedSeconds = Math.ceil((Date.now() - quotaViewedAt) / 1000) + 2;
        if (earlier.end !== customQuota.start || Math.abs(earlier.start - (earlier.end - duration)) > elapsedSeconds)
            throw new Error(
                `Custom quota period did not page by its duration: ${JSON.stringify(earlier)} / ${JSON.stringify(customQuota)}`,
            );
        const resumedQuota = await click('→');
        if (!instance(required(control('Now')), St.Widget).has_style_class_name('tmb-tab-selected'))
            throw new Error('Custom forward paging did not resume Now');
        const liveDaily = await chooseMetric('inputTokens');
        if (
            liveDaily.start !== dayStart(resumedQuota.start, true) ||
            liveDaily.end !== shiftDay(dayStart(resumedQuota.end, true), 1, true)
        )
            throw new Error(
                `Live custom daily period did not follow UTC days: ${JSON.stringify(liveDaily)} / ${JSON.stringify(resumedQuota)}`,
            );
        clickButton(required(control('Custom')));
        const beforeDailyCustom = requests.length;
        clickButton(required(apply()));
        await waitOrFail(
            () => requests.length > beforeDailyCustom && requests.at(-1)?.done,
            'Custom daily period did not load',
        );
        const dailyCustom = await currentView();
        const dailyEarlier = await click('←');
        if (
            dailyCustom.end !== shiftDay(dailyCustom.start, 2, true) ||
            dailyEarlier.end !== dailyCustom.start ||
            dailyEarlier.start !== shiftDay(dailyEarlier.end, -2, true)
        )
            throw new Error(
                `Custom daily period did not page by whole UTC days: ${JSON.stringify(dailyEarlier)} / ${JSON.stringify(dailyCustom)}`,
            );
        await click('→');
        if (!instance(required(control('Now')), St.Widget).has_style_class_name('tmb-tab-selected')) await click('→');
        if (!instance(required(control('Now')), St.Widget).has_style_class_name('tmb-tab-selected'))
            throw new Error('Custom daily forward paging did not resume Now');
        const dailyToday = await click('Today');
        if (dailyToday.start !== shiftDay(dailyToday.end, -1, true))
            throw new Error('Daily Today did not use UTC midnight');
        const fixedQuota = await chooseMetric('usage');
        if (fixedQuota.start !== shiftDay(fixedQuota.end, -1, false))
            throw new Error('Quota metric did not rebase fixed Today to local time');
        const fixedDaily = await chooseMetric('inputTokens');
        if (fixedDaily.start !== shiftDay(fixedDaily.end, -1, true))
            throw new Error('Daily metric did not restore fixed UTC Today');
        const previousDaily = await click('←');
        if (
            previousDaily.end !== shiftDay(fixedDaily.end, -1, true) ||
            previousDaily.start !== shiftDay(previousDaily.end, -1, true)
        )
            throw new Error('Daily Today did not page to the previous UTC day');
        await click('→');
        indicator.menu.close(false);
    } finally {
        proxyCalls.call = original;
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
