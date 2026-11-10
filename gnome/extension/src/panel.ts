import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import St from 'gi://St';
import type {Monitor} from 'resource:///org/gnome/shell/ui/layout.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import type * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
import type {PresentedHistory} from './history-presentation.js';
import {controlMethods} from './panel/controls.js';
import {displayMethods} from './panel/display.js';
import {historyMethods} from './panel/history.js';
import {logMethods} from './panel/log.js';
import {BUS, type ErrorHandler, IFACE, PATH, type ServiceReplies, serviceMethods} from './panel/service.js';
import {settingsMethods} from './panel/settings.js';
import {box, button, label} from './panel/support.js';
import {usageMethods} from './panel/usage.js';
import type {LogEntry, ModelGroup, PanelState, SpendSummary, TimePresentation} from './service-state.js';
import {COPY} from './shared-copy.js';
import {Tooltip} from './tooltip.js';
import {SnapshotWidget} from './widget.js';

// The menu is a full PopupMenu because the button is built with dontCreateMenu false.
export interface Indicator extends PanelMenu.Button {
    menu: PopupMenu.PopupMenu;
}
export type PanelMonitor = Pick<Monitor, 'x' | 'width' | 'height'>;

export interface HistoryClick {
    id: string;
    time: number;
    x: number;
    y: number;
}

export class TokenPanel {
    _drainNotifications = serviceMethods._drainNotifications.bind(this);
    _exportHistory = serviceMethods._exportHistory.bind(this);
    _logDiagnostic = serviceMethods._logDiagnostic.bind(this);
    _syncPanelOpen = serviceMethods._syncPanelOpen.bind(this);
    _requestQuit = serviceMethods._requestQuit.bind(this);
    _load = serviceMethods._load.bind(this);
    _loadLogs = serviceMethods._loadLogs.bind(this);
    _discardLog = serviceMethods._discardLog.bind(this);
    _scrollToSettingsBottom = serviceMethods._scrollToSettingsBottom.bind(this);
    _loadModelGroups = serviceMethods._loadModelGroups.bind(this);
    _apply = serviceMethods._apply.bind(this);
    _set = serviceMethods._set.bind(this);
    _refresh = serviceMethods._refresh.bind(this);
    _loadCosts = serviceMethods._loadCosts.bind(this);
    _loadReviews = serviceMethods._loadReviews.bind(this);
    _panelCell = displayMethods._panelCell.bind(this);
    _miniBarCell = displayMethods._miniBarCell.bind(this);
    _render = displayMethods._render.bind(this);
    _panelMonitor = displayMethods._panelMonitor.bind(this);
    _updateHeader = displayMethods._updateHeader.bind(this);
    _menuWidth = displayMethods._menuWidth.bind(this);
    _alignMenu = displayMethods._alignMenu.bind(this);
    _renderPage = displayMethods._renderPage.bind(this);
    _selectTab = displayMethods._selectTab.bind(this);
    _renderUsage = usageMethods._renderUsage.bind(this);
    _providerIcon = usageMethods._providerIcon.bind(this);
    _keyValue = usageMethods._keyValue.bind(this);
    _renderQuotaRows = usageMethods._renderQuotaRows.bind(this);
    _renderUsageExtras = usageMethods._renderUsageExtras.bind(this);
    _loadHistory = historyMethods._loadHistory.bind(this);
    _renderHistory = historyMethods._renderHistory.bind(this);
    _isolateHistorySeries = historyMethods._isolateHistorySeries.bind(this);
    _updateHistoryVisibility = historyMethods._updateHistoryVisibility.bind(this);
    _pageHistory = historyMethods._pageHistory.bind(this);
    _stepHistoryReading = historyMethods._stepHistoryReading.bind(this);
    _renderHistoryContent = historyMethods._renderHistoryContent.bind(this);
    _section = controlMethods._section.bind(this);
    _settingRow = controlMethods._settingRow.bind(this);
    _segments = controlMethods._segments.bind(this);
    _check = controlMethods._check.bind(this);
    _reorderButton = controlMethods._reorderButton.bind(this);
    _settingNote = controlMethods._settingNote.bind(this);
    _stepper = controlMethods._stepper.bind(this);
    _entry = controlMethods._entry.bind(this);
    _moveProvider = controlMethods._moveProvider.bind(this);
    _moveModel = controlMethods._moveModel.bind(this);
    _dragHandle = controlMethods._dragHandle.bind(this);
    _reorderTarget = controlMethods._reorderTarget.bind(this);
    _moveProviderTo = controlMethods._moveProviderTo.bind(this);
    _moveModelTo = controlMethods._moveModelTo.bind(this);
    _reorder = controlMethods._reorder.bind(this);
    _openDocument = controlMethods._openDocument.bind(this);
    _showProviderSettings = controlMethods._showProviderSettings.bind(this);
    _scrollToSettingsTarget = controlMethods._scrollToSettingsTarget.bind(this);
    _showProviderSignIn = controlMethods._showProviderSignIn.bind(this);
    _startSignIn = controlMethods._startSignIn.bind(this);
    _renderSettings = settingsMethods._renderSettings.bind(this);
    _renderSettingsOverview = settingsMethods._renderSettingsOverview.bind(this);
    _renderSettingsModels = settingsMethods._renderSettingsModels.bind(this);
    _renderSettingsProviders = settingsMethods._renderSettingsProviders.bind(this);
    _renderSettingsData = settingsMethods._renderSettingsData.bind(this);
    _renderLog = logMethods._renderLog.bind(this);
    _historyStepRequest = 0;
    _historyStepDirections: number[] | null = null;
    _historyStepBase: number | null = null;
    uuid: string;
    path: string;
    _repositoryURL: string;
    _onQuit: () => void;
    _enabled: boolean;
    _tab: string;
    _state: PanelState | null;
    _providerIds: string[];
    _usageOrder: string[];
    _providerNames: Record<string, string>;
    _preferencesLoaded: boolean;
    _firstRunOpened: boolean;
    _proxy: Gio.DBusProxy | null;
    _history: PresentedHistory | null;
    _historyError: string | null;
    _chart: St.DrawingArea | null;
    _costs: Record<string, SpendSummary> | null;
    _costError: string | null;
    _costAt: number;
    _costDemo: boolean | null;
    _reviews: string | null;
    _reviewAt: number;
    _days: number;
    _metric: string;
    _metricOpen: boolean;
    _historyRange: [number, number] | null;
    _customFrom: string;
    _customTo: string;
    _customOpen: boolean;
    _utc: boolean;
    _rollup: string;
    _stacked: boolean;
    _shown: Set<string>;
    _hidden: Set<string>;
    _historyClick: HistoryClick | null;
    _selectedHistoryAt: number | null;
    _historyDetailsOpen: boolean;
    _expandedDetails: Set<string>;
    _providerDetails: Set<string>;
    _providerRows: Map<string, St.BoxLayout>;
    _quotaRows: Map<string, St.Widget>;
    _settingFocus: Map<string, St.Widget>;
    _historyFocus: Map<string, St.Widget>;
    _labelEntries: Map<string, St.Entry>;
    _labelConflictIcons: Map<string, St.Widget>;
    _labelDrafts: Map<string, string>;
    _suppressedLabelSaves: WeakSet<St.Entry>;
    _modelsOpen: boolean;
    _modelFilter: string;
    _revealedModelKey: string | null;
    _modelFilterEntry: St.Entry | null;
    _modelGroups: ModelGroup[] | null;
    _modelGroupsState: PanelState | null;
    _modelGroupsQuery: string | null;
    _modelGroupsRevealed: string | null;
    _modelGroupsRequest: number;
    _modelGroupsError: string | null;
    _logEntries: LogEntry[] | null;
    _logLoading: boolean;
    _logDirty: boolean;
    _logRequest: number;
    _logLevel: string;
    _logSearch: string;
    _fullLog: boolean;
    _logFocus: boolean;
    _logPage: number;
    _logScrollSource: number;
    _geometrySignature: string | null;
    _exportCancel: (() => void) | null;
    _clock: number;
    _ageClock: number;
    _providerStatusLabels: {actor: St.Label; id: string}[];
    _monitorId: number;
    _scrollSource: number;
    _scrollbarHide: number;
    _quitPending: boolean;
    _historyRequest: number;
    _renderedTab: string | null;
    _pageTab: string | null;
    _panelSignature: string | null;
    _layoutWidth: number;
    _menuSourceAlignment: number | null;
    _light: boolean;
    _themeSignals: number[];
    _error?: string | null;
    _notice?: string | null;
    _timePresentation?: TimePresentation | null;
    _cancel!: Gio.Cancellable;
    _tooltip!: Tooltip;
    _indicator!: Indicator;
    _widget!: SnapshotWidget | null;
    _panelIcon!: St.Widget;
    _panelGlyph!: St.Icon;
    _panelDot!: St.Widget;
    _panelCells!: St.BoxLayout;
    _root!: St.BoxLayout;
    _monitorWidth!: number;
    _compact!: boolean;
    _headerInfo!: St.BoxLayout;
    _headerGlyph!: St.Icon;
    _headerDot!: St.Widget;
    _headerUpdated!: St.Label;
    _tabs!: St.BoxLayout;
    _headerBalance!: St.BoxLayout;
    _headerActivity!: St.Label;
    _scroll!: St.ScrollView;
    _page!: St.BoxLayout;
    _footer!: St.BoxLayout;
    _footerPrimary!: St.BoxLayout;
    _footerPrimarySpacer!: St.Widget;
    _footerSecondary!: St.BoxLayout;
    _footerSpacer!: St.Widget;
    _footerQuit!: St.Button;
    _openId!: number;
    _allocationId!: number;
    _themeSettings!: St.Settings;
    _signalId!: number;
    _ownerId!: number;
    _settingsSection!: St.BoxLayout;

    constructor(uuid: string, path: string, repositoryURL: string, onQuit: () => void) {
        this.uuid = uuid;
        this.path = path;
        this._repositoryURL = repositoryURL;
        this._onQuit = onQuit;
        this._enabled = false;
        this._tab = 'Usage';
        this._state = null;
        this._providerIds = [];
        this._usageOrder = [];
        this._providerNames = {};
        this._preferencesLoaded = false;
        this._firstRunOpened = false;
        this._proxy = null;
        this._history = null;
        this._historyError = null;
        this._chart = null;
        this._costs = null;
        this._costError = null;
        this._costAt = 0;
        this._costDemo = null;
        this._reviews = null;
        this._reviewAt = 0;
        this._days = 1;
        this._metric = 'usage';
        this._metricOpen = false;
        this._historyRange = null;
        this._customFrom = '';
        this._customTo = '';
        this._customOpen = false;
        this._utc = false;
        this._rollup = 'minute';
        this._stacked = false;
        this._shown = new Set();
        this._hidden = new Set();
        this._historyClick = null;
        this._selectedHistoryAt = null;
        this._historyDetailsOpen = false;
        this._expandedDetails = new Set();
        this._providerDetails = new Set();
        this._providerRows = new Map();
        this._quotaRows = new Map();
        this._settingFocus = new Map();
        this._historyFocus = new Map();
        this._labelEntries = new Map();
        this._labelConflictIcons = new Map();
        this._labelDrafts = new Map();
        this._suppressedLabelSaves = new WeakSet();
        this._modelsOpen = true;
        this._modelFilter = '';
        this._revealedModelKey = null;
        this._modelFilterEntry = null;
        this._modelGroups = null;
        this._modelGroupsState = null;
        this._modelGroupsQuery = null;
        this._modelGroupsRevealed = null;
        this._modelGroupsRequest = 0;
        this._modelGroupsError = null;
        this._logEntries = null;
        this._logLoading = false;
        this._logDirty = false;
        this._logRequest = 0;
        this._logLevel = 'All';
        this._logSearch = '';
        this._fullLog = false;
        this._logFocus = false;
        this._logPage = 0;
        this._logScrollSource = 0;
        this._geometrySignature = null;
        this._exportCancel = null;
        this._clock = 0;
        this._ageClock = 0;
        this._providerStatusLabels = [];
        this._monitorId = 0;
        this._scrollSource = 0;
        this._scrollbarHide = 0;
        this._quitPending = false;
        this._historyRequest = 0;
        this._renderedTab = null;
        this._pageTab = null;
        this._panelSignature = null;
        this._layoutWidth = 0;
        this._menuSourceAlignment = null;
        this._light = false;
        this._themeSignals = [];
    }

    _call<Method extends keyof ServiceReplies>(
        method: Method,
        signature: string,
        values: unknown[],
        callback: (value: ServiceReplies[Method]) => void,
        onError: ErrorHandler | null = null,
    ): void {
        const proxy = this._proxy;
        if (!proxy) return;
        const parameters = new GLib.Variant(signature, values);
        proxy.call(method, parameters, Gio.DBusCallFlags.NONE, 12000, this._cancel, (_source, result) => {
            if (!this._enabled) return;
            try {
                callback(proxy.call_finish(result).deep_unpack<[ServiceReplies[Method]]>()[0]);
            } catch (error) {
                if (onError?.(error as Error) === false) return;
                this._error = (error as Error).message;
                this._render();
            }
        });
    }

    enable(): void {
        this._enabled = true;
        this._cancel = new Gio.Cancellable();
        this._tooltip = new Tooltip();
        this._indicator = new PanelMenu.Button(0.5, COPY.appName, false) as Indicator;
        this._indicator.add_style_class_name('tmb-panel-button');
        const panel = box(false, 'panel-status-menu-box');
        const appIcon = new Gio.FileIcon({
            file: Gio.File.new_for_path(GLib.build_filenamev([this.path, 'icons', 'quotaline-symbolic.svg'])),
        });
        const markedIcon = (size: number, styleClass: string) => {
            const holder = new St.Widget({layout_manager: new Clutter.FixedLayout(), width: size, height: size});
            const glyph = new St.Icon({gicon: appIcon, icon_size: size, style_class: styleClass});
            const diameter = Math.round(size * 0.3);
            const dot = new St.Widget({
                style_class: 'tmb-app-icon-dot',
                width: diameter,
                height: diameter,
                x: size - diameter,
                y: 0,
                visible: false,
            });
            holder.add_child(glyph);
            holder.add_child(dot);
            return {holder, glyph, dot};
        };
        const panelMark = markedIcon(16, 'system-status-icon');
        this._panelIcon = panelMark.holder;
        this._panelGlyph = panelMark.glyph;
        this._panelDot = panelMark.dot;
        panel.add_child(this._panelIcon);
        this._panelCells = box(false, 'tmb-panel-cells');
        this._panelCells.add_child(label('…', 'tmb-panel-label'));
        panel.add_child(this._panelCells);
        this._indicator.add_child(panel);
        this._tooltip.attach(this._indicator, COPY.appName, () => this._state?.status_tooltip ?? COPY.noUsageYet);
        this._root = box(true, 'tmb-root');
        this._root.reactive = true;
        const shortcuts = new Clutter.KeyController();
        shortcuts.connect('key-press', controller => {
            const symbol = controller.get_key()[1];
            const [, pressed, latched, locked] = controller.get_state();
            const modifiers =
                (pressed | latched | locked) &
                (Clutter.ModifierType.CONTROL_MASK |
                    Clutter.ModifierType.MOD1_MASK |
                    Clutter.ModifierType.SUPER_MASK |
                    Clutter.ModifierType.SHIFT_MASK);
            if (
                !(modifiers & Clutter.ModifierType.CONTROL_MASK) ||
                modifiers & (Clutter.ModifierType.MOD1_MASK | Clutter.ModifierType.SUPER_MASK)
            )
                return Clutter.EVENT_PROPAGATE;
            if (symbol === Clutter.KEY_r || symbol === Clutter.KEY_R) {
                this._refresh();
                return Clutter.EVENT_STOP;
            }
            if (symbol === Clutter.KEY_f || symbol === Clutter.KEY_F) {
                this._selectTab('Settings');
                this._modelFilterEntry?.grab_key_focus();
                return Clutter.EVENT_STOP;
            }
            if (symbol === Clutter.KEY_q || symbol === Clutter.KEY_Q) {
                this._requestQuit();
                return Clutter.EVENT_STOP;
            }
            return Clutter.EVENT_PROPAGATE;
        });
        this._root.add_action_full('tmb-shortcuts', Clutter.EventPhase.CAPTURE, shortcuts);
        const monitor = this._panelMonitor();
        this._monitorWidth = monitor.width;
        this._root.width = Math.min(880, Math.max(280, this._monitorWidth - 32));
        this._compact = this._root.width < 790;
        const header = box(false, 'tmb-header');
        this._headerInfo = box(false, 'tmb-header-info');
        const headerMark = markedIcon(21, 'tmb-usage-summary-icon');
        this._headerGlyph = headerMark.glyph;
        this._headerDot = headerMark.dot;
        this._headerInfo.add_child(headerMark.holder);
        const heading = box(true, 'tmb-header-heading');
        heading.add_child(label(COPY.appName, 'tmb-usage-summary-title'));
        this._headerUpdated = label('Updated never', 'tmb-header-updated');
        heading.add_child(this._headerUpdated);
        this._headerInfo.add_child(heading);
        const leftGap = box();
        leftGap.x_expand = true;
        leftGap.add_child(this._headerInfo);
        header.add_child(leftGap);
        this._tabs = box(false, 'tmb-tabs');
        header.add_child(this._tabs);
        const rightGap = box();
        rightGap.x_expand = true;
        this._headerBalance = box();
        this._headerBalance.width = 165;
        this._headerActivity = label('', 'tmb-header-activity');
        this._headerBalance.add_child(new St.Widget({x_expand: true}));
        this._headerActivity.x_align = Clutter.ActorAlign.END;
        this._headerBalance.add_child(this._headerActivity);
        rightGap.add_child(new St.Widget({x_expand: true}));
        rightGap.add_child(this._headerBalance);
        header.add_child(rightGap);
        this._root.add_child(header);
        this._scroll = new St.ScrollView({style_class: 'tmb-scroll', overlay_scrollbars: true});
        this._scroll.style = `max-height: ${Math.max(180, monitor.height - 160)}px;`;
        this._page = box(true, 'tmb-page');
        this._scroll.set_child(this._page);
        const scrollbars = this._scroll.get_children().filter(actor => actor instanceof St.ScrollBar);
        for (const scrollbar of scrollbars) scrollbar.opacity = 0;
        this._scroll.get_vadjustment().connect('notify::value', () => {
            for (const scrollbar of scrollbars) scrollbar.opacity = 255;
            if (this._scrollbarHide) GLib.Source.remove(this._scrollbarHide);
            this._scrollbarHide = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 1200, () => {
                for (const scrollbar of scrollbars) scrollbar.opacity = 0;
                this._scrollbarHide = 0;
                return GLib.SOURCE_REMOVE;
            });
        });
        this._root.add_child(this._scroll);
        this._footer = box(false, 'tmb-footer');
        this._footerPrimary = box(false, 'tmb-footer-group');
        const refresh = button(
            'Refresh',
            () => {
                this._refresh();
            },
            'tmb-control tmb-footer-action',
        );
        const refreshContent = box(false, 'tmb-footer-action-content');
        refreshContent.add_child(new St.Icon({icon_name: 'view-refresh-symbolic', icon_size: 13}));
        refreshContent.add_child(label('Refresh'));
        refresh.set_child(refreshContent);
        this._tooltip.attach(
            refresh,
            'Refresh',
            'Fetches current usage from each active provider. History analytics keep their own schedule.',
        );
        this._footerPrimary.add_child(refresh);
        const offlineIssue =
            `${this._repositoryURL}/issues/new?title=Issue%20report&body=` +
            '%60%60%60%0AQuotaline%20service%20unavailable.%0A%60%60%60';
        const report = button(
            'Report issue',
            () => {
                if (!this._proxy) {
                    Gio.AppInfo.launch_default_for_uri(offlineIssue, null);
                    return;
                }
                this._call(
                    'GetIssueUrl',
                    '()',
                    [],
                    value => Gio.AppInfo.launch_default_for_uri(value, null),
                    () => {
                        Gio.AppInfo.launch_default_for_uri(offlineIssue, null);
                        return false;
                    },
                );
            },
            'tmb-control tmb-footer-action',
        );
        const reportContent = box(false, 'tmb-footer-action-content');
        reportContent.add_child(
            new St.Icon({
                gicon: new Gio.FileIcon({
                    file: Gio.File.new_for_path(
                        GLib.build_filenamev([this.path, 'icons', 'report-issue-symbolic.svg']),
                    ),
                }),
                icon_size: 13,
            }),
        );
        reportContent.add_child(label('Report issue'));
        report.set_child(reportContent);
        this._tooltip.attach(report, 'Report issue', 'Opens a draft issue with diagnostics when available.');
        this._footerPrimary.add_child(report);
        this._footerPrimarySpacer = new St.Widget({x_expand: true});
        this._footerPrimary.add_child(this._footerPrimarySpacer);
        this._footer.add_child(this._footerPrimary);
        this._footerSecondary = box(false, 'tmb-footer-group');
        const footerLinks = box(false, 'tmb-footer-group');
        const diagnostics = button(
            'Copy diagnostics',
            () => {
                this._call('GetDiagnostics', '()', [], value => {
                    St.Clipboard.get_default().set_text(St.ClipboardType.CLIPBOARD, value);
                });
            },
            'tmb-control tmb-footer-action',
        );
        this._tooltip.attach(
            diagnostics,
            'Copy diagnostics',
            'Copies provider status and recent log entries. Credentials and tokens are excluded.',
        );
        footerLinks.add_child(diagnostics);
        const source = button(
            'Source',
            () => Gio.AppInfo.launch_default_for_uri(this._repositoryURL, null),
            'tmb-control tmb-footer-action',
        );
        this._tooltip.attach(source, 'Source', 'Opens the Quotaline source repository in your browser.');
        footerLinks.add_child(source);
        this._footerSecondary.add_child(footerLinks);
        this._footer.add_child(this._footerSecondary);
        this._footerSpacer = new St.Widget({x_expand: true});
        this._footer.add_child(this._footerSpacer);
        this._footerQuit = button(
            'Quit',
            () => {
                this._requestQuit();
            },
            'tmb-control tmb-footer-action',
        );
        const quitContent = box(false, 'tmb-footer-action-content');
        quitContent.add_child(new St.Icon({icon_name: 'system-shutdown-symbolic', icon_size: 13}));
        quitContent.add_child(label('Quit'));
        this._footerQuit.set_child(quitContent);
        this._footerQuit.accessible_name = 'Quit';
        this._footerQuit.x_align = Clutter.ActorAlign.END;
        this._tooltip.attach(
            this._footerQuit,
            'Quit',
            'Stops polling and hides the panel until next login. Settings and history remain. Toggle the extension off and on in Extensions to reopen sooner.',
        );
        this._footer.add_child(this._footerQuit);
        this._root.add_child(this._footer);
        this._indicator.menu.box.add_child(this._root);
        this._openId = this._indicator.menu.connect('open-state-changed', (_menu, open) => {
            this._syncPanelOpen(open);
            if (open) {
                this._tooltip.hide();
                this._renderPage();
                this._load();
                if (this._tab === 'Usage') this._loadCosts();
                if (this._tab === 'History') this._loadHistory();
                this._clock = GLib.timeout_add_seconds(GLib.PRIORITY_DEFAULT, 60, () => {
                    if (this._tab === 'Usage') this._load();
                    else if (this._tab === 'History' && !this._historyRange) this._loadHistory();
                    return GLib.SOURCE_CONTINUE;
                });
            } else {
                GLib.Source.remove(this._clock);
                this._clock = 0;
                if (this._ageClock) {
                    GLib.Source.remove(this._ageClock);
                    this._ageClock = 0;
                }
            }
            if (!open) {
                this._notice = null;
                this._discardLog();
                this._tooltip.hide();
            }
        });
        Main.panel.addToStatusArea(this.uuid, this._indicator);
        this._allocationId = this._indicator.connect('notify::allocation', () => {
            if (!this._indicator.menu.isOpen) return;
            const monitor = this._panelMonitor();
            if (Math.abs(this._root.width - this._menuWidth(monitor)) >= 1) this._renderPage();
            else this._alignMenu(monitor, this._root.width);
        });
        this._monitorId = Main.layoutManager.connect('monitors-changed', () => {
            this._monitorWidth = this._panelMonitor().width;
            this._panelSignature = null;
            this._render();
        });
        this._widget = new SnapshotWidget(
            () => {
                Main.panel.statusArea.quickSettings?.menu.close();
                this._indicator.menu.open(false);
            },
            appIcon,
            () => {
                this._load();
            },
        );
        this._themeSettings = St.Settings.get();
        const themeChanged = () => {
            this._light =
                this._themeSettings.color_scheme === St.SystemColorScheme.PREFER_LIGHT &&
                !this._themeSettings.high_contrast;
            if (this._light) {
                this._root.add_style_class_name('tmb-light');
                this._indicator.add_style_class_name('tmb-light');
            } else {
                this._root.remove_style_class_name('tmb-light');
                this._indicator.remove_style_class_name('tmb-light');
            }
            this._panelSignature = null;
            this._render();
        };
        this._themeSignals = (['notify::color-scheme', 'notify::high-contrast'] as const).map(signal =>
            this._themeSettings.connect(signal, themeChanged),
        );
        themeChanged();
        Gio.DBusProxy.new_for_bus(
            Gio.BusType.SESSION,
            Gio.DBusProxyFlags.NONE,
            null,
            BUS,
            PATH,
            IFACE,
            this._cancel,
            (_source, result) => {
                if (!this._enabled) return;
                try {
                    this._proxy = Gio.DBusProxy.new_for_bus_finish(result);
                    this._signalId = this._proxy.connect('g-signal', (_proxy, _sender, name, parameters) => {
                        if (name === 'Changed') {
                            this._apply(parameters.deep_unpack<[string]>()[0]);
                            if (this._tab === 'Settings' && this._indicator.menu.isOpen) this._loadLogs();
                        } else if (name === 'LogChanged' && this._tab === 'Settings' && this._indicator.menu.isOpen)
                            this._loadLogs();
                        else if (name === 'SignInRequested')
                            this._showProviderSignIn(parameters.deep_unpack<[string]>()[0]);
                        else if (name === 'NotificationPlanned') this._drainNotifications();
                    });
                    this._ownerId = this._proxy.connect('notify::g-name-owner', proxy => {
                        if (proxy.g_name_owner) {
                            this._drainNotifications();
                            this._load();
                        } else {
                            this._state = null;
                            this._timePresentation = null;
                            this._preferencesLoaded = false;
                            this._modelGroups = null;
                            this._modelGroupsState = null;
                            this._modelGroupsRequest++;
                            this._discardLog();
                            this._render();
                        }
                    });
                    this._drainNotifications();
                    this._syncPanelOpen(this._indicator.menu.isOpen);
                    this._load();
                } catch (error) {
                    this._error = (error as Error).message;
                    this._render();
                }
            },
        );
    }

    disable(): void {
        this._syncPanelOpen(false);
        this._enabled = false;
        Main.layoutManager.disconnectObject(this._indicator);
        Main.layoutManager.disconnect(this._monitorId);
        this._monitorId = 0;
        for (const signal of this._themeSignals) this._themeSettings.disconnect(signal);
        this._themeSignals = [];
        this._exportCancel?.();
        this._cancel.cancel();
        this._discardLog();
        if (this._clock) {
            GLib.Source.remove(this._clock);
            this._clock = 0;
        }
        if (this._ageClock) {
            GLib.Source.remove(this._ageClock);
            this._ageClock = 0;
        }
        if (this._scrollSource) {
            global.stage.disconnect(this._scrollSource);
            this._scrollSource = 0;
        }
        if (this._scrollbarHide) {
            GLib.Source.remove(this._scrollbarHide);
            this._scrollbarHide = 0;
        }
        if (this._proxy) {
            if (this._signalId) this._proxy.disconnect(this._signalId);
            if (this._ownerId) this._proxy.disconnect(this._ownerId);
        }
        this._indicator.disconnect(this._allocationId);
        this._indicator.menu.disconnect(this._openId);
        this._indicator.destroy();
        this._tooltip.destroy();
        this._widget?.destroy();
        this._widget = null;
        this._proxy = null;
        this._menuSourceAlignment = null;
        this._state = null;
        this._history = null;
        this._costs = null;
        this._costError = null;
        this._reviews = null;
        this._modelGroups = null;
        this._modelGroupsRequest++;
        this._providerRows.clear();
        this._quotaRows.clear();
        this._settingFocus.clear();
        this._historyFocus.clear();
        this._labelEntries.clear();
        this._labelConflictIcons.clear();
        this._labelDrafts.clear();
    }
}
