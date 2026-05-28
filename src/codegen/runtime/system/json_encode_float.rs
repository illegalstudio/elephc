//! Purpose:
//! Emits JSON float encoder runtime helper.
//! Provides the runtime assembly used by JSON builtins on the selected target.
//!
//! Called from:
//! - `crate::codegen::runtime::system` during runtime emission.
//!
//! Key details:
//! - Non-finite values must update JSON error state and interact correctly with throw/partial-output flags.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// __rt_json_encode_float: encode a float as JSON, rejecting Inf/NaN.
///
/// PHP's `json_encode` reports `JSON_ERROR_INF_OR_NAN` (and throws
/// `JsonException` when `JSON_THROW_ON_ERROR` is set) for non-finite
/// floats. Without the throw flag, this helper substitutes `0` so surrounding
/// container encoders can keep producing partial JSON; the json_encode wrapper
/// later returns `false` unless JSON_PARTIAL_OUTPUT_ON_ERROR is active. Finite
/// floats tail-call into the existing `__rt_ftoa` formatter unchanged.
///
/// Input:  ARM64 d0 / x86_64 xmm0 = float value
/// Output: x1, x2 / rax, rdx = result ptr, len (in concat_buf)
pub(crate) fn emit_json_encode_float(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: json_encode_float ---");
    emitter.label_global("__rt_json_encode_float");

    // Extract the raw bit pattern, then mask off the sign bit and shift the
    // exponent down to the low 11 bits. Inf and NaN both have exponent 0x7FF.
    emitter.instruction("fmov x9, d0");                                         // grab the float bit pattern in an integer register
    emitter.instruction("lsl x9, x9, #1");                                      // discard the sign bit by shifting it out
    emitter.instruction("lsr x9, x9, #53");                                     // shift the 11-bit exponent into the low bits
    emitter.instruction("cmp x9, #0x7FF");                                      // does the exponent equal 0x7FF (Inf or NaN)?
    emitter.instruction("b.eq __rt_json_encode_float_non_finite");              // jump to the error-handling path on Inf/NaN

    // Finite path: format via __rt_ftoa and then optionally append `.0`
    // when JSON_PRESERVE_ZERO_FRACTION is set on an integer-valued result.
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // save frame pointer and link register so we can run after __rt_ftoa
    emitter.instruction("mov x29, sp");                                         // establish a stable frame pointer
    emitter.instruction("bl __rt_ftoa");                                        // format the finite float as a decimal slice (x1=ptr, x2=len)
    emitter.instruction("b __rt_json_encode_float_post");                       // hand off to the post-formatter polish

    emitter.label("__rt_json_encode_float_non_finite");
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // save frame pointer and link register before the throw helper
    emitter.instruction("mov x29, sp");                                         // establish a stable frame pointer for the helper sequence
    emitter.instruction("mov x0, #7");                                          // JSON_ERROR_INF_OR_NAN = 7
    emitter.instruction("bl __rt_json_throw_error");                            // record the error and throw when JSON_THROW_ON_ERROR is set
    emitter.instruction("fmov d0, xzr");                                        // substitute 0 for the wrapper's partial-output path
    emitter.instruction("bl __rt_ftoa");                                        // format the substituted zero value
    // fall through to the post-formatter polish so PRESERVE_ZERO_FRACTION
    // also applies to the substituted zero result.

    emitter.label("__rt_json_encode_float_post");
    // Decide whether to append `.0`: only when JSON_PRESERVE_ZERO_FRACTION
    // is set AND the formatted slice has no '.' or 'e'/'E' marker.
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_active_flags");
    emitter.instruction("ldr x9, [x9]");                                        // load the active flag bitmask
    emitter.instruction("tst x9, #1024");                                       // is JSON_PRESERVE_ZERO_FRACTION (bit 1024) set?
    emitter.instruction("b.eq __rt_json_encode_float_done");                    // skip the tail polish when the flag is clear
    emitter.instruction("mov x9, #0");                                          // initialize the scan index over the formatted slice
    emitter.label("__rt_json_encode_float_scan");
    emitter.instruction("cmp x9, x2");                                          // have we scanned every byte of the formatted slice?
    emitter.instruction("b.ge __rt_json_encode_float_append_dot_zero");         // no fractional or exponent marker found → append `.0`
    emitter.instruction("ldrb w10, [x1, x9]");                                  // load the next byte of the formatted slice
    emitter.instruction("cmp w10, #46");                                        // is it a decimal point '.'?
    emitter.instruction("b.eq __rt_json_encode_float_done");                    // already has a fraction → no append needed
    emitter.instruction("cmp w10, #101");                                       // is it a lowercase exponent marker 'e'?
    emitter.instruction("b.eq __rt_json_encode_float_done");                    // exponential form already, leave as-is
    emitter.instruction("cmp w10, #69");                                        // is it an uppercase exponent marker 'E'?
    emitter.instruction("b.eq __rt_json_encode_float_done");                    // exponential form already, leave as-is
    emitter.instruction("add x9, x9, #1");                                      // advance the scan index
    emitter.instruction("b __rt_json_encode_float_scan");                       // continue scanning

    emitter.label("__rt_json_encode_float_append_dot_zero");
    // Append `.0` at the tail of the formatted slice. The slice already
    // lives in concat_buf at concat_off-len, so writing two bytes at the
    // current concat_off and bumping concat_off + len keeps the slice
    // contiguous.
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_concat_off");
    emitter.instruction("ldr x11, [x10]");                                      // load the current concat-buffer offset (one past the formatted slice)
    crate::codegen::abi::emit_symbol_address(emitter, "x12", "_concat_buf");
    emitter.instruction("add x12, x12, x11");                                   // compute the address of the next free byte
    emitter.instruction("mov w13, #46");                                        // ASCII '.'
    emitter.instruction("strb w13, [x12]");                                     // emit the decimal point
    emitter.instruction("mov w13, #48");                                        // ASCII '0'
    emitter.instruction("strb w13, [x12, #1]");                                 // emit the trailing zero
    emitter.instruction("add x11, x11, #2");                                    // advance the concat offset by the appended bytes
    emitter.instruction("str x11, [x10]");                                      // republish the concat-buffer offset
    emitter.instruction("add x2, x2, #2");                                      // grow the result length to cover the appended `.0`

    emitter.label("__rt_json_encode_float_done");
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore frame pointer and link register
    emitter.instruction("ret");                                                 // return the (possibly extended) formatted slice
}

/// Emits x86_64 for this module.
fn emit_x86_64(emitter: &mut Emitter) {
    //! Emits x86_64-specific runtime helper for JSON float encoding.
    //!
    //! Mirrors the ARM64 `__rt_json_encode_float` path: detects Inf/NaN,
    //! records `JSON_ERROR_INF_OR_NAN` via `__rt_json_throw_error`, substitutes
    //! zero for partial-output, formats via `__rt_ftoa`, and appends `.0` when
    //! `JSON_PRESERVE_ZERO_FRACTION` is set on an integer-valued result.
    //!
    //! Input:  x86_64 xmm0 = float value
    //! Output: rax, rdx = result ptr, len (in concat_buf)

    emitter.blank();
    emitter.comment("--- runtime: json_encode_float ---");
    emitter.label_global("__rt_json_encode_float");

    emitter.instruction("movq r10, xmm0");                                      // grab the float bit pattern in an integer register
    emitter.instruction("shl r10, 1");                                          // discard the sign bit by shifting it out
    emitter.instruction("shr r10, 53");                                         // shift the 11-bit exponent into the low bits
    emitter.instruction("cmp r10, 0x7FF");                                      // does the exponent equal 0x7FF (Inf or NaN)?
    emitter.instruction("je __rt_json_encode_float_non_finite_x");              // jump to the error-handling path on Inf/NaN

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer so we can run after __rt_ftoa
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the post-formatter polish
    emitter.instruction("call __rt_ftoa");                                      // format the finite float as a decimal slice (rax=ptr, rdx=len)
    emitter.instruction("jmp __rt_json_encode_float_post_x");                   // hand off to the post-formatter polish

    emitter.label("__rt_json_encode_float_non_finite_x");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before the throw helper
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the helper sequence
    emitter.instruction("mov rax, 7");                                          // JSON_ERROR_INF_OR_NAN = 7
    emitter.instruction("call __rt_json_throw_error");                          // record the error and throw when JSON_THROW_ON_ERROR is set
    emitter.instruction("xorpd xmm0, xmm0");                                    // substitute 0 for the wrapper's partial-output path
    emitter.instruction("call __rt_ftoa");                                      // format the substituted zero value
    // fall through to the post-formatter polish so PRESERVE_ZERO_FRACTION
    // also applies to the substituted zero result.

    emitter.label("__rt_json_encode_float_post_x");
    // Decide whether to append `.0`: only when JSON_PRESERVE_ZERO_FRACTION
    // is set AND the formatted slice has no '.' or 'e'/'E' marker.
    emitter.instruction("mov r10, QWORD PTR [rip + _json_active_flags]");       // load the active flag bitmask
    emitter.instruction("test r10, 1024");                                      // is JSON_PRESERVE_ZERO_FRACTION (bit 1024) set?
    emitter.instruction("je __rt_json_encode_float_done_x");                    // skip the tail polish when the flag is clear
    emitter.instruction("xor rcx, rcx");                                        // initialize the scan index over the formatted slice
    emitter.label("__rt_json_encode_float_scan_x");
    emitter.instruction("cmp rcx, rdx");                                        // have we scanned every byte of the formatted slice?
    emitter.instruction("jae __rt_json_encode_float_append_dot_zero_x");        // no fractional or exponent marker found → append `.0`
    emitter.instruction("movzx r9, BYTE PTR [rax + rcx]");                      // load the next byte of the formatted slice
    emitter.instruction("cmp r9, 46");                                          // is it a decimal point '.'?
    emitter.instruction("je __rt_json_encode_float_done_x");                    // already has a fraction → no append needed
    emitter.instruction("cmp r9, 101");                                         // is it a lowercase exponent marker 'e'?
    emitter.instruction("je __rt_json_encode_float_done_x");                    // exponential form already, leave as-is
    emitter.instruction("cmp r9, 69");                                          // is it an uppercase exponent marker 'E'?
    emitter.instruction("je __rt_json_encode_float_done_x");                    // exponential form already, leave as-is
    emitter.instruction("add rcx, 1");                                          // advance the scan index
    emitter.instruction("jmp __rt_json_encode_float_scan_x");                   // continue scanning

    emitter.label("__rt_json_encode_float_append_dot_zero_x");
    emitter.instruction("mov r9, QWORD PTR [rip + _concat_off]");               // load the current concat-buffer offset
    emitter.instruction("lea r10, [rip + _concat_buf]");                        // materialize the concat-buffer base
    emitter.instruction("add r10, r9");                                         // compute the address of the next free byte
    emitter.instruction("mov BYTE PTR [r10], 46");                              // emit the decimal point
    emitter.instruction("mov BYTE PTR [r10 + 1], 48");                          // emit the trailing zero
    emitter.instruction("add r9, 2");                                           // advance the concat offset by the appended bytes
    emitter.instruction("mov QWORD PTR [rip + _concat_off], r9");               // republish the concat-buffer offset
    emitter.instruction("add rdx, 2");                                          // grow the result length to cover the appended `.0`

    emitter.label("__rt_json_encode_float_done_x");
    emitter.instruction("mov rsp, rbp");                                        // unwind the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the (possibly extended) formatted slice
}
