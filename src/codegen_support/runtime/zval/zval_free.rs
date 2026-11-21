//! Purpose:
//! Emits the `__rt_zval_free` and `__rt_zval_free_children` runtime helpers that
//! release PHP-shaped storage owned by a `zval`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::zval`,
//!   and from `__rt_zval_free_array` for each bucket.
//!
//! Key details:
//! - `__rt_zval_free` releases the owned children and then the 16-byte zval block.
//! - `__rt_zval_free_children` releases only the owned children referenced by the
//!   zval value (a `zend_string` for `IS_STRING`, a `zend_array` for `IS_ARRAY`).
//!   The array free path calls it per bucket because bucket slots live inside the
//!   shared data block and must not be freed as standalone blocks.
//! - Object/resource payloads are borrowed elephc pointers and are not freed.

pub use elephc_memoir::interop::zval::emit_zval_free;
