//! Purpose:
//! Groups the regressions integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for scalars and regex, arrays, syntax edges, closures and refs, string memory, symbol collisions, pinned fixes for issues main already resolves, and related suites.

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
#[path = "regressions/pinned_fixed_issues.rs"]
mod pinned_fixed_issues;
#[path = "regressions/builtins_misc.rs"]
mod builtins_misc;
#[path = "regressions/concat_buffer_args.rs"]
mod concat_buffer_args;
#[path = "regressions/param_inference.rs"]
mod param_inference;
#[path = "regressions/method_array_assoc_param.rs"]
mod method_array_assoc_param;
#[path = "regressions/borrowed_array_widening.rs"]
mod borrowed_array_widening;
#[path = "regressions/mixed_method_dispatch.rs"]
mod mixed_method_dispatch;
#[path = "regressions/switch_and_float_params.rs"]
mod switch_and_float_params;
#[path = "regressions/return_this_ownership.rs"]
mod return_this_ownership;
#[path = "regressions/symbol_collisions.rs"]
mod symbol_collisions;
#[path = "regressions/handles_statics_and_reflectors.rs"]
mod handles_statics_and_reflectors;
#[path = "regressions/class_vars_and_owned_views.rs"]
mod class_vars_and_owned_views;
#[path = "regressions/nullable_receivers_and_spl_lists.rs"]
mod nullable_receivers_and_spl_lists;
#[path = "regressions/dynamic_define_and_static_hashes.rs"]
mod dynamic_define_and_static_hashes;
#[path = "regressions/untyped_getters_and_properties.rs"]
mod untyped_getters_and_properties;
#[path = "regressions/function_aliases.rs"]
mod function_aliases;
#[path = "regressions/mixed_receiver_call_results.rs"]
mod mixed_receiver_call_results;
#[path = "regressions/reference_captures.rs"]
mod reference_captures;
#[path = "regressions/callback_elements_and_results.rs"]
mod callback_elements_and_results;
#[path = "regressions/numeric_folds.rs"]
mod numeric_folds;
#[path = "regressions/mbstring_and_string_ownership.rs"]
mod mbstring_and_string_ownership;
#[path = "regressions/laravel_merge_seams.rs"]
mod laravel_merge_seams;
#[path = "regressions/fixed_issue_behaviour.rs"]
mod fixed_issue_behaviour;
#[path = "regressions/fixed_issue_leaks.rs"]
mod fixed_issue_leaks;
#[path = "regressions/large_runtime_outputs.rs"]
mod large_runtime_outputs;
#[path = "regressions/symfony_merge_deferrals.rs"]
mod symfony_merge_deferrals;
#[path = "regressions/agent_backend_deferrals.rs"]
mod agent_backend_deferrals;
#[path = "regressions/closed_world_refusals.rs"]
mod closed_world_refusals;
#[path = "regressions/dyn_prop_tables.rs"]
mod dyn_prop_tables;
#[path = "regressions/frame_slot_base.rs"]
mod frame_slot_base;
#[path = "regressions/invoker_name_tables.rs"]
mod invoker_name_tables;
#[path = "regressions/agent_checker_deferrals.rs"]
mod agent_checker_deferrals;
#[path = "regressions/agent_probe_throwable.rs"]
mod agent_probe_throwable;
#[path = "regressions/demo_language_gaps.rs"]
mod demo_language_gaps;
#[path = "regressions/laravel_language_gaps.rs"]
mod laravel_language_gaps;
mod review_fixes;
#[path = "regressions/symfony_backend_refusals.rs"]
mod symfony_backend_refusals;
