import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import St from 'gi://St';
import {ExtensionState} from 'resource:///org/gnome/shell/misc/extensionUtils.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import type {TestActor} from '../../support/actors.js';
import {clickButton, contentOf, indicator as getIndicator, optionalInstance, required} from '../../support/actors.js';
import type {SmokeContext} from './helpers.js';
import {afterPaint, capture, findActor, serviceSettings, UUID, waitOrFail} from './helpers.js';

export async function run({root, press, indicator}: Pick<SmokeContext, 'root' | 'press' | 'indicator'>) {
    clickButton(
        required(
            findActor(root, actor => actor.accessible_name === 'Usage' && actor.has_style_class_name?.('tmb-tab')),
        ),
    );
    const usageTab = required(
        findActor(root, actor => actor.accessible_name === 'Usage' && actor.has_style_class_name?.('tmb-tab')),
    );
    usageTab.grab_key_focus();
    await press(Clutter.KEY_f, true);
    await waitOrFail(
        () => {
            const filter = findActor(
                contentOf(root.get_children()[1]),
                actor => actor.has_style_class_name?.('tmb-model-filter'),
                St.Entry,
            );
            return filter?.clutter_text.has_key_focus() && serviceSettings().last_tab === 'Settings';
        },
        `Ctrl+F did not focus the model filter: ${JSON.stringify({
            tab: serviceSettings().last_tab,
            filter: Boolean(
                findActor(contentOf(root.get_children()[1]), actor => actor.has_style_class_name?.('tmb-model-filter')),
            ),
            focus: global.stage.get_key_focus()?.constructor.name,
        })}`,
    );
    await capture('settings-filter-shortcut');
    let sourceChanges = 0;
    const subscription = Gio.DBus.session.signal_subscribe(
        'tech.bernat.Quotaline.Gnome',
        'tech.bernat.Quotaline.Gnome',
        'Changed',
        '/tech/bernat/Quotaline/Gnome',
        null,
        Gio.DBusSignalFlags.NONE,
        () => sourceChanges++,
    );
    await press(Clutter.KEY_r, true);
    await waitOrFail(() => sourceChanges > 0, 'Ctrl+R did not request a provider refresh');
    if (
        !optionalInstance(
            findActor(contentOf(root.get_children()[1]), actor => actor.has_style_class_name?.('tmb-model-filter')),
            St.Entry,
        )?.clutter_text.has_key_focus()
    )
        throw new Error('Refreshing lost keyboard focus from the model filter');
    Gio.DBus.session.signal_unsubscribe(subscription);
    await press(Clutter.KEY_Escape);
    await waitOrFail(() => !indicator.menu.isOpen, 'Escape did not close the menu');
    await afterPaint(() => {
        indicator.menu.open(false);
    });
    if (!Main.extensionManager.disableExtension(UUID))
        throw new Error('Disabling the extension left its panel item behind');
    await waitOrFail(
        () => Main.extensionManager.lookup(UUID).state === ExtensionState.INACTIVE && !getIndicator(),
        'Disabling the extension left its panel item behind',
    );
    if (!Main.extensionManager.enableExtension(UUID))
        throw new Error('Re-enabling the extension did not restore its panel item');
    await waitOrFail(
        () => Main.extensionManager.lookup(UUID).state === ExtensionState.ACTIVE && getIndicator(),
        'Re-enabling the extension did not restore its panel item',
    );
    await afterPaint(() => {});
    for (let cycle = 0; cycle < 2; cycle++) {
        const item = required(getIndicator());
        await afterPaint(() => {
            item.menu.open(false);
        });
        await waitOrFail(
            () =>
                item.menu.isOpen &&
                item.menu.box.get_children().filter((actor: TestActor) => actor.has_style_class_name?.('tmb-root'))
                    .length === 1,
            `Extension cycle ${cycle} did not open one menu`,
        );
        await waitOrFail(() => {
            const menuRoot = required(findActor(item.menu.box, actor => actor.has_style_class_name?.('tmb-root')));
            const left = menuRoot.get_transformed_position()[0];
            return menuRoot.width >= 440 && left >= 0 && left + menuRoot.width <= global.stage.width;
        }, `Extension cycle ${cycle} lost the menu layout`);
        if (!Main.extensionManager.disableExtension(UUID))
            throw new Error(`Extension cycle ${cycle} left its panel item behind`);
        await waitOrFail(
            () => Main.extensionManager.lookup(UUID).state === ExtensionState.INACTIVE && !getIndicator(),
            `Extension cycle ${cycle} left its panel item behind`,
        );
        if (!Main.extensionManager.enableExtension(UUID))
            throw new Error(`Extension cycle ${cycle} did not restore its panel item`);
        await waitOrFail(
            () => Main.extensionManager.lookup(UUID).state === ExtensionState.ACTIVE && getIndicator(),
            `Extension cycle ${cycle} did not restore its panel item`,
        );
        await afterPaint(() => {});
    }
    const layoutItem = required(getIndicator());
    await afterPaint(() => {
        layoutItem.menu.open(false);
    });
    clickButton(
        required(
            findActor(
                layoutItem.menu.box,
                actor => actor.accessible_name === 'History' && actor.has_style_class_name?.('tmb-tab'),
            ),
        ),
    );
    if (!Main.extensionManager.disableExtension(UUID))
        throw new Error('Disabling after a tab change left its panel item behind');
    await waitOrFail(
        () => Main.extensionManager.lookup(UUID).state === ExtensionState.INACTIVE && !getIndicator(),
        'Disabling after a tab change left its panel item behind',
    );
    if (!Main.extensionManager.enableExtension(UUID))
        throw new Error('Re-enabling after rapid shutdown did not restore the panel item');
    await waitOrFail(
        () => Main.extensionManager.lookup(UUID).state === ExtensionState.ACTIVE && getIndicator(),
        'Re-enabling after rapid shutdown did not restore the panel item',
    );
    if (!Main.extensionManager.disableExtension(UUID))
        throw new Error('Final extension shutdown left its panel item behind');
    await waitOrFail(
        () => Main.extensionManager.lookup(UUID).state === ExtensionState.INACTIVE && !getIndicator(),
        'Final extension shutdown left its panel item behind',
    );
}
