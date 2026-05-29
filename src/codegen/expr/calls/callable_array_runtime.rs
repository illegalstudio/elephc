//! Purpose:
//! Selects callable-array descriptors at runtime when receiver or method slots are not static literals.
//! Keeps dynamic `[$object, $method]` and `[$class, $method]` direct-call logic out of the call dispatcher.
//!
//! Called from:
//! - `crate::codegen::expr::calls::emit_runtime_callable_array_call()`
//! - `crate::codegen::expr::calls::emit_callable_array_literal_call()`
//! - `crate::codegen::expr::calls::emit_callable_array_variable_call()`
//!
//! Key details:
//! - Selector slots are read before user arguments so runtime method resolution observes the callable value first.
//! - Matched cases invoke the same descriptor invoker path as static callable-array calls.

use crate::codegen::builtins::arrays::receiver_call_args;
use crate::codegen::callable_dispatch::{
    RuntimeCallableCase, RuntimeInstanceMethodCallableCase, RuntimeStaticMethodCallableCase,
};
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::parser::ast::{Expr, ExprKind};
use crate::span::Span;
use crate::types::PhpType;

const MIXED_METHOD_TAG_OFFSET: usize = 0;
const MIXED_METHOD_PAYLOAD_OFFSET: usize = 16;
const MIXED_RECEIVER_TAG_OFFSET: usize = 32;
const MIXED_RECEIVER_PAYLOAD_OFFSET: usize = 48;
const MIXED_SELECTOR_BYTES: usize = 64;
const STRING_METHOD_OFFSET: usize = 0;
const STRING_CLASS_OFFSET: usize = 16;
const STRING_SELECTOR_BYTES: usize = 32;

/// Emits a descriptor invocation for callable-array variables whose receiver or method is runtime-selected.
pub(super) fn emit_variable_call(
    var: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    let var_ty = ctx.variables.get(var)?.ty.codegen_repr();
    match var_ty {
        PhpType::Array(elem_ty) if matches!(elem_ty.codegen_repr(), PhpType::Mixed) => {
            emit_mixed_variable_call(var, args, emitter, ctx, data);
            Some(PhpType::Mixed)
        }
        PhpType::Array(elem_ty) if matches!(elem_ty.codegen_repr(), PhpType::Str) => {
            emit_string_variable_call(var, args, emitter, ctx, data);
            Some(PhpType::Mixed)
        }
        _ => None,
    }
}

/// Emits a descriptor invocation for callable-array literals whose slots are runtime-selected.
pub(super) fn emit_literal_call(
    callee: &Expr,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    if !is_two_slot_callable_array_literal(callee) {
        return None;
    }
    let callee_ty = crate::codegen::functions::infer_contextual_type(callee, ctx).codegen_repr();
    match callee_ty {
        PhpType::Array(elem_ty) if matches!(elem_ty.codegen_repr(), PhpType::Mixed) => {
            emit_mixed_literal_call(callee, args, emitter, ctx, data);
            Some(PhpType::Mixed)
        }
        PhpType::Array(elem_ty) if matches!(elem_ty.codegen_repr(), PhpType::Str) => {
            emit_string_literal_call(callee, args, emitter, ctx, data);
            Some(PhpType::Mixed)
        }
        _ => None,
    }
}

/// Emits runtime descriptor selection for heterogeneous callable arrays such as `[$object, $method]`.
fn emit_mixed_variable_call(
    var: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let instance_cases =
        crate::codegen::callable_dispatch::runtime_public_instance_method_cases(ctx, data);
    let static_cases =
        crate::codegen::callable_dispatch::runtime_public_static_method_cases(ctx, data);
    emit_mixed_selector_slots(var, emitter, ctx, data);
    emit_mixed_dispatch(
        var,
        args,
        &instance_cases,
        &static_cases,
        emitter,
        ctx,
        data,
    );
    abi::emit_release_temporary_stack(emitter, MIXED_SELECTOR_BYTES);
}

/// Emits runtime descriptor selection for string callable arrays such as `[$class, $method]`.
fn emit_string_variable_call(
    var: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let static_cases =
        crate::codegen::callable_dispatch::runtime_public_static_method_cases(ctx, data);
    emit_string_selector_slots(var, emitter, ctx, data);
    emit_string_dispatch(args, &static_cases, emitter, ctx, data);
    abi::emit_release_temporary_stack(emitter, STRING_SELECTOR_BYTES);
}

/// Emits runtime descriptor selection for heterogeneous callable-array literals.
fn emit_mixed_literal_call(
    callee: &Expr,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let instance_cases =
        crate::codegen::callable_dispatch::runtime_public_instance_method_cases(ctx, data);
    let static_cases =
        crate::codegen::callable_dispatch::runtime_public_static_method_cases(ctx, data);
    super::super::emit_expr(callee, emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the evaluated runtime callable-array literal during descriptor selection
    emit_mixed_literal_selector_slots(emitter);
    emit_mixed_literal_dispatch(args, &instance_cases, &static_cases, emitter, ctx, data);
    abi::emit_release_temporary_stack(emitter, MIXED_SELECTOR_BYTES);           // discard literal callable-array selector slots after invocation
    release_preserved_literal_array_after_mixed_result(
        &PhpType::Array(Box::new(PhpType::Mixed)),
        emitter,
    );
}

/// Emits runtime descriptor selection for string callable-array literals.
fn emit_string_literal_call(
    callee: &Expr,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let static_cases =
        crate::codegen::callable_dispatch::runtime_public_static_method_cases(ctx, data);
    super::super::emit_expr(callee, emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the evaluated runtime string callable-array literal during descriptor selection
    emit_string_literal_selector_slots(emitter);
    emit_string_dispatch(args, &static_cases, emitter, ctx, data);
    abi::emit_release_temporary_stack(emitter, STRING_SELECTOR_BYTES);          // discard literal callable-array selector slots after invocation
    release_preserved_literal_array_after_mixed_result(
        &PhpType::Array(Box::new(PhpType::Str)),
        emitter,
    );
}

/// Saves the unboxed receiver and method slots for a runtime heterogeneous callable-array dispatch.
fn emit_mixed_selector_slots(
    var: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.comment("runtime callable-array mixed selector");
    let receiver = callable_array_slot_expr(var, 0);
    super::super::emit_expr(&receiver, emitter, ctx, data);
    emit_unbox_mixed_result(emitter);
    emit_push_mixed_unbox_payload(emitter);

    let method = callable_array_slot_expr(var, 1);
    super::super::emit_expr(&method, emitter, ctx, data);
    emit_unbox_mixed_result(emitter);
    emit_push_mixed_unbox_payload(emitter);
}

/// Saves class and method string slots for a runtime string callable-array dispatch.
fn emit_string_selector_slots(
    var: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.comment("runtime callable-array string selector");
    let class = callable_array_slot_expr(var, 0);
    super::super::emit_expr(&class, emitter, ctx, data);
    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
    abi::emit_push_reg_pair(emitter, ptr_reg, len_reg);                         // preserve the runtime class string while the method slot is read

    let method = callable_array_slot_expr(var, 1);
    super::super::emit_expr(&method, emitter, ctx, data);
    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
    abi::emit_push_reg_pair(emitter, ptr_reg, len_reg);                         // preserve the runtime method string for descriptor-case selection
}

/// Saves selector slots read from an already-evaluated mixed callable-array literal.
fn emit_mixed_literal_selector_slots(emitter: &mut Emitter) {
    emitter.comment("runtime callable-array literal mixed selector");
    emit_unbox_mixed_literal_slot(0, 0, emitter);
    emit_push_mixed_unbox_payload(emitter);
    emit_unbox_mixed_literal_slot(32, 1, emitter);
    emit_push_mixed_unbox_payload(emitter);
}

/// Saves selector slots read from an already-evaluated string callable-array literal.
fn emit_string_literal_selector_slots(emitter: &mut Emitter) {
    emitter.comment("runtime callable-array literal string selector");
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
    abi::emit_push_reg_pair(emitter, ptr_reg, len_reg);                         // preserve the literal callable-array string slot for descriptor-case selection
}

/// Unboxes the current Mixed result into the target-specific tag and payload registers.
fn emit_unbox_mixed_result(emitter: &mut Emitter) {
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
}

/// Pushes the tag and payload returned by `__rt_mixed_unbox` onto the temporary stack.
fn emit_push_mixed_unbox_payload(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg_pair(emitter, "x1", "x2");                      // preserve the unboxed Mixed payload words for runtime callable selection
            abi::emit_push_reg(emitter, "x0");                                  // preserve the unboxed Mixed tag beside its payload words
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rdi", "rdx");                    // preserve the unboxed Mixed payload words for runtime callable selection
            abi::emit_push_reg(emitter, "rax");                                 // preserve the unboxed Mixed tag beside its payload words
        }
    }
}

/// Dispatches a heterogeneous callable array to a descriptor selected from runtime receiver/method data.
#[allow(clippy::too_many_arguments)]
fn emit_mixed_dispatch(
    var: &str,
    args: &[Expr],
    instance_cases: &[RuntimeInstanceMethodCallableCase],
    static_cases: &[RuntimeStaticMethodCallableCase],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let done_label = ctx.next_label("callable_array_runtime_done");
    for case in instance_cases {
        let next_case = ctx.next_label("callable_array_instance_next");
        emit_branch_if_mixed_instance_case_mismatch(case, &next_case, emitter, ctx, data);
        emit_instance_case_call(var, args, &case.case, emitter, ctx, data);
        abi::emit_jump(emitter, &done_label);
        emitter.label(&next_case);
    }
    for case in static_cases {
        let next_case = ctx.next_label("callable_array_static_next");
        emit_branch_if_mixed_static_case_mismatch(case, &next_case, emitter, ctx, data);
        emit_static_case_call(args, &case.case, emitter, ctx, data);
        abi::emit_jump(emitter, &done_label);
        emitter.label(&next_case);
    }
    emit_no_match_abort(emitter, data);
    emitter.label(&done_label);
}

/// Dispatches a heterogeneous callable-array literal to a descriptor selected at runtime.
fn emit_mixed_literal_dispatch(
    args: &[Expr],
    instance_cases: &[RuntimeInstanceMethodCallableCase],
    static_cases: &[RuntimeStaticMethodCallableCase],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let done_label = ctx.next_label("callable_array_runtime_done");
    for case in instance_cases {
        let next_case = ctx.next_label("callable_array_instance_next");
        emit_branch_if_mixed_instance_case_mismatch(case, &next_case, emitter, ctx, data);
        emit_instance_literal_case_call(args, &case.case, emitter, ctx, data);
        abi::emit_jump(emitter, &done_label);
        emitter.label(&next_case);
    }
    for case in static_cases {
        let next_case = ctx.next_label("callable_array_static_next");
        emit_branch_if_mixed_static_case_mismatch(case, &next_case, emitter, ctx, data);
        emit_static_case_call(args, &case.case, emitter, ctx, data);
        abi::emit_jump(emitter, &done_label);
        emitter.label(&next_case);
    }
    emit_no_match_abort(emitter, data);
    emitter.label(&done_label);
}

/// Dispatches a string callable array to a static-method descriptor selected from runtime strings.
fn emit_string_dispatch(
    args: &[Expr],
    static_cases: &[RuntimeStaticMethodCallableCase],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let done_label = ctx.next_label("callable_array_runtime_done");
    for case in static_cases {
        let next_case = ctx.next_label("callable_array_static_next");
        emit_branch_if_string_static_case_mismatch(case, &next_case, emitter, ctx, data);
        emit_static_case_call(args, &case.case, emitter, ctx, data);
        abi::emit_jump(emitter, &done_label);
        emitter.label(&next_case);
    }
    emit_no_match_abort(emitter, data);
    emitter.label(&done_label);
}

/// Emits the descriptor call for one selected runtime instance-method callable-array case.
fn emit_instance_case_call(
    var: &str,
    args: &[Expr],
    case: &RuntimeCallableCase,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let receiver = callable_array_slot_expr(var, 0);
    let mut descriptor_args = Vec::with_capacity(args.len() + 1);
    descriptor_args.push(receiver);
    descriptor_args.extend(args.iter().cloned());
    let _ = super::emit_callable_array_descriptor_case_call(
        &case.descriptor_label,
        &case.sig,
        &descriptor_args,
        emitter,
        ctx,
        data,
    );
}

/// Emits the descriptor call for a runtime literal instance-method callable-array case.
fn emit_instance_literal_case_call(
    args: &[Expr],
    case: &RuntimeCallableCase,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.comment("call runtime literal callable-array descriptor");
    let save_concat_before_args =
        emitter.target.arch == crate::codegen::platform::Arch::X86_64;
    if save_concat_before_args {
        super::super::save_concat_offset_before_nested_call(emitter, ctx);
    }
    let concat_save_stack_bytes = if save_concat_before_args {
        concat_save_stack_bytes(emitter, ctx)
    } else {
        0
    };
    let object_stack_offset =
        MIXED_RECEIVER_PAYLOAD_OFFSET + concat_save_stack_bytes;
    let arr_ty = if let Some(arg_array) = single_spread_inner(args) {
        let arg_array_ty = crate::codegen::functions::infer_contextual_type(arg_array, ctx);
        let emitted_saved_args = receiver_call_args::emit_saved_receiver_prefixed_dynamic_arg_mixed(
            object_stack_offset,
            arg_array,
            &arg_array_ty,
            emitter,
            ctx,
            data,
        );
        if emitted_saved_args {
            PhpType::Mixed
        } else {
            super::descriptor_invoker_args::emit_descriptor_invoker_arg_array_with_saved_object_prefix(
                object_stack_offset,
                args,
                Some(&case.sig),
                Span::dummy(),
                emitter,
                ctx,
                data,
            )
        }
    } else {
        super::descriptor_invoker_args::emit_descriptor_invoker_arg_array_with_saved_object_prefix(
            object_stack_offset,
            args,
            Some(&case.sig),
            Span::dummy(),
            emitter,
            ctx,
            data,
        )
    };
    let call_reg = abi::nested_call_reg(emitter);
    abi::emit_symbol_address(emitter, call_reg, &case.descriptor_label);
    crate::codegen::builtins::arrays::call_user_func_array::emit_call_descriptor_array_invoker(
        crate::codegen::builtins::arrays::call_user_func_array::LoadedArraySource::Result,
        &arr_ty,
        call_reg,
        save_concat_before_args,
        emitter,
        ctx,
        data,
    );
}

/// Returns how many temporary stack bytes the concat-offset save added.
fn concat_save_stack_bytes(emitter: &Emitter, ctx: &Context) -> usize {
    match emitter.target.arch {
        Arch::AArch64 => 16,
        Arch::X86_64 if ctx.nested_concat_offset_offset.is_none() => 16,
        Arch::X86_64 => 0,
    }
}

/// Returns the inner argument array when descriptor invocation forwards one spread segment.
fn single_spread_inner(args: &[Expr]) -> Option<&Expr> {
    if let [arg] = args {
        if let ExprKind::Spread(inner) = &arg.kind {
            return Some(inner);
        }
    }
    None
}

/// Emits the descriptor call for one selected runtime static-method callable-array case.
fn emit_static_case_call(
    args: &[Expr],
    case: &RuntimeCallableCase,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let _ = super::emit_callable_array_descriptor_case_call(
        &case.descriptor_label,
        &case.sig,
        args,
        emitter,
        ctx,
        data,
    );
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
            emitter.instruction(&format!("cmp x9, #{}", expected_tag));         // compare the saved callable-array runtime tag against this descriptor shape
            emitter.instruction(&format!("b.ne {}", next_case));                // try the next descriptor case when the callable-array slot shape differs
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(emitter, "r10", tag_offset);
            emitter.instruction(&format!("cmp r10, {}", expected_tag));         // compare the saved callable-array runtime tag against this descriptor shape
            emitter.instruction(&format!("jne {}", next_case));                 // try the next descriptor case when the callable-array slot shape differs
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
            emitter.instruction(&format!("cbz x9, {}", next_case));             // reject null receiver pointers before reading their class id
            emitter.instruction("ldr x10, [x9]");                               // load the receiver runtime class id from the object header
            abi::emit_load_int_immediate(emitter, "x11", class_id as i64);
            emitter.instruction("cmp x10, x11");                                // compare receiver class id against this descriptor case
            emitter.instruction(&format!("b.ne {}", next_case));                // try the next descriptor case when the receiver class differs
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(emitter, "r10", receiver_offset);
            emitter.instruction("test r10, r10");                               // reject null receiver pointers before reading their class id
            emitter.instruction(&format!("je {}", next_case));                  // try the next descriptor case when the receiver pointer is null
            emitter.instruction("mov r11, QWORD PTR [r10]");                    // load the receiver runtime class id from the object header
            abi::emit_load_int_immediate(emitter, "r10", class_id as i64);
            emitter.instruction("cmp r11, r10");                                // compare receiver class id against this descriptor case
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
    let matched_label = ctx.next_label("callable_array_class_match");
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
    let matched_label = ctx.next_label("callable_array_string_match");
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
            emitter.instruction("cmp x0, #0");                                  // did the callable-array runtime string match this descriptor name?
            emitter.instruction(&format!("b.eq {}", matched_label));            // select this descriptor case when names match case-insensitively
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(emitter, "rdi", ptr_offset);
            abi::emit_load_temporary_stack_slot(emitter, "rsi", len_offset);
            abi::emit_symbol_address(emitter, "rdx", &expected_label);
            abi::emit_load_int_immediate(emitter, "rcx", expected_len as i64);
            abi::emit_call_label(emitter, "__rt_strcasecmp");
            emitter.instruction("test rax, rax");                               // did the callable-array runtime string match this descriptor name?
            emitter.instruction(&format!("je {}", matched_label));              // select this descriptor case when names match case-insensitively
        }
    }
}

/// Releases the preserved callable-array literal while keeping the Mixed call result live.
fn release_preserved_literal_array_after_mixed_result(arr_ty: &PhpType, emitter: &mut Emitter) {
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the boxed call result while releasing the temporary callable-array literal
    abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), 16);
    abi::emit_decref_if_refcounted(emitter, arr_ty);
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // restore the boxed call result after callable-array literal cleanup
    abi::emit_release_temporary_stack(emitter, 16);                             // discard the preserved callable-array literal slot
}

/// Emits the fatal diagnostic for callable arrays that cannot be resolved to a descriptor.
fn emit_no_match_abort(emitter: &mut Emitter, data: &mut DataSection) {
    let (message_label, message_len) = data.add_string(
        b"Fatal error: callable array did not resolve to an invokable target\n",
    );
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #2");                                  // write the callable-array diagnostic to stderr
            emitter.adrp("x1", &message_label);
            emitter.add_lo12("x1", "x1", &message_label);
            emitter.instruction(&format!("mov x2, #{}", message_len));          // pass the callable-array diagnostic byte length to write()
            emitter.syscall(4);
            abi::emit_exit(emitter, 1);
        }
        Arch::X86_64 => {
            emitter.instruction("mov edi, 2");                                  // write the callable-array diagnostic to stderr
            abi::emit_symbol_address(emitter, "rsi", &message_label);
            emitter.instruction(&format!("mov edx, {}", message_len));          // pass the callable-array diagnostic byte length to write()
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall 1 = write
            emitter.instruction("syscall");                                     // emit the fatal callable-array diagnostic
            abi::emit_exit(emitter, 1);
        }
    }
}

/// Builds `$callback[$index]`, a positional slot stored inside a callable-array value.
fn callable_array_slot_expr(var: &str, index: i64) -> Expr {
    Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(Expr::new(ExprKind::Variable(var.to_string()), Span::dummy())),
            index: Box::new(Expr::new(ExprKind::IntLiteral(index), Span::dummy())),
        },
        Span::dummy(),
    )
}

/// Returns true when an expression is a two-element indexed-array literal.
fn is_two_slot_callable_array_literal(callee: &Expr) -> bool {
    matches!(&callee.kind, ExprKind::ArrayLiteral(elems) if elems.len() == 2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::{Platform, Target};

    /// Verifies x86_64 frame-slot concat saves do not shift saved callable-array selector offsets.
    #[test]
    fn test_concat_save_stack_bytes_tracks_actual_stack_pushes() {
        let x86 = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        let mut x86_frame_ctx = Context::new();
        x86_frame_ctx.nested_concat_offset_offset = Some(24);
        assert_eq!(concat_save_stack_bytes(&x86, &x86_frame_ctx), 0);

        let x86_raw_ctx = Context::new();
        assert_eq!(concat_save_stack_bytes(&x86, &x86_raw_ctx), 16);

        let arm = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        let arm_ctx = Context::new();
        assert_eq!(concat_save_stack_bytes(&arm, &arm_ctx), 16);
    }
}
