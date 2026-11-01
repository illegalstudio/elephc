//! Purpose:
//! Array aggregate, fill, combine, flip, reverse, and unique builtins.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::arrays`.
//!
//! Key details:
//! - Preserves callback ABI, target parity, array storage, and ownership contracts.

use super::*;

/// Rejects `call_user_func*` calls that escaped the dedicated EIR callback lowering path.
pub(crate) fn lower_call_user_func_builtin_escape(
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

/// Lowers sum through the storage-neutral numeric aggregate helper.
pub(crate) fn lower_array_sum(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::boxed_aggregate::lower_aggregate(ctx, inst, false)
}

/// Lowers product through the storage-neutral numeric aggregate helper.
pub(crate) fn lower_array_product(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::boxed_aggregate::lower_aggregate(ctx, inst, true)
}

/// Lowers `array_chunk()` by splitting an indexed array into nested indexed arrays.
///
/// PHP's `bool $preserve_keys = false` renumbers each chunk from zero; a literal `true` keeps the
/// chunk's source integer keys instead. A dense indexed array cannot hold a window that does not
/// start at key 0, so the key-preserving form lowers to `__rt_array_chunk_to_hash`, which builds
/// one owned hash per chunk. The checker guarantees the flag is a literal (it decides the
/// result's static shape), so a non-literal operand can only mean the checker and the backend
/// disagree.
pub(crate) fn lower_array_chunk(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "array_chunk", 2, 3)?;
    let array = expect_operand(inst, 0)?;
    let length = expect_operand(inst, 1)?;
    let flag = inst.operands.get(2).copied();
    let preserve_keys = match flag {
        None => Some(false),
        Some(flag) => const_bool_operand(ctx, flag)?,
    };
    let Some(preserve_keys) = preserve_keys else {
        return lower_array_chunk_dynamic_preserve_keys(
            ctx,
            inst,
            array,
            length,
            flag.expect("a non-literal flag came from an operand"),
        );
    };
    let result_elem_ty =
        result_array_element_type("array_chunk", &inst.result_php_type.codegen_repr())?;
    emit_array_chunk_arm(ctx, array, length, preserve_keys, &result_elem_ty)?;
    store_if_result(ctx, inst)
}

/// Emits one `array_chunk()` arm, leaving the stamped result in the result register.
///
/// `result_elem_ty` is the CHUNK type this arm produces, which is the result array's element type.
/// Taking it as an argument rather than reading `inst.result_php_type` is what lets the runtime-flag
/// path below emit both arms: there the instruction's own type is the union of the two.
fn emit_array_chunk_arm(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    length: ValueId,
    preserve_keys: bool,
    result_elem_ty: &PhpType,
) -> Result<()> {
    // An associative receiver walks its own entries rather than indexing a dense payload, so it
    // takes the hash helper for BOTH modes; the flag only picks which key each entry lands under.
    if matches!(
        ctx.value_php_type(array)?.codegen_repr(),
        PhpType::AssocArray { .. }
    ) {
        lower_hash_chunk_call(ctx, array, length, preserve_keys)?;
        crate::codegen::emit_array_value_type_stamp(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            result_elem_ty,
        );
        return Ok(());
    }
    let source_elem_ty = array_chunk_source_element_type(ctx.value_php_type(array)?)?;
    let result_inner_elem_ty = if preserve_keys {
        array_chunk_result_inner_hash_value_type(result_elem_ty)?
    } else {
        array_chunk_result_inner_element_type(result_elem_ty)?
    };
    require_array_chunk_result_type(&source_elem_ty, &result_inner_elem_ty)?;
    let runtime_label = if preserve_keys {
        "__rt_array_chunk_to_hash"
    } else {
        array_chunk_runtime_helper(&source_elem_ty)
    };
    lower_array_chunk_call(ctx, array, length, runtime_label)?;
    crate::codegen::emit_array_value_type_stamp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        result_elem_ty,
    );
    Ok(())
}

/// Lowers `array_chunk($array, $size, $flag)` when `$flag` is only known at run time.
///
/// The checker has already restricted this to an INDEXED source, whose chunks are dense arrays one
/// way and integer-keyed hashes the other. Both are emitted and boxed as Mixed, because they are
/// different representations and the call's result type is their union.
fn lower_array_chunk_dynamic_preserve_keys(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
    length: ValueId,
    flag: ValueId,
) -> Result<()> {
    let false_case = ctx.next_label("array_chunk_preserve_false");
    let done = ctx.next_label("array_chunk_preserve_done");

    let source_ty = ctx.value_php_type(array)?.codegen_repr();
    let PhpType::Array(elem) = source_ty else {
        return Err(CodegenIrError::unsupported(format!(
            "array_chunk with a runtime preserve_keys flag for PHP type {:?}",
            ctx.value_php_type(array)?
        )));
    };
    let preserved_elem_ty = PhpType::AssocArray {
        key: Box::new(PhpType::Int),
        value: elem.clone(),
    };
    let renumbered_elem_ty = PhpType::Array(elem);

    crate::codegen::lower_inst::builtins::spl::emit_preserve_keys_truthiness(ctx, flag)?;
    abi::emit_branch_if_int_result_zero(ctx.emitter, &false_case);

    emit_array_chunk_arm(ctx, array, length, true, &preserved_elem_ty)?;
    crate::codegen::emit_box_current_owned_value_as_mixed(
        ctx.emitter,
        &PhpType::Array(Box::new(preserved_elem_ty.clone())),
    );
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&false_case);
    emit_array_chunk_arm(ctx, array, length, false, &renumbered_elem_ty)?;
    crate::codegen::emit_box_current_owned_value_as_mixed(
        ctx.emitter,
        &PhpType::Array(Box::new(renumbered_elem_ty.clone())),
    );

    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}

/// Lowers `array_pad()` by copying an indexed array and filling missing slots.
pub(crate) fn lower_array_pad(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "array_pad", 3)?;
    let array = expect_operand(inst, 0)?;
    let target_size = expect_operand(inst, 1)?;
    let pad_value = expect_operand(inst, 2)?;
    let source_elem_ty = array_pad_source_element_type(ctx.value_php_type(array)?)?;
    let pad_value_ty = ctx.value_php_type(pad_value)?.codegen_repr();
    let result_elem_ty =
        result_array_element_type("array_pad", &inst.result_php_type.codegen_repr())?;
    require_array_pad_value_type(&source_elem_ty, &pad_value_ty)?;
    require_array_pad_result_type(&source_elem_ty, &result_elem_ty)?;
    lower_array_pad_call(ctx, array, target_size, pad_value, &source_elem_ty)?;
    normalize_indexed_array_result(ctx, "array_pad", &source_elem_ty, &result_elem_ty)?;
    store_if_result(ctx, inst)
}

/// Lowers `array_fill()` for pointer-sized scalar and refcounted payloads.
pub(crate) fn lower_array_fill(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "array_fill", 3)?;
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

/// Lowers `array_fill_keys()` through the hash-building runtime helpers.
pub(crate) fn lower_array_fill_keys(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "array_fill_keys", 2)?;
    let keys = expect_operand(inst, 0)?;
    let value = expect_operand(inst, 1)?;
    let key_elem_ty = array_fill_keys_key_element_type(ctx.value_php_type(keys)?)?;
    let value_ty = ctx.value_php_type(value)?.codegen_repr();
    require_array_fill_keys_key_layout(&key_elem_ty)?;
    require_array_fill_keys_value_type(&value_ty)?;
    require_array_fill_keys_result_type(
        &key_elem_ty,
        &value_ty,
        &inst.result_php_type.codegen_repr(),
    )?;
    lower_array_fill_keys_call(ctx, keys, value, &value_ty)?;
    store_if_result(ctx, inst)
}

/// Lowers `array_combine()` through the hash-building runtime helpers.
pub(crate) fn lower_array_combine(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "array_combine", 2)?;
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

/// Lowers `array_column()` through the target-aware column helpers.
pub(crate) fn lower_array_column(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    column::lower_array_column(ctx, inst)
}

/// Lowers `array_flip()` through the hash-building runtime helpers.
///
/// Associative sources take the `__rt_hash_flip` path, which walks the source hash and
/// dispatches on each entry's RUNTIME value tag; indexed sources keep the existing
/// static-element-type helpers.
pub(crate) fn lower_array_flip(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "array_flip", 1)?;
    let array = expect_operand(inst, 0)?;
    let source_ty = ctx.value_php_type(array)?.codegen_repr();
    // A boxed source with a boxed result raises php's TypeError for a non-array; a typed result
    // (the checker proved the source an array, as `is_array($sleep)` does in DeepClone) takes
    // the gradual hash path below.
    if source_ty == PhpType::Mixed && inst.result_php_type.codegen_repr() == PhpType::Mixed {
        ctx.load_value_to_reg(array, abi::int_arg_reg_name(ctx.emitter.target, 0))?;
        abi::emit_call_label(ctx.emitter, "__rt_array_flip_boxed");
        let valid = ctx.next_label("array_flip_boxed_valid");
        abi::emit_branch_if_int_result_nonzero(ctx.emitter, &valid);
        // php names the source's type (`..., int given`); the boxed source still holds it.
        ctx.load_value_to_result(array)?;
        abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
        let wrong = ctx.next_label("array_flip_boxed_wrong_type");
        abi::emit_jump(ctx.emitter, &wrong);
        super::union_type_guard::emit_mixed_wrong_tag_type_error_dispatch(
            ctx,
            &wrong,
            &|given| format!("array_flip(): Argument #1 ($array) must be of type array, {given} given"),
        );
        ctx.emitter.label(&valid);
        box_hash_result_for_mixed_builtin(ctx, inst, &PhpType::Mixed);
        return store_if_result(ctx, inst);
    }
    if matches!(&source_ty, PhpType::Mixed | PhpType::Union(_)) {
        return lower_gradual_array_flip(ctx, inst, array);
    }
    if matches!(&source_ty, PhpType::AssocArray { .. }) {
        return lower_hash_flip(ctx, inst, array);
    }
    if matches!(&source_ty, PhpType::Array(elem) if elem.codegen_repr() == PhpType::Mixed) {
        return lower_mixed_indexed_flip(ctx, inst, array);
    }
    let value_elem_ty = array_flip_source_element_type(source_ty)?;
    require_array_flip_result_type(&value_elem_ty, &inst.result_php_type.codegen_repr())?;
    ctx.load_value_to_result(array)?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the source indexed-array pointer as the flip helper argument
    }
    abi::emit_call_label(ctx.emitter, array_flip_runtime_helper(&value_elem_ty));
    store_if_result(ctx, inst)
}

/// Lowers `array_flip()` across a gradual array boundary.
///
/// The boxed source is normalized into an independently owned Mixed-valued hash so the runtime
/// helper can inspect every value tag. The fresh flipped hash must survive releasing that
/// conversion temporary, then ownership transfers into the boxed gradual result.
fn lower_gradual_array_flip(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
) -> Result<()> {
    let result_ty = inst.result_php_type.codegen_repr();
    let (dest_value_ty, box_result) = match &result_ty {
        PhpType::Mixed | PhpType::Union(_) => (PhpType::Mixed, true),
        PhpType::AssocArray { value, .. } => (value.codegen_repr(), false),
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_flip gradual result PHP type {:?}",
                other
            )))
        }
    };
    super::misc_dispatch::materialize_owned_mixed_hash_operand(ctx, array, "array_flip")?;
    let dest_value_tag = runtime_value_tag("array_flip", &dest_value_ty)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(ctx.emitter, "x0");                              // preserve the owned normalized source hash across the flip
            ctx.emitter
                .instruction(&format!("mov x1, #{}", dest_value_tag));          // destination values are boxed source keys of either supported key kind
            abi::emit_call_label(ctx.emitter, "__rt_hash_flip");
            abi::emit_pop_reg(ctx.emitter, "x1");                               // recover the normalized source hash after the flip
            abi::emit_push_reg(ctx.emitter, "x0");                              // preserve the fresh flipped result across source release
            ctx.emitter.instruction("mov x0, x1");                              // release the independently owned normalized source hash
            abi::emit_call_label(ctx.emitter, "__rt_decref_hash");
            abi::emit_pop_reg(ctx.emitter, "x0");                               // restore the fresh flipped hash for result boxing
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // pass and preserve the normalized source hash as argument zero
            abi::emit_push_reg(ctx.emitter, "rdi");
            ctx.emitter
                .instruction(&format!("mov rsi, {}", dest_value_tag));          // destination values are boxed source keys of either supported key kind
            abi::emit_call_label(ctx.emitter, "__rt_hash_flip");
            abi::emit_pop_reg(ctx.emitter, "rcx");                              // recover the normalized source hash after the flip
            abi::emit_push_reg(ctx.emitter, "rax");                             // preserve the fresh flipped result across source release
            ctx.emitter.instruction("mov rdi, rcx");                            // release the independently owned normalized source hash
            abi::emit_call_label(ctx.emitter, "__rt_decref_hash");
            abi::emit_pop_reg(ctx.emitter, "rax");                              // restore the fresh flipped hash for result boxing
        }
    }
    if box_result {
        crate::codegen::emit_box_current_owned_value_as_mixed(
            ctx.emitter,
            &PhpType::AssocArray {
                key: Box::new(PhpType::Mixed),
                value: Box::new(PhpType::Mixed),
            },
        );
    }
    store_if_result(ctx, inst)
}

/// Lowers `array_flip()` over an indexed array whose slots contain boxed `Mixed` cells.
///
/// The packed source is first copied into an integer-keyed hash so `__rt_hash_flip` can inspect
/// each cell's runtime tag. The conversion is an owned temporary and must be released after the
/// fresh flipped hash has been preserved across the release call.
fn lower_mixed_indexed_flip(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
) -> Result<()> {
    require_array_flip_result_type(&PhpType::Mixed, &inst.result_php_type.codegen_repr())?;
    let dest_value_ty = hash_flip_result_value_type(&inst.result_php_type.codegen_repr())?;
    let dest_value_tag = runtime_value_tag("array_flip", &dest_value_ty)?;
    ctx.load_value_to_result(array)?;
    super::misc_dispatch::emit_convert_indexed_to_hash(ctx);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(ctx.emitter, "x0");                              // preserve the owned converted hash across the flip
            ctx.emitter
                .instruction(&format!("mov x1, #{}", dest_value_tag));          // destination values are the original integer indexes
            abi::emit_call_label(ctx.emitter, "__rt_hash_flip");
            abi::emit_pop_reg(ctx.emitter, "x1");                               // recover the converted source hash
            abi::emit_push_reg(ctx.emitter, "x0");                              // preserve the fresh flipped result across release
            ctx.emitter.instruction("mov x0, x1");                              // release the owned converted source hash
            abi::emit_call_label(ctx.emitter, "__rt_decref_hash");
            abi::emit_pop_reg(ctx.emitter, "x0");                               // restore the fresh flipped result
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // pass and preserve the converted hash as argument zero
            abi::emit_push_reg(ctx.emitter, "rdi");
            ctx.emitter
                .instruction(&format!("mov rsi, {}", dest_value_tag));          // destination values are the original integer indexes
            abi::emit_call_label(ctx.emitter, "__rt_hash_flip");
            abi::emit_pop_reg(ctx.emitter, "rcx");                              // recover the converted source hash
            abi::emit_push_reg(ctx.emitter, "rax");                             // preserve the fresh flipped result across release
            ctx.emitter.instruction("mov rdi, rcx");                            // release the owned converted source hash
            abi::emit_call_label(ctx.emitter, "__rt_decref_hash");
            abi::emit_pop_reg(ctx.emitter, "rax");                              // restore the fresh flipped result
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `array_flip()` over an ASSOCIATIVE source through `__rt_hash_flip`.
///
/// Flipping turns source keys into destination values, so the destination hash's declared
/// `value_type` is the runtime tag of the RESULT's value type — which the checker derived
/// from the source KEY type. The helper dispatches per entry on the runtime value tag, so
/// `Int`, `Str`, and boxed `Mixed` source values all share this one lowering; values PHP
/// refuses as keys are warned about and skipped inside the helper.
pub(super) fn lower_hash_flip(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
) -> Result<()> {
    hash_flip_source_value_type(&ctx.value_php_type(array)?.codegen_repr())?;
    let dest_value_ty = hash_flip_result_value_type(&inst.result_php_type.codegen_repr())?;
    let dest_value_tag = runtime_value_tag("array_flip", &dest_value_ty)?;
    ctx.load_value_to_result(array)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("mov x1, #{}", dest_value_tag));           // pass the destination value_type tag to the hash-flip helper
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // pass the source hash pointer as the first hash-flip argument
            ctx.emitter
                .instruction(&format!("mov rsi, {}", dest_value_tag));           // pass the destination value_type tag to the hash-flip helper
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_hash_flip");
    store_if_result(ctx, inst)
}

/// Returns the source value type when `__rt_hash_flip` can flip a hash faithfully.
///
/// `Int`, `Str`, and boxed `Mixed` values are accepted. The runtime helper unboxes each `Mixed`
/// cell and dispatches on its concrete tag, which lets one heterogeneous source flip integer and
/// string entries while warning and skipping values PHP does not accept as keys. Concrete static
/// types outside that set remain compile-time refusals because their entire source is invalid.
pub(super) fn hash_flip_source_value_type(source_ty: &PhpType) -> Result<PhpType> {
    match source_ty {
        PhpType::AssocArray { value, .. } => {
            let value = value.codegen_repr();
            if matches!(value, PhpType::Int | PhpType::Str | PhpType::Mixed) {
                return Ok(value);
            }
            Err(CodegenIrError::unsupported(format!(
                "array_flip for associative value PHP type {:?}",
                value
            )))
        }
        other => Err(CodegenIrError::unsupported(format!(
            "array_flip for PHP type {:?}",
            other
        ))),
    }
}

/// Returns the destination `value_type` for an associative `array_flip()`.
///
/// Rejects any result shape other than a hash: `__rt_hash_flip` always builds a hash, so a
/// non-`AssocArray` result would mean the checker and the backend disagree.
pub(super) fn hash_flip_result_value_type(result_ty: &PhpType) -> Result<PhpType> {
    match result_ty {
        PhpType::AssocArray { value, .. } => Ok(value.codegen_repr()),
        other => Err(CodegenIrError::unsupported(format!(
            "array_flip associative result PHP type {:?}",
            other
        ))),
    }
}

/// Lowers boxed PHP arrays or concrete indexed arrays through their representation-safe reverse helper.
///
/// PHP's `bool $preserve_keys = false` renumbers the reversed array from zero; a literal `true`
/// keeps the source integer keys while reversing the iteration order. A dense indexed array
/// cannot hold keys in descending order, so the key-preserving form lowers to
/// `__rt_array_to_hash_reverse`, which builds an owned hash. The checker guarantees the flag is a
/// literal (it decides the result's static shape), so a non-literal operand can only mean the
/// checker and the backend disagree about this call.
pub(crate) fn lower_array_reverse(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "array_reverse", 1, 2)?;
    let array = expect_operand(inst, 0)?;
    if ctx.value_php_type(array)?.codegen_repr() == PhpType::Mixed {
        return super::boxed_reverse::lower_boxed_array_reverse(ctx, inst, array);
    }
    // Every concrete source, hash included, takes the arms below: a hash reverses with its keys
    // and `false` renumbers them (`emit_array_reverse_reindexed`), whatever its value type, and a
    // non-literal flag is decided at run time. That covers the boxed Mixed-value hash too.
    let flag = inst.operands.get(1).copied();
    let preserve_keys = match flag {
        None => Some(false),
        Some(flag) => const_bool_operand(ctx, flag)?,
    };
    match preserve_keys {
        Some(true) => lower_array_reverse_preserve_keys(ctx, inst, array),
        Some(false) => {
            emit_array_reverse_reindexed(ctx, array)?;
            store_if_result(ctx, inst)
        }
        None => lower_array_reverse_dynamic_preserve_keys(
            ctx,
            inst,
            array,
            flag.expect("a non-literal flag came from an operand"),
        ),
    }
}

/// Emits the `preserve_keys = false` arm: the reversed value keeps the source's own shape.
///
/// A HASH takes the key-preserving reverse and then `__rt_hash_reindex`, which is php's rule for
/// the flag: integer keys are renumbered from zero in the NEW order, string keys are left alone.
/// Composing the two is what gives a hash a `false` arm at all — before it, `array_reverse($h,
/// false)` refused with `unsupported EIR backend feature: array_reverse for PHP type AssocArray`.
fn emit_array_reverse_reindexed(ctx: &mut FunctionContext<'_>, array: ValueId) -> Result<()> {
    if matches!(
        ctx.value_php_type(array)?.codegen_repr(),
        PhpType::AssocArray { .. }
    ) {
        emit_array_reverse_preserve_keys_value(ctx, array)?;
        if ctx.emitter.target.arch == Arch::X86_64 {
            ctx.emitter.instruction("mov rdi, rax");                            // pass the reversed hash to the renumbering helper
        }
        abi::emit_call_label(ctx.emitter, "__rt_hash_reindex");
        return Ok(());
    }
    let elem_ty =
        eight_byte_indexed_array_element_type(ctx.value_php_type(array)?, "array_reverse")?;
    ctx.load_value_to_result(array)?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the source indexed-array pointer as the reverse helper argument
    }
    abi::emit_call_label(ctx.emitter, array_reverse_runtime_helper(&elem_ty));
    Ok(())
}

/// Lowers `array_reverse($value, $flag)` when `$flag` is only known at run time.
///
/// The two arms produce DIFFERENT representations -- a dense array and a hash -- so neither can be
/// the single result. Each arm is boxed as Mixed, which is what the checker's union return type
/// already says the call yields, and is the same shape `iterator_to_array()` uses for its own
/// runtime flag. Twig's `CoreExtension::reverse()` forwards an untyped parameter here, so
/// refusing a non-literal flag refused the whole file.
fn lower_array_reverse_dynamic_preserve_keys(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
    flag: ValueId,
) -> Result<()> {
    let false_case = ctx.next_label("array_reverse_preserve_false");
    let done = ctx.next_label("array_reverse_preserve_done");

    let source_ty = ctx.value_php_type(array)?.codegen_repr();
    if matches!(
        source_ty,
        PhpType::Mixed | PhpType::Union(_) | PhpType::Iterable
    ) {
        return lower_array_reverse_gradual_dynamic(ctx, inst, array, flag);
    }
    let preserved_ty = match &source_ty {
        PhpType::Array(elem) => PhpType::AssocArray {
            key: Box::new(PhpType::Int),
            value: elem.clone(),
        },
        other => other.clone(),
    };

    // Box ONLY when the two arms really do differ. A hash reverses to a hash either way, so the
    // checker answers one concrete type for it, and boxing would hand the caller a cell where its
    // own declared type says container pointer -- `count()` on the result segfaulted.
    let arms_differ = matches!(
        inst.result_php_type.codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    );

    crate::codegen::lower_inst::builtins::spl::emit_preserve_keys_truthiness(ctx, flag)?;
    abi::emit_branch_if_int_result_zero(ctx.emitter, &false_case);

    emit_array_reverse_preserve_keys_value(ctx, array)?;
    if arms_differ {
        crate::codegen::emit_box_current_owned_value_as_mixed(ctx.emitter, &preserved_ty);
    }
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&false_case);
    emit_array_reverse_reindexed(ctx, array)?;
    if arms_differ {
        crate::codegen::emit_box_current_owned_value_as_mixed(ctx.emitter, &source_ty);
    }

    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}

/// Lowers `array_reverse($value, $flag)` when NEITHER the flag nor the source storage is static.
///
/// The checker reaches this shape through its own narrowing: `twig/twig`'s `CoreExtension::reverse`
/// calls `array_reverse($item, $preserveKeys)` inside `if (\is_array($item))`, so the checker has an
/// `array` while the lowering still holds the boxed Mixed the untyped parameter arrives in. Both
/// arms answer a HASH -- the source's own storage is not known, and a hash represents either one --
/// which is exactly what `preserve_keys` already produces on the true arm and what
/// `__rt_hash_reindex` turns into php's `false` arm: integer keys renumbered from zero in the new
/// order, string keys left alone. Each arm is boxed, because the checker's union return type says
/// the call answers one of two shapes; a concrete result type means the caller expects one storage
/// and is refused rather than handed the other.
fn lower_array_reverse_gradual_dynamic(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
    flag: ValueId,
) -> Result<()> {
    if !matches!(
        inst.result_php_type.codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        return Err(CodegenIrError::unsupported(format!(
            "array_reverse over a gradual source for result PHP type {:?}",
            inst.result_php_type
        )));
    }
    let hash_ty = PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(PhpType::Mixed),
    };
    let false_case = ctx.next_label("array_reverse_gradual_false");
    let done = ctx.next_label("array_reverse_gradual_done");

    crate::codegen::lower_inst::builtins::spl::emit_preserve_keys_truthiness(ctx, flag)?;
    abi::emit_branch_if_int_result_zero(ctx.emitter, &false_case);

    emit_array_reverse_gradual_to_hash(ctx, array)?;
    crate::codegen::emit_box_current_owned_value_as_mixed(ctx.emitter, &hash_ty);
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&false_case);
    emit_array_reverse_gradual_to_hash(ctx, array)?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the reversed hash to the renumbering helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_hash_reindex");
    crate::codegen::emit_box_current_owned_value_as_mixed(ctx.emitter, &hash_ty);

    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}

/// Reverses a source whose container is only known at run time into an owned key-preserving hash.
///
/// Unboxing is what makes this possible at all: the two reverse helpers take a raw container, and
/// each reads its own layout from the header -- the indexed one its value_type and stride, the hash
/// one its entry chain -- so neither needs a static element type. A payload that is not a container
/// takes the same TypeError php raises for `array_reverse('x')`.
fn emit_array_reverse_gradual_to_hash(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
) -> Result<()> {
    let hash_case = ctx.next_label("array_reverse_gradual_hash");
    let invalid = ctx.next_label("array_reverse_gradual_wrong_type");
    let done = ctx.next_label("array_reverse_gradual_unboxed");
    ctx.load_value_to_result(array)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #5");                              // runtime tag 5 = associative array
            ctx.emitter.instruction(&format!("b.eq {hash_case}"));
            ctx.emitter.instruction("cmp x0, #4");                              // runtime tag 4 = indexed array
            ctx.emitter.instruction(&format!("b.ne {invalid}"));
            ctx.emitter.instruction("mov x0, x1");                              // pass the unboxed indexed array pointer
            abi::emit_call_label(ctx.emitter, "__rt_array_to_hash_reverse");
            abi::emit_jump(ctx.emitter, &done);
            ctx.emitter.label(&hash_case);
            ctx.emitter.instruction("mov x0, x1");                              // pass the unboxed hash pointer
            abi::emit_call_label(ctx.emitter, "__rt_hash_to_hash_reverse");
            abi::emit_jump(ctx.emitter, &done);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 5");                              // runtime tag 5 = associative array
            ctx.emitter.instruction(&format!("je {hash_case}"));
            ctx.emitter.instruction("cmp rax, 4");                              // runtime tag 4 = indexed array
            ctx.emitter.instruction(&format!("jne {invalid}"));
            abi::emit_call_label(ctx.emitter, "__rt_array_to_hash_reverse");    // the unboxed payload is already the first argument
            abi::emit_jump(ctx.emitter, &done);
            ctx.emitter.label(&hash_case);
            abi::emit_call_label(ctx.emitter, "__rt_hash_to_hash_reverse");     // the unboxed payload is already the first argument
            abi::emit_jump(ctx.emitter, &done);
        }
    }
    union_type_guard::emit_mixed_wrong_tag_type_error_dispatch(ctx, &invalid, &|given| {
        format!(
            "array_reverse(): Argument #1 ($array) must be of type array, {} given",
            given
        )
    });
    ctx.emitter.label(&done);
    Ok(())
}

/// Lowers `array_unique()` for indexed arrays with 8-byte payload slots.
pub(crate) fn lower_array_unique(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "array_unique", 1)?;
    let array = expect_operand(inst, 0)?;
    // Verified rather than assumed: the checker widens an indexed input to a hash because PHP
    // keeps each survivor's ORIGINAL key, so the result of `[1,2,2,3,1]` has no key 2. A
    // lowering that still built a dense array would disagree with its own declared type,
    // which miscompiles instead of failing to build.
    let PhpType::AssocArray { .. } = inst.result_php_type.codegen_repr() else {
        return Err(CodegenIrError::unsupported(format!(
            "array_unique result PHP type {:?}",
            inst.result_php_type
        )));
    };
    // An argument that is ALREADY a hash takes its own helper. PHP accepts it — `array_unique` of
    // an associative array is ordinary code — and the indexed builder cannot serve it, because it
    // walks fixed-size slots and a hash has none.
    if let PhpType::AssocArray { value, .. } = ctx.value_php_type(array)?.codegen_repr() {
        let value_ty = value.codegen_repr();
        // Boxed values would be keyed by their POINTER in the seen table, deduplicating by
        // identity instead of by value; refused for the same reason as the indexed path.
        if !matches!(value_ty, PhpType::Int | PhpType::Str) {
            return Err(CodegenIrError::unsupported(format!(
                "array_unique for hash values of PHP type {:?}",
                value_ty
            )));
        }
        ctx.load_value_to_result(array)?;
        if ctx.emitter.target.arch == Arch::X86_64 {
            ctx.emitter.instruction("mov rdi, rax");                            // pass the source hash as the dedup helper argument
        }
        abi::emit_call_label(ctx.emitter, "__rt_hash_to_hash_unique");
        return store_if_result(ctx, inst);
    }

    // The element type is read here rather than through the shared 8-byte gate: that gate is also
    // `array_reverse`, `shuffle` and `array_merge`, none of which compare their elements, and it
    // refuses `Str` because a string slot is a 16-byte (pointer, length) pair. `array_unique` can
    // take strings — the helper reads its stride from the array header and compares string slots
    // BY VALUE — so refusing them here would only keep `array_unique($names)` from compiling.
    let PhpType::Array(elem) = ctx.value_php_type(array)?.codegen_repr() else {
        return Err(CodegenIrError::unsupported(format!(
            "array_unique for PHP type {:?}",
            ctx.value_php_type(array)?
        )));
    };
    let elem_ty = elem.codegen_repr();
    if !matches!(
        elem_ty,
        PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Str
            | PhpType::Callable
            | PhpType::Void
            | PhpType::Never
    ) && !elem_ty.is_refcounted()
    {
        return Err(CodegenIrError::unsupported(format!(
            "array_unique for indexed-array element PHP type {:?}",
            elem_ty
        )));
    }
    // A BOXED element's slot holds a CELL POINTER, so the raw-word scan compared cell ADDRESSES:
    // two separately boxed `1`s never matched and `array_unique([1,"b",1,4])` answered `1,b,1,4`
    // where PHP answers `1,b,4`. `__rt_array_to_hash_unique` now recognizes the boxed value_type
    // and compares those slots through `__rt_mixed_string_eq`, PHP's own `SORT_STRING` rule for
    // this builtin, so the boxed shape lowers with the same helper as every other element type.
    ctx.load_value_to_result(array)?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the source indexed-array pointer as the dedup helper argument
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_to_hash_unique");
    store_if_result(ctx, inst)
}



/// Lowers `array_reverse($array, true)` into an owned integer-keyed hash.
///
/// The runtime helper walks the source payload from the last slot to the first and inserts each
/// element at its ORIGINAL index, persisting strings and retaining heap payloads, so the result
/// is a freshly owned hash whose keys match PHP's `preserve_keys` output exactly. The checker
/// types this call as `AssocArray { key: Int, value: T }`, which is re-verified here.
fn lower_array_reverse_preserve_keys(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
) -> Result<()> {
    let PhpType::AssocArray { .. } = inst.result_php_type.codegen_repr() else {
        return Err(CodegenIrError::unsupported(format!(
            "array_reverse preserve_keys result PHP type {:?}",
            inst.result_php_type
        )));
    };
    emit_array_reverse_preserve_keys_value(ctx, array)?;
    store_if_result(ctx, inst)
}

/// Emits the `preserve_keys = true` arm, leaving the reversed hash in the result registers.
fn emit_array_reverse_preserve_keys_value(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
) -> Result<()> {
    // A source that is ALREADY a hash takes its own helper, for the reason `array_unique` does:
    // `__rt_array_to_hash_reverse` walks fixed-size payload slots and a hash has none. PHP accepts
    // `array_reverse($assoc, true)` as ordinary code, and it became reachable everywhere the bare
    // `array` contract started resolving to the hash.
    let source_is_hash = matches!(
        ctx.value_php_type(array)?.codegen_repr(),
        PhpType::AssocArray { .. }
    );
    if !source_is_hash && !matches!(ctx.value_php_type(array)?.codegen_repr(), PhpType::Array(_)) {
        return Err(CodegenIrError::unsupported(format!(
            "array_reverse preserve_keys for PHP type {:?}",
            ctx.value_php_type(array)?
        )));
    }
    ctx.load_value_to_result(array)?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the source container pointer as the key-preserving reverse helper argument
    }
    abi::emit_call_label(
        ctx.emitter,
        if source_is_hash {
            "__rt_hash_to_hash_reverse"
        } else {
            "__rt_array_to_hash_reverse"
        },
    );
    Ok(())
}

/// Reads a literal boolean operand produced by a constant instruction, or `None` when non-literal.
///
/// Accepts `ConstBool`, integer, float, and null const instructions using PHP truthiness, so any
/// literal flag the frontend folds into an argument slot resolves at compile time.
fn const_bool_operand(ctx: &FunctionContext<'_>, value: ValueId) -> Result<Option<bool>> {
    let value_ref = ctx
        .function
        .value(value)
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))?;
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(None);
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    match (inst_ref.op, inst_ref.immediate.as_ref()) {
        (Op::ConstBool, Some(Immediate::Bool(value))) => Ok(Some(*value)),
        (Op::ConstI64, Some(Immediate::I64(value))) => Ok(Some(*value != 0)),
        (Op::ConstF64, Some(Immediate::F64(value))) => Ok(Some(*value != 0.0)),
        (Op::ConstNull, _) => Ok(Some(false)),
        _ => Ok(None),
    }
}
