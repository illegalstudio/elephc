//! Purpose:
//! Emits descriptor-based runtime callable invokers for the active EIR backend.
//! Adapts the uniform `(descriptor, boxed argument container) -> boxed Mixed` ABI to typed entries.
//!
//! Called from:
//! - `crate::codegen::lower_inst::emit_runtime_callable_invoker_inline()`.
//!
//! Key details:
//! - The invoker accepts only normalized boxed Mixed argument containers.
//! - Capture values are loaded from the callable descriptor, not caller frame state.
//! - Argument materialization supports indexed arrays, associative arrays, defaults, variadics,
//!   by-reference marker cells, and target-aware ABI calls without depending on `Context`.

use crate::codegen::callable_descriptor;
use crate::codegen::callable_invoker_args::{
    emit_branch_if_mixed_arg_tag, emit_call_user_func_array_invalid_mixed_args_abort,
    INVOKER_ARG_REF_CELL_TAG,
};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::codegen::{abi, emit_box_current_value_as_mixed, emit_box_runtime_payload_as_mixed};
use crate::codegen_support::try_handlers::{
    TRY_HANDLER_DIAG_DEPTH_OFFSET, TRY_HANDLER_JMP_BUF_OFFSET, TRY_HANDLER_SLOT_SIZE,
};
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{FunctionSig, PhpType};

const INVOKER_DESCRIPTOR_OFFSET: usize = 8;
const INVOKER_CONCAT_OFFSET: usize = 16;
const INVOKER_FRAME_SIZE: usize = 32;
const INVOKER_ARG_ARRAY_OFFSET: usize = 24;
const INVOKER_BOUNDARY_FRAME_SIZE: usize = INVOKER_FRAME_SIZE + TRY_HANDLER_SLOT_SIZE + 16;
const INVOKER_BOUNDARY_BASE_OFFSET: usize = INVOKER_BOUNDARY_FRAME_SIZE - 16;

/// Runtime invoker metadata emitted beside callable descriptors.
pub(super) struct RuntimeCallableInvoker<'a> {
    pub(super) label: &'a str,
    pub(super) sig: &'a FunctionSig,
    pub(super) captures: &'a [(String, PhpType, bool)],
}

/// Minimal state needed by the descriptor invoker emitter.
struct InvokerEmitContext {
    label_prefix: String,
    label_counter: usize,
}

impl InvokerEmitContext {
    /// Creates a fresh label context for one generated invoker body.
    fn new(invoker_label: &str) -> Self {
        Self {
            label_prefix: local_label_prefix(invoker_label),
            label_counter: 0,
        }
    }

    /// Allocates a deterministic local label for generated branches.
    fn next_label(&mut self, prefix: &str) -> String {
        let id = self.label_counter;
        self.label_counter += 1;
        format!("{}_{}_{}", self.label_prefix, prefix, id)
    }
}

/// Converts an invoker's global assembly label into a safe prefix for its local labels.
fn local_label_prefix(label: &str) -> String {
    label
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

/// Emits a descriptor invoker wrapper for a runtime-callable signature.
pub(super) fn emit_runtime_callable_invoker(
    emitter: &mut Emitter,
    data: &mut DataSection,
    invoker: &RuntimeCallableInvoker<'_>,
) {
    emit_runtime_callable_invoker_impl(emitter, data, invoker, false);
}

/// Emits a descriptor invoker wrapper that catches native throws for eval callbacks.
pub(crate) fn emit_runtime_callable_invoker_with_exception_boundary(
    emitter: &mut Emitter,
    data: &mut DataSection,
    invoker: &RuntimeCallableInvoker<'_>,
) {
    emit_runtime_callable_invoker_impl(emitter, data, invoker, true);
}

/// Emits a descriptor invoker wrapper, optionally bounded by an exception handler.
fn emit_runtime_callable_invoker_impl(
    emitter: &mut Emitter,
    data: &mut DataSection,
    invoker: &RuntimeCallableInvoker<'_>,
    catch_native_throws: bool,
) {
    let mut ctx = InvokerEmitContext::new(invoker.label);
    let call_reg = abi::nested_call_reg(emitter);
    let escape_label = format!("{}_eval_escape", invoker.label);
    let frame_size = if catch_native_throws {
        INVOKER_BOUNDARY_FRAME_SIZE
    } else {
        INVOKER_FRAME_SIZE
    };

    emitter.blank();
    emitter.comment(&format!("runtime callable invoker {}", invoker.label));
    emitter.raw(".align 2");
    emitter.label_global(invoker.label);
    abi::emit_frame_prologue(emitter, frame_size);
    abi::store_at_offset(
        emitter,
        abi::int_arg_reg_name(emitter.target, 0),
        INVOKER_DESCRIPTOR_OFFSET,
    );
    if catch_native_throws {
        abi::store_at_offset(
            emitter,
            abi::int_arg_reg_name(emitter.target, 1),
            INVOKER_ARG_ARRAY_OFFSET,
        );
        emit_invoker_exception_boundary_push(
            emitter,
            INVOKER_BOUNDARY_BASE_OFFSET,
            &escape_label,
        );
        abi::load_at_offset(
            emitter,
            abi::int_arg_reg_name(emitter.target, 1),
            INVOKER_ARG_ARRAY_OFFSET,
        );
        emit_saved_descriptor_entry_to_call_reg(emitter, call_reg);
    } else {
        emit_descriptor_entry_to_call_reg(emitter, call_reg);
    }

    let ret_ty = emit_loaded_array_callback_call(
        LoadedArraySource::ArgumentRegister(1),
        &PhpType::Mixed,
        call_reg,
        invoker.captures,
        invoker.sig,
        emitter,
        &mut ctx,
        data,
    );
    emit_box_current_value_as_mixed(emitter, &ret_ty.codegen_repr());
    if catch_native_throws {
        emit_invoker_exception_boundary_pop(emitter, INVOKER_BOUNDARY_BASE_OFFSET);
    }
    abi::emit_frame_restore(emitter, frame_size);
    abi::emit_return(emitter);
    if catch_native_throws {
        emitter.label(&escape_label);
        emit_invoker_exception_boundary_pop(emitter, INVOKER_BOUNDARY_BASE_OFFSET);
        emit_null_invoker_result(emitter);
        abi::emit_frame_restore(emitter, frame_size);
        abi::emit_return(emitter);
    }
}

/// Loads the descriptor entry slot from the first invoker argument into `call_reg`.
fn emit_descriptor_entry_to_call_reg(emitter: &mut Emitter, call_reg: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov {}, x0", call_reg));              // keep descriptor while loading its native entry
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov {}, rdi", call_reg));             // keep descriptor while loading its native entry
        }
    }
    callable_descriptor::emit_load_entry_from_descriptor(emitter, call_reg, call_reg);
}

/// Loads the saved descriptor entry slot into `call_reg` after a `setjmp` boundary.
fn emit_saved_descriptor_entry_to_call_reg(emitter: &mut Emitter, call_reg: &str) {
    abi::load_at_offset(emitter, call_reg, INVOKER_DESCRIPTOR_OFFSET);
    callable_descriptor::emit_load_entry_from_descriptor(emitter, call_reg, call_reg);
}

/// Pushes a native exception boundary around an eval-owned descriptor invoker call.
fn emit_invoker_exception_boundary_push(
    emitter: &mut Emitter,
    handler_base: usize,
    escape_label: &str,
) {
    emitter.comment("push eval callable exception boundary");
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg(emitter, "x10", "_exc_handler_top", 0);
            emitter.instruction(&format!("stur x10, [x29, #-{}]", handler_base)); // save the previous native exception-handler head
            abi::emit_load_symbol_to_reg(emitter, "x10", "_exc_call_frame_top", 0);
            emitter.instruction(&format!("stur x10, [x29, #-{}]", handler_base - 8)); // preserve the caller activation frame across callable unwinding
            abi::emit_load_symbol_to_reg(emitter, "x10", "_rt_diag_suppression", 0);
            emitter.instruction(&format!(
                "stur x10, [x29, #-{}]",
                handler_base - TRY_HANDLER_DIAG_DEPTH_OFFSET
            )); // save diagnostic suppression depth for restoration
            emitter.instruction(&format!("sub x10, x29, #{}", handler_base)); // compute the boundary handler record address
            abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0);
            emitter.instruction(&format!(
                "sub x0, x29, #{}",
                handler_base - TRY_HANDLER_JMP_BUF_OFFSET
            )); // pass the boundary jmp_buf to setjmp
            emitter.bl_c("setjmp"); // snapshot the bridge stack before entering the callable
            emitter.instruction(&format!("cbnz x0, {}", escape_label)); // non-zero setjmp result means a callable Throwable escaped
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_handler_top", 0);
            emitter.instruction(&format!("mov QWORD PTR [rbp - {}], r10", handler_base)); // save the previous native exception-handler head
            abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_call_frame_top", 0);
            emitter.instruction(&format!("mov QWORD PTR [rbp - {}], r10", handler_base - 8)); // preserve the caller activation frame across callable unwinding
            abi::emit_load_symbol_to_reg(emitter, "r10", "_rt_diag_suppression", 0);
            emitter.instruction(&format!(
                "mov QWORD PTR [rbp - {}], r10",
                handler_base - TRY_HANDLER_DIAG_DEPTH_OFFSET
            )); // save diagnostic suppression depth for restoration
            emitter.instruction(&format!("lea r10, [rbp - {}]", handler_base)); // compute the boundary handler record address
            abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0);
            emitter.instruction(&format!(
                "lea rdi, [rbp - {}]",
                handler_base - TRY_HANDLER_JMP_BUF_OFFSET
            )); // pass the boundary jmp_buf to setjmp
            emitter.bl_c("setjmp"); // snapshot the bridge stack before entering the callable
            emitter.instruction("test eax, eax"); // did control arrive through longjmp?
            emitter.instruction(&format!("jne {}", escape_label)); // non-zero setjmp result means a callable Throwable escaped
        }
    }
}

/// Pops the native exception boundary around an eval-owned descriptor invoker call.
fn emit_invoker_exception_boundary_pop(emitter: &mut Emitter, handler_base: usize) {
    emitter.comment("pop eval callable exception boundary");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldur x10, [x29, #-{}]", handler_base)); // reload the previous native exception-handler head
            abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0);
            emitter.instruction(&format!(
                "ldur x10, [x29, #-{}]",
                handler_base - TRY_HANDLER_DIAG_DEPTH_OFFSET
            )); // reload the saved diagnostic suppression depth
            abi::emit_store_reg_to_symbol(emitter, "x10", "_rt_diag_suppression", 0);
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov r10, QWORD PTR [rbp - {}]", handler_base)); // reload the previous native exception-handler head
            abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0);
            emitter.instruction(&format!(
                "mov r10, QWORD PTR [rbp - {}]",
                handler_base - TRY_HANDLER_DIAG_DEPTH_OFFSET
            )); // reload the saved diagnostic suppression depth
            abi::emit_store_reg_to_symbol(emitter, "r10", "_rt_diag_suppression", 0);
        }
    }
}

/// Leaves a null boxed-Mixed result for Rust to translate into a pending throwable.
fn emit_null_invoker_result(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, xzr"); // return null so magician takes the pending Throwable
        }
        Arch::X86_64 => {
            emitter.instruction("xor eax, eax"); // return null so magician takes the pending Throwable
        }
    }
}

/// Source location for a callback argument array already materialized by caller code.
#[derive(Clone, Copy)]
enum LoadedArraySource {
    TemporaryStackSlot(usize),
    ArgumentRegister(usize),
}

/// Loads a callback argument array source into `dest_reg`.
fn emit_loaded_array_source_to_reg(
    array_source: LoadedArraySource,
    dest_reg: &str,
    emitter: &mut Emitter,
) {
    match array_source {
        LoadedArraySource::TemporaryStackSlot(offset) => {
            abi::emit_load_temporary_stack_slot(emitter, dest_reg, offset);
        }
        LoadedArraySource::ArgumentRegister(index) => {
            let arg_reg = abi::int_arg_reg_name(emitter.target, index);
            if arg_reg != dest_reg {
                emitter.instruction(&format!("mov {}, {}", dest_reg, arg_reg)); // copy invoker ABI argument into the selected scratch
            }
        }
    }
}

/// Emits a loaded argument-container callback call.
fn emit_loaded_array_callback_call(
    array_source: LoadedArraySource,
    arr_ty: &PhpType,
    call_reg: &str,
    captures: &[(String, PhpType, bool)],
    sig: &FunctionSig,
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    data: &mut DataSection,
) -> PhpType {
    if matches!(arr_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        return emit_loaded_mixed_array_callback_call(
            array_source,
            call_reg,
            captures,
            sig,
            emitter,
            ctx,
            data,
        );
    }
    if matches!(arr_ty, PhpType::AssocArray { .. }) {
        return emit_loaded_assoc_array_callback_call(
            array_source,
            arr_ty,
            call_reg,
            captures,
            sig,
            emitter,
            ctx,
            data,
        );
    }
    emit_loaded_indexed_array_callback_call(
        array_source,
        arr_ty,
        call_reg,
        captures,
        sig,
        emitter,
        ctx,
        data,
    )
}

/// Emits callback dispatch for a boxed Mixed argument container.
fn emit_loaded_mixed_array_callback_call(
    array_source: LoadedArraySource,
    call_reg: &str,
    captures: &[(String, PhpType, bool)],
    sig: &FunctionSig,
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    data: &mut DataSection,
) -> PhpType {
    let (mixed_reg, tag_reg, payload_reg) = match emitter.target.arch {
        Arch::AArch64 => ("x20", "x21", "x22"),
        Arch::X86_64 => ("r13", "r14", "r15"),
    };
    let indexed_label = ctx.next_label("cufa_mixed_indexed");
    let assoc_label = ctx.next_label("cufa_mixed_assoc");
    let done_label = ctx.next_label("cufa_mixed_done");
    let indexed_ty = PhpType::Array(Box::new(PhpType::Mixed));
    let assoc_ty = PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(PhpType::Mixed),
    };

    emit_loaded_array_source_to_reg(array_source, mixed_reg, emitter);
    abi::emit_load_from_address(emitter, tag_reg, mixed_reg, 0);
    abi::emit_load_from_address(emitter, payload_reg, mixed_reg, 8);
    abi::emit_push_reg(emitter, payload_reg); // preserve unboxed argument payload while branching by shape
    emit_branch_if_mixed_arg_tag(
        tag_reg,
        crate::codegen::runtime_value_tag(&indexed_ty),
        &indexed_label,
        emitter,
    );
    emit_branch_if_mixed_arg_tag(
        tag_reg,
        crate::codegen::runtime_value_tag(&assoc_ty),
        &assoc_label,
        emitter,
    );
    emit_call_user_func_array_invalid_mixed_args_abort(emitter, data);

    emitter.label(&indexed_label);
    emit_loaded_array_callback_call(
        LoadedArraySource::TemporaryStackSlot(0),
        &indexed_ty,
        call_reg,
        captures,
        sig,
        emitter,
        ctx,
        data,
    );
    abi::emit_jump(emitter, &done_label);

    emitter.label(&assoc_label);
    emit_loaded_assoc_array_callback_call(
        LoadedArraySource::TemporaryStackSlot(0),
        &assoc_ty,
        call_reg,
        captures,
        sig,
        emitter,
        ctx,
        data,
    );
    abi::emit_jump(emitter, &done_label);

    emitter.label(&done_label);
    abi::emit_release_temporary_stack(emitter, 16); // discard preserved borrowed argument-container payload
    sig.return_type.clone()
}

/// Emits callback dispatch for an indexed argument array.
#[allow(clippy::too_many_arguments)]
fn emit_loaded_indexed_array_callback_call(
    array_source: LoadedArraySource,
    arr_ty: &PhpType,
    call_reg: &str,
    captures: &[(String, PhpType, bool)],
    sig: &FunctionSig,
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    data: &mut DataSection,
) -> PhpType {
    let (
        array_reg,
        len_reg,
        tail_count_reg,
        tail_index_reg,
        index_reg,
        offset_reg,
        data_reg,
        peek_reg,
        array_new_capacity_reg,
        array_new_elem_size_reg,
        len_store_reg,
    ) = match emitter.target.arch {
        Arch::AArch64 => (
            "x20", "x21", "x22", "x23", "x24", "x25", "x26", "x9", "x0", "x1", "x10",
        ),
        Arch::X86_64 => (
            "r13", "r14", "r15", "rbx", "rcx", "r8", "r9", "r11", "rdi", "rsi", "r10",
        ),
    };
    let elem_ty = match arr_ty {
        PhpType::Array(t) => *t.clone(),
        PhpType::AssocArray { value, .. } => *value.clone(),
        _ => PhpType::Mixed,
    };
    let elem_size = array_element_stride(&elem_ty);
    let visible_param_count = sig.params.len();
    let regular_param_count = if sig.variadic.is_some() {
        visible_param_count.saturating_sub(1)
    } else {
        visible_param_count
    };

    emit_loaded_array_source_to_reg(array_source, array_reg, emitter);
    abi::emit_load_from_address(emitter, len_reg, array_reg, 0);
    emit_indexed_required_arg_count_check(sig, regular_param_count, len_reg, emitter, ctx, data);

    let mut arg_types = Vec::new();
    for index in 0..regular_param_count {
        let has_default = sig.defaults.get(index).and_then(Option::as_ref).is_some();
        let target_ty = callback_arg_target_ty(sig, index, has_default, &elem_ty);
        let is_ref = sig.ref_params.get(index).copied().unwrap_or(false);
        if is_ref {
            if let Some(default_expr) = sig.defaults.get(index).and_then(Option::as_ref) {
                let load_label = ctx.next_label("invoker_ref_load_arg");
                let done_label = ctx.next_label("invoker_ref_arg_done");
                emit_compare_len_ge(emitter, len_reg, index + 1, &load_label);
                push_default_ref_arg(default_expr, target_ty, emitter, ctx, data);
                abi::emit_jump(emitter, &done_label);
                emitter.label(&load_label);
                load_array_element_to_result(emitter, &elem_ty, array_reg, 24 + index * elem_size);
                push_loaded_indexed_array_ref_arg(&elem_ty, target_ty, emitter, ctx, data);
                emitter.label(&done_label);
            } else {
                load_array_element_to_result(emitter, &elem_ty, array_reg, 24 + index * elem_size);
                push_loaded_indexed_array_ref_arg(&elem_ty, target_ty, emitter, ctx, data);
            }
            arg_types.push(PhpType::Int);
            continue;
        }

        let pushed_ty = target_ty
            .map(PhpType::codegen_repr)
            .unwrap_or_else(|| elem_ty.codegen_repr());
        if let Some(default_expr) = sig.defaults.get(index).and_then(Option::as_ref) {
            let load_label = ctx.next_label("invoker_load_arg");
            let done_label = ctx.next_label("invoker_arg_done");
            emit_compare_len_ge(emitter, len_reg, index + 1, &load_label);
            push_default_value_arg(default_expr, target_ty, emitter, ctx, data);
            abi::emit_jump(emitter, &done_label);
            emitter.label(&load_label);
            load_array_element_to_result(emitter, &elem_ty, array_reg, 24 + index * elem_size);
            push_loaded_indexed_array_value_arg(&elem_ty, target_ty, emitter, ctx, data);
            emitter.label(&done_label);
        } else {
            load_array_element_to_result(emitter, &elem_ty, array_reg, 24 + index * elem_size);
            push_loaded_indexed_array_value_arg(&elem_ty, target_ty, emitter, ctx, data);
        }
        arg_types.push(pushed_ty);
    }

    if sig.variadic.is_some() {
        let variadic_elem_ty = sig
            .params
            .get(visible_param_count.saturating_sub(1))
            .and_then(|(_, ty)| match ty {
                PhpType::Array(elem) => Some((**elem).clone()),
                _ => None,
            })
            .unwrap_or_else(|| elem_ty.clone());
        let build_label = ctx.next_label("invoker_build_variadic");
        let done_label = ctx.next_label("invoker_variadic_done");
        emit_compare_len_gt(emitter, len_reg, regular_param_count, &build_label);
        emit_empty_indexed_array(emitter, &variadic_elem_ty);
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
        abi::emit_jump(emitter, &done_label);

        emitter.label(&build_label);
        emit_tail_count(emitter, tail_count_reg, len_reg, regular_param_count);
        emitter.instruction(&format!(
            "mov {}, {}",
            array_new_capacity_reg, tail_count_reg
        ));
        abi::emit_load_int_immediate(
            emitter,
            array_new_elem_size_reg,
            variadic_elem_ty.stack_size() as i64,
        );
        abi::emit_call_label(emitter, "__rt_array_new");
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
        emitter.instruction(&format!(
            "mov {}, {}",
            peek_reg,
            abi::int_result_reg(emitter)
        ));
        crate::codegen::emit_array_value_type_stamp(emitter, peek_reg, &variadic_elem_ty);
        abi::emit_load_int_immediate(emitter, tail_index_reg, 0);
        let loop_label = ctx.next_label("invoker_variadic_loop");
        let loop_done_label = ctx.next_label("invoker_variadic_loop_done");
        emitter.label(&loop_label);
        emit_compare_reg_ge(emitter, tail_index_reg, tail_count_reg, &loop_done_label);
        emitter.instruction(&format!("mov {}, {}", index_reg, tail_index_reg));
        emit_add_usize_if_nonzero(emitter, index_reg, regular_param_count);
        emitter.instruction(&format!("mov {}, {}", data_reg, array_reg));
        emit_add_usize(emitter, data_reg, 24);
        emit_scale_index_to_offset(emitter, offset_reg, index_reg, elem_size);
        emit_add_reg(emitter, data_reg, offset_reg);
        load_array_element_to_result(emitter, &elem_ty, data_reg, 0);
        let (stored_ty, boxed_to_mixed) =
            coerce_current_value_to_target(emitter, ctx, data, &elem_ty, Some(&variadic_elem_ty));
        if !boxed_to_mixed {
            abi::emit_incref_if_refcounted(emitter, &stored_ty);
        }
        abi::emit_load_temporary_stack_slot(emitter, peek_reg, 0);
        emit_store_current_value_to_array_slot(
            emitter,
            &stored_ty,
            peek_reg,
            len_store_reg,
            offset_reg,
            tail_index_reg,
        );
        emit_increment_reg(emitter, tail_index_reg);
        abi::emit_store_to_address(emitter, tail_index_reg, peek_reg, 0);
        abi::emit_jump(emitter, &loop_label);
        emitter.label(&loop_done_label);
        emitter.label(&done_label);
        let variadic_ty = PhpType::Array(Box::new(variadic_elem_ty));
        if variadic_param_is_by_ref(sig) {
            wrap_pushed_value_in_ref_cell(emitter, &variadic_ty);
            arg_types.push(PhpType::Int);
        } else {
            arg_types.push(variadic_ty);
        }
    }

    push_descriptor_captures_as_hidden_args(captures, emitter, &mut arg_types);
    call_target_with_pushed_args(call_reg, &arg_types, sig, emitter);
    sig.return_type.clone()
}

/// Emits callback dispatch for an associative argument array.
#[allow(clippy::too_many_arguments)]
fn emit_loaded_assoc_array_callback_call(
    array_source: LoadedArraySource,
    arr_ty: &PhpType,
    call_reg: &str,
    captures: &[(String, PhpType, bool)],
    sig: &FunctionSig,
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    data: &mut DataSection,
) -> PhpType {
    let hash_reg = match emitter.target.arch {
        Arch::AArch64 => "x20",
        Arch::X86_64 => "r13",
    };
    let elem_ty = match arr_ty {
        PhpType::AssocArray { value, .. } => *value.clone(),
        _ => PhpType::Mixed,
    };
    emit_loaded_array_source_to_reg(array_source, hash_reg, emitter);

    let visible_param_count = sig.params.len();
    let regular_param_count = if sig.variadic.is_some() {
        visible_param_count.saturating_sub(1)
    } else {
        visible_param_count
    };
    let mut arg_types = Vec::new();

    for index in 0..regular_param_count {
        let has_default = sig.defaults.get(index).and_then(Option::as_ref).is_some();
        let target_ty = callback_arg_target_ty(sig, index, has_default, &elem_ty);
        let param_name = sig.params.get(index).map(|(name, _)| name.as_str());
        emit_hash_lookup_for_param_or_index(hash_reg, param_name, index, emitter, ctx, data);

        let is_ref = sig.ref_params.get(index).copied().unwrap_or(false);
        if is_ref {
            if let Some(default_expr) = sig.defaults.get(index).and_then(Option::as_ref) {
                let use_default = ctx.next_label("invoker_assoc_ref_default");
                let done = ctx.next_label("invoker_assoc_ref_done");
                abi::emit_branch_if_int_result_zero(emitter, &use_default);
                push_loaded_hash_value_ref_arg(&elem_ty, target_ty, emitter, ctx, data);
                abi::emit_jump(emitter, &done);
                emitter.label(&use_default);
                push_default_ref_arg(default_expr, target_ty, emitter, ctx, data);
                emitter.label(&done);
            } else {
                let missing = ctx.next_label("invoker_assoc_ref_missing");
                let done = ctx.next_label("invoker_assoc_ref_done");
                abi::emit_branch_if_int_result_zero(emitter, &missing);
                push_loaded_hash_value_ref_arg(&elem_ty, target_ty, emitter, ctx, data);
                abi::emit_jump(emitter, &done);
                emitter.label(&missing);
                emit_call_user_func_array_missing_arg_abort(emitter, data);
                emitter.label(&done);
            }
            arg_types.push(PhpType::Int);
            continue;
        }

        let pushed_ty = if let Some(default_expr) = sig.defaults.get(index).and_then(Option::as_ref)
        {
            let use_default = ctx.next_label("invoker_assoc_default");
            let done = ctx.next_label("invoker_assoc_done");
            abi::emit_branch_if_int_result_zero(emitter, &use_default);
            let loaded_ty = push_loaded_hash_value_arg(&elem_ty, target_ty, emitter, ctx, data);
            abi::emit_jump(emitter, &done);
            emitter.label(&use_default);
            let default_ty = push_default_value_arg(default_expr, target_ty, emitter, ctx, data);
            emitter.label(&done);
            widen_callback_arg_type(&loaded_ty, &default_ty)
        } else {
            let missing = ctx.next_label("invoker_assoc_missing");
            let done = ctx.next_label("invoker_assoc_done");
            abi::emit_branch_if_int_result_zero(emitter, &missing);
            let loaded_ty = push_loaded_hash_value_arg(&elem_ty, target_ty, emitter, ctx, data);
            abi::emit_jump(emitter, &done);
            emitter.label(&missing);
            emit_call_user_func_array_missing_arg_abort(emitter, data);
            emitter.label(&done);
            loaded_ty
        };
        arg_types.push(pushed_ty);
    }

    if sig.variadic.is_some() {
        let variadic_ty = emit_loaded_assoc_variadic_array_arg(
            hash_reg,
            &elem_ty,
            sig,
            regular_param_count,
            regular_param_count,
            emitter,
            ctx,
            data,
        );
        if variadic_param_is_by_ref(sig) {
            wrap_pushed_value_in_ref_cell(emitter, &variadic_ty);
            arg_types.push(PhpType::Int);
        } else {
            arg_types.push(variadic_ty);
        }
    }

    push_descriptor_captures_as_hidden_args(captures, emitter, &mut arg_types);
    call_target_with_pushed_args(call_reg, &arg_types, sig, emitter);
    sig.return_type.clone()
}

/// Computes a target type for one callback parameter.
fn callback_arg_target_ty<'a>(
    sig: &'a FunctionSig,
    index: usize,
    has_default: bool,
    source_elem_ty: &PhpType,
) -> Option<&'a PhpType> {
    if declared_target_ty(Some(sig), index).is_some()
        || has_default
        || matches!(source_elem_ty.codegen_repr(), PhpType::Mixed)
    {
        sig.params.get(index).map(|(_, ty)| ty)
    } else {
        None
    }
}

/// Returns whether the visible variadic parameter is declared by-reference.
fn variadic_param_is_by_ref(sig: &FunctionSig) -> bool {
    sig.variadic.is_some()
        && sig
            .ref_params
            .get(sig.params.len().saturating_sub(1))
            .copied()
            .unwrap_or(false)
}

/// Returns the declared target PHP type for a parameter.
fn declared_target_ty<'a>(sig: Option<&'a FunctionSig>, param_idx: usize) -> Option<&'a PhpType> {
    sig.and_then(|sig| {
        let target_ty = sig.params.get(param_idx).map(|(_, ty)| ty)?;
        if sig.declared_params.get(param_idx).copied().unwrap_or(false)
            || matches!(target_ty.codegen_repr(), PhpType::Mixed)
        {
            Some(target_ty)
        } else {
            None
        }
    })
}

/// Emits a required-argument count check for indexed argument containers.
fn emit_indexed_required_arg_count_check(
    sig: &FunctionSig,
    regular_param_count: usize,
    len_reg: &str,
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    data: &mut DataSection,
) {
    let required_count = (0..regular_param_count)
        .filter(|idx| sig.defaults.get(*idx).and_then(Option::as_ref).is_none())
        .map(|idx| idx + 1)
        .max()
        .unwrap_or(0);
    if required_count == 0 {
        return;
    }
    let ok_label = ctx.next_label("invoker_indexed_required_ok");
    emit_compare_len_ge(emitter, len_reg, required_count, &ok_label);
    emit_call_user_func_array_missing_arg_abort(emitter, data);
    emitter.label(&ok_label);
}

/// Emits a length >= immediate branch.
fn emit_compare_len_ge(emitter: &mut Emitter, len_reg: &str, value: usize, label: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {}, #{}", len_reg, value));       // compare runtime argument count against required bound
            emitter.instruction(&format!("b.ge {}", label));
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {}, {}", len_reg, value));        // compare runtime argument count against required bound
            emitter.instruction(&format!("jge {}", label));
        }
    }
}

/// Emits a length > immediate branch.
fn emit_compare_len_gt(emitter: &mut Emitter, len_reg: &str, value: usize, label: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {}, #{}", len_reg, value));       // compare runtime argument count against fixed prefix length
            emitter.instruction(&format!("b.gt {}", label));
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {}, {}", len_reg, value));        // compare runtime argument count against fixed prefix length
            emitter.instruction(&format!("jg {}", label));
        }
    }
}

/// Emits a register >= register branch.
fn emit_compare_reg_ge(emitter: &mut Emitter, left_reg: &str, right_reg: &str, label: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {}, {}", left_reg, right_reg));   // compare variadic loop index against tail count
            emitter.instruction(&format!("b.ge {}", label));
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {}, {}", left_reg, right_reg));   // compare variadic loop index against tail count
            emitter.instruction(&format!("jge {}", label));
        }
    }
}

/// Computes `dest_reg = len_reg - regular_param_count`.
fn emit_tail_count(
    emitter: &mut Emitter,
    dest_reg: &str,
    len_reg: &str,
    regular_param_count: usize,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!(
                "sub {}, {}, #{}",
                dest_reg, len_reg, regular_param_count
            ));
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov {}, {}", dest_reg, len_reg));
            emitter.instruction(&format!("sub {}, {}", dest_reg, regular_param_count));
        }
    }
}

/// Adds an immediate to a register.
fn emit_add_usize(emitter: &mut Emitter, reg: &str, value: usize) {
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("add {}, {}, #{}", reg, reg, value)),
        Arch::X86_64 => emitter.instruction(&format!("add {}, {}", reg, value)),
    }
}

/// Adds one register into another register in the target's operand form.
fn emit_add_reg(emitter: &mut Emitter, dest_reg: &str, rhs_reg: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("add {}, {}, {}", dest_reg, dest_reg, rhs_reg));
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("add {}, {}", dest_reg, rhs_reg));
        }
    }
}

/// Adds a non-zero immediate to a register.
fn emit_add_usize_if_nonzero(emitter: &mut Emitter, reg: &str, value: usize) {
    if value > 0 {
        emit_add_usize(emitter, reg, value);
    }
}

/// Increments a register by one.
fn emit_increment_reg(emitter: &mut Emitter, reg: &str) {
    emit_add_usize(emitter, reg, 1);
}

/// Scales an index register into a byte offset.
fn emit_scale_index_to_offset(
    emitter: &mut Emitter,
    offset_reg: &str,
    index_reg: &str,
    stride: usize,
) {
    match stride {
        0 => abi::emit_load_int_immediate(emitter, offset_reg, 0),
        8 => match emitter.target.arch {
            Arch::AArch64 => emitter.instruction(&format!("lsl {}, {}, #3", offset_reg, index_reg)),
            Arch::X86_64 => {
                emitter.instruction(&format!("mov {}, {}", offset_reg, index_reg));
                emitter.instruction(&format!("shl {}, 3", offset_reg));
            }
        },
        16 => match emitter.target.arch {
            Arch::AArch64 => emitter.instruction(&format!("lsl {}, {}, #4", offset_reg, index_reg)),
            Arch::X86_64 => {
                emitter.instruction(&format!("mov {}, {}", offset_reg, index_reg));
                emitter.instruction(&format!("shl {}, 4", offset_reg));
            }
        },
        _ => match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("mov {}, #{}", offset_reg, stride));
                emitter.instruction(&format!(
                    "mul {}, {}, {}",
                    offset_reg, index_reg, offset_reg
                ));
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("mov {}, {}", offset_reg, index_reg));
                emitter.instruction(&format!("imul {}, {}", offset_reg, stride));
            }
        },
    }
}

/// Returns the byte stride for an array element type.
fn array_element_stride(source_elem_ty: &PhpType) -> usize {
    match source_elem_ty.codegen_repr() {
        PhpType::Str => 16,
        PhpType::Void => 0,
        _ => 8,
    }
}

/// Loads an array element into the canonical result registers.
fn load_array_element_to_result(
    emitter: &mut Emitter,
    source_elem_ty: &PhpType,
    data_base_reg: &str,
    byte_offset: usize,
) {
    match source_elem_ty.codegen_repr() {
        PhpType::Float => {
            abi::emit_load_from_address(
                emitter,
                abi::float_result_reg(emitter),
                data_base_reg,
                byte_offset,
            );
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_load_from_address(emitter, ptr_reg, data_base_reg, byte_offset);
            abi::emit_load_from_address(emitter, len_reg, data_base_reg, byte_offset + 8);
        }
        PhpType::Void => {}
        _ => {
            abi::emit_load_from_address(
                emitter,
                abi::int_result_reg(emitter),
                data_base_reg,
                byte_offset,
            );
        }
    }
}

/// Pushes a loaded indexed-array element as a by-reference callback argument.
fn push_loaded_indexed_array_ref_arg(
    source_elem_ty: &PhpType,
    target_ty: Option<&PhpType>,
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    data: &mut DataSection,
) -> PhpType {
    if !matches!(
        source_elem_ty.codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        return push_current_result_ref_arg_address(source_elem_ty, target_ty, emitter, ctx, data);
    }
    let special_label = ctx.next_label("invoker_ref_cell");
    let temp_label = ctx.next_label("invoker_ref_temp");
    let done_label = ctx.next_label("invoker_ref_done");
    let result_reg = abi::int_result_reg(emitter);
    let tag_reg = abi::secondary_scratch_reg(emitter);

    abi::emit_load_from_address(emitter, tag_reg, result_reg, 0);
    emit_branch_if_invoker_ref_cell_tag(tag_reg, &special_label, emitter);
    abi::emit_jump(emitter, &temp_label);

    emitter.label(&special_label);
    abi::emit_load_from_address(emitter, result_reg, result_reg, 8);
    abi::emit_push_result_value(emitter, &PhpType::Int);
    abi::emit_jump(emitter, &done_label);

    emitter.label(&temp_label);
    push_current_result_ref_arg_address(source_elem_ty, target_ty, emitter, ctx, data);

    emitter.label(&done_label);
    PhpType::Int
}

/// Pushes a loaded indexed-array element as a by-value callback argument.
fn push_loaded_indexed_array_value_arg(
    source_elem_ty: &PhpType,
    target_ty: Option<&PhpType>,
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    data: &mut DataSection,
) -> PhpType {
    if !matches!(
        source_elem_ty.codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        return push_loaded_array_element_arg(source_elem_ty, target_ty, emitter, ctx, data);
    }

    let special_label = ctx.next_label("invoker_ref_value");
    let done_label = ctx.next_label("invoker_value_done");
    let result_reg = abi::int_result_reg(emitter);
    let tag_reg = abi::secondary_scratch_reg(emitter);

    abi::emit_load_from_address(emitter, tag_reg, result_reg, 0);
    emit_branch_if_invoker_ref_cell_tag(tag_reg, &special_label, emitter);
    let ordinary_ty = push_loaded_array_element_arg(source_elem_ty, target_ty, emitter, ctx, data);
    abi::emit_jump(emitter, &done_label);

    emitter.label(&special_label);
    let ref_cell_ty = push_loaded_invoker_ref_cell_value_arg(target_ty, emitter, ctx, data);

    emitter.label(&done_label);
    widen_callback_arg_type(&ordinary_ty, &ref_cell_ty)
}

/// Pushes the value inside an invoker reference-cell marker for a by-value parameter.
fn push_loaded_invoker_ref_cell_value_arg(
    target_ty: Option<&PhpType>,
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    data: &mut DataSection,
) -> PhpType {
    emit_box_loaded_invoker_ref_cell_value_as_mixed(emitter, ctx);
    let release_mixed_after_coerce = target_ty.is_some_and(|target_ty| {
        !matches!(target_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
            && can_coerce_result_to_type(&PhpType::Mixed, target_ty)
    });
    if release_mixed_after_coerce {
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
    }
    let (pushed_ty, _boxed_to_mixed) =
        coerce_current_value_to_target(emitter, ctx, data, &PhpType::Mixed, target_ty);
    if release_mixed_after_coerce {
        release_preserved_mixed_after_arg_coercion(emitter, &pushed_ty);
    }
    abi::emit_push_result_value(emitter, &pushed_ty);
    pushed_ty
}

/// Boxes the value referenced by an invoker marker into an owned Mixed cell.
fn emit_box_loaded_invoker_ref_cell_value_as_mixed(
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
) {
    let result_reg = abi::int_result_reg(emitter);
    let ref_cell_reg = abi::symbol_scratch_reg(emitter);
    let tag_reg = abi::secondary_scratch_reg(emitter);
    let lo_reg = abi::tertiary_scratch_reg(emitter);
    let hi_reg = match emitter.target.arch {
        Arch::AArch64 => "x12",
        Arch::X86_64 => "rdx",
    };
    let string_hi_label = ctx.next_label("invoker_ref_string_hi");
    let box_label = ctx.next_label("invoker_ref_box");

    abi::emit_load_from_address(emitter, ref_cell_reg, result_reg, 8);
    abi::emit_load_from_address(emitter, tag_reg, result_reg, 16);
    abi::emit_load_from_address(emitter, lo_reg, ref_cell_reg, 0);
    abi::emit_load_int_immediate(emitter, hi_reg, 0);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {}, #1", tag_reg));
            emitter.instruction(&format!("b.eq {}", string_hi_label));
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {}, 1", tag_reg));
            emitter.instruction(&format!("je {}", string_hi_label));
        }
    }
    abi::emit_jump(emitter, &box_label);
    emitter.label(&string_hi_label);
    abi::emit_load_from_address(emitter, hi_reg, ref_cell_reg, 8);
    emitter.label(&box_label);
    emit_box_runtime_payload_as_mixed(emitter, tag_reg, lo_reg, hi_reg);
}

/// Branches when a boxed Mixed argument is an invoker ref-cell marker.
fn emit_branch_if_invoker_ref_cell_tag(tag_reg: &str, label: &str, emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {}, #{}", tag_reg, INVOKER_ARG_REF_CELL_TAG));
            emitter.instruction(&format!("b.eq {}", label));
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {}, {}", tag_reg, INVOKER_ARG_REF_CELL_TAG));
            emitter.instruction(&format!("je {}", label));
        }
    }
}

/// Branches when the raw hash lookup value carries the requested runtime tag.
fn emit_branch_if_hash_value_tag(
    raw_tag_reg: &str,
    tag: u8,
    label: &str,
    emitter: &mut Emitter,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {}, #{}", raw_tag_reg, tag));
            emitter.instruction(&format!("b.eq {}", label));
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {}, {}", raw_tag_reg, tag));
            emitter.instruction(&format!("je {}", label));
        }
    }
}

/// Branches when a hash value is a boxed Mixed invoker ref-cell marker.
fn emit_branch_if_boxed_invoker_ref_cell(
    raw_lo_reg: &str,
    raw_tag_reg: &str,
    label: &str,
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
) {
    let not_boxed_label = ctx.next_label("hash_invoker_ref_not_boxed");
    let marker_tag_reg = abi::temp_int_reg(emitter.target);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {}, #7", raw_tag_reg));
            emitter.instruction(&format!("b.ne {}", not_boxed_label));
            abi::emit_load_from_address(emitter, marker_tag_reg, raw_lo_reg, 0);
            emitter.instruction(&format!(
                "cmp {}, #{}",
                marker_tag_reg, INVOKER_ARG_REF_CELL_TAG
            ));
            emitter.instruction(&format!("b.eq {}", label));
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {}, 7", raw_tag_reg));
            emitter.instruction(&format!("jne {}", not_boxed_label));
            abi::emit_load_from_address(emitter, marker_tag_reg, raw_lo_reg, 0);
            emitter.instruction(&format!(
                "cmp {}, {}",
                marker_tag_reg, INVOKER_ARG_REF_CELL_TAG
            ));
            emitter.instruction(&format!("je {}", label));
        }
    }
    emitter.label(&not_boxed_label);
}

/// Extracts ref-cell pointer and source tag from a boxed Mixed invoker marker.
fn load_boxed_invoker_ref_cell_to_raw_regs(
    raw_lo_reg: &str,
    raw_hi_reg: &str,
    emitter: &mut Emitter,
) {
    let marker_reg = abi::temp_int_reg(emitter.target);
    emitter.instruction(&format!("mov {}, {}", marker_reg, raw_lo_reg));
    abi::emit_load_from_address(emitter, raw_lo_reg, marker_reg, 8);
    abi::emit_load_from_address(emitter, raw_hi_reg, marker_reg, 16);
}

/// Coerces and pushes a loaded indexed-array element as a call argument.
fn push_loaded_array_element_arg(
    source_elem_ty: &PhpType,
    target_ty: Option<&PhpType>,
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    data: &mut DataSection,
) -> PhpType {
    let (pushed_ty, boxed_to_mixed) =
        coerce_current_value_to_target(emitter, ctx, data, source_elem_ty, target_ty);
    if !boxed_to_mixed {
        abi::emit_incref_if_refcounted(emitter, &pushed_ty);
    }
    abi::emit_push_result_value(emitter, &pushed_ty);
    pushed_ty
}

/// Allocates a stable by-reference heap cell for the current result and pushes its address.
fn push_current_result_ref_arg_address(
    source_ty: &PhpType,
    target_ty: Option<&PhpType>,
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    data: &mut DataSection,
) -> PhpType {
    let source_repr = source_ty.codegen_repr();
    let (pushed_ty, boxed_to_mixed) =
        coerce_current_value_to_target(emitter, ctx, data, source_ty, target_ty);
    if !boxed_to_mixed {
        abi::emit_incref_if_refcounted(emitter, &source_repr);
    }
    abi::emit_push_result_value(emitter, &pushed_ty);
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 16);
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    let cell_reg = abi::symbol_scratch_reg(emitter);
    emitter.instruction(&format!(
        "mov {}, {}",
        cell_reg,
        abi::int_result_reg(emitter)
    ));
    store_pushed_value_to_ref_cell(emitter, cell_reg, &pushed_ty);
    abi::emit_push_reg(emitter, cell_reg);
    PhpType::Int
}

/// Wraps the value currently on top of the invoker stack in a heap reference cell.
fn wrap_pushed_value_in_ref_cell(emitter: &mut Emitter, val_ty: &PhpType) {
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 16);
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    let cell_reg = abi::symbol_scratch_reg(emitter);
    emitter.instruction(&format!(
        "mov {}, {}",
        cell_reg,
        abi::int_result_reg(emitter)
    ));
    store_pushed_value_to_ref_cell(emitter, cell_reg, val_ty);
    abi::emit_push_reg(emitter, cell_reg);
}

/// Stores a just-pushed value into a heap reference cell.
fn store_pushed_value_to_ref_cell(emitter: &mut Emitter, cell_reg: &str, val_ty: &PhpType) {
    let temp_reg = abi::temp_int_reg(emitter.target);
    match val_ty.codegen_repr() {
        PhpType::Bool
        | PhpType::False
        | PhpType::Int
        | PhpType::Callable
        | PhpType::Pointer(_)
        | PhpType::Buffer(_)
        | PhpType::Packed(_) => {
            abi::emit_pop_reg(emitter, temp_reg);
            abi::emit_store_to_address(emitter, temp_reg, cell_reg, 0);
            abi::emit_store_zero_to_address(emitter, cell_reg, 8);
        }
        PhpType::Float => {
            abi::emit_pop_float_reg(emitter, abi::float_result_reg(emitter));
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("fmov x10, d0");
                    abi::emit_store_to_address(emitter, "x10", cell_reg, 0);
                }
                Arch::X86_64 => {
                    emitter.instruction("movq r10, xmm0");
                    abi::emit_store_to_address(emitter, "r10", cell_reg, 0);
                }
            }
            abi::emit_load_int_immediate(emitter, temp_reg, 2);
            abi::emit_store_to_address(emitter, temp_reg, cell_reg, 8);
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_pop_reg_pair(emitter, ptr_reg, len_reg);
            abi::emit_store_to_address(emitter, ptr_reg, cell_reg, 0);
            abi::emit_store_to_address(emitter, len_reg, cell_reg, 8);
        }
        PhpType::Mixed | PhpType::Union(_) | PhpType::Iterable => {
            abi::emit_pop_reg(emitter, temp_reg);
            abi::emit_store_to_address(emitter, temp_reg, cell_reg, 0);
            abi::emit_load_int_immediate(emitter, temp_reg, 7);
            abi::emit_store_to_address(emitter, temp_reg, cell_reg, 8);
        }
        PhpType::Array(_) => {
            abi::emit_pop_reg(emitter, temp_reg);
            abi::emit_store_to_address(emitter, temp_reg, cell_reg, 0);
            abi::emit_load_int_immediate(emitter, temp_reg, 4);
            abi::emit_store_to_address(emitter, temp_reg, cell_reg, 8);
        }
        PhpType::AssocArray { .. } => {
            abi::emit_pop_reg(emitter, temp_reg);
            abi::emit_store_to_address(emitter, temp_reg, cell_reg, 0);
            abi::emit_load_int_immediate(emitter, temp_reg, 5);
            abi::emit_store_to_address(emitter, temp_reg, cell_reg, 8);
        }
        PhpType::Object(_) => {
            abi::emit_pop_reg(emitter, temp_reg);
            abi::emit_store_to_address(emitter, temp_reg, cell_reg, 0);
            abi::emit_load_int_immediate(emitter, temp_reg, 6);
            abi::emit_store_to_address(emitter, temp_reg, cell_reg, 8);
        }
        PhpType::Resource(_) => {
            abi::emit_pop_reg(emitter, temp_reg);
            abi::emit_store_to_address(emitter, temp_reg, cell_reg, 0);
            abi::emit_load_int_immediate(emitter, temp_reg, 9);
            abi::emit_store_to_address(emitter, temp_reg, cell_reg, 8);
        }
        PhpType::TaggedScalar | PhpType::Void | PhpType::Never => {
            abi::emit_pop_reg(emitter, temp_reg);
            abi::emit_store_to_address(emitter, temp_reg, cell_reg, 0);
            abi::emit_store_zero_to_address(emitter, cell_reg, 8);
        }
    }
}

/// Looks up a named or numeric associative argument.
fn emit_hash_lookup_for_param_or_index(
    hash_base_reg: &str,
    param_name: Option<&str>,
    numeric_idx: usize,
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    data: &mut DataSection,
) {
    let key_ptr_reg = abi::int_arg_reg_name(emitter.target, 1);
    let key_len_reg = abi::int_arg_reg_name(emitter.target, 2);
    let found_label = param_name.map(|_| ctx.next_label("invoker_assoc_key_found"));

    if let Some(name) = param_name {
        let (key_label, key_len) = data.add_string(name.as_bytes());
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("mov x0, {}", hash_base_reg));
                abi::emit_symbol_address(emitter, key_ptr_reg, &key_label);
                abi::emit_load_int_immediate(emitter, key_len_reg, key_len as i64);
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("mov rdi, {}", hash_base_reg));
                abi::emit_symbol_address(emitter, key_ptr_reg, &key_label);
                abi::emit_load_int_immediate(emitter, key_len_reg, key_len as i64);
            }
        }
        abi::emit_call_label(emitter, "__rt_hash_get");
        if let Some(found_label) = &found_label {
            abi::emit_branch_if_int_result_nonzero(emitter, found_label);
        }
    }

    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("mov x0, {}", hash_base_reg)),
        Arch::X86_64 => emitter.instruction(&format!("mov rdi, {}", hash_base_reg)),
    }
    abi::emit_load_int_immediate(emitter, key_ptr_reg, numeric_idx as i64);
    abi::emit_load_int_immediate(emitter, key_len_reg, -1);
    abi::emit_call_label(emitter, "__rt_hash_get");
    if let Some(found_label) = found_label {
        emitter.label(&found_label);
    }
}

/// Pushes a loaded hash value as a by-value argument.
fn push_loaded_hash_value_arg(
    source_elem_ty: &PhpType,
    target_ty: Option<&PhpType>,
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    data: &mut DataSection,
) -> PhpType {
    if matches!(
        source_elem_ty.codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        return push_loaded_mixed_hash_value_arg(target_ty, emitter, ctx, data);
    }
    materialize_hash_value_to_result(emitter, source_elem_ty);
    push_loaded_array_element_arg(source_elem_ty, target_ty, emitter, ctx, data)
}

/// Pushes a loaded hash value as a by-reference argument.
fn push_loaded_hash_value_ref_arg(
    source_elem_ty: &PhpType,
    target_ty: Option<&PhpType>,
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    data: &mut DataSection,
) -> PhpType {
    if matches!(
        source_elem_ty.codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        return push_loaded_mixed_hash_value_ref_arg(target_ty, emitter, ctx, data);
    }
    materialize_hash_value_to_result(emitter, source_elem_ty);
    push_current_result_ref_arg_address(source_elem_ty, target_ty, emitter, ctx, data)
}

/// Pushes a Mixed hash value as a by-value argument, honoring invoker ref markers.
fn push_loaded_mixed_hash_value_arg(
    target_ty: Option<&PhpType>,
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    data: &mut DataSection,
) -> PhpType {
    let direct_marker_label = ctx.next_label("hash_invoker_ref_value_direct");
    let boxed_marker_label = ctx.next_label("hash_invoker_ref_value_boxed");
    let ordinary_label = ctx.next_label("hash_invoker_ref_value_ordinary");
    let ordinary_boxed_label = ctx.next_label("hash_invoker_ref_value_ordinary_boxed");
    let ordinary_raw_label = ctx.next_label("hash_invoker_ref_value_ordinary_raw");
    let done_label = ctx.next_label("hash_invoker_ref_value_done");
    let (raw_lo_reg, raw_hi_reg, raw_tag_reg) = raw_hash_value_regs(emitter);

    emit_branch_if_invoker_ref_cell_tag(raw_tag_reg, &direct_marker_label, emitter);
    emit_branch_if_boxed_invoker_ref_cell(
        raw_lo_reg,
        raw_tag_reg,
        &boxed_marker_label,
        emitter,
        ctx,
    );
    abi::emit_jump(emitter, &ordinary_label);

    emitter.label(&ordinary_label);
    emit_branch_if_hash_value_tag(raw_tag_reg, 7, &ordinary_boxed_label, emitter);
    abi::emit_jump(emitter, &ordinary_raw_label);

    emitter.label(&ordinary_boxed_label);
    materialize_hash_value_to_result(emitter, &PhpType::Mixed);
    let ordinary_boxed_ty =
        push_materialized_mixed_hash_value_arg(target_ty, false, emitter, ctx, data);
    abi::emit_jump(emitter, &done_label);

    emitter.label(&ordinary_raw_label);
    box_raw_hash_value_to_mixed_result(emitter);
    let ordinary_raw_ty =
        push_materialized_mixed_hash_value_arg(target_ty, true, emitter, ctx, data);
    abi::emit_jump(emitter, &done_label);

    emitter.label(&direct_marker_label);
    let direct_ty =
        push_raw_invoker_ref_cell_value_arg(raw_lo_reg, raw_hi_reg, target_ty, emitter, ctx, data);
    abi::emit_jump(emitter, &done_label);

    emitter.label(&boxed_marker_label);
    load_boxed_invoker_ref_cell_to_raw_regs(raw_lo_reg, raw_hi_reg, emitter);
    let boxed_ty =
        push_raw_invoker_ref_cell_value_arg(raw_lo_reg, raw_hi_reg, target_ty, emitter, ctx, data);

    emitter.label(&done_label);
    let ordinary_ty = widen_callback_arg_type(&ordinary_boxed_ty, &ordinary_raw_ty);
    widen_callback_arg_type(&widen_callback_arg_type(&ordinary_ty, &direct_ty), &boxed_ty)
}

/// Pushes a Mixed hash value as a by-reference argument, honoring invoker ref markers.
fn push_loaded_mixed_hash_value_ref_arg(
    target_ty: Option<&PhpType>,
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    data: &mut DataSection,
) -> PhpType {
    let direct_marker_label = ctx.next_label("hash_invoker_ref_direct");
    let boxed_marker_label = ctx.next_label("hash_invoker_ref_boxed");
    let ordinary_label = ctx.next_label("hash_invoker_ref_ordinary");
    let ordinary_boxed_label = ctx.next_label("hash_invoker_ref_ordinary_boxed");
    let ordinary_raw_label = ctx.next_label("hash_invoker_ref_ordinary_raw");
    let done_label = ctx.next_label("hash_invoker_ref_done");
    let (raw_lo_reg, raw_hi_reg, raw_tag_reg) = raw_hash_value_regs(emitter);

    emit_branch_if_invoker_ref_cell_tag(raw_tag_reg, &direct_marker_label, emitter);
    emit_branch_if_boxed_invoker_ref_cell(
        raw_lo_reg,
        raw_tag_reg,
        &boxed_marker_label,
        emitter,
        ctx,
    );
    abi::emit_jump(emitter, &ordinary_label);

    emitter.label(&direct_marker_label);
    move_raw_hash_value_lo_to_result(emitter);
    abi::emit_push_result_value(emitter, &PhpType::Int);
    abi::emit_jump(emitter, &done_label);

    emitter.label(&boxed_marker_label);
    load_boxed_invoker_ref_cell_to_raw_regs(raw_lo_reg, raw_hi_reg, emitter);
    move_raw_hash_value_lo_to_result(emitter);
    abi::emit_push_result_value(emitter, &PhpType::Int);
    abi::emit_jump(emitter, &done_label);

    emitter.label(&ordinary_label);
    emit_branch_if_hash_value_tag(raw_tag_reg, 7, &ordinary_boxed_label, emitter);
    abi::emit_jump(emitter, &ordinary_raw_label);

    emitter.label(&ordinary_boxed_label);
    materialize_hash_value_to_result(emitter, &PhpType::Mixed);
    push_current_result_ref_arg_address(&PhpType::Mixed, target_ty, emitter, ctx, data);
    abi::emit_jump(emitter, &done_label);

    emitter.label(&ordinary_raw_label);
    box_raw_hash_value_to_mixed_result(emitter);
    push_current_result_ref_arg_address(&PhpType::Mixed, target_ty, emitter, ctx, data);

    emitter.label(&done_label);
    PhpType::Int
}

/// Coerces and pushes a materialized boxed Mixed hash value.
fn push_materialized_mixed_hash_value_arg(
    target_ty: Option<&PhpType>,
    release_source_mixed_after_coerce: bool,
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    data: &mut DataSection,
) -> PhpType {
    let release_mixed_after_coerce = release_source_mixed_after_coerce
        && target_ty.is_some_and(|target_ty| {
            !matches!(target_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
                && can_coerce_result_to_type(&PhpType::Mixed, target_ty)
        });
    if release_mixed_after_coerce {
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
    }
    let (pushed_ty, _boxed_to_mixed) =
        coerce_current_value_to_target(emitter, ctx, data, &PhpType::Mixed, target_ty);
    if release_mixed_after_coerce {
        release_preserved_mixed_after_arg_coercion(emitter, &pushed_ty);
    }
    abi::emit_push_result_value(emitter, &pushed_ty);
    pushed_ty
}

/// Pushes the value referenced by a raw invoker ref-cell marker.
fn push_raw_invoker_ref_cell_value_arg(
    ref_cell_reg: &str,
    source_tag_reg: &str,
    target_ty: Option<&PhpType>,
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    data: &mut DataSection,
) -> PhpType {
    emit_box_raw_invoker_ref_cell_value_as_mixed(ref_cell_reg, source_tag_reg, emitter, ctx);
    push_materialized_mixed_hash_value_arg(target_ty, true, emitter, ctx, data)
}

/// Boxes a raw reference-cell value as Mixed.
fn emit_box_raw_invoker_ref_cell_value_as_mixed(
    ref_cell_reg: &str,
    source_tag_reg: &str,
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
) {
    let lo_reg = abi::tertiary_scratch_reg(emitter);
    let hi_reg = match emitter.target.arch {
        Arch::AArch64 => "x12",
        Arch::X86_64 => "rdx",
    };
    let string_hi_label = ctx.next_label("hash_invoker_ref_string_hi");
    let box_label = ctx.next_label("hash_invoker_ref_box");
    abi::emit_load_from_address(emitter, lo_reg, ref_cell_reg, 0);
    abi::emit_load_int_immediate(emitter, hi_reg, 0);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {}, #1", source_tag_reg));
            emitter.instruction(&format!("b.eq {}", string_hi_label));
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {}, 1", source_tag_reg));
            emitter.instruction(&format!("je {}", string_hi_label));
        }
    }
    abi::emit_jump(emitter, &box_label);
    emitter.label(&string_hi_label);
    abi::emit_load_from_address(emitter, hi_reg, ref_cell_reg, 8);
    emitter.label(&box_label);
    emit_box_runtime_payload_as_mixed(emitter, source_tag_reg, lo_reg, hi_reg);
}

/// Returns raw registers produced by `__rt_hash_get`.
fn raw_hash_value_regs(emitter: &Emitter) -> (&'static str, &'static str, &'static str) {
    match emitter.target.arch {
        Arch::AArch64 => ("x1", "x2", "x3"),
        Arch::X86_64 => ("rdi", "rsi", "rcx"),
    }
}

/// Moves the raw hash low payload into the standard integer result register.
fn move_raw_hash_value_lo_to_result(emitter: &mut Emitter) {
    let (raw_lo_reg, _, _) = raw_hash_value_regs(emitter);
    let result_reg = abi::int_result_reg(emitter);
    emitter.instruction(&format!("mov {}, {}", result_reg, raw_lo_reg));
}

/// Materializes the current hash lookup value into canonical result registers.
fn materialize_hash_value_to_result(emitter: &mut Emitter, source_elem_ty: &PhpType) {
    let (raw_lo_reg, raw_hi_reg, _) = raw_hash_value_regs(emitter);
    match source_elem_ty.codegen_repr() {
        PhpType::Float => match emitter.target.arch {
            Arch::AArch64 => emitter.instruction(&format!(
                "fmov {}, {}",
                abi::float_result_reg(emitter),
                raw_lo_reg
            )),
            Arch::X86_64 => emitter.instruction(&format!(
                "movq {}, {}",
                abi::float_result_reg(emitter),
                raw_lo_reg
            )),
        },
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            emitter.instruction(&format!("mov {}, {}", ptr_reg, raw_lo_reg));
            emitter.instruction(&format!("mov {}, {}", len_reg, raw_hi_reg));
        }
        PhpType::Void => {}
        _ => move_raw_hash_value_lo_to_result(emitter),
    }
}

/// Boxes the current raw hash lookup payload into a canonical Mixed result.
fn box_raw_hash_value_to_mixed_result(emitter: &mut Emitter) {
    let (raw_lo_reg, raw_hi_reg, raw_tag_reg) = raw_hash_value_regs(emitter);
    emit_box_runtime_payload_as_mixed(emitter, raw_tag_reg, raw_lo_reg, raw_hi_reg);
}

/// Emits and pushes a default value argument.
fn push_default_value_arg(
    default: &Expr,
    target_ty: Option<&PhpType>,
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    data: &mut DataSection,
) -> PhpType {
    let source_ty = emit_default_to_result(default, target_ty, emitter, ctx, data);
    let (pushed_ty, boxed_to_mixed) =
        coerce_current_value_to_target(emitter, ctx, data, &source_ty, target_ty);
    if !boxed_to_mixed {
        abi::emit_incref_if_refcounted(emitter, &pushed_ty);
    }
    abi::emit_push_result_value(emitter, &pushed_ty);
    pushed_ty
}

/// Emits and pushes a default value by-reference cell.
fn push_default_ref_arg(
    default: &Expr,
    target_ty: Option<&PhpType>,
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    data: &mut DataSection,
) -> PhpType {
    let pushed_ty = push_default_value_arg(default, target_ty, emitter, ctx, data);
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 16);
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    let cell_reg = abi::symbol_scratch_reg(emitter);
    emitter.instruction(&format!(
        "mov {}, {}",
        cell_reg,
        abi::int_result_reg(emitter)
    ));
    store_pushed_value_to_ref_cell(emitter, cell_reg, &pushed_ty);
    abi::emit_push_reg(emitter, cell_reg);
    PhpType::Int
}

/// Emits a supported default expression into result registers.
fn emit_default_to_result(
    default: &Expr,
    target_ty: Option<&PhpType>,
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    data: &mut DataSection,
) -> PhpType {
    match &default.kind {
        ExprKind::IntLiteral(value) => {
            abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), *value);
            PhpType::Int
        }
        ExprKind::Negate(inner) => match &inner.kind {
            ExprKind::IntLiteral(value) => {
                abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), -*value);
                PhpType::Int
            }
            ExprKind::FloatLiteral(value) => {
                emit_float_literal_to_result(emitter, data, -*value);
                PhpType::Float
            }
            _ => emit_null_default_to_result(emitter, target_ty),
        },
        ExprKind::BoolLiteral(value) => {
            abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), i64::from(*value));
            PhpType::Bool
        }
        ExprKind::FloatLiteral(value) => {
            emit_float_literal_to_result(emitter, data, *value);
            PhpType::Float
        }
        ExprKind::StringLiteral(value) => {
            let (label, len) = data.add_string(value.as_bytes());
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_symbol_address(emitter, ptr_reg, &label);
            abi::emit_load_int_immediate(emitter, len_reg, len as i64);
            PhpType::Str
        }
        ExprKind::ArrayLiteral(items) if items.is_empty() => {
            let elem_ty = target_ty
                .and_then(|ty| match ty.codegen_repr() {
                    PhpType::Array(elem) => Some(*elem),
                    _ => None,
                })
                .unwrap_or(PhpType::Mixed);
            emit_empty_indexed_array(emitter, &elem_ty);
            PhpType::Array(Box::new(elem_ty))
        }
        ExprKind::Null => emit_null_default_to_result(emitter, target_ty),
        _ => {
            emit_unsupported_default_abort(emitter, data, ctx);
            PhpType::Void
        }
    }
}

/// Emits a float literal into the float result register.
fn emit_float_literal_to_result(emitter: &mut Emitter, data: &mut DataSection, value: f64) {
    let label = data.add_float(value);
    let scratch = abi::symbol_scratch_reg(emitter);
    let float_reg = abi::float_result_reg(emitter);
    abi::emit_symbol_address(emitter, scratch, &label);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr {}, [{}]", float_reg, scratch));
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("movsd {}, QWORD PTR [{}]", float_reg, scratch));
        }
    }
}

/// Emits a null default into result registers for the target storage shape.
fn emit_null_default_to_result(emitter: &mut Emitter, target_ty: Option<&PhpType>) -> PhpType {
    if target_ty.is_some_and(|ty| matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))) {
        let tag_reg = abi::int_result_reg(emitter);
        let lo_reg = abi::secondary_scratch_reg(emitter);
        let hi_reg = abi::tertiary_scratch_reg(emitter);
        abi::emit_load_int_immediate(
            emitter,
            tag_reg,
            crate::codegen::runtime_value_tag(&PhpType::Void) as i64,
        );
        abi::emit_load_int_immediate(emitter, lo_reg, 0);
        abi::emit_load_int_immediate(emitter, hi_reg, 0);
        emit_box_runtime_payload_as_mixed(emitter, tag_reg, lo_reg, hi_reg);
        return PhpType::Mixed;
    }
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
    PhpType::Void
}

/// Emits an empty indexed array of `elem_ty` into the integer result register.
fn emit_empty_indexed_array(emitter: &mut Emitter, elem_ty: &PhpType) {
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 0), 4);
    abi::emit_load_int_immediate(
        emitter,
        abi::int_arg_reg_name(emitter.target, 1),
        elem_ty.stack_size() as i64,
    );
    abi::emit_call_label(emitter, "__rt_array_new");
    crate::codegen::emit_array_value_type_stamp(emitter, abi::int_result_reg(emitter), elem_ty);
}

/// Emits a fatal diagnostic for unsupported runtime default expressions.
fn emit_unsupported_default_abort(
    emitter: &mut Emitter,
    data: &mut DataSection,
    ctx: &mut InvokerEmitContext,
) {
    let label = ctx.next_label("invoker_unsupported_default");
    let (message_label, message_len) = data.add_string(
        b"Fatal error: callable descriptor invoker cannot materialize this default value\n",
    );
    emitter.label(&label);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #2");
            abi::emit_symbol_address(emitter, "x1", &message_label);
            emitter.instruction(&format!("mov x2, #{}", message_len));
            emitter.syscall(4);
            abi::emit_exit(emitter, 1);
        }
        Arch::X86_64 => {
            emitter.instruction("mov edi, 2");
            abi::emit_symbol_address(emitter, "rsi", &message_label);
            emitter.instruction(&format!("mov edx, {}", message_len));
            emitter.instruction("mov eax, 1");
            emitter.instruction("syscall");
            abi::emit_exit(emitter, 1);
        }
    }
}

/// Coerces the current result to a target argument type.
fn coerce_current_value_to_target(
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    data: &mut DataSection,
    source_ty: &PhpType,
    target_ty: Option<&PhpType>,
) -> (PhpType, bool) {
    let source_repr = source_ty.codegen_repr();
    let pushed_ty = target_ty
        .filter(|target_ty| can_coerce_result_to_type(source_ty, target_ty))
        .map(PhpType::codegen_repr)
        .or_else(|| matches!(source_repr, PhpType::Void).then_some(PhpType::Int))
        .unwrap_or_else(|| source_repr.clone());
    let boxed_to_mixed =
        matches!(pushed_ty, PhpType::Mixed) && !matches!(source_repr, PhpType::Mixed);

    if source_repr != pushed_ty {
        let coerce_source_ty = if matches!(pushed_ty, PhpType::Mixed) {
            source_ty
        } else {
            &source_repr
        };
        coerce_result_to_type(emitter, ctx, data, coerce_source_ty, &pushed_ty);
    }

    (pushed_ty, boxed_to_mixed)
}

/// Emits runtime coercion from one result type to another.
fn coerce_result_to_type(
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    data: &mut DataSection,
    source_ty: &PhpType,
    target_ty: &PhpType,
) {
    if source_ty == target_ty {
        return;
    }
    if matches!(source_ty, PhpType::Mixed | PhpType::Union(_)) {
        match target_ty.codegen_repr() {
            PhpType::Int | PhpType::Resource(_) | PhpType::Pointer(_) => {
                abi::emit_call_label(emitter, "__rt_mixed_cast_int");
            }
            PhpType::Bool => {
                abi::emit_call_label(emitter, "__rt_mixed_cast_bool");
            }
            PhpType::Float => {
                abi::emit_call_label(emitter, "__rt_mixed_cast_float");
            }
            PhpType::Str => {
                abi::emit_call_label(emitter, "__rt_mixed_cast_string");
            }
            PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) => {
                abi::emit_call_label(emitter, "__rt_mixed_unbox");
                match emitter.target.arch {
                    Arch::AArch64 => emitter.instruction("mov x0, x1"),
                    Arch::X86_64 => emitter.instruction("mov rax, rdi"),
                }
            }
            PhpType::Mixed | PhpType::Union(_) => {}
            _ => {}
        }
    } else if matches!(target_ty, PhpType::Mixed | PhpType::Union(_)) {
        emit_box_current_value_as_mixed(emitter, source_ty);
    } else if *target_ty == PhpType::Str {
        coerce_to_string(emitter, ctx, data, source_ty);
    } else if *target_ty == PhpType::Float
        && matches!(source_ty, PhpType::Int | PhpType::Bool | PhpType::Void)
    {
        if *source_ty == PhpType::Void {
            abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
        }
        abi::emit_int_result_to_float_result(emitter);
    } else if *target_ty == PhpType::Int && *source_ty == PhpType::Float {
        abi::emit_float_result_to_int_result(emitter);
    }
}

/// Returns true if the invoker can coerce this source/target pair.
fn can_coerce_result_to_type(source_ty: &PhpType, target_ty: &PhpType) -> bool {
    if source_ty == target_ty {
        return true;
    }
    if matches!(source_ty, PhpType::Mixed | PhpType::Union(_)) {
        return matches!(
            target_ty.codegen_repr(),
            PhpType::Int
                | PhpType::Resource(_)
                | PhpType::Pointer(_)
                | PhpType::Bool
                | PhpType::Float
                | PhpType::Str
                | PhpType::Array(_)
                | PhpType::AssocArray { .. }
                | PhpType::Object(_)
                | PhpType::Mixed
                | PhpType::Union(_)
        );
    }
    matches!(target_ty, PhpType::Mixed | PhpType::Union(_))
        || *target_ty == PhpType::Str
        || (*target_ty == PhpType::Float
            && matches!(source_ty, PhpType::Int | PhpType::Bool | PhpType::Void))
        || (*target_ty == PhpType::Int && *source_ty == PhpType::Float)
}

/// Coerces the current result to PHP string result registers.
fn coerce_to_string(
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    data: &mut DataSection,
    ty: &PhpType,
) {
    match ty.codegen_repr() {
        PhpType::Int => abi::emit_call_label(emitter, "__rt_itoa"),
        PhpType::Float => abi::emit_call_label(emitter, "__rt_ftoa"),
        PhpType::Bool => emit_bool_to_string(emitter, ctx),
        PhpType::Void | PhpType::Never => emit_empty_string_result(emitter),
        PhpType::Mixed | PhpType::Union(_) => {
            abi::emit_call_label(emitter, "__rt_mixed_cast_string")
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable => {
            let (label, len) = data.add_string(b"Array");
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_symbol_address(emitter, ptr_reg, &label);
            abi::emit_load_int_immediate(emitter, len_reg, len as i64);
        }
        _ => {}
    }
}

/// Converts a bool result to PHP string result registers.
fn emit_bool_to_string(emitter: &mut Emitter, ctx: &mut InvokerEmitContext) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cbz x0, 1f");
            abi::emit_call_label(emitter, "__rt_itoa");
            emitter.instruction("b 2f");
            emitter.raw("1:");
            emitter.instruction("mov x2, #0");
            emitter.raw("2:");
        }
        Arch::X86_64 => {
            let false_label = ctx.next_label("bool_to_str_false");
            let done_label = ctx.next_label("bool_to_str_done");
            emitter.instruction("test rax, rax");
            emitter.instruction(&format!("je {}", false_label));
            abi::emit_call_label(emitter, "__rt_itoa");
            emitter.instruction(&format!("jmp {}", done_label));
            emitter.label(&false_label);
            emitter.instruction("mov rdx, 0");
            emitter.label(&done_label);
        }
    }
}

/// Materializes an empty PHP string result.
fn emit_empty_string_result(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("mov x2, #0"),
        Arch::X86_64 => emitter.instruction("mov rdx, 0"),
    }
}

/// Releases a preserved Mixed value after coercion.
fn release_preserved_mixed_after_arg_coercion(emitter: &mut Emitter, pushed_ty: &PhpType) {
    abi::emit_push_result_value(emitter, pushed_ty);
    abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), 16);
    abi::emit_decref_if_refcounted(emitter, &PhpType::Mixed);
    restore_pushed_value_after_release(emitter, pushed_ty);
    abi::emit_release_temporary_stack(emitter, 16);
}

/// Restores a pushed result value after releasing another value.
fn restore_pushed_value_after_release(emitter: &mut Emitter, pushed_ty: &PhpType) {
    match pushed_ty.codegen_repr() {
        PhpType::Float => abi::emit_pop_float_reg(emitter, abi::float_result_reg(emitter)),
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_pop_reg_pair(emitter, ptr_reg, len_reg);
        }
        PhpType::Void | PhpType::Never => {}
        _ => abi::emit_pop_reg(emitter, abi::int_result_reg(emitter)),
    }
}

/// Pushes captures loaded from the runtime descriptor as hidden arguments.
fn push_descriptor_captures_as_hidden_args(
    captures: &[(String, PhpType, bool)],
    emitter: &mut Emitter,
    arg_types: &mut Vec<PhpType>,
) {
    let descriptor_reg = abi::symbol_scratch_reg(emitter);
    for (idx, (_capture_name, capture_ty, by_ref)) in captures.iter().enumerate() {
        abi::load_at_offset(emitter, descriptor_reg, INVOKER_DESCRIPTOR_OFFSET);
        if *by_ref {
            callable_descriptor::emit_load_runtime_capture_to_result(
                emitter,
                descriptor_reg,
                idx,
                &PhpType::Int,
            );
            abi::emit_push_result_value(emitter, &PhpType::Int);
            arg_types.push(PhpType::Int);
        } else {
            callable_descriptor::emit_load_runtime_capture_to_result(
                emitter,
                descriptor_reg,
                idx,
                capture_ty,
            );
            abi::emit_push_result_value(emitter, capture_ty);
            arg_types.push(capture_ty.clone());
        }
    }
}

/// Calls the target register with already-pushed ABI arguments.
fn call_target_with_pushed_args(
    call_reg: &str,
    arg_types: &[PhpType],
    sig: &FunctionSig,
    emitter: &mut Emitter,
) {
    let assignments = abi::build_outgoing_arg_assignments_for_target(emitter.target, arg_types, 0);
    let overflow_bytes = abi::materialize_outgoing_args(emitter, &assignments);
    save_concat_offset_before_nested_call(emitter);
    abi::emit_call_reg(emitter, call_reg);
    restore_concat_offset_after_nested_call(emitter, &sig.return_type);
    abi::emit_release_temporary_stack(emitter, overflow_bytes);
}

/// Saves the current concat offset before the nested callable target runs.
fn save_concat_offset_before_nested_call(emitter: &mut Emitter) {
    let scratch = abi::temp_int_reg(emitter.target);
    abi::emit_load_symbol_to_reg(emitter, scratch, "_concat_off", 0);
    match emitter.target.arch {
        Arch::AArch64 => abi::emit_push_reg(emitter, scratch),
        Arch::X86_64 => abi::store_at_offset(emitter, scratch, INVOKER_CONCAT_OFFSET),
    }
}

/// Restores the concat offset after a nested callable target returns.
fn restore_concat_offset_after_nested_call(emitter: &mut Emitter, return_ty: &PhpType) {
    if return_ty.codegen_repr() == PhpType::Str {
        abi::emit_call_label(emitter, "__rt_str_persist");
    }
    let scratch = abi::temp_int_reg(emitter.target);
    match emitter.target.arch {
        Arch::AArch64 => abi::emit_pop_reg(emitter, scratch),
        Arch::X86_64 => abi::load_at_offset(emitter, scratch, INVOKER_CONCAT_OFFSET),
    }
    abi::emit_store_reg_to_symbol(emitter, scratch, "_concat_off", 0);
}

/// Emits an associative variadic array argument from remaining hash entries.
#[allow(clippy::too_many_arguments)]
fn emit_loaded_assoc_variadic_array_arg(
    source_hash_reg: &str,
    elem_ty: &PhpType,
    sig: &FunctionSig,
    skip_numeric_before: usize,
    skip_param_names_before: usize,
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    data: &mut DataSection,
) -> PhpType {
    let visible_param_count = sig.params.len();
    let variadic_elem_ty = sig
        .params
        .get(visible_param_count.saturating_sub(1))
        .and_then(|(_, ty)| match ty {
            PhpType::Array(elem) => Some((**elem).clone()),
            PhpType::Iterable => Some(PhpType::Mixed),
            _ => None,
        })
        .unwrap_or_else(|| elem_ty.clone());
    let variadic_ty = PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(variadic_elem_ty.clone()),
    };
    let capacity_reg = abi::int_arg_reg_name(emitter.target, 0);
    let tag_reg = abi::int_arg_reg_name(emitter.target, 1);

    abi::emit_load_int_immediate(emitter, capacity_reg, 16);
    abi::emit_load_int_immediate(
        emitter,
        tag_reg,
        crate::codegen::runtime_value_tag(&variadic_elem_ty.codegen_repr()) as i64,
    );
    abi::emit_call_label(emitter, "__rt_hash_new");
    abi::emit_push_result_value(emitter, &variadic_ty);
    emit_loaded_assoc_variadic_entries(
        source_hash_reg,
        sig,
        skip_numeric_before,
        skip_param_names_before,
        emitter,
        ctx,
        data,
    );
    variadic_ty
}

/// Copies unconsumed associative source entries into the variadic hash.
fn emit_loaded_assoc_variadic_entries(
    source_hash_reg: &str,
    sig: &FunctionSig,
    skip_numeric_before: usize,
    skip_param_names_before: usize,
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    data: &mut DataSection,
) {
    const SCRATCH_BYTES: usize = 96;
    const CURSOR_OFF: usize = 0;
    const SOURCE_HASH_OFF: usize = 8;
    const KEY_PTR_OFF: usize = 16;
    const KEY_LEN_OFF: usize = 24;
    const VALUE_LO_OFF: usize = 32;
    const VALUE_HI_OFF: usize = 40;
    const VALUE_TAG_OFF: usize = 48;
    const NUMERIC_KEY_OFF: usize = 56;

    let loop_label = ctx.next_label("assoc_variadic_loop");
    let done_label = ctx.next_label("assoc_variadic_done");
    let skip_label = ctx.next_label("assoc_variadic_skip");
    let numeric_key_label = ctx.next_label("assoc_variadic_numeric_key");
    let string_key_label = ctx.next_label("assoc_variadic_string_key");
    let insert_label = ctx.next_label("assoc_variadic_insert");

    abi::emit_reserve_temporary_stack(emitter, SCRATCH_BYTES);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_store_to_address(emitter, source_hash_reg, "sp", SOURCE_HASH_OFF);
            abi::emit_store_zero_to_address(emitter, "sp", CURSOR_OFF);
            abi::emit_store_zero_to_address(emitter, "sp", NUMERIC_KEY_OFF);
        }
        Arch::X86_64 => {
            abi::emit_store_to_address(emitter, source_hash_reg, "rsp", SOURCE_HASH_OFF);
            abi::emit_store_zero_to_address(emitter, "rsp", CURSOR_OFF);
            abi::emit_store_zero_to_address(emitter, "rsp", NUMERIC_KEY_OFF);
        }
    }

    emitter.label(&loop_label);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(emitter, "x0", SOURCE_HASH_OFF);
            abi::emit_load_temporary_stack_slot(emitter, "x1", CURSOR_OFF);
            abi::emit_call_label(emitter, "__rt_hash_iter_next");
            emitter.instruction("cmn x0, #1");
            emitter.instruction(&format!("b.eq {}", done_label));
            abi::emit_store_to_address(emitter, "x0", "sp", CURSOR_OFF);
            abi::emit_store_to_address(emitter, "x1", "sp", KEY_PTR_OFF);
            abi::emit_store_to_address(emitter, "x2", "sp", KEY_LEN_OFF);
            abi::emit_store_to_address(emitter, "x3", "sp", VALUE_LO_OFF);
            abi::emit_store_to_address(emitter, "x4", "sp", VALUE_HI_OFF);
            abi::emit_store_to_address(emitter, "x5", "sp", VALUE_TAG_OFF);
            emitter.instruction("cmn x2, #1");
            emitter.instruction(&format!("b.eq {}", numeric_key_label));
            emitter.instruction(&format!("b {}", string_key_label));
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(emitter, "rdi", SOURCE_HASH_OFF);
            abi::emit_load_temporary_stack_slot(emitter, "rsi", CURSOR_OFF);
            abi::emit_call_label(emitter, "__rt_hash_iter_next");
            emitter.instruction("cmp rax, -1");
            emitter.instruction(&format!("je {}", done_label));
            abi::emit_store_to_address(emitter, "rax", "rsp", CURSOR_OFF);
            abi::emit_store_to_address(emitter, "rdi", "rsp", KEY_PTR_OFF);
            abi::emit_store_to_address(emitter, "rdx", "rsp", KEY_LEN_OFF);
            abi::emit_store_to_address(emitter, "rcx", "rsp", VALUE_LO_OFF);
            abi::emit_store_to_address(emitter, "r8", "rsp", VALUE_HI_OFF);
            abi::emit_store_to_address(emitter, "r9", "rsp", VALUE_TAG_OFF);
            emitter.instruction("cmp rdx, -1");
            emitter.instruction(&format!("je {}", numeric_key_label));
            emitter.instruction(&format!("jmp {}", string_key_label));
        }
    }

    emitter.label(&numeric_key_label);
    emit_skip_if_consumed_numeric_key(skip_numeric_before, &skip_label, emitter);
    emit_use_next_variadic_numeric_key(KEY_PTR_OFF, KEY_LEN_OFF, NUMERIC_KEY_OFF, emitter);
    abi::emit_jump(emitter, &insert_label);

    emitter.label(&string_key_label);
    for (param_name, _) in sig.params.iter().take(skip_param_names_before) {
        emit_skip_if_key_matches_param(param_name, &skip_label, emitter, data);
    }

    emitter.label(&insert_label);
    emit_insert_assoc_variadic_entry(
        SCRATCH_BYTES,
        KEY_PTR_OFF,
        KEY_LEN_OFF,
        VALUE_LO_OFF,
        VALUE_HI_OFF,
        VALUE_TAG_OFF,
        &loop_label,
        emitter,
        ctx,
    );

    emitter.label(&skip_label);
    abi::emit_jump(emitter, &loop_label);
    emitter.label(&done_label);
    abi::emit_release_temporary_stack(emitter, SCRATCH_BYTES);
}

/// Skips numeric keys consumed by fixed parameters.
fn emit_skip_if_consumed_numeric_key(
    skip_numeric_before: usize,
    skip_label: &str,
    emitter: &mut Emitter,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(emitter, "x8", 16);
            abi::emit_load_int_immediate(emitter, "x9", skip_numeric_before as i64);
            emitter.instruction("cmp x8, x9");
            emitter.instruction(&format!("b.lt {}", skip_label));
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(emitter, "r10", 16);
            abi::emit_load_int_immediate(emitter, "r11", skip_numeric_before as i64);
            emitter.instruction("cmp r10, r11");
            emitter.instruction(&format!("jl {}", skip_label));
        }
    }
}

/// Rewrites an accepted numeric key to the next compact variadic key.
fn emit_use_next_variadic_numeric_key(
    key_ptr_off: usize,
    key_len_off: usize,
    numeric_key_off: usize,
    emitter: &mut Emitter,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(emitter, "x8", numeric_key_off);
            abi::emit_store_to_address(emitter, "x8", "sp", key_ptr_off);
            abi::emit_load_int_immediate(emitter, "x9", -1);
            abi::emit_store_to_address(emitter, "x9", "sp", key_len_off);
            emitter.instruction("add x8, x8, #1");
            abi::emit_store_to_address(emitter, "x8", "sp", numeric_key_off);
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(emitter, "r10", numeric_key_off);
            abi::emit_store_to_address(emitter, "r10", "rsp", key_ptr_off);
            abi::emit_load_int_immediate(emitter, "r11", -1);
            abi::emit_store_to_address(emitter, "r11", "rsp", key_len_off);
            emitter.instruction("add r10, 1");
            abi::emit_store_to_address(emitter, "r10", "rsp", numeric_key_off);
        }
    }
}

/// Skips a string key that matches a consumed fixed parameter name.
fn emit_skip_if_key_matches_param(
    param_name: &str,
    skip_label: &str,
    emitter: &mut Emitter,
    data: &mut DataSection,
) {
    let (key_label, key_len) = data.add_string(param_name.as_bytes());
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(emitter, "x1", 16);
            abi::emit_load_temporary_stack_slot(emitter, "x2", 24);
            abi::emit_symbol_address(emitter, "x3", &key_label);
            abi::emit_load_int_immediate(emitter, "x4", key_len as i64);
            abi::emit_call_label(emitter, "__rt_hash_key_eq");
            emitter.instruction("cmp x0, #0");
            emitter.instruction(&format!("b.ne {}", skip_label));
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(emitter, "rdi", 16);
            abi::emit_load_temporary_stack_slot(emitter, "rsi", 24);
            abi::emit_symbol_address(emitter, "rdx", &key_label);
            abi::emit_load_int_immediate(emitter, "rcx", key_len as i64);
            abi::emit_call_label(emitter, "__rt_hash_key_eq");
            emitter.instruction("test rax, rax");
            emitter.instruction(&format!("jne {}", skip_label));
        }
    }
}

/// Inserts the current saved hash iterator entry into the variadic hash.
#[allow(clippy::too_many_arguments)]
fn emit_insert_assoc_variadic_entry(
    hash_slot_off: usize,
    key_ptr_off: usize,
    key_len_off: usize,
    value_lo_off: usize,
    value_hi_off: usize,
    value_tag_off: usize,
    loop_label: &str,
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
) {
    let value_string_label = ctx.next_label("assoc_variadic_value_string");
    let value_ref_label = ctx.next_label("assoc_variadic_value_ref");
    let value_scalar_label = ctx.next_label("assoc_variadic_value_scalar");
    let insert_call_label = ctx.next_label("assoc_variadic_insert_call");

    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(emitter, "x5", value_tag_off);
            emitter.instruction("cmp x5, #1");
            emitter.instruction(&format!("b.eq {}", value_string_label));
            emitter.instruction("cmp x5, #4");
            emitter.instruction(&format!("b.lo {}", value_scalar_label));
            emitter.instruction("cmp x5, #7");
            emitter.instruction(&format!("b.hi {}", value_scalar_label));
            emitter.instruction(&format!("b {}", value_ref_label));
            emitter.label(&value_string_label);
            abi::emit_load_temporary_stack_slot(emitter, "x1", value_lo_off);
            abi::emit_load_temporary_stack_slot(emitter, "x2", value_hi_off);
            abi::emit_call_label(emitter, "__rt_str_persist");
            emitter.instruction("mov x3, x1");
            emitter.instruction("mov x4, x2");
            abi::emit_load_temporary_stack_slot(emitter, "x5", value_tag_off);
            emitter.instruction(&format!("b {}", insert_call_label));
            emitter.label(&value_ref_label);
            abi::emit_load_temporary_stack_slot(emitter, "x0", value_lo_off);
            abi::emit_call_label(emitter, "__rt_incref");
            abi::emit_load_temporary_stack_slot(emitter, "x3", value_lo_off);
            abi::emit_load_temporary_stack_slot(emitter, "x4", value_hi_off);
            abi::emit_load_temporary_stack_slot(emitter, "x5", value_tag_off);
            emitter.instruction(&format!("b {}", insert_call_label));
            emitter.label(&value_scalar_label);
            abi::emit_load_temporary_stack_slot(emitter, "x3", value_lo_off);
            abi::emit_load_temporary_stack_slot(emitter, "x4", value_hi_off);
            abi::emit_load_temporary_stack_slot(emitter, "x5", value_tag_off);
            emitter.label(&insert_call_label);
            abi::emit_load_temporary_stack_slot(emitter, "x0", hash_slot_off);
            abi::emit_load_temporary_stack_slot(emitter, "x1", key_ptr_off);
            abi::emit_load_temporary_stack_slot(emitter, "x2", key_len_off);
            abi::emit_call_label(emitter, "__rt_hash_set");
            emitter.instruction(&format!("b {}", loop_label));
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(emitter, "r9", value_tag_off);
            emitter.instruction("cmp r9, 1");
            emitter.instruction(&format!("je {}", value_string_label));
            emitter.instruction("cmp r9, 4");
            emitter.instruction(&format!("jl {}", value_scalar_label));
            emitter.instruction("cmp r9, 7");
            emitter.instruction(&format!("jg {}", value_scalar_label));
            emitter.instruction(&format!("jmp {}", value_ref_label));
            emitter.label(&value_string_label);
            abi::emit_load_temporary_stack_slot(emitter, "rax", value_lo_off);
            abi::emit_load_temporary_stack_slot(emitter, "rdx", value_hi_off);
            abi::emit_call_label(emitter, "__rt_str_persist");
            emitter.instruction("mov rcx, rax");
            emitter.instruction("mov r8, rdx");
            abi::emit_load_temporary_stack_slot(emitter, "r9", value_tag_off);
            emitter.instruction(&format!("jmp {}", insert_call_label));
            emitter.label(&value_ref_label);
            abi::emit_load_temporary_stack_slot(emitter, "rax", value_lo_off);
            abi::emit_call_label(emitter, "__rt_incref");
            abi::emit_load_temporary_stack_slot(emitter, "rcx", value_lo_off);
            abi::emit_load_temporary_stack_slot(emitter, "r8", value_hi_off);
            abi::emit_load_temporary_stack_slot(emitter, "r9", value_tag_off);
            emitter.instruction(&format!("jmp {}", insert_call_label));
            emitter.label(&value_scalar_label);
            abi::emit_load_temporary_stack_slot(emitter, "rcx", value_lo_off);
            abi::emit_load_temporary_stack_slot(emitter, "r8", value_hi_off);
            abi::emit_load_temporary_stack_slot(emitter, "r9", value_tag_off);
            emitter.label(&insert_call_label);
            abi::emit_load_temporary_stack_slot(emitter, "rdi", hash_slot_off);
            abi::emit_load_temporary_stack_slot(emitter, "rsi", key_ptr_off);
            abi::emit_load_temporary_stack_slot(emitter, "rdx", key_len_off);
            abi::emit_call_label(emitter, "__rt_hash_set");
            emitter.instruction(&format!("jmp {}", loop_label));
        }
    }
}

/// Stores the current result value into an indexed array slot.
fn emit_store_current_value_to_array_slot(
    emitter: &mut Emitter,
    ty: &PhpType,
    array_reg: &str,
    dest_reg: &str,
    offset_reg: &str,
    index_reg: &str,
) {
    emitter.instruction(&format!("mov {}, {}", dest_reg, array_reg));
    emit_add_usize(emitter, dest_reg, 24);
    emit_scale_index_to_offset(emitter, offset_reg, index_reg, ty.stack_size());
    emit_add_reg(emitter, dest_reg, offset_reg);
    match ty.codegen_repr() {
        PhpType::Float => {
            abi::emit_store_to_address(emitter, abi::float_result_reg(emitter), dest_reg, 0);
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_store_to_address(emitter, ptr_reg, dest_reg, 0);
            abi::emit_store_to_address(emitter, len_reg, dest_reg, 8);
        }
        PhpType::Void | PhpType::Never => {}
        _ => {
            abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), dest_reg, 0);
        }
    }
}

/// Returns a common pushed argument type for branch-merged callback args.
fn widen_callback_arg_type(left: &PhpType, right: &PhpType) -> PhpType {
    if left == right {
        return left.clone();
    }
    if matches!(left, PhpType::Mixed | PhpType::Union(_))
        || matches!(right, PhpType::Mixed | PhpType::Union(_))
    {
        return PhpType::Mixed;
    }
    if *left == PhpType::Str || *right == PhpType::Str {
        return PhpType::Str;
    }
    if *left == PhpType::Float || *right == PhpType::Float {
        return PhpType::Float;
    }
    if *left == PhpType::Void {
        return right.clone();
    }
    if *right == PhpType::Void {
        return left.clone();
    }
    left.clone()
}

/// Emits a fatal diagnostic for missing callback arguments.
fn emit_call_user_func_array_missing_arg_abort(emitter: &mut Emitter, data: &mut DataSection) {
    let (message_label, message_len) =
        data.add_string(b"Fatal error: call_user_func_array(): missing required argument\n");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #2");
            abi::emit_symbol_address(emitter, "x1", &message_label);
            emitter.instruction(&format!("mov x2, #{}", message_len));
            emitter.syscall(4);
            abi::emit_exit(emitter, 1);
        }
        Arch::X86_64 => {
            emitter.instruction("mov edi, 2");
            abi::emit_symbol_address(emitter, "rsi", &message_label);
            emitter.instruction(&format!("mov edx, {}", message_len));
            emitter.instruction("mov eax, 1");
            emitter.instruction("syscall");
            abi::emit_exit(emitter, 1);
        }
    }
}
