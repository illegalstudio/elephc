//! Purpose:
//! Emits PHP weak-scalar deprecations required while validating stream-wrapper callbacks.
//! Keeps lossy float-to-int diagnostics separate from ordinary explicit-cast helpers.
//!
//! Called from:
//! - `crate::codegen::user_wrapper_adapters::coercion`.
//!
//! Key details:
//! - Only in-range, fractional float and float-string conversions to `int` are deprecated.
//! - Diagnostics run during argument preflight, before later parameters or callback code.
//! - Dynamic fragments use the shared `@`-aware diagnostic channel on every target.

use crate::codegen::{abi, data_section::DataSection};
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

const FLOAT_PREFIX: &[u8] = b"Deprecated: Implicit conversion from float ";
const FLOAT_STRING_PREFIX: &[u8] =
    b"Deprecated: Implicit conversion from float-string \"";
const FLOAT_SUFFIX: &[u8] = b" to int loses precision\n";
const FLOAT_STRING_SUFFIX: &[u8] = b"\" to int loses precision\n";

/// Emits a deprecation when the current string weakly converts to a lossy integer.
pub(super) fn emit_string_to_int_if_lossy(
    emitter: &mut Emitter,
    data: &mut DataSection,
    label_prefix: &str,
) {
    let done = format!("{label_prefix}_string_int_deprecation_done");
    let (pointer, length) = abi::string_result_regs(emitter);
    abi::emit_push_reg_pair(emitter, pointer, length);
    abi::emit_call_label(emitter, "__rt_str_numeric_union_kind");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #3");                                  // only in-range float-form strings can lose integer precision
            emitter.instruction(&format!("b.ne {done}"));                       // skip integer-form, invalid, and out-of-range strings
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 3");                                  // only in-range float-form strings can lose integer precision
            emitter.instruction(&format!("jne {done}"));                        // skip integer-form, invalid, and out-of-range strings
        }
    }
    abi::emit_load_temporary_stack_slot(emitter, pointer, 0);
    abi::emit_load_temporary_stack_slot(emitter, length, 8);
    abi::emit_call_label(emitter, "__rt_str_to_number");
    emit_branch_unless_lossy_float_to_int(emitter, &done);
    emit_fragment(emitter, data, FLOAT_STRING_PREFIX);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(emitter, "x1", 0);
            abi::emit_load_temporary_stack_slot(emitter, "x2", 8);
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(emitter, "rdi", 0);
            abi::emit_load_temporary_stack_slot(emitter, "rsi", 8);
        }
    }
    abi::emit_call_label(emitter, "__rt_diag_warning");
    emit_fragment(emitter, data, FLOAT_STRING_SUFFIX);
    emitter.label(&done);
    abi::emit_pop_reg_pair(emitter, pointer, length);
}

/// Emits lossy integer deprecations for the current boxed dynamic callback argument.
pub(super) fn emit_dynamic_to_int_if_lossy(
    emitter: &mut Emitter,
    data: &mut DataSection,
    label_prefix: &str,
    float_can_select_int: bool,
    string_can_select_int: bool,
) {
    if !float_can_select_int && !string_can_select_int {
        return;
    }
    let float_case = format!("{label_prefix}_dynamic_float_int_deprecation");
    let string_case = format!("{label_prefix}_dynamic_string_int_deprecation");
    let done = format!("{label_prefix}_dynamic_int_deprecation_done");
    let result = abi::int_result_reg(emitter);
    abi::emit_push_reg(emitter, result);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    match emitter.target.arch {
        Arch::AArch64 => {
            if float_can_select_int {
                emitter.instruction("cmp x0, #2");                              // does the dynamic source hold a PHP float?
                emitter.instruction(&format!("b.eq {float_case}"));             // inspect a possible preferred float-to-int conversion
            }
            if string_can_select_int {
                emitter.instruction("cmp x0, #1");                              // does the dynamic source hold a PHP string?
                emitter.instruction(&format!("b.eq {string_case}"));            // inspect a possible preferred float-string-to-int conversion
            }
            emitter.instruction(&format!("b {done}"));                          // other runtime tags cannot emit this deprecation
        }
        Arch::X86_64 => {
            if float_can_select_int {
                emitter.instruction("cmp rax, 2");                              // does the dynamic source hold a PHP float?
                emitter.instruction(&format!("je {float_case}"));               // inspect a possible preferred float-to-int conversion
            }
            if string_can_select_int {
                emitter.instruction("cmp rax, 1");                              // does the dynamic source hold a PHP string?
                emitter.instruction(&format!("je {string_case}"));              // inspect a possible preferred float-string-to-int conversion
            }
            emitter.instruction(&format!("jmp {done}"));                        // other runtime tags cannot emit this deprecation
        }
    }

    if float_can_select_int {
        emitter.label(&float_case);
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("fmov d0, x1");                             // move the boxed float bits into the FP result register
            }
            Arch::X86_64 => {
                emitter.instruction("movq xmm0, rdi");                          // move the boxed float bits into the FP result register
            }
        }
        emit_branch_unless_lossy_float_to_int(emitter, &done);
        emit_float_deprecation(emitter, data);
        abi::emit_jump(emitter, &done);
    }

    if string_can_select_int {
        emitter.label(&string_case);
        if emitter.target.arch == Arch::X86_64 {
            emitter.instruction("mov rsi, rdx");                                // normalize the unboxed string length to the string-result ABI
        }
        emit_string_to_int_if_lossy(
            emitter,
            data,
            &format!("{label_prefix}_dynamic"),
        );
    }
    emitter.label(&done);
    abi::emit_pop_reg(emitter, result);
}

/// Branches away unless the current finite in-range float loses precision as an integer.
fn emit_branch_unless_lossy_float_to_int(emitter: &mut Emitter, done: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("fcmp d0, d0");                                 // unordered self-comparison detects NaN
            emitter.instruction(&format!("b.vs {done}"));                       // NaN cannot weakly satisfy an integer declaration
            abi::emit_load_int_immediate(emitter, "x9", 0x43e0000000000000);
            emitter.instruction("fmov d1, x9");                                 // materialize positive 2^63 as the exclusive integer bound
            emitter.instruction("fcmp d0, d1");                                 // compare the candidate with PHP_INT_MAX plus one
            emitter.instruction(&format!("b.ge {done}"));                       // positive overflow cannot select the integer conversion
            abi::emit_load_int_immediate(
                emitter,
                "x9",
                0xc3e0000000000000u64 as i64,
            );
            emitter.instruction("fmov d1, x9");                                 // materialize negative 2^63 as the inclusive integer bound
            emitter.instruction("fcmp d0, d1");                                 // compare the candidate with PHP_INT_MIN
            emitter.instruction(&format!("b.lt {done}"));                       // negative overflow cannot select the integer conversion
            emitter.instruction("fcvtzs x9, d0");                               // truncate the accepted float exactly like PHP
            emitter.instruction("scvtf d1, x9");                                // convert the integer result back to a double
            emitter.instruction("fcmp d0, d1");                                 // test whether truncation changed the numeric value
            emitter.instruction(&format!("b.eq {done}"));                       // integer-valued floats do not emit a deprecation
        }
        Arch::X86_64 => {
            emitter.instruction("ucomisd xmm0, xmm0");                          // unordered self-comparison detects NaN
            emitter.instruction(&format!("jp {done}"));                         // NaN cannot weakly satisfy an integer declaration
            abi::emit_load_int_immediate(emitter, "r10", 0x43e0000000000000);
            emitter.instruction("movq xmm1, r10");                              // materialize positive 2^63 as the exclusive integer bound
            emitter.instruction("ucomisd xmm0, xmm1");                          // compare the candidate with PHP_INT_MAX plus one
            emitter.instruction(&format!("jae {done}"));                        // positive overflow cannot select the integer conversion
            abi::emit_load_int_immediate(
                emitter,
                "r10",
                0xc3e0000000000000u64 as i64,
            );
            emitter.instruction("movq xmm1, r10");                              // materialize negative 2^63 as the inclusive integer bound
            emitter.instruction("ucomisd xmm0, xmm1");                          // compare the candidate with PHP_INT_MIN
            emitter.instruction(&format!("jb {done}"));                         // negative overflow cannot select the integer conversion
            emitter.instruction("cvttsd2si r10, xmm0");                         // truncate the accepted float exactly like PHP
            emitter.instruction("cvtsi2sd xmm1, r10");                          // convert the integer result back to a double
            emitter.instruction("ucomisd xmm0, xmm1");                          // test whether truncation changed the numeric value
            emitter.instruction(&format!("je {done}"));                         // integer-valued floats do not emit a deprecation
        }
    }
}

/// Emits the exact dynamic float-to-int deprecation while preserving concat scratch state.
fn emit_float_deprecation(emitter: &mut Emitter, data: &mut DataSection) {
    let cursor = match emitter.target.arch {
        Arch::AArch64 => "x9",
        Arch::X86_64 => "r10",
    };
    abi::emit_load_symbol_to_reg(emitter, cursor, "_concat_off", 0);
    abi::emit_push_reg(emitter, cursor);
    abi::emit_call_label(emitter, "__rt_ftoa");
    let (pointer, length) = abi::string_result_regs(emitter);
    abi::emit_push_reg_pair(emitter, pointer, length);
    emit_fragment(emitter, data, FLOAT_PREFIX);
    match emitter.target.arch {
        Arch::AArch64 => abi::emit_pop_reg_pair(emitter, "x1", "x2"),
        Arch::X86_64 => abi::emit_pop_reg_pair(emitter, "rdi", "rsi"),
    }
    abi::emit_call_label(emitter, "__rt_diag_warning");
    emit_fragment(emitter, data, FLOAT_SUFFIX);
    abi::emit_pop_reg(emitter, cursor);
    abi::emit_store_reg_to_symbol(emitter, cursor, "_concat_off", 0);
}

/// Writes one static deprecation fragment through the shared suppression-aware channel.
fn emit_fragment(emitter: &mut Emitter, data: &mut DataSection, fragment: &[u8]) {
    let (label, length) = data.add_string(fragment);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x1", &label);
            abi::emit_load_int_immediate(emitter, "x2", length as i64);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "rdi", &label);
            abi::emit_load_int_immediate(emitter, "rsi", length as i64);
        }
    }
    abi::emit_call_label(emitter, "__rt_diag_warning");
}
