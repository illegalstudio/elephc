mod extern_globals;
mod locals;

use super::super::context::Context;
use super::super::emit::Emitter;
use super::PhpType;

pub(super) fn emit_static_store(
    emitter: &mut Emitter,
    ctx: &Context,
    name: &str,
    ty: &PhpType,
) {
    locals::emit_static_store(emitter, ctx, name, ty)
}

pub(super) fn emit_global_store(
    emitter: &mut Emitter,
    ctx: &mut Context,
    name: &str,
    ty: &PhpType,
) {
    locals::emit_global_store(emitter, ctx, name, ty)
}

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

pub(super) fn emit_extern_global_store(emitter: &mut Emitter, name: &str, ty: &PhpType) {
    extern_globals::emit_extern_global_store(emitter, name, ty)
}
