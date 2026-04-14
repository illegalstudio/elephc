use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_shift: remove and return the first element of an integer array.
/// Input: x0 = array pointer
/// Output: x0 = removed first element value
/// Mutates the array in place: shifts all elements left, decrements length.
pub fn emit_array_shift(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_shift_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_shift ---");
    emitter.label_global("__rt_array_shift");

    // -- check if array is empty --
    emitter.instruction("ldr x9, [x0]");                                        // x9 = current array length
    emitter.instruction("cbnz x9, __rt_array_shift_notempty");                  // if length != 0, proceed normally

    // -- empty array: return null sentinel --
    emitter.instruction("movz x0, #0xFFFE");                                    // load null sentinel bits [15:0]
    emitter.instruction("movk x0, #0xFFFF, lsl #16");                           // load null sentinel bits [31:16]
    emitter.instruction("movk x0, #0xFFFF, lsl #32");                           // load null sentinel bits [47:32]
    emitter.instruction("movk x0, #0x7FFF, lsl #48");                           // load null sentinel bits [63:48] = 0x7FFFFFFFFFFFFFFE
    emitter.instruction("ret");                                                 // return null to caller

    // -- array is not empty, proceed --
    emitter.label("__rt_array_shift_notempty");
    emitter.instruction("add x10, x0, #24");                                    // x10 = base of data region

    // -- save the first element --
    emitter.instruction("ldr x11, [x10]");                                      // x11 = data[0] (element to return)

    // -- shift all elements left by one position --
    emitter.instruction("mov x12, #1");                                         // x12 = src_index = 1

    emitter.label("__rt_array_shift_loop");
    emitter.instruction("cmp x12, x9");                                         // compare src_index with length
    emitter.instruction("b.ge __rt_array_shift_done");                          // if src_index >= length, shifting complete
    emitter.instruction("ldr x13, [x10, x12, lsl #3]");                         // x13 = data[src_index]
    emitter.instruction("sub x14, x12, #1");                                    // x14 = dst_index = src_index - 1
    emitter.instruction("str x13, [x10, x14, lsl #3]");                         // data[dst_index] = data[src_index]
    emitter.instruction("add x12, x12, #1");                                    // src_index += 1
    emitter.instruction("b __rt_array_shift_loop");                             // continue loop

    // -- decrement length and return removed element --
    emitter.label("__rt_array_shift_done");
    emitter.instruction("sub x9, x9, #1");                                      // length -= 1
    emitter.instruction("str x9, [x0]");                                        // write updated length to header
    emitter.instruction("mov x0, x11");                                         // return the removed first element
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_array_shift_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_shift ---");
    emitter.label_global("__rt_array_shift");

    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the current indexed-array length before checking whether the shift operation is empty
    emitter.instruction("test r10, r10");                                       // check whether the indexed array currently stores any live scalar payloads
    emitter.instruction("jnz __rt_array_shift_notempty_x86");                   // continue with the scalar left-shift loop when the indexed array is not empty
    emitter.instruction("mov rax, 0x7ffffffffffffffe");                         // materialize the shared null sentinel as the empty-array shift result on x86_64
    emitter.instruction("ret");                                                 // return the null sentinel immediately when array_shift runs on an empty indexed array

    emitter.label("__rt_array_shift_notempty_x86");
    emitter.instruction("lea r11, [rdi + 24]");                                 // compute the first scalar payload slot address in the source indexed array
    emitter.instruction("mov rax, QWORD PTR [r11]");                            // preserve the removed first scalar payload in the x86_64 integer result register
    emitter.instruction("mov rcx, 1");                                          // start the source cursor at the second live scalar payload slot

    emitter.label("__rt_array_shift_loop_x86");
    emitter.instruction("cmp rcx, r10");                                        // compare the shifting source cursor against the original indexed-array length
    emitter.instruction("jge __rt_array_shift_done_x86");                       // finish once every remaining live scalar payload has moved one slot toward the front
    emitter.instruction("mov r8, QWORD PTR [r11 + rcx * 8]");                   // load the next scalar payload that must slide one slot toward the front of the indexed array
    emitter.instruction("mov QWORD PTR [r11 + rcx * 8 - 8], r8");               // store that scalar payload into the previous indexed-array slot to close the removed-element gap
    emitter.instruction("add rcx, 1");                                          // advance the shifting source cursor to the next live scalar payload slot
    emitter.instruction("jmp __rt_array_shift_loop_x86");                       // continue shifting until every trailing scalar payload has moved one slot left

    emitter.label("__rt_array_shift_done_x86");
    emitter.instruction("sub r10, 1");                                          // decrement the indexed-array logical length after removing the first scalar payload
    emitter.instruction("mov QWORD PTR [rdi], r10");                            // persist the decremented indexed-array logical length back into the array header
    emitter.instruction("ret");                                                 // return the removed first scalar payload in rax
}
