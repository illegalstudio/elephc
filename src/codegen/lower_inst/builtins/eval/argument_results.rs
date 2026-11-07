//! Purpose:
//! Materializes eval call arguments and converts bridge results.
//!
//! Called from:
//! - The eval lowering facade and sibling eval support modules.
//!
//! Key details:
//! - Scratch offsets, Mixed ownership, and target register ordering are unchanged.

use super::*;

/// Returns the aligned scratch size for an eval-declared function call.
pub(super) fn eval_function_call_stack_bytes(arg_count: usize) -> usize {
    let bytes = EVAL_STACK_BYTES + arg_count * 8;
    (bytes + 15) & !15
}

/// Returns the aligned scratch size for an eval dynamic method-call argument pack.
pub(super) fn eval_method_call_stack_bytes(arg_count: usize) -> usize {
    let bytes = EVAL_STACK_BYTES + 8 + arg_count * 8;
    (bytes + 15) & !15
}

/// Returns the aligned scratch size for an eval dynamic static-method call.
pub(super) fn eval_static_method_call_stack_bytes(arg_count: usize) -> usize {
    let bytes = EVAL_STACK_BYTES + 8 + arg_count * 8;
    (bytes + 15) & !15
}

/// Stores positional operands as boxed Mixed cells for the eval function-call ABI.
pub(super) fn store_eval_function_call_args(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    args_offset: usize,
) -> Result<()> {
    store_eval_function_call_operands(ctx, &inst.operands, args_offset)
}

/// Stores one operand slice as boxed Mixed cells for eval positional-call ABIs.
pub(super) fn store_eval_function_call_operands(
    ctx: &mut FunctionContext<'_>,
    operands: &[ValueId],
    args_offset: usize,
) -> Result<()> {
    for (index, operand) in operands.iter().enumerate() {
        let ty = ctx.load_value_to_result(*operand)?.codegen_repr();
        if !matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
            emit_box_current_value_as_mixed(ctx.emitter, &ty);
        }
        let result_reg = abi::int_result_reg(ctx.emitter);
        abi::emit_store_to_sp(ctx.emitter, result_reg, args_offset + index * 8);
    }
    Ok(())
}

/// Stores a count-prefixed positional argument pack for the eval method-call ABI.
pub(super) fn store_eval_method_call_arg_pack(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    args_offset: usize,
) -> Result<()> {
    let arg_count = inst.operands.len().saturating_sub(1);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, arg_count as i64);
    abi::emit_store_to_sp(ctx.emitter, result_reg, args_offset);
    for (index, operand) in inst.operands.iter().skip(1).enumerate() {
        let ty = ctx.load_value_to_result(*operand)?.codegen_repr();
        if !matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
            emit_box_current_value_as_mixed(ctx.emitter, &ty);
        }
        let result_reg = abi::int_result_reg(ctx.emitter);
        abi::emit_store_to_sp(ctx.emitter, result_reg, args_offset + 8 + index * 8);
    }
    Ok(())
}

/// Stores all positional operands as a count-prefixed static-method argument pack.
pub(super) fn store_eval_static_method_call_arg_pack(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    args_offset: usize,
) -> Result<()> {
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, inst.operands.len() as i64);
    abi::emit_store_to_sp(ctx.emitter, result_reg, args_offset);
    for (index, operand) in inst.operands.iter().enumerate() {
        let ty = ctx.load_value_to_result(*operand)?.codegen_repr();
        if !matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
            emit_box_current_value_as_mixed(ctx.emitter, &ty);
        }
        let result_reg = abi::int_result_reg(ctx.emitter);
        abi::emit_store_to_sp(ctx.emitter, result_reg, args_offset + 8 + index * 8);
    }
    Ok(())
}

/// Stores an object operand as a boxed Mixed cell in eval scratch storage.
pub(super) fn store_eval_object_operand(ctx: &mut FunctionContext<'_>, object: ValueId) -> Result<()> {
    store_eval_mixed_operand_at(ctx, object, EVAL_TEMP_CELL_OFFSET)
}

/// Stores one operand as a boxed Mixed cell; native representations create an owner to retire.
pub(super) fn store_eval_mixed_operand_at(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    offset: usize,
) -> Result<()> {
    let value_ty = ctx.load_value_to_result(value)?.codegen_repr();
    if !matches!(value_ty, PhpType::Mixed | PhpType::Union(_)) {
        emit_box_current_value_as_mixed(ctx.emitter, &value_ty);
    }
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_store_to_sp(ctx.emitter, result_reg, offset);
    Ok(())
}

/// Retires a positional eval argument array while preserving the bridge status.
pub(super) fn retire_eval_call_argument_boxes(
    ctx: &mut FunctionContext<'_>, operands: &[ValueId], args_offset: usize,
) -> Result<()> {
    let boxes: Vec<_> = operands.iter().enumerate().map(|(index, &value)|
        (value, args_offset + index * 8)).collect();
    retire_eval_metadata_operand_boxes(ctx, &boxes)
}

/// Retires metadata-query adapter boxes while preserving the C status or scalar predicate result.
/// Existing Mixed operands remain borrowed; native payloads retain their original EIR owner.
pub(super) fn retire_eval_metadata_operand_boxes(
    ctx: &mut FunctionContext<'_>,
    operands: &[(ValueId, usize)],
) -> Result<()> {
    let result = abi::int_result_reg(ctx.emitter);
    for &(value, offset) in operands {
        if matches!(ctx.value_php_type(value)?.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
            continue;
        }
        ctx.emitter.comment("retire temporary eval metadata operand box");
        abi::emit_push_reg(ctx.emitter, result);
        abi::emit_load_temporary_stack_slot(ctx.emitter, result, offset + 16);
        abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
        abi::emit_pop_reg(ctx.emitter, result);
    }
    Ok(())
}

/// Probes whether eval has a late-static called-class override for an AOT frame.
pub(super) fn emit_eval_native_frame_override_probe(
    ctx: &mut FunctionContext<'_>,
    frame_class: &str,
    no_override_label: &str,
) {
    let (frame_label, frame_len) = ctx.data.add_string(frame_class.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 0),
        &frame_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        frame_len as i64,
    );
    let out_ptr_arg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_temporary_stack_address(ctx.emitter, out_ptr_arg, EVAL_CALLED_CLASS_PTR_OFFSET);
    let out_len_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    abi::emit_temporary_stack_address(ctx.emitter, out_len_arg, EVAL_CALLED_CLASS_LEN_OFFSET);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_native_frame_called_class_override");
    abi::emit_call_label(ctx.emitter, &symbol);
    abi::emit_branch_if_int_result_zero(ctx.emitter, no_override_label);
}

/// Consumes an owned eval result box when a concrete ABI replaces its Mixed representation.
/// Mixed and union callers instead receive the box itself.
pub(super) fn emit_eval_result_as_type(ctx: &mut FunctionContext<'_>, result_ty: &PhpType) -> Result<()> {
    let representation = result_ty.codegen_repr();
    match representation {
        PhpType::Narrow(_) | PhpType::Resource(_) | PhpType::Packed(_) => {
            super::super::super::callables::results::emit_detach_owned_mixed_heap_result(
                ctx.emitter, &representation,
            );
            Ok(())
        }
        _ => {
            let ty = if representation == PhpType::False { PhpType::Bool } else { representation };
            super::super::super::callables::results::emit_unbox_owned_descriptor_result(
                ctx.emitter, &ty, crate::ir::Ownership::Owned,
            )
        }
    }
}

/// Boxes a raw eval predicate result when the enclosing IR value expects Mixed storage.
pub(super) fn box_eval_bool_result_if_mixed(ctx: &mut FunctionContext<'_>, inst: &Instruction) {
    if inst.result.is_some() && inst.result_php_type.codegen_repr() == PhpType::Mixed {
        emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
    }
}

/// Returns the eval ABI discriminator for a class-name builtin.
pub(super) fn eval_class_lookup_kind(name: &str) -> Result<i64> {
    match name {
        "get_class" => Ok(EVAL_CLASS_LOOKUP_GET_CLASS),
        "get_parent_class" => Ok(EVAL_CLASS_LOOKUP_GET_PARENT_CLASS),
        _ => Err(CodegenIrError::unsupported(format!(
            "eval object class-name lookup {}",
            name
        ))),
    }
}

/// Returns the eval ABI discriminator for member-existence builtins.
pub(super) fn eval_member_lookup_kind(name: &str) -> Result<i64> {
    match name {
        "method_exists" => Ok(EVAL_MEMBER_LOOKUP_METHOD_EXISTS),
        "property_exists" => Ok(EVAL_MEMBER_LOOKUP_PROPERTY_EXISTS),
        _ => Err(CodegenIrError::unsupported(format!(
            "eval member-exists lookup {}",
            name
        ))),
    }
}

/// Returns the eval ABI discriminator for class/interface/trait relation builtins.
pub(super) fn eval_class_relation_kind(name: &str) -> Result<i64> {
    match name {
        "class_implements" => Ok(EVAL_CLASS_RELATION_IMPLEMENTS),
        "class_parents" => Ok(EVAL_CLASS_RELATION_PARENTS),
        "class_uses" => Ok(EVAL_CLASS_RELATION_USES),
        _ => Err(CodegenIrError::unsupported(format!(
            "eval class-relation lookup {}",
            name
        ))),
    }
}

/// Branches when `__rt_mixed_unbox` did not expose an object payload.
pub(super) fn emit_branch_if_eval_unboxed_not_object(ctx: &mut FunctionContext<'_>, label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #6");                              // runtime tag 6 means the Mixed value contains an object
            ctx.emitter.instruction(&format!("b.ne {}", label));                // non-object values use the native false/empty fallback
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 6");                              // runtime tag 6 means the Mixed value contains an object
            ctx.emitter.instruction(&format!("jne {}", label));                 // non-object values use the native false/empty fallback
        }
    }
}

/// Branches to the invalid-target fatal unless an eval dynamic target is string or object.
pub(super) fn emit_validate_eval_dynamic_instanceof_target(ctx: &mut FunctionContext<'_>, label: &str) {
    let ok_label = ctx.next_label("eval_object_is_a_dynamic_target_ok");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #1");                              // runtime tag 1 means the dynamic target is a string
            ctx.emitter.instruction(&format!("b.eq {}", ok_label));             // accept string targets for dynamic instanceof
            ctx.emitter.instruction("cmp x0, #6");                              // runtime tag 6 means the dynamic target is an object
            ctx.emitter.instruction(&format!("b.eq {}", ok_label));             // accept object targets for dynamic instanceof
            ctx.emitter.instruction(&format!("b {}", label));                   // reject every other dynamic instanceof target kind
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 1");                              // runtime tag 1 means the dynamic target is a string
            ctx.emitter.instruction(&format!("je {}", ok_label));               // accept string targets for dynamic instanceof
            ctx.emitter.instruction("cmp rax, 6");                              // runtime tag 6 means the dynamic target is an object
            ctx.emitter.instruction(&format!("je {}", ok_label));               // accept object targets for dynamic instanceof
            ctx.emitter.instruction(&format!("jmp {}", label));                 // reject every other dynamic instanceof target kind
        }
    }
    ctx.emitter.label(&ok_label);
}

/// Branches when an eval C-ABI call returned a negative `int` sentinel.
pub(super) fn emit_branch_if_eval_c_int_negative(ctx: &mut FunctionContext<'_>, label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            let branch = format!("tbnz w0, #31, {}", label);
            ctx.emitter.instruction(&branch);                                   // branch when the C int result is the invalid-target sentinel
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test eax, eax");                           // set flags from the C int result
            ctx.emitter.instruction(&format!("js {}", label));                  // branch when the C int result is the invalid-target sentinel
        }
    }
}

/// Reorders an unboxed eval string cell into the target string result registers.
pub(super) fn emit_eval_unboxed_string_result(ctx: &mut FunctionContext<'_>) {
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rax, rdi");                                // move the unboxed string pointer into the x86_64 string-result register
    }
}

/// Emits a borrowed string literal as the current native string result.
pub(super) fn emit_eval_string_result(ctx: &mut FunctionContext<'_>, bytes: &[u8]) {
    let (label, len) = ctx.data.add_string(bytes);
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
}

/// Saves the loaded eval source string while scope setup calls use argument registers.
pub(super) fn save_eval_code_string(ctx: &mut FunctionContext<'_>) {
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_store_to_sp(ctx.emitter, ptr_reg, EVAL_CODE_PTR_OFFSET);
    abi::emit_store_to_sp(ctx.emitter, len_reg, EVAL_CODE_LEN_OFFSET);
}
