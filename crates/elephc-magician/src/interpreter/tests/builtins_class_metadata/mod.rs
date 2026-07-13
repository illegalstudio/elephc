//! Purpose:
//! Organizes interpreter coverage for eval-backed class metadata and Reflection.
//! Each child module owns one coherent PHP-visible metadata surface.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - The split mirrors the production Reflection boundaries without sharing fixtures.

mod attributes;
mod callable_types;
mod class_capabilities;
mod class_identity;
mod class_operations;
mod constants_enums_objects;
mod member_constructors;
mod member_metadata;
mod parameter_defaults;
mod property_values;
mod relations;
