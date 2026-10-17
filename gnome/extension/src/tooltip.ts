import Clutter from 'gi://Clutter';
import GLib from 'gi://GLib';
import St from 'gi://St';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';

// Walking up from a widget can reach plain Clutter actors, which have no style classes.
type Ancestor = Clutter.Actor & Partial<Pick<St.Widget, 'has_style_class_name'>>;

export type TooltipBody = string | (() => string);

export class Tooltip {
    _popup: St.BoxLayout;
    _title: St.Label;
    _body: St.Label;
    _target: St.Widget | null;
    _timer: number;
    _follow: number;

    constructor() {
        this._popup = new St.BoxLayout({
            orientation: Clutter.Orientation.VERTICAL,
            style_class: 'tmb-tooltip',
            visible: false,
            reactive: false,
        });
        this._title = new St.Label({style_class: 'tmb-tooltip-title'});
        this._body = new St.Label({style_class: 'tmb-tooltip-body'});
        this._body.clutter_text.set_line_wrap(true);
        this._popup.add_child(this._title);
        this._popup.add_child(this._body);
        Main.layoutManager.uiGroup.add_child(this._popup);
        this._target = null;
        this._timer = 0;
        this._follow = 0;
    }

    attach(actor: St.Widget, title: string, body: TooltipBody): void {
        const focus = actor instanceof St.Entry ? actor.clutter_text : actor;
        actor.reactive = true;
        actor.track_hover = true;
        actor.can_focus = true;
        actor.connect('notify::hover', () => {
            if (actor.hover) this._arm(actor, title, body);
            else if (!focus.has_key_focus()) this.hide(actor);
        });
        focus.connect('key-focus-in', () => {
            this._arm(actor, title, body);
        });
        focus.connect('key-focus-out', () => {
            if (!actor.hover) this.hide(actor);
        });
        actor.connect('destroy', () => {
            this.hide(actor);
        });
    }

    destroy(): void {
        this.hide();
        this._popup.destroy();
    }

    _arm(actor: St.Widget, title: string, body: TooltipBody): void {
        if (actor === this._target) return;
        this.hide();
        this._target = actor;
        this._timer = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 150, () => {
            this._timer = 0;
            this._title.text = title;
            this._body.text = typeof body === 'function' ? body() : body;
            let boundary: Ancestor | null = actor;
            let footer = false;
            while (boundary && !boundary.has_style_class_name?.('tmb-root')) {
                footer = footer || Boolean(boundary.has_style_class_name?.('tmb-footer'));
                boundary = boundary.get_parent();
            }
            const horizontalBoundary = footer ? null : boundary;
            const width = Math.min(320, Math.max(80, (boundary?.width ?? global.stage.width) - 16));
            this._popup.width = width;
            this._body.width = width - 20;
            this._position(actor, boundary, horizontalBoundary);
            Main.layoutManager.uiGroup.set_child_above_sibling(this._popup, null);
            this._popup.show();
            this._follow = global.stage.connect('after-paint', () => {
                this._position(actor, boundary, horizontalBoundary);
            });
            return GLib.SOURCE_REMOVE;
        });
    }

    hide(actor: St.Widget | null = null): void {
        if (actor && actor !== this._target) return;
        if (this._timer) GLib.Source.remove(this._timer);
        if (this._follow) global.stage.disconnect(this._follow);
        this._timer = 0;
        this._follow = 0;
        this._target = null;
        this._popup.hide();
    }

    _position(actor: St.Widget, boundary: Ancestor | null, horizontalBoundary: Ancestor | null): void {
        const width = this._popup.width;
        const popupHeight = this._popup.height;
        const [left, top] = actor.get_transformed_position();
        const boundaryLeft = horizontalBoundary?.get_transformed_position()[0] ?? 0;
        const minimumX = horizontalBoundary ? Math.max(8, boundaryLeft + 8) : 8;
        const maximumX = horizontalBoundary
            ? Math.min(global.stage.width - width - 8, boundaryLeft + horizontalBoundary.width - width - 8)
            : global.stage.width - width - 8;
        const x = Math.max(minimumX, Math.min(maximumX, left + actor.width / 2 - width / 2));
        const below = top + actor.height + 7;
        const bottom = boundary
            ? Math.min(global.stage.height - 8, boundary.get_transformed_position()[1] + boundary.height - 8)
            : global.stage.height - 8;
        const y = below + popupHeight <= bottom ? below : Math.max(8, top - popupHeight - 7);
        const nextX = Math.round(x);
        const nextY = Math.round(y);
        if (this._popup.x !== nextX || this._popup.y !== nextY) this._popup.set_position(nextX, nextY);
    }
}
