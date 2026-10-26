mod support;

use std::collections::BTreeSet;

use fleece::{
    drawing::{self, Primitive},
    editor_drawing::build_editor_scene,
    export::{self, ExportOptions},
    model::*,
    theme::Theme,
};

fn pin(node: u64) -> Endpoint {
    Endpoint { node, port: 0 }
}

#[test]
fn shared_joints_preserve_connectivity_and_colors_and_survive_copy_save_and_export() {
    let mut doc = Document::new(Circuit::empty());
    doc.circuit.add(Kind::Input, Point::new(100.0, 200.0));
    doc.circuit.add(Kind::Joint, Point::new(300.0, 220.0));
    doc.circuit.node_mut(2).unwrap().label = "Should not be drawn".into();
    doc.circuit.connect(pin(1), pin(2)).unwrap();
    doc.circuit
        .node_mut(1)
        .unwrap()
        .set_net_color(0, NetColor::Red);
    for i in 0..5 {
        let output = doc
            .circuit
            .add(Kind::Output, Point::new(600.0, 100.0 + i as f32 * 100.0));
        doc.circuit.connect(pin(2), pin(output)).unwrap();
    }
    doc.validate().unwrap();
    let c = &doc.circuit;
    for target in 3..=7 {
        assert_eq!(support::source_kind(c, pin(target), false), Kind::Input);
    }
    assert_eq!(c.net_source(pin(2)), pin(1));
    assert_eq!(
        drawing::endpoint_color(c, pin(2)),
        drawing::endpoint_color(c, pin(1))
    );
    let restored = Document::from_toml(&doc.to_toml().unwrap()).unwrap();
    assert_eq!(restored.circuit, doc.circuit);
    let mut recolored = restored.clone();
    let source = recolored.circuit.net_source(pin(2));
    recolored
        .circuit
        .node_mut(source.node)
        .unwrap()
        .set_net_color(source.port, NetColor::Blue);
    assert_eq!(
        recolored.circuit.node(1).unwrap().net_color(0),
        NetColor::Blue
    );
    assert_eq!(
        drawing::endpoint_color(&recolored.circuit, pin(2)),
        drawing::endpoint_color(&recolored.circuit, pin(1))
    );
    let fragment = c.selection(&(1..=7).collect());
    let mut pasted = Circuit::empty();
    pasted
        .paste_fragment(&fragment, Point::new(40.0, 40.0))
        .unwrap();
    assert_eq!(pasted.node(2).unwrap().position, Point::new(340.0, 260.0));
    for target in 3..=7 {
        assert_eq!(
            support::source_kind(&pasted, pin(target), false),
            Kind::Input
        );
    }
    for scene in [
        drawing::build_scene(c),
        build_editor_scene(c, &Theme::original(true).canvas),
    ] {
        assert!(!scene.items.iter().any(
            |item| matches!(item, Primitive::Text { text, .. } if text == "Should not be drawn")
        ));
    }
    let svg = export::svg(c, ExportOptions::default()).unwrap();
    assert!(!svg.contains("Should not be drawn"));
    assert!(!export::png(c, ExportOptions::default()).unwrap().is_empty());
    let mut unknown = c.clone();
    unknown.delete(&BTreeSet::from([1]));
    for target in 3..=7 {
        assert_eq!(
            support::source_kind(&unknown, pin(target), false),
            Kind::Joint
        );
    }
    // Removing the shared joint splices every branch back to the original driver.
    let mut removed = c.clone();
    removed.delete(&BTreeSet::from([2]));
    assert_eq!(removed.wires.len(), 5);
    assert!(removed.wires.iter().all(|w| w.from == pin(1)));
    Document::new(removed).validate().unwrap();
}

#[test]
fn splitting_is_atomic_and_removing_a_chain_of_joints_preserves_fanout() {
    let mut c = Circuit::empty();
    c.add(Kind::Input, Point::new(100.0, 200.0));
    c.add(Kind::Output, Point::new(600.0, 200.0));
    c.add(Kind::Output, Point::new(600.0, 400.0));
    c.connect(pin(1), pin(2)).unwrap();
    let original = c.clone();
    assert!(
        c.insert_joint(0, Point::new(f32::NAN, 0.0), vec![], vec![])
            .is_err()
    );
    assert_eq!(c, original);
    assert!(
        c.insert_joint(0, Point::default(), vec![Point::default(); 65], vec![])
            .is_err()
    );
    assert_eq!(c, original);
    let downstream = c
        .insert_joint(0, Point::new(400.0, 220.0), vec![], vec![])
        .unwrap();
    // Insert upstream afterward so ID order differs from connectivity order.
    let upstream = c
        .insert_joint(0, Point::new(300.0, 220.0), vec![], vec![])
        .unwrap();
    c.connect(pin(downstream), pin(3)).unwrap();
    c.delete(&BTreeSet::from([upstream, downstream]));
    assert_eq!(c.nodes.len(), 3);
    assert_eq!(c.wires.len(), 2);
    assert!(c.wires.iter().all(|w| w.from == pin(1)));
    Document::new(c).validate().unwrap();
}

#[test]
fn removing_a_joint_in_feedback_never_leaves_an_invalid_self_connection() {
    let mut c = Circuit::empty();
    let gate = c.add(Kind::Not, Point::default());
    let joint = c.add(Kind::Joint, Point::new(200.0, 100.0));
    c.connect(pin(gate), pin(joint)).unwrap();
    c.connect(pin(joint), pin(gate)).unwrap();
    Document::new(c.clone()).validate().unwrap();
    c.delete(&BTreeSet::from([joint]));
    Document::new(c).validate().unwrap();
}

#[test]
fn joint_markers_follow_actual_incident_wire_counts_in_editor_and_exports() {
    let point = Point::new(300.0, 220.0);
    let mut c = Circuit::empty();
    c.add(Kind::Input, Point::new(100.0, 200.0));
    let joint = c.suspend_wire(pin(1), true, point).unwrap();
    let first = c.add(Kind::Output, Point::new(600.0, 200.0));
    let second = c.add(Kind::Output, Point::new(600.0, 400.0));
    for degree in 1..=3 {
        if degree == 2 {
            c.connect(pin(joint), pin(first)).unwrap();
        }
        if degree == 3 {
            c.connect(pin(joint), pin(second)).unwrap();
        }
        assert_eq!(c.joint_degree(joint), degree);
        for (editing, scene) in [
            (false, drawing::build_scene(&c)),
            (true, build_editor_scene(&c, &Theme::original(true).canvas)),
        ] {
            // Only marker paths lie wholly within six units of the joint.
            let markers: Vec<_> = scene
                .items
                .iter()
                .filter_map(|item| match item {
                    Primitive::Path { path, fill, .. } => {
                        let points = path.flattened();
                        (!points.is_empty()
                            && points.iter().all(|p| {
                                (p.x - point.x).abs() <= 6.0 && (p.y - point.y).abs() <= 6.0
                            }))
                        .then_some(fill)
                    }
                    _ => None,
                })
                .collect();
            if degree == 2 && !editing {
                assert!(markers.is_empty());
            } else {
                assert!(!markers.is_empty());
                assert!(markers.iter().all(|fill| fill.is_some()));
            }
        }
    }
    c.wires.retain(|w| w.to.node != second);
    assert_eq!(c.joint_degree(joint), 2);
    c.wires.retain(|w| w.to.node != first);
    assert_eq!(c.joint_degree(joint), 1);
    let restored = Document::from_toml(&Document::new(c.clone()).to_toml().unwrap()).unwrap();
    assert_eq!(restored.circuit.joint_degree(joint), 1);
    let mut invalid = c.clone();
    assert!(
        invalid
            .suspend_wire(pin(999), true, Point::default())
            .is_err()
    );
    assert_eq!(invalid, c);
    assert!(
        invalid
            .suspend_wire(pin(1), true, Point::new(f32::INFINITY, 0.0))
            .is_err()
    );
    assert_eq!(invalid, c);
}

#[test]
fn routing_joints_and_automatic_elbows_are_always_dots_only_in_the_editor() {
    let mut c = Circuit::empty();
    c.add(Kind::Input, Point::new(100.0, 200.0));
    c.add(Kind::Output, Point::new(600.0, 400.0));
    c.connect(pin(1), pin(2)).unwrap();
    for manual in [false, true] {
        if manual {
            c.wires[0].waypoints = vec![Point::new(320.0, 220.0), Point::new(320.0, 420.0)];
        }
        let before = c.clone();
        let editor = build_editor_scene(&c, &Theme::original(true).canvas);
        let exported = drawing::build_scene(&c);
        for point in drawing::wire_joints(&c, &c.wires[0]) {
            let dots_at = |scene: &drawing::Scene| {
                scene
                    .items
                    .iter()
                    .filter(|item| match item {
                        Primitive::Path {
                            path,
                            fill: Some(_),
                            stroke: None,
                        } => {
                            let points = path.flattened();
                            !points.is_empty()
                                && points.iter().all(|p| {
                                    (p.x - point.x).abs() <= 4.0 && (p.y - point.y).abs() <= 4.0
                                })
                        }
                        _ => false,
                    })
                    .count()
            };
            assert!(dots_at(&editor) > 0);
            assert_eq!(dots_at(&exported), 0);
        }
        assert_eq!(
            c, before,
            "Drawing automatic joints must not materialize routing edits"
        );
    }
}

#[test]
fn removing_a_collinear_joint_removes_its_dot_and_preserves_the_next_elbow() {
    let mut c = Circuit::empty();
    let input = c.add(Kind::Input, Point::new(100.0, 200.0));
    let output = c.add(Kind::Output, Point::new(700.0, 400.0));
    let start = c.node(input).unwrap().pin(true, 0);
    let left = Point::new(start.x + 80.0, start.y);
    let selected = Point::new(start.x + 200.0, start.y);
    let joint = c.suspend_wire(pin(input), true, selected).unwrap();
    // Endpoint controls can coincide with a node after a route has been edited.
    c.wires[0].waypoints = vec![left, selected];
    c.connect(pin(joint), pin(output)).unwrap();
    let right = drawing::wire_joints(&c, &c.wires[1])[0];
    let before = c.clone();
    let has_dot = |c: &Circuit, point: Point| {
        let scene = build_editor_scene(c, &Theme::original(true).canvas);
        scene.items.iter().any(|item| match item {
            Primitive::Path {
                path,
                fill: Some(_),
                stroke: None,
            } => {
                let points = path.flattened();
                !points.is_empty()
                    && points
                        .iter()
                        .all(|p| (p.x - point.x).abs() < 4.0 && (p.y - point.y).abs() < 4.0)
            }
            _ => false,
        })
    };
    assert!(has_dot(&c, selected));
    assert!(has_dot(&c, right));
    c.delete(&BTreeSet::from([joint]));
    assert!(c.node(joint).is_none());
    assert!(
        !has_dot(&c, selected),
        "The selected joint must not survive as a duplicate waypoint"
    );
    assert!(has_dot(&c, left));
    assert!(
        has_dot(&c, right),
        "Deleting the middle dot must keep the right elbow visible"
    );
    assert_eq!(c.wires.len(), 1);
    assert_eq!(c.wires[0].from, pin(input));
    assert_eq!(c.wires[0].to, pin(output));
    assert_eq!(support::source_kind(&c, pin(output), false), Kind::Input);
    let mut original_route = drawing::wire_points(&before, &before.wires[0]);
    original_route.extend(
        drawing::wire_points(&before, &before.wires[1])
            .into_iter()
            .skip(1),
    );
    original_route.retain(|p| *p != selected);
    original_route.dedup();
    assert_eq!(drawing::wire_points(&c, &c.wires[0]), original_route);
    Document::new(c).validate().unwrap();
}

#[test]
fn removing_a_collinear_route_control_preserves_other_dots_and_rejects_stale_indices() {
    let mut c = Circuit::empty();
    c.add(Kind::Input, Point::new(100.0, 200.0));
    c.add(Kind::Output, Point::new(700.0, 400.0));
    c.connect(pin(1), pin(2)).unwrap();
    c.wires[0].horizontal_first = false;
    let start = c.node(1).unwrap().pin(true, 0);
    let selected = Point::new(start.x + 100.0, start.y);
    c.wires[0].waypoints = vec![selected];
    let original_route = drawing::wire_points(&c, &c.wires[0]);
    let right = original_route[original_route.len() - 2];
    c.remove_wire_controls(0, &BTreeSet::from([0])).unwrap();
    assert!(!drawing::wire_joints(&c, &c.wires[0]).contains(&selected));
    assert!(drawing::wire_joints(&c, &c.wires[0]).contains(&right));
    let before = c.clone();
    assert!(c.remove_wire_controls(0, &BTreeSet::from([99])).is_err());
    assert_eq!(c, before);
    Document::new(c).validate().unwrap();
}
