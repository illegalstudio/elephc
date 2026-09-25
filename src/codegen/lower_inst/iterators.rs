//! Purpose:
//! Lowers high-level EIR iterator opcodes for the Phase 04 backend.
//! Handles stack-resident iteration over indexed and associative arrays.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - `IterStart` names an addressable stack state for source, cursor, and current hash payload.
//! - A successful `IteratorAggregate::getIterator()` result is transferred into the
//!   optional Mixed owner slot before the raw iterator pointer is published. The
//!   source word then borrows that payload; the aggregate word is overwritten
//!   without being released. `IterEnd` releases owned successor-key anchors. The optional owner
//!   in the `IterStart` immediate is the lowering decision for whether the aggregate probe is
//!   reachable; backend value types may have widened after that decision.
//! - Current values are boxed into `Mixed` unless EIR preserves a concrete indexed-array element type.
//! - A source that is not iterable does NOT abort. Every dispatch that misses the
//!   indexed/hash/object cases — the static `NonIterable` kind, the `__rt_mixed_unbox` tag
//!   dispatch, and the `__rt_heap_kind` dispatch — calls `__rt_warn_foreach_non_iterable`
//!   (or, past `IterStart`, simply reports "no more elements") and parks the iterator in the
//!   empty state built by `emit_empty_iterator_state`. That mirrors php-src, which raises
//!   `foreach() argument must be of type array|object, <type> given` and continues. These
//!   paths previously called `__rt_iterable_unsupported_kind`, which printed a
//!   compiler-internal fatal and exited 70; that helper now only serves the SPL and
//!   `IteratorIterator` sites, where the shape really is unsupported.

use crate::codegen::platform::Arch;
use crate::codegen::{
    abi, emit_box_current_owned_value_as_mixed, emit_box_current_value_as_mixed,
    emit_box_runtime_payload_as_mixed,
};
use crate::intrinsics::IntrinsicCall;
use crate::ir::{Immediate, Instruction, LocalSlotId, Op, ValueDef, ValueId};
use crate::names::php_symbol_key;
use crate::types::PhpType;

use super::super::context::FunctionContext;
use super::{
    direct_call_stack_pad_bytes, emit_direct_resolved_method_call, expect_local_slot,
    expect_operand, resolve_method_call_target, store_if_result,
};
use crate::codegen::{CodegenIrError, Result};

const ITER_SOURCE_OFFSET_DELTA: usize = 0;
const ITER_CURSOR_OFFSET_DELTA: usize = 8;
const ITER_KEY_LO_OFFSET_DELTA: usize = 16;
const ITER_KEY_HI_OFFSET_DELTA: usize = 24;
const ITER_VALUE_LO_OFFSET_DELTA: usize = 32;
const ITER_VALUE_HI_OFFSET_DELTA: usize = 40;
const ITER_VALUE_TAG_OFFSET_DELTA: usize = 48;
const ITER_VALUE_ADDR_OFFSET_DELTA: usize = 56;
const ITER_SNAPSHOT_LEN_OFFSET_DELTA: usize = 64;
/// Table pointer the current associative cursor was computed against.
///
/// `IterNext` compares the live container published by the loop body against this word. A
/// mismatch means growth or a copy-on-write split replaced the table, so the source word is
/// republished before key-identity validation rebuilds the cursor from the successor anchors.
const ITER_TABLE_SNAPSHOT_OFFSET_DELTA: usize = 72;
/// Owned keys of the next two entries the cursor can yield, used after table relocation.
///
/// String keys carry one retain per occupied anchor word pair. The primary anchor is the immediate
/// successor. The fallback anchor is its successor, so deleting the immediate successor before a
/// grow still resumes at the first surviving entry. Integer anchors carry no heap ownership.
const ITER_NEXT_KEY_LO_OFFSET_DELTA: usize = 80;
const ITER_NEXT_KEY_HI_OFFSET_DELTA: usize = 88;
const ITER_FALLBACK_KEY_LO_OFFSET_DELTA: usize = 96;
const ITER_FALLBACK_KEY_HI_OFFSET_DELTA: usize = 104;
/// Key high word meaning "no successor entry to resume from".
///
/// A live key uses -1 for an integer key and a non-negative length for a string key, so this can
/// never collide with one. Must match `NO_SUCCESSOR_KEY_MARKER` in the runtime emitter.
const NO_SUCCESSOR_KEY_MARKER: i64 = -2;
const MIXED_CELL_PAYLOAD_LOW_OFFSET: usize = 8;

/// The runtime value tag `__rt_warn_foreach_non_iterable` reads as "null".
///
/// Matches `crate::codegen_support::value_boxing::runtime_value_tag(&PhpType::Void)`.
const NULL_VALUE_TAG: i64 = 8;

/// The runtime value tag for a boolean, whose warning text depends on the payload.
///
/// Matches `crate::codegen_support::value_boxing::runtime_value_tag(&PhpType::Bool)`.
const BOOL_VALUE_TAG: u8 = 3;

enum IteratorSourceKind {
    Indexed { elem: PhpType },
    Hash,
    DynamicIterable,
    DynamicMixed,
    /// A source whose STATIC type can never be iterated (`int`, `string`, `bool`, `null`, …).
    ///
    /// PHP does not reject this at compile time: `foreach (false as $x)` warns and skips the
    /// loop. The checker mirrors that with a compile warning (`src/types/checker/stmt_check/
    /// control_flow.rs`) and codegen emits the same E_WARNING at runtime, then leaves the
    /// iterator state empty so `IterNext` reports "no more elements" on its first probe.
    NonIterable {
        /// Runtime value tag naming the offending value in the warning.
        value_tag: u8,
    },
    Object {
        class_name: String,
        aggregate_class_name: Option<String>,
    },
    Interface {
        interface_name: String,
        aggregate_class_name: Option<String>,
    },
}

/// Lowers iterator initialization by storing the source pointer and initial cursor.
pub(super) fn lower_iter_start(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let source = expect_operand(inst, 0)?;
    let source_kind = iterator_source_kind_from_type(ctx, &ctx.value_php_type(source)?, inst)?;
    let by_ref = iter_start_is_by_ref(inst);
    let owner = iter_start_owner_slot(inst);
    if inst.result.is_none() {
        return Err(CodegenIrError::invalid_module(
            "iter_start missing result value".to_string(),
        ));
    }
    let offset = ctx.local_offset(iter_start_state_slot(inst)?)?;
    // -- statically non-iterable sources warn and skip before any value is loaded --
    // A `float` source lives in `d0`, not the integer result register, so this must run
    // BEFORE the unconditional `load_value_to_reg` below.
    if let IteratorSourceKind::NonIterable { value_tag } = source_kind {
        return initialize_non_iterable_iterator(ctx, offset, value_tag, source);
    }
    let result_reg = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(source, result_reg)?;
    if matches!(source_kind, IteratorSourceKind::DynamicMixed) {
        initialize_dynamic_mixed_iterator(ctx, offset, by_ref, owner)?;
        if iter_start_origin(ctx, inst).is_some() {
            emit_snapshot_origin_container(ctx, offset);
        }
        return Ok(());
    }
    if by_ref {
        ensure_unique_static_iter_source(ctx, source, &source_kind)?;
        if matches!(
            source_kind,
            IteratorSourceKind::Indexed { .. } | IteratorSourceKind::Hash
        ) {
            // -- normalize a missed-read sentinel source to the canonical zero pointer --
            // Only the iterator's private slot is normalized; the origin local keeps the
            // sentinel so the user-visible value stays null. `IterNext` re-reads the live
            // length from this slot every iteration, so folding the sentinel here keeps
            // that hot path on the cheap zero check (issue #556).
            crate::codegen::sentinels::emit_normalize_null_container_to_zero(
                ctx.emitter,
                result_reg,
                abi::secondary_scratch_reg(ctx.emitter),
            );
        }
    }
    abi::store_at_offset(ctx.emitter, result_reg, offset - ITER_SOURCE_OFFSET_DELTA);
    if matches!(source_kind, IteratorSourceKind::DynamicIterable) {
        initialize_dynamic_iterable_iterator(ctx, offset, by_ref, source, owner)?;
        if iter_start_origin(ctx, inst).is_some() {
            emit_snapshot_origin_container(ctx, offset);
        }
        return Ok(());
    }
    // -- the loop's reference on an object source is taken by EIR lowering, not here --
    // `IterStart` used to `incref` an `Object`/`Interface` source so the object stayed
    // alive for the whole loop, but nothing ever emitted the matching `decref`: every
    // `foreach` over an Iterator (a `Generator` included) leaked the object and every
    // heap block it owned. `lower_foreach` now wraps a borrowed object source in an
    // `Op::Acquire`, which the loop's existing exit/`LoopCleanup` release paths balance.
    let initial_cursor = match &source_kind {
        IteratorSourceKind::Indexed { .. } if by_ref => 0,
        IteratorSourceKind::Indexed { .. } => -1,
        IteratorSourceKind::Hash => 0,
        IteratorSourceKind::DynamicIterable => 0,
        IteratorSourceKind::DynamicMixed => 0,
        // Unreachable: `lower_iter_start` returns before this point for a non-iterable.
        IteratorSourceKind::NonIterable { .. } => 0,
        IteratorSourceKind::Object { .. } => 0,
        IteratorSourceKind::Interface { .. } => 0,
    };
    match &source_kind {
        IteratorSourceKind::Object {
            aggregate_class_name: Some(aggregate_class_name),
            ..
        }
        | IteratorSourceKind::Interface {
            aggregate_class_name: Some(aggregate_class_name),
            ..
        } => {
            let return_ty =
                emit_object_iterator_method_call(ctx, offset, aggregate_class_name, "getIterator")?;
            adopt_get_iterator_result(ctx, offset, owner, &return_ty)?;
        }
        IteratorSourceKind::Interface { interface_name, .. }
            if interface_needs_get_iterator(ctx, interface_name) =>
        {
            let return_ty = emit_interface_iterator_method_call(
                ctx,
                offset,
                interface_name,
                "getIterator",
            )?;
            adopt_get_iterator_result(ctx, offset, owner, &return_ty)?;
        }
        _ => {}
    }
    store_iterator_cursor(ctx, offset, initial_cursor);
    if iter_start_origin(ctx, inst).is_some() {
        emit_snapshot_origin_container(ctx, offset);
    }
    if !by_ref && matches!(source_kind, IteratorSourceKind::Indexed { .. }) {
        snapshot_indexed_array_length(ctx, offset);
    }
    match source_kind {
        IteratorSourceKind::Object { class_name, .. } => {
            emit_object_iterator_method_call(ctx, offset, &class_name, "rewind")?;
        }
        IteratorSourceKind::Interface { interface_name, .. } => {
            let rewind_interface = if interface_needs_get_iterator(ctx, &interface_name) {
                "Iterator"
            } else {
                interface_name.as_str()
            };
            emit_interface_iterator_method_call(ctx, offset, rewind_interface, "rewind")?;
        }
        _ => {}
    }
    Ok(())
}

/// Lowers iterator advancement into a boolean result without moving past end.
pub(super) fn lower_iter_next(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let iterator = expect_operand(inst, 0)?;
    let offset = iterator_state_offset(ctx, iterator, inst)?;
    let by_ref = iterator_is_by_ref(ctx, iterator, inst)?;
    // -- republished containers are picked up BEFORE any dispatch reads the source word --
    // `branch_on_dynamic_source_heap_kind` probes the heap header of the source pointer, so a
    // table freed by growth or replaced by a copy-on-write split has to be swapped out here
    // rather than inside the hash arm, or the dispatch itself reads released storage.
    let origin = iter_origin(ctx, iterator, inst)?;
    let has_origin = origin.is_some();
    if let Some(origin) = origin {
        emit_reload_live_iter_source(ctx, offset, origin);
    }
    if has_origin {
        // A deletion followed by insertion can reuse the saved cursor's physical tombstone slot
        // without changing the table pointer. Rebuild from key identity before consuming anchors.
        emit_validate_hash_cursor_anchor(ctx, offset);
        emit_release_successor_keys(ctx, offset);
    }
    match iterator_source_kind(ctx, iterator, inst)? {
        IteratorSourceKind::Indexed { .. } if by_ref => match ctx.emitter.target.arch {
            Arch::AArch64 => lower_hash_iter_next_aarch64(ctx, offset, has_origin),
            Arch::X86_64 => lower_hash_iter_next_x86_64(ctx, offset, has_origin),
        },
        IteratorSourceKind::Indexed { .. } => match ctx.emitter.target.arch {
            Arch::AArch64 => lower_indexed_iter_next_aarch64(ctx, offset, false),
            Arch::X86_64 => lower_indexed_iter_next_x86_64(ctx, offset, false),
        },
        IteratorSourceKind::Hash => match ctx.emitter.target.arch {
            Arch::AArch64 => lower_hash_iter_next_aarch64(ctx, offset, has_origin),
            Arch::X86_64 => lower_hash_iter_next_x86_64(ctx, offset, has_origin),
        },
        IteratorSourceKind::DynamicIterable | IteratorSourceKind::DynamicMixed => {
            lower_dynamic_iter_next(ctx, offset, by_ref, has_origin)?;
        }
        // A non-iterable source has no elements: report "loop finished" on the first probe
        // so the body never runs and the statement after the loop still executes.
        IteratorSourceKind::NonIterable { .. } => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
        IteratorSourceKind::Object { class_name, .. } => {
            lower_object_iter_next(ctx, offset, &class_name)?;
        }
        IteratorSourceKind::Interface { interface_name, .. } => {
            lower_interface_iter_next(ctx, offset, &interface_name)?;
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers the current iterator key by boxing it as a `Mixed` value.
pub(super) fn lower_iter_current_key(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let iterator = expect_operand(inst, 0)?;
    let offset = iterator_state_offset(ctx, iterator, inst)?;
    match iterator_source_kind(ctx, iterator, inst)? {
        IteratorSourceKind::Indexed { .. } if iterator_is_by_ref(ctx, iterator, inst)? => {
            match ctx.emitter.target.arch {
                Arch::AArch64 => load_current_hash_key_as_mixed_aarch64(ctx, offset),
                Arch::X86_64 => load_current_hash_key_as_mixed_x86_64(ctx, offset),
            }
        }
        IteratorSourceKind::Indexed { .. } => {
            let result_reg = abi::int_result_reg(ctx.emitter);
            abi::load_at_offset(ctx.emitter, result_reg, offset - ITER_CURSOR_OFFSET_DELTA);
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
        }
        IteratorSourceKind::Hash => match ctx.emitter.target.arch {
            Arch::AArch64 => load_current_hash_key_as_mixed_aarch64(ctx, offset),
            Arch::X86_64 => load_current_hash_key_as_mixed_x86_64(ctx, offset),
        },
        IteratorSourceKind::DynamicIterable | IteratorSourceKind::DynamicMixed => {
            lower_dynamic_iter_current_key(ctx, inst, offset)?;
        }
        // Dead code in practice — `IterNext` already reported "no more elements" — but the
        // block is still emitted, so materialize a null Mixed rather than reading the
        // uninitialized iterator state.
        IteratorSourceKind::NonIterable { .. } => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
        IteratorSourceKind::Object { class_name, .. } => {
            let return_ty = emit_object_iterator_method_call(ctx, offset, &class_name, "key")?;
            box_iterator_method_result_if_needed(ctx, inst, &return_ty)?;
        }
        IteratorSourceKind::Interface { interface_name, .. } => {
            let return_ty = emit_interface_iterator_method_call(ctx, offset, &interface_name, "key")?;
            box_iterator_method_result_if_needed(ctx, inst, &return_ty)?;
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers the current iterator value into the EIR result representation.
pub(super) fn lower_iter_current_value(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let iterator = expect_operand(inst, 0)?;
    let offset = iterator_state_offset(ctx, iterator, inst)?;
    let result_ty = iter_current_result_type(ctx, inst)?;
    let preserve_reference = inst.immediate == Some(Immediate::Bool(true));
    let promote_reference = inst.immediate == Some(Immediate::I64(1));
    match iterator_source_kind(ctx, iterator, inst)? {
        IteratorSourceKind::Indexed { elem } => {
            match ctx.emitter.target.arch {
                Arch::AArch64 => load_current_array_value_aarch64(ctx, offset, &elem)?,
                Arch::X86_64 => load_current_array_value_x86_64(ctx, offset, &elem)?,
            }
            retain_current_indexed_value_if_unboxed(&mut ctx.emitter, &elem, &result_ty);
            box_current_indexed_value_if_needed(ctx, &elem, &result_ty)?;
        }
        IteratorSourceKind::Hash => load_current_hash_call_value_as_mixed(ctx, offset, preserve_reference, promote_reference),
        IteratorSourceKind::DynamicIterable | IteratorSourceKind::DynamicMixed => {
            lower_dynamic_iter_current_value(ctx, inst, offset, preserve_reference, promote_reference)?;
        }
        // Dead code in practice — `IterNext` already reported "no more elements" — but the
        // block is still emitted, so materialize a null Mixed rather than reading the
        // uninitialized iterator state.
        IteratorSourceKind::NonIterable { .. } => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
        IteratorSourceKind::Object { class_name, .. } => {
            let return_ty = emit_object_iterator_method_call(ctx, offset, &class_name, "current")?;
            box_iterator_method_result_if_needed(ctx, inst, &return_ty)?;
        }
        IteratorSourceKind::Interface { interface_name, .. } => {
            let return_ty = emit_interface_iterator_method_call(ctx, offset, &interface_name, "current")?;
            box_iterator_method_result_if_needed(ctx, inst, &return_ty)?;
        }
    }
    store_if_result(ctx, inst)
}

/// Returns the declared PHP result type for an `iter_current_value` instruction.
fn iter_current_result_type(ctx: &FunctionContext<'_>, inst: &Instruction) -> Result<PhpType> {
    let Some(result) = inst.result else {
        return Ok(PhpType::Void);
    };
    Ok(ctx.value_php_type(result)?.codegen_repr())
}

/// Retains concrete indexed-array foreach values that are returned without Mixed boxing.
fn retain_current_indexed_value_if_unboxed(
    emitter: &mut crate::codegen::emit::Emitter,
    elem: &PhpType,
    result_ty: &PhpType,
) {
    if elem.codegen_repr() == result_ty.codegen_repr() {
        abi::emit_incref_if_refcounted(emitter, &elem.codegen_repr());
    }
}

/// Boxes an indexed iterator element only when the EIR result expects `Mixed`.
fn box_current_indexed_value_if_needed(
    ctx: &mut FunctionContext<'_>,
    elem: &PhpType,
    result_ty: &PhpType,
) -> Result<()> {
    match result_ty.codegen_repr() {
        PhpType::Mixed | PhpType::Union(_) => {
            emit_box_current_value_as_mixed(ctx.emitter, elem);
            Ok(())
        }
        result_ty if result_ty == elem.codegen_repr() => Ok(()),
        other => Err(CodegenIrError::unsupported(format!(
            "indexed iterator value PHP type {:?} stored as {:?}",
            elem,
            other
        ))),
    }
}

/// Binds a local slot to the current iterator value address for by-reference foreach.
pub(super) fn lower_iter_current_value_ref(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let iterator = expect_operand(inst, 0)?;
    let slot = expect_local_slot(inst)?;
    let offset = iterator_state_offset(ctx, iterator, inst)?;
    match iterator_source_kind(ctx, iterator, inst)? {
        IteratorSourceKind::Indexed { .. } => bind_hash_current_value_ref(ctx, offset, slot)?,
        IteratorSourceKind::Hash => {
            bind_hash_current_value_ref(ctx, offset, slot)?;
        }
        IteratorSourceKind::DynamicIterable | IteratorSourceKind::DynamicMixed => {
            bind_dynamic_current_value_ref(ctx, offset, slot)?;
        }
        // Dead code in practice — `IterNext` already reported "no more elements" — but bind
        // the slot to a null cell so nothing downstream dereferences stack garbage.
        IteratorSourceKind::NonIterable { .. } => {
            ctx.release_counted_ref_binding(slot);
            let local_offset = ctx.local_offset(slot)?;
            let result_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
            abi::store_at_offset(ctx.emitter, result_reg, local_offset);
        }
        IteratorSourceKind::Object { .. } | IteratorSourceKind::Interface { .. } => {
            return Err(CodegenIrError::unsupported(
                "by-reference foreach over object iterators in EIR backend",
            ))
        }
    }
    ctx.record_promoted_ref_cell(slot);
    Ok(())
}

/// Initializes an `Iterable`-typed iterator by dispatching on the source heap kind.
fn initialize_dynamic_iterable_iterator(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
    by_ref: bool,
    source: ValueId,
    owner: Option<LocalSlotId>,
) -> Result<()> {
    let indexed_case = ctx.next_label("iter_start_dyn_indexed");
    let hash_case = ctx.next_label("iter_start_dyn_hash");
    let object_case = ctx.next_label("iter_start_dyn_object");
    let done = ctx.next_label("iter_start_dyn_done");
    branch_on_dynamic_source_heap_kind(ctx, offset, &indexed_case, &hash_case, &object_case);
    // -- the source is not a live array/object: warn like PHP and iterate zero times --
    // The STATIC type here is `array`/`iterable`, so the only PHP-reachable value that
    // misses every heap kind is a null container (a missed read, or an uninitialized
    // `iterable` local); report it as `null given`.
    emit_foreach_non_iterable_warning(ctx, NULL_VALUE_TAG);
    emit_empty_iterator_state(ctx, offset);
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&indexed_case);
    if by_ref {
        convert_dynamic_indexed_source_for_ref(ctx, offset)?;
        store_iter_source_to_origin_if_local(ctx, offset, source)?;
    }
    store_iterator_cursor(ctx, offset, if by_ref { 0 } else { -1 });
    if !by_ref {
        snapshot_indexed_array_length(ctx, offset);
    }
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&hash_case);
    if by_ref {
        convert_dynamic_hash_source_for_ref(ctx, offset)?;
        store_iter_source_to_origin_if_local(ctx, offset, source)?;
    }
    store_iterator_cursor(ctx, offset, 0);
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&object_case);
    store_iterator_cursor(ctx, offset, 0);
    resolve_dynamic_object_iterator_source(ctx, offset, owner)?;
    emit_interface_iterator_method_call(ctx, offset, "Iterator", "rewind")?;
    ctx.emitter.label(&done);
    Ok(())
}

/// Initializes a `Mixed`-typed iterator by unboxing the source once into raw iterable state.
fn initialize_dynamic_mixed_iterator(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
    by_ref: bool,
    owner: Option<LocalSlotId>,
) -> Result<()> {
    let indexed_case = ctx.next_label("iter_start_mixed_indexed");
    let hash_case = ctx.next_label("iter_start_mixed_hash");
    let object_case = ctx.next_label("iter_start_mixed_object");
    let done = ctx.next_label("iter_start_mixed_done");
    if by_ref {
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    }
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    branch_on_mixed_iterable_tag(ctx, &indexed_case, &hash_case, &object_case);
    // -- the unboxed value is a scalar/null: warn like PHP and iterate zero times --
    // `__rt_mixed_unbox` left the concrete runtime tag and payload low word in exactly the
    // registers `__rt_warn_foreach_non_iterable` expects, so the offending value names
    // itself (`false`, `int`, `string`, …) with no extra probing.
    abi::emit_call_label(ctx.emitter, "__rt_warn_foreach_non_iterable");
    emit_empty_iterator_state(ctx, offset);
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&indexed_case);
    if by_ref {
        convert_mixed_indexed_source_for_ref(ctx, offset)?;
    } else {
        store_mixed_payload_low_as_iterator_source(ctx, offset);
    }
    store_iterator_cursor(ctx, offset, if by_ref { 0 } else { -1 });
    if !by_ref {
        snapshot_indexed_array_length(ctx, offset);
    }
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&hash_case);
    if by_ref {
        convert_mixed_hash_source_for_ref(ctx, offset)?;
    } else {
        store_mixed_payload_low_as_iterator_source(ctx, offset);
    }
    store_iterator_cursor(ctx, offset, 0);
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&object_case);
    store_mixed_payload_low_as_iterator_source(ctx, offset);
    store_iterator_cursor(ctx, offset, 0);
    resolve_dynamic_object_iterator_source(ctx, offset, owner)?;
    emit_interface_iterator_method_call(ctx, offset, "Iterator", "rewind")?;
    ctx.emitter.label(&done);
    if by_ref {
        abi::emit_pop_reg(ctx.emitter, abi::temp_int_reg(ctx.emitter.target));
    }
    Ok(())
}

/// Initializes an iterator whose source is statically non-iterable.
///
/// PHP's `ZEND_FE_RESET_R` warns and skips the loop rather than aborting, so this emits the
/// same `E_WARNING` and then parks the iterator in the empty state that
/// `lower_iter_next`'s `NonIterable` arm reports as "finished". The value itself is never
/// consumed except for a `bool`, whose payload chooses between `true` and `false` in the
/// message — PHP names the VALUE, not the declared type.
fn initialize_non_iterable_iterator(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
    value_tag: u8,
    source: ValueId,
) -> Result<()> {
    if value_tag == BOOL_VALUE_TAG {
        let result_reg = abi::int_result_reg(ctx.emitter);
        ctx.load_value_to_reg(source, result_reg)?;
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("mov x1, x0");                          // pass the bool payload so the warning can print true or false
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov rdi, rax");                        // pass the bool payload so the warning can print true or false
            }
        }
    } else {
        let payload_reg = match ctx.emitter.target.arch {
            Arch::AArch64 => "x1",
            Arch::X86_64 => "rdi",
        };
        abi::emit_load_int_immediate(ctx.emitter, payload_reg, 0);
    }
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        i64::from(value_tag),
    );
    abi::emit_call_label(ctx.emitter, "__rt_warn_foreach_non_iterable");
    emit_empty_iterator_state(ctx, offset);
    Ok(())
}

/// Emits the PHP `foreach()` non-iterable warning for a statically known runtime value tag.
fn emit_foreach_non_iterable_warning(ctx: &mut FunctionContext<'_>, value_tag: i64) {
    let payload_reg = match ctx.emitter.target.arch {
        Arch::AArch64 => "x1",
        Arch::X86_64 => "rdi",
    };
    abi::emit_load_int_immediate(ctx.emitter, payload_reg, 0);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), value_tag);
    abi::emit_call_label(ctx.emitter, "__rt_warn_foreach_non_iterable");
}

/// Parks iterator state in the shape every `IterNext` path reads as "no more elements".
///
/// A zero source pointer makes `__rt_heap_kind` report kind 0, which now falls through to
/// the dynamic `IterNext` false result instead of the removed fatal; the zero snapshot
/// length keeps the indexed path from visiting anything even if it is entered.
fn emit_empty_iterator_state(ctx: &mut FunctionContext<'_>, offset: usize) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
    abi::store_at_offset(ctx.emitter, result_reg, offset - ITER_SOURCE_OFFSET_DELTA);
    abi::store_at_offset(ctx.emitter, result_reg, offset - ITER_CURSOR_OFFSET_DELTA);
    abi::store_at_offset(ctx.emitter, result_reg, offset - ITER_SNAPSHOT_LEN_OFFSET_DELTA);
    abi::store_at_offset(
        ctx.emitter,
        result_reg,
        offset - ITER_TABLE_SNAPSHOT_OFFSET_DELTA,
    );
    emit_clear_successor_keys(ctx, offset);
}

/// Returns true when an `iter_start` instruction is preparing a by-reference foreach.
fn iter_start_is_by_ref(inst: &Instruction) -> bool {
    match inst.immediate.as_ref() {
        Some(Immediate::IterStart(metadata)) => metadata.is_by_ref(),
        _ => false,
    }
}

/// Returns the optional Mixed owner slot named by an `iter_start` immediate.
fn iter_start_owner_slot(inst: &Instruction) -> Option<LocalSlotId> {
    match inst.immediate.as_ref() {
        Some(Immediate::IterStart(metadata)) => metadata.owner(),
        _ => None,
    }
}

/// Returns the optional origin local slot named by an `iter_start` immediate.
fn iter_start_origin_slot(inst: &Instruction) -> Option<LocalSlotId> {
    match inst.immediate.as_ref() {
        Some(Immediate::IterStart(metadata)) => metadata.origin(),
        _ => None,
    }
}

/// Returns the addressable iterator-state slot named by an `iter_start` immediate.
fn iter_start_state_slot(inst: &Instruction) -> Result<LocalSlotId> {
    match inst.immediate.as_ref() {
        Some(Immediate::IterStart(metadata)) => Ok(metadata.state()),
        _ => Err(CodegenIrError::invalid_module(
            "iter_start missing iterator-state metadata".to_string(),
        )),
    }
}

/// Returns the frame offset of the addressable state behind an iterator SSA handle.
fn iterator_state_offset(
    ctx: &FunctionContext<'_>,
    iterator: ValueId,
    inst: &Instruction,
) -> Result<usize> {
    let iter_start = iterator_start_instruction(ctx, iterator, inst)?;
    ctx.local_offset(iter_start_state_slot(iter_start)?)
}

/// Where a by-reference foreach can re-read its source container after the loop body moved it.
///
/// `value_offset` is the origin local's own frame slot. `storage` records the CFG-aware backend
/// representation at the current instruction, including the runtime flag only when the slot is
/// genuinely path-dependent. This lets an aliased source (`function f(array &$a) { foreach
/// ($a as &$v) ... }`) be reloaded instead of being mistaken for a raw pointer because function
/// entry flags start at zero.
#[derive(Clone, Copy)]
struct IterOrigin {
    value_offset: usize,
    storage: IterOriginStorage,
}

/// Authoritative representation of the origin slot at the current EIR instruction.
#[derive(Clone, Copy)]
enum IterOriginStorage {
    Raw,
    RefCell,
    Dynamic { state_offset: usize },
}

/// Resolves the origin an `iter_start` instruction names, for use during initialization.
fn iter_start_origin(ctx: &FunctionContext<'_>, inst: &Instruction) -> Option<IterOrigin> {
    let slot = iter_start_origin_slot(inst)?;
    let value_offset = ctx.local_offset(slot).ok()?;
    Some(IterOrigin {
        value_offset,
        storage: iter_origin_storage(ctx, slot)?,
    })
}

/// Returns where to re-read the local that republishes a relocated by-reference source.
///
/// `lower_array_push` and the copy-on-write helpers write the replacement container back into the
/// origin local, never into the iterator's private source word. Reloading it every `IterNext`
/// is what keeps growth inside a by-reference foreach from walking freed storage.
fn iter_origin(
    ctx: &FunctionContext<'_>,
    iterator: ValueId,
    inst: &Instruction,
) -> Result<Option<IterOrigin>> {
    let iter_start = iterator_start_instruction(ctx, iterator, inst)?;
    let Some(slot) = iter_start_origin_slot(iter_start) else {
        return Ok(None);
    };
    let Ok(value_offset) = ctx.local_offset(slot) else {
        return Ok(None);
    };
    Ok(Some(IterOrigin {
        value_offset,
        storage: iter_origin_storage(ctx, slot).ok_or_else(|| {
            CodegenIrError::invalid_module(format!(
                "dynamic iterator origin slot {} has no representation flag",
                slot.as_raw()
            ))
        })?,
    }))
}

/// Classifies an origin slot from the backend's CFG-aware local representation analysis.
fn iter_origin_storage(
    ctx: &FunctionContext<'_>,
    slot: LocalSlotId,
) -> Option<IterOriginStorage> {
    if ctx.local_ref_cell_representation_is_definite(slot) {
        return Some(IterOriginStorage::RefCell);
    }
    if ctx.local_ref_cell_representation_is_dynamic(slot) {
        return ctx
            .ref_cell_state_offset(slot)
            .map(|state_offset| IterOriginStorage::Dynamic { state_offset });
    }
    Some(IterOriginStorage::Raw)
}

/// Materializes the container the origin local currently holds into `dest`.
///
/// A raw slot holds the container itself. A definite reference slot always holds an address whose
/// target is the value, regardless of its zero-initialized runtime state word. Only a dynamic slot
/// consults that word because the same slot may be raw on one path and indirect on another.
fn emit_load_origin_container(
    ctx: &mut FunctionContext<'_>,
    origin: IterOrigin,
    dest: &str,
    scratch: &str,
) {
    abi::load_at_offset_scratch(ctx.emitter, dest, origin.value_offset, scratch);
    let state_offset = match origin.storage {
        IterOriginStorage::Raw => return,
        IterOriginStorage::RefCell => {
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction(&format!("ldr {dest}, [{dest}]"));  // a definite reference origin stores its value behind the incoming cell pointer
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction(                                    // a definite reference origin stores its value behind the incoming cell pointer
                        &format!("mov {dest}, QWORD PTR [{dest}]")
                    );
                }
            }
            return;
        }
        IterOriginStorage::Dynamic { state_offset } => state_offset,
    };
    let direct = ctx.next_label("iter_origin_direct");
    abi::load_at_offset_scratch(
        ctx.emitter,
        scratch,
        state_offset,
        abi::int_result_reg(ctx.emitter),
    );
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz {scratch}, {direct}"));       // a raw slot already holds the container itself
            ctx.emitter.instruction(&format!("ldr {dest}, [{dest}]"));          // a promoted slot holds it behind the alias address
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {scratch}, {scratch}"));     // is this slot still in its raw representation?
            ctx.emitter.instruction(&format!("jz {direct}"));                   // a raw slot already holds the container itself
            ctx.emitter
                .instruction(&format!("mov {dest}, QWORD PTR [{dest}]"));        // a promoted slot holds it behind the alias address
        }
    }
    ctx.emitter.label(&direct);
}

/// Anchors relocation checks on the normalized iterator source at loop entry.
///
/// Dynamic and aliased origins may hold an outer Mixed box while the iterator source word holds
/// the unboxed container. The reload path normalizes that outer box before comparing pointers, so
/// the snapshot must use the same representation.
fn emit_snapshot_origin_container(ctx: &mut FunctionContext<'_>, offset: usize) {
    let (dest, scratch) = match ctx.emitter.target.arch {
        Arch::AArch64 => ("x9", "x11"),
        Arch::X86_64 => ("r11", "r10"),
    };
    abi::load_at_offset_scratch(
        ctx.emitter,
        dest,
        offset - ITER_SOURCE_OFFSET_DELTA,
        scratch,
    );
    abi::store_at_offset_scratch(
        ctx.emitter,
        dest,
        offset - ITER_TABLE_SNAPSHOT_OFFSET_DELTA,
        scratch,
    );
}

/// Republishes a relocated by-reference source into the iterator.
///
/// The origin is normalized to the same unboxed representation as the iterator source BEFORE the
/// pointer compare. A stable outer Mixed box can contain a replaced array or hash after copy-on-write
/// or growth, so comparing the box itself would miss relocation and leave a freed table published.
/// Cursor identity is validated exactly once afterwards, because delete followed by insertion can
/// reuse a physical slot without replacing the table.
///
/// The replacement is then CLASSIFIED rather than assumed. One `Mixed` box is unwrapped, because
/// an aliased `Mixed` local holds the container one level deeper than the iterator's source word.
/// Heap kind 3 is associative storage and needs a rebuilt cursor, since rehashing permutes slot
/// indices. Heap kind 2 is an indexed array, whose positional cursor survives reallocation, so it
/// only needs the fresh pointer. ANY OTHER kind, including the zero that a destroyed or
/// non-container value reports, parks the iterator in the done state instead of handing an
/// invalid pointer to the heap-kind dispatch that runs next.
fn emit_reload_live_iter_source(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
    origin: IterOrigin,
) {
    let stable = ctx.next_label("iter_source_stable");
    let classified = ctx.next_label("iter_source_classified");
    let indexed = ctx.next_label("iter_source_indexed");
    let normalized = ctx.next_label("iter_source_normalized");
    let (dest, scratch) = match ctx.emitter.target.arch {
        Arch::AArch64 => ("x9", "x11"),
        Arch::X86_64 => ("r11", "r10"),
    };
    emit_load_origin_container(ctx, origin, dest, scratch);
    abi::store_at_offset_scratch(ctx.emitter, dest, offset - ITER_SOURCE_OFFSET_DELTA, scratch);
    abi::emit_reg_move(ctx.emitter, abi::int_result_reg(ctx.emitter), dest);
    abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #5");                              // heap kind 5 identifies a boxed Mixed value
            ctx.emitter.instruction(&format!("b.ne {}", normalized));           // an unboxed container is already the right pointer
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 5");                              // heap kind 5 identifies a boxed Mixed value
            ctx.emitter.instruction(&format!("jne {}", normalized));            // an unboxed container is already the right pointer
        }
    }
    abi::load_at_offset_scratch(ctx.emitter, dest, offset - ITER_SOURCE_OFFSET_DELTA, scratch);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [x9, #8]");                        // an aliased Mixed local holds the container one box deeper
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r11, QWORD PTR [r11 + 8]");            // an aliased Mixed local holds the container one box deeper
        }
    }
    abi::store_at_offset_scratch(ctx.emitter, dest, offset - ITER_SOURCE_OFFSET_DELTA, scratch);
    ctx.emitter.label(&normalized);
    abi::load_at_offset_scratch(ctx.emitter, dest, offset - ITER_SOURCE_OFFSET_DELTA, scratch);
    abi::load_at_offset_scratch(
        ctx.emitter,
        scratch,
        offset - ITER_TABLE_SNAPSHOT_OFFSET_DELTA,
        abi::int_result_reg(ctx.emitter),
    );
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x9, x11");                             // is the normalized live container still the cursor's source?
            ctx.emitter.instruction(&format!("b.eq {}", stable));               // unchanged sources need no republish or classification
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp r11, r10");                            // is the normalized live container still the cursor's source?
            ctx.emitter.instruction(&format!("je {}", stable));                 // unchanged sources need no republish or classification
        }
    }
    abi::store_at_offset_scratch(
        ctx.emitter,
        dest,
        offset - ITER_TABLE_SNAPSHOT_OFFSET_DELTA,
        scratch,
    );
    abi::emit_reg_move(ctx.emitter, abi::int_result_reg(ctx.emitter), dest);
    abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
    ctx.emitter.label(&classified);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #3");                              // heap kind 3 identifies associative table storage
            ctx.emitter.instruction(&format!("b.eq {}", stable));               // the separate anchor validator rebuilds associative cursors once
            ctx.emitter.instruction("cmp x0, #2");                              // heap kind 2 identifies indexed-array storage
            ctx.emitter.instruction(&format!("b.eq {}", indexed));              // a positional cursor survives indexed reallocation
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 3");                              // heap kind 3 identifies associative table storage
            ctx.emitter.instruction(&format!("je {}", stable));                 // the separate anchor validator rebuilds associative cursors once
            ctx.emitter.instruction("cmp rax, 2");                              // heap kind 2 identifies indexed-array storage
            ctx.emitter.instruction(&format!("je {}", indexed));                // a positional cursor survives indexed reallocation
        }
    }
    // -- the replacement is not a live container, so stop instead of dispatching on it --
    // A destroyed source reports heap kind 0, and so does any non-container value. Parking the
    // iterator here is what keeps the heap-kind dispatch in `IterNext` from probing it.
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
    abi::store_at_offset(ctx.emitter, result_reg, offset - ITER_SOURCE_OFFSET_DELTA);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, -1);
    abi::store_at_offset(ctx.emitter, result_reg, offset - ITER_CURSOR_OFFSET_DELTA);
    abi::emit_jump(ctx.emitter, &stable);

    ctx.emitter.label(&indexed);
    abi::emit_jump(ctx.emitter, &stable);
    ctx.emitter.label(&stable);
}

/// Rebuilds an active hash cursor from owned key identity even when the table pointer is stable.
///
/// Deletion may leave the cursor aimed at a tombstone, and a later insertion may reuse that exact
/// physical slot. The occupied marker alone cannot distinguish the replacement entry. Restricting
/// this validation to by-reference iterators with an origin keeps by-value iteration unchanged.
fn emit_validate_hash_cursor_anchor(ctx: &mut FunctionContext<'_>, offset: usize) {
    let done = ctx.next_label("iter_anchor_validate_done");
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::load_at_offset(ctx.emitter, result_reg, offset - ITER_CURSOR_OFFSET_DELTA);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // only an active positive cursor has a successor identity
            ctx.emitter.instruction(&format!("b.le {done}"));                   // fresh and terminal cursors need no validation
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 0");                              // only an active positive cursor has a successor identity
            ctx.emitter.instruction(&format!("jle {done}"));                    // fresh and terminal cursors need no validation
        }
    }
    abi::load_at_offset(ctx.emitter, result_reg, offset - ITER_SOURCE_OFFSET_DELTA);
    abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #3");                              // heap kind 3 is associative storage
            ctx.emitter.instruction(&format!("b.ne {done}"));                   // indexed and parked sources have no hash cursor
            abi::load_at_offset(ctx.emitter, "x0", offset - ITER_SOURCE_OFFSET_DELTA);
            abi::load_at_offset(ctx.emitter, "x1", offset - ITER_NEXT_KEY_LO_OFFSET_DELTA);
            abi::load_at_offset(ctx.emitter, "x2", offset - ITER_NEXT_KEY_HI_OFFSET_DELTA);
            abi::load_at_offset(ctx.emitter, "x3", offset - ITER_FALLBACK_KEY_LO_OFFSET_DELTA);
            abi::load_at_offset(ctx.emitter, "x4", offset - ITER_FALLBACK_KEY_HI_OFFSET_DELTA);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 3");                              // heap kind 3 is associative storage
            ctx.emitter.instruction(&format!("jne {done}"));                    // indexed and parked sources have no hash cursor
            abi::load_at_offset(ctx.emitter, "rdi", offset - ITER_SOURCE_OFFSET_DELTA);
            abi::load_at_offset(ctx.emitter, "rsi", offset - ITER_NEXT_KEY_LO_OFFSET_DELTA);
            abi::load_at_offset(ctx.emitter, "rdx", offset - ITER_NEXT_KEY_HI_OFFSET_DELTA);
            abi::load_at_offset(ctx.emitter, "rcx", offset - ITER_FALLBACK_KEY_LO_OFFSET_DELTA);
            abi::load_at_offset(ctx.emitter, "r8", offset - ITER_FALLBACK_KEY_HI_OFFSET_DELTA);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_hash_iter_resync");
    abi::store_at_offset(ctx.emitter, result_reg, offset - ITER_CURSOR_OFFSET_DELTA);
    ctx.emitter.label(&done);
}

/// Splits statically typed array sources and boxes hash entries before reference iteration.
///
/// A null/sentinel source — the value a missed read such as `foreach ($a[7] as &$v)`
/// materializes — has nothing to split: the copy-on-write helpers recognize both the zero
/// pointer and the in-band `NULL_SENTINEL` and return them unchanged (issue #556). The
/// origin local and SSA slot therefore keep the sentinel; the caller normalizes only the
/// iterator's private source slot afterwards.
fn ensure_unique_static_iter_source(
    ctx: &mut FunctionContext<'_>,
    source: ValueId,
    source_kind: &IteratorSourceKind,
) -> Result<()> {
    let source_local = source_load_local_slot(ctx, source)?;
    let helper = match source_kind {
        // Every indexed by-reference source becomes a hash before iteration. Hash entries carry
        // the persistent tag-11 reference-set marker, so the last alias can safely outlive a
        // literal or function-result source without introducing a second indexed wrapper format.
        IteratorSourceKind::Indexed { .. } => {
            // Loading a concrete container from boxed PHP-array storage retained its payload.
            // Retire and clear the old box before the consuming conversion, then publish the
            // converted owner below. Raw concrete slots need no release here.
            if let Some(slot) = source_local {
                ctx.release_mutated_source_local_owner(slot, source)?;
            }
            convert_loaded_indexed_source_to_hash(ctx);
            ctx.store_result_value(source)?;
            if let Some(slot) = source_local {
                ctx.store_container_writeback_to_local(slot, source)?;
            }
            return Ok(());
        }
        // Hash references use the boxed Mixed entry slot as their stable value cell. The
        // converter performs COW itself before replacing concrete entry payloads.
        IteratorSourceKind::Hash => "__rt_hash_to_mixed",
        _ => return Ok(()),
    };
    // HashToMixed can replace the payload at its copy-on-write boundary. Transfer a raw boxed
    // local's payload owner into that operation so storeback neither leaks the previous box nor
    // leaves the iterator's origin pointing at a retired generation.
    if let Some(slot) = source_local {
        ctx.release_mutated_source_local_owner(slot, source)?;
    }
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the foreach source pointer to the COW helper
    }
    abi::emit_call_label(ctx.emitter, helper);
    ctx.store_result_value(source)?;
    if let Some(slot) = source_local {
        ctx.store_container_writeback_to_local(slot, source)?;
    }
    Ok(())
}

/// Converts the array-like value in the integer result register into a Mixed-entry hash.
///
/// A source may already have been promoted by addressable-source preparation before this shared
/// helper reaches it. Probe the runtime heap kind first so that existing hashes go directly
/// through the idempotent Mixed-entry conversion instead of being reinterpreted as indexed
/// storage. For a genuine indexed array, `__rt_array_to_hash` borrows its input and returns a
/// fresh hash. The replacement consumes the caller's indexed-array reference, so this helper
/// releases that old owner after the hash has retained every child.
pub(super) fn convert_loaded_indexed_source_to_hash(ctx: &mut FunctionContext<'_>) {
    let already_hash = ctx.next_label("iter_indexed_source_already_hash");
    let done = ctx.next_label("iter_indexed_source_hash_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(ctx.emitter, "x0");
            abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
            ctx.emitter.instruction("cmp x0, #3");                              // detect a source already promoted by addressable-source lowering
            ctx.emitter.instruction(&format!("b.eq {}", already_hash));         // never reinterpret an existing hash as indexed storage
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_push_reg(ctx.emitter, "x0");
            abi::emit_call_label(ctx.emitter, "__rt_array_to_hash");
            abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");
            abi::emit_push_reg(ctx.emitter, "x0");
            ctx.emitter.instruction("ldr x0, [sp, #16]");                       // release the indexed owner replaced by the promoted hash
            abi::emit_call_label(ctx.emitter, "__rt_decref_array");
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_pop_reg(ctx.emitter, "x9");
            ctx.emitter.instruction(&format!("b {}", done));                    // join the already-hash and newly-promoted results
            ctx.emitter.label(&already_hash);
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // pass the indexed source to hash promotion
            abi::emit_push_reg(ctx.emitter, "rdi");
            abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
            ctx.emitter.instruction("cmp rax, 3");                              // detect a source already promoted by addressable-source lowering
            ctx.emitter.instruction(&format!("je {}", already_hash));           // never reinterpret an existing hash as indexed storage
            abi::emit_pop_reg(ctx.emitter, "rdi");
            abi::emit_push_reg(ctx.emitter, "rdi");
            abi::emit_call_label(ctx.emitter, "__rt_array_to_hash");
            ctx.emitter.instruction("mov rdi, rax");                            // widen the promoted hash entries to boxed Mixed
            abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");
            abi::emit_push_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 16]");           // release the indexed owner replaced by the promoted hash
            abi::emit_call_label(ctx.emitter, "__rt_decref_array");
            abi::emit_pop_reg(ctx.emitter, "rax");
            abi::emit_pop_reg(ctx.emitter, "r10");
            ctx.emitter.instruction(&format!("jmp {}", done));                  // join the already-hash and newly-promoted results
            ctx.emitter.label(&already_hash);
            abi::emit_pop_reg(ctx.emitter, "rdi");
            abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");
        }
    }
    ctx.emitter.label(&done);
}

/// Stores a converted dynamic iterator source back to its originating local when possible.
fn store_iter_source_to_origin_if_local(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
    source: ValueId,
) -> Result<()> {
    let Some(slot) = source_load_local_slot(ctx, source)? else {
        return Ok(());
    };
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::load_at_offset(ctx.emitter, "x0", offset - ITER_SOURCE_OFFSET_DELTA);
        }
        Arch::X86_64 => {
            abi::load_at_offset(ctx.emitter, "rax", offset - ITER_SOURCE_OFFSET_DELTA);
        }
    }
    ctx.store_result_value(source)?;
    ctx.store_container_writeback_to_local(slot, source)
}

/// Resolves a source SSA value back to its direct or reference-bound local origin.
fn source_load_local_slot(
    ctx: &FunctionContext<'_>,
    value: ValueId,
) -> Result<Option<LocalSlotId>> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(None);
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    if !matches!(inst_ref.op, Op::LoadLocal | Op::LoadRefCell) {
        return Ok(None);
    }
    let Some(Immediate::LocalSlot(slot)) = inst_ref.immediate else {
        return Err(CodegenIrError::invalid_module(
            "load_local iterator source missing local slot",
        ));
    };
    Ok(Some(slot))
}

/// Converts the raw dynamic indexed-array iterator source to a Mixed-entry hash.
fn convert_dynamic_indexed_source_for_ref(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::load_at_offset(ctx.emitter, "x0", offset - ITER_SOURCE_OFFSET_DELTA);
            convert_loaded_indexed_source_to_hash(ctx);
            abi::store_at_offset(ctx.emitter, "x0", offset - ITER_SOURCE_OFFSET_DELTA);
        }
        Arch::X86_64 => {
            abi::load_at_offset(ctx.emitter, "rdi", offset - ITER_SOURCE_OFFSET_DELTA);
            ctx.emitter.instruction("mov rax, rdi");                            // put the indexed source in the shared promotion input register
            convert_loaded_indexed_source_to_hash(ctx);
            abi::store_at_offset(ctx.emitter, "rax", offset - ITER_SOURCE_OFFSET_DELTA);
        }
    }
    Ok(())
}

/// Converts the raw dynamic hash iterator source to boxed Mixed entries.
fn convert_dynamic_hash_source_for_ref(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::load_at_offset(ctx.emitter, "x0", offset - ITER_SOURCE_OFFSET_DELTA);
            abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");
            abi::store_at_offset(ctx.emitter, "x0", offset - ITER_SOURCE_OFFSET_DELTA);
        }
        Arch::X86_64 => {
            abi::load_at_offset(ctx.emitter, "rdi", offset - ITER_SOURCE_OFFSET_DELTA);
            abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");
            abi::store_at_offset(ctx.emitter, "rax", offset - ITER_SOURCE_OFFSET_DELTA);
        }
    }
    Ok(())
}

/// Converts an unboxed Mixed indexed payload to hash storage and updates its owning Mixed cell.
fn convert_mixed_indexed_source_for_ref(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // pass the unboxed indexed payload to hash promotion
            convert_loaded_indexed_source_to_hash(ctx);
            ctx.emitter.instruction("ldr x9, [sp]");                            // reload the preserved boxed Mixed source cell
            ctx.emitter.instruction("mov x10, #5");                             // runtime Mixed tag 5 identifies associative hash storage
            ctx.emitter.instruction("str x10, [x9]");                           // publish the promoted container's new runtime tag
            ctx.emitter.instruction("str x0, [x9, #8]");                        // publish the promoted hash pointer into the Mixed cell
            abi::store_at_offset(ctx.emitter, "x0", offset - ITER_SOURCE_OFFSET_DELTA);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, rdi");                            // pass the unboxed indexed payload to hash promotion
            convert_loaded_indexed_source_to_hash(ctx);
            ctx.emitter.instruction("mov r10, QWORD PTR [rsp]");                // reload the preserved boxed Mixed source cell
            ctx.emitter.instruction("mov QWORD PTR [r10], 5");                  // publish associative-hash as the new runtime Mixed tag
            ctx.emitter.instruction("mov QWORD PTR [r10 + 8], rax");            // publish the promoted hash pointer into the Mixed cell
            abi::store_at_offset(ctx.emitter, "rax", offset - ITER_SOURCE_OFFSET_DELTA);
        }
    }
    Ok(())
}

/// Converts an unboxed Mixed hash payload and updates the preserved Mixed source cell.
fn convert_mixed_hash_source_for_ref(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // pass the unboxed hash payload to the Mixed conversion helper
            abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");
            ctx.emitter.instruction("ldr x9, [sp]");                            // reload the preserved boxed Mixed source cell
            ctx.emitter.instruction("str x0, [x9, #8]");                        // publish the unique converted hash pointer into the Mixed cell
            abi::store_at_offset(ctx.emitter, "x0", offset - ITER_SOURCE_OFFSET_DELTA);
        }
        Arch::X86_64 => {
            abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");
            ctx.emitter.instruction("mov r10, QWORD PTR [rsp]");                // reload the preserved boxed Mixed source cell
            ctx.emitter.instruction("mov QWORD PTR [r10 + 8], rax");            // publish the unique converted hash pointer into the Mixed cell
            abi::store_at_offset(ctx.emitter, "rax", offset - ITER_SOURCE_OFFSET_DELTA);
        }
    }
    Ok(())
}

/// Binds a local slot to the current associative-array entry's managed reference cell.
///
/// The entry is promoted into a PHP reference set first, so what the local receives is a real
/// managed reference-cell allocation rather than an interior pointer into the table. That is what
/// lets the alias survive growth, a copy-on-write split and even destruction of the source array,
/// and what makes closure capture and returning the reference retain something real. Promotion is
/// idempotent, so a repeated by-reference foreach reuses the existing cell instead of restamping
/// the entry.
fn bind_hash_current_value_ref(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
    slot: LocalSlotId,
) -> Result<()> {
    ctx.release_counted_ref_binding(slot);
    let local_offset = ctx.local_offset(slot)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::load_at_offset(ctx.emitter, "x0", offset - ITER_VALUE_ADDR_OFFSET_DELTA);
            abi::emit_call_label(ctx.emitter, "__rt_hash_entry_make_reference");
            abi::store_at_offset_scratch(ctx.emitter, "x0", local_offset, "x11");
            ctx.bind_hash_entry_ref_state(slot, "x0")?;
        }
        Arch::X86_64 => {
            abi::load_at_offset(ctx.emitter, "rdi", offset - ITER_VALUE_ADDR_OFFSET_DELTA);
            abi::emit_call_label(ctx.emitter, "__rt_hash_entry_make_reference");
            abi::store_at_offset(ctx.emitter, "rax", local_offset);
            ctx.bind_hash_entry_ref_state(slot, "rax")?;
        }
    }
    Ok(())
}

/// Binds a local slot to the current value address after dynamic iterable dispatch.
fn bind_dynamic_current_value_ref(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
    slot: LocalSlotId,
) -> Result<()> {
    let indexed_case = ctx.next_label("iter_ref_dyn_indexed");
    let hash_case = ctx.next_label("iter_ref_dyn_hash");
    let object_case = ctx.next_label("iter_ref_dyn_object");
    let done = ctx.next_label("iter_ref_dyn_done");
    branch_on_dynamic_source_heap_kind(ctx, offset, &indexed_case, &hash_case, &object_case);
    // -- an empty iterator state reaches here; bind null instead of stack garbage --
    // `IterStart` already warned and zeroed the source, and `IterNext` already reported
    // "no more elements", so this block is unreachable at runtime.
    {
        let local_offset = ctx.local_offset(slot)?;
        let result_reg = abi::int_result_reg(ctx.emitter);
        abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
        abi::store_at_offset(ctx.emitter, result_reg, local_offset);
        abi::emit_jump(ctx.emitter, &done);
    }

    ctx.emitter.label(&indexed_case);
    bind_hash_current_value_ref(ctx, offset, slot)?;
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&hash_case);
    bind_hash_current_value_ref(ctx, offset, slot)?;
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&object_case);
    abi::emit_call_label(ctx.emitter, "__rt_iterable_unsupported_kind");
    ctx.emitter.label(&done);
    Ok(())
}

/// Lowers dynamic iterator advancement by dispatching to the concrete heap layout.
fn lower_dynamic_iter_next(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
    by_ref: bool,
    capture_successor: bool,
) -> Result<()> {
    let indexed_case = ctx.next_label("iter_next_dyn_indexed");
    let hash_case = ctx.next_label("iter_next_dyn_hash");
    let object_case = ctx.next_label("iter_next_dyn_object");
    let done = ctx.next_label("iter_next_dyn_done");
    branch_on_dynamic_source_heap_kind(ctx, offset, &indexed_case, &hash_case, &object_case);
    // -- a source that matches no heap kind has no elements left to visit --
    // `IterStart` warned and zeroed the iterator state for a non-iterable, so reporting
    // false here is what skips the loop body and lets the next statement run.
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&indexed_case);
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_indexed_iter_next_aarch64(ctx, offset, by_ref),
        Arch::X86_64 => lower_indexed_iter_next_x86_64(ctx, offset, by_ref),
    }
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&hash_case);
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_hash_iter_next_aarch64(ctx, offset, capture_successor),
        Arch::X86_64 => lower_hash_iter_next_x86_64(ctx, offset, capture_successor),
    }
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&object_case);
    lower_interface_iter_next(ctx, offset, "Iterator")?;
    ctx.emitter.label(&done);
    Ok(())
}

/// Lowers dynamic iterator key loading by dispatching to the concrete heap layout.
fn lower_dynamic_iter_current_key(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    offset: usize,
) -> Result<()> {
    let indexed_case = ctx.next_label("iter_key_dyn_indexed");
    let hash_case = ctx.next_label("iter_key_dyn_hash");
    let object_case = ctx.next_label("iter_key_dyn_object");
    let done = ctx.next_label("iter_key_dyn_done");
    branch_on_dynamic_source_heap_kind(ctx, offset, &indexed_case, &hash_case, &object_case);
    // -- unreachable: `IterNext` already reported "no more elements" for this state --
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&indexed_case);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::load_at_offset(ctx.emitter, result_reg, offset - ITER_CURSOR_OFFSET_DELTA);
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&hash_case);
    match ctx.emitter.target.arch {
        Arch::AArch64 => load_current_hash_key_as_mixed_aarch64(ctx, offset),
        Arch::X86_64 => load_current_hash_key_as_mixed_x86_64(ctx, offset),
    }
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&object_case);
    let return_ty = emit_interface_iterator_method_call(ctx, offset, "Iterator", "key")?;
    box_iterator_method_result_if_needed(ctx, inst, &return_ty)?;
    ctx.emitter.label(&done);
    Ok(())
}

/// Lowers dynamic iterator value loading by dispatching to the concrete heap layout.
fn lower_dynamic_iter_current_value(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    offset: usize,
    preserve_reference: bool,
    promote_reference: bool,
) -> Result<()> {
    let indexed_case = ctx.next_label("iter_value_dyn_indexed");
    let hash_case = ctx.next_label("iter_value_dyn_hash");
    let object_case = ctx.next_label("iter_value_dyn_object");
    let done = ctx.next_label("iter_value_dyn_done");
    branch_on_dynamic_source_heap_kind(ctx, offset, &indexed_case, &hash_case, &object_case);
    // -- unreachable: `IterNext` already reported "no more elements" for this state --
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&indexed_case);
    match ctx.emitter.target.arch {
        Arch::AArch64 => load_current_dynamic_indexed_value_as_mixed_aarch64(ctx, offset),
        Arch::X86_64 => load_current_dynamic_indexed_value_as_mixed_x86_64(ctx, offset),
    }
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&hash_case);
    load_current_hash_call_value_as_mixed(ctx, offset, preserve_reference, promote_reference);
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&object_case);
    let return_ty = emit_interface_iterator_method_call(ctx, offset, "Iterator", "current")?;
    box_iterator_method_result_if_needed(ctx, inst, &return_ty)?;
    ctx.emitter.label(&done);
    Ok(())
}

/// Replaces a dynamic object iterator source with `IteratorAggregate::getIterator()` when available.
///
/// The owner recorded in `IterStart` is authoritative. Its absence means lowering proved
/// this source cannot produce an aggregate result, even if later type widening makes the
/// backend operand look dynamic.
fn resolve_dynamic_object_iterator_source(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
    owner: Option<LocalSlotId>,
) -> Result<()> {
    if owner.is_none() || !ctx.module.interface_infos.contains_key("IteratorAggregate") {
        return Ok(());
    }
    let return_ty =
        emit_interface_iterator_method_call(ctx, offset, "IteratorAggregate", "getIterator")?;
    adopt_get_iterator_result(ctx, offset, owner, &return_ty)
}

/// Owns a nonzero `getIterator()` result in the Mixed owner slot, then publishes a borrow.
///
/// `emit_box_current_owned_value_as_mixed` is the transfer: `__rt_mixed_from_value`
/// retains the payload into a fresh Mixed cell, then the helper releases the
/// original method-return owner. There is no user callback between those two
/// steps, so the Mixed cell is the sole owner afterward. The iterator source
/// word is then loaded from that cell's payload. The previous aggregate pointer
/// is overwritten without a matching release; the loop's source retain still
/// owns the aggregate.
fn adopt_get_iterator_result(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
    owner: Option<LocalSlotId>,
    return_ty: &PhpType,
) -> Result<()> {
    let slot = require_get_iterator_owner(owner)?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let keep_original = ctx.next_label("iter_keep_original_source");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(                                            // keep the original source when getIterator() returned null
                &format!("cbz {}, {}", result_reg, keep_original)
            );
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(                                            // keep the original source when getIterator() returned null
                &format!("test {}, {}", result_reg, result_reg)
            );
            ctx.emitter.instruction(&format!("je {}", keep_original));          // skip replacement when getIterator() was not resolved
        }
    }
    emit_box_current_owned_value_as_mixed(ctx.emitter, &return_ty.codegen_repr());
    let local_offset = ctx.local_offset(slot)?;
    abi::store_at_offset(ctx.emitter, result_reg, local_offset);
    abi::emit_load_from_address(
        ctx.emitter,
        result_reg,
        result_reg,
        MIXED_CELL_PAYLOAD_LOW_OFFSET,
    );
    abi::store_at_offset(ctx.emitter, result_reg, offset - ITER_SOURCE_OFFSET_DELTA);
    ctx.emitter.label(&keep_original);
    Ok(())
}

/// Rejects an aggregate adoption path that has no place to retain the returned iterator.
fn require_get_iterator_owner(owner: Option<LocalSlotId>) -> Result<LocalSlotId> {
    owner.ok_or_else(|| {
        CodegenIrError::invalid_module(
            "iterator aggregate result has no owning Mixed slot".to_string(),
        )
    })
}

/// Returns true when an interface-typed source must call `getIterator()` before rewind.
fn interface_needs_get_iterator(ctx: &FunctionContext<'_>, interface_name: &str) -> bool {
    let name = interface_name.trim_start_matches('\\');
    if name == "Iterator" || interface_extends_interface(ctx, name, "Iterator") {
        return false;
    }
    name == "IteratorAggregate"
        || interface_extends_interface(ctx, name, "IteratorAggregate")
}

/// Branches on the heap kind of the raw iterable source stored in iterator state.
fn branch_on_dynamic_source_heap_kind(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
    indexed_case: &str,
    hash_case: &str,
    object_case: &str,
) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::load_at_offset(ctx.emitter, result_reg, offset - ITER_SOURCE_OFFSET_DELTA);
    abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
    branch_on_heap_kind_result(ctx, indexed_case, hash_case, object_case);
}

/// Branches to the concrete iterator path from a `__rt_heap_kind` result.
fn branch_on_heap_kind_result(
    ctx: &mut FunctionContext<'_>,
    indexed_case: &str,
    hash_case: &str,
    object_case: &str,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #2");                              // heap kind 2 identifies indexed arrays
            ctx.emitter.instruction(&format!("b.eq {}", indexed_case));         // dispatch to the indexed-array iterator path
            ctx.emitter.instruction("cmp x0, #3");                              // heap kind 3 identifies associative arrays
            ctx.emitter.instruction(&format!("b.eq {}", hash_case));            // dispatch to the associative-array iterator path
            ctx.emitter.instruction("cmp x0, #4");                              // heap kind 4 identifies object payloads
            ctx.emitter.instruction(&format!("b.eq {}", object_case));          // dispatch to the object Iterator protocol path
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 2");                              // heap kind 2 identifies indexed arrays
            ctx.emitter.instruction(&format!("je {}", indexed_case));           // dispatch to the indexed-array iterator path
            ctx.emitter.instruction("cmp rax, 3");                              // heap kind 3 identifies associative arrays
            ctx.emitter.instruction(&format!("je {}", hash_case));              // dispatch to the associative-array iterator path
            ctx.emitter.instruction("cmp rax, 4");                              // heap kind 4 identifies object payloads
            ctx.emitter.instruction(&format!("je {}", object_case));            // dispatch to the object Iterator protocol path
        }
    }
}

/// Branches to the concrete iterator path from a `__rt_mixed_unbox` tag result.
fn branch_on_mixed_iterable_tag(
    ctx: &mut FunctionContext<'_>,
    indexed_case: &str,
    hash_case: &str,
    object_case: &str,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #4");                              // mixed tag 4 identifies indexed arrays
            ctx.emitter.instruction(&format!("b.eq {}", indexed_case));         // dispatch to the indexed-array iterator path
            ctx.emitter.instruction("cmp x0, #5");                              // mixed tag 5 identifies associative arrays
            ctx.emitter.instruction(&format!("b.eq {}", hash_case));            // dispatch to the associative-array iterator path
            ctx.emitter.instruction("cmp x0, #6");                              // mixed tag 6 identifies object payloads
            ctx.emitter.instruction(&format!("b.eq {}", object_case));          // dispatch to the object Iterator protocol path
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 4");                              // mixed tag 4 identifies indexed arrays
            ctx.emitter.instruction(&format!("je {}", indexed_case));           // dispatch to the indexed-array iterator path
            ctx.emitter.instruction("cmp rax, 5");                              // mixed tag 5 identifies associative arrays
            ctx.emitter.instruction(&format!("je {}", hash_case));              // dispatch to the associative-array iterator path
            ctx.emitter.instruction("cmp rax, 6");                              // mixed tag 6 identifies object payloads
            ctx.emitter.instruction(&format!("je {}", object_case));            // dispatch to the object Iterator protocol path
        }
    }
}

/// Stores the low payload produced by `__rt_mixed_unbox` as the raw iterator source.
fn store_mixed_payload_low_as_iterator_source(ctx: &mut FunctionContext<'_>, offset: usize) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::store_at_offset(ctx.emitter, "x1", offset - ITER_SOURCE_OFFSET_DELTA);
        }
        Arch::X86_64 => {
            abi::store_at_offset(ctx.emitter, "rdi", offset - ITER_SOURCE_OFFSET_DELTA);
        }
    }
}

/// Stores an iterator cursor value into the stack-resident iterator state.
fn store_iterator_cursor(ctx: &mut FunctionContext<'_>, offset: usize, cursor: i64) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, cursor);
    abi::store_at_offset(ctx.emitter, result_reg, offset - ITER_CURSOR_OFFSET_DELTA);
    // -- record which container this cursor is valid against --
    // Every initialization path reaches this helper AFTER the source word holds its final
    // pointer, including the by-reference paths that first convert an indexed array or a hash
    // to boxed Mixed storage. Snapshotting here rather than at the top of `lower_iter_start` is
    // what keeps the first `IterNext` on the fast path instead of resyncing against a stale
    // zero snapshot with no key yielded yet.
    let snapshot_reg = abi::secondary_scratch_reg(ctx.emitter);
    let scratch_reg = abi::tertiary_scratch_reg(ctx.emitter);
    abi::load_at_offset_scratch(
        ctx.emitter,
        snapshot_reg,
        offset - ITER_SOURCE_OFFSET_DELTA,
        scratch_reg,
    );
    abi::store_at_offset_scratch(
        ctx.emitter,
        snapshot_reg,
        offset - ITER_TABLE_SNAPSHOT_OFFSET_DELTA,
        scratch_reg,
    );
    emit_clear_successor_keys(ctx, offset);
}

/// Parks both successor-key anchors in their "nothing to resume from" state.
///
/// Every walk starts without a successor recorded, and an exhausted walk ends the same way, so
/// a resync can never probe for a key this iterator never yielded a position for.
fn emit_clear_successor_keys(ctx: &mut FunctionContext<'_>, offset: usize) {
    let value_reg = abi::secondary_scratch_reg(ctx.emitter);
    let scratch_reg = abi::tertiary_scratch_reg(ctx.emitter);
    for (low_delta, high_delta) in [
        (ITER_NEXT_KEY_LO_OFFSET_DELTA, ITER_NEXT_KEY_HI_OFFSET_DELTA),
        (
            ITER_FALLBACK_KEY_LO_OFFSET_DELTA,
            ITER_FALLBACK_KEY_HI_OFFSET_DELTA,
        ),
    ] {
        abi::emit_load_int_immediate(ctx.emitter, value_reg, 0);
        abi::store_at_offset_scratch(ctx.emitter, value_reg, offset - low_delta, scratch_reg);
        abi::emit_load_int_immediate(ctx.emitter, value_reg, NO_SUCCESSOR_KEY_MARKER);
        abi::store_at_offset_scratch(ctx.emitter, value_reg, offset - high_delta, scratch_reg);
    }
}

/// Releases every owned string key stored in the relocation anchors, then clears both pairs.
///
/// Integer keys use the `-1` high-word sentinel and own nothing. String keys use a non-negative
/// length and carry exactly one retain acquired by [`emit_capture_successor_keys`].
fn emit_release_successor_keys(ctx: &mut FunctionContext<'_>, offset: usize) {
    for (low_delta, high_delta) in [
        (ITER_NEXT_KEY_LO_OFFSET_DELTA, ITER_NEXT_KEY_HI_OFFSET_DELTA),
        (
            ITER_FALLBACK_KEY_LO_OFFSET_DELTA,
            ITER_FALLBACK_KEY_HI_OFFSET_DELTA,
        ),
    ] {
        let done = ctx.next_label("iter_anchor_release_done");
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                abi::load_at_offset(ctx.emitter, "x9", offset - high_delta);
                ctx.emitter.instruction("cmp x9, #0");                          // only non-negative high words describe owned string keys
                ctx.emitter.instruction(&format!("b.lt {done}"));               // integer and absent sentinels carry no ownership
                abi::load_at_offset(ctx.emitter, "x0", offset - low_delta);
                ctx.emitter.instruction(&format!("cbz x0, {done}"));            // tolerate a cleared defensive state
                abi::emit_call_label(ctx.emitter, "__rt_decref_any");
            }
            Arch::X86_64 => {
                abi::load_at_offset(ctx.emitter, "r10", offset - high_delta);
                ctx.emitter.instruction("cmp r10, 0");                          // only non-negative high words describe owned string keys
                ctx.emitter.instruction(&format!("jl {done}"));                 // integer and absent sentinels carry no ownership
                abi::load_at_offset(ctx.emitter, "rax", offset - low_delta);
                ctx.emitter.instruction("test rax, rax");                       // tolerate a cleared defensive state
                ctx.emitter.instruction(&format!("jz {done}"));                 // skip release when the defensive pointer is already clear
                abi::emit_call_label(ctx.emitter, "__rt_decref_any");
            }
        }
        ctx.emitter.label(&done);
    }
    emit_clear_successor_keys(ctx, offset);
}

/// Acquires the ownership represented by both populated string-key anchor pairs.
fn emit_retain_successor_keys(ctx: &mut FunctionContext<'_>, offset: usize) {
    for (low_delta, high_delta) in [
        (ITER_NEXT_KEY_LO_OFFSET_DELTA, ITER_NEXT_KEY_HI_OFFSET_DELTA),
        (
            ITER_FALLBACK_KEY_LO_OFFSET_DELTA,
            ITER_FALLBACK_KEY_HI_OFFSET_DELTA,
        ),
    ] {
        let done = ctx.next_label("iter_anchor_retain_done");
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                abi::load_at_offset(ctx.emitter, "x9", offset - high_delta);
                ctx.emitter.instruction("cmp x9, #0");                          // non-negative high words identify string keys
                ctx.emitter.instruction(&format!("b.lt {done}"));               // integer and absent sentinels are inline
                abi::load_at_offset(ctx.emitter, "x0", offset - low_delta);
                ctx.emitter.instruction(&format!("cbz x0, {done}"));            // an empty defensive pointer owns nothing
                abi::emit_call_label(ctx.emitter, "__rt_incref");
            }
            Arch::X86_64 => {
                abi::load_at_offset(ctx.emitter, "r10", offset - high_delta);
                ctx.emitter.instruction("cmp r10, 0");                          // non-negative high words identify string keys
                ctx.emitter.instruction(&format!("jl {done}"));                 // integer and absent sentinels are inline
                abi::load_at_offset(ctx.emitter, "rax", offset - low_delta);
                ctx.emitter.instruction("test rax, rax");                       // an empty defensive pointer owns nothing
                ctx.emitter.instruction(&format!("jz {done}"));                 // skip retain when the defensive pointer is already clear
                abi::emit_call_label(ctx.emitter, "__rt_incref");
            }
        }
        ctx.emitter.label(&done);
    }
}

/// Snapshots the indexed-array length into the iterator state so `IterNext` compares
/// against the original length rather than re-reading it each iteration. PHP snapshots
/// the array length at loop entry, so elements appended during iteration are not visited.
/// A null/sentinel source (a missed outer array read) snapshots length zero so the loop
/// body never runs instead of dereferencing the sentinel as an array header.
fn snapshot_indexed_array_length(ctx: &mut FunctionContext<'_>, offset: usize) {
    let array_reg = abi::int_result_reg(ctx.emitter);
    let len_reg = abi::secondary_scratch_reg(ctx.emitter);
    let null_label = ctx.next_label("iter_len_null_source");
    let done_label = ctx.next_label("iter_len_done");
    abi::load_at_offset(ctx.emitter, array_reg, offset - ITER_SOURCE_OFFSET_DELTA);
    // -- guard the source: a missed outer read carries a null/sentinel container --
    crate::codegen::sentinels::emit_branch_if_null_container(
        ctx.emitter,
        array_reg,
        len_reg,
        &null_label,
    );
    abi::emit_load_from_address(ctx.emitter, len_reg, array_reg, 0);
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&null_label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, 0);
    ctx.emitter.label(&done_label);
    abi::store_at_offset(ctx.emitter, len_reg, offset - ITER_SNAPSHOT_LEN_OFFSET_DELTA);
}

/// Loads the live entry count from the indexed-array header in `array_reg` into `len_reg`,
/// substituting zero for a zero source pointer. Used by the by-reference `IterNext`
/// advancement, whose source slot `IterStart` has already normalized: a missed-read
/// sentinel was folded to zero there, so the per-iteration hot path needs only the cheap
/// zero check instead of the full null-container guard (issue #556). `len_reg` must not
/// alias `array_reg`.
fn load_live_indexed_array_length(ctx: &mut FunctionContext<'_>, array_reg: &str, len_reg: &str) {
    let null_label = ctx.next_label("iter_len_null_source");
    let done_label = ctx.next_label("iter_len_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("cbz {}, {}", array_reg, null_label));    // zero sources (normalized missed reads) have no header to read
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("test {}, {}", array_reg, array_reg));    // is the source the canonical zero container pointer?
            ctx.emitter.instruction(&format!("jz {}", null_label));             // zero sources (normalized missed reads) have no header to read
        }
    }
    abi::emit_load_from_address(ctx.emitter, len_reg, array_reg, 0);
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&null_label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, 0);
    ctx.emitter.label(&done_label);
}

/// Boxes a concrete iterator method result when the EIR result slot expects `Mixed`.
fn box_iterator_method_result_if_needed(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    return_ty: &PhpType,
) -> Result<()> {
    let Some(result) = inst.result else {
        return Ok(());
    };
    let result_ty = ctx.value_php_type(result)?;
    if result_ty == PhpType::Mixed && return_ty.codegen_repr() != PhpType::Mixed {
        emit_box_current_value_as_mixed(ctx.emitter, &return_ty.codegen_repr());
    }
    Ok(())
}

/// Lowers object iterator advancement using PHP's Iterator method protocol.
fn lower_object_iter_next(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
    class_name: &str,
) -> Result<()> {
    let first_label = ctx.next_label("object_iter_first");
    let valid_label = ctx.next_label("object_iter_valid");
    let started_reg = abi::int_result_reg(ctx.emitter);
    abi::load_at_offset(ctx.emitter, started_reg, offset - ITER_CURSOR_OFFSET_DELTA);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, #0", started_reg));       // check whether this object iterator has already yielded once
            ctx.emitter.instruction(&format!("b.eq {}", first_label));          // skip next() before the first valid() probe
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(                                            // check whether this object iterator has already yielded once
                &format!("test {}, {}", started_reg, started_reg)
            );
            ctx.emitter.instruction(&format!("je {}", first_label));            // skip next() before the first valid() probe
        }
    }
    emit_object_iterator_method_call(ctx, offset, class_name, "next")?;
    abi::emit_jump(ctx.emitter, &valid_label);
    ctx.emitter.label(&first_label);
    abi::emit_load_int_immediate(ctx.emitter, started_reg, 1);
    abi::store_at_offset(ctx.emitter, started_reg, offset - ITER_CURSOR_OFFSET_DELTA);
    ctx.emitter.label(&valid_label);
    emit_object_iterator_method_call(ctx, offset, class_name, "valid")?;
    Ok(())
}

/// Lowers iterator advancement through an `Iterator`-typed interface receiver.
fn lower_interface_iter_next(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
    interface_name: &str,
) -> Result<()> {
    let first_label = ctx.next_label("interface_iter_first");
    let valid_label = ctx.next_label("interface_iter_valid");
    let started_reg = abi::int_result_reg(ctx.emitter);
    abi::load_at_offset(ctx.emitter, started_reg, offset - ITER_CURSOR_OFFSET_DELTA);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, #0", started_reg));       // check whether this interface iterator has already yielded once
            ctx.emitter.instruction(&format!("b.eq {}", first_label));          // skip next() before the first valid() probe
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(                                            // check whether this interface iterator has already yielded once
                &format!("test {}, {}", started_reg, started_reg)
            );
            ctx.emitter.instruction(&format!("je {}", first_label));            // skip next() before the first valid() probe
        }
    }
    emit_interface_iterator_method_call(ctx, offset, interface_name, "next")?;
    abi::emit_jump(ctx.emitter, &valid_label);
    ctx.emitter.label(&first_label);
    abi::emit_load_int_immediate(ctx.emitter, started_reg, 1);
    abi::store_at_offset(ctx.emitter, started_reg, offset - ITER_CURSOR_OFFSET_DELTA);
    ctx.emitter.label(&valid_label);
    emit_interface_iterator_method_call(ctx, offset, interface_name, "valid")?;
    Ok(())
}

/// Emits a zero-argument Iterator method call against the object stored in iterator state.
fn emit_object_iterator_method_call(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
    class_name: &str,
    method_name: &str,
) -> Result<PhpType> {
    let method_key = php_symbol_key(method_name);
    if let Some(helper) = generator_iterator_runtime_helper(class_name, &method_key) {
        emit_generator_iterator_runtime_call(ctx, offset, helper);
        return Ok(generator_iterator_return_type(&method_key));
    }
    let target = resolve_method_call_target(ctx, class_name, &method_key, 1)?;
    let assignments = abi::build_outgoing_arg_assignments_for_target(
        ctx.emitter.target,
        &[PhpType::Object(class_name.to_string())],
        0,
    );
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::load_at_offset(ctx.emitter, result_reg, offset - ITER_SOURCE_OFFSET_DELTA);
    abi::emit_push_result_value(ctx.emitter, &PhpType::Object(class_name.to_string()));
    let overflow_bytes = abi::materialize_outgoing_args(ctx.emitter, &assignments);
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    if let Some(helper) = IntrinsicCall::instance_method(&target.impl_class, &method_key)
        .and_then(|intrinsic| intrinsic.runtime_helper())
    {
        abi::emit_call_label(ctx.emitter, helper);
    } else {
        emit_direct_resolved_method_call(ctx, &target)?;
    }
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, overflow_bytes);
    Ok(target.return_ty)
}

/// Emits a zero-argument Iterator method call through runtime interface metadata.
fn emit_interface_iterator_method_call(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
    interface_name: &str,
    method_name: &str,
) -> Result<PhpType> {
    let receiver_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    abi::load_at_offset(ctx.emitter, receiver_arg, offset - ITER_SOURCE_OFFSET_DELTA);
    let method_key = php_symbol_key(method_name);
    let done = if interface_name.trim_start_matches('\\') == "Iterator" {
        IntrinsicCall::instance_method("Generator", &method_key)
            .and_then(|intrinsic| intrinsic.runtime_helper())
            .map(|helper| emit_generator_interface_fast_path(ctx, helper))
    } else {
        None
    };
    let return_ty = emit_interface_dispatch_call(ctx, interface_name, &method_key, done.as_deref())?;
    if let Some(done) = done {
        ctx.emitter.label(&done);
    }
    Ok(return_ty)
}

/// Emits a fast path for Generator objects before generic `Iterator` interface dispatch.
fn emit_generator_interface_fast_path(
    ctx: &mut FunctionContext<'_>,
    helper: &str,
) -> String {
    let done = ctx.next_label("interface_dispatch_done");
    let not_generator = ctx.next_label("interface_dispatch_not_generator");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x10, [x0]");                           // load receiver class id before checking for the built-in Generator
            abi::emit_load_symbol_to_reg(ctx.emitter, "x11", "_generator_class_id", 0);
            ctx.emitter.instruction("cmp x10, x11");                            // compare receiver class id with Generator
            ctx.emitter.instruction(&format!("b.ne {}", not_generator));        // fall back to interface dispatch for non-Generator iterators
            abi::emit_call_label(ctx.emitter, helper);
            ctx.emitter.instruction(&format!("b {}", done));                    // skip generic interface dispatch after the fast path
            ctx.emitter.label(&not_generator);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r10, QWORD PTR [rdi]");                // load receiver class id before checking for the built-in Generator
            abi::emit_load_symbol_to_reg(ctx.emitter, "r11", "_generator_class_id", 0);
            ctx.emitter.instruction("cmp r10, r11");                            // compare receiver class id with Generator
            ctx.emitter.instruction(&format!("jne {}", not_generator));         // fall back to interface dispatch for non-Generator iterators
            abi::emit_call_label(ctx.emitter, helper);
            ctx.emitter.instruction(&format!("jmp {}", done));                  // skip generic interface dispatch after the fast path
            ctx.emitter.label(&not_generator);
        }
    }
    done
}

/// Emits the interface table scan and calls the resolved method slot.
pub(super) fn emit_interface_dispatch_call(
    ctx: &mut FunctionContext<'_>,
    interface_name: &str,
    method_key: &str,
    external_done: Option<&str>,
) -> Result<PhpType> {
    let normalized = interface_name.trim_start_matches('\\');
    let interface_info = ctx
        .module
        .interface_infos
        .get(normalized)
        .ok_or_else(|| CodegenIrError::unsupported(format!("iterator interface {}", normalized)))?;
    let interface_id = interface_info.interface_id as i64;
    let slot = interface_info.method_slots.get(method_key).copied().ok_or_else(|| {
        CodegenIrError::unsupported(format!("iterator interface method {}::{}", normalized, method_key))
    })?;
    let return_ty = interface_info
        .methods
        .get(method_key)
        .map(|sig| sig.return_type.clone())
        .unwrap_or(PhpType::Mixed);
    let scan_loop = ctx.next_label("interface_dispatch_scan");
    let found = ctx.next_label("interface_dispatch_found");
    let missing = ctx.next_label("interface_dispatch_missing");
    let local_done = external_done
        .map(str::to_string)
        .unwrap_or_else(|| ctx.next_label("interface_dispatch_done"));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x10, [x0]");                           // load receiver class id for interface metadata lookup
            abi::emit_symbol_address(ctx.emitter, "x11", "_class_interface_ptrs");
            ctx.emitter.instruction("ldr x11, [x11, x10, lsl #3]");             // select this class's interface metadata block
            ctx.emitter.instruction("ldr x10, [x11]");                          // load implemented interface count
            ctx.emitter.instruction("add x11, x11, #8");                        // move to first [interface id, table] pair
            abi::emit_load_int_immediate(ctx.emitter, "x13", interface_id);
            ctx.emitter.label(&scan_loop);
            ctx.emitter.instruction(&format!("cbz x10, {}", missing));          // stop when no implemented interface matched
            ctx.emitter.instruction("ldr x12, [x11]");                          // load current implemented interface id
            ctx.emitter.instruction("cmp x12, x13");                            // compare with target interface id
            ctx.emitter.instruction(&format!("b.eq {}", found));                // dispatch through this table when matched
            ctx.emitter.instruction("add x11, x11, #16");                       // advance to next interface metadata entry
            ctx.emitter.instruction("sub x10, x10, #1");                        // consume one interface entry
            ctx.emitter.instruction(&format!("b {}", scan_loop));               // continue scanning implemented interfaces
            ctx.emitter.label(&found);
            ctx.emitter.instruction("ldr x11, [x11, #8]");                      // load implementation table pointer
            if slot == 0 {
                ctx.emitter.instruction("ldr x11, [x11]");                      // load first interface method implementation pointer
            } else {
                ctx.emitter.instruction(                                        // load selected interface method implementation pointer
                    &format!("ldr x11, [x11, #{}]", slot * 8)
                );
            }
            ctx.emitter.instruction("blr x11");                                 // call resolved interface method implementation
            ctx.emitter.instruction(&format!("b {}", local_done));              // skip defensive missing-interface fallback
            ctx.emitter.label(&missing);
            ctx.emitter.instruction("mov x0, #0");                              // defensive fallback for invalid interface metadata
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r10, QWORD PTR [rdi]");                // load receiver class id for interface metadata lookup
            abi::emit_symbol_address(ctx.emitter, "r11", "_class_interface_ptrs");
            ctx.emitter.instruction("mov r11, QWORD PTR [r11 + r10 * 8]");      // select this class's interface metadata block
            ctx.emitter.instruction("mov r10, QWORD PTR [r11]");                // load implemented interface count
            ctx.emitter.instruction("add r11, 8");                              // move to first [interface id, table] pair
            abi::emit_load_int_immediate(ctx.emitter, "r9", interface_id);
            ctx.emitter.label(&scan_loop);
            ctx.emitter.instruction("test r10, r10");                           // check whether implemented interfaces remain
            ctx.emitter.instruction(&format!("je {}", missing));                // stop when no implemented interface matched
            ctx.emitter.instruction("mov r8, QWORD PTR [r11]");                 // load current implemented interface id
            ctx.emitter.instruction("cmp r8, r9");                              // compare with target interface id
            ctx.emitter.instruction(&format!("je {}", found));                  // dispatch through this table when matched
            ctx.emitter.instruction("add r11, 16");                             // advance to next interface metadata entry
            ctx.emitter.instruction("sub r10, 1");                              // consume one interface entry
            ctx.emitter.instruction(&format!("jmp {}", scan_loop));             // continue scanning implemented interfaces
            ctx.emitter.label(&found);
            ctx.emitter.instruction("mov r11, QWORD PTR [r11 + 8]");            // load implementation table pointer
            if slot == 0 {
                ctx.emitter.instruction("mov r11, QWORD PTR [r11]");            // load first interface method implementation pointer
            } else {
                ctx.emitter.instruction(                                        // load selected interface method implementation pointer
                    &format!("mov r11, QWORD PTR [r11 + {}]", slot * 8)
                );
            }
            ctx.emitter.instruction("call r11");                                // call resolved interface method implementation
            ctx.emitter.instruction(&format!("jmp {}", local_done));            // skip defensive missing-interface fallback
            ctx.emitter.label(&missing);
            ctx.emitter.instruction("xor eax, eax");                            // defensive fallback for invalid interface metadata
        }
    }
    if external_done.is_none() {
        ctx.emitter.label(&local_done);
    }
    Ok(return_ty)
}

/// Returns the PHP type produced by a `Generator` iterator runtime helper.
fn generator_iterator_return_type(method_key: &str) -> PhpType {
    match method_key {
        "valid" => PhpType::Bool,
        "rewind" | "next" => PhpType::Void,
        _ => PhpType::Mixed,
    }
}

/// Returns the runtime helper for `Generator` methods used by object iterator lowering.
fn generator_iterator_runtime_helper(class_name: &str, method_key: &str) -> Option<&'static str> {
    if class_name.trim_start_matches('\\') != "Generator" {
        return None;
    }
    match method_key {
        "rewind" => Some("__rt_gen_rewind"),
        "current" => Some("__rt_gen_current"),
        "key" => Some("__rt_gen_key"),
        "next" => Some("__rt_gen_next"),
        "valid" => Some("__rt_gen_valid"),
        _ => None,
    }
}

/// Emits a direct `Generator` runtime call for foreach iterator protocol methods.
fn emit_generator_iterator_runtime_call(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
    helper: &str,
) {
    let receiver_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    abi::load_at_offset(ctx.emitter, receiver_arg, offset - ITER_SOURCE_OFFSET_DELTA);
    abi::emit_call_label(ctx.emitter, helper);
}

/// Lowers iterator cleanup for owned relocation anchors in the stack-resident state.
pub(super) fn lower_iter_end(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.result.is_some() {
        return Err(CodegenIrError::invalid_module(
            "iter_end must not produce a result".to_string(),
        ));
    }
    let state = expect_local_slot(inst)?;
    let offset = ctx.local_offset(state)?;
    emit_release_successor_keys(ctx, offset);
    Ok(())
}

/// Emits AArch64 cursor advancement for a stack-resident indexed-array iterator.
///
/// By-value foreach compares against the snapshotted entry length, while by-reference
/// foreach reads the live array length so appended elements are visited like PHP.
fn lower_indexed_iter_next_aarch64(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
    by_ref: bool,
) {
    let index_reg = abi::secondary_scratch_reg(ctx.emitter);
    let len_reg = abi::tertiary_scratch_reg(ctx.emitter);
    let result_reg = abi::int_result_reg(ctx.emitter);
    let done_label = ctx.next_label("iter_next_done");

    abi::load_at_offset(ctx.emitter, index_reg, offset - ITER_CURSOR_OFFSET_DELTA);
    ctx.emitter.instruction(&format!("add {}, {}, #1", index_reg, index_reg));  // advance to the candidate indexed-array offset
    if by_ref {
        let array_reg = abi::int_result_reg(ctx.emitter);
        abi::load_at_offset(ctx.emitter, array_reg, offset - ITER_SOURCE_OFFSET_DELTA);
        load_live_indexed_array_length(ctx, array_reg, len_reg);
        ctx.emitter.instruction(&format!("cmp {}, {}", index_reg, len_reg));    // compare the candidate offset against the live array length
    } else {
        abi::load_at_offset(ctx.emitter, len_reg, offset - ITER_SNAPSHOT_LEN_OFFSET_DELTA);
        ctx.emitter.instruction(&format!("cmp {}, {}", index_reg, len_reg));    // compare the candidate offset against the snapshotted array length
    }
    ctx.emitter.instruction(&format!("cset {}, lt", result_reg));               // materialize whether another element is available
    ctx.emitter.instruction(&format!("b.ge {}", done_label));                   // leave the cursor unchanged once iteration reaches the end
    abi::store_at_offset(ctx.emitter, index_reg, offset - ITER_CURSOR_OFFSET_DELTA);
    ctx.emitter.label(&done_label);
}

/// Emits x86_64 cursor advancement for a stack-resident indexed-array iterator.
///
/// By-value foreach compares against the snapshotted entry length, while by-reference
/// foreach reads the live array length so appended elements are visited like PHP.
fn lower_indexed_iter_next_x86_64(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
    by_ref: bool,
) {
    let index_reg = abi::secondary_scratch_reg(ctx.emitter);
    let len_reg = abi::tertiary_scratch_reg(ctx.emitter);
    let result_reg = abi::int_result_reg(ctx.emitter);
    let done_label = ctx.next_label("iter_next_done");

    abi::load_at_offset(ctx.emitter, index_reg, offset - ITER_CURSOR_OFFSET_DELTA);
    ctx.emitter.instruction(&format!("add {}, 1", index_reg));                  // advance to the candidate indexed-array offset
    if by_ref {
        let array_reg = abi::symbol_scratch_reg(ctx.emitter);
        abi::load_at_offset(ctx.emitter, array_reg, offset - ITER_SOURCE_OFFSET_DELTA);
        load_live_indexed_array_length(ctx, array_reg, len_reg);
        ctx.emitter.instruction(&format!("cmp {}, {}", index_reg, len_reg));    // compare the candidate offset against the live array length
    } else {
        abi::load_at_offset(ctx.emitter, len_reg, offset - ITER_SNAPSHOT_LEN_OFFSET_DELTA);
        ctx.emitter.instruction(&format!("cmp {}, {}", index_reg, len_reg));    // compare the candidate offset against the snapshotted array length
    }
    ctx.emitter.instruction("setl al");                                         // materialize whether another element is available in the low result byte
    ctx.emitter.instruction(&format!("movzx {}, al", result_reg));              // widen the availability flag into the integer result register
    ctx.emitter.instruction(&format!("jge {}", done_label));                    // leave the cursor unchanged once iteration reaches the end
    abi::store_at_offset(ctx.emitter, index_reg, offset - ITER_CURSOR_OFFSET_DELTA);
    ctx.emitter.label(&done_label);
}

/// Emits AArch64 advancement for a stack-resident associative-array iterator.
///
/// The walk uses the dereferencing iterator, so a tag-11 entry reports the value it references
/// while the returned entry address still points at the reference entry for by-reference
/// binding. Reloading a relocated source happens earlier, in [`emit_reload_live_iter_source`],
/// because the dynamic dispatch on heap kind must also see the live pointer.
fn lower_hash_iter_next_aarch64(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
    capture_successor: bool,
) {
    abi::load_at_offset(ctx.emitter, "x0", offset - ITER_SOURCE_OFFSET_DELTA);
    abi::load_at_offset(ctx.emitter, "x1", offset - ITER_CURSOR_OFFSET_DELTA);
    abi::emit_call_label(ctx.emitter, "__rt_hash_iter_next_value");
    ctx.emitter.instruction("cmn x0, #1");                                      // check whether the hash iterator returned the done sentinel
    abi::store_at_offset(ctx.emitter, "x0", offset - ITER_CURSOR_OFFSET_DELTA);
    abi::store_at_offset(ctx.emitter, "x1", offset - ITER_KEY_LO_OFFSET_DELTA);
    abi::store_at_offset(ctx.emitter, "x2", offset - ITER_KEY_HI_OFFSET_DELTA);
    abi::store_at_offset(ctx.emitter, "x3", offset - ITER_VALUE_LO_OFFSET_DELTA);
    abi::store_at_offset(ctx.emitter, "x4", offset - ITER_VALUE_HI_OFFSET_DELTA);
    abi::store_at_offset(ctx.emitter, "x5", offset - ITER_VALUE_TAG_OFFSET_DELTA);
    abi::store_at_offset(ctx.emitter, "x6", offset - ITER_VALUE_ADDR_OFFSET_DELTA);
    if capture_successor {
        emit_capture_successor_keys(ctx, offset);
    }
    abi::load_at_offset(ctx.emitter, "x9", offset - ITER_CURSOR_OFFSET_DELTA);
    ctx.emitter.instruction("cmn x9, #1");                                      // check the saved cursor after anchor retain calls
    ctx.emitter.instruction("cset x0, ne");                                     // materialize whether the associative iterator has a current entry
}

/// Emits x86_64 advancement for a stack-resident associative-array iterator.
///
/// Mirrors [`lower_hash_iter_next_aarch64`].
fn lower_hash_iter_next_x86_64(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
    capture_successor: bool,
) {
    abi::load_at_offset(ctx.emitter, "rdi", offset - ITER_SOURCE_OFFSET_DELTA);
    abi::load_at_offset(ctx.emitter, "rsi", offset - ITER_CURSOR_OFFSET_DELTA);
    abi::emit_call_label(ctx.emitter, "__rt_hash_iter_next_value");
    ctx.emitter.instruction("cmp rax, -1");                                     // check whether the hash iterator returned the done sentinel
    abi::store_at_offset(ctx.emitter, "rax", offset - ITER_CURSOR_OFFSET_DELTA);
    abi::store_at_offset(ctx.emitter, "rdi", offset - ITER_KEY_LO_OFFSET_DELTA);
    abi::store_at_offset(ctx.emitter, "rdx", offset - ITER_KEY_HI_OFFSET_DELTA);
    abi::store_at_offset(ctx.emitter, "rcx", offset - ITER_VALUE_LO_OFFSET_DELTA);
    abi::store_at_offset(ctx.emitter, "r8", offset - ITER_VALUE_HI_OFFSET_DELTA);
    abi::store_at_offset(ctx.emitter, "r9", offset - ITER_VALUE_TAG_OFFSET_DELTA);
    abi::store_at_offset(ctx.emitter, "r10", offset - ITER_VALUE_ADDR_OFFSET_DELTA);
    if capture_successor {
        emit_capture_successor_keys(ctx, offset);
    }
    abi::load_at_offset(ctx.emitter, "r10", offset - ITER_CURSOR_OFFSET_DELTA);
    ctx.emitter.instruction("cmp r10, -1");                                     // check the saved cursor after anchor retain calls
    ctx.emitter.instruction("setne al");                                        // materialize whether the associative iterator has a current entry
    ctx.emitter.instruction("movzx rax, al");                                   // widen the availability flag into the integer result register
}

/// Records owned keys for the next two entries the cursor can yield.
///
/// Emitted only for a by-reference walk that named an origin, so an ordinary `foreach` pays
/// nothing for it. A post-last or done cursor leaves both anchors absent. String keys are retained
/// after both pairs have been copied out of the table, so helper calls cannot invalidate the entry
/// address used to discover the fallback.
fn emit_capture_successor_keys(ctx: &mut FunctionContext<'_>, offset: usize) {
    let absent = ctx.next_label("iter_successor_absent");
    let retain = ctx.next_label("iter_successor_retain");
    let done = ctx.next_label("iter_successor_done");
    emit_clear_successor_keys(ctx, offset);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::load_at_offset_scratch(ctx.emitter, "x10", offset - ITER_CURSOR_OFFSET_DELTA, "x12");
            ctx.emitter.instruction("cmp x10, #0");                             // do the end sentinels leave any successor to anchor on?
            ctx.emitter.instruction(&format!("b.le {}", absent));               // the post-last and done cursors have none
            abi::load_at_offset_scratch(ctx.emitter, "x9", offset - ITER_SOURCE_OFFSET_DELTA, "x12");
            ctx.emitter.instruction("ldr x9, [x9, #40]");                       // locate the separately allocated hash entries
            ctx.emitter.instruction("sub x10, x10, #1");                        // decode the successor slot index from the cursor
            ctx.emitter.instruction("lsl x10, x10, #6");                        // 64 bytes per hash entry
            ctx.emitter.instruction("add x10, x9, x10");                        // advance from entry storage to the successor slot
            ctx.emitter.instruction("ldr x11, [x10, #8]");                      // successor key pointer or integer payload
            ctx.emitter.instruction("ldr x12, [x10, #16]");                     // successor key length or integer sentinel
            abi::store_at_offset_scratch(ctx.emitter, "x11", offset - ITER_NEXT_KEY_LO_OFFSET_DELTA, "x9");
            abi::store_at_offset_scratch(ctx.emitter, "x12", offset - ITER_NEXT_KEY_HI_OFFSET_DELTA, "x9");
            ctx.emitter.instruction("ldr x13, [x10, #56]");                     // load the successor's own insertion-order successor
            ctx.emitter.instruction("cmp x13, #-1");                            // is the primary anchor the current tail?
            ctx.emitter.instruction(&format!("b.eq {retain}"));                 // no fallback exists, retain only the primary key
            abi::load_at_offset_scratch(ctx.emitter, "x9", offset - ITER_SOURCE_OFFSET_DELTA, "x12");
            ctx.emitter.instruction("ldr x9, [x9, #40]");                       // locate the separately allocated hash entries
            ctx.emitter.instruction("lsl x13, x13, #6");                        // 64 bytes per fallback hash entry
            ctx.emitter.instruction("add x13, x9, x13");                        // advance from entry storage to the fallback slot
            ctx.emitter.instruction("ldr x11, [x13, #8]");                      // fallback key pointer or integer payload
            ctx.emitter.instruction("ldr x12, [x13, #16]");                     // fallback key length or integer sentinel
            abi::store_at_offset_scratch(ctx.emitter, "x11", offset - ITER_FALLBACK_KEY_LO_OFFSET_DELTA, "x9");
            abi::store_at_offset_scratch(ctx.emitter, "x12", offset - ITER_FALLBACK_KEY_HI_OFFSET_DELTA, "x9");
        }
        Arch::X86_64 => {
            abi::load_at_offset(ctx.emitter, "r10", offset - ITER_CURSOR_OFFSET_DELTA);
            ctx.emitter.instruction("cmp r10, 0");                              // do the end sentinels leave any successor to anchor on?
            ctx.emitter.instruction(&format!("jle {}", absent));                // the post-last and done cursors have none
            abi::load_at_offset(ctx.emitter, "r11", offset - ITER_SOURCE_OFFSET_DELTA);
            ctx.emitter.instruction("mov r11, QWORD PTR [r11 + 40]");           // locate the separately allocated hash entries
            ctx.emitter.instruction("sub r10, 1");                              // decode the successor slot index from the cursor
            ctx.emitter.instruction("shl r10, 6");                              // 64 bytes per hash entry
            ctx.emitter.instruction("add r10, r11");                            // advance from entry storage to the successor slot
            ctx.emitter.instruction("mov r11, QWORD PTR [r10 + 8]");            // successor key pointer or integer payload
            ctx.emitter.instruction("mov rcx, QWORD PTR [r10 + 16]");           // successor key length or integer sentinel
            abi::store_at_offset(ctx.emitter, "r11", offset - ITER_NEXT_KEY_LO_OFFSET_DELTA);
            abi::store_at_offset(ctx.emitter, "rcx", offset - ITER_NEXT_KEY_HI_OFFSET_DELTA);
            ctx.emitter.instruction("mov r9, QWORD PTR [r10 + 56]");            // load the successor's own insertion-order successor
            ctx.emitter.instruction("cmp r9, -1");                              // is the primary anchor the current tail?
            ctx.emitter.instruction(&format!("je {retain}"));                   // no fallback exists, retain only the primary key
            abi::load_at_offset(ctx.emitter, "r11", offset - ITER_SOURCE_OFFSET_DELTA);
            ctx.emitter.instruction("mov r11, QWORD PTR [r11 + 40]");           // locate the separately allocated hash entries
            ctx.emitter.instruction("shl r9, 6");                               // 64 bytes per fallback hash entry
            ctx.emitter.instruction("add r9, r11");                             // advance from entry storage to the fallback slot
            ctx.emitter.instruction("mov r11, QWORD PTR [r9 + 8]");             // fallback key pointer or integer payload
            ctx.emitter.instruction("mov rcx, QWORD PTR [r9 + 16]");            // fallback key length or integer sentinel
            abi::store_at_offset(ctx.emitter, "r11", offset - ITER_FALLBACK_KEY_LO_OFFSET_DELTA);
            abi::store_at_offset(ctx.emitter, "rcx", offset - ITER_FALLBACK_KEY_HI_OFFSET_DELTA);
        }
    }
    ctx.emitter.label(&retain);
    emit_retain_successor_keys(ctx, offset);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&absent);
    ctx.emitter.label(&done);
}

/// Boxes the current AArch64 hash key saved by `IterNext` into a `Mixed` cell.
fn load_current_hash_key_as_mixed_aarch64(ctx: &mut FunctionContext<'_>, offset: usize) {
    let key_string = ctx.next_label("iter_hash_key_string");
    let key_done = ctx.next_label("iter_hash_key_done");
    abi::load_at_offset(ctx.emitter, "x1", offset - ITER_KEY_LO_OFFSET_DELTA);
    abi::load_at_offset(ctx.emitter, "x2", offset - ITER_KEY_HI_OFFSET_DELTA);
    ctx.emitter.instruction("cmn x2, #1");                                      // check whether this normalized hash key is integer-backed
    ctx.emitter.instruction(&format!("b.ne {}", key_string));                   // branch to string-key boxing when key_hi is not the integer sentinel
    ctx.emitter.instruction("mov x0, #0");                                      // runtime tag 0 = integer mixed key
    ctx.emitter.instruction("mov x2, xzr");                                     // integer mixed payloads do not use a high word
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
    ctx.emitter.instruction(&format!("b {}", key_done));                        // skip string-key boxing after producing the integer key box
    ctx.emitter.label(&key_string);
    ctx.emitter.instruction("mov x0, #1");                                      // runtime tag 1 = string mixed key
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
    ctx.emitter.label(&key_done);
}

/// Boxes the current x86_64 hash key saved by `IterNext` into a `Mixed` cell.
fn load_current_hash_key_as_mixed_x86_64(ctx: &mut FunctionContext<'_>, offset: usize) {
    let key_string = ctx.next_label("iter_hash_key_string");
    let key_done = ctx.next_label("iter_hash_key_done");
    abi::load_at_offset(ctx.emitter, "rdi", offset - ITER_KEY_LO_OFFSET_DELTA);
    abi::load_at_offset(ctx.emitter, "rdx", offset - ITER_KEY_HI_OFFSET_DELTA);
    ctx.emitter.instruction("cmp rdx, -1");                                     // check whether this normalized hash key is integer-backed
    ctx.emitter.instruction(&format!("jne {}", key_string));                    // branch to string-key boxing when key_hi is not the integer sentinel
    ctx.emitter.instruction("xor esi, esi");                                    // integer mixed payloads do not use a high word
    ctx.emitter.instruction("mov eax, 0");                                      // runtime tag 0 = integer mixed key
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
    ctx.emitter.instruction(&format!("jmp {}", key_done));                      // skip string-key boxing after producing the integer key box
    ctx.emitter.label(&key_string);
    ctx.emitter.instruction("mov rsi, rdx");                                    // move the string key length into the mixed helper high-word register
    ctx.emitter.instruction("mov eax, 1");                                      // runtime tag 1 = string mixed key
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
    ctx.emitter.label(&key_done);
}

/// Keeps an unpacked PHP reference's cell while boxing ordinary iterator values by value.
fn load_current_hash_call_value_as_mixed(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
    preserve_reference: bool,
    promote_reference: bool,
) {
    if preserve_reference || promote_reference {
        let ordinary = ctx.next_label("iter_call_value_ordinary");
        let done = ctx.next_label("iter_call_value_done");
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                abi::load_at_offset(ctx.emitter, "x10", offset - ITER_VALUE_ADDR_OFFSET_DELTA);
                if promote_reference {
                    ctx.emitter.instruction("mov x0, x10");                   // promote this writable entry in the caller's array
                    abi::emit_call_label(ctx.emitter, "__rt_hash_entry_make_reference");
                    ctx.emitter.instruction("mov x10, x0");                   // carry the managed cell into the descriptor marker
                } else {
                    ctx.emitter.instruction("ldr x9, [x10, #16]");           // inspect the original entry before iterator dereferencing
                    ctx.emitter.instruction("cmp x9, #11");                  // only a PHP reference shares caller storage
                    ctx.emitter.instruction(&format!("b.ne {ordinary}"));    // ordinary entries keep their by-value result
                    ctx.emitter.instruction("ldr x10, [x10]");                // borrow the cell from the pinned source
                }
                ctx.emitter.instruction("mov x9, #11");                       // encode a descriptor reference marker
                ctx.emitter.instruction("mov x11, #7");                       // its referenced PHP value is boxed Mixed
                emit_box_runtime_payload_as_mixed(ctx.emitter, "x9", "x10", "x11");
            }
            Arch::X86_64 => {
                abi::load_at_offset(ctx.emitter, "r10", offset - ITER_VALUE_ADDR_OFFSET_DELTA);
                if promote_reference {
                    ctx.emitter.instruction("mov rdi, r10");                 // promote this writable entry in the caller's array
                    abi::emit_call_label(ctx.emitter, "__rt_hash_entry_make_reference");
                    ctx.emitter.instruction("mov rcx, rax");                 // carry the managed cell into the descriptor marker
                } else {
                    ctx.emitter.instruction("cmp QWORD PTR [r10 + 16], 11"); // inspect the original entry before iterator dereferencing
                    ctx.emitter.instruction(&format!("jne {ordinary}"));     // ordinary entries keep their by-value result
                    ctx.emitter.instruction("mov rcx, QWORD PTR [r10]");     // borrow the cell from the pinned source
                }
                ctx.emitter.instruction("mov r9, 11");                        // encode a descriptor reference marker
                ctx.emitter.instruction("mov r8, 7");                         // its referenced PHP value is boxed Mixed
                emit_box_runtime_payload_as_mixed(ctx.emitter, "r9", "rcx", "r8");
            }
        }
        abi::emit_jump(ctx.emitter, &done);
        ctx.emitter.label(&ordinary);
        match ctx.emitter.target.arch {
            Arch::AArch64 => load_current_hash_value_as_mixed_aarch64(ctx, offset),
            Arch::X86_64 => load_current_hash_value_as_mixed_x86_64(ctx, offset),
        }
        ctx.emitter.label(&done);
    } else {
        match ctx.emitter.target.arch {
            Arch::AArch64 => load_current_hash_value_as_mixed_aarch64(ctx, offset),
            Arch::X86_64 => load_current_hash_value_as_mixed_x86_64(ctx, offset),
        }
    }
}

/// Boxes the current AArch64 hash value payload saved by `IterNext` into `Mixed`.
fn load_current_hash_value_as_mixed_aarch64(ctx: &mut FunctionContext<'_>, offset: usize) {
    abi::load_at_offset(ctx.emitter, "x5", offset - ITER_VALUE_TAG_OFFSET_DELTA);
    abi::load_at_offset(ctx.emitter, "x3", offset - ITER_VALUE_LO_OFFSET_DELTA);
    abi::load_at_offset(ctx.emitter, "x4", offset - ITER_VALUE_HI_OFFSET_DELTA);
    box_hash_payload_as_mixed_aarch64(ctx);
}

/// Boxes the current x86_64 hash value payload saved by `IterNext` into `Mixed`.
fn load_current_hash_value_as_mixed_x86_64(ctx: &mut FunctionContext<'_>, offset: usize) {
    abi::load_at_offset(ctx.emitter, "r9", offset - ITER_VALUE_TAG_OFFSET_DELTA);
    abi::load_at_offset(ctx.emitter, "rcx", offset - ITER_VALUE_LO_OFFSET_DELTA);
    abi::load_at_offset(ctx.emitter, "r8", offset - ITER_VALUE_HI_OFFSET_DELTA);
    box_hash_payload_as_mixed_x86_64(ctx);
}

/// Boxes or retains an AArch64 hash payload as an owned `Mixed` value.
fn box_hash_payload_as_mixed_aarch64(ctx: &mut FunctionContext<'_>) {
    let inspect_tagged_box = ctx.next_label("iter_hash_value_inspect_box");
    let done = ctx.next_label("iter_hash_value_boxed");
    ctx.emitter.instruction("cmp x5, #7");                                      // does the hash entry use the Mixed-or-iterable runtime tag?
    ctx.emitter.instruction(&format!("b.eq {}", inspect_tagged_box));           // inspect tag-7 payloads because iterable hashes also use that tag
    emit_box_runtime_payload_as_mixed(ctx.emitter, "x5", "x3", "x4");
    ctx.emitter.instruction(&format!("b {}", done));                            // skip tag-7 inspection after boxing a concrete payload
    ctx.emitter.label(&inspect_tagged_box);
    box_tagged_hash_payload_as_mixed_aarch64(ctx);
    ctx.emitter.label(&done);
}

/// Boxes or retains an AArch64 tag-7 hash payload as Mixed after checking its heap kind.
fn box_tagged_hash_payload_as_mixed_aarch64(ctx: &mut FunctionContext<'_>) {
    let reuse_box = ctx.next_label("iter_hash_value_reuse_box");
    let done = ctx.next_label("iter_hash_value_tagged_done");
    ctx.emitter.instruction("str x3, [sp, #-16]!");                             // preserve the tag-7 payload while probing its heap kind
    ctx.emitter.instruction("mov x0, x3");                                      // pass the tag-7 payload to the heap-kind probe
    abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
    ctx.emitter.instruction("cmp x0, #5");                                      // heap kind 5 means the payload is already a boxed Mixed cell
    ctx.emitter.instruction(&format!("b.eq {}", reuse_box));                    // retain existing Mixed boxes instead of nesting them
    ctx.emitter.instruction("ldr x0, [sp], #16");                               // restore the raw iterable payload before boxing it as Mixed
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Iterable);
    ctx.emitter.instruction(&format!("b {}", done));                            // skip the existing Mixed retention path
    ctx.emitter.label(&reuse_box);
    ctx.emitter.instruction("ldr x0, [sp], #16");                               // restore the existing Mixed box before retaining it
    abi::emit_call_label(ctx.emitter, "__rt_incref");
    ctx.emitter.label(&done);
}

/// Boxes or retains an x86_64 hash payload as an owned `Mixed` value.
fn box_hash_payload_as_mixed_x86_64(ctx: &mut FunctionContext<'_>) {
    let inspect_tagged_box = ctx.next_label("iter_hash_value_inspect_box");
    let done = ctx.next_label("iter_hash_value_boxed");
    ctx.emitter.instruction("cmp r9, 7");                                       // does the hash entry use the Mixed-or-iterable runtime tag?
    ctx.emitter.instruction(&format!("je {}", inspect_tagged_box));             // inspect tag-7 payloads because iterable hashes also use that tag
    emit_box_runtime_payload_as_mixed(ctx.emitter, "r9", "rcx", "r8");
    ctx.emitter.instruction(&format!("jmp {}", done));                          // skip tag-7 inspection after boxing a concrete payload
    ctx.emitter.label(&inspect_tagged_box);
    box_tagged_hash_payload_as_mixed_x86_64(ctx);
    ctx.emitter.label(&done);
}

/// Boxes or retains an x86_64 tag-7 hash payload as Mixed after checking its heap kind.
fn box_tagged_hash_payload_as_mixed_x86_64(ctx: &mut FunctionContext<'_>) {
    let reuse_box = ctx.next_label("iter_hash_value_reuse_box");
    let done = ctx.next_label("iter_hash_value_tagged_done");
    abi::emit_push_reg(ctx.emitter, "rcx");
    ctx.emitter.instruction("mov rax, rcx");                                    // pass the tag-7 payload to the heap-kind probe
    abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
    ctx.emitter.instruction("cmp rax, 5");                                      // heap kind 5 means the payload is already a boxed Mixed cell
    ctx.emitter.instruction(&format!("je {}", reuse_box));                      // retain existing Mixed boxes instead of nesting them
    abi::emit_pop_reg(ctx.emitter, "rax");
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Iterable);
    ctx.emitter.instruction(&format!("jmp {}", done));                          // skip the existing Mixed retention path
    ctx.emitter.label(&reuse_box);
    abi::emit_pop_reg(ctx.emitter, "rax");
    abi::emit_call_label(ctx.emitter, "__rt_incref");
    ctx.emitter.label(&done);
}

/// Loads a runtime-typed AArch64 indexed-array element and returns it as an owned `Mixed` box.
fn load_current_dynamic_indexed_value_as_mixed_aarch64(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
) {
    let string_case = ctx.next_label("iter_dynamic_indexed_string");
    let loaded = ctx.next_label("iter_dynamic_indexed_loaded");
    let reuse_box = ctx.next_label("iter_dynamic_indexed_reuse_box");
    let done = ctx.next_label("iter_dynamic_indexed_done");
    abi::load_at_offset_scratch(ctx.emitter, "x11", offset - ITER_SOURCE_OFFSET_DELTA, "x9");
    abi::load_at_offset(ctx.emitter, "x0", offset - ITER_CURSOR_OFFSET_DELTA);
    ctx.emitter.instruction("ldr x5, [x11, #-8]");                              // load the packed indexed-array heap metadata
    ctx.emitter.instruction("lsr x5, x5, #8");                                  // move the runtime value_type tag into the low bits
    ctx.emitter.instruction("and x5, x5, #0x7f");                               // isolate the indexed-array value_type tag
    ctx.emitter.instruction("cmp x5, #1");                                      // does this indexed array store string slots?
    ctx.emitter.instruction(&format!("b.eq {}", string_case));                  // branch to the 16-byte string-slot loader
    ctx.emitter.instruction("add x11, x11, #24");                               // skip the indexed-array header to the 8-byte payload slots
    ctx.emitter.instruction("ldr x3, [x11, x0, lsl #3]");                       // load the scalar or pointer payload from the selected indexed slot
    ctx.emitter.instruction("mov x4, xzr");                                     // non-string indexed payloads have no high payload word
    ctx.emitter.instruction(&format!("b {}", loaded));                          // continue with a normalized runtime payload triple

    ctx.emitter.label(&string_case);
    ctx.emitter.instruction("lsl x10, x0, #4");                                 // scale the index by the 16-byte string slot size
    ctx.emitter.instruction("add x11, x11, x10");                               // move to the selected string slot
    ctx.emitter.instruction("add x11, x11, #24");                               // skip the indexed-array header before loading the slot
    ctx.emitter.instruction("ldr x3, [x11]");                                   // load the string pointer payload
    ctx.emitter.instruction("ldr x4, [x11, #8]");                               // load the string length payload

    ctx.emitter.label(&loaded);
    ctx.emitter.instruction("cmp x5, #7");                                      // does the slot already hold a boxed Mixed value?
    ctx.emitter.instruction(&format!("b.eq {}", reuse_box));                    // retain existing Mixed boxes instead of nesting them
    emit_box_runtime_payload_as_mixed(ctx.emitter, "x5", "x3", "x4");
    ctx.emitter.instruction(&format!("b {}", done));                            // skip the existing-box retention path
    ctx.emitter.label(&reuse_box);
    ctx.emitter.instruction("mov x0, x3");                                      // pass the existing Mixed box to the retain helper
    abi::emit_call_label(ctx.emitter, "__rt_incref");
    ctx.emitter.label(&done);
}

/// Loads a runtime-typed x86_64 indexed-array element and returns it as an owned `Mixed` box.
fn load_current_dynamic_indexed_value_as_mixed_x86_64(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
) {
    let string_case = ctx.next_label("iter_dynamic_indexed_string");
    let loaded = ctx.next_label("iter_dynamic_indexed_loaded");
    let reuse_box = ctx.next_label("iter_dynamic_indexed_reuse_box");
    let done = ctx.next_label("iter_dynamic_indexed_done");
    abi::load_at_offset(ctx.emitter, "r11", offset - ITER_SOURCE_OFFSET_DELTA);
    abi::load_at_offset(ctx.emitter, "r10", offset - ITER_CURSOR_OFFSET_DELTA);
    ctx.emitter.instruction("mov r9, QWORD PTR [r11 - 8]");                     // load the packed indexed-array heap metadata
    ctx.emitter.instruction("shr r9, 8");                                       // move the runtime value_type tag into the low bits
    ctx.emitter.instruction("and r9, 0x7f");                                    // isolate the indexed-array value_type tag
    ctx.emitter.instruction("cmp r9, 1");                                       // does this indexed array store string slots?
    ctx.emitter.instruction(&format!("je {}", string_case));                    // branch to the 16-byte string-slot loader
    ctx.emitter.instruction("add r11, 24");                                     // skip the indexed-array header to the 8-byte payload slots
    ctx.emitter.instruction("mov rcx, QWORD PTR [r11 + r10 * 8]");              // load the scalar or pointer payload from the selected indexed slot
    ctx.emitter.instruction("xor r8, r8");                                      // non-string indexed payloads have no high payload word
    ctx.emitter.instruction(&format!("jmp {}", loaded));                        // continue with a normalized runtime payload triple

    ctx.emitter.label(&string_case);
    ctx.emitter.instruction("shl r10, 4");                                      // scale the index by the 16-byte string slot size
    ctx.emitter.instruction("add r11, r10");                                    // move to the selected string slot
    ctx.emitter.instruction("add r11, 24");                                     // skip the indexed-array header before loading the slot
    ctx.emitter.instruction("mov rcx, QWORD PTR [r11]");                        // load the string pointer payload
    ctx.emitter.instruction("mov r8, QWORD PTR [r11 + 8]");                     // load the string length payload

    ctx.emitter.label(&loaded);
    ctx.emitter.instruction("cmp r9, 7");                                       // does the slot already hold a boxed Mixed value?
    ctx.emitter.instruction(&format!("je {}", reuse_box));                      // retain existing Mixed boxes instead of nesting them
    emit_box_runtime_payload_as_mixed(ctx.emitter, "r9", "rcx", "r8");
    ctx.emitter.instruction(&format!("jmp {}", done));                          // skip the existing-box retention path
    ctx.emitter.label(&reuse_box);
    ctx.emitter.instruction("mov rax, rcx");                                    // pass the existing Mixed box to the retain helper
    abi::emit_call_label(ctx.emitter, "__rt_incref");
    ctx.emitter.label(&done);
}

/// Loads the current indexed-array element into AArch64 result registers.
fn load_current_array_value_aarch64(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
    elem_ty: &PhpType,
) -> Result<()> {
    let array_reg = "x12";
    let index_reg = abi::secondary_scratch_reg(ctx.emitter);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::load_at_offset_scratch(ctx.emitter, array_reg, offset - ITER_SOURCE_OFFSET_DELTA, "x9");
    abi::load_at_offset(ctx.emitter, index_reg, offset - ITER_CURSOR_OFFSET_DELTA);
    match elem_ty {
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
        }
        PhpType::Int | PhpType::Bool | PhpType::Callable | PhpType::Mixed => {
            ctx.emitter.instruction(                                            // skip the indexed-array header to reach element payloads
                &format!("add {}, {}, #24", array_reg, array_reg)
            );
            ctx.emitter.instruction(                                            // load the selected pointer-sized indexed-array element
                &format!("ldr {}, [{}, {}, lsl #3]", result_reg, array_reg, index_reg)
            );
        }
        PhpType::Float => {
            ctx.emitter.instruction(                                            // skip the indexed-array header to reach float payloads
                &format!("add {}, {}, #24", array_reg, array_reg)
            );
            ctx.emitter.instruction(                                            // load the selected indexed-array float element
                &format!("ldr d0, [{}, {}, lsl #3]", array_reg, index_reg)
            );
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            ctx.emitter.instruction(                                            // scale the string-array offset by pointer-plus-length slot size
                &format!("lsl {}, {}, #4", index_reg, index_reg)
            );
            ctx.emitter.instruction(                                            // move to the selected string slot within the indexed array
                &format!("add {}, {}, {}", array_reg, array_reg, index_reg)
            );
            ctx.emitter.instruction(                                            // skip the indexed-array header before loading the string slot
                &format!("add {}, {}, #24", array_reg, array_reg)
            );
            abi::emit_load_from_address(ctx.emitter, ptr_reg, array_reg, 0);
            abi::emit_load_from_address(ctx.emitter, len_reg, array_reg, 8);
        }
        other if other.is_refcounted() => {
            ctx.emitter.instruction(                                            // skip the indexed-array header to reach refcounted payloads
                &format!("add {}, {}, #24", array_reg, array_reg)
            );
            ctx.emitter.instruction(                                            // load the selected refcounted indexed-array element
                &format!("ldr {}, [{}, {}, lsl #3]", result_reg, array_reg, index_reg)
            );
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "indexed iterator value PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Loads the current indexed-array element into x86_64 result registers.
fn load_current_array_value_x86_64(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
    elem_ty: &PhpType,
) -> Result<()> {
    let array_reg = abi::symbol_scratch_reg(ctx.emitter);
    let index_reg = abi::secondary_scratch_reg(ctx.emitter);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::load_at_offset(ctx.emitter, array_reg, offset - ITER_SOURCE_OFFSET_DELTA);
    abi::load_at_offset(ctx.emitter, index_reg, offset - ITER_CURSOR_OFFSET_DELTA);
    match elem_ty {
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
        }
        PhpType::Int | PhpType::Bool | PhpType::Callable | PhpType::Mixed => {
            ctx.emitter.instruction(                                            // skip the indexed-array header to reach element payloads
                &format!("lea {}, [{} + 24]", array_reg, array_reg)
            );
            ctx.emitter.instruction(                                            // load the selected pointer-sized indexed-array element
                &format!("mov {}, QWORD PTR [{} + {} * 8]", result_reg, array_reg, index_reg)
            );
        }
        PhpType::Float => {
            ctx.emitter.instruction(                                            // skip the indexed-array header to reach float payloads
                &format!("lea {}, [{} + 24]", array_reg, array_reg)
            );
            ctx.emitter.instruction(                                            // load the selected indexed-array float element
                &format!("movsd xmm0, QWORD PTR [{} + {} * 8]", array_reg, index_reg)
            );
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            ctx.emitter.instruction(&format!("shl {}, 4", index_reg));          // scale the string-array offset by pointer-plus-length slot size
            ctx.emitter.instruction(                                            // move to the selected string slot within the indexed array
                &format!("add {}, {}", array_reg, index_reg)
            );
            ctx.emitter.instruction(&format!("add {}, 24", array_reg));         // skip the indexed-array header before loading the string slot
            abi::emit_load_from_address(ctx.emitter, ptr_reg, array_reg, 0);
            abi::emit_load_from_address(ctx.emitter, len_reg, array_reg, 8);
        }
        other if other.is_refcounted() => {
            ctx.emitter.instruction(                                            // skip the indexed-array header to reach refcounted payloads
                &format!("lea {}, [{} + 24]", array_reg, array_reg)
            );
            ctx.emitter.instruction(                                            // load the selected refcounted indexed-array element
                &format!("mov {}, QWORD PTR [{} + {} * 8]", result_reg, array_reg, index_reg)
            );
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "indexed iterator value PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Returns the source layout handled by a stack-resident iterator.
fn iterator_source_kind(
    ctx: &FunctionContext<'_>,
    iterator: ValueId,
    inst: &Instruction,
) -> Result<IteratorSourceKind> {
    iterator_source_kind_from_type(ctx, &iterator_source_type(ctx, iterator, inst)?, inst)
}

/// Returns the source PHP type referenced by an `IterStart` result value.
fn iterator_source_type(
    ctx: &FunctionContext<'_>,
    iterator: ValueId,
    inst: &Instruction,
) -> Result<PhpType> {
    let source = iterator_source_value(ctx, iterator, inst)?;
    ctx.value_php_type(source)
}

/// Returns true when an iterator handle came from a by-reference `IterStart`.
fn iterator_is_by_ref(
    ctx: &FunctionContext<'_>,
    iterator: ValueId,
    inst: &Instruction,
) -> Result<bool> {
    let iter_start = iterator_start_instruction(ctx, iterator, inst)?;
    Ok(iter_start_is_by_ref(iter_start))
}

/// Returns the source operand for an iterator handle, rejecting malformed EIR.
fn iterator_source_value(
    ctx: &FunctionContext<'_>,
    iterator: ValueId,
    inst: &Instruction,
) -> Result<ValueId> {
    let iter_start = iterator_start_instruction(ctx, iterator, inst)?;
    iter_start
        .operands
        .first()
        .copied()
        .ok_or_else(|| CodegenIrError::invalid_module("iter_start missing source operand".to_string()))
}

/// Returns the `IterStart` instruction that produced an iterator handle.
fn iterator_start_instruction<'a>(
    ctx: &'a FunctionContext<'_>,
    iterator: ValueId,
    inst: &Instruction,
) -> Result<&'a Instruction> {
    let value = ctx
        .function
        .value(iterator)
        .ok_or_else(|| CodegenIrError::missing_entry("value", iterator.as_raw()))?;
    let ValueDef::Instruction { inst: iter_start, .. } = value.def else {
        return Err(CodegenIrError::invalid_module(format!(
            "{} operand is not an iterator value",
            inst.op.name()
        )));
    };
    let iter_start = ctx
        .function
        .instruction(iter_start)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", iter_start.as_raw()))?;
    if iter_start.op != Op::IterStart {
        return Err(CodegenIrError::invalid_module(format!(
            "{} operand was produced by {} instead of iter_start",
            inst.op.name(),
            iter_start.op.name()
        )));
    }
    Ok(iter_start)
}

/// Classifies iterator sources whose storage layouts are handled here.
fn iterator_source_kind_from_type(
    ctx: &FunctionContext<'_>,
    ty: &PhpType,
    inst: &Instruction,
) -> Result<IteratorSourceKind> {
    match ty.codegen_repr() {
        PhpType::Array(elem) => {
            let elem_repr = elem.codegen_repr();
            // A boxed-Mixed/Union-element indexed array may be runtime-promoted
            // to associative hash storage by `Op::ArraySetMixedKey` (notably a
            // `foreach($src as $k=>$v) $dst[$k]=$v` rebuild that writes string
            // keys), so its iteration must dispatch on the runtime heap kind
            // instead of assuming indexed storage. The dynamic indexed value
            // loader reuses existing Mixed boxes (value_type 7) via incref, so a
            // genuinely indexed Mixed-element array iterates identically to the
            // static indexed path; only runtime-promoted hashes route to hash
            // iteration. Concrete-element indexed arrays (int/string/etc.) can
            // never be promoted by `Op::ArraySetMixedKey`, so they keep the
            // faster static indexed path with no extra heap-kind check.
            if matches!(elem_repr, PhpType::Mixed | PhpType::Union(_)) {
                Ok(IteratorSourceKind::DynamicIterable)
            } else {
                Ok(IteratorSourceKind::Indexed { elem: elem_repr })
            }
        }
        PhpType::AssocArray { .. } => Ok(IteratorSourceKind::Hash),
        PhpType::Iterable => Ok(IteratorSourceKind::DynamicIterable),
        PhpType::Mixed | PhpType::Union(_) => Ok(IteratorSourceKind::DynamicMixed),
        PhpType::Object(class_name) => {
            let source = object_iterator_source(ctx, class_name.trim_start_matches('\\'));
            Ok(source)
        }
        // -- PHP-visible scalars: warn at runtime and iterate zero times, like php-src --
        // The checker has already emitted a compile warning for these (it only lets a
        // PHP-visible scalar through); compiler-internal types below stay a hard error.
        ty @ (PhpType::Int
        | PhpType::Float
        | PhpType::Str
        | PhpType::Bool
        | PhpType::False
        | PhpType::Void
        | PhpType::Resource(_)) => Ok(IteratorSourceKind::NonIterable {
            value_tag: crate::codegen::runtime_value_tag(&ty),
        }),
        other => Err(CodegenIrError::unsupported(format!(
            "{} over PHP type {:?}",
            inst.op.name(),
            other
        ))),
    }
}

/// Returns the effective iterator dispatch target for an object source.
fn object_iterator_source(
    ctx: &FunctionContext<'_>,
    class_name: &str,
) -> IteratorSourceKind {
    if ctx.module.interface_infos.contains_key(class_name) {
        if class_name == "Iterator" || interface_extends_interface(ctx, class_name, "Iterator") {
            return IteratorSourceKind::Interface {
                interface_name: class_name.to_string(),
                aggregate_class_name: None,
            };
        }
        // A Traversable-typed value can be either an Iterator or an
        // IteratorAggregate at runtime. The dynamic iterable path distinguishes
        // those cases before invoking any protocol method.
        return IteratorSourceKind::DynamicIterable;
    }
    if class_implements_interface(ctx, class_name, "Iterator") {
        return IteratorSourceKind::Object {
            class_name: class_name.to_string(),
            aggregate_class_name: None,
        };
    }
    if !class_implements_interface(ctx, class_name, "IteratorAggregate") {
        return IteratorSourceKind::Object {
            class_name: class_name.to_string(),
            aggregate_class_name: None,
        };
    }
    match iterator_method_return_type(ctx, class_name, "getIterator") {
        PhpType::Object(iterator_class) => {
            let iterator_class = iterator_class.trim_start_matches('\\').to_string();
            if ctx.module.interface_infos.contains_key(&iterator_class) {
                IteratorSourceKind::Interface {
                    interface_name: iterator_return_interface_dispatch_name(ctx, &iterator_class),
                    aggregate_class_name: Some(class_name.to_string()),
                }
            } else {
                IteratorSourceKind::Object {
                    class_name: iterator_class,
                    aggregate_class_name: Some(class_name.to_string()),
                }
            }
        }
        _ => IteratorSourceKind::Object {
            class_name: class_name.to_string(),
            aggregate_class_name: None,
        },
    }
}

/// Returns the interface whose method slots should drive an IteratorAggregate result.
fn iterator_return_interface_dispatch_name(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
) -> String {
    if interface_name == "Traversable"
        || (interface_extends_interface(ctx, interface_name, "Traversable")
            && !interface_extends_interface(ctx, interface_name, "Iterator"))
    {
        "Iterator".to_string()
    } else {
        interface_name.to_string()
    }
}

/// Returns the declared return type for a no-arg iterator protocol method.
fn iterator_method_return_type(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    method_name: &str,
) -> PhpType {
    let method_key = php_symbol_key(method_name);
    ctx.module
        .class_infos
        .get(class_name)
        .and_then(|class_info| class_info.methods.get(&method_key))
        .map(|sig| sig.return_type.clone())
        .unwrap_or(PhpType::Mixed)
}

/// Returns true when a class implements or inherits an implementation of an interface.
fn class_implements_interface(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    interface_name: &str,
) -> bool {
    let Some(class_info) = ctx.module.class_infos.get(class_name) else {
        return false;
    };
    class_info.interfaces.iter().any(|implemented| {
        normalized_type_name(implemented) == interface_name
            || interface_extends_interface(ctx, normalized_type_name(implemented), interface_name)
    })
}

/// Returns true when an interface extends the requested ancestor interface.
fn interface_extends_interface(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
    ancestor_name: &str,
) -> bool {
    if interface_name == ancestor_name {
        return true;
    }
    let Some(interface_info) = ctx.module.interface_infos.get(interface_name) else {
        return false;
    };
    interface_info.parents.iter().any(|parent| {
        normalized_type_name(parent) == ancestor_name
            || interface_extends_interface(ctx, normalized_type_name(parent), ancestor_name)
    })
}

/// Returns a class-like name without PHP's optional leading namespace separator.
fn normalized_type_name(name: &str) -> &str {
    name.trim_start_matches('\\')
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit coverage for iterator lowering contracts that reject malformed EIR.
    //!
    //! Called from:
    //! - `cargo test` through the Rust test harness.
    //!
    //! Key details:
    //! - IteratorAggregate results must never be published without an owner.

    use super::*;

    /// Missing owner metadata is a codegen error before a result can be adopted.
    #[test]
    fn aggregate_adoption_requires_an_owner_slot() {
        assert!(require_get_iterator_owner(None).is_err());
        let slot = LocalSlotId::from_raw(7);
        assert_eq!(require_get_iterator_owner(Some(slot)).unwrap(), slot);
    }
}
