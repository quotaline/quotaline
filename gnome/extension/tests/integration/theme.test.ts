import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import St from 'gi://St';
import * as BoxPointer from 'resource:///org/gnome/shell/ui/boxpointer.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {QuickMenuToggle} from 'resource:///org/gnome/shell/ui/quickSettings.js';
import type {Indicator} from '../../src/panel.js';
import {
    clickButton,
    contentOf,
    failure,
    findActor,
    indicator as getIndicator,
    instance,
    optionalInstance,
    required,
} from '../support/actors.js';
import {waitOrFail} from '../support/shell-events.js';

function luminance(color: readonly number[]) {
    return color.reduce((sum, value, index) => {
        const channel = value > 1 ? value / 255 : value;
        return (
            sum +
            required([0.2126, 0.7152, 0.0722][index]) *
                (channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4)
        );
    }, 0);
}

function widthSettled(indicator: Indicator, cap: number) {
    if (!indicator.has_allocation()) return false;
    const root = required(findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-root')));
    const monitor = required(Main.layoutManager.primaryMonitor);
    const [menuX] = root.get_transformed_position();
    const [sourceX] = indicator.get_transformed_position();
    const menuCenter = menuX + root.width / 2;
    const closestCenter = Math.clamp(
        sourceX + indicator.width / 2,
        monitor.x + root.width / 2,
        monitor.x + monitor.width - root.width / 2,
    );
    return (
        root.width >= Math.min(440, monitor.width - 32) - 3 &&
        root.width <= Math.min(cap, monitor.width - 32) + 3 &&
        menuX >= monitor.x &&
        menuX + root.width <= monitor.x + monitor.width &&
        Math.abs(menuCenter - closestCenter) <= 25
    );
}

function widthDetails(indicator: Indicator, cap: number) {
    const root = required(findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-root')));
    const children = root.get_children().map(child => [child.width, child.get_preferred_width(-1)]);
    const page = contentOf(required(root.get_children()[1]));
    const sections = page
        .get_children()
        .map(child => [instance(child, St.Widget).get_style_class_name(), child.width, child.get_preferred_width(-1)]);
    return (
        `${root.width}px, cap ${cap}px, ` + `children ${JSON.stringify(children)}, sections ${JSON.stringify(sections)}`
    );
}

async function capture(name: string) {
    const screenshots = GLib.getenv('TMB_SCREENSHOT_DIR');
    if (!screenshots) return;
    GLib.mkdir_with_parents(screenshots, 0o755);
    const stream = Gio.File.new_for_path(GLib.build_filenamev([screenshots, name])).replace(
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

function waitFor(check: () => unknown, description: string | (() => string) = 'GNOME theme') {
    const caller = new Error().stack;
    return waitOrFail(
        check,
        () => `${typeof description === 'function' ? description() : description} did not update; ${caller}`,
    );
}

export async function run() {
    const interfaceSettings = new Gio.Settings({schema_id: 'org.gnome.desktop.interface'});
    const accessibilitySettings = new Gio.Settings({schema_id: 'org.gnome.desktop.a11y.interface'});
    const previousScheme = interfaceSettings.get_string('color-scheme');
    const previousContrast = accessibilitySettings.get_boolean('high-contrast');
    try {
        Main.overview.hide();
        const mode = GLib.getenv('TMB_THEME_MODE');
        accessibilitySettings.set_boolean('high-contrast', mode === 'high-contrast');
        interfaceSettings.set_string('color-scheme', mode === 'light' ? 'prefer-light' : 'prefer-dark');
        if (mode === 'light') {
            await waitFor(() => St.Settings.get().color_scheme === St.SystemColorScheme.PREFER_LIGHT);
        } else if (mode === 'high-contrast') {
            await waitFor(
                () =>
                    St.Settings.get().high_contrast &&
                    St.Settings.get().color_scheme === St.SystemColorScheme.PREFER_DARK,
                () =>
                    `High contrast ${St.Settings.get().high_contrast} and color scheme ${St.Settings.get().color_scheme}`,
            );
        } else {
            await waitFor(() => St.Settings.get().color_scheme === St.SystemColorScheme.PREFER_DARK);
        }
        const indicator = getIndicator();
        if (!indicator) throw new Error('Quotaline indicator is missing');
        indicator.menu.open(false);
        await waitFor(
            () =>
                findActor(
                    indicator.menu.box,
                    actor =>
                        actor.has_style_class_name?.('tmb-root') &&
                        actor.has_style_class_name('tmb-light') === (mode === 'light'),
                ),
            () =>
                `Menu theme for ${mode}; Shell scheme ${St.Settings.get().color_scheme}, contrast ${St.Settings.get().high_contrast}`,
        );
        clickButton(
            required(
                findActor(
                    indicator.menu.box,
                    actor => actor.accessible_name === 'Usage' && actor.has_style_class_name?.('tmb-tab'),
                ),
            ),
        );
        await waitFor(() => findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-card')));
        await waitFor(() => widthSettled(indicator, 880), 'Usage width');
        await waitFor(() =>
            optionalInstance(
                findActor(indicator, actor => actor.has_style_class_name?.('tmb-panel-value')),
                St.Widget,
            )
                ?.get_style()
                ?.match(/#[0-9a-f]{6}/i),
        );
        const value = required(findActor(indicator, actor => actor.has_style_class_name?.('tmb-panel-value')));
        const color = required(
            required(required(required(instance(value, St.Widget).get_style()).match(/#([0-9a-f]{6})/i))[1]).match(
                /../g,
            ),
        ).map(channel => parseInt(channel, 16));
        const background = Main.panel.get_theme_node().get_background_color();
        const [lighter, darker] = [
            luminance([background.red, background.green, background.blue]),
            luminance(color),
        ].sort((left, right) => right - left);
        const contrast = (required(lighter) + 0.05) / (required(darker) + 0.05);
        if (contrast < 4.5) throw new Error(`${mode} panel value contrast is ${contrast.toFixed(2)}:1`);
        if (mode === 'light') {
            const selected = required(
                findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-tab-selected')),
            );
            const foreground = instance(selected, St.Widget).get_theme_node().get_foreground_color();
            if (luminance([foreground.red, foreground.green, foreground.blue]) > 0.18)
                throw new Error('Selected tab text stayed light on a light background');
        }
        Main.overview.hide();
        await waitFor(() => !Main.overview.visible);
        await capture(`usage-${mode}.png`);
        clickButton(required(findActor(indicator.menu.box, actor => actor.accessible_name === 'Settings')));
        await waitFor(() =>
            findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-setting-segments')),
        );
        await waitFor(() => widthSettled(indicator, 880), 'Settings width');
        if (mode === 'light') {
            const selected = required(
                findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-setting-segment-selected')),
            );
            const foreground = instance(selected, St.Widget).get_theme_node().get_foreground_color();
            if (luminance([foreground.red, foreground.green, foreground.blue]) > 0.18)
                throw new Error('Selected Settings text stayed light on a light background');
            const preview = findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-status-preview'));
            const previewValue = required(findActor(preview, actor => actor.has_style_class_name?.('tmb-panel-value')));
            const previewColor = required(
                required(
                    required(required(instance(previewValue, St.Widget).get_style()).match(/#([0-9a-f]{6})/i))[1],
                ).match(/../g),
            ).map(channel => parseInt(channel, 16));
            const previewContrast = (luminance([218, 218, 219]) + 0.05) / (luminance(previewColor) + 0.05);
            if (previewContrast < 4.5)
                throw new Error(`Light status preview contrast is ${previewContrast.toFixed(2)}:1`);
        }
        await capture(`settings-${mode}.png`);
        if (mode === 'light') {
            const settings = new Gio.Settings({schema_id: 'org.gnome.desktop.interface'});
            const root = required(findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-root')));
            const lightStyle = instance(
                required(findActor(indicator, actor => actor.has_style_class_name?.('tmb-panel-value'))),
                St.Widget,
            ).get_style();
            settings.set_string('color-scheme', 'prefer-dark');
            await waitFor(
                () =>
                    !instance(root, St.Widget).has_style_class_name('tmb-light') &&
                    optionalInstance(
                        findActor(indicator, actor => actor.has_style_class_name?.('tmb-panel-value')),
                        St.Widget,
                    )?.get_style() !== lightStyle,
            );
            settings.set_string('color-scheme', 'prefer-light');
            await waitFor(
                () =>
                    instance(root, St.Widget).has_style_class_name('tmb-light') &&
                    optionalInstance(
                        findActor(indicator, actor => actor.has_style_class_name?.('tmb-panel-value')),
                        St.Widget,
                    )?.get_style() === lightStyle,
            );
        }
        clickButton(required(findActor(indicator.menu.box, actor => actor.accessible_name === 'History')));
        await waitFor(
            () =>
                findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-chart')) &&
                findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-legend-toggle')),
        );
        await waitFor(
            () => widthSettled(indicator, 880),
            () => `History width ${widthDetails(indicator, 880)}`,
        );
        await waitFor(() => {
            const chart = required(findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-chart')));
            const legend = required(
                findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-history-legend')),
            );
            const root = required(findActor(indicator.menu.box, actor => actor.has_style_class_name?.('tmb-root')));
            return (
                chart.has_allocation() &&
                legend.has_allocation() &&
                (root.width >= 875
                    ? chart.width >= 580 &&
                      legend.get_transformed_position()[0] >= chart.get_transformed_position()[0] + chart.width
                    : chart.width >= root.width - 50 &&
                      legend.get_transformed_position()[1] >= chart.get_transformed_position()[1] + chart.height)
            );
        }, 'History layout did not fit the menu width');
        await capture(`history-${mode}.png`);
        indicator.menu.close(false);
        const quick = required(Main.panel.statusArea.quickSettings);
        quick.menu.open(BoxPointer.PopupAnimation.NONE);
        const widget = required(
            findActor(quick.menu.box, actor => actor.title === 'Quotaline' && actor.menu, QuickMenuToggle),
        );
        widget.menu.open(BoxPointer.PopupAnimation.NONE);
        await waitFor(
            () =>
                widget.menu.box.opacity === 255 &&
                findActor(widget.menu.box, actor => actor.has_style_class_name?.('tmb-widget-value')),
            'Snapshot value',
        );
        const valueLabel = required(
            findActor(widget.menu.box, actor => actor.has_style_class_name?.('tmb-widget-value')),
        );
        const foreground = required(
            required(required(required(instance(valueLabel, St.Widget).get_style()).match(/#([0-9a-f]{6})/i))[1]).match(
                /../g,
            ),
        ).map(channel => parseInt(channel, 16));
        const surface = widget.menu.box.get_theme_node().get_background_color();
        const [higher, lower] = [luminance(foreground), luminance([surface.red, surface.green, surface.blue])].sort(
            (left, right) => right - left,
        );
        if ((required(higher) + 0.05) / (required(lower) + 0.05) < 4.5)
            throw new Error(`${mode} snapshot value has low contrast on ${surface.to_string()}`);
        quick.menu.close(BoxPointer.PopupAnimation.NONE);
    } finally {
        accessibilitySettings.set_boolean('high-contrast', previousContrast);
        interfaceSettings.set_string('color-scheme', previousScheme);
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
