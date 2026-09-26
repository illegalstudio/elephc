//! Purpose:
//! Converts float array keys to PHP integers and emits precision or range diagnostics.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::managed::emit_managed_runtime()`.
//!
//! Key details:
//! - Uses the shared modulo conversion and shortest float formatter on every target.
//! - NaN emits both PHP diagnostics; out-of-range finite values and infinities emit one warning.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

const PRECISION_PREFIX: &str = "Deprecated: Implicit conversion from float ";
const PRECISION_SUFFIX: &str = " to int loses precision\n";
const RANGE_PREFIX: &str = "Warning: The float ";
const RANGE_SUFFIX: &str = " is not representable as an int, cast occurred\n";

/// Emits the float-array-key conversion with PHP's warning and deprecation rules.
///
/// Accepts a double in `d0` or `xmm0` and returns its PHP integer key in `x0` or `rax`.
pub fn emit_float_key_to_int(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64(emitter);
    } else {
        emit_aarch64(emitter);
    }
}

/// Emits the AArch64 conversion, retaining the input float and result across callbacks.
fn emit_aarch64(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: float_key_to_int ---");
    e.label_global("__rt_float_key_to_int");
    e.instruction("sub sp, sp, #64");                                           // reserve input, converted key, concat cursor, and linkage
    e.instruction("stp x29, x30, [sp, #48]");                                   // preserve the caller frame and return address
    e.instruction("add x29, sp, #48");                                          // establish the helper frame
    e.instruction("str d0, [sp, #0]");                                          // retain the source float across diagnostic callbacks
    abi::emit_call_label(e, "__rt_php_float_to_int");
    e.instruction("str x9, [sp, #8]");                                          // save PHP's modulo-converted integer key
    abi::emit_symbol_address(e, "x9", "_concat_off");
    e.instruction("ldr x10, [x9]");                                             // read the caller's concat scratch cursor
    e.instruction("str x10, [sp, #16]");                                        // restore concat scratch after float formatting

    e.instruction("fcmp d0, d0");                                               // identify NaN before ordered range comparisons
    e.instruction("b.vs __rt_float_key_nan");                                   // NaN receives both warning and deprecation
    abi::emit_load_int_immediate(e, "x9", 0x43e0000000000000);
    e.instruction("fmov d1, x9");                                               // load positive 2^63, outside the signed key range
    e.instruction("fcmp d0, d1");                                               // compare the positive bound inclusively
    e.instruction("b.ge __rt_float_key_range");                                 // warn for positive overflow and infinity
    abi::emit_load_int_immediate(e, "x9", 0xc3e0000000000000_u64 as i64);
    e.instruction("fmov d1, x9");                                               // load negative 2^63, the minimum signed key
    e.instruction("fcmp d0, d1");                                               // compare the negative bound exclusively
    e.instruction("b.lt __rt_float_key_range");                                 // warn for negative overflow and infinity
    e.instruction("ldr x9, [sp, #8]");                                          // reload the PHP-converted integer key
    e.instruction("scvtf d1, x9");                                              // reconstruct an exactly representable integer float
    e.instruction("fcmp d0, d1");                                               // detect fractional precision loss
    e.instruction("b.eq __rt_float_key_done");                                  // exact integral keys stay silent
    e.instruction("b __rt_float_key_precision");                                // report the fractional key

    e.label("__rt_float_key_nan");
    emit_diagnostic_aarch64(e, "_diag_float_key_range_prefix", RANGE_PREFIX.len(), "_diag_float_key_range_suffix", RANGE_SUFFIX.len());
    e.label("__rt_float_key_precision");
    emit_diagnostic_aarch64(e, "_diag_float_key_precision_prefix", PRECISION_PREFIX.len(), "_diag_float_key_precision_suffix", PRECISION_SUFFIX.len());
    e.instruction("b __rt_float_key_done");                                     // finish after the precision deprecation
    e.label("__rt_float_key_range");
    emit_diagnostic_aarch64(e, "_diag_float_key_range_prefix", RANGE_PREFIX.len(), "_diag_float_key_range_suffix", RANGE_SUFFIX.len());
    e.label("__rt_float_key_done");
    e.instruction("ldr x0, [sp, #8]");                                          // return the original PHP-converted integer key
    e.instruction("ldp x29, x30, [sp, #48]");                                   // restore caller linkage
    e.instruction("add sp, sp, #64");                                           // release the helper frame
    e.instruction("ret");                                                       // return to the array-key caller
}

/// Appends one complete AArch64 float-key diagnostic through the shared dispatcher.
fn emit_diagnostic_aarch64(e: &mut Emitter, prefix: &str, prefix_len: usize, suffix: &str, suffix_len: usize) {
    abi::emit_symbol_address(e, "x1", prefix);
    abi::emit_load_int_immediate(e, "x2", prefix_len as i64);
    abi::emit_call_label(e, "__rt_diag_warning_fragment");
    e.instruction("ldr d0, [sp, #0]");                                          // reload the original float for exact display
    abi::emit_call_label(e, "__rt_ftoa_repr");
    abi::emit_call_label(e, "__rt_diag_warning_fragment");
    e.instruction("ldr x10, [sp, #16]");                                        // reload the caller's concat cursor
    abi::emit_symbol_address(e, "x9", "_concat_off");
    e.instruction("str x10, [x9]");                                             // release formatter scratch before invoking the handler
    abi::emit_symbol_address(e, "x1", suffix);
    abi::emit_load_int_immediate(e, "x2", suffix_len as i64);
    abi::emit_call_label(e, "__rt_diag_warning");
}

/// Emits the x86_64 conversion with the same signed range and precision checks.
fn emit_x86_64(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: float_key_to_int ---");
    e.label_global("__rt_float_key_to_int");
    e.instruction("push rbp");                                                  // preserve the caller frame pointer
    e.instruction("mov rbp, rsp");                                              // establish the helper frame
    e.instruction("sub rsp, 48");                                               // reserve aligned input, key, and concat slots
    e.instruction("movsd QWORD PTR [rbp - 8], xmm0");                           // retain the source float across diagnostic callbacks
    abi::emit_call_label(e, "__rt_php_float_to_int");
    e.instruction("mov QWORD PTR [rbp - 16], r11");                             // save PHP's modulo-converted integer key
    abi::emit_load_symbol_to_reg(e, "r10", "_concat_off", 0);
    e.instruction("mov QWORD PTR [rbp - 24], r10");                             // restore concat scratch after formatting

    e.instruction("ucomisd xmm0, xmm0");                                        // identify NaN before ordered range comparisons
    e.instruction("jp __rt_float_key_nan");                                     // NaN receives both warning and deprecation
    abi::emit_load_int_immediate(e, "r10", 0x43e0000000000000);
    e.instruction("movq xmm1, r10");                                            // load positive 2^63, outside the signed key range
    e.instruction("ucomisd xmm0, xmm1");                                        // compare the positive bound inclusively
    e.instruction("jae __rt_float_key_range");                                  // warn for positive overflow and infinity
    abi::emit_load_int_immediate(e, "r10", 0xc3e0000000000000_u64 as i64);
    e.instruction("movq xmm1, r10");                                            // load negative 2^63, the minimum signed key
    e.instruction("ucomisd xmm0, xmm1");                                        // compare the negative bound exclusively
    e.instruction("jb __rt_float_key_range");                                   // warn for negative overflow and infinity
    e.instruction("mov r11, QWORD PTR [rbp - 16]");                             // reload the PHP-converted integer key
    e.instruction("cvtsi2sd xmm1, r11");                                        // reconstruct an exactly representable integer float
    e.instruction("ucomisd xmm0, xmm1");                                        // detect fractional precision loss
    e.instruction("je __rt_float_key_done");                                    // exact integral keys stay silent
    e.instruction("jmp __rt_float_key_precision");                              // report the fractional key

    e.label("__rt_float_key_nan");
    emit_diagnostic_x86_64(e, "_diag_float_key_range_prefix", RANGE_PREFIX.len(), "_diag_float_key_range_suffix", RANGE_SUFFIX.len());
    e.label("__rt_float_key_precision");
    emit_diagnostic_x86_64(e, "_diag_float_key_precision_prefix", PRECISION_PREFIX.len(), "_diag_float_key_precision_suffix", PRECISION_SUFFIX.len());
    e.instruction("jmp __rt_float_key_done");                                   // finish after the precision deprecation
    e.label("__rt_float_key_range");
    emit_diagnostic_x86_64(e, "_diag_float_key_range_prefix", RANGE_PREFIX.len(), "_diag_float_key_range_suffix", RANGE_SUFFIX.len());
    e.label("__rt_float_key_done");
    e.instruction("mov rax, QWORD PTR [rbp - 16]");                             // return the original PHP-converted integer key
    e.instruction("mov rsp, rbp");                                              // release the helper frame
    e.instruction("pop rbp");                                                   // restore the caller frame pointer
    e.instruction("ret");                                                       // return to the array-key caller
}

/// Appends one complete x86_64 float-key diagnostic through the shared dispatcher.
fn emit_diagnostic_x86_64(e: &mut Emitter, prefix: &str, prefix_len: usize, suffix: &str, suffix_len: usize) {
    abi::emit_symbol_address(e, "rdi", prefix);
    abi::emit_load_int_immediate(e, "rsi", prefix_len as i64);
    abi::emit_call_label(e, "__rt_diag_warning_fragment");
    e.instruction("movsd xmm0, QWORD PTR [rbp - 8]");                           // reload the original float for exact display
    abi::emit_call_label(e, "__rt_ftoa_repr");
    e.instruction("mov rdi, rax");                                              // pass the formatted float pointer to the dispatcher
    e.instruction("mov rsi, rdx");                                              // pass the formatted float length to the dispatcher
    abi::emit_call_label(e, "__rt_diag_warning_fragment");
    e.instruction("mov r10, QWORD PTR [rbp - 24]");                             // reload the caller's concat cursor
    abi::emit_store_reg_to_symbol(e, "r10", "_concat_off", 0);
    abi::emit_symbol_address(e, "rdi", suffix);
    abi::emit_load_int_immediate(e, "rsi", suffix_len as i64);
    abi::emit_call_label(e, "__rt_diag_warning");
}
