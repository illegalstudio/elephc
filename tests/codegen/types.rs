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
#[path = "types/narrowed_object_arguments.rs"]
mod narrowed_object_arguments;
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
