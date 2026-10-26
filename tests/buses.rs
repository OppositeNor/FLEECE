use fleece::{
    drawing::{self, Primitive, Scene, TextStyle},
    editor_drawing::build_editor_scene,
    export::{self, ExportOptions},
    history::History,
    model::*,
    theme::Theme,
};
use std::collections::BTreeSet;

fn diagram() -> Document {
    let mut circuit = Circuit::empty();
    circuit.add(Kind::Input, Point::new(100.0, 200.0));
    circuit.add(Kind::Output, Point::new(600.0, 400.0));
    circuit
        .connect(Endpoint { node: 1, port: 0 }, Endpoint { node: 2, port: 0 })
        .unwrap();
    Document::new(circuit)
}

#[test]
fn bus_annotations_validate_roundtrip_and_clear() {
    let mut document = diagram();
    assert!(!document.to_toml().unwrap().contains("bus_width"));
    document
        .circuit
        .set_wire_bus_width(Endpoint { node: 2, port: 0 }, Some(16))
        .unwrap();
    assert_eq!(document.circuit.wires[0].bus_width, Some(16));
    assert_eq!(
        Document::from_toml(&document.to_toml().unwrap()).unwrap(),
        document
    );
    let before = document.clone();
    for width in [0, MAX_BUS_WIDTH + 1] {
        assert!(
            document
                .circuit
                .set_wire_bus_width(Endpoint { node: 2, port: 0 }, Some(width))
                .is_err()
        );
        assert_eq!(document, before);
    }
    assert!(
        document
            .circuit
            .set_wire_bus_width(Endpoint { node: 1, port: 0 }, Some(8))
            .is_err()
    );
    assert_eq!(document, before);
    document
        .circuit
        .set_wire_bus_width(Endpoint { node: 2, port: 0 }, None)
        .unwrap();
    assert_eq!(document.circuit.wires[0].bus_width, None);
    document.circuit.wires[0].bus_width = Some(0);
    assert!(document.validate().is_err());
}

#[test]
fn bus_slashes_and_widths_render_on_horizontal_and_vertical_segments() {
    for vertical in [false, true] {
        let mut scene = Scene::default();
        let points = if vertical {
            vec![Point::new(100.0, 0.0), Point::new(100.0, 200.0)]
        } else {
            vec![Point::new(0.0, 100.0), Point::new(200.0, 100.0)]
        };
        drawing::draw_bus_annotation(&mut scene, &points, 16, drawing::INK, TextStyle::Math);
        let Primitive::Path { path, .. } = &scene.items[0] else {
            panic!("Bus marker must be a slash")
        };
        let slash = path.flattened();
        assert_eq!(slash.len(), 2);
        assert_eq!((slash[0].x + slash[1].x) / 2.0, 100.0);
        assert_eq!((slash[0].y + slash[1].y) / 2.0, 100.0);
        assert!(slash[0].x < slash[1].x && slash[0].y > slash[1].y);
        assert!(scene.items.iter().any(
            |item| matches!(item,Primitive::Text{text,style:TextStyle::Math,..} if text=="16")
        ));
    }
    let mut document = diagram();
    document
        .circuit
        .set_wire_bus_width(Endpoint { node: 2, port: 0 }, Some(16))
        .unwrap();
    let scene = build_editor_scene(&document.circuit, &Theme::original(true).canvas);
    assert!(
        scene
            .items
            .iter()
            .any(|item| matches!(item,Primitive::Text{text,style:TextStyle::Ui,..} if text=="16"))
    );
    assert!(
        export::svg(&document.circuit, ExportOptions::default())
            .unwrap()
            .contains("aria-label=\"16\"")
    );
}

#[test]
fn bus_metadata_survives_history_copy_split_and_joint_removal() {
    let mut document = diagram();
    let before = document.circuit.clone();
    document
        .circuit
        .set_wire_bus_width(Endpoint { node: 2, port: 0 }, Some(8))
        .unwrap();
    let mut history = History::default();
    history.record(before.clone(), &document.circuit);
    assert!(history.undo(&mut document.circuit));
    assert_eq!(document.circuit, before);
    assert!(history.redo(&mut document.circuit));
    let route = drawing::wire_points(&document.circuit, &document.circuit.wires[0]);
    let joint = document
        .circuit
        .insert_joint(0, route[1], vec![], vec![])
        .unwrap();
    assert!(
        document
            .circuit
            .wires
            .iter()
            .all(|wire| wire.bus_width == Some(8))
    );
    document.circuit.wires[1].bus_width = None;
    document.circuit.delete(&BTreeSet::from([joint]));
    assert_eq!(document.circuit.wires[0].bus_width, Some(8));
    let fragment = document.circuit.selection(&BTreeSet::from([1, 2]));
    document
        .circuit
        .paste_fragment(&fragment, Point::new(40.0, 40.0))
        .unwrap();
    assert!(
        document
            .circuit
            .wires
            .iter()
            .all(|wire| wire.bus_width == Some(8))
    );
    document.validate().unwrap();
}

#[test]
fn register_exports_omit_component_bit_count_captions() {
    for bits in 1..=8 {
        let mut circuit = Circuit::empty();
        let id = circuit.add(Kind::Register, Point::default());
        circuit.resize_register(id, bits).unwrap();
        let scene = export::export_scene(&circuit, ExportOptions::default())
            .unwrap()
            .0;
        assert!(!scene.items.iter().any(|item|matches!(item,Primitive::Text{text,..} if text.ends_with(" bit") || text.ends_with(" bits"))));
    }
}

#[test]
fn bus_positions_transfer_across_joint_branches_and_roundtrip() {
    let mut document = diagram();
    document.circuit.wires[0].bus_width = Some(16);
    let joint = document
        .circuit
        .insert_joint(0, Point::new(400.0, 220.0), vec![], vec![])
        .unwrap();
    let output = document.circuit.add(Kind::Output, Point::new(600.0, 600.0));
    document
        .circuit
        .connect(
            Endpoint {
                node: joint,
                port: 0,
            },
            Endpoint {
                node: output,
                port: 0,
            },
        )
        .unwrap();
    let before = document.circuit.clone();
    document.circuit.move_bus_annotation(0, 2, 0.75).unwrap();
    assert_eq!(document.circuit.wires[0].bus_width, None);
    assert_eq!(document.circuit.wires[2].bus_width, Some(16));
    assert_eq!(document.circuit.wires[2].bus_position, Some(0.75));
    for (old, new) in before.wires.iter().zip(&document.circuit.wires) {
        assert_eq!(
            (old.from, old.to, &old.waypoints),
            (new.from, new.to, &new.waypoints)
        );
    }
    assert_eq!(
        Document::from_toml(&document.to_toml().unwrap()).unwrap(),
        document
    );
    let fragment = document
        .circuit
        .selection(&document.circuit.nodes.iter().map(|n| n.id).collect());
    let mut pasted = Circuit::empty();
    pasted
        .paste_fragment(&fragment, Point::new(40.0, 40.0))
        .unwrap();
    assert_eq!(pasted.wires[2].bus_position, Some(0.75));
    for position in [-0.1, 1.1, f32::NAN, f32::INFINITY] {
        let before = document.clone();
        assert!(
            document
                .circuit
                .move_bus_annotation(2, 0, position)
                .is_err()
        );
        assert_eq!(document, before);
        let mut invalid = before;
        invalid.circuit.wires[2].bus_position = Some(position);
        assert!(invalid.validate().is_err());
    }
    document.circuit.wires[0].bus_width = Some(8);
    let before = document.clone();
    assert!(document.circuit.move_bus_annotation(2, 0, 0.5).is_err());
    assert_eq!(document, before);
    document
        .circuit
        .set_wire_bus_width(document.circuit.wires[2].to, None)
        .unwrap();
    assert_eq!(document.circuit.wires[2].bus_position, None);
}

#[test]
fn marker_projects_around_elbows_and_cannot_move_to_an_unrelated_wire() {
    let path = [
        Point::new(0.0, 0.0),
        Point::new(200.0, 0.0),
        Point::new(200.0, 200.0),
    ];
    assert_eq!(
        Wire::bus_anchor(&path, Some(0.75)),
        Some((Point::new(200.0, 100.0), false))
    );
    let (distance, fraction) = Wire::project_bus_position(&path, Point::new(210.0, 100.0)).unwrap();
    assert!((distance - 10.0).abs() < 0.001);
    assert!((fraction - 0.75).abs() < 0.001);
    let mut document = diagram();
    document.circuit.wires[0].bus_width = Some(8);
    let a = document.circuit.add(Kind::Input, Point::new(100.0, 500.0));
    let b = document.circuit.add(Kind::Output, Point::new(600.0, 100.0));
    document
        .circuit
        .connect(Endpoint { node: a, port: 0 }, Endpoint { node: b, port: 0 })
        .unwrap();
    let before = document.clone();
    assert!(document.circuit.move_bus_annotation(0, 1, 0.5).is_err());
    assert_eq!(document, before);
    let mut scene = Scene::default();
    drawing::draw_positioned_bus_annotation(
        &mut scene,
        &path,
        Some(0.75),
        8,
        drawing::INK,
        TextStyle::Math,
    );
    let Primitive::Path { path, .. } = &scene.items[0] else {
        panic!()
    };
    assert_eq!(path.flattened()[0], Point::new(195.0, 107.0));
}
