//! Purpose:
//! Process spawning and wait bookkeeping behind PHP's `proc_open()` family.
//!
//! Called from:
//! - Compiled programs through the `elephc_proc` bridge (`src/proc_prelude.rs`).
//! - Magician's `proc_*` builtins through the crate's rlib form.
//!
//! Key details:
//! - `spawn` starts a child with PHP's descriptor/cwd/env rules; `table` answers
//!   `proc_get_status()`, `proc_close()`, and `proc_terminate()` with php-src's wait semantics.
//! - PHP values, resources, and arrays stay with the caller: only integers, byte strings, and raw
//!   descriptors cross this crate's boundary.

pub mod abi;
pub mod spawn;
pub mod table;

pub use abi::*;

#[cfg(test)]
mod tests;
