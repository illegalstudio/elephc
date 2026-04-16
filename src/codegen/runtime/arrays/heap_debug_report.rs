use crate::codegen::{emit::Emitter, platform::Arch};

/// heap_debug_report: print allocator/debug summary and leak info to stderr.
pub fn emit_heap_debug_report(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emitter.blank();
        emitter.comment("--- runtime: heap_debug_report ---");
        emitter.label_global("__rt_heap_debug_report");

        emitter.instruction("sub rsp, 40");                                     // reserve aligned stack space for saved counters across nested itoa calls
        crate::codegen::abi::emit_symbol_address(emitter, "r8", "_gc_allocs");
        emitter.instruction("mov r8, QWORD PTR [r8]");                          // load the total allocation count once for the summary
        crate::codegen::abi::emit_symbol_address(emitter, "r9", "_gc_frees");
        emitter.instruction("mov r9, QWORD PTR [r9]");                          // load the total free count once for the summary
        emitter.instruction("mov r10, r8");                                     // start deriving the live block count from allocs - frees
        emitter.instruction("sub r10, r9");                                     // compute the current live block count from allocs - frees
        crate::codegen::abi::emit_symbol_address(emitter, "r11", "_gc_live");
        emitter.instruction("mov r11, QWORD PTR [r11]");                        // load the current live-byte count once for the summary
        emitter.instruction("mov QWORD PTR [rsp], r8");                         // save alloc count across syscalls and nested itoa calls
        emitter.instruction("mov QWORD PTR [rsp + 8], r9");                     // save free count across syscalls and nested itoa calls
        emitter.instruction("mov QWORD PTR [rsp + 16], r10");                   // save live block count for the second report line
        emitter.instruction("mov QWORD PTR [rsp + 24], r11");                   // save live-byte count for the second report line

        crate::codegen::abi::emit_symbol_address(emitter, "rsi", "_heap_dbg_stats_prefix");
        emitter.instruction("mov edx, 19");                                     // pass the exact heap-debug summary prefix length to write
        emitter.instruction("mov edi, 2");                                      // fd = stderr for the heap-debug summary prefix
        emitter.instruction("mov eax, 1");                                      // Linux x86_64 syscall 1 = write
        emitter.instruction("syscall");                                         // print the heap-debug summary prefix
        emitter.instruction("mov rax, QWORD PTR [rsp]");                        // reload the alloc count after the write syscall
        emitter.instruction("call __rt_itoa");                                  // convert the alloc count to decimal text through the shared runtime helper
        emitter.instruction("mov rsi, rax");                                    // point the Linux write syscall at the decimal alloc-count string
        emitter.instruction("mov edi, 2");                                      // fd = stderr for the alloc-count decimal text
        emitter.instruction("mov eax, 1");                                      // Linux x86_64 syscall 1 = write
        emitter.instruction("syscall");                                         // print the alloc-count decimal text

        crate::codegen::abi::emit_symbol_address(emitter, "rsi", "_heap_dbg_frees_label");
        emitter.instruction("mov edx, 7");                                      // pass the exact \" frees=\" label length to write
        emitter.instruction("mov edi, 2");                                      // fd = stderr for the frees label
        emitter.instruction("mov eax, 1");                                      // Linux x86_64 syscall 1 = write
        emitter.instruction("syscall");                                         // print the frees label
        emitter.instruction("mov rax, QWORD PTR [rsp + 8]");                    // reload the free count after the previous write syscall
        emitter.instruction("call __rt_itoa");                                  // convert the free count to decimal text through the shared runtime helper
        emitter.instruction("mov rsi, rax");                                    // point the Linux write syscall at the decimal free-count string
        emitter.instruction("mov edi, 2");                                      // fd = stderr for the free-count decimal text
        emitter.instruction("mov eax, 1");                                      // Linux x86_64 syscall 1 = write
        emitter.instruction("syscall");                                         // print the free-count decimal text

        crate::codegen::abi::emit_symbol_address(emitter, "rsi", "_heap_dbg_live_blocks_label");
        emitter.instruction("mov edx, 13");                                     // pass the exact \" live_blocks=\" label length to write
        emitter.instruction("mov edi, 2");                                      // fd = stderr for the live-block label
        emitter.instruction("mov eax, 1");                                      // Linux x86_64 syscall 1 = write
        emitter.instruction("syscall");                                         // print the live-block label
        emitter.instruction("mov rax, QWORD PTR [rsp + 16]");                   // reload the live block count after the previous write syscall
        emitter.instruction("call __rt_itoa");                                  // convert the live block count to decimal text through the shared runtime helper
        emitter.instruction("mov rsi, rax");                                    // point the Linux write syscall at the decimal live-block string
        emitter.instruction("mov edi, 2");                                      // fd = stderr for the live-block decimal text
        emitter.instruction("mov eax, 1");                                      // Linux x86_64 syscall 1 = write
        emitter.instruction("syscall");                                         // print the live-block decimal text

        crate::codegen::abi::emit_symbol_address(emitter, "rsi", "_heap_dbg_live_bytes_label");
        emitter.instruction("mov edx, 12");                                     // pass the exact \" live_bytes=\" label length to write
        emitter.instruction("mov edi, 2");                                      // fd = stderr for the live-bytes label
        emitter.instruction("mov eax, 1");                                      // Linux x86_64 syscall 1 = write
        emitter.instruction("syscall");                                         // print the live-bytes label
        emitter.instruction("mov rax, QWORD PTR [rsp + 24]");                   // reload the live-byte count after the previous write syscall
        emitter.instruction("call __rt_itoa");                                  // convert the live-byte count to decimal text through the shared runtime helper
        emitter.instruction("mov rsi, rax");                                    // point the Linux write syscall at the decimal live-byte string
        emitter.instruction("mov edi, 2");                                      // fd = stderr for the live-byte decimal text
        emitter.instruction("mov eax, 1");                                      // Linux x86_64 syscall 1 = write
        emitter.instruction("syscall");                                         // print the live-byte decimal text

        crate::codegen::abi::emit_symbol_address(emitter, "rsi", "_heap_dbg_peak_label");
        emitter.instruction("mov edx, 17");                                     // pass the exact \" peak_live_bytes=\" label length to write
        emitter.instruction("mov edi, 2");                                      // fd = stderr for the peak-live-bytes label
        emitter.instruction("mov eax, 1");                                      // Linux x86_64 syscall 1 = write
        emitter.instruction("syscall");                                         // print the peak-live-bytes label
        crate::codegen::abi::emit_symbol_address(emitter, "r8", "_gc_peak");
        emitter.instruction("mov rax, QWORD PTR [r8]");                         // load the peak live-byte watermark after the prefix writes
        emitter.instruction("call __rt_itoa");                                  // convert the peak live-byte watermark to decimal text
        emitter.instruction("mov rsi, rax");                                    // point the Linux write syscall at the decimal peak-live-byte string
        emitter.instruction("mov edi, 2");                                      // fd = stderr for the peak-live-byte decimal text
        emitter.instruction("mov eax, 1");                                      // Linux x86_64 syscall 1 = write
        emitter.instruction("syscall");                                         // print the peak-live-byte decimal text
        crate::codegen::abi::emit_symbol_address(emitter, "rsi", "_heap_dbg_newline");
        emitter.instruction("mov edx, 1");                                      // pass the newline byte count to write
        emitter.instruction("mov edi, 2");                                      // fd = stderr for the newline terminator
        emitter.instruction("mov eax, 1");                                      // Linux x86_64 syscall 1 = write
        emitter.instruction("syscall");                                         // terminate the summary line with a newline

        crate::codegen::abi::emit_symbol_address(emitter, "rsi", "_heap_dbg_leak_prefix");
        emitter.instruction("mov edx, 26");                                     // pass the exact leak-summary prefix length to write
        emitter.instruction("mov edi, 2");                                      // fd = stderr for the leak-summary prefix
        emitter.instruction("mov eax, 1");                                      // Linux x86_64 syscall 1 = write
        emitter.instruction("syscall");                                         // print the leak-summary prefix
        emitter.instruction("mov r8, QWORD PTR [rsp + 16]");                    // reload the live-block count to choose between clean and leak-detail output
        emitter.instruction("test r8, r8");                                     // are there any live heap blocks left at process exit?
        emitter.instruction("jnz __rt_heap_debug_report_leak_details");         // yes — print the detailed leak counts instead of the clean marker
        crate::codegen::abi::emit_symbol_address(emitter, "rsi", "_heap_dbg_clean_label");
        emitter.instruction("mov edx, 6");                                      // pass the exact \"clean\\n\" label length to write
        emitter.instruction("mov edi, 2");                                      // fd = stderr for the clean leak-summary marker
        emitter.instruction("mov eax, 1");                                      // Linux x86_64 syscall 1 = write
        emitter.instruction("syscall");                                         // print the clean leak-summary marker
        emitter.instruction("jmp __rt_heap_debug_report_done");                 // skip the leak-detail path once the clean marker is written

        emitter.label("__rt_heap_debug_report_leak_details");
        crate::codegen::abi::emit_symbol_address(emitter, "rsi", "_heap_dbg_live_blocks_short_label");
        emitter.instruction("mov edx, 12");                                     // pass the exact short live-block label length to write
        emitter.instruction("mov edi, 2");                                      // fd = stderr for the short live-block label
        emitter.instruction("mov eax, 1");                                      // Linux x86_64 syscall 1 = write
        emitter.instruction("syscall");                                         // print the short live-block label
        emitter.instruction("mov rax, QWORD PTR [rsp + 16]");                   // reload the live-block count for decimal conversion
        emitter.instruction("call __rt_itoa");                                  // convert the live-block count to decimal text for the leak summary
        emitter.instruction("mov rsi, rax");                                    // point the Linux write syscall at the decimal live-block string
        emitter.instruction("mov edi, 2");                                      // fd = stderr for the live-block leak-summary value
        emitter.instruction("mov eax, 1");                                      // Linux x86_64 syscall 1 = write
        emitter.instruction("syscall");                                         // print the leak-summary live-block value
        crate::codegen::abi::emit_symbol_address(emitter, "rsi", "_heap_dbg_live_bytes_label");
        emitter.instruction("mov edx, 12");                                     // pass the exact live-bytes label length to write
        emitter.instruction("mov edi, 2");                                      // fd = stderr for the leak-summary live-bytes label
        emitter.instruction("mov eax, 1");                                      // Linux x86_64 syscall 1 = write
        emitter.instruction("syscall");                                         // print the leak-summary live-bytes label
        emitter.instruction("mov rax, QWORD PTR [rsp + 24]");                   // reload the live-byte count for decimal conversion
        emitter.instruction("call __rt_itoa");                                  // convert the live-byte count to decimal text for the leak summary
        emitter.instruction("mov rsi, rax");                                    // point the Linux write syscall at the decimal live-byte string
        emitter.instruction("mov edi, 2");                                      // fd = stderr for the live-byte leak-summary value
        emitter.instruction("mov eax, 1");                                      // Linux x86_64 syscall 1 = write
        emitter.instruction("syscall");                                         // print the leak-summary live-byte value
        crate::codegen::abi::emit_symbol_address(emitter, "rsi", "_heap_dbg_newline");
        emitter.instruction("mov edx, 1");                                      // pass the newline byte count to write
        emitter.instruction("mov edi, 2");                                      // fd = stderr for the leak-summary newline terminator
        emitter.instruction("mov eax, 1");                                      // Linux x86_64 syscall 1 = write
        emitter.instruction("syscall");                                         // terminate the leak-summary line with a newline

        emitter.label("__rt_heap_debug_report_done");
        emitter.instruction("add rsp, 40");                                     // release the temporary stack frame used for saved counters
        emitter.instruction("ret");                                             // return to the process epilogue after printing the heap-debug summary
        return;
    }

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
