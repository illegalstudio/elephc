//! Purpose:
//! Groups the runtime GC integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for basics, regressions, stack args, copy-on-write and cycle handling, growth, related suites, resource scope-cleanup, by-reference builtin arguments that name a property, static property, or container element, calls that OMIT an optional by-reference argument (whose caller-side cell nothing reads back), and the reference a `foreach` loop holds on an object source.

#[path = "runtime_gc/basics.rs"]
mod basics;
#[path = "runtime_gc/nullable_string_return.rs"]
mod nullable_string_return;
#[path = "runtime_gc/iconv.rs"]
mod iconv;
#[path = "runtime_gc/parse_url.rs"]
mod parse_url;
#[path = "runtime_gc/getenv.rs"]
mod getenv;
#[path = "runtime_gc/pcntl.rs"]
mod pcntl;
#[path = "runtime_gc/putenv.rs"]
mod putenv;
#[path = "runtime_gc/regressions.rs"]
mod regressions;
#[path = "runtime_gc/assoc_rebind_release.rs"]
mod assoc_rebind_release;
#[path = "runtime_gc/by_ref_place_args.rs"]
mod by_ref_place_args;
#[path = "runtime_gc/omitted_by_ref_default_args.rs"]
mod omitted_by_ref_default_args;
#[path = "runtime_gc/foreach_object_source.rs"]
mod foreach_object_source;
#[path = "runtime_gc/stack_args.rs"]
mod stack_args;
#[path = "runtime_gc/cow_and_cycles.rs"]
mod cow_and_cycles;
#[path = "runtime_gc/core_builtins.rs"]
mod core_builtins;
#[path = "runtime_gc/dynamic_property_cycles.rs"]
mod dynamic_property_cycles;
mod gc_exception_recovery;
mod destructor_cleanup;
mod callback_argument_cleanup;
mod call_coercion_owners;
mod eval_sparse_arrays;
mod boxed_array_merge;
mod boxed_array_flip;
mod boxed_array_membership;
mod boxed_array_spread;
mod boxed_array_map;
mod boxed_array_implode;
mod boxed_array_reference_outputs;
mod boxed_array_column;
mod boxed_array_take;
mod boxed_array_unshift;
mod boxed_array_sort;
mod boxed_array_splice;
mod boxed_array_slice;
mod boxed_array_key_sort;
mod boxed_array_usort;
mod reference_cell_owners;
mod descriptor_reference_args;
mod descriptor_callable_owners;
mod class_name_owners;
mod handler_registration_owners;
mod closure_reference_owners;
mod boxed_array_call_results;
mod mixed_parameters;
mod native_string_arguments;
mod native_property_unset;
mod eval_closure_receivers;
mod eval_argument_lifetimes;
mod eval_array_references;
mod eval_operand_owners;
mod eval_scope_writeback;
mod unserialize_hydration_data;
mod serialize_magic_results;
mod nested_property_defaults;
#[path = "runtime_gc/growth.rs"]
mod growth;
#[path = "runtime_gc/heap.rs"]
mod heap;
#[path = "runtime_gc/heap_codegen.rs"]
mod heap_codegen;
#[path = "runtime_gc/resource_scope_cleanup.rs"]
mod resource_scope_cleanup;
#[path = "runtime_gc/resource_inventory.rs"]
mod resource_inventory;
