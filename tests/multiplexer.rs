use fleece::{editor_drawing, export, model::*, theme::Theme};

#[test]
fn resizing_mux_preserves_selector_identity_and_removes_deleted_pins() {
    let mut document = Document::new(Circuit::empty());
    let id = document.circuit.add(Kind::Mux, Point::new(400.0, 100.0));
    document.circuit.resize_mux(id, 2).unwrap();
    let c = &mut document.circuit;
    for port in 0..6 {
        let id = c.add(Kind::Input, Point::new(0.0, port as f32 * 100.0));
        c.connect(Endpoint { node: id, port: 0 }, Endpoint { node: 1, port })
            .unwrap();
    }
    let before = document.clone();
    document.circuit.resize_mux(1, 3).unwrap();
    let c = &document.circuit;
    assert_eq!(c.node(1).unwrap().inputs, 11);
    assert_eq!(
        c.wires.iter().map(|w| w.to.port).collect::<Vec<_>>(),
        [0, 1, 2, 3, 8, 9]
    );
    assert_eq!(
        Document::from_toml(&document.to_toml().unwrap()).unwrap(),
        document
    );
    document.circuit.resize_mux(1, 1).unwrap();
    assert_eq!(
        document
            .circuit
            .wires
            .iter()
            .map(|w| (w.from.node, w.to.port))
            .collect::<Vec<_>>(),
        [(2, 0), (3, 1), (6, 2)]
    );
    let valid = document.clone();
    assert!(document.circuit.resize_mux(1, 4).is_err());
    assert_eq!(document, valid);
    document.circuit.node_mut(1).unwrap().inputs = 5;
    assert!(document.validate().is_err());
    before.validate().unwrap();
}

#[test]
fn mux_ports_render_and_export_with_grid_alignment_for_each_selector_width() {
    for bits in 1..=3 {
        let mut c = Circuit::empty();
        let id = c.add(Kind::Mux, Point::new(200.0, 100.0));
        c.resize_mux(id, bits).unwrap();
        let node = c.node(id).unwrap();
        for port in 0..node.inputs {
            let pin = node.pin(false, port);
            assert_eq!(pin, pin.snapped());
            assert!(pin.y > node.position.y && pin.y < node.position.y + node.size().y);
        }
        let editor = editor_drawing::build_editor_scene(&c, &Theme::original(true).canvas);
        assert!(!editor.items.is_empty());
        let svg = export::svg(&c, export::ExportOptions::default()).unwrap();
        assert!(svg.contains("MUX"));
        let d = Document::new(c);
        d.validate().unwrap();
        assert_eq!(Document::from_toml(&d.to_toml().unwrap()).unwrap(), d);
    }
}
