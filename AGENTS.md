# FLEECE contributor and coding-agent guide

FLEECE is a Rust diagram editor for logic circuits and finite-state machines,
with desktop and WebAssembly browser hosts. Its small launch interface opens a document by path and exports a
saved document to PNG/SVG with `-o`. Use the graphical editor or Rust library to
create and edit diagrams; keep editable documents in TOML.

This guide applies throughout the repository. Preserve existing user changes,
keep changes focused on requested behavior, and update documentation when changing
public APIs, formats or interactions. The command-line interface is a general
application feature. Do not reintroduce the removed document-editing subcommands.

## Repository structure and ownership

This is one Cargo package, `fleece`, with a reusable library and a native binary.
It uses Rust edition 2024; `Cargo.toml` specifies the minimum Rust version.
Keep `Cargo.lock` committed and use `--locked` for reproducible builds.

| Path | Responsibility |
| --- | --- |
| `src/lib.rs` | Public library entry point, architecture overview and documentation lints |
| `src/main.rs` | Desktop/image-output dispatch and human-readable error reporting |
| `src/desktop.rs` | Shared winit input/window and wgpu renderer; native startup and asynchronous browser startup |
| `src/web.rs`, `web/` | Browser callbacks, static launcher, download/clipboard/storage adapter and browser documentation |
| `src/app/files.rs` | Host-managed file requests, import validation, download acknowledgement and portable browser preferences |
| `scripts/build-web.sh` | Release WASM build and matching wasm-bindgen JS binding generation |
| `src/model.rs` | Circuit/document types, connectivity, validation, serialization and atomic writes |
| `src/model/components.rs` | Component kinds and catalog metadata: groups, labels, shortcuts, ports and capabilities |
| `src/model/attachment.rs` | Transactional pin attachment to coincident wires/joints after placement/movement |
| `src/model/routing.rs` | Orthogonal routes, route controls, joint splicing and wire bus metadata edits |
| `src/model/bus.rs` | Bus marker positions, route projections and transfer across joints |
| `src/model/fsm.rs` | FSM states, transitions, topology and model edits |
| `src/history.rs` | Bounded circuit-snapshot undo/redo |
| `src/launch.rs` | Minimal argument parser: optional document path, `-o`/`--output`, help and `--` |
| `src/drawing.rs`, `src/drawing/fsm.rs` | Scene primitives, static document geometry and logic/FSM export scenes |
| `src/editor_drawing.rs` | Interactive symbols, editor geometry and theme-aware canvas scenes |
| `src/notation.rs` | Math label parsing, font metrics, scripts, primes and overbars |
| `src/export.rs` | Export options, crop bounds and atomic PNG/SVG output |
| `src/theme.rs`, `themes/` | Palette types, strict theme loading and embedded built-in templates |
| `src/app.rs` | Editor state, document lifecycle, dirty tracking and top-level UI composition |
| `src/app/canvas.rs`, `src/app/fsm.rs` | Logic-specific and FSM-specific canvas behavior |
| `src/app/components.rs` | Component library, repeated placement and preview configuration |
| `src/app/inspector.rs` | Selected-object properties and coalesced property edits |
| `src/app/keyboard.rs` | Global/canvas shortcut tables and typed action dispatch |
| `src/app/mode.rs` | Mutually exclusive Select/Wire/Drag modes and gesture cleanup |
| `src/app/interaction.rs` | Shared selection, drag snapshots, snapping and hit calculations |
| `src/app/utilities.rs` | Shared canvas setup, coordinates, pan/zoom, grid and controls |
| `src/app/painting.rs` | Conversion of scene primitives into egui painting operations |
| `src/app/clipboard.rs` | Selection serialization, cut/copy/paste and cursor-relative placement |
| `src/app/dialogs.rs` | File browser, export and other editor dialogs |
| `src/app/export_preview.rs` | Shared export settings and cached, bounded raster previews |
| `src/app/configuration.rs` | Managed theme files and active-theme configuration |
| `src/app/preferences.rs` | Save/export history and storage locations |
| `tests/` | Integration tests for model, CLI, editing, rendering, themes and compatibility |
| `examples/` | Editable TOML documents and reference image exports |
| `assets/` | Bundled fonts and their license; exports must not require system fonts |

Keep persisted domain behavior in the model and transaction layer. Keep window,
GPU and platform integration in the desktop host. Image output with `-o` must run
without initializing a window, GPU or native preferences. Rendering should consume model data;
painting must not mutate diagram connectivity or execute logic.

Use the existing focused child modules when extending the editor. Share navigation,
selection and gesture behavior between logic and FSM documents; avoid growing a
second domain-specific event loop or a large conditional dispatcher.

## Rust conventions and change workflow

- Follow the surrounding module's naming and visibility conventions. Types use
  `UpperCamelCase`; functions, modules and fields use `snake_case`.
- Keep helpers private or `pub(super)` where possible. Add public API only when
  callers need it, and document its contract with Rustdoc.
- Use typed enums for modes, actions and component kinds. Extend the catalog and
  binding tables instead of duplicating string matching in panels and canvases.
- Follow the existing `Result<_, String>` error convention at model, launch and file
  boundaries. Return actionable errors for malformed user input; reserve panics
  for internal invariants. Do not use `unwrap` for fallible user file operations.
- Keep serialization strict: unknown fields are errors. Add defaults for optional
  persisted fields so older documents continue to load, and validate new ranges.
- Do not reintroduce retired schema fields, aliases or migrations. Preserve
  unrelated labels and routes during ordinary edits.
- Prefer existing dependencies and helpers. Changes to dependencies should include
  the corresponding lockfile update and a concrete reason.
- Use `cargo fmt --all` for formatting. Keep API documentation and behavior tests
  with the implementation changes they describe.

Before editing, inspect the owning module and related tests. For a model change,
trace serialization, validation, clipboard/history and both renderers.
For an interaction change, trace mode transitions, text focus, cancellation and
undo boundaries in both diagram editors.

## Model edits, history and transaction boundaries

`Document` contains the circuit and persisted editor view. `Circuit` contains
either logic nodes/wires or an FSM. Selection, current gestures, dialogs and
application preferences are transient editor state.

Undo/redo uses `History` snapshots of `Circuit`, not reversible command objects.
Use `Editor::change` for discrete diagram edits. Inspector text/coordinate edits
and drags collect a before-snapshot and finish as one action; do not record every
frame or mouse movement. A no-op must preserve the redo stack. Viewport, theme,
selection and native preferences do not belong in diagram history.

Low-level model mutations do not all validate the complete document. Validate at
load/save and library transaction boundaries. Fallible edits should validate
before mutation or work on a clone; failure must not partially change the
caller's document or its file. Use the shared atomic-write implementation for
persisted output instead of truncating the destination before success.

When adding persisted properties, preserve them through TOML round-trips,
copy/paste, duplication, undo/redo and relevant topology edits. Test rejected
values and rollback as well as the successful path.

## Build, launch and image output

Run from the repository root:

```sh
cargo build --locked
./target/debug/fleece
./target/debug/fleece examples/half-adder.toml
./target/debug/fleece examples/half-adder.toml -o /tmp/half-adder.png
./target/debug/fleece -o /tmp/half-adder.svg examples/half-adder.toml
./target/debug/fleece --help
```

No arguments opens an empty logic circuit. A positional document path opens the
desktop editor. `-o IMAGE` (or `--output IMAGE`) exports without opening a window;
PNG/SVG is selected by the output extension. Options may precede or follow the
input. `--` ends option parsing for document paths beginning with a dash.
Output images are atomically replaced, as with compiler output files. The input
is loaded and validated before rendering. Failures print a human-readable
`FLEECE: ...` diagnostic to stderr and exit 1; success exits 0. Output mode is
silent on success. There is no JSON command protocol.

The former `new`, `demo`, `inspect`, `validate`, `apply`, `export`, `theme-check`
and `gui` subcommands and `--theme` option have been removed. Use File → New,
File → Open, the inspector and View → Theme for those editor workflows.
Programmatic clients use `Document`, `Circuit`, and `export`.
The library requires no display and does not initialize native preferences.

FLEECE edits diagrams; editor/launch features do not expose simulation, input
bit toggling, pulses, register resets, generated truth tables or live overlays.
There is no simulation engine or stored input/register signal state. Loading
a document only parses and validates; it never changes labels, pins or data.

The GUI checks external file changes once per second and reloads clean documents.
It protects unsaved work; Ctrl+Shift+R explicitly reloads. Coordinate external
edits with users who have unsaved changes. Never replace their work silently.

## Component and wire contracts

Component names, shortcuts, groups, default ports and configurable-input
capabilities belong in `Kind::definition` in `model/components.rs`.
`Kind::ALL` supplies the component library. Pins use zero-based port indices.
AND, OR, NAND, NOR and XNOR support 2–6 inputs; the placement panel offers 2/3.
XOR always has two inputs. Custom modules are diagram blocks with 2–6 inputs and
1–4 outputs, without executable truth tables.

Input and output names are numbered independently using the count of that kind
already present. Other components keep their catalog names. Labels are not
unique identifiers: duplicates, pasted names and manual names are all valid.

D-flip-flops use the persisted kind `register`, with 1–8 data bits (default 2).
Data inputs are `0..bits`, CLK is input `bits`, RST is input `bits+1`, and Q
outputs are `0..bits`. Width changes preserve CLK/RST connectivity and discard
wires on removed data pins. Do not add register state or execute resets/clocks.
Multiplexers have 1–3 selection bits, 2/4/8 data pins followed by selectors;
S0 is least significant. Their total input count is 3, 6 or 11. Width changes
preserve surviving data pins and selector identity.

Explicit net colors are black, red, green, blue, purple and amber. Colors apply
to the output and its branches in both scenes. Default editor ink follows the
active theme; static export ink is independent of it. Inversion bubbles annotate
gate inputs and outputs; output inversion switches the paired gate kind.

Labels support subscripts (`s_0`, `s_{10}`), primes (`s_1'`), superscripts
(`x^{2}`) and overbars (`\bar{s_1}`, `\overline{Q}`). TOML literal strings avoid
backslash escaping. Module headings use plain bold serif text.

A regular wire has `from`/`to` endpoints and `horizontal_first` (default true).
It routes horizontally then vertically, or vertically then horizontally, with
at most one elbow. Explicit `waypoints` override that route and remain an active
part of route editing. Use `Wire::points_between` with editor or export endpoints.
The first dominant mouse movement after a segment starts chooses its initial
axis. Jitter below three screen pixels or equal-axis movement does not lock it;
input-first gestures reverse the stored flag. Every new segment resets the
choice, which commits in the same undo action as the wire.

Wires may have `bus_width = 1..1024` as a display-only annotation. Select a wire
and enable Bus annotation in the inspector. The shared drawing helper puts a
slash and width on its longest segment, above horizontal wires or beside
vertical wires. Preserve the field through splitting, splicing, clipboard,
history and persistence. Splicing prefers the outgoing annotation, falling back
to the incoming one. An optional `bus_position` in 0–1 stores normalized distance
along the route; missing values use the longest-segment midpoint. Both renderers
resolve it against their own geometry. Select-mode dragging of the slash or label
moves the marker, including across persistent joint nodes, as one history action.
Unrelated crossings and paths through components are excluded. Matching destination
widths merge; conflicting widths are preserved. Clearing an annotation clears its
position. Splits/splices reproject explicit positions; reversing a route reverses
the fraction. Bus markers have their own drag hit target and never alter connectivity.
Registers do not display a bit-count footer; their Bits control sets pin count.

Removing a block removes its attached wires. Connecting an occupied input
replaces its previous driver; output fan-out is supported. Reset route and
`auto_layout` clear explicit route controls. Reducing ports removes wires that
reference deleted pins.

## Document TOML format

Documents have `format_version = 1`, a `[circuit]` table, and an optional `[editor]`
table. See `examples/half-adder.toml` for a complete saved document.

```toml
format_version = 1

[circuit]
title = "Buffer example"

[[circuit.nodes]]
id = 1
kind = "input"
label = "A"
position = { x = 40.0, y = 40.0 }
inputs = 0
outputs = 1

[[circuit.nodes]]
id = 2
kind = "output"
label = "Y"
position = { x = 300.0, y = 40.0 }
inputs = 1
outputs = 0

[[circuit.wires]]
from = { node = 1, port = 0 }
to = { node = 2, port = 0 }
waypoints = []
```

For an empty circuit include `nodes = []` and `wires = []` in `[circuit]`.
IDs must be unique positive integers. Labels may
contain at most 80 printable characters; titles at most 200. Positions must be
finite and within ±100,000. Documents support up to 1,000 blocks, 8,000 wires,
64 waypoints per wire, and 8 MiB of TOML. Register Q-to-D feedback is supported;
Q cannot drive the same register's CLK or RST directly.

Node `input_bubbles` is an optional boolean array in input-port order, available
only on logic gates. Missing entries mean no inversion. Output bubbles use the
existing NAND/NOR/XNOR/NOT kinds. Reducing gate inputs removes deleted bubbles.

Node `colors` is an optional array of output colors in port order. There are no
`value`, `truth_table` or `register` state fields. Unknown fields are rejected.

`[editor]` fields are `layout` (`standard`, `library_right`, `focus`), `pan`
(`{x=40.0,y=60.0}`), `zoom` (0.15–4.0), `grid`, `snap`, and `theme`. Missing editor
fields use defaults. Unknown document fields are rejected to catch typos.

`[editor].theme` is an optional UI theme reference: `vscode`, `original-dark`,
`original-light`, `tokyo-night`, or an absolute path to a custom theme TOML file.
Without a reference, Original dark is used. New documents default to Original dark and visible grid. Original dark
uses dark component bodies and light outlines/pin labels; Original light and
static exports retain their light backgrounds.

UI themes are independent of exported diagrams. They must never change export
fonts, shapes, colors, routing, or crop bounds. Interactive geometry belongs in
`Node::size`/`pin`; document geometry uses `drawing::diagram_size`/`diagram_pin`.

## Configuring UI themes

Use **View → Theme** to select an included palette, load a TOML theme, or reload
the active theme after editing it. The source files are in `themes/`. Built-ins
also have embedded fallbacks so the executable can run without the source tree.

Theme files contain `theme_version = 1`, a required `name`, optional `base`
(`dark` or `light`), and optional `[ui]`, `[canvas]`, `[metrics]` tables. Colors
are `#RRGGBB` or `#RRGGBBAA` strings. Omitted colors inherit the selected base.
See `themes/tokyo-night.toml` for a complete palette and README.md for all keys.
Font size is 10–24; corner radius is 0–24. Invalid colors, unknown fields,
unsupported versions, and files larger than 256 KiB are rejected.

Theme references persist with saved documents; native theme choices are also
recorded immediately in app configuration. Failed custom-theme loads
and reloads preserve the active palette. A saved document referencing an
unavailable theme opens with Original dark and a status message.

In logic circuits, right-click removes a component or joint, or disconnects the
clicked wire segment, on release. Wire removal preserves components and other
branches and is undoable. Pin hits take priority: right-clicking an input pin
removes its incoming wire; right-clicking an output pin removes all fan-out from
that output, preserving the component and other pins' wires. Unconnected pin
clicks do not create an undo entry.
Regular Select mode (V or Esc) uses left-click to select nodes, pins, or wires,
node dragging to move, and empty-canvas dragging to box-select. Shift extends
selection. D toggles Drag mode, where primary dragging anywhere pans; V/Esc selects.
Delete/Backspace delete the current selection; X places XOR
and Shift+E places XNOR. Text fields keep normal X typing.
Shift+D enters repeated D-flip-flop placement. Middle drag and Space+drag also pan. Regular mode never starts wiring or disconnects
pins. Wire mode (W or the components panel) routes all left-clicks into wiring: choose
pins/joints, split existing wires, or start/extend suspended wires on empty
canvas. W toggles Wire mode, while Shift+W opens the keyboard pin dialog.
Wire mode preserves fan-out when clicking connected pins. Choosing a component
enters repeated placement and leaves Wire mode. Esc returns to regular mode.
Removing a joint preserves its wire connections; Ctrl+Z undoes removal.
Both diagram types pan on ordinary mouse-wheel/touchpad scroll.
Pinch and Ctrl+scroll zoom at the pointer. Shift+scroll retains
horizontal panning, and purely horizontal scrolling pans. Navigation is disabled
over side panels and while modal dialogs are open.


Joint nodes may terminate suspended wires. While wiring, clicking empty canvas
creates a persisted joint endpoint. All joints, including explicit routing
controls and automatic elbows, draw solid dots on the canvas without selecting
their wire. Selection handles are round. Dragging a routing control or automatic elbow
promotes it to a persistent joint in the same undo action; clicking only selects.
Other joint positions stay fixed. Incident wires stretch, and existing explicit
route controls stay fixed. In Select mode, horizontal segments drag vertically
and vertical segments drag horizontally, preserving endpoints and connectivity.
Exports hide the dot for exactly two incident wires; other joints keep solid dots. Module exports use their normal symbols without
colored outer frames. Exported inputs and outputs are plain wire stubs with their
names above them, without terminal tag shapes or INPUT/OUTPUT captions.
Input components do not show 0/1 value text on the canvas or in exports.

## FSM documents

Startup opens an empty logic circuit. File → New has Logic Circuit and FSM
choices. Programmatic clients use `Circuit::new_fsm()`; `examples/fsm.toml`
is a complete editable example.
Document validation, library edits and PNG/SVG output support FSMs.
FSM condition/output strings are diagram annotations, not executable expressions.

FSM documents keep `nodes = []` and `wires = []` in `[circuit]`, and store states,
transitions and an optional initial state ID in `[circuit.fsm]`. State positions
are circle centers. States have `id`, `name`, `position`, optional `output` and
`accepting`. Transitions have `id`, `from`, `to`, optional `condition` and `output`.
IDs are positive and unique within states/transitions respectively. Self-loops,
parallel edges, and reciprocal transitions are supported. Deleting a state
removes incident transitions and, if necessary, picks a surviving initial state.

FSM model, diagram paths and GUI live in `model/fsm.rs`, `drawing/fsm.rs` and
`app/fsm.rs`. Both editors share
`app/utilities.rs` for canvas setup, coordinate transforms, pan/zoom, grid and
controls; `app/interaction.rs` for selection, drag snapshots, snapping and hit
calculations; `app/keyboard.rs` for shortcut bindings/actions; and
`app/painting.rs` for scene painting. Use the shared Drag state and helpers rather
than adding a separate FSM interaction loop. `app/mode.rs` owns the mutually
exclusive Select/Wire/Drag modes and transition cleanup. FSM uses S for repeated state
placement (click/Enter), W for transition mode (T alias), V/Esc for Select mode,
empty-canvas box selection, D for Drag mode/panning, and right-click removal. Clipboard
and duplication preserve internal transitions with remapped IDs. Pasting anchors
the selection near the mouse cursor, with a small offset on repeated pastes at
the same location; both editors preserve relative positions and routed geometry. FSM edits use
the same circuit snapshot history and transactional validation as logic documents.

Native state in `app/preferences.rs` remembers successful save and export paths
and export options across documents and app launches. Successful theme changes
are persisted immediately in `~/.config/fleece/config.toml`; defaults use Original
dark. The `theme` field contains the active managed theme's absolute path (paths
relative to the config file are also readable). The remembered theme is applied
across new documents and app restarts. Failed theme loads preserve the active and stored
choice. If a remembered custom theme is unavailable, keep the document/default
palette and report the failure. Save dialogs prefer the
current document path, then the last save folder; new names default to
circuit.toml/fsm.toml. Export dialogs prefer the last export path and format,
then the current document/last save folder. Only successful operations persist
state; cancelled or failed operations preserve the previous paths/options.
Export options are scale, transparency, title and padding; `signals` is rejected.
Both hosts share a live export preview, cached by circuit and options. Preview
rasterization uses the same paths/fonts/crop as PNG output, with bounded dimensions
and a checkerboard for transparency; it does not allocate the full output image.
Preview state is transient and must not mark documents saved or persist options. On Linux the desktop host uses `$XDG_STATE_HOME/fleece/state.toml`,
defaulting to `~/.local/state/fleece/state.toml`; relative XDG values
are ignored. Windows uses LOCALAPPDATA (APPDATA fallback), macOS uses
`~/Library/Application Support`. The history folder is created on the first
successful save/export and state is written atomically. The desktop uses
Editor::with_native_preferences; Editor::new remains isolated and
Editor::with_preferences_path injects a test/library state path and a sibling
config.toml without reading native locations. Editor::with_storage_paths allows
separate history/config paths to be injected. Export options loaded from state are validated; invalid options
revert to defaults without discarding remembered paths.

## Modifying the implementation

Keep new code formatted with `cargo fmt --all`. Follow the existing separation:
logic and persistence in `model.rs`, component metadata in
`model/components.rs`, shared orthogonal routing in `model/routing.rs`, document
paths in `drawing.rs`, editor
symbols in `editor_drawing.rs`, UI themes in `theme.rs`, exports in
`export.rs`, math layout in `notation.rs`, native integration in `desktop.rs`,
and GUI behavior in `app.rs` and `app/`.

After code changes, run:

```sh
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

Preserve export independence from the current viewport, selection, theme, grid,
and unfinished placement. Preserve transactional edits and validation at file
boundaries. Prefer meaningful behavior tests over tests of widget labels.

Component names, shortcuts, groups, default ports and input capabilities belong
in `Kind::definition` in `model/components.rs`; register library kinds in
`Kind::ALL`. The component library and keyboard placement read this catalog.
Add editor shortcuts to the global/canvas binding tables in `app/keyboard.rs`
and dispatch their typed action there. Use `set_mode`/`toggle_mode` instead of
independent boolean tool flags. Placement and preview share
`app/components.rs::configure_placement`.

Wire rendering uses `Wire::points_between` with interactive or export endpoints.
Remove route controls with `Circuit::remove_wire_controls`; joint removal splices
routes through `Circuit::splice_joint`. Both operations preserve unselected
corners and remove duplicate controls at the removed joint, so deleting a
collinear dot does not make a different automatic corner disappear.

Native theme configuration belongs to `app/configuration.rs`, separate from
runtime file history. On Linux it uses `$XDG_CONFIG_HOME/fleece/config.toml`,
defaulting to `~/.config/fleece/config.toml`, with theme files in the adjacent
`themes/` directory. Native startup installs missing embedded built-in templates
and writes a default config only when needed; existing user-edited theme
files must not be overwritten. Custom theme selection validates and imports the
file before atomically recording its path. Filename collisions choose a different
file. Failed selection leaves the active theme/config unchanged. Theme choices belong only in config.toml; runtime history does not contain a
theme field and older locations are not searched. Tests inject temporary config paths and native
host tests use Editor::new so they never create real user configuration files.

Source documentation is generated with `cargo doc --locked --no-deps`; its entry
point is `src/lib.rs`. Keep module/API contracts and comments about invariants in
sync when changing routing, history, persistence or editor interactions. Runnable
Rust documentation examples are checked by `cargo test --locked --doc`.

Public source APIs use proper Rustdoc (`///` for items, `//!` for modules) with
intra-doc links, error/panic contracts where applicable, and runnable examples.
The library enables `missing_docs` and denies broken Rustdoc links. Document new
public types, fields, variants and methods before adding them. Validate with
`RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps` and keep doctests passing.
Implementation-only comments should continue explaining algorithms/invariants
where ordinary inline comments are appropriate.


## Extending a feature across layers

For a new component, register its kind in `Kind::ALL` and define catalog metadata
in `Kind::definition`. Implement port names, interactive geometry, document
geometry and symbols in their existing owners. Add validation and compatibility
defaults before exposing new persisted properties. Placement and preview must use
the same configuration path. Exercise both the editor scene and static export.

For a shortcut, add a typed action and binding in `app/keyboard.rs`. Respect text
focus and modifiers; typing into labels must not trigger canvas commands. Use
`set_mode`/`toggle_mode` for tool changes so unfinished wiring and placement are
cleaned up consistently. Check both logic and FSM behavior.

For a wire property, consider split, splice, copy/paste, duplication, route reset,
history and persistence. Derive paths through `Wire::points_between` and call the
existing topology helpers. A renderer-only annotation must not alter hit testing
or connectivity. Use the shared bus-marker drawing helper for both scenes.

For a persistence change, use injected temporary storage paths in tests. Separate
document state, theme configuration and runtime save/export history. Preserve
failed-operation rollback and existing theme files.

## Test organization and verification

| Test location | Main contracts |
| --- | --- |
| `tests/core.rs` | Validation, persistence, connectivity and document bounds |
| `tests/editing.rs` | Bubbles, labels, routing, catalog and copy/duplicate behavior |
| `tests/attachment.rs` | Pin overlap, safe branching, rerooting, ambiguity and transactional limits |
| `tests/joints.rs` | Joint creation, stretching, branching and route preservation |
| `tests/buses.rs` | Bus ranges, slash placement, metadata preservation and export captions |
| `tests/multiplexer.rs`, `tests/register.rs` | Port resizing, wiring identity, compatibility and model edits |
| `tests/fsm.rs` | FSM topology, transitions, scenes, clipboard and external reload |
| `tests/diagram.rs` | Static geometry, label notation, crop bounds and rendering |
| `tests/themes.rs` | Theme validation, defaults, editor colors and export independence |
| `tests/editor.rs` | Headless egui input, keyboard/mouse gestures, panels, undo and dialogs |
| `tests/external_files.rs` | Clean reload and protection of unsaved local changes |
| `tests/schema.rs` | Current schema, read-only loading and rejection of retired fields |
| `tests/launch.rs` | Process-level image output, diagnostics and document preservation |
| Inline unit tests | Implementation-specific helpers, current storage and native integration |
| Rustdoc examples | Public API usage and executable documentation |

Prefer behavioral assertions over tests of labels alone. Model tests should use
public APIs and validate persisted results. UI tests feed events to an egui
context; they do not require a native window. Use `Editor::new` for isolated
editors and injected temporary paths when testing preferences/configuration.
Never write tests against the real home configuration or runtime history.

After implementation changes, run:

```sh
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps
```

Run focused tests while developing, then the required full checks. For image
changes, inspect an actual PNG/SVG generated from a representative document;
include branches, labels or modules relevant to the change. Keep previews in a
temporary directory. Avoid replacing reference example images unless requested.

For documentation-only changes, check links, referenced paths, commands and
formatting; a full build is needed when examples or API documentation change.
Do not stage generated `target/` content, local credentials or tool settings.


## Retired features and schema

The batch-editing API and patch files, simulation engine and tests, signal-aware
rendering/options/theme colors, input values, register storage, module truth tables,
old clipboard header, old preference locations and automatic document migrations
have been removed. Do not restore them as compatibility fallbacks. XOR documents
must have exactly two inputs. The canonical D-flip-flop kind is `register`; the
old `d_flip_flop` spelling is not an alias. Theme references replace the removed
`[editor].dark` field. Current clipboard documents start with `# FLEECE selection`.

Existing older files containing removed fields must be edited to the current
schema before opening. Keep normal user labels unchanged, including labels that
happen to resemble old automatic names. Bundled examples use the current schema.


## WebAssembly host conventions

The library builds as `rlib` and `cdylib`. Native launch arguments and `pollster`
are gated to non-WASM targets. `src/web.rs` compiles the shared winit/wgpu host
from `src/desktop.rs` for the browser and exports a wasm-bindgen startup function.
Do not fork circuit/FSM input or drawing behavior into a browser-specific loop.
Initialize browser graphics asynchronously, retain initial texture updates and
resize the surface to the latest canvas size after initialization. Use browser
animation frames rather than a busy polling loop. Use `web_time::Instant` in
shared UI/host code; the browser target cannot use native monotonic clocks.
The browser renderer selects WebGL2; keep its device limits compatible with it.

Construct browser editors with `Editor::with_external_files`, never with native
preferences. File requests are exposed through `app::FileRequest`; the host drains
requests after each frame and acknowledges generated downloads by identifier.
Only successful handoffs update saved snapshots and recent file names. A late
acknowledgement must not mark newer edits saved or discard them for a pending
open/new action. Browser acknowledgements mean download initiation, since download
manager cancellation is not observable. Exports never mark diagrams saved.
Uploaded documents are bounded/validated before the usual unsaved-change flow.

`web/platform.js` owns DOM file pickers, Blob URLs, clipboard, localStorage and
tab-close prompts. Clipboard access requires a secure origin (localhost qualifies).
Browser preferences use `fleece.preferences.v1` in the hosting origin's storage;
no native config/state locations are accessed. Custom themes are validated and
stored as text there; downloaded TOML omits the browser-only theme reference so
files remain portable to desktop. Failed settings restoration preserves stored
preferences until the user changes settings. Browser storage is not diagram
autosave: the user downloads editable TOML to keep work.

Generated `web/pkg/` is ignored. `scripts/build-web.sh` requires wasm-bindgen-cli
matching Cargo.lock; serve all of web/, including generated snippets, as static
files. Detailed setup and browser smoke-test steps are in `web/README.md`.
When changing browser integration, also run:

```sh
cargo clippy --locked --target wasm32-unknown-unknown --lib -- -D warnings
cargo build --locked --release --target wasm32-unknown-unknown --lib
node web/platform.test.mjs
```

Tests in `app/files.rs` exercise host-independent transfer contracts. Node tests
use browser API stand-ins; they do not establish that the GPU/browser app boots.
After generating the bundle, smoke-test real browser startup, resize, import,
save/export, clipboard, unsaved confirmation and preference restoration. Report
any missing tooling or unverified runtime behavior explicitly.

Copy events must remain available to egui when label text is selected, including
status/error messages; selectable labels do not request TextEdit keyboard focus.
Diagram clipboard handling must not overwrite selected text with node TOML.


Cut uses Ctrl/Command+X or a native egui Cut event; X without Command places XOR. Serialize the selected nodes/states and internal connections
before deleting anything. Failed serialization and empty selections leave the
clipboard/document intact. Cut and paste are separate undo actions; text widgets
and selected labels retain normal clipboard behavior. Uncopied routing controls
must not be removed by cutting a component selection.

Use `Circuit::connect_overlapping_pins` for automatic attachment after placement,
paste, drag release, keyboard movement or inspector position edits. Exact interactive
pin positions determine matches, independent of zoom. Preserve occupied inputs,
existing routes, bus metadata and one-driver validation. Reuse joints; split wires
only when necessary. Output pins may drive suspended joint trees by rerooting their
joint-only parent chain; driven nets, cyclic suspended nets and ambiguous crossings
are skipped. Candidate wires incident on moved/pasted components are excluded.
Automatic attachment is part of the same history snapshot as the placement/move.
Selection clicks with unchanged positions must never create connections. Keep FSM
interaction shared while making this logic-specific model helper a no-op for FSMs.
