//! Purpose:
//! Lowers checked integer arithmetic whose only observable result is a PHP `int`.
//! Keeps the in-range path scalar and STOPS on an overflow it cannot represent.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` for `IChecked*ToInt` opcodes.
//!
//! Key details:
//! - These opcodes exist because `checked_int_sink` proved every observation of the value is a
//!   PHP `int`, which is what lets the result live in a raw slot with no Mixed cell.
//! - That proof is exactly what an overflow breaks. php promotes to float; a raw slot cannot
//!   hold one, and converting the true double back to an int printed
//!   `int(-9223372036854775808)` where php prints `float(9.223372036854776E+18)` — measured on
//!   `for ($i = PHP_INT_MAX - 1; …; $i++)`. So the overflow path reports and exits instead.
//! - The alternative was measured and rejected: boxing every incremented int local fixes the
//!   promotion and costs 4.7x on a 20M-iteration loop, because the boxed slot also turns the
//!   loop's `icmp` into `php_rel_cmp` and its `ichecked_add` into `mixed_numeric_binop`.
//! - The in-range path is untouched: the overflow branch already existed to compute the double.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen::Result;
use crate::ir::{Immediate, Instruction};

use super::super::context::FunctionContext;
use super::{arithmetic::load_integer_operand, expect_operand, store_if_result};

/// Checked arithmetic operation selected by the typed EIR opcode.
#[derive(Clone, Copy)]
pub(super) enum CheckedIntOp {
    Add,
    Sub,
    Mul,
}

impl CheckedIntOp {
    /// Returns the target mnemonic for the overflow-to-double slow path.
    fn float_mnemonic(self, arch: Arch) -> &'static str {
        match (self, arch) {
            (Self::Add, Arch::AArch64) => "fadd",
            (Self::Sub, Arch::AArch64) => "fsub",
            (Self::Mul, Arch::AArch64) => "fmul",
            (Self::Add, Arch::X86_64) => "addsd",
            (Self::Sub, Arch::X86_64) => "subsd",
            (Self::Mul, Arch::X86_64) => "mulsd",
        }
    }
}

/// Lowers checked add/sub/mul directly to the PHP integer observed by the sink.
pub(super) fn lower_checked_int_to_int(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    op: CheckedIntOp,
) -> Result<()> {
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let rhs_reg = abi::secondary_scratch_reg(ctx.emitter);
    let saved_lhs_reg = abi::tertiary_scratch_reg(ctx.emitter);
    load_integer_operand(ctx, lhs, result_reg, inst)?;
    load_integer_operand(ctx, rhs, rhs_reg, inst)?;
    let overflow_label = ctx.next_label("checked_int_to_int_overflow");
    let done_label = ctx.next_label("checked_int_to_int_done");

    match ctx.emitter.target.arch {
        Arch::AArch64 => emit_aarch64_checked(
            ctx,
            op,
            result_reg,
            rhs_reg,
            saved_lhs_reg,
            &overflow_label,
            &done_label,
        ),
        Arch::X86_64 => emit_x86_64_checked(
            ctx,
            op,
            result_reg,
            rhs_reg,
            saved_lhs_reg,
            &overflow_label,
            &done_label,
        ),
    }

    ctx.emitter.label(&overflow_label);
    // `Immediate::Bool(true)` means an explicit `(int)` cast observed this value, and php's own
    // answer there is the promoted double converted back to an int. Anything else means the
    // narrowing came from a raw int SLOT, where php would have kept the float — so the program
    // stops rather than print a number php never produces.
    if matches!(inst.immediate, Some(Immediate::Bool(true))) {
        emit_overflow_conversion(ctx, op, saved_lhs_reg, rhs_reg);
        abi::emit_php_float_to_int(ctx.emitter, result_reg);
    } else {
        emit_int_overflow_fatal(ctx);
    }
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Emits the AArch64 scalar fast path and branches to the shared overflow path.
fn emit_aarch64_checked(
    ctx: &mut FunctionContext<'_>,
    op: CheckedIntOp,
    result_reg: &str,
    rhs_reg: &str,
    saved_lhs_reg: &str,
    overflow_label: &str,
    done_label: &str,
) {
    // -- compute the scalar result and detect signed overflow --
    ctx.emitter.instruction(&format!("mov {}, {}", saved_lhs_reg, result_reg)); // preserve the original left operand for overflow promotion
    match op {
        CheckedIntOp::Add => {
            ctx.emitter.instruction(
                &format!("adds {}, {}, {}", result_reg, result_reg, rhs_reg)
            );                                                                  // compute addition and set the signed-overflow flag
            ctx.emitter.instruction(&format!("b.vs {}", overflow_label));       // convert the promoted double only when signed addition overflowed
        }
        CheckedIntOp::Sub => {
            ctx.emitter.instruction(
                &format!("subs {}, {}, {}", result_reg, result_reg, rhs_reg)
            );                                                                  // compute subtraction and set the signed-overflow flag
            ctx.emitter.instruction(&format!("b.vs {}", overflow_label));       // convert the promoted double only when signed subtraction overflowed
        }
        CheckedIntOp::Mul => {
            let high_reg = abi::symbol_scratch_reg(ctx.emitter);
            ctx.emitter.instruction(
                &format!("smulh {}, {}, {}", high_reg, result_reg, rhs_reg)
            );                                                                  // compute the signed high product for overflow detection
            ctx.emitter.instruction(
                &format!("mul {}, {}, {}", result_reg, result_reg, rhs_reg)
            );                                                                  // compute the low 64 bits of the signed product
            ctx.emitter.instruction(
                &format!("cmp {}, {}, asr #63", high_reg, result_reg)
            );                                                                  // compare the high product with the low half's sign extension
            ctx.emitter.instruction(&format!("b.ne {}", overflow_label));       // convert the promoted double when the product does not fit in I64
        }
    }
    ctx.emitter.instruction(&format!("b {}", done_label));                      // keep the in-range scalar result and skip overflow conversion
}

/// Emits the x86_64 scalar fast path and branches to the shared overflow path.
fn emit_x86_64_checked(
    ctx: &mut FunctionContext<'_>,
    op: CheckedIntOp,
    result_reg: &str,
    rhs_reg: &str,
    saved_lhs_reg: &str,
    overflow_label: &str,
    done_label: &str,
) {
    // -- compute the scalar result and detect signed overflow --
    ctx.emitter.instruction(&format!("mov {}, {}", saved_lhs_reg, result_reg)); // preserve the original left operand for overflow promotion
    let mnemonic = match op {
        CheckedIntOp::Add => "add",
        CheckedIntOp::Sub => "sub",
        CheckedIntOp::Mul => "imul",
    };
    ctx.emitter.instruction(
        &format!("{} {}, {}", mnemonic, result_reg, rhs_reg)
    );                                                                          // compute the scalar result and set the signed-overflow flag
    ctx.emitter.instruction(&format!("jo {}", overflow_label));                 // convert the promoted double only when the operation overflowed
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // keep the in-range scalar result and skip overflow conversion
}

/// Reports an integer overflow the narrowed slot cannot represent, and exits.
///
/// Reached only from a `IChecked*ToInt` overflow branch, so the value is one `checked_int_sink`
/// proved is observed as an `int` everywhere. php would promote it to a float; this slot holds
/// 64 raw bits and cannot, and the previous answer — php's float-to-int conversion of the true
/// result — was a number php never produces.
fn emit_int_overflow_fatal(ctx: &mut FunctionContext<'_>) {
    super::objects::emit_fatal_message(
        ctx,
        b"Fatal error: integer overflow in arithmetic whose result is used as an int; \
php would promote this value to float, which this storage cannot hold\n",
    );
}

/// Recomputes an overflowing operation as double, matching PHP's promotion path.
fn emit_overflow_conversion(
    ctx: &mut FunctionContext<'_>,
    op: CheckedIntOp,
    lhs_reg: &str,
    rhs_reg: &str,
) {
    let mnemonic = op.float_mnemonic(ctx.emitter.target.arch);
    // -- reproduce PHP's overflow promotion in floating-point registers --
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("scvtf d0, {}", lhs_reg));         // promote the original left integer operand to double
            ctx.emitter.instruction(&format!("scvtf d1, {}", rhs_reg));         // promote the original right integer operand to double
            ctx.emitter.instruction(&format!("{} d0, d0, d1", mnemonic));       // reproduce PHP's overflow-promoted double arithmetic
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cvtsi2sd xmm0, {}", lhs_reg));    // promote the original left integer operand to double
            ctx.emitter.instruction(&format!("cvtsi2sd xmm1, {}", rhs_reg));    // promote the original right integer operand to double
            ctx.emitter.instruction(&format!("{} xmm0, xmm1", mnemonic));       // reproduce PHP's overflow-promoted double arithmetic
        }
    }
}
