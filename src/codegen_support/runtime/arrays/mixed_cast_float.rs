//! Purpose:
//! Emits the `__rt_mixed_cast_float`, `__rt_mixed_unbox` runtime helper assembly for mixed cast float.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Mixed helpers use boxed tag/payload cells; tag constants and ownership rules are shared with type checking and codegen.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::{abi, platform::Arch};

/// Emits `__rt_mixed_cast_float` which casts a boxed PhpMixed cell to a float.
/// Uses the PhpMixed runtime tag to dispatch to per-type conversion paths:
/// - Tag 0 (int): widens integer in x1 to float via `emit_int_result_to_float_result`
/// - Tag 1 (string): parses the string through `__rt_str_to_number` (PHP's numeric-string
///   grammar: leading numeric run, no hex/`INF`/`NAN`), matching the direct `(float)` cast
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
    emitter.instruction("b.eq __rt_mixed_cast_float_from_string");              // strings parse through the PHP numeric-string grammar scanner
    emitter.instruction("cmp x0, #2");                                          // does the mixed payload already hold a float?
    emitter.instruction("b.eq __rt_mixed_cast_float_from_float");               // floats reuse their stored payload directly
    emitter.instruction("cmp x0, #3");                                          // does the mixed payload hold a bool?
    emitter.instruction("b.eq __rt_mixed_cast_float_from_bool");                // bools widen from their 0/1 payloads
    emitter.instruction("cmp x0, #6");                                          // does the mixed payload hold an object?
    emitter.instruction("b.eq __rt_mixed_cast_float_from_object");              // objects warn and cast to 1.0 like PHP
    emitter.instruction("mov x0, #0");                                          // null and unsupported payloads cast to 0.0 for now
    abi::emit_int_result_to_float_result(emitter);                              // convert the normalized zero integer payload into the floating-point result register
    emitter.instruction("b __rt_mixed_cast_float_done");                        // return the normalized 0.0 result

    emitter.label("__rt_mixed_cast_float_from_int");
    emitter.instruction("mov x0, x1");                                          // move the unboxed integer payload into the canonical integer result register
    abi::emit_int_result_to_float_result(emitter);                              // widen the integer payload into the floating-point result register
    emitter.instruction("b __rt_mixed_cast_float_done");                        // return the converted integer payload

    emitter.label("__rt_mixed_cast_float_from_string");
    emitter.instruction("bl __rt_str_to_number");                               // parse the string with PHP's numeric-string grammar (clips to the leading numeric run)
    emitter.instruction("b __rt_mixed_cast_float_done");                        // return the parsed floating-point string payload in d0

    emitter.label("__rt_mixed_cast_float_from_float");
    emitter.instruction("fmov d0, x1");                                         // move the unboxed float bits into the floating-point result register
    emitter.instruction("b __rt_mixed_cast_float_done");                        // return the unboxed float payload directly

    emitter.label("__rt_mixed_cast_float_from_bool");
    emitter.instruction("mov x0, x1");                                          // move the unboxed bool payload into the canonical integer result register
    abi::emit_int_result_to_float_result(emitter);                              // widen the 0/1 bool payload into the floating-point result register
    emitter.instruction("b __rt_mixed_cast_float_done");                        // return the converted bool payload

    emitter.label("__rt_mixed_cast_float_from_object");
    emitter.instruction("mov x0, x1");                                          // pass the object payload to the warning helper
    emitter.instruction("mov x1, #1");                                          // select the float warning wording
    emitter.instruction("mov x2, #0");                                          // no eval context is available from this cast helper
    emitter.instruction("mov x3, #0");                                          // native object, not a callable descriptor
    emitter.instruction("bl __rt_sprintf_warn_object_numeric");                 // emit PHP's object-to-float warning
    emitter.instruction("mov x0, #1");                                          // PHP casts a non-numeric object to 1.0
    abi::emit_int_result_to_float_result(emitter);                              // widen the normalized integer payload

    emitter.label("__rt_mixed_cast_float_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper stack frame
    emitter.instruction("ret");                                                 // return the floating-point cast result in d0
}

/// x86_64 Linux variant of `emit_mixed_cast_float`:
/// - Input: rax = boxed mixed pointer. The cell is forwarded untouched to
///   `__rt_mixed_unbox`, whose x86_64 input register is `rax`, so `rdi` is NOT the input.
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
    emitter.instruction("je __rt_mixed_cast_float_from_string_linux_x86_64");   // strings parse through the PHP numeric-string grammar scanner
    emitter.instruction("cmp rax, 2");                                          // does the mixed payload already hold a float?
    emitter.instruction("je __rt_mixed_cast_float_from_float_linux_x86_64");    // floats reuse their stored payload directly
    emitter.instruction("cmp rax, 3");                                          // does the mixed payload hold a bool?
    emitter.instruction("je __rt_mixed_cast_float_from_bool_linux_x86_64");     // bools widen from their 0/1 payloads
    emitter.instruction("cmp rax, 6");                                          // does the mixed payload hold an object?
    emitter.instruction("je __rt_mixed_cast_float_from_object_linux_x86_64");   // objects warn and cast to 1.0 like PHP
    emitter.instruction("xor rax, rax");                                        // null and unsupported payloads cast to 0 before widening to 0.0
    abi::emit_int_result_to_float_result(emitter);                              // convert the normalized zero integer payload into the floating-point result register
    emitter.instruction("jmp __rt_mixed_cast_float_done_linux_x86_64");         // return the normalized 0.0 result

    emitter.label("__rt_mixed_cast_float_from_int_linux_x86_64");
    emitter.instruction("mov rax, rdi");                                        // move the unboxed integer payload into the canonical integer result register
    abi::emit_int_result_to_float_result(emitter);                              // widen the integer payload into the floating-point result register
    emitter.instruction("jmp __rt_mixed_cast_float_done_linux_x86_64");         // return the converted integer payload

    emitter.label("__rt_mixed_cast_float_from_string_linux_x86_64");
    emitter.instruction("mov rax, rdi");                                        // move the unboxed string pointer into the x86_64 string-result pointer register
    abi::emit_call_label(emitter, "__rt_str_to_number");                        // parse the string with PHP's numeric-string grammar (clips to the leading numeric run)
    emitter.instruction("jmp __rt_mixed_cast_float_done_linux_x86_64");         // return the parsed floating-point string payload in xmm0

    emitter.label("__rt_mixed_cast_float_from_float_linux_x86_64");
    emitter.instruction("movq xmm0, rdi");                                      // move the unboxed float bits into the floating-point result register
    emitter.instruction("jmp __rt_mixed_cast_float_done_linux_x86_64");         // return the unboxed float payload directly

    emitter.label("__rt_mixed_cast_float_from_bool_linux_x86_64");
    emitter.instruction("mov rax, rdi");                                        // move the unboxed bool payload into the canonical integer result register
    abi::emit_int_result_to_float_result(emitter);                              // widen the 0/1 bool payload into the floating-point result register
    emitter.instruction("jmp __rt_mixed_cast_float_done_linux_x86_64");         // return the converted bool payload

    emitter.label("__rt_mixed_cast_float_from_object_linux_x86_64");
    emitter.instruction("mov rdi, rdi");                                        // object payload is already in the first argument register
    emitter.instruction("mov esi, 1");                                          // select the float warning wording
    emitter.instruction("xor edx, edx");                                        // no eval context is available from this cast helper
    emitter.instruction("xor ecx, ecx");                                        // native object, not a callable descriptor
    abi::emit_call_label(emitter, "__rt_sprintf_warn_object_numeric_x64");      // emit PHP's object-to-float warning
    emitter.instruction("mov rax, 1");                                          // PHP casts a non-numeric object to 1.0
    abi::emit_int_result_to_float_result(emitter);                              // widen the normalized integer payload

    emitter.label("__rt_mixed_cast_float_done_linux_x86_64");
    emitter.instruction("add rsp, 16");                                         // release the aligned temporary slot reserved for nested calls
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return the floating-point cast result in xmm0
}