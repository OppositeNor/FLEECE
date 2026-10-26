//! FLEECE: editable logic-circuit and finite-state-machine diagrams.
//!
//! The library has no window or GPU requirement. The native host lives in the
//! binary's `desktop` module; it creates an [`app::Editor`] only for GUI launches.
//! Simulation controls are not part of the editor.
//!
//! # Building and validating a diagram
//!
//! ```
//! use fleece::model::{Circuit, Document, Endpoint, Kind, Point};
//!
//! let mut circuit = Circuit::empty();
//! let input = circuit.add(Kind::Input, Point::new(40.0, 40.0));
//! let output = circuit.add(Kind::Output, Point::new(300.0, 40.0));
//! circuit.connect(Endpoint { node: input, port: 0 },
//!                 Endpoint { node: output, port: 0 })?;
//! let document = Document::new(circuit);
//! document.validate()?;
//! let toml = document.to_toml()?;
//! assert_eq!(Document::from_toml(&toml)?, document);
//! # Ok::<(), String>(())
//! ```
//!
//! # Module boundaries
//!
//! - [`model`] owns persisted diagrams, validation and connectivity. Its component
//!   catalog supplies default names, shortcuts, categories and port counts.
//! - [`drawing`] defines scene primitives and document geometry;
//!   [`editor_drawing`] uses interactive geometry and the active UI palette.
//! - [`export`] crops and writes static PNG/SVG diagrams independently of selection,
//!   viewport, theme and unfinished gestures.
//! - [`history`] stores model snapshots, one per completed user action.
//! - [`theme`] and [`notation`] handle UI palettes and small math-label layouts.
//!
//! Validate externally constructed documents at persistence boundaries. Low-level
//! model mutations are useful for composing edits, but do not all validate an
//! entire document on their own.

#![warn(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

#[cfg(target_arch = "wasm32")]
extern crate self as fleece;

pub mod app;
pub mod drawing;
pub mod editor_drawing;
pub mod export;
pub mod history;
pub mod model;
pub mod notation;
pub mod theme;

#[cfg(target_arch = "wasm32")]
mod web;
