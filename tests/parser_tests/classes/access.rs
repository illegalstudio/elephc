//! Purpose:
//! Groups the class access integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for properties, methods, static properties, nullsafe property and method access, chains.

use super::*;

#[path = "access/properties.rs"]
mod properties;
#[path = "access/methods.rs"]
mod methods;
#[path = "access/static_properties.rs"]
mod static_properties;
#[path = "access/nullsafe.rs"]
mod nullsafe;
#[path = "access/chains.rs"]
mod chains;
