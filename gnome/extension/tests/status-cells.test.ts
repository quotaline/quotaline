import assert from 'node:assert/strict';
import {test} from 'node:test';
import {moduleUrl, required} from './support/node.ts';
import * as fixtures from './support/state.ts';
import {provider, providerDefinition, quotaWindow, serviceState, settings} from './support/state.ts';

const {statusCells, windowTag} = (await import(
    moduleUrl('status-cells.js')
)) as typeof import('../src/status-cells.js');
function fixture() {
    return serviceState(
        {
            claude: provider({
                name: 'Claude',
                status: 'connected',
                plan: 'Max',
                credit_balance: 5,
                windows: [
                    quotaWindow({
                        id: 'session',
                        label: 'Current session',
                        used: 25,
                        active: true,
                        reset: null,
                        reset_precision: 'instant',
                    }),
                ],
            }),
            codex: provider({name: 'Codex', status: 'connected', plan: '', credit_balance: null, windows: []}),
        },
        {
            now: 100,
            provider_definitions: [providerDefinition({id: 'claude'}), providerDefinition({id: 'codex'})],
            status_entries: [
                {
                    provider: 'claude',
                    provider_code: 'CC',
                    provider_name: 'Claude',
                    window_id: 'session',
                    window_label: 'Current session',
                    window_scope: null,
                    short_label: 'CC 5h',
                    used: 25,
                    reset_at: null,
                    active: true,
                    restricted: false,
                    restrictions: [],
                },
            ],
            template_tokens: [],
            settings: settings({
                providers: {claude: true, codex: true},
                provider_order: [],
                model_order: [],
                hide_zero: false,
                show_as: 'used',
                decimals: 0,
                format: 'stacked',
                order: 'provider',
            }),
        },
    );
}
for (const [id, scope, tag] of [
    ['session', null, '5h'],
    ['weekly', null, '7d'],
    ['monthly', null, '1mo'],
    ['weekly:model', 'Spark Model', 'SPA'],
    ['code_review:weekly', null, 'WEE'],
    ['tangelo', 'GPT-5.3 Spark', 'GPT'],
    ['', '', ''],
] as const) {
    await test(`window tag ${id || 'empty'} ${scope ?? 'without scope'}`, () => {
        assert.equal(windowTag(id, scope), tag);
    });
}
await test('status cells use selected quota facts and raw tooltip readings', () => {
    const cells = statusCells(fixture());
    assert.equal(cells.length, 1);
    assert.deepEqual(cells[0], {
        id: 'claude:session',
        label: 'CC 5h',
        text: 'CC 5h\n25%',
        tooltip: 'Claude Current session: 25%, resets —',
        used: 25,
        provider: 'claude',
        restricted: false,
        limited: false,
        stale: false,
    });
});
await test('status cells ignore missing provider and window data', () => {
    const state = fixture();
    state.providers.claude.windows = [];
    assert.deepEqual(statusCells(state), []);
    Reflect.deleteProperty(state.providers, 'claude');
    assert.deepEqual(statusCells(state), []);
});
await test('status cells format inline, mini bars, countdown and remaining values', () => {
    const state = fixture();
    state.settings.format = 'inline';
    state.settings.show_as = 'remaining';
    state.settings.decimals = 1;
    assert.equal(required(statusCells(state)[0]).text, 'CC 5h:75.0%');
    assert.equal(required(statusCells(state)[0]).tooltip, 'Claude Current session: 75% left, resets —');
    state.settings.format = 'mini-bars';
    assert.equal(required(statusCells(state)[0]).text, 'CC 5h ▰▱▱▱▱ 75.0%');
    state.settings.format = 'countdown';
    assert.equal(required(statusCells(state)[0]).text, 'CC 5h 75.0% · --');
    state.settings.format = 'exhausted';
    assert.equal(required(statusCells(state)[0]).text, 'CC 5h\n75.0%');
});
await test('status cells preserve blocking quota text and stale readings', () => {
    const state = fixture();
    state.providers.claude.status = 'stale';
    required(state.status_entries[0]).used = 100;
    required(state.status_entries[0]).restricted = true;
    required(state.status_entries[0]).restrictions = [{label: 'weekly', reset_at: null}];
    state.settings.format = 'exhausted';
    const cell = statusCells(state)[0];
    assert.equal(required(cell).text, 'CC 5h\nLimit');
    assert.equal(
        required(cell).tooltip,
        'Stale, last known values. Claude Current session: 25%, resets —\n' +
            'Included quota limited by weekly (100% used; resets —).',
    );
    assert.equal(required(cell).restricted, true);
    assert.equal(required(cell).limited, true);
    assert.equal(required(cell).stale, true);
    required(state.status_entries[0]).reset_at = 90;
    required(state.status_entries[0]).restricted = false;
    assert.equal(required(statusCells(state)[0]).restricted, false);
    required(state.status_entries[0]).reset_at = 200;
    required(state.status_entries[0]).restricted = true;
    assert.equal(required(statusCells(state)[0]).text, 'CC 5h\n1m');
    required(state.providers.claude.windows[0]).active = false;
    required(state.status_entries[0]).restrictions = [];
    required(state.status_entries[0]).restricted = false;
    assert.equal(required(statusCells(state)[0]).restricted, false);
});
await test('restriction help includes each blocking reset in source order', () => {
    const state = fixture();
    required(state.status_entries[0]).restrictions = [
        {label: 'Session', reset_at: 200},
        {label: 'Weekly', reset_at: null},
    ];
    assert.ok(
        required(statusCells(state)[0]).tooltip.endsWith(
            'Included quota limited by Session (100% used; resets 1 min); Weekly (100% used; resets —).',
        ),
    );
});
await test('status cells preserve projected order and count each provider window', () => {
    const state = fixture();
    state.providers.claude.windows.push(
        fixtures.quotaWindow({
            id: 'weekly',
            label: 'Weekly',
            used: 60,
            active: true,
            reset: null,
            reset_precision: 'instant',
        }),
    );
    state.status_entries.push({
        ...required(state.status_entries[0]),
        window_id: 'weekly',
        window_label: 'Weekly',
        window_scope: null,
        short_label: 'CC 7d',
        used: 60,
    });
    state.providers.codex.windows.push(
        fixtures.quotaWindow({
            id: 'session',
            label: 'Codex session',
            used: 40,
            active: true,
            reset: null,
            reset_precision: 'instant',
        }),
    );
    state.status_entries.push({
        ...required(state.status_entries[0]),
        provider: 'codex',
        provider_code: 'CX',
        provider_name: 'Codex',
        short_label: 'CX',
        used: 40,
    });
    state.status_entries = [
        required(state.status_entries[1]),
        required(state.status_entries[2]),
        required(state.status_entries[0]),
    ];
    assert.deepEqual(
        statusCells(state).map(cell => cell.id),
        ['claude:weekly', 'codex:session', 'claude:session'],
    );
    state.settings.format = 'custom';
    state.template_tokens = [{Placeholder: 'cell'}];
    assert.deepEqual(
        statusCells(state).map(cell => cell.text),
        ['CC 7d', 'CX', 'CC 5h'],
    );
});
await test('custom template renders tokens, escapes, unknowns and limits', () => {
    const state = fixture();
    state.settings.format = 'custom';
    state.template_tokens = [
        {Placeholder: 'cell'},
        {Text: '|'},
        {Placeholder: 'provider'},
        {Text: '|'},
        {Placeholder: 'providerName'},
        {Text: '|'},
        {Placeholder: 'window'},
        {Text: '|'},
        {Placeholder: 'label'},
        {Text: '|'},
        {Placeholder: 'pct'},
        {Text: '|'},
        {Placeholder: 'pct0'},
        {Text: '|'},
        {Placeholder: 'pct1'},
        {Text: '|'},
        {Placeholder: 'pct2'},
        {Text: '|'},
        {Placeholder: 'remaining'},
        {Text: '|'},
        {Placeholder: 'pctOrReset'},
        {Text: '|'},
        {Placeholder: 'reset'},
        {Text: '|'},
        {Placeholder: 'resetClock'},
        {Text: '|'},
        {Placeholder: 'plan'},
        {Text: '|'},
        {Placeholder: 'credits'},
        {Text: '|'},
        {Placeholder: 'unknown'},
        'Newline',
        {Text: 'end'},
    ];
    assert.equal(
        required(statusCells(state)[0]).text,
        'CC|CC|Claude|5h|CC 5h|25%|25%|25.0%|25.00%|75%|25%|--|—|Max|5.00|\nend',
    );
    required(state.status_entries[0]).used = 100;
    required(state.status_entries[0]).restrictions = [{label: 'Weekly limit', reset_at: 200}];
    assert.ok(required(statusCells(state)[0]).text.includes('|Limit|Limit|Limit|Limit|Limit|Limit|'));
    state.template_tokens = [{Placeholder: 'unknown'}];
    assert.equal(required(statusCells(state)[0]).text, ' Limit');
});
await test('status cells use reset deadlines and reject malformed raw reset timestamps', () => {
    const state = fixture();
    required(state.providers.claude.windows[0]).reset = 'invalid';
    assert.ok(required(statusCells(state)[0]).tooltip.endsWith('resets —'));
    required(state.providers.claude.windows[0]).reset = '1970-01-01T00:03:20Z';
    required(state.status_entries[0]).reset_at = 200;
    state.settings.format = 'countdown';
    assert.equal(required(statusCells(state)[0]).text, 'CC 5h 25% · 1m');
    assert.ok(required(statusCells(state)[0]).tooltip.endsWith('resets 1 min'));
    required(state.status_entries[0]).reset_at = 100;
    assert.equal(required(statusCells(state)[0]).text, 'CC 5h 25% · 0m');
    required(state.status_entries[0]).reset_at = 130;
    assert.ok(required(statusCells(state)[0]).text.endsWith('0m'));
    required(state.status_entries[0]).reset_at = 15940;
    assert.ok(required(statusCells(state)[0]).text.endsWith('4h24m'));
    required(state.status_entries[0]).reset_at = 274000;
    assert.ok(required(statusCells(state)[0]).text.endsWith('3d4h'));
    required(state.providers.claude.windows[0]).reset = '1970-01-01T00:02:10Z';
    assert.ok(required(statusCells(state)[0]).tooltip.endsWith('resets < 1 min'));
});
await test('custom reset clocks retain day and elapsed deadline wording', () => {
    const state = fixture();
    state.settings.format = 'custom';
    state.template_tokens = [{Placeholder: 'resetClock'}];
    required(state.status_entries[0]).reset_at = 100;
    assert.equal(required(statusCells(state)[0]).text, 'now');
    required(state.status_entries[0]).reset_at = 200;
    required(state.providers.claude.windows[0]).reset_precision = 'day';
    assert.equal(required(statusCells(state)[0]).text, 'Jan 1');
    required(state.providers.claude.windows[0]).reset_precision = 'instant';
    const clock = (at: number) =>
        new Date(at * 1000).toLocaleTimeString('en-GB', {hour: '2-digit', minute: '2-digit', hour12: false});
    assert.equal(required(statusCells(state)[0]).text, clock(200));
    const nearby = 100 + 3 * 86400;
    required(state.status_entries[0]).reset_at = nearby;
    assert.equal(
        required(statusCells(state)[0]).text,
        `${new Date(nearby * 1000).toLocaleDateString('en-US', {weekday: 'short'})} ${clock(nearby)}`,
    );
    const distant = 100 + 10 * 86400;
    required(state.status_entries[0]).reset_at = distant;
    assert.equal(
        required(statusCells(state)[0]).text,
        `${new Date(distant * 1000).toLocaleDateString('en-US', {month: 'short', day: 'numeric'})} ${clock(distant)}`,
    );
});
await test('custom limited text remains visible around blank and multi-line templates', () => {
    const state = fixture();
    state.settings.format = 'custom';
    required(state.status_entries[0]).used = 100;
    required(state.status_entries[0]).restrictions = [{label: 'Weekly limit', reset_at: 200}];
    state.template_tokens = [{Text: 'Name'}];
    assert.equal(required(statusCells(state)[0]).text, 'Name Limit');
    state.template_tokens = ['Newline'];
    assert.equal(required(statusCells(state)[0]).text, ' Limit\n');
    state.template_tokens = [{Text: 'Name'}, 'Newline', {Text: 'More'}];
    assert.equal(required(statusCells(state)[0]).text, 'Name Limit\nMore');
    required(state.status_entries[0]).reset_at = 200;
    state.template_tokens = [{Placeholder: 'pctOrReset'}];
    assert.equal(required(statusCells(state)[0]).text, '1m');
});
