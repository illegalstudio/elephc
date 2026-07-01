//! Purpose:
//! Single source of truth for PHP builtin functions: each builtin is declared
//! once via `builtin!` and collected through `inventory` into a lazy registry
//! that drives the catalog, signatures, type-check, lowering dispatch, and docs.
//!
//! Called from:
//! - `crate::types::checker::builtins`, `crate::types::signatures`,
//!   `crate::codegen_ir::lower_inst::builtins`, and `gen_builtins` (doc export).
//!
//! Key details:
//! - Homes live under `<area>/<name>.rs`; the legacy dispatch points fall back to
//!   their old paths until every area has migrated.

#[macro_use]
mod macros;
pub mod spec;
pub mod registry;
mod convert;
mod array;
mod io;
mod string;
mod math;
mod spl;
mod pointers;
mod system;
#[cfg(test)]
mod parity_tests;
