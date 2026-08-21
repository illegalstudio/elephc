//! Purpose:
//! Normalizes PHP method results into the fixed userspace stream-wrapper callback ABI.
//! Applies php-src's per-slot truthiness, scalar-conversion, and exact-type contracts.
//!
//! Called from:
//! - `crate::codegen::user_wrapper_adapters::emit_user_wrapper_adapter()`.
//!
//! Key details:
//! - Every owned method result is first transferred into a boxed `Mixed` cell.
//! - Scalar and string callback results outlive that box through explicit preservation.
//! - `stream_eof` receives a hidden runtime mode so post-read truthiness stays
//!   distinct from `feof()`'s exact-bool warning without process-global state.

use crate::codegen::{
    DataSection, abi, emit_box_current_owned_value_as_mixed, emit_box_current_value_as_mixed,
};
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::types::PhpType;

/// The runtime result shape and PHP conversion rule for one wrapper callback slot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WrapperReturnContract {
    TruthyBool,
    String,
    StringUnlessBool,
    IntWithFalseFailure,
    ExactInt,
    ExactBool,
    StatArray,
    StreamResource,
    Discard,
}

/// Normalizes one owned compiled-method result for its fixed wrapper vtable slot.
pub(super) fn emit_normalize_wrapper_return(
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: usize,
    return_ty: &PhpType,
    label_prefix: &str,
    class_name: &str,
    method_name: &str,
    eof_check_mode_offset: Option<usize>,
) {
    let source_ty = wrapper_return_codegen_type(return_ty);
    emit_box_current_owned_value_as_mixed(emitter, &source_ty);
    if slot == 4 {
        emit_normalize_stream_eof_return(
            emitter,
            data,
            label_prefix,
            class_name,
            method_name,
            eof_check_mode_offset.expect("stream_eof adapter mode slot"),
        );
        return;
    }
    match wrapper_return_contract(slot) {
        WrapperReturnContract::TruthyBool => {
            emit_cast_boxed_mixed_to_bool(emitter);
        }
        WrapperReturnContract::String => {
            emit_cast_boxed_mixed_to_string(emitter, label_prefix);
        }
        WrapperReturnContract::StringUnlessBool => {
            emit_cast_boxed_mixed_to_directory_string(emitter, label_prefix);
        }
        WrapperReturnContract::IntWithFalseFailure => {
            emit_cast_boxed_mixed_to_int(emitter, label_prefix, true);
        }
        WrapperReturnContract::ExactInt => {
            emit_exact_boxed_mixed_scalar(emitter, label_prefix, 0, -1);
        }
        WrapperReturnContract::ExactBool => {
            emit_exact_boxed_mixed_scalar(emitter, label_prefix, 3, 0);
        }
        WrapperReturnContract::StatArray => {
            emit_require_boxed_mixed_array(emitter, label_prefix);
        }
        WrapperReturnContract::StreamResource => {
            emit_exact_boxed_mixed_scalar(emitter, label_prefix, 9, -1);
        }
        WrapperReturnContract::Discard => {
            abi::emit_decref_if_refcounted(emitter, &PhpType::Mixed);
            abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
        }
    }
}

/// Applies php-src's context-sensitive `stream_eof()` result contract.
fn emit_normalize_stream_eof_return(
    emitter: &mut Emitter,
    data: &mut DataSection,
    label_prefix: &str,
    class_name: &str,
    method_name: &str,
    mode_offset: usize,
) {
    let lenient_label = format!("{label_prefix}_return_eof_lenient");
    let invalid_label = format!("{label_prefix}_return_eof_invalid");
    let done_label = format!("{label_prefix}_return_eof_done");
    let pointer = abi::int_result_reg(emitter);
    let mode = abi::secondary_scratch_reg(emitter);
    let tag = abi::tertiary_scratch_reg(emitter);
    abi::load_at_offset(emitter, mode, mode_offset);
    emit_compare_and_branch_equal(emitter, mode, 0, &lenient_label);
    abi::emit_load_from_address(emitter, tag, pointer, 0);
    emit_compare_and_branch_not_equal(emitter, tag, 3, &invalid_label);
    abi::emit_load_from_address(emitter, tag, pointer, 8);
    abi::emit_push_reg(emitter, tag);
    abi::emit_decref_if_refcounted(emitter, &PhpType::Mixed);
    abi::emit_pop_reg(emitter, pointer);
    abi::emit_jump(emitter, &done_label);

    emitter.label(&invalid_label);
    emit_stream_eof_type_warning(
        emitter,
        data,
        label_prefix,
        class_name,
        method_name,
    );
    abi::emit_decref_if_refcounted(emitter, &PhpType::Mixed);
    abi::emit_load_int_immediate(emitter, pointer, 1);
    abi::emit_jump(emitter, &done_label);

    emitter.label(&lenient_label);
    emit_cast_boxed_mixed_to_bool(emitter);
    emitter.label(&done_label);
}

/// Emits the suppressible exact-type warning for an invalid strict EOF result.
fn emit_stream_eof_type_warning(
    emitter: &mut Emitter,
    data: &mut DataSection,
    label_prefix: &str,
    class_name: &str,
    method_name: &str,
) {
    let type_ready_label = format!("{label_prefix}_return_eof_type_ready");
    let object_label = format!("{label_prefix}_return_eof_type_object");
    let type_cases = [
        (0_u8, "int"),
        (1, "string"),
        (2, "float"),
        (4, "array"),
        (5, "array"),
        (8, "null"),
        (9, "resource"),
        (10, "Closure"),
    ];
    let pointer = abi::int_result_reg(emitter);
    let tag = abi::secondary_scratch_reg(emitter);
    abi::emit_push_reg(emitter, pointer);
    abi::emit_load_from_address(emitter, tag, pointer, 0);
    for (runtime_tag, _) in type_cases {
        let case_label = format!("{label_prefix}_return_eof_type_{runtime_tag}");
        emit_compare_and_branch_equal(emitter, tag, runtime_tag as i64, &case_label);
    }
    emit_compare_and_branch_equal(emitter, tag, 6, &object_label);
    emit_static_type_name(emitter, data, "mixed");
    abi::emit_jump(emitter, &type_ready_label);

    for (runtime_tag, type_name) in type_cases {
        let case_label = format!("{label_prefix}_return_eof_type_{runtime_tag}");
        emitter.label(&case_label);
        emit_static_type_name(emitter, data, type_name);
        abi::emit_jump(emitter, &type_ready_label);
    }

    emitter.label(&object_label);
    emit_object_type_name(emitter);
    emitter.label(&type_ready_label);
    let (type_ptr, type_len) = abi::string_result_regs(emitter);
    abi::emit_push_reg_pair(emitter, type_ptr, type_len);
    let prefix = format!(
        "Warning: feof(): {}::{} value must be of type bool, ",
        class_name.trim_start_matches('\\'),
        method_name,
    );
    emit_warning_fragment(emitter, data, prefix.as_bytes());
    match emitter.target.arch {
        Arch::AArch64 => abi::emit_pop_reg_pair(emitter, "x1", "x2"),
        Arch::X86_64 => abi::emit_pop_reg_pair(emitter, "rdi", "rsi"),
    }
    abi::emit_call_label(emitter, "__rt_diag_warning");
    emit_warning_fragment(emitter, data, b" given\n");
    abi::emit_pop_reg(emitter, pointer);
}

/// Loads one static PHP diagnostic type name into the string result registers.
fn emit_static_type_name(emitter: &mut Emitter, data: &mut DataSection, type_name: &str) {
    let (label, len) = data.add_string(type_name.as_bytes());
    let (pointer, length) = abi::string_result_regs(emitter);
    abi::emit_symbol_address(emitter, pointer, &label);
    abi::emit_load_int_immediate(emitter, length, len as i64);
}

/// Resolves an object payload's concrete runtime class name for a PHP diagnostic.
fn emit_object_type_name(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [x0, #8]");                            // load the invalid EOF result's object payload
            emitter.instruction("ldr x10, [x9]");                               // load the concrete runtime class identifier
            abi::emit_symbol_address(emitter, "x11", "_class_name_entries");
            emitter.instruction("lsl x10, x10, #4");                            // scale the class id to one name-table row
            emitter.instruction("add x11, x11, x10");                           // address the concrete class-name metadata
            emitter.instruction("ldr x1, [x11]");                               // return the concrete PHP class-name pointer
            emitter.instruction("ldr x2, [x11, #8]");                           // return the concrete PHP class-name length
        }
        Arch::X86_64 => {
            emitter.instruction("mov r9, QWORD PTR [rax + 8]");                 // load the invalid EOF result's object payload
            emitter.instruction("mov r10, QWORD PTR [r9]");                     // load the concrete runtime class identifier
            abi::emit_symbol_address(emitter, "r11", "_class_name_entries");
            emitter.instruction("shl r10, 4");                                  // scale the class id to one name-table row
            emitter.instruction("mov rax, QWORD PTR [r11 + r10]");              // return the concrete PHP class-name pointer
            emitter.instruction("mov rdx, QWORD PTR [r11 + r10 + 8]");          // return the concrete PHP class-name length
        }
    }
}

/// Writes one static warning fragment through the `@`-aware diagnostic channel.
fn emit_warning_fragment(emitter: &mut Emitter, data: &mut DataSection, fragment: &[u8]) {
    let (label, len) = data.add_string(fragment);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x1", &label);
            abi::emit_load_int_immediate(emitter, "x2", len as i64);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "rdi", &label);
            abi::emit_load_int_immediate(emitter, "rsi", len as i64);
        }
    }
    abi::emit_call_label(emitter, "__rt_diag_warning");
}

/// Returns the concrete ABI type actually produced by the compiled method body.
fn wrapper_return_codegen_type(return_ty: &PhpType) -> PhpType {
    match return_ty {
        PhpType::Resource(_) => return_ty.clone(),
        _ => return_ty.codegen_repr(),
    }
}

/// Returns php-src's result contract for one fixed userspace-wrapper vtable slot.
fn wrapper_return_contract(slot: usize) -> WrapperReturnContract {
    match slot {
        0 | 4 | 6 | 7 | 13 | 19 => WrapperReturnContract::TruthyBool,
        1 | 21 | 22 => WrapperReturnContract::Discard,
        2 => WrapperReturnContract::String,
        3 => WrapperReturnContract::IntWithFalseFailure,
        5 => WrapperReturnContract::ExactInt,
        8 | 9 => WrapperReturnContract::StatArray,
        10 => WrapperReturnContract::StreamResource,
        11 | 12 | 14 | 15 | 16 | 17 | 18 => WrapperReturnContract::ExactBool,
        20 => WrapperReturnContract::StringUnlessBool,
        _ => unreachable!("unknown user-wrapper vtable slot"),
    }
}

/// Converts and consumes one boxed callback result using PHP truthiness.
fn emit_cast_boxed_mixed_to_bool(emitter: &mut Emitter) {
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
    abi::emit_call_label(emitter, "__rt_mixed_cast_bool");
    emit_release_pushed_mixed_preserving_int(emitter);
}

/// Converts and consumes one boxed callback result as a persistent PHP string.
fn emit_cast_boxed_mixed_to_string(emitter: &mut Emitter, label_prefix: &str) {
    let already_string_label = format!("{label_prefix}_return_string_source");
    let release_label = format!("{label_prefix}_return_string_release");
    let pointer = abi::int_result_reg(emitter);
    let tag = abi::secondary_scratch_reg(emitter);
    abi::emit_load_from_address(emitter, tag, pointer, 0);
    emit_compare_and_branch_equal(emitter, tag, 1, &already_string_label);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
    abi::emit_call_label(emitter, "__rt_mixed_cast_string");
    abi::emit_call_label(emitter, "__rt_str_persist");
    abi::emit_jump(emitter, &release_label);
    emitter.label(&already_string_label);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
    abi::emit_call_label(emitter, "__rt_mixed_cast_string");
    emitter.label(&release_label);
    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
    abi::emit_push_reg_pair(emitter, ptr_reg, len_reg);
    abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), 16);
    abi::emit_decref_if_refcounted(emitter, &PhpType::Mixed);
    abi::emit_pop_reg_pair(emitter, ptr_reg, len_reg);
    abi::emit_release_temporary_stack(emitter, 16);
}

/// Converts a directory-read result, treating either boolean as end-of-directory.
fn emit_cast_boxed_mixed_to_directory_string(emitter: &mut Emitter, label_prefix: &str) {
    let bool_label = format!("{label_prefix}_return_dir_bool");
    let done_label = format!("{label_prefix}_return_dir_done");
    let pointer = abi::int_result_reg(emitter);
    let tag = abi::secondary_scratch_reg(emitter);
    abi::emit_load_from_address(emitter, tag, pointer, 0);
    emit_compare_and_branch_equal(emitter, tag, 3, &bool_label);
    emit_cast_boxed_mixed_to_string(emitter, label_prefix);
    abi::emit_jump(emitter, &done_label);
    emitter.label(&bool_label);
    abi::emit_decref_if_refcounted(emitter, &PhpType::Mixed);
    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
    abi::emit_load_int_immediate(emitter, ptr_reg, 0);
    abi::emit_load_int_immediate(emitter, len_reg, 0);
    emitter.label(&done_label);
}

/// Converts and consumes one boxed result to int, optionally mapping exact false to -1.
fn emit_cast_boxed_mixed_to_int(
    emitter: &mut Emitter,
    label_prefix: &str,
    false_is_failure: bool,
) {
    if !false_is_failure {
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
        abi::emit_call_label(emitter, "__rt_mixed_cast_int");
        emit_release_pushed_mixed_preserving_int(emitter);
        return;
    }

    let cast_label = format!("{label_prefix}_return_int_cast");
    let false_label = format!("{label_prefix}_return_int_false");
    let done_label = format!("{label_prefix}_return_int_done");
    let pointer = abi::int_result_reg(emitter);
    let tag = abi::secondary_scratch_reg(emitter);
    let payload = abi::tertiary_scratch_reg(emitter);
    abi::emit_load_from_address(emitter, tag, pointer, 0);
    emit_compare_and_branch_not_equal(emitter, tag, 3, &cast_label);
    abi::emit_load_from_address(emitter, payload, pointer, 8);
    emit_compare_and_branch_equal(emitter, payload, 0, &false_label);
    emitter.label(&cast_label);
    abi::emit_push_reg(emitter, pointer);
    abi::emit_call_label(emitter, "__rt_mixed_cast_int");
    emit_release_pushed_mixed_preserving_int(emitter);
    abi::emit_jump(emitter, &done_label);
    emitter.label(&false_label);
    abi::emit_decref_if_refcounted(emitter, &PhpType::Mixed);
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), -1);
    emitter.label(&done_label);
}

/// Accepts one exact boxed scalar tag, returning its payload or a failure sentinel.
fn emit_exact_boxed_mixed_scalar(
    emitter: &mut Emitter,
    label_prefix: &str,
    expected_tag: i64,
    failure: i64,
) {
    let invalid_label = format!("{label_prefix}_return_tag_{expected_tag}_invalid");
    let done_label = format!("{label_prefix}_return_tag_{expected_tag}_done");
    let pointer = abi::int_result_reg(emitter);
    let tag = abi::secondary_scratch_reg(emitter);
    let payload = abi::tertiary_scratch_reg(emitter);
    abi::emit_load_from_address(emitter, tag, pointer, 0);
    emit_compare_and_branch_not_equal(emitter, tag, expected_tag, &invalid_label);
    abi::emit_load_from_address(emitter, payload, pointer, 8);
    abi::emit_push_reg(emitter, pointer);
    emit_move_int_result(emitter, payload);
    emit_release_pushed_mixed_preserving_int(emitter);
    abi::emit_jump(emitter, &done_label);
    emitter.label(&invalid_label);
    abi::emit_decref_if_refcounted(emitter, &PhpType::Mixed);
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), failure);
    emitter.label(&done_label);
}

/// Keeps only boxed indexed/associative arrays; every other result becomes boxed false.
fn emit_require_boxed_mixed_array(emitter: &mut Emitter, label_prefix: &str) {
    let valid_label = format!("{label_prefix}_return_stat_array");
    let done_label = format!("{label_prefix}_return_stat_done");
    let pointer = abi::int_result_reg(emitter);
    let tag = abi::secondary_scratch_reg(emitter);
    abi::emit_load_from_address(emitter, tag, pointer, 0);
    emit_compare_and_branch_equal(emitter, tag, 4, &valid_label);
    emit_compare_and_branch_equal(emitter, tag, 5, &valid_label);
    abi::emit_decref_if_refcounted(emitter, &PhpType::Mixed);
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
    emit_box_current_value_as_mixed(emitter, &PhpType::Bool);
    abi::emit_jump(emitter, &done_label);
    emitter.label(&valid_label);
    emitter.label(&done_label);
}

/// Releases the pushed boxed owner while keeping the current integer result live.
fn emit_release_pushed_mixed_preserving_int(emitter: &mut Emitter) {
    abi::emit_push_result_value(emitter, &PhpType::Int);
    abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), 16);
    abi::emit_decref_if_refcounted(emitter, &PhpType::Mixed);
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));
    abi::emit_release_temporary_stack(emitter, 16);
}

/// Moves one scratch integer payload into the target's integer result register.
fn emit_move_int_result(emitter: &mut Emitter, source: &str) {
    let result = abi::int_result_reg(emitter);
    if source != result {
        emitter.instruction(&format!("mov {result}, {source}"));                // move the validated callback payload into the fixed result register
    }
}

/// Emits a target-aware integer equality branch.
fn emit_compare_and_branch_equal(
    emitter: &mut Emitter,
    register: &str,
    immediate: i64,
    label: &str,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {register}, #{immediate}"));      // compare the callback result tag or payload with the required value
            emitter.instruction(&format!("b.eq {label}"));                      // branch when the callback result component matches
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {register}, {immediate}"));       // compare the callback result tag or payload with the required value
            emitter.instruction(&format!("je {label}"));                        // branch when the callback result component matches
        }
    }
}

/// Emits a target-aware integer inequality branch.
fn emit_compare_and_branch_not_equal(
    emitter: &mut Emitter,
    register: &str,
    immediate: i64,
    label: &str,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {register}, #{immediate}"));      // compare the callback result tag or payload with the required value
            emitter.instruction(&format!("b.ne {label}"));                      // branch when the callback result component differs
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {register}, {immediate}"));       // compare the callback result tag or payload with the required value
            emitter.instruction(&format!("jne {label}"));                       // branch when the callback result component differs
        }
    }
}
