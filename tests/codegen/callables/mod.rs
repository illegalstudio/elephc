//! Purpose:
//! Groups the callables integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for closures, closure array-literal returns, expr calls, language features, constants and system, state and variadics, argument introspection, and callable strings.

mod callable_strings;
mod closure_array_returns;
mod closures;
mod expr_calls;
mod func_args;
mod language_features;
mod constants_and_system;
mod core_reflection;
mod core_runtime_introspection;
mod state_and_variadics;
mod pipe;
