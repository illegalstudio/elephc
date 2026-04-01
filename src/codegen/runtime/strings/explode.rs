use crate::codegen::emit::Emitter;

/// explode: split string by delimiter into array of strings.
/// Input: x1/x2=delimiter, x3/x4=string
/// Output: x0 = array pointer
pub fn emit_explode(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: explode ---");
    emitter.label_global("__rt_explode");

    // -- set up stack frame (80 bytes) --
    emitter.instruction("sub sp, sp, #80");                                     // allocate 80 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish new frame pointer
    emitter.instruction("stp x1, x2, [sp]");                                    // save delimiter ptr and length
    emitter.instruction("stp x3, x4, [sp, #16]");                               // save input string ptr and length

    // -- create a new string array --
    emitter.instruction("mov x0, #16");                                         // initial array capacity = 16 elements
    emitter.instruction("mov x1, #16");                                         // element size = 16 bytes (ptr + len)
    emitter.instruction("bl __rt_array_new");                                   // call array constructor, returns array in x0
    emitter.instruction("str x0, [sp, #32]");                                   // save array pointer on stack

    // -- initialize scan state --
    emitter.instruction("mov x13, #0");                                         // current scan position = 0
    emitter.instruction("str x13, [sp, #40]");                                  // save current scan position
    emitter.instruction("str x13, [sp, #48]");                                  // segment start = 0

    // -- main loop: scan for delimiter occurrences --
    emitter.label("__rt_explode_loop");
    emitter.instruction("ldp x3, x4, [sp, #16]");                               // reload string ptr and length
    emitter.instruction("ldr x13, [sp, #40]");                                  // reload current scan position
    emitter.instruction("cmp x13, x4");                                         // check if past end of string
    emitter.instruction("b.ge __rt_explode_last");                              // if done, push final segment

    // -- check if delimiter fits at current position --
    emitter.instruction("ldp x1, x2, [sp]");                                    // reload delimiter ptr and length
    emitter.instruction("sub x14, x4, x13");                                    // remaining = string_len - scan_pos
    emitter.instruction("cmp x2, x14");                                         // check if delimiter fits in remaining
    emitter.instruction("b.gt __rt_explode_last");                              // delimiter longer than remaining, done

    // -- compare delimiter at current position --
    emitter.instruction("mov x15, #0");                                         // delimiter comparison index = 0
    emitter.label("__rt_explode_cmp");
    emitter.instruction("cmp x15, x2");                                         // check if all delimiter bytes matched
    emitter.instruction("b.ge __rt_explode_match");                             // full match, delimiter found
    emitter.instruction("add x16, x13, x15");                                   // compute string index = scan_pos + cmp_idx
    emitter.instruction("ldrb w17, [x3, x16]");                                 // load string byte at computed index
    emitter.instruction("ldrb w18, [x1, x15]");                                 // load delimiter byte at cmp index
    emitter.instruction("cmp w17, w18");                                        // compare string and delimiter bytes
    emitter.instruction("b.ne __rt_explode_advance");                           // mismatch, advance by 1
    emitter.instruction("add x15, x15, #1");                                    // advance delimiter index
    emitter.instruction("b __rt_explode_cmp");                                  // continue comparing

    // -- no match: advance scan position by 1 --
    emitter.label("__rt_explode_advance");
    emitter.instruction("add x13, x13, #1");                                    // move scan position forward by 1
    emitter.instruction("str x13, [sp, #40]");                                  // save updated scan position
    emitter.instruction("b __rt_explode_loop");                                 // continue scanning

    // -- delimiter found: push segment before it to array --
    emitter.label("__rt_explode_match");
    emitter.instruction("ldr x0, [sp, #32]");                                   // load array pointer
    emitter.instruction("ldp x3, x4, [sp, #16]");                               // reload string ptr and length
    emitter.instruction("ldr x16, [sp, #48]");                                  // load segment start position
    emitter.instruction("add x1, x3, x16");                                     // segment ptr = string + segment_start
    emitter.instruction("sub x2, x13, x16");                                    // segment len = scan_pos - segment_start
    emitter.instruction("bl __rt_array_push_str");                              // push segment string to array
    emitter.instruction("str x0, [sp, #32]");                                   // update array pointer after possible realloc

    // -- advance past delimiter, update segment start --
    emitter.instruction("ldp x1, x2, [sp]");                                    // reload delimiter ptr and length
    emitter.instruction("ldr x13, [sp, #40]");                                  // reload scan position
    emitter.instruction("add x13, x13, x2");                                    // skip past delimiter
    emitter.instruction("str x13, [sp, #40]");                                  // save new scan position
    emitter.instruction("str x13, [sp, #48]");                                  // update segment start to after delimiter
    emitter.instruction("b __rt_explode_loop");                                 // continue scanning

    // -- push final segment (from last delimiter to end of string) --
    emitter.label("__rt_explode_last");
    emitter.instruction("ldr x0, [sp, #32]");                                   // load array pointer
    emitter.instruction("ldp x3, x4, [sp, #16]");                               // reload string ptr and length
    emitter.instruction("ldr x16, [sp, #48]");                                  // load segment start position
    emitter.instruction("add x1, x3, x16");                                     // segment ptr = string + segment_start
    emitter.instruction("sub x2, x4, x16");                                     // segment len = string_len - segment_start
    emitter.instruction("bl __rt_array_push_str");                              // push final segment to array
    emitter.instruction("str x0, [sp, #32]");                                   // update array pointer after possible realloc

    // -- return array and restore frame --
    emitter.instruction("ldr x0, [sp, #32]");                                   // return array pointer in x0
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
