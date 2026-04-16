use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// mixed_cast_string: cast a boxed mixed payload to string using the current scalar rules.
/// Input:  x0 = boxed mixed pointer
/// Output: x1 = string pointer, x2 = string length
pub fn emit_mixed_cast_string(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_cast_string_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: mixed_cast_string ---");
    emitter.label_global("__rt_mixed_cast_string");

    emitter.instruction("sub sp, sp, #32");                                     // allocate a small stack frame for nested helper calls
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish the helper stack frame
    emitter.instruction("bl __rt_mixed_unbox");                                 // x0=tag, x1=value_lo, x2=value_hi for the boxed payload
    emitter.instruction("cmp x0, #0");                                          // does the mixed payload hold an int?
    emitter.instruction("b.eq __rt_mixed_cast_string_from_int");                // ints cast through itoa
    emitter.instruction("cmp x0, #1");                                          // does the mixed payload already hold a string?
    emitter.instruction("b.eq __rt_mixed_cast_string_done");                    // strings already satisfy the cast result registers
    emitter.instruction("cmp x0, #2");                                          // does the mixed payload hold a float?
    emitter.instruction("b.eq __rt_mixed_cast_string_from_float");              // floats cast through ftoa
    emitter.instruction("cmp x0, #3");                                          // does the mixed payload hold a bool?
    emitter.instruction("b.eq __rt_mixed_cast_string_from_bool");               // bools cast to "1" or ""
    emitter.instruction("mov x1, xzr");                                         // unsupported and null payloads produce an empty string pointer
    emitter.instruction("mov x2, xzr");                                         // unsupported and null payloads produce an empty string length
    emitter.instruction("b __rt_mixed_cast_string_done");                       // return the normalized empty-string result

    emitter.label("__rt_mixed_cast_string_from_int");
    emitter.instruction("mov x0, x1");                                          // move the integer payload into the itoa argument register
    emitter.instruction("bl __rt_itoa");                                        // convert the integer payload to decimal text
    emitter.instruction("b __rt_mixed_cast_string_done");                       // return the converted integer string

    emitter.label("__rt_mixed_cast_string_from_float");
    emitter.instruction("fmov d0, x1");                                         // move the unboxed float bits into the FP register file
    emitter.instruction("bl __rt_ftoa");                                        // convert the float payload to decimal text
    emitter.instruction("b __rt_mixed_cast_string_done");                       // return the converted float string

    emitter.label("__rt_mixed_cast_string_from_bool");
    emitter.instruction("cbz x1, __rt_mixed_cast_string_false");                // false casts to the empty string
    emitter.instruction("mov x0, x1");                                          // move the true payload (1) into the itoa argument register
    emitter.instruction("bl __rt_itoa");                                        // convert true to the string "1"
    emitter.instruction("b __rt_mixed_cast_string_done");                       // return the converted bool string

    emitter.label("__rt_mixed_cast_string_false");
    emitter.instruction("mov x1, xzr");                                         // false produces an empty string pointer
    emitter.instruction("mov x2, xzr");                                         // false produces an empty string length

    emitter.label("__rt_mixed_cast_string_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper stack frame
    emitter.instruction("ret");                                                 // return the string cast result in x1/x2
}

fn emit_mixed_cast_string_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_cast_string ---");
    emitter.label_global("__rt_mixed_cast_string");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while mixed string casting uses nested helpers
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the helper
    emitter.instruction("call __rt_mixed_unbox");                               // rax=tag, rdi=value_lo, rdx=value_hi for the boxed payload
    emitter.instruction("cmp rax, 0");                                          // does the mixed payload hold an int?
    emitter.instruction("je __rt_mixed_cast_string_from_int");                  // ints cast through itoa
    emitter.instruction("cmp rax, 1");                                          // does the mixed payload already hold a string?
    emitter.instruction("je __rt_mixed_cast_string_from_string");               // strings already satisfy the cast result registers
    emitter.instruction("cmp rax, 2");                                          // does the mixed payload hold a float?
    emitter.instruction("je __rt_mixed_cast_string_from_float");                // floats cast through ftoa
    emitter.instruction("cmp rax, 3");                                          // does the mixed payload hold a bool?
    emitter.instruction("je __rt_mixed_cast_string_from_bool");                 // bools cast to \"1\" or \"\"
    emitter.instruction("xor rax, rax");                                        // unsupported and null payloads produce an empty string pointer
    emitter.instruction("xor rdx, rdx");                                        // unsupported and null payloads produce an empty string length
    emitter.instruction("jmp __rt_mixed_cast_string_done");                     // return the normalized empty-string result

    emitter.label("__rt_mixed_cast_string_from_int");
    emitter.instruction("mov rax, rdi");                                        // move the integer payload into the itoa input register
    emitter.instruction("call __rt_itoa");                                      // convert the integer payload to decimal text
    emitter.instruction("jmp __rt_mixed_cast_string_done");                     // return the converted integer string

    emitter.label("__rt_mixed_cast_string_from_string");
    emitter.instruction("mov rax, rdi");                                        // move the unboxed string pointer into the ABI string result register
    emitter.instruction("jmp __rt_mixed_cast_string_done");                     // keep the existing string length in rdx

    emitter.label("__rt_mixed_cast_string_from_float");
    emitter.instruction("movq xmm0, rdi");                                      // move the unboxed float bits into the FP register file
    emitter.instruction("call __rt_ftoa");                                      // convert the float payload to decimal text
    emitter.instruction("jmp __rt_mixed_cast_string_done");                     // return the converted float string

    emitter.label("__rt_mixed_cast_string_from_bool");
    emitter.instruction("test rdi, rdi");                                       // false casts to the empty string
    emitter.instruction("je __rt_mixed_cast_string_false");                     // skip integer conversion when the bool payload is false
    emitter.instruction("mov rax, rdi");                                        // move the true payload (1) into the itoa input register
    emitter.instruction("call __rt_itoa");                                      // convert true to the string \"1\"
    emitter.instruction("jmp __rt_mixed_cast_string_done");                     // return the converted bool string

    emitter.label("__rt_mixed_cast_string_false");
    emitter.instruction("xor rax, rax");                                        // false produces an empty string pointer
    emitter.instruction("xor rdx, rdx");                                        // false produces an empty string length

    emitter.label("__rt_mixed_cast_string_done");
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return the string cast result in rax/rdx
}
