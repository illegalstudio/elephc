//! Purpose:
//! Groups the object-oriented PHP integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for instanceof, traits, inheritance, interfaces, class modifiers and properties, and related suites.

use crate::support::*;

#[path = "oop/instanceof.rs"]
mod instanceof;
#[path = "oop/traits.rs"]
mod traits;
#[path = "oop/inheritance.rs"]
mod inheritance;
#[path = "oop/interfaces.rs"]
mod interfaces;
#[path = "oop/modifiers_and_properties.rs"]
mod modifiers_and_properties;
#[path = "oop/callables/mod.rs"]
mod callables;
#[path = "oop/union_types.rs"]
mod union_types;
#[path = "oop/misc.rs"]
mod misc;
#[path = "oop/attributes.rs"]
mod attributes;
#[path = "oop/constants.rs"]
mod constants;
