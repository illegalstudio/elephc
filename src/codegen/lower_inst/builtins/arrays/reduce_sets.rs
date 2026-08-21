//! Purpose:
//! Array reduce, walk, merge, set operations, slice, and splice entry points.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::arrays`.
//!
//! Key details:
//! - Preserves callback ABI, target parity, array storage, and ownership contracts.

use super::*;
use crate::codegen::lower_inst::receiver_place::ReceiverPlace;

/// Lowers `array_reduce()` through the callback-driven runtime helper.
pub(crate) fn lower_array_reduce(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "array_reduce", 3)?;
    let array = expect_operand(inst, 0)?;
    let callback = expect_operand(inst, 1)?;
    let initial = expect_operand(inst, 2)?;
    let elem_ty = array_reduce_callback_array_element_type(ctx.value_php_type(array)?)?;
    let initial_ty =
        eight_byte_callback_value_type(ctx.value_php_type(initial)?, "array_reduce initial")?;
    let reduce_helper = array_reduce_runtime_label(&elem_ty);
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
                    abi::emit_call_label(ctx.emitter, reduce_helper);
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
                super::super::instruction_strict_php_profile(inst),
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
                    abi::emit_call_label(ctx.emitter, reduce_helper);
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
    abi::emit_call_label(ctx.emitter, reduce_helper);
    if env_bytes != 0 {
        abi::emit_release_temporary_stack(ctx.emitter, env_bytes);
    }
    box_int_result_for_mixed_builtin(ctx, inst);
    store_if_result(ctx, inst)
}

/// Lowers `array_walk()` through the callback-driven runtime helper.
pub(crate) fn lower_array_walk(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "array_walk", 2)?;
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
                super::super::super::instruction_strict_php_profile(inst),
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
pub(crate) fn lower_array_merge(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "array_merge", 2)?;
    let first = expect_operand(inst, 0)?;
    let second = expect_operand(inst, 1)?;
    // STRING arrays take the 16-byte-slot helper. An EMPTY literal (`Never`/`Void` element)
    // rides along: its length is zero, so the copy loop never reads a slot and its declared
    // slot size is moot. The shared 8-byte gate keeps refusing a string array beside any
    // non-empty other layout, where a merged container would need one common slot shape.
    let first_ty = ctx.value_php_type(first)?;
    let second_ty = ctx.value_php_type(second)?;
    let str_side = |ty: &PhpType| {
        matches!(ty.codegen_repr(), PhpType::Array(elem) if elem.codegen_repr() == PhpType::Str)
    };
    let empty_side = |ty: &PhpType| {
        matches!(
            ty.codegen_repr(),
            PhpType::Array(elem) if matches!(elem.codegen_repr(), PhpType::Never | PhpType::Void)
        )
    };
    let str_merge = (str_side(&first_ty) || str_side(&second_ty))
        && (str_side(&first_ty) || empty_side(&first_ty))
        && (str_side(&second_ty) || empty_side(&second_ty));
    let elem_ty = if str_merge {
        PhpType::Str
    } else {
        compatible_eight_byte_indexed_array_element_type(first_ty, second_ty, "array_merge")?
    };
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
    // The helper allocates its result through the shared constructor, which leaves the
    // value_type lane empty. Unstamped, every reader treats the merged slots as raw words:
    // merging two heterogeneous arrays produced the right COUNT and printed ADDRESSES.
    crate::codegen::emit_array_value_type_stamp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        &elem_ty,
    );
    store_if_result(ctx, inst)
}

/// Lowers `array_diff()` into the key-preserving hash php returns.
pub(crate) fn lower_array_diff(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_indexed_array_set_op(ctx, inst, "array_diff", "__rt_array_diff_to_hash")
}

/// Lowers `array_intersect()` into the key-preserving hash php returns.
pub(crate) fn lower_array_intersect(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_indexed_array_set_op(ctx, inst, "array_intersect", "__rt_array_intersect_to_hash")
}

/// Lowers `array_diff_key()` for two associative arrays by filtering first-operand keys.
pub(crate) fn lower_array_diff_key(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_assoc_array_key_set_op(ctx, inst, "array_diff_key", "__rt_array_diff_key")
}

/// Lowers `array_intersect_key()` for two associative arrays by keeping shared first-operand keys.
pub(crate) fn lower_array_intersect_key(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_assoc_array_key_set_op(ctx, inst, "array_intersect_key", "__rt_array_intersect_key")
}

/// Lowers `array_slice()` for indexed arrays with pointer-sized payload slots.
///
/// PHP's `bool $preserve_keys = false` keeps the source integer keys of the selected window
/// instead of renumbering it from zero. A dense indexed array cannot hold a window that does not
/// start at key 0, so the key-preserving form lowers to `__rt_array_slice_to_hash`, which builds an
/// owned hash. The checker guarantees the flag is a literal (it decides the result's static
/// shape), so a non-literal operand can only mean the checker and the backend disagree.
/// Lowers `array_slice()` for indexed arrays with pointer-sized payload slots.
///
/// PHP's `bool $preserve_keys = false` keeps the source integer keys of the selected window
/// instead of renumbering it from zero. A dense indexed array cannot hold a window that does not
/// start at key 0, so the key-preserving form lowers to `__rt_array_slice_to_hash`, which builds an
/// owned hash. The checker guarantees the flag is a literal (it decides the result's static
/// shape), so a non-literal operand can only mean the checker and the backend disagree.
pub(crate) fn lower_array_slice(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "array_slice", 2, 4)?;
    let array = expect_operand(inst, 0)?;
    if slice_like_preserve_keys(ctx, inst, "array_slice")? {
        return lower_array_slice_preserve_keys(ctx, inst, array);
    }
    if matches!(
        ctx.value_php_type(array)?.codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        return lower_mixed_array_slice(ctx, inst);
    }
    let offset = expect_operand(inst, 1)?;
    let length = slice_like_length_operand(inst)?;
    let source_elem_ty = array_slice_source_element_type(ctx.value_php_type(array)?)?;
    let result_elem_ty =
        result_array_element_type("array_slice", &inst.result_php_type.codegen_repr())?;
    require_array_slice_result_type(&source_elem_ty, &result_elem_ty)?;
    lower_array_slice_call(ctx, array, offset, length, &source_elem_ty)?;
    normalize_indexed_array_result(ctx, "array_slice", &source_elem_ty, &result_elem_ty)?;
    store_if_result(ctx, inst)
}

/// Lowers `array_slice()` for an indexed array stored inside a boxed Mixed cell.
pub(super) fn lower_mixed_array_slice(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let array = expect_operand(inst, 0)?;
    let offset = expect_operand(inst, 1)?;
    let length = slice_like_length_operand(inst)?;
    let result_elem_ty =
        result_array_element_type("array_slice", &inst.result_php_type.codegen_repr())?;
    require_array_slice_result_type(&PhpType::Mixed, &result_elem_ty)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_mixed_array_slice_aarch64(ctx, array, offset, length)?,
        Arch::X86_64 => lower_mixed_array_slice_x86_64(ctx, array, offset, length)?,
    }
    normalize_indexed_array_result(ctx, "array_slice", &PhpType::Mixed, &result_elem_ty)?;
    store_if_result(ctx, inst)
}

/// Lowers `array_splice()` by mutating an indexed source array and returning removed elements.
///
/// PHP's optional `$replacement` is written into the gap the removal opened, which can make the
/// source array longer than it was. `__rt_array_splice_insert*` grows the payload for that, and a
/// growth relocates the array, so the by-reference receiver is written back a second time after
/// the insertion rather than only after the copy-on-write split.
pub(crate) fn lower_array_splice(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "array_splice", 2, 4)?;
    let array = expect_operand(inst, 0)?;
    if matches!(
        ctx.value_php_type(array)?.codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        return lower_mixed_array_splice(ctx, inst);
    }
    let offset = expect_operand(inst, 1)?;
    let length = inst.operands.get(2).copied();
    let elem_ty = array_pop_element_type(ctx.value_php_type(array)?)?;
    let replacement =
        SpliceReplacement::resolve(ctx, inst.operands.get(3).copied(), &elem_ty)?;
    let receiver_ty = ctx.value_php_type(array)?;
    let receiver = ReceiverPlace::resolve(ctx, array)?;
    ensure_unique_array_pop_source(ctx, array)?;
    receiver.store_back(ctx, array, &receiver_ty)?;
    lower_array_splice_call(ctx, array, offset, length, &elem_ty)?;
    emit_splice_replacement_insert(ctx, array, receiver, &receiver_ty, &replacement, &elem_ty)?;
    normalize_array_splice_result(ctx, &elem_ty, &inst.result_php_type.codegen_repr())?;
    store_if_result(ctx, inst)
}

/// Lowers `array_splice()` for an indexed array stored inside a boxed Mixed cell.
pub(super) fn lower_mixed_array_splice(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let array = expect_operand(inst, 0)?;
    let offset = expect_operand(inst, 1)?;
    let length = inst.operands.get(2).copied();
    let replacement =
        SpliceReplacement::resolve(ctx, inst.operands.get(3).copied(), &PhpType::Mixed)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            lower_mixed_array_splice_aarch64(ctx, array, offset, length, &replacement)?
        }
        Arch::X86_64 => {
            lower_mixed_array_splice_x86_64(ctx, array, offset, length, &replacement)?
        }
    }
    normalize_array_splice_result(ctx, &PhpType::Mixed, &inst.result_php_type.codegen_repr())?;
    store_if_result(ctx, inst)
}
