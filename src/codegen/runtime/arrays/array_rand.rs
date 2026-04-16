use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_rand: return a random key (index) from an integer array.
/// Input: x0 = array pointer
/// Output: x0 = random index in [0, length)
pub fn emit_array_rand(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_rand_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_rand ---");
    emitter.label_global("__rt_array_rand");

    // -- set up stack frame (needed for bl call) --
    emitter.instruction("sub sp, sp, #32");                                     // allocate 32 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // set up new frame pointer

    // -- get array length and generate random index --
    emitter.instruction("ldr x0, [x0]");                                        // x0 = array length
    emitter.instruction("bl __rt_random_uniform");                              // x0 = random value in [0, length)

    // -- tear down stack frame and return --
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = random index
}

fn emit_array_rand_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_rand ---");
    emitter.label_global("__rt_array_rand");

    emitter.instruction("mov rdi, QWORD PTR [rdi]");                            // load the source indexed-array logical length into the x86_64 random-uniform bound register
    emitter.instruction("call __rt_random_uniform");                            // sample a random scalar index in the half-open range [0, length)
    emitter.instruction("ret");                                                 // return the sampled scalar index in rax
}
