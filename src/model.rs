//! Persisted diagram data, connectivity and file boundaries.
//!
//! Logic diagrams use nodes and directed wires. A joint is a zero-size node whose
//! input and output occupy the same position; one incoming edge can feed multiple
//! outgoing branches. FSM documents instead hold an [`Fsm`] and empty node/wire
//! lists. [`Document::validate`] enforces that these representations are not mixed.
//!
//! Positions are world coordinates, independent of pan/zoom. Interactive pin
//! geometry lives on [`Node`]; export pin geometry lives in [`crate::drawing`].
//! Documents contain diagram structure and annotations only; loading never executes logic.

mod attachment;
mod bus;
mod components;
mod routing;
pub use components::{ComponentDefinition, ComponentGroup, Kind};
mod fsm;
pub use fsm::{Fsm, State, Transition};

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;

/// Current supported version of the persisted document envelope.
pub const FORMAT_VERSION: u32 = 1;
/// World-space grid spacing used for snapping and interactive pin alignment.
pub const GRID: f32 = 20.0;

/// Largest supported display-only bus-width annotation.
pub const MAX_BUS_WIDTH: usize = 1024;

/// A bounded world-coordinate position; pan and zoom are not stored here.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Point {
    /// Horizontal world coordinate.
    pub x: f32,
    /// Vertical world coordinate.
    pub y: f32,
}
impl Point {
    /// Construct a point without validating document coordinate bounds.
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
    /// Round both coordinates to the nearest [`GRID`] intersection.
    pub fn snapped(self) -> Self {
        Self::new(
            (self.x / GRID).round() * GRID,
            (self.y / GRID).round() * GRID,
        )
    }
    /// Check both finiteness and the document coordinate limit of ±100,000.
    pub fn finite(self) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && self.x.abs() <= 100_000.0
            && self.y.abs() <= 100_000.0
    }
}

/// Persisted explicit wire-color choice, independent of UI themes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetColor {
    /// Default document ink; the editor uses neutral theme outline ink.
    #[default]
    Black,
    /// Red diagram ink.
    Red,
    /// Green diagram ink.
    Green,
    /// Blue diagram ink.
    Blue,
    /// Purple diagram ink.
    Purple,
    /// Amber diagram ink.
    Amber,
}

impl NetColor {
    /// Available net colors in inspector/menu order.
    pub const ALL: [Self; 6] = [
        Self::Black,
        Self::Red,
        Self::Green,
        Self::Blue,
        Self::Purple,
        Self::Amber,
    ];
    /// Return the lowercase persisted/display name for this color.
    pub fn name(self) -> &'static str {
        match self {
            Self::Black => "Black",
            Self::Red => "Red",
            Self::Green => "Green",
            Self::Blue => "Blue",
            Self::Purple => "Purple",
            Self::Amber => "Amber",
        }
    }
}

/// A persisted component, terminal or joint. IDs identify connectivity independently
/// of editable labels; duplicate labels are valid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    /// Circuit-local positive identifier referenced by connected wires.
    pub id: u64,
    /// Component/symbol kind, including the special zero-size joint kind.
    pub kind: Kind,
    /// Editable display label; duplicates are allowed and do not affect connectivity.
    pub label: String,
    /// Top-left interactive body position, or the exact location for a joint.
    pub position: Point,
    /// Number of input ports, indexed from zero.
    pub inputs: usize,
    /// Number of output ports, indexed from zero.
    pub outputs: usize,
    /// Explicit output net colors in port order; omitted entries use the default ink.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub colors: Vec<NetColor>,
    /// Input inversion annotations in port order; omitted entries are not inverted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_bubbles: Vec<bool>,
}
impl Node {
    /// Construct an unattached component with its catalog name and default ports.
    ///
    /// Use [`Circuit::add`] for circuit-aware input/output numbering. Other kinds
    /// keep their plain names, and joint labels start empty.
    pub fn new(id: u64, kind: Kind, position: Point) -> Self {
        let definition = kind.definition();
        let (inputs, outputs) = (definition.inputs, definition.outputs);
        Self {
            id,
            kind,
            label: if kind == Kind::Joint {
                String::new()
            } else {
                kind.name().into()
            },
            position,
            inputs,
            outputs,
            colors: vec![],
            input_bubbles: vec![],
        }
    }
    /// Interactive body size in world units. Export proportions are intentionally
    /// defined separately by [`crate::drawing::diagram_size`].
    pub fn size(&self) -> Point {
        if self.kind == Kind::Joint {
            return Point::default();
        }
        if self.kind == Kind::Register {
            Point::new(180.0, 120.0 + self.outputs as f32 * GRID)
        } else if self.kind.module() {
            Point::new(
                180.0,
                80.0_f32.max((self.inputs.max(self.outputs) + 1) as f32 * GRID),
            )
        } else if matches!(
            self.kind,
            Kind::Input | Kind::Output | Kind::High | Kind::Low
        ) {
            Point::new(80.0, 40.0)
        } else {
            // An even number of grid rows also keeps the single output centered.
            let rows = (self.inputs + 1).max(4).next_multiple_of(2);
            Point::new(100.0, rows as f32 * GRID)
        }
    }
    /// Locate a zero-based interactive pin in world coordinates. Pins align to
    /// the grid when the node position does; joint pins coincide with the node.
    pub fn pin(&self, output: bool, port: usize) -> Point {
        if self.kind == Kind::Joint {
            return self.position;
        }
        let size = self.size();
        let count = if output { self.outputs } else { self.inputs };
        let y = if self.kind == Kind::Register {
            if !output && port == self.outputs {
                size.y - 60.0
            } else if !output && port == self.outputs + 1 {
                size.y - GRID
            } else {
                40.0 + port as f32 * GRID
            }
        } else {
            (size.y / GRID * (port + 1) as f32 / (count + 1) as f32).round() * GRID
        };
        Point::new(
            self.position.x + if output { size.x + GRID } else { -GRID },
            self.position.y + y,
        )
    }
    /// Return the symbol annotation for a pin, including D/Q, CLK/RST and mux selectors.
    pub fn port_name(&self, output: bool, port: usize) -> String {
        match (self.kind, output, port) {
            (Kind::Register, false, port) if port == self.outputs => "CLK".into(),
            (Kind::Register, false, port) if port == self.outputs + 1 => "RST".into(),
            (Kind::Register, output, port) => {
                let name = if output { "Q" } else { "D" };
                if self.outputs == 1 {
                    name.into()
                } else {
                    format!("{name}_{port}")
                }
            }
            (Kind::Mux, false, port) => {
                let bits = self.mux_select_bits();
                let data = 1 << bits;
                if port < data {
                    format!("D_{port}")
                } else if bits == 1 {
                    "S".into()
                } else {
                    format!("S_{}", port - data)
                }
            }
            (Kind::HalfAdder | Kind::FullAdder, true, 0) => "Sum".into(),
            (Kind::HalfAdder | Kind::FullAdder, true, 1) => "C_{out}".into(),
            (Kind::FullAdder, false, 2) => "C_{in}".into(),
            (_, false, n) => char::from(b'A' + n as u8).to_string(),
            (_, true, n) => format!("Y_{n}"),
        }
    }
    /// Mux input count encodes 2^bits data pins followed by bits selector pins.
    /// S_0 is the least significant selection bit.
    pub fn mux_select_bits(&self) -> usize {
        (1..=3)
            .find(|bits| self.inputs == (1 << bits) + bits)
            .unwrap_or(1)
    }
    /// Set mux data/selector port counts for a supported width of 1–3 selector bits.
    /// Use [`Circuit::resize_mux`] to validate the width and preserve attached wiring.
    pub fn resize_mux(&mut self, select_bits: usize) {
        self.resize_ports((1 << select_bits) + select_bits, 1);
    }
    /// Resize component ports, dropping colors and bubbles for removed pins.
    /// The owning circuit must prune unavailable wire endpoints afterward.
    pub fn resize_ports(&mut self, inputs: usize, outputs: usize) {
        self.inputs = inputs;
        self.outputs = outputs;
        self.colors.truncate(outputs);
        self.input_bubbles.truncate(inputs);
    }

    /// Resize register pins for a supported width of 1–8 bits.
    /// Use [`Circuit::resize_register`] to remap attached wiring.
    pub fn resize_register(&mut self, bits: usize) {
        self.inputs = bits + 2;
        self.outputs = bits;
        self.colors.truncate(bits);
    }

    /// Read an output color, using the default when no explicit entry exists.
    pub fn net_color(&self, port: usize) -> NetColor {
        self.colors.get(port).copied().unwrap_or_default()
    }

    /// Set an output color after expanding omitted entries to defaults.
    ///
    /// # Panics
    /// Panics when `port` is outside `0..self.outputs`; callers must validate the port.
    pub fn set_net_color(&mut self, port: usize, color: NetColor) {
        self.colors.resize(self.outputs, NetColor::Black);
        self.colors[port] = color;
    }

    /// Read the inversion annotation of a gate pin. Unspecified input bubbles are false;
    /// output inversion is represented by the gate kind.
    pub fn bubble(&self, output: bool, port: usize) -> bool {
        if output {
            port == 0 && matches!(self.kind, Kind::Not | Kind::Nand | Kind::Nor | Kind::Xnor)
        } else {
            self.input_bubbles.get(port).copied().unwrap_or(false)
        }
    }

    /// Change an inversion annotation on a supported gate pin. Output inversion
    /// switches the corresponding gate kind; invalid kinds/ports return an error.
    ///
    /// # Errors
    ///
    /// Returns an error when the component is not a supported gate or the requested
    /// pin is outside that component's port range.
    pub fn set_bubble(&mut self, output: bool, port: usize, value: bool) -> Result<(), String> {
        if !self.kind.gate() || port >= if output { self.outputs } else { self.inputs } {
            return Err("Bubbles require an available logic-gate pin.".into());
        }
        if output {
            self.kind = match self.kind {
                Kind::And | Kind::Nand => {
                    if value {
                        Kind::Nand
                    } else {
                        Kind::And
                    }
                }
                Kind::Or | Kind::Nor => {
                    if value {
                        Kind::Nor
                    } else {
                        Kind::Or
                    }
                }
                Kind::Xor | Kind::Xnor => {
                    if value {
                        Kind::Xnor
                    } else {
                        Kind::Xor
                    }
                }
                Kind::Buffer | Kind::Not => {
                    if value {
                        Kind::Not
                    } else {
                        Kind::Buffer
                    }
                }
                _ => unreachable!(),
            };
        } else {
            self.input_bubbles.resize(self.inputs, false);
            self.input_bubbles[port] = value;
            while self.input_bubbles.last() == Some(&false) {
                self.input_bubbles.pop();
            }
        }
        Ok(())
    }
}

/// A node ID and zero-based port index. The containing wire determines whether
/// this refers to an output (`from`) or an input (`to`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Endpoint {
    /// ID of the component containing the pin.
    pub node: u64,
    /// Zero-based index within the wire endpoint's input or output ports.
    pub port: usize,
}
/// A directed connection with optional explicit routing controls.
///
/// Without waypoints, `horizontal_first` chooses the single automatic elbow.
/// Waypoints override that automatic route; they are world coordinates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Wire {
    /// Driving output endpoint.
    pub from: Endpoint,
    /// Driven input endpoint; at most one wire may target an input.
    pub to: Endpoint,
    /// Explicit world-space routing controls; an empty list uses automatic routing.
    #[serde(default)]
    pub waypoints: Vec<Point>,
    /// Automatic route orientation: horizontal then vertical when true, otherwise
    /// vertical then horizontal. Explicit waypoints take precedence.
    #[serde(default = "default_horizontal_first")]
    pub horizontal_first: bool,
    /// Optional display-only bus width, rendered as a slash and a number.
    /// Valid widths are 1 through [`MAX_BUS_WIDTH`]; port counts are unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bus_width: Option<usize>,
    /// Optional distance along the route, normalized from source (0) to destination (1).
    /// Missing positions use the midpoint of the longest segment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bus_position: Option<f32>,
}
fn default_horizontal_first() -> bool {
    true
}

/// Persisted diagram content. Logic uses `nodes`/`wires`; an FSM uses `fsm`.
/// Editor view settings and native preferences belong outside this structure.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Circuit {
    /// Diagram title, up to 200 printable characters.
    pub title: String,
    /// Logic components in document order; empty for FSM documents.
    pub nodes: Vec<Node>,
    /// Logic connections in document order; empty for FSM documents.
    pub wires: Vec<Wire>,
    /// FSM state/transition data; absent for logic documents.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fsm: Option<Fsm>,
}
impl Circuit {
    /// Create empty logic-diagram content without editor settings.
    pub fn empty() -> Self {
        Self {
            title: "Untitled circuit".into(),
            ..Self::default()
        }
    }
    /// Create empty FSM content with no logic nodes or wires.
    pub fn new_fsm() -> Self {
        Self {
            title: "Untitled FSM".into(),
            fsm: Some(Fsm::default()),
            ..Self::default()
        }
    }
    /// Find a component by its persisted circuit-local ID.
    pub fn node(&self, id: u64) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }
    /// Mutably find a component by ID without changing its order or identity.
    pub fn node_mut(&mut self, id: u64) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }

    /// Preserve data and selector identities when the mux address width changes.
    ///
    /// # Errors
    ///
    /// Returns an error when the node is missing/not a mux or selector width is
    /// outside 1–3. Invalid requests are rejected before changing wiring.
    pub fn resize_mux(&mut self, id: u64, select_bits: usize) -> Result<(), String> {
        if !(1..=3).contains(&select_bits) {
            return Err("Multiplexers require 1–3 selection bits.".into());
        }
        let node = self
            .node_mut(id)
            .filter(|n| n.kind == Kind::Mux)
            .ok_or("Selection width changes require a multiplexer.")?;
        let old_bits = node.mux_select_bits();
        let old_data = 1 << old_bits;
        let new_data = 1 << select_bits;
        node.resize_mux(select_bits);
        self.wires.retain_mut(|wire| {
            if wire.to.node != id {
                return true;
            }
            if wire.to.port < old_data {
                wire.to.port < new_data
            } else {
                let bit = wire.to.port - old_data;
                wire.to.port = new_data + bit;
                bit < select_bits
            }
        });
        self.prune_wires();
        Ok(())
    }

    /// Keep both control inputs connected when their indices move after a width change.
    ///
    /// # Errors
    ///
    /// Returns an error when the node is missing/not a register or width is outside
    /// 1–8. Invalid requests are rejected before changing wiring.
    pub fn resize_register(&mut self, id: u64, bits: usize) -> Result<(), String> {
        let node = self
            .node_mut(id)
            .filter(|node| node.kind == Kind::Register)
            .ok_or("Width changes require a D-flip-flop.")?;
        if !(1..=8).contains(&bits) {
            return Err("D-flip-flop width must be 1–8 bits.".into());
        }
        let old_clock = node.outputs;
        let old_reset = node.outputs + 1;
        node.resize_register(bits);
        self.wires.retain_mut(|wire| {
            if wire.to.node != id {
                return true;
            }
            if wire.to.port == old_clock {
                wire.to.port = bits;
                true
            } else if wire.to.port == old_reset {
                wire.to.port = bits + 1;
                true
            } else {
                wire.to.port < bits
            }
        });
        self.prune_wires();
        Ok(())
    }
    /// Add a logic component and return its fresh circuit-local ID.
    ///
    /// Only inputs and outputs receive automatic numbers, counted independently.
    /// These labels are conveniences, not uniqueness guards: custom names and copied
    /// labels may repeat. Other component kinds keep their catalog names.
    pub fn add(&mut self, kind: Kind, position: Point) -> u64 {
        let id = self.nodes.iter().map(|n| n.id).max().unwrap_or(0) + 1;
        let mut node = Node::new(id, kind, position);
        if matches!(kind, Kind::Input | Kind::Output) {
            // These are placement conveniences, not uniqueness constraints.
            // Count each terminal kind separately; names may intentionally repeat
            // after deletion, manual renaming, duplication or clipboard paste.
            let number = self.nodes.iter().filter(|node| node.kind == kind).count() + 1;
            node.label = format!("{} {number}", kind.name());
        }
        self.nodes.push(node);
        id
    }
    /// Connect an output to an input, replacing that input's previous driver.
    /// Output fan-out is retained. Invalid ports and unsupported self-connections fail
    /// before the existing connection is removed.
    ///
    /// # Errors
    ///
    /// Returns an error for nonexistent/out-of-range endpoints or an unsupported
    /// self-connection. The existing driver is retained when validation fails.
    pub fn connect(&mut self, from: Endpoint, to: Endpoint) -> Result<(), String> {
        if !self.node(from.node).is_some_and(|n| from.port < n.outputs)
            || !self.node(to.node).is_some_and(|n| to.port < n.inputs)
        {
            return Err("That pin is unavailable.".into());
        }
        if from.node == to.node
            && !self
                .node(from.node)
                .is_some_and(|node| node.kind == Kind::Register && to.port < node.outputs)
        {
            return Err("Only a D-flip-flop Q output can feed its own D input.".into());
        }
        self.wires.retain(|w| w.to != to);
        self.wires.push(Wire {
            from,
            to,
            waypoints: vec![],
            horizontal_first: true,
            bus_width: None,
            bus_position: None,
        });
        Ok(())
    }
    /// Count edges incident on a node, used to decide joint markers in exports.
    pub fn joint_degree(&self, id: u64) -> usize {
        self.wires
            .iter()
            .filter(|w| w.from.node == id || w.to.node == id)
            .count()
    }

    /// Terminate a wire at a free joint, atomically and in either direction.
    ///
    /// # Errors
    ///
    /// Returns an error when the endpoint, position, connectivity or resulting
    /// document is invalid. The original circuit remains unchanged.
    pub fn suspend_wire(
        &mut self,
        endpoint: Endpoint,
        output: bool,
        position: Point,
    ) -> Result<u64, String> {
        let mut working = self.clone();
        let id = working.add(Kind::Joint, position);
        working.node_mut(id).unwrap().label.clear();
        let joint = Endpoint { node: id, port: 0 };
        let (from, to) = if output {
            (endpoint, joint)
        } else {
            (joint, endpoint)
        };
        working.connect(from, to)?;
        Document::new(working.clone()).validate()?;
        *self = working;
        Ok(id)
    }

    /// Split a routed wire into two edges sharing a persistent joint. Prefix and
    /// suffix contain only interior routing controls, never terminal positions.
    ///
    /// # Errors
    ///
    /// Returns an error for an unavailable wire or an invalid resulting document,
    /// including node/route limits. The original circuit remains unchanged.
    pub fn insert_joint(
        &mut self,
        wire: usize,
        position: Point,
        prefix: Vec<Point>,
        suffix: Vec<Point>,
    ) -> Result<u64, String> {
        let edge = self
            .wires
            .get(wire)
            .cloned()
            .ok_or("Wire is unavailable.")?;
        let mut working = self.clone();
        let id = working.add(Kind::Joint, position);
        working.node_mut(id).unwrap().label.clear();
        let pin = Endpoint { node: id, port: 0 };
        working.wires[wire] = Wire {
            from: edge.from,
            to: pin,
            waypoints: prefix,
            horizontal_first: edge.horizontal_first,
            bus_width: edge.bus_width,
            bus_position: None,
        };
        working.wires.push(Wire {
            from: pin,
            to: edge.to,
            waypoints: suffix,
            horizontal_first: edge.horizontal_first,
            bus_width: edge.bus_width,
            bus_position: None,
        });
        if edge.bus_position.is_some()
            && let Some((center, _)) = Wire::bus_anchor(&self.wire_path(&edge), edge.bus_position)
        {
            for index in [wire, working.wires.len() - 1] {
                let path = working.wire_path(&working.wires[index]);
                working.wires[index].bus_position =
                    Wire::project_bus_position(&path, center).map(|(_, fraction)| fraction);
            }
        }
        Document::new(working.clone()).validate()?;
        *self = working;
        Ok(id)
    }

    /// Resolve a branch back to its driving output, keeping net colors shared.
    pub fn net_source(&self, mut endpoint: Endpoint) -> Endpoint {
        let mut seen = BTreeSet::new();
        while self
            .node(endpoint.node)
            .is_some_and(|n| n.kind == Kind::Joint)
            && seen.insert(endpoint.node)
        {
            let Some(wire) = self.wires.iter().find(|w| w.to.node == endpoint.node) else {
                break;
            };
            endpoint = wire.from;
        }
        endpoint
    }

    /// Remove logic nodes and their incident edges. Joints are spliced out first
    /// so surviving branches retain connectivity and their other route corners.
    pub fn delete(&mut self, ids: &BTreeSet<u64>) {
        // Removing a joint removes the routing control while preserving branches.
        for id in ids {
            if !self.node(*id).is_some_and(|n| n.kind == Kind::Joint) {
                continue;
            }
            self.splice_joint(*id);
        }

        self.nodes.retain(|n| !ids.contains(&n.id));
        let register_data: BTreeMap<_, _> = self
            .nodes
            .iter()
            .filter(|n| n.kind == Kind::Register)
            .map(|n| (n.id, n.outputs))
            .collect();
        self.wires.retain(|w| {
            !ids.contains(&w.from.node)
                && !ids.contains(&w.to.node)
                && (w.from.node != w.to.node
                    || register_data
                        .get(&w.to.node)
                        .is_some_and(|bits| w.to.port < *bits))
        });
    }
    /// Remove edges attached to deleted nodes/ports after structural edits.
    pub fn prune_wires(&mut self) {
        let valid: BTreeSet<_> = self
            .wires
            .iter()
            .enumerate()
            .filter(|(_, w)| {
                self.node(w.from.node)
                    .is_some_and(|n| w.from.port < n.outputs)
                    && self.node(w.to.node).is_some_and(|n| w.to.port < n.inputs)
            })
            .map(|(i, _)| i)
            .collect();
        self.wires = self
            .wires
            .iter()
            .enumerate()
            .filter(|(i, _)| valid.contains(i))
            .map(|(_, w)| w.clone())
            .collect();
    }
    /// Duplicate selected nodes/states and their internal connections with remapped
    /// IDs and a fixed offset. Labels are preserved, including intentional duplicates.
    pub fn duplicate(&mut self, ids: &BTreeSet<u64>) -> BTreeSet<u64> {
        if let Some(fsm) = &mut self.fsm {
            return fsm
                .paste_fragment(&fsm.selection(ids), Point::new(40.0, 40.0))
                .unwrap_or_default();
        }
        let mut map = BTreeMap::new();
        for mut n in self
            .nodes
            .clone()
            .into_iter()
            .filter(|n| ids.contains(&n.id))
        {
            let id = self.nodes.iter().map(|n| n.id).max().unwrap_or(0) + 1;
            map.insert(n.id, id);
            n.id = id;
            n.position.x += 40.0;
            n.position.y += 40.0;
            self.nodes.push(n);
        }
        for mut w in self.wires.clone() {
            if let (Some(&from), Some(&to)) = (map.get(&w.from.node), map.get(&w.to.node)) {
                w.from.node = from;
                w.to.node = to;
                for p in &mut w.waypoints {
                    p.x += 40.0;
                    p.y += 40.0;
                }
                self.wires.push(w);
            }
        }
        map.values().copied().collect()
    }

    /// Copy blocks and only the wires whose two endpoints are included.
    pub fn selection(&self, ids: &BTreeSet<u64>) -> Self {
        if let Some(fsm) = &self.fsm {
            return Self {
                title: "Copied states".into(),
                fsm: Some(fsm.selection(ids)),
                ..Self::default()
            };
        }
        Self {
            title: "Copied blocks".into(),
            fsm: None,
            nodes: self
                .nodes
                .iter()
                .filter(|node| ids.contains(&node.id))
                .cloned()
                .collect(),
            wires: self
                .wires
                .iter()
                .filter(|wire| ids.contains(&wire.from.node) && ids.contains(&wire.to.node))
                .cloned()
                .collect(),
        }
    }

    /// Remap IDs and translate geometry; reject the entire paste if it is invalid.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid fragment, incompatible diagram type, exhausted
    /// IDs or resulting diagram limits. The original circuit remains unchanged.
    pub fn paste_fragment(
        &mut self,
        fragment: &Self,
        offset: Point,
    ) -> Result<BTreeSet<u64>, String> {
        Document::new(fragment.clone()).validate()?;
        match (&mut self.fsm, &fragment.fsm) {
            (Some(fsm), Some(fragment)) => return fsm.paste_fragment(fragment, offset),
            (None, None) => {}
            _ => return Err("Paste diagrams into a document of the same type.".into()),
        }
        if fragment.nodes.is_empty() {
            return Err("No blocks in the clipboard.".into());
        }
        if self.nodes.len() + fragment.nodes.len() > 1000
            || self.wires.len() + fragment.wires.len() > 8000
        {
            return Err("Paste exceeds the limit of 1,000 blocks or 8,000 wires.".into());
        }
        let mut working = self.clone();
        let mut next_id = self.nodes.iter().map(|node| node.id).max().unwrap_or(0);
        let mut ids = BTreeMap::new();
        for mut node in fragment.nodes.clone() {
            next_id = next_id.checked_add(1).ok_or("No available block IDs.")?;
            ids.insert(node.id, next_id);
            node.id = next_id;
            node.position.x += offset.x;
            node.position.y += offset.y;
            working.nodes.push(node);
        }
        for mut wire in fragment.wires.clone() {
            wire.from.node = ids[&wire.from.node];
            wire.to.node = ids[&wire.to.node];
            for point in &mut wire.waypoints {
                point.x += offset.x;
                point.y += offset.y;
            }
            working.wires.push(wire);
        }
        Document::new(working.clone()).validate()?;
        *self = working;
        Ok(ids.into_values().collect())
    }
    /// Arrange logic nodes by dependency columns and clear manual routes.
    /// Register feedback is excluded from column dependencies to keep feedback legible.
    pub fn auto_layout(&mut self) {
        // Q is previous-state data: omit its feedback edges when assigning
        // columns, keeping next-state gates to the left of their register.
        let dependencies: Vec<_> = self
            .wires
            .iter()
            .filter(|wire| {
                self.node(wire.from.node)
                    .is_none_or(|node| node.kind != Kind::Register)
                    || self
                        .node(wire.to.node)
                        .is_some_and(|node| node.kind == Kind::Output)
            })
            .map(|wire| (wire.from.node, wire.to.node))
            .collect();
        let mut indegree: BTreeMap<u64, usize> = self.nodes.iter().map(|n| (n.id, 0)).collect();
        let mut level: BTreeMap<u64, usize> = self.nodes.iter().map(|n| (n.id, 0)).collect();
        for &(_, to) in &dependencies {
            *indegree.get_mut(&to).unwrap() += 1;
        }
        let mut queue: VecDeque<_> = indegree
            .iter()
            .filter(|(_, d)| **d == 0)
            .map(|(id, _)| *id)
            .collect();
        while let Some(id) = queue.pop_front() {
            for &(_, to) in dependencies.iter().filter(|(from, _)| *from == id) {
                let l = level[&id] + 1;
                let dest = level.get_mut(&to).unwrap();
                *dest = (*dest).max(l);
                let d = indegree.get_mut(&to).unwrap();
                *d -= 1;
                if *d == 0 {
                    queue.push_back(to);
                }
            }
        }
        let mut y: BTreeMap<usize, f32> = BTreeMap::new();
        for node in &mut self.nodes {
            let column = level[&node.id];
            let row = y.entry(column).or_insert(40.0);
            node.position = Point::new(40.0 + column as f32 * 280.0, *row);
            *row += node.size().y + 80.0;
        }
        for wire in &mut self.wires {
            wire.waypoints.clear();
        }
    }
    /// Construct the bundled six-component half-adder reference diagram.
    pub fn half_adder() -> Self {
        let mut c = Self::empty();
        c.title = "Half adder".into();
        let a = c.add(Kind::Input, Point::new(60.0, 80.0));
        let b = c.add(Kind::Input, Point::new(60.0, 360.0));
        let xor = c.add(Kind::Xor, Point::new(360.0, 60.0));
        let and = c.add(Kind::And, Point::new(360.0, 260.0));
        let sum = c.add(Kind::Output, Point::new(680.0, 60.0));
        let carry = c.add(Kind::Output, Point::new(680.0, 260.0));
        for (id, label) in [
            (a, "A"),
            (b, "B"),
            (xor, "Sum logic"),
            (and, "Carry logic"),
            (sum, "Sum"),
            (carry, "Carry"),
        ] {
            c.node_mut(id).unwrap().label = label.into();
        }
        for (from, to, port) in [
            (a, xor, 0),
            (b, xor, 1),
            (a, and, 0),
            (b, and, 1),
            (xor, sum, 0),
            (and, carry, 0),
        ] {
            c.connect(
                Endpoint {
                    node: from,
                    port: 0,
                },
                Endpoint { node: to, port },
            )
            .unwrap();
        }
        // Separate the shared inputs' trunks so fan-out remains easy to read.
        for w in &mut c.wires {
            if w.from.node == a && w.to.node == and {
                w.waypoints = vec![Point::new(220.0, 100.0), Point::new(220.0, 280.0)];
            }
            if w.from.node == b && w.to.node == xor {
                w.waypoints = vec![Point::new(280.0, 380.0), Point::new(280.0, 120.0)];
            }
        }
        c
    }
}

/// Available editor panel arrangements.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Layout {
    /// Component library on the left and inspector on the right.
    #[default]
    Standard,
    /// Component library on the right and inspector on the left.
    LibraryRight,
    /// Canvas-focused layout with side panels hidden.
    Focus,
}
/// Saved editor presentation, excluded from diagram history and exports.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct View {
    /// Panel arrangement used to lay out the editor.
    pub layout: Layout,
    /// World-origin translation in screen points within the canvas.
    pub pan: Point,
    /// World-to-screen zoom factor, constrained to 0.15–4.0.
    pub zoom: f32,
    /// Whether grid points are visible on the interactive canvas.
    pub grid: bool,
    /// Whether placements and drag positions snap to the grid.
    pub snap: bool,
    /// Optional theme key or custom TOML path recorded with the document.
    /// A native application preference can override it when opening the editor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
}
impl Default for View {
    fn default() -> Self {
        Self {
            layout: Layout::Standard,
            pan: Point::new(40.0, 60.0),
            zoom: 1.0,
            grid: true,
            snap: true,
            theme: Some("original-dark".into()),
        }
    }
}
/// Versioned TOML envelope containing diagram content and optional editor settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    /// Persisted schema version; currently [`FORMAT_VERSION`].
    pub format_version: u32,
    /// Diagram content that participates in editing and snapshot history.
    pub circuit: Circuit,
    /// Saved viewport/theme settings, excluded from static diagram exports.
    #[serde(default)]
    pub editor: View,
}
impl Document {
    /// Wrap diagram content with the current format version and default view.
    /// This constructor does not validate the supplied circuit; call [`Self::validate`]
    /// at external/persistence boundaries.
    pub fn new(circuit: Circuit) -> Self {
        Self {
            format_version: FORMAT_VERSION,
            circuit,
            editor: View::default(),
        }
    }
    /// Validate the entire document without changing it.
    ///
    /// Checks include format version, diagram type, IDs, text/coordinate bounds,
    /// component ports, routing limits and one-driver-per-input connectivity.
    ///
    /// # Errors
    ///
    /// Returns a descriptive error for unsupported schema versions, inconsistent
    /// diagram types, invalid IDs/ports/coordinates/text, or diagram-size limits.
    pub fn validate(&self) -> Result<(), String> {
        let fail = |s: &str| Err(s.to_owned());
        if self.format_version != FORMAT_VERSION {
            return fail("Unsupported TOML document version.");
        }
        if self.editor.theme.as_ref().is_some_and(|theme| {
            theme.is_empty() || theme.len() > 4096 || theme.chars().any(char::is_control)
        }) {
            return fail("Invalid UI theme reference.");
        }
        if !self.editor.pan.finite()
            || !self.editor.zoom.is_finite()
            || !(0.15..=4.0).contains(&self.editor.zoom)
        {
            return fail("Invalid editor pan or zoom.");
        }
        if self.circuit.nodes.len() > 1000 || self.circuit.wires.len() > 8000 {
            return fail("Document exceeds the limit of 1,000 blocks or 8,000 wires.");
        }
        if self.circuit.title.chars().count() > 200
            || self.circuit.title.chars().any(char::is_control)
        {
            return fail("Circuit title must contain at most 200 printable characters.");
        }
        if let Some(fsm) = &self.circuit.fsm {
            if !self.circuit.nodes.is_empty() || !self.circuit.wires.is_empty() {
                return fail("FSM documents cannot also contain logic circuit blocks or wires.");
            }
            return fsm.validate();
        }
        let mut ids = BTreeSet::new();
        for n in &self.circuit.nodes {
            if n.id == 0 || n.id >= u64::MAX - 2000 || !ids.insert(n.id) {
                return fail("Block IDs must be unique, positive integers.");
            }
            if !n.position.finite()
                || n.label.chars().count() > 80
                || n.label.chars().any(char::is_control)
            {
                return fail("Invalid block position or label (maximum 80 printable characters).");
            }
            let reference = Node::new(1, n.kind, Point::default());
            if n.input_bubbles.len() > n.inputs || (!n.input_bubbles.is_empty() && !n.kind.gate()) {
                return fail("Input bubbles require available logic-gate input pins.");
            }
            if n.colors.len() > n.outputs {
                return fail("A block has colors for unavailable output pins.");
            }
            if n.kind == Kind::Register
                && (!(1..=8).contains(&n.outputs) || n.inputs != n.outputs + 2)
            {
                return fail(
                    "D-flip-flops require 1–8 bits, one D input per bit, followed by CLK and RST inputs.",
                );
            }
            if n.kind == Kind::Register {
                // Register port counts are determined by its bit width above.
            } else if n.kind == Kind::Mux {
                if ![3, 6, 11].contains(&n.inputs) {
                    return fail(
                        "Multiplexers require 1–3 selection bits (3, 6 or 11 total input pins).",
                    );
                }
            } else if n.kind == Kind::Xor {
                if n.inputs != 2 {
                    return fail("XOR gates require exactly two inputs.");
                }
            } else if n.kind.variable_inputs() {
                if !(2..=6).contains(&n.inputs) {
                    return fail("Gates and custom modules need 2–6 inputs.");
                }
            } else if n.inputs != reference.inputs {
                return fail("Invalid input count for this block type.");
            }
            if n.kind == Kind::Module {
                if !(1..=4).contains(&n.outputs) {
                    return fail("Custom modules require 1–4 outputs.");
                }
            } else if n.kind != Kind::Register && n.outputs != reference.outputs {
                return fail("Invalid output count for this block type.");
            }
        }
        let mut destinations = BTreeSet::new();
        for w in &self.circuit.wires {
            if !self
                .circuit
                .node(w.from.node)
                .is_some_and(|n| w.from.port < n.outputs)
                || !self
                    .circuit
                    .node(w.to.node)
                    .is_some_and(|n| w.to.port < n.inputs)
                || (w.from.node == w.to.node
                    && !self.circuit.node(w.from.node).is_some_and(|node| {
                        node.kind == Kind::Register && w.to.port < node.outputs
                    }))
            {
                return fail("Wire references an invalid block or pin.");
            }
            if !destinations.insert(w.to) {
                return fail("Each input pin may have only one driver.");
            }
            if w.bus_width
                .is_some_and(|width| !(1..=MAX_BUS_WIDTH).contains(&width))
            {
                return fail("Bus width must be between 1 and 1024.");
            }
            if w.bus_position
                .is_some_and(|position| !position.is_finite() || !(0.0..=1.0).contains(&position))
            {
                return fail("Bus position must be between 0 and 1.");
            }
            if w.waypoints.len() > 64 || w.waypoints.iter().any(|p| !p.finite()) {
                return fail("Invalid wire waypoints.");
            }
        }
        Ok(())
    }
    /// Parse and validate diagram TOML without modifying its contents.
    /// Unknown fields are rejected instead of silently dropping misspelled data.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed/unknown TOML fields or invalid document data.
    /// Unknown fields and unsupported component layouts are rejected.
    pub fn from_toml(text: &str) -> Result<Self, String> {
        let doc: Self = toml::from_str(text).map_err(|e| format!("Invalid TOML: {e}"))?;
        doc.validate()?;
        Ok(doc)
    }
    /// Validate before serializing so invalid in-memory edits cannot be saved.
    ///
    /// # Errors
    ///
    /// Returns validation or TOML serialization errors without changing the document.
    pub fn to_toml(&self) -> Result<String, String> {
        self.validate()?;
        toml::to_string_pretty(self).map_err(|e| e.to_string())
    }
    /// Read at most 8 MiB, require UTF-8, then parse and validate the document.
    ///
    /// # Errors
    ///
    /// Returns an error for unreadable files, input over 8 MiB, invalid UTF-8,
    /// malformed TOML or invalid document data.
    pub fn load(path: &Path) -> Result<Self, String> {
        use std::io::Read;
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        file.take(8 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 8 * 1024 * 1024 {
            return Err("File exceeds 8 MiB.".into());
        }
        Self::from_toml(std::str::from_utf8(&bytes).map_err(|e| e.to_string())?)
    }
    /// Validate and atomically replace a TOML document. The parent directory must
    /// already exist; overwrite prompts are the responsibility of the calling UI/CLI.
    ///
    /// # Errors
    ///
    /// Returns validation/serialization errors or filesystem errors from
    /// [`atomic_write`]. A failed write does not intentionally replace the destination.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        atomic_write(path, self.to_toml()?.as_bytes())
    }
}

/// Write beside the destination and rename only after the whole file is on disk.
///
/// # Errors
///
/// Returns an error for a missing filename, unavailable parent directory, or
/// failed write/sync/rename. Temporary files are removed on failure when possible.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let name = path
        .file_name()
        .ok_or("Choose a file name.")?
        .to_string_lossy();
    let temp = path.with_file_name(format!(
        ".{name}.{}-{}.tmp",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temp);
    }
    result.map_err(|e: std::io::Error| e.to_string())
}
