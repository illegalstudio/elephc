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

/// Lowers `array_push()` by appending every value and publishing the mutated array.
///
/// The operand list is `[array, value…]` with any number of trailing values, matching PHP's
/// `array_push(array &$array, mixed ...$values)`. `array_push($a)` with no values is legal PHP
/// too and just reads the current length back.
///
/// Values are appended one at a time in source order. Each append can reach `__rt_array_grow`
/// and relocate the array, so the per-value helper republishes the receiver between steps rather
/// than once at the end.
pub(crate) fn lower_array_push(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.is_empty() {
        return Err(CodegenIrError::invalid_module(
            "array_push expected at least 1 arg, got 0".to_string(),
        ));
    }
    let array = expect_operand(inst, 0)?;
    if matches!(
        ctx.value_php_type(array)?.codegen_repr(),
        PhpType::AssocArray { .. }
    ) {
        return lower_array_push_into_hash(ctx, inst, array);
    }
    let boxed_receiver = matches!(
        ctx.value_php_type(array)?.codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    );
    if boxed_receiver && inst.operands.len() > 1 {
        super::boxed_mutation::prepare_boxed_array_receiver(ctx, array, "array_push")?;
    }
    for index in 1..inst.operands.len() {
        let value = expect_operand(inst, index)?;
        if boxed_receiver {
            super::super::super::arrays::lower_mixed_array_append_value(ctx, array, value)?;
        } else {
            super::super::super::arrays::lower_array_push_value(ctx, inst, array, value)?;
        }
    }
    load_array_push_length_to_result(ctx, array)?;
    store_if_result(ctx, inst)
}

/// Lowers `array_push()` onto an ASSOCIATIVE receiver, appending at PHP's next automatic
/// integer key.
///
/// PHP draws no distinction between an indexed and an associative array here: `array_push()` is
/// `$hash[] = $value` repeated, and a hash appends at `max(int keys) + 1`, or `0` when it has
/// none. The receiver bookkeeping is done ONCE around the whole run of values rather than per
/// value, because a hash insert does not relocate the table the way an indexed append can — it
/// is `__rt_hash_set` underneath, which handles growth and the copy-on-write split itself
/// (issue #1087).
fn lower_array_push_into_hash(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
) -> Result<()> {
    // An `AssocArray`-typed value can still be the in-band null-container sentinel at run time:
    // a missed hash read materializes one, typed as the element type it failed to find. PHP
    // answers that with a TypeError; dereferencing it segfaults, which is what
    // `array_push($h["missing"], 2)` did. Guarding here covers the inserts and the count read
    // alike, since neither may touch the table on that path.
    let sentinel_label = ctx.next_label("array_push_hash_null");
    let done_label = ctx.next_label("array_push_hash_done");
    let result_reg = abi::int_result_reg(ctx.emitter);
    let scratch_reg = abi::secondary_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(array, result_reg)?;
    crate::codegen::sentinels::emit_branch_if_null_container(
        ctx.emitter,
        result_reg,
        scratch_reg,
        &sentinel_label,
    );
    let receiver = ReceiverPlace::resolve(ctx, array)?;
    // A zero-value call only reads the count below; it never publishes a replacement pointer.
    // Releasing a boxed local here would leave its slot unchanged, so epilogue cleanup would
    // decref the same owner again. The first append is what makes the old owner mutable/consumed.
    if inst.operands.len() > 1 {
        if let Some(slot) = receiver.slot() {
            ctx.release_mutated_source_local_owner(slot, array)?;
        }
    }
    let storage_value_ty = match ctx.value_php_type(array)?.codegen_repr() {
        PhpType::AssocArray { value, .. } => value.codegen_repr(),
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_push for PHP type {:?}",
                other
            )))
        }
    };
    for index in 1..inst.operands.len() {
        let value = expect_operand(inst, index)?;
        require_hash_push_value_fits_storage(ctx, value, &storage_value_ty)?;
        crate::codegen::lower_inst::hashes::append_one_value_to_hash(ctx, array, value, inst)?;
        // An insert can split the table for copy-on-write or grow it, so the pointer the
        // receiver must publish is the helper's RESULT, not the one this side started with.
        // Recording it per value is what makes the next insert address the new table.
        ctx.store_result_value(array)?;
        receiver.store_back_value(ctx, array)?;
    }
    ctx.writeback_global_array_source(array)?;
    // Read the count back from the table rather than from the last insert's return register:
    // the write-back above can clobber it, and `array_push($hash)` with no values never calls
    // a helper at all. The logical entry count is the table's first payload word.
    let result_reg = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(array, result_reg)?;
    abi::emit_load_from_address(ctx.emitter, result_reg, result_reg, 0);
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&sentinel_label);
    super::super::exceptions::emit_type_error(
        ctx,
        "array_push(): Argument #1 ($array) must be of type array, null given",
    );
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Refuses a pushed value the receiver's entry storage cannot hold.
///
/// `$hash[] = $value` gets its receiver widened by the checker before it is lowered, so a
/// `string` appended to an `array<string, int>` arrives with `Mixed` entry storage waiting for
/// it. `array_push()` cannot: its receiver is a by-reference builtin argument, which the checker
/// pins as a reference alias root and never retypes, so the declared entry type is still `int`.
/// Storing the string into int-sized, int-tagged slots read back as the pointer's integer value
/// — silent corruption, with no diagnostic anywhere (issue #1087).
///
/// Refusing is the honest answer until the receiver can be widened. It is not a regression:
/// before this change every associative receiver was a compile error, so the shapes this still
/// rejects are exactly the ones that never compiled.
fn require_hash_push_value_fits_storage(
    ctx: &FunctionContext<'_>,
    value: ValueId,
    storage_value_ty: &PhpType,
) -> Result<()> {
    if matches!(storage_value_ty, PhpType::Mixed | PhpType::Iterable) {
        return Ok(());
    }
    let value_ty = ctx.value_php_type(value)?.codegen_repr();
    if value_ty == *storage_value_ty {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "array_push() of {:?} into an array whose entries are {:?}: the receiver keeps its \
         declared entry type across a by-reference builtin argument, so the value has nowhere \
         to go. Assign through `$array[] = ...` instead, which widens the receiver",
        value_ty, storage_value_ty
    )))
}

/// Materializes the receiver's post-append element count into the int result register.
///
/// PHP's `array_push()` returns the new number of elements. Reading it back from the array
/// rather than from the last append's return register covers the value-less form, which never
/// calls a helper at all, and survives the write-backs each append performs.
///
/// A boxed `Mixed` receiver holds the container behind a cell, so the count comes from the
/// generic length helper; a typed indexed array keeps its logical length in the first payload
/// word and is read directly.
fn load_array_push_length_to_result(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
) -> Result<()> {
    if matches!(
        ctx.value_php_type(array)?.codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        // -- a boxed receiver keeps its container behind a cell, so ask the generic counter --
        // `__rt_mixed_count` is on the single-argument INT-RESULT ABI (`x0` / `rax` in and
        // out), not the C argument ABI. Handing it `rdi` left `rax` holding whatever the last
        // append had put there, so `array_push()` on a boxed receiver returned 0 instead of
        // the new element count (issue #1191).
        ctx.load_value_to_result(array)?;
        abi::emit_call_label(ctx.emitter, "__rt_mixed_count");
        return Ok(());
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            // -- a typed indexed array keeps its logical length in the first payload word --
            ctx.load_value_to_reg(array, "x0")?;
            ctx.emitter.instruction("ldr x0, [x0]");                            // read the post-append element count as the int result
        }
        Arch::X86_64 => {
            // -- a typed indexed array keeps its logical length in the first payload word --
            ctx.load_value_to_reg(array, "rax")?;
            ctx.emitter.instruction("mov rax, QWORD PTR [rax]");                // read the post-append element count as the int result
        }
    }
    Ok(())
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
    let preserve_keys = match inst.operands.get(2).copied() {
        None => false,
        Some(flag) => const_bool_operand(ctx, flag)?.ok_or_else(|| {
            CodegenIrError::unsupported(
                "array_chunk preserve_keys argument that is not a compile-time literal".to_string(),
            )
        })?,
    };
    let result_elem_ty =
        result_array_element_type("array_chunk", &inst.result_php_type.codegen_repr())?;
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
            &result_elem_ty,
        );
        return store_if_result(ctx, inst);
    }
    let source_elem_ty = array_chunk_source_element_type(ctx.value_php_type(array)?)?;
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
    if ctx.value_php_type(array)?.codegen_repr() == PhpType::Mixed {
        if inst.result_php_type.codegen_repr() != PhpType::Mixed {
            return Err(CodegenIrError::unsupported(
                "boxed array_flip requires a boxed array result".to_string(),
            ));
        }
        ctx.load_value_to_reg(array, abi::int_arg_reg_name(ctx.emitter.target, 0))?;
        abi::emit_call_label(ctx.emitter, "__rt_array_flip_boxed");
        let valid = ctx.next_label("array_flip_boxed_valid");
        abi::emit_branch_if_int_result_nonzero(ctx.emitter, &valid);
        crate::codegen::lower_inst::exceptions::emit_type_error(
            ctx, "array_flip(): Argument #1 ($array) must be of type array",
        );
        ctx.emitter.label(&valid);
        box_hash_result_for_mixed_builtin(ctx, inst, &PhpType::Mixed);
        return store_if_result(ctx, inst);
    }
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
