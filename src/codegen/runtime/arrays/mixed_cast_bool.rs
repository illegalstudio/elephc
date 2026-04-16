use crate::codegen::abi;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// mixed_cast_bool: cast a boxed mixed payload to bool using the current scalar rules.
/// Input:  x0 = boxed mixed pointer
/// Output: x0 = boolean result
pub fn emit_mixed_cast_bool(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_cast_bool_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: mixed_cast_bool ---");
    emitter.label_global("__rt_mixed_cast_bool");

    emitter.instruction("sub sp, sp, #32");                                     // allocate a small stack frame for nested helper calls
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish the helper stack frame
    emitter.instruction("bl __rt_mixed_unbox");                                 // x0=tag, x1=value_lo, x2=value_hi for the boxed payload
    emitter.instruction("cmp x0, #0");                                          // does the mixed payload hold an int?
    emitter.instruction("b.eq __rt_mixed_cast_bool_from_int");                  // ints use zero/nonzero truthiness
    emitter.instruction("cmp x0, #1");                                          // does the mixed payload hold a string?
    emitter.instruction("b.eq __rt_mixed_cast_bool_from_string");               // strings use empty/non-empty truthiness
    emitter.instruction("cmp x0, #2");                                          // does the mixed payload hold a float?
    emitter.instruction("b.eq __rt_mixed_cast_bool_from_float");                // floats use 0.0/non-zero truthiness
    emitter.instruction("cmp x0, #3");                                          // does the mixed payload hold a bool?
    emitter.instruction("b.eq __rt_mixed_cast_bool_from_bool");                 // bools reuse their stored payload directly
    emitter.instruction("cmp x0, #4");                                          // does the mixed payload hold an indexed array?
    emitter.instruction("b.eq __rt_mixed_cast_bool_from_array");                // arrays are truthy when non-empty
    emitter.instruction("cmp x0, #5");                                          // does the mixed payload hold an associative array?
    emitter.instruction("b.eq __rt_mixed_cast_bool_from_array");                // hashes are truthy when non-empty
    emitter.instruction("mov x0, #0");                                          // null and unsupported payloads are falsy for now
    emitter.instruction("b __rt_mixed_cast_bool_done");                         // return the normalized boolean result

    emitter.label("__rt_mixed_cast_bool_from_int");
    emitter.instruction("cmp x1, #0");                                          // compare the integer payload against zero
    emitter.instruction("cset x0, ne");                                         // integers are truthy when non-zero
    emitter.instruction("b __rt_mixed_cast_bool_done");                         // return the integer truthiness result

    emitter.label("__rt_mixed_cast_bool_from_string");
    emitter.instruction("cbz x2, __rt_mixed_cast_bool_done");                   // empty strings are falsy
    emitter.instruction("cmp x2, #1");                                          // check whether the string length is exactly one byte
    emitter.instruction("b.ne __rt_mixed_cast_bool_string_truthy");             // strings longer than one byte are truthy
    emitter.instruction("ldrb w9, [x1]");                                       // load the first byte of the string payload
    emitter.instruction("cmp w9, #48");                                         // compare against ASCII '0'
    emitter.instruction("cset x0, ne");                                         // the one-byte string \"0\" is falsy, everything else is truthy
    emitter.instruction("b __rt_mixed_cast_bool_done");                         // return the string truthiness result
    emitter.label("__rt_mixed_cast_bool_string_truthy");
    emitter.instruction("mov x0, #1");                                          // non-empty strings other than \"0\" are truthy
    emitter.instruction("b __rt_mixed_cast_bool_done");                         // return the string truthiness result

    emitter.label("__rt_mixed_cast_bool_from_float");
    emitter.instruction("fmov d0, x1");                                         // move the unboxed float bits into the FP register file
    emitter.instruction("fcmp d0, #0.0");                                       // compare the float payload against zero
    emitter.instruction("cset x0, ne");                                         // floats are truthy when non-zero
    emitter.instruction("b __rt_mixed_cast_bool_done");                         // return the float truthiness result

    emitter.label("__rt_mixed_cast_bool_from_bool");
    emitter.instruction("mov x0, x1");                                          // bool payloads are already normalized to 0 or 1
    emitter.instruction("b __rt_mixed_cast_bool_done");                         // return the bool payload directly

    emitter.label("__rt_mixed_cast_bool_from_array");
    emitter.instruction("cbz x1, __rt_mixed_cast_bool_done");                   // null containers stay falsy
    emitter.instruction("ldr x0, [x1]");                                        // load the current container element count from the header
    emitter.instruction("cmp x0, #0");                                          // compare the element count against zero
    emitter.instruction("cset x0, ne");                                         // containers are truthy when non-empty

    emitter.label("__rt_mixed_cast_bool_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper stack frame
    emitter.instruction("ret");                                                 // return the boolean cast result in x0
}

fn emit_mixed_cast_bool_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_cast_bool ---");
    emitter.label_global("__rt_mixed_cast_bool");

    emitter.instruction("push rbp");                                            // save the caller frame pointer before this helper allocates its own frame
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame pointer for the helper body
    emitter.instruction("sub rsp, 16");                                         // reserve one aligned temporary slot so nested helper calls keep the SysV stack aligned
    abi::emit_call_label(emitter, "__rt_mixed_unbox");                          // return the mixed runtime tag in rax and payload words in rdi/rdx for the boxed value
    emitter.instruction("cmp rax, 0");                                          // does the mixed payload hold an int?
    emitter.instruction("je __rt_mixed_cast_bool_from_int_linux_x86_64");       // ints use zero/nonzero truthiness
    emitter.instruction("cmp rax, 1");                                          // does the mixed payload hold a string?
    emitter.instruction("je __rt_mixed_cast_bool_from_string_linux_x86_64");    // strings use PHP's empty/non-empty truthiness rule
    emitter.instruction("cmp rax, 2");                                          // does the mixed payload hold a float?
    emitter.instruction("je __rt_mixed_cast_bool_from_float_linux_x86_64");     // floats are truthy when non-zero
    emitter.instruction("cmp rax, 3");                                          // does the mixed payload hold a bool?
    emitter.instruction("je __rt_mixed_cast_bool_from_bool_linux_x86_64");      // bools reuse their stored payload directly
    emitter.instruction("cmp rax, 4");                                          // does the mixed payload hold an indexed array?
    emitter.instruction("je __rt_mixed_cast_bool_from_array_linux_x86_64");     // arrays are truthy when non-empty
    emitter.instruction("cmp rax, 5");                                          // does the mixed payload hold an associative array?
    emitter.instruction("je __rt_mixed_cast_bool_from_array_linux_x86_64");     // hashes are truthy when non-empty
    emitter.instruction("mov rax, 0");                                          // null and unsupported payloads are falsy for now
    emitter.instruction("jmp __rt_mixed_cast_bool_done_linux_x86_64");          // return the normalized boolean result

    emitter.label("__rt_mixed_cast_bool_from_int_linux_x86_64");
    emitter.instruction("test rdi, rdi");                                       // check whether the unboxed integer payload is zero
    emitter.instruction("setne al");                                            // integers are truthy when non-zero
    emitter.instruction("movzx rax, al");                                       // normalize the boolean result back to a full integer register
    emitter.instruction("jmp __rt_mixed_cast_bool_done_linux_x86_64");          // return the integer truthiness result

    emitter.label("__rt_mixed_cast_bool_from_string_linux_x86_64");
    emitter.instruction("test rdx, rdx");                                       // empty strings are falsy
    emitter.instruction("je __rt_mixed_cast_bool_done_linux_x86_64");           // return the default false result when the string length is zero
    emitter.instruction("cmp rdx, 1");                                          // check whether the string length is exactly one byte
    emitter.instruction("jne __rt_mixed_cast_bool_string_truthy_linux_x86_64"); // strings longer than one byte are always truthy
    emitter.instruction("movzx r8d, BYTE PTR [rdi]");                           // load the first byte of the string payload
    emitter.instruction("cmp r8d, 48");                                         // compare the single byte against ASCII '0'
    emitter.instruction("setne al");                                            // the one-byte string \"0\" is falsy, everything else is truthy
    emitter.instruction("movzx rax, al");                                       // normalize the boolean result back to a full integer register
    emitter.instruction("jmp __rt_mixed_cast_bool_done_linux_x86_64");          // return the string truthiness result

    emitter.label("__rt_mixed_cast_bool_string_truthy_linux_x86_64");
    emitter.instruction("mov rax, 1");                                          // non-empty strings other than \"0\" are truthy
    emitter.instruction("jmp __rt_mixed_cast_bool_done_linux_x86_64");          // return the string truthiness result

    emitter.label("__rt_mixed_cast_bool_from_float_linux_x86_64");
    emitter.instruction("movq xmm0, rdi");                                      // move the unboxed float bits into the floating-point result register
    emitter.instruction("xorpd xmm1, xmm1");                                    // materialize a zero floating-point register for the truthiness comparison
    emitter.instruction("ucomisd xmm0, xmm1");                                  // compare the float payload against zero
    emitter.instruction("setne al");                                            // floats are truthy when they compare non-equal to zero
    emitter.instruction("movzx rax, al");                                       // normalize the boolean result back to a full integer register
    emitter.instruction("jmp __rt_mixed_cast_bool_done_linux_x86_64");          // return the float truthiness result

    emitter.label("__rt_mixed_cast_bool_from_bool_linux_x86_64");
    emitter.instruction("mov rax, rdi");                                        // bool payloads are already normalized to 0 or 1
    emitter.instruction("jmp __rt_mixed_cast_bool_done_linux_x86_64");          // return the bool payload directly

    emitter.label("__rt_mixed_cast_bool_from_array_linux_x86_64");
    emitter.instruction("test rdi, rdi");                                       // null container pointers stay falsy
    emitter.instruction("je __rt_mixed_cast_bool_done_linux_x86_64");           // return the default false result when the container pointer is null
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // load the current container element count from the header
    emitter.instruction("test rax, rax");                                       // compare the container element count against zero
    emitter.instruction("setne al");                                            // containers are truthy when non-empty
    emitter.instruction("movzx rax, al");                                       // normalize the boolean result back to a full integer register

    emitter.label("__rt_mixed_cast_bool_done_linux_x86_64");
    emitter.instruction("add rsp, 16");                                         // release the aligned temporary slot reserved for nested helper calls
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return the boolean cast result in rax
}
