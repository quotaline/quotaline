import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import St from 'gi://St';
import {findActor, indicator as getIndicator, required} from '../support/actors.js';
import {afterPaint, waitOrFail} from '../support/shell-events.js';

export async function run() {
    const indicator = required(getIndicator());
    const cells = required(findActor(indicator, actor => actor.has_style_class_name?.('tmb-panel-cells')));
    const stacked = () => cells.get_children().find(actor => actor.has_style_class_name?.('tmb-panel-cell-stacked'));
    const mini = Boolean(GLib.getenv('TMB_PANEL_MINI'));
    if (mini)
        for (const [key, value] of [
            ['selected_windows', ['claude:session', 'claude:weekly']],
            ['format', 'mini-bars'],
        ] as const)
            Gio.DBus.session.call_sync(
                'tech.bernat.Quotaline.Gnome',
                '/tech/bernat/Quotaline/Gnome',
                'tech.bernat.Quotaline.Gnome',
                'SetSetting',
                new GLib.Variant('(ss)', [key, JSON.stringify(value)]),
                new GLib.VariantType('(s)'),
                Gio.DBusCallFlags.NONE,
                2000,
                null,
            );
    const rows = () => findActor(cells, actor => actor.has_style_class_name?.('tmb-panel-mini-rows'));
    await waitOrFail(
        () => (mini ? rows()?.get_n_children() === 2 : stacked()?.has_allocation()),
        'Provider status did not load',
    );
    const flatten = (actor: Clutter.Actor) => {
        if (actor instanceof St.BoxLayout) actor.orientation = Clutter.Orientation.HORIZONTAL;
        for (const child of actor.get_children()) flatten(child);
    };
    flatten(indicator);
    await afterPaint();
    const [first, second] = required(mini ? rows() : stacked()).get_children();
    if (required(first).get_transformed_position()[1] >= required(second).get_transformed_position()[1])
        throw new Error('Panel layout reversed the provider status rows');
}

export function finish() {}
