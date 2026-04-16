use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

pub fn emit_decref_array(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_decref_array_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: decref_array ---");
    emitter.label_global("__rt_decref_array");

    // -- null check --
    emitter.instruction("cbz x0, __rt_decref_array_skip");                      // skip if null pointer

    // -- heap range check: x0 >= _heap_buf --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_heap_buf");
    emitter.instruction("cmp x0, x9");                                          // is pointer below heap start?
    emitter.instruction("b.lo __rt_decref_array_skip");                         // yes — not a heap pointer, skip

    // -- heap range check: x0 < _heap_buf + _heap_off --
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_heap_off");
    emitter.instruction("ldr x10, [x10]");                                      // x10 = current heap offset
    emitter.instruction("add x10, x9, x10");                                    // x10 = heap_buf + heap_off = heap end
    emitter.instruction("cmp x0, x10");                                         // is pointer at or beyond heap end?
    emitter.instruction("b.hs __rt_decref_array_skip");                         // yes — not a valid heap pointer, skip

    // -- debug mode: reject decref on freed storage --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_heap_debug_enabled");
    emitter.instruction("ldr x9, [x9]");                                        // load the heap-debug enabled flag
    emitter.instruction("cbz x9, __rt_decref_array_checked");                   // skip debug validation when heap-debug mode is disabled
    emitter.instruction("str x30, [sp, #-16]!");                                // preserve the caller return address before nested validation
    emitter.instruction("bl __rt_heap_debug_check_live");                       // ensure the array block still has a live refcount
    emitter.instruction("ldr x30, [sp], #16");                                  // restore the caller return address after validation
    emitter.label("__rt_decref_array_checked");

    // -- decrement refcount and check for zero --
    emitter.instruction("ldr w9, [x0, #-12]");                                  // load 32-bit refcount from the uniform heap header
    emitter.instruction("subs w9, w9, #1");                                     // decrement refcount, set flags
    emitter.instruction("str w9, [x0, #-12]");                                  // store decremented refcount
    emitter.instruction("b.eq __rt_decref_array_free");                         // zero refcount means the array can be freed immediately

    // -- non-zero refcount may indicate a now-unrooted cycle; run the targeted collector unless it is already active --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_gc_release_suppressed");
    emitter.instruction("ldr x9, [x9]");                                        // load the release-suppression flag
    emitter.instruction("cbnz x9, __rt_decref_array_skip");                     // ordinary deep-free walks suppress nested collector runs
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_gc_collecting");
    emitter.instruction("ldr x9, [x9]");                                        // load the collector-active flag
    emitter.instruction("cbnz x9, __rt_decref_array_skip");                     // nested decref calls during collection must not restart the collector
    emitter.instruction("ldr x9, [x0, #-8]");                                   // load the packed array kind word from the heap header
    emitter.instruction("lsr x9, x9, #8");                                      // move the array value_type tag into the low bits
    emitter.instruction("and x9, x9, #0x7f");                                   // isolate the runtime array value_type tag without the persistent COW flag
    emitter.instruction("cmp x9, #4");                                          // is this an array of indexed arrays?
    emitter.instruction("b.eq __rt_decref_array_collect");                      // array-of-array cycles can require a collector pass
    emitter.instruction("cmp x9, #5");                                          // is this an array of associative arrays?
    emitter.instruction("b.eq __rt_decref_array_collect");                      // nested heap payloads can participate in cycles
    emitter.instruction("cmp x9, #6");                                          // is this an array of objects?
    emitter.instruction("b.ne __rt_decref_array_skip");                         // scalar/string arrays skip the targeted collector
    emitter.label("__rt_decref_array_collect");
    emitter.instruction("str x30, [sp, #-16]!");                                // preserve the caller return address across the collector call
    emitter.instruction("bl __rt_gc_collect_cycles");                           // reclaim any newly-unrooted refcounted graph components
    emitter.instruction("ldr x30, [sp], #16");                                  // restore the caller return address after collection
    emitter.instruction("b __rt_decref_array_skip");                            // return after the optional collection pass

    // -- refcount reached zero: deep free the array --
    emitter.label("__rt_decref_array_free");
    emitter.instruction("b __rt_array_free_deep");                              // tail-call to deep free array + elements

    emitter.label("__rt_decref_array_skip");
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_decref_array_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: decref_array ---");
    emitter.label_global("__rt_decref_array");

    emitter.instruction("test rax, rax");                                       // skip null array pointers so non-values do not participate in refcount traffic
    emitter.instruction("jz __rt_decref_array_skip");                           // null array pointers need no heap refcount update
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // load the stamped x86_64 heap kind word from the uniform header
    emitter.instruction("shr r10, 32");                                         // isolate the high-word heap marker used by the x86_64 heap wrapper
    emitter.instruction(&format!("cmp r10d, 0x{:x}", X86_64_HEAP_MAGIC_HI32));  // verify that the payload is owned by the x86_64 heap wrapper before mutating refcount state
    emitter.instruction("jne __rt_decref_array_skip");                          // skip foreign/static pointers that do not carry elephc heap headers
    emitter.instruction("mov r10d, DWORD PTR [rax - 12]");                      // load the 32-bit refcount stored in the uniform heap header
    emitter.instruction("sub r10d, 1");                                         // decrement the refcount for the array owner that is going away
    emitter.instruction("mov DWORD PTR [rax - 12], r10d");                      // persist the decremented array refcount in the uniform heap header
    emitter.instruction("jz __rt_decref_array_free");                           // zero refcount means the indexed array can be deep-freed immediately
    crate::codegen::abi::emit_symbol_address(emitter, "r11", "_gc_release_suppressed");
    emitter.instruction("mov r11, QWORD PTR [r11]");                            // load the release-suppression flag before considering a targeted cycle-collector run
    emitter.instruction("test r11, r11");                                       // is this decref happening inside an ordinary deep-free walk?
    emitter.instruction("jnz __rt_decref_array_skip");                          // yes — nested collector runs stay suppressed during deep frees
    crate::codegen::abi::emit_symbol_address(emitter, "r11", "_gc_collecting");
    emitter.instruction("mov r11, QWORD PTR [r11]");                            // load the collector-active flag before attempting another collection pass
    emitter.instruction("test r11, r11");                                       // is the collector already running?
    emitter.instruction("jnz __rt_decref_array_skip");                          // yes — nested decref calls during collection must not restart the collector
    emitter.instruction("mov r11, QWORD PTR [rax - 8]");                        // load the packed array kind word to inspect the runtime array value_type tag
    emitter.instruction("shr r11, 8");                                          // move the packed array value_type tag into the low bits
    emitter.instruction("and r11, 0x7f");                                       // isolate the runtime array value_type without the persistent COW flag bit
    emitter.instruction("cmp r11, 4");                                          // does this array carry refcounted element payloads that can participate in cycles?
    emitter.instruction("jb __rt_decref_array_skip");                           // scalar and string arrays do not require a targeted cycle-collector pass
    emitter.instruction("cmp r11, 7");                                          // is the array value_type within the supported refcounted range?
    emitter.instruction("ja __rt_decref_array_skip");                           // unknown array payload tags are ignored by the x86_64 collector trigger
    emitter.instruction("call __rt_gc_collect_cycles");                         // reclaim any newly unrooted refcounted graph components reachable from arrays
    emitter.instruction("jmp __rt_decref_array_skip");                          // return after the optional x86_64 collector pass

    emitter.label("__rt_decref_array_free");
    emitter.instruction("jmp __rt_array_free_deep");                            // tail-call into the indexed-array deep-free helper so nested heap-backed elements are released too
    emitter.label("__rt_decref_array_skip");
    emitter.instruction("ret");                                                 // return to the caller after the optional x86_64 array refcount update
}
