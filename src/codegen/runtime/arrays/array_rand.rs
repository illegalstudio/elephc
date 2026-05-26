//! Purpose:
//! Emits the `__rt_array_rand`, `__rt_random_uniform` runtime helper assembly for array rand.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - Array helpers operate on runtime array headers and element cells; mutations must respect capacity and COW contracts.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the `__rt_array_rand` runtime helper.
///
/// Loads the array length from the header at `[x0]`, calls `__rt_random_uniform` to
/// sample a uniform index in the half-open range `[0, length)`, and returns the index in `x0`.
/// On x86_64 this delegates to the platform-specific `emit_array_rand_linux_x86_64`.
///
/// # ABI
/// - ARM64: x0 = array pointer (pointer to array header where length is at offset 0)
/// - ARM64: x0 = random index in `[0, length)` on return
/// - x86_64: rdi = array pointer, rax = random index on return
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

/// x86_64/Linux-specific emitter for `__rt_array_rand`.
/// Loads the array length from `[rdi]` into `rdi`, calls `__rt_random_uniform`, and returns
/// the sampled index in `rax`.
fn emit_array_rand_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_rand ---");
    emitter.label_global("__rt_array_rand");

    emitter.instruction("mov rdi, QWORD PTR [rdi]");                            // load the source indexed-array logical length into the x86_64 random-uniform bound register
    emitter.instruction("call __rt_random_uniform");                            // sample a random scalar index in the half-open range [0, length)
    emitter.instruction("ret");                                                 // return the sampled scalar index in rax
}
