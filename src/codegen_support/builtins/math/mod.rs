//! Purpose:
//! Dispatches numeric PHP builtins and libm-backed operations to their focused codegen emitters.
//! Keeps the public builtin category surface small while leaf files own lowering details.
//!
//! Called from:
//! - `crate::codegen_support::builtins::emit_builtin_call()`.
//!
//! Key details:
//! - Dispatcher names must stay aligned with the builtin catalog and signature normalization layer.

mod abs;
mod acos;
mod asin;
mod atan;
mod atan2;
mod ceil;
mod clamp;
mod cos;
mod cosh;
mod deg2rad;
mod exp;
mod fdiv;
mod floor;
mod fmod;
mod hypot;
mod intdiv;
mod log;
mod log10;
mod log2;
mod max;
mod min;
mod pi;
mod pow;
mod rad2deg;
mod rand;
mod random_int;
mod round;
mod sin;
mod sinh;
mod sqrt;
mod tan;
mod tanh;

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Dispatches to the appropriate math builtin emitter based on `name`.
///
/// Returns `Some(PhpType)` if `name` matches a known math builtin, or `None` if
/// the builtin is not recognized. Callers must have already validated argument
/// count and types via the type checker.
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
        "clamp" => clamp::emit(name, args, emitter, ctx, data),
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
        "sin" => sin::emit(name, args, emitter, ctx, data),
        "cos" => cos::emit(name, args, emitter, ctx, data),
        "tan" => tan::emit(name, args, emitter, ctx, data),
        "asin" => asin::emit(name, args, emitter, ctx, data),
        "acos" => acos::emit(name, args, emitter, ctx, data),
        "atan" => atan::emit(name, args, emitter, ctx, data),
        "atan2" => atan2::emit(name, args, emitter, ctx, data),
        "sinh" => sinh::emit(name, args, emitter, ctx, data),
        "cosh" => cosh::emit(name, args, emitter, ctx, data),
        "tanh" => tanh::emit(name, args, emitter, ctx, data),
        "log" => log::emit(name, args, emitter, ctx, data),
        "log2" => log2::emit(name, args, emitter, ctx, data),
        "log10" => log10::emit(name, args, emitter, ctx, data),
        "exp" => exp::emit(name, args, emitter, ctx, data),
        "hypot" => hypot::emit(name, args, emitter, ctx, data),
        "pi" => pi::emit(name, args, emitter, ctx, data),
        "deg2rad" => deg2rad::emit(name, args, emitter, ctx, data),
        "rad2deg" => rad2deg::emit(name, args, emitter, ctx, data),
        _ => None,
    }
}
