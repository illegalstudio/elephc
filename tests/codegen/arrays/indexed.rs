//! Purpose:
//! Groups the indexed array builtins integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for aggregates, array search, merge, and union builtins, array slicing, stack, and range builtins, array set-operation builtins, array shape-transform builtins, and related suites.

use crate::support::*;

#[path = "indexed/aggregates.rs"]
mod aggregates;
#[path = "indexed/heterogeneous.rs"]
mod heterogeneous;
#[path = "indexed/nullsafe_literals.rs"]
mod nullsafe_literals;
#[path = "indexed/key_sort_nested_mixed.rs"]
mod key_sort_nested_mixed;
#[path = "indexed/pad_bounds.rs"]
mod pad_bounds;
#[path = "indexed/search_merge_union.rs"]
mod search_merge_union;
#[path = "indexed/slice_bounds.rs"]
mod slice_bounds;
#[path = "indexed/slice_stack_range.rs"]
mod slice_stack_range;
#[path = "indexed/splice_replacement.rs"]
mod splice_replacement;
#[path = "indexed/splice_strings.rs"]
mod splice_strings;
#[path = "indexed/set_ops.rs"]
mod set_ops;
#[path = "indexed/shape_transforms.rs"]
mod shape_transforms;
#[path = "indexed/sorting.rs"]
mod sorting;
#[path = "indexed/oob_reads.rs"]
mod oob_reads;
