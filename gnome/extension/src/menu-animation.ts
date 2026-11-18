import * as Config from 'resource:///org/gnome/shell/misc/config.js';
import * as BoxPointer from 'resource:///org/gnome/shell/ui/boxpointer.js';
import type * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
import {noAnimation} from './popup-animation.js';

type Menu = Pick<PopupMenu.PopupMenuBase, 'open' | 'close'>;

// The @girs typings describe only GNOME 50's animation flags.
const instant = noAnimation(Config.PACKAGE_VERSION, BoxPointer.PopupAnimation.NONE) as BoxPointer.PopupAnimation;

export function openInstantly(menu: Menu): void {
    menu.open(instant);
}

export function closeInstantly(menu: Menu): void {
    menu.close(instant);
}
