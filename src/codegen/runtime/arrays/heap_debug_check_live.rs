use crate::codegen::{emit::Emitter, platform::Arch};

/// heap_debug_check_live: fail if a heap pointer refers to a freed block.
/// Input: x0 = heap user pointer
pub fn emit_heap_debug_check_live(emitter: &mut Emitter) {
    let msg = "Fatal error: heap debug detected bad refcount\n";

    if emitter.target.arch == Arch::X86_64 {
        emitter.blank();
        emitter.comment("--- runtime: heap_debug_check_live ---");
        emitter.label_global("__rt_heap_debug_check_live");
        emitter.instruction("mov ecx, DWORD PTR [rax - 12]");                   // load the current block refcount from the uniform heap header
        emitter.instruction("test ecx, ecx");                                   // does the heap block still look live?
        emitter.instruction("jnz __rt_heap_debug_check_live_done");             // nonzero refcount means the block is still live
        crate::codegen::abi::emit_symbol_address(emitter, "rsi", "_heap_dbg_bad_refcount_msg");
        emitter.instruction(&format!("mov edx, {}", msg.len()));                // pass the exact bad-refcount message length to the failure helper
        emitter.instruction("jmp __rt_heap_debug_fail");                        // report the bad-refcount heap-debug failure and terminate

        emitter.label("__rt_heap_debug_check_live_done");
        emitter.instruction("ret");                                             // return when the block still looks live
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: heap_debug_check_live ---");
    emitter.label_global("__rt_heap_debug_check_live");
    emitter.instruction("ldr w9, [x0, #-12]");                                  // load the current block refcount from the uniform heap header
    emitter.instruction("cbnz w9, __rt_heap_debug_check_live_done");            // nonzero refcount means the block still looks live
    emitter.adrp("x1", "_heap_dbg_bad_refcount_msg");            // load page of the bad-refcount debug message
    emitter.add_lo12("x1", "x1", "_heap_dbg_bad_refcount_msg");      // resolve the bad-refcount debug message address
    emitter.instruction(&format!("mov x2, #{}", msg.len()));                    // pass the exact bad-refcount message length
    emitter.instruction("b __rt_heap_debug_fail");                              // report the heap-debug failure and exit

    emitter.label("__rt_heap_debug_check_live_done");
    emitter.instruction("ret");                                                 // return when the block still looks live
}
