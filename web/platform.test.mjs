// Test the browser I/O boundary without a GPU or generated WASM bindings.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const source = await readFile(new URL('./platform.js', import.meta.url), 'utf8');

function element() {
    const listeners = new Map();
    return {
        listeners, hidden: false, removed: false,
        addEventListener(type, callback) { listeners.set(type, callback); },
        remove() { this.removed = true; },
        focus() { document.activeElement = this; },
        click() { this.clicked = true; },
    };
}

async function browser() {
    const canvas = element();
    const loading = element();
    const children = [];
    globalThis.document = {
        activeElement: canvas, documentElement: { dataset: {} },
        listeners: new Map(),
        getElementById(id) { return id === 'fleece-canvas' ? canvas : loading; },
        createElement() { return element(); },
        body: { appendChild(child) { children.push(child); } },
        addEventListener(type, callback) { this.listeners.set(type, callback); },
    };
    globalThis.window = { listeners: new Map(), addEventListener(type, callback) { this.listeners.set(type, callback); } };
    const storage = new Map();
    globalThis.localStorage = { getItem: key => storage.get(key) ?? null, setItem: (key, value) => storage.set(key, value) };
    Object.defineProperty(globalThis, 'navigator', { configurable: true, value: { platform: 'Linux', clipboard: { writeText: async text => { storage.set('clipboard', text); } } } });
    const api = await import(`data:text/javascript,${encodeURIComponent(source)}#${Math.random()}`);
    const events = [];
    api.install((...event) => events.push(event));
    return { api, canvas, loading, children, events, storage };
}

function file(name, bytes) {
    const buffer = typeof bytes === 'string' ? new TextEncoder().encode(bytes) : bytes;
    return { name, size: buffer.length, arrayBuffer: async () => buffer.buffer };
}
const settle = () => new Promise(resolve => setImmediate(resolve));

test('uploads validate UTF-8 and size before delivering documents or palettes', async () => {
    const b = await browser();
    b.api.pick_file(false);
    let input = b.children.at(-1);
    input.files = [file('circuit.toml', 'format_version = 1')];
    input.listeners.get('change')();
    await settle();
    assert.deepEqual(b.events.pop(), ['document', 'circuit.toml', 'format_version = 1']);
    assert.equal(input.removed, true);
    b.api.pick_file(true);
    input = b.children.at(-1);
    input.files = [{ name: 'huge.toml', size: 256 * 1024 + 1 }];
    input.listeners.get('change')();
    await settle();
    assert.match(b.events.pop()[2], /Theme exceeds/);
    b.api.pick_file(false);
    input = b.children.at(-1);
    input.files = [file('invalid.toml', new Uint8Array([0xff]))];
    input.listeners.get('change')();
    await settle();
    assert.equal(b.events.pop()[0], 'error');
});

test('downloads use Blob URLs and remove temporary anchors', async () => {
    const b = await browser();
    const original = globalThis.setTimeout;
    let cleanup;
    globalThis.setTimeout = callback => { cleanup = callback; };
    try {
        b.api.download('image.svg', 'image/svg+xml', new TextEncoder().encode('<svg/>'));
    } finally { globalThis.setTimeout = original; }
        const link = b.children.at(-1);
        assert.equal(link.download, 'image.svg');
        assert.equal(link.clicked, true);
        assert.equal(link.removed, true);
        const blob = await fetch(link.href).then(response => response.blob());
        assert.equal(blob.type, 'image/svg+xml');
        assert.equal(await blob.text(), '<svg/>');
        cleanup();
});

test('clipboard, storage and unsaved-close behavior stay in the browser', async () => {
    const b = await browser();
    b.api.save_settings('theme = "original-dark"');
    assert.equal(b.api.load_settings(), 'theme = "original-dark"');
    b.api.copy('copied diagram');
    await settle();
    assert.equal(b.storage.get('clipboard'), 'copied diagram');
    let prevented = false;
    document.listeners.get('paste')({ clipboardData: { getData: () => 'pasted diagram' }, preventDefault() { prevented = true; } });
    assert.equal(prevented, true);
    assert.deepEqual(b.events.pop(), ['paste', '', 'pasted diagram']);
    b.api.update_page('Unsaved — FLEECE', true);
    const event = { preventDefault() { this.prevented = true; } };
    window.listeners.get('beforeunload')(event);
    assert.equal(event.prevented, true);
    b.api.show_ready();
    assert.equal(document.documentElement.dataset.fleece, 'ready');
    assert.equal(b.loading.hidden, true);
});
