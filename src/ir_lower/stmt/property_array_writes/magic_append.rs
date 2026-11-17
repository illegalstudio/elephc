//! Purpose:
//! Implements append fetches of ordinary overloaded properties without a magic setter.
//!
//! Called from:
//! - Exact-class property append dispatch before the ordinary declared-slot writer.
//!
//! Key details:
//! - Existing dynamic entries bypass __get, including entries whose value is null.
//! - Missing overloaded getters are refused before a temporary could become fake storage.
//! - Native XML selection remains the separate supported overloaded write-fetch path.

use super::*;

/// Selects an inaccessible or missing ordinary name whose read invokes __get.
pub(super) fn is_candidate(
    ctx: &LoweringContext<'_, '_>, class: &str, info: &crate::types::ClassInfo, property: &str,
) -> bool {
    info.methods.contains_key("__get")
        && !info.property_hooks.get(property).is_some_and(|hooks| hooks.any())
        && crate::types::builtin_classes::native_property_method(class, property,
            elephc_builtin_contract::ObjectProperty::Get, ctx.classes).is_none()
        && (info.visible_property(property).is_none()
            || !crate::ir_lower::expr::property_is_accessible_for_ir(ctx, class, info, property))
}

/// Uses existing dynamic entries and diagnoses unsupported missing getter storage.
pub(super) fn lower(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, property: &str,
    value: LoweredValue, span: Span,
) -> bool {
    let PhpType::Object(class) = ctx.builder.value_php_type(object.value).codegen_repr() else { return false; };
    let class = class.trim_start_matches('\\').to_string();
    let Some(info) = ctx.classes.get(&class) else { return false; };
    if !is_candidate(ctx, &class, info, property) { return false; }
    let has_hash = info.dynamic_property_hash_is_name_addressable()
        || crate::types::checker::builtin_stdclass::is_stdclass(&class);
    if !has_hash {
        refuse_missing_getter(ctx, span);
        return true;
    }
    let name = string(ctx, property, span);
    let present = ctx.emit_value(Op::RuntimeCall, vec![object.value, name.value],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::PropertyHashContains)), PhpType::Bool,
        effects_lookup::runtime_effects(), Some(span));
    let stored = ctx.builder.create_named_block("magic.append.stored", vec![]);
    let getter = ctx.builder.create_named_block("magic.append.getter", vec![]);
    let done = ctx.builder.create_named_block("magic.append.done", vec![]);
    let initial = ctx.initialized_slots_snapshot();
    ctx.builder.terminate(Terminator::CondBr { cond: present.value,
        then_target: stored, then_args: vec![], else_target: getter, else_args: vec![] });
    ctx.builder.position_at_end(stored);
    native_xml::attached_append(ctx, object, property, value, span);
    let stored_slots = ctx.initialized_slots_snapshot();
    jump(ctx, done);
    ctx.builder.position_at_end(getter);
    ctx.restore_initialized_slots(initial);
    refuse_missing_getter(ctx, span);
    let getter_slots = ctx.initialized_slots_snapshot();
    jump(ctx, done);
    ctx.builder.position_at_end(done);
    ctx.restore_initialized_slots(stored_slots.intersection(&getter_slots).copied().collect());
    true
}

/// Refuses a missing overloaded lvalue until receiver-to-result lifetime handoff is supported.
fn refuse_missing_getter(ctx: &mut LoweringContext<'_, '_>, span: Span) {
    lower_throw_access_exception(ctx, "Error", "Unsupported property append through an overloaded __get()", span);
}

/// Interns an immutable property name for a raw dynamic-hash presence probe.
fn string(ctx: &mut LoweringContext<'_, '_>, value: &str, span: Span) -> LoweredValue {
    let data = ctx.intern_string(value);
    ctx.emit_value(Op::ConstStr, vec![], Some(Immediate::Data(data)), PhpType::Str,
        Op::ConstStr.default_effects(), Some(span))
}

/// Ends a returning arm while leaving an unsupported fetch terminated.
fn jump(ctx: &mut LoweringContext<'_, '_>, target: BlockId) {
    if !ctx.builder.insertion_block_is_terminated() {
        ctx.builder.terminate(Terminator::Br { target, args: vec![] });
    }
}
