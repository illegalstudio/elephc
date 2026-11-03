//! Purpose:
//! Lowering and shape predicates for the clock builtins (`time` and `microtime`).
//!
//! Called from:
//! - `super::lower_direct_builtin` and `super::direct_builtin_shape_issue` dispatch to it.
//! - `crate::codegen_wasm::capability` reads its `*_shape_issue` predicates for the static audit.
//! - `crate::codegen_wasm::clock` owns the WAT helpers these call.

use super::*;

/// Validates `time()` and `microtime($as_float)`.
///
/// `microtime`'s flag decides its RETURN TYPE — a float when true, a string otherwise — so a
/// flag known only at runtime would need both answers boxed behind a branch. php's own default
/// is the string form, and a literal flag is what every call in the corpus passes, so a
/// computed one is refused rather than given a second code path nothing exercises.
pub(super) fn clock_shape_issue(
    function: &Function,
    call: &Instruction,
    target: RuntimeFnId,
) -> Option<String> {
    if target == RuntimeFnId::Time {
        if !call.operands.is_empty() {
            return Some(format!("time takes nothing, got {} operands", call.operands.len()));
        }
        if call.result_type != IrType::I64 {
            return Some(format!("time answers an int, not {:?}", call.result_type));
        }
        return None;
    }
    if call.operands.len() > 1 {
        return Some(format!(
            "microtime takes at most one flag, got {}",
            call.operands.len()
        ));
    }
    if microtime_as_float(function, call).is_none() {
        return Some(
            "microtime needs a literal $as_float: the flag picks the answer's TYPE, and a \
             runtime one would need the float and the string both built and boxed"
                .to_string(),
        );
    }
    if call.result.is_none() {
        return Some("microtime must materialize its result".to_string());
    }
    None
}

/// Whether this `microtime` call asks for the float form, or `None` when the flag is computed.
pub(super) fn microtime_as_float(function: &Function, call: &Instruction) -> Option<bool> {
    let Some(operand) = call.operands.first() else {
        return Some(false); // php's default is the string form
    };
    let inst = function
        .values
        .get(operand.as_raw() as usize)
        .and_then(|value| match value.def {
            crate::ir::ValueDef::Instruction { inst, .. } => function.instruction(inst),
            _ => None,
        })?;
    if inst.op != Op::ConstBool {
        return None;
    }
    match inst.immediate {
        Some(Immediate::Bool(flag)) => Some(flag),
        _ => None,
    }
}

/// Lowers `time()` and `microtime()` onto the WASI clock.
pub(super) fn lower_clock(ctx: &mut FnCtx, inst: &Instruction, target: RuntimeFnId) -> Result<()> {
    if target == RuntimeFnId::Time {
        ctx.fb.ins("call $__rt_time", "whole seconds since the epoch");
        return store_result(ctx, inst);
    }
    let as_float = microtime_as_float(ctx.function, inst)
        .ok_or_else(|| WasmError::Unsupported("microtime flag is not a literal".into()))?;
    // The flag is a constant, so only one of the two forms is ever built — but the operand is
    // still loaded and dropped, because a discarded argument may be a call with effects.
    if !inst.operands.is_empty() {
        ctx.emit_load_value(operand(inst, 0)?)?;
        ctx.fb.ins("drop", "the flag was read at compile time");
    }
    // php declares `mixed`, so the call site may want the value boxed or bare depending on
    // what the checker settled its slot to. Both forms answer both shapes.
    let boxed = inst.result_type == IrType::Heap(IrHeapKind::Mixed);
    if as_float {
        ctx.fb.ins("call $__rt_microtime_float", "seconds as a double");
        if boxed {
            // `__rt_mixed_from_value` takes (tag, lo, hi) in that order and a float cell
            // carries the double's BITS in `lo`, so the value has to be spilled before the
            // tag can go under it.
            let bits = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I64);
            ctx.fb.ins("i64.reinterpret_f64", "the cell carries the bits");
            ctx.fb.ins(&format!("local.set {bits}"), "spill them");
            ctx.fb.ins("i64.const 2", "tag 2 = float");
            ctx.fb.ins(&format!("local.get {bits}"), "the double's bits");
            ctx.fb.ins("i64.const 0", "no high word");
            ctx.fb.ins("call $__rt_mixed_from_value", "box the float");
        }
    } else {
        ctx.fb
            .ins("call $__rt_microtime_string", "php's `0.ffffffff ssss` form");
        if boxed {
            let len = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I64);
            let ptr = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I32);
            ctx.fb.ins(&format!("local.set {len}"), "its length (top of the pair)");
            ctx.fb.ins(&format!("local.set {ptr}"), "its bytes");
            ctx.fb.ins("i64.const 1", "tag 1 = string");
            ctx.fb
                .ins(&format!("(i64.extend_i32_u (local.get {ptr}))"), "the bytes");
            ctx.fb.ins(&format!("local.get {len}"), "the length");
            ctx.fb.ins("call $__rt_mixed_from_value", "box the string");
        }
    }
    store_result(ctx, inst)
}
