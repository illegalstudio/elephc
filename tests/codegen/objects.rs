//! Purpose:
//! Groups the object suites integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for classes, object GC aliasing, magic methods, property access, constructor property promotion, and related suites.

use crate::support::*;

#[path = "objects/classes.rs"]
mod classes;
#[path = "objects/gc_aliasing.rs"]
mod gc_aliasing;
#[path = "objects/magic_methods.rs"]
mod magic_methods;
#[path = "objects/property_access/mod.rs"]
mod property_access;
#[path = "objects/constructor_promotion.rs"]
mod constructor_promotion;
#[path = "objects/static_properties.rs"]
mod static_properties;
#[path = "objects/nested_arrays.rs"]
mod nested_arrays;
#[path = "objects/nullable_dispatch.rs"]
mod nullable_dispatch;
