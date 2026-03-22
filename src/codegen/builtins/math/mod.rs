mod abs;
mod ceil;
mod fdiv;
mod floor;
mod fmod;
mod intdiv;
mod max;
mod min;
mod pow;
mod rand;
mod random_int;
mod round;
mod sqrt;

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
        "abs" => abs::emit(name, args, emitter, ctx, data),
        "floor" => floor::emit(name, args, emitter, ctx, data),
        "ceil" => ceil::emit(name, args, emitter, ctx, data),
        "round" => round::emit(name, args, emitter, ctx, data),
        "sqrt" => sqrt::emit(name, args, emitter, ctx, data),
        "pow" => pow::emit(name, args, emitter, ctx, data),
        "min" => min::emit(name, args, emitter, ctx, data),
        "max" => max::emit(name, args, emitter, ctx, data),
        "intdiv" => intdiv::emit(name, args, emitter, ctx, data),
        "fmod" => fmod::emit(name, args, emitter, ctx, data),
        "fdiv" => fdiv::emit(name, args, emitter, ctx, data),
        "rand" | "mt_rand" => rand::emit(name, args, emitter, ctx, data),
        "random_int" => random_int::emit(name, args, emitter, ctx, data),
        _ => None,
    }
}
