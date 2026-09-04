//! Purpose:
//! Groups the runtime GC integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for basics, regressions, stack args, copy-on-write and cycle handling, growth, related suites, resource scope-cleanup, stream-registry lifecycle, by-reference builtin arguments that name a property, static property, or container element, calls that OMIT an optional by-reference argument (whose caller-side cell nothing reads back), and the reference a `foreach` loop holds on an object source.

#[path = "runtime_gc/basics.rs"]
mod basics;
#[path = "runtime_gc/nullable_string_return.rs"]
mod nullable_string_return;
#[path = "runtime_gc/iconv.rs"]
mod iconv;
#[path = "runtime_gc/parse_url.rs"]
mod parse_url;
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
#[path = "runtime_gc/growth.rs"]
mod growth;
#[path = "runtime_gc/heap.rs"]
mod heap;
#[path = "runtime_gc/heap_codegen.rs"]
mod heap_codegen;
#[path = "runtime_gc/resource_scope_cleanup.rs"]
mod resource_scope_cleanup;
#[path = "runtime_gc/stream_registry.rs"]
mod stream_registry;
#[path = "runtime_gc/stream_backend_registry.rs"]
mod stream_backend_registry;
#[path = "runtime_gc/stream_context_registry.rs"]
mod stream_context_registry;
#[path = "runtime_gc/stream_filter_registry.rs"]
mod stream_filter_registry;
#[path = "runtime_gc/stream_tls_registry.rs"]
mod stream_tls_registry;
#[path = "runtime_gc/user_wrapper_registry.rs"]
mod user_wrapper_registry;
