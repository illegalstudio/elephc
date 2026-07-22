//! Purpose:
//! Single source of truth for PHP builtin functions: each builtin is declared
//! once via `builtin!` and collected through `inventory` into a lazy registry
//! that drives the catalog, signatures, type-check, lowering dispatch, and docs.
//!
//! Called from:
//! - Checker, optimizer, EIR lowering, callable wrappers, and `gen_builtins`.
//!
//! Key details:
//! - Homes live under `<area>/<name>.rs` and select backend-neutral EIR semantics.
//! - Backend dispatch consumes typed runtime targets and never PHP function names.

#[macro_use]
mod macros;
pub mod semantics;
pub mod spec;
pub mod registry;
pub mod docs;
mod convert;
mod requirements;
mod array;
mod callables;
mod io;
mod string;
mod math;
mod spl;
mod pointers;
mod system;
mod types;
#[cfg(test)]
mod parity_tests;
