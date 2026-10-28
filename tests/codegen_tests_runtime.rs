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
}
