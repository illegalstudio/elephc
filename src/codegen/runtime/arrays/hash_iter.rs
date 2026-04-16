use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// hash_iter_next: iterate over hash table entries in insertion order.
/// Input:  x0=hash_table_ptr, x1=cursor (start with 0)
/// Output: x0=next_cursor (or -1 if done), x1=key_ptr, x2=key_len, x3=value_lo, x4=value_hi, x5=value_tag
pub fn emit_hash_iter(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_iter_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: hash_iter_next ---");
    emitter.label_global("__rt_hash_iter_next");

    // -- cursor protocol --
    // 0   = start from header.head
    // >0  = slot index + 1 of the next entry to return
    // -2  = post-last cursor returned with the final yielded entry
    // -1  = no more entries
    emitter.instruction("cmp x1, #-1");                                         // has the caller already consumed the end sentinel?
    emitter.instruction("b.eq __rt_hash_iter_end");                             // repeated end probes stay at done
    emitter.instruction("cmp x1, #-2");                                         // was the previous yielded entry the tail?
    emitter.instruction("b.eq __rt_hash_iter_end");                             // convert the post-last cursor into the final done signal
    emitter.instruction("cbnz x1, __rt_hash_iter_resume");                      // non-zero cursors already encode the next slot to return

    // -- start a fresh insertion-order walk from the head slot --
    emitter.instruction("ldr x6, [x0, #24]");                                   // x6 = head slot index from the hash header
    emitter.instruction("cmp x6, #-1");                                         // does the hash contain any entries?
    emitter.instruction("b.eq __rt_hash_iter_end");                             // empty hashes are immediately done
    emitter.instruction("b __rt_hash_iter_entry");                              // load and return the head entry

    // -- resume iteration from the encoded next slot --
    emitter.label("__rt_hash_iter_resume");
    emitter.instruction("sub x6, x1, #1");                                      // decode slot index = cursor - 1

    // -- compute entry address: base + 40 + index * 64 --
    emitter.label("__rt_hash_iter_entry");
    emitter.instruction("mov x7, #64");                                         // x7 = hash entry size in bytes
    emitter.instruction("mul x8, x6, x7");                                      // x8 = slot index * 64
    emitter.instruction("add x8, x0, x8");                                      // advance from the hash base to the selected slot
    emitter.instruction("add x8, x8, #40");                                     // skip the 40-byte hash header

    // -- return the selected entry and encode the next cursor --
    emitter.instruction("ldr x9, [x8, #56]");                                   // x9 = next slot index from the insertion-order chain
    emitter.instruction("cmp x9, #-1");                                         // is this the tail entry?
    emitter.instruction("b.eq __rt_hash_iter_tail");                            // tail entries return the post-last cursor
    emitter.instruction("add x0, x9, #1");                                      // x0 = next cursor (slot index + 1)
    emitter.instruction("b __rt_hash_iter_return");                             // emit the current entry payload
    emitter.label("__rt_hash_iter_tail");
    emitter.instruction("mov x0, #-2");                                         // x0 = post-last cursor for the next probe
    emitter.label("__rt_hash_iter_return");
    emitter.instruction("ldr x1, [x8, #8]");                                    // x1 = key_ptr
    emitter.instruction("ldr x2, [x8, #16]");                                   // x2 = key_len
    emitter.instruction("ldr x3, [x8, #24]");                                   // x3 = value_lo
    emitter.instruction("ldr x4, [x8, #32]");                                   // x4 = value_hi
    emitter.instruction("ldr x5, [x8, #40]");                                   // x5 = value_tag
    emitter.instruction("ret");                                                 // return the current entry payload

    // -- no more entries --
    emitter.label("__rt_hash_iter_end");
    emitter.instruction("mov x0, #-1");                                         // return -1 to signal end of iteration
    emitter.instruction("mov x5, #8");                                          // value_tag = null when iteration is done
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_hash_iter_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_iter_next ---");
    emitter.label_global("__rt_hash_iter_next");

    emitter.instruction("cmp rsi, -1");                                         // has the caller already consumed the terminal done sentinel?
    emitter.instruction("je __rt_hash_iter_end");                               // repeated end probes keep returning the done sentinel
    emitter.instruction("cmp rsi, -2");                                         // did the previous yielded entry already encode the post-last cursor?
    emitter.instruction("je __rt_hash_iter_end");                               // convert the post-last cursor into the final done signal
    emitter.instruction("test rsi, rsi");                                       // does the caller want to resume from a previously returned next cursor?
    emitter.instruction("jne __rt_hash_iter_resume");                           // non-zero cursors already encode the next slot to return

    emitter.instruction("mov r10, QWORD PTR [rdi + 24]");                       // load the insertion-order head slot from the hash header for a fresh walk
    emitter.instruction("cmp r10, -1");                                         // is the hash table empty?
    emitter.instruction("je __rt_hash_iter_end");                               // empty hashes are immediately done
    emitter.instruction("jmp __rt_hash_iter_entry");                            // return the head entry on the first iteration step

    emitter.label("__rt_hash_iter_resume");
    emitter.instruction("mov r10, rsi");                                        // copy the encoded cursor before decoding it into a raw slot index
    emitter.instruction("sub r10, 1");                                          // decode slot index = cursor - 1 for resumed insertion-order walks

    emitter.label("__rt_hash_iter_entry");
    emitter.instruction("mov r11, r10");                                        // copy the slot index before scaling it into a byte offset
    emitter.instruction("shl r11, 6");                                          // convert the slot index into a 64-byte hash-entry offset
    emitter.instruction("add r11, rdi");                                        // advance from the hash-table base pointer to the selected entry block
    emitter.instruction("add r11, 40");                                         // skip the fixed 40-byte hash header to land on the selected entry
    emitter.instruction("mov rax, QWORD PTR [r11 + 56]");                       // load the insertion-order next-slot index from the current entry
    emitter.instruction("cmp rax, -1");                                         // is this entry the insertion-order tail?
    emitter.instruction("je __rt_hash_iter_tail");                              // tail entries return the post-last cursor so the next probe yields done
    emitter.instruction("add rax, 1");                                          // encode the next slot as cursor = slot index + 1 for the caller
    emitter.instruction("jmp __rt_hash_iter_return");                           // return the current entry payload alongside the encoded next cursor

    emitter.label("__rt_hash_iter_tail");
    emitter.instruction("mov rax, -2");                                         // encode the post-last cursor after yielding the current tail entry

    emitter.label("__rt_hash_iter_return");
    emitter.instruction("mov rdi, QWORD PTR [r11 + 8]");                        // return the entry key pointer in the first borrowed-string result register
    emitter.instruction("mov rdx, QWORD PTR [r11 + 16]");                       // return the entry key length in the paired borrowed-string result register
    emitter.instruction("mov rcx, QWORD PTR [r11 + 24]");                       // return the low payload word for the current hash entry value
    emitter.instruction("mov r8, QWORD PTR [r11 + 32]");                        // return the high payload word for the current hash entry value
    emitter.instruction("mov r9, QWORD PTR [r11 + 40]");                        // return the runtime value tag that describes the current hash entry payload
    emitter.instruction("ret");                                                 // return the current insertion-order entry payload to the caller

    emitter.label("__rt_hash_iter_end");
    emitter.instruction("mov rax, -1");                                         // return the done sentinel cursor once iteration has completed
    emitter.instruction("xor edi, edi");                                        // clear the borrowed key pointer when iteration is done
    emitter.instruction("xor edx, edx");                                        // clear the borrowed key length when iteration is done
    emitter.instruction("xor ecx, ecx");                                        // clear the low payload word when iteration is done
    emitter.instruction("xor r8d, r8d");                                        // clear the high payload word when iteration is done
    emitter.instruction("mov r9, 8");                                           // return runtime value tag 8 = null when iteration is done
    emitter.instruction("ret");                                                 // return the terminal iterator state to the caller
}
