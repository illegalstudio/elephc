//! Purpose:
//! Lowers small indexed-array and associative-array builtins for the EIR backend.
//! Delegates aggregate iteration, set operations, and key checks to existing runtime helpers.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::lower_language_construct_call()`.
//!
//! Key details:
//! - Aggregate helpers accept indexed arrays with 8-byte payload slots, and
//!   dispatch to refcount-aware runtime variants when payloads own heap values.
//! - Associative key filters require hash operands because their runtime helpers copy hash entries.

use crate::codegen::platform::Arch;
use crate::codegen::{
    abi, callable_descriptor, callable_dispatch, emit_box_current_owned_value_as_mixed,
    emit_box_current_value_as_mixed,
};
use crate::codegen::{CodegenIrError, Result};
use crate::codegen_support::runtime::HashMapResultKind;
use crate::codegen_support::DeferredCallbackWrapper;
use crate::ir::{BlockId, Immediate, Instruction, LocalSlotId, Op, ValueDef, ValueId};
use crate::names::{function_symbol, method_symbol, php_symbol_key, static_method_symbol};
use crate::types::{array_key_type_from_value_type, PhpType};

use super::super::super::context::FunctionContext;
use super::super::callables::runtime_string_descriptor_cases;
use super::super::{expect_operand, resolve_int_operand_to_result, store_if_result};

mod internal_pointer;
mod range_size;
mod column;
mod key_exists;
mod keys;
mod search;
mod shift;
mod unshift;
pub(in crate::codegen::lower_inst::builtins) mod values;
mod basic;
mod filter;
mod map_dispatch;
mod map_results;
mod reduce_sets;
mod misc_dispatch;
mod callback_builtins;
mod sort_dispatch;
mod type_validation;
mod callback_binding;
mod callback_sources;
mod callback_targets;
mod callback_wrapper_frame;
mod callback_wrapper_emit;
mod helper_validation;
mod fill_helpers;
mod slice_splice;
mod pop_search;
mod in_array_cases;
mod in_array_coercions;
mod in_array_strings;

use map_dispatch::*;
use map_results::*;
use misc_dispatch::*;
use crate::codegen::lower_inst::receiver_place::ReceiverPlace;
use sort_dispatch::*;
use type_validation::*;
use callback_binding::*;
use callback_sources::*;
use callback_targets::*;
use callback_wrapper_frame::*;
use callback_wrapper_emit::*;
use helper_validation::*;
use fill_helpers::*;
use slice_splice::*;
use pop_search::*;
use in_array_cases::*;
use in_array_coercions::*;
use in_array_strings::*;

pub(crate) use basic::{
    lower_call_user_func_builtin_escape, lower_array_sum, lower_array_product, lower_array_push,
    lower_array_chunk, lower_array_pad, lower_array_fill, lower_array_fill_keys,
    lower_array_combine, lower_array_column, lower_array_flip, lower_array_reverse,
    lower_array_unique,
};
pub(crate) use filter::{
    lower_array_filter,
};
pub(crate) use map_dispatch::{
    lower_array_map,
};
pub(crate) use reduce_sets::{
    lower_array_reduce, lower_array_walk, lower_array_merge, lower_array_diff,
    lower_array_intersect, lower_array_diff_key, lower_array_intersect_key, lower_array_slice,
    lower_array_splice,
};
pub(crate) use misc_dispatch::{
    lower_array_values, lower_array_keys, lower_array_rand, lower_range,
    lower_array_pop, lower_array_shift, lower_array_unshift, lower_sort,
    lower_rsort, lower_asort, lower_arsort, lower_ksort,
    lower_krsort, lower_natsort, lower_natcasesort, lower_shuffle,
    lower_usort, lower_uksort, lower_uasort, lower_array_key_exists,
    lower_array_is_list, lower_array_key_first, lower_array_key_last, lower_array_replace,
    lower_array_replace_recursive, lower_array_diff_assoc, lower_array_intersect_assoc, lower_array_merge_recursive,
};
pub(crate) use callback_builtins::{
    lower_array_find, lower_array_any, lower_array_all, lower_array_walk_recursive,
    lower_array_udiff, lower_array_uintersect, lower_array_multisort, lower_array_search,
    lower_in_array,
};
pub(super) use in_array_cases::InArrayMode;

/// How `array_splice()`'s optional `$replacement` argument is handed to the insert helper.
///
/// PHP casts a non-array `$replacement` to `(array) $replacement`, so a bare scalar inserts one
/// element. `null` and an empty array insert nothing, which is also what an omitted argument does.
enum SpliceReplacement {
    /// No `$replacement` argument, or one that is statically known to insert nothing.
    Empty,
    /// An indexed array whose payload slots are inserted verbatim.
    Array(ValueId),
    /// An indexed array of typed scalars each boxed into a Mixed cell before insertion,
    /// carrying the runtime value_type tag those payloads are boxed with.
    BoxedArray(ValueId, u8),
    /// An indexed array of boxed Mixed cells read back as plain integers before insertion.
    UnboxedArray(ValueId),
    /// A single scalar wrapped in a one-element array before the insertion.
    ///
    /// `scalar_ty` selects the synthesized array's slot width (16 bytes for a string
    /// pointer/length pair, 8 for every other scalar). `boxed_tag` is `Some` when the receiver
    /// stores boxed Mixed cells, in which case that one-element array goes to the boxing insert
    /// helper with the scalar's runtime value_type tag.
    Scalar {
        value: ValueId,
        scalar_ty: PhpType,
        boxed_tag: Option<u8>,
    },
}

/// Returns the chunk value type from a key-preserving `array<assoc<int, T>>` result.
///
/// `array_chunk($a, $n, true)` builds one integer-keyed hash per chunk, so the result element is
/// an `AssocArray` whose keys are the preserved source indices and whose values carry the source
/// element layout the runtime helper copies.
fn array_chunk_result_inner_hash_value_type(result_elem_ty: &PhpType) -> Result<PhpType> {
    match result_elem_ty {
        PhpType::AssocArray { key, value } if key.codegen_repr() == PhpType::Int => {
            Ok(value.codegen_repr())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "array_chunk preserve_keys result element PHP type {:?}",
            other
        ))),
    }
}

/// Returns the runtime element tag `__rt_array_count_values` needs for an indexed source.
///
/// Only `Int`, `Str`, and boxed `Mixed` elements can produce a PHP array key. Every other
/// element type is reported with its own tag so the helper warns and skips each entry the way
/// php-src does, instead of reading a float payload as a pointer.
fn array_count_values_element_tag(source_ty: &PhpType) -> Result<u8> {
    match source_ty {
        PhpType::Array(elem) => runtime_value_tag("array_count_values", &elem.codegen_repr()),
        other => Err(CodegenIrError::unsupported(format!(
            "array_count_values for PHP type {:?}",
            other
        ))),
    }
}

/// Returns the indexed-array element type accepted by the `array_reduce()` runtimes.
///
/// String elements are allowed because `__rt_array_reduce_str` reads the 16-byte
/// `[ptr][len]` payload slots and hands the callback a pointer/length pair; every
/// other accepted element kind is a single 8-byte payload consumed by
/// `__rt_array_reduce`. The accumulator is validated separately and must still fit
/// in one integer register, so no intermediate string ever needs persisting.
fn array_reduce_callback_array_element_type(ty: PhpType) -> Result<PhpType> {
    match ty.codegen_repr() {
        PhpType::Array(elem) => {
            let elem = elem.codegen_repr();
            if elem == PhpType::Str {
                return Ok(elem);
            }
            eight_byte_callback_value_type(elem, "array_reduce")
        }
        other => Err(CodegenIrError::unsupported(format!(
            "array_reduce for PHP type {:?}",
            other
        ))),
    }
}

/// Returns the `array_reduce()` runtime helper matching the source element width.
fn array_reduce_runtime_label(elem_ty: &PhpType) -> &'static str {
    if elem_ty.codegen_repr() == PhpType::Str {
        "__rt_array_reduce_str"
    } else {
        "__rt_array_reduce"
    }
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

/// Rejects the non-positive `array_chunk()` `$length` reference PHP refuses to chunk with.
///
/// The chunking helpers advance their cursor by `$length`, so a zero length never reaches the
/// end of the source and kept allocating empty chunks until the heap was exhausted; a negative
/// length walks the cursor backwards. The guard runs while `$length` still sits in its ABI
/// argument register, so it covers every chunk-helper variant at once.
fn emit_array_chunk_length_guard(ctx: &mut FunctionContext<'_>) {
    let length_reg = match ctx.emitter.target.arch {
        Arch::AArch64 => "x1",
        Arch::X86_64 => "rsi",
    };
    crate::codegen::lower_inst::exceptions::emit_value_error_unless(
        ctx,
        crate::codegen::lower_inst::exceptions::ValueGuard::SignedAtLeast(length_reg, 1),
        ARRAY_CHUNK_NON_POSITIVE_LENGTH_MESSAGE,
    );
}

/// Writes `$replacement` values into a boxed-Mixed receiver after its removal window closed.
///
/// The Mixed cell owns the indexed array, so a growth relocation has to be republished into the
/// cell's payload slot rather than into a frame slot. The removed-elements array and the
/// normalized insertion index are parked on the temporary stack across the insertion, which can
/// reach `__rt_array_grow`.
fn emit_mixed_splice_replacement_insert(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    replacement: &SpliceReplacement,
) -> Result<()> {
    if !replacement.inserts_values() {
        return Ok(());
    }
    let (removed_reg, at_reg) = splice_result_regs(ctx);
    let (_dst_reg, index_reg, replacement_reg) = splice_insert_arg_regs(ctx);
    let cell_reg = abi::secondary_scratch_reg(ctx.emitter);
    // Temporary layout after both pushes: [0] = replacement array, [16] = removed array,
    // [24] = normalized insertion index.
    abi::emit_push_reg_pair(ctx.emitter, removed_reg, at_reg);
    emit_splice_replacement_pointer(ctx, replacement)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    ctx.load_value_to_reg(array, cell_reg)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x0, [x10, #8]");                       // read the converted indexed array out of the Mixed cell
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, QWORD PTR [r10 + 8]");            // read the converted indexed array out of the Mixed cell
        }
    }
    abi::emit_load_temporary_stack_slot(ctx.emitter, index_reg, 24);
    abi::emit_load_temporary_stack_slot(ctx.emitter, replacement_reg, 0);
    emit_splice_boxing_tag(ctx, replacement);
    abi::emit_call_label(
        ctx.emitter,
        array_splice_insert_runtime_helper(replacement, &PhpType::Mixed),
    );
    ctx.load_value_to_reg(array, cell_reg)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("str x0, [x10, #8]");                       // republish the possibly-relocated indexed array into the Mixed cell
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov QWORD PTR [r10 + 8], rax");            // republish the possibly-relocated indexed array into the Mixed cell
        }
    }
    if replacement.owns_temporary_array() {
        // `__rt_heap_free` reads its pointer from the INT RESULT register on both targets
        // (`x0`/`rax`), not from the first argument register — those differ on x86_64.
        abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        abi::emit_call_label(ctx.emitter, "__rt_heap_free");
    }
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), 16);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    Ok(())
}

/// Allocates the one-element replacement shell with the requested payload slot width.
fn emit_splice_one_element_array_new(ctx: &mut FunctionContext<'_>, slot_size: i64) {
    let count_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let size_reg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_int_immediate(ctx.emitter, count_reg, 1);
    abi::emit_load_int_immediate(ctx.emitter, size_reg, slot_size);
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
}

/// Writes `array_splice()`'s `$replacement` values into the gap the removal just opened.
///
/// Runs directly after the splice helper, whose removed-elements array and normalized insertion
/// index are still live in the result registers; both are parked on the temporary stack because
/// the insertion can reach `__rt_array_grow`. The by-reference receiver's frame slot is refreshed
/// with the possibly-relocated pointer BEFORE the removed array is restored, because that
/// write-back goes through the integer result register. On return the removed array is back in
/// the integer result register, which is what the caller's result normalization reads.
fn emit_splice_replacement_insert(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    receiver: ReceiverPlace,
    receiver_ty: &PhpType,
    replacement: &SpliceReplacement,
    elem_ty: &PhpType,
) -> Result<()> {
    if !replacement.inserts_values() {
        return Ok(());
    }
    receiver.require_writable("array_splice replacement")?;
    let (removed_reg, at_reg) = splice_result_regs(ctx);
    let (dst_reg, index_reg, replacement_reg) = splice_insert_arg_regs(ctx);
    // Temporary layout after both pushes: [0] = replacement array, [16] = removed array,
    // [24] = normalized insertion index.
    abi::emit_push_reg_pair(ctx.emitter, removed_reg, at_reg);
    emit_splice_replacement_pointer(ctx, replacement)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    ctx.load_value_to_reg(array, dst_reg)?;
    abi::emit_load_temporary_stack_slot(ctx.emitter, index_reg, 24);
    abi::emit_load_temporary_stack_slot(ctx.emitter, replacement_reg, 0);
    emit_splice_boxing_tag(ctx, replacement);
    abi::emit_call_label(
        ctx.emitter,
        array_splice_insert_runtime_helper(replacement, elem_ty),
    );
    ctx.store_result_value(array)?;
    if replacement.owns_temporary_array() {
        // `__rt_heap_free` reads its pointer from the INT RESULT register on both targets
        // (`x0`/`rax`), not from the first argument register — those differ on x86_64.
        abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        abi::emit_call_label(ctx.emitter, "__rt_heap_free");
    }
    receiver.store_back(ctx, array, receiver_ty)?;
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), 16);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    Ok(())
}

/// Lowers `array_count_values()` through the tally-building runtime helpers.
///
/// Associative sources take `__rt_hash_count_values`, which dispatches on each entry's
/// RUNTIME value tag. Indexed sources take `__rt_array_count_values` with the COMPILE-TIME
/// element tag, because an indexed array's payload carries no per-slot tag: the tag selects
/// between the 8-byte integer slot layout, the 16-byte string slot layout, and the boxed
/// `Mixed` pointer layout. Any other element tag makes every entry skippable, which is exactly
/// php-src's behaviour for a `float`/`bool`/array/object element.


/// Lowers `ArrayPtrKey` through the internal-array-pointer backend.
pub(crate) fn lower_array_ptr_key(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    internal_pointer::lower_array_ptr_key(ctx, inst)
}

/// Lowers `ArrayPtrSeek` through the internal-array-pointer backend.
pub(crate) fn lower_array_ptr_seek(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    internal_pointer::lower_array_ptr_seek(ctx, inst)
}

/// Lowers `ArrayPtrValue` through the internal-array-pointer backend.
pub(crate) fn lower_array_ptr_value(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    internal_pointer::lower_array_ptr_value(ctx, inst)
}

/// php-src's verbatim `ValueError` wording for `array_chunk()` with a non-positive `$length`.
const ARRAY_CHUNK_NON_POSITIVE_LENGTH_MESSAGE: &str =
    "array_chunk(): Argument #2 ($length) must be greater than 0";

/// Returns the helper that inserts `$replacement` values with the right ownership handling.
fn array_splice_insert_runtime_helper(
    replacement: &SpliceReplacement,
    elem_ty: &PhpType,
) -> &'static str {
    if replacement.boxing_tag().is_some() {
        return "__rt_array_splice_insert_boxed";
    }
    if matches!(replacement, SpliceReplacement::UnboxedArray(_)) {
        return "__rt_array_splice_insert_unboxed";
    }
    if elem_ty.codegen_repr() == PhpType::Str {
        return "__rt_array_splice_insert_str";
    }
    if elem_ty.is_refcounted() {
        "__rt_array_splice_insert_refcounted"
    } else {
        "__rt_array_splice_insert"
    }
}

/// Materializes the extra value_type-tag argument the boxing insert helper reads.
fn emit_splice_boxing_tag(ctx: &mut FunctionContext<'_>, replacement: &SpliceReplacement) {
    let Some(tag) = replacement.boxing_tag() else {
        return;
    };
    let reg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    abi::emit_load_int_immediate(ctx.emitter, reg, i64::from(tag));
}

/// Materializes the replacement's indexed-array pointer into the integer result register.
///
/// An array argument is loaded directly. A bare scalar is wrapped in a fresh one-element array
/// whose payload slot holds the value; the insert helpers persist or retain what they insert, so
/// the caller frees that temporary shell afterwards without touching the value itself.
fn emit_splice_replacement_pointer(
    ctx: &mut FunctionContext<'_>,
    replacement: &SpliceReplacement,
) -> Result<()> {
    let result_reg = abi::int_result_reg(ctx.emitter);
    match replacement {
        SpliceReplacement::Empty => {
            abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
            Ok(())
        }
        SpliceReplacement::Array(value)
        | SpliceReplacement::BoxedArray(value, _)
        | SpliceReplacement::UnboxedArray(value) => {
            ctx.load_value_to_reg(*value, result_reg)?;
            Ok(())
        }
        SpliceReplacement::Scalar {
            value, scalar_ty, ..
        } if scalar_ty.codegen_repr() == PhpType::Str => {
            emit_splice_one_element_string_array(ctx, *value)
        }
        SpliceReplacement::Scalar { value, .. } => {
            let scratch = abi::secondary_scratch_reg(ctx.emitter);
            ctx.load_value_to_reg(*value, scratch)?;
            abi::emit_push_reg(ctx.emitter, scratch);
            emit_splice_one_element_array_new(ctx, 8);
            abi::emit_pop_reg(ctx.emitter, scratch);
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction("str x10, [x0, #24]");              // store the scalar replacement into the one-element array
                    ctx.emitter.instruction("mov x10, #1");                     // the synthesized replacement array holds exactly one element
                    ctx.emitter.instruction("str x10, [x0]");                   // publish the one-element logical length
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction("mov QWORD PTR [rax + 24], r10");   // store the scalar replacement into the one-element array
                    ctx.emitter.instruction("mov r10, 1");                      // the synthesized replacement array holds exactly one element
                    ctx.emitter.instruction("mov QWORD PTR [rax], r10");        // publish the one-element logical length
                }
            }
            Ok(())
        }
    }
}

/// Lowers `array_count_values()` through the tally-building runtime helpers.
///
/// Associative sources take `__rt_hash_count_values`, which dispatches on each entry's
/// RUNTIME value tag. Indexed sources take `__rt_array_count_values` with the COMPILE-TIME
/// element tag, because an indexed array's payload carries no per-slot tag: the tag selects
/// between the 8-byte integer slot layout, the 16-byte string slot layout, and the boxed
/// `Mixed` pointer layout. Any other element tag makes every entry skippable, which is exactly
/// php-src's behaviour for a `float`/`bool`/array/object element.
pub(crate) fn lower_array_count_values(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "array_count_values", 1)?;
    let array = expect_operand(inst, 0)?;
    let source_ty = ctx.value_php_type(array)?.codegen_repr();
    if matches!(source_ty, PhpType::AssocArray { .. }) {
        ctx.load_value_to_result(array)?;
        if ctx.emitter.target.arch == Arch::X86_64 {
            ctx.emitter.instruction("mov rdi, rax");                            // pass the source hash pointer as the tally helper argument
        }
        abi::emit_call_label(ctx.emitter, "__rt_hash_count_values");
        return store_if_result(ctx, inst);
    }
    let element_tag = array_count_values_element_tag(&source_ty)?;
    ctx.load_value_to_result(array)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("mov x1, #{}", element_tag));             // pass the compile-time element tag to the tally helper
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // pass the source indexed-array pointer as the tally helper argument
            ctx.emitter
                .instruction(&format!("mov rsi, {}", element_tag));             // pass the compile-time element tag to the tally helper
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_count_values");
    store_if_result(ctx, inst)
}

/// Lowers `array_slice($array, $offset, $length, true)` into an owned integer-keyed hash.
///
/// The runtime helper normalizes the PHP window through the same `emit_slice_bounds` prologue as
/// `__rt_array_slice`, then inserts each selected element at its ORIGINAL index, persisting
/// strings and retaining heap payloads, so the result is a freshly owned hash whose keys match
/// PHP's `preserve_keys` output exactly. The checker types this call as
/// `AssocArray { key: Int, value: T }`, which is re-verified here.
fn lower_array_slice_preserve_keys(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
) -> Result<()> {
    let PhpType::Array(_) = ctx.value_php_type(array)?.codegen_repr() else {
        return Err(CodegenIrError::unsupported(format!(
            "array_slice preserve_keys for PHP type {:?}",
            ctx.value_php_type(array)?
        )));
    };
    let PhpType::AssocArray { .. } = inst.result_php_type.codegen_repr() else {
        return Err(CodegenIrError::unsupported(format!(
            "array_slice preserve_keys result PHP type {:?}",
            inst.result_php_type
        )));
    };
    let offset = expect_operand(inst, 1)?;
    let length = slice_like_length_operand(inst)?;
    lower_slice_like_args(ctx, array, offset, length, "array_slice")?;
    abi::emit_call_label(ctx.emitter, "__rt_array_slice_to_hash");
    store_if_result(ctx, inst)
}

/// Calls one of the `__rt_hash_*sort` insertion-order relinking helpers.
///
/// Ordinary receivers are split with `__rt_hash_ensure_unique` first, so an aliased copy taken
/// before the call keeps the original iteration order, and the possibly relocated pointer is
/// written back to the source local before the sorter runs. A hash returned by
/// `MixedCellPromoteAttachedToHash(_)` is already unique and published into its parent-owned Mixed
/// cell; splitting it again would create an opaque clone that cannot be republished. The helpers
/// only rewrite the table's `prev`/`next`/`head`/`tail` links, so no key or value changes ownership.
///
/// The receiver is split with `__rt_hash_ensure_unique` first, so an aliased copy taken
/// before the call keeps the original iteration order, and the possibly relocated pointer
/// is written back to the source local before the sorter runs. The helpers only rewrite
/// the table's `prev`/`next`/`head`/`tail` links, so no key or value changes ownership.
///
/// A local whose FRAME storage is boxed `Mixed` needs both halves of the ownership pairing that
/// `lower_hash_set` already performs, and for the same reason: loading a concrete hash out of a
/// Mixed slot unboxes it with an EXTRA owned reference, so the slot's box and the loaded value each
/// hold one. Releasing the box first (`release_mutated_source_local_owner`) leaves the loaded value
/// sole owner, which is also what stops `__rt_hash_ensure_unique` from splitting a table nothing
/// else aliases; and the write-back must then re-box WITHOUT consuming that reference, because this
/// builtin's EIR releases the receiver after the call. Measured with neither half,
/// `function g(array $a) { $a["k"] = "img2"; natsort($a); return json_encode($a); }` printed the
/// EMPTY string: the table was split, the clone was owned once by the fresh box, released twice,
/// and `json_encode` then read freed storage. `ksort`, `krsort`, `asort` and `arsort` reach this
/// same entry point on a hash and shared the defect.
fn lower_hash_link_sort(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    helper: &str,
) -> Result<()> {
    let array = expect_operand(inst, 0)?;
    let receiver = ReceiverPlace::resolve(ctx, array)?;
    if !hash_sort_source_is_attached_mixed_cell(ctx, array)? {
        if let Some(slot) = receiver.slot() {
            ctx.release_mutated_source_local_owner(slot, array)?;
        }
        ensure_unique_hash_sort_source(ctx, array)?;
        receiver.store_back_borrowed_value(ctx, array)?;
    }
    let array_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    ctx.load_value_to_reg(array, array_arg_reg)?;
    abi::emit_call_label(ctx.emitter, helper);
    let result = if inst.result_php_type.codegen_repr() == PhpType::Bool {
        1
    } else {
        0x7fff_ffff_ffff_fffe
    };
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        result,
    );
    store_if_result(ctx, inst)
}

/// Reports whether `value` is the hash already made unique and republished by a Mixed-cell helper.
fn hash_sort_source_is_attached_mixed_cell(
    ctx: &FunctionContext<'_>,
    value: ValueId,
) -> Result<bool> {
    let value_ref = ctx
        .function
        .value(value)
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))?;
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(false);
    };
    let instruction = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    Ok(matches!(
        (&instruction.op, &instruction.immediate),
        (
            Op::RuntimeCall,
            Some(Immediate::RuntimeCall(
                crate::ir::RuntimeCallTarget::MixedCellPromoteAttachedToHash(_)
            ))
        )
    ))
}

/// Resolves the `array_slice`/`array_splice` length-present flag into the integer result register.
///
/// PHP's `?int $length` treats `null` as "to the end of the array", and every other `i64` — including
/// `-1` — is a real length, so the runtime helpers cannot recognise "no length" from the length value
/// itself. The flag is therefore materialized separately: an omitted or statically `Void` argument is
/// the immediate `0`, a statically typed integer is the immediate `1`, and a boxed `Mixed` argument is
/// unboxed at runtime so a `null` payload (runtime tag 8) also reports `0`.
fn resolve_slice_length_present_to_result(
    ctx: &mut FunctionContext<'_>,
    length: Option<ValueId>,
) -> Result<()> {
    let reg = abi::int_result_reg(ctx.emitter);
    if slice_length_is_statically_absent(ctx, length)? {
        abi::emit_load_int_immediate(ctx.emitter, reg, 0);
        return Ok(());
    }
    let length = length.expect("length present");
    if !matches!(
        ctx.value_php_type(length)?.codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        abi::emit_load_int_immediate(ctx.emitter, reg, 1);
        return Ok(());
    }
    ctx.load_value_to_result(length)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #8");                              // runtime tag 8 marks a boxed PHP null length argument
            ctx.emitter.instruction("cset x0, ne");                             // report a length only when the boxed payload is not null
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 8");                              // runtime tag 8 marks a boxed PHP null length argument
            ctx.emitter.instruction("setne al");                                // report a length only when the boxed payload is not null
            ctx.emitter.instruction("movzx rax, al");                           // widen the length-present flag to a full integer argument word
        }
    }
    Ok(())
}

/// Reports whether the `array_slice`/`array_splice` length argument is absent at compile time.
///
/// A missing operand and a statically `Void` operand both mean the PHP call omitted `$length` (or
/// passed a literal `null`), which selects the "slice to the end of the array" behavior.
fn slice_length_is_statically_absent(
    ctx: &mut FunctionContext<'_>,
    length: Option<ValueId>,
) -> Result<bool> {
    match length {
        None => Ok(true),
        Some(length) => Ok(matches!(
            ctx.value_php_type(length)?.codegen_repr(),
            PhpType::Void
        )),
    }
}

/// Returns the `$length` operand of a slice-like call, or `None` when the argument was omitted.
///
/// PHP's `$length` is the third parameter, so a call that also passes `$preserve_keys` always
/// materializes it — the argument planner fills the gap with the parameter's `null` default.
fn slice_like_length_operand(inst: &Instruction) -> Result<Option<ValueId>> {
    if inst.operands.len() >= 3 {
        return Ok(Some(expect_operand(inst, 2)?));
    }
    Ok(None)
}

/// Reads the literal `$preserve_keys` flag of a slice-like call.
///
/// The checker rejects a non-literal flag because it decides the result's static shape, so a
/// non-literal operand here can only mean the checker and the backend disagree about this call.
fn slice_like_preserve_keys(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<bool> {
    match inst.operands.get(3).copied() {
        None => Ok(false),
        Some(flag) => const_bool_operand(ctx, flag)?.ok_or_else(|| {
            CodegenIrError::unsupported(format!(
                "{} preserve_keys argument that is not a compile-time literal",
                name
            ))
        }),
    }
}

/// Reports whether a mutating array builtin's first operand is a hash-backed array.
fn sort_receiver_is_hash(ctx: &FunctionContext<'_>, inst: &Instruction) -> Result<bool> {
    let array = expect_operand(inst, 0)?;
    Ok(matches!(
        ctx.value_php_type(array)?.codegen_repr(),
        PhpType::AssocArray { .. }
    ))
}

/// Reports whether a natural-order sort's receiver is a hash whose values are strings.
///
/// php's `natsort` orders every value through `zval_get_tmp_string()`, so a faithful sort of
/// non-string values would have to materialize those strings first. The relinking helpers
/// compare the payloads the table already holds, which is exact for string values and only
/// for those — measured: `natsort` puts `-5` before `-10` (it compares `"-5"` against
/// `"-10"`) where `asort` puts `-10` first, so borrowing the numeric comparator for an
/// integer-valued hash would silently produce php's asort order under natsort's name.
/// Every other hash therefore keeps reporting the unsupported-feature error it reports today.
fn natural_sort_receiver_is_string_hash(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<bool> {
    let array = expect_operand(inst, 0)?;
    let PhpType::AssocArray { value, .. } = ctx.value_php_type(array)?.codegen_repr() else {
        return Ok(false);
    };
    Ok(value.codegen_repr() == PhpType::Str)
}

/// The `__rt_array_splice_insert*` argument registers: destination, index, replacement.
fn splice_insert_arg_regs(
    ctx: &FunctionContext<'_>,
) -> (&'static str, &'static str, &'static str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => ("x0", "x1", "x2"),
        Arch::X86_64 => ("rdi", "rsi", "rdx"),
    }
}

/// The registers `__rt_array_splice*` leaves the removed array and the insertion index in.
fn splice_result_regs(ctx: &FunctionContext<'_>) -> (&'static str, &'static str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => ("x0", "x1"),
        Arch::X86_64 => ("rax", "rdx"),
    }
}

/// Wraps a bare string replacement in a one-element 16-byte-slot indexed array.
///
/// The shell holds the caller's borrowed pointer/length pair: `__rt_array_splice_insert_str`
/// duplicates it with `__rt_str_persist` and `__rt_array_splice_insert_boxed` persists it into a
/// Mixed cell, so freeing the shell afterwards never touches the string bytes.
fn emit_splice_one_element_string_array(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
) -> Result<()> {
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    // The pointer/length pair only both land in the canonical string result registers through
    // the result loader; a single-register load leaves the length slot holding stale bytes.
    ctx.load_value_to_result(value)?;
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);
    emit_splice_one_element_array_new(ctx, 16);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldp x10, x11, [sp], #16");                 // pop the borrowed string pointer/length pair after the constructor
            ctx.emitter.instruction("stp x10, x11, [x0, #24]");                 // store the borrowed pair into the one-element string array
            ctx.emitter.instruction("mov x10, #1");                             // the synthesized replacement array holds exactly one element
            ctx.emitter.instruction("str x10, [x0]");                           // publish the one-element logical length
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r10, QWORD PTR [rsp]");                // reload the borrowed string pointer after the constructor
            ctx.emitter.instruction("mov r11, QWORD PTR [rsp + 8]");            // reload the borrowed string length after the constructor
            ctx.emitter.instruction("add rsp, 16");                             // release the borrowed string pointer/length staging slot
            ctx.emitter.instruction("mov QWORD PTR [rax + 24], r10");           // store the borrowed string pointer into the one-element string array
            ctx.emitter.instruction("mov QWORD PTR [rax + 32], r11");           // store the borrowed string length into the one-element string array
            ctx.emitter.instruction("mov r10, 1");                              // the synthesized replacement array holds exactly one element
            ctx.emitter.instruction("mov QWORD PTR [rax], r10");                // publish the one-element logical length
        }
    }
    Ok(())
}

impl SpliceReplacement {
    /// Classifies the `$replacement` operand against the receiver's element type.
    ///
    /// Four shapes are accepted, in this order: an indexed array whose element type already
    /// matches the receiver's payload slots; an indexed array of typed scalars going into a
    /// heterogeneous `array<mixed>` receiver, whose values are boxed one at a time; an indexed
    /// array of boxed `Mixed` cells going into an `array<int>`/`array<bool>` receiver, which is
    /// what an overflow-checked expression such as `[$x + 1]` produces; and a bare non-refcounted
    /// scalar of the receiver's element type, which PHP casts to a one-element array. Anything
    /// else — an array the backend cannot re-represent in the receiver's slots, a bare boxed
    /// `Mixed` value, a `Str` receiver (whose payload slots are wider than the splice helpers
    /// move) — is an explicit `unsupported` diagnostic rather than a mistyped insertion.
    fn resolve(
        ctx: &mut FunctionContext<'_>,
        replacement: Option<ValueId>,
        elem_ty: &PhpType,
    ) -> Result<Self> {
        let Some(replacement) = replacement else {
            return Ok(Self::Empty);
        };
        let replacement_ty = ctx.value_php_type(replacement)?.codegen_repr();
        if matches!(replacement_ty, PhpType::Void | PhpType::Never) {
            // A literal `null` replacement, or the registry's own `[]` default.
            return Ok(Self::Empty);
        }
        if let PhpType::Array(inner) = &replacement_ty {
            let inner = inner.codegen_repr();
            if matches!(inner, PhpType::Void | PhpType::Never) {
                return Ok(Self::Empty);
            }
            if &inner == elem_ty && splice_insert_slot_is_supported(elem_ty) {
                return Ok(Self::Array(replacement));
            }
            // A heterogeneous receiver stores boxed Mixed cells, so a typed scalar replacement
            // has to be boxed element by element before it lands in the payload.
            if elem_ty == &PhpType::Mixed && splice_boxable_scalar_slot(&inner) {
                let tag = runtime_value_tag("array_splice", &inner)?;
                return Ok(Self::BoxedArray(replacement, tag));
            }
            // The mirror case: an overflow-checked expression boxes its result, so
            // `[$x + 1, $x + 2]` is an `array<mixed>` even for an `array<int>` receiver. Read
            // each cell back as a plain integer instead of storing the cell pointer.
            if inner == PhpType::Mixed && matches!(elem_ty, PhpType::Int | PhpType::Bool) {
                return Ok(Self::UnboxedArray(replacement));
            }
        }
        if &replacement_ty == elem_ty
            && splice_insert_slot_is_supported(elem_ty)
            && !elem_ty.is_refcounted()
        {
            return Ok(Self::Scalar {
                value: replacement,
                scalar_ty: replacement_ty,
                boxed_tag: None,
            });
        }
        // A bare scalar into a heterogeneous receiver: PHP casts it to a one-element array, and
        // that element has to be boxed exactly like an array replacement's elements are.
        if elem_ty == &PhpType::Mixed && splice_boxable_scalar_slot(&replacement_ty) {
            let tag = runtime_value_tag("array_splice", &replacement_ty)?;
            return Ok(Self::Scalar {
                value: replacement,
                scalar_ty: replacement_ty,
                boxed_tag: Some(tag),
            });
        }
        Err(CodegenIrError::unsupported(format!(
            "array_splice replacement PHP type {:?} for indexed-array element PHP type {:?}. \
             PHP would make the receiver heterogeneous, which needs an `array<mixed>` receiver \
             slot; a by-reference parameter, a `&$x` binding, and an object/static property \
             receiver all share their storage with a slot this call cannot retype",
            replacement_ty, elem_ty
        )))
    }

    /// Reports whether any element has to be written into the removal gap at run time.
    fn inserts_values(&self) -> bool {
        !matches!(self, Self::Empty)
    }

    /// Reports whether the helper receives a one-element array this lowering allocated.
    fn owns_temporary_array(&self) -> bool {
        matches!(self, Self::Scalar { .. })
    }

    /// Returns the runtime value_type tag the boxing insert helper reads, when boxing applies.
    fn boxing_tag(&self) -> Option<u8> {
        match self {
            Self::BoxedArray(_, tag) => Some(*tag),
            Self::Scalar { boxed_tag, .. } => *boxed_tag,
            _ => None,
        }
    }
}

/// Reports whether a replacement element type can be boxed one slot at a time into a Mixed cell.
///
/// `__rt_mixed_from_value` stores the raw payload word without retaining it, so only the
/// non-refcounted scalars whose slot IS the value qualify. `Str` qualifies too: the boxing helper
/// reads its wider pointer/length slot and `__rt_mixed_from_value` persists the bytes itself, so
/// the Mixed cell owns storage the replacement array still holds independently.
fn splice_boxable_scalar_slot(elem_ty: &PhpType) -> bool {
    matches!(
        elem_ty,
        PhpType::Int | PhpType::Bool | PhpType::Float | PhpType::Str
    )
}

/// Reports whether the splice insert helpers can move this element type's payload slots.
///
/// Every scalar and refcounted element representation qualifies. `Str` reaches a dedicated
/// helper rather than the shared 8-byte one, because indexed string arrays store 16-byte
/// `{pointer, length}` slots that must not be moved eight bytes at a time.
fn splice_insert_slot_is_supported(elem_ty: &PhpType) -> bool {
    matches!(
        elem_ty,
        PhpType::Int | PhpType::Bool | PhpType::Float | PhpType::Str | PhpType::Callable
    ) || elem_ty.is_refcounted()
}

/// php-src's verbatim `ValueError` wording for a negative `range()` `$step` on an increasing range.
const RANGE_NEGATIVE_STEP_MESSAGE: &str =
    "range(): Argument #3 ($step) must be greater than 0 for increasing ranges";

/// php-src's verbatim `ValueError` wording for a `range()` `$step` wider than the spanned interval.
const RANGE_STEP_TOO_WIDE_MESSAGE: &str = "range(): Argument #3 ($step) must be less than the range spanned by argument #1 ($start) and argument #2 ($end)";

/// php-src's verbatim `ValueError` wording for `range()` with a zero `$step`.
const RANGE_ZERO_STEP_MESSAGE: &str = "range(): Argument #3 ($step) cannot be 0";

/// Returns whether `array_search(..., strict: true)` can never match for these static types.
///
/// PHP's `===` requires identical types, so a scalar needle can never match an element of a
/// different scalar type (`array_search(1, [true, false], true)` is `false` while the loose
/// form finds index 0). A boxed `Mixed` element type carries its type tag at runtime and is
/// therefore never statically impossible.
fn array_search_strict_never_matches(needle_ty: &PhpType, array_ty: &PhpType) -> bool {
    let needle_ty = needle_ty.clone().codegen_repr();
    let element_ty = match array_ty.clone().codegen_repr() {
        PhpType::Array(elem) => elem.codegen_repr(),
        PhpType::AssocArray { value, .. } => value.codegen_repr(),
        _ => return false,
    };
    matches!(needle_ty, PhpType::Int | PhpType::Bool | PhpType::Str)
        && matches!(element_ty, PhpType::Int | PhpType::Bool | PhpType::Str)
        && needle_ty != element_ty
}

/// Emits `array_search()`'s ordinary (non-strict-impossible) element scan.
///
/// Leaves the boxed `int|string|false` result in the integer result register for the caller's
/// `store_if_result`, dispatching between the associative, empty, scalar, and string paths.
fn lower_array_search_loose(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
    needle_ty: PhpType,
    array_ty: PhpType,
) -> Result<()> {
    if search::try_lower_assoc_array_search(
        ctx,
        needle,
        array,
        needle_ty.clone(),
        array_ty.clone(),
    )? {
        return Ok(());
    }
    match supported_array_search_case(needle_ty, array_ty)? {
        ArraySearchCase::Empty => box_array_search_miss(ctx),
        ArraySearchCase::Scalar => lower_array_search_scalar(ctx, needle, array)?,
        ArraySearchCase::String => lower_array_search_string(ctx, needle, array)?,
    }
    Ok(())
}
