import Clutter from 'gi://Clutter';
import St from 'gi://St';
import type {TokenPanel} from '../panel.js';
import type {LogEntry, PanelState} from '../service-state.js';
import {COPY} from '../shared-copy.js';
import {box, button, clear, label} from './support.js';

export const logMethods = {
    _renderLog(this: TokenPanel, state: PanelState): void {
        this._settingRow('Detail', this._check(state, COPY.detailedLogging, 'detailed_logging'));
        const veryNarrow = this._root.width < 440;
        const controls = box(veryNarrow, 'tmb-setting-controls');
        if (veryNarrow) controls.x_align = Clutter.ActorAlign.START;
        const firstLevels = veryNarrow ? box(false, 'tmb-setting-controls') : controls;
        const lastLevels = veryNarrow ? box(false, 'tmb-setting-controls') : firstLevels;
        const levels = veryNarrow ? [firstLevels, lastLevels] : [firstLevels];
        if (veryNarrow)
            levels.forEach(row => {
                controls.add_child(row);
            });
        const list = box(true, 'tmb-log-list');
        const scroll = new St.ScrollView({style_class: 'tmb-log-scroll', overlay_scrollbars: true});
        scroll.set_child(list);
        const pages = box(false, 'tmb-log-pages');
        const position = label('', 'tmb-secondary');
        let shown: LogEntry[] = [];
        let older: St.Button;
        let newer: St.Button;
        const matches = (entry: LogEntry) =>
            (this._logLevel === 'All' || entry.level === this._logLevel.toLowerCase()) &&
            entry.line.toLocaleLowerCase().includes(this._logSearch.trim().toLocaleLowerCase());
        const renderRows = () => {
            clear(list);
            const matching = (this._logEntries ?? []).filter(matches);
            const count = Math.max(1, Math.ceil(matching.length / 200));
            this._logPage = Math.min(this._logPage, count - 1);
            shown = this._fullLog
                ? matching.slice(
                      Math.max(0, matching.length - (this._logPage + 1) * 200),
                      matching.length - this._logPage * 200,
                  )
                : matching.slice(-200).reverse();
            for (const entry of shown) {
                const row = box(veryNarrow, 'tmb-log-row');
                const heading = veryNarrow ? box(false, 'tmb-log-row') : row;
                if (veryNarrow) row.add_child(heading);
                heading.add_child(label(new Date(entry.timestamp).toLocaleTimeString(), 'tmb-log-time'));
                heading.add_child(label(entry.level.toUpperCase(), `tmb-log-level tmb-log-${entry.level}`));
                const message = label(`${entry.category} · ${entry.message}`, 'tmb-log-message');
                message.width = veryNarrow ? this._root.width - 40 : Math.max(190, this._root.width - 245);
                message.clutter_text.set_line_wrap(true);
                row.add_child(message);
                list.add_child(row);
            }
            if (this._logEntries === null) list.add_child(label('Loading log…', 'tmb-secondary'));
            else if (matching.length === 0) list.add_child(label('No matching log entries', 'tmb-secondary'));
            position.set_text(`Page ${this._logPage + 1} of ${count}`);
            if (this._fullLog) {
                older.reactive = this._logPage < count - 1;
                newer.reactive = this._logPage > 0;
            }
            const contentHeight = shown.reduce(
                (height, entry) =>
                    height + 22 * Math.max(1, Math.ceil((entry.category.length + entry.message.length + 3) / 75)),
                12,
            );
            const maximum = this._fullLog ? Math.min(560, Math.max(100, this._panelMonitor().height - 190)) : 150;
            scroll.style = `height: ${Math.min(maximum, Math.max(this._fullLog ? 150 : 36, contentHeight))}px;`;
        };
        for (const [index, title] of ['All', 'Debug', 'Info', 'Warning', 'Error'].entries()) {
            const choice = button(
                title,
                () => {
                    this._logLevel = title;
                    this._logPage = 0;
                    for (const row of levels) {
                        for (const sibling of row.get_children()) {
                            if (sibling instanceof St.Button)
                                sibling.remove_style_class_name('tmb-setting-segment-selected');
                        }
                    }
                    choice.add_style_class_name('tmb-setting-segment-selected');
                    renderRows();
                },
                `tmb-setting-segment${this._logLevel === title ? ' tmb-setting-segment-selected' : ''}`,
            );
            choice.accessible_name = `Log level ${title}`;
            this._tooltip.attach(
                choice,
                COPY.logLevel,
                'Shows one severity. All keeps debug, information, warning and error lines visible.',
            );
            (index >= 3 ? lastLevels : firstLevels).add_child(choice);
        }
        const search = new St.Entry({
            text: this._logSearch,
            hint_text: COPY.searchLogPlaceholder,
            style_class: 'tmb-log-search',
            can_focus: true,
        });
        if (veryNarrow) search.x_align = Clutter.ActorAlign.START;
        search.accessible_name = COPY.searchLog;
        this._tooltip.attach(search, COPY.searchLog, COPY.searchLogHelp);
        search.clutter_text.connect('text-changed', () => {
            this._logSearch = search.get_text();
            this._logPage = 0;
            renderRows();
        });
        controls.add_child(search);
        this._settingsSection.add_child(controls);
        const actions = box(false, 'tmb-setting-controls');
        const copy = button(
            'Copy',
            () => {
                const text = shown.map(entry => entry.line).join('\n');
                St.Clipboard.get_default().set_text(St.ClipboardType.CLIPBOARD, text);
            },
            'tmb-small-control',
        );
        this._tooltip.attach(copy, COPY.copyLog, COPY.copyLogHelp);
        actions.add_child(copy);
        let clearPending = false;
        const clearButton = button(
            'Clear…',
            () => {
                if (!clearPending) {
                    clearPending = true;
                    clearButton.set_label('Confirm clear log');
                } else {
                    this._call('ClearLogs', '()', [], () => {
                        this._logEntries = [];
                        this._renderPage();
                        this._scrollToSettingsBottom();
                    });
                }
            },
            'tmb-small-control',
        );
        this._tooltip.attach(clearButton, COPY.clearLog, COPY.clearLogHelp);
        actions.add_child(clearButton);
        const fullLog = button(
            this._fullLog ? 'Show recent log' : COPY.showFullLog,
            () => {
                this._fullLog = !this._fullLog;
                this._logEntries = null;
                this._logLoading = false;
                this._logDirty = false;
                this._logRequest++;
                this._logFocus = !this._fullLog;
                this._logPage = 0;
                this._loadLogs();
                this._renderPage();
                if (this._fullLog) this._scroll.get_vadjustment().set_value(0);
            },
            'tmb-small-control',
        );
        this._tooltip.attach(
            fullLog,
            this._fullLog ? 'Show recent log' : COPY.showFullLog,
            this._fullLog
                ? 'Returns to the 200 most recent lines.'
                : 'Shows every retained line with search and severity filters.',
        );
        actions.add_child(fullLog);
        this._settingsSection.add_child(actions);
        if (this._fullLog) {
            older = button(
                'Older',
                () => {
                    this._logPage++;
                    renderRows();
                },
                'tmb-small-control',
            );
            newer = button(
                'Newer',
                () => {
                    this._logPage--;
                    renderRows();
                },
                'tmb-small-control',
            );
            pages.add_child(older);
            pages.add_child(position);
            pages.add_child(newer);
            this._settingsSection.add_child(pages);
        }
        renderRows();
        if (this._logEntries === null) this._loadLogs();
        this._settingsSection.add_child(scroll);
    },
};

export type LogMethods = typeof logMethods;
