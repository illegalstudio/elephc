//! Purpose:
//! Groups the array suites integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for associative arrays, indexed, associative-array helper builtins, nested arrays, array callbacks, list/key-edge builtins, the internal array pointer family, the hash key/value sorts that relink iteration order, runtime allocation-size guards, the maximum-size bounds reference PHP reports as a catchable `ValueError`, mutating builtins whose by-reference argument is a by-reference parameter or a property, static property, or container element, array builtin parameters widened to match reference PHP's parameter lists, the storage type stamped on array literals returned directly from a closure, and when an element write reads its index relative to the right-hand side.

mod allocation_guards;
mod size_bounds;
mod assoc;
mod closure_literal_returns;
mod by_ref_params;
mod by_ref_places;
mod indexed;
mod internal_pointer;
mod key_sort;
mod assoc_helpers;
mod nested;
mod callbacks;
mod foreach_key_write;
mod foreach_value_append;
mod list_and_keys;
mod list_unpack;
mod nested_autovivify;
mod nested_mixed_write;
mod mixed_append_autovivify;
mod assoc_set_ops;
mod widened_signatures;
mod write_evaluation_order;
