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
mod buffer_ptr;
mod buffer_arrays;
mod buffer_merges;
mod buffer_record_copy;
mod static_buffer_properties;
mod packed_nested_fields;
mod packed_field_narrowing;
mod data_widths;
mod generics;
mod generic_native_storage;
mod generics_review;
mod generics_constructors;
mod generics_named_spreads;
mod generics_docblock_methods;
mod generics_docblock_enums;
mod preprocessor;
mod namespaces;
mod null_sentinel;
mod case_insensitive_symbols;
mod cli;
mod repl;
mod strict_php;
mod testing;
mod testing_command;
mod testing_generic_functions;
mod testing_generics;
mod strict_php_source_guards;
mod lfc;
mod benchmarks;
mod echo_vars;
mod eval;
mod eval_generics;
mod symbol_catalog;
mod core_classes;
mod eval_builtin_parity;
mod eval_callable_ref_errors;
mod eval_callables;
mod eval_callback_receiver_ownership;
mod eval_closures;
mod eval_constructors;
mod eval_reflection_invocation;
mod eval_global_scope;
mod eval_escape_guard;
mod eval_scope_access_mask;
mod eval_native_catalog;
mod eval_undefined_functions;
mod operators;
mod control_flow;
mod scalar_strings;
mod array_basics;
mod float_sort;
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
mod pcntl;
mod xml;
mod tokenizer;
mod arrays;
mod calendar;
mod call_counters;
mod callables;
mod eval_deferred_catalog;
mod known_closure_loops;
mod system;
mod json;
mod serialize;
mod regressions;
mod objects;
mod destructors;
mod destructor_unwind_calls;
mod reference_property_payload;
mod references;
mod runtime_gc;
mod runtime_reachability;
mod runtime_introspection;
mod math;
mod gmp_phpt;
mod gmp_eval;
mod gmp_class;
mod gmp_math;
mod random;
mod misc;
mod pointers;
mod pointer_strings;
mod nullable_pointers;
mod pointer_arrays;
mod ffi;
mod oop;
mod static_class_features;
mod types;
mod optimizer;
mod preg_replace_callback_throw;
mod iterators;
mod dom_html;
mod spl;
mod generators;
mod dead_strip;
mod locals_retype;
mod stack_guard;
mod zval;

mod fileinfo;
mod fileinfo_autoload;
mod fileinfo_reflection;
mod fileinfo_validation;
mod fileinfo_stream_diagnostics;
mod fileinfo_wrapper_stat;
mod fileinfo_php_src;
mod fileinfo_stream_magic;
