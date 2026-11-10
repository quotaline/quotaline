import Atk from 'gi://Atk';
import Gio from 'gi://Gio';
import GioUnix from 'gi://GioUnix';
import GLib from 'gi://GLib';
import St from 'gi://St';
import type {TestActor} from '../../support/actors.js';
import {buttonLabel, clickButton, contentOf, instance, required} from '../../support/actors.js';
import {readProvider, readProviderSettings} from '../../support/dbus.js';
import type {SmokeContext} from './helpers.js';
import {
    afterPaint,
    capture,
    changedAfter,
    checkMark,
    containsText,
    copiedTextAfter,
    findActor,
    serviceLogs,
    serviceSettings,
    serviceState,
    setSetting,
    waitOrFail,
} from './helpers.js';

export async function run({
    root,
    footer,
    adjustment,
    source,
}: Pick<SmokeContext, 'root' | 'footer' | 'adjustment' | 'source'>) {
    clickButton(required(required(required(root.get_children()[0]).get_children()[1]).get_children()[2]));
    await afterPaint(() => {});
    if (
        !required(
            required(
                required(findActor(footer, actor => buttonLabel(actor) === 'Copy diagnostics')).get_parent(),
            ).get_parent(),
        ).visible
    )
        throw new Error('Settings footer actions are hidden on Settings');
    const diagnostics = await copiedTextAfter(
        () => {
            clickButton(required(findActor(footer, actor => buttonLabel(actor) === 'Copy diagnostics')));
        },
        value => value?.includes('"providers"'),
    );
    if (!diagnostics.includes('"claude"') || diagnostics.includes('you@example.com'))
        throw new Error('Copy diagnostics did not produce a private service report');
    adjustment.set_value(0);
    await afterPaint(() => {});
    await capture('settings');
    if (GLib.getenv('TMB_EXPECT_DEMO') === '1') {
        const page = contentOf(root.get_children()[1]);
        const preview = findActor(page, actor => actor.has_style_class_name?.('tmb-status-preview'));
        if (
            preview?.get_children().flatMap(row => row.get_children()).length !== 13 ||
            preview.height < 38 ||
            !preview
                .get_children()
                .flatMap(row => row.get_children())
                .every(actor => instance(actor, St.Widget).has_style_class_name('tmb-preview-cell'))
        )
            throw new Error('Settings preview does not show all quota cells in a full-height track');
        clickButton(
            required(
                preview
                    .get_children()
                    .flatMap(row => row.get_children())
                    .at(-1),
            ),
        );
        await waitOrFail(
            () =>
                (adjustment.upper <= adjustment.page_size || adjustment.value > 0) &&
                findActor(
                    root,
                    actor =>
                        actor.has_style_class_name?.('tmb-short-entry') &&
                        instance(actor, St.Entry).clutter_text.has_key_focus(),
                ),
            'Selecting a preview cell did not reveal and focus its model label',
        );
        adjustment.set_value(0);
        await capture('settings-preview-focus');
        const providerOrder = serviceSettings().provider_order;
        const providerDrag = findActor(root, actor => actor.accessible_name === 'Drag Claude to reorder');
        const providerDrop = findActor(
            root,
            actor =>
                actor.has_style_class_name?.('tmb-model-heading') &&
                actor._delegate &&
                'acceptDrop' in actor._delegate &&
                containsText(actor, 'Codex'),
        );
        if (
            !providerDrag?.get_actions().length ||
            !providerDrop ||
            dropTarget(providerDrop).handleDragOver(providerDrag._delegate) !== 2 ||
            dropTarget(providerDrop).acceptDrop(providerDrop._delegate, null, 0, 0, 0)
        )
            throw new Error('Provider drag target did not accept a different provider');
        const dragPreview = dragSource(providerDrag).getDragActor();
        if (dragPreview.text !== 'Claude' || !dragPreview.has_style_class_name('tmb-drag-preview'))
            throw new Error('Provider drag did not show its source label');
        await changedAfter(() => dropTarget(providerDrop).acceptDrop(providerDrag._delegate, null, 0, 1000, 0));
        if (
            serviceSettings().provider_order.indexOf('claude') !==
            serviceSettings().provider_order.indexOf('codex') + 1
        )
            throw new Error('Dropping Claude after Codex did not save their order');
        await setSetting('provider_order', providerOrder);
        await afterPaint(() => {});
        const modelOrder = serviceSettings().model_order;
        const modelDrag = findActor(root, actor => actor.accessible_name === 'Drag Claude Current session to reorder');
        const modelDrop = findActor(
            root,
            actor =>
                actor.has_style_class_name?.('tmb-model-row') &&
                actor._delegate &&
                'acceptDrop' in actor._delegate &&
                containsText(actor, 'All models'),
        );
        const otherModel = findActor(root, actor => actor.accessible_name === 'Drag Codex 5-hour to reorder');
        if (
            !modelDrag?.get_actions().length ||
            !modelDrop ||
            !otherModel ||
            dropTarget(modelDrop).handleDragOver(otherModel._delegate) !== 0 ||
            dropTarget(modelDrop).acceptDrop(otherModel._delegate, null, 0, 0, 0)
        )
            throw new Error('Model drag target accepted a different provider');
        await changedAfter(() => dropTarget(modelDrop).acceptDrop(modelDrag._delegate, null, 0, 1000, 0));
        if (
            serviceSettings().model_order.indexOf('claude:session') !==
            serviceSettings().model_order.indexOf('claude:weekly') + 1
        )
            throw new Error('Dropping a model after its neighbor did not save their order');
        await setSetting('model_order', modelOrder);
        if (root.width >= 800) {
            const controls = ['Countdown at 100%', 'Remaining'].map(title =>
                findActor(page, actor => buttonLabel(actor) === title),
            );
            const right = root.get_transformed_position()[0] + root.width - 12;
            if (
                !findActor(page, actor => actor.text === 'Show usage as') ||
                controls.some(actor => !actor || actor.get_transformed_position()[0] + actor.width > right)
            )
                throw new Error('Full macOS display labels are missing or clipped in wide Settings');
        }
    }
    if (!['Privacy Policy', 'Acknowledgements'].every(title => containsText(contentOf(root.get_children()[1]), title)))
        throw new Error('Legal links are missing from Settings');
    if (!findActor(contentOf(root.get_children()[1]), actor => buttonLabel(actor) === 'Open Extensions'))
        throw new Error('GNOME startup control is missing from Settings');
    const launched: string[] = [];
    const launchUri = Gio.AppInfo.launch_default_for_uri.bind(Gio.AppInfo);
    Gio.AppInfo.launch_default_for_uri = uri => {
        launched.push(uri);
        global.stage.queue_redraw();
        return true;
    };
    try {
        const settingsPage = contentOf(root.get_children()[1]);
        const copiedPath = await copiedTextAfter(() => {
            clickButton(required(findActor(settingsPage, actor => actor.accessible_name === 'Copy history path')));
        });
        if (copiedPath !== serviceState().history_path)
            throw new Error('History path control did not copy the full path');
        for (const title of ['Privacy Policy', 'Acknowledgements'] as const)
            clickButton(required(findActor(settingsPage, actor => buttonLabel(actor) === title)));
        clickButton(required(findActor(settingsPage, actor => buttonLabel(actor) === 'Open folder')));
        for (const title of ['Report issue', 'Source'] as const)
            clickButton(required(findActor(footer, actor => buttonLabel(actor) === title)));
        await waitOrFail(() => launched.some(uri => uri.includes('/issues/new?')), 'Report issue did not open a draft');
    } finally {
        Gio.AppInfo.launch_default_for_uri = launchUri;
    }
    const issue = launched.find(uri => uri.includes('/issues/new?'));
    if (
        !launched.includes(`${source}/privacy.html`) ||
        !launched.includes(`${source}/notices.html`) ||
        !launched.includes(Gio.File.new_for_path(GLib.path_get_dirname(serviceState().history_path)).get_uri()) ||
        !issue?.includes('title=Issue+report') ||
        issue.length > 8000 ||
        !decodeURIComponent(issue).includes('"providers"') ||
        !launched.includes('https://github.com/quotaline/quotaline')
    )
        throw new Error('Settings and footer links did not open their destinations');
    if (GLib.getenv('TMB_EXPECT_DEMO') === '1') {
        Gio.AppInfo.launch_default_for_uri = () => {
            throw new Error('No document handler');
        };
        try {
            clickButton(
                required(
                    findActor(contentOf(root.get_children()[1]), actor => buttonLabel(actor) === 'Acknowledgements'),
                ),
            );
        } finally {
            Gio.AppInfo.launch_default_for_uri = launchUri;
        }
        if (!containsText(contentOf(root.get_children()[1]), 'No document handler'))
            throw new Error('Document launch failure was not explained in Settings');
        const desktopApps: {'new': (id: string) => GioUnix.DesktopAppInfo | null} = GioUnix.DesktopAppInfo;
        const desktopApp = desktopApps.new;
        const desktopMethods: {
            launch: (
                this: GioUnix.DesktopAppInfo,
                files: Gio.File[] | null,
                context: Gio.AppLaunchContext | null,
            ) => boolean;
        } = GioUnix.DesktopAppInfo.prototype;
        const desktopLaunch = desktopMethods.launch;
        try {
            const openedApp: {id: string | null} = {id: null};
            desktopMethods.launch = function () {
                openedApp.id = this.get_id();
                return true;
            };
            clickButton(
                required(
                    findActor(contentOf(root.get_children()[1]), actor => buttonLabel(actor) === 'Open Extensions'),
                ),
            );
            if (openedApp.id !== 'org.gnome.Shell.Extensions.desktop')
                throw new Error('Startup control did not open GNOME Extensions');
            desktopApps.new = () => null;
            clickButton(
                required(
                    findActor(contentOf(root.get_children()[1]), actor => buttonLabel(actor) === 'Open Extensions'),
                ),
            );
            if (!containsText(contentOf(root.get_children()[1]), 'GNOME Extensions is not installed'))
                throw new Error('Missing Extensions app was not explained');
            desktopApps.new = desktopApp;
            desktopMethods.launch = () => {
                throw new Error('Cannot start Extensions');
            };
            clickButton(
                required(
                    findActor(contentOf(root.get_children()[1]), actor => buttonLabel(actor) === 'Open Extensions'),
                ),
            );
            if (!containsText(contentOf(root.get_children()[1]), 'Cannot start Extensions'))
                throw new Error('Extensions launch failure was not explained');
        } finally {
            desktopApps.new = desktopApp;
            desktopMethods.launch = desktopLaunch;
        }
        clickButton(
            required(
                findActor(
                    contentOf(root.get_children()[1]),
                    actor => actor.accessible_name === 'Hide account and project details',
                ),
            ),
        );
        await waitOrFail(
            () => serviceSettings().hide_identity && readProvider('claude').identity === 'account',
            'Privacy control did not mask the service account',
        );
        clickButton(required(required(required(root.get_children()[0]).get_children()[1]).get_children()[0]));
        const accountChip = () =>
            findActor(
                contentOf(root.get_children()[1]),
                actor => actor.has_style_class_name?.('tmb-card-header-badges') && containsText(actor, 'Max 20x'),
            );
        await waitOrFail(
            () => {
                const badges = accountChip();
                return badges && containsText(badges, 'account') && !containsText(badges, 'you@example.com');
            },
            () =>
                `Usage did not replace the account chip: ${JSON.stringify({
                    badges: accountChip()
                        ?.get_children()
                        .map(actor => actor.text),
                    state: readProvider('claude').identity,
                    setting: serviceSettings().hide_identity,
                })}`,
        );
        clickButton(required(required(required(root.get_children()[0]).get_children()[1]).get_children()[2]));
        clickButton(
            required(
                findActor(
                    contentOf(root.get_children()[1]),
                    actor => actor.accessible_name === 'Hide account and project details',
                ),
            ),
        );
        await waitOrFail(() => !serviceSettings().hide_identity, 'Privacy control did not restore account details');
    }
    adjustment.set_value((adjustment.upper - adjustment.page_size) * 0.57);
    await afterPaint(() => {});
    await capture('settings-providers');
    if (GLib.getenv('TMB_EXPECT_DEMO') === '1') {
        const disclosure = findActor(
            contentOf(root.get_children()[1]),
            actor => actor.accessible_name === 'Connection details for Claude',
        );
        if (!disclosure || !Gio.File.new_for_uri(`${source}/provider-setup.html`).query_exists(null))
            throw new Error('Claude connection details or local setup guide is missing');
        clickButton(disclosure);
        const details = findActor(
            contentOf(root.get_children()[1]),
            actor =>
                actor.has_style_class_name?.('tmb-provider-setting-details') &&
                containsText(actor, 'Account · you@example.com'),
        );
        if (
            !details?.visible ||
            !containsText(details, 'Authentication · Demo data') ||
            !findActor(details, actor => actor.accessible_name === 'Setup guide for Claude')
        )
            throw new Error('Claude connection details omitted the source, account, or guide');
        const setupUri: {value: string | null} = {value: null};
        Gio.AppInfo.launch_default_for_uri = uri => {
            setupUri.value = uri;
            return true;
        };
        try {
            clickButton(required(findActor(details, actor => actor.accessible_name === 'Setup guide for Claude')));
        } finally {
            Gio.AppInfo.launch_default_for_uri = launchUri;
        }
        if (setupUri.value !== `${source}/provider-setup.html#claude`)
            throw new Error('Claude setup guide did not open its provider section');
        await afterPaint(() => {});
        const codexRow = required(
            findActor(
                contentOf(root.get_children()[1]),
                actor => actor.has_style_class_name?.('tmb-setting-row') && actor.get_children()[0]?.text === 'Codex',
            ),
        );
        if (details.get_transformed_position()[1] + details.height > codexRow.get_transformed_position()[1])
            throw new Error(
                `Claude details overlap Codex: ${details.height}, ${codexRow.get_transformed_position()[1]}`,
            );
        const providerHeading = required(
            findActor(
                contentOf(root.get_children()[1]),
                actor => actor.has_style_class_name?.('tmb-section') && actor.text === 'PROVIDERS',
            ),
        );
        adjustment.set_value(
            adjustment.value + providerHeading.get_transformed_position()[1] - root.get_transformed_position()[1] - 80,
        );
        await afterPaint(() => {});
        await capture('settings-provider-details');
        clickButton(
            required(
                findActor(
                    contentOf(root.get_children()[1]),
                    actor => actor.accessible_name === 'Hide account and project details',
                ),
            ),
        );
        await waitOrFail(() => {
            const masked = findActor(
                contentOf(root.get_children()[1]),
                actor =>
                    actor.has_style_class_name?.('tmb-provider-setting-details') &&
                    actor.visible &&
                    containsText(actor, 'Authentication · Demo data'),
            );
            return (
                serviceSettings().hide_identity &&
                masked &&
                containsText(masked, 'Account · account') &&
                !containsText(masked, 'you@example.com')
            );
        }, 'Provider connection details revealed the account in privacy mode');
        clickButton(
            required(
                findActor(
                    contentOf(root.get_children()[1]),
                    actor => actor.accessible_name === 'Hide account and project details',
                ),
            ),
        );
        await waitOrFail(
            () =>
                !serviceSettings().hide_identity &&
                findActor(
                    contentOf(root.get_children()[1]),
                    actor => actor.accessible_name === 'Connection details for Claude',
                ),
            'Provider connection details did not restore after privacy mode',
        );
        clickButton(
            required(
                findActor(
                    contentOf(root.get_children()[1]),
                    actor => actor.accessible_name === 'Connection details for Claude',
                ),
            ),
        );
        if (
            required(
                findActor(
                    contentOf(root.get_children()[1]),
                    actor =>
                        actor.has_style_class_name?.('tmb-provider-setting-details') &&
                        containsText(actor, 'Authentication · Demo data'),
                ),
            ).visible
        )
            throw new Error('Claude connection details did not collapse');
        const providerPage = contentOf(root.get_children()[1]);
        clickButton(required(findActor(providerPage, actor => actor.accessible_name === 'Show all providers')));
        await waitOrFail(() => serviceSettings().show_all_providers, 'Provider setup switch did not open');
        clickButton(required(findActor(providerPage, actor => actor.accessible_name === 'Enable Claude')));
        await waitOrFail(
            () =>
                !readProviderSettings('claude').enabled &&
                checkMark(findActor(providerPage, actor => actor.accessible_name === 'Enable Claude')) === '☐',
            'Disabling Claude did not update the provider control',
        );
        clickButton(required(findActor(providerPage, actor => actor.accessible_name === 'Enable Claude')));
        await waitOrFail(
            () =>
                readProviderSettings('claude').enabled &&
                checkMark(findActor(providerPage, actor => actor.accessible_name === 'Enable Claude')) === '☑',
            'Re-enabling Claude did not restore its provider control',
        );
        clickButton(required(findActor(providerPage, actor => actor.accessible_name === 'Show all providers')));
        await waitOrFail(() => !serviceSettings().show_all_providers, 'Provider setup switch did not close');
        clickButton(
            required(findActor(providerPage, actor => actor.accessible_name === 'Connection details for Claude')),
        );
        clickButton(required(findActor(providerPage, actor => actor.accessible_name === 'Move Claude down')));
        await waitOrFail(
            () =>
                serviceSettings().provider_order[0] === 'codex' &&
                findActor(providerPage, actor => actor.accessible_name === 'Move Claude up')?.reactive,
            'Moving Claude down did not reorder providers',
        );
        clickButton(required(findActor(providerPage, actor => actor.accessible_name === 'Move Claude up')));
        await waitOrFail(
            () => serviceSettings().provider_order[0] === 'claude',
            'Moving Claude up did not restore its order',
        );
        const interval = readProviderSettings('claude').intervalMinutes;
        await setSetting('interval_minutes', {...serviceSettings().interval_minutes, claude: 30});
        await waitOrFail(
            () =>
                findActor(
                    providerPage,
                    actor =>
                        actor.has_style_class_name?.('tmb-setting-row') &&
                        actor.get_children()[0]?.text === 'Claude' &&
                        containsText(actor, 'Every 30 min'),
                ),
            'Claude interval did not reach the provider controls',
        );
        const claudeRow = () =>
            findActor(
                providerPage,
                actor => actor.has_style_class_name?.('tmb-setting-row') && actor.get_children()[0]?.text === 'Claude',
            );
        if (
            required(findActor(claudeRow(), actor => actor.accessible_name === 'Increase Claude refresh interval'))
                .reactive
        )
            throw new Error('Claude interval can exceed 30 minutes');
        clickButton(
            required(findActor(claudeRow(), actor => actor.accessible_name === 'Decrease Claude refresh interval')),
        );
        await waitOrFail(
            () => readProviderSettings('claude').intervalMinutes === 29 && containsText(claudeRow(), 'Every 29 min'),
            'Claude interval did not decrease to 29 minutes',
        );
        await setSetting('interval_minutes', {...serviceSettings().interval_minutes, claude: 2});
        await waitOrFail(
            () => containsText(claudeRow(), 'Every 2 min'),
            'Claude minimum interval did not reach the provider controls',
        );
        if (
            required(findActor(claudeRow(), actor => actor.accessible_name === 'Decrease Claude refresh interval'))
                .reactive
        )
            throw new Error('Claude interval can fall below two minutes');
        clickButton(
            required(findActor(claudeRow(), actor => actor.accessible_name === 'Increase Claude refresh interval')),
        );
        await waitOrFail(
            () => readProviderSettings('claude').intervalMinutes === 3,
            'Claude interval did not increase to three minutes',
        );
        await setSetting('interval_minutes', {...serviceSettings().interval_minutes, claude: interval});
        await changedAfter(() => {
            clickButton(required(findActor(providerPage, actor => actor.accessible_name === 'Check Claude again')));
        });
    }
    adjustment.set_value(adjustment.upper - adjustment.page_size);
    await afterPaint(() => {});
    await capture('settings-bottom');
    const logPage = contentOf(root.get_children()[1]);
    await waitOrFail(() => containsText(logPage, 'Service started'), 'Service log did not load in Settings');
    const logRow = findActor(
        logPage,
        actor => actor.has_style_class_name?.('tmb-log-row') && containsText(actor, 'Service started'),
    );
    if (!logRow || !containsText(logRow, 'app · Service started'))
        throw new Error('Service start entry is missing from the Log section');
    const logScroll = required(findActor(logPage, actor => actor.has_style_class_name?.('tmb-log-scroll')));
    const logRows = contentOf(logScroll).get_n_children();
    if (GLib.getenv('TMB_EXPECT_DEMO') === '1' && logScroll.height > Math.max(60, 28 * logRows + 16))
        throw new Error('Log entries reserve too much space in Settings');
    clickButton(required(findActor(logPage, actor => actor.accessible_name === 'Log level Debug')));
    if (
        findActor(
            logPage,
            actor => actor.has_style_class_name?.('tmb-log-row') && containsText(actor, 'Service started'),
        )
    )
        throw new Error('Log severity filter did not hide information entries');
    clickButton(required(findActor(logPage, actor => actor.accessible_name === 'Log level All')));
    const logSearch = required(findActor(logPage, actor => actor.accessible_name === 'Search log'));
    instance(logSearch, St.Entry).set_text('missing message');
    if (
        findActor(
            logPage,
            actor => actor.has_style_class_name?.('tmb-log-row') && containsText(actor, 'Service started'),
        )
    )
        throw new Error('Log search did not hide unmatched entries');
    instance(logSearch, St.Entry).set_text('Service started');
    if (
        !findActor(
            logPage,
            actor => actor.has_style_class_name?.('tmb-log-row') && containsText(actor, 'Service started'),
        )
    )
        throw new Error('Log search did not restore matching entries');
    const copiedLog = await copiedTextAfter(() => {
        clickButton(required(findActor(logPage, actor => buttonLabel(actor) === 'Copy')));
    });
    if (!copiedLog.includes('[info] Service started'))
        throw new Error('Copy log did not export the visible filtered entry');
    instance(logSearch, St.Entry).set_text(copiedLog.slice(1, 11));
    if (
        !findActor(
            logPage,
            actor => actor.has_style_class_name?.('tmb-log-row') && containsText(actor, 'Service started'),
        )
    )
        throw new Error('Log search did not match the entry date');
    instance(logSearch, St.Entry).set_text(' [INFO] SERVICE ');
    if (
        !findActor(
            logPage,
            actor => actor.has_style_class_name?.('tmb-log-row') && containsText(actor, 'Service started'),
        )
    )
        throw new Error('Log search did not match a trimmed severity and message');
    instance(logSearch, St.Entry).set_text('');
    const previewCell = findActor(logPage, actor => actor.has_style_class_name?.('tmb-preview-cell'));
    if (previewCell) clickButton(previewCell);
    clickButton(required(findActor(logPage, actor => buttonLabel(actor) === 'Show full log')));
    await waitOrFail(
        () => containsText(logPage, 'Show recent log') && containsText(logPage, 'Service started'),
        'Full log did not load retained entries',
    );
    const fullLogScroll = required(findActor(logPage, actor => actor.has_style_class_name?.('tmb-log-scroll')));
    await waitOrFail(
        () =>
            fullLogScroll.height >= 145 &&
            fullLogScroll.height <= 566 &&
            (GLib.getenv('TMB_EXPECT_LOCAL') === '1' ? fullLogScroll.height > 400 : fullLogScroll.height < 200),
        'The full log did not size to its contents and screen',
    );
    if (!findActor(logPage, actor => actor.text === 'FULL LOG') || containsText(logPage, 'ABOUT'))
        throw new Error('The full log did not open its own view');
    await waitOrFail(() => adjustment.value < 2, 'Full log controls opened outside the visible menu');
    await capture(GLib.getenv('TMB_EXPECT_LOCAL') === '1' ? 'settings-full-log-retained' : 'settings-full-log-sparse');
    if (GLib.getenv('TMB_EXPECT_LOCAL') === '1') {
        const list = required(findActor(logPage, actor => actor.has_style_class_name?.('tmb-log-list')));
        if (!containsText(logPage, 'Page 1 of 2') || list.get_n_children() > 200)
            throw new Error('Full log did not paginate retained entries');
        const fullSearch = required(findActor(logPage, actor => actor.accessible_name === 'Search log'));
        instance(fullSearch, St.Entry).set_text('Log fixture 0');
        if (!containsText(logPage, 'Page 1 of 1') || list.get_n_children() !== 1)
            throw new Error('Full log search did not filter retained entries');
        instance(fullSearch, St.Entry).set_text('');
        clickButton(required(findActor(logPage, actor => buttonLabel(actor) === 'Older')));
        if (
            !containsText(logPage, 'Page 2 of 2') ||
            !containsText(list, 'Log fixture 0') ||
            list.get_n_children() > 200
        )
            throw new Error('Older log page did not show the earliest retained entries');
        clickButton(required(findActor(logPage, actor => buttonLabel(actor) === 'Newer')));
        if (!containsText(logPage, 'Page 1 of 2')) throw new Error('Newer log page did not restore recent entries');
    }
    clickButton(required(findActor(logPage, actor => buttonLabel(actor) === 'Show recent log')));
    await waitOrFail(
        () => containsText(logPage, 'Show full log') && containsText(logPage, 'Service started'),
        'Recent log did not reload',
    );
    if (!findActor(logPage, actor => actor.text === 'ABOUT'))
        throw new Error('Returning from the full log did not restore Settings');
    const detail = required(findActor(logPage, actor => actor.accessible_name === 'Detailed logging'));
    clickButton(detail);
    await waitOrFail(
        () =>
            serviceSettings().detailed_logging &&
            checkMark(findActor(logPage, actor => actor.accessible_name === 'Detailed logging')) === '☑',
        'Detailed logging preference was not saved',
    );
    clickButton(required(findActor(logPage, actor => actor.accessible_name === 'Detailed logging')));
    await waitOrFail(
        () =>
            !serviceSettings().detailed_logging &&
            checkMark(findActor(logPage, actor => actor.accessible_name === 'Detailed logging')) === '☐',
        'Detailed logging preference could not be disabled',
    );
    clickButton(required(findActor(logPage, actor => buttonLabel(actor) === 'Clear…')));
    clickButton(required(findActor(logPage, actor => buttonLabel(actor) === 'Confirm clear log')));
    await waitOrFail(
        () =>
            !findActor(
                logPage,
                actor => actor.has_style_class_name?.('tmb-log-row') && containsText(actor, 'Service started'),
            ),
        'Clear log did not remove retained entries',
    );
    await new Promise<void>(resolve =>
        GLib.idle_add(GLib.PRIORITY_DEFAULT_IDLE, () => {
            resolve();
            return GLib.SOURCE_REMOVE;
        }),
    );
    if (adjustment.value < adjustment.upper - adjustment.page_size - 2)
        throw new Error('Clear log did not return to the Settings footer');
    if ([...serviceLogs(), ...serviceLogs(true)].some(entry => entry.message === 'Service started'))
        throw new Error('Clear log left service entries on disk or in memory');
    if (GLib.getenv('TMB_EXPECT_DEMO') === '1') {
        const refreshControl = () =>
            findActor(
                contentOf(root.get_children()[1]),
                actor => actor.accessible_name === 'Refresh expired Claude, Codex, Gemini and Antigravity tokens',
            );
        const refresh = refreshControl();
        if (!refresh || refresh.get_accessible().ref_state_set().contains_state(Atk.StateType.CHECKED))
            throw new Error('Token refresh is not opt-in');
        clickButton(refresh);
        await waitOrFail(
            () =>
                serviceSettings().allow_token_refresh &&
                refreshControl()?.get_accessible().ref_state_set().contains_state(Atk.StateType.CHECKED),
            'Token refresh consent was not saved',
        );
        await capture('settings-token-refresh');
        clickButton(required(refreshControl()));
        await waitOrFail(() => !serviceSettings().allow_token_refresh, 'Token refresh consent could not be withdrawn');
        const controlsPage = () => contentOf(root.get_children()[1]);
        const previousRetention = serviceSettings().retention_days;
        clickButton(
            required(findActor(controlsPage(), actor => actor.accessible_name === 'Increase history retention')),
        );
        await waitOrFail(
            () =>
                serviceSettings().retention_days === previousRetention + 1 &&
                findActor(controlsPage(), actor => actor.text === `${previousRetention + 1} days`),
            'Retention control did not save an arbitrary day',
        );
        clickButton(
            required(findActor(controlsPage(), actor => actor.accessible_name === 'Decrease history retention')),
        );
        await waitOrFail(
            () =>
                serviceSettings().retention_days === previousRetention &&
                findActor(controlsPage(), actor => actor.text === `${previousRetention} days`),
            'Retention control did not decrease',
        );
        await setSetting('retention_days', 7);
        await waitOrFail(
            () => findActor(controlsPage(), actor => actor.text === '7 days'),
            'Minimum retention did not reach Settings',
        );
        if (
            required(findActor(controlsPage(), actor => actor.accessible_name === 'Decrease history retention'))
                .reactive
        )
            throw new Error('Retention control is active below its minimum');
        await setSetting('retention_days', 365);
        await waitOrFail(
            () => findActor(controlsPage(), actor => actor.text === '365 days'),
            'Maximum retention did not reach Settings',
        );
        if (
            required(findActor(controlsPage(), actor => actor.accessible_name === 'Increase history retention'))
                .reactive
        )
            throw new Error('Retention control is active above its maximum');
        await setSetting('retention_days', previousRetention);
        await waitOrFail(
            () => findActor(controlsPage(), actor => actor.text === `${previousRetention} days`),
            'Restored retention did not reach Settings',
        );
        clickButton(
            required(findActor(controlsPage(), actor => actor.accessible_name === 'Increase analytics interval')),
        );
        await waitOrFail(
            () =>
                serviceSettings().analytics_interval_minutes === 20 &&
                findActor(controlsPage(), actor => actor.text === '20 min'),
            'Analytics interval control skipped a valid step',
        );
        clickButton(
            required(findActor(controlsPage(), actor => actor.accessible_name === 'Decrease analytics interval')),
        );
        await waitOrFail(
            () =>
                serviceSettings().analytics_interval_minutes === 15 &&
                findActor(controlsPage(), actor => actor.text === '15 min'),
            'Analytics interval control did not decrease',
        );
        await setSetting('analytics_interval_minutes', 5);
        await waitOrFail(
            () => findActor(controlsPage(), actor => actor.text === '5 min'),
            'Minimum analytics interval did not reach Settings',
        );
        if (
            required(findActor(controlsPage(), actor => actor.accessible_name === 'Decrease analytics interval'))
                .reactive
        )
            throw new Error('Analytics interval control is active below its minimum');
        await setSetting('analytics_interval_minutes', 120);
        await waitOrFail(
            () => findActor(controlsPage(), actor => actor.text === '120 min'),
            'Maximum analytics interval did not reach Settings',
        );
        if (
            required(findActor(controlsPage(), actor => actor.accessible_name === 'Increase analytics interval'))
                .reactive
        )
            throw new Error('Analytics interval control is active above its maximum');
        await setSetting('analytics_interval_minutes', 15);
        await waitOrFail(
            () => findActor(controlsPage(), actor => actor.text === '15 min'),
            'Restored analytics interval did not reach Settings',
        );
        if (required(findActor(controlsPage(), actor => actor.accessible_name === 'Increase pace workdays')).reactive)
            throw new Error('Pace workdays control is active above its maximum');
        clickButton(required(findActor(controlsPage(), actor => actor.accessible_name === 'Decrease pace workdays')));
        await waitOrFail(
            () => serviceSettings().workdays === 6 && findActor(controlsPage(), actor => actor.text === '6 / week'),
            'Pace workdays control did not decrease',
        );
        await setSetting('workdays', 4);
        await waitOrFail(
            () => findActor(controlsPage(), actor => actor.text === '4 / week'),
            'Minimum pace workdays did not reach Settings',
        );
        if (required(findActor(controlsPage(), actor => actor.accessible_name === 'Decrease pace workdays')).reactive)
            throw new Error('Pace workdays control is active below its minimum');
        clickButton(required(findActor(controlsPage(), actor => actor.accessible_name === 'Increase pace workdays')));
        await waitOrFail(
            () => serviceSettings().workdays === 5 && findActor(controlsPage(), actor => actor.text === '5 / week'),
            'Pace workdays control did not increase',
        );
        await setSetting('workdays', 7);
        clickButton(required(findActor(controlsPage(), actor => actor.accessible_name === 'Notify at 50%')));
        await waitOrFail(
            () =>
                serviceSettings().notify_thresholds.includes(50) &&
                checkMark(findActor(controlsPage(), actor => actor.accessible_name === 'Notify at 50%')) === '☑',
            'Notification threshold control did not add 50%',
        );
        clickButton(required(findActor(controlsPage(), actor => actor.accessible_name === 'Notify at 50%')));
        await waitOrFail(
            () => !serviceSettings().notify_thresholds.includes(50),
            'Notification threshold control did not remove 50%',
        );
        clickButton(required(findActor(controlsPage(), actor => actor.accessible_name === 'Sound')));
        await waitOrFail(
            () =>
                !serviceSettings().notify_sound &&
                checkMark(findActor(controlsPage(), actor => actor.accessible_name === 'Sound')) === '☐',
            'Notification sound control did not save',
        );
        clickButton(required(findActor(controlsPage(), actor => actor.accessible_name === 'Sound')));
        await waitOrFail(
            () =>
                serviceSettings().notify_sound &&
                checkMark(findActor(controlsPage(), actor => actor.accessible_name === 'Sound')) === '☑',
            'Notification sound control did not restore',
        );
        clickButton(required(findActor(controlsPage(), actor => actor.accessible_name === 'Notifications')));
        await waitOrFail(
            () =>
                !serviceSettings().notifications &&
                !findActor(controlsPage(), actor => actor.accessible_name === 'Sound') &&
                findActor(
                    controlsPage(),
                    actor =>
                        actor.has_style_class_name?.('tmb-setting-row') &&
                        actor.get_children()[0]?.text === '' &&
                        actor.get_children()[1]?.text === 'Off',
                ),
            'Disabling notifications did not hide event options',
        );
        clickButton(required(findActor(controlsPage(), actor => actor.accessible_name === 'Notifications')));
        await waitOrFail(
            () =>
                serviceSettings().notifications &&
                findActor(controlsPage(), actor => actor.accessible_name === 'Sound') &&
                findActor(
                    controlsPage(),
                    actor =>
                        actor.has_style_class_name?.('tmb-setting-row') &&
                        actor.get_children()[0]?.text === '' &&
                        actor.get_children()[1]?.text?.startsWith('On · '),
                ),
            'Re-enabling notifications did not restore event options',
        );
        const claudeControls = findActor(
            controlsPage(),
            actor => actor.has_style_class_name?.('tmb-setting-row') && actor.get_children()[0]?.text === 'Claude',
        );
        const previousInterval = readProviderSettings('claude').intervalMinutes;
        clickButton(
            required(findActor(claudeControls, actor => actor.accessible_name === 'Increase Claude refresh interval')),
        );
        await waitOrFail(
            () => readProviderSettings('claude').intervalMinutes === required(previousInterval) + 1,
            'Provider polling interval control did not save',
        );
        await setSetting('interval_minutes', {...serviceSettings().interval_minutes, claude: previousInterval});
        clickButton(required(findActor(controlsPage(), actor => actor.accessible_name === 'Show all providers')));
        await waitOrFail(() => serviceSettings().show_all_providers, 'Provider setup switch did not open');
        clickButton(required(findActor(controlsPage(), actor => actor.accessible_name === 'Enable Claude')));
        await waitOrFail(
            () =>
                readProviderSettings('claude').enabled === false &&
                checkMark(findActor(controlsPage(), actor => actor.accessible_name === 'Enable Claude')) === '☐',
            'Provider switch did not disable Claude',
        );
        clickButton(required(findActor(controlsPage(), actor => actor.accessible_name === 'Enable Claude')));
        await waitOrFail(
            () => readProviderSettings('claude').enabled === true,
            'Provider switch did not restore Claude',
        );
        clickButton(required(findActor(controlsPage(), actor => actor.accessible_name === 'Show all providers')));
        await waitOrFail(() => !serviceSettings().show_all_providers, 'Provider setup switch did not close');
        const clearHistory = required(findActor(controlsPage(), actor => buttonLabel(actor) === 'Clear history…'));
        clickButton(clearHistory);
        if (instance(clearHistory, St.Button).get_label() !== 'Confirm clear history')
            throw new Error('History clear confirmation is missing');
        clickButton(clearHistory);
        await waitOrFail(
            () => containsText(controlsPage(), 'History cleared'),
            'History clear action did not report completion',
        );
    }
}

function dragSource(actor: TestActor | null) {
    return required(required(actor)._delegate) as {getDragActor(): St.Label};
}

function dropTarget(actor: TestActor | null) {
    return required(required(actor)._delegate) as {
        handleDragOver(source: object | undefined): number;
        acceptDrop(source: object | undefined, actor: null, x: number, y: number, time: number): boolean;
    };
}
