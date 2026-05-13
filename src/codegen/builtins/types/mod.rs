//! Purpose:
//! Dispatches type predicate, conversion, and variable-state PHP builtins to their focused codegen emitters.
//! Keeps the public builtin category surface small while leaf files own lowering details.
//!
//! Called from:
//! - `crate::codegen::builtins::emit_builtin_call()`.
//!
//! Key details:
//! - Dispatcher names must stay aligned with the builtin catalog and signature normalization layer.

mod boolval;
mod empty;
mod floatval;
mod gettype;
mod is_bool;
mod is_callable;
mod is_finite;
mod is_float;
mod is_infinite;
mod is_int;
mod is_iterable;
mod is_nan;
mod is_null;
mod is_numeric;
mod is_string;
mod settype;
mod unset;

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
        "is_bool" => is_bool::emit(name, args, emitter, ctx, data),
        "is_callable" => is_callable::emit(name, args, emitter, ctx, data),
        "boolval" => boolval::emit(name, args, emitter, ctx, data),
        "is_null" => is_null::emit(name, args, emitter, ctx, data),
        "floatval" => floatval::emit(name, args, emitter, ctx, data),
        "is_float" => is_float::emit(name, args, emitter, ctx, data),
        "is_int" => is_int::emit(name, args, emitter, ctx, data),
        "is_iterable" => is_iterable::emit(name, args, emitter, ctx, data),
        "is_string" => is_string::emit(name, args, emitter, ctx, data),
        "is_numeric" => is_numeric::emit(name, args, emitter, ctx, data),
        "is_nan" => is_nan::emit(name, args, emitter, ctx, data),
        "is_infinite" => is_infinite::emit(name, args, emitter, ctx, data),
        "is_finite" => is_finite::emit(name, args, emitter, ctx, data),
        "gettype" => gettype::emit(name, args, emitter, ctx, data),
        "empty" => empty::emit(name, args, emitter, ctx, data),
        "unset" => unset::emit(name, args, emitter, ctx, data),
        "settype" => settype::emit(name, args, emitter, ctx, data),
        _ => None,
    }
}
