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
#[path = "regressions/method_array_assoc_param.rs"]
mod method_array_assoc_param;
#[path = "regressions/mixed_method_dispatch.rs"]
mod mixed_method_dispatch;
#[path = "regressions/switch_and_float_params.rs"]
mod switch_and_float_params;
#[path = "regressions/return_this_ownership.rs"]
mod return_this_ownership;
#[path = "regressions/superglobal_function_scope.rs"]
mod superglobal_function_scope;
#[path = "regressions/symbol_writeback.rs"]
mod symbol_writeback;
#[path = "regressions/refcell_return.rs"]
mod refcell_return;
#[path = "regressions/list_unpack_assoc.rs"]
mod list_unpack_assoc;
#[path = "regressions/implode_assoc.rs"]
mod implode_assoc;
#[path = "regressions/top_level_static_null_guard.rs"]
mod top_level_static_null_guard;
