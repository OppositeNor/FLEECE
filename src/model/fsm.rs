//! Persisted finite state machine diagrams. Conditions and outputs are editable
//! annotations; the editor does not interpret them as executable expressions.
use super::Point;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// An annotation-only state graph; IDs are local to its states/transitions.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fsm {
    /// States in document/display order.
    #[serde(default)]
    pub states: Vec<State>,
    /// Directed transitions in document order, including loops and parallel edges.
    #[serde(default)]
    pub transitions: Vec<Transition>,
    /// Optional ID of the initial state; must refer to a state when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial: Option<u64>,
}
/// A named state whose position is its circle center in world coordinates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    /// Positive identifier unique within this FSM's states.
    pub id: u64,
    /// Display name; not used as a connectivity key.
    pub name: String,
    /// Circle center in world coordinates.
    pub position: Point,
    /// Moore-style output annotation, not an executable expression.
    #[serde(default)]
    pub output: String,
    /// Whether an accepting-state inner circle is drawn.
    #[serde(default)]
    pub accepting: bool,
}
/// A directed annotated edge; self-loops and parallel edges are valid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transition {
    /// Positive identifier unique within this FSM's transitions.
    pub id: u64,
    /// Source state ID.
    pub from: u64,
    /// Destination state ID; may equal the source for a self-loop.
    pub to: u64,
    /// Condition annotation drawn along the transition.
    #[serde(default)]
    pub condition: String,
    /// Mealy-style output annotation, not an executable expression.
    #[serde(default)]
    pub output: String,
}
impl Fsm {
    /// Find a state by its positive persisted ID.
    pub fn state(&self, id: u64) -> Option<&State> {
        self.states.iter().find(|s| s.id == id)
    }
    /// Mutably find a state without changing its identity/order.
    pub fn state_mut(&mut self, id: u64) -> Option<&mut State> {
        self.states.iter_mut().find(|s| s.id == id)
    }
    /// Add a state at a world-space center and return its fresh FSM-local ID.
    /// The first state becomes initial when no initial state has been assigned.
    pub fn add_state(&mut self, position: Point) -> u64 {
        let id = self.states.iter().map(|s| s.id).max().unwrap_or(0) + 1;
        let name = format!("S{}", self.states.len());
        self.states.push(State {
            id,
            name,
            position,
            output: String::new(),
            accepting: false,
        });
        if self.initial.is_none() {
            self.initial = Some(id);
        }
        id
    }
    /// Add an unannotated directed transition and return its fresh local ID.
    ///
    /// # Errors
    /// Fails before mutation if either endpoint state is missing. Loops and parallel
    /// edges are valid.
    pub fn add_transition(&mut self, from: u64, to: u64) -> Result<u64, String> {
        if self.state(from).is_none() || self.state(to).is_none() {
            return Err("A transition must connect existing states.".into());
        }
        let id = self.transitions.iter().map(|t| t.id).max().unwrap_or(0) + 1;
        self.transitions.push(Transition {
            id,
            from,
            to,
            condition: String::new(),
            output: String::new(),
        });
        Ok(id)
    }
    /// Remove states and incident transitions. If the initial state is removed,
    /// choose the first surviving state, or clear the initial state when empty.
    pub fn delete_states(&mut self, ids: &BTreeSet<u64>) {
        self.states.retain(|s| !ids.contains(&s.id));
        self.transitions
            .retain(|t| !ids.contains(&t.from) && !ids.contains(&t.to));
        if self.initial.is_some_and(|id| ids.contains(&id)) {
            self.initial = self.states.first().map(|s| s.id);
        }
    }
    /// Copy selected states and transitions whose two endpoints are selected.
    /// The initial-state reference is retained only if that state is included.
    pub fn selection(&self, ids: &BTreeSet<u64>) -> Self {
        Self {
            states: self
                .states
                .iter()
                .filter(|s| ids.contains(&s.id))
                .cloned()
                .collect(),
            transitions: self
                .transitions
                .iter()
                .filter(|t| ids.contains(&t.from) && ids.contains(&t.to))
                .cloned()
                .collect(),
            initial: self.initial.filter(|id| ids.contains(id)),
        }
    }
    /// Paste an FSM fragment with translated positions and remapped state/edge IDs.
    /// Existing names/annotations are preserved, including duplicate names.
    ///
    /// # Errors
    /// Fails without mutation for invalid fragments, ID exhaustion or diagram limits.
    pub fn paste_fragment(
        &mut self,
        fragment: &Self,
        offset: Point,
    ) -> Result<BTreeSet<u64>, String> {
        fragment.validate()?;
        if fragment.states.is_empty() {
            return Err("No states in the clipboard.".into());
        }
        if self.states.len() + fragment.states.len() > 1000
            || self.transitions.len() + fragment.transitions.len() > 8000
        {
            return Err("Paste exceeds the limit of 1,000 states or 8,000 transitions.".into());
        }
        let mut working = self.clone();
        let mut next_state = working.states.iter().map(|s| s.id).max().unwrap_or(0);
        let mut next_transition = working.transitions.iter().map(|t| t.id).max().unwrap_or(0);
        let mut ids = BTreeMap::new();
        for mut state in fragment.states.clone() {
            next_state += 1;
            ids.insert(state.id, next_state);
            state.id = next_state;
            state.position.x += offset.x;
            state.position.y += offset.y;
            working.states.push(state);
        }
        for mut transition in fragment.transitions.clone() {
            next_transition += 1;
            transition.id = next_transition;
            transition.from = ids[&transition.from];
            transition.to = ids[&transition.to];
            working.transitions.push(transition);
        }
        if working.initial.is_none() {
            working.initial = fragment.initial.and_then(|id| ids.get(&id).copied());
        }
        working.validate()?;
        *self = working;
        Ok(ids.into_values().collect())
    }
    /// Check IDs, endpoints, annotation lengths and coordinate/graph-size limits.
    ///
    /// # Errors
    ///
    /// Returns a descriptive error for duplicate/invalid IDs, missing referenced
    /// states, invalid annotations/positions, or graph-size limits.
    pub fn validate(&self) -> Result<(), String> {
        if self.states.len() > 1000 || self.transitions.len() > 8000 {
            return Err("FSMs support at most 1,000 states and 8,000 transitions.".into());
        }
        let valid_text =
            |s: &str, limit| s.chars().count() <= limit && !s.chars().any(char::is_control);
        let mut ids = BTreeSet::new();
        for s in &self.states {
            if s.id == 0
                || s.id >= u64::MAX - 2000
                || !ids.insert(s.id)
                || !s.position.finite()
                || s.name.trim().is_empty()
                || !valid_text(&s.name, 80)
                || !valid_text(&s.output, 160)
            {
                return Err("States require unique positive IDs, finite positions, a nonempty name (≤80 characters), and outputs of at most 160 printable characters.".into());
            }
        }
        if self.initial.is_some_and(|id| !ids.contains(&id)) {
            return Err("The initial state must reference an existing state.".into());
        }
        let mut transitions = BTreeSet::new();
        for t in &self.transitions {
            if t.id == 0
                || t.id >= u64::MAX - 2000
                || !transitions.insert(t.id)
                || !ids.contains(&t.from)
                || !ids.contains(&t.to)
                || !valid_text(&t.condition, 160)
                || !valid_text(&t.output, 160)
            {
                return Err("Transitions require unique positive IDs, existing source/destination states, and conditions/outputs of at most 160 printable characters.".into());
            }
        }
        Ok(())
    }
}
