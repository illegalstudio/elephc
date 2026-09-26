//! Purpose:
//! Groups the runtime GC integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for basics, regressions, stack args, copy-on-write and cycle handling, growth, related suites, resource scope-cleanup, by-reference builtin arguments that name a property, static property, or container element, calls that OMIT an optional by-reference argument (whose caller-side cell nothing reads back), the reference a `foreach` loop holds on an object source, the containers an array literal allocates when it defaults a property, boxed or nested, and read-modify-write stores into a typed static property or a property array element.
//! - Submodules group focused fixtures for basics, regressions, stack args, copy-on-write and cycle handling, growth, related suites, resource scope-cleanup, by-reference builtin arguments that name a property, static property, or container element, calls that OMIT an optional by-reference argument (whose caller-side cell nothing reads back), the reference a `foreach` loop holds on an object source, and the ownership of an array literal whose element is an array-returning builtin call.
//! - The `assoc_chunk` submodule checks heap balance when `array_chunk()` copies an associative receiver.

#[path = "runtime_gc/basics.rs"]
mod basics;
#[path = "runtime_gc/mixed_string_cast_return.rs"]
mod mixed_string_cast_return;
#[path = "runtime_gc/object_cast.rs"]
mod object_cast;
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
#[path = "runtime_gc/by_ref_foreach_reference_cells.rs"]
mod by_ref_foreach_reference_cells;
#[path = "runtime_gc/compound_assign_stores.rs"]
mod compound_assign_stores;
#[path = "runtime_gc/assoc_chunk.rs"]
mod assoc_chunk;
#[path = "runtime_gc/literal_builtin_elements.rs"]
mod literal_builtin_elements;
#[path = "runtime_gc/object_supertype_rebind.rs"]
mod object_supertype_rebind;
#[path = "runtime_gc/boxed_property_defaults.rs"]
mod boxed_property_defaults;
#[path = "runtime_gc/by_ref_place_args.rs"]
mod by_ref_place_args;
#[path = "runtime_gc/by_ref_variadic_writeback.rs"]
mod by_ref_variadic_writeback;
#[path = "runtime_gc/omitted_by_ref_default_args.rs"]
mod omitted_by_ref_default_args;

#[path = "runtime_gc/callable_property_owners.rs"]
mod callable_property_owners;
#[path = "runtime_gc/foreach_object_source.rs"]
mod foreach_object_source;
#[path = "runtime_gc/foreach_iterator_aggregate_owner.rs"]
mod foreach_iterator_aggregate_owner;
#[path = "runtime_gc/nested_property_defaults.rs"]
mod nested_property_defaults;
#[path = "runtime_gc/spread_promotion.rs"]
mod spread_promotion;
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
mod destructor_catch_cleanup;
mod callback_argument_cleanup;
mod argument_evaluation_owners;
mod codegen_guard_operand_owners;
mod call_coercion_owners;
mod eval_sparse_arrays;
mod boxed_array_merge;
mod boxed_array_flip;
mod boxed_array_membership;
mod boxed_array_spread;
mod boxed_array_map;
mod boxed_array_reduce;
mod boxed_array_walk;
mod boxed_array_aggregates;
mod boxed_array_multisort;
mod instanceof_operand_owners;
mod boxed_array_set_comparators;
mod boxed_array_write_owners;
mod static_callable_string_owners;
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
mod callable_operand_owners;
mod capture_view_owners;
mod descriptor_reference_args;
mod descriptor_callable_owners;
mod descriptor_unpack_keys;
mod descriptor_variadic_container;
mod class_name_owners;
mod handler_registration_owners;
mod closure_reference_owners;
mod boxed_array_call_results;
mod mixed_parameters;
mod issue_551_ownership;
mod object_mixed_return_owners;
mod native_string_arguments;
mod native_property_unset;
mod eval_closure_receivers;
mod eval_argument_lifetimes;
mod eval_array_references;
mod eval_operand_owners;
mod eval_scope_writeback;
mod unserialize_hydration_data;
mod serialize_magic_results;
#[path = "runtime_gc/growth.rs"]
mod growth;
#[path = "runtime_gc/heap.rs"]
mod heap;
#[path = "runtime_gc/implode_boxed_operand.rs"]
mod implode_boxed_operand;
#[path = "runtime_gc/heap_codegen.rs"]
mod heap_codegen;
#[path = "runtime_gc/resource_scope_cleanup.rs"]
mod resource_scope_cleanup;
#[path = "runtime_gc/resource_inventory.rs"]
mod resource_inventory;
#[path = "runtime_gc/class_param_return.rs"]
mod class_param_return;
