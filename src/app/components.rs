//! Component library and repeated placement use the model's component catalog.
use super::*;

pub(super) fn configure_placement(node: &mut Node, inputs: usize) {
    if node.kind.configurable_gate_inputs() {
        node.resize_ports(inputs, 1);
    }
}

impl Editor {
    pub(super) fn start_placing(&mut self, kind: Kind) {
        self.set_mode(CanvasMode::Select);
        self.placing = Some(kind);
        self.pending_pin = None;
        self.pending_joint = None;
        self.status = format!(
            "Place {} · click repeatedly or Enter · arrows move · Esc exits",
            kind.name()
        );
    }
    pub(super) fn place(&mut self, advance: bool) {
        if let Some(kind) = self.placing {
            if self.circuit().nodes.len() >= 1000 {
                self.status = "Maximum 1,000 blocks per document".into();
                return;
            }
            let p = if self.document.editor.snap {
                self.cursor.snapped()
            } else {
                self.cursor
            };
            let mut id = 0;
            let inputs = self.gate_inputs;
            let mut attached = Ok(0);
            self.change(|c| {
                id = c.add(kind, p);
                configure_placement(c.node_mut(id).unwrap(), inputs);
                attached = c.connect_overlapping_pins(&BTreeSet::from([id]));
            });
            self.selected = BTreeSet::from([id]);
            self.selected_bends.clear();
            self.selected_wire = None;
            if advance {
                self.cursor.x += 160.0;
            }
            self.status = match attached {
                Ok(_) => format!(
                    "Place more {} · click or Enter · Esc exits placement",
                    kind.name()
                ),
                Err(error) => format!(
                    "Placed {} · automatic connection failed: {error}",
                    kind.name()
                ),
            };
        }
    }
    pub(super) fn library(&mut self, ui: &mut egui::Ui) {
        if self.circuit().fsm.is_some() {
            self.fsm_library(ui);
            return;
        }
        ui.add_space(12.0);
        ui.label(
            RichText::new("COMPONENTS")
                .small()
                .color(self.accent())
                .strong(),
        );
        ui.label(
            RichText::new("Choose a block, then place it")
                .small()
                .weak(),
        );
        ui.add_space(10.0);
        if ui
            .add_sized(
                [ui.available_width(), 30.0],
                egui::Button::new("Wire (W)").selected(self.mode == CanvasMode::Wire),
            )
            .clicked()
        {
            self.set_mode(CanvasMode::Wire);
        }
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            for group in ComponentGroup::LIBRARY {
                ui.label(RichText::new(group.title()).small().weak());
                if group == ComponentGroup::Gates {
                    ui.horizontal(|ui| {
                        ui.label("Inputs");
                        ui.selectable_value(&mut self.gate_inputs, 2, "2");
                        ui.selectable_value(&mut self.gate_inputs, 3, "3");
                    });
                    ui.label(RichText::new("XOR stays at 2 inputs").small().weak());
                }
                for kind in Kind::ALL
                    .into_iter()
                    .filter(|kind| kind.definition().group == group)
                {
                    let selected = self.placing == Some(kind);
                    if ui
                        .add_sized(
                            [ui.available_width(), 30.0],
                            egui::Button::new(format!("{:<17}  {}", kind.name(), kind.shortcut()))
                                .selected(selected),
                        )
                        .clicked()
                    {
                        self.start_placing(kind);
                    }
                }
                ui.add_space(12.0);
            }
            ui.separator();
            ui.label(
                RichText::new(
                    "Ctrl+K to search\nW for Wire mode · V to select\nF to fit the circuit",
                )
                .small()
                .weak(),
            );
        });
    }
}
