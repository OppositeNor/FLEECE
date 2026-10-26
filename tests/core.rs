use fleece::{
    drawing::{Primitive, build_scene, text_paths},
    export::{self, ExportOptions},
    history::History,
    model::*,
};
use std::collections::BTreeSet;

fn endpoint(node: u64, port: usize) -> Endpoint {
    Endpoint { node, port }
}

#[test]
fn feedback_and_floating_inputs_validate_and_can_be_arranged() {
    let mut circuit = Circuit::empty();
    let a = circuit.add(Kind::Not, Point::default());
    let b = circuit.add(Kind::Not, Point::new(200.0, 0.0));
    circuit.connect(endpoint(a, 0), endpoint(b, 0)).unwrap();
    circuit.connect(endpoint(b, 0), endpoint(a, 0)).unwrap();
    Document::new(circuit.clone()).validate().unwrap();
    circuit.auto_layout();
    Document::new(circuit).validate().unwrap();
}

#[test]
fn reconnect_replaces_one_driver_and_deletion_cleans_wires() {
    let mut circuit = Circuit::half_adder();
    let original = circuit.wires.len();
    circuit.connect(endpoint(1, 0), endpoint(5, 0)).unwrap();
    assert_eq!(circuit.wires.len(), original);
    assert_eq!(
        circuit
            .wires
            .iter()
            .filter(|w| w.to == endpoint(5, 0))
            .count(),
        1
    );
    assert!(circuit.connect(endpoint(5, 0), endpoint(1, 0)).is_err());
    circuit.delete(&BTreeSet::from([1]));
    assert!(
        circuit
            .wires
            .iter()
            .all(|w| w.from.node != 1 && w.to.node != 1)
    );
}

#[test]
fn toml_roundtrip_preserves_layout_modules_and_routes() {
    let mut document = Document::new(Circuit::half_adder());
    document.editor.layout = Layout::LibraryRight;
    document.editor.zoom = 0.75;
    let module = document.circuit.add(Kind::Module, Point::new(-100.0, 50.0));
    document
        .circuit
        .node_mut(module)
        .unwrap()
        .resize_ports(3, 2);
    let text = document.to_toml().unwrap();
    assert_eq!(Document::from_toml(&text).unwrap(), document);
    assert!(
        Document::from_toml(&text.replace("format_version = 1", "format_version = 99")).is_err()
    );
    assert!(Document::from_toml(&text.replace("zoom = 0.75", "zoom = nan")).is_err());
    assert!(Document::from_toml(&format!("typo = true\n{text}")).is_err());
}

#[test]
fn validation_rejects_broken_ids_pins_drivers_and_port_counts() {
    let valid = Document::new(Circuit::half_adder());
    let mut document = valid.clone();
    document.circuit.nodes[1].id = 1;
    assert!(document.validate().is_err());
    let mut document = valid.clone();
    document.circuit.wires[0].to.port = 99;
    assert!(document.validate().is_err());
    let mut document = valid;
    document
        .circuit
        .wires
        .push(document.circuit.wires[0].clone());
    assert!(document.validate().is_err());
    let mut document = Document::new(Circuit::empty());
    let module = document.circuit.add(Kind::Module, Point::default());
    document.circuit.node_mut(module).unwrap().outputs = 5;
    assert!(document.validate().is_err());
}

#[test]
fn history_restores_wires_and_branches_correctly() {
    let mut circuit = Circuit::half_adder();
    let original = circuit.clone();
    let mut history = History::default();
    circuit.delete(&BTreeSet::from([1]));
    history.record(original.clone(), &circuit);
    let deleted = circuit.clone();
    assert!(history.undo(&mut circuit));
    assert_eq!(circuit, original);
    assert!(history.redo(&mut circuit));
    assert_eq!(circuit, deleted);
    history.undo(&mut circuit);
    let before = circuit.clone();
    circuit.title = "A different edit".into();
    history.record(before, &circuit);
    assert!(!history.can_redo());
}

#[test]
fn group_duplicate_preserves_internal_connections() {
    let mut circuit = Circuit::half_adder();
    let selected = circuit.duplicate(&BTreeSet::from([1, 3]));
    assert_eq!(selected.len(), 2);
    assert_eq!(circuit.nodes.len(), 8);
    assert_eq!(circuit.wires.len(), 7);
    Document::new(circuit).validate().unwrap();
}

#[test]
fn png_is_cropped_transparent_when_requested_and_independent_of_editor_view() {
    let circuit = Circuit::half_adder();
    let options = ExportOptions {
        scale: 1,
        transparent: true,
        ..Default::default()
    };
    let bytes = export::png(&circuit, options).unwrap();
    let image = tiny_skia::Pixmap::decode_png(&bytes).unwrap();
    assert!(image.width() > 700 && image.width() < 1000);
    assert!(image.height() > 200 && image.height() < 500);
    assert_eq!(image.pixel(0, 0).unwrap().alpha(), 0);
    assert!(image.pixels().iter().filter(|p| p.alpha() > 0).count() > 3000);
    let opaque = export::png(
        &circuit,
        ExportOptions {
            scale: 1,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        tiny_skia::Pixmap::decode_png(&opaque)
            .unwrap()
            .pixel(0, 0)
            .unwrap()
            .alpha(),
        255
    );
}

#[test]
fn vector_export_embeds_outlines_and_handles_negative_positions_and_labels() {
    let mut circuit = Circuit::empty();
    let id = circuit.add(Kind::Input, Point::new(-200.0, -100.0));
    circuit.node_mut(id).unwrap().label = "A < B & C".into();
    let svg = export::svg(&circuit, ExportOptions::default()).unwrap();
    assert!(svg.contains("A &lt; B &amp; C"));
    assert!(!svg.contains("<text"));
    assert!(!svg.contains("INSPECTOR"));
    assert!(!text_paths(Point::default(), "Signal", 16.0).is_empty());
    let scene = build_scene(&circuit);
    assert!(scene.bounds().min.x < -200.0);
    assert!(
        scene
            .items
            .iter()
            .any(|p| matches!(p, Primitive::Text { .. }))
    );
}

#[test]
fn unsafe_image_sizes_and_empty_circuits_are_rejected() {
    assert!(export::png(&Circuit::empty(), ExportOptions::default()).is_err());
    let mut circuit = Circuit::half_adder();
    circuit.nodes[0].position.x = -90_000.0;
    assert!(export::png(&circuit, ExportOptions::default()).is_err());
    assert!(
        export::png(
            &circuit,
            ExportOptions {
                scale: 0,
                ..Default::default()
            }
        )
        .is_err()
    );
}
