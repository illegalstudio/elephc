//! Purpose:
//! Collects runtime emitters for the compiler pointer extension.
//! The module owns re-export wiring for pointer formatting, null checks, and C-string conversion helpers.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` during the pointer runtime section.
//!
//! Key details:
//! - Pointer helpers must keep null checks and C-string conversions aligned with the pointer extension ABI.

mod cstr_to_str;
mod ptoa;
mod ptr_check_nonnull;
mod ptr_read_string;
mod ptr_write_string;
mod str_to_cstr;

pub(crate) use cstr_to_str::emit_cstr_to_str;
pub(crate) use ptoa::emit_ptoa;
pub(crate) use ptr_check_nonnull::emit_ptr_check_nonnull;
pub(crate) use ptr_read_string::emit_ptr_read_string;
pub(crate) use ptr_write_string::emit_ptr_write_string;
pub(crate) use str_to_cstr::emit_str_to_cstr;
