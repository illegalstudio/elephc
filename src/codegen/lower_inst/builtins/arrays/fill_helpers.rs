//! Purpose:
//! Array fill/combine/flip runtime selection and slice layout checks.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::arrays`.
//!
//! Key details:
//! - Preserves callback ABI, target parity, array storage, and ownership contracts.

use super::*;

/// Calls the legacy runtime helper after materializing `array_fill()` arguments.
///
/// String fills use the `(count, ptr, len)` ABI of `__rt_array_fill_str` (the helper is always
/// 0-indexed, so `start` is unused); every other value type uses the shared `(start, count, value)`
/// scalar/refcounted ABI. The register loads are independent stack reads, so loading `count` before
/// the string pointer/length cannot clobber it.
pub(super) fn lower_array_fill_call(
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
        emit_array_fill_count_guard(ctx, false);
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
    emit_array_fill_count_guard(ctx, true);
    abi::emit_call_label(ctx.emitter, array_fill_runtime_helper(value_ty));
    Ok(())
}

/// php-src's verbatim `ValueError` wording for `array_fill()` with a negative `$count`.
const ARRAY_FILL_NEGATIVE_COUNT_MESSAGE: &str =
    "array_fill(): Argument #2 ($count) must be greater than or equal to 0";

/// The largest `array_fill()` `$count` reference PHP will even attempt to build an array for.
///
/// php-src bounds the count against `INT_MAX` — not against the maximum array size — before it
/// reaches the allocator, so `array_fill(0, 2147483647, …)` is accepted (and then fails on
/// memory) while `array_fill(0, 2147483648, …)` is a `ValueError` for every `$start` and value.
const ARRAY_FILL_MAX_COUNT: i64 = 2_147_483_647;

/// php-src's verbatim `ValueError` wording for an oversized `array_fill()` `$count`.
const ARRAY_FILL_COUNT_TOO_LARGE_MESSAGE: &str = "array_fill(): Argument #2 ($count) is too large";

/// Rejects the `array_fill()` `$count` values reference PHP refuses to build an array for.
///
/// The fill helpers write `$count` straight into the array header's length field without
/// clamping it, so a negative count produced an array whose header claimed a negative length
/// — `count()` answered `-1` and every walk over it read past the payload. A count past
/// `INT_MAX` is memory-safe today (the allocation guards catch it) but reported the process's
/// uncatchable heap fatal where reference PHP throws, so `try { … } catch (ValueError $e)`
/// could never see it; bounding the argument here raises PHP's own error instead.
/// `second_arg_reg` selects which ABI register currently holds `$count`: the string fill helper
/// takes `(count, ptr, len)`, every other fill helper takes `(start, count, value)`.
fn emit_array_fill_count_guard(ctx: &mut FunctionContext<'_>, second_arg_reg: bool) {
    let count_reg = match (ctx.emitter.target.arch, second_arg_reg) {
        (Arch::AArch64, false) => "x0",
        (Arch::AArch64, true) => "x1",
        (Arch::X86_64, false) => "rdi",
        (Arch::X86_64, true) => "rsi",
    };
    crate::codegen::lower_inst::exceptions::emit_value_error_unless(
        ctx,
        crate::codegen::lower_inst::exceptions::ValueGuard::SignedAtLeast(count_reg, 0),
        ARRAY_FILL_NEGATIVE_COUNT_MESSAGE,
    );
    crate::codegen::lower_inst::exceptions::emit_value_error_unless(
        ctx,
        crate::codegen::lower_inst::exceptions::ValueGuard::SignedAtMost(
            count_reg,
            ARRAY_FILL_MAX_COUNT,
        ),
        ARRAY_FILL_COUNT_TOO_LARGE_MESSAGE,
    );
}

/// Calls the keyed `array_fill()` runtime helper after materializing the boxed payload fields.
pub(super) fn lower_array_fill_assoc_call(
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
    emit_array_fill_count_guard(ctx, true);
    abi::emit_call_label(ctx.emitter, "__rt_array_fill_assoc");
    Ok(())
}

/// Materializes a fill payload as the low/high words consumed by `__rt_array_fill_assoc`.
pub(super) fn materialize_array_fill_assoc_value_words(
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
                    ctx.emitter
                        .instruction(&format!("xor {}, {}", hi_reg, hi_reg)); // clear the unused assoc-fill value high word
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
                    ctx.emitter
                        .instruction(&format!("xor {}, {}", hi_reg, hi_reg)); // clear the unused assoc-fill value high word
                }
            }
            Ok(())
        }
    }
}

/// Calls the legacy runtime helper after materializing `array_fill_keys()` arguments.
pub(super) fn lower_array_fill_keys_call(
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
pub(super) fn lower_array_combine_call(
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
pub(super) fn array_fill_keys_runtime_helper(value_ty: &PhpType) -> &'static str {
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
pub(super) fn array_fill_runtime_helper(value_ty: &PhpType) -> &'static str {
    if matches!(value_ty.codegen_repr(), PhpType::Str) {
        "__rt_array_fill_str"
    } else if value_ty.is_refcounted() {
        "__rt_array_fill_refcounted"
    } else {
        "__rt_array_fill"
    }
}

/// Returns the helper matching the combined value element ownership representation.
pub(super) fn array_combine_runtime_helper(value_elem_ty: &PhpType) -> &'static str {
    if value_elem_ty.is_refcounted() {
        "__rt_array_combine_refcounted"
    } else {
        "__rt_array_combine"
    }
}

/// Returns the helper matching the flipped source value slot layout.
pub(super) fn array_flip_runtime_helper(value_elem_ty: &PhpType) -> &'static str {
    if value_elem_ty == &PhpType::Str {
        "__rt_array_flip_string"
    } else {
        "__rt_array_flip"
    }
}

/// Returns the element type for indexed arrays supported by 8-byte helper slots.
pub(super) fn eight_byte_indexed_array_element_type(ty: PhpType, name: &str) -> Result<PhpType> {
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
                name, elem
            )))
        }
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            name, other
        ))),
    }
}

/// Returns the runtime helper for `array_reverse()` based on element ownership.
pub(super) fn array_reverse_runtime_helper(elem_ty: &PhpType) -> &'static str {
    if elem_ty.is_refcounted() {
        "__rt_array_reverse_refcounted"
    } else {
        "__rt_array_reverse"
    }
}

/// Returns the runtime helper for `array_merge()` based on element ownership.
pub(super) fn array_merge_runtime_helper(elem_ty: &PhpType) -> &'static str {
    if elem_ty.is_refcounted() {
        "__rt_array_merge_refcounted"
    } else {
        "__rt_array_merge"
    }
}

/// Returns the source element type when `array_flip()` can use existing runtime helpers.
pub(super) fn array_flip_source_element_type(ty: PhpType) -> Result<PhpType> {
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
pub(super) fn array_slice_source_element_type(ty: PhpType) -> Result<PhpType> {
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
pub(super) fn array_chunk_source_element_type(ty: PhpType) -> Result<PhpType> {
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
pub(super) fn array_pad_source_element_type(ty: PhpType) -> Result<PhpType> {
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
pub(super) fn result_array_element_type(name: &str, ty: &PhpType) -> Result<PhpType> {
    match ty {
        PhpType::Array(elem) => Ok(elem.codegen_repr()),
        other => Err(CodegenIrError::unsupported(format!(
            "{} result PHP type {:?}",
            name, other
        ))),
    }
}

/// Returns the inner chunk element type from an `array<array<T>>` result.
pub(super) fn array_chunk_result_inner_element_type(result_elem_ty: &PhpType) -> Result<PhpType> {
    match result_elem_ty {
        PhpType::Array(inner) => Ok(inner.codegen_repr()),
        other => Err(CodegenIrError::unsupported(format!(
            "array_chunk result element PHP type {:?}",
            other
        ))),
    }
}

/// Verifies that the runtime slice helper can copy this element representation.
pub(super) fn require_array_slice_element_layout(elem: &PhpType) -> Result<()> {
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
pub(super) fn require_array_chunk_element_layout(elem: &PhpType) -> Result<()> {
    if matches!(
        elem,
        PhpType::Int | PhpType::Bool | PhpType::Float | PhpType::Callable | PhpType::Void
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
pub(super) fn require_array_pad_element_layout(elem: &PhpType) -> Result<()> {
    if matches!(
        elem,
        PhpType::Int | PhpType::Bool | PhpType::Float | PhpType::Callable | PhpType::Void
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
pub(super) fn require_array_slice_result_type(
    source_elem_ty: &PhpType,
    result_elem_ty: &PhpType,
) -> Result<()> {
    if source_elem_ty == result_elem_ty || result_elem_ty == &PhpType::Mixed {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "array_slice result element PHP type {:?} for source element PHP type {:?}",
        result_elem_ty, source_elem_ty
    )))
}

/// Verifies the pad value can be copied into the source array's slot layout.
pub(super) fn require_array_pad_value_type(source_elem_ty: &PhpType, pad_value_ty: &PhpType) -> Result<()> {
    if source_elem_ty == pad_value_ty {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "array_pad value PHP type {:?} for source element PHP type {:?}",
        pad_value_ty, source_elem_ty
    )))
}

/// Verifies the produced padded array retains the source element type.
pub(super) fn require_array_pad_result_type(source_elem_ty: &PhpType, result_elem_ty: &PhpType) -> Result<()> {
    if source_elem_ty == result_elem_ty || result_elem_ty == &PhpType::Mixed {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "array_pad result element PHP type {:?} for source element PHP type {:?}",
        result_elem_ty, source_elem_ty
    )))
}

/// Verifies the produced chunk inner arrays retain the source element type.
pub(super) fn require_array_chunk_result_type(
    source_elem_ty: &PhpType,
    result_inner_elem_ty: &PhpType,
) -> Result<()> {
    if source_elem_ty == result_inner_elem_ty {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "array_chunk result inner element PHP type {:?} for source element PHP type {:?}",
        result_inner_elem_ty, source_elem_ty
    )))
}
