use crate::codegen::emit::Emitter;

/// heap_debug_report: print allocator/debug summary and leak info to stderr.
pub fn emit_heap_debug_report(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: heap_debug_report ---");
    emitter.label("__rt_heap_debug_report");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #64");                                     // allocate a stack frame for saved temporaries and counters
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up the frame pointer

    // -- compute alloc/free/live counters once for the full report --
    emitter.instruction("adrp x9, _gc_allocs@PAGE");                            // load page of the allocation counter
    emitter.instruction("add x9, x9, _gc_allocs@PAGEOFF");                      // resolve the allocation counter address
    emitter.instruction("ldr x9, [x9]");                                        // load the total allocation count
    emitter.instruction("adrp x10, _gc_frees@PAGE");                            // load page of the free counter
    emitter.instruction("add x10, x10, _gc_frees@PAGEOFF");                     // resolve the free counter address
    emitter.instruction("ldr x10, [x10]");                                      // load the total free count
    emitter.instruction("sub x11, x9, x10");                                    // live_blocks = allocs - frees
    emitter.instruction("adrp x12, _gc_live@PAGE");                             // load page of the live-byte counter
    emitter.instruction("add x12, x12, _gc_live@PAGEOFF");                      // resolve the live-byte counter address
    emitter.instruction("ldr x12, [x12]");                                      // load current live bytes
    emitter.instruction("str x9, [sp, #24]");                                   // save alloc count across syscalls and nested itoa calls
    emitter.instruction("str x10, [sp, #16]");                                  // save free count across nested itoa calls
    emitter.instruction("str x11, [sp, #0]");                                   // save live block count for the second line
    emitter.instruction("str x12, [sp, #8]");                                   // save live bytes for the second line

    // -- print summary prefix --
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.instruction("adrp x1, _heap_dbg_stats_prefix@PAGE");                // load page of the heap-debug summary prefix
    emitter.instruction("add x1, x1, _heap_dbg_stats_prefix@PAGEOFF");          // resolve the heap-debug summary prefix address
    emitter.instruction("mov x2, #19");                                         // "HEAP DEBUG: allocs=" length
    emitter.instruction("mov x16, #4");                                         // syscall 4 = sys_write
    emitter.instruction("svc #0x80");                                           // write the summary prefix

    // -- print alloc count --
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload alloc count after the write syscall
    emitter.instruction("bl __rt_itoa");                                        // convert alloc count to decimal string
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.instruction("mov x16, #4");                                         // syscall 4 = sys_write
    emitter.instruction("svc #0x80");                                           // write alloc count

    // -- print frees label and count --
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.instruction("adrp x1, _heap_dbg_frees_label@PAGE");                 // load page of the frees label
    emitter.instruction("add x1, x1, _heap_dbg_frees_label@PAGEOFF");           // resolve the frees label address
    emitter.instruction("mov x2, #7");                                          // " frees=" length
    emitter.instruction("mov x16, #4");                                         // syscall 4 = sys_write
    emitter.instruction("svc #0x80");                                           // write the frees label
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload free count after the previous itoa call
    emitter.instruction("bl __rt_itoa");                                        // convert free count to decimal string
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.instruction("mov x16, #4");                                         // syscall 4 = sys_write
    emitter.instruction("svc #0x80");                                           // write free count

    // -- print live_blocks label and count --
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.instruction("adrp x1, _heap_dbg_live_blocks_label@PAGE");           // load page of the live-block label
    emitter.instruction("add x1, x1, _heap_dbg_live_blocks_label@PAGEOFF");     // resolve the live-block label address
    emitter.instruction("mov x2, #13");                                         // " live_blocks=" length
    emitter.instruction("mov x16, #4");                                         // syscall 4 = sys_write
    emitter.instruction("svc #0x80");                                           // write the live-block label
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload live block count for decimal conversion
    emitter.instruction("bl __rt_itoa");                                        // convert live block count to decimal string
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.instruction("mov x16, #4");                                         // syscall 4 = sys_write
    emitter.instruction("svc #0x80");                                           // write live block count

    // -- print live_bytes label and count --
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.instruction("adrp x1, _heap_dbg_live_bytes_label@PAGE");            // load page of the live-bytes label
    emitter.instruction("add x1, x1, _heap_dbg_live_bytes_label@PAGEOFF");      // resolve the live-bytes label address
    emitter.instruction("mov x2, #12");                                         // " live_bytes=" length
    emitter.instruction("mov x16, #4");                                         // syscall 4 = sys_write
    emitter.instruction("svc #0x80");                                           // write the live-bytes label
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload live bytes for decimal conversion
    emitter.instruction("bl __rt_itoa");                                        // convert live bytes to decimal string
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.instruction("mov x16, #4");                                         // syscall 4 = sys_write
    emitter.instruction("svc #0x80");                                           // write live bytes

    // -- print peak_live_bytes label and count --
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.instruction("adrp x1, _heap_dbg_peak_label@PAGE");                  // load page of the peak-live-bytes label
    emitter.instruction("add x1, x1, _heap_dbg_peak_label@PAGEOFF");            // resolve the peak-live-bytes label address
    emitter.instruction("mov x2, #17");                                         // " peak_live_bytes=" length
    emitter.instruction("mov x16, #4");                                         // syscall 4 = sys_write
    emitter.instruction("svc #0x80");                                           // write the peak-live-bytes label
    emitter.instruction("adrp x9, _gc_peak@PAGE");                              // load page of the peak-live-bytes counter
    emitter.instruction("add x9, x9, _gc_peak@PAGEOFF");                        // resolve the peak-live-bytes counter address
    emitter.instruction("ldr x0, [x9]");                                        // load peak live bytes
    emitter.instruction("bl __rt_itoa");                                        // convert peak live bytes to decimal string
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.instruction("mov x16, #4");                                         // syscall 4 = sys_write
    emitter.instruction("svc #0x80");                                           // write peak live bytes
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.instruction("adrp x1, _heap_dbg_newline@PAGE");                     // load page of the newline label
    emitter.instruction("add x1, x1, _heap_dbg_newline@PAGEOFF");               // resolve the newline label address
    emitter.instruction("mov x2, #1");                                          // newline length
    emitter.instruction("mov x16, #4");                                         // syscall 4 = sys_write
    emitter.instruction("svc #0x80");                                           // terminate the summary line

    // -- print leak-summary prefix --
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.instruction("adrp x1, _heap_dbg_leak_prefix@PAGE");                 // load page of the leak-summary prefix
    emitter.instruction("add x1, x1, _heap_dbg_leak_prefix@PAGEOFF");           // resolve the leak-summary prefix address
    emitter.instruction("mov x2, #26");                                         // "HEAP DEBUG: leak summary: " length
    emitter.instruction("mov x16, #4");                                         // syscall 4 = sys_write
    emitter.instruction("svc #0x80");                                           // write the leak-summary prefix

    // -- either print "clean" or the live leak counters --
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload live block count for the leak summary branch
    emitter.instruction("cbnz x9, __rt_heap_debug_report_leak_details");        // nonzero live blocks mean there is still heap state outstanding at exit
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.instruction("adrp x1, _heap_dbg_clean_label@PAGE");                 // load page of the clean summary label
    emitter.instruction("add x1, x1, _heap_dbg_clean_label@PAGEOFF");           // resolve the clean summary label address
    emitter.instruction("mov x2, #6");                                          // "clean\n" length
    emitter.instruction("mov x16, #4");                                         // syscall 4 = sys_write
    emitter.instruction("svc #0x80");                                           // write the clean leak summary
    emitter.instruction("b __rt_heap_debug_report_done");                       // skip the leak-detail path once the clean summary is written

    emitter.label("__rt_heap_debug_report_leak_details");
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.instruction("adrp x1, _heap_dbg_live_blocks_short_label@PAGE");     // load page of the short live-block label
    emitter.instruction("add x1, x1, _heap_dbg_live_blocks_short_label@PAGEOFF"); //resolve the short live-block label address
    emitter.instruction("mov x2, #12");                                         // "live_blocks=" length
    emitter.instruction("mov x16, #4");                                         // syscall 4 = sys_write
    emitter.instruction("svc #0x80");                                           // write the short live-block label
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload live block count for decimal conversion
    emitter.instruction("bl __rt_itoa");                                        // convert live block count to decimal string
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.instruction("mov x16, #4");                                         // syscall 4 = sys_write
    emitter.instruction("svc #0x80");                                           // write live block count
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.instruction("adrp x1, _heap_dbg_live_bytes_label@PAGE");            // load page of the live-bytes label
    emitter.instruction("add x1, x1, _heap_dbg_live_bytes_label@PAGEOFF");      // resolve the live-bytes label address
    emitter.instruction("mov x2, #12");                                         // " live_bytes=" length
    emitter.instruction("mov x16, #4");                                         // syscall 4 = sys_write
    emitter.instruction("svc #0x80");                                           // write the live-bytes label
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload live bytes for decimal conversion
    emitter.instruction("bl __rt_itoa");                                        // convert live bytes to decimal string
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.instruction("mov x16, #4");                                         // syscall 4 = sys_write
    emitter.instruction("svc #0x80");                                           // write live bytes
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.instruction("adrp x1, _heap_dbg_newline@PAGE");                     // load page of the newline label
    emitter.instruction("add x1, x1, _heap_dbg_newline@PAGEOFF");               // resolve the newline label address
    emitter.instruction("mov x2, #1");                                          // newline length
    emitter.instruction("mov x16, #4");                                         // syscall 4 = sys_write
    emitter.instruction("svc #0x80");                                           // terminate the leak-detail line

    emitter.label("__rt_heap_debug_report_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // tear down the heap-debug report frame
    emitter.instruction("ret");                                                 // return to the caller
}
