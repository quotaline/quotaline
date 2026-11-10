import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import St from 'gi://St';
import type {HistoryAction, HistoryViewResponse, Window} from '../../../src/service-state.js';
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
import {readProvider} from '../../support/dbus.js';
import type {SmokeContext} from './helpers.js';
import {
    afterPaint,
    capture,
    captureStatus,
    changedAfter,
    containsText,
    copiedTextAfter,
    findActor,
    serviceState,
    setSetting,
    waitOrFail,
} from './helpers.js';

export async function run({
    indicator,
    root,
    footer,
    panelCells,
    openWithFastClock,
    demoState,
    usageColor,
    usageStatusColor,
}: Pick<
    SmokeContext,
    | 'indicator'
    | 'root'
    | 'footer'
    | 'panelCells'
    | 'openWithFastClock'
    | 'demoState'
    | 'usageColor'
    | 'usageStatusColor'
>) {
    const statusCells = () => panelCells.get_children().filter(actor => actor.has_style_class_name?.('tmb-panel-cell'));
    indicator.menu.close(false);
    openWithFastClock();
    await afterPaint(() => {});
    const usageBeforeTick = findActor(root, actor => actor.has_style_class_name?.('tmb-card'));
    await waitOrFail(
        () => findActor(root, actor => actor.has_style_class_name?.('tmb-card')) !== usageBeforeTick,
        'Open Usage menu did not update on its minute timer',
    );
    indicator.menu.close(false);
    indicator.menu.open(false);
    const scroll = instance(required(findActor(root, actor => actor instanceof St.ScrollView)), St.ScrollView);
    const scrollbar = instance(
        required(scroll.get_children().find(actor => actor instanceof St.ScrollBar)),
        St.ScrollBar,
    );
    if (scrollbar.opacity !== 0) throw new Error('Idle Usage scrollbar remains visible');
    const scrollAdjustment = scroll.get_vadjustment();
    if (scrollAdjustment.upper > scrollAdjustment.page_size) {
        scrollAdjustment.value = Math.min(scrollAdjustment.upper - scrollAdjustment.page_size, 30);
        await waitOrFail(() => scrollbar.opacity === 255, 'Usage scrollbar did not appear while scrolling');
        await waitOrFail(() => scrollbar.opacity === 0, 'Usage scrollbar did not hide after scrolling');
        scrollAdjustment.value = 0;
        await waitOrFail(() => scrollbar.opacity === 0, 'Usage scrollbar did not hide after returning to the top');
    }
    if (GLib.getenv('TMB_EXPECT_DEMO') === '1') {
        await changedAfter(() => {
            clickButton(required(findActor(footer, actor => buttonLabel(actor)?.includes('Refresh'))));
        });
        await changedAfter(() => {
            clickButton(required(findActor(root, actor => actor.accessible_name === 'Refresh Claude')));
        });
        await afterPaint(() => {});
    }
    if (GLib.getenv('TMB_EXPECT_DEMO') === '1') {
        if (
            statusCells().length < 2 ||
            !statusCells().every(
                actor =>
                    instance(actor, St.Widget).has_style_class_name('tmb-panel-cell-stacked') &&
                    actor.get_n_children() === 2,
            )
        )
            throw new Error('Stacked menu bar cells must keep separate label and percentage lines');
        if (panelCells.get_preferred_width(-1)[1] >= 400)
            throw new Error('Stacked menu bar cells use too much horizontal space');
        const separators = panelCells
            .get_children()
            .filter(actor => actor.has_style_class_name?.('tmb-panel-separator'));
        if (
            separators.length !== statusCells().length - 1 ||
            separators.some(actor => actor.width !== 1 || actor.height !== 20)
        )
            throw new Error('Stacked menu bar separators are missing or clipped');
        await waitOrFail(
            () =>
                statusCells().every(cell => {
                    const [top, bottom] = cell.get_children();
                    return (
                        required(top).get_y() < required(bottom).get_y() &&
                        Math.abs(required(top).get_width() - required(bottom).get_width()) <= 1
                    );
                }),
            `Stacked menu bar labels are not aligned in two rows: ${JSON.stringify(
                statusCells().map(cell => cell.get_children().map(row => [row.get_y(), row.get_width()])),
            )}`,
        );
        const firstPanelActor = statusCells()[0];
        const firstPanelCell = serviceState().cells.find(
            cell => cell.label === required(required(firstPanelActor).get_children()[0]).text,
        );
        const firstPanelUsed = required(firstPanelCell).restricted ? 100 : required(firstPanelCell).used;
        if (
            !required(
                instance(instance(required(firstPanelActor).get_children()[1], St.Widget), St.Widget).get_style(),
            ).includes(usageStatusColor(firstPanelUsed)) ||
            !required(
                instance(instance(required(firstPanelActor).get_children()[1], St.Widget), St.Widget).get_style(),
            ).includes(usageColor(firstPanelUsed))
        )
            throw new Error(
                `Menu bar usage color: ${JSON.stringify({
                    cell: firstPanelCell,
                    style: instance(required(firstPanelActor).get_children()[1], St.Widget).get_style(),
                })}`,
            );
        await capture('menubar-stacked');
        const comparisonKeys = [
            'claude:session',
            'claude:weekly',
            'claude:weekly:fable',
            'claude:weekly:sonnet',
            'codex:session',
            'codex:weekly',
        ];
        await setSetting('selected_windows', comparisonKeys);
        await setSetting('order', 'stable');
        await waitOrFail(
            () =>
                serviceState()
                    .cells.map(cell => cell.id)
                    .join() === comparisonKeys.join(),
            'Comparison status cells changed order',
        );
        await captureStatus('menubar-comparison');
        await setSetting('selected_windows', ['claude:weekly', 'codex:session']);
        await waitOrFail(
            () => statusCells().length === 2,
            'Two selected limits did not produce two stacked menu bar cells',
        );
        await capture('menubar-stacked-two-cells');
        await setSetting('has_custom_selection', false);
        await waitOrFail(() => statusCells().length >= 6, 'Default stacked menu bar selection was not restored');
    }
    const tabs = required(required(root.get_children()[0]).get_children()[1]).get_children();
    if (
        tabs.length !== 3 ||
        tabs.some(tab => {
            const content = contentOf(tab);
            return (
                content.get_children().length !== 2 ||
                !instance(required(content.get_children()[0]), St.Icon).get_icon_name() ||
                instance(required(content.get_children()[0]), St.Icon).icon_size !== 11 ||
                required(content.get_children()[1]).text !== tab.accessible_name
            );
        })
    )
        throw new Error('Expected labeled Usage, History, and Settings tabs with icons');
    const tabStrip = required(required(tabs[0]).get_parent());
    const [tabX] = tabStrip.get_transformed_position();
    const [rootX] = root.get_transformed_position();
    if (
        tabStrip.width < 265 ||
        tabStrip.width > 285 ||
        Math.abs(tabX + tabStrip.width / 2 - rootX - root.width / 2) > 2
    )
        throw new Error(`Tab strip is oversized or off-center: ${tabStrip.width}px at ${tabX - rootX}px`);
    if (GLib.getenv('TMB_EXPECT_DEMO') === '1') {
        await waitOrFail(
            () => containsText(contentOf(root.get_children()[1]), 'API-EQUIVALENT ESTIMATES'),
            'Claude daily cost history did not reach Usage',
        );
        await waitOrFail(
            () => containsText(contentOf(root.get_children()[1]), 'Code reviews'),
            'Codex code reviews did not reach Usage',
        );
        const now = new Date();
        const tomorrow = Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate() + 1) / 1000;
        const historyView = (action: HistoryAction) =>
            JSON.parse(
                (
                    Gio.DBus.session
                        .call_sync(
                            'tech.bernat.Quotaline.Gnome',
                            '/tech/bernat/Quotaline/Gnome',
                            'tech.bernat.Quotaline.Gnome',
                            'GetHistoryView',
                            new GLib.Variant('(ssi)', [JSON.stringify(action), 'UTC', 0]),
                            new GLib.VariantType('(s)'),
                            Gio.DBusCallFlags.NONE,
                            2000,
                            null,
                        )
                        .deep_unpack() as [string]
                )[0],
            ) as HistoryViewResponse;
        const {view} = historyView({kind: 'load'});
        historyView({kind: 'metric', metric: 'codeReviews'});
        const {history} = historyView({kind: 'period', days: 60});
        for (const action of [
            {kind: 'period', days: view.days},
            {kind: 'metric', metric: view.metric},
            {kind: 'now'},
        ] as const)
            historyView(action);
        const points = required(history)
            .series.filter(series => series.provider === 'codex')
            .flatMap(series => series.points);
        const countSince = (start: number) =>
            Math.trunc(
                points
                    .filter(point => point[0] >= start && point[0] < tomorrow)
                    .reduce((count: number, point) => count + point[1], 0),
            );
        const expectedReviews = `${countSince(tomorrow - 86400)} today · ${countSince(tomorrow - 7 * 86400)} this week`;
        await waitOrFail(
            () =>
                findActor(contentOf(root.get_children()[1]), actor =>
                    actor.has_style_class_name?.('tmb-review-summary'),
                )?.get_children()[1]?.text === expectedReviews,
            'Codex review counts differ from daily analytics',
        );
        await afterPaint(() => {});
        const page = contentOf(root.get_children()[1]);
        const cards = page.get_children().filter((actor: TestActor) => actor.has_style_class_name?.('tmb-card'));
        if (cards.length !== 6) throw new Error(`Expected six demo provider cards, got ${cards.length}`);
        if (
            !['Antigravity', 'Claude', 'Codex', 'GitHub Copilot', 'Cursor', 'Gemini'].every((name, index) =>
                containsText(required(cards[index]).get_children()[0], name),
            )
        )
            throw new Error('Usage cards do not follow the macOS provider order');
        const iconFiles = cards.map(card =>
            optionalInstance(
                optionalInstance(
                    findActor(card, actor => actor.has_style_class_name?.('tmb-provider-icon')),
                    St.Icon,
                )?.gicon,
                Gio.FileIcon,
            )?.get_file(),
        );
        if (
            iconFiles.some(
                (file, index) =>
                    !file ||
                    !GLib.file_test(required(file.get_path()), GLib.FileTest.EXISTS) ||
                    required(file.get_basename()) !==
                        [
                            'antigravity-symbolic.svg',
                            'claude.svg',
                            'codex-symbolic.svg',
                            'copilot-symbolic.svg',
                            'cursor-symbolic.svg',
                            'gemini.svg',
                        ][index],
            )
        )
            throw new Error('Provider cards do not use the expected monochrome and color marks');
        for (const [index, message] of [
            [1, 'Weekly limits are boosted 2x until the next reset.'],
            [2, 'Usage limits are doubled through the end of the month.'],
            [3, 'Chat and completions are unlimited on this plan.'],
        ] as const) {
            const notice = findActor(cards[index], actor => actor.has_style_class_name?.('tmb-provider-notice'));
            if (
                !notice ||
                !containsText(notice, message) ||
                notice.get_transformed_position()[1] >=
                    required(
                        findActor(cards[index], actor => actor.has_style_class_name?.('tmb-window')),
                    ).get_transformed_position()[1]
            )
                throw new Error(`Provider notice is missing or below the quota rows: ${message}`);
        }
        const claudeCard = cards[1];
        const codexCard = cards[2];
        const renewal = findActor(codexCard, actor => buttonLabel(actor)?.startsWith('Renews '));
        if (!renewal?.reactive || !accessibleName(renewal)?.startsWith('Copy renewal date: Renews '))
            throw new Error('Codex renewal date is missing from its Usage card');
        const copiedRenewal = await copiedTextAfter(() => {
            clickButton(renewal);
        });
        if (copiedRenewal !== instance(renewal, St.Button).get_label())
            throw new Error('Codex renewal chip did not copy its displayed date');
        const antigravityRows = required(required(cards[0]).get_children()[1])
            .get_children()
            .filter((actor: TestActor) => actor.has_style_class_name?.('tmb-window'));
        const claudeBody = required(claudeCard).get_children()[1];
        const claudeRows = required(claudeBody)
            .get_children()
            .filter((actor: TestActor) => actor.has_style_class_name?.('tmb-window'));
        const codexRows = required(required(codexCard).get_children()[1])
            .get_children()
            .filter((actor: TestActor) => actor.has_style_class_name?.('tmb-window'));
        if (
            antigravityRows.length !== 2 ||
            !['Gemini 5-hour', 'Gemini weekly'].every((title, index) => containsText(antigravityRows[index], title)) ||
            codexRows.length !== 2 ||
            !['5-hour', 'Weekly'].every((title, index) => containsText(codexRows[index], title)) ||
            claudeRows.length !== 4 ||
            !findActor(claudeRows[0], actor => actor.has_style_class_name?.('tmb-quota-reset')) ||
            claudeRows
                .slice(1)
                .some((actor: TestActor) =>
                    findActor(actor, child => child.has_style_class_name?.('tmb-quota-reset')),
                ) ||
            required(claudeBody)
                .get_children()
                .filter((actor: TestActor) => actor.has_style_class_name?.('tmb-quota-group-reset')).length !== 1
        )
            throw new Error('Windows with one reset are not grouped like the macOS cards');
        if (
            !containsText(claudeRows[0], 'window · 5h') ||
            !containsText(claudeRows[2], 'weekly:fable') ||
            !containsText(antigravityRows[0], 'gemini:session')
        )
            throw new Error('Quota rows omit their macOS window details');
        const currentClaude = readProvider('claude').windows[0];
        const claudeFill = findActor(claudeCard, actor => actor.has_style_class_name?.('tmb-meter-fill'), St.Widget);
        if (
            !required(claudeFill?.get_style()).includes(
                usageColor(required(currentClaude).used, required(currentClaude).pace.status),
            )
        )
            throw new Error('Usage meter color does not match the macOS scale');
        const header = findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-header'));
        const headerInfo = required(findActor(header, actor => actor.has_style_class_name?.('tmb-header-info')));
        const title = findActor(header, actor => actor.text === 'Quotaline');
        const updated = findActor(header, actor => actor.has_style_class_name?.('tmb-header-updated'));
        const activity = findActor(header, actor => actor.has_style_class_name?.('tmb-header-activity'));
        if (
            !header ||
            !title ||
            updated?.text !== 'Updated just now' ||
            activity?.text !== 'Demo data' ||
            !containsText(claudeCard, 'fetched just now') ||
            !findActor(
                header,
                actor =>
                    optionalInstance(actor.gicon, Gio.FileIcon)?.get_file().get_basename() === 'quotaline-symbolic.svg',
            ) ||
            findActor(page, actor => actor.has_style_class_name?.('tmb-usage-summary'))
        )
            throw new Error('Usage title or provider freshness is missing');
        if (
            root.width >= 480 &&
            (!headerInfo.visible ||
                header.height >= 50 ||
                Math.abs(title.get_transformed_position()[1] - required(tabs[0]).get_transformed_position()[1]) > 12)
        )
            throw new Error('Usage title and tabs do not share a compact header row');
        if (root.width < 480 && headerInfo.visible)
            throw new Error('The narrow header clips the tabs with a visible title');
        if (
            !containsText(claudeCard, 'Cloud session credits') ||
            !containsText(claudeCard, '$83.00 of $250.00 left') ||
            !containsText(claudeCard, 'Expires')
        )
            throw new Error('Claude cloud session credit is missing');
        const paceTitle = {
            on_track: 'On pace',
            ahead: root.width < 790 ? 'Ahead' : 'Ahead of pace',
            behind: 'Under pace',
            unknown: 'Learning pace',
            exhausted: 'Limit reached',
        };
        const paceText = (row: Window) =>
            row.pace.expected_percent === null
                ? paceTitle[row.pace.status]
                : `${paceTitle[row.pace.status]} · ${root.width < 790 ? 'exp' : 'expected'}`;
        if (
            ![required(demoState), serviceState()].some(
                ({providers: {claude, antigravity}}) =>
                    containsText(claudeCard, paceText(required(required(claude).windows[0]))) &&
                    containsText(antigravityRows[0], paceText(required(required(antigravity).windows[0]))) &&
                    containsText(antigravityRows[1], paceText(required(required(antigravity).windows[1]))),
            )
        )
            throw new Error('Usage pace labels differ from the macOS card');
        const costLabels =
            root.width < 440 ? ['Today', 'Yesterday', '30 days'] : ['Today (UTC)', 'Yesterday (UTC)', 'Last 30 days'];
        if (!costLabels.every(title => containsText(claudeCard, title)))
            throw new Error('Claude daily cost tiles are missing');
        const claudeSpend = findActor(
            claudeCard,
            actor => actor.has_style_class_name?.('tmb-spend') && containsText(actor, 'Usage credits'),
        );
        const costSummary = required(findActor(claudeCard, actor => actor.has_style_class_name?.('tmb-cost-summary')));
        if (
            !claudeSpend ||
            !containsText(claudeSpend, 'Usage credits') ||
            !containsText(claudeSpend, '$12.40 of $50.00 · 25% used') ||
            claudeSpend.get_y() <= costSummary.get_y()
        )
            throw new Error(
                `Claude spend section is missing or precedes cost history: ${JSON.stringify({
                    spend: claudeSpend?.get_y(),
                    cost: costSummary.get_y(),
                    credits: claudeSpend && containsText(claudeSpend, 'Usage credits'),
                    amount: claudeSpend && containsText(claudeSpend, '$12.40 of $50.00 · 25% used'),
                })}`,
            );
        const codexSpend = findActor(codexCard, actor => actor.has_style_class_name?.('tmb-spend'));
        if (
            !codexSpend ||
            !containsText(codexSpend, 'Unavailable of $200.00') ||
            findActor(codexSpend, actor => actor.has_style_class_name?.('tmb-meter-fill'))
        )
            throw new Error('Codex credit allowance does not show its unreported usage');
        const cursorSpend = findActor(cards[4], actor => actor.has_style_class_name?.('tmb-spend'));
        const {cursor} = required(demoState).providers;
        const cursorUsed = required(required(cursor).windows[1]).used;
        if (
            !cursorSpend ||
            !containsText(cursorSpend, 'Spend control') ||
            !containsText(
                cursorSpend,
                `$${(Math.trunc(cursorUsed * 50) / 100).toFixed(2)} of $50.00 · ${Math.round(cursorUsed)}% used`,
            ) ||
            !findActor(cursorSpend, actor => actor.has_style_class_name?.('tmb-meter-fill'))
        )
            throw new Error('Cursor spend summary and meter are missing');
        if (!containsText(cards[5], 'Credits $1500.00 remaining'))
            throw new Error('Gemini reference credits are missing');
        const cardHeader = required(claudeCard).get_children()[0];
        const cardTop = findActor(cardHeader, actor => actor.has_style_class_name?.('tmb-card-header-top'));
        const cardBadges = findActor(
            cardHeader,
            actor => actor.has_style_class_name?.('tmb-card-header-badges') && containsText(actor, 'Max 20x'),
        );
        if (
            !cardBadges ||
            !containsText(cardBadges, 'you@example.com') ||
            root.width >= 560 !== (cardBadges.get_parent() === cardTop)
        )
            throw new Error('Claude plan and account do not share the title row when they fit');
        const accountChip = findActor(cardBadges, actor => buttonLabel(actor) === 'you@example.com');
        if (!accountChip?.reactive) throw new Error('Claude account chip is not actionable');
        const copiedAccount = await copiedTextAfter(() => {
            clickButton(accountChip);
        });
        if (copiedAccount !== 'you@example.com')
            throw new Error('Claude account chip did not copy its displayed value');
        const currentClaudeCard = () =>
            contentOf(root.get_children()[1])
                .get_children()
                .filter((actor: TestActor) => actor.has_style_class_name?.('tmb-card'))[1];
        await waitOrFail(
            () =>
                currentClaudeCard() &&
                findActor(currentClaudeCard(), actor => actor.has_style_class_name?.('tmb-quota-row')),
            'Claude quota row disappeared after copying the account',
        );
        const visibleClaudeCard = currentClaudeCard();
        if (root.width < 440) {
            const updated = findActor(visibleClaudeCard, actor => actor.has_style_class_name?.('tmb-card-updated'));
            const costs = findActor(visibleClaudeCard, actor => actor.has_style_class_name?.('tmb-cost-summary'));
            const tiles = required(findActor(costs, actor => actor.has_style_class_name?.('tmb-cost-tiles')));
            const visibleBadges = findActor(
                visibleClaudeCard,
                actor => actor.has_style_class_name?.('tmb-card-header-badges') && containsText(actor, 'Max 20x'),
            );
            if (
                !updated ||
                !visibleBadges ||
                updated.get_y() <= visibleBadges.get_y() ||
                !findActor(costs, actor => actor.text === 'API-EQUIVALENT ESTIMATES · UTC') ||
                tiles.get_children().some(tile => {
                    const title = tile.get_children()[0];
                    return required(title).width < required(title).get_preferred_width(-1)[1];
                })
            )
                throw new Error('Compact freshness or cost labels are clipped');
        }
        const quota = required(findActor(visibleClaudeCard, actor => actor.has_style_class_name?.('tmb-quota-row')));
        if (root.width < 600) {
            const [title, value] = quota.get_children();
            const track = findActor(quota.get_parent(), actor => actor.has_style_class_name?.('tmb-meter'));
            const reset = findActor(quota.get_parent(), actor =>
                actor.has_style_class_name?.('tmb-quota-reset-compact'),
            );
            if (
                !track ||
                !reset ||
                required(title).get_x() >= required(value).get_x() ||
                track.width > required(quota.get_parent()).width
            )
                throw new Error('Compact quota row clips the meter or reset');
        } else if (root.width < 790) {
            const [title, meterColumn, value, reset] = quota.get_children();
            const [track, pace] = required(meterColumn).get_children();
            if (
                required(track).width < 160 ||
                required(pace).get_y() <= required(track).get_y() ||
                required(title).get_x() >= required(meterColumn).get_x() ||
                required(meterColumn).get_x() >= required(value).get_x() ||
                required(value).get_x() >= required(reset).get_x() ||
                !required(required(reset).text).includes('Resets in')
            )
                throw new Error('Medium quota row lost its inline columns');
        } else {
            await waitOrFail(() => {
                const card = currentClaudeCard();
                const row = card && findActor(card, actor => actor.has_style_class_name?.('tmb-quota-row'));
                if (!row) return false;
                const [title, meterColumn, value, reset] = row.get_children();
                const [track, pace] = required(meterColumn).get_children();
                return (
                    required(track).get_width() >= 300 &&
                    required(pace).get_y() > required(track).get_y() &&
                    required(title).get_x() < required(meterColumn).get_x() &&
                    required(meterColumn).get_x() < required(value).get_x() &&
                    required(value).get_x() < required(reset).get_x() &&
                    required(required(reset).text).includes('\n')
                );
            }, 'Wide quota row does not match the meter, pace and reset columns');
        }
        const costDisclosure = () =>
            findActor(currentClaudeCard(), actor => buttonLabel(actor)?.includes('Model cost estimates'));
        if (!costDisclosure() || containsText(costSummary, 'Estimated from recorded tokens'))
            throw new Error('Claude model cost estimates did not start collapsed');
        clickButton(required(costDisclosure()));
        await waitOrFail(() => {
            const costs = findActor(currentClaudeCard(), actor => actor.has_style_class_name?.('tmb-cost-summary'));
            return containsText(costs, 'Estimated from recorded tokens') && containsText(costs, 'fable');
        }, 'Claude model cost estimates did not open');
        clickButton(required(costDisclosure()));
        if (
            containsText(
                findActor(currentClaudeCard(), actor => actor.has_style_class_name?.('tmb-cost-summary')),
                'Estimated from recorded tokens',
            )
        )
            throw new Error('Claude model cost estimates did not close');
        const localDisclosure = () =>
            findActor(currentClaudeCard(), actor => buttonLabel(actor)?.includes('Local usage details'));
        if (
            !localDisclosure() ||
            findActor(currentClaudeCard(), actor => actor.has_style_class_name?.('tmb-local-summary'))
        )
            throw new Error('Demo local usage did not start collapsed');
        clickButton(required(localDisclosure()));
        const localSummary = findActor(currentClaudeCard(), actor => actor.has_style_class_name?.('tmb-local-summary'));
        const local = serviceState().claude_local_usage;
        const compact = (value: number) =>
            new Intl.NumberFormat(undefined, {notation: 'compact', maximumFractionDigits: 1}).format(value);
        const money = (value: number) =>
            new Intl.NumberFormat(undefined, {
                style: 'currency',
                currency: 'USD',
                maximumFractionDigits: value < 10 ? 2 : 0,
            }).format(value);
        if (
            !localSummary ||
            ![
                `${compact(required(local).window_tokens)} tokens`,
                money(required(local).window_cost),
                `${money(required(local).cost_per_hour)}/hr`,
                `${compact(required(local).today_tokens)} tokens`,
                String(required(local).today_messages),
                money(required(local).today_cost),
            ].every(value => containsText(localSummary, value))
        )
            throw new Error('Demo local usage differs from the macOS reference');
        clickButton(required(localDisclosure()));
        if (findActor(currentClaudeCard(), actor => actor.has_style_class_name?.('tmb-local-summary')))
            throw new Error('Demo local usage did not close');
        await capture('usage');
        const usageScroll = instance(root.get_children()[1], St.ScrollView).get_vadjustment();
        await afterPaint(() => {
            usageScroll.set_value((usageScroll.upper - usageScroll.page_size) / 2);
        });
        await capture('usage-middle');
        await afterPaint(() => {
            usageScroll.set_value(usageScroll.upper - usageScroll.page_size);
        });
        await capture('usage-bottom');
        usageScroll.set_value(0);
        const currentCodexCard = () =>
            contentOf(root.get_children()[1])
                .get_children()
                .filter((actor: TestActor) => actor.has_style_class_name?.('tmb-card'))[2];
        if (
            !containsText(currentCodexCard(), 'No cost history in the last 30 days') ||
            findActor(currentCodexCard(), actor => actor.has_style_class_name?.('tmb-cost-summary'))
        )
            throw new Error('Codex empty cost history did not show its empty state');
        if (
            !containsText(currentCodexCard(), 'Credits') ||
            !containsText(currentCodexCard(), '$42.50') ||
            !containsText(currentCodexCard(), '1 available') ||
            !containsText(currentCodexCard(), 'Next reset-credit expiry') ||
            !containsText(currentCodexCard(), 'Reset purchase') ||
            containsText(currentCodexCard(), 'Local messages') ||
            containsText(currentCodexCard(), 'Bonus reset')
        )
            throw new Error('Codex credits do not match the compact summary');
        clickButton(
            required(
                findActor(currentCodexCard(), actor => buttonLabel(actor)?.includes('Credit estimates and totals')),
            ),
        );
        if (
            !containsText(currentCodexCard(), 'Local messages') ||
            !containsText(currentCodexCard(), '~120–260') ||
            !containsText(currentCodexCard(), 'Cloud messages') ||
            !containsText(currentCodexCard(), '~60–130') ||
            !containsText(currentCodexCard(), 'Resets earned') ||
            !containsText(currentCodexCard(), '3')
        )
            throw new Error('Codex credit estimates did not open');
        clickButton(
            required(findActor(currentCodexCard(), actor => buttonLabel(actor)?.includes('Reset credit details'))),
        );
        if (
            !containsText(currentCodexCard(), 'Bonus reset') ||
            !containsText(currentCodexCard(), 'One extra session reset')
        )
            throw new Error('Codex reset details did not open');
        const rowsFor = (index: number) =>
            required(
                required(
                    contentOf(root.get_children()[1])
                        .get_children()
                        .filter((actor: TestActor) => actor.has_style_class_name?.('tmb-card'))
                        .at(index),
                ).get_children()[1],
            )
                .get_children()
                .filter((actor: TestActor) => actor.has_style_class_name?.('tmb-window'));
        await setSetting('selected_windows', ['antigravity:gemini:session']);
        await waitOrFail(
            () =>
                rowsFor(0).length === 1 &&
                containsText(rowsFor(0)[0], 'Gemini 5-hour') &&
                rowsFor(1).length === 0 &&
                containsText(contentOf(root.get_children()[1]), 'Cloud session credits'),
            'Usage did not follow custom model selection while preserving provider details',
        );
        await setSetting('has_custom_selection', false);
        await waitOrFail(
            () => rowsFor(0).length === 2 && rowsFor(1).length === 4,
            'Usage did not restore the default model selection',
        );
    }
    if (GLib.getenv('TMB_EXPECT_LOCAL') === '1') {
        await waitOrFail(() => containsText(root, 'Local usage details'), 'Local usage disclosure did not load');
        const page = contentOf(root.get_children()[1]);
        const cards = page.get_children().filter((actor: TestActor) => actor.has_style_class_name?.('tmb-card'));
        if (cards.length !== 1) throw new Error(`Expected one Claude local card, got ${cards.length}`);
        if (findActor(cards[0], actor => actor.has_style_class_name?.('tmb-local-summary')))
            throw new Error('Local usage details did not start collapsed');
        const disclosure = required(findActor(cards[0], actor => buttonLabel(actor)?.includes('Local usage details')));
        clickButton(disclosure);
        const local = findActor(root, actor => actor.has_style_class_name?.('tmb-local-summary'));
        if (
            !local ||
            !containsText(local, 'Messages today') ||
            !containsText(local, '1') ||
            !containsText(local, '$1.20')
        )
            throw new Error('Claude local session totals were not rendered');
        clickButton(required(findActor(root, actor => buttonLabel(actor)?.includes('Local usage details'))));
        if (findActor(root, actor => actor.has_style_class_name?.('tmb-local-summary')))
            throw new Error('Local usage details did not close');
    }
}
