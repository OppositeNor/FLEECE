//! Canvas interaction, wire routing handles, and diagram painting.
use super::*;

#[derive(Default)]
pub(super) struct WireGesture {
    origin: Option<Pos2>,
    horizontal_first: Option<bool>,
}

impl Editor {
    fn update_wire_direction(&mut self, ctx: &egui::Context) {
        if self.mode != CanvasMode::Wire || self.pending_pin.is_none() {
            self.wire_gesture = WireGesture::default();
            return;
        }
        if self.wire_gesture.horizontal_first.is_some() {
            return;
        }
        let navigating = ctx.input(|i| {
            i.pointer.button_down(egui::PointerButton::Middle)
                || i.key_down(Key::Space)
                || i.raw
                    .events
                    .iter()
                    .any(|e| matches!(e, egui::Event::MouseWheel { .. } | egui::Event::Zoom(_)))
        }) || matches!(self.drag, Some(Drag::Pan { .. }));
        if navigating {
            self.wire_gesture.origin = ctx.input(|i| i.pointer.hover_pos());
            return;
        }
        let Some(origin) = self.wire_gesture.origin else {
            return;
        };
        let moves: Vec<_> = ctx.input(|i| {
            i.raw
                .events
                .iter()
                .filter_map(|event| match event {
                    egui::Event::PointerMoved(position) => Some(*position),
                    _ => None,
                })
                .collect()
        });
        for position in moves {
            let delta = position - origin;
            // Ignore click jitter, and wait for a dominant axis on diagonal ties.
            if delta.length_sq() >= 9.0 && delta.x.abs() != delta.y.abs() {
                self.wire_gesture.horizontal_first = Some(delta.x.abs() > delta.y.abs());
                break;
            }
        }
    }
    fn pending_wire_horizontal_first(&self, output: bool) -> bool {
        // Reversing an input-first gesture also reverses the elbow orientation.
        self.wire_gesture
            .horizontal_first
            .is_none_or(|horizontal| if output { horizontal } else { !horizontal })
    }

    fn pin_at(&self, p: Pos2) -> Option<Pin> {
        for n in self.circuit().nodes.iter().rev() {
            if n.kind == Kind::Joint {
                continue;
            }
            for output in [false, true] {
                for port in 0..if output { n.outputs } else { n.inputs } {
                    if self.screen(n.pin(output, port)).distance(p) < 11.0 {
                        return Some(Pin {
                            endpoint: Endpoint { node: n.id, port },
                            output,
                        });
                    }
                }
            }
        }
        None
    }
    fn node_at(&self, p: Pos2) -> Option<u64> {
        self.circuit()
            .nodes
            .iter()
            .rev()
            .find(|n| {
                if n.kind == Kind::Joint {
                    return self.screen(n.position).distance(p) < 10.0;
                }
                let size = n.size();
                Rect::from_min_max(
                    self.screen(n.position),
                    self.screen(Point::new(n.position.x + size.x, n.position.y + size.y)),
                )
                .contains(p)
            })
            .map(|n| n.id)
    }
    fn wire_at(&self, p: Pos2) -> Option<usize> {
        self.circuit()
            .wires
            .iter()
            .enumerate()
            .rev()
            .find(|(_, w)| {
                wire_points(self.circuit(), w)
                    .windows(2)
                    .any(|s| segment_distance(p, self.screen(s[0]), self.screen(s[1])) < 7.0)
            })
            .map(|(i, _)| i)
    }
    fn click_pin(&mut self, pin: Pin, extend: bool, bubble: bool) {
        if let Some((wire, joint)) = self.pending_joint.take()
            && !pin.output
        {
            let point = self.editable_bends(wire)[joint];
            if let Some(id) = self.split_wire_joint(wire, self.screen(point)) {
                self.pending_pin = Some(Pin {
                    endpoint: Endpoint { node: id, port: 0 },
                    output: true,
                });
            } else {
                return;
            }
        }
        if bubble
            && self
                .circuit()
                .node(pin.endpoint.node)
                .is_some_and(|node| node.kind.gate())
        {
            self.change(|circuit| {
                let node = circuit.node_mut(pin.endpoint.node).unwrap();
                let value = !node.bubble(pin.output, pin.endpoint.port);
                let _ = node.set_bubble(pin.output, pin.endpoint.port, value);
            });
            self.pending_pin = None;
            self.pending_joint = None;
            self.status = "Toggled inversion bubble".into();
            return;
        }
        if let Some(mut first) = self.pending_pin {
            if self
                .circuit()
                .node(first.endpoint.node)
                .is_some_and(|n| n.kind == Kind::Joint)
                && !self
                    .circuit()
                    .wires
                    .iter()
                    .any(|w| w.to.node == first.endpoint.node)
            {
                first.output = !pin.output;
            }
            if first.output != pin.output {
                let (from, to) = if first.output {
                    (first.endpoint, pin.endpoint)
                } else {
                    (pin.endpoint, first.endpoint)
                };
                let horizontal_first = self.pending_wire_horizontal_first(first.output);
                let mut result = Ok(());
                self.change(|c| {
                    result = c.connect(from, to);
                    if result.is_ok() {
                        c.wires.last_mut().unwrap().horizontal_first = horizontal_first;
                    }
                });
                self.status = match result {
                    Ok(()) => "Connected · an output can drive several input pins".into(),
                    Err(e) => e,
                };
                self.pending_pin = None;
                self.pending_joint = None;
            } else {
                self.pending_pin = Some(pin);
            }
        } else {
            let connected = self
                .circuit()
                .wires
                .iter()
                .any(|wire| pin.matches_wire(wire));
            if connected && !extend {
                self.disconnect_pin(pin);
                self.selected_wire = None;
                self.selected_bends.clear();
                self.status = "Disconnected pin · Ctrl+Z to undo".into();
                return;
            }
            self.pending_pin = Some(pin);
            self.status =
                "Choose a pin or joint, or click empty space to suspend the wire · Esc cancels"
                    .into();
        }
    }

    pub(super) fn editable_bends(&self, index: usize) -> Vec<Point> {
        wire_joints(self.circuit(), &self.circuit().wires[index])
    }

    pub(super) fn all_bends(&self) -> Vec<((usize, usize), Point)> {
        (0..self.circuit().wires.len())
            .flat_map(|wire| {
                self.editable_bends(wire)
                    .into_iter()
                    .enumerate()
                    .map(move |(bend, point)| ((wire, bend), point))
            })
            .collect()
    }

    /// Project onto the clicked segment before splitting, preserving the route.
    fn prepare_wire_joint(&self, wire: usize, position: Pos2) -> Result<(Circuit, u64), String> {
        let mut route = wire_points(self.circuit(), &self.circuit().wires[wire]);
        route.dedup();
        let (segment, pair) = route
            .windows(2)
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                segment_distance(position, self.screen(a[0]), self.screen(a[1])).total_cmp(
                    &segment_distance(position, self.screen(b[0]), self.screen(b[1])),
                )
            })
            .ok_or("Wire has no editable segment.")?;
        let click = self.world(position);
        let (a, b) = (pair[0], pair[1]);
        let snap = self.document.editor.snap;
        let coordinate = |value: f32, a: f32, b: f32| {
            let value = if snap {
                (value / GRID).round() * GRID
            } else {
                value
            };
            value.clamp(a.min(b), a.max(b))
        };
        let point = if (a.x - b.x).abs() < 0.01 {
            Point::new(a.x, coordinate(click.y, a.y, b.y))
        } else {
            Point::new(coordinate(click.x, a.x, b.x), a.y)
        };
        if let Some(id) = self
            .circuit()
            .nodes
            .iter()
            .find(|n| {
                n.kind == Kind::Joint
                    && n.position == point
                    && (self.circuit().wires[wire].from.node == n.id
                        || self.circuit().wires[wire].to.node == n.id)
            })
            .map(|n| n.id)
        {
            return Ok((self.circuit().clone(), id));
        }
        let prefix = route[1..segment + 1]
            .iter()
            .copied()
            .filter(|p| *p != point)
            .collect();
        let suffix = route[segment + 1..route.len() - 1]
            .iter()
            .copied()
            .filter(|p| *p != point)
            .collect();
        let mut circuit = self.circuit().clone();
        let id = circuit.insert_joint(wire, point, prefix, suffix)?;
        Ok((circuit, id))
    }

    fn split_wire_joint(&mut self, wire: usize, position: Pos2) -> Option<u64> {
        match self.prepare_wire_joint(wire, position) {
            Ok((circuit, id)) => {
                self.change(|c| *c = circuit);
                self.selected = BTreeSet::from([id]);
                self.selected_wire = None;
                self.selected_bends.clear();
                Some(id)
            }
            Err(error) => {
                self.status = error;
                None
            }
        }
    }

    fn click_joint(&mut self, id: u64) {
        self.pending_joint = None;
        let driven = self.circuit().wires.iter().any(|wire| wire.to.node == id);
        if self.pending_pin.is_some_and(|pin| pin.endpoint.node == id) {
            self.pending_pin = Some(Pin {
                endpoint: Endpoint { node: id, port: 0 },
                output: driven,
            });
            return;
        }
        let pending_free_joint = self.pending_pin.is_some_and(|pin| {
            self.circuit()
                .node(pin.endpoint.node)
                .is_some_and(|n| n.kind == Kind::Joint)
                && !self
                    .circuit()
                    .wires
                    .iter()
                    .any(|w| w.to.node == pin.endpoint.node)
        });
        if self.pending_pin.is_some_and(|pin| pin.output) && !pending_free_joint && driven {
            self.status =
                "This joint already has a driver · connect to a free joint or an input pin".into();
            return;
        }
        let output = if driven && pending_free_joint {
            true
        } else {
            self.pending_pin.map_or(driven, |pin| !pin.output)
        };
        let pin = Pin {
            endpoint: Endpoint { node: id, port: 0 },
            output,
        };
        if self.pending_pin.is_some() {
            self.click_pin(pin, true, false);
        } else {
            self.pending_pin = Some(pin);
            self.status =
                "Choose a pin, joint, or empty space to extend the wire · Esc cancels".into();
        }
    }

    fn suspend_wire_at(&mut self, position: Pos2) {
        let point = self.world(position);
        let point = if self.document.editor.snap {
            point.snapped()
        } else {
            point
        };
        let result = (|| {
            let (mut circuit, pin) = if let Some((wire, joint)) = self.pending_joint {
                let origin = self
                    .editable_bends(wire)
                    .get(joint)
                    .copied()
                    .ok_or("Joint is unavailable.")?;
                let (circuit, id) = self.prepare_wire_joint(wire, self.screen(origin))?;
                (
                    circuit,
                    Pin {
                        endpoint: Endpoint { node: id, port: 0 },
                        output: true,
                    },
                )
            } else {
                (
                    self.circuit().clone(),
                    self.pending_pin.ok_or("Choose a starting pin or joint.")?,
                )
            };
            let id = circuit.suspend_wire(pin.endpoint, pin.output, point)?;
            circuit.wires.last_mut().unwrap().horizontal_first =
                self.pending_wire_horizontal_first(pin.output);
            Ok::<_, String>((circuit, id))
        })();
        match result {
            Ok((circuit, id)) => {
                self.change(|c| *c = circuit);
                self.selected = BTreeSet::from([id]);
                self.selected_wire = None;
                self.selected_bends.clear();
                self.pending_pin = None;
                self.pending_joint = None;
                self.drag = None;
                if self.mode == CanvasMode::Wire {
                    self.pending_pin = Some(Pin {
                        endpoint: Endpoint { node: id, port: 0 },
                        output: self.circuit().wires.iter().any(|w| w.to.node == id),
                    });
                }
                self.status =
                    "Wire mode · click to extend the wire · Esc returns to selection".into();
            }
            Err(error) => self.status = error,
        }
    }

    fn wire_click(&mut self, position: Pos2) {
        self.finish_edit();
        let previous_pin = self.pending_pin;
        if let Some(pin) = self.pin_at(position) {
            self.click_pin(pin, true, false);
        } else if let Some(id) = self.node_at(position) {
            let node = self.circuit().node(id).unwrap();
            if node.kind == Kind::Joint {
                self.click_joint(id);
            } else {
                let output = self.pending_pin.map_or(node.outputs > 0, |pin| !pin.output);
                if (if output { node.outputs } else { node.inputs }) > 0 {
                    self.click_pin(
                        Pin {
                            endpoint: Endpoint { node: id, port: 0 },
                            output,
                        },
                        true,
                        false,
                    );
                }
            }
        } else if let Some(index) = self.wire_at(position) {
            if let Some(id) = self.split_wire_joint(index, position) {
                self.click_joint(id);
            }
        } else if self.pending_pin.is_some() || self.pending_joint.is_some() {
            self.suspend_wire_at(position);
        } else {
            let point = self.world(position);
            let point = if self.document.editor.snap {
                point.snapped()
            } else {
                point
            };
            let mut circuit = self.circuit().clone();
            let id = circuit.add(Kind::Joint, point);
            circuit.node_mut(id).unwrap().label.clear();
            match Document::new(circuit.clone()).validate() {
                Ok(()) => {
                    self.change(|c| *c = circuit);
                    self.selected = BTreeSet::from([id]);
                    self.selected_wire = None;
                    self.selected_bends.clear();
                    self.pending_pin = Some(Pin {
                        endpoint: Endpoint { node: id, port: 0 },
                        output: true,
                    });
                    self.status =
                        "Wire mode · click to extend the wire · Esc returns to selection".into();
                }
                Err(error) => self.status = error,
            }
        }
        if self.pending_pin != previous_pin {
            self.wire_gesture = WireGesture {
                origin: self.pending_pin.map(|_| position),
                horizontal_first: None,
            };
        }
    }

    fn select_click(&mut self, position: Pos2, shift: bool, alt: bool) {
        self.finish_edit();
        if alt
            && let Some(pin) = self.pin_at(position)
            && self
                .circuit()
                .node(pin.endpoint.node)
                .is_some_and(|n| n.kind.gate())
        {
            self.click_pin(pin, true, true);
            return;
        }
        self.pending_pin = None;
        self.pending_joint = None;
        let node = self
            .node_at(position)
            .or_else(|| self.pin_at(position).map(|pin| pin.endpoint.node));
        if node.is_none_or(|id| {
            self.circuit()
                .node(id)
                .is_some_and(|node| node.kind == Kind::Joint)
        }) && self.begin_bus_drag(position)
        {
            return;
        }
        if let Some(id) = node {
            self.select_node(id, shift, position);
        } else if let Some((key, _)) = self
            .all_bends()
            .into_iter()
            .rev()
            .find(|(_, point)| self.screen(*point).distance(position) < 10.0)
        {
            if shift {
                if !self.selected_bends.insert(key) {
                    self.selected_bends.remove(&key);
                }
            } else if !self.selected_bends.contains(&key) {
                self.selected.clear();
                self.selected_bends = BTreeSet::from([key]);
            }
            self.selected_wire = None;
            self.begin_selection_drag(self.world(position));
        } else if let Some(index) = self.wire_at(position) {
            self.selected.clear();
            self.selected_bends.clear();
            self.selected_wire = Some(index);
            let mut route = wire_points(self.circuit(), &self.circuit().wires[index]);
            route.dedup();
            if let Some((segment, _)) = route.windows(2).enumerate().min_by(|(_, a), (_, b)| {
                segment_distance(position, self.screen(a[0]), self.screen(a[1])).total_cmp(
                    &segment_distance(position, self.screen(b[0]), self.screen(b[1])),
                )
            }) {
                self.drag = Some(Drag::WireSegment {
                    start: self.world(position),
                    wire: index,
                    segment,
                    route,
                    before: self.circuit().clone(),
                });
            }
        } else {
            self.begin_empty_drag(position, shift);
        }
    }

    fn disconnect_pin(&mut self, pin: Pin) {
        self.change(|circuit| circuit.wires.retain(|wire| !pin.matches_wire(wire)));
    }

    fn remove_at(&mut self, position: Pos2) {
        self.finish_edit();
        self.finish_drag();
        let status = if let Some(pin) = self.pin_at(position) {
            self.disconnect_pin(pin);
            "Disconnected pin · Ctrl+Z to undo"
        } else if let Some(id) = self.node_at(position) {
            self.change(|c| c.delete(&BTreeSet::from([id])));
            "Removed node · Ctrl+Z to undo"
        } else if let Some(wire) = self.wire_at(position) {
            self.change(|c| {
                c.wires.remove(wire);
            });
            "Disconnected wire · Ctrl+Z to undo"
        } else {
            return;
        };
        self.clean_selection();
        self.pending_pin = None;
        self.pending_joint = None;
        self.status = status.into();
    }

    pub(super) fn canvas(&mut self, ui: &mut egui::Ui) {
        if self.circuit().fsm.is_some() {
            self.fsm_canvas(ui);
            return;
        }
        let (response, painter, ctx, mouse, modal) = self.canvas_frame(ui);
        if !modal {
            self.update_wire_direction(&ctx);
            if response.secondary_clicked()
                && let Some(position) = mouse
            {
                self.remove_at(position);
            }
            let pressed = ctx.input(|i| i.pointer.primary_pressed());
            let shift = ctx.input(|i| i.modifiers.shift);
            let alt = ctx.input(|i| i.modifiers.alt);
            if let Some(p) = mouse.filter(|p| response.rect.contains(*p))
                && !self.begin_pan_gesture(&ctx, p)
                && pressed
                && response.contains_pointer()
            {
                if self.placing.is_some() {
                    self.cursor = self.world(p);
                    self.place(false);
                } else if self.mode == CanvasMode::Wire {
                    self.wire_click(p);
                } else {
                    self.select_click(p, shift, alt);
                }
            }
            self.update_canvas_drag(&ctx, mouse);
            if self.mode == CanvasMode::Select
                && self.placing.is_none()
                && self.drag.is_none()
                && let Some(position) = mouse.filter(|p| response.rect.contains(*p))
                && self.bus_at(position).is_some()
                && self.node_at(position).is_none_or(|id| {
                    self.circuit()
                        .node(id)
                        .is_some_and(|node| node.kind == Kind::Joint)
                })
            {
                ctx.set_cursor_icon(egui::CursorIcon::Grab);
            }
        }
        let scene = build_editor_scene(self.circuit(), &self.theme.canvas);
        paint_scene(
            &painter,
            &scene,
            |p| self.screen(p),
            self.document.editor.zoom,
            1.0,
        );
        for id in &self.selected {
            if let Some(n) = self.circuit().node(*id) {
                let size = n.size();
                let r = Rect::from_min_max(
                    self.screen(n.position),
                    self.screen(Point::new(n.position.x + size.x, n.position.y + size.y)),
                )
                .expand(7.0);
                painter.rect_stroke(
                    r,
                    5.0,
                    Stroke::new(1.5, self.theme.canvas.selection.egui()),
                    egui::StrokeKind::Outside,
                );
            }
        }
        if let Some(index) = self.selected_wire
            && let Some(w) = self.circuit().wires.get(index)
        {
            let points: Vec<_> = wire_points(self.circuit(), w)
                .iter()
                .map(|p| self.screen(*p))
                .collect();
            painter.add(egui::Shape::line(
                points,
                Stroke::new(4.0, self.theme.canvas.selection.egui().gamma_multiply(0.6)),
            ));
            for (bend, point) in self.editable_bends(index).iter().enumerate() {
                painter.circle_filled(self.screen(*point), 4.5, self.theme.canvas.selection.egui());
                if self.selected_bends.contains(&(index, bend)) {
                    painter.circle_stroke(
                        self.screen(*point),
                        6.0,
                        Stroke::new(1.0, self.theme.canvas.label.egui()),
                    );
                }
            }
        }
        for (key, point) in self.all_bends() {
            if self.selected_bends.contains(&key) {
                painter.circle_stroke(
                    self.screen(point),
                    6.0,
                    Stroke::new(1.5, self.theme.canvas.selection.egui()),
                );
            }
        }
        self.paint_selection_box(&painter, mouse);
        if let Some((wire, joint)) = self.pending_joint
            && let Some(point) = self.editable_bends(wire).get(joint)
        {
            let start = self.screen(*point);
            let end = mouse.unwrap_or(start);
            painter.add(egui::Shape::line(
                vec![start, Pos2::new(end.x, start.y), end],
                Stroke::new(2.0, self.theme.canvas.selection.egui()),
            ));
        }
        if let Some(pin) = self.pending_pin
            && let Some(n) = self.circuit().node(pin.endpoint.node)
        {
            let start = self.screen(n.pin(pin.output, pin.endpoint.port));
            let end = mouse.unwrap_or_else(|| self.screen(self.cursor));
            painter.circle_stroke(
                start,
                7.0,
                Stroke::new(2.0, self.theme.canvas.selection.egui()),
            );
            painter.add(egui::Shape::line(
                vec![
                    start,
                    if self.wire_gesture.horizontal_first.unwrap_or(pin.output) {
                        Pos2::new(end.x, start.y)
                    } else {
                        Pos2::new(start.x, end.y)
                    },
                    end,
                ],
                Stroke::new(2.0, self.theme.canvas.selection.egui()),
            ));
        }
        if let Some(kind) = self.placing {
            let p = if self.document.editor.snap {
                self.cursor.snapped()
            } else {
                self.cursor
            };
            let mut c = Circuit::empty();
            let mut node = Node::new(1, kind, p);
            components::configure_placement(&mut node, self.gate_inputs);
            c.nodes.push(node);
            let scene = build_editor_scene(&c, &self.theme.canvas);
            paint_scene(
                &painter,
                &scene,
                |p| self.screen(p),
                self.document.editor.zoom,
                0.55,
            );
            painter.circle_stroke(
                self.screen(p),
                5.0,
                Stroke::new(1.5, self.theme.canvas.selection.egui()),
            );
        }
        if self.circuit().nodes.is_empty() && self.placing.is_none() {
            painter.text(
                response.rect.center() - Vec2::new(0.0, 25.0),
                egui::Align2::CENTER_CENTER,
                "Build your first circuit",
                egui::FontId::proportional(26.0),
                self.theme.canvas.label.egui(),
            );
            painter.text(
                response.rect.center() + Vec2::new(0.0, 15.0),
                egui::Align2::CENTER_CENTER,
                "Press I for an input · A for an AND gate · O for an output",
                egui::FontId::proportional(15.0),
                self.theme.ui.muted.egui(),
            );
        }
        self.canvas_controls(&ctx, response.rect);
    }
}
