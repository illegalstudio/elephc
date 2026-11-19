//! Purpose:
//! Shares diagnostics for reads that require an initialized source binding.
//!
//! Called from:
//! - Compiler checking, native read guards and interpreter variable access.
//!
//! Key details:
//! - Binding presence is independent of the value kind, including initialized null.

/// Describes a missing binding consistently across static and runtime boundaries.
pub fn variable_message(name: &str) -> String {
    format!("--strict-initialization: variable ${name} is not initialized")
}
