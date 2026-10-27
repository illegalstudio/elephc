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
#[path = "types/enum_review_runtime_args.rs"]
mod enum_review_runtime_args;
#[path = "types/enum_argument_boundaries.rs"]
mod enum_argument_boundaries;
#[path = "types/enum_selected_type_review.rs"]
mod enum_selected_type_review;
mod enum_reference_type_review;
mod enum_boxed_reference_review;
mod enum_argument_lifetime_review;
mod enum_unpack_effect_review;
mod enum_result_owner_review;
mod enum_reference_selection_review;
mod enum_omission_review;
#[path = "types/enum_keyed_review.rs"]
mod enum_keyed_review;
#[path = "types/type_annotations.rs"]
mod type_annotations;
#[path = "types/nullable_array_merge.rs"]
mod nullable_array_merge;
#[path = "types/narrowing.rs"]
mod narrowing;
#[path = "types/narrowed_object_arguments.rs"]
mod narrowed_object_arguments;
mod guarded_interface_receivers;
#[path = "types/param_coercion.rs"]
mod param_coercion;
#[path = "types/declare_groups.rs"]
mod declare_groups;
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
