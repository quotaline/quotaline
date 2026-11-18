import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Pango from 'gi://Pango';
import St from 'gi://St';
import * as DND from 'resource:///org/gnome/shell/ui/dnd.js';
import {openInstantly} from '../menu-animation.js';
import type {TokenPanel} from '../panel.js';
import type {PanelState} from '../service-state.js';
import {COPY} from '../shared-copy.js';
import type {CheckKey, SegmentKey} from './support.js';
import {box, button, CHECK_HELP, checkButton, label, SEGMENT_HELP} from './support.js';

type StepperKey = 'retention_days' | 'analytics_interval_minutes' | 'workdays';
type ReorderKind = 'provider' | 'model';
type ReorderMove = (source: string, target: string, after: boolean) => void;

interface DragSource {
    kind: ReorderKind;
    key: string;
}

export const controlMethods = {
    _section(this: TokenPanel, title: string): void {
        const section = box(true, 'tmb-settings-section');
        section.add_child(label(title.toUpperCase(), 'tmb-section'));
        this._page.add_child(section);
        this._settingsSection = section;
    },

    _settingRow(this: TokenPanel, title: string, control: St.Widget, expand = false): St.BoxLayout {
        const veryNarrow = this._root.width < 520;
        const row = box(veryNarrow, 'tmb-setting-row');
        if (title || !veryNarrow) {
            const name = label(title, 'tmb-setting-name');
            if (!veryNarrow) name.width = this._compact ? 100 : 116;
            row.add_child(name);
        }
        control.x_expand = expand;
        if (veryNarrow && control.has_style_class_name('tmb-setting-check')) control.x_align = Clutter.ActorAlign.START;
        row.add_child(control);
        if (!expand && !veryNarrow) {
            const spacer = box();
            spacer.x_expand = true;
            row.add_child(spacer);
        }
        this._settingsSection.add_child(row);
        if (veryNarrow) {
            const children = row.get_children();
            row.height =
                children.reduce((height, child) => height + child.get_preferred_height(this._root.width - 28)[1], 0) +
                Math.max(0, children.length - 1) * 7 +
                2;
        }
        return row;
    },

    _segments(
        this: TokenPanel,
        state: PanelState,
        key: SegmentKey,
        options: [string | number, string][],
    ): St.BoxLayout {
        const group = box(false, 'tmb-setting-segments');
        const current = state.settings[key];
        for (const [value, title] of options) {
            const segment = button(
                title,
                () => {
                    this._set(key, value);
                },
                `tmb-setting-segment${current === value ? ' tmb-setting-segment-selected' : ''}`,
            );
            segment.accessible_name = `${key}: ${value}`;
            this._settingFocus.set(`segment:${key}:${value}`, segment);
            this._tooltip.attach(segment, SEGMENT_HELP[key][0], SEGMENT_HELP[key][1]);
            group.add_child(segment);
        }
        return group;
    },

    _check(this: TokenPanel, state: PanelState, title: string, key: CheckKey): St.Button {
        const selected = state.settings[key];
        const control = checkButton(
            title,
            selected,
            () => {
                this._set(key, !selected);
            },
            `tmb-setting-check${selected ? ' tmb-setting-check-on' : ''}`,
        );
        control.accessible_name = title;
        this._settingFocus.set(`check:${key}`, control);
        this._tooltip.attach(control, title, CHECK_HELP[key]);
        return control;
    },

    _reorderButton(
        this: TokenPanel,
        name: string,
        direction: 'up' | 'down',
        enabled: boolean,
        action: () => void,
    ): St.Button {
        const control = button('', action, 'tmb-model-move');
        control.set_child(label(direction === 'up' ? '⌃' : '⌄', 'tmb-model-move-glyph'));
        control.accessible_name = name;
        this._tooltip.attach(control, name, 'Moves one place in Stable order.');
        control.reactive = enabled;
        control.can_focus = enabled;
        control.opacity = enabled ? 255 : 90;
        return control;
    },

    _settingNote(this: TokenPanel, text: string): void {
        const note = label(text, 'tmb-setting-note');
        note.width = Math.max(220, this._root.width - 54);
        note.clutter_text.set_line_wrap(true);
        note.clutter_text.set_ellipsize(Pango.EllipsizeMode.NONE);
        this._settingsSection.add_child(note);
    },

    _stepper(
        this: TokenPanel,
        state: PanelState,
        key: StepperKey,
        title: string,
        suffix: string,
        minimum: number,
        maximum: number,
        step: number,
        help: string,
    ): St.BoxLayout {
        const value = state.settings[key];
        const controls = box(false, 'tmb-interval-stepper');
        const decrease = button(
            '−',
            () => {
                this._set(key, value - step);
            },
            'tmb-small-control tmb-interval-button',
        );
        decrease.accessible_name = `Decrease ${title}`;
        decrease.reactive = value > minimum;
        decrease.can_focus = decrease.reactive;
        decrease.opacity = decrease.reactive ? 255 : 102;
        controls.add_child(decrease);
        controls.add_child(label(`${value} ${suffix}`, 'tmb-secondary'));
        const increase = button(
            '+',
            () => {
                this._set(key, value + step);
            },
            'tmb-small-control tmb-interval-button',
        );
        increase.accessible_name = `Increase ${title}`;
        increase.reactive = value < maximum;
        increase.can_focus = increase.reactive;
        increase.opacity = increase.reactive ? 255 : 102;
        controls.add_child(increase);
        this._tooltip.attach(controls, title, help);
        return controls;
    },

    _entry(this: TokenPanel, title: string, key: string, value: string): void {
        const input = new St.Entry({text: value, style_class: 'tmb-entry', can_focus: true});
        input.accessible_name = title;
        input.x_expand = true;
        this._tooltip.attach(
            input,
            COPY.customTemplate,
            'Builds each status cell from tokens. Return starts another line; unknown tokens render no text.',
        );
        input.clutter_text.connect('activate', () => {
            this._set(key, input.get_text());
        });
        const controls = box(false, 'tmb-setting-controls');
        controls.add_child(input);
        const save = button('Save', () => {
            this._set(key, input.get_text());
        });
        this._tooltip.attach(save, 'Save template', 'Applies the template to the menu bar and its preview.');
        controls.add_child(save);
        this._settingRow(title, controls, true);
    },

    _moveProvider(this: TokenPanel, state: PanelState, id: string, direction: number): void {
        const visible = Object.entries(state.providers)
            .filter(([, provider]) => provider.windows.length)
            .map(([id]) => id)
            .sort((left, right) => state.ordering.providers.indexOf(left) - state.ordering.providers.indexOf(right));
        const neighbor = visible[visible.indexOf(id) + direction];
        this._call('SwapProvider', '(ss)', [id, neighbor], raw => {
            this._apply(raw);
        });
    },

    _moveModel(this: TokenPanel, state: PanelState, id: string, key: string, direction: number): void {
        const siblings = state.ordering.models.filter(model => model.startsWith(`${id}:`));
        const position = siblings.indexOf(key);
        const neighbor = siblings[position + direction];
        this._reorder('model', key, neighbor as string, direction > 0);
    },

    _dragHandle(this: TokenPanel, kind: ReorderKind, key: string, title: string): St.Label {
        const handle = new St.Label({
            text: '≡',
            style_class: 'tmb-drag-handle',
            reactive: true,
            accessible_name: `Drag ${title} to reorder`,
        });
        this._tooltip.attach(handle, `Move ${title}`, 'Drag to place this row before or after another row.');
        handle._delegate = {
            kind,
            key,
            getDragActor: () => new St.Label({text: title, style_class: 'tmb-drag-preview'}),
        };
        DND.makeDraggable(handle, {dragActorOpacity: 220});
        return handle;
    },

    _reorderTarget(this: TokenPanel, actor: St.Widget, kind: ReorderKind, key: string, move: ReorderMove): void {
        actor.reactive = true;
        const accepts = (source: DragSource | undefined): source is DragSource =>
            source?.kind === kind &&
            source.key !== key &&
            (kind !== 'model' || source.key.split(':')[0] === key.split(':')[0]);
        actor._delegate = {
            handleDragOver: (source: DragSource | undefined) =>
                accepts(source) ? DND.DragMotionResult.MOVE_DROP : DND.DragMotionResult.NO_DROP,
            acceptDrop: (source: DragSource | undefined, _dragActor: Clutter.Actor, _x: number, y: number) => {
                if (!accepts(source)) return false;
                move(source.key, key, y > actor.height / 2);
                return true;
            },
        };
    },

    _moveProviderTo(this: TokenPanel, source: string, target: string, after: boolean): void {
        this._reorder('provider', source, target, after);
    },

    _moveModelTo(this: TokenPanel, source: string, target: string, after: boolean): void {
        this._reorder('model', source, target, after);
    },

    _reorder(this: TokenPanel, kind: ReorderKind, source: string, target: string, after: boolean): void {
        this._call('Reorder', '(sssb)', [kind, source, target, after], raw => {
            this._apply(raw);
        });
    },

    _openDocument(this: TokenPanel, file: string, fragment = ''): void {
        try {
            Gio.AppInfo.launch_default_for_uri(
                `${Gio.File.new_for_path(
                    GLib.build_filenamev([this.path, file]),
                ).get_uri()}${fragment ? `#${fragment}` : ''}`,
                null,
            );
        } catch (error) {
            this._notice = (error as Error).message;
            this._renderPage();
        }
    },

    _showProviderSettings(this: TokenPanel, id: string): boolean {
        if (!this._providerIds.includes(id) || !this._state) return false;
        this._fullLog = false;
        this._providerDetails.add(id);
        this._tab = 'Settings';
        openInstantly(this._indicator.menu);
        this._render();
        this._scrollToSettingsTarget(() => this._providerRows.get(id));
        return true;
    },

    _scrollToSettingsTarget(this: TokenPanel, resolve: () => St.BoxLayout | St.Entry | undefined, focus = false): void {
        if (this._scrollSource) global.stage.disconnect(this._scrollSource);
        this._scrollSource = global.stage.connect('after-paint', () => {
            global.stage.disconnect(this._scrollSource);
            this._scrollSource = global.stage.connect('after-paint', () => {
                global.stage.disconnect(this._scrollSource);
                this._scrollSource = 0;
                if (this._enabled && this._indicator.menu.isOpen && this._tab === 'Settings') {
                    const row = resolve();
                    if (!row) return;
                    const adjustment = this._scroll.get_vadjustment();
                    const y = row.get_transformed_position()[1] - this._page.get_transformed_position()[1];
                    adjustment.set_value(Math.min(adjustment.upper - adjustment.page_size, Math.max(0, y - 40)));
                    if (focus) (row as St.Entry).clutter_text.grab_key_focus();
                }
            });
            global.stage.queue_redraw();
        });
    },

    _showProviderSignIn(this: TokenPanel, id: string): void {
        if (this._showProviderSettings(id)) this._startSignIn(id);
    },

    _startSignIn(this: TokenPanel, id: string): void {
        this._call(
            'StartSignIn',
            '(s)',
            [id],
            started => {
                if (!started) this._openDocument('provider-setup.html', id);
            },
            () => {
                this._openDocument('provider-setup.html', id);
            },
        );
    },
};

export type ControlMethods = typeof controlMethods;
