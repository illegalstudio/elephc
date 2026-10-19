//! Purpose:
//! Lowers `LoadDynamicPropRefCell` and `BindDynamicPropRefCell`: a PHP reference to a property
//! that lives in an object's per-instance property hash (`$s->{$k} = &$v` on `stdClass`, or a
//! dynamic property of a class with hash storage).
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - IR lowering only emits these ops for a receiver whose static class is known and whose
//!   property hash sits at one offset for every runtime class the receiver can hold
//!   (`ir_lower::expr::dynamic_reference_places`); declared properties never reach them.
//! - The receiver and the name are stacked first, so php's dynamic-creation notice
//!   (`Creation of dynamic property C::$p is deprecated`) can probe the hash before the helper
//!   inserts the name. `stdClass` and `#[\AllowDynamicProperties]` classes report nothing.
//! - `__rt_property_hash_reference` does the hash work for both targets.

use super::*;

/// Lowers `LoadDynamicPropRefCell` (`object, name` → cell) and `BindDynamicPropRefCell`
/// (`object, name, cell`).
pub(in crate::codegen::lower_inst) fn lower_dynamic_prop_ref_cell(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let object = expect_operand(inst, 0)?;
    let name = expect_operand(inst, 1)?;
    let cell = if inst.op == Op::BindDynamicPropRefCell {
        Some(expect_operand(inst, 2)?)
    } else {
        None
    };
    let PhpType::Object(class_name) = ctx.value_php_type(object)?.codegen_repr() else {
        return Err(CodegenIrError::unsupported(format!(
            "{} needs a receiver of a statically known class",
            inst.op.name()
        )));
    };
    let class_name = class_name.trim_start_matches('\\').to_string();
    let Some(hash_offset) = dynamic_property_hash_offset_for_class(ctx, &class_name, "")? else {
        return Err(CodegenIrError::unsupported(format!(
            "{} on {class_name}, which has no property hash",
            inst.op.name()
        )));
    };
    let target = ctx.emitter.target;
    // Stack: [0] name_ptr, [8] name_len, [16] receiver, [24] padding.
    let object_reg = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(object, object_reg)?;
    abi::emit_push_reg(ctx.emitter, object_reg);
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    ctx.load_string_value_to_regs(name, ptr_reg, len_reg)?;
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);
    if !is_builtin_stdclass(&class_name) {
        emit_dynamic_property_creation_deprecation(ctx, &class_name, hash_offset, 16, 0)?;
    }
    let slot_reg = abi::int_arg_reg_name(target, 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, slot_reg, 16);
    let add = match target.arch {
        Arch::AArch64 => format!("add {slot_reg}, {slot_reg}, #{hash_offset}"),
        Arch::X86_64 => format!("add {slot_reg}, {hash_offset}"),
    };
    ctx.emitter.instruction(&add);                                              // address the receiver's property-hash slot
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(target, 1), 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(target, 2), 8);
    let cell_reg = abi::int_arg_reg_name(target, 3);
    match cell {
        Some(cell) => {
            ctx.load_value_to_reg(cell, cell_reg)?;
        }
        None => abi::emit_load_int_immediate(ctx.emitter, cell_reg, 0),
    }
    abi::emit_call_label(ctx.emitter, "__rt_property_hash_reference");
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    if let Some(result) = inst.result {
        ctx.store_int_result_value(result)?;
    }
    Ok(())
}
