//! Diagram property panels for components, wires and joints.
use super::*;

impl Editor {
    pub(super) fn inspector(&mut self, ui: &mut egui::Ui) {
        if self.circuit().fsm.is_some() {
            self.fsm_inspector(ui);
            return;
        }
        ui.add_space(12.0);
        ui.label(
            RichText::new("INSPECTOR")
                .small()
                .color(self.accent())
                .strong(),
        );
        ui.add_space(8.0);

        egui::ScrollArea::vertical().show(ui, |ui| {
            if let Some(index) = self
                .selected_wire
                .filter(|i| *i < self.circuit().wires.len())
            {
                self.wire_inspector(ui, index);
            } else if !self.selected_bends.is_empty() {
                ui.heading(format!(
                    "{} blocks, {} joints selected",
                    self.selected.len(),
                    self.selected_bends.len()
                ));
                ui.label(
                    "Drag or use arrows to move the group. Shift+click adds or removes items.",
                );
                if ui.button("Delete (Del)").clicked() {
                    self.delete();
                }
            } else if self.selected.len() == 1 {
                self.node_inspector(ui, *self.selected.first().unwrap());
            } else if self.selected.len() > 1 {
                ui.heading(format!("{} selected", self.selected.len()));
                ui.label("Drag or use arrows to move the group.");
                if ui.button("Duplicate (Ctrl+D)").clicked() {
                    self.duplicate();
                }
                if ui.button("Delete (Del)").clicked() {
                    self.delete();
                }
            } else {
                self.circuit_inspector(ui);
            }
        });
    }

    fn wire_inspector(&mut self, ui: &mut egui::Ui, index: usize) {
        let wire = self.circuit().wires[index].clone();
        ui.heading("Wire");
        ui.label(format!(
            "Block {} / {} → {} / {}",
            wire.from.node, wire.from.port, wire.to.node, wire.to.port
        ));
        let mut horizontal_first = wire.horizontal_first;
        if ui
            .checkbox(&mut horizontal_first, "Horizontal first")
            .changed()
        {
            self.change(|c| {
                c.wires[index].horizontal_first = horizontal_first;
                c.wires[index].waypoints.clear();
            });
            self.selected_bends.clear();
        }
        let mut editing = false;
        let mut bus = wire.bus_width.is_some();
        let mut width = wire.bus_width.unwrap_or(2);
        let mut bus_changed = ui.checkbox(&mut bus, "Bus annotation").changed();
        if bus {
            let response = ui.add(
                egui::DragValue::new(&mut width)
                    .range(1..=MAX_BUS_WIDTH)
                    .prefix("Width "),
            );
            bus_changed |= response.changed();
            editing |= response.has_focus() || response.dragged();
            ui.label("Drag the slash or width along connected wires.");
        }
        if bus_changed {
            if self.edit_before.is_none() {
                self.edit_before = Some(self.circuit().clone());
            }
            if let Err(error) = self
                .document
                .circuit
                .set_wire_bus_width(wire.to, bus.then_some(width))
            {
                self.status = error;
            }
        }
        let source = self.circuit().net_source(wire.from);
        let mut color = self
            .circuit()
            .node(source.node)
            .unwrap()
            .net_color(source.port);
        if Self::color_picker(ui, ("wire_color", index), &mut color) {
            self.change(|circuit| {
                circuit
                    .node_mut(source.node)
                    .unwrap()
                    .set_net_color(source.port, color)
            });
        }
        ui.label(
            RichText::new("Color applies to every branch from this output.")
                .small()
                .weak(),
        );
        ui.label("Use Wire mode (W) to add joints and branches. In Select mode (V), drag a joint freely or drag a wire segment sideways. Connections stay attached; Ctrl+Z undoes the drag.");
        let mut points = self.editable_bends(index);
        let mut changed = false;
        let mut remove = None;
        ui.label(RichText::new("JOINTS").small().weak());
        for (bend, point) in points.iter_mut().enumerate() {
            ui.push_id(("wire_joint", wire.to.node, wire.to.port, bend), |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(
                            self.selected_bends.contains(&(index, bend)),
                            format!("{}", bend + 1),
                        )
                        .clicked()
                    {
                        self.selected.clear();
                        self.selected_bends = BTreeSet::from([(index, bend)]);
                    }
                    for (label, value) in [("X", &mut point.x), ("Y", &mut point.y)] {
                        let response = ui.add(
                            egui::DragValue::new(value)
                                .prefix(format!("{label} "))
                                .range(-100000.0..=100000.0)
                                .speed(1.0),
                        );
                        changed |= response.changed();
                        editing |= response.has_focus() || response.dragged();
                    }
                    if ui.small_button("×").on_hover_text("Remove joint").clicked() {
                        remove = Some(bend);
                    }
                });
            });
        }
        if changed {
            if self.edit_before.is_none() {
                self.edit_before = Some(self.circuit().clone());
            }
            self.document.circuit.wires[index].waypoints = points.clone();
        }
        if !editing && !ui.ctx().input(|input| input.pointer.any_down()) {
            self.finish_edit();
        }
        if let Some(bend) = remove {
            let mut result = Ok(());
            self.change(|circuit| {
                result = circuit.remove_wire_controls(index, &BTreeSet::from([bend]))
            });
            if let Err(error) = result {
                self.status = error;
            }
            self.selected_bends.clear();
        }
        if ui.button("Reset route").clicked() {
            self.change(|circuit| circuit.wires[index].waypoints.clear());
            self.selected_bends.clear();
        }
        if ui.button("Delete wire").clicked() {
            self.selected_bends.clear();
            self.delete();
        }
    }

    fn joint_inspector(&mut self, ui: &mut egui::Ui, mut node: Node) {
        let id = node.id;
        ui.heading("Joint");
        ui.label("In Wire mode (W), click a joint, then a pin or empty space to extend a wire. Right-click removes the joint. Drag or use arrows to move attached wires together. All joints show solid dots while editing. Image exports hide dots for joints with exactly two wires.");
        let edges: Vec<_> = self
            .circuit()
            .wires
            .iter()
            .enumerate()
            .filter(|(_, w)| w.from.node == node.id || w.to.node == node.id)
            .map(|(i, w)| (i, w.from.node == node.id))
            .collect();
        ui.label(format!("{} attached wires", edges.len()));
        let before = self.circuit().clone();
        let mut changed = false;
        let mut editing = false;
        ui.horizontal(|ui| {
            for (label, coordinate) in [("X", &mut node.position.x), ("Y", &mut node.position.y)] {
                let response = ui.add(
                    egui::DragValue::new(coordinate)
                        .prefix(format!("{label} "))
                        .range(-99000.0..=99000.0)
                        .speed(1.0),
                );
                changed |= response.changed();
                editing |= response.has_focus() || response.dragged();
            }
        });
        if changed {
            if self.edit_before.is_none() {
                self.edit_before = Some(before);
            }
            *self.document.circuit.node_mut(id).unwrap() = node;
            let base = self.edit_before.as_ref().unwrap();
            interaction::stretch_incident_routes(base, &mut self.document.circuit);
        }
        if !editing && !ui.ctx().input(|input| input.pointer.any_down()) {
            self.finish_edit();
        }
        for (index, outgoing) in edges {
            if ui
                .button(format!(
                    "{} wire {}",
                    if outgoing { "Outgoing" } else { "Incoming" },
                    index + 1
                ))
                .clicked()
            {
                self.selected.clear();
                self.selected_wire = Some(index);
                self.pending_pin = None;
                self.pending_joint = None;
            }
        }
        if ui.button("Remove joint (preserve connections)").clicked() {
            self.delete();
        }
    }

    fn node_inspector(&mut self, ui: &mut egui::Ui, id: u64) {
        let Some(mut node) = self.circuit().node(id).cloned() else {
            return;
        };
        if node.kind == Kind::Joint {
            self.joint_inspector(ui, node);
            return;
        }
        let before = self.circuit().clone();
        ui.heading(node.kind.name());
        ui.label(RichText::new(format!("Block #{id}")).small().weak());
        ui.add_space(8.0);
        ui.label("Label");
        let label = ui.add(
            egui::TextEdit::singleline(&mut node.label)
                .id(egui::Id::unique(("node_label", id)))
                .char_limit(80)
                .desired_width(f32::INFINITY),
        );
        let mut changed = label.changed();
        let text_active = label.has_focus();
        label.on_hover_text(r"Math labels: s_0, s_1', \bar{s_1}, x^{2}");

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            for (name, value) in [("X", &mut node.position.x), ("Y", &mut node.position.y)] {
                ui.label(name);
                changed |= ui
                    .add(
                        egui::DragValue::new(value)
                            .range(-99000.0..=99000.0)
                            .speed(2.0),
                    )
                    .changed();
            }
        });
        if node.kind.variable_inputs() && node.kind != Kind::Xor {
            let mut count = node.inputs;
            changed |= ui
                .add(egui::Slider::new(&mut count, 2..=6).text("Inputs"))
                .changed();
            if count != node.inputs {
                node.resize_ports(count, node.outputs);
            }
        }
        if node.kind == Kind::Module {
            let mut count = node.outputs;
            changed |= ui
                .add(egui::Slider::new(&mut count, 1..=4).text("Outputs"))
                .changed();
            if count != node.outputs {
                node.resize_ports(node.inputs, count);
            }
        }
        let old_select_bits = node.mux_select_bits();
        let mut select_bits = old_select_bits;
        if node.kind == Kind::Mux {
            changed |= ui
                .add(egui::Slider::new(&mut select_bits, 1..=3).text("Selection bits"))
                .changed();
            ui.label(format!(
                "{} data inputs · S₀ is the lowest selection bit",
                1 << select_bits
            ));
        }
        let old_bits = node.outputs;
        let mut register_bits = old_bits;
        if node.kind == Kind::Register {
            changed |= ui
                .add(egui::Slider::new(&mut register_bits, 1..=8).text("Bits"))
                .changed();
            ui.label("Set the number of data inputs and Q outputs shown on the component.");
        }

        ui.add_space(8.0);
        ui.label(RichText::new("PINS").small().weak());
        if node.kind.gate() {
            ui.label(
                RichText::new("Alt+click a gate pin to toggle its inversion bubble.")
                    .small()
                    .weak(),
            );
        }
        for output in [false, true] {
            let count = if output { node.outputs } else { node.inputs };
            for port in 0..count {
                ui.label(format!(
                    "{} {}",
                    if output { "Out" } else { "In" },
                    node.port_name(output, port)
                ));
                if node.kind.gate() {
                    let mut bubble = node.bubble(output, port);
                    if ui.checkbox(&mut bubble, "Inversion bubble").changed() {
                        let _ = node.set_bubble(output, port, bubble);
                        changed = true;
                    }
                }
                if output {
                    let mut color = node.net_color(port);
                    if Self::color_picker(ui, ("output_color", id, port), &mut color) {
                        node.set_net_color(port, color);
                        changed = true;
                    }
                }
            }
        }
        if changed {
            if self.edit_before.is_none() {
                self.edit_before = Some(before);
            }
            *self.document.circuit.node_mut(id).unwrap() = node;
            if register_bits != old_bits {
                let _ = self.document.circuit.resize_register(id, register_bits);
            }
            if select_bits != old_select_bits {
                let _ = self.document.circuit.resize_mux(id, select_bits);
            }
            self.document.circuit.prune_wires();
        }
        if !text_active && !ui.ctx().input(|input| input.pointer.any_down()) {
            self.finish_edit();
        }
        ui.separator();
        if ui.button("Connect… (W)").clicked() {
            self.open_connect();
        }
        if ui.button("Duplicate (Ctrl+D)").clicked() {
            self.duplicate();
        }
        if ui.button("Delete (Del)").clicked() {
            self.delete();
        }
    }

    fn color_picker(ui: &mut egui::Ui, id: impl egui::AsIdSalt, color: &mut NetColor) -> bool {
        let mut changed = false;
        egui::ComboBox::from_id_salt(id)
            .selected_text(color.name())
            .width(130.0)
            .show_ui(ui, |ui| {
                for option in NetColor::ALL {
                    let text_color = if option == NetColor::Black {
                        ui.visuals().text_color()
                    } else {
                        to_color(net_color(option))
                    };
                    let text = RichText::new(option.name()).color(text_color);
                    changed |= ui.selectable_value(color, option, text).changed();
                }
            });
        changed
    }

    fn circuit_inspector(&mut self, ui: &mut egui::Ui) {
        ui.heading("Circuit");
        let before = self.circuit().clone();
        let title = ui.add(
            egui::TextEdit::singleline(&mut self.document.circuit.title)
                .id(egui::Id::unique("circuit_title"))
                .char_limit(200)
                .desired_width(f32::INFINITY),
        );
        if title.changed() && self.edit_before.is_none() {
            self.edit_before = Some(before);
        }
        if title.lost_focus() {
            self.finish_edit();
        }
        ui.add_space(12.0);
        ui.label("Select a block to edit its label, pins, position, or wire colors.");
        ui.add_space(12.0);
        ui.separator();
        ui.label("W enters Wire mode to connect pins, joints, or empty canvas. V or Esc returns to Select mode. Drag empty canvas to box-select; D enters Drag mode to pan. Ctrl+C / Ctrl+V copies and pastes selected blocks.");
        ui.add_space(8.0);
        if ui.button("Arrange circuit (Shift+L)").clicked() {
            self.change(|circuit| circuit.auto_layout());
            self.fit_requested = true;
        }
        if ui.button("Fit circuit (F)").clicked() {
            self.fit_requested = true;
        }
    }
}
