//! Native editor state, document lifecycle and top-level layout.
//!
//! The editor owns a persisted document plus transient selection, dialogs, modes,
//! clipboard placement and gesture state. Only circuit snapshots enter undo/redo;
//! viewport and application preferences are managed separately.
//!
//! Focused child modules implement components, keyboard actions, mutually exclusive
//! modes, shared interactions, painting, persistence and domain-specific panels.
//! Logic and FSM canvases share navigation/drag helpers instead of separate input
//! loops. Use `change` for discrete model edits; text edits and drags collect one
//! snapshot for their whole gesture.

mod canvas;
mod clipboard;
mod components;
pub mod configuration;
mod dialogs;
mod export_preview;
mod files;
mod fsm;
pub use files::FileRequest;
mod inspector;
mod interaction;
mod keyboard;
mod mode;
mod painting;
pub mod preferences;
mod utilities;

use interaction::{Drag, segment_distance};
use mode::CanvasMode;
use painting::paint_scene;

use crate::{
    drawing::*,
    editor_drawing::build_editor_scene,
    export::{self, ExportOptions},
    history::History,
    model::*,
    theme::{self, Theme},
};
use egui::{self, Color32, Key, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    time::{Duration, SystemTime},
};
use web_time::Instant;

/// Install embedded diagram fonts and the editor font-family mappings in an egui context.
/// Call before the first frame when embedding an editor.
pub fn configure_context(context: &egui::Context) {
    let mut fonts = egui::FontDefinitions::empty();
    fonts.font_data.insert(
        "diagram".into(),
        std::sync::Arc::new(egui::FontData::from_static(FONT_BYTES)),
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts.families.insert(family, vec!["diagram".into()]);
    }
    for (style, bytes) in [
        (TextStyle::Ui, FONT_BYTES),
        (TextStyle::Serif, SERIF_BYTES),
        (TextStyle::Math, MATH_BYTES),
        (TextStyle::Heading, HEADING_BYTES),
    ] {
        let name = style.family();
        fonts.font_data.insert(
            name.into(),
            std::sync::Arc::new(egui::FontData::from_static(bytes)),
        );
        fonts
            .families
            .insert(egui::FontFamily::Name(name.into()), vec![name.into()]);
    }
    context.set_fonts(fonts);
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct Pin {
    endpoint: Endpoint,
    output: bool,
}
impl Pin {
    fn matches_wire(self, wire: &Wire) -> bool {
        if self.output {
            wire.from == self.endpoint
        } else {
            wire.to == self.endpoint
        }
    }
}
#[derive(Clone)]
enum Pending {
    New(bool),
    Demo,
    Open(PathBuf),
    Import {
        document: Box<Document>,
        name: String,
    },
    Quit,
}
#[derive(Clone, Copy, PartialEq)]
enum FileMode {
    Open,
    Save,
    Export,
    Theme,
}
struct FileDialog {
    mode: FileMode,
    directory: PathBuf,
    directory_text: String,
    name: String,
    error: String,
    overwrite: bool,
    focus_name: bool,
}
impl FileDialog {
    fn new(mode: FileMode, initial: Option<&PathBuf>) -> Self {
        let directory = initial
            .and_then(|p| p.parent())
            .filter(|p| !p.as_os_str().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        let name = initial
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| {
                if mode == FileMode::Theme {
                    "theme.toml".into()
                } else if mode == FileMode::Export {
                    "circuit.png".into()
                } else {
                    "circuit.toml".into()
                }
            });
        Self {
            mode,
            directory_text: directory.display().to_string(),
            directory,
            name,
            error: String::new(),
            overwrite: false,
            focus_name: true,
        }
    }
}

/// Document editor plus transient UI state. Use [`Editor::new`] for an isolated
/// embedded editor or [`Editor::with_native_preferences`] in a desktop host.
pub struct Editor {
    /// Editable document and saved view settings. Mutations through editor actions
    /// participate in undo/redo; direct mutations by an embedder do not automatically do so.
    pub document: Document,
    /// Current native path, or a logical download name for host-managed files.
    /// `None` identifies an unnamed document.
    pub path: Option<PathBuf>,
    saved: Circuit,
    external_files: Option<files::ExternalFiles>,
    preferences: preferences::Preferences,
    preferences_path: Option<PathBuf>,
    theme_store: Option<configuration::ThemeStore>,
    theme: Theme,
    theme_key: String,
    remembered_theme: Option<String>,
    file_stamp: Option<(SystemTime, u64)>,
    external_changed: bool,
    last_file_check: Instant,
    history: History,
    selected: BTreeSet<u64>,
    selected_wire: Option<usize>,
    selected_bends: BTreeSet<(usize, usize)>,
    last_paste: String,
    paste_count: u32,
    last_paste_anchor: Option<Point>,
    placing: Option<Kind>,
    gate_inputs: usize,
    mode: CanvasMode,
    cursor: Point,
    pointer_last: Option<Pos2>,
    pending_pin: Option<Pin>,
    wire_gesture: canvas::WireGesture,
    pending_joint: Option<(usize, usize)>,
    drag: Option<Drag>,
    edit_before: Option<Circuit>,
    canvas_rect: Rect,
    fit_requested: bool,
    file_dialog: Option<FileDialog>,
    pending: Option<Pending>,
    confirm_unsaved: bool,
    palette: bool,
    new_dialog: bool,
    fsm_ui: fsm::FsmUi,
    palette_query: String,
    palette_index: usize,
    palette_focus: bool,
    connect_dialog: bool,
    connect_from: Endpoint,
    connect_to: Endpoint,
    help: bool,
    /// Set after a close request is accepted, including any unsaved-work confirmation.
    /// The native host should then close its window/event loop.
    pub should_quit: bool,
    status: String,
    export_options: ExportOptions,
    export_preview: Option<export_preview::ExportPreview>,
}
impl Editor {
    /// Create an editor without accessing native application preferences.
    /// The supplied document selects the initial theme; no window/GPU is created.
    pub fn new(document: Document, path: Option<PathBuf>) -> Self {
        let mut editor = Self {
            saved: document.circuit.clone(),
            external_files: None,
            preferences: preferences::Preferences::default(),
            preferences_path: None,
            theme_store: None,
            theme: Theme::original(true),
            theme_key: String::new(),
            remembered_theme: None,
            file_stamp: path.as_ref().and_then(file_stamp),
            external_changed: false,
            last_file_check: Instant::now(),
            document,
            path,
            history: History::default(),
            selected: BTreeSet::new(),
            selected_wire: None,
            selected_bends: BTreeSet::new(),
            last_paste: String::new(),
            paste_count: 0,
            last_paste_anchor: None,
            placing: None,
            gate_inputs: 2,
            mode: CanvasMode::Select,
            cursor: Point::new(200.0, 160.0),
            pointer_last: None,
            pending_pin: None,
            wire_gesture: canvas::WireGesture::default(),
            pending_joint: None,
            drag: None,
            edit_before: None,
            canvas_rect: Rect::NOTHING,
            fit_requested: false,
            file_dialog: None,
            pending: None,
            confirm_unsaved: false,
            palette: false,
            new_dialog: false,
            fsm_ui: fsm::FsmUi::default(),
            palette_query: String::new(),
            palette_index: 0,
            palette_focus: false,
            connect_dialog: false,
            connect_from: Endpoint { node: 0, port: 0 },
            connect_to: Endpoint { node: 0, port: 0 },
            help: false,
            should_quit: false,
            status: "Ready · I: input  A: AND  O: output  Ctrl+K: component search  F1: shortcuts"
                .into(),
            export_options: ExportOptions::default(),
            export_preview: None,
        };
        editor.sync_theme();
        editor
    }
    /// Whether diagram content differs from the last saved snapshot.
    /// Viewport, selection and theme changes alone do not make the circuit dirty.
    pub fn dirty(&self) -> bool {
        self.document.circuit != self.saved
    }

    /// Load and apply a built-in key or custom theme path. Native editors import
    /// custom files into their managed theme directory and persist the choice.
    ///
    /// # Errors
    /// Returns an error for missing/invalid themes or failed configuration writes.
    /// The previous active palette and recorded choice are preserved on failure.
    pub fn select_theme(&mut self, reference: &str) -> Result<(), String> {
        if self.external_files.is_some() && theme::builtin_source(reference).is_none() {
            return Err("Import a TOML theme to use a custom browser palette.".into());
        }
        let (reference, theme) = if let Some(store) = &self.theme_store {
            store.select(reference)?
        } else {
            let reference = if theme::BUILT_INS.iter().any(|(key, _)| *key == reference) {
                reference.to_owned()
            } else {
                std::fs::canonicalize(reference)
                    .map_err(|error| error.to_string())?
                    .to_string_lossy()
                    .into_owned()
            };
            let theme = Theme::load_reference(&reference)?;
            (reference, theme)
        };

        self.document.editor.theme = Some(reference.clone());
        self.theme_key = reference;
        self.status = format!("UI theme: {}", theme.name);
        self.theme = theme;
        self.remembered_theme = Some(self.theme_key.clone());
        if let Some(files) = &mut self.external_files {
            files.custom_theme = None;
        }
        Ok(())
    }

    /// Borrow the currently resolved UI palette and metrics.
    pub fn active_theme(&self) -> &Theme {
        &self.theme
    }

    /// Reload the active theme from disk and remember a successful selection.
    ///
    /// # Errors
    /// Returns the same loading/configuration errors as [`Self::select_theme`].
    /// A failed reload keeps the previous active palette.
    pub fn reload_theme(&mut self) -> Result<(), String> {
        let reference = self.theme_reference();
        self.select_theme(&reference)
    }

    fn theme_reference(&self) -> String {
        self.document
            .editor
            .theme
            .clone()
            .unwrap_or_else(|| "original-dark".into())
    }

    fn sync_theme(&mut self) {
        let reference = self.theme_reference();
        if reference == self.theme_key {
            return;
        }
        match Theme::load_reference(&reference) {
            Ok(theme) => {
                self.theme = theme;
            }
            Err(error) => {
                self.theme = Theme::original(true);
                self.status = format!("Cannot load UI theme; using original palette: {error}");
            }
        }
        self.theme_key = reference;
    }

    fn accent(&self) -> Color32 {
        self.theme.ui.accent.egui()
    }
    /// Format the native window title, including a marker for unsaved diagram content.
    pub fn title(&self) -> String {
        format!(
            "{}{} — FLEECE",
            if self.dirty() { "● " } else { "" },
            self.document.circuit.title
        )
    }
    fn circuit(&self) -> &Circuit {
        &self.document.circuit
    }
    /// Commit one discrete diagram edit to snapshot history. Pending drag/text edits
    /// are finished first; the mutation does not run simulation. Callers enforce
    /// their structural constraints before exposing an invalid document.
    fn change(&mut self, f: impl FnOnce(&mut Circuit)) {
        self.finish_edit();
        self.finish_drag();
        let before = self.circuit().clone();
        f(&mut self.document.circuit);
        self.history.record(before, &self.document.circuit);
    }
    /// Close a coalesced inspector edit, recording one snapshot for an entire
    /// text/coordinate edit instead of one history entry per frame.
    fn finish_edit(&mut self) {
        if let Some(before) = self.edit_before.take() {
            self.record_edit_with_connections(before);
        }
    }
    fn undo(&mut self) {
        self.finish_edit();
        self.finish_drag();
        self.drag = None;
        self.pending_pin = None;
        self.pending_joint = None;
        if self.history.undo(&mut self.document.circuit) {
            self.clean_selection();
            self.status = "Undone".into();
        }
    }
    fn redo(&mut self) {
        self.finish_edit();
        self.finish_drag();
        self.drag = None;
        self.pending_pin = None;
        self.pending_joint = None;
        if self.history.redo(&mut self.document.circuit) {
            self.clean_selection();
            self.status = "Redone".into();
        }
    }
    fn clean_selection(&mut self) {
        self.fsm_ui.from = None;
        let ids: BTreeSet<_> = if let Some(fsm) = &self.circuit().fsm {
            fsm.states.iter().map(|s| s.id).collect()
        } else {
            self.circuit().nodes.iter().map(|n| n.id).collect()
        };
        self.selected.retain(|id| ids.contains(id));
        self.selected_wire = None;
        self.selected_bends.clear();
    }
    fn replace(&mut self, doc: Document, path: Option<PathBuf>) {
        self.file_stamp = if self.external_files.is_none() {
            path.as_ref().and_then(file_stamp)
        } else {
            None
        };
        self.external_changed = false;
        self.document = doc;
        self.saved = self.circuit().clone();
        self.path = path;
        self.history = History::default();
        self.selected.clear();
        self.selected_wire = None;
        self.pending_pin = None;
        self.pending_joint = None;
        self.selected_bends.clear();
        self.placing = None;
        self.mode = CanvasMode::Select;
        self.last_paste.clear();
        self.paste_count = 0;
        self.last_paste_anchor = None;
        self.drag = None;
        self.edit_before = None;
        self.fit_requested = false;
        self.fsm_ui = fsm::FsmUi::default();
        self.apply_remembered_theme();
    }
    /// Request a new logic document, confirming before discarding unsaved work.
    pub fn new_logic_circuit(&mut self) {
        self.request(Pending::New(false));
    }
    /// Request a new FSM document, confirming before discarding unsaved work.
    pub fn new_fsm(&mut self) {
        self.request(Pending::New(true));
    }
    fn request(&mut self, action: Pending) {
        self.finish_edit();
        self.finish_drag();
        if self.dirty() {
            self.pending = Some(action);
            self.confirm_unsaved = true;
        } else {
            self.perform(action);
        }
    }
    fn perform(&mut self, action: Pending) {
        match action {
            Pending::New(fsm) => {
                self.replace(
                    Document::new(if fsm {
                        Circuit::new_fsm()
                    } else {
                        Circuit::empty()
                    }),
                    None,
                );
                self.status = if fsm {
                    "New FSM · S: state · W: wire transitions · Esc: select"
                } else {
                    "New logic circuit"
                }
                .into();
            }
            Pending::Demo => {
                self.replace(Document::new(Circuit::half_adder()), None);
                self.fit_requested = true;
                self.status = "Half-adder example · edit components, joints and wires".into();
            }
            Pending::Open(path) => match Document::load(&path) {
                Ok(doc) => {
                    self.replace(doc, Some(path));
                    self.status = "Document loaded".into();
                }
                Err(e) => self.status = format!("Open failed: {e}"),
            },
            Pending::Import { document, name } => {
                self.replace(*document, Some(PathBuf::from(name)));
                self.fit_requested = true;
                self.status = "Document imported".into();
            }
            Pending::Quit => self.should_quit = true,
        }
    }
    /// Request application shutdown through the unsaved-work confirmation flow.
    pub fn request_close(&mut self) {
        self.request(Pending::Quit);
    }
    /// Request loading a document through the unsaved-work confirmation flow.
    /// Load errors are reported in the editor status without replacing the document.
    pub fn open_path(&mut self, path: PathBuf) {
        self.request(Pending::Open(path));
    }
    fn save(&mut self) {
        if self.external_files.is_some() {
            self.save_external();
            return;
        }
        self.finish_edit();
        self.finish_drag();
        if let Some(path) = self.path.clone() {
            if self.external_changed || file_stamp(&path) != self.file_stamp {
                self.status =
                    "File changed on disk. Reload with Ctrl+Shift+R, or use Save as.".into();
                return;
            }
            match self.document.save(&path) {
                Ok(()) => {
                    self.saved = self.circuit().clone();
                    self.remember_save_path(&path);
                    self.file_stamp = file_stamp(&path);
                    self.status = format!("Saved {}", path.display());
                    self.after_save();
                }
                Err(e) => self.status = format!("Save failed: {e}"),
            }
        } else {
            self.file_dialog = Some(self.save_dialog());
        }
    }

    /// Adopt external edits when clean; protect local changes from being overwritten.
    /// Host-managed file editors do not monitor filesystem paths.
    pub fn check_external_file(&mut self) {
        if self.external_files.is_some() {
            return;
        }
        let Some(path) = self.path.clone() else {
            return;
        };
        let stamp = file_stamp(&path);
        if stamp == self.file_stamp {
            return;
        }
        if self.dirty() || self.edit_before.is_some() || self.drag.is_some() {
            self.external_changed = true;
            self.status = "File changed on disk. Reload with Ctrl+Shift+R, or use Save as.".into();
            return;
        }
        match Document::load(&path) {
            Ok(mut document) => {
                document.editor = self.document.editor.clone();
                self.replace(document, Some(path));
                self.status = "Reloaded external changes · editor view preserved".into();
            }
            Err(error) => {
                self.file_stamp = stamp;
                self.external_changed = true;
                self.status = format!("Cannot reload external changes: {error}");
            }
        }
    }
    fn after_save(&mut self) {
        if let Some(action) = self.pending.take() {
            self.confirm_unsaved = false;
            self.perform(action);
        }
    }
    fn delete(&mut self) {
        if self.circuit().fsm.is_some() {
            self.fsm_delete();
            return;
        }
        if !self.selected_bends.is_empty() || !self.selected.is_empty() {
            let mut controls: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
            for &(wire, bend) in &self.selected_bends {
                controls.entry(wire).or_default().insert(bend);
            }
            let ids = self.selected.clone();
            let mut result = Ok(());
            self.change(|c| {
                let mut working = c.clone();
                for (wire, bends) in controls {
                    if let Err(error) = working.remove_wire_controls(wire, &bends) {
                        result = Err(error);
                        return;
                    }
                }
                working.delete(&ids);
                *c = working;
            });
            if let Err(error) = result {
                self.status = error;
            }
            self.selected.clear();
            self.selected_bends.clear();
            self.selected_wire = None;
            self.pending_pin = None;
            self.pending_joint = None;
            return;
        }
        if let Some(index) = self.selected_wire.take() {
            self.change(|c| {
                if index < c.wires.len() {
                    c.wires.remove(index);
                }
            });
        } else {
            let ids = self.selected.clone();
            self.change(|c| c.delete(&ids));
            self.selected.clear();
        }
        self.pending_pin = None;
        self.pending_joint = None;
    }
    fn duplicate(&mut self) {
        let ids = self.selected.clone();
        let mut selected = BTreeSet::new();
        if self.node_ids().len() + ids.len() > 1000 {
            return;
        }
        self.change(|c| selected = c.duplicate(&ids));
        self.selected = selected;
        self.selected_bends.clear();
        self.selected_wire = None;
    }
    fn open_connect(&mut self) {
        self.connect_from = self
            .selected
            .iter()
            .filter_map(|id| self.circuit().node(*id))
            .find(|n| n.outputs > 0)
            .or_else(|| self.circuit().nodes.iter().find(|n| n.outputs > 0))
            .map(|n| Endpoint {
                node: n.id,
                port: 0,
            })
            .unwrap_or(Endpoint { node: 0, port: 0 });
        self.connect_to = self
            .circuit()
            .nodes
            .iter()
            .find(|n| n.inputs > 0 && n.id != self.connect_from.node)
            .map(|n| Endpoint {
                node: n.id,
                port: 0,
            })
            .unwrap_or(Endpoint { node: 0, port: 0 });
        self.connect_dialog = true;
    }
    /// Draw one editor frame and process its shortcuts, panels, canvas and dialogs.
    /// Native input conversion, clipboard transport and window lifecycle are host tasks.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if self.last_file_check.elapsed() >= Duration::from_secs(1) {
            self.check_external_file();
            self.last_file_check = Instant::now();
        }
        self.sync_theme();
        ctx.set_global_style(self.theme.style());
        ui.set_style(ctx.global_style());
        self.shortcuts(&ctx);
        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.add_space(5.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("FLEECE")
                        .color(self.accent())
                        .strong()
                        .size(21.0),
                );
                ui.separator();
                ui.menu_button("File", |ui| {
                    ui.menu_button("New", |ui| {
                        if ui.button("Logic Circuit").clicked() {
                            self.new_logic_circuit();
                            ui.close();
                        }
                        if ui.button("FSM (Finite State Machine)").clicked() {
                            self.new_fsm();
                            ui.close();
                        }
                    });
                    if ui.button("Open…              Ctrl+O").clicked() {
                        self.file_dialog =
                            Some(FileDialog::new(FileMode::Open, self.path.as_ref()));
                        ui.close();
                    }
                    if ui.button("Save                 Ctrl+S").clicked() {
                        self.save();
                        ui.close();
                    }
                    if ui.button("Save as…      Ctrl+Shift+S").clicked() {
                        self.file_dialog = Some(self.save_dialog());
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Half-adder example").clicked() {
                        self.request(Pending::Demo);
                        ui.close();
                    }
                    if ui.button("Export image…   Ctrl+E").clicked() {
                        self.file_dialog = Some(self.export_dialog());
                        ui.close();
                    }
                });
                if ui
                    .add_enabled(self.history.can_undo(), egui::Button::new("↶ Undo"))
                    .clicked()
                {
                    self.undo();
                }
                if ui
                    .add_enabled(self.history.can_redo(), egui::Button::new("↷ Redo"))
                    .clicked()
                {
                    self.redo();
                }
                ui.separator();
                if self.circuit().fsm.is_some() {
                    if ui.button("+ State (S)").clicked() {
                        self.set_fsm_tool(fsm::Tool::State);
                    }
                } else if ui.button("+ Component").clicked() {
                    self.palette = true;
                    self.palette_query.clear();
                    self.palette_index = 0;
                    self.palette_focus = true;
                }
                let selecting = self.mode == CanvasMode::Select
                    && self.placing.is_none()
                    && (self.circuit().fsm.is_none() || self.fsm_ui.tool == fsm::Tool::Select);
                if ui.selectable_label(selecting, "Select (V)").clicked() {
                    self.set_mode(CanvasMode::Select);
                }
                if ui
                    .selectable_label(self.mode == CanvasMode::Drag, "Drag (D)")
                    .clicked()
                {
                    self.toggle_mode(CanvasMode::Drag);
                }
                if ui.button("Export image").clicked() {
                    self.file_dialog = Some(self.export_dialog());
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("? F1").clicked() {
                        self.help = true;
                    }
                    ui.menu_button("View", |ui| {
                        for (layout, name) in [
                            (Layout::Standard, "Library left"),
                            (Layout::LibraryRight, "Library right"),
                            (Layout::Focus, "Focus canvas"),
                        ] {
                            ui.selectable_value(&mut self.document.editor.layout, layout, name);
                        }
                        ui.separator();
                        ui.checkbox(&mut self.document.editor.grid, "Grid (G)");
                        ui.checkbox(&mut self.document.editor.snap, "Snap to grid");
                        ui.menu_button("Theme", |ui| {
                            for (key, name) in theme::BUILT_INS {
                                if ui
                                    .selectable_label(self.theme_reference() == key, name)
                                    .clicked()
                                {
                                    if let Err(error) = self.select_theme(key) {
                                        self.status = format!("Theme failed: {error}");
                                    }
                                    ui.close();
                                }
                            }
                            ui.separator();
                            if ui.button("Load TOML theme…").clicked() {
                                let initial = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                                    .join("themes")
                                    .join("tokyo-night.toml");
                                self.file_dialog =
                                    Some(FileDialog::new(FileMode::Theme, Some(&initial)));
                                ui.close();
                            }
                            if self.external_files.is_none()
                                && ui.button("Reload theme from disk").clicked()
                            {
                                if let Err(error) = self.reload_theme() {
                                    self.status = format!("Theme reload failed: {error}");
                                }
                                ui.close();
                            }
                        });
                        if ui.button("Fit circuit (F)").clicked() {
                            self.fit_requested = true;
                            ui.close();
                        }
                        if ui.button("Arrange circuit (Shift+L)").clicked() {
                            self.arrange();
                            self.fit_requested = true;
                            ui.close();
                        }
                    });
                });
            });
            ui.add_space(5.0);
        });
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::Label::new(RichText::new(&self.status).small()).selectable(true))
                    .on_hover_text(&self.status)
                    .context_menu(|ui| {
                        if ui.button("Copy message").clicked() {
                            ui.ctx().copy_text(self.status.clone());
                            ui.close();
                        }
                    });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(fsm) = &self.circuit().fsm {
                        ui.label(format!(
                            "{} states · {} transitions · {:.0}%",
                            fsm.states.len(),
                            fsm.transitions.len(),
                            self.document.editor.zoom * 100.0
                        ));
                    } else {
                        ui.label(format!(
                            "{} blocks · {} wires · {:.0}%",
                            self.circuit().nodes.len(),
                            self.circuit().wires.len(),
                            self.document.editor.zoom * 100.0
                        ));
                    }
                });
            });
        });
        let layout = self.document.editor.layout;
        if layout != Layout::Focus {
            let library = if layout == Layout::Standard {
                egui::Panel::left("library_left")
            } else {
                egui::Panel::right("library_right")
            };
            library
                .default_size(210.0)
                .min_size(170.0)
                .max_size(340.0)
                .resizable(true)
                .show(ui, |ui| self.library(ui));
            let inspector = if layout == Layout::Standard {
                egui::Panel::right("inspector_right")
            } else {
                egui::Panel::left("inspector_left")
            };
            inspector
                .default_size(255.0)
                .min_size(220.0)
                .max_size(450.0)
                .resizable(true)
                .show(ui, |ui| self.inspector(ui));
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| self.canvas(ui));
        self.dialogs(&ctx);
    }
}

fn to_color(c: Color) -> Color32 {
    Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
}

fn file_stamp(path: &PathBuf) -> Option<(SystemTime, u64)> {
    let metadata = std::fs::metadata(path).ok()?;
    Some((metadata.modified().ok()?, metadata.len()))
}
