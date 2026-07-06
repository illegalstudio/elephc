//! Purpose:
//! Groups statement storage helpers for locals and external globals.
//! Provides type-directed load/store helpers used by assignment and IO paths.
//!
//! Called from:
//! - `crate::codegen_support::stmt` and assignment emitters
//!
//! Key details:
//! - Storage helpers must match ABI value layout and static symbol ownership conventions.

mod extern_globals;
mod locals;

use super::super::context::Context;
use super::super::emit::Emitter;
use super::PhpType;

/// Emits the store for a static variable into its data-section symbol.
/// The variable is identified by `name` and laid out according to `ty`.
pub(super) fn emit_static_store(
    emitter: &mut Emitter,
    ctx: &Context,
    name: &str,
    ty: &PhpType,
) {
    locals::emit_static_store(emitter, ctx, name, ty)
}

/// Emits the store for a global variable into its data-section symbol.
/// The variable is identified by `name` and laid out according to `ty`.
pub(super) fn emit_global_store(
    emitter: &mut Emitter,
    ctx: &mut Context,
    name: &str,
    ty: &PhpType,
) {
    locals::emit_global_store(emitter, ctx, name, ty)
}

/// Emits the load for a global variable from its data-section symbol.
pub(super) fn emit_global_load(
    emitter: &mut Emitter,
    ctx: &mut Context,
    name: &str,
    ty: &PhpType,
) {
    if ctx.extern_globals.contains_key(name) {
        extern_globals::emit_extern_global_load(emitter, name, ty);
        return;
    }
    locals::emit_global_load(emitter, name, ty)
}

/// Emits the store for an extern (FFI) global variable.
pub(super) fn emit_extern_global_store(emitter: &mut Emitter, name: &str, ty: &PhpType) {
    extern_globals::emit_extern_global_store(emitter, name, ty)
}
