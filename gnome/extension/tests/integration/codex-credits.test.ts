import GLib from 'gi://GLib';
import St from 'gi://St';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import type {TestActor} from '../support/actors.js';
import {
    buttonLabel,
    clickButton,
    containsText,
    findActor,
    indicator as getIndicator,
    instance,
    required,
} from '../support/actors.js';
import {waitOrFail} from '../support/shell-events.js';

export async function run() {
    try {
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        indicator.menu.open(false);
        const section = () =>
            findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-credit-section'));
        await waitOrFail(() => section(), 'Codex credits did not reach Usage');
        const creditTile = () =>
            findActor(
                section(),
                actor => actor.has_style_class_name?.('tmb-credit-tile') && actor.get_children()[0]?.text === 'Credits',
            );
        const disclosure = (name: string) => findActor(section(), actor => buttonLabel(actor)?.includes(name));
        switch (required(GLib.getenv('TMB_CREDIT_CASE'))) {
            case 'unlimited':
                if (
                    !containsText(creditTile(), 'Unlimited') ||
                    !containsText(section(), 'Limit reached') ||
                    containsText(section(), 'Limit resets') ||
                    disclosure('Credit estimates and totals') ||
                    disclosure('Reset credit details')
                )
                    throw new Error('Unlimited usage credits and overage state are wrong');
                break;
            case 'missing': {
                if (
                    !containsText(creditTile(), 'Balance not reported') ||
                    !containsText(section(), '1 available') ||
                    containsText(section(), 'Next reset-credit expiry') ||
                    containsText(section(), 'Reset purchase') ||
                    disclosure('Credit estimates and totals')
                )
                    throw new Error('Missing balance or reset summary is wrong');
                clickButton(required(disclosure('Reset credit details')));
                await waitOrFail(() => containsText(section(), 'Expires soon'), 'Textual reset expiry is missing');
                const row = () =>
                    findActor(
                        section(),
                        actor => actor.has_style_class_name?.('tmb-key-value') && containsText(actor, 'Near reset'),
                    );
                let armed: TestActor | null = null;
                await waitOrFail(() => {
                    if (row() && row() !== armed) {
                        armed = row();
                        instance(required(armed), St.Widget).set_hover(false);
                        instance(required(armed), St.Widget).set_hover(true);
                    }
                    const tooltip = findActor(
                        Main.layoutManager.uiGroup,
                        actor => actor.has_style_class_name?.('tmb-tooltip') && actor.visible,
                    );
                    return (
                        tooltip &&
                        containsText(tooltip, 'Reading this information never reserves or consumes the credit.')
                    );
                }, 'Reset credit explanation did not appear on hover');
                instance(armed, St.Widget).set_hover(false);
                clickButton(required(disclosure('Reset credit details')));
                if (containsText(section(), 'Expires soon')) throw new Error('Reset credit details did not collapse');
                break;
            }
            case 'reset-only':
                if (creditTile() || !containsText(section(), '2 available') || disclosure('Reset credit details'))
                    throw new Error('Reset credits should appear without usage credits');
                clickButton(required(disclosure('Credit estimates and totals')));
                await waitOrFail(
                    () => containsText(section(), 'Resets earned') && containsText(section(), '3'),
                    'Reset credit total is missing',
                );
                clickButton(required(disclosure('Credit estimates and totals')));
                if (containsText(section(), 'Resets earned')) throw new Error('Reset credit totals did not collapse');
                break;
            default:
                throw new Error('Unknown Codex credit case');
        }
        indicator.menu.close(false);
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
