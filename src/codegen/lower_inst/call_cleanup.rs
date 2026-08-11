//! Purpose:
//! Plans and releases temporary or borrowed Mixed call arguments.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` and sibling lowering helpers.
//!
//! Key details:
//! - Preserves EIR ownership, ABI ordering, runtime symbols, and target-aware lowering.

use super::*;

/// Plans scalar Mixed arguments that can be borrowed on the caller stack for a direct callee.
pub(super) fn plan_borrowed_stack_mixed_args(
    ctx: &FunctionContext<'_>,
    callee: &Function,
    args: &[ValueId],
    param_types: &[PhpType],
    ref_params: &[bool],
) -> Result<Vec<BorrowedStackMixedArg>> {
    let mut borrowed_args = Vec::new();
    for (index, (value, param_ty)) in args.iter().zip(param_types.iter()).enumerate() {
        if ref_params[index]
            || callee.params[index].variadic
            || param_ty.codegen_repr() != PhpType::Mixed
        {
            continue;
        }
        let source_ty = ctx.raw_value_php_type(*value)?.codegen_repr();
        if !matches!(source_ty, PhpType::Int | PhpType::Bool) {
            continue;
        }
        if !callee_mixed_param_is_truthiness_only(callee, index) {
            continue;
        }
        borrowed_args.push(BorrowedStackMixedArg {
            param_index: index,
            offset: borrowed_args.len() * BORROWED_MIXED_ARG_CELL_BYTES,
            source_ty,
        });
    }
    Ok(borrowed_args)
}

/// Returns true when a Mixed parameter is only loaded for boolean conversion.
pub(super) fn callee_mixed_param_is_truthiness_only(callee: &Function, param_index: usize) -> bool {
    let slot = LocalSlotId::from_raw(param_index as u32);
    let mut loaded_values = Vec::new();
    for inst in &callee.instructions {
        match (&inst.op, &inst.immediate) {
            (Op::LoadLocal, Some(Immediate::LocalSlot(candidate))) if *candidate == slot => {
                let Some(result) = inst.result else {
                    return false;
                };
                loaded_values.push(result);
            }
            (_, Some(Immediate::LocalSlot(candidate))) if *candidate == slot => return false,
            _ => {}
        }
    }
    loaded_values
        .iter()
        .all(|value| callee_value_is_only_truthiness_operand(callee, *value))
}

/// Returns true when every use of `value` feeds a non-escaping boolean conversion.
pub(super) fn callee_value_is_only_truthiness_operand(callee: &Function, value: ValueId) -> bool {
    for inst in &callee.instructions {
        if !inst.operands.iter().any(|operand| *operand == value) {
            continue;
        }
        if !matches!(inst.op, Op::IsTruthy | Op::MixedCastBool) {
            return false;
        }
    }
    !callee_terminator_uses_value(callee, value)
}

/// Returns true when any terminator directly consumes `value`.
pub(super) fn callee_terminator_uses_value(callee: &Function, value: ValueId) -> bool {
    callee
        .blocks
        .iter()
        .filter_map(|block| block.terminator.as_ref())
        .any(|terminator| terminator_uses_value(terminator, value))
}

/// Returns true when one terminator directly consumes `value`.
pub(super) fn terminator_uses_value(terminator: &Terminator, value: ValueId) -> bool {
    match terminator {
        Terminator::Br { args, .. } => args.contains(&value),
        Terminator::CondBr {
            cond,
            then_args,
            else_args,
            ..
        } => *cond == value || then_args.contains(&value) || else_args.contains(&value),
        Terminator::Switch {
            scrutinee,
            cases,
            default_args,
            ..
        } => {
            *scrutinee == value
                || default_args.contains(&value)
                || cases.iter().any(|case| case.args.contains(&value))
        }
        Terminator::Return {
            value: Some(return_value),
        } => *return_value == value,
        Terminator::Return { value: None } => false,
        Terminator::Throw { value: thrown } => *thrown == value,
        Terminator::Fatal { .. } | Terminator::Unreachable => false,
        Terminator::GeneratorSuspend {
            key,
            value: yielded,
            resume_args,
            ..
        } => {
            key.is_some_and(|key| key == value)
                || yielded.is_some_and(|yielded| yielded == value)
                || resume_args.contains(&value)
        }
    }
}

/// Writes a borrowed stack Mixed cell for a scalar argument and returns its address as the result.
pub(super) fn emit_borrowed_stack_mixed_arg_cell(
    ctx: &mut FunctionContext<'_>,
    borrowed: &BorrowedStackMixedArg,
    base_offset: usize,
) {
    let payload_reg = abi::secondary_scratch_reg(ctx.emitter);
    let cell_reg = abi::symbol_scratch_reg(ctx.emitter);
    let result_reg = abi::int_result_reg(ctx.emitter);
    ctx.emitter
        .instruction(&format!("mov {}, {}", payload_reg, result_reg)); // preserve the scalar payload before writing the borrowed Mixed tag
    abi::emit_temporary_stack_address(ctx.emitter, cell_reg, base_offset + borrowed.offset);
    abi::emit_load_int_immediate(
        ctx.emitter,
        result_reg,
        runtime_value_tag(&borrowed.source_ty) as i64,
    );
    abi::emit_store_to_address(ctx.emitter, result_reg, cell_reg, 0);
    abi::emit_store_to_address(ctx.emitter, payload_reg, cell_reg, 8);
    abi::emit_store_zero_to_address(ctx.emitter, cell_reg, 16);
    move_reg_to_int_result(ctx, cell_reg);
}

/// Plans temporary Mixed call arguments that must remain alive until after the callee returns.
pub(super) fn plan_call_arg_temp_cleanups(
    ctx: &FunctionContext<'_>,
    args: &[ValueId],
    param_types: &[PhpType],
    ref_params: &[bool],
    borrowed_stack_mixed_args: &[BorrowedStackMixedArg],
) -> Result<Vec<CallArgTempCleanup>> {
    let mut cleanups = Vec::new();
    for (index, (value, param_ty)) in args.iter().zip(param_types.iter()).enumerate() {
        if ref_params[index]
            || borrowed_stack_mixed_args
                .iter()
                .any(|borrowed| borrowed.param_index == index)
        {
            continue;
        }
        let source_ty = ctx.raw_value_php_type(*value)?;
        if direct_call_arg_creates_mixed_temp(&source_ty, param_ty) {
            cleanups.push(CallArgTempCleanup {
                param_index: index,
                offset: cleanups.len() * 16,
                ty: PhpType::Mixed,
            });
        } else if direct_call_arg_splits_borrowed_array(ctx, *value, &source_ty, param_ty)? {
            cleanups.push(CallArgTempCleanup {
                param_index: index,
                offset: cleanups.len() * 16,
                ty: widened_array_temp_type(&source_ty),
            });
        }
    }
    Ok(cleanups)
}

/// Returns whether argument materialization allocates a caller-owned Mixed box.
pub(super) fn direct_call_arg_creates_mixed_temp(source_ty: &PhpType, param_ty: &PhpType) -> bool {
    matches!(param_ty.codegen_repr(), PhpType::Mixed)
        && !matches!(source_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
}

/// Returns whether widening a typed array for a gradual `array` parameter must COPY it first.
///
/// `__rt_array_to_mixed` CONSUMES an owner slot: it splits through
/// `__rt_array_ensure_unique`, which rewrites the element slots in place when the refcount is
/// 1 and only clones when the array is visibly shared. Handing it a BORROWED array therefore
/// rewrote the caller's own array — `f($pts)` with `function f(array $a)` and `$pts` an array
/// of objects left `$pts[0]->x` reading a boxed cell as a raw object pointer AFTER the call,
/// on data the callee never touched, with no diagnostic. An owned temporary is left alone: it
/// has no other reader, so converting it in place is both correct and free.
fn direct_call_arg_splits_borrowed_array(
    ctx: &FunctionContext<'_>,
    value: ValueId,
    source_ty: &PhpType,
    param_ty: &PhpType,
) -> Result<bool> {
    if !argument_widens_typed_array(source_ty, param_ty) {
        return Ok(false);
    }
    Ok(ctx.value_ownership(value)? != Ownership::Owned)
}

/// Returns whether this argument boundary widens a typed array into Mixed element slots.
pub(super) fn argument_widens_typed_array(source_ty: &PhpType, param_ty: &PhpType) -> bool {
    let (PhpType::Array(param_elem), PhpType::Array(source_elem)) =
        (param_ty.codegen_repr(), source_ty.codegen_repr())
    else {
        return false;
    };
    param_elem.codegen_repr() == PhpType::Mixed && source_elem.codegen_repr() != PhpType::Mixed
}

/// The type of the widened copy, used to pick its release helper.
fn widened_array_temp_type(source_ty: &PhpType) -> PhpType {
    match source_ty.codegen_repr() {
        PhpType::AssocArray { key, .. } => PhpType::AssocArray {
            key,
            value: Box::new(PhpType::Mixed),
        },
        _ => PhpType::Array(Box::new(PhpType::Mixed)),
    }
}

/// Saves the current pointer result into the reserved call-argument cleanup area.
pub(super) fn save_call_arg_temp_cleanup(
    ctx: &mut FunctionContext<'_>,
    cleanup: &CallArgTempCleanup,
    arg_temp_bytes: usize,
) {
    let scratch = abi::symbol_scratch_reg(ctx.emitter);
    let offset = arg_temp_bytes + cleanup.offset;
    abi::emit_temporary_stack_address(ctx.emitter, scratch, offset);
    abi::emit_store_to_address(ctx.emitter, abi::int_result_reg(ctx.emitter), scratch, 0);
}

/// Releases caller-owned temporary arguments after the call result has been saved.
pub(super) fn emit_call_arg_temp_cleanups(
    ctx: &mut FunctionContext<'_>,
    call_args: &CallArgMaterialization,
    result: Option<ValueId>,
) -> Result<()> {
    if call_args.cleanup_slots.is_empty() {
        return Ok(());
    }
    let result_alias = call_result_can_alias_mixed_temp(ctx, result)?;
    for cleanup in &call_args.cleanup_slots {
        abi::emit_load_temporary_stack_slot(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            cleanup.offset,
        );
        let skip_cleanup_label = if let Some(result) = result_alias {
            let label = ctx.next_label("call_arg_temp_cleanup_result_alias");
            emit_branch_if_cleanup_temp_aliases_result(ctx, result, &label)?;
            Some(label)
        } else {
            None
        };
        abi::emit_decref_if_refcounted(ctx.emitter, &cleanup.ty);
        if let Some(label) = skip_cleanup_label {
            ctx.emitter.label(&label);
        }
    }
    abi::emit_release_temporary_stack(ctx.emitter, call_args.cleanup_bytes);
    Ok(())
}

/// Returns the result value when it can alias a caller-owned temporary Mixed argument.
pub(super) fn call_result_can_alias_mixed_temp(
    ctx: &FunctionContext<'_>,
    result: Option<ValueId>,
) -> Result<Option<ValueId>> {
    let Some(result) = result else {
        return Ok(None);
    };
    if matches!(
        ctx.value_php_type(result)?.codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        return Ok(Some(result));
    }
    Ok(None)
}

/// Skips temp cleanup when a callee returned the same Mixed cell that was passed as an argument.
pub(super) fn emit_branch_if_cleanup_temp_aliases_result(
    ctx: &mut FunctionContext<'_>,
    result: ValueId,
    skip_label: &str,
) -> Result<()> {
    let cleanup_reg = abi::int_result_reg(ctx.emitter);
    let result_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(result, result_reg)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("cmp {}, {}", cleanup_reg, result_reg)); // compare the temporary Mixed cell with the saved call result
            ctx.emitter.instruction(&format!("b.eq {}", skip_label));           // keep the temp alive when ownership moved to the result
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("cmp {}, {}", cleanup_reg, result_reg)); // compare the temporary Mixed cell with the saved call result
            ctx.emitter.instruction(&format!("je {}", skip_label));             // keep the temp alive when ownership moved to the result
        }
    }
    Ok(())
}

/// Releases borrowed stack Mixed cells after heap temp cleanups and before by-ref cells.
pub(super) fn emit_borrowed_stack_mixed_arg_release(
    ctx: &mut FunctionContext<'_>,
    call_args: &CallArgMaterialization,
) {
    if call_args.borrowed_stack_arg_bytes == 0 {
        return;
    }
    abi::emit_release_temporary_stack(ctx.emitter, call_args.borrowed_stack_arg_bytes);
}

/// Converts the currently loaded indexed-array argument into boxed Mixed slots.
pub(super) fn emit_loaded_indexed_array_to_mixed(ctx: &mut FunctionContext<'_>, source_elem_ty: &PhpType) {
    let value_tag = runtime_value_tag(source_elem_ty) as i64;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x1", value_tag);
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rsi", value_tag);
            ctx.emitter.instruction("mov rdi, rax");                            // pass the loaded indexed-array argument to the Mixed conversion helper
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_to_mixed");
}

/// Converts the currently loaded associative-array argument into boxed Mixed values.
pub(super) fn emit_loaded_assoc_array_to_mixed(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {}
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // pass the loaded associative-array argument to the Mixed conversion helper
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");
}

