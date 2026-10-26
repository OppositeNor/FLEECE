mod support;

use egui::{Event, Key, Modifiers, Pos2, Rect, Vec2};
use fleece::{
    app::{Editor, configure_context},
    model::*,
};

struct Harness {
    context: egui::Context,
    editor: Editor,
    time: f64,
    clipboard: String,
    canvas_rect: Rect,
    text_positions: Vec<(String, Pos2)>,
}
impl Harness {
    fn new() -> Self {
        let context = egui::Context::default();
        configure_context(&context);
        let mut harness = Self {
            context,
            editor: Editor::new(Document::new(Circuit::empty()), None),
            time: 0.0,
            clipboard: String::new(),
            canvas_rect: Rect::NOTHING,
            text_positions: Vec::new(),
        };
        harness.frame(vec![]);
        harness
    }
    fn frame(&mut self, events: Vec<Event>) {
        self.time += 1.0 / 60.0;
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 900.0))),
            events,
            time: Some(self.time),
            ..Default::default()
        };
        let mut output = self.context.run_ui(input, |ui| self.editor.ui(ui));
        assert!(!output.shapes.is_empty());
        self.text_positions = output
            .shapes
            .iter()
            .filter_map(|shape| {
                if let egui::Shape::Text(text) = &shape.shape {
                    Some((
                        text.galley.text().to_owned(),
                        text.pos + text.galley.size() / 2.0,
                    ))
                } else {
                    None
                }
            })
            .collect();
        for command in &output.platform_output.commands {
            if let egui::OutputCommand::CopyText(text) = command {
                self.clipboard = text.clone();
            }
        }
        self.canvas_rect = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect)
                    if rect.fill == self.editor.active_theme().canvas.background.egui() =>
                {
                    Some(rect.rect)
                }
                _ => None,
            })
            .max_by(|a, b| a.area().total_cmp(&b.area()))
            .unwrap();
        output.textures_delta.clear();
    }
    fn key(&mut self, key: Key, modifiers: Modifiers) {
        let mut events = vec![
            Event::ModifiersChanged(modifiers),
            Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            },
        ];
        if modifiers.command {
            match key {
                Key::C => events.push(Event::Copy),
                Key::X => events.push(Event::Cut),
                Key::V => events.push(Event::Paste(self.clipboard.clone())),
                _ => {}
            }
        }
        self.frame(events);
        self.frame(vec![
            Event::Key {
                key,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers,
            },
            Event::ModifiersChanged(Modifiers::NONE),
        ]);
    }

    fn focus(&mut self, id: egui::Id) {
        self.context.memory_mut(|memory| memory.request_focus(id));
        self.frame(vec![]);
        assert!(self.context.egui_wants_keyboard_input());
    }

    fn type_key(&mut self, key: Key, text: &str) {
        self.frame(vec![
            Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
            Event::Text(text.into()),
        ]);
        self.frame(vec![Event::Key {
            key,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: Modifiers::NONE,
        }]);
    }

    fn screen(&self, point: Point) -> Pos2 {
        let view = &self.editor.document.editor;
        self.canvas_rect.min
            + egui::vec2(
                view.pan.x + point.x * view.zoom,
                view.pan.y + point.y * view.zoom,
            )
    }

    fn pointer(&mut self, point: Point, pressed: bool, modifiers: Modifiers) {
        let pos = self.screen(point);
        self.frame(vec![
            Event::ModifiersChanged(modifiers),
            Event::PointerMoved(pos),
            Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers,
            },
        ]);
    }

    fn right_pointer(&mut self, point: Point, pressed: bool) {
        let pos = self.screen(point);
        self.frame(vec![
            Event::PointerMoved(pos),
            Event::PointerButton {
                pos,
                button: egui::PointerButton::Secondary,
                pressed,
                modifiers: Modifiers::NONE,
            },
        ]);
    }
    fn right_click(&mut self, point: Point) {
        self.right_pointer(point, true);
        self.right_pointer(point, false);
    }

    fn click(&mut self, point: Point, modifiers: Modifiers) {
        self.time += 0.5;
        self.pointer(point, true, modifiers);
        self.pointer(point, false, modifiers);
    }
}

#[test]
fn copied_blocks_paste_with_internal_wires_and_independent_ids_and_history() {
    let mut harness = Harness::new();
    harness.editor.document = Document::new(Circuit::half_adder());
    harness
        .editor
        .document
        .circuit
        .node_mut(3)
        .unwrap()
        .set_bubble(false, 0, true)
        .unwrap();
    harness
        .editor
        .document
        .circuit
        .node_mut(1)
        .unwrap()
        .set_net_color(0, NetColor::Red);
    harness.frame(vec![]);
    let original = harness.editor.document.circuit.clone();
    harness.key(Key::A, Modifiers::COMMAND);
    harness.key(Key::C, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit, original);
    assert!(!harness.clipboard.is_empty());
    let anchor = Point::new(360.0, 320.0);
    harness.frame(vec![Event::PointerMoved(harness.screen(anchor))]);
    let origin = original
        .nodes
        .iter()
        .fold(Point::new(f32::INFINITY, f32::INFINITY), |p, n| {
            Point::new(p.x.min(n.position.x), p.y.min(n.position.y))
        });
    let offset = Point::new(anchor.x - origin.x, anchor.y - origin.y);
    harness.key(Key::V, Modifiers::COMMAND);
    let pasted = harness.editor.document.circuit.clone();
    assert_eq!(pasted.nodes.len(), 12);
    assert_eq!(pasted.wires.len(), 12);
    for (source, copy) in original.nodes.iter().zip(&pasted.nodes[6..]) {
        let mut expected = source.clone();
        expected.id += 6;
        expected.position.x += offset.x;
        expected.position.y += offset.y;
        assert_eq!(*copy, expected);
    }
    for (source, copy) in original.wires.iter().zip(&pasted.wires[6..]) {
        let mut expected = source.clone();
        expected.from.node += 6;
        expected.to.node += 6;
        for point in &mut expected.waypoints {
            point.x += offset.x;
            point.y += offset.y;
        }
        assert_eq!(*copy, expected);
    }
    harness.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit, original);
    harness.key(Key::Y, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit, pasted);
    harness.key(Key::ArrowRight, Modifiers::NONE);
    assert_eq!(harness.editor.document.circuit.nodes[..6], original.nodes);
    harness.key(Key::V, Modifiers::COMMAND);
    assert_eq!(
        harness.editor.document.circuit.nodes[12].position.x,
        original.nodes[0].position.x + offset.x + 40.0
    );
    let next_anchor = Point::new(600.0, 500.0);
    harness.frame(vec![Event::PointerMoved(harness.screen(next_anchor))]);
    harness.key(Key::V, Modifiers::COMMAND);
    assert_eq!(
        harness.editor.document.circuit.nodes[18].position,
        Point::new(
            original.nodes[0].position.x + next_anchor.x - origin.x,
            original.nodes[0].position.y + next_anchor.y - origin.y
        )
    );
    harness.editor.document.validate().unwrap();
}

#[test]
fn clipboard_can_move_partial_selections_between_documents_and_leaves_text_edits_to_widgets() {
    let mut source = Harness::new();
    source.editor.document = Document::new(Circuit::half_adder());
    source.frame(vec![]);
    source.click(Point::new(100.0, 104.0), Modifiers::SHIFT);
    source.click(Point::new(410.0, 92.0), Modifiers::SHIFT);
    source.key(Key::C, Modifiers::COMMAND);
    let fragment = Document::from_toml(&source.clipboard).unwrap();
    assert_eq!(fragment.circuit.nodes.len(), 2);
    assert_eq!(fragment.circuit.wires.len(), 1);

    let mut target = Harness::new();
    target.key(Key::I, Modifiers::NONE);
    target.key(Key::Enter, Modifiers::NONE);
    target.clipboard = source.clipboard;
    target.key(Key::V, Modifiers::COMMAND);
    assert_eq!(target.editor.document.circuit.nodes.len(), 3);
    assert_eq!(target.editor.document.circuit.wires[0].from.node, 2);
    assert_eq!(target.editor.document.circuit.wires[0].to.node, 3);
    target.editor.document.validate().unwrap();

    target.key(Key::Escape, Modifiers::NONE);
    target.focus(egui::Id::unique("circuit_title"));
    target.key(Key::A, Modifiers::COMMAND);
    target.key(Key::C, Modifiers::COMMAND);
    assert_eq!(target.clipboard, "Untitled circuit");
    target.clipboard = "Pasted text".into();
    target.key(Key::V, Modifiers::COMMAND);
    assert_eq!(target.editor.document.circuit.title, "Pasted text");
    assert_eq!(target.editor.document.circuit.nodes.len(), 3);
}

#[test]
fn normal_mode_selects_pins_and_wire_mode_preserves_existing_fanout() {
    let mut harness = Harness::new();
    harness.editor.document = Document::new(Circuit::half_adder());
    harness.frame(vec![]);
    let original = harness.editor.document.circuit.clone();
    let target = original.node(3).unwrap().pin(false, 0);
    let source = original.node(2).unwrap().pin(true, 0);
    harness.click(target, Modifiers::NONE);
    harness.click(source, Modifiers::NONE);
    assert_eq!(harness.editor.document.circuit, original);
    harness.key(Key::W, Modifiers::NONE);
    harness.click(source, Modifiers::NONE);
    assert_eq!(harness.editor.document.circuit, original);
    harness.click(target, Modifiers::NONE);
    assert_eq!(
        harness.editor.document.circuit.wires.len(),
        original.wires.len()
    );
    assert!(
        harness
            .editor
            .document
            .circuit
            .wires
            .iter()
            .any(|w| w.from == Endpoint { node: 2, port: 0 }
                && w.to == Endpoint { node: 3, port: 0 })
    );
    assert!(
        harness
            .editor
            .document
            .circuit
            .wires
            .iter()
            .any(|w| w.from == Endpoint { node: 2, port: 0 }
                && w.to == Endpoint { node: 4, port: 1 })
    );
    harness.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit, original);
}

#[test]
fn gate_pin_bubbles_toggle_without_disconnects_and_are_undoable() {
    let mut harness = Harness::new();
    harness.editor.document = Document::new(Circuit::half_adder());
    harness.frame(vec![]);
    let original = harness.editor.document.circuit.clone();
    harness.click(original.node(4).unwrap().pin(false, 0), Modifiers::ALT);
    assert!(
        harness
            .editor
            .document
            .circuit
            .node(4)
            .unwrap()
            .bubble(false, 0)
    );
    assert_eq!(harness.editor.document.circuit.wires, original.wires);
    harness.click(original.node(4).unwrap().pin(true, 0), Modifiers::ALT);
    assert_eq!(
        harness.editor.document.circuit.node(4).unwrap().kind,
        Kind::Nand
    );
    assert_eq!(harness.editor.document.circuit.wires, original.wires);
    harness.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(
        harness.editor.document.circuit.node(4).unwrap().kind,
        Kind::And
    );
    harness.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit, original);
}

#[test]
fn wire_joints_can_be_dragged_added_in_route_order_deleted_and_undone() {
    use fleece::drawing::wire_points;
    let mut harness = Harness::new();
    let circuit = &mut harness.editor.document.circuit;
    circuit.add(Kind::Input, Point::new(100.0, 200.0));
    circuit.add(Kind::Output, Point::new(500.0, 400.0));
    circuit
        .connect(Endpoint { node: 1, port: 0 }, Endpoint { node: 2, port: 0 })
        .unwrap();
    harness.frame(vec![]);
    let original = harness.editor.document.circuit.clone();
    let route = wire_points(&original, &original.wires[0]);
    harness.click(
        Point::new(route[1].x, (route[1].y + route[2].y) / 2.0),
        Modifiers::ALT,
    );
    harness.click(route[1], Modifiers::NONE);
    assert_eq!(
        harness.editor.document.circuit, original,
        "Selecting an automatic joint must not edit the circuit"
    );
    harness.time += 0.5;
    harness.pointer(route[1], true, Modifiers::NONE);
    let destination = Point::new(380.0, 260.0);
    harness.frame(vec![Event::PointerMoved(harness.screen(destination))]);
    harness.pointer(destination, false, Modifiers::NONE);
    let moved = harness.editor.document.circuit.clone();
    let persistent = moved.nodes.iter().find(|n| n.kind == Kind::Joint).unwrap();
    assert_eq!(persistent.position, destination);
    assert_eq!(moved.joint_degree(persistent.id), 2);
    assert_eq!(moved.wires.len(), 2);
    assert_eq!(moved.nodes[..2], original.nodes);
    // A second drag targets the same node rather than generating a new elbow.
    harness.time += 0.5;
    harness.pointer(destination, true, Modifiers::NONE);
    harness.pointer(Point::new(420.0, 300.0), false, Modifiers::NONE);
    assert_eq!(
        harness.editor.document.circuit.nodes.len(),
        moved.nodes.len()
    );
    assert_eq!(
        harness
            .editor
            .document
            .circuit
            .node(persistent.id)
            .unwrap()
            .position,
        Point::new(420.0, 300.0)
    );
    harness.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit, moved);
    harness.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit, original);
    harness.key(Key::Y, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit, moved);

    harness.key(Key::W, Modifiers::NONE);
    let point = Point::new(240.0, 224.0);
    harness.click(point, Modifiers::NONE);
    let split = harness.editor.document.circuit.clone();
    let joint = split
        .nodes
        .iter()
        .find(|n| n.kind == Kind::Joint && n.position == Point::new(240.0, 220.0))
        .unwrap();
    assert_eq!(joint.position, Point::new(240.0, 220.0));
    assert_eq!(split.wires.len(), moved.wires.len() + 1);
    harness.key(Key::Delete, Modifiers::NONE);
    let removed = &harness.editor.document.circuit;
    assert_eq!(removed.nodes, moved.nodes);
    assert_eq!(removed.wires.len(), moved.wires.len());
    assert_eq!(
        removed
            .wires
            .iter()
            .map(|w| (w.from, w.to))
            .collect::<std::collections::BTreeSet<_>>(),
        moved.wires.iter().map(|w| (w.from, w.to)).collect()
    );
    harness.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit, split);
    let saved = Document::from_toml(&harness.editor.document.to_toml().unwrap()).unwrap();
    assert_eq!(saved.circuit, harness.editor.document.circuit);
}

#[test]
fn labels_accept_typing_backspace_and_paste_without_triggering_canvas_shortcuts() {
    let mut harness = Harness::new();
    harness.key(Key::I, Modifiers::NONE);
    harness.key(Key::Enter, Modifiers::NONE);
    let node = harness.editor.document.circuit.nodes[0].id;
    let original = harness.editor.document.circuit.nodes[0].label.clone();
    harness.focus(egui::Id::unique(("node_label", node)));
    harness.key(Key::A, Modifiers::COMMAND);
    harness.type_key(Key::A, "A");
    harness.type_key(Key::Space, " ");
    harness.type_key(Key::N, "N");
    harness.key(Key::Backspace, Modifiers::NONE);
    harness.frame(vec![Event::Paste("signal".into())]);
    assert_eq!(harness.editor.document.circuit.nodes[0].label, "A signal");
    assert_eq!(harness.editor.document.circuit.nodes.len(), 1);

    harness.context.memory_mut(|memory| {
        memory.surrender_focus(egui::Id::unique(("node_label", node)));
    });
    harness.frame(vec![]);
    harness.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit.nodes[0].label, original);
    harness.key(Key::Y, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit.nodes[0].label, "A signal");
    harness.editor.document.validate().unwrap();
}

#[test]
fn circuit_title_can_be_edited_without_adding_blocks() {
    let mut harness = Harness::new();
    harness.focus(egui::Id::unique("circuit_title"));
    harness.key(Key::A, Modifiers::COMMAND);
    harness.type_key(Key::N, "New circuit");
    harness.frame(vec![Event::Text(" name".into())]);
    assert_eq!(harness.editor.document.circuit.title, "New circuit name");
    assert!(harness.editor.document.circuit.nodes.is_empty());
}

#[test]
fn keyboard_placement_undo_redo_works_without_input_toggling() {
    let mut harness = Harness::new();
    harness.key(Key::I, Modifiers::NONE);
    harness.key(Key::Enter, Modifiers::NONE);
    assert_eq!(harness.editor.document.circuit.nodes.len(), 1);
    assert_eq!(harness.editor.document.circuit.nodes[0].kind, Kind::Input);
    harness.key(Key::Space, Modifiers::NONE);
    harness.key(Key::Z, Modifiers::COMMAND);
    assert!(harness.editor.document.circuit.nodes.is_empty());
    harness.key(Key::Y, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit.nodes.len(), 1);
    harness.key(Key::A, Modifiers::NONE);
    harness.key(Key::ArrowRight, Modifiers::NONE);
    harness.key(Key::Enter, Modifiers::NONE);
    assert_eq!(harness.editor.document.circuit.nodes[1].kind, Kind::And);
    harness.editor.document.validate().unwrap();
}

#[test]
fn every_layout_and_modal_can_be_drawn_headlessly() {
    let mut harness = Harness::new();
    harness.editor.document = Document::new(Circuit::half_adder());
    for layout in [Layout::Standard, Layout::LibraryRight, Layout::Focus] {
        harness.editor.document.editor.layout = layout;
        harness.frame(vec![]);
    }
    harness.key(Key::F1, Modifiers::NONE);
    harness.frame(vec![]);
    harness.key(Key::Escape, Modifiers::NONE);
    harness.key(Key::E, Modifiers::COMMAND);
    harness.frame(vec![]);
    harness.key(Key::Escape, Modifiers::NONE);
    harness.key(Key::K, Modifiers::COMMAND);
    harness.frame(vec![]);
    harness.key(Key::Escape, Modifiers::NONE);
    harness.key(Key::W, Modifiers::NONE);
    harness.frame(vec![]);
}

#[test]
fn keyboard_register_placement_and_idle_keys_preserve_diagram() {
    let mut harness = Harness::new();
    for key in [Key::D, Key::I, Key::I] {
        harness.key(
            key,
            if key == Key::D {
                Modifiers::SHIFT
            } else {
                Modifiers::NONE
            },
        );
        harness.key(Key::Enter, Modifiers::NONE);
    }
    let circuit = &mut harness.editor.document.circuit;
    assert_eq!(circuit.nodes[0].kind, Kind::Register);
    circuit.resize_register(1, 1).unwrap();
    circuit
        .connect(Endpoint { node: 2, port: 0 }, Endpoint { node: 1, port: 0 })
        .unwrap();
    circuit
        .connect(Endpoint { node: 3, port: 0 }, Endpoint { node: 1, port: 1 })
        .unwrap();
    let before = harness.editor.document.circuit.clone();
    harness.key(Key::P, Modifiers::NONE);
    harness.key(Key::Space, Modifiers::NONE);
    assert_eq!(harness.editor.document.circuit, before);
    harness.editor.document.validate().unwrap();
}

#[test]
fn themes_apply_to_controls_and_canvas_without_breaking_text_entry() {
    let mut harness = Harness::new();
    for (key, _) in fleece::theme::BUILT_INS {
        harness.editor.select_theme(key).unwrap();
        harness.frame(vec![]);
        assert_eq!(
            harness.context.global_style().visuals.panel_fill,
            harness.editor.active_theme().ui.panel.egui()
        );
        harness.key(Key::I, Modifiers::NONE);
        harness.key(Key::Enter, Modifiers::NONE);
        let id = harness.editor.document.circuit.nodes.last().unwrap().id;
        harness.focus(egui::Id::unique(("node_label", id)));
        harness.key(Key::A, Modifiers::COMMAND);
        harness.frame(vec![Event::Text("Themed input".into())]);
        assert_eq!(
            harness.editor.document.circuit.node(id).unwrap().label,
            "Themed input"
        );
        harness
            .context
            .memory_mut(|memory| memory.surrender_focus(egui::Id::unique(("node_label", id))));
        harness.frame(vec![]);
    }
}

fn wheel(delta: Vec2, unit: egui::MouseWheelUnit, modifiers: Modifiers) -> Event {
    Event::MouseWheel {
        unit,
        delta,
        modifiers,
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Unknown,
    }
}

#[test]
fn shift_scrolling_and_sideways_scroll_pan_without_zooming() {
    let mut harness = Harness::new();
    harness.frame(vec![Event::PointerMoved(harness.canvas_rect.center())]);
    let before = harness.editor.document.clone();
    let start = before.editor.pan;
    harness.frame(vec![wheel(
        egui::vec2(0.0, -80.0),
        egui::MouseWheelUnit::Point,
        Modifiers::SHIFT,
    )]);
    assert_eq!(
        harness.editor.document.editor.pan,
        Point::new(start.x - 80.0, start.y)
    );
    harness.frame(vec![wheel(
        egui::vec2(50.0, 0.0),
        egui::MouseWheelUnit::Point,
        Modifiers::NONE,
    )]);
    assert_eq!(
        harness.editor.document.editor.pan,
        Point::new(start.x - 30.0, start.y)
    );
    harness.frame(vec![wheel(
        egui::vec2(15.0, 25.0),
        egui::MouseWheelUnit::Point,
        Modifiers::SHIFT,
    )]);
    harness.frame(vec![wheel(
        egui::vec2(0.0, 2.0),
        egui::MouseWheelUnit::Line,
        Modifiers::SHIFT,
    )]);
    assert_eq!(
        harness.editor.document.editor.pan,
        Point::new(start.x + 65.0, start.y + 25.0)
    );
    let height = harness.canvas_rect.height();
    harness.frame(vec![wheel(
        egui::vec2(0.0, 1.0),
        egui::MouseWheelUnit::Page,
        Modifiers::SHIFT,
    )]);
    assert_eq!(
        harness.editor.document.editor.pan.x,
        start.x + 65.0 + height
    );
    assert_eq!(harness.editor.document.editor.zoom, before.editor.zoom);
    assert_eq!(harness.editor.document.circuit, before.circuit);
    harness.editor.document.validate().unwrap();
}

#[test]
fn control_scroll_zooms_once_at_the_pointer_and_pinch_still_works() {
    let mut harness = Harness::new();
    let pointer = harness.canvas_rect.center() + egui::vec2(60.0, -45.0);
    harness.frame(vec![Event::PointerMoved(pointer)]);
    let view = &harness.editor.document.editor;
    let world = Point::new(
        (pointer.x - harness.canvas_rect.min.x - view.pan.x) / view.zoom,
        (pointer.y - harness.canvas_rect.min.y - view.pan.y) / view.zoom,
    );
    harness.frame(vec![wheel(
        egui::vec2(0.0, 80.0),
        egui::MouseWheelUnit::Point,
        Modifiers::CTRL,
    )]);
    let zoom = (80.0_f32 * 0.002).exp();
    assert!((harness.editor.document.editor.zoom - zoom).abs() < 0.0001);
    assert!(harness.screen(world).distance(pointer) < 0.001);
    harness.frame(vec![]);
    assert!(
        (harness.editor.document.editor.zoom - zoom).abs() < 0.0001,
        "egui's scroll smoothing must not apply a second zoom"
    );
    harness.frame(vec![Event::Zoom(1.25)]);
    assert!((harness.editor.document.editor.zoom - zoom * 1.25).abs() < 0.0001);
    assert!(harness.screen(world).distance(pointer) < 0.001);
    harness.frame(vec![wheel(
        egui::vec2(0.0, -80.0),
        egui::MouseWheelUnit::Point,
        Modifiers::CTRL,
    )]);
    assert!((harness.editor.document.editor.zoom - 1.25).abs() < 0.0001);
    assert!(harness.screen(world).distance(pointer) < 0.001);
}

#[test]
fn middle_button_drag_pans_without_a_press_jump_or_editing_blocks_and_stops_on_release() {
    let mut harness = Harness::new();
    harness.key(Key::I, Modifiers::NONE);
    harness.key(Key::Enter, Modifiers::NONE);
    let before = harness.editor.document.clone();
    let node = &before.circuit.nodes[0];
    let start = harness.screen(Point::new(node.position.x + 40.0, node.position.y + 24.0));
    harness.frame(vec![Event::PointerMoved(harness.canvas_rect.center())]);
    let button = |pos, pressed| Event::PointerButton {
        pos,
        button: egui::PointerButton::Middle,
        pressed,
        modifiers: Modifiers::NONE,
    };
    harness.frame(vec![Event::PointerMoved(start), button(start, true)]);
    assert_eq!(harness.editor.document.editor.pan, before.editor.pan);
    let movement = egui::vec2(75.0, -30.0);
    harness.frame(vec![Event::PointerMoved(start + movement)]);
    assert_eq!(
        harness.editor.document.editor.pan,
        Point::new(
            before.editor.pan.x + movement.x,
            before.editor.pan.y + movement.y
        )
    );
    let final_movement = movement + egui::vec2(20.0, 15.0);
    harness.frame(vec![
        Event::PointerMoved(start + final_movement),
        button(start + final_movement, false),
    ]);
    let released_pan = harness.editor.document.editor.pan;
    assert_eq!(
        released_pan,
        Point::new(
            before.editor.pan.x + final_movement.x,
            before.editor.pan.y + final_movement.y
        )
    );
    harness.frame(vec![Event::PointerMoved(
        start + final_movement + egui::vec2(80.0, 80.0),
    )]);
    assert_eq!(harness.editor.document.editor.pan, released_pan);
    assert_eq!(harness.editor.document.circuit, before.circuit);
    harness.key(Key::Z, Modifiers::COMMAND);
    assert!(
        harness.editor.document.circuit.nodes.is_empty(),
        "Panning must not add circuit undo entries"
    );
}

#[test]
fn scrolling_over_side_panels_or_modals_does_not_move_the_canvas() {
    let mut harness = Harness::new();
    let before = harness.editor.document.editor.clone();
    harness.frame(vec![
        Event::PointerMoved(egui::pos2(30.0, 200.0)),
        wheel(
            egui::vec2(0.0, -80.0),
            egui::MouseWheelUnit::Point,
            Modifiers::NONE,
        ),
    ]);
    assert_eq!(harness.editor.document.editor, before);
    harness.key(Key::F1, Modifiers::NONE);
    harness.frame(vec![
        Event::PointerMoved(harness.canvas_rect.center()),
        wheel(
            egui::vec2(30.0, -80.0),
            egui::MouseWheelUnit::Point,
            Modifiers::NONE,
        ),
    ]);
    assert_eq!(harness.editor.document.editor, before);
}

#[test]
fn joints_select_directly_and_shift_selection_moves_across_wires() {
    let mut harness = Harness::new();
    let c = &mut harness.editor.document.circuit;
    for (kind, p) in [
        (Kind::Input, Point::new(100.0, 200.0)),
        (Kind::Output, Point::new(600.0, 400.0)),
        (Kind::Input, Point::new(100.0, 500.0)),
        (Kind::Output, Point::new(600.0, 600.0)),
    ] {
        c.add(kind, p);
    }
    c.connect(Endpoint { node: 1, port: 0 }, Endpoint { node: 2, port: 0 })
        .unwrap();
    c.connect(Endpoint { node: 3, port: 0 }, Endpoint { node: 4, port: 0 })
        .unwrap();
    c.wires[0].waypoints = vec![Point::new(300.0, 220.0), Point::new(300.0, 420.0)];
    c.wires[1].waypoints = vec![Point::new(420.0, 520.0), Point::new(420.0, 620.0)];
    harness.frame(vec![]);
    let original = harness.editor.document.circuit.clone();
    harness.click(Point::new(300.0, 220.0), Modifiers::NONE);
    harness.click(Point::new(420.0, 520.0), Modifiers::SHIFT);
    assert_eq!(harness.editor.document.circuit, original);
    harness.time += 0.5;
    harness.pointer(Point::new(300.0, 220.0), true, Modifiers::NONE);
    harness.frame(vec![Event::PointerMoved(
        harness.screen(Point::new(340.0, 260.0)),
    )]);
    harness.pointer(Point::new(340.0, 260.0), false, Modifiers::NONE);
    let moved = harness.editor.document.circuit.clone();
    assert_eq!(moved.nodes[..4], original.nodes);
    let joints: Vec<_> = moved
        .nodes
        .iter()
        .filter(|n| n.kind == Kind::Joint)
        .collect();
    assert_eq!(joints.len(), 2);
    assert!(
        joints
            .iter()
            .any(|n| n.position == Point::new(340.0, 260.0))
    );
    assert!(
        joints
            .iter()
            .any(|n| n.position == Point::new(460.0, 560.0))
    );
    assert_eq!(moved.wires.len(), 4);
    for point in [Point::new(300.0, 420.0), Point::new(420.0, 620.0)] {
        assert!(moved.wires.iter().any(|w| w.waypoints.contains(&point)));
    }
    harness.key(Key::ArrowRight, Modifiers::NONE);
    let edited = &harness.editor.document.circuit;
    assert!(
        edited
            .nodes
            .iter()
            .any(|n| n.kind == Kind::Joint && n.position == Point::new(480.0, 560.0))
    );
    harness.key(Key::Delete, Modifiers::NONE);
    assert_eq!(harness.editor.document.circuit.nodes, original.nodes);
    assert_eq!(harness.editor.document.circuit.wires.len(), 2);
    assert!(
        harness
            .editor
            .document
            .circuit
            .wires
            .iter()
            .all(|w| w.from.node < 5 && w.to.node < 5)
    );
    harness.key(Key::Z, Modifiers::COMMAND);
    harness.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit, moved);
    harness.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit, original);
}

#[test]
fn box_selection_moves_blocks_and_automatic_joints_together() {
    let mut harness = Harness::new();
    let c = &mut harness.editor.document.circuit;
    c.add(Kind::Input, Point::new(100.0, 200.0));
    c.add(Kind::Output, Point::new(600.0, 400.0));
    c.connect(Endpoint { node: 1, port: 0 }, Endpoint { node: 2, port: 0 })
        .unwrap();
    harness.frame(vec![]);
    let original = harness.editor.document.circuit.clone();
    let route = fleece::drawing::wire_points(&original, &original.wires[0]);
    harness.pointer(Point::new(80.0, 180.0), true, Modifiers::SHIFT);
    harness.frame(vec![Event::PointerMoved(
        harness.screen(Point::new(640.0, 300.0)),
    )]);
    harness.pointer(Point::new(640.0, 300.0), false, Modifiers::SHIFT);
    assert_eq!(harness.editor.document.circuit, original);
    harness.time += 0.5;
    harness.pointer(Point::new(140.0, 220.0), true, Modifiers::NONE);
    harness.frame(vec![Event::PointerMoved(
        harness.screen(Point::new(180.0, 260.0)),
    )]);
    harness.pointer(Point::new(180.0, 260.0), false, Modifiers::NONE);
    let moved = &harness.editor.document.circuit;
    assert_eq!(moved.nodes[0].position, Point::new(140.0, 240.0));
    assert_eq!(moved.nodes[1], original.nodes[1]);
    let joint = moved.nodes.iter().find(|n| n.kind == Kind::Joint).unwrap();
    assert_eq!(
        joint.position,
        Point::new(route[1].x + 40.0, route[1].y + 40.0).snapped()
    );
    assert_eq!(moved.joint_degree(joint.id), 2);
    assert_eq!(moved.wires.len(), 2);
    harness.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit, original);
}

#[test]
fn editor_pins_use_distinct_grid_intersections_for_all_supported_port_counts() {
    for kind in Kind::ALL {
        for width in 1..=8 {
            let mut node = Node::new(1, kind, Point::new(100.0, 200.0));
            if kind == Kind::Register {
                node.resize_register(width);
            } else if kind.variable_inputs() {
                node.resize_ports(
                    width.clamp(2, 6),
                    if kind == Kind::Module {
                        width.clamp(1, 4)
                    } else {
                        1
                    },
                );
            }
            for output in [false, true] {
                let count = if output { node.outputs } else { node.inputs };
                let pins: Vec<_> = (0..count).map(|port| node.pin(output, port)).collect();
                for (i, pin) in pins.iter().enumerate() {
                    assert_eq!(*pin, pin.snapped(), "{kind:?} pin {i}");
                    assert!(pin.y > node.position.y && pin.y < node.position.y + node.size().y);
                    assert!(
                        !pins[..i].contains(pin),
                        "{kind:?} pins must remain separate"
                    );
                }
            }
        }
    }
}

#[test]
fn one_click_adds_a_shared_joint_and_multiple_branches_move_and_undo_together() {
    use fleece::drawing::wire_points;
    let mut harness = Harness::new();
    let c = &mut harness.editor.document.circuit;
    c.add(Kind::Input, Point::new(100.0, 200.0));
    for y in [200.0, 320.0, 440.0, 560.0] {
        c.add(Kind::Output, Point::new(600.0, y));
    }
    c.connect(Endpoint { node: 1, port: 0 }, Endpoint { node: 2, port: 0 })
        .unwrap();
    harness.frame(vec![]);
    harness.key(Key::W, Modifiers::NONE);
    let original = harness.editor.document.circuit.clone();
    // A single ordinary click, slightly off the wire, projects onto its segment.
    let point = Point::new(300.0, 224.0);
    harness.click(point, Modifiers::NONE);
    let split = harness.editor.document.circuit.clone();
    let joint = split
        .nodes
        .iter()
        .find(|n| n.kind == Kind::Joint)
        .unwrap()
        .id;
    let origin = Point::new(300.0, 220.0);
    assert_eq!(split.node(joint).unwrap().position, origin);
    assert_eq!(split.nodes.len(), original.nodes.len() + 1);
    assert_eq!(split.wires.len(), 2);
    harness.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit, original);
    harness.key(Key::Y, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit, split);
    for target in 3..=5 {
        harness.click(origin, Modifiers::SHIFT);
        let pin = harness
            .editor
            .document
            .circuit
            .node(target)
            .unwrap()
            .pin(false, 0);
        harness.click(pin, Modifiers::NONE);
    }
    let branched = harness.editor.document.circuit.clone();
    assert_eq!(
        branched
            .wires
            .iter()
            .filter(|w| w.from.node == joint)
            .count(),
        4
    );

    for target in 2..=5 {
        assert_eq!(
            support::source_kind(
                &branched,
                Endpoint {
                    node: target,
                    port: 0
                },
                false
            ),
            Kind::Input
        );
    }
    harness.key(Key::V, Modifiers::NONE);
    // Dragging the shared point moves all five incident edges, with no extra joints.
    harness.time += 0.5;
    harness.pointer(origin, true, Modifiers::NONE);
    let destination = Point::new(340.0, 260.0);
    harness.frame(vec![Event::PointerMoved(harness.screen(destination))]);
    harness.pointer(destination, false, Modifiers::NONE);
    let moved = harness.editor.document.circuit.clone();
    assert_eq!(moved.node(joint).unwrap().position, destination);
    assert_eq!(moved.nodes.len(), branched.nodes.len());
    assert_eq!(moved.wires, branched.wires);
    for wire in &moved.wires {
        let route = wire_points(&moved, wire);
        if wire.from.node == joint {
            assert_eq!(route.first(), Some(&destination));
        }
        if wire.to.node == joint {
            assert_eq!(route.last(), Some(&destination));
        }
    }
    harness.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit, branched);
    harness.key(Key::Y, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit, moved);
    // Box selection includes the shared joint even though it has no block body.
    harness.pointer(Point::new(310.0, 230.0), true, Modifiers::SHIFT);
    harness.frame(vec![Event::PointerMoved(
        harness.screen(Point::new(370.0, 290.0)),
    )]);
    harness.pointer(Point::new(370.0, 290.0), false, Modifiers::SHIFT);
    harness.key(Key::ArrowRight, Modifiers::NONE);
    assert_eq!(
        harness
            .editor
            .document
            .circuit
            .node(joint)
            .unwrap()
            .position,
        Point::new(360.0, 260.0)
    );
    harness.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit, moved);
    harness.key(Key::Delete, Modifiers::NONE);
    let removed = &harness.editor.document.circuit;
    assert!(removed.node(joint).is_none());
    assert_eq!(removed.wires.len(), 4);
    for target in 2..=5 {
        assert_eq!(
            support::source_kind(
                removed,
                Endpoint {
                    node: target,
                    port: 0
                },
                false
            ),
            Kind::Input
        );
    }
    harness.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit, moved);
    harness.editor.document.validate().unwrap();
    assert_eq!(
        Document::from_toml(&harness.editor.document.to_toml().unwrap())
            .unwrap()
            .circuit,
        moved
    );
}

#[test]
fn branching_can_start_at_an_input_and_finish_on_a_wire_or_routing_control() {
    for routing_control in [false, true] {
        let mut harness = Harness::new();
        let c = &mut harness.editor.document.circuit;
        c.add(Kind::Input, Point::new(100.0, 200.0));
        c.add(Kind::Output, Point::new(600.0, 400.0));
        c.add(Kind::Output, Point::new(600.0, 560.0));
        c.connect(Endpoint { node: 1, port: 0 }, Endpoint { node: 2, port: 0 })
            .unwrap();
        let point = if routing_control {
            c.wires[0].waypoints = vec![Point::new(300.0, 220.0), Point::new(300.0, 420.0)];
            Point::new(300.0, 220.0)
        } else {
            Point::new(300.0, 220.0)
        };
        harness.frame(vec![]);
        harness.key(Key::W, Modifiers::NONE);
        let target = harness
            .editor
            .document
            .circuit
            .node(3)
            .unwrap()
            .pin(false, 0);
        harness.click(target, Modifiers::NONE);
        harness.click(point, Modifiers::NONE);
        let c = &harness.editor.document.circuit;
        assert_eq!(c.nodes.iter().filter(|n| n.kind == Kind::Joint).count(), 1);
        assert_eq!(c.wires.len(), 3);
        assert_eq!(
            support::source_kind(c, Endpoint { node: 3, port: 0 }, false),
            Kind::Input
        );
        harness.editor.document.validate().unwrap();
    }
}

#[test]
fn fsm_states_transitions_loops_selection_drag_and_undo_work_headlessly() {
    let mut harness = Harness::new();
    harness.editor.new_fsm();
    harness.frame(vec![]);
    assert!(harness.editor.document.circuit.fsm.is_some());
    harness.key(Key::S, Modifiers::NONE);
    harness.click(Point::new(160.0, 220.0), Modifiers::NONE);
    harness.click(Point::new(460.0, 220.0), Modifiers::NONE);
    let original_states = harness.editor.document.circuit.clone();
    assert_eq!(original_states.fsm.as_ref().unwrap().states.len(), 2);
    harness.key(Key::T, Modifiers::NONE);
    for (from, to) in [
        (Point::new(160.0, 220.0), Point::new(460.0, 220.0)),
        (Point::new(460.0, 220.0), Point::new(160.0, 220.0)),
        (Point::new(160.0, 220.0), Point::new(160.0, 220.0)),
    ] {
        harness.click(from, Modifiers::NONE);
        harness.click(to, Modifiers::NONE);
    }
    let connected = harness.editor.document.circuit.clone();
    assert_eq!(connected.fsm.as_ref().unwrap().transitions.len(), 3);
    harness.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(
        harness
            .editor
            .document
            .circuit
            .fsm
            .as_ref()
            .unwrap()
            .transitions
            .len(),
        2
    );
    harness.key(Key::Y, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit, connected);
    harness.key(Key::V, Modifiers::NONE);
    harness.pointer(Point::new(160.0, 220.0), true, Modifiers::NONE);
    harness.frame(vec![Event::PointerMoved(
        harness.screen(Point::new(200.0, 260.0)),
    )]);
    harness.pointer(Point::new(200.0, 260.0), false, Modifiers::NONE);
    assert_eq!(
        harness
            .editor
            .document
            .circuit
            .fsm
            .as_ref()
            .unwrap()
            .state(1)
            .unwrap()
            .position,
        Point::new(200.0, 260.0)
    );
    harness.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit, connected);
    // Box select both states, then move them together with an arrow key.
    harness.pointer(Point::new(80.0, 280.0), true, Modifiers::SHIFT);
    harness.frame(vec![Event::PointerMoved(
        harness.screen(Point::new(540.0, 300.0)),
    )]);
    harness.pointer(Point::new(540.0, 180.0), false, Modifiers::SHIFT);
    harness.key(Key::ArrowRight, Modifiers::NONE);
    let moved = harness.editor.document.circuit.fsm.as_ref().unwrap();
    assert_eq!(moved.state(1).unwrap().position, Point::new(180.0, 220.0));
    assert_eq!(moved.state(2).unwrap().position, Point::new(480.0, 220.0));
    harness.key(Key::Delete, Modifiers::NONE);
    assert!(
        harness
            .editor
            .document
            .circuit
            .fsm
            .as_ref()
            .unwrap()
            .states
            .is_empty()
    );
    assert!(
        harness
            .editor
            .document
            .circuit
            .fsm
            .as_ref()
            .unwrap()
            .transitions
            .is_empty()
    );
    harness.key(Key::Z, Modifiers::COMMAND);
    harness.editor.document.validate().unwrap();
}

#[test]
fn fsm_property_text_and_document_creation_preserve_unsaved_changes() {
    let mut harness = Harness::new();
    harness.editor.new_fsm();
    harness.frame(vec![]);
    harness.key(Key::S, Modifiers::NONE);
    harness.click(Point::new(180.0, 220.0), Modifiers::NONE);
    harness.key(Key::V, Modifiers::NONE);
    harness.focus(egui::Id::unique(("fsm_state", 1u64, "Name")));
    harness.key(Key::A, Modifiers::COMMAND);
    harness.frame(vec![Event::Paste("Idle".into())]);
    assert_eq!(
        harness
            .editor
            .document
            .circuit
            .fsm
            .as_ref()
            .unwrap()
            .state(1)
            .unwrap()
            .name,
        "Idle"
    );
    assert_eq!(
        harness
            .editor
            .document
            .circuit
            .fsm
            .as_ref()
            .unwrap()
            .states
            .len(),
        1
    );
    assert!(harness.editor.dirty());
    harness.editor.new_logic_circuit();
    harness.frame(vec![]);
    assert!(
        harness.editor.document.circuit.fsm.is_some(),
        "Unsaved FSM must be protected by the save/discard dialog"
    );
    let mut clean = Harness::new();
    clean.editor.new_fsm();
    clean.editor.new_logic_circuit();
    clean.frame(vec![]);
    assert!(clean.editor.document.circuit.fsm.is_none());
    assert!(clean.editor.document.circuit.nodes.is_empty());
}

#[test]
fn component_placement_stays_active_across_clicks_and_switches_until_escape() {
    let mut harness = Harness::new();
    harness.key(Key::W, Modifiers::NONE);
    harness.key(Key::I, Modifiers::NONE);
    let points = [
        Point::new(100.0, 100.0),
        Point::new(100.0, 240.0),
        Point::new(100.0, 380.0),
    ];
    for point in points {
        harness.click(point, Modifiers::NONE);
    }
    let inputs = harness.editor.document.circuit.clone();
    assert_eq!(inputs.nodes.len(), 3);
    for (node, point) in inputs.nodes.iter().zip(points) {
        assert_eq!(node.kind, Kind::Input);
        assert_eq!(node.position, point);
    }
    harness.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit.nodes.len(), 2);
    // Undo does not leave placement mode.
    harness.click(Point::new(100.0, 520.0), Modifiers::NONE);
    assert_eq!(harness.editor.document.circuit.nodes.len(), 3);
    assert_eq!(harness.editor.document.circuit.nodes[2].kind, Kind::Input);
    harness.key(Key::A, Modifiers::NONE);
    harness.click(Point::new(340.0, 100.0), Modifiers::NONE);
    harness.click(Point::new(340.0, 260.0), Modifiers::NONE);
    assert_eq!(harness.editor.document.circuit.nodes.len(), 5);
    assert!(
        harness.editor.document.circuit.nodes[3..]
            .iter()
            .all(|node| node.kind == Kind::And)
    );
    harness.key(Key::Escape, Modifiers::NONE);
    let before = harness.editor.document.circuit.clone();
    harness.click(Point::new(600.0, 500.0), Modifiers::NONE);
    assert_eq!(harness.editor.document.circuit, before);
    harness.click(Point::new(130.0, 120.0), Modifiers::SHIFT);
    harness.key(Key::ArrowRight, Modifiers::NONE);
    assert_eq!(
        harness.editor.document.circuit.nodes[0].position,
        Point::new(120.0, 100.0)
    );
    assert_eq!(harness.editor.document.circuit.nodes.len(), 5);
}

#[test]
fn repeated_clicks_at_one_location_place_without_toggling_inputs_and_enter_repeats() {
    let mut harness = Harness::new();
    harness.key(Key::I, Modifiers::NONE);
    let point = Point::new(200.0, 200.0);
    harness.click(point, Modifiers::NONE);
    // A rapid second click also overlaps the input body and must only place.
    harness.pointer(point, true, Modifiers::NONE);
    harness.pointer(point, false, Modifiers::NONE);
    assert_eq!(harness.editor.document.circuit.nodes.len(), 2);
    assert!(
        harness
            .editor
            .document
            .circuit
            .nodes
            .iter()
            .all(|n| n.position == point)
    );
    harness.key(Key::Enter, Modifiers::NONE);
    harness.key(Key::Enter, Modifiers::NONE);
    let nodes = &harness.editor.document.circuit.nodes;
    assert_eq!(nodes.len(), 4);
    assert_eq!(nodes[2].position, point);
    assert_eq!(nodes[3].position, Point::new(360.0, 200.0));
    harness.key(Key::Escape, Modifiers::NONE);
    harness.key(Key::Enter, Modifiers::NONE);
    assert_eq!(harness.editor.document.circuit.nodes.len(), 4);
}

#[test]
fn output_wires_can_end_at_suspended_joints_and_be_extended_or_finished_later() {
    let mut harness = Harness::new();
    let c = &mut harness.editor.document.circuit;
    c.add(Kind::Input, Point::new(100.0, 200.0));
    c.add(Kind::Output, Point::new(600.0, 400.0));
    harness.frame(vec![]);
    harness.key(Key::W, Modifiers::NONE);
    let original = harness.editor.document.circuit.clone();
    let start = original.node(1).unwrap().pin(true, 0);
    harness.click(start, Modifiers::NONE);
    harness.click(Point::new(343.0, 303.0), Modifiers::NONE);
    let suspended = harness.editor.document.circuit.clone();
    let joint = suspended
        .nodes
        .iter()
        .find(|n| n.kind == Kind::Joint)
        .unwrap()
        .id;
    assert_eq!(
        suspended.node(joint).unwrap().position,
        Point::new(340.0, 300.0)
    );
    assert_eq!(suspended.joint_degree(joint), 1);
    assert_eq!(
        support::source_kind(
            &suspended,
            Endpoint {
                node: joint,
                port: 0
            },
            true
        ),
        Kind::Input
    );
    assert_eq!(
        Document::from_toml(&harness.editor.document.to_toml().unwrap())
            .unwrap()
            .circuit,
        suspended
    );
    harness.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit, original);
    harness.key(Key::Y, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit, suspended);
    harness.click(Point::new(340.0, 300.0), Modifiers::SHIFT);
    harness.click(Point::new(460.0, 360.0), Modifiers::SHIFT);
    let extended = harness.editor.document.circuit.clone();
    let end = extended.nodes.last().unwrap().id;
    assert_eq!(extended.joint_degree(joint), 2);
    assert_eq!(extended.joint_degree(end), 1);
    harness.click(Point::new(460.0, 360.0), Modifiers::SHIFT);
    harness.click(extended.node(2).unwrap().pin(false, 0), Modifiers::NONE);
    let finished = &harness.editor.document.circuit;
    assert_eq!(finished.joint_degree(end), 2);
    assert_eq!(
        support::source_kind(finished, Endpoint { node: 2, port: 0 }, false),
        Kind::Input
    );
    harness.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit, extended);
    harness.editor.document.validate().unwrap();
}

#[test]
fn suspended_joint_from_an_input_can_receive_a_driver_from_either_click_direction() {
    for source_first in [false, true] {
        let mut harness = Harness::new();
        let c = &mut harness.editor.document.circuit;
        c.add(Kind::Input, Point::new(100.0, 200.0));
        c.add(Kind::Output, Point::new(600.0, 400.0));
        harness.frame(vec![]);
        harness.key(Key::W, Modifiers::NONE);
        let input = harness
            .editor
            .document
            .circuit
            .node(2)
            .unwrap()
            .pin(false, 0);
        harness.click(input, Modifiers::NONE);
        harness.click(Point::new(400.0, 300.0), Modifiers::NONE);
        let suspended = harness.editor.document.circuit.clone();
        let joint = suspended.nodes.last().unwrap().id;
        assert_eq!(suspended.joint_degree(joint), 1);
        assert_eq!(
            support::source_kind(&suspended, Endpoint { node: 2, port: 0 }, false),
            Kind::Joint
        );
        let source = suspended.node(1).unwrap().pin(true, 0);
        let point = suspended.node(joint).unwrap().position;
        // End the previous preview without leaving Wire mode.
        harness.key(Key::V, Modifiers::NONE);
        harness.key(Key::W, Modifiers::NONE);
        if source_first {
            harness.click(source, Modifiers::NONE);
            harness.click(point, Modifiers::NONE);
        } else {
            harness.click(point, Modifiers::SHIFT);
            harness.click(source, Modifiers::NONE);
        }
        let connected = &harness.editor.document.circuit;
        assert_eq!(connected.joint_degree(joint), 2);
        assert_eq!(
            support::source_kind(connected, Endpoint { node: 2, port: 0 }, false),
            Kind::Input
        );
        harness.key(Key::Z, Modifiers::COMMAND);
        assert_eq!(harness.editor.document.circuit, suspended);
        harness.editor.document.validate().unwrap();
    }
}

#[test]
fn wire_mode_joint_insertion_and_extension_are_undoable_and_escape_cancels() {
    let mut harness = Harness::new();
    let c = &mut harness.editor.document.circuit;
    c.add(Kind::Input, Point::new(100.0, 200.0));
    c.add(Kind::Output, Point::new(600.0, 400.0));
    c.connect(Endpoint { node: 1, port: 0 }, Endpoint { node: 2, port: 0 })
        .unwrap();
    c.wires[0].waypoints = vec![Point::new(300.0, 220.0), Point::new(300.0, 420.0)];
    harness.frame(vec![]);
    harness.key(Key::W, Modifiers::NONE);
    let original = harness.editor.document.circuit.clone();
    harness.click(Point::new(300.0, 220.0), Modifiers::NONE);
    let inserted = harness.editor.document.circuit.clone();
    harness.click(Point::new(440.0, 300.0), Modifiers::NONE);
    let branched = harness.editor.document.circuit.clone();
    assert_eq!(branched.nodes.len(), original.nodes.len() + 2);
    let degrees: Vec<_> = branched
        .nodes
        .iter()
        .filter(|n| n.kind == Kind::Joint)
        .map(|n| branched.joint_degree(n.id))
        .collect();
    assert_eq!(degrees, [3, 1]);
    harness.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit, inserted);
    harness.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit, original);
    harness.key(Key::Y, Modifiers::COMMAND);
    harness.key(Key::Y, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit, branched);
    harness.click(Point::new(440.0, 300.0), Modifiers::SHIFT);
    harness.key(Key::Escape, Modifiers::NONE);
    harness.click(Point::new(540.0, 500.0), Modifiers::NONE);
    assert_eq!(harness.editor.document.circuit, branched);
}

#[test]
fn right_click_removes_a_component_and_its_wires_and_undo_restores_them() {
    let mut harness = Harness::new();
    let c = &mut harness.editor.document.circuit;
    c.add(Kind::Input, Point::new(100.0, 200.0));
    c.add(Kind::And, Point::new(300.0, 200.0));
    c.add(Kind::Output, Point::new(600.0, 200.0));
    c.connect(Endpoint { node: 1, port: 0 }, Endpoint { node: 2, port: 0 })
        .unwrap();
    c.connect(Endpoint { node: 2, port: 0 }, Endpoint { node: 3, port: 0 })
        .unwrap();
    harness.frame(vec![]);
    let original = harness.editor.document.circuit.clone();
    harness.right_pointer(Point::new(340.0, 240.0), true);
    assert_eq!(
        harness.editor.document.circuit, original,
        "Removal waits for click release so dragging remains possible"
    );
    harness.right_pointer(Point::new(340.0, 240.0), false);
    assert!(harness.editor.document.circuit.node(2).is_none());
    assert_eq!(harness.editor.document.circuit.nodes.len(), 2);
    assert!(harness.editor.document.circuit.wires.is_empty());
    harness.editor.document.validate().unwrap();
    harness.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit, original);
    harness.key(Key::Y, Modifiers::COMMAND);
    assert!(harness.editor.document.circuit.node(2).is_none());
}

#[test]
fn right_click_removes_a_joint_while_preserving_the_connection() {
    let mut harness = Harness::new();
    let c = &mut harness.editor.document.circuit;
    c.add(Kind::Input, Point::new(100.0, 200.0));
    c.add(Kind::Output, Point::new(600.0, 200.0));
    let joint = c
        .suspend_wire(
            Endpoint { node: 1, port: 0 },
            true,
            Point::new(300.0, 220.0),
        )
        .unwrap();
    c.connect(
        Endpoint {
            node: joint,
            port: 0,
        },
        Endpoint { node: 2, port: 0 },
    )
    .unwrap();
    harness.frame(vec![]);
    let original = harness.editor.document.circuit.clone();
    harness.right_click(Point::new(300.0, 220.0));
    let removed = &harness.editor.document.circuit;
    assert!(removed.node(joint).is_none());
    assert_eq!(removed.wires.len(), 1);
    assert_eq!(
        support::source_kind(removed, Endpoint { node: 2, port: 0 }, false),
        Kind::Input
    );
    harness.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit, original);
}

#[test]
fn dragging_and_shift_selection_do_not_remove_components() {
    let mut harness = Harness::new();
    let c = &mut harness.editor.document.circuit;
    c.add(Kind::Input, Point::new(100.0, 200.0));
    c.add(Kind::And, Point::new(300.0, 200.0));
    harness.frame(vec![]);
    harness.pointer(Point::new(340.0, 240.0), true, Modifiers::NONE);
    harness.frame(vec![Event::PointerMoved(
        harness.screen(Point::new(380.0, 280.0)),
    )]);
    harness.pointer(Point::new(380.0, 280.0), false, Modifiers::NONE);
    assert_eq!(
        harness.editor.document.circuit.node(2).unwrap().position,
        Point::new(340.0, 240.0)
    );
    harness.click(Point::new(130.0, 220.0), Modifiers::NONE);
    assert_eq!(harness.editor.document.circuit.nodes.len(), 2);
    harness.click(Point::new(380.0, 280.0), Modifiers::SHIFT);
    harness.key(Key::ArrowDown, Modifiers::NONE);
    let c = &harness.editor.document.circuit;
    assert_eq!(c.nodes.len(), 2);
    assert_eq!(c.node(1).unwrap().position, Point::new(100.0, 220.0));
    assert_eq!(c.node(2).unwrap().position, Point::new(340.0, 260.0));
}

#[test]
fn drag_mode_pans_without_editing_the_selected_nodes() {
    let mut harness = Harness::new();
    let c = &mut harness.editor.document.circuit;
    c.add(Kind::Input, Point::new(100.0, 200.0));
    c.add(Kind::Output, Point::new(600.0, 400.0));
    let joint = c
        .suspend_wire(
            Endpoint { node: 1, port: 0 },
            true,
            Point::new(300.0, 300.0),
        )
        .unwrap();
    c.connect(
        Endpoint {
            node: joint,
            port: 0,
        },
        Endpoint { node: 2, port: 0 },
    )
    .unwrap();
    harness.frame(vec![]);
    let original = harness.editor.document.circuit.clone();
    harness.click(Point::new(130.0, 220.0), Modifiers::NONE);
    harness.click(original.node(1).unwrap().pin(true, 0), Modifiers::NONE);
    harness.click(Point::new(300.0, 300.0), Modifiers::NONE);
    harness.click(Point::new(240.0, 220.0), Modifiers::NONE);
    assert_eq!(harness.editor.document.circuit, original);
    harness.key(Key::D, Modifiers::NONE);
    let pan = harness.editor.document.editor.pan;
    let start = harness.screen(Point::new(660.0, 540.0));
    harness.pointer(Point::new(660.0, 540.0), true, Modifiers::NONE);
    assert_eq!(harness.editor.document.editor.pan, pan);
    let movement = Vec2::new(70.0, -30.0);
    harness.frame(vec![Event::PointerMoved(start + movement)]);
    harness.frame(vec![Event::PointerButton {
        pos: start + movement,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    }]);
    assert_eq!(
        harness.editor.document.editor.pan,
        Point::new(pan.x + 70.0, pan.y - 30.0)
    );
    assert_eq!(harness.editor.document.circuit, original);
    harness.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(
        harness.editor.document.circuit, original,
        "Panning is not a circuit edit"
    );
}

#[test]
fn wire_mode_can_start_empty_connect_a_driver_and_extend_continuously_until_escape() {
    let mut harness = Harness::new();
    let source = harness
        .editor
        .document
        .circuit
        .add(Kind::Input, Point::new(100.0, 200.0));
    harness.frame(vec![]);
    harness.key(Key::W, Modifiers::NONE);
    harness.click(Point::new(300.0, 300.0), Modifiers::NONE);
    let root = harness.editor.document.circuit.nodes.last().unwrap().id;
    assert_eq!(harness.editor.document.circuit.joint_degree(root), 0);
    let source_pin = harness
        .editor
        .document
        .circuit
        .node(source)
        .unwrap()
        .pin(true, 0);
    harness.click(source_pin, Modifiers::NONE);
    assert_eq!(harness.editor.document.circuit.joint_degree(root), 1);
    harness.click(Point::new(300.0, 300.0), Modifiers::NONE);
    harness.click(Point::new(400.0, 400.0), Modifiers::NONE);
    harness.click(Point::new(500.0, 500.0), Modifiers::NONE);
    let connected = harness.editor.document.circuit.clone();
    assert_eq!(connected.nodes.len(), 4);
    assert_eq!(connected.wires.len(), 3);
    let last = connected.nodes.last().unwrap().id;
    assert_eq!(
        support::source_kind(
            &connected,
            Endpoint {
                node: last,
                port: 0
            },
            true
        ),
        Kind::Input
    );
    harness.key(Key::Escape, Modifiers::NONE);
    harness.click(Point::new(500.0, 500.0), Modifiers::NONE);
    harness.click(Point::new(660.0, 540.0), Modifiers::NONE);
    assert_eq!(harness.editor.document.circuit, connected);
    harness.key(Key::W, Modifiers::NONE);
    harness.key(Key::W, Modifiers::NONE);
    harness.click(Point::new(700.0, 600.0), Modifiers::NONE);
    assert_eq!(harness.editor.document.circuit, connected);
    harness.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit.nodes.len(), 3);
    harness.editor.document.validate().unwrap();
}

#[test]
fn both_diagram_types_share_pan_zoom_box_selection_and_arrow_movement() {
    for fsm_document in [false, true] {
        let mut harness = Harness::new();
        let c = &mut harness.editor.document.circuit;
        if fsm_document {
            *c = Circuit::new_fsm();
            let fsm = c.fsm.as_mut().unwrap();
            fsm.add_state(Point::new(140.0, 220.0));
            fsm.add_state(Point::new(540.0, 220.0));
            fsm.add_transition(1, 2).unwrap();
        } else {
            c.add(Kind::Input, Point::new(100.0, 200.0));
            c.add(Kind::Output, Point::new(500.0, 200.0));
            c.connect(Endpoint { node: 1, port: 0 }, Endpoint { node: 2, port: 0 })
                .unwrap();
        }
        harness.frame(vec![]);
        let original = harness.editor.document.circuit.clone();
        harness.key(Key::D, Modifiers::NONE);
        let pan = harness.editor.document.editor.pan;
        let start = harness.screen(Point::new(650.0, 500.0));
        harness.pointer(Point::new(650.0, 500.0), true, Modifiers::NONE);
        harness.frame(vec![Event::PointerMoved(start + Vec2::new(40.0, 20.0))]);
        harness.frame(vec![Event::PointerButton {
            pos: start + Vec2::new(70.0, 30.0),
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }]);
        assert_eq!(
            harness.editor.document.editor.pan,
            Point::new(pan.x + 70.0, pan.y + 30.0)
        );
        assert_eq!(harness.editor.document.circuit, original);
        harness.key(Key::V, Modifiers::NONE);
        // Horizontal Ctrl+scroll zooms identically and keeps the pointer's world point fixed.
        let point = Point::new(300.0, 300.0);
        let anchor = harness.screen(point);
        harness.frame(vec![Event::PointerMoved(anchor)]);
        harness.frame(vec![wheel(
            Vec2::new(60.0, 0.0),
            egui::MouseWheelUnit::Point,
            Modifiers::CTRL,
        )]);
        assert!(harness.editor.document.editor.zoom > 1.0);
        assert!(harness.screen(point).distance(anchor) < 0.01);
        harness.pointer(Point::new(80.0, 160.0), true, Modifiers::NONE);
        harness.frame(vec![Event::PointerMoved(
            harness.screen(Point::new(600.0, 280.0)),
        )]);
        harness.pointer(Point::new(600.0, 280.0), false, Modifiers::NONE);
        harness.key(Key::ArrowDown, Modifiers::NONE);
        let edited = &harness.editor.document.circuit;
        if let Some(fsm) = &edited.fsm {
            assert_eq!(fsm.state(1).unwrap().position.y, 240.0);
            assert_eq!(fsm.state(2).unwrap().position.y, 240.0);
        } else {
            assert_eq!(edited.node(1).unwrap().position.y, 220.0);
            assert_eq!(edited.node(2).unwrap().position.y, 220.0);
        }
        harness.key(Key::Z, Modifiers::COMMAND);
        assert_eq!(harness.editor.document.circuit, original);
    }
}

#[test]
fn fsm_uses_repeated_keyboard_placement_wire_mode_and_right_click_removal() {
    let mut harness = Harness::new();
    harness.editor.new_fsm();
    harness.frame(vec![]);
    harness.key(Key::S, Modifiers::NONE);
    harness.key(Key::Enter, Modifiers::NONE);
    harness.key(Key::Enter, Modifiers::NONE);
    harness.key(Key::ArrowRight, Modifiers::NONE);
    harness.key(Key::Enter, Modifiers::NONE);
    let states = harness.editor.document.circuit.clone();
    let fsm = states.fsm.as_ref().unwrap();
    assert_eq!(fsm.states.len(), 3);
    assert_eq!(fsm.state(1).unwrap().position, Point::new(200.0, 160.0));
    assert_eq!(fsm.state(2).unwrap().position, Point::new(360.0, 160.0));
    assert_eq!(fsm.state(3).unwrap().position, Point::new(540.0, 160.0));
    harness.key(Key::W, Modifiers::NONE);
    harness.click(Point::new(200.0, 160.0), Modifiers::NONE);
    harness.click(Point::new(360.0, 160.0), Modifiers::NONE);
    let connected = harness.editor.document.circuit.clone();
    assert_eq!(connected.fsm.as_ref().unwrap().transitions.len(), 1);
    harness.key(Key::Escape, Modifiers::NONE);
    let transition = &connected.fsm.as_ref().unwrap().transitions[0];
    let label = fleece::drawing::transition_geometry(connected.fsm.as_ref().unwrap(), transition)
        .unwrap()
        .label;
    harness.right_click(label);
    assert!(
        harness
            .editor
            .document
            .circuit
            .fsm
            .as_ref()
            .unwrap()
            .transitions
            .is_empty()
    );
    harness.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit, connected);
    harness.right_click(Point::new(200.0, 160.0));
    assert_eq!(
        harness
            .editor
            .document
            .circuit
            .fsm
            .as_ref()
            .unwrap()
            .states
            .len(),
        2
    );
    assert!(
        harness
            .editor
            .document
            .circuit
            .fsm
            .as_ref()
            .unwrap()
            .transitions
            .is_empty()
    );
    harness.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit, connected);
    harness.editor.document.validate().unwrap();
}

#[test]
fn fsm_copy_paste_and_duplicate_preserve_internal_transitions_and_are_undoable() {
    let mut harness = Harness::new();
    let document = Document::from_toml(include_str!("../examples/fsm.toml")).unwrap();
    harness.editor.document = document;
    harness.frame(vec![]);
    harness.key(Key::A, Modifiers::COMMAND);
    harness.key(Key::C, Modifiers::COMMAND);
    assert_eq!(
        Document::from_toml(&harness.clipboard)
            .unwrap()
            .circuit
            .fsm
            .unwrap()
            .states
            .len(),
        2
    );
    harness.frame(vec![Event::PointerMoved(
        harness.screen(Point::new(400.0, 400.0)),
    )]);
    harness.key(Key::V, Modifiers::COMMAND);
    let pasted = harness.editor.document.circuit.clone();
    let fsm = pasted.fsm.as_ref().unwrap();
    assert_eq!(fsm.states.len(), 4);
    assert_eq!(fsm.transitions.len(), 6);
    assert_eq!(fsm.initial, Some(1));
    assert_eq!(fsm.state(3).unwrap().position, Point::new(400.0, 400.0));
    assert!(fsm.state(4).unwrap().accepting);
    assert!(
        fsm.transitions[3..]
            .iter()
            .all(|t| t.from >= 3 && t.to >= 3)
    );
    harness.key(Key::D, Modifiers::COMMAND);
    assert_eq!(
        harness
            .editor
            .document
            .circuit
            .fsm
            .as_ref()
            .unwrap()
            .states
            .len(),
        6
    );
    assert_eq!(
        harness
            .editor
            .document
            .circuit
            .fsm
            .as_ref()
            .unwrap()
            .transitions
            .len(),
        9
    );
    harness.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit, pasted);
    let mut logic = Harness::new();
    logic.clipboard = harness.clipboard;
    logic.key(Key::V, Modifiers::COMMAND);
    assert!(logic.editor.document.circuit.nodes.is_empty());
    assert!(logic.editor.document.circuit.fsm.is_none());
}

#[test]
fn first_mouse_movement_locks_wire_direction_for_output_and_input_first_drawing() {
    for input_first in [false, true] {
        for horizontal in [false, true] {
            let mut harness = Harness::new();
            let c = &mut harness.editor.document.circuit;
            c.add(Kind::Input, Point::new(100.0, 200.0));
            c.add(Kind::Output, Point::new(600.0, 400.0));
            harness.frame(vec![]);
            harness.key(Key::W, Modifiers::NONE);
            let c = &harness.editor.document.circuit;
            let source = c.node(1).unwrap().pin(true, 0);
            let sink = c.node(2).unwrap().pin(false, 0);
            let (start, end) = if input_first {
                (sink, source)
            } else {
                (source, sink)
            };
            harness.click(start, Modifiers::NONE);
            let anchor = harness.screen(start);
            harness.frame(vec![Event::PointerMoved(anchor + Vec2::new(1.0, 0.0))]);
            let first = if horizontal {
                Vec2::new(8.0, 1.0)
            } else {
                Vec2::new(1.0, 8.0)
            };
            let later = if horizontal {
                Vec2::new(1.0, 80.0)
            } else {
                Vec2::new(80.0, 1.0)
            };
            // Multiple moves in one frame must use the first meaningful move.
            harness.frame(vec![
                Event::PointerMoved(anchor + first),
                Event::PointerMoved(anchor + later),
            ]);
            assert!(harness.editor.document.circuit.wires.is_empty());
            harness.click(end, Modifiers::NONE);
            let c = &harness.editor.document.circuit;
            assert_eq!(c.wires.len(), 1);
            assert_eq!(c.wires[0].from, Endpoint { node: 1, port: 0 });
            assert_eq!(c.wires[0].to, Endpoint { node: 2, port: 0 });
            assert_eq!(c.wires[0].horizontal_first, horizontal != input_first);
            let mut route = fleece::drawing::wire_points(c, &c.wires[0]);
            if input_first {
                route.reverse();
            }
            if horizontal {
                assert_eq!(route[0].y, route[1].y);
            } else {
                assert_eq!(route[0].x, route[1].x);
            }
            let saved = Document::from_toml(&harness.editor.document.to_toml().unwrap()).unwrap();
            assert_eq!(saved.circuit, harness.editor.document.circuit);
            harness.key(Key::Z, Modifiers::COMMAND);
            assert!(harness.editor.document.circuit.wires.is_empty());
        }
    }
}

#[test]
fn continuous_wire_segments_choose_their_direction_independently() {
    let mut harness = Harness::new();
    harness.key(Key::W, Modifiers::NONE);
    harness.click(Point::new(200.0, 200.0), Modifiers::NONE);
    let start = harness.screen(Point::new(200.0, 200.0));
    harness.frame(vec![Event::PointerMoved(start + Vec2::new(0.0, 10.0))]);
    harness.click(Point::new(400.0, 300.0), Modifiers::NONE);
    assert!(!harness.editor.document.circuit.wires[0].horizontal_first);
    let start = harness.screen(Point::new(400.0, 300.0));
    harness.frame(vec![Event::PointerMoved(start + Vec2::new(4.0, 4.0))]);
    harness.frame(vec![Event::PointerMoved(start + Vec2::new(10.0, 1.0))]);
    harness.click(Point::new(440.0, 500.0), Modifiers::NONE);
    let connected = harness.editor.document.circuit.clone();
    assert_eq!(connected.wires.len(), 2);
    assert!(!connected.wires[0].horizontal_first);
    assert!(connected.wires[1].horizontal_first);
    harness.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit.wires.len(), 1);
    harness.key(Key::Y, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit, connected);
}

#[test]
fn drawing_from_a_free_joint_to_a_driven_joint_preserves_the_driver_and_gesture() {
    let mut harness = Harness::new();
    let c = &mut harness.editor.document.circuit;
    c.add(Kind::Input, Point::new(100.0, 200.0));
    let driven = c
        .suspend_wire(
            Endpoint { node: 1, port: 0 },
            true,
            Point::new(300.0, 300.0),
        )
        .unwrap();
    harness.frame(vec![]);
    harness.key(Key::W, Modifiers::NONE);
    harness.click(Point::new(500.0, 500.0), Modifiers::NONE);
    let free = harness.editor.document.circuit.nodes.last().unwrap().id;
    let start = harness.screen(Point::new(500.0, 500.0));
    harness.frame(vec![Event::PointerMoved(start + Vec2::new(10.0, 1.0))]);
    harness.click(Point::new(300.0, 300.0), Modifiers::NONE);
    let c = &harness.editor.document.circuit;
    assert_eq!(c.wires.len(), 2);
    assert_eq!(c.wires[0].from, Endpoint { node: 1, port: 0 });
    assert_eq!(
        c.wires[0].to,
        Endpoint {
            node: driven,
            port: 0
        }
    );
    assert_eq!(
        c.wires[1].from,
        Endpoint {
            node: driven,
            port: 0
        }
    );
    assert_eq!(
        c.wires[1].to,
        Endpoint {
            node: free,
            port: 0
        }
    );
    assert!(!c.wires[1].horizontal_first);
    assert_eq!(
        support::source_kind(
            c,
            Endpoint {
                node: free,
                port: 0
            },
            true
        ),
        Kind::Input
    );
    harness.editor.document.validate().unwrap();
}

#[test]
fn mouse_wheel_and_touchpad_scroll_pan_and_pinch_zooms_in_both_editors() {
    for fsm_document in [false, true] {
        for (unit, amount) in [
            (egui::MouseWheelUnit::Point, 80.0),
            (egui::MouseWheelUnit::Line, 2.0),
        ] {
            let mut harness = Harness::new();
            if fsm_document {
                harness.editor.new_fsm();
                harness.frame(vec![]);
            }
            let original = harness.editor.document.circuit.clone();
            let pointer = harness.canvas_rect.center() + Vec2::new(60.0, -45.0);
            harness.frame(vec![Event::PointerMoved(pointer)]);
            let before = harness.editor.document.editor.clone();
            harness.frame(vec![wheel(Vec2::new(0.0, amount), unit, Modifiers::NONE)]);
            let expected_pan = Point::new(before.pan.x, before.pan.y + 80.0);
            assert_eq!(harness.editor.document.editor.zoom, before.zoom);
            assert_eq!(harness.editor.document.editor.pan, expected_pan);
            harness.frame(vec![]);
            assert_eq!(
                harness.editor.document.editor.pan, expected_pan,
                "Scroll smoothing must not pan twice"
            );
            harness.frame(vec![wheel(Vec2::new(0.0, -amount), unit, Modifiers::NONE)]);
            assert_eq!(harness.editor.document.editor, before);
            harness.frame(vec![Event::Zoom(1.2)]);
            assert!((harness.editor.document.editor.zoom - 1.2).abs() < 0.0001);
            let pan = harness.editor.document.editor.pan;
            harness.frame(vec![wheel(
                Vec2::new(0.0, 40.0),
                egui::MouseWheelUnit::Point,
                Modifiers::SHIFT,
            )]);
            assert!((harness.editor.document.editor.zoom - 1.2).abs() < 0.0001);
            assert_eq!(
                harness.editor.document.editor.pan,
                Point::new(pan.x + 40.0, pan.y)
            );
            assert_eq!(harness.editor.document.circuit, original);
            harness.editor.document.validate().unwrap();
        }
    }
}

#[test]
fn segment_dragging_moves_perpendicularly_keeps_endpoints_and_undoes_once() {
    use fleece::drawing::wire_points;
    for horizontal in [false, true] {
        let mut harness = Harness::new();
        let c = &mut harness.editor.document.circuit;
        c.add(Kind::Input, Point::new(100.0, 200.0));
        c.add(Kind::Output, Point::new(600.0, 400.0));
        c.connect(Endpoint { node: 1, port: 0 }, Endpoint { node: 2, port: 0 })
            .unwrap();
        c.wires[0].waypoints = vec![Point::new(300.0, 220.0), Point::new(300.0, 420.0)];
        harness.frame(vec![]);
        let original = harness.editor.document.circuit.clone();
        let original_route = wire_points(&original, &original.wires[0]);
        let segment = if horizontal { 0 } else { 1 };
        let a = original_route[segment];
        let b = original_route[segment + 1];
        let start = Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
        harness.click(start, Modifiers::NONE);
        assert_eq!(harness.editor.document.circuit, original);
        harness.time += 0.5;
        harness.pointer(start, true, Modifiers::NONE);
        let end = Point::new(start.x + 60.0, start.y + 60.0);
        harness.frame(vec![Event::PointerMoved(harness.screen(end))]);
        harness.pointer(end, false, Modifiers::NONE);
        let moved = harness.editor.document.circuit.clone();
        assert_eq!(moved.nodes, original.nodes);
        assert_eq!(moved.wires[0].from, original.wires[0].from);
        assert_eq!(moved.wires[0].to, original.wires[0].to);
        let route = wire_points(&moved, &moved.wires[0]);
        assert_eq!(route.first(), original_route.first());
        assert_eq!(route.last(), original_route.last());
        assert!(
            route
                .windows(2)
                .all(|s| s[0].x == s[1].x || s[0].y == s[1].y)
        );
        let translated = if horizontal {
            [Point::new(a.x, a.y + 60.0), Point::new(b.x, b.y + 60.0)]
        } else {
            [Point::new(a.x + 60.0, a.y), Point::new(b.x + 60.0, b.y)]
        };
        assert!(route.windows(2).any(|s| s == translated));
        harness.editor.document.validate().unwrap();
        harness.key(Key::Z, Modifiers::COMMAND);
        assert_eq!(harness.editor.document.circuit, original);
        harness.key(Key::Y, Modifiers::COMMAND);
        assert_eq!(harness.editor.document.circuit, moved);
    }
}

#[test]
fn drag_mode_pans_over_nodes_and_shift_d_places_registers() {
    let mut harness = Harness::new();
    harness.key(Key::I, Modifiers::NONE);
    harness.click(Point::new(100.0, 200.0), Modifiers::NONE);
    let original = harness.editor.document.circuit.clone();
    harness.key(Key::D, Modifiers::NONE);
    let pan = harness.editor.document.editor.pan;
    harness.pointer(Point::new(140.0, 220.0), true, Modifiers::NONE);
    let end = harness.screen(Point::new(180.0, 260.0));
    harness.frame(vec![Event::PointerMoved(end)]);
    harness.frame(vec![Event::PointerButton {
        pos: end,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    }]);
    assert_eq!(
        harness.editor.document.editor.pan,
        Point::new(pan.x + 40.0, pan.y + 40.0)
    );
    assert_eq!(harness.editor.document.circuit, original);
    harness.key(Key::D, Modifiers::SHIFT);
    harness.click(Point::new(400.0, 400.0), Modifiers::NONE);
    assert_eq!(harness.editor.document.circuit.nodes.len(), 2);
    assert_eq!(
        harness.editor.document.circuit.nodes[1].kind,
        Kind::Register
    );
    harness.key(Key::Escape, Modifiers::NONE);
    let pan = harness.editor.document.editor.pan;
    harness.pointer(Point::new(70.0, 170.0), true, Modifiers::NONE);
    harness.pointer(Point::new(230.0, 280.0), false, Modifiers::NONE);
    assert_eq!(harness.editor.document.editor.pan, pan);
    harness.key(Key::ArrowRight, Modifiers::NONE);
    assert_eq!(
        harness.editor.document.circuit.nodes[0].position,
        Point::new(120.0, 200.0)
    );
    assert_eq!(
        harness.editor.document.circuit.nodes[1].position,
        Point::new(400.0, 400.0)
    );
}

#[test]
fn right_click_disconnects_only_the_target_wire_on_release_and_undo_restores_it() {
    for wire_mode in [false, true] {
        let mut harness = Harness::new();
        let c = &mut harness.editor.document.circuit;
        c.add(Kind::Input, Point::new(100.0, 200.0));
        c.add(Kind::Output, Point::new(600.0, 200.0));
        c.add(Kind::Output, Point::new(600.0, 500.0));
        c.connect(Endpoint { node: 1, port: 0 }, Endpoint { node: 2, port: 0 })
            .unwrap();
        c.connect(Endpoint { node: 1, port: 0 }, Endpoint { node: 3, port: 0 })
            .unwrap();
        c.wires[1].horizontal_first = false;
        harness.frame(vec![]);
        if wire_mode {
            harness.key(Key::W, Modifiers::NONE);
        }
        let original = harness.editor.document.circuit.clone();
        let p = Point::new(400.0, 220.0);
        harness.right_pointer(p, true);
        assert_eq!(harness.editor.document.circuit, original);
        harness.right_pointer(p, false);
        let disconnected = harness.editor.document.circuit.clone();
        assert_eq!(disconnected.nodes, original.nodes);
        assert_eq!(disconnected.wires, vec![original.wires[1].clone()]);
        harness.key(Key::Z, Modifiers::COMMAND);
        assert_eq!(harness.editor.document.circuit, original);
        harness.key(Key::Y, Modifiers::COMMAND);
        assert_eq!(harness.editor.document.circuit, disconnected);
        harness.editor.document.validate().unwrap();
    }
}

#[test]
fn every_catalog_shortcut_places_the_corresponding_component_with_valid_default_ports() {
    for kind in Kind::ALL {
        let mut harness = Harness::new();
        let shortcut = kind.shortcut();
        let (name, modifiers) = match shortcut.strip_prefix("Shift+") {
            Some(key) => (key, Modifiers::SHIFT),
            None => (shortcut, Modifiers::NONE),
        };
        let key = Key::from_name(name).expect("Component catalog shortcut must name an egui key");
        harness.key(key, modifiers);
        harness.key(Key::Enter, Modifiers::NONE);
        let circuit = &harness.editor.document.circuit;
        assert_eq!(circuit.nodes.len(), 1, "{shortcut}");
        assert_eq!(circuit.nodes[0].kind, kind, "{shortcut}");
        assert_eq!(circuit.nodes[0].inputs, kind.definition().inputs);
        assert_eq!(circuit.nodes[0].outputs, kind.definition().outputs);
        harness.editor.document.validate().unwrap();
    }
}

#[test]
fn right_clicking_input_and_output_pins_disconnects_only_their_attached_wires() {
    for wire_mode in [false, true] {
        for output in [false, true] {
            let mut harness = Harness::new();
            let c = &mut harness.editor.document.circuit;
            c.add(Kind::Input, Point::new(100.0, 200.0));
            c.add(Kind::And, Point::new(400.0, 200.0));
            c.add(Kind::Output, Point::new(600.0, 500.0));
            c.add(Kind::Input, Point::new(100.0, 500.0));
            c.connect(Endpoint { node: 1, port: 0 }, Endpoint { node: 2, port: 0 })
                .unwrap();
            c.connect(Endpoint { node: 1, port: 0 }, Endpoint { node: 3, port: 0 })
                .unwrap();
            c.connect(Endpoint { node: 4, port: 0 }, Endpoint { node: 2, port: 1 })
                .unwrap();
            harness.frame(vec![]);
            if wire_mode {
                harness.key(Key::W, Modifiers::NONE);
            }
            let before = harness.editor.document.circuit.clone();
            let endpoint = Endpoint {
                node: if output { 1 } else { 2 },
                port: 0,
            };
            let position = before
                .node(endpoint.node)
                .unwrap()
                .pin(output, endpoint.port);
            harness.right_pointer(position, true);
            assert_eq!(
                harness.editor.document.circuit, before,
                "Disconnection occurs on release"
            );
            harness.right_pointer(position, false);
            let disconnected = harness.editor.document.circuit.clone();
            assert_eq!(
                disconnected.nodes, before.nodes,
                "Right-clicking a pin must keep its component"
            );
            let expected: Vec<_> = before
                .wires
                .iter()
                .filter(|wire| {
                    if output {
                        wire.from != endpoint
                    } else {
                        wire.to != endpoint
                    }
                })
                .cloned()
                .collect();
            assert_eq!(disconnected.wires, expected);
            assert_eq!(disconnected.wires.len(), if output { 1 } else { 2 });
            harness.editor.document.validate().unwrap();
            harness.key(Key::Z, Modifiers::COMMAND);
            assert_eq!(harness.editor.document.circuit, before);
            harness.key(Key::Y, Modifiers::COMMAND);
            assert_eq!(harness.editor.document.circuit, disconnected);
            // An already disconnected pin remains a pin hit and cannot delete
            // a component or a nearby wire on a second right-click.
            harness.right_click(position);
            assert_eq!(harness.editor.document.circuit, disconnected);
            harness.key(Key::Z, Modifiers::COMMAND);
            assert_eq!(
                harness.editor.document.circuit, before,
                "Empty-pin clicks add no undo entry"
            );
        }
    }
}

#[test]
fn double_click_opens_the_selected_file_and_preserves_unsaved_change_confirmation() {
    for dirty in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target.toml");
        let mut document = Document::new(Circuit::empty());
        document.circuit.title = "Opened by double-click".into();
        document.save(&target).unwrap();
        let mut harness = Harness::new();
        let original_path = dir.path().join("current.toml");
        harness.editor.path = Some(original_path.clone());
        if dirty {
            harness.editor.document.circuit.title = "Unsaved local work".into();
        }
        let before = harness.editor.document.circuit.clone();
        harness.key(Key::O, Modifiers::COMMAND);
        // Let the modal settle before locating its file entry.
        harness.frame(vec![]);
        let position = harness
            .text_positions
            .iter()
            .find(|(text, _)| text == "· target.toml")
            .expect("Open dialog must list the target file")
            .1;
        let view = &harness.editor.document.editor;
        let point = Point::new(
            (position.x - harness.canvas_rect.min.x - view.pan.x) / view.zoom,
            (position.y - harness.canvas_rect.min.y - view.pan.y) / view.zoom,
        );
        harness.pointer(point, true, Modifiers::NONE);
        harness.pointer(point, false, Modifiers::NONE);
        assert_eq!(
            harness.editor.document.circuit, before,
            "A single click only selects"
        );
        assert_eq!(harness.editor.path, Some(original_path.clone()));
        harness.pointer(point, true, Modifiers::NONE);
        harness.pointer(point, false, Modifiers::NONE);
        if dirty {
            // A newly shown egui modal gets an initial sizing pass.
            harness.frame(vec![]);
            harness.frame(vec![]);
            assert_eq!(harness.editor.document.circuit, before);
            assert_eq!(harness.editor.path, Some(original_path));
            assert!(
                harness
                    .text_positions
                    .iter()
                    .any(|(text, _)| text.contains("unsaved changes")),
                "Visible text: {:?}",
                harness
                    .text_positions
                    .iter()
                    .map(|(text, _)| text)
                    .collect::<Vec<_>>()
            );
        } else {
            assert_eq!(harness.editor.path, Some(target));
            assert_eq!(
                harness.editor.document.circuit.title,
                "Opened by double-click"
            );
        }
    }
}

#[test]
fn x_deletes_selection_shift_x_places_xor_and_text_fields_keep_x_typing() {
    let mut harness = Harness::new();
    harness.key(Key::I, Modifiers::NONE);
    harness.key(Key::Enter, Modifiers::NONE);
    let before = harness.editor.document.circuit.clone();
    harness.key(Key::X, Modifiers::NONE);
    assert!(harness.editor.document.circuit.nodes.is_empty());
    harness.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit, before);
    harness.key(Key::X, Modifiers::SHIFT);
    assert_eq!(
        harness.editor.document.circuit, before,
        "Shift+X must not delete"
    );
    harness.key(Key::Enter, Modifiers::NONE);
    assert_eq!(
        harness.editor.document.circuit.nodes.last().unwrap().kind,
        Kind::Xor
    );
    harness.key(Key::E, Modifiers::SHIFT);
    harness.key(Key::Enter, Modifiers::NONE);
    let node = harness.editor.document.circuit.nodes.last().unwrap().id;
    assert_eq!(
        harness.editor.document.circuit.node(node).unwrap().kind,
        Kind::Xnor
    );
    harness.focus(egui::Id::unique(("node_label", node)));
    harness.key(Key::A, Modifiers::COMMAND);
    harness.type_key(Key::X, "x");
    assert_eq!(
        harness.editor.document.circuit.node(node).unwrap().label,
        "x"
    );
    assert_eq!(harness.editor.document.circuit.nodes.len(), 3);
}

#[test]
fn x_deletes_selected_fsm_states_and_undo_restores_transitions() {
    let mut harness = Harness::new();
    harness.editor.new_fsm();
    harness.key(Key::S, Modifiers::NONE);
    harness.key(Key::Enter, Modifiers::NONE);
    harness.key(Key::Enter, Modifiers::NONE);
    harness
        .editor
        .document
        .circuit
        .fsm
        .as_mut()
        .unwrap()
        .add_transition(1, 2)
        .unwrap();
    let before = harness.editor.document.circuit.clone();
    harness.key(Key::X, Modifiers::NONE);
    let fsm = harness.editor.document.circuit.fsm.as_ref().unwrap();
    assert_eq!(fsm.states.len(), 1);
    assert!(fsm.transitions.is_empty());
    harness.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit, before);
}

#[test]
fn wire_bus_annotation_toggle_is_undoable_in_the_inspector() {
    let mut harness = Harness::new();
    let circuit = &mut harness.editor.document.circuit;
    circuit.add(Kind::Input, Point::new(100.0, 200.0));
    circuit.add(Kind::Output, Point::new(600.0, 200.0));
    circuit
        .connect(Endpoint { node: 1, port: 0 }, Endpoint { node: 2, port: 0 })
        .unwrap();
    harness.frame(vec![]);
    harness.click(Point::new(400.0, 220.0), Modifiers::NONE);
    let position = harness
        .text_positions
        .iter()
        .find(|(text, _)| text == "Bus annotation")
        .expect("Selected wire must expose bus annotation control")
        .1;
    let view = &harness.editor.document.editor;
    let point = Point::new(
        (position.x - harness.canvas_rect.min.x - view.pan.x) / view.zoom,
        (position.y - harness.canvas_rect.min.y - view.pan.y) / view.zoom,
    );
    harness.pointer(point, true, Modifiers::NONE);
    harness.pointer(point, false, Modifiers::NONE);
    assert_eq!(harness.editor.document.circuit.wires[0].bus_width, Some(2));
    harness.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit.wires[0].bus_width, None);
    harness.key(Key::Y, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit.wires[0].bus_width, Some(2));
}

#[test]
fn clipboard_requires_the_current_fleece_header() {
    let mut source = Harness::new();
    source.editor.document = Document::new(Circuit::half_adder());
    source.frame(vec![]);
    source.key(Key::A, Modifiers::COMMAND);
    source.key(Key::C, Modifiers::COMMAND);
    let text = source.clipboard;
    assert!(text.starts_with("# FLEECE selection\n"));
    let mut target = Harness::new();
    let old = text.replacen(
        "# FLEECE selection\n",
        "# Logic Circuit Editor selection\n",
        1,
    );
    target.frame(vec![Event::Paste(old)]);
    assert!(target.editor.document.circuit.nodes.is_empty());
    target.frame(vec![Event::Paste(text)]);
    assert_eq!(target.editor.document.circuit.nodes.len(), 6);
    target.editor.document.validate().unwrap();
}

#[test]
fn copying_selected_status_text_does_not_copy_or_modify_the_diagram() {
    let mut harness = Harness::new();
    harness.key(Key::U, Modifiers::NONE);
    harness.key(Key::Enter, Modifiers::NONE);
    harness.key(Key::Escape, Modifiers::NONE);
    let before = harness.editor.document.clone();
    let message = "Unable to load saved diagram";
    harness.editor.report_host_error(message);
    harness.frame(vec![]);
    harness.frame(vec![]);
    let position = harness
        .text_positions
        .iter()
        .find(|(text, _)| text == message)
        .unwrap()
        .1;
    // Triple-click selects the whole status label, without focusing a TextEdit.
    for _ in 0..3 {
        for pressed in [true, false] {
            harness.frame(vec![
                Event::PointerMoved(position),
                Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ]);
        }
    }
    assert!(
        harness
            .context
            .plugin::<egui::text_selection::LabelSelectionState>()
            .lock()
            .has_selection()
    );
    harness.frame(vec![Event::Copy]);
    assert_eq!(harness.clipboard, message);
    harness.key(Key::C, Modifiers::COMMAND);
    assert_eq!(harness.clipboard, message);
    harness.frame(vec![Event::Cut]);
    assert_eq!(harness.clipboard, message);
    assert_eq!(harness.editor.document, before);
    // Returning to the canvas restores normal diagram copy behavior.
    let node = harness.editor.document.circuit.nodes[0].clone();
    harness.click(
        Point::new(node.position.x + 30.0, node.position.y + 30.0),
        Modifiers::NONE,
    );
    harness.key(Key::C, Modifiers::COMMAND);
    assert!(harness.clipboard.starts_with("# FLEECE selection\n"));
}

#[test]
fn cut_and_paste_preserve_diagram_data_and_each_have_one_undo_step() {
    let mut harness = Harness::new();
    harness.editor.document = Document::new(Circuit::half_adder());
    harness.editor.document.circuit.wires[0].bus_width = Some(8);
    harness.frame(vec![]);
    harness.key(Key::A, Modifiers::COMMAND);
    let original = harness.editor.document.circuit.clone();
    harness.key(Key::X, Modifiers::COMMAND);
    let fragment = Document::from_toml(&harness.clipboard).unwrap().circuit;
    assert_eq!(fragment.nodes, original.nodes);
    assert_eq!(fragment.wires, original.wires);
    assert!(harness.editor.document.circuit.nodes.is_empty());
    assert!(harness.editor.document.circuit.wires.is_empty());
    harness.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit, original);
    harness.key(Key::Y, Modifiers::COMMAND);
    assert!(harness.editor.document.circuit.nodes.is_empty());
    harness.frame(vec![Event::PointerMoved(
        harness.screen(Point::new(500.0, 500.0)),
    )]);
    harness.key(Key::V, Modifiers::COMMAND);
    assert_eq!(
        harness.editor.document.circuit.nodes.len(),
        original.nodes.len()
    );
    assert_eq!(
        harness.editor.document.circuit.wires.len(),
        original.wires.len()
    );
    assert_eq!(harness.editor.document.circuit.wires[0].bus_width, Some(8));
    harness.editor.document.validate().unwrap();
    harness.key(Key::Z, Modifiers::COMMAND);
    assert!(harness.editor.document.circuit.nodes.is_empty());
    harness.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit, original);
}

#[test]
fn cut_events_support_fsm_states_and_preserve_internal_transitions_when_pasted() {
    let mut source = Harness::new();
    source.editor.document = Document::from_toml(include_str!("../examples/fsm.toml")).unwrap();
    source.frame(vec![]);
    source.key(Key::A, Modifiers::COMMAND);
    let original = source.editor.document.circuit.clone();
    source.frame(vec![Event::Cut]);
    let fsm = source.editor.document.circuit.fsm.as_ref().unwrap();
    assert!(fsm.states.is_empty() && fsm.transitions.is_empty());
    let mut target = Harness::new();
    target.editor.new_fsm();
    target.frame(vec![]);
    target.clipboard = source.clipboard.clone();
    target.key(Key::V, Modifiers::COMMAND);
    let fsm = target.editor.document.circuit.fsm.as_ref().unwrap();
    assert_eq!(fsm.states.len(), 2);
    assert_eq!(fsm.transitions.len(), 3);
    target.editor.document.validate().unwrap();
    source.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(source.editor.document.circuit, original);
}

#[test]
fn cutting_text_does_not_delete_selected_components() {
    let mut harness = Harness::new();
    harness.key(Key::U, Modifiers::NONE);
    harness.key(Key::Enter, Modifiers::NONE);
    harness.key(Key::Escape, Modifiers::NONE);
    let nodes = harness.editor.document.circuit.nodes.clone();
    harness.focus(egui::Id::unique("circuit_title"));
    harness.key(Key::A, Modifiers::COMMAND);
    harness.key(Key::X, Modifiers::COMMAND);
    assert_eq!(harness.clipboard, "Untitled circuit");
    assert_eq!(harness.editor.document.circuit.title, "");
    assert_eq!(harness.editor.document.circuit.nodes, nodes);
    harness.key(Key::V, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit.title, "Untitled circuit");
}

#[test]
fn placing_a_pin_on_a_wire_connects_and_undo_removes_the_whole_placement() {
    let mut harness = Harness::new();
    let c = &mut harness.editor.document.circuit;
    c.add(Kind::Input, Point::new(0.0, 0.0));
    c.add(Kind::Output, Point::new(400.0, 0.0));
    c.connect(Endpoint { node: 1, port: 0 }, Endpoint { node: 2, port: 0 })
        .unwrap();
    let before = c.clone();
    harness.frame(vec![]);
    harness.key(Key::O, Modifiers::NONE);
    harness.click(Point::new(220.0, 0.0), Modifiers::NONE);
    assert_eq!(harness.editor.document.circuit.nodes.len(), 4);
    assert_eq!(harness.editor.document.circuit.wires.len(), 3);
    harness.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit, before);
}

#[test]
fn dragging_a_pin_onto_a_joint_connects_only_after_actual_movement_in_one_undo_step() {
    let mut harness = Harness::new();
    let c = &mut harness.editor.document.circuit;
    let input = c.add(Kind::Input, Point::new(0.0, 400.0));
    let joint = c.add(Kind::Joint, Point::new(400.0, 220.0));
    let output = c.add(Kind::Output, Point::new(200.0, 200.0));
    c.connect(
        Endpoint {
            node: input,
            port: 0,
        },
        Endpoint {
            node: joint,
            port: 0,
        },
    )
    .unwrap();
    let before = c.clone();
    harness.frame(vec![]);
    harness.click(Point::new(240.0, 220.0), Modifiers::NONE);
    assert_eq!(harness.editor.document.circuit, before);
    harness.pointer(Point::new(240.0, 220.0), true, Modifiers::NONE);
    harness.frame(vec![Event::PointerMoved(
        harness.screen(Point::new(460.0, 220.0)),
    )]);
    harness.pointer(Point::new(460.0, 220.0), false, Modifiers::NONE);
    assert!(
        harness
            .editor
            .document
            .circuit
            .wires
            .iter()
            .any(|wire| wire.from.node == joint && wire.to.node == output)
    );
    harness.editor.document.validate().unwrap();
    harness.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(harness.editor.document.circuit, before);
}

#[test]
fn cut_with_no_selected_nodes_keeps_the_clipboard_and_previous_undo_action() {
    let mut harness = Harness::new();
    harness.key(Key::U, Modifiers::NONE);
    harness.key(Key::Enter, Modifiers::NONE);
    harness.key(Key::Escape, Modifiers::NONE);
    harness.click(Point::new(700.0, 500.0), Modifiers::NONE);
    let before = harness.editor.document.clone();
    harness.clipboard = "Previous clipboard".into();
    harness.key(Key::X, Modifiers::COMMAND);
    assert_eq!(harness.clipboard, "Previous clipboard");
    assert_eq!(harness.editor.document, before);
    harness.key(Key::Z, Modifiers::COMMAND);
    assert!(harness.editor.document.circuit.nodes.is_empty());
}

#[test]
fn pasted_pins_attach_to_existing_joints_in_the_same_paste_undo_step() {
    let mut source = Harness::new();
    source
        .editor
        .document
        .circuit
        .add(Kind::Output, Point::new(200.0, 200.0));
    source.frame(vec![]);
    source.key(Key::A, Modifiers::COMMAND);
    source.key(Key::C, Modifiers::COMMAND);
    let mut target = Harness::new();
    let c = &mut target.editor.document.circuit;
    c.add(Kind::Input, Point::new(0.0, 400.0));
    c.add(Kind::Joint, Point::new(400.0, 220.0));
    c.connect(Endpoint { node: 1, port: 0 }, Endpoint { node: 2, port: 0 })
        .unwrap();
    let before = c.clone();
    target.frame(vec![]);
    target.clipboard = source.clipboard;
    target.frame(vec![Event::PointerMoved(
        target.screen(Point::new(420.0, 200.0)),
    )]);
    target.key(Key::V, Modifiers::COMMAND);
    assert_eq!(target.editor.document.circuit.nodes.len(), 3);
    assert!(
        target
            .editor
            .document
            .circuit
            .wires
            .iter()
            .any(|wire| wire.from.node == 2 && wire.to.node == 3)
    );
    target.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(target.editor.document.circuit, before);
}

#[test]
fn bus_label_drags_across_a_joint_as_one_undo_without_moving_wires() {
    let mut harness = Harness::new();
    let mut circuit = Circuit::empty();
    let a = circuit.add(Kind::Input, Point::new(40.0, 160.0));
    let joint = circuit.add(Kind::Joint, Point::new(360.0, 180.0));
    let b = circuit.add(Kind::Output, Point::new(600.0, 160.0));
    circuit
        .connect(
            Endpoint { node: a, port: 0 },
            Endpoint {
                node: joint,
                port: 0,
            },
        )
        .unwrap();
    circuit
        .connect(
            Endpoint {
                node: joint,
                port: 0,
            },
            Endpoint { node: b, port: 0 },
        )
        .unwrap();
    circuit.wires[0].bus_width = Some(8);
    let (center, horizontal) =
        Wire::bus_anchor(&circuit.wire_path(&circuit.wires[0]), None).unwrap();
    let label =
        fleece::drawing::bus_label_position(center, horizontal, 8, fleece::drawing::TextStyle::Ui);
    harness.editor.document = Document::new(circuit.clone());
    harness.frame(vec![]);
    harness.click(label, Modifiers::NONE);
    assert_eq!(&harness.editor.document.circuit, &circuit);
    harness.time += 0.5;
    harness.pointer(label, true, Modifiers::NONE);
    let end = Point::new(500.0, 164.0);
    harness.frame(vec![Event::PointerMoved(harness.screen(end))]);
    harness.pointer(end, false, Modifiers::NONE);
    let moved = harness.editor.document.circuit.clone();
    assert_eq!(moved.wires[0].bus_width, None);
    assert_eq!(moved.wires[1].bus_width, Some(8));
    let (anchor, _) = Wire::bus_anchor(
        &moved.wire_path(&moved.wires[1]),
        moved.wires[1].bus_position,
    )
    .unwrap();
    assert!((anchor.x - 500.0).abs() < 0.01);
    assert_eq!(circuit.nodes, moved.nodes);
    for (old, new) in circuit.wires.iter().zip(&moved.wires) {
        assert_eq!(
            (old.from, old.to, &old.waypoints),
            (new.from, new.to, &new.waypoints)
        );
    }
    harness.key(Key::Z, Modifiers::CTRL | Modifiers::COMMAND);
    assert_eq!(&harness.editor.document.circuit, &circuit);
    harness.key(
        Key::Z,
        Modifiers::CTRL | Modifiers::COMMAND | Modifiers::SHIFT,
    );
    assert_eq!(&harness.editor.document.circuit, &moved);
}

#[test]
fn export_dialog_previews_logic_and_fsm_in_native_and_browser_modes() {
    for external in [false, true] {
        for circuit in [Circuit::half_adder(), Circuit::new_fsm()] {
            let mut circuit = circuit;
            if let Some(fsm) = &mut circuit.fsm {
                fsm.add_state(Point::new(100.0, 100.0));
            }
            let mut harness = Harness::new();
            harness.editor = if external {
                Editor::with_external_files(Document::new(circuit.clone()))
            } else {
                Editor::new(Document::new(circuit.clone()), None)
            };
            harness.frame(vec![]);
            harness.key(Key::E, Modifiers::CTRL | Modifiers::COMMAND);
            harness.frame(vec![]);
            assert!(
                harness
                    .text_positions
                    .iter()
                    .any(|(text, _)| text == "Export preview")
            );
            assert_eq!(&harness.editor.document.circuit, &circuit);
        }
    }
}

#[test]
fn bus_slash_drags_around_an_elbow_without_jumping_to_a_crossing_wire() {
    let mut harness = Harness::new();
    let mut circuit = Circuit::empty();
    let a = circuit.add(Kind::Input, Point::new(40.0, 100.0));
    let b = circuit.add(Kind::Output, Point::new(400.0, 300.0));
    circuit
        .connect(Endpoint { node: a, port: 0 }, Endpoint { node: b, port: 0 })
        .unwrap();
    let c = circuit.add(Kind::Input, Point::new(200.0, 200.0));
    let d = circuit.add(Kind::Output, Point::new(600.0, 200.0));
    circuit
        .connect(Endpoint { node: c, port: 0 }, Endpoint { node: d, port: 0 })
        .unwrap();
    circuit.wires[0].bus_width = Some(16);
    let (center, _) = Wire::bus_anchor(&circuit.wire_path(&circuit.wires[0]), None).unwrap();
    harness.editor.document = Document::new(circuit.clone());
    harness.frame(vec![]);
    harness.time += 0.5;
    harness.pointer(center, true, Modifiers::NONE);
    let end = Point::new(380.0, 220.0);
    harness.frame(vec![Event::PointerMoved(harness.screen(end))]);
    harness.pointer(end, false, Modifiers::NONE);
    let moved = &harness.editor.document.circuit;
    let (anchor, horizontal) = Wire::bus_anchor(
        &moved.wire_path(&moved.wires[0]),
        moved.wires[0].bus_position,
    )
    .unwrap();
    assert!(!horizontal);
    assert!(
        (anchor.x - end.x).abs() < 0.01 && (anchor.y - end.y).abs() < 0.01,
        "anchor={anchor:?} end={end:?} center={center:?}"
    );
    assert_eq!(moved.wires[1], circuit.wires[1]);
    assert_eq!(moved.wires[0].waypoints, circuit.wires[0].waypoints);
    assert_eq!(moved.nodes, circuit.nodes);
}
