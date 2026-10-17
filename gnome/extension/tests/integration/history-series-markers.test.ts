import Clutter from 'gi://Clutter';
import GdkPixbuf from 'gi://GdkPixbuf';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import St from 'gi://St';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import type {HistorySeries} from '../../src/service-state.js';
import type {TestActor} from '../support/actors.js';
import {failure, required} from '../support/actors.js';
import {afterPaint} from '../support/shell-events.js';

export async function run() {
    let probe: St.BoxLayout | undefined;
    try {
        const source = Main.extensionManager.lookup('quotaline@bernat.tech').dir.get_uri();
        const {createChart, createLegendSwatch} = (await import(
            `${source}/src/chart.js`
        )) as typeof import('../../src/chart.js');
        const square = createLegendSwatch(8, 'usage');
        const diamond = createLegendSwatch(16, 'usage');
        const swatches = new St.BoxLayout({style: 'spacing: 28px;'});
        swatches.add_child(square);
        swatches.add_child(diamond);
        const series: HistorySeries[] = Array.from({length: 17}, (_, index) => ({
            id: `series-${index}`,
            provider: 'test',
            label: `Series ${index}`,
            points:
                index === 16
                    ? [
                          [0, 95, null, false],
                          [50, 90, null, false],
                          [100, 80, null, false],
                      ]
                    : [
                          [0, 5 + index, null, false],
                          [50, 6 + index, null, false],
                          [100, 7 + index, null, false],
                      ],
        }));
        const chart = createChart(series, 0, 100, new Set(series.map(item => item.id)));
        probe = new St.BoxLayout({
            orientation: Clutter.Orientation.VERTICAL,
            x: 40,
            y: 80,
            style: 'background-color: #242424; padding: 16px; spacing: 12px;',
        });
        probe.add_child(swatches);
        probe.add_child(chart);
        Main.layoutManager.uiGroup.add_child(probe);
        await afterPaint();
        if (!chart.accessible_name.includes('17 series; Y-axis 0% to 100%'))
            throw new Error('A dense History chart lost its visible series or percentage scale');

        const directory = GLib.getenv('TMB_SCREENSHOT_DIR');
        GLib.mkdir_with_parents(required(directory), 0o755);
        const path = GLib.build_filenamev([required(directory), 'history-series-markers.png']);
        const stream = Gio.File.new_for_path(path).replace(null, false, Gio.FileCreateFlags.REPLACE_DESTINATION, null);
        const [left, top] = probe.get_transformed_position();
        await new Promise<void>((resolve, reject) => {
            new Shell.Screenshot().screenshot_area(
                left,
                top,
                required(probe).width,
                required(probe).height,
                stream,
                (source, result) => {
                    try {
                        required(source).screenshot_area_finish(result);
                        stream.close(null);
                        resolve();
                    } catch (error) {
                        reject(failure(error));
                    }
                },
            );
        });
        const image = GdkPixbuf.Pixbuf.new_from_file(path);
        const pixels = image.get_pixels();
        const channels = image.get_n_channels();
        const blue = (x: number, y: number) => {
            const offset = y * image.get_rowstride() + x * channels;
            return (
                required(pixels[offset + 2]) > required(pixels[offset]) + 45 &&
                required(pixels[offset + 2]) > required(pixels[offset + 1]) + 25
            );
        };
        const count = (actor: TestActor, y: number, start: number, end: number) => {
            const [x, topY] = actor.get_transformed_position();
            return Array.from({length: end - start + 1}, (_, index) =>
                blue(Math.round(x - left) + start + index, Math.round(topY - top) + y),
            ).filter(Boolean).length;
        };
        const squareWidth = count(square, 4, 4, 13);
        const diamondWidth = count(diamond, 4, 4, 13);
        if (squareWidth < 4 || diamondWidth < 1 || squareWidth <= diamondWidth)
            throw new Error(`Legend markers are indistinguishable: square ${squareWidth}, diamond ${diamondWidth}`);
        const [chartX, chartY] = chart.get_transformed_position();
        let markerPixels = 0;
        for (let y = 30; y <= 37; y++)
            for (let x = 329; x <= 335; x++)
                markerPixels += Number(blue(Math.round(chartX - left) + x, Math.round(chartY - top) + y));
        if (markerPixels < 3)
            throw new Error(`The seventeenth chart series did not draw its diamond marker: ${markerPixels}`);
    } finally {
        probe?.destroy();
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
