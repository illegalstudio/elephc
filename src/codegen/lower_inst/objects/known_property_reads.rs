//! Purpose:
//! Lowers property reads for statically known and nullable object receivers.
//!
//! Called from:
//! - The object lowering facade and sibling object support modules.
//!
//! Key details:
//! - Reference cells, magic getters, stdClass, and dynamic-property routes stay distinct.

use super::*;

/// Wording of the catchable `Error` a runtime-dispatched by-reference return raises when the
/// receiver's actual class stores the property with a representation the declared result cannot
/// read. Reference PHP has no equivalent condition (its references are untyped), so this is a
/// compiler-subset diagnostic and makes no PHP-equivalence claim.
const REFERENCE_RETURN_PAYLOAD_MISMATCH_MESSAGE: &str =
    "Cannot return a reference to a property whose stored representation differs from the \
     declared by-reference result type";

/// Wording of the catchable `Error` a `Mixed`-receiver reference load raises when the receiver is
/// not an object of a class that stores the property in a shared cell.
///
/// The path used to publish a literal zero as the cell pointer instead. A zero cell is a LIVE
/// alias as far as everything downstream is concerned: the next read or write through the alias
/// dereferences it, and an arm that merely omitted an unreachable class fell here too, so the
/// unsafe answer was reachable from more than the non-object case. Raising leaves the caller with
/// no cell at all, which is the only answer that cannot be dereferenced.
const REFERENCE_WITHOUT_CELL_MESSAGE: &str =
    "Cannot take a reference to a property the receiver's runtime class does not store in a \
     shared reference cell";

/// Lowers a declared object property read for statically known object receivers.
pub(in crate::codegen::lower_inst) fn lower_prop_get(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let object = expect_operand(inst, 0)?;
    if let Some(Immediate::ReflectionPropertyRef { class, property }) = inst.immediate {
        let slot = resolve_physical_property_slot(ctx, object, class, property, inst)?;
        let base_reg = abi::symbol_scratch_reg(ctx.emitter);
        ctx.load_value_to_reg(object, base_reg)?;
        let read_done = emit_property_read_state_guard(
            ctx,
            &slot,
            base_reg,
            property_fetch_mode(inst),
            PropertyReadMissingResult::Instruction(inst),
        )?;
        emit_property_load(ctx, &slot, base_reg)?;
        materialize_loaded_property_result(ctx, inst, &slot.php_type)?;
        if let Some(read_done) = read_done {
            ctx.emitter.label(&read_done);
        }
        return store_if_result(ctx, inst);
    }
    let property = property_name_immediate(ctx, inst)?.to_string();
    if matches!(ctx.value_php_type(object)?.codegen_repr(), PhpType::Object(_)) {
        return lower_object_prop_get_with_null_guard(ctx, inst, object, &property);
    }
    lower_prop_get_nonnull(ctx, inst, object, &property)
}

/// Guards statically typed object receivers before selecting declared, dynamic,
/// stdClass, or magic-property lowering.
pub(super) fn lower_object_prop_get_with_null_guard(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    property: &str,
) -> Result<()> {
    let null_label = ctx.next_label("prop_get_null_receiver");
    let done_label = ctx.next_label("prop_get_done");
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(object, base_reg)?;
    let scratch_reg = abi::secondary_scratch_reg(ctx.emitter);
    crate::codegen::sentinels::emit_branch_if_null_container(
        ctx.emitter,
        base_reg,
        scratch_reg,
        &null_label,
    );
    lower_prop_get_nonnull(ctx, inst, object, property)?;
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&null_label);
    if inst.op != Op::NullsafePropGet {
        emit_property_on_null_warning(ctx, property);
    }
    // Property reads keep the legacy zero-float miss shape: their null result is never
    // re-tested for null the way a silent `??` element read is.
    super::super::arrays::emit_array_get_null_fallback(ctx, &inst.result_php_type.codegen_repr(), false);
    store_if_result(ctx, inst)?;

    ctx.emitter.label(&done_label);
    Ok(())
}

/// Selects the property representation after a statically typed object receiver
/// has been proven non-null, or for receiver shapes with their own null handling.
pub(super) fn lower_prop_get_nonnull(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    property: &str,
) -> Result<()> {
    if let Some((class_name, true)) = nullable_object_receiver_class(ctx, object)? {
        return lower_nullable_prop_get_with_warning(ctx, inst, object, &class_name, property);
    }
    if let Some(class_name) = union_object_member_class(ctx, object)? {
        return lower_union_object_prop_get(ctx, inst, object, &class_name, property);
    }
    if matches!(
        ctx.value_php_type(object)?.codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        return lower_mixed_prop_get(ctx, inst, object, property);
    }
    if object_is_builtin_stdclass(ctx, object)? {
        return lower_stdclass_prop_get(ctx, inst, object, property);
    }
    if let Some(class_name) = magic_get_receiver_class(ctx, object, property)? {
        return lower_magic_get_prop(ctx, inst, object, &class_name, property);
    }
    // php does not answer this name from a declared slot on the receiver's STATIC class: it is a
    // strict ancestor's private slot, which php 7.4 made invisible here, or an undeclared name the
    // class keeps in its per-instance hash. Either way the runtime class decides what happens, and
    // it can decide something of a different KIND: a subclass that redeclares the name public
    // answers from its own slot, one that declares `__get` answers the accessor, and each class
    // lays its hash out at its own offset and reports its own name.
    let mode = property_fetch_mode(inst);
    if let Some(plan) = dynamic_property_runtime_plan_for_object(
        ctx,
        object,
        property,
        PropertyAccessKind::DirectRead(mode),
        inst,
    )? {
        return emit_object_property_runtime_dispatch(
            ctx,
            object,
            &plan,
            "prop_get_dynamic",
            DispatchStackCleanup::NONE,
            |ctx, arm| emit_dynamic_plan_read(ctx, inst, object, property, arm, mode),
        );
    }
    let slot = resolve_property_slot(ctx, object, property, inst)?;
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(object, base_reg)?;
    let read_done = emit_property_read_state_guard(
        ctx,
        &slot,
        base_reg,
        property_fetch_mode(inst),
        PropertyReadMissingResult::Instruction(inst),
    )?;
    emit_property_load(ctx, &slot, base_reg)?;
    materialize_loaded_property_result(ctx, inst, &slot.php_type)?;
    if let Some(read_done) = read_done {
        ctx.emitter.label(&read_done);
    }
    store_if_result(ctx, inst)
}

/// Resolves the reference-cell slot a statically typed receiver exposes, or emits php's refusal.
///
/// Both ref-cell loads used to call `resolve_property_slot`, which answers from the class's
/// by-name table and therefore from the PHYSICAL layout. That table still carries a strict
/// ancestor's private slot under its plain name, and it answers for a private or protected slot
/// this scope may not touch at all, so both were routes that handed a caller the ADDRESS of
/// storage php forbids it. `resolve_property_reference_arm` is the scope-aware authority.
///
/// `Ok(None)` means the refusal was emitted and the instruction is complete: the `Error` is
/// raised before any cell pointer exists, so there is nothing to publish as the result and
/// nothing on the temporary stack to release.
fn typed_receiver_reference_slot(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    property: &str,
    inst: &Instruction,
) -> Result<Option<PropertySlot>> {
    let PhpType::Object(class_name) = ctx.value_php_type(object)?.codegen_repr() else {
        return resolve_property_slot(ctx, object, property, inst).map(Some);
    };
    match resolve_property_reference_arm(ctx, &class_name, property, inst)? {
        Some(PropertyNameArm::Slot(slot)) => Ok(Some(slot)),
        Some(PropertyNameArm::Refuse { message, .. }) => {
            super::super::exceptions::emit_error(ctx, &message);
            Ok(None)
        }
        // php binds the reference to a DISTINCT dynamic property here, never to the ancestor's
        // slot. Binding into the per-instance hash is not a capability this backend has yet, for
        // any class, so the honest answer is the same `unsupported` diagnostic `stdClass` already
        // produces rather than a slot this scope may not see.
        Some(PropertyNameArm::ScopeDynamic)
        | Some(PropertyNameArm::MagicDeferred)
        | None => Err(CodegenIrError::unsupported(format!(
            "{} for dynamic or missing property {}::${}",
            inst.op.name(),
            class_name.trim_start_matches('\\'),
            property
        ))),
    }
}

/// Lowers `LoadPropRefCell`: loads the raw ref-cell pointer stored in a reference
/// property's slot without dereferencing it. The result (an integer-sized pointer)
/// is the cell shared by the property; callers alias a local to it (`$x = &$obj->prop`)
/// or return it by reference (`fn &() => $this->prop`).
pub(in crate::codegen::lower_inst) fn lower_load_prop_ref_cell(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let object = expect_operand(inst, 0)?;
    let property = property_name_immediate(ctx, inst)?.to_string();
    if matches!(ctx.value_php_type(object)?.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        return lower_mixed_load_prop_ref_cell(ctx, inst, object, &property);
    }
    let Some(slot) = typed_receiver_reference_slot(ctx, object, &property, inst)? else {
        return Ok(());
    };
    if !slot.is_reference {
        return Err(CodegenIrError::unsupported(format!(
            "load_prop_ref_cell on non-reference property {}::${}",
            slot.class_name, slot.property
        )));
    }
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(object, base_reg)?;
    let int_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_from_address(ctx.emitter, int_reg, base_reg, slot.offset); // load the reference-cell pointer from the property slot (no deref)
    if slot.is_declared {
        emit_uninitialized_owned_ref_property_guard(ctx, &slot, int_reg);
    }
    store_ref_cell_pointer_result(ctx, inst)
}

/// Lowers `LoadPropRefCellChecked`: the by-reference-return form of the load above, which hands
/// the caller a cell it will dereference with the callee's DECLARED result representation.
///
/// The cell pointer itself is one word whatever it aliases, so the danger is not the transfer but
/// the claim that travels with it: a caller told the cell holds a `Mixed` box while the slot
/// actually holds a raw `int` would read that integer as a pointer. The slot's ACTUAL payload
/// type is therefore checked here, before the pointer is published.
///
/// A statically typed receiver has exactly one slot, and `ir_lower::stmt::control_exit` already
/// refuses that disagreement with a source diagnostic; the check below is defence in depth for a
/// receiver type the early diagnostic could not resolve. A `Mixed` receiver is decided PER
/// CANDIDATE CLASS instead: the compatible classes keep working through the same function, and
/// only a receiver whose runtime class stores an incompatible payload raises a catchable `Error`.
pub(in crate::codegen::lower_inst) fn lower_load_prop_ref_cell_checked(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let object = expect_operand(inst, 0)?;
    let property = property_name_immediate(ctx, inst)?.to_string();
    let expected = inst.result_php_type.clone();
    if matches!(ctx.value_php_type(object)?.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        return lower_mixed_load_prop_ref_cell_checked(ctx, inst, object, &property, &expected);
    }
    let Some(slot) = typed_receiver_reference_slot(ctx, object, &property, inst)? else {
        return Ok(());
    };
    if !slot.is_reference {
        return Err(CodegenIrError::unsupported(format!(
            "load_prop_ref_cell_checked on non-reference property {}::${}",
            slot.class_name, slot.property
        )));
    }
    if !slot.php_type.reference_payload_compatible(&expected) {
        return Err(CodegenIrError::unsupported(format!(
            "by-reference return of {}::${} stores {:?}, which cannot be read as the declared \
             result {:?}",
            slot.class_name, slot.property, slot.php_type, expected
        )));
    }
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(object, base_reg)?;
    let int_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_from_address(ctx.emitter, int_reg, base_reg, slot.offset); // load the reference-cell pointer from the property slot (no deref)
    store_ref_cell_pointer_result(ctx, inst)
}

/// Lowers the guarded by-reference-return cell load for a receiver whose class is only known at
/// run time (a closure's `Closure::bind`-supplied `$this`, or a `mixed` parameter).
///
/// The class-id ladder is the same one the unguarded `Mixed` form uses, so every declared
/// reference-property owner still gets its own arm and dispatch stays a compile-time-known
/// comparison chain. What differs is the arm BODY: a class whose slot payload cannot be read as
/// the declared result jumps to one shared throw site instead of loading its cell. That keeps the
/// decision per candidate: a program that calls the same by-reference function with a compatible
/// class and with an incompatible one gets the reference for the first and a catchable `Error`
/// for the second, which a blanket static refusal of `Mixed` receivers could not express.
fn lower_mixed_load_prop_ref_cell_checked(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    property: &str,
    expected: &PhpType,
) -> Result<()> {
    let candidates = declared_mixed_reference_property_candidates(ctx, property, inst)?;
    if candidates.is_empty() {
        return Err(CodegenIrError::unsupported(format!(
            "load_prop_ref_cell_checked on Mixed receiver for property ${} with no \
             reference-property class",
            property
        )));
    }
    let done_label = ctx.next_label("mixed_propref_checked_done");
    let null_label = ctx.next_label("mixed_propref_checked_null");
    let mismatch_label = ctx.next_label("mixed_propref_checked_mismatch");
    let match_labels = candidates
        .iter()
        .map(|candidate| {
            ctx.next_label(&format!(
                "mixed_propref_checked_{}",
                label_fragment(&candidate.candidate.slot.class_name)
            ))
        })
        .collect::<Vec<_>>();

    ctx.load_value_to_reg(object, abi::int_result_reg(ctx.emitter))?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_mixed_object_payload_or_null(ctx, &null_label);
    // stdClass and classes without this reference property have no matching cell.
    let dispatch = mixed_reference_dispatch_candidates(&candidates);
    emit_mixed_property_class_dispatch(
        ctx,
        &dispatch,
        &match_labels,
        &null_label,
        &null_label,
    );

    let int_reg = abi::int_result_reg(ctx.emitter);
    let mut any_mismatch = false;
    for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
        ctx.emitter.label(label);
        // php refuses the access from this scope, so no cell is published at all. The arm exists
        // precisely so this runtime class cannot fall into the shared no-cell path.
        if let Some(message) = &candidate.refusal {
            let message = message.clone();
            super::super::exceptions::emit_error(ctx, &message);
            continue;
        }
        let slot = &candidate.candidate.slot;
        if slot.php_type.reference_payload_compatible(expected) {
            abi::emit_load_from_address(ctx.emitter, int_reg, int_reg, slot.offset); // load the reference-cell pointer from the matched class's property slot
        } else {
            any_mismatch = true;
            abi::emit_jump(ctx.emitter, &mismatch_label);
        }
        abi::emit_jump(ctx.emitter, &done_label);
    }

    if any_mismatch {
        ctx.emitter.label(&mismatch_label);
        // No pointer has been published yet, so the throw leaves the caller with no cell at all
        // rather than with one it would read through the wrong representation.
        super::super::exceptions::emit_error(ctx, REFERENCE_RETURN_PAYLOAD_MISMATCH_MESSAGE);
    }

    ctx.emitter.label(&null_label);
    // NEVER a zero cell pointer: see `REFERENCE_WITHOUT_CELL_MESSAGE`.
    super::super::exceptions::emit_error(ctx, REFERENCE_WITHOUT_CELL_MESSAGE);

    ctx.emitter.label(&done_label);
    store_ref_cell_pointer_result(ctx, inst)
}

/// Projects the reference arms onto the class-id dispatch shape the shared ladder expects.
fn mixed_reference_dispatch_candidates(candidates: &[MixedReferenceCandidate]) -> Vec<u64> {
    candidates
        .iter()
        .map(|candidate| candidate.candidate.class_id)
        .collect()
}

/// Stores the materialized reference-cell pointer (in the integer result register) into the
/// instruction's result value as a single machine word.
///
/// The cell pointer is one pointer-sized word whatever element type it aliases, so it must
/// not go through the type-driven result store (which would split a `Str`/`Float` result and
/// drop the pointer). Shared by both the typed-object and `Mixed`-receiver `LoadPropRefCell`
/// lowerings.
pub(super) fn store_ref_cell_pointer_result(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if let Some(result) = inst.result {
        ctx.store_int_result_value(result)?;
    }
    Ok(())
}

/// Lowers `LoadPropRefCell` when the receiver is a `Mixed` object (e.g. a closure's `$this`
/// bound via `Closure::bind`). Unboxes the receiver, dispatches on its runtime class id, and
/// loads the reference-cell pointer from the matching class's property slot — the raw cell
/// pointer, not its dereferenced value.
pub(super) fn lower_mixed_load_prop_ref_cell(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    property: &str,
) -> Result<()> {
    let candidates = declared_mixed_reference_property_candidates(ctx, property, inst)?;
    if candidates.is_empty() {
        return Err(CodegenIrError::unsupported(format!(
            "load_prop_ref_cell on Mixed receiver for property ${} with no reference-property class",
            property
        )));
    }
    let done_label = ctx.next_label("mixed_propref_done");
    let null_label = ctx.next_label("mixed_propref_null");
    let match_labels = candidates
        .iter()
        .map(|candidate| {
            ctx.next_label(&format!(
                "mixed_propref_{}",
                label_fragment(&candidate.candidate.slot.class_name)
            ))
        })
        .collect::<Vec<_>>();

    ctx.load_value_to_reg(object, abi::int_result_reg(ctx.emitter))?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_mixed_object_payload_or_null(ctx, &null_label);
    // stdClass and classes without this reference property have no matching cell.
    let dispatch = mixed_reference_dispatch_candidates(&candidates);
    emit_mixed_property_class_dispatch(
        ctx,
        &dispatch,
        &match_labels,
        &null_label,
        &null_label,
    );

    let int_reg = abi::int_result_reg(ctx.emitter);
    for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
        ctx.emitter.label(label);
        // php refuses the access from this scope, so no cell is published at all.
        if let Some(message) = &candidate.refusal {
            let message = message.clone();
            super::super::exceptions::emit_error(ctx, &message);
            continue;
        }
        abi::emit_load_from_address(ctx.emitter, int_reg, int_reg, candidate.candidate.slot.offset); // load the reference-cell pointer from the matched class's property slot
        if candidate.candidate.slot.is_declared {
            emit_uninitialized_owned_ref_property_guard(
                ctx,
                &candidate.candidate.slot,
                int_reg,
            );
        }
        abi::emit_jump(ctx.emitter, &done_label);
    }

    ctx.emitter.label(&null_label);
    // NEVER a zero cell pointer: see `REFERENCE_WITHOUT_CELL_MESSAGE`.
    super::super::exceptions::emit_error(ctx, REFERENCE_WITHOUT_CELL_MESSAGE);

    ctx.emitter.label(&done_label);
    store_ref_cell_pointer_result(ctx, inst)
}

/// Lowers a declared object-property initialization probe.
pub(in crate::codegen::lower_inst) fn lower_prop_initialized(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let object = expect_operand(inst, 0)?;
    let property = property_name_immediate(ctx, inst)?.to_string();
    if let Some((class_name, true)) = nullable_object_receiver_class(ctx, object)? {
        return lower_nullable_prop_initialized(ctx, inst, object, &class_name, &property);
    }
    let slot = resolve_property_slot(ctx, object, &property, inst)?;
    if !slot.is_declared && !slot_supports_untyped_unset_marker(&slot) {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
        return store_if_result(ctx, inst);
    }
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(object, base_reg)?;
    emit_typed_property_initialized_bool(ctx, &slot, base_reg);
    store_if_result(ctx, inst)
}

/// Probes a typed property through a NULLABLE (`?C`) receiver.
///
/// Such a receiver represents as a boxed `Mixed`, so the probe above has no object pointer to
/// read the slot from and this instruction used to be refused outright — which made
/// `isset($c->p)` a compile error and left `$c->p ?? "d"` on the ordinary read, where it fatals
/// on an uninitialized slot. The receiver is unboxed here the same way every other nullable
/// receiver is, and a NULL one answers `false`: that is the answer both callers want, since
/// `isset(null->p)` is false and `null->p ?? "d"` is the default.
///
/// The probe and the read that follows it consume the SAME unboxed value, so nothing can
/// re-null the receiver between them — the initialized branch is only ever entered with the
/// object this instruction just proved present.
fn lower_nullable_prop_initialized(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    class_name: &str,
    property: &str,
) -> Result<()> {
    let slot = resolve_property_slot_for_class(ctx, class_name, property, inst)?;
    if !slot.is_declared && !slot_supports_untyped_unset_marker(&slot) {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
        return store_if_result(ctx, inst);
    }
    let null_label = ctx.next_label("prop_initialized_null_receiver");
    let done_label = ctx.next_label("prop_initialized_done");
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    emit_nullable_receiver_object_payload(ctx, object, &null_label, base_reg)?;
    emit_typed_property_initialized_bool(ctx, &slot, base_reg);
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&null_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0); // a null receiver has no slot to be initialized

    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Returns the receiver class when an undeclared property should route through `__get`.
pub(super) fn magic_get_receiver_class(
    ctx: &FunctionContext<'_>,
    object: ValueId,
    property: &str,
) -> Result<Option<String>> {
    let PhpType::Object(class_name) = ctx.value_php_type(object)?.codegen_repr() else {
        return Ok(None);
    };
    let normalized = class_name.trim_start_matches('\\');
    let Some(class_info) = ctx.module.class_infos.get(normalized) else {
        return Ok(None);
    };
    // A slot this SCOPE does not resolve the name to is not a declaration for this decision.
    // php 7.4 removed shadow properties, so a strict ancestor's private name is not in this
    // class's by-name table at all and php answers it from `__get`, measured on php 8.5.10 from
    // the child scope and from global scope alike. This is the read half of the pair
    // `magic_set_receiver_has_method` decides for a write: routing the write to `__set` while the
    // read kept answering null would report a value php never stores.
    if class_info
        .properties
        .iter()
        .any(|(name, _)| name == property)
        && !property_name_is_scope_dynamic(ctx, normalized, property)
    {
        return Ok(None);
    }
    if class_info.methods.contains_key(&php_symbol_key("__get")) {
        return Ok(Some(normalized.to_string()));
    }
    Ok(None)
}

/// Lowers a missing declared-property read by calling the class `__get` method.
pub(super) fn lower_magic_get_prop(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    class_name: &str,
    property: &str,
) -> Result<()> {
    let target = resolve_method_call_target(ctx, class_name, "__get", 2)?;
    if target.ref_params.first().copied().unwrap_or(false) {
        return Err(CodegenIrError::unsupported(format!(
            "magic __get by-reference name parameter on {}",
            class_name
        )));
    }
    emit_magic_get_args(ctx, object, property)?;
    super::super::emit_resolved_method_call(ctx, &target)?;
    store_method_call_result(ctx, inst, &target)
}

/// Loads `$this` and the static property name into ABI registers for `__get`.
pub(super) fn emit_magic_get_args(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    property: &str,
) -> Result<()> {
    let (label, len) = ctx.data.add_string(property.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(object, "x0")?;
            abi::emit_symbol_address(ctx.emitter, "x1", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(object, "rdi")?;
            abi::emit_symbol_address(ctx.emitter, "rsi", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", len as i64);
        }
    }
    Ok(())
}

/// Lowers a named property read from a statically known stdClass receiver.
pub(super) fn lower_stdclass_prop_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    property: &str,
) -> Result<()> {
    emit_stdclass_get_call(ctx, object, property)?;
    cast_loaded_mixed_pointer_to_result(ctx, &inst.result_php_type.codegen_repr())?;
    store_if_result(ctx, inst)
}

/// Calls the stdClass runtime getter for an object receiver and static property name.
pub(super) fn emit_stdclass_get_call(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    property: &str,
) -> Result<()> {
    let (label, len) = ctx.data.add_string(property.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(object, "x0")?;
            abi::emit_symbol_address(ctx.emitter, "x1", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(object, "rdi")?;
            abi::emit_symbol_address(ctx.emitter, "rsi", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", len as i64);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_get");
    Ok(())
}

/// Reads a name php resolves to a DYNAMIC property on a statically known class.
///
/// php 7.4 removed shadow properties: a strict ancestor's private slot lives under a mangled key,
/// so outside the class that declared it the plain name belongs to the per-instance hash and the
/// private slot keeps its own value. A class that reserves no hash can never hold such an entry,
/// so the name is simply php `null`.
///
/// Both the class and the name are compile-time constants here, so a READ's `Undefined property`
/// warning is one static line. A PROBE stays silent: `isset()`, `empty()` and `??` never report
/// it. The result is left in the instruction's result representation for the caller's shared
/// `store_if_result`.
pub(super) fn emit_scope_dynamic_property_read(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    class_name: &str,
    property: &str,
    object_reg: &str,
    mode: PropertyFetchMode,
) -> Result<()> {
    let warn_on_miss = mode.is_read() && property_name_is_scope_dynamic(ctx, class_name, property);
    match dynamic_property_hash_offset_for_class(ctx, class_name, property)? {
        Some(hash_offset) => emit_scope_dynamic_property_hash_probe(
            ctx, class_name, property, object_reg, hash_offset, warn_on_miss,
        )?,
        None => {
            if warn_on_miss {
                emit_undefined_property_warning(ctx, class_name, property);
            }
            emit_boxed_null(ctx);
        }
    }
    cast_loaded_mixed_pointer_to_result(ctx, &inst.result_php_type.codegen_repr())
}

/// The probe half of the read above, leaving the answer as a BOXED Mixed pointer.
///
/// A ladder whose arms converge on one shared `cast_loaded_mixed_pointer_to_result` needs the
/// boxed pointer, not the already-cast result: casting inside the arm and again at the
/// convergence point would read the cast value as a cell pointer.
///
/// `class_name` and `hash_offset` both come from the caller because both have to describe the
/// RUNTIME class. A ladder arm supplies its own; a caller that has already proven the runtime
/// class is the static one supplies that one.
pub(super) fn emit_scope_dynamic_property_hash_probe(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    property: &str,
    object_reg: &str,
    hash_offset: usize,
    warn_on_miss: bool,
) -> Result<()> {
    let target = ctx.emitter.target;
    let (label, key_len) = ctx.data.add_string(property.as_bytes());
    let miss_label = ctx.next_label("scope_dynamic_prop_miss");
    let done_label = ctx.next_label("scope_dynamic_prop_done");
    abi::emit_load_from_address(
        ctx.emitter,
        abi::int_arg_reg_name(target, 0),
        object_reg,
        hash_offset,
    );
    abi::emit_symbol_address(ctx.emitter, abi::int_arg_reg_name(target, 1), &label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(target, 2), key_len as i64);
    abi::emit_call_label(ctx.emitter, "__rt_hash_get");
    abi::emit_branch_if_int_result_zero(ctx.emitter, &miss_label);
    match target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // return the boxed Mixed cell stored in the hash entry
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, rdi");                            // return the boxed Mixed cell stored in the hash entry
        }
    }
    abi::emit_incref_if_refcounted(ctx.emitter, &PhpType::Mixed);
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&miss_label);
    if warn_on_miss {
        emit_undefined_property_warning(ctx, class_name, property);
    }
    emit_boxed_null(ctx);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits one runtime-class arm of a read whose name php answers dynamically on the static class.
///
/// Every arm publishes the instruction's own result representation and stores it, so the arms
/// converge AFTER the store and nothing has to agree on an intermediate register shape. The
/// `Refuse` arm never reaches here: the dispatcher raises it.
pub(super) fn emit_dynamic_plan_read(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    property: &str,
    arm: &PropertyRuntimeArm,
    mode: PropertyFetchMode,
) -> Result<()> {
    match &arm.action {
        // This runtime class DECLARES the name, so php reads its slot and nothing is dynamic
        // about the access at all. Reaching a slot here is the polymorphic case, not an escape:
        // the arm only runs when the receiver's runtime class id matched this class.
        PropertyRuntimeAction::Slot(slot) => {
            let base_reg = abi::symbol_scratch_reg(ctx.emitter);
            ctx.load_value_to_reg(object, base_reg)?;
            let read_done = emit_property_read_state_guard(
                ctx,
                slot,
                base_reg,
                property_fetch_mode(inst),
                PropertyReadMissingResult::Instruction(inst),
            )?;
            emit_property_load(ctx, slot, base_reg)?;
            materialize_loaded_property_result(ctx, inst, &slot.php_type)?;
            if let Some(read_done) = read_done {
                ctx.emitter.label(&read_done);
            }
            store_if_result(ctx, inst)
        }
        PropertyRuntimeAction::DynamicHash {
            hash_offset,
            warns_on_miss,
        } => {
            let base_reg = abi::symbol_scratch_reg(ctx.emitter);
            ctx.load_value_to_reg(object, base_reg)?;
            emit_scope_dynamic_property_hash_probe(
                ctx,
                &arm.class_name,
                property,
                base_reg,
                *hash_offset,
                mode.is_read() && *warns_on_miss,
            )?;
            cast_loaded_mixed_pointer_to_result(ctx, &inst.result_php_type.codegen_repr())?;
            store_if_result(ctx, inst)
        }
        // No hash on this runtime class, so the entry can never exist: php warns on a value read
        // and answers null, and stays silent for a probe.
        PropertyRuntimeAction::DynamicMissing { warns_on_miss } => {
            if mode.is_read() && *warns_on_miss {
                emit_undefined_property_warning(ctx, &arm.class_name, property);
            }
            emit_boxed_null(ctx);
            cast_loaded_mixed_pointer_to_result(ctx, &inst.result_php_type.codegen_repr())?;
            store_if_result(ctx, inst)
        }
        // php calls `__get` on this runtime class and the name is a constant here, so the arm
        // makes the real call. `lower_magic_get_prop` is the same lowering a receiver whose STATIC
        // class declares the accessor already takes, so the two agree by construction.
        PropertyRuntimeAction::MagicGet => {
            lower_magic_get_prop(ctx, inst, object, &arm.class_name, property)
        }
        // A silent probe would consult `__isset`, which has no codegen call site in this phase. It
        // answers php null rather than a slot so that the deferral can only lose a value, never
        // expose storage.
        PropertyRuntimeAction::MagicDeferred => {
            emit_dynamic_property_miss_result(ctx, inst);
            store_if_result(ctx, inst)
        }
        PropertyRuntimeAction::Refuse { .. } => Err(CodegenIrError::invalid_module(
            "property dispatch handed a refusal arm to its action emitter",
        )),
    }
}

/// Reads a RUNTIME-name undeclared property from the receiver's dynamic-property hash.
///
/// The static-name sibling interns the key in the data pool; a name known only at run time takes
/// its pointer/length pair from the caller's temporary stack frame, the same frame the declared
/// slot ladder staged the receiver and the name in. The block is released here.
///
/// Without this the ladder's MISS arm answered PHP `null` for every undeclared runtime name, so
/// `$o->{$name}` could not see what `$o->{"literal"}` had just stored in the very same hash.
///
/// The result is left in the instruction's result representation for the caller's SHARED
/// `store_if_result`, exactly like the declared-slot arms it sits next to.
pub(super) fn lower_runtime_allow_dynamic_prop_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    hash_offset: usize,
    receiver_offset: usize,
    name_offset: usize,
    frame_bytes: usize,
) -> Result<()> {
    let target = ctx.emitter.target;
    let miss_label = ctx.next_label("runtime_dynamic_prop_hash_miss");
    let done_label = ctx.next_label("runtime_dynamic_prop_hash_done");
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, object_reg, receiver_offset);
    abi::emit_load_from_address(
        ctx.emitter,
        abi::int_arg_reg_name(target, 0),
        object_reg,
        hash_offset,
    );
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(target, 1), name_offset);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(target, 2),
        name_offset + 8,
    );
    abi::emit_call_label(ctx.emitter, "__rt_hash_get");
    // Released before the found test so the branch reads freshly computed flags on every target.
    abi::emit_release_temporary_stack(ctx.emitter, frame_bytes);
    abi::emit_branch_if_int_result_zero(ctx.emitter, &miss_label);
    match target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // return the boxed Mixed cell stored in the hash entry
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, rdi");                            // return the boxed Mixed cell stored in the hash entry
        }
    }
    abi::emit_incref_if_refcounted(ctx.emitter, &PhpType::Mixed);
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&miss_label);
    emit_boxed_null(ctx);
    ctx.emitter.label(&done_label);
    cast_loaded_mixed_pointer_to_result(ctx, &inst.result_php_type.codegen_repr())
}
