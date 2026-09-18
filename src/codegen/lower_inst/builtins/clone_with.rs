//! Purpose:
//! Lowers the typed PHP 8.5 `clone()` runtime operation.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions::group_02` for
//!   `RuntimeFnId::CloneWith`.
//!
//! Key details:
//! - Runtime-class cloning reuses the boxed shallow-copy adapter shared with Magician,
//!   which is emitted once per program by the managed runtime on every supported target.
//! - The adapter is a C-ABI wrapper, so the boxed input travels in the first ARGUMENT
//!   register and the boxed clone comes back in the result register.
//! - A non-object input is rejected by inspecting the boxed runtime tag BEFORE the clone,
//!   so a bad argument raises `TypeError` while a class the adapter cannot copy raises
//!   `Error`. Both are ordinary catchable throwables.
//! - The fresh clone is published as a temporary unwind owner before `__clone()` runs, so a
//!   throwing hook releases it instead of leaking it.
//! - Hook selection uses the runtime class id while visibility follows the INVOCATION SITE's
//!   lexical class, exactly as php-src does. A direct call takes that scope statically from the
//!   function being lowered. A body that a callable dispatch shared across call sites entered
//!   cannot: it reads the trailing hidden ABI operand and compares it against the statically
//!   computed set of scopes the hook is visible from, so one escaped `clone(...)` reports
//!   `global scope` and `scope X` at the two places it is invoked from.
//! - An object Magician declared inside `eval()` has no generated class layout at all, so EVERY
//!   object operand is offered to the installed eval clone callback FIRST. A statically typed
//!   operand is offered too: a parameter annotated with an emitted base class can still hold an
//!   eval-declared SUBCLASS at run time. That callback owns the whole operation for an identity
//!   it recognizes: dynamic-class registration, reference-alias copying, eval `__clone()`
//!   dispatch, invocation-scope visibility, and PHP 8.5 overrides. A miss, or a program that
//!   never linked Magician, falls straight through to everything below.
//! - `$withProperties` is applied by `overrides`, after `__clone()` and while the clone is still
//!   an unwind-visible owner. A literal `[]` needs no code at all, and an omitted second operand
//!   IS the empty array by definition. A runtime class this program generated no applicator for
//!   still REPORTS rather than dropping the write.

use crate::codegen::abi;
use crate::codegen::context::FunctionContext;
use crate::codegen::platform::Arch;
use crate::codegen::{emit_box_current_value_as_mixed, CodegenIrError, Result};
use crate::ir::{Immediate, Instruction, Op, ValueDef, ValueId};
use crate::names::php_symbol_key;
use crate::parser::ast::Visibility;
use crate::types::PhpType;

pub(crate) mod overrides;

use super::super::{
    direct_call_stack_pad_bytes, emit_call_arg_temp_cleanups, emit_ref_arg_writebacks,
    emit_resolved_method_call, materialize_method_call_args_with_receiver_reg_and_refs,
    resolve_method_call_target, MethodCallTarget, RefArgCellLifetime,
};

const CLONE_OWNER_BYTES: usize = 16 + abi::CALL_OPERAND_OWNER_RECORD_BYTES;

/// Runtime class branch and the exact inherited clone-hook declaration it selects.
struct CloneHookCandidate {
    class_id: u64,
    class_name: String,
    declaring_class: String,
    visibility: Visibility,
    target: MethodCallTarget,
}

/// The message PHP-facing code sees when no generated applicator serves the clone's class.
const PROPERTY_OVERRIDE_MESSAGE: &str =
    "clone(): Argument #2 ($withProperties) property overrides are not supported for this class";

/// Clones a runtime object and invokes its visible `__clone()` hook.
pub(crate) fn lower_clone_with(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if !(1..=3).contains(&inst.operands.len()) {
        return Err(CodegenIrError::invalid_module(format!(
            "clone expected 1 to 2 PHP args and an optional hidden invocation scope, got {} operands",
            inst.operands.len()
        )));
    }
    let object = super::expect_operand(inst, 0)?;
    let object_ty = ctx.value_php_type(object)?.codegen_repr();
    if !matches!(
        object_ty,
        PhpType::Object(_) | PhpType::Mixed | PhpType::Union(_)
    ) {
        // A statically non-object argument is a run-time `TypeError` in PHP, not a compile
        // failure, and a runtime callable can reach this lowering without the checker's
        // argument rule ever running. Throw the catchable error instead of refusing the build.
        super::super::exceptions::emit_type_error(
            ctx,
            &format!(
                "clone(): Argument #1 ($object) must be of type object, {} given",
                static_type_name(&object_ty)
            ),
        );
        return super::store_if_result(ctx, inst);
    }

    emit_clone_object_type_guard(ctx, object)?;
    if let Some(properties) = inst.operands.get(1).copied() {
        emit_clone_properties_type_guard(ctx, properties)?;
    }
    // Both argument guards have already run, so an eval-owned identity reaches Magician with
    // exactly the errors PHP reports first. `eval_handled` lands on the shared result store.
    let eval_handled = ctx.next_label("clone_eval_bridge_handled");
    emit_eval_clone_bridge(
        ctx,
        object,
        inst.operands.get(1).copied(),
        inst.operands.get(2).copied(),
        &eval_handled,
    )?;
    emit_boxed_shallow_clone(ctx, object)?;
    emit_uncloneable_guard(ctx);

    let clone_box = abi::int_result_reg(ctx.emitter).to_string();
    abi::emit_reserve_temporary_stack(ctx.emitter, CLONE_OWNER_BYTES);
    abi::emit_store_to_sp(ctx.emitter, &clone_box, 0);
    let owner_addr = abi::symbol_scratch_reg(ctx.emitter).to_string();
    abi::emit_temporary_stack_address(ctx.emitter, &owner_addr, 0);
    abi::emit_link_call_operand_owner_at_stack(ctx.emitter, &owner_addr, false, 16);

    emit_enum_uncloneable_guard(ctx);
    emit_clone_hook(ctx, object, inst.operands.get(2).copied())?;
    // An omitted second argument IS the empty override array, so there is nothing to apply.
    if let Some(properties) = inst.operands.get(1).copied() {
        overrides::emit_property_overrides(
            ctx,
            properties,
            inst.operands.get(2).copied(),
            0,
        )?;
    }

    abi::emit_unlink_call_operand_owner_at_stack(ctx.emitter, 16);
    abi::emit_load_temporary_stack_slot(ctx.emitter, &clone_box, 0);
    abi::emit_release_temporary_stack(ctx.emitter, CLONE_OWNER_BYTES);
    // The eval callback returns its own finished clone box in the same register, with the
    // temporary stack already balanced, so both producers share one result store.
    ctx.emitter.label(&eval_handled);
    store_clone_result(ctx, inst)
}

/// Byte offsets inside the eval clone bridge's own temporary frame.
const EVAL_CLONE_OBJECT_OFFSET: usize = 0;
const EVAL_CLONE_PROPERTIES_OFFSET: usize = 8;
const EVAL_CLONE_OUT_OFFSET: usize = 16;
const EVAL_CLONE_THROWABLE_OFFSET: usize = 24;
const EVAL_CLONE_SCOPE_PTR_OFFSET: usize = 32;
const EVAL_CLONE_SCOPE_LEN_OFFSET: usize = 40;
const EVAL_CLONE_CALLBACK_OFFSET: usize = 48;
const EVAL_CLONE_STATUS_OFFSET: usize = 56;
const EVAL_CLONE_FRAME_BYTES: usize = 64;

/// Offers a Mixed operand to Magician's installed clone callback before the AOT path runs.
///
/// Status one means Magician owned the identity and produced the finished clone: the boxed clone
/// is left in the result register and control jumps to `handled`. Status two means eval
/// `__clone()` threw; the operation already released its own unfinished clone, so the owned
/// Throwable box is the only thing handed back and it is rethrown natively here, outside the
/// Rust frame. Status zero is a miss and simply falls through.
fn emit_eval_clone_bridge(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    properties: Option<ValueId>,
    invocation_scope: Option<ValueId>,
    handled: &str,
) -> Result<()> {
    let object_ty = ctx.value_php_type(object)?.codegen_repr();
    if !matches!(
        object_ty,
        PhpType::Object(_) | PhpType::Mixed | PhpType::Union(_)
    ) {
        return Ok(());
    }
    let absent = ctx.next_label("clone_eval_bridge_absent");
    let missed = ctx.next_label("clone_eval_bridge_missed");
    let threw = ctx.next_label("clone_eval_bridge_threw");
    let result_reg = abi::int_result_reg(ctx.emitter).to_string();
    let scratch = abi::secondary_scratch_reg(ctx.emitter).to_string();
    let callback_reg = abi::symbol_scratch_reg(ctx.emitter).to_string();

    // A program without the eval bridge linked leaves this slot zero, so nothing changes.
    abi::emit_symbol_address(ctx.emitter, &callback_reg, "_elephc_eval_object_clone_fn");
    abi::emit_load_from_address(ctx.emitter, &callback_reg, &callback_reg, 0);
    emit_branch_if_zero(ctx, &callback_reg, &absent);

    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_CLONE_FRAME_BYTES);
    abi::emit_store_to_sp(ctx.emitter, &callback_reg, EVAL_CLONE_CALLBACK_OFFSET);
    abi::emit_load_int_immediate(ctx.emitter, &scratch, 0);
    for offset in [
        EVAL_CLONE_PROPERTIES_OFFSET,
        EVAL_CLONE_OUT_OFFSET,
        EVAL_CLONE_THROWABLE_OFFSET,
        EVAL_CLONE_SCOPE_PTR_OFFSET,
        EVAL_CLONE_SCOPE_LEN_OFFSET,
    ] {
        abi::emit_store_to_sp(ctx.emitter, &scratch, offset);
    }

    // The callback keys on the raw object identity, exactly like the destructor bridge, and the
    // operand stays borrowed for the whole call. A concrete slot already holds that pointer; a
    // boxed one has to be unboxed first.
    ctx.load_value_to_result(object)?;
    if matches!(object_ty, PhpType::Object(_)) {
        abi::emit_store_to_sp(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            EVAL_CLONE_OBJECT_OFFSET,
        );
    } else {
        abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
        let payload_reg = crate::codegen_support::mixed_unbox_payload_reg(ctx.emitter.target);
        abi::emit_store_to_sp(ctx.emitter, payload_reg, EVAL_CLONE_OBJECT_OFFSET);
    }

    let owns_properties_box = emit_eval_clone_properties_operand(ctx, properties)?;
    emit_eval_clone_invocation_scope(ctx, invocation_scope)?;

    let target = ctx.emitter.target;
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(target, 0),
        EVAL_CLONE_OBJECT_OFFSET,
    );
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(target, 1),
        EVAL_CLONE_PROPERTIES_OFFSET,
    );
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(target, 2),
        EVAL_CLONE_SCOPE_PTR_OFFSET,
    );
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(target, 3),
        EVAL_CLONE_SCOPE_LEN_OFFSET,
    );
    abi::emit_temporary_stack_address(
        ctx.emitter,
        abi::int_arg_reg_name(target, 4),
        EVAL_CLONE_OUT_OFFSET,
    );
    abi::emit_temporary_stack_address(
        ctx.emitter,
        abi::int_arg_reg_name(target, 5),
        EVAL_CLONE_THROWABLE_OFFSET,
    );
    abi::emit_load_temporary_stack_slot(ctx.emitter, &scratch, EVAL_CLONE_CALLBACK_OFFSET);
    abi::emit_call_reg(ctx.emitter, &scratch);

    abi::emit_store_to_sp(ctx.emitter, &result_reg, EVAL_CLONE_STATUS_OFFSET);
    if owns_properties_box {
        // The Mixed box made only to carry a typed array argument is retired on every status.
        abi::emit_load_temporary_stack_slot(
            ctx.emitter,
            &result_reg,
            EVAL_CLONE_PROPERTIES_OFFSET,
        );
        abi::emit_decref_if_refcounted(ctx.emitter, &PhpType::Mixed);
    }
    abi::emit_load_temporary_stack_slot(ctx.emitter, &result_reg, EVAL_CLONE_STATUS_OFFSET);
    emit_branch_if_reg_equals_immediate(ctx, &result_reg, 2, &threw);
    emit_branch_if_zero(ctx, &result_reg, &missed);
    abi::emit_load_temporary_stack_slot(ctx.emitter, &result_reg, EVAL_CLONE_OUT_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_CLONE_FRAME_BYTES);
    abi::emit_jump(ctx.emitter, handled);

    ctx.emitter.label(&threw);
    abi::emit_load_temporary_stack_slot(ctx.emitter, &result_reg, EVAL_CLONE_THROWABLE_OFFSET);
    abi::emit_call_label(ctx.emitter, "__rt_throwable_take_boxed");
    abi::emit_store_reg_to_symbol(ctx.emitter, &result_reg, "_exc_value", 0);
    // The frame is balanced before unwinding, and the throw happens outside the Rust callback.
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_CLONE_FRAME_BYTES);
    abi::emit_call_label(ctx.emitter, "__rt_throw_current");

    ctx.emitter.label(&missed);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_CLONE_FRAME_BYTES);
    ctx.emitter.label(&absent);
    Ok(())
}

/// Stages the borrowed `$withProperties` box, reporting whether a temporary box was created.
fn emit_eval_clone_properties_operand(
    ctx: &mut FunctionContext<'_>,
    properties: Option<ValueId>,
) -> Result<bool> {
    // An omitted second argument IS the empty override array, so the slot stays null.
    let Some(properties) = properties else {
        return Ok(false);
    };
    let properties_ty = ctx.value_php_type(properties)?.codegen_repr();
    ctx.load_value_to_result(properties)?;
    let owns_box = !matches!(properties_ty, PhpType::Mixed | PhpType::Union(_));
    if owns_box {
        emit_box_current_value_as_mixed(ctx.emitter, &properties_ty);
    }
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        EVAL_CLONE_PROPERTIES_OFFSET,
    );
    Ok(owns_box)
}

/// Stages the caller's lexical AOT class name, which PHP checks `__clone()` visibility against.
///
/// A direct site knows its own scope statically. A callable wrapper body shared across call
/// sites reads the trailing hidden operand instead and maps that dense class id back to its
/// name, so a method that invoked `clone(...)` never reports global scope.
fn emit_eval_clone_invocation_scope(
    ctx: &mut FunctionContext<'_>,
    invocation_scope: Option<ValueId>,
) -> Result<()> {
    let Some(invocation_scope) = invocation_scope else {
        let Some(lexical_class) = ctx.function.lexical_class.clone() else {
            return Ok(());
        };
        emit_store_eval_clone_scope_name(ctx, &lexical_class);
        return Ok(());
    };
    let mut scopes = ctx
        .module
        .class_infos
        .iter()
        .map(|(name, info)| (info.class_id as i64, name.clone()))
        .collect::<Vec<_>>();
    scopes.sort_by_key(|(class_id, _)| *class_id);
    if scopes.is_empty() {
        return Ok(());
    }
    let done = ctx.next_label("clone_eval_bridge_scope_ready");
    let labels = scopes
        .iter()
        .map(|_| ctx.next_label("clone_eval_bridge_scope"))
        .collect::<Vec<_>>();
    ctx.load_value_to_result(invocation_scope)?;
    let scope_reg = abi::int_result_reg(ctx.emitter).to_string();
    for ((class_id, _), label) in scopes.iter().zip(labels.iter()) {
        emit_branch_if_reg_equals_immediate(ctx, &scope_reg, *class_id, label);
    }
    // No dense id matched, so the call really did come from global scope and the slots stay null.
    abi::emit_jump(ctx.emitter, &done);
    for ((_, class_name), label) in scopes.iter().zip(labels.iter()) {
        ctx.emitter.label(label);
        emit_store_eval_clone_scope_name(ctx, class_name);
        abi::emit_jump(ctx.emitter, &done);
    }
    ctx.emitter.label(&done);
    Ok(())
}

/// Writes one interned class name and its byte length into the bridge's scope slots.
fn emit_store_eval_clone_scope_name(ctx: &mut FunctionContext<'_>, class_name: &str) {
    let (label, len) = ctx.data.add_string(class_name.as_bytes());
    let scratch = abi::secondary_scratch_reg(ctx.emitter).to_string();
    abi::emit_symbol_address(ctx.emitter, &scratch, &label);
    abi::emit_store_to_sp(ctx.emitter, &scratch, EVAL_CLONE_SCOPE_PTR_OFFSET);
    abi::emit_load_int_immediate(ctx.emitter, &scratch, len as i64);
    abi::emit_store_to_sp(ctx.emitter, &scratch, EVAL_CLONE_SCOPE_LEN_OFFSET);
}

/// Refuses property overrides no generated applicator can apply, as early as the operand allows.
pub(super) fn emit_property_override_guard(
    ctx: &mut FunctionContext<'_>,
    properties: ValueId,
) -> Result<()> {
    if value_is_empty_array_literal(ctx, properties)? {
        return Ok(());
    }
    let ty = ctx.value_php_type(properties)?.codegen_repr();
    let empty_label = ctx.next_label("clone_properties_empty");
    let result_reg = abi::int_result_reg(ctx.emitter).to_string();
    match ty {
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            ctx.load_value_to_result(properties)?;
            let scratch_reg = abi::secondary_scratch_reg(ctx.emitter).to_string();
            // A missed read forwarded as the null-container sentinel carries no overrides.
            crate::codegen::sentinels::emit_branch_if_null_container(
                ctx.emitter,
                &result_reg,
                &scratch_reg,
                &empty_label,
            );
            abi::emit_load_from_address(ctx.emitter, &result_reg, &result_reg, 0);
        }
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.load_value_to_result(properties)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_count");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "clone() property override operand of type {other:?}",
            )))
        }
    }
    emit_branch_if_zero(ctx, &result_reg, &empty_label);
    super::super::exceptions::emit_error(ctx, PROPERTY_OVERRIDE_MESSAGE);
    ctx.emitter.label(&empty_label);
    Ok(())
}

/// Produces a boxed clone while retiring a temporary box made for a concrete object input.
fn emit_boxed_shallow_clone(ctx: &mut FunctionContext<'_>, object: ValueId) -> Result<()> {
    let object_ty = ctx.value_php_type(object)?.codegen_repr();
    match object_ty {
        PhpType::Object(_) => {
            // A concrete object is statically known to satisfy the argument type, so the
            // only work here is handing the adapter a Mixed box and retiring it afterwards.
            ctx.load_value_to_result(object)?;
            emit_box_current_value_as_mixed(ctx.emitter, &object_ty);
            let input_box = abi::int_result_reg(ctx.emitter).to_string();
            let saved_clone = abi::nested_call_reg(ctx.emitter).to_string();
            abi::emit_reserve_temporary_stack(ctx.emitter, 16);
            abi::emit_store_to_sp(ctx.emitter, &input_box, 0);
            emit_clone_adapter_call(ctx);
            abi::emit_reg_move(ctx.emitter, &saved_clone, &input_box);
            abi::emit_load_temporary_stack_slot(ctx.emitter, &input_box, 0);
            abi::emit_decref_if_refcounted(ctx.emitter, &PhpType::Mixed);
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            abi::emit_reg_move(ctx.emitter, &input_box, &saved_clone);
            Ok(())
        }
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.load_value_to_result(object)?;
            emit_clone_adapter_call(ctx);
            Ok(())
        }
        other => Err(CodegenIrError::invalid_module(format!(
            "clone() received non-object EIR operand {other:?}",
        ))),
    }
}

/// Validates the first argument before the shallow clone is allocated.
fn emit_clone_object_type_guard(ctx: &mut FunctionContext<'_>, object: ValueId) -> Result<()> {
    match ctx.value_php_type(object)?.codegen_repr() {
        PhpType::Object(_) => Ok(()),
        PhpType::Mixed | PhpType::Union(_) => emit_mixed_object_type_guard(ctx, object),
        other => Err(CodegenIrError::invalid_module(format!(
            "clone() received non-object EIR operand {other:?}",
        ))),
    }
}

/// Validates PHP 8.5's override array before allocating the shallow clone or invoking `__clone`.
fn emit_clone_properties_type_guard(
    ctx: &mut FunctionContext<'_>,
    properties: ValueId,
) -> Result<()> {
    let properties_ty = ctx.value_php_type(properties)?.codegen_repr();
    match properties_ty {
        PhpType::Array(_) | PhpType::AssocArray { .. } => return Ok(()),
        PhpType::Mixed | PhpType::Union(_) => {}
        other => {
            super::super::exceptions::emit_type_error(
                ctx,
                &format!(
                    "clone(): Argument #2 ($withProperties) must be of type array, {} given",
                    static_type_name(&other)
                ),
            );
            return Ok(());
        }
    }

    let valid = ctx.next_label("clone_properties_array");
    let int_case = ctx.next_label("clone_properties_int");
    let string_case = ctx.next_label("clone_properties_string");
    let float_case = ctx.next_label("clone_properties_float");
    let bool_case = ctx.next_label("clone_properties_bool");
    let true_case = ctx.next_label("clone_properties_true");
    let object_case = ctx.next_label("clone_properties_object");
    let resource_case = ctx.next_label("clone_properties_resource");
    let callable_case = ctx.next_label("clone_properties_callable");
    ctx.load_value_to_result(properties)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    for tag in [4u8, 5] {
        super::scalar_metadata::emit_branch_on_gettype_mixed_tag(ctx, tag, &valid);
    }
    super::scalar_metadata::emit_branch_on_gettype_mixed_tag(ctx, 0, &int_case);
    super::scalar_metadata::emit_branch_on_gettype_mixed_tag(ctx, 1, &string_case);
    super::scalar_metadata::emit_branch_on_gettype_mixed_tag(ctx, 2, &float_case);
    super::scalar_metadata::emit_branch_on_gettype_mixed_tag(ctx, 3, &bool_case);
    super::scalar_metadata::emit_branch_on_gettype_mixed_tag(ctx, 6, &object_case);
    super::scalar_metadata::emit_branch_on_gettype_mixed_tag(ctx, 9, &resource_case);
    super::scalar_metadata::emit_branch_on_gettype_mixed_tag(ctx, 10, &callable_case);
    emit_clone_properties_type_error(ctx, "null");

    ctx.emitter.label(&int_case);
    emit_clone_properties_type_error(ctx, "int");
    ctx.emitter.label(&string_case);
    emit_clone_properties_type_error(ctx, "string");
    ctx.emitter.label(&float_case);
    emit_clone_properties_type_error(ctx, "float");
    ctx.emitter.label(&object_case);
    emit_clone_properties_type_error(ctx, "object");
    ctx.emitter.label(&resource_case);
    emit_clone_properties_type_error(ctx, "resource");
    ctx.emitter.label(&callable_case);
    emit_clone_properties_type_error(ctx, "Closure");

    ctx.emitter.label(&bool_case);
    let payload = crate::codegen_support::mixed_unbox_payload_reg(ctx.emitter.target);
    abi::emit_reg_move(ctx.emitter, abi::int_result_reg(ctx.emitter), payload);
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, &true_case);
    emit_clone_properties_type_error(ctx, "false");
    ctx.emitter.label(&true_case);
    emit_clone_properties_type_error(ctx, "true");
    ctx.emitter.label(&valid);
    Ok(())
}

/// Raises the runtime second-argument TypeError for one concrete Mixed payload tag.
fn emit_clone_properties_type_error(ctx: &mut FunctionContext<'_>, type_name: &str) {
    super::super::exceptions::emit_type_error(
        ctx,
        &format!(
            "clone(): Argument #2 ($withProperties) must be of type array, {type_name} given"
        ),
    );
}

/// Names a statically known argument type the way PHP's `TypeError` wording does.
fn static_type_name(ty: &PhpType) -> &str {
    match ty {
        PhpType::Int => "int",
        PhpType::Float => "float",
        PhpType::Str => "string",
        PhpType::Bool | PhpType::False => "bool",
        PhpType::Void | PhpType::Never => "null",
        PhpType::Array(_) | PhpType::AssocArray { .. } => "array",
        PhpType::Object(name) | PhpType::Packed(name) => name,
        PhpType::Resource(_) | PhpType::Buffer(_) | PhpType::Pointer(_) => "resource",
        PhpType::Callable => "Closure",
        PhpType::Iterable => "object",
        PhpType::Mixed | PhpType::Union(_) | PhpType::TaggedScalar => "mixed",
    }
}

/// Rejects a runtime-shaped argument whose boxed tag is not an object, leaving the box loaded.
fn emit_mixed_object_type_guard(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
) -> Result<()> {
    let result_reg = abi::int_result_reg(ctx.emitter).to_string();
    let object_label = ctx.next_label("clone_argument_object");
    ctx.load_value_to_result(object)?;
    abi::emit_reserve_temporary_stack(ctx.emitter, 16);
    abi::emit_store_to_sp(ctx.emitter, &result_reg, 0);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_branch_if_tag_is_object(ctx, &result_reg, &object_label);
    super::super::exceptions::emit_type_error(
        ctx,
        "clone(): Argument #1 ($object) must be of type object",
    );
    ctx.emitter.label(&object_label);
    abi::emit_load_temporary_stack_slot(ctx.emitter, &result_reg, 0);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    Ok(())
}

/// Calls the shared boxed shallow-clone adapter through its first argument register.
fn emit_clone_adapter_call(ctx: &mut FunctionContext<'_>) {
    let arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let result_reg = abi::int_result_reg(ctx.emitter).to_string();
    abi::emit_reg_move(ctx.emitter, arg_reg, &result_reg);
    abi::emit_call_label(ctx.emitter, "__rt_object_clone_shallow_boxed");
}

/// Turns the adapter's null sentinel into PHP's catchable uncloneable-object `Error`.
fn emit_uncloneable_guard(ctx: &mut FunctionContext<'_>) {
    let clone_box = abi::int_result_reg(ctx.emitter).to_string();
    let cloned_label = ctx.next_label("clone_object_cloned");
    emit_branch_if_nonzero(ctx, &clone_box, &cloned_label);
    super::super::exceptions::emit_error(ctx, "Trying to clone an uncloneable object");
    ctx.emitter.label(&cloned_label);
}

/// Refuses an enum case, which php forbids cloning, naming the enum the way php-src does.
///
/// The shared shallow-copy adapter happily copies an enum case, so the refusal is decided here
/// from the clone's own runtime class id. It runs INSIDE the owner window, so the copy the
/// adapter already made is released by the unwinder instead of leaking.
fn emit_enum_uncloneable_guard(ctx: &mut FunctionContext<'_>) {
    let mut enums = ctx
        .module
        .enum_infos
        .keys()
        .filter_map(|name| {
            ctx.module
                .class_infos
                .get(name)
                .map(|info| (info.class_id as i64, name.clone()))
        })
        .collect::<Vec<_>>();
    if enums.is_empty() {
        return;
    }
    enums.sort_by(|left, right| left.0.cmp(&right.0));
    let cloneable = ctx.next_label("clone_not_an_enum");
    let labels = enums
        .iter()
        .map(|(class_id, _)| ctx.next_label(&format!("clone_enum_{class_id}")))
        .collect::<Vec<_>>();
    let class_id_reg = abi::symbol_scratch_reg(ctx.emitter).to_string();
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    let payload_reg = crate::codegen_support::mixed_unbox_payload_reg(ctx.emitter.target);
    abi::emit_load_from_address(ctx.emitter, &class_id_reg, payload_reg, 0);
    for ((class_id, _), label) in enums.iter().zip(labels.iter()) {
        emit_branch_if_reg_equals_immediate(ctx, &class_id_reg, *class_id, label);
    }
    abi::emit_jump(ctx.emitter, &cloneable);
    for ((_, name), label) in enums.iter().zip(labels.iter()) {
        ctx.emitter.label(label);
        super::super::exceptions::emit_error(
            ctx,
            &format!("Trying to clone an uncloneable object of class {name}"),
        );
    }
    ctx.emitter.label(&cloneable);
}

/// Dispatches `__clone()` by runtime class id, or does nothing when the class has no hook.
fn emit_clone_hook(
    ctx: &mut FunctionContext<'_>,
    object_operand: ValueId,
    invocation_scope: Option<ValueId>,
) -> Result<()> {
    let candidates = clone_hook_candidates(ctx)?;
    if candidates.is_empty() {
        return Ok(());
    }
    let receiver_reg = abi::nested_call_reg(ctx.emitter).to_string();
    let no_hook = ctx.next_label("clone_hook_absent");
    let done = ctx.next_label("clone_hook_done");
    let labels = candidates
        .iter()
        .map(|candidate| ctx.next_label(&format!("clone_hook_{}", candidate.class_id)))
        .collect::<Vec<_>>();

    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    // The unboxed payload low word is the clone's object pointer; park it in the
    // callee-saved nested-call register the receiver-register contract requires.
    let payload_reg = crate::codegen_support::mixed_unbox_payload_reg(ctx.emitter.target);
    abi::emit_reg_move(ctx.emitter, &receiver_reg, payload_reg);
    emit_clone_hook_class_dispatch(ctx, &receiver_reg, &candidates, &labels, &no_hook);
    abi::emit_jump(ctx.emitter, &no_hook);

    for (candidate, label) in candidates.iter().zip(labels.iter()) {
        ctx.emitter.label(label);
        if let Some(invocation_scope) = invocation_scope {
            emit_runtime_clone_hook_visibility_guard(
                ctx,
                candidate,
                invocation_scope,
            )?;
        } else if !clone_hook_is_visible(ctx, candidate) {
            emit_static_clone_hook_visibility_error(ctx, candidate);
            continue;
        }
        let receiver_ty = PhpType::Object(candidate.class_name.clone());
        let params = [receiver_ty.clone()];
        let refs = [false];
        let operands = [object_operand];
        let call_args = materialize_method_call_args_with_receiver_reg_and_refs(
            ctx,
            &receiver_reg,
            &receiver_ty,
            &operands,
            &params,
            &refs,
            RefArgCellLifetime::CallOnly,
        )?;
        let pad = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
        abi::emit_reserve_temporary_stack(ctx.emitter, pad);
        emit_resolved_method_call(ctx, &candidate.target)?;
        abi::emit_release_temporary_stack(ctx.emitter, pad);
        abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
        emit_call_arg_temp_cleanups(ctx, &call_args, None)?;
        emit_ref_arg_writebacks(ctx, &call_args)?;
        abi::emit_jump(ctx.emitter, &done);
    }
    ctx.emitter.label(&no_hook);
    ctx.emitter.label(&done);
    Ok(())
}

/// Collects clone hooks for every runtime class, including an inherited private hook.
///
/// Normal method maps intentionally omit inaccessible parent-private methods. Cloning differs:
/// PHP still selects that hook and then performs the invocation-site visibility check, producing
/// `Call to private method ...` instead of silently skipping it on a child object.
fn clone_hook_candidates(ctx: &FunctionContext<'_>) -> Result<Vec<CloneHookCandidate>> {
    let method_key = php_symbol_key("__clone");
    let mut candidates = Vec::new();
    for (runtime_class, runtime_info) in &ctx.module.class_infos {
        let mut owner = Some(runtime_class.as_str());
        while let Some(class_name) = owner {
            let Some(info) = ctx.module.class_infos.get(class_name) else {
                break;
            };
            if info.methods.contains_key(&method_key) {
                let target = resolve_method_call_target(ctx, class_name, "__clone", 1)?;
                candidates.push(CloneHookCandidate {
                    class_id: runtime_info.class_id,
                    class_name: runtime_class.clone(),
                    declaring_class: info
                        .method_declaring_classes
                        .get(&method_key)
                        .cloned()
                        .unwrap_or_else(|| class_name.to_string()),
                    visibility: info
                        .method_visibilities
                        .get(&method_key)
                        .cloned()
                        .unwrap_or(Visibility::Public),
                    target,
                });
                break;
            }
            owner = info.parent.as_deref();
        }
    }
    candidates.sort_by_key(|candidate| candidate.class_id);
    Ok(candidates)
}

/// Dispatches the cloned receiver's dense class id to its resolved hook branch.
fn emit_clone_hook_class_dispatch(
    ctx: &mut FunctionContext<'_>,
    receiver_reg: &str,
    candidates: &[CloneHookCandidate],
    labels: &[String],
    no_hook: &str,
) {
    let (class_id_reg, compare_reg) = match ctx.emitter.target.arch {
        Arch::AArch64 => ("x9", "x10"),
        Arch::X86_64 => ("r11", "r10"),
    };
    abi::emit_load_from_address(ctx.emitter, class_id_reg, receiver_reg, 0);
    for (candidate, label) in candidates.iter().zip(labels.iter()) {
        abi::emit_load_int_immediate(ctx.emitter, compare_reg, candidate.class_id as i64);
        ctx.emitter
            .instruction(&format!("cmp {class_id_reg}, {compare_reg}"));
        match ctx.emitter.target.arch {
            Arch::AArch64 => ctx.emitter.instruction(&format!("b.eq {label}")),
            Arch::X86_64 => ctx.emitter.instruction(&format!("je {label}")),
        }
    }
    abi::emit_jump(ctx.emitter, no_hook);
}

/// Checks a callable wrapper's invocation scope against the selected hook declaration.
fn emit_runtime_clone_hook_visibility_guard(
    ctx: &mut FunctionContext<'_>,
    candidate: &CloneHookCandidate,
    invocation_scope: ValueId,
) -> Result<()> {
    let visibility = candidate.visibility.clone();
    if visibility == Visibility::Public {
        return Ok(());
    }
    let declaring = candidate.declaring_class.clone();
    let declaring_id = ctx
        .module
        .class_infos
        .get(&declaring)
        .map(|class| class.class_id as i64);
    let visible = ctx.next_label("clone_hook_scope_visible");
    ctx.load_value_to_result(invocation_scope)?;
    let scope_reg = abi::int_result_reg(ctx.emitter).to_string();
    let mut allowed = ctx
        .module
        .class_infos
        .iter()
        .filter_map(|(name, info)| {
            let permitted = match visibility {
                Visibility::Public => true,
                Visibility::Private => Some(info.class_id as i64) == declaring_id,
                Visibility::Protected => {
                    name == &declaring
                        || class_is_subclass_of(ctx, name, &declaring)
                        || class_is_subclass_of(ctx, &declaring, name)
                }
            };
            permitted.then_some(info.class_id as i64)
        })
        .collect::<Vec<_>>();
    allowed.sort_unstable();
    allowed.dedup();
    for class_id in allowed {
        emit_branch_if_reg_equals_immediate(ctx, &scope_reg, class_id, &visible);
    }
    emit_runtime_clone_hook_visibility_error(ctx, &visibility, &declaring, invocation_scope)?;
    ctx.emitter.label(&visible);
    Ok(())
}

/// Emits the statically known direct-call visibility error.
fn emit_static_clone_hook_visibility_error(
    ctx: &mut FunctionContext<'_>,
    candidate: &CloneHookCandidate,
) {
    super::super::exceptions::emit_error(
        ctx,
        &format!(
            "Call to {} method {}::__clone() from {}",
            visibility_name(&candidate.visibility),
            candidate.declaring_class,
            invocation_scope_phrase(ctx),
        ),
    );
}

/// Emits the exact denial phrase for the transported runtime scope id.
fn emit_runtime_clone_hook_visibility_error(
    ctx: &mut FunctionContext<'_>,
    visibility: &Visibility,
    declaring: &str,
    invocation_scope: ValueId,
) -> Result<()> {
    let mut scopes = ctx
        .module
        .class_infos
        .iter()
        .map(|(name, info)| (info.class_id as i64, name.clone()))
        .collect::<Vec<_>>();
    scopes.sort_by_key(|(class_id, _)| *class_id);
    let labels = scopes
        .iter()
        .map(|(class_id, _)| (*class_id, ctx.next_label("clone_hook_denied_scope")))
        .collect::<Vec<_>>();
    ctx.load_value_to_result(invocation_scope)?;
    let scope_reg = abi::int_result_reg(ctx.emitter).to_string();
    for (class_id, label) in &labels {
        emit_branch_if_reg_equals_immediate(ctx, &scope_reg, *class_id, label);
    }
    super::super::exceptions::emit_error(
        ctx,
        &format!(
            "Call to {} method {}::__clone() from global scope",
            visibility_name(visibility),
            declaring,
        ),
    );
    for ((_, class_name), (_, label)) in scopes.iter().zip(labels.iter()) {
        ctx.emitter.label(label);
        super::super::exceptions::emit_error(
            ctx,
            &format!(
                "Call to {} method {}::__clone() from scope {}",
                visibility_name(visibility),
                declaring,
                class_name,
            ),
        );
    }
    Ok(())
}

/// Branches when a transported dense class id equals one compile-time id.
pub(super) fn emit_branch_if_reg_equals_immediate(
    ctx: &mut FunctionContext<'_>,
    reg: &str,
    value: i64,
    label: &str,
) {
    let scratch = abi::secondary_scratch_reg(ctx.emitter).to_string();
    abi::emit_load_int_immediate(ctx.emitter, &scratch, value);
    ctx.emitter.instruction(&format!("cmp {reg}, {scratch}"));
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("b.eq {label}")),
        Arch::X86_64 => ctx.emitter.instruction(&format!("je {label}")),
    }
}

/// Transfers the boxed clone to either a boxed or concrete object result slot.
fn store_clone_result(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let Some(result) = inst.result else {
        abi::emit_decref_if_refcounted(ctx.emitter, &PhpType::Mixed);
        return Ok(());
    };
    let result_ty = ctx.value_php_type(result)?.codegen_repr();
    if matches!(result_ty, PhpType::Mixed | PhpType::Union(_)) {
        return ctx.store_result_value(result);
    }
    let PhpType::Object(_) = result_ty else {
        return Err(CodegenIrError::invalid_module(format!(
            "clone() produced incompatible EIR result {result_ty:?}",
        )));
    };
    // A concrete result slot owns the object itself, so retain the payload, then retire the
    // Mixed box that carried it out of the adapter.
    let box_reg = abi::int_result_reg(ctx.emitter).to_string();
    let object_reg = abi::nested_call_reg(ctx.emitter).to_string();
    abi::emit_reserve_temporary_stack(ctx.emitter, 16);
    abi::emit_store_to_sp(ctx.emitter, &box_reg, 0);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    let payload_reg = crate::codegen_support::mixed_unbox_payload_reg(ctx.emitter.target);
    abi::emit_reg_move(ctx.emitter, &object_reg, payload_reg);
    abi::emit_reg_move(ctx.emitter, &box_reg, &object_reg);
    abi::emit_incref_if_refcounted(ctx.emitter, &result_ty);
    abi::emit_load_temporary_stack_slot(ctx.emitter, &box_reg, 0);
    abi::emit_decref_if_refcounted(ctx.emitter, &PhpType::Mixed);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    abi::emit_reg_move(ctx.emitter, &box_reg, &object_reg);
    ctx.store_result_value(result)
}

/// Returns whether the operand is a literal empty array the frontend materialized inline.
pub(super) fn value_is_empty_array_literal(ctx: &FunctionContext<'_>, value: ValueId) -> Result<bool> {
    let mut value = value;
    loop {
        let Some(value_ref) = ctx.function.value(value) else {
            return Ok(false);
        };
        let ValueDef::Instruction { inst, .. } = value_ref.def else {
            return Ok(false);
        };
        let Some(source) = ctx.function.instruction(inst) else {
            return Ok(false);
        };
        if source.op == Op::Acquire {
            let Some(operand) = source.operands.first().copied() else {
                return Ok(false);
            };
            value = operand;
            continue;
        }
        return Ok(source.op == Op::ArrayNew
            && matches!(source.immediate, Some(Immediate::Capacity(0))));
    }
}

/// Branches to `label` when the register holds zero.
fn emit_branch_if_zero(ctx: &mut FunctionContext<'_>, reg: &str, label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("cbz {reg}, {label}")),
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {reg}, {reg}"));
            ctx.emitter.instruction(&format!("jz {label}"));
        }
    }
}

/// Branches to `label` when the register holds a non-zero value.
fn emit_branch_if_nonzero(ctx: &mut FunctionContext<'_>, reg: &str, label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("cbnz {reg}, {label}")),
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {reg}, {reg}"));
            ctx.emitter.instruction(&format!("jnz {label}"));
        }
    }
}

/// Branches to `label` when an unboxed runtime tag register holds the object tag.
fn emit_branch_if_tag_is_object(ctx: &mut FunctionContext<'_>, tag_reg: &str, label: &str) {
    ctx.emitter.instruction(&format!("cmp {tag_reg}, 6"));
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("b.eq {label}")),
        Arch::X86_64 => ctx.emitter.instruction(&format!("je {label}")),
    }
}

/// Returns whether the active lexical class may invoke this runtime class's clone hook.
fn clone_hook_is_visible(ctx: &FunctionContext<'_>, candidate: &CloneHookCandidate) -> bool {
    let declaring = candidate.declaring_class.as_str();
    match candidate.visibility {
        Visibility::Public => true,
        Visibility::Private => ctx.function.lexical_class.as_deref() == Some(declaring),
        Visibility::Protected => ctx.function.lexical_class.as_deref().is_some_and(|current| {
            current == declaring
                || class_is_subclass_of(ctx, current, declaring)
                || class_is_subclass_of(ctx, declaring, current)
        }),
    }
}

/// Walks the emitted parent chain for protected hook access checks.
fn class_is_subclass_of(ctx: &FunctionContext<'_>, class_name: &str, ancestor: &str) -> bool {
    let mut current = ctx
        .module
        .class_infos
        .get(class_name)
        .and_then(|info| info.parent.as_deref());
    while let Some(name) = current {
        if name == ancestor {
            return true;
        }
        current = ctx
            .module
            .class_infos
            .get(name)
            .and_then(|info| info.parent.as_deref());
    }
    false
}

/// Names the invocation scope the way PHP's member-access diagnostics do.
///
/// PHP writes `from global scope` at the top level and `from scope Other` inside a class,
/// so the phrase carries its own trailing word and callers must not append one.
fn invocation_scope_phrase(ctx: &FunctionContext<'_>) -> String {
    ctx.function
        .lexical_class
        .as_deref()
        .map_or_else(|| "global scope".to_string(), |class| format!("scope {class}"))
}

/// Formats PHP visibility in runtime diagnostics.
fn visibility_name(visibility: &Visibility) -> &'static str {
    match visibility {
        Visibility::Public => "public",
        Visibility::Protected => "protected",
        Visibility::Private => "private",
    }
}
