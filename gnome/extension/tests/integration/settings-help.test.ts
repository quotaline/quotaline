import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import St from 'gi://St';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import type {TestActor} from '../support/actors.js';
import {
    accessibleName,
    buttonLabel,
    clickButton,
    failure,
    findActor,
    indicator as getIndicator,
    instance,
    required,
} from '../support/actors.js';
import {waitOrFail} from '../support/shell-events.js';

function waitFor(check: () => unknown, failure: string | (() => string) = 'Settings help did not update') {
    return waitOrFail(check, failure);
}

export async function run() {
    try {
        Main.overview.hide();
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        indicator.menu.open(false);
        await waitFor(() => findActor(indicator.menu.box, actor => actor.accessible_name === 'Settings'));
        clickButton(required(findActor(indicator.menu.box, actor => actor.accessible_name === 'Settings')));
        const mode = GLib.getenv('TMB_HELP_TARGET') ?? '';
        const footer = mode === 'footer';
        const lower = mode === 'notifications' || mode === 'log';
        const scrolled = [
            'filter',
            'group',
            'selection',
            'meter',
            'label',
            'template',
            'save',
            'provider',
            'threshold',
            'export',
            'reset',
        ].includes(mode);
        if (mode === 'template' || mode === 'save') {
            await waitFor(() => findActor(indicator.menu.box, actor => actor.accessible_name === 'format: custom'));
            clickButton(required(findActor(indicator.menu.box, actor => actor.accessible_name === 'format: custom')));
        }
        if (footer) await waitFor(() => findActor(indicator.menu.box, actor => actor.text === 'ABOUT'));
        const name =
            {
                format: 'format: stacked',
                demo: 'Demo data',
                notifications: 'Notifications',
                log: 'Log level All',
                filter: 'Filter models…',
                group: 'Show all Claude models',
                selection: 'Show Claude Current session',
                meter: 'tmb-model-meter',
                label: 'Claude Current session short label',
                template: 'Template',
                save: 'Save',
                provider: 'Enable Claude',
                threshold: 'Notify at 75%',
                export: 'Export all history CSV…',
                reset: 'Reset all settings…',
            }[mode] ?? 'Hide 0%';
        const phrase =
            {
                format: 'Stacked uses two lines',
                footer: 'History analytics keep their own schedule',
                demo: 'generated provider usage',
                notifications: 'silences all notices',
                log: 'Shows one severity',
                filter: 'Matches provider names',
                group: 'At least one model stays selected',
                selection: 'Unchecking keeps stored history',
                meter: 'when usage was last observed',
                label: 'Clearing the field restores',
                template: 'unknown tokens render no text',
                save: 'Applies the template',
                provider: 'explicit polling choice',
                threshold: 'reaches 75 percent',
                export: 'without changing it',
                reset: 'Stored usage history remains available',
            }[mode] ?? 'zero-usage models';
        const target = (actor: TestActor) =>
            footer
                ? buttonLabel(actor) === 'Refresh'
                : mode === 'filter'
                  ? actor.hint_text === name
                  : mode === 'meter'
                    ? actor.has_style_class_name?.(name)
                    : mode === 'label'
                      ? accessibleName(actor)?.includes(name)
                      : mode === 'save'
                        ? buttonLabel(actor) === name
                        : actor.accessible_name === name;
        await waitFor(() => findActor(indicator.menu.box, target), `Settings control ${name} did not render`);
        if (scrolled) {
            const scroll = required(findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-scroll')));
            const adjustment = instance(scroll, St.ScrollView).get_vadjustment();
            await waitFor(() => adjustment.upper > adjustment.page_size, 'Settings scroll was not allocated');
            await waitFor(() => {
                const control = findActor(indicator.menu.box, target);
                return control && Number.isFinite(control.get_transformed_position()[1]) && control.height > 0;
            }, `Settings control ${name} was not allocated`);
            adjustment.set_value(
                adjustment.value +
                    required(findActor(indicator.menu.box, target)).get_transformed_position()[1] -
                    scroll.get_transformed_position()[1] -
                    80,
            );
            await waitFor(
                () => {
                    const control = required(findActor(indicator.menu.box, target));
                    const y = control.get_transformed_position()[1];
                    const scrollY = scroll.get_transformed_position()[1];
                    return Number.isFinite(y) && y >= scrollY && y + control.height <= scrollY + scroll.height;
                },
                () =>
                    `Settings control ${name} did not enter the viewport: ${JSON.stringify({
                        controlY: findActor(indicator.menu.box, target)?.get_transformed_position()[1],
                        controlHeight: findActor(indicator.menu.box, target)?.height,
                        scrollY: scroll.get_transformed_position()[1],
                        scrollHeight: scroll.height,
                        value: adjustment.value,
                        upper: adjustment.upper,
                        pageSize: adjustment.page_size,
                    })}`,
            );
        }
        if (lower) {
            const scroll = required(findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-scroll')));
            const adjustment = instance(scroll, St.ScrollView).get_vadjustment();
            await waitFor(() => adjustment.upper > adjustment.page_size + 100, 'Settings scroll was not allocated');
            adjustment.set_value(adjustment.upper - adjustment.page_size);
            await waitFor(
                () => {
                    const y = findActor(indicator.menu.box, target)?.get_transformed_position()[1];
                    if (
                        mode === 'log' &&
                        (!Number.isFinite(y) || required(y) < 0 || required(y) >= global.stage.height - 80)
                    )
                        adjustment.set_value(adjustment.upper - adjustment.page_size);
                    return Number.isFinite(y) && required(y) > 0 && required(y) < global.stage.height - 80;
                },
                () =>
                    `Settings control ${name} did not enter the viewport: ${JSON.stringify({
                        controlY: findActor(indicator.menu.box, target)?.get_transformed_position()[1],
                        scrollY: scroll.get_transformed_position()[1],
                        scrollHeight: scroll.height,
                        value: adjustment.value,
                        upper: adjustment.upper,
                        pageSize: adjustment.page_size,
                    })}`,
            );
        }
        await waitFor(() => {
            const control = findActor(indicator.menu.box, target);
            const focus = ['filter', 'label', 'template'].includes(mode)
                ? instance(control, St.Entry).clutter_text
                : control;
            if (!focus?.has_key_focus()) focus?.grab_key_focus();
            return findActor(
                Main.layoutManager.uiGroup,
                actor =>
                    actor.has_style_class_name?.('tmb-tooltip') &&
                    actor.visible &&
                    findActor(actor, child => child.text?.includes(phrase)),
            );
        }, `Settings help for ${name} did not open`);
        const control = findActor(indicator.menu.box, target);
        if (
            !(mode === 'filter' || mode === 'label' || mode === 'template'
                ? instance(control, St.Entry).clutter_text.has_key_focus()
                : control?.has_key_focus())
        )
            throw new Error('Settings refresh lost focus from the control');
        if (lower) {
            const popup = required(
                findActor(
                    Main.layoutManager.uiGroup,
                    actor => actor.has_style_class_name?.('tmb-tooltip') && actor.visible,
                ),
            );
            const root = required(findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-root')));
            const [popupX] = popup.get_transformed_position();
            const [rootX] = root.get_transformed_position();
            if (popupX < rootX + 4 || popupX + popup.width > rootX + root.width - 4)
                throw new Error('Settings help escaped the popover horizontally');
        }
        if (mode === 'notifications')
            await waitFor(() => {
                const popup = findActor(
                    Main.layoutManager.uiGroup,
                    actor => actor.has_style_class_name?.('tmb-tooltip') && actor.visible,
                );
                const control = findActor(indicator.menu.box, target);
                const root = findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-root'));
                if (!popup || !control || !root) return false;
                const popupTop = popup.get_transformed_position()[1];
                const controlTop = control.get_transformed_position()[1];
                const rootTop = root.get_transformed_position()[1];
                return (
                    popupTop >= rootTop + 8 &&
                    popupTop + popup.height <= rootTop + root.height - 8 &&
                    (popupTop + popup.height < controlTop - 3 || popupTop > controlTop + control.height + 3)
                );
            }, `Settings help for ${name} overlapped its control or left the popover`);
        if (footer) {
            await waitFor(() => {
                const popup = findActor(
                    Main.layoutManager.uiGroup,
                    actor => actor.has_style_class_name?.('tmb-tooltip') && actor.visible,
                );
                const control = findActor(indicator.menu.box, target);
                return (
                    popup &&
                    control &&
                    Math.abs(
                        popup.get_transformed_position()[0] +
                            popup.width / 2 -
                            control.get_transformed_position()[0] -
                            control.width / 2,
                    ) < 8 &&
                    Math.abs(
                        control.get_transformed_position()[1] - popup.get_transformed_position()[1] - popup.height - 7,
                    ) < 5
                );
            });
            const popup = required(
                findActor(
                    Main.layoutManager.uiGroup,
                    actor => actor.has_style_class_name?.('tmb-tooltip') && actor.visible,
                ),
            );
            const control = required(findActor(indicator.menu.box, target));
            const gap = control.get_transformed_position()[1] - popup.get_transformed_position()[1] - popup.height;
            if (gap < 0 || gap > 12) throw new Error(`Footer help gap is ${gap}px`);
        }
        const screenshots = GLib.getenv('TMB_SCREENSHOT_DIR');
        const screenshot =
            {
                demo: 'demo-help',
                notifications: 'notifications-help',
                log: 'log-help',
                footer: 'footer-help',
                selection: 'model-selection-help',
            }[mode] ?? (mode ? null : 'settings-help');
        if (screenshots && screenshot) {
            for (let frame = 0; frame < (lower ? 3 : 1); frame++) {
                await new Promise<void>(resolve => {
                    const signal = global.stage.connect('after-paint', () => {
                        global.stage.disconnect(signal);
                        resolve();
                    });
                    global.stage.queue_redraw();
                });
            }
            GLib.mkdir_with_parents(screenshots, 0o755);
            const stream = Gio.File.new_for_path(GLib.build_filenamev([screenshots, `${screenshot}.png`])).replace(
                null,
                false,
                Gio.FileCreateFlags.REPLACE_DESTINATION,
                null,
            );
            await new Promise<void>((resolve, reject) => {
                new Shell.Screenshot().screenshot(false, stream, (source, result) => {
                    try {
                        required(source).screenshot_finish(result);
                        stream.close(null);
                        resolve();
                    } catch (error) {
                        reject(failure(error));
                    }
                });
            });
        }
        indicator.menu.close(false);
        await waitFor(
            () =>
                !findActor(
                    Main.layoutManager.uiGroup,
                    actor => actor.has_style_class_name?.('tmb-tooltip') && actor.visible,
                ),
        );
    } finally {
        const directory = GLib.getenv('TMB_JS_COVERAGE_DIR');
        if (directory) {
            GLib.mkdir_with_parents(directory, 0o755);
            GLib.file_set_contents(
                GLib.build_filenamev([directory, `${GLib.uuid_string_random()}.json`]),
                JSON.stringify(globalThis.__coverage__ ?? {}),
            );
        }
    }
}

export function finish() {}
