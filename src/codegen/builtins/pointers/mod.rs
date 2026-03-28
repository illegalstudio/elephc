mod ptr;
mod ptr_get;
mod ptr_is_null;
mod ptr_null;
mod ptr_offset;
mod ptr_read32;
mod ptr_read8;
mod ptr_set;
mod ptr_sizeof;
mod ptr_write32;
mod ptr_write8;

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    match name {
        "ptr" => ptr::emit(name, args, emitter, ctx, data),
        "ptr_null" => ptr_null::emit(name, args, emitter, ctx, data),
        "ptr_is_null" => ptr_is_null::emit(name, args, emitter, ctx, data),
        "ptr_offset" => ptr_offset::emit(name, args, emitter, ctx, data),
        "ptr_get" => ptr_get::emit(name, args, emitter, ctx, data),
        "ptr_read8" => ptr_read8::emit(name, args, emitter, ctx, data),
        "ptr_read32" => ptr_read32::emit(name, args, emitter, ctx, data),
        "ptr_set" => ptr_set::emit(name, args, emitter, ctx, data),
        "ptr_write8" => ptr_write8::emit(name, args, emitter, ctx, data),
        "ptr_write32" => ptr_write32::emit(name, args, emitter, ctx, data),
        "ptr_sizeof" => ptr_sizeof::emit(name, args, emitter, ctx, data),
        _ => None,
    }
}
