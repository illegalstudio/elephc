use crate::codegen::emit::Emitter;

/// array_rand: return a random key (index) from an integer array.
/// Input: x0 = array pointer
/// Output: x0 = random index in [0, length)
/// Uses _arc4random_uniform for random number generation.
pub fn emit_array_rand(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_rand ---");
    emitter.label("__rt_array_rand");

    // -- set up stack frame (needed for bl call) --
    emitter.instruction("sub sp, sp, #32");                                     // allocate 32 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // set up new frame pointer

    // -- get array length and generate random index --
    emitter.instruction("ldr x0, [x0]");                                        // x0 = array length
    emitter.instruction("bl _arc4random_uniform");                              // x0 = random value in [0, length)

    // -- tear down stack frame and return --
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = random index
}
