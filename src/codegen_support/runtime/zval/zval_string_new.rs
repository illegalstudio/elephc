//! Purpose:
//! Emits the `__rt_zval_string_new` runtime helper that builds a PHP
//! `zend_string` structure from an elephc string (pointer + length).
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::zval`,
//!   and directly from `__rt_zval_pack` for the string kind.
//!
//! Key details:
//! - `zend_string` layout: `refcount`(u32)@0, `gc.type_info`(u32)@4, `h`(u64)@8,
//!   `len`(u64)@16, `val[]`@24, plus a NUL terminator at `24 + len`.
//! - The bytes are copied from the elephc source so the new `zend_string` owns
//!   independent storage (the elephc source is not consumed).
//! - Register convention matches `__rt_str_persist` per target so callers can
//!   stage the pointer/length pair once.

pub use elephc_memoir::interop::zval::emit_zval_string_new;
