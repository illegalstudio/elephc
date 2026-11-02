//! Purpose:
//! Groups the object suites integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for classes, object GC aliasing, magic methods, dynamic-property dumps, property access, constructor property promotion, and related suites.
//! - `mixed_sourced_typed_param_limitation` is a KNOWN-MISCOMPILE pin rather than a parity fixture: it asserts today's WRONG answer for an object that reaches a typed object parameter through Mixed storage, and names the curl workaround that exists because of it.

use crate::support::*;

#[path = "objects/classes.rs"]
mod classes;
#[path = "objects/gc_aliasing.rs"]
mod gc_aliasing;
#[path = "objects/magic_methods.rs"]
mod magic_methods;
#[path = "objects/cloning.rs"]
mod cloning;
#[path = "objects/debug_info_dumps.rs"]
mod debug_info_dumps;
#[path = "objects/dynamic_property_dumps.rs"]
mod dynamic_property_dumps;
#[path = "objects/property_access/mod.rs"]
mod property_access;
#[path = "objects/constructor_promotion.rs"]
mod constructor_promotion;
#[path = "objects/static_properties.rs"]
mod static_properties;
#[path = "objects/static_false_writes.rs"]
mod static_false_writes;
#[path = "objects/nested_write_review.rs"]
mod nested_write_review;
#[path = "objects/nested_append_review.rs"]
mod nested_append_review;
#[path = "objects/untyped_property_defaults.rs"]
mod untyped_property_defaults;
#[path = "objects/nested_array_property_defaults.rs"]
mod nested_array_property_defaults;
#[path = "objects/nested_arrays.rs"]
mod nested_arrays;
#[path = "objects/nullable_dispatch.rs"]
mod nullable_dispatch;
#[path = "objects/mixed_sourced_typed_param_limitation.rs"]
mod mixed_sourced_typed_param_limitation;
