//! Purpose:
//! End-to-end codegen test binary for the runtime: garbage collection, references, destructors, buffers, pointers and FFI.
//!
//! Called from:
//! - `cargo test --test codegen_tests_runtime` through Rust's test harness.
//!
//! Key details:
//! - The codegen suite is split into six `codegen_tests_*` binaries by domain so a library edit
//!   recompiles them in parallel; test names keep the `codegen::<module>::` path, and CI selects
//!   the family with `binary(/^codegen_tests_/)`.
//! - Each binary mounts the shared runner helpers; the ones a domain never calls are expected.

#[allow(dead_code)]
#[path = "support/managed_pcre2.rs"]
mod managed_pcre2_support;

#[allow(dead_code, unused_imports, unused_macros)]
#[path = "codegen/support/mod.rs"]
mod support;

#[allow(dead_code)]
#[path = "support/locals_retype_fixtures.rs"]
mod locals_retype_fixtures;

mod codegen {
    mod buffers;
    mod buffer_ptr;
    mod buffer_arrays;
    mod buffer_merges;
    mod buffer_record_copy;
    mod static_buffer_properties;
    mod packed_nested_fields;
    mod packed_field_narrowing;
    mod data_widths;
    mod null_sentinel;
    mod call_counters;
    mod known_closure_loops;
    mod destructors;
    mod destructor_unwind_calls;
    mod reference_property_payload;
    mod references;
    mod runtime_gc;
    mod runtime_reachability;
    mod runtime_introspection;
    mod pointers;
    mod pointer_strings;
    mod nullable_pointers;
    mod pointer_arrays;
    mod ffi;
    mod dead_strip;
    mod locals_retype;
    mod stack_guard;
    mod descriptor_reference_layouts;
    mod typed_element_reference_args;
    mod strict_semantics;
    mod strict_profiles_public;
    mod strict_initialization;
    mod strict_indexing;
    mod strict_nullability;
    mod strict_numeric_locals;
    mod strict_copy_origins;
    mod strict_structural_locals;
    mod strict_resource_locals;
    mod strict_parameter_locals;
    mod strict_capture_locals;
    mod strict_static_snapshot_owners;
    mod strict_numeric_aggregates;
    mod strict_numeric_eval;
    mod strict_eval_diagnostics;
    mod strict_eval_division;
    mod strict_eval_modulo;
    mod strict_eval_intdiv;
    mod strict_division_modulo;
    mod strict_numeric_domains;
    mod strict_overflow_public;
    mod strict_integer_power;
    mod strict_unary_domains;
    mod strict_unary_ieee;
    mod strict_object_casts;
    mod strict_object_cast_domains;
    mod strict_gmp_intval;
    mod eval_scope_contracts;
    mod strict_conversion_parameters;
    mod strict_comparisons;
    mod strict_comparison_callbacks;
    mod strict_comparisons_public;
    mod strict_strings;
    mod strict_string_native_views;
    mod strict_string_missing_reads;
    mod strict_string_boxed_nulls;
    mod strict_string_eval_hosts;
    mod strict_string_public;
    mod strict_scope_snapshot_controls;
    mod strict_eval_property_references;
    mod strict_coercions_public;
    mod strict_coercion_hosts;
    mod strict_coercion_missing_values;
    mod strict_cast_reflection_bindings;
    mod strict_casts_public;
    mod strict_cast_argument_aliases;
    mod strict_static_rebind_candidates;
    mod strict_conversion_returns;
    mod strict_static_locals;
    mod strict_native_static_bindings;
    mod strict_foreach_bindings;
}
