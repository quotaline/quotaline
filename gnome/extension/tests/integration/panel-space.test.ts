import GLib from 'gi://GLib';
import St from 'gi://St';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {findActor, indicator as getIndicator, required} from '../support/actors.js';
import {afterPaint, waitOrFail} from '../support/shell-events.js';

export async function run() {
    const indicator = required(getIndicator());
    const cells = required(findActor(indicator, actor => actor.has_style_class_name?.('tmb-panel-cells')));
    const icon = required(required(cells.get_parent()).get_children()[0]);
    await waitOrFail(() => indicator.width > 200, 'Provider status did not fill the panel');
    const box = required(indicator.container.get_parent());
    const host = GLib.getenv('TMB_PANEL_PEER') ? required(box.get_parent()) : box;
    const neighbor = new St.Widget({width: 1000, height: 20});
    host.add_child(neighbor);
    try {
        Main.layoutManager.emit('monitors-changed');
        await afterPaint();
        if (!icon.visible) throw new Error('Provider status did not yield space to another panel item');
    } finally {
        host.remove_child(neighbor);
        neighbor.destroy();
    }
}

export function finish() {}
