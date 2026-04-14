use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// __rt_random_uniform: return a uniform random value in [0, x0).
pub fn emit_random_uniform(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_random_uniform_linux_x86_64(emitter);
        return;
    }

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

fn emit_random_uniform_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: random_uniform ---");
    emitter.label_global("__rt_random_uniform");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving x86_64 rejection-sampling scratch slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved bound and rejection threshold
    emitter.instruction("sub rsp, 16");                                         // reserve aligned stack space for the uint32 bound and rejection threshold across helper calls
    emitter.instruction("mov DWORD PTR [rbp - 4], edi");                        // preserve the exclusive upper bound as a uint32 across the rejection-sampling loop
    emitter.instruction("cmp edi, 1");                                          // detect degenerate bounds that only admit the zero result
    emitter.instruction("ja __rt_random_uniform_calc_x86");                     // continue with rejection sampling only when the exclusive upper bound exceeds one
    emitter.instruction("xor eax, eax");                                        // degenerate ranges always map to the scalar zero result
    emitter.instruction("jmp __rt_random_uniform_done_x86");                    // skip the rejection-sampling loop on degenerate bounds

    emitter.label("__rt_random_uniform_calc_x86");
    emitter.instruction("xor eax, eax");                                        // seed the threshold dividend from zero before subtracting the exclusive upper bound modulo 2^32
    emitter.instruction("sub eax, DWORD PTR [rbp - 4]");                        // compute 2^32 - bound modulo 2^32 using 32-bit wraparound arithmetic
    emitter.instruction("xor edx, edx");                                        // clear the high dividend half before computing the uint32 rejection threshold remainder
    emitter.instruction("div DWORD PTR [rbp - 4]");                             // divide the wrapped dividend by the bound so edx becomes the rejection threshold
    emitter.instruction("mov DWORD PTR [rbp - 8], edx");                        // preserve the uint32 rejection threshold across random_u32 helper calls

    emitter.label("__rt_random_uniform_loop_x86");
    emitter.instruction("call __rt_random_u32");                                // generate a fresh uint32 candidate for the rejection-sampling loop
    emitter.instruction("cmp eax, DWORD PTR [rbp - 8]");                        // compare the candidate against the rejection threshold that removes modulo bias
    emitter.instruction("jb __rt_random_uniform_loop_x86");                     // discard biased candidates that fall below the rejection threshold
    emitter.instruction("xor edx, edx");                                        // clear the high dividend half before reducing the unbiased uint32 candidate modulo the bound
    emitter.instruction("div DWORD PTR [rbp - 4]");                             // divide the unbiased uint32 candidate by the bound so edx becomes candidate % bound
    emitter.instruction("mov eax, edx");                                        // return the unbiased remainder as the sampled scalar value in [0, bound)

    emitter.label("__rt_random_uniform_done_x86");
    emitter.instruction("add rsp, 16");                                         // release the rejection-sampling scratch slots before returning to the caller
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the x86_64 rejection-sampling helper completes
    emitter.instruction("ret");                                                 // return the unbiased random value in eax
}
