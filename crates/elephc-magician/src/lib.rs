//! Purpose:
//! Optional C ABI bridge crate for elephc's runtime `eval()` support.
//! The crate root owns only the public module map and re-exports stable FFI
//! entry points whose implementations live in focused modules.
//!
//! Called from:
//! - Generated EIR backend assembly through `__elephc_eval_*` symbols.
//! - `cargo test -p elephc-magician` for ABI-shape validation.
//!
//! Key details:
//! - No Rust panic or Rust-specific enum crosses the ABI boundary.
//! - Non-test builds execute EvalIR through generated runtime value wrappers.

pub mod abi;
pub mod context;
pub mod errors;
pub mod eval_ir;
mod ffi;
pub mod interpreter;
mod json_validate;
mod lexer;
pub mod lower;
pub mod parser;
pub mod runtime_hooks;
pub mod scope;
mod stream_resources;
pub mod value;

pub use ffi::*;
