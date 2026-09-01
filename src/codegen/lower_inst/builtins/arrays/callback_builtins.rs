//! Purpose:
//! Predicate, recursive walk, comparator, search, and membership callbacks.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::arrays`.
//!
//! Key details:
//! - Preserves callback ABI, target parity, array storage, and ownership contracts.

use super::*;
use crate::codegen::lower_inst::receiver_place::ReceiverPlace;

/// Returns the scalar callback element type for an indexed-array predicate/comparator builtin.
///
/// The `__rt_array_find_any_all` / `__rt_array_udiff_uintersect` runtimes load each element as a
/// single 8-byte word and pass it in an integer argument register, so only `int`/`bool` indexed
/// arrays are supported (float elements would need the float register file).
pub(super) fn predicate_callback_element_type(ty: PhpType, name: &str) -> Result<PhpType> {
    match ty.codegen_repr() {
        PhpType::Array(elem) => {
            let elem = elem.codegen_repr();
            if matches!(elem, PhpType::Int | PhpType::Bool) {
                Ok(elem)
            } else {
                Err(CodegenIrError::unsupported(format!(
                    "{} indexed-array element PHP type {:?}",
                    name, elem
                )))
            }
        }
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            name, other
        ))),
    }
}

/// Loads the `(wrapper, array, env[, mode])` argument registers and calls a single-array callback
/// runtime helper. Shared by `array_find`/`array_any`/`array_all` (mode 0/1/2) and
/// `array_walk_recursive` (no mode). The callback wrapper goes in arg0, the array in arg1, the
/// environment pointer in arg2, and the optional mode selector in arg3.
pub(super) fn emit_single_array_callback_call(
    ctx: &mut FunctionContext<'_>,
    wrapper_label: &str,
    array: ValueId,
    env_bytes: usize,
    mode: Option<i64>,
    runtime_label: &str,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x0", wrapper_label);
            ctx.load_value_to_reg(array, "x1")?;
            load_static_callback_env_arg(ctx, "x2", env_bytes);
            if let Some(m) = mode {
                abi::emit_load_int_immediate(ctx.emitter, "x3", m);
            }
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rdi", wrapper_label);
            ctx.load_value_to_reg(array, "rsi")?;
            load_static_callback_env_arg(ctx, "rdx", env_bytes);
            if let Some(m) = mode {
                abi::emit_load_int_immediate(ctx.emitter, "rcx", m);
            }
        }
    }
    abi::emit_call_label(ctx.emitter, runtime_label);
    Ok(())
}

/// Lowers a single-array callback builtin through the EIR callback machinery, dispatching on the
/// callback's closure/first-class-callable, runtime-string, or static form (mirrors `array_filter`).
pub(super) fn lower_single_array_callback_builtin(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    runtime_label: &str,
    array: ValueId,
    callback: ValueId,
    source_arg_ty: &PhpType,
    visible_arg_types: Vec<PhpType>,
    return_ty: PhpType,
    mode: Option<i64>,
) -> Result<()> {
    match ctx.value_php_type(callback)?.codegen_repr() {
        PhpType::Callable => {
            lower_descriptor_callback_runtime(
                ctx,
                callback,
                visible_arg_types,
                return_ty,
                |ctx, wrapper_label, env_bytes| {
                    emit_single_array_callback_call(
                        ctx,
                        wrapper_label,
                        array,
                        env_bytes,
                        mode,
                        runtime_label,
                    )
                },
            )?;
            store_if_result(ctx, inst)
        }
        PhpType::Str => {
            lower_runtime_string_descriptor_callback(
                ctx,
                callback,
                Some(source_arg_ty),
                visible_arg_types,
                return_ty,
                super::super::super::instruction_strict_php_profile(inst),
                name,
                |ctx, wrapper_label, env_bytes| {
                    emit_single_array_callback_call(
                        ctx,
                        wrapper_label,
                        array,
                        env_bytes,
                        mode,
                        runtime_label,
                    )
                },
            )?;
            store_if_result(ctx, inst)
        }
        _ => {
            let binding = static_sort_callback_binding(
                ctx,
                callback,
                &format!("{} callback", name),
                Some(&visible_arg_types),
            )?;
            let env_bytes = reserve_static_callback_env(ctx, binding.env_source)?;
            emit_single_array_callback_call(
                ctx,
                &binding.label,
                array,
                env_bytes,
                mode,
                runtime_label,
            )?;
            if env_bytes != 0 {
                abi::emit_release_temporary_stack(ctx.emitter, env_bytes);
            }
            store_if_result(ctx, inst)
        }
    }
}

/// Lowers a predicate builtin (`array_find` mode 0 / `array_any` mode 1 / `array_all` mode 2)
/// over an indexed scalar array, validating the element type and routing through the shared
/// `__rt_array_find_any_all` runtime. The predicate callback always returns `bool`; the builtin's
/// own result type (Mixed for `array_find`, bool for any/all) is taken from `inst.result_php_type`.
pub(super) fn lower_array_predicate_builtin(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    mode: i64,
) -> Result<()> {
    super::super::ensure_arg_count(inst, name, 2)?;
    let array = expect_operand(inst, 0)?;
    let callback = expect_operand(inst, 1)?;
    let elem_ty = predicate_callback_element_type(ctx.value_php_type(array)?, name)?;
    let source_arg_ty = PhpType::Array(Box::new(elem_ty.clone()));
    lower_single_array_callback_builtin(
        ctx,
        inst,
        name,
        "__rt_array_find_any_all",
        array,
        callback,
        &source_arg_ty,
        vec![elem_ty],
        PhpType::Bool,
        Some(mode),
    )
}

/// Lowers `array_find()`: returns the first element satisfying the predicate, boxed as Mixed (or null).
pub(crate) fn lower_array_find(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_array_predicate_builtin(ctx, inst, "array_find", 0)
}

/// Lowers `array_any()`: returns true when some element satisfies the predicate.
pub(crate) fn lower_array_any(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_array_predicate_builtin(ctx, inst, "array_any", 1)
}

/// Lowers `array_all()`: returns true when every element satisfies the predicate.
pub(crate) fn lower_array_all(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_array_predicate_builtin(ctx, inst, "array_all", 2)
}

/// Lowers `array_walk_recursive()`: invokes the callback on each scalar leaf of a (possibly nested)
/// array, descending into array-valued elements. Returns void; leaves are passed as 8-byte scalars.
pub(crate) fn lower_array_walk_recursive(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "array_walk_recursive", 2)?;
    let array = expect_operand(inst, 0)?;
    let callback = expect_operand(inst, 1)?;
    require_array_like_operand(ctx.value_php_type(array)?, "array_walk_recursive")?;
    let source_arg_ty = PhpType::Array(Box::new(PhpType::Int));
    lower_single_array_callback_builtin(
        ctx,
        inst,
        "array_walk_recursive",
        "__rt_array_walk_recursive",
        array,
        callback,
        &source_arg_ty,
        vec![PhpType::Int],
        PhpType::Void,
        None,
    )
}

/// Loads the `(wrapper, arr1, arr2, env, mode)` argument registers and calls the two-array
/// comparator runtime helper `__rt_array_udiff_uintersect`. The comparator wrapper goes in arg0,
/// the two arrays in arg1/arg2, the environment pointer in arg3, and the mode selector in arg4.
pub(super) fn emit_two_array_comparator_call(
    ctx: &mut FunctionContext<'_>,
    wrapper_label: &str,
    arr1: ValueId,
    arr2: ValueId,
    env_bytes: usize,
    mode: i64,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x0", wrapper_label);
            ctx.load_value_to_reg(arr1, "x1")?;
            ctx.load_value_to_reg(arr2, "x2")?;
            load_static_callback_env_arg(ctx, "x3", env_bytes);
            abi::emit_load_int_immediate(ctx.emitter, "x4", mode);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rdi", wrapper_label);
            ctx.load_value_to_reg(arr1, "rsi")?;
            ctx.load_value_to_reg(arr2, "rdx")?;
            load_static_callback_env_arg(ctx, "rcx", env_bytes);
            abi::emit_load_int_immediate(ctx.emitter, "r8", mode);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_udiff_uintersect");
    Ok(())
}

/// Lowers a two-array comparator builtin (`array_udiff` mode 0 / `array_uintersect` mode 1) over
/// indexed scalar arrays, dispatching on the comparator's closure/string/static form. The result is
/// a sequentially re-indexed array of the first array's surviving elements.
pub(super) fn lower_two_array_comparator_builtin(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    mode: i64,
) -> Result<()> {
    super::super::ensure_arg_count(inst, name, 3)?;
    let arr1 = expect_operand(inst, 0)?;
    let arr2 = expect_operand(inst, 1)?;
    let comparator = expect_operand(inst, 2)?;
    let elem_ty = predicate_callback_element_type(ctx.value_php_type(arr1)?, name)?;
    predicate_callback_element_type(ctx.value_php_type(arr2)?, name)?;
    // The comparator returns an int (negative/zero/positive); the builtin's own array result type
    // is taken from `inst.result_php_type` at `store_if_result`.
    let comparator_return_ty = PhpType::Int;
    let source_arg_ty = PhpType::Array(Box::new(elem_ty.clone()));
    let visible_arg_types = vec![elem_ty.clone(), elem_ty];
    match ctx.value_php_type(comparator)?.codegen_repr() {
        PhpType::Callable => {
            lower_descriptor_callback_runtime(
                ctx,
                comparator,
                visible_arg_types,
                comparator_return_ty,
                |ctx, wrapper_label, env_bytes| {
                    emit_two_array_comparator_call(ctx, wrapper_label, arr1, arr2, env_bytes, mode)
                },
            )?;
            store_if_result(ctx, inst)
        }
        PhpType::Str => {
            lower_runtime_string_descriptor_callback(
                ctx,
                comparator,
                Some(&source_arg_ty),
                visible_arg_types,
                comparator_return_ty,
                super::super::super::instruction_strict_php_profile(inst),
                name,
                |ctx, wrapper_label, env_bytes| {
                    emit_two_array_comparator_call(ctx, wrapper_label, arr1, arr2, env_bytes, mode)
                },
            )?;
            store_if_result(ctx, inst)
        }
        _ => {
            let binding = static_sort_callback_binding(
                ctx,
                comparator,
                &format!("{} comparator", name),
                Some(&visible_arg_types),
            )?;
            let env_bytes = reserve_static_callback_env(ctx, binding.env_source)?;
            emit_two_array_comparator_call(ctx, &binding.label, arr1, arr2, env_bytes, mode)?;
            if env_bytes != 0 {
                abi::emit_release_temporary_stack(ctx.emitter, env_bytes);
            }
            store_if_result(ctx, inst)
        }
    }
}

/// Lowers `array_udiff()`: keeps first-array elements not equal (per comparator) to any second-array element.
pub(crate) fn lower_array_udiff(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_two_array_comparator_builtin(ctx, inst, "array_udiff", 0)
}

/// Lowers `array_uintersect()`: keeps first-array elements equal (per comparator) to some second-array element.
pub(crate) fn lower_array_uintersect(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_two_array_comparator_builtin(ctx, inst, "array_uintersect", 1)
}

/// Lowers `array_multisort()`: stable-sorts the first indexed array ascending and reorders the second
/// in tandem, both in place. Both arguments are by-reference, so each is copy-on-write split with
/// `ensure_unique_sort_source` and the (possibly relocated) pointer is written back to its local
/// before the runtime mutates the storage. Returns `true`. Supports 8-byte scalar indexed arrays.
pub(crate) fn lower_array_multisort(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "array_multisort", 2)?;
    let arr1 = expect_operand(inst, 0)?;
    let arr2 = expect_operand(inst, 1)?;
    eight_byte_indexed_array_element_type(ctx.value_php_type(arr1)?, "array_multisort")?;
    eight_byte_indexed_array_element_type(ctx.value_php_type(arr2)?, "array_multisort")?;

    // -- copy-on-write split both by-ref arrays and publish the new pointers to their locals --
    let receiver1 = ReceiverPlace::resolve(ctx, arr1)?;
    ensure_unique_sort_source(ctx, arr1)?;
    receiver1.store_back_value(ctx, arr1)?;
    let receiver2 = ReceiverPlace::resolve(ctx, arr2)?;
    ensure_unique_sort_source(ctx, arr2)?;
    receiver2.store_back_value(ctx, arr2)?;

    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(arr1, "x0")?;
            ctx.load_value_to_reg(arr2, "x1")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(arr1, "rdi")?;
            ctx.load_value_to_reg(arr2, "rsi")?;
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_multisort");
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        0x7fff_ffff_ffff_fffe,
    );
    store_if_result(ctx, inst)
}

/// Lowers `array_search()` for indexed arrays with integer-like payloads.
///
/// PHP's third parameter (`bool $strict = false`) selects `===` instead of `==`. Every
/// comparison this emitter can already lower is value-exact, so the two modes only diverge
/// when the needle and the element type are statically different scalar types — the case
/// `array_search_strict_never_matches()` detects. There, the strict answer is unconditionally
/// `false`, so the flag is resolved with a runtime branch around the ordinary search.
pub(crate) fn lower_array_search(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "array_search", 2, 3)?;
    let needle = expect_operand(inst, 0)?;
    let array = expect_operand(inst, 1)?;
    let needle_ty = ctx.value_php_type(needle)?;
    let array_ty = ctx.value_php_type(array)?;
    let strict = inst.operands.get(2).copied();
    match strict {
        Some(strict) if array_search_strict_never_matches(&needle_ty, &array_ty) => {
            let strict_label = ctx.next_label("array_search_strict");
            let done_label = ctx.next_label("array_search_strict_done");
            branch_if_bool_value_true(ctx, strict, &strict_label)?;
            lower_array_search_loose(ctx, needle, array, needle_ty, array_ty)?;
            abi::emit_jump(ctx.emitter, &done_label);
            ctx.emitter.label(&strict_label);
            box_array_search_miss(ctx);
            ctx.emitter.label(&done_label);
        }
        // Either `strict` was omitted, or the needle and element types agree (or one side is
        // a boxed `Mixed` compared tag-exactly), in which case `===` and `==` pick the same
        // element and the flag has no observable effect on the emitted search.
        _ => lower_array_search_loose(ctx, needle, array, needle_ty, array_ty)?,
    }
    store_if_result(ctx, inst)
}

/// Lowers `in_array()` for indexed and associative arrays with PHP loose or strict membership.
pub(crate) fn lower_in_array(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "in_array", 2, 3)?;
    let needle = expect_operand(inst, 0)?;
    let array = expect_operand(inst, 1)?;
    let needle_ty = ctx.value_php_type(needle)?;
    let array_ty = ctx.value_php_type(array)?;
    let Some(strict) = inst.operands.get(2).copied() else {
        lower_in_array_with_mode(ctx, needle, array, needle_ty, array_ty, InArrayMode::Loose)?;
        store_if_result(ctx, inst)?;
        return Ok(());
    };

    let strict_label = ctx.next_label("in_array_strict");
    let done_label = ctx.next_label("in_array_done");
    branch_if_bool_value_true(ctx, strict, &strict_label)?;
    lower_in_array_with_mode(
        ctx,
        needle,
        array,
        needle_ty.clone(),
        array_ty.clone(),
        InArrayMode::Loose,
    )?;
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&strict_label);
    lower_in_array_with_mode(ctx, needle, array, needle_ty, array_ty, InArrayMode::Strict)?;
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Lowers `in_array()` using either PHP loose (`==`) or strict (`===`) comparison rules.
pub(super) fn lower_in_array_with_mode(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
    needle_ty: PhpType,
    array_ty: PhpType,
    mode: InArrayMode,
) -> Result<()> {
    if search::try_lower_assoc_in_array(
        ctx,
        needle,
        array,
        needle_ty.clone(),
        array_ty.clone(),
        mode,
    )? {
        return Ok(());
    }
    match supported_in_array_case(needle_ty, array_ty, mode)? {
        InArrayCase::Empty => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
        InArrayCase::AlwaysFalse => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
        InArrayCase::ScalarExact => lower_in_array_scalar(ctx, needle, array)?,
        InArrayCase::ScalarTruthy => lower_in_array_scalar_truthy(ctx, needle, array)?,
        InArrayCase::StringExact => lower_in_array_string(ctx, needle, array, "__rt_str_eq")?,
        InArrayCase::StringLoose => lower_in_array_string(ctx, needle, array, "__rt_str_loose_eq")?,
        InArrayCase::StringNeedleIntArray => {
            lower_in_array_string_needle_int_array(ctx, needle, array)?
        }
        InArrayCase::IntNeedleStringArray => {
            lower_in_array_int_needle_string_array(ctx, needle, array)?
        }
        InArrayCase::StringNeedleBoolArray => {
            lower_in_array_string_needle_bool_array(ctx, needle, array)?
        }
        InArrayCase::BoolNeedleStringArray => {
            lower_in_array_bool_needle_string_array(ctx, needle, array)?
        }
        InArrayCase::MixedNeedleStringExact => {
            lower_in_array_mixed_needle_string_array(ctx, needle, array, "__rt_str_eq")?
        }
        InArrayCase::MixedNeedleStringLoose => {
            lower_in_array_mixed_needle_string_array(ctx, needle, array, "__rt_str_loose_eq")?
        }
        InArrayCase::MixedIntExact => lower_in_array_mixed_int(ctx, needle, array, true)?,
        InArrayCase::MixedIntLoose => lower_in_array_mixed_int(ctx, needle, array, false)?,
        InArrayCase::MixedStringExact => {
            lower_in_array_mixed_string(ctx, needle, array, "__rt_str_eq")?
        }
        InArrayCase::MixedStringLoose => {
            lower_in_array_mixed_string(ctx, needle, array, "__rt_str_loose_eq")?
        }
        InArrayCase::MixedMixedExact => {
            lower_in_array_mixed_mixed(ctx, needle, array, InArrayMode::Strict)?
        }
        InArrayCase::MixedMixedLoose => {
            lower_in_array_mixed_mixed(ctx, needle, array, InArrayMode::Loose)?
        }
    }
    Ok(())
}
