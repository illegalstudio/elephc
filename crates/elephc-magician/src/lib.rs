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
#[cfg(feature = "curl")]
mod curl_ffi;
pub mod errors;
pub mod eval_ir;
mod eval_php_profile;
mod ffi;
pub mod interpreter;
mod json_validate;
mod lexer;
pub mod lower;
mod parse_cache;
pub mod parser;
mod regex_provider;
pub mod runtime_hooks;
pub mod scope;
mod strict_php_mode;
mod stream_resources;
mod stream_wrappers;
pub mod value;

pub use interpreter::builtin_metadata;
pub use ffi::*;
pub use regex_provider::__elephc_eval_register_regex_provider;
