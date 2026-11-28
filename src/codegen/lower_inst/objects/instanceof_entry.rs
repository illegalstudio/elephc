//! Purpose:
//! Lowers named and dynamic instanceof entry points.
//!
//! Called from:
//! - The object lowering facade and sibling object support modules.
//!
//! Key details:
//! - Eval-aware and runtime metadata paths keep their existing precedence.

use super::*;

/// Tests a boxed `is_a` subject through the same class metadata as named `instanceof`.
pub(in crate::codegen::lower_inst) fn lower_mixed_is_a_literal_target(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    value: ValueId,
    class_name: &str,
) -> Result<()> {
    let normalized = class_name.trim_start_matches('\\');
    let canonical = ctx.module.class_infos.keys().chain(ctx.module.interface_infos.keys())
        .find(|name| name.eq_ignore_ascii_case(normalized))
        .map(String::as_str).unwrap_or(normalized);
    if let Some((target_id, target_kind)) = classify_named_target(ctx, canonical) {
        ctx.load_value_to_reg(value, abi::int_arg_reg_name(ctx.emitter.target, 0))?;
        emit_match_call(ctx, target_id, target_kind, "__rt_mixed_instanceof");
    } else {
        emit_false(ctx);
    }
    store_if_result(ctx, inst)
}

/// Whether a class name names PHP's built-in `Closure` (any case, with or without a leading `\`).
pub(in crate::codegen::lower_inst) fn is_closure_class_name(name: &str) -> bool {
    name.trim_start_matches('\\').eq_ignore_ascii_case("Closure")
}

/// Lowers named `instanceof` using runtime class/interface metadata.
pub(in crate::codegen::lower_inst) fn lower_instanceof(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    let value_ty = ctx.value_php_type(value)?;
    let class_name = class_name_immediate(ctx, inst)?.to_string();
    if is_closure_class_name(&class_name) {
        return lower_closure_class_check(ctx, inst, value);
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
    if ctx.module.required_runtime_features.eval_bridge {
        // An escaped eval-declared object carries its owner independently of this AOT frame.
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

/// Lowers a `Closure` class check (`instanceof Closure`, `is_a($v, 'Closure')`) for one value.
///
/// A native closure is a callable descriptor (a `callable` value, or Mixed runtime tag 10) with an
/// object handle; an implicit string or array callable has none. A closure created inside
/// `eval()` is a boxed eval object instead (tag 6), whose `Closure` identity only the eval bridge
/// knows, so with eval in the program a tag-6 subject asks the bridge.
pub(in crate::codegen::lower_inst) fn lower_closure_class_check(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    value: ValueId,
) -> Result<()> {
    let value_ty = ctx.value_php_type(value)?;
    let boxed = matches!(value_ty, PhpType::Mixed | PhpType::Union(_))
        && matches!(value_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_));
    if !(boxed && ctx.module.required_runtime_features.eval_bridge) {
        super::super::predicates::emit_closure_identity(ctx, value, &value_ty, false)?;
        return store_if_result(ctx, inst);
    }
    let eval_object = ctx.next_label("closure_check_eval_object");
    let done = ctx.next_label("closure_check_done");
    ctx.load_value_to_reg(value, abi::int_result_reg(ctx.emitter))?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #6");                              // runtime tag 6 is an object, possibly an eval closure
            ctx.emitter.instruction(&format!("b.eq {eval_object}"));            // ask the eval bridge about an object subject
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 6");                              // runtime tag 6 is an object, possibly an eval closure
            ctx.emitter.instruction(&format!("je {eval_object}"));              // ask the eval bridge about an object subject
        }
    }
    super::super::predicates::emit_closure_identity(ctx, value, &value_ty, false)?;
    store_if_result(ctx, inst)?;
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&eval_object);
    builtins::lower_eval_object_is_a(ctx, inst, value, "Closure", false)?;
    ctx.emitter.label(&done);
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
                    elephc_memoir::values::object_storage::emit_load_class_id(ctx.emitter, "x0", "x0");
                    ctx.emitter.instruction("cmn x0, #2");                      // class id -2 marks an incomplete object
                    ctx.emitter.instruction("cset x0, eq");                     // return whether this is the synthetic class
                }
                Arch::X86_64 => {
                    elephc_memoir::values::object_storage::emit_load_class_id(ctx.emitter, "rax", "rax");
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
                    elephc_memoir::values::object_storage::emit_load_class_id(ctx.emitter, "x1", "x1");
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
                    elephc_memoir::values::object_storage::emit_load_class_id(ctx.emitter, "rdi", "rdi");
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
    if ctx.module.required_runtime_features.eval_bridge {
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
