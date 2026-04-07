use crate::codegen::emit::Emitter;

/// __rt_random_uniform: return a uniform random value in [0, x0).
pub fn emit_random_uniform(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: random_uniform ---");
    emitter.label_global("__rt_random_uniform");
    emitter.instruction("sub sp, sp, #32");                                     // allocate stack space for locals and saved frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish a frame pointer
    emitter.instruction("str w0, [sp, #0]");                                    // save the exclusive upper bound as a uint32
    emitter.instruction("cmp w0, #1");                                          // is the bound 0 or 1?
    emitter.instruction("b.hi __rt_random_uniform_calc");                       // no — continue with rejection sampling
    emitter.instruction("mov x0, #0");                                          // degenerate ranges always map to zero
    emitter.instruction("b __rt_random_uniform_done");                          // skip the sampling loop

    emitter.label("__rt_random_uniform_calc");
    emitter.instruction("neg w9, w0");                                          // compute 2^32 - bound modulo 2^32
    emitter.instruction("udiv w10, w9, w0");                                    // quotient = floor((2^32 - bound) / bound)
    emitter.instruction("msub w9, w10, w0, w9");                                // threshold = (2^32 % bound)
    emitter.instruction("str w9, [sp, #4]");                                    // save the rejection threshold

    emitter.label("__rt_random_uniform_loop");
    emitter.instruction("bl __rt_random_u32");                                  // generate a fresh uint32 candidate
    emitter.instruction("ldr w9, [sp, #4]");                                    // reload the rejection threshold
    emitter.instruction("cmp w0, w9");                                          // candidate below the biased prefix?
    emitter.instruction("b.lo __rt_random_uniform_loop");                       // yes — discard and resample
    emitter.instruction("ldr w1, [sp, #0]");                                    // reload the exclusive upper bound
    emitter.instruction("udiv w10, w0, w1");                                    // quotient = candidate / bound
    emitter.instruction("msub w0, w10, w1, w0");                                // remainder = candidate % bound

    emitter.label("__rt_random_uniform_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the temporary stack frame
    emitter.instruction("ret");                                                 // return the unbiased random value
}
