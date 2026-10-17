//! Purpose:
//! Dispatches the bounded BCMath runtime-function group to its target-aware lowerers.
//!
//! Called from:
//! - `super::lower()` while lowering typed EIR runtime calls.
//!
//! Key details:
//! - All fourteen PHP BCMath identities stay grouped and never dispatch by source name.

use crate::codegen::context::FunctionContext;
use crate::codegen::Result;
use crate::ir::{Instruction, RuntimeFnId};

/// Lowers one BCMath target, or returns `None` for another runtime-function group.
pub(super) fn lower(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: RuntimeFnId,
) -> Option<Result<()>> {
    use crate::codegen::lower_inst::builtins::bcmath;
    match target {
        RuntimeFnId::BcNumberAdd => Some(bcmath::lower_bcnumber_add(ctx, inst)),
        RuntimeFnId::BcNumberCeil => Some(bcmath::lower_bcnumber_ceil(ctx, inst)),
        RuntimeFnId::BcNumberComp => Some(bcmath::lower_bcnumber_comp(ctx, inst)),
        RuntimeFnId::BcNumberCreate => Some(bcmath::lower_bcnumber_create(ctx, inst)),
        RuntimeFnId::BcNumberDiv => Some(bcmath::lower_bcnumber_div(ctx, inst)),
        RuntimeFnId::BcNumberDivmod => Some(bcmath::lower_bcnumber_divmod(ctx, inst)),
        RuntimeFnId::BcNumberFloor => Some(bcmath::lower_bcnumber_floor(ctx, inst)),
        RuntimeFnId::BcNumberMod => Some(bcmath::lower_bcnumber_mod(ctx, inst)),
        RuntimeFnId::BcNumberMul => Some(bcmath::lower_bcnumber_mul(ctx, inst)),
        RuntimeFnId::BcNumberPow => Some(bcmath::lower_bcnumber_pow(ctx, inst)),
        RuntimeFnId::BcNumberPowmod => Some(bcmath::lower_bcnumber_powmod(ctx, inst)),
        RuntimeFnId::BcNumberRound => Some(bcmath::lower_bcnumber_round(ctx, inst)),
        RuntimeFnId::BcNumberSqrt => Some(bcmath::lower_bcnumber_sqrt(ctx, inst)),
        RuntimeFnId::BcNumberSub => Some(bcmath::lower_bcnumber_sub(ctx, inst)),

        RuntimeFnId::BcAdd => Some(bcmath::lower_bcadd(ctx, inst)),
        RuntimeFnId::BcCeil => Some(bcmath::lower_bcceil(ctx, inst)),
        RuntimeFnId::BcComp => Some(bcmath::lower_bccomp(ctx, inst)),
        RuntimeFnId::BcDiv => Some(bcmath::lower_bcdiv(ctx, inst)),
        RuntimeFnId::BcDivmod => Some(bcmath::lower_bcdivmod(ctx, inst)),
        RuntimeFnId::BcFloor => Some(bcmath::lower_bcfloor(ctx, inst)),
        RuntimeFnId::BcMod => Some(bcmath::lower_bcmod(ctx, inst)),
        RuntimeFnId::BcMul => Some(bcmath::lower_bcmul(ctx, inst)),
        RuntimeFnId::BcPow => Some(bcmath::lower_bcpow(ctx, inst)),
        RuntimeFnId::BcPowmod => Some(bcmath::lower_bcpowmod(ctx, inst)),
        RuntimeFnId::BcRound => Some(bcmath::lower_bcround(ctx, inst)),
        RuntimeFnId::BcScale => Some(bcmath::lower_bcscale(ctx, inst)),
        RuntimeFnId::BcSqrt => Some(bcmath::lower_bcsqrt(ctx, inst)),
        RuntimeFnId::BcSub => Some(bcmath::lower_bcsub(ctx, inst)),
        _ => None,
    }
}
