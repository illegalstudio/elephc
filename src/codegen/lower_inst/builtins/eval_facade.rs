//! Purpose:
//! Forwards eval-related EIR operations to the eval lowering implementation.
//!
//! Called from:
//! - `super` and `crate::codegen::lower_inst` dispatchers.
//!
//! Key details:
//! - Keeps the public lowering surface stable while the eval implementation remains internally partitioned.

use super::*;

/// Routes one eval bridge status through the shared fatal/throwable handling.
pub(in crate::codegen::lower_inst) fn emit_eval_bridge_status_check(
    ctx: &mut FunctionContext<'_>,
) {
    eval::emit_eval_status_check(ctx);
}

/// Lowers a statically-known eval fragment through the current bridge fallback path.
pub(in crate::codegen::lower_inst) fn lower_eval_literal_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    eval::lower_eval(ctx, inst)
}

/// Lowers a direct EIR eval-scope lookup by static variable name.
pub(in crate::codegen::lower_inst) fn lower_eval_scope_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    eval::lower_eval_scope_get(ctx, inst)
}

/// Lowers a direct EIR eval-scope write by static variable name.
pub(in crate::codegen::lower_inst) fn lower_eval_scope_set(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    eval::lower_eval_scope_set(ctx, inst)
}

/// Lowers a native call to a zero-argument function declared through `eval()`.
pub(in crate::codegen::lower_inst) fn lower_eval_function_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    eval::lower_eval_function_call(ctx, inst)
}

/// Lowers a post-eval function call whose arguments are packed in an array/hash container.
pub(in crate::codegen::lower_inst) fn lower_eval_function_call_array(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    eval::lower_eval_function_call_array(ctx, inst)
}

/// Lowers post-eval object construction for classes declared by eval fragments.
pub(in crate::codegen::lower_inst) fn lower_eval_object_new(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    eval::lower_eval_object_new(ctx, inst)
}

/// Lowers fallback construction of a runtime class name through eval dynamic metadata.
pub(in crate::codegen::lower_inst) fn lower_eval_object_new_dynamic_fallback(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    miss_label: &str,
) -> Result<()> {
    eval::lower_eval_object_new_dynamic_fallback(ctx, inst, miss_label)
}

/// Lowers a post-eval method call that may target an eval-created object.
pub(in crate::codegen::lower_inst) fn lower_eval_method_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    method_name: &str,
) -> Result<()> {
    eval::lower_eval_method_call(ctx, inst, object, method_name)
}

/// Lowers a post-eval static method call to an eval-declared class.
pub(in crate::codegen::lower_inst) fn lower_eval_static_method_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    class_name: &str,
    method_name: &str,
) -> Result<()> {
    eval::lower_eval_static_method_call(ctx, inst, class_name, method_name)
}

/// Lowers a late-static AOT-frame static method call through an active eval override.
pub(in crate::codegen::lower_inst) fn lower_eval_native_frame_static_method_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    frame_class: &str,
    method_name: &str,
    no_override_label: &str,
    done_label: &str,
) -> Result<()> {
    eval::lower_eval_native_frame_static_method_call(
        ctx,
        inst,
        frame_class,
        method_name,
        no_override_label,
        done_label,
    )
}

/// Lowers a late-static AOT-frame static-property read through an active eval override.
pub(in crate::codegen::lower_inst) fn lower_eval_native_frame_static_property_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    frame_class: &str,
    property_name: &str,
    no_override_label: &str,
    done_label: &str,
) -> Result<()> {
    eval::lower_eval_native_frame_static_property_get(
        ctx,
        inst,
        frame_class,
        property_name,
        no_override_label,
        done_label,
    )
}

/// Lowers a late-static AOT-frame static-property write through an active eval override.
pub(in crate::codegen::lower_inst) fn lower_eval_native_frame_static_property_set(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    value: ValueId,
    frame_class: &str,
    property_name: &str,
    no_override_label: &str,
    done_label: &str,
) -> Result<()> {
    eval::lower_eval_native_frame_static_property_set(
        ctx,
        inst,
        value,
        frame_class,
        property_name,
        no_override_label,
        done_label,
    )
}

/// Lowers post-eval callable-array dispatch against eval dynamic callables.
pub(in crate::codegen::lower_inst) fn lower_eval_callable_call_array(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    callback: ValueId,
    arg_array: ValueId,
) -> Result<()> {
    eval::lower_eval_callable_call_array(ctx, inst, callback, arg_array)
}

/// Lowers post-eval callable probes against eval dynamic callables.
pub(in crate::codegen::lower_inst) fn lower_eval_is_callable(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    callback: ValueId,
) -> Result<()> {
    eval::lower_eval_is_callable(ctx, inst, callback)
}

/// Lowers post-eval member-existence probes against eval dynamic metadata.
pub(in crate::codegen::lower_inst) fn lower_eval_member_exists(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: ValueId,
    member: ValueId,
    name: &str,
) -> Result<()> {
    eval::lower_eval_member_exists(ctx, inst, target, member, name)
}

/// Lowers post-eval class-relation probes against eval dynamic metadata.
pub(in crate::codegen::lower_inst) fn lower_eval_class_relation(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: ValueId,
    name: &str,
) -> Result<()> {
    eval::lower_eval_class_relation(ctx, inst, target, name)
}

/// Lowers post-eval object class-name introspection against eval dynamic metadata.
pub(in crate::codegen::lower_inst) fn lower_eval_object_class_name(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    name: &str,
) -> Result<()> {
    eval::lower_eval_object_class_name(ctx, inst, object, name)
}

/// Lowers post-eval object/class relation predicates against eval dynamic metadata.
pub(in crate::codegen::lower_inst) fn lower_eval_object_is_a(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    target_class: &str,
    exclude_self: bool,
) -> Result<()> {
    eval::lower_eval_object_is_a(ctx, inst, object, target_class, exclude_self)
}

/// Lowers post-eval object/class relation predicates with runtime target cells.
pub(in crate::codegen::lower_inst) fn lower_eval_object_is_a_dynamic(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    target: ValueId,
    exclude_self: bool,
) -> Result<()> {
    eval::lower_eval_object_is_a_dynamic(ctx, inst, object, target, exclude_self)
}

/// Returns true when this lowered function has a persistent eval context local.
pub(in crate::codegen::lower_inst) fn has_eval_context(ctx: &FunctionContext<'_>) -> bool {
    eval::has_eval_context(ctx)
}

/// Lowers post-eval dynamic function existence probes to the optional eval bridge.
pub(in crate::codegen::lower_inst) fn lower_eval_function_exists(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    eval::lower_eval_function_exists(ctx, inst)
}

/// Lowers post-eval dynamic class existence probes to the optional eval bridge.
pub(in crate::codegen::lower_inst) fn lower_eval_class_exists(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    eval::lower_eval_class_exists(ctx, inst)
}

/// Lowers post-eval dynamic constant existence probes to the optional eval bridge.
pub(in crate::codegen::lower_inst) fn lower_eval_constant_exists(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    eval::lower_eval_constant_exists(ctx, inst)
}

/// Lowers post-eval dynamic constant fetches to the optional eval bridge.
pub(in crate::codegen::lower_inst) fn lower_eval_constant_fetch(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    eval::lower_eval_constant_fetch(ctx, inst)
}

/// Lowers post-eval class-like constant fetches to the optional eval bridge.
pub(in crate::codegen::lower_inst) fn lower_eval_class_constant_fetch(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    class_name: &str,
    constant_name: &str,
) -> Result<()> {
    eval::lower_eval_class_constant_fetch(ctx, inst, class_name, constant_name)
}

/// Lowers post-eval static-property reads to the optional eval bridge.
pub(in crate::codegen::lower_inst) fn lower_eval_static_property_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    class_name: &str,
    property_name: &str,
) -> Result<()> {
    eval::lower_eval_static_property_get(ctx, inst, class_name, property_name)
}

/// Lowers post-eval static-property writes to the optional eval bridge.
pub(in crate::codegen::lower_inst) fn lower_eval_static_property_set(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    value: ValueId,
    class_name: &str,
    property_name: &str,
) -> Result<()> {
    eval::lower_eval_static_property_set(ctx, inst, value, class_name, property_name)
}
