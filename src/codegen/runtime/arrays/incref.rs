use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

pub fn emit_incref(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_incref_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: incref ---");
    emitter.label_global("__rt_incref");

    // -- null check --
    emitter.instruction("cbz x0, __rt_incref_skip");                            // skip if null pointer

    // -- heap range check: x0 >= _heap_buf --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_heap_buf");
    emitter.instruction("cmp x0, x9");                                          // is pointer below heap start?
    emitter.instruction("b.lo __rt_incref_skip");                               // yes — not a heap pointer, skip

    // -- heap range check: x0 < _heap_buf + _heap_off --
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_heap_off");
    emitter.instruction("ldr x10, [x10]");                                      // x10 = current heap offset
    emitter.instruction("add x10, x9, x10");                                    // x10 = heap_buf + heap_off = heap end
    emitter.instruction("cmp x0, x10");                                         // is pointer at or beyond heap end?
    emitter.instruction("b.hs __rt_incref_skip");                               // yes — not a valid heap pointer, skip

    // -- debug mode: reject incref on freed storage --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_heap_debug_enabled");
    emitter.instruction("ldr x9, [x9]");                                        // load the heap-debug enabled flag
    emitter.instruction("cbz x9, __rt_incref_checked");                         // skip debug validation when heap-debug mode is disabled
    emitter.instruction("str x30, [sp, #-16]!");                                // preserve the caller return address before nested validation
    emitter.instruction("bl __rt_heap_debug_check_live");                       // ensure the referenced heap block still has a live refcount
    emitter.instruction("ldr x30, [sp], #16");                                  // restore the caller return address after validation
    emitter.label("__rt_incref_checked");

    // -- increment refcount --
    emitter.instruction("ldr w9, [x0, #-12]");                                  // load 32-bit refcount from the uniform heap header
    emitter.instruction("add w9, w9, #1");                                      // increment refcount
    emitter.instruction("str w9, [x0, #-12]");                                  // store incremented refcount

    emitter.label("__rt_incref_skip");
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_incref_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: incref ---");
    emitter.label_global("__rt_incref");
    emitter.instruction("ret");                                                 // minimal x86_64 runtime does not refcount heap payloads yet
}
