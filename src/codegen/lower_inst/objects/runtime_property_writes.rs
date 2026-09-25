//! Purpose:
//! Lowers static and runtime-name property writes across receiver shapes.
//!
//! Called from:
//! - The object lowering facade and sibling object support modules.
//!
//! Key details:
//! - Declared-slot dispatch and Mixed object validation preserve value ownership.
//! - A static-name write through a Mixed receiver dispatches on the runtime class id across
//!   the classes that declare that name, and only falls back to the dynamic-property helper
//!   when no declared slot matches.

use super::*;
use crate::codegen::platform::Platform;
use crate::codegen_support::try_handlers::{
    TRY_HANDLER_DIAG_DEPTH_OFFSET, TRY_HANDLER_JMP_BUF_OFFSET, TRY_HANDLER_SLOT_SIZE,
};

const MAGIC_SET_GUARD_FRAME_BYTES: usize = TRY_HANDLER_SLOT_SIZE + 64;
const MAGIC_SET_GUARD_NODE_OFFSET: usize = 0;
const MAGIC_SET_GUARD_HANDLER_OFFSET: usize = 48;

/// Lowers a declared object property write for statically known object receivers.
pub(in crate::codegen::lower_inst) fn lower_prop_set(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let object = expect_operand(inst, 0)?;
    let value = expect_operand(inst, 1)?;
    if let Some(Immediate::ReflectionPropertyRef { class, property }) = inst.immediate {
        let slot = resolve_physical_property_slot(ctx, object, class, property, inst)?;
        let value_ty = ctx.value_php_type(value)?;
        ensure_property_value_supported(ctx, &slot, value, &value_ty, inst)?;
        let base_reg = abi::symbol_scratch_reg(ctx.emitter);
        ctx.load_value_to_reg(object, base_reg)?;
        return emit_property_store(ctx, value, &slot, base_reg);
    }
    if let Some(Immediate::PropertyRef { class, property }) = inst.immediate {
        let slot = resolve_initializer_property_slot(ctx, object, class, property, inst)?;
        let value_ty = ctx.value_php_type(value)?;
        ensure_property_value_supported(ctx, &slot, value, &value_ty, inst)?;
        let base_reg = abi::symbol_scratch_reg(ctx.emitter);
        ctx.load_value_to_reg(object, base_reg)?;
        initialize_owned_property_reference(ctx, &slot, base_reg);
        return emit_property_store(ctx, value, &slot, base_reg);
    }
    let property = property_name_immediate(ctx, inst)?.to_string();
    if let Some((class_name, true)) = nullable_object_receiver_class(ctx, object)? {
        return lower_nullable_prop_set(ctx, inst, object, value, &class_name, &property);
    }
    if matches!(ctx.value_php_type(object)?.codegen_repr(), PhpType::Mixed) {
        return lower_mixed_named_prop_set(ctx, object, value, &property, inst);
    }
    if object_is_builtin_stdclass(ctx, object)? {
        return lower_stdclass_prop_set(ctx, object, value, &property);
    }
    if let Some(plan) = dynamic_property_runtime_plan_for_object(
        ctx,
        object,
        &property,
        PropertyAccessKind::DirectWrite,
        inst,
    )? {
        return lower_planned_dynamic_prop_set(ctx, object, None, value, &property, &plan, inst);
    }
    let slot = resolve_property_slot(ctx, object, &property, inst)?;
    let value_ty = ctx.value_php_type(value)?;
    ensure_property_value_supported(ctx, &slot, value, &value_ty, inst)?;
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(object, base_reg)?;
    if is_promoted_reference_property_bind(ctx, object, value, &slot)? {
        return emit_reference_property_bind(ctx, value, &slot, base_reg);
    }
    emit_property_store(ctx, value, &slot, base_reg)
}
/// Lowers a dynamic property write (`$object->{$name} = $value`).
pub(in crate::codegen::lower_inst) fn lower_dynamic_prop_set(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let object = expect_operand(inst, 0)?;
    let property_value = expect_operand(inst, 1)?;
    let value = expect_operand(inst, 2)?;
    if let Some(property) = const_string_operand(ctx, property_value)? {
        return lower_const_dynamic_prop_set(
            ctx,
            object,
            property_value,
            value,
            property,
            inst,
        );
    }
    if object_is_builtin_stdclass(ctx, object)? {
        return lower_runtime_stdclass_prop_set(ctx, object, property_value, value, inst);
    }
    match ctx.value_php_type(object)?.codegen_repr() {
        PhpType::Mixed | PhpType::Union(_) => {
            lower_runtime_mixed_prop_set(ctx, object, property_value, value, inst)
        }
        PhpType::Object(class_name) => {
            lower_runtime_object_prop_set(ctx, object, property_value, value, &class_name, inst)
        }
        object_ty => Err(CodegenIrError::unsupported(format!(
            "{} for receiver PHP type {:?}",
            inst.op.name(),
            object_ty
        ))),
    }
}

/// Lowers a dynamic property write when the name expression folded to a string.
pub(super) fn lower_const_dynamic_prop_set(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    property_value: ValueId,
    value: ValueId,
    property: &str,
    inst: &Instruction,
) -> Result<()> {
    if matches!(
        ctx.value_php_type(object)?.codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        return lower_mixed_named_prop_set(ctx, object, value, property, inst);
    }
    if object_is_builtin_stdclass(ctx, object)? {
        return lower_stdclass_prop_set(ctx, object, value, property);
    }
    if let Some(plan) = dynamic_property_runtime_plan_for_object(
        ctx,
        object,
        property,
        PropertyAccessKind::DirectWrite,
        inst,
    )? {
        return lower_planned_dynamic_prop_set(
            ctx,
            object,
            Some(property_value),
            value,
            property,
            &plan,
            inst,
        );
    }
    let slot = resolve_property_slot(ctx, object, property, inst)?;
    let value_ty = ctx.value_php_type(value)?;
    ensure_property_value_supported(ctx, &slot, value, &value_ty, inst)?;
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(object, base_reg)?;
    emit_property_store(ctx, value, &slot, base_reg)
}

/// Lowers a runtime-name write to a statically known stdClass receiver.
pub(super) fn lower_runtime_stdclass_prop_set(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    property_value: ValueId,
    value: ValueId,
    inst: &Instruction,
) -> Result<()> {
    ensure_runtime_dynamic_property_name(ctx, property_value, inst)?;
    let value_ty = ctx.value_php_type(value)?.codegen_repr();
    materialize_dynamic_property_mixed_value(ctx, value, &value_ty)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    ctx.load_string_value_to_regs(property_value, ptr_reg, len_reg)?;
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(object, "x0")?;
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
            abi::emit_pop_reg(ctx.emitter, "x3");
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(object, "rdi")?;
            abi::emit_pop_reg_pair(ctx.emitter, "rsi", "rdx");
            abi::emit_pop_reg(ctx.emitter, "rcx");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_set");
    Ok(())
}

/// Lowers a runtime-name write to a known class by comparing against declared slots.
pub(super) fn lower_runtime_object_prop_set(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    property_value: ValueId,
    value: ValueId,
    class_name: &str,
    inst: &Instruction,
) -> Result<()> {
    ensure_runtime_dynamic_property_name(ctx, property_value, inst)?;
    let write_kind = PropertyAccessKind::RuntimeWrite;
    // EVERY candidate name gets an arm, and every arm dispatches by RUNTIME class. Emitting the
    // static class's answer directly wrote the wrong storage under polymorphism in both
    // directions: a subclass can redeclare a strict ancestor's private name as its own property,
    // and it can WIDEN a `protected` the static class refuses into a `public` slot.
    let property_names = runtime_name_candidate_properties(ctx, class_name)?;
    let match_labels = property_names
        .iter()
        .map(|property| ctx.next_label(&format!("dyn_prop_set_{}", label_fragment(property))))
        .collect::<Vec<_>>();
    let miss_label = ctx.next_label("dyn_prop_set_miss");
    let done_label = ctx.next_label("dyn_prop_set_done");

    let object_reg = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(object, object_reg)?;
    abi::emit_push_reg(ctx.emitter, object_reg);
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    ctx.load_string_value_to_regs(property_value, ptr_reg, len_reg)?;
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);

    for (property, label) in property_names.iter().zip(match_labels.iter()) {
        emit_branch_if_dynamic_name_matches(ctx, property, label);
    }
    abi::emit_jump(ctx.emitter, &miss_label);

    for (property, label) in property_names.iter().zip(match_labels.iter()) {
        ctx.emitter.label(label);
        emit_runtime_name_stacked_write_arm(
            ctx,
            class_name,
            property,
            value,
            &miss_label,
            &done_label,
            write_kind,
            inst,
        )?;
    }

    ctx.emitter.label(&miss_label);
    // A class with per-instance hash storage takes the undeclared name there. Falling straight
    // through instead DROPPED the write, with no diagnostic anywhere. The receiver is still
    // stacked at offset 16 here, so the offset ladder probes it where it lies.
    match dynamic_property_runtime_plan_for_class(
        ctx,
        class_name,
        "",
        write_kind,
        inst,
    )? {
        // The receiver is still stacked at offset 16 here, so the ladder probes it there and each
        // arm releases the site's 32-byte block itself. The arm's class name is the one php
        // reports in its creation notice, not the receiver's static type.
        Some(plan) => emit_property_runtime_dispatch(
            ctx,
            &plan,
            "dyn_prop_set_hash",
            DispatchStackCleanup(32),
            |ctx, class_id, label| {
                emit_branch_if_stacked_object_class_matches(ctx, class_id, 16, label)
            },
            |ctx, arm| match &arm.action {
                PropertyRuntimeAction::DynamicHash { hash_offset, .. } => {
                    emit_dynamic_property_creation_deprecation(
                        ctx,
                        &arm.class_name,
                        *hash_offset,
                        16,
                        0,
                    )?;
                    lower_runtime_allow_dynamic_prop_set(ctx, value, *hash_offset, 16, 0, 32)
                }
                PropertyRuntimeAction::MagicDeferred => {
                    lower_runtime_magic_set(
                        ctx,
                        object,
                        property_value,
                        value,
                        &arm.class_name,
                        dynamic_property_hash_offset_for_class(ctx, &arm.class_name, "")?,
                        None,
                        16,
                        32,
                    )
                }
                // A class in this subtree with no hash cannot hold the name at all. php would
                // store, so the build fails rather than dropping the write in silence.
                _ => Err(dynamic_write_without_storage(&arm.class_name, "{runtime name}")),
            },
        )?,
        None => abi::emit_release_temporary_stack(ctx.emitter, 32),
    }
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Lowers a STATIC-name property write whose receiver is only known to be a boxed Mixed.
///
/// `__rt_mixed_property_set` understands stdClass alone, so routing every Mixed receiver
/// there silently DROPPED writes to declared slots of ordinary classes. PHP has no such
/// hole: `$o = pick(); $o->k = 4;` stores into `k` whatever concrete class `pick()`
/// returned. The name is a compile-time constant here, so the dispatch narrows to the
/// classes that actually declare it and each arm reuses the ordinary declared-slot store.
/// Receivers that match no declared arm keep the previous helper, which still covers
/// stdClass and leaves non-object payloads alone.
pub(super) fn lower_mixed_named_prop_set(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    value: ValueId,
    property: &str,
    inst: &Instruction,
) -> Result<()> {
    let candidates = declared_mixed_named_property_set_candidates(ctx, value, property, inst)?;
    if candidates.is_empty() {
        return lower_mixed_prop_set(ctx, object, value, property);
    }
    let done_label = ctx.next_label("mixed_named_prop_set_done");
    let miss_label = ctx.next_label("mixed_named_prop_set_miss");
    let match_labels = candidates
        .iter()
        .map(|candidate| {
            ctx.next_label(&format!(
                "mixed_named_prop_set_{}_{}",
                candidate.class_id,
                label_fragment(property)
            ))
        })
        .collect::<Vec<_>>();

    ctx.load_value_to_reg(object, abi::int_result_reg(ctx.emitter))?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_branch_if_mixed_unboxed_not_object(ctx, &done_label);
    push_mixed_unboxed_object_payload(ctx);

    for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
        emit_branch_if_stacked_object_class_matches(ctx, candidate.class_id, 0, label);
    }
    abi::emit_jump(ctx.emitter, &miss_label);

    for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
        ctx.emitter.label(label);
        match &candidate.action {
            MixedPropertyWriteAction::Refuse(message) => {
                abi::emit_release_temporary_stack(ctx.emitter, 16);
                super::super::exceptions::emit_error(ctx, message);
            }
            MixedPropertyWriteAction::Slot(slot) => {
                let base_reg = abi::symbol_scratch_reg(ctx.emitter);
                abi::emit_load_temporary_stack_slot(ctx.emitter, base_reg, 0);
                emit_property_store(ctx, value, slot, base_reg)?;
                abi::emit_release_temporary_stack(ctx.emitter, 16);
                abi::emit_jump(ctx.emitter, &done_label);
            }
            // php resolves the name to a DYNAMIC property on this runtime class, so the arm
            // stores into that class's own per-instance hash. The offset is the arm's class's
            // own, which is why no offset ladder is needed here: the class id IS the dispatch.
            MixedPropertyWriteAction::DynamicHash {
                class_name,
                hash_offset,
            } => {
                emit_stacked_named_dynamic_property_creation_deprecation(
                    ctx,
                    class_name,
                    property,
                    *hash_offset,
                    0,
                )?;
                lower_stacked_named_dynamic_prop_set(ctx, value, property, *hash_offset, 0)?;
                abi::emit_release_temporary_stack(ctx.emitter, 16);
                abi::emit_jump(ctx.emitter, &done_label);
            }
            MixedPropertyWriteAction::MagicSetRecursiveRefusal { .. } => {
                return Err(CodegenIrError::invalid_module(
                    "literal Mixed property write retained a runtime-name magic-set refusal",
                ));
            }
        }
    }

    ctx.emitter.label(&miss_label);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    lower_mixed_prop_set(ctx, object, value, property)?;
    ctx.emitter.label(&done_label);
    Ok(())
}

/// One class-id arm of a Mixed-receiver property WRITE dispatch.
pub(super) struct MixedPropertyWriteCandidate {
    /// Runtime class id the arm matches on.
    class_id: u64,
    /// Property name the arm answers for. The runtime-name ladder compares it too.
    property: String,
    /// What the arm does once the runtime class matched.
    action: MixedPropertyWriteAction,
}

/// What one class-id arm of a Mixed-receiver property WRITE does.
enum MixedPropertyWriteAction {
    /// Store into this class's declared slot.
    Slot(PropertySlot),
    /// Raise a catchable `Error` and store nothing, either for PHP visibility or because this
    /// backend cannot safely change a refined untyped slot's physical representation at runtime.
    Refuse(String),
    /// php resolves the name to a DYNAMIC property here, so the arm stores into this class's
    /// per-instance hash instead of into any slot.
    ///
    /// Without this arm the write fell through to the receiver-shaped miss path, whose helper
    /// understands stdClass alone, and a `$mixed->p = v` on a strict ancestor's private name
    /// stored NOTHING with no diagnostic anywhere.
    DynamicHash {
        /// Class whose hash this arm writes, for php's dynamic-creation notice.
        class_name: String,
        /// That class's own `8 + slots * 16` hash offset.
        hash_offset: usize,
    },
    /// Invoke `__set`, but preserve this visibility error if the same receiver/name pair reenters.
    MagicSetRecursiveRefusal {
        /// Runtime class whose magic setter PHP selects.
        class_name: String,
        /// Access error raised when the active pair suppresses recursive dispatch.
        message: String,
    },
}

/// Emits the matched-name arm of a runtime-name write, dispatched by RUNTIME class.
///
/// The site's 32-byte block stays live across the dispatch, so the probes read the receiver where
/// it already lies and each action releases the block exactly once. That is what lets the
/// `DynamicMissing` action hand the name back to the ladder's own miss arm, which still needs the
/// receiver and the runtime key on the stack.
fn emit_runtime_name_stacked_write_arm(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    property: &str,
    value: ValueId,
    miss_label: &str,
    done_label: &str,
    write_kind: PropertyAccessKind,
    inst: &Instruction,
) -> Result<()> {
    let plan = resolve_property_runtime_plan(
        ctx,
        class_name,
        property,
        write_kind,
        inst,
    )?;
    emit_property_runtime_dispatch(
        ctx,
        &plan,
        &format!("dyn_prop_set_{}", label_fragment(property)),
        DispatchStackCleanup(32),
        |ctx, class_id, label| emit_branch_if_stacked_object_class_matches(ctx, class_id, 16, label),
        |ctx, arm| match &arm.action {
            // This runtime class declares the name, so php stores into its own slot.
            PropertyRuntimeAction::Slot(slot) => {
                let value_ty = ctx.value_php_type(value)?;
                if let Some(message) = refined_untyped_mixed_write_refusal(slot, &value_ty) {
                    abi::emit_release_temporary_stack(ctx.emitter, 32);
                    super::super::exceptions::emit_error(ctx, &message);
                    return Ok(());
                }
                ensure_property_value_supported(ctx, slot, value, &value_ty, inst)?;
                let base_reg = abi::symbol_scratch_reg(ctx.emitter);
                abi::emit_load_temporary_stack_slot(ctx.emitter, base_reg, 16);
                emit_property_store(ctx, value, slot, base_reg)?;
                abi::emit_release_temporary_stack(ctx.emitter, 32);
                abi::emit_jump(ctx.emitter, done_label);
                Ok(())
            }
            // The name is a compile-time constant in this arm even though the ladder matched it
            // at run time, so the static-key helpers apply and php's notice names THIS class.
            PropertyRuntimeAction::DynamicHash { hash_offset, .. } => {
                emit_stacked_named_dynamic_property_creation_deprecation(
                    ctx,
                    &arm.class_name,
                    property,
                    *hash_offset,
                    16,
                )?;
                lower_stacked_named_dynamic_prop_set(ctx, value, property, *hash_offset, 16)?;
                abi::emit_release_temporary_stack(ctx.emitter, 32);
                abi::emit_jump(ctx.emitter, done_label);
                Ok(())
            }
            // No hash on this runtime class, so the name has nowhere to go HERE. That is the
            // answer the ladder's receiver-shaped miss arm already gives, and it is where this
            // name went before the arm existed, so it is handed back rather than failing the
            // build: the storage reservation deliberately covers only the strict-ancestor-private
            // shape this phase owns, not every undeclared name in the program.
            PropertyRuntimeAction::DynamicMissing { .. } => {
                abi::emit_jump(ctx.emitter, miss_label);
                Ok(())
            }
            // php answers an accessor on this class, which a runtime name cannot reach yet. The
            // arm must not store into any slot, so it stores nothing at all.
            PropertyRuntimeAction::MagicDeferred => {
                let recursive_refusal =
                    magic_set_recursive_refusal(ctx, &arm.class_name, property);
                lower_runtime_magic_set(
                    ctx,
                    expect_operand(inst, 0)?,
                    expect_operand(inst, 1)?,
                    value,
                    &arm.class_name,
                    dynamic_property_hash_offset_for_class(ctx, &arm.class_name, "")?,
                    recursive_refusal.as_deref(),
                    16,
                    32,
                )?;
                abi::emit_jump(ctx.emitter, done_label);
                Ok(())
            }
            PropertyRuntimeAction::MagicGet => Err(CodegenIrError::invalid_module(
                "runtime property write resolved to a magic getter",
            )),
            PropertyRuntimeAction::Refuse { .. } => Err(CodegenIrError::invalid_module(
                "property dispatch handed a refusal arm to its action emitter",
            )),
        },
    )
}

/// Collects the declared slots named `property` that can accept this value, by class id.
///
/// stdClass is excluded because its properties are dynamic, and the miss path already
/// routes it to the dynamic-property helper. A class whose by-name table does not contain the
/// name from THIS scope is excluded too: php answers it from the dynamic-property hash, which is
/// the receiver-shaped miss path, never this class's private slot.
fn declared_mixed_named_property_set_candidates(
    ctx: &FunctionContext<'_>,
    value: ValueId,
    property: &str,
    inst: &Instruction,
) -> Result<Vec<MixedPropertyWriteCandidate>> {
    let value_ty = ctx.value_php_type(value)?;
    let mut candidates = Vec::new();
    for (class_name, class_info) in &ctx.module.class_infos {
        if crate::types::checker::builtin_stdclass::is_stdclass(class_name) {
            continue;
        }
        // A class that does not DECLARE the name can still keep it in its own per-instance hash.
        // Requiring a declared slot excluded every `#[\AllowDynamicProperties]` user class, and
        // the write then fell through to the miss path, whose helper understands `stdClass`
        // alone, so the store was dropped where php puts it in that class's hash.
        let declares = class_info
            .properties
            .iter()
            .any(|(declared, _)| declared == property);
        if !declares && dynamic_property_hash_offset_for_class(ctx, class_name, property)?.is_none()
        {
            continue;
        }
        let Some(candidate) =
            mixed_property_write_candidate(
                ctx,
                class_name,
                property,
                value,
                &value_ty,
                PropertyAccessKind::DirectWrite,
                inst,
            )?
        else {
            continue;
        };
        candidates.push(candidate);
    }
    candidates.sort_by_key(|candidate| candidate.class_id);
    Ok(candidates)
}

/// Builds one Mixed-receiver write arm for a class, or `None` when the miss path must answer.
///
/// A refusal arm still carries the slot: it is what the ladder compares the runtime class id and
/// the property name against. The store simply never runs, so the value type is not checked for
/// it either, which keeps a refused write from being silently dropped for an unrelated reason.
fn mixed_property_write_candidate(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    property: &str,
    value: ValueId,
    value_ty: &PhpType,
    kind: PropertyAccessKind,
    inst: &Instruction,
) -> Result<Option<MixedPropertyWriteCandidate>> {
    let Some(class_info) = ctx.module.class_infos.get(class_name) else {
        return Ok(None);
    };
    let class_id = class_info.class_id;
    // This arm IS a runtime class, so one per-class answer is exactly what it needs and the
    // ladder never has to dispatch a second time. Sharing the resolver with the typed-receiver
    // plan is what keeps the two from drifting.
    //
    // A resolver failure is PROPAGATED, never swallowed into "no arm". Swallowing it omitted a
    // known runtime class from the ladder and the miss path then dropped the write, which is the
    // one outcome php never has. Failing the build is the fail-closed answer this backend already
    // gives elsewhere for a store it cannot model.
    let action = resolve_property_runtime_action(ctx, class_name, property, kind, inst)?;
    let action = match action {
        PropertyRuntimeAction::Slot(slot) => {
            // FAIL CLOSED, never omit. `crate::ir_lower` boxes the value for a receiver whose
            // class is only known at run time, so this check sees a `Mixed` value and accepts it
            // for every slot the weak-mode guard can model; the guard then produces php's own two
            // answers from the RUNTIME tag, coercing `'5'` into an `int` slot and raising the
            // `TypeError` for `'nope'`. What is left here is a slot shape the backend genuinely
            // cannot model, and the honest answer for that is to fail the build rather than to
            // drop a whole runtime class out of the dispatch and lose the assignment with it.
            // An untyped slot keeps the historical `Void` storage marker until a concrete
            // assignment widens it. A Mixed-receiver ladder is conservative and enumerates
            // classes the receiver may never hold, so rejecting that marker here makes an
            // unrelated runtime class prevent the whole function from compiling. There is no
            // safe fixed representation to emit for this candidate yet. Leave it to the miss
            // route, while typed slots continue to fail closed on unsupported assignments.
            if !slot.is_declared
                && matches!(slot.php_type.codegen_repr(), PhpType::Void | PhpType::Never)
                && matches!(value_ty.codegen_repr(), PhpType::Mixed)
            {
                return Ok(None);
            }
            if let Some(message) = refined_untyped_mixed_write_refusal(&slot, value_ty) {
                MixedPropertyWriteAction::Refuse(message)
            } else {
                ensure_property_value_supported(ctx, &slot, value, value_ty, inst)?;
                MixedPropertyWriteAction::Slot(slot)
            }
        }
        PropertyRuntimeAction::Refuse { message } => MixedPropertyWriteAction::Refuse(message),
        PropertyRuntimeAction::DynamicHash { hash_offset, .. } => {
            MixedPropertyWriteAction::DynamicHash {
                class_name: class_name.to_string(),
                hash_offset,
            }
        }
        // This class has no hash, so it cannot hold the name at all, and php's answer for a
        // receiver of this class is whatever the ladder's shared miss path already gives it.
        // A write that php WOULD store never lands here: the storage reservation covers exactly
        // the classes a reachable mutation can address.
        PropertyRuntimeAction::DynamicMissing { .. } => return Ok(None),
        // php answers an accessor on this class. The write is peeled off upstream for a direct
        // name and deferred for a runtime one; either way this arm must not store into a slot,
        // so the class simply does not contribute one.
        PropertyRuntimeAction::MagicDeferred if kind == PropertyAccessKind::RuntimeWrite => {
            let Some(message) = magic_set_recursive_refusal(ctx, class_name, property) else {
                return Ok(None);
            };
            MixedPropertyWriteAction::MagicSetRecursiveRefusal {
                class_name: class_name.to_string(),
                message,
            }
        }
        PropertyRuntimeAction::MagicGet | PropertyRuntimeAction::MagicDeferred => return Ok(None),
    };
    Ok(Some(MixedPropertyWriteCandidate {
        class_id,
        property: property.to_string(),
        action,
    }))
}

/// Refuses a runtime-shaped write into an untyped slot whose inferred storage is specialized.
///
/// PHP's untyped property accepts the value, but this backend cannot replace a raw array, string
/// or object pointer slot with a boxed Mixed cell without changing every aliasing load and store.
/// Keeping the class arm and raising only after it matches avoids making an unrelated refined
/// property reject compilation, while also avoiding a representation-unsafe store.
fn refined_untyped_mixed_write_refusal(
    slot: &PropertySlot,
    value_ty: &PhpType,
) -> Option<String> {
    if slot.is_declared || !matches!(value_ty.codegen_repr(), PhpType::Mixed) {
        return None;
    }
    if matches!(
        slot.php_type.codegen_repr(),
        PhpType::Mixed | PhpType::Void | PhpType::Never
    ) {
        return None;
    }
    Some(format!(
        "Unsupported dynamic property write: runtime Mixed value cannot be stored safely in the refined untyped property {}::${}",
        slot.class_name, slot.property
    ))
}

/// Branches to `matched_label` when a stacked object payload has the given class id.
pub(super) fn emit_branch_if_stacked_object_class_matches(
    ctx: &mut FunctionContext<'_>,
    class_id: u64,
    object_stack_offset: usize,
    matched_label: &str,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", object_stack_offset);
            ctx.emitter.instruction("ldr x10, [x9]");                           // load the stacked receiver's runtime class id
            abi::emit_load_int_immediate(ctx.emitter, "x11", class_id as i64);
            ctx.emitter.instruction("cmp x10, x11");                            // compare it with this candidate class
            ctx.emitter.instruction(&format!("b.eq {}", matched_label));        // take the candidate's declared-slot store
        }
        Arch::X86_64 => {
            // NOT `r12`. That register is callee-saved on this ABI and is the one
            // `abi::nested_call_frame_pointer_reg` reserves, and an ordinary property op does not
            // enable its frame preservation, so materializing a class id into it silently
            // corrupted whatever the frame had parked there. `tertiary_scratch_reg` is `rcx`
            // here, caller-saved, and it aliases neither the stacked receiver in `r11` nor the
            // class-id word in `r10`.
            let candidate_reg = abi::tertiary_scratch_reg(ctx.emitter);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r11", object_stack_offset);
            ctx.emitter.instruction("mov r10, QWORD PTR [r11]");                // load the stacked receiver's runtime class id
            abi::emit_load_int_immediate(ctx.emitter, candidate_reg, class_id as i64);
            ctx.emitter
                .instruction(&format!("cmp r10, {}", candidate_reg));           // compare it with this candidate class
            ctx.emitter.instruction(&format!("je {}", matched_label));          // take the candidate's declared-slot store
        }
    }
}

/// One runtime class whose missing property names are handled by `__set`.
struct RuntimeMagicSetArm {
    class_id: u64,
    class_name: String,
    hash_offset: Option<usize>,
    target: super::super::MethodCallTarget,
}

/// Collects class-id arms for runtime-name writes through a Mixed or union receiver.
fn runtime_magic_set_arms(ctx: &FunctionContext<'_>) -> Result<Vec<RuntimeMagicSetArm>> {
    let mut arms = Vec::new();
    for (class_name, class_info) in &ctx.module.class_infos {
        if !class_info.methods.contains_key(&php_symbol_key("__set")) {
            continue;
        }
        let hash_offset = dynamic_property_hash_offset_for_class(ctx, class_name, "")?;
        arms.push(RuntimeMagicSetArm {
            class_id: class_info.class_id,
            class_name: class_name.clone(),
            hash_offset,
            target: resolve_method_call_target(ctx, class_name, "__set", 3)?,
        });
    }
    arms.sort_by_key(|arm| arm.class_id);
    Ok(arms)
}

/// Resolves and calls one runtime class's magic setter from a staged property-write frame.
fn lower_runtime_magic_set(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    property_value: ValueId,
    value: ValueId,
    class_name: &str,
    hash_offset: Option<usize>,
    recursive_refusal: Option<&str>,
    receiver_stack_offset: usize,
    staged_stack_bytes: usize,
) -> Result<()> {
    let target = resolve_method_call_target(ctx, class_name, "__set", 3)?;
    emit_runtime_magic_set_call(
        ctx,
        object,
        property_value,
        value,
        class_name,
        hash_offset,
        &target,
        recursive_refusal,
        receiver_stack_offset,
        staged_stack_bytes,
    )
}

/// Stages a literal property name and invokes the shared receiver/name guarded magic setter.
pub(super) fn lower_direct_magic_set(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    property_value: ValueId,
    value: ValueId,
    class_name: &str,
    hash_offset: Option<usize>,
    recursive_refusal: Option<&str>,
) -> Result<()> {
    let object_reg = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(object, object_reg)?;
    abi::emit_push_reg(ctx.emitter, object_reg);
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    ctx.load_string_value_to_regs(property_value, ptr_reg, len_reg)?;
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);
    lower_runtime_magic_set(
        ctx,
        object,
        property_value,
        value,
        class_name,
        hash_offset,
        recursive_refusal,
        16,
        32,
    )
}

/// Invokes `__set` unless this receiver/name pair is already active.
///
/// PHP's recursion guard is dynamic and pair-specific. A different name written by the same
/// setter invokes `__set` again, while the same name written through any helper stores directly
/// in the receiver's dynamic hash. A local exception boundary guarantees that the stack-owned
/// guard node is unlinked before an escaping Throwable resumes the surrounding PHP handler.
#[allow(clippy::too_many_arguments)]
fn emit_runtime_magic_set_call(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    property_value: ValueId,
    value: ValueId,
    class_name: &str,
    hash_offset: Option<usize>,
    target: &super::super::MethodCallTarget,
    recursive_refusal: Option<&str>,
    receiver_stack_offset: usize,
    staged_stack_bytes: usize,
) -> Result<()> {
    let existing_label = ctx.next_label("runtime_magic_set_existing");
    let recursive_label = ctx.next_label("runtime_magic_set_recursive");
    let caught_label = ctx.next_label("runtime_magic_set_caught");
    let done_label = ctx.next_label("runtime_magic_set_done");

    if recursive_refusal.is_none() {
        if let Some(hash_offset) = hash_offset {
            emit_branch_if_stacked_runtime_hash_contains(
                ctx,
                hash_offset,
                receiver_stack_offset,
                0,
                &existing_label,
            );
        }
    }

    abi::emit_reserve_temporary_stack(ctx.emitter, MAGIC_SET_GUARD_FRAME_BYTES);
    let guarded_receiver_offset = receiver_stack_offset + MAGIC_SET_GUARD_FRAME_BYTES;
    let guarded_name_offset = MAGIC_SET_GUARD_FRAME_BYTES;
    emit_magic_set_guard_push(
        ctx,
        guarded_receiver_offset,
        guarded_name_offset,
        MAGIC_SET_GUARD_NODE_OFFSET,
    );
    abi::emit_branch_if_int_result_zero(ctx.emitter, &recursive_label);

    emit_magic_set_exception_handler(ctx, &caught_label);
    emit_runtime_magic_set_method_call(
        ctx,
        object,
        property_value,
        value,
        class_name,
        target,
        guarded_receiver_offset,
    )?;
    emit_magic_set_exception_handler_pop(ctx);
    emit_magic_set_guard_pop(ctx, MAGIC_SET_GUARD_NODE_OFFSET);
    abi::emit_release_temporary_stack(
        ctx.emitter,
        MAGIC_SET_GUARD_FRAME_BYTES + staged_stack_bytes,
    );
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&caught_label);
    emit_magic_set_exception_handler_pop(ctx);
    emit_magic_set_guard_pop(ctx, MAGIC_SET_GUARD_NODE_OFFSET);
    abi::emit_release_temporary_stack(
        ctx.emitter,
        MAGIC_SET_GUARD_FRAME_BYTES + staged_stack_bytes,
    );
    abi::emit_jump(ctx.emitter, "__rt_throw_current");

    ctx.emitter.label(&recursive_label);
    abi::emit_release_temporary_stack(ctx.emitter, MAGIC_SET_GUARD_FRAME_BYTES);
    if let Some(message) = recursive_refusal {
        abi::emit_release_temporary_stack(ctx.emitter, staged_stack_bytes);
        super::super::exceptions::emit_error(ctx, message);
    } else {
        if let Some(hash_offset) = hash_offset {
            emit_dynamic_property_creation_deprecation(
                ctx,
                class_name,
                hash_offset,
                receiver_stack_offset,
                0,
            )?;
            lower_runtime_allow_dynamic_prop_set(
                ctx,
                value,
                hash_offset,
                receiver_stack_offset,
                0,
                staged_stack_bytes,
            )?;
            abi::emit_jump(ctx.emitter, &done_label);
        } else {
            emit_readonly_runtime_dynamic_property_error(ctx, class_name, 0, staged_stack_bytes);
        }

        ctx.emitter.label(&existing_label);
        if let Some(hash_offset) = hash_offset {
            lower_runtime_allow_dynamic_prop_set(
                ctx,
                value,
                hash_offset,
                receiver_stack_offset,
                0,
                staged_stack_bytes,
            )?;
        }
    }
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Throws the readonly dynamic-property error with the active runtime name appended.
fn emit_readonly_runtime_dynamic_property_error(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    name_stack_offset: usize,
    staged_stack_bytes: usize,
) {
    let prefix = format!(
        "Cannot create dynamic property {}::$",
        class_name.trim_start_matches('\\')
    );
    let (prefix_label, prefix_len) = ctx.data.add_string(prefix.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x1", &prefix_label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", prefix_len as i64);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x3", name_stack_offset);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x4", name_stack_offset + 8);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rax", &prefix_label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", prefix_len as i64);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", name_stack_offset);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", name_stack_offset + 8);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_concat");
    abi::emit_release_temporary_stack(ctx.emitter, staged_stack_bytes);
    super::super::exceptions::emit_error_from_string_result(ctx);
}

/// Materializes the receiver, runtime property name, and value for one selected `__set` call.
#[allow(clippy::too_many_arguments)]
fn emit_runtime_magic_set_method_call(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    property_value: ValueId,
    value: ValueId,
    class_name: &str,
    target: &super::super::MethodCallTarget,
    receiver_stack_offset: usize,
) -> Result<()> {
    let receiver_reg = abi::nested_call_reg(ctx.emitter).to_string();
    abi::emit_load_temporary_stack_slot(ctx.emitter, &receiver_reg, receiver_stack_offset);

    let receiver_ty = PhpType::Object(class_name.to_string());
    let mut param_types = Vec::with_capacity(target.params.len() + 1);
    param_types.push(receiver_ty.clone());
    param_types.extend(target.params.iter().map(PhpType::codegen_repr));
    let mut ref_params = Vec::with_capacity(target.ref_params.len() + 1);
    ref_params.push(false);
    ref_params.extend(target.ref_params.iter().copied());
    let operands = [object, property_value, value];
    let call_args = materialize_method_call_args_with_receiver_reg_and_refs(
        ctx,
        &receiver_reg,
        &receiver_ty,
        &operands,
        &param_types,
        &ref_params,
        RefArgCellLifetime::CallOnly,
    )?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    emit_resolved_method_call(ctx, target)?;
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    emit_call_arg_temp_cleanups(ctx, &call_args, None)?;
    emit_ref_arg_writebacks(ctx, &call_args)
}

/// Branches when the selected class's dynamic hash already contains the runtime key.
fn emit_branch_if_stacked_runtime_hash_contains(
    ctx: &mut FunctionContext<'_>,
    hash_offset: usize,
    receiver_offset: usize,
    name_offset: usize,
    found_label: &str,
) {
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, object_reg, receiver_offset);
    abi::emit_load_from_address(
        ctx.emitter,
        abi::runtime_helper_int_arg_reg(ctx.emitter, 0),
        object_reg,
        hash_offset,
    );
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::runtime_helper_int_arg_reg(ctx.emitter, 1),
        name_offset,
    );
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::runtime_helper_int_arg_reg(ctx.emitter, 2),
        name_offset + 8,
    );
    abi::emit_call_label(ctx.emitter, "__rt_hash_get");
    emit_branch_if_hash_entry_found(ctx, found_label);
}

/// Adds the caller-owned stack node unless the active chain already contains this pair.
fn emit_magic_set_guard_push(
    ctx: &mut FunctionContext<'_>,
    receiver_offset: usize,
    name_offset: usize,
    node_offset: usize,
) {
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::runtime_helper_int_arg_reg(ctx.emitter, 0),
        receiver_offset,
    );
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::runtime_helper_int_arg_reg(ctx.emitter, 1),
        name_offset,
    );
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::runtime_helper_int_arg_reg(ctx.emitter, 2),
        name_offset + 8,
    );
    abi::emit_temporary_stack_address(
        ctx.emitter,
        abi::runtime_helper_int_arg_reg(ctx.emitter, 3),
        node_offset,
    );
    abi::emit_call_label(ctx.emitter, "__rt_magic_set_guard_push");
}

/// Removes the exact stack node this invocation published.
fn emit_magic_set_guard_pop(ctx: &mut FunctionContext<'_>, node_offset: usize) {
    abi::emit_temporary_stack_address(
        ctx.emitter,
        abi::runtime_helper_int_arg_reg(ctx.emitter, 0),
        node_offset,
    );
    abi::emit_call_label(ctx.emitter, "__rt_magic_set_guard_pop");
}

/// Installs a local handler so an escaping `__set` cannot strand a stack guard node.
fn emit_magic_set_exception_handler(ctx: &mut FunctionContext<'_>, caught_label: &str) {
    let scratch = abi::temp_int_reg(ctx.emitter.target);
    for (symbol, offset) in [
        ("_exc_handler_top", MAGIC_SET_GUARD_HANDLER_OFFSET),
        ("_exc_call_frame_top", MAGIC_SET_GUARD_HANDLER_OFFSET + 8),
        (
            "_rt_diag_suppression",
            MAGIC_SET_GUARD_HANDLER_OFFSET + TRY_HANDLER_DIAG_DEPTH_OFFSET,
        ),
    ] {
        abi::emit_load_symbol_to_reg(ctx.emitter, scratch, symbol, 0);
        abi::emit_store_to_sp(ctx.emitter, scratch, offset);
    }
    abi::emit_temporary_stack_address(ctx.emitter, scratch, MAGIC_SET_GUARD_HANDLER_OFFSET);
    abi::emit_store_reg_to_symbol(ctx.emitter, scratch, "_exc_handler_top", 0);
    abi::emit_temporary_stack_address(
        ctx.emitter,
        abi::runtime_helper_int_arg_reg(ctx.emitter, 0),
        MAGIC_SET_GUARD_HANDLER_OFFSET + TRY_HANDLER_JMP_BUF_OFFSET,
    );
    ctx.emitter.bl_c("setjmp");
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, caught_label);
}

/// Restores the handler and diagnostic state saved by the local `__set` boundary.
fn emit_magic_set_exception_handler_pop(ctx: &mut FunctionContext<'_>) {
    let scratch = abi::temp_int_reg(ctx.emitter.target);
    for (symbol, offset) in [
        ("_exc_handler_top", MAGIC_SET_GUARD_HANDLER_OFFSET),
        (
            "_rt_diag_suppression",
            MAGIC_SET_GUARD_HANDLER_OFFSET + TRY_HANDLER_DIAG_DEPTH_OFFSET,
        ),
    ] {
        abi::emit_load_temporary_stack_slot(ctx.emitter, scratch, offset);
        abi::emit_store_reg_to_symbol(ctx.emitter, scratch, symbol, 0);
    }
}

/// Lowers a runtime-name write when the receiver is a boxed Mixed object.
pub(super) fn lower_runtime_mixed_prop_set(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    property_value: ValueId,
    value: ValueId,
    inst: &Instruction,
) -> Result<()> {
    ensure_runtime_dynamic_property_name(ctx, property_value, inst)?;
    let write_kind = PropertyAccessKind::RuntimeWrite;
    let candidates = declared_mixed_property_set_candidates(ctx, value, write_kind, inst)?;
    let done_label = ctx.next_label("mixed_dyn_prop_set_done");
    let miss_label = ctx.next_label("mixed_dyn_prop_set_miss");
    let stdclass_label = ctx.next_label("mixed_dyn_prop_set_stdclass");
    let match_labels = candidates
        .iter()
        .map(|candidate| {
            ctx.next_label(&format!(
                "mixed_dyn_prop_set_{}",
                label_fragment(&candidate.property)
            ))
        })
        .collect::<Vec<_>>();
    // A runtime name a class does not DECLARE still belongs in that class's own per-instance
    // hash. The declared (class, name) probes above run first, so a name the class declares keeps
    // its declared answer, and only the rest reach this per-CLASS arm.
    let hash_arms = mixed_class_hash_arms(ctx, "", &[])?;
    let hash_labels = hash_arms
        .iter()
        .map(|arm| {
            ctx.next_label(&format!(
                "mixed_dyn_prop_set_hash_{}",
                label_fragment(&arm.class_name)
            ))
        })
        .collect::<Vec<_>>();
    let magic_arms = runtime_magic_set_arms(ctx)?;
    let magic_labels = magic_arms
        .iter()
        .map(|arm| {
            ctx.next_label(&format!(
                "mixed_dyn_prop_set_magic_{}",
                label_fragment(&arm.class_name)
            ))
        })
        .collect::<Vec<_>>();

    ctx.load_value_to_reg(object, abi::int_result_reg(ctx.emitter))?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_branch_if_mixed_unboxed_not_object(ctx, &done_label);
    push_mixed_unboxed_object_payload(ctx);
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    ctx.load_string_value_to_regs(property_value, ptr_reg, len_reg)?;
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);

    for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
        emit_branch_if_mixed_dynamic_property_candidate_matches(
            ctx,
            candidate.class_id,
            &candidate.property,
            label,
        );
    }
    for (arm, label) in magic_arms.iter().zip(magic_labels.iter()) {
        emit_branch_if_stacked_object_class_matches(ctx, arm.class_id, 16, label);
    }
    for (arm, label) in hash_arms.iter().zip(hash_labels.iter()) {
        emit_branch_if_stacked_object_class_matches(ctx, arm.class_id, 16, label);
    }
    emit_branch_if_stacked_object_is_stdclass(ctx, 16, &stdclass_label);
    abi::emit_jump(ctx.emitter, &miss_label);

    for (arm, label) in magic_arms.iter().zip(magic_labels.iter()) {
        ctx.emitter.label(label);
        emit_runtime_magic_set_call(
            ctx,
            object,
            property_value,
            value,
            &arm.class_name,
            arm.hash_offset,
            &arm.target,
            None,
            16,
            32,
        )?;
        abi::emit_jump(ctx.emitter, &done_label);
    }

    for (arm, label) in hash_arms.iter().zip(hash_labels.iter()) {
        ctx.emitter.label(label);
        // The arm matched this runtime class, so the offset and the class php names in its
        // creation notice are both its own. The store helper takes the runtime key from the
        // stacked block and releases that block itself.
        emit_dynamic_property_creation_deprecation(ctx, &arm.class_name, arm.hash_offset, 16, 0)?;
        lower_runtime_allow_dynamic_prop_set(ctx, value, arm.hash_offset, 16, 0, 32)?;
        abi::emit_jump(ctx.emitter, &done_label);
    }

    for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
        ctx.emitter.label(label);
        match &candidate.action {
            MixedPropertyWriteAction::Refuse(message) => {
                abi::emit_release_temporary_stack(ctx.emitter, 32);
                super::super::exceptions::emit_error(ctx, message);
            }
            MixedPropertyWriteAction::Slot(slot) => {
                let base_reg = abi::symbol_scratch_reg(ctx.emitter);
                abi::emit_load_temporary_stack_slot(ctx.emitter, base_reg, 16);
                emit_property_store(ctx, value, slot, base_reg)?;
                abi::emit_release_temporary_stack(ctx.emitter, 32);
                abi::emit_jump(ctx.emitter, &done_label);
            }
            // php resolves the name to a DYNAMIC property on this runtime class. The arm matched
            // on the class id AND the name, so both the offset and the class php names are this
            // class's own and no second dispatch is needed. Falling through to the miss arm
            // instead dropped the write: that helper understands stdClass alone.
            MixedPropertyWriteAction::DynamicHash {
                class_name,
                hash_offset,
            } => {
                emit_stacked_named_dynamic_property_creation_deprecation(
                    ctx,
                    class_name,
                    &candidate.property,
                    *hash_offset,
                    16,
                )?;
                lower_stacked_named_dynamic_prop_set(
                    ctx,
                    value,
                    &candidate.property,
                    *hash_offset,
                    16,
                )?;
                abi::emit_release_temporary_stack(ctx.emitter, 32);
                abi::emit_jump(ctx.emitter, &done_label);
            }
            MixedPropertyWriteAction::MagicSetRecursiveRefusal {
                class_name,
                message,
            } => {
                lower_runtime_magic_set(
                    ctx,
                    object,
                    property_value,
                    value,
                    class_name,
                    dynamic_property_hash_offset_for_class(ctx, class_name, "")?,
                    Some(message),
                    16,
                    32,
                )?;
                abi::emit_jump(ctx.emitter, &done_label);
            }
        }
    }

    ctx.emitter.label(&stdclass_label);
    let value_ty = ctx.value_php_type(value)?.codegen_repr();
    emit_runtime_stdclass_set_for_stacked_name_after_eval_probe(ctx, value, &value_ty, 16, 0, 32)?;
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&miss_label);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Collects Mixed receiver declared-property candidates that can accept this value.
pub(super) fn declared_mixed_property_set_candidates(
    ctx: &FunctionContext<'_>,
    value: ValueId,
    write_kind: PropertyAccessKind,
    inst: &Instruction,
) -> Result<Vec<MixedPropertyWriteCandidate>> {
    let value_ty = ctx.value_php_type(value)?;
    let mut candidates = Vec::new();
    let mut sorted_classes = ctx.module.class_infos.iter().collect::<Vec<_>>();
    sorted_classes.sort_by_key(|(_, class_info)| class_info.class_id);
    for (class_name, class_info) in sorted_classes {
        if crate::types::checker::builtin_stdclass::is_stdclass(class_name) {
            continue;
        }
        for (property, _) in &class_info.properties {
            let Some(candidate) = mixed_property_write_candidate(
                ctx,
                class_name,
                property,
                value,
                &value_ty,
                write_kind,
                inst,
            )?
            else {
                continue;
            };
            candidates.push(candidate);
        }
    }
    candidates.sort_by(|left, right| {
        left.class_id
            .cmp(&right.class_id)
            .then_with(|| left.property.cmp(&right.property))
    });
    Ok(candidates)
}

/// Branches to `target_label` when the unboxed Mixed result is not an object.
pub(super) fn emit_branch_if_mixed_unboxed_not_object(ctx: &mut FunctionContext<'_>, target_label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #6");                              // check whether the boxed receiver holds an object payload
            ctx.emitter.instruction(&format!("b.ne {}", target_label));         // non-object dynamic property writes are ignored
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 6");                              // check whether the boxed receiver holds an object payload
            ctx.emitter.instruction(&format!("jne {}", target_label));          // non-object dynamic property writes are ignored
        }
    }
}

/// Pushes the object payload returned by `__rt_mixed_unbox` onto the temp stack.
///
/// The payload register is read from the shared unbox contract rather than restated, because
/// it is NOT the first argument register on AArch64 and a local restatement is exactly how
/// that contract drifts.
pub(super) fn push_mixed_unboxed_object_payload(ctx: &mut FunctionContext<'_>) {
    let payload_reg =
        crate::codegen_support::mixed_unbox_payload_reg(ctx.emitter.target);
    abi::emit_push_reg(ctx.emitter, payload_reg);
}

/// Branches when both the stacked object class id and runtime property name match.
pub(super) fn emit_branch_if_mixed_dynamic_property_candidate_matches(
    ctx: &mut FunctionContext<'_>,
    class_id: u64,
    property: &str,
    matched_label: &str,
) {
    let next_label = ctx.next_label("mixed_dyn_prop_set_next");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", 16);
            ctx.emitter.instruction("ldr x10, [x9]");                           // load the candidate receiver class id
            abi::emit_load_int_immediate(ctx.emitter, "x11", class_id as i64);
            ctx.emitter.instruction("cmp x10, x11");                            // compare receiver class id before checking the property name
            ctx.emitter.instruction(&format!("b.ne {}", next_label));           // skip name comparison for unrelated classes
        }
        Arch::X86_64 => {
            // Caller-saved `rcx`, never callee-saved `r12`. See the sibling probe above.
            let candidate_reg = abi::tertiary_scratch_reg(ctx.emitter);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r11", 16);
            ctx.emitter.instruction("mov r10, QWORD PTR [r11]");                // load the candidate receiver class id
            abi::emit_load_int_immediate(ctx.emitter, candidate_reg, class_id as i64);
            ctx.emitter
                .instruction(&format!("cmp r10, {}", candidate_reg));           // compare receiver class id before checking the property name
            ctx.emitter.instruction(&format!("jne {}", next_label));            // skip name comparison for unrelated classes
        }
    }
    emit_branch_if_dynamic_name_matches(ctx, property, matched_label);
    ctx.emitter.label(&next_label);
}

/// Branches when a stacked object payload is a stdClass instance.
pub(super) fn emit_branch_if_stacked_object_is_stdclass(
    ctx: &mut FunctionContext<'_>,
    object_stack_offset: usize,
    matched_label: &str,
) {
    let Some(stdclass_id) = stdclass_class_id(ctx) else {
        return;
    };
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", object_stack_offset);
            ctx.emitter.instruction("ldr x10, [x9]");                           // load the stacked object's class id
            abi::emit_load_int_immediate(ctx.emitter, "x11", stdclass_id as i64);
            ctx.emitter.instruction("cmp x10, x11");                            // check whether the runtime receiver is stdClass
            ctx.emitter.instruction(&format!("b.eq {}", matched_label));        // route stdClass writes through the dynamic-property helper
        }
        Arch::X86_64 => {
            // Caller-saved `rcx`, never callee-saved `r12`. See the sibling probes above.
            let candidate_reg = abi::tertiary_scratch_reg(ctx.emitter);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r11", object_stack_offset);
            ctx.emitter.instruction("mov r10, QWORD PTR [r11]");                // load the stacked object's class id
            abi::emit_load_int_immediate(ctx.emitter, candidate_reg, stdclass_id as i64);
            ctx.emitter
                .instruction(&format!("cmp r10, {}", candidate_reg));           // check whether the runtime receiver is stdClass
            ctx.emitter.instruction(&format!("je {}", matched_label));          // route stdClass writes through the dynamic-property helper
        }
    }
}

/// Calls `__rt_stdclass_get` using a stacked object pointer and runtime name pair.
pub(super) fn emit_runtime_stdclass_get_for_stacked_name(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object_stack_offset: usize,
    name_stack_offset: usize,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", object_stack_offset);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", name_stack_offset);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x2", name_stack_offset + 8);
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", object_stack_offset);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", name_stack_offset);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdx", name_stack_offset + 8);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_get");
    cast_loaded_mixed_pointer_to_result(ctx, &inst.result_php_type.codegen_repr())
}

/// Same write, but an eval-declared receiver is handed to Magician's setter first.
///
/// A boxed receiver reaches the stdClass arm when no declared slot matched, and an eval-declared
/// class instance is backed by exactly that layout: writing into its hash here skipped the eval
/// class's private slots and its `__set` guard. `__elephc_eval_dynamic_object_property_set`
/// answers zero for an ordinary receiver, so the plain `__rt_stdclass_set` path stays the fast
/// one, one when it completed the write, and two with an owned Throwable that is raised here
/// after every temporary block — this probe's, the value's and the enclosing ladder's
/// `enclosing_stack_bytes` — has been released, which is what the unwinder expects.
///
/// Emitted only when the module can run eval at all: an eval-free program links no Magician.
fn emit_runtime_stdclass_set_for_stacked_name_after_eval_probe(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    value_ty: &PhpType,
    object_stack_offset: usize,
    name_stack_offset: usize,
    enclosing_stack_bytes: usize,
) -> Result<()> {
    if !crate::codegen::eval_callable_helpers::module_needs_eval_callable_descriptor_support(
        ctx.module,
    ) {
        return emit_runtime_stdclass_set_for_stacked_name(
            ctx, value, value_ty, object_stack_offset, name_stack_offset,
        );
    }
    let plain = ctx.next_label("eval_prop_set_plain");
    let handled = ctx.next_label("eval_prop_set_handled");
    let raise = ctx.next_label("eval_prop_set_raise");
    let done = ctx.next_label("eval_prop_set_done");
    materialize_dynamic_property_mixed_value(ctx, value, value_ty)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    // Frame from here: [sp] Throwable out slot, [sp+16] value cell, then the caller's block.
    abi::emit_reserve_temporary_stack(ctx.emitter, 16);
    let target = ctx.emitter.target;
    // php decides visibility from the WRITING function's lexical class; a free function has none.
    let scope = ctx.function.lexical_class.clone().unwrap_or_default();
    let (scope_label, scope_len) = ctx.data.add_string(scope.as_bytes());
    let external_overflow_bytes = if (target.platform, target.arch)
        == (Platform::Windows, Arch::X86_64)
    {
        // Magician exports a Rust `extern "C"` function, so its PE call follows the native
        // MSx64 ABI: four positional register slots followed by stack words.  This is unlike
        // `__rt_hash_set`, which is hand-written SysV assembly even on PE targets.
        let result_reg = abi::int_result_reg(ctx.emitter);
        let original_frame_bytes = 32;
        let staged_arg_bytes = 16;
        abi::emit_load_temporary_stack_slot(
            ctx.emitter,
            result_reg,
            object_stack_offset + original_frame_bytes,
        );
        abi::emit_push_result_value(ctx.emitter, &PhpType::Pointer(None));
        abi::emit_load_temporary_stack_slot(
            ctx.emitter,
            result_reg,
            name_stack_offset + original_frame_bytes + staged_arg_bytes,
        );
        abi::emit_push_result_value(ctx.emitter, &PhpType::Pointer(None));
        abi::emit_load_temporary_stack_slot(
            ctx.emitter,
            result_reg,
            name_stack_offset + original_frame_bytes + staged_arg_bytes * 2,
        );
        abi::emit_push_result_value(ctx.emitter, &PhpType::Int);
        // The throwable/value block began at the pre-staging stack pointer, three slots above us.
        abi::emit_temporary_stack_address(ctx.emitter, result_reg, staged_arg_bytes * 3);
        abi::emit_push_result_value(ctx.emitter, &PhpType::Pointer(None));
        abi::emit_symbol_address(ctx.emitter, result_reg, &scope_label);
        abi::emit_push_result_value(ctx.emitter, &PhpType::Pointer(None));
        abi::emit_load_int_immediate(ctx.emitter, result_reg, scope_len as i64);
        abi::emit_push_result_value(ctx.emitter, &PhpType::Int);
        let assignments = abi::build_c_abi_outgoing_arg_assignments_for_target(
            target,
            &[
                PhpType::Pointer(None),
                PhpType::Pointer(None),
                PhpType::Int,
                PhpType::Pointer(None),
                PhpType::Pointer(None),
                PhpType::Int,
            ],
        );
        abi::materialize_outgoing_c_abi_args(ctx.emitter, &assignments)
    } else {
        let (arg0, arg1, arg2, arg3, arg4, arg5) = (
            abi::int_arg_reg_name(target, 0),
            abi::int_arg_reg_name(target, 1),
            abi::int_arg_reg_name(target, 2),
            abi::int_arg_reg_name(target, 3),
            abi::int_arg_reg_name(target, 4),
            abi::int_arg_reg_name(target, 5),
        );
        abi::emit_load_temporary_stack_slot(ctx.emitter, arg0, object_stack_offset + 32);
        abi::emit_load_temporary_stack_slot(ctx.emitter, arg1, name_stack_offset + 32);
        abi::emit_load_temporary_stack_slot(ctx.emitter, arg2, name_stack_offset + 40);
        // One block carries both: word 0 is the Throwable slot, word 2 the value cell pushed above.
        abi::emit_temporary_stack_address(ctx.emitter, arg3, 0);
        abi::emit_symbol_address(ctx.emitter, arg4, &scope_label);
        abi::emit_load_int_immediate(ctx.emitter, arg5, scope_len as i64);
        0
    };
    let call_pad_bytes = abi::outgoing_call_stack_pad_bytes(target, external_overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, call_pad_bytes);
    let symbol = target.extern_symbol("__elephc_eval_dynamic_object_property_set");
    abi::emit_call_label(ctx.emitter, &symbol);
    abi::emit_release_temporary_stack(ctx.emitter, call_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, external_overflow_bytes);
    abi::emit_branch_if_int_result_zero(ctx.emitter, &plain);
    let result_reg = abi::int_result_reg(ctx.emitter);
    match target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {result_reg}, #2"));          // did Magician hand back a Throwable instead of writing?
            ctx.emitter.instruction(&format!("b.eq {raise}"));                  // raise it after unwinding every temporary block
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {result_reg}, 2"));           // did Magician hand back a Throwable instead of writing?
            ctx.emitter.instruction(&format!("je {raise}"));                    // raise it after unwinding every temporary block
        }
    }
    ctx.emitter.label(&handled);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, 16);
    abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&raise);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, 16);
    abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, 0);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match target.arch {
        Arch::AArch64 => abi::emit_store_reg_to_symbol(ctx.emitter, "x1", "_exc_value", 0),
        Arch::X86_64 => abi::emit_store_reg_to_symbol(ctx.emitter, "rdi", "_exc_value", 0),
    }
    abi::emit_release_temporary_stack(ctx.emitter, 32 + enclosing_stack_bytes);
    abi::emit_jump(ctx.emitter, "__rt_throw_current");

    ctx.emitter.label(&plain);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    match target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", object_stack_offset + 16);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", name_stack_offset + 16);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x2", name_stack_offset + 24);
            abi::emit_pop_reg(ctx.emitter, "x3");
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", object_stack_offset + 16);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", name_stack_offset + 16);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdx", name_stack_offset + 24);
            abi::emit_pop_reg(ctx.emitter, "rcx");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_set");
    ctx.emitter.label(&done);
    Ok(())
}

/// Calls `__rt_stdclass_set` using a stacked object pointer and runtime name pair.
pub(super) fn emit_runtime_stdclass_set_for_stacked_name(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    value_ty: &PhpType,
    object_stack_offset: usize,
    name_stack_offset: usize,
) -> Result<()> {
    materialize_dynamic_property_mixed_value(ctx, value, value_ty)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", object_stack_offset + 16);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", name_stack_offset + 16);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x2", name_stack_offset + 24);
            abi::emit_pop_reg(ctx.emitter, "x3");
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", object_stack_offset + 16);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", name_stack_offset + 16);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdx", name_stack_offset + 24);
            abi::emit_pop_reg(ctx.emitter, "rcx");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_set");
    Ok(())
}

/// Lowers `unset($object->property)` for a declared, accessible instance property.
///
/// PHP removes the property from the instance; a *typed* property becomes
/// "uninitialized" again. elephc renders declared properties from a fixed per-class
/// descriptor and cannot drop a slot, so the slot is stamped with the shared
/// uninitialized-typed-property marker — exactly the state a typed property without
/// a default starts in. `isset()` then answers false, `print_r`/`var_export` skip the
/// property, and a later read raises the "must not be accessed before initialization"
/// diagnostic. The slot is marked removed before any refcounted payload is released, so a
/// reentrant destructor observes the completed removal and may safely recreate the property.
///
/// A property that lives in the receiver's DYNAMIC-property hash instead of a fixed
/// slot — every `stdClass` property, and an undeclared name on an
/// `#[AllowDynamicProperties]` class — is genuinely removable, so it takes the hash
/// removal path and matches PHP exactly: the key disappears, `isset()` answers false,
/// the value renderers stop listing it, and a later write re-appends it.
///
/// Every other slot shape is REFUSED rather than silently skipped. A by-reference
/// property slot holds an object-owned ref-cell pointer that the destructor still has
/// to free and that a later write would write THROUGH — reviving the alias PHP's
/// `unset()` just broke — so neither zeroing nor keeping the cell reproduces PHP.
/// A packed field and a refined untyped slot that type checking did not widen have no removable
/// storage. Skipping them quietly leaves `isset()` answering `true` after an `unset()`, so they
/// name themselves instead.
pub(in crate::codegen::lower_inst) fn lower_prop_unset(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let object = expect_operand(inst, 0)?;
    if let Some(Immediate::PropertyRef { class, property }) = inst.immediate {
        let slot = resolve_initializer_property_slot(ctx, object, class, property, inst)?;
        let base_reg = abi::symbol_scratch_reg(ctx.emitter);
        ctx.load_value_to_reg(object, base_reg)?;
        // Match direct allocation's object-owned reference cells before exposing the object.
        if !initialize_owned_property_reference(ctx, &slot, base_reg) {
            if !slot.is_declared && !slot_supports_untyped_unset_marker(&slot) {
                return Err(CodegenIrError::invalid_module(
                    "uninitialized marker on an unsupported untyped property slot",
                ));
            }
            emit_property_uninitialized_marker(ctx, &slot, base_reg);
        }
        return Ok(());
    }
    let property = property_name_immediate(ctx, inst)?.to_string();
    lower_named_prop_unset(ctx, object, &property, inst)
}

/// Removes ONE named property from a receiver, shared by the direct-name `unset()` and by the
/// runtime-name form whose name folded to a literal.
///
/// Routing the folded runtime name here rather than duplicating the ladder is what keeps
/// `unset($o->p)` and `unset($o->{"p"})` from ever disagreeing.
pub(super) fn lower_named_prop_unset(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    property: &str,
    inst: &Instruction,
) -> Result<()> {
    if let Some(plan) = dynamic_property_runtime_plan_for_object(
        ctx,
        object,
        property,
        PropertyAccessKind::DirectUnset,
        inst,
    )? {
        return emit_object_property_runtime_dispatch(
            ctx,
            object,
            &plan,
            "prop_unset_dynamic",
            DispatchStackCleanup::NONE,
            |ctx, arm| emit_dynamic_plan_unset(ctx, object, property, arm),
        );
    }
    // A name php resolves to a DYNAMIC property has no physical slot to clear, and the slot the
    // by-name table finds for it belongs to a strict ancestor's private storage. php's answer
    // when no dynamic entry was ever created is a plain no-op, so that is what this emits rather
    // than marking the ancestor's slot uninitialized.
    if scope_dynamic_property_class_for_object(ctx, object, property)?.is_some() {
        return Ok(());
    }
    let slot = resolve_property_slot(ctx, object, property, inst)?;
    if let Some(reason) = unset_unsupported_slot_reason(&slot) {
        return Err(CodegenIrError::unsupported(format!(
            "unset() of {} {}::${}",
            reason, slot.class_name, slot.property
        )));
    }
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(object, base_reg)?;
    detach_and_release_unset_property_value(ctx, &slot, base_reg);
    Ok(())
}

/// Emits one runtime-class arm of an `unset()` whose name php answers dynamically on the static
/// class.
///
/// php's three answers are all represented: a class that DECLARES the name clears its own slot, a
/// class that keeps it in its per-instance hash removes the key, and a class with no hash has
/// nothing to remove, which is php's no-op for a dynamic property that was never created. The
/// ancestor's private slot is never touched on any of them, because no arm addresses it.
pub(super) fn emit_dynamic_plan_unset(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    property: &str,
    arm: &PropertyRuntimeArm,
) -> Result<()> {
    match &arm.action {
        PropertyRuntimeAction::Slot(slot) => {
            if let Some(reason) = unset_unsupported_slot_reason(slot) {
                return Err(CodegenIrError::unsupported(format!(
                    "unset() of {} {}::${}",
                    reason, slot.class_name, slot.property
                )));
            }
            let base_reg = abi::symbol_scratch_reg(ctx.emitter);
            ctx.load_value_to_reg(object, base_reg)?;
            detach_and_release_unset_property_value(ctx, slot, base_reg);
            Ok(())
        }
        PropertyRuntimeAction::DynamicHash { hash_offset, .. } => {
            lower_dynamic_prop_unset(ctx, object, property, *hash_offset)
        }
        // php's `unset()` of a dynamic property that was never created is a no-op, and a class
        // with no hash can never have created one.
        PropertyRuntimeAction::DynamicMissing { .. } => Ok(()),
        // A DIRECT name never reaches this arm: `crate::ir_lower::expr::unset` peels a runtime
        // class declaring `__unset` off and calls the accessor there. `MagicGet` is a read-only
        // answer and cannot be built for an unset, but it removes nothing either way.
        PropertyRuntimeAction::MagicDeferred | PropertyRuntimeAction::MagicGet => Ok(()),
        PropertyRuntimeAction::Refuse { .. } => Err(CodegenIrError::invalid_module(
            "property dispatch handed a refusal arm to its action emitter",
        )),
    }
}

/// Lowers `unset($object->{$name})` for a property NAME only known at run time.
///
/// A folded literal name is exactly the direct-name removal, so it takes that lowering verbatim
/// and the two can never disagree. Everything else compares the runtime name against the
/// receiver's declared names and then asks php's answer for the matched name on the receiver's
/// RUNTIME class, which is the only authority that knows whether the name is a slot, a hash entry
/// or a refusal on the instance actually in hand.
pub(in crate::codegen::lower_inst) fn lower_dynamic_prop_unset_runtime(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let object = expect_operand(inst, 0)?;
    let property_value = expect_operand(inst, 1)?;
    if let Some(property) = const_string_operand(ctx, property_value)? {
        let property = property.to_string();
        return lower_named_prop_unset(ctx, object, &property, inst);
    }
    match ctx.value_php_type(object)?.codegen_repr() {
        PhpType::Object(class_name) => {
            lower_runtime_object_prop_unset(ctx, object, property_value, &class_name, inst)
        }
        PhpType::Mixed | PhpType::Union(_) => {
            lower_runtime_mixed_prop_unset(ctx, object, property_value, inst)
        }
        object_ty => Err(CodegenIrError::unsupported(format!(
            "{} for receiver PHP type {:?}",
            inst.op.name(),
            object_ty
        ))),
    }
}

/// Lowers a runtime-name removal on a receiver whose class is statically known.
///
/// The frame is the one `lower_runtime_object_prop_set` already uses: the receiver at offset 16,
/// the name pointer at 0 and its length at 8, in a 32-byte block every arm releases exactly once.
fn lower_runtime_object_prop_unset(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    property_value: ValueId,
    class_name: &str,
    inst: &Instruction,
) -> Result<()> {
    ensure_runtime_dynamic_property_name(ctx, property_value, inst)?;
    // Every name the runtime subtree can declare, not just the static class's layout: a subclass
    // that INTRODUCES a property owns a real slot for it, and php clears that slot rather than
    // removing a hash entry that never existed.
    let property_names = runtime_name_candidate_properties(ctx, class_name)?;
    let match_labels = property_names
        .iter()
        .map(|property| ctx.next_label(&format!("dyn_prop_unset_{}", label_fragment(property))))
        .collect::<Vec<_>>();
    let miss_label = ctx.next_label("dyn_prop_unset_miss");
    let done_label = ctx.next_label("dyn_prop_unset_done");

    let object_reg = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(object, object_reg)?;
    abi::emit_push_reg(ctx.emitter, object_reg);
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    ctx.load_string_value_to_regs(property_value, ptr_reg, len_reg)?;
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);

    for (property, label) in property_names.iter().zip(match_labels.iter()) {
        emit_branch_if_dynamic_name_matches(ctx, property, label);
    }
    abi::emit_jump(ctx.emitter, &miss_label);

    for (property, label) in property_names.iter().zip(match_labels.iter()) {
        ctx.emitter.label(label);
        emit_runtime_name_unset_arm(ctx, object, class_name, property, inst)?;
        abi::emit_jump(ctx.emitter, &done_label);
    }

    ctx.emitter.label(&miss_label);
    // A name the class does not declare can only live in its per-instance hash, and a class that
    // reserves none never created it, which is php's no-op.
    emit_runtime_name_hash_unset(ctx, class_name, inst)?;
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits one matched-name arm of a runtime-name removal on a known receiver class.
///
/// The name matched, but the receiver's runtime class is still only bounded by the static one, so
/// the whole ACTION is selected by class id: a subclass that declares the name clears its own
/// slot, one that refuses it raises, and one that keeps it in its hash removes the key there. The
/// 32-byte block is released before any of that, so every arm converges with one stack pointer.
fn emit_runtime_name_unset_arm(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    class_name: &str,
    property: &str,
    inst: &Instruction,
) -> Result<()> {
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    let plan = resolve_property_runtime_plan(
        ctx,
        class_name,
        property,
        PropertyAccessKind::RuntimeUnset,
        inst,
    )?;
    emit_object_property_runtime_dispatch(
        ctx,
        object,
        &plan,
        &format!("dyn_prop_unset_{}", label_fragment(property)),
        DispatchStackCleanup::NONE,
        |ctx, arm| emit_dynamic_plan_unset(ctx, object, property, arm),
    )
}

/// Removes a runtime name from the receiver's per-instance hash, at the RUNTIME class's offset.
///
/// The receiver and the name are still staged in the caller's 32-byte block, which every arm of
/// the dispatch releases exactly once.
fn emit_runtime_name_hash_unset(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    inst: &Instruction,
) -> Result<()> {
    match dynamic_property_runtime_plan_for_class(
        ctx,
        class_name,
        "",
        PropertyAccessKind::RuntimeHashMiss,
        inst,
    )? {
        Some(plan) => emit_property_runtime_dispatch(
            ctx,
            &plan,
            "dyn_prop_unset_hash",
            DispatchStackCleanup(32),
            |ctx, class_id, label| {
                emit_branch_if_stacked_object_class_matches(ctx, class_id, 16, label)
            },
            |ctx, arm| match &arm.action {
                PropertyRuntimeAction::DynamicHash { hash_offset, .. } => {
                    lower_runtime_stacked_prop_unset(ctx, *hash_offset, 16, 0, 32)
                }
                // No hash on this runtime class, so nothing was ever created under that name and
                // php's `unset()` is a no-op.
                _ => {
                    abi::emit_release_temporary_stack(ctx.emitter, 32);
                    Ok(())
                }
            },
        ),
        None => {
            abi::emit_release_temporary_stack(ctx.emitter, 32);
            Ok(())
        }
    }
}

/// Removes a RUNTIME-name key from a stacked receiver's dynamic-property hash.
///
/// The static-name sibling interns the key in the data pool; a name known only at run time takes
/// its pointer/length pair from the caller's temporary block. The table is made unique and
/// published back before the removal, exactly as the static-name form does, so an alias of the
/// old table keeps the entry this instance just dropped. The block is released here.
fn lower_runtime_stacked_prop_unset(
    ctx: &mut FunctionContext<'_>,
    hash_offset: usize,
    receiver_offset: usize,
    name_offset: usize,
    frame_bytes: usize,
) -> Result<()> {
    let object_reg = abi::symbol_scratch_reg(ctx.emitter).to_string();
    abi::emit_load_temporary_stack_slot(ctx.emitter, &object_reg, receiver_offset);
    abi::emit_load_from_address(
        ctx.emitter,
        abi::runtime_helper_int_arg_reg(ctx.emitter, 0),
        &object_reg,
        hash_offset,
    );
    abi::emit_call_label(ctx.emitter, "__rt_hash_ensure_unique");
    // The split can move the table, so the fresh one is published before anything removes a key.
    abi::emit_load_temporary_stack_slot(ctx.emitter, &object_reg, receiver_offset);
    abi::emit_store_to_address(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        &object_reg,
        hash_offset,
    );
    abi::emit_reg_move(
        ctx.emitter,
        abi::runtime_helper_int_arg_reg(ctx.emitter, 0),
        abi::int_result_reg(ctx.emitter),
    );
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::runtime_helper_int_arg_reg(ctx.emitter, 1), name_offset);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::runtime_helper_int_arg_reg(ctx.emitter, 2),
        name_offset + 8,
    );
    abi::emit_call_label(ctx.emitter, "__rt_hash_unset");
    abi::emit_release_temporary_stack(ctx.emitter, frame_bytes);
    Ok(())
}

/// Lowers a runtime-name removal whose receiver is only known to be a boxed Mixed.
///
/// The ladder is the one the Mixed write already uses: unbox, keep non-objects out, then dispatch
/// on the runtime class id. Each arm is a concrete class, so it resolves its own plan with its own
/// offsets and its own refusals.
fn lower_runtime_mixed_prop_unset(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    property_value: ValueId,
    inst: &Instruction,
) -> Result<()> {
    ensure_runtime_dynamic_property_name(ctx, property_value, inst)?;
    let candidates = declared_mixed_property_unset_candidates(ctx, inst)?;
    let done_label = ctx.next_label("mixed_dyn_prop_unset_done");
    let miss_label = ctx.next_label("mixed_dyn_prop_unset_miss");
    let stdclass_label = ctx.next_label("mixed_dyn_prop_unset_stdclass");
    let match_labels = candidates
        .iter()
        .map(|candidate| {
            ctx.next_label(&format!(
                "mixed_dyn_prop_unset_{}_{}",
                candidate.class_id,
                label_fragment(candidate.property.as_deref().unwrap_or("hash"))
            ))
        })
        .collect::<Vec<_>>();

    ctx.load_value_to_reg(object, abi::int_result_reg(ctx.emitter))?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_branch_if_mixed_unboxed_not_object(ctx, &done_label);
    push_mixed_unboxed_object_payload(ctx);
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    ctx.load_string_value_to_regs(property_value, ptr_reg, len_reg)?;
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);

    // Every DECLARED name is probed before any class-only arm, so a name a class declares takes
    // its own slot answer and only the names it does not declare reach that class's hash.
    for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
        if let Some(property) = &candidate.property {
            emit_branch_if_mixed_dynamic_property_candidate_matches(
                ctx,
                candidate.class_id,
                property,
                label,
            );
        }
    }
    for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
        if candidate.property.is_none() {
            emit_branch_if_stacked_object_class_matches(ctx, candidate.class_id, 16, label);
        }
    }
    emit_branch_if_stacked_object_is_stdclass(ctx, 16, &stdclass_label);
    abi::emit_jump(ctx.emitter, &miss_label);

    for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
        ctx.emitter.label(label);
        emit_mixed_runtime_name_unset_arm(ctx, candidate, &done_label)?;
    }

    ctx.emitter.label(&stdclass_label);
    // stdClass keeps every property in the hash the header points at, which is the layout of a
    // class with no declared slots at all.
    lower_runtime_stacked_prop_unset(ctx, dynamic_property_hash_offset(0), 16, 0, 32)?;
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&miss_label);
    // A runtime class this module never enumerated cannot have a hash this ladder can address,
    // and php's `unset()` of a property that was never created removes nothing.
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits one arm of the boxed `Mixed` runtime-name removal.
///
/// Every arm releases the ladder's 32-byte block exactly once and converges on `done_label`,
/// except a refusal, which raises and never converges.
fn emit_mixed_runtime_name_unset_arm(
    ctx: &mut FunctionContext<'_>,
    candidate: &MixedPropertyUnsetCandidate,
    done_label: &str,
) -> Result<()> {
    match &candidate.action {
        // The arm matched this runtime class, so the slot is that class's own declared storage
        // and this scope resolves the name to it. No ancestor's private slot is reachable here:
        // such a name resolves to `DynamicHash` or `DynamicMissing` instead.
        PropertyRuntimeAction::Slot(slot) => {
            if let Some(reason) = unset_unsupported_slot_reason(slot) {
                return Err(CodegenIrError::unsupported(format!(
                    "unset() of {} {}::${}",
                    reason, slot.class_name, slot.property
                )));
            }
            let base_reg = abi::symbol_scratch_reg(ctx.emitter).to_string();
            abi::emit_load_temporary_stack_slot(ctx.emitter, &base_reg, 16);
            detach_and_release_unset_property_value(ctx, slot, &base_reg);
            abi::emit_release_temporary_stack(ctx.emitter, 32);
            abi::emit_jump(ctx.emitter, done_label);
        }
        // php refuses the access from this scope, so nothing is removed and nothing is read.
        PropertyRuntimeAction::Refuse { message } => {
            let message = message.clone();
            abi::emit_release_temporary_stack(ctx.emitter, 32);
            super::super::exceptions::emit_error(ctx, &message);
        }
        // The removal is the one `lower_runtime_stacked_prop_unset` performs, at THIS class's own
        // hash offset, which is exact because the arm already matched this class id.
        PropertyRuntimeAction::DynamicHash { hash_offset, .. } => {
            lower_runtime_stacked_prop_unset(ctx, *hash_offset, 16, 0, 32)?;
            abi::emit_jump(ctx.emitter, done_label);
        }
        // Nothing can ever have been created under the name on this class, and php's `unset()` of
        // a property that does not exist is a no-op. A runtime name cannot reach `__unset` in
        // this phase either, and php's accessor removes nothing from storage in any case.
        PropertyRuntimeAction::DynamicMissing { .. }
        | PropertyRuntimeAction::MagicDeferred
        | PropertyRuntimeAction::MagicGet => {
            abi::emit_release_temporary_stack(ctx.emitter, 32);
            abi::emit_jump(ctx.emitter, done_label);
        }
    }
    Ok(())
}

/// Detaches one fixed property value before releasing its former owner.
///
/// Destruction can execute arbitrary PHP, including reads, recursive `unset()`, and assignment
/// to this same property. Publishing the removed marker first makes all three operations observe
/// the committed state. No receiver storage is touched after the release callback, so a
/// reentrant assignment remains installed and a thrown destructor cannot expose stale storage.
fn detach_and_release_unset_property_value(
    ctx: &mut FunctionContext<'_>,
    slot: &PropertySlot,
    base_reg: &str,
) {
    let prop_ty = slot.php_type.codegen_repr();
    let releases_value =
        matches!(prop_ty, PhpType::Str | PhpType::Callable) || prop_ty.is_refcounted();
    if !releases_value {
        emit_property_uninitialized_marker(ctx, slot, base_reg);
        return;
    }

    abi::emit_push_reg(ctx.emitter, base_reg);
    abi::emit_load_from_address(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        base_reg,
        slot.offset,
    );
    emit_property_uninitialized_marker(ctx, slot, base_reg);
    match prop_ty {
        PhpType::Str => abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe"),
        PhpType::Callable => callable_descriptor::emit_release_current_descriptor(ctx.emitter),
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            abi::emit_call_label(ctx.emitter, "__rt_decref_any");
        }
        ty => abi::emit_decref_if_refcounted(ctx.emitter, &ty),
    }
    abi::emit_pop_reg(ctx.emitter, base_reg);
}

/// One arm of a boxed `Mixed` receiver's runtime-name `unset()`.
struct MixedPropertyUnsetCandidate {
    /// Runtime class id the arm matches on.
    class_id: u64,
    /// The declared name the arm answers for, or `None` for the arm that answers every name the
    /// class does NOT declare from that class's per-instance hash.
    property: Option<String>,
    /// php's answer for that name on that class.
    action: PropertyRuntimeAction,
}

/// Collects the removal arms a boxed `Mixed` receiver can take, by runtime class and name.
///
/// Each arm IS a runtime class, so one per-class answer is exact and no second dispatch is
/// needed, which is the same property the Mixed WRITE ladder relies on. `stdClass` is left out
/// because the ladder probes it separately, with the layout of a class that declares no slots.
fn declared_mixed_property_unset_candidates(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<Vec<MixedPropertyUnsetCandidate>> {
    let mut candidates = Vec::new();
    for (class_name, class_info) in &ctx.module.class_infos {
        if is_builtin_stdclass(class_name) {
            continue;
        }
        for (property, _) in &class_info.properties {
            let Ok(action) = resolve_property_runtime_action(
                ctx,
                class_name,
                property,
                PropertyAccessKind::RuntimeUnset,
                inst,
            ) else {
                continue;
            };
            candidates.push(MixedPropertyUnsetCandidate {
                class_id: class_info.class_id,
                property: Some(property.clone()),
                action,
            });
        }
        // The empty name stands for "a name this class does not declare", which is exactly what
        // the class-only arm answers, and it is the same key the known-receiver miss path asks
        // the offset for.
        if let Some(hash_offset) = dynamic_property_hash_offset_for_class(ctx, class_name, "")? {
            candidates.push(MixedPropertyUnsetCandidate {
                class_id: class_info.class_id,
                property: None,
                action: PropertyRuntimeAction::DynamicHash {
                    hash_offset,
                    warns_on_miss: false,
                },
            });
        }
    }
    candidates.sort_by(|left, right| {
        left.class_id
            .cmp(&right.class_id)
            .then_with(|| left.property.cmp(&right.property))
    });
    Ok(candidates)
}
