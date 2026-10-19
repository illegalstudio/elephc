//! Purpose:
//! Builds Reflection owners whose target is only known at run time: `new
//! ReflectionFunction($closure)` and the name-only `ReflectionClass` that
//! `ReflectionFunction::getClosureScopeClass()` returns.
//!
//! Called from:
//! - `owner_dispatch::lower_reflection_owner_new()`.
//!
//! Key details:
//! - A Closure reflector keeps the Closure in its private `__callable` slot and sets
//!   `__runtime_callable`; its synthetic methods read the descriptor through
//!   `CallableReflectionMeta` and throw `Error` for every query that has no runtime answer.
//! - A name-only ReflectionClass stores the scope class name in `__name` and sets `__name_only`;
//!   only `getName()` and the `name` property answer, every other method throws `Error`.
//! - The name-only form is selected by a second constructor operand that only the internal
//!   `__elephc_reflection_class_named()` lowering emits; user `new ReflectionClass()` calls are
//!   arity-checked to one argument.

use super::*;

/// Returns the Closure operand of `new ReflectionFunction($closure)`.
pub(super) fn reflection_function_closure_operand(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    inst: &Instruction,
) -> Result<Option<ValueId>> {
    if class_name != "ReflectionFunction" {
        return Ok(None);
    }
    let Some(operand) = inst.operands.first().copied() else {
        return Ok(None);
    };
    Ok((ctx.value_php_type(operand)?.codegen_repr() == PhpType::Callable).then_some(operand))
}

/// Returns the runtime class-name operand of an internal name-only ReflectionClass.
pub(super) fn reflection_class_named_operand(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    inst: &Instruction,
) -> Result<Option<ValueId>> {
    if class_name != "ReflectionClass" || inst.operands.len() != 2 {
        return Ok(None);
    }
    let operand = inst.operands[0];
    Ok((ctx.value_php_type(operand)?.codegen_repr() == PhpType::Str).then_some(operand))
}

/// Allocates a ReflectionFunction that reflects the Closure in `closure` at run time.
pub(super) fn emit_reflection_function_from_closure(
    ctx: &mut FunctionContext<'_>,
    closure: ValueId,
) -> Result<()> {
    const OWNER: &str = "ReflectionFunction";
    // Box an owned reference to the Closure first; the reflector keeps it alive.
    ctx.load_value_to_result(closure)?;
    crate::codegen::callable_descriptor::emit_retain_current_descriptor(ctx.emitter);
    emit_box_current_owned_value_as_mixed(ctx.emitter, &PhpType::Callable);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    emit_reflection_owner_object(ctx, OWNER, &empty_reflection_metadata())?;
    emit_reflection_owner_bool_property(ctx, OWNER, "__runtime_callable", true)?;
    let callable_offset = {
        let class_info = ctx
            .module
            .class_infos
            .get(OWNER)
            .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
        reflection_property_offset(class_info, "__callable")?
    };
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    // Release the slot's default value, then move the boxed Closure into it.
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_load_from_address(ctx.emitter, result_reg, result_reg, callable_offset);
    abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
    abi::emit_pop_reg(ctx.emitter, object_reg);
    abi::emit_pop_reg(ctx.emitter, result_reg);
    abi::emit_store_to_address(ctx.emitter, result_reg, object_reg, callable_offset);
    abi::emit_store_zero_to_address(ctx.emitter, object_reg, callable_offset + 8);
    abi::emit_reg_move(ctx.emitter, result_reg, object_reg);
    Ok(())
}

/// Allocates a ReflectionClass that only knows its class name, read from `name` at run time.
pub(super) fn emit_reflection_class_named(
    ctx: &mut FunctionContext<'_>,
    name: ValueId,
) -> Result<()> {
    const OWNER: &str = "ReflectionClass";
    let name_offset = {
        let class_info = ctx
            .module
            .class_infos
            .get(OWNER)
            .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
        reflection_property_offset(class_info, "__name")?
    };
    emit_reflection_owner_object(ctx, OWNER, &empty_reflection_metadata())?;
    emit_reflection_owner_bool_property(ctx, OWNER, "__name_only", true)?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    ctx.load_value_to_result(name)?;
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    abi::emit_pop_reg(ctx.emitter, object_reg);
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_store_to_address(ctx.emitter, ptr_reg, object_reg, name_offset);
    abi::emit_store_to_address(ctx.emitter, len_reg, object_reg, name_offset + 8);
    abi::emit_reg_move(ctx.emitter, result_reg, object_reg);
    Ok(())
}
