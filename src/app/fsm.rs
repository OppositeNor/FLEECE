//! Native state machine tools, canvas gestures, and property editing.
use super::*;

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub(super) enum Tool {
    #[default]
    Select,
    State,
    Transition,
}
#[derive(Default)]
pub(super) struct FsmUi {
    pub tool: Tool,
    pub from: Option<u64>,
}
impl Editor {
    pub(super) fn set_fsm_tool(&mut self, tool: Tool) {
        self.set_mode(match tool {
            Tool::Transition => CanvasMode::Wire,
            Tool::Select | Tool::State => CanvasMode::Select,
        });
        self.fsm_ui.tool = tool;
        self.status = match tool {
            Tool::Select => {
                "Select mode · drag empty canvas to box select · D pans · Shift extends selection"
            }
            Tool::State => "Place states · click repeatedly or Enter · arrows move · Esc selects",
            Tool::Transition => "Wire mode · click source and destination states · Esc selects",
        }
        .into();
    }
    fn fsm_state_at(&self, position: Pos2) -> Option<u64> {
        self.circuit()
            .fsm
            .as_ref()?
            .states
            .iter()
            .rev()
            .find(|s| {
                self.screen(s.position).distance(position)
                    <= STATE_RADIUS * self.document.editor.zoom
            })
            .map(|s| s.id)
    }
    fn fsm_transition_at(&self, position: Pos2) -> Option<usize> {
        let fsm = self.circuit().fsm.as_ref()?;
        fsm.transitions
            .iter()
            .enumerate()
            .rev()
            .find(|(_, transition)| {
                transition_geometry(fsm, transition).is_some_and(|g| {
                    self.screen(g.label).distance(position) < 18.0
                        || g.path.flattened().windows(2).any(|s| {
                            segment_distance(position, self.screen(s[0]), self.screen(s[1])) < 7.0
                        })
                })
            })
            .map(|(index, _)| index)
    }
    fn place_fsm_state(&mut self, advance: bool) {
        if self.circuit().fsm.as_ref().unwrap().states.len() >= 1000 {
            self.status = "Maximum 1,000 states per document".into();
            return;
        }
        let point = if self.document.editor.snap {
            self.cursor.snapped()
        } else {
            self.cursor
        };
        if !point.finite() {
            return;
        }
        let mut id = 0;
        self.change(|c| id = c.fsm.as_mut().unwrap().add_state(point));
        self.selected = BTreeSet::from([id]);
        self.selected_wire = None;
        if advance {
            self.cursor.x += 160.0;
        }
        self.status = "Place more states · click or Enter · Esc selects".into();
    }
    fn fsm_connect_click(&mut self, position: Pos2) {
        if let Some(id) = self.fsm_state_at(position) {
            if let Some(from) = self.fsm_ui.from.take() {
                if self.circuit().fsm.as_ref().unwrap().transitions.len() >= 8000 {
                    self.status = "Maximum 8,000 transitions per document".into();
                    return;
                }
                let mut result = Ok(0);
                self.change(|c| result = c.fsm.as_mut().unwrap().add_transition(from, id));
                match result {
                    Ok(_) => {
                        self.selected.clear();
                        self.selected_wire = self
                            .circuit()
                            .fsm
                            .as_ref()
                            .unwrap()
                            .transitions
                            .len()
                            .checked_sub(1);
                    }
                    Err(error) => self.status = error,
                }
            } else {
                self.fsm_ui.from = Some(id);
                self.status = "Choose destination state · same state adds a loop".into();
            }
        }
    }
    pub(super) fn arrange(&mut self) {
        self.change(|c| {
            if let Some(fsm) = &mut c.fsm {
                let columns = (fsm.states.len() as f32).sqrt().ceil().max(1.0) as usize;
                for (index, state) in fsm.states.iter_mut().enumerate() {
                    state.position = Point::new(
                        160.0 + (index % columns) as f32 * 240.0,
                        160.0 + (index / columns) as f32 * 220.0,
                    );
                }
            } else {
                c.auto_layout();
            }
        });
    }
    pub(super) fn fsm_delete(&mut self) {
        let ids = self.selected.clone();
        let transition = self.selected_wire;
        self.change(|c| {
            let fsm = c.fsm.as_mut().unwrap();
            if let Some(index) = transition {
                if index < fsm.transitions.len() {
                    fsm.transitions.remove(index);
                }
            } else {
                fsm.delete_states(&ids);
            }
        });
        self.selected.clear();
        self.selected_wire = None;
        self.fsm_ui.from = None;
    }
    pub(super) fn fsm_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.modifiers.command || i.modifiers.alt) {
            return;
        }
        if ctx.input(|i| i.key_pressed(Key::S)) {
            self.set_fsm_tool(Tool::State);
        }
        if ctx.input(|i| i.key_pressed(Key::T)) {
            self.set_fsm_tool(Tool::Transition);
        }
        if self.fsm_ui.tool == Tool::State && ctx.input(|i| i.key_pressed(Key::Enter)) {
            self.place_fsm_state(true);
        }
    }
    pub(super) fn fsm_library(&mut self, ui: &mut egui::Ui) {
        ui.add_space(12.0);
        ui.heading("FSM");
        for (tool, name) in [
            (Tool::Select, "Select / move (V)"),
            (Tool::State, "Add state (S)"),
            (Tool::Transition, "Wire / transition (W)"),
        ] {
            if ui
                .selectable_label(
                    self.mode != CanvasMode::Drag && self.fsm_ui.tool == tool,
                    name,
                )
                .clicked()
            {
                self.set_fsm_tool(tool);
            }
        }
        if ui
            .selectable_label(self.mode == CanvasMode::Drag, "Drag / pan (D)")
            .clicked()
        {
            self.toggle_mode(CanvasMode::Drag);
        }
        ui.separator();
        ui.label("Click the canvas to add states. For transitions, click a source state, then a destination. Choose the same state for a loop.");
        ui.label("Drag empty canvas to box-select. D enters Drag mode to pan. Shift extends selection. Right-click removes a state or transition. Esc returns to selection.");
        ui.separator();
        ui.label("STATES");
        let states = self.circuit().fsm.as_ref().unwrap().states.clone();
        for state in states {
            if ui
                .selectable_label(self.selected.contains(&state.id), &state.name)
                .clicked()
            {
                self.finish_edit();
                self.selected = BTreeSet::from([state.id]);
                self.selected_wire = None;
                self.set_mode(CanvasMode::Select);
            }
        }
    }
    pub(super) fn fsm_inspector(&mut self, ui: &mut egui::Ui) {
        ui.add_space(12.0);
        ui.heading("Inspector");
        let before = self.circuit().clone();
        let mut circuit = before.clone();
        let fsm = circuit.fsm.as_mut().unwrap();
        let mut changed = false;
        let mut editing = false;
        let mut remove = false;
        egui::ScrollArea::vertical().show(ui, |ui| {
            if let Some(index) = self.selected_wire.filter(|i| *i < fsm.transitions.len()) {
                let transition = &mut fsm.transitions[index];
                ui.heading("Transition");
                ui.label(format!("{} → {}", fsm.states.iter().find(|s| s.id == transition.from).map_or("?", |s| s.name.as_str()), fsm.states.iter().find(|s| s.id == transition.to).map_or("?", |s| s.name.as_str())));
                for (label, endpoint) in [("From", &mut transition.from), ("To", &mut transition.to)] {
                    egui::ComboBox::from_id_salt(("fsm_endpoint", transition.id, label))
                        .selected_text(format!("{label}: {}", fsm.states.iter().find(|s| s.id == *endpoint).map_or("?", |s| s.name.as_str())))
                        .show_ui(ui, |ui| { for state in &fsm.states {
                            changed |= ui.selectable_value(endpoint, state.id, &state.name).changed();
                        } });
                }
                for (label, text) in [("Condition / input", &mut transition.condition), ("Output (optional)", &mut transition.output)] {
                    ui.label(label);
                    let response = ui.add(egui::TextEdit::singleline(text).char_limit(160));
                    changed |= response.changed(); editing |= response.has_focus();
                }
                remove = ui.button("Delete transition").clicked();
            } else if self.selected.len() == 1 && let Some(state) = fsm.state_mut(*self.selected.first().unwrap()) {
                let id = state.id;
                ui.heading("State");
                for (label, text, limit) in [("Name", &mut state.name, 80), ("Output (optional)", &mut state.output, 160)] {
                    ui.label(label);
                    let response = ui.add(egui::TextEdit::singleline(text).id(egui::Id::unique(("fsm_state", id, label))).char_limit(limit));
                    changed |= response.changed(); editing |= response.has_focus();
                }
                changed |= ui.checkbox(&mut state.accepting, "Accepting / final state").changed();
                ui.horizontal(|ui| for (label, value) in [("X", &mut state.position.x), ("Y", &mut state.position.y)] {
                    let response = ui.add(egui::DragValue::new(value).prefix(format!("{label} ")).range(-99000.0..=99000.0));
                    changed |= response.changed(); editing |= response.has_focus() || response.dragged();
                });
                let mut initial = fsm.initial == Some(id);
                if ui.checkbox(&mut initial, "Initial state").changed() { fsm.initial = initial.then_some(id); changed = true; }
                remove = ui.button("Delete state").clicked();
            } else if self.selected.len() > 1 {
                ui.label(format!("{} states selected", self.selected.len()));
                ui.label("Drag or use arrow keys to move the group.");
                remove = ui.button("Delete selected states").clicked();
            } else {
                ui.heading("Finite State Machine");
                ui.label("Title");
                let response = ui.add(egui::TextEdit::singleline(&mut circuit.title).char_limit(200));
                changed |= response.changed(); editing |= response.has_focus();
                ui.label(format!("{} states · {} transitions", fsm.states.len(), fsm.transitions.len()));
                ui.label("State outputs describe Moore machines; transition outputs describe Mealy machines. Conditions and outputs are annotations.");
                ui.separator();
                ui.label("TRANSITIONS");
                for (index, transition) in fsm.transitions.iter().enumerate() {
                    let from = fsm.state(transition.from).map_or("?", |s| s.name.as_str());
                    let to = fsm.state(transition.to).map_or("?", |s| s.name.as_str());
                    if ui.button(format!("{from} → {to}: {}", transition.condition)).clicked() { self.selected.clear(); self.selected_wire = Some(index); }
                }
            }
        });
        if changed {
            if self.edit_before.is_none() {
                self.edit_before = Some(before);
            }
            self.document.circuit = circuit;
        }
        if !editing && !ui.ctx().input(|i| i.pointer.any_down()) {
            self.finish_edit();
        }
        if remove {
            self.fsm_delete();
        }
    }
    pub(super) fn fsm_canvas(&mut self, ui: &mut egui::Ui) {
        let (response, painter, ctx, mouse, modal) = self.canvas_frame(ui);
        if !modal {
            if response.secondary_clicked()
                && let Some(position) = mouse
            {
                if let Some(id) = self.fsm_state_at(position) {
                    self.finish_drag();
                    self.change(|c| c.fsm.as_mut().unwrap().delete_states(&BTreeSet::from([id])));
                    self.clean_selection();
                } else if let Some(index) = self.fsm_transition_at(position) {
                    self.selected.clear();
                    self.selected_wire = Some(index);
                    self.fsm_delete();
                }
            }
            let pressed = ctx.input(|i| i.pointer.primary_pressed());
            let extend = ctx.input(|i| i.modifiers.shift);
            if let Some(position) = mouse.filter(|p| response.rect.contains(*p))
                && !self.begin_pan_gesture(&ctx, position)
                && pressed
                && response.contains_pointer()
            {
                self.finish_edit();
                match self.fsm_ui.tool {
                    Tool::State => {
                        self.cursor = self.world(position);
                        self.place_fsm_state(false);
                    }
                    Tool::Transition => self.fsm_connect_click(position),
                    Tool::Select => {
                        if let Some(id) = self.fsm_state_at(position) {
                            self.select_node(id, extend, position);
                        } else if let Some(index) = self.fsm_transition_at(position) {
                            self.selected.clear();
                            self.selected_wire = Some(index);
                        } else {
                            self.begin_empty_drag(position, extend);
                        }
                    }
                }
            }
            self.update_canvas_drag(&ctx, mouse);
        }
        let fsm = self.circuit().fsm.as_ref().unwrap();
        let scene = build_fsm_scene(
            fsm,
            self.theme.canvas.gate_outline.0,
            self.theme.canvas.gate_fill.0,
            self.theme.canvas.label.0,
        );
        paint_scene(
            &painter,
            &scene,
            |point| self.screen(point),
            self.document.editor.zoom,
            1.0,
        );
        for state in &fsm.states {
            if self.selected.contains(&state.id) || self.fsm_ui.from == Some(state.id) {
                painter.circle_stroke(
                    self.screen(state.position),
                    (STATE_RADIUS + 5.0) * self.document.editor.zoom,
                    Stroke::new(2.0, self.theme.canvas.selection.egui()),
                );
            }
        }
        if let Some(index) = self.selected_wire
            && let Some(transition) = fsm.transitions.get(index)
            && let Some(g) = transition_geometry(fsm, transition)
        {
            painter.add(egui::Shape::line(
                g.path.flattened().iter().map(|p| self.screen(*p)).collect(),
                Stroke::new(4.0, self.theme.canvas.selection.egui().gamma_multiply(0.65)),
            ));
        }
        self.paint_selection_box(&painter, mouse);
        if let Some(from) = self.fsm_ui.from.and_then(|id| fsm.state(id))
            && let Some(position) = mouse
        {
            painter.line_segment(
                [self.screen(from.position), position],
                Stroke::new(1.5, self.theme.canvas.selection.egui()),
            );
        }
        if self.fsm_ui.tool == Tool::State {
            let point = if self.document.editor.snap {
                self.cursor.snapped()
            } else {
                self.cursor
            };
            let mut preview = Fsm::default();
            preview.add_state(point);
            preview.initial = None;
            preview.states[0].name = format!("S{}", fsm.states.len());
            let scene = build_fsm_scene(
                &preview,
                self.theme.canvas.gate_outline.0,
                self.theme.canvas.gate_fill.0,
                self.theme.canvas.label.0,
            );
            paint_scene(
                &painter,
                &scene,
                |p| self.screen(p),
                self.document.editor.zoom,
                0.55,
            );
        }
        if fsm.states.is_empty() && self.fsm_ui.tool != Tool::State {
            painter.text(
                response.rect.center(),
                egui::Align2::CENTER_CENTER,
                "Add a state (S), then click the canvas",
                egui::FontId::proportional(21.0),
                self.theme.canvas.label.egui(),
            );
        }
        self.canvas_controls(&ctx, response.rect);
    }
}
