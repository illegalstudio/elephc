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
//! - A raw integer/float seed of a proven numeric local writes that slot's own seed cell.
//! - A clone of a proven numeric value copies its tag and payload into the clone's own cell.
//! - A `min()`/`max()` selection copies the winning operand's own tag and payload, so an integer
//!   that beats a float stays an integer; only the comparison promotes a mixed pair to doubles.
//! - Tagged numeric arithmetic remains a clobber point, so its extra caller-saved tag registers
//!   cannot contain unrelated live SSA values. Both operand homes are read first.

use crate::codegen::{abi, platform::Arch, Result};
use crate::codegen::context::FunctionContext;
use crate::ir::{Instruction, MinMaxRule, MixedNumericOp, Op};
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

/// Selects a two-argument `min()`/`max()` of proven numeric operands into the result's frame cell.
///
/// Two integer tags compare as signed integers; any float promotes both sides to doubles for
/// the comparison only, as PHP does. The winner's original tag and payload are then copied,
/// so the result keeps the selected operand's type. Both payloads are read before the cell is
/// written, which keeps `$m = max($m, $x)` correct when `$m` already lives in this cell.
///
/// An unordered (`NAN`) pair returns the right operand, with php-src's one exception: a
/// `max()` whose other operand is an integer that a double cannot represent compares
/// generically there and keeps its left operand (`unordered_max`).
pub(super) fn lower_min_max(ctx: &mut FunctionContext<'_>, inst: &Instruction, offset: usize, rule: MinMaxRule) -> Result<()> {
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    let (left, right, left_tag, right_tag, combined) = registers(ctx);
    ctx.load_value_to_reg(lhs, left)?;
    ctx.load_value_to_reg(rhs, right)?;
    let left_type = numeric_payloads::payload(ctx, lhs, left, left_tag)?;
    let right_type = numeric_payloads::payload(ctx, rhs, right, right_tag)?;
    let integer = ctx.next_label("stack_min_max_integer");
    let unordered = ctx.next_label("stack_min_max_unordered");
    let take_left = ctx.next_label("stack_min_max_left");
    let take_right = ctx.next_label("stack_min_max_right");
    let done = ctx.next_label("stack_min_max_done");
    let other_float = match ctx.emitter.target.arch { Arch::AArch64 => "d1", Arch::X86_64 => "xmm1" };
    // -- compare two integer tags exactly and any other numeric pair as PHP doubles --
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("orr {combined}, {left_tag}, {right_tag}")); // a zero combined tag means both payloads are PHP integers
            ctx.emitter.instruction(&format!("cbz {combined}, {integer}"));     // compare two integers without a lossy double conversion
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov {combined}, {left_tag}"));    // preserve the individual tags for the selected result
            ctx.emitter.instruction(&format!("or {combined}, {right_tag}"));    // a zero combined tag means both payloads are PHP integers
            ctx.emitter.instruction(&format!("test {combined}, {combined}"));   // select integer or floating-point comparison from the actual tags
            ctx.emitter.instruction(&format!("jz {integer}"));                  // compare two integers without a lossy double conversion
        }
    }
    numeric_payloads::float(ctx, &left_type, left, left_tag, abi::float_result_reg(ctx.emitter));
    numeric_payloads::float(ctx, &right_type, right, right_tag, other_float);
    match (ctx.emitter.target.arch, rule) {
        (Arch::AArch64, MinMaxRule::MaxPair) => {
            ctx.emitter.instruction("fcmp d0, d1");                             // order the left double against the right double
            ctx.emitter.instruction(&format!("b.vs {unordered}"));              // a NAN is decided by php-src's comparison mode
            ctx.emitter.instruction(&format!("b.mi {take_right}"));             // a smaller left operand fails `left >= right`
        }
        (Arch::AArch64, MinMaxRule::MinPair) => {
            ctx.emitter.instruction("fcmp d0, d1");                             // order the left double against the right double
            ctx.emitter.instruction(&format!("b.pl {take_right}"));             // an equal, greater or unordered left operand fails `left < right`
        }
        (Arch::X86_64, MinMaxRule::MaxPair) => {
            ctx.emitter.instruction("ucomisd xmm0, xmm1");                      // order the left double against the right double
            ctx.emitter.instruction(&format!("jp {unordered}"));                // a NAN is decided by php-src's comparison mode
            ctx.emitter.instruction(&format!("jb {take_right}"));               // a smaller left operand fails `left >= right`
        }
        (Arch::X86_64, MinMaxRule::MinPair) => {
            ctx.emitter.instruction("ucomisd xmm1, xmm0");                      // order the right double against the left double
            ctx.emitter.instruction(&format!("jbe {take_right}"));              // a right operand that is not strictly greater, or unordered, wins
        }
        (_, MinMaxRule::MaxFold | MinMaxRule::MinFold) => {
            return Err(crate::codegen::CodegenIrError::invalid_module(
                "a min/max selection of three or more operands has no inline frame form".to_string(),
            ));
        }
    }
    ctx.emit_branch(&take_left);
    if rule == MinMaxRule::MaxPair {
        ctx.emitter.label(&unordered);
        unordered_max(ctx, (left, left_tag), (right, right_tag), combined, &take_left, &take_right);
    }
    ctx.emitter.label(&integer);
    let (aarch64, x86_64) = match rule {
        MinMaxRule::MaxPair | MinMaxRule::MaxFold => ("b.lt", "jl"),
        MinMaxRule::MinPair | MinMaxRule::MinFold => ("b.ge", "jge"),
    };
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {left}, {right}"));           // order the two signed integer payloads
            ctx.emitter.instruction(&format!("{aarch64} {take_right}"));        // select the right operand when the rule rejects the left one
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {left}, {right}"));           // order the two signed integer payloads
            ctx.emitter.instruction(&format!("{x86_64} {take_right}"));         // select the right operand when the rule rejects the left one
        }
    }
    // -- copy the winner's own tag and payload into the frame-local Mixed cell --
    ctx.emitter.label(&take_left);
    write_cell(ctx, offset, left, CellTag::Register(left_tag));
    ctx.emit_branch(&done);
    ctx.emitter.label(&take_right);
    // The right tag lives in the register `store_at_offset` uses as its AArch64 address
    // scratch for a cell beyond the short frame range, so the payload store would destroy it.
    // The combined-tag register is dead after the comparison and is never a store scratch.
    abi::emit_reg_move(ctx.emitter, combined, right_tag);
    write_cell(ctx, offset, right, CellTag::Register(combined));
    ctx.emitter.label(&done);
    abi::emit_frame_slot_address(ctx.emitter, abi::int_result_reg(ctx.emitter), offset);
    store_if_result(ctx, inst)
}

/// Decides a two-argument `max()` whose double comparison was unordered.
///
/// At least one operand is `NAN`. Two floats, or a float and an integer that a double
/// represents exactly, are php-src's numeric path: `left >= right` is false and the right
/// operand is returned. A float against any other integer is its generic comparison, which
/// reports the pair as "greater" and keeps the left operand.
fn unordered_max(
    ctx: &mut FunctionContext<'_>,
    (left, left_tag): (&str, &str),
    (right, right_tag): (&str, &str),
    scratch: &str,
    take_left: &str,
    take_right: &str,
) {
    let right_integer = ctx.next_label("stack_min_max_nan_right");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbnz {left_tag}, {right_integer}")); // a float left operand leaves the right one to check
            representable_or(ctx, left, scratch, take_left);
            ctx.emit_branch(take_right);
            ctx.emitter.label(&right_integer);
            ctx.emitter.instruction(&format!("cbnz {right_tag}, {take_right}")); // two floats fail `left >= right`
            representable_or(ctx, right, scratch, take_left);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {left_tag}, {left_tag}"));   // is the left operand the integer of the pair?
            ctx.emitter.instruction(&format!("jnz {right_integer}"));           // a float left operand leaves the right one to check
            representable_or(ctx, left, scratch, take_left);
            ctx.emit_branch(take_right);
            ctx.emitter.label(&right_integer);
            ctx.emitter.instruction(&format!("test {right_tag}, {right_tag}")); // is the right operand the integer of the pair?
            ctx.emitter.instruction(&format!("jnz {take_right}"));              // two floats fail `left >= right`
            representable_or(ctx, right, scratch, take_left);
        }
    }
    ctx.emit_branch(take_right);
}

/// Branches to `otherwise` unless a double represents the signed integer in `value` exactly.
///
/// This is php-src's `zend_dval_to_lval((double) value) == value` test. `scratch` and the
/// third float register are clobbered.
fn representable_or(ctx: &mut FunctionContext<'_>, value: &str, scratch: &str, otherwise: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("scvtf d2, {value}"));             // convert the integer to a double
            ctx.emitter.instruction(&format!("fcvtzs {scratch}, d2"));          // convert the double back to an integer
            ctx.emitter.instruction(&format!("cmp {scratch}, {value}"));        // did the round trip preserve the integer?
            ctx.emitter.instruction(&format!("b.ne {otherwise}"));              // a double cannot represent this integer
            ctx.emitter.instruction(&format!("mov {scratch}, #0x7fffffffffffffff")); // the largest integer rounds up to 2^63 and saturates back
            ctx.emitter.instruction(&format!("cmp {value}, {scratch}"));        // is this that unrepresentable integer?
            ctx.emitter.instruction(&format!("b.eq {otherwise}"));              // a double cannot represent the largest integer either
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cvtsi2sd xmm2, {value}"));        // convert the integer to a double
            ctx.emitter.instruction(&format!("cvttsd2si {scratch}, xmm2"));     // convert it back; 2^63 yields the integer indefinite value
            ctx.emitter.instruction(&format!("cmp {scratch}, {value}"));        // did the round trip preserve the integer?
            ctx.emitter.instruction(&format!("jne {otherwise}"));               // a double cannot represent this integer
        }
    }
}

/// Copies the operand cell selected by `__rt_mixed_min_max_select` into the result's frame cell.
///
/// The integer result register holds the winner's borrowed cell pointer, which may point
/// into the `staging` bytes of temporary stack that this releases once both words are read.
pub(super) fn store_selected(ctx: &mut FunctionContext<'_>, inst: &Instruction, offset: usize, staging: usize) -> Result<()> {
    let (payload, _, _, _, tag) = registers(ctx);
    abi::emit_load_from_address(ctx.emitter, tag, payload, 0);
    abi::emit_load_from_address(ctx.emitter, payload, payload, 8);
    abi::emit_release_temporary_stack(ctx.emitter, staging);
    write_cell(ctx, offset, payload, CellTag::Register(tag));
    abi::emit_frame_slot_address(ctx.emitter, abi::int_result_reg(ctx.emitter), offset);
    store_if_result(ctx, inst)
}

/// Stores the operand selected by `__rt_mixed_min_max_select` for a runtime-typed selection
/// whose result is only read.
///
/// A winner holding an integer or a float is copied into the selection's own frame cell,
/// whose address becomes the result, so the common numeric call allocates nothing. Any other
/// winner (a string, a bool, null, a nested box) becomes an owned heap copy through
/// `__rt_mixed_clone`. Users of the result read it through the pointer either way,
/// and its release ignores the frame cell because it lies outside the managed heap.
pub(super) fn store_guarded(ctx: &mut FunctionContext<'_>, inst: &Instruction, offset: usize, staging: usize) -> Result<()> {
    let (payload, _, _, _, tag) = registers(ctx);
    let numeric = ctx.next_label("guarded_min_max_numeric");
    let owned = ctx.next_label("guarded_min_max_owned");
    let done = ctx.next_label("guarded_min_max_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz {payload}, {owned}"));        // a null cell is materialized as PHP null on the heap
            ctx.emitter.instruction(&format!("ldr {tag}, [{payload}]"));        // load the winner's runtime tag
            ctx.emitter.instruction(&format!("cbz {tag}, {numeric}"));          // an integer winner fits the frame cell
            ctx.emitter.instruction(&format!("cmp {tag}, #2"));                 // is the winner a float?
            ctx.emitter.instruction(&format!("b.eq {numeric}"));                // a float winner fits the frame cell
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {payload}, {payload}"));     // is the winner a null cell pointer?
            ctx.emitter.instruction(&format!("jz {owned}"));                    // a null cell is materialized as PHP null on the heap
            ctx.emitter.instruction(&format!("mov {tag}, QWORD PTR [{payload}]")); // load the winner's runtime tag
            ctx.emitter.instruction(&format!("test {tag}, {tag}"));             // is the winner an integer?
            ctx.emitter.instruction(&format!("jz {numeric}"));                  // an integer winner fits the frame cell
            ctx.emitter.instruction(&format!("cmp {tag}, 2"));                  // is the winner a float?
            ctx.emitter.instruction(&format!("je {numeric}"));                  // a float winner fits the frame cell
        }
    }
    ctx.emitter.label(&owned);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_clone");
    abi::emit_release_temporary_stack(ctx.emitter, staging);
    ctx.emit_branch(&done);
    ctx.emitter.label(&numeric);
    abi::emit_load_from_address(ctx.emitter, payload, payload, 8);
    abi::emit_release_temporary_stack(ctx.emitter, staging);
    write_cell(ctx, offset, payload, CellTag::Register(tag));
    abi::emit_frame_slot_address(ctx.emitter, abi::int_result_reg(ctx.emitter), offset);
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
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

/// Publishes a raw integer or float seed through its slot's frame cell instead of a heap box.
///
/// The slot's previous occupant was already retired by the preceding `release_local_slot`;
/// the scalar is loaded before the cell is rewritten, so a slot that still points at this
/// cell is simply republished with the new payload.
pub(in crate::codegen::lower_inst) fn store_seed(ctx: &mut FunctionContext<'_>, slot: crate::ir::LocalSlotId, value: crate::ir::ValueId, cell: usize) -> Result<()> {
    let result = abi::int_result_reg(ctx.emitter);
    write_scalar(ctx, value, cell)?;
    let offset = ctx.local_offset(slot)?;
    abi::store_at_offset(ctx.emitter, result, offset);
    Ok(())
}

/// Boxes a raw integer or float seed into its proven frame cell instead of the heap.
pub(in crate::codegen::lower_inst) fn lower_box(ctx: &mut FunctionContext<'_>, inst: &Instruction, cell: usize) -> Result<()> {
    write_scalar(ctx, expect_operand(inst, 0)?, cell)?;
    store_if_result(ctx, inst)
}

/// Copies a proven numeric value into its clone's frame cell instead of a fresh heap box.
///
/// The numeric-domain proof admits only integer and float sources, so the clone's private copy
/// is the source tag and payload words, exactly what `__rt_mixed_clone` would box. A source that
/// optimization narrowed to a raw scalar is tagged like a seed.
pub(in crate::codegen::lower_inst) fn lower_clone(ctx: &mut FunctionContext<'_>, inst: &Instruction, cell: usize) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    if ctx.value_php_type(value)?.codegen_repr() != PhpType::Mixed {
        write_scalar(ctx, value, cell)?;
        return store_if_result(ctx, inst);
    }
    let (payload, _, tag, _, _) = registers(ctx);
    ctx.load_value_to_reg(value, payload)?;
    numeric_payloads::payload(ctx, value, payload, tag)?;
    // -- copy the numeric tag and payload into the clone's frame-local Mixed cell --
    write_cell(ctx, cell, payload, CellTag::Register(tag));
    abi::emit_frame_slot_address(ctx.emitter, abi::int_result_reg(ctx.emitter), cell);
    store_if_result(ctx, inst)
}

/// Writes one raw scalar and its tag to a cell, leaving the cell address in the result register.
fn write_scalar(ctx: &mut FunctionContext<'_>, value: crate::ir::ValueId, cell: usize) -> Result<()> {
    let ty = ctx.load_value_to_result(value)?.codegen_repr();
    let result = abi::int_result_reg(ctx.emitter);
    // -- tag the scalar payload in the frame-local Mixed cell --
    let tag = if ty == PhpType::Float {
        abi::emit_reg_move(ctx.emitter, result, abi::float_result_reg(ctx.emitter));
        2
    } else {
        0
    };
    store(ctx, cell, tag);
    abi::emit_frame_slot_address(ctx.emitter, result, cell);
    Ok(())
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
    write_cell(ctx, offset, result, CellTag::Immediate(tag));
}

/// Where a frame cell's runtime tag comes from.
enum CellTag<'a> {
    Immediate(i64),
    Register(&'a str),
}

/// Lays out one 24-byte frame cell: payload at `offset - 8`, tag at `offset`, a cleared
/// word at `offset - 16`. The payload register is clobbered.
fn write_cell(ctx: &mut FunctionContext<'_>, offset: usize, payload: &str, tag: CellTag<'_>) {
    abi::store_at_offset(ctx.emitter, payload, offset - 8);
    match tag {
        CellTag::Immediate(tag) => {
            abi::emit_load_int_immediate(ctx.emitter, payload, tag);
            abi::store_at_offset(ctx.emitter, payload, offset);
        }
        CellTag::Register(tag) => abi::store_at_offset(ctx.emitter, tag, offset),
    }
    abi::emit_load_int_immediate(ctx.emitter, payload, 0);
    abi::store_at_offset(ctx.emitter, payload, offset - 16);
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
