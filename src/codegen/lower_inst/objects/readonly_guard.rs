//! Purpose:
//! Enforces `readonly` on the runtime-name and boxed-receiver write and removal arms.
//!
//! Called from:
//! - The Mixed-receiver and runtime-name write/removal ladders, immediately before an arm stores
//!   into or detaches a declared slot.
//!
//! Key details:
//! - A statically typed `$o->ro = 1` is decided by the checker. A write whose receiver class or
//!   property name is only known at run time reaches a slot arm that used to store without asking
//!   whether the slot is readonly, so `$o->$n = 9` replaced a readonly value php refuses to touch.
//! - php 8.5: an INITIALIZED readonly property cannot be modified or unset from any scope
//!   (`Cannot modify|unset readonly property D::$p`). An uninitialized one is `protected(set)`:
//!   the declaring class and its subclasses may initialize it, anyone else gets
//!   `Cannot modify|unset protected(set) readonly property D::$p from global scope|scope S`. `D`
//!   is the DECLARING class.
//! - The calling scope is the lexical class of the body being lowered. A body with a runtime
//!   closure scope cannot know its scope here, so it is treated as in scope: it still refuses an
//!   initialized property, which is the common case.
//! - A shared helper emitted for the global scope reports `global scope` for an out-of-scope
//!   initialization even when the caller's scope is some other class, because the helper does not
//!   know its caller. A helper for a class that is in scope holds its own arms (see
//!   `outlined_dynamic_property::scope_overrides_with`).

use super::*;

/// Where the receiver object pointer is when the guard runs.
pub(super) enum GuardReceiver {
    /// In the temporary-stack slot at this byte offset.
    Stack(usize),
    /// In this SSA value.
    Value(ValueId),
}

/// What the guard needs to know about one readonly slot.
struct ReadonlyPlan {
    declaring: String,
    in_scope: bool,
    scope_text: String,
}

/// Returns the declaring class of a readonly property, or `None` for an ordinary slot.
pub(super) fn readonly_declaring_class(ctx: &FunctionContext<'_>, slot: &PropertySlot) -> Option<String> {
    if slot.is_packed || slot.is_reference {
        return None;
    }
    let declaring = super::runtime_property_writes::message_class_for_slot(ctx, slot);
    let info = class_info_by_name(ctx, &declaring)?;
    if info.readonly_properties.contains(&slot.property) {
        Some(declaring)
    } else {
        None
    }
}

/// Whether the class `sub` is `base` or extends it.
fn class_is_or_extends(ctx: &FunctionContext<'_>, sub: &str, base: &str) -> bool {
    let mut current = class_info_by_name(ctx, sub);
    let wanted = php_symbol_key(base.trim_start_matches('\\'));
    for _ in 0..256 {
        let Some(info) = current else {
            return false;
        };
        let Some(name) = ctx
            .module
            .class_infos
            .iter()
            .find(|(_, candidate)| candidate.class_id == info.class_id)
            .map(|(name, _)| name.clone())
        else {
            return false;
        };
        if php_symbol_key(name.trim_start_matches('\\')) == wanted {
            return true;
        }
        current = info.parent.as_deref().and_then(|parent| class_info_by_name(ctx, parent));
    }
    false
}

/// Whether the body being lowered may initialize a readonly property declared in `declaring`.
pub(super) fn scope_may_initialize(ctx: &FunctionContext<'_>, declaring: &str) -> bool {
    if ctx.local_slot_by_name(crate::names::CLOSURE_SCOPE_LOCAL).is_some() {
        return true;
    }
    match ctx.function.lexical_class.as_deref() {
        Some(scope) => class_is_or_extends(ctx, scope, declaring),
        None => false,
    }
}

fn plan_for(ctx: &FunctionContext<'_>, slot: &PropertySlot) -> Option<ReadonlyPlan> {
    let declaring = readonly_declaring_class(ctx, slot)?;
    let in_scope = scope_may_initialize(ctx, &declaring);
    let scope_text = match ctx.function.lexical_class.as_deref() {
        Some(scope) => format!("scope {}", scope.trim_start_matches('\\')),
        None => "global scope".to_string(),
    };
    Some(ReadonlyPlan {
        declaring,
        in_scope,
        scope_text,
    })
}

/// Emits the readonly check for a store into (`unset == false`) or removal of (`unset == true`)
/// `slot`. Falls through only when the access is allowed; the object register is reloaded by the
/// caller afterwards, so the scratch registers may be clobbered.
pub(super) fn emit_readonly_slot_guard(
    ctx: &mut FunctionContext<'_>,
    slot: &PropertySlot,
    unset: bool,
    receiver: GuardReceiver,
    stacked_bytes: usize,
) -> Result<()> {
    let Some(plan) = plan_for(ctx, slot) else {
        return Ok(());
    };
    let verb = if unset { "unset" } else { "modify" };
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    match receiver {
        GuardReceiver::Stack(offset) => abi::emit_load_temporary_stack_slot(ctx.emitter, base_reg, offset),
        GuardReceiver::Value(value) => {
            ctx.load_value_to_reg(value, base_reg)?;
        }
    }
    let uninitialized = ctx.next_label("readonly_slot_uninitialized");
    let marker_reg = abi::secondary_scratch_reg(ctx.emitter);
    let sentinel_reg = abi::tertiary_scratch_reg(ctx.emitter);
    abi::emit_load_from_address(ctx.emitter, marker_reg, base_reg, slot.offset + 8);
    abi::emit_load_int_immediate(ctx.emitter, sentinel_reg, UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {marker_reg}, {sentinel_reg}")); // is the readonly slot still uninitialized?
            ctx.emitter.instruction(&format!("b.eq {uninitialized}"));          // an uninitialized slot may be initialized from scope
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {marker_reg}, {sentinel_reg}")); // is the readonly slot still uninitialized?
            ctx.emitter.instruction(&format!("je {uninitialized}"));            // an uninitialized slot may be initialized from scope
        }
    }
    abi::emit_release_temporary_stack(ctx.emitter, stacked_bytes);
    super::super::exceptions::emit_error(
        ctx,
        &format!("Cannot {verb} readonly property {}::${}", plan.declaring, slot.property),
    );
    ctx.emitter.label(&uninitialized);
    if !plan.in_scope {
        abi::emit_release_temporary_stack(ctx.emitter, stacked_bytes);
        super::super::exceptions::emit_error(
            ctx,
            &format!(
                "Cannot {verb} protected(set) readonly property {}::${} from {}",
                plan.declaring, slot.property, plan.scope_text
            ),
        );
    }
    Ok(())
}

/// A key part that makes two slot arms share code only when their readonly guards agree.
pub(super) fn readonly_key(ctx: &FunctionContext<'_>, slot: &PropertySlot) -> String {
    match plan_for(ctx, slot) {
        Some(plan) => format!("ro|{}|{}", plan.declaring, plan.in_scope),
        None => String::new(),
    }
}

/// Whether the body's scope may initialize the readonly property `property` of `class_name`.
///
/// False for an ordinary property, for a body with no class scope, and for a runtime closure scope
/// (which is always treated as in scope and so needs no helper of its own).
pub(super) fn scope_initializes_readonly_pair(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    property: &str,
) -> bool {
    let Some(lexical) = ctx.function.lexical_class.as_deref() else {
        return false;
    };
    let Some(info) = class_info_by_name(ctx, class_name) else {
        return false;
    };
    if !info.properties.iter().any(|(name, _)| name == property) {
        return false;
    }
    let Some(declaring) = info.property_declaring_classes.get(property) else {
        return false;
    };
    let Some(declaring_info) = class_info_by_name(ctx, declaring) else {
        return false;
    };
    declaring_info.readonly_properties.contains(property) && class_is_or_extends(ctx, lexical, declaring)
}
