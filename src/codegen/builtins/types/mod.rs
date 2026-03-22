mod boolval;
mod empty;
mod floatval;
mod gettype;
mod is_bool;
mod is_finite;
mod is_float;
mod is_infinite;
mod is_int;
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
        "boolval" => boolval::emit(name, args, emitter, ctx, data),
        "is_null" => is_null::emit(name, args, emitter, ctx, data),
        "floatval" => floatval::emit(name, args, emitter, ctx, data),
        "is_float" => is_float::emit(name, args, emitter, ctx, data),
        "is_int" => is_int::emit(name, args, emitter, ctx, data),
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
