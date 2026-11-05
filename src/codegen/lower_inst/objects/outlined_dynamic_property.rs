//! Purpose:
//! Emits the runtime-name property ladders ONCE per module per key and calls them from every
//! site, instead of re-emitting a class-id search, a name comparison per declared property and
//! one load/materialize arm per declared property at each `$obj->{$name}`.
//!
//! Called from:
//! - `lower_runtime_dynamic_mixed_prop_get()` in `dynamic_property_read_entry`, for a boxed
//!   `Mixed`/`Union` receiver whose result is a boxed `Mixed`.
//! - `crate::codegen::block_emit` through `emit_pending_property_guard_helpers()`, which writes
//!   the helper bodies right behind the function that first asked for them.
//!
//! Key details:
//! - Measured on the compiled Laravel `--web` app, ONE `dynamic_prop_get` site on an untyped
//!   receiver was ~245 000 instructions: the ladder spans every declared property of the whole
//!   program. `ReadsClassAttributes::getAttributeValue` has five of them and is copied into
//!   eight classes.
//! - Helper key: the fetch mode (value read versus silent probe) and the LEXICAL scope class.
//!   Those are the only inputs the ladder's arms depend on: which `(class, name)` pairs are
//!   visible, refused, scope-dynamic or magic is a function of the module (constant), the scope
//!   class and the mode. The eval-bridge flag is a module fact too. Receiver, name and the
//!   eval-retry flag are RUNTIME arguments.
//! - Calling convention. In: boxed `Mixed` receiver in the first integer argument register, the
//!   name's pointer and length in the second and third, and in the fourth a flag that is `1`
//!   when the caller has already given the eval owner its chance. Out: the owned boxed `Mixed`
//!   answer in the integer result register, or `0` when (and only when) the flag was `0` and the
//!   receiver may be owned by an eval context. The site then runs the eval-owner probe -- it
//!   needs the site's value operands -- and, when the probe declines, calls again with the flag
//!   set. A non-object receiver never needs the probe, so it is answered right away.
//! - Everything site-specific stays at the site: loading the operands, the eval-owner probe and
//!   storing the result. The helper restricts itself to results of `Mixed` shape (the dominant
//!   case: an untyped receiver reads untyped data); any other result shape keeps the inline
//!   ladder, whose arms materialize into the site's own representation.
//! - A body with a runtime closure scope (`CLOSURE_SCOPE_LOCAL`) resolves names as each
//!   property's declaring class would and relies on the scope gate at the site. The module-wide
//!   read helper's frame holds no such local and could not tell, so such a body's reads, writes
//!   and removals go through a synthetic helper taking the hidden scope capture as one more
//!   parameter of the same name: the lowering inside resolves and gates exactly as the site
//!   would. The DeepClone polyfill's hydrators (`Closure::bind`-scoped `$o->$name = $v`) were
//!   ~3 M instructions on a small Symfony probe before this, one whole-program ladder per site;
//!   a read was 830 000 lines per site in the Symfony Demo's `OptionsResolverIntrospector`.
//! - Writes and removals (`$mixed->{$name} = $boxed`, `unset($mixed->{$name})`) are shared the same
//!   way but are emitted as ordinary synthetic EIR functions holding the one instruction, taking the
//!   receiver, the name and (for a write) the boxed value as parameters in that order. That runs the
//!   existing lowering -- frame, parameter spills, exception cleanup -- unchanged, so the ladder
//!   inside is the code the inline sites used. A write helper receives the value as a BORROWED
//!   parameter, so it retains where the inline store adopted an owning temporary; the site releases
//!   that temporary after the call (see `try_lower_outlined_mixed_prop_set`).
//! - Scope. A body's lexical scope changes the arm of only the `(class, name)` pairs it resolves
//!   differently from the global scope (its own private and protected members and those of its
//!   relatives), so a scope whose set of such pairs is empty shares the global helper, and any other
//!   scope gets a thin helper holding just those pairs that hands every other receiver or name to
//!   the global helper with the same operands (`scope_overrides_with`). Without this a class-heavy
//!   program got one full-size helper per class, which is what the inline ladders already were.
//! - `ELEPHC_NO_OUTLINED_DYNAMIC_PROPS=1` keeps the inline ladders, for size and behaviour comparisons.
//! - Not outlined: results that are not a boxed `Mixed`, receivers typed as a class or `object`
//!   (their ladder is bounded by the class subtree), writes whose value is not already a boxed
//!   `Mixed`, `DynamicPropRefCell`, and bodies with a runtime closure scope.
//! - Helper bodies are emitted behind the function that requested them, bracketed with the
//!   keyed-helper markers the parallel codegen merge deduplicates on.

use super::*;
use crate::codegen::context::{helper_marker_scope, HELPER_MARKER_CLOSE, HELPER_MARKER_OPEN};
use crate::codegen::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen::shared_helper::emit_shared_helper_in_scope;
use crate::codegen::shared_state::{DynamicPropHelperKind, SharedCodegenState};
use crate::ir::{Effects, IrHeapKind, IrType, Module, Ownership};

/// Readable part of the read helper's entry symbol.
const GET_PREFIX: &str = "dyn_prop_get";
/// Readable part of the write helper's entry symbol.
pub(super) const SET_PREFIX: &str = "dyn_prop_set";
/// Readable part of the removal helper's entry symbol.
pub(super) const UNSET_PREFIX: &str = "dyn_prop_unset";
/// Bytes of the outer frame that parks the eval-retry flag.
const FLAG_FRAME_BYTES: usize = 16;
/// Bytes of the frame holding the stacked receiver (offset 16) and runtime name (offset 0).
const LADDER_FRAME_BYTES: usize = 32;

/// Environment switch that keeps the old inline ladders, for size comparisons.
fn outlining_disabled() -> bool {
    std::env::var_os("ELEPHC_NO_OUTLINED_DYNAMIC_PROPS").is_some()
}

/// The cache key of the read helper for one fetch mode and scope.
fn get_helper_key(mode: PropertyFetchMode, scope: Option<&str>) -> String {
    format!("{}:{:?}:{}", GET_PREFIX, mode, scope.unwrap_or(""))
}

/// Lowers a runtime-name read from a boxed `Mixed` receiver through the shared helper.
///
/// Returns `false` without emitting anything when this site must keep the inline ladder.
pub(super) fn try_lower_outlined_mixed_prop_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    property_value: ValueId,
) -> Result<bool> {
    if outlining_disabled() || inst.result_php_type.codegen_repr() != PhpType::Mixed {
        return Ok(false);
    }
    if let Some(scope_slot) = ctx.local_slot_by_name(crate::names::CLOSURE_SCOPE_LOCAL) {
        if inside_function_helper(ctx, GET_PREFIX)
            || ctx.value_php_type(object)?.codegen_repr() != PhpType::Mixed
            || ctx.value_php_type(property_value)?.codegen_repr() != PhpType::Str
        {
            return Ok(false);
        }
        let mode = property_fetch_mode(inst);
        let key = format!("{}:runtime-scope:{:?}", GET_PREFIX, mode);
        let label = ctx.keyed_global_label(&key, GET_PREFIX);
        ctx.shared.request_dynamic_prop_helper(
            &key,
            &label,
            DynamicPropHelperKind::MixedGetRuntimeScope(mode),
            None,
        );
        emit_runtime_scope_helper_call(ctx, object, property_value, None, scope_slot, &label)?;
        store_if_result(ctx, inst)?;
        return Ok(true);
    }
    ensure_runtime_dynamic_property_name(ctx, property_value, inst)?;
    ensure_dynamic_property_miss_supported(inst)?;
    let mode = property_fetch_mode(inst);
    // The scope only matters through the (class, name) pairs it resolves differently from the
    // global scope. A scope with none shares the global helper; one with a few gets a thin helper
    // that answers those and delegates the rest to the global one.
    let Some(overrides) = scope_overrides(ctx, mode, inst)? else {
        return Ok(false);
    };
    let scope = if overrides.is_empty() {
        None
    } else {
        ctx.function
            .lexical_class
            .as_deref()
            .map(|class| class.trim_start_matches('\\').to_string())
    };
    let key = get_helper_key(mode, scope.as_deref());
    let label = ctx.keyed_global_label(&key, GET_PREFIX);
    ctx.shared.request_dynamic_prop_helper(
        &key,
        &label,
        DynamicPropHelperKind::MixedGet(mode),
        scope.as_deref(),
    );

    let eval_fallback = super::mixed_property_reads::eval_owner_fallback_available(ctx);
    emit_get_call(ctx, object, property_value, &label, false)?;
    if eval_fallback {
        let done_label = ctx.next_label("outlined_dyn_prop_get_done");
        let native_label = ctx.next_label("outlined_dyn_prop_get_native");
        abi::emit_branch_if_int_result_nonzero(ctx.emitter, &done_label);
        builtins::lower_eval_owner_mixed_dynamic_property_get(
            ctx,
            object,
            property_value,
            &native_label,
            &done_label,
        )?;
        ctx.emitter.label(&native_label);
        emit_get_call(ctx, object, property_value, &label, true)?;
        ctx.emitter.label(&done_label);
    }
    store_if_result(ctx, inst)?;
    Ok(true)
}

/// The `(class, name)` pairs whose arm under the body's lexical scope differs from the one under
/// the global scope, or `None` when this scope cannot be expressed that way.
///
/// The arm of a pair is a function of how php resolves the name (visible, scope-private,
/// dynamic, inaccessible) plus module facts, so pairs that resolve alike under both scopes keep
/// the global helper's arm. `arm_exists` says whether the ladder has an arm for a pair under the
/// scope: a pair whose scoped arm drops out of the ladder (a silent probe of a name the scope
/// cannot reach) would need an explicit "answer null" entry, so it opts out, as does a body that
/// resolves names as `ReflectionProperty` does.
fn scope_overrides_with(
    ctx: &FunctionContext<'_>,
    readonly_scope: bool,
    arm_exists: impl Fn(&FunctionContext<'_>, &str, &str) -> bool,
) -> Option<crate::fast_hash::FastSet<(String, String)>> {
    let mut pairs = crate::fast_hash::FastSet::default();
    let Some(lexical) = ctx.function.lexical_class.as_deref() else {
        return Some(pairs);
    };
    if lexical.trim_start_matches('\\').eq_ignore_ascii_case("ReflectionProperty") {
        return None;
    }
    for (class_name, class_info) in &ctx.module.class_infos {
        if crate::types::checker::builtin_stdclass::is_stdclass(class_name) {
            continue;
        }
        for (property, _) in &class_info.properties {
            let scoped = resolve_property_name_in_current_scope(ctx, class_name, property);
            let global = crate::types::resolve_property_name(
                &ctx.module.class_infos,
                class_name,
                property,
                None,
            );
            // A readonly property this scope may initialize is answered differently from the
            // global scope (which refuses to initialize it), even though the name resolves alike.
            let readonly_in_scope = readonly_scope
                && super::readonly_guard::scope_initializes_readonly_pair(ctx, class_name, property);
            if scoped == global && !readonly_in_scope {
                continue;
            }
            if !arm_exists(ctx, class_name, property) {
                return None;
            }
            pairs.insert((class_name.clone(), property.clone()));
        }
    }
    Some(pairs)
}

/// [`scope_overrides_with`] for a runtime-name read.
fn scope_overrides(
    ctx: &FunctionContext<'_>,
    mode: PropertyFetchMode,
    inst: &Instruction,
) -> Result<Option<crate::fast_hash::FastSet<(String, String)>>> {
    Ok(scope_overrides_with(ctx, false, |ctx, class_name, property| {
        matches!(resolve_property_read_arm(ctx, class_name, property, mode, inst), Ok(Some(_)))
    }))
}

/// [`scope_overrides_with`] for a runtime-name write of the boxed `value`.
pub(super) fn scope_overrides_write(
    ctx: &FunctionContext<'_>,
    value: ValueId,
    inst: &Instruction,
) -> Result<Option<crate::fast_hash::FastSet<(String, String)>>> {
    let value_ty = ctx.value_php_type(value)?;
    Ok(scope_overrides_with(ctx, true, |ctx, class_name, property| {
        matches!(
            mixed_property_write_candidate(
                ctx,
                class_name,
                property,
                value,
                &value_ty,
                PropertyAccessKind::RuntimeWrite,
                inst,
            ),
            Ok(Some(_))
        )
    }))
}

/// [`scope_overrides_with`] for a runtime-name removal.
pub(super) fn scope_overrides_unset(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<Option<crate::fast_hash::FastSet<(String, String)>>> {
    Ok(scope_overrides_with(ctx, true, |ctx, class_name, property| {
        resolve_property_runtime_action(
            ctx,
            class_name,
            property,
            PropertyAccessKind::RuntimeUnset,
            inst,
        )
        .is_ok()
    }))
}

/// Loads the helper's arguments and calls it.
fn emit_get_call(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    property_value: ValueId,
    label: &str,
    eval_already_asked: bool,
) -> Result<()> {
    let target = ctx.emitter.target;
    ctx.load_value_to_reg(object, abi::int_arg_reg_name(target, 0))?;
    ctx.load_string_value_to_regs(
        property_value,
        abi::int_arg_reg_name(target, 1),
        abi::int_arg_reg_name(target, 2),
    )?;
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(target, 3),
        i64::from(eval_already_asked),
    );
    abi::emit_call_label(ctx.emitter, label);
    Ok(())
}

/// Writes the body of every runtime-name property helper the function just emitted asked for.
///
/// Returns whether any body was written, so the caller can drain again: a body may itself ask
/// for another shared helper.
pub(in crate::codegen) fn emit_pending_dynamic_property_helpers(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    shared: &mut SharedCodegenState,
) -> Result<bool> {
    let mut emitted_any = false;
    for pending in shared.take_pending_dynamic_prop_helpers() {
        emitted_any = true;
        emitter.comment(&format!(
            "{}{:012x}",
            HELPER_MARKER_OPEN,
            helper_marker_scope(&pending.key)
        ));
        let emitted = match pending.kind.clone() {
            DynamicPropHelperKind::MixedGet(mode) => emit_shared_helper_in_scope(
                module,
                emitter,
                data,
                shared,
                false,
                &pending.label,
                PhpType::Mixed,
                pending.scope.as_deref(),
                &format!("--- shared runtime-name property helper: {} ---", pending.label),
                |ctx| emit_get_helper_body(ctx, mode, pending.scope.as_deref()),
            ),
            DynamicPropHelperKind::MixedSet
            | DynamicPropHelperKind::MixedUnset
            | DynamicPropHelperKind::MixedGetRuntimeScope(_)
            | DynamicPropHelperKind::MixedSetRuntimeScope
            | DynamicPropHelperKind::MixedUnsetRuntimeScope => {
                emit_function_helper(module, emitter, data, shared, &pending)
            }
            DynamicPropHelperKind::NamedMixedSet { .. } => {
                emit_named_set_helper(module, emitter, data, shared, &pending)
            }
            DynamicPropHelperKind::NamedMixedGet { property, mode } => {
                emit_named_get_helper(module, emitter, data, shared, &pending, &property, mode)
            }
            DynamicPropHelperKind::MixedMethodCall => {
                crate::codegen::lower_inst::emit_mixed_method_call_helper(
                    module, emitter, data, shared, &pending,
                )
            }
        };
        emitter.comment(HELPER_MARKER_CLOSE);
        emitted?;
    }
    Ok(emitted_any)
}

/// The instruction the shared arms are written against: a `Mixed`-result read in `mode`.
fn synthetic_get_instruction(mode: PropertyFetchMode) -> Instruction {
    Instruction::new(
        Op::DynamicPropGet,
        Vec::new(),
        Some(Immediate::PropertyFetchMode(mode)),
        None,
        IrType::Heap(IrHeapKind::Mixed),
        PhpType::Mixed,
        Ownership::Owned,
        Effects::default(),
        None,
    )
}

/// Emits the ladder itself: class-id search, name comparison, then the arm that matched.
///
/// With a `scope` the ladder holds only the pairs the scope resolves differently from the global
/// scope, and every other receiver or name is handed to the global helper unchanged.
fn emit_get_helper_body(
    ctx: &mut FunctionContext<'_>,
    mode: PropertyFetchMode,
    scope: Option<&str>,
) -> Result<()> {
    let target = ctx.emitter.target;
    let inst = synthetic_get_instruction(mode);
    let delegate = scope.is_some();
    let eval_fallback = !delegate && super::mixed_property_reads::eval_owner_fallback_available(ctx);
    let overrides = if delegate {
        scope_overrides(ctx, mode, &inst)?
    } else {
        None
    };
    let candidates =
        declared_mixed_property_get_candidates(ctx, mode, &inst, overrides.as_ref())?;
    let done_label = ctx.next_label("mixed_dyn_prop_get_done");
    let finish_label = ctx.next_label("mixed_dyn_prop_get_finish");
    let miss_label = ctx.next_label("mixed_dyn_prop_get_miss");
    let not_object_label = ctx.next_label("mixed_dyn_prop_get_not_object");
    let stdclass_label = ctx.next_label("mixed_dyn_prop_get_stdclass");
    let match_labels = candidates
        .iter()
        .map(|candidate| {
            ctx.next_label(&format!(
                "mixed_dyn_prop_get_{}",
                label_fragment(&candidate.candidate.slot.property)
            ))
        })
        .collect::<Vec<_>>();
    let class_names = super::dynamic_property_read_entry::class_names_by_id(ctx);
    let arm_owner = super::dynamic_property_read_entry::shared_read_arm_owners(ctx, &class_names, &candidates);
    let match_labels = arm_owner.iter().map(|owner| match_labels[*owner].clone()).collect::<Vec<_>>();
    let hash_arms = if delegate { Vec::new() } else { mixed_class_hash_arms(ctx, "", &[])? };
    let hash_labels = hash_arms
        .iter()
        .map(|arm| {
            ctx.next_label(&format!(
                "mixed_dyn_prop_get_hash_{}",
                label_fragment(&arm.class_name)
            ))
        })
        .collect::<Vec<_>>();

    let hash_owner = super::dynamic_property_read_entry::shared_hash_arm_owners(&hash_arms);
    let hash_labels = hash_owner.iter().map(|owner| hash_labels[*owner].clone()).collect::<Vec<_>>();
    let box_arg = abi::int_arg_reg_name(target, 0);
    let name_ptr_arg = abi::int_arg_reg_name(target, 1);
    let name_len_arg = abi::int_arg_reg_name(target, 2);
    let flag_arg = abi::int_arg_reg_name(target, 3);
    let result_reg = abi::int_result_reg(ctx.emitter);

    // The outer frame parks the eval-retry flag (offset 0) and the receiver box (offset 8), which
    // the ladder's frame sits on top of.
    let outer = eval_fallback || delegate;
    if outer {
        abi::emit_reserve_temporary_stack(ctx.emitter, FLAG_FRAME_BYTES);
        abi::emit_store_to_sp(ctx.emitter, flag_arg, 0);
        abi::emit_store_to_sp(ctx.emitter, box_arg, 8);
    }
    abi::emit_reserve_temporary_stack(ctx.emitter, LADDER_FRAME_BYTES);
    abi::emit_store_to_sp(ctx.emitter, name_ptr_arg, 0);
    abi::emit_store_to_sp(ctx.emitter, name_len_arg, 8);
    abi::emit_reg_move(ctx.emitter, result_reg, box_arg);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_branch_if_mixed_unboxed_not_object(ctx, &not_object_label);
    let payload_reg = crate::codegen_support::mixed_unbox_payload_reg(target);
    abi::emit_store_to_sp(ctx.emitter, payload_reg, 16);

    let dispatch_candidates = candidates
        .iter()
        .map(|candidate| candidate.candidate.clone())
        .collect::<Vec<_>>();
    emit_mixed_dynamic_property_candidate_dispatch(ctx, &dispatch_candidates, &match_labels);
    for (arm, label) in hash_arms.iter().zip(hash_labels.iter()) {
        emit_branch_if_stacked_object_class_matches(ctx, arm.class_id, 16, label);
    }
    if !delegate {
        emit_branch_if_stacked_object_is_stdclass(ctx, 16, &stdclass_label);
    }
    abi::emit_jump(ctx.emitter, &miss_label);

    for (index, (arm, label)) in hash_arms.iter().zip(hash_labels.iter()).enumerate() {
        if hash_owner[index] != index {
            continue;
        }
        ctx.emitter.label(label);
        // The runtime-key helper takes the key from the stacked block and releases it itself.
        lower_runtime_allow_dynamic_prop_get(ctx, &inst, arm.hash_offset, 16, 0, LADDER_FRAME_BYTES)?;
        abi::emit_jump(ctx.emitter, &done_label);
    }

    for (index, (candidate, label)) in candidates.iter().zip(match_labels.iter()).enumerate() {
        if arm_owner[index] != index {
            continue;
        }
        ctx.emitter.label(label);
        if let Some(visibility) =
            super::dynamic_property_read_entry::read_arm_dynamic_refusal(ctx, &class_names, candidate)
        {
            // Composes the text from the receiver's own class name, releasing the block.
            super::closure_scope_gate::emit_scope_refusal(ctx, visibility);
            continue;
        }
        let mut shared_slot = candidate.candidate.slot.clone();
        if matches!(candidate.kind, MixedPropertyReadKind::Slot) {
            shared_slot.class_name = super::runtime_property_writes::message_class_for_slot(ctx, &shared_slot);
        }
        let slot = &shared_slot;
        let base_reg = abi::symbol_scratch_reg(ctx.emitter);
        abi::emit_load_temporary_stack_slot(ctx.emitter, base_reg, 16);
        match &candidate.kind {
            MixedPropertyReadKind::Refuse(message) => {
                abi::emit_release_temporary_stack(ctx.emitter, LADDER_FRAME_BYTES);
                emit_refused_read_or_eval_child_property(
                    ctx, slot, message, base_reg, mode, &done_label,
                )?;
                continue;
            }
            MixedPropertyReadKind::ScopeDynamic => {
                let class_name = slot.class_name.clone();
                let property = slot.property.clone();
                abi::emit_release_temporary_stack(ctx.emitter, LADDER_FRAME_BYTES);
                emit_scope_dynamic_property_read(ctx, &inst, &class_name, &property, base_reg, mode)?;
                abi::emit_jump(ctx.emitter, &done_label);
                continue;
            }
            MixedPropertyReadKind::MagicDeferred => {
                // A declared slot this scope cannot reach, on a class with the accessor php asks
                // instead: `__isset` for `isset()`, `__get` for a read, `??` and `empty()`.
                // `isset($o->$name)` over a private name answered false without asking.
                let accessor = if super::mixed_property_reads::magic_isset_alone(mode) { "__isset" } else { "__get" };
                let class_name = slot.class_name.clone();
                if super::property_resolution::class_declares_method(ctx, &class_name, accessor) {
                    super::mixed_property_reads::emit_runtime_name_magic_get_arm(
                        ctx,
                        &class_name,
                        mode,
                        LADDER_FRAME_BYTES,
                        &done_label,
                    )?;
                    continue;
                }
                abi::emit_release_temporary_stack(ctx.emitter, LADDER_FRAME_BYTES);
                emit_dynamic_property_miss_result(ctx, &inst);
                abi::emit_jump(ctx.emitter, &done_label);
                continue;
            }
            MixedPropertyReadKind::Slot => {}
        }
        let read_done = emit_property_read_state_guard(
            ctx,
            slot,
            base_reg,
            mode,
            PropertyReadMissingResult::Instruction(&inst),
        )?;
        emit_property_load(ctx, slot, base_reg)?;
        materialize_loaded_property_result(ctx, &inst, &slot.php_type)?;
        if let Some(read_done) = read_done {
            ctx.emitter.label(&read_done);
        }
        abi::emit_release_temporary_stack(ctx.emitter, LADDER_FRAME_BYTES);
        abi::emit_jump(ctx.emitter, &done_label);
    }

    if delegate {
        // Nothing here matched: the global helper answers for this receiver and name, with the
        // caller's own flag, so its eval handshake reaches the site unchanged.
        ctx.emitter.label(&not_object_label);
        ctx.emitter.label(&miss_label);
        abi::emit_load_temporary_stack_slot(ctx.emitter, name_ptr_arg, 0);
        abi::emit_load_temporary_stack_slot(ctx.emitter, name_len_arg, 8);
        abi::emit_load_temporary_stack_slot(ctx.emitter, flag_arg, LADDER_FRAME_BYTES);
        abi::emit_load_temporary_stack_slot(ctx.emitter, box_arg, LADDER_FRAME_BYTES + 8);
        abi::emit_release_temporary_stack(ctx.emitter, LADDER_FRAME_BYTES);
        let base_key = get_helper_key(mode, None);
        let base_label = ctx.keyed_global_label(&base_key, GET_PREFIX);
        ctx.shared.request_dynamic_prop_helper(
            &base_key,
            &base_label,
            DynamicPropHelperKind::MixedGet(mode),
            None,
        );
        abi::emit_call_label(ctx.emitter, &base_label);
    } else {
        // An object of an eval-declared class is carried by a stdClass runtime object (or matches
        // no compiled class at all), so both arrival points hand the site a `0` the first time
        // round and let it ask the owning eval context before the native hash answers.
        ctx.emitter.label(&stdclass_label);
        if eval_fallback {
            emit_return_zero_unless_eval_asked(ctx, &finish_label);
        }
        emit_runtime_stdclass_get_for_stacked_name(ctx, &inst, 16, 0)?;
        abi::emit_release_temporary_stack(ctx.emitter, LADDER_FRAME_BYTES);
        abi::emit_jump(ctx.emitter, &done_label);

        ctx.emitter.label(&miss_label);
        // A class that declares no such property but has `__get` answers through it, as php
        // does, instead of the undefined-property miss; `isset()` asks its `__isset` instead.
        let magic_arms = if super::mixed_property_reads::magic_calls_get(mode)
            || super::mixed_property_reads::magic_isset_alone(mode)
        {
            super::mixed_property_reads::runtime_name_magic_get_classes(ctx, mode)
                .into_iter()
                .map(|(class_name, class_id)| {
                    let label = ctx.next_label(&format!(
                        "mixed_dyn_prop_get_magic_{}",
                        label_fragment(&class_name)
                    ));
                    (class_name, class_id, label)
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let plain_miss_label = ctx.next_label("mixed_dyn_prop_get_plain_miss");
        for (_, class_id, label) in &magic_arms {
            emit_branch_if_stacked_object_class_matches(ctx, *class_id, 16, label);
        }
        abi::emit_jump(ctx.emitter, &plain_miss_label);
        for (class_name, _, label) in &magic_arms {
            ctx.emitter.label(label);
            super::mixed_property_reads::emit_runtime_name_magic_get_arm(
                ctx,
                class_name,
                mode,
                LADDER_FRAME_BYTES,
                &done_label,
            )?;
        }
        ctx.emitter.label(&plain_miss_label);
        if eval_fallback {
            emit_return_zero_unless_eval_asked(ctx, &finish_label);
        }
        abi::emit_release_temporary_stack(ctx.emitter, LADDER_FRAME_BYTES);
        emit_dynamic_property_miss_result(ctx, &inst);
        abi::emit_jump(ctx.emitter, &done_label);

        // Only an object can have an eval owner, so a non-object receiver never asks the site.
        ctx.emitter.label(&not_object_label);
        abi::emit_release_temporary_stack(ctx.emitter, LADDER_FRAME_BYTES);
        emit_dynamic_property_miss_result(ctx, &inst);
    }

    ctx.emitter.label(&done_label);
    ctx.emitter.label(&finish_label);
    if outer {
        abi::emit_release_temporary_stack(ctx.emitter, FLAG_FRAME_BYTES);
    }
    Ok(())
}

/// At an eval arrival point: when the caller has not asked the eval owner yet, releases the
/// ladder's frame and leaves `0` in the result register, finishing the helper.
fn emit_return_zero_unless_eval_asked(ctx: &mut FunctionContext<'_>, finish_label: &str) {
    let asked_label = ctx.next_label("mixed_dyn_prop_get_eval_asked");
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, LADDER_FRAME_BYTES);
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, &asked_label);
    abi::emit_release_temporary_stack(ctx.emitter, LADDER_FRAME_BYTES);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
    abi::emit_jump(ctx.emitter, finish_label);
    ctx.emitter.label(&asked_label);
}

/// The key of a write or removal helper for one scope.
fn function_helper_key(prefix: &str, scope: Option<&str>) -> String {
    format!("{}:{}", prefix, scope.unwrap_or(""))
}

/// Returns whether the body being lowered IS one of the shared write/removal helpers.
fn inside_function_helper(ctx: &FunctionContext<'_>, prefix: &str) -> bool {
    ctx.function.flags.is_synthetic && ctx.function.name.contains(&format!("_{}_", prefix))
}

/// Inside a SCOPED write/removal helper, the entry label of the global helper it delegates to.
///
/// A helper emitted for a scope that resolves some pairs differently only holds those pairs.
/// Whatever it does not match is the global helper's, called with the same operands.
pub(super) fn outlined_helper_delegate(ctx: &mut FunctionContext<'_>, prefix: &str) -> Option<String> {
    // A runtime-scope helper holds the whole ladder: its scope is only known at run time.
    if !inside_function_helper(ctx, prefix)
        || ctx.function.lexical_class.is_none()
        || ctx.local_slot_by_name(crate::names::CLOSURE_SCOPE_LOCAL).is_some()
    {
        return None;
    }
    Some(ctx.keyed_global_label(&function_helper_key(prefix, None), prefix))
}

/// Calls the global helper from a scoped one with the operands the scoped helper received.
pub(super) fn emit_delegate_call_to_global_helper(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    property_value: ValueId,
    value: Option<ValueId>,
    base_label: &str,
) -> Result<()> {
    let target = ctx.emitter.target;
    ctx.load_value_to_reg(object, abi::int_arg_reg_name(target, 0))?;
    ctx.load_string_value_to_regs(
        property_value,
        abi::int_arg_reg_name(target, 1),
        abi::int_arg_reg_name(target, 2),
    )?;
    if let Some(value) = value {
        ctx.load_value_to_reg(value, abi::int_arg_reg_name(target, 3))?;
    }
    let (prefix, kind) = if value.is_some() {
        (SET_PREFIX, DynamicPropHelperKind::MixedSet)
    } else {
        (UNSET_PREFIX, DynamicPropHelperKind::MixedUnset)
    };
    let key = function_helper_key(prefix, None);
    ctx.shared.request_dynamic_prop_helper(&key, base_label, kind, None);
    abi::emit_call_label(ctx.emitter, base_label);
    Ok(())
}

/// Lowers a runtime-name write on a boxed `Mixed` receiver through the shared helper.
///
/// Returns `false` without emitting anything when this site must keep the inline ladder.
pub(super) fn try_lower_outlined_mixed_prop_set(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    property_value: ValueId,
    value: ValueId,
    inst: &Instruction,
) -> Result<bool> {
    if outlining_disabled()
        || inside_function_helper(ctx, SET_PREFIX)
        || ctx.value_php_type(value)?.codegen_repr() != PhpType::Mixed
        || ctx.value_php_type(property_value)?.codegen_repr() != PhpType::Str
    {
        return Ok(false);
    }
    if let Some(scope_slot) = ctx.local_slot_by_name(crate::names::CLOSURE_SCOPE_LOCAL) {
        let label = request_runtime_scope_helper(ctx, SET_PREFIX, DynamicPropHelperKind::MixedSetRuntimeScope);
        emit_runtime_scope_helper_call(ctx, object, property_value, Some(value), scope_slot, &label)?;
    } else {
        let Some(overrides) = scope_overrides_write(ctx, value, inst)? else {
            return Ok(false);
        };
        let scope = helper_scope(ctx, overrides.is_empty());
        let key = function_helper_key(SET_PREFIX, scope.as_deref());
        let label = ctx.keyed_global_label(&key, SET_PREFIX);
        ctx.shared.request_dynamic_prop_helper(&key, &label, DynamicPropHelperKind::MixedSet, scope.as_deref());
        emit_function_helper_call(ctx, object, property_value, Some(value), &label)?;
    }
    // The inline ladder ADOPTS an owning box into the slot, and the lowering that emitted this
    // write relies on it: it releases such a temporary only when the store would not take it
    // over. The helper receives the box as a borrowed parameter, so it retains instead; the
    // reference the inline store would have consumed is released here, once the call is done.
    if ctx.value_can_own_mixed_box_source(value)? {
        ctx.load_value_to_result(value)?;
        abi::emit_decref_if_refcounted(ctx.emitter, &PhpType::Mixed);
    }
    Ok(true)
}

/// Lowers a runtime-name removal on a boxed `Mixed` receiver through the shared helper.
pub(super) fn try_lower_outlined_mixed_prop_unset(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    property_value: ValueId,
    inst: &Instruction,
) -> Result<bool> {
    if outlining_disabled()
        || inside_function_helper(ctx, UNSET_PREFIX)
        || ctx.value_php_type(object)?.codegen_repr() != PhpType::Mixed
        || ctx.value_php_type(property_value)?.codegen_repr() != PhpType::Str
    {
        return Ok(false);
    }
    if let Some(scope_slot) = ctx.local_slot_by_name(crate::names::CLOSURE_SCOPE_LOCAL) {
        let label =
            request_runtime_scope_helper(ctx, UNSET_PREFIX, DynamicPropHelperKind::MixedUnsetRuntimeScope);
        emit_runtime_scope_helper_call(ctx, object, property_value, None, scope_slot, &label)?;
        return Ok(true);
    }
    let Some(overrides) = scope_overrides_unset(ctx, inst)? else {
        return Ok(false);
    };
    let scope = helper_scope(ctx, overrides.is_empty());
    let key = function_helper_key(UNSET_PREFIX, scope.as_deref());
    let label = ctx.keyed_global_label(&key, UNSET_PREFIX);
    ctx.shared.request_dynamic_prop_helper(&key, &label, DynamicPropHelperKind::MixedUnset, scope.as_deref());
    emit_function_helper_call(ctx, object, property_value, None, &label)?;
    Ok(true)
}

/// Readable part of the named-read helper's entry symbol.
pub(super) const NAMED_GET_PREFIX: &str = "named_prop_get";

/// Smallest ladder, in class arms, worth a call: below it the inline ladder is smaller than
/// the call sequence plus a helper of its own.
const NAMED_GET_MIN_ARMS: usize = 3;

/// Lowers `$mixed->name` (a declared name, a `Mixed` result) through a helper shared by every
/// site that reads that name from the same scope.
///
/// The inline ladder is one arm per class that declares the name, re-emitted at every site, so
/// its cost is sites x classes. Measured on the compiled Laravel `--web` app: 18 212 `prop_get`
/// sites averaging 2 430 instructions, 44 M instructions in all, 16% of the binary; the average
/// had grown 3.7x as more of the vendor tree compiled, because every site's ladder grows with the
/// class count. The helper is an ordinary synthetic EIR function `(object: Mixed) -> Mixed`
/// holding the one instruction, so the existing lowering -- ladder, eval-owner fallback, refusal
/// and undefined-property diagnostics -- runs unchanged inside it.
///
/// Key: the name, the opcode (plain or nullsafe read) and, only when the name resolves
/// differently by scope, the lexical class. A name is scope-sensitive when some class declares it
/// non-public or keeps more than one slot under it (an ancestor's hidden private); otherwise
/// every scope reads it alike and one global helper serves the whole program.
pub(super) fn try_lower_outlined_named_mixed_prop_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    property: &str,
    arm_count: usize,
) -> Result<bool> {
    if outlining_disabled()
        || arm_count < NAMED_GET_MIN_ARMS
        || inside_function_helper(ctx, NAMED_GET_PREFIX)
        || ctx.local_slot_by_name(crate::names::CLOSURE_SCOPE_LOCAL).is_some()
        || inst.result_php_type.codegen_repr() != PhpType::Mixed
        || inst.result_ownership != Ownership::Owned
        || ctx.value_php_type(object)?.codegen_repr() != PhpType::Mixed
        || !matches!(inst.immediate, Some(Immediate::Data(_)))
    {
        return Ok(false);
    }
    let scope = if named_property_is_scope_sensitive(ctx, property) {
        ctx.function
            .lexical_class
            .as_deref()
            .map(|class| class.trim_start_matches('\\').to_string())
    } else {
        None
    };
    let mode = super::property_resolution::property_fetch_mode(inst);
    let key = format!(
        "{}:{}:{}:{:?}:{}",
        NAMED_GET_PREFIX,
        inst.op.name(),
        property,
        mode,
        scope.as_deref().unwrap_or("")
    );
    let label = ctx.keyed_global_label(&key, NAMED_GET_PREFIX);
    ctx.shared.request_dynamic_prop_helper(
        &key,
        &label,
        DynamicPropHelperKind::NamedMixedGet {
            property: property.to_string(),
            mode,
        },
        scope.as_deref(),
    );
    pending_named_get_ops(ctx, &key, inst);
    ctx.load_value_to_reg(object, abi::int_arg_reg_name(ctx.emitter.target, 0))?;
    abi::emit_call_label(ctx.emitter, &label);
    store_if_result(ctx, inst)?;
    Ok(true)
}

/// Readable part of the named-write helper's entry symbol.
pub(super) const NAMED_SET_PREFIX: &str = "named_prop_set";

/// Lowers `$mixed->name = $boxed` (a declared name, a boxed `Mixed` value) through a helper shared
/// by every site that writes that name from the same scope.
///
/// The inline ladder is one arm per class that declares the name (a typed-slot guard and a store
/// each), re-emitted at every site, so its cost is sites x classes: a Symfony console carried 7.9 M
/// instructions of them. The helper is an ordinary synthetic EIR function
/// `(object: Mixed, value: Mixed)` holding the one `PropSet`, so the ladder, the dynamic-hash table
/// block, the eval-owner probe and every refusal run unchanged inside it.
///
/// Key: the name and, only when the name resolves differently by scope (see
/// [`named_property_is_scope_sensitive`]), the lexical class. The value is a BORROWED parameter of
/// the helper, which retains where the inline store adopted an owning temporary; the site releases
/// that temporary after the call, exactly as for the runtime-name write.
pub(super) fn try_lower_outlined_named_mixed_prop_set(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    value: ValueId,
    property: &str,
    arm_count: usize,
) -> Result<bool> {
    if outlining_disabled()
        || arm_count < NAMED_GET_MIN_ARMS
        || inst.op != Op::PropSet
        || inside_function_helper(ctx, NAMED_SET_PREFIX)
        || ctx.local_slot_by_name(crate::names::CLOSURE_SCOPE_LOCAL).is_some()
        || ctx.value_php_type(object)?.codegen_repr() != PhpType::Mixed
        || ctx.value_php_type(value)?.codegen_repr() != PhpType::Mixed
        || !matches!(inst.immediate, Some(Immediate::Data(_)))
    {
        return Ok(false);
    }
    let scope = if named_property_is_scope_sensitive(ctx, property) {
        ctx.function
            .lexical_class
            .as_deref()
            .map(|class| class.trim_start_matches('\\').to_string())
    } else {
        None
    };
    let key = format!("{}:{}:{}", NAMED_SET_PREFIX, property, scope.as_deref().unwrap_or(""));
    let label = ctx.keyed_global_label(&key, NAMED_SET_PREFIX);
    ctx.shared.request_dynamic_prop_helper(
        &key,
        &label,
        DynamicPropHelperKind::NamedMixedSet { property: property.to_string() },
        scope.as_deref(),
    );
    pending_named_get_ops(ctx, &key, inst);
    let target = ctx.emitter.target;
    ctx.load_value_to_reg(object, abi::int_arg_reg_name(target, 0))?;
    ctx.load_value_to_reg(value, abi::int_arg_reg_name(target, 1))?;
    abi::emit_call_label(ctx.emitter, &label);
    if ctx.value_can_own_mixed_box_source(value)? {
        ctx.load_value_to_result(value)?;
        abi::emit_decref_if_refcounted(ctx.emitter, &PhpType::Mixed);
    }
    Ok(true)
}

/// Emits a named-write helper as a synthetic EIR function holding the one `PropSet`.
fn emit_named_set_helper(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    shared: &mut SharedCodegenState,
    pending: &crate::codegen::shared_state::PendingDynamicPropHelper,
) -> Result<()> {
    use crate::ir::{Builder, Function, FunctionParam, LocalKind, LocalSlotId, Terminator};

    let Some((op, immediate)) = shared.named_get_templates.get(&pending.key).cloned() else {
        return Err(CodegenIrError::invalid_module(format!(
            "named property write helper {} has no template",
            pending.key
        )));
    };
    let mixed = IrType::Heap(IrHeapKind::Mixed);
    let mut function = Function::new(pending.label.clone(), IrType::Void, PhpType::Void);
    function.flags.is_synthetic = true;
    function.lexical_class = pending.scope.clone();
    for name in ["object", "value"] {
        function.params.push(FunctionParam {
            name: name.to_string(),
            ir_type: mixed.clone(),
            php_type: PhpType::Mixed,
            by_ref: false,
            variadic: false,
        });
        function.add_local(Some(name.to_string()), mixed.clone(), PhpType::Mixed, LocalKind::PhpLocal);
    }
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", Vec::new());
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let operands = (0..2)
            .map(|index| {
                builder.emit_load_local(LocalSlotId::from_raw(index), mixed.clone(), PhpType::Mixed)
            })
            .collect::<Vec<_>>();
        builder.emit_with_effects(
            op,
            operands,
            immediate,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
            // The site already published the file and line of the PHP statement.
            op.default_effects() - crate::ir::Effects::MAY_WARN,
            None,
        );
        builder.terminate(Terminator::Return { value: None });
    }
    crate::codegen::block_emit::emit_synthetic_function_with_label(
        module,
        &function,
        &pending.label,
        emitter,
        data,
        shared,
        false,
    )
}

/// Remembers the opcode and name immediate a named-read helper is built from.
fn pending_named_get_ops(ctx: &mut FunctionContext<'_>, key: &str, inst: &Instruction) {
    ctx.shared
        .named_get_templates
        .entry(key.to_string())
        .or_insert_with(|| (inst.op, inst.immediate.clone()));
}

/// Whether a read of `property` may resolve differently from one class scope to another.
fn named_property_is_scope_sensitive(ctx: &FunctionContext<'_>, property: &str) -> bool {
    ctx.module.class_infos.values().any(|info| {
        let slots = info
            .properties
            .iter()
            .filter(|(name, _)| name == property)
            .count();
        slots > 1
            || (slots == 1
                && !matches!(
                    info.property_visibilities.get(property),
                    Some(Visibility::Public)
                ))
    })
}

/// Emits a named-read helper as a synthetic EIR function holding the one read instruction.
fn emit_named_get_helper(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    shared: &mut SharedCodegenState,
    pending: &crate::codegen::shared_state::PendingDynamicPropHelper,
    _property: &str,
    _mode: crate::ir::PropertyFetchMode,
) -> Result<()> {
    use crate::ir::{Builder, Function, FunctionParam, LocalKind, LocalSlotId, Terminator};

    let Some((op, immediate)) = shared.named_get_templates.get(&pending.key).cloned() else {
        return Err(CodegenIrError::invalid_module(format!(
            "named property read helper {} has no template",
            pending.key
        )));
    };
    let mixed = IrType::Heap(IrHeapKind::Mixed);
    let mut function = Function::new(pending.label.clone(), mixed.clone(), PhpType::Mixed);
    function.flags.is_synthetic = true;
    function.lexical_class = pending.scope.clone();
    function.params.push(FunctionParam {
        name: "object".to_string(),
        ir_type: mixed.clone(),
        php_type: PhpType::Mixed,
        by_ref: false,
        variadic: false,
    });
    function.add_local(Some("object".to_string()), mixed.clone(), PhpType::Mixed, LocalKind::PhpLocal);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", Vec::new());
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let object = builder.emit_load_local(LocalSlotId::from_raw(0), mixed.clone(), PhpType::Mixed);
        let value = builder.emit_with_effects(
            op,
            vec![object],
            immediate,
            mixed.clone(),
            PhpType::Mixed,
            Ownership::Owned,
            // The site already published the file and line of the PHP statement. This
            // instruction has no span, so letting it warn-publish would overwrite them with zero.
            op.default_effects() - crate::ir::Effects::MAY_WARN,
            None,
        );
        builder.terminate(Terminator::Return { value });
    }
    crate::codegen::block_emit::emit_synthetic_function_with_label(
        module,
        &function,
        &pending.label,
        emitter,
        data,
        shared,
        false,
    )
}

/// The scope a helper is keyed on: none when the scope resolves nothing differently.
fn helper_scope(ctx: &FunctionContext<'_>, no_overrides: bool) -> Option<String> {
    if no_overrides {
        return None;
    }
    ctx.function
        .lexical_class
        .as_deref()
        .map(|class| class.trim_start_matches('\\').to_string())
}

/// Requests THE runtime-scope write/removal helper (one per prefix for the whole program).
///
/// The helper holds the whole `(class, name)` ladder, so keying it by the caller's lexical class
/// emitted one full-size copy per class that binds a scope (DeepClone's hydrators, the console's
/// `Ask`/`AskChoice` closures, ...): three copies of a 330 000-instruction body on the Symfony
/// app. The lexical class only mattered as the FALLBACK for an unstamped scope, and the caller
/// now supplies that fallback in the scope argument itself (`emit_runtime_scope_helper_call`),
/// so the helper is the same for every caller.
fn request_runtime_scope_helper(
    ctx: &mut FunctionContext<'_>,
    prefix: &str,
    kind: DynamicPropHelperKind,
) -> String {
    let key = format!("{}:runtime-scope", prefix);
    let label = ctx.keyed_global_label(&key, prefix);
    ctx.shared.request_dynamic_prop_helper(&key, &label, kind, None);
    label
}

/// Calls a runtime-scope helper: the ordinary operands, then the raw hidden scope
/// capture in the next argument register.
fn emit_runtime_scope_helper_call(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    property_value: ValueId,
    value: Option<ValueId>,
    scope_slot: crate::ir::LocalSlotId,
    label: &str,
) -> Result<()> {
    let target = ctx.emitter.target;
    ctx.load_value_to_reg(object, abi::int_arg_reg_name(target, 0))?;
    ctx.load_string_value_to_regs(
        property_value,
        abi::int_arg_reg_name(target, 1),
        abi::int_arg_reg_name(target, 2),
    )?;
    let mut next = 3;
    if let Some(value) = value {
        ctx.load_value_to_reg(value, abi::int_arg_reg_name(target, 3))?;
        next = 4;
    }
    let offset = ctx.local_offset(scope_slot)?;
    let scope_arg = abi::int_arg_reg_name(target, next);
    abi::load_at_offset(ctx.emitter, scope_arg, offset);
    // An unstamped capture (0) means "the closure's own lexical class". The shared helper has no
    // lexical class of its own, so the fallback travels in the argument, in the stamp's own
    // encoding (class id plus one). A closure outside any class keeps 0: the helper's global scope.
    let lexical_id = ctx.lexical_class_id();
    if lexical_id >= 0 {
        let stamped = ctx.next_label("scope_stamped");
        match target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("cbnz {scope_arg}, {stamped}")); // a bound scope keeps its stamp
            }
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("test {scope_arg}, {scope_arg}")); // 0 = never rebound
                ctx.emitter.instruction(&format!("jnz {stamped}"));             // a bound scope keeps its stamp
            }
        }
        abi::emit_load_int_immediate(ctx.emitter, scope_arg, lexical_id + 1);
        ctx.emitter.label(&stamped);
    }
    abi::emit_call_label(ctx.emitter, label);
    Ok(())
}

/// Loads the operands into the argument registers and calls a write/removal helper.
fn emit_function_helper_call(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    property_value: ValueId,
    value: Option<ValueId>,
    label: &str,
) -> Result<()> {
    let target = ctx.emitter.target;
    ctx.load_value_to_reg(object, abi::int_arg_reg_name(target, 0))?;
    ctx.load_string_value_to_regs(
        property_value,
        abi::int_arg_reg_name(target, 1),
        abi::int_arg_reg_name(target, 2),
    )?;
    if let Some(value) = value {
        ctx.load_value_to_reg(value, abi::int_arg_reg_name(target, 3))?;
    }
    abi::emit_call_label(ctx.emitter, label);
    Ok(())
}

/// Emits a function helper as an ordinary synthetic EIR function whose body is the one
/// instruction, so the existing lowering (frame, parameter spills, exception cleanup) runs
/// unchanged and the ladder inside it is the same code the inline sites used.
fn emit_function_helper(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    shared: &mut SharedCodegenState,
    pending: &crate::codegen::shared_state::PendingDynamicPropHelper,
) -> Result<()> {
    use crate::ir::{Builder, Function, FunctionParam, LocalKind, LocalSlotId, Terminator};

    let is_set = matches!(
        pending.kind,
        DynamicPropHelperKind::MixedSet | DynamicPropHelperKind::MixedSetRuntimeScope
    );
    let read_mode = match pending.kind {
        DynamicPropHelperKind::MixedGetRuntimeScope(mode) => Some(mode),
        _ => None,
    };
    let runtime_scope = read_mode.is_some()
        || matches!(
            pending.kind,
            DynamicPropHelperKind::MixedSetRuntimeScope | DynamicPropHelperKind::MixedUnsetRuntimeScope
        );
    let mixed = IrType::Heap(IrHeapKind::Mixed);
    let (return_ir, return_php) = match read_mode {
        Some(_) => (mixed.clone(), PhpType::Mixed),
        None => (IrType::Void, PhpType::Void),
    };
    let mut function = Function::new(pending.label.clone(), return_ir.clone(), return_php.clone());
    function.flags.is_synthetic = true;
    function.lexical_class = pending.scope.clone();
    let mut params = vec![
        ("object", IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed),
        ("name", IrType::Str, PhpType::Str),
    ];
    if is_set {
        params.push(("value", IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed));
    }
    // The opcode's operands; a runtime-scope helper's trailing scope capture is not one of them.
    let operand_count = params.len();
    if runtime_scope {
        params.push((crate::names::CLOSURE_SCOPE_LOCAL, IrType::I64, PhpType::Int));
    }
    for (name, ir_type, php_type) in &params {
        function.params.push(FunctionParam {
            name: (*name).to_string(),
            ir_type: ir_type.clone(),
            php_type: php_type.clone(),
            by_ref: false,
            variadic: false,
        });
        function.add_local(
            Some((*name).to_string()),
            ir_type.clone(),
            php_type.clone(),
            LocalKind::PhpLocal,
        );
    }
    let op = match (read_mode, is_set) {
        (Some(_), _) => Op::DynamicPropGet,
        (None, true) => Op::DynamicPropSet,
        (None, false) => Op::DynamicPropUnset,
    };
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", Vec::new());
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let operands = params[..operand_count]
            .iter()
            .enumerate()
            .map(|(index, (_, ir_type, php_type))| {
                builder.emit_load_local(
                    LocalSlotId::from_raw(index as u32),
                    ir_type.clone(),
                    php_type.clone(),
                )
            })
            .collect::<Vec<_>>();
        let result = builder.emit_with_effects(
            op,
            operands,
            read_mode.map(Immediate::PropertyFetchMode),
            return_ir,
            return_php,
            if read_mode.is_some() { Ownership::Owned } else { Ownership::NonHeap },
            // The site already published the file and line of the PHP statement. This
            // instruction has no span, so letting it warn-publish would overwrite them with zero.
            op.default_effects() - crate::ir::Effects::MAY_WARN,
            None,
        );
        builder.terminate(Terminator::Return { value: result });
    }
    crate::codegen::block_emit::emit_synthetic_function_with_label(
        module,
        &function,
        &pending.label,
        emitter,
        data,
        shared,
        false,
    )
}
