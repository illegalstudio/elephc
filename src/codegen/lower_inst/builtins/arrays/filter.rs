//! Purpose:
//! Array filter callback dispatch.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::arrays`.
//!
//! Key details:
//! - Preserves callback ABI, target parity, array storage, and ownership contracts.

use super::*;

/// Lowers `array_filter()` for static and first-class callbacks through the runtime helper.
pub(crate) fn lower_array_filter(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "array_filter", 1, 3)?;
    let array = expect_operand(inst, 0)?;
    if array_filter_callback_is_absent(ctx, inst)? {
        return lower_array_filter_without_callback(ctx, inst, array);
    }
    let callback = expect_operand(inst, 1)?;
    let mode = inst.operands.get(2).copied();
    let elem_ty = array_filter_source_element_type(ctx.value_php_type(array)?)?;
    require_array_filter_result_type(&elem_ty, &inst.result_php_type.codegen_repr())?;
    // php preserves the keys, so the destination is a keyed table either way. The ownership
    // rules live in `__rt_hash_clone_shallow`, which is why the refcounted/plain split the
    // indexed helpers needed does not reappear here.
    let runtime_label = if matches!(
        ctx.value_php_type(array)?.codegen_repr(),
        PhpType::AssocArray { .. }
    ) {
        "__rt_hash_filter"
    } else {
        "__rt_array_filter_keyed"
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
                    super::super::super::instruction_strict_php_profile(inst),
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
    let callback_binding = static_sort_callback_binding(
        ctx,
        callback,
        "array_filter callback",
        callback_arg_types.as_deref(),
    )?;
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

/// Reports whether this `array_filter()` call has no callback to invoke.
///
/// php's signature is `array_filter(array $array, ?callable $callback = null, int $mode = 0)`,
/// so BOTH `array_filter($a)` and an explicit `array_filter($a, null)` mean "keep the truthy
/// elements". The omitted spelling arrives with one operand because positional builtin
/// lowering does not materialize defaults; the explicit spelling arrives as a null constant.
fn array_filter_callback_is_absent(ctx: &FunctionContext<'_>, inst: &Instruction) -> Result<bool> {
    let Some(callback) = inst.operands.get(1).copied() else {
        return Ok(true);
    };
    Ok(matches!(
        ctx.value_php_type(callback)?.codegen_repr(),
        PhpType::Void
    ))
}

/// Lowers `array_filter($array)` — no callback — through the shared filter runtime.
///
/// php keeps the elements that are truthy. Rather than growing a second filter loop that could
/// drift from the callback one, this passes an implicit predicate carrying the callback
/// wrapper's own ABI, so the shared filter loop drives it unchanged. Mode is pinned to ARRAY_FILTER_USE_VALUE and the capture environment to zero:
/// there is nothing to capture, and php never hands the key to a predicate it did not receive.
fn lower_array_filter_without_callback(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
) -> Result<()> {
    let elem_ty = array_filter_source_element_type(ctx.value_php_type(array)?)?;
    require_array_filter_result_type(&elem_ty, &inst.result_php_type.codegen_repr())?;
    // php preserves the keys, so the destination is a keyed table either way. The ownership
    // rules live in `__rt_hash_clone_shallow`, which is why the refcounted/plain split the
    // indexed helpers needed does not reappear here.
    let runtime_label = if matches!(
        ctx.value_php_type(array)?.codegen_repr(),
        PhpType::AssocArray { .. }
    ) {
        "__rt_hash_filter"
    } else {
        "__rt_array_filter_keyed"
    };
    let predicate = match elem_ty.codegen_repr() {
        // An empty literal carries a `Never` element type. The filter loop runs zero times, so
        // the predicate is never invoked and its choice cannot be observed — but the runtime
        // still needs an address, and refusing the call would reject php's `array_filter([])`.
        PhpType::Int | PhpType::Bool | PhpType::Void => "__rt_filter_truthy_int",
        PhpType::Float => "__rt_filter_truthy_float",
        PhpType::Str => "__rt_filter_truthy_str",
        other if other.is_refcounted() => "__rt_filter_truthy_mixed",
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_filter without a callback for indexed-array element PHP type {:?}",
                other
            )))
        }
    };
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x0", predicate);
            ctx.load_value_to_reg(array, "x1")?;
            abi::emit_load_int_immediate(ctx.emitter, "x2", 0); // no capture environment
            abi::emit_load_int_immediate(ctx.emitter, "x3", 0); // ARRAY_FILTER_USE_VALUE
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rdi", predicate);
            ctx.load_value_to_reg(array, "rsi")?;
            abi::emit_load_int_immediate(ctx.emitter, "rdx", 0); // no capture environment
            abi::emit_load_int_immediate(ctx.emitter, "rcx", 0); // ARRAY_FILTER_USE_VALUE
        }
    }
    abi::emit_call_label(ctx.emitter, runtime_label);
    store_if_result(ctx, inst)
}
