//! Purpose:
//! Operand inspection helpers shared by several builtin families: literal operands, array storage tests and use scans.
//!
//! Called from:
//! - Sibling family modules under `super` (strings, arrays, classes, files, format).
//!
//! Key details:
//! - Nothing here emits code; it only reads the IR a lowerer or predicate is about to consume.

use super::*;

/// Whether `value` is an array literal built for one call and consumed by nothing else.
///
/// The EIR gives the literal `own=owned` and emits NO release for it: the call is expected to take
/// it. A lowering that only reads the array (a key, a length) must therefore give it back itself,
/// or every `array_key_first(["a" => 1])` strands one array. The test is deliberately narrow — an
/// allocating `array_new`/`hash_new` whose every other use is a store INTO it — because a value
/// that is also released, stored in a slot or passed elsewhere has an owner already.
pub(super) fn is_unconsumed_literal_array(ctx: &FnCtx, value: crate::ir::ValueId) -> bool {
    use crate::ir::{Op, Ownership, ValueDef};
    let Some(info) = ctx.function.value(value) else {
        return false;
    };
    if info.ownership != Ownership::Owned {
        return false;
    }
    let built_here = match info.def {
        ValueDef::Instruction { inst, .. } => ctx
            .function
            .instruction(inst)
            .is_some_and(|def| matches!(def.op, Op::ArrayNew | Op::HashNew)),
        _ => false,
    };
    if !built_here {
        return false;
    }
    let mut calls = 0;
    for candidate in &ctx.function.instructions {
        if !candidate.operands.contains(&value) {
            continue;
        }
        match candidate.op {
            Op::ArraySet | Op::HashSet | Op::ArrayPush if candidate.operands.first() == Some(&value) => {}
            Op::RuntimeCall => calls += 1,
            _ => return false,
        }
    }
    if ctx
        .function
        .blocks
        .iter()
        .any(|block| block.terminator.as_ref().is_some_and(|t| terminator_mentions(t, value)))
    {
        return false;
    }
    calls == 1
}

/// Whether a terminator names `value` among its operands or branch arguments.
pub(super) fn terminator_mentions(terminator: &crate::ir::Terminator, value: crate::ir::ValueId) -> bool {
    use crate::ir::Terminator;
    match terminator {
        Terminator::Br { args, .. } => args.contains(&value),
        Terminator::CondBr { cond, then_args, else_args, .. } => {
            *cond == value || then_args.contains(&value) || else_args.contains(&value)
        }
        Terminator::Switch { scrutinee, cases, default_args, .. } => {
            *scrutinee == value
                || default_args.contains(&value)
                || cases.iter().any(|case| case.args.contains(&value))
        }
        Terminator::Return { value: returned } => *returned == Some(value),
        Terminator::Throw { value: thrown } => *thrown == value,
        Terminator::GeneratorSuspend { key, value: yielded, resume_args, .. } => {
            *key == Some(value) || *yielded == Some(value) || resume_args.contains(&value)
        }
        Terminator::Fatal { .. } | Terminator::Unreachable => false,
    }
}

/// Recovers a literal string operand's text, or `None` when it is not a literal.
pub(super) fn literal_string_operand(
    function: &Function,
    module: &Module,
    value: crate::ir::ValueId,
) -> Option<String> {
    let defined = function.value(value)?;
    let crate::ir::ValueDef::Instruction { inst, .. } = defined.def else {
        return None;
    };
    let defining = function.instruction(inst)?;
    if defining.op != Op::ConstStr {
        return None;
    }
    let Some(Immediate::Data(id)) = defining.immediate else {
        return None;
    };
    module.data.strings.get(id.as_raw() as usize).cloned()
}

/// Whether `value` is read by anything other than an `echo` or an int-to-string conversion.
///
/// Those two only render the number, so a `false` that arrived as `0` changes the text and no
/// decision; every other reader (a comparison, a branch, a store that is read again) could take a
/// different path.
pub(super) fn function_uses_value_beyond_printing(function: &Function, value: ValueId) -> bool {
    use crate::ir::Terminator;
    if function.instructions.iter().any(|inst| {
        inst.operands.contains(&value) && !matches!(inst.op, Op::EchoValue | Op::IToStr)
    }) {
        return true;
    }
    function.blocks.iter().any(|block| match &block.terminator {
        Some(Terminator::Br { args, .. }) => args.contains(&value),
        Some(Terminator::CondBr {
            cond,
            then_args,
            else_args,
            ..
        }) => *cond == value || then_args.contains(&value) || else_args.contains(&value),
        Some(Terminator::Switch {
            scrutinee,
            cases,
            default_args,
            ..
        }) => {
            *scrutinee == value
                || default_args.contains(&value)
                || cases.iter().any(|case| case.args.contains(&value))
        }
        Some(Terminator::Return { value: returned }) => *returned == Some(value),
        Some(Terminator::Throw { value: thrown }) => *thrown == value,
        Some(Terminator::GeneratorSuspend {
            key,
            value: yielded,
            resume_args,
            ..
        }) => *key == Some(value) || *yielded == Some(value) || resume_args.contains(&value),
        _ => false,
    })
}

/// Returns whether `value` is an indexed array whose slots this module can read directly.
///
/// Everything in the array family below reads raw i64 slots, which is what an `array<int>`
/// stores. A string or mixed element array uses a different slot width and carries refcounted
/// payloads, so it is not served here rather than being read at the wrong stride.
///
/// `array<never>` is admitted alongside `array<int>`: it is the type of the empty array literal,
/// and an array that provably holds nothing is read at no stride at all — every operation here
/// answers from its length, which is zero.
pub(super) fn indexed_int_array(value: &crate::ir::Value) -> bool {
    value.ir_type == IrType::Heap(IrHeapKind::Array)
        && matches!(
            value.php_type.codegen_repr(),
            PhpType::Array(element) if matches!(*element, PhpType::Int | PhpType::Never)
        )
}

/// Validates `array_keys($list)` and `array_values($list)`, which both answer a fresh
/// `array<int>` built from an `array<int>`.
///
/// `array_keys` of a list is `[0, 1, ..., n-1]` because its keys ARE its positions, and
/// `array_values` of a list is a copy because re-indexing a list changes nothing. Both facts
/// hold only for the indexed representation, so a hash operand is refused.
/// Whether a one-array builtin's operand is a declared `array`: a boxed list or hash.
pub(super) fn boxed_array_operand(function: &Function, call: &Instruction) -> bool {
    let [operand] = call.operands.as_slice() else {
        return false;
    };
    function.value(*operand).is_some_and(|value| {
        value.ir_type == IrType::Heap(IrHeapKind::Mixed) && value.php_type.is_php_array()
    })
}

/// Whether a one-array builtin's operand is a concrete list of Mixed cells (`array<mixed>`).
///
/// Its elements carry php's per-type fold rules just like a declared `array`'s, so
/// `array_sum`/`array_product` box the list into a temporary cell and take the same walk.
pub(super) fn mixed_list_operand(function: &Function, call: &Instruction) -> bool {
    let [operand] = call.operands.as_slice() else {
        return false;
    };
    function.value(*operand).is_some_and(|value| {
        value.ir_type == IrType::Heap(IrHeapKind::Array)
            && matches!(
                value.php_type.codegen_repr(),
                PhpType::Array(element) if element.codegen_repr() == PhpType::Mixed
            )
    })
}

/// Whether a runtime-call operand is a hash rather than an indexed array.
pub(super) fn hash_operand(ctx: &FnCtx, value: crate::ir::ValueId) -> bool {
    matches!(
        ctx.function.value(value).map(|value| value.ir_type),
        Some(IrType::Heap(IrHeapKind::Hash))
    )
}

/// The element type of an `array<T>` result, if it is one.
pub(super) fn element_php_type(php_type: &PhpType) -> Option<PhpType> {
    match php_type.codegen_repr() {
        PhpType::Array(element) => Some(*element),
        _ => None,
    }
}

/// Returns whether `value` is the constant integer zero.
pub(super) fn literal_zero(function: &Function, value: crate::ir::ValueId) -> bool {
    let Some(defined) = function.value(value) else {
        return false;
    };
    let crate::ir::ValueDef::Instruction { inst, .. } = defined.def else {
        return false;
    };
    let Some(defining) = function.instruction(inst) else {
        return false;
    };
    defining.op == crate::ir::Op::ConstI64
        && matches!(defining.immediate, Some(crate::ir::Immediate::I64(0)))
}

/// Pushes one integral builtin argument, coercing a BOXED operand PHP's way first.
///
/// A concrete `I64` operand is pushed as it is. A Mixed one goes through
/// `__rt_mixed_arg_int`, which is php-src's conversion at a declared `int` parameter: it
/// truncates a float after a `Deprecated`, converts a wholly numeric string, turns `null` into 0
/// after a different `Deprecated` naming the parameter, and raises a `TypeError` for everything
/// with no conversion at all.
pub(in crate::codegen_wasm) fn emit_int_operand(ctx: &mut FnCtx, inst: &Instruction, index: usize) -> Result<()> {
    let Some((name, parameter, position)) =
        crate::codegen_wasm::capability::runtime_call_int_operand_coercion(ctx.function, inst, index)
    else {
        return ctx.emit_load_value(operand(inst, index)?);
    };
    ctx.emit_load_value(operand(inst, index)?)?;
    crate::codegen_wasm::inst::emit_argument_coercion_names(ctx, name, &parameter, position)?;
    ctx.fb.ins(
        "call $__rt_mixed_arg_int",
        "PHP's implicit coercion at a declared int parameter",
    );
    Ok(())
}

/// The literal value behind an operand, when a `ConstI64` defines it.
pub(super) fn constant_i64_operand(function: &Function, value: crate::ir::ValueId) -> Option<i64> {
    let defining = function
        .instructions
        .iter()
        .find(|candidate| candidate.result == Some(value))?;
    match (defining.op, defining.immediate.as_ref()) {
        (Op::ConstI64, Some(Immediate::I64(constant))) => Some(*constant),
        _ => None,
    }
}
