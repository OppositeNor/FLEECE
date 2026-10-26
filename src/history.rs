//! Snapshot-based undo/redo for diagram content.
//!
//! Each completed action contributes its previous [`Circuit`], rather than a
//! reversible command object. Drags and text edits coalesce before calling
//! [`History::record`]. Viewport, selection, theme and native preferences are not
//! part of these snapshots. A no-op preserves the redo stack; a changed edit
//! clears it. The most recent 150 edits are retained.
//!
//! ```
//! use fleece::{history::History, model::Circuit};
//! let mut circuit = Circuit::empty();
//! let mut history = History::default();
//! let before = circuit.clone();
//! circuit.title = "New title".into();
//! history.record(before, &circuit);
//! assert!(history.undo(&mut circuit));
//! assert_eq!(circuit.title, "Untitled circuit");
//! assert!(history.redo(&mut circuit));
//! assert_eq!(circuit.title, "New title");
//! ```

use crate::model::Circuit;

/// One entry per user action, including an entire drag or text edit.
#[derive(Default)]
pub struct History {
    undo: Vec<Circuit>,
    redo: Vec<Circuit>,
}
impl History {
    /// Record a completed mutation; identical snapshots add no entry.
    pub fn record(&mut self, before: Circuit, after: &Circuit) {
        if &before == after {
            return;
        }
        self.undo.push(before);
        self.redo.clear();
        if self.undo.len() > 150 {
            self.undo.remove(0);
        }
    }
    /// Restore the previous snapshot and move the current one to redo.
    /// Returns false without changing the circuit when the stack is empty.
    pub fn undo(&mut self, circuit: &mut Circuit) -> bool {
        if let Some(previous) = self.undo.pop() {
            self.redo.push(std::mem::replace(circuit, previous));
            true
        } else {
            false
        }
    }
    /// Restore the next snapshot and move the current one back to undo.
    pub fn redo(&mut self, circuit: &mut Circuit) -> bool {
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(circuit, next));
            true
        } else {
            false
        }
    }
    /// Whether an earlier diagram snapshot can be restored.
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    /// Whether an undone diagram snapshot can be reapplied.
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}
