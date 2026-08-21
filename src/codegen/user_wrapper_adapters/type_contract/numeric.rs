//! Purpose:
//! Emits PHP weak numeric-string union conversions for stream-wrapper callbacks.
//! Preserves source strings across classification and boxes each selected scalar member.
//!
//! Called from:
//! - `crate::codegen::user_wrapper_adapters::type_contract`.
//!
//! Key details:
//! - `int|float` uses the runtime classifier derived from php-src numeric-string semantics.
//! - A bool member receives only strings rejected by every preferred numeric member.

use crate::codegen::{abi, emit_box_current_value_as_mixed};
use crate::codegen_support::{emit::Emitter, platform::Arch};
use crate::types::PhpType;

use super::coercion;

/// Converts a numeric string to the selected int/float/bool union member and boxes the result.
pub(super) fn emit_string_numeric_union_conversion(
    emitter: &mut Emitter,
    label_prefix: &str,
    mixed_stack_slot: Option<usize>,
    allow_int: bool,
    allow_float: bool,
    fallback_bool: bool,
) {
    if allow_int && !allow_float && !fallback_bool {
        load_numeric_conversion_source(emitter, mixed_stack_slot, false);
        coercion::emit_scalar_conversion(emitter, label_prefix, &PhpType::Str, &PhpType::Int);
        emit_box_current_value_as_mixed(emitter, &PhpType::Int);
        return;
    }
    if allow_float && !allow_int && !fallback_bool {
        load_numeric_conversion_source(emitter, mixed_stack_slot, false);
        coercion::emit_scalar_conversion(emitter, label_prefix, &PhpType::Str, &PhpType::Float);
        emit_box_current_value_as_mixed(emitter, &PhpType::Float);
        return;
    }
    let int_case = format!("{label_prefix}_numeric_union_int");
    let float_case = format!("{label_prefix}_numeric_union_float");
    let bool_case = format!("{label_prefix}_numeric_union_bool");
    let done = format!("{label_prefix}_numeric_union_boxed");
    let saved_static_string = mixed_stack_slot.is_none();
    if saved_static_string {
        let (ptr, len) = abi::string_result_regs(emitter);
        abi::emit_push_reg_pair(emitter, ptr, len);
    }
    load_numeric_conversion_source(emitter, mixed_stack_slot, saved_static_string);
    abi::emit_call_label(emitter, "__rt_str_numeric_union_kind");
    if fallback_bool {
        emit_branch_if_int_result_zero(emitter, &bool_case);
    }
    if allow_int && allow_float {
        emit_branch_if_int_result_not_equal(emitter, 1, &float_case);
        abi::emit_jump(emitter, &int_case);
    } else if allow_float {
        abi::emit_jump(emitter, &float_case);
    } else {
        if fallback_bool {
            emit_branch_if_int_result_equal(emitter, 2, &bool_case);
        }
        abi::emit_jump(emitter, &int_case);
    }
    emitter.label(&int_case);
    load_numeric_conversion_source(emitter, mixed_stack_slot, saved_static_string);
    coercion::emit_scalar_conversion(emitter, label_prefix, &PhpType::Str, &PhpType::Int);
    emit_box_current_value_as_mixed(emitter, &PhpType::Int);
    abi::emit_jump(emitter, &done);
    emitter.label(&float_case);
    load_numeric_conversion_source(emitter, mixed_stack_slot, saved_static_string);
    coercion::emit_scalar_conversion(emitter, label_prefix, &PhpType::Str, &PhpType::Float);
    emit_box_current_value_as_mixed(emitter, &PhpType::Float);
    abi::emit_jump(emitter, &done);
    if fallback_bool {
        emitter.label(&bool_case);
        load_numeric_conversion_source(emitter, mixed_stack_slot, saved_static_string);
        coercion::emit_scalar_conversion(emitter, label_prefix, &PhpType::Str, &PhpType::Bool);
        emit_box_current_value_as_mixed(emitter, &PhpType::Bool);
    }
    emitter.label(&done);
    if saved_static_string {
        abi::emit_release_temporary_stack(emitter, 16);
    }
}

/// Classifies an unboxed string and branches when it can satisfy numeric union members.
pub(super) fn emit_string_numeric_preflight(
    emitter: &mut Emitter,
    allow_float: bool,
    success: &str,
) {
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rax, rdi");                                    // move the unboxed string pointer into the classifier ABI
    }
    abi::emit_call_label(emitter, "__rt_str_numeric_union_kind");
    if allow_float {
        emit_branch_if_int_nonzero(emitter, success);
    } else {
        emit_branch_if_int_result_equal(emitter, 1, success);
        emit_branch_if_int_result_equal(emitter, 3, success);
    }
}

/// Loads a concrete string or unboxes the saved dynamic Mixed source into string registers.
fn load_numeric_conversion_source(
    emitter: &mut Emitter,
    mixed_stack_slot: Option<usize>,
    saved_static_string: bool,
) {
    let Some(slot) = mixed_stack_slot else {
        if saved_static_string {
            let (ptr, len) = abi::string_result_regs(emitter);
            abi::emit_load_temporary_stack_slot(emitter, ptr, 0);
            abi::emit_load_temporary_stack_slot(emitter, len, 8);
        }
        return;
    };
    abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), slot);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rax, rdi");                                    // move the unboxed string pointer into the string result register
    }
}

/// Branches when the current integer result is nonzero.
fn emit_branch_if_int_nonzero(emitter: &mut Emitter, label: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbnz x0, {}", label));                // accept a classified PHP numeric string
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // inspect the numeric-string classification result
            emitter.instruction(&format!("jnz {}", label));                     // accept a classified PHP numeric string
        }
    }
}

/// Branches when the integer result equals one immediate numeric-kind value.
fn emit_branch_if_int_result_equal(emitter: &mut Emitter, value: i64, label: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp x0, #{}", value));                // compare the numeric-string union kind
            emitter.instruction(&format!("b.eq {}", label));                    // select the matching weak scalar member
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp rax, {}", value));                // compare the numeric-string union kind
            emitter.instruction(&format!("je {}", label));                      // select the matching weak scalar member
        }
    }
}

/// Branches when the integer result differs from one immediate numeric-kind value.
fn emit_branch_if_int_result_not_equal(emitter: &mut Emitter, value: i64, label: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp x0, #{}", value));                // compare the numeric-string union kind
            emitter.instruction(&format!("b.ne {}", label));                    // non-integer numeric forms select the float member
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp rax, {}", value));                // compare the numeric-string union kind
            emitter.instruction(&format!("jne {}", label));                     // non-integer numeric forms select the float member
        }
    }
}

/// Branches when the current integer result is zero.
fn emit_branch_if_int_result_zero(emitter: &mut Emitter, label: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz x0, {}", label));                 // non-numeric strings fall back to an allowed bool member
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // inspect the numeric-string classification result
            emitter.instruction(&format!("jz {}", label));                      // non-numeric strings fall back to bool
        }
    }
}
