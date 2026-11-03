//! Purpose:
//! Lowering and shape predicates for the `bc*` arbitrary-precision builtins.
//!
//! Called from:
//! - `super::lower_direct_builtin` and `super::direct_builtin_shape_issue` dispatch to it.
//! - `crate::codegen_wasm::capability` reads its `*_shape_issue` predicates for the static audit.

use super::*;

/// Validates the four decimal-arithmetic builtins.
///
/// The operands are STRINGS in php's own contract, not numbers — that is the whole point of
/// bcmath — so a caller that passes an int is passing something php would coerce first, and
/// the coercion carries its own diagnostics. An exact `Str` is required rather than converted
/// here, the same rule `sprintf`'s `%s` follows.
///
/// The optional scale must be a literal INT when present. It is not a compile-time constant in
/// the answer — the runtime reads it — but php's own type is `?int`, and a null would mean
/// "use the default scale", which is a second code path no example needs.
pub(super) fn bcmath_shape_issue(
    function: &Function,
    call: &Instruction,
    target: RuntimeFnId,
) -> Option<String> {
    if target == RuntimeFnId::BcScale {
        if call.operands.len() > 1 {
            return Some(format!(
                "bcscale takes at most one scale, got {}",
                call.operands.len()
            ));
        }
        if let Some(scale) = call.operands.first().and_then(|value| function.value(*value)) {
            if scale.ir_type != IrType::I64 || scale.php_type.codegen_repr() != PhpType::Int {
                return Some(format!(
                    "bcscale takes an int scale, not {:?}/{:?}",
                    scale.ir_type,
                    scale.php_type.codegen_repr()
                ));
            }
        }
        if call.result_type != IrType::I64 {
            return Some(format!(
                "bcscale answers the previous scale as an int, not {:?}",
                call.result_type
            ));
        }
        return None;
    }

    if call.operands.len() < 2 || call.operands.len() > 3 {
        return Some(format!(
            "{target:?} takes two decimal strings and an optional scale, got {} operands",
            call.operands.len()
        ));
    }
    for index in 0..2 {
        let Some(value) = function.value(call.operands[index]) else {
            return Some("operand is missing from the value table".to_string());
        };
        if value.ir_type != IrType::Str || value.php_type.codegen_repr() != PhpType::Str {
            return Some(format!(
                "{target:?} operand #{} is {:?}/{:?}, not the decimal STRING bcmath reads",
                index + 1,
                value.ir_type,
                value.php_type.codegen_repr()
            ));
        }
    }
    if let Some(scale) = call.operands.get(2).and_then(|value| function.value(*value)) {
        if scale.ir_type != IrType::I64 || scale.php_type.codegen_repr() != PhpType::Int {
            return Some(format!(
                "{target:?} scale is {:?}/{:?}, not an int",
                scale.ir_type,
                scale.php_type.codegen_repr()
            ));
        }
    }
    let want = if target == RuntimeFnId::BcDivmod {
        IrType::Heap(IrHeapKind::Array)
    } else {
        IrType::Str
    };
    if call.result.is_none() || call.result_type != want {
        return Some(format!(
            "{target:?} answers {want:?}, not {:?}",
            call.result_type
        ));
    }
    None
}

/// Loads a scale argument and raises php's `ValueError` unless it lies in `[0, INT_MAX]`.
///
/// The scale is narrowed to an i32 right after, so an unchecked 2147483648 wrapped negative and a
/// negative one was read as a huge count. Leaves the validated i64 on the stack.
pub(super) fn emit_checked_bc_scale(ctx: &mut FnCtx, value: crate::ir::ValueId, failure_code: i32) -> Result<()> {
    let scale = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I64);
    ctx.emit_load_value(value)?;
    ctx.fb.ins(&format!("local.tee {scale}"), "keep the scale for the range check");
    ctx.fb.ins("i64.const 2147483647", "php's upper bound");
    ctx.fb.ins("i64.gt_u", "negative or past INT_MAX (unsigned compare covers both)?");
    ctx.fb.ins("if", "php rejects a scale outside [0, INT_MAX]");
    crate::codegen_wasm::inst::emit_runtime_failure(ctx, failure_code, "bcmath scale out of range");
    ctx.fb.ins("end", "end scale range guard");
    ctx.fb.ins(&format!("local.get {scale}"), "the validated scale");
    Ok(())
}

/// Pushes the scale a bcmath call works at: its own argument, or the module's default.
pub(super) fn emit_bc_scale_operand(ctx: &mut FnCtx, inst: &Instruction, index: usize) -> Result<()> {
    match inst.operands.get(index) {
        Some(value) => {
            emit_checked_bc_scale(ctx, *value, crate::codegen_wasm::bcmath::SCALE_FAILURE_CODE)?;
            ctx.fb.ins("i32.wrap_i64", "the scale this call asked for");
        }
        None => ctx
            .fb
            .ins("global.get $__bc_scale", "the scale bcscale() last set"),
    }
    Ok(())
}

/// Raises php's `ValueError` when a bcmath helper reported a malformed operand, and its
/// `DivisionByZeroError` when it reported a zero divisor.
///
/// The helper cannot raise: a raise site is emitted code, not a runtime function, because it
/// has to build the throwable the module's own `catch` clauses will match. So the status comes
/// back as a value and the branch lives here.
pub(super) fn emit_bc_status_guard(ctx: &mut FnCtx, status: &str, divides: bool) {
    ctx.fb.ins(&format!("local.get {status}"), "the helper's verdict");
    ctx.fb.ins("i32.const 1", "1 = an operand php would reject");
    ctx.fb.ins("i32.eq", "malformed?");
    ctx.fb.ins("if", "bcmath rejects an operand that is not well-formed");
    crate::codegen_wasm::inst::emit_runtime_failure(
        ctx,
        crate::codegen_wasm::bcmath::MALFORMED_FAILURE_CODE,
        "bcmath malformed operand",
    );
    ctx.fb.ins("end", "end the well-formed guard");
    if divides {
        ctx.fb.ins(&format!("local.get {status}"), "the helper's verdict");
        ctx.fb.ins("i32.const 2", "2 = a zero divisor");
        ctx.fb.ins("i32.eq", "division by zero?");
        ctx.fb.ins("if", "bcdivmod by zero is php's DivisionByZeroError");
        crate::codegen_wasm::inst::emit_runtime_failure(ctx, 1, "bcdivmod by zero");
        ctx.fb.ins("end", "end the zero-divisor guard");
    }
}

/// Lowers the four decimal-arithmetic builtins onto the digit runtime.
pub(super) fn lower_bcmath(ctx: &mut FnCtx, inst: &Instruction, target: RuntimeFnId) -> Result<()> {
    use crate::codegen_wasm::wat::ValType;

    if target == RuntimeFnId::BcScale {
        // php answers the PREVIOUS scale, which is what makes save/restore work. With no
        // argument it answers the current one and changes nothing.
        let previous = ctx.fresh_temp(ValType::I32);
        ctx.fb
            .ins("global.get $__bc_scale", "the scale in force");
        ctx.fb.ins(&format!("local.set {previous}"), "keep it to answer with");
        if let Some(value) = inst.operands.first() {
            emit_checked_bc_scale(ctx, *value, crate::codegen_wasm::bcmath::BCSCALE_FAILURE_CODE)?;
            ctx.fb.ins("i32.wrap_i64", "the new scale");
            ctx.fb.ins("global.set $__bc_scale", "adopt it");
        }
        ctx.fb
            .ins(&format!("(i64.extend_i32_u (local.get {previous}))"), "php answers the old scale");
        return store_result(ctx, inst);
    }

    let ptr = ctx.fresh_temp(ValType::I32);
    let len = ctx.fresh_temp(ValType::I64);
    let status = ctx.fresh_temp(ValType::I32);

    if target != RuntimeFnId::BcDivmod {
        for index in 0..2 {
            ctx.emit_load_value(operand(inst, index)?)?;
            ctx.fb.ins("i32.wrap_i64", "the helper takes an i32 length");
        }
        emit_bc_scale_operand(ctx, inst, 2)?;
        ctx.fb.ins(
            if target == RuntimeFnId::BcAdd {
                "call $__rt_bcadd"
            } else {
                "call $__rt_bcmul"
            },
            "decimal arithmetic in digits",
        );
        ctx.fb.ins(&format!("local.set {status}"), "the verdict");
        ctx.fb.ins(&format!("local.set {len}"), "the answer's length");
        ctx.fb.ins(&format!("local.set {ptr}"), "the answer's bytes");
        emit_bc_status_guard(ctx, &status, false);
        ctx.fb.ins(&format!("local.get {ptr}"), "the decimal string");
        ctx.fb.ins(&format!("local.get {len}"), "its length");
        return store_result(ctx, inst);
    }

    // `bcdivmod` answers BOTH halves, so the helper runs twice — once for the quotient and
    // once for the remainder. Running it twice rather than returning a pair keeps the runtime
    // signature to the three values every other helper here uses, and the division is over a
    // handful of digits.
    let array = ctx.fresh_temp(ValType::I32);
    ctx.fb.ins(
        &format!("(local.set {array} (call $__rt_array_new (i64.const 2) (i64.const 16)))"),
        "the quotient and the remainder",
    );
    for want_remainder in [0, 1] {
        ctx.fb.ins(&format!("local.get {array}"), "the pair so far");
        for index in 0..2 {
            ctx.emit_load_value(operand(inst, index)?)?;
            ctx.fb.ins("i32.wrap_i64", "the helper takes an i32 length");
        }
        emit_bc_scale_operand(ctx, inst, 2)?;
        ctx.fb.ins(
            &format!("i32.const {want_remainder}"),
            if want_remainder == 0 {
                "the quotient, always at scale 0"
            } else {
                "the remainder, at the call's scale"
            },
        );
        ctx.fb.ins("call $__rt_bcdivmod", "decimal division in digits");
        ctx.fb.ins(&format!("local.set {status}"), "the verdict");
        ctx.fb.ins(&format!("local.set {len}"), "the answer's length");
        ctx.fb.ins(&format!("local.set {ptr}"), "the answer's bytes");
        emit_bc_status_guard(ctx, &status, true);
        ctx.fb.ins(&format!("local.get {ptr}"), "the half just computed");
        ctx.fb.ins(&format!("local.get {len}"), "its length");
        ctx.fb.ins("call $__rt_array_push_str", "append it");
        ctx.fb.ins(&format!("local.set {array}"), "keep what the push answered");
    }
    ctx.fb.ins(&format!("local.get {array}"), "the finished pair");
    store_result(ctx, inst)
}
