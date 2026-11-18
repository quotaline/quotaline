import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import {fileURLToPath} from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

test('site builds with versioned links and platform guides', context => {
    const {destination, result} = build(context);
    assert.equal(result.status, 0, result.stderr);
    const start = fs.readFileSync(path.join(destination, 'start/index.html'), 'utf8');
    assert.match(start, /\/en\/preview\/start\/connect\//);
    assert.match(start, /gnome-menubar/);
    assert.match(start, /gnome-usage/);
    assert.match(start, /menubar-light/);
    assert.match(start, /popover-usage-light/);
    const menuBar = fs.readFileSync(path.join(destination, 'reference/menu-bar/index.html'), 'utf8');
    assert.match(menuBar, /\/en\/preview\/reference\/settings\/menu-bar\/#custom-templates/);
    const home = fs.readFileSync(path.join(destination, 'index.html'), 'utf8');
    assert.match(home, /Get started<\/a>/);
    assert.doesNotMatch(home, /Build from source<\/a>/);
    assert.match(home, /href=\/en\/preview\/macos\//);
    assert.match(home, /href=\/en\/preview\/gnome\//);
    assert.match(home, /id=hero-tab-all/);
    assert.match(home, /id=hero-tab-macos/);
    assert.match(home, /id=hero-tab-gnome/);
    const navigation = home.match(/<nav class=nav aria-label=Main>(.*?)<\/nav>/s)?.[1];
    assert.ok(navigation);
    assert.equal((navigation.match(/<a /g) ?? []).length, (navigation.match(/<svg /g) ?? []).length);
    for (const provider of ['claude', 'codex', 'gemini', 'antigravity', 'cursor', 'copilot']) {
        assert.match(home, new RegExp(`/en/preview/reference/providers/${provider}/`));
        assert.ok(fs.statSync(path.join(destination, `reference/providers/${provider}/index.md`)).size > 0);
    }
    for (const file of ['llms.txt', 'reference/providers/index.md', 'gnome/index.html', 'macos/index.html']) {
        assert.ok(fs.statSync(path.join(destination, file)).size > 0);
    }
    for (const platform of ['macos', 'gnome']) {
        const guide = fs.readFileSync(path.join(destination, platform, 'index.html'), 'utf8');
        for (const tab of ['usage', 'history', 'settings']) {
            assert.match(guide, new RegExp(`id=gallery-tab-${platform}-${tab}`));
            assert.match(guide, new RegExp(`id=gallery-panel-${platform}-${tab}`));
            const image = guide.match(new RegExp(`id=gallery-panel-${platform}-${tab}[^<]*><img[^>]*width=880 height=(\\d+)`));
            assert.ok(image);
            assert.ok(Number(image[1]) > 760, `${platform} ${tab} should show the full tab`);
        }
    }
});

test('terminal dashboard appears in navigation and install cards', context => {
    const {destination, result} = build(context);
    assert.equal(result.status, 0, result.stderr);
    const home = fs.readFileSync(path.join(destination, 'index.html'), 'utf8');
    const start = fs.readFileSync(path.join(destination, 'start/index.html'), 'utf8');
    const install = fs.readFileSync(path.join(destination, 'tui/install/index.html'), 'utf8');
    assert.match(home, /href=\/en\/preview\/tui\//);
    assert.match(home, /id=hero-tab-tui/);
    assert.match(start, /Install the terminal dashboard/);
    assert.match(install, /quotaline-tui --demo/);
});

test('site shows both platform previews without hidden desktop state', context => {
    const {destination, result} = build(context);
    assert.equal(result.status, 0, result.stderr);
    const usage = fs.readFileSync(path.join(destination, 'reference/usage/index.html'), 'utf8');
    assert.match(usage, /<strong>macOS<\/strong>/);
    assert.match(usage, /<strong>GNOME<\/strong>/);
    const start = fs.readFileSync(path.join(destination, 'start/index.html'), 'utf8');
    assert.match(start, /Install the app/);
    assert.match(start, /Install the extension/);
    assert.doesNotMatch(start, /tmb-platform|data-platform/);
    const macosInstall = fs.readFileSync(path.join(destination, 'macos/install/index.html'), 'utf8');
    assert.match(macosInstall, /brew install --cask/);
    const gnomeInstall = fs.readFileSync(path.join(destination, 'gnome/install/index.html'), 'utf8');
    assert.match(gnomeInstall, /just install/);
});

test('site redirects moved platform-only pages', context => {
    const {destination, result} = build(context);
    assert.equal(result.status, 0, result.stderr);
    const widgets = fs.readFileSync(path.join(destination, 'reference/widgets/index.html'), 'utf8');
    assert.match(widgets, /\/en\/preview\/macos\/widgets\//);
    const updates = fs.readFileSync(path.join(destination, 'troubleshooting/application/index.html'), 'utf8');
    assert.match(updates, /\/en\/preview\/macos\/install-troubleshooting\//);
});

test('site reuses downloaded diagrams without network access', context => {
    const {destination, result} = build(context, undefined, {
        HTTPS_PROXY: 'http://127.0.0.1:9',
        HTTP_PROXY: 'http://127.0.0.1:9',
        NO_PROXY: '',
    });
    assert.equal(result.status, 0, result.stderr);
    const providers = fs.readFileSync(path.join(destination, 'contributing/providers/index.html'), 'utf8');
    assert.match(providers, /\/js\/mermaid\.[a-f0-9]+\.js/);
});

test('site rejects missing GNOME screenshots', context => {
    const {result} = build(context, '{{< shot gnome="missing-gnome-image" alt="Missing" >}}');
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /Missing screenshot images\/missing-gnome-image/);
});

test('site rejects missing internal pages', context => {
    const {result} = build(context, '[Missing page](/missing-doc-page/)');
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /Unresolved documentation link/);
});

test('site rejects missing screenshots', context => {
    const {result} = build(context, '{{< shot name="missing-doc-image" alt="Missing" >}}');
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /Missing screenshot images\/missing-doc-image/);
});

function build(context, addition, environment = {}) {
    const temporary = path.join(root, 'target/tmp');
    fs.mkdirSync(temporary, {recursive: true});
    const scratch = fs.mkdtempSync(path.join(temporary, 'site-test-'));
    context.after(() => fs.rmSync(scratch, {recursive: true, force: true}));
    const source = path.join(scratch, 'site');
    fs.mkdirSync(source);
    for (const item of ['hugo.toml', 'content', 'layouts', 'assets', 'data', 'static', 'resources']) {
        fs.cpSync(path.join(root, 'website', item), path.join(source, item), {recursive: true});
    }
    if (addition) {
        fs.writeFileSync(path.join(source, 'content/link-check.md'), `---\ntitle: Link check\n---\n${addition}\n`);
    }
    const destination = path.join(scratch, 'output');
    const result = spawnSync('hugo', ['--source', source, '--destination', destination,
        '--baseURL', 'https://example.invalid/en/preview/', '--minify'],
    {encoding: 'utf8', env: {...process.env, TMPDIR: temporary, ...environment}});
    return {destination, result};
}
