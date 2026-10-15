//! Purpose:
//! Network-flavoured builtins that need no runtime library: protocol lookups and IPv4 conversion shape checks.
//!
//! Called from:
//! - `super::lower_direct_builtin` and `super::direct_builtin_shape_issue` dispatch to it.
//! - `crate::codegen_wasm::capability` reads its `*_shape_issue` predicates for the static audit.

use super::*;

/// Validates `getprotobyname($name)` / `getprotobynumber($number)`.
///
/// Both answer a BOXED union, because php's failure value has a different type from the
/// success value and `getprotobyname("ip")` is `0` — a value `== false` but not `=== false`,
/// which is the comparison programs actually write. A bare int or string result would lose
/// that distinction before the program could ask.
pub(super) fn protocol_lookup_shape_issue(
    function: &Function,
    call: &Instruction,
    target: RuntimeFnId,
) -> Option<String> {
    let by_name = target == RuntimeFnId::Getprotobyname;
    let [operand] = call.operands.as_slice() else {
        return Some(format!(
            "{target:?} expects one operand, got {}",
            call.operands.len()
        ));
    };
    let Some(value) = function.value(*operand) else {
        return Some("operand is missing from the value table".to_string());
    };
    let operand_ok = if by_name {
        value.ir_type == IrType::Str
    } else {
        value.ir_type == IrType::I64 && value.php_type.codegen_repr() == PhpType::Int
    };
    if !operand_ok {
        return Some(format!(
            "{target:?} operand {:?}/{:?} is not the {} it looks up",
            value.ir_type,
            value.php_type.codegen_repr(),
            if by_name { "name" } else { "number" }
        ));
    }
    if call.result.is_none() || call.result_type != IrType::Heap(IrHeapKind::Mixed) {
        return Some(format!(
            "{target:?} answers a boxed union, not {:?}",
            call.result_type
        ));
    }
    None
}

/// Lowers `getprotobyname()` / `getprotobynumber()` onto the carried registry.
pub(super) fn lower_protocol_lookup(ctx: &mut FnCtx, inst: &Instruction, target: RuntimeFnId) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb.ins(
        if target == RuntimeFnId::Getprotobyname {
            "call $__rt_getprotobyname"
        } else {
            "call $__rt_getprotobynumber"
        },
        "the carried protocol registry",
    );
    store_result(ctx, inst)
}

/// Validates one IPv4 conversion against the exact storage its helper reads and answers.
///
/// `long2ip` takes an int and answers a bare string; the other three take a string and answer a
/// BOXED `int|false` / `string|false`, because php's failure value is a different type from the
/// success value.
pub(super) fn inet_shape_issue(
    module: &Module,
    function: &Function,
    call: &Instruction,
    target: RuntimeFnId,
) -> Option<String> {
    // The helpers write through the string runtime, which the command runtime owns.
    if !module.functions.iter().any(|candidate| candidate.flags.is_main) {
        return Some("the IPv4 conversions are part of the command runtime".to_string());
    }
    let [operand] = call.operands.as_slice() else {
        return Some(format!(
            "{target:?} expects one operand, got {}",
            call.operands.len()
        ));
    };
    let Some(value) = function.value(*operand) else {
        return Some("operand is missing from the value table".to_string());
    };
    let want_int = target == RuntimeFnId::Long2ip;
    let operand_ok = if want_int {
        value.ir_type == IrType::I64 && value.php_type.codegen_repr() == PhpType::Int
    } else {
        value.ir_type == IrType::Str
    };
    if !operand_ok {
        return Some(format!(
            "{target:?} operand {:?}/{:?} is not the {} it reads",
            value.ir_type,
            value.php_type.codegen_repr(),
            if want_int { "int" } else { "string" }
        ));
    }
    if call.result.is_none() {
        return Some(format!("{target:?} must materialize its result"));
    }
    let result_ok = if want_int {
        call.result_type == IrType::Str
    } else {
        call.result_type == IrType::Heap(IrHeapKind::Mixed)
    };
    if !result_ok {
        return Some(format!(
            "{target:?} result {:?} is not the {} it answers",
            call.result_type,
            if want_int { "string" } else { "boxed union" }
        ));
    }
    None
}
