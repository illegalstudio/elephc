//! Purpose:
//! Groups eval raw pointer and buffer extension builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins` module loading.
//!
//! Key details:
//! - Leaf files register metadata through `eval_builtin!` and own their
//!   PHP-visible direct/by-value wrappers and implementation code.
//! - Helper reuse stays between builtin files with `ptr` owning raw address
//!   conversions, `ptr_get` owning read widths, and `ptr_set` owning write widths.

mod buffer_free;
mod buffer_len;
mod buffer_new;
mod ptr;
mod ptr_get;
mod ptr_is_null;
mod ptr_null;
mod ptr_offset;
mod ptr_read16;
mod ptr_read32;
mod ptr_read8;
mod ptr_read_string;
mod ptr_set;
mod ptr_sizeof;
mod ptr_write16;
mod ptr_write32;
mod ptr_write8;
mod ptr_write_string;

pub(in crate::interpreter) use buffer_free::*;
pub(in crate::interpreter) use buffer_len::*;
pub(in crate::interpreter) use buffer_new::*;
pub(in crate::interpreter) use ptr::*;
pub(in crate::interpreter) use ptr_get::*;
pub(in crate::interpreter) use ptr_is_null::*;
pub(in crate::interpreter) use ptr_null::*;
pub(in crate::interpreter) use ptr_offset::*;
pub(in crate::interpreter) use ptr_read16::*;
pub(in crate::interpreter) use ptr_read32::*;
pub(in crate::interpreter) use ptr_read8::*;
pub(in crate::interpreter) use ptr_read_string::*;
pub(in crate::interpreter) use ptr_set::*;
pub(in crate::interpreter) use ptr_sizeof::*;
pub(in crate::interpreter) use ptr_write16::*;
pub(in crate::interpreter) use ptr_write32::*;
pub(in crate::interpreter) use ptr_write8::*;
pub(in crate::interpreter) use ptr_write_string::*;
