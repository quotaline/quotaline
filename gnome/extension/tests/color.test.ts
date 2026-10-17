import assert from 'node:assert/strict';
import {test} from 'node:test';
import {moduleUrl} from './support/node.ts';

const {usageColor, usageStatusColor, usageTextColor} = (await import(
    moduleUrl('color.js')
)) as typeof import('../src/color.js');
function luminance(color: string): number {
    const linear = (offset: number) => {
        const value = parseInt(color.slice(offset, offset + 2), 16) / 255;
        return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
    };
    return 0.2126 * linear(1) + 0.7152 * linear(3) + 0.0722 * linear(5);
}
await test('usage meter palette follows percentage and pace', () => {
    for (const [percent, color] of [
        [0, '#258540'],
        [30, '#40a32b'],
        [60, '#ff9838'],
        [100, '#ff8873'],
    ] as const)
        assert.equal(usageColor(percent), color);
    assert.equal(usageColor(-5), usageColor(0));
    assert.equal(usageColor(105), usageColor(100));
    assert.equal(usageColor(20, 'ahead'), usageColor(60));
    assert.equal(usageColor(20, 'exhausted'), usageColor(100));
});
await test('status text palette follows percentage and pace', () => {
    for (const [percent, color] of [
        [0, '#4dff7e'],
        [60, '#ff9838'],
        [100, '#ff8873'],
    ] as const)
        assert.equal(usageStatusColor(percent), color);
    assert.equal(usageStatusColor(20, 'ahead'), usageStatusColor(60));
    assert.equal(usageStatusColor(20, 'exhausted'), usageStatusColor(100));
});
await test('high usage meters meet dark tile contrast', () => {
    const darkTile = luminance('#3b3b3e');
    for (let percent = 60; percent <= 100; percent++)
        assert.ok((luminance(usageColor(percent)) + 0.05) / (darkTile + 0.05) >= 4.5, `${percent}%`);
});
await test('status and widget text meet their surface contrast', () => {
    const activePanel = luminance('#303034');
    const silverMenuBar = luminance('#f1f1f1');
    const widget = luminance('#494949');
    const lightPanel = luminance('#e3e3e8');
    const lightWidget = luminance('#bdbdbf');
    for (let percent = 0; percent <= 100; percent++) {
        assert.ok((luminance(usageStatusColor(percent)) + 0.05) / (activePanel + 0.05) >= 4.5, `${percent}% status`);
        assert.ok(
            (silverMenuBar + 0.05) / (luminance(usageStatusColor(percent, null, true)) + 0.05) >= 7,
            `${percent}% light status`,
        );
        assert.ok((luminance(usageTextColor(percent)) + 0.05) / (widget + 0.05) >= 4.5, `${percent}% widget`);
        assert.ok(
            (lightPanel + 0.05) / (luminance(usageTextColor(percent, null, true)) + 0.05) >= 4.5,
            `${percent}% light status`,
        );
        assert.ok(
            (lightWidget + 0.05) / (luminance(usageTextColor(percent, null, true)) + 0.05) >= 4.5,
            `${percent}% light widget`,
        );
    }
});
