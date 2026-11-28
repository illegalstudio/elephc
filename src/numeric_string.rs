//! Purpose:
//! Reexports the dependency-neutral PHP numeric-string grammar for compiler consumers.
//!
//! Called from:
//! - Constant folding, argument binding, numeric proofs and operator inference.
//!
//! Key details:
//! - Compiler and eval adapters share one host-side grammar; native emitters mirror it.

pub(crate) use elephc_semantics::numeric_string::{scan_numeric_prefix, NumericScan};
