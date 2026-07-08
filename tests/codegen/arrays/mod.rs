//! Purpose:
//! Groups the array suites integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for associative arrays, indexed, associative-array helper builtins, nested arrays, array callbacks, list/key-edge builtins.

mod assoc;
mod indexed;
mod assoc_helpers;
mod nested;
mod callbacks;
mod foreach_key_write;
mod list_and_keys;
mod assoc_set_ops;
