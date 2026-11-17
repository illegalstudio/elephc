//! Purpose:
//! Owns dependency-neutral native property reference metadata and validation coordination.
//!
//! Called from:
//! - Compiler ABI emission and generated native reference publishers through versioned C exports.
//!
//! Key details:
//! - The bridge depends only on Rust's standard library and imports no eval runtime hooks.
//! - Metadata is weak and worker-local; protected callbacks preserve native ownership and exits.
//! - Native and eval publishers share the same exported registry in one compiled program.

pub mod native_reference_sources;
pub mod native_reference_validation;
