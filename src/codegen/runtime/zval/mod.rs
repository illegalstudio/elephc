//! Purpose:
//! Collects runtime emitters for the PHP `zval` bridge extension.
//! Owns re-export wiring for the pack/unpack routines that convert elephc
//! runtime values (boxed `Mixed` cells, indexed/hash arrays, strings) into
//! PHP `zval`/`zend_string`/`zend_array` structures and back.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` during the zval runtime section.
//!
//! Key details:
//! - `zval_pack` consumes a boxed `Mixed` cell pointer and produces a 16-byte
//!   `zval` heap block; `zval_unpack` reverses it.
//! - String/array children are freshly allocated through `__rt_heap_alloc` so
//!   the produced `zval` owns independent PHP-shaped storage.
//! - All routines emit both ARM64 and x86_64 variants gated on `emitter.target.arch`.

mod djbx33a;
mod zval_free;
mod zval_free_array;
mod zval_pack;
mod zval_pack_array_hash;
mod zval_pack_array_packed;
mod zval_string_new;
mod zval_type;
mod zval_unpack;
mod zval_unpack_array;

pub(crate) use djbx33a::emit_zval_djbx33a;
pub(crate) use zval_free::emit_zval_free;
pub(crate) use zval_free_array::emit_zval_free_array;
pub(crate) use zval_pack::emit_zval_pack;
pub(crate) use zval_pack_array_hash::emit_zval_pack_array_hash;
pub(crate) use zval_pack_array_packed::emit_zval_pack_array_packed;
pub(crate) use zval_string_new::emit_zval_string_new;
pub(crate) use zval_type::emit_zval_type;
pub(crate) use zval_unpack::emit_zval_unpack;
pub(crate) use zval_unpack_array::emit_zval_unpack_array;
