//! Purpose:
//! Lowers named and dynamic instanceof entry points.
//!
//! Called from:
//! - The object lowering facade and sibling object support modules.
//!
//! Key details:
//! - Eval-aware and runtime metadata paths keep their existing precedence.

use super::*;

/// Lowers named `instanceof` using runtime class/interface metadata.
pub(in crate::codegen::lower_inst) fn lower_instanceof(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    let value_ty = ctx.value_php_type(value)?;
    let class_name = class_name_immediate(ctx, inst)?.to_string();
    if class_name
        .trim_start_matches('\\')
        .eq_ignore_ascii_case("Closure")
    {
        emit_closure_instanceof(ctx, value, &value_ty)?;
        return store_if_result(ctx, inst);
    }
    if !matches!(
        value_ty,
        PhpType::Object(_) | PhpType::Mixed | PhpType::Union(_)
    ) {
        emit_false(ctx);
        return store_if_result(ctx, inst);
    }
    if class_name
        .trim_start_matches('\\')
        .eq_ignore_ascii_case("__PHP_Incomplete_Class")
    {
        emit_incomplete_class_instanceof(ctx, value, &value_ty)?;
        return store_if_result(ctx, inst);
    }
    if builtins::has_eval_context(ctx) {
        return builtins::lower_eval_object_is_a(ctx, inst, value, &class_name, false);
    }
    let Some((target_id, target_kind)) = classify_named_target(ctx, &class_name) else {
        emit_false(ctx);
        return store_if_result(ctx, inst);
    };
    match value_ty {
        PhpType::Object(_) => {
            ctx.load_value_to_reg(value, abi::int_arg_reg_name(ctx.emitter.target, 0))?;
            emit_match_call(ctx, target_id, target_kind, "__rt_exception_matches");
        }
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.load_value_to_reg(value, abi::int_arg_reg_name(ctx.emitter.target, 0))?;
            emit_match_call(ctx, target_id, target_kind, "__rt_mixed_instanceof");
        }
        _ => emit_false(ctx),
    }
    store_if_result(ctx, inst)
}

/// Tests callable storage against PHP's built-in `Closure` class identity.
fn emit_closure_instanceof(
    ctx: &mut FunctionContext<'_>,
    value: crate::ir::ValueId,
    value_ty: &PhpType,
) -> Result<()> {
    match value_ty {
        PhpType::Callable => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
        }
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.load_value_to_reg(value, abi::int_result_reg(ctx.emitter))?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction("cmp x0, #10");                     // runtime tag 10 is a Closure callable descriptor
                    ctx.emitter.instruction("cset x0, eq");                     // return whether the boxed value carries that tag
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction("cmp rax, 10");                     // runtime tag 10 is a Closure callable descriptor
                    ctx.emitter.instruction("sete al");                         // materialize the callable-tag comparison
                    ctx.emitter.instruction("movzx rax, al");                   // widen the boolean result to the integer ABI
                }
            }
        }
        _ => emit_false(ctx),
    }
    Ok(())
}

/// Tests the synthetic `__PHP_Incomplete_Class` identity carried by class id `-2`.
///
/// `unserialize(..., ['allowed_classes' => false])` stores refused objects with a reserved
/// class id and the original class name; PHP still reports them as instances of the internal
/// `__PHP_Incomplete_Class`. No declared class-info entry exists for that name, so the normal
/// target classifier would answer false before looking at the object.
fn emit_incomplete_class_instanceof(
    ctx: &mut FunctionContext<'_>,
    value: crate::ir::ValueId,
    value_ty: &PhpType,
) -> Result<()> {
    match value_ty {
        PhpType::Object(_) => {
            ctx.load_value_to_reg(value, abi::int_result_reg(ctx.emitter))?;
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction("ldr x0, [x0]");                    // object header class id
                    ctx.emitter.instruction("cmn x0, #2");                      // class id -2 marks an incomplete object
                    ctx.emitter.instruction("cset x0, eq");                     // return whether this is the synthetic class
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction("mov rax, QWORD PTR [rax]");        // object header class id
                    ctx.emitter.instruction("cmp rax, -2");                     // class id -2 marks an incomplete object
                    ctx.emitter.instruction("sete al");                         // materialize the identity comparison
                    ctx.emitter.instruction("movzx rax, al");                   // widen the boolean result to the integer ABI
                }
            }
        }
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.load_value_to_reg(value, abi::int_result_reg(ctx.emitter))?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            let false_label = ctx.next_label("incomplete_instanceof_false");
            let done = ctx.next_label("incomplete_instanceof_done");
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction("cmp x0, #6");                      // runtime tag 6 = object payload
                    ctx.emitter.instruction(&format!("b.ne {}", false_label));  // non-objects are not instances
                    ctx.emitter.instruction("ldr x1, [x1]");                    // object header class id
                    ctx.emitter.instruction("cmn x1, #2");                      // class id -2 marks an incomplete object
                    ctx.emitter.instruction("cset x0, eq");                     // return the synthetic-class identity
                    ctx.emitter.instruction(&format!("b {}", done));            // skip the false arm
                    ctx.emitter.label(&false_label);
                    ctx.emitter.instruction("mov x0, #0");                      // scalar payloads are not instances
                    ctx.emitter.label(&done);
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction("cmp rax, 6");                      // runtime tag 6 = object payload
                    ctx.emitter.instruction(&format!("jne {}", false_label));   // non-objects are not instances
                    ctx.emitter.instruction("mov rdi, QWORD PTR [rdi]");        // object header class id
                    ctx.emitter.instruction("cmp rdi, -2");                     // class id -2 marks an incomplete object
                    ctx.emitter.instruction("sete al");                         // materialize the identity comparison
                    ctx.emitter.instruction("movzx rax, al");                   // widen the boolean result to the integer ABI
                    ctx.emitter.instruction(&format!("jmp {}", done));          // skip the false arm
                    ctx.emitter.label(&false_label);
                    ctx.emitter.instruction("xor eax, eax");                    // scalar payloads are not instances
                    ctx.emitter.label(&done);
                }
            }
        }
        _ => emit_false(ctx),
    }
    Ok(())
}

/// Lowers dynamic `instanceof` where the target is resolved from a runtime string or object.
pub(in crate::codegen::lower_inst) fn lower_instanceof_dynamic(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    let target = expect_operand(inst, 1)?;
    if builtins::has_eval_context(ctx) {
        return builtins::lower_eval_object_is_a_dynamic(ctx, inst, value, target, false);
    }
    let value_ty = ctx.value_php_type(value)?;
    let target_ty = ctx.value_php_type(target)?;
    let target_false = ctx.next_label("instanceof_dynamic_target_false");
    let done = ctx.next_label("instanceof_dynamic_done");
    emit_normalized_dynamic_instanceof_value(ctx, value, &value_ty)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    emit_dynamic_target_metadata(ctx, target, &target_ty, &target_false)?;
    emit_dynamic_match_call(ctx);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&target_false);
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    emit_false(ctx);
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}
