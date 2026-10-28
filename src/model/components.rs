//! Component identity and metadata shared by the model, library, and shortcuts.
use serde::{Deserialize, Serialize};

/// Persisted component kind, also used to select default ports and symbol geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Named input terminal.
    Input,
    /// Named output terminal.
    Output,
    /// AND gate.
    And,
    /// OR gate.
    Or,
    /// Unary inverter gate.
    Not,
    /// AND gate with output inversion.
    Nand,
    /// OR gate with output inversion.
    Nor,
    /// Exclusive-OR gate.
    Xor,
    /// Exclusive-OR gate with output inversion.
    Xnor,
    /// Unary buffer gate.
    Buffer,
    /// Constant-one source symbol.
    High,
    /// Constant-zero source symbol.
    Low,
    /// Multiplexer with configurable selector width.
    Mux,
    /// Half-adder component with sum and carry outputs.
    HalfAdder,
    /// Full-adder component with carry input, sum and carry outputs.
    FullAdder,
    /// D-flip-flop or register-bank symbol with D, CLK, RST and Q ports.
    Register,
    /// Custom component with configurable input/output ports.
    Module,
    /// Persistent zero-size connection joint inserted directly on wires.
    Joint,
}
/// Component-library category; grouping is independent of catalog array positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentGroup {
    /// Named input/output terminals.
    Terminals,
    /// Logic gate symbols.
    Gates,
    /// Constant source symbols.
    Sources,
    /// Multiplexers, adders, registers and custom modules.
    Modules,
    /// Internal items such as joints, omitted from component placement menus.
    Internal,
}
impl ComponentGroup {
    /// Visible library categories in display order.
    pub const LIBRARY: [Self; 4] = [Self::Terminals, Self::Gates, Self::Sources, Self::Modules];
    /// Return the heading shown above this category in the component panel.
    pub fn title(self) -> &'static str {
        match self {
            Self::Terminals => "TERMINALS",
            Self::Gates => "LOGIC GATES",
            Self::Sources => "SOURCES",
            Self::Modules => "MODULES",
            Self::Internal => "INTERNAL",
        }
    }
}

/// Add a component's presentation, category and default ports in one place.
#[derive(Debug, Clone, Copy)]
pub struct ComponentDefinition {
    /// Plain display name used by menus and initial component labels.
    pub name: &'static str,
    /// Keyboard placement shortcut, such as `Shift+A`, or an empty string for joints.
    pub shortcut: &'static str,
    /// Library category and broad symbol capability group.
    pub group: ComponentGroup,
    /// Default number of zero-based input ports.
    pub inputs: usize,
    /// Default number of zero-based output ports.
    pub outputs: usize,
    /// Whether the kind supports configurable input counts.
    pub variable_inputs: bool,
}
impl ComponentDefinition {
    const fn new(
        name: &'static str,
        shortcut: &'static str,
        group: ComponentGroup,
        inputs: usize,
        outputs: usize,
    ) -> Self {
        Self {
            name,
            shortcut,
            group,
            inputs,
            outputs,
            variable_inputs: false,
        }
    }
    const fn variable(mut self) -> Self {
        self.variable_inputs = true;
        self
    }
}

impl Kind {
    /// Components offered in the library; joints are inserted directly on wires.
    pub const ALL: [Self; 17] = [
        Self::Input,
        Self::Output,
        Self::And,
        Self::Or,
        Self::Not,
        Self::Nand,
        Self::Nor,
        Self::Xor,
        Self::Xnor,
        Self::Buffer,
        Self::High,
        Self::Low,
        Self::Mux,
        Self::HalfAdder,
        Self::FullAdder,
        Self::Register,
        Self::Module,
    ];
    /// Return the shared metadata used by constructors, library groups and shortcuts.
    pub const fn definition(self) -> ComponentDefinition {
        use ComponentGroup::*;
        match self {
            Self::Input => ComponentDefinition::new("Input", "I", Terminals, 0, 1),
            Self::Output => ComponentDefinition::new("Output", "O", Terminals, 1, 0),
            Self::And => ComponentDefinition::new("AND", "A", Gates, 2, 1).variable(),
            Self::Or => ComponentDefinition::new("OR", "R", Gates, 2, 1).variable(),
            Self::Not => ComponentDefinition::new("NOT", "N", Gates, 1, 1),
            Self::Nand => ComponentDefinition::new("NAND", "Shift+A", Gates, 2, 1).variable(),
            Self::Nor => ComponentDefinition::new("NOR", "Shift+R", Gates, 2, 1).variable(),
            Self::Xor => ComponentDefinition::new("XOR", "X", Gates, 2, 1),
            Self::Xnor => ComponentDefinition::new("XNOR", "Shift+E", Gates, 2, 1).variable(),
            Self::Buffer => ComponentDefinition::new("Buffer", "B", Gates, 1, 1),
            Self::High => ComponentDefinition::new("Constant 1", "1", Sources, 0, 1),
            Self::Low => ComponentDefinition::new("Constant 0", "0", Sources, 0, 1),
            Self::Mux => ComponentDefinition::new("Multiplexer", "M", Modules, 3, 1),
            Self::HalfAdder => ComponentDefinition::new("Half adder", "H", Modules, 2, 2),
            Self::FullAdder => ComponentDefinition::new("Full adder", "Shift+H", Modules, 3, 2),
            Self::Register => ComponentDefinition::new("D-flip-flop", "Shift+D", Modules, 4, 2),
            Self::Module => {
                ComponentDefinition::new("Custom module", "U", Modules, 2, 1).variable()
            }
            Self::Joint => ComponentDefinition::new("Joint", "", Internal, 1, 1),
        }
    }
    /// Return the plain catalog name without instance numbering.
    pub const fn name(self) -> &'static str {
        self.definition().name
    }
    /// Return the placement shortcut shown by the library and used by keyboard dispatch.
    pub const fn shortcut(self) -> &'static str {
        self.definition().shortcut
    }
    /// Whether this kind belongs to the multiport module-symbol category.
    pub fn module(self) -> bool {
        self.definition().group == ComponentGroup::Modules
    }
    /// Whether this kind is a logic gate supporting gate-style inversion annotations.
    pub fn gate(self) -> bool {
        self.definition().group == ComponentGroup::Gates
    }
    /// Whether the component input count can be configured.
    pub const fn variable_inputs(self) -> bool {
        self.definition().variable_inputs
    }
    /// Whether placement uses the library's gate-input preference.
    /// XOR and unary gates have fixed input counts.
    pub fn configurable_gate_inputs(self) -> bool {
        self.gate() && self.variable_inputs()
    }
}
