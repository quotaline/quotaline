import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {closeInstantly, openInstantly} from '../menu-animation.js';
import type {TokenPanel} from '../panel.js';
import {quotaPresentation} from '../quota-presentation.js';
import type {LogEntry, ModelGroup, PanelState, ReviewCounts, SpendReport} from '../service-state.js';
import {statusCells} from '../status-cells.js';
import {emptyUsageTitle, miniBarGroups, statusIconTone, statusTooltip, statusVariants} from '../status-presentation.js';
import {timePresentation} from '../time-presentation.js';
import {reviewCountText} from '../usage-copy.js';
import {snapshotRows} from '../widget-data.js';
import {chooseExportPath} from './support.js';

export type ErrorHandler = (error: Error) => boolean | undefined;

export interface ServiceReplies {
    DeliverNotifications: undefined;
    ExportAllHistory: boolean;
    ExportMetricHistory: boolean;
    RecordShellDiagnostic: boolean;
    GetState: string;
    GetIssueUrl: string;
    GetDiagnostics: string;
    GetLogs: string;
    GetModelGroups: string;
    GetSpendSummary: string;
    GetReviewSummary: string;
    GetHistoryView: string;
    SetSetting: string;
    ResetSettings: string;
    StepProviderInterval: string;
    ToggleProviderModels: string;
    ToggleModel: string;
    PreviewShortLabel: string;
    ToggleNotificationThreshold: string;
    UpdateHistoryVisibility: string;
    SwapProvider: string;
    Reorder: string;
    StartSignIn: boolean;
    StepHistoryReading: number;
    Refresh: boolean;
    ClearHistory: boolean;
    ClearLogs: boolean;
}

export const BUS = 'tech.bernat.Quotaline.Gnome';
export const PATH = '/tech/bernat/Quotaline/Gnome';
export const IFACE = 'tech.bernat.Quotaline.Gnome';

export const serviceMethods = {
    _drainNotifications(this: TokenPanel): void {
        this._call(
            'DeliverNotifications',
            '()',
            [],
            () => {},
            () => false,
        );
    },
    async _exportHistory(
        this: TokenPanel,
        method: 'ExportAllHistory' | 'ExportMetricHistory',
        signature: string,
        values: unknown[],
        filename: string,
    ): Promise<void> {
        this._exportCancel?.();
        // GJS rejects with GLib.Error for D-Bus failures, which is not an instance of Error.
        const choice = await chooseExportPath(cancel => (this._exportCancel = cancel), filename).catch(
            (error: unknown) => ({message: (error as {message: string}).message}),
        );
        if (!this._enabled || !choice) return;
        if (typeof choice !== 'string') {
            this._notice = choice.message;
            this._renderPage();
            return;
        }
        this._call(method, signature, [...values, choice], () => {
            this._notice = `Exported ${choice}`;
            this._renderPage();
        });
    },

    _logDiagnostic(this: TokenPanel, category: string, message: string): void {
        if (!this._state?.settings.detailed_logging) return;
        this._call(
            'RecordShellDiagnostic',
            '(ss)',
            [category, message],
            () => {},
            () => false,
        );
    },

    _syncPanelOpen(this: TokenPanel, open: boolean): void {
        const proxy = this._proxy;
        proxy?.call(
            'SetPanelOpen',
            new GLib.Variant('(b)', [open]),
            Gio.DBusCallFlags.NONE,
            12000,
            null,
            (_source, result) => {
                try {
                    proxy.call_finish(result);
                } catch (error) {
                    if (this._enabled) {
                        this._error = (error as Error).message;
                        this._render();
                    }
                }
            },
        );
    },

    _requestQuit(this: TokenPanel): void {
        closeInstantly(this._indicator.menu);
        if (this._quitPending) return;
        this._quitPending = true;
        global.stage.connectObject(
            'after-paint',
            () => {
                this._onQuit();
                Gio.DBus.session.call(
                    BUS,
                    PATH,
                    IFACE,
                    'Quit',
                    new GLib.Variant('()', []),
                    null,
                    Gio.DBusCallFlags.NONE,
                    -1,
                    null,
                    null,
                );
            },
            this._indicator,
        );
        global.stage.queue_redraw();
    },

    _load(this: TokenPanel): void {
        this._call('GetState', '()', [], raw => {
            this._apply(raw);
        });
    },

    _loadLogs(this: TokenPanel): void {
        if (this._logLoading) {
            this._logDirty = true;
            return;
        }
        this._logLoading = true;
        const request = ++this._logRequest;
        this._call(
            'GetLogs',
            '(b)',
            [this._fullLog],
            raw => {
                if (request !== this._logRequest) return;
                this._logLoading = false;
                this._logEntries = JSON.parse(raw) as LogEntry[];
                this._renderPage();
                if (this._logFocus) {
                    this._logFocus = false;
                    this._scrollToSettingsBottom();
                }
                if (this._logDirty) {
                    this._logDirty = false;
                    this._loadLogs();
                }
            },
            (): boolean | undefined => {
                if (request !== this._logRequest) return false;
                this._logLoading = false;
                this._logDirty = false;
                this._logEntries = [];
                return undefined;
            },
        );
    },

    _discardLog(this: TokenPanel): void {
        this._logEntries = null;
        this._logLoading = false;
        this._logDirty = false;
        this._logRequest++;
        this._logPage = 0;
        this._fullLog = false;
        this._logFocus = false;
        if (this._logScrollSource) {
            GLib.Source.remove(this._logScrollSource);
            this._logScrollSource = 0;
        }
    },

    _scrollToSettingsBottom(this: TokenPanel): void {
        if (this._logScrollSource) GLib.Source.remove(this._logScrollSource);
        this._logScrollSource = GLib.idle_add(GLib.PRIORITY_DEFAULT_IDLE, () => {
            this._logScrollSource = 0;
            const adjustment = this._scroll.get_vadjustment();
            adjustment.set_value(adjustment.upper - adjustment.page_size);
            return GLib.SOURCE_REMOVE;
        });
    },

    _loadModelGroups(this: TokenPanel, force = false): void {
        if (
            !force &&
            this._modelGroupsState === this._state &&
            this._modelGroupsQuery === this._modelFilter &&
            this._modelGroupsRevealed === this._revealedModelKey
        )
            return;
        this._modelGroupsState = this._state;
        this._modelGroupsQuery = this._modelFilter;
        this._modelGroupsRevealed = this._revealedModelKey;
        this._modelGroupsError = null;
        const request = ++this._modelGroupsRequest;
        this._call(
            'GetModelGroups',
            '(ss)',
            [this._modelFilter, this._revealedModelKey ?? ''],
            raw => {
                if (request !== this._modelGroupsRequest) return;
                this._modelGroups = JSON.parse(raw) as ModelGroup[];
                if (this._tab === 'Settings') this._renderPage();
            },
            error => {
                if (request === this._modelGroupsRequest) {
                    this._modelGroups = [];
                    this._modelGroupsError = error.message;
                    this._renderPage();
                }
                return false;
            },
        );
    },

    _apply(this: TokenPanel, raw: string): void {
        try {
            const previous = this._state;
            // _apply fills in the client-side PanelState fields below before anything reads them.
            this._state = JSON.parse(raw) as PanelState;
            for (const provider of Object.values(this._state.providers)) {
                for (const row of provider.windows) Object.assign(row, quotaPresentation(row, this._state.now));
            }
            this._state.cells = statusCells(this._state);
            this._state.snapshot_rows = snapshotRows(this._state.cells, this._state.providers);
            this._state.status_tooltip = statusTooltip(this._state.cells);
            this._state.status_icon_tone = statusIconTone(
                this._state.cells,
                this._state.providers,
                this._state.settings,
            );
            this._state.empty_usage_title = emptyUsageTitle(this._state.settings);
            this._state.mini_bar_groups = miniBarGroups(this._state.cells, this._state.status_mini_groups);
            this._state.status_variants = statusVariants(
                this._state.cells,
                this._state.settings,
                this._state.status_provider_indices,
            );
            this._timePresentation = timePresentation(this._state, GLib.get_real_time() / 1_000_000);
            this._state.time_presentation = this._timePresentation;
            this._modelGroups = this._state.model_groups;
            this._providerIds = this._state.provider_definitions.map(provider => provider.id);
            this._usageOrder = [...this._providerIds].sort();
            this._providerNames = Object.fromEntries(
                this._state.provider_definitions.map(provider => [provider.id, provider.name]),
            );
            const state = this._state;
            const historyScopeChanged =
                previous &&
                (this._providerIds.some(id => previous.settings.providers[id] !== state.settings.providers[id]) ||
                    JSON.stringify(previous.selected_window_keys) !== JSON.stringify(this._state.selected_window_keys));
            if (historyScopeChanged) {
                this._history = null;
                this._historyError = null;
                this._costAt = 0;
                this._reviewAt = 0;
            }
            if (this._costDemo !== this._state.settings.demo) {
                this._costAt = 0;
                this._costs = null;
                this._costError = null;
                this._reviews = null;
                this._reviewAt = 0;
                this._costDemo = this._state.settings.demo;
            }
            if (!this._preferencesLoaded) {
                const settings = this._state.settings;
                this._tab = settings.last_tab;
                this._days = settings.history_days;
                this._metric = settings.history_metric;
                this._rollup = settings.history_rollup;
                this._utc = settings.history_utc;
                this._stacked = settings.history_stacked;
                this._preferencesLoaded = true;
            }
            this._hidden = new Set(this._state.settings.history_hidden_keys);
            if (this._history)
                this._shown = new Set(
                    this._history.series.map(series => series.id).filter(id => !this._hidden.has(id)),
                );
            this._error = null;
            this._render();
            if (this._state.first_run_pending && !this._firstRunOpened) {
                this._firstRunOpened = true;
                this._tab = 'Usage';
                this._render();
                if (Main.layoutManager._startingUp) {
                    Main.layoutManager.connectObject(
                        'startup-complete',
                        () => {
                            Main.layoutManager.disconnectObject(this._indicator);
                            this._render();
                            openInstantly(this._indicator.menu);
                        },
                        this._indicator,
                    );
                } else {
                    openInstantly(this._indicator.menu);
                }
            }
            if (historyScopeChanged && this._tab === 'History') this._loadHistory();
            if (this._tab === 'Usage') this._loadCosts();
        } catch (error) {
            this._error = `Invalid service response: ${(error as Error).message}`;
            this._render();
        }
    },

    _set(this: TokenPanel, key: string, value: unknown): void {
        this._call('SetSetting', '(ss)', [key, JSON.stringify(value)], raw => {
            this._apply(raw);
        });
    },

    _refresh(this: TokenPanel, provider = 'all'): void {
        this._costAt = 0;
        this._reviewAt = 0;
        this._call('Refresh', '(s)', [provider], () => {
            this._load();
        });
    },

    _loadCosts(this: TokenPanel): void {
        if (
            !this._indicator.menu.isOpen ||
            this._tab !== 'Usage' ||
            !['claude', 'codex'].some(id => this._state?.settings.providers[id])
        )
            return;
        if (Date.now() - this._costAt < 60_000) {
            if (this._costs) this._loadReviews();
            return;
        }
        this._costAt = Date.now();
        this._costError = null;
        this._call(
            'GetSpendSummary',
            '()',
            [],
            raw => {
                try {
                    this._costs = (JSON.parse(raw) as SpendReport).by_provider;
                    if (this._tab === 'Usage' && this._indicator.menu.isOpen) this._renderPage();
                    this._loadReviews();
                } catch (error) {
                    this._costs = {};
                    this._costError = `Cost history error: ${(error as Error).message}`;
                    this._renderPage();
                    this._loadReviews();
                }
            },
            error => {
                this._costs = {};
                this._costError = `Cost history error: ${error.message}`;
                this._renderPage();
                this._loadReviews();
                return false;
            },
        );
    },

    _loadReviews(this: TokenPanel): void {
        const {codex} = this._state?.settings.providers ?? {};
        if (!this._indicator.menu.isOpen || this._tab !== 'Usage' || !codex || Date.now() - this._reviewAt < 60_000)
            return;
        this._reviewAt = Date.now();
        const show = (value: string | null) => {
            const previous = this._reviews;
            this._reviews = value;
            if (this._reviews !== previous && this._tab === 'Usage' && this._indicator.menu.isOpen) this._renderPage();
        };
        this._call(
            'GetReviewSummary',
            '()',
            [],
            raw => {
                show(raw ? reviewCountText(JSON.parse(raw) as ReviewCounts) : null);
            },
            () => {
                show(null);
                return false;
            },
        );
    },
};

export type ServiceMethods = typeof serviceMethods;
