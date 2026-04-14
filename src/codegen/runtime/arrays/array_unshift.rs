use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_unshift: prepend an integer value to the front of an array.
/// Input: x0 = array pointer, x1 = value to prepend
/// Output: x0 = new array length
/// Mutates the array in place: shifts all elements right, inserts at index 0.
pub fn emit_array_unshift(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_unshift_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_unshift ---");
    emitter.label_global("__rt_array_unshift");

    // -- load array metadata --
    emitter.instruction("ldr x9, [x0]");                                        // x9 = current array length
    emitter.instruction("add x10, x0, #24");                                    // x10 = base of data region

    // -- shift all elements right by one, starting from the end --
    emitter.instruction("sub x11, x9, #1");                                     // x11 = src_index = length - 1 (last element)

    emitter.label("__rt_array_unshift_loop");
    emitter.instruction("cmp x11, #0");                                         // check if src_index < 0
    emitter.instruction("b.lt __rt_array_unshift_insert");                      // if so, shifting complete
    emitter.instruction("ldr x12, [x10, x11, lsl #3]");                         // x12 = data[src_index]
    emitter.instruction("add x13, x11, #1");                                    // x13 = dst_index = src_index + 1
    emitter.instruction("str x12, [x10, x13, lsl #3]");                         // data[dst_index] = data[src_index]
    emitter.instruction("sub x11, x11, #1");                                    // src_index -= 1
    emitter.instruction("b __rt_array_unshift_loop");                           // continue loop

    // -- insert value at index 0 and update length --
    emitter.label("__rt_array_unshift_insert");
    emitter.instruction("str x1, [x10]");                                       // data[0] = value
    emitter.instruction("add x9, x9, #1");                                      // length += 1
    emitter.instruction("str x9, [x0]");                                        // write updated length to header
    emitter.instruction("mov x0, x9");                                          // return new length
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_array_unshift_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_unshift ---");
    emitter.label_global("__rt_array_unshift");

    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the current indexed-array length before shifting every live scalar payload one slot to the right
    emitter.instruction("lea r11, [rdi + 24]");                                 // compute the first scalar payload slot address in the destination indexed array
    emitter.instruction("test r10, r10");                                       // detect the empty-array case before seeding the reverse shift cursor
    emitter.instruction("jz __rt_array_unshift_insert_x86");                    // skip the reverse shift loop entirely when prepending into an empty indexed array
    emitter.instruction("mov rcx, r10");                                        // seed the reverse shift cursor from the current indexed-array length
    emitter.instruction("sub rcx, 1");                                          // move the reverse shift cursor to the last live scalar payload slot

    emitter.label("__rt_array_unshift_loop_x86");
    emitter.instruction("cmp rcx, 0");                                          // check whether every live scalar payload has already been shifted one slot to the right
    emitter.instruction("jl __rt_array_unshift_insert_x86");                    // stop shifting once the reverse cursor has moved before the front of the indexed array
    emitter.instruction("mov r8, QWORD PTR [r11 + rcx * 8]");                   // load the current scalar payload before moving it one slot toward the back of the indexed array
    emitter.instruction("mov QWORD PTR [r11 + rcx * 8 + 8], r8");               // store the shifted scalar payload into the next indexed-array slot
    emitter.instruction("sub rcx, 1");                                          // move the reverse shift cursor toward the front of the indexed array
    emitter.instruction("jmp __rt_array_unshift_loop_x86");                     // continue shifting until every live scalar payload has moved one slot to the right

    emitter.label("__rt_array_unshift_insert_x86");
    emitter.instruction("mov QWORD PTR [r11], rsi");                            // store the prepended scalar payload into the now-free first indexed-array slot
    emitter.instruction("add r10, 1");                                          // increment the indexed-array logical length after prepending one scalar payload
    emitter.instruction("mov QWORD PTR [rdi], r10");                            // persist the incremented indexed-array logical length back into the array header
    emitter.instruction("mov rax, r10");                                        // return the new indexed-array length in the x86_64 integer result register
    emitter.instruction("ret");                                                 // return to the caller after prepending the scalar payload
}
