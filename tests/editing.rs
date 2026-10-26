use std::collections::BTreeSet;

use fleece::{
    drawing::{self, Cmd, Primitive},
    editor_drawing::build_editor_scene,
    export::{self, ExportOptions},
    model::*,
    theme::Theme,
};

#[test]
fn both_renderers_draw_input_bubbles_and_bubbled_gates_export_to_png_and_svg() {
    let mut circuit = Circuit::empty();
    let id = circuit.add(Kind::And, Point::new(100.0, 100.0));
    circuit
        .node_mut(id)
        .unwrap()
        .set_bubble(false, 0, true)
        .unwrap();
    circuit
        .node_mut(id)
        .unwrap()
        .set_bubble(true, 0, true)
        .unwrap();
    let node = circuit.node(id).unwrap();
    let contains_circle = |scene: &drawing::Scene, center: Point| {
        scene.items.iter().any(|item| matches!(item,
            Primitive::Path {path, fill: Some(_), ..}
                if matches!(path.0.first(), Some(Cmd::Move(point)) if *point == Point::new(center.x + 6.0, center.y))
                    && path.0.iter().filter(|cmd| matches!(cmd, Cmd::Cubic(..))).count() == 4
        ))
    };
    let editor = build_editor_scene(&circuit, &Theme::load_reference("vscode").unwrap().canvas);
    let document = drawing::build_scene(&circuit);
    assert!(contains_circle(
        &editor,
        Point::new(node.position.x - 6.0, node.pin(false, 0).y)
    ));
    assert!(contains_circle(
        &document,
        Point::new(
            node.position.x - 6.0,
            drawing::diagram_pin(node, false, 0).y
        )
    ));
    let svg = export::svg(&circuit, ExportOptions::default()).unwrap();
    let png = export::png(
        &circuit,
        ExportOptions {
            scale: 1,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!svg.is_empty() && !png.is_empty());
}

#[test]
fn paste_preserves_register_ports_and_rejects_invalid_or_out_of_bounds_fragments() {
    let mut fragment = Circuit::empty();
    let register = fragment.add(Kind::Register, Point::new(100.0, 100.0));
    fragment.resize_register(register, 3).unwrap();
    let mut target = Circuit::half_adder();
    let selected = target
        .paste_fragment(&fragment, Point::new(40.0, 40.0))
        .unwrap();
    assert_eq!(selected, BTreeSet::from([7]));
    assert_eq!(
        target.node(7).unwrap().outputs,
        fragment.node(1).unwrap().outputs
    );
    let before = target.clone();
    assert!(
        target
            .paste_fragment(&fragment, Point::new(100000.0, 0.0))
            .is_err()
    );
    assert_eq!(target, before);
    fragment.wires.push(Wire {
        from: Endpoint { node: 99, port: 0 },
        to: Endpoint { node: 1, port: 0 },
        waypoints: vec![],
        horizontal_first: true,
        bus_width: None,
        bus_position: None,
    });
    assert!(target.paste_fragment(&fragment, Point::default()).is_err());
    assert_eq!(target, before);
    let mut full = Circuit::empty();
    for _ in 0..1000 {
        full.add(Kind::Input, Point::default());
    }
    let before = full.clone();
    assert!(
        full.paste_fragment(&Circuit::half_adder(), Point::default())
            .is_err()
    );
    assert_eq!(full, before);
}

#[test]
fn regular_wires_have_at_most_one_turn_in_either_direction_and_preserve_the_flag() {
    for destination in [
        Point::new(600.0, 400.0),
        Point::new(-200.0, -100.0),
        Point::new(600.0, 200.0),
    ] {
        let mut c = Circuit::empty();
        c.add(Kind::Input, Point::new(100.0, 200.0));
        c.add(Kind::Output, destination);
        c.connect(Endpoint { node: 1, port: 0 }, Endpoint { node: 2, port: 0 })
            .unwrap();
        for horizontal_first in [false, true] {
            c.wires[0].horizontal_first = horizontal_first;
            for route in [
                drawing::wire_points(&c, &c.wires[0]),
                drawing::diagram_wire_points(&c, &c.wires[0]),
            ] {
                assert!((2..=3).contains(&route.len()));
                for pair in route.windows(2) {
                    assert!(pair[0].x == pair[1].x || pair[0].y == pair[1].y);
                }
                if route.len() == 3 {
                    if horizontal_first {
                        assert_eq!(route[0].y, route[1].y);
                    } else {
                        assert_eq!(route[0].x, route[1].x);
                    }
                }
            }
            let restored =
                Document::from_toml(&Document::new(c.clone()).to_toml().unwrap()).unwrap();
            assert_eq!(restored.circuit, c);
        }
    }
}

#[test]
fn editor_and_exports_use_neutral_ink_and_explicit_wire_colors() {
    let mut c = Circuit::half_adder();
    let palette = Theme::original(true).canvas;
    let first = build_editor_scene(&c, &palette);
    let svg = export::svg(&c, ExportOptions::default()).unwrap();
    let first_ink = match &first.items[0] {
        Primitive::Path {
            stroke: Some((color, _)),
            ..
        } => *color,
        _ => panic!("First editor primitive should be a wire"),
    };
    assert_eq!(first_ink, palette.gate_outline.0);
    let scene = build_editor_scene(&c, &palette);
    assert!(!scene.items.iter().any(
        |item| matches!(item,Primitive::Text {text,..} if ["0","1","?"].contains(&text.as_str()))
    ));
    assert_eq!(export::svg(&c, ExportOptions::default()).unwrap(), svg);
    c.node_mut(1).unwrap().set_net_color(0, NetColor::Red);
    let scene = build_editor_scene(&c, &palette);
    assert!(
        matches!(&scene.items[0],Primitive::Path {stroke:Some((color,_)),..} if *color==drawing::net_color(NetColor::Red))
    );
}

#[test]
fn automatic_labels_number_inputs_and_outputs_independently_and_allow_duplicates() {
    let mut circuit = Circuit::empty();
    for (kind, label) in [
        (Kind::And, "AND"),
        (Kind::Input, "Input 1"),
        (Kind::Register, "D-flip-flop"),
        (Kind::Output, "Output 1"),
        (Kind::Input, "Input 2"),
        (Kind::And, "AND"),
        (Kind::Output, "Output 2"),
        (Kind::Module, "Custom module"),
    ] {
        let id = circuit.add(kind, Point::default());
        assert_eq!(circuit.node(id).unwrap().label, label);
    }
    let mut doc = Document::new(circuit);
    for id in [2, 5, 4] {
        doc.circuit.node_mut(id).unwrap().label = "same".into();
    }
    let fragment = doc.circuit.selection(&BTreeSet::from([2, 4, 5]));
    let pasted = doc
        .circuit
        .paste_fragment(&fragment, Point::new(40.0, 40.0))
        .unwrap();
    assert!(
        pasted
            .iter()
            .all(|id| doc.circuit.node(*id).unwrap().label == "same")
    );
    doc.validate().unwrap();
    assert_eq!(Document::from_toml(&doc.to_toml().unwrap()).unwrap(), doc);
}
