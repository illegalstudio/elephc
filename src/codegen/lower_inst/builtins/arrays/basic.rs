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

/// Lowers `array_sum()` over supported indexed arrays and boxed-Mixed associative values.
pub(crate) fn lower_array_sum(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "array_sum", 1)?;
    let array = expect_operand(inst, 0)?;
    if matches!(
        ctx.value_php_type(array)?.codegen_repr(),
        PhpType::AssocArray { value, .. } if value.codegen_repr() == PhpType::Mixed
    ) {
        ctx.load_value_to_result(array)?;
        if ctx.emitter.target.arch == Arch::X86_64 {
            ctx.emitter.instruction("mov rdi, rax");                            // pass the associative-array pointer as the runtime helper argument
        }
        abi::emit_call_label(ctx.emitter, "__rt_hash_sum_mixed");
        return store_if_result(ctx, inst);
    }

    lower_indexed_array_aggregate(
        ctx,
        inst,
        "array_sum",
        "__rt_array_sum",
        Some("__rt_array_sum_mixed"),
    )
}

/// Lowers `array_product()` over supported indexed-array payloads.
pub(crate) fn lower_array_product(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_indexed_array_aggregate(
        ctx,
        inst,
        "array_product",
        "__rt_array_product",
        None,
    )
}

/// Lowers `array_push()` by appending one value and publishing the mutated array.
pub(crate) fn lower_array_push(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "array_push", 2)?;
    let array = expect_operand(inst, 0)?;
    if matches!(
        ctx.value_php_type(array)?.codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        super::super::super::arrays::lower_mixed_array_append(ctx, inst)?;
    } else {
        super::super::super::arrays::lower_array_push(ctx, inst)?;
    }
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        0x7fff_ffff_ffff_fffe,
    );
    store_if_result(ctx, inst)
}

/// Lowers `array_chunk()` by splitting an indexed array into nested indexed arrays.
///
/// PHP's `bool $preserve_keys = false` keeps each chunk's source integer keys instead of
/// renumbering it from zero. A dense indexed array cannot hold a window that does not start at
/// key 0, so the key-preserving form lowers to `__rt_array_chunk_to_hash`, which builds one owned
/// hash per chunk. The checker guarantees the flag is a literal (it decides the result's static
/// shape), so a non-literal operand can only mean the checker and the backend disagree.
pub(crate) fn lower_array_chunk(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "array_chunk", 2, 3)?;
    let array = expect_operand(inst, 0)?;
    let length = expect_operand(inst, 1)?;
    let preserve_keys = match inst.operands.get(2).copied() {
        None => false,
        Some(flag) => const_bool_operand(ctx, flag)?.ok_or_else(|| {
            CodegenIrError::unsupported(
                "array_chunk preserve_keys argument that is not a compile-time literal".to_string(),
            )
        })?,
    };
    let source_elem_ty = array_chunk_source_element_type(ctx.value_php_type(array)?)?;
    let result_elem_ty =
        result_array_element_type("array_chunk", &inst.result_php_type.codegen_repr())?;
    let result_inner_elem_ty = if preserve_keys {
        array_chunk_result_inner_hash_value_type(&result_elem_ty)?
    } else {
        array_chunk_result_inner_element_type(&result_elem_ty)?
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
        &result_elem_ty,
    );
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
    if matches!(
        ctx.value_php_type(array)?.codegen_repr(),
        PhpType::AssocArray { .. }
    ) {
        return lower_hash_flip(ctx, inst, array);
    }
    let value_elem_ty = array_flip_source_element_type(ctx.value_php_type(array)?)?;
    require_array_flip_result_type(&value_elem_ty, &inst.result_php_type.codegen_repr())?;
    ctx.load_value_to_result(array)?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the source indexed-array pointer as the flip helper argument
    }
    abi::emit_call_label(ctx.emitter, array_flip_runtime_helper(&value_elem_ty));
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
/// Only `Int` and `Str` values are accepted. A `Mixed`-valued hash is refused ON PURPOSE:
/// building a heterogeneous associative array currently mis-tags its entries UPSTREAM of this
/// lowering — `$a["k1"] = 1; $a["k2"] = "s";` stores the string payload under the int tag, which
/// `var_dump()` of the source array already renders as `int(<pointer>)` without `array_flip()`
/// ever being involved. The flip dispatches on that per-entry tag, so accepting a Mixed-valued
/// source would turn a visible upstream defect into a silent pointer-keyed miscompile. Refusing
/// keeps the failure honest until the hash-construction path tags Mixed values correctly.
pub(super) fn hash_flip_source_value_type(source_ty: &PhpType) -> Result<PhpType> {
    match source_ty {
        PhpType::AssocArray { value, .. } => {
            let value = value.codegen_repr();
            if matches!(value, PhpType::Int | PhpType::Str) {
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

/// Lowers `array_reverse()` for indexed arrays with 8-byte payload slots.
/// Lowers `array_reverse()` for indexed arrays with 8-byte payload slots.
///
/// PHP's `bool $preserve_keys = false` keeps the source integer keys while reversing the
/// iteration order. A dense indexed array cannot hold keys in descending order, so the
/// key-preserving form lowers to `__rt_array_to_hash_reverse`, which builds an owned hash. The
/// checker guarantees the flag is a literal (it decides the result's static shape), so a
/// non-literal operand can only mean the checker and the backend disagree about this call.
pub(crate) fn lower_array_reverse(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "array_reverse", 1, 2)?;
    let array = expect_operand(inst, 0)?;
    let preserve_keys = match inst.operands.get(1).copied() {
        None => false,
        Some(flag) => const_bool_operand(ctx, flag)?.ok_or_else(|| {
            CodegenIrError::unsupported(
                "array_reverse preserve_keys argument that is not a compile-time literal"
                    .to_string(),
            )
        })?,
    };
    if preserve_keys {
        return lower_array_reverse_preserve_keys(ctx, inst, array);
    }
    let elem_ty =
        eight_byte_indexed_array_element_type(ctx.value_php_type(array)?, "array_reverse")?;
    ctx.load_value_to_result(array)?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the source indexed-array pointer as the reverse helper argument
    }
    abi::emit_call_label(ctx.emitter, array_reverse_runtime_helper(&elem_ty));
    store_if_result(ctx, inst)
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
    let elem_ty = eight_byte_indexed_array_element_type(ctx.value_php_type(array)?, "array_unique")?;
    // The dedup scan compares slots as RAW words, which is a POINTER for a boxed element, so
    // two separately boxed `1`s never matched: `array_unique([1,"b",1,4])` answered `1,b,1,4`
    // where PHP answers `1,b,4`. PHP compares these elements by their STRING rendering.
    // Refused rather than answered wrongly, like the set operations that share the defect; the
    // gate itself cannot carry this, because `array_reverse`, `shuffle` and `array_merge` use
    // it too and never compare their elements.
    if matches!(elem_ty, PhpType::Mixed | PhpType::Union(_)) {
        return Err(CodegenIrError::unsupported(format!(
            "array_unique compares boxed elements by identity, not by value, for indexed-array \
             element PHP type {:?}",
            elem_ty
        )));
    }
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
    let PhpType::Array(_) = ctx.value_php_type(array)?.codegen_repr() else {
        return Err(CodegenIrError::unsupported(format!(
            "array_reverse preserve_keys for PHP type {:?}",
            ctx.value_php_type(array)?
        )));
    };
    let PhpType::AssocArray { .. } = inst.result_php_type.codegen_repr() else {
        return Err(CodegenIrError::unsupported(format!(
            "array_reverse preserve_keys result PHP type {:?}",
            inst.result_php_type
        )));
    };
    ctx.load_value_to_result(array)?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the source indexed-array pointer as the key-preserving reverse helper argument
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_to_hash_reverse");
    store_if_result(ctx, inst)
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

