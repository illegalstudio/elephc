use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::emit::Emitter;
use crate::types::PhpType;

use super::super::static_storage_label;

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

pub(super) fn emit_global_load(emitter: &mut Emitter, name: &str, ty: &PhpType) {
    let label = format!("_gvar_{}", name);
    emitter.comment(&format!("load from global ${}", name));
    abi::emit_load_symbol_to_result(emitter, &label, ty);
}
