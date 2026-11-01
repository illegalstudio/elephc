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
mod curl;
mod io;
pub(crate) mod mbstring;
mod mbstring_callback;
mod string;
mod math;
pub(crate) mod gmp;
mod spl;
mod pointers;
mod system;
pub(crate) use array::array_column::rows_need_generic_helper as array_column_rows_need_generic_helper;
pub(crate) use array::array_merge::indexed_operands_result_type as array_merge_indexed_result_type;
pub(crate) use system::constant::literal_class_constant_expr;
mod types;
pub(crate) mod xml;
#[cfg(test)]
mod parity_tests;
