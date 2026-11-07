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
        return emit_hook_aware_property_store(ctx, value, &slot, base_reg);
    }
    if let Some(Immediate::PropertyRef { class, property }) = inst.immediate {
        let slot = resolve_initializer_property_slot(ctx, object, class, property, inst)?;
        let value_ty = ctx.value_php_type(value)?;
        ensure_property_value_supported(ctx, &slot, value, &value_ty, inst)?;
        let base_reg = abi::symbol_scratch_reg(ctx.emitter);
        ctx.load_value_to_reg(object, base_reg)?;
        initialize_owned_property_reference(ctx, &slot, base_reg, false);
        return emit_property_store(ctx, value, &slot, base_reg);
    }
    let property = property_name_immediate(ctx, inst)?.to_string();
    if let Some(class_name) = extern_receiver_class(ctx, &ctx.value_php_type(object)?) {
        return lower_extern_prop_set(ctx, inst, object, value, &class_name, &property);
    }
    if let Some((class_name, true)) = nullable_object_receiver_class(ctx, object)? {
        return lower_nullable_prop_set(ctx, inst, object, value, &class_name, &property);
    }
    if matches!(ctx.value_php_type(object)?.codegen_repr(), PhpType::Mixed) {
        return lower_mixed_named_prop_set(ctx, object, value, &property, inst);
    }
    if matches!(
        ctx.value_php_type(object)?.codegen_repr(),
        PhpType::Object(class_name) if class_name.trim_start_matches('\\').is_empty()
    ) {
        return lower_generic_object_prop_set(ctx, object, value, &property, inst);
    }
    if object_is_builtin_stdclass(ctx, object)? {
        return lower_stdclass_prop_set(ctx, object, value, &property);
    }
    // A receiver whose class the closed world does not contain. Its layout cannot be resolved,
    // but the write is not unlowerable: the class can only come into existence at run time, and
    // the by-name path already used for an untyped object receiver resolves it there.
    //
    // The shape is ordinary framework code. Symfony's `CacheAttributeListener` declares
    // `processAttributeBeforeController(Cache $cache, …)` and assigns `$cache->if`; nothing
    // constructs a `Cache` statically — instances only ever arrive through
    // `ReflectionAttribute::newInstance()` — so the class is absent from the module while the
    // method around it is compiled. Refusing the write made the whole listener uncompilable.
    if object_class_is_absent_from_module(ctx, object)? {
        return lower_generic_object_prop_set(ctx, object, value, &property, inst);
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
    emit_typed_pointer_receiver_null_check(ctx, object, base_reg)?;
    if is_promoted_reference_property_bind(ctx, object, value, &slot)? {
        return emit_reference_property_bind(ctx, value, &slot, base_reg);
    }
    emit_hook_aware_property_store(ctx, value, &slot, base_reg)
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
    emit_hook_aware_property_store(ctx, value, &slot, base_reg)
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
                        inst.span.map_or(0, |span| span.line),
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
                        inst.span.map_or(0, |span| span.line),
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
    // A later site of a name reaches the helper its first site asked for, without searching the
    // whole program for arms it will not emit.
    if try_lower_outlined_named_mixed_prop_set(ctx, inst, object, value, property, None)? {
        return Ok(());
    }
    let candidates = declared_mixed_named_property_set_candidates(ctx, value, property, inst)?;
    if candidates.is_empty() {
        return lower_mixed_prop_set(ctx, object, value, property, inst);
    }
    // Every class that merely ACCEPTS the name into its own hash would otherwise contribute an
    // identical inline arm (probe, deprecation, `__rt_hash_set`) at every site: a Symfony build
    // carried 35,608 of them in `VarCloner::doClone` alone. Those classes share one table-driven
    // block (`emit_table_dynamic_hash_write`); only the classes this site must keep away from it
    // still get an explicit comparison.
    if try_lower_outlined_named_mixed_prop_set(ctx, inst, object, value, property, Some(candidates.len()))? {
        return Ok(());
    }
    let (candidates, table_routed) =
        split_table_routed_dynamic_hash_candidates(ctx, candidates, property);
    let table_misses = if table_routed.is_empty() {
        Vec::new()
    } else {
        table_addressable_classes_outside(ctx, &candidates, &table_routed)
    };
    let done_label = ctx.next_label("mixed_named_prop_set_done");
    let miss_label = ctx.next_label("mixed_named_prop_set_miss");
    let mut match_labels = candidates
        .iter()
        .map(|candidate| {
            ctx.next_label(&format!(
                "mixed_named_prop_set_{}_{}",
                candidate.class_id,
                label_fragment(property)
            ))
        })
        .collect::<Vec<_>>();
    let arm_owners = shared_named_set_arm_owners(ctx, &candidates);
    for (index, owner) in arm_owners.iter().enumerate() {
        match_labels[index] = match_labels[*owner].clone();
    }

    ctx.load_value_to_reg(object, abi::int_result_reg(ctx.emitter))?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_branch_if_mixed_unboxed_not_object(ctx, &done_label);
    push_mixed_unboxed_object_payload(ctx);

    let dispatch = candidates
        .iter()
        .zip(match_labels.iter())
        .map(|(candidate, label)| (candidate.class_id, label.as_str()));
    for (first, last, label) in class_id_runs(dispatch) {
        emit_branch_if_stacked_object_class_in_run(ctx, first, last, 0, label);
    }
    for class_id in &table_misses {
        emit_branch_if_stacked_object_class_matches(ctx, *class_id, 0, &miss_label);
    }
    if table_routed.is_empty() {
        abi::emit_jump(ctx.emitter, &miss_label);
    } else {
        emit_table_dynamic_hash_write(ctx, value, property, &miss_label)?;
        abi::emit_release_temporary_stack(ctx.emitter, 16);
        abi::emit_jump(ctx.emitter, &done_label);
    }

    for (index, (candidate, label)) in candidates.iter().zip(match_labels.iter()).enumerate() {
        if arm_owners[index] != index {
            continue;
        }
        ctx.emitter.label(label);
        match &candidate.action {
            MixedPropertyWriteAction::Refuse(message) => {
                abi::emit_release_temporary_stack(ctx.emitter, 16);
                super::super::exceptions::emit_error(ctx, message);
            }
            MixedPropertyWriteAction::RefuseUnlessTagMatches { slot, message } => {
                emit_refined_untyped_tag_guarded_store(ctx, value, slot, message, 0, 16, &done_label)?;
            }
            MixedPropertyWriteAction::Slot(slot) => {
                // The arm can answer a whole subtree, so its diagnostics name the declaring class.
                let mut slot = slot.clone();
                slot.class_name = message_class_for_slot(ctx, &slot);
                let slot = &slot;
                super::readonly_guard::emit_readonly_slot_guard(
                    ctx,
                    slot,
                    false,
                    super::readonly_guard::GuardReceiver::Stack(0),
                    16,
                )?;
                emit_refined_scalar_slot_guard(ctx, value, slot, 16)?;
                let base_reg = abi::symbol_scratch_reg(ctx.emitter);
                abi::emit_load_temporary_stack_slot(ctx.emitter, base_reg, 0);
                emit_hook_aware_property_store(ctx, value, slot, base_reg)?;
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
                    inst.span.map_or(0, |span| span.line),
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
    // Every explicit arm above ends the write (store, refusal or hash write), so a receiver of one
    // of those classes never reaches this fallback.
    let answered = candidates
        .iter()
        .map(|candidate| candidate.class_id)
        .collect::<Vec<_>>();
    super::named_property_writes::lower_mixed_prop_set_excluding(
        ctx, object, value, property, inst, &answered,
    )?;
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Splits off the dynamic-hash arms the `_class_dyn_hash_rows` table answers exactly.
///
/// An arm qualifies when its class does not declare the name at all (so no scope rule can turn
/// it into a mangled private) and its hash offset is the one the table records. Anything else
/// keeps its explicit arm. Returns `(explicit candidates, table-routed class ids)`.
fn split_table_routed_dynamic_hash_candidates(
    ctx: &FunctionContext<'_>,
    candidates: Vec<MixedPropertyWriteCandidate>,
    property: &str,
) -> (Vec<MixedPropertyWriteCandidate>, Vec<u64>) {
    let mut explicit = Vec::new();
    let mut routed = Vec::new();
    for candidate in candidates {
        let table_answers = match &candidate.action {
            MixedPropertyWriteAction::DynamicHash {
                class_name,
                hash_offset,
            } => ctx.module.class_infos.get(class_name).is_some_and(|info| {
                info.dynamic_property_hash_is_name_addressable()
                    && !info.properties.iter().any(|(declared, _)| declared == property)
                    && *hash_offset
                        == super::allocation_clone::dynamic_property_hash_offset(
                            info.properties.len(),
                        )
            }),
            _ => false,
        };
        if table_answers {
            routed.push(candidate.class_id);
        } else {
            explicit.push(candidate);
        }
    }
    (explicit, routed)
}

/// Lists the classes the table would route into a hash write that this site must NOT write:
/// name-addressable classes that are neither explicit arms nor table-routed here (an accessor
/// owner, say). Each gets an explicit jump to the miss path ahead of the table block.
fn table_addressable_classes_outside(
    ctx: &FunctionContext<'_>,
    explicit: &[MixedPropertyWriteCandidate],
    routed: &[u64],
) -> Vec<u64> {
    let routed = routed.iter().copied().collect::<std::collections::HashSet<_>>();
    let explicit = explicit
        .iter()
        .map(|candidate| candidate.class_id)
        .collect::<std::collections::HashSet<_>>();
    let mut ids = ctx
        .module
        .class_infos
        .iter()
        .filter(|(name, info)| {
            info.dynamic_property_hash_is_name_addressable()
                && !crate::types::checker::builtin_stdclass::is_stdclass(name)
                && !routed.contains(&info.class_id)
                && !explicit.contains(&info.class_id)
        })
        .map(|(_, info)| info.class_id)
        .collect::<Vec<_>>();
    ids.sort_unstable();
    ids
}

/// Writes a literal-name dynamic property through the receiver class's `_class_dyn_hash_rows`
/// row: the per-instance hash offset, then php's creation deprecation when the row flags it.
///
/// Expects the unboxed receiver at `[sp]` and leaves the stack as it found it. A class id outside
/// the table, or a row without a hash, goes to `miss_label` with only the receiver stacked, which
/// is where an unmatched class has always gone.
fn emit_table_dynamic_hash_write(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    property: &str,
    miss_label: &str,
) -> Result<()> {
    let have_row = ctx.next_label("table_dyn_prop_row");
    let skip_deprecation = ctx.next_label("table_dyn_prop_no_deprecation");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", 0);
            ctx.emitter.instruction("ldr x10, [x9]");                           // load the receiver's runtime class id
            abi::emit_symbol_address(ctx.emitter, "x11", "_class_name_count");
            ctx.emitter.instruction("ldr x11, [x11]");                          // load the dense class table bound
            ctx.emitter.instruction("cmp x10, x11");                            // ids outside the table have no row
            ctx.emitter.instruction(&format!("b.hs {}", miss_label));           // leave them to the miss path
            abi::emit_symbol_address(ctx.emitter, "x11", "_class_dyn_hash_rows");
            ctx.emitter.instruction("add x11, x11, x10, lsl #4");               // address the class's 16-byte row
            ctx.emitter.instruction("ldp x12, x13, [x11]");                     // load the hash offset and the deprecation flag
            ctx.emitter.instruction(&format!("cbnz x12, {}", have_row));        // a class with an addressable hash takes the write
            ctx.emitter.instruction(&format!("b {}", miss_label));              // no hash: the miss path answers
            ctx.emitter.label(&have_row);
            ctx.emitter.instruction("stp x12, x13, [sp, #-16]!");               // keep the row across the helper calls
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r11", 0);
            ctx.emitter.instruction("mov r10, QWORD PTR [r11]");                // load the receiver's runtime class id
            ctx.emitter.instruction("cmp r10, QWORD PTR [rip + _class_name_count]"); // ids outside the table have no row
            ctx.emitter.instruction(&format!("jae {}", miss_label));            // leave them to the miss path
            ctx.emitter.instruction("lea r11, [rip + _class_dyn_hash_rows]");   // materialize the row table base
            ctx.emitter.instruction("shl r10, 4");                              // scale the class id to its 16-byte row
            ctx.emitter.instruction("mov r9, QWORD PTR [r11 + r10]");           // load the hash offset
            ctx.emitter.instruction("test r9, r9");                             // does this class have an addressable hash?
            ctx.emitter.instruction(&format!("jnz {}", have_row));              // a class with one takes the write
            ctx.emitter.instruction(&format!("jmp {}", miss_label));            // no hash: the miss path answers
            ctx.emitter.label(&have_row);
            ctx.emitter.instruction("mov r10, QWORD PTR [r11 + r10 + 8]");      // load the deprecation flag
            ctx.emitter.instruction("sub rsp, 16");                             // keep the row across the helper calls
            ctx.emitter.instruction("mov QWORD PTR [rsp], r9");                 // row word 0: the hash offset
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], r10");            // row word 1: the deprecation flag
        }
    }
    // Stack: [sp] = hash offset, [sp + 8] = deprecation flag, [sp + 16] = receiver.
    let (key_label, key_len) = ctx.data.add_string(property.as_bytes());
    let target = ctx.emitter.target;
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), 8);
    abi::emit_branch_if_int_result_zero(ctx.emitter, &skip_deprecation);
    emit_table_row_hash_to_first_arg(ctx);
    abi::emit_symbol_address(ctx.emitter, abi::int_arg_reg_name(target, 1), &key_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(target, 2), key_len as i64);
    abi::emit_call_label(ctx.emitter, "__rt_hash_get");
    super::named_property_writes::emit_branch_if_hash_entry_found(ctx, &skip_deprecation);
    super::emit_property_warning_fragment(ctx, b"Deprecated: Creation of dynamic property ", false);
    let result_reg = abi::int_result_reg(ctx.emitter).to_string();
    abi::emit_load_temporary_stack_slot(ctx.emitter, &result_reg, 16);
    super::super::runtime_class_messages::emit_runtime_class_name_to_string_result(
        ctx,
        &result_reg,
        "object",
    );
    if ctx.emitter.target.arch == Arch::X86_64 {
        let (name_ptr, name_len) = abi::string_result_regs(ctx.emitter);
        ctx.emitter.instruction(&format!("mov rdi, {}", name_ptr));             // pass the class name to the diagnostic fragment
        ctx.emitter.instruction(&format!("mov rsi, {}", name_len));             // pass its length
    }
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning_fragment");
    super::emit_property_warning_fragment(
        ctx,
        format!("::${} is deprecated\n", property).as_bytes(),
        true,
    );
    ctx.emitter.label(&skip_deprecation);

    let value_ty = ctx.value_php_type(value)?.codegen_repr();
    super::named_property_writes::materialize_dynamic_property_mixed_value(ctx, value, &value_ty)?;
    // The row reload below uses the scratch registers, so the boxed value waits on the stack.
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    emit_table_row_hash_to_first_arg_at(ctx, 16);
    abi::emit_symbol_address(ctx.emitter, abi::int_arg_reg_name(target, 1), &key_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(target, 2), key_len as i64);
    abi::emit_pop_reg(ctx.emitter, abi::int_arg_reg_name(target, 3));
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(target, 4), 0);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(target, 5),
        runtime_value_tag(&PhpType::Mixed) as i64,
    );
    abi::emit_call_label(ctx.emitter, "__rt_hash_set");
    // The helper can reallocate, so the fresh table is stored back through the row's offset.
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [sp, #16]");                       // reload the receiver
            ctx.emitter.instruction("ldr x10, [sp]");                           // reload the hash offset
            ctx.emitter.instruction("str x0, [x9, x10]");                       // publish the possibly-moved hash
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r11, QWORD PTR [rsp + 16]");           // reload the receiver
            ctx.emitter.instruction("mov r10, QWORD PTR [rsp]");                // reload the hash offset
            ctx.emitter.instruction("mov QWORD PTR [r11 + r10], rax");          // publish the possibly-moved hash
        }
    }
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    Ok(())
}

/// Loads `receiver->hash` into the first argument register from the stacked table row.
fn emit_table_row_hash_to_first_arg(ctx: &mut FunctionContext<'_>) {
    emit_table_row_hash_to_first_arg_at(ctx, 0);
}

/// Same, with `extra` bytes pushed above the row since it was stacked.
fn emit_table_row_hash_to_first_arg_at(ctx: &mut FunctionContext<'_>, extra: usize) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("ldr x9, [sp, #{}]", 16 + extra)); // reload the receiver
            ctx.emitter.instruction(&format!("ldr x10, [sp, #{}]", extra));     // reload the hash offset
            ctx.emitter.instruction("ldr x0, [x9, x10]");                       // pass the receiver's dynamic-property hash
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov r11, QWORD PTR [rsp + {}]", 16 + extra)); // reload the receiver
            ctx.emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", extra));      // reload the hash offset
            ctx.emitter.instruction("mov rdi, QWORD PTR [r11 + r10]");          // pass the receiver's dynamic-property hash
        }
    }
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
    /// Store into a refined untyped slot when the runtime value's tag matches its refined
    /// representation, and otherwise raise the refusal `Error` and store nothing.
    ///
    /// PHP's untyped property accepts any value, but elephc's refined slot (for example `int`
    /// from a `3` default) cannot hold a mismatched one without changing its physical
    /// representation. A matching tag is safe to store; a mismatch keeps the refusal instead of
    /// silently coercing, which would diverge from PHP's store-as-is rule.
    RefuseUnlessTagMatches { slot: PropertySlot, message: String },
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
                    emit_refined_untyped_tag_guarded_store(ctx, value, slot, &message, 16, 32, done_label)?;
                    return Ok(());
                }
                ensure_property_value_supported(ctx, slot, value, &value_ty, inst)?;
                super::readonly_guard::emit_readonly_slot_guard(
                    ctx,
                    slot,
                    false,
                    super::readonly_guard::GuardReceiver::Stack(16),
                    32,
                )?;
                let base_reg = abi::symbol_scratch_reg(ctx.emitter);
                abi::emit_load_temporary_stack_slot(ctx.emitter, base_reg, 16);
                emit_hook_aware_property_store(ctx, value, slot, base_reg)?;
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
                    inst.span.map_or(0, |span| span.line),
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
                    inst.span.map_or(0, |span| span.line),
                )?;
                abi::emit_jump(ctx.emitter, done_label);
                Ok(())
            }
            PropertyRuntimeAction::MagicGet | PropertyRuntimeAction::DynamicHashThenMagicGet { .. } => Err(CodegenIrError::invalid_module(
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
pub(super) fn mixed_property_write_candidate(
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
                MixedPropertyWriteAction::RefuseUnlessTagMatches { slot, message }
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
        PropertyRuntimeAction::MagicGet | PropertyRuntimeAction::DynamicHashThenMagicGet { .. } | PropertyRuntimeAction::MagicDeferred => return Ok(None),
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
    // A scalar or string slot copies its payload OUT of the boxed cell (the store unboxes and
    // guards the runtime tag), so a boxed value never replaces the slot's representation: only
    // container and object pointers are refused. `$o->v = 7` through a `use (&$o)` capture, where
    // `Box::$v = 1` is refined to `int`, is the ordinary shape this used to turn into a fatal.
    if matches!(
        slot.php_type.codegen_repr(),
        PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Str
    ) {
        return None;
    }
    Some(refined_untyped_refusal_message(slot))
}

/// The diagnostic raised when a boxed value cannot be stored in a refined untyped slot.
fn refined_untyped_refusal_message(slot: &PropertySlot) -> String {
    format!(
        "Unsupported dynamic property write: runtime Mixed value cannot be stored safely in the refined untyped property {}::${}",
        slot.class_name, slot.property
    )
}

/// Lets a boxed value into a refined untyped SCALAR slot only when its runtime tag is the slot's own.
///
/// The store copies the payload out of the box, which is exact for a matching tag and a silent
/// coercion for any other (`"x"` into an `int` slot became `0`). php would simply keep the string,
/// which this slot cannot represent, so a mismatching tag takes the same refusal a container slot
/// takes. `stacked_bytes` is the temporary stack the arm still holds, released before the raise.
fn emit_refined_scalar_slot_guard(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    slot: &PropertySlot,
    stacked_bytes: usize,
) -> Result<()> {
    if slot.is_declared {
        return Ok(());
    }
    let expected = match slot.php_type.codegen_repr() {
        PhpType::Int => 0,
        PhpType::Str => 1,
        PhpType::Float => 2,
        PhpType::Bool => 3,
        _ => return Ok(()),
    };
    if !matches!(
        ctx.value_ir_type(value)?,
        crate::ir::IrType::Heap(crate::ir::IrHeapKind::Mixed)
    ) {
        return Ok(());
    }
    let accepted = ctx.next_label("refined_scalar_slot_accepted");
    ctx.load_value_to_result(value)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    super::super::enums::emit_mixed_tag_branch(
        ctx,
        abi::int_result_reg(ctx.emitter),
        expected,
        &accepted,
    );
    abi::emit_release_temporary_stack(ctx.emitter, stacked_bytes);
    super::super::exceptions::emit_error(ctx, &refined_untyped_refusal_message(slot));
    ctx.emitter.label(&accepted);
    Ok(())
}

/// The runtime value tags a refined untyped slot can store as-is.
///
/// Only the shapes `load_property_store_value_to_result` copies OUT of the box are listed
/// (scalars, string, object, iterable). An array/hash or callable refined slot is left to the
/// unconditional refusal, because that store path would publish the box pointer instead of the
/// payload. A packed field or a reference cell has its own store path, so it is refused too.
fn refined_untyped_store_tags(slot: &PropertySlot) -> &'static [i64] {
    if slot.is_packed || slot.is_reference {
        return &[];
    }
    match slot.php_type.codegen_repr() {
        PhpType::Int => &[0],
        PhpType::Str => &[1],
        PhpType::Float => &[2],
        PhpType::Bool => &[3],
        PhpType::Object(_) => &[6],
        PhpType::TaggedScalar => &[0, 8],
        PhpType::Iterable => &[4, 5, 6],
        _ => &[],
    }
}

/// Branches to `store_label` when the runtime value's boxed tag matches the refined slot.
fn emit_refined_untyped_tag_guard(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    slot: &PropertySlot,
    store_label: &str,
) -> Result<()> {
    let tags = refined_untyped_store_tags(slot);
    if tags.is_empty() {
        return Ok(()); // no tag is safe: fall straight through to the refusal
    }
    ctx.load_value_to_result(value)?;
    let box_reg = abi::int_result_reg(ctx.emitter);
    let tag_reg = abi::secondary_scratch_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("ldr {}, [{}, #0]", tag_reg, box_reg)); // boxed value runtime tag
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov {}, QWORD PTR [{}]", tag_reg, box_reg)); // boxed value runtime tag
        }
    }
    // An object slot's reads use the refined class's property layout, so tag 6 alone is not
    // enough: the payload must be an instance of that class (or a subclass). Any other class
    // would be read back through the wrong offsets (issue #1319 review).
    if let PhpType::Object(class_name) = slot.php_type.codegen_repr() {
        return emit_refined_untyped_object_class_guard(ctx, box_reg, tag_reg, &class_name, store_label);
    }
    for tag in tags {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("cmp {}, #{}", tag_reg, tag)); // compare the runtime tag with a storable shape
            }
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("cmp {}, {}", tag_reg, tag));  // compare the runtime tag with a storable shape
            }
        }
        match ctx.emitter.target.arch {
            Arch::AArch64 => ctx.emitter.instruction(&format!("b.eq {}", store_label)), // take the store arm when the tag matches
            Arch::X86_64 => ctx.emitter.instruction(&format!("je {}", store_label)), // take the store arm when the tag matches
        }
    }
    Ok(())
}

/// Branches to `store_label` when the boxed value is an instance of the refined class.
///
/// The runtime class id is walked up `_class_parent_ids`, so a subclass is accepted while an
/// unrelated class falls through to the caller's refusal. `__rt_exception_matches` is the
/// shared object-to-class matcher (the same walk `instanceof` and `catch` use); its class-target
/// arm is exactly `is_a(object, class_id)`. A refined class with no emitted metadata leaves the
/// refusal in place rather than guessing.
fn emit_refined_untyped_object_class_guard(
    ctx: &mut FunctionContext<'_>,
    box_reg: &str,
    tag_reg: &str,
    class_name: &str,
    store_label: &str,
) -> Result<()> {
    let Some(class_id) = ctx
        .module
        .class_infos
        .get(class_name.trim_start_matches('\\'))
        .map(|info| info.class_id)
    else {
        return Ok(()); // no emitted class metadata to match: keep the refusal
    };
    let done = ctx.next_label("refined_untyped_object_refuse");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, #6", tag_reg));           // only an object payload can occupy an object slot
            ctx.emitter.instruction(&format!("b.ne {}", done));                 // a non-object value falls through to the refusal
            abi::emit_load_from_address(ctx.emitter, "x0", box_reg, 8);         // unbox the object pointer (boxed low word)
            abi::emit_load_int_immediate(ctx.emitter, "x1", class_id as i64);   // the refined class the slot's layout belongs to
            ctx.emitter.instruction("mov x2, #0");                              // kind 0 = class target, not an interface
            abi::emit_call_label(ctx.emitter, "__rt_exception_matches");        // is_a(object, refined class): walks parent ids
            ctx.emitter.instruction(&format!("cbnz x0, {}", store_label));      // the same class or a subclass can use this slot's layout
            ctx.emitter.label(&done);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {}, 6", tag_reg));            // only an object payload can occupy an object slot
            ctx.emitter.instruction(&format!("jne {}", done));                  // a non-object value falls through to the refusal
            abi::emit_load_from_address(ctx.emitter, "rdi", box_reg, 8);        // unbox the object pointer (boxed low word)
            abi::emit_load_int_immediate(ctx.emitter, "rsi", class_id as i64);  // the refined class the slot's layout belongs to
            ctx.emitter.instruction("xor edx, edx");                            // kind 0 = class target, not an interface
            abi::emit_call_label(ctx.emitter, "__rt_exception_matches");        // is_a(object, refined class): walks parent ids
            ctx.emitter.instruction("test eax, eax");                           // did the matcher accept the object's class?
            ctx.emitter.instruction(&format!("jne {}", store_label));           // the same class or a subclass can use this slot's layout
            ctx.emitter.label(&done);
        }
    }
    Ok(())
}

/// Stores a runtime-shaped value into a refined untyped slot when its tag matches, else refuses.
fn emit_refined_untyped_tag_guarded_store(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    slot: &PropertySlot,
    message: &str,
    base_offset: usize,
    stack_cleanup: usize,
    done_label: &str,
) -> Result<()> {
    if refined_untyped_store_tags(slot).is_empty() {
        abi::emit_release_temporary_stack(ctx.emitter, stack_cleanup);
        super::super::exceptions::emit_error(ctx, message);
        return Ok(());
    }
    let store_label = ctx.next_label("refined_untyped_mixed_store");
    emit_refined_untyped_tag_guard(ctx, value, slot, &store_label)?;
    abi::emit_release_temporary_stack(ctx.emitter, stack_cleanup);
    super::super::exceptions::emit_error(ctx, message);
    ctx.emitter.label(&store_label);
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, base_reg, base_offset);
    emit_hook_aware_property_store(ctx, value, slot, base_reg)?;
    abi::emit_release_temporary_stack(ctx.emitter, stack_cleanup);
    abi::emit_jump(ctx.emitter, done_label);
    Ok(())
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

/// For each arm of a literal-name write ladder, the index of the first arm with the same code.
///
/// A plain store to an untyped slot is the same at one offset on any class. Any other slot store
/// depends on the slot alone, so the subclasses that inherit it share its declaring class's arm
/// ([`write_arm_key`]); that is most of Symfony, whose properties are typed. A Symfony build
/// carried 177 MB of these arms, one per class declaring or inheriting a common name.
fn shared_named_set_arm_owners(
    ctx: &FunctionContext<'_>,
    candidates: &[MixedPropertyWriteCandidate],
) -> Vec<usize> {
    let no_names = crate::fast_hash::FastMap::default();
    let mut first_for_key: crate::fast_hash::FastMap<String, usize> = Default::default();
    candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| match &candidate.action {
            MixedPropertyWriteAction::Slot(slot) if store_cannot_refuse(ctx, slot) => {
                *first_for_key.entry(format!("plain|{}", slot.offset)).or_insert(index)
            }
            MixedPropertyWriteAction::Slot(_) => {
                *first_for_key.entry(write_arm_key(ctx, &no_names, candidate)).or_insert(index)
            }
            _ => index,
        })
        .collect()
}

/// Whether a write to `slot` is the same plain store on every class that has it at that offset.
fn store_cannot_refuse(ctx: &FunctionContext<'_>, slot: &PropertySlot) -> bool {
    !slot.is_declared
        && !slot.is_packed
        && !slot.is_reference
        && slot.php_type.codegen_repr() == PhpType::Mixed
        && !super::readonly_guard::slot_has_readonly_guard(ctx, slot)
        && ctx
            .module
            .class_infos
            .get(&slot.class_name)
            .is_some_and(|info| info.retained_abstract_setter(&slot.property).is_none())
}

/// Sorts `(class id, label)` pairs and merges consecutive ids that jump to the same label.
fn class_id_runs<'a>(
    arms: impl Iterator<Item = (u64, &'a str)>,
) -> Vec<(u64, u64, &'a str)> {
    let mut sorted = arms.collect::<Vec<_>>();
    sorted.sort_unstable_by_key(|(class_id, _)| *class_id);
    let mut runs: Vec<(u64, u64, &'a str)> = Vec::new();
    for (class_id, label) in sorted {
        match runs.last_mut() {
            Some((_, last, run_label)) if *last + 1 == class_id && *run_label == label => {
                *last = class_id;
            }
            _ => runs.push((class_id, class_id, label)),
        }
    }
    runs
}

/// Branches when the stacked receiver's class id lies in `first..=last`.
fn emit_branch_if_stacked_object_class_in_run(
    ctx: &mut FunctionContext<'_>,
    first: u64,
    last: u64,
    object_stack_offset: usize,
    matched_label: &str,
) {
    if first == last {
        emit_branch_if_stacked_object_class_matches(ctx, first, object_stack_offset, matched_label);
        return;
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", object_stack_offset);
            ctx.emitter.instruction("ldr x10, [x9]");                           // load the receiver's class id
            abi::emit_load_int_immediate(ctx.emitter, "x11", first as i64);
            ctx.emitter.instruction("sub x10, x10, x11");                       // make the run start at zero
            abi::emit_load_int_immediate(ctx.emitter, "x11", (last - first) as i64);
            ctx.emitter.instruction("cmp x10, x11");                            // unsigned: below the run wraps high
            ctx.emitter.instruction(&format!("b.ls {}", matched_label));        // the class is in the run
        }
        Arch::X86_64 => {
            let candidate_reg = abi::tertiary_scratch_reg(ctx.emitter);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r11", object_stack_offset);
            ctx.emitter.instruction("mov r10, QWORD PTR [r11]");                // load the receiver's class id
            abi::emit_load_int_immediate(ctx.emitter, candidate_reg, first as i64);
            ctx.emitter.instruction(&format!("sub r10, {}", candidate_reg));    // make the run start at zero
            abi::emit_load_int_immediate(ctx.emitter, candidate_reg, (last - first) as i64);
            ctx.emitter.instruction(&format!("cmp r10, {}", candidate_reg));    // unsigned: below the run wraps high
            ctx.emitter.instruction(&format!("jbe {}", matched_label));         // the class is in the run
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
    line: u32,
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
        line,
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
    line: u32,
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
        line,
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
    line: u32,
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
                line,
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
    let target = ctx.emitter.target;
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, object_reg, receiver_offset);
    abi::emit_load_from_address(
        ctx.emitter,
        abi::int_arg_reg_name(target, 0),
        object_reg,
        hash_offset,
    );
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(target, 1),
        name_offset,
    );
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(target, 2),
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
    let target = ctx.emitter.target;
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(target, 0),
        receiver_offset,
    );
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(target, 1),
        name_offset,
    );
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(target, 2),
        name_offset + 8,
    );
    abi::emit_temporary_stack_address(
        ctx.emitter,
        abi::int_arg_reg_name(target, 3),
        node_offset,
    );
    abi::emit_call_label(ctx.emitter, "__rt_magic_set_guard_push");
}

/// Removes the exact stack node this invocation published.
fn emit_magic_set_guard_pop(ctx: &mut FunctionContext<'_>, node_offset: usize) {
    abi::emit_temporary_stack_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 0),
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
        abi::int_arg_reg_name(ctx.emitter.target, 0),
        MAGIC_SET_GUARD_HANDLER_OFFSET + TRY_HANDLER_JMP_BUF_OFFSET,
    );
    ctx.emitter.save_exception_handler();
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
    if try_lower_outlined_mixed_prop_set(ctx, object, property_value, value, inst)? {
        return Ok(());
    }
    // Inside a scoped shared helper only the pairs this scope resolves differently are probed;
    // everything else is the global helper's answer.
    let delegate = outlined_helper_delegate(ctx, SET_PREFIX);
    let only = if delegate.is_some() {
        Some(scope_overrides_write(ctx, value, inst)?.ok_or_else(|| {
            CodegenIrError::invalid_module("scoped dynamic-write helper without overrides")
        })?)
    } else {
        None
    };
    ensure_runtime_dynamic_property_name(ctx, property_value, inst)?;
    let write_kind = PropertyAccessKind::RuntimeWrite;
    let candidates =
        declared_mixed_property_set_candidates(ctx, value, write_kind, inst, only.as_ref())?;
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
    let hash_arms = if delegate.is_some() { Vec::new() } else { mixed_class_hash_arms(ctx, "", &[])? };
    let hash_labels = hash_arms
        .iter()
        .map(|arm| {
            ctx.next_label(&format!(
                "mixed_dyn_prop_set_hash_{}",
                label_fragment(&arm.class_name)
            ))
        })
        .collect::<Vec<_>>();
    let magic_arms = if delegate.is_some() { Vec::new() } else { runtime_magic_set_arms(ctx)? };
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

    let class_names = ctx
        .module
        .class_infos
        .iter()
        .map(|(name, info)| (info.class_id, name.clone()))
        .collect::<crate::fast_hash::FastMap<_, _>>();
    let arm_entries = candidates
        .iter()
        .map(|candidate| {
            (candidate.class_id, candidate.property.as_str(), write_arm_key(ctx, &class_names, candidate))
        })
        .collect::<Vec<_>>();
    let groups = super::arm_groups::group_arms(ctx, &arm_entries);
    if std::env::var_os("ELEPHC_ARM_STATS").is_some() {
        eprintln!("ARMGROUPS candidates={} arms={}", candidates.len(), groups.len());
    }
    let no_match_label = ctx.next_label("mixed_dyn_prop_set_no_match");
    // A visibility refusal that composes its text from the receiver's class has no per-name code:
    // every such arm branches to ONE tail per visibility word.
    let refusal_tails = [
        ("private", ctx.next_label("mixed_dyn_prop_set_refuse_private")),
        ("protected", ctx.next_label("mixed_dyn_prop_set_refuse_protected")),
    ];
    let mut refusal_tail_used = [false; 2];
    for group in &groups {
        let target = match group.representative {
            Some(index) => match dynamic_refusal_visibility(&class_names, &candidates[index]) {
                Some(visibility) => {
                    let tail = refusal_tails.iter().position(|(word, _)| *word == visibility).unwrap_or(0);
                    refusal_tail_used[tail] = true;
                    &refusal_tails[tail].1
                }
                None => &match_labels[index],
            },
            None => &no_match_label,
        };
        emit_branch_if_mixed_dynamic_property_group_matches(ctx, group, target);
    }
    ctx.emitter.label(&no_match_label);
    for (arm, label) in magic_arms.iter().zip(magic_labels.iter()) {
        emit_branch_if_stacked_object_class_matches(ctx, arm.class_id, 16, label);
    }
    for (arm, label) in hash_arms.iter().zip(hash_labels.iter()) {
        emit_branch_if_stacked_object_class_matches(ctx, arm.class_id, 16, label);
    }
    if delegate.is_none() {
        emit_branch_if_stacked_object_is_stdclass(ctx, 16, &stdclass_label);
    }
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
            inst.span.map_or(0, |span| span.line),
        )?;
        abi::emit_jump(ctx.emitter, &done_label);
    }

    for (arm, label) in hash_arms.iter().zip(hash_labels.iter()) {
        ctx.emitter.label(label);
        // The arm matched this runtime class, so the offset and the class php names in its
        // creation notice are both its own. The store helper takes the runtime key from the
        // stacked block and releases that block itself.
        emit_dynamic_property_creation_deprecation(
            ctx,
            &arm.class_name,
            arm.hash_offset,
            16,
            0,
            inst.span.map_or(0, |span| span.line),
        )?;
        lower_runtime_allow_dynamic_prop_set(ctx, value, arm.hash_offset, 16, 0, 32)?;
        abi::emit_jump(ctx.emitter, &done_label);
    }

    let mut arm_needed = vec![false; candidates.len()];
    for group in &groups {
        if let Some(index) = group.representative {
            arm_needed[index] = dynamic_refusal_visibility(&class_names, &candidates[index]).is_none();
        }
    }
    for ((word, label), used) in refusal_tails.iter().zip(refusal_tail_used) {
        if used {
            ctx.emitter.label(label);
            super::closure_scope_gate::emit_scope_refusal(ctx, word);
        }
    }
    for ((candidate, label), needed) in candidates.iter().zip(match_labels.iter()).zip(arm_needed) {
        if !needed {
            continue;
        }
        ctx.emitter.label(label);
        match &candidate.action {
            MixedPropertyWriteAction::Refuse(message) => {
                abi::emit_release_temporary_stack(ctx.emitter, 32);
                super::super::exceptions::emit_error(ctx, message);
            }
            MixedPropertyWriteAction::RefuseUnlessTagMatches { slot, message } => {
                emit_refined_untyped_tag_guarded_store(ctx, value, slot, message, 16, 32, &done_label)?;
            }
            MixedPropertyWriteAction::Slot(slot) => {
                // The arm answers a whole subtree, so its diagnostics name the declaring class.
                let mut slot = slot.clone();
                slot.class_name = message_class_for_slot(ctx, &slot);
                let slot = &slot;
                super::readonly_guard::emit_readonly_slot_guard(
                    ctx,
                    slot,
                    false,
                    super::readonly_guard::GuardReceiver::Stack(16),
                    32,
                )?;
                emit_refined_scalar_slot_guard(ctx, value, slot, 32)?;
                let base_reg = abi::symbol_scratch_reg(ctx.emitter);
                abi::emit_load_temporary_stack_slot(ctx.emitter, base_reg, 16);
                emit_hook_aware_property_store(ctx, value, slot, base_reg)?;
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
                    inst.span.map_or(0, |span| span.line),
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
                    inst.span.map_or(0, |span| span.line),
                )?;
                abi::emit_jump(ctx.emitter, &done_label);
            }
        }
    }

    if delegate.is_none() {
        ctx.emitter.label(&stdclass_label);
        let value_ty = ctx.value_php_type(value)?.codegen_repr();
        emit_runtime_stdclass_set_for_stacked_name_after_eval_probe(ctx, value, &value_ty, 16, 0, 32)?;
        abi::emit_release_temporary_stack(ctx.emitter, 32);
        abi::emit_jump(ctx.emitter, &done_label);
    }

    ctx.emitter.label(&miss_label);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    if let Some(base_label) = &delegate {
        emit_delegate_call_to_global_helper(ctx, object, property_value, Some(value), base_label)?;
    }
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Collects Mixed receiver declared-property candidates that can accept this value.
pub(super) fn declared_mixed_property_set_candidates(
    ctx: &FunctionContext<'_>,
    value: ValueId,
    write_kind: PropertyAccessKind,
    inst: &Instruction,
    only: Option<&crate::fast_hash::FastSet<(String, String)>>,
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
            if only.is_some_and(|only| !only.contains(&(class_name.clone(), property.clone()))) {
                continue;
            }
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

/// The key under which a write arm may be merged with its parent class's arm.
///
/// A slot store depends on the slot alone, so every subclass that inherits the slot unchanged
/// shares it. The other actions name the RUNTIME class in their text or their hash offset, so they
/// key on the class id and never merge.
fn write_arm_key(
    ctx: &FunctionContext<'_>,
    class_names: &crate::fast_hash::FastMap<u64, String>,
    candidate: &MixedPropertyWriteCandidate,
) -> String {
    if let Some(visibility) = dynamic_refusal_visibility(class_names, candidate) {
        return format!("refuse-dynamic|{}|{}", visibility, candidate.property);
    }
    match &candidate.action {
        MixedPropertyWriteAction::Slot(slot) => format!(
            "slot{}|{}|{}|{:?}|{}|{}|{}|{}",
            super::readonly_guard::readonly_key(ctx, slot),
            message_class_for_slot(ctx, slot),
            slot.property,
            slot.php_type,
            slot.offset,
            slot.is_declared,
            slot.is_packed,
            slot.is_reference
        ),
        MixedPropertyWriteAction::Refuse(message) => format!("refuse|{}|{}", candidate.class_id, message),
        // Refuses unless the stored tag matches, otherwise writes the slot: it depends on both the
        // slot and the message, and the message names the runtime class, so it never merges.
        MixedPropertyWriteAction::RefuseUnlessTagMatches { slot, message } => format!(
            "refuse-unless-tag|{}|{}|{}|{}",
            candidate.class_id, slot.offset, slot.property, message
        ),
        MixedPropertyWriteAction::DynamicHash { .. } => format!("hash|{}", candidate.class_id),
        MixedPropertyWriteAction::MagicSetRecursiveRefusal { .. } => format!("magic|{}", candidate.class_id),
    }
}

/// The class php names in a typed-property `TypeError`: the class that DECLARES the property.
///
/// php prints `Cannot assign string to property A::$n of type int` for a `B extends A` receiver;
/// the slot carries the receiver's own name, which made every subclass arm differ from its
/// parent's. A name with more than one physical slot (a shadowed private) keeps the slot's class,
/// since the name-keyed declaring-class table cannot say which slot the arm stores into.
pub(super) fn message_class_for_slot(ctx: &FunctionContext<'_>, slot: &PropertySlot) -> String {
    let Some(info) = ctx.module.class_infos.get(&slot.class_name) else {
        return slot.class_name.clone();
    };
    if info.properties.iter().filter(|(name, _)| *name == slot.property).count() != 1 {
        return slot.class_name.clone();
    }
    info.property_declaring_classes
        .get(&slot.property)
        .cloned()
        .unwrap_or_else(|| slot.class_name.clone())
}

/// The visibility word of a refusal whose text is exactly php's
/// `Cannot access <visibility> property <RuntimeClass>::$<name>` for this arm's own class.
///
/// Such an arm can be emitted by composing the message from the RECEIVER's class name at run time
/// (`emit_scope_refusal`), so it no longer names its class and a whole subtree shares it.
fn dynamic_refusal_visibility(
    class_names: &crate::fast_hash::FastMap<u64, String>,
    candidate: &MixedPropertyWriteCandidate,
) -> Option<&'static str> {
    let MixedPropertyWriteAction::Refuse(message) = &candidate.action else {
        return None;
    };
    refusal_message_visibility(class_names, candidate.class_id, &candidate.property, message)
}

/// The visibility word when `message` is php's refusal for exactly this class and property.
pub(super) fn refusal_message_visibility(
    class_names: &crate::fast_hash::FastMap<u64, String>,
    class_id: u64,
    property: &str,
    message: &str,
) -> Option<&'static str> {
    let class_name = class_names.get(&class_id)?;
    ["private", "protected"].into_iter().find(|visibility| {
        message == format!("Cannot access {} property {}::${}", visibility, class_name, property)
    })
}

/// The key under which a removal arm may share its body with another class's arm.
///
/// A slot removal depends only on the slot's offset and storage type, a hash removal only on the
/// hash offset, and a visibility refusal composed from the receiver's class on the visibility
/// word, so every class whose arm has the same key shares one body.
fn unset_arm_key(
    ctx: &FunctionContext<'_>,
    class_names: &crate::fast_hash::FastMap<u64, String>,
    candidate: &MixedPropertyUnsetCandidate,
) -> String {
    match &candidate.action {
        PropertyRuntimeAction::Slot(slot) => {
            if unset_unsupported_slot_reason(ctx, slot).is_some() {
                return format!("unsupported|{}|{}", candidate.class_id, slot.property);
            }
            format!(
                "slot{}|{:?}|{:?}|{}|{}|{}|{}|{}",
                super::readonly_guard::readonly_key(ctx, slot),
                slot.property,
                slot.release_type(),
                slot.php_type,
                slot.offset,
                slot.is_declared,
                slot.is_packed,
                slot.is_reference
            )
        }
        PropertyRuntimeAction::Refuse { message } => {
            let property = candidate.property.as_deref().unwrap_or("");
            match refusal_message_visibility(class_names, candidate.class_id, property, message) {
                Some(visibility) => format!("refuse-dynamic|{}", visibility),
                None => format!("refuse|{}|{}", candidate.class_id, message),
            }
        }
        PropertyRuntimeAction::DynamicHash { hash_offset, .. } => format!("hash|{}", hash_offset),
        PropertyRuntimeAction::DynamicMissing { .. }
        | PropertyRuntimeAction::MagicDeferred
        | PropertyRuntimeAction::MagicGet | PropertyRuntimeAction::DynamicHashThenMagicGet { .. } => "noop".to_string(),
    }
}

/// Branches to `matched_label` when the stacked receiver's class falls under the group's class and
/// the runtime name is the group's property.
///
/// A leaf class is the plain exact test. A class with subclasses tests the NAME first and then asks
/// the shared matcher whether the receiver extends the class; the arm sits behind every deeper arm
/// of the same name, so the first match is the receiver's nearest top ancestor.
pub(super) fn emit_branch_if_mixed_dynamic_property_group_matches(
    ctx: &mut FunctionContext<'_>,
    group: &super::arm_groups::ArmGroup,
    matched_label: &str,
) {
    if !group.subtree {
        emit_branch_if_mixed_dynamic_property_candidate_matches(
            ctx,
            group.class_id,
            &group.property,
            matched_label,
        );
        return;
    }
    let next_label = ctx.next_label("mixed_dyn_prop_set_subtree_next");
    emit_branch_if_dynamic_name_differs(ctx, &group.property, &next_label);
    let object_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, object_arg, 16);
    super::instanceof_helpers::emit_match_call(ctx, group.class_id, 0, "__rt_exception_matches");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbnz x0, {}", matched_label));    // the receiver extends the group's class
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // test the subtree matcher result
            ctx.emitter.instruction(&format!("jne {}", matched_label));         // the receiver extends the group's class
        }
    }
    ctx.emitter.label(&next_label);
}

/// Branches to `differs_label` when the stacked runtime name is not `property`.
fn emit_branch_if_dynamic_name_differs(ctx: &mut FunctionContext<'_>, property: &str, differs_label: &str) {
    let (label, len) = ctx.data.add_string(property.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", 0);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x2", 8);
            abi::emit_symbol_address(ctx.emitter, "x3", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x4", len as i64);
            ctx.emitter.instruction("bl __rt_str_eq");                          // compare the runtime property name against this group's property
            ctx.emitter.instruction(&format!("cbz x0, {}", differs_label));     // another name: try the next arm
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", 0);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", 8);
            abi::emit_symbol_address(ctx.emitter, "rdx", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rcx", len as i64);
            ctx.emitter.instruction("call __rt_str_eq");                        // compare the runtime property name against this group's property
            ctx.emitter.instruction("test rax, rax");                           // check whether the runtime string comparison matched
            ctx.emitter.instruction(&format!("je {}", differs_label));          // another name: try the next arm
        }
    }
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
    let (arg0, arg1, arg2, arg3, arg4, arg5) = (
        abi::int_arg_reg_name(target, 0),
        abi::int_arg_reg_name(target, 1),
        abi::int_arg_reg_name(target, 2),
        abi::int_arg_reg_name(target, 3),
        abi::int_arg_reg_name(target, 4),
        abi::int_arg_reg_name(target, 5),
    );
    // php decides visibility from the WRITING function's lexical class; a free function has none.
    let scope = ctx.lexical_class().unwrap_or_default().to_owned();
    abi::emit_load_temporary_stack_slot(ctx.emitter, arg0, object_stack_offset + 32);
    abi::emit_load_temporary_stack_slot(ctx.emitter, arg1, name_stack_offset + 32);
    abi::emit_load_temporary_stack_slot(ctx.emitter, arg2, name_stack_offset + 40);
    // One block carries both: word 0 is the Throwable slot, word 2 the value cell pushed above.
    abi::emit_temporary_stack_address(ctx.emitter, arg3, 0);
    // A closure `Closure::bind` rebound at run time writes under its BOUND scope instead.
    crate::codegen::lower_inst::builtins::eval::emit_effective_scope_name(ctx, &scope, arg4, arg5)?;
    let symbol = target.extern_symbol("__elephc_eval_dynamic_object_property_set");
    abi::emit_call_label(ctx.emitter, &symbol);
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
        if !initialize_owned_property_reference(ctx, &slot, base_reg, slot.is_declared) {
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
    if let Some(reason) = unset_unsupported_slot_reason(ctx, &slot) {
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
            if let Some(reason) = unset_unsupported_slot_reason(ctx, slot) {
                return Err(CodegenIrError::unsupported(format!(
                    "unset() of {} {}::${}",
                    reason, slot.class_name, slot.property
                )));
            }
            super::readonly_guard::emit_readonly_slot_guard(
                ctx,
                slot,
                true,
                super::readonly_guard::GuardReceiver::Value(object),
                0,
            )?;
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
        PropertyRuntimeAction::MagicDeferred | PropertyRuntimeAction::MagicGet | PropertyRuntimeAction::DynamicHashThenMagicGet { .. } => Ok(()),
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
    // A named property of a BOXED receiver (`unset($record->uuid)` over an untyped closure
    // parameter, laravel's `DatabaseUuidFailedJobProvider::all()`) has no static class for the
    // by-name path to resolve; the runtime-class ladder below takes the literal as its name.
    let boxed_receiver = matches!(
        ctx.value_php_type(object)?.codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    );
    if !boxed_receiver {
        if let Some(property) = const_string_operand(ctx, property_value)? {
            let property = property.to_string();
            return lower_named_prop_unset(ctx, object, &property, inst);
        }
    }
    match ctx.value_php_type(object)?.codegen_repr() {
        // A receiver typed only `object` names no class: dispatch on its runtime class id, as a
        // boxed receiver does.
        PhpType::Object(class_name) if class_name.trim_start_matches('\\').is_empty() => {
            lower_runtime_mixed_prop_unset(ctx, object, property_value, inst, true)
        }
        PhpType::Object(class_name) => {
            lower_runtime_object_prop_unset(ctx, object, property_value, &class_name, inst)
        }
        PhpType::Mixed | PhpType::Union(_) => {
            lower_runtime_mixed_prop_unset(ctx, object, property_value, inst, false)
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
    let target = ctx.emitter.target;
    let object_reg = abi::symbol_scratch_reg(ctx.emitter).to_string();
    abi::emit_load_temporary_stack_slot(ctx.emitter, &object_reg, receiver_offset);
    abi::emit_load_from_address(
        ctx.emitter,
        abi::int_arg_reg_name(target, 0),
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
        abi::int_arg_reg_name(target, 0),
        abi::int_result_reg(ctx.emitter),
    );
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(target, 1), name_offset);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(target, 2),
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
    generic_object: bool,
) -> Result<()> {
    if !generic_object && try_lower_outlined_mixed_prop_unset(ctx, object, property_value, inst)? {
        return Ok(());
    }
    let delegate = outlined_helper_delegate(ctx, UNSET_PREFIX);
    let only = if delegate.is_some() {
        Some(scope_overrides_unset(ctx, inst)?.ok_or_else(|| {
            CodegenIrError::invalid_module("scoped dynamic-unset helper without overrides")
        })?)
    } else {
        None
    };
    ensure_runtime_dynamic_property_name(ctx, property_value, inst)?;
    let candidates = declared_mixed_property_unset_candidates(ctx, inst, only.as_ref())?;
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
    // Arms with the same key run the same code, so they share the first one's label and body.
    let class_names = ctx
        .module
        .class_infos
        .iter()
        .map(|(name, info)| (info.class_id, name.clone()))
        .collect::<crate::fast_hash::FastMap<_, _>>();
    let mut first_for_key: crate::fast_hash::FastMap<String, usize> = Default::default();
    let mut arm_owner = Vec::with_capacity(candidates.len());
    for (index, candidate) in candidates.iter().enumerate() {
        let key = if std::env::var_os("ELEPHC_NO_ARM_GROUPS").is_some() {
            format!("unique|{index}")
        } else {
            unset_arm_key(ctx, &class_names, candidate)
        };
        arm_owner.push(*first_for_key.entry(key).or_insert(index));
    }
    let match_labels = arm_owner.iter().map(|owner| match_labels[*owner].clone()).collect::<Vec<_>>();

    let result_reg = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(object, result_reg)?;
    if generic_object {
        // An `object`-typed receiver is already the payload; only a null one is skipped.
        abi::emit_branch_if_int_result_zero(ctx.emitter, &done_label);
        abi::emit_push_reg(ctx.emitter, result_reg);
    } else {
        abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
        emit_branch_if_mixed_unboxed_not_object(ctx, &done_label);
        push_mixed_unboxed_object_payload(ctx);
    }
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
    if delegate.is_none() {
        emit_branch_if_stacked_object_is_stdclass(ctx, 16, &stdclass_label);
    }
    abi::emit_jump(ctx.emitter, &miss_label);

    for (index, (candidate, label)) in candidates.iter().zip(match_labels.iter()).enumerate() {
        if arm_owner[index] != index {
            continue;
        }
        ctx.emitter.label(label);
        if let PropertyRuntimeAction::Refuse { message } = &candidate.action {
            let property = candidate.property.as_deref().unwrap_or("");
            if let Some(visibility) =
                refusal_message_visibility(&class_names, candidate.class_id, property, message)
            {
                // Composes the text from the receiver's own class name, releasing the block.
                super::closure_scope_gate::emit_scope_refusal(ctx, visibility);
                continue;
            }
        }
        emit_mixed_runtime_name_unset_arm(ctx, candidate, &done_label)?;
    }

    if delegate.is_none() {
        ctx.emitter.label(&stdclass_label);
        // stdClass keeps every property in the hash the header points at, which is the layout of a
        // class with no declared slots at all.
        lower_runtime_stacked_prop_unset(ctx, dynamic_property_hash_offset(0), 16, 0, 32)?;
        abi::emit_jump(ctx.emitter, &done_label);
    }

    ctx.emitter.label(&miss_label);
    // A runtime class this module never enumerated cannot have a hash this ladder can address,
    // and php's `unset()` of a property that was never created removes nothing.
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    if let Some(base_label) = &delegate {
        emit_delegate_call_to_global_helper(ctx, object, property_value, None, base_label)?;
    }
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
            // The name is only known at run time, so this arm is one of every class declaring
            // SOME property, and the checker could not widen a slot it never saw named
            // (`property_unset_destinations`). A slot that cannot hold the removed state raises
            // when this arm actually runs instead of refusing the whole file at compile time:
            // Symfony's DeepClone hydrators `unset($object->$name)` only typed properties of the
            // class they hydrate, yet the refusal on `ClassNotFoundError::$code` handed DeepClone
            // and everything depending on it to the interpreter.
            if let Some(reason) = unset_unsupported_slot_reason(ctx, slot) {
                let message = format!(
                    "Cannot unset {} {}::${} in compiled code",
                    reason, slot.class_name, slot.property
                );
                abi::emit_release_temporary_stack(ctx.emitter, 32);
                super::super::exceptions::emit_error(ctx, &message);
                return Ok(());
            }
            super::readonly_guard::emit_readonly_slot_guard(
                ctx,
                slot,
                true,
                super::readonly_guard::GuardReceiver::Stack(16),
                32,
            )?;
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
        | PropertyRuntimeAction::MagicGet | PropertyRuntimeAction::DynamicHashThenMagicGet { .. } => {
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
    // A by-reference slot holds a reference CELL that its alias still shares. `unset()` breaks
    // the reference -- the alias keeps its value -- and leaves the property uninitialized: drop
    // this slot's share of the cell (exactly as rebinding it does) and give the slot a fresh
    // owned cell marked uninitialized, so a later write lands in the object, not in the alias.
    if slot.is_reference {
        abi::emit_push_reg(ctx.emitter, base_reg);
        abi::emit_load_from_address(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            base_reg,
            slot.offset,
        );
        abi::emit_release_local_ref_cell(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            &slot.php_type,
        );
        abi::emit_pop_reg(ctx.emitter, base_reg);
        super::allocation_clone::emit_owned_reference_property_cell(
            ctx,
            base_reg,
            slot.offset,
            &slot.php_type,
            true,
        );
        abi::emit_store_zero_to_address(ctx.emitter, base_reg, slot.offset + 8);
        return;
    }
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
    only: Option<&crate::fast_hash::FastSet<(String, String)>>,
) -> Result<Vec<MixedPropertyUnsetCandidate>> {
    let mut candidates = Vec::new();
    for (class_name, class_info) in &ctx.module.class_infos {
        if is_builtin_stdclass(class_name) {
            continue;
        }
        for (property, _) in &class_info.properties {
            if only.is_some_and(|only| !only.contains(&(class_name.clone(), property.clone()))) {
                continue;
            }
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
        if only.is_some() {
            continue;
        }
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

/// Branches to the label of the candidate whose class id and property name match the stacked
/// receiver and runtime name, and falls through when none does.
///
/// The candidates span every declared property of the whole program: thousands for a framework.
/// Checking them one after another made each write through `ReflectionProperty::setRawValue()`
/// walk half of them on average, and Symfony's `Request` sets its properties that way on every
/// request. The receiver's class is chosen first, by a binary search over the candidates' class
/// ids, and names are compared only against that class's own candidates, in their given order.
pub(super) fn emit_mixed_dynamic_property_candidate_dispatch(
    ctx: &mut FunctionContext<'_>,
    candidates: &[MixedPropertyCandidate],
    matched_labels: &[String],
) {
    let mut classes: std::collections::BTreeMap<u64, Vec<usize>> = std::collections::BTreeMap::new();
    for (index, candidate) in candidates.iter().enumerate() {
        classes.entry(candidate.class_id).or_default().push(index);
    }
    if classes.is_empty() {
        return;
    }
    let classes = classes.into_iter().collect::<Vec<_>>();
    let no_match_label = ctx.next_label("mixed_dyn_prop_no_match");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", 16);
            ctx.emitter.instruction("ldr x10, [x9]");                           // x10 = receiver class id, live through the class search
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r11", 16);
            ctx.emitter.instruction("mov r10, QWORD PTR [r11]");                // r10 = receiver class id, live through the class search
        }
    }
    emit_candidate_class_search(ctx, &classes, candidates, matched_labels, &no_match_label);
    ctx.emitter.label(&no_match_label);
}

/// Emits the binary search over `classes` (sorted by class id) for the receiver class id held in
/// x10/r10. Nothing in the search calls out, so the id stays in its register until a class is
/// found. Small ranges are compared one class at a time.
fn emit_candidate_class_search(
    ctx: &mut FunctionContext<'_>,
    classes: &[(u64, Vec<usize>)],
    candidates: &[MixedPropertyCandidate],
    matched_labels: &[String],
    no_match_label: &str,
) {
    if classes.len() <= 3 {
        for (class_id, members) in classes {
            let next_label = ctx.next_label("mixed_dyn_prop_class_next");
            emit_compare_receiver_class_id(ctx, *class_id);
            match ctx.emitter.target.arch {
                Arch::AArch64 => ctx.emitter.instruction(&format!("b.ne {}", next_label)), // try the next class
                Arch::X86_64 => ctx.emitter.instruction(&format!("jne {}", next_label)),   // try the next class
            }
            emit_candidate_class_names(ctx, members, candidates, matched_labels, no_match_label);
            ctx.emitter.label(&next_label);
        }
        abi::emit_jump(ctx.emitter, no_match_label);
        return;
    }
    let middle = classes.len() / 2;
    let (class_id, members) = &classes[middle];
    let lower_label = ctx.next_label("mixed_dyn_prop_class_lower");
    let upper_label = ctx.next_label("mixed_dyn_prop_class_upper");
    emit_compare_receiver_class_id(ctx, *class_id);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("b.lo {}", lower_label));          // the receiver's class id sorts below this class
            ctx.emitter.instruction(&format!("b.hi {}", upper_label));          // the receiver's class id sorts above this class
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("jb {}", lower_label));            // the receiver's class id sorts below this class
            ctx.emitter.instruction(&format!("ja {}", upper_label));            // the receiver's class id sorts above this class
        }
    }
    emit_candidate_class_names(ctx, members, candidates, matched_labels, no_match_label);
    ctx.emitter.label(&lower_label);
    emit_candidate_class_search(ctx, &classes[..middle], candidates, matched_labels, no_match_label);
    ctx.emitter.label(&upper_label);
    emit_candidate_class_search(ctx, &classes[middle + 1..], candidates, matched_labels, no_match_label);
}

/// Compares the receiver class id held in x10/r10 with `class_id`.
fn emit_compare_receiver_class_id(ctx: &mut FunctionContext<'_>, class_id: u64) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x11", class_id as i64);
            ctx.emitter.instruction("cmp x10, x11");                            // compare the receiver class id with this class
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "r12", class_id as i64);
            ctx.emitter.instruction("cmp r10, r12");                            // compare the receiver class id with this class
        }
    }
}

/// Compares the runtime name with each candidate of the receiver's class, then gives up: no
/// other class's candidate can match a receiver of this class.
fn emit_candidate_class_names(
    ctx: &mut FunctionContext<'_>,
    members: &[usize],
    candidates: &[MixedPropertyCandidate],
    matched_labels: &[String],
    no_match_label: &str,
) {
    for &index in members {
        emit_branch_if_dynamic_name_matches(ctx, &candidates[index].slot.property, &matched_labels[index]);
    }
    abi::emit_jump(ctx.emitter, no_match_label);
}

/// Reports whether a receiver names a class the compiled module does not contain.
///
/// A named class missing from `class_infos` has no layout to resolve, so every slot-based path
/// below would refuse it. It is NOT a broken program: PHP resolves a parameter's declared class
/// only when an operation needs its metadata, so a class that nothing constructs statically is
/// absent from the closed world while the code around it still compiles.
///
/// `stdClass` and the empty "some object" spelling are handled by their own paths above and are
/// deliberately not reported here.
fn object_class_is_absent_from_module(
    ctx: &FunctionContext<'_>,
    object: ValueId,
) -> Result<bool> {
    let PhpType::Object(class_name) = ctx.value_php_type(object)?.codegen_repr() else {
        return Ok(false);
    };
    let normalized = class_name.trim_start_matches('\\');
    if normalized.is_empty() {
        return Ok(false);
    }
    Ok(!ctx.module.class_infos.contains_key(normalized))
}
