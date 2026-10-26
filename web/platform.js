// Browser-owned I/O. Documents, themes and exports stay on this device.
let deliver;
let dirty = false;
const SETTINGS_KEY = 'fleece.preferences.v1';
const canvas = () => document.getElementById('fleece-canvas');

async function importFile(file, theme) {
    if (!file) return;
    try {
        const limit = theme ? 256 * 1024 : 8 * 1024 * 1024;
        if (file.size > limit) throw new Error(theme ? 'Theme exceeds 256 KiB.' : 'Document exceeds 8 MiB.');
        const text = new TextDecoder('utf-8', { fatal: true }).decode(await file.arrayBuffer());
        deliver(theme ? 'theme' : 'document', file.name, text);
    } catch (error) {
        deliver('error', '', `Import failed: ${error.message}`);
    }
}

export function install(callback) {
    deliver = callback;
    window.addEventListener('beforeunload', event => {
        if (dirty) { event.preventDefault(); event.returnValue = ''; }
    });
    // Let the browser generate its trusted paste event instead of swallowing Ctrl+V.
    canvas().addEventListener('keydown', event => {
        if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'v') {
            event.stopImmediatePropagation();
        }
    }, true);
    document.addEventListener('paste', event => {
        if (document.activeElement !== canvas()) return;
        const text = event.clipboardData?.getData('text/plain');
        if (text !== undefined) { event.preventDefault(); deliver('paste', '', text); }
    });
    canvas().addEventListener('contextmenu', event => event.preventDefault());
    canvas().addEventListener('wheel', event => event.preventDefault(), { passive: false });
    canvas().addEventListener('dragover', event => event.preventDefault());
    canvas().addEventListener('drop', event => {
        event.preventDefault();
        void importFile(event.dataTransfer?.files[0], false);
    });
}

export function pick_file(theme) {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = '.toml';
    input.hidden = true;
    document.body.appendChild(input);
    input.addEventListener('change', () => {
        void importFile(input.files[0], theme);
        input.remove();
        canvas().focus();
    }, { once: true });
    input.addEventListener('cancel', () => { input.remove(); canvas().focus(); }, { once: true });
    input.click();
}

export function download(name, mime, bytes) {
    const blob = new Blob([new Uint8Array(bytes)], { type: mime });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url;
    link.download = name;
    document.body.appendChild(link);
    try { link.click(); } finally {
        link.remove();
        setTimeout(() => URL.revokeObjectURL(url), 30_000);
    }
}

export function load_settings() { return localStorage.getItem(SETTINGS_KEY); }
export function save_settings(text) { localStorage.setItem(SETTINGS_KEY, text); }
export function update_page(title, hasChanges) { document.title = title; dirty = hasChanges; }
export function is_macos() { return /Mac|iPhone|iPad/.test(navigator.platform); }
export function copy(text) {
    if (!navigator.clipboard) {
        deliver('error', '', 'Clipboard access requires HTTPS or localhost.');
        return;
    }
    navigator.clipboard.writeText(text).catch(error => deliver('error', '', `Clipboard failed: ${error.message}`));
}
export function show_ready() {
    document.getElementById('loading').hidden = true;
    document.documentElement.dataset.fleece = 'ready';
    canvas().focus();
}
export function show_error(error) {
    const loading = document.getElementById('loading');
    loading.hidden = false;
    loading.textContent = `FLEECE could not start: ${error}`;
    document.documentElement.dataset.fleece = 'error';
}
