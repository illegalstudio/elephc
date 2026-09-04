//! Purpose:
//! Groups the types integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for return type inference, enums, type annotations, named arguments, parameter coercion, examples, and related suites.

use crate::support::*;

#[path = "types/return_inference.rs"]
mod return_inference;

#[path = "types/enums.rs"]
mod enums;
#[path = "types/type_annotations.rs"]
mod type_annotations;
#[path = "types/narrowing.rs"]
mod narrowing;
#[path = "types/static_property_hash.rs"]
mod static_property_hash;
#[path = "types/property_element_unset.rs"]
mod property_element_unset;
#[path = "types/reassign_across_kinds.rs"]
mod reassign_across_kinds;
#[path = "types/param_coercion.rs"]
mod param_coercion;
#[path = "types/strict_types.rs"]
mod strict_types;
#[path = "types/named_arguments/mod.rs"]
mod named_arguments;
#[path = "types/examples.rs"]
mod examples;
#[path = "types/never.rs"]
mod never;
#[path = "types/iterable/mod.rs"]
mod iterable;
