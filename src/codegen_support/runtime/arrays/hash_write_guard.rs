//! Purpose:
//! Exposes compiler imports and tests for Memoir storage assembly.
//!
//! Called from:
//! - Compiler runtime orchestration and focused memory integration tests.
//!
//! Key details:
//! - Preserves the compiler namespace while Memoir owns the assembly implementation.

pub use elephc_memoir::values::hash_write_guard::*;
