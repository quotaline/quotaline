import assert from 'node:assert/strict';
import {test} from 'node:test';
import {moduleUrl} from './support/node.ts';

const {noAnimation} = (await import(moduleUrl('popup-animation.js'))) as typeof import('../src/popup-animation.js');
await test('popup menus skip animation with the argument each Shell version reads', () => {
    assert.equal(noAnimation('50.4', 0), 0);
    assert.deepEqual(noAnimation('51.1', 0), {animate: false});
    assert.deepEqual(noAnimation('52.alpha', 0), {animate: false});
});
