//! Purpose:
//! Lowers scalar EIR instructions (the `Op` enum subset) to WebAssembly for the
//! wasm32-wasi backend: integer/float arithmetic, comparisons, conversions,
//! truthiness/null predicates, constants, and local-variable access.
//!
//! Called from:
//! - `crate::codegen_wasm::function::emit_dispatch_loop` for each instruction in a
//!   block, before the block's terminator.
//!
//! Key details:
//! - Each value-producing op loads its operands onto the WASM operand stack,
//!   computes the result, then stores it into the result value's local(s).
//! - `IDiv` is PHP `/`, which always yields a float; both i64 operands are widened
//!   with `f64.convert_i64_s` before `f64.div`.
//! - Float constants are emitted bit-exactly (`i64.const <bits>; f64.reinterpret_i64`)
//!   to avoid any float-literal formatting ambiguity.
//! - Borrow rule: `value_repr`/`slot_repr` borrow `ctx`; clone the needed strings
//!   (via `local_refs()` or `.clone()`) before calling a `&mut self` method.

use super::context::{wasm_fn_symbol, FnCtx, Result};
use super::values::WasmRepr;
use super::WasmError;
use crate::ir::{CmpPredicate, DataId, Immediate, InstId, Instruction, LocalSlotId, Op, ValueId};

/// Lowers one EIR instruction by id. Loads operands, computes the result on the
/// WASM operand stack, and stores it into the result value's local(s). Unsupported
/// ops return `WasmError::Unsupported` so the pipeline can surface a clean diagnostic.
pub(super) fn lower_instruction(ctx: &mut FnCtx, inst_id: InstId) -> Result<()> {
    // Clone the instruction so we can mutate ctx.fb without holding a borrow on ctx.function.
    let inst = ctx
        .function
        .instruction(inst_id)
        .cloned()
        .ok_or_else(|| WasmError::Unsupported(format!("missing instruction {:?}", inst_id)))?;

    match inst.op {
        Op::ConstI64 => lower_const_i64(ctx, &inst),
        Op::ConstF64 => lower_const_f64(ctx, &inst),
        Op::ConstBool => lower_const_bool(ctx, &inst),
        Op::ConstNull => lower_const_null(ctx, &inst),
        Op::Nop => lower_nop(ctx),
        Op::ConcatReset => lower_concat_reset(ctx),
        Op::LoadLocal => lower_load_local(ctx, &inst),
        Op::StoreLocal => lower_store_local(ctx, &inst),
        Op::IAdd => lower_int_binop(ctx, &inst, "i64.add"),
        Op::ISub => lower_int_binop(ctx, &inst, "i64.sub"),
        Op::IMul => lower_int_binop(ctx, &inst, "i64.mul"),
        Op::IBitAnd => lower_int_binop(ctx, &inst, "i64.and"),
        Op::IBitOr => lower_int_binop(ctx, &inst, "i64.or"),
        Op::IBitXor => lower_int_binop(ctx, &inst, "i64.xor"),
        Op::IShl => lower_int_binop(ctx, &inst, "i64.shl"),
        Op::IShrA => lower_int_binop(ctx, &inst, "i64.shr_s"),
        Op::ISDiv => lower_int_binop(ctx, &inst, "i64.div_s"),
        Op::ISMod => lower_int_binop(ctx, &inst, "i64.rem_s"),
        Op::INeg => lower_int_neg(ctx, &inst),
        Op::IBitNot => lower_int_bitnot(ctx, &inst),
        Op::IDiv => lower_int_div_to_float(ctx, &inst),
        Op::FAdd => lower_float_binop(ctx, &inst, "f64.add"),
        Op::FSub => lower_float_binop(ctx, &inst, "f64.sub"),
        Op::FMul => lower_float_binop(ctx, &inst, "f64.mul"),
        Op::FDiv => lower_float_binop(ctx, &inst, "f64.div"),
        Op::FNeg => lower_float_neg(ctx, &inst),
        Op::ICmp => lower_int_cmp(ctx, &inst),
        Op::FCmp => lower_float_cmp(ctx, &inst),
        Op::IToF => lower_itof(ctx, &inst),
        Op::FToI => lower_ftoi(ctx, &inst),
        Op::IsTruthy => lower_is_truthy(ctx, &inst),
        Op::IsNull => lower_is_null(ctx, &inst),
        Op::Call => lower_call(ctx, &inst),
        Op::EchoValue | Op::PrintValue => lower_echo(ctx, &inst),
        other => Err(WasmError::Unsupported(format!("op {:?}", other))),
    }
}

/// Stores the instruction's result into its value local(s), if it produces one.
fn store_result(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    if let Some(r) = inst.result {
        ctx.emit_store_value(r)?;
    }
    Ok(())
}

/// Returns the i-th operand of the instruction, or an error if missing.
fn operand(inst: &Instruction, i: usize) -> Result<ValueId> {
    inst.operands
        .get(i)
        .copied()
        .ok_or_else(|| WasmError::Unsupported(format!("missing operand {} in {:?}", i, inst.op)))
}

/// Extracts a `CmpPredicate` from the instruction's immediate, or an error.
fn cmp_immediate(inst: &Instruction) -> Result<CmpPredicate> {
    match &inst.immediate {
        Some(Immediate::CmpPredicate(pred)) => Ok(*pred),
        _ => Err(WasmError::Unsupported(format!(
            "missing CmpPredicate in {:?}",
            inst.op
        ))),
    }
}

/// Extracts an i64 from the instruction's immediate, or an error.
fn i64_immediate(inst: &Instruction) -> Result<i64> {
    match &inst.immediate {
        Some(Immediate::I64(n)) => Ok(*n),
        _ => Err(WasmError::Unsupported(format!(
            "missing i64 immediate in {:?}",
            inst.op
        ))),
    }
}

/// Extracts an f64 from the instruction's immediate, or an error.
fn f64_immediate(inst: &Instruction) -> Result<f64> {
    match &inst.immediate {
        Some(Immediate::F64(f)) => Ok(*f),
        _ => Err(WasmError::Unsupported(format!(
            "missing f64 immediate in {:?}",
            inst.op
        ))),
    }
}

/// Extracts a bool from the instruction's immediate, or an error.
fn bool_immediate(inst: &Instruction) -> Result<bool> {
    match &inst.immediate {
        Some(Immediate::Bool(b)) => Ok(*b),
        _ => Err(WasmError::Unsupported(format!(
            "missing bool immediate in {:?}",
            inst.op
        ))),
    }
}

/// Extracts a `LocalSlotId` from the instruction's immediate, or an error.
fn slot_immediate(inst: &Instruction) -> Result<LocalSlotId> {
    match &inst.immediate {
        Some(Immediate::LocalSlot(slot)) => Ok(*slot),
        _ => Err(WasmError::Unsupported(format!(
            "missing LocalSlot immediate in {:?}",
            inst.op
        ))),
    }
}

/// Extracts a `DataId` from the instruction's immediate, or an error.
fn data_immediate(inst: &Instruction) -> Result<DataId> {
    match &inst.immediate {
        Some(Immediate::Data(d)) => Ok(*d),
        _ => Err(WasmError::Unsupported(format!(
            "missing Data immediate in {:?}",
            inst.op
        ))),
    }
}

/// Lowers `Op::Call` to a direct WebAssembly call of a user function.
///
/// The callee is named by an `Immediate::Data` index into the module's
/// function-name pool; arguments are pushed in source order (matching the callee's
/// declared parameter locals), then `call $fn_<name>` is emitted. A produced result
/// is stored; if the call's result is discarded, the callee's return values are
/// dropped so the WASM operand stack stays balanced.
fn lower_call(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let data_id = data_immediate(inst)?;
    let name = ctx
        .module
        .data
        .function_names
        .get(data_id.as_raw() as usize)
        .cloned()
        .ok_or_else(|| WasmError::Unsupported(format!("call: unknown function data {:?}", data_id)))?;

    // Arity of the callee's WASM result, to balance the stack when the result is unused.
    let return_arity = ctx
        .module
        .functions
        .iter()
        .find(|f| f.name == name)
        .map(|f| WasmRepr::val_types(f.return_type).len())
        .unwrap_or(0);

    for &arg in &inst.operands {
        ctx.emit_load_value(arg)?;
    }
    ctx.fb
        .ins(&format!("call ${}", wasm_fn_symbol(&name)), &format!("call {}", name));

    if let Some(r) = inst.result {
        ctx.emit_store_value(r)?;
    } else {
        for _ in 0..return_arity {
            ctx.fb.ins("drop", "discard unused call result");
        }
    }
    Ok(())
}

/// Lowers an integer binary op: load both operands, emit the wasm op, store result.
fn lower_int_binop(ctx: &mut FnCtx, inst: &Instruction, wasm_op: &str) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.emit_load_value(operand(inst, 1)?)?;
    ctx.fb.ins(wasm_op, "integer binary op");
    store_result(ctx, inst)
}

/// Lowers a float binary op: load both operands, emit the wasm op, store result.
fn lower_float_binop(ctx: &mut FnCtx, inst: &Instruction, wasm_op: &str) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.emit_load_value(operand(inst, 1)?)?;
    ctx.fb.ins(wasm_op, "float binary op");
    store_result(ctx, inst)
}

/// Lowers `ConstI64`: pushes the immediate integer constant.
fn lower_const_i64(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let n = i64_immediate(inst)?;
    ctx.fb.ins(&format!("i64.const {}", n), "int literal");
    store_result(ctx, inst)
}

/// Lowers `ConstF64` bit-exactly: push the f64's raw bits and reinterpret them as f64.
fn lower_const_f64(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let bits = f64_immediate(inst)?.to_bits() as i64;
    ctx.fb.ins(&format!("i64.const {}", bits), "f64 literal bits");
    ctx.fb.ins("f64.reinterpret_i64", "reinterpret bits as f64");
    store_result(ctx, inst)
}

/// Lowers `ConstBool`: pushes 1 for true, 0 for false (PHP bool is an i64).
fn lower_const_bool(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let val = if bool_immediate(inst)? { 1 } else { 0 };
    ctx.fb.ins(&format!("i64.const {}", val), "bool literal");
    store_result(ctx, inst)
}

/// Lowers `ConstNull`: pushes the i64 null sentinel (0x7fff_ffff_ffff_fffe).
fn lower_const_null(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.fb.ins(
        "i64.const 9223372036854775806",
        "null sentinel (0x7fff_ffff_ffff_fffe)",
    );
    store_result(ctx, inst)
}

/// Lowers `Nop`: emits a comment; the result local (if any) keeps its default 0.
fn lower_nop(ctx: &mut FnCtx) -> Result<()> {
    ctx.fb.comment("nop");
    Ok(())
}

/// Lowers `ConcatReset`: a no-op until string concatenation is implemented.
fn lower_concat_reset(ctx: &mut FnCtx) -> Result<()> {
    ctx.fb
        .comment("concat_reset (no-op until string concat is implemented)");
    Ok(())
}

/// Lowers `LoadLocal`: copies the slot's local(s) into the result value's local(s).
fn lower_load_local(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let slot = slot_immediate(inst)?;
    let result = inst
        .result
        .ok_or_else(|| WasmError::Unsupported("load_local without result".to_string()))?;
    let slot_refs = ctx.slot_repr(slot)?.local_refs();
    let result_refs = ctx.value_repr(result)?.local_refs();
    if slot_refs.len() != result_refs.len() {
        return Err(WasmError::Unsupported(format!(
            "load_local repr mismatch: slot has {} local(s), result has {}",
            slot_refs.len(),
            result_refs.len()
        )));
    }
    for r in &slot_refs {
        ctx.fb.ins(&format!("local.get {}", r), "load local slot");
    }
    ctx.emit_store_value(result)
}

/// Lowers `StoreLocal`: stores the operand value into the slot's local(s).
fn lower_store_local(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let slot = slot_immediate(inst)?;
    let value = operand(inst, 0)?;
    let slot_refs = ctx.slot_repr(slot)?.local_refs();
    let value_refs = ctx.value_repr(value)?.local_refs();
    if slot_refs.len() != value_refs.len() {
        return Err(WasmError::Unsupported(format!(
            "store_local repr mismatch: slot has {} local(s), value has {}",
            slot_refs.len(),
            value_refs.len()
        )));
    }
    ctx.emit_load_value(value)?;
    // Pop in reverse so the first slot local takes the bottom-most stack value.
    for r in slot_refs.iter().rev() {
        ctx.fb.ins(&format!("local.set {}", r), "store local slot");
    }
    Ok(())
}

/// Lowers `INeg`: computes `0 - x`.
fn lower_int_neg(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.fb.ins("i64.const 0", "0 for negation");
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb.ins("i64.sub", "0 - x");
    store_result(ctx, inst)
}

/// Lowers `IBitNot`: computes `x ^ -1` (one's complement).
fn lower_int_bitnot(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb.ins("i64.const -1", "all-ones mask");
    ctx.fb.ins("i64.xor", "bitwise not");
    store_result(ctx, inst)
}

/// Lowers `IDiv` (PHP `/`): widens both i64 operands to f64 and divides.
fn lower_int_div_to_float(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb.ins("f64.convert_i64_s", "lhs to float");
    ctx.emit_load_value(operand(inst, 1)?)?;
    ctx.fb.ins("f64.convert_i64_s", "rhs to float");
    ctx.fb.ins("f64.div", "php / is float division");
    store_result(ctx, inst)
}

/// Lowers `FNeg`: negates a float.
fn lower_float_neg(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb.ins("f64.neg", "negate float");
    store_result(ctx, inst)
}

/// Maps an integer comparison predicate to its signed wasm comparison op.
fn int_cmp_op(pred: CmpPredicate) -> Result<&'static str> {
    Ok(match pred {
        CmpPredicate::Eq => "i64.eq",
        CmpPredicate::Ne => "i64.ne",
        CmpPredicate::Slt => "i64.lt_s",
        CmpPredicate::Sle => "i64.le_s",
        CmpPredicate::Sgt => "i64.gt_s",
        CmpPredicate::Sge => "i64.ge_s",
        other => {
            return Err(WasmError::Unsupported(format!(
                "integer compare predicate {:?}",
                other
            )))
        }
    })
}

/// Maps a float comparison predicate to its (ordered) wasm comparison op.
fn float_cmp_op(pred: CmpPredicate) -> &'static str {
    match pred {
        CmpPredicate::Eq => "f64.eq",
        CmpPredicate::Ne => "f64.ne",
        CmpPredicate::Slt | CmpPredicate::Olt => "f64.lt",
        CmpPredicate::Sle | CmpPredicate::Ole => "f64.le",
        CmpPredicate::Sgt | CmpPredicate::Ogt => "f64.gt",
        CmpPredicate::Sge | CmpPredicate::Oge => "f64.ge",
    }
}

/// Lowers `ICmp`: signed integer comparison yielding an i64 boolean (0/1).
fn lower_int_cmp(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let wasm_op = int_cmp_op(cmp_immediate(inst)?)?;
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.emit_load_value(operand(inst, 1)?)?;
    ctx.fb.ins(wasm_op, "integer comparison");
    ctx.fb.ins("i64.extend_i32_u", "bool i32 -> i64");
    store_result(ctx, inst)
}

/// Lowers `FCmp`: ordered float comparison yielding an i64 boolean (0/1).
fn lower_float_cmp(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let wasm_op = float_cmp_op(cmp_immediate(inst)?);
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.emit_load_value(operand(inst, 1)?)?;
    ctx.fb.ins(wasm_op, "float comparison");
    ctx.fb.ins("i64.extend_i32_u", "bool i32 -> i64");
    store_result(ctx, inst)
}

/// Lowers `IToF`: signed integer to float.
fn lower_itof(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb.ins("f64.convert_i64_s", "int to float");
    store_result(ctx, inst)
}

/// Lowers `FToI`: float to signed integer (truncate toward zero; NaN -> 0).
fn lower_ftoi(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb
        .ins("i64.trunc_sat_f64_s", "float to int (truncate, NaN->0)");
    store_result(ctx, inst)
}

/// Lowers `IsTruthy` for i64 (int/bool) and f64 operands; other reprs are unsupported.
fn lower_is_truthy(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let op0 = operand(inst, 0)?;
    let repr = ctx.value_repr(op0)?.clone();
    match repr {
        WasmRepr::I64(_) => {
            ctx.emit_load_value(op0)?;
            ctx.fb.ins("i64.const 0", "zero");
            ctx.fb.ins("i64.ne", "truthy = x != 0");
            ctx.fb.ins("i64.extend_i32_u", "bool i32 -> i64");
        }
        WasmRepr::F64(_) => {
            ctx.emit_load_value(op0)?;
            ctx.fb.ins("f64.const 0.0", "zero");
            ctx.fb.ins("f64.ne", "truthy = x != 0.0");
            ctx.fb.ins("i64.extend_i32_u", "bool i32 -> i64");
        }
        other => {
            return Err(WasmError::Unsupported(format!("is_truthy of {:?}", other)));
        }
    }
    store_result(ctx, inst)
}

/// Lowers `EchoValue`/`PrintValue` by dispatching on the operand's representation.
///
/// Integers are written via the `__rt_echo_i64` runtime helper. Other types
/// (float, string, bool, mixed, ...) need additional runtime support and are not
/// handled yet.
fn lower_echo(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let op0 = operand(inst, 0)?;
    let repr = ctx.value_repr(op0)?.clone();
    match repr {
        WasmRepr::I64(_) => {
            ctx.emit_load_value(op0)?;
            ctx.fb.ins("call $__rt_echo_i64", "echo integer to stdout");
            Ok(())
        }
        other => Err(WasmError::Unsupported(format!("echo of {:?}", other))),
    }
}

/// Lowers `IsNull` for i64 operands by comparing against the null sentinel.
fn lower_is_null(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let op0 = operand(inst, 0)?;
    let repr = ctx.value_repr(op0)?.clone();
    match repr {
        WasmRepr::I64(_) => {
            ctx.emit_load_value(op0)?;
            ctx.fb
                .ins("i64.const 9223372036854775806", "null sentinel");
            ctx.fb.ins("i64.eq", "is_null = x == sentinel");
            ctx.fb.ins("i64.extend_i32_u", "bool i32 -> i64");
        }
        other => {
            return Err(WasmError::Unsupported(format!("is_null of {:?}", other)));
        }
    }
    store_result(ctx, inst)
}
