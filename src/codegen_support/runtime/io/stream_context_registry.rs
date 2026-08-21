//! Purpose:
//! Emits the process-local registry backing PHP stream-context resources.
//! Gives every created context stable identity and independently owned options.
//!
//! Called from:
//! - Stream-context builtin lowering for create, select, replace, and mutation commit.
//! - Main and web-handler epilogues for deterministic retained-state cleanup.
//!
//! Key details:
//! - Context resources carry a managed-heap context-record pointer; resource zero names the default context.
//! - The legacy `_stream_context_options` symbol remains a borrowed alias to the selected record.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

const STREAM_CONTEXT_MAGIC: u32 = 0x5354_4358;
const STREAM_CONTEXT_RECORD_SIZE: usize = 40;

/// Emits stream-context registry helpers for the active target.
pub fn emit_stream_context_registry(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stream_context_registry_x86_64(emitter);
        return;
    }
    emit_stream_context_registry_aarch64(emitter);
}

/// Emits AArch64 stream-context create, select, replace, commit, and cleanup helpers.
fn emit_stream_context_registry_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream context registry ---");
    emitter.label_global("__rt_stream_context_create");
    emitter.instruction("sub sp, sp, #32");                                     // reserve options and context spill slots
    emitter.instruction("stp x29, x30, [sp, #16]");                             // preserve the caller frame and return address
    emitter.instruction("mov x29, sp");                                         // establish the context-create frame
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the borrowed options hash
    emitter.instruction(&format!("mov x0, #{}", STREAM_CONTEXT_RECORD_SIZE));   // request one context-record payload
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    emitter.instruction(&format!("mov w9, #0x{:x}", STREAM_CONTEXT_MAGIC & 0xffff)); // materialize the context-record magic low half
    emitter.instruction(&format!("movk w9, #0x{:x}, lsl #16", STREAM_CONTEXT_MAGIC >> 16)); // materialize the context-record magic high half
    emitter.instruction("str x9, [x0, #0]");                                    // stamp the context record for checked selection
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the borrowed options owner
    emitter.instruction("str x10, [x0, #8]");                                   // make the record point at its options hash
    emitter.instruction("stp xzr, xzr, [x0, #16]");                             // initialize params and notification owners
    abi::emit_symbol_address(emitter, "x9", "_stream_context_head");
    emitter.instruction("ldr x10, [x9]");                                       // load the previous dynamic-context list head
    emitter.instruction("str x10, [x0, #32]");                                  // chain the new record to the previous head
    emitter.instruction("str x0, [x9]");                                        // publish the new dynamic-context list head
    emitter.instruction("str x0, [sp, #8]");                                    // preserve the resource pointer across incref
    emitter.instruction("ldr x0, [sp, #0]");                                    // pass the options hash to the ownership retain
    emitter.instruction("cbz x0, __rt_stream_context_create_ret");              // an empty context owns no options allocation
    abi::emit_call_label(emitter, "__rt_incref");
    emitter.label("__rt_stream_context_create_ret");
    emitter.instruction("ldr x0, [sp, #8]");                                    // return the new context-record resource pointer
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #32");                                     // release the context-create frame
    emitter.instruction("ret");                                                 // return the distinct stream-context resource

    emitter.blank();
    emitter.label_global("__rt_stream_context_select");
    emitter.instruction("cbz x0, __rt_stream_context_select_default");          // resource zero selects PHP's default context
    abi::emit_symbol_address(emitter, "x9", "_heap_buf");
    emitter.instruction("add x10, x9, #16");                                    // compute the first managed-heap payload address
    emitter.instruction("cmp x0, x10");                                         // reject descriptors below the managed heap
    emitter.instruction("b.lo __rt_stream_context_select_invalid");             // non-context resources cannot be dereferenced
    abi::emit_symbol_address(emitter, "x10", "_heap_off");
    emitter.instruction("ldr x10, [x10]");                                      // load the current managed-heap extent
    emitter.instruction("add x10, x9, x10");                                    // compute the current managed-heap end
    emitter.instruction("cmp x0, x10");                                         // reject descriptors outside the live heap window
    emitter.instruction("b.hs __rt_stream_context_select_invalid");             // avoid reading a foreign resource as a context
    emitter.instruction("ldr w9, [x0, #0]");                                    // load the candidate context-record magic
    emitter.instruction(&format!("mov w10, #0x{:x}", STREAM_CONTEXT_MAGIC & 0xffff)); // materialize the expected magic low half
    emitter.instruction(&format!("movk w10, #0x{:x}, lsl #16", STREAM_CONTEXT_MAGIC >> 16)); // materialize the expected magic high half
    emitter.instruction("cmp w9, w10");                                         // does this managed block represent a stream context?
    emitter.instruction("b.ne __rt_stream_context_select_invalid");             // other heap resources are invalid contexts
    emitter.instruction("b __rt_stream_context_select_ready");                  // keep the validated dynamic record pointer
    emitter.label("__rt_stream_context_select_default");
    abi::emit_symbol_address(emitter, "x0", "_stream_context_default");
    emitter.label("__rt_stream_context_select_ready");
    abi::emit_symbol_address(emitter, "x9", "_stream_context_selected");
    emitter.instruction("str x0, [x9]");                                        // remember the selected record for mutation commit
    emitter.instruction("ldr x0, [x0, #8]");                                    // load the selected record's options hash
    abi::emit_symbol_address(emitter, "x9", "_stream_context_options");
    emitter.instruction("str x0, [x9]");                                        // publish the borrowed compatibility alias
    emitter.instruction("ret");                                                 // return the selected options pointer
    emitter.label("__rt_stream_context_select_invalid");
    abi::emit_symbol_address(emitter, "x9", "_stream_context_selected");
    emitter.instruction("str xzr, [x9]");                                       // prevent commits through an invalid resource
    abi::emit_symbol_address(emitter, "x9", "_stream_context_options");
    emitter.instruction("str xzr, [x9]");                                       // expose no options for an invalid resource
    emitter.instruction("mov x0, #0");                                          // return a null options pointer
    emitter.instruction("ret");                                                 // return without dereferencing the invalid resource

    emitter.blank();
    emitter.label_global("__rt_stream_context_replace_options");
    emitter.instruction("sub sp, sp, #48");                                     // reserve context, new, and old option spill slots
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve the caller frame and return address
    emitter.instruction("mov x29, sp");                                         // establish the context-replacement frame
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the context resource
    emitter.instruction("str x1, [sp, #8]");                                    // preserve the replacement options hash
    abi::emit_call_label(emitter, "__rt_stream_context_select");
    emitter.instruction("str x0, [sp, #16]");                                   // preserve the previous options owner
    abi::emit_symbol_address(emitter, "x9", "_stream_context_selected");
    emitter.instruction("ldr x10, [x9]");                                       // reload the validated selected context record
    emitter.instruction("cbz x10, __rt_stream_context_replace_done");           // invalid resources cannot be mutated
    emitter.instruction("str x10, [sp, #24]");                                  // preserve the record across ownership helpers
    emitter.instruction("ldr x0, [sp, #8]");                                    // pass the new options hash to incref
    emitter.instruction("cbz x0, __rt_stream_context_replace_store");           // null replacement owns no heap allocation
    abi::emit_call_label(emitter, "__rt_incref");
    emitter.label("__rt_stream_context_replace_store");
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the selected context record
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the replacement options hash
    emitter.instruction("str x11, [x10, #8]");                                  // transfer the retained options owner into the record
    abi::emit_symbol_address(emitter, "x9", "_stream_context_options");
    emitter.instruction("str x11, [x9]");                                       // update the borrowed compatibility alias
    emitter.instruction("ldr x0, [sp, #16]");                                   // pass the replaced options owner to decref
    emitter.instruction("cbz x0, __rt_stream_context_replace_done");            // no previous allocation needs release
    abi::emit_call_label(emitter, "__rt_decref_any");
    emitter.label("__rt_stream_context_replace_done");
    emitter.instruction("mov x0, #1");                                          // report successful context mutation
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #48");                                     // release the context-replacement frame
    emitter.instruction("ret");                                                 // return PHP true

    emitter.blank();
    emitter.label_global("__rt_stream_context_commit_options");
    abi::emit_symbol_address(emitter, "x9", "_stream_context_selected");
    emitter.instruction("ldr x10, [x9]");                                       // load the record selected before nested hash mutation
    emitter.instruction("cbz x10, __rt_stream_context_commit_done");            // invalid selections have no record to update
    abi::emit_symbol_address(emitter, "x9", "_stream_context_options");
    emitter.instruction("ldr x11, [x9]");                                       // load the possibly-relocated top-level hash
    emitter.instruction("str x11, [x10, #8]");                                  // publish the relocated owner in the selected record
    emitter.label("__rt_stream_context_commit_done");
    emitter.instruction("mov x0, #1");                                          // report successful mutation commit
    emitter.instruction("ret");                                                 // return to builtin lowering

    emitter.blank();
    emitter.label_global("__rt_stream_context_cleanup");
    emitter.instruction("sub sp, sp, #32");                                     // reserve current and next context-record spill slots
    emitter.instruction("stp x29, x30, [sp, #16]");                             // preserve the caller frame and return address
    emitter.instruction("mov x29, sp");                                         // establish the registry-cleanup frame
    abi::emit_symbol_address(emitter, "x9", "_stream_context_head");
    emitter.instruction("ldr x10, [x9]");                                       // load the first dynamic context record
    emitter.instruction("str x10, [sp, #0]");                                   // seed the dynamic-context cleanup cursor
    emitter.instruction("str xzr, [x9]");                                       // detach the registry before releasing its records
    emitter.label("__rt_stream_context_cleanup_loop");
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the current dynamic context record
    emitter.instruction("cbz x10, __rt_stream_context_cleanup_default");        // continue with the fixed default context at list end
    emitter.instruction("ldr x11, [x10, #32]");                                 // load the next record before freeing the current one
    emitter.instruction("str x11, [sp, #8]");                                   // preserve the next cleanup cursor
    emitter.instruction("ldr x0, [x10, #8]");                                   // load the current record's options owner
    emitter.instruction("cbz x0, __rt_stream_context_cleanup_record");          // empty contexts own no options allocation
    abi::emit_call_label(emitter, "__rt_decref_any");
    emitter.label("__rt_stream_context_cleanup_record");
    emitter.instruction("ldr x0, [sp, #0]");                                    // pass the raw context record to safe free
    abi::emit_call_label(emitter, "__rt_heap_free_safe");
    emitter.instruction("ldr x10, [sp, #8]");                                   // advance to the saved next record
    emitter.instruction("str x10, [sp, #0]");                                   // persist the next cleanup cursor
    emitter.instruction("b __rt_stream_context_cleanup_loop");                  // release every dynamically created context
    emitter.label("__rt_stream_context_cleanup_default");
    abi::emit_symbol_address(emitter, "x10", "_stream_context_default");
    emitter.instruction("ldr x0, [x10, #8]");                                   // load the default context's options owner
    emitter.instruction("str xzr, [x10, #8]");                                  // clear the default owner before recursive release
    emitter.instruction("cbz x0, __rt_stream_context_cleanup_notification");    // skip an empty default context
    abi::emit_call_label(emitter, "__rt_decref_any");
    emitter.label("__rt_stream_context_cleanup_notification");
    abi::emit_symbol_address(emitter, "x9", "_stream_notification_callback");
    emitter.instruction("ldr x0, [x9]");                                        // load the retained notification callable descriptor
    emitter.instruction("str xzr, [x9]");                                       // detach the descriptor before invoking its release path
    emitter.instruction("cbz x0, __rt_stream_context_cleanup_clear");           // no callback descriptor needs release
    abi::emit_call_label(emitter, "__rt_callable_descriptor_release");
    emitter.label("__rt_stream_context_cleanup_clear");
    abi::emit_symbol_address(emitter, "x9", "_stream_context_selected");
    emitter.instruction("str xzr, [x9]");                                       // clear the selected-record borrow
    abi::emit_symbol_address(emitter, "x9", "_stream_context_options");
    emitter.instruction("str xzr, [x9]");                                       // clear the compatibility options alias
    abi::emit_symbol_address(emitter, "x9", "_dom_stream_context_active");
    emitter.instruction("str xzr, [x9]");                                       // clear any interrupted DOM context-injection flag
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #32");                                     // release the registry-cleanup frame
    emitter.instruction("ret");                                                 // return after deterministic context cleanup
}

/// Emits x86_64 stream-context create, select, replace, commit, and cleanup helpers.
fn emit_stream_context_registry_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream context registry ---");
    emitter.label_global("__rt_stream_context_create");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the context-create frame
    emitter.instruction("sub rsp, 16");                                         // reserve options and context spill slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the borrowed options hash
    emitter.instruction(&format!("mov eax, {}", STREAM_CONTEXT_RECORD_SIZE));   // request one context-record payload
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    emitter.instruction(&format!("mov QWORD PTR [rax], 0x{:x}", STREAM_CONTEXT_MAGIC)); // stamp the context record for checked selection
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the borrowed options owner
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // make the record point at its options hash
    emitter.instruction("mov QWORD PTR [rax + 16], 0");                         // initialize the params owner
    emitter.instruction("mov QWORD PTR [rax + 24], 0");                         // initialize the notification owner
    abi::emit_load_symbol_to_reg(emitter, "r10", "_stream_context_head", 0);
    emitter.instruction("mov QWORD PTR [rax + 32], r10");                       // chain the new record to the previous head
    abi::emit_store_reg_to_symbol(emitter, "rax", "_stream_context_head", 0);
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the resource pointer across incref
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // pass the options hash to the ownership retain
    emitter.instruction("test rax, rax");                                       // does the new context own options?
    emitter.instruction("jz __rt_stream_context_create_ret_x86");               // an empty context owns no options allocation
    abi::emit_call_label(emitter, "__rt_incref");
    emitter.label("__rt_stream_context_create_ret_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return the new context-record resource pointer
    emitter.instruction("add rsp, 16");                                         // release the context-create spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the distinct stream-context resource

    emitter.blank();
    emitter.label_global("__rt_stream_context_select");
    emitter.instruction("test rax, rax");                                       // does resource zero select the default context?
    emitter.instruction("jz __rt_stream_context_select_default_x86");           // resolve PHP's fixed default context
    abi::emit_symbol_address(emitter, "r9", "_heap_buf");
    emitter.instruction("lea r10, [r9 + 16]");                                  // compute the first managed-heap payload address
    emitter.instruction("cmp rax, r10");                                        // reject descriptors below the managed heap
    emitter.instruction("jb __rt_stream_context_select_invalid_x86");           // non-context resources cannot be dereferenced
    abi::emit_load_symbol_to_reg(emitter, "r10", "_heap_off", 0);
    emitter.instruction("add r10, r9");                                         // compute the current managed-heap end
    emitter.instruction("cmp rax, r10");                                        // reject descriptors outside the live heap window
    emitter.instruction("jae __rt_stream_context_select_invalid_x86");          // avoid reading a foreign resource as a context
    emitter.instruction(&format!("cmp DWORD PTR [rax], 0x{:x}", STREAM_CONTEXT_MAGIC)); // validate the candidate context-record magic
    emitter.instruction("jne __rt_stream_context_select_invalid_x86");          // other heap resources are invalid contexts
    emitter.instruction("jmp __rt_stream_context_select_ready_x86");            // keep the validated dynamic record pointer
    emitter.label("__rt_stream_context_select_default_x86");
    abi::emit_symbol_address(emitter, "rax", "_stream_context_default");
    emitter.label("__rt_stream_context_select_ready_x86");
    abi::emit_store_reg_to_symbol(emitter, "rax", "_stream_context_selected", 0);
    emitter.instruction("mov rax, QWORD PTR [rax + 8]");                        // load the selected record's options hash
    abi::emit_store_reg_to_symbol(emitter, "rax", "_stream_context_options", 0);
    emitter.instruction("ret");                                                 // return the selected options pointer
    emitter.label("__rt_stream_context_select_invalid_x86");
    abi::emit_store_zero_to_symbol(emitter, "_stream_context_selected", 0);
    abi::emit_store_zero_to_symbol(emitter, "_stream_context_options", 0);
    emitter.instruction("xor eax, eax");                                        // return a null options pointer
    emitter.instruction("ret");                                                 // return without dereferencing the invalid resource

    emitter.blank();
    emitter.label_global("__rt_stream_context_replace_options");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the context-replacement frame
    emitter.instruction("sub rsp, 32");                                         // reserve context, new, old, and record spill slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the context resource
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // preserve the replacement options hash
    abi::emit_call_label(emitter, "__rt_stream_context_select");
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the previous options owner
    abi::emit_load_symbol_to_reg(emitter, "r10", "_stream_context_selected", 0);
    emitter.instruction("test r10, r10");                                       // did selection validate a context record?
    emitter.instruction("jz __rt_stream_context_replace_done_x86");             // invalid resources cannot be mutated
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // preserve the record across ownership helpers
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // pass the new options hash to incref
    emitter.instruction("test rax, rax");                                       // does the replacement own heap storage?
    emitter.instruction("jz __rt_stream_context_replace_store_x86");            // null replacement owns no allocation
    abi::emit_call_label(emitter, "__rt_incref");
    emitter.label("__rt_stream_context_replace_store_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the selected context record
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the replacement options hash
    emitter.instruction("mov QWORD PTR [r10 + 8], r11");                        // transfer the retained options owner into the record
    abi::emit_store_reg_to_symbol(emitter, "r11", "_stream_context_options", 0);
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // pass the replaced options owner to decref
    emitter.instruction("test rax, rax");                                       // did the record previously own options?
    emitter.instruction("jz __rt_stream_context_replace_done_x86");             // no previous allocation needs release
    abi::emit_call_label(emitter, "__rt_decref_any");
    emitter.label("__rt_stream_context_replace_done_x86");
    emitter.instruction("mov eax, 1");                                          // report successful context mutation
    emitter.instruction("add rsp, 32");                                         // release the context-replacement spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return PHP true

    emitter.blank();
    emitter.label_global("__rt_stream_context_commit_options");
    abi::emit_load_symbol_to_reg(emitter, "r10", "_stream_context_selected", 0);
    emitter.instruction("test r10, r10");                                       // is there a validated selected context?
    emitter.instruction("jz __rt_stream_context_commit_done_x86");              // invalid selections have no record to update
    abi::emit_load_symbol_to_reg(emitter, "r11", "_stream_context_options", 0);
    emitter.instruction("mov QWORD PTR [r10 + 8], r11");                        // publish the possibly-relocated top-level hash
    emitter.label("__rt_stream_context_commit_done_x86");
    emitter.instruction("mov eax, 1");                                          // report successful mutation commit
    emitter.instruction("ret");                                                 // return to builtin lowering

    emitter.blank();
    emitter.label_global("__rt_stream_context_cleanup");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the registry-cleanup frame
    emitter.instruction("sub rsp, 16");                                         // reserve current and next context-record spill slots
    abi::emit_load_symbol_to_reg(emitter, "r10", "_stream_context_head", 0);
    emitter.instruction("mov QWORD PTR [rbp - 8], r10");                        // seed the dynamic-context cleanup cursor
    abi::emit_store_zero_to_symbol(emitter, "_stream_context_head", 0);
    emitter.label("__rt_stream_context_cleanup_loop_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the current dynamic context record
    emitter.instruction("test r10, r10");                                       // has the dynamic context list ended?
    emitter.instruction("jz __rt_stream_context_cleanup_default_x86");          // continue with the fixed default context
    emitter.instruction("mov r11, QWORD PTR [r10 + 32]");                       // load the next record before freeing the current one
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // preserve the next cleanup cursor
    emitter.instruction("mov rax, QWORD PTR [r10 + 8]");                        // load the current record's options owner
    emitter.instruction("test rax, rax");                                       // does this context own options?
    emitter.instruction("jz __rt_stream_context_cleanup_record_x86");           // empty contexts own no options allocation
    abi::emit_call_label(emitter, "__rt_decref_any");
    emitter.label("__rt_stream_context_cleanup_record_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // pass the raw context record to safe free
    abi::emit_call_label(emitter, "__rt_heap_free_safe");
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // advance to the saved next record
    emitter.instruction("mov QWORD PTR [rbp - 8], r10");                        // persist the next cleanup cursor
    emitter.instruction("jmp __rt_stream_context_cleanup_loop_x86");            // release every dynamically created context
    emitter.label("__rt_stream_context_cleanup_default_x86");
    abi::emit_symbol_address(emitter, "r10", "_stream_context_default");
    emitter.instruction("mov rax, QWORD PTR [r10 + 8]");                        // load the default context's options owner
    emitter.instruction("mov QWORD PTR [r10 + 8], 0");                          // clear the default owner before recursive release
    emitter.instruction("test rax, rax");                                       // does the default context own options?
    emitter.instruction("jz __rt_stream_context_cleanup_notification_x86");     // skip an empty default context
    abi::emit_call_label(emitter, "__rt_decref_any");
    emitter.label("__rt_stream_context_cleanup_notification_x86");
    abi::emit_load_symbol_to_reg(emitter, "rax", "_stream_notification_callback", 0);
    abi::emit_store_zero_to_symbol(emitter, "_stream_notification_callback", 0);
    emitter.instruction("test rax, rax");                                       // is a notification callable descriptor retained?
    emitter.instruction("jz __rt_stream_context_cleanup_clear_x86");            // no callback descriptor needs release
    abi::emit_call_label(emitter, "__rt_callable_descriptor_release");
    emitter.label("__rt_stream_context_cleanup_clear_x86");
    abi::emit_store_zero_to_symbol(emitter, "_stream_context_selected", 0);
    abi::emit_store_zero_to_symbol(emitter, "_stream_context_options", 0);
    abi::emit_store_zero_to_symbol(emitter, "_dom_stream_context_active", 0);
    emitter.instruction("add rsp, 16");                                         // release the registry-cleanup spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return after deterministic context cleanup
}
