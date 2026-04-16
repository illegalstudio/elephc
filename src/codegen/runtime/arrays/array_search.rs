use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_search: find a value in an integer array and return its index.
/// Input: x0 = array pointer, x1 = needle (integer value)
/// Output: x0 = index of first match, or -1 if not found
pub fn emit_array_search(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_search_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_search ---");
    emitter.label_global("__rt_array_search");

    // -- set up loop variables --
    emitter.instruction("ldr x9, [x0]");                                        // x9 = array length from header
    emitter.instruction("add x10, x0, #24");                                    // x10 = base of data region (skip 24-byte header)
    emitter.instruction("mov x11, #0");                                         // x11 = i = 0 (loop counter)

    // -- iterate through elements looking for needle --
    emitter.label("__rt_array_search_loop");
    emitter.instruction("cmp x11, x9");                                         // compare i with array length
    emitter.instruction("b.ge __rt_array_search_notfound");                     // if i >= length, value not found
    emitter.instruction("ldr x12, [x10, x11, lsl #3]");                         // x12 = data[i] (load element at index i)
    emitter.instruction("cmp x12, x1");                                         // compare element with needle
    emitter.instruction("b.eq __rt_array_search_found");                        // if equal, we found it
    emitter.instruction("add x11, x11, #1");                                    // i += 1
    emitter.instruction("b __rt_array_search_loop");                            // continue loop

    // -- value found at index x11 --
    emitter.label("__rt_array_search_found");
    emitter.instruction("mov x0, x11");                                         // return the index
    emitter.instruction("ret");                                                 // return to caller

    // -- value not found --
    emitter.label("__rt_array_search_notfound");
    emitter.instruction("mov x0, #-1");                                         // return -1 (not found sentinel)
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_array_search_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_search ---");
    emitter.label_global("__rt_array_search");

    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the indexed-array length from the header before starting the linear scan
    emitter.instruction("lea r11, [rdi + 24]");                                 // point at the indexed-array payload region just after the fixed header
    emitter.instruction("xor eax, eax");                                        // start scanning from element index 0

    emitter.label("__rt_array_search_loop");
    emitter.instruction("cmp rax, r10");                                        // have we visited every element in the indexed array?
    emitter.instruction("jge __rt_array_search_notfound");                      // stop once the scan reaches the logical array length
    emitter.instruction("mov rdx, QWORD PTR [r11 + rax * 8]");                  // load the current indexed-array element from the 8-byte payload region
    emitter.instruction("cmp rdx, rsi");                                        // compare the current indexed-array element against the searched needle
    emitter.instruction("je __rt_array_search_found");                          // return the first matching index immediately
    emitter.instruction("add rax, 1");                                          // advance to the next indexed-array element after a mismatch
    emitter.instruction("jmp __rt_array_search_loop");                          // continue the linear scan until match or exhaustion

    emitter.label("__rt_array_search_found");
    emitter.instruction("ret");                                                 // return the first matching index in the standard integer result register

    emitter.label("__rt_array_search_notfound");
    emitter.instruction("mov rax, -1");                                         // return -1 when the indexed-array scan does not find any matching element
    emitter.instruction("ret");                                                 // return the not-found sentinel to the caller
}
