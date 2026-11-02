//! Purpose:
//! Emits checked integer arithmetic into a nonescaping frame-local Mixed cell.
//!
//! Called from:
//! - Checked arithmetic lowering after frame layout proves the complete cell lifetime.
//!
//! Key details:
//! - The usual Mixed tag and payload are preserved, including PHP overflow to float.
//! - The cell has no heap header and must never escape, acquire children, or be freed.
//! - Every target uses the same 24-byte layout and the target-aware frame helpers.
//! - Tagged numeric arithmetic remains a clobber point, so its extra caller-saved tag registers
//!   cannot contain unrelated live SSA values. Both operand homes are read first.

use crate::codegen::{abi, platform::Arch, Result};
use crate::codegen::context::FunctionContext;
use crate::ir::{Instruction, MixedNumericOp, Op};
use crate::types::PhpType;
use super::{expect_operand, load_integer_operand, store_if_result};
use super::super::numeric_payloads;

/// Computes the checked result without allocating a heap cell on either outcome.
pub(super) fn lower(ctx: &mut FunctionContext<'_>, inst: &Instruction, offset: usize) -> Result<()> {
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    load_integer_operand(ctx, lhs, abi::int_result_reg(ctx.emitter), inst)?;
    load_integer_operand(ctx, rhs, abi::secondary_scratch_reg(ctx.emitter), inst)?;
    let overflow = ctx.next_label("stack_numeric_overflow");
    let done = ctx.next_label("stack_numeric_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => arm64(ctx, inst.op, offset, &overflow, &done),
        Arch::X86_64 => x86_64(ctx, inst.op, offset, &overflow, &done),
    }
    ctx.emitter.label(&done);
    abi::emit_frame_slot_address(ctx.emitter, abi::int_result_reg(ctx.emitter), offset);
    store_if_result(ctx, inst)
}

/// Reads both numeric payloads before updating the cell, including self-add and float continuation.
pub(super) fn lower_mixed(ctx: &mut FunctionContext<'_>, inst: &Instruction, offset: usize) -> Result<()> {
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    let op = match super::expect_mixed_numeric_op(inst)? {
        MixedNumericOp::Add => Op::ICheckedAdd,
        MixedNumericOp::Sub => Op::ICheckedSub,
        MixedNumericOp::Mul => Op::ICheckedMul,
        _ => unreachable!("the numeric frame proof accepts add/sub/mul only"),
    };
    let (left, right, left_tag, right_tag, _) = registers(ctx);
    ctx.load_value_to_reg(lhs, left)?;
    ctx.load_value_to_reg(rhs, right)?;
    let left_type = numeric_payloads::payload(ctx, lhs, left, left_tag)?;
    let right_type = numeric_payloads::payload(ctx, rhs, right, right_tag)?;
    tagged(ctx, inst, offset, op, left_type, right_type)
}

/// Updates a proven numeric increment cell while retaining float promotion across iterations.
pub(in crate::codegen::lower_inst) fn lower_increment(ctx: &mut FunctionContext<'_>, inst: &Instruction, offset: usize) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    let delta = super::super::expect_i64(inst)?;
    let (left, right, left_tag, right_tag, _) = registers(ctx);
    ctx.load_value_to_reg(value, left)?;
    let left_type = numeric_payloads::payload(ctx, value, left, left_tag)?;
    abi::emit_load_int_immediate(ctx.emitter, right, delta);
    abi::emit_load_int_immediate(ctx.emitter, right_tag, 0);
    tagged(ctx, inst, offset, Op::ICheckedAdd, left_type, PhpType::Int)
}

/// Uses the same caller-saved register set for arithmetic and unary numeric updates.
fn registers(ctx: &FunctionContext<'_>) -> (&'static str, &'static str, &'static str, &'static str, &'static str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => ("x0", "x10", "x11", "x9", "x12"),
        Arch::X86_64 => ("rax", "r10", "r11", "r9", "r8"),
    }
}

/// Dispatches on the actual numeric tags after all original payloads have been loaded.
fn tagged(ctx: &mut FunctionContext<'_>, inst: &Instruction, offset: usize, op: Op, left_type: PhpType, right_type: PhpType) -> Result<()> {
    let (left, right, left_tag, right_tag, combined) = registers(ctx);
    let integer = ctx.next_label("stack_numeric_integer");
    let overflow = ctx.next_label("stack_numeric_overflow");
    let done = ctx.next_label("stack_numeric_done");
    // -- keep a promoted float on every subsequent execution of the loop body --
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("orr {combined}, {left_tag}, {right_tag}")); // a zero combined tag means both payloads are PHP integers
            ctx.emitter.instruction(&format!("cbz {combined}, {integer}"));     // use checked integer arithmetic only for two integer tags
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov {combined}, {left_tag}"));    // preserve the individual tags for float conversion
            ctx.emitter.instruction(&format!("or {combined}, {right_tag}"));    // a zero combined tag means both payloads are PHP integers
            ctx.emitter.instruction(&format!("test {combined}, {combined}"));   // select integer or floating-point arithmetic from the actual tags
            ctx.emitter.instruction(&format!("jz {integer}"));                  // use checked integer arithmetic only for two integer tags
        }
    }
    numeric_payloads::float(ctx, &left_type, left, left_tag, abi::float_result_reg(ctx.emitter));
    let other_float = match ctx.emitter.target.arch { Arch::AArch64 => "d1", Arch::X86_64 => "xmm1" };
    numeric_payloads::float(ctx, &right_type, right, right_tag, other_float);
    let float = match (ctx.emitter.target.arch, op) {
        (Arch::AArch64, Op::ICheckedAdd) => "fadd d0, d0, d1",
        (Arch::AArch64, Op::ICheckedSub) => "fsub d0, d0, d1",
        (Arch::AArch64, Op::ICheckedMul) => "fmul d0, d0, d1",
        (Arch::X86_64, Op::ICheckedAdd) => "addsd xmm0, xmm1",
        (Arch::X86_64, Op::ICheckedSub) => "subsd xmm0, xmm1",
        (Arch::X86_64, Op::ICheckedMul) => "mulsd xmm0, xmm1",
        _ => unreachable!(),
    };
    ctx.emitter.instruction(float);                                             // preserve PHP floating-point arithmetic after promotion
    abi::emit_reg_move(ctx.emitter, left, abi::float_result_reg(ctx.emitter));
    store(ctx, offset, 2);
    ctx.emit_branch(&done);
    ctx.emitter.label(&integer);
    match ctx.emitter.target.arch {
        Arch::AArch64 => arm64(ctx, op, offset, &overflow, &done),
        Arch::X86_64 => x86_64(ctx, op, offset, &overflow, &done),
    }
    ctx.emitter.label(&done);
    abi::emit_frame_slot_address(ctx.emitter, left, offset);
    store_if_result(ctx, inst)
}

/// Writes the current scalar bits and its concrete runtime tag to the reserved cell.
fn store(ctx: &mut FunctionContext<'_>, offset: usize, tag: i64) {
    let result = abi::int_result_reg(ctx.emitter);
    abi::store_at_offset(ctx.emitter, result, offset - 8);
    abi::emit_load_int_immediate(ctx.emitter, result, tag);
    abi::store_at_offset(ctx.emitter, result, offset);
    abi::emit_load_int_immediate(ctx.emitter, result, 0);
    abi::store_at_offset(ctx.emitter, result, offset - 16);
}

/// Preserves both ARM64 inputs until the overflow branch has selected its result type.
fn arm64(ctx: &mut FunctionContext<'_>, op: Op, offset: usize, overflow: &str, done: &str) {
    // -- compute the integer result while preserving the original operands --
    let (integer, float) = match op {
        Op::ICheckedAdd => ("adds", "fadd"),
        Op::ICheckedSub => ("subs", "fsub"),
        Op::ICheckedMul => ("mul", "fmul"),
        _ => unreachable!("stack cells accept checked add, subtract and multiply only"),
    };
    ctx.emitter.instruction(&format!("{integer} x11, x0, x10"));                // keep both inputs intact until overflow has been tested
    if op == Op::ICheckedMul {
        ctx.emitter.instruction("smulh x9, x0, x10");                           // recover the high half of the signed integer product
        ctx.emitter.instruction("cmp x9, x11, asr #63");                        // a fitting product has only sign extension in its high half
        ctx.emitter.instruction(&format!("b.ne {overflow}"));                   // promote an overflowing signed product to PHP float
    } else {
        ctx.emitter.instruction(&format!("b.vs {overflow}"));                   // promote an overflowing signed sum or difference to PHP float
    }
    ctx.emitter.instruction("mov x0, x11");                                     // publish the fitting integer bits for the frame-local Mixed cell
    store(ctx, offset, 0);
    ctx.emitter.instruction(&format!("b {done}"));                              // skip float conversion after the integer result has been stored
    ctx.emitter.label(overflow);
    // -- reproduce PHP promotion from the original signed operands --
    ctx.emitter.instruction("scvtf d0, x0");                                    // convert the original left integer rather than the wrapped result
    ctx.emitter.instruction("scvtf d1, x10");                                   // convert the original right integer with signed semantics
    ctx.emitter.instruction(&format!("{float} d0, d0, d1"));                    // compute the promoted PHP floating-point result
    ctx.emitter.instruction("fmov x0, d0");                                     // store double bits in the Mixed payload word
    store(ctx, offset, 2);
}

/// Uses an independent x86 result register so signed overflow keeps both original inputs.
fn x86_64(ctx: &mut FunctionContext<'_>, op: Op, offset: usize, overflow: &str, done: &str) {
    let (integer, float) = match op {
        Op::ICheckedAdd => ("add", "addsd"),
        Op::ICheckedSub => ("sub", "subsd"),
        Op::ICheckedMul => ("imul", "mulsd"),
        _ => unreachable!("stack cells accept checked add, subtract and multiply only"),
    };
    // -- compute the integer result while preserving the original operands --
    ctx.emitter.instruction("mov r11, rax");                                    // retain the original left operand for PHP overflow promotion
    ctx.emitter.instruction(&format!("{integer} r11, r10"));                    // compute the checked integer result in a separate scratch register
    ctx.emitter.instruction(&format!("jo {overflow}"));                         // promote when the signed arithmetic does not fit in an integer
    ctx.emitter.instruction("mov rax, r11");                                    // publish the fitting integer bits for the frame-local Mixed cell
    store(ctx, offset, 0);
    ctx.emitter.instruction(&format!("jmp {done}"));                            // skip float conversion after the integer result has been stored
    ctx.emitter.label(overflow);
    // -- reproduce PHP promotion from the original signed operands --
    ctx.emitter.instruction("cvtsi2sd xmm0, rax");                              // convert the original signed left operand before overflow wrapping
    ctx.emitter.instruction("cvtsi2sd xmm1, r10");                              // convert the original signed right operand into a second double
    ctx.emitter.instruction(&format!("{float} xmm0, xmm1"));                    // compute the promoted PHP floating-point result
    ctx.emitter.instruction("movq rax, xmm0");                                  // store double bits in the Mixed payload word
    store(ctx, offset, 2);
}
