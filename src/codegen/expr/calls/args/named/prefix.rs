//! Purpose:
//! Lowers prefix positional elements produced by spread arrays before named arguments.
//! Works with the shared call-argument plan to preserve PHP named-argument semantics.
//!
//! Called from:
//! - `crate::codegen::expr::calls::args::named`
//!
//! Key details:
//! - Side effects occur in source order, while final argument materialization follows parameter and ABI order.

use crate::codegen::emit::Emitter;
use crate::codegen::{abi, context::Context, data_section::DataSection};
use crate::parser::ast::Expr;
use crate::types::{PhpType};

use super::temps::source_temp_offset;
use super::super::{
    array_element_stride, emit_array_length_bounds_check, emit_named_spread_length_abort,
    load_array_element_to_result, push_expr_arg, push_loaded_array_element_arg,
    spread_source_elem_ty,
};

pub(super) fn emit_prefix_array_length_check(
    prefix_temp_idx: usize,
    source_temp_types: &[PhpType],
    min_len: usize,
    max_len: Option<usize>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let ok_label = ctx.next_label("named_prefix_len_ok");
    let fail_label = ctx.next_label("named_prefix_len_fail");
    emitter.comment("validate named-argument positional prefix length");
    let prefix_offset = source_temp_offset(source_temp_types, prefix_temp_idx, 0);
    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(emitter, "x8", prefix_offset);
            emitter.instruction("ldr x9, [x8]");                                // load the evaluated positional-prefix array length
            emit_array_length_bounds_check("x9", min_len, max_len, &fail_label, &ok_label, emitter);
        }
        crate::codegen::platform::Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(emitter, "r8", prefix_offset);
            emitter.instruction("mov r10, QWORD PTR [r8]");                     // load the evaluated positional-prefix array length
            emit_array_length_bounds_check("r10", min_len, max_len, &fail_label, &ok_label, emitter);
        }
    }
    emitter.label(&fail_label);
    emit_named_spread_length_abort(emitter, data);
    emitter.label(&ok_label);
}

pub(super) fn push_prefix_array_element_arg(
    prefix_temp_idx: usize,
    element_idx: usize,
    default: Option<&Expr>,
    target_ty: Option<&PhpType>,
    source_temp_types: &[PhpType],
    final_pushed_bytes: usize,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    if let Some(default) = default {
        let use_default = ctx.next_label("named_prefix_default");
        let done = ctx.next_label("named_prefix_done");
        emit_branch_if_prefix_element_missing(
            prefix_temp_idx,
            element_idx,
            source_temp_types,
            final_pushed_bytes,
            &use_default,
            emitter,
        );
        let loaded_ty = push_existing_prefix_array_element_arg(
            prefix_temp_idx,
            element_idx,
            target_ty,
            source_temp_types,
            final_pushed_bytes,
            emitter,
            ctx,
            data,
        );
        abi::emit_jump(emitter, &done);
        emitter.label(&use_default);
        let default_ty = push_expr_arg(default, target_ty, emitter, ctx, data);
        emitter.label(&done);
        return super::super::super::super::widen_codegen_type(&loaded_ty, &default_ty);
    }

    push_existing_prefix_array_element_arg(
        prefix_temp_idx,
        element_idx,
        target_ty,
        source_temp_types,
        final_pushed_bytes,
        emitter,
        ctx,
        data,
    )
}

fn emit_branch_if_prefix_element_missing(
    prefix_temp_idx: usize,
    element_idx: usize,
    source_temp_types: &[PhpType],
    final_pushed_bytes: usize,
    label: &str,
    emitter: &mut Emitter,
) {
    let prefix_offset = source_temp_offset(source_temp_types, prefix_temp_idx, final_pushed_bytes);
    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(emitter, "x8", prefix_offset);
            emitter.instruction("ldr x9, [x8]");                                // load prefix length before choosing spread element or default
            abi::emit_load_int_immediate(emitter, "x10", element_idx as i64);
            emitter.instruction("cmp x9, x10");                                 // check whether this optional prefix element exists
            emitter.instruction(&format!("b.le {}", label));                    // use the default when the prefix is too short for this slot
        }
        crate::codegen::platform::Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(emitter, "r8", prefix_offset);
            emitter.instruction("mov r10, QWORD PTR [r8]");                     // load prefix length before choosing spread element or default
            abi::emit_load_int_immediate(emitter, "r11", element_idx as i64);
            emitter.instruction("cmp r10, r11");                                // check whether this optional prefix element exists
            emitter.instruction(&format!("jle {}", label));                     // use the default when the prefix is too short for this slot
        }
    }
}

fn push_existing_prefix_array_element_arg(
    prefix_temp_idx: usize,
    element_idx: usize,
    target_ty: Option<&PhpType>,
    source_temp_types: &[PhpType],
    final_pushed_bytes: usize,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let prefix_ty = source_temp_types[prefix_temp_idx].clone();
    let source_elem_ty = spread_source_elem_ty(&prefix_ty);
    let elem_stride = array_element_stride(&source_elem_ty);
    let prefix_offset = source_temp_offset(source_temp_types, prefix_temp_idx, final_pushed_bytes);
    let array_data_reg = match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => "x20",
        crate::codegen::platform::Arch::X86_64 => "r10",
    };
    abi::emit_load_temporary_stack_slot(emitter, array_data_reg, prefix_offset);
    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            emitter.instruction(&format!("add {}, {}, #24", array_data_reg, array_data_reg)); // address the positional-prefix array payload
        }
        crate::codegen::platform::Arch::X86_64 => {
            emitter.instruction(&format!("add {}, 24", array_data_reg));        // address the positional-prefix array payload
        }
    }
    load_array_element_to_result(emitter, &source_elem_ty, array_data_reg, element_idx * elem_stride);
    push_loaded_array_element_arg(&source_elem_ty, target_ty, emitter, ctx, data)
}
