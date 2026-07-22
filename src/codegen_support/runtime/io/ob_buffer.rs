//! Purpose:
//! Emits the output-buffering (`ob_*`) runtime helpers that maintain the stack of
//! capture buffers behind PHP's output-control builtins: `__rt_ob_start_ex` /
//! `__rt_ob_start`, `__rt_ob_append` (with chunk auto-flush), `__rt_ob_contents`,
//! `__rt_ob_length`, `__rt_ob_level`, the flags-gated `__rt_ob_clean` /
//! `__rt_ob_end_clean` / `__rt_ob_flush` / `__rt_ob_end_flush` /
//! `__rt_ob_get_clean_pop` / `__rt_ob_get_flush_pop`, and `__rt_ob_flush_all`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::io`.
//! - `__rt_stdout_write` and `__rt_pr_write` call `__rt_ob_append` while
//!   `_ob_level` is non-zero so every terminal write is captured.
//! - `crate::codegen_support::abi::emit_exit`/`emit_exit_with_result_reg` call
//!   `__rt_ob_flush_all` so still-active buffers reach stdout at process exit.
//!
//! Key details:
//! - State lives in the fixed runtime data section: `_ob_level` plus 64-slot
//!   parallel arrays (`_ob_ptrs`/`_ob_lens`/`_ob_caps` for storage,
//!   `_ob_handler_stubs`/`_ob_handler_envs`/`_ob_name_ptrs`/`_ob_name_lens`/
//!   `_ob_chunk_sizes`/`_ob_flags`/`_ob_started` for handler metadata), indexed
//!   by level-1.
//! - PHP-parity gating: `ob_clean` needs CLEANABLE (16), `ob_flush` needs
//!   FLUSHABLE (32), every end/get variant needs REMOVABLE (64); refusals raise
//!   PHP's notices (through the funnel, so parent buffers capture them).
//! - Flush/clean paths run the user handler through `__rt_ob_apply_handler`
//!   with PHP's phase bits (WRITE 0 / CLEAN 2 / FLUSH 4 / FINAL 8, plus START
//!   on first run); a replacement string substitutes the raw bytes on flush
//!   paths and is discarded on clean paths.
//! - Flushing to the parent sink temporarily publishes `_ob_level = slot` and
//!   routes back through `__rt_stdout_write` (parent buffer, `--web` capture,
//!   or terminal). Buffer capacity is PHP-shaped: 16384 by default,
//!   `((chunk >> 12) + 1) << 12` when a chunk size is set, growing by doubling.
//! - `__rt_ob_flush_all` drains top-down with FINAL handler phases behind the
//!   `_ob_flushing` re-entry guard and never frees (the process is exiting).

use crate::codegen_support::abi;
use crate::codegen_support::runtime::data::{
    OB_DEFAULT_HANDLER_NAME, OB_FATAL_IN_HANDLER, OB_NTC_CREATE_FAIL, OB_NTC_G_CLEAN,
    OB_NTC_G_END_CLEAN, OB_NTC_G_END_FLUSH, OB_NTC_G_FLUSH, OB_NTC_G_GET_CLEAN,
    OB_NTC_G_GET_FLUSH, OB_NTC_NO_CLEAN, OB_NTC_NO_END_CLEAN, OB_NTC_NO_END_FLUSH,
    OB_NTC_NO_FLUSH, OB_NTC_NO_GET_FLUSH,
};
use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Maximum output-buffer nesting depth (slots in the `_ob_*` parallel arrays).
pub(crate) const OB_MAX_LEVELS: i64 = 64;

/// Default capacity of a chunk-less output buffer (PHP reports 16384).
const OB_DEFAULT_CAPACITY: i64 = 16384;

/// PHP_OUTPUT_HANDLER_CLEANABLE: gates `ob_clean()`.
const OB_FLAG_CLEANABLE: i64 = 16;

/// PHP_OUTPUT_HANDLER_FLUSHABLE: gates `ob_flush()`.
const OB_FLAG_FLUSHABLE: i64 = 32;

/// PHP_OUTPUT_HANDLER_REMOVABLE: gates every end/get-and-pop variant.
const OB_FLAG_REMOVABLE: i64 = 64;

/// PHP_OUTPUT_HANDLER_CLEAN phase bit.
const OB_PHASE_CLEAN: i64 = 2;

/// PHP_OUTPUT_HANDLER_FLUSH phase bit.
const OB_PHASE_FLUSH: i64 = 4;

/// PHP_OUTPUT_HANDLER_FINAL phase bit.
const OB_PHASE_FINAL: i64 = 8;

/// Emits `__rt_ob_start_ex`: push a new output buffer with handler metadata.
///
/// Inputs: `x0`/`rdi` = handler stub (0 = default handler), `x1`/`rsi` = handler
/// env word, `x2`/`rdx` = chunk size, `x3`/`rcx` = flags, `x4`/`r8` = display
/// name pointer, `x5`/`r9` = display name length. Returns 1 in `x0`/`rax` on
/// success, 0 when the nesting limit refuses the buffer (with PHP's
/// failed-create notice). Fatal when called from inside a running handler.
/// Also emits the `__rt_ob_start` default-handler compatibility wrapper.
pub fn emit_ob_start(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_ob_start_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: ob_start_ex / ob_start ---");
    emitter.label_global("__rt_ob_start_ex");
    // frame: [0]=stub, [8]=env, [16]=chunk, [24]=flags, [32]=name ptr, [40]=name len, [48]=capacity
    emitter.instruction("sub sp, sp, #80");                                     // allocate the ob_start frame
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish the ob_start frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the handler stub
    emitter.instruction("str x1, [sp, #8]");                                    // save the handler env word
    emitter.instruction("str x2, [sp, #16]");                                   // save the chunk size
    emitter.instruction("str x3, [sp, #24]");                                   // save the flags word
    // -- PHP forbids starting a buffer from inside a running handler --
    abi::emit_symbol_address(emitter, "x9", "_ob_in_handler");                  // materialize the in-handler flag address
    emitter.instruction("ldr x9, [x9]");                                        // load the in-handler flag
    emitter.instruction("cbz x9, __rt_ob_start_allowed");                       // not inside a handler — proceed
    abi::emit_symbol_address(emitter, "x1", "_ob_fatal_in_handler");            // load the nested-handler fatal message
    emitter.instruction(&format!("mov x2, #{}", OB_FATAL_IN_HANDLER.len()));    // fatal message byte length
    emitter.instruction("mov x0, #1");                                          // fd = stdout (raw write; the funnel would discard it)
    emitter.syscall(4);                                                         // write the fatal message
    emitter.instruction("mov x0, #1");                                          // process exit code 1 like other runtime fatals
    emitter.syscall(1);                                                         // terminate the process
    emitter.label("__rt_ob_start_allowed");
    // -- persist the display name so the slot always owns heap bytes --
    emitter.instruction("mov x1, x4");                                          // str_persist source pointer = the display name
    emitter.instruction("mov x2, x5");                                          // str_persist source length = the display name length
    emitter.instruction("bl __rt_str_persist");                                 // copy the display name into an owned heap string
    emitter.instruction("stp x1, x2, [sp, #32]");                               // save the owned name pair
    // -- refuse past the nesting limit with PHP's failed-create notice --
    abi::emit_symbol_address(emitter, "x9", "_ob_level");                       // materialize the address of the buffer-stack depth
    emitter.instruction("ldr x10, [x9]");                                       // load the current buffer-stack depth
    emitter.instruction(&format!("cmp x10, #{}", OB_MAX_LEVELS));               // is the nesting limit reached?
    emitter.instruction("b.lt __rt_ob_start_capacity");                         // room available — size the buffer
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the owned name for release
    emitter.instruction("bl __rt_decref_any");                                  // release the unused owned name
    abi::emit_symbol_address(emitter, "x0", "_ob_ntc_create_fail");             // load the failed-create notice line
    emitter.instruction(&format!("mov x1, #{}", OB_NTC_CREATE_FAIL.len()));     // notice byte length
    emitter.instruction("bl __rt_stdout_write");                                // write the notice through the funnel
    emitter.instruction("mov x0, #0");                                          // report failure
    emitter.instruction("b __rt_ob_start_done");                                // finish
    // -- PHP-shaped capacity: 16384 default, page-aligned chunk+1 otherwise --
    emitter.label("__rt_ob_start_capacity");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the chunk size
    emitter.instruction("cbnz x9, __rt_ob_start_chunk_cap");                    // a chunk size selects the aligned capacity
    emitter.instruction(&format!("mov x0, #{}", OB_DEFAULT_CAPACITY));          // default buffer capacity
    emitter.instruction("b __rt_ob_start_alloc");                               // allocate the buffer
    emitter.label("__rt_ob_start_chunk_cap");
    emitter.instruction("lsr x10, x9, #12");                                    // whole 4 KiB pages in the chunk size
    emitter.instruction("add x10, x10, #1");                                    // round up to the next page
    emitter.instruction("lsl x0, x10, #12");                                    // capacity = ((chunk >> 12) + 1) << 12
    emitter.label("__rt_ob_start_alloc");
    emitter.instruction("str x0, [sp, #48]");                                   // save the chosen capacity
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the capture buffer (raw kind)
    // -- record every per-slot field --
    abi::emit_symbol_address(emitter, "x9", "_ob_level");                       // materialize the address of the buffer-stack depth
    emitter.instruction("ldr x10, [x9]");                                       // reload the depth (the new slot index)
    abi::emit_symbol_address(emitter, "x11", "_ob_ptrs");                       // materialize the buffer-pointer slot array
    emitter.instruction("str x0, [x11, x10, lsl #3]");                          // record the new buffer base pointer
    abi::emit_symbol_address(emitter, "x11", "_ob_lens");                       // materialize the used-bytes slot array
    emitter.instruction("str xzr, [x11, x10, lsl #3]");                         // the new buffer starts empty
    abi::emit_symbol_address(emitter, "x11", "_ob_caps");                       // materialize the capacity slot array
    emitter.instruction("ldr x12, [sp, #48]");                                  // reload the chosen capacity
    emitter.instruction("str x12, [x11, x10, lsl #3]");                         // record the capacity
    abi::emit_symbol_address(emitter, "x11", "_ob_handler_stubs");              // materialize the handler-stub slot array
    emitter.instruction("ldr x12, [sp, #0]");                                   // reload the handler stub
    emitter.instruction("str x12, [x11, x10, lsl #3]");                         // record the handler stub
    abi::emit_symbol_address(emitter, "x11", "_ob_handler_envs");               // materialize the handler-env slot array
    emitter.instruction("ldr x12, [sp, #8]");                                   // reload the handler env word
    emitter.instruction("str x12, [x11, x10, lsl #3]");                         // record the handler env word
    abi::emit_symbol_address(emitter, "x11", "_ob_name_ptrs");                  // materialize the handler-name pointer array
    emitter.instruction("ldr x12, [sp, #32]");                                  // reload the owned name pointer
    emitter.instruction("str x12, [x11, x10, lsl #3]");                         // record the owned name pointer
    abi::emit_symbol_address(emitter, "x11", "_ob_name_lens");                  // materialize the handler-name length array
    emitter.instruction("ldr x12, [sp, #40]");                                  // reload the owned name length
    emitter.instruction("str x12, [x11, x10, lsl #3]");                         // record the owned name length
    abi::emit_symbol_address(emitter, "x11", "_ob_chunk_sizes");                // materialize the chunk-size slot array
    emitter.instruction("ldr x12, [sp, #16]");                                  // reload the chunk size
    emitter.instruction("str x12, [x11, x10, lsl #3]");                         // record the chunk size
    abi::emit_symbol_address(emitter, "x11", "_ob_flags");                      // materialize the flags slot array
    emitter.instruction("ldr x12, [sp, #24]");                                  // reload the flags word
    emitter.instruction("str x12, [x11, x10, lsl #3]");                         // record the flags word
    abi::emit_symbol_address(emitter, "x11", "_ob_started");                    // materialize the started-flag slot array
    emitter.instruction("str xzr, [x11, x10, lsl #3]");                         // the handler has not run yet
    emitter.instruction("add x10, x10, #1");                                    // the stack is now one level deeper
    emitter.instruction("str x10, [x9]");                                       // publish the new depth
    emitter.instruction("mov x0, #1");                                          // report success
    emitter.label("__rt_ob_start_done");
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the ob_start frame
    emitter.instruction("ret");                                                 // return the success flag

    // -- default-handler compatibility wrapper --
    emitter.blank();
    emitter.label_global("__rt_ob_start");
    emitter.instruction("mov x0, #0");                                          // no handler stub (default handler)
    emitter.instruction("mov x1, #0");                                          // no handler env word
    emitter.instruction("mov x2, #0");                                          // no chunk size
    emitter.instruction("mov x3, #112");                                        // PHP_OUTPUT_HANDLER_STDFLAGS
    abi::emit_symbol_address(emitter, "x4", "_ob_handler_name");                // default display name pointer
    emitter.instruction(&format!("mov x5, #{}", OB_DEFAULT_HANDLER_NAME.len())); // default display name length
    emitter.instruction("b __rt_ob_start_ex");                                  // tail-call the full entry point
}

/// Emits the Linux x86_64 variant of `__rt_ob_start_ex` / `__rt_ob_start`.
fn emit_ob_start_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ob_start_ex / ob_start ---");
    emitter.label_global("__rt_ob_start_ex");
    // frame: [rbp-8]=stub, [rbp-16]=env, [rbp-24]=chunk, [rbp-32]=flags,
    //        [rbp-40]=name ptr, [rbp-48]=name len, [rbp-56]=capacity
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the ob_start frame pointer
    emitter.instruction("sub rsp, 64");                                         // reserve the ob_start local slots (16-aligned)
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the handler stub
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the handler env word
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the chunk size
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the flags word
    // -- PHP forbids starting a buffer from inside a running handler --
    abi::emit_symbol_address(emitter, "r10", "_ob_in_handler");                 // materialize the in-handler flag address
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // load the in-handler flag
    emitter.instruction("test r10, r10");                                       // is a user output handler running?
    emitter.instruction("jz __rt_ob_start_allowed_x86");                        // not inside a handler — proceed
    abi::emit_symbol_address(emitter, "rsi", "_ob_fatal_in_handler");           // load the nested-handler fatal message
    emitter.instruction(&format!("mov edx, {}", OB_FATAL_IN_HANDLER.len()));    // fatal message byte length
    emitter.instruction("mov edi, 1");                                          // fd = stdout (raw write; the funnel would discard it)
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 = write
    emitter.instruction("syscall");                                             // write the fatal message
    emitter.instruction("mov edi, 1");                                          // process exit code 1 like other runtime fatals
    emitter.instruction("mov eax, 60");                                         // Linux x86_64 syscall 60 = exit
    emitter.instruction("syscall");                                             // terminate the process
    emitter.label("__rt_ob_start_allowed_x86");
    // -- persist the display name so the slot always owns heap bytes --
    emitter.instruction("mov rax, r8");                                         // str_persist source pointer = the display name
    emitter.instruction("mov rdx, r9");                                         // str_persist source length = the display name length
    emitter.instruction("call __rt_str_persist");                               // copy the display name into an owned heap string
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the owned name pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                       // save the owned name length
    // -- refuse past the nesting limit with PHP's failed-create notice --
    abi::emit_symbol_address(emitter, "r9", "_ob_level");                       // materialize the address of the buffer-stack depth
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // load the current buffer-stack depth
    emitter.instruction(&format!("cmp r10, {}", OB_MAX_LEVELS));                // is the nesting limit reached?
    emitter.instruction("jl __rt_ob_start_capacity_x86");                       // room available — size the buffer
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the owned name for release
    emitter.instruction("call __rt_decref_any");                                // release the unused owned name
    abi::emit_symbol_address(emitter, "rdi", "_ob_ntc_create_fail");            // load the failed-create notice line
    emitter.instruction(&format!("mov esi, {}", OB_NTC_CREATE_FAIL.len()));     // notice byte length
    emitter.instruction("call __rt_stdout_write");                              // write the notice through the funnel
    emitter.instruction("xor eax, eax");                                        // report failure
    emitter.instruction("jmp __rt_ob_start_done_x86");                          // finish
    // -- PHP-shaped capacity: 16384 default, page-aligned chunk+1 otherwise --
    emitter.label("__rt_ob_start_capacity_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the chunk size
    emitter.instruction("test r10, r10");                                       // was a chunk size requested?
    emitter.instruction("jnz __rt_ob_start_chunk_cap_x86");                     // a chunk size selects the aligned capacity
    emitter.instruction(&format!("mov rax, {}", OB_DEFAULT_CAPACITY));          // default buffer capacity
    emitter.instruction("jmp __rt_ob_start_alloc_x86");                         // allocate the buffer
    emitter.label("__rt_ob_start_chunk_cap_x86");
    emitter.instruction("shr r10, 12");                                         // whole 4 KiB pages in the chunk size
    emitter.instruction("add r10, 1");                                          // round up to the next page
    emitter.instruction("shl r10, 12");                                         // capacity = ((chunk >> 12) + 1) << 12
    emitter.instruction("mov rax, r10");                                        // pass the capacity to the allocator
    emitter.label("__rt_ob_start_alloc_x86");
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save the chosen capacity
    emitter.instruction("call __rt_heap_alloc");                                // allocate the capture buffer (raw kind)
    // -- record every per-slot field --
    abi::emit_symbol_address(emitter, "r9", "_ob_level");                       // materialize the address of the buffer-stack depth
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // reload the depth (the new slot index)
    abi::emit_symbol_address(emitter, "r11", "_ob_ptrs");                       // materialize the buffer-pointer slot array
    emitter.instruction("mov QWORD PTR [r11 + r10*8], rax");                    // record the new buffer base pointer
    abi::emit_symbol_address(emitter, "r11", "_ob_lens");                       // materialize the used-bytes slot array
    emitter.instruction("mov QWORD PTR [r11 + r10*8], 0");                      // the new buffer starts empty
    abi::emit_symbol_address(emitter, "r11", "_ob_caps");                       // materialize the capacity slot array
    emitter.instruction("mov rcx, QWORD PTR [rbp - 56]");                       // reload the chosen capacity
    emitter.instruction("mov QWORD PTR [r11 + r10*8], rcx");                    // record the capacity
    abi::emit_symbol_address(emitter, "r11", "_ob_handler_stubs");              // materialize the handler-stub slot array
    emitter.instruction("mov rcx, QWORD PTR [rbp - 8]");                        // reload the handler stub
    emitter.instruction("mov QWORD PTR [r11 + r10*8], rcx");                    // record the handler stub
    abi::emit_symbol_address(emitter, "r11", "_ob_handler_envs");               // materialize the handler-env slot array
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the handler env word
    emitter.instruction("mov QWORD PTR [r11 + r10*8], rcx");                    // record the handler env word
    abi::emit_symbol_address(emitter, "r11", "_ob_name_ptrs");                  // materialize the handler-name pointer array
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // reload the owned name pointer
    emitter.instruction("mov QWORD PTR [r11 + r10*8], rcx");                    // record the owned name pointer
    abi::emit_symbol_address(emitter, "r11", "_ob_name_lens");                  // materialize the handler-name length array
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the owned name length
    emitter.instruction("mov QWORD PTR [r11 + r10*8], rcx");                    // record the owned name length
    abi::emit_symbol_address(emitter, "r11", "_ob_chunk_sizes");                // materialize the chunk-size slot array
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload the chunk size
    emitter.instruction("mov QWORD PTR [r11 + r10*8], rcx");                    // record the chunk size
    abi::emit_symbol_address(emitter, "r11", "_ob_flags");                      // materialize the flags slot array
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the flags word
    emitter.instruction("mov QWORD PTR [r11 + r10*8], rcx");                    // record the flags word
    abi::emit_symbol_address(emitter, "r11", "_ob_started");                    // materialize the started-flag slot array
    emitter.instruction("mov QWORD PTR [r11 + r10*8], 0");                      // the handler has not run yet
    emitter.instruction("add r10, 1");                                          // the stack is now one level deeper
    emitter.instruction("mov QWORD PTR [r9], r10");                             // publish the new depth
    emitter.instruction("mov eax, 1");                                          // report success
    emitter.label("__rt_ob_start_done_x86");
    emitter.instruction("add rsp, 64");                                         // release the ob_start local slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the success flag

    // -- default-handler compatibility wrapper --
    emitter.blank();
    emitter.label_global("__rt_ob_start");
    emitter.instruction("xor edi, edi");                                        // no handler stub (default handler)
    emitter.instruction("xor esi, esi");                                        // no handler env word
    emitter.instruction("xor edx, edx");                                        // no chunk size
    emitter.instruction("mov ecx, 112");                                        // PHP_OUTPUT_HANDLER_STDFLAGS
    abi::emit_symbol_address(emitter, "r8", "_ob_handler_name");                // default display name pointer
    emitter.instruction(&format!("mov r9, {}", OB_DEFAULT_HANDLER_NAME.len())); // default display name length
    emitter.instruction("jmp __rt_ob_start_ex");                                // tail-call the full entry point
}

/// Emits `__rt_ob_process_and_write`: shared flush/clean core for one slot.
///
/// Inputs: `x0`/`rdi` = slot index, `x1`/`rsi` = handler phase bits,
/// `x2`/`rdx` = write flag (non-zero: emit the surviving bytes to the parent
/// sink). No result. Runs the handler, substitutes its replacement on write
/// paths, discards it on clean paths, and truncates the buffer.
pub fn emit_ob_process_and_write(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_ob_process_and_write_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: ob_process_and_write ---");
    emitter.label_global("__rt_ob_process_and_write");
    // frame: [0]=slot, [8]=write flag, [16]=replaced?, [24]=rep ptr, [32]=rep len
    emitter.instruction("sub sp, sp, #64");                                     // allocate the process frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the process frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the slot index
    emitter.instruction("str x2, [sp, #8]");                                    // save the write flag
    emitter.instruction("bl __rt_ob_apply_handler");                            // run the user handler (slot in x0, phase in x1)
    emitter.instruction("str x0, [sp, #16]");                                   // save the replaced flag
    emitter.instruction("str x1, [sp, #24]");                                   // save the replacement pointer
    emitter.instruction("str x2, [sp, #32]");                                   // save the replacement length
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the write flag
    emitter.instruction("cbz x9, __rt_ob_process_free_rep");                    // clean path — discard without writing
    // -- choose the surviving bytes: replacement or raw buffer --
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the replaced flag
    emitter.instruction("cbz x9, __rt_ob_process_raw");                         // pass-through — write the raw buffer
    emitter.instruction("ldr x0, [sp, #24]");                                   // write pointer = the replacement string
    emitter.instruction("ldr x1, [sp, #32]");                                   // write length = the replacement length
    emitter.instruction("b __rt_ob_process_have_bytes");                        // emit the chosen bytes
    emitter.label("__rt_ob_process_raw");
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the slot index
    abi::emit_symbol_address(emitter, "x11", "_ob_ptrs");                       // materialize the buffer-pointer slot array
    emitter.instruction("ldr x0, [x11, x10, lsl #3]");                          // write pointer = the raw buffer base
    abi::emit_symbol_address(emitter, "x11", "_ob_lens");                       // materialize the used-bytes slot array
    emitter.instruction("ldr x1, [x11, x10, lsl #3]");                          // write length = the raw byte count
    emitter.label("__rt_ob_process_have_bytes");
    emitter.instruction("cbz x1, __rt_ob_process_free_rep");                    // nothing to write — skip the parent write
    // -- publish level = slot so the write routes to the parent sink --
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the slot index
    abi::emit_symbol_address(emitter, "x9", "_ob_level");                       // materialize the address of the buffer-stack depth
    emitter.instruction("str x10, [x9]");                                       // temporarily pop the level for parent routing
    emitter.instruction("bl __rt_stdout_write");                                // write the surviving bytes to the parent sink
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the slot index
    emitter.instruction("add x10, x10, #1");                                    // restore depth = slot + 1
    abi::emit_symbol_address(emitter, "x9", "_ob_level");                       // materialize the address of the buffer-stack depth
    emitter.instruction("str x10, [x9]");                                       // restore the buffer-stack depth
    emitter.label("__rt_ob_process_free_rep");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the replaced flag
    emitter.instruction("cbz x9, __rt_ob_process_truncate");                    // no replacement to free
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the replacement string
    emitter.instruction("bl __rt_decref_any");                                  // release the replacement string
    emitter.label("__rt_ob_process_truncate");
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the slot index
    abi::emit_symbol_address(emitter, "x11", "_ob_lens");                       // materialize the used-bytes slot array
    emitter.instruction("str xzr, [x11, x10, lsl #3]");                         // truncate the processed buffer
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the process frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 variant of `__rt_ob_process_and_write`.
fn emit_ob_process_and_write_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ob_process_and_write ---");
    emitter.label_global("__rt_ob_process_and_write");
    // frame: [rbp-8]=slot, [rbp-16]=write flag, [rbp-24]=replaced?, [rbp-32]=rep ptr, [rbp-40]=rep len
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the process frame pointer
    emitter.instruction("sub rsp, 48");                                         // reserve the process local slots (16-aligned)
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the slot index
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the write flag
    emitter.instruction("call __rt_ob_apply_handler");                          // run the user handler (slot in rdi, phase in rsi)
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the replaced flag
    emitter.instruction("mov QWORD PTR [rbp - 32], rdi");                       // save the replacement pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // save the replacement length
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload the write flag
    emitter.instruction("test r9, r9");                                         // is this a flush path?
    emitter.instruction("jz __rt_ob_process_free_rep_x86");                     // clean path — discard without writing
    // -- choose the surviving bytes: replacement or raw buffer --
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload the replaced flag
    emitter.instruction("test r9, r9");                                         // did the handler replace the contents?
    emitter.instruction("jz __rt_ob_process_raw_x86");                          // pass-through — write the raw buffer
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // write pointer = the replacement string
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // write length = the replacement length
    emitter.instruction("jmp __rt_ob_process_have_bytes_x86");                  // emit the chosen bytes
    emitter.label("__rt_ob_process_raw_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the slot index
    abi::emit_symbol_address(emitter, "r11", "_ob_ptrs");                       // materialize the buffer-pointer slot array
    emitter.instruction("mov rdi, QWORD PTR [r11 + r10*8]");                    // write pointer = the raw buffer base
    abi::emit_symbol_address(emitter, "r11", "_ob_lens");                       // materialize the used-bytes slot array
    emitter.instruction("mov rsi, QWORD PTR [r11 + r10*8]");                    // write length = the raw byte count
    emitter.label("__rt_ob_process_have_bytes_x86");
    emitter.instruction("test rsi, rsi");                                       // is there anything to write?
    emitter.instruction("jz __rt_ob_process_free_rep_x86");                     // nothing to write — skip the parent write
    // -- publish level = slot so the write routes to the parent sink --
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the slot index
    abi::emit_symbol_address(emitter, "r9", "_ob_level");                       // materialize the address of the buffer-stack depth
    emitter.instruction("mov QWORD PTR [r9], r10");                             // temporarily pop the level for parent routing
    emitter.instruction("call __rt_stdout_write");                              // write the surviving bytes to the parent sink
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the slot index
    emitter.instruction("add r10, 1");                                          // restore depth = slot + 1
    abi::emit_symbol_address(emitter, "r9", "_ob_level");                       // materialize the address of the buffer-stack depth
    emitter.instruction("mov QWORD PTR [r9], r10");                             // restore the buffer-stack depth
    emitter.label("__rt_ob_process_free_rep_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload the replaced flag
    emitter.instruction("test r9, r9");                                         // was a replacement allocated?
    emitter.instruction("jz __rt_ob_process_truncate_x86");                     // no replacement to free
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the replacement string
    emitter.instruction("call __rt_decref_any");                                // release the replacement string
    emitter.label("__rt_ob_process_truncate_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the slot index
    abi::emit_symbol_address(emitter, "r11", "_ob_lens");                       // materialize the used-bytes slot array
    emitter.instruction("mov QWORD PTR [r11 + r10*8], 0");                      // truncate the processed buffer
    emitter.instruction("add rsp, 48");                                         // release the process local slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits `__rt_ob_pop_free`: pop the top buffer, releasing its handler
/// descriptor (when AOT-owned), its persisted display name, and its storage.
///
/// No inputs, no result. Publishes the decremented depth before freeing so a
/// fatal inside the deallocator cannot observe the dying buffer.
pub fn emit_ob_pop_free(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_ob_pop_free_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: ob_pop_free ---");
    emitter.label_global("__rt_ob_pop_free");
    emitter.instruction("sub sp, sp, #32");                                     // allocate the pop frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish the pop frame pointer
    abi::emit_symbol_address(emitter, "x9", "_ob_level");                       // materialize the address of the buffer-stack depth
    emitter.instruction("ldr x10, [x9]");                                       // load the current buffer-stack depth
    emitter.instruction("cbz x10, __rt_ob_pop_done");                           // defensive: nothing to pop
    emitter.instruction("sub x10, x10, #1");                                    // dying slot index = depth - 1
    emitter.instruction("str x10, [x9]");                                       // publish the popped depth before freeing
    emitter.instruction("str x10, [sp, #0]");                                   // save the dying slot index
    // -- release an AOT handler descriptor (env is a retained descriptor) --
    abi::emit_symbol_address(emitter, "x11", "_ob_handler_stubs");              // materialize the handler-stub slot array
    emitter.instruction("ldr x12, [x11, x10, lsl #3]");                         // load the slot's handler stub
    abi::emit_symbol_address(emitter, "x13", "__rt_ob_invoke_descriptor");      // materialize the descriptor-invoker stub address
    emitter.instruction("cmp x12, x13");                                        // is the env a retained callable descriptor?
    emitter.instruction("b.ne __rt_ob_pop_name");                               // eval/default handlers carry no descriptor
    abi::emit_symbol_address(emitter, "x11", "_ob_handler_envs");               // materialize the handler-env slot array
    emitter.instruction("ldr x0, [x11, x10, lsl #3]");                          // load the retained descriptor pointer
    emitter.instruction("cbz x0, __rt_ob_pop_name");                            // defensive: nothing to release
    emitter.instruction("bl __rt_decref_any");                                  // release the retained descriptor
    emitter.label("__rt_ob_pop_name");
    // -- release the persisted display name --
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the dying slot index
    abi::emit_symbol_address(emitter, "x11", "_ob_name_ptrs");                  // materialize the handler-name pointer array
    emitter.instruction("ldr x0, [x11, x10, lsl #3]");                          // load the persisted display name
    emitter.instruction("bl __rt_decref_any");                                  // release the persisted display name
    // -- release the capture buffer block --
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the dying slot index
    abi::emit_symbol_address(emitter, "x11", "_ob_ptrs");                       // materialize the buffer-pointer slot array
    emitter.instruction("ldr x0, [x11, x10, lsl #3]");                          // load the dying buffer base pointer
    emitter.instruction("bl __rt_heap_free");                                   // release the dying buffer block
    emitter.label("__rt_ob_pop_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the pop frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 variant of `__rt_ob_pop_free`.
fn emit_ob_pop_free_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ob_pop_free ---");
    emitter.label_global("__rt_ob_pop_free");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the pop frame pointer
    emitter.instruction("sub rsp, 16");                                         // reserve an aligned slot for the dying index
    abi::emit_symbol_address(emitter, "r9", "_ob_level");                       // materialize the address of the buffer-stack depth
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // load the current buffer-stack depth
    emitter.instruction("test r10, r10");                                       // defensive: is anything active?
    emitter.instruction("jz __rt_ob_pop_done_x86");                             // nothing to pop
    emitter.instruction("sub r10, 1");                                          // dying slot index = depth - 1
    emitter.instruction("mov QWORD PTR [r9], r10");                             // publish the popped depth before freeing
    emitter.instruction("mov QWORD PTR [rbp - 8], r10");                        // save the dying slot index
    // -- release an AOT handler descriptor (env is a retained descriptor) --
    abi::emit_symbol_address(emitter, "r11", "_ob_handler_stubs");              // materialize the handler-stub slot array
    emitter.instruction("mov rcx, QWORD PTR [r11 + r10*8]");                    // load the slot's handler stub
    abi::emit_symbol_address(emitter, "r8", "__rt_ob_invoke_descriptor");       // materialize the descriptor-invoker stub address
    emitter.instruction("cmp rcx, r8");                                         // is the env a retained callable descriptor?
    emitter.instruction("jne __rt_ob_pop_name_x86");                            // eval/default handlers carry no descriptor
    abi::emit_symbol_address(emitter, "r11", "_ob_handler_envs");               // materialize the handler-env slot array
    emitter.instruction("mov rax, QWORD PTR [r11 + r10*8]");                    // load the retained descriptor pointer
    emitter.instruction("test rax, rax");                                       // defensive: is a descriptor recorded?
    emitter.instruction("jz __rt_ob_pop_name_x86");                             // nothing to release
    emitter.instruction("call __rt_decref_any");                                // release the retained descriptor
    emitter.label("__rt_ob_pop_name_x86");
    // -- release the persisted display name --
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the dying slot index
    abi::emit_symbol_address(emitter, "r11", "_ob_name_ptrs");                  // materialize the handler-name pointer array
    emitter.instruction("mov rax, QWORD PTR [r11 + r10*8]");                    // load the persisted display name
    emitter.instruction("call __rt_decref_any");                                // release the persisted display name
    // -- release the capture buffer block --
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the dying slot index
    abi::emit_symbol_address(emitter, "r11", "_ob_ptrs");                       // materialize the buffer-pointer slot array
    emitter.instruction("mov rax, QWORD PTR [r11 + r10*8]");                    // load the dying buffer base pointer
    emitter.instruction("call __rt_heap_free");                                 // release the dying buffer block
    emitter.label("__rt_ob_pop_done_x86");
    emitter.instruction("add rsp, 16");                                         // release the pop frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits `__rt_ob_append`: append bytes to the top output buffer, growing it as
/// needed and honoring the PHP chunk-size auto-flush.
///
/// Inputs: AArch64 `x0`=src pointer, `x1`=length / x86_64 `rdi`=src, `rsi`=length.
/// No result. A zero `_ob_level` makes the call a defensive no-op. After the
/// append, a non-zero chunk size whose threshold is reached triggers an
/// auto-flush with the WRITE handler phase.
pub fn emit_ob_append(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_ob_append_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: ob_append ---");
    emitter.label_global("__rt_ob_append");
    // frame: [0]=src, [8]=len, [16]=slot index, [24]=used, [32]=new capacity
    emitter.instruction("sub sp, sp, #64");                                     // allocate the ob_append frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the ob_append frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the source byte pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the source byte length
    abi::emit_symbol_address(emitter, "x9", "_ob_level");                       // materialize the address of the buffer-stack depth
    emitter.instruction("ldr x10, [x9]");                                       // load the current buffer-stack depth
    emitter.instruction("cbz x10, __rt_ob_append_done");                        // no active buffer — defensive no-op
    emitter.instruction("sub x10, x10, #1");                                    // top slot index = depth - 1
    emitter.instruction("str x10, [sp, #16]");                                  // save the top slot index
    abi::emit_symbol_address(emitter, "x11", "_ob_lens");                       // materialize the used-bytes slot array
    emitter.instruction("ldr x12, [x11, x10, lsl #3]");                         // load the top buffer's used byte count
    emitter.instruction("str x12, [sp, #24]");                                  // save the used byte count
    abi::emit_symbol_address(emitter, "x13", "_ob_caps");                       // materialize the capacity slot array
    emitter.instruction("ldr x14, [x13, x10, lsl #3]");                         // load the top buffer's capacity
    emitter.instruction("ldr x15, [sp, #8]");                                   // reload the incoming byte length
    emitter.instruction("add x15, x12, x15");                                   // needed bytes = used + incoming length
    emitter.instruction("cmp x15, x14");                                        // does the payload fit the current capacity?
    emitter.instruction("b.ls __rt_ob_append_copy");                            // fits — skip the growth path
    // -- grow: double the capacity until the payload fits --
    emitter.label("__rt_ob_append_grow_size");
    emitter.instruction("lsl x14, x14, #1");                                    // double the candidate capacity
    emitter.instruction("cmp x14, x15");                                        // does the doubled capacity fit the payload?
    emitter.instruction("b.lo __rt_ob_append_grow_size");                       // keep doubling until it fits
    emitter.instruction("str x14, [sp, #32]");                                  // save the new capacity
    emitter.instruction("mov x0, x14");                                         // request the new capacity from the allocator
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the replacement buffer (raw kind)
    // -- copy the used prefix from the old buffer into the replacement --
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the top slot index
    abi::emit_symbol_address(emitter, "x11", "_ob_ptrs");                       // materialize the buffer-pointer slot array
    emitter.instruction("ldr x12, [x11, x10, lsl #3]");                         // load the old buffer base pointer
    emitter.instruction("ldr x13, [sp, #24]");                                  // reload the used byte count
    emitter.instruction("mov x14, #0");                                         // start the copy at offset zero
    emitter.label("__rt_ob_append_grow_copy");
    emitter.instruction("cmp x14, x13");                                        // copied all used bytes?
    emitter.instruction("b.ge __rt_ob_append_grow_swap");                       // yes — publish the replacement buffer
    emitter.instruction("ldrb w15, [x12, x14]");                                // load the next byte from the old buffer
    emitter.instruction("strb w15, [x0, x14]");                                 // store the byte into the replacement buffer
    emitter.instruction("add x14, x14, #1");                                    // advance the copy cursor
    emitter.instruction("b __rt_ob_append_grow_copy");                          // continue copying the used prefix
    emitter.label("__rt_ob_append_grow_swap");
    emitter.instruction("str x0, [x11, x10, lsl #3]");                          // publish the replacement buffer base pointer
    abi::emit_symbol_address(emitter, "x13", "_ob_caps");                       // materialize the capacity slot array
    emitter.instruction("ldr x14, [sp, #32]");                                  // reload the new capacity
    emitter.instruction("str x14, [x13, x10, lsl #3]");                         // publish the new capacity
    emitter.instruction("mov x0, x12");                                         // pass the old buffer to the deallocator
    emitter.instruction("bl __rt_heap_free");                                   // release the old buffer block
    // -- copy the incoming bytes to buffer base + used --
    emitter.label("__rt_ob_append_copy");
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the top slot index
    abi::emit_symbol_address(emitter, "x11", "_ob_ptrs");                       // materialize the buffer-pointer slot array
    emitter.instruction("ldr x12, [x11, x10, lsl #3]");                         // load the (possibly replaced) buffer base pointer
    emitter.instruction("ldr x13, [sp, #24]");                                  // reload the used byte count
    emitter.instruction("add x12, x12, x13");                                   // destination cursor = base + used
    emitter.instruction("ldr x14, [sp, #0]");                                   // reload the source byte pointer
    emitter.instruction("ldr x15, [sp, #8]");                                   // reload the source byte length
    emitter.label("__rt_ob_append_copy_loop");
    emitter.instruction("cbz x15, __rt_ob_append_publish");                     // no bytes left — publish the new length
    emitter.instruction("ldrb w9, [x14]");                                      // load the next source byte
    emitter.instruction("strb w9, [x12]");                                      // store the byte at the destination cursor
    emitter.instruction("add x14, x14, #1");                                    // advance the source cursor
    emitter.instruction("add x12, x12, #1");                                    // advance the destination cursor
    emitter.instruction("sub x15, x15, #1");                                    // one byte fewer to copy
    emitter.instruction("b __rt_ob_append_copy_loop");                          // continue copying the incoming bytes
    emitter.label("__rt_ob_append_publish");
    emitter.instruction("ldr x13, [sp, #24]");                                  // reload the used byte count
    emitter.instruction("ldr x14, [sp, #8]");                                   // reload the incoming byte length
    emitter.instruction("add x13, x13, x14");                                   // new used count = used + incoming length
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the top slot index
    abi::emit_symbol_address(emitter, "x11", "_ob_lens");                       // materialize the used-bytes slot array
    emitter.instruction("str x13, [x11, x10, lsl #3]");                         // publish the new used byte count
    // -- PHP chunk-size auto-flush: reaching the threshold flushes the buffer --
    abi::emit_symbol_address(emitter, "x11", "_ob_chunk_sizes");                // materialize the chunk-size slot array
    emitter.instruction("ldr x12, [x11, x10, lsl #3]");                         // load the slot's chunk size
    emitter.instruction("cbz x12, __rt_ob_append_done");                        // chunking disabled — done
    emitter.instruction("cmp x13, x12");                                        // did the buffer reach the chunk threshold?
    emitter.instruction("b.lo __rt_ob_append_done");                            // below the threshold — done
    emitter.instruction("mov x0, x10");                                         // auto-flush the top slot
    emitter.instruction("mov x1, #0");                                          // handler phase = PHP_OUTPUT_HANDLER_WRITE
    emitter.instruction("mov x2, #1");                                          // emit the surviving bytes to the parent sink
    emitter.instruction("bl __rt_ob_process_and_write");                        // run the chunk auto-flush
    emitter.label("__rt_ob_append_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the ob_append frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 variant of `__rt_ob_append`.
fn emit_ob_append_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ob_append ---");
    emitter.label_global("__rt_ob_append");
    // frame: [rbp-8]=src, [rbp-16]=len, [rbp-24]=slot index, [rbp-32]=used, [rbp-40]=new capacity
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the ob_append frame pointer
    emitter.instruction("sub rsp, 48");                                         // reserve the ob_append local slots (16-aligned)
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the source byte pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the source byte length
    abi::emit_symbol_address(emitter, "r9", "_ob_level");                       // materialize the address of the buffer-stack depth
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // load the current buffer-stack depth
    emitter.instruction("test r10, r10");                                       // is any buffer active?
    emitter.instruction("jz __rt_ob_append_done_x86");                          // no active buffer — defensive no-op
    emitter.instruction("sub r10, 1");                                          // top slot index = depth - 1
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save the top slot index
    abi::emit_symbol_address(emitter, "r11", "_ob_lens");                       // materialize the used-bytes slot array
    emitter.instruction("mov rcx, QWORD PTR [r11 + r10*8]");                    // load the top buffer's used byte count
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the used byte count
    abi::emit_symbol_address(emitter, "r11", "_ob_caps");                       // materialize the capacity slot array
    emitter.instruction("mov rdx, QWORD PTR [r11 + r10*8]");                    // load the top buffer's capacity
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // reload the incoming byte length
    emitter.instruction("add r8, rcx");                                         // needed bytes = used + incoming length
    emitter.instruction("cmp r8, rdx");                                         // does the payload fit the current capacity?
    emitter.instruction("jbe __rt_ob_append_copy_x86");                         // fits — skip the growth path
    // -- grow: double the capacity until the payload fits --
    emitter.label("__rt_ob_append_grow_size_x86");
    emitter.instruction("shl rdx, 1");                                          // double the candidate capacity
    emitter.instruction("cmp rdx, r8");                                         // does the doubled capacity fit the payload?
    emitter.instruction("jb __rt_ob_append_grow_size_x86");                     // keep doubling until it fits
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // save the new capacity
    emitter.instruction("mov rax, rdx");                                        // request the new capacity from the allocator
    emitter.instruction("call __rt_heap_alloc");                                // allocate the replacement buffer (raw kind)
    // -- copy the used prefix from the old buffer into the replacement --
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the top slot index
    abi::emit_symbol_address(emitter, "r11", "_ob_ptrs");                       // materialize the buffer-pointer slot array
    emitter.instruction("mov rcx, QWORD PTR [r11 + r10*8]");                    // load the old buffer base pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // reload the used byte count
    emitter.instruction("xor r8d, r8d");                                        // start the copy at offset zero
    emitter.label("__rt_ob_append_grow_copy_x86");
    emitter.instruction("cmp r8, rdx");                                         // copied all used bytes?
    emitter.instruction("jge __rt_ob_append_grow_swap_x86");                    // yes — publish the replacement buffer
    emitter.instruction("mov r9b, BYTE PTR [rcx + r8]");                        // load the next byte from the old buffer
    emitter.instruction("mov BYTE PTR [rax + r8], r9b");                        // store the byte into the replacement buffer
    emitter.instruction("add r8, 1");                                           // advance the copy cursor
    emitter.instruction("jmp __rt_ob_append_grow_copy_x86");                    // continue copying the used prefix
    emitter.label("__rt_ob_append_grow_swap_x86");
    emitter.instruction("mov QWORD PTR [r11 + r10*8], rax");                    // publish the replacement buffer base pointer
    abi::emit_symbol_address(emitter, "r9", "_ob_caps");                        // materialize the capacity slot array
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // reload the new capacity
    emitter.instruction("mov QWORD PTR [r9 + r10*8], rdx");                     // publish the new capacity
    emitter.instruction("mov rax, rcx");                                        // pass the old buffer to the deallocator
    emitter.instruction("call __rt_heap_free");                                 // release the old buffer block
    // -- copy the incoming bytes to buffer base + used --
    emitter.label("__rt_ob_append_copy_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the top slot index
    abi::emit_symbol_address(emitter, "r11", "_ob_ptrs");                       // materialize the buffer-pointer slot array
    emitter.instruction("mov rcx, QWORD PTR [r11 + r10*8]");                    // load the (possibly replaced) buffer base pointer
    emitter.instruction("add rcx, QWORD PTR [rbp - 32]");                       // destination cursor = base + used
    emitter.instruction("mov rdx, QWORD PTR [rbp - 8]");                        // reload the source byte pointer
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // reload the source byte length
    emitter.label("__rt_ob_append_copy_loop_x86");
    emitter.instruction("test r8, r8");                                         // any bytes left to copy?
    emitter.instruction("jz __rt_ob_append_publish_x86");                       // no bytes left — publish the new length
    emitter.instruction("mov r9b, BYTE PTR [rdx]");                             // load the next source byte
    emitter.instruction("mov BYTE PTR [rcx], r9b");                             // store the byte at the destination cursor
    emitter.instruction("add rdx, 1");                                          // advance the source cursor
    emitter.instruction("add rcx, 1");                                          // advance the destination cursor
    emitter.instruction("sub r8, 1");                                           // one byte fewer to copy
    emitter.instruction("jmp __rt_ob_append_copy_loop_x86");                    // continue copying the incoming bytes
    emitter.label("__rt_ob_append_publish_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the used byte count
    emitter.instruction("add rcx, QWORD PTR [rbp - 16]");                       // new used count = used + incoming length
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the top slot index
    abi::emit_symbol_address(emitter, "r11", "_ob_lens");                       // materialize the used-bytes slot array
    emitter.instruction("mov QWORD PTR [r11 + r10*8], rcx");                    // publish the new used byte count
    // -- PHP chunk-size auto-flush: reaching the threshold flushes the buffer --
    abi::emit_symbol_address(emitter, "r11", "_ob_chunk_sizes");                // materialize the chunk-size slot array
    emitter.instruction("mov rdx, QWORD PTR [r11 + r10*8]");                    // load the slot's chunk size
    emitter.instruction("test rdx, rdx");                                       // is chunking enabled for this buffer?
    emitter.instruction("jz __rt_ob_append_done_x86");                          // chunking disabled — done
    emitter.instruction("cmp rcx, rdx");                                        // did the buffer reach the chunk threshold?
    emitter.instruction("jb __rt_ob_append_done_x86");                          // below the threshold — done
    emitter.instruction("mov rdi, r10");                                        // auto-flush the top slot
    emitter.instruction("xor esi, esi");                                        // handler phase = PHP_OUTPUT_HANDLER_WRITE
    emitter.instruction("mov edx, 1");                                          // emit the surviving bytes to the parent sink
    emitter.instruction("call __rt_ob_process_and_write");                      // run the chunk auto-flush
    emitter.label("__rt_ob_append_done_x86");
    emitter.instruction("add rsp, 48");                                         // release the ob_append local slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits `__rt_ob_contents`: return a persisted copy of the top buffer contents.
///
/// No inputs. Returns the platform string result pair (AArch64 `x1`=ptr, `x2`=len /
/// x86_64 `rax`=ptr, `rdx`=len); a null pointer signals "no active buffer" so the
/// caller can box PHP `false`.
pub fn emit_ob_contents(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_ob_contents_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: ob_contents ---");
    emitter.label_global("__rt_ob_contents");
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish a frame pointer for the persist call
    abi::emit_symbol_address(emitter, "x9", "_ob_level");                       // materialize the address of the buffer-stack depth
    emitter.instruction("ldr x10, [x9]");                                       // load the current buffer-stack depth
    emitter.instruction("cbz x10, __rt_ob_contents_none");                      // no active buffer — return the null failure pair
    emitter.instruction("sub x10, x10, #1");                                    // top slot index = depth - 1
    abi::emit_symbol_address(emitter, "x11", "_ob_ptrs");                       // materialize the buffer-pointer slot array
    emitter.instruction("ldr x1, [x11, x10, lsl #3]");                          // load the top buffer base pointer
    abi::emit_symbol_address(emitter, "x11", "_ob_lens");                       // materialize the used-bytes slot array
    emitter.instruction("ldr x2, [x11, x10, lsl #3]");                          // load the top buffer's used byte count
    emitter.instruction("bl __rt_str_persist");                                 // copy the contents into an owned heap string
    emitter.instruction("b __rt_ob_contents_done");                             // return the persisted string pair
    emitter.label("__rt_ob_contents_none");
    emitter.instruction("mov x1, #0");                                          // null pointer signals "no active buffer"
    emitter.instruction("mov x2, #0");                                          // zero length for the failure pair
    emitter.label("__rt_ob_contents_done");
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore frame pointer and return address
    emitter.instruction("ret");                                                 // return the string result pair
}

/// Emits the Linux x86_64 variant of `__rt_ob_contents`.
fn emit_ob_contents_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ob_contents ---");
    emitter.label_global("__rt_ob_contents");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer across the persist call
    emitter.instruction("mov rbp, rsp");                                        // establish a frame pointer for the persist call
    abi::emit_symbol_address(emitter, "r9", "_ob_level");                       // materialize the address of the buffer-stack depth
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // load the current buffer-stack depth
    emitter.instruction("test r10, r10");                                       // is any buffer active?
    emitter.instruction("jz __rt_ob_contents_none_x86");                        // no active buffer — return the null failure pair
    emitter.instruction("sub r10, 1");                                          // top slot index = depth - 1
    abi::emit_symbol_address(emitter, "r11", "_ob_ptrs");                       // materialize the buffer-pointer slot array
    emitter.instruction("mov rax, QWORD PTR [r11 + r10*8]");                    // load the top buffer base pointer
    abi::emit_symbol_address(emitter, "r11", "_ob_lens");                       // materialize the used-bytes slot array
    emitter.instruction("mov rdx, QWORD PTR [r11 + r10*8]");                    // load the top buffer's used byte count
    emitter.instruction("call __rt_str_persist");                               // copy the contents into an owned heap string
    emitter.instruction("jmp __rt_ob_contents_done_x86");                       // return the persisted string pair
    emitter.label("__rt_ob_contents_none_x86");
    emitter.instruction("xor eax, eax");                                        // null pointer signals "no active buffer"
    emitter.instruction("xor edx, edx");                                        // zero length for the failure pair
    emitter.label("__rt_ob_contents_done_x86");
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the string result pair
}

/// Emits `__rt_ob_length` and `__rt_ob_level`: the buffer-stack integer queries.
///
/// `__rt_ob_length` returns the top buffer's used byte count in `x0`/`rax`, or -1
/// when no buffer is active. `__rt_ob_level` returns the nesting depth. Neither
/// takes inputs nor calls other helpers.
pub fn emit_ob_queries(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_ob_queries_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: ob_length / ob_level ---");
    emitter.label_global("__rt_ob_length");
    abi::emit_symbol_address(emitter, "x9", "_ob_level");                       // materialize the address of the buffer-stack depth
    emitter.instruction("ldr x10, [x9]");                                       // load the current buffer-stack depth
    emitter.instruction("cbz x10, __rt_ob_length_none");                        // no active buffer — return the -1 sentinel
    emitter.instruction("sub x10, x10, #1");                                    // top slot index = depth - 1
    abi::emit_symbol_address(emitter, "x11", "_ob_lens");                       // materialize the used-bytes slot array
    emitter.instruction("ldr x0, [x11, x10, lsl #3]");                          // return the top buffer's used byte count
    emitter.instruction("ret");                                                 // return the length
    emitter.label("__rt_ob_length_none");
    emitter.instruction("mov x0, #-1");                                         // -1 signals "no active buffer"
    emitter.instruction("ret");                                                 // return the failure sentinel

    emitter.blank();
    emitter.label_global("__rt_ob_level");
    abi::emit_symbol_address(emitter, "x9", "_ob_level");                       // materialize the address of the buffer-stack depth
    emitter.instruction("ldr x0, [x9]");                                        // return the current nesting depth
    emitter.instruction("ret");                                                 // return the level
}

/// Emits the Linux x86_64 variants of `__rt_ob_length` and `__rt_ob_level`.
fn emit_ob_queries_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ob_length / ob_level ---");
    emitter.label_global("__rt_ob_length");
    abi::emit_symbol_address(emitter, "r9", "_ob_level");                       // materialize the address of the buffer-stack depth
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // load the current buffer-stack depth
    emitter.instruction("test r10, r10");                                       // is any buffer active?
    emitter.instruction("jz __rt_ob_length_none_x86");                          // no active buffer — return the -1 sentinel
    emitter.instruction("sub r10, 1");                                          // top slot index = depth - 1
    abi::emit_symbol_address(emitter, "r11", "_ob_lens");                       // materialize the used-bytes slot array
    emitter.instruction("mov rax, QWORD PTR [r11 + r10*8]");                    // return the top buffer's used byte count
    emitter.instruction("ret");                                                 // return the length
    emitter.label("__rt_ob_length_none_x86");
    emitter.instruction("mov rax, -1");                                         // -1 signals "no active buffer"
    emitter.instruction("ret");                                                 // return the failure sentinel

    emitter.blank();
    emitter.label_global("__rt_ob_level");
    abi::emit_symbol_address(emitter, "r9", "_ob_level");                       // materialize the address of the buffer-stack depth
    emitter.instruction("mov rax, QWORD PTR [r9]");                             // return the current nesting depth
    emitter.instruction("ret");                                                 // return the level
}

/// Emits one flags-gated public ob_* mutation entry point (AArch64).
///
/// Shared shape: no buffer → write `no_buffer_msg` and return 0; missing
/// `required_flag` → `__rt_ob_notice_named(gated_msg, slot)` and return 0;
/// otherwise run `__rt_ob_process_and_write(slot, phase, write)` (and
/// `__rt_ob_pop_free` when `pop`) and return 1.
#[allow(clippy::too_many_arguments)]
fn emit_gated_op(
    emitter: &mut Emitter,
    label: &str,
    no_buffer_msg: (&str, usize),
    gated_msg: (&str, usize),
    required_flag: i64,
    phase: i64,
    write: bool,
    pop: bool,
) {
    let fail = format!("{label}_fail");
    let gated = format!("{label}_gated");
    let done = format!("{label}_done");

    emitter.blank();
    emitter.comment(&format!(
        "--- runtime: {} ---",
        label.trim_start_matches("__rt_")
    ));
    emitter.label_global(label);
    emitter.instruction("sub sp, sp, #32");                                     // allocate the gated-op frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish the gated-op frame pointer
    abi::emit_symbol_address(emitter, "x9", "_ob_level");                       // materialize the address of the buffer-stack depth
    emitter.instruction("ldr x10, [x9]");                                       // load the current buffer-stack depth
    emitter.instruction(&format!("cbz x10, {fail}"));                           // no active buffer — refuse
    emitter.instruction("sub x10, x10, #1");                                    // top slot index = depth - 1
    emitter.instruction("str x10, [sp, #0]");                                   // save the top slot index
    abi::emit_symbol_address(emitter, "x11", "_ob_flags");                      // materialize the flags slot array
    emitter.instruction("ldr x12, [x11, x10, lsl #3]");                         // load the slot's flags word
    emitter.instruction(&format!("tst x12, #{}", required_flag));               // does the buffer allow this operation?
    emitter.instruction(&format!("b.eq {gated}"));                              // flag missing — refuse with the gated notice
    emitter.instruction("mov x0, x10");                                         // process the top slot
    emitter.instruction(&format!("mov x1, #{}", phase));                        // this operation's handler phase bits
    emitter.instruction(&format!("mov x2, #{}", i64::from(write)));             // write flag: flush paths emit to the parent sink
    emitter.instruction("bl __rt_ob_process_and_write");                        // run the handler and clean/flush the buffer
    if pop {
        emitter.instruction("bl __rt_ob_pop_free");                             // pop and free the processed buffer
    }
    emitter.instruction("mov x0, #1");                                          // report success
    emitter.instruction(&format!("b {done}"));                                  // finish
    emitter.label(&fail);
    abi::emit_symbol_address(emitter, "x0", no_buffer_msg.0);                   // load the no-buffer notice line
    emitter.instruction(&format!("mov x1, #{}", no_buffer_msg.1));              // notice byte length
    emitter.instruction("bl __rt_stdout_write");                                // write the notice through the funnel
    emitter.instruction("mov x0, #0");                                          // report failure
    emitter.instruction(&format!("b {done}"));                                  // finish
    emitter.label(&gated);
    abi::emit_symbol_address(emitter, "x0", gated_msg.0);                       // load the flags-gated notice prefix
    emitter.instruction(&format!("mov x1, #{}", gated_msg.1));                  // notice prefix byte length
    emitter.instruction("ldr x2, [sp, #0]");                                    // reload the top slot index for the notice
    emitter.instruction("bl __rt_ob_notice_named");                             // write "<prefix>NAME (LEVEL)"
    emitter.instruction("mov x0, #0");                                          // report failure
    emitter.label(&done);
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the gated-op frame
    emitter.instruction("ret");                                                 // return the success flag
}

/// Emits the Linux x86_64 variant of one flags-gated public ob_* mutation.
#[allow(clippy::too_many_arguments)]
fn emit_gated_op_x86_64(
    emitter: &mut Emitter,
    label: &str,
    no_buffer_msg: (&str, usize),
    gated_msg: (&str, usize),
    required_flag: i64,
    phase: i64,
    write: bool,
    pop: bool,
) {
    let fail = format!("{label}_fail_x86");
    let gated = format!("{label}_gated_x86");
    let done = format!("{label}_done_x86");

    emitter.blank();
    emitter.comment(&format!(
        "--- runtime: {} ---",
        label.trim_start_matches("__rt_")
    ));
    emitter.label_global(label);
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the gated-op frame pointer
    emitter.instruction("sub rsp, 16");                                         // reserve an aligned slot for the top index
    abi::emit_symbol_address(emitter, "r9", "_ob_level");                       // materialize the address of the buffer-stack depth
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // load the current buffer-stack depth
    emitter.instruction("test r10, r10");                                       // is any buffer active?
    emitter.instruction(&format!("jz {fail}"));                                 // no active buffer — refuse
    emitter.instruction("sub r10, 1");                                          // top slot index = depth - 1
    emitter.instruction("mov QWORD PTR [rbp - 8], r10");                        // save the top slot index
    abi::emit_symbol_address(emitter, "r11", "_ob_flags");                      // materialize the flags slot array
    emitter.instruction("mov rcx, QWORD PTR [r11 + r10*8]");                    // load the slot's flags word
    emitter.instruction(&format!("test rcx, {}", required_flag));               // does the buffer allow this operation?
    emitter.instruction(&format!("jz {gated}"));                                // flag missing — refuse with the gated notice
    emitter.instruction("mov rdi, r10");                                        // process the top slot
    emitter.instruction(&format!("mov esi, {}", phase));                        // this operation's handler phase bits
    emitter.instruction(&format!("mov edx, {}", i64::from(write)));             // write flag: flush paths emit to the parent sink
    emitter.instruction("call __rt_ob_process_and_write");                      // run the handler and clean/flush the buffer
    if pop {
        emitter.instruction("call __rt_ob_pop_free");                           // pop and free the processed buffer
    }
    emitter.instruction("mov eax, 1");                                          // report success
    emitter.instruction(&format!("jmp {done}"));                                // finish
    emitter.label(&fail);
    abi::emit_symbol_address(emitter, "rdi", no_buffer_msg.0);                  // load the no-buffer notice line
    emitter.instruction(&format!("mov esi, {}", no_buffer_msg.1));              // notice byte length
    emitter.instruction("call __rt_stdout_write");                              // write the notice through the funnel
    emitter.instruction("xor eax, eax");                                        // report failure
    emitter.instruction(&format!("jmp {done}"));                                // finish
    emitter.label(&gated);
    abi::emit_symbol_address(emitter, "rdi", gated_msg.0);                      // load the flags-gated notice prefix
    emitter.instruction(&format!("mov esi, {}", gated_msg.1));                  // notice prefix byte length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 8]");                        // reload the top slot index for the notice
    emitter.instruction("call __rt_ob_notice_named");                           // write "<prefix>NAME (LEVEL)"
    emitter.instruction("xor eax, eax");                                        // report failure
    emitter.label(&done);
    emitter.instruction("add rsp, 16");                                         // release the gated-op frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the success flag
}

/// Emits the four flags-gated bool-returning ob_* mutations: `__rt_ob_clean`,
/// `__rt_ob_end_clean`, `__rt_ob_flush`, and `__rt_ob_end_flush`, with PHP's
/// per-operation gating flags, handler phases, and notice texts.
pub fn emit_ob_gated_ops(emitter: &mut Emitter) {
    let ops: [(&str, (&str, usize), (&str, usize), i64, i64, bool, bool); 4] = [
        (
            "__rt_ob_clean",
            ("_ob_ntc_no_clean", OB_NTC_NO_CLEAN.len()),
            ("_ob_ntc_g_clean", OB_NTC_G_CLEAN.len()),
            OB_FLAG_CLEANABLE,
            OB_PHASE_CLEAN,
            false,
            false,
        ),
        (
            "__rt_ob_end_clean",
            ("_ob_ntc_no_end_clean", OB_NTC_NO_END_CLEAN.len()),
            ("_ob_ntc_g_end_clean", OB_NTC_G_END_CLEAN.len()),
            OB_FLAG_REMOVABLE,
            OB_PHASE_CLEAN | OB_PHASE_FINAL,
            false,
            true,
        ),
        (
            "__rt_ob_flush",
            ("_ob_ntc_no_flush", OB_NTC_NO_FLUSH.len()),
            ("_ob_ntc_g_flush", OB_NTC_G_FLUSH.len()),
            OB_FLAG_FLUSHABLE,
            OB_PHASE_FLUSH,
            true,
            false,
        ),
        (
            "__rt_ob_end_flush",
            ("_ob_ntc_no_end_flush", OB_NTC_NO_END_FLUSH.len()),
            ("_ob_ntc_g_end_flush", OB_NTC_G_END_FLUSH.len()),
            OB_FLAG_REMOVABLE,
            OB_PHASE_FINAL,
            true,
            true,
        ),
    ];
    for (label, no_buffer, gated, flag, phase, write, pop) in ops {
        if emitter.target.arch == Arch::X86_64 {
            emit_gated_op_x86_64(emitter, label, no_buffer, gated, flag, phase, write, pop);
        } else {
            emit_gated_op(emitter, label, no_buffer, gated, flag, phase, write, pop);
        }
    }
}

/// Emits one composite get-and-pop entry (AArch64): gate on REMOVABLE, persist
/// the raw contents, run the handler, optionally flush the survivors to the
/// parent, pop, and return the raw pair (null pointer on refusal).
fn emit_get_pop_op(
    emitter: &mut Emitter,
    label: &str,
    no_buffer_msg: Option<(&str, usize)>,
    gated_msg: (&str, usize),
    phase: i64,
    write: bool,
) {
    if emitter.target.arch == Arch::X86_64 {
        emit_get_pop_op_x86_64(emitter, label, no_buffer_msg, gated_msg, phase, write);
        return;
    }
    let fail = format!("{label}_fail");
    let gated = format!("{label}_gated");
    let none = format!("{label}_none");
    let done = format!("{label}_done");

    emitter.blank();
    emitter.comment(&format!(
        "--- runtime: {} ---",
        label.trim_start_matches("__rt_")
    ));
    emitter.label_global(label);
    // frame: [0]=slot, [8]=raw ptr, [16]=raw len
    emitter.instruction("sub sp, sp, #48");                                     // allocate the get-pop frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the get-pop frame pointer
    abi::emit_symbol_address(emitter, "x9", "_ob_level");                       // materialize the address of the buffer-stack depth
    emitter.instruction("ldr x10, [x9]");                                       // load the current buffer-stack depth
    emitter.instruction(&format!("cbz x10, {fail}"));                           // no active buffer — refuse
    emitter.instruction("sub x10, x10, #1");                                    // top slot index = depth - 1
    emitter.instruction("str x10, [sp, #0]");                                   // save the top slot index
    abi::emit_symbol_address(emitter, "x11", "_ob_flags");                      // materialize the flags slot array
    emitter.instruction("ldr x12, [x11, x10, lsl #3]");                         // load the slot's flags word
    emitter.instruction(&format!("tst x12, #{}", OB_FLAG_REMOVABLE));           // may this buffer be removed?
    emitter.instruction(&format!("b.eq {gated}"));                              // not removable — refuse with the gated notice
    // -- persist the raw contents before the handler runs --
    abi::emit_symbol_address(emitter, "x11", "_ob_ptrs");                       // materialize the buffer-pointer slot array
    emitter.instruction("ldr x1, [x11, x10, lsl #3]");                          // persist source pointer = the raw buffer base
    abi::emit_symbol_address(emitter, "x11", "_ob_lens");                       // materialize the used-bytes slot array
    emitter.instruction("ldr x2, [x11, x10, lsl #3]");                          // persist source length = the raw byte count
    emitter.instruction("bl __rt_str_persist");                                 // copy the raw contents into an owned heap string
    emitter.instruction("stp x1, x2, [sp, #8]");                                // save the raw contents pair
    // -- run the handler (flush paths emit the survivors), then pop --
    emitter.instruction("ldr x0, [sp, #0]");                                    // process the saved top slot
    emitter.instruction(&format!("mov x1, #{}", phase));                        // this operation's handler phase bits
    emitter.instruction(&format!("mov x2, #{}", i64::from(write)));             // write flag: get_flush emits to the parent sink
    emitter.instruction("bl __rt_ob_process_and_write");                        // run the handler and clean/flush the buffer
    emitter.instruction("bl __rt_ob_pop_free");                                 // pop and free the processed buffer
    emitter.instruction("ldp x1, x2, [sp, #8]");                                // return the raw contents pair
    emitter.instruction(&format!("b {done}"));                                  // finish
    emitter.label(&fail);
    if let Some((msg_sym, msg_len)) = no_buffer_msg {
        abi::emit_symbol_address(emitter, "x0", msg_sym);                       // load the no-buffer notice line
        emitter.instruction(&format!("mov x1, #{}", msg_len));                  // notice byte length
        emitter.instruction("bl __rt_stdout_write");                            // write the notice through the funnel
    }
    emitter.instruction(&format!("b {none}"));                                  // return the null failure pair
    emitter.label(&gated);
    abi::emit_symbol_address(emitter, "x0", gated_msg.0);                       // load the flags-gated notice prefix
    emitter.instruction(&format!("mov x1, #{}", gated_msg.1));                  // notice prefix byte length
    emitter.instruction("ldr x2, [sp, #0]");                                    // reload the top slot index for the notice
    emitter.instruction("bl __rt_ob_notice_named");                             // write "<prefix>NAME (LEVEL)"
    emitter.label(&none);
    emitter.instruction("mov x1, #0");                                          // null pointer signals refusal
    emitter.instruction("mov x2, #0");                                          // zero length for the failure pair
    emitter.label(&done);
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the get-pop frame
    emitter.instruction("ret");                                                 // return the raw contents pair
}

/// Emits the Linux x86_64 variant of one composite get-and-pop entry.
fn emit_get_pop_op_x86_64(
    emitter: &mut Emitter,
    label: &str,
    no_buffer_msg: Option<(&str, usize)>,
    gated_msg: (&str, usize),
    phase: i64,
    write: bool,
) {
    let fail = format!("{label}_fail_x86");
    let gated = format!("{label}_gated_x86");
    let none = format!("{label}_none_x86");
    let done = format!("{label}_done_x86");

    emitter.blank();
    emitter.comment(&format!(
        "--- runtime: {} ---",
        label.trim_start_matches("__rt_")
    ));
    emitter.label_global(label);
    // frame: [rbp-8]=slot, [rbp-16]=raw ptr, [rbp-24]=raw len
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the get-pop frame pointer
    emitter.instruction("sub rsp, 32");                                         // reserve the get-pop local slots (16-aligned)
    abi::emit_symbol_address(emitter, "r9", "_ob_level");                       // materialize the address of the buffer-stack depth
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // load the current buffer-stack depth
    emitter.instruction("test r10, r10");                                       // is any buffer active?
    emitter.instruction(&format!("jz {fail}"));                                 // no active buffer — refuse
    emitter.instruction("sub r10, 1");                                          // top slot index = depth - 1
    emitter.instruction("mov QWORD PTR [rbp - 8], r10");                        // save the top slot index
    abi::emit_symbol_address(emitter, "r11", "_ob_flags");                      // materialize the flags slot array
    emitter.instruction("mov rcx, QWORD PTR [r11 + r10*8]");                    // load the slot's flags word
    emitter.instruction(&format!("test rcx, {}", OB_FLAG_REMOVABLE));           // may this buffer be removed?
    emitter.instruction(&format!("jz {gated}"));                                // not removable — refuse with the gated notice
    // -- persist the raw contents before the handler runs --
    abi::emit_symbol_address(emitter, "r11", "_ob_ptrs");                       // materialize the buffer-pointer slot array
    emitter.instruction("mov rax, QWORD PTR [r11 + r10*8]");                    // persist source pointer = the raw buffer base
    abi::emit_symbol_address(emitter, "r11", "_ob_lens");                       // materialize the used-bytes slot array
    emitter.instruction("mov rdx, QWORD PTR [r11 + r10*8]");                    // persist source length = the raw byte count
    emitter.instruction("call __rt_str_persist");                               // copy the raw contents into an owned heap string
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the raw contents pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the raw contents length
    // -- run the handler (flush paths emit the survivors), then pop --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // process the saved top slot
    emitter.instruction(&format!("mov esi, {}", phase));                        // this operation's handler phase bits
    emitter.instruction(&format!("mov edx, {}", i64::from(write)));             // write flag: get_flush emits to the parent sink
    emitter.instruction("call __rt_ob_process_and_write");                      // run the handler and clean/flush the buffer
    emitter.instruction("call __rt_ob_pop_free");                               // pop and free the processed buffer
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return the raw contents pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // return the raw contents length
    emitter.instruction(&format!("jmp {done}"));                                // finish
    emitter.label(&fail);
    if let Some((msg_sym, msg_len)) = no_buffer_msg {
        abi::emit_symbol_address(emitter, "rdi", msg_sym);                      // load the no-buffer notice line
        emitter.instruction(&format!("mov esi, {}", msg_len));                  // notice byte length
        emitter.instruction("call __rt_stdout_write");                          // write the notice through the funnel
    }
    emitter.instruction(&format!("jmp {none}"));                                // return the null failure pair
    emitter.label(&gated);
    abi::emit_symbol_address(emitter, "rdi", gated_msg.0);                      // load the flags-gated notice prefix
    emitter.instruction(&format!("mov esi, {}", gated_msg.1));                  // notice prefix byte length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 8]");                        // reload the top slot index for the notice
    emitter.instruction("call __rt_ob_notice_named");                           // write "<prefix>NAME (LEVEL)"
    emitter.label(&none);
    emitter.instruction("xor eax, eax");                                        // null pointer signals refusal
    emitter.instruction("xor edx, edx");                                        // zero length for the failure pair
    emitter.label(&done);
    emitter.instruction("add rsp, 32");                                         // release the get-pop local slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the raw contents pair
}

/// Emits `__rt_ob_get_clean_pop` and `__rt_ob_get_flush_pop`: the composite
/// helpers behind `ob_get_clean()` (silent on no buffer) and `ob_get_flush()`
/// (PHP notice on no buffer), both gated on REMOVABLE.
pub fn emit_ob_get_pop_ops(emitter: &mut Emitter) {
    emit_get_pop_op(
        emitter,
        "__rt_ob_get_clean_pop",
        None,
        ("_ob_ntc_g_get_clean", OB_NTC_G_GET_CLEAN.len()),
        OB_PHASE_CLEAN | OB_PHASE_FINAL,
        false,
    );
    emit_get_pop_op(
        emitter,
        "__rt_ob_get_flush_pop",
        Some(("_ob_ntc_no_get_flush", OB_NTC_NO_GET_FLUSH.len())),
        ("_ob_ntc_g_get_flush", OB_NTC_G_GET_FLUSH.len()),
        OB_PHASE_FINAL,
        true,
    );
}

/// Emits `__rt_ob_flush_all`: drain every still-active buffer at process exit.
///
/// No inputs, no result. Drains top-down so each handler (FINAL phase) sees its
/// own buffer and its output folds into the parent, matching PHP's shutdown
/// order. Guarded by `_ob_flushing` against handler-triggered re-entry (e.g. a
/// handler calling `exit()`); never frees storage because the process is
/// exiting. Gating flags are ignored: PHP force-flushes at shutdown.
pub fn emit_ob_flush_all(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_ob_flush_all_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: ob_flush_all (process-exit drain) ---");
    emitter.label_global("__rt_ob_flush_all");
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the flush_all frame pointer
    abi::emit_symbol_address(emitter, "x9", "_ob_flushing");                    // materialize the re-entry guard address
    emitter.instruction("ldr x10, [x9]");                                       // load the re-entry guard
    emitter.instruction("cbnz x10, __rt_ob_flush_all_done");                    // already draining — nested exits are a no-op
    emitter.instruction("mov x10, #1");                                         // mark the drain as active
    emitter.instruction("str x10, [x9]");                                       // publish the re-entry guard
    emitter.label("__rt_ob_flush_all_loop");
    abi::emit_symbol_address(emitter, "x9", "_ob_level");                       // materialize the address of the buffer-stack depth
    emitter.instruction("ldr x10, [x9]");                                       // load the current buffer-stack depth
    emitter.instruction("cbz x10, __rt_ob_flush_all_done");                     // stack drained — done
    emitter.instruction("sub x0, x10, #1");                                     // drain the top slot next
    emitter.instruction(&format!("mov x1, #{}", OB_PHASE_FINAL));               // shutdown handler phase = FINAL
    emitter.instruction("mov x2, #1");                                          // emit the surviving bytes to the parent sink
    emitter.instruction("bl __rt_ob_process_and_write");                        // run the handler and flush the buffer
    abi::emit_symbol_address(emitter, "x9", "_ob_level");                       // materialize the address of the buffer-stack depth
    emitter.instruction("ldr x10, [x9]");                                       // reload the depth (handlers may have changed it)
    emitter.instruction("cbz x10, __rt_ob_flush_all_done");                     // stack drained — done
    emitter.instruction("sub x10, x10, #1");                                    // pop the drained slot (no frees at exit)
    emitter.instruction("str x10, [x9]");                                       // publish the shrunken depth
    emitter.instruction("b __rt_ob_flush_all_loop");                            // continue draining
    emitter.label("__rt_ob_flush_all_done");
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore frame pointer and return address
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 variant of `__rt_ob_flush_all`.
fn emit_ob_flush_all_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ob_flush_all (process-exit drain) ---");
    emitter.label_global("__rt_ob_flush_all");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the flush_all frame pointer
    abi::emit_symbol_address(emitter, "r9", "_ob_flushing");                    // materialize the re-entry guard address
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // load the re-entry guard
    emitter.instruction("test r10, r10");                                       // is a drain already running?
    emitter.instruction("jnz __rt_ob_flush_all_done_x86");                      // already draining — nested exits are a no-op
    emitter.instruction("mov QWORD PTR [r9], 1");                               // publish the re-entry guard
    emitter.label("__rt_ob_flush_all_loop_x86");
    abi::emit_symbol_address(emitter, "r9", "_ob_level");                       // materialize the address of the buffer-stack depth
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // load the current buffer-stack depth
    emitter.instruction("test r10, r10");                                       // is anything still buffered?
    emitter.instruction("jz __rt_ob_flush_all_done_x86");                       // stack drained — done
    emitter.instruction("lea rdi, [r10 - 1]");                                  // drain the top slot next
    emitter.instruction(&format!("mov esi, {}", OB_PHASE_FINAL));               // shutdown handler phase = FINAL
    emitter.instruction("mov edx, 1");                                          // emit the surviving bytes to the parent sink
    emitter.instruction("call __rt_ob_process_and_write");                      // run the handler and flush the buffer
    abi::emit_symbol_address(emitter, "r9", "_ob_level");                       // materialize the address of the buffer-stack depth
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // reload the depth (handlers may have changed it)
    emitter.instruction("test r10, r10");                                       // is anything left to pop?
    emitter.instruction("jz __rt_ob_flush_all_done_x86");                       // stack drained — done
    emitter.instruction("sub r10, 1");                                          // pop the drained slot (no frees at exit)
    emitter.instruction("mov QWORD PTR [r9], r10");                             // publish the shrunken depth
    emitter.instruction("jmp __rt_ob_flush_all_loop_x86");                      // continue draining
    emitter.label("__rt_ob_flush_all_done_x86");
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Arch, Platform, Target};

    /// Renders every ob_* buffer helper for one target.
    fn render(platform: Platform, arch: Arch) -> String {
        let mut emitter = Emitter::new(Target::new(platform, arch));
        emit_ob_start(&mut emitter);
        emit_ob_process_and_write(&mut emitter);
        emit_ob_pop_free(&mut emitter);
        emit_ob_append(&mut emitter);
        emit_ob_contents(&mut emitter);
        emit_ob_queries(&mut emitter);
        emit_ob_gated_ops(&mut emitter);
        emit_ob_get_pop_ops(&mut emitter);
        emit_ob_flush_all(&mut emitter);
        emitter.output()
    }

    /// Verifies every target exports all ob_* buffer helper labels.
    #[test]
    fn emits_global_labels_for_all_targets() {
        for (platform, arch) in [
            (Platform::MacOS, Arch::AArch64),
            (Platform::Linux, Arch::AArch64),
            (Platform::Linux, Arch::X86_64),
        ] {
            let asm = render(platform, arch);
            for label in [
                "__rt_ob_start_ex",
                "__rt_ob_start",
                "__rt_ob_process_and_write",
                "__rt_ob_pop_free",
                "__rt_ob_append",
                "__rt_ob_contents",
                "__rt_ob_length",
                "__rt_ob_level",
                "__rt_ob_clean",
                "__rt_ob_end_clean",
                "__rt_ob_flush",
                "__rt_ob_end_flush",
                "__rt_ob_get_clean_pop",
                "__rt_ob_get_flush_pop",
                "__rt_ob_flush_all",
            ] {
                assert!(
                    asm.contains(&format!(".globl {label}\n")),
                    "missing {label} for {:?}/{:?}",
                    platform,
                    arch
                );
            }
        }
    }

    /// Verifies the growth path allocates a replacement block and frees the old one.
    #[test]
    fn append_grows_through_heap_alloc_and_free() {
        let mac = render(Platform::MacOS, Arch::AArch64);
        assert!(mac.contains("bl __rt_heap_alloc"));
        assert!(mac.contains("bl __rt_heap_free"));
        let linux_x86 = render(Platform::Linux, Arch::X86_64);
        assert!(linux_x86.contains("call __rt_heap_alloc"));
        assert!(linux_x86.contains("call __rt_heap_free"));
    }

    /// Verifies flush paths route buffered bytes back through the stdout funnel
    /// and run the user handler first.
    #[test]
    fn flush_paths_run_handler_and_route_through_stdout_write() {
        let mac = render(Platform::MacOS, Arch::AArch64);
        assert!(mac.contains("bl __rt_stdout_write"));
        assert!(mac.contains("bl __rt_ob_apply_handler"));
        let linux_x86 = render(Platform::Linux, Arch::X86_64);
        assert!(linux_x86.contains("call __rt_stdout_write"));
        assert!(linux_x86.contains("call __rt_ob_apply_handler"));
    }

    /// Verifies gated refusals reference the PHP notice constants and the
    /// named-notice writer.
    #[test]
    fn gated_ops_reference_notice_symbols() {
        for (platform, arch) in [
            (Platform::MacOS, Arch::AArch64),
            (Platform::Linux, Arch::X86_64),
        ] {
            let asm = render(platform, arch);
            for sym in [
                "_ob_ntc_no_clean",
                "_ob_ntc_no_end_flush",
                "_ob_ntc_g_end_clean",
                "_ob_ntc_g_get_flush",
                "__rt_ob_notice_named",
            ] {
                assert!(
                    asm.contains(sym),
                    "missing {sym} for {:?}/{:?}",
                    platform,
                    arch
                );
            }
        }
    }

    /// Verifies ob_get_contents persists the buffer through `__rt_str_persist`.
    #[test]
    fn contents_persists_the_buffer() {
        let mac = render(Platform::MacOS, Arch::AArch64);
        assert!(mac.contains("bl __rt_str_persist"));
        let linux_x86 = render(Platform::Linux, Arch::X86_64);
        assert!(linux_x86.contains("call __rt_str_persist"));
    }

    /// Verifies chunked buffers auto-flush through the shared process core.
    #[test]
    fn append_chunk_threshold_triggers_process_and_write() {
        let mac = render(Platform::MacOS, Arch::AArch64);
        assert!(mac.contains("_ob_chunk_sizes"));
        assert!(mac.contains("bl __rt_ob_process_and_write"));
        let linux_x86 = render(Platform::Linux, Arch::X86_64);
        assert!(linux_x86.contains("_ob_chunk_sizes"));
        assert!(linux_x86.contains("call __rt_ob_process_and_write"));
    }
}
