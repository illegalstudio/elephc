use crate::codegen::{emit::Emitter, platform::Arch};

/// hash_may_have_cyclic_values: detect whether any entry can participate in a cycle.
/// Input:  x0 = hash table pointer
/// Output: x0 = 1 if the hash contains array/hash/object/mixed graph values, else 0
pub fn emit_hash_may_have_cyclic_values(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emitter.blank();
        emitter.comment("--- runtime: hash_may_have_cyclic_values ---");
        emitter.label_global("__rt_hash_may_have_cyclic_values");

        emitter.instruction("push r12");                                        // preserve the first callee-saved scratch register used by the x86_64 slot walker
        emitter.instruction("push r13");                                        // preserve the second callee-saved scratch register used by the x86_64 slot walker
        emitter.instruction("push r14");                                        // preserve the third callee-saved scratch register used by the x86_64 tag dispatcher
        emitter.instruction("push r15");                                        // preserve the callee-saved register used by the nested mixed unwrapping loop
        emitter.instruction("test rax, rax");                                   // null hashes are never cycle roots
        emitter.instruction("jz __rt_hash_may_have_cyclic_values_no");          // skip the slot walk entirely for null hash pointers
        emitter.instruction("mov r9, QWORD PTR [rax + 8]");                     // load the table capacity from the hash header
        emitter.instruction("xor r10, r10");                                    // start scanning slots from index zero

        emitter.label("__rt_hash_may_have_cyclic_values_loop");
        emitter.instruction("cmp r10, r9");                                     // have all hash slots been scanned already?
        emitter.instruction("jae __rt_hash_may_have_cyclic_values_no");         // yes — no cyclic-capable payloads were found
        emitter.instruction("mov r11, 64");                                     // hash entries are 64 bytes wide in the runtime layout
        emitter.instruction("mov r12, r10");                                    // preserve the slot index while scaling it by the entry width
        emitter.instruction("imul r12, r11");                                   // compute slot_index * entry_size for this hash entry walk
        emitter.instruction("lea r12, [rax + r12 + 40]");                       // advance from the hash base to the current slot payload after the 40-byte header
        emitter.instruction("mov r13, QWORD PTR [r12]");                        // load the occupied flag for the current hash slot
        emitter.instruction("cmp r13, 1");                                      // is this slot currently occupied by a live hash entry?
        emitter.instruction("jne __rt_hash_may_have_cyclic_values_next");       // skip empty and tombstone slots during the cycle-capability scan
        emitter.instruction("mov r14, QWORD PTR [r12 + 40]");                   // load the runtime value tag for this hash entry payload
        emitter.instruction("cmp r14, 4");                                      // does the entry hold an indexed array directly?
        emitter.instruction("je __rt_hash_may_have_cyclic_values_yes");         // indexed arrays can participate in reference cycles
        emitter.instruction("cmp r14, 5");                                      // does the entry hold an associative array directly?
        emitter.instruction("je __rt_hash_may_have_cyclic_values_yes");         // associative arrays can participate in reference cycles
        emitter.instruction("cmp r14, 6");                                      // does the entry hold an object directly?
        emitter.instruction("je __rt_hash_may_have_cyclic_values_yes");         // objects can participate in reference cycles
        emitter.instruction("cmp r14, 7");                                      // does the entry hold a boxed mixed value?
        emitter.instruction("jne __rt_hash_may_have_cyclic_values_next");       // scalars and strings cannot form cycles through this helper
        emitter.instruction("mov r15, QWORD PTR [r12 + 24]");                   // load the boxed mixed pointer from the entry value_lo field
        emitter.instruction("test r15, r15");                                   // does the mixed entry actually carry a boxed payload?
        emitter.instruction("jz __rt_hash_may_have_cyclic_values_next");        // null mixed boxes behave like null scalars here

        emitter.label("__rt_hash_may_have_cyclic_values_mixed_loop");
        emitter.instruction("mov r14, QWORD PTR [r15]");                        // load the current boxed mixed payload tag while unwrapping nested mixed cells
        emitter.instruction("cmp r14, 4");                                      // does the boxed payload hold an indexed array?
        emitter.instruction("je __rt_hash_may_have_cyclic_values_yes");         // boxed arrays can participate in reference cycles
        emitter.instruction("cmp r14, 5");                                      // does the boxed payload hold an associative array?
        emitter.instruction("je __rt_hash_may_have_cyclic_values_yes");         // boxed hashes can participate in reference cycles
        emitter.instruction("cmp r14, 6");                                      // does the boxed payload hold an object?
        emitter.instruction("je __rt_hash_may_have_cyclic_values_yes");         // boxed objects can participate in reference cycles
        emitter.instruction("cmp r14, 7");                                      // does the boxed payload wrap another mixed cell?
        emitter.instruction("jne __rt_hash_may_have_cyclic_values_next");       // scalar and string payloads do not need cycle collection
        emitter.instruction("mov r15, QWORD PTR [r15 + 8]");                    // follow the nested mixed pointer stored in value_lo
        emitter.instruction("test r15, r15");                                   // did the nested mixed chain terminate with a null payload?
        emitter.instruction("jz __rt_hash_may_have_cyclic_values_next");        // yes — terminate the nested mixed scan safely
        emitter.instruction("jmp __rt_hash_may_have_cyclic_values_mixed_loop"); // continue unboxing nested mixed wrappers until a real payload appears

        emitter.label("__rt_hash_may_have_cyclic_values_next");
        emitter.instruction("add r10, 1");                                      // advance to the next hash slot after skipping or scanning this entry
        emitter.instruction("jmp __rt_hash_may_have_cyclic_values_loop");       // keep scanning until a cyclic-capable payload is found or slots are exhausted

        emitter.label("__rt_hash_may_have_cyclic_values_yes");
        emitter.instruction("mov eax, 1");                                      // report that the hash may need cycle collection on x86_64
        emitter.instruction("pop r15");                                         // restore the mixed-loop callee-saved scratch register before returning true
        emitter.instruction("pop r14");                                         // restore the tag-dispatch callee-saved scratch register before returning true
        emitter.instruction("pop r13");                                         // restore the slot-walk callee-saved scratch register before returning true
        emitter.instruction("pop r12");                                         // restore the scaled-offset callee-saved scratch register before returning true
        emitter.instruction("ret");                                             // return true to the caller

        emitter.label("__rt_hash_may_have_cyclic_values_no");
        emitter.instruction("xor eax, eax");                                    // report that ordinary decref can skip cycle collection
        emitter.instruction("pop r15");                                         // restore the mixed-loop callee-saved scratch register before returning false
        emitter.instruction("pop r14");                                         // restore the tag-dispatch callee-saved scratch register before returning false
        emitter.instruction("pop r13");                                         // restore the slot-walk callee-saved scratch register before returning false
        emitter.instruction("pop r12");                                         // restore the scaled-offset callee-saved scratch register before returning false
        emitter.instruction("ret");                                             // return false to the caller
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: hash_may_have_cyclic_values ---");
    emitter.label_global("__rt_hash_may_have_cyclic_values");

    // -- null hashes cannot contain cyclic children --
    emitter.instruction("cbz x0, __rt_hash_may_have_cyclic_values_no");         // null hashes are never cycle roots

    // -- load capacity and start scanning slots --
    emitter.instruction("ldr x9, [x0, #8]");                                    // x9 = table capacity
    emitter.instruction("mov x10, #0");                                         // x10 = slot index

    emitter.label("__rt_hash_may_have_cyclic_values_loop");
    emitter.instruction("cmp x10, x9");                                         // have we scanned every slot?
    emitter.instruction("b.ge __rt_hash_may_have_cyclic_values_no");            // stop once the table has no cyclic-capable entries

    // -- compute the current slot address --
    emitter.instruction("mov x11, #64");                                        // hash entries are 64 bytes wide
    emitter.instruction("mul x12, x10, x11");                                   // x12 = slot index * entry size
    emitter.instruction("add x12, x0, x12");                                    // advance from the hash base to this slot
    emitter.instruction("add x12, x12, #40");                                   // skip the 40-byte hash header
    emitter.instruction("ldr x13, [x12]");                                      // load the occupied flag
    emitter.instruction("cmp x13, #1");                                         // is this slot occupied?
    emitter.instruction("b.ne __rt_hash_may_have_cyclic_values_next");          // skip empty and tombstone slots

    // -- direct array/hash/object payloads can obviously participate in cycles --
    emitter.instruction("ldr x14, [x12, #40]");                                 // load the entry runtime value tag
    emitter.instruction("cmp x14, #4");                                         // is the entry an indexed array?
    emitter.instruction("b.eq __rt_hash_may_have_cyclic_values_yes");           // arrays can participate in reference cycles
    emitter.instruction("cmp x14, #5");                                         // is the entry an associative array?
    emitter.instruction("b.eq __rt_hash_may_have_cyclic_values_yes");           // hashes can participate in reference cycles
    emitter.instruction("cmp x14, #6");                                         // is the entry an object?
    emitter.instruction("b.eq __rt_hash_may_have_cyclic_values_yes");           // objects can participate in reference cycles
    emitter.instruction("cmp x14, #7");                                         // is the entry a boxed mixed value?
    emitter.instruction("b.ne __rt_hash_may_have_cyclic_values_next");          // plain scalars and strings cannot form cycles

    // -- mixed payloads only need GC if their nested payload graph can cycle --
    emitter.instruction("ldr x15, [x12, #24]");                                 // load the boxed mixed pointer from value_lo
    emitter.instruction("cbz x15, __rt_hash_may_have_cyclic_values_next");      // null mixed boxes behave like null scalars
    emitter.label("__rt_hash_may_have_cyclic_values_mixed_loop");
    emitter.instruction("ldr x14, [x15]");                                      // load the boxed mixed payload tag
    emitter.instruction("cmp x14, #4");                                         // does the mixed payload hold an indexed array?
    emitter.instruction("b.eq __rt_hash_may_have_cyclic_values_yes");           // boxed arrays can participate in reference cycles
    emitter.instruction("cmp x14, #5");                                         // does the mixed payload hold an associative array?
    emitter.instruction("b.eq __rt_hash_may_have_cyclic_values_yes");           // boxed hashes can participate in reference cycles
    emitter.instruction("cmp x14, #6");                                         // does the mixed payload hold an object?
    emitter.instruction("b.eq __rt_hash_may_have_cyclic_values_yes");           // boxed objects can participate in reference cycles
    emitter.instruction("cmp x14, #7");                                         // does the mixed payload wrap another mixed cell?
    emitter.instruction("b.ne __rt_hash_may_have_cyclic_values_next");          // scalar and string payloads do not need cycle collection
    emitter.instruction("ldr x15, [x15, #8]");                                  // follow the nested mixed pointer stored in value_lo
    emitter.instruction("cbz x15, __rt_hash_may_have_cyclic_values_next");      // null nested boxes terminate the scan safely
    emitter.instruction("b __rt_hash_may_have_cyclic_values_mixed_loop");       // continue unboxing nested mixed wrappers

    emitter.label("__rt_hash_may_have_cyclic_values_next");
    emitter.instruction("add x10, x10, #1");                                    // advance to the next slot
    emitter.instruction("b __rt_hash_may_have_cyclic_values_loop");             // keep scanning until we find a cyclic-capable payload

    emitter.label("__rt_hash_may_have_cyclic_values_yes");
    emitter.instruction("mov x0, #1");                                          // report that the hash may need cycle collection
    emitter.instruction("ret");                                                 // return true to the caller

    emitter.label("__rt_hash_may_have_cyclic_values_no");
    emitter.instruction("mov x0, #0");                                          // report that ordinary decref can skip cycle collection
    emitter.instruction("ret");                                                 // return false to the caller
}
