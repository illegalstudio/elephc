use crate::codegen::emit::Emitter;
use crate::codegen::{abi, platform::Arch};

/// mixed_cast_int: cast a boxed mixed payload to int using the current scalar rules.
/// Input:  x0 = boxed mixed pointer
/// Output: x0 = integer result
pub fn emit_mixed_cast_int(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_cast_int_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: mixed_cast_int ---");
    emitter.label_global("__rt_mixed_cast_int");

    emitter.instruction("sub sp, sp, #32");                                     // allocate a small stack frame for nested helper calls
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish the helper stack frame
    emitter.instruction("bl __rt_mixed_unbox");                                 // x0=tag, x1=value_lo, x2=value_hi for the boxed payload
    emitter.instruction("cmp x0, #0");                                          // does the mixed payload already hold an int?
    emitter.instruction("b.eq __rt_mixed_cast_int_from_int");                   // ints reuse their stored payload directly
    emitter.instruction("cmp x0, #1");                                          // does the mixed payload hold a string?
    emitter.instruction("b.eq __rt_mixed_cast_int_from_string");                // strings cast through the runtime atoi helper
    emitter.instruction("cmp x0, #2");                                          // does the mixed payload hold a float?
    emitter.instruction("b.eq __rt_mixed_cast_int_from_float");                 // floats cast by truncating toward zero
    emitter.instruction("cmp x0, #3");                                          // does the mixed payload hold a bool?
    emitter.instruction("b.eq __rt_mixed_cast_int_from_bool");                  // bools reuse their 0/1 payload directly
    emitter.instruction("cmp x0, #4");                                          // does the mixed payload hold an indexed array?
    emitter.instruction("b.eq __rt_mixed_cast_int_from_array");                 // arrays cast to their current element count
    emitter.instruction("cmp x0, #5");                                          // does the mixed payload hold an associative array?
    emitter.instruction("b.eq __rt_mixed_cast_int_from_array");                 // hashes cast to their current element count
    emitter.instruction("mov x0, #0");                                          // null and unsupported payloads cast to zero for now
    emitter.instruction("b __rt_mixed_cast_int_done");                          // return the normalized integer result

    emitter.label("__rt_mixed_cast_int_from_int");
    emitter.instruction("mov x0, x1");                                          // forward the stored integer payload directly
    emitter.instruction("b __rt_mixed_cast_int_done");                          // return the unboxed integer payload

    emitter.label("__rt_mixed_cast_int_from_string");
    emitter.instruction("bl __rt_atoi");                                        // parse the unboxed string payload as an integer
    emitter.instruction("b __rt_mixed_cast_int_done");                          // return the parsed integer result

    emitter.label("__rt_mixed_cast_int_from_float");
    emitter.instruction("fmov d0, x1");                                         // move the unboxed float bits into the FP register file
    emitter.instruction("fcvtzs x0, d0");                                       // truncate the float payload toward zero
    emitter.instruction("b __rt_mixed_cast_int_done");                          // return the converted integer result

    emitter.label("__rt_mixed_cast_int_from_bool");
    emitter.instruction("mov x0, x1");                                          // bool payloads are already normalized to 0 or 1
    emitter.instruction("b __rt_mixed_cast_int_done");                          // return the bool-as-int result

    emitter.label("__rt_mixed_cast_int_from_array");
    emitter.instruction("cbz x1, __rt_mixed_cast_int_zero");                    // null container pointers cast like empty containers
    emitter.instruction("ldr x0, [x1]");                                        // load the current container element count from the header
    emitter.instruction("b __rt_mixed_cast_int_done");                          // return the container size as the cast result

    emitter.label("__rt_mixed_cast_int_zero");
    emitter.instruction("mov x0, #0");                                          // null containers cast to zero

    emitter.label("__rt_mixed_cast_int_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper stack frame
    emitter.instruction("ret");                                                 // return the integer cast result in x0
}

fn emit_mixed_cast_int_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_cast_int ---");
    emitter.label_global("__rt_mixed_cast_int");

    emitter.instruction("push rbp");                                            // save the caller frame pointer before this helper allocates its own frame
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame pointer for the helper body
    emitter.instruction("sub rsp, 16");                                         // reserve one aligned temporary slot so nested helper calls keep the SysV stack aligned
    abi::emit_call_label(emitter, "__rt_mixed_unbox");                          // return the mixed runtime tag in rax and payload words in rdi/rdx for the boxed value
    emitter.instruction("cmp rax, 0");                                          // does the mixed payload already hold an int?
    emitter.instruction("je __rt_mixed_cast_int_from_int_linux_x86_64");        // ints reuse their stored payload directly
    emitter.instruction("cmp rax, 1");                                          // does the mixed payload hold a string?
    emitter.instruction("je __rt_mixed_cast_int_from_string_linux_x86_64");     // strings cast through the runtime atoi helper
    emitter.instruction("cmp rax, 2");                                          // does the mixed payload hold a float?
    emitter.instruction("je __rt_mixed_cast_int_from_float_linux_x86_64");      // floats cast by truncating toward zero
    emitter.instruction("cmp rax, 3");                                          // does the mixed payload hold a bool?
    emitter.instruction("je __rt_mixed_cast_int_from_bool_linux_x86_64");       // bools reuse their 0/1 payload directly
    emitter.instruction("cmp rax, 4");                                          // does the mixed payload hold an indexed array?
    emitter.instruction("je __rt_mixed_cast_int_from_array_linux_x86_64");      // arrays cast to their current element count
    emitter.instruction("cmp rax, 5");                                          // does the mixed payload hold an associative array?
    emitter.instruction("je __rt_mixed_cast_int_from_array_linux_x86_64");      // hashes cast to their current element count
    emitter.instruction("mov rax, 0");                                          // null and unsupported payloads cast to zero for now
    emitter.instruction("jmp __rt_mixed_cast_int_done_linux_x86_64");           // return the normalized integer result

    emitter.label("__rt_mixed_cast_int_from_int_linux_x86_64");
    emitter.instruction("mov rax, rdi");                                        // forward the stored integer payload directly
    emitter.instruction("jmp __rt_mixed_cast_int_done_linux_x86_64");           // return the unboxed integer payload

    emitter.label("__rt_mixed_cast_int_from_string_linux_x86_64");
    emitter.instruction("mov rax, rdi");                                        // move the unboxed string pointer into the standard x86_64 string result register
    abi::emit_call_label(emitter, "__rt_atoi");                                 // parse the unboxed string payload as an integer
    emitter.instruction("jmp __rt_mixed_cast_int_done_linux_x86_64");           // return the parsed integer result

    emitter.label("__rt_mixed_cast_int_from_float_linux_x86_64");
    emitter.instruction("movq xmm0, rdi");                                      // move the unboxed float bits into the floating-point result register
    emitter.instruction("cvttsd2si rax, xmm0");                                 // truncate the floating-point payload toward zero
    emitter.instruction("jmp __rt_mixed_cast_int_done_linux_x86_64");           // return the converted integer result

    emitter.label("__rt_mixed_cast_int_from_bool_linux_x86_64");
    emitter.instruction("mov rax, rdi");                                        // bool payloads are already normalized to 0 or 1
    emitter.instruction("jmp __rt_mixed_cast_int_done_linux_x86_64");           // return the bool-as-int result

    emitter.label("__rt_mixed_cast_int_from_array_linux_x86_64");
    emitter.instruction("test rdi, rdi");                                       // null container pointers cast like empty containers
    emitter.instruction("je __rt_mixed_cast_int_zero_linux_x86_64");            // skip the header load when the container pointer is null
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // load the current container element count from the header
    emitter.instruction("jmp __rt_mixed_cast_int_done_linux_x86_64");           // return the container size as the cast result

    emitter.label("__rt_mixed_cast_int_zero_linux_x86_64");
    emitter.instruction("mov rax, 0");                                          // null containers cast to zero

    emitter.label("__rt_mixed_cast_int_done_linux_x86_64");
    emitter.instruction("add rsp, 16");                                         // release the aligned temporary slot reserved for nested helper calls
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return the integer cast result in rax
}
