//! Purpose:
//! Runtime dispatch for cloning object payloads held in boxed Mixed values.
//!
//! Called from:
//! - `crate::codegen::lower_inst::objects::lower_object_clone_shallow()`.
//!
//! Key details:
//! - Restricts dispatch to fixed-layout clone-safe classes and preserves owned fields.

use super::*;

/// Clones a boxed runtime object through fixed-layout class dispatch.
pub(super) fn lower_runtime_mixed_object_clone(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    source: ValueId,
) -> Result<()> {
    let result = inst.result.ok_or_else(|| {
        CodegenIrError::invalid_module("runtime mixed object clone missing result value")
    })?;
    let clone_key = php_symbol_key("__clone");
    let mut candidates = ctx
        .module
        .class_infos
        .iter()
        .filter(|(class_name, class_info)| {
            !is_runtime_managed_object_clone_class(class_name)
                && !class_info.methods.contains_key(&clone_key)
        })
        .map(|(class_name, class_info)| {
            (class_info.class_id, class_name.clone())
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|(class_id, _)| *class_id);

    let done_label = ctx.next_label("mixed_object_clone_done");
    let error_label = ctx.next_label("mixed_object_clone_type_error");
    let case_labels = candidates
        .iter()
        .map(|(_, class_name)| {
            ctx.next_label(&format!("mixed_object_clone_{}", label_fragment(class_name)))
        })
        .collect::<Vec<_>>();

    ctx.load_value_to_reg(source, abi::int_result_reg(ctx.emitter))?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #6");                             // runtime tag 6 is an object payload
            ctx.emitter.instruction(&format!("b.ne {}", error_label));         // never dereference non-object Mixed payloads
            ctx.emitter.instruction("ldr x9, [x1]");                          // load the runtime object's class id
            for ((class_id, _), label) in candidates.iter().zip(case_labels.iter()) {
                abi::emit_load_int_immediate(ctx.emitter, "x10", *class_id as i64);
                ctx.emitter.instruction("cmp x9, x10");                       // select the fixed-layout clone implementation
                ctx.emitter.instruction(&format!("b.eq {}", label));          // clone the matching runtime class
            }
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 6");                            // runtime tag 6 is an object payload
            ctx.emitter.instruction(&format!("jne {}", error_label));         // never dereference non-object Mixed payloads
            ctx.emitter.instruction("mov r11, QWORD PTR [rdi]");              // load the runtime object's class id
            for ((class_id, _), label) in candidates.iter().zip(case_labels.iter()) {
                abi::emit_load_int_immediate(ctx.emitter, "r10", *class_id as i64);
                ctx.emitter.instruction("cmp r11, r10");                      // select the fixed-layout clone implementation
                ctx.emitter.instruction(&format!("je {}", label));            // clone the matching runtime class
            }
        }
    }
    abi::emit_jump(ctx.emitter, &error_label);

    for ((_, class_name), label) in candidates.iter().zip(case_labels.iter()) {
        ctx.emitter.label(label);
        let source_reg = match ctx.emitter.target.arch {
            Arch::AArch64 => "x1",
            Arch::X86_64 => "rdi",
        };
        emit_fixed_layout_object_clone_from_reg(ctx, result, source_reg, class_name)?;
        emit_box_current_owned_value_as_mixed(
            ctx.emitter,
            &PhpType::Object(class_name.clone()),
        );
        ctx.store_result_value(result)?;
        abi::emit_jump(ctx.emitter, &done_label);
    }

    ctx.emitter.label(&error_label);
    super::super::exceptions::emit_type_error(
        ctx,
        "clone(): Argument #1 ($object) must be of type object",
    );
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Copies a fixed-layout runtime object while retaining or duplicating owned fields.
fn emit_fixed_layout_object_clone_from_reg(
    ctx: &mut FunctionContext<'_>,
    result: ValueId,
    source_reg: &str,
    class_name: &str,
) -> Result<()> {
    if is_builtin_stdclass(class_name) {
        abi::emit_push_reg(ctx.emitter, source_reg);
        abi::emit_call_label(ctx.emitter, "__rt_stdclass_new");
        ctx.store_result_value(result)?;
        let source_reg = abi::secondary_scratch_reg(ctx.emitter);
        let dest_reg = abi::symbol_scratch_reg(ctx.emitter);
        abi::emit_pop_reg(ctx.emitter, source_reg);
        ctx.load_value_to_reg(result, dest_reg)?;
        emit_clone_dynamic_property_hash(ctx, source_reg, dest_reg, 8);
        ctx.load_value_to_result(result)?;
        return Ok(());
    }

    let (
        class_id,
        property_count,
        allow_dynamic_properties,
        string_offsets,
        retained_offsets,
        owned_reference_property_offsets,
    ) = {
        let class_info = ctx.module.class_infos.get(class_name).ok_or_else(|| {
            CodegenIrError::unsupported(format!("unknown class {}", class_name))
        })?;
        (
            class_info.class_id,
            class_info.properties.len(),
            class_info.allow_dynamic_properties,
            cloned_string_property_offsets(class_info),
            cloned_property_retain_offsets(class_info),
            owned_reference_property_offsets(class_info),
        )
    };
    abi::emit_push_reg(ctx.emitter, source_reg);
    emit_object_allocation(
        ctx,
        class_id,
        property_count,
        allow_dynamic_properties,
        &[],
        &owned_reference_property_offsets,
    )?;
    ctx.store_result_value(result)?;
    let source_reg = abi::secondary_scratch_reg(ctx.emitter);
    let dest_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_pop_reg(ctx.emitter, source_reg);
    ctx.load_value_to_reg(result, dest_reg)?;
    emit_clone_declared_property_slots(
        ctx,
        source_reg,
        dest_reg,
        property_count,
        &string_offsets,
        &retained_offsets,
    );
    if allow_dynamic_properties {
        emit_clone_dynamic_property_hash(
            ctx,
            source_reg,
            dest_reg,
            dynamic_property_hash_offset(property_count),
        );
    }
    ctx.load_value_to_result(result)?;
    Ok(())
}
