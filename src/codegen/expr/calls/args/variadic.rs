//! Purpose:
//! Lowers variadic array argument construction and storage.
//! Converts evaluated PHP argument expressions into temporary values ready for ABI assignment.
//!
//! Called from:
//! - `crate::codegen::expr::calls::args`
//!
//! Key details:
//! - Argument checks must happen at PHP-observable points without skipping later side effects.

use crate::codegen::emit::Emitter;
use crate::codegen::{abi, context::Context, data_section::DataSection, functions};
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub(super) fn store_current_array_element(
    emitter: &mut Emitter,
    array_reg: &str,
    elem_idx: usize,
    elem_ty: &PhpType,
) {
    match elem_ty.codegen_repr() {
        PhpType::Float => {
            abi::emit_store_to_address(emitter, abi::float_result_reg(emitter), array_reg, 24 + elem_idx * 8); // store float element into the variadic array payload
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_store_to_address(emitter, ptr_reg, array_reg, 24 + elem_idx * 16); // store variadic string pointer into the array payload
            abi::emit_store_to_address(emitter, len_reg, array_reg, 24 + elem_idx * 16 + 8); // store variadic string length next to the payload pointer
        }
        PhpType::Void => {}
        _ => {
            abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), array_reg, 24 + elem_idx * 8); // store scalar or boxed variadic payload into the array data area
        }
    }
}

pub(super) fn variadic_container_elem_ty(elem_ty: &PhpType) -> PhpType {
    if matches!(elem_ty.codegen_repr(), PhpType::Iterable) {
        PhpType::Mixed
    } else {
        elem_ty.clone()
    }
}

pub(crate) fn emit_empty_variadic_array_arg(context_label: &str, emitter: &mut Emitter) -> PhpType {
    emitter.comment(context_label);
    let (capacity_reg, elem_size_reg) = match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => ("x0", "x1"),
        crate::codegen::platform::Arch::X86_64 => ("rdi", "rsi"),
    };
    abi::emit_load_int_immediate(emitter, capacity_reg, 4);
    abi::emit_load_int_immediate(emitter, elem_size_reg, 8);
    abi::emit_call_label(emitter, "__rt_array_new");
    abi::emit_push_result_value(emitter, &PhpType::Array(Box::new(PhpType::Int)));
    PhpType::Array(Box::new(PhpType::Int))
}

pub(crate) fn emit_variadic_array_arg_from_exprs(
    variadic_args: &[Expr],
    context_label: &str,
    retain_heap_values: bool,
    stamp_value_type: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let elem_count = variadic_args.len();
    let first_elem_ty = functions::infer_contextual_type(&variadic_args[0], ctx);
    let container_elem_ty = variadic_container_elem_ty(&first_elem_ty);
    let elem_size = match container_elem_ty.codegen_repr() {
        PhpType::Str => 16,
        _ => 8,
    };
    let (capacity_reg, elem_size_reg, peek_reg, len_reg) = match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => ("x0", "x1", "x9", "x10"),
        crate::codegen::platform::Arch::X86_64 => ("rdi", "rsi", "r11", "r10"),
    };

    emitter.comment(&format!("{} ({} elements)", context_label, elem_count));
    abi::emit_load_int_immediate(emitter, capacity_reg, elem_count as i64);
    abi::emit_load_int_immediate(emitter, elem_size_reg, elem_size as i64);
    abi::emit_call_label(emitter, "__rt_array_new");
    abi::emit_push_result_value(emitter, &PhpType::Array(Box::new(container_elem_ty.clone())));

    for (idx, variadic_arg) in variadic_args.iter().enumerate() {
        let mut elem_ty = super::super::super::emit_expr(variadic_arg, emitter, ctx, data);
        let boxed_for_container = if matches!(container_elem_ty, PhpType::Mixed)
            && !matches!(elem_ty, PhpType::Mixed | PhpType::Union(_))
        {
            crate::codegen::emit_box_current_value_as_mixed(emitter, &elem_ty);
            elem_ty = PhpType::Mixed;
            true
        } else {
            false
        };
        if retain_heap_values && !boxed_for_container {
            super::super::super::retain_borrowed_heap_arg(emitter, variadic_arg, &elem_ty);
        }
        match emitter.target.arch {
            crate::codegen::platform::Arch::AArch64 => {
                emitter.instruction(&format!("ldr {}, [sp]", peek_reg));        // peek the variadic array pointer without removing it from the stack
            }
            crate::codegen::platform::Arch::X86_64 => {
                emitter.instruction(&format!("mov {}, QWORD PTR [rsp]", peek_reg)); // peek the variadic array pointer without removing it from the stack
            }
        }
        if stamp_value_type && idx == 0 {
            super::super::super::arrays::emit_array_value_type_stamp(emitter, peek_reg, &elem_ty);
        }
        store_current_array_element(emitter, peek_reg, idx, &elem_ty);
        abi::emit_load_int_immediate(emitter, len_reg, (idx + 1) as i64);
        abi::emit_store_to_address(emitter, len_reg, peek_reg, 0);
    }

    PhpType::Array(Box::new(container_elem_ty))
}
