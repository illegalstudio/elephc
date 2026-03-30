use crate::codegen::emit::Emitter;

/// mixed_cast_bool: cast a boxed mixed payload to bool using the current scalar rules.
/// Input:  x0 = boxed mixed pointer
/// Output: x0 = boolean result
pub fn emit_mixed_cast_bool(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_cast_bool ---");
    emitter.label("__rt_mixed_cast_bool");

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
    emitter.instruction("cmp x2, #0");                                          // compare the string length against zero
    emitter.instruction("cset x0, ne");                                         // strings are truthy when non-empty under current cast rules
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
