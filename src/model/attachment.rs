//! Exact pin-to-route attachment after placement or movement. Geometry uses the
//! interactive pins, independent of zoom and export proportions. Explicit joints
//! win over crossing wires; ambiguous driven nets are never merged implicitly.
use super::*;

const EPSILON: f32 = 0.01;

fn same_point(a: Point, b: Point) -> bool {
    (a.x - b.x).abs() <= EPSILON && (a.y - b.y).abs() <= EPSILON
}

fn on_segment(point: Point, a: Point, b: Point) -> bool {
    let horizontal = (a.y - b.y).abs() <= EPSILON && (point.y - a.y).abs() <= EPSILON;
    let vertical = (a.x - b.x).abs() <= EPSILON && (point.x - a.x).abs() <= EPSILON;
    (horizontal || vertical)
        && point.x >= a.x.min(b.x) - EPSILON
        && point.x <= a.x.max(b.x) + EPSILON
        && point.y >= a.y.min(b.y) - EPSILON
        && point.y <= a.y.max(b.y) + EPSILON
        && !same_point(a, b)
}

enum Target {
    Joint(u64),
    Wire {
        index: usize,
        segment: usize,
        route: Vec<Point>,
    },
}

impl Circuit {
    /// Attach unconnected input pins and compatible output pins of the supplied
    /// components to coincident wires/joints, using interactive geometry.
    ///
    /// Existing input wiring is retained. Output pins can drive suspended joint
    /// networks, but never replace another output's driver. Wires incident on
    /// the supplied components are excluded. Unrelated nets at a crossing are
    /// ambiguous and skipped unless an explicit joint identifies the target.
    /// Routes, fan-out and bus annotations survive splitting and rerooting.
    /// FSM documents and joint-only selections are unchanged.
    ///
    /// Returns the number of newly attached component pins. Repeated calls are
    /// idempotent. Callers include this mutation in the placement/move snapshot.
    ///
    /// # Errors
    /// Returns validation/topology-limit errors without changing the circuit.
    pub fn connect_overlapping_pins(&mut self, ids: &BTreeSet<u64>) -> Result<usize, String> {
        if self.fsm.is_some() {
            return Ok(0);
        }
        let pins: Vec<_> = self
            .nodes
            .iter()
            .filter(|node| ids.contains(&node.id) && node.kind != Kind::Joint)
            .flat_map(|node| {
                [false, true].into_iter().flat_map(move |output| {
                    (0..if output { node.outputs } else { node.inputs }).map(move |port| {
                        (
                            Endpoint {
                                node: node.id,
                                port,
                            },
                            output,
                            node.pin(output, port),
                        )
                    })
                })
            })
            .collect();
        if pins.is_empty() {
            return Ok(0);
        }
        let mut working = self.clone();
        let mut count = 0;
        for (pin, output, point) in pins {
            if !output && working.wires.iter().any(|wire| wire.to == pin) {
                continue;
            }
            let Some(target) = working.attachment_target(point, ids) else {
                continue;
            };
            let source = match &target {
                Target::Joint(id) => working.net_source(Endpoint { node: *id, port: 0 }),
                Target::Wire { index, .. } => working.net_source(working.wires[*index].from),
            };
            if output
                && (!working
                    .node(source.node)
                    .is_some_and(|node| node.kind == Kind::Joint)
                    || working.wires.iter().any(|wire| wire.to.node == source.node))
            {
                continue;
            }
            let (joint, bus_width) = match target {
                Target::Joint(id) => (
                    id,
                    working
                        .wires
                        .iter()
                        .find(|wire| wire.from.node == id || wire.to.node == id)
                        .and_then(|wire| wire.bus_width),
                ),
                Target::Wire {
                    index,
                    segment,
                    route,
                } => {
                    let edge = working.wires[index].clone();
                    // A wire's normal output endpoint can feed another input
                    // directly, without creating a coincident routing joint.
                    if !output && same_point(point, route[0]) {
                        working.connect(edge.from, pin)?;
                        working.wires.last_mut().unwrap().bus_width = edge.bus_width;
                        count += 1;
                        continue;
                    }
                    let prefix = route[1..segment + 1]
                        .iter()
                        .copied()
                        .filter(|p| !same_point(*p, point))
                        .collect();
                    let suffix = route[segment + 1..route.len() - 1]
                        .iter()
                        .copied()
                        .filter(|p| !same_point(*p, point))
                        .collect();
                    (
                        working.insert_joint(index, point, prefix, suffix)?,
                        edge.bus_width,
                    )
                }
            };
            let joint_pin = Endpoint {
                node: joint,
                port: 0,
            };
            if output {
                working.reroot_suspended_joint(joint);
                working.connect(pin, joint_pin)?;
            } else {
                working.connect(joint_pin, pin)?;
            }
            working.wires.last_mut().unwrap().bus_width = bus_width;
            count += 1;
        }
        if count > 0 {
            Document::new(working.clone()).validate()?;
            *self = working;
        }
        Ok(count)
    }

    fn attachment_target(&self, point: Point, moving: &BTreeSet<u64>) -> Option<Target> {
        let joints: Vec<_> = self
            .nodes
            .iter()
            .filter(|node| {
                node.kind == Kind::Joint
                    && !moving.contains(&node.id)
                    && same_point(node.position, point)
            })
            .collect();
        if !joints.is_empty() {
            let sources: BTreeSet<_> = joints
                .iter()
                .map(|node| {
                    self.net_source(Endpoint {
                        node: node.id,
                        port: 0,
                    })
                })
                .collect();
            return (sources.len() == 1).then(|| Target::Joint(joints[0].id));
        }
        let candidates: Vec<_> = self
            .wires
            .iter()
            .enumerate()
            .filter(|(_, wire)| {
                !moving.contains(&wire.from.node) && !moving.contains(&wire.to.node)
            })
            .filter_map(|(index, wire)| {
                let route = self.wire_path(wire);
                let segment = route
                    .windows(2)
                    .position(|pair| on_segment(point, pair[0], pair[1]))?;
                Some((index, segment, route))
            })
            .collect();
        let sources: BTreeSet<_> = candidates
            .iter()
            .map(|(index, _, _)| self.net_source(self.wires[*index].from))
            .collect();
        if sources.len() != 1 {
            return None;
        }
        candidates
            .into_iter()
            .next()
            .map(|(index, segment, route)| Target::Wire {
                index,
                segment,
                route,
            })
    }

    fn reroot_suspended_joint(&mut self, joint: u64) {
        // Collect the original parent chain before reversing any edges, otherwise
        // the next lookup would follow an edge we just reversed back again.
        let mut chain = Vec::new();
        let mut current = joint;
        while let Some(index) = self.wires.iter().position(|wire| wire.to.node == current) {
            chain.push(index);
            current = self.wires[index].from.node;
        }
        for index in chain {
            let edge = self.wires[index].clone();
            let mut route = self.wire_path(&edge);
            route.reverse();
            let wire = &mut self.wires[index];
            wire.from = Endpoint {
                node: edge.to.node,
                port: 0,
            };
            wire.to = Endpoint {
                node: edge.from.node,
                port: 0,
            };
            wire.horizontal_first = !edge.horizontal_first;
            wire.bus_position = edge.bus_position.map(|position| 1.0 - position);
            wire.waypoints = if route.len() > 2 {
                route[1..route.len() - 1].to_vec()
            } else {
                Vec::new()
            };
        }
    }
}
