//! Purpose:
//! Groups the math integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for functions.

#[path = "math/functions.rs"]
mod functions;
