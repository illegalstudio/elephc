//! Purpose:
//! Forwards eval-related EIR operations to the eval lowering implementation.
//!
//! Called from:
//! - `super` and `crate::codegen::lower_inst` dispatchers.
//!
//! Key details:
//! - Keeps the public lowering surface stable while the eval implementation remains internally partitioned.

use super::*;

/// Lowers a resolver-preserved dynamic include/require through the eval bridge.
pub(in crate::codegen::lower_inst) fn lower_dynamic_include(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    once: bool,
    required: bool,
    strict_php: bool,
    value_discarded: bool,
) -> Result<()> {
    eval::lower_dynamic_include(ctx, inst, once, required, strict_php, value_discarded)
}

/// Appends live eval declarations to the current owned native inventory container.
pub(in crate::codegen::lower_inst) fn append_eval_inventory(
    ctx: &mut FunctionContext<'_>,
    constants: bool,
) -> Result<()> {
    eval::append_eval_inventory(ctx, constants)
}

/// Emits the body of the shared eval status dispatch helper.
pub(in crate::codegen) fn emit_eval_status_helper_body(ctx: &mut FunctionContext<'_>) {
    eval::emit_eval_status_helper_body(ctx)
}

/// Registers the native catalog into the eval context passed in argument register 0.
///
/// This is the body of the module-level catalog helper; it reads the handle from
/// the module-level parking symbol rather than from a caller frame slot.
pub(in crate::codegen) fn register_eval_inventory(ctx: &mut FunctionContext<'_>) -> Result<()> {
    eval::register_eval_inventory(ctx)
}

/// Lowers a statically-known eval fragment through the current bridge fallback path.
pub(in crate::codegen::lower_inst) fn lower_eval_literal_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    eval::lower_eval(ctx, inst)
}

/// Lowers `extract()` through the eval bridge's materialized activation scope.
pub(in crate::codegen::lower_inst) fn lower_extract(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    eval::lower_extract(ctx, inst)
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

/// Lowers a runtime-metadata construction while preserving a statically typed object result.
pub(in crate::codegen::lower_inst) fn lower_eval_native_object_new(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    eval::lower_eval_native_object_new(ctx, inst)
}

/// Probes a nominal class through the eval bridge before native missing-class handling.
pub(in crate::codegen::lower_inst) fn lower_eval_native_object_new_fallback(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    miss_label: &str,
) -> Result<()> {
    eval::lower_eval_native_object_new_fallback(ctx, inst, miss_label)
}

/// Lowers fallback construction of a runtime class name through eval dynamic metadata.
pub(in crate::codegen::lower_inst) fn lower_eval_object_new_dynamic_fallback(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    miss_label: &str,
) -> Result<()> {
    eval::lower_eval_object_new_dynamic_fallback(ctx, inst, miss_label)
}

/// Lowers the eval fallback of a constructorless dynamic allocation.
pub(in crate::codegen::lower_inst) fn lower_eval_object_new_without_constructor_fallback(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    miss_label: &str,
) -> Result<()> {
    eval::lower_eval_object_new_without_constructor_fallback(ctx, inst, miss_label)
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

/// Reads an instance property through the active eval context.
pub(in crate::codegen::lower_inst) fn lower_eval_property_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    property: &str,
) -> Result<()> {
    eval::lower_eval_property_get(ctx, inst, object, property)
}

/// Probes a native receiver before routing an instance-property read through eval.
pub(in crate::codegen::lower_inst) fn lower_eval_owned_property_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    property: &str,
    miss_label: &str,
    done_label: &str,
) -> Result<()> {
    eval::lower_eval_owned_property_get(ctx, inst, object, property, miss_label, done_label)
}

/// Reads a `Mixed` receiver's property through its owning eval context when one owns it, from a
/// frame without an eval context; other receivers branch to `miss_label`.
pub(in crate::codegen::lower_inst) fn lower_eval_owner_mixed_property_get(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    property: &str,
    miss_label: &str,
    done_label: &str,
) -> Result<()> {
    eval::lower_eval_owner_mixed_property_get(ctx, object, property, miss_label, done_label)
}

/// Reads a runtime-named property of an eval-owned object reached through a `Mixed` receiver.
pub(in crate::codegen::lower_inst) fn lower_eval_owner_mixed_dynamic_property_get(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    property_value: ValueId,
    miss_label: &str,
    done_label: &str,
) -> Result<()> {
    eval::lower_eval_owner_mixed_dynamic_property_get(
        ctx,
        object,
        property_value,
        miss_label,
        done_label,
    )
}

/// Probes a typed receiver for an eval-owned dynamic override before native dispatch.
pub(in crate::codegen::lower_inst) fn lower_eval_owned_method_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    method_name: &str,
    miss_label: &str,
    done_label: &str,
) -> Result<()> {
    eval::lower_eval_owned_method_call(ctx, inst, object, method_name, miss_label, done_label)
}

/// [`lower_eval_owned_method_call`] with a caller-chosen result type, optionally left unstored.
pub(in crate::codegen::lower_inst) fn lower_eval_owned_method_call_with_result(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    method_name: &str,
    miss_label: &str,
    done_label: &str,
    result_type: &PhpType,
    store_result: bool,
) -> Result<()> {
    eval::lower_eval_owned_method_call_with_result(
        ctx,
        inst,
        object,
        method_name,
        miss_label,
        done_label,
        result_type,
        store_result,
    )
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

/// Lowers post-eval callable-array dispatch without a local eval context.
pub(in crate::codegen::lower_inst) fn lower_eval_global_callable_call_array(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    callback: ValueId,
    arg_array: ValueId,
) -> Result<()> {
    eval::lower_eval_global_callable_call_array(ctx, inst, callback, arg_array)
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

/// Emits a null-context member-existence probe for an object an eval context may own.
pub(in crate::codegen::lower_inst) fn emit_eval_member_exists_owner_probe(
    ctx: &mut FunctionContext<'_>,
    target: ValueId,
    member: ValueId,
    name: &str,
    matched_label: &str,
) -> Result<()> {
    eval::emit_eval_member_exists_owner_probe(ctx, target, member, name, matched_label)
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

/// Lowers a class-relation probe from a frame without an eval context, against the process-global
/// eval class metadata. The boxed answer is left in the result register.
pub(in crate::codegen::lower_inst) fn lower_eval_global_class_relation_to_result(
    ctx: &mut FunctionContext<'_>,
    target: ValueId,
    name: &str,
) -> Result<()> {
    eval::lower_eval_global_class_relation_to_result(ctx, target, name)
}

/// Emits a `defined()` / `constant()` lookup through the eval bridge's shared null-handle context.
pub(in crate::codegen::lower_inst) fn emit_eval_dynamic_constant_null_context(
    ctx: &mut FunctionContext<'_>,
    name: ValueId,
    want_value: bool,
) -> Result<()> {
    eval::emit_eval_dynamic_constant_null_context(ctx, name, want_value)
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

/// Emits a null-context eval ownership probe before native nominal matching.
pub(in crate::codegen::lower_inst) fn emit_eval_object_is_a_named_fallback(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    target_class: &str,
    matched_label: &str,
    native_fallback_label: &str,
    definite_no_label: &str,
) -> Result<()> {
    eval::emit_eval_object_is_a_named_fallback(
        ctx,
        object,
        target_class,
        matched_label,
        native_fallback_label,
        definite_no_label,
    )
}

/// Same probe for an object that is a boxed Mixed cell in the int result register.
pub(in crate::codegen::lower_inst) fn emit_eval_object_is_a_named_fallback_from_result(
    ctx: &mut FunctionContext<'_>,
    target_class: &str,
    matched_label: &str,
    native_fallback_label: &str,
    definite_no_label: &str,
) -> Result<()> {
    eval::emit_eval_object_is_a_named_fallback_from_result(
        ctx,
        target_class,
        matched_label,
        native_fallback_label,
        definite_no_label,
    )
}

/// Emits a null-context eval ownership probe against a runtime class-name string.
pub(in crate::codegen::lower_inst) fn emit_eval_object_is_a_dynamic_name_fallback(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    target: ValueId,
    target_ty: &crate::types::PhpType,
    matched_label: &str,
    native_fallback_label: &str,
) -> Result<()> {
    eval::emit_eval_object_is_a_dynamic_name_fallback(
        ctx,
        object,
        target,
        target_ty,
        matched_label,
        native_fallback_label,
    )
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

/// Returns true when the body being lowered is a shared owner-call helper.
pub(in crate::codegen::lower_inst) fn inside_owner_call_helper(ctx: &FunctionContext<'_>) -> bool {
    eval::inside_owner_call_helper(ctx)
}

/// Lowers the one instruction of a shared owner-call helper.
pub(in crate::codegen::lower_inst) fn lower_owner_call_helper_body(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    eval::lower_owner_call_helper_body(ctx, inst)
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
