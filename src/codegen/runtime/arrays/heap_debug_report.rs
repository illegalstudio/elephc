use crate::codegen::emit::Emitter;

/// heap_debug_report: print allocator/debug summary and leak info to stderr.
pub fn emit_heap_debug_report(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: heap_debug_report ---");
    emitter.label_global("__rt_heap_debug_report");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #64");                                     // allocate a stack frame for saved temporaries and counters
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up the frame pointer

    // -- compute alloc/free/live counters once for the full report --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_gc_allocs");
    emitter.instruction("ldr x9, [x9]");                                        // load the total allocation count
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_gc_frees");
    emitter.instruction("ldr x10, [x10]");                                      // load the total free count
    emitter.instruction("sub x11, x9, x10");                                    // live_blocks = allocs - frees
    crate::codegen::abi::emit_symbol_address(emitter, "x12", "_gc_live");
    emitter.instruction("ldr x12, [x12]");                                      // load current live bytes
    emitter.instruction("str x9, [sp, #24]");                                   // save alloc count across syscalls and nested itoa calls
    emitter.instruction("str x10, [sp, #16]");                                  // save free count across nested itoa calls
    emitter.instruction("str x11, [sp, #0]");                                   // save live block count for the second line
    emitter.instruction("str x12, [sp, #8]");                                   // save live bytes for the second line

    // -- print summary prefix --
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.adrp("x1", "_heap_dbg_stats_prefix");                // load page of the heap-debug summary prefix
    emitter.add_lo12("x1", "x1", "_heap_dbg_stats_prefix");          // resolve the heap-debug summary prefix address
    emitter.instruction("mov x2, #19");                                         // "HEAP DEBUG: allocs=" length
    emitter.syscall(4);

    // -- print alloc count --
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload alloc count after the write syscall
    emitter.instruction("bl __rt_itoa");                                        // convert alloc count to decimal string
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.syscall(4);

    // -- print frees label and count --
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.adrp("x1", "_heap_dbg_frees_label");                 // load page of the frees label
    emitter.add_lo12("x1", "x1", "_heap_dbg_frees_label");           // resolve the frees label address
    emitter.instruction("mov x2, #7");                                          // " frees=" length
    emitter.syscall(4);
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload free count after the previous itoa call
    emitter.instruction("bl __rt_itoa");                                        // convert free count to decimal string
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.syscall(4);

    // -- print live_blocks label and count --
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.adrp("x1", "_heap_dbg_live_blocks_label");           // load page of the live-block label
    emitter.add_lo12("x1", "x1", "_heap_dbg_live_blocks_label");     // resolve the live-block label address
    emitter.instruction("mov x2, #13");                                         // " live_blocks=" length
    emitter.syscall(4);
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload live block count for decimal conversion
    emitter.instruction("bl __rt_itoa");                                        // convert live block count to decimal string
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.syscall(4);

    // -- print live_bytes label and count --
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.adrp("x1", "_heap_dbg_live_bytes_label");            // load page of the live-bytes label
    emitter.add_lo12("x1", "x1", "_heap_dbg_live_bytes_label");      // resolve the live-bytes label address
    emitter.instruction("mov x2, #12");                                         // " live_bytes=" length
    emitter.syscall(4);
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload live bytes for decimal conversion
    emitter.instruction("bl __rt_itoa");                                        // convert live bytes to decimal string
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.syscall(4);

    // -- print peak_live_bytes label and count --
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.adrp("x1", "_heap_dbg_peak_label");                  // load page of the peak-live-bytes label
    emitter.add_lo12("x1", "x1", "_heap_dbg_peak_label");            // resolve the peak-live-bytes label address
    emitter.instruction("mov x2, #17");                                         // " peak_live_bytes=" length
    emitter.syscall(4);
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_gc_peak");
    emitter.instruction("ldr x0, [x9]");                                        // load peak live bytes
    emitter.instruction("bl __rt_itoa");                                        // convert peak live bytes to decimal string
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.syscall(4);
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.adrp("x1", "_heap_dbg_newline");                     // load page of the newline label
    emitter.add_lo12("x1", "x1", "_heap_dbg_newline");               // resolve the newline label address
    emitter.instruction("mov x2, #1");                                          // newline length
    emitter.syscall(4);

    // -- print leak-summary prefix --
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.adrp("x1", "_heap_dbg_leak_prefix");                 // load page of the leak-summary prefix
    emitter.add_lo12("x1", "x1", "_heap_dbg_leak_prefix");           // resolve the leak-summary prefix address
    emitter.instruction("mov x2, #26");                                         // "HEAP DEBUG: leak summary: " length
    emitter.syscall(4);

    // -- either print "clean" or the live leak counters --
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload live block count for the leak summary branch
    emitter.instruction("cbnz x9, __rt_heap_debug_report_leak_details");        // nonzero live blocks mean there is still heap state outstanding at exit
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.adrp("x1", "_heap_dbg_clean_label");                 // load page of the clean summary label
    emitter.add_lo12("x1", "x1", "_heap_dbg_clean_label");           // resolve the clean summary label address
    emitter.instruction("mov x2, #6");                                          // "clean\n" length
    emitter.syscall(4);
    emitter.instruction("b __rt_heap_debug_report_done");                       // skip the leak-detail path once the clean summary is written

    emitter.label("__rt_heap_debug_report_leak_details");
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.adrp("x1", "_heap_dbg_live_blocks_short_label");     // load page of the short live-block label
    emitter.add_lo12("x1", "x1", "_heap_dbg_live_blocks_short_label"); //resolve the short live-block label address
    emitter.instruction("mov x2, #12");                                         // "live_blocks=" length
    emitter.syscall(4);
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload live block count for decimal conversion
    emitter.instruction("bl __rt_itoa");                                        // convert live block count to decimal string
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.syscall(4);
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.adrp("x1", "_heap_dbg_live_bytes_label");            // load page of the live-bytes label
    emitter.add_lo12("x1", "x1", "_heap_dbg_live_bytes_label");      // resolve the live-bytes label address
    emitter.instruction("mov x2, #12");                                         // " live_bytes=" length
    emitter.syscall(4);
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload live bytes for decimal conversion
    emitter.instruction("bl __rt_itoa");                                        // convert live bytes to decimal string
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.syscall(4);
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.adrp("x1", "_heap_dbg_newline");                     // load page of the newline label
    emitter.add_lo12("x1", "x1", "_heap_dbg_newline");               // resolve the newline label address
    emitter.instruction("mov x2, #1");                                          // newline length
    emitter.syscall(4);

    emitter.label("__rt_heap_debug_report_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // tear down the heap-debug report frame
    emitter.instruction("ret");                                                 // return to the caller
}
