//! Purpose:
//! Integration test root wiring for codegen support helpers and end-to-end PHP-to-native suites.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules register the codegen tree and shared runner helpers used by native binary fixtures.

#[path = "codegen/support/mod.rs"]
mod support;

#[path = "codegen/mod.rs"]
mod codegen;
