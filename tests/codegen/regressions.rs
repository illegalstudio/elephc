//! Purpose:
//! Groups the regressions integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for scalars and regex, arrays, syntax edges, closures and refs, string memory, and related suites.

use crate::support::*;

#[path = "regressions/scalars_and_regex.rs"]
mod scalars_and_regex;
#[path = "regressions/arrays.rs"]
mod arrays;
#[path = "regressions/syntax_edges.rs"]
mod syntax_edges;
#[path = "regressions/closures_and_refs.rs"]
mod closures_and_refs;
#[path = "regressions/string_memory.rs"]
mod string_memory;
#[path = "regressions/builtins_misc.rs"]
mod builtins_misc;
#[path = "regressions/concat_buffer_args.rs"]
mod concat_buffer_args;
#[path = "regressions/param_inference.rs"]
mod param_inference;
#[path = "regressions/mixed_method_dispatch.rs"]
mod mixed_method_dispatch;
