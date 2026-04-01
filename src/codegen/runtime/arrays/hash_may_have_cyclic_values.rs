use crate::codegen::emit::Emitter;

/// hash_may_have_cyclic_values: detect whether any entry can participate in a cycle.
/// Input:  x0 = hash table pointer
/// Output: x0 = 1 if the hash contains array/hash/object/mixed graph values, else 0
pub fn emit_hash_may_have_cyclic_values(emitter: &mut Emitter) {
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
