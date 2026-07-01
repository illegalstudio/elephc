//! Purpose:
//! Groups the top-level end-to-end codegen test modules into the integration suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for exceptions, fibers, buffers, preprocessor, namespaces, and related suites.

mod exceptions;
mod fibers;
mod buffers;
mod preprocessor;
mod namespaces;
mod null_sentinel;
mod case_insensitive_symbols;
mod cli;
mod benchmarks;
mod echo_vars;
mod eval;
mod eval_builtin_parity;
mod eval_callable_ref_errors;
mod eval_callables;
mod eval_closures;
mod eval_constructors;
mod eval_reflection_invocation;
mod operators;
mod control_flow;
mod scalar_strings;
mod array_basics;
mod numeric_scalars;
mod type_builtins;
mod casts_and_constants;
mod include_paths;
mod magic_constants;
mod strings;
mod io;
mod pdo;
mod pdo_mysql;
mod pdo_pgsql;
mod image;
mod arrays;
mod calendar;
mod callables;
mod system;
mod json;
mod regressions;
mod objects;
mod destructors;
mod runtime_gc;
mod math;
mod misc;
mod pointers;
mod ffi;
mod oop;
mod static_class_features;
mod types;
mod optimizer;
mod iterators;
mod spl;
mod generators;
