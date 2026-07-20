//! Purpose:
//! Defines and re-exports the dynamic EvalIR used by runtime `eval()` fragments.
//! Node families live in focused child modules without changing the public model.
//!
//! Called from:
//! - `crate::parser::parse_fragment()`
//! - `crate::interpreter` execution.
//!
//! Key details:
//! - Runtime execution must turn constants into elephc runtime cells through
//!   value-bridge hooks; EvalIR constants are syntax data, not owned PHP values.

mod attributes;
mod callable;
mod classes;
mod enums;
mod expressions;
mod interfaces;
mod methods;
mod program;
mod properties;
mod statements;
mod traits;

pub use attributes::*;
pub use callable::*;
pub use classes::*;
pub use enums::*;
pub use expressions::*;
pub use interfaces::*;
pub use methods::*;
pub use program::*;
pub use properties::*;
pub use statements::*;
pub use traits::*;
