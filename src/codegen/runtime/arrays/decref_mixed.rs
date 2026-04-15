use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

pub fn emit_decref_mixed(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_decref_mixed_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: decref_mixed ---");
    emitter.label_global("__rt_decref_mixed");

    emitter.instruction("cbz x0, __rt_decref_mixed_skip");                      // skip null mixed pointers immediately
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_heap_buf");
    emitter.instruction("cmp x0, x9");                                          // is pointer below heap start?
    emitter.instruction("b.lo __rt_decref_mixed_skip");                         // non-heap pointers need no mixed decref
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_heap_off");
    emitter.instruction("ldr x10, [x10]");                                      // x10 = current heap offset
    emitter.instruction("add x10, x9, x10");                                    // compute the current heap end
    emitter.instruction("cmp x0, x10");                                         // is pointer at or beyond heap end?
    emitter.instruction("b.hs __rt_decref_mixed_skip");                         // invalid heap pointers must be ignored here

    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_heap_debug_enabled");
    emitter.instruction("ldr x9, [x9]");                                        // load the heap-debug enabled flag
    emitter.instruction("cbz x9, __rt_decref_mixed_checked");                   // skip debug validation when heap-debug mode is disabled
    emitter.instruction("str x30, [sp, #-16]!");                                // preserve return address before nested validation
    emitter.instruction("bl __rt_heap_debug_check_live");                       // ensure the mixed cell is still live
    emitter.instruction("ldr x30, [sp], #16");                                  // restore return address after validation
    emitter.label("__rt_decref_mixed_checked");

    emitter.instruction("ldr w9, [x0, #-12]");                                  // load the mixed cell refcount from the uniform header
    emitter.instruction("subs w9, w9, #1");                                     // decrement the mixed cell refcount and set flags
    emitter.instruction("str w9, [x0, #-12]");                                  // store the decremented mixed cell refcount
    emitter.instruction("b.eq __rt_decref_mixed_free");                         // zero refcount means the boxed payload can be released now

    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_gc_release_suppressed");
    emitter.instruction("ldr x9, [x9]");                                        // load the release-suppression flag
    emitter.instruction("cbnz x9, __rt_decref_mixed_skip");                     // ordinary deep-free walks suppress nested collector runs
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_gc_collecting");
    emitter.instruction("ldr x9, [x9]");                                        // load the collector-active flag
    emitter.instruction("cbnz x9, __rt_decref_mixed_skip");                     // nested decref calls during collection must not restart the collector
    emitter.instruction("ldr x9, [x0]");                                        // load the boxed runtime payload tag
    emitter.instruction("cmp x9, #4");                                          // does the mixed cell point to an indexed array?
    emitter.instruction("b.eq __rt_decref_mixed_collect");                      // refcounted boxed children can participate in cycles
    emitter.instruction("cmp x9, #5");                                          // does the mixed cell point to an associative array?
    emitter.instruction("b.eq __rt_decref_mixed_collect");                      // refcounted boxed children can participate in cycles
    emitter.instruction("cmp x9, #6");                                          // does the mixed cell point to an object?
    emitter.instruction("b.eq __rt_decref_mixed_collect");                      // refcounted boxed children can participate in cycles
    emitter.instruction("cmp x9, #7");                                          // does the mixed cell point to another mixed cell?
    emitter.instruction("b.ne __rt_decref_mixed_skip");                         // scalar/string children cannot participate in heap cycles
    emitter.label("__rt_decref_mixed_collect");
    emitter.instruction("str x30, [sp, #-16]!");                                // preserve return address across the collector call
    emitter.instruction("bl __rt_gc_collect_cycles");                           // reclaim any newly-unrooted graph components
    emitter.instruction("ldr x30, [sp], #16");                                  // restore return address after the collector call
    emitter.instruction("b __rt_decref_mixed_skip");                            // return after the optional collection pass

    emitter.label("__rt_decref_mixed_free");
    emitter.instruction("b __rt_mixed_free_deep");                              // tail-call to deep free the mixed cell and its boxed child

    emitter.label("__rt_decref_mixed_skip");
    emitter.instruction("ret");                                                 // nothing to release
}

fn emit_decref_mixed_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: decref_mixed ---");
    emitter.label_global("__rt_decref_mixed");

    emitter.instruction("test rax, rax");                                       // skip null mixed pointers immediately because they do not own heap storage
    emitter.instruction("jz __rt_decref_mixed_skip");                           // null mixed values need no release work
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // load the stamped x86_64 heap kind word from the uniform header
    emitter.instruction("shr r10, 32");                                         // isolate the high-word heap marker used by the x86_64 heap wrapper
    emitter.instruction(&format!("cmp r10d, 0x{:x}", X86_64_HEAP_MAGIC_HI32));  // ignore foreign pointers that do not carry the elephc x86_64 heap marker
    emitter.instruction("jne __rt_decref_mixed_skip");                          // only elephc-owned mixed boxes participate in x86_64 decref bookkeeping
    emitter.instruction("mov r10d, DWORD PTR [rax - 12]");                      // load the 32-bit mixed-box refcount from the uniform heap header
    emitter.instruction("sub r10d, 1");                                         // decrement the mixed-box refcount for the releasing x86_64 owner
    emitter.instruction("mov DWORD PTR [rax - 12], r10d");                      // store the decremented mixed-box refcount back into the uniform heap header
    emitter.instruction("jz __rt_decref_mixed_free");                           // zero refcount means the boxed payload can be released now
    crate::codegen::abi::emit_symbol_address(emitter, "r11", "_gc_release_suppressed");
    emitter.instruction("mov r11, QWORD PTR [r11]");                            // load the release-suppression flag before considering a targeted cycle-collector run
    emitter.instruction("test r11, r11");                                       // is this decref happening inside an ordinary deep-free walk?
    emitter.instruction("jnz __rt_decref_mixed_skip");                          // yes — nested collector runs stay suppressed during deep frees
    crate::codegen::abi::emit_symbol_address(emitter, "r11", "_gc_collecting");
    emitter.instruction("mov r11, QWORD PTR [r11]");                            // load the collector-active flag before attempting another collection pass
    emitter.instruction("test r11, r11");                                       // is the collector already running?
    emitter.instruction("jnz __rt_decref_mixed_skip");                          // yes — nested decref calls during collection must not restart the collector
    emitter.instruction("mov r11, QWORD PTR [rax]");                            // load the boxed mixed runtime value_tag before deciding whether it can participate in cycles
    emitter.instruction("cmp r11, 4");                                          // does this mixed box currently hold a heap-backed child?
    emitter.instruction("jb __rt_decref_mixed_skip");                           // scalar, string, and null boxed values cannot participate in heap cycles
    emitter.instruction("cmp r11, 7");                                          // is the boxed runtime tag within the supported heap-backed range?
    emitter.instruction("ja __rt_decref_mixed_skip");                           // unknown boxed runtime tags are ignored by the x86_64 collector trigger
    emitter.instruction("call __rt_gc_collect_cycles");                         // reclaim any newly unrooted graph components reachable through boxed mixed values
    emitter.instruction("jmp __rt_decref_mixed_skip");                          // return after the optional x86_64 collector pass
    emitter.label("__rt_decref_mixed_skip");
    emitter.instruction("ret");                                                 // nothing else needs to happen for non-zero refcounts or foreign pointers

    emitter.label("__rt_decref_mixed_free");
    emitter.instruction("jmp __rt_mixed_free_deep");                            // tail-call to deep free the mixed box once the last owner is gone
}
