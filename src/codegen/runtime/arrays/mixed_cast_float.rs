use crate::codegen::emit::Emitter;
use crate::codegen::{abi, platform::Arch};

/// mixed_cast_float: cast a boxed mixed payload to float using the current scalar rules.
/// Input:  x0 = boxed mixed pointer
/// Output: d0 = floating-point result
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
    emitter.instruction("bl __rt_cstr");                                        // materialize a null-terminated copy of the unboxed elephc string payload
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
    abi::emit_call_label(emitter, "__rt_cstr");                                 // materialize a null-terminated copy of the unboxed elephc string payload
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
    emitter.instruction("add rsp, 16");                                         // release the aligned temporary slot reserved for nested helper calls
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return the floating-point cast result in xmm0
}
