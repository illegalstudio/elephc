//! Purpose:
//! Groups the callables integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for closures, closure array-literal returns, expr calls, language features, constants and system, state and variadics, argument introspection, callable strings, compiler-generated local visibility, callable dispatch branch reach, shared boxed-callable selectors, methods callable only from their class scope, where descriptor thunks land relative to their caller, and untyped parameters that receive a closure beside another kind of value.

mod boxed_calls;
mod shared_boxed_dispatch;
mod shared_selectors;
mod branch_reach;
mod callable_strings;
mod declared_callable_values;
mod scope_method_callables;
mod closure_array_returns;
mod closure_call_returns;
mod closure_binding_scopes;
mod closures;
mod dynamic_first_class;
mod closure_factory_forms;
mod property_callable_metadata;
mod expr_calls;
mod func_args;
mod language_features;
mod constants_and_system;
mod core_reflection;
mod core_hook_introspection;
mod core_spread_regressions;
mod core_error_regressions;
mod core_warning_fragments;
mod core_reflection_regressions;
mod core_native_subclasses;
mod core_inventory_regressions;
mod core_backtrace_regressions;
mod core_generated_local_visibility;
mod core_runtime_introspection;
mod state_and_variadics;
mod thunk_layout;
mod pipe;
mod untyped_closure_params;
