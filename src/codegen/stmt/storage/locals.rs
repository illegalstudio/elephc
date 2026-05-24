//! Purpose:
//! Provides local slot storage helpers with optional static backing symbols.
//! Offers a typed storage interface to assignment and expression statement paths.
//!
//! Called from:
//! - `crate::codegen::stmt::storage`
//!
//! Key details:
//! - Loads and stores must use ABI value sizes and preserve refcounted ownership conventions.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::emit::Emitter;
use crate::types::PhpType;

use super::super::static_storage_label;

/// Stores the result register value into a statically-allocated symbol.
/// Uses the result register (x0 for integers, d0 for floats, x1:x2 for strings).
/// The static symbol persists across function invocations.
pub(super) fn emit_static_store(
    emitter: &mut Emitter,
    ctx: &Context,
    name: &str,
    ty: &PhpType,
) {
    let data_label = static_storage_label(ctx, name);
    emitter.comment(&format!("store to static ${}", name));
    abi::emit_store_result_to_symbol(emitter, &data_label, ty, true);
}

/// Stores the result register value into a global variable symbol.
/// Uses the result register (x0 for integers, d0 for floats, x1:x2 for strings).
/// Global variables are per-program and persist for the lifetime of the process.
pub(super) fn emit_global_store(
    emitter: &mut Emitter,
    _ctx: &mut Context,
    name: &str,
    ty: &PhpType,
) {
    let label = format!("_gvar_{}", name);
    emitter.comment(&format!("store to global ${}", name));
    abi::emit_store_result_to_symbol(emitter, &label, ty, true);
}

/// Loads a value from a global variable symbol into the result register.
/// Places integers in x0, floats in d0, strings in x1:x2.
/// Returns the loaded value in the standard result register(s) for the type.
pub(super) fn emit_global_load(emitter: &mut Emitter, name: &str, ty: &PhpType) {
    let label = format!("_gvar_{}", name);
    emitter.comment(&format!("load from global ${}", name));
    abi::emit_load_symbol_to_result(emitter, &label, ty);
}
