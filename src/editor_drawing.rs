//! Interactive editor appearance, separate from the report/export renderer.
use crate::{drawing::*, model::*, theme::CanvasPalette};

/// Build interactive symbols in world coordinates using the active UI palette.
/// Wires use explicit net colors or neutral outline ink, never simulated values.
/// The caller applies pan/zoom while painting the returned scene.
pub fn build_editor_scene(circuit: &Circuit, palette: &CanvasPalette) -> Scene {
    if let Some(fsm) = &circuit.fsm {
        return build_fsm_scene(
            fsm,
            palette.gate_outline.0,
            palette.gate_fill.0,
            palette.label.0,
        );
    }
    let mut scene = Scene::default();
    for wire in &circuit.wires {
        let color = connection_color(circuit, wire.from, palette);
        scene.path(
            PathData::polyline(&wire_points(circuit, wire)),
            None,
            Some((color, 2.0)),
        );
        if let Some(width) = wire.bus_width {
            draw_positioned_bus_annotation(
                &mut scene,
                &wire_points(circuit, wire),
                wire.bus_position,
                width,
                color,
                TextStyle::Ui,
            );
        }
        for point in wire_joints(circuit, wire) {
            scene.path(PathData::circle(point, 3.4), Some(color), None);
        }
    }
    for node in &circuit.nodes {
        draw_node(&mut scene, circuit, node, palette);
    }
    scene
}

fn draw_node(scene: &mut Scene, circuit: &Circuit, node: &Node, palette: &CanvasPalette) {
    if node.kind == Kind::Joint {
        // Every joint stays visible and easy to find while editing.
        scene.path(
            PathData::circle(node.position, 3.4),
            Some(connection_color(
                circuit,
                Endpoint {
                    node: node.id,
                    port: 0,
                },
                palette,
            )),
            None,
        );
        return;
    }
    let size = node.size();
    let h = size.y;
    let inverted = matches!(node.kind, Kind::Not | Kind::Nand | Kind::Nor | Kind::Xnor);
    let w = size.x - if inverted { 12.0 } else { 0.0 };
    let p = Point::new;
    let body = match node.kind {
        Kind::And | Kind::Nand => PathData(vec![
            Cmd::Move(p(0.0, 0.0)),
            Cmd::Line(p(w * 0.5, 0.0)),
            Cmd::Cubic(p(w * 0.776, 0.0), p(w, h * 0.224), p(w, h * 0.5)),
            Cmd::Cubic(p(w, h * 0.776), p(w * 0.776, h), p(w * 0.5, h)),
            Cmd::Line(p(0.0, h)),
            Cmd::Close,
        ]),
        Kind::Or | Kind::Nor | Kind::Xor | Kind::Xnor => PathData(vec![
            Cmd::Move(p(0.0, 0.0)),
            Cmd::Cubic(p(w * 0.45, 0.0), p(w * 0.8, h * 0.08), p(w, h * 0.5)),
            Cmd::Cubic(p(w * 0.8, h * 0.92), p(w * 0.45, h), p(0.0, h)),
            Cmd::Quad(p(28.0, h * 0.5), p(0.0, 0.0)),
            Cmd::Close,
        ]),
        Kind::Not | Kind::Buffer => PathData(vec![
            Cmd::Move(p(0.0, 0.0)),
            Cmd::Line(p(w, h / 2.0)),
            Cmd::Line(p(0.0, h)),
            Cmd::Close,
        ]),
        _ => PathData::rect(Point::default(), size),
    };
    let fill = palette.gate_fill.0;
    let ink = palette.gate_outline.0;
    scene.path(body.translated(node.position), Some(fill), Some((ink, 2.0)));
    if matches!(node.kind, Kind::Xor | Kind::Xnor) {
        scene.path(
            PathData(vec![
                Cmd::Move(p(-7.0, 0.0)),
                Cmd::Quad(p(21.0, h / 2.0), p(-7.0, h)),
            ])
            .translated(node.position),
            None,
            Some((ink, 2.0)),
        );
    }
    if inverted {
        scene.path(
            PathData::circle(
                p(node.position.x + size.x - 6.0, node.position.y + h / 2.0),
                6.0,
            ),
            Some(fill),
            Some((ink, 2.0)),
        );
    }

    for output in [false, true] {
        for port in 0..if output { node.outputs } else { node.inputs } {
            let endpoint = Endpoint {
                node: node.id,
                port,
            };
            let pin = node.pin(output, port);
            let mut edge = Point::new(node.position.x + if output { size.x } else { 0.0 }, pin.y);
            if !output && matches!(node.kind, Kind::Or | Kind::Nor | Kind::Xor | Kind::Xnor) {
                let t = (pin.y - node.position.y) / h;
                edge.x += 56.0 * t * (1.0 - t);
            }
            let source = if output {
                Some(endpoint)
            } else {
                circuit
                    .wires
                    .iter()
                    .find(|wire| wire.to == endpoint)
                    .map(|wire| wire.from)
            };
            let color = source
                .map(|source| connection_color(circuit, source, palette))
                .unwrap_or(palette.gate_outline.0);
            if !output && node.bubble(false, port) {
                if matches!(node.kind, Kind::Xor | Kind::Xnor) {
                    edge.x -= 7.0;
                }
                scene.path(
                    PathData::circle(p(edge.x - 6.0, edge.y), 6.0),
                    Some(fill),
                    Some((ink, 2.0)),
                );
                edge.x -= 12.0;
            }
            scene.path(PathData::polyline(&[edge, pin]), None, Some((color, 2.0)));
            scene.path(PathData::circle(pin, 3.5), Some(fill), Some((color, 1.8)));
            if node.kind.module() {
                let name = node.port_name(output, port).replace(['_', '{', '}'], "");
                let half = styled_text_width(&name, 12.0, TextStyle::Ui) / 2.0;
                scene.styled_text(
                    Point::new(
                        edge.x + if output { -half - 8.0 } else { half + 8.0 },
                        edge.y,
                    ),
                    name,
                    12.0,
                    ink,
                    TextStyle::Ui,
                );
            }
        }
    }
    scene.styled_text(
        p(node.position.x + size.x / 2.0, node.position.y - 17.0),
        &node.label,
        16.0,
        palette.label.0,
        TextStyle::Ui,
    );
    let center = p(node.position.x + size.x / 2.0, node.position.y + h / 2.0);
    let caption = match node.kind {
        Kind::Input | Kind::Output => None,
        Kind::High => Some("1"),
        Kind::Low => Some("0"),
        Kind::Mux => Some("MUX"),
        Kind::HalfAdder => Some("HA"),
        Kind::FullAdder => Some("FA"),
        Kind::Module => Some("f"),
        Kind::Register => Some("DFF"),
        _ => None,
    };
    if let Some(caption) = caption {
        scene.styled_text(
            center,
            caption,
            if node.kind.module() { 16.0 } else { 19.0 },
            ink,
            TextStyle::Ui,
        );
    }
}

/// Neutral theme ink is used unless the diagram explicitly assigns a net color.
fn connection_color(circuit: &Circuit, endpoint: Endpoint, palette: &CanvasPalette) -> Color {
    let source = circuit.net_source(endpoint);
    let color = circuit
        .node(source.node)
        .map(|node| node.net_color(source.port))
        .unwrap_or_default();
    if color == NetColor::Black {
        palette.gate_outline.0
    } else {
        net_color(color)
    }
}
