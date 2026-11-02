//! Purpose:
//! Lowers named stdClass, Mixed, dynamic-class, and nullable property writes.
//!
//! Called from:
//! - The object lowering facade and sibling object support modules.
//!
//! Key details:
//! - Prior values are released and null writes fail at the same observable point.

use super::*;

/// Lowers a named property write to a statically known stdClass receiver.
pub(super) fn lower_stdclass_prop_set(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    value: ValueId,
    property: &str,
) -> Result<()> {
    let value_ty = ctx.value_php_type(value)?.codegen_repr();
    materialize_dynamic_property_mixed_value(ctx, value, &value_ty)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    let (label, len) = ctx.data.add_string(property.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(object, "x0")?;
            abi::emit_symbol_address(ctx.emitter, "x1", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
            abi::emit_pop_reg(ctx.emitter, "x3");
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(object, "rdi")?;
            abi::emit_symbol_address(ctx.emitter, "rsi", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", len as i64);
            abi::emit_pop_reg(ctx.emitter, "rcx");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_set");
    Ok(())
}

/// Lowers a named property write through declared-class dispatch for a boxed Mixed receiver.
pub(super) fn lower_mixed_prop_set(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    value: ValueId,
    property: &str,
    inst: &Instruction,
) -> Result<()> {
    lower_mixed_prop_set_excluding(ctx, object, value, property, inst, &[])
}

/// [`lower_mixed_prop_set`] for a miss path: `answered` are class ids an earlier ladder of the SAME
/// site already matched and wrote, so none of them can reach this one and their arms are left out.
///
/// The named-write ladder falls back here for the classes it did not answer. Re-emitting every
/// declared owner's arm (a typed-slot guard each) behind it doubled the site for classes that can
/// never arrive. Whether the declared ladder is used at all is still decided on the FULL candidate
/// set, so a receiver of an unrelated class keeps reaching the same "undeclared property" fatal.
pub(super) fn lower_mixed_prop_set_excluding(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    value: ValueId,
    property: &str,
    inst: &Instruction,
    answered: &[u64],
) -> Result<()> {
    let value_ty = ctx.value_php_type(value)?;
    let candidates = declared_mixed_property_slot_candidates(ctx, property, inst)?
        .into_iter()
        .filter(|candidate| {
            ensure_property_value_supported(ctx, &candidate.slot, value, &value_ty, inst).is_ok()
        })
        .collect::<Vec<_>>();
    if !candidates.is_empty() {
        let candidates = candidates
            .into_iter()
            .filter(|candidate| !answered.contains(&candidate.class_id))
            .collect::<Vec<_>>();
        return lower_declared_mixed_prop_set(ctx, object, value, property, inst, candidates);
    }
    let value_ty = value_ty.codegen_repr();
    materialize_dynamic_property_mixed_value(ctx, value, &value_ty)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    let (label, len) = ctx.data.add_string(property.as_bytes());
    let done = emit_eval_owner_mixed_prop_set_probe(ctx, object, &label, len)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(object, "x0")?;
            abi::emit_symbol_address(ctx.emitter, "x1", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
            abi::emit_pop_reg(ctx.emitter, "x3");
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(object, "rdi")?;
            abi::emit_symbol_address(ctx.emitter, "rsi", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", len as i64);
            abi::emit_pop_reg(ctx.emitter, "rcx");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_mixed_property_set");
    if let Some(done) = done {
        ctx.emitter.label(&done);
    }
    Ok(())
}

/// Hands a literal-name write on a boxed receiver to Magician when an eval context owns it.
///
/// An eval-declared object is carried by a stdClass runtime object, but its declared properties
/// live in the interpreter's own storage, which is what every read consults. The plain
/// `__rt_mixed_property_set` wrote into the carrier's hash instead, so `$o->count = 5` on an
/// eval-declared object was lost and the next read answered the class default. The runtime-name
/// path already asks `__elephc_eval_dynamic_object_property_set` first; this is the same probe
/// for a name known at compile time.
///
/// Expects the boxed value cell on the top of the temporary stack and leaves it there for the
/// plain path, which then pops it as before. Returns the label that ends the whole write when the
/// probe was emitted, `None` in a module that cannot run eval.
fn emit_eval_owner_mixed_prop_set_probe(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    name_label: &str,
    name_len: usize,
) -> Result<Option<String>> {
    if !crate::codegen::eval_callable_helpers::module_needs_eval_callable_descriptor_support(
        ctx.module,
    ) {
        return Ok(None);
    }
    let plain = ctx.next_label("eval_owner_prop_set_plain");
    let plain_reserved = ctx.next_label("eval_owner_prop_set_plain_reserved");
    let raise = ctx.next_label("eval_owner_prop_set_raise");
    let done = ctx.next_label("eval_owner_prop_set_done");
    let target = ctx.emitter.target;
    let result_reg = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(object, result_reg)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_deref");
    abi::emit_branch_if_int_result_zero(ctx.emitter, &plain);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_branch_if_mixed_unboxed_not_object(ctx, &plain);
    // Frame from here: [sp] Throwable out slot, [sp+16] the value cell pushed by the caller.
    abi::emit_reserve_temporary_stack(ctx.emitter, 16);
    let (arg0, arg1, arg2, arg3, arg4, arg5) = (
        abi::int_arg_reg_name(target, 0),
        abi::int_arg_reg_name(target, 1),
        abi::int_arg_reg_name(target, 2),
        abi::int_arg_reg_name(target, 3),
        abi::int_arg_reg_name(target, 4),
        abi::int_arg_reg_name(target, 5),
    );
    let payload_reg = crate::codegen_support::mixed_unbox_payload_reg(target);
    if payload_reg != arg0 {
        abi::emit_reg_move(ctx.emitter, arg0, payload_reg);
    }
    // php decides visibility from the WRITING function's lexical class; a free function has none.
    let scope = ctx.function.lexical_class.clone().unwrap_or_default();
    abi::emit_symbol_address(ctx.emitter, arg1, name_label);
    abi::emit_load_int_immediate(ctx.emitter, arg2, name_len as i64);
    abi::emit_temporary_stack_address(ctx.emitter, arg3, 0);
    // A closure `Closure::bind` rebound at run time writes under its BOUND scope instead.
    crate::codegen::lower_inst::builtins::eval::emit_effective_scope_name(ctx, &scope, arg4, arg5)?;
    let symbol = target.extern_symbol("__elephc_eval_dynamic_object_property_set");
    abi::emit_call_label(ctx.emitter, &symbol);
    abi::emit_branch_if_int_result_zero(ctx.emitter, &plain_reserved);
    match target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {result_reg}, #2"));          // did Magician hand back a Throwable instead of writing?
            ctx.emitter.instruction(&format!("b.eq {raise}"));                  // raise it after unwinding the temporary blocks
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {result_reg}, 2"));           // did Magician hand back a Throwable instead of writing?
            ctx.emitter.instruction(&format!("je {raise}"));                    // raise it after unwinding the temporary blocks
        }
    }
    // Written by the owner: the value cell is ours to release, and the plain path is skipped.
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
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    abi::emit_jump(ctx.emitter, "__rt_throw_current");

    ctx.emitter.label(&plain_reserved);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    ctx.emitter.label(&plain);
    Ok(Some(done))
}

/// Lowers a named property write through PHP's bare `object` pseudo-type.
///
/// The receiver is an unboxed object payload whose concrete class remains runtime-only. Dispatch
/// therefore mirrors generic property reads, while candidate collection excludes declared slots
/// that cannot accept the replacement value.
pub(super) fn lower_generic_object_prop_set(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    value: ValueId,
    property: &str,
    inst: &Instruction,
) -> Result<()> {
    ctx.load_value_to_reg(object, abi::int_result_reg(ctx.emitter))?;
    lower_generic_object_prop_set_from_loaded_object(ctx, value, property, inst)
}

/// Dispatches a named property write after the raw object payload is in the result register.
fn lower_generic_object_prop_set_from_loaded_object(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    property: &str,
    inst: &Instruction,
) -> Result<()> {
    let value_ty = ctx.value_php_type(value)?;
    let candidates = declared_mixed_property_slot_candidates(ctx, property, inst)?
        .into_iter()
        .filter(|candidate| {
            ensure_property_value_supported(ctx, &candidate.slot, value, &value_ty, inst).is_ok()
        })
        .collect::<Vec<_>>();
    let miss_label = ctx.next_label("generic_object_prop_set_miss");
    let done_label = ctx.next_label("generic_object_prop_set_done");
    let stdclass_label = ctx.next_label("generic_object_prop_set_stdclass");
    let match_labels = candidates
        .iter()
        .map(|candidate| {
            ctx.next_label(&format!(
                "generic_object_prop_set_{}",
                label_fragment(&candidate.slot.class_name)
            ))
        })
        .collect::<Vec<_>>();

    let class_ids = candidates.iter().map(|candidate| candidate.class_id).collect::<Vec<_>>();
    emit_mixed_property_class_dispatch(
        ctx,
        &class_ids,
        &match_labels,
        &stdclass_label,
        &miss_label,
    );

    for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
        ctx.emitter.label(label);
        let base_reg = abi::symbol_scratch_reg(ctx.emitter);
        abi::emit_reg_move(ctx.emitter, base_reg, abi::int_result_reg(ctx.emitter));
        emit_property_store(ctx, value, &candidate.slot, base_reg)?;
        abi::emit_jump(ctx.emitter, &done_label);
    }

    ctx.emitter.label(&stdclass_label);
    emit_stdclass_set_from_loaded_object(ctx, value, property)?;
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&miss_label);
    let message = format!(
        "Fatal error: Cannot write undeclared property ${} through an object receiver\n",
        property
    );
    emit_fatal_message(ctx, message.as_bytes());

    ctx.emitter.label(&done_label);
    Ok(())
}

/// Dispatches a named Mixed-receiver property write across every declared owner of that slot.
fn lower_declared_mixed_prop_set(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    value: ValueId,
    property: &str,
    inst: &Instruction,
    candidates: Vec<MixedPropertyCandidate>,
) -> Result<()> {
    let null_label = ctx.next_label("mixed_prop_set_null");
    let miss_label = ctx.next_label("mixed_prop_set_miss");
    let done_label = ctx.next_label("mixed_prop_set_done");
    let stdclass_label = ctx.next_label("mixed_prop_set_stdclass");
    let match_labels = candidates
        .iter()
        .map(|candidate| {
            ctx.next_label(&format!(
                "mixed_prop_set_{}",
                label_fragment(&candidate.slot.class_name)
            ))
        })
        .collect::<Vec<_>>();

    ctx.load_value_to_reg(object, abi::int_result_reg(ctx.emitter))?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_mixed_object_payload_or_null(ctx, &null_label);
    let class_ids = candidates.iter().map(|candidate| candidate.class_id).collect::<Vec<_>>();
    emit_mixed_property_class_dispatch(
        ctx,
        &class_ids,
        &match_labels,
        &stdclass_label,
        &miss_label,
    );

    let value_ty = ctx.value_php_type(value)?;
    for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
        ctx.emitter.label(label);
        ensure_property_value_supported(ctx, &candidate.slot, value, &value_ty, inst)?;
        let base_reg = abi::symbol_scratch_reg(ctx.emitter);
        let result_reg = abi::int_result_reg(ctx.emitter);
        abi::emit_reg_move(ctx.emitter, base_reg, result_reg);
        emit_property_store(ctx, value, &candidate.slot, base_reg)?;
        abi::emit_jump(ctx.emitter, &done_label);
    }

    ctx.emitter.label(&stdclass_label);
    emit_mixed_stdclass_set_after_eval_probe(ctx, object, value, property)?;
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&miss_label);
    let message = format!(
        "Fatal error: Cannot write undeclared property ${} through a mixed receiver\n",
        property
    );
    emit_fatal_message(ctx, message.as_bytes());

    ctx.emitter.label(&null_label);
    emit_property_assign_on_null_fatal(ctx, property);

    ctx.emitter.label(&done_label);
    Ok(())
}

/// The stdClass arm of a declared-slot ladder, which an eval-declared object also lands in.
///
/// An eval-declared object is carried by a stdClass runtime object, so a name some COMPILED
/// class declares -- `class` is one of `ReflectionProperty`'s -- sent `$o->class = 'A'` on an
/// interpreted `Definition` straight into the carrier's hash, where no reader ever looks: the
/// write was lost. The interpreter is asked first, exactly as in [`lower_mixed_prop_set`].
fn emit_mixed_stdclass_set_after_eval_probe(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    value: ValueId,
    property: &str,
) -> Result<()> {
    let value_ty = ctx.value_php_type(value)?.codegen_repr();
    materialize_dynamic_property_mixed_value(ctx, value, &value_ty)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    let (label, len) = ctx.data.add_string(property.as_bytes());
    let done = emit_eval_owner_mixed_prop_set_probe(ctx, object, &label, len)?;
    ctx.load_value_to_reg(object, abi::int_result_reg(ctx.emitter))?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // the stdClass object pointer
            abi::emit_pop_reg(ctx.emitter, "x3");
            abi::emit_symbol_address(ctx.emitter, "x1", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
        }
        Arch::X86_64 => {
            // `__rt_mixed_unbox` already left the object pointer in rdi.
            abi::emit_pop_reg(ctx.emitter, "rcx");
            abi::emit_symbol_address(ctx.emitter, "rsi", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", len as i64);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_set");
    if let Some(done) = done {
        ctx.emitter.label(&done);
    }
    Ok(())
}

/// Stores a boxed value into a stdClass whose raw object pointer is in the result register.
fn emit_stdclass_set_from_loaded_object(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    property: &str,
) -> Result<()> {
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    let value_ty = ctx.value_php_type(value)?.codegen_repr();
    materialize_dynamic_property_mixed_value(ctx, value, &value_ty)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    let (label, len) = ctx.data.add_string(property.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_reg(ctx.emitter, "x3");
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_symbol_address(ctx.emitter, "x1", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
        }
        Arch::X86_64 => {
            abi::emit_pop_reg(ctx.emitter, "rcx");
            abi::emit_pop_reg(ctx.emitter, "rdi");
            abi::emit_symbol_address(ctx.emitter, "rsi", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", len as i64);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_set");
    Ok(())
}

/// Lowers a static-name write php does not answer from a declared slot on the STATIC class.
///
/// The runtime class decides the whole action, not merely the hash offset: a subclass that
/// redeclares the name as a public property stores into ITS OWN slot, a subclass that declares
/// `__set` is peeled off upstream by `crate::ir_lower`, one that refuses the name raises, and each
/// class lays its hash out at its own offset and reports its own name in php's notice.
pub(super) fn lower_planned_dynamic_prop_set(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    property_value: Option<ValueId>,
    value: ValueId,
    property: &str,
    plan: &PropertyRuntimePlan,
    inst: &Instruction,
) -> Result<()> {
    emit_object_property_runtime_dispatch(
        ctx,
        object,
        plan,
        "prop_set_dynamic",
        DispatchStackCleanup::NONE,
        |ctx, arm| {
            emit_dynamic_plan_write(
                ctx,
                object,
                property_value,
                value,
                property,
                arm,
                inst,
            )
        },
    )
}

/// Emits one runtime-class arm of such a write.
fn emit_dynamic_plan_write(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    property_value: Option<ValueId>,
    value: ValueId,
    property: &str,
    arm: &PropertyRuntimeArm,
    inst: &Instruction,
) -> Result<()> {
    match &arm.action {
        // This runtime class DECLARES the name, so php stores into its slot. The arm only runs
        // when the receiver's runtime class id matched, so the slot is this class's own.
        PropertyRuntimeAction::Slot(slot) => {
            let value_ty = ctx.value_php_type(value)?;
            ensure_property_value_supported(ctx, slot, value, &value_ty, inst)?;
            let base_reg = abi::symbol_scratch_reg(ctx.emitter);
            ctx.load_value_to_reg(object, base_reg)?;
            emit_hook_aware_property_store(ctx, value, slot, base_reg)
        }
        PropertyRuntimeAction::DynamicHash { hash_offset, .. } => {
            // The NAME comes from the arm, not from the receiver's static type: php reports the
            // class the instance really is, so a `Mid`-typed write on a `Leaf` says `Leaf::$p`.
            emit_named_dynamic_property_creation_deprecation(
                ctx,
                &arm.class_name,
                property,
                object,
                *hash_offset,
                inst.span.map_or(0, |span| span.line),
            )?;
            lower_allow_dynamic_prop_set(ctx, object, value, property, *hash_offset)
        }
        // php STORES a value here, so there is no correct way to emit nothing.
        // `crate::types::checker::scope_dynamic_storage` reserves the hash for exactly the classes
        // a reachable mutation can address and expands that over subclasses, which makes this arm
        // unreachable; reaching it means the reservation and this dispatch disagree, and the build
        // has to fail rather than drop the write in silence.
        PropertyRuntimeAction::DynamicMissing { .. } => {
            Err(dynamic_write_without_storage(&arm.class_name, property))
        }
        PropertyRuntimeAction::MagicDeferred => {
            let property_value = property_value.ok_or_else(|| {
                CodegenIrError::invalid_module(
                    "guarded direct magic property write missing its string operand",
                )
            })?;
            let hash_offset =
                dynamic_property_hash_offset_for_class(ctx, &arm.class_name, property)?;
            let recursive_refusal =
                magic_set_recursive_refusal(ctx, &arm.class_name, property);
            lower_direct_magic_set(
                ctx,
                object,
                property_value,
                value,
                &arm.class_name,
                hash_offset,
                recursive_refusal.as_deref(),
                inst.span.map_or(0, |span| span.line),
            )
        }
        PropertyRuntimeAction::MagicGet | PropertyRuntimeAction::DynamicHashThenMagicGet { .. } => Err(CodegenIrError::invalid_module(
            "property write resolved to a magic getter",
        )),
        PropertyRuntimeAction::Refuse { .. } => Err(CodegenIrError::invalid_module(
            "property dispatch handed a refusal arm to its action emitter",
        )),
    }
}

/// Emits php 8.5's `Creation of dynamic property C::$p is deprecated` for a STATIC name.
///
/// The runtime-name sibling below has to splice the key in from the temporary stack. A literal
/// name makes the whole line a single constant, so the only runtime work is the existence probe
/// php itself performs: the notice is reported when the key is ABSENT, and assigning to a
/// dynamic property that already exists is an ordinary write php says nothing about.
///
/// `ClassInfo::dynamic_property_creation_is_deprecated()` is the single authority for whether
/// php reports anything at all, so an `#[\AllowDynamicProperties]` class stays silent and a class
/// whose hash the compiler reserved on its own behalf still reports, exactly as php does for an
/// ordinary class.
fn emit_named_dynamic_property_creation_deprecation(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    property: &str,
    object: ValueId,
    hash_offset: usize,
    line: u32,
) -> Result<()> {
    let deprecated = class_dynamic_property_creation_is_deprecated(ctx, class_name);
    if !deprecated {
        return Ok(());
    }
    let target = ctx.emitter.target;
    let skip_label = ctx.next_label("named_dyn_prop_create_deprecation_skip");
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    let (key_label, key_len) = ctx.data.add_string(property.as_bytes());
    ctx.load_value_to_reg(object, object_reg)?;
    abi::emit_load_from_address(
        ctx.emitter,
        abi::int_arg_reg_name(target, 0),
        object_reg,
        hash_offset,
    );                                                                          // pass the receiver's dynamic-property hash to the existence probe
    abi::emit_symbol_address(ctx.emitter, abi::int_arg_reg_name(target, 1), &key_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(target, 2),
        key_len as i64,
    );
    abi::emit_call_label(ctx.emitter, "__rt_hash_get");
    emit_branch_if_hash_entry_found(ctx, &skip_label);                          // an existing key is a plain write, including false and null
    emit_property_warning_fragment(
        ctx,
        format!(
            "Deprecated: Creation of dynamic property {}::${} is deprecated",
            class_name, property
        )
        .as_bytes(),
        false,
    );
    emit_property_warning_location(ctx, line);
    emit_property_warning_fragment(ctx, b"\n", true);
    ctx.emitter.label(&skip_label);
    Ok(())
}

/// Lowers a static-name write to an undeclared property on an allow-dynamic class.
pub(super) fn lower_allow_dynamic_prop_set(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    value: ValueId,
    property: &str,
    hash_offset: usize,
) -> Result<()> {
    let value_ty = ctx.value_php_type(value)?.codegen_repr();
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    let boxed_reg = abi::secondary_scratch_reg(ctx.emitter);
    let (label, key_len) = ctx.data.add_string(property.as_bytes());
    ctx.load_value_to_reg(object, object_reg)?;
    abi::emit_push_reg(ctx.emitter, object_reg);
    materialize_dynamic_property_mixed_value(ctx, value, &value_ty)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("mov {}, x0", boxed_reg));         // preserve the boxed dynamic-property value across receiver restore
            abi::emit_pop_reg(ctx.emitter, object_reg);
            ctx.emitter
                .instruction(&format!("ldr x0, [{}, #{}]", object_reg, hash_offset)); // load the dynamic-property hash pointer from the receiver
            abi::emit_push_reg(ctx.emitter, object_reg);
            abi::emit_symbol_address(ctx.emitter, "x1", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", key_len as i64);
            ctx.emitter.instruction(&format!("mov x3, {}", boxed_reg));         // pass the boxed Mixed cell as the hash value payload
            ctx.emitter.instruction("mov x4, xzr");                             // boxed Mixed hash entries do not use the high payload word
            abi::emit_load_int_immediate(
                ctx.emitter,
                "x5",
                runtime_value_tag(&PhpType::Mixed) as i64,
            );
            abi::emit_call_label(ctx.emitter, "__rt_hash_set");
            abi::emit_pop_reg(ctx.emitter, object_reg);
            abi::emit_store_to_address(ctx.emitter, "x0", object_reg, hash_offset);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov {}, rax", boxed_reg));        // preserve the boxed dynamic-property value across receiver restore
            abi::emit_pop_reg(ctx.emitter, object_reg);
            ctx.emitter.instruction(&format!(                                   // load the dynamic-property hash pointer from the receiver
                "mov rdi, QWORD PTR [{} + {}]",
                object_reg, hash_offset
            ));
            abi::emit_push_reg(ctx.emitter, object_reg);
            abi::emit_symbol_address(ctx.emitter, "rsi", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", key_len as i64);
            ctx.emitter.instruction(&format!("mov rcx, {}", boxed_reg));        // pass the boxed Mixed cell as the hash value payload
            ctx.emitter.instruction("xor r8, r8");                              // boxed Mixed hash entries do not use the high payload word
            abi::emit_load_int_immediate(
                ctx.emitter,
                "r9",
                runtime_value_tag(&PhpType::Mixed) as i64,
            );
            abi::emit_call_label(ctx.emitter, "__rt_hash_set");
            abi::emit_pop_reg(ctx.emitter, object_reg);
            abi::emit_store_to_address(ctx.emitter, "rax", object_reg, hash_offset);
        }
    }
    Ok(())
}

/// Lowers a RUNTIME-name write to an undeclared property on an allow-dynamic class.
///
/// The static-name sibling interns the key in the data pool. A name only known at run time has
/// no such label, so the pointer/length pair is taken from the caller's temporary stack frame,
/// the same frame `lower_runtime_object_prop_set` already staged the receiver and the name in.
///
/// Without this the declared-slot ladder's MISS arm fell off the end and the write vanished:
/// `$dyn->{$name} = $v` on an `#[\AllowDynamicProperties]` class stored nothing at all, which is
/// also what `clone($dyn, [$name => $v])` would have done for every undeclared key.
///
/// `frame_bytes` is the caller's reserved block: the receiver sits at `receiver_offset`, the
/// name pointer at `name_offset` and its length at `name_offset + 8`. The block is released here.
pub(super) fn lower_runtime_allow_dynamic_prop_set(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    hash_offset: usize,
    receiver_offset: usize,
    name_offset: usize,
    frame_bytes: usize,
) -> Result<()> {
    let value_ty = ctx.value_php_type(value)?.codegen_repr();
    let boxed_reg = abi::secondary_scratch_reg(ctx.emitter).to_string();
    let object_reg = abi::symbol_scratch_reg(ctx.emitter).to_string();
    let target = ctx.emitter.target;
    materialize_dynamic_property_mixed_value(ctx, value, &value_ty)?;
    abi::emit_reg_move(ctx.emitter, &boxed_reg, abi::int_result_reg(ctx.emitter));
    abi::emit_load_temporary_stack_slot(ctx.emitter, &object_reg, receiver_offset);
    abi::emit_load_from_address(
        ctx.emitter,
        abi::int_arg_reg_name(target, 0),
        &object_reg,
        hash_offset,
    );
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(target, 1), name_offset);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(target, 2),
        name_offset + 8,
    );
    abi::emit_reg_move(ctx.emitter, abi::int_arg_reg_name(target, 3), &boxed_reg);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(target, 4), 0);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(target, 5),
        runtime_value_tag(&PhpType::Mixed) as i64,
    );
    abi::emit_call_label(ctx.emitter, "__rt_hash_set");
    // The helper can reallocate, so the receiver is reloaded and the fresh table stored back.
    abi::emit_load_temporary_stack_slot(ctx.emitter, &object_reg, receiver_offset);
    abi::emit_store_to_address(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        &object_reg,
        hash_offset,
    );
    abi::emit_release_temporary_stack(ctx.emitter, frame_bytes);
    Ok(())
}

/// Lowers a STATIC-name dynamic-property write whose receiver is staged on the temporary stack.
///
/// The sibling above takes the key from the stack because its name is only known at run time;
/// this one takes the receiver from the stack because the name is a constant but the RECEIVER is
/// a Mixed-ladder arm's unboxed payload. It does not release the frame: the Mixed write ladder
/// releases one block for every arm at the arm's own end, so that all arms converge with the
/// same stack pointer.
pub(super) fn lower_stacked_named_dynamic_prop_set(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    property: &str,
    hash_offset: usize,
    receiver_offset: usize,
) -> Result<()> {
    let value_ty = ctx.value_php_type(value)?.codegen_repr();
    let boxed_reg = abi::secondary_scratch_reg(ctx.emitter).to_string();
    let object_reg = abi::symbol_scratch_reg(ctx.emitter).to_string();
    let target = ctx.emitter.target;
    let (key_label, key_len) = ctx.data.add_string(property.as_bytes());
    materialize_dynamic_property_mixed_value(ctx, value, &value_ty)?;
    abi::emit_reg_move(ctx.emitter, &boxed_reg, abi::int_result_reg(ctx.emitter));
    abi::emit_load_temporary_stack_slot(ctx.emitter, &object_reg, receiver_offset);
    abi::emit_load_from_address(
        ctx.emitter,
        abi::int_arg_reg_name(target, 0),
        &object_reg,
        hash_offset,
    );
    abi::emit_symbol_address(ctx.emitter, abi::int_arg_reg_name(target, 1), &key_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(target, 2), key_len as i64);
    abi::emit_reg_move(ctx.emitter, abi::int_arg_reg_name(target, 3), &boxed_reg);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(target, 4), 0);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(target, 5),
        runtime_value_tag(&PhpType::Mixed) as i64,
    );
    abi::emit_call_label(ctx.emitter, "__rt_hash_set");
    // The helper can reallocate, so the receiver is reloaded and the fresh table stored back.
    abi::emit_load_temporary_stack_slot(ctx.emitter, &object_reg, receiver_offset);
    abi::emit_store_to_address(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        &object_reg,
        hash_offset,
    );
    Ok(())
}

/// Returns whether creating a dynamic property on `class_name` is deprecated.
///
/// `ClassInfo` is the authority for compiler-reserved storage; a reviewed class contract
/// can additionally force the php-src deprecation for an internal class that opts into
/// dynamic-property storage (GMP) without the `AllowDynamicProperties` exemption.
fn class_dynamic_property_creation_is_deprecated(
    ctx: &FunctionContext<'_>,
    class_name: &str,
) -> bool {
    ctx.module
        .class_infos
        .get(class_name)
        .is_some_and(|info| info.dynamic_property_creation_is_deprecated())
        || elephc_builtin_contract::lookup_class(class_name)
            .is_some_and(|contract| contract.dynamic_properties_deprecated)
}

/// Emits php's dynamic-creation notice for a STATIC name on a stacked receiver.
///
/// Same probe-then-report shape as the two siblings: the notice belongs to CREATION, so an
/// existing key is a plain write php says nothing about, and
/// `dynamic_property_creation_is_deprecated()` stays the single authority for whether php
/// reports anything on this class at all.
pub(super) fn emit_stacked_named_dynamic_property_creation_deprecation(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    property: &str,
    hash_offset: usize,
    receiver_offset: usize,
    line: u32,
) -> Result<()> {
    let deprecated = class_dynamic_property_creation_is_deprecated(ctx, class_name);
    if !deprecated {
        return Ok(());
    }
    let target = ctx.emitter.target;
    let skip_label = ctx.next_label("stacked_dyn_prop_create_deprecation_skip");
    let object_reg = abi::symbol_scratch_reg(ctx.emitter).to_string();
    let (key_label, key_len) = ctx.data.add_string(property.as_bytes());
    abi::emit_load_temporary_stack_slot(ctx.emitter, &object_reg, receiver_offset);
    abi::emit_load_from_address(
        ctx.emitter,
        abi::int_arg_reg_name(target, 0),
        &object_reg,
        hash_offset,
    );
    abi::emit_symbol_address(ctx.emitter, abi::int_arg_reg_name(target, 1), &key_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(target, 2), key_len as i64);
    abi::emit_call_label(ctx.emitter, "__rt_hash_get");
    emit_branch_if_hash_entry_found(ctx, &skip_label);                          // an existing key is a plain write, including false and null
    emit_property_warning_fragment(
        ctx,
        format!(
            "Deprecated: Creation of dynamic property {}::${} is deprecated",
            class_name, property
        )
        .as_bytes(),
        false,
    );
    emit_property_warning_location(ctx, line);
    emit_property_warning_fragment(ctx, b"\n", true);
    ctx.emitter.label(&skip_label);
    Ok(())
}

/// Materializes a property value as an owned boxed `Mixed` cell in the result register.
pub(super) fn materialize_dynamic_property_mixed_value(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    value_ty: &PhpType,
) -> Result<()> {
    ctx.load_value_to_result(value)?;
    if matches!(value_ty, PhpType::Mixed | PhpType::Union(_)) {
        if !ctx.value_can_own_mixed_box_source(value)? {
            abi::emit_incref_if_refcounted(ctx.emitter, &value_ty.codegen_repr());
        }
    } else {
        let staged = if value_ty == &PhpType::Str {
            let (pointer_reg, length_reg) = abi::string_result_regs(ctx.emitter);
            super::super::ownership::stage_string_byte_view(
                ctx,
                value,
                pointer_reg,
                length_reg,
            )
        } else {
            false
        };
        // A `?ptr<T>` boxes by its declared type, so address 0 is stored as PHP null.
        let raw_ty = ctx.raw_value_php_type(value)?;
        let box_ty = if crate::types::is_nullable_pointer_type(&raw_ty) {
            raw_ty
        } else {
            value_ty.clone()
        };
        emit_box_current_value_as_mixed(ctx.emitter, &box_ty);
        super::super::ownership::release_string_byte_stage(ctx, staged);
    }
    Ok(())
}

/// Lowers a property write on a nullable receiver, fataling after RHS evaluation when null.
pub(super) fn lower_nullable_prop_set(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    value: ValueId,
    class_name: &str,
    property: &str,
) -> Result<()> {
    if class_name.trim_start_matches('\\').is_empty() {
        return lower_nullable_generic_object_prop_set(ctx, inst, object, value, property);
    }
    let slot = resolve_property_slot_for_class(ctx, class_name, property, inst)?;
    let value_ty = ctx.value_php_type(value)?;
    ensure_property_value_supported(ctx, &slot, value, &value_ty, inst)?;
    let null_label = ctx.next_label("nullable_prop_set_null");
    let done_label = ctx.next_label("nullable_prop_set_done");
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    emit_nullable_receiver_object_payload(ctx, object, &null_label, base_reg)?;
    emit_hook_aware_property_store(ctx, value, &slot, base_reg)?;
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&null_label);
    emit_property_assign_on_null_fatal(ctx, property);

    ctx.emitter.label(&done_label);
    Ok(())
}

/// Lowers a nullable bare-object property write through concrete runtime class dispatch.
fn lower_nullable_generic_object_prop_set(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    value: ValueId,
    property: &str,
) -> Result<()> {
    let null_label = ctx.next_label("nullable_generic_prop_set_null");
    let done_label = ctx.next_label("nullable_generic_prop_set_done");
    let object_reg = abi::int_result_reg(ctx.emitter);
    emit_nullable_receiver_object_payload(ctx, object, &null_label, object_reg)?;
    lower_generic_object_prop_set_from_loaded_object(ctx, value, property, inst)?;
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&null_label);
    emit_property_assign_on_null_fatal(ctx, property);

    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits PHP's fatal diagnostic for assigning a property on null.
pub(super) fn emit_property_assign_on_null_fatal(ctx: &mut FunctionContext<'_>, property: &str) {
    let message = format!(
        "Fatal error: Attempt to assign property \"{}\" on null\n",
        property
    );
    let (message_label, message_len) = ctx.data.add_string(message.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #2");                              // write the property-assign-on-null fatal to stderr
            ctx.emitter.adrp("x1", &message_label);
            ctx.emitter.add_lo12("x1", "x1", &message_label);
            ctx.emitter
                .instruction(&format!("mov x2, #{}", message_len)); // pass the property-assign-on-null fatal byte length
            ctx.emitter.syscall(4);
            abi::emit_exit(ctx.emitter, 1);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov edi, 2");                              // write the property-assign-on-null fatal to Linux stderr
            abi::emit_symbol_address(ctx.emitter, "rsi", &message_label);
            ctx.emitter
                .instruction(&format!("mov edx, {}", message_len)); // pass the property-assign-on-null fatal byte length
            ctx.emitter.instruction("mov eax, 1");                              // Linux x86_64 syscall 1 = write
            ctx.emitter.instruction("syscall");                                 // emit the property-assign-on-null fatal before exiting
            abi::emit_exit(ctx.emitter, 1);
        }
    }
}

/// Emits php 8.5's dynamic-property deprecation before a runtime-name hash write CREATES a key.
///
/// The class reached this path through the property hash
/// `crate::types::checker::clone_override_storage` reserved for `clone($object, [...])`, which is
/// storage only: php still reports `Creation of dynamic property C::$n is deprecated` for a class
/// that carries neither `#[\AllowDynamicProperties]` nor stdClass's engine exemption. The level is
/// not passed explicitly; `__rt_diag_warning` derives `E_DEPRECATED` from the `Deprecated: `
/// prefix, strips it for a user error handler, and gates the default line on `error_reporting`.
///
/// Only a CREATION is reported, so the key is probed first: re-cloning an object that already
/// carries the name overwrites it, and php 8.5.10 stays silent for that second write.
///
/// Nothing owned exists yet at this point. The override value is boxed by
/// `lower_runtime_allow_dynamic_prop_set` afterwards, so an error handler that THROWS out of the
/// deprecation unwinds with no boxed cell and no half-written hash entry to leak, and the clone
/// itself is released by the applicator's ordinary throwing path.
pub(super) fn emit_dynamic_property_creation_deprecation(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    hash_offset: usize,
    receiver_offset: usize,
    name_offset: usize,
    line: u32,
) -> Result<()> {
    let deprecated = class_dynamic_property_creation_is_deprecated(ctx, class_name);
    if !deprecated {
        return Ok(());
    }
    let target = ctx.emitter.target;
    let skip_label = ctx.next_label("dyn_prop_create_deprecation_skip");
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
    emit_branch_if_hash_entry_found(ctx, &skip_label);
    emit_property_warning_fragment(
        ctx,
        format!("Deprecated: Creation of dynamic property {}::$", class_name).as_bytes(),
        false,
    );
    let (name_ptr_reg, name_len_reg) = match target.arch {
        Arch::AArch64 => ("x1", "x2"),
        Arch::X86_64 => ("rdi", "rsi"),
    };
    abi::emit_load_temporary_stack_slot(ctx.emitter, name_ptr_reg, name_offset);
    abi::emit_load_temporary_stack_slot(ctx.emitter, name_len_reg, name_offset + 8);
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning_fragment");
    emit_property_warning_fragment(ctx, b" is deprecated", false);
    emit_property_warning_location(ctx, line);
    emit_property_warning_fragment(ctx, b"\n", true);
    ctx.emitter.label(&skip_label);
    Ok(())
}

/// Branches on `__rt_hash_get`'s entry-address output, which distinguishes a missing key from a
/// present key whose stored value is false or null.
pub(super) fn emit_branch_if_hash_entry_found(ctx: &mut FunctionContext<'_>, label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x4, #0");                              // test the matching entry address, not its possibly-false value
            ctx.emitter.instruction(&format!("b.ne {label}"));                  // skip creation behavior when the key is already present
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test r8, r8");                             // test the matching entry address, not its possibly-false value
            ctx.emitter.instruction(&format!("jnz {label}"));                   // skip creation behavior when the key is already present
        }
    }
}
