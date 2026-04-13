use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// hash_new: create a new hash table on the heap.
/// Input:  x0=initial_capacity, x1=value_type_tag
///         (0=int, 1=str, 2=float, 3=bool, 4=array, 5=assoc, 6=object, 7=mixed, 8=null)
/// Output: x0=pointer to hash table
/// Layout: [count:8][capacity:8][value_type:8][head:8][tail:8][entries...]
///         where each entry is 64 bytes:
///         [occupied:8][key_ptr:8][key_len:8][value_lo:8][value_hi:8][value_tag:8][prev:8][next:8]
pub fn emit_hash_new(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_new_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: hash_new ---");
    emitter.label_global("__rt_hash_new");

    // -- set up stack frame, save arguments --
    emitter.instruction("sub sp, sp, #32");                                     // allocate 32 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save capacity to stack
    emitter.instruction("str x1, [sp, #8]");                                    // save value_type to stack

    // -- calculate total size: 40 + capacity * 64 --
    emitter.instruction("mov x9, #64");                                         // entry size = 64 bytes with per-entry tags and insertion-order links
    emitter.instruction("mul x2, x0, x9");                                      // x2 = capacity * 64 = entries region size
    emitter.instruction("add x0, x2, #40");                                     // x0 = total size (40-byte header + entries)
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate memory, x0 = pointer to hash table
    emitter.instruction("mov x9, #3");                                          // heap kind 3 = associative array / hash table
    emitter.instruction("mov x10, #0x8000");                                    // bit 15 marks heap containers that participate in copy-on-write
    emitter.instruction("orr x9, x9, x10");                                     // preserve the persistent copy-on-write container flag in the kind word
    emitter.instruction("str x9, [x0, #-8]");                                   // store hash-table kind in the uniform heap header

    // -- initialize header fields --
    emitter.instruction("str xzr, [x0]");                                       // header[0]: count = 0
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload capacity from stack
    emitter.instruction("str x9, [x0, #8]");                                    // header[8]: capacity
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload value_type from stack
    emitter.instruction("str x10, [x0, #16]");                                  // header[16]: value_type
    emitter.instruction("mov x15, #-1");                                        // sentinel index for an empty insertion-order chain
    emitter.instruction("str x15, [x0, #24]");                                  // header[24]: head = none
    emitter.instruction("str x15, [x0, #32]");                                  // header[32]: tail = none

    // -- zero all entry slots (set occupied=0 for each entry) --
    emitter.instruction("add x11, x0, #40");                                    // x11 = base of entries region after the extended header
    emitter.instruction("mov x12, #64");                                        // x12 = entry size
    emitter.instruction("mul x13, x9, x12");                                    // x13 = total bytes in entries region
    emitter.instruction("add x14, x11, x13");                                   // x14 = end of entries region

    emitter.label("__rt_hash_new_zero");
    emitter.instruction("cmp x11, x14");                                        // check if we've reached end of entries
    emitter.instruction("b.ge __rt_hash_new_done");                             // if past end, zeroing is complete
    emitter.instruction("str xzr, [x11]");                                      // set occupied field to 0 (empty)
    emitter.instruction("add x11, x11, #64");                                   // advance to next entry
    emitter.instruction("b __rt_hash_new_zero");                                // continue zeroing

    // -- tear down stack frame and return --
    emitter.label("__rt_hash_new_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = hash table pointer
}

fn emit_hash_new_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_new ---");
    emitter.label_global("__rt_hash_new");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving hash-construction spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved capacity and value-type metadata
    emitter.instruction("sub rsp, 16");                                         // reserve local slots for capacity and value_type across the malloc call
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the requested hash capacity across the allocator call
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the requested runtime value_type across the allocator call
    emitter.instruction("mov rax, rdi");                                        // copy the capacity into a scratch register before scaling it by the entry size
    emitter.instruction("imul rax, 64");                                        // compute the total bytes needed for the 64-byte hash entry array
    emitter.instruction("add rax, 40");                                         // include the fixed 40-byte hash header in the allocation size
    emitter.instruction("call __rt_heap_alloc");                                // allocate the hash-table storage through the shared x86_64 heap wrapper
    emitter.instruction(&format!("mov r10, 0x{:x}", (X86_64_HEAP_MAGIC_HI32 << 32) | 0x8003)); // materialize the copy-on-write hash-table heap kind word with the x86_64 heap marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the allocated payload as an associative-array heap object in the uniform header
    emitter.instruction("mov QWORD PTR [rax], 0");                              // header[0]: initialize the live-entry count to zero
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the requested capacity after heap_alloc clobbered caller-saved registers
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // header[8]: store the chosen table capacity
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the runtime value_type tag after heap_alloc returns
    emitter.instruction("mov QWORD PTR [rax + 16], r10");                       // header[16]: store the table-wide value_type tag
    emitter.instruction("mov r10, -1");                                         // materialize the empty-list sentinel for the insertion-order head and tail
    emitter.instruction("mov QWORD PTR [rax + 24], r10");                       // header[24]: head = none for a newly allocated empty hash table
    emitter.instruction("mov QWORD PTR [rax + 32], r10");                       // header[32]: tail = none for a newly allocated empty hash table
    emitter.instruction("mov r10, rax");                                        // seed the entry-region cursor from the hash-table base pointer
    emitter.instruction("add r10, 40");                                         // advance the cursor to the first hash entry after the fixed header
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload the requested capacity to determine how many entry headers to clear

    emitter.label("__rt_hash_new_zero");
    emitter.instruction("test r11, r11");                                       // stop clearing once every entry slot in the requested capacity has been visited
    emitter.instruction("je __rt_hash_new_done");                               // skip the zeroing loop entirely for a zero-capacity hash table
    emitter.instruction("mov QWORD PTR [r10], 0");                              // clear the occupied/tombstone marker for the current hash entry slot
    emitter.instruction("add r10, 64");                                         // advance the entry cursor to the next hash slot in the entries region
    emitter.instruction("sub r11, 1");                                          // decrement the number of remaining hash entry headers to clear
    emitter.instruction("jmp __rt_hash_new_zero");                              // continue clearing occupied markers until the entries region is initialized

    emitter.label("__rt_hash_new_done");
    emitter.instruction("add rsp, 16");                                         // release the temporary capacity and value-type spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the hash-table pointer
    emitter.instruction("ret");                                                 // return the newly allocated hash-table pointer in rax
}
