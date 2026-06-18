//! Purpose:
//! Interpreter test module wiring and shared fake runtime support.
//! The concrete tests live in focused child modules so each file owns one
//! execution surface instead of one large mixed test bucket.
//!
//! Called from:
//! - `cargo test -p elephc-eval` through Rust's test harness.
//!
//! Key details:
//! - `support` exposes the fake runtime cells used by all interpreter tests.
//! - Child modules import the interpreter entry points from their parent module.

mod array_literals;
mod builtins_arrays_core;
mod builtins_arrays_iterators;
mod builtins_arrays_sets;
mod builtins_class_metadata;
mod builtins_directory_streams;
mod builtins_file_streams;
mod builtins_filesystem_metadata;
mod builtins_filesystem_ops;
mod builtins_json;
mod builtins_math_formatting;
mod builtins_scalars;
mod builtins_stream_contexts;
mod builtins_stream_settings;
mod builtins_strings_binary;
mod builtins_strings_encoding;
mod builtins_strings_text;
mod builtins_symbols;
mod builtins_system_network;
mod control_flow;
mod core;
mod dynamic_calls;
mod expressions;
mod functions_namespaces;
mod native_scope;
mod support;
