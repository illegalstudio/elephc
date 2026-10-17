//! Purpose:
//! Array walk, merge, set operations, slice, and splice entry points.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::arrays`.
//!
//! Key details:
//! - Preserves callback ABI, target parity, array storage, and ownership contracts.

use super::*;
use crate::codegen::lower_inst::receiver_place::ReceiverPlace;

/// Lowers `array_walk()` through the callback-driven runtime helper.
pub(crate) fn lower_array_walk(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "array_walk", 2)?;
    let array = expect_operand(inst, 0)?;
    let callback = expect_operand(inst, 1)?;
    if super::boxed_walk::lower_boxed_array_walk(
        ctx,
        inst,
        array,
        callback,
        "array_walk",
        false,
    )? {
        return Ok(());
    }
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

/// Merges boxed PHP arrays through layout dispatch, retaining the concrete indexed fast path.
pub(crate) fn lower_array_merge(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "array_merge", 2)?;
    let first = expect_operand(inst, 0)?;
    let second = expect_operand(inst, 1)?;
    if ctx.value_php_type(first)?.codegen_repr() == PhpType::Mixed
        || ctx.value_php_type(second)?.codegen_repr() == PhpType::Mixed
    {
        return super::boxed_merge::lower_boxed_array_merge(ctx, inst, first, second);
    }
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

/// Lowers `array_diff()`, keeping each survivor under its original key (#1645).
///
/// PHP keeps the keys (`array_diff([1, 2, 3], [2])` is `[0 => 1, 2 => 3]`), so the checker types
/// the result of an int/float/bool/string first operand as a hash and this lowers through
/// `__rt_hash_value_diff_intersect`, converting an indexed operand to a hash keyed `0..n-1`.
/// Element types the value comparison cannot cast (objects, arrays, callables) keep an indexed
/// result and the legacy identity-comparing helpers; the declared result decides which path runs.
pub(crate) fn lower_array_diff(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if super::boxed_set_ops::needs_rendering_scan(ctx, inst)? {
        return super::boxed_set_ops::lower_array_diff_values(ctx, inst);
    }
    if matches!(inst.result_php_type.codegen_repr(), PhpType::AssocArray { .. }) {
        return lower_value_set_op_to_hash(ctx, inst, "array_diff", 0);
    }
    lower_indexed_array_set_op(
        ctx,
        inst,
        "array_diff",
        "__rt_array_diff",
        "__rt_array_diff_refcounted",
    )
}

/// Lowers `array_intersect()`, keeping each survivor under its original key (#1645).
///
/// Same split as [`lower_array_diff`]: a hash result runs the key-preserving value comparison,
/// an indexed result (element types that cannot be cast to string) the legacy helpers.
pub(crate) fn lower_array_intersect(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if super::boxed_set_ops::needs_rendering_scan(ctx, inst)? {
        return super::boxed_set_ops::lower_array_intersect_values(ctx, inst);
    }
    if matches!(inst.result_php_type.codegen_repr(), PhpType::AssocArray { .. }) {
        return lower_value_set_op_to_hash(ctx, inst, "array_intersect", 1);
    }
    lower_indexed_array_set_op(
        ctx,
        inst,
        "array_intersect",
        "__rt_array_intersect",
        "__rt_array_intersect_refcounted",
    )
}

/// Runs `__rt_hash_value_diff_intersect` in `mode` (0 = diff, 1 = intersect) over two operands,
/// converting an indexed one whose values can be compared by string cast to an owned hash first.
fn lower_value_set_op_to_hash(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    mode: i64,
) -> Result<()> {
    super::misc_dispatch::lower_two_hash_arg_builtin_converting(
        ctx,
        inst,
        name,
        "__rt_hash_value_diff_intersect",
        Some(mode),
        value_set_op_element_converts,
    )
}

/// Accepts the indexed element types whose values `__rt_hash_value_diff_intersect` compares by
/// string cast: scalars, strings, and the element type of an empty literal.
pub(crate) fn value_set_op_element_converts(elem: &PhpType) -> bool {
    matches!(
        elem,
        PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Str | PhpType::Void | PhpType::Never
    )
}

/// Accepts every indexed element type for a KEY set operation: keys never compare the values,
/// and `__rt_array_to_hash` persists strings and retains heap values when it converts.
fn key_set_op_element_converts(_elem: &PhpType) -> bool {
    true
}

/// Lowers `array_diff_key()`, converting an indexed operand to a hash keyed `0..n-1` so an indexed
/// first argument keeps its surviving keys (#1645).
pub(crate) fn lower_array_diff_key(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::misc_dispatch::lower_two_hash_arg_builtin_converting(
        ctx,
        inst,
        "array_diff_key",
        "__rt_array_diff_key",
        None,
        key_set_op_element_converts,
    )
}

/// Lowers `array_intersect_key()`, converting an indexed operand to a hash keyed `0..n-1` so an
/// indexed first argument keeps its surviving keys (#1645).
pub(crate) fn lower_array_intersect_key(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::misc_dispatch::lower_two_hash_arg_builtin_converting(
        ctx,
        inst,
        "array_intersect_key",
        "__rt_array_intersect_key",
        None,
        key_set_op_element_converts,
    )
}

/// Lowers `array_slice()` for indexed arrays with pointer-sized payload slots.
///
/// PHP's `bool $preserve_keys = false` renumbers the selected window from zero; a literal `true`
/// keeps the source integer keys instead. A dense indexed array cannot hold a window that does not
/// start at key 0, so the key-preserving form lowers to `__rt_array_slice_to_hash`, which builds an
/// owned hash. For concrete storage the checker guarantees the flag is a literal (it decides the
/// result's static shape), so a non-literal operand can only mean the checker and the backend
/// disagree. A boxed source is dispatched first: its result is the boxed PHP array whatever the
/// flag says, so it reads the flag at runtime (see `lower_mixed_array_slice`).
pub(crate) fn lower_array_slice(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "array_slice", 2, 4)?;
    let array = expect_operand(inst, 0)?;
    if matches!(
        ctx.value_php_type(array)?.codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        return lower_mixed_array_slice(ctx, inst);
    }
    let preserve_keys = slice_like_preserve_keys(ctx, inst, "array_slice")?;
    if matches!(
        ctx.value_php_type(array)?.codegen_repr(),
        PhpType::AssocArray { .. }
    ) {
        return lower_hash_slice(ctx, inst, array, preserve_keys);
    }
    if preserve_keys {
        return lower_array_slice_preserve_keys(ctx, inst, array);
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

/// Lowers `array_slice()` over an ASSOCIATIVE receiver, in either `preserve_keys` mode.
///
/// `$offset`/`$length` count positions in insertion order, so the window cannot be addressed by
/// key and `__rt_hash_slice` walks the source instead. One helper serves both modes because they
/// differ by a single per-entry decision: php-src renumbers INTEGER keys when `preserve_keys` is
/// false and leaves string keys alone either way, so the result's key type is the source's in
/// both modes — which is exactly what the checker records.
///
/// Both modes were an explicit `unsupported` diagnostic until issue #683, one from this
/// function's key-preserving sibling and one from `array_slice_source_element_type`.
fn lower_hash_slice(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
    preserve_keys: bool,
) -> Result<()> {
    let PhpType::AssocArray { .. } = inst.result_php_type.codegen_repr() else {
        return Err(CodegenIrError::unsupported(format!(
            "array_slice of an associative array into result PHP type {:?}",
            inst.result_php_type
        )));
    };
    let offset = expect_operand(inst, 1)?;
    let length = slice_like_length_operand(inst)?;
    lower_slice_like_args(ctx, array, offset, length, "array_slice")?;
    // The window arguments occupy the first four registers; the mode flag rides in the fifth.
    match ctx.emitter.target.arch {
        Arch::AArch64 => abi::emit_load_int_immediate(
            ctx.emitter,
            "x4",
            i64::from(preserve_keys),
        ),
        Arch::X86_64 => abi::emit_load_int_immediate(
            ctx.emitter,
            "r8",
            i64::from(preserve_keys),
        ),
    }
    abi::emit_call_label(ctx.emitter, "__rt_hash_slice");
    store_if_result(ctx, inst)
}

/// Lowers `array_slice()` for an indexed or hash array stored inside a boxed Mixed cell.
///
/// A boxed result (the PHP array type the checker, the fallback, and the callable wrapper give a
/// boxed source) receives whichever storage the slice built, boxed by its runtime heap kind. An
/// `array<mixed>` result only comes from a checked list type whose operand EIR still boxes, such
/// as a call-site-specialized untyped parameter; a hash payload there stays a hash-backed array,
/// which boxing recognizes by its heap kind.
///
/// `$preserve_keys` is read at runtime. The callable wrapper forwards whatever its caller passed,
/// and `call_user_func_array($f, $args)` hides that argument from the checker, so the flag is
/// staged on the temporary stack before the payload dispatch and both payload shapes honor it: a
/// hash hands it to `__rt_hash_slice`, and a list keeps its keys through
/// `__rt_array_slice_to_hash`. That hash can only be carried by a boxed result, so a flag that may
/// be set is refused for any other result type.
pub(super) fn lower_mixed_array_slice(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let array = expect_operand(inst, 0)?;
    let offset = expect_operand(inst, 1)?;
    let length = slice_like_length_operand(inst)?;
    let preserve_keys = inst.operands.get(3).copied();
    let result_ty = inst.result_php_type.codegen_repr();
    let boxed_result = matches!(result_ty, PhpType::Mixed | PhpType::Union(_));
    let result_elem_ty = if boxed_result {
        PhpType::Mixed
    } else {
        result_array_element_type("array_slice", &result_ty)?
    };
    require_array_slice_result_type(&PhpType::Mixed, &result_elem_ty)?;
    let keys_may_be_kept = slice_preserve_keys_may_be_set(ctx, preserve_keys)?;
    if keys_may_be_kept && !boxed_result {
        return Err(CodegenIrError::unsupported(format!(
            "array_slice preserve_keys of a boxed source into result PHP type {:?}",
            inst.result_php_type
        )));
    }
    stage_slice_preserve_keys_flag(ctx, preserve_keys)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_mixed_array_slice_aarch64(
            ctx,
            array,
            offset,
            length,
            &result_elem_ty,
            keys_may_be_kept,
        )?,
        Arch::X86_64 => lower_mixed_array_slice_x86_64(
            ctx,
            array,
            offset,
            length,
            &result_elem_ty,
            keys_may_be_kept,
        )?,
    }
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    if boxed_result {
        emit_box_current_owned_value_as_mixed(
            ctx.emitter,
            &PhpType::Array(Box::new(PhpType::Mixed)),
        );
    }
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
    super::boxed_mutation::prepare_boxed_array_receiver(ctx, array, "array_splice")?;
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
