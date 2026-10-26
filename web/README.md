# FLEECE in the browser

The browser build runs the same Rust circuit/FSM editor, history, interactions,
notation and PNG/SVG renderer as the desktop app. It uses WebAssembly, winit and
wgpu's WebGL2 backend. Documents and exports stay on the user's device; hosting
needs only a static file server.

## Build and serve

From the repository root, with Rust 1.95 or newer:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked
./scripts/build-web.sh
python3 -m http.server 8080 --directory web
```

Open <http://localhost:8080>. Use HTTPS when hosting on another machine so browser
clipboard APIs are available. Serve the `web/` directory, including generated
`pkg/` and its `snippets/` subdirectories. Do not open `index.html` with `file://`.
The server must serve `.wasm` as `application/wasm`; Python's server does this.
Static hosting can compress the assets to reduce download size.

The `wasm-bindgen-cli` version must match the `wasm-bindgen` package in
`Cargo.lock`. The build script checks this before generating files. If the
lockfile changes that version, install the matching CLI again. Generated bundle
files live in `web/pkg/`, which is ignored by Git.

## Browser behavior

- **Open / Ctrl+O:** choose a TOML file, or drop one onto the canvas. Invalid,
  oversized and non-UTF-8 documents leave the current diagram untouched.
- **Save / Ctrl+S:** download editable TOML. Save As asks for a new file name.
  The app remembers names, not filesystem paths or write permissions.
- **Export / Ctrl+E:** choose a `.png` or `.svg` name and export options, then
  download the full diagram. Exporting does not mark the document saved.
- **New / opening another document:** use the same unsaved-change dialog as the
  desktop app. A failed download handoff preserves unsaved work and pending actions.
- **View → Theme:** select a built-in or upload custom theme TOML. Successful
  theme choices, custom palette source, export options and recent download names
  persist in this origin's `localStorage`, under `fleece.preferences.v1`.
- **Clipboard:** Ctrl/Command+X/C/V uses browser clipboard facilities for both
  selected diagram objects and text fields. Browser permissions still apply.
- **Navigation:** selection, repeated placement, Wire/Drag modes, scroll panning,
  Ctrl+scroll/pinch zoom and keyboard shortcuts use the shared editor logic.
- **Reload / Ctrl+Shift+R:** choose the document again. Browsers do not monitor
  or automatically overwrite a local file as the native host can.

A download acknowledgement means the browser accepted the download initiation;
it cannot tell FLEECE whether the user later cancelled the download manager.
The app prompts before leaving a tab with unsaved diagram changes. Browser
storage holds preferences, not an autosaved diagram: download documents to keep
work across sessions. Storage restrictions leave preferences in memory and show
an editor status message. Native `~/.config/fleece` and `~/.local/state/fleece`
files are never used in the browser.

Custom browser themes stay in browser preferences. Downloaded documents omit the
browser-only theme reference so they open on desktop without an invalid path.
The scene itself is unchanged. Desktop `fleece FILE.toml` and `-o` continue to
work independently of the web bundle.

## Architecture and verification

`src/web.rs` installs browser callbacks and starts the shared window/input/GPU
host from `src/desktop.rs` using asynchronous graphics initialization. Browser
animation frames schedule drawing; `web-time` supplies a working monotonic clock.
`web/platform.js` owns file pickers, Blob downloads, clipboard access, tab-close
prompts and local storage. `src/app/files.rs` supplies the host-neutral file
request/acknowledgement interface, validated imports and external preferences.

Checks that do not need the JS binding generator or a running browser:

```sh
cargo check --locked --target wasm32-unknown-unknown --lib
cargo clippy --locked --target wasm32-unknown-unknown --lib -- -D warnings
cargo test --locked
node web/platform.test.mjs
```

The Rust transfer tests cover import validation and unsaved confirmation, failed
and stale acknowledgements, saved snapshots, byte-identical native/browser
exports, portable custom themes and preference restoration. The Node tests
exercise DOM/file/clipboard/storage boundaries with browser API stand-ins;
they do not substitute for a real GPU/browser smoke test.

After building the bundle, smoke-test a browser with WebGL2 enabled: open a
circuit and an FSM, place and drag objects, pan/zoom, connect wires/transitions,
undo/redo, cut/copy/paste, pin attachment on placement/dragging, download TOML, and export PNG/SVG. Refresh after changing a
theme and reopen the downloads on desktop. Test the unsaved-change prompt when
opening another file and when leaving the tab. Check the browser console for
startup/rendering errors. Desktop-oriented browsers are the primary target;
winit's browser host does not provide native IME integration. Some browser-reserved
shortcuts may require using the corresponding toolbar/menu action.
