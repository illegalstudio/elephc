//! Purpose:
//! Lowers small indexed-array and associative-array builtins for the EIR backend.
//! Delegates aggregate iteration, set operations, and key checks to existing runtime helpers.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::builtins::lower_builtin_call()`.
//!
//! Key details:
//! - Aggregate helpers accept indexed arrays with 8-byte payload slots, and
//!   dispatch to refcount-aware runtime variants when payloads own heap values.
//! - Associative key filters require hash operands because their runtime helpers copy hash entries.

use crate::codegen::{
    abi, callable_descriptor, callable_dispatch, emit_box_current_owned_value_as_mixed,
    emit_box_current_value_as_mixed,
};
use crate::codegen::context::DeferredCallbackWrapper;
use crate::codegen::platform::Arch;
use crate::codegen_ir::{CodegenIrError, Result};
use crate::ir::{BlockId, Immediate, Instruction, LocalSlotId, Op, ValueDef, ValueId};
use crate::names::{function_symbol, method_symbol, php_symbol_key, static_method_symbol};
use crate::types::{array_key_type_from_value_type, PhpType};

use super::super::super::context::FunctionContext;
use super::super::{
    expect_operand, legacy_context_from_eir_module, resolve_int_operand_to_result, store_if_result,
};

mod column;
mod key_exists;
mod keys;
mod search;
mod shift;
mod unshift;
pub(in crate::codegen_ir::lower_inst::builtins) mod values;

/// Rejects `call_user_func*` calls that escaped the dedicated EIR callback lowering path.
pub(super) fn lower_call_user_func_builtin_escape(
    _ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    Err(CodegenIrError::unsupported(format!(
        "{} builtin dispatcher escape with {} lowered operands",
        name,
        inst.operands.len()
    )))
}

/// Lowers `array_sum()` over supported indexed-array payloads.
pub(super) fn lower_array_sum(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_indexed_array_aggregate(ctx, inst, "array_sum", "__rt_array_sum")
}

/// Lowers `array_product()` over supported indexed-array payloads.
pub(super) fn lower_array_product(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_indexed_array_aggregate(ctx, inst, "array_product", "__rt_array_product")
}

/// Lowers `array_push()` by appending one value and publishing the mutated array.
pub(super) fn lower_array_push(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "array_push", 2)?;
    let array = expect_operand(inst, 0)?;
    if matches!(
        ctx.value_php_type(array)?.codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        super::super::arrays::lower_mixed_array_append(ctx, inst)?;
    } else {
        super::super::arrays::lower_array_push(ctx, inst)?;
    }
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        0x7fff_ffff_ffff_fffe,
    );
    store_if_result(ctx, inst)
}

/// Lowers `array_chunk()` by splitting an indexed array into nested indexed arrays.
pub(super) fn lower_array_chunk(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "array_chunk", 2)?;
    let array = expect_operand(inst, 0)?;
    let length = expect_operand(inst, 1)?;
    let source_elem_ty = array_chunk_source_element_type(ctx.value_php_type(array)?)?;
    let result_elem_ty = result_array_element_type("array_chunk", &inst.result_php_type.codegen_repr())?;
    let result_inner_elem_ty = array_chunk_result_inner_element_type(&result_elem_ty)?;
    require_array_chunk_result_type(&source_elem_ty, &result_inner_elem_ty)?;
    lower_array_chunk_call(ctx, array, length, &source_elem_ty)?;
    crate::codegen::emit_array_value_type_stamp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        &result_elem_ty,
    );
    store_if_result(ctx, inst)
}

/// Lowers `array_pad()` by copying an indexed array and filling missing slots.
pub(super) fn lower_array_pad(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "array_pad", 3)?;
    let array = expect_operand(inst, 0)?;
    let target_size = expect_operand(inst, 1)?;
    let pad_value = expect_operand(inst, 2)?;
    let source_elem_ty = array_pad_source_element_type(ctx.value_php_type(array)?)?;
    let pad_value_ty = ctx.value_php_type(pad_value)?.codegen_repr();
    let result_elem_ty = result_array_element_type("array_pad", &inst.result_php_type.codegen_repr())?;
    require_array_pad_value_type(&source_elem_ty, &pad_value_ty)?;
    require_array_pad_result_type(&source_elem_ty, &result_elem_ty)?;
    lower_array_pad_call(ctx, array, target_size, pad_value, &source_elem_ty)?;
    normalize_indexed_array_result(ctx, "array_pad", &source_elem_ty, &result_elem_ty)?;
    store_if_result(ctx, inst)
}

/// Lowers `array_fill()` for pointer-sized scalar and refcounted payloads.
pub(super) fn lower_array_fill(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "array_fill", 3)?;
    let start = expect_operand(inst, 0)?;
    let count = expect_operand(inst, 1)?;
    let value = expect_operand(inst, 2)?;
    let value_ty = ctx.value_php_type(value)?.codegen_repr();
    let result_ty = inst.result_php_type.codegen_repr();
    if array_fill_result_is_assoc(&result_ty) {
        require_array_fill_assoc_value_type(&value_ty)?;
        require_array_fill_assoc_result_type(&result_ty)?;
        lower_array_fill_assoc_call(ctx, start, count, value, &value_ty)?;
        store_if_result(ctx, inst)?;
        return Ok(());
    }
    require_array_fill_indexed_value_type(&value_ty)?;
    let result_elem_ty = result_array_element_type("array_fill", &result_ty)?;
    require_array_fill_result_type(&value_ty, &result_elem_ty)?;
    lower_array_fill_call(ctx, start, count, value, &value_ty)?;
    normalize_indexed_array_result(ctx, "array_fill", &value_ty, &result_elem_ty)?;
    store_if_result(ctx, inst)
}

/// Lowers `array_fill_keys()` through the legacy hash-building runtime helpers.
pub(super) fn lower_array_fill_keys(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "array_fill_keys", 2)?;
    let keys = expect_operand(inst, 0)?;
    let value = expect_operand(inst, 1)?;
    let key_elem_ty = array_fill_keys_key_element_type(ctx.value_php_type(keys)?)?;
    let value_ty = ctx.value_php_type(value)?.codegen_repr();
    require_array_fill_keys_key_layout(&key_elem_ty)?;
    require_array_fill_keys_value_type(&value_ty)?;
    require_array_fill_keys_result_type(&key_elem_ty, &value_ty, &inst.result_php_type.codegen_repr())?;
    lower_array_fill_keys_call(ctx, keys, value, &value_ty)?;
    store_if_result(ctx, inst)
}

/// Lowers `array_combine()` through the legacy hash-building runtime helpers.
pub(super) fn lower_array_combine(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "array_combine", 2)?;
    let keys = expect_operand(inst, 0)?;
    let values = expect_operand(inst, 1)?;
    let key_elem_ty = array_combine_key_element_type(ctx.value_php_type(keys)?)?;
    let value_elem_ty = array_combine_value_element_type(ctx.value_php_type(values)?)?;
    require_array_combine_key_layout(&key_elem_ty)?;
    require_array_combine_value_layout(&value_elem_ty)?;
    require_array_combine_result_type(&value_elem_ty, &inst.result_php_type.codegen_repr())?;
    lower_array_combine_call(ctx, keys, values, &value_elem_ty)?;
    store_if_result(ctx, inst)
}

/// Lowers `array_column()` through the target-aware legacy column helpers.
pub(super) fn lower_array_column(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    column::lower_array_column(ctx, inst)
}

/// Lowers `array_flip()` through the legacy hash-building runtime helpers.
pub(super) fn lower_array_flip(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "array_flip", 1)?;
    let array = expect_operand(inst, 0)?;
    let value_elem_ty = array_flip_source_element_type(ctx.value_php_type(array)?)?;
    require_array_flip_result_type(&value_elem_ty, &inst.result_php_type.codegen_repr())?;
    ctx.load_value_to_result(array)?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the source indexed-array pointer as the flip helper argument
    }
    abi::emit_call_label(ctx.emitter, array_flip_runtime_helper(&value_elem_ty));
    store_if_result(ctx, inst)
}

/// Lowers `array_reverse()` for indexed arrays with 8-byte payload slots.
pub(super) fn lower_array_reverse(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "array_reverse", 1)?;
    let array = expect_operand(inst, 0)?;
    let elem_ty = eight_byte_indexed_array_element_type(ctx.value_php_type(array)?, "array_reverse")?;
    ctx.load_value_to_result(array)?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the source indexed-array pointer as the reverse helper argument
    }
    abi::emit_call_label(ctx.emitter, array_reverse_runtime_helper(&elem_ty));
    store_if_result(ctx, inst)
}

/// Lowers `array_unique()` for indexed arrays with 8-byte payload slots.
pub(super) fn lower_array_unique(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "array_unique", 1)?;
    let array = expect_operand(inst, 0)?;
    let elem_ty = eight_byte_indexed_array_element_type(ctx.value_php_type(array)?, "array_unique")?;
    ctx.load_value_to_result(array)?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the source indexed-array pointer as the dedup helper argument
    }
    abi::emit_call_label(ctx.emitter, array_unique_runtime_helper(&elem_ty));
    store_if_result(ctx, inst)
}

/// Lowers `array_filter()` for static and first-class callbacks through the runtime helper.
pub(super) fn lower_array_filter(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "array_filter", 2, 3)?;
    let array = expect_operand(inst, 0)?;
    let callback = expect_operand(inst, 1)?;
    let mode = inst.operands.get(2).copied();
    let elem_ty = array_filter_source_element_type(ctx.value_php_type(array)?)?;
    require_array_filter_result_type(&elem_ty, &inst.result_php_type.codegen_repr())?;
    let runtime_label = if array_filter_uses_refcounted_runtime(&elem_ty) {
        "__rt_array_filter_refcounted"
    } else {
        "__rt_array_filter"
    };
    let callback_arg_types = array_filter_callback_arg_types(ctx, mode, &elem_ty)?;
    if let Some(visible_arg_types) = callback_arg_types.clone() {
        match ctx.value_php_type(callback)?.codegen_repr() {
            PhpType::Callable => {
                lower_descriptor_callback_runtime(
                    ctx,
                    callback,
                    visible_arg_types,
                    PhpType::Bool,
                    |ctx, wrapper_label, env_bytes| {
                        match ctx.emitter.target.arch {
                            Arch::AArch64 => {
                                abi::emit_symbol_address(ctx.emitter, "x0", wrapper_label);
                                ctx.load_value_to_reg(array, "x1")?;
                                load_static_callback_env_arg(ctx, "x2", env_bytes);
                                load_array_filter_mode(ctx, mode, "x3")?;
                            }
                            Arch::X86_64 => {
                                abi::emit_symbol_address(ctx.emitter, "rdi", wrapper_label);
                                ctx.load_value_to_reg(array, "rsi")?;
                                load_static_callback_env_arg(ctx, "rdx", env_bytes);
                                load_array_filter_mode(ctx, mode, "rcx")?;
                            }
                        }
                        abi::emit_call_label(ctx.emitter, runtime_label);
                        Ok(())
                    },
                )?;
                store_if_result(ctx, inst)?;
                return Ok(());
            }
            PhpType::Str => {
                lower_runtime_string_descriptor_callback(
                    ctx,
                    callback,
                    Some(&PhpType::Array(Box::new(elem_ty.clone()))),
                    visible_arg_types,
                    PhpType::Bool,
                    "array_filter",
                    |ctx, wrapper_label, env_bytes| {
                        match ctx.emitter.target.arch {
                            Arch::AArch64 => {
                                abi::emit_symbol_address(ctx.emitter, "x0", wrapper_label);
                                ctx.load_value_to_reg(array, "x1")?;
                                load_static_callback_env_arg(ctx, "x2", env_bytes);
                                load_array_filter_mode(ctx, mode, "x3")?;
                            }
                            Arch::X86_64 => {
                                abi::emit_symbol_address(ctx.emitter, "rdi", wrapper_label);
                                ctx.load_value_to_reg(array, "rsi")?;
                                load_static_callback_env_arg(ctx, "rdx", env_bytes);
                                load_array_filter_mode(ctx, mode, "rcx")?;
                            }
                        }
                        abi::emit_call_label(ctx.emitter, runtime_label);
                        Ok(())
                    },
                )?;
                store_if_result(ctx, inst)?;
                return Ok(());
            }
            _ => {}
        }
    }
    let callback_binding =
        static_sort_callback_binding(ctx, callback, "array_filter callback", callback_arg_types.as_deref())?;
    let env_bytes = reserve_static_callback_env(ctx, callback_binding.env_source)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x0", &callback_binding.label);
            ctx.load_value_to_reg(array, "x1")?;
            load_static_callback_env_arg(ctx, "x2", env_bytes);
            load_array_filter_mode(ctx, mode, "x3")?;
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rdi", &callback_binding.label);
            ctx.load_value_to_reg(array, "rsi")?;
            load_static_callback_env_arg(ctx, "rdx", env_bytes);
            load_array_filter_mode(ctx, mode, "rcx")?;
        }
    }
    abi::emit_call_label(ctx.emitter, runtime_label);
    if env_bytes != 0 {
        abi::emit_release_temporary_stack(ctx.emitter, env_bytes);
    }
    store_if_result(ctx, inst)
}

/// Lowers `array_map()` through the callback runtime helper matching the callback result type.
pub(super) fn lower_array_map(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "array_map", 2)?;
    let callback = expect_operand(inst, 0)?;
    let array = expect_operand(inst, 1)?;
    let elem_ty = array_map_callback_array_element_type(ctx.value_php_type(array)?)?;
    match ctx.value_php_type(callback)?.codegen_repr() {
        PhpType::Callable => {
            let callback_elem_ty = array_map_descriptor_callback_result_element_type(inst)?;
            let result_elem_ty = array_map_result_element_type(inst, &callback_elem_ty)?;
            return lower_array_map_descriptor_callback(
                ctx,
                inst,
                callback,
                array,
                &elem_ty,
                &callback_elem_ty,
                &result_elem_ty,
            );
        }
        PhpType::Str => {
            let callback_elem_ty = PhpType::Mixed;
            let result_elem_ty = array_map_result_element_type(inst, &callback_elem_ty)?;
            lower_runtime_string_descriptor_callback(
                ctx,
                callback,
                Some(&PhpType::Array(Box::new(elem_ty.clone()))),
                vec![elem_ty.clone()],
                PhpType::Mixed,
                "array_map",
                |ctx, wrapper_label, env_bytes| {
                    let callback_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
                    let array_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 1);
                    let env_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 2);
                    abi::emit_symbol_address(ctx.emitter, callback_arg_reg, wrapper_label);
                    ctx.load_value_to_reg(array, array_arg_reg)?;
                    load_static_callback_env_arg(ctx, env_arg_reg, env_bytes);
                    abi::emit_call_label(ctx.emitter, array_map_runtime_label(&callback_elem_ty, env_bytes));
                    Ok(())
                },
            )?;
            normalize_indexed_array_result(ctx, "array_map", &callback_elem_ty, &result_elem_ty)?;
            box_array_result_for_mixed_builtin(ctx, inst, &result_elem_ty);
            store_if_result(ctx, inst)?;
            return Ok(());
        }
        PhpType::Array(elem) if matches!(elem.codegen_repr(), PhpType::Mixed | PhpType::Str) => {
            let callback_elem_ty = array_map_descriptor_callback_result_element_type(inst)?;
            let result_elem_ty = array_map_result_element_type(inst, &callback_elem_ty)?;
            return lower_array_map_callable_array_descriptor_callback(
                ctx,
                inst,
                callback,
                array,
                &elem_ty,
                &callback_elem_ty,
                &result_elem_ty,
            );
        }
        _ => {}
    }
    if descriptor_callback_local_without_same_block_store(ctx, callback)? {
        let callback_elem_ty = array_map_descriptor_callback_result_element_type(inst)?;
        let result_elem_ty = array_map_result_element_type(inst, &callback_elem_ty)?;
        return lower_array_map_descriptor_callback(
            ctx,
            inst,
            callback,
            array,
            &elem_ty,
            &callback_elem_ty,
            &result_elem_ty,
        );
    }
    let callback_binding =
        static_sort_callback_binding(ctx, callback, "array_map callback", Some(&[elem_ty]))?;
    let callback_elem_ty = array_map_callback_result_element_type(&callback_binding.return_ty)?;
    let result_elem_ty = array_map_result_element_type(inst, &callback_elem_ty)?;
    let env_bytes = reserve_static_callback_env(ctx, callback_binding.env_source)?;
    let callback_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let array_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    let env_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_symbol_address(ctx.emitter, callback_arg_reg, &callback_binding.label);
    ctx.load_value_to_reg(array, array_arg_reg)?;
    load_static_callback_env_arg(ctx, env_arg_reg, env_bytes);
    let runtime_label = if callback_elem_ty == PhpType::Str {
        if env_bytes == 0 {
            "__rt_array_map_str"
        } else {
            "__rt_array_map_str_owned"
        }
    } else {
        "__rt_array_map"
    };
    abi::emit_call_label(ctx.emitter, runtime_label);
    if env_bytes != 0 {
        abi::emit_release_temporary_stack(ctx.emitter, env_bytes);
    }
    normalize_indexed_array_result(ctx, "array_map", &callback_elem_ty, &result_elem_ty)?;
    box_array_result_for_mixed_builtin(ctx, inst, &result_elem_ty);
    store_if_result(ctx, inst)
}

/// Lowers `array_map()` through a descriptor-backed callback wrapper.
fn lower_array_map_descriptor_callback(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    callback: ValueId,
    array: ValueId,
    elem_ty: &PhpType,
    callback_elem_ty: &PhpType,
    result_elem_ty: &PhpType,
) -> Result<()> {
    let wrapper_label = emit_descriptor_callback_wrapper(
        ctx,
        vec![elem_ty.clone()],
        callback_elem_ty.clone(),
    );
    let env_bytes = reserve_descriptor_callback_env(ctx, callback)?;
    let callback_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let array_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    let env_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_symbol_address(ctx.emitter, callback_arg_reg, &wrapper_label);
    ctx.load_value_to_reg(array, array_arg_reg)?;
    load_static_callback_env_arg(ctx, env_arg_reg, env_bytes);
    abi::emit_call_label(ctx.emitter, array_map_runtime_label(callback_elem_ty, env_bytes));
    abi::emit_release_temporary_stack(ctx.emitter, env_bytes);
    normalize_indexed_array_result(ctx, "array_map", callback_elem_ty, result_elem_ty)?;
    box_array_result_for_mixed_builtin(ctx, inst, result_elem_ty);
    store_if_result(ctx, inst)
}

/// Lowers `array_map()` for runtime callable-array callbacks through descriptor envs.
fn lower_array_map_callable_array_descriptor_callback(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    callback: ValueId,
    array: ValueId,
    elem_ty: &PhpType,
    callback_elem_ty: &PhpType,
    result_elem_ty: &PhpType,
) -> Result<()> {
    let wrapper_label = emit_descriptor_callback_wrapper(
        ctx,
        vec![elem_ty.clone()],
        callback_elem_ty.clone(),
    );
    super::super::callables::emit_runtime_callable_array_descriptor_value(
        ctx,
        callback,
        "array_map callable array",
    )?;
    let descriptor_reg = abi::int_result_reg(ctx.emitter);
    let env_bytes = reserve_descriptor_callback_env_from_reg(ctx, descriptor_reg);
    let callback_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let array_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    let env_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_symbol_address(ctx.emitter, callback_arg_reg, &wrapper_label);
    ctx.load_value_to_reg(array, array_arg_reg)?;
    load_static_callback_env_arg(ctx, env_arg_reg, env_bytes);
    abi::emit_call_label(ctx.emitter, array_map_runtime_label(callback_elem_ty, env_bytes));
    release_descriptor_callback_env_preserving_result(ctx);
    abi::emit_release_temporary_stack(ctx.emitter, env_bytes);
    normalize_indexed_array_result(ctx, "array_map", callback_elem_ty, result_elem_ty)?;
    box_array_result_for_mixed_builtin(ctx, inst, result_elem_ty);
    store_if_result(ctx, inst)
}

/// Releases a one-slot descriptor callback env while preserving the runtime result.
fn release_descriptor_callback_env_preserving_result(ctx: &mut FunctionContext<'_>) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, 16);
    callable_descriptor::emit_release_current_descriptor(ctx.emitter);
    abi::emit_pop_reg(ctx.emitter, result_reg);
}

/// Emits a descriptor callback wrapper next to the current EIR function body.
fn emit_descriptor_callback_wrapper(
    ctx: &mut FunctionContext<'_>,
    visible_arg_types: Vec<PhpType>,
    return_ty: PhpType,
) -> String {
    let wrapper_label = ctx.next_label("array_map_descriptor_callback_wrapper");
    let done_label = ctx.next_label("array_map_descriptor_callback_after_wrapper");
    let wrapper = DeferredCallbackWrapper {
        label: wrapper_label.clone(),
        visible_arg_types,
        target_visible_arg_types: None,
        capture_types: Vec::new(),
        descriptor_prefix_types: Vec::new(),
        descriptor_return_type: Some(return_ty),
    };
    abi::emit_jump(ctx.emitter, &done_label);
    crate::codegen::emit_callback_wrapper(ctx.emitter, &wrapper);
    ctx.emitter.label(&done_label);
    wrapper_label
}

/// Reserves a one-slot callback environment containing the runtime callable descriptor.
fn reserve_descriptor_callback_env(
    ctx: &mut FunctionContext<'_>,
    callback: ValueId,
) -> Result<usize> {
    abi::emit_reserve_temporary_stack(ctx.emitter, 16);
    let callback_ty = ctx.load_value_to_result(callback)?;
    if callback_ty != PhpType::Callable {
        return Err(CodegenIrError::invalid_module(format!(
            "descriptor callback operand has PHP type {:?}",
            callback_ty
        )));
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("str x0, [sp]");                            // store the runtime callable descriptor for the descriptor callback wrapper
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                // store the runtime callable descriptor for the descriptor callback wrapper
        }
    }
    Ok(16)
}

/// Calls a descriptor-backed array callback runtime using a callable descriptor value.
fn lower_descriptor_callback_runtime<F>(
    ctx: &mut FunctionContext<'_>,
    callback: ValueId,
    visible_arg_types: Vec<PhpType>,
    return_ty: PhpType,
    mut emit_call: F,
) -> Result<()>
where
    F: FnMut(&mut FunctionContext<'_>, &str, usize) -> Result<()>,
{
    let wrapper_label = emit_descriptor_callback_wrapper(ctx, visible_arg_types, return_ty);
    let env_bytes = reserve_descriptor_callback_env(ctx, callback)?;
    emit_call(ctx, &wrapper_label, env_bytes)?;
    abi::emit_release_temporary_stack(ctx.emitter, env_bytes);
    Ok(())
}

/// Dispatches a runtime string callback name to a descriptor-backed array callback runtime.
fn lower_runtime_string_descriptor_callback<F>(
    ctx: &mut FunctionContext<'_>,
    callback: ValueId,
    source_arg_ty: Option<&PhpType>,
    visible_arg_types: Vec<PhpType>,
    return_ty: PhpType,
    owner: &str,
    mut emit_call: F,
) -> Result<()>
where
    F: FnMut(&mut FunctionContext<'_>, &str, usize) -> Result<()>,
{
    let callback_ty = ctx.load_value_to_result(callback)?;
    if callback_ty.codegen_repr() != PhpType::Str {
        return Err(CodegenIrError::invalid_module(format!(
            "{} runtime string callback has PHP type {:?}",
            owner, callback_ty
        )));
    }

    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);

    let call_reg = abi::nested_call_reg(ctx.emitter);
    let mut legacy_ctx = legacy_context_from_eir_module(ctx.module);
    legacy_ctx.functions.retain(|name, _| {
        ctx.module
            .functions
            .iter()
            .any(|function| !function.flags.is_main && function.name == *name)
    });
    let cases = callable_dispatch::runtime_callable_cases(&mut legacy_ctx, ctx.data, &[], source_arg_ty);
    emit_legacy_deferred_runtime_callable_support(ctx, &mut legacy_ctx);

    let done_label = ctx.next_label(&format!("{}_runtime_string_callback_done", owner));
    let selector = callable_dispatch::RuntimeCallableSelector::StringNameStack {
        ptr_offset: 0,
        len_offset: 8,
        call_reg,
    };
    for case in &cases {
        let next_case = ctx.next_label(&format!("{}_runtime_string_callback_next", owner));
        callable_dispatch::emit_branch_if_callable_case_mismatch(
            &selector,
            case,
            &next_case,
            ctx.emitter,
            &mut legacy_ctx,
            ctx.data,
        );
        let wrapper_label = emit_descriptor_callback_wrapper(
            ctx,
            visible_arg_types.clone(),
            return_ty.clone(),
        );
        let env_bytes = reserve_descriptor_callback_env_from_reg(ctx, call_reg);
        emit_call(ctx, &wrapper_label, env_bytes)?;
        abi::emit_release_temporary_stack(ctx.emitter, env_bytes);
        abi::emit_jump(ctx.emitter, &done_label);
        ctx.emitter.label(&next_case);
    }

    emit_dynamic_string_callback_abort(ctx, owner);
    ctx.emitter.label(&done_label);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    Ok(())
}

/// Emits legacy deferred callable wrappers/invokers inline and branches around them.
fn emit_legacy_deferred_runtime_callable_support(
    ctx: &mut FunctionContext<'_>,
    legacy_ctx: &mut crate::codegen::context::Context,
) {
    if legacy_ctx.deferred_closures.is_empty()
        && legacy_ctx.deferred_fiber_wrappers.is_empty()
        && legacy_ctx.deferred_callback_wrappers.is_empty()
        && legacy_ctx.deferred_extern_callback_trampolines.is_empty()
        && legacy_ctx.deferred_runtime_callable_invokers.is_empty()
    {
        return;
    }
    let done_label = ctx.next_label("runtime_callable_support_done");
    abi::emit_jump(ctx.emitter, &done_label);
    crate::codegen::emit_deferred_closures(ctx.emitter, ctx.data, legacy_ctx);
    ctx.emitter.label(&done_label);
}

/// Reserves a descriptor callback environment using a descriptor already held in a register.
fn reserve_descriptor_callback_env_from_reg(
    ctx: &mut FunctionContext<'_>,
    descriptor_reg: &str,
) -> usize {
    abi::emit_reserve_temporary_stack(ctx.emitter, 16);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("str {}, [sp]", descriptor_reg));  // store the selected runtime string descriptor for the descriptor callback wrapper
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov QWORD PTR [rsp], {}", descriptor_reg)); // store the selected runtime string descriptor for the descriptor callback wrapper
        }
    }
    16
}

/// Emits a fatal diagnostic for runtime callback names that do not resolve to descriptors.
fn emit_dynamic_string_callback_abort(ctx: &mut FunctionContext<'_>, owner: &str) {
    let message = format!(
        "Fatal error: {} callback string does not name a supported callable\n",
        owner
    );
    let (message_label, message_len) = ctx.data.add_string(message.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #2");                              // write the unresolved runtime callback diagnostic to stderr
            ctx.emitter.adrp("x1", &message_label);                             // load the runtime callback diagnostic page
            ctx.emitter.add_lo12("x1", "x1", &message_label);                  // resolve the runtime callback diagnostic address
            ctx.emitter.instruction(&format!("mov x2, #{}", message_len));      // pass the runtime callback diagnostic byte length to write
            ctx.emitter.syscall(4);
            abi::emit_exit(ctx.emitter, 1);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov edi, 2");                              // write the unresolved runtime callback diagnostic to Linux stderr
            abi::emit_symbol_address(ctx.emitter, "rsi", &message_label);
            ctx.emitter.instruction(&format!("mov edx, {}", message_len));      // pass the runtime callback diagnostic byte length to write
            ctx.emitter.instruction("mov eax, 1");                              // Linux x86_64 syscall 1 = write
            ctx.emitter.instruction("syscall");                                 // emit the fatal diagnostic before terminating
            abi::emit_exit(ctx.emitter, 1);
        }
    }
}

/// Returns the runtime helper selected for an `array_map()` callback result shape.
fn array_map_runtime_label(callback_elem_ty: &PhpType, env_bytes: usize) -> &'static str {
    if callback_elem_ty == &PhpType::Mixed {
        return "__rt_array_map_mixed";
    }
    if callback_elem_ty == &PhpType::Str {
        if env_bytes == 0 {
            "__rt_array_map_str"
        } else {
            "__rt_array_map_str_owned"
        }
    } else {
        "__rt_array_map"
    }
}

/// Lowers `array_reduce()` through the callback-driven runtime helper.
pub(super) fn lower_array_reduce(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "array_reduce", 3)?;
    let array = expect_operand(inst, 0)?;
    let callback = expect_operand(inst, 1)?;
    let initial = expect_operand(inst, 2)?;
    let elem_ty = eight_byte_callback_array_element_type(ctx.value_php_type(array)?, "array_reduce")?;
    let initial_ty = eight_byte_callback_value_type(ctx.value_php_type(initial)?, "array_reduce initial")?;
    match ctx.value_php_type(callback)?.codegen_repr() {
        PhpType::Callable => {
            lower_descriptor_callback_runtime(
                ctx,
                callback,
                vec![initial_ty.clone(), elem_ty.clone()],
                PhpType::Int,
                |ctx, wrapper_label, env_bytes| {
                    let callback_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
                    let array_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 1);
                    let initial_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 2);
                    let env_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 3);
                    abi::emit_symbol_address(ctx.emitter, callback_arg_reg, wrapper_label);
                    ctx.load_value_to_reg(array, array_arg_reg)?;
                    ctx.load_value_to_reg(initial, initial_arg_reg)?;
                    load_static_callback_env_arg(ctx, env_arg_reg, env_bytes);
                    abi::emit_call_label(ctx.emitter, "__rt_array_reduce");
                    Ok(())
                },
            )?;
            box_int_result_for_mixed_builtin(ctx, inst);
            store_if_result(ctx, inst)?;
            return Ok(());
        }
        PhpType::Str => {
            lower_runtime_string_descriptor_callback(
                ctx,
                callback,
                Some(&PhpType::Array(Box::new(elem_ty.clone()))),
                vec![initial_ty.clone(), elem_ty.clone()],
                PhpType::Int,
                "array_reduce",
                |ctx, wrapper_label, env_bytes| {
                    let callback_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
                    let array_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 1);
                    let initial_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 2);
                    let env_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 3);
                    abi::emit_symbol_address(ctx.emitter, callback_arg_reg, wrapper_label);
                    ctx.load_value_to_reg(array, array_arg_reg)?;
                    ctx.load_value_to_reg(initial, initial_arg_reg)?;
                    load_static_callback_env_arg(ctx, env_arg_reg, env_bytes);
                    abi::emit_call_label(ctx.emitter, "__rt_array_reduce");
                    Ok(())
                },
            )?;
            box_int_result_for_mixed_builtin(ctx, inst);
            store_if_result(ctx, inst)?;
            return Ok(());
        }
        _ => {}
    }
    let callback_binding = static_sort_callback_binding(
        ctx,
        callback,
        "array_reduce callback",
        Some(&[initial_ty.clone(), elem_ty]),
    )?;
    let env_bytes = reserve_static_callback_env(ctx, callback_binding.env_source)?;
    let callback_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let array_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    let initial_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    let env_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    abi::emit_symbol_address(ctx.emitter, callback_arg_reg, &callback_binding.label);
    ctx.load_value_to_reg(array, array_arg_reg)?;
    ctx.load_value_to_reg(initial, initial_arg_reg)?;
    load_static_callback_env_arg(ctx, env_arg_reg, env_bytes);
    abi::emit_call_label(ctx.emitter, "__rt_array_reduce");
    if env_bytes != 0 {
        abi::emit_release_temporary_stack(ctx.emitter, env_bytes);
    }
    box_int_result_for_mixed_builtin(ctx, inst);
    store_if_result(ctx, inst)
}

/// Lowers `array_walk()` through the callback-driven runtime helper.
pub(super) fn lower_array_walk(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "array_walk", 2)?;
    let array = expect_operand(inst, 0)?;
    let callback = expect_operand(inst, 1)?;
    let elem_ty = eight_byte_callback_array_element_type(ctx.value_php_type(array)?, "array_walk")?;
    match ctx.value_php_type(callback)?.codegen_repr() {
        PhpType::Callable => {
            lower_descriptor_callback_runtime(
                ctx,
                callback,
                vec![elem_ty.clone()],
                PhpType::Void,
                |ctx, wrapper_label, env_bytes| {
                    let callback_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
                    let array_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 1);
                    let env_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 2);
                    abi::emit_symbol_address(ctx.emitter, callback_arg_reg, wrapper_label);
                    ctx.load_value_to_reg(array, array_arg_reg)?;
                    load_static_callback_env_arg(ctx, env_arg_reg, env_bytes);
                    abi::emit_call_label(ctx.emitter, "__rt_array_walk");
                    Ok(())
                },
            )?;
            store_void_builtin_result(ctx, inst)?;
            return Ok(());
        }
        PhpType::Str => {
            lower_runtime_string_descriptor_callback(
                ctx,
                callback,
                Some(&PhpType::Array(Box::new(elem_ty.clone()))),
                vec![elem_ty.clone()],
                PhpType::Void,
                "array_walk",
                |ctx, wrapper_label, env_bytes| {
                    let callback_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
                    let array_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 1);
                    let env_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 2);
                    abi::emit_symbol_address(ctx.emitter, callback_arg_reg, wrapper_label);
                    ctx.load_value_to_reg(array, array_arg_reg)?;
                    load_static_callback_env_arg(ctx, env_arg_reg, env_bytes);
                    abi::emit_call_label(ctx.emitter, "__rt_array_walk");
                    Ok(())
                },
            )?;
            store_void_builtin_result(ctx, inst)?;
            return Ok(());
        }
        _ => {}
    }
    let callback_binding =
        static_sort_callback_binding(ctx, callback, "array_walk callback", Some(&[elem_ty]))?;
    let env_bytes = reserve_static_callback_env(ctx, callback_binding.env_source)?;
    let callback_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let array_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    let env_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_symbol_address(ctx.emitter, callback_arg_reg, &callback_binding.label);
    ctx.load_value_to_reg(array, array_arg_reg)?;
    load_static_callback_env_arg(ctx, env_arg_reg, env_bytes);
    abi::emit_call_label(ctx.emitter, "__rt_array_walk");
    if env_bytes != 0 {
        abi::emit_release_temporary_stack(ctx.emitter, env_bytes);
    }
    store_void_builtin_result(ctx, inst)
}

/// Lowers `array_merge()` for two compatible indexed arrays with 8-byte payload slots.
pub(super) fn lower_array_merge(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "array_merge", 2)?;
    let first = expect_operand(inst, 0)?;
    let second = expect_operand(inst, 1)?;
    let elem_ty = compatible_eight_byte_indexed_array_element_type(
        ctx.value_php_type(first)?,
        ctx.value_php_type(second)?,
        "array_merge",
    )?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(first, "x0")?;
            ctx.load_value_to_reg(second, "x1")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(first, "rdi")?;
            ctx.load_value_to_reg(second, "rsi")?;
        }
    }
    abi::emit_call_label(ctx.emitter, array_merge_runtime_helper(&elem_ty));
    store_if_result(ctx, inst)
}

/// Lowers `array_diff()` for two compatible indexed arrays with pointer-sized payload slots.
pub(super) fn lower_array_diff(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_indexed_array_set_op(
        ctx,
        inst,
        "array_diff",
        "__rt_array_diff",
        "__rt_array_diff_refcounted",
    )
}

/// Lowers `array_intersect()` for two compatible indexed arrays with pointer-sized payload slots.
pub(super) fn lower_array_intersect(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_indexed_array_set_op(
        ctx,
        inst,
        "array_intersect",
        "__rt_array_intersect",
        "__rt_array_intersect_refcounted",
    )
}

/// Lowers `array_diff_key()` for two associative arrays by filtering first-operand keys.
pub(super) fn lower_array_diff_key(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_assoc_array_key_set_op(ctx, inst, "array_diff_key", "__rt_array_diff_key")
}

/// Lowers `array_intersect_key()` for two associative arrays by keeping shared first-operand keys.
pub(super) fn lower_array_intersect_key(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_assoc_array_key_set_op(ctx, inst, "array_intersect_key", "__rt_array_intersect_key")
}

/// Lowers `array_slice()` for indexed arrays with pointer-sized payload slots.
pub(super) fn lower_array_slice(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "array_slice", 2, 3)?;
    let array = expect_operand(inst, 0)?;
    if matches!(
        ctx.value_php_type(array)?.codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        return lower_mixed_array_slice(ctx, inst);
    }
    let offset = expect_operand(inst, 1)?;
    let length = if inst.operands.len() == 3 {
        Some(expect_operand(inst, 2)?)
    } else {
        None
    };
    let source_elem_ty = array_slice_source_element_type(ctx.value_php_type(array)?)?;
    let result_elem_ty = result_array_element_type("array_slice", &inst.result_php_type.codegen_repr())?;
    require_array_slice_result_type(&source_elem_ty, &result_elem_ty)?;
    lower_array_slice_call(ctx, array, offset, length, &source_elem_ty)?;
    normalize_indexed_array_result(ctx, "array_slice", &source_elem_ty, &result_elem_ty)?;
    store_if_result(ctx, inst)
}

/// Lowers `array_slice()` for an indexed array stored inside a boxed Mixed cell.
fn lower_mixed_array_slice(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let array = expect_operand(inst, 0)?;
    let offset = expect_operand(inst, 1)?;
    let length = if inst.operands.len() == 3 {
        Some(expect_operand(inst, 2)?)
    } else {
        None
    };
    let result_elem_ty = result_array_element_type("array_slice", &inst.result_php_type.codegen_repr())?;
    require_array_slice_result_type(&PhpType::Mixed, &result_elem_ty)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_mixed_array_slice_aarch64(ctx, array, offset, length)?,
        Arch::X86_64 => lower_mixed_array_slice_x86_64(ctx, array, offset, length)?,
    }
    normalize_indexed_array_result(ctx, "array_slice", &PhpType::Mixed, &result_elem_ty)?;
    store_if_result(ctx, inst)
}

/// Lowers `array_splice()` by mutating an indexed source array and returning removed elements.
pub(super) fn lower_array_splice(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "array_splice", 2, 3)?;
    let array = expect_operand(inst, 0)?;
    if matches!(
        ctx.value_php_type(array)?.codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        return lower_mixed_array_splice(ctx, inst);
    }
    let offset = expect_operand(inst, 1)?;
    let length = if inst.operands.len() == 3 {
        Some(expect_operand(inst, 2)?)
    } else {
        None
    };
    let elem_ty = array_pop_element_type(ctx.value_php_type(array)?)?;
    let source_local = source_load_local_slot(ctx, array)?;
    ensure_unique_array_pop_source(ctx, array)?;
    if let Some(slot) = source_local {
        ctx.store_value_to_local(slot, array)?;
    }
    lower_array_splice_call(ctx, array, offset, length, &elem_ty)?;
    normalize_array_splice_result(ctx, &elem_ty, &inst.result_php_type.codegen_repr())?;
    store_if_result(ctx, inst)
}

/// Lowers `array_splice()` for an indexed array stored inside a boxed Mixed cell.
fn lower_mixed_array_splice(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let array = expect_operand(inst, 0)?;
    let offset = expect_operand(inst, 1)?;
    let length = if inst.operands.len() == 3 {
        Some(expect_operand(inst, 2)?)
    } else {
        None
    };
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_mixed_array_splice_aarch64(ctx, array, offset, length)?,
        Arch::X86_64 => lower_mixed_array_splice_x86_64(ctx, array, offset, length)?,
    }
    normalize_array_splice_result(
        ctx,
        &PhpType::Mixed,
        &inst.result_php_type.codegen_repr(),
    )?;
    store_if_result(ctx, inst)
}

/// Lowers `array_values()` through the dedicated values-array builtin emitter.
pub(super) fn lower_array_values(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    values::lower_array_values(ctx, inst)
}

/// Lowers `array_keys()` through the dedicated keys-array builtin emitter.
pub(super) fn lower_array_keys(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    keys::lower_array_keys(ctx, inst)
}

/// Lowers `array_rand()` for indexed arrays.
pub(super) fn lower_array_rand(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "array_rand", 1)?;
    let array = expect_operand(inst, 0)?;
    require_indexed_array_builtin(ctx.value_php_type(array)?, "array_rand")?;
    ctx.load_value_to_result(array)?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the indexed-array pointer as the random-key helper argument
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_rand");
    store_if_result(ctx, inst)
}

/// Lowers `range()` for integer endpoints through the shared runtime constructor.
pub(super) fn lower_range(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "range", 2)?;
    let start = expect_operand(inst, 0)?;
    let end = expect_operand(inst, 1)?;
    require_range_endpoint(ctx.value_php_type(start)?, "start")?;
    require_range_endpoint(ctx.value_php_type(end)?, "end")?;
    require_range_result_type(&inst.result_php_type.codegen_repr())?;
    // Resolve each endpoint to a plain integer, unboxing a Mixed cell read from a heterogeneous
    // array. The end resolution may call __rt_mixed_cast_int, which clobbers caller-saved registers,
    // so the resolved start is spilled across it instead of being staged in an argument register.
    resolve_int_operand_to_result(ctx, start, "range start")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    resolve_int_operand_to_result(ctx, end, "range end")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                              // move the resolved range end into the second runtime argument
            abi::emit_pop_reg(ctx.emitter, "x0");                                   // restore the resolved range start into the first runtime argument
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, rax");                            // move the resolved range end into the second runtime argument
            abi::emit_pop_reg(ctx.emitter, "rdi");                                  // restore the resolved range start into the first runtime argument
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_range");
    store_if_result(ctx, inst)
}

/// Lowers `array_pop()` for indexed arrays by mutating length and boxing `T|null` as Mixed.
pub(super) fn lower_array_pop(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "array_pop", 1)?;
    let array = expect_operand(inst, 0)?;
    let elem_ty = array_pop_element_type(ctx.value_php_type(array)?)?;
    require_array_pop_result_type(&inst.result_php_type.codegen_repr())?;
    let source_local = source_load_local_slot(ctx, array)?;
    ensure_unique_array_pop_source(ctx, array)?;
    if let Some(slot) = source_local {
        ctx.store_value_to_local(slot, array)?;
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_array_pop_aarch64(ctx, array, &elem_ty)?,
        Arch::X86_64 => lower_array_pop_x86_64(ctx, array, &elem_ty)?,
    }
    store_if_result(ctx, inst)
}

/// Lowers `array_shift()` for indexed arrays by compacting slots and boxing `T|null` as Mixed.
pub(super) fn lower_array_shift(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    shift::lower_array_shift(ctx, inst)
}

/// Lowers `array_unshift()` for indexed arrays by prepending a scalar payload.
pub(super) fn lower_array_unshift(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    unshift::lower_array_unshift(ctx, inst)
}

/// Lowers `sort()` for indexed integer arrays by mutating the source array in place.
pub(super) fn lower_sort(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_indexed_array_sort(ctx, inst, "sort", "__rt_sort_int", Some("__rt_sort_str"))
}

/// Lowers `rsort()` for indexed integer arrays by mutating the source array in place.
pub(super) fn lower_rsort(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_indexed_array_sort(ctx, inst, "rsort", "__rt_rsort_int", Some("__rt_rsort_str"))
}

/// Lowers `asort()` for indexed integer arrays through the value-sort runtime wrapper.
pub(super) fn lower_asort(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_indexed_array_sort(ctx, inst, "asort", "__rt_asort", None)
}

/// Lowers `arsort()` for indexed integer arrays through the descending value-sort wrapper.
pub(super) fn lower_arsort(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_indexed_array_sort(ctx, inst, "arsort", "__rt_arsort", None)
}

/// Lowers `ksort()` through the legacy key-sort helper surface.
pub(super) fn lower_ksort(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_array_key_sort(ctx, inst, "ksort", "__rt_ksort")
}

/// Lowers `krsort()` through the legacy reverse key-sort helper surface.
pub(super) fn lower_krsort(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_array_key_sort(ctx, inst, "krsort", "__rt_krsort")
}

/// Lowers `natsort()` for indexed integer arrays through the natural-sort runtime wrapper.
pub(super) fn lower_natsort(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_indexed_array_sort(ctx, inst, "natsort", "__rt_natsort", None)
}

/// Lowers `natcasesort()` for indexed integer arrays through the case-insensitive wrapper.
pub(super) fn lower_natcasesort(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_indexed_array_sort(ctx, inst, "natcasesort", "__rt_natcasesort", None)
}

/// Lowers `shuffle()` for indexed arrays with 8-byte slots by mutating the source array in place.
pub(super) fn lower_shuffle(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_indexed_array_shuffle(ctx, inst)
}

/// Lowers `usort()` for indexed integer arrays with a static user comparator.
pub(super) fn lower_usort(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_user_sort_static_callback(ctx, inst, "usort")
}

/// Lowers `uksort()` through the legacy user-sort helper for static comparators.
pub(super) fn lower_uksort(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_user_sort_static_callback(ctx, inst, "uksort")
}

/// Lowers `uasort()` through the legacy user-sort helper for static comparators.
pub(super) fn lower_uasort(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_user_sort_static_callback(ctx, inst, "uasort")
}

/// Lowers `array_key_exists()` through the dedicated key-existence builtin emitter.
pub(super) fn lower_array_key_exists(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    key_exists::lower_array_key_exists(ctx, inst)
}

/// Lowers `array_search()` for indexed arrays with integer-like payloads.
pub(super) fn lower_array_search(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "array_search", 2)?;
    let needle = expect_operand(inst, 0)?;
    let array = expect_operand(inst, 1)?;
    let needle_ty = ctx.value_php_type(needle)?;
    let array_ty = ctx.value_php_type(array)?;
    if search::try_lower_assoc_array_search(ctx, needle, array, needle_ty.clone(), array_ty.clone())? {
        store_if_result(ctx, inst)?;
        return Ok(());
    }
    match supported_array_search_case(needle_ty, array_ty)? {
        ArraySearchCase::Empty => box_array_search_miss(ctx),
        ArraySearchCase::Scalar => lower_array_search_scalar(ctx, needle, array)?,
        ArraySearchCase::String => lower_array_search_string(ctx, needle, array)?,
    }
    store_if_result(ctx, inst)
}

/// Lowers `in_array()` for indexed arrays with scalar or string payloads.
pub(super) fn lower_in_array(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "in_array", 2)?;
    let needle = expect_operand(inst, 0)?;
    let array = expect_operand(inst, 1)?;
    let needle_ty = ctx.value_php_type(needle)?;
    let array_ty = ctx.value_php_type(array)?;
    if search::try_lower_assoc_in_array(ctx, needle, array, needle_ty.clone(), array_ty.clone())? {
        store_if_result(ctx, inst)?;
        return Ok(());
    }
    match supported_in_array_case(needle_ty, array_ty)? {
        InArrayCase::Empty => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
        InArrayCase::Scalar => lower_in_array_scalar(ctx, needle, array)?,
        InArrayCase::String => lower_in_array_string(ctx, needle, array)?,
        InArrayCase::MixedString => lower_in_array_mixed_string(ctx, needle, array)?,
    }
    store_if_result(ctx, inst)
}

/// Loads an indexed array argument and calls the selected runtime aggregate helper.
fn lower_indexed_array_aggregate(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    helper: &str,
) -> Result<()> {
    super::ensure_arg_count(inst, name, 1)?;
    let array = expect_operand(inst, 0)?;
    require_supported_indexed_array(ctx.value_php_type(array)?, name)?;
    ctx.load_value_to_result(array)?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the indexed-array pointer as the runtime helper argument
    }
    abi::emit_call_label(ctx.emitter, helper);
    store_if_result(ctx, inst)
}

/// Calls a value set-operation helper after validating compatible indexed-array layouts.
fn lower_indexed_array_set_op(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    scalar_helper: &str,
    refcounted_helper: &str,
) -> Result<()> {
    super::ensure_arg_count(inst, name, 2)?;
    let first = expect_operand(inst, 0)?;
    let second = expect_operand(inst, 1)?;
    let first_elem_ty = set_op_indexed_array_element_type(ctx.value_php_type(first)?, name)?;
    let second_elem_ty = set_op_indexed_array_element_type(ctx.value_php_type(second)?, name)?;
    require_set_op_compatible_element_types(name, &first_elem_ty, &second_elem_ty)?;
    require_set_op_result_type(name, &first_elem_ty, &inst.result_php_type.codegen_repr())?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(first, "x0")?;
            ctx.load_value_to_reg(second, "x1")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(first, "rdi")?;
            ctx.load_value_to_reg(second, "rsi")?;
        }
    }
    let helper = if first_elem_ty.is_refcounted() {
        refcounted_helper
    } else {
        scalar_helper
    };
    abi::emit_call_label(ctx.emitter, helper);
    crate::codegen::emit_array_value_type_stamp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        &first_elem_ty,
    );
    store_if_result(ctx, inst)
}

/// Calls a key set-operation helper after validating associative-array hash operands.
fn lower_assoc_array_key_set_op(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    helper: &str,
) -> Result<()> {
    super::ensure_arg_count(inst, name, 2)?;
    let first = expect_operand(inst, 0)?;
    let second = expect_operand(inst, 1)?;
    let first_ty = assoc_array_key_set_operand_type(ctx.value_php_type(first)?, name, "first")?;
    let _second_ty = assoc_array_key_set_operand_type(ctx.value_php_type(second)?, name, "second")?;
    require_assoc_array_key_set_result_type(name, &first_ty, &inst.result_php_type.codegen_repr())?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(first, "x0")?;
            ctx.load_value_to_reg(second, "x1")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(first, "rdi")?;
            ctx.load_value_to_reg(second, "rsi")?;
        }
    }
    abi::emit_call_label(ctx.emitter, helper);
    store_if_result(ctx, inst)
}

/// Calls a mutating indexed-array sort helper after copy-on-write splitting.
fn lower_indexed_array_sort(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    int_helper: &str,
    str_helper: Option<&str>,
) -> Result<()> {
    super::ensure_arg_count(inst, name, 1)?;
    let array = expect_operand(inst, 0)?;
    let elem_ty = indexed_sort_element_type(ctx.value_php_type(array)?, name, str_helper.is_some())?;
    let source_local = source_load_local_slot(ctx, array)?;
    ensure_unique_sort_source(ctx, array)?;
    if let Some(slot) = source_local {
        ctx.store_value_to_local(slot, array)?;
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(array, "x0")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(array, "rdi")?;
        }
    }
    let helper = if elem_ty == PhpType::Str {
        str_helper.expect("string sort helper is required after validation")
    } else {
        int_helper
    };
    abi::emit_call_label(ctx.emitter, helper);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        0x7fff_ffff_ffff_fffe,
    );
    store_if_result(ctx, inst)
}

/// Calls the mutating shuffle helper for indexed arrays whose payload slots are pointer-sized.
fn lower_indexed_array_shuffle(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "shuffle", 1)?;
    let array = expect_operand(inst, 0)?;
    eight_byte_indexed_array_element_type(ctx.value_php_type(array)?, "shuffle")?;
    let source_local = source_load_local_slot(ctx, array)?;
    ensure_unique_sort_source(ctx, array)?;
    if let Some(slot) = source_local {
        ctx.store_value_to_local(slot, array)?;
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(array, "x0")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(array, "rdi")?;
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_shuffle");
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        0x7fff_ffff_ffff_fffe,
    );
    store_if_result(ctx, inst)
}

/// Calls the legacy user-sort helper with a static comparator and optional late-static environment.
fn lower_user_sort_static_callback(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    super::ensure_arg_count(inst, name, 2)?;
    let array = expect_operand(inst, 0)?;
    let callback = expect_operand(inst, 1)?;
    user_sort_element_type(ctx.value_php_type(array)?, name)?;
    let source_local = source_load_local_slot(ctx, array)?;
    ensure_unique_sort_source(ctx, array)?;
    if let Some(slot) = source_local {
        ctx.store_value_to_local(slot, array)?;
    }
    let callback_ty = ctx.value_php_type(callback)?.codegen_repr();
    let callback_owner = format!("{} callback", name);
    if callback_ty == PhpType::Callable && static_callback_operand_is_recoverable(ctx, callback) {
        let callback_binding =
            static_sort_callback_binding(ctx, callback, &callback_owner, Some(&[PhpType::Int, PhpType::Int]))?;
        return lower_user_sort_with_static_callback_binding(ctx, inst, array, callback_binding);
    }
    match callback_ty {
        PhpType::Callable => {
            lower_descriptor_callback_runtime(
                ctx,
                callback,
                vec![PhpType::Int, PhpType::Int],
                PhpType::Int,
                |ctx, wrapper_label, env_bytes| {
                    let callback_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
                    let array_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 1);
                    let env_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 2);
                    abi::emit_symbol_address(ctx.emitter, callback_arg_reg, wrapper_label);
                    ctx.load_value_to_reg(array, array_arg_reg)?;
                    load_static_callback_env_arg(ctx, env_arg_reg, env_bytes);
                    abi::emit_call_label(ctx.emitter, "__rt_usort");
                    Ok(())
                },
            )?;
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                0x7fff_ffff_ffff_fffe,
            );
            store_if_result(ctx, inst)?;
            return Ok(());
        }
        PhpType::Str => {
            lower_runtime_string_descriptor_callback(
                ctx,
                callback,
                Some(&PhpType::Array(Box::new(PhpType::Int))),
                vec![PhpType::Int, PhpType::Int],
                PhpType::Int,
                name,
                |ctx, wrapper_label, env_bytes| {
                    let callback_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
                    let array_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 1);
                    let env_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 2);
                    abi::emit_symbol_address(ctx.emitter, callback_arg_reg, wrapper_label);
                    ctx.load_value_to_reg(array, array_arg_reg)?;
                    load_static_callback_env_arg(ctx, env_arg_reg, env_bytes);
                    abi::emit_call_label(ctx.emitter, "__rt_usort");
                    Ok(())
                },
            )?;
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                0x7fff_ffff_ffff_fffe,
            );
            store_if_result(ctx, inst)?;
            return Ok(());
        }
        _ => {}
    }
    let callback_binding =
        static_sort_callback_binding(ctx, callback, &callback_owner, Some(&[PhpType::Int, PhpType::Int]))?;
    lower_user_sort_with_static_callback_binding(ctx, inst, array, callback_binding)
}

/// Calls the user-sort runtime with a statically recovered callback binding.
fn lower_user_sort_with_static_callback_binding(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
    callback_binding: StaticSortCallbackBinding,
) -> Result<()> {
    let env_bytes = reserve_static_callback_env(ctx, callback_binding.env_source)?;
    let callback_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let array_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    let env_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_symbol_address(ctx.emitter, callback_arg_reg, &callback_binding.label);
    ctx.load_value_to_reg(array, array_arg_reg)?;
    load_static_callback_env_arg(ctx, env_arg_reg, env_bytes);
    abi::emit_call_label(ctx.emitter, "__rt_usort");
    if env_bytes != 0 {
        abi::emit_release_temporary_stack(ctx.emitter, env_bytes);
    }
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        0x7fff_ffff_ffff_fffe,
    );
    store_if_result(ctx, inst)
}

/// Calls the legacy key-sort helper for array-like values.
fn lower_array_key_sort(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    helper: &str,
) -> Result<()> {
    super::ensure_arg_count(inst, name, 1)?;
    let array = expect_operand(inst, 0)?;
    require_array_key_sort_type(ctx.value_php_type(array)?, name)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(array, "x0")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(array, "rdi")?;
        }
    }
    abi::emit_call_label(ctx.emitter, helper);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        0x7fff_ffff_ffff_fffe,
    );
    store_if_result(ctx, inst)
}

/// Returns the indexed-array element type accepted by the selected sort helper.
fn indexed_sort_element_type(
    ty: PhpType,
    name: &str,
    allow_strings: bool,
) -> Result<PhpType> {
    match ty.codegen_repr() {
        PhpType::Array(elem) => {
            let elem = elem.codegen_repr();
            if matches!(elem, PhpType::Int | PhpType::Void | PhpType::Never)
                || (allow_strings && elem == PhpType::Str)
            {
                return Ok(elem);
            }
            Err(CodegenIrError::unsupported(format!(
                "{} indexed-array element PHP type {:?}",
                name,
                elem
            )))
        }
        other => Err(CodegenIrError::unsupported(format!("{} for PHP type {:?}", name, other))),
    }
}

/// Returns the indexed-array element type accepted by a user-comparator sort.
///
/// User-comparator sorts (`usort`/`uasort`/`uksort`) permute existing
/// pointer-sized slots through `__rt_usort`, so integer and object/refcounted
/// handles (each a single 8-byte payload) are sortable; the comparator decides
/// the ordering and receives each element by its handle. String elements are
/// rejected here exactly as before — their multi-word descriptors are not
/// permuted by the 8-byte slot sorter — so they keep producing a clear
/// unsupported-feature error rather than a corrupt sort.
fn user_sort_element_type(ty: PhpType, name: &str) -> Result<PhpType> {
    match ty.codegen_repr() {
        PhpType::Array(elem) => {
            let elem = elem.codegen_repr();
            if matches!(
                elem,
                PhpType::Int | PhpType::Void | PhpType::Never | PhpType::Object(_)
            ) {
                return Ok(elem);
            }
            Err(CodegenIrError::unsupported(format!(
                "{} indexed-array element PHP type {:?}",
                name, elem
            )))
        }
        other => Err(CodegenIrError::unsupported(format!("{} for PHP type {:?}", name, other))),
    }
}

/// Verifies key-sort helpers only receive array-like PHP values.
fn require_array_key_sort_type(ty: PhpType, name: &str) -> Result<()> {
    match ty.codegen_repr() {
        PhpType::Array(_) | PhpType::AssocArray { .. } => Ok(()),
        other => Err(CodegenIrError::unsupported(format!("{} for PHP type {:?}", name, other))),
    }
}

/// Splits a shared indexed array before a sort helper mutates its slots in place.
fn ensure_unique_sort_source(ctx: &mut FunctionContext<'_>, array: ValueId) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(array, "x0")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(array, "rdi")?;
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_ensure_unique");
    ctx.store_result_value(array)
}

/// Verifies the aggregate can use the current raw integer-slot runtime helper.
fn require_supported_indexed_array(ty: PhpType, name: &str) -> Result<()> {
    match ty.codegen_repr() {
        PhpType::Array(elem) if matches!(*elem, PhpType::Int | PhpType::Bool | PhpType::Never) => Ok(()),
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            name,
            other
        ))),
    }
}

/// Returns the indexed-array element type supported by the current filter runtime helpers.
fn array_filter_source_element_type(ty: PhpType) -> Result<PhpType> {
    match ty.codegen_repr() {
        PhpType::Array(elem) => {
            let elem = elem.codegen_repr();
            if matches!(
                elem,
                PhpType::Int | PhpType::Bool | PhpType::Str | PhpType::Void | PhpType::Never
            ) || elem.is_refcounted()
            {
                return Ok(elem);
            }
            Err(CodegenIrError::unsupported(format!(
                "array_filter indexed-array element PHP type {:?}",
                elem
            )))
        }
        other => Err(CodegenIrError::unsupported(format!(
            "array_filter for PHP type {:?}",
            other
        ))),
    }
}

/// Verifies the filtered result preserves the source element type metadata.
fn require_array_filter_result_type(source_elem_ty: &PhpType, result_ty: &PhpType) -> Result<()> {
    match result_ty {
        PhpType::Array(elem)
            if elem.codegen_repr() == source_elem_ty.codegen_repr()
                || matches!(source_elem_ty, PhpType::Never | PhpType::Void) =>
        {
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "array_filter result PHP type {:?} for source element PHP type {:?}",
            other,
            source_elem_ty
        ))),
    }
}

/// Returns true when filtering should preserve/copy refcounted payload slots.
fn array_filter_uses_refcounted_runtime(elem_ty: &PhpType) -> bool {
    elem_ty.is_refcounted() || matches!(elem_ty.codegen_repr(), PhpType::Str)
}

/// Loads the optional `array_filter()` mode operand into the runtime helper register.
fn load_array_filter_mode(
    ctx: &mut FunctionContext<'_>,
    mode: Option<ValueId>,
    reg: &str,
) -> Result<()> {
    if let Some(mode) = mode {
        ctx.load_value_to_reg(mode, reg)?;
    } else {
        abi::emit_load_int_immediate(ctx.emitter, reg, 0);
    }
    Ok(())
}

/// Returns the visible callback argument types for `array_filter()` mode.
fn array_filter_callback_arg_types(
    ctx: &FunctionContext<'_>,
    mode: Option<ValueId>,
    elem_ty: &PhpType,
) -> Result<Option<Vec<PhpType>>> {
    match static_array_filter_mode(ctx, mode)? {
        Some(1) => Ok(Some(vec![elem_ty.codegen_repr(), PhpType::Int])),
        Some(2) => Ok(Some(vec![PhpType::Int])),
        Some(_) => Ok(Some(vec![elem_ty.codegen_repr()])),
        None => Ok(None),
    }
}

/// Returns a compile-time `array_filter()` mode when it is visible in EIR.
fn static_array_filter_mode(ctx: &FunctionContext<'_>, mode: Option<ValueId>) -> Result<Option<i64>> {
    let Some(mode) = mode else {
        return Ok(Some(0));
    };
    array_filter_mode_const_i64(ctx, mode)
}

/// Returns a visible integer mode from a direct constant or same-block local load.
fn array_filter_mode_const_i64(ctx: &FunctionContext<'_>, value: ValueId) -> Result<Option<i64>> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { block, index, inst } = value_ref.def else {
        return Ok(None);
    };
    let Some(inst_ref) = ctx.function.instruction(inst) else {
        return Err(CodegenIrError::missing_entry("instruction", inst.as_raw()));
    };
    let inst_ref = if inst_ref.op == Op::LoadLocal {
        let Some(inst_ref) = array_filter_local_mode_source_instruction(ctx, block, index, inst_ref)? else {
            return Ok(None);
        };
        inst_ref
    } else {
        inst_ref
    };
    if inst_ref.op != Op::ConstI64 {
        return Ok(None);
    }
    let Some(Immediate::I64(value)) = inst_ref.immediate else {
        return Err(CodegenIrError::invalid_module(
            "array_filter mode const_i64 has no immediate",
        ));
    };
    Ok(Some(value))
}

/// Resolves an `array_filter()` mode local load to the last same-block store before it.
fn array_filter_local_mode_source_instruction<'a>(
    ctx: &'a FunctionContext<'_>,
    block: BlockId,
    load_index: u32,
    load_inst: &Instruction,
) -> Result<Option<&'a Instruction>> {
    let Some(Immediate::LocalSlot(slot)) = load_inst.immediate else {
        return Err(CodegenIrError::invalid_module(
            "array_filter mode load_local has no local slot",
        ));
    };
    let block_ref = ctx
        .function
        .block(block)
        .ok_or_else(|| CodegenIrError::missing_entry("block", block.as_raw()))?;
    let mut stored = None;
    for (index, inst_id) in block_ref.instructions.iter().enumerate() {
        if index as u32 >= load_index {
            break;
        }
        let inst_ref = ctx
            .function
            .instruction(*inst_id)
            .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst_id.as_raw()))?;
        if inst_ref.op == Op::StoreLocal
            && matches!(inst_ref.immediate, Some(Immediate::LocalSlot(candidate)) if candidate == slot)
        {
            stored = inst_ref.operands.first().copied();
        }
    }
    let Some(stored) = stored else {
        return Ok(None);
    };
    let Some(value_ref) = ctx.function.value(stored) else {
        return Err(CodegenIrError::missing_entry("value", stored.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(None);
    };
    ctx.function
        .instruction(inst)
        .map(Some)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))
}

/// Returns an indexed-array element type compatible with callback runtime helpers.
fn eight_byte_callback_array_element_type(ty: PhpType, name: &str) -> Result<PhpType> {
    match ty.codegen_repr() {
        PhpType::Array(elem) => eight_byte_callback_value_type(*elem, name),
        other => Err(CodegenIrError::unsupported(format!("{} for PHP type {:?}", name, other))),
    }
}

/// Returns the indexed-array element type accepted by `array_map()` callback runtimes.
fn array_map_callback_array_element_type(ty: PhpType) -> Result<PhpType> {
    match ty.codegen_repr() {
        PhpType::Array(elem) => {
            let elem = elem.codegen_repr();
            if matches!(elem, PhpType::Int | PhpType::Bool | PhpType::Str | PhpType::Void | PhpType::Never) {
                return Ok(elem);
            }
            Err(CodegenIrError::unsupported(format!(
                "array_map indexed-array element PHP type {:?}",
                elem
            )))
        }
        other => Err(CodegenIrError::unsupported(format!(
            "array_map for PHP type {:?}",
            other
        ))),
    }
}

/// Returns a scalar callback value type that fits in one integer ABI register.
fn eight_byte_callback_value_type(ty: PhpType, name: &str) -> Result<PhpType> {
    let ty = ty.codegen_repr();
    if matches!(ty, PhpType::Int | PhpType::Bool | PhpType::Void | PhpType::Never) {
        Ok(ty)
    } else {
        Err(CodegenIrError::unsupported(format!(
            "{} PHP type {:?}",
            name,
            ty
        )))
    }
}

/// Boxes the integer runtime result when the EIR builtin result slot is Mixed-like.
fn box_int_result_for_mixed_builtin(ctx: &mut FunctionContext<'_>, inst: &Instruction) {
    if inst.result.is_some()
        && matches!(
            inst.result_php_type.codegen_repr(),
            PhpType::Mixed | PhpType::Union(_)
        )
    {
        emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
    }
}

/// Stores the void sentinel, boxing it when the EIR builtin result slot is Mixed-like.
fn store_void_builtin_result(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        0x7fff_ffff_ffff_fffe,
    );
    if inst.result.is_some()
        && matches!(
            inst.result_php_type.codegen_repr(),
            PhpType::Mixed | PhpType::Union(_)
        )
    {
        emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Void);
    }
    store_if_result(ctx, inst)
}

/// Returns the indexed-array slot type produced by the selected `array_map()` runtime helper.
fn array_map_callback_result_element_type(return_ty: &PhpType) -> Result<PhpType> {
    let return_ty = return_ty.codegen_repr();
    if matches!(return_ty, PhpType::Int | PhpType::Bool | PhpType::Str) {
        Ok(return_ty)
    } else {
        Err(CodegenIrError::unsupported(format!(
            "array_map callback return PHP type {:?}",
            return_ty
        )))
    }
}

/// Returns the descriptor callback result element type from the EIR result slot metadata.
fn array_map_descriptor_callback_result_element_type(inst: &Instruction) -> Result<PhpType> {
    match inst.result_php_type.codegen_repr() {
        PhpType::Array(elem) => {
            let elem = elem.codegen_repr();
            if matches!(elem, PhpType::Int | PhpType::Bool | PhpType::Str | PhpType::Mixed) {
                Ok(elem)
            } else {
                Err(CodegenIrError::unsupported(format!(
                    "array_map descriptor callback result element PHP type {:?}",
                    elem
                )))
            }
        }
        PhpType::Mixed | PhpType::Union(_) => Ok(PhpType::Mixed),
        other => Err(CodegenIrError::unsupported(format!(
            "array_map descriptor callback result PHP type {:?}",
            other
        ))),
    }
}

/// Returns the element type expected by the EIR `array_map()` result slot.
fn array_map_result_element_type(inst: &Instruction, callback_elem_ty: &PhpType) -> Result<PhpType> {
    match inst.result_php_type.codegen_repr() {
        PhpType::Array(elem) => {
            let result_elem_ty = elem.codegen_repr();
            if &result_elem_ty == callback_elem_ty || result_elem_ty == PhpType::Mixed {
                Ok(result_elem_ty)
            } else {
                Err(CodegenIrError::unsupported(format!(
                    "array_map result element PHP type {:?} for callback result PHP type {:?}",
                    result_elem_ty,
                    callback_elem_ty
                )))
            }
        }
        PhpType::Mixed | PhpType::Union(_) => Ok(callback_elem_ty.clone()),
        other => Err(CodegenIrError::unsupported(format!(
            "array_map result PHP type {:?}",
            other
        ))),
    }
}

/// Boxes an indexed-array result when the EIR builtin result slot is Mixed-like.
fn box_array_result_for_mixed_builtin(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    elem_ty: &PhpType,
) {
    if inst.result.is_some()
        && matches!(
            inst.result_php_type.codegen_repr(),
            PhpType::Mixed | PhpType::Union(_)
        )
    {
        emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Array(Box::new(elem_ty.clone())));
    }
}

/// Callback label, return type, and optional environment source for callback runtime helpers.
struct StaticSortCallbackBinding {
    label: String,
    env_source: Option<StaticCallbackEnvSource>,
    return_ty: PhpType,
}

/// Returns a static callback binding for callback runtimes, including late-static env when needed.
fn static_sort_callback_binding(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    owner: &str,
    visible_arg_types: Option<&[PhpType]>,
) -> Result<StaticSortCallbackBinding> {
    let callback = match static_callable_array_callback_name(ctx, value, owner)? {
        Some(callback) => callback,
        None => static_callback_name_operand(ctx, value, owner)?,
    };
    if let Some(callee) = ctx.callable_function_by_name(&callback.name) {
        return Ok(StaticSortCallbackBinding {
            label: function_symbol(&callee.name),
            env_source: None,
            return_ty: callee.return_php_type.codegen_repr(),
        });
    }
    if callback.kind == StaticCallbackOperandKind::FirstClassCallable {
        if let Some(target) = instance_method_sort_callback_target(ctx, &callback, owner, visible_arg_types)? {
            let visible_arg_types =
                visible_arg_types.expect("instance sort callback target requires known argument types");
            let label = emit_instance_method_callback_wrapper(ctx, &target, visible_arg_types);
            return Ok(StaticSortCallbackBinding {
                label,
                env_source: Some(StaticCallbackEnvSource::Value(target.receiver)),
                return_ty: target.return_ty,
            });
        }
        if let Some(target) = static_method_sort_callback_target(ctx, &callback.name, owner, visible_arg_types)? {
            let visible_arg_types =
                visible_arg_types.expect("static sort callback target requires known argument types");
            let label = emit_static_method_callback_wrapper(ctx, &target, visible_arg_types);
            return Ok(StaticSortCallbackBinding {
                label,
                env_source: target.env_source,
                return_ty: target.return_ty,
            });
        }
    }
    Err(CodegenIrError::unsupported(format!(
        "{} '{}' is not a user function or supported first-class static method",
        owner,
        callback.name
    )))
}

/// Recovers a static `[class, method]` callable array as a static-method callback name.
fn static_callable_array_callback_name(
    ctx: &FunctionContext<'_>,
    value: ValueId,
    owner: &str,
) -> Result<Option<StaticCallbackName>> {
    let Some((array, block, limit_index)) = static_callable_array_source(ctx, value, owner)? else {
        return Ok(None);
    };
    let items = static_callable_array_items(ctx, array, block, limit_index)?;
    let [receiver, method] = items.as_slice() else {
        return Ok(None);
    };
    let Some(method_name) = static_callback_const_string(ctx, *method)? else {
        return Ok(None);
    };
    if static_callback_object_receiver(ctx, *receiver)? {
        return Ok(Some(StaticCallbackName {
            name: format!("object::{}", method_name),
            kind: StaticCallbackOperandKind::FirstClassCallable,
            receiver: Some(*receiver),
        }));
    }
    let Some(class_name) = static_callback_const_string(ctx, *receiver)? else {
        return Ok(None);
    };
    Ok(Some(StaticCallbackName {
        name: format!("{}::{}", class_name, method_name),
        kind: StaticCallbackOperandKind::FirstClassCallable,
        receiver: None,
    }))
}

/// Returns the backing array value for a same-block static callable-array operand.
fn static_callable_array_source(
    ctx: &FunctionContext<'_>,
    value: ValueId,
    owner: &str,
) -> Result<Option<(ValueId, BlockId, u32)>> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { block, index, inst } = value_ref.def else {
        return Ok(None);
    };
    let Some(inst_ref) = ctx.function.instruction(inst) else {
        return Err(CodegenIrError::missing_entry("instruction", inst.as_raw()));
    };
    let candidate = if inst_ref.op == Op::LoadLocal {
        let Some(stored) = static_callback_local_stored_value(ctx, block, index, inst_ref, owner)? else {
            return Ok(None);
        };
        stored
    } else {
        value
    };
    let array = strip_static_callback_acquire(ctx, candidate)?;
    if value_defining_op(ctx, array)? == Some(Op::ArrayNew) {
        let (array_block, _) = value_instruction_location(ctx, array)?;
        let limit_index = if array_block == block { index } else { u32::MAX };
        Ok(Some((array, array_block, limit_index)))
    } else {
        Ok(None)
    }
}

/// Resolves the last same-block local store before a callback local load.
fn static_callback_local_stored_value(
    ctx: &FunctionContext<'_>,
    block: BlockId,
    load_index: u32,
    load_inst: &Instruction,
    owner: &str,
) -> Result<Option<ValueId>> {
    let Some(Immediate::LocalSlot(slot)) = load_inst.immediate else {
        return Err(CodegenIrError::invalid_module(format!(
            "{} load_local callback has no local slot",
            owner
        )));
    };
    let block_ref = ctx
        .function
        .block(block)
        .ok_or_else(|| CodegenIrError::missing_entry("block", block.as_raw()))?;
    let mut stored = None;
    for (index, inst_id) in block_ref.instructions.iter().enumerate() {
        if index as u32 >= load_index {
            break;
        }
        let inst_ref = ctx
            .function
            .instruction(*inst_id)
            .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst_id.as_raw()))?;
        if inst_ref.op == Op::StoreLocal
            && matches!(inst_ref.immediate, Some(Immediate::LocalSlot(candidate)) if candidate == slot)
        {
            stored = inst_ref.operands.first().copied();
        }
    }
    if stored.is_none() {
        stored = unique_static_callback_local_store(ctx, slot)?;
    }
    Ok(stored)
}

/// Returns the stored value for a callback local only when the function writes it once.
fn unique_static_callback_local_store(
    ctx: &FunctionContext<'_>,
    slot: LocalSlotId,
) -> Result<Option<ValueId>> {
    let mut stored = None;
    for block in &ctx.function.blocks {
        for inst_id in &block.instructions {
            let inst_ref = ctx
                .function
                .instruction(*inst_id)
                .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst_id.as_raw()))?;
            if inst_ref.op == Op::StoreLocal
                && matches!(inst_ref.immediate, Some(Immediate::LocalSlot(candidate)) if candidate == slot)
            {
                if stored.is_some() {
                    return Ok(None);
                }
                stored = inst_ref.operands.first().copied();
            }
        }
    }
    Ok(stored)
}

/// Removes a refcount acquire wrapper from a static callback-array value.
fn strip_static_callback_acquire(ctx: &FunctionContext<'_>, value: ValueId) -> Result<ValueId> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(value);
    };
    let Some(inst_ref) = ctx.function.instruction(inst) else {
        return Err(CodegenIrError::missing_entry("instruction", inst.as_raw()));
    };
    if inst_ref.op == Op::Acquire {
        Ok(inst_ref.operands.first().copied().unwrap_or(value))
    } else {
        Ok(value)
    }
}

/// Returns the defining opcode for an SSA value when it comes from an instruction.
fn value_defining_op(ctx: &FunctionContext<'_>, value: ValueId) -> Result<Option<Op>> {
    let (inst, _) = match value_instruction(ctx, value)? {
        Some(location) => location,
        None => return Ok(None),
    };
    Ok(Some(inst.op))
}

/// Returns the instruction and block location that define an SSA value.
fn value_instruction<'a>(
    ctx: &'a FunctionContext<'_>,
    value: ValueId,
) -> Result<Option<(&'a Instruction, BlockId)>> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { block, inst, .. } = value_ref.def else {
        return Ok(None);
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    Ok(Some((inst_ref, block)))
}

/// Returns the block and instruction index that define an instruction-backed SSA value.
fn value_instruction_location(
    ctx: &FunctionContext<'_>,
    value: ValueId,
) -> Result<(BlockId, u32)> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { block, index, .. } = value_ref.def else {
        return Err(CodegenIrError::invalid_module(
            "static callable-array source is not instruction-backed",
        ));
    };
    Ok((block, index))
}

/// Collects item values pushed into a static callable-array literal before use.
fn static_callable_array_items(
    ctx: &FunctionContext<'_>,
    array: ValueId,
    block: BlockId,
    limit_index: u32,
) -> Result<Vec<ValueId>> {
    let block_ref = ctx
        .function
        .block(block)
        .ok_or_else(|| CodegenIrError::missing_entry("block", block.as_raw()))?;
    let mut items = Vec::new();
    for (index, inst_id) in block_ref.instructions.iter().enumerate() {
        if index as u32 >= limit_index {
            break;
        }
        let inst_ref = ctx
            .function
            .instruction(*inst_id)
            .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst_id.as_raw()))?;
        if inst_ref.op == Op::ArrayPush && inst_ref.operands.first().copied() == Some(array) {
            let Some(item) = inst_ref.operands.get(1).copied() else {
                return Err(CodegenIrError::invalid_module(
                    "callable array push missing value operand",
                ));
            };
            items.push(item);
        }
    }
    Ok(items)
}

/// Returns true when a callable-array receiver item is a statically typed object value.
fn static_callback_object_receiver(ctx: &FunctionContext<'_>, value: ValueId) -> Result<bool> {
    Ok(matches!(
        ctx.value_php_type(value)?.codegen_repr(),
        PhpType::Object(_)
    ))
}

/// Returns a constant string value used by a static callable-array item.
fn static_callback_const_string(
    ctx: &FunctionContext<'_>,
    value: ValueId,
) -> Result<Option<String>> {
    let value = strip_static_callback_acquire(ctx, value)?;
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(None);
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    if inst_ref.op != Op::ConstStr {
        return Ok(None);
    }
    let Some(Immediate::Data(data)) = inst_ref.immediate else {
        return Err(CodegenIrError::invalid_module(
            "callable array const_str item has no data id",
        ));
    };
    ctx.module
        .data
        .strings
        .get(data.as_raw() as usize)
        .cloned()
        .map(Some)
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))
}

/// Static callback operand metadata recovered from a literal-producing EIR instruction.
struct StaticCallbackName {
    name: String,
    kind: StaticCallbackOperandKind,
    receiver: Option<ValueId>,
}

/// Classifies whether a static callback came from a PHP string or `foo(...)` syntax.
#[derive(Clone, Copy, PartialEq, Eq)]
enum StaticCallbackOperandKind {
    StringLiteral,
    FirstClassCallable,
}

/// Returns a static callback name from a string literal or `foo(...)` descriptor instruction.
fn static_callback_name_operand(
    ctx: &FunctionContext<'_>,
    value: ValueId,
    owner: &str,
) -> Result<StaticCallbackName> {
    let inst_ref = static_callback_source_instruction(ctx, value, owner)?;
    let receiver = inst_ref.operands.first().copied();
    let kind = match inst_ref.op {
        Op::ConstStr => StaticCallbackOperandKind::StringLiteral,
        Op::FirstClassCallableNew => StaticCallbackOperandKind::FirstClassCallable,
        _ => unreachable!("callback source instruction was validated earlier"),
    };
    let Some(Immediate::Data(data)) = inst_ref.immediate.as_ref() else {
        return Err(CodegenIrError::invalid_module(format!(
            "{} string literal has no data id",
            owner
        )));
    };
    let name = ctx
        .module
        .data
        .strings
        .get(data.as_raw() as usize)
        .cloned()
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))?;
    Ok(StaticCallbackName { name, kind, receiver })
}

/// Returns the literal callback-producing instruction for a callback operand.
fn static_callback_source_instruction<'a>(
    ctx: &'a FunctionContext<'_>,
    value: ValueId,
    owner: &str,
) -> Result<&'a Instruction> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { block, index, inst } = value_ref.def else {
        return Err(CodegenIrError::unsupported(format!(
            "{} with non-static callback operand",
            owner
        )));
    };
    let Some(inst_ref) = ctx.function.instruction(inst) else {
        return Err(CodegenIrError::missing_entry("instruction", inst.as_raw()));
    };
    if inst_ref.op == Op::LoadLocal {
        return static_callback_local_source_instruction(ctx, block, index, inst_ref, owner);
    }
    require_static_callback_source(inst_ref, owner)
}

/// Returns whether a callback operand can use the static callback binding path.
fn static_callback_operand_is_recoverable(ctx: &FunctionContext<'_>, value: ValueId) -> bool {
    static_callback_source_instruction(ctx, value, "static callback probe").is_ok()
}

/// Returns true when a callback operand is a dynamic callable local, such as a parameter.
fn descriptor_callback_local_without_same_block_store(
    ctx: &FunctionContext<'_>,
    value: ValueId,
) -> Result<bool> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { block, index, inst } = value_ref.def else {
        return Ok(false);
    };
    let Some(inst_ref) = ctx.function.instruction(inst) else {
        return Err(CodegenIrError::missing_entry("instruction", inst.as_raw()));
    };
    if inst_ref.op != Op::LoadLocal {
        return Ok(false);
    }
    let Some(Immediate::LocalSlot(slot)) = inst_ref.immediate else {
        return Err(CodegenIrError::invalid_module(
            "array_map callback load_local has no local slot",
        ));
    };
    if same_block_store_before(ctx, block, index, slot)? {
        return Ok(false);
    }
    Ok(ctx.local_php_type(slot)? == PhpType::Callable)
}

/// Returns true when the selected local slot is stored earlier in the same EIR block.
fn same_block_store_before(
    ctx: &FunctionContext<'_>,
    block: BlockId,
    load_index: u32,
    slot: LocalSlotId,
) -> Result<bool> {
    let block_ref = ctx
        .function
        .block(block)
        .ok_or_else(|| CodegenIrError::missing_entry("block", block.as_raw()))?;
    for (index, inst_id) in block_ref.instructions.iter().enumerate() {
        if index as u32 >= load_index {
            break;
        }
        let inst_ref = ctx
            .function
            .instruction(*inst_id)
            .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst_id.as_raw()))?;
        if inst_ref.op == Op::StoreLocal
            && matches!(inst_ref.immediate, Some(Immediate::LocalSlot(candidate)) if candidate == slot)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Resolves a local callback load to the last same-block store before that load.
fn static_callback_local_source_instruction<'a>(
    ctx: &'a FunctionContext<'_>,
    block: BlockId,
    load_index: u32,
    load_inst: &Instruction,
    owner: &str,
) -> Result<&'a Instruction> {
    let Some(Immediate::LocalSlot(slot)) = load_inst.immediate else {
        return Err(CodegenIrError::invalid_module(format!(
            "{} load_local callback has no local slot",
            owner
        )));
    };
    let block_ref = ctx
        .function
        .block(block)
        .ok_or_else(|| CodegenIrError::missing_entry("block", block.as_raw()))?;
    let mut stored = None;
    for (index, inst_id) in block_ref.instructions.iter().enumerate() {
        if index as u32 >= load_index {
            break;
        }
        let inst_ref = ctx
            .function
            .instruction(*inst_id)
            .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst_id.as_raw()))?;
        if inst_ref.op == Op::StoreLocal
            && matches!(inst_ref.immediate, Some(Immediate::LocalSlot(candidate)) if candidate == slot)
        {
            stored = inst_ref.operands.first().copied();
        }
    }
    let Some(stored) = stored else {
        return Err(CodegenIrError::unsupported(format!(
            "{} with local callback operand that has no prior same-block store",
            owner
        )));
    };
    let Some(value_ref) = ctx.function.value(stored) else {
        return Err(CodegenIrError::missing_entry("value", stored.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Err(CodegenIrError::unsupported(format!(
            "{} with local callback operand from non-instruction value",
            owner
        )));
    };
    let Some(inst_ref) = ctx.function.instruction(inst) else {
        return Err(CodegenIrError::missing_entry("instruction", inst.as_raw()));
    };
    require_static_callback_source(inst_ref, owner)
}

/// Verifies an instruction directly materializes a callback identity supported by the runtime.
fn require_static_callback_source<'a>(inst: &'a Instruction, owner: &str) -> Result<&'a Instruction> {
    if matches!(inst.op, Op::ConstStr | Op::FirstClassCallableNew) {
        Ok(inst)
    } else {
        Err(CodegenIrError::unsupported(format!(
            "{} with non-static callback operand",
            owner
        )))
    }
}

/// Resolved static-method callback metadata for a small runtime helper wrapper.
struct StaticMethodCallbackTarget {
    entry_label: String,
    called_class: StaticCallbackCalledClass,
    dynamic_slot: Option<usize>,
    env_source: Option<StaticCallbackEnvSource>,
    return_ty: PhpType,
}

/// Resolved instance-method callback metadata for sort runtime wrappers.
struct InstanceMethodCallbackTarget {
    entry_label: String,
    receiver: ValueId,
    return_ty: PhpType,
}

/// Source used by a callback wrapper to materialize the hidden called-class id.
enum StaticCallbackCalledClass {
    Immediate(u64),
    Env,
}

/// Source used by the sort call site to build the callback environment.
#[derive(Clone, Copy)]
enum StaticCallbackEnvSource {
    Local(LocalSlotId),
    ThisObject(LocalSlotId),
    Value(ValueId),
}

/// Resolves a sort static-method callback, allowing `static::` with an environment.
fn static_method_sort_callback_target(
    ctx: &FunctionContext<'_>,
    callback_name: &str,
    owner: &str,
    visible_arg_types: Option<&[PhpType]>,
) -> Result<Option<StaticMethodCallbackTarget>> {
    static_method_callback_target_inner(ctx, callback_name, owner, visible_arg_types, true)
}

/// Resolves static-method callback metadata and optionally supports late-static env dispatch.
fn static_method_callback_target_inner(
    ctx: &FunctionContext<'_>,
    callback_name: &str,
    owner: &str,
    visible_arg_types: Option<&[PhpType]>,
    allow_static_env: bool,
) -> Result<Option<StaticMethodCallbackTarget>> {
    let Some((receiver, method)) = callback_name.rsplit_once("::") else {
        return Ok(None);
    };
    let receiver = receiver.trim_start_matches('\\');
    if receiver == "static" && allow_static_env {
        return static_late_bound_method_callback_target(ctx, method, owner, visible_arg_types);
    }
    if matches!(receiver, "self" | "parent" | "static" | "object") {
        return Err(CodegenIrError::unsupported(format!(
            "{} with lexical or receiver-bound static method callback '{}'",
            owner,
            callback_name
        )));
    }
    let visible_arg_types = visible_arg_types.ok_or_else(|| {
        CodegenIrError::unsupported(format!(
            "{} '{}' with dynamic callback argument shape",
            owner,
            callback_name
        ))
    })?;
    require_static_method_callback_arg_types(owner, callback_name, visible_arg_types)?;
    let receiver_info = ctx
        .module
        .class_infos
        .get(receiver)
        .ok_or_else(|| CodegenIrError::unsupported(format!(
            "{} with unknown static method callback class '{}'",
            owner,
            receiver
        )))?;
    let method_key = php_symbol_key(method);
    let impl_class = receiver_info
        .static_method_impl_classes
        .get(&method_key)
        .map(String::as_str)
        .unwrap_or(receiver);
    let impl_info = ctx
        .module
        .class_infos
        .get(impl_class)
        .ok_or_else(|| CodegenIrError::unsupported(format!(
            "{} with unknown static method implementation class '{}'",
            owner,
            impl_class
        )))?;
    let sig = impl_info.static_methods.get(&method_key).ok_or_else(|| {
        CodegenIrError::unsupported(format!(
            "{} with unknown static method callback '{}'",
            owner,
            callback_name
        ))
    })?;
    if sig.params.len() != visible_arg_types.len() {
        return Err(CodegenIrError::unsupported(format!(
            "{} '{}' with {} visible args for {} params",
            owner,
            callback_name,
            visible_arg_types.len(),
            sig.params.len()
        )));
    }
    require_static_method_callback_param_types(owner, callback_name, sig, visible_arg_types)?;
    Ok(Some(StaticMethodCallbackTarget {
        entry_label: static_method_symbol(impl_class, &method_key),
        called_class: StaticCallbackCalledClass::Immediate(receiver_info.class_id),
        dynamic_slot: None,
        env_source: None,
        return_ty: sig.return_type.codegen_repr(),
    }))
}

/// Resolves a late-bound `static::method(...)` callback target for sort runtime wrappers.
fn static_late_bound_method_callback_target(
    ctx: &FunctionContext<'_>,
    method: &str,
    owner: &str,
    visible_arg_types: Option<&[PhpType]>,
) -> Result<Option<StaticMethodCallbackTarget>> {
    let receiver = current_callback_class(ctx)?;
    let callback_name = format!("static::{}", method);
    let visible_arg_types = visible_arg_types.ok_or_else(|| {
        CodegenIrError::unsupported(format!(
            "{} '{}' with dynamic callback argument shape",
            owner,
            callback_name
        ))
    })?;
    require_static_method_callback_arg_types(owner, &callback_name, visible_arg_types)?;
    let receiver_info = ctx
        .module
        .class_infos
        .get(receiver)
        .ok_or_else(|| CodegenIrError::unsupported(format!(
            "{} with unknown static callback receiver class '{}'",
            owner,
            receiver
        )))?;
    let method_key = php_symbol_key(method);
    let impl_class = receiver_info
        .static_method_impl_classes
        .get(&method_key)
        .map(String::as_str)
        .unwrap_or(receiver);
    let impl_info = ctx
        .module
        .class_infos
        .get(impl_class)
        .ok_or_else(|| CodegenIrError::unsupported(format!(
            "{} with unknown static method implementation class '{}'",
            owner,
            impl_class
        )))?;
    let sig = impl_info.static_methods.get(&method_key).ok_or_else(|| {
        CodegenIrError::unsupported(format!(
            "{} with unknown static method callback '{}'",
            owner,
            callback_name
        ))
    })?;
    if sig.params.len() != visible_arg_types.len() {
        return Err(CodegenIrError::unsupported(format!(
            "{} '{}' with {} visible args for {} params",
            owner,
            callback_name,
            visible_arg_types.len(),
            sig.params.len()
        )));
    }
    require_static_method_callback_param_types(owner, &callback_name, sig, visible_arg_types)?;
    Ok(Some(StaticMethodCallbackTarget {
        entry_label: static_method_symbol(impl_class, &method_key),
        called_class: StaticCallbackCalledClass::Env,
        dynamic_slot: receiver_info.static_vtable_slots.get(&method_key).copied(),
        env_source: Some(static_callback_env_source(ctx)?),
        return_ty: sig.return_type.codegen_repr(),
    }))
}

/// Returns the lexical class for the current EIR class method.
fn current_callback_class<'a>(ctx: &'a FunctionContext<'_>) -> Result<&'a str> {
    ctx.function
        .name
        .rsplit_once("::")
        .map(|(class_name, _)| class_name)
        .ok_or_else(|| CodegenIrError::unsupported(format!(
            "static callback outside class method {}",
            ctx.function.name
        )))
}

/// Returns the current called-class id source available to a late-static callback.
fn static_callback_env_source(ctx: &FunctionContext<'_>) -> Result<StaticCallbackEnvSource> {
    if let Some(slot) = ctx.local_slot_by_name("__elephc_called_class_id") {
        return Ok(StaticCallbackEnvSource::Local(slot));
    }
    if let Some(slot) = ctx.local_slot_by_name("this") {
        return Ok(StaticCallbackEnvSource::ThisObject(slot));
    }
    Err(CodegenIrError::unsupported(format!(
        "static callback without called-class context in {}",
        ctx.function.name
    )))
}

/// Resolves an `object::method(...)` callback target and its captured receiver for sort helpers.
fn instance_method_sort_callback_target(
    ctx: &FunctionContext<'_>,
    callback: &StaticCallbackName,
    owner: &str,
    visible_arg_types: Option<&[PhpType]>,
) -> Result<Option<InstanceMethodCallbackTarget>> {
    let Some((receiver_label, method)) = callback.name.rsplit_once("::") else {
        return Ok(None);
    };
    if receiver_label.trim_start_matches('\\') != "object" {
        return Ok(None);
    }
    let Some(receiver) = callback.receiver else {
        return Err(CodegenIrError::unsupported(format!(
            "{} '{}' without captured receiver operand",
            owner,
            callback.name
        )));
    };
    let visible_arg_types = visible_arg_types.ok_or_else(|| {
        CodegenIrError::unsupported(format!(
            "{} '{}' with dynamic callback argument shape",
            owner,
            callback.name
        ))
    })?;
    require_static_method_callback_arg_types(owner, &callback.name, visible_arg_types)?;
    let receiver_ty = ctx.value_php_type(receiver)?.codegen_repr();
    let PhpType::Object(class_name) = receiver_ty else {
        return Err(CodegenIrError::unsupported(format!(
            "{} '{}' with receiver PHP type {:?}",
            owner,
            callback.name,
            receiver_ty
        )));
    };
    let normalized = class_name.trim_start_matches('\\');
    let class_info = ctx
        .module
        .class_infos
        .get(normalized)
        .ok_or_else(|| CodegenIrError::unsupported(format!(
            "{} with unknown instance callback class '{}'",
            owner,
            normalized
        )))?;
    let method_key = php_symbol_key(method);
    let sig = class_info.methods.get(&method_key).ok_or_else(|| {
        CodegenIrError::unsupported(format!(
            "{} with unknown instance method callback '{}'",
            owner,
            callback.name
        ))
    })?;
    if sig.params.len() != visible_arg_types.len() {
        return Err(CodegenIrError::unsupported(format!(
            "{} '{}' with {} visible args for {} params",
            owner,
            callback.name,
            visible_arg_types.len(),
            sig.params.len()
        )));
    }
    require_static_method_callback_param_types(owner, &callback.name, sig, visible_arg_types)?;
    let impl_class = class_info
        .method_impl_classes
        .get(&method_key)
        .map(String::as_str)
        .unwrap_or(normalized);
    if !instance_method_already_emitted(ctx, impl_class, &method_key) {
        return Err(CodegenIrError::unsupported(format!(
            "{} '{}' without emitted EIR method body",
            owner,
            callback.name
        )));
    }
    Ok(Some(InstanceMethodCallbackTarget {
        entry_label: method_symbol(impl_class, &method_key),
        receiver,
        return_ty: sig.return_type.codegen_repr(),
    }))
}

/// Returns true when the instance callback target has a generated EIR method body.
fn instance_method_already_emitted(ctx: &FunctionContext<'_>, class_name: &str, method_key: &str) -> bool {
    ctx.module.class_methods.iter().any(|function| {
        !function.flags.is_static
            && function
                .name
                .rsplit_once("::")
                .is_some_and(|(class, method)| class == class_name && php_symbol_key(method) == method_key)
    })
}

/// Verifies the wrapper can forward the callback argument ABI without boxing or shuffling pairs.
fn require_static_method_callback_arg_types(
    owner: &str,
    callback_name: &str,
    visible_arg_types: &[PhpType],
) -> Result<()> {
    if !(1..=2).contains(&visible_arg_types.len()) {
        return Err(CodegenIrError::unsupported(format!(
            "{} '{}' with {} visible callback args",
            owner,
            callback_name,
            visible_arg_types.len()
        )));
    }
    if visible_arg_types
        .iter()
        .any(|ty| matches!(ty.codegen_repr(), PhpType::Str))
        && !(visible_arg_types.len() == 1
            && matches!(visible_arg_types[0].codegen_repr(), PhpType::Str))
    {
        return Err(CodegenIrError::unsupported(format!(
            "{} '{}' with string callback args outside the one-argument ABI",
            owner,
            callback_name
        )));
    }
    for ty in visible_arg_types {
        if !matches!(
            ty.codegen_repr(),
            PhpType::Int | PhpType::Bool | PhpType::Str | PhpType::Void | PhpType::Never
        ) {
            return Err(CodegenIrError::unsupported(format!(
                "{} '{}' with unsupported callback arg type {:?}",
                owner,
                callback_name,
                ty.codegen_repr()
            )));
        }
    }
    Ok(())
}

/// Verifies the target static method can consume the wrapper's unboxed integer ABI values.
fn require_static_method_callback_param_types(
    owner: &str,
    callback_name: &str,
    sig: &crate::types::FunctionSig,
    visible_arg_types: &[PhpType],
) -> Result<()> {
    for ((_, param_ty), visible_ty) in sig.params.iter().zip(visible_arg_types.iter()) {
        let param_ty = param_ty.codegen_repr();
        let visible_ty = visible_ty.codegen_repr();
        if matches!(visible_ty, PhpType::Void | PhpType::Never) {
            continue;
        }
        if matches!((&param_ty, &visible_ty), (PhpType::Int | PhpType::Bool, PhpType::Int | PhpType::Bool)) {
            continue;
        }
        if matches!((&param_ty, &visible_ty), (PhpType::Str, PhpType::Str)) {
            continue;
        }
        return Err(CodegenIrError::unsupported(format!(
            "{} '{}' with callback param type {:?} for runtime arg type {:?}",
            owner,
            callback_name,
            param_ty,
            visible_ty
        )));
    }
    Ok(())
}

/// Counts integer ABI registers consumed by the visible callback argument list.
fn callback_arg_abi_slots(visible_arg_types: &[PhpType]) -> usize {
    visible_arg_types
        .iter()
        .map(|ty| {
            if matches!(ty.codegen_repr(), PhpType::Str) {
                2
            } else {
                1
            }
        })
        .sum()
}

/// Shifts AArch64 callback arguments right by one slot for a hidden receiver/class id.
fn shift_callback_args_after_hidden_aarch64(
    ctx: &mut FunctionContext<'_>,
    visible_arg_types: &[PhpType],
) {
    match visible_arg_types {
        [ty] if matches!(ty.codegen_repr(), PhpType::Str) => {
            ctx.emitter.instruction("mov x2, x1");                              // shift the callback string length after the hidden receiver/class id
            ctx.emitter.instruction("mov x1, x0");                              // shift the callback string pointer after the hidden receiver/class id
        }
        [_] => {
            ctx.emitter.instruction("mov x1, x0");                              // shift the scalar callback argument after the hidden receiver/class id
        }
        [_, _] => {
            ctx.emitter.instruction("mov x2, x1");                              // shift the second scalar callback argument after the hidden receiver/class id
            ctx.emitter.instruction("mov x1, x0");                              // shift the first scalar callback argument after the hidden receiver/class id
        }
        _ => {}
    }
}

/// Shifts x86_64 callback arguments right by one slot for a hidden receiver/class id.
fn shift_callback_args_after_hidden_x86_64(
    ctx: &mut FunctionContext<'_>,
    visible_arg_types: &[PhpType],
) {
    match visible_arg_types {
        [ty] if matches!(ty.codegen_repr(), PhpType::Str) => {
            ctx.emitter.instruction("mov rdx, rsi");                            // shift the callback string length after the hidden receiver/class id
            ctx.emitter.instruction("mov rsi, rdi");                            // shift the callback string pointer after the hidden receiver/class id
        }
        [_] => {
            ctx.emitter.instruction("mov rsi, rdi");                            // shift the scalar callback argument after the hidden receiver/class id
        }
        [_, _] => {
            ctx.emitter.instruction("mov rdx, rsi");                            // shift the second scalar callback argument after the hidden receiver/class id
            ctx.emitter.instruction("mov rsi, rdi");                            // shift the first scalar callback argument after the hidden receiver/class id
        }
        _ => {}
    }
}

/// Emits a local wrapper that prepends the hidden static called-class id.
fn emit_static_method_callback_wrapper(
    ctx: &mut FunctionContext<'_>,
    target: &StaticMethodCallbackTarget,
    visible_arg_types: &[PhpType],
) -> String {
    let wrapper_label = ctx.next_label("static_method_callback_wrapper");
    let done_label = ctx.next_label("static_method_callback_after_wrapper");
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&wrapper_label);
    match ctx.emitter.target.arch {
        Arch::AArch64 => emit_static_method_callback_wrapper_aarch64(ctx, target, visible_arg_types),
        Arch::X86_64 => emit_static_method_callback_wrapper_x86_64(ctx, target, visible_arg_types),
    }
    ctx.emitter.label(&done_label);
    wrapper_label
}

/// Emits the AArch64 static-method callback ABI adapter.
fn emit_static_method_callback_wrapper_aarch64(
    ctx: &mut FunctionContext<'_>,
    target: &StaticMethodCallbackTarget,
    visible_arg_types: &[PhpType],
) {
    let env_reg = abi::int_arg_reg_name(ctx.emitter.target, callback_arg_abi_slots(visible_arg_types));
    ctx.emitter.instruction("sub sp, sp, #16");                                 // reserve wrapper spill space for the runtime callback return address
    ctx.emitter.instruction("str x30, [sp, #8]");                               // preserve the runtime helper return address across the static method call
    match target.called_class {
        StaticCallbackCalledClass::Immediate(class_id) => {
            abi::emit_load_int_immediate(ctx.emitter, "x3", class_id as i64);
        }
        StaticCallbackCalledClass::Env => {
            ctx.emitter.instruction(&format!("ldr x3, [{}]", env_reg));         // load the late-static called-class id from the callback environment
        }
    }
    shift_callback_args_after_hidden_aarch64(ctx, visible_arg_types);
    ctx.emitter.instruction("mov x0, x3");                                      // pass the called-class id as the hidden static method argument
    emit_static_callback_dispatch(ctx, target);
    ctx.emitter.instruction("ldr x30, [sp, #8]");                               // restore the runtime helper return address after the static method call
    ctx.emitter.instruction("add sp, sp, #16");                                 // release the wrapper spill space before returning to the runtime helper
    ctx.emitter.instruction("ret");                                             // return the static method result to the runtime callback helper
}

/// Emits the x86_64 static-method callback ABI adapter.
fn emit_static_method_callback_wrapper_x86_64(
    ctx: &mut FunctionContext<'_>,
    target: &StaticMethodCallbackTarget,
    visible_arg_types: &[PhpType],
) {
    let env_reg = abi::int_arg_reg_name(ctx.emitter.target, callback_arg_abi_slots(visible_arg_types));
    ctx.emitter.instruction("push rbp");                                        // preserve the runtime helper frame pointer for the nested static method call
    ctx.emitter.instruction("mov rbp, rsp");                                    // establish a wrapper frame while shifting callback arguments
    match target.called_class {
        StaticCallbackCalledClass::Immediate(class_id) => {
            abi::emit_load_int_immediate(ctx.emitter, "rcx", class_id as i64);
        }
        StaticCallbackCalledClass::Env => {
            ctx.emitter.instruction(&format!("mov rcx, QWORD PTR [{}]", env_reg)); // load the late-static called-class id from the callback environment
        }
    }
    shift_callback_args_after_hidden_x86_64(ctx, visible_arg_types);
    ctx.emitter.instruction("mov rdi, rcx");                                    // pass the called-class id as the hidden static method argument
    emit_static_callback_dispatch(ctx, target);
    ctx.emitter.instruction("pop rbp");                                         // restore the runtime helper frame pointer before returning
    ctx.emitter.instruction("ret");                                             // return the static method result to the runtime callback helper
}

/// Emits a local wrapper that prepends the captured object receiver.
fn emit_instance_method_callback_wrapper(
    ctx: &mut FunctionContext<'_>,
    target: &InstanceMethodCallbackTarget,
    visible_arg_types: &[PhpType],
) -> String {
    let wrapper_label = ctx.next_label("instance_method_callback_wrapper");
    let done_label = ctx.next_label("instance_method_callback_after_wrapper");
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&wrapper_label);
    match ctx.emitter.target.arch {
        Arch::AArch64 => emit_instance_method_callback_wrapper_aarch64(ctx, target, visible_arg_types),
        Arch::X86_64 => emit_instance_method_callback_wrapper_x86_64(ctx, target, visible_arg_types),
    }
    ctx.emitter.label(&done_label);
    wrapper_label
}

/// Emits the AArch64 instance-method callback ABI adapter.
fn emit_instance_method_callback_wrapper_aarch64(
    ctx: &mut FunctionContext<'_>,
    target: &InstanceMethodCallbackTarget,
    visible_arg_types: &[PhpType],
) {
    let env_reg = abi::int_arg_reg_name(ctx.emitter.target, callback_arg_abi_slots(visible_arg_types));
    ctx.emitter.instruction("sub sp, sp, #16");                                 // reserve wrapper spill space for the runtime callback return address
    ctx.emitter.instruction("str x30, [sp, #8]");                               // preserve the runtime helper return address across the instance method call
    ctx.emitter.instruction(&format!("ldr x3, [{}]", env_reg));                 // load the captured object receiver from the callback environment
    shift_callback_args_after_hidden_aarch64(ctx, visible_arg_types);
    ctx.emitter.instruction("mov x0, x3");                                      // pass the captured object receiver as the method receiver
    abi::emit_call_label(ctx.emitter, &target.entry_label);
    ctx.emitter.instruction("ldr x30, [sp, #8]");                               // restore the runtime helper return address after the instance method call
    ctx.emitter.instruction("add sp, sp, #16");                                 // release the wrapper spill space before returning to the runtime helper
    ctx.emitter.instruction("ret");                                             // return the instance method result to the runtime callback helper
}

/// Emits the x86_64 instance-method callback ABI adapter.
fn emit_instance_method_callback_wrapper_x86_64(
    ctx: &mut FunctionContext<'_>,
    target: &InstanceMethodCallbackTarget,
    visible_arg_types: &[PhpType],
) {
    let env_reg = abi::int_arg_reg_name(ctx.emitter.target, callback_arg_abi_slots(visible_arg_types));
    ctx.emitter.instruction("push rbp");                                        // preserve the runtime helper frame pointer for the nested instance method call
    ctx.emitter.instruction("mov rbp, rsp");                                    // establish a wrapper frame while shifting callback arguments
    ctx.emitter.instruction(&format!("mov rcx, QWORD PTR [{}]", env_reg));      // load the captured object receiver from the callback environment
    shift_callback_args_after_hidden_x86_64(ctx, visible_arg_types);
    ctx.emitter.instruction("mov rdi, rcx");                                    // pass the captured object receiver as the method receiver
    abi::emit_call_label(ctx.emitter, &target.entry_label);
    ctx.emitter.instruction("pop rbp");                                         // restore the runtime helper frame pointer before returning
    ctx.emitter.instruction("ret");                                             // return the instance method result to the runtime callback helper
}

/// Emits either a direct static-method callback call or a late-static vtable call.
fn emit_static_callback_dispatch(ctx: &mut FunctionContext<'_>, target: &StaticMethodCallbackTarget) {
    if let Some(slot) = target.dynamic_slot {
        emit_static_callback_dynamic_call(ctx, slot);
    } else {
        abi::emit_call_label(ctx.emitter, &target.entry_label);
    }
}

/// Emits an indirect static-vtable callback call for a late-bound `static::method()` wrapper.
fn emit_static_callback_dynamic_call(ctx: &mut FunctionContext<'_>, slot: usize) {
    let hidden_called_class_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let class_id_scratch = abi::temp_int_reg(ctx.emitter.target);
    let dispatch_scratch = abi::symbol_scratch_reg(ctx.emitter);
    ctx.emitter.instruction(&format!("mov {}, {}", class_id_scratch, hidden_called_class_reg)); // preserve the forwarded called-class id across static-vtable address materialization
    abi::emit_symbol_address(ctx.emitter, dispatch_scratch, "_class_static_vtable_ptrs");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("ldr {}, [{}, {}, lsl #3]", dispatch_scratch, dispatch_scratch, class_id_scratch)); // load the class-specific static-vtable pointer from the global table
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov {}, QWORD PTR [{} + {} * 8]", dispatch_scratch, dispatch_scratch, class_id_scratch)); // load the class-specific static-vtable pointer from the global table
        }
    }
    abi::emit_load_from_address(ctx.emitter, dispatch_scratch, dispatch_scratch, slot * 8);
    abi::emit_call_reg(ctx.emitter, dispatch_scratch);
}

/// Reserves and fills the optional callback environment consumed by sort runtime helpers.
fn reserve_static_callback_env(
    ctx: &mut FunctionContext<'_>,
    source: Option<StaticCallbackEnvSource>,
) -> Result<usize> {
    let Some(source) = source else {
        return Ok(0);
    };
    abi::emit_reserve_temporary_stack(ctx.emitter, 16);
    match source {
        StaticCallbackEnvSource::Local(slot) => {
            let source_ty = ctx.load_local_to_result(slot)?;
            if source_ty != PhpType::Int {
                return Err(CodegenIrError::invalid_module(format!(
                    "hidden called-class id local has PHP type {:?}",
                    source_ty
                )));
            }
        }
        StaticCallbackEnvSource::ThisObject(slot) => {
            let source_ty = ctx.load_local_to_result(slot)?;
            if !matches!(source_ty.codegen_repr(), PhpType::Object(_)) {
                return Err(CodegenIrError::invalid_module(format!(
                    "this local has PHP type {:?} for forwarded called-class id",
                    source_ty
                )));
            }
            abi::emit_load_from_address(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                abi::int_result_reg(ctx.emitter),
                0,
            );
        }
        StaticCallbackEnvSource::Value(value) => {
            let source_ty = ctx.load_value_to_result(value)?;
            if !matches!(source_ty.codegen_repr(), PhpType::Object(_)) {
                return Err(CodegenIrError::invalid_module(format!(
                    "callback environment value has PHP type {:?}",
                    source_ty
                )));
            }
        }
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("str x0, [sp]");                            // store the callback environment payload for the runtime helper
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                // store the callback environment payload for the runtime helper
        }
    }
    Ok(16)
}

/// Loads the optional callback environment argument expected by sort runtime helpers.
fn load_static_callback_env_arg(ctx: &mut FunctionContext<'_>, env_reg: &str, env_bytes: usize) {
    if env_bytes == 0 {
        abi::emit_load_int_immediate(ctx.emitter, env_reg, 0);
    } else {
        abi::emit_temporary_stack_address(ctx.emitter, env_reg, 0);
    }
}

/// Returns the element type accepted by indexed-array value set-operation helpers.
fn set_op_indexed_array_element_type(ty: PhpType, name: &str) -> Result<PhpType> {
    match ty.codegen_repr() {
        PhpType::Array(elem) => {
            let elem = elem.codegen_repr();
            if matches!(
                elem,
                PhpType::Int
                    | PhpType::Bool
                    | PhpType::Float
                    | PhpType::Callable
                    | PhpType::Void
                    | PhpType::Never
            ) || elem.is_refcounted()
            {
                return Ok(elem);
            }
            Err(CodegenIrError::unsupported(format!(
                "{} indexed-array element PHP type {:?}",
                name,
                elem
            )))
        }
        other => Err(CodegenIrError::unsupported(format!("{} for PHP type {:?}", name, other))),
    }
}

/// Verifies two set-operation operands can share one raw slot comparison helper.
fn require_set_op_compatible_element_types(
    name: &str,
    first: &PhpType,
    second: &PhpType,
) -> Result<()> {
    if first == second
        || matches!(first, PhpType::Never | PhpType::Void)
        || matches!(second, PhpType::Never | PhpType::Void)
    {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} for incompatible indexed-array element PHP types {:?} and {:?}",
        name,
        first,
        second
    )))
}

/// Verifies the EIR result preserves the first operand element metadata.
fn require_set_op_result_type(name: &str, first_elem_ty: &PhpType, result_ty: &PhpType) -> Result<()> {
    match result_ty {
        PhpType::Array(elem) if elem.codegen_repr() == first_elem_ty.codegen_repr() => Ok(()),
        other => Err(CodegenIrError::unsupported(format!(
            "{} result PHP type {:?} for first element PHP type {:?}",
            name,
            other,
            first_elem_ty
        ))),
    }
}

/// Returns the hash operand type accepted by key set-operation helpers.
fn assoc_array_key_set_operand_type(ty: PhpType, name: &str, position: &str) -> Result<PhpType> {
    match ty.codegen_repr() {
        PhpType::AssocArray { key, value } => Ok(PhpType::AssocArray { key, value }),
        other => Err(CodegenIrError::unsupported(format!(
            "{} {} argument PHP type {:?}",
            name, position, other
        ))),
    }
}

/// Verifies a key set-operation result preserves the first operand's hash metadata.
fn require_assoc_array_key_set_result_type(
    name: &str,
    first_ty: &PhpType,
    result_ty: &PhpType,
) -> Result<()> {
    if result_ty == first_ty {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} result PHP type {:?} for first argument PHP type {:?}",
        name, result_ty, first_ty
    )))
}

/// Verifies that a `range()` endpoint can be passed to the integer runtime helper.
///
/// `Mixed`/`Union` endpoints are accepted here and unboxed to a plain integer by `lower_range`
/// (via `resolve_int_operand_to_result`); the `__rt_range` helper only consumes integer endpoints.
fn require_range_endpoint(ty: PhpType, name: &str) -> Result<()> {
    match ty.codegen_repr() {
        PhpType::Int | PhpType::Bool | PhpType::Mixed | PhpType::Union(_) => Ok(()),
        other => Err(CodegenIrError::unsupported(format!(
            "range {} PHP type {:?}",
            name,
            other
        ))),
    }
}

/// Verifies `range()` is represented as an indexed integer array.
fn require_range_result_type(result_ty: &PhpType) -> Result<()> {
    match result_ty {
        PhpType::Array(elem) if elem.codegen_repr() == PhpType::Int => Ok(()),
        other => Err(CodegenIrError::unsupported(format!(
            "range result PHP type {:?}",
            other
        ))),
    }
}

/// Returns the shared element type for two compatible 8-byte indexed arrays.
fn compatible_eight_byte_indexed_array_element_type(
    first: PhpType,
    second: PhpType,
    name: &str,
) -> Result<PhpType> {
    let first = eight_byte_indexed_array_element_type(first, name)?;
    let second = eight_byte_indexed_array_element_type(second, name)?;
    if first == second
        || matches!(first, PhpType::Never | PhpType::Void)
        || matches!(second, PhpType::Never | PhpType::Void)
    {
        if matches!(first, PhpType::Never | PhpType::Void) {
            return Ok(second);
        }
        return Ok(first);
    }
    Err(CodegenIrError::unsupported(format!(
        "{} for incompatible indexed-array element PHP types {:?} and {:?}",
        name,
        first,
        second
    )))
}

/// Verifies that a builtin call has a lowered operand count within an inclusive range.
fn ensure_arg_count_between(inst: &Instruction, name: &str, min: usize, max: usize) -> Result<()> {
    let actual = inst.operands.len();
    if (min..=max).contains(&actual) {
        return Ok(());
    }
    Err(CodegenIrError::invalid_module(format!(
        "{} expected {}..={} args, got {}",
        name, min, max, actual
    )))
}

/// Verifies that the indexed `array_fill()` helper can store the fill value.
///
/// `Str` is accepted here and routed to `__rt_array_fill_str`, which materializes 16-byte
/// (pointer + length) string slots; the single-word scalar/refcounted helpers cannot carry a
/// string payload.
fn require_array_fill_indexed_value_type(value_ty: &PhpType) -> Result<()> {
    if matches!(
        value_ty,
        PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Str
            | PhpType::Void
            | PhpType::Mixed
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Object(_)
    ) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "array_fill indexed value PHP type {:?}",
        value_ty
    )))
}

/// Verifies that the assoc `array_fill()` helper can box the fill value.
fn require_array_fill_assoc_value_type(value_ty: &PhpType) -> Result<()> {
    if matches!(
        value_ty,
        PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Str
            | PhpType::Void
            | PhpType::Mixed
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Object(_)
    ) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "array_fill assoc value PHP type {:?}",
        value_ty
    )))
}

/// Returns the key element type accepted by `array_fill_keys()`.
fn array_fill_keys_key_element_type(ty: PhpType) -> Result<PhpType> {
    match ty.codegen_repr() {
        PhpType::Array(elem) => Ok(elem.codegen_repr()),
        other => Err(CodegenIrError::unsupported(format!(
            "array_fill_keys keys PHP type {:?}",
            other
        ))),
    }
}

/// Returns the key element type accepted by `array_combine()`.
fn array_combine_key_element_type(ty: PhpType) -> Result<PhpType> {
    match ty.codegen_repr() {
        PhpType::Array(elem) => Ok(elem.codegen_repr()),
        other => Err(CodegenIrError::unsupported(format!(
            "array_combine keys PHP type {:?}",
            other
        ))),
    }
}

/// Returns the value element type accepted by `array_combine()`.
fn array_combine_value_element_type(ty: PhpType) -> Result<PhpType> {
    match ty.codegen_repr() {
        PhpType::Array(elem) => Ok(elem.codegen_repr()),
        other => Err(CodegenIrError::unsupported(format!(
            "array_combine values PHP type {:?}",
            other
        ))),
    }
}

/// Verifies the key array uses the string-slot layout expected by the runtime helper.
fn require_array_fill_keys_key_layout(key_elem_ty: &PhpType) -> Result<()> {
    if matches!(key_elem_ty, PhpType::Str | PhpType::Void | PhpType::Never) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "array_fill_keys key element PHP type {:?}",
        key_elem_ty
    )))
}

/// Verifies the fill payload can be passed through the current runtime helper ABI.
///
/// String values are deliberately excluded because the helper accepts only one value word;
/// preserving string payloads requires a value_hi register/slot path.
fn require_array_fill_keys_value_type(value_ty: &PhpType) -> Result<()> {
    if matches!(
        value_ty,
        PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Void
            | PhpType::Mixed
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Object(_)
    ) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "array_fill_keys value PHP type {:?}",
        value_ty
    )))
}

/// Verifies the key array uses the string-slot layout expected by the runtime helper.
fn require_array_combine_key_layout(key_elem_ty: &PhpType) -> Result<()> {
    if matches!(key_elem_ty, PhpType::Str | PhpType::Void | PhpType::Never) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "array_combine key element PHP type {:?}",
        key_elem_ty
    )))
}

/// Verifies the values array uses a slot layout the runtime helper can copy.
///
/// String values are deliberately excluded because indexed string arrays store 16-byte
/// inline slots, while the existing `array_combine` runtime helper reads 8-byte value slots.
fn require_array_combine_value_layout(value_elem_ty: &PhpType) -> Result<()> {
    if matches!(
        value_elem_ty,
        PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Callable
            | PhpType::Void
            | PhpType::Never
    ) || value_elem_ty.is_refcounted()
    {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "array_combine value element PHP type {:?}",
        value_elem_ty
    )))
}

/// Verifies `array_fill_keys()` produces a hash matching the selected key/value metadata.
fn require_array_fill_keys_result_type(
    key_elem_ty: &PhpType,
    value_ty: &PhpType,
    result_ty: &PhpType,
) -> Result<()> {
    let expected_key_ty = array_key_type_from_value_type(key_elem_ty.clone()).codegen_repr();
    match result_ty {
        PhpType::AssocArray { key, value }
            if key.codegen_repr() == expected_key_ty && value.codegen_repr() == *value_ty =>
        {
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "array_fill_keys result PHP type {:?} for key element PHP type {:?} and value PHP type {:?}",
            other,
            key_elem_ty,
            value_ty
        ))),
    }
}

/// Verifies `array_combine()` produces a hash with the selected value element metadata.
fn require_array_combine_result_type(value_elem_ty: &PhpType, result_ty: &PhpType) -> Result<()> {
    match result_ty {
        PhpType::AssocArray { value, .. } if value.codegen_repr() == *value_elem_ty => Ok(()),
        other => Err(CodegenIrError::unsupported(format!(
            "array_combine result PHP type {:?} for value element PHP type {:?}",
            other,
            value_elem_ty
        ))),
    }
}

/// Verifies `array_flip()` produces a hash with normalized keys and integer source indexes.
fn require_array_flip_result_type(value_elem_ty: &PhpType, result_ty: &PhpType) -> Result<()> {
    let expected_key_ty = array_key_type_from_value_type(value_elem_ty.clone()).codegen_repr();
    match result_ty {
        PhpType::AssocArray { key, value }
            if key.codegen_repr() == expected_key_ty && value.codegen_repr() == PhpType::Int =>
        {
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "array_flip result PHP type {:?} for value element PHP type {:?}",
            other,
            value_elem_ty
        ))),
    }
}

/// Verifies the destination element type matches the fill layout or is a Mixed widening.
fn require_array_fill_result_type(value_ty: &PhpType, result_elem_ty: &PhpType) -> Result<()> {
    if value_ty == result_elem_ty || result_elem_ty == &PhpType::Mixed {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "array_fill result element PHP type {:?} for value PHP type {:?}",
        result_elem_ty,
        value_ty
    )))
}

/// Returns true when `array_fill()` is expected to build a keyed hash result.
fn array_fill_result_is_assoc(result_ty: &PhpType) -> bool {
    matches!(result_ty.codegen_repr(), PhpType::AssocArray { .. })
}

/// Verifies the assoc `array_fill()` result shape expected by the runtime helper.
fn require_array_fill_assoc_result_type(result_ty: &PhpType) -> Result<()> {
    match result_ty.codegen_repr() {
        PhpType::AssocArray { key, value }
            if key.codegen_repr() == PhpType::Int && value.codegen_repr() == PhpType::Mixed =>
        {
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "array_fill assoc result PHP type {:?}",
            other
        ))),
    }
}

/// Calls the legacy runtime helper after materializing `array_fill()` arguments.
///
/// String fills use the `(count, ptr, len)` ABI of `__rt_array_fill_str` (the helper is always
/// 0-indexed, so `start` is unused); every other value type uses the shared `(start, count, value)`
/// scalar/refcounted ABI. The register loads are independent stack reads, so loading `count` before
/// the string pointer/length cannot clobber it.
fn lower_array_fill_call(
    ctx: &mut FunctionContext<'_>,
    start: ValueId,
    count: ValueId,
    value: ValueId,
    value_ty: &PhpType,
) -> Result<()> {
    if matches!(value_ty.codegen_repr(), PhpType::Str) {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.load_value_to_reg(count, "x0")?;
                ctx.load_string_value_to_regs(value, "x1", "x2")?;
            }
            Arch::X86_64 => {
                ctx.load_value_to_reg(count, "rdi")?;
                ctx.load_string_value_to_regs(value, "rsi", "rdx")?;
            }
        }
        abi::emit_call_label(ctx.emitter, array_fill_runtime_helper(value_ty));
        return Ok(());
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(start, "x0")?;
            ctx.load_value_to_reg(count, "x1")?;
            ctx.load_value_to_reg(value, "x2")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(start, "rdi")?;
            ctx.load_value_to_reg(count, "rsi")?;
            ctx.load_value_to_reg(value, "rdx")?;
        }
    }
    abi::emit_call_label(ctx.emitter, array_fill_runtime_helper(value_ty));
    Ok(())
}

/// Calls the keyed `array_fill()` runtime helper after materializing the boxed payload fields.
fn lower_array_fill_assoc_call(
    ctx: &mut FunctionContext<'_>,
    start: ValueId,
    count: ValueId,
    value: ValueId,
    value_ty: &PhpType,
) -> Result<()> {
    let value_tag = runtime_value_tag("array_fill", value_ty)? as i64;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(start, "x0")?;
            ctx.load_value_to_reg(count, "x1")?;
            materialize_array_fill_assoc_value_words(ctx, value, value_ty, "x2", "x3")?;
            abi::emit_load_int_immediate(ctx.emitter, "x4", value_tag);
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(start, "rdi")?;
            ctx.load_value_to_reg(count, "rsi")?;
            materialize_array_fill_assoc_value_words(ctx, value, value_ty, "rdx", "rcx")?;
            abi::emit_load_int_immediate(ctx.emitter, "r8", value_tag);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_fill_assoc");
    Ok(())
}

/// Materializes a fill payload as the low/high words consumed by `__rt_array_fill_assoc`.
fn materialize_array_fill_assoc_value_words(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    value_ty: &PhpType,
    lo_reg: &str,
    hi_reg: &str,
) -> Result<()> {
    match value_ty.codegen_repr() {
        PhpType::Str => ctx.load_string_value_to_regs(value, lo_reg, hi_reg),
        PhpType::Float => {
            ctx.load_value_to_result(value)?;
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction(&format!("fmov {}, d0", lo_reg));   // pass the floating-point fill bits as the assoc-fill value low word
                    ctx.emitter.instruction(&format!("mov {}, #0", hi_reg));    // clear the unused assoc-fill value high word
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction(&format!("movq {}, xmm0", lo_reg)); // pass the floating-point fill bits as the assoc-fill value low word
                    ctx.emitter.instruction(&format!("xor {}, {}", hi_reg, hi_reg)); // clear the unused assoc-fill value high word
                }
            }
            Ok(())
        }
        _ => {
            ctx.load_value_to_reg(value, lo_reg)?;
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction(&format!("mov {}, #0", hi_reg));    // clear the unused assoc-fill value high word
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction(&format!("xor {}, {}", hi_reg, hi_reg)); // clear the unused assoc-fill value high word
                }
            }
            Ok(())
        }
    }
}

/// Calls the legacy runtime helper after materializing `array_fill_keys()` arguments.
fn lower_array_fill_keys_call(
    ctx: &mut FunctionContext<'_>,
    keys: ValueId,
    value: ValueId,
    value_ty: &PhpType,
) -> Result<()> {
    let value_tag = runtime_value_tag("array_fill_keys", value_ty)? as i64;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(keys, "x0")?;
            ctx.load_value_to_reg(value, "x1")?;
            abi::emit_load_int_immediate(ctx.emitter, "x2", value_tag);
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(keys, "rdi")?;
            ctx.load_value_to_reg(value, "rsi")?;
            abi::emit_load_int_immediate(ctx.emitter, "rdx", value_tag);
        }
    }
    abi::emit_call_label(ctx.emitter, array_fill_keys_runtime_helper(value_ty));
    Ok(())
}

/// Calls the legacy runtime helper after materializing `array_combine()` arguments.
fn lower_array_combine_call(
    ctx: &mut FunctionContext<'_>,
    keys: ValueId,
    values: ValueId,
    value_elem_ty: &PhpType,
) -> Result<()> {
    let value_tag = runtime_value_tag("array_combine", value_elem_ty)? as i64;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(keys, "x0")?;
            ctx.load_value_to_reg(values, "x1")?;
            abi::emit_load_int_immediate(ctx.emitter, "x2", value_tag);
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(keys, "rdi")?;
            ctx.load_value_to_reg(values, "rsi")?;
            abi::emit_load_int_immediate(ctx.emitter, "rdx", value_tag);
        }
    }
    abi::emit_call_label(ctx.emitter, array_combine_runtime_helper(value_elem_ty));
    Ok(())
}

/// Returns the helper matching the fill-keys value ownership representation.
fn array_fill_keys_runtime_helper(value_ty: &PhpType) -> &'static str {
    if value_ty.is_refcounted() {
        "__rt_array_fill_keys_refcounted"
    } else {
        "__rt_array_fill_keys"
    }
}

/// Returns the helper matching the fill value's ownership representation.
///
/// `Str` routes to the dedicated `__rt_array_fill_str`, which takes a `(count, ptr, len)` ABI and
/// builds 16-byte string slots; it must be checked before the generic refcounted helper, whose ABI
/// only carries a single heap-pointer value word.
fn array_fill_runtime_helper(value_ty: &PhpType) -> &'static str {
    if matches!(value_ty.codegen_repr(), PhpType::Str) {
        "__rt_array_fill_str"
    } else if value_ty.is_refcounted() {
        "__rt_array_fill_refcounted"
    } else {
        "__rt_array_fill"
    }
}

/// Returns the helper matching the combined value element ownership representation.
fn array_combine_runtime_helper(value_elem_ty: &PhpType) -> &'static str {
    if value_elem_ty.is_refcounted() {
        "__rt_array_combine_refcounted"
    } else {
        "__rt_array_combine"
    }
}

/// Returns the helper matching the flipped source value slot layout.
fn array_flip_runtime_helper(value_elem_ty: &PhpType) -> &'static str {
    if value_elem_ty == &PhpType::Str {
        "__rt_array_flip_string"
    } else {
        "__rt_array_flip"
    }
}

/// Returns the element type for indexed arrays supported by 8-byte helper slots.
fn eight_byte_indexed_array_element_type(ty: PhpType, name: &str) -> Result<PhpType> {
    match ty.codegen_repr() {
        PhpType::Array(elem) => {
            let elem = elem.codegen_repr();
            if matches!(
                elem,
                PhpType::Int
                    | PhpType::Bool
                    | PhpType::Float
                    | PhpType::Callable
                    | PhpType::Void
                    | PhpType::Never
            ) || elem.is_refcounted()
            {
                return Ok(elem);
            }
            Err(CodegenIrError::unsupported(format!(
                "{} for indexed-array element PHP type {:?}",
                name,
                elem
            )))
        }
        other => Err(CodegenIrError::unsupported(format!("{} for PHP type {:?}", name, other))),
    }
}

/// Returns the runtime helper for `array_reverse()` based on element ownership.
fn array_reverse_runtime_helper(elem_ty: &PhpType) -> &'static str {
    if elem_ty.is_refcounted() {
        "__rt_array_reverse_refcounted"
    } else {
        "__rt_array_reverse"
    }
}

/// Returns the runtime helper for `array_unique()` based on element ownership.
fn array_unique_runtime_helper(elem_ty: &PhpType) -> &'static str {
    if elem_ty.is_refcounted() {
        "__rt_array_unique_refcounted"
    } else {
        "__rt_array_unique"
    }
}

/// Returns the runtime helper for `array_merge()` based on element ownership.
fn array_merge_runtime_helper(elem_ty: &PhpType) -> &'static str {
    if elem_ty.is_refcounted() {
        "__rt_array_merge_refcounted"
    } else {
        "__rt_array_merge"
    }
}

/// Returns the source element type when `array_flip()` can use existing runtime helpers.
fn array_flip_source_element_type(ty: PhpType) -> Result<PhpType> {
    match ty.codegen_repr() {
        PhpType::Array(elem) => {
            let elem = elem.codegen_repr();
            if matches!(
                elem,
                PhpType::Int | PhpType::Bool | PhpType::Str | PhpType::Void | PhpType::Never
            ) {
                return Ok(elem);
            }
            Err(CodegenIrError::unsupported(format!(
                "array_flip source element PHP type {:?}",
                elem
            )))
        }
        other => Err(CodegenIrError::unsupported(format!(
            "array_flip for PHP type {:?}",
            other
        ))),
    }
}

/// Returns the source element type when `array_slice()` can use legacy pointer-sized helpers.
fn array_slice_source_element_type(ty: PhpType) -> Result<PhpType> {
    match ty.codegen_repr() {
        PhpType::Array(elem) => {
            let elem = elem.codegen_repr();
            require_array_slice_element_layout(&elem)?;
            Ok(elem)
        }
        other => Err(CodegenIrError::unsupported(format!(
            "array_slice for PHP type {:?}",
            other
        ))),
    }
}

/// Returns the copied element type when `array_chunk()` can use legacy pointer-sized helpers.
fn array_chunk_source_element_type(ty: PhpType) -> Result<PhpType> {
    match ty.codegen_repr() {
        PhpType::Array(elem) => {
            let elem = elem.codegen_repr();
            require_array_chunk_element_layout(&elem)?;
            Ok(elem)
        }
        other => Err(CodegenIrError::unsupported(format!(
            "array_chunk for PHP type {:?}",
            other
        ))),
    }
}

/// Returns the copied element type when `array_pad()` can use legacy pointer-sized helpers.
fn array_pad_source_element_type(ty: PhpType) -> Result<PhpType> {
    match ty.codegen_repr() {
        PhpType::Array(elem) => {
            let elem = elem.codegen_repr();
            require_array_pad_element_layout(&elem)?;
            Ok(elem)
        }
        other => Err(CodegenIrError::unsupported(format!(
            "array_pad for PHP type {:?}",
            other
        ))),
    }
}

/// Returns the result element type declared by the lowered builtin instruction.
fn result_array_element_type(name: &str, ty: &PhpType) -> Result<PhpType> {
    match ty {
        PhpType::Array(elem) => Ok(elem.codegen_repr()),
        other => Err(CodegenIrError::unsupported(format!(
            "{} result PHP type {:?}",
            name, other
        ))),
    }
}

/// Returns the inner chunk element type from an `array<array<T>>` result.
fn array_chunk_result_inner_element_type(result_elem_ty: &PhpType) -> Result<PhpType> {
    match result_elem_ty {
        PhpType::Array(inner) => Ok(inner.codegen_repr()),
        other => Err(CodegenIrError::unsupported(format!(
            "array_chunk result element PHP type {:?}",
            other
        ))),
    }
}

/// Verifies that the runtime slice helper can copy this element representation.
fn require_array_slice_element_layout(elem: &PhpType) -> Result<()> {
    if matches!(
        elem,
        PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Void
            | PhpType::Mixed
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Object(_)
    ) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "array_slice indexed-array element PHP type {:?}",
        elem
    )))
}

/// Verifies that the runtime chunk helper can copy this element representation.
fn require_array_chunk_element_layout(elem: &PhpType) -> Result<()> {
    if matches!(
        elem,
        PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Callable
            | PhpType::Void
    ) || elem.is_refcounted()
    {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "array_chunk indexed-array element PHP type {:?}",
        elem
    )))
}

/// Verifies that the runtime pad helper can copy this element representation.
fn require_array_pad_element_layout(elem: &PhpType) -> Result<()> {
    if matches!(
        elem,
        PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Callable
            | PhpType::Void
    ) || elem.is_refcounted()
    {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "array_pad indexed-array element PHP type {:?}",
        elem
    )))
}

/// Verifies the destination element type matches the copied layout or is a Mixed widening.
fn require_array_slice_result_type(source_elem_ty: &PhpType, result_elem_ty: &PhpType) -> Result<()> {
    if source_elem_ty == result_elem_ty || result_elem_ty == &PhpType::Mixed {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "array_slice result element PHP type {:?} for source element PHP type {:?}",
        result_elem_ty,
        source_elem_ty
    )))
}

/// Verifies the pad value can be copied into the source array's slot layout.
fn require_array_pad_value_type(source_elem_ty: &PhpType, pad_value_ty: &PhpType) -> Result<()> {
    if source_elem_ty == pad_value_ty {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "array_pad value PHP type {:?} for source element PHP type {:?}",
        pad_value_ty,
        source_elem_ty
    )))
}

/// Verifies the produced padded array retains the source element type.
fn require_array_pad_result_type(source_elem_ty: &PhpType, result_elem_ty: &PhpType) -> Result<()> {
    if source_elem_ty == result_elem_ty || result_elem_ty == &PhpType::Mixed {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "array_pad result element PHP type {:?} for source element PHP type {:?}",
        result_elem_ty,
        source_elem_ty
    )))
}

/// Verifies the produced chunk inner arrays retain the source element type.
fn require_array_chunk_result_type(source_elem_ty: &PhpType, result_inner_elem_ty: &PhpType) -> Result<()> {
    if source_elem_ty == result_inner_elem_ty {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "array_chunk result inner element PHP type {:?} for source element PHP type {:?}",
        result_inner_elem_ty,
        source_elem_ty
    )))
}

/// Calls the appropriate legacy runtime helper after materializing slice arguments.
fn lower_array_slice_call(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    offset: ValueId,
    length: Option<ValueId>,
    source_elem_ty: &PhpType,
) -> Result<()> {
    lower_slice_like_args(ctx, array, offset, length, "array_slice")?;
    abi::emit_call_label(ctx.emitter, array_slice_runtime_helper(source_elem_ty));
    Ok(())
}

/// Calls the appropriate legacy runtime helper after materializing splice arguments.
fn lower_array_splice_call(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    offset: ValueId,
    length: Option<ValueId>,
    elem_ty: &PhpType,
) -> Result<()> {
    lower_slice_like_args(ctx, array, offset, length, "array_splice")?;
    abi::emit_call_label(ctx.emitter, array_splice_runtime_helper(elem_ty));
    Ok(())
}

/// Materializes the shared `(array, offset, length)` argument triple for `array_slice` and
/// `array_splice` into the runtime argument registers.
///
/// The offset and length are resolved to plain integers first — unboxing a `Mixed` cell read from a
/// heterogeneous array via `__rt_mixed_cast_int` — and spilled to the stack, because that unbox call
/// clobbers caller-saved registers. The array pointer (a plain stack load that clobbers nothing) is
/// then placed, and the staged integers are restored into the offset/length argument registers, so
/// the runtime helper sees the array pointer plus two genuine integers rather than a boxed pointer.
fn lower_slice_like_args(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    offset: ValueId,
    length: Option<ValueId>,
    name: &str,
) -> Result<()> {
    resolve_int_operand_to_result(ctx, offset, &format!("{} offset", name))?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    resolve_slice_length_to_result(ctx, length, name)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(array, "x0")?;
            abi::emit_pop_reg(ctx.emitter, "x2");                                   // restore the resolved length into the third runtime argument
            abi::emit_pop_reg(ctx.emitter, "x1");                                   // restore the resolved offset into the second runtime argument
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(array, "rdi")?;
            abi::emit_pop_reg(ctx.emitter, "rdx");                                  // restore the resolved length into the third runtime argument
            abi::emit_pop_reg(ctx.emitter, "rsi");                                  // restore the resolved offset into the second runtime argument
        }
    }
    Ok(())
}

/// Resolves an optional `array_slice`/`array_splice` length into the integer result register.
///
/// An absent or `Void` length becomes the runtime "until the end" sentinel; otherwise the length is
/// resolved through the shared integer resolver, unboxing a `Mixed` value to a plain integer.
fn resolve_slice_length_to_result(
    ctx: &mut FunctionContext<'_>,
    length: Option<ValueId>,
    name: &str,
) -> Result<()> {
    let until_end = match length {
        None => true,
        Some(length) => matches!(ctx.value_php_type(length)?.codegen_repr(), PhpType::Void),
    };
    if until_end {
        let reg = abi::int_result_reg(ctx.emitter);
        emit_array_slice_until_end_sentinel(ctx, reg);
        return Ok(());
    }
    resolve_int_operand_to_result(ctx, length.expect("length present"), &format!("{} length", name))
}

/// Resolves the offset/length arguments for a boxed-Mixed `array_slice`/`array_splice` into the
/// refcounted runtime helper's argument registers, restoring a previously-staged array pointer.
///
/// On entry the converted (now-owned) indexed-array pointer must be the topmost value on the
/// temporary stack. The offset and length are resolved to plain integers first — `__rt_mixed_cast_int`
/// unboxes a `Mixed` cell read from a heterogeneous array, and an absent/`Void` length becomes the
/// until-the-end sentinel — and spilled to the stack, because each unbox call clobbers caller-saved
/// registers. The three staged values are then popped into the array/offset/length argument registers
/// so the helper sees a pointer plus two genuine integers rather than a boxed pointer.
fn materialize_mixed_slice_args(
    ctx: &mut FunctionContext<'_>,
    offset: ValueId,
    length: Option<ValueId>,
    name: &str,
) -> Result<()> {
    resolve_int_operand_to_result(ctx, offset, &format!("{} offset", name))?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    resolve_slice_length_to_result(ctx, length, name)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_reg(ctx.emitter, "x2");                                   // restore the resolved length into the third runtime argument
            abi::emit_pop_reg(ctx.emitter, "x1");                                   // restore the resolved offset into the second runtime argument
            abi::emit_pop_reg(ctx.emitter, "x0");                                   // restore the converted array pointer into the first runtime argument
        }
        Arch::X86_64 => {
            abi::emit_pop_reg(ctx.emitter, "rdx");                                  // restore the resolved length into the third runtime argument
            abi::emit_pop_reg(ctx.emitter, "rsi");                                  // restore the resolved offset into the second runtime argument
            abi::emit_pop_reg(ctx.emitter, "rdi");                                  // restore the converted array pointer into the first runtime argument
        }
    }
    Ok(())
}

/// Materializes a boxed-Mixed indexed array for `array_slice()` on AArch64.
fn lower_mixed_array_slice_aarch64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    offset: ValueId,
    length: Option<ValueId>,
) -> Result<()> {
    let empty_label = ctx.next_label("mixed_array_slice_empty");
    let done_label = ctx.next_label("mixed_array_slice_done");
    ctx.load_value_to_reg(array, "x0")?;
    abi::emit_push_reg(ctx.emitter, "x0");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    ctx.emitter.instruction("cmp x0, #4");                                      // require an indexed-array payload before slicing the Mixed cell
    ctx.emitter.instruction(&format!("b.ne {}", empty_label));                  // return an empty slice for non-array Mixed payloads
    ctx.emitter.instruction(&format!("cbz x1, {}", empty_label));               // return an empty slice for null array payloads
    ctx.emitter.instruction("mov x0, x1");                                      // pass the unboxed indexed-array payload to the Mixed conversion helper
    ctx.emitter.instruction("ldr x1, [x0, #-8]");                               // load indexed-array metadata before Mixed-slot conversion
    ctx.emitter.instruction("lsr x1, x1, #8");                                  // move the runtime value_type tag into the low bits
    ctx.emitter.instruction("and x1, x1, #0x7f");                               // isolate the indexed-array value_type tag
    abi::emit_call_label(ctx.emitter, "__rt_array_to_mixed");
    abi::emit_pop_reg(ctx.emitter, "x10");
    ctx.emitter.instruction("str x0, [x10, #8]");                               // publish the converted unique array back into the Mixed cell
    abi::emit_push_reg(ctx.emitter, "x0");
    materialize_mixed_slice_args(ctx, offset, length, "array_slice")?;
    abi::emit_call_label(ctx.emitter, "__rt_array_slice_refcounted");
    ctx.emitter.instruction(&format!("b {}", done_label));                      // skip the empty-array fallback after slicing the boxed payload
    ctx.emitter.label(&empty_label);
    abi::emit_pop_reg(ctx.emitter, "x9");
    allocate_empty_mixed_array_result(ctx);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Materializes a boxed-Mixed indexed array for `array_slice()` on x86_64.
fn lower_mixed_array_slice_x86_64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    offset: ValueId,
    length: Option<ValueId>,
) -> Result<()> {
    let empty_label = ctx.next_label("mixed_array_slice_empty");
    let done_label = ctx.next_label("mixed_array_slice_done");
    ctx.load_value_to_reg(array, "rax")?;
    abi::emit_push_reg(ctx.emitter, "rax");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    ctx.emitter.instruction("cmp rax, 4");                                      // require an indexed-array payload before slicing the Mixed cell
    ctx.emitter.instruction(&format!("jne {}", empty_label));                   // return an empty slice for non-array Mixed payloads
    ctx.emitter.instruction("test rdi, rdi");                                   // verify the unboxed indexed-array payload is present
    ctx.emitter.instruction(&format!("je {}", empty_label));                    // return an empty slice for null array payloads
    ctx.emitter.instruction("mov rsi, QWORD PTR [rdi - 8]");                    // load indexed-array metadata before Mixed-slot conversion
    ctx.emitter.instruction("shr rsi, 8");                                      // move the runtime value_type tag into the low bits
    ctx.emitter.instruction("and rsi, 0x7f");                                   // isolate the indexed-array value_type tag
    abi::emit_call_label(ctx.emitter, "__rt_array_to_mixed");
    abi::emit_pop_reg(ctx.emitter, "r10");
    ctx.emitter.instruction("mov QWORD PTR [r10 + 8], rax");                    // publish the converted unique array back into the Mixed cell
    abi::emit_push_reg(ctx.emitter, "rax");
    materialize_mixed_slice_args(ctx, offset, length, "array_slice")?;
    abi::emit_call_label(ctx.emitter, "__rt_array_slice_refcounted");
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip the empty-array fallback after slicing the boxed payload
    ctx.emitter.label(&empty_label);
    abi::emit_pop_reg(ctx.emitter, "r11");
    allocate_empty_mixed_array_result(ctx);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Materializes and mutates a boxed-Mixed indexed array for `array_splice()` on AArch64.
fn lower_mixed_array_splice_aarch64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    offset: ValueId,
    length: Option<ValueId>,
) -> Result<()> {
    let drop_label = ctx.next_label("mixed_array_splice_empty");
    let done_label = ctx.next_label("mixed_array_splice_done");
    ctx.load_value_to_reg(array, "x0")?;
    abi::emit_push_reg(ctx.emitter, "x0");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    ctx.emitter.instruction("cmp x0, #4");                                      // require an indexed-array payload before splicing the Mixed cell
    ctx.emitter.instruction(&format!("b.ne {}", drop_label));                   // return an empty removed-elements array for non-array Mixed payloads
    ctx.emitter.instruction(&format!("cbz x1, {}", drop_label));                // return an empty removed-elements array for null array payloads
    ctx.emitter.instruction("mov x0, x1");                                      // pass the unboxed indexed-array payload to the Mixed conversion helper
    ctx.emitter.instruction("ldr x1, [x0, #-8]");                               // load indexed-array metadata before Mixed-slot conversion
    ctx.emitter.instruction("lsr x1, x1, #8");                                  // move the runtime value_type tag into the low bits
    ctx.emitter.instruction("and x1, x1, #0x7f");                               // isolate the indexed-array value_type tag
    abi::emit_call_label(ctx.emitter, "__rt_array_to_mixed");
    abi::emit_pop_reg(ctx.emitter, "x10");
    ctx.emitter.instruction("str x0, [x10, #8]");                               // publish the converted unique array back into the Mixed cell
    abi::emit_push_reg(ctx.emitter, "x0");
    materialize_mixed_slice_args(ctx, offset, length, "array_splice")?;
    abi::emit_call_label(ctx.emitter, "__rt_array_splice_refcounted");
    ctx.emitter.instruction(&format!("b {}", done_label));                      // skip the empty-array fallback after splicing the boxed payload
    ctx.emitter.label(&drop_label);
    abi::emit_pop_reg(ctx.emitter, "x9");
    allocate_empty_mixed_array_result(ctx);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Materializes and mutates a boxed-Mixed indexed array for `array_splice()` on x86_64.
fn lower_mixed_array_splice_x86_64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    offset: ValueId,
    length: Option<ValueId>,
) -> Result<()> {
    let drop_label = ctx.next_label("mixed_array_splice_empty");
    let done_label = ctx.next_label("mixed_array_splice_done");
    ctx.load_value_to_reg(array, "rax")?;
    abi::emit_push_reg(ctx.emitter, "rax");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    ctx.emitter.instruction("cmp rax, 4");                                      // require an indexed-array payload before splicing the Mixed cell
    ctx.emitter.instruction(&format!("jne {}", drop_label));                    // return an empty removed-elements array for non-array Mixed payloads
    ctx.emitter.instruction("test rdi, rdi");                                   // verify the unboxed indexed-array payload is present
    ctx.emitter.instruction(&format!("je {}", drop_label));                     // return an empty removed-elements array for null array payloads
    ctx.emitter.instruction("mov rsi, QWORD PTR [rdi - 8]");                    // load indexed-array metadata before Mixed-slot conversion
    ctx.emitter.instruction("shr rsi, 8");                                      // move the runtime value_type tag into the low bits
    ctx.emitter.instruction("and rsi, 0x7f");                                   // isolate the indexed-array value_type tag
    abi::emit_call_label(ctx.emitter, "__rt_array_to_mixed");
    abi::emit_pop_reg(ctx.emitter, "r10");
    ctx.emitter.instruction("mov QWORD PTR [r10 + 8], rax");                    // publish the converted unique array back into the Mixed cell
    abi::emit_push_reg(ctx.emitter, "rax");
    materialize_mixed_slice_args(ctx, offset, length, "array_splice")?;
    abi::emit_call_label(ctx.emitter, "__rt_array_splice_refcounted");
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip the empty-array fallback after splicing the boxed payload
    ctx.emitter.label(&drop_label);
    abi::emit_pop_reg(ctx.emitter, "r11");
    allocate_empty_mixed_array_result(ctx);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Adapts the removed-elements array returned by `array_splice` to the EIR result type.
fn normalize_array_splice_result(
    ctx: &mut FunctionContext<'_>,
    elem_ty: &PhpType,
    result_ty: &PhpType,
) -> Result<()> {
    let removed_ty = PhpType::Array(Box::new(elem_ty.codegen_repr()));
    match result_ty {
        PhpType::Mixed => {
            emit_box_current_owned_value_as_mixed(ctx.emitter, &removed_ty);
            Ok(())
        }
        PhpType::Array(result_elem) if result_elem.codegen_repr() == elem_ty.codegen_repr() => {
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "array_splice result PHP type {:?}",
            other
        ))),
    }
}

/// Allocates an empty boxed-Mixed indexed array for dynamic splice fallback paths.
fn allocate_empty_mixed_array_result(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", 0);
            abi::emit_load_int_immediate(ctx.emitter, "x1", 8);
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdi", 0);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", 8);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
    crate::codegen::emit_array_value_type_stamp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        &PhpType::Mixed,
    );
}

/// Calls the appropriate legacy runtime helper after materializing chunk arguments.
fn lower_array_chunk_call(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    length: ValueId,
    source_elem_ty: &PhpType,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(array, "x0")?;
            ctx.load_value_to_reg(length, "x1")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(array, "rdi")?;
            ctx.load_value_to_reg(length, "rsi")?;
        }
    }
    abi::emit_call_label(ctx.emitter, array_chunk_runtime_helper(source_elem_ty));
    Ok(())
}

/// Calls the appropriate legacy runtime helper after materializing pad arguments.
fn lower_array_pad_call(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    target_size: ValueId,
    pad_value: ValueId,
    source_elem_ty: &PhpType,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(array, "x0")?;
            ctx.load_value_to_reg(target_size, "x1")?;
            ctx.load_value_to_reg(pad_value, "x2")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(array, "rdi")?;
            ctx.load_value_to_reg(target_size, "rsi")?;
            ctx.load_value_to_reg(pad_value, "rdx")?;
        }
    }
    abi::emit_call_label(ctx.emitter, array_pad_runtime_helper(source_elem_ty));
    Ok(())
}

/// Emits the `-1` runtime sentinel used when slicing to the end of the source array.
fn emit_array_slice_until_end_sentinel(ctx: &mut FunctionContext<'_>, reg: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("mov {}, #-1", reg));              // use -1 as the array_slice() runtime sentinel for length until the end
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov {}, -1", reg));               // use -1 as the x86_64 array_slice() runtime sentinel for length until the end
        }
    }
}

/// Returns the helper that matches the chunk source element ownership representation.
fn array_chunk_runtime_helper(source_elem_ty: &PhpType) -> &'static str {
    if source_elem_ty.is_refcounted() {
        "__rt_array_chunk_refcounted"
    } else {
        "__rt_array_chunk"
    }
}

/// Returns the helper that matches the pad source element ownership representation.
fn array_pad_runtime_helper(source_elem_ty: &PhpType) -> &'static str {
    if source_elem_ty.is_refcounted() {
        "__rt_array_pad_refcounted"
    } else {
        "__rt_array_pad"
    }
}

/// Returns the helper that matches the source element ownership representation.
fn array_slice_runtime_helper(source_elem_ty: &PhpType) -> &'static str {
    if source_elem_ty.is_refcounted() {
        "__rt_array_slice_refcounted"
    } else {
        "__rt_array_slice"
    }
}

/// Returns the helper that matches the spliced element ownership representation.
fn array_splice_runtime_helper(elem_ty: &PhpType) -> &'static str {
    if elem_ty.is_refcounted() {
        "__rt_array_splice_refcounted"
    } else {
        "__rt_array_splice"
    }
}

/// Stamps the result array and widens typed slots when the EIR result expects Mixed.
fn normalize_indexed_array_result(
    ctx: &mut FunctionContext<'_>,
    name: &str,
    source_elem_ty: &PhpType,
    result_elem_ty: &PhpType,
) -> Result<()> {
    if result_elem_ty == &PhpType::Mixed && source_elem_ty != &PhpType::Mixed {
        let source_tag = runtime_value_tag(name, source_elem_ty)?;
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("mov x1, #{}", source_tag));   // pass the source slot value_type tag to widen the indexed-array result to Mixed
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov rdi, rax");                        // pass the produced indexed-array pointer to the Mixed-widening helper
                ctx.emitter.instruction(&format!("mov rsi, {}", source_tag));   // pass the source slot value_type tag to widen the indexed-array result to Mixed
            }
        }
        abi::emit_call_label(ctx.emitter, "__rt_array_to_mixed");
        return Ok(());
    }
    crate::codegen::emit_array_value_type_stamp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        result_elem_ty,
    );
    Ok(())
}

/// Returns the runtime value_type tag used by the array-to-Mixed widening helper.
fn runtime_value_tag(name: &str, elem: &PhpType) -> Result<u8> {
    match elem {
        PhpType::Int => Ok(0),
        PhpType::Str => Ok(1),
        PhpType::Float => Ok(2),
        PhpType::Bool => Ok(3),
        PhpType::Array(_) => Ok(4),
        PhpType::AssocArray { .. } => Ok(5),
        PhpType::Object(_) => Ok(6),
        PhpType::Mixed => Ok(7),
        PhpType::Void => Ok(8),
        PhpType::Callable => Ok(10),
        other => Err(CodegenIrError::unsupported(format!(
            "{} Mixed widening for element PHP type {:?}",
            name, other
        ))),
    }
}

/// Verifies a builtin receives an indexed array operand.
fn require_indexed_array_builtin(ty: PhpType, name: &str) -> Result<()> {
    match ty.codegen_repr() {
        PhpType::Array(_) => Ok(()),
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            name,
            other
        ))),
    }
}

/// Returns the supported element payload type for an indexed-array `array_pop()`.
fn array_pop_element_type(ty: PhpType) -> Result<PhpType> {
    match ty.codegen_repr() {
        PhpType::Array(elem) => {
            let elem = elem.codegen_repr();
            if matches!(
                elem,
                PhpType::Int
                    | PhpType::Bool
                    | PhpType::Float
                    | PhpType::Str
                    | PhpType::Callable
                    | PhpType::Mixed
                    | PhpType::Void
                    | PhpType::Never
            ) || elem.is_refcounted()
            {
                return Ok(elem);
            }
            Err(CodegenIrError::unsupported(format!(
                "array_pop indexed-array element PHP type {:?}",
                elem
            )))
        }
        other => Err(CodegenIrError::unsupported(format!(
            "array_pop for PHP type {:?}",
            other
        ))),
    }
}

/// Verifies the lowered `array_pop()` result uses PHP's `mixed` shape.
fn require_array_pop_result_type(result_ty: &PhpType) -> Result<()> {
    if result_ty == &PhpType::Mixed {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "array_pop result PHP type {:?}",
        result_ty
    )))
}

/// Splits a shared indexed array before `array_pop()` mutates its header.
fn ensure_unique_array_pop_source(ctx: &mut FunctionContext<'_>, array: ValueId) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(array, "x0")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(array, "rdi")?;
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_ensure_unique");
    ctx.store_result_value(array)
}

/// Emits the AArch64 `array_pop()` sequence for indexed arrays.
fn lower_array_pop_aarch64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    elem_ty: &PhpType,
) -> Result<()> {
    let empty_label = ctx.next_label("array_pop_empty");
    let done_label = ctx.next_label("array_pop_done");
    ctx.load_value_to_reg(array, "x0")?;
    ctx.emitter.instruction("ldr x9, [x0]");                                    // load the indexed-array length before deciding whether pop is empty
    ctx.emitter.instruction(&format!("cbz x9, {}", empty_label));               // return boxed null when array_pop() runs on an empty array
    ctx.emitter.instruction("sub x9, x9, #1");                                  // convert the old length into the removed last-element index
    ctx.emitter.instruction("str x9, [x0]");                                    // persist the shortened indexed-array length in the header
    emit_array_pop_value_aarch64(ctx, elem_ty)?;
    crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, elem_ty);
    ctx.emitter.instruction(&format!("b {}", done_label));                      // skip the empty-array boxed-null path after loading the removed value
    ctx.emitter.label(&empty_label);
    emit_array_pop_null(ctx);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits the x86_64 `array_pop()` sequence for indexed arrays.
fn lower_array_pop_x86_64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    elem_ty: &PhpType,
) -> Result<()> {
    let empty_label = ctx.next_label("array_pop_empty");
    let done_label = ctx.next_label("array_pop_done");
    ctx.load_value_to_reg(array, "rax")?;
    ctx.emitter.instruction("mov r10, QWORD PTR [rax]");                        // load the indexed-array length before deciding whether pop is empty
    ctx.emitter.instruction("test r10, r10");                                   // check whether the indexed array has any live elements
    ctx.emitter.instruction(&format!("jz {}", empty_label));                    // return boxed null when array_pop() runs on an empty array
    ctx.emitter.instruction("sub r10, 1");                                      // convert the old length into the removed last-element index
    ctx.emitter.instruction("mov QWORD PTR [rax], r10");                        // persist the shortened indexed-array length in the header
    emit_array_pop_value_x86_64(ctx, elem_ty)?;
    crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, elem_ty);
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip the empty-array boxed-null path after loading the removed value
    ctx.emitter.label(&empty_label);
    emit_array_pop_null(ctx);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Loads the removed AArch64 indexed-array payload into the canonical result registers.
fn emit_array_pop_value_aarch64(ctx: &mut FunctionContext<'_>, elem_ty: &PhpType) -> Result<()> {
    match elem_ty {
        PhpType::Int | PhpType::Bool | PhpType::Callable | PhpType::Mixed => {
            ctx.emitter.instruction("add x10, x0, #24");                        // compute the first pointer-sized payload slot in the indexed array
            ctx.emitter.instruction("ldr x0, [x10, x9, lsl #3]");               // load the removed pointer-sized payload into the result register
        }
        PhpType::Float => {
            ctx.emitter.instruction("add x10, x0, #24");                        // compute the first float payload slot in the indexed array
            ctx.emitter.instruction("ldr d0, [x10, x9, lsl #3]");               // load the removed float payload into the result register
        }
        PhpType::Str => {
            ctx.emitter.instruction("lsl x10, x9, #4");                         // scale the removed index by the 16-byte string slot size
            ctx.emitter.instruction("add x10, x0, x10");                        // advance from the array base to the removed string slot
            ctx.emitter.instruction("add x10, x10, #24");                       // skip the indexed-array header before loading string payloads
            ctx.emitter.instruction("ldr x1, [x10]");                           // load the removed string pointer into the mixed payload register
            ctx.emitter.instruction("ldr x2, [x10, #8]");                       // load the removed string length into the mixed payload high word
        }
        PhpType::Void | PhpType::Never => {
            ctx.emitter.instruction("mov x0, #0");                              // materialize a null payload for impossible void-array live elements
        }
        other if other.is_refcounted() => {
            ctx.emitter.instruction("add x10, x0, #24");                        // compute the first refcounted payload slot in the indexed array
            ctx.emitter.instruction("ldr x0, [x10, x9, lsl #3]");               // load the removed heap pointer into the result register
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_pop element PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Loads the removed x86_64 indexed-array payload into the canonical result registers.
fn emit_array_pop_value_x86_64(ctx: &mut FunctionContext<'_>, elem_ty: &PhpType) -> Result<()> {
    match elem_ty {
        PhpType::Int | PhpType::Bool | PhpType::Callable | PhpType::Mixed => {
            ctx.emitter.instruction("lea r11, [rax + 24]");                     // compute the first pointer-sized payload slot in the indexed array
            ctx.emitter.instruction("mov rax, QWORD PTR [r11 + r10 * 8]");      // load the removed pointer-sized payload into the result register
        }
        PhpType::Float => {
            ctx.emitter.instruction("lea r11, [rax + 24]");                     // compute the first float payload slot in the indexed array
            ctx.emitter.instruction("movsd xmm0, QWORD PTR [r11 + r10 * 8]");   // load the removed float payload into the result register
        }
        PhpType::Str => {
            ctx.emitter.instruction("lea r11, [rax + 24]");                     // compute the first string payload slot in the indexed array
            ctx.emitter.instruction("shl r10, 4");                              // scale the removed index by the 16-byte string slot size
            ctx.emitter.instruction("add r11, r10");                            // advance to the removed string slot payload
            ctx.emitter.instruction("mov rax, QWORD PTR [r11]");                // load the removed string pointer into the string result register
            ctx.emitter.instruction("mov rdx, QWORD PTR [r11 + 8]");            // load the removed string length into the string result register
        }
        PhpType::Void | PhpType::Never => {
            ctx.emitter.instruction("xor eax, eax");                            // materialize a null payload for impossible void-array live elements
        }
        other if other.is_refcounted() => {
            ctx.emitter.instruction("lea r11, [rax + 24]");                     // compute the first refcounted payload slot in the indexed array
            ctx.emitter.instruction("mov rax, QWORD PTR [r11 + r10 * 8]");      // load the removed heap pointer into the result register
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_pop element PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Boxes PHP null for an empty `array_pop()` result.
fn emit_array_pop_null(ctx: &mut FunctionContext<'_>) {
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Void);
}

/// Returns the local slot loaded by an `array_pop()` argument when it came from `load_local`.
fn source_load_local_slot(ctx: &FunctionContext<'_>, value: ValueId) -> Result<Option<LocalSlotId>> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(None);
    };
    let Some(inst_ref) = ctx.function.instruction(inst) else {
        return Err(CodegenIrError::missing_entry("instruction", inst.as_raw()));
    };
    if inst_ref.op == Op::LoadLocal {
        if let Some(Immediate::LocalSlot(slot)) = inst_ref.immediate {
            return Ok(Some(slot));
        }
    }
    Ok(None)
}

/// Describes which indexed-array `array_search()` lowering path applies.
enum ArraySearchCase {
    Empty,
    Scalar,
    String,
}

/// Verifies that an indexed-array `array_search()` call can use the scalar search helper.
fn supported_array_search_case(needle_ty: PhpType, array_ty: PhpType) -> Result<ArraySearchCase> {
    let needle_ty = needle_ty.codegen_repr();
    match array_ty.codegen_repr() {
        PhpType::Array(elem) => match elem.codegen_repr() {
            PhpType::Never | PhpType::Void => Ok(ArraySearchCase::Empty),
            PhpType::Int | PhpType::Bool if matches!(needle_ty, PhpType::Int | PhpType::Bool) => {
                Ok(ArraySearchCase::Scalar)
            }
            PhpType::Str if needle_ty == PhpType::Str => Ok(ArraySearchCase::String),
            elem_ty => Err(CodegenIrError::unsupported(format!(
                "array_search needle PHP type {:?} for indexed-array element PHP type {:?}",
                needle_ty,
                elem_ty
            ))),
        },
        other => Err(CodegenIrError::unsupported(format!(
            "array_search for PHP array type {:?}",
            other
        ))),
    }
}

/// Lowers integer-like indexed-array search and boxes the PHP `int|false` result.
fn lower_array_search_scalar(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(array, "x0")?;
            ctx.load_value_to_reg(needle, "x1")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(array, "rdi")?;
            ctx.load_value_to_reg(needle, "rsi")?;
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_search");
    box_array_search_result(ctx);
    Ok(())
}

/// Lowers string indexed-array search and boxes the PHP `int|false` result.
fn lower_array_search_string(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_array_search_string_aarch64(ctx, needle, array),
        Arch::X86_64 => lower_array_search_string_x86_64(ctx, needle, array),
    }
}

/// Emits the AArch64 string-array search loop.
fn lower_array_search_string_aarch64(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
) -> Result<()> {
    let loop_label = ctx.next_label("array_search_str_loop");
    let found_label = ctx.next_label("array_search_str_found");
    let miss_label = ctx.next_label("array_search_str_miss");
    let done_label = ctx.next_label("array_search_str_done");

    ctx.load_value_to_reg(array, "x10")?;
    ctx.emitter.instruction("ldr x9, [x10]");                                   // load indexed string-array length before scanning payload slots
    ctx.emitter.instruction("add x10, x10, #24");                               // point at the first indexed string-array payload slot
    ctx.emitter.instruction("mov x12, #0");                                     // start the string search at index zero
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("cmp x12, x9");                                     // compare the scan index against indexed-array length
    ctx.emitter.instruction(&format!("b.ge {}", miss_label));                   // finish with false after all string elements are scanned
    ctx.emitter.instruction("lsl x13, x12, #4");                                // scale the element index by the 16-byte string slot width
    ctx.emitter.instruction("ldr x1, [x10, x13]");                              // load the current string element pointer for comparison
    ctx.emitter.instruction("add x14, x13, #8");                                // compute the current string element length-slot offset
    ctx.emitter.instruction("ldr x2, [x10, x14]");                              // load the current string element length for comparison
    abi::emit_push_reg_pair(ctx.emitter, "x9", "x10");
    abi::emit_push_reg(ctx.emitter, "x12");
    ctx.load_string_value_to_regs(needle, "x3", "x4")?;
    abi::emit_call_label(ctx.emitter, "__rt_str_eq");
    abi::emit_pop_reg(ctx.emitter, "x12");
    abi::emit_pop_reg_pair(ctx.emitter, "x9", "x10");
    ctx.emitter.instruction(&format!("cbnz x0, {}", found_label));              // stop as soon as the searched string matches an element
    ctx.emitter.instruction("add x12, x12, #1");                                // advance to the next indexed string element
    ctx.emitter.instruction(&format!("b {}", loop_label));                      // continue scanning remaining string payload slots
    ctx.emitter.label(&found_label);
    ctx.emitter.instruction("mov x1, x12");                                     // move the found index into the mixed helper payload register
    ctx.emitter.instruction("mov x2, #0");                                      // integer mixed payloads do not use a high word
    ctx.emitter.instruction("mov x0, #0");                                      // runtime tag 0 = integer
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
    ctx.emitter.instruction(&format!("b {}", done_label));                      // skip false boxing after producing the found index
    ctx.emitter.label(&miss_label);
    box_array_search_miss(ctx);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits the x86_64 string-array search loop.
fn lower_array_search_string_x86_64(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
) -> Result<()> {
    let loop_label = ctx.next_label("array_search_str_loop");
    let found_label = ctx.next_label("array_search_str_found");
    let miss_label = ctx.next_label("array_search_str_miss");
    let done_label = ctx.next_label("array_search_str_done");

    ctx.load_value_to_reg(array, "r10")?;
    ctx.emitter.instruction("mov r11, QWORD PTR [r10]");                        // load indexed string-array length before scanning payload slots
    ctx.emitter.instruction("lea r12, [r10 + 24]");                             // point at the first indexed string-array payload slot
    ctx.emitter.instruction("xor r13d, r13d");                                  // start the string search at index zero
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("cmp r13, r11");                                    // compare the scan index against indexed-array length
    ctx.emitter.instruction(&format!("jge {}", miss_label));                    // finish with false after all string elements are scanned
    ctx.emitter.instruction("mov rcx, r13");                                    // copy the scan index before scaling it to a byte offset
    ctx.emitter.instruction("shl rcx, 4");                                      // scale the element index by the 16-byte string slot width
    ctx.emitter.instruction("mov rdi, QWORD PTR [r12 + rcx]");                  // load the current string element pointer for comparison
    ctx.emitter.instruction("mov rsi, QWORD PTR [r12 + rcx + 8]");              // load the current string element length for comparison
    abi::emit_push_reg_pair(ctx.emitter, "r11", "r12");
    abi::emit_push_reg(ctx.emitter, "r13");
    ctx.load_string_value_to_regs(needle, "rdx", "rcx")?;
    abi::emit_call_label(ctx.emitter, "__rt_str_eq");
    abi::emit_pop_reg(ctx.emitter, "r13");
    abi::emit_pop_reg_pair(ctx.emitter, "r11", "r12");
    ctx.emitter.instruction("test rax, rax");                                   // check whether the current string element matched the needle
    ctx.emitter.instruction(&format!("jne {}", found_label));                   // stop as soon as the searched string matches an element
    ctx.emitter.instruction("add r13, 1");                                      // advance to the next indexed string element
    ctx.emitter.instruction(&format!("jmp {}", loop_label));                    // continue scanning remaining string payload slots
    ctx.emitter.label(&found_label);
    ctx.emitter.instruction("mov rdi, r13");                                    // move the found index into the mixed helper payload register
    ctx.emitter.instruction("xor esi, esi");                                    // integer mixed payloads do not use a high word
    ctx.emitter.instruction("xor eax, eax");                                    // runtime tag 0 = integer
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip false boxing after producing the found index
    ctx.emitter.label(&miss_label);
    box_array_search_miss(ctx);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Boxes a raw array-search helper result into PHP `int|false` Mixed form.
fn box_array_search_result(ctx: &mut FunctionContext<'_>) {
    let found_label = ctx.next_label("array_search_found");
    let end_label = ctx.next_label("array_search_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // distinguish a found index from the array_search() not-found sentinel
            ctx.emitter.instruction(&format!("b.ge {}", found_label));          // box a found index as an integer mixed result
            box_array_search_miss(ctx);
            ctx.emitter.instruction(&format!("b {}", end_label));               // skip integer boxing after producing false for a miss
            ctx.emitter.label(&found_label);
            ctx.emitter.instruction("mov x1, x0");                              // move the found index into the mixed helper payload register
            ctx.emitter.instruction("mov x2, #0");                              // integer mixed payloads do not use a high word
            ctx.emitter.instruction("mov x0, #0");                              // runtime tag 0 = integer
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&end_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 0");                              // distinguish a found index from the array_search() not-found sentinel
            ctx.emitter.instruction(&format!("jge {}", found_label));           // box a found index as an integer mixed result
            box_array_search_miss(ctx);
            ctx.emitter.instruction(&format!("jmp {}", end_label));             // skip integer boxing after producing false for a miss
            ctx.emitter.label(&found_label);
            ctx.emitter.instruction("mov rdi, rax");                            // move the found index into the mixed helper payload register
            ctx.emitter.instruction("xor esi, esi");                            // integer mixed payloads do not use a high word
            ctx.emitter.instruction("xor eax, eax");                            // runtime tag 0 = integer
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&end_label);
        }
    }
}

/// Boxes `false` for an array-search miss.
fn box_array_search_miss(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, #0");                              // false mixed payload is zero
            ctx.emitter.instruction("mov x2, #0");                              // bool mixed payloads do not use a high word
            ctx.emitter.instruction("mov x0, #3");                              // runtime tag 3 = bool
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("xor edi, edi");                            // false mixed payload is zero
            ctx.emitter.instruction("xor esi, esi");                            // bool mixed payloads do not use a high word
            ctx.emitter.instruction("mov eax, 3");                              // runtime tag 3 = bool
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
        }
    }
}

/// Describes which indexed-array `in_array()` lowering path applies.
enum InArrayCase {
    Empty,
    Scalar,
    String,
    MixedString,
}

/// Verifies that an indexed-array `in_array()` call has a lowered Phase 04 payload shape.
fn supported_in_array_case(needle_ty: PhpType, array_ty: PhpType) -> Result<InArrayCase> {
    let needle_ty = needle_ty.codegen_repr();
    match array_ty.codegen_repr() {
        PhpType::Array(elem) => match elem.codegen_repr() {
            PhpType::Never | PhpType::Void => Ok(InArrayCase::Empty),
            PhpType::Int | PhpType::Bool if matches!(needle_ty, PhpType::Int | PhpType::Bool) => {
                Ok(InArrayCase::Scalar)
            }
            PhpType::Str if needle_ty == PhpType::Str => Ok(InArrayCase::String),
            // An indexed `array<Mixed>` (e.g. the boxed result of a function that returns a
            // container built from an untyped parameter) stores one boxed Mixed cell per 8-byte
            // slot. A string needle is matched by unboxing each cell and string-comparing the
            // string-tagged ones, mirroring the concrete string-array path's `__rt_str_eq` scan.
            PhpType::Mixed if needle_ty == PhpType::Str => Ok(InArrayCase::MixedString),
            elem_ty => Err(CodegenIrError::unsupported(format!(
                "in_array needle PHP type {:?} for indexed-array element PHP type {:?}",
                needle_ty,
                elem_ty
            ))),
        },
        other => Err(CodegenIrError::unsupported(format!(
            "in_array for PHP array type {:?}",
            other
        ))),
    }
}

/// Lowers integer-like indexed-array membership via the existing search helper.
fn lower_in_array_scalar(
    ctx: &mut FunctionContext<'_>,
    needle: crate::ir::ValueId,
    array: crate::ir::ValueId,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(array, "x0")?;
            ctx.load_value_to_reg(needle, "x1")?;
            abi::emit_call_label(ctx.emitter, "__rt_array_search");
            ctx.emitter.instruction("cmp x0, #0");                              // check whether indexed-array search returned a non-negative match index
            ctx.emitter.instruction("cset x0, ge");                             // materialize in_array() as true for any found index
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(array, "rdi")?;
            ctx.load_value_to_reg(needle, "rsi")?;
            abi::emit_call_label(ctx.emitter, "__rt_array_search");
            ctx.emitter.instruction("cmp rax, 0");                              // check whether indexed-array search returned a non-negative match index
            ctx.emitter.instruction("setge al");                                // materialize in_array() as true for any found index
            ctx.emitter.instruction("movzx rax, al");                           // widen the membership flag into the integer result register
        }
    }
    Ok(())
}

/// Lowers string indexed-array membership with a linear scan and `__rt_str_eq`.
fn lower_in_array_string(
    ctx: &mut FunctionContext<'_>,
    needle: crate::ir::ValueId,
    array: crate::ir::ValueId,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_in_array_string_aarch64(ctx, needle, array),
        Arch::X86_64 => lower_in_array_string_x86_64(ctx, needle, array),
    }
}

/// Emits the AArch64 string-array membership loop.
fn lower_in_array_string_aarch64(
    ctx: &mut FunctionContext<'_>,
    needle: crate::ir::ValueId,
    array: crate::ir::ValueId,
) -> Result<()> {
    let loop_label = ctx.next_label("in_array_str_loop");
    let found_label = ctx.next_label("in_array_str_found");
    let end_label = ctx.next_label("in_array_str_end");
    let done_label = ctx.next_label("in_array_str_done");

    ctx.load_value_to_reg(array, "x10")?;
    ctx.emitter.instruction("ldr x9, [x10]");                                   // load indexed string-array length before scanning payload slots
    ctx.emitter.instruction("add x10, x10, #24");                               // point at the first indexed string-array payload slot
    ctx.emitter.instruction("mov x12, #0");                                     // start the string membership scan at index zero
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("cmp x12, x9");                                     // compare the scan index against indexed-array length
    ctx.emitter.instruction(&format!("b.ge {}", end_label));                    // finish with false after all string elements are scanned
    ctx.emitter.instruction("lsl x13, x12, #4");                                // scale the element index by the 16-byte string slot width
    ctx.emitter.instruction("ldr x1, [x10, x13]");                              // load the current string element pointer for comparison
    ctx.emitter.instruction("add x14, x13, #8");                                // compute the current string element length-slot offset
    ctx.emitter.instruction("ldr x2, [x10, x14]");                              // load the current string element length for comparison
    abi::emit_push_reg_pair(ctx.emitter, "x9", "x10");
    abi::emit_push_reg(ctx.emitter, "x12");
    ctx.load_string_value_to_regs(needle, "x3", "x4")?;
    abi::emit_call_label(ctx.emitter, "__rt_str_eq");
    abi::emit_pop_reg(ctx.emitter, "x12");
    abi::emit_pop_reg_pair(ctx.emitter, "x9", "x10");
    ctx.emitter.instruction(&format!("cbnz x0, {}", found_label));              // stop as soon as the searched string matches an element
    ctx.emitter.instruction("add x12, x12, #1");                                // advance to the next indexed string element
    ctx.emitter.instruction(&format!("b {}", loop_label));                      // continue scanning remaining string payload slots
    ctx.emitter.label(&found_label);
    ctx.emitter.instruction("mov x0, #1");                                      // return true after finding the searched string
    ctx.emitter.instruction(&format!("b {}", done_label));                      // skip the not-found result after a match
    ctx.emitter.label(&end_label);
    ctx.emitter.instruction("mov x0, #0");                                      // return false when no indexed string element matches
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Lowers a string-needle membership scan over an indexed `array<Mixed>`.
///
/// Each 8-byte slot holds a boxed Mixed cell, so every cell is unboxed and the string-tagged ones
/// are compared with `__rt_str_eq`, mirroring the concrete string-array path's exact-match scan.
fn lower_in_array_mixed_string(
    ctx: &mut FunctionContext<'_>,
    needle: crate::ir::ValueId,
    array: crate::ir::ValueId,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_in_array_mixed_string_aarch64(ctx, needle, array),
        Arch::X86_64 => lower_in_array_mixed_string_x86_64(ctx, needle, array),
    }
}

/// Emits the AArch64 boxed-Mixed-array string membership loop.
fn lower_in_array_mixed_string_aarch64(
    ctx: &mut FunctionContext<'_>,
    needle: crate::ir::ValueId,
    array: crate::ir::ValueId,
) -> Result<()> {
    let loop_label = ctx.next_label("in_array_mix_loop");
    let not_string_label = ctx.next_label("in_array_mix_not_string");
    let have_flag_label = ctx.next_label("in_array_mix_have_flag");
    let found_label = ctx.next_label("in_array_mix_found");
    let end_label = ctx.next_label("in_array_mix_end");
    let done_label = ctx.next_label("in_array_mix_done");

    ctx.load_value_to_reg(array, "x10")?;
    ctx.emitter.instruction("ldr x9, [x10]");                                   // load array<Mixed> length before scanning boxed slots
    ctx.emitter.instruction("add x10, x10, #24");                               // point at the first boxed Mixed cell slot
    ctx.emitter.instruction("mov x12, #0");                                     // start the membership scan at index zero
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("cmp x12, x9");                                     // compare the scan index against the array length
    ctx.emitter.instruction(&format!("b.ge {}", end_label));                    // finish with false once every cell is scanned
    ctx.emitter.instruction("ldr x0, [x10, x12, lsl #3]");                      // load the current boxed Mixed cell pointer from its 8-byte slot
    abi::emit_push_reg_pair(ctx.emitter, "x9", "x10");
    abi::emit_push_reg(ctx.emitter, "x12");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");                      // unbox the cell → x0=tag, x1=string ptr, x2=string len
    ctx.emitter.instruction("cmp x0, #1");                                      // is this cell a string value (runtime tag 1)?
    ctx.emitter.instruction(&format!("b.ne {}", not_string_label));             // non-string cells can never equal a string needle
    ctx.load_string_value_to_regs(needle, "x3", "x4")?;
    abi::emit_call_label(ctx.emitter, "__rt_str_eq");                           // compare the unboxed string element (x1/x2) against the needle (x3/x4)
    ctx.emitter.instruction(&format!("b {}", have_flag_label));                 // carry the str-eq result into the shared match-flag join
    ctx.emitter.label(&not_string_label);
    ctx.emitter.instruction("mov x0, #0");                                      // a non-string cell yields a not-matched flag
    ctx.emitter.label(&have_flag_label);
    abi::emit_pop_reg(ctx.emitter, "x12");
    abi::emit_pop_reg_pair(ctx.emitter, "x9", "x10");
    ctx.emitter.instruction(&format!("cbnz x0, {}", found_label));              // stop as soon as a cell matches the needle
    ctx.emitter.instruction("add x12, x12, #1");                                // advance to the next boxed Mixed cell
    ctx.emitter.instruction(&format!("b {}", loop_label));                      // continue scanning the remaining cells
    ctx.emitter.label(&found_label);
    ctx.emitter.instruction("mov x0, #1");                                      // return true after finding a matching cell
    ctx.emitter.instruction(&format!("b {}", done_label));                      // skip the not-found result after a match
    ctx.emitter.label(&end_label);
    ctx.emitter.instruction("mov x0, #0");                                      // return false when no cell matches the needle
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits the x86_64 boxed-Mixed-array string membership loop.
fn lower_in_array_mixed_string_x86_64(
    ctx: &mut FunctionContext<'_>,
    needle: crate::ir::ValueId,
    array: crate::ir::ValueId,
) -> Result<()> {
    let loop_label = ctx.next_label("in_array_mix_loop");
    let not_string_label = ctx.next_label("in_array_mix_not_string");
    let have_flag_label = ctx.next_label("in_array_mix_have_flag");
    let found_label = ctx.next_label("in_array_mix_found");
    let end_label = ctx.next_label("in_array_mix_end");
    let done_label = ctx.next_label("in_array_mix_done");

    ctx.load_value_to_reg(array, "r10")?;
    ctx.emitter.instruction("mov r11, QWORD PTR [r10]");                        // load array<Mixed> length before scanning boxed slots
    ctx.emitter.instruction("lea r12, [r10 + 24]");                             // point at the first boxed Mixed cell slot
    ctx.emitter.instruction("xor r13d, r13d");                                  // start the membership scan at index zero
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("cmp r13, r11");                                    // compare the scan index against the array length
    ctx.emitter.instruction(&format!("jge {}", end_label));                     // finish with false once every cell is scanned
    ctx.emitter.instruction("mov rax, QWORD PTR [r12 + r13*8]");                // load the boxed Mixed cell pointer into rax (the unbox input register)
    abi::emit_push_reg_pair(ctx.emitter, "r11", "r12");
    abi::emit_push_reg(ctx.emitter, "r13");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");                      // unbox the cell → rax=tag, rdi=string ptr, rdx=string len
    ctx.emitter.instruction("cmp rax, 1");                                      // is this cell a string value (runtime tag 1)?
    ctx.emitter.instruction(&format!("jne {}", not_string_label));              // non-string cells can never equal a string needle
    ctx.emitter.instruction("mov rsi, rdx");                                    // move the unboxed string length into the comparison argument
    ctx.load_string_value_to_regs(needle, "rdx", "rcx")?;
    abi::emit_call_label(ctx.emitter, "__rt_str_eq");                           // compare the unboxed string element (rdi/rsi) against the needle (rdx/rcx)
    ctx.emitter.instruction(&format!("jmp {}", have_flag_label));               // carry the str-eq result into the shared match-flag join
    ctx.emitter.label(&not_string_label);
    ctx.emitter.instruction("xor eax, eax");                                    // a non-string cell yields a not-matched flag
    ctx.emitter.label(&have_flag_label);
    abi::emit_pop_reg(ctx.emitter, "r13");
    abi::emit_pop_reg_pair(ctx.emitter, "r11", "r12");
    ctx.emitter.instruction("test rax, rax");                                   // did the current cell match the needle?
    ctx.emitter.instruction(&format!("jne {}", found_label));                   // stop as soon as a cell matches the needle
    ctx.emitter.instruction("add r13, 1");                                      // advance to the next boxed Mixed cell
    ctx.emitter.instruction(&format!("jmp {}", loop_label));                    // continue scanning the remaining cells
    ctx.emitter.label(&found_label);
    ctx.emitter.instruction("mov rax, 1");                                      // return true after finding a matching cell
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip the not-found result after a match
    ctx.emitter.label(&end_label);
    ctx.emitter.instruction("xor eax, eax");                                    // return false when no cell matches the needle
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits the x86_64 string-array membership loop.
fn lower_in_array_string_x86_64(
    ctx: &mut FunctionContext<'_>,
    needle: crate::ir::ValueId,
    array: crate::ir::ValueId,
) -> Result<()> {
    let loop_label = ctx.next_label("in_array_str_loop");
    let found_label = ctx.next_label("in_array_str_found");
    let end_label = ctx.next_label("in_array_str_end");
    let done_label = ctx.next_label("in_array_str_done");

    ctx.load_value_to_reg(array, "r10")?;
    ctx.emitter.instruction("mov r11, QWORD PTR [r10]");                        // load indexed string-array length before scanning payload slots
    ctx.emitter.instruction("lea r12, [r10 + 24]");                             // point at the first indexed string-array payload slot
    ctx.emitter.instruction("xor r13d, r13d");                                  // start the string membership scan at index zero
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("cmp r13, r11");                                    // compare the scan index against indexed-array length
    ctx.emitter.instruction(&format!("jge {}", end_label));                     // finish with false after all string elements are scanned
    ctx.emitter.instruction("mov rcx, r13");                                    // copy the scan index before scaling it to a byte offset
    ctx.emitter.instruction("shl rcx, 4");                                      // scale the element index by the 16-byte string slot width
    ctx.emitter.instruction("mov rdi, QWORD PTR [r12 + rcx]");                  // load the current string element pointer for comparison
    ctx.emitter.instruction("mov rsi, QWORD PTR [r12 + rcx + 8]");              // load the current string element length for comparison
    abi::emit_push_reg_pair(ctx.emitter, "r11", "r12");
    abi::emit_push_reg(ctx.emitter, "r13");
    ctx.load_string_value_to_regs(needle, "rdx", "rcx")?;
    abi::emit_call_label(ctx.emitter, "__rt_str_eq");
    abi::emit_pop_reg(ctx.emitter, "r13");
    abi::emit_pop_reg_pair(ctx.emitter, "r11", "r12");
    ctx.emitter.instruction("test rax, rax");                                   // check whether the current string element matched the needle
    ctx.emitter.instruction(&format!("jne {}", found_label));                   // stop as soon as the searched string matches an element
    ctx.emitter.instruction("add r13, 1");                                      // advance to the next indexed string element
    ctx.emitter.instruction(&format!("jmp {}", loop_label));                    // continue scanning remaining string payload slots
    ctx.emitter.label(&found_label);
    ctx.emitter.instruction("mov rax, 1");                                      // return true after finding the searched string
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip the not-found result after a match
    ctx.emitter.label(&end_label);
    ctx.emitter.instruction("xor eax, eax");                                    // return false when no indexed string element matches
    ctx.emitter.label(&done_label);
    Ok(())
}
