//! Purpose:
//! Emits the `__rt_zval_free_array` runtime helper that releases a PHP
//! `zend_array` (HashTable), its element tree, and its data block.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::zval`,
//!   and from `__rt_zval_free_children` when freeing nested arrays.
//!
//! Key details:
//! - Walks the element tree (packed or hash) via `__rt_zval_free_children`,
//!   frees each bucket's `zend_string` key when present, then frees the data
//!   block (base = `arData + (nTableMask << 2)`, which covers both the packed
//!   sentinel prefix and the hash index prefix) and the HashTable itself.
//! - The empty-hash placeholder has `arData = NULL`; the element walk is then
//!   skipped and only the HashTable structure is freed.
//! - Emits both ARM64 and x86_64 variants gated on `emitter.target.arch`.

pub use elephc_memoir::interop::zval::emit_zval_free_array;
