use fleece::model::*;
use std::collections::BTreeSet;

fn pin(node: u64) -> Endpoint {
    Endpoint { node, port: 0 }
}
fn attach(circuit: &mut Circuit, node: u64) -> Result<usize, String> {
    circuit.connect_overlapping_pins(&BTreeSet::from([node]))
}
fn line() -> Circuit {
    let mut circuit = Circuit::empty();
    let input = circuit.add(Kind::Input, Point::new(0.0, 0.0));
    let output = circuit.add(Kind::Output, Point::new(400.0, 0.0));
    circuit.connect(pin(input), pin(output)).unwrap();
    circuit.wires[0].bus_width = Some(8);
    circuit
}

#[test]
fn input_on_wire_splits_once_and_preserves_fanout_routes_and_bus_annotations() {
    let mut circuit = line();
    let output = circuit.add(Kind::Output, Point::new(220.0, 0.0));
    assert_eq!(attach(&mut circuit, output).unwrap(), 1);
    assert_eq!(circuit.nodes.len(), 4);
    assert_eq!(circuit.wires.len(), 3);
    assert!(circuit.wires.iter().all(|wire| wire.bus_width == Some(8)));
    let joint = circuit
        .nodes
        .iter()
        .find(|node| node.kind == Kind::Joint)
        .unwrap()
        .id;
    assert_eq!(
        circuit.node(joint).unwrap().position,
        Point::new(200.0, 20.0)
    );
    assert_eq!(
        circuit
            .wires
            .iter()
            .find(|wire| wire.to == pin(output))
            .unwrap()
            .from,
        pin(joint)
    );
    assert_eq!(circuit.net_source(pin(joint)), pin(1));
    let before = circuit.clone();
    assert_eq!(attach(&mut circuit, output).unwrap(), 0);
    assert_eq!(circuit, before);
    Document::new(circuit).validate().unwrap();
}

#[test]
fn an_existing_joint_is_reused_and_explicit_route_corners_stay_fixed() {
    let mut circuit = line();
    circuit.wires[0].waypoints = vec![
        Point::new(200.0, 20.0),
        Point::new(200.0, 100.0),
        Point::new(300.0, 100.0),
    ];
    let output = circuit.add(Kind::Output, Point::new(220.0, 40.0));
    attach(&mut circuit, output).unwrap();
    let joint = circuit
        .nodes
        .iter()
        .find(|node| node.kind == Kind::Joint)
        .unwrap()
        .id;
    for corner in [
        Point::new(200.0, 20.0),
        Point::new(200.0, 100.0),
        Point::new(300.0, 100.0),
    ] {
        assert!(
            circuit
                .wires
                .iter()
                .any(|wire| circuit.wire_path(wire).contains(&corner))
        );
    }
    let other = circuit.add(Kind::Output, Point::new(220.0, 40.0));
    let count = circuit.nodes.len();
    assert_eq!(attach(&mut circuit, other).unwrap(), 1);
    assert_eq!(circuit.nodes.len(), count);
    assert_eq!(
        circuit
            .wires
            .iter()
            .find(|wire| wire.to == pin(other))
            .unwrap()
            .from,
        pin(joint)
    );
    Document::new(circuit).validate().unwrap();
}

#[test]
fn outputs_can_drive_a_suspended_wire_or_an_interior_joint_without_losing_branches() {
    for split in [false, true] {
        let mut circuit = Circuit::empty();
        let root = circuit.add(Kind::Joint, Point::new(100.0, 20.0));
        let output = circuit.add(Kind::Output, Point::new(400.0, 0.0));
        circuit.connect(pin(root), pin(output)).unwrap();
        circuit.wires[0].bus_width = Some(16);
        let target = if split {
            None
        } else {
            Some(
                circuit
                    .insert_joint(0, Point::new(200.0, 20.0), vec![], vec![])
                    .unwrap(),
            )
        };
        let input = circuit.add(Kind::Input, Point::new(100.0, 0.0));
        assert_eq!(attach(&mut circuit, input).unwrap(), 1);
        let target = target.unwrap_or_else(|| {
            circuit
                .nodes
                .iter()
                .find(|node| node.kind == Kind::Joint && node.id != root)
                .unwrap()
                .id
        });
        assert_eq!(circuit.net_source(pin(root)), pin(input));
        assert_eq!(circuit.net_source(pin(target)), pin(input));
        assert_eq!(
            circuit
                .wires
                .iter()
                .find(|wire| wire.to == pin(root))
                .unwrap()
                .from,
            pin(target)
        );
        assert!(circuit.wires.iter().all(|wire| wire.bus_width == Some(16)));
        let before = circuit.clone();
        assert_eq!(attach(&mut circuit, input).unwrap(), 0);
        assert_eq!(circuit, before);
        Document::new(circuit).validate().unwrap();
    }
}

#[test]
fn driven_outputs_occupied_inputs_and_near_misses_do_not_change_existing_wires() {
    let mut circuit = line();
    let output = circuit.add(Kind::Output, Point::new(220.0, 1.0));
    let input = circuit.add(Kind::Input, Point::new(100.0, 0.0));
    let before = circuit.clone();
    assert_eq!(attach(&mut circuit, output).unwrap(), 0);
    assert_eq!(attach(&mut circuit, input).unwrap(), 0);
    assert_eq!(circuit, before);
    circuit.node_mut(output).unwrap().position.y = 0.0;
    circuit.connect(pin(input), pin(output)).unwrap();
    let before = circuit.clone();
    assert_eq!(attach(&mut circuit, output).unwrap(), 0);
    assert_eq!(circuit, before);
}

#[test]
fn an_unrelated_wire_crossing_is_ambiguous_and_is_not_connected() {
    let mut circuit = Circuit::empty();
    let a = circuit.add(Kind::Input, Point::new(0.0, 180.0));
    let b = circuit.add(Kind::Output, Point::new(600.0, 180.0));
    let c = circuit.add(Kind::Input, Point::new(200.0, 0.0));
    let d = circuit.add(Kind::Output, Point::new(400.0, 380.0));
    circuit.connect(pin(a), pin(b)).unwrap();
    circuit.connect(pin(c), pin(d)).unwrap();
    circuit.wires[1].horizontal_first = false;
    let target = circuit.add(Kind::Output, Point::new(320.0, 180.0));
    let before = circuit.clone();
    assert_eq!(attach(&mut circuit, target).unwrap(), 0);
    assert_eq!(circuit, before);
}

#[test]
fn topology_limits_fail_atomically() {
    let mut circuit = line();
    let output = circuit.add(Kind::Output, Point::new(220.0, 0.0));
    while circuit.nodes.len() < 1000 {
        circuit.add(Kind::Joint, Point::new(-100.0, -100.0));
    }
    let before = circuit.clone();
    assert!(attach(&mut circuit, output).is_err());
    assert_eq!(circuit, before);
}

#[test]
fn suspended_joints_at_the_same_position_can_be_driven_without_route_panics() {
    let mut circuit = Circuit::empty();
    let child = circuit.add(Kind::Joint, Point::new(200.0, 20.0));
    let root = circuit.add(Kind::Joint, Point::new(200.0, 20.0));
    let output = circuit.add(Kind::Output, Point::new(400.0, 0.0));
    circuit.connect(pin(root), pin(child)).unwrap();
    circuit.connect(pin(child), pin(output)).unwrap();
    let input = circuit.add(Kind::Input, Point::new(100.0, 0.0));
    assert_eq!(attach(&mut circuit, input).unwrap(), 1);
    assert_eq!(circuit.net_source(pin(root)), pin(input));
    Document::new(circuit).validate().unwrap();
}
