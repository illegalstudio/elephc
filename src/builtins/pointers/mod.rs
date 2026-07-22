//! Purpose:
//! Groups all `pointers`-area builtin homes into this module so the registry can
//! collect them in one place. Each submodule declares exactly one builtin via
//! `builtin!` and provides its checker contract and typed runtime target.
//!
//! Called from:
//! - `crate::builtins` (`mod pointers;` in `src/builtins/mod.rs`).
//!
//! Key details:
//! - Pointer builtins require a `check` hook because they validate
//!   pointer/argument types at compile time and some return `PhpType::Pointer(None)`
//!   which `TypeSpec` cannot express statically.
//! - Add `pub mod <name>;` here for every new pointer builtin home.

pub mod __elephc_ptr_is_null;
pub mod __elephc_ptr_read_string;
pub mod __elephc_ptr_write_string;
pub mod buffer_free;
pub mod buffer_len;
pub mod ptr;
pub mod ptr_get;
pub mod ptr_is_null;
pub mod ptr_null;
pub mod ptr_offset;
pub mod ptr_read16;
pub mod ptr_read32;
pub mod ptr_read8;
pub mod ptr_read_string;
pub mod ptr_set;
pub mod ptr_sizeof;
pub mod ptr_write16;
pub mod ptr_write32;
pub mod ptr_write8;
pub mod ptr_write_string;
pub mod zval_free;
pub mod zval_pack;
pub mod zval_type;
pub mod zval_unpack;
