import {ExtensionState} from 'resource:///org/gnome/shell/misc/extensionUtils.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {indicator} from './actors.js';
import {waitOrFail} from './shell-events.js';

const UUID = 'quotaline@bernat.tech';

// GNOME 51 applies enable and disable requests after an asynchronous lock, so the change lands after the call returns.
export async function disableExtension(failure: string): Promise<void> {
    if (!Main.extensionManager.disableExtension(UUID)) throw new Error(failure);
    await waitOrFail(
        () => Main.extensionManager.lookup(UUID).state === ExtensionState.INACTIVE && !indicator(),
        failure,
    );
}

export async function enableExtension(failure: string): Promise<void> {
    if (!Main.extensionManager.enableExtension(UUID)) throw new Error(failure);
    await waitOrFail(() => Main.extensionManager.lookup(UUID).state === ExtensionState.ACTIVE && indicator(), failure);
}
