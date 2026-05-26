//! Purpose:
//! Emits the `__rt_mixed_cast_float`, `__rt_mixed_unbox` runtime helper assembly for mixed cast float.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - Mixed helpers use boxed tag/payload cells; tag constants and ownership rules are shared with type checking and codegen.

use crate::codegen::emit::Emitter;
use crate::codegen::{abi, platform::Arch};

/// Emits `__rt_mixed_cast_float` which casts a boxed PhpMixed cell to a float.
/// Uses the PhpMixed runtime tag to dispatch to per-type conversion paths:
/// - Tag 0 (int): widens integer in x1 to float via `emit_int_result_to_float_result`
/// - Tag 1 (string): calls `__rt_cstr` then `atof` to parse the string as a double
/// - Tag 2 (float): moves float bits from x1 directly into d0
/// - Tag 3 (bool): widens 0/1 bool in x1 to float
/// - Tag >= 4 (null, unsupported): returns 0.0
///
/// ARM64 ABI: input in x0 (boxed pointer), output in d0 (float result). Uses a
/// 32-byte stack frame for nested calls. Does not clobber callee-saved registers.
pub fn emit_mixed_cast_float(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_cast_float_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: mixed_cast_float ---");
    emitter.label_global("__rt_mixed_cast_float");

    emitter.instruction("sub sp, sp, #32");                                     // allocate a small stack frame for nested helper calls
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish the helper stack frame
    emitter.instruction("bl __rt_mixed_unbox");                                 // x0=tag, x1=value_lo, x2=value_hi for the boxed payload
    emitter.instruction("cmp x0, #0");                                          // does the mixed payload already hold an int?
    emitter.instruction("b.eq __rt_mixed_cast_float_from_int");                 // ints widen directly into the floating-point result register
    emitter.instruction("cmp x0, #1");                                          // does the mixed payload hold a string?
    emitter.instruction("b.eq __rt_mixed_cast_float_from_string");              // strings cast through the runtime C-string bridge plus atof()
    emitter.instruction("cmp x0, #2");                                          // does the mixed payload already hold a float?
    emitter.instruction("b.eq __rt_mixed_cast_float_from_float");               // floats reuse their stored payload directly
    emitter.instruction("cmp x0, #3");                                          // does the mixed payload hold a bool?
    emitter.instruction("b.eq __rt_mixed_cast_float_from_bool");                // bools widen from their 0/1 payloads
    emitter.instruction("mov x0, #0");                                          // null and unsupported payloads cast to 0.0 for now
    abi::emit_int_result_to_float_result(emitter);                              // convert the normalized zero integer payload into the floating-point result register
    emitter.instruction("b __rt_mixed_cast_float_done");                        // return the normalized 0.0 result

    emitter.label("__rt_mixed_cast_float_from_int");
    emitter.instruction("mov x0, x1");                                          // move the unboxed integer payload into the canonical integer result register
    abi::emit_int_result_to_float_result(emitter);                              // widen the integer payload into the floating-point result register
    emitter.instruction("b __rt_mixed_cast_float_done");                        // return the converted integer payload

    emitter.label("__rt_mixed_cast_float_from_string");
    emitter.instruction("bl __rt_cstr");                                        // materialize a null-terminated copy of the unboxed elefant string payload
    emitter.bl_c("atof");                                                       // parse the current C string payload as double
    emitter.instruction("b __rt_mixed_cast_float_done");                        // return the parsed floating-point string payload

    emitter.label("__rt_mixed_cast_float_from_float");
    emitter.instruction("fmov d0, x1");                                         // move the unboxed float bits into the floating-point result register
    emitter.instruction("b __rt_mixed_cast_float_done");                        // return the unboxed float payload directly

    emitter.label("__rt_mixed_cast_float_from_bool");
    emitter.instruction("mov x0, x1");                                          // move the unboxed bool payload into the canonical integer result register
    abi::emit_int_result_to_float_result(emitter);                              // widen the 0/1 bool payload into the floating-point result register

    emitter.label("__rt_mixed_cast_float_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper stack frame
    emitter.instruction("ret");                                                 // return the floating-point cast result in d0
}

/// x86_64 Linux SysV ABI variant of `emit_mixed_cast_float`. Uses the SysV calling
/// convention:
/// - Input: rdi = boxed mixed pointer
/// - Tag returned in rax, payload words in rdi/rdx after `__rt_mixed_unbox`
/// - Float result returned in xmm0
/// - Stack kept 16-byte aligned; one 16-byte scratch slot reserved for nested calls.
fn emit_mixed_cast_float_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_cast_float ---");
    emitter.label_global("__rt_mixed_cast_float");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while this helper uses nested calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the helper body
    emitter.instruction("sub rsp, 16");                                         // reserve one aligned temporary slot so nested helper calls keep the SysV stack aligned
    abi::emit_call_label(emitter, "__rt_mixed_unbox");                          // return the mixed runtime tag in rax and payload words in rdi/rdx for the boxed value
    emitter.instruction("cmp rax, 0");                                          // does the mixed payload already hold an int?
    emitter.instruction("je __rt_mixed_cast_float_from_int_linux_x86_64");      // ints widen directly into the floating-point result register
    emitter.instruction("cmp rax, 1");                                          // does the mixed payload hold a string?
    emitter.instruction("je __rt_mixed_cast_float_from_string_linux_x86_64");   // strings cast through the runtime C-string bridge plus atof()
    emitter.instruction("cmp rax, 2");                                          // does the mixed payload already hold a float?
    emitter.instruction("je __rt_mixed_cast_float_from_float_linux_x86_64");    // floats reuse their stored payload directly
    emitter.instruction("cmp rax, 3");                                          // does the mixed payload hold a bool?
    emitter.instruction("je __rt_mixed_cast_float_from_bool_linux_x86_64");     // bools widen from their 0/1 payloads
    emitter.instruction("xor rax, rax");                                        // null and unsupported payloads cast to 0 before widening to 0.0
    abi::emit_int_result_to_float_result(emitter);                              // convert the normalized zero integer payload into the floating-point result register
    emitter.instruction("jmp __rt_mixed_cast_float_done_linux_x86_64");         // return the normalized 0.0 result

    emitter.label("__rt_mixed_cast_float_from_int_linux_x86_64");
    emitter.instruction("mov rax, rdi");                                        // move the unboxed integer payload into the canonical integer result register
    abi::emit_int_result_to_float_result(emitter);                              // widen the integer payload into the floating-point result register
    emitter.instruction("jmp __rt_mixed_cast_float_done_linux_x86_64");         // return the converted integer payload

    emitter.label("__rt_mixed_cast_float_from_string_linux_x86_64");
    emitter.instruction("mov rax, rdi");                                        // move the unboxed string pointer into the x86_64 string-result pointer register
    abi::emit_call_label(emitter, "__rt_cstr");                                 // materialize a null-terminated copy of the unboxed elefant string payload
    emitter.instruction("mov rdi, rax");                                        // pass the temporary C string through the SysV first integer argument register before atof()
    emitter.instruction("call atof");                                           // parse the current C string payload as double
    emitter.instruction("jmp __rt_mixed_cast_float_done_linux_x86_64");         // return the parsed floating-point string payload

    emitter.label("__rt_mixed_cast_float_from_float_linux_x86_64");
    emitter.instruction("movq xmm0, rdi");                                      // move the unboxed float bits into the floating-point result register
    emitter.instruction("jmp __rt_mixed_cast_float_done_linux_x86_64");         // return the unboxed float payload directly

    emitter.label("__rt_mixed_cast_float_from_bool_linux_x86_64");
    emitter.instruction("mov rax, rdi");                                        // move the unboxed bool payload into the canonical integer result register
    abi::emit_int_result_to_float_result(emitter);                              // widen the 0/1 bool payload into the floating-point result register

    emitter.label("__rt_mixed_cast_float_done_linux_x86_64");
    emitter.instruction("add rsp, 16");                                         // release the aligned temporary slot reserved for nested calls
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return the floating-point cast result in xmm0
}