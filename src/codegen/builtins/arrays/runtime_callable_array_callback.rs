//! Purpose:
//! Selects dynamic callable-array descriptors for callback-style runtimes.
//! Builds descriptor callback environments for `[$object, $method]` and
//! `[$class, $method]` values whose slots are only known at runtime.
//!
//! Called from:
//! - Fixed-return array callback builtins such as `array_filter()` and sort helpers.
//! - Caller-managed callback runtimes such as `preg_replace_callback()`.
//!
//! Key details:
//! - The caller must have already pushed the source array pointer before callback
//!   selection, preserving PHP argument evaluation order for second-argument callbacks.
//! - For first-argument callbacks such as `array_map()`, this module can reserve
//!   the saved-array slot before selector evaluation and fill it after the array is evaluated.
//! - Caller-managed runtimes can consume the selected descriptor after this module
//!   has matched the runtime callable-array slots and discarded selector scratch storage.
//! - Each matched descriptor gets a shape-specific wrapper so instance methods can
//!   receive their saved receiver prefix while static methods receive only visible args.

use crate::codegen::abi;
use crate::codegen::callable_dispatch::{
    RuntimeCallableCase, RuntimeInstanceMethodCallableCase, RuntimeStaticMethodCallableCase,
};
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

use super::callback_env;

const MIXED_METHOD_TAG_OFFSET: usize = 0;
const MIXED_METHOD_PAYLOAD_OFFSET: usize = 16;
const MIXED_RECEIVER_TAG_OFFSET: usize = 32;
const MIXED_RECEIVER_PAYLOAD_OFFSET: usize = 48;
const MIXED_SELECTOR_BYTES: usize = 64;
const STRING_METHOD_OFFSET: usize = 0;
const STRING_CLASS_OFFSET: usize = 16;
const STRING_SELECTOR_BYTES: usize = 32;
const SAVED_ARRAY_BYTES: usize = 16;

/// Emits runtime callable-array descriptor selection for a callback runtime with a saved array.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_after_saved_array<F>(
    callback: &Expr,
    array_reg: &str,
    visible_arg_types: Vec<PhpType>,
    descriptor_return_type: PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    mut emit_runtime_call: F,
) -> bool
where
    F: FnMut(
        &callback_env::DescriptorCallbackEnv,
        &mut Emitter,
        &mut Context,
        &mut DataSection,
    ),
{
    let ExprKind::Variable(var_name) = &callback.kind else {
        return false;
    };
    if ctx.callable_array_targets.contains_key(var_name) {
        return false;
    }
    let Some(var_info) = ctx.variables.get(var_name) else {
        return false;
    };

    match var_info.ty.codegen_repr() {
        PhpType::Array(elem_ty) if matches!(elem_ty.codegen_repr(), PhpType::Mixed) => {
            emit_mixed_after_saved_array(
                var_name,
                array_reg,
                visible_arg_types,
                descriptor_return_type,
                emitter,
                ctx,
                data,
                &mut emit_runtime_call,
            );
            true
        }
        PhpType::Array(elem_ty) if matches!(elem_ty.codegen_repr(), PhpType::Str) => {
            emit_string_after_saved_array(
                var_name,
                array_reg,
                visible_arg_types,
                descriptor_return_type,
                emitter,
                ctx,
                data,
                &mut emit_runtime_call,
            );
            true
        }
        _ => false,
    }
}

/// Emits runtime callable-array descriptor selection before evaluating the source array.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_before_array<F>(
    callback: &Expr,
    array: &Expr,
    array_reg: &str,
    visible_arg_types: Vec<PhpType>,
    descriptor_return_type: PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    mut emit_runtime_call: F,
) -> bool
where
    F: FnMut(
        &callback_env::DescriptorCallbackEnv,
        &mut Emitter,
        &mut Context,
        &mut DataSection,
    ),
{
    let ExprKind::Variable(var_name) = &callback.kind else {
        return false;
    };
    if ctx.callable_array_targets.contains_key(var_name) {
        return false;
    }
    let Some(var_info) = ctx.variables.get(var_name) else {
        return false;
    };

    match var_info.ty.codegen_repr() {
        PhpType::Array(elem_ty) if matches!(elem_ty.codegen_repr(), PhpType::Mixed) => {
            emit_mixed_before_array(
                var_name,
                array,
                array_reg,
                visible_arg_types,
                descriptor_return_type,
                emitter,
                ctx,
                data,
                &mut emit_runtime_call,
            );
            true
        }
        PhpType::Array(elem_ty) if matches!(elem_ty.codegen_repr(), PhpType::Str) => {
            emit_string_before_array(
                var_name,
                array,
                array_reg,
                visible_arg_types,
                descriptor_return_type,
                emitter,
                ctx,
                data,
                &mut emit_runtime_call,
            );
            true
        }
        _ => false,
    }
}

/// Emits runtime callable-array descriptor selection for caller-managed callback payloads.
///
/// This mode matches a runtime `[$object, $method]` or `[$class, $method]`
/// variable, releases the selector scratch slots inside the matched case, and
/// then lets the caller build the descriptor environment and runtime call. For
/// instance-method cases, the selected receiver is pushed on top of the temporary
/// stack before `emit_selected_case` runs and must be consumed by the caller.
pub(crate) fn emit_without_saved_array<F>(
    callback: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    mut emit_selected_case: F,
) -> bool
where
    F: FnMut(&RuntimeCallableCase, Option<&PhpType>, &mut Emitter, &mut Context, &mut DataSection),
{
    let ExprKind::Variable(var_name) = &callback.kind else {
        return false;
    };
    if ctx.callable_array_targets.contains_key(var_name) {
        return false;
    }
    let Some(var_info) = ctx.variables.get(var_name) else {
        return false;
    };

    match var_info.ty.codegen_repr() {
        PhpType::Array(elem_ty) if matches!(elem_ty.codegen_repr(), PhpType::Mixed) => {
            emit_mixed_without_saved_array(var_name, emitter, ctx, data, &mut emit_selected_case);
            true
        }
        PhpType::Array(elem_ty) if matches!(elem_ty.codegen_repr(), PhpType::Str) => {
            emit_string_without_saved_array(var_name, emitter, ctx, data, &mut emit_selected_case);
            true
        }
        _ => false,
    }
}

/// Emits runtime callable-array descriptor selection for a two-slot literal callback.
///
/// The selected-case callback may consume an instance receiver from the temporary
/// stack, but it must leave the preserved literal array as the top stack slot so
/// this helper can release the temporary array before returning to the caller.
pub(crate) fn emit_literal_without_saved_array<F>(
    callback: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    mut emit_selected_case: F,
) -> bool
where
    F: FnMut(&RuntimeCallableCase, Option<&PhpType>, &mut Emitter, &mut Context, &mut DataSection),
{
    if !is_two_slot_callable_array_literal(callback) {
        return false;
    }
    let callback_ty = crate::codegen::functions::infer_contextual_type(callback, ctx).codegen_repr();
    match callback_ty {
        PhpType::Array(elem_ty) if matches!(elem_ty.codegen_repr(), PhpType::Mixed) => {
            emit_mixed_literal_without_saved_array(
                callback,
                emitter,
                ctx,
                data,
                &mut emit_selected_case,
            );
            true
        }
        PhpType::Array(elem_ty) if matches!(elem_ty.codegen_repr(), PhpType::Str) => {
            emit_string_literal_without_saved_array(
                callback,
                emitter,
                ctx,
                data,
                &mut emit_selected_case,
            );
            true
        }
        _ => false,
    }
}

/// Emits descriptor selection for heterogeneous callable arrays above a saved source array.
#[allow(clippy::too_many_arguments)]
fn emit_mixed_after_saved_array<F>(
    var_name: &str,
    array_reg: &str,
    visible_arg_types: Vec<PhpType>,
    descriptor_return_type: PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    emit_runtime_call: &mut F,
)
where
    F: FnMut(
        &callback_env::DescriptorCallbackEnv,
        &mut Emitter,
        &mut Context,
        &mut DataSection,
    ),
{
    let instance_cases =
        crate::codegen::callable_dispatch::runtime_public_instance_method_cases(ctx, data);
    let static_cases =
        crate::codegen::callable_dispatch::runtime_public_static_method_cases(ctx, data);
    emit_mixed_selector_slots(var_name, emitter, ctx, data);
    emit_mixed_dispatch(
        &instance_cases,
        &static_cases,
        array_reg,
        &visible_arg_types,
        &descriptor_return_type,
        emitter,
        ctx,
        data,
        emit_runtime_call,
    );
    abi::emit_release_temporary_stack(emitter, MIXED_SELECTOR_BYTES + SAVED_ARRAY_BYTES);
}

/// Emits descriptor selection for heterogeneous callable arrays before the source array.
#[allow(clippy::too_many_arguments)]
fn emit_mixed_before_array<F>(
    var_name: &str,
    array: &Expr,
    array_reg: &str,
    visible_arg_types: Vec<PhpType>,
    descriptor_return_type: PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    emit_runtime_call: &mut F,
)
where
    F: FnMut(
        &callback_env::DescriptorCallbackEnv,
        &mut Emitter,
        &mut Context,
        &mut DataSection,
    ),
{
    let instance_cases =
        crate::codegen::callable_dispatch::runtime_public_instance_method_cases(ctx, data);
    let static_cases =
        crate::codegen::callable_dispatch::runtime_public_static_method_cases(ctx, data);
    abi::emit_reserve_temporary_stack(emitter, SAVED_ARRAY_BYTES);
    emit_mixed_selector_slots(var_name, emitter, ctx, data);
    crate::codegen::expr::emit_expr(array, emitter, ctx, data);
    emitter.instruction(&format!("mov {}, {}", array_reg, abi::int_result_reg(emitter))); // preserve the mapped array pointer before descriptor-case selection
    emit_store_saved_array_reg(array_reg, MIXED_SELECTOR_BYTES, emitter);
    emit_mixed_dispatch(
        &instance_cases,
        &static_cases,
        array_reg,
        &visible_arg_types,
        &descriptor_return_type,
        emitter,
        ctx,
        data,
        emit_runtime_call,
    );
    abi::emit_release_temporary_stack(emitter, MIXED_SELECTOR_BYTES + SAVED_ARRAY_BYTES);
}

/// Emits descriptor selection for string callable arrays above a saved source array.
#[allow(clippy::too_many_arguments)]
fn emit_string_after_saved_array<F>(
    var_name: &str,
    array_reg: &str,
    visible_arg_types: Vec<PhpType>,
    descriptor_return_type: PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    emit_runtime_call: &mut F,
)
where
    F: FnMut(
        &callback_env::DescriptorCallbackEnv,
        &mut Emitter,
        &mut Context,
        &mut DataSection,
    ),
{
    let static_cases =
        crate::codegen::callable_dispatch::runtime_public_static_method_cases(ctx, data);
    emit_string_selector_slots(var_name, emitter, ctx, data);
    emit_string_dispatch(
        &static_cases,
        array_reg,
        &visible_arg_types,
        &descriptor_return_type,
        emitter,
        ctx,
        data,
        emit_runtime_call,
    );
    abi::emit_release_temporary_stack(emitter, STRING_SELECTOR_BYTES + SAVED_ARRAY_BYTES);
}

/// Emits descriptor selection for string callable arrays before the source array.
#[allow(clippy::too_many_arguments)]
fn emit_string_before_array<F>(
    var_name: &str,
    array: &Expr,
    array_reg: &str,
    visible_arg_types: Vec<PhpType>,
    descriptor_return_type: PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    emit_runtime_call: &mut F,
)
where
    F: FnMut(
        &callback_env::DescriptorCallbackEnv,
        &mut Emitter,
        &mut Context,
        &mut DataSection,
    ),
{
    let static_cases =
        crate::codegen::callable_dispatch::runtime_public_static_method_cases(ctx, data);
    abi::emit_reserve_temporary_stack(emitter, SAVED_ARRAY_BYTES);
    emit_string_selector_slots(var_name, emitter, ctx, data);
    crate::codegen::expr::emit_expr(array, emitter, ctx, data);
    emitter.instruction(&format!("mov {}, {}", array_reg, abi::int_result_reg(emitter))); // preserve the mapped array pointer before descriptor-case selection
    emit_store_saved_array_reg(array_reg, STRING_SELECTOR_BYTES, emitter);
    emit_string_dispatch(
        &static_cases,
        array_reg,
        &visible_arg_types,
        &descriptor_return_type,
        emitter,
        ctx,
        data,
        emit_runtime_call,
    );
    abi::emit_release_temporary_stack(emitter, STRING_SELECTOR_BYTES + SAVED_ARRAY_BYTES);
}

/// Emits heterogeneous callable-array descriptor selection without a saved source array.
fn emit_mixed_without_saved_array<F>(
    var_name: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    emit_selected_case: &mut F,
)
where
    F: FnMut(&RuntimeCallableCase, Option<&PhpType>, &mut Emitter, &mut Context, &mut DataSection),
{
    let instance_cases =
        crate::codegen::callable_dispatch::runtime_public_instance_method_cases(ctx, data);
    let static_cases =
        crate::codegen::callable_dispatch::runtime_public_static_method_cases(ctx, data);
    emit_mixed_selector_slots(var_name, emitter, ctx, data);
    emit_mixed_dispatch_without_saved_array(
        &instance_cases,
        &static_cases,
        emitter,
        ctx,
        data,
        emit_selected_case,
    );
}

/// Emits static-method string callable-array descriptor selection without a saved source array.
fn emit_string_without_saved_array<F>(
    var_name: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    emit_selected_case: &mut F,
)
where
    F: FnMut(&RuntimeCallableCase, Option<&PhpType>, &mut Emitter, &mut Context, &mut DataSection),
{
    let static_cases =
        crate::codegen::callable_dispatch::runtime_public_static_method_cases(ctx, data);
    emit_string_selector_slots(var_name, emitter, ctx, data);
    emit_string_dispatch_without_saved_array(&static_cases, emitter, ctx, data, emit_selected_case);
}

/// Emits heterogeneous callable-array descriptor selection for a runtime literal.
fn emit_mixed_literal_without_saved_array<F>(
    callback: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    emit_selected_case: &mut F,
)
where
    F: FnMut(&RuntimeCallableCase, Option<&PhpType>, &mut Emitter, &mut Context, &mut DataSection),
{
    let instance_cases =
        crate::codegen::callable_dispatch::runtime_public_instance_method_cases(ctx, data);
    let static_cases =
        crate::codegen::callable_dispatch::runtime_public_static_method_cases(ctx, data);
    crate::codegen::expr::emit_expr(callback, emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the runtime callable-array literal while selecting its descriptor
    emit_mixed_literal_selector_slots(emitter);
    emit_mixed_dispatch_without_saved_array(
        &instance_cases,
        &static_cases,
        emitter,
        ctx,
        data,
        emit_selected_case,
    );
    release_preserved_literal_array_after_selection(
        &PhpType::Array(Box::new(PhpType::Mixed)),
        emitter,
    );
}

/// Emits static-method string callable-array descriptor selection for a runtime literal.
fn emit_string_literal_without_saved_array<F>(
    callback: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    emit_selected_case: &mut F,
)
where
    F: FnMut(&RuntimeCallableCase, Option<&PhpType>, &mut Emitter, &mut Context, &mut DataSection),
{
    let static_cases =
        crate::codegen::callable_dispatch::runtime_public_static_method_cases(ctx, data);
    crate::codegen::expr::emit_expr(callback, emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the runtime static callable-array literal while selecting its descriptor
    emit_string_literal_selector_slots(emitter);
    emit_string_dispatch_without_saved_array(&static_cases, emitter, ctx, data, emit_selected_case);
    release_preserved_literal_array_after_selection(&PhpType::Array(Box::new(PhpType::Str)), emitter);
}

/// Saves the unboxed receiver and method slots for a runtime heterogeneous callable array.
fn emit_mixed_selector_slots(
    var_name: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.comment("runtime callback callable-array mixed selector");
    let receiver = callable_array_slot_expr(var_name, 0);
    crate::codegen::expr::emit_expr(&receiver, emitter, ctx, data);
    emit_unbox_mixed_result(emitter);
    emit_push_mixed_unbox_payload(emitter);

    let method = callable_array_slot_expr(var_name, 1);
    crate::codegen::expr::emit_expr(&method, emitter, ctx, data);
    emit_unbox_mixed_result(emitter);
    emit_push_mixed_unbox_payload(emitter);
}

/// Saves class and method string slots for a runtime static-method callable array.
fn emit_string_selector_slots(
    var_name: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.comment("runtime callback callable-array string selector");
    let class = callable_array_slot_expr(var_name, 0);
    crate::codegen::expr::emit_expr(&class, emitter, ctx, data);
    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
    abi::emit_push_reg_pair(emitter, ptr_reg, len_reg);                         // preserve the runtime class string while the method slot is read

    let method = callable_array_slot_expr(var_name, 1);
    crate::codegen::expr::emit_expr(&method, emitter, ctx, data);
    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
    abi::emit_push_reg_pair(emitter, ptr_reg, len_reg);                         // preserve the runtime method string for descriptor-case selection
}

/// Saves selector slots read from an already-evaluated mixed callable-array literal.
fn emit_mixed_literal_selector_slots(emitter: &mut Emitter) {
    emitter.comment("runtime callback callable-array literal mixed selector");
    emit_unbox_mixed_literal_slot(0, 0, emitter);
    emit_push_mixed_unbox_payload(emitter);
    emit_unbox_mixed_literal_slot(32, 1, emitter);
    emit_push_mixed_unbox_payload(emitter);
}

/// Saves selector slots read from an already-evaluated string callable-array literal.
fn emit_string_literal_selector_slots(emitter: &mut Emitter) {
    emitter.comment("runtime callback callable-array literal string selector");
    emit_push_string_literal_slot(0, 0, emitter);
    emit_push_string_literal_slot(16, 1, emitter);
}

/// Loads and unboxes one boxed Mixed slot from a preserved callable-array literal.
fn emit_unbox_mixed_literal_slot(array_stack_offset: usize, slot: usize, emitter: &mut Emitter) {
    let array_reg = abi::symbol_scratch_reg(emitter);
    abi::emit_load_temporary_stack_slot(emitter, array_reg, array_stack_offset);
    abi::emit_load_from_address(
        emitter,
        abi::int_result_reg(emitter),
        array_reg,
        24 + slot * 8,
    );
    emit_unbox_mixed_result(emitter);
}

/// Loads and saves one string slot from a preserved callable-array literal.
fn emit_push_string_literal_slot(array_stack_offset: usize, slot: usize, emitter: &mut Emitter) {
    let array_reg = abi::symbol_scratch_reg(emitter);
    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
    abi::emit_load_temporary_stack_slot(emitter, array_reg, array_stack_offset);
    abi::emit_load_from_address(emitter, ptr_reg, array_reg, 24 + slot * 16);
    abi::emit_load_from_address(emitter, len_reg, array_reg, 24 + slot * 16 + 8);
    abi::emit_push_reg_pair(emitter, ptr_reg, len_reg);                         // preserve the runtime string slot for descriptor-case selection
}

/// Unboxes the current Mixed result into target-specific tag and payload registers.
fn emit_unbox_mixed_result(emitter: &mut Emitter) {
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
}

/// Pushes the unboxed Mixed tag and payload onto the temporary stack.
fn emit_push_mixed_unbox_payload(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg_pair(emitter, "x1", "x2");                      // preserve the unboxed callable-array payload words for callback selection
            abi::emit_push_reg(emitter, "x0");                                  // preserve the unboxed callable-array tag beside its payload
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rdi", "rdx");                    // preserve the unboxed callable-array payload words for callback selection
            abi::emit_push_reg(emitter, "rax");                                 // preserve the unboxed callable-array tag beside its payload
        }
    }
}

/// Stores the mapped source array into the reserved saved-array stack slot.
fn emit_store_saved_array_reg(array_reg: &str, offset: usize, emitter: &mut Emitter) {
    let scratch = abi::symbol_scratch_reg(emitter);
    abi::emit_temporary_stack_address(emitter, scratch, offset);
    abi::emit_store_to_address(emitter, array_reg, scratch, 0);
}

/// Releases a preserved callable-array literal after caller-managed descriptor selection.
fn release_preserved_literal_array_after_selection(arr_ty: &PhpType, emitter: &mut Emitter) {
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the selected-case result while releasing the temporary callable-array literal
    abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), 16);
    abi::emit_decref_if_refcounted(emitter, arr_ty);
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // restore the selected-case result after literal cleanup
    abi::emit_release_temporary_stack(emitter, 16);                             // discard the preserved callable-array literal slot
}

/// Dispatches a heterogeneous callable array to a descriptor-backed callback runtime call.
#[allow(clippy::too_many_arguments)]
fn emit_mixed_dispatch<F>(
    instance_cases: &[RuntimeInstanceMethodCallableCase],
    static_cases: &[RuntimeStaticMethodCallableCase],
    array_reg: &str,
    visible_arg_types: &[PhpType],
    descriptor_return_type: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    emit_runtime_call: &mut F,
)
where
    F: FnMut(
        &callback_env::DescriptorCallbackEnv,
        &mut Emitter,
        &mut Context,
        &mut DataSection,
    ),
{
    let done_label = ctx.next_label("runtime_callback_array_done");
    for case in instance_cases {
        let next_case = ctx.next_label("runtime_callback_array_instance_next");
        emit_branch_if_mixed_instance_case_mismatch(case, &next_case, emitter, ctx, data);
        emit_instance_case_callback(
            &case.case,
            array_reg,
            visible_arg_types,
            descriptor_return_type,
            emitter,
            ctx,
            emit_runtime_call,
            data,
        );
        abi::emit_jump(emitter, &done_label);
        emitter.label(&next_case);
    }
    for case in static_cases {
        let next_case = ctx.next_label("runtime_callback_array_static_next");
        emit_branch_if_mixed_static_case_mismatch(case, &next_case, emitter, ctx, data);
        emit_static_case_callback(
            &case.case,
            array_reg,
            MIXED_SELECTOR_BYTES,
            visible_arg_types,
            descriptor_return_type,
            emitter,
            ctx,
            emit_runtime_call,
            data,
        );
        abi::emit_jump(emitter, &done_label);
        emitter.label(&next_case);
    }
    emit_no_match_abort(emitter, data);
    emitter.label(&done_label);
}

/// Dispatches a string callable array to a descriptor-backed callback runtime call.
#[allow(clippy::too_many_arguments)]
fn emit_string_dispatch<F>(
    static_cases: &[RuntimeStaticMethodCallableCase],
    array_reg: &str,
    visible_arg_types: &[PhpType],
    descriptor_return_type: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    emit_runtime_call: &mut F,
)
where
    F: FnMut(
        &callback_env::DescriptorCallbackEnv,
        &mut Emitter,
        &mut Context,
        &mut DataSection,
    ),
{
    let done_label = ctx.next_label("runtime_callback_array_done");
    for case in static_cases {
        let next_case = ctx.next_label("runtime_callback_array_static_next");
        emit_branch_if_string_static_case_mismatch(case, &next_case, emitter, ctx, data);
        emit_static_case_callback(
            &case.case,
            array_reg,
            STRING_SELECTOR_BYTES,
            visible_arg_types,
            descriptor_return_type,
            emitter,
            ctx,
            emit_runtime_call,
            data,
        );
        abi::emit_jump(emitter, &done_label);
        emitter.label(&next_case);
    }
    emit_no_match_abort(emitter, data);
    emitter.label(&done_label);
}

/// Dispatches a heterogeneous callable array for caller-managed callback runtime emission.
fn emit_mixed_dispatch_without_saved_array<F>(
    instance_cases: &[RuntimeInstanceMethodCallableCase],
    static_cases: &[RuntimeStaticMethodCallableCase],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    emit_selected_case: &mut F,
)
where
    F: FnMut(&RuntimeCallableCase, Option<&PhpType>, &mut Emitter, &mut Context, &mut DataSection),
{
    let done_label = ctx.next_label("runtime_callback_array_done");
    for case in instance_cases {
        let next_case = ctx.next_label("runtime_callback_array_instance_next");
        emit_branch_if_mixed_instance_case_mismatch(case, &next_case, emitter, ctx, data);
        emit_instance_case_without_saved_array(
            &case.case,
            emitter,
            ctx,
            data,
            emit_selected_case,
        );
        abi::emit_jump(emitter, &done_label);
        emitter.label(&next_case);
    }
    for case in static_cases {
        let next_case = ctx.next_label("runtime_callback_array_static_next");
        emit_branch_if_mixed_static_case_mismatch(case, &next_case, emitter, ctx, data);
        emit_static_case_without_saved_array(
            &case.case,
            MIXED_SELECTOR_BYTES,
            emitter,
            ctx,
            data,
            emit_selected_case,
        );
        abi::emit_jump(emitter, &done_label);
        emitter.label(&next_case);
    }
    emit_no_match_abort(emitter, data);
    emitter.label(&done_label);
}

/// Dispatches a string callable array for caller-managed callback runtime emission.
fn emit_string_dispatch_without_saved_array<F>(
    static_cases: &[RuntimeStaticMethodCallableCase],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    emit_selected_case: &mut F,
)
where
    F: FnMut(&RuntimeCallableCase, Option<&PhpType>, &mut Emitter, &mut Context, &mut DataSection),
{
    let done_label = ctx.next_label("runtime_callback_array_done");
    for case in static_cases {
        let next_case = ctx.next_label("runtime_callback_array_static_next");
        emit_branch_if_string_static_case_mismatch(case, &next_case, emitter, ctx, data);
        emit_static_case_without_saved_array(
            &case.case,
            STRING_SELECTOR_BYTES,
            emitter,
            ctx,
            data,
            emit_selected_case,
        );
        abi::emit_jump(emitter, &done_label);
        emitter.label(&next_case);
    }
    emit_no_match_abort(emitter, data);
    emitter.label(&done_label);
}

/// Emits the runtime call for one selected instance-method descriptor case.
#[allow(clippy::too_many_arguments)]
fn emit_instance_case_callback<F>(
    case: &RuntimeCallableCase,
    array_reg: &str,
    visible_arg_types: &[PhpType],
    descriptor_return_type: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
    emit_runtime_call: &mut F,
    data: &mut DataSection,
)
where
    F: FnMut(
        &callback_env::DescriptorCallbackEnv,
        &mut Emitter,
        &mut Context,
        &mut DataSection,
    ),
{
    let receiver_ty = case
        .sig
        .params
        .first()
        .map(|(_, ty)| ty.clone())
        .unwrap_or(PhpType::Mixed);
    let call_reg = abi::nested_call_reg(emitter);
    abi::emit_load_temporary_stack_slot(emitter, call_reg, MIXED_RECEIVER_PAYLOAD_OFFSET);
    abi::emit_load_temporary_stack_slot(emitter, array_reg, MIXED_SELECTOR_BYTES);
    let wrapper = callback_env::emit_descriptor_callback_env_from_static_descriptor(
        &case.descriptor_label,
        visible_arg_types.to_vec(),
        vec![receiver_ty.clone()],
        descriptor_return_type.clone(),
        emitter,
        ctx,
    );
    emitter.instruction(&format!("mov {}, {}", abi::int_result_reg(emitter), call_reg)); // restore the runtime callable-array receiver for descriptor prefix storage
    callback_env::store_descriptor_callback_prefix_result(&wrapper, 0, &receiver_ty, emitter);
    callback_env::store_descriptor_callback_array_reg(&wrapper, array_reg, emitter);
    emit_runtime_call(&wrapper, emitter, ctx, data);
    callback_env::release_descriptor_callback_env(&wrapper, emitter);
}

/// Emits the runtime call for one selected static-method descriptor case.
#[allow(clippy::too_many_arguments)]
fn emit_static_case_callback<F>(
    case: &RuntimeCallableCase,
    array_reg: &str,
    saved_array_offset: usize,
    visible_arg_types: &[PhpType],
    descriptor_return_type: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
    emit_runtime_call: &mut F,
    data: &mut DataSection,
)
where
    F: FnMut(
        &callback_env::DescriptorCallbackEnv,
        &mut Emitter,
        &mut Context,
        &mut DataSection,
    ),
{
    abi::emit_load_temporary_stack_slot(emitter, array_reg, saved_array_offset);
    let wrapper = callback_env::emit_descriptor_callback_env_from_static_descriptor(
        &case.descriptor_label,
        visible_arg_types.to_vec(),
        Vec::new(),
        descriptor_return_type.clone(),
        emitter,
        ctx,
    );
    callback_env::store_descriptor_callback_array_reg(&wrapper, array_reg, emitter);
    emit_runtime_call(&wrapper, emitter, ctx, data);
    callback_env::release_descriptor_callback_env(&wrapper, emitter);
}

/// Emits one selected instance-method case for a caller-managed callback runtime.
///
/// The receiver is copied out of the selector scratch slots before they are
/// released, then pushed back onto the temporary stack so the caller can prepend
/// it to the descriptor environment after evaluating any remaining PHP arguments.
fn emit_instance_case_without_saved_array<F>(
    case: &RuntimeCallableCase,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    emit_selected_case: &mut F,
)
where
    F: FnMut(&RuntimeCallableCase, Option<&PhpType>, &mut Emitter, &mut Context, &mut DataSection),
{
    let receiver_ty = case
        .sig
        .params
        .first()
        .map(|(_, ty)| ty.clone())
        .unwrap_or(PhpType::Mixed);
    let call_reg = abi::nested_call_reg(emitter);
    abi::emit_load_temporary_stack_slot(emitter, call_reg, MIXED_RECEIVER_PAYLOAD_OFFSET);
    abi::emit_release_temporary_stack(emitter, MIXED_SELECTOR_BYTES);
    abi::emit_push_reg(emitter, call_reg);                                      // preserve the selected runtime callable-array receiver for caller-managed emission
    emit_selected_case(case, Some(&receiver_ty), emitter, ctx, data);
}

/// Emits one selected static-method case for a caller-managed callback runtime.
fn emit_static_case_without_saved_array<F>(
    case: &RuntimeCallableCase,
    selector_bytes: usize,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    emit_selected_case: &mut F,
)
where
    F: FnMut(&RuntimeCallableCase, Option<&PhpType>, &mut Emitter, &mut Context, &mut DataSection),
{
    abi::emit_release_temporary_stack(emitter, selector_bytes);
    emit_selected_case(case, None, emitter, ctx, data);
}

/// Branches when the saved heterogeneous callable-array slots do not match an instance-method case.
fn emit_branch_if_mixed_instance_case_mismatch(
    case: &RuntimeInstanceMethodCallableCase,
    next_case: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emit_branch_if_stack_tag_mismatch(MIXED_RECEIVER_TAG_OFFSET, 6, next_case, emitter);
    emit_branch_if_stack_tag_mismatch(MIXED_METHOD_TAG_OFFSET, 1, next_case, emitter);
    emit_branch_if_receiver_class_id_mismatch(
        case.class_id,
        MIXED_RECEIVER_PAYLOAD_OFFSET,
        next_case,
        emitter,
    );
    emit_branch_if_stack_string_mismatch(
        MIXED_METHOD_PAYLOAD_OFFSET,
        MIXED_METHOD_PAYLOAD_OFFSET + 8,
        case.method_name.as_bytes(),
        next_case,
        emitter,
        ctx,
        data,
    );
}

/// Branches when the saved heterogeneous callable-array slots do not match a static-method case.
fn emit_branch_if_mixed_static_case_mismatch(
    case: &RuntimeStaticMethodCallableCase,
    next_case: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emit_branch_if_stack_tag_mismatch(MIXED_RECEIVER_TAG_OFFSET, 1, next_case, emitter);
    emit_branch_if_stack_tag_mismatch(MIXED_METHOD_TAG_OFFSET, 1, next_case, emitter);
    emit_branch_if_static_class_string_mismatch(
        MIXED_RECEIVER_PAYLOAD_OFFSET,
        MIXED_RECEIVER_PAYLOAD_OFFSET + 8,
        &case.class_name,
        next_case,
        emitter,
        ctx,
        data,
    );
    emit_branch_if_stack_string_mismatch(
        MIXED_METHOD_PAYLOAD_OFFSET,
        MIXED_METHOD_PAYLOAD_OFFSET + 8,
        case.method_name.as_bytes(),
        next_case,
        emitter,
        ctx,
        data,
    );
}

/// Branches when the saved string callable-array slots do not match a static-method case.
fn emit_branch_if_string_static_case_mismatch(
    case: &RuntimeStaticMethodCallableCase,
    next_case: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emit_branch_if_static_class_string_mismatch(
        STRING_CLASS_OFFSET,
        STRING_CLASS_OFFSET + 8,
        &case.class_name,
        next_case,
        emitter,
        ctx,
        data,
    );
    emit_branch_if_stack_string_mismatch(
        STRING_METHOD_OFFSET,
        STRING_METHOD_OFFSET + 8,
        case.method_name.as_bytes(),
        next_case,
        emitter,
        ctx,
        data,
    );
}

/// Branches when a saved Mixed tag stack slot does not equal `expected_tag`.
fn emit_branch_if_stack_tag_mismatch(
    tag_offset: usize,
    expected_tag: i64,
    next_case: &str,
    emitter: &mut Emitter,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(emitter, "x9", tag_offset);
            emitter.instruction(&format!("cmp x9, #{}", expected_tag));         // compare the callable-array callback tag against this descriptor shape
            emitter.instruction(&format!("b.ne {}", next_case));                // try the next callback descriptor case when the tag differs
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(emitter, "r10", tag_offset);
            emitter.instruction(&format!("cmp r10, {}", expected_tag));         // compare the callable-array callback tag against this descriptor shape
            emitter.instruction(&format!("jne {}", next_case));                 // try the next callback descriptor case when the tag differs
        }
    }
}

/// Branches when the saved receiver object's class id does not match `class_id`.
fn emit_branch_if_receiver_class_id_mismatch(
    class_id: u64,
    receiver_offset: usize,
    next_case: &str,
    emitter: &mut Emitter,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(emitter, "x9", receiver_offset);
            emitter.instruction(&format!("cbz x9, {}", next_case));             // reject null callback receivers before reading their class id
            emitter.instruction("ldr x10, [x9]");                               // load the callback receiver runtime class id
            abi::emit_load_int_immediate(emitter, "x11", class_id as i64);
            emitter.instruction("cmp x10, x11");                                // compare callback receiver class id against this descriptor case
            emitter.instruction(&format!("b.ne {}", next_case));                // try the next descriptor case when the receiver class differs
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(emitter, "r10", receiver_offset);
            emitter.instruction("test r10, r10");                               // reject null callback receivers before reading their class id
            emitter.instruction(&format!("je {}", next_case));                  // try the next descriptor case when the receiver pointer is null
            emitter.instruction("mov r11, QWORD PTR [r10]");                    // load the callback receiver runtime class id
            abi::emit_load_int_immediate(emitter, "r10", class_id as i64);
            emitter.instruction("cmp r11, r10");                                // compare callback receiver class id against this descriptor case
            emitter.instruction(&format!("jne {}", next_case));                 // try the next descriptor case when the receiver class differs
        }
    }
}

/// Branches when a saved class string does not match either bare or leading-slash form.
#[allow(clippy::too_many_arguments)]
fn emit_branch_if_static_class_string_mismatch(
    ptr_offset: usize,
    len_offset: usize,
    class_name: &str,
    next_case: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let matched_label = ctx.next_label("runtime_callback_array_class_match");
    emit_stack_string_compare_branch(
        ptr_offset,
        len_offset,
        class_name.as_bytes(),
        &matched_label,
        emitter,
        data,
    );
    let leading_slash = format!("\\{}", class_name);
    emit_stack_string_compare_branch(
        ptr_offset,
        len_offset,
        leading_slash.as_bytes(),
        &matched_label,
        emitter,
        data,
    );
    abi::emit_jump(emitter, next_case);
    emitter.label(&matched_label);
}

/// Branches when a saved stack string does not match the expected PHP name case-insensitively.
#[allow(clippy::too_many_arguments)]
fn emit_branch_if_stack_string_mismatch(
    ptr_offset: usize,
    len_offset: usize,
    expected: &[u8],
    next_case: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let matched_label = ctx.next_label("runtime_callback_array_string_match");
    emit_stack_string_compare_branch(
        ptr_offset,
        len_offset,
        expected,
        &matched_label,
        emitter,
        data,
    );
    abi::emit_jump(emitter, next_case);
    emitter.label(&matched_label);
}

/// Compares a saved stack string with `expected` and branches to `matched_label` on equality.
fn emit_stack_string_compare_branch(
    ptr_offset: usize,
    len_offset: usize,
    expected: &[u8],
    matched_label: &str,
    emitter: &mut Emitter,
    data: &mut DataSection,
) {
    let (expected_label, expected_len) = data.add_string(expected);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(emitter, "x1", ptr_offset);
            abi::emit_load_temporary_stack_slot(emitter, "x2", len_offset);
            abi::emit_symbol_address(emitter, "x3", &expected_label);
            abi::emit_load_int_immediate(emitter, "x4", expected_len as i64);
            abi::emit_call_label(emitter, "__rt_strcasecmp");
            emitter.instruction("cmp x0, #0");                                  // did the callable-array callback string match this descriptor name?
            emitter.instruction(&format!("b.eq {}", matched_label));            // select this callback descriptor case when names match
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(emitter, "rdi", ptr_offset);
            abi::emit_load_temporary_stack_slot(emitter, "rsi", len_offset);
            abi::emit_symbol_address(emitter, "rdx", &expected_label);
            abi::emit_load_int_immediate(emitter, "rcx", expected_len as i64);
            abi::emit_call_label(emitter, "__rt_strcasecmp");
            emitter.instruction("test rax, rax");                               // did the callable-array callback string match this descriptor name?
            emitter.instruction(&format!("je {}", matched_label));              // select this callback descriptor case when names match
        }
    }
}

/// Emits the fatal diagnostic for callable arrays that cannot be resolved to a descriptor.
fn emit_no_match_abort(emitter: &mut Emitter, data: &mut DataSection) {
    let (message_label, message_len) = data.add_string(
        b"Fatal error: callable array did not resolve to an invokable target\n",
    );
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #2");                                  // write the callable-array callback diagnostic to stderr
            abi::emit_symbol_address(emitter, "x1", &message_label);
            emitter.instruction(&format!("mov x2, #{}", message_len));          // pass the callable-array callback diagnostic length to write()
            emitter.syscall(4);
            abi::emit_exit(emitter, 1);
        }
        Arch::X86_64 => {
            emitter.instruction("mov edi, 2");                                  // write the callable-array callback diagnostic to stderr
            abi::emit_symbol_address(emitter, "rsi", &message_label);
            emitter.instruction(&format!("mov edx, {}", message_len));          // pass the callable-array callback diagnostic length to write()
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall 1 = write
            emitter.instruction("syscall");                                     // emit the fatal callable-array callback diagnostic
            abi::emit_exit(emitter, 1);
        }
    }
}

/// Builds `$callback[$index]`, a positional slot stored inside a callable-array value.
fn callable_array_slot_expr(var_name: &str, index: i64) -> Expr {
    Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(Expr::new(
                ExprKind::Variable(var_name.to_string()),
                crate::span::Span::dummy(),
            )),
            index: Box::new(Expr::new(
                ExprKind::IntLiteral(index),
                crate::span::Span::dummy(),
            )),
        },
        crate::span::Span::dummy(),
    )
}

/// Returns true for a PHP callable-array literal with receiver/class and method slots.
fn is_two_slot_callable_array_literal(callback: &Expr) -> bool {
    matches!(&callback.kind, ExprKind::ArrayLiteral(elems) if elems.len() == 2)
}
