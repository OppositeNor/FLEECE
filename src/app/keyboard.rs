//! Shortcut bindings resolve to actions before any editor state is changed.
//! Global bindings run before text focus; canvas bindings run after clipboard
//! handling and text focus guards. Add bindings here without growing a key chain.
use super::*;
use interaction::{moved_point, stretch_incident_routes};

#[derive(Clone, Copy)]
enum Action {
    Reload,
    Help,
    Save,
    Open,
    New,
    Export,
    Search,
    Undo,
    Redo,
    Duplicate,
    SelectAll,
    Cancel,
    Select,
    Drag,
    Wire,
    Delete,
    Fit,
    Grid,
    Layout,
    CycleSelection,
}

struct Binding {
    key: Key,
    action: Action,
    command: Option<bool>,
    shift: Option<bool>,
}
impl Binding {
    const fn plain(key: Key, action: Action) -> Self {
        Self {
            key,
            action,
            command: Some(false),
            shift: None,
        }
    }
    const fn command(key: Key, action: Action) -> Self {
        Self {
            key,
            action,
            command: Some(true),
            shift: None,
        }
    }
    const fn shifted(mut self, shift: bool) -> Self {
        self.shift = Some(shift);
        self
    }
    fn matches(&self, input: &egui::InputState) -> bool {
        input.key_pressed(self.key)
            && self
                .command
                .is_none_or(|command| input.modifiers.command == command)
            && self
                .shift
                .is_none_or(|shift| input.modifiers.shift == shift)
            && (self.command != Some(false) || !input.modifiers.alt)
    }
}

const GLOBAL_BINDINGS: &[Binding] = &[
    Binding::command(Key::R, Action::Reload).shifted(true),
    Binding {
        key: Key::F1,
        action: Action::Help,
        command: None,
        shift: None,
    },
    Binding::command(Key::S, Action::Save),
    Binding::command(Key::O, Action::Open),
    Binding::command(Key::N, Action::New),
    Binding::command(Key::E, Action::Export),
    Binding::command(Key::K, Action::Search),
];
const CANVAS_BINDINGS: &[Binding] = &[
    Binding::command(Key::Z, Action::Undo),
    Binding::command(Key::Y, Action::Redo),
    Binding::command(Key::D, Action::Duplicate),
    Binding::command(Key::A, Action::SelectAll),
    Binding::plain(Key::Escape, Action::Cancel),
    Binding::plain(Key::V, Action::Select),
    Binding::plain(Key::D, Action::Drag).shifted(false),
    Binding::plain(Key::W, Action::Wire),
    Binding::plain(Key::Delete, Action::Delete),
    Binding::plain(Key::Backspace, Action::Delete),
    Binding::plain(Key::F, Action::Fit),
    Binding::plain(Key::G, Action::Grid),
    Binding::plain(Key::L, Action::Layout),
    Binding::plain(Key::Tab, Action::CycleSelection),
];

fn bound_action(ctx: &egui::Context, bindings: &[Binding]) -> Option<Action> {
    ctx.input(|input| {
        bindings
            .iter()
            .find(|binding| binding.matches(input))
            .map(|binding| binding.action)
    })
}

/// The library and keyboard both read the component catalog. Exact shifted
/// bindings take precedence over unshifted bindings, which allow a
/// Shift fallback (e.g. Shift+I still places an input).
fn component_shortcut(ctx: &egui::Context) -> Option<Kind> {
    ctx.input(|input| {
        if input.modifiers.command || input.modifiers.alt {
            return None;
        }
        let lookup = |shifted: bool| {
            Kind::ALL.into_iter().find(|kind| {
                let shortcut = kind.shortcut();
                let key = shortcut.strip_prefix("Shift+");
                key.is_some() == shifted
                    && Key::from_name(key.unwrap_or(shortcut))
                        .is_some_and(|key| input.key_pressed(key))
            })
        };
        if input.modifiers.shift {
            lookup(true).or_else(|| lookup(false))
        } else {
            lookup(false)
        }
    })
}

impl Editor {
    pub(super) fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.file_dialog.is_some()
            || self.confirm_unsaved
            || self.palette
            || self.connect_dialog
            || self.help
            || self.new_dialog
        {
            return;
        }
        if let Some(action) = bound_action(ctx, GLOBAL_BINDINGS) {
            self.perform_action(action, ctx.input(|i| i.modifiers.shift));
            return;
        }
        if ctx.egui_wants_keyboard_input() || self.clipboard_shortcuts(ctx) {
            return;
        }
        if let Some(action) = bound_action(ctx, CANVAS_BINDINGS) {
            self.perform_action(action, ctx.input(|i| i.modifiers.shift));
            return;
        }
        let delta = arrow_delta(ctx);
        if !ctx.input(|i| i.modifiers.command || i.modifiers.alt)
            && (delta.x != 0.0 || delta.y != 0.0)
        {
            self.nudge_selection(delta);
            return;
        }
        if self.circuit().fsm.is_some() {
            self.fsm_shortcuts(ctx);
            return;
        }
        self.logic_shortcuts(ctx);
    }

    fn perform_action(&mut self, action: Action, shift: bool) {
        match action {
            Action::Reload => {
                if self.external_files.is_some() {
                    self.file_dialog = Some(FileDialog::new(FileMode::Open, self.path.as_ref()));
                } else if let Some(path) = self.path.clone() {
                    self.request(Pending::Open(path));
                }
            }
            Action::Help => self.help = true,
            Action::Save => {
                if shift {
                    self.file_dialog = Some(self.save_dialog());
                } else {
                    self.save();
                }
            }
            Action::Open => {
                self.file_dialog = Some(FileDialog::new(FileMode::Open, self.path.as_ref()))
            }
            Action::New => self.new_dialog = true,
            Action::Export => self.file_dialog = Some(self.export_dialog()),
            Action::Search => {
                if self.circuit().fsm.is_none() {
                    self.palette = true;
                    self.palette_query.clear();
                    self.palette_index = 0;
                    self.palette_focus = true;
                }
            }
            Action::Undo => {
                if shift {
                    self.redo();
                } else {
                    self.undo();
                }
            }
            Action::Redo => self.redo(),
            Action::Duplicate => self.duplicate(),
            Action::SelectAll => {
                self.selected = self.node_ids().into_iter().collect();
                self.selected_wire = None;
                self.selected_bends = if self.circuit().fsm.is_none() {
                    self.all_bends().into_iter().map(|(key, _)| key).collect()
                } else {
                    BTreeSet::new()
                };
            }
            Action::Cancel => {
                self.set_mode(CanvasMode::Select);
                self.selected.clear();
                self.selected_bends.clear();
                self.selected_wire = None;
            }
            Action::Select => self.set_mode(CanvasMode::Select),
            Action::Drag => self.toggle_mode(CanvasMode::Drag),
            Action::Wire => {
                if shift && self.circuit().fsm.is_none() {
                    self.open_connect();
                } else {
                    self.toggle_mode(CanvasMode::Wire);
                }
            }
            Action::Delete => self.delete(),
            Action::Fit => self.fit_requested = true,
            Action::Grid => self.document.editor.grid = !self.document.editor.grid,
            Action::Layout => {
                if shift {
                    self.arrange();
                    self.fit_requested = true;
                } else {
                    self.document.editor.layout = match self.document.editor.layout {
                        Layout::Standard => Layout::LibraryRight,
                        Layout::LibraryRight => Layout::Focus,
                        Layout::Focus => Layout::Standard,
                    };
                }
            }
            Action::CycleSelection => self.cycle_selection(shift),
        }
    }

    fn cycle_selection(&mut self, backwards: bool) {
        let ids = self.node_ids();
        if ids.is_empty() {
            return;
        }
        let current = self
            .selected
            .first()
            .and_then(|id| ids.iter().position(|n| n == id));
        let index = match current {
            Some(i) if backwards => (i + ids.len() - 1) % ids.len(),
            Some(i) => (i + 1) % ids.len(),
            None => 0,
        };
        self.selected = BTreeSet::from([ids[index]]);
        self.selected_bends.clear();
        self.selected_wire = None;
    }

    fn nudge_selection(&mut self, delta: Point) {
        let placing = self.placing.is_some()
            || (self.circuit().fsm.is_some() && self.fsm_ui.tool == fsm::Tool::State);
        if placing || (self.selected.is_empty() && self.selected_bends.is_empty()) {
            self.cursor.x += delta.x;
            self.cursor.y += delta.y;
            return;
        }
        self.finish_drag();
        self.begin_selection_drag(Point::default());
        if !self.selected_bends.is_empty() {
            self.promote_drag_joints();
        }
        let base = self.circuit().clone();
        if let Some(fsm) = &mut self.document.circuit.fsm {
            for state in &mut fsm.states {
                if self.selected.contains(&state.id) {
                    state.position = moved_point(state.position, delta, false);
                }
            }
        } else {
            for node in &mut self.document.circuit.nodes {
                if self.selected.contains(&node.id) {
                    node.position = moved_point(node.position, delta, false);
                }
            }
            stretch_incident_routes(&base, &mut self.document.circuit);
        }
        self.finish_drag();
    }

    fn logic_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.key_pressed(Key::Enter)) && self.placing.is_some() {
            self.place(true);
            return;
        }
        if let Some(kind) = component_shortcut(ctx) {
            self.start_placing(kind);
        }
    }
}
pub(super) fn arrow_delta(ctx: &egui::Context) -> Point {
    ctx.input(|i| {
        let step = if i.modifiers.shift { GRID * 5.0 } else { GRID };
        Point::new(
            if i.key_pressed(Key::ArrowRight) {
                step
            } else if i.key_pressed(Key::ArrowLeft) {
                -step
            } else {
                0.0
            },
            if i.key_pressed(Key::ArrowDown) {
                step
            } else if i.key_pressed(Key::ArrowUp) {
                -step
            } else {
                0.0
            },
        )
    })
}
