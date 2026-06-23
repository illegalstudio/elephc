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
use crate::ir::{
    CmpPredicate, DataId, Immediate, InstId, Instruction, LocalSlotId, Op, Ownership, ValueDef,
    ValueId,
};
use crate::types::PhpType;

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
        Op::ConstStr => lower_const_str(ctx, &inst),
        Op::StrLen => lower_strlen(ctx, &inst),
        Op::StrConcat => lower_str_concat(ctx, &inst),
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
        Op::LoadGlobal => lower_load_global(ctx, &inst),
        Op::BuiltinCall => lower_builtin_call(ctx, &inst),
        Op::EchoValue | Op::PrintValue => lower_echo(ctx, &inst),
        Op::Acquire => lower_acquire(ctx, &inst),
        Op::Release => lower_release(ctx, &inst),
        Op::Move | Op::Borrow => lower_forward(ctx, &inst),
        Op::ArrayNew => lower_array_new(ctx, &inst),
        Op::ArrayLen => lower_array_len(ctx, &inst),
        Op::ArrayGet => lower_array_get(ctx, &inst),
        Op::ArrayPush => lower_array_push(ctx, &inst),
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

/// Lowers `ConstStr`: pushes the literal's linear-memory pointer (i32) and byte
/// length (i64) from the module's string-literal layout.
fn lower_const_str(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let (offset, len) = ctx.str_literal(data_immediate(inst)?)?;
    ctx.fb
        .ins(&format!("i32.const {}", offset), "string literal ptr");
    ctx.fb.ins(&format!("i64.const {}", len), "string literal len");
    store_result(ctx, inst)
}

/// Lowers `StrLen`: reads the length component of a string value.
fn lower_strlen(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let op0 = operand(inst, 0)?;
    let repr = ctx.value_repr(op0)?.clone();
    match repr {
        WasmRepr::Str { len, .. } => {
            ctx.fb.ins(&format!("local.get {}", len), "string length");
        }
        other => return Err(WasmError::Unsupported(format!("strlen of {:?}", other))),
    }
    store_result(ctx, inst)
}

/// Lowers `Nop`: emits a comment; the result local (if any) keeps its default 0.
fn lower_nop(ctx: &mut FnCtx) -> Result<()> {
    ctx.fb.comment("nop");
    Ok(())
}

/// Lowers `ConcatReset`: restores the global concat cursor to this frame's
/// baseline, freeing string temporaries built during the statement.
fn lower_concat_reset(ctx: &mut FnCtx) -> Result<()> {
    ctx.fb
        .ins(&format!("local.get {}", ctx.concat_base_local), "frame concat baseline");
    ctx.fb
        .ins("global.set $__concat_off", "reset concat cursor to baseline");
    Ok(())
}

/// Lowers `StrConcat`: appends two strings into the concat buffer via `__rt_concat`.
///
/// Pushes (a_ptr, a_len, b_ptr, b_len) — matching `__rt_concat`'s parameter order —
/// and stores the returned `(ptr, len)` into the result string value.
fn lower_str_concat(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.emit_load_value(operand(inst, 1)?)?;
    ctx.fb.ins("call $__rt_concat", "concatenate two strings");
    store_result(ctx, inst)
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

/// Lowers `Op::LoadGlobal` for supported superglobals.
///
/// `$argc` is read via the `__rt_argc` runtime helper (WASI `args_sizes_get`).
/// Other globals (including `$argv`, which needs the array runtime) are not yet
/// supported.
fn lower_load_global(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let data_id = match &inst.immediate {
        Some(Immediate::GlobalName(d)) => *d,
        _ => return Err(WasmError::Unsupported("load_global without a name".to_string())),
    };
    let name = ctx
        .module
        .data
        .global_names
        .get(data_id.as_raw() as usize)
        .cloned()
        .ok_or_else(|| WasmError::Unsupported(format!("load_global: unknown name {:?}", data_id)))?;
    match name.as_str() {
        "argc" => {
            ctx.fb.ins("call $__rt_argc", "load $argc");
            store_result(ctx, inst)
        }
        other => Err(WasmError::Unsupported(format!("global ${}", other))),
    }
}

/// Lowers `Op::BuiltinCall` by dispatching on the builtin's name.
///
/// Only `exit`/`die` are handled so far; other builtins return `Unsupported`.
fn lower_builtin_call(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let data_id = data_immediate(inst)?;
    let name = ctx
        .module
        .data
        .function_names
        .get(data_id.as_raw() as usize)
        .cloned()
        .ok_or_else(|| WasmError::Unsupported(format!("builtin: unknown name data {:?}", data_id)))?;
    match name.as_str() {
        "exit" | "die" => lower_exit(ctx, inst),
        other => Err(WasmError::Unsupported(format!("builtin {}", other))),
    }
}

/// Lowers `exit`/`die`: an integer argument becomes the WASI exit status; any
/// other argument (a message string) or no argument exits with status 0. Matching
/// the native backend, a string message is NOT printed.
fn lower_exit(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let int_code = inst.operands.first().is_some_and(|arg| {
        ctx.function
            .value(*arg)
            .map(|v| v.php_type.codegen_repr() == PhpType::Int)
            .unwrap_or(false)
    });
    if int_code {
        ctx.emit_load_value(operand(inst, 0)?)?;
        ctx.fb.ins("i32.wrap_i64", "exit code to i32");
    } else {
        ctx.fb.ins("i32.const 0", "exit status 0");
    }
    ctx.fb.ins("call $wasi_proc_exit", "WASI proc_exit(code)");
    Ok(())
}

/// Lowers `EchoValue`/`PrintValue` by dispatching on the operand's PHP type.
///
/// Integers and booleans share the i64 representation, so the PHP type is used to
/// pick the right runtime helper (booleans print "1"/"" rather than "0"/"1").
/// Float, mixed, array, and object output need more runtime support and are not
/// handled yet.
fn lower_echo(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let op0 = operand(inst, 0)?;
    let php = ctx
        .function
        .value(op0)
        .map(|v| v.php_type.codegen_repr())
        .ok_or_else(|| WasmError::Unsupported(format!("echo: unknown operand {:?}", op0)))?;
    match php {
        PhpType::Bool => {
            ctx.emit_load_value(op0)?;
            ctx.fb.ins("call $__rt_echo_bool", "echo boolean to stdout");
            Ok(())
        }
        PhpType::Int => {
            ctx.emit_load_value(op0)?;
            ctx.fb.ins("call $__rt_echo_i64", "echo integer to stdout");
            Ok(())
        }
        PhpType::Str => {
            // Pushes ptr (i32) then len (i64), matching __rt_echo_str's params.
            ctx.emit_load_value(op0)?;
            ctx.fb.ins("call $__rt_echo_str", "echo string to stdout");
            Ok(())
        }
        other => Err(WasmError::Unsupported(format!("echo of {:?}", other))),
    }
}

/// Lowers `Op::Acquire`: makes the operand value safe to store as a new owner.
///
/// A PHP string is copied into an owned heap block (`__rt_str_persist`), matching
/// PHP string value semantics; a heap pointer is increfed (`__rt_incref`); scalars
/// forward unchanged. The result value receives the acquired value. A `Mixed`
/// (tagged) value is not handled yet (its ownership lands with the boxing phase).
fn lower_acquire(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let value = operand(inst, 0)?;
    let repr = ctx.value_repr(value)?.clone();
    match repr {
        WasmRepr::Str { .. } => {
            ctx.emit_load_value(value)?;
            ctx.fb
                .ins("call $__rt_str_persist", "persist string to an owned heap copy");
            store_result(ctx, inst)
        }
        WasmRepr::Ptr(_) => {
            ctx.emit_load_value(value)?;
            ctx.fb.ins("call $__rt_incref", "incref the owned heap value");
            forward_value(ctx, value, inst)
        }
        WasmRepr::I64(_) | WasmRepr::F64(_) | WasmRepr::Void => forward_value(ctx, value, inst),
        WasmRepr::Tagged { .. } => {
            Err(WasmError::Unsupported("acquire of a Mixed value".to_string()))
        }
    }
}

/// Lowers `Op::Release`: releases storage the value may own.
///
/// No-op for ownership states that cannot own heap storage (non-heap, borrowed,
/// persistent, moved). A string is freed through the bounds/refcount-guarded
/// `__rt_heap_free_safe` (so transient concat/literal pointers are skipped there);
/// a heap pointer is released through the `__rt_decref_any` kind dispatcher. A
/// `Mixed` (tagged) value is not handled yet.
fn lower_release(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let value = operand(inst, 0)?;
    let ownership = ctx
        .function
        .value(value)
        .map(|v| v.ownership)
        .unwrap_or(Ownership::NonHeap);
    if matches!(
        ownership,
        Ownership::NonHeap | Ownership::Borrowed | Ownership::Persistent | Ownership::Moved
    ) {
        return Ok(());
    }
    let repr = ctx.value_repr(value)?.clone();
    match repr {
        WasmRepr::Str { ptr, .. } => {
            ctx.fb
                .ins(&format!("local.get {}", ptr), "string pointer to free");
            ctx.fb
                .ins("call $__rt_heap_free_safe", "free the owned string (skips non-heap)");
            Ok(())
        }
        WasmRepr::Ptr(_) => {
            ctx.emit_load_value(value)?;
            ctx.fb
                .ins("call $__rt_decref_any", "release the owned heap value by kind");
            Ok(())
        }
        WasmRepr::I64(_) | WasmRepr::F64(_) | WasmRepr::Void => Ok(()),
        WasmRepr::Tagged { .. } => {
            Err(WasmError::Unsupported("release of a Mixed value".to_string()))
        }
    }
}

/// Lowers `Op::Move` / `Op::Borrow`: pure value forwarding, copying the operand's
/// local(s) into the result's local(s) with no refcount change.
fn lower_forward(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let value = operand(inst, 0)?;
    forward_value(ctx, value, inst)
}

/// Copies `value`'s local(s) into the instruction result's local(s), if the
/// instruction produces a result. Errors if the two reprs differ in local arity.
fn forward_value(ctx: &mut FnCtx, value: ValueId, inst: &Instruction) -> Result<()> {
    let Some(result) = inst.result else {
        return Ok(());
    };
    let value_refs = ctx.value_repr(value)?.local_refs();
    let result_refs = ctx.value_repr(result)?.local_refs();
    if value_refs.len() != result_refs.len() {
        return Err(WasmError::Unsupported(format!(
            "forward repr mismatch: operand has {} local(s), result has {}",
            value_refs.len(),
            result_refs.len()
        )));
    }
    for r in &value_refs {
        ctx.fb
            .ins(&format!("local.get {}", r), "forward operand local");
    }
    ctx.emit_store_value(result)
}

/// Returns the local slot a value was loaded from, if its defining instruction is
/// a `LoadLocal`. Used by `ArrayPush` to write a reallocated array pointer back to
/// the variable's slot (mirroring the native `source_load_local_slot`).
fn value_source_slot(ctx: &FnCtx, value: ValueId) -> Option<LocalSlotId> {
    let v = ctx.function.value(value)?;
    let ValueDef::Instruction { inst, .. } = v.def else {
        return None;
    };
    let inst = ctx.function.instruction(inst)?;
    if inst.op == Op::LoadLocal {
        if let Some(Immediate::LocalSlot(slot)) = inst.immediate {
            return Some(slot);
        }
    }
    None
}

/// Lowers `Op::ArrayNew`: allocates an empty indexed array with the immediate
/// capacity. The element size defaults to 16 bytes; `__rt_array_push_int` shrinks
/// it to 8 on the first scalar push, matching the native backend.
fn lower_array_new(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let capacity = match &inst.immediate {
        Some(Immediate::Capacity(c)) => *c as i64,
        _ => return Err(WasmError::Unsupported("array_new without a capacity".to_string())),
    };
    ctx.fb
        .ins(&format!("i64.const {}", capacity), "initial capacity");
    ctx.fb
        .ins("i64.const 16", "default elem_size (specialized on first push)");
    ctx.fb.ins("call $__rt_array_new", "allocate indexed array");
    store_result(ctx, inst)
}

/// Lowers `Op::ArrayLen`: reads the i64 length stored at the array header (A+0).
fn lower_array_len(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb.ins("i64.load", "array length @ +0");
    store_result(ctx, inst)
}

/// Lowers `Op::ArrayGet` for scalar (int) arrays via the bounded runtime getter,
/// which returns the PHP null sentinel for an out-of-range index.
fn lower_array_get(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let result = inst
        .result
        .ok_or_else(|| WasmError::Unsupported("array_get without a result".to_string()))?;
    let result_repr = ctx.value_repr(result)?.clone();
    match result_repr {
        WasmRepr::I64(_) => {
            ctx.emit_load_value(operand(inst, 0)?)?; // array pointer
            ctx.emit_load_value(operand(inst, 1)?)?; // index (i64)
            ctx.fb
                .ins("call $__rt_array_get_int", "indexed array get (int)");
            store_result(ctx, inst)
        }
        other => Err(WasmError::Unsupported(format!("array_get into {:?}", other))),
    }
}

/// Lowers `Op::ArrayPush`. Appends via the runtime (which may reallocate) and
/// writes the returned pointer back into the operand value's local and its source
/// slot, so `$arr[] = v` keeps the variable pointing at the live array — exactly
/// what the native backend does.
fn lower_array_push(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let array = operand(inst, 0)?;
    let value = operand(inst, 1)?;
    let value_repr = ctx.value_repr(value)?.clone();
    match value_repr {
        WasmRepr::I64(_) => {
            ctx.emit_load_value(array)?;
            ctx.emit_load_value(value)?;
            ctx.fb
                .ins("call $__rt_array_push_int", "append int (may reallocate)");
        }
        other => return Err(WasmError::Unsupported(format!("array_push of {:?}", other))),
    }
    // The runtime returned the (possibly reallocated) pointer: store it back into
    // the array operand value's local.
    ctx.emit_store_value(array)?;
    // And mirror it to the source slot so a later LoadLocal sees the live pointer.
    if let Some(slot) = value_source_slot(ctx, array) {
        let array_ref = ctx.value_repr(array)?.local_refs();
        let slot_ref = ctx.slot_repr(slot)?.local_refs();
        if array_ref.len() == 1 && slot_ref.len() == 1 {
            ctx.fb
                .ins(&format!("local.get {}", array_ref[0]), "reallocated array pointer");
            ctx.fb
                .ins(&format!("local.set {}", slot_ref[0]), "write back to the array slot");
        }
    }
    Ok(())
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
