//! Purpose:
//! Groups the types integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for return type inference, enums, type annotations, named arguments, examples, and related suites.

use crate::support::*;

#[path = "types/return_inference.rs"]
mod return_inference;

#[path = "types/enums.rs"]
mod enums;
#[path = "types/type_annotations.rs"]
mod type_annotations;
#[path = "types/named_arguments/mod.rs"]
mod named_arguments;
#[path = "types/examples.rs"]
mod examples;
#[path = "types/never.rs"]
mod never;
#[path = "types/iterable/mod.rs"]
mod iterable;
