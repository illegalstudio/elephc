//! Purpose:
//! Parser test module wiring for eval fragment syntax coverage.
//! Focused child modules keep expression, statement, namespace, object, and
//! diagnostic cases separate.
//!
//! Called from:
//! - `cargo test -p elephc-eval` through Rust's test harness.
//!
//! Key details:
//! - `support` re-exports parser helpers and EvalIR types for test assertions.
//! - Fixtures intentionally use eval fragments without PHP opening tags.

mod arrays_objects;
mod assignments;
mod calls;
mod class_constants;
mod classes_errors;
mod control_statements;
mod enums;
mod exceptions_control;
mod magic_comments;
mod namespaces;
mod operators;
mod static_members;
mod support;
mod trait_adaptations;
