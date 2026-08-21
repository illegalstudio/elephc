//! Purpose:
//! Array values, keys, range, pop, sort, edge-key, and hash builtin entry points.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::arrays`.
//!
//! Key details:
//! - Preserves callback ABI, target parity, array storage, and ownership contracts.

use super::*;
use super::sort_dispatch::KeySortOrder;
use crate::codegen::lower_inst::receiver_place::ReceiverPlace;
use super::range_size;

/// Lowers `array_values()` through the dedicated values-array builtin emitter.
pub(crate) fn lower_array_values(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    values::lower_array_values(ctx, inst)
}

/// Lowers `array_keys()` through the dedicated keys-array builtin emitter.
pub(crate) fn lower_array_keys(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    keys::lower_array_keys(ctx, inst)
}

/// Lowers `array_rand()` for indexed arrays.
pub(crate) fn lower_array_rand(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "array_rand", 1)?;
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
///
/// PHP's optional `$step` becomes the helper's third argument. Its sign never chooses the
/// direction (`start` vs `end` does), so the three `ValueError`s php-src raises for a bad step
/// are emitted here, before the helper ever sees the arguments.
pub(crate) fn lower_range(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "range", 2, 3)?;
    let start = expect_operand(inst, 0)?;
    let end = expect_operand(inst, 1)?;
    let step = inst.operands.get(2).copied();
    require_range_endpoint(ctx.value_php_type(start)?, "start")?;
    require_range_endpoint(ctx.value_php_type(end)?, "end")?;
    if let Some(step) = step {
        require_range_endpoint(ctx.value_php_type(step)?, "step")?;
    }
    require_range_result_type(&inst.result_php_type.codegen_repr())?;
    // Resolve each argument to a plain integer, unboxing a Mixed cell read from a heterogeneous
    // array. Each resolution may call __rt_mixed_cast_int, which clobbers caller-saved registers,
    // so already-resolved values are spilled across it instead of being staged in argument registers.
    resolve_int_operand_to_result(ctx, start, "range start")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    resolve_int_operand_to_result(ctx, end, "range end")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    match step {
        Some(step) => {
            resolve_int_operand_to_result(ctx, step, "range step")?;
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction("mov x2, x0");                      // move the resolved range step into the third runtime argument
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction("mov rdx, rax");                    // move the resolved range step into the third runtime argument
                }
            }
        }
        None => {
            let step_reg = match ctx.emitter.target.arch {
                Arch::AArch64 => "x2",
                Arch::X86_64 => "rdx",
            };
            abi::emit_load_int_immediate(ctx.emitter, step_reg, 1);
        }
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_reg(ctx.emitter, "x1"); // restore the resolved range end into the second runtime argument
            abi::emit_pop_reg(ctx.emitter, "x0"); // restore the resolved range start into the first runtime argument
        }
        Arch::X86_64 => {
            abi::emit_pop_reg(ctx.emitter, "rsi"); // restore the resolved range end into the second runtime argument
            abi::emit_pop_reg(ctx.emitter, "rdi"); // restore the resolved range start into the first runtime argument
        }
    }
    emit_range_guards(ctx, step.is_some());
    abi::emit_call_label(ctx.emitter, "__rt_range");
    store_if_result(ctx, inst)
}

/// Raises every `range()` `ValueError` reference PHP checks before the runtime helper runs.
///
/// The guards read `start`/`end`/`step` while they still sit in their ABI argument registers, so
/// one sequence covers every supported target. Reference PHP rejects, in this order: a zero step,
/// a negative step when `$start < $end` (a decreasing range accepts either sign), a step whose
/// magnitude exceeds the spanned interval — except when `$start === $end`, which always yields the
/// single-element `[$start]` — and finally a requested element count past the maximum array size.
/// The magnitude comparison is UNSIGNED so `PHP_INT_MIN`, whose negation is itself, still reads as
/// wider than any span instead of wrapping back to a negative "magnitude".
///
/// `has_explicit_step` skips the three `$step` guards for a two-argument `range()`: the implicit
/// step is the literal `1` this lowering just materialized, which none of them can reject. The
/// size guard runs either way, because `range(1, 3000000000)` is oversized without any `$step`.
fn emit_range_guards(ctx: &mut FunctionContext<'_>, has_explicit_step: bool) {
    let (start_reg, end_reg, step_reg) = match ctx.emitter.target.arch {
        Arch::AArch64 => ("x0", "x1", "x2"),
        Arch::X86_64 => ("rdi", "rsi", "rdx"),
    };
    if has_explicit_step {
        crate::codegen::lower_inst::exceptions::emit_value_error_unless(
            ctx,
            crate::codegen::lower_inst::exceptions::ValueGuard::NotEqualToImmediate(step_reg, 0),
            RANGE_ZERO_STEP_MESSAGE,
        );
        crate::codegen::lower_inst::exceptions::emit_value_error_unless(
            ctx,
            crate::codegen::lower_inst::exceptions::ValueGuard::NonNegativeUnlessSignedBelow(
                step_reg, start_reg, end_reg,
            ),
            RANGE_NEGATIVE_STEP_MESSAGE,
        );
        crate::codegen::lower_inst::exceptions::emit_value_error_unless(
            ctx,
            crate::codegen::lower_inst::exceptions::ValueGuard::MagnitudeWithinSpan(
                step_reg, start_reg, end_reg,
            ),
            RANGE_STEP_TOO_WIDE_MESSAGE,
        );
    }
    range_size::emit_range_size_guard(ctx);
}

/// Lowers `array_pop()` for indexed arrays by mutating length and boxing `T|null` as Mixed.
pub(crate) fn lower_array_pop(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "array_pop", 1)?;
    let array = expect_operand(inst, 0)?;
    let elem_ty = array_pop_element_type(ctx.value_php_type(array)?)?;
    require_array_pop_result_type(&inst.result_php_type.codegen_repr())?;
    let receiver = ReceiverPlace::resolve(ctx, array)?;
    ensure_unique_array_pop_source(ctx, array)?;
    receiver.store_back_value(ctx, array)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_array_pop_aarch64(ctx, array, &elem_ty)?,
        Arch::X86_64 => lower_array_pop_x86_64(ctx, array, &elem_ty)?,
    }
    store_if_result(ctx, inst)
}

/// Lowers `array_shift()` for indexed arrays by compacting slots and boxing `T|null` as Mixed.
pub(crate) fn lower_array_shift(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    shift::lower_array_shift(ctx, inst)
}

/// Lowers `array_unshift()` for indexed arrays by prepending a scalar payload.
pub(crate) fn lower_array_unshift(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    unshift::lower_array_unshift(ctx, inst)
}

/// Lowers `sort()`, rebuilding a hash receiver re-keyed from zero the way PHP does.
pub(crate) fn lower_sort(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if sort_receiver_is_hash(ctx, inst)? {
        return lower_hash_reindexing_sort(ctx, inst, "sort", "__rt_sort_int", "__rt_sort_str");
    }
    lower_indexed_array_sort(ctx, inst, "sort", "__rt_sort_int", Some("__rt_sort_str"))
}

/// Lowers `rsort()`, rebuilding a hash receiver re-keyed from zero the way PHP does.
pub(crate) fn lower_rsort(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if sort_receiver_is_hash(ctx, inst)? {
        return lower_hash_reindexing_sort(ctx, inst, "rsort", "__rt_rsort_int", "__rt_rsort_str");
    }
    lower_indexed_array_sort(ctx, inst, "rsort", "__rt_rsort_int", Some("__rt_rsort_str"))
}

/// Lowers `asort()` for indexed integer arrays through the value-sort runtime wrapper.
/// Lowers `asort()`, routing hash receivers to the insertion-order value sorter.
///
/// A hash-backed associative array keeps its key/value association while its iteration
/// order changes, which `__rt_hash_asort` implements by relinking the table's chain.
/// Indexed arrays have no separate key storage, so they keep using the slot permuter.
pub(crate) fn lower_asort(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "asort", 1)?;
    if sort_receiver_is_hash(ctx, inst)? {
        return lower_hash_link_sort(ctx, inst, "__rt_hash_asort");
    }
    lower_indexed_array_sort(ctx, inst, "asort", "__rt_asort", None)
}

/// Lowers `arsort()` for indexed integer arrays through the descending value-sort wrapper.
/// Lowers `arsort()`, routing hash receivers to the descending insertion-order value sorter.
pub(crate) fn lower_arsort(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "arsort", 1)?;
    if sort_receiver_is_hash(ctx, inst)? {
        return lower_hash_link_sort(ctx, inst, "__rt_hash_arsort");
    }
    lower_indexed_array_sort(ctx, inst, "arsort", "__rt_arsort", None)
}

/// Lowers `ksort()` through the key-sort helper surface.
pub(crate) fn lower_ksort(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_array_key_sort(ctx, inst, "ksort", KeySortOrder::Ascending)
}

/// Lowers `krsort()` through the reverse key-sort helper surface.
pub(crate) fn lower_krsort(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_array_key_sort(ctx, inst, "krsort", KeySortOrder::Descending)
}

/// Lowers `natsort()`, routing string-valued hash receivers to the key-preserving sorter.
///
/// php sorts naturally with `zend_array_sort(..., php_array_natural_compare, 0)`: the
/// trailing `0` is `renumber`, which `sort()` passes as `1` and `asort()`/`natsort()` pass
/// as `0`. A hash-backed receiver can honour that exactly, because `__rt_hash_natsort`
/// relinks the table's iteration chain and leaves every key attached to its own value.
/// An indexed array stores its keys implicitly as slot positions `0..n-1`, so it has no
/// storage able to hold the permuted keys php produces; those receivers keep using the
/// slot permuter and stay reindexed — the sort family's tracked divergence.
pub(crate) fn lower_natsort(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "natsort", 1)?;
    if natural_sort_receiver_is_string_hash(ctx, inst)? {
        return lower_hash_link_sort(ctx, inst, "__rt_hash_natsort");
    }
    lower_indexed_array_sort(ctx, inst, "natsort", "__rt_natsort", Some("__rt_natsort_str"))
}

/// Lowers `natcasesort()`, routing string-valued hash receivers to the key-preserving sorter.
pub(crate) fn lower_natcasesort(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "natcasesort", 1)?;
    if natural_sort_receiver_is_string_hash(ctx, inst)? {
        return lower_hash_link_sort(ctx, inst, "__rt_hash_natcasesort");
    }
    lower_indexed_array_sort(
        ctx,
        inst,
        "natcasesort",
        "__rt_natcasesort",
        Some("__rt_natcasesort_str"),
    )
}

/// Lowers `shuffle()` for indexed arrays with 8-byte slots by mutating the source array in place.
pub(crate) fn lower_shuffle(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_indexed_array_shuffle(ctx, inst)
}

/// Lowers `usort()` for indexed integer arrays with a static user comparator.
pub(crate) fn lower_usort(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_user_sort_static_callback(ctx, inst, "usort")
}

/// Lowers `uksort()` through the user-sort helper for static comparators.
pub(crate) fn lower_uksort(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_user_sort_static_callback(ctx, inst, "uksort")
}

/// Lowers `uasort()` through the user-sort helper for static comparators.
pub(crate) fn lower_uasort(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_user_sort_static_callback(ctx, inst, "uasort")
}

/// Lowers `array_key_exists()` through the dedicated key-existence builtin emitter.
pub(crate) fn lower_array_key_exists(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    key_exists::lower_array_key_exists(ctx, inst)
}

/// Lowers `array_is_list()` to the `__rt_array_is_list` runtime predicate, returning a bool.
///
/// The runtime helper accepts any array kind (indexed, associative hash, or boxed mixed cell) and
/// reports `1` when the keys are the sequential integers `0..n-1` in insertion order, `0` otherwise.
pub(crate) fn lower_array_is_list(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "array_is_list", 1)?;
    let array = expect_operand(inst, 0)?;
    require_array_like_operand(ctx.value_php_type(array)?, "array_is_list")?;
    let arg0 = abi::int_arg_reg_name(ctx.emitter.target, 0);
    ctx.load_value_to_reg(array, arg0)?;
    abi::emit_call_label(ctx.emitter, "__rt_array_is_list");
    store_if_result(ctx, inst)
}

/// Lowers `array_key_first()` through the shared edge-key helper with selector `0`.
pub(crate) fn lower_array_key_first(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_array_edge_key(ctx, inst, "array_key_first", 0)
}

/// Lowers `array_key_last()` through the shared edge-key helper with selector `1`.
pub(crate) fn lower_array_key_last(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_array_edge_key(ctx, inst, "array_key_last", 1)
}

/// Loads the array operand plus a first/last selector, then calls `__rt_array_edge_key`.
///
/// `which` is `0` for the first key and `1` for the last. The runtime helper boxes the resulting
/// integer or string key into a mixed cell (or a boxed null for empty/non-array inputs) via a tail
/// call to `__rt_mixed_from_value`, leaving the boxed pointer in the integer result register.
pub(super) fn lower_array_edge_key(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    which: i64,
) -> Result<()> {
    super::super::ensure_arg_count(inst, name, 1)?;
    let array = expect_operand(inst, 0)?;
    require_array_like_operand(ctx.value_php_type(array)?, name)?;
    let arg0 = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let arg1 = abi::int_arg_reg_name(ctx.emitter.target, 1);
    ctx.load_value_to_reg(array, arg0)?;
    abi::emit_load_int_immediate(ctx.emitter, arg1, which);
    abi::emit_call_label(ctx.emitter, "__rt_array_edge_key");
    store_if_result(ctx, inst)
}

/// Verifies an operand is array-like: `array_is_list` / `array_key_first` / `array_key_last` accept
/// any indexed array, associative hash, or boxed mixed value, matching their uniform runtime helpers.
pub(super) fn require_array_like_operand(ty: PhpType, name: &str) -> Result<()> {
    match ty.codegen_repr() {
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed => Ok(()),
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            name, other
        ))),
    }
}

/// Validates a two-input hash builtin operand and reports whether it must be converted to a hash.
///
/// Associative arrays are used directly; scalar indexed arrays (`int`/`float`/`bool` elements) are
/// converted to integer-keyed hashes at runtime. Any other shape is unsupported.
pub(super) fn two_hash_operand_needs_conversion(ty: PhpType, name: &str) -> Result<bool> {
    match ty.codegen_repr() {
        PhpType::AssocArray { .. } => Ok(false),
        PhpType::Array(elem) if matches!(*elem, PhpType::Int | PhpType::Float | PhpType::Bool) => {
            Ok(true)
        }
        other => Err(CodegenIrError::unsupported(format!(
            "{} hash operand PHP type {:?}",
            name, other
        ))),
    }
}

/// Loads the array pointer currently in the integer result register and converts it to an owned hash.
///
/// `__rt_array_to_hash` reads its argument from the first argument register; on AArch64 the result
/// register already is that register, but on x86_64 the value lives in `rax` and must move to `rdi`.
pub(super) fn emit_convert_indexed_to_hash(ctx: &mut FunctionContext<'_>) {
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // move the array pointer into the first SysV argument register
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_to_hash");
}

/// Lowers a two-input hash builtin: materializes both operands (converting scalar indexed inputs to
/// owned hashes), calls `runtime_label`, then releases any converted temporaries.
///
/// `mode` is loaded into the third argument register for `array_diff_assoc` (0) /
/// `array_intersect_assoc` (1). The result hash pointer is left in the integer result register.
/// Mirrors the legacy two-hash choreography but sources operands from EIR values.
pub(super) fn lower_two_hash_arg_builtin(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    runtime_label: &str,
    mode: Option<i64>,
) -> Result<()> {
    super::super::ensure_arg_count(inst, name, 2)?;
    let first = expect_operand(inst, 0)?;
    let second = expect_operand(inst, 1)?;
    let conv0 = two_hash_operand_needs_conversion(ctx.value_php_type(first)?, name)?;
    let conv1 = two_hash_operand_needs_conversion(ctx.value_php_type(second)?, name)?;
    let result_reg = abi::int_result_reg(ctx.emitter);

    // -- materialize first operand into the result register, convert if indexed, then spill --
    ctx.load_value_to_reg(first, result_reg)?;
    if conv0 {
        emit_convert_indexed_to_hash(ctx);
    }
    abi::emit_push_reg(ctx.emitter, result_reg);

    // -- materialize second operand into the result register, convert if indexed --
    ctx.load_value_to_reg(second, result_reg)?;
    if conv1 {
        emit_convert_indexed_to_hash(ctx);
    }

    if !conv0 && !conv1 {
        // -- fast path: both inputs are already hashes, no temporaries to free --
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("mov x1, x0");                          // second hash pointer into the second argument register
                abi::emit_pop_reg(ctx.emitter, "x0");
                if let Some(m) = mode {
                    ctx.emitter.instruction(&format!("mov x2, #{}", m));        // mode selector into the third argument register
                }
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov rsi, rax");                        // second hash pointer into the second SysV argument register
                abi::emit_pop_reg(ctx.emitter, "rdi");
                if let Some(m) = mode {
                    ctx.emitter.instruction(&format!("mov edx, {}", m));        // mode selector into the third SysV argument register
                }
            }
        }
        abi::emit_call_label(ctx.emitter, runtime_label);
        return store_if_result(ctx, inst);
    }

    // -- freeing path: at least one input was converted to a temporary hash that must be released --
    abi::emit_push_reg(ctx.emitter, result_reg);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x0, [sp, #16]");                       // first hash pointer kept on the stack for freeing
            ctx.emitter.instruction("ldr x1, [sp]");                            // second hash pointer kept on the stack for freeing
            if let Some(m) = mode {
                ctx.emitter.instruction(&format!("mov x2, #{}", m));            // mode selector into the third argument register
            }
            abi::emit_call_label(ctx.emitter, runtime_label);
            ctx.emitter.instruction("str x0, [sp, #-16]!");                     // spill the result; stack holds [result, h2, h1]
            if conv1 {
                ctx.emitter.instruction("ldr x0, [sp, #16]");                   // reload the converted second hash temporary
                abi::emit_call_label(ctx.emitter, "__rt_decref_hash");
            }
            if conv0 {
                ctx.emitter.instruction("ldr x0, [sp, #32]");                   // reload the converted first hash temporary
                abi::emit_call_label(ctx.emitter, "__rt_decref_hash");
            }
            ctx.emitter.instruction("ldr x0, [sp], #16");                       // restore the result hash pointer
            ctx.emitter.instruction("add sp, sp, #32");                         // discard the two spilled input hash pointers
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");           // first hash pointer kept on the stack for freeing
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp]");                // second hash pointer kept on the stack for freeing
            if let Some(m) = mode {
                ctx.emitter.instruction(&format!("mov edx, {}", m));            // mode selector into the third SysV argument register
            }
            abi::emit_call_label(ctx.emitter, runtime_label);
            ctx.emitter.instruction("sub rsp, 16");                             // reserve a slot for the result
            ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                // spill the result; stack holds [result, h2, h1]
            if conv1 {
                ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");       // reload the converted second hash temporary
                abi::emit_call_label(ctx.emitter, "__rt_decref_hash");
            }
            if conv0 {
                ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 32]");       // reload the converted first hash temporary
                abi::emit_call_label(ctx.emitter, "__rt_decref_hash");
            }
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp]");                // restore the result hash pointer
            ctx.emitter.instruction("add rsp, 48");                             // discard the result slot and the two spilled inputs
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `array_replace()` (right-wins hash merge of two hashes).
pub(crate) fn lower_array_replace(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_two_hash_arg_builtin(ctx, inst, "array_replace", "__rt_array_replace", None)
}

/// Lowers `array_replace_recursive()` (recursive right-wins hash merge).
pub(crate) fn lower_array_replace_recursive(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_two_hash_arg_builtin(
        ctx,
        inst,
        "array_replace_recursive",
        "__rt_array_replace_recursive",
        None,
    )
}

/// Lowers `array_diff_assoc()` via the shared associative diff/intersect helper (mode 0 = diff).
pub(crate) fn lower_array_diff_assoc(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_two_hash_arg_builtin(
        ctx,
        inst,
        "array_diff_assoc",
        "__rt_assoc_diff_intersect",
        Some(0),
    )
}

/// Lowers `array_intersect_assoc()` via the shared associative diff/intersect helper (mode 1 = intersect).
pub(crate) fn lower_array_intersect_assoc(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_two_hash_arg_builtin(
        ctx,
        inst,
        "array_intersect_assoc",
        "__rt_assoc_diff_intersect",
        Some(1),
    )
}

/// Lowers `array_merge_recursive()` (recursive merge with scalar collisions combined into lists).
pub(crate) fn lower_array_merge_recursive(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_two_hash_arg_builtin(
        ctx,
        inst,
        "array_merge_recursive",
        "__rt_array_merge_recursive",
        None,
    )
}

