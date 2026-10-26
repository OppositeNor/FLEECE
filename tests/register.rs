use fleece::{history::History, model::*};

fn pin(node: u64, port: usize) -> Endpoint {
    Endpoint { node, port }
}

fn fixture() -> (Circuit, u64, u64, u64) {
    let mut circuit = Circuit::empty();
    let data = circuit.add(Kind::Input, Point::new(40.0, 40.0));
    let clock = circuit.add(Kind::Input, Point::new(40.0, 200.0));
    let register = circuit.add(Kind::Register, Point::new(300.0, 40.0));
    circuit.resize_register(register, 2).unwrap();
    circuit.connect(pin(data, 0), pin(register, 0)).unwrap();
    circuit.connect(pin(clock, 0), pin(register, 2)).unwrap();
    (circuit, data, clock, register)
}

#[test]
fn resizing_preserves_clock_and_reset_wiring_and_prunes_removed_data_bits() {
    let (mut circuit, _, clock, register) = fixture();
    let reset = circuit.add(Kind::Input, Point::new(40.0, 300.0));
    circuit.connect(pin(reset, 0), pin(register, 3)).unwrap();
    circuit.resize_register(register, 4).unwrap();
    assert!(
        circuit
            .wires
            .iter()
            .any(|w| w.from == pin(clock, 0) && w.to == pin(register, 4))
    );
    assert!(
        circuit
            .wires
            .iter()
            .any(|w| w.from == pin(reset, 0) && w.to == pin(register, 5))
    );
    circuit.connect(pin(clock, 0), pin(register, 3)).unwrap();
    circuit.resize_register(register, 1).unwrap();
    assert!(
        circuit
            .wires
            .iter()
            .any(|w| w.from == pin(clock, 0) && w.to == pin(register, 1))
    );
    assert!(
        circuit
            .wires
            .iter()
            .any(|w| w.from == pin(reset, 0) && w.to == pin(register, 2))
    );
    assert!(
        !circuit
            .wires
            .iter()
            .any(|w| w.to.node == register && w.to.port == 3)
    );
    Document::new(circuit).validate().unwrap();
}

#[test]
fn register_width_colors_and_wires_roundtrip_and_undo() {
    let (mut circuit, _, _, register) = fixture();
    let before = circuit.clone();
    circuit.resize_register(register, 4).unwrap();
    circuit
        .node_mut(register)
        .unwrap()
        .set_net_color(0, NetColor::Red);
    let mut history = History::default();
    history.record(before.clone(), &circuit);
    let document = Document::new(circuit.clone());
    assert_eq!(
        Document::from_toml(&document.to_toml().unwrap()).unwrap(),
        document
    );
    let after = circuit.clone();
    assert!(history.undo(&mut circuit));
    assert_eq!(circuit, before);
    assert!(history.redo(&mut circuit));
    assert_eq!(circuit, after);
}

#[test]
fn register_feedback_and_module_layout_remain_diagram_properties() {
    let mut circuit = Circuit::empty();
    let register = circuit.add(Kind::Register, Point::default());
    let gate = circuit.add(Kind::Not, Point::new(300.0, 0.0));
    let output = circuit.add(Kind::Output, Point::new(600.0, 0.0));
    circuit.connect(pin(register, 0), pin(gate, 0)).unwrap();
    circuit.connect(pin(gate, 0), pin(register, 0)).unwrap();
    circuit.connect(pin(register, 0), pin(output, 0)).unwrap();
    circuit.auto_layout();
    assert!(circuit.node(gate).unwrap().position.x < circuit.node(register).unwrap().position.x);
    assert!(circuit.node(register).unwrap().position.x < circuit.node(output).unwrap().position.x);
    Document::new(circuit).validate().unwrap();
}
