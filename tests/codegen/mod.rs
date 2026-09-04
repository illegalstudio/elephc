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
mod null_receivers;
mod undefined_variables;
mod nullable_builtin_arguments;
mod prelude_injection_parity;
mod php_tags;
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
mod array_to_string;
mod object_to_string;
mod offset_on_scalar;
mod numeric_scalars;
mod type_builtins;
mod casts_and_constants;
mod include_paths;
mod magic_constants;
mod strings;
mod curl;
pub(crate) mod io;
mod mysqli;
mod mysqli_mysql;
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
mod call_counters;
mod callables;
mod system;
mod json;
mod serialize;
mod regressions;
mod objects;
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
mod locals_retype;
mod stack_guard;
mod zval;
