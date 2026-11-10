import Atk from 'gi://Atk';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import St from 'gi://St';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {miniBarGroups, statusVariants} from '../../../src/status-presentation.js';
import {
    accessibleName,
    buttonLabel,
    clickButton,
    contentOf,
    instance,
    optionalInstance,
    required,
} from '../../support/actors.js';
import {readProvider} from '../../support/dbus.js';
import type {SmokeContext} from './helpers.js';
import {
    afterPaint,
    capture,
    checkMark,
    containsText,
    findActor,
    findActors,
    serviceLogs,
    serviceSettings,
    serviceState,
    setSetting,
    waitOrFail,
} from './helpers.js';

export async function run({
    demoPhase,
    root,
    adjustment,
    panelCells,
    indicator,
    footer,
    usageColor,
}: Pick<SmokeContext, 'demoPhase' | 'root' | 'adjustment' | 'panelCells' | 'indicator' | 'footer' | 'usageColor'>) {
    const statusCells = () => panelCells.get_children().filter(actor => actor.has_style_class_name?.('tmb-panel-cell'));
    if (demoPhase === 'formats') {
        clickButton(required(required(required(root.get_children()[0]).get_children()[1]).get_children()[2]));
        await waitOrFail(
            () => findActor(contentOf(root.get_children()[1]), actor => actor.accessible_name === 'format: stacked'),
            'Menu format controls did not render',
        );
    }
    adjustment.set_value(0);
    if (GLib.getenv('TMB_EXPECT_DEMO') === '1') {
        const page = contentOf(root.get_children()[1]);
        const format = findActor(page, actor => actor.accessible_name === 'format: inline');
        if (
            !format ||
            !findActor(
                page,
                actor =>
                    actor.accessible_name === 'format: stacked' &&
                    instance(actor, St.Widget).has_style_class_name('tmb-setting-segment-selected'),
            )
        )
            throw new Error('Stacked format is not selected');
        clickButton(format);
        await waitOrFail(() => serviceSettings().format === 'inline', 'Inline menu bar format was not saved');
        await setSetting('selected_windows', ['claude:weekly', 'codex:session']);
        await waitOrFail(
            () =>
                statusCells().length === 2 &&
                statusCells().every(
                    actor => !instance(actor, St.Widget).has_style_class_name('tmb-panel-cell-stacked'),
                ),
            'Two selected limits did not use the configured inline format',
        );
        await capture('menubar-inline');
        await capture('menubar-two-cells');
        await setSetting('has_custom_selection', false);
        await waitOrFail(() => statusCells().length >= 6, 'Default menu bar selection was not restored');
        await setSetting('format', 'countdown');
        const current = serviceState();
        const candidates = statusVariants(current.cells, current.settings, current.status_provider_indices);
        if (
            candidates.map(candidate => candidate.tier).join(',') !== 'configured,stacked,providers,icon' ||
            required(candidates[0]).cells.length < required(candidates[2]).cells.length ||
            required(candidates[3]).cells.length
        )
            throw new Error('Shell did not build the adaptive status candidates');
        const expectedLabels = required(candidates[2]).cells.map(cell => cell.label);
        await waitOrFail(
            () =>
                serviceSettings().format === 'countdown' &&
                statusCells().length >= 6 &&
                statusCells().every(actor =>
                    instance(actor, St.Widget).has_style_class_name('tmb-panel-cell-stacked'),
                ) &&
                statusCells()
                    .map(actor => required(actor.get_children()[0]).text)
                    .join('|') === expectedLabels.join('|'),
            `Constrained menu bar lost provider limits: ${statusCells()
                .map(cell =>
                    cell
                        .get_children()
                        .map(line => line.text ?? buttonLabel(line) ?? '')
                        .join('/'),
                )
                .join(', ')}`,
        );
        await capture('menubar-adaptive');
        await setSetting('format', 'inline');
        await waitOrFail(
            () =>
                serviceSettings().format === 'inline' &&
                statusCells().length >= 6 &&
                findActor(
                    page,
                    actor =>
                        actor.accessible_name === 'format: inline' &&
                        instance(actor, St.Widget).has_style_class_name('tmb-setting-segment-selected'),
                ),
            'Menu bar did not expand to the configured order',
        );
        await waitOrFail(() => {
            const preview = required(findActor(page, actor => actor.has_style_class_name?.('tmb-status-preview')));
            const filter = findActor(page, actor => actor.has_style_class_name?.('tmb-model-filter'));
            const previewY = preview.get_transformed_position()[1];
            const filterY = filter?.get_transformed_position()[1];
            const cells = preview.get_children().flatMap(row => row.get_children());
            return (
                preview.get_n_children() >= 2 &&
                cells.length === 13 &&
                cells.every(cell => cell.width >= 88) &&
                Number.isFinite(previewY) &&
                Number.isFinite(filterY) &&
                previewY + preview.height <= required(filterY)
            );
        }, 'Inline preview overlaps the model controls');
        await capture('models');
        const firstModel = required(
            findActor(page, actor => actor.has_style_class_name?.('tmb-model-group-card')),
        ).get_children()[1];
        const modelUsage = findActor(firstModel, actor => actor.has_style_class_name?.('tmb-model-usage'));
        const modelPercent = findActor(modelUsage, actor => actor.has_style_class_name?.('tmb-model-percent'));
        const firstModelMeter = findActor(modelUsage, actor => actor.has_style_class_name?.('tmb-model-meter'));
        if (
            !modelPercent ||
            !firstModelMeter ||
            firstModelMeter.get_transformed_position()[1] <= modelPercent.get_transformed_position()[1] ||
            firstModelMeter.width < 90
        )
            throw new Error('Settings model gauge is missing beneath the percentage');
        const unused = required(findActor(page, actor => actor.accessible_name === 'Hide unused in range'));
        if (
            !optionalInstance(unused, St.Button)?.toggle_mode ||
            unused.get_accessible().ref_state_set().contains_state(Atk.StateType.CHECKED) ||
            !findActor(page, actor => actor.text?.match(/^\d+(?:\.\d+)?% today$/))
        )
            throw new Error('Model activity and hide-unused control are missing');
        const filterArea = required(findActor(page, actor => actor.has_style_class_name?.('tmb-model-filter')));
        const modelNote = required(
            findActor(
                page,
                actor =>
                    actor.has_style_class_name?.('tmb-setting-note') && containsText(actor, 'Checked limits appear'),
            ),
        );
        const menuRight = root.get_transformed_position()[0] + root.width - 12;
        if (
            filterArea.get_transformed_position()[0] + filterArea.width > menuRight ||
            unused.get_transformed_position()[0] + unused.width > menuRight ||
            (root.width < 740 &&
                unused.get_transformed_position()[1] < filterArea.get_transformed_position()[1] + filterArea.height) ||
            modelNote.get_transformed_position()[1] < unused.get_transformed_position()[1] + unused.height
        )
            throw new Error('Model filter and hide-unused control overflow the Settings menu');
        adjustment.set_value(
            adjustment.value + filterArea.get_transformed_position()[1] - root.get_transformed_position()[1] - 100,
        );
        await afterPaint(() => {});
        await capture('models-controls');
        clickButton(required(findActor(page, actor => actor.accessible_name === 'Hide unused in range')));
        await waitOrFail(
            () =>
                serviceSettings().hide_unused_models &&
                findActor(page, actor => actor.accessible_name === 'Hide unused in range')
                    ?.get_accessible()
                    .ref_state_set()
                    .contains_state(Atk.StateType.CHECKED),
            'Hide-unused choice was not saved',
        );
        clickButton(required(findActor(page, actor => actor.accessible_name === 'Hide unused in range')));
        await waitOrFail(() => !serviceSettings().hide_unused_models, 'Hide-unused choice could not be cleared');
        const filter = findActor(page, actor => actor.has_style_class_name?.('tmb-model-filter'));
        if (!filter) throw new Error('Model filter is missing');
        instance(filter, St.Entry).set_text('weekly');
        await waitOrFail(
            () =>
                findActor(
                    page,
                    actor =>
                        actor.has_style_class_name?.('tmb-model-row') &&
                        actor.visible &&
                        containsText(actor, 'All models'),
                ) &&
                !findActor(
                    page,
                    actor =>
                        actor.has_style_class_name?.('tmb-model-row') &&
                        actor.visible &&
                        containsText(actor, 'Current session'),
                ),
            'Model filter did not limit the visible rows',
        );
        instance(
            required(findActor(page, actor => actor.has_style_class_name?.('tmb-model-filter'))),
            St.Entry,
        ).set_text('');
        await waitOrFail(
            () =>
                findActor(
                    page,
                    actor => actor.has_style_class_name?.('tmb-model-row') && containsText(actor, 'Current session'),
                ),
            'Clearing the model filter did not restore the rows',
        );
        const model = findActor(
            page,
            actor => actor.has_style_class_name?.('tmb-model-row') && containsText(actor, 'Current session'),
        );
        const modelMeter = findActor(model, actor => actor.has_style_class_name?.('tmb-meter-fill'), St.Widget);
        if (!required(modelMeter?.get_style()).includes(usageColor(required(readProvider('claude').windows[0]).used)))
            throw new Error('Model meter does not fit the current Settings layout');
        const shortLabel = findActor(model, actor => actor.has_style_class_name?.('tmb-short-entry'), St.Entry);
        if (
            shortLabel?.get_text() !== 'CC 5h' ||
            findActor(model, actor => buttonLabel(actor) === 'Save') ||
            findActor(model, actor => accessibleName(actor)?.startsWith('Revert label for '))
        )
            throw new Error('Model label field should fit without a Save button');
        global.stage.set_key_focus(instance(shortLabel, St.Entry).clutter_text);
        instance(shortLabel, St.Entry).set_text('TEST123');
        if (shortLabel.get_text() !== 'TEST12' || serviceSettings().short_labels['claude:session'])
            throw new Error('Short label should wait for focus loss and stay within six characters');
        instance(shortLabel, St.Entry).set_text('A👨‍👩‍👧‍👦BCDEF');
        if (shortLabel.get_text() !== 'A👨‍👩‍👧‍👦BCDE')
            throw new Error('Short label length must count visible characters');
        instance(shortLabel, St.Entry).set_text('TEST12');
        global.stage.set_key_focus(
            instance(required(findActor(page, actor => actor.has_style_class_name?.('tmb-model-filter'))), St.Entry)
                .clutter_text,
        );
        await waitOrFail(
            () =>
                serviceSettings().short_labels['claude:session'] === 'TEST12' &&
                indicator.accessible_name.includes('TEST12'),
            'Losing focus did not save the edited short label',
        );
        await capture('menubar-custom-label');
        const sessionEntry = () =>
            findActor(
                findActor(
                    page,
                    actor => actor.has_style_class_name?.('tmb-model-row') && containsText(actor, 'Current session'),
                ),
                actor => actor.has_style_class_name?.('tmb-short-entry'),
            );
        const draftEntry = sessionEntry();
        global.stage.set_key_focus(instance(required(draftEntry), St.Entry).clutter_text);
        instance(required(draftEntry), St.Entry).set_text('DRAFT');
        await setSetting('history_days', serviceSettings().history_days);
        await waitOrFail(() => {
            const entry = sessionEntry();
            return (
                entry &&
                entry !== draftEntry &&
                instance(entry, St.Entry).get_text() === 'DRAFT' &&
                instance(entry, St.Entry).clutter_text.has_key_focus()
            );
        }, 'Refreshing Settings discarded the active short-label draft or focus');
        const preservedEntry = sessionEntry();
        global.stage.set_key_focus(null);
        await waitOrFail(
            () => serviceSettings().short_labels['claude:session'] === 'DRAFT' && sessionEntry() !== preservedEntry,
            'Preserved short-label draft did not save on focus loss',
        );
        const resetEntry = sessionEntry();
        global.stage.set_key_focus(instance(required(resetEntry), St.Entry).clutter_text);
        instance(required(resetEntry), St.Entry).set_text('CC 5h');
        instance(required(resetEntry), St.Entry).clutter_text.emit('activate');
        global.stage.set_key_focus(null);
        await waitOrFail(
            () =>
                serviceSettings().short_labels['claude:session'] === undefined &&
                indicator.accessible_name.includes('CC 5h'),
            'Restoring the default model label did not remove its override',
        );
        instance(required(sessionEntry()), St.Entry).set_text('TEST12');
        instance(required(sessionEntry()), St.Entry).clutter_text.emit('activate');
        await waitOrFail(
            () =>
                serviceSettings().short_labels['claude:session'] === 'TEST12' &&
                indicator.accessible_name.includes('TEST12'),
            'Pressing Enter did not save a model label',
        );
        const revert = findActor(page, actor => actor.accessible_name === 'Revert label for Claude Current session');
        if (!revert) throw new Error('Saved model label has no revert control');
        await capture('models-label-override');
        instance(required(sessionEntry()), St.Entry).set_text('CX 5h');
        await waitOrFail(
            () =>
                required(sessionEntry()).accessible_name.includes('already used by Codex 5-hour') &&
                findActor(
                    required(sessionEntry()).get_parent(),
                    actor => actor.has_style_class_name?.('tmb-label-conflict') && actor.visible,
                ),
            'Duplicate default labels need a visible conflict hint',
        );
        required(
            findActor(required(sessionEntry()).get_parent(), actor =>
                actor.has_style_class_name?.('tmb-label-conflict'),
            ),
        ).grab_key_focus();
        await waitOrFail(
            () =>
                findActor(
                    Main.layoutManager.uiGroup,
                    actor =>
                        actor.has_style_class_name?.('tmb-tooltip') &&
                        actor.visible &&
                        containsText(actor, 'Already used by Codex 5-hour'),
                ),
            'The duplicate-label warning did not explain the conflicting model',
        );
        await capture('models-label-conflict');
        instance(required(sessionEntry()), St.Entry).clutter_text.emit('activate');
        if (serviceSettings().short_labels['claude:session'] !== 'TEST12')
            throw new Error('A conflicting short label changed the saved override');
        const conflictingEntry = sessionEntry();
        await setSetting('history_days', serviceSettings().history_days);
        await waitOrFail(
            () =>
                sessionEntry() !== conflictingEntry &&
                instance(required(sessionEntry()), St.Entry).get_text() === 'CX 5h',
            'A conflicting draft disappeared after a service update',
        );
        instance(required(sessionEntry()), St.Entry).set_text('TEST12');
        await waitOrFail(
            () =>
                !findActor(
                    required(sessionEntry()).get_parent(),
                    actor => actor.has_style_class_name?.('tmb-label-conflict') && actor.visible,
                ),
            'Resolving a label conflict left the hint visible',
        );
        const codexModel = () =>
            findActor(page, actor => actor.has_style_class_name?.('tmb-model-row') && containsText(actor, '5-hour'));
        const codexEntry = () => findActor(codexModel(), actor => actor.has_style_class_name?.('tmb-short-entry'));
        instance(required(codexEntry()), St.Entry).set_text('test12');
        await waitOrFail(
            () => codexEntry()?.accessible_name.includes('already used by Claude Current session'),
            'Duplicate overrides should ignore letter case',
        );
        instance(required(codexEntry()), St.Entry).set_text('CX 5h');
        clickButton(
            required(findActor(page, actor => actor.accessible_name === 'Revert label for Claude Current session')),
        );
        await waitOrFail(
            () =>
                serviceSettings().short_labels['claude:session'] === undefined &&
                indicator.accessible_name.includes('CC 5h') &&
                !findActor(page, actor => actor.accessible_name === 'Revert label for Claude Current session'),
            'Reverting the short label did not restore its default',
        );
        instance(required(sessionEntry()), St.Entry).set_text('TEST12');
        instance(required(sessionEntry()), St.Entry).clutter_text.emit('activate');
        await waitOrFail(
            () =>
                serviceSettings().short_labels['claude:session'] === 'TEST12' &&
                indicator.accessible_name.includes('TEST12'),
            'The short label could not be saved after reverting it',
        );
        clickButton(required(findActor(page, actor => actor.accessible_name === 'Show all Claude models')));
        await waitOrFail(
            () =>
                serviceSettings().selected_windows.length > 0 &&
                !serviceSettings().selected_windows.some(key => key.startsWith('claude:')) &&
                checkMark(findActor(page, actor => actor.accessible_name === 'Show all Claude models')) === '☐',
            'Clearing a provider model group did not update the menu bar selection',
        );
        clickButton(required(findActor(page, actor => actor.accessible_name === 'Show all Claude models')));
        await waitOrFail(
            () =>
                serviceSettings().selected_windows.includes('claude:session') &&
                checkMark(findActor(page, actor => actor.accessible_name === 'Show all Claude models')) === '☑',
            'Restoring a provider model group did not select its windows',
        );
        await setSetting('selected_windows', ['claude:session']);
        await waitOrFail(
            () =>
                serviceSettings().selected_windows.join() === 'claude:session' &&
                checkMark(findActor(page, actor => actor.accessible_name === 'Show Claude Current session')) === '☑',
            'Selecting one model did not update its checkbox',
        );
        const lastModel = required(findActor(page, actor => actor.accessible_name === 'Show Claude Current session'));
        if (lastModel.reactive || !containsText(page, 'At least one model stays selected.'))
            throw new Error('The last selected model is not protected from deselection');
        clickButton(lastModel);
        if (serviceSettings().selected_windows.join() !== 'claude:session')
            throw new Error('The last selected model was removed');
        await setSetting('has_custom_selection', false);
        await waitOrFail(
            () => checkMark(findActor(page, actor => actor.accessible_name === 'Show Claude Current session')) === '☑',
            'Default model selection did not return after clearing the panel',
        );
        clickButton(required(findActor(page, actor => actor.accessible_name === 'Show Claude Current session')));
        await waitOrFail(
            () =>
                !serviceSettings().selected_windows.includes('claude:session') &&
                !indicator.accessible_name.includes('TEST12'),
            'Clearing one model did not remove its panel cell',
        );
        clickButton(required(findActor(page, actor => actor.accessible_name === 'Show Claude Current session')));
        await waitOrFail(
            () =>
                serviceSettings().selected_windows.includes('claude:session') &&
                indicator.accessible_name.includes('TEST12'),
            'Restoring one model did not restore its panel cell',
        );
        const claudeWindows = readProvider('claude').windows.map(row => `claude:${row.id}`);
        await setSetting('selected_windows', claudeWindows);
        await waitOrFail(
            () =>
                serviceSettings().selected_windows.length === claudeWindows.length &&
                checkMark(findActor(page, actor => actor.accessible_name === 'Show all Codex models')) === '☐',
            'Claude-only model selection did not save',
        );
        const lastGroup = required(findActor(page, actor => actor.accessible_name === 'Show all Claude models'));
        if (lastGroup.reactive) throw new Error('The last selected provider group is not protected from deselection');
        clickButton(lastGroup);
        if (serviceSettings().selected_windows.length !== claudeWindows.length)
            throw new Error('The last selected provider group was removed');
        await setSetting('has_custom_selection', false);
        await waitOrFail(
            () =>
                !serviceSettings().has_custom_selection &&
                serviceSettings().selected_windows.length === claudeWindows.length &&
                statusCells().length >= 6,
            'Default model selection did not return',
        );
        await waitOrFail(() => {
            const all = required(findActor(page, actor => actor.accessible_name === 'Show all Codex models'));
            const mark = findActor(all, actor => actor.has_style_class_name?.('tmb-check-mark'));
            return (
                mark?.text === '−' &&
                instance(mark, St.Widget).has_style_class_name('tmb-check-mark-on') &&
                !all.checked
            );
        }, 'Partially selected provider does not show a mixed All checkbox');
        await setSetting('order', 'usage');
        await waitOrFail(
            () =>
                serviceSettings().order === 'usage' &&
                !findActor(page, actor => actor.accessible_name === 'Move Claude Current session down') &&
                !findActor(page, actor => actor.accessible_name === 'Move Codex up'),
            'Usage sort kept Stable order controls visible',
        );
        await setSetting('order', 'stable');
        await waitOrFail(
            () =>
                serviceSettings().order === 'stable' &&
                findActor(page, actor => actor.accessible_name === 'Move Claude Current session down'),
            'Stable sort did not restore order controls',
        );
        if (
            required(findActor(page, actor => actor.accessible_name === 'Move Claude Current session up')).reactive ||
            !required(findActor(page, actor => actor.accessible_name === 'Move Claude Current session down'))
                .reactive ||
            required(findActor(page, actor => actor.accessible_name === 'Move Claude up')).reactive ||
            !required(findActor(page, actor => actor.accessible_name === 'Move Claude down')).reactive
        )
            throw new Error('Stable order controls did not disable their first positions');
        if (
            contentOf(required(findActor(page, actor => actor.accessible_name === 'Move Claude Current session up')))
                .text !== '⌃' ||
            contentOf(required(findActor(page, actor => actor.accessible_name === 'Move Claude Current session down')))
                .text !== '⌄'
        )
            throw new Error('Stable order controls did not render their direction');
        const moveModel = findActor(page, actor => actor.accessible_name === 'Move Claude Current session down');
        if (!moveModel) throw new Error('Model order control is missing');
        clickButton(moveModel);
        await waitOrFail(
            () => indicator.accessible_name.startsWith('CC 7d'),
            'Moving a model did not change panel order',
        );
        clickButton(required(findActor(page, actor => actor.accessible_name === 'Move Claude Current session up')));
        await waitOrFail(
            () =>
                indicator.accessible_name.startsWith('TEST12') && serviceSettings().model_order[0] === 'claude:session',
            `Moving a model up did not restore panel order: ${JSON.stringify({
                panel: indicator.accessible_name,
                order: serviceSettings().model_order,
            })}`,
        );
        const moveProvider = findActor(page, actor => actor.accessible_name === 'Move Codex up');
        if (!moveProvider) throw new Error('Provider order control is missing');
        clickButton(moveProvider);
        await waitOrFail(
            () => indicator.accessible_name.startsWith('CX 5h'),
            'Moving a provider did not change panel order',
        );
        await setSetting('selected_windows', ['claude:session', 'claude:weekly', 'codex:session']);
        clickButton(required(findActor(page, actor => actor.accessible_name === 'format: mini-bars')));
        await waitOrFail(
            () =>
                serviceSettings().format === 'mini-bars' &&
                statusCells().length === 2 &&
                statusCells().every(actor => instance(actor, St.Widget).has_style_class_name('tmb-panel-mini')),
            'Mini bars format did not group limits by provider',
        );
        const claudeBars = required(
            statusCells()
                .find(actor => accessibleName(actor)?.startsWith('Claude:'))
                ?.get_children()[1],
        ).get_children();
        if (
            miniBarGroups(serviceState().cells, serviceState().status_mini_groups).find(
                group => group.provider === 'claude',
            )?.bars.length !== 2
        )
            throw new Error('Shell did not group Claude mini bars');
        const miniPreview = findActor(page, actor => actor.has_style_class_name?.('tmb-status-preview'));
        const widths = claudeBars
            .map(row => required(required(row.get_children()[1]).get_children()[0]).width)
            .sort((a, b) => a - b);
        const expectedWidths = serviceState()
            .cells.filter(cell => cell.provider === 'claude')
            .map(cell => Math.max(2, Math.round((30 * Math.clamp(cell.used, 0, 100)) / 100)))
            .sort((left, right) => left - right);
        if (
            claudeBars.length !== 2 ||
            !claudeBars.every(row =>
                instance(row.get_children()[1], St.Widget).has_style_class_name('tmb-panel-mini-track'),
            ) ||
            widths.join(',') !== expectedWidths.join(',') ||
            miniPreview?.get_children().flatMap(row => row.get_children()).length !== 2
        )
            throw new Error(
                `Mini bars layout: rows=${claudeBars.length}, widths=${JSON.stringify(widths)}, preview=${miniPreview?.get_n_children()}`,
            );
        await capture('menubar-mini-bars');
        await setSetting('selected_windows', [
            'claude:session',
            'claude:weekly',
            'claude:weekly:fable',
            'codex:session',
        ]);
        await waitOrFail(
            () =>
                statusCells().some(
                    actor =>
                        accessibleName(actor)?.startsWith('Claude:') &&
                        instance(actor.get_children()[1], St.Widget).has_style_class_name('tmb-panel-mini-dense'),
                ),
            'Three Claude limits did not use dense mini bars',
        );
        await capture('menubar-mini-bars-dense');
        await setSetting('selected_windows', ['copilot:premium_interactions']);
        await waitOrFail(
            () =>
                statusCells().length === 1 &&
                accessibleName(required(statusCells()[0]))?.startsWith('GitHub Copilot:') &&
                findActor(panelCells, actor => actor.has_style_class_name?.('tmb-panel-mini-fill'))?.width ===
                    Math.max(2, Math.round((30 * Math.clamp(required(serviceState().cells[0]).used, 0, 100)) / 100)),
            'Copilot usage did not fill its grouped mini bar proportionally',
        );
        await setSetting('selected_windows', ['claude:session', 'claude:weekly', 'codex:session']);
        clickButton(required(findActor(page, actor => actor.accessible_name === 'format: exhausted')));
        await waitOrFail(
            () =>
                serviceSettings().format === 'exhausted' &&
                indicator.accessible_name.includes('CX 5h') &&
                serviceState().cells.every(cell => cell.text.includes('\n')) &&
                statusCells().every(actor => instance(actor, St.Widget).has_style_class_name('tmb-panel-cell-stacked')),
            'At 100% format did not reach the panel',
        );
        await capture('menubar-at-100');
        clickButton(required(findActor(page, actor => actor.accessible_name === 'format: custom')));
        await waitOrFail(
            () =>
                serviceSettings().format === 'custom' &&
                findActor(page, actor => actor instanceof St.Entry && actor.accessible_name === 'Template'),
            'Custom menu bar template editor is missing',
        );
        await waitOrFail(
            () =>
                required(
                    findActor(page, actor => actor instanceof St.Entry && actor.accessible_name === 'Template')?.width,
                ) >
                root.width / 2,
            'Custom template editor leaves usable row width empty',
        );
        const template = required(
            findActor(page, actor => actor instanceof St.Entry && actor.accessible_name === 'Template'),
        );
        instance(template, St.Entry).set_text('{label}-{pct}');
        clickButton(required(findActor(template.get_parent(), actor => buttonLabel(actor) === 'Save')));
        await waitOrFail(
            () => serviceSettings().template === '{label}-{pct}' && /TEST12-\d+%/.test(indicator.accessible_name),
            'Custom menu bar template did not reach the panel',
        );
        await capture('menubar-custom-template');
        instance(
            required(findActor(page, actor => actor instanceof St.Entry && actor.accessible_name === 'Template')),
            St.Entry,
        ).set_text('{label}/{pct}');
        instance(
            required(findActor(page, actor => actor instanceof St.Entry && actor.accessible_name === 'Template')),
            St.Entry,
        ).clutter_text.emit('activate');
        await waitOrFail(
            () => serviceSettings().template === '{label}/{pct}' && /TEST12\/\d+%/.test(indicator.accessible_name),
            'Pressing Enter did not save the custom menu bar template',
        );
        await setSetting('template', '{pct}');
        await waitOrFail(
            () =>
                /^\d+%/.test(indicator.accessible_name) &&
                required(statusCells()[0]).get_children().length === 1 &&
                instance(required(statusCells()[0]).get_children()[0], St.Widget).has_style_class_name(
                    'tmb-panel-value',
                ),
            'A percentage-only template did not render as one colored value',
        );
        await setSetting('template', '{cell}|{provider}|{providerName}|{window}');
        await waitOrFail(
            () =>
                indicator.accessible_name.includes('CC 5h|CC|Claude|5h') &&
                indicator.accessible_name.includes('CX|CX|Codex|5h'),
            'Template tokens did not match the macOS panel labels',
        );
        await capture('menubar-template-tokens');
        const control = required(findActor(page, actor => actor.accessible_name === 'Show all providers'));
        if (
            !optionalInstance(control, St.Button)?.toggle_mode ||
            control.get_accessible().ref_state_set().contains_state(Atk.StateType.CHECKED)
        )
            throw new Error('Provider setup switch is missing');
        clickButton(control);
        await waitOrFail(
            () =>
                findActor(
                    contentOf(root.get_children()[1]),
                    actor =>
                        actor.accessible_name === 'Show all providers' &&
                        checkMark(actor) === '☑' &&
                        actor.get_accessible().ref_state_set().contains_state(Atk.StateType.CHECKED),
                ),
            'Provider setup switch did not save',
        );
        const settingsPage = contentOf(root.get_children()[1]);
        const reset = findActor(settingsPage, actor => buttonLabel(actor) === 'Reset all settings…');
        if (!reset) throw new Error('Reset settings control is missing');
        clickButton(reset);
        if (instance(reset, St.Button).get_label() !== 'Confirm reset all settings')
            throw new Error('Reset settings confirmation is missing');
        clickButton(reset);
        await waitOrFail(
            () =>
                findActor(
                    contentOf(root.get_children()[1]),
                    actor => actor.accessible_name === 'Show all providers' && checkMark(actor) === '☐',
                ),
            'Reset did not restore provider setup settings',
        );
        clickButton(
            required(
                findActor(root, actor => actor.accessible_name === 'Usage' && actor.has_style_class_name?.('tmb-tab')),
            ),
        );
        await waitOrFail(
            () =>
                readProvider('claude').status === 'unreadable' &&
                containsText(contentOf(root.get_children()[1]), 'No usage available') &&
                containsText(contentOf(root.get_children()[1]), 'Set up providers under Settings > Providers.') &&
                !findActor(contentOf(root.get_children()[1]), actor => actor.has_style_class_name?.('tmb-card')),
            'Unreadable credentials produced an empty Usage card',
        );
        if (root.width > 640) throw new Error('Empty Usage leaves unused horizontal space');
        await capture('first-run');
        await waitOrFail(
            () => findActor(contentOf(root.get_children()[1]), actor => buttonLabel(actor) === 'Open Providers'),
            `First-run provider action did not render: ${JSON.stringify({
                status: readProvider('claude').status,
                empty: containsText(contentOf(root.get_children()[1]), 'No usage available'),
                buttons: findActors(contentOf(root.get_children()[1]), actor => buttonLabel(actor)).map(actor =>
                    instance(actor, St.Button).get_label(),
                ),
            })}`,
        );
        clickButton(
            required(findActor(contentOf(root.get_children()[1]), actor => buttonLabel(actor) === 'Open Providers')),
        );
        await waitOrFail(
            () => containsText(contentOf(root.get_children()[1]), 'MENU BAR'),
            'First-run provider action did not open Settings',
        );
        let logSignals = 0;
        const logSubscription = Gio.DBus.session.signal_subscribe(
            'tech.bernat.Quotaline.Gnome',
            'tech.bernat.Quotaline.Gnome',
            'LogChanged',
            '/tech/bernat/Quotaline/Gnome',
            null,
            Gio.DBusSignalFlags.NONE,
            () => logSignals++,
        );
        await setSetting('detailed_logging', true);
        await waitOrFail(
            () => serviceState().settings.detailed_logging,
            'Detailed logging setting did not reach the Shell',
        );
        clickButton(
            required(
                findActor(root, actor => actor.accessible_name === 'Usage' && actor.has_style_class_name?.('tmb-tab')),
            ),
        );
        await waitOrFail(
            () => serviceLogs().some(entry => entry.message.includes('tab=Usage')),
            'The Shell did not record a tab diagnostic',
        );
        clickButton(
            required(
                findActor(
                    root,
                    actor => actor.accessible_name === 'Settings' && actor.has_style_class_name?.('tmb-tab'),
                ),
            ),
        );
        const beforeRefresh = logSignals;
        Gio.DBus.session.call_sync(
            'tech.bernat.Quotaline.Gnome',
            '/tech/bernat/Quotaline/Gnome',
            'tech.bernat.Quotaline.Gnome',
            'Refresh',
            new GLib.Variant('(s)', ['claude']),
            new GLib.VariantType('(b)'),
            Gio.DBusCallFlags.NONE,
            2000,
            null,
        );
        await waitOrFail(() => logSignals > beforeRefresh, 'Detailed logging did not emit a log signal');
        await waitOrFail(
            () => containsText(contentOf(root.get_children()[1]), 'claude refresh completed in'),
            'Live log signal did not update Settings',
        );
        if (
            !serviceLogs().some(
                entry => entry.level === 'debug' && entry.message.includes('claude refresh completed in'),
            )
        )
            throw new Error('Detailed logging did not record refresh timing');
        Gio.DBus.session.signal_unsubscribe(logSubscription);
        for (let index = 0; index < 36; index++) {
            const accepted = (
                Gio.DBus.session
                    .call_sync(
                        'tech.bernat.Quotaline.Gnome',
                        '/tech/bernat/Quotaline/Gnome',
                        'tech.bernat.Quotaline.Gnome',
                        'RecordShellDiagnostic',
                        new GLib.Variant('(ss)', ['tabs', `${index}:${'x'.repeat(240)}`]),
                        new GLib.VariantType('(b)'),
                        Gio.DBusCallFlags.NONE,
                        2000,
                        null,
                    )
                    .deep_unpack() as [boolean]
            )[0];
            if (!accepted) throw new Error('Detailed diagnostics rejected a valid Shell event');
        }
        const fullReport = (
            Gio.DBus.session
                .call_sync(
                    'tech.bernat.Quotaline.Gnome',
                    '/tech/bernat/Quotaline/Gnome',
                    'tech.bernat.Quotaline.Gnome',
                    'GetDiagnostics',
                    null,
                    new GLib.VariantType('(s)'),
                    Gio.DBusCallFlags.NONE,
                    2000,
                    null,
                )
                .deep_unpack() as [string]
        )[0];
        if (fullReport.length <= 8000) throw new Error('The issue URL limit fixture is too small');
        let issueURL = '';
        const launchIssue = Gio.AppInfo.launch_default_for_uri.bind(Gio.AppInfo);
        Gio.AppInfo.launch_default_for_uri = uri => {
            issueURL = uri;
            global.stage.queue_redraw();
            return true;
        };
        try {
            clickButton(required(findActor(footer, actor => buttonLabel(actor) === 'Report issue')));
            await waitOrFail(() => issueURL !== '', 'Report issue did not open a long diagnostic draft');
        } finally {
            Gio.AppInfo.launch_default_for_uri = launchIssue;
        }
        if (
            issueURL.length > 8000 ||
            !decodeURIComponent(issueURL).includes('"providers"') ||
            decodeURIComponent(issueURL).includes(serviceState().history_path)
        )
            throw new Error('Report issue did not bound and redact the diagnostic draft');
        await setSetting('detailed_logging', false);
    }
}
