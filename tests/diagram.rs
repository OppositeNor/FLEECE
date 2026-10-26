use fleece::{
    drawing::*,
    export::{self, ExportOptions},
    model::*,
    notation,
};

#[test]
fn math_notation_lays_out_subscripts_primes_and_overbars() {
    let label = notation::layout(r"s_1' = \bar{s_1} s_0 B", 20.0);
    assert!(label.width > 50.0);
    assert!(
        label
            .runs
            .iter()
            .any(|run| run.text == "1" && run.y > 0.0 && run.size < 20.0)
    );
    assert!(label.runs.iter().any(|run| run.text.contains('′')));
    assert_eq!(label.bars.len(), 1);
    assert!(label.bars[0].2 < 0.0);
    let superscript = notation::layout("x^{2}", 20.0);
    assert!(
        superscript
            .runs
            .iter()
            .any(|run| run.text == "2" && run.y < 0.0)
    );
    for literal in [
        r"unterminated \bar{A",
        "x_",
        r"\unknown{label}",
        r"escaped\_label",
    ] {
        assert!(notation::layout(literal, 20.0).width.is_finite());
    }
}

#[test]
fn colored_nets_and_named_wire_terminals_are_present_in_exports() {
    let mut circuit = Circuit::half_adder();
    circuit
        .node_mut(1)
        .unwrap()
        .set_net_color(0, NetColor::Blue);
    circuit
        .node_mut(3)
        .unwrap()
        .set_net_color(0, NetColor::Purple);
    let svg = export::svg(&circuit, ExportOptions::default()).unwrap();
    assert!(svg.contains("#005aeb"));
    assert!(svg.contains("#7b1db6"));
    assert!(!svg.contains("aria-label=\"INPUT\""));
    assert!(!svg.contains("aria-label=\"OUTPUT\""));
    for label in ["A", "B", "Sum", "Carry"] {
        assert!(svg.contains(&format!("aria-label=\"{label}\"")));
    }
    assert!(!svg.contains("<text"));
}

#[test]
fn junctions_mark_shared_branches_but_leave_unrelated_crossings_alone() {
    let paths = vec![
        vec![Point::new(0.0, 0.0), Point::new(100.0, 0.0)],
        vec![
            Point::new(0.0, 0.0),
            Point::new(50.0, 0.0),
            Point::new(50.0, 80.0),
        ],
    ];
    assert_eq!(junction_points(&paths), vec![Point::new(50.0, 0.0)]);
    assert!(junction_points(&paths[..1]).is_empty());
}

#[test]
fn every_symbol_including_registers_exports_to_png_and_svg() {
    for kind in Kind::ALL {
        let mut circuit = Circuit::empty();
        circuit.add(kind, Point::new(-40.0, -20.0));
        let scene = build_scene(&circuit);
        assert!(scene.bounds().width() > 50.0);
        let bytes = export::png(
            &circuit,
            ExportOptions {
                scale: 1,
                ..Default::default()
            },
        )
        .unwrap();
        let image = tiny_skia::Pixmap::decode_png(&bytes).unwrap();
        assert!(image.width() > 40 && image.height() > 40);
        assert!(
            export::svg(&circuit, ExportOptions::default())
                .unwrap()
                .contains("<path")
        );
    }
}

#[test]
fn register_clock_label_matches_other_pin_fonts_and_clears_the_clock_triangle() {
    for bits in 1..=8 {
        let mut circuit = Circuit::empty();
        let id = circuit.add(Kind::Register, Point::new(100.0, 100.0));
        circuit.resize_register(id, bits).unwrap();
        let scene = export::export_scene(&circuit, ExportOptions::default())
            .unwrap()
            .0;
        let label = |name: &str| {
            scene
                .items
                .iter()
                .find_map(|item| match item {
                    Primitive::Text {
                        text,
                        position,
                        size,
                        style,
                        ..
                    } if text == name => Some((*position, *size, *style)),
                    _ => None,
                })
                .expect("Register control label must be present")
        };
        let (position, size, style) = label("CLK");
        let (_, reset_size, reset_style) = label("RST");
        assert_eq!(style, TextStyle::Math);
        assert_eq!((size, style), (reset_size, reset_style));
        let node = circuit.node(id).unwrap();
        let clock_pin = diagram_pin(node, false, bits);
        assert_eq!(position.y, clock_pin.y);
        let left = position.x - styled_text_width("CLK", size, style) / 2.0;
        assert!((left - (node.position.x + 23.0)).abs() < 0.001);
        assert!(
            position.x + styled_text_width("CLK", size, style) / 2.0
                < node.position.x + diagram_size(node).x
        );
    }
}
