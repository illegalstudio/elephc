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
mod case_insensitive_symbols;
mod cli;
mod benchmarks;
mod echo_vars;
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
mod arrays;
mod callables;
mod system;
mod json;
mod regressions;
mod objects;
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
mod generators;
