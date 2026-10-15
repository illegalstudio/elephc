//! Purpose:
//! Lowers checked integer arithmetic whose only observable result is a PHP `int`.
//! Keeps the in-range path scalar and STOPS on an overflow it cannot represent.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` for `IChecked*ToInt` opcodes.
//!
//! Key details:
//! - The in-range result falls through; one conditional branch enters the cold overflow path,
//!   which is deferred to the function's out-of-line tail and jumps back after reporting.
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
use crate::ir_passes::rematerialize::immediate_use;
use crate::codegen_support::abi::immediate::IntImmediate;

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
    let immediate = immediate_use(ctx.function, inst, ctx.emitter.target);
    if let Some(usage) = &immediate {
        load_integer_operand(ctx, usage.other, result_reg, inst)?;
    } else {
        load_integer_operand(ctx, lhs, result_reg, inst)?;
        load_integer_operand(ctx, rhs, rhs_reg, inst)?;
    }
    let overflow_label = ctx.next_label("checked_int_to_int_overflow");
    let done_label = ctx.next_label("checked_int_to_int_done");

    if let Some(usage) = &immediate {
        emit_immediate_checked(ctx, &usage.encoding, &overflow_label);
    } else {
        match ctx.emitter.target.arch {
            Arch::AArch64 => emit_aarch64_checked(
                ctx,
                op,
                result_reg,
                rhs_reg,
                saved_lhs_reg,
                &overflow_label,
            ),
            Arch::X86_64 => emit_x86_64_checked(
                ctx,
                op,
                result_reg,
                rhs_reg,
                saved_lhs_reg,
                &overflow_label,
            ),
        }
    }
    ctx.emitter.label(&done_label);

    let constant_index = immediate.as_ref().map(|usage| usage.constant_index);
    ctx.emit_cold_path(|ctx| {
        ctx.emitter.label(&overflow_label);
        // `Immediate::Bool(true)` means an explicit `(int)` cast observed this value, and php's
        // own answer there is the promoted double converted back to an int. Anything else means
        // the narrowing came from a raw int SLOT, where php would have kept the float — so the
        // program stops rather than print a number php never produces.
        if matches!(inst.immediate, Some(Immediate::Bool(true))) {
            if let Some(index) = constant_index {
                load_integer_operand(ctx, inst.operands[index], rhs_reg, inst)?;
            }
            emit_overflow_conversion(ctx, op, saved_lhs_reg, rhs_reg);
            abi::emit_php_float_to_int(ctx.emitter, result_reg);
        } else {
            emit_int_overflow_fatal(ctx);
        }
        abi::emit_jump(ctx.emitter, &done_label);
        Ok(())
    })?;
    store_if_result(ctx, inst)
}

/// Branches to the cold overflow path when the signed-overflow condition holds.
///
/// AArch64 uses the wide form (`b.<ok> 1f; b overflow; 1:`) so the deferred cold tail stays
/// reachable in any function size; the machine peephole folds it into one `b.<overflow>` when
/// the function is small enough. `ok` / `overflow` are AArch64 conditions, `x86_jump` the
/// x86_64 jcc mnemonic.
pub(super) fn emit_overflow_branch(
    ctx: &mut FunctionContext<'_>,
    ok: &str,
    x86_jump: &str,
    overflow: &str,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("b.{ok} 1f"));                     // keep the in-range result on the fallthrough path
            ctx.emitter.instruction(&format!("b {overflow}"));                  // enter the out-of-line overflow path
            ctx.emitter.label("1");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("{x86_jump} {overflow}"));         // enter the out-of-line overflow path
        }
    }
}

/// Uses overflow-setting immediate forms and reconstructs no right operand on the fast path.
fn emit_immediate_checked(ctx: &mut FunctionContext<'_>, immediate: &IntImmediate, overflow: &str) {
    let reg = abi::int_result_reg(ctx.emitter);
    let saved = abi::tertiary_scratch_reg(ctx.emitter);
    // -- retain the original input for PHP overflow promotion --
    ctx.emitter.instruction(&format!("mov {saved}, {reg}"));                    // preserve the nonconstant operand for the cold conversion path
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("{}s {reg}, {reg}, {}", immediate.mnemonic, immediate.operand)); // compute the scalar result with signed overflow flags
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("{} {reg}, {}", immediate.mnemonic, immediate.operand)); // compute the scalar result with signed overflow flags
        }
    }
    emit_overflow_branch(ctx, "vc", "jo", overflow);
}

/// Emits the AArch64 scalar fast path, branching to `overflow_label` only on overflow.
fn emit_aarch64_checked(
    ctx: &mut FunctionContext<'_>,
    op: CheckedIntOp,
    result_reg: &str,
    rhs_reg: &str,
    saved_lhs_reg: &str,
    overflow_label: &str,
) {
    // -- compute the scalar result and detect signed overflow --
    ctx.emitter.instruction(&format!("mov {}, {}", saved_lhs_reg, result_reg)); // preserve the original left operand for overflow promotion
    match op {
        CheckedIntOp::Add => {
            ctx.emitter.instruction(
                &format!("adds {}, {}, {}", result_reg, result_reg, rhs_reg)
            );                                                                  // compute addition and set the signed-overflow flag
            emit_overflow_branch(ctx, "vc", "jo", overflow_label);
        }
        CheckedIntOp::Sub => {
            ctx.emitter.instruction(
                &format!("subs {}, {}, {}", result_reg, result_reg, rhs_reg)
            );                                                                  // compute subtraction and set the signed-overflow flag
            emit_overflow_branch(ctx, "vc", "jo", overflow_label);
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
            emit_overflow_branch(ctx, "eq", "jne", overflow_label);
        }
    }
}

/// Emits the x86_64 scalar fast path, branching to `overflow_label` only on overflow.
fn emit_x86_64_checked(
    ctx: &mut FunctionContext<'_>,
    op: CheckedIntOp,
    result_reg: &str,
    rhs_reg: &str,
    saved_lhs_reg: &str,
    overflow_label: &str,
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
    emit_overflow_branch(ctx, "vc", "jo", overflow_label);
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
