//! Purpose:
//! Groups the object property access integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for nullsafe property and method access, mutations, deep chains, null-capable int property storage, weak-mode typed writes from runtime `mixed` values, properties passed as by-reference arguments to mutating array builtins, and php's scope-dependent answer to one property name on a read.

use super::*;

mod by_ref_builtin_args;
mod nullsafe;
mod nullsafe_side_effects;
mod mutations;
mod deep_chains;
mod nullable_int_defaults;
mod scope_visibility;
mod weak_typed_writes;
