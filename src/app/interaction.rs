//! Shared selection, dragging and connection-preserving route edits.
use super::*;

/// Transient gesture snapshots shared by logic and FSM editors.
/// Selection/segment variants retain one before-image for undo; panning and box
/// selection change presentation/selection only and never create history entries.
pub(super) enum Drag {
    Selection {
        start: Point,
        positions: BTreeMap<u64, Point>,
        bends: BTreeMap<usize, Vec<Point>>,
        before: Circuit,
        base: Circuit,
    },
    BusAnnotation {
        start: Pos2,
        offset: Point,
        source: usize,
        reachable: BTreeSet<usize>,
        before: Circuit,
    },
    WireSegment {
        start: Point,
        wire: usize,
        segment: usize,
        route: Vec<Point>,
        before: Circuit,
    },
    Box {
        start: Point,
        additive: bool,
    },
    Pan {
        button: egui::PointerButton,
        last: Pos2,
    },
}

pub(super) fn moved_point(origin: Point, delta: Point, snap: bool) -> Point {
    let mut point = Point::new(origin.x + delta.x, origin.y + delta.y);
    if snap && (delta.x.abs() > 0.01 || delta.y.abs() > 0.01) {
        point = point.snapped();
    }
    point.x = point.x.clamp(-99_000.0, 99_000.0);
    point.y = point.y.clamp(-99_000.0, 99_000.0);
    point
}

pub(super) fn select_item<T: Ord + Copy>(selection: &mut BTreeSet<T>, item: T, extend: bool) {
    if extend {
        if !selection.insert(item) {
            selection.remove(&item);
        }
    } else if !selection.contains(&item) {
        *selection = BTreeSet::from([item]);
    }
}

impl Editor {
    pub(super) fn node_ids(&self) -> Vec<u64> {
        if let Some(fsm) = &self.circuit().fsm {
            fsm.states.iter().map(|s| s.id).collect()
        } else {
            self.circuit().nodes.iter().map(|n| n.id).collect()
        }
    }
    pub(super) fn finish_drag(&mut self) {
        match self.drag.take() {
            Some(Drag::Selection { before, .. }) => self.record_edit_with_connections(before),
            Some(Drag::WireSegment { before, .. } | Drag::BusAnnotation { before, .. }) => {
                self.history.record(before, &self.document.circuit);
            }
            other => self.drag = other,
        }
    }

    pub(super) fn record_edit_with_connections(&mut self, before: Circuit) {
        self.connect_moved_pins(&before);
        self.history.record(before, &self.document.circuit);
    }

    fn connect_moved_pins(&mut self, before: &Circuit) {
        let moved = self
            .circuit()
            .nodes
            .iter()
            .filter(|node| {
                before
                    .node(node.id)
                    .is_some_and(|old| old.position != node.position)
            })
            .map(|node| node.id)
            .collect();
        if let Err(error) = self.document.circuit.connect_overlapping_pins(&moved) {
            self.status = format!("Automatic connection failed: {error}");
        }
    }
    pub(super) fn bus_at(&self, position: Pos2) -> Option<(usize, Point)> {
        for (index, wire) in self.circuit().wires.iter().enumerate().rev() {
            let Some(width) = wire.bus_width else {
                continue;
            };
            let Some((center, horizontal)) =
                Wire::bus_anchor(&wire_points(self.circuit(), wire), wire.bus_position)
            else {
                continue;
            };
            let label = bus_label_position(center, horizontal, width, TextStyle::Ui);
            let text_width = styled_text_width(&width.to_string(), 12.0, TextStyle::Ui);
            let label_rect = Rect::from_center_size(
                self.screen(label),
                Vec2::new(text_width + 8.0, 20.0) * self.document.editor.zoom,
            );
            let slash_distance = segment_distance(
                position,
                self.screen(Point::new(center.x - 5.0, center.y + 7.0)),
                self.screen(Point::new(center.x + 5.0, center.y - 7.0)),
            );
            if label_rect.contains(position) || slash_distance < 6.0 {
                return Some((index, center));
            }
        }
        None
    }

    pub(super) fn begin_bus_drag(&mut self, position: Pos2) -> bool {
        let Some((index, center)) = self.bus_at(position) else {
            return false;
        };
        let world = self.world(position);
        self.drag = Some(Drag::BusAnnotation {
            start: position,
            offset: Point::new(world.x - center.x, world.y - center.y),
            source: index,
            reachable: self.circuit().joint_connected_wires(index),
            before: self.circuit().clone(),
        });
        self.selected.clear();
        self.selected_bends.clear();
        self.selected_wire = Some(index);
        true
    }

    pub(super) fn begin_selection_drag(&mut self, start: Point) {
        let positions = if let Some(fsm) = &self.circuit().fsm {
            fsm.states
                .iter()
                .filter(|s| self.selected.contains(&s.id))
                .map(|s| (s.id, s.position))
                .collect()
        } else {
            self.circuit()
                .nodes
                .iter()
                .filter(|n| self.selected.contains(&n.id))
                .map(|n| (n.id, n.position))
                .collect()
        };
        let bends = if self.circuit().fsm.is_some() {
            BTreeMap::new()
        } else {
            self.selected_bends
                .iter()
                .map(|(wire, _)| (*wire, self.editable_bends(*wire)))
                .collect()
        };
        self.drag = Some(Drag::Selection {
            start,
            positions,
            bends,
            before: self.circuit().clone(),
            base: self.circuit().clone(),
        });
    }
    pub(super) fn begin_empty_drag(&mut self, position: Pos2, extend: bool) {
        self.selected_wire = None;
        if !extend {
            self.selected.clear();
            self.selected_bends.clear();
        }
        self.drag = Some(Drag::Box {
            start: self.world(position),
            additive: extend,
        });
    }

    pub(super) fn begin_pan_gesture(&mut self, ctx: &egui::Context, position: Pos2) -> bool {
        let (middle, middle_down, space_pan) = ctx.input(|i| {
            (
                i.pointer.button_pressed(egui::PointerButton::Middle),
                i.pointer.button_down(egui::PointerButton::Middle),
                i.pointer.primary_pressed()
                    && (i.key_down(Key::Space) || self.mode == CanvasMode::Drag),
            )
        });
        if middle || space_pan {
            self.finish_edit();
            self.finish_drag();
            self.drag = Some(Drag::Pan {
                button: if middle {
                    egui::PointerButton::Middle
                } else {
                    egui::PointerButton::Primary
                },
                last: position,
            });
            true
        } else {
            middle_down
        }
    }
    pub(super) fn select_node(&mut self, id: u64, extend: bool, position: Pos2) {
        if !extend && !self.selected.contains(&id) {
            self.selected_bends.clear();
        }
        select_item(&mut self.selected, id, extend);
        self.selected_wire = None;
        self.begin_selection_drag(self.world(position));
    }
    pub(super) fn promote_drag_joints(&mut self) {
        let mut circuit = self.circuit().clone();
        let mut promoted = Vec::new();
        for &(wire, bend) in self.selected_bends.iter().rev() {
            let Some(point) = self.editable_bends(wire).get(bend).copied() else {
                return;
            };
            let mut route = wire_points(&circuit, &circuit.wires[wire]);
            route.dedup();
            let Some(index) = route.iter().position(|p| *p == point) else {
                return;
            };
            if index == 0 || index + 1 == route.len() {
                continue;
            }
            let result = circuit.insert_joint(
                wire,
                point,
                route[1..index].to_vec(),
                route[index + 1..route.len() - 1].to_vec(),
            );
            let Ok(id) = result else {
                self.status = "Cannot add more routing joints to this document".into();
                return;
            };
            promoted.push((id, point));
        }
        if let Some(Drag::Selection {
            positions,
            bends,
            base,
            ..
        }) = &mut self.drag
        {
            for (id, point) in promoted {
                positions.insert(id, point);
                self.selected.insert(id);
            }
            bends.clear();
            *base = circuit.clone();
        }
        self.selected_bends.clear();
        self.document.circuit = circuit;
    }
    /// Apply movement from the gesture origin/snapshot, not the previous frame.
    /// This prevents accumulated rounding drift and commits the release position
    /// before recording the whole drag as one action.
    pub(super) fn update_canvas_drag(&mut self, ctx: &egui::Context, mouse: Option<Pos2>) {
        let (down, released) = ctx.input(|i| {
            let button = match &self.drag {
                Some(Drag::Pan { button, .. }) => *button,
                _ => egui::PointerButton::Primary,
            };
            (
                i.pointer.button_down(button),
                i.pointer.button_released(button),
            )
        });
        if (down || released)
            && let Some(position) = mouse
        {
            let world = self.world(position);
            let snap = self.document.editor.snap;
            // A click only selects. The first actual movement promotes routing
            // controls to persistent nodes, in the same undo action as the drag.
            if let Some(Drag::Selection { start, .. }) = &self.drag
                && (world.x - start.x).abs().max((world.y - start.y).abs()) > 0.01
                && !self.selected_bends.is_empty()
            {
                self.promote_drag_joints();
            }
            match &self.drag {
                Some(Drag::Pan { last, .. }) => {
                    self.pan_by(position - *last);
                    if let Some(Drag::Pan { last, .. }) = &mut self.drag {
                        *last = position;
                    }
                    self.cursor = self.world(position);
                    ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                }
                Some(Drag::Selection {
                    start,
                    positions,
                    bends,
                    before,
                    base,
                }) => {
                    let delta = Point::new(world.x - start.x, world.y - start.y);
                    let moved = delta.x.abs() > 0.01 || delta.y.abs() > 0.01;
                    if moved {
                        self.pending_pin = None;
                        self.pending_joint = None;
                    }
                    for (&wire, points) in bends {
                        let mut points = points.clone();
                        for (bend, point) in points.iter_mut().enumerate() {
                            if self.selected_bends.contains(&(wire, bend)) {
                                *point = moved_point(*point, delta, snap);
                            }
                        }
                        self.document.circuit.wires[wire].waypoints = if moved {
                            points
                        } else {
                            before.wires[wire].waypoints.clone()
                        };
                    }
                    if let Some(fsm) = &mut self.document.circuit.fsm {
                        for state in &mut fsm.states {
                            if let Some(origin) = positions.get(&state.id) {
                                state.position = moved_point(*origin, delta, snap);
                            }
                        }
                    } else {
                        for node in &mut self.document.circuit.nodes {
                            if let Some(origin) = positions.get(&node.id) {
                                node.position = moved_point(*origin, delta, snap);
                            }
                        }
                        stretch_incident_routes(base, &mut self.document.circuit);
                    }
                }
                Some(Drag::BusAnnotation {
                    start,
                    offset,
                    source,
                    reachable,
                    before,
                }) => {
                    ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                    if position.distance(*start) > 3.0 {
                        let point = Point::new(world.x - offset.x, world.y - offset.y);
                        let width = before.wires[*source].bus_width;
                        let candidate = reachable
                            .iter()
                            .filter_map(|&index| {
                                let wire = &before.wires[index];
                                if wire.bus_width.is_some() && wire.bus_width != width {
                                    return None;
                                }
                                Wire::project_bus_position(&wire_points(before, wire), point)
                                    .map(|(distance, fraction)| (index, distance, fraction))
                            })
                            .min_by(|a, b| a.1.total_cmp(&b.1));
                        if let Some((target, distance, fraction)) = candidate
                            && distance * self.document.editor.zoom <= 24.0
                        {
                            let mut circuit = before.clone();
                            if circuit
                                .move_bus_annotation(*source, target, fraction)
                                .is_ok()
                            {
                                self.document.circuit = circuit;
                                self.selected_wire = Some(target);
                            }
                        }
                    } else {
                        self.document.circuit = before.clone();
                        self.selected_wire = Some(*source);
                    }
                }
                Some(Drag::WireSegment {
                    start,
                    wire,
                    segment,
                    route,
                    before,
                }) => {
                    let horizontal = route[*segment].y == route[*segment + 1].y;
                    ctx.set_cursor_icon(if horizontal {
                        egui::CursorIcon::ResizeVertical
                    } else {
                        egui::CursorIcon::ResizeHorizontal
                    });
                    let delta = if horizontal {
                        Point::new(0.0, world.y - start.y)
                    } else {
                        Point::new(world.x - start.x, 0.0)
                    };
                    if delta.x.abs().max(delta.y.abs()) < 0.01 {
                        self.document.circuit.wires[*wire] = before.wires[*wire].clone();
                    } else {
                        let mut points = route.clone();
                        let original = points[*segment];
                        let target = moved_point(original, delta, snap);
                        for point in &mut points[*segment..=*segment + 1] {
                            if horizontal {
                                point.y = target.y;
                            } else {
                                point.x = target.x;
                            }
                        }
                        if *segment == 0 {
                            points.insert(0, route[0]);
                        }
                        if *segment + 2 == route.len() {
                            points.push(*route.last().unwrap());
                        }
                        points.dedup();
                        // Leave room for the two endpoint connectors in the file limit.
                        if points.len() <= 66 {
                            self.document.circuit.wires[*wire].waypoints =
                                points[1..points.len() - 1].to_vec();
                        }
                    }
                }
                _ => {}
            }
        }
        if !down && let Some(drag) = self.drag.take() {
            match drag {
                Drag::Selection { before, .. } => self.record_edit_with_connections(before),
                Drag::WireSegment { before, .. } | Drag::BusAnnotation { before, .. } => {
                    self.history.record(before, &self.document.circuit)
                }
                Drag::Box { start, additive } => {
                    if let Some(position) = mouse {
                        let region = Rect::from_two_pos(self.screen(start), position);
                        if !additive {
                            self.selected.clear();
                            self.selected_bends.clear();
                        }
                        let ids: Vec<_> = if let Some(fsm) = &self.circuit().fsm {
                            fsm.states
                                .iter()
                                .filter(|s| {
                                    region.intersects(Rect::from_center_size(
                                        self.screen(s.position),
                                        Vec2::splat(STATE_RADIUS * 2.0 * self.document.editor.zoom),
                                    ))
                                })
                                .map(|s| s.id)
                                .collect()
                        } else {
                            let bends: Vec<_> = self
                                .all_bends()
                                .into_iter()
                                .filter(|(_, point)| region.contains(self.screen(*point)))
                                .map(|(key, _)| key)
                                .collect();
                            self.selected_bends.extend(bends);
                            self.circuit()
                                .nodes
                                .iter()
                                .filter(|n| {
                                    let size = n.size();
                                    region.intersects(Rect::from_min_max(
                                        self.screen(n.position),
                                        self.screen(Point::new(
                                            n.position.x + size.x,
                                            n.position.y + size.y,
                                        )),
                                    ))
                                })
                                .map(|n| n.id)
                                .collect()
                        };
                        self.selected.extend(ids);
                    }
                }
                Drag::Pan { .. } => {}
            }
        }
    }
}

pub(super) fn segment_distance(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let d = b - a;
    let t = if d.length_sq() > 0.0 {
        ((p - a).dot(d) / d.length_sq()).clamp(0.0, 1.0)
    } else {
        0.0
    };
    p.distance(a + d * t)
}

/// Reconnect moved endpoints locally; interior controls and unrelated wires stay
/// fixed. Work from the gesture snapshot so repeated pointer events cannot drift.
pub(super) fn stretch_incident_routes(base: &Circuit, circuit: &mut Circuit) {
    for (index, original) in base.wires.iter().enumerate() {
        let Some(from) = circuit.node(original.from.node) else {
            continue;
        };
        let Some(to) = circuit.node(original.to.node) else {
            continue;
        };
        let start = from.pin(true, original.from.port);
        let end = to.pin(false, original.to.port);
        let route = wire_points(base, original);
        if route.len() < 2 {
            continue;
        }
        let old_start = route[0];
        let old_end = *route.last().unwrap();
        if start == old_start && end == old_end {
            circuit.wires[index] = original.clone();
            continue;
        }
        // A whole selected subgraph translates as a unit, including its route.
        let delta = Point::new(start.x - old_start.x, start.y - old_start.y);
        if delta == Point::new(end.x - old_end.x, end.y - old_end.y) {
            circuit.wires[index].waypoints = original
                .waypoints
                .iter()
                .map(|p| Point::new(p.x + delta.x, p.y + delta.y))
                .collect();
            continue;
        }
        if original.waypoints.is_empty() && route.len() > 2 {
            // Automatic elbows stretch within the edge between persistent joints.
            // Freezing a computed elbow would introduce backtracking segments.
            circuit.wires[index] = original.clone();
            continue;
        }
        if route.len() == 2 {
            // A straight edge needs just one elbow when its endpoint moves.
            circuit.wires[index].waypoints.clear();
            circuit.wires[index].horizontal_first = old_start.y == old_end.y;
            continue;
        }
        let mut points = route[1..route.len() - 1].to_vec();
        let first = points[0];
        if start != old_start {
            let corner = if old_start.y == first.y {
                Point::new(first.x, start.y)
            } else {
                Point::new(start.x, first.y)
            };
            points.insert(0, corner);
        }
        let last = *points.last().unwrap();
        if end != old_end {
            let corner = if old_end.y == route[route.len() - 2].y {
                Point::new(last.x, end.y)
            } else {
                Point::new(end.x, last.y)
            };
            points.push(corner);
        }
        points.dedup();
        if points.len() <= 64 {
            circuit.wires[index].waypoints = points;
        }
    }
}
