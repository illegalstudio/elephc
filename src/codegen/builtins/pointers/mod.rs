//! Purpose:
//! Dispatches compiler-extension pointer builtins to their focused codegen emitters.
//! Keeps the public builtin category surface small while leaf files own lowering details.
//!
//! Called from:
//! - `crate::codegen::builtins::emit_builtin_call()`.
//!
//! Key details:
//! - Dispatcher names must stay aligned with the builtin catalog and signature normalization layer.

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

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::{can_coerce_result_to_type, coerce_result_to_type};
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
        "ptr_read16" => ptr_read16::emit(name, args, emitter, ctx, data),
        "ptr_read32" => ptr_read32::emit(name, args, emitter, ctx, data),
        "ptr_read_string" => ptr_read_string::emit(name, args, emitter, ctx, data),
        "ptr_set" => ptr_set::emit(name, args, emitter, ctx, data),
        "ptr_write8" => ptr_write8::emit(name, args, emitter, ctx, data),
        "ptr_write16" => ptr_write16::emit(name, args, emitter, ctx, data),
        "ptr_write32" => ptr_write32::emit(name, args, emitter, ctx, data),
        "ptr_write_string" => ptr_write_string::emit(name, args, emitter, ctx, data),
        "ptr_sizeof" => ptr_sizeof::emit(name, args, emitter, ctx, data),
        _ => None,
    }
}

pub(super) fn coerce_current_result_to_int_arg(
    arg: &Expr,
    source_ty: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    if !can_coerce_result_to_type(source_ty, &PhpType::Int) {
        return source_ty.clone();
    }
    if crate::codegen::stmt::helpers::should_release_owned_mixed_after_coerce(
        arg,
        source_ty,
        &PhpType::Int,
    ) {
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));              // preserve the boxed Mixed value so it can be released after integer coercion
        coerce_result_to_type(emitter, ctx, data, source_ty, &PhpType::Int);
        crate::codegen::stmt::helpers::release_preserved_mixed_after_coercion(
            emitter,
            &PhpType::Int,
        );
    } else {
        coerce_result_to_type(emitter, ctx, data, source_ty, &PhpType::Int);
    }
    PhpType::Int
}
