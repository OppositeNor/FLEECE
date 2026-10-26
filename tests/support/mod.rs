use fleece::model::{Circuit, Endpoint, Kind};

/// Resolve connectivity through joints without evaluating any logic.
pub fn source_kind(circuit: &Circuit, endpoint: Endpoint, output: bool) -> Kind {
    let source = if output {
        endpoint
    } else {
        circuit
            .wires
            .iter()
            .find(|wire| wire.to == endpoint)
            .unwrap()
            .from
    };
    circuit.node(circuit.net_source(source).node).unwrap().kind
}
