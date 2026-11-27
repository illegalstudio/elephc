//! Purpose:
//! Interpreter test module wiring and shared fake runtime support.
//! The concrete tests live in focused child modules so each file owns one
//! execution surface instead of one large mixed test bucket.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - `support` exposes the fake runtime cells used by all interpreter tests.
//! - Child modules import the interpreter entry points from their parent module.

mod array_literals;
mod strict_nullable_receivers;
mod array_references;
mod bytecode;
mod bytecode_calls;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod bytecode_signals;
mod builtins_arrays_core;
mod builtins_arrays_iterators;
mod builtins_arrays_sets;
mod builtins_bcmath;
mod builtins_call_user_func_ownership;
mod builtins_class_metadata;
#[cfg(feature = "curl")]
mod builtins_curl;
mod builtins_debug_output;
mod builtins_func_args;
mod builtins_gc;
mod builtins_core_introspection;
mod include_inventory;
mod builtins_collection_ownership;
mod builtins_directory_streams;
mod builtins_output_buffering;
mod builtins_file_streams;
mod builtins_filesystem_metadata;
mod builtins_filesystem_ops;
mod builtins_json;
mod builtins_language_constructs;
mod builtins_math_formatting;
mod builtins_pcntl;
mod builtins_process_pipes;
mod builtins_raw_memory;
mod builtins_readline;
mod builtins_reflection_functions;
mod builtins_scalars;
mod builtins_xml;
mod builtins_spl_autoload;
mod builtins_stream_contexts;
mod builtins_stream_extensions;
mod builtins_stream_settings;
mod builtins_stream_sockets;
mod builtins_stream_wrapper_cast;
mod builtins_stream_wrapper_directories;
mod builtins_stream_wrapper_file_io;
mod builtins_stream_wrapper_metadata;
mod builtins_stream_wrapper_options;
mod builtins_stream_wrapper_path_ops;
mod builtins_stream_wrappers;
mod builtins_strings_binary;
mod builtins_strings_encoding;
mod builtins_strings_openssl;
mod builtins_strings_text;
mod builtins_symbols;
mod builtins_system_network;
mod class_constants;
mod classes;
mod closures;
mod control_flow;
mod strict_semantics;
mod strict_initialization;
mod strict_local_origins;
mod strict_binding_alternatives;
mod strict_structural_locals;
mod strict_resource_locals;
mod strict_parameter_locals;
mod strict_capture_locals;
mod strict_static_snapshot_owners;
mod strict_checked_casts;
mod strict_builtin_casts;
mod strict_settype;
mod strict_object_casts;
mod strict_gmp_intval;
mod strict_modulo;
mod strict_numeric_domains;
mod strict_numeric_parameters;
mod strict_numeric_aggregate_domains;
mod strict_intdiv;
mod strict_integer_power;
mod strict_unary_domains;
mod core;
mod dynamic_calls;
mod early_binding;
mod enums;
mod expressions;
mod fragment_constructs;
mod functions_namespaces;
mod jumps;
mod method_arguments;
mod native_scope;
mod null_object_boundaries;
mod opcache_script_cache;
mod operand_ownership;
mod magic_property_owners;
mod operand_string_sources;
mod operand_shared_runtime;
mod native_ref_cleanup;
mod nested_dimension_unset;
mod static_members;
mod static_dimension_writes;
mod generics;
mod support;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod repl;
mod trait_adaptations;

mod strict_conversion_parameters;
mod strict_comparisons;
mod strict_strings;
mod strict_conversion_properties;

mod strict_conversion_returns;

mod strict_string_conversions;

mod strict_static_locals;
mod strict_foreach_bindings;

mod exact_key_metadata;
mod owned_reference_keys;

mod exact_key_guards;

mod exact_builtin_keys;

mod exact_parent_keys;
mod exact_parent_update_keys;

mod exact_parent_places;
mod false_parent_completion;
mod escaped_reference_sources;

mod fresh_reference_sources;

mod strict_indexing;

mod strict_array_contents;
mod strict_array_mutation_origins;
mod strict_mixed_runtime;
mod strict_mixed_boundaries;
mod strict_mixed_producer_origins;
mod strict_mixed_builtin_boundaries;
mod strict_mixed_declared_sources;
mod strict_mixed_numeric_domains;
mod strict_mixed_spread_sources;
mod strict_mixed_stored_sources;
mod strict_mixed_native_sources;
mod strict_mixed_result_sources;
mod strict_mixed_callable_results;

mod strict_mixed_namespace_sources;

mod strict_mixed_forwarded_results;

mod strict_mixed_method_results;

mod strict_mixed_nullsafe_sources;

mod strict_mixed_access_sources;

mod strict_mixed_foreach_sources;

mod strict_mixed_class_sources;

mod strict_mixed_member_promises;

mod strict_array_member_promises;

mod strict_array_member_origins;

mod strict_declared_member_roots;

mod strict_member_source_promises;

mod strict_nested_array_origins;
mod strict_pending_member_consumers;

mod strict_write_source_domains;

mod strict_pending_admission_surfaces;
