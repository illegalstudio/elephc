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
mod strict_php;
mod lfc;
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
#[cfg(feature = "pdo-dblib")]
mod pdo_dblib;
#[cfg(feature = "pdo-firebird")]
mod pdo_firebird;
#[cfg(feature = "pdo-odbc")]
mod pdo_odbc;
#[cfg(feature = "pdo-informix")]
mod pdo_informix;
#[cfg(feature = "pdo-ibm")]
mod pdo_ibm;
#[cfg(feature = "pdo-sqlsrv")]
mod pdo_sqlsrv;
#[cfg(feature = "pdo-oci")]
mod pdo_oci;
#[cfg(feature = "pdo-cubrid")]
mod pdo_cubrid;
mod pdo_mysql;
mod pdo_pgsql;
mod image;
mod arrays;
mod calendar;
mod callables;
mod system;
mod json;
mod serialize;
mod regressions;
mod objects;
mod object_debug_output;
mod destructors;
mod references;
mod runtime_gc;
mod runtime_reachability;
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
mod dead_strip;
mod stack_guard;
mod dom;
mod dom_legacy_matrix;
mod dom_modern_matrix;
mod dom_selectors_matrix;
mod dom_xpath_matrix;
mod dom_fragment;
mod dom_validation;
mod dom_xinclude;
mod dom_c14n;
mod dom_xpath;
mod dom_xpath_direct_index;
mod dom_namespace_info;
mod dom_reflection_surface;
mod dom_parsing_limits;
mod dom_diagnostics_matrix;
mod dom_uncovered_route_probes;
mod libxml_entity_loader;
mod dom_stream_entity_matrix;
mod dom_validation_matrix;
mod dom_c14n_matrix;
mod dom_xinclude_matrix;
mod simplexml;
mod simplexml_tostring_override;
mod simplexml_debug_output;
mod simplexml_streams;
mod zval;
