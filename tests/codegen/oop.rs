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
#[path = "oop/relative_types.rs"]
mod relative_types;
#[path = "oop/anonymous_classes.rs"]
mod anonymous_classes;
#[path = "oop/intersection_types.rs"]
mod intersection_types;
#[path = "oop/dynamic_dispatch.rs"]
mod dynamic_dispatch;
#[path = "oop/misc.rs"]
mod misc;
#[path = "oop/attributes.rs"]
mod attributes;
#[path = "oop/constants.rs"]
mod constants;
#[path = "oop/abstract_properties.rs"]
mod abstract_properties;
#[path = "oop/property_hooks.rs"]
mod property_hooks;
#[path = "oop/datetime.rs"]
mod datetime;
