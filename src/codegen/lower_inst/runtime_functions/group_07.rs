//! Purpose:
//! Dispatches one bounded group of typed builtin runtime targets.
//!
//! Called from:
//! - `super::lower()` while lowering typed EIR runtime calls.
//!
//! Key details:
//! - Dispatch is by enum identity, never by PHP function-name strings.
//! - Extracted bodies remain thin calls into target-aware backend emitters.

use crate::codegen::context::FunctionContext;
use crate::codegen::Result;
use crate::ir::{RuntimeFnId, Instruction};

/// Lowers a target owned by bounded dispatch group 07, or returns `None`.
pub(super) fn lower(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: RuntimeFnId,
) -> Option<Result<()>> {
    match target {
        RuntimeFnId::Touch => Some({
            crate::codegen::lower_inst::builtins::io::lower_touch(ctx, inst)
        }),
        RuntimeFnId::Umask => Some({
            crate::codegen::lower_inst::builtins::io::lower_umask(ctx, inst)
        }),
        RuntimeFnId::Unlink => Some({
            crate::codegen::lower_inst::builtins::io::lower_unlink(ctx, inst)
        }),
        RuntimeFnId::VarDump => Some({
            crate::codegen::lower_inst::builtins::debug::lower_var_dump(ctx, inst)
        }),
        RuntimeFnId::Vfprintf => Some({
            crate::codegen::lower_inst::builtins::io::lower_vfprintf(ctx, inst)
        }),
        RuntimeFnId::Abs => Some({
            crate::codegen::lower_inst::builtins::math::lower_abs(ctx, inst)
        }),
        RuntimeFnId::Acos => Some({
            crate::codegen::lower_inst::builtins::math::lower_unary_libm(ctx, inst, "acos")
        }),
        RuntimeFnId::Asin => Some({
            crate::codegen::lower_inst::builtins::math::lower_unary_libm(ctx, inst, "asin")
        }),
        RuntimeFnId::Atan => Some({
            crate::codegen::lower_inst::builtins::math::lower_unary_libm(ctx, inst, "atan")
        }),
        RuntimeFnId::Atan2 => Some({
            crate::codegen::lower_inst::builtins::math::lower_atan2(ctx, inst)
        }),
        RuntimeFnId::Ceil => Some({
            crate::codegen::lower_inst::builtins::math::lower_ceil(ctx, inst)
        }),
        RuntimeFnId::Clamp => Some({
            crate::codegen::lower_inst::builtins::math::lower_clamp(ctx, inst)
        }),
        RuntimeFnId::Cos => Some({
            crate::codegen::lower_inst::builtins::math::lower_unary_libm(ctx, inst, "cos")
        }),
        RuntimeFnId::Cosh => Some({
            crate::codegen::lower_inst::builtins::math::lower_unary_libm(ctx, inst, "cosh")
        }),
        RuntimeFnId::Deg2rad => Some({
            crate::codegen::lower_inst::builtins::math::lower_deg2rad(ctx, inst)
        }),
        RuntimeFnId::Exp => Some({
            crate::codegen::lower_inst::builtins::math::lower_unary_libm(ctx, inst, "exp")
        }),
        RuntimeFnId::Fdiv => Some({
            crate::codegen::lower_inst::builtins::math::lower_fdiv(ctx, inst)
        }),
        RuntimeFnId::Floor => Some({
            crate::codegen::lower_inst::builtins::math::lower_floor(ctx, inst)
        }),
        RuntimeFnId::Fmod => Some({
            crate::codegen::lower_inst::builtins::math::lower_fmod(ctx, inst)
        }),
        RuntimeFnId::Hypot => Some({
            crate::codegen::lower_inst::builtins::math::lower_hypot(ctx, inst)
        }),
        RuntimeFnId::Intdiv => Some({
            crate::codegen::lower_inst::builtins::math::lower_intdiv(ctx, inst)
        }),
        RuntimeFnId::Log => Some({
            crate::codegen::lower_inst::builtins::math::lower_log(ctx, inst)
        }),
        RuntimeFnId::Log10 => Some({
            crate::codegen::lower_inst::builtins::math::lower_unary_libm(ctx, inst, "log10")
        }),
        RuntimeFnId::Log2 => Some({
            crate::codegen::lower_inst::builtins::math::lower_unary_libm(ctx, inst, "log2")
        }),
        RuntimeFnId::Max => Some({
            crate::codegen::lower_inst::builtins::math::lower_min_max(ctx, inst, true)
        }),
        RuntimeFnId::Min => Some({
            crate::codegen::lower_inst::builtins::math::lower_min_max(ctx, inst, false)
        }),
        RuntimeFnId::MtRand => Some({
            crate::codegen::lower_inst::builtins::math::lower_rand(ctx, inst, "mt_rand")
        }),
        RuntimeFnId::Pi => Some({
            crate::codegen::lower_inst::builtins::math::lower_pi(ctx, inst)
        }),
        RuntimeFnId::Pow => Some({
            crate::codegen::lower_inst::builtins::math::lower_pow(ctx, inst)
        }),
        RuntimeFnId::Rad2deg => Some({
            crate::codegen::lower_inst::builtins::math::lower_rad2deg(ctx, inst)
        }),
        RuntimeFnId::Rand => Some({
            crate::codegen::lower_inst::builtins::math::lower_rand(ctx, inst, "rand")
        }),
        RuntimeFnId::RandomInt => Some({
            crate::codegen::lower_inst::builtins::math::lower_random_int(ctx, inst)
        }),
        RuntimeFnId::Round => Some({
            crate::codegen::lower_inst::builtins::math::lower_round(ctx, inst)
        }),
        RuntimeFnId::Sin => Some({
            crate::codegen::lower_inst::builtins::math::lower_unary_libm(ctx, inst, "sin")
        }),
        RuntimeFnId::Sinh => Some({
            crate::codegen::lower_inst::builtins::math::lower_unary_libm(ctx, inst, "sinh")
        }),
        _ => None,
    }
}
