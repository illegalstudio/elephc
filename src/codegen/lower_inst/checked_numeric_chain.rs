//! Purpose:
//! Lowers fused checked integer arithmetic chains to allocation-free target assembly.
//! Keeps the hot path in integer registers and completes the suffix as double after overflow.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` for `ICheckedNumericChainToInt`.
//!
//! Key details:
//! - Every overflow edge preserves the exact integer accumulator and right operand at that step.
//! - Shared float suffix blocks avoid duplicating code while preserving PHP's first-overflow
//!   promotion point, including integer precision above the exact-double range.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen::{CodegenIrError, Result};
use crate::ir::{Immediate, Instruction, MixedNumericOp};

use super::super::context::FunctionContext;
use super::{arithmetic::load_integer_operand, store_if_result};

/// Lowers one fused checked chain and stores its final PHP integer result.
pub(super) fn lower_checked_numeric_chain_to_int(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let operations = expect_operations(inst)?;
    if inst.operands.len() != operations.len() + 1 {
        return Err(CodegenIrError::invalid_module(format!(
            "{} operand count does not match its operation sequence",
            inst.op.name()
        )));
    }

    let result_reg = abi::int_result_reg(ctx.emitter);
    let rhs_reg = abi::secondary_scratch_reg(ctx.emitter);
    let saved_lhs_reg = abi::tertiary_scratch_reg(ctx.emitter);
    load_integer_operand(ctx, inst.operands[0], result_reg, inst)?;

    let overflow_labels = (0..operations.len())
        .map(|_| ctx.next_label("checked_numeric_chain_overflow"))
        .collect::<Vec<_>>();
    let suffix_labels = (1..operations.len())
        .map(|_| ctx.next_label("checked_numeric_chain_float_suffix"))
        .collect::<Vec<_>>();
    let float_finish_label = ctx.next_label("checked_numeric_chain_float_finish");
    let done_label = ctx.next_label("checked_numeric_chain_done");

    for (index, operation) in operations.iter().copied().enumerate() {
        load_integer_operand(ctx, inst.operands[index + 1], rhs_reg, inst)?;
        emit_checked_step(
            ctx,
            operation,
            result_reg,
            rhs_reg,
            saved_lhs_reg,
            &overflow_labels[index],
        );
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip every float slow block after an in-range integer chain
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip every float slow block after an in-range integer chain
        }
    }

    for (index, operation) in operations.iter().copied().enumerate() {
        ctx.emitter.label(&overflow_labels[index]);
        emit_overflowing_step_as_float(ctx, operation, saved_lhs_reg, rhs_reg);
        let target = if index + 1 < operations.len() {
            &suffix_labels[index]
        } else {
            &float_finish_label
        };
        emit_jump(ctx, target);
    }

    for index in 1..operations.len() {
        ctx.emitter.label(&suffix_labels[index - 1]);
        load_integer_operand(ctx, inst.operands[index + 1], rhs_reg, inst)?;
        emit_rhs_as_float(ctx, rhs_reg);
        emit_float_step(ctx, operations[index]);
    }
    ctx.emitter.label(&float_finish_label);
    abi::emit_php_float_to_int(ctx.emitter, result_reg);
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Returns the checked operation sequence attached to the fused opcode.
fn expect_operations(inst: &Instruction) -> Result<&[MixedNumericOp]> {
    match inst.immediate.as_ref() {
        Some(Immediate::CheckedNumericChain(chain))
            if !chain.operations().is_empty()
                && chain
                    .operations()
                    .iter()
                    .all(|op| !matches!(op, MixedNumericOp::Pow | MixedNumericOp::UnaryPlus)) =>
        {
            Ok(chain.operations())
        }
        _ => Err(CodegenIrError::invalid_module(format!(
            "{} missing valid checked numeric chain immediate",
            inst.op.name()
        ))),
    }
}

/// Emits one checked integer operation and branches when signed overflow occurs.
fn emit_checked_step(
    ctx: &mut FunctionContext<'_>,
    operation: MixedNumericOp,
    result_reg: &str,
    rhs_reg: &str,
    saved_lhs_reg: &str,
    overflow_label: &str,
) {
    ctx.emitter.instruction(&format!("mov {}, {}", saved_lhs_reg, result_reg)); // preserve the exact accumulator at this possible promotion point
    match ctx.emitter.target.arch {
        Arch::AArch64 => emit_aarch64_checked_step(
            ctx,
            operation,
            result_reg,
            rhs_reg,
            overflow_label,
        ),
        Arch::X86_64 => {
            let mnemonic = integer_mnemonic(operation, Arch::X86_64);
            let assembly = format!("{} {}, {}", mnemonic, result_reg, rhs_reg);
            ctx.emitter.instruction(&assembly);                                 // compute the next integer value and set signed-overflow state
            ctx.emitter.instruction(&format!("jo {}", overflow_label));         // enter float semantics exactly at the first overflowing operation
        }
    }
}

/// Emits one checked AArch64 operation, including full-width multiply detection.
fn emit_aarch64_checked_step(
    ctx: &mut FunctionContext<'_>,
    operation: MixedNumericOp,
    result_reg: &str,
    rhs_reg: &str,
    overflow_label: &str,
) {
    match operation {
        MixedNumericOp::Add | MixedNumericOp::Sub => {
            let mnemonic = integer_mnemonic(operation, Arch::AArch64);
            let assembly = format!("{} {}, {}, {}", mnemonic, result_reg, result_reg, rhs_reg);
            ctx.emitter.instruction(&assembly);                                 // compute the next integer value and set signed-overflow state
            ctx.emitter.instruction(&format!("b.vs {}", overflow_label));       // enter float semantics exactly at the first overflowing operation
        }
        MixedNumericOp::Mul => {
            let high_reg = abi::symbol_scratch_reg(ctx.emitter);
            let high_product = format!("smulh {}, {}, {}", high_reg, result_reg, rhs_reg);
            ctx.emitter.instruction(&high_product);                             // retain the signed high product for full-width overflow detection
            let low_product = format!("mul {}, {}, {}", result_reg, result_reg, rhs_reg);
            ctx.emitter.instruction(&low_product);                              // compute the low 64 bits of the signed product
            let compare = format!("cmp {}, {}, asr #63", high_reg, result_reg);
            ctx.emitter.instruction(&compare);                                  // compare the high product with the low half's sign extension
            ctx.emitter.instruction(&format!("b.ne {}", overflow_label));       // enter float semantics when the product does not fit in I64
        }
        MixedNumericOp::Pow | MixedNumericOp::UnaryPlus => {
            unreachable!("validated checked chains exclude pow and unary plus")
        }
    }
}

/// Converts the saved overflowing operands to double and evaluates that operation.
fn emit_overflowing_step_as_float(
    ctx: &mut FunctionContext<'_>,
    operation: MixedNumericOp,
    lhs_reg: &str,
    rhs_reg: &str,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("scvtf d0, {}", lhs_reg));         // promote the exact pre-overflow accumulator to double
            ctx.emitter.instruction(&format!("scvtf d1, {}", rhs_reg));         // promote the overflowing operation's right integer to double
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cvtsi2sd xmm0, {}", lhs_reg));    // promote the exact pre-overflow accumulator to double
            ctx.emitter.instruction(&format!("cvtsi2sd xmm1, {}", rhs_reg));    // promote the overflowing operation's right integer to double
        }
    }
    emit_float_step(ctx, operation);
}

/// Promotes a suffix right-hand integer operand into the secondary float register.
fn emit_rhs_as_float(ctx: &mut FunctionContext<'_>, rhs_reg: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("scvtf d1, {}", rhs_reg));         // promote the next integer operand after overflow
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cvtsi2sd xmm1, {}", rhs_reg));    // promote the next integer operand after overflow
        }
    }
}

/// Applies one add/sub/mul step to the float accumulator.
fn emit_float_step(ctx: &mut FunctionContext<'_>, operation: MixedNumericOp) {
    let mnemonic = float_mnemonic(operation, ctx.emitter.target.arch);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("{} d0, d0, d1", mnemonic));       // continue the promoted PHP numeric chain in double precision
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("{} xmm0, xmm1", mnemonic));       // continue the promoted PHP numeric chain in double precision
        }
    }
}

/// Emits an unconditional target-aware branch to a local label.
fn emit_jump(ctx: &mut FunctionContext<'_>, label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("b {}", label));                   // continue at the shared float suffix for this overflow point
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("jmp {}", label));                 // continue at the shared float suffix for this overflow point
        }
    }
}

/// Returns the integer mnemonic for one supported checked operation.
fn integer_mnemonic(operation: MixedNumericOp, arch: Arch) -> &'static str {
    match (operation, arch) {
        (MixedNumericOp::Add, Arch::AArch64) => "adds",
        (MixedNumericOp::Sub, Arch::AArch64) => "subs",
        (MixedNumericOp::Mul, Arch::AArch64) => "mul",
        (MixedNumericOp::Add, Arch::X86_64) => "add",
        (MixedNumericOp::Sub, Arch::X86_64) => "sub",
        (MixedNumericOp::Mul, Arch::X86_64) => "imul",
        (MixedNumericOp::Pow | MixedNumericOp::UnaryPlus, _) => {
            unreachable!("validated checked chains exclude pow and unary plus")
        }
    }
}

/// Returns the double mnemonic for one supported checked operation.
fn float_mnemonic(operation: MixedNumericOp, arch: Arch) -> &'static str {
    match (operation, arch) {
        (MixedNumericOp::Add, Arch::AArch64) => "fadd",
        (MixedNumericOp::Sub, Arch::AArch64) => "fsub",
        (MixedNumericOp::Mul, Arch::AArch64) => "fmul",
        (MixedNumericOp::Add, Arch::X86_64) => "addsd",
        (MixedNumericOp::Sub, Arch::X86_64) => "subsd",
        (MixedNumericOp::Mul, Arch::X86_64) => "mulsd",
        (MixedNumericOp::Pow | MixedNumericOp::UnaryPlus, _) => {
            unreachable!("validated checked chains exclude pow and unary plus")
        }
    }
}
